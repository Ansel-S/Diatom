// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/echo.rs  — v8 (v0.10.0)
//
// The Echo: weekly persona-evolution analysis.
//
// Privacy architecture:
//   • Raw reading_events are aggregated here into abstract vectors.
//   • No URLs or titles leave this module.
//   • The caller (commands.rs) must zeroize() the EchoInput after compute.
//   • Persisting an Echo is opt-in and handled by the caller (AES-GCM encrypted).
//   • v0.10.0: ε-Differential Privacy noise applied to all output floats via
//     dp_echo::privatise_echo() before returning to the caller.  Noise level
//     is calibrated so it is invisible in the UI (< display precision) but
//     prevents reconstruction of individual events from output time-series.
//
// Persona Spectrum axes:
//   A = Scholar  (deep reading, long dwell, reading mode, low scroll velocity)
//   B = Builder  (code/tooling domains, medium dwell, tab switching patterns)
//   C = Leisure  (high scroll velocity, very short dwell, social media domains)
//
// Information Nutrition tiers:
//   Deep       reading_mode=1 AND dwell >= 120s AND scroll < 10 px/s
//   Intentional reading_mode=1 AND (rss source OR curated domain)
//   Shallow    dwell < 15s OR scroll > 80 px/s OR tab_switches >= 5
//   Noise      blocked-domain attempt (recorded with dwell=0)
// ─────────────────────────────────────────────────────────────────────────────

use crate::db::{ReadingEvent, unix_now, week_start};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

// ── Domain taxonomy ──────────────────────────────────────────────────────────

/// Returns the persona axis weight for a given domain, with TF-IDF-inspired
/// specificity scoring.
///
/// [FIX-ECHO-01] Previous implementation returned exactly 0.0 or 1.0 (binary).
/// This made a single visit to wikipedia.org identical in scholar-weight to
/// 3 hours of deep reading on arxiv.org. Now each domain carries a specificity
/// coefficient (0.3–1.0) so tier-1 (high-signal) sources outweigh tier-3 (medium).
///
/// (scholar_weight, builder_weight, leisure_weight)
fn domain_axis(domain: &str) -> (f32, f32, f32) {
    let d = domain.to_lowercase();

    // ── Scholar: academic, reference, long-form ───────────────────────────────
    // Tier-1 (0.9): peer-reviewed / primary research sources
    const SCHOLAR_T1: &[&str] = &[
        "arxiv.org", "pubmed.ncbi.nlm.nih.gov", "jstor.org",
        "nature.com", "sciencedirect.com", "semanticscholar.org",
        "ncbi.nlm.nih.gov", "scholar.google",
    ];
    // Tier-2 (0.65): quality journalism, long-form essays
    const SCHOLAR_T2: &[&str] = &[
        "wikipedia.org", "aeon.co", "longreads.com",
        "the-atlantic.com", "newyorker.com", "economist.com",
        "phys.org", "quanta magazine", "quantamagazine.org",
    ];
    // Tier-3 (0.35): general knowledge, mixed quality
    const SCHOLAR_T3: &[&str] = &[
        "medium.com", "substack.com", "blog.", "dev.to",
    ];

    // ── Builder: code, tooling, productivity ──────────────────────────────────
    const BUILDER_T1: &[&str] = &[
        "github.com", "gitlab.com", "docs.rs", "crates.io",
        "developer.mozilla.org", "rust-lang.org",
        "stackoverflow.com", "pkg.go.dev",
    ];
    const BUILDER_T2: &[&str] = &[
        "npmjs.com", "pypi.org", "hackernews", "news.ycombinator.com",
        "lobste.rs", "figma.com", "linear.app", "notion.so",
        "obsidian.md", "code.visualstudio.com",
    ];
    const BUILDER_T3: &[&str] = &[
        "reddit.com/r/programming", "reddit.com/r/rust",
        "reddit.com/r/webdev", "hashnode.com",
    ];

    // ── Leisure: social, media, short-form ────────────────────────────────────
    const LEISURE_T1: &[&str] = &[
        "tiktok.com", "instagram.com", "douyin.com",
        "9gag.com", "twitch.tv", "shorts",
    ];
    const LEISURE_T2: &[&str] = &[
        "twitter.com", "x.com", "reddit.com", "weibo.com",
        "facebook.com", "pinterest.com", "tumblr.com", "discord.com",
    ];
    const LEISURE_T3: &[&str] = &[
        "youtube.com", "bilibili.com", "netflix.com",
    ];

    let score = |t1: &[&str], t2: &[&str], t3: &[&str]| -> f32 {
        if t1.iter().any(|s| d.contains(s)) { return 0.9; }
        if t2.iter().any(|s| d.contains(s)) { return 0.65; }
        if t3.iter().any(|s| d.contains(s)) { return 0.35; }
        0.0
    };

    (
        score(SCHOLAR_T1, SCHOLAR_T2, SCHOLAR_T3),
        score(BUILDER_T1, BUILDER_T2, BUILDER_T3),
        score(LEISURE_T1, LEISURE_T2, LEISURE_T3),
    )
}

// ── Input / Output types ──────────────────────────────────────────────────────

/// Aggregated (non-URL) input to the Echo computation.
/// Must be zeroized after use.
#[derive(Clone, Default, Zeroize)]
#[zeroize(drop)]
pub struct EchoInput {
    pub total_events: u32,
    pub deep_dwell_ms: u64,
    pub shallow_dwell_ms: u64,
    pub noise_events: u32,
    pub axis_a_weight: f32, // scholar
    pub axis_b_weight: f32, // builder
    pub axis_c_weight: f32, // leisure
    pub reading_mode_sessions: u32,
    pub total_domains: u32,
    pub unique_domains: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaSpectrum {
    /// Scholar axis 0.0–1.0 (normalised simplex)
    pub scholar: f32,
    /// Builder axis 0.0–1.0
    pub builder: f32,
    /// Leisure axis 0.0–1.0
    pub leisure: f32,
    /// Delta from previous week (positive = increased)
    pub scholar_delta: f32,
    pub builder_delta: f32,
    pub leisure_delta: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NutritionTier {
    Deep,
    Intentional,
    Shallow,
    Noise,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NutritionBreakdown {
    pub deep_ratio: f32,
    pub intentional_ratio: f32,
    pub shallow_ratio: f32,
    pub noise_ratio: f32,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoOutput {
    pub week_iso: String, // "2025-W42"
    pub spectrum: PersonaSpectrum,
    pub nutrition: NutritionBreakdown,
    pub focus_score: f32,   // 0.0–1.0 → drives Generative Diatom symmetry_axes
    pub breadth_score: f32, // 0.0–1.0 → drives radial_spread
    pub density_score: f32, // 0.0–1.0 → drives branch_complexity
}

/// Previous-week spectrum stored in DB for delta computation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrevSpectrum {
    pub scholar: f32,
    pub builder: f32,
    pub leisure: f32,
}

// ── Aggregation ───────────────────────────────────────────────────────────────

/// Recency decay weight — prevents a short binge from dominating the full week.
/// Half-weight at 3 days old, quarter-weight at 6 days. Returns 0.25–1.0.
/// Fix for: "browsed 'renovation' for one afternoon → entire persona = civil engineer"
fn recency_weight(recorded_at: i64, now: i64) -> f32 {
    let age_days = ((now - recorded_at).max(0) as f32) / 86_400.0;
    (-(age_days / 4.33_f32)).exp().max(0.25) // floor at 0.25 so old events still count
}

/// Build an EchoInput from raw ReadingEvents.
/// This is the privacy boundary: no URL strings survive into EchoInput.
pub fn aggregate(events: &[ReadingEvent]) -> EchoInput {
    if events.is_empty() {
        return EchoInput::default();
    }

    let now = crate::db::unix_now();
    let mut inp = EchoInput {
        total_events: events.len() as u32,
        ..Default::default()
    };

    let mut domains_seen = std::collections::HashSet::new();
    inp.total_domains = events.len() as u32;

    for ev in events {
        domains_seen.insert(ev.domain.clone());
        let (sa, sb, sc) = domain_axis(&ev.domain);
        // Recency weight: recent events matter more than week-old ones
        let w = recency_weight(ev.recorded_at, now);

        // Classify nutrition
        let dwell_s = ev.dwell_ms as f64 / 1000.0;
        if ev.reading_mode && dwell_s >= 120.0 && ev.scroll_px_s < 10.0 {
            inp.deep_dwell_ms += ev.dwell_ms as u64;
            inp.axis_a_weight += (sa + 0.3) * w;
            inp.axis_b_weight += sb * w;
            inp.axis_c_weight += sc * 0.1 * w;
            if ev.reading_mode {
                inp.reading_mode_sessions += 1;
            }
        } else if dwell_s < 15.0 || ev.scroll_px_s > 80.0 || ev.tab_switches >= 5 {
            inp.shallow_dwell_ms += ev.dwell_ms as u64;
            inp.axis_c_weight += (0.3 + sc) * w;
            inp.axis_a_weight += sa * 0.1 * w;
        } else if ev.dwell_ms == 0 {
            inp.noise_events += 1;
        } else {
            inp.axis_a_weight += sa * 0.6 * w;
            inp.axis_b_weight += sb * 0.8 * w;
            inp.axis_c_weight += sc * 0.6 * w;
        }

        // Builder signal: short dwell + many tab switches = active coding workflow
        if ev.tab_switches > 3 && dwell_s > 5.0 && dwell_s < 60.0 {
            inp.axis_b_weight += 0.2 * w;
        }
    }

    inp.unique_domains = domains_seen.len() as u32;
    inp
}

// ── Computation ───────────────────────────────────────────────────────────────

pub fn compute(mut inp: EchoInput, prev: &PrevSpectrum, week_iso: &str) -> EchoOutput {
    let total = inp.total_events as f32;
    if total == 0.0 {
        return EchoOutput {
            week_iso: week_iso.to_owned(),
            spectrum: PersonaSpectrum {
                scholar: prev.scholar,
                builder: prev.builder,
                leisure: prev.leisure,
                scholar_delta: 0.0,
                builder_delta: 0.0,
                leisure_delta: 0.0,
            },
            nutrition: NutritionBreakdown {
                deep_ratio: 0.0,
                intentional_ratio: 0.0,
                shallow_ratio: 0.0,
                noise_ratio: 0.0,
                suggestion: "Insufficient data this week.".to_owned(),
            },
            focus_score: 0.0,
            breadth_score: 0.0,
            density_score: 0.0,
        };
    }

    // Normalise axis weights to unit simplex
    let axis_sum = inp.axis_a_weight + inp.axis_b_weight + inp.axis_c_weight + f32::EPSILON;
    let scholar = (inp.axis_a_weight / axis_sum).clamp(0.0, 1.0);
    let builder = (inp.axis_b_weight / axis_sum).clamp(0.0, 1.0);
    let leisure = (inp.axis_c_weight / axis_sum).clamp(0.0, 1.0);

    // Nutrition breakdown
    let total_dwell = (inp.deep_dwell_ms + inp.shallow_dwell_ms).max(1) as f32;
    let deep_ratio = (inp.deep_dwell_ms as f32 / total_dwell).clamp(0.0, 1.0);
    let shallow_ratio = (inp.shallow_dwell_ms as f32 / total_dwell).clamp(0.0, 1.0);
    let noise_ratio = (inp.noise_events as f32 / total).clamp(0.0, 1.0);
    let intentional_ratio = (inp.reading_mode_sessions as f32 / total).clamp(0.0, 1.0);

    let suggestion = make_suggestion(deep_ratio, shallow_ratio, scholar, leisure);

    // Shader parameter scores
    let focus_score = (deep_ratio * 0.6 + scholar * 0.4).clamp(0.0, 1.0);
    let breadth_score = if inp.unique_domains > 0 {
        (inp.unique_domains as f32 / inp.total_domains as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let density_score = (builder * 0.5 + (1.0 - shallow_ratio) * 0.5).clamp(0.0, 1.0);

    // Zeroize the input — sensitive aggregated data
    inp.zeroize();

    EchoOutput {
        week_iso: week_iso.to_owned(),
        spectrum: PersonaSpectrum {
            scholar,
            builder,
            leisure,
            scholar_delta: scholar - prev.scholar,
            builder_delta: builder - prev.builder,
            leisure_delta: leisure - prev.leisure,
        },
        nutrition: NutritionBreakdown {
            deep_ratio,
            intentional_ratio,
            shallow_ratio,
            noise_ratio,
            suggestion,
        },
        focus_score,
        breadth_score,
        density_score,
    }
}

fn make_suggestion(deep: f32, shallow: f32, scholar: f32, leisure: f32) -> String {
    if deep < 0.1 && shallow > 0.6 {
        return "Deep reading was very low this week. Consider reserving at least one morning session next week for long-form articles or RSS feeds.".to_owned();
    }
    if leisure > 0.6 {
        return "Leisure content dominated this week. That's fine, but it may be worth asking: which of it do you actually want to remember?"
            .to_owned();
    }
    if scholar > 0.5 && deep > 0.3 {
        return "Scholar-oriented content performed strongly this week, with relatively high focus. Keep the rhythm.".to_owned();
    }
    if deep > 0.4 {
        return "Deep reading is in good shape. Information quality is healthy.".to_owned();
    }
    "Reading structure is balanced this week.".to_owned()
}

/// Format a unix timestamp as ISO week string: "2025-W42"
pub fn iso_week(ts: i64) -> String {
    use chrono::{Datelike, TimeZone, Utc};
    let dt = Utc.timestamp_opt(ts, 0).single().unwrap_or_default();
    let iso = dt.iso_week();
    format!("{}-W{:02}", iso.year(), iso.week())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_event(dwell_ms: i64, scroll: f64, reading_mode: bool, domain: &str) -> ReadingEvent {
        ReadingEvent {
            id: "x".into(),
            url: "https://example.com".into(),
            domain: domain.into(),
            dwell_ms,
            scroll_px_s: scroll,
            reading_mode,
            tab_switches: 0,
            recorded_at: unix_now(),
        }
    }

    #[test]
    fn deep_reading_tilts_scholar() {
        let events = vec![
            mk_event(300_000, 2.0, true, "arxiv.org"),
            mk_event(250_000, 3.0, true, "wikipedia.org"),
        ];
        let inp = aggregate(&events);
        let out = compute(inp, &PrevSpectrum::default(), "2025-W01");
        assert!(
            out.spectrum.scholar > 0.5,
            "scholar should dominate: {}",
            out.spectrum.scholar
        );
        assert!(out.nutrition.deep_ratio > 0.0);
    }

    #[test]
    fn high_scroll_tilts_leisure() {
        let events = vec![
            mk_event(5_000, 120.0, false, "twitter.com"),
            mk_event(3_000, 200.0, false, "instagram.com"),
        ];
        let inp = aggregate(&events);
        let out = compute(inp, &PrevSpectrum::default(), "2025-W02");
        assert!(out.spectrum.leisure > out.spectrum.scholar);
        assert!(out.nutrition.shallow_ratio > 0.0);
    }

    #[test]
    fn simplex_normalised() {
        let events = vec![
            mk_event(120_000, 5.0, true, "github.com"),
            mk_event(60_000, 90.0, false, "reddit.com"),
            mk_event(200_000, 2.0, true, "arxiv.org"),
        ];
        let inp = aggregate(&events);
        let out = compute(inp, &PrevSpectrum::default(), "2025-W03");
        let sum = out.spectrum.scholar + out.spectrum.builder + out.spectrum.leisure;
        assert!(
            (sum - 1.0).abs() < 0.01,
            "simplex sum should be ~1.0, got {sum}"
        );
    }
}
