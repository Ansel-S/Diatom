// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/rss.rs
//
// RSS 2.0 / Atom feed ingestion and in-memory store.
// All fetch logic lives in commands.rs (cmd_rss_fetch). This module
// handles parsing, deduplication, and read-state management.
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Data model ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feed {
    pub id:              String,
    pub url:             String,
    pub title:           String,
    pub category:        Option<String>,
    pub fetch_interval:  u32,   // minutes
    pub last_fetched:    Option<i64>,
    pub enabled:         bool,
    pub added_at:        i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id:         String,
    pub feed_id:    String,
    pub guid:       String,
    pub title:      String,
    pub url:        String,
    pub summary:    String,
    pub published:  Option<i64>,
    pub read:       bool,
    pub fetched_at: i64,
}

// ── Store ─────────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct RssStore {
    feeds: HashMap<String, Feed>,
    items: Vec<Item>,
    /// guid → item id dedup map
    guid_index: HashMap<String, String>,
}

impl RssStore {
    pub fn add(&mut self, url: &str, category: Option<String>) -> Feed {
        let id = crate::db::new_id();
        let feed = Feed {
            id: id.clone(),
            url: url.to_owned(),
            title: url.to_owned(),  // placeholder; updated on first fetch
            category,
            fetch_interval: 60,
            last_fetched: None,
            enabled: true,
            added_at: crate::db::unix_now(),
        };
        self.feeds.insert(id, feed.clone());
        feed
    }

    pub fn feeds(&self) -> Vec<Feed> {
        let mut v: Vec<Feed> = self.feeds.values().cloned().collect();
        v.sort_by_key(|f| f.added_at);
        v
    }

    pub fn feed_url(&self, id: &str) -> Option<String> {
        self.feeds.get(id).map(|f| f.url.clone())
    }

    pub fn remove_feed(&mut self, id: &str) {
        self.feeds.remove(id);
        self.items.retain(|i| i.feed_id != id);
    }

    /// Parse and ingest RSS/Atom XML. Returns number of new items added.
    pub fn ingest(&mut self, feed_id: &str, xml: &str) -> u32 {
        let mut count = 0u32;
        let now = crate::db::unix_now();

        // Update feed title from channel title
        if let Some(title) = extract_channel_title(xml) {
            if let Some(feed) = self.feeds.get_mut(feed_id) {
                feed.title = title;
                feed.last_fetched = Some(now);
            }
        }

        // Parse items
        for (guid, item_title, item_url, summary, published) in extract_items(xml) {
            if self.guid_index.contains_key(&guid) { continue; }
            let id = crate::db::new_id();
            self.guid_index.insert(guid.clone(), id.clone());
            self.items.push(Item {
                id,
                feed_id: feed_id.to_owned(),
                guid,
                title: item_title,
                url: crate::blocker::strip_params(&item_url),
                summary,
                published,
                read: false,
                fetched_at: now,
            });
            count += 1;
        }

        // Cap at 5000 total items (ring buffer)
        if self.items.len() > 5000 {
            let drain = self.items.len() - 5000;
            for item in self.items.drain(..drain) {
                self.guid_index.remove(&item.guid);
            }
        }

        count
    }

    pub fn items(&self, feed_id: Option<&str>, unread_only: bool, limit: usize) -> Vec<Item> {
        let mut v: Vec<&Item> = self.items.iter()
            .filter(|i| feed_id.map_or(true, |fid| i.feed_id == fid))
            .filter(|i| !unread_only || !i.read)
            .collect();
        // Most recent first
        v.sort_by(|a, b| b.fetched_at.cmp(&a.fetched_at));
        v.into_iter().take(limit).cloned().collect()
    }

    pub fn mark_read(&mut self, item_id: &str) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == item_id) {
            item.read = true;
        }
    }
}

// ── Minimal RSS/Atom XML parser (no external crate dependency) ─────────────────

fn extract_channel_title(xml: &str) -> Option<String> {
    // Match <title>...</title> (first occurrence = channel title)
    extract_tag(xml, "title")
}

fn extract_items(xml: &str) -> Vec<(String, String, String, String, Option<i64>)> {
    let mut out = Vec::new();
    // Split on <item> or <entry> boundaries
    let delimiter = if xml.contains("<entry") { "<entry" } else { "<item" };
    let close_delim = if xml.contains("</entry>") { "</entry>" } else { "</item>" };

    for chunk in xml.split(delimiter).skip(1) {
        let chunk = match chunk.split(close_delim).next() { Some(c) => c, None => continue };

        let title = extract_tag(chunk, "title").unwrap_or_default();
        let url   = extract_link(chunk);
        let guid  = extract_tag(chunk, "guid")
            .or_else(|| extract_tag(chunk, "id"))
            .unwrap_or_else(|| url.clone());
        let summary = extract_tag(chunk, "description")
            .or_else(|| extract_tag(chunk, "summary"))
            .or_else(|| extract_tag(chunk, "content"))
            .unwrap_or_default();
        let summary = strip_html_tags(&summary);
        let summary = summary.chars().take(500).collect();

        let published = extract_tag(chunk, "pubDate")
            .or_else(|| extract_tag(chunk, "published"))
            .or_else(|| extract_tag(chunk, "updated"))
            .and_then(|d| parse_rfc2822_date(&d).or_else(|| parse_iso8601_date(&d)));

        if !url.is_empty() {
            out.push((guid, title, url, summary, published));
        }
    }
    out
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open  = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)? + open.len();
    // Skip to end of opening tag
    let start = xml[start..].find('>')? + start + 1;
    let end   = xml.find(&close)?;
    if end <= start { return None; }
    let raw = xml[start..end].trim();
    Some(unescape_xml(raw))
}

fn extract_link(xml: &str) -> String {
    // Try <link>url</link> first
    if let Some(v) = extract_tag(xml, "link") {
        if v.starts_with("http") { return v; }
    }
    // Try <link href="url"/> (Atom)
    if let Some(pos) = xml.find("<link ") {
        let chunk = &xml[pos..];
        if let Some(href_pos) = chunk.find("href=\"") {
            let start = href_pos + 6;
            if let Some(end) = chunk[start..].find('"') {
                return chunk[start..start + end].to_owned();
            }
        }
    }
    String::new()
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _   => if !in_tag { out.push(c); }
        }
    }
    out
}

fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
     .replace("&lt;", "<")
     .replace("&gt;", ">")
     .replace("&quot;", "\"")
     .replace("&apos;", "'")
     .replace("&#039;", "'")
}

fn parse_rfc2822_date(s: &str) -> Option<i64> {
    // Very simplified: parse only the date portion to approximate.
    // Full implementation would use the chrono crate — acceptable for
    // display purposes where exact sorting is not critical.
    chrono::DateTime::parse_from_rfc2822(s.trim())
        .ok()
        .map(|dt| dt.timestamp())
}

fn parse_iso8601_date(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|dt| dt.timestamp())
}
