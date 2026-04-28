
'use strict';

const MIN_SPEED = 0.1;
const MAX_SPEED = 16.0;
const STEP      = 0.1;
const SKIP_SECS = 10;

export const PAGE_INJECTION = /* js */`
(function() {
  'use strict';

  const MIN = 0.1, MAX = 16, STEP = 0.1, SKIP = 10;
  let overlay = null;
  let activeVideo = null;
  let hidden = false;

  function getSpeed(v) { return v ? parseFloat(v.playbackRate.toFixed(2)) : 1; }

  function setSpeed(v, s) {
    if (!v) return;
    v.playbackRate = Math.max(MIN, Math.min(MAX, s));
    updateOverlay(v);
  }

  function buildOverlay() {
    const el = document.createElement('div');
    el.id = '__diatom_vc';
    el.style.cssText = \`
      position:absolute; z-index:2147483647; pointer-events:none;
      top:8px; left:8px; padding:3px 8px;
      background:rgba(0,0,0,.65); color:#fff;
      font:700 13px/1.4 'Inter',system-ui,monospace;
      border-radius:4px; transition:opacity .15s;
      user-select:none;
    \`;
    return el;
  }

  function updateOverlay(v) {
    if (!overlay || hidden) return;
    const s = getSpeed(v);
    overlay.textContent = s === 1 ? '' : s.toFixed(2) + '×';
    overlay.style.opacity = s === 1 ? '0' : '1';
  }

  function attachToVideo(v) {
    if (v.__diatom_vc) return;
    v.__diatom_vc = true;
    activeVideo = v;

    const wrap = v.parentElement;
    if (!wrap) return;
    const pos = getComputedStyle(wrap).position;
    if (pos === 'static') wrap.style.position = 'relative';

    overlay = buildOverlay();
    wrap.appendChild(overlay);

    v.addEventListener('mouseenter', () => { activeVideo = v; updateOverlay(v); });
    v.addEventListener('mouseleave', () => {
      if (overlay) overlay.style.opacity = '0';
    });
  }

  function scanVideos() {
    document.querySelectorAll('video, audio').forEach(attachToVideo);
  }
  scanVideos();
  new MutationObserver(scanVideos).observe(document.body || document.documentElement,
    { childList: true, subtree: true });

  document.addEventListener('keydown', function(e) {
    const tag = e.target?.tagName?.toLowerCase();
    if (tag === 'input' || tag === 'textarea' || e.target?.isContentEditable) return;
    const v = activeVideo;

    switch (e.key) {
      case 's': setSpeed(v, getSpeed(v) - STEP); break;
      case 'd': setSpeed(v, getSpeed(v) + STEP); break;
      case 'r': setSpeed(v, 1); break;
      case 'z': if (v) v.currentTime = Math.max(0, v.currentTime - SKIP); break;
      case 'x': if (v) v.currentTime = v.currentTime + SKIP; break;
      case 'v':
        hidden = !hidden;
        if (overlay) overlay.style.display = hidden ? 'none' : 'block';
        break;
      default: return;
    }
    e.preventDefault();
  }, { capture: false });

  document.addEventListener('wheel', function(e) {
    if (!e.altKey || !activeVideo) return;
    const delta = e.deltaY > 0 ? -STEP : STEP;
    setSpeed(activeVideo, getSpeed(activeVideo) + delta);
    e.preventDefault();
  }, { passive: false });

})();
`;

import { invoke } from '../browser/ipc.js';

let _enabled = true;

export function setEnabled(v) { _enabled = v; }

export async function injectVideoController() {
  if (!_enabled) return;
  try {
    await invoke('cmd_page_eval', { script: PAGE_INJECTION });
  } catch {
  }
}

export function showSpeedBadge(speed) {
  let badge = document.querySelector('#vc-badge');
  if (!badge) {
    badge = document.createElement('span');
    badge.id = 'vc-badge';
    badge.style.cssText = `
      display:inline-flex; align-items:center; height:18px;
      padding:0 6px; border-radius:9px;
      background:rgba(96,165,250,.15); color:#60a5fa;
      font:600 10px/1 'Inter',system-ui; letter-spacing:.04em;
      cursor:pointer; transition:opacity .2s;
    `;
    badge.title = 'S = slow down · D = speed up · R = reset · Z/X = skip 10s';
    const omniArea = document.querySelector('#omnibox-right');
    if (omniArea) omniArea.appendChild(badge);
  }
  badge.textContent = speed === 1 ? '' : `${speed.toFixed(2)}×`;
  badge.style.opacity = speed === 1 ? '0' : '1';
}
