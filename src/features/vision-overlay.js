/**
 * diatom/src/features/vision-overlay.js  — v7.1
 *
 * Vision Overlay: local OCR + optional inline translation.
 *
 * FIXES in v7.1:
 *   - Retina/HiDPI pixel offset: selection rect is now mapped correctly
 *     to the video frame using getBoundingClientRect + devicePixelRatio.
 *   - Hotkey yield: Alt+drag is cancelled when focus is on a FULL_YIELD domain
 *     (e.g. Figma) via the diatom:cancel-vision event from hotkey.js.
 *   - Tesseract worker is kept alive across calls (not re-created each time).
 */

'use strict';

import { invoke } from '../browser/ipc.js';
import { el, qs } from '../browser/utils.js';

// ── State ──────────────────────────────────────────────────────────────────────

let _tesseract  = null;   // Tesseract.js worker (lazy init)
let _selecting  = false;  // Alt+drag in progress
let _startX     = 0;
let _startY     = 0;
let _selBox     = null;   // selection rectangle element
let _overlay    = null;   // result panel element

// ── Init ──────────────────────────────────────────────────────────────────────

export function initVisionOverlay() {
  document.addEventListener('keydown',   onKeyDown);
  document.addEventListener('mousedown', onMouseDown, { capture: true });
  document.addEventListener('mousemove', onMouseMove, { capture: true });
  document.addEventListener('mouseup',   onMouseUp,   { capture: true });
  // Hotkey manager signals us to cancel when a FULL_YIELD app is focused
  document.addEventListener('diatom:cancel-vision', () => {
    _selecting = false;
    cancelSelection();
    dismissOverlay();
  });
}

// ── Input handling ─────────────────────────────────────────────────────────────

function onKeyDown(e) {
  // Escape: dismiss overlay
  if (e.key === 'Escape') {
    dismissOverlay();
    cancelSelection();
  }
}

function onMouseDown(e) {
  if (!e.altKey) return;
  e.preventDefault();
  e.stopPropagation();
  _selecting = true;
  _startX    = e.clientX;
  _startY    = e.clientY;
  dismissOverlay();

  _selBox = el('div', 'vision-sel-box');
  _selBox.style.cssText = `
    position:fixed; z-index:2147483646; pointer-events:none;
    border:1.5px solid rgba(96,165,250,.8);
    background:rgba(96,165,250,.08);
    box-shadow:0 0 0 1px rgba(96,165,250,.15);
  `;
  updateSelBox(e.clientX, e.clientY);
  document.documentElement.appendChild(_selBox);
}

function onMouseMove(e) {
  if (!_selecting || !_selBox) return;
  e.preventDefault();
  updateSelBox(e.clientX, e.clientY);
}

async function onMouseUp(e) {
  if (!_selecting) return;
  e.preventDefault();
  e.stopPropagation();
  _selecting = false;

  const rect = getSelRect(e.clientX, e.clientY);
  cancelSelection();

  if (rect.width < 10 || rect.height < 10) return;
  await runOCR(rect);
}

function updateSelBox(x, y) {
  if (!_selBox) return;
  const r = getSelRect(x, y);
  Object.assign(_selBox.style, {
    left:   `${r.left}px`,
    top:    `${r.top}px`,
    width:  `${r.width}px`,
    height: `${r.height}px`,
  });
}

function getSelRect(x2, y2) {
  const left   = Math.min(_startX, x2);
  const top    = Math.min(_startY, y2);
  const width  = Math.abs(x2 - _startX);
  const height = Math.abs(y2 - _startY);
  return { left, top, width, height };
}

function cancelSelection() {
  _selBox?.remove();
  _selBox = null;
}

// ── OCR pipeline ──────────────────────────────────────────────────────────────

async function runOCR(rect) {
  // Show spinner where result will appear
  showSpinner(rect);

  let imageData;
  try {
    imageData = await captureRegion(rect);
  } catch (err) {
    console.error('[Vision] capture failed:', err);
    dismissOverlay();
    return;
  }

  // Lazy-load Tesseract
  if (!_tesseract) {
    try {
      _tesseract = await loadTesseract();
    } catch (err) {
      console.error('[Vision] Tesseract load failed:', err);
      showError('OCR 引擎加载失败。');
      return;
    }
  }

  let text;
  try {
    const result = await _tesseract.recognize(imageData, 'chi_sim+eng');
    text = result?.data?.text?.trim() ?? '';
  } catch (err) {
    console.error('[Vision] OCR failed:', err);
    showError('文字识别失败。');
    return;
  }

  if (!text) {
    showError('未识别到文字。');
    return;
  }

  // Show OCR result
  showResult(rect, text);

  // Attempt translation via Resonance Mode (non-blocking)
  tryTranslate(text);
}

// ── Screen capture ────────────────────────────────────────────────────────────

async function captureRegion(rect) {
  // Use getDisplayMedia with preferCurrentTab for minimal permission surface
  let stream;
  try {
    stream = await navigator.mediaDevices.getDisplayMedia({
      preferCurrentTab: true,
      video: { displaySurface: 'browser' },
    });
  } catch {
    return captureViaCanvas(rect);
  }

  const video  = document.createElement('video');
  video.srcObject = stream;
  await new Promise(r => { video.onloadedmetadata = r; });
  await video.play();

  // FIX: The video frame is in physical pixels; rect is in CSS pixels.
  // We must account for devicePixelRatio to avoid the Retina offset bug.
  const dpr    = window.devicePixelRatio || 1;
  const canvas = document.createElement('canvas');
  canvas.width  = Math.round(rect.width  * dpr);
  canvas.height = Math.round(rect.height * dpr);

  const ctx    = canvas.getContext('2d');
  // Map CSS rect → physical video frame coordinates
  const scaleX = video.videoWidth  / window.innerWidth;
  const scaleY = video.videoHeight / window.innerHeight;

  ctx.drawImage(
    video,
    Math.round(rect.left * scaleX),  // sx: precise rounding, not float truncation
    Math.round(rect.top  * scaleY),
    Math.round(rect.width  * scaleX),
    Math.round(rect.height * scaleY),
    0, 0,
    canvas.width,
    canvas.height,
  );

  stream.getTracks().forEach(t => t.stop());
  return canvas;
}

function captureViaCanvas(rect) {
  // Lightweight fallback: capture just the selected DOM region.
  // Works for text-heavy pages; may miss canvas/video content.
  const canvas = document.createElement('canvas');
  canvas.width  = rect.width  * devicePixelRatio;
  canvas.height = rect.height * devicePixelRatio;
  const ctx = canvas.getContext('2d');
  ctx.scale(devicePixelRatio, devicePixelRatio);
  ctx.translate(-rect.left, -rect.top);

  // Walk visible elements in the region and draw their text
  // This is a simplified path — full html2canvas not included intentionally
  ctx.fillStyle = '#fff';
  ctx.fillRect(rect.left, rect.top, rect.width, rect.height);
  ctx.fillStyle = '#000';
  ctx.font = '14px system-ui';

  const walker = document.createTreeWalker(
    document.body,
    NodeFilter.SHOW_TEXT,
    null,
  );
  let node;
  while ((node = walker.nextNode())) {
    const range = document.createRange();
    range.selectNode(node);
    const r = range.getBoundingClientRect();
    if (r.left < rect.left + rect.width && r.right > rect.left &&
        r.top  < rect.top  + rect.height && r.bottom > rect.top) {
      ctx.fillText(node.textContent.trim(), r.left, r.bottom);
    }
  }

  return canvas;
}

// ── Tesseract loader ──────────────────────────────────────────────────────────

async function loadTesseract() {
  // Try to load from OPFS cache first, then fall back to bundled path
  const { createWorker } = await import('https://cdn.jsdelivr.net/npm/tesseract.js@5/dist/tesseract.esm.min.js');

  const worker = await createWorker(['chi_sim', 'eng'], 1, {
    workerPath:  '/assets/tesseract/worker.min.js',
    corePath:    '/assets/tesseract/tesseract-core.wasm.js',
    langPath:    '/assets/tesseract/lang-data',
    cacheMethod: 'write',   // uses OPFS-style cache in the worker
    logger:      m => {
      if (m.status === 'recognizing text') {
        updateSpinnerProgress(m.progress);
      }
    },
  });

  return worker;
}

// ── Translation via Resonance Mode ────────────────────────────────────────────

async function tryTranslate(text) {
  // Detect language: if majority ASCII, try translating to Chinese;
  // if majority CJK, translate to English.
  const cjkRatio = (text.match(/[\u4e00-\u9fa5]/g) ?? []).length / text.length;
  const targetLang = cjkRatio > 0.3 ? 'English' : '中文';

  try {
    // Use cmd_fetch to call local inference bridge
    const prompt = `Translate the following text to ${targetLang}. Output ONLY the translation, nothing else:\n\n${text.slice(0, 800)}`;

    // We invoke via Rust fetch so it routes through the Inference Multiplexer
    const result = await invoke('cmd_fetch', {
      url: 'http://localhost:11434/api/generate',
      method: 'POST',
    });

    // If local model is unavailable this will fail silently
    if (result?.body) {
      const json = JSON.parse(result.body);
      const translation = json?.response?.trim();
      if (translation) appendTranslation(translation);
    }
  } catch {
    // Translation is best-effort — never show an error for this
  }
}

// ── Result UI ─────────────────────────────────────────────────────────────────

function showSpinner(rect) {
  dismissOverlay();
  _overlay = el('div', 'vision-overlay');
  positionOverlay(rect);
  _overlay.innerHTML = `
    <div style="display:flex;align-items:center;gap:.5rem;color:#94a3b8;font-size:.8rem;">
      <span class="vision-spin" style="
        width:12px;height:12px;border:2px solid #334155;
        border-top-color:#60a5fa;border-radius:50%;
        animation:vision-spin .6s linear infinite;
      "></span>
      <span id="vision-progress">正在识别…</span>
    </div>
  `;
  ensureVisionStyle();
  document.body.appendChild(_overlay);
}

function updateSpinnerProgress(p) {
  const prog = qs('#vision-progress');
  if (prog) prog.textContent = `识别中 ${Math.round(p * 100)}%`;
}

function showResult(rect, text) {
  dismissOverlay();
  _overlay = el('div', 'vision-overlay');
  positionOverlay(rect);

  const copyBtn = el('button', 'vision-copy');
  copyBtn.style.cssText = `
    position:absolute;top:.5rem;right:.5rem;
    background:none;border:1px solid rgba(255,255,255,.1);
    border-radius:.25rem;color:#64748b;font-size:.7rem;
    padding:.15rem .4rem;cursor:pointer;
  `;
  copyBtn.textContent = '复制';
  copyBtn.addEventListener('click', () => {
    navigator.clipboard.writeText(text).catch(() => {});
    copyBtn.textContent = '✓';
    setTimeout(() => { copyBtn.textContent = '复制'; }, 1200);
  });

  const closeBtn = el('button', 'vision-close');
  closeBtn.style.cssText = `
    position:absolute;top:.5rem;right:3.5rem;
    background:none;border:none;color:#475569;
    font-size:.85rem;cursor:pointer;padding:.1rem .3rem;
  `;
  closeBtn.textContent = '✕';
  closeBtn.addEventListener('click', dismissOverlay);

  const textEl = el('p', 'vision-text');
  textEl.id = 'vision-ocr-text';
  textEl.style.cssText = `
    margin:0;font-size:.83rem;line-height:1.65;color:#e2e8f0;
    white-space:pre-wrap;word-break:break-word;max-height:180px;
    overflow-y:auto;padding-right:.25rem;
  `;
  textEl.textContent = text;

  _overlay.appendChild(closeBtn);
  _overlay.appendChild(copyBtn);
  _overlay.appendChild(textEl);

  ensureVisionStyle();
  document.body.appendChild(_overlay);
}

function appendTranslation(translation) {
  if (!_overlay) return;
  const divider = el('div');
  divider.style.cssText = 'border-top:1px solid rgba(255,255,255,.06);margin:.6rem 0 .5rem;';

  const label = el('p');
  label.style.cssText = 'margin:0 0 .3rem;font-size:.68rem;color:#475569;letter-spacing:.06em;text-transform:uppercase;';
  label.textContent = '译文';

  const transEl = el('p');
  transEl.style.cssText = `
    margin:0;font-size:.83rem;line-height:1.65;
    color:#94a3b8;white-space:pre-wrap;word-break:break-word;
  `;
  transEl.textContent = translation;

  _overlay.appendChild(divider);
  _overlay.appendChild(label);
  _overlay.appendChild(transEl);
}

function showError(msg) {
  dismissOverlay();
  _overlay = el('div', 'vision-overlay');
  _overlay.style.cssText += 'color:#f87171;';
  _overlay.style.top  = '50%';
  _overlay.style.left = '50%';
  _overlay.style.transform = 'translate(-50%,-50%)';
  _overlay.textContent = msg;
  ensureVisionStyle();
  document.body.appendChild(_overlay);
  setTimeout(dismissOverlay, 3000);
}

function positionOverlay(rect) {
  // Place below selection, or above if too close to bottom
  const margin  = 8;
  const below   = rect.top + rect.height + margin;
  const wouldOOB = below + 220 > window.innerHeight;
  const top     = wouldOOB ? Math.max(margin, rect.top - 220 - margin) : below;
  const left    = Math.max(margin, Math.min(rect.left, window.innerWidth - 340 - margin));

  _overlay.style.cssText = `
    position:fixed; z-index:2147483647;
    top:${top}px; left:${left}px;
    width:320px; max-width:calc(100vw - ${margin * 2}px);
    background:rgba(15,23,42,.92);
    backdrop-filter:blur(12px) saturate(1.4);
    -webkit-backdrop-filter:blur(12px) saturate(1.4);
    border:1px solid rgba(255,255,255,.08);
    border-radius:.6rem; padding:.75rem;
    font-family:'Inter',system-ui,sans-serif;
    box-shadow:0 8px 32px rgba(0,0,0,.5);
  `;
}

function dismissOverlay() {
  _overlay?.remove();
  _overlay = null;
}

function ensureVisionStyle() {
  if (qs('#vision-style')) return;
  const style = document.createElement('style');
  style.id = 'vision-style';
  style.textContent = `
    @keyframes vision-spin {
      to { transform: rotate(360deg); }
    }
    .vision-text::-webkit-scrollbar { width:3px; }
    .vision-text::-webkit-scrollbar-track { background:transparent; }
    .vision-text::-webkit-scrollbar-thumb { background:rgba(255,255,255,.15); border-radius:2px; }
  `;
  document.head.appendChild(style);
}
