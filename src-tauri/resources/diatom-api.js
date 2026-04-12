
(function () {
  'use strict';

  const _init     = window.__DIATOM_INIT__ || {};
  const _platform = _init.platform || 'macos'; // 'macos' | 'windows' | 'linux'

  const _platformLangs = ['en-US', 'en'];
  try {
    Object.defineProperty(navigator, 'languages', {
      get: () => Object.freeze(_platformLangs),
      configurable: true,
    });
    Object.defineProperty(navigator, 'plugins', {
      get: () => Object.freeze([]),
    });

  if (window.RTCPeerConnection) {
    const _origRTC = window.RTCPeerConnection;
    window.RTCPeerConnection = function (cfg, ...rest) {
      if (cfg && cfg.iceServers) cfg.iceServers = [];
      return new _origRTC(cfg, ...rest);
    };
    Object.setPrototypeOf(window.RTCPeerConnection, _origRTC);
  }

  const _fakeBattery = Object.freeze({
    level:            1.0,
    charging:         true,
    chargingTime:     0,
    dischargingTime:  Infinity,
    addEventListener:    function() {},
    removeEventListener: function() {},
    dispatchEvent:       function() { return false; },
  });

  if (navigator.getBattery) {
    Object.defineProperty(navigator, 'getBattery', {
      value: function () { return Promise.resolve(_fakeBattery); },
      writable: false, configurable: false,
    });
  }

  (function patchMotionAPIs() {
    for (const evtName of ['devicemotion', 'deviceorientation', 'deviceorientationabsolute']) {
      window.addEventListener(evtName, function(e) {
        e.stopImmediatePropagation();
      }, { capture: true, passive: true });
    }

    const SENSOR_CLASSES = [
      'Accelerometer', 'Gyroscope', 'LinearAccelerationSensor',
      'AbsoluteOrientationSensor', 'RelativeOrientationSensor',
      'GravitySensor', 'Magnetometer',
    ];
    for (const cls of SENSOR_CLASSES) {
      if (typeof window[cls] !== 'undefined') {
        const Orig = window[cls];
        window[cls] = class FakeSensor extends Orig {
          get x()          { return 0; }
          get y()          { return 0; }
          get z()          { return 0; }
          get quaternion() { return [0, 0, 0, 1]; }
          start() {}
          stop()  {}
        };
        Object.setPrototypeOf(window[cls].prototype, Orig.prototype);
      }
    }
  })();

  if (typeof AmbientLightSensor !== 'undefined') {
    const _OrigALS = AmbientLightSensor;
    window.AmbientLightSensor = class FakeALS {
      get illuminance() { return 500; }
      start() {}
      stop()  {}
      addEventListener()    {}
      removeEventListener() {}
    };
    Object.setPrototypeOf(window.AmbientLightSensor.prototype, _OrigALS.prototype);
  }

  ['usb', 'hid', 'serial'].forEach(api => {
    if (navigator[api]) {
      try {
        Object.defineProperty(navigator, api, { value: undefined, configurable: false });
    }
  });
  if (navigator.requestMIDIAccess) {
    Object.defineProperty(navigator, 'requestMIDIAccess', {
      value: () => Promise.reject(new DOMException('Blocked by Diatom', 'SecurityError')),
    });
  }

  const _crusherRules = _init.crusher_rules || [];
  if (_crusherRules.length) {
    const style = document.createElement('style');
    style.id = 'diatom-crusher-injected';
    style.textContent = _crusherRules.map(s => `${s}{display:none!important}`).join('\n');
    document.documentElement.appendChild(style);

    const observer = new MutationObserver(() => {
      _crusherRules.forEach(selector => {
        try {
          document.querySelectorAll(selector).forEach(el => {
            if (!el.getAttribute('data-diatom-crushed')) {
              el.style.setProperty('display', 'none', 'important');
              el.setAttribute('data-diatom-crushed', '1');
            }
          });
      });
    });
    document.addEventListener('DOMContentLoaded', () => {
      observer.observe(document.body, { childList: true, subtree: true });
    }, { once: true });
  }

  if (_init.zen_active && window.__DIATOM_ZEN_BLOCK__) {
    document.addEventListener('DOMContentLoaded', () => {
      window.dispatchEvent(new CustomEvent('diatom:zen_block', {
        detail: window.__DIATOM_ZEN_BLOCK__,
      }));
    }, { once: true });
  }

  if (_init.zen_active) {
    const _origRequest = Notification.requestPermission?.bind(Notification);
    if (_origRequest) {
      Notification.requestPermission = () => Promise.resolve('denied');
    }
    window.Notification = new Proxy(window.Notification || function () {}, {
      construct: () => ({ close: () => {} }),
    });
  }

  let _readingMode = false;
  Object.defineProperty(window, '__diatomReadingMode', {
    set: v => { _readingMode = !!v; },
    get: () => _readingMode,
  });

  let _lastScrollY = window.scrollY, _lastScrollTs = Date.now();
  let _scrollVelocity = 0, _tabSwitches = 0;

  document.addEventListener('scroll', () => {
    const now = Date.now(), dy = Math.abs(window.scrollY - _lastScrollY);
    const dt = (now - _lastScrollTs) / 1000;
    if (dt > 0) _scrollVelocity = _scrollVelocity * 0.7 + (dy / dt) * 0.3;
    _lastScrollY = window.scrollY; _lastScrollTs = now;
  }, { passive: true });

  document.addEventListener('visibilitychange', () => {
    if (!document.hidden) _tabSwitches++;
  });

  (function patchTrackingPixels() {
    const HEATMAP_HOSTS = [
      'static.hotjar.com', 'script.hotjar.com', 'vars.hotjar.com',
      'cdn.mouseflow.com', 'mouseflow.com',
      'edge.fullstory.com', 'rs.fullstory.com',
      'cdn.heapanalytics.com', 'heapanalytics.com',
      'www.clarity.ms', 'clarity.ms',
      'cdn.logrocket.io', 'logrocket.io',
      'cdn.lr-ingest.io',
      'script.crazyegg.com', 'crazyegg.com',
      'cdn.inspectlet.com', 'inspectlet.com',
      'cdn.amplitude.com',
      'cdn.segment.com', 'cdn.segment.io',
    ];

    function isHeatmapSrc(src) {
      if (!src) return false;
      try {
        const host = new URL(src).hostname.replace(/^www\./, '');
        return HEATMAP_HOSTS.some(h => host === h || host.endsWith('.' + h));
      } catch { return false; }
    }

    function isTrackingPixel(img) {
      const w = img.getAttribute('width');
      const h = img.getAttribute('height');
      if (w !== null && h !== null) {
        const pw = parseFloat(w), ph = parseFloat(h);
        if (pw <= 1 && ph <= 1 && pw >= 0 && ph >= 0) return true;
      }
      if (img.complete && img.naturalWidth <= 1 && img.naturalHeight <= 1
          && img.naturalWidth > 0) return true;
      const src = img.getAttribute('src') || '';
      if (/[?&](pixel|beacon|track|open|impression|event)=/i.test(src)) return true;
      return false;
    }

    function scrubNode(node) {
      if (node.nodeType !== 1) return;
      if (node.tagName === 'IMG' && isTrackingPixel(node)) {
        node.setAttribute('data-diatom-tp-blocked', '1');
        node.setAttribute('src', 'data:image/gif;base64,R0lGODlhAQABAAD/ACwAAAAAAQABAAACADs=');
        node.style.setProperty('display', 'none', 'important');
        return;
      }
      if (node.tagName === 'SCRIPT') {
        const src = node.getAttribute('src') || '';
        if (isHeatmapSrc(src)) {
          node.setAttribute('data-diatom-hm-blocked', '1');
          node.remove();
          return;
        }
      }
      for (const child of node.children) scrubNode(child);
    }

    const _tpObserver = new MutationObserver(records => {
      for (const rec of records) {
        for (const node of rec.addedNodes) scrubNode(node);
      }
      document.querySelectorAll('img:not([data-diatom-tp-blocked])').forEach(img => {
        if (isTrackingPixel(img)) scrubNode(img);
      });
    });

    _tpObserver.observe(document.documentElement, { childList: true, subtree: true });
  })();

  (function patchSearchFingerprint() {
    const STRIP_EXACT = new Set([
      'utm_source','utm_medium','utm_campaign','utm_term','utm_content',
      'utm_id','utm_source_platform','utm_creative_format','utm_marketing_tactic',
      'gclid','gclsrc','dclid','gbraid','wbraid','_ga','_gl','gad_source',
      'fbclid','fb_action_ids','fb_action_types','fb_source','fb_ref','fbid',
      'msclkid','twclid','s_kwcid','ttclid','sccid','li_fat_id','li_source',
      'epik','_hsenc','_hsmi','hsa_acc','hsa_ad','hsa_cam','hsa_grp','hsa_kw',
      'hsa_la','hsa_mt','hsa_net','hsa_src','hsa_tgt','hsa_ver',
      'mc_eid','mc_cid','vero_id','vero_conv','mkt_tok',
      'iterableEmailCampaignId','iterableTemplateId','iterableMessageId',
      '_kx','sg_uid','sg_mid','psc','smid','oborigurl','tblci',
      'click_id','clickid','icid','ncid','ocid','yclid','wickedid',
      'irclickid','sref','otc','referrer','ref_src','ref_url',
    ]);
    const STRIP_PREFIXES = ['utm_','hsa_','fb_','ga_','iterable'];
    const PROTECTED = new Set([
      'sid','session','session_id','sessionid','auth','token','access_token',
      'id_token','refresh_token','api_key','apikey','key','nonce','state',
      'code','oauth_token','csrf','csrf_token','_token','xsrf_token',
    ]);

    function shouldStrip(name) {
      const lo = name.toLowerCase();
      if (PROTECTED.has(lo)) return false;
      if (STRIP_EXACT.has(lo))  return true;
      return STRIP_PREFIXES.some(p => lo.startsWith(p));
    }

    function cleanUrl(raw) {
      let u;
      try { u = new URL(raw); } catch { return raw; }
      const before = [...u.searchParams.keys()];
      const keep   = before.filter(k => !shouldStrip(k));
      if (keep.length === before.length) return raw; // nothing to do
      const fresh = new URLSearchParams();
      for (const k of keep) u.searchParams.getAll(k).forEach(v => fresh.append(k, v));
      u.search = fresh.toString() ? '?' + fresh.toString() : '';
      return u.toString();
    }

    document.addEventListener('click', e => {
      const a = e.target.closest('a[href]');
      if (!a) return;
      const original = a.href;
      const cleaned  = cleanUrl(original);
      if (cleaned !== original) {
        e.preventDefault();
        const next = a.target && a.target !== '_self' ? a.target : '_self';
        window.open(cleaned, next, 'noopener,noreferrer');
      }
    }, { capture: true, passive: false });

    document.addEventListener('submit', e => {
      const form = e.target;
      if (!form || form.method?.toLowerCase() === 'post') return;
      const action = form.action || location.href;
      try {
        const u = new URL(action, location.href);
        const fd = new FormData(form);
        for (const [k] of fd.entries()) {
          if (shouldStrip(k)) fd.delete(k);
        }
        const params = new URLSearchParams([...fd.entries()].map(([k,v]) => [k, String(v)]));
        u.search = params.toString() ? '?' + params.toString() : '';
        const cleaned = u.toString();
        if (cleaned !== action) {
          e.preventDefault();
          const tgt = form.target || '_self';
          window.open(cleaned, tgt, 'noopener,noreferrer');
        }
    }, { capture: true, passive: false });
  })();

  (function autoDismissCookieBanners() {
    try {
      Object.defineProperty(navigator, 'globalPrivacyControl', {
        get: () => true, configurable: false, enumerable: true,
      });

    if (!window.__tcfapi) {
      const _noConsent = Object.freeze({
        gdprApplies:          true,
        eventStatus:          'useractioncomplete',
        cmpStatus:            'loaded',
        listenerId:           0,
        tcString:             '',
        purpose:              Object.freeze({ consents: {}, legitimateInterests: {} }),
        vendor:               Object.freeze({ consents: {}, legitimateInterests: {} }),
        specialFeatureOptins: Object.freeze({}),
      });
      window.__tcfapi = function _diatomTcfShim(cmd, _ver, cb) {
        if (typeof cb === 'function') cb(_noConsent, true);
      };
      window.__tcfapi._diatom = true;
    }

    const _css = document.createElement('style');
    _css.id = 'diatom-cookie-hider';
    _css.textContent = `
      #onetrust-consent-sdk, #onetrust-banner-sdk, #onetrust-pc-sdk,
      #cookielaw-banner, #cookie-law-info-bar, #cookie-law-info-again,
      #cookie-consent-banner, .cookie-consent-banner,
      #gdpr-cookie-notice, .gdpr-cookie-notice,
      #cookie-banner, .cookie-banner, #cookies-banner, .cookies-banner,
      #cookie-notice, .cookie-notice, #cookie-popup, .cookie-popup,
      #consent-banner, .consent-banner, #consent-popup, .consent-popup,
      .cc-banner, #cc-main, .cc-window,
      .cookiealert, #cookiealert,
      .CookieConsent, #CookieConsent,
      #truste-consent-track, .truste-msgr, #truste-show-consent,
      .qc-cmp2-container, .qc-cmp2-ui,
      #sp-cc, #didomi-host, .didomi-popup-container,
      #usercentrics-root, #uc-center-container,
      .cmp-container, [id^="cmp-"], [class^="cmp-"],
      [id*="cookie-banner"], [id*="cookie-notice"], [id*="cookie-popup"],
      [class*="cookie-banner"], [class*="cookie-notice"], [class*="cookie-popup"],
      [id*="consent-banner"], [class*="consent-banner"],
      [id*="gdpr-banner"], [class*="gdpr-banner"],
      [aria-label*="cookie" i], [aria-label*="consent" i],
      [data-testid*="cookie"], [data-testid*="consent"]
      { display: none !important; visibility: hidden !important;
        opacity: 0 !important; pointer-events: none !important; }
    `;
    (document.head || document.documentElement).appendChild(_css);

    const _REJECT = /\b(reject all|decline all|refuse all|deny all|disagree|no,?\s*thanks?|only necessary|nur notwendige|solo necesarias|refuser tout|ablehnen|\u62d2\u7edd\u5168\u90e8|\u5168\u90e8\u62d2\u7edd|\u4e0d\u540c\u610f|\u4ec5\u9650\u5fc5\u8981)\b/i;
    const _ACCEPT = /\b(accept all|agree|allow all|i accept|consent|ok,?\s*(got it)?|\u540c\u610f|\u63a5\u53d7\u5168\u90e8)\b/i;

    function _textOf(el) {
      return (el.textContent || el.value || el.getAttribute('aria-label') || '').trim();
    }

    function _tryRejectInSubtree(root) {
      if (!root || typeof root.querySelectorAll !== 'function') return;
      const candidates = Array.from(root.querySelectorAll(
        'button, [role="button"], a[href="#"], a[href="javascript:void(0)"], input[type="button"], input[type="submit"]'
      ));
      const rejectBtn = candidates.find(b => _REJECT.test(_textOf(b)));
      if (rejectBtn) { rejectBtn.click(); return; }
      const bannerRoot = (root.closest
        ? root.closest('[id*="cookie"],[id*="consent"],[id*="gdpr"],[class*="cookie"],[class*="consent"],[class*="gdpr"],[role="dialog"],[role="alertdialog"]')
        : null);
      if (bannerRoot) bannerRoot.style.setProperty('display', 'none', 'important');
    }

    const _cmpObserver = new MutationObserver(recs => {
      for (const rec of recs) {
        for (const node of rec.addedNodes) {
          if (node.nodeType !== 1) continue;
          const text = node.textContent || '';
          if (/cookie|consent|gdpr|privacy policy|datenschutz/i.test(text) && _ACCEPT.test(text)) {
            _tryRejectInSubtree(node);
          }
        }
      }
    });
    _cmpObserver.observe(document.documentElement, { childList: true, subtree: true });

    document.addEventListener('DOMContentLoaded', () => {
      _tryRejectInSubtree(document.body);
    }, { once: true });
  })();

  (function patchClipboardCopy() {
    const _WATERMARK_RE = new RegExp(
      '[' +
      '\u00AD\u034F\u2060' +
      '\u061C\u200E\u200F' +
      '\u200B-\u200D' +
      '\u202A-\u202E' +
      '\u17B4\u17B5' +
      '\u115F\u1160\u3164\uFFA0' +
      '\u180B-\u180F' +
      '\uFE00-\uFE0F' +
      '\uFEFF' +
      ']',
      'g'
    );
    const _SURR_RE = /\uDB40[\uDC00-\uDDEF]/g;

    function _stripWatermark(text) {
      if (typeof text !== 'string' || text.length === 0) return text;
      return text.replace(_WATERMARK_RE, '').replace(_SURR_RE, '');
    }

    document.addEventListener('copy', function _diatomCopyGuard(e) {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed) return;
      const raw = sel.toString();
      if (!raw) return;
      const clean = _stripWatermark(raw);
      if (clean !== raw) {
        try {
          e.clipboardData.setData('text/plain', clean);
          e.preventDefault();
      }
    }, { capture: true, passive: false });

    if (navigator.clipboard && typeof navigator.clipboard.writeText === 'function') {
      const _origWrite = navigator.clipboard.writeText.bind(navigator.clipboard);
      try {
        Object.defineProperty(navigator.clipboard, 'writeText', {
          value: function _diatomWriteText(text) {
            return _origWrite(_stripWatermark(text));
          },
          writable: true, configurable: true,
        });
    }
  })();

  (function checkOnionMirror() {
    const _ipc = window.__TAURI_INTERNALS__ || window.__TAURI__?.tauri;
    if (!_ipc) return;
    const host = location.hostname;
    if (!host || host === 'localhost' || host.endsWith('.onion') || host.endsWith('.i2p')) return;
    try {
      _ipc.invoke('cmd_onion_suggest', { host }).then(suggestion => {
        if (!suggestion) return;
        window.dispatchEvent(new CustomEvent('diatom:onion_suggest', { detail: suggestion }));
      }).catch(() => {});
  })();

  window.__diatom_compat_handoff = function (url) {
    if (typeof window.__TAURI_INTERNALS__ !== 'undefined') {
      window.__TAURI_INTERNALS__.invoke('cmd_compat_handoff', { url });
    } else if (typeof window.__TAURI__ !== 'undefined') {
      window.__TAURI__.tauri.invoke('cmd_compat_handoff', { url });
    }
  };

  window.__diatom = Object.freeze({
    version: '0.14.3',
    isBlocked: async (_url) => false,
    get readingMode()    { return _readingMode;    },
    get scrollVelocity() { return _scrollVelocity; },
    get tabSwitches()    { return _tabSwitches;    },
  });

})();

