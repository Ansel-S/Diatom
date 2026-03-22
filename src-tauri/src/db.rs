// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/db.rs  — v7
//
// Single source-of-truth for SQLite.
// Migration 2 adds: museum_bundles, dom_blocks, war_report_stats,
//                   knowledge_packs, zen_config, dead_man_config.
// All existing columns/tables are untouched.
// ─────────────────────────────────────────────────────────────────────────────

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

// ── Migration list ────────────────────────────────────────────────────────────

const MIGRATIONS: &[(u32, &str)] = &[
    // ── v1: original schema ────────────────────────────────────────────────────
    (1, "
        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS workspaces (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL,
            color      TEXT NOT NULL DEFAULT '#00d4ff',
            is_private INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS history (
            id           TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            url          TEXT NOT NULL,
            title        TEXT NOT NULL DEFAULT '',
            favicon_hex  TEXT,
            visited_at   INTEGER NOT NULL,
            dwell_ms     INTEGER NOT NULL DEFAULT 0,
            visit_count  INTEGER NOT NULL DEFAULT 1
        );
        CREATE UNIQUE INDEX IF NOT EXISTS uq_history   ON history(url, workspace_id);
        CREATE INDEX        IF NOT EXISTS idx_hist_time ON history(visited_at DESC);

        CREATE TABLE IF NOT EXISTS bookmarks (
            id           TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            url          TEXT NOT NULL,
            title        TEXT NOT NULL,
            tags         TEXT NOT NULL DEFAULT '[]',
            ephemeral    INTEGER NOT NULL DEFAULT 0,
            expires_at   INTEGER,
            created_at   INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_bk_workspace ON bookmarks(workspace_id);

        CREATE TABLE IF NOT EXISTS snapshots (
            tab_id     TEXT NOT NULL,
            hash       TEXT NOT NULL,
            text_body  TEXT NOT NULL,
            saved_at   INTEGER NOT NULL,
            PRIMARY KEY (tab_id, hash)
        );

        CREATE TABLE IF NOT EXISTS rss_feeds (
            id               TEXT PRIMARY KEY,
            url              TEXT NOT NULL UNIQUE,
            title            TEXT NOT NULL,
            category         TEXT,
            fetch_interval_m INTEGER NOT NULL DEFAULT 60,
            last_fetched     INTEGER,
            enabled          INTEGER NOT NULL DEFAULT 1,
            added_at         INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS rss_items (
            id          TEXT PRIMARY KEY,
            feed_id     TEXT NOT NULL,
            guid        TEXT NOT NULL,
            title       TEXT NOT NULL,
            url         TEXT NOT NULL,
            summary     TEXT NOT NULL DEFAULT '',
            published   INTEGER,
            read        INTEGER NOT NULL DEFAULT 0,
            fetched_at  INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS uq_rss_item ON rss_items(feed_id, guid);

        CREATE TABLE IF NOT EXISTS privacy_stats (
            week_start    INTEGER PRIMARY KEY,
            block_count   INTEGER NOT NULL DEFAULT 0,
            noise_count   INTEGER NOT NULL DEFAULT 0
        );
    "),

    // ── v2: Diatom v7 additions ────────────────────────────────────────────────
    (2, "
        -- E-WBN frozen page bundles (replaces raw snapshots for new freezes)
        CREATE TABLE IF NOT EXISTS museum_bundles (
            id            TEXT PRIMARY KEY,
            url           TEXT NOT NULL,
            title         TEXT NOT NULL DEFAULT '',
            content_hash  TEXT NOT NULL,   -- BLAKE3 of canonical URL (dedup key)
            bundle_path   TEXT NOT NULL,   -- path relative to data_dir/bundles/
            tfidf_tags    TEXT NOT NULL DEFAULT '[]',  -- JSON array, max 8
            bundle_size   INTEGER NOT NULL DEFAULT 0,
            frozen_at     INTEGER NOT NULL,
            workspace_id  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_bundle_ws   ON museum_bundles(workspace_id);
        CREATE INDEX IF NOT EXISTS idx_bundle_hash ON museum_bundles(content_hash);
        CREATE VIRTUAL TABLE IF NOT EXISTS museum_fts
            USING fts5(tfidf_tags, title, content=museum_bundles, content_rowid=rowid);

        -- Persistent DOM crusher rules (per-domain CSS selectors to nuke)
        CREATE TABLE IF NOT EXISTS dom_blocks (
            id         TEXT PRIMARY KEY,
            domain     TEXT NOT NULL,
            selector   TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            UNIQUE (domain, selector)
        );
        CREATE INDEX IF NOT EXISTS idx_domblocks_domain ON dom_blocks(domain);

        -- Extended privacy/war-report counters (week-level granularity)
        -- Adds columns to existing privacy_stats table
        ALTER TABLE privacy_stats ADD COLUMN tracking_block_count  INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE privacy_stats ADD COLUMN fingerprint_noise_count INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE privacy_stats ADD COLUMN ram_saved_mb           REAL    NOT NULL DEFAULT 0;
        ALTER TABLE privacy_stats ADD COLUMN time_saved_min         REAL    NOT NULL DEFAULT 0;

        -- Offline Knowledge Packs (DocSet / ZIM)
        CREATE TABLE IF NOT EXISTS knowledge_packs (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL,
            format     TEXT NOT NULL CHECK(format IN ('docset','zim')),
            pack_path  TEXT NOT NULL,
            size_bytes INTEGER NOT NULL DEFAULT 0,
            added_at   INTEGER NOT NULL,
            enabled    INTEGER NOT NULL DEFAULT 1
        );

        -- Reading behaviour log (Echo input — raw entries, never persisted after compute)
        -- This table is a ring buffer: max 1000 rows. Older rows are deleted after Echo runs.
        CREATE TABLE IF NOT EXISTS reading_events (
            id              TEXT PRIMARY KEY,
            url             TEXT NOT NULL,
            domain          TEXT NOT NULL,
            dwell_ms        INTEGER NOT NULL DEFAULT 0,
            scroll_px_s     REAL    NOT NULL DEFAULT 0,  -- avg scroll velocity px/s
            reading_mode    INTEGER NOT NULL DEFAULT 0,  -- 1 = reading mode was active
            tab_switches    INTEGER NOT NULL DEFAULT 0,  -- switches away during dwell
            recorded_at     INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_re_time ON reading_events(recorded_at DESC);
    "),
];

// ── Db wrapper ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Db(pub Arc<Mutex<Connection>>);

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .context("open SQLite")?;

        conn.execute_batch("
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous  = NORMAL;
            PRAGMA foreign_keys = ON;
            PRAGMA mmap_size    = 268435456;  -- 256 MB
        ")?;

        let db = Db(Arc::new(Mutex::new(conn)));
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.0.lock().unwrap();
        let version: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap_or(0);

        for (v, sql) in MIGRATIONS {
            if *v > version {
                conn.execute_batch(sql)
                    .with_context(|| format!("migration {v}"))?;
                conn.execute_batch(&format!("PRAGMA user_version = {v}"))?;
            }
        }
        Ok(())
    }

    // ── History helpers ────────────────────────────────────────────────────────

    pub fn upsert_history(
        &self, workspace_id: &str, url: &str, title: &str, dwell_ms: u64,
    ) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO history(id,workspace_id,url,title,visited_at,dwell_ms,visit_count)
             VALUES(?1,?2,?3,?4,?5,?6,1)
             ON CONFLICT(url,workspace_id) DO UPDATE SET
               title      = excluded.title,
               visited_at = excluded.visited_at,
               dwell_ms   = dwell_ms + excluded.dwell_ms,
               visit_count= visit_count + 1",
            params![new_id(), workspace_id, url, title, unix_now(), dwell_ms],
        )?;
        Ok(())
    }

    pub fn search_history(
        &self, workspace_id: &str, query: &str, limit: u32,
    ) -> Result<Vec<HistoryRow>> {
        let conn  = self.0.lock().unwrap();
        let pat   = format!("%{query}%");
        let mut stmt = conn.prepare(
            "SELECT id,url,title,visited_at,dwell_ms,visit_count
             FROM history
             WHERE workspace_id=?1 AND (url LIKE ?2 OR title LIKE ?2)
             ORDER BY visited_at DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![workspace_id, pat, limit], |r| {
            Ok(HistoryRow {
                id: r.get(0)?, url: r.get(1)?, title: r.get(2)?,
                visited_at: r.get(3)?, dwell_ms: r.get(4)?, visit_count: r.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>().context("search_history")
    }

    pub fn clear_history(&self, workspace_id: &str) -> Result<()> {
        self.0.lock().unwrap().execute(
            "DELETE FROM history WHERE workspace_id=?1", [workspace_id],
        )?;
        Ok(())
    }

    // ── Settings helpers ───────────────────────────────────────────────────────

    pub fn get_setting(&self, key: &str) -> Option<String> {
        self.0.lock().unwrap()
            .query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0))
            .ok()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ── War report stats ───────────────────────────────────────────────────────

    pub fn increment_block_count(&self, week_start: i64) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO privacy_stats(week_start,block_count,tracking_block_count)
             VALUES(?1,1,1)
             ON CONFLICT(week_start) DO UPDATE SET
               block_count           = block_count + 1,
               tracking_block_count  = tracking_block_count + 1",
            [week_start],
        )?;
        Ok(())
    }

    pub fn increment_noise_count(&self, week_start: i64, count: i64) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO privacy_stats(week_start,fingerprint_noise_count)
             VALUES(?1,?2)
             ON CONFLICT(week_start) DO UPDATE SET
               fingerprint_noise_count = fingerprint_noise_count + excluded.fingerprint_noise_count",
            params![week_start, count],
        )?;
        Ok(())
    }

    pub fn add_ram_saved(&self, week_start: i64, mb: f64) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO privacy_stats(week_start,ram_saved_mb) VALUES(?1,?2)
             ON CONFLICT(week_start) DO UPDATE SET ram_saved_mb = ram_saved_mb + excluded.ram_saved_mb",
            params![week_start, mb],
        )?;
        Ok(())
    }

    pub fn war_report_week(&self, week_start: i64) -> Result<WarReportRow> {
        self.0.lock().unwrap()
            .query_row(
                "SELECT tracking_block_count,fingerprint_noise_count,ram_saved_mb,time_saved_min
                 FROM privacy_stats WHERE week_start=?1",
                [week_start],
                |r| Ok(WarReportRow {
                    tracking_block_count:   r.get(0)?,
                    fingerprint_noise_count: r.get(1)?,
                    ram_saved_mb:           r.get(2)?,
                    time_saved_min:         r.get(3)?,
                }),
            )
            .context("war_report_week")
    }

    // ── Reading events (Echo input) ────────────────────────────────────────────

    pub fn insert_reading_event(&self, evt: &ReadingEvent) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT OR IGNORE INTO reading_events
             (id,url,domain,dwell_ms,scroll_px_s,reading_mode,tab_switches,recorded_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                evt.id, evt.url, evt.domain, evt.dwell_ms,
                evt.scroll_px_s, evt.reading_mode as i32,
                evt.tab_switches, evt.recorded_at,
            ],
        )?;
        // Enforce 1000-row ring buffer
        self.0.lock().unwrap().execute(
            "DELETE FROM reading_events WHERE id NOT IN (
                SELECT id FROM reading_events ORDER BY recorded_at DESC LIMIT 1000
             )",
            [],
        )?;
        Ok(())
    }

    pub fn reading_events_since(&self, since_unix: i64) -> Result<Vec<ReadingEvent>> {
        let conn  = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,url,domain,dwell_ms,scroll_px_s,reading_mode,tab_switches,recorded_at
             FROM reading_events WHERE recorded_at >= ?1 ORDER BY recorded_at DESC",
        )?;
        let rows = stmt.query_map([since_unix], |r| {
            Ok(ReadingEvent {
                id:           r.get(0)?,
                url:          r.get(1)?,
                domain:       r.get(2)?,
                dwell_ms:     r.get(3)?,
                scroll_px_s:  r.get(4)?,
                reading_mode: r.get::<_,i32>(5)? != 0,
                tab_switches: r.get(6)?,
                recorded_at:  r.get(7)?,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>().context("reading_events_since")
    }

    pub fn purge_reading_events_before(&self, before_unix: i64) -> Result<usize> {
        let n = self.0.lock().unwrap().execute(
            "DELETE FROM reading_events WHERE recorded_at < ?1", [before_unix],
        )?;
        Ok(n)
    }

    // ── Museum bundles ─────────────────────────────────────────────────────────

    pub fn insert_bundle(&self, b: &BundleRow) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT OR REPLACE INTO museum_bundles
             (id,url,title,content_hash,bundle_path,tfidf_tags,bundle_size,frozen_at,workspace_id)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                b.id, b.url, b.title, b.content_hash, b.bundle_path,
                b.tfidf_tags, b.bundle_size, b.frozen_at, b.workspace_id,
            ],
        )?;
        Ok(())
    }

    pub fn list_bundles(&self, workspace_id: &str, limit: u32) -> Result<Vec<BundleRow>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,url,title,content_hash,bundle_path,tfidf_tags,bundle_size,frozen_at,workspace_id
             FROM museum_bundles WHERE workspace_id=?1 ORDER BY frozen_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![workspace_id, limit], |r| {
            Ok(BundleRow {
                id: r.get(0)?, url: r.get(1)?, title: r.get(2)?,
                content_hash: r.get(3)?, bundle_path: r.get(4)?,
                tfidf_tags: r.get(5)?, bundle_size: r.get(6)?,
                frozen_at: r.get(7)?, workspace_id: r.get(8)?,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>().context("list_bundles")
    }

    pub fn search_bundles_fts(&self, query: &str, workspace_id: &str) -> Result<Vec<BundleRow>> {
        let conn = self.0.lock().unwrap();
        // FTS5 query against tags + title
        let mut stmt = conn.prepare(
            "SELECT b.id,b.url,b.title,b.content_hash,b.bundle_path,
                    b.tfidf_tags,b.bundle_size,b.frozen_at,b.workspace_id
             FROM museum_bundles b
             JOIN museum_fts f ON f.rowid = b.rowid
             WHERE museum_fts MATCH ?1 AND b.workspace_id = ?2
             ORDER BY rank LIMIT 10",
        )?;
        let rows = stmt.query_map(params![query, workspace_id], |r| {
            Ok(BundleRow {
                id: r.get(0)?, url: r.get(1)?, title: r.get(2)?,
                content_hash: r.get(3)?, bundle_path: r.get(4)?,
                tfidf_tags: r.get(5)?, bundle_size: r.get(6)?,
                frozen_at: r.get(7)?, workspace_id: r.get(8)?,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>().context("search_bundles_fts")
    }

    pub fn delete_bundle(&self, id: &str) -> Result<()> {
        self.0.lock().unwrap().execute("DELETE FROM museum_bundles WHERE id=?1", [id])?;
        Ok(())
    }

    // ── DOM blocks ─────────────────────────────────────────────────────────────

    pub fn insert_dom_block(&self, domain: &str, selector: &str) -> Result<String> {
        let id = new_id();
        self.0.lock().unwrap().execute(
            "INSERT OR IGNORE INTO dom_blocks(id,domain,selector,created_at) VALUES(?1,?2,?3,?4)",
            params![id, domain, selector, unix_now()],
        )?;
        Ok(id)
    }

    pub fn dom_blocks_for(&self, domain: &str) -> Result<Vec<DomBlock>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,domain,selector,created_at FROM dom_blocks WHERE domain=?1",
        )?;
        let rows = stmt.query_map([domain], |r| {
            Ok(DomBlock { id: r.get(0)?, domain: r.get(1)?, selector: r.get(2)?, created_at: r.get(3)? })
        })?;
        rows.collect::<rusqlite::Result<_>>().context("dom_blocks_for")
    }

    pub fn delete_dom_block(&self, id: &str) -> Result<()> {
        self.0.lock().unwrap().execute("DELETE FROM dom_blocks WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn all_dom_block_domains(&self) -> Result<Vec<String>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare("SELECT DISTINCT domain FROM dom_blocks ORDER BY domain")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>().context("all_dom_block_domains")
    }

    // ── Knowledge packs ────────────────────────────────────────────────────────

    pub fn insert_knowledge_pack(&self, p: &KnowledgePack) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT OR REPLACE INTO knowledge_packs(id,name,format,pack_path,size_bytes,added_at,enabled)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![p.id, p.name, p.format, p.pack_path, p.size_bytes, p.added_at, p.enabled as i32],
        )?;
        Ok(())
    }

    pub fn list_knowledge_packs(&self) -> Result<Vec<KnowledgePack>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,name,format,pack_path,size_bytes,added_at,enabled FROM knowledge_packs ORDER BY added_at DESC"
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(KnowledgePack {
                id: r.get(0)?, name: r.get(1)?, format: r.get(2)?,
                pack_path: r.get(3)?, size_bytes: r.get(4)?,
                added_at: r.get(5)?, enabled: r.get::<_,i32>(6)? != 0,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>().context("list_knowledge_packs")
    }
}

// ── Row types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRow {
    pub id:          String,
    pub url:         String,
    pub title:       String,
    pub visited_at:  i64,
    pub dwell_ms:    i64,
    pub visit_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarReportRow {
    pub tracking_block_count:    i64,
    pub fingerprint_noise_count: i64,
    pub ram_saved_mb:            f64,
    pub time_saved_min:          f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingEvent {
    pub id:           String,
    pub url:          String,
    pub domain:       String,
    pub dwell_ms:     i64,
    pub scroll_px_s:  f64,
    pub reading_mode: bool,
    pub tab_switches: i64,
    pub recorded_at:  i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleRow {
    pub id:           String,
    pub url:          String,
    pub title:        String,
    pub content_hash: String,
    pub bundle_path:  String,
    pub tfidf_tags:   String,  // JSON array
    pub bundle_size:  i64,
    pub frozen_at:    i64,
    pub workspace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomBlock {
    pub id:         String,
    pub domain:     String,
    pub selector:   String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgePack {
    pub id:         String,
    pub name:       String,
    pub format:     String,  // "docset" | "zim"
    pub pack_path:  String,
    pub size_bytes: i64,
    pub added_at:   i64,
    pub enabled:    bool,
}

// ── Utilities ─────────────────────────────────────────────────────────────────

pub fn new_id() -> String { uuid::Uuid::new_v4().to_string() }

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// ISO week start (Monday 00:00 UTC) for the given unix timestamp.
pub fn week_start(ts: i64) -> i64 {
    let day_secs  = ts % (7 * 86_400);
    let week_secs = ts - day_secs;
    // Align to Monday: epoch 1970-01-01 is Thursday = day 3
    let dow = ((ts / 86_400) + 4) % 7; // 0=Mon..6=Sun
    ts - (dow as i64 * 86_400) - (ts % 86_400)
}
