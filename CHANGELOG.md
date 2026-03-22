# Diatom — Development Log

A vibe-coded browser built from first principles, one version at a time.

---

## v0.9.0 — The Scheduler (current)

**Theme:** From information consumer to compute orchestrator.

- **Local AI Microkernel** (`slm.rs`): OpenAI-compatible API at `127.0.0.1:11435`. Three curated models: Qwen 2.5 3B (fast), Phi-4 Mini (balanced), Gemma 3 4B (deep context). Auto-detects Ollama/llama.cpp; falls back to Candle Wasm. Other local apps can use Diatom as their AI backend.
- **Extreme Privacy AI Mode**: When active, all inference is sandboxed to Candle Wasm — no filesystem access, no network, only in-memory page content.
- **Adaptive Tab Budget** (`tab_budget.rs`): Three interlocking models. Resource-Aware Scaling: T_max = ⌊M_available × ρ / ω_avg⌋. Golden Ratio Zones: Focus (61.8%) / Buffer (38.2%). Screen Gravity: 3 tabs on phone, 8 on laptop, 10 on desktop, 13 on ultrawide. Entropy-reduction sleep timer shortens as pressure rises.
- **`diatom://labs`** (`labs.rs` + `src/ui/labs.html`): 14 experimental features — AI, performance, privacy, sync, interface. Instrument-quality UI: Instrument Serif + DM Mono, paper-and-ink palette, per-card risk assessment.
- **P0 compile fixes**: Added 6 previously-missing modules — `blocker.rs`, `tabs.rs`, `privacy.rs`, `totp.rs`, `trust.rs`, `rss.rs`.
- **Browser chrome**: `index.html`, `diatom.css` — precision UI with zen mode slide, AI panel, budget indicator.
- License changed from MIT to **BUSL-1.1** (3-year protection, Change License → MIT).

---

## v0.8.0 — Bug Eradicator

**Theme:** Fix every fatal defect inherited from the research branch.

- Fixed `commands.rs` stray token `(§7.1 mitigation)` that prevented compilation.
- Fixed `decoy.rs` `block_on` deadlock inside Tokio runtime.
- Fixed `db.rs` `settings` → `meta` table rename migration for v7.0 data preservation.
- Ported `crdt.rs` from v8 research: OR-Set + LWW-Register Museum sync, BLAKE3 chunk integrity, temporal jitter injection.
- Rewrote `zkp.rs`: replaced integer M127 arithmetic (no ZK properties) with Ristretto255 Schnorr Sigma proofs — verify function now actually verifies.
- Hardened `ohttp.rs`: honest compliance documentation, fire-and-forget semantics clearly labelled, response decapsulation marked TODO.
- Ported `pqc.rs` from v8 with conditional compilation guard (`--features pqc`).
- Added `PHILOSOPHY.md` (12 prohibitions + permanent black zone table).

---

## v0.7.0 — Frontier Tech

**Theme:** Ship the most advanced privacy and sync primitives in any open-source browser.

- The Echo: weekly personality spectrum (Scholar / Builder / Leisure) from reading behaviour.
- E-WBN Freeze: AES-256-GCM encrypted offline page archives (Museum).
- War Report: narrative anti-tracking statistics.
- CRDT Museum Sync (research): OR-Set conflict-free merge for offline devices.
- Post-quantum cryptography stub (research): Kyber-768 + Dilithium-3.
- Oblivious HTTP relay integration (research).
- Zero-knowledge proof identity protocol (research).

---

## v0.6.0 — Architecture & Philosophy

**Theme:** Code modularisation and product ethics locked in writing.

- Unified `AppState` — eliminated N independent `Arc<Mutex<_>>` locks.
- Single `unix_now()`, `new_id()`, `domain_of()` — removed 5 duplicate copies each.
- `compliance.rs`: consent gating for legally complex features.
- `storage_guard.rs`: LRU eviction with configurable budget.
- `a11y.rs`: ARIA injection and keyboard navigation.
- `compat.rs`: legacy site detection with tracking-clean system browser handoff.
- Product philosophy drafted: "A tool with boundaries is more trustworthy than a tool that is everywhere."

---

## v0.5.0 — Offline & UI

**Theme:** The browser works when the internet doesn't.

- Museum v1: snapshot-based offline reading.
- Service Worker with offline fallback strategy.
- Reading mode with typographic optimisation.
- Zen mode with 50-character unlock ritual.
- DOM Crusher: CSS selector–based element removal.

---

## v0.4.0 — Mesh Networking

**Theme:** Devices talk to each other without a server.

- Mesh P2P: WebRTC-based local network tab syncing.
- Dead Man's Switch: time-gated workspace self-destruct.
- RSS reader with workspace isolation.
- Threat intelligence via Quad9 DoH.

---

## v0.3.0 — Workspaces & Identity

**Theme:** The browser knows who you are in each context.

- Workspace isolation: cookies, history, and storage partitioned per workspace.
- Reading mode: Readability algorithm, configurable typography.
- Trust levels: per-domain capability grants (L0–L3).
- TOTP/HOTP 2FA manager built into the chrome.

---

## v0.2.0 — Speed & Privacy

**Theme:** Faster pages through less data.

- Aho-Corasick tracker blocker with O(n) matching.
- LZ4 ZRAM tab compression (80 MB → ~20 MB per deep-sleep tab).
- Shallow + Deep sleep tab lifecycle.
- UTM/fbclid/gclid parameter stripping.
- HTTPS upgrade for all non-localhost origins.

---

## v0.1.0 — The Spark

**Theme:** An extremely private browser that uses less RAM.

- Tauri shell with a single WebView.
- SQLite history with workspace partitioning.
- Basic tab management with LRU sleep candidates.
- Fingerprint noise injection (Canvas, WebRTC, Battery, USB).
- "Why does my browser need 4 GB of RAM?" — the question that started this.
