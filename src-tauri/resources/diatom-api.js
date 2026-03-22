/**
 * diatom/src-tauri/resources/diatom-api.js  — v7
 *
 * Injected into every page rendered in the Diatom WebView.
 * Provides fingerprint noise, DOM Crusher rules, Zen intercept,
 * and the window.__diatom API surface.
 *
 * Security: this script runs in the page context.
 *   - It receives data from Rust via __DIATOM_INIT__ (set before injection).
 *   - It posts data to Rust via __TAURI_IPC__ (Tauri's injection point).
 *   - It must never expose the Tauri invoke bridge directly to page JS.
 */

(function() {
  'use strict';

  const _init = window.__DIATOM_INIT__ ?? {};
  const _seed = _init.noise_seed ?? Math.floor(Math.random() * 2**32);

  // ── Fingerprint noise ──────────────────────────────────────────────────────

  // Seeded PRNG (xorshift32)
  let _s = _seed >>> 0 || 1;
  function rand() {
    _s ^= _s << 13; _s ^= _s >>> 17; _s ^= _s << 5;
    return (_s >>> 0) / 2**32;
  }
  function randInRange(lo, hi) { return lo + rand() * (hi - lo); }

  // Canvas noise: perturb pixel data deterministically per seed
  const _origGetContext = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function(type, opts) {
    const ctx = _origGetContext.call(this, type, opts);
    if (!ctx || (type !== '2d' && type !== 'webgl' && type !== 'webgl2')) return ctx;
    if (type === '2d') {
      const _origGetImageData = ctx.getImageData.bind(ctx);
      ctx.getImageData = function(x, y, w, h) {
        const imageData = _origGetImageData(x, y, w, h);
        // Add sub-1-bit noise — visually imperceptible, breaks fingerprinting
        const d = imageData.data;
        for (let i = 0; i < d.length; i += 4) {
          const n = rand() < 0.015 ? 1 : 0;  // ~1.5% of pixels get +1 on one channel
          d[i + ((_s % 3) | 0)] = Math.min(255, d[i + ((_s % 3) | 0)] + n);
        }
        return imageData;
      };
    }
    return ctx;
  };

  // WebGL renderer string noise
  const _origGetParam = WebGLRenderingContext.prototype.getParameter;
  WebGLRenderingContext.prototype.getParameter = function(pname) {
    // RENDERER (0x1F01) and VENDOR (0x1F00) — return plausible spoofed values
    if (pname === 0x1F01) return 'ANGLE (Apple, ANGLE Metal Renderer: Apple M-series, Unspecified Version)';
    if (pname === 0x1F00) return 'WebKit';
    return _origGetParam.call(this, pname);
  };
  if (typeof WebGL2RenderingContext !== 'undefined') {
    WebGL2RenderingContext.prototype.getParameter =
      WebGLRenderingContext.prototype.getParameter;
  }

  // AudioContext noise
  const _origCreateAnalyser = AudioContext.prototype.createAnalyser;
  AudioContext.prototype.createAnalyser = function() {
    const analyser = _origCreateAnalyser.call(this);
    const _origGetFloat32 = analyser.getFloatFrequencyData.bind(analyser);
    analyser.getFloatFrequencyData = function(array) {
      _origGetFloat32(array);
      for (let i = 0; i < array.length; i++) {
        if (rand() < 0.03) array[i] += randInRange(-0.3, 0.3);
      }
    };
    return analyser;
  };

  // Navigator platform/plugins/languages noise
  try {
    Object.defineProperty(navigator, 'plugins', {
      get: () => Object.freeze([]),
    });
    Object.defineProperty(navigator, 'languages', {
      get: () => Object.freeze(['zh-CN', 'zh', 'en-US', 'en']),
    });
  } catch { /* property may be non-configurable */ }

  // ── DOM Crusher (early apply) ──────────────────────────────────────────────

  const _crusherRules = _init.crusher_rules ?? [];
  if (_crusherRules.length) {
    const style = document.createElement('style');
    style.id = 'diatom-crusher-injected';
    const css = _crusherRules.map(s => `${s}{display:none!important}`).join('\n');
    style.textContent = css;
    document.documentElement.appendChild(style);

    // MutationObserver for dynamic content
    const observer = new MutationObserver(() => {
      _crusherRules.forEach(selector => {
        try {
          document.querySelectorAll(selector).forEach(el => {
            if (!el.getAttribute('data-diatom-crushed')) {
              el.style.setProperty('display', 'none', 'important');
              el.setAttribute('data-diatom-crushed', '1');
            }
          });
        } catch { /* invalid selector */ }
      });
    });
    document.addEventListener('DOMContentLoaded', () => {
      observer.observe(document.body, { childList: true, subtree: true });
    }, { once: true });
  }

  // ── Zen mode intercept ────────────────────────────────────────────────────

  if (_init.zen_active && window.__DIATOM_ZEN_BLOCK__) {
    // Page was loaded by SW Zen intercept — signal the Zen module
    // The module handles the full interstitial UI via main.js
    document.addEventListener('DOMContentLoaded', () => {
      window.dispatchEvent(new CustomEvent('diatom:zen_block', {
        detail: window.__DIATOM_ZEN_BLOCK__,
      }));
    }, { once: true });
  }

  // ── Notification suppression (Zen mode) ───────────────────────────────────

  if (_init.zen_active) {
    const _origRequest = Notification.requestPermission?.bind(Notification);
    if (_origRequest) {
      Notification.requestPermission = function() {
        return Promise.resolve('denied');
      };
    }
    window.Notification = new Proxy(window.Notification ?? function() {}, {
      construct: () => ({ close: () => {} }),
    });
  }

  // ── Reading mode detection ────────────────────────────────────────────────

  // Expose setter so reading.js can update the dwell telemetry flag
  let _readingMode = false;
  Object.defineProperty(window, '__diatomReadingMode', {
    set: v => { _readingMode = !!v; },
    get: () => _readingMode,
  });

  // ── Scroll telemetry ──────────────────────────────────────────────────────

  let _lastScrollY  = window.scrollY;
  let _lastScrollTs = Date.now();
  let _scrollVelocity = 0;
  let _tabSwitches  = 0;

  document.addEventListener('scroll', () => {
    const now = Date.now();
    const dy  = Math.abs(window.scrollY - _lastScrollY);
    const dt  = (now - _lastScrollTs) / 1000;
    if (dt > 0) _scrollVelocity = _scrollVelocity * 0.7 + (dy / dt) * 0.3;
    _lastScrollY  = window.scrollY;
    _lastScrollTs = now;
  }, { passive: true });

  document.addEventListener('visibilitychange', () => {
    if (!document.hidden) _tabSwitches++;
  });

  // ── Public API ────────────────────────────────────────────────────────────

  // Minimal surface area — only what page JS should legitimately access
  window.__diatom = Object.freeze({
    version: '7.0.0',

    /** Check if a URL would be blocked. */
    isBlocked: async (url) => {
      // Communicate via BroadcastChannel to SW (no Tauri IPC from page context)
      return false; // placeholder — full implementation in sw.js
    },

    /** Get current reading mode state. */
    get readingMode() { return _readingMode; },

    /** Scroll velocity in px/s (for reading telemetry). */
    get scrollVelocity() { return _scrollVelocity; },

    /** Number of tab switches during current dwell. */
    get tabSwitches() { return _tabSwitches; },
  });

})();
