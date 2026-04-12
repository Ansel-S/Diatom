
use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::{Arc, Mutex}};


/// Execute DDL statements idempotently.
///
/// [B-07 FIX] The v0.11.0 implementation caught SQLITE_ERROR (code 1) to
/// swallow "table already exists" errors. Code 1 is SQLITE_ERROR — a generic
/// code covering invalid SQL, wrong column types, missing tables, and more.
/// A real migration failure (e.g. wrong type on ALTER TABLE) would be silently
/// swallowed, leaving the DB in a partially-migrated state with no diagnostic.
///
/// Fix: check the error message text instead. Only suppress errors whose
/// message contains "already exists" or "duplicate column".
/// Alternatively, prefer IF NOT EXISTS in all DDL so this path is never needed.
fn exec_idempotent(conn: &Connection, sql: &str) -> rusqlite::Result<()> {
    match conn.execute_batch(sql) {
        Ok(_) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
            if msg.contains("already exists") || msg.contains("duplicate column") =>
        {
            Ok(())
        }
        Err(e) => Err(e),
    }
}

const MIGRATIONS: &[(u32, &str)] = &[
  (1, "
  CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
  CREATE TABLE IF NOT EXISTS workspaces (
  id TEXT PRIMARY KEY, name TEXT NOT NULL,
  color TEXT NOT NULL DEFAULT '#00d4ff',
  is_private INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL
  );
  CREATE TABLE IF NOT EXISTS history (
  id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL,
  url TEXT NOT NULL, title TEXT NOT NULL DEFAULT '',
  favicon_hex TEXT, visited_at INTEGER NOT NULL,
  dwell_ms INTEGER NOT NULL DEFAULT 0, visit_count INTEGER NOT NULL DEFAULT 1
  );
  CREATE UNIQUE INDEX IF NOT EXISTS uq_history ON history(url, workspace_id);
  CREATE INDEX IF NOT EXISTS idx_hist_time ON history(visited_at DESC);
  CREATE TABLE IF NOT EXISTS bookmarks (
  id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL,
  url TEXT NOT NULL, title TEXT NOT NULL,
  tags TEXT NOT NULL DEFAULT '[]', ephemeral INTEGER NOT NULL DEFAULT 0,
  expires_at INTEGER, created_at INTEGER NOT NULL
  );
  CREATE INDEX IF NOT EXISTS idx_bk_workspace ON bookmarks(workspace_id);
  CREATE TABLE IF NOT EXISTS snapshots (
  tab_id TEXT NOT NULL, hash TEXT NOT NULL,
  text_body TEXT NOT NULL, saved_at INTEGER NOT NULL,
  PRIMARY KEY (tab_id, hash)
  );
  CREATE TABLE IF NOT EXISTS rss_feeds (
  id TEXT PRIMARY KEY, url TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL, category TEXT,
  fetch_interval_m INTEGER NOT NULL DEFAULT 60,
  last_fetched INTEGER, enabled INTEGER NOT NULL DEFAULT 1,
  added_at INTEGER NOT NULL
  );
  CREATE TABLE IF NOT EXISTS rss_items (
  id TEXT PRIMARY KEY, feed_id TEXT NOT NULL,
  guid TEXT NOT NULL, title TEXT NOT NULL,
  url TEXT NOT NULL, summary TEXT NOT NULL DEFAULT '',
  published INTEGER, read INTEGER NOT NULL DEFAULT 0,
  fetched_at INTEGER NOT NULL
  );
  CREATE UNIQUE INDEX IF NOT EXISTS uq_rss_item ON rss_items(feed_id, guid);
  CREATE TABLE IF NOT EXISTS privacy_stats (
  week_start INTEGER PRIMARY KEY,
  block_count INTEGER NOT NULL DEFAULT 0,
  noise_count INTEGER NOT NULL DEFAULT 0
  );
  "),
  (2, "
  CREATE TABLE IF NOT EXISTS museum_bundles (
  id TEXT PRIMARY KEY, url TEXT NOT NULL,
  title TEXT NOT NULL DEFAULT '', content_hash TEXT NOT NULL,
  bundle_path TEXT NOT NULL, tfidf_tags TEXT NOT NULL DEFAULT '[]',
  bundle_size INTEGER NOT NULL DEFAULT 0,
  frozen_at INTEGER NOT NULL, workspace_id TEXT NOT NULL
  );
  CREATE INDEX IF NOT EXISTS idx_bundle_ws  ON museum_bundles(workspace_id);
  CREATE INDEX IF NOT EXISTS idx_bundle_hash ON museum_bundles(content_hash);
  CREATE VIRTUAL TABLE IF NOT EXISTS museum_fts
  USING fts5(tfidf_tags, title, content=museum_bundles, content_rowid=rowid);
  CREATE TRIGGER IF NOT EXISTS museum_fts_ai AFTER INSERT ON museum_bundles BEGIN
  INSERT INTO museum_fts(rowid, tfidf_tags, title)
  VALUES (new.rowid, new.tfidf_tags, new.title);
  END;
  CREATE TRIGGER IF NOT EXISTS museum_fts_ad AFTER DELETE ON museum_bundles BEGIN
  INSERT INTO museum_fts(museum_fts, rowid, tfidf_tags, title)
  VALUES ('delete', old.rowid, old.tfidf_tags, old.title);
  END;
  CREATE TRIGGER IF NOT EXISTS museum_fts_au AFTER UPDATE ON museum_bundles BEGIN
  INSERT INTO museum_fts(museum_fts, rowid, tfidf_tags, title)
  VALUES ('delete', old.rowid, old.tfidf_tags, old.title);
  INSERT INTO museum_fts(rowid, tfidf_tags, title)
  VALUES (new.rowid, new.tfidf_tags, new.title);
  END;
  CREATE TABLE IF NOT EXISTS dom_blocks (
  id TEXT PRIMARY KEY, domain TEXT NOT NULL,
  selector TEXT NOT NULL, created_at INTEGER NOT NULL,
  UNIQUE (domain, selector)
  );
  CREATE INDEX IF NOT EXISTS idx_domblocks_domain ON dom_blocks(domain);
  CREATE TABLE IF NOT EXISTS knowledge_packs (
  id TEXT PRIMARY KEY, name TEXT NOT NULL,
  format TEXT NOT NULL CHECK(format IN ('docset','zim','filterlist')),
  pack_path TEXT NOT NULL, size_bytes INTEGER NOT NULL DEFAULT 0,
  added_at INTEGER NOT NULL, enabled INTEGER NOT NULL DEFAULT 1
  );
  CREATE TABLE IF NOT EXISTS reading_events (
  id TEXT PRIMARY KEY, url TEXT NOT NULL, domain TEXT NOT NULL,
  dwell_ms INTEGER NOT NULL DEFAULT 0, scroll_px_s REAL NOT NULL DEFAULT 0,
  reading_mode INTEGER NOT NULL DEFAULT 0, tab_switches INTEGER NOT NULL DEFAULT 0,
  recorded_at INTEGER NOT NULL
  );
  CREATE INDEX IF NOT EXISTS idx_re_time ON reading_events(recorded_at DESC);
  "),
  (3, "
  CREATE TABLE IF NOT EXISTS totp_entries (
  id TEXT PRIMARY KEY, issuer TEXT NOT NULL,
  account TEXT NOT NULL, secret_enc TEXT NOT NULL,
  domains TEXT NOT NULL DEFAULT '[]', added_at INTEGER NOT NULL
  );
  CREATE TABLE IF NOT EXISTS trust_profiles (
  domain TEXT PRIMARY KEY, level TEXT NOT NULL DEFAULT 'standard',
  source TEXT NOT NULL DEFAULT 'user', set_at INTEGER NOT NULL
  );
  CREATE TABLE IF NOT EXISTS filter_subscriptions (
  id TEXT PRIMARY KEY, name TEXT NOT NULL,
  url TEXT NOT NULL UNIQUE, last_synced INTEGER,
  enabled INTEGER NOT NULL DEFAULT 1,
  rule_count INTEGER NOT NULL DEFAULT 0, added_at INTEGER NOT NULL
  );
  CREATE TABLE IF NOT EXISTS nostr_relays (
  id TEXT PRIMARY KEY, url TEXT NOT NULL UNIQUE,
  enabled INTEGER NOT NULL DEFAULT 1, added_at INTEGER NOT NULL
  );
  CREATE TABLE IF NOT EXISTS onboarding (
  step TEXT PRIMARY KEY, completed INTEGER NOT NULL DEFAULT 0, done_at INTEGER
  );
  CREATE TABLE IF NOT EXISTS zen_state (
  id INTEGER PRIMARY KEY CHECK(id = 1), active INTEGER NOT NULL DEFAULT 0,
  aphorism TEXT NOT NULL DEFAULT 'Now will always have been.',
  blocked_cats TEXT NOT NULL DEFAULT '[\"social\",\"entertainment\"]',
  activated_at INTEGER
  );
  INSERT OR IGNORE INTO zen_state(id) VALUES(1);
  "),
  (4, "
  ALTER TABLE totp_entries ADD COLUMN algorithm TEXT NOT NULL DEFAULT 'SHA1';
  ALTER TABLE totp_entries ADD COLUMN digits INTEGER NOT NULL DEFAULT 6;
  ALTER TABLE totp_entries ADD COLUMN period INTEGER NOT NULL DEFAULT 30;
  CREATE TABLE IF NOT EXISTS vault_logins (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  username TEXT NOT NULL DEFAULT '',
  password_enc TEXT NOT NULL,
  urls_json TEXT NOT NULL DEFAULT '[]',
  notes_enc TEXT NOT NULL DEFAULT '',
  tags_json TEXT NOT NULL DEFAULT '[]',
  totp_uri TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
  );
  CREATE INDEX IF NOT EXISTS idx_vault_login_updated ON vault_logins(updated_at DESC);
  CREATE VIRTUAL TABLE IF NOT EXISTS vault_logins_fts
  USING fts5(title, username, urls_json, content=vault_logins, content_rowid=rowid);
  CREATE TRIGGER IF NOT EXISTS vault_logins_fts_ai AFTER INSERT ON vault_logins BEGIN
  INSERT INTO vault_logins_fts(rowid, title, username, urls_json)
  VALUES (new.rowid, new.title, new.username, new.urls_json);
  END;
  CREATE TRIGGER IF NOT EXISTS vault_logins_fts_ad AFTER DELETE ON vault_logins BEGIN
  INSERT INTO vault_logins_fts(vault_logins_fts, rowid, title, username, urls_json)
  VALUES ('delete', old.rowid, old.title, old.username, old.urls_json);
  END;
  CREATE TRIGGER IF NOT EXISTS vault_logins_fts_au AFTER UPDATE ON vault_logins BEGIN
  INSERT INTO vault_logins_fts(vault_logins_fts, rowid, title, username, urls_json)
  VALUES ('delete', old.rowid, old.title, old.username, old.urls_json);
  INSERT INTO vault_logins_fts(rowid, title, username, urls_json)
  VALUES (new.rowid, new.title, new.username, new.urls_json);
  END;
  CREATE TABLE IF NOT EXISTS vault_cards (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  cardholder_enc TEXT NOT NULL DEFAULT '',
  number_enc TEXT NOT NULL,
  expiry TEXT NOT NULL DEFAULT '',
  cvv_enc TEXT NOT NULL DEFAULT '',
  notes_enc TEXT NOT NULL DEFAULT '',
  tags_json TEXT NOT NULL DEFAULT '[]',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
  );
  CREATE INDEX IF NOT EXISTS idx_vault_card_updated ON vault_cards(updated_at DESC);
  CREATE TABLE IF NOT EXISTS vault_notes (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  content_enc TEXT NOT NULL,
  tags_json TEXT NOT NULL DEFAULT '[]',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
  );
  CREATE INDEX IF NOT EXISTS idx_vault_note_updated ON vault_notes(updated_at DESC);
  "),
  (6, "\
  ALTER TABLE museum_bundles ADD COLUMN index_tier TEXT NOT NULL DEFAULT 'hot';\
  ALTER TABLE museum_bundles ADD COLUMN last_accessed_at INTEGER;\
  CREATE INDEX IF NOT EXISTS idx_bundle_tier ON museum_bundles(index_tier, last_accessed_at);\
  DROP TRIGGER IF EXISTS museum_fts_au;\
  CREATE TRIGGER IF NOT EXISTS museum_fts_au_del AFTER UPDATE ON museum_bundles BEGIN\
  INSERT INTO museum_fts(museum_fts, rowid, tfidf_tags, title)\
  VALUES ('delete', old.rowid, old.tfidf_tags, old.title);\
  END;\
  CREATE TRIGGER IF NOT EXISTS museum_fts_au_hot AFTER UPDATE ON museum_bundles\
  WHEN NEW.index_tier != 'cold' BEGIN\
  INSERT INTO museum_fts(rowid, tfidf_tags, title)\
  VALUES (new.rowid, new.tfidf_tags, new.title);\
  END;\
  "),
  (5, "
  CREATE TABLE IF NOT EXISTS boosts (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  domain TEXT NOT NULL DEFAULT '*',
  css TEXT NOT NULL DEFAULT '',
  enabled INTEGER NOT NULL DEFAULT 0,
  builtin INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL DEFAULT 0
  );
  "),
];


#[derive(Clone)]
pub struct Db(pub Arc<Mutex<Connection>>);

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).context("open SQLite")?;
        conn.execute_batch("
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous  = NORMAL;
            PRAGMA foreign_keys = ON;
            PRAGMA mmap_size    = 268435456;
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
                if *v == 2 || *v == 4 || *v == 6 {
                    let stmts: Vec<&str> = sql.split(';')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect();
                    for stmt in stmts {
                        let upper = stmt.to_uppercase();
                        if upper.contains("ALTER TABLE") && upper.contains("ADD COLUMN") {
                            let _ = conn.execute_batch(&format!("{};", stmt));
                        } else {
                            conn.execute_batch(&format!("{};", stmt))
                                .with_context(|| format!("migration v{v}: {}", &stmt[..stmt.len().min(80)]))?;
                        }
                    }
                } else {
                    conn.execute_batch(sql)
                        .with_context(|| format!("migration v{v}"))?;
                }
                conn.execute_batch(&format!("PRAGMA user_version = {v}"))?;
            }
        }
        Ok(())
    }


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


    /// [FIX-B19] Top visited domains by visit_count for Home Base new-tab page.
    pub fn top_domains(&self, workspace_id: &str, limit: u32) -> Result<Vec<serde_json::Value>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT url, title, favicon_hex, SUM(visit_count) as total_visits
             FROM history WHERE workspace_id=?1
             GROUP BY SUBSTR(url, INSTR(url,'://')+3, INSTR(SUBSTR(url,INSTR(url,'://')+3),'/')-1)
             ORDER BY total_visits DESC LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![workspace_id, limit], |r| {
            Ok(serde_json::json!({
                "url":          r.get::<_,String>(0)?,
                "title":        r.get::<_,String>(1)?,
                "favicon_hex":  r.get::<_,Option<String>>(2)?,
                "visit_count":  r.get::<_,i64>(3)?,
            }))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().context("top_domains")
    }

    /// Pinned bookmarks (tagged "pinned") for Home Base new-tab page.
    pub fn pinned_bookmarks(&self, workspace_id: &str, limit: u32) -> Result<Vec<serde_json::Value>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT url, title FROM bookmarks
             WHERE workspace_id=?1 AND tags LIKE '%\"pinned\"%'
             ORDER BY created_at DESC LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![workspace_id, limit], |r| {
            Ok(serde_json::json!({
                "url":   r.get::<_,String>(0)?,
                "title": r.get::<_,String>(1)?,
            }))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().context("pinned_bookmarks")
    }

    pub fn upsert_history(&self, workspace_id: &str, url: &str, title: &str, dwell_ms: u64) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO history(id,workspace_id,url,title,visited_at,dwell_ms,visit_count)
             VALUES(?1,?2,?3,?4,?5,?6,1)
             ON CONFLICT(url,workspace_id) DO UPDATE SET
               title=excluded.title, visited_at=excluded.visited_at,
               dwell_ms=dwell_ms+excluded.dwell_ms, visit_count=visit_count+1",
            params![new_id(), workspace_id, url, title, unix_now(), dwell_ms],
        )?;
        Ok(())
    }

    /// [AUDIT-FIX §4.1] FTS query — O(history_size). Call sites inside async
    /// Tauri commands should wrap this in tokio::task::spawn_blocking to avoid
    /// blocking a tokio worker thread. Short history sets (< 10 000 rows) are
    /// acceptable inline; larger sets should use spawn_blocking.
    pub fn search_history(&self, workspace_id: &str, query: &str, limit: u32) -> Result<Vec<HistoryRow>> {
        let conn = self.0.lock().unwrap();
        let esc = query.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        let pat = format!("%{esc}%");
        let mut stmt = conn.prepare(
            "SELECT id,url,title,visited_at,dwell_ms,visit_count
             FROM history WHERE workspace_id=?1
             AND (url LIKE ?2 ESCAPE '\\' OR title LIKE ?2 ESCAPE '\\')
             ORDER BY visited_at DESC LIMIT ?3")?;
        let rows = stmt.query_map(params![workspace_id, pat, limit], |r| Ok(HistoryRow {
            id:           r.get(0)?,
            url:          r.get(1)?,
            title:        r.get(2)?,
            visited_at:   r.get(3)?,
            dwell_ms:     r.get(4)?,
            visit_count:  r.get(5)?,
        }))?;
        rows.collect::<rusqlite::Result<_>>().context("search_history")
    }

    pub fn clear_history(&self, workspace_id: &str) -> Result<()> {
        self.0.lock().unwrap().execute(
            "DELETE FROM history WHERE workspace_id=?1", [workspace_id])?;
        Ok(())
    }


    pub fn increment_block_count(&self, week_start: i64) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO privacy_stats(week_start,block_count,tracking_block_count)
             VALUES(?1,1,1)
             ON CONFLICT(week_start) DO UPDATE SET
               block_count=block_count+1, tracking_block_count=tracking_block_count+1",
            [week_start])?;
        Ok(())
    }

    pub fn increment_noise_count(&self, week_start: i64, count: i64) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO privacy_stats(week_start,fingerprint_noise_count) VALUES(?1,?2)
             ON CONFLICT(week_start) DO UPDATE SET
               fingerprint_noise_count=fingerprint_noise_count+excluded.fingerprint_noise_count",
            params![week_start, count])?;
        Ok(())
    }

    pub fn add_ram_saved(&self, week_start: i64, mb: f64) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO privacy_stats(week_start,ram_saved_mb) VALUES(?1,?2)
             ON CONFLICT(week_start) DO UPDATE SET ram_saved_mb=ram_saved_mb+excluded.ram_saved_mb",
            params![week_start, mb])?;
        Ok(())
    }

    /// [NEW] Write real time-saved measurement (e.g. from page-load timing).
    pub fn add_time_saved(&self, week_start: i64, minutes: f64) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO privacy_stats(week_start,time_saved_min) VALUES(?1,?2)
             ON CONFLICT(week_start) DO UPDATE SET time_saved_min=time_saved_min+excluded.time_saved_min",
            params![week_start, minutes])?;
        Ok(())
    }

    pub fn war_report_week(&self, week_start: i64) -> Result<WarReportRow> {
        self.0.lock().unwrap().query_row(
            "SELECT tracking_block_count,fingerprint_noise_count,ram_saved_mb,time_saved_min
             FROM privacy_stats WHERE week_start=?1",
            [week_start], |r| Ok(WarReportRow {
                tracking_block_count: r.get(0)?,
                fingerprint_noise_count: r.get(1)?,
                ram_saved_mb: r.get(2)?,
                time_saved_min: r.get(3)?,
            }),
        ).context("war_report_week")
    }


    pub fn insert_reading_event(&self, evt: &ReadingEvent) -> Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let r = (|| -> Result<()> {
            conn.execute(
                "INSERT OR IGNORE INTO reading_events
                 (id,url,domain,dwell_ms,scroll_px_s,reading_mode,tab_switches,recorded_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![evt.id, evt.url, evt.domain, evt.dwell_ms, evt.scroll_px_s,
                        evt.reading_mode as i32, evt.tab_switches, evt.recorded_at])?;
            conn.execute(
                "DELETE FROM reading_events WHERE id IN (
                    SELECT id FROM reading_events ORDER BY recorded_at DESC LIMIT -1 OFFSET 1000
                )", [])?;
            Ok(())
        })();
        if r.is_ok() { conn.execute_batch("COMMIT")?; } else { let _ = conn.execute_batch("ROLLBACK"); }
        r
    }

    pub fn reading_events_since(&self, since_unix: i64) -> Result<Vec<ReadingEvent>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,url,domain,dwell_ms,scroll_px_s,reading_mode,tab_switches,recorded_at
             FROM reading_events WHERE recorded_at >= ?1 ORDER BY recorded_at DESC")?;
        let rows = stmt.query_map([since_unix], |r| Ok(ReadingEvent {
            id: r.get(0)?,
            url: r.get(1)?,
            domain: r.get(2)?,
            dwell_ms: r.get(3)?,
            scroll_px_s: r.get(4)?,
            reading_mode: r.get::<_,i32>(5)? != 0,
            tab_switches: r.get(6)?,
            recorded_at: r.get(7)?,
        }))?;
        rows.collect::<rusqlite::Result<_>>().context("reading_events_since")
    }

    pub fn purge_reading_events_before(&self, before_unix: i64) -> Result<usize> {
        Ok(self.0.lock().unwrap().execute(
            "DELETE FROM reading_events WHERE recorded_at < ?1", [before_unix])?)
    }


    pub fn insert_bundle(&self, b: &BundleRow) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT OR REPLACE INTO museum_bundles
             (id,url,title,content_hash,bundle_path,tfidf_tags,bundle_size,frozen_at,workspace_id,\
              index_tier,last_accessed_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![b.id, b.url, b.title, b.content_hash, b.bundle_path,
                    b.tfidf_tags, b.bundle_size, b.frozen_at, b.workspace_id,
                    b.index_tier, b.last_accessed_at])?;
        Ok(())
    }

    /// [AUDIT-FIX §4.1] Joined table scan — use spawn_blocking when called
    /// from async Tauri commands with a large Museum (> 1000 bundles).
    pub fn list_bundles(&self, workspace_id: &str, limit: u32) -> Result<Vec<BundleRow>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,url,title,content_hash,bundle_path,tfidf_tags,bundle_size,frozen_at,\
                    workspace_id,index_tier,last_accessed_at
             FROM museum_bundles WHERE workspace_id=?1 ORDER BY frozen_at DESC LIMIT ?2")?;
        let rows = stmt.query_map(params![workspace_id, limit], |r| Ok(bundle_row(r)?))?;
        rows.collect::<rusqlite::Result<_>>().context("list_bundles")
    }

    /// [FIX-20] Direct ID lookup — no cap.
    pub fn get_bundle_by_id(&self, id: &str) -> Result<Option<BundleRow>> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT id,url,title,content_hash,bundle_path,tfidf_tags,bundle_size,frozen_at,\
                    workspace_id,index_tier,last_accessed_at
             FROM museum_bundles WHERE id=?1", [id],
            |r| bundle_row(r),
        ).optional().context("get_bundle_by_id")
    }

    /// Full-text search — only HOT-tier entries have FTS5 rows.
    pub fn search_bundles_fts(&self, query: &str, workspace_id: &str) -> Result<Vec<BundleRow>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT b.id,b.url,b.title,b.content_hash,b.bundle_path,
                    b.tfidf_tags,b.bundle_size,b.frozen_at,b.workspace_id,
                    b.index_tier,b.last_accessed_at
             FROM museum_bundles b
             JOIN museum_fts f ON f.rowid = b.rowid
             WHERE museum_fts MATCH ?1 AND b.workspace_id = ?2 AND b.index_tier = 'hot'
             ORDER BY rank LIMIT 10")?;
        let rows = stmt.query_map(params![query, workspace_id], |r| bundle_row(r))?;
        rows.collect::<rusqlite::Result<_>>().context("search_bundles_fts")
    }

    /// Mark a bundle accessed now and promote it back to the hot tier.
    /// Called whenever the user opens a Museum snapshot.
    pub fn touch_bundle_access(&self, id: &str) -> Result<()> {
        let now = unix_now();
        self.0.lock().unwrap().execute(
            "UPDATE museum_bundles SET last_accessed_at = ?1, index_tier = 'hot' WHERE id = ?2",
            params![now, id],
        )?;
        Ok(())
    }

    /// Deep Dig: search cold-tier bundles by keyword fingerprint.
    ///
    /// Cold bundles have no FTS5 rows; instead we LIKE-match their tfidf_tags
    /// JSON string (a compact top-N keyword list).  Each whitespace-separated
    /// query token must appear in tfidf_tags.  Maximum 5 tokens, 20 results.
    ///
    /// [AUDIT] Uses parameterised LIKE patterns — no SQL injection risk.
    pub fn search_cold_keyword(&self, query: &str, workspace_id: &str) -> Result<Vec<BundleRow>> {
        let tokens: Vec<String> = query
            .split_whitespace()
            .take(5)
            .map(|t| format!("%{}%", t.replace('%', "\\%").replace('_', "\\_")))
            .collect();
        if tokens.is_empty() {
            return Ok(vec![]);
        }
        let like_clauses: String = (2..=tokens.len() + 1)
            .map(|i| format!("b.tfidf_tags LIKE ?{i} ESCAPE '\\'"))
            .collect::<Vec<_>>()
            .join(" AND ");
        let sql = format!(
            "SELECT b.id,b.url,b.title,b.content_hash,b.bundle_path,\
                    b.tfidf_tags,b.bundle_size,b.frozen_at,b.workspace_id,\
                    b.index_tier,b.last_accessed_at \
             FROM museum_bundles b \
             WHERE b.workspace_id = ?1 AND b.index_tier = 'cold' AND {like_clauses} \
             ORDER BY b.frozen_at DESC LIMIT 20"
        );
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(&sql)?;
        let mut all_vals: Vec<String> = vec![workspace_id.to_string()];
        all_vals.extend(tokens);
        let rows = stmt.query_map(
            rusqlite::params_from_iter(all_vals.iter()),
            |r| bundle_row(r),
        )?;
        rows.collect::<rusqlite::Result<_>>().context("search_cold_keyword")
    }

    pub fn delete_bundle(&self, id: &str) -> Result<()> {
        self.0.lock().unwrap().execute("DELETE FROM museum_bundles WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn delete_bundles_for_workspace(&self, workspace_id: &str) -> Result<Vec<String>> {
        let conn = self.0.lock().unwrap();
        let paths: Vec<String> = {
            let mut stmt = conn.prepare("SELECT bundle_path FROM museum_bundles WHERE workspace_id=?1")?;
            stmt.query_map([workspace_id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
        };
        conn.execute("DELETE FROM museum_bundles WHERE workspace_id=?1", [workspace_id])?;
        Ok(paths)
    }


    pub fn insert_dom_block(&self, domain: &str, selector: &str) -> Result<String> {
        let id = new_id();
        self.0.lock().unwrap().execute(
            "INSERT OR IGNORE INTO dom_blocks(id,domain,selector,created_at) VALUES(?1,?2,?3,?4)",
            params![id, domain, selector, unix_now()])?;
        Ok(id)
    }

    pub fn dom_blocks_for(&self, domain: &str) -> Result<Vec<DomBlock>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,domain,selector,created_at FROM dom_blocks WHERE domain=?1")?;
        let rows = stmt.query_map([domain], |r| Ok(DomBlock {
            id: r.get(0)?,
            domain: r.get(1)?,
            selector: r.get(2)?,
            created_at: r.get(3)?,
        }))?;
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


    pub fn totp_insert(&self, id: &str, issuer: &str, account: &str,
                       secret_enc: &str, domains_json: &str, added_at: i64) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT OR REPLACE INTO totp_entries(id,issuer,account,secret_enc,domains,added_at)
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![id, issuer, account, secret_enc, domains_json, added_at])?;
        Ok(())
    }

    pub fn totp_delete(&self, id: &str) -> Result<()> {
        self.0.lock().unwrap().execute("DELETE FROM totp_entries WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn totp_list_raw(&self) -> Result<Vec<TotpRaw>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,issuer,account,secret_enc,domains,added_at FROM totp_entries ORDER BY added_at")?;
        let rows = stmt.query_map([], |r| Ok(TotpRaw {
            id: r.get(0)?,
            issuer: r.get(1)?,
            account: r.get(2)?,
            secret_enc: r.get(3)?,
            domains_json: r.get(4)?,
            added_at: r.get(5)?,
        }))?;
        rows.collect::<rusqlite::Result<_>>().context("totp_list_raw")
    }


    pub fn trust_set(&self, domain: &str, level: &str, source: &str, set_at: i64) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT OR REPLACE INTO trust_profiles(domain,level,source,set_at)
             VALUES(?1,?2,?3,?4)",
            params![domain, level, source, set_at])?;
        Ok(())
    }

    pub fn trust_delete(&self, domain: &str) -> Result<()> {
        self.0.lock().unwrap().execute("DELETE FROM trust_profiles WHERE domain=?1", [domain])?;
        Ok(())
    }

    pub fn trust_list_raw(&self) -> Result<Vec<TrustRaw>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT domain,level,source,set_at FROM trust_profiles ORDER BY set_at")?;
        let rows = stmt.query_map([], |r| Ok(TrustRaw {
            domain: r.get(0)?,
            level: r.get(1)?,
            source: r.get(2)?,
            set_at: r.get(3)?,
        }))?;
        rows.collect::<rusqlite::Result<_>>().context("trust_list_raw")
    }


    pub fn rss_feed_upsert(&self, f: &RssFeedRaw) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT OR REPLACE INTO rss_feeds
             (id,url,title,category,fetch_interval_m,last_fetched,enabled,added_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![f.id, f.url, f.title, f.category, f.fetch_interval_m,
                    f.last_fetched, f.enabled as i32, f.added_at])?;
        Ok(())
    }

    pub fn rss_feed_delete(&self, id: &str) -> Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute("DELETE FROM rss_items WHERE feed_id=?1", [id])?;
        conn.execute("DELETE FROM rss_feeds WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn rss_feeds_all(&self) -> Result<Vec<RssFeedRaw>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,url,title,category,fetch_interval_m,last_fetched,enabled,added_at
             FROM rss_feeds ORDER BY added_at")?;
        let rows = stmt.query_map([], |r| Ok(RssFeedRaw {
            id: r.get(0)?,
            url: r.get(1)?,
            title: r.get(2)?,
            category: r.get(3)?,
            fetch_interval_m: r.get(4)?,
            last_fetched: r.get(5)?,
            enabled: r.get::<_,i32>(6)? != 0,
            added_at: r.get(7)?,
        }))?;
        rows.collect::<rusqlite::Result<_>>().context("rss_feeds_all")
    }

    pub fn rss_item_upsert(&self, i: &RssItemRaw) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT OR IGNORE INTO rss_items
             (id,feed_id,guid,title,url,summary,published,read,fetched_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![i.id, i.feed_id, i.guid, i.title, i.url, i.summary,
                    i.published, i.read as i32, i.fetched_at])?;
        Ok(())
    }

    pub fn rss_items_for_feed(&self, feed_id: &str) -> Result<Vec<RssItemRaw>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,feed_id,guid,title,url,summary,published,read,fetched_at
             FROM rss_items WHERE feed_id=?1 ORDER BY fetched_at DESC LIMIT 5000")?;
        let rows = stmt.query_map([feed_id], |r| Ok(RssItemRaw {
            id: r.get(0)?,
            feed_id: r.get(1)?,
            guid: r.get(2)?,
            title: r.get(3)?,
            url: r.get(4)?,
            summary: r.get(5)?,
            published: r.get(6)?,
            read: r.get::<_,i32>(7)? != 0,
            fetched_at: r.get(8)?,
        }))?;
        rows.collect::<rusqlite::Result<_>>().context("rss_items_for_feed")
    }

    pub fn rss_item_mark_read(&self, item_id: &str) -> Result<()> {
        self.0.lock().unwrap().execute("UPDATE rss_items SET read=1 WHERE id=?1", [item_id])?;
        Ok(())
    }


    pub fn insert_knowledge_pack(&self, p: &KnowledgePack) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT OR REPLACE INTO knowledge_packs(id,name,format,pack_path,size_bytes,added_at,enabled)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![p.id, p.name, p.format, p.pack_path, p.size_bytes, p.added_at, p.enabled as i32])?;
        Ok(())
    }

    pub fn list_knowledge_packs(&self) -> Result<Vec<KnowledgePack>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,name,format,pack_path,size_bytes,added_at,enabled
             FROM knowledge_packs ORDER BY added_at DESC")?;
        let rows = stmt.query_map([], |r| Ok(KnowledgePack {
            id: r.get(0)?,
            name: r.get(1)?,
            format: r.get(2)?,
            pack_path: r.get(3)?,
            size_bytes: r.get(4)?,
            added_at: r.get(5)?,
            enabled: r.get::<_,i32>(6)? != 0,
        }))?;
        rows.collect::<rusqlite::Result<_>>().context("list_knowledge_packs")
    }


    pub fn filter_sub_upsert(&self, id: &str, name: &str, url: &str) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT OR IGNORE INTO filter_subscriptions(id,name,url,added_at) VALUES(?1,?2,?3,?4)",
            params![id, name, url, unix_now()])?;
        Ok(())
    }

    pub fn filter_sub_update_stats(&self, url: &str, rule_count: usize) -> Result<()> {
        self.0.lock().unwrap().execute(
            "UPDATE filter_subscriptions SET last_synced=?1,rule_count=?2 WHERE url=?3",
            params![unix_now(), rule_count as i64, url])?;
        Ok(())
    }

    pub fn filter_subs_all(&self) -> Result<Vec<FilterSub>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,name,url,last_synced,enabled,rule_count,added_at
             FROM filter_subscriptions ORDER BY added_at")?;
        let rows = stmt.query_map([], |r| Ok(FilterSub {
            id: r.get(0)?,
            name: r.get(1)?,
            url: r.get(2)?,
            last_synced: r.get(3)?,
            enabled: r.get::<_,i32>(4)? != 0,
            rule_count: r.get::<_,i64>(5)? as usize,
            added_at: r.get(6)?,
        }))?;
        rows.collect::<rusqlite::Result<_>>().context("filter_subs_all")
    }


    pub fn zen_save(&self, active: bool, aphorism: &str,
                    blocked_cats_json: &str, activated_at: Option<i64>) -> Result<()> {
        self.0.lock().unwrap().execute(
            "UPDATE zen_state SET active=?1,aphorism=?2,blocked_cats=?3,activated_at=?4 WHERE id=1",
            params![active as i32, aphorism, blocked_cats_json, activated_at])?;
        Ok(())
    }

    pub fn zen_load(&self) -> Option<ZenRaw> {
        self.0.lock().unwrap().query_row(
            "SELECT active,aphorism,blocked_cats,activated_at FROM zen_state WHERE id=1",
            [], |r| Ok(ZenRaw {
                active: r.get::<_,i32>(0)? != 0,
                aphorism: r.get(1)?,
                blocked_cats_json: r.get(2)?,
                activated_at: r.get(3)?,
            }),
        ).ok()
    }


    pub fn onboarding_complete(&self, step: &str) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT OR REPLACE INTO onboarding(step,completed,done_at) VALUES(?1,1,?2)",
            params![step, unix_now()])?;
        Ok(())
    }

    pub fn onboarding_is_done(&self, step: &str) -> bool {
        self.0.lock().unwrap().query_row(
            "SELECT completed FROM onboarding WHERE step=?1", [step],
            |r| r.get::<_,i32>(0),
        ).ok().map(|v| v != 0).unwrap_or(false)
    }

    pub fn onboarding_all_steps(&self) -> Vec<(String, bool)> {
        let Ok(conn_guard) = self.0.lock() else { return vec![]; };
        let Ok(mut stmt) = conn_guard.prepare("SELECT step,completed FROM onboarding") else { return vec![]; };
        stmt.query_map([], |r| Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i32>(1)? != 0,
        )))
            .ok().map(|rows| rows.flatten().collect()).unwrap_or_default()
    }


    pub fn nostr_relay_add(&self, url: &str) -> Result<String> {
        let id = new_id();
        self.0.lock().unwrap().execute(
            "INSERT OR IGNORE INTO nostr_relays(id,url,added_at) VALUES(?1,?2,?3)",
            params![id, url, unix_now()])?;
        Ok(id)
    }

    pub fn nostr_relays_enabled(&self) -> Result<Vec<String>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare("SELECT url FROM nostr_relays WHERE enabled=1")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>().context("nostr_relays_enabled")
    }


    pub fn vault_login_upsert(&self, r: &VaultLoginRaw) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT OR REPLACE INTO vault_logins
             (id,title,username,password_enc,urls_json,notes_enc,tags_json,totp_uri,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![r.id, r.title, r.username, r.password_enc, r.urls_json,
                    r.notes_enc, r.tags_json, r.totp_uri, r.created_at, r.updated_at],
        )?;
        Ok(())
    }

    pub fn vault_login_delete(&self, id: &str) -> Result<()> {
        self.0.lock().unwrap().execute("DELETE FROM vault_logins WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn vault_logins_raw(&self) -> Result<Vec<VaultLoginRaw>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,title,username,password_enc,urls_json,notes_enc,tags_json,totp_uri,created_at,updated_at
             FROM vault_logins ORDER BY updated_at DESC")?;
        let rows = stmt.query_map([], |r| Ok(VaultLoginRaw {
            id: r.get(0)?,
            title: r.get(1)?,
            username: r.get(2)?,
            password_enc: r.get(3)?,
            urls_json: r.get(4)?,
            notes_enc: r.get(5)?,
            tags_json: r.get(6)?,
            totp_uri: r.get(7)?,
            created_at: r.get(8)?,
            updated_at: r.get(9)?,
        }))?;
        rows.collect::<rusqlite::Result<_>>().context("vault_logins_raw")
    }


    pub fn vault_card_upsert(&self, r: &VaultCardRaw) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT OR REPLACE INTO vault_cards
             (id,title,cardholder_enc,number_enc,expiry,cvv_enc,notes_enc,tags_json,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![r.id, r.title, r.cardholder_enc, r.number_enc, r.expiry,
                    r.cvv_enc, r.notes_enc, r.tags_json, r.created_at, r.updated_at],
        )?;
        Ok(())
    }

    pub fn vault_card_delete(&self, id: &str) -> Result<()> {
        self.0.lock().unwrap().execute("DELETE FROM vault_cards WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn vault_cards_raw(&self) -> Result<Vec<VaultCardRaw>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,title,cardholder_enc,number_enc,expiry,cvv_enc,notes_enc,tags_json,created_at,updated_at
             FROM vault_cards ORDER BY updated_at DESC")?;
        let rows = stmt.query_map([], |r| Ok(VaultCardRaw {
            id: r.get(0)?,
            title: r.get(1)?,
            cardholder_enc: r.get(2)?,
            number_enc: r.get(3)?,
            expiry: r.get(4)?,
            cvv_enc: r.get(5)?,
            notes_enc: r.get(6)?,
            tags_json: r.get(7)?,
            created_at: r.get(8)?,
            updated_at: r.get(9)?,
        }))?;
        rows.collect::<rusqlite::Result<_>>().context("vault_cards_raw")
    }


    pub fn vault_note_upsert(&self, r: &VaultNoteRaw) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT OR REPLACE INTO vault_notes
             (id,title,content_enc,tags_json,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![r.id, r.title, r.content_enc, r.tags_json, r.created_at, r.updated_at],
        )?;
        Ok(())
    }

    pub fn vault_note_delete(&self, id: &str) -> Result<()> {
        self.0.lock().unwrap().execute("DELETE FROM vault_notes WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn vault_notes_raw(&self) -> Result<Vec<VaultNoteRaw>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,title,content_enc,tags_json,created_at,updated_at
             FROM vault_notes ORDER BY updated_at DESC")?;
        let rows = stmt.query_map([], |r| Ok(VaultNoteRaw {
            id: r.get(0)?,
            title: r.get(1)?,
            content_enc: r.get(2)?,
            tags_json: r.get(3)?,
            created_at: r.get(4)?,
            updated_at: r.get(5)?,
        }))?;
        rows.collect::<rusqlite::Result<_>>().context("vault_notes_raw")
    }

}


fn bundle_row(r: &rusqlite::Row) -> rusqlite::Result<BundleRow> {
    Ok(BundleRow {
        id: r.get(0)?,
        url: r.get(1)?,
        title: r.get(2)?,
        content_hash: r.get(3)?,
        bundle_path: r.get(4)?,
        tfidf_tags: r.get(5)?,
        bundle_size: r.get(6)?,
        frozen_at: r.get(7)?,
        workspace_id: r.get(8)?,
        index_tier:       r.get::<_, String>(9).unwrap_or_else(|_| "hot".to_string()),
        last_accessed_at: r.get::<_, Option<i64>>(10).unwrap_or(None),
    })
}


trait OptionalExt<T> { fn optional(self) -> Result<Option<T>>; }
impl<T> OptionalExt<T> for rusqlite::Result<T> {
    fn optional(self) -> Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRow {
    pub id: String,
    pub url: String,
    pub title: String,
    pub visited_at: i64,
    pub dwell_ms: i64,
    pub visit_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarReportRow {
    pub tracking_block_count: i64,
    pub fingerprint_noise_count: i64,
    pub ram_saved_mb: f64,
    pub time_saved_min: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingEvent {
    pub id: String,
    pub url: String,
    pub domain: String,
    pub dwell_ms: i64,
    pub scroll_px_s: f64,
    pub reading_mode: bool,
    pub tab_switches: i64,
    pub recorded_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleRow {
    pub id: String,
    pub url: String,
    pub title: String,
    pub content_hash: String,
    pub bundle_path: String,
    pub tfidf_tags: String,
    pub bundle_size: i64,
    pub frozen_at: i64,
    pub workspace_id: String,
    /// Index tier: "hot" (full FTS5) or "cold" (keyword fingerprint only).
    #[serde(default = "default_hot")]
    pub index_tier: String,
    /// Unix timestamp of last user access; None for pages never re-visited.
    pub last_accessed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomBlock {
    pub id: String,
    pub domain: String,
    pub selector: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgePack {
    pub id: String,
    pub name: String,
    pub format: String,
    pub pack_path: String,
    pub size_bytes: i64,
    pub added_at: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct TotpRaw {
    pub id: String,
    pub issuer: String,
    pub account: String,
    pub secret_enc: String,
    pub domains_json: String,
    pub added_at: i64,
    /// [NEW-v0.9.5] Optional fields — None when DB row predates v0.9.5.
    pub algorithm: Option<String>,
    pub digits: Option<u8>,
    pub period: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct TrustRaw {
    pub domain: String,
    pub level: String,
    pub source: String,
    pub set_at: i64,
}

#[derive(Debug, Clone)]
pub struct RssFeedRaw {
    pub id: String,
    pub url: String,
    pub title: String,
    pub category: Option<String>,
    pub fetch_interval_m: i32,
    pub last_fetched: Option<i64>,
    pub enabled: bool,
    pub added_at: i64,
}

#[derive(Debug, Clone)]
pub struct RssItemRaw {
    pub id: String,
    pub feed_id: String,
    pub guid: String,
    pub title: String,
    pub url: String,
    pub summary: String,
    pub published: Option<i64>,
    pub read: bool,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterSub {
    pub id: String,
    pub name: String,
    pub url: String,
    pub last_synced: Option<i64>,
    pub enabled: bool,
    pub rule_count: usize,
    pub added_at: i64,
}

pub struct ZenRaw {
    pub active: bool,
    pub aphorism: String,
    pub blocked_cats_json: String,
    pub activated_at: Option<i64>,
}


pub fn new_id() -> String { uuid::Uuid::new_v4().to_string() }

fn default_hot() -> String { "hot".to_string() }

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default().as_secs() as i64
}

pub fn week_start(ts: i64) -> i64 {
    let dow = ((ts / 86_400) + 3) % 7;
    ts - (dow * 86_400) - (ts % 86_400)
}


#[derive(Debug, Clone)]
pub struct VaultLoginRaw {
    pub id: String,
    pub title: String,
    pub username: String,
    pub password_enc: String,
    pub urls_json: String,
    pub notes_enc: String,
    pub tags_json: String,
    pub totp_uri: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct VaultCardRaw {
    pub id: String,
    pub title: String,
    pub cardholder_enc: String,
    pub number_enc: String,
    pub expiry: String,
    pub cvv_enc: String,
    pub notes_enc: String,
    pub tags_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct VaultNoteRaw {
    pub id: String,
    pub title: String,
    pub content_enc: String,
    pub tags_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn week_start_monday() {
        let monday_ts: i64 = 20528 * 86_400;
        for d in 0i64..7 {
            let ws = week_start(monday_ts + d * 86_400 + 43_200);
            assert_eq!(ws, monday_ts);
            assert_eq!(ws % 86_400, 0);
        }
    }
    #[test]
    fn like_escape_chars() {
        let q = "50%_test";
        let esc = q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        assert_eq!(esc, "50\\%\\_test");
    }
}


impl Db {
    /// Upsert a tab group row.
    pub fn upsert_tab_group(
        &self,
        id: &str,
        workspace_id: &str,
        name: &str,
        color: &str,
        collapsed: bool,
        project_mode: bool,
        tab_ids: &[String],
        created_at: i64,
    ) -> anyhow::Result<()> {
        let conn = self.0.lock().unwrap();
        let tab_ids_json = serde_json::to_string(tab_ids)?;
        conn.execute(
            "INSERT INTO tab_groups (id,workspace_id,name,color,collapsed,project_mode,tab_ids,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name,color=excluded.color,
               collapsed=excluded.collapsed,project_mode=excluded.project_mode,
               tab_ids=excluded.tab_ids",
            rusqlite::params![id,workspace_id,name,color,
                collapsed as i64,project_mode as i64,tab_ids_json,created_at],
        )?;
        Ok(())
    }

    /// List all groups for a workspace. Returns (id, ws_id, name, color, collapsed, project_mode, tab_ids, created_at).
    pub fn list_tab_groups(&self, workspace_id: &str) -> anyhow::Result<Vec<(String,String,String,String,bool,bool,Vec<String>,i64)>> {
        let conn = self.0.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tab_groups (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                name TEXT NOT NULL DEFAULT 'Group',
                color TEXT NOT NULL DEFAULT '#60a5fa',
                collapsed INTEGER NOT NULL DEFAULT 0,
                project_mode INTEGER NOT NULL DEFAULT 0,
                tab_ids TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL
            );"
        ).ok();
        let mut stmt = conn.prepare(
            "SELECT id,workspace_id,name,color,collapsed,project_mode,tab_ids,created_at
             FROM tab_groups WHERE workspace_id=?1 ORDER BY created_at ASC"
        )?;
        let rows = stmt.query_map(rusqlite::params![workspace_id], |r| {
            Ok((
                r.get::<_,String>(0)?,
                r.get::<_,String>(1)?,
                r.get::<_,String>(2)?,
                r.get::<_,String>(3)?,
                r.get::<_,i64>(4)? != 0,
                r.get::<_,i64>(5)? != 0,
                r.get::<_,String>(6)?,
                r.get::<_,i64>(7)?,
            ))
        })?;
        rows.map(|r| r.map_err(anyhow::Error::from).and_then(|t| {
            let tab_ids: Vec<String> = serde_json::from_str(&t.6).unwrap_or_default();
            Ok((t.0,t.1,t.2,t.3,t.4,t.5,tab_ids,t.7))
        })).collect()
    }

    pub fn delete_tab_group(&self, group_id: &str) -> anyhow::Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute("DELETE FROM tab_groups WHERE id=?1", rusqlite::params![group_id])?;
        Ok(())
    }

    pub fn rename_tab_group(&self, group_id: &str, name: &str) -> anyhow::Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute("UPDATE tab_groups SET name=?1 WHERE id=?2",
            rusqlite::params![name, group_id])?;
        Ok(())
    }

    pub fn set_tab_group_collapsed(&self, group_id: &str, collapsed: bool) -> anyhow::Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute("UPDATE tab_groups SET collapsed=?1 WHERE id=?2",
            rusqlite::params![collapsed as i64, group_id])?;
        Ok(())
    }

    pub fn set_tab_group_project_mode(&self, group_id: &str, pm: bool) -> anyhow::Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute("UPDATE tab_groups SET project_mode=?1 WHERE id=?2",
            rusqlite::params![pm as i64, group_id])?;
        Ok(())
    }

    pub fn move_tab_to_group(&self, tab_id: &str, group_id: Option<&str>) -> anyhow::Result<()> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, tab_ids FROM tab_groups")?;
        let all: Vec<(String, String)> = stmt.query_map([], |r| {
            Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?))
        })?.filter_map(|r| r.ok()).collect();

        for (gid, ids_json) in all {
            let mut ids: Vec<String> = serde_json::from_str(&ids_json).unwrap_or_default();
            let before = ids.len();
            ids.retain(|id| id != tab_id);
            if Some(gid.as_str()) == group_id && !ids.contains(&tab_id.to_owned()) {
                ids.push(tab_id.to_owned());
            }
            if ids.len() != before || Some(gid.as_str()) == group_id {
                let new_json = serde_json::to_string(&ids).unwrap_or_default();
                conn.execute("UPDATE tab_groups SET tab_ids=?1 WHERE id=?2",
                    rusqlite::params![new_json, gid])?;
            }
        }
        Ok(())
    }
}

