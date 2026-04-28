
'use strict';

import { qs } from './utils.js';

const SATURATION_THRESHOLD = 0.25;    // minimum S to qualify
const LIGHTNESS_MIN        = 0.08;    // exclude near-black
const LIGHTNESS_MAX        = 0.92;    // exclude near-white
const MIN_QUALIFIED_PIXELS = 10;      // if fewer qualified, use fallback
const FALLBACK_HSL         = [220, 18, 22];  // slate — neutral and clean

let _canvas = null;
let _ctx    = null;
let _current = null;   // current applied HSL string

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

async function extractDominantHsl(url) {
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

  if (pixels.length < MIN_QUALIFIED_PIXELS) {
    return FALLBACK_HSL;
  }

  const buckets = new Array(12).fill(0);
  for (const h of pixels) {
    buckets[Math.floor(h / 30) % 12]++;
  }

  const domIdx   = buckets.indexOf(Math.max(...buckets));
  const domStart = domIdx * 30;
  const domEnd   = domStart + 30;

  const inSector = pixels.filter(h => h >= domStart && h < domEnd);
  const avgHue   = inSector.reduce((a, b) => a + b, 0) / inSector.length;

  return [Math.round(avgHue), 55, 28];
}

function loadImage(url) {
  return new Promise((resolve, reject) => {
    const img   = new Image();
    img.crossOrigin = 'anonymous';
    img.onload  = () => resolve(img);
    img.onerror = reject;
    img.src     = url;
    setTimeout(() => reject(new Error('favicon timeout')), 3000);
  });
}

function applyHsl([h, s, l]) {
  const hslStr = `hsl(${h},${s}%,${l}%)`;
  if (hslStr === _current) return;
  _current = hslStr;

  const root = document.documentElement;
  root.style.setProperty('--lustre-h', String(h));
  root.style.setProperty('--lustre-s', `${s}%`);
  root.style.setProperty('--lustre-l', `${l}%`);
  root.style.setProperty('--lustre', hslStr);

  root.style.setProperty('--lustre-transition', 'color 100ms ease, border-color 100ms ease, box-shadow 100ms ease');
}

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
