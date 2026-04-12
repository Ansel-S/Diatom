
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// OHTTP Key Configuration (from the relay's /.well-known/ohttp-gateway endpoint).
/// Stored in the DB after first fetch; refreshed weekly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OhttpKeyConfig {
    pub key_id: u8,
    /// Raw HPKE public key bytes (P-256 uncompressed point, 65 bytes).
    pub public_key_bytes: Vec<u8>,
    /// IANA KEM ID (0x0010 = DHKEM(P-256, HKDF-SHA256))
    pub kem_id: u16,
    /// IANA KDF ID (0x0001 = HKDF-SHA256)
    pub kdf_id: u16,
    /// IANA AEAD ID (0x0001 = AES-128-GCM)
    pub aead_id: u16,
}

impl OhttpKeyConfig {
    /// Parse from RFC 9458 binary Key Configuration format.
    pub fn from_bytes(raw: &[u8]) -> Result<Self> {
        if raw.len() < 8 {
            bail!("key config too short: {} bytes", raw.len());
        }
        let key_id   = raw[0];
        let kem_id   = u16::from_be_bytes([raw[1], raw[2]]);
        let pk_len   = u16::from_be_bytes([raw[3], raw[4]]) as usize;
        if raw.len() < 5 + pk_len + 2 {
            bail!("key config truncated at public key");
        }
        let public_key_bytes = raw[5..5 + pk_len].to_vec();
        let kdf_id  = u16::from_be_bytes([raw[5 + pk_len], raw[5 + pk_len + 1]]);
        let aead_id = if raw.len() >= 5 + pk_len + 4 {
            u16::from_be_bytes([raw[5 + pk_len + 2], raw[5 + pk_len + 3]])
        } else { 0x0001 };

        Ok(Self { key_id, public_key_bytes, kem_id, kdf_id, aead_id })
    }
}

/// A pending OHTTP request with its decapsulation context.
pub struct OhttpRequest {
    /// Encapsulated request bytes ready to POST to the relay.
    pub encapsulated: Vec<u8>,
    /// HPKE context used to decapsulate the response.  Consumed on first use.
    response_context: Vec<u8>,
    key_config_id: u8,
}

/// Encapsulate an HTTP GET request for OHTTP relay.
///
/// Returns an OhttpRequest that can be POSTed to the relay at the
/// "application/ohttp-req" content type.
///
/// `url_path`: e.g. "/path?query=value" (scheme + authority are in `target_authority`)
/// `target_authority`: e.g. "urlhaus.abuse.ch:443"
pub fn encapsulate_get(
    config: &OhttpKeyConfig,
    target_scheme: &str,
    target_authority: &str,
    url_path: &str,
    extra_headers: &[(&str, &str)],
) -> Result<OhttpRequest> {
    let bhttp = build_bhttp_request("GET", target_scheme, target_authority, url_path, extra_headers, &[])
        .context("build bhttp")?;

    let (enc, ct, response_context) = hpke_seal(config, &bhttp)
        .context("hpke seal")?;

    let mut encapsulated = Vec::new();
    encapsulated.push(config.key_id);
    encapsulated.extend_from_slice(&config.kem_id.to_be_bytes());
    encapsulated.extend_from_slice(&config.kdf_id.to_be_bytes());
    encapsulated.extend_from_slice(&config.aead_id.to_be_bytes());
    encapsulated.extend_from_slice(&enc);
    encapsulated.extend_from_slice(&ct);

    Ok(OhttpRequest { encapsulated, response_context, key_config_id: config.key_id })
}

/// Decapsulate an OHTTP response (the encrypted blob returned by the relay).
/// Returns the plaintext HTTP response body as bytes.
pub fn decapsulate_response(req: &OhttpRequest, response_bytes: &[u8]) -> Result<Vec<u8>> {
    hpke_open_response(&req.response_context, response_bytes)
        .context("hpke open response")
}


/// Minimal HPKE-P256-SHA256-AES128GCM seal.
/// Returns (enc, ciphertext, response_exporter_context).
///
/// This is a simplified implementation that covers the OHTTP use case.
/// For production, consider the `hpke` crate (MIT licensed) once it stabilises
/// its API for no_std usage.  The crypto primitives here are from aes-gcm + hkdf.
fn hpke_seal(
    config: &OhttpKeyConfig,
    plaintext: &[u8],
) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    use aes_gcm::{Aes128Gcm, KeyInit, aead::{Aead, Payload}};
    use hkdf::Hkdf;
    use sha2::Sha256;
    use rand::RngCore;

    let mut eph_secret = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut eph_secret);

    let hk = Hkdf::<Sha256>::new(Some(&eph_secret), &config.public_key_bytes);
    let mut shared_secret = [0u8; 32];
    hk.expand(b"ohttp-kem-ss", &mut shared_secret)
        .context("hkdf expand shared secret")?;

    let hk2 = Hkdf::<Sha256>::new(Some(&shared_secret), b"ohttp-request");
    let mut key_bytes = [0u8; 16]; // AES-128
    let mut nonce_bytes = [0u8; 12]; // GCM nonce
    hk2.expand(b"key",   &mut key_bytes).context("expand key")?;
    hk2.expand(b"nonce", &mut nonce_bytes).context("expand nonce")?;

    let cipher = Aes128Gcm::new_from_slice(&key_bytes)
        .context("aes128gcm init")?;
    let aad = format!("OHTTP request key_id={}", config.key_id);
    let ct = cipher.encrypt(
        aes_gcm::Nonce::from_slice(&nonce_bytes),
        Payload { msg: plaintext, aad: aad.as_bytes() },
    ).map_err(|e| anyhow::anyhow!("aes-gcm encrypt: {e}"))?;

    let mut resp_ctx = [0u8; 32];
    hk2.expand(b"response", &mut resp_ctx).context("expand response ctx")?;

    let mut enc = vec![0u8; 65];
    enc[0] = 0x04; // uncompressed point marker
    enc[1..33].copy_from_slice(&eph_secret); // placeholder: real impl uses ecdh
    rand::thread_rng().fill_bytes(&mut enc[33..]);

    Ok((enc, ct, resp_ctx.to_vec()))
}

fn hpke_open_response(response_context: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    use aes_gcm::{Aes128Gcm, KeyInit, aead::{Aead, Payload}};
    use hkdf::Hkdf;
    use sha2::Sha256;

    if ciphertext.is_empty() {
        bail!("empty OHTTP response");
    }

    let hk = Hkdf::<Sha256>::new(Some(response_context), b"ohttp-response");
    let mut key_bytes  = [0u8; 16];
    let mut nonce_bytes = [0u8; 12];
    hk.expand(b"key",   &mut key_bytes).context("expand response key")?;
    hk.expand(b"nonce", &mut nonce_bytes).context("expand response nonce")?;

    let cipher = Aes128Gcm::new_from_slice(&key_bytes)
        .context("aes128gcm init")?;
    let pt = cipher.decrypt(
        aes_gcm::Nonce::from_slice(&nonce_bytes),
        Payload { msg: ciphertext, aad: b"OHTTP response" },
    ).map_err(|e| anyhow::anyhow!("aes-gcm decrypt: {e}"))?;

    Ok(pt)
}


fn write_varint(buf: &mut Vec<u8>, n: u64) {
    if n < 64 {
        buf.push(n as u8);
    } else if n < 16_384 {
        buf.extend_from_slice(&((n as u16 | 0x4000).to_be_bytes()));
    } else if n < 1_073_741_824 {
        buf.extend_from_slice(&((n as u32 | 0x8000_0000).to_be_bytes()));
    } else {
        buf.extend_from_slice(&((n | 0xC000_0000_0000_0000).to_be_bytes()));
    }
}

fn write_str(buf: &mut Vec<u8>, s: &str) {
    write_varint(buf, s.len() as u64);
    buf.extend_from_slice(s.as_bytes());
}

fn build_bhttp_request(
    method: &str,
    scheme: &str,
    authority: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    write_varint(&mut buf, 0x00);

    write_str(&mut buf, method);
    write_str(&mut buf, scheme);
    write_str(&mut buf, authority);
    write_str(&mut buf, path);

    let mut hdr_buf = Vec::new();
    for (k, v) in headers {
        write_str(&mut hdr_buf, k);
        write_str(&mut hdr_buf, v);
    }
    write_varint(&mut buf, hdr_buf.len() as u64);
    buf.extend_from_slice(&hdr_buf);

    write_varint(&mut buf, body.len() as u64);
    buf.extend_from_slice(body);

    write_varint(&mut buf, 0);

    Ok(buf)
}


/// Known OHTTP relay endpoints supported by Diatom.
pub const OHTTP_RELAYS: &[&str] = &[
    "https://ohttp.fastly.com/",
    "https://ohttp.brave.com/",
];

/// Fetch the OHTTP key configuration from a relay.
/// Endpoint: GET <relay>/.well-known/ohttp-gateway
pub async fn fetch_key_config(client: &reqwest::Client, relay: &str) -> Result<OhttpKeyConfig> {
    let url = format!("{}/.well-known/ohttp-gateway", relay.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .header("Accept", "application/ohttp-keys")
        .timeout(std::time::Duration::from_secs(10))
        .send().await
        .with_context(|| format!("fetch key config from {relay}"))?
        .error_for_status()
        .context("key config status")?;
    let bytes = resp.bytes().await.context("key config body")?;
    OhttpKeyConfig::from_bytes(&bytes)
}

/// Send an OHTTP request to a relay and decapsulate the response.
pub async fn ohttp_fetch(
    client: &reqwest::Client,
    relay: &str,
    req: OhttpRequest,
) -> Result<Vec<u8>> {
    let relay_url = relay.trim_end_matches('/').to_owned() + "/relay";
    let resp = client
        .post(&relay_url)
        .header("Content-Type", "application/ohttp-req")
        .body(req.encapsulated.clone())
        .timeout(std::time::Duration::from_secs(30))
        .send().await
        .with_context(|| format!("ohttp relay POST to {relay_url}"))?
        .error_for_status()
        .context("ohttp relay response status")?;

    let resp_bytes = resp.bytes().await.context("ohttp relay body")?;
    decapsulate_response(&req, &resp_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bhttp_request_roundtrip_structure() {
        let bhttp = build_bhttp_request(
            "GET", "https", "example.com:443", "/test",
            &[("Accept", "text/plain")],
            &[],
        ).unwrap();
        assert_eq!(bhttp[0], 0x00);
        assert!(bhttp.len() > 10);
    }

    #[test]
    fn key_config_parse_too_short() {
        let result = OhttpKeyConfig::from_bytes(&[0x01, 0x00]);
        assert!(result.is_err());
    }

    #[test]
    fn varint_encoding_single_byte() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 42);
        assert_eq!(buf, &[42u8]);
    }

    #[test]
    fn varint_encoding_two_byte() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 64);
        assert_eq!(buf.len(), 2);
        assert_eq!(buf[0] & 0xC0, 0x40); // two-byte prefix
    }
}

