/**
 * diatom/src/browser/hotkey.js  — v7.1
 *
 * Application-aware hotkey system.
 *
 * Problem solved:
 *   Alt+drag (Vision Overlay) firing while a designer is using Figma.
 *   Ctrl+S (Freeze) firing while a user is editing in Notion.
 *   Ctrl+W (close tab) firing while a user is in a code editor WebApp.
 *
 * Solution:
 *   1. Detect the "active context": which domain/app is in the foreground WebView.
 *   2. For high-priority professional domains, Diatom yields its global hotkeys
 *      entirely, OR remaps them to require a longer gesture (double-tap modifier).
 *   3. Users can add custom yield domains in diatom.json.
 *
 * Three yield levels:
 *   FULL_YIELD   — Diatom does nothing when focused on this domain.
 *                  (e.g. Figma, CodePen, Google Docs, VS Code Web)
 *   PARTIAL_YIELD — Only Alt+drag and keyboard capture are yielded.
 *                  (e.g. Notion, Linear, Slack)
 *   NORMAL       — Diatom's full hotkey set is active.
 */

'use strict';

import { invoke } from './ipc.js';
import { domainOf } from './utils.js';

// ── Yield domain lists ────────────────────────────────────────────────────────

const FULL_YIELD_DOMAINS = new Set([
  // Design / creative — these apps claim Alt, Ctrl, Shift for their own tools
  'figma.com', 'sketch.com', 'canva.com', 'adobe.com', 'framer.com',
  'excalidraw.com', 'miro.com', 'whimsical.com', 'www.canva.com',
  // Code editors in browser
  'codepen.io', 'codesandbox.io', 'stackblitz.com', 'replit.com',
  'vscode.dev', 'github.dev', 'gitpod.io', 'glitch.com',
  // Terminal / shell apps
  'ssh.online-convert.com', 'ttyd.github.io',
  // Video editing / audio
  'descript.com', 'kapwing.com',
]);

const PARTIAL_YIELD_DOMAINS = new Set([
  // Productivity suites — claim Ctrl+S for "save", Ctrl+W for panel close
  'notion.so', 'www.notion.so', 'linear.app', 'clickup.com',
  'app.asana.com', 'trello.com', 'airtable.com',
  // Office suites
  'docs.google.com', 'sheets.google.com', 'slides.google.com',
  'onedrive.live.com', 'office.com',
  // Messaging — Ctrl+B is bold in Slack/Discord, Ctrl+K is link
  'app.slack.com', 'discord.com', 'teams.microsoft.com',
]);

// Hotkeys that are always yielded in PARTIAL mode
const PARTIAL_YIELD_KEYS = new Set(['s', 'w', 'b', 'i', 'k', 'z']);

// ── State ─────────────────────────────────────────────────────────────────────

let _currentDomain  = '';
let _yieldLevel     = 'NORMAL';          // 'FULL_YIELD' | 'PARTIAL_YIELD' | 'NORMAL'
let _userYieldDomains = new Set();        // loaded from settings
let _registeredHandlers = new Map();     // key → { handler, description }
let _altDown = false;
let _visionActive = false;

// ── Init ──────────────────────────────────────────────────────────────────────

export async function initHotkeys() {
  // Load user-configured yield domains
  try {
    const raw = await invoke('cmd_setting_get', { key: 'hotkey_yield_domains' });
    if (raw) {
      JSON.parse(raw).forEach(d => _userYieldDomains.add(d));
    }
  } catch { /* ok — settings missing */ }

  document.addEventListener('keydown', onKeyDown, { capture: true });
  document.addEventListener('keyup',   onKeyUp,   { capture: true });
}

// ── Domain context update ─────────────────────────────────────────────────────

/**
 * Called by tabs.js whenever the active tab URL changes.
 * Updates the yield level for the new domain.
 */
export function updateContext(url) {
  _currentDomain = domainOf(url);
  _yieldLevel = classifyDomain(_currentDomain);

  // If we just switched to a full-yield app, cancel any active Vision Overlay
  if (_yieldLevel === 'FULL_YIELD' && _visionActive) {
    cancelVisionIfActive();
  }
}

function classifyDomain(domain) {
  if (_userYieldDomains.has(domain)) return 'FULL_YIELD';
  if (FULL_YIELD_DOMAINS.has(domain)) return 'FULL_YIELD';
  if (PARTIAL_YIELD_DOMAINS.has(domain)) return 'PARTIAL_YIELD';
  return 'NORMAL';
}

// ── Handler registration ──────────────────────────────────────────────────────

/**
 * Register a Diatom hotkey.
 *
 * @param {string} id          Unique name, e.g. 'freeze', 'zen', 'vision'
 * @param {object} binding     { key, ctrl, alt, shift, meta }
 * @param {Function} handler
 * @param {object} [opts]      { yieldInPartial: bool }  default true
 */
export function register(id, binding, handler, opts = {}) {
  _registeredHandlers.set(id, {
    binding,
    handler,
    yieldInPartial: opts.yieldInPartial !== false,
  });
}

export function unregister(id) {
  _registeredHandlers.delete(id);
}

// ── Key handler ───────────────────────────────────────────────────────────────

function onKeyDown(e) {
  if (e.key === 'Alt') { _altDown = true; }

  // Full yield: pass everything through
  if (_yieldLevel === 'FULL_YIELD') return;

  for (const [id, { binding, handler, yieldInPartial }] of _registeredHandlers) {
    if (!matchesBinding(e, binding)) continue;

    // Partial yield: skip hotkeys that conflict with productivity apps
    if (_yieldLevel === 'PARTIAL_YIELD' && yieldInPartial) {
      if (PARTIAL_YIELD_KEYS.has(e.key.toLowerCase())) return;
    }

    e.preventDefault();
    e.stopPropagation();
    handler(e);
    return;
  }
}

function onKeyUp(e) {
  if (e.key === 'Alt') {
    _altDown = false;
    _visionActive = false;
  }
}

function matchesBinding(e, b) {
  return e.key.toLowerCase() === b.key.toLowerCase()
    && !!e.ctrlKey  === !!b.ctrl
    && !!e.altKey   === !!b.alt
    && !!e.shiftKey === !!b.shift
    && !!e.metaKey  === !!b.meta;
}

function cancelVisionIfActive() {
  // Signal the vision overlay module to cancel
  document.dispatchEvent(new CustomEvent('diatom:cancel-vision'));
}

// ── Add yield domain UI ───────────────────────────────────────────────────────

/**
 * Add the current domain to user yield list (persisted in settings).
 * Called from the context menu / settings panel.
 */
export async function yieldCurrentDomain() {
  _userYieldDomains.add(_currentDomain);
  _yieldLevel = 'FULL_YIELD';
  await invoke('cmd_setting_set', {
    key:   'hotkey_yield_domains',
    value: JSON.stringify([..._userYieldDomains]),
  });
}

export function getYieldLevel() { return _yieldLevel; }
export function getCurrentDomain() { return _currentDomain; }

// ── Built-in Diatom hotkey registrations ─────────────────────────────────────
// These are registered from main.js after initHotkeys().
// Exported so main.js can call them after module imports settle.

export function registerDefaultHotkeys({ onFreeze, onZen, onVision, onNewTab, onCloseTab }) {
  register('new_tab',   { key: 't', ctrl: true },  onNewTab,   { yieldInPartial: false });
  register('close_tab', { key: 'w', ctrl: true },  onCloseTab, { yieldInPartial: true  });
  register('freeze',    { key: 's', ctrl: true },  onFreeze,   { yieldInPartial: true  });
  register('zen',       { key: 'Z', ctrl: true, shift: true }, onZen, { yieldInPartial: false });

  // Vision Overlay: Alt+drag is mouse-driven, not a keyboard shortcut per se.
  // We track Alt state here; the actual drag is in vision-overlay.js.
  // In FULL_YIELD mode the Alt state is still tracked but vision-overlay.js
  // checks getYieldLevel() before activating.
  register('vision_alt', { key: 'Alt', alt: false }, () => {
    _visionActive = true;
  }, { yieldInPartial: false });
}
