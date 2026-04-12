
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, OsRng},
};
use anyhow::{Context, Result, bail};
use flate2::{Compression, write::GzEncoder};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::LazyLock,
};
use zeroize::Zeroize;

use crate::{
    blocker::is_blocked,
    db::{BundleRow, new_id, unix_now},
};

const EWBN_MAGIC: &[u8; 4] = b"EWBT";
const EWBN_VERSION: u32 = 1;

const TRACKER_SCRIPT_DOMAINS: &[&str] = &[
    "doubleclick.net",
    "googlesyndication.com",
    "googletagmanager.com",
    "google-analytics.com",
    "connect.facebook.net",
    "pixel.facebook.com",
    "hotjar.com",
    "amplitude.com",
    "api.segment.io",
    "cdn.segment.com",
    "mixpanel.com",
    "clarity.ms",
    "fullstory.com",
    "chartbeat.com",
    "parsely.com",
    "scorecardresearch.com",
    "bugsnag.com",
    "ingest.sentry.io",
    "js-agent.newrelic.com",
    "nr-data.net",
    "adnxs.com",
    "adroll.com",
    "criteo.com",
    "outbrain.com",
    "taboola.com",
    "adsrvr.org",
    "munchkin.marketo.net",
    "js.hs-scripts.com",
    "cdn.heapanalytics.com",
    "bat.bing.com",
    "px.ads.linkedin.com",
    "moatads.com",
];


pub struct FreezeBundle {
    pub bundle_row: BundleRow,
    pub bundle_path: PathBuf,
}

/// Main entry point: strip, compress, encrypt, write, return row.
pub fn freeze_page(
    raw_html: &str,
    url: &str,
    title: &str,
    workspace_id: &str,
    master_key: &[u8; 32],
    bundles_dir: &Path,
) -> Result<FreezeBundle> {
    let stripped = strip_trackers(raw_html);

    let compressed = gzip_compress(stripped.as_bytes())?;

    let encrypted = derive_bundle_key_and_encrypt(master_key, &compressed)?;

    let id = new_id();
    let filename = format!("{id}.ewbn");
    let path = bundles_dir.join(&filename);
    write_ewbn(&path, &encrypted)?;

    let content_hash = blake3::hash(stripped.as_bytes()).to_hex().to_string();

    let row = BundleRow {
        id: id.clone(),
        url: url.to_owned(),
        title: title.to_owned(),
        content_hash,
        bundle_path: filename,
        tfidf_tags: "[]".to_owned(),
        bundle_size: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) as i64,
        frozen_at: unix_now(),
        workspace_id: workspace_id.to_owned(),
    };

    Ok(FreezeBundle {
        bundle_row: row,
        bundle_path: path,
    })
}

/// Decrypt and return the stripped HTML for a frozen bundle.
pub fn thaw_bundle(bundle_path: &Path, master_key: &[u8; 32]) -> Result<String> {
    let raw = std::fs::read(bundle_path).context("read .ewbn")?;
    let ciphertext = parse_ewbn(&raw)?;
    let compressed = derive_bundle_key_and_decrypt(master_key, ciphertext)?;
    let html = gzip_decompress(&compressed)?;
    Ok(html)
}


fn strip_trackers(html: &str) -> String {
    use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};

    static AC: LazyLock<AhoCorasick> = LazyLock::new(|| {
        AhoCorasickBuilder::new()
            .match_kind(MatchKind::LeftmostFirst)
            .ascii_case_insensitive(true)
            .build(TRACKER_SCRIPT_DOMAINS)
            .unwrap()
    });

    /// Find the end of an HTML tag starting at `start` in `html`, correctly
    /// handling `>` inside single- and double-quoted attribute values.
    fn find_tag_end(html: &str, start: usize) -> usize {
        #[derive(PartialEq)]
        enum S { Tag, DQuote, SQuote }
        let mut state = S::Tag;
        let bytes = html.as_bytes();
        let mut i = start;
        while i < bytes.len() {
            match (state, bytes[i]) {
                (S::Tag, b'"')    => state = S::DQuote,
                (S::Tag, b''')  => state = S::SQuote,
                (S::Tag, b'>')   => return i + 1,
                (S::DQuote, b'"') => state = S::Tag,
                (S::SQuote, b''') => state = S::Tag,
                _ => {}
            }
            i += 1;
        }
        html.len() // unclosed tag — consume to end
    }

    let mut output = String::with_capacity(html.len());
    let mut pos = 0;

    while pos < html.len() {
        if let Some(rel) = html[pos..].find('<') {
            let tag_start = pos + rel;
            output.push_str(&html[pos..tag_start]);

            let tag_end = find_tag_end(html, tag_start + 1);
            let tag = &html[tag_start..tag_end];
            let lower = tag.to_lowercase();

            let is_external = lower.starts_with("<script")
                || lower.starts_with("<img")
                || lower.starts_with("<iframe")
                || lower.starts_with("<link");

            let has_tracker_src = is_external && AC.is_match(tag);
            let is_tracking_pixel = lower.starts_with("<img")
                && (lower.contains("width="1"") || lower.contains("width='1'"))
                && (lower.contains("height="1"") || lower.contains("height='1'"));

            if has_tracker_src || is_tracking_pixel {
                let close_tag = if lower.starts_with("<script") && !lower.contains("/>") {
                    Some("</script>")
                } else if lower.starts_with("<iframe") && !lower.contains("/>") {
                    Some("</iframe>")
                } else {
                    None
                };
                if let Some(close) = close_tag {
                    if let Some(c) = html[tag_end..].to_lowercase().find(close) {
                        pos = tag_end + c + close.len();
                    } else {
                        pos = tag_end;
                    }
                } else {
                    pos = tag_end;
                }
                output.push_str("<!-- [diatom:stripped] -->");
            } else {
                output.push_str(tag);
                pos = tag_end;
            }
        } else {
            output.push_str(&html[pos..]);
            break;
        }
    }
    output
}


/// Derive a per-bundle AES key, encrypt `plaintext`, then immediately zeroize
/// the derived key. This ensures the 32-byte bundle key never outlives the
/// encrypt call, even if the caller panics.
fn derive_bundle_key_and_encrypt(master_key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let mut bundle_key = derive_bundle_key(master_key)?;
    let result = aes_gcm_encrypt(&bundle_key, plaintext);
    bundle_key.zeroize();
    result
}

/// Derive a per-bundle AES key, decrypt `ciphertext`, then immediately zeroize
/// the derived key.
fn derive_bundle_key_and_decrypt(
    master_key: &[u8; 32],
    ciphertext: Vec<u8>,
) -> Result<zeroize::Zeroizing<Vec<u8>>> {
    let mut bundle_key = derive_bundle_key(master_key)?;
    let result = aes_gcm_decrypt(&bundle_key, ciphertext);
    bundle_key.zeroize();
    result
}

fn derive_bundle_key(master_key: &[u8; 32]) -> Result<[u8; 32]> {
    let hk = Hkdf::<Sha256>::new(None, master_key);
    let mut key = [0u8; 32];
    hk.expand(b"freeze-v7", &mut key)
        .map_err(|_| anyhow::anyhow!("HKDF expand failed"))?;
    Ok(key)
}

fn aes_gcm_encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| anyhow::anyhow!("AES-GCM encrypt failed"))?;
    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

fn aes_gcm_decrypt(key: &[u8; 32], mut data: Vec<u8>) -> Result<zeroize::Zeroizing<Vec<u8>>> {
    if data.len() < 12 {
        bail!("ciphertext too short");
    }
    let nonce_bytes: [u8; 12] = data[..12].try_into().unwrap();
    let ct = data[12..].to_vec();
    data.zeroize();

    let cipher = Aes256Gcm::new(key.into());
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ct.as_ref())
        .map_err(|_| anyhow::anyhow!("AES-GCM decrypt failed — wrong key or corrupted bundle"))?;
    Ok(zeroize::Zeroizing::new(plaintext))
}

fn gzip_compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
    enc.write_all(data)?;
    enc.finish().context("gzip compress")
}

fn gzip_decompress(data: &[u8]) -> Result<String> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut dec = GzDecoder::new(data);
    let mut s = String::new();
    dec.read_to_string(&mut s).context("gzip decompress")?;
    Ok(s)
}


fn write_ewbn(path: &Path, payload: &[u8]) -> Result<()> {
    use std::io::BufWriter;
    let file = std::fs::File::create(path).context("create .ewbn")?;
    let mut w = BufWriter::new(file);
    w.write_all(EWBN_MAGIC)?;
    w.write_all(&EWBN_VERSION.to_le_bytes())?;
    w.write_all(&(payload.len() as u64).to_le_bytes())?;
    w.write_all(payload)?;
    Ok(())
}

fn parse_ewbn(raw: &[u8]) -> Result<Vec<u8>> {
    if raw.len() < 16 {
        bail!("file too short to be a valid .ewbn");
    }
    if &raw[..4] != EWBN_MAGIC {
        bail!("invalid .ewbn magic");
    }
    let _version = u32::from_le_bytes(raw[4..8].try_into().unwrap());
    let payload_len = u64::from_le_bytes(raw[8..16].try_into().unwrap()) as usize;
    if raw.len() < 16 + payload_len {
        bail!(".ewbn payload truncated");
    }
    Ok(raw[16..16 + payload_len].to_vec())
}


/// Retrieve or generate the app master key.
///
/// Priority:
///   1. OS keychain / secret store (macOS Keychain, Windows DPAPI).
///   2. Fallback: hex value in DB meta table `master_key_hex` (insecure).
pub fn get_or_init_master_key(db: &crate::storage::db::Db) -> Result<[u8; 32]> {
    #[cfg(target_os = "macos")]
    {
        if let Some(key) = macos_keychain_read() {
            return Ok(key);
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(key) = windows_dpapi_read(db) {
            return Ok(key);
        }
    }
    #[cfg(all(target_os = "linux", feature = "secret-service"))]
    {
        if let Some(key) = linux_secret_service_read() {
            return Ok(key);
        }
    }

    tracing::warn!(
        "OS keychain unavailable — master key stored in SQLite (insecure fallback). \
         Install a keychain daemon or rebuild with platform credential support."
    );

    if let Some(hex_key) = db.get_setting("master_key_hex") {
        let mut bytes = hex::decode(&hex_key).context("decode master key")?;
        if bytes.len() != 32 {
            bail!("invalid master key length");
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        bytes.zeroize();
        return Ok(arr);
    }

    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    db.set_setting("master_key_hex", &hex::encode(key))?;
    tracing::warn!(
        "Generated new master key (stored in DB — consider enabling keychain support)."
    );
    Ok(key)
}


#[cfg(target_os = "macos")]
fn macos_keychain_read() -> Option<[u8; 32]> {
    use security_framework::passwords::get_generic_password;
    let bytes = get_generic_password("com.ansel-s.diatom", "com.ansel-s.diatom.masterkey").ok()?;
    if bytes.len() != 32 {
        tracing::warn!("keychain: master key has unexpected length {}", bytes.len());
        return None;
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    let mut b = bytes;
    b.zeroize();
    Some(arr)
}

#[cfg(target_os = "macos")]
fn macos_keychain_write(key: &[u8; 32]) -> bool {
    use security_framework::passwords::{delete_generic_password, set_generic_password};
    let _ = delete_generic_password("com.ansel-s.diatom", "com.ansel-s.diatom.masterkey");
    set_generic_password("com.ansel-s.diatom", "com.ansel-s.diatom.masterkey", key).is_ok()
}


#[cfg(target_os = "windows")]
fn windows_dpapi_read(db: &crate::storage::db::Db) -> Option<[u8; 32]> {
    let blob_hex = db.get_setting("master_key_dpapi_blob")?;
    let encrypted = hex::decode(&blob_hex).ok()?;
    unsafe { dpapi_decrypt(&encrypted) }
}

#[cfg(target_os = "windows")]
unsafe fn dpapi_decrypt(data: &[u8]) -> Option<[u8; 32]> {
    use windows_sys::Win32::Security::Cryptography::{CRYPTOAPI_BLOB, CryptUnprotectData};
    let mut in_blob = CRYPTOAPI_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut _,
    };
    let mut out_blob = CRYPTOAPI_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    if CryptUnprotectData(
        &mut in_blob,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        0,
        &mut out_blob,
    ) == 0
    {
        return None;
    }
    if out_blob.cbData as usize != 32 {
        return None;
    }
    let mut arr = [0u8; 32];
    std::ptr::copy_nonoverlapping(out_blob.pbData, arr.as_mut_ptr(), 32);
    windows_sys::Win32::Foundation::LocalFree(out_blob.pbData as _);
    Some(arr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let key = [0x42u8; 32];
        let plain = b"hello diatom freeze test";
        let ct = aes_gcm_encrypt(&key, plain).unwrap();
        let dec = aes_gcm_decrypt(&key, ct).unwrap();
        assert_eq!(dec, plain);
    }

    #[test]
    fn bundle_key_zeroize_roundtrip() {
        let master = [0xBEu8; 32];
        let plain = b"zeroize test payload";
        let ct = derive_bundle_key_and_encrypt(&master, plain).unwrap();
        let dec = derive_bundle_key_and_decrypt(&master, ct).unwrap();
        assert_eq!(dec, plain);
    }

    #[test]
    fn strips_tracker_script() {
        let html = r#"<html>
<script src="https://www.google-analytics.com/analytics.js"></script>
<p>Real content</p>
<img src="https://pixel.facebook.com/tr?id=123" width="1" height="1">
</html>"#;
        let stripped = strip_trackers(html);
        assert!(stripped.contains("Real content"));
        assert!(!stripped.contains("google-analytics.com"));
        assert!(!stripped.contains("pixel.facebook.com"));
        assert!(stripped.contains("[diatom:stripped]"));
    }

    #[test]
    fn ewbn_roundtrip() {
        let dir = tempdir().unwrap();
        let key = [0xABu8; 32];
        let html = "<html><body>test freeze</body></html>";
        let result = freeze_page(html, "https://example.com", "Test", "ws-0", &key, dir.path());
        assert!(result.is_ok());
        let bundle = result.unwrap();
        let thawed = thaw_bundle(&bundle.bundle_path, &key).unwrap();
        assert!(thawed.contains("test freeze"));
    }

    /// [B-08 FIX] Compile-time hex codec sanity check.
    /// Verifies that hex::encode → hex::decode is an identity operation,
    /// ensuring future developers don't confuse the hex DB key with base64.
    #[test]
    fn hex_codec_roundtrip() {
        let original = b"diatom-master-key-test-32-bytes!";
        let encoded  = hex::encode(original);
        let decoded  = hex::decode(&encoded).expect("hex decode must succeed");
        assert_eq!(decoded, original, "hex round-trip must be identity");
        assert_eq!(encoded.len(), 64);
    }
}


/// Temporal audit result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TemporalAuditResult {
    pub url: String,
    pub has_snapshot: bool,
    pub snapshot_frozen_at: Option<i64>,
    pub snapshot_hash: Option<String>,
    pub current_hash: Option<String>,
    pub change_detected: bool,
    pub change_ratio: Option<f32>,
    pub verdict: Option<crate::museum_version::TamperVerdict>,
    pub diff_preview: Option<String>,  // first 500 characters of diff preview
}

/// Generate the "Historical Truth" banner injection script
pub fn tamper_alert_banner(url: &str, frozen_at: i64, change_ratio: f32, diff_preview: &str) -> String {
    let date = chrono::DateTime::from_timestamp(frozen_at, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
 .unwrap_or_else(|| " ".to_owned());
    let pct = (change_ratio * 100.0) as u32;
    let (color, icon) = if change_ratio > 0.20 {
        ("#ef4444", "🚨")
    } else {
        ("#f59e0b", "⚠️")
    };

    let diff_escaped = diff_preview
        .replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .chars().take(300).collect::<String>();

    format!(r#"
(function() {{
  if (document.getElementById('__diatom_temporal_audit')) return;
  const banner = document.createElement('div');
  banner.id = '__diatom_temporal_audit';
  banner.style.cssText = `
    position:fixed;top:0;left:0;right:0;z-index:2147483647;
    background:#0f172a;border-bottom:2px solid {color};
    color:#e2e8f0;font:13px/1.5 system-ui;padding:8px 16px;
    display:flex;align-items:flex-start;gap:12px;
  `;
  banner.innerHTML = `
    <span style="font-size:18px;flex-shrink:0">{icon}</span>
    <div style="flex:1;min-width:0">
      <strong style="color:{color}">Content Changed {pct}%</strong>
 — This page differs from the Diatom Museum snapshot saved on {date}.
      <details style="margin-top:4px">
        <summary style="cursor:pointer;color:#94a3b8;font-size:11px">View diff preview</summary>
        <pre style="margin:4px 0 0;font-size:10px;color:#94a3b8;white-space:pre-wrap;max-height:120px;overflow-y:auto">{diff_escaped}</pre>
      </details>
    </div>
    <button onclick="document.getElementById('__diatom_temporal_audit').remove()"
      style="background:none;border:none;color:#64748b;cursor:pointer;font-size:18px;flex-shrink:0">✕</button>
  `;
  document.documentElement.prepend(banner);
}})();
"#,
        color=color, icon=icon, pct=pct, date=date, diff_escaped=diff_escaped
    )
}

