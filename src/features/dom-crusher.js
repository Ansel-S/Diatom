
'use strict';

import { invoke } from '../browser/ipc.js';
import { el, qs, domainOf, escHtml } from '../browser/utils.js';

let _active      = false;        // Ctrl-click capture mode
let _overlay     = null;         // current highlight overlay element
let _lastTarget  = null;         // last hovered element
let _rules       = [];           // { id, selector } for current domain

export function applyCrusherRules(rules) {
  if (!rules?.length) return;
  _rules = rules;

  for (const rule of rules) {
    crushSelector(rule.selector);
  }

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
      el.setAttribute('data-diatom-crushed', '1');
    });
  } catch {
  }
}

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

async function crushElement(target) {
  const selector = generateSelector(target);
  if (!selector) return;

  target.style.setProperty('display', 'none', 'important');
  target.setAttribute('data-diatom-crushed', '1');

  const domain = domainOf(location.href);
  try {
    const id = await invoke('cmd_dom_crush', { domain, selector });
    _rules.push({ id, selector });

    flashConfirmation(selector);
  } catch (err) {
    console.warn('[DOM Crusher] selector rejected:', err.message);
    target.style.removeProperty('display');
    target.removeAttribute('data-diatom-crushed');
    showRejectionHint(err.message);
  }
}

function generateSelector(el) {
  if (!el || el === document.body || el === document.documentElement) return null;

  if (el.id && /^[\w-]+$/.test(el.id) && document.querySelectorAll(`#${CSS.escape(el.id)}`).length === 1) {
    return `#${CSS.escape(el.id)}`;
  }

  for (const attr of el.attributes) {
    if (attr.name.startsWith('data-') && attr.value && attr.value.length < 80) {
      const sel = `${el.tagName.toLowerCase()}[${attr.name}="${CSS.escape(attr.value)}"]`;
      if (document.querySelectorAll(sel).length === 1) return sel;
    }
  }

  const tag     = el.tagName.toLowerCase();
  const classes = [...el.classList]
    .filter(c => /^[\w-]+$/.test(c) && !c.startsWith('_') && c.length < 40)
    .slice(0, 3)
    .map(c => `.${CSS.escape(c)}`)
    .join('');

  if (classes) {
    const candidate = `${tag}${classes}`;
    if (document.querySelectorAll(candidate).length === 1) return candidate;

    const parent = el.parentElement;
    if (parent && parent !== document.body) {
      const parentSel = generateSelector(parent);
      if (parentSel) {
        const full = `${parentSel} > ${candidate}`;
        if (full.length < 200 && document.querySelectorAll(full).length === 1) return full;
      }
    }
  }

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
  label.textContent = 'Click to crush';
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
  msg.textContent = `Permanently blocked · ${selector.slice(0, 48)}`;

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
  msg.textContent = `Selector rejected: ${reason}`;
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

export async function showCrusherRules() {
  const domain = domainOf(location.href);
  let rules;
  try {
    rules = await invoke('cmd_dom_blocks_for', { domain });
  } catch {
    return;
  }
  if (!rules.length) {
    alert(`No blocked elements on ${domain}.`);
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
      <span style="font-weight:600;color:#e8e8f0;">Block rules for ${domain}</span>
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
      _rules = _rules.filter(r => r.id !== id);
    });
  });

  document.body.appendChild(panel);
}
