/**
 * diatom/src/features/video-controller.js  — v7.1
 *
 * Video Speed Controller: global 0.1×–16× playback speed for every
 * <video> and <audio> element on any page.
 *
 * Absorbed from the Video Speed Controller extension pattern.
 * Runs entirely locally — no network calls, no external scripts.
 *
 * Activation:
 *   - Speed overlay appears on mouseenter over any video element.
 *   - Keyboard shortcuts (page-context, not global):
 *       S        → slow down 0.1×
 *       D        → speed up 0.1×
 *       R        → reset to 1×
 *       Z        → rewind 10s
 *       X        → forward 10s
 *       V        → toggle overlay visibility
 *
 * The overlay is injected into the page via diatom-api.js.
 * This file is the shell-side controller; it receives
 * speed-change requests from the page via BroadcastChannel.
 */

'use strict';

// ── Constants ─────────────────────────────────────────────────────────────────

const MIN_SPEED = 0.1;
const MAX_SPEED = 16.0;
const STEP      = 0.1;
const SKIP_SECS = 10;

// ── Page-side injection code ──────────────────────────────────────────────────
// This string is eval'd into every page context via cmd_page_eval.
// It is self-contained: no imports, no closure over outer scope.

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

    // Wrap in a positioned container so overlay is relative to video
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

  // Attach to all existing and future videos
  function scanVideos() {
    document.querySelectorAll('video, audio').forEach(attachToVideo);
  }
  scanVideos();
  new MutationObserver(scanVideos).observe(document.body || document.documentElement,
    { childList: true, subtree: true });

  // Keyboard shortcuts (page-context only, not captured globally)
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

  // Speed dial: mouse wheel over video while holding Alt
  document.addEventListener('wheel', function(e) {
    if (!e.altKey || !activeVideo) return;
    const delta = e.deltaY > 0 ? -STEP : STEP;
    setSpeed(activeVideo, getSpeed(activeVideo) + delta);
    e.preventDefault();
  }, { passive: false });

})();
`;

// ── Shell-side: inject on every navigation ────────────────────────────────────

import { invoke } from '../browser/ipc.js';

let _enabled = true;

export function setEnabled(v) { _enabled = v; }

/**
 * Inject the video controller into the current page.
 * Called from tabs.js on navigation.
 */
export async function injectVideoController() {
  if (!_enabled) return;
  try {
    // Use Tauri's eval to inject the page-side code
    await invoke('cmd_page_eval', { script: PAGE_INJECTION });
  } catch {
    // Non-critical — page may block eval in strict CSP contexts
  }
}

/**
 * Render a small speed badge in the Diatom chrome (address bar area)
 * showing the current playback speed for ambient awareness.
 */
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
    badge.title = 'S = 减速 · D = 加速 · R = 重置 · Z/X = 跳10秒';
    const omniArea = document.querySelector('#omnibox-right');
    if (omniArea) omniArea.appendChild(badge);
  }
  badge.textContent = speed === 1 ? '' : `${speed.toFixed(2)}×`;
  badge.style.opacity = speed === 1 ? '0' : '1';
}
