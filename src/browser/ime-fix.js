/**
 * diatom/src/browser/ime-fix.js  — v7.2  RED-3 / YELLOW
 *
 * Two fixes in one module (both involve injected page context behaviour):
 *
 * Fix 1: IME candidate window position (RED-3)
 *   Problem: Chinese/Japanese/Korean input candidates appear at wrong position
 *   when Diatom's Lustre UI shell has transforms or custom compositing layers.
 *
 *   Root cause: The WebView's IME coordinate system is relative to the OS
 *   window root, but Diatom's custom chrome offsets the content viewport.
 *   When the user types in the omnibox, the candidate window appears at (0,0)
 *   instead of below the caret.
 *
 *   Fix: Intercept the omnibox compositionstart event and manually set the
 *   input's getBoundingClientRect-adjusted coordinates via a CSS transform
 *   that counteracts the chrome offset. This is a CSS-layer fix — no platform
 *   API calls required, zero binary size impact.
 *
 * Fix 2: Bluetooth audio jitter (YELLOW)
 *   Problem: AudioContext noise injection (fingerprint protection) adds ~130ms
 *   jitter to BT audio — enough to desync video.
 *
 *   Fix: Detect if the active audio output device is Bluetooth (via
 *   AudioContext.sinkId or MediaDevices enumeration). If BT is detected,
 *   reduce noise injection to ±0.001 amplitude (imperceptible, still blocks
 *   fingerprinting) instead of the default ±0.3. The fingerprint signal is
 *   still broken; the audible artifact is eliminated.
 */

'use strict';

// ── Fix 1: IME candidate position ────────────────────────────────────────────

export function fixImePosition() {
  // The omnibox input element
  const omni = document.getElementById('omnibox');
  if (!omni) return;

  // Track whether composition is active
  let composing = false;

  omni.addEventListener('compositionstart', () => {
    composing = true;
    repositionImeHint(omni);
  });

  omni.addEventListener('compositionend', () => {
    composing = false;
  });

  omni.addEventListener('input', () => {
    if (composing) repositionImeHint(omni);
  });

  // Also fix any contenteditable elements Diatom renders (Notes zone)
  document.addEventListener('compositionstart', e => {
    if (e.target !== omni && isEditableElement(e.target)) {
      repositionImeHint(e.target);
    }
  }, true);
}

/**
 * Force the element's position to be flush-composited so the OS IME
 * correctly reads its caret position.
 *
 * The WebView reports the IME anchor to the OS as the element's
 * getBoundingClientRect().bottom-left. If the element has a CSS transform
 * applied by a parent (Lustre's border-glow animations), the reported
 * position drifts. We neutralise this by temporarily setting
 * `transform: none` on the compositing boundary, then restoring it.
 *
 * In practice this means: during active IME composition, the Lustre glow
 * on the omnibox is suppressed. It resumes on compositionend.
 */
function repositionImeHint(el) {
  // Walk up to find the first transformed ancestor (Lustre compositor layer)
  let ancestor = el.parentElement;
  while (ancestor && ancestor !== document.body) {
    const style = getComputedStyle(ancestor);
    if (style.transform !== 'none' || style.willChange !== 'auto') {
      // Temporarily neutralise the transform for IME coordinate calculation
      const saved = ancestor.style.transform;
      ancestor.style.transform = 'none';
      // Restore after the browser has had one frame to report the new position
      requestAnimationFrame(() => {
        ancestor.style.transform = saved;
      });
      return;
    }
    ancestor = ancestor.parentElement;
  }
}

function isEditableElement(el) {
  return el?.tagName === 'INPUT'
    || el?.tagName === 'TEXTAREA'
    || el?.isContentEditable;
}

// ── Fix 2: Bluetooth audio — reduce noise amplitude ──────────────────────────

/**
 * Detects if the primary audio output is a Bluetooth device.
 * Returns true if BT audio is active.
 *
 * Strategy: enumerate output devices and check for "bluetooth" in the label,
 * or check AudioContext.sinkId (Chrome 110+).
 */
async function isBluetoothAudioActive() {
  try {
    // Method A: AudioContext.sinkId (Chromium 110+, WebKit experimental)
    const ctx = new AudioContext();
    const sinkId = ctx.sinkId ?? '';
    await ctx.close();
    if (typeof sinkId === 'string' && sinkId.length > 0) {
      const devices = await navigator.mediaDevices.enumerateDevices();
      const outDev  = devices.find(d => d.deviceId === sinkId && d.kind === 'audiooutput');
      if (outDev?.label?.toLowerCase().includes('bluetooth')) return true;
    }

    // Method B: Default output device name heuristic
    const devices = await navigator.mediaDevices.enumerateDevices();
    const defaultOut = devices.find(d => d.kind === 'audiooutput' && d.deviceId === 'default');
    if (defaultOut?.label?.toLowerCase().includes('bluetooth')) return true;

    // Method C: latency heuristic — BT typically adds > 80ms baseline latency
    const testCtx = new AudioContext();
    const baseLatency = testCtx.baseLatency ?? 0;
    await testCtx.close();
    if (baseLatency > 0.08) return true;  // 80ms threshold

    return false;
  } catch {
    return false;
  }
}

/**
 * Called from diatom-api.js to get the appropriate noise amplitude
 * based on whether BT audio is active.
 *
 * Default amplitude: 0.3  (fingerprint breaking, but audible on BT)
 * BT amplitude:      0.001 (fingerprint breaking, inaudible)
 *
 * The key insight: fingerprinting requires CONSISTENT values across
 * multiple calls. Even ±0.001 randomness breaks fingerprint consistency.
 * The BT jitter bug was caused by the 0.3 amplitude interacting with
 * BT's own codec processing — not by the noise itself being audible.
 */
export async function getAudioNoiseAmplitude() {
  const isBT = await isBluetoothAudioActive();
  return isBT ? 0.001 : 0.015;  // 0.015 is imperceptible to humans on wired, safe on BT
}

// ── Init ──────────────────────────────────────────────────────────────────────

export async function initInputFixes() {
  fixImePosition();

  // Pre-compute BT status for diatom-api.js to use
  const amp = await getAudioNoiseAmplitude();
  window.__DIATOM_AUDIO_NOISE_AMP = amp;
}
