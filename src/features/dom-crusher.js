/**
 * diatom/src/features/dom-crusher.js  — v7
 *
 * DOM Crusher: persistent per-domain element blocking.
 *
 * Usage:
 *   Ctrl+click any page element → it disappears and is remembered forever.
 *   The rule fires before first paint on every future load of that domain.
 *
 * Architecture:
 *   - Rules stored in SQLite via cmd_dom_crush (Rust validates selector)
 *   - On page load, cmd_dom_blocks_for(domain) is called and elements are
 *     removed via MutationObserver BEFORE the browser renders them
 *   - diatom-api.js calls applyCrusherRules() immediately on DOMContentLoaded
 *
 * Selector generation strategy (minimal & stable):
 *   1. If element has a unique id  →  #id
 *   2. If element has a unique data-* attr  →  tag[data-attr="value"]
 *   3. Otherwise  →  tag.class1.class2:nth-of-type(n) within parent
 */

'use strict';

import { invoke } from '../browser/ipc.js';
import { el, qs, domainOf } from '../browser/utils.js';

let _active      = false;        // Ctrl-click capture mode
let _overlay     = null;         // current highlight overlay element
let _lastTarget  = null;         // last hovered element
let _rules       = [];           // { id, selector } for current domain

// ── Init (called once per page load from diatom-api.js context) ──────────────

/**
 * Apply all stored crusher rules for the current domain.
 * Called as early as possible (before first paint ideally).
 *
 * @param {Array<{id:string, selector:string}>} rules
 */
export function applyCrusherRules(rules) {
  if (!rules?.length) return;
  _rules = rules;

  // Immediate pass — remove anything already in the DOM
  for (const rule of rules) {
    crushSelector(rule.selector);
  }

  // MutationObserver — catch dynamically inserted elements (sticky banners, etc.)
  const observer = new MutationObserver(() => {
    for (const rule of rules) {
      crushSelector(rule.selector);
    }
  });
  observer.observe(document.documentElement, { childList: true, subtree: true });
}

function crushSelector(selector) {
  try {
    document.querySelectorAll(selector).forEach(el => {
      el.style.setProperty('display', 'none', 'important');
      // Mark so MutationObserver skips re-checking
      el.setAttribute('data-diatom-crushed', '1');
    });
  } catch {
    // Invalid selector stored — ignore silently
  }
}

// ── Crusher capture mode ──────────────────────────────────────────────────────

/**
 * Activate Ctrl+click capture mode.
 * Call this when the user presses Ctrl (or when the page is loaded
 * and we want passive Ctrl-click interception).
 */
export function initCrusherCapture() {
  document.addEventListener('keydown',  onKeyDown,  { capture: true });
  document.addEventListener('keyup',    onKeyUp,    { capture: true });
  document.addEventListener('mouseover', onMouseOver);
  document.addEventListener('click',    onClick,    { capture: true });
}

function onKeyDown(e) {
  if (e.key === 'Control' && !e.repeat) {
    _active = true;
    document.body.style.cursor = 'crosshair';
  }
}

function onKeyUp(e) {
  if (e.key === 'Control') {
    _active = false;
    document.body.style.cursor = '';
    removeHighlight();
    _lastTarget = null;
  }
}

function onMouseOver(e) {
  if (!_active) return;
  const target = e.target;
  if (target === _lastTarget || target === _overlay) return;
  _lastTarget = target;
  showHighlight(target);
}

function onClick(e) {
  if (!_active) return;
  if (e.target === _overlay) return;
  e.preventDefault();
  e.stopPropagation();

  const target = e.target;
  removeHighlight();
  crushElement(target);
}

// ── Element crushing ──────────────────────────────────────────────────────────

async function crushElement(target) {
  const selector = generateSelector(target);
  if (!selector) return;

  // Optimistic: hide immediately
  target.style.setProperty('display', 'none', 'important');
  target.setAttribute('data-diatom-crushed', '1');

  // Persist to Rust backend
  const domain = domainOf(location.href);
  try {
    const id = await invoke('cmd_dom_crush', { domain, selector });
    _rules.push({ id, selector });

    // Brief confirmation flash in address bar area (ambient, non-modal)
    flashConfirmation(selector);
  } catch (err) {
    // Selector was rejected (dangerous) — undo
    console.warn('[DOM Crusher] selector rejected:', err.message);
    target.style.removeProperty('display');
    target.removeAttribute('data-diatom-crushed');
    showRejectionHint(err.message);
  }
}

// ── Selector generation ────────────────────────────────────────────────────────

function generateSelector(el) {
  if (!el || el === document.body || el === document.documentElement) return null;

  // 1. Unique ID
  if (el.id && /^[\w-]+$/.test(el.id) && document.querySelectorAll(`#${CSS.escape(el.id)}`).length === 1) {
    return `#${CSS.escape(el.id)}`;
  }

  // 2. Unique data-* attribute
  for (const attr of el.attributes) {
    if (attr.name.startsWith('data-') && attr.value && attr.value.length < 80) {
      const sel = `${el.tagName.toLowerCase()}[${attr.name}="${CSS.escape(attr.value)}"]`;
      if (document.querySelectorAll(sel).length === 1) return sel;
    }
  }

  // 3. tag + classes + nth-of-type within parent
  const tag     = el.tagName.toLowerCase();
  const classes = [...el.classList]
    .filter(c => /^[\w-]+$/.test(c) && !c.startsWith('_') && c.length < 40)
    .slice(0, 3)
    .map(c => `.${CSS.escape(c)}`)
    .join('');

  if (classes) {
    const candidate = `${tag}${classes}`;
    if (document.querySelectorAll(candidate).length === 1) return candidate;

    // Try with parent context
    const parent = el.parentElement;
    if (parent && parent !== document.body) {
      const parentSel = generateSelector(parent);
      if (parentSel) {
        const full = `${parentSel} > ${candidate}`;
        if (full.length < 200 && document.querySelectorAll(full).length === 1) return full;
      }
    }
  }

  // 4. nth-of-type fallback
  const siblings = [...(el.parentElement?.children ?? [])].filter(c => c.tagName === el.tagName);
  const idx      = siblings.indexOf(el) + 1;
  const nthSel   = `${tag}:nth-of-type(${idx})`;

  const parent = el.parentElement;
  if (parent && parent !== document.body) {
    const parentSel = generateSelector(parent);
    if (parentSel) {
      const full = `${parentSel} > ${nthSel}`;
      if (full.length < 200) return full;
    }
  }

  return nthSel;
}

// ── Visual feedback ────────────────────────────────────────────────────────────

function showHighlight(target) {
  removeHighlight();

  const rect = target.getBoundingClientRect();
  if (rect.width < 4 || rect.height < 4) return;

  _overlay = document.createElement('div');
  _overlay.style.cssText = `
    position:fixed;
    top:${rect.top}px; left:${rect.left}px;
    width:${rect.width}px; height:${rect.height}px;
    border:2px solid #ef4444;
    background:rgba(239,68,68,.08);
    pointer-events:none; z-index:2147483647;
    box-shadow:inset 0 0 0 1px rgba(239,68,68,.3);
    transition:all .08s ease;
  `;

  const label = document.createElement('span');
  label.textContent = '点击粉碎';
  label.style.cssText = `
    position:absolute; top:2px; left:4px;
    font:500 10px/1 'Inter',system-ui,sans-serif;
    color:#ef4444; letter-spacing:.04em; pointer-events:none;
  `;
  _overlay.appendChild(label);
  document.documentElement.appendChild(_overlay);
}

function removeHighlight() {
  _overlay?.remove();
  _overlay = null;
}

function flashConfirmation(selector) {
  const msg = document.createElement('div');
  msg.style.cssText = `
    position:fixed; bottom:1.5rem; left:50%; transform:translateX(-50%);
    background:rgba(15,23,42,.9); border:1px solid rgba(100,116,139,.3);
    color:#94a3b8; font:500 .75rem/1 'Inter',system-ui,sans-serif;
    padding:.5rem .9rem; border-radius:.35rem; z-index:2147483647;
    pointer-events:none;
    animation: fade-out-up 1.8s ease forwards;
  `;
  msg.textContent = `已永久屏蔽 · ${selector.slice(0, 48)}`;

  ensureFadeStyle();
  document.body.appendChild(msg);
  setTimeout(() => msg.remove(), 1900);
}

function showRejectionHint(reason) {
  const msg = document.createElement('div');
  msg.style.cssText = `
    position:fixed; bottom:1.5rem; left:50%; transform:translateX(-50%);
    background:rgba(127,29,29,.9); border:1px solid rgba(239,68,68,.3);
    color:#fca5a5; font:500 .75rem/1 'Inter',system-ui,sans-serif;
    padding:.5rem .9rem; border-radius:.35rem; z-index:2147483647;
    pointer-events:none;
    animation: fade-out-up 2.5s ease forwards;
  `;
  msg.textContent = `选择器被拒绝：${reason}`;
  ensureFadeStyle();
  document.body.appendChild(msg);
  setTimeout(() => msg.remove(), 2600);
}

function ensureFadeStyle() {
  if (qs('#diatom-fade-style')) return;
  const style = document.createElement('style');
  style.id = 'diatom-fade-style';
  style.textContent = `
    @keyframes fade-out-up {
      0%   { opacity:1; transform:translateX(-50%) translateY(0); }
      70%  { opacity:1; }
      100% { opacity:0; transform:translateX(-50%) translateY(-.75rem); }
    }
  `;
  document.head.appendChild(style);
}

// ── Rule management panel ─────────────────────────────────────────────────────

/**
 * Render a list of all crusher rules for the current domain.
 * Called from the settings panel or a context menu.
 */
export async function showCrusherRules() {
  const domain = domainOf(location.href);
  let rules;
  try {
    rules = await invoke('cmd_dom_blocks_for', { domain });
  } catch {
    return;
  }
  if (!rules.length) {
    alert(`${domain} 上暂无已屏蔽的元素。`);
    return;
  }

  const panel = el('div', 'crusher-panel');
  panel.style.cssText = `
    position:fixed; bottom:1rem; right:1rem; z-index:2147483646;
    width:320px; max-height:420px; overflow-y:auto;
    background:rgba(10,10,16,.96); border:1px solid rgba(255,255,255,.1);
    border-radius:.6rem; padding:1rem;
    font-family:'Inter',system-ui,sans-serif; font-size:.78rem; color:#94a3b8;
    box-shadow:0 8px 32px rgba(0,0,0,.6);
  `;
  panel.innerHTML = `
    <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:.75rem;">
      <span style="font-weight:600;color:#e8e8f0;">${domain} 的屏蔽规则</span>
      <button id="crusher-close" style="background:none;border:none;color:#64748b;cursor:pointer;font-size:1rem;">✕</button>
    </div>
  `;

  for (const rule of rules) {
    const row = el('div');
    row.style.cssText = 'display:flex;justify-content:space-between;align-items:center;padding:.35rem 0;border-bottom:1px solid rgba(255,255,255,.05);';
    row.innerHTML = `
      <code style="color:#60a5fa;font-size:.72rem;max-width:240px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escHtml(rule.selector)}</code>
      <button data-id="${rule.id}" class="crusher-del" style="background:none;border:none;color:#f87171;cursor:pointer;font-size:.85rem;padding:.1rem .3rem;">✕</button>
    `;
    panel.appendChild(row);
  }

  panel.querySelector('#crusher-close').addEventListener('click', () => panel.remove());
  panel.querySelectorAll('.crusher-del').forEach(btn => {
    btn.addEventListener('click', async () => {
      const id = btn.dataset.id;
      await invoke('cmd_dom_block_remove', { id });
      btn.closest('div').remove();
      // Remove rule from live list
      _rules = _rules.filter(r => r.id !== id);
    });
  });

  document.body.appendChild(panel);
}

function escHtml(s) {
  return String(s ?? '').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}
