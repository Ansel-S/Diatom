'use strict';

/**
 * agent.js — Diatom micro-agent JS bridge.
 *
 * Listens for AgentEvent::ToolCall from the Rust runner via Tauri events,
 * executes browser actions in the active tab, and reports results back via
 * cmd_agent_tool_result. Renders a non-blocking HUD showing plan progress.
 */

const invoke = window.__TAURI__?.invoke
  ?? (() => Promise.reject(new Error('Tauri IPC not available')));

const listen = window.__TAURI__?.event?.listen;

// ── Constants ─────────────────────────────────────────────────────────────────

/** CSS selector for the agent HUD overlay element. */
const HUD_ID = 'diatom-agent-hud';

/** Max characters of DOM summary sent to the model. */
const DOM_SUMMARY_MAX_CHARS = 1_500;

/** Screenshot JPEG quality (0–1). Lower = fewer tokens when base64-encoded. */
const SCREENSHOT_QUALITY = 0.6;

// ── Tauri event listener ──────────────────────────────────────────────────────

let _unlisten = null;

/**
 * Initialise the agent bridge.  Call once at browser-UI startup.
 * Safe to call multiple times — re-registers without duplication.
 */
export async function initAgentBridge() {
  if (_unlisten) { await _unlisten(); _unlisten = null; }

  if (!listen) {
    console.warn('[agent] Tauri event listener not available');
    return;
  }

  _unlisten = await listen('agent-event', ({ payload }) => {
    handleAgentEvent(payload).catch(err =>
      console.error('[agent] handleAgentEvent error:', err)
    );
  });

  // Expose the optional page-script API.
  window.diatom_action = (call) => executeToolCall(call);
}

// ── Event dispatcher ──────────────────────────────────────────────────────────

async function handleAgentEvent(event) {
  switch (event.type) {
    case 'plan_ready':
      hudShow(event.plan_id);
      hudSetSteps(event.steps);
      break;

    case 'tool_call':
      hudMarkStepActive(event.step_idx);
      await executeAndReport(event);
      break;

    case 'step_done':
      hudMarkStepDone(event.step_idx, event.output);
      break;

    case 'done':
      hudFinish(true, event.summary);
      break;

    case 'failed':
      hudFinish(false, event.reason);
      break;

    case 'step_timeout':
      hudMarkStepError(event.step_idx, 'timed out');
      break;

    case 'cancelled':
      hudFinish(false, 'Cancelled');
      break;
  }
}

// ── Tool execution ────────────────────────────────────────────────────────────

/**
 * Execute a single tool call and report the result to Rust.
 * @param {{ plan_id: number, step_idx: number, call: object }} event
 */
async function executeAndReport(event) {
  let result;
  try {
    result = await executeToolCall(event.call);
  } catch (err) {
    result = { ok: false, output: String(err), image_b64: null };
  }
  await invoke('cmd_agent_tool_result', {
    planId:   event.plan_id,
    ok:       result.ok,
    output:   result.output,
    imageb64: result.image_b64 ?? null,
  });
}

/**
 * Dispatch a tool call object to the appropriate DOM handler.
 * @param {object} call  - Validated ToolCall from the Rust schema.
 * @returns {Promise<{ ok: boolean, output: string, image_b64?: string }>}
 */
export async function executeToolCall(call) {
  switch (call.action) {
    case 'click':      return actionClick(call.target);
    case 'type':       return actionType(call.target, call.text);
    case 'navigate':   return actionNavigate(call.url);
    case 'wait_ms':    return actionWaitMs(call.ms);
    case 'read_page':  return actionReadPage();
    case 'screenshot': return actionScreenshot(call.x, call.y, call.w, call.h);
    case 'scroll':     return actionScroll(call.direction, call.px);
    default:
      return { ok: false, output: `Unknown action: ${call.action}` };
  }
}

// ── Individual actions ────────────────────────────────────────────────────────

/**
 * Click the first element matching `target`.
 * Accepts CSS selectors.  Falls back to text-content scan when selector
 * yields no results (handles cases where the model emits a label like
 * "Submit" instead of "#submit-btn").
 */
async function actionClick(target) {
  let el = querySelector(target);
  if (!el) el = findByText(target);
  if (!el) return { ok: false, output: `No element found for target: ${target}` };

  el.scrollIntoView({ block: 'center', behavior: 'instant' });

  // Prefer a real pointer event so event listeners fire correctly.
  el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
  el.dispatchEvent(new MouseEvent('mouseup',   { bubbles: true }));
  el.click();

  return { ok: true, output: `Clicked: ${describeEl(el)}` };
}

/** Type `text` into the first matching input / textarea / contenteditable. */
async function actionType(target, text) {
  const el = querySelector(target) ?? findByText(target);
  if (!el) return { ok: false, output: `No input found for target: ${target}` };

  const safe = String(text).replace(/\0/g, '');   // strip null bytes

  if (el.isContentEditable) {
    el.focus();
    el.textContent = safe;
    el.dispatchEvent(new Event('input', { bubbles: true }));
  } else {
    el.focus();
    // Use the native setter to work with React's synthetic event system.
    const nativeInputValueSetter = Object.getOwnPropertyDescriptor(
      window.HTMLInputElement.prototype, 'value'
    )?.set;
    if (nativeInputValueSetter) {
      nativeInputValueSetter.call(el, safe);
    } else {
      el.value = safe;
    }
    el.dispatchEvent(new Event('input',  { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  }

  return { ok: true, output: `Typed ${safe.length} chars into: ${describeEl(el)}` };
}

/** Navigate the current tab to an absolute URL. */
async function actionNavigate(url) {
  if (!/^https?:\/\//i.test(url)) {
    return { ok: false, output: `Rejected non-http URL: ${url}` };
  }
  try {
    await invoke('cmd_preprocess_url', { url });   // let Diatom validate/clean it
    window.location.href = url;
    return { ok: true, output: `Navigating to ${url}` };
  } catch (err) {
    return { ok: false, output: `Navigate failed: ${err}` };
  }
}

/** Wait for `ms` milliseconds. */
async function actionWaitMs(ms) {
  const clamped = Math.min(Number(ms) || 500, 30_000);
  await new Promise(r => setTimeout(r, clamped));
  return { ok: true, output: `Waited ${clamped} ms` };
}

/**
 * Summarise the interactive DOM elements on the current page.
 *
 * Returns a compact text block:
 *   button#submit "Submit payment"
 *   input[name=email] placeholder="your@email.com"
 *   a[href="/logout"] "Log out"
 *   …
 *
 * This is the model's "eyes" for pages it hasn't navigated yet.
 * Truncated at DOM_SUMMARY_MAX_CHARS.
 */
async function actionReadPage() {
  const lines = [];

  // Interactive elements
  const SELECTORS = 'button,input,select,textarea,a[href],[role=button],[role=link],[role=checkbox],[role=menuitem],label';
  document.querySelectorAll(SELECTORS).forEach(el => {
    if (!isVisible(el)) return;

    const tag  = el.tagName.toLowerCase();
    const id   = el.id ? `#${el.id}` : '';
    const name = el.getAttribute('name') ? `[name=${el.getAttribute('name')}]` : '';
    const ph   = el.getAttribute('placeholder') ? ` placeholder="${el.getAttribute('placeholder')}"` : '';
    const text = (el.textContent || el.value || el.getAttribute('aria-label') || '').trim().slice(0, 60);
    const href = el.href ? ` href="${el.href}"` : '';

    lines.push(`${tag}${id}${name}${href}${ph}${text ? ` "${text}"` : ''}`);
  });

  const summary = lines.join('\n').slice(0, DOM_SUMMARY_MAX_CHARS);
  return { ok: true, output: summary || '(no interactive elements found)' };
}

/**
 * Capture a viewport-relative crop and return it as base64 JPEG.
 *
 * Vision path — used when the model needs to "see" an area of the page
 * rather than parse the DOM.  Crops to the requested w×h rectangle.
 */
async function actionScreenshot(x, y, w, h) {
  try {
    // Ask Tauri for a full webview screenshot (PNG bytes → base64).
    const fullB64 = await invoke('cmd_tab_screenshot');

    // Decode, crop, re-encode at low quality.
    const img = await loadImageFromBase64(fullB64);
    const canvas = new OffscreenCanvas(w, h);
    canvas.getContext('2d').drawImage(img, x, y, w, h, 0, 0, w, h);
    const blob = await canvas.convertToBlob({ type: 'image/jpeg', quality: SCREENSHOT_QUALITY });
    const croppedB64 = await blobToBase64(blob);

    return {
      ok:        true,
      output:    `Screenshot ${w}×${h} at (${x},${y})`,
      image_b64: croppedB64,
    };
  } catch (err) {
    return { ok: false, output: `Screenshot failed: ${err}` };
  }
}

/** Scroll the document or active scroll container. */
async function actionScroll(direction, px) {
  const amount = Math.min(Number(px) || 300, 5_000);
  const opts   = { behavior: 'smooth' };
  switch (direction) {
    case 'down':  window.scrollBy({ top:  amount, ...opts }); break;
    case 'up':    window.scrollBy({ top: -amount, ...opts }); break;
    case 'right': window.scrollBy({ left:  amount, ...opts }); break;
    case 'left':  window.scrollBy({ left: -amount, ...opts }); break;
  }
  await new Promise(r => setTimeout(r, 400));  // let scroll settle
  return { ok: true, output: `Scrolled ${direction} ${amount}px` };
}

// ── DOM helpers ───────────────────────────────────────────────────────────────

function querySelector(selector) {
  try { return document.querySelector(selector); } catch { return null; }
}

/** Find a visible element whose text content contains `text` (case-insensitive). */
function findByText(text) {
  const lower = text.toLowerCase();
  for (const el of document.querySelectorAll('button,a,label,[role=button]')) {
    if (isVisible(el) && el.textContent.toLowerCase().includes(lower)) {
      return el;
    }
  }
  return null;
}

function isVisible(el) {
  const r = el.getBoundingClientRect();
  return r.width > 0 && r.height > 0 && r.top < window.innerHeight && r.bottom > 0;
}

function describeEl(el) {
  const tag = el.tagName.toLowerCase();
  const id  = el.id ? `#${el.id}` : '';
  const txt = (el.textContent || el.value || '').trim().slice(0, 40);
  return `${tag}${id}${txt ? ` "${txt}"` : ''}`;
}

// ── Image utilities ───────────────────────────────────────────────────────────

function loadImageFromBase64(b64) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload  = () => resolve(img);
    img.onerror = reject;
    img.src     = `data:image/png;base64,${b64}`;
  });
}

function blobToBase64(blob) {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload  = () => resolve(r.result.split(',')[1]);
    r.onerror = reject;
    r.readAsDataURL(blob);
  });
}

// ── Public context extractor (called from Rust via eval_js) ───────────────────

/**
 * Extract current page context for the Rust executor.
 * Called by the Tauri backend via:
 *   webview.eval("window.__diatom_page_ctx = window.extractPageContext()")
 *
 * @returns {{ url: string, title: string, dom_summary: string }}
 */
export function extractPageContext() {
  const lines = [];
  const SELECTORS = 'button,input,select,textarea,a[href],[role=button]';
  document.querySelectorAll(SELECTORS).forEach(el => {
    if (!isVisible(el)) return;
    const tag  = el.tagName.toLowerCase();
    const id   = el.id ? `#${el.id}` : '';
    const name = el.getAttribute('name') ? `[name=${el.getAttribute('name')}]` : '';
    const text = (el.textContent || el.value || '').trim().slice(0, 50);
    lines.push(`${tag}${id}${name}${text ? ` "${text}"` : ''}`);
  });

  return {
    url:         location.href,
    title:       document.title,
    dom_summary: lines.join('\n').slice(0, DOM_SUMMARY_MAX_CHARS),
  };
}

window.extractPageContext = extractPageContext;

// ── HUD overlay ───────────────────────────────────────────────────────────────
// A lightweight non-blocking overlay rendered inside the browser chrome,
// not inside the content page.  Shows the current plan and step progress.

let _hudEl   = null;
let _hudSteps = [];

function hudEnsure() {
  if (_hudEl) return _hudEl;

  _hudEl = document.createElement('div');
  _hudEl.id = HUD_ID;
  _hudEl.style.cssText = `
    position: fixed;
    bottom: 12px;
    right: 12px;
    z-index: 2147483647;
    width: 300px;
    max-height: 360px;
    overflow-y: auto;
    background: rgba(14, 14, 22, 0.96);
    border: 1px solid rgba(100, 100, 220, 0.3);
    border-radius: 10px;
    padding: 12px 14px;
    font: 12px/1.5 "Inter", system-ui, sans-serif;
    color: #d0d0e0;
    box-shadow: 0 4px 24px rgba(0,0,0,0.5);
    backdrop-filter: blur(8px);
    transition: opacity 0.3s;
  `;

  // Header row
  const header = document.createElement('div');
  header.style.cssText = 'display:flex;justify-content:space-between;margin-bottom:8px;';
  header.innerHTML = `
    <span style="font-weight:600;color:#9090ff;">🤖 Agent</span>
    <button id="${HUD_ID}-close" style="background:none;border:none;color:#888;cursor:pointer;font-size:14px;">✕</button>
  `;
  _hudEl.appendChild(header);

  document.querySelector('body')?.appendChild(_hudEl);

  document.getElementById(`${HUD_ID}-close`)
    ?.addEventListener('click', () => hudHide());

  return _hudEl;
}

function hudShow(planId) {
  const el = hudEnsure();
  el.dataset.planId = planId;
  el.style.opacity = '1';
  el.style.display  = 'block';
}

function hudHide() {
  if (_hudEl) _hudEl.style.display = 'none';
}

function hudSetSteps(steps) {
  _hudSteps = steps;
  const el = hudEnsure();
  const list = el.querySelector('.agent-steps') ?? (() => {
    const d = document.createElement('div');
    d.className = 'agent-steps';
    el.appendChild(d);
    return d;
  })();

  list.innerHTML = steps.map((s, i) => `
    <div id="${HUD_ID}-step-${i}" style="
      display:flex; gap:8px; align-items:flex-start;
      padding:4px 0; border-bottom:1px solid rgba(255,255,255,0.05);
    ">
      <span class="step-icon" style="min-width:16px;text-align:center;">⬜</span>
      <span style="opacity:0.7;">${escHtml(s)}</span>
    </div>
  `).join('');
}

function hudMarkStepActive(idx) {
  const row = document.getElementById(`${HUD_ID}-step-${idx}`);
  if (!row) return;
  row.querySelector('.step-icon').textContent = '⏳';
  row.style.opacity = '1';
}

function hudMarkStepDone(idx, output) {
  const row = document.getElementById(`${HUD_ID}-step-${idx}`);
  if (!row) return;
  row.querySelector('.step-icon').textContent = '✅';
  row.title = output;
}

function hudMarkStepError(idx, reason) {
  const row = document.getElementById(`${HUD_ID}-step-${idx}`);
  if (!row) return;
  row.querySelector('.step-icon').textContent = '⚠️';
  row.title = reason;
}

function hudFinish(success, message) {
  const el = hudEnsure();
  const banner = document.createElement('div');
  banner.style.cssText = `
    margin-top: 10px;
    padding: 8px 10px;
    border-radius: 6px;
    font-weight: 600;
    background: ${success ? 'rgba(40,180,80,0.15)' : 'rgba(220,60,60,0.15)'};
    color: ${success ? '#60e080' : '#e06060'};
  `;
  banner.textContent = `${success ? '✔' : '✘'} ${message}`;
  el.appendChild(banner);

  // Auto-dismiss after 6 s on success.
  if (success) setTimeout(() => hudHide(), 6_000);
}

function escHtml(str) {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

// ── IPC additions (extend ipc.js exports) ────────────────────────────────────
// Call these from the AI panel or address bar to start / stop an agent run.

/**
 * Start a new agent run.
 * @param {string} goal     - Natural-language goal.
 * @param {string} [model]  - Optional model override (defaults to active model).
 * @returns {Promise<number>} planId
 */
export const agentStart = (goal, model = '') =>
  invoke('cmd_agent_start', { goal, model });

/**
 * Abort the currently running agent plan.
 * @param {number} planId
 */
export const agentAbort = (planId) =>
  invoke('cmd_agent_abort', { planId });
