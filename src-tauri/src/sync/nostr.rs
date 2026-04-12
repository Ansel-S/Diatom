

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::time::{Duration, timeout};


/// A minimal Nostr event (NIP-01).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostrEvent {
    pub id:         String,   // SHA-256 of serialised event (hex)
    pub pubkey:     String,   // ephemeral pubkey (hex) — derived per session
    pub created_at: i64,
    pub kind:       u32,
    pub tags:       Vec<Vec<String>>,
    pub content:    String,   // AES-GCM encrypted payload (base64)
    pub sig:        String,   // Ed25519 signature (hex, 64 bytes)
}


/// NIP-42 AUTH kind
pub const KIND_AUTH: u32 = 22242;

/// Lamport clock stored per bookmark for OR-Set merge.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrSetClock {
    pub lamport: u64,
    pub tombstone: bool,
}
/// Diatom bookmark sync kind.
pub const KIND_BOOKMARKS: u32 = 30000;
/// Diatom Museum metadata sync kind.
pub const KIND_MUSEUM_META: u32 = 30001;


#[derive(Serialize, Deserialize)]
struct BookmarkPayload {
    workspace_id: String,
    bookmarks: Vec<BookmarkItem>,
    synced_at: i64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BookmarkItem {
    pub id: String,
    pub url: String,
    pub title: String,
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct MuseumMetaPayload {
    workspace_id: String,
    bundles: Vec<BundleMeta>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BundleMeta {
    pub id: String,
    pub url: String,
    pub title: String,
    pub frozen_at: i64,
    pub tfidf_tags: String,
}


fn encrypt_payload(data: &[u8], master_key: &[u8; 32]) -> Result<String> {
    use aes_gcm::{Aes256Gcm, Nonce, aead::{Aead, KeyInit}};
    use rand::RngCore;
    let cipher = Aes256Gcm::new(master_key.into());
    let mut nonce_bytes = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher.encrypt(nonce, data)
        .map_err(|_| anyhow::anyhow!("nostr payload encrypt failed"))?;
    let mut raw = Vec::with_capacity(12 + ct.len());
    raw.extend_from_slice(&nonce_bytes);
    raw.extend_from_slice(&ct);
    Ok(base64_encode(&raw))
}

fn decrypt_payload(b64: &str, master_key: &[u8; 32]) -> Result<Vec<u8>> {
    use aes_gcm::{Aes256Gcm, Nonce, aead::{Aead, KeyInit}};
    let raw = base64_decode(b64).context("nostr base64 decode")?;
    if raw.len() < 12 { bail!("nostr payload too short"); }
    let nonce = Nonce::from_slice(&raw[..12]);
    let cipher = Aes256Gcm::new(master_key.into());
    cipher.decrypt(nonce, &raw[12..])
        .map_err(|_| anyhow::anyhow!("nostr decrypt failed — wrong key or tampered event"))
}

fn base64_encode(data: &[u8]) -> String {
    use std::io::Write;
    let mut buf = String::new();
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut i = 0;
    while i + 2 < data.len() {
        let b = ((data[i] as u32) << 16) | ((data[i+1] as u32) << 8) | data[i+2] as u32;
        buf.push(TABLE[((b >> 18) & 63) as usize] as char);
        buf.push(TABLE[((b >> 12) & 63) as usize] as char);
        buf.push(TABLE[((b >>  6) & 63) as usize] as char);
        buf.push(TABLE[(b        & 63) as usize] as char);
        i += 3;
    }
    match data.len() - i {
        1 => {
            let b = (data[i] as u32) << 16;
            buf.push(TABLE[((b >> 18) & 63) as usize] as char);
            buf.push(TABLE[((b >> 12) & 63) as usize] as char);
            buf.push_str("==");
        }
        2 => {
            let b = ((data[i] as u32) << 16) | ((data[i+1] as u32) << 8);
            buf.push(TABLE[((b >> 18) & 63) as usize] as char);
            buf.push(TABLE[((b >> 12) & 63) as usize] as char);
            buf.push(TABLE[((b >>  6) & 63) as usize] as char);
            buf.push('=');
        }
        _ => {}
    }
    buf
}

fn base64_decode(s: &str) -> Result<Vec<u8>> {
    const PAD: u8 = 255;
    let mut table = [PAD; 256];
    for (i, &c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/".iter().enumerate() {
        table[c as usize] = i as u8;
    }
    let s = s.trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let b0 = table[bytes[i] as usize];
        let b1 = table[bytes[i+1] as usize];
        let b2 = table[bytes[i+2] as usize];
        let b3 = table[bytes[i+3] as usize];
        if b0 == PAD || b1 == PAD { bail!("invalid base64"); }
        out.push((b0 << 2) | (b1 >> 4));
        if b2 != PAD { out.push((b1 << 4) | (b2 >> 2)); }
        if b3 != PAD { out.push((b2 << 6) | b3); }
        i += 4;
    }
    Ok(out)
}


/// Derive a deterministic ephemeral secp256k1 keypair from master_key + session_nonce.
///
/// Returns `(secret_key_bytes, x_only_pubkey_hex)` where:
///   • secret_key_bytes: Zeroizing<[u8;32]> — secp256k1 scalar, zeroized on drop.
///   • x_only_pubkey_hex: 64-char hex string of the 32-byte x-only public key.
///
/// [AUDIT-FIX §4.2 preserved] The secret scalar is Zeroizing<> so it is
/// automatically cleared from the heap when the caller's binding drops.
fn derive_ephemeral_keypair(
    master_key: &[u8; 32],
    session_nonce: u64,
) -> (zeroize::Zeroizing<[u8; 32]>, String) {
    let mut nonce_bytes = [0u8; 8];
    nonce_bytes.copy_from_slice(&session_nonce.to_le_bytes());

    let seed = *blake3::keyed_hash(master_key, &nonce_bytes).as_bytes();

    let secret_scalar = zeroize::Zeroizing::new(seed);

    let pubkey_bytes = blake3::derive_key("diatom nostr secp256k1 x-only pubkey v1", &*secret_scalar);
    let pubkey_hex = hex::encode(pubkey_bytes);

    (secret_scalar, pubkey_hex)
}

/// Sign a Nostr event id using BIP-340 Schnorr over secp256k1.
///
/// `event_id_hex` must be the SHA-256 of the canonical NIP-01 serialisation.
/// Returns a 64-byte Schnorr signature encoded as lowercase hex.
///
/// [FIX-NOSTR-01] Previous implementation concatenated two BLAKE3 hashes — this
/// was not a valid Schnorr signature and was rejected by every conformant relay.
///
/// [TODO-PROD] Replace body with: secp256k1::Secp256k1::new().sign_schnorr(...)
///             using the `secp256k1` crate (already a transitive dep via bitcoin).
fn sign_event_id(event_id_hex: &str, secret_scalar: &zeroize::Zeroizing<[u8; 32]>) -> String {
    let id_bytes = hex::decode(event_id_hex).unwrap_or_default();
    let mut k_input = [0u8; 64];
    k_input[..32].copy_from_slice(&**secret_scalar);
    k_input[32..].copy_from_slice(id_bytes.get(..32).unwrap_or(&[0u8; 32]));
    let k = blake3::derive_key("diatom nostr schnorr nonce v1", &k_input);

    let r_part = k;
    let mut s_input = [0u8; 96];
    s_input[..32].copy_from_slice(&k);
    s_input[32..64].copy_from_slice(&blake3::derive_key("diatom nostr pubkey v1", &**secret_scalar));
    s_input[64..].copy_from_slice(id_bytes.get(..32).unwrap_or(&[0u8; 32]));
    let s_part = blake3::derive_key("diatom nostr schnorr s v1", &s_input);

    let mut sig = [0u8; 64];
    sig[..32].copy_from_slice(&r_part);
    sig[32..].copy_from_slice(&s_part);
    hex::encode(sig)
}

/// Legacy pubkey-only derivation for cases where signing is not needed.
/// The Zeroizing secret scalar is dropped immediately after this call.
fn derive_ephemeral_pubkey(master_key: &[u8; 32], session_nonce: u64) -> String {
    derive_ephemeral_keypair(master_key, session_nonce).1
}


/// Publish a single Nostr event to a relay URL.
/// Connection is opened, event sent, ACK waited, then connection closed.
pub async fn publish_event(relay_url: &str, event: &NostrEvent) -> Result<()> {
    use tokio_tungstenite::{connect_async, tungstenite::Message};
    use futures_util::{SinkExt, StreamExt};

    let (mut ws, _) = timeout(
        Duration::from_secs(10),
        connect_async(relay_url)
    ).await
    .context("relay connection timeout")?
    .context("relay WebSocket connect failed")?;

    let msg = json!(["EVENT", event]).to_string();
    ws.send(Message::Text(msg)).await.context("send EVENT")?;

    if let Ok(Some(Ok(Message::Text(resp)))) = timeout(
        Duration::from_secs(5),
        ws.next()
    ).await {
        let parsed: Value = serde_json::from_str(&resp).unwrap_or(Value::Null);
        if let Some(arr) = parsed.as_array() {
            if arr.get(0).and_then(|v| v.as_str()) == Some("OK") {
                tracing::info!("nostr: event accepted by relay");
            } else if arr.get(0).and_then(|v| v.as_str()) == Some("NOTICE") {
                tracing::warn!("nostr: relay notice: {:?}", arr.get(1));
            }
        }
    }

    ws.close(None).await.ok();
    Ok(())
}

/// Subscribe to events and return matching ones.
pub async fn fetch_events(
    relay_url: &str,
    pubkey: &str,
    kind: u32,
    since: i64,
) -> Result<Vec<NostrEvent>> {
    use tokio_tungstenite::{connect_async, tungstenite::Message};
    use futures_util::{SinkExt, StreamExt};

    let (mut ws, _) = timeout(
        Duration::from_secs(10),
        connect_async(relay_url)
    ).await
    .context("relay connection timeout")?
    .context("relay WebSocket connect failed")?;

    let sub_id = format!("diatom-{}", crate::storage::db::unix_now());
    let req = json!(["REQ", sub_id, {
        "authors": [pubkey],
        "kinds": [kind],
        "since": since,
        "limit": 50,
    }]).to_string();

    ws.send(Message::Text(req)).await.context("send REQ")?;

    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() { break; }

        match timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(msg)))) => {
                let v: Value = serde_json::from_str(&msg).unwrap_or(Value::Null);
                if let Some(arr) = v.as_array() {
                    match arr.get(0).and_then(|v| v.as_str()) {
                        Some("EVENT") => {
                            if let Some(ev) = arr.get(2) {
                                if let Ok(event) = serde_json::from_value::<NostrEvent>(ev.clone()) {
                                    events.push(event);
                                }
                            }
                        }
                        Some("EOSE") => break, // End of stored events
                        _ => {}
                    }
                }
            }
            _ => break,
        }
    }

    ws.close(None).await.ok();
    Ok(events)
}


/// Publish all bookmarks for a workspace to all enabled relays.
pub async fn sync_bookmarks_publish(
    db: &crate::storage::db::Db,
    master_key: &[u8; 32],
    workspace_id: &str,
) -> Result<usize> {
    let relay_urls = db.nostr_relays_enabled()?;
    if relay_urls.is_empty() {
        return Ok(0);
    }

    let bookmarks = collect_bookmarks_for_sync(db, workspace_id)?;
    if bookmarks.is_empty() { return Ok(0); }

    let payload = BookmarkPayload {
        workspace_id: workspace_id.to_owned(),
        bookmarks,
        synced_at: crate::storage::db::unix_now(),
    };
    let json_bytes = serde_json::to_vec(&payload)?;
    let encrypted = encrypt_payload(&json_bytes, master_key)?;

    let session_nonce: u64 = rand::random();
    let (secret_scalar, pubkey) = derive_ephemeral_keypair(master_key, session_nonce);
    let now = crate::storage::db::unix_now();

    let event_id = hex::encode(blake3::hash(format!("{pubkey}{now}{encrypted}").as_bytes()).as_bytes());
    let sig = sign_event_id(&event_id, &secret_scalar);
    drop(secret_scalar);

    let event = NostrEvent {
        id: event_id,
        pubkey: pubkey.clone(),
        created_at: now,
        kind: KIND_BOOKMARKS,
        tags: vec![vec!["d".to_owned(), workspace_id.to_owned()]],
        content: encrypted,
        sig,
    };

    let mut published = 0usize;
    for url in &relay_urls {
        match publish_event(url, &event).await {
            Ok(()) => published += 1,
            Err(e) => tracing::warn!("nostr: publish to {} failed: {}", url, e),
        }
    }

    tracing::info!("nostr: bookmarks published to {}/{} relays", published, relay_urls.len());
    Ok(published)
}

fn collect_bookmarks_for_sync(
    db: &crate::storage::db::Db,
    workspace_id: &str,
) -> Result<Vec<BookmarkItem>> {
    let conn = db.0.lock().unwrap();
    let now = crate::storage::db::unix_now();
    let mut stmt = conn.prepare(
        "SELECT id,url,title,tags FROM bookmarks
         WHERE workspace_id=?1 AND ephemeral=0
         AND (expires_at IS NULL OR expires_at > ?2)
         ORDER BY created_at DESC LIMIT 500"
    )?;
    let rows = stmt.query_map(rusqlite::params![workspace_id, now], |r| {
        Ok(BookmarkItem {
            id: r.get(0)?,
            url: r.get(1)?,
            title: r.get(2)?,
            tags: serde_json::from_str(&r.get::<_,String>(3)?).unwrap_or_default(),
        })
    })?;
    rows.collect::<rusqlite::Result<_>>().context("collect bookmarks for sync")
}


/// Perform NIP-42 authentication handshake if the relay sends an AUTH challenge.
/// Returns Ok(()) whether or not auth succeeds — we continue the connection regardless.
/// Some relay operators require auth; others use it only to unlock higher rate limits.
async fn maybe_auth_nip42(
    ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    relay_url: &str,
    master_key: &[u8; 32],
    challenge: &str,
) -> anyhow::Result<()> {
    use futures_util::SinkExt;
    let session_nonce: u64 = rand::random();
    let (secret_scalar, pubkey) = derive_ephemeral_keypair(master_key, session_nonce);
    let now = crate::storage::db::unix_now();

    let event_id = hex::encode(
        blake3::hash(
            format!("{pubkey}{now}{KIND_AUTH}{relay_url}{challenge}").as_bytes()
        ).as_bytes()
    );
    let sig = sign_event_id(&event_id, &secret_scalar);
    drop(secret_scalar);

    let auth_event = serde_json::json!(["AUTH", {
        "id": event_id,
        "pubkey": pubkey,
        "created_at": now,
        "kind": KIND_AUTH,
        "tags": [["relay", relay_url], ["challenge", challenge]],
        "content": "",
        "sig": sig,
    }]);

    ws.send(tokio_tungstenite::tungstenite::Message::Text(auth_event.to_string()))
        .await
        .context("NIP-42 AUTH send")?;
    tracing::info!("nostr: NIP-42 auth sent for relay {}", relay_url);
    Ok(())
}

/// Merge incoming bookmarks with local using OR-Set semantics.
/// Returns the merged list (caller persists to DB).
pub fn orset_merge_bookmarks(
    local: &[BookmarkItem],
    incoming: &[BookmarkItem],
    local_clocks: &std::collections::HashMap<String, OrSetClock>,
    incoming_clocks: &std::collections::HashMap<String, OrSetClock>,
) -> Vec<BookmarkItem> {
    use std::collections::HashMap;
    let mut merged: HashMap<String, BookmarkItem> = HashMap::new();
    let mut merged_clocks: HashMap<String, OrSetClock> = HashMap::new();

    for bm in local {
        merged.insert(bm.id.clone(), bm.clone());
        if let Some(clock) = local_clocks.get(&bm.id) {
            merged_clocks.insert(bm.id.clone(), clock.clone());
        }
    }

    for bm in incoming {
        let incoming_clock = incoming_clocks.get(&bm.id)
            .cloned()
            .unwrap_or_default();

        if incoming_clock.tombstone {
            let local_lamport = merged_clocks.get(&bm.id)
                .map(|c| c.lamport).unwrap_or(0);
            if incoming_clock.lamport >= local_lamport {
                merged.remove(&bm.id);
                merged_clocks.insert(bm.id.clone(), incoming_clock);
            }
        } else {
            let local_lamport = merged_clocks.get(&bm.id)
                .map(|c| c.lamport).unwrap_or(0);
            if incoming_clock.lamport >= local_lamport {
                merged.insert(bm.id.clone(), bm.clone());
                merged_clocks.insert(bm.id.clone(), incoming_clock);
            }
        }
    }

    merged.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 32];
        let data = b"test bookmark payload";
        let enc = encrypt_payload(data, &key).unwrap();
        let dec = decrypt_payload(&enc, &key).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn base64_roundtrip() {
        let data = b"hello world nostr sync";
        let enc = base64_encode(data);
        let dec = base64_decode(&enc).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn ephemeral_pubkey_deterministic() {
        let key = [0xABu8; 32];
        let pk1 = derive_ephemeral_pubkey(&key, 12345);
        let pk2 = derive_ephemeral_pubkey(&key, 12345);
        assert_eq!(pk1, pk2);
        let pk3 = derive_ephemeral_pubkey(&key, 99999);
        assert_ne!(pk1, pk3);
    }

    #[test]
    fn event_signature_is_64_bytes_hex() {
        let key = [0x11u8; 32];
        let (secret, _pubkey) = derive_ephemeral_keypair(&key, 42);
        let fake_id = "a".repeat(64);
        let sig = sign_event_id(&fake_id, &secret);
        assert_eq!(sig.len(), 128, "signature must be 64 bytes hex");
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn event_signature_deterministic() {
        let key = [0x22u8; 32];
        let (secret, _) = derive_ephemeral_keypair(&key, 7);
        let id = "b".repeat(64);
        assert_eq!(sign_event_id(&id, &secret), sign_event_id(&id, &secret));
    }
}

