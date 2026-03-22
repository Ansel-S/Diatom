// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/labs.rs  — v0.9.0
//
// Experimental Feature Registry — the backend of diatom://labs
//
// Labs is Diatom's answer to chrome://flags: a curated set of experimental
// features with honest stability ratings. The UI (src/ui/labs.html) renders
// them as precision instrument controls, not debug switches.
//
// Each lab has:
//   - A stable ID (snake_case, never renamed after ship)
//   - A human name and description
//   - A stability tier (Alpha / Beta / Stable)
//   - A risk level (Low / Medium / High) + description
//   - An enabled state (persisted in the meta table)
//   - Optional restartRequired flag
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LabStability {
    Alpha,   // Proof of concept — may break pages
    Beta,    // Functional but incomplete — may have edge cases
    Stable,  // Ready for daily use — waiting for graduation
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LabRisk {
    Low,     // Cosmetic / UX only — no data or privacy impact
    Medium,  // May break sites or change behaviour in noticeable ways
    High,    // Privacy trade-off, experimental crypto, or kernel-level change
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lab {
    pub id:               &'static str,
    pub name:             &'static str,
    pub description:      &'static str,
    pub category:         &'static str,
    pub stability:        LabStability,
    pub risk:             LabRisk,
    pub risk_note:        &'static str,
    pub enabled:          bool,
    pub restart_required: bool,
    pub added_in:         &'static str,  // "v0.9.0"
}

// ── Lab catalogue ─────────────────────────────────────────────────────────────

pub fn all_labs() -> Vec<Lab> {
    vec![
        // ── Privacy ──────────────────────────────────────────────────────────
        Lab {
            id:               "pqc_envelope",
            name:             "Post-Quantum Freeze Encryption",
            description:      "Encrypts Museum bundles with Kyber-768 + AES-256-GCM hybrid envelope, \
                               protecting archives against harvest-now-decrypt-later attacks.",
            category:         "Privacy",
            stability:        LabStability::Beta,
            risk:             LabRisk::Low,
            risk_note:        "Bundles encrypted with this flag cannot be decrypted by older Diatom versions.",
            enabled:          false,
            restart_required: false,
            added_in:         "v0.8.0",
        },
        Lab {
            id:               "ohttp_decoy",
            name:             "OHTTP Decoy Relay",
            description:      "Routes privacy-noise requests through Oblivious HTTP relays (Cloudflare, Fastly, Brave). \
                               The relay sees your IP but not the destination; the destination sees the request but not your IP.",
            category:         "Privacy",
            stability:        LabStability::Alpha,
            risk:             LabRisk::Medium,
            risk_note:        "Response decapsulation is not yet implemented — decoy requests only, no bidirectional data.",
            enabled:          false,
            restart_required: false,
            added_in:         "v0.8.0",
        },
        Lab {
            id:               "zkp_age_gate",
            name:             "Zero-Knowledge Age Verification",
            description:      "Proves you are over 18 (or other age threshold) to participating sites \
                               without revealing your birth year. Uses Ristretto255 Schnorr Sigma proofs.",
            category:         "Privacy",
            stability:        LabStability::Alpha,
            risk:             LabRisk::High,
            risk_note:        "Proof transport via HTTP headers is non-standard. Sites must opt-in to accept ZK-Proof headers.",
            enabled:          false,
            restart_required: false,
            added_in:         "v0.8.0",
        },
        Lab {
            id:               "timezone_spoof",
            name:             "Timezone Normalisation",
            description:      "Overrides JavaScript timezone APIs to report UTC, preventing locale-based \
                               fingerprinting. Breaks sites that rely on local time for display.",
            category:         "Privacy",
            stability:        LabStability::Beta,
            risk:             LabRisk::Medium,
            risk_note:        "Calendar apps, scheduling tools, and trading platforms may display incorrect times.",
            enabled:          false,
            restart_required: false,
            added_in:         "v0.9.0",
        },
        Lab {
            id:               "jitter_all_requests",
            name:             "Global Request Jitter",
            description:      "Adds 0–50ms cryptographic random delay to every outbound request, \
                               defeating timing correlation attacks on browsing patterns.",
            category:         "Privacy",
            stability:        LabStability::Stable,
            risk:             LabRisk::Low,
            risk_note:        "Adds up to 50ms latency per request. Imperceptible on most connections.",
            enabled:          false,
            restart_required: false,
            added_in:         "v0.8.0",
        },
        // ── AI ────────────────────────────────────────────────────────────────
        Lab {
            id:               "slm_server",
            name:             "Local AI Microkernel",
            description:      "Starts an OpenAI-compatible API server at 127.0.0.1:11435, making \
                               Diatom's curated SLMs available to any local app (VS Code, Obsidian, CLI tools).",
            category:         "AI",
            stability:        LabStability::Beta,
            risk:             LabRisk::Low,
            risk_note:        "Opens a local TCP port. Bound to loopback — not accessible from the network.",
            enabled:          false,
            restart_required: true,
            added_in:         "v0.9.0",
        },
        Lab {
            id:               "slm_extreme_privacy",
            name:             "AI Extreme Privacy Mode",
            description:      "Forces all AI inference to run in the Wasm sandbox. Disables Ollama \
                               and llama.cpp backends. The model can only see content already loaded \
                               in the current page — no filesystem, no network.",
            category:         "AI",
            stability:        LabStability::Alpha,
            risk:             LabRisk::Low,
            risk_note:        "Wasm inference is 5–20× slower than native. Only works for short prompts.",
            enabled:          false,
            restart_required: false,
            added_in:         "v0.9.0",
        },
        Lab {
            id:               "page_summarise",
            name:             "Instant Page Summariser",
            description:      "Adds a ⌘K shortcut that summarises the current page using the active \
                               SLM. Summary appears in the AI panel without leaving the page.",
            category:         "AI",
            stability:        LabStability::Beta,
            risk:             LabRisk::Low,
            risk_note:        "Page content is sent to the local model only — never to external servers.",
            enabled:          false,
            restart_required: false,
            added_in:         "v0.9.0",
        },
        // ── Performance ───────────────────────────────────────────────────────
        Lab {
            id:               "dynamic_tab_budget",
            name:             "Adaptive Tab Limit",
            description:      "Replaces the fixed 10-tab limit with a memory-aware formula that \
                               adjusts based on available RAM, screen width, and current tab weights. \
                               Ultrawide monitors get up to 13 tabs; phones get 3.",
            category:         "Performance",
            stability:        LabStability::Beta,
            risk:             LabRisk::Low,
            risk_note:        "Budget recalculates every 60 seconds. Opening many tabs quickly may \
                               temporarily exceed the limit before the next budget cycle.",
            enabled:          true,   // on by default in v0.9.0
            restart_required: false,
            added_in:         "v0.9.0",
        },
        Lab {
            id:               "golden_ratio_zones",
            name:             "Golden Ratio Tab Zones",
            description:      "Divides your tab budget into a Focus zone (61.8% of tabs, never \
                               auto-slept) and a Buffer zone (38.2%, aggressively hibernated). \
                               Focus tabs are marked with a subtle indicator.",
            category:         "Performance",
            stability:        LabStability::Beta,
            risk:             LabRisk::Low,
            risk_note:        "None. Cosmetic behavioural change only.",
            enabled:          true,
            restart_required: false,
            added_in:         "v0.9.0",
        },
        Lab {
            id:               "entropy_sleep",
            name:             "Entropy-Reduction Sleep",
            description:      "As tabs approach the budget limit, the auto-sleep timer shortens \
                               (from 10 min to 5 min at full capacity) and the heaviest tab is \
                               prioritised for hibernation.",
            category:         "Performance",
            stability:        LabStability::Stable,
            risk:             LabRisk::Low,
            risk_note:        "May surprise users who expect tabs to stay alive longer under load.",
            enabled:          true,
            restart_required: false,
            added_in:         "v0.9.0",
        },
        // ── Sync ─────────────────────────────────────────────────────────────
        Lab {
            id:               "crdt_museum_sync",
            name:             "P2P Museum Sync",
            description:      "Synchronises frozen page archives between your devices using \
                               OR-Set CRDTs. No server. Devices exchange a compact JSON diff \
                               that merges without conflicts.",
            category:         "Sync",
            stability:        LabStability::Alpha,
            risk:             LabRisk::Medium,
            risk_note:        "Requires manual export/import until the Mesh transport layer is complete.",
            enabled:          false,
            restart_required: false,
            added_in:         "v0.8.0",
        },
        // ── Interface ─────────────────────────────────────────────────────────
        Lab {
            id:               "bloom_startup",
            name:             "Bloom Startup Animation",
            description:      "Displays a procedural geometric animation on first load. \
                               Pure CSS — zero JavaScript, zero performance cost.",
            category:         "Interface",
            stability:        LabStability::Stable,
            risk:             LabRisk::Low,
            risk_note:        "None.",
            enabled:          true,
            restart_required: false,
            added_in:         "v0.9.0",
        },
        Lab {
            id:               "screen_gravity",
            name:             "Screen Gravity Tab Ceiling",
            description:      "Automatically adjusts the maximum tab count based on display width: \
                               3 tabs on phone, 8 on 13\" laptop, 10 on desktop, 13 on ultrawide.",
            category:         "Interface",
            stability:        LabStability::Beta,
            risk:             LabRisk::Low,
            risk_note:        "The ceiling adjusts within 60 seconds of a window resize.",
            enabled:          true,
            restart_required: false,
            added_in:         "v0.9.0",
        },
    ]
}

// ── Labs store ────────────────────────────────────────────────────────────────

pub fn load_labs(db: &crate::db::Db) -> Vec<Lab> {
    let mut labs = all_labs();
    for lab in &mut labs {
        let key = format!("lab_{}", lab.id);
        if let Some(val) = db.get_setting(&key) {
            lab.enabled = val == "true";
        }
    }
    labs
}

pub fn set_lab(db: &crate::db::Db, id: &str, enabled: bool) -> anyhow::Result<bool> {
    // Validate the lab exists
    if !all_labs().iter().any(|l| l.id == id) {
        anyhow::bail!("unknown lab id: {}", id);
    }
    let key = format!("lab_{}", id);
    db.set_setting(&key, if enabled { "true" } else { "false" })?;
    // Return whether a restart is required
    let restart = all_labs().iter()
        .find(|l| l.id == id)
        .map(|l| l.restart_required)
        .unwrap_or(false);
    Ok(restart)
}

pub fn is_lab_enabled(db: &crate::db::Db, id: &str) -> bool {
    let key = format!("lab_{}", id);
    db.get_setting(&key)
        .map(|v| v == "true")
        .unwrap_or_else(|| {
            all_labs().iter().find(|l| l.id == id).map(|l| l.enabled).unwrap_or(false)
        })
}
