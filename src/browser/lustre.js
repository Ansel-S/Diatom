/**
 * diatom/src/browser/lustre.js  — v7.1
 *
 * Lustre: ambient colour engine.
 * Extracts the dominant hue from the active tab's favicon and applies
 * a subtle glow to the browser chrome (address bar edge, tab strip).
 *
 * FIX in v7.1: "muddy colour" bug.
 *   When a favicon has a complex gradient or many competing hues (e.g. a
 *   busy photo-based icon), the naive average produced a murky brown/grey.
 *
 *   Solution: instead of averaging all pixels, we find the MOST SATURATED
 *   cluster of pixels. Desaturated pixels (greys, whites, blacks) are
 *   excluded from the hue calculation. If no sufficiently saturated cluster
 *   exists (e.g. a black/white logo), we fall back to a neutral slate tint
 *   instead of outputting an ugly muddy colour.
 *
 * Algorithm:
 *   1. Downscale favicon to 16×16 (already small, but normalise to this).
 *   2. Convert each pixel to HSL.
 *   3. Discard pixels with saturation < 0.25 or lightness < 0.08 or > 0.92.
 *   4. If fewer than 10 qualified pixels remain → use fallback colour.
 *   5. Bucket remaining hues into 12 × 30° sectors.
 *   6. Pick the dominant sector by pixel count.
 *   7. Average hue within that sector → output as HSL.
 */

'use strict';

import { qs } from './utils.js';

// ── Config ────────────────────────────────────────────────────────────────────

const SATURATION_THRESHOLD = 0.25;    // minimum S to qualify
const LIGHTNESS_MIN        = 0.08;    // exclude near-black
const LIGHTNESS_MAX        = 0.92;    // exclude near-white
const MIN_QUALIFIED_PIXELS = 10;      // if fewer qualified, use fallback
const FALLBACK_HSL         = [220, 18, 22];  // slate — neutral and clean

// ── Public API ────────────────────────────────────────────────────────────────

let _canvas = null;
let _ctx    = null;
let _current = null;   // current applied HSL string

/**
 * Update the Lustre ambient colour for the given favicon URL.
 * Pass null to reset to the fallback (no active tab).
 */
export async function updateLustre(faviconUrl) {
  if (!faviconUrl) {
    applyHsl(FALLBACK_HSL);
    return;
  }

  let hsl;
  try {
    hsl = await extractDominantHsl(faviconUrl);
  } catch {
    hsl = FALLBACK_HSL;
  }
  applyHsl(hsl);
}

export function resetLustre() {
  applyHsl(FALLBACK_HSL);
}

// ── Colour extraction ─────────────────────────────────────────────────────────

async function extractDominantHsl(url) {
  // Load favicon into a 16×16 offscreen canvas
  if (!_canvas) {
    _canvas = document.createElement('canvas');
    _canvas.width = _canvas.height = 16;
    _ctx = _canvas.getContext('2d', { willReadFrequently: true });
  }

  const img = await loadImage(url);
  _ctx.clearRect(0, 0, 16, 16);
  _ctx.drawImage(img, 0, 0, 16, 16);

  const { data } = _ctx.getImageData(0, 0, 16, 16);  // RGBA, 256 values
  const pixels = [];

  for (let i = 0; i < data.length; i += 4) {
    const r = data[i] / 255;
    const g = data[i + 1] / 255;
    const b = data[i + 2] / 255;
    const a = data[i + 3] / 255;
    if (a < 0.5) continue;  // skip transparent pixels

    const [h, s, l] = rgbToHsl(r, g, b);
    if (s >= SATURATION_THRESHOLD && l >= LIGHTNESS_MIN && l <= LIGHTNESS_MAX) {
      pixels.push(h);
    }
  }

  // Not enough saturated pixels → fallback to avoid muddy output
  if (pixels.length < MIN_QUALIFIED_PIXELS) {
    return FALLBACK_HSL;
  }

  // Bucket hues into 12 sectors of 30° each
  const buckets = new Array(12).fill(0);
  for (const h of pixels) {
    buckets[Math.floor(h / 30) % 12]++;
  }

  // Find dominant bucket
  const domIdx   = buckets.indexOf(Math.max(...buckets));
  const domStart = domIdx * 30;
  const domEnd   = domStart + 30;

  // Average hue within the dominant sector
  const inSector = pixels.filter(h => h >= domStart && h < domEnd);
  const avgHue   = inSector.reduce((a, b) => a + b, 0) / inSector.length;

  // Use a tasteful, low-saturation version for the chrome glow
  // (highly saturated chrome glows look gaudy)
  return [Math.round(avgHue), 55, 28];
}

function loadImage(url) {
  return new Promise((resolve, reject) => {
    const img   = new Image();
    img.crossOrigin = 'anonymous';
    img.onload  = () => resolve(img);
    img.onerror = reject;
    img.src     = url;
    // Timeout after 3s
    setTimeout(() => reject(new Error('favicon timeout')), 3000);
  });
}

// ── HSL application ───────────────────────────────────────────────────────────

function applyHsl([h, s, l]) {
  const hslStr = `hsl(${h},${s}%,${l}%)`;
  if (hslStr === _current) return;
  _current = hslStr;

  const root = document.documentElement;
  root.style.setProperty('--lustre-h', String(h));
  root.style.setProperty('--lustre-s', `${s}%`);
  root.style.setProperty('--lustre-l', `${l}%`);
  root.style.setProperty('--lustre', hslStr);

  // Animate transition (100ms ease)
  root.style.setProperty('--lustre-transition', 'color 100ms ease, border-color 100ms ease, box-shadow 100ms ease');
}

// ── Colour math ───────────────────────────────────────────────────────────────

/** Convert RGB (0–1 each) to HSL [h°, s%, l%] */
function rgbToHsl(r, g, b) {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d   = max - min;
  const l   = (max + min) / 2;
  let h = 0, s = 0;

  if (d !== 0) {
    s = d / (1 - Math.abs(2 * l - 1));
    switch (max) {
      case r: h = ((g - b) / d + 6) % 6; break;
      case g: h = (b - r) / d + 2;       break;
      case b: h = (r - g) / d + 4;       break;
    }
    h = h * 60;
  }

  return [h, s, l];
}
