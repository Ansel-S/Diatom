/**
 * diatom/src/features/zen.js  — v7
 *
 * Zen Mode frontend.
 *
 * Activation: /zen address-bar command or keyboard shortcut (Ctrl+Shift+Z).
 * Deactivation: user types ≥ 50-character "intent declaration" in the interstitial.
 *
 * When active:
 *   - All Notification API calls are suppressed (handled in sw.js via BroadcastChannel)
 *   - Navigations to blocked-category domains show the Zen interstitial instead of loading
 *   - The address bar receives a faint teal left border
 */

'use strict';

import { invoke } from '../browser/ipc.js';
import { el, qs } from '../browser/utils.js';

let _zenActive = false;
let _aphorism  = '当下这一刻，将永远曾经存在。';

// ── Init ──────────────────────────────────────────────────────────────────────

export async function initZen() {
  try {
    const cfg = await invoke('cmd_zen_state');
    _zenActive = cfg.state === 'Active';
    _aphorism  = cfg.aphorism || _aphorism;
    if (_zenActive) applyZenUi(true);
  } catch (err) {
    console.warn('[Zen] init failed:', err);
  }

  // Keyboard shortcut: Ctrl+Shift+Z
  document.addEventListener('keydown', e => {
    if (e.ctrlKey && e.shiftKey && e.key === 'Z') {
      e.preventDefault();
      _zenActive ? deactivate() : activate();
    }
  });

  // Notify Service Worker of current state
  broadcastToSW();
}

// ── Activate / Deactivate ────────────────────────────────────────────────────

export async function activate() {
  await invoke('cmd_zen_activate');
  _zenActive = true;
  applyZenUi(true);
  broadcastToSW();
}

export async function deactivate() {
  await invoke('cmd_zen_deactivate');
  _zenActive = false;
  applyZenUi(false);
  broadcastToSW();
}

export function isActive() { return _zenActive; }

// ── Interstitial ─────────────────────────────────────────────────────────────

/**
 * Show the Zen interstitial when a blocked domain is navigated to.
 * Returns a Promise that resolves when the user unlocks Zen (or refuses).
 */
export function showInterstitial(domain, category) {
  return new Promise(resolve => {
    // Remove any existing interstitial
    qs('#zen-interstitial')?.remove();

    const overlay = el('div', '');
    overlay.id = 'zen-interstitial';
    overlay.style.cssText = `
      position:fixed; inset:0; z-index:99999;
      background:#0a0a10;
      display:flex; flex-direction:column; align-items:center; justify-content:center;
      font-family:'Inter',system-ui,sans-serif; color:#e8e8f0;
    `;

    // Breathing aphorism
    const quote = el('p');
    quote.style.cssText = `
      font-family:'Playfair Display',Georgia,serif;
      font-size:clamp(1.2rem,4vw,2.2rem); font-weight:700;
      text-align:center; max-width:600px; line-height:1.5;
      color:#e8e8f0; margin:0 0 3rem;
      animation: zen-breathe 4s ease-in-out infinite;
    `;
    quote.textContent = _aphorism;
    overlay.appendChild(quote);

    // Breathing CSS
    if (!qs('#zen-breathe-style')) {
      const style = document.createElement('style');
      style.id = 'zen-breathe-style';
      style.textContent = `
        @keyframes zen-breathe {
          0%,100% { transform:scale(1);   opacity:.9; }
          50%      { transform:scale(1.02);opacity:1; }
        }
      `;
      document.head.appendChild(style);
    }

    // Blocked domain info
    const info = el('p');
    info.style.cssText = 'color:#475569;font-size:.85rem;margin:0 0 2rem;text-align:center;';
    info.textContent = `${domain} · ${category === 'social' ? '社交媒体' : '娱乐内容'} · 已被 Zen 模式屏蔽`;
    overlay.appendChild(info);

    // Unlock gate — 50-char intent declaration
    const unlockWrap = el('div');
    unlockWrap.style.cssText = 'width:100%;max-width:480px;';

    const label = el('label');
    label.style.cssText = 'display:block;color:#64748b;font-size:.8rem;margin-bottom:.5rem;';
    label.textContent   = '输入你的专注声明（至少 50 字）以暂时解除 Zen 模式：';
    unlockWrap.appendChild(label);

    const textarea = el('textarea');
    textarea.style.cssText = `
      width:100%; height:4rem; background:rgba(255,255,255,.04);
      border:1px solid rgba(255,255,255,.1); border-radius:.4rem;
      color:#e8e8f0; font-family:inherit; font-size:.85rem;
      padding:.5rem .75rem; resize:none; box-sizing:border-box;
      outline:none;
    `;
    textarea.placeholder = '我需要暂时打开这个网站，因为……';
    unlockWrap.appendChild(textarea);

    const counter = el('p');
    counter.style.cssText = 'color:#475569;font-size:.75rem;margin:.25rem 0 1rem;text-align:right;';
    counter.textContent   = '0 / 50';
    unlockWrap.appendChild(counter);

    textarea.addEventListener('input', () => {
      const len = textarea.value.length;
      counter.textContent  = `${len} / 50`;
      counter.style.color  = len >= 50 ? '#60a5fa' : '#475569';
      unlockBtn.disabled   = len < 50;
    });

    const buttons = el('div');
    buttons.style.cssText = 'display:flex;gap:.75rem;';

    const unlockBtn = el('button');
    unlockBtn.disabled    = true;
    unlockBtn.textContent = '暂时离开专注状态';
    unlockBtn.style.cssText = `
      flex:1; padding:.65rem; border:none; border-radius:.4rem;
      background:#1e40af; color:#fff; font-size:.85rem; cursor:pointer;
      opacity:.4; transition:opacity .15s;
    `;
    unlockBtn.addEventListener('input', () => {
      unlockBtn.style.opacity = unlockBtn.disabled ? '.4' : '1';
    });
    textarea.addEventListener('input', () => {
      unlockBtn.style.opacity = textarea.value.length >= 50 ? '1' : '.4';
    });
    unlockBtn.addEventListener('click', async () => {
      await deactivate();
      overlay.remove();
      resolve('unlocked');
    });

    const stayBtn = el('button');
    stayBtn.textContent = '返回专注';
    stayBtn.style.cssText = `
      flex:1; padding:.65rem; border:1px solid rgba(255,255,255,.1);
      border-radius:.4rem; background:none; color:#94a3b8;
      font-size:.85rem; cursor:pointer;
    `;
    stayBtn.addEventListener('click', () => {
      overlay.remove();
      resolve('stayed');
      // Navigate back
      history.back();
    });

    buttons.appendChild(stayBtn);
    buttons.appendChild(unlockBtn);
    unlockWrap.appendChild(buttons);
    overlay.appendChild(unlockWrap);

    document.body.appendChild(overlay);
  });
}

// ── UI helpers ────────────────────────────────────────────────────────────────

function applyZenUi(active) {
  const omni = qs('#omnibox');
  if (!omni) return;
  omni.style.borderLeft = active ? '2px solid #0d9488' : '';
  omni.title = active ? 'Zen 模式已激活 · Ctrl+Shift+Z 切换' : '';
}

function broadcastToSW() {
  const bc = new BroadcastChannel('diatom:sw');
  bc.postMessage({ type: 'ZEN', active: _zenActive });
  bc.close();
}
