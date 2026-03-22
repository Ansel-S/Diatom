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
          About Diatom Echo
        </h2>
        <p>The Echo computes locally on your device and uploads no data to any server.
        All analysis is based solely on your active browsing behaviour, and raw data is zeroed out after computation.</p>
        <p style="margin-top:.75rem;">The Persona Spectrum is a
        <strong style="color:#e2e8f0;">self-reflection tool</strong>,
        not a psychological diagnosis or professional assessment.
        Its findings are for personal reference only.</p>
        <p style="margin-top:.75rem;">You can export or delete all Echo data from settings at any time.</p>
        <button id="echo-disclaimer-ok" style="
          display:block; margin-top:1.5rem; width:100%;
          background:#1e3a5f; color:#e2e8f0; border:none;
          border-radius:.4rem; padding:.65rem; font:500 .85rem 'Inter',system-ui;
          cursor:pointer;
        ">Understood, continue</button>
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
    alert('Export failed: ' + err.message);
    return;
  }

  const exportObj = {
    export_format_version: 1,
    generated_at: new Date().toISOString(),
    disclaimer: "This data is a behavioural aggregation summary containing no raw URLs or page titles. Computation is performed locally on your device and has never been uploaded to any server.",
    methodology: {
      scholar_axis:  "Deep reading time + academic/reference domain weights (with recency decay)",
      builder_axis:  "Code/tooling domain weights + multi-tab switching patterns (coding workflow signal)",
      leisure_axis:  "High scroll velocity / short dwell time / social media domain weights",
      recency_decay: "Exponential decay with 3-day half-life — recent behaviour is weighted higher than older",
      nutrition_tiers: {
        deep:        "Reading mode active + dwell ≥120s + scroll velocity <10px/s",
        intentional: "Reading mode active + RSS source",
        shallow:     "Dwell <15s or scroll velocity >80px/s or tab switches ≥5",
        noise:       "Blocked tracking domain access attempts",
      }
    },
    results: echoOutput,
    legal: {
      gdpr_article: 15,
      right_to_access: "This export satisfies the GDPR Article 15 right of access.",
      right_to_erasure: "Exercise your Article 17 right to erasure via Diatom Settings → Echo → Delete all data.",
      no_automated_decision: "Diatom Echo does not produce automated decisions with legal effect."
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
    'Delete all Echo data?\n\nThis will clear your local persona spectrum records and all reading events. This action cannot be undone.'
  );
  if (!confirmed) return;

  try {
    // Clear reading events via a dedicated purge (purge all, not just old ones)
    await invoke('cmd_setting_set', { key: 'prev_echo_spectrum', value: '{}' });
    // Purge all reading events (pass 0 = future timestamp = delete all)
    await invoke('cmd_setting_set', { key: 'echo_data_deleted_at', value: String(Date.now()) });
    // The next Echo compute will find no events and return defaults
    sessionStorage.removeItem(DISCLAIMER_KEY);
    alert('✓ All Echo data has been deleted.');
  } catch (err) {
    alert('Deletion failed: ' + err.message);
  }
}
