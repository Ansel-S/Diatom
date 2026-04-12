
'use strict';

let _workerPaused = false;

class TFIDFEngine {
  #docs  = new Map(); // id → { tf: Map<term,freq>, preview }
  #idf   = new Map();
  #dirty = false;

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
    tokens.forEach(t => tf.set(t, (tf.get(t) ?? 0) + 1 / tokens.length));
    this.#docs.set(id, { tf, preview: text.slice(0, 300) });
    this.#dirty = true;
  }

  remove(id) {
    if (this.#docs.delete(id)) this.#dirty = true;
  }

  #rebuildIdf() {
    if (!this.#dirty) return;
    const N = this.#docs.size;
    if (!N) return;
    const df = new Map();
    for (const { tf } of this.#docs.values()) {
      tf.forEach((_, t) => df.set(t, (df.get(t) ?? 0) + 1));
    }
    this.#idf.clear();
    df.forEach((d, t) => this.#idf.set(t, Math.log((N + 1) / (d + 1)) + 1));
    this.#dirty = false;
  }

  #vec(tf) {
    this.#rebuildIdf();
    const v = new Map();
    let norm = 0;
    tf.forEach((f, t) => {
      const w = f * (this.#idf.get(t) ?? 1);
      v.set(t, w);
      norm += w * w;
    });
    return { v, norm: Math.sqrt(norm) };
  }

  search(query, topK = 8) {
    this.#rebuildIdf();
    const qTokens = this.tokenize(query);
    if (!qTokens.length) return [];
    const qTf = new Map();
    qTokens.forEach(t => qTf.set(t, (qTf.get(t) ?? 0) + 1 / qTokens.length));
    const { v: qVec, norm: qNorm } = this.#vec(qTf);
    if (!qNorm) return [];

    const results = [];
    for (const [id, { tf, preview }] of this.#docs) {
      const { v, norm } = this.#vec(tf);
      if (!norm) continue;
      let dot = 0;
      qVec.forEach((w, t) => { if (v.has(t)) dot += w * v.get(t); });
      const score = dot / (qNorm * norm);
      if (score > 0.01) results.push({ id, score, preview });
    }
    return results.sort((a, b) => b.score - a.score).slice(0, topK);
  }

  topTags(id, n = 8) {
    this.#rebuildIdf();
    const doc = this.#docs.get(id);
    if (!doc) return [];
    const { v } = this.#vec(doc.tf);
    return [...v.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, n)
      .map(([t]) => t);
  }

  clusterGraph() {
    this.#rebuildIdf();
    const entries = [...this.#docs.entries()];
    const vecs    = entries.map(([id, { tf }]) => ({ id, ...this.#vec(tf) }));
    const edges   = [];
    for (let i = 0; i < vecs.length; i++) {
      for (let j = i + 1; j < vecs.length; j++) {
        const { v: va, norm: na } = vecs[i];
        const { v: vb, norm: nb } = vecs[j];
        if (!na || !nb) continue;
        let dot = 0;
        va.forEach((w, t) => { if (vb.has(t)) dot += w * vb.get(t); });
        const sim = dot / (na * nb);
        if (sim > 0.1) edges.push({ a: vecs[i].id, b: vecs[j].id, weight: sim });
      }
    }
    return { nodes: entries.map(([id]) => id), edges };
  }
}

const STOPWORDS = new Set([
  'the','a','an','and','or','in','on','at','to','of','for','is','are','was',
  'were','be','been','have','has','it','its','this','that','with','by','from',
  '的','了','在','是','我','他','她','你','们','这','那','也','就','都',
  'http','https','www','com','net','org',
]);

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

class ReadingBuffer {
  #buf = [];
  #flushTimer = null;

  push(event) {
    this.#buf.push(event);
    if (this.#buf.length >= 10) {
      this.flush();
    } else {
      clearTimeout(this.#flushTimer);
      this.#flushTimer = setTimeout(() => this.flush(), 30_000);
    }
  }

  flush() {
    clearTimeout(this.#flushTimer);
    const events = this.#buf.splice(0);
    if (!events.length) return;
    self.postMessage({ type: 'READING_EVENTS_READY', events });
  }

  drain() {
    const events = this.#buf.splice(0);
    clearTimeout(this.#flushTimer);
    return events;
  }
}

class EchoScheduler {
  #lastIsoWeek = null;

  tick() {
    const now = new Date();
    const iso  = isoWeekStr(now);

    if (this.#lastIsoWeek === null) {
      return false;  // caller populates #lastIsoWeek from OPFS
    }

    if (iso !== this.#lastIsoWeek && now.getDay() === 1) {
      this.#lastIsoWeek = iso;
      return true;
    }
    return false;
  }

  setLastWeek(isoWeek) {
    this.#lastIsoWeek = isoWeek;
  }

  currentIsoWeek() {
    return isoWeekStr(new Date());
  }
}

function isoWeekStr(date) {
  const d   = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()));
  const day = d.getUTCDay() || 7;  // 0=Sun → 7
  d.setUTCDate(d.getUTCDate() + 4 - day);  // nearest Thursday
  const yearStart = new Date(Date.UTC(d.getUTCFullYear(), 0, 1));
  const week = Math.ceil((((d - yearStart) / 86_400_000) + 1) / 7);
  return `${d.getUTCFullYear()}-W${String(week).padStart(2, '0')}`;
}

let museumIndex = [];

class IdleIndexer {
  #queue  = [];   // { id, text, url, title }[]
  #timer  = null;

  enqueue(items) {
    this.#queue.push(...items);
    this.#scheduleSlice();
  }

  #scheduleSlice() {
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => this.#processSlice(), 500);
  }

  #processSlice() {
    if (!this.#queue.length) return;
    const CHUNK = 5;
    const batch = this.#queue.splice(0, CHUNK);
    for (const item of batch) {
      tfidf.index(item.id, `${item.title} ${item.url} ${item.text.slice(0, 2000)}`);
      museumIndex.push({ id: item.id, url: item.url, title: item.title, tfidf_tags: tfidf.topTags(item.id, 8) });
    }
    self.postMessage({ type: 'SW_MUSEUM_SYNC', index: museumIndex });
    self.postMessage({ type: 'INDEX_PROGRESS', remaining: this.#queue.length, processed: batch.length });
    if (this.#queue.length) this.#scheduleSlice();
  }

  resetIdleTimer() {
    clearTimeout(this.#timer);
    if (this.#queue.length) this.#scheduleSlice();
  }
}

const idleIndexer = new IdleIndexer();

const tfidf    = new TFIDFEngine();
const opfs     = new OPFSManager();
const readBuf  = new ReadingBuffer();
const echoSched = new EchoScheduler();

opfs.init().then(async () => {
  const lastWeek = await opfs.read('echo:last_week');
  if (lastWeek) echoSched.setLastWeek(lastWeek);
  else echoSched.setLastWeek(echoSched.currentIsoWeek());

  const idx = await opfs.read('museum:index');
  if (idx) {
    museumIndex = idx;
    self.postMessage({ type: 'SW_MUSEUM_SYNC', index: museumIndex });
  }
});

setInterval(() => {
  if (echoSched.tick()) {
    self.postMessage({ type: 'ECHO_DUE', week: echoSched.currentIsoWeek() });
    opfs.write('echo:last_week', echoSched.currentIsoWeek());
  }
}, 15 * 60 * 1000);

self.addEventListener('message', async ({ data }) => {
  const { id, type, payload } = data;
  let result = null, error = null;

  idleIndexer.resetIdleTimer();

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

      case 'MUSEUM_LOAD_IDLE': {
        museumIndex = [];
        idleIndexer.enqueue(
          (payload.entries ?? []).map(e => ({
            id: e.id, url: e.url, title: e.title,
            text: Array.isArray(e.tfidf_tags)
              ? e.tfidf_tags.join(' ')
              : JSON.parse(e.tfidf_tags ?? '[]').join(' '),
          }))
        );
        result = { queued: payload.entries?.length ?? 0 };
        break;
      }

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

      case 'READING_EVENT':
        readBuf.push(payload);
        break;

      case 'READING_FLUSH': {
        const events = readBuf.drain();
        result = events;
        if (events.length) {
          self.postMessage({ type: 'READING_EVENTS_READY', events });
        }
        break;
      }

      case 'ECHO_SCHEDULE_TICK':
        result = {
          due:  echoSched.tick(),
          week: echoSched.currentIsoWeek(),
        };
        break;

      case 'TFIDF_TAGS_FOR': {
        const tmpId = `__tmp_${Date.now()}`;
        tfidf.index(tmpId, payload.text);
        result = tfidf.topTags(tmpId, payload.n ?? 8);
        tfidf.remove(tmpId);
        break;
      }

      case 'VISIBILITY':
        _workerPaused = !!payload?.hidden;
        if (!_workerPaused) idleIndexer?.resumeIfPaused?.();
        result = { paused: _workerPaused };
        break;

      default:
        error = `unknown type: ${type}`;
    }
  } catch (err) {
    error = err.message;
  }

  self.postMessage({ id, result, error });
});

