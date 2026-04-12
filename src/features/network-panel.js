
'use strict';

import { el, qs, escHtml } from '../browser/utils.js';

let _entries  = [];   // NetworkEntry[]
let _filter   = '';
let _paused   = false;
let _bc       = null;
let _panel    = null;
let _tbody    = null;
const MAX_ROWS = 500;

function startListening() {
  if (_bc) return;
  _bc = new BroadcastChannel('diatom:devnet');
  _bc.addEventListener('message', e => {
    if (_paused || !e.data?.type === 'NET_ENTRY') return;
    addEntry(e.data.entry);
  });
}

function stopListening() {
  _bc?.close();
  _bc = null;
}

function addEntry(entry) {
  _entries.unshift(entry);
  if (_entries.length > MAX_ROWS) _entries.pop();
  if (_panel) renderRow(entry, true);  // prepend to table
}

export function openNetworkPanel() {
  startListening();

  if (_panel) { _panel.scrollTop = 0; return; }

  document.title = 'Diatom · /devnet';
  document.body.innerHTML = '';
  document.body.style.cssText = `
    background:#0a0a10; color:#e2e8f0; margin:0; padding:0;
    font-family:'Inter',system-ui,sans-serif; font-size:13px;
  `;

  _panel = el('div', 'devnet-panel');
  _panel.style.cssText = 'display:flex; flex-direction:column; height:100vh; overflow:hidden;';

  const toolbar = el('div', 'devnet-toolbar');
  toolbar.style.cssText = `
    display:flex; align-items:center; gap:.75rem;
    padding:.5rem .75rem; background:#0f172a;
    border-bottom:1px solid rgba(255,255,255,.07); flex-shrink:0;
  `;

  const title = el('span');
  title.style.cssText = 'font-weight:600; color:#60a5fa; font-size:.8rem; letter-spacing:.06em;';
  title.textContent = '/DEVNET';

  const filterInput = el('input');
  filterInput.type        = 'text';
  filterInput.placeholder = 'Filter URL or rule…';
  filterInput.style.cssText = `
    flex:1; background:rgba(255,255,255,.06); border:1px solid rgba(255,255,255,.1);
    border-radius:.3rem; color:#e2e8f0; padding:.25rem .5rem; font:13px 'Inter',system-ui;
    outline:none;
  `;
  filterInput.addEventListener('input', () => {
    _filter = filterInput.value.toLowerCase();
    rebuildTable();
  });

  const pauseBtn = el('button');
  pauseBtn.style.cssText = `
    background:none; border:1px solid rgba(255,255,255,.15); border-radius:.3rem;
    color:#94a3b8; font:500 .75rem 'Inter',system-ui; padding:.25rem .6rem; cursor:pointer;
  `;
  pauseBtn.textContent = '⏸ Pause';
  pauseBtn.addEventListener('click', () => {
    _paused = !_paused;
    pauseBtn.textContent = _paused ? '▶ Resume' : '⏸ Pause';
    pauseBtn.style.color = _paused ? '#f87171' : '#94a3b8';
  });

  const clearBtn = el('button');
  clearBtn.style.cssText = pauseBtn.style.cssText;
  clearBtn.textContent = '🗑 Clear';
  clearBtn.addEventListener('click', () => { _entries = []; rebuildTable(); });

  const countBadge = el('span', 'devnet-count');
  countBadge.style.cssText = 'color:#475569; font-size:.72rem; min-width:60px; text-align:right;';

  toolbar.appendChild(title);
  toolbar.appendChild(filterInput);
  toolbar.appendChild(pauseBtn);
  toolbar.appendChild(clearBtn);
  toolbar.appendChild(countBadge);

  const thead = el('div', 'devnet-thead');
  thead.style.cssText = `
    display:grid; grid-template-columns: 40px 60px 1fr 80px 80px 160px;
    padding:.3rem .75rem; background:#0f172a;
    border-bottom:1px solid rgba(255,255,255,.05); flex-shrink:0;
    font-size:.68rem; color:#475569; letter-spacing:.06em; text-transform:uppercase;
  `;
  thead.innerHTML = `
    <span>#</span><span>Method</span><span>URL</span>
    <span>Status</span><span>Duration</span><span>Block Rule</span>
  `;

  const tableWrap = el('div', 'devnet-table-wrap');
  tableWrap.style.cssText = 'overflow-y:auto; flex:1;';
  _tbody = el('div', 'devnet-tbody');
  tableWrap.appendChild(_tbody);

  _panel.appendChild(toolbar);
  _panel.appendChild(thead);
  _panel.appendChild(tableWrap);
  document.body.appendChild(_panel);

  rebuildTable();

  setInterval(() => {
    if (countBadge) {
      const visible = _entries.filter(matchesFilter).length;
      countBadge.textContent = `${visible} / ${_entries.length} requests`;
    }
  }, 500);
}

function rebuildTable() {
  if (!_tbody) return;
  _tbody.innerHTML = '';
  _entries.filter(matchesFilter).forEach(e => renderRow(e, false));
}

function renderRow(entry, prepend) {
  if (!_tbody) return;
  if (!matchesFilter(entry)) return;

  const row = el('div', 'devnet-row');
  const isBlocked = entry.status === -1;
  const isError   = entry.status >= 400;
  const isPending = entry.status === 0;

  row.style.cssText = `
    display:grid; grid-template-columns:40px 60px 1fr 80px 80px 160px;
    padding:.25rem .75rem; border-bottom:1px solid rgba(255,255,255,.03);
    font-size:.75rem; line-height:1.5;
    background:${isBlocked ? 'rgba(239,68,68,.06)' : 'transparent'};
    transition:background .1s;
  `;
  row.addEventListener('mouseenter', () => row.style.background = 'rgba(255,255,255,.03)');
  row.addEventListener('mouseleave', () => row.style.background = isBlocked ? 'rgba(239,68,68,.06)' : 'transparent');

  const idx = el('span');
  idx.style.color = '#334155';
  idx.textContent = _entries.indexOf(entry) + 1;

  const method = el('span');
  method.style.cssText = `color:${entry.method === 'GET' ? '#60a5fa' : '#a78bfa'}; font-weight:600;`;
  method.textContent = entry.method ?? 'GET';

  const urlCell = el('span');
  urlCell.style.cssText = 'overflow:hidden; text-overflow:ellipsis; white-space:nowrap; color:#cbd5e1; cursor:pointer;';
  urlCell.title   = entry.url;
  urlCell.textContent = entry.url.replace(/^https?:\/\//, '');
  urlCell.addEventListener('click', () => navigator.clipboard.writeText(entry.url).catch(() => {}));

  const status = el('span');
  status.style.color = isBlocked ? '#f87171'
    : isError   ? '#fb923c'
    : isPending ? '#94a3b8'
    : '#4ade80';
  status.textContent = isBlocked ? 'BLOCKED' : isPending ? '…' : entry.status;

  const dur = el('span');
  dur.style.color = '#64748b';
  dur.textContent = entry.durationMs ? `${entry.durationMs}ms` : '–';

  const rule = el('span');
  rule.style.cssText = 'color:#f87171; font-size:.68rem; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;';
  rule.title   = entry.blockedBy ?? '';
  rule.textContent = entry.blockedBy ?? '';

  row.appendChild(idx);
  row.appendChild(method);
  row.appendChild(urlCell);
  row.appendChild(status);
  row.appendChild(dur);
  row.appendChild(rule);

  if (prepend && _tbody.firstChild) {
    _tbody.insertBefore(row, _tbody.firstChild);
  } else {
    _tbody.appendChild(row);
  }
}

function matchesFilter(entry) {
  if (!_filter) return true;
  return entry.url.toLowerCase().includes(_filter)
    || (entry.blockedBy ?? '').toLowerCase().includes(_filter)
    || String(entry.status).includes(_filter);
}

const _devnetBC = new BroadcastChannel('diatom:devnet');
let _reqCount = 0;

function emitNetEntry(id, url, method, status, durationMs, blockedBy) {
  _devnetBC.postMessage({
    type: 'NET_ENTRY',
    entry: { id, url, method, status, durationMs, blockedBy, ts: Date.now() },
  });
}
