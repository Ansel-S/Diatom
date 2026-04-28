
export function domainOf(url) {
    try {
        return new URL(url).hostname.replace(/^www\./, '');
    } catch {
        return url;
    }
}

export function isDiatomPage(url) {
    return url.startsWith('diatom://') || url === 'about:blank' || url === '';
}

export function upgradeHttps(url) {
    if (url.startsWith('http://') && !url.startsWith('http://localhost')) {
        return 'https://' + url.slice(7);
    }
    return url;
}

export function diatomPagePath(url) {
    const routes = {
        'diatom://labs':       '/src/ui/labs.html',
        'diatom://about':      '/src/ui/about.html',
        'diatom://newtab':     null,   // handled by #new-tab-page in index.html
        'diatom://settings':   '/src/ui/settings.html',
        'diatom://museum':     '/src/ui/museum.html',
    };
    return routes[url.toLowerCase()] ?? null;
}

export function truncate(str, max = 60) {
    return str.length > max ? str.slice(0, max - 1) + '…' : str;
}

export function formatBytes(bytes) {
    if (bytes < 1024)           return `${bytes} B`;
    if (bytes < 1024 ** 2)      return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 ** 3)      return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
    return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

export function relativeTime(unixSec) {
    const diff = Math.floor(Date.now() / 1000) - unixSec;
    if (diff < 60)     return 'just now';
    if (diff < 3600)   return `${Math.floor(diff / 60)} min ago`;
    if (diff < 86400)  return `${Math.floor(diff / 3600)}h ago`;
    if (diff < 604800) return `${Math.floor(diff / 86400)}d ago`;
    return new Date(unixSec * 1000).toLocaleDateString();
}

export function q(selector, root = document) {
    return root.querySelector(selector);
}

export function qAll(selector, root = document) {
    return Array.from(root.querySelectorAll(selector));
}

export function el(tag, attrs = {}, ...children) {
    const node = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) {
        if (k === 'class') node.className = v;
        else if (k.startsWith('on') && typeof v === 'function') {
            node.addEventListener(k.slice(2), v);
        } else {
            node.setAttribute(k, v);
        }
    }
    for (const child of children.flat()) {
        if (typeof child === 'string') node.append(document.createTextNode(child));
        else if (child instanceof Node) node.append(child);
    }
    return node;
}

export function announce(message) {
    const live = document.getElementById('aria-live');
    if (!live) return;
    live.textContent = '';
    requestAnimationFrame(() => { live.textContent = message; });
}

export function clamp(value, min, max) {
    return Math.min(max, Math.max(min, value));
}

export const PHI = 1.618033988749895;

export function focusZoneCount(tMax) {
    return Math.max(1, Math.floor(tMax / PHI));
}

export function once(target, event, handler) {
    const wrapped = (...args) => {
        target.removeEventListener(event, wrapped);
        handler(...args);
    };
    target.addEventListener(event, wrapped);
}

export function debounce(fn, wait = 200) {
    let timer;
    return (...args) => {
        clearTimeout(timer);
        timer = setTimeout(() => fn(...args), wait);
    };
}

export function throttle(fn, interval = 100) {
    let last = 0;
    return (...args) => {
        const now = Date.now();
        if (now - last >= interval) {
    last = now;
    fn(...args);
  }
    };
}

export function escHtml(s) {
    return String(s ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;')
                          .replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

export const session = {
    get(key) {
        try   { return JSON.parse(sessionStorage.getItem(key)); }
        catch { return null; }
    },
    set(key, val) {
        try { sessionStorage.setItem(key, JSON.stringify(val)); } catch {}
    },
    del(key) {
        try { sessionStorage.removeItem(key); } catch {}
    },
};

// ── Aliases & missing exports (referenced in tabs.js and main.js) ─────────────

/** `qs` is the conventional shorthand for querySelector; alias of `q`. */
export const qs = q;

/** Resolve a raw user input to an absolute URL string. */
export function resolveUrl(raw) {
    const s = raw.trim();
    if (!s) return 'about:blank';
    if (s.startsWith('diatom://') || s.startsWith('about:')) return s;
    try { return new URL(s).href; } catch { /* not an absolute URL */ }
    if (/^[\w-]+\.[\w.-]+/.test(s)) return 'https://' + s;
    return s;   // fall back; caller decides how to handle
}

/** Human-readable relative time string from a Unix timestamp (seconds). */
export const timeAgo = relativeTime;

/** Generate a random unique id string (hex, 8 chars). */
export function uid() {
    return Math.random().toString(16).slice(2, 10);
}
