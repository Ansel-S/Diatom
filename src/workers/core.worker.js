'use strict';

let _workerPaused = false;

// ── Stop-words (English + CJK + URL noise) ──────────────────────────────────
const STOPWORDS = new Set([
  'the','a','an','and','or','in','on','at','to','of','for','is','are','was',
  'were','be','been','have','has','it','its','this','that','with','by','from',
  '的','了','在','是','我','他','她','你','们','这','那','也','就','都',
  'http','https','www','com','net','org',
]);

// ── TF-IDF engine ────────────────────────────────────────────────────────────
// Stores: docs Map<id, {tf, preview}>, idf Map<term, weight>.
// Vectors are computed lazily and cached in #vecCache until IDF is rebuilt.
class TFIDFEngine {
  #docs     = new Map();  // id → { tf: Map<term, freq>, preview: string }
  #idf      = new Map();  // term → idf weight
  #vecCache = new Map();  // id → { v: Map<term, w>, norm: number }
  #dirty    = false;

  tokenize(text) {
    return text
      .toLowerCase()
      .replace(/[^\u4e00-\u9fa5a-z0-9\s]/g, ' ')
      .split(/\s+/)
      .filter(t => t.length > 1 && t.length < 30 && !STOPWORDS.has(t));
  }

  index(id, text) {
    const tokens = this.tokenize(text);
    if (tokens.length < 3) return;
    const tf = new Map();
    for (const t of tokens) tf.set(t, (tf.get(t) ?? 0) + 1 / tokens.length);
    this.#docs.set(id, { tf, preview: text.slice(0, 300) });
    this.#dirty = true;
    this.#vecCache.delete(id);  // invalidate cached vector for this doc
  }

  remove(id) {
    if (this.#docs.delete(id)) {
      this.#dirty = true;
      this.#vecCache.delete(id);
    }
  }

  // Rebuild IDF weights and clear the vector cache.
  // Called at most once per dirty cycle regardless of how many queries follow.
  #rebuildIdf() {
    if (!this.#dirty) return;
    const N = this.#docs.size;
    if (!N) return;

    const df = new Map();
    for (const { tf } of this.#docs.values()) {
      for (const t of tf.keys()) df.set(t, (df.get(t) ?? 0) + 1);
    }
    this.#idf.clear();
    df.forEach((d, t) => this.#idf.set(t, Math.log((N + 1) / (d + 1)) + 1));

    this.#vecCache.clear();  // all cached vectors are now stale
    this.#dirty = false;
  }

  // Build (or return cached) TF-IDF vector for a given TF map.
  // This is the hot path in search(); memoizing it turns search from O(N·T·Q)
  // to O(T·Q + N·|query_terms|) per query.
  #vec(id, tf) {
    this.#rebuildIdf();
    const cached = this.#vecCache.get(id);
    if (cached) return cached;

    const v = new Map();
    let norm = 0;
    for (const [t, f] of tf) {
      const w = f * (this.#idf.get(t) ?? 1);
      v.set(t, w);
      norm += w * w;
    }
    const entry = { v, norm: Math.sqrt(norm) };
    this.#vecCache.set(id, entry);
    return entry;
  }

  // Cosine-similarity search.  Complexity: O(Q · N) where Q = query terms.
  search(query, topK = 8) {
    this.#rebuildIdf();
    const qTokens = this.tokenize(query);
    if (!qTokens.length) return [];

    // Build query TF vector (not cached — queries are one-shot).
    const qTf = new Map();
    for (const t of qTokens) qTf.set(t, (qTf.get(t) ?? 0) + 1 / qTokens.length);

    const qVec = new Map();
    let qNorm = 0;
    for (const [t, f] of qTf) {
      const w = f * (this.#idf.get(t) ?? 1);
      qVec.set(t, w);
      qNorm += w * w;
    }
    qNorm = Math.sqrt(qNorm);
    if (!qNorm) return [];

    const results = [];
    for (const [id, { tf, preview }] of this.#docs) {
      const { v, norm } = this.#vec(id, tf);
      if (!norm) continue;

      // Only iterate over query terms (sparse dot product).
      let dot = 0;
      for (const [t, qw] of qVec) {
        const dw = v.get(t);
        if (dw) dot += qw * dw;
      }
      const score = dot / (qNorm * norm);
      if (score > 0.01) results.push({ id, score, preview });
    }

    results.sort((a, b) => b.score - a.score);
    return results.slice(0, topK);
  }

  topTags(id, n = 8) {
    this.#rebuildIdf();
    const doc = this.#docs.get(id);
    if (!doc) return [];
    const { v } = this.#vec(id, doc.tf);
    return [...v.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, n)
      .map(([t]) => t);
  }

  // Pairwise cosine-similarity graph for clustering visualisation.
  // O(N²): suitable only for small Museum collections (< 500 docs).
  clusterGraph() {
    this.#rebuildIdf();
    const entries = [...this.#docs.entries()];
    const vecs    = entries.map(([id, { tf }]) => ({ id, ...this.#vec(id, tf) }));
    const edges   = [];

    for (let i = 0; i < vecs.length; i++) {
      for (let j = i + 1; j < vecs.length; j++) {
        const { v: va, norm: na } = vecs[i];
        const { v: vb, norm: nb } = vecs[j];
        if (!na || !nb) continue;
        let dot = 0;
        // Iterate over the smaller map for the sparse dot product.
        const [small, large] = va.size <= vb.size ? [va, vb] : [vb, va];
        for (const [t, w] of small) {
          const dw = large.get(t);
          if (dw) dot += w * dw;
        }
        const sim = dot / (na * nb);
        if (sim > 0.1) edges.push({ a: vecs[i].id, b: vecs[j].id, weight: sim });
      }
    }
    return { nodes: entries.map(([id]) => id), edges };
  }
}

// ── OPFS persistence manager ─────────────────────────────────────────────────
class OPFSManager {
  #dir = null;

  async init() {
    try {
      const root = await navigator.storage.getDirectory();
      this.#dir  = await root.getDirectoryHandle('diatom', { create: true });
      return true;
    } catch { return false; }
  }

  async write(key, data) {
    if (!this.#dir) return false;
    try {
      const fh = await this.#dir.getFileHandle(this.#key(key), { create: true });
      const ws = await fh.createWritable();
      await ws.write(JSON.stringify({ ts: Date.now(), data }));
      await ws.close();
      return true;
    } catch { return false; }
  }

  async read(key) {
    if (!this.#dir) return null;
    try {
      const fh   = await this.#dir.getFileHandle(this.#key(key));
      const file = await fh.getFile();
      const text = await file.text();
      return JSON.parse(text).data;
    } catch { return null; }
  }

  async delete(key) {
    try { await this.#dir?.removeEntry(this.#key(key)); } catch {}
  }

  async list() {
    if (!this.#dir) return [];
    const keys = [];
    for await (const name of this.#dir.keys()) keys.push(name.slice(0, -5));
    return keys;
  }

  async clearAll() {
    if (!this.#dir) return;
    for await (const name of this.#dir.keys()) {
      await this.#dir.removeEntry(name).catch(() => {});
    }
  }

  #key(k) { return k.replace(/[^\w-]/g, '_').slice(0, 80) + '.json'; }
}

// ── Idle indexer ─────────────────────────────────────────────────────────────
// Processes queued Museum entries in small time-sliced chunks so indexing
// doesn't block the worker's message loop.
let museumIndex = [];

class IdleIndexer {
  #queue = [];   // { id, text, url, title }[]
  #timer = null;

  enqueue(items) {
    this.#queue.push(...items);
    this.#schedule();
  }

  #schedule() {
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => this.#processSlice(), 500);
  }

  #processSlice() {
    if (!this.#queue.length) return;

    // Process CHUNK items per slice; topTags() triggers one IDF rebuild
    // at the start of the slice (dirty flag), not per item.
    const CHUNK = 5;
    const batch = this.#queue.splice(0, CHUNK);

    for (const item of batch) {
      tfidf.index(item.id, `${item.title} ${item.url} ${item.text.slice(0, 2000)}`);
    }
    // Compute tags after indexing the whole batch — IDF is rebuilt once.
    for (const item of batch) {
      const tags = tfidf.topTags(item.id, 8);
      museumIndex.push({ id: item.id, url: item.url, title: item.title, tfidf_tags: tags });
    }

    self.postMessage({ type: 'SW_MUSEUM_SYNC', index: museumIndex });
    self.postMessage({ type: 'INDEX_PROGRESS', remaining: this.#queue.length, processed: batch.length });

    if (this.#queue.length) this.#schedule();
  }

  resumeIfPaused() {
    if (this.#queue.length) this.#schedule();
  }
}

// ── Singletons ────────────────────────────────────────────────────────────────
const tfidf       = new TFIDFEngine();
const opfs        = new OPFSManager();
const idleIndexer = new IdleIndexer();

opfs.init().then(async () => {
  const idx = await opfs.read('museum:index');
  if (idx) {
    museumIndex = idx;
    self.postMessage({ type: 'SW_MUSEUM_SYNC', index: museumIndex });
  }
});

// ── Message dispatch ──────────────────────────────────────────────────────────
self.addEventListener('message', async ({ data }) => {
  const { id, type, payload } = data;
  let result = null;
  let error  = null;

  idleIndexer.resumeIfPaused();

  try {
    switch (type) {

      case 'INDEX':
        tfidf.index(payload.id, payload.text);
        break;

      case 'REMOVE':
        tfidf.remove(payload.id);
        break;

      case 'SEARCH':
        result = tfidf.search(payload.query, payload.topK);
        break;

      case 'CLUSTER':
        result = tfidf.clusterGraph();
        break;

      case 'OPFS_WRITE':
        result = await opfs.write(payload.key, payload.data);
        break;

      case 'OPFS_READ':
        result = await opfs.read(payload.key);
        break;

      case 'OPFS_DELETE':
        await opfs.delete(payload.key);
        break;

      case 'OPFS_LIST':
        result = await opfs.list();
        break;

      case 'OPFS_CLEAR':
        await opfs.clearAll();
        break;

      case 'INDEX_BUNDLE': {
        const { bundleId, text, url, title } = payload;
        tfidf.index(bundleId, text);
        const tags = tfidf.topTags(bundleId, 8);
        museumIndex = museumIndex.filter(e => e.id !== bundleId);
        museumIndex.push({ id: bundleId, url, title, tfidf_tags: tags });
        await opfs.write('museum:index', museumIndex);
        self.postMessage({ type: 'SW_MUSEUM_SYNC', index: museumIndex });
        result = tags;
        break;
      }

      case 'MUSEUM_REMOVE': {
        tfidf.remove(payload.bundleId);
        museumIndex = museumIndex.filter(e => e.id !== payload.bundleId);
        await opfs.write('museum:index', museumIndex);
        self.postMessage({ type: 'SW_MUSEUM_SYNC', index: museumIndex });
        break;
      }

      // Async/incremental load: queue all entries and return immediately.
      case 'MUSEUM_LOAD_IDLE': {
        museumIndex = [];
        idleIndexer.enqueue(
          (payload.entries ?? []).map(e => ({
            id:    e.id,
            url:   e.url,
            title: e.title,
            text:  Array.isArray(e.tfidf_tags)
              ? e.tfidf_tags.join(' ')
              : JSON.parse(e.tfidf_tags ?? '[]').join(' '),
          }))
        );
        result = { queued: payload.entries?.length ?? 0 };
        break;
      }

      // Synchronous load: index all at once and persist to OPFS.
      case 'MUSEUM_LOAD': {
        for (const entry of (payload.entries ?? [])) {
          const tagText = Array.isArray(entry.tfidf_tags)
            ? entry.tfidf_tags.join(' ')
            : JSON.parse(entry.tfidf_tags ?? '[]').join(' ');
          tfidf.index(entry.id, `${entry.title} ${entry.url} ${tagText}`);
        }
        museumIndex = payload.entries ?? [];
        await opfs.write('museum:index', museumIndex);
        self.postMessage({ type: 'SW_MUSEUM_SYNC', index: museumIndex });
        break;
      }

      case 'TFIDF_TAGS_FOR': {
        const tmpId = `__tmp_${Date.now()}`;
        tfidf.index(tmpId, payload.text);
        result = tfidf.topTags(tmpId, payload.n ?? 8);
        tfidf.remove(tmpId);
        break;
      }

      case 'VISIBILITY':
        _workerPaused = !!payload?.hidden;
        if (!_workerPaused) idleIndexer.resumeIfPaused();
        result = { paused: _workerPaused };
        break;

      default:
        error = `unknown message type: ${type}`;
    }
  } catch (err) {
    error = err.message;
  }

  self.postMessage({ id, result, error });
});
