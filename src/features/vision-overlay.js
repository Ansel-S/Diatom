
'use strict';

import { invoke } from '../browser/ipc.js';
import { el, qs } from '../browser/utils.js';

let _tesseract  = null;   // Tesseract.js worker (lazy init, reused)
let _selecting  = false;  // Alt+drag in progress
let _startX     = 0;
let _startY     = 0;
let _selBox     = null;   // selection rectangle element
let _overlay    = null;   // result panel element

export function initVisionOverlay() {
  document.addEventListener('keydown',   onKeyDown);
  document.addEventListener('mousedown', onMouseDown, { capture: true });
  document.addEventListener('mousemove', onMouseMove, { capture: true });
  document.addEventListener('mouseup',   onMouseUp,   { capture: true });
  document.addEventListener('diatom:cancel-vision', () => {
    _selecting = false;
    cancelSelection();
    dismissOverlay();
  });
}

function onKeyDown(e) {
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

  _selBox = el('div', 'vision-sel-box');
  _selBox.style.cssText = `
    position:fixed; border:1.5px dashed rgba(96,165,250,.8);
    background:rgba(96,165,250,.06); pointer-events:none; z-index:99998;
    left:${_startX}px; top:${_startY}px; width:0; height:0;
  `;
  document.body.appendChild(_selBox);
}

function onMouseMove(e) {
  if (!_selecting || !_selBox) return;
  const x = Math.min(e.clientX, _startX);
  const y = Math.min(e.clientY, _startY);
  const w = Math.abs(e.clientX - _startX);
  const h = Math.abs(e.clientY - _startY);
  _selBox.style.left   = x + 'px';
  _selBox.style.top    = y + 'px';
  _selBox.style.width  = w + 'px';
  _selBox.style.height = h + 'px';
}

function onMouseUp(e) {
  if (!_selecting) return;
  _selecting = false;
  if (_selBox) {
    _selBox.remove();
    _selBox = null;
  }

  const x = Math.min(e.clientX, _startX);
  const y = Math.min(e.clientY, _startY);
  const w = Math.abs(e.clientX - _startX);
  const h = Math.abs(e.clientY - _startY);

  if (w < 8 || h < 8) return;  // too small to be intentional

  const rect = { left: x, top: y, width: w, height: h };
  runOCR(rect);
}

async function runOCR(rect) {
  showSpinner(rect);

  let imageData;
  try {
    imageData = await captureRegion(rect);
  } catch (err) {
    console.error('[Vision] capture failed:', err);
    dismissOverlay();
    return;
  }

  if (!_tesseract) {
    try {
      _tesseract = await loadTesseract();
    } catch (err) {
      console.error('[Vision] Tesseract load failed:', err);
      showError('OCR engine failed to load.');
      return;
    }
  }

  let text;
  try {
    const result = await _tesseract.recognize(imageData, 'chi_sim+eng');
    text = result?.data?.text?.trim() ?? '';
  } catch (err) {
    console.error('[Vision] OCR failed:', err);
    showError('Text recognition failed.');
    return;
  }

  if (!text) { showError('No text detected.'); return; }

  showResult(rect, text);
  tryTranslate(text);
}

async function captureRegion(rect) {
  let stream;
  try {
    stream = await navigator.mediaDevices.getDisplayMedia({
      preferCurrentTab: true,
      video: { displaySurface: 'browser' },
    });

    const track     = stream.getVideoTracks()[0];
    const settings  = track?.getSettings?.() ?? {};
    if (settings.displaySurface && settings.displaySurface !== 'browser') {
      stream.getTracks().forEach(t => t.stop());
      throw new Error(
        `[Vision] Captured surface is "${settings.displaySurface}", ` +
        `not "browser". Aborting to enforce browser-only OCR boundary.`
      );
    }
  } catch (err) {
    if (err.message?.startsWith('[Vision]')) throw err;
    return captureViaCanvas(rect);
  }

  const video = document.createElement('video');
  video.srcObject = stream;
  await new Promise(r => { video.onloadedmetadata = r; });
  await video.play();

  const dpr    = window.devicePixelRatio || 1;
  const canvas = document.createElement('canvas');
  canvas.width  = Math.round(rect.width  * dpr);
  canvas.height = Math.round(rect.height * dpr);

  const ctx    = canvas.getContext('2d');
  const scaleX = video.videoWidth  / window.innerWidth;
  const scaleY = video.videoHeight / window.innerHeight;

  ctx.drawImage(
    video,
    Math.round(rect.left  * scaleX),
    Math.round(rect.top   * scaleY),
    Math.round(rect.width  * scaleX),
    Math.round(rect.height * scaleY),
    0, 0, canvas.width, canvas.height,
  );

  stream.getTracks().forEach(t => t.stop());
  return canvas;
}

function captureViaCanvas(rect) {
  const canvas = document.createElement('canvas');
  canvas.width  = rect.width  * devicePixelRatio;
  canvas.height = rect.height * devicePixelRatio;
  const ctx = canvas.getContext('2d');
  ctx.scale(devicePixelRatio, devicePixelRatio);
  ctx.fillStyle = '#fff';
  ctx.fillRect(0, 0, rect.width, rect.height);
  ctx.fillStyle = '#000';
  ctx.font = '14px system-ui';

  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, null);
  let node;
  while ((node = walker.nextNode())) {
    const range = document.createRange();
    range.selectNode(node);
    const r = range.getBoundingClientRect();
    if (r.left < rect.left + rect.width && r.right > rect.left &&
        r.top  < rect.top  + rect.height && r.bottom > rect.top) {
      ctx.fillText(node.textContent.trim(), r.left - rect.left, r.bottom - rect.top);
    }
  }
  return canvas;
}

async function loadTesseract() {
  const { createWorker } = await import('https://cdn.jsdelivr.net/npm/tesseract.js@5/dist/tesseract.esm.min.js');
  return createWorker(['chi_sim', 'eng'], 1, {
    workerPath:  '/assets/tesseract/worker.min.js',
    corePath:    '/assets/tesseract/tesseract-core.wasm.js',
    langPath:    '/assets/tesseract/lang-data',
    cacheMethod: 'write',
    logger: m => {
      if (m.status === 'recognizing text') updateSpinnerProgress(m.progress);
    },
  });
}

async function tryTranslate(text) {
  const cjkRatio = (text.match(/[\u4e00-\u9fa5]/g) ?? []).length / text.length;
  const targetLang = cjkRatio > 0.3 ? 'English' : 'Chinese';
  try {
    const prompt = `Translate the following text to ${targetLang}. Output ONLY the translation:\n\n${text.slice(0, 800)}`;
    const result = await invoke('cmd_fetch', {
      url: 'http://localhost:11434/api/generate', method: 'POST',
    });
    if (result?.body) {
      const json        = JSON.parse(result.body);
      const translation = json?.response?.trim();
      if (translation) appendTranslation(translation);
    }
}

function cancelSelection() {
  _selBox?.remove(); _selBox = null;
}

function dismissOverlay() {
  _overlay?.remove(); _overlay = null;
}

function showSpinner(rect) {
  dismissOverlay();
  _overlay = el('div', 'vision-overlay');
  _overlay.style.cssText = `
    position:fixed; z-index:99999;
    left:${rect.left}px; top:${rect.top + rect.height + 8}px;
    background:rgba(15,23,42,.92); border:1px solid rgba(96,165,250,.22);
    border-radius:.5rem; padding:.6rem .9rem; color:#94a3b8; font-size:.75rem;
    font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;
  `;
  _overlay.innerHTML = `<span id="vision-progress">Recognising…</span>`;
  document.body.appendChild(_overlay);
}

function updateSpinnerProgress(progress) {
  const el = qs('#vision-progress');
  if (el) el.textContent = `Recognising… ${Math.round(progress * 100)}%`;
}

function showResult(rect, text) {
  dismissOverlay();
  _overlay = el('div', 'vision-overlay');
  _overlay.style.cssText = `
    position:fixed; z-index:99999;
    left:${Math.min(rect.left, window.innerWidth - 340)}px;
    top:${rect.top + rect.height + 8}px;
    max-width:320px; min-width:160px;
    background:rgba(15,23,42,.96); border:1px solid rgba(96,165,250,.22);
    border-radius:.5rem; padding:.8rem 1rem;
    font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;
  `;

  const copyBtn  = el('button', 'vision-copy');
  copyBtn.textContent  = 'Copy';
  copyBtn.style.cssText = 'cursor:pointer;background:rgba(96,165,250,.14);border:1px solid rgba(96,165,250,.3);color:#60a5fa;border-radius:.3rem;padding:.2rem .6rem;font-size:.72rem;margin-right:.4rem;';
  copyBtn.addEventListener('click', () => {
    navigator.clipboard.writeText(text).then(() => { copyBtn.textContent = 'Copied ✓'; });
  });

  const closeBtn = el('button', 'vision-close');
  closeBtn.textContent  = '✕';
  closeBtn.style.cssText = 'cursor:pointer;background:none;border:none;color:#475569;font-size:.9rem;padding:0;';
  closeBtn.addEventListener('click', dismissOverlay);

  const header = el('div', 'vision-header');
  header.style.cssText = 'display:flex;align-items:center;justify-content:space-between;margin-bottom:.5rem;';
  header.appendChild(copyBtn);
  header.appendChild(closeBtn);

  const textEl = el('p', 'vision-text');
  textEl.style.cssText = 'color:#e2e8f0;font-size:.8rem;line-height:1.5;margin:0;word-break:break-word;white-space:pre-wrap;';
  textEl.textContent = text;

  _overlay.appendChild(header);
  _overlay.appendChild(textEl);
  document.body.appendChild(_overlay);
}

function appendTranslation(translation) {
  if (!_overlay) return;
  const div = el('div', 'vision-translation');
  div.style.cssText = 'border-top:1px solid rgba(255,255,255,.07);margin-top:.6rem;padding-top:.6rem;';
  const label = el('span', 'vision-translation-label');
  label.style.cssText = 'color:#475569;font-size:.68rem;display:block;margin-bottom:.2rem;';
  label.textContent = 'Translation';
  const text = el('p', 'vision-translation-text');
  text.style.cssText = 'color:#94a3b8;font-size:.78rem;line-height:1.5;margin:0;white-space:pre-wrap;';
  text.textContent = translation;
  div.appendChild(label);
  div.appendChild(text);
  _overlay.appendChild(div);
}

function showError(msg) {
  dismissOverlay();
  _overlay = el('div', 'vision-error');
  _overlay.style.cssText = `
    position:fixed; z-index:99999; bottom:1rem; right:1rem;
    background:rgba(196,72,72,.18); border:1px solid rgba(196,72,72,.3);
    color:#f87171; border-radius:.4rem; padding:.5rem .8rem;
    font-size:.75rem; font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;
  `;
  _overlay.textContent = msg;
  document.body.appendChild(_overlay);
  setTimeout(dismissOverlay, 3000);
}

