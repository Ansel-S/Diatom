
'use strict';

import { invoke, listen } from './browser/ipc.js';
import { initTabs, createTab, closeTab, navigate, freezeCurrentPage, setReadingMode } from './browser/tabs.js';
import { initHotkeys, registerDefaultHotkeys, updateContext as updateHotkeyContext } from './browser/hotkey.js';
import { updateLustre } from './browser/lustre.js';
import { initZen, activate as zenActivate, isActive as zenIsActive } from './features/zen.js';
import { initVisionOverlay } from './features/vision-overlay.js';
import { initCrusherCapture } from './features/dom-crusher.js';
import { openNetworkPanel } from './features/network-panel.js';
import { injectVideoController } from './features/video-controller.js';
import { qs } from './browser/utils.js';
import { tosAuditor } from './features/tos-auditor.js';
import { shadowIndex } from './features/shadow-index.js';

const _loadedStyles = new Set();

function loadStylesheet(href) {
  if (_loadedStyles.has(href)) return Promise.resolve();
  return new Promise((resolve) => {
    const link = document.createElement('link');
    link.rel  = 'stylesheet';
    link.href = href;
    link.onload  = () => { _loadedStyles.add(href); resolve(); };
    link.onerror = resolve;
    document.head.appendChild(link);
  });
}

const worker = new Worker('/workers/core.worker.js', { type: 'module' });

worker.addEventListener('message', e => {
  if (e.data?.type === 'SW_MUSEUM_SYNC') {
    navigator.serviceWorker?.controller?.postMessage({
      type:  'MUSEUM_INDEX',
      index: e.data.index,
    });
  }
  if (e.data?.type === 'INDEX_PROGRESS') {
    updateIndexProgressBadge(e.data.remaining);
  }
  if (e.data?.type === 'READING_EVENTS_READY') {
    for (const evt of e.data.events) {
      invoke('cmd_record_reading', { evt }).catch(() => {});
    }
  }
});

async function registerSW() {
  if (!('serviceWorker' in navigator)) return;
  try {
    await navigator.serviceWorker.register('/sw.js', { scope: '/' });
    const bc = new BroadcastChannel('diatom:sw');
    bc.postMessage({ type: 'CONFIG', config: {
      adblock:        true,
      ua_uniformity:  true,
      csp_injection:  true,
      zen_active:     zenIsActive(),
    }});
    bc.close();
  } catch (err) {
    console.warn('[SW] registration failed:', err);
  }
}

function routeCommand(input) {
  const s = input.trim();

  if (s === '/devnet') {
    openNetworkPanel();
    return true;
  }
  if (s === '/files' || s === '/localfiles') {
    navigate('diatom://localfiles');
    return true;
  }
  if (s === '/zen') {
    zenActivate();
    return true;
  }
  if (s.startsWith('/json')) {
    openWasmTool('json', s.slice(5).trim());
    return true;
  }
  if (s.startsWith('/crypto')) {
    openWasmTool('crypto', s.slice(7).trim());
    return true;
  }
  if (s.startsWith('/img')) {
    openWasmTool('img', '');
    return true;
  }
  if (s.startsWith('/scholar ') || s.startsWith('/debug ') ||
      s.startsWith('/scribe ')  || s.startsWith('/oracle ')) {
    const [mode, ...rest] = s.slice(1).split(' ');
    openAiPanel(mode, rest.join(' '));
    return true;
  }
  return false;
}

function openWasmTool(tool, input) {
  const params = new URLSearchParams({ tool, input });
  navigate(`diatom://tools?${params}`);
}

function openAiPanel(mode, query) {
  const panel = qs('#ai-panel');
  if (panel) {
    panel.dataset.mode  = mode;
    panel.dataset.query = query;
    panel.hidden = false;
  }
}

async function startMuseumIndexing() {
  try {
    const bundles = await invoke('cmd_museum_list', { limit: 1000 });
    if (!bundles?.length) return;
    if (bundles.length > 50) {
      worker.postMessage({ id: 'startup', type: 'MUSEUM_LOAD_IDLE', payload: { entries: bundles } });
    } else {
      worker.postMessage({ id: 'startup', type: 'MUSEUM_LOAD', payload: { entries: bundles } });
    }
}

function updateIndexProgressBadge(remaining) {
  let badge = qs('#index-progress');
  if (!badge && remaining > 0) {
    badge = document.createElement('div');
    badge.id = 'index-progress';
    badge.style.cssText = `
      position:fixed; bottom:1rem; right:1rem; z-index:9000;
      background:rgba(15,23,42,.88); border:1px solid rgba(255,255,255,.08);
      color:#64748b; font:500 .7rem/1 system-ui,sans-serif;
      padding:.3rem .6rem; border-radius:.25rem; pointer-events:none;
    `;
    document.body.appendChild(badge);
  }
  if (badge) {
    if (remaining === 0) badge.remove();
    else badge.textContent = `🗂 Indexing… ${remaining} remaining`;
  }
}

function showOnionSuggestionBanner(suggestion) {
  const existing = qs('#diatom-onion-banner');
  if (existing) existing.remove();

  const banner = document.createElement('div');
  banner.id = 'diatom-onion-banner';
  banner.style.cssText = `
    position: fixed; bottom: 1.2rem; right: 1.2rem; z-index: 9500;
    background: rgba(15,23,42,.96); border: 1px solid rgba(96,165,250,.25);
    border-radius: .5rem; padding: .8rem 1rem; max-width: 320px;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
    font-size: .78rem; color: #94a3b8; line-height: 1.5;
    box-shadow: 0 4px 20px rgba(0,0,0,.4);
    display: flex; align-items: flex-start; gap: .7rem;
  `;

  const icon = document.createElement('span');
  icon.textContent = '🧅';
  icon.style.cssText = 'font-size:1.1rem;flex-shrink:0;margin-top:.05rem;';

  const body = document.createElement('div');
  body.style.cssText = 'flex:1;min-width:0;';

  const title = document.createElement('div');
  title.style.cssText = 'color:#60a5fa;font-weight:500;margin-bottom:.2rem;';
  title.textContent = 'More private mirror available';

  const desc = document.createElement('div');
  desc.textContent = suggestion.label;

  const addr = document.createElement('div');
  addr.style.cssText = 'font-family:SF Mono,Cascadia Code,monospace;font-size:.68rem;color:#475569;word-break:break-all;margin-top:.2rem;';
  addr.textContent = suggestion.hidden_host;

  const btnRow = document.createElement('div');
  btnRow.style.cssText = 'display:flex;gap:.5rem;margin-top:.6rem;';

  const copyBtn = document.createElement('button');
  copyBtn.textContent = 'Copy address';
  copyBtn.style.cssText = 'background:rgba(96,165,250,.14);border:1px solid rgba(96,165,250,.3);color:#60a5fa;border-radius:.3rem;padding:.25rem .6rem;font-size:.7rem;cursor:pointer;';
  copyBtn.addEventListener('click', () => {
    navigator.clipboard.writeText(suggestion.hidden_host).then(() => {
      copyBtn.textContent = 'Copied ✓';
      setTimeout(() => banner.remove(), 1500);
    });
  });

  const dismissBtn = document.createElement('button');
  dismissBtn.textContent = 'Dismiss';
  dismissBtn.style.cssText = 'background:none;border:1px solid rgba(255,255,255,.08);color:#475569;border-radius:.3rem;padding:.25rem .6rem;font-size:.7rem;cursor:pointer;';
  dismissBtn.addEventListener('click', () => banner.remove());

  btnRow.appendChild(copyBtn);
  btnRow.appendChild(dismissBtn);

  body.appendChild(title);
  body.appendChild(desc);
  body.appendChild(addr);
  body.appendChild(btnRow);

  banner.appendChild(icon);
  banner.appendChild(body);
  document.body.appendChild(banner);

  setTimeout(() => banner.remove(), 15_000);
}

async function onTabChange(tabId) {
  try {
    const state = await invoke('cmd_tabs_state');
    const tab   = state.tabs?.find(t => t.id === tabId);
    if (tab?.url) {
      updateHotkeyContext(tab.url);
      let faviconUrl = '';
      try {
        const host = new URL(tab.url).hostname;
        faviconUrl = `https://icons.duckduckgo.com/ip3/${host}.ico`;
      if (faviconUrl) updateLustre(faviconUrl);
      injectVideoController();
    }
}

async function boot() {
  try {
    await invoke('cmd_signal_window_ready').catch(() => {});

  await initHotkeys();
  registerDefaultHotkeys({
    onNewTab:   () => createTab(),
    onCloseTab: () => { const id = qs('[data-tab-id].active')?.dataset.tabId; if (id) closeTab(id); },
    onFreeze:   () => freezeCurrentPage(),
    onZen:      () => zenIsActive() ? import('./features/zen.js').then(m => m.deactivate()) : zenActivate(),
  });

  const omnibox = qs('#omnibox');
  if (omnibox) {
    omnibox.addEventListener('keydown', e => {
      if (e.key !== 'Enter') return;
      const input = omnibox.value.trim();
      if (routeCommand(input)) {
        e.preventDefault();
        omnibox.blur();
        omnibox.value = '';
      }
    });
  }

  await initTabs(worker);

  await initZen();

  initVisionOverlay();

  initCrusherCapture();

  await listen('diatom:tab_activated', e => onTabChange(e.tab_id));

  document.addEventListener('visibilitychange', () => {
    worker?.postMessage({ type: 'VISIBILITY', payload: { hidden: document.hidden } });
  });

  await registerSW();

  setTimeout(() => startMuseumIndexing(), 3000);

  setTimeout(() => invoke('cmd_threat_list_refresh').catch(() => {}), 10_000);

  if (window.__DIATOM_INIT__?.labs?.tos_auditor !== false) {
    window.__diatom_tos_auditor = tosAuditor;
  }

  window.__diatom_shadow_index = shadowIndex;

  window.addEventListener('diatom:onion_suggest', e => {
    if (e.detail) showOnionSuggestionBanner(e.detail);
  });
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', boot);
} else {
  boot();
}

