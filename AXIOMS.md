# Diatom — Axioms

> This document is Diatom's axiomatic foundation. It defines what Diatom is and
> what Diatom will never become. Every feature, every architectural decision, and
> every partnership proposal must pass review against these axioms. If a change
> conflicts with any axiom, the change is rejected — the axiom is never amended.
>
> "Axiom" is chosen deliberately. These are not philosophical preferences open to
> reinterpretation. They are load-bearing constraints; violating one collapses the
> integrity of the whole. The *reasoning* behind each axiom lives in README.md.

---

## I. Sovereignty Axioms

### Axiom 1 — No Centralised Sync Server
Diatom will never operate a server that stores user bookmarks, history, or any
browsing artefact. P2P mesh sync is slow and requires both devices online — that
is an acceptable trade-off. A central server acquires a god's-eye view of user
behaviour; that is not.

### Axiom 2 — No Skip Button on Zen Mode
The 50-character unlock gate is a ritual, not a speed bump. Making the act of
breaking focus *conscious* is the feature. A skip button destroys its premise.
This axiom extends to all commitment gates in Diatom — no bypass paths.

### Axiom 3 — Predictive Features Are Opt-In, Never Default
URL prefetch, search suggestions, predictive scrolling — each trades privacy for
milliseconds. The default posture is always the most conservative. Intent remains
completely private until the user explicitly acts.

---

## II. Legal Axioms

### Axiom 4 — No Official Platform-Specific Block Rule Distribution
Diatom may publish its blocking engine as open source. The official repository
will never ship curated rule files targeting specific platforms by name.

### Axiom 5 — No DRM Circumvention
Widevine's absence is a deliberate boundary, not a technical limitation to
engineer around.

---

## III. Technical Axioms

### Axiom 6 — The Microkernel Stays Clean
No business logic enters the core. If a feature can be hot-swapped as a module,
it does not enter the kernel.

**Binary size hard ceiling: 10 MB** for the Tauri shell binary.
The DevPanel (`diatom-devpanel`) is a separate process with its own budget.
Combined RSS ceiling: **55 MB**.

### Axiom 7 — No Chromium
Servo first. WebView2 / WKWebView second. Blink never.

### Axiom 8 — Zero Telemetry
"Anonymous data" does not exist. Diatom's codebase calls no analytics service.
The integrated Zed editor core strips all Zed telemetry crates at build time
(enforced by `scripts/strip-zed.sh`).

```
// These will never appear in the Diatom codebase:
// telemetry::event!(...)
// sentry::capture_message(...)
// reqwest::get("https://analytics.example.com/event")
```

### Axiom 9 — URL Stripping Is Rule-Based, Never AI-Delegated
Tracking-parameter removal is driven exclusively by curated Regex / filter lists
(`src-tauri/src/url_stripper.rs`). Delegating this decision to a model risks two
failure modes: stripping a Session ID that breaks login, or missing a novel
tracker hidden in an unusual parameter name. Static, audited rules are the only
acceptable mechanism.

### Axiom 10 — Fingerprinting Countermeasure Is Normalisation, Not Noise
Diatom normalises browser fingerprints to values statistically consistent with a
large population of common hardware configurations. It does not inject random
noise. Noise-based approaches produce detectable entropy signatures and can break
site functionality. Normalisation is invisible and deterministic.

---

## IV. Commercial Axioms

### Axiom 11 — No Default Search Engine Revenue Deals

### Axiom 12 — No Monetisation of User Attention

---

## V. Community Axioms

### Axiom 13 — Core Logic Stays Open Source

### Axiom 14 — No Module Registry Monopoly
Diatom accepts plugins from local paths, IPFS CIDs, and user-defined HTTP
sources. The Zed extension registry is treated as a read-only public index —
metadata is fetched without sending telemetry or account tokens.

---

## VI. Zed Integration Axioms (v0.14.3)

### Axiom 15 — The Editor Core Is a Tool, Not a Product Dependency
Diatom integrates Zed's **editor core** (GPUI, Rope, LSP, WASM extensions), not
Zed-the-product. Every Zed component that touches cloud infrastructure (collab,
telemetry, cloud AI, auto-update) is excised before build via
`scripts/strip-zed.sh`. The stripped workspace lives at `zed-core/`.

### Axiom 16 — The AI Backend Is Always Local
DevPanel AI features are served exclusively by Diatom's `slm.rs` (:11435).
Resonance shares context with the external Zed IDE via a Unix Domain Socket
(`~/.diatom/resonance.sock`). No cloud AI crate enters the build graph.

### Axiom 17 — Zed Collaboration Features Are Permanently Absent
LiveKit, collab, channel, call — these crates are blacklisted at build time.
Diatom is a single-user, zero-network-surface tool.

### Axiom 18 — Zed Official Plugins Are Welcome; Zed Account Is Not
Any `.wasm` plugin conforming to the Zed extension API runs inside Diatom's
extension sandbox. No Zed account, token, or cloud registry authentication is
required or requested.

### Axiom 19 — New Zed Releases Use Whitelist Integration
Each Zed release triggers `scripts/strip-zed.sh`. New crates not on the
whitelist are not imported until a human reviewer explicitly adds them. The
script fails loudly if unreviewed crates appear.

---

## Permanent Black Zone

| Constraint | Rationale |
|---|---|
| Zen Mode 50-char gate | Ritual: removing it removes the feature itself |
| The Echo shows no flattering spin | The mirror must be honest, even when uncomfortable |
| Mesh P2P, no central server | Centralisation = god's-eye view = something that can be sold |
| WebUSB / WebMIDI disabled | The browser does not touch physical hardware |
| Fingerprint normalisation (not noise) | Deterministic, invisible, does not break site function |
| Service Worker force-suspended | Miss a push notification before allowing background tracking |
| 0-RTT and prefetch disabled | Milliseconds are not worth trading for intent privacy |
| No skip buttons anywhere | Skip buttons are a betrayal of feature seriousness |
| Shell binary ≤ 10 MB | The browser shell stays lightweight |
| Combined RSS ≤ 55 MB | Physical constraint on a lightweight promise |
| Local AI only (no cloud fallback) | Cloud AI = your thoughts on someone else's server |
| Zed telemetry / collab = build error | strip-zed.sh enforces this at source level |
| URL stripping = Regex rules only | AI-generated rules risk breaking login sessions |
