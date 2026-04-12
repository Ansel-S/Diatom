
'use strict';

const CACHE = (typeof __DIATOM_VERSION__ !== 'undefined')
  ? 'diatom-' + __DIATOM_VERSION__
  : 'diatom-dev-' + Math.floor(Date.now() / 86400000);
const SHELL   = ['/', '/index.html', '/diatom.css', '/main.js', '/sw.js', '/manifest.json'];

let CONFIG = {
  adblock:         true,
  ua_uniformity:   true,
  csp_injection:   true,
  degrade_images:  false,
  image_quality:   0.4,
  image_scale:     0.5,
  decoy_traffic:   false,
  zen_active:      false,
  zen_categories:  ['social', 'entertainment'],
};

let MUSEUM_INDEX = [];

const IDB_NAME        = 'diatom-sw';
const IDB_VERSION     = 1;
const IDB_STORE       = 'kv';
const IDB_KEY_MUSEUM  = 'museum_index';

function idbOpen() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(IDB_NAME, IDB_VERSION);
    req.onupgradeneeded = e => { e.target.result.createObjectStore(IDB_STORE); };
    req.onsuccess  = e => resolve(e.target.result);
    req.onerror    = e => reject(e.target.error);
  });
}

async function idbGet(key) {
  try {
    const db = await idbOpen();
    return await new Promise((resolve, reject) => {
      const tx  = db.transaction(IDB_STORE, 'readonly');
      const req = tx.objectStore(IDB_STORE).get(key);
      req.onsuccess = e => resolve(e.target.result ?? null);
      req.onerror   = e => reject(e.target.error);
    });
  } catch { return null; }
}

async function idbSet(key, value) {
  try {
    const db = await idbOpen();
    await new Promise((resolve, reject) => {
      const tx  = db.transaction(IDB_STORE, 'readwrite');
      const req = tx.objectStore(IDB_STORE).put(value, key);
      req.onsuccess = () => resolve(true);
      req.onerror   = e  => reject(e.target.error);
      tx.onerror    = e  => reject(e.target.error);
      tx.onabort    = () => reject(new DOMException('Transaction aborted', 'AbortError'));
    });
    return true;
  } catch (err) {
    const isQuota = err?.name === 'QuotaExceededError' ||
                    err?.name === 'NS_ERROR_DOM_INDEXEDDB_QUOTA_ERR';
    if (isQuota && key === IDB_KEY_MUSEUM && Array.isArray(value) && value.length > 50) {
      console.warn('[diatom-sw] Quota exceeded — trimming Museum index to 50 entries');
      const trimmed = [...value].sort((a,b) => (b.frozen_at??0)-(a.frozen_at??0)).slice(0,50);
      try {
        const db2 = await idbOpen();
        await new Promise((res, rej) => {
          const tx2 = db2.transaction(IDB_STORE, 'readwrite');
          const r2  = tx2.objectStore(IDB_STORE).put(trimmed, key);
          r2.onsuccess = () => res(true);
          r2.onerror   = e  => rej(e.target.error);
        });
        return true;
    }
    console.warn('[diatom-sw] idbSet failed', { key, errorName: err?.name, message: err?.message });
    return false;
  }
}

async function restoreMuseumIndex() {
  const stored = await idbGet(IDB_KEY_MUSEUM);
  if (Array.isArray(stored) && stored.length > 0) {
    MUSEUM_INDEX = stored;
  }
}

let THREAT_SET   = new Set();
let CRUSHER_RULES = new Map();
let DIATOM_UA    = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/619.1.26 (KHTML, like Gecko) Version/18.0 Safari/619.1.26';

const bc        = new BroadcastChannel('diatom:sw');
const devnetBC  = new BroadcastChannel('diatom:devnet');
let   _reqSeq   = 0;

bc.addEventListener('message', e => {
  const msg = e.data;
  if (!msg?.type) return;
  switch (msg.type) {
    case 'CONFIG':
      Object.assign(CONFIG, msg.config);
      if (msg.config?.synthesised_ua) DIATOM_UA = msg.config.synthesised_ua;
      break;
    case 'ZEN':
      CONFIG.zen_active = !!msg.active;
      break;
    case 'MUSEUM_INDEX':
      MUSEUM_INDEX = msg.index ?? [];
      idbSet(IDB_KEY_MUSEUM, MUSEUM_INDEX).catch(() => {});
      break;
    case 'THREAT_LIST':
      THREAT_SET = new Set(msg.list ?? []);
      break;
    case 'CRUSHER_RULES':
      CRUSHER_RULES.set(msg.domain, msg.selectors ?? []);
      break;
  }
});

const BLOCKED = new Set([
  'doubleclick.net','googlesyndication.com','googletagmanager.com',
  'google-analytics.com','adservice.google.','connect.facebook.net',
  'pixel.facebook.com','hotjar.com','amplitude.com','api.segment.io',
  'cdn.segment.com','mixpanel.com','clarity.ms','fullstory.com',
  'chartbeat.com','parsely.com','quantserve.com','scorecardresearch.com',
  'bugsnag.com','ingest.sentry.io','js-agent.newrelic.com','nr-data.net',
  'adnxs.com','adroll.com','criteo.com','media.net','moatads.com',
  'outbrain.com','pubmatic.com','rubiconproject.com','taboola.com',
  'adsrvr.org','beacon.krxd.net','px.ads.linkedin.com','bat.bing.com',
  'munchkin.marketo.net','js.hs-scripts.com','cdn.heapanalytics.com',
]);

const HEATMAP_SCRIPTS = new Set([
  'static.hotjar.com',
  'script.hotjar.com',
  'vars.hotjar.com',
  'mouseflow.com',
  'cdn.mouseflow.com',
  'rec.smartlook.com',
  'manager.smartlook.com',
  'recordings.lucky-orange.com',
  'lo.lt.lucky-orange.com',
  'cdn.crazyegg.com',
  'heapanalytics.com',
  'cdn.heapanalytics.com',
  'cdn.reamaze.com',       // session replay overlay
  'cdn.contentsquare.net',
  'y.clarity.ms',
]);

const STRIP_PARAMS = new Set([
  '_ga','_gac','_gl','dclid','fbclid','gad_source','gbraid','gclid',
  'gclsrc','igshid','li_fat_id','mc_eid','msclkid','s_kwcid','trk',
  'ttclid','twclid','utm_campaign','utm_content','utm_id','utm_medium',
  'utm_source','utm_term','wbraid','wickedid','yclid',
]);

const STUB_MAP = {
  'google-analytics.com': 'window.ga=function(){};window.gtag=function(){};',
  'googletagmanager.com': 'window.dataLayer=window.dataLayer||[];',
  'hotjar.com':           '(function(h){h.hj=h.hj||function(){(h.hj.q=h.hj.q||[]).push(arguments)}})(window);',
  'connect.facebook.net': '!function(f){f.fbq=function(){};f.fbq.loaded=!0;}(window);',
};

const ZEN_CATEGORIES = {
  social: new Set([
    'twitter.com','x.com','instagram.com','facebook.com','tiktok.com',
    'weibo.com','douyin.com','threads.net','mastodon.social','bluesky.app',
    'reddit.com','discord.com','snapchat.com','linkedin.com','pinterest.com',
  ]),
  entertainment: new Set([
    'youtube.com','bilibili.com','netflix.com','twitch.tv','hulu.com',
    'disneyplus.com','primevideo.com','9gag.com','tumblr.com',
    'buzzfeed.com','dailymotion.com','vimeo.com',
  ]),
};

function hostOf(url) {
  try { return new URL(url).hostname.replace(/^www\./, ''); }
  catch { return ''; }
}

function isBlocked(url) {
  const h = hostOf(url);
  if (!h) return false;
  for (const p of BLOCKED)        { if (h.includes(p)) return true; }
  for (const p of HEATMAP_SCRIPTS){ if (h === p || h.endsWith(`.${p}`)) return true; }
  return false;
}

function isThreat(url) { return THREAT_SET.has(hostOf(url)); }

function zenCategory(url) {
  if (!CONFIG.zen_active) return null;
  const h = hostOf(url);
  for (const cat of CONFIG.zen_categories) {
    const set = ZEN_CATEGORIES[cat];
    if (set && (set.has(h) || [...set].some(d => h.endsWith(`.${d}`)))) return cat;
  }
  return null;
}

function stubFor(url) {
  const h = hostOf(url);
  for (const [pat, stub] of Object.entries(STUB_MAP)) {
    if (h.includes(pat)) return stub;
  }
  return null;
}

function stripParams(url) {
  try {
    const u = new URL(url);
    for (const key of [...u.searchParams.keys()]) {
      if (STRIP_PARAMS.has(key)) u.searchParams.delete(key);
    }
    return u.toString();
  } catch { return url; }
}

function upgradeHttps(url) {
  return url.startsWith('http://') ? url.replace('http://', 'https://') : url;
}

function cleanHeaders(req) {
  const headers = new Headers();
  headers.set('User-Agent', DIATOM_UA);
  headers.set('DNT', '1');
  headers.set('Sec-GPC', '1');
  const accept = req.headers.get('Accept');
  if (accept) headers.set('Accept', accept);
  return headers;
}

function escHtml(s) {
  return String(s ?? '').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}

const TRACKING_PIXEL_RE =
  /<img\b[^>]*\s(?:width=["']?[01]["']?|width=0)[^>]*\s(?:height=["']?[01]["']?|height=0)[^>]*>/gi;

const HIDDEN_IMG_RE =
  /<img\b[^>]*\bstyle=["'][^"']*(?:display\s*:\s*none|visibility\s*:\s*hidden)[^"']*["'][^>]*>/gi;

function stripTrackingPixels(html) {
  return html
    .replace(TRACKING_PIXEL_RE, '<!-- [diatom] tracking pixel removed -->')
    .replace(HIDDEN_IMG_RE,     '<!-- [diatom] hidden img removed -->');
}

const CONSENT_REJECT_SCRIPT = `<script>
(function diatomConsentReject(){
  'use strict';
  try {
    if (typeof __tcfapi === 'function') {
      __tcfapi('setUserDecision', 2, function(){}, { decision: 2 });
    }

    if (typeof OneTrust !== 'undefined' && OneTrust.RejectAll) OneTrust.RejectAll();
    if (typeof OptanonWrapper === 'function') {
      document.cookie = 'OptanonAlertBoxClosed=' + new Date().toISOString();
      document.cookie = 'OptanonConsent=isGpcEnabled=0&datestamp=' + encodeURIComponent(new Date().toUTCString()) + '&version=6.38.0&consentId=diatom&isIABGlobal=false&hosts=&landingPath=NotLandingPage&groups=C0001%3A1&AwaitingReconsent=false';
    }

    if (typeof Cookiebot !== 'undefined') {
      Cookiebot.decline();
    }
    document.cookie = 'CookieConsent={stamp:%27reject%27%2C+necessary:true%2C+preferences:false%2C+statistics:false%2C+marketing:false%2C+method:%27explicit%27%2C+ver:1}; max-age=31536000; SameSite=Lax';

    document.cookie = 'cookieyes-consent=consentid:diatom,consent:no,action:no,necessary:yes,functional:no,analytics:no,performance:no,advertisement:no; max-age=31536000; SameSite=Lax';

    const style = document.createElement('style');
    style.textContent = [
      '#onetrust-consent-sdk','#onetrust-banner-sdk',
      '#cookie-law-info-bar','.cookieconsent','.cookie-consent',
      '#CybotCookiebotDialog','#cookiebot','.cc-banner','.cc-window',
      '#qc-cmp2-container','.qc-cmp2-container',
      '[id*="cookie-banner"],[id*="cookieBanner"],[class*="cookie-banner"]',
      '[id*="consent-banner"],[class*="consent-banner"]',
      '#didomi-notice','.didomi-notice-banner',
      '#axeptio_overlay','.axeptio_overlay',
      '.osano-cm-window','#osano-cm-window',
      '#sp_message_container','.sp_message_container',
    ].map(s => s + '{display:none!important;visibility:hidden!important;opacity:0!important;}').join('\\n');
    (document.head || document.documentElement).appendChild(style);
})();
</script>`;

function injectConsentReject(html) {
  if (html.includes('<head>')) return html.replace('<head>', '<head>' + CONSENT_REJECT_SCRIPT);
  if (html.includes('<body')) return html.replace('<body', CONSENT_REJECT_SCRIPT + '<body');
  return CONSENT_REJECT_SCRIPT + html;
}

const CLIPBOARD_STRIP_SCRIPT = `<script>
(function diatomClipboardStrip(){
  'use strict';
  document.addEventListener('copy', function(e){
    try {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed) return;
      const raw = sel.toString();
      const clean = raw.replace(/[\u200B-\u200F\u2060\u2063\uFEFF\u00AD]/g, '');
      if (clean === raw) return; // nothing to strip
      e.preventDefault();
      e.clipboardData.setData('text/plain', clean);
  }, true);
})();
</script>`;

function injectClipboardStrip(html) {
  if (html.includes('</body>')) return html.replace('</body>', CLIPBOARD_STRIP_SCRIPT + '</body>');
  return html + CLIPBOARD_STRIP_SCRIPT;
}

const SENSOR_SPOOF_SCRIPT = `<script>
(function diatomSensorSpoof(){
  'use strict';
  if (window.__DIATOM_SENSOR_SPOOF__) return;
  window.__DIATOM_SENSOR_SPOOF__ = true;

  if (navigator.getBattery) {
    const fakeBattery = {
      level: 1.0,
      charging: true,
      chargingTime: 0,
      dischargingTime: Infinity,
      addEventListener: function(){},
      removeEventListener: function(){},
    };
    Object.defineProperty(navigator, 'getBattery', {
      value: function() { return Promise.resolve(fakeBattery); },
      writable: false, configurable: false,
    });
  }

  window.addEventListener('devicemotion', function(e) {
    e.stopImmediatePropagation();
  }, true);

  window.addEventListener('deviceorientation', function(e) {
    e.stopImmediatePropagation();
  }, true);

  if (typeof AmbientLightSensor !== 'undefined') {
    window.AmbientLightSensor = class FakeALS {
      get illuminance() { return 500; }
      start() {}
      stop()  {}
      addEventListener()    {}
      removeEventListener() {}
    };
  }
})();
</script>`;

function injectSensorSpoof(html) {
  if (html.includes('<head>')) return html.replace('<head>', '<head>' + SENSOR_SPOOF_SCRIPT);
  return SENSOR_SPOOF_SCRIPT + html;
}

function injectCrusherStyles(html, domain) {
  const selectors = CRUSHER_RULES.get(domain) ?? [];
  if (!selectors.length) return html;
  const css      = selectors.map(s => `${s}{display:none!important}`).join('\n');
  const injected = `<style id="diatom-crusher">\n${css}\n</style>`;
  if (html.includes('<head>')) return html.replace('<head>', `<head>${injected}`);
  if (html.includes('</head>')) return html.replace('</head>', `${injected}</head>`);
  return injected + html;
}

function findArchiveMatches(failedUrl) {
  if (!MUSEUM_INDEX.length) return [];

  let parsedHost = '', parsedPath = '';
  try {
    const u   = new URL(failedUrl);
    parsedHost = u.hostname;
    parsedPath = u.pathname;
  } catch { return []; }

  const tokens = new Set(
    (parsedPath + ' ' + parsedHost)
      .toLowerCase()
      .replace(/[^a-z0-9\u4e00-\u9fa5]+/g, ' ')
      .split(/\s+/)
      .filter(t => t.length > 2),
  );

  return MUSEUM_INDEX
    .map(entry => {
      const tags  = Array.isArray(entry.tfidf_tags)
        ? entry.tfidf_tags : JSON.parse(entry.tfidf_tags ?? '[]');
      const score = tags.filter(t => tokens.has(t.toLowerCase())).length;
      return { ...entry, score };
    })
    .filter(e => e.score > 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, 5);
}

function buildArchiveSuggestionBanner(failedUrl) {
  const matches = findArchiveMatches(failedUrl);
  if (!matches.length) return '';

  const n     = matches.length;
  const items = matches.map(m => {
    const ageDays = m.frozen_at
      ? Math.floor((Date.now() / 1000 - m.frozen_at) / 86400) : 0;
    const ageStr  = ageDays > 0 ? ` · ${ageDays}d ago` : '';
    return `<li>
      <a href="diatom://museum/${escHtml(m.id)}" style="color:#60a5fa;text-decoration:none;font-size:.82rem;">
        ${escHtml(m.title || m.url)}
      </a>
      <span style="color:#475569;font-size:.72rem;">${ageStr}</span>
    </li>`;
  }).join('');

  return `
<aside id="diatom-archive-suggestion" style="
  position:fixed; bottom:1.2rem; right:1.2rem; z-index:99999;
  background:#1e293b; border:1px solid rgba(96,165,250,.22);
  border-radius:.5rem; padding:1rem 1.1rem; max-width:340px;
  font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;
  box-shadow:0 4px 20px rgba(0,0,0,.4);
  color:#94a3b8; font-size:.8rem; line-height:1.5;
">
  <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:.5rem;">
    <span style="color:#60a5fa;font-weight:500;">
      📚 Found ${n} archive(s) in Museum
    </span>
    <button onclick="document.getElementById('diatom-archive-suggestion').remove()"
      style="background:none;border:none;cursor:pointer;color:#475569;font-size:1rem;line-height:1;padding:0 0 0 .5rem;">✕</button>
  </div>
  <ul style="list-style:none;margin:0;padding:0;">${items}</ul>
</aside>`;
}

async function rewriteHtml(response, url) {
  let html = await response.text();

  html = stripTrackingPixels(html);

  html = injectSensorSpoof(html);

  html = injectConsentReject(html);

  html = injectClipboardStrip(html);

  const domain = hostOf(url);
  html = injectCrusherStyles(html, domain);

  return new Response(html, {
    status:  response.status,
    headers: { 'Content-Type': 'text/html; charset=utf-8' },
  });
}

self.addEventListener('install', e => {
  e.waitUntil(
    caches.open(CACHE).then(c => c.addAll(SHELL)).then(() => self.skipWaiting()),
  );
});

self.addEventListener('activate', e => {
  e.waitUntil(
    caches.keys()
      .then(keys => Promise.all(keys.filter(k => k !== CACHE).map(k => caches.delete(k))))
      .then(() => restoreMuseumIndex())
      .then(() => self.clients.claim()),
  );
});

self.addEventListener('fetch', e => {
  const req  = e.request;
  const url  = req.url;
  const mode = req.mode;

  if (SHELL.some(s => url.endsWith(s))) {
    e.respondWith(caches.match(req).then(r => r ?? fetch(req)));
    return;
  }

  if (isThreat(url)) {
    devnetBC.postMessage({ type:'NET_ENTRY', entry:{ id:++_reqSeq, url, method:req.method, status:-1, durationMs:0, blockedBy:'threat:local_list', ts:Date.now() }});
    e.respondWith(threatInterstitial(url));
    return;
  }

  if (CONFIG.adblock && isBlocked(url)) {
    devnetBC.postMessage({ type:'NET_ENTRY', entry:{ id:++_reqSeq, url, method:req.method, status:-1, durationMs:0, blockedBy:'adblock:aho-corasick', ts:Date.now() }});
    const stub = stubFor(url);
    if (stub) {
      e.respondWith(new Response(stub, { headers: { 'Content-Type': 'application/javascript; charset=utf-8' } }));
    } else {
      e.respondWith(new Response('', { status: 204 }));
    }
    return;
  }

  if (mode === 'navigate') {
    const cat = zenCategory(url);
    if (cat) {
    e.respondWith(zenInterstitialResponse(url, cat));
    return;
  }
  }

  if (mode === 'navigate') {
    e.respondWith(handleNavigate(req, url));
    return;
  }

  const clean   = upgradeHttps(stripParams(url));
  const cleaned = new Request(clean, {
    method:  req.method,
    headers: CONFIG.ua_uniformity ? cleanHeaders(req) : req.headers,
    body:    req.method !== 'GET' && req.method !== 'HEAD' ? req.body : undefined,
    mode:    req.mode,
    credentials: 'omit',
    redirect: 'follow',
  });

  const reqId = ++_reqSeq;
  const t0    = Date.now();
  devnetBC.postMessage({ type:'NET_ENTRY', entry:{ id:reqId, url:clean, method:req.method, status:0, durationMs:0, blockedBy:null, ts:t0 }});

  e.respondWith(
    fetch(cleaned).then(resp => {
      devnetBC.postMessage({ type:'NET_ENTRY', entry:{ id:reqId, url:clean, method:req.method, status:resp.status, durationMs:Date.now()-t0, blockedBy:null, ts:t0 }});
      return resp;
    }).catch(() => caches.match(req)),
  );
});

async function handleNavigate(req, url) {
  try {
    const netResp = await fetch(new Request(upgradeHttps(stripParams(url)), {
      headers: CONFIG.ua_uniformity ? cleanHeaders(req) : req.headers,
      credentials: 'omit',
    }));

    if (netResp.ok) {
      const ct = netResp.headers.get('Content-Type') ?? '';
      if (ct.includes('text/html')) {
        return await rewriteHtml(netResp, url);
      }
      return netResp;
    }

  const cached = await caches.match(req);
  if (cached) return cached;

  return offlinePage(url);
}

function zenInterstitialResponse(url, category) {
  const html = `<!DOCTYPE html>
<html><head><meta charset="UTF-8"><title>Zen</title>
<script>window.__DIATOM_ZEN_BLOCK__ = { url: ${JSON.stringify(url)}, category: ${JSON.stringify(category)} };</script>
</head><body><script type="module" src="/main.js"></script></body></html>`;
  return new Response(html, { status: 200, headers: { 'Content-Type': 'text/html; charset=utf-8' } });
}


function threatInterstitial(url) {
  const domain = hostOf(url);
  const html = `<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8"><title>Security Warning</title>
<style>
  body{background:#0a0a10;color:#f87171;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;
       display:flex;align-items:center;justify-content:center;min-height:100vh;text-align:center;}
  .card{max-width:460px;}h1{font-size:1.3rem;margin-bottom:.75rem;}
  p{font-size:.85rem;color:#94a3b8;line-height:1.6;margin-bottom:1rem;}
  code{background:rgba(239,68,68,.1);padding:.2rem .4rem;border-radius:.25rem;}a{color:#60a5fa;}
</style></head>
<body><div class="card">
  <h1>⚠ Threat Intelligence Block</h1>
  <p>Independent threat intelligence flagged <code>${escHtml(domain)}</code>.</p>
  <p><a href="javascript:history.back()">← Go back</a></p>
</div></body></html>`;
  return new Response(html, { status: 200, headers: { 'Content-Type': 'text/html; charset=utf-8' } });
}

function offlinePage(failedUrl) {
  const archiveBanner = buildArchiveSuggestionBanner(failedUrl);

  const html = `<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8"><title>Offline</title>
<style>
  body{background:#0a0a10;color:#475569;
       font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;
       display:flex;align-items:center;justify-content:center;min-height:100vh;
       text-align:center;padding:2rem;}
  p{font-size:1rem;line-height:1.7;max-width:420px;}
</style></head>
<body>
<p>
  This page has not been archived. Use Freeze (⌘⇧S) to save it when online.<br>
  <span style="color:#334155;font-size:.78rem;display:block;margin-top:.5rem;word-break:break-all;">${escHtml(failedUrl.slice(0, 80))}</span>
</p>
${archiveBanner}
</body></html>`;
  return new Response(html, { status: 503, headers: { 'Content-Type': 'text/html; charset=utf-8' } });
}

