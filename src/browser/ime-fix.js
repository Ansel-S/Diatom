
'use strict';

export function fixImePosition() {
  const omni = document.getElementById('omnibox');
  if (!omni) return;

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

  document.addEventListener('compositionstart', e => {
    if (e.target !== omni && isEditableElement(e.target)) {
      repositionImeHint(e.target);
    }
  }, true);
}

function repositionImeHint(el) {
  let ancestor = el.parentElement;
  while (ancestor && ancestor !== document.body) {
    const style = getComputedStyle(ancestor);
    if (style.transform !== 'none' || style.willChange !== 'auto') {
      const saved = ancestor.style.transform;
      ancestor.style.transform = 'none';
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

async function isBluetoothAudioActive() {
  try {
    const ctx = new AudioContext();
    const sinkId = ctx.sinkId ?? '';
    await ctx.close();
    if (typeof sinkId === 'string' && sinkId.length > 0) {
      const devices = await navigator.mediaDevices.enumerateDevices();
      const outDev  = devices.find(d => d.deviceId === sinkId && d.kind === 'audiooutput');
      if (outDev?.label?.toLowerCase().includes('bluetooth')) return true;
    }

    const devices = await navigator.mediaDevices.enumerateDevices();
    const defaultOut = devices.find(d => d.kind === 'audiooutput' && d.deviceId === 'default');
    if (defaultOut?.label?.toLowerCase().includes('bluetooth')) return true;

    const testCtx = new AudioContext();
    const baseLatency = testCtx.baseLatency ?? 0;
    await testCtx.close();
    if (baseLatency > 0.08) return true;  // 80ms threshold

    return false;
  } catch {
    return false;
  }
}

export async function getAudioNoiseAmplitude() {
  const isBT = await isBluetoothAudioActive();
  return isBT ? 0.001 : 0.015;  // 0.015 is imperceptible to humans on wired, safe on BT
}

export async function initInputFixes() {
  fixImePosition();

  const amp = await getAudioNoiseAmplitude();
  window.__DIATOM_AUDIO_NOISE_AMP = amp;
}

