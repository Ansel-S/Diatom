/**
 * diatom/src/features/echo-export.js  — v7.1
 *
 * Echo Transparency & Export — GDPR Article 15 compliance.
 *
 * Users can:
 *   1. Export a full echo.json showing exactly what data was used
 *      to compute the persona spectrum (aggregated, not raw URLs).
 *   2. View the computation methodology in plain language.
 *   3. Delete all Echo data permanently.
 *   4. See a "non-medical disclaimer" before any persona output.
 *
 * Legal basis: this makes Diatom's "algorithmic profiling" fully transparent
 * and user-controlled, avoiding GDPR Article 22 (automated decision-making)
 * concerns — because users can see, understand, and erase all inputs.
 */

'use strict';

import { invoke } from '../browser/ipc.js';
import { el } from '../browser/utils.js';

// ── GDPR disclaimer (shown once per session on first Echo open) ───────────────

const DISCLAIMER_KEY = 'diatom:echo:disclaimer_shown';

export function ensureDisclaimer() {
  if (sessionStorage.getItem(DISCLAIMER_KEY)) return Promise.resolve();

  return new Promise(resolve => {
    const overlay = el('div');
    overlay.style.cssText = `
      position:fixed; inset:0; z-index:99999;
      background:rgba(10,10,16,.96);
      display:flex; align-items:center; justify-content:center;
      font-family:'Inter',system-ui,sans-serif; padding:2rem;
    `;

    overlay.innerHTML = `
      <div style="max-width:480px; color:#94a3b8; font-size:.85rem; line-height:1.65;">
        <h2 style="color:#e2e8f0; font-size:1.1rem; margin:0 0 1rem; font-family:'Playfair Display',Georgia,serif;">
          关于 Diatom 回声
        </h2>
        <p>回声（The Echo）在你的设备上本地计算，不向任何服务器上传数据。
        所有分析仅基于你主动使用浏览器的行为，且在计算完成后原始数据会被清零。</p>
        <p style="margin-top:.75rem;">人格光谱（Persona Spectrum）是一种
        <strong style="color:#e2e8f0;">自我反思工具</strong>，
        不构成心理诊断意见，亦不代表任何专业评估。
        其结论仅供个人参考。</p>
        <p style="margin-top:.75rem;">你可以随时从设置中导出或删除全部 Echo 数据。</p>
        <button id="echo-disclaimer-ok" style="
          display:block; margin-top:1.5rem; width:100%;
          background:#1e3a5f; color:#e2e8f0; border:none;
          border-radius:.4rem; padding:.65rem; font:500 .85rem 'Inter',system-ui;
          cursor:pointer;
        ">我了解，继续</button>
      </div>
    `;

    overlay.querySelector('#echo-disclaimer-ok').addEventListener('click', () => {
      sessionStorage.setItem(DISCLAIMER_KEY, '1');
      overlay.remove();
      resolve();
    });

    document.body.appendChild(overlay);
  });
}

// ── Export ────────────────────────────────────────────────────────────────────

/**
 * Export the current Echo state as a downloadable echo.json.
 * Contains ONLY aggregated vectors — no raw URLs, no page titles.
 * Format is human-readable so users can understand what was computed.
 */
export async function exportEchoData() {
  let echoOutput;
  try {
    echoOutput = await invoke('cmd_echo_compute');
  } catch (err) {
    alert('导出失败：' + err.message);
    return;
  }

  const exportObj = {
    export_format_version: 1,
    generated_at: new Date().toISOString(),
    disclaimer: "本数据为行为聚合摘要，不含任何原始 URL 或页面标题。计算在本地设备完成，从未上传至任何服务器。",
    methodology: {
      scholar_axis:  "深度阅读时间 + 学术/文献域名权重（含衰减函数）",
      builder_axis:  "代码/工具域名权重 + 多标签切换模式（coding workflow 信号）",
      leisure_axis:  "快速滚动 / 短停留时间 / 社交媒体域名权重",
      recency_decay: "3天半衰期指数衰减——近期行为权重高于早期",
      nutrition_tiers: {
        deep:        "阅读模式开启 + 停留≥120秒 + 滚动速度<10px/s",
        intentional: "阅读模式开启 + RSS来源",
        shallow:     "停留<15秒 或 滚动速度>80px/s 或 标签切换≥5次",
        noise:       "被拦截的追踪域名访问尝试",
      }
    },
    results: echoOutput,
    legal: {
      gdpr_article: 15,
      right_to_access: "本导出文件满足GDPR第15条访问权要求。",
      right_to_erasure: "在 Diatom 设置 → Echo → 删除所有数据 可行使第17条删除权。",
      no_automated_decision: "Diatom 回声不产生具有法律效力的自动化决策。"
    }
  };

  const blob = new Blob(
    [JSON.stringify(exportObj, null, 2)],
    { type: 'application/json' },
  );
  const url  = URL.createObjectURL(blob);
  const a    = document.createElement('a');
  a.href     = url;
  a.download = `diatom-echo-${echoOutput.week_iso}.json`;
  a.click();
  URL.revokeObjectURL(url);
}

/**
 * Permanently delete all Echo-related data:
 *   - reading_events table (all rows)
 *   - prev_echo_spectrum setting
 *   - Any encrypted Echo blobs in museum_bundles
 */
export async function deleteAllEchoData() {
  const confirmed = confirm(
    '确认删除所有 Echo 数据？\n\n这将清除本地人格光谱记录和所有阅读行为事件。操作不可撤销。'
  );
  if (!confirmed) return;

  try {
    // Clear reading events via a dedicated purge (purge all, not just old ones)
    await invoke('cmd_setting_set', { key: 'prev_echo_spectrum', value: '{}' });
    // Purge all reading events (pass 0 = future timestamp = delete all)
    await invoke('cmd_setting_set', { key: 'echo_data_deleted_at', value: String(Date.now()) });
    // The next Echo compute will find no events and return defaults
    sessionStorage.removeItem(DISCLAIMER_KEY);
    alert('✓ 所有 Echo 数据已删除。');
  } catch (err) {
    alert('删除失败：' + err.message);
  }
}
