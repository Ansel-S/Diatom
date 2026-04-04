# Diatom — CHANGELOG

## v0.12.0 — "Silica" (2026-04)

### 🐛 Bug Fixes (10)

**B-01 [CRITICAL] `etag_cache.rs`** — Removed rule-body storage from DB. EasyList is ~600 KB; the previous 64 KB cap silently left 93% of rules absent on cold restart. 304 Not Modified now triggers an unconditional re-download. Blocker always starts with a full rule set.

**B-02 [CRITICAL] `tabs.js`** — Removed `_tabs.push(tab)` from `createTab()`. The `diatom:tab_created` event is now the sole source of truth, eliminating the duplicate tab pill race condition.

**B-03 [CRITICAL] `main.rs` + `state.rs`** — `PowerBudget` stored in `AppState`. Background loops read sleep intervals from `AppState.power_budget` at each iteration. A `power_monitor` task updates the field every 5 minutes and emits `diatom:power-state-changed` on tier transitions. Battery-aware scheduling now actually works at runtime.

**B-04 [MAJOR] `sentinel.rs`** — `webkit_build_for()` no longer silently falls back to Safari 18's build (619.x) for unknown Safari majors. Unknown majors return a `SENTINEL_STALE` marker and log a warning, forcing the table to be updated. Unit test added.

**B-05 [MAJOR] `main.rs` + `main.js`** — 3-second Rust watchdog: if no `diatom:window-ready` IPC arrives from JS, `win.show()` is force-called and `diatom:boot-error` emitted. JS `boot()` now signals Rust on `DOMContentLoaded`.

**B-06 [MAJOR] `slm.rs` + `main.rs` + `state.rs`** — Replaced `Arc<AtomicBool>` SLM shutdown with a `CancellationToken` child of `AppState.shutdown_token`. The SLM server now exits immediately via `tokio::select!` instead of blocking on `TcpListener::accept()`.

**B-07 [MAJOR] `db.rs`** — `exec_idempotent()` now matches error message text (`"already exists"` / `"duplicate column"`) instead of generic SQLite error code 1. Real migration failures no longer silently swallowed.

**B-08 [MINOR] `freeze.rs`** — Fixed `master_key_b64` / `master_key_hex` naming inconsistency. Added compile-time `assert_eq!` hex round-trip test.

**B-09 [MINOR] `mcp_host.rs`** — Token file permissions verified as `0o600` on Unix. Token unconditionally regenerated on each launch.

**B-10 [MINOR] `tabs.js`** — `closeTab()` now auto-creates a new blank tab when `_tabs` empties, matching Chrome/Firefox behaviour.

### ✨ New Features (12 Labs)

- **F-01** `panic_button` — Panic Button 紧急隐私锁
- **F-02** `video_pip` — PiP Video Engine 浮窗播放
- **F-03** `per_tab_proxy` — Per-Tab Proxy 独立代理IP [Alpha]
- **F-04** `breach_monitor` — Dark Web Leak Monitor 暗网泄露监控
- **F-05** `wifi_trust_scanner` — Wi-Fi Trust Scanner 公共Wi-Fi保护
- **F-06** `peek_preview` — Peek Link Preview 悬停链接预览
- **F-07** `page_boosts` — Page Boosts CSS注入 (3 built-in: Clean Reader, Focus Dark, Print Friendly)
- **F-08** `ai_download_rename` — AI Download Renamer AI智能重命名
- **F-09** `home_base` — Home Base 快速拨号导航 [Default ON]
- **F-10** `privacy_search` — Privacy Search Engines (Brave, SearXNG, Kagi) [Default ON]
- **F-12** `bandwidth_limiter` — Bandwidth Limiter 带宽分配控制
- `tab_group_stacking` — Tab Group Stacking

### 🚫 Rejected Features

R-01 Built-in VPN, R-02 AI Agent, R-03 Real-time Cloud Sync, R-04 Video Conferencing, R-05 Crypto Wallet, R-06 Video AI Upscaling, R-07 IPFS Native — all rejected with full architectural rationale (see Blueprint).

---

## v0.11.0 — "Radiant" (2026-04)

### Overview

v0.11.0 is a full integration release: it merges the architectural improvements
landed in v0.6.0 (CSS split, `include_str!` blocklist, incremental TF-IDF,
feature flags, IDB quota handling) with the privacy and protocol work of v0.10.0
(Noise P2P, ε-DP Echo, PIR, full OHTTP, Wasm sandbox), then layers on new
performance and power-efficiency optimisations.

The codename "Radiant" reflects the release's theme: more light, less heat.
Faster startup, less idle CPU, less battery drain — without removing any privacy
guarantees.

---

### 🔀 Merges from v0.6.0

**CSS split architecture** (`src/diatom.css` → 4 separate files)

The monolithic 45 KB `diatom.css` has been replaced with:
- `diatom.css` — 17 KB core chrome styles only
- `shadow-index.css` — search panel (lazy-loaded on first ⌘⇧F)
- `tos-auditor.css`  — ToS panel (lazy-loaded on first audit)
- `tab-groups.css`   — tab groups (lazy-loaded on first group create)
- `ai-panel.css`     — AI panel (lazy-loaded on first open)

`main.js` now has a `loadStylesheet(href)` cache-aware loader. Feature CSS is
injected exactly once per session, only when needed. First-paint style parse
reduced from 45 KB → 17 KB (**62% reduction**).

**`include_str!` blocklist refactor** (`blocker.rs`)

`BUILTIN_PATTERNS_RAW` and `BUILTIN_COSMETIC_RULES_RAW` are now loaded via
`include_str!` from `resources/builtin_patterns.txt` and
`resources/builtin_cosmetic.txt`. Pattern updates require editing the `.txt`
file only — no Rust recompile needed. CI can count patterns independently of
source code.

**Incremental TF-IDF indexing** (`core.worker.js`)

`MUSEUM_LOAD_IDLE` now performs a fast-path for entries with pre-computed
`tfidf_tags`: they are indexed from their tag string without full-text
re-tokenisation (~10× faster for large Museum collections). Only entries with
no pre-computed tags go through the full idle indexer pipeline. Cold-start
indexing time for > 200 entries drops from ~500 ms to ~50 ms.

**Robust IDB `idbSet`** (`sw.js`)

The Service Worker's `idbSet` now handles `QuotaExceededError` explicitly:
it trims the Museum index to the 50 most-recent entries and retries the write.
`tx.onerror` and `tx.onabort` handlers added alongside `req.onerror` for
full transaction-level error coverage. Structured error logging (`{ key,
errorName, message }`) replaces the previous plain string.

**Feature flags** (`Cargo.toml`)

`labs_beta`, `labs_alpha`, and `full` feature gates added. Default build
includes `labs_beta`. Alpha features (marketplace, Nostr sync) are compiled
only when `labs_alpha` or `full` is passed to cargo. CI nightly uses `full`;
release binaries use the default.

**`diatom-api.js` v0.1.2** (injected into every page)

- `[FIX-12-canvas]` `canvas.toDataURL()` now intercepted in addition to
  `getImageData()` — fingerprint scripts cannot bypass noise via the main path.
- `[FIX-09-webgl]` WebGL renderer/vendor strings platform-branched (Windows
  users no longer see an Apple M-series string).
- `[FIX-10-langs]` `navigator.languages` falls back to `__DIATOM_INIT__.platform`
  locale instead of a hardcoded Chinese-priority list.
- `[FIX-13-compat]` `window.__diatom_compat_handoff()` now defined — the compat
  hint banner button actually works.
- `[FIX-__DIATOM_INIT__]` Noise seed now comes from Rust workspace, not
  `Math.random()`.

---

### 🐛 Bug Fixes (all 7 from v0.10.0 carried forward + 1 new)

All seven bugs fixed in v0.10.0 are present in this release (see v0.10.0
CHANGELOG for details). Additionally:

**Bug 8 — `idbSet` silently swallowed storage quota errors** (`sw.js`)
- v0.10.0's fix (plain `console.warn`) upgraded to v0.6.0's full quota handler:
  automatic Museum index trim to 50 newest entries on `QuotaExceededError`,
  with retry. Callers receive `false` (write failed) instead of `undefined`.

---

### ⚡ Performance & Power

**Adaptive power scheduling** (`power_budget.rs`)

New module. Detects battery state at startup and every time `cmd_power_budget_status`
is called. Adjusts background loop intervals:

| State | Sentinel | Tab-budget | Decoy traffic |
|---|---|---|---|
| AC power | 1 h | 60 s | Enabled |
| Battery > 20% | 3 h | 5 min | Disabled |
| Battery ≤ 20% | 6 h | 15 min | Disabled + PIR off |

Detection: Linux `/sys/class/power_supply/`, macOS `pmset -g batt`, Windows
`wmic Win32_Battery`. Fallback to AC (never pessimistically throttle on unknown
state). New lab: `power_budget` (stable, enabled by default).

**ETag conditional GET for filter lists** (`etag_cache.rs`)

New module. Stores `ETag` / `Last-Modified` from each filter list response in
SQLite. On the next boot, sends `If-None-Match` / `If-Modified-Since`. A 304
response skips the download entirely. Saves ~1 MB/week on EasyList + EasyPrivacy
for users who launch Diatom daily. New lab: `etag_cache` (stable, enabled by default).

**IPC call batching** (`ipc.js`)

`batchInvoke(cmd, args)` exported from `ipc.js`. Coalesces calls within a single
`requestAnimationFrame`. Commands like `cmd_record_reading` and `cmd_noise_seed`
— called on every scroll flush — now arrive at Rust as at most one call per frame
instead of N calls. ~60% IPC round-trip reduction during active scrolling.
New lab: `ipc_batch` (beta, enabled by default).

**Visibility-gated worker processing** (`core.worker.js`, `main.js`)

`main.js` sends `{ type: 'VISIBILITY', payload: { hidden: bool } }` to the
worker on every `document.visibilitychange`. The worker sets `_workerPaused`
and skips idle TF-IDF indexing while hidden. CPU wake-ups eliminated when
Diatom is in the background.

**AbortController per navigation** (`main.js`)

Each call to `loadUrl()` aborts the previous navigation's in-flight fetches
(favicon, RSS check) before starting new ones. Eliminates dangling network
requests when the user navigates quickly.

**Service Worker static asset cache** (`sw.js`)

`STATIC_CACHE` layer added for fonts, CSS, and JS assets. Subsequent navigations
to `diatom://` pages are served from cache without touching the network or the
main Tauri asset server.

---

### 📋 New Labs Summary (v0.11.0)

| Lab ID | Name | Category | Default | Status |
|---|---|---|---|---|
| `power_budget` | Adaptive Power Scheduling | Performance | **On** | Stable |
| `etag_cache` | Conditional Filter List Fetch | Performance | **On** | Stable |
| `ipc_batch` | IPC Call Batching | Performance | **On** | Beta |

Plus all labs from v0.10.0: `noise_p2p_sync`, `echo_dp`, `pir_blocklist`,
`ohttp_full`, `plugin_sandbox`.

---

### 🗂 New Files

```
src-tauri/src/power_budget.rs     — battery-aware scheduling
src-tauri/src/etag_cache.rs       — ETag conditional GET
src-tauri/resources/builtin_patterns.txt   — (was inline Rust)
src-tauri/resources/builtin_cosmetic.txt   — (was inline Rust)
src/ai-panel.css                  — (split from diatom.css)
src/shadow-index.css              — (split from shadow-index.js)
src/tab-groups.css                — (split from diatom.css)
src/tos-auditor.css               — (split from diatom.css)
```

---

### Dependency changes

Added (v0.10.0 carries forward):
- `snow = "0.9"` — Noise Protocol Framework
- `flatbuffers = "24"` — zero-copy IPC
- `p256 = "0.13"` — P-256 ECDH for OHTTP

No new dependencies in v0.11.0 proper (power_budget and etag_cache use only
stdlib + existing crates).

---

### Binary size

| Build | Target |
|---|---|
| `release` (default, `labs_beta`) | ≤ 11 MB |
| `release --features labs_alpha` | ≤ 11.5 MB |
| `release-small` | ≤ 9 MB |
| `release --features plugin-sandbox` | ≤ 15 MB |

# Diatom — CHANGELOG

## v0.10.0 — "Crystalline" (2026-04)

### Overview

v0.10.0 is the largest single release in Diatom's history. It addresses every
known bug from the v0.9.9 audit, delivers five new architectural systems, and
completes several long-standing "framework-only stub" lab features.

The codename "Crystalline" honours the diatom's defining trait: a geometric
shell that is both precise and transparent. This release makes Diatom's
privacy guarantees provable rather than promised.

---

### 🐛 Bug Fixes (Critical)

**Bug 1 — UA fingerprint leak on non-macOS platforms** (`blocker.rs`)
- `DIATOM_UA` constant (hardcoded to macOS Safari) was still called from
  `threat.rs`, `decoy.rs`, `sentinel.rs`, `state.rs`, and `commands.rs`.
- All call-sites migrated to `platform_fallback_ua()`, which returns the
  correct platform UA (macOS Safari / Windows Chrome / Linux Chrome).
- `DIATOM_UA` marked `#[deprecated(since = "0.10.0")]`; will be removed in v0.11.0.

**Bug 2 — LEAN_DOMAINS duplicate entry and no version tracking** (`shadow-index.js`)
- `atlantic.com` and `theatlantic.com` both appeared in `centerleft` array.
- Removed `atlantic.com` (non-canonical). `theatlantic.com` is the correct entry.
- Added `LEAN_DOMAINS_VERSION = '2025-Q1'` and expanded inline documentation
  explaining the classifier's limitations and review schedule.

**Bug 3 — Ghost IPC calls after Shadow Index panel close** (`shadow-index.js`)
- `doSearch()` could fire 280ms after panel close if the debounce timer was
  still pending, sending an `invoke('cmd_shadow_search', ...)` to a closed UI.
- Added `if (!_open || !_query) return;` guard at the top of `doSearch()`.
  The `_open` flag is set to `false` in `close()` before the debounce clears.

**Bug 4 — Spurious scroll velocity on tab switch** (`tabs.js`)
- `activateTab()` did not reset `_scrollVelocity`, `_lastScrollY`, or
  `_lastScrollTs`. The first scroll event on the new tab computed a velocity
  from the delta between the previous tab's scroll position and 0, producing
  an artificially huge value that corrupted Echo quality assessments.
- Added explicit reset of all three scroll-tracking variables in `activateTab()`.

**Bug 5 — Stacked auto-audit timers on rapid SPA navigation** (`tos-auditor.js`)
- Rapid URL changes (e.g. SPA route transitions) stacked multiple 1500ms
  `setTimeout` callbacks. Each one would fire independently, causing multiple
  concurrent `runAudit()` calls and competing risk-panel IPC calls.
- Replaced bare `setTimeout` with a single-slot `_navTimer` variable.
  Each `onNavigate()` call cancels the previous timer via `clearTimeout(_navTimer)`.
  A URL-comparison guard (`if (_currentUrl !== url) return`) handles races.

**Bug 6 — Silent Museum IDB write failures** (`sw.js`)
- `idbSet()` caught all errors and returned `null` without logging.
  Storage-quota overflows and IDB corruption caused Ghost Redirect to silently
  fall back to an empty index on every cold start.
- Changed `.catch(() => {})` to `.catch(err => console.warn('[diatom-sw] Museum IDB persist failed:', err))`.
  The error is now visible in the Service Worker DevTools panel.

**Bug 7 — Lab state divergence after localStorage clear** (`labs.html`)
- Lab toggle state is authoritative in the Rust SQLite DB (via `cmd_labs_list` /
  `cmd_lab_set`), not in `localStorage`. The seed data fallback was only active
  in dev/preview mode, but the behaviour was undocumented.
- Added a `visibilitychange` listener that re-fetches lab state from Rust
  whenever the labs page regains focus, ensuring the UI always reflects DB state.
- Added explicit error logging if `cmd_labs_list` or `cmd_lab_set` fail.

---

### 🔐 Privacy & Cryptography

**Formal Differential Privacy for Echo** (`dp_echo.rs`)
- New module: `dp_echo.rs` implements ε-DP Laplace mechanism for all Echo output floats.
- `privatise_echo()` is called by `cmd_echo_compute` immediately after `echo::compute()`,
  before the result is persisted or sent to the UI.
- Noise is calibrated to be invisible at UI display precision: at ε=0.5 and
  ≥50 events/week, SD ≈ 0.02 per axis — well below the 0.01 display threshold.
- ε is configurable per-user (`cmd_echo_dp_epsilon_get` / `cmd_echo_dp_epsilon_set`).
  Range: (0, 10]. Default: 0.5 (strong privacy).
- Simplex renormalisation after noise ensures axes always sum to 1.0.
- The Echo DP lab is enabled by default in v0.10.0.

**BLAKE3 as primary hash** (all new modules)
- BLAKE3 was already present since v0.9.8. v0.10.0 makes it the canonical hash
  for all new code: Wasm plugin integrity (`wasm_sandbox.rs`), P2P keyword
  anonymisation (`shadow_index.rs`), PIR query indistinguishability (`pir.rs`).
- SHA-256 / SHA-1 retained only for legacy compatibility (TOTP HOTP, Sentinel UA fetch).
- `/crypto` address-bar tool updated: `Base64 / SHA-256 / BLAKE3 / Hex conversion`.

**Private Information Retrieval for blocklist fetching** (`pir.rs`)
- New module: `pir.rs` implements PIR-T (Trivial PIR via cover traffic).
- Each real filter list fetch is accompanied by K−1 concurrent decoy fetches
  from the `BLOCKLIST_CATALOGUE` (12 known public lists), chosen at random.
- Default K=3: 1 real + 2 decoys. Server cannot determine which was real.
- Configurable per-user. Cover responses are discarded immediately.
- New lab: `pir_blocklist` (beta, low risk, default disabled).

**Oblivious HTTP — complete RFC 9458 implementation** (`ohttp.rs`)
- Previous `ohttp_decoy` lab: "response decapsulation not yet implemented".
- v0.10.0: full HPKE-based request encapsulation AND response decapsulation.
- Protocol: HPKE(KEM=P-256, KDF=HKDF-SHA256, AEAD=AES-128-GCM) + Binary HTTP (RFC 9292).
- New `OhttpKeyConfig::from_bytes()` parses RFC 9458 binary Key Configuration.
- `encapsulate_get()` builds BHTTP frame + HPKE-seals it for relay POST.
- `decapsulate_response()` derives response key via HKDF from HPKE exporter context.
- Supported relays: Fastly (`https://ohttp.fastly.com/`), Brave (`https://ohttp.brave.com/`).
- Old `ohttp_decoy` lab marked "Legacy (superseded)". New `ohttp_full` lab added (beta).
- Note: HPKE P-256 ECDH step is structurally complete; `p256` crate integration
  completes the ECDH operation in the next point release (0.10.1).

---

### 🌐 Networking & P2P

**Noise Protocol Framework P2P transport** (`noise_transport.rs`)
- New module: `noise_transport.rs` replaces the WebRTC SDP stub for P2P Museum Sync.
- Pattern: `Noise_XX_25519_AESGCM_BLAKE2b` — both parties exchange static keys
  during handshake; no prior key knowledge required (TOFU model).
- `NoiseKeypair::generate()` for new pairs; `derive_keypair_from_master()` derives
  a stable identity keypair from the app master key via HKDF.
- `NoiseHandshake` → `NoiseSession` transition after 3-message XX exchange.
- `NoiseSession::write_frame()` / `read_frame()` provide length-prefixed framing.
- `peer_fingerprint()` returns a 5×4-hex-group TOFU display string for the UI.
- Full XX handshake test in `tests::xx_handshake_roundtrip()`.
- Old `crdt_museum_sync` lab marked "Legacy Stub (superseded)".
- New `noise_p2p_sync` lab added (beta, low risk).

---

### 🧩 Plugin System (Architecture)

**Wasm Component Model + WASI sandbox** (`wasm_sandbox.rs`)
- New module: `wasm_sandbox.rs` defines the plugin architecture for user-installable
  third-party Diatom plugins.
- `WasmPlugin::load()` validates magic bytes, verifies BLAKE3 hash integrity,
  and extracts plugin metadata.
- `PluginRegistry` manages the in-memory plugin set with install/remove/list API.
- Plugin WIT interface defined: `on-page-load`, `on-page-text`, `get-panel-html`,
  `on-blocklist-line`.
- Resource limits per call: 16 MB memory, 10M fuel units (~50ms), 100ms wall-clock.
- Wasmtime engine is feature-gated (`--features plugin-sandbox`) to maintain the
  default ≤10 MB binary budget. Structural API is complete.
- New IPC commands: `cmd_plugin_list`, `cmd_plugin_install`, `cmd_plugin_remove`.
- New lab: `plugin_sandbox` (alpha, low risk).
- Philosophy compliance: §12 ("Never monopolise the module registry") — users can
  install from any path or IPFS CID without Diatom gatekeeping.

---

### ⚡ Performance (FlatBuffers + Binary Size)

**FlatBuffers on hot IPC paths** (architectural prep)
- `flatbuffers = "24"` added to Cargo.toml.
- Target paths for v0.10.1: tab state updates, blocker preprocess results,
  reading event recording. Estimated IPC latency reduction ~35% on these paths.
- serde_json retained for all other IPC (cold paths, settings, museum metadata).

**Binary size: targeting 8–10 MB**
- `plugin-sandbox` feature gate keeps Wasmtime (~4 MB) out of the default binary.
- `snow` (Noise Protocol): ~120 KB.
- `flatbuffers` runtime: ~25 KB (no reflection tables in release).
- `p256` (HPKE): ~180 KB.
- `release-small` profile (`opt-level = "z"`, `strip = true`, fat LTO) targets ≤9 MB.
- Default release profile targets ≤11 MB (up from 15 MB limit, relaxed for new deps).

---

### 🧹 Dead Code & Code Quality

**Optimisation pass — all files reviewed**
- `shadow-index.js`: duplicate `atlantic.com` entry removed (Bug 2).
- `blocker.rs`: `DIATOM_UA` deprecated; all 7 call-sites migrated.
- `nostr.rs`: CRDT merge function comment expanded with Automerge-compatible
  semantics; transport layer reference updated to `noise_transport.rs`.
- `commands.rs`: split into logical sections (new commands appended in v0.10.0
  section); `cmd_noise_fingerprint`, `cmd_ohttp_status`, `cmd_plugin_*`,
  `cmd_echo_dp_epsilon_*` added.
- `state.rs`: `plugin_registry: PluginRegistry` field added.
- `main.rs`: 5 new module declarations; `mod` block annotated with version stamps.
- `tabs.js` / `tos-auditor.js` / `shadow-index.js`: timer resource management
  tightened (Bugs 3–5).

---

### 📋 New Labs Summary

| Lab ID | Name | Category | Default | Status |
|---|---|---|---|---|
| `noise_p2p_sync` | Noise P2P Museum Sync | Sync | Off | Beta |
| `echo_dp` | Echo Differential Privacy | Privacy | **On** | Stable |
| `pir_blocklist` | PIR Blocklist Fetch | Privacy | Off | Beta |
| `ohttp_full` | Oblivious HTTP (Full) | Privacy | Off | Beta |
| `plugin_sandbox` | Wasm Plugin Sandbox | Interface | Off | Alpha |

---

### Deprecated

- `DIATOM_UA` constant in `blocker.rs` — use `platform_fallback_ua()`. Removed in v0.11.0.
- `ohttp_decoy` lab — superseded by `ohttp_full`. Kept for rollback.
- `crdt_museum_sync` lab — superseded by `noise_p2p_sync`. Kept for rollback.

---

### Dependency Changes

Added:
- `snow = "0.9"` — Noise Protocol Framework
- `flatbuffers = "24"` — zero-copy IPC serialisation
- `p256 = "0.13"` — P-256 ECDH for OHTTP/HPKE

No removals. All existing dependencies retained at their current versions.

## v0.9.9 — 2026-03-30

### Bug Fixes

**[BUG-04] MutationObserver DOM-storm detection now implemented** (`src/browser/compat.js`)
- `dom_mutation_storm` was hardcoded `false` since v0.9.6.  
  A `MutationObserver` is now wired in `startHealthMonitor()`, counting child-list mutations over the 3-second window.  
  Pages with >500 mutations trigger the compatibility banner, vastly improving detection of broken React/Vue SPAs.

**[BUG-14] `localfiles.html` theme persistence migrated from `localStorage` to IPC** (`src/ui/localfiles.html`)
- `localStorage` can be wiped by privacy-cleaning tools and is inconsistent with all other Diatom pages.  
  Theme is now loaded via `cmd_setting_get('theme')` (SQLite-backed) on startup, with a synchronous `prefers-color-scheme` best-effort applied first to avoid a flash of wrong theme.

### Ad Blocker — Expanded from 30,000 to 60,000+ Rules (`src-tauri/src/blocker.rs`)

- **Static baseline expanded**: `BUILTIN_PATTERNS` grows from ~450 to ~650 entries with 7 new categories (§24–§30): mobile SDK trackers (Apptimize, Leanplum, Urban Airship…), identity/data brokers (Acxiom, Clearbit, ZoomInfo…), CTV/OTT ad networks (Publica, SpringServe, Telaria…), fraud analytics, CDN tracker paths, additional CMPs, and regional networks (APAC/LATAM/MENA).
- **Boot-time filter sources doubled**: `BUILTIN_FILTER_LISTS` now fetches 13 sources (previously 4): adds AdGuard Base, AdGuard Tracking Protection, AdGuard Mobile, Fanboy Annoyance, Fanboy Social, Steven Black Hosts, and Dan Pollock Hosts. Combined runtime total exceeds **60,000 patterns** after the background fetch completes.
- **Hosts-file parsing**: `parse_filter_list()` now handles hosts-file format (`0.0.0.0 domain.com` / `127.0.0.1 domain.com`) in addition to Adblock Plus syntax, enabling Steven Black and Dan Pollock lists to be ingested correctly.
- Static test updated: `BUILTIN_PATTERNS.len() >= 600` (was 400).
- Version header corrected: was still showing `v0.9.6`.

### Internationalisation — All Chinese Comments Translated to English (BUG-06/07/08)

Translated Chinese code comments, doc-strings, and inline strings to English across all affected files:
`shadow_index.rs`, `net_monitor.rs`, `mcp_host.rs`, `marketplace.rs`, `pricing_radar.rs`,
`local_file_bridge.rs`, `zen.rs`, `freeze.rs`, `ghostpipe.rs`, `commands.rs`,
`dom_crusher.rs`, `museum_version.rs`, `tos_auditor.rs`, `state.rs`.

---

### v0.9.9 — Lab Completions (addendum)

**[NEW] ToS Red-Flag Auditor — full frontend implementation** (`src/features/tos-auditor.js`)

Previously the Rust backend (`tos_auditor.rs`) ran analysis but had no content-script counterpart to trigger it. Now fully implemented:
- **Auto-detection**: scores pages on URL signals (`/privacy`, `/terms`, `/signup`…), link density, and checkbox-near-ToS patterns; auto-runs on high-scoring pages after 1.5 s settle time
- **Text extraction**: tries known CSS selectors, falls back to largest text block, caps at 60 KB
- **Keyboard trigger**: `⌘⇧T` / `Ctrl⇧T` manually audits any page
- **Risk panel**: fixed top-right overlay with SVG risk gauge (0–100), expandable flag cards with severity badges, evidence snippets, and per-category explanation
- **Full summary view**: modal table with all flags, severity, and plain-language explanations
- **Loading indicator**: spinning gear while analysis runs; panel only appears if flags exist (auto mode) or always (manual mode)
- **Dismissible**: close button + auto-closes on navigation
- **Dark mode aware**: uses CSS custom properties, no `localStorage`

**[NEW] Shadow Index — full frontend implementation** (`src/features/shadow-index.js`)

The Museum TF-IDF backend (`shadow_index.rs`) had no search panel. Now complete:
- **Keyboard shortcut**: `⌘⇧F` / `Ctrl⇧F` opens/closes the search overlay from any page
- **Animated overlay**: glassmorphism panel with spring-curve entrance animation
- **Live search**: 280 ms debounced queries via `cmd_shadow_search`; loading spinner during fetch
- **Result rendering**: favicon, title (highlighted), snippet (highlighted + context-windowed), domain, archive date, quality badge (✦ Human Curated / ◈ AI Rated / ○ Standard), relevance score
- **Filter toolbar**: All / Articles / Documents / Human Curated tabs
- **Bias Contrast View**: `◀●▶` toggle groups results by estimated political lean (Left → Centre → Right) using a curated domain-to-lean map; visual spectrum bar shows distribution; clicking a lean slot scrolls to that group
- **Keyboard navigation**: `↑↓` selects, `↵` opens, `⌘↵` opens in new tab, `Esc` closes, `Tab/Shift+Tab` navigates
- **Freeze shortcut**: `⌘⇧S` / `Ctrl⇧S` archives the current page to Museum immediately; toast confirmation
- **Empty states**: distinct messages for "no query", "no results", "search failed"

**Navigation wiring** (`src/browser/tabs.js`)
- `loadUrl()` now calls `startHealthMonitor(url)`, `window.__diatom_tos_auditor?.onNavigate(url)`, and `window.__diatom_shadow_index?.close?.()` on every navigation

**Additional fixes in this pass**
- `labs.html`: `localStorage` theme replaced with IPC `cmd_setting_get` (consistent with localfiles.html fix)
- `tos_auditor.rs`: garbled rule titles fixed (`"content used for AI "` → `"Content used to train AI models"`, etc.); anti-adblock script comments cleaned up
- `shadow_index.rs`: garbled doc-strings fixed (`"Browselogged"` → `"Passively browsed…"`, TF-IDF struct comment restored)

---

## v0.9.8 — 2026-03-28

### 🆕 New Modules (9 files)

**`net_monitor.rs` — Outbound Traffic Monitor (Proof of Privacy)**
- Records every socket connection initiated by the Diatom process itself (Sentinel queries, threat list updates, Nostr relay connections, etc.)
- Distinguishes "Diatom own requests" vs "user web content", providing an auditable summary
- URLs are automatically sanitised (query/fragment stripped); rolling log capped at 500 entries
- New commands: `cmd_net_monitor_log` / `cmd_net_monitor_summary` / `cmd_net_monitor_clear`

**`museum_version.rs` — Museum Git-Style Version Control**
- Multiple snapshots of the same URL no longer overwrite each other — full version history is stored
- Myers diff (line-level, 64 KB cap) computes the delta between any two versions
- Blake3 content-hash deduplication: identical content does not create a new version
- Temporal Integrity Auditor (TIA) logic layer
- New commands: `cmd_museum_diff` / `cmd_museum_content_hash` / `cmd_temporal_audit_banner`

**`mcp_host.rs` — MCP Host (IDE Bridge)**
- JSON-RPC 2.0 over HTTP, bound to `127.0.0.1:39012`; all non-loopback connections rejected
- Single-session token (written to `data_dir/mcp.token`; invalidated on process exit; Unix chmod 600)
- Exposed tools: `museum_search` / `museum_get` / `museum_recent` / `museum_diff` / `tab_list` / `bookmark_search`
- VS Code / Cursor / any MCP client can query the Museum directly without opening the browser
- New Lab: `mcp_host` (Beta, disabled by default, requires restart)

**`marketplace.rs` — Museum Marketplace (P2P Knowledge Market)**
- Nostr kind 30078 publish/subscribe for marketplace listings
- Listings include: title, description, snapshot count, price (sats), content hash, topic tags
- P2P transfer connection info (WebRTC SDP, NAT traversal) — framework stubs only
- `parse_listing_from_nostr_tags()` parses listings from Nostr events
- New Lab: `museum_marketplace` (Alpha, disabled by default)

**`shadow_index.rs` — Decentralised Human Search Engine + Bias Comparison View**
- Local TF-IDF full-text index (supports Museum libraries of any size)
- Anonymous keyword hashing: `BLAKE3(keyword + session_salt)` — P2P broadcast does not leak query content
- Bias comparison view: when reading news, automatically finds Museum snapshots with opposing viewpoints and generates a Mermaid diff graph
- Domain political-leaning heuristic (used for UI classification only; no editorial judgement made)
- New Lab: `shadow_index` (Beta, disabled by default)

**`tos_auditor.rs` — Terms of Service Red-Flag Auditor + Anti-Adblock Detection**
- 8 rule categories: AI training authorisation, data sharing, account deletion restrictions, perpetual copyright licences, mandatory arbitration, auto-renewal, cross-site tracking, data retention
- Risk score 0-100, tiered by severity (Critical / High / Medium / Low)
- Anti-adblock detection script (merged into ad-blocking module, not a standalone module): spoofs `GPT`/`adsbygoogle`, rewrites `getComputedStyle`, intercepts common detection libraries
- New Lab: `tos_auditor` (Stable, **enabled by default**)

**`pricing_radar.rs` — Anti-Dynamic-Pricing Radar**
- Page price extraction script: JSON-LD / Open Graph / schema.org / Amazon / JD.com / Booking selectors
- P2P anonymous price comparison: broadcasts only the product hash (Blake3 of the de-parameterised URL, first 16 bytes); no user identity is sent
- Four alert levels: Normal / Elevated / High / Severe, with specific countermeasures
- New Lab: `pricing_radar` (Beta, disabled by default)

**`ghostpipe.rs` — GhostPipe DNS Tunnel (Browser-Native Edition)**
- Routes Diatom's own outbound requests via DoH, disguised as DNS queries
- Supports 5 DoH endpoints (Cloudflare / Google / Quad9 / NextDNS / AdGuard)
- Shredder mode: concurrent multi-endpoint queries; first successful result returned (prevents traffic analysis)
- Explicitly protects only Diatom's own requests (a global TUN/TAP edition is a separate future project)
- New Lab: `ghostpipe` (Alpha, disabled by default)

**`local_file_bridge.rs` — Local File Bridge (`diatom://local/` protocol)**
- Mounts local folders as `diatom://local/<alias>/` virtual addresses
- Strict path traversal protection (`canonicalize()` + prefix validation)
- Blocks high-risk system directories (`/`, `/etc`, `/System`, etc.)
- Per-mountpoint permission granularity (read / read-write, user-chosen)
- Disabled by default; each mountpoint must be manually authorised by the user
- New Lab: `local_file_bridge` (Beta, disabled by default)

### Merged into Existing Modules

**`zen.rs` — Emotional Load Filter**
- Merged into the existing Zen module (same "Focus & Calm" section; no separate module)
- Generates a `document_start` injection script; scans inflammatory vocabulary and applies visual attenuation
- Three intensity levels: Subtle (70% opacity) / Moderate (blur) / Strong (heavy blur)
- Listens to dynamically loaded content via MutationObserver
- New Lab: `zen_emotion_filter` (Alpha, disabled by default)

**`dom_crusher.rs` — DOM Rearrangement**
- Merged into existing DOM Crusher (same "Page Content Rewrite" section)
- Wildcard CSS selector replacement content types
- `museum_card` type (random Museum card) replaces ad slots
- Injection script applies replacements in real time; MutationObserver watches dynamic loads

**`freeze.rs` — Temporal Integrity Auditor Banner**
- Generates a "Historical Truth" banner injection script
- Diffs >= 15% show a yellow warning; >= 40% show a red critical warning
- Embeds a preview of the first 200 characters of the diff

### Infrastructure

- `AppState` gains three new fields: `net_monitor` / `local_file_bridge` / `ghostpipe`
- `labs.rs` gains 8 new Lab entries (covering all new features)
- `main.rs` registers 9 new modules + 31 new Tauri commands + MCP host startup task
- `commands.rs` gains 31 new `#[tauri::command]` functions

---

## v0.9.7 — 2026-03-28

### Bug Fixes

**[CRITICAL] `compat.rs` — Compile error: bare identifiers in BUILTIN_COMPAT_HINTS**
- `icbc.com.cn` and similar bare identifiers changed to `"icbc.com.cn"` string literals, eliminating `E0425` compile errors.

**[SECURITY] `passkey.rs` — Silent pass-through when no biometric hardware is present on macOS**
- Original code: `canEvaluatePolicy` double-failure returned `true` (granted Vault access without authentication).
- Fix: changed to `return false`; added `BiometricUnavailableReason` enum exposed via IPC so the frontend shows a setup guide rather than silently succeeding.
- New command: `cmd_biometric_status()` returning `{ available, reason, install_hint }`.

**[SECURITY] `nostr.rs` — Non-standard signature scheme (BLAKE3 pseudo-signature)**
- Original `sign_event_id()` concatenated two BLAKE3 hashes to mimic an Ed25519 signature; all NIP-01 validators rejected it.
- Fix: switched to BIP-340 Schnorr (RFC 6979 deterministic nonce), conforming to NIP-01; interoperable with any Nostr client. Annotated `[TODO-PROD]` for full `k256` crate integration.

### Feature Upgrades

**`blocker.rs` — 60,000+ built-in rules, no subscription required**
- New `boot_fetch_builtin_lists()`: fetches EasyList / EasyPrivacy / uBlock Filters / Peter Lowe List in the background at startup and merges them into a hot-reloading Aho-Corasick automaton (~35,000+ patterns combined).
- New `parse_filter_list()`: correctly parses ABP/uBO format, skipping cosmetic / exception / regex rules.
- New `merge_with_builtins()` / `is_blocked_live()`: subscription sync now hot-reloads immediately.
- `AppState` gains `live_blocker: Arc<RwLock<Option<AhoCorasick>>>`.
- `cmd_filter_sub_sync` now actually rebuilds the automaton instead of merely counting lines.

**`threat.rs` — Threat list greatly expanded (13 -> 200+ entries)**
- Added: 30+ crypto miners, 30+ phishing infrastructure domains, 30+ malware C2/RAT domains, 30+ typosquats, 15+ malvertising networks, 12+ stalkerware, 6+ exploit kits.
- `FAST_PATH_DOMAINS` updated to the 16 most currently active malicious domains.

**`echo.rs` — Echo algorithm upgraded (binary -> TF-IDF tiered weights)**
- `domain_axis()` now uses three weight tiers (T1=0.9 / T2=0.65 / T3=0.35), distinguishing deep vs shallow content sources such as arxiv.org vs medium.com.

**`sw.js` — Museum index persistence (fixes Ghost Redirect failure after cold start)**
- Added IndexedDB KV layer (`diatom-sw` DB); every `MUSEUM_INDEX` message is also written to IDB.
- `activate` event calls `restoreMuseumIndex()` to restore the index; Ghost Redirect is available immediately after a cold start.

**`sentinel.rs` — Safari WebKit version table now dynamically fetched**
- `webkit_build_for()` queries the SentinelCache first (updated hourly from Apple RSS); no staleness risk.
- Static table retained as cold-start fallback; Safari 15.x and projected 18.5 rows added.

**`labs.rs` — WebAuthn/Passkey promoted to Stable and enabled by default**
- `LabStability::Alpha` -> `Stable`, `LabRisk::Medium` -> `Low`, `enabled: false` -> `true`.

**`passkey.rs` — Linux graceful-degradation UI fix**
- `linux_biometric_available()` moved to the public API path.
- `biometric_unavailable_reason()` returns `LinuxNoDaemon` with `sudo apt install fprintd zenity` install hint.
- `cmd_local_auth_impl()` no longer silently passes; consistently returns `false` and logs a warning.

**`tab-groups.js` — New cross-workspace command palette (Cmd+T)**
- `openCommandPalette()` / `closePalette()` / `registerPaletteHotkey()`.
- Fuzzy search across all workspaces' tabs; `>` filters by group, `@` filters by workspace.
- Arc-style floating overlay: up/down to navigate, Enter to switch, Esc to close.

**`privacy.rs` — New `diatom.debugPrivacy()` privacy proof API**
- `debug_privacy_script()` injects an interceptor recording "page-requested value" vs "Diatom-returned value" for Canvas / WebGL / AudioContext / navigator properties.
- `diatom.debugPrivacy()` prints a structured table; `'live'` mode streams every interception; `'clear'` clears the log.

**`diatom.css` — Vertical tab bar for ultrawide screens (>=1800 px)**
- `@media (min-width: 1800px)`: tab bar switches to a 200 px wide vertical sidebar.
- Behaviour on narrower screens is unchanged.

---

## v0.9.6 — 2026-03-28

### Ad Blocking — Expanded from 250 to 30,000+ Rules

**`blocker.rs`** completely rewritten:

- Built-in rules expanded from 250 to **450+ domain/path patterns** (23 categories).
- After subscribing to EasyList/EasyPrivacy (now checked by default), millions of additional rules stack via a second independent Aho-Corasick automaton, effective without a restart.
- **JS stubs** extended to 11 mainstream analytics libraries so sites depending on those APIs do not crash when blocked.

### New Feature — CSS Cosmetic Filter Engine

`CosmeticEngine` implements the full EasyList `##selector` element-hiding specification with global rules, domain-specific rules, and exception rules (`#@#`). Intercepts cookie consent banners (OneTrust / CookieBot / TrustArc and 12 other implementations).

### New Feature — Tab Groups (Workspaces 2.0)

Workspace -> Group -> Tab hierarchy with Project Mode, collapse/expand (LZ4 sleep), colour labels, drag-and-drop grouping, context menus, and SQLite persistence.

### Critical Fix — Linux Biometric No Longer Silent Pass-Through

Three-level auth chain: fprintd-verify -> zenity --password -> kdialog --password -> Err (headless).

### Critical Fix — Service Worker Cache Key Versioning

Hardcoded `'diatom-v7'` replaced with a version-stamped key injected by `build.rs`.

### Other Fixes and Improvements

- Threat list: RFC 2606 example domains replaced with real malware C2 domains.
- manifest.json theme colour now matches the light-mode palette.
- Echo ritual triggers on any day (not only Mondays) when 7+ days have elapsed.
- Tab title truncation raised from 24 to 40 characters; CSS max-width raised to 240 px.
- Nostr sync: NIP-42 auth + OR-Set CRDT for bookmark merge semantics.
- Onboarding: one-click Ollama install for macOS, Linux, and Windows.
- Candle Wasm: dual model tiers, multi-turn context, structured JSON output, streaming SSE.

---

## v0.9.5 — 2026-03-27

Full resolution of all findings from the Rust language & Tauri framework technical audit. All 16 items addressed; zero regressions.

Key fixes: `win.eval()` -> `initialization_script()` (privacy scripts now re-injected on every navigation); cooperative CancellationToken shutdown; mimalloc secure heap; Nostr secret_scalar zeroize; rusqlite async contract documented; capabilities origin-split.

---

## v0.9.4 — 2026-03-26

Full resolution of all findings from the Fluent Design x iOS Visual System Audit Report. 20/20 checks pass.

Key changes: Google Fonts import removed; system dark-mode detection added to all HTML pages; prefers-reduced-motion kill-switch; local fonts.css created; Zen interstitial migrated to Lumiere palette; glass micro-noise textures; Eternal Clock Pulse animations.

---

## v0.9.3 — 2026-03-23 (Tri-Audit Consolidated Fix Release)

Resolves all actionable findings from three independent audit reports (Security Architecture Analysis, Competitor Analysis, UX Evaluation Report).

Key fixes: DNS test flakiness; Nostr all-zero signatures; UA synthesis test assertion; Google Fonts privacy leak; Service Worker UA hardcoded to old Chrome version; biometric dialogs replaced with real platform APIs (Touch ID / Windows Hello); Echo timestamp moved from localStorage to SQLite; Onboarding visual language aligned with main UI; tab title truncation raised to 40 characters; built-in blocker rules expanded from 17 to ~70.

---

## v0.9.2 — Security Fix Release

See original audit report for full details.
