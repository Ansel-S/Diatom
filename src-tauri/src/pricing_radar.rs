// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/pricing_radar.rs
//
// Anti-Algorithmic Dynamic Pricing Radar
//
// Problem: E-commerce platforms dynamically adjust prices based on IP address,
// browsing history, and device info in real time. The price shown to one user
// ($100) may differ from what another user sees ($80).
//
// Implementation:
//   1. Local detection: parse price elements via JSON-LD / meta tags / common
//      CSS selectors.
//   2. P2P peer query: anonymously broadcast "who has seen this product price?"
//      via Nostr.
//      - Product identifier: de-parameterised canonical URL + ASIN / SKU hash.
//      - No user identity is transmitted; only the product hash is sent.
//   3. Price comparison: display "your price vs. network average" with diff.
//   4. Countermeasure hint: suggest rotating to a shadow-fingerprint IP when
//      significant price discrimination is detected.
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

// ── Price detection ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceSignal {
    pub url: String,
 pub canonical_product_id: String, // URL hash
    pub detected_price: Option<f64>,
    pub currency: Option<String>,
    pub detected_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceComparisonResult {
    pub product_id: String,
    pub your_price: f64,
    pub currency: String,
    pub peer_prices: Vec<PeerPrice>,
    pub network_avg: f64,
    pub network_min: f64,
 pub deviation_pct: f32, // = 
    pub alert_level: PriceAlertLevel,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerPrice {
    pub price: f64,
    pub reported_at: i64,
 pub node_count: usize, // 
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceAlertLevel {
 /// Prices match within ±3% — no significant deviation.
    Normal,
 /// Price deviation 3–10% — mild concern.
    Elevated,
 /// Price deviation 10–20% — likely dynamic pricing.
    High,
 /// Price deviation >20% — strong evidence of discriminatory pricing.
    Severe,
}

/// Derives a stable product identifier from a URL (de-parameterised canonical form + ASIN/SKU hash) for P2P queries.
pub fn canonical_product_id(url: &str) -> String {
    let normalized = url::Url::parse(url)
        .map(|mut u| {
            u.set_query(None);
            u.set_fragment(None);
            u.to_string()
        })
        .unwrap_or_else(|_| url.to_owned());
    hex::encode(blake3::hash(normalized.as_bytes()).as_bytes())[..16].to_owned()
}

/// Calculate alert level from price difference
pub fn compute_alert(your_price: f64, network_avg: f64) -> PriceAlertLevel {
    if network_avg <= 0.0 { return PriceAlertLevel::Normal; }
    let deviation = (your_price - network_avg) / network_avg * 100.0;
    match deviation as i32 {
        d if d <= 3  => PriceAlertLevel::Normal,
        d if d <= 10 => PriceAlertLevel::Elevated,
        d if d <= 20 => PriceAlertLevel::High,
        _            => PriceAlertLevel::Severe,
    }
}

/// Generate countermeasure recommendation text
pub fn generate_suggestion(alert: &PriceAlertLevel, deviation_pct: f32) -> String {
    match alert {
        PriceAlertLevel::Normal =>
 format!("Prices match within {:.1}% — no significant deviation detected.", deviation_pct),
        PriceAlertLevel::Elevated =>
 format!("⚠️ Price deviation {:.1}%. You may be seeing mild dynamic pricing.", deviation_pct),
        PriceAlertLevel::High =>
 format!("⚠️ Price deviation {:.1}%. Dynamic pricing likely. Consider using a shadow-fingerprint IP.", deviation_pct),
        PriceAlertLevel::Severe =>
 format!("🚨 Strong price discrimination detected ({:.1}% deviation). Strongly recommend rotating IP and clearing cookies.", deviation_pct),
    }
}

// ── JS price extraction script ────────────────────────────────────────────────

/// inject page versions
/// : JSON-LD / Open Graph / schema.org / selectors
pub const PRICE_EXTRACTOR_SCRIPT: &str = r#"
(function() {
  function extractPrice() {
    // 1. JSON-LD (schema.org/Offer)
    for (const script of document.querySelectorAll('script[type="application/ld+json"]')) {
      try {
        const data = JSON.parse(script.textContent);
        const offers = data.offers || (data['@graph'] || []).flatMap(g => g.offers || []);
        const offer = Array.isArray(offers) ? offers[0] : offers;
        if (offer && offer.price) {
          return { price: parseFloat(offer.price), currency: offer.priceCurrency || 'USD', source: 'json-ld' };
        }
      } catch(e) {}
    }

    // 2. meta tags (OG / itemprop)
    const metaPrice = document.querySelector('meta[property="product:price:amount"], meta[itemprop="price"]');
    if (metaPrice) {
      const currency = document.querySelector('meta[property="product:price:currency"]')?.content || 'USD';
      return { price: parseFloat(metaPrice.content), currency, source: 'meta' };
    }

    // 3. Common selectors (Amazon, Taobao, JD, Booking patterns)
    const selectors = [
      '.a-price .a-offscreen',  // Amazon
      '#priceblock_ourprice', '#priceblock_dealprice',
      '.price-box .price', '[itemprop="price"]',
      '.J-price strong', '.price-num',  // JD / Taobao
      '[data-testid="price-and-discount"] .prco-text',  // Booking
    ];
    for (const sel of selectors) {
      const el = document.querySelector(sel);
      if (el) {
        const text = el.textContent.replace(/[^\d.,]/g, '');
        const price = parseFloat(text.replace(',', '.'));
        if (!isNaN(price) && price > 0) {
          return { price, currency: 'USD', source: 'selector', selector: sel };
        }
      }
    }
    return null;
  }

  const result = extractPrice();
  if (result) {
    window.__diatom_price = result;
    window.dispatchEvent(new CustomEvent('diatom:price-detected', { detail: result }));
  }
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_id_strips_params() {
        let id1 = canonical_product_id("https://amazon.com/dp/B08N5WRWNW?ref=foo&tag=bar");
        let id2 = canonical_product_id("https://amazon.com/dp/B08N5WRWNW?ref=other");
        assert_eq!(id1, id2, "Same product URL with different params should yield same ID");
    }

    #[test]
    fn alert_level_computation() {
        assert!(matches!(compute_alert(100.0, 100.0), PriceAlertLevel::Normal));
        assert!(matches!(compute_alert(120.0, 100.0), PriceAlertLevel::High));
        assert!(matches!(compute_alert(150.0, 100.0), PriceAlertLevel::Severe));
    }
}
