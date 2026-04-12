# Diatom — Zed Integration Guide

This directory contains the DevPanel, bridge crates, and Zed IDE integration
for Diatom v0.14.3+.

---

## Architecture

```
Diatom shell (Tauri)
   │
   ├── dev_panel.rs          Spawns diatom-devpanel; manages BridgeClient
   ├── fingerprint_norm.rs   FingerprintNorm JS injection (Axiom 10)
   └── url_stripper.rs       Rule-based tracking-param stripper (Axiom 9)
         │
         └── Unix socket / Named Pipe
                │
         diatom-devpanel (GPUI process)
               │
               ├── diatom_bridge/     IPC protocol + transport
               ├── diatom_devtools/   Console, Network, Sources, DevTools window
               └── slm_adapter.rs    Routes DevPanel AI → Diatom :11435
                         │
                  ~/.diatom/resonance.sock
                         │
                  External Zed IDE (optional)
                  Reads ResonanceContext pushed on every navigation
```

---

## "Open in Zed" button

When the DevPanel Sources panel has a file selected, an **Open in Zed** button
appears in the toolbar. Clicking it:

1. Sends `DevPanelMessage::OpenInZedIde { url, line }` through the bridge.
2. `dev_panel.rs` receives it, calls `open_in_zed(url, project_root, line)`.
3. `open_in_zed` resolves the resource URL to a local filesystem path:
   - `file://` URLs → strip scheme directly.
   - HTTP/HTTPS URLs → `project_root + URL path component`.
4. Spawns `zed <resolved_path>:<line>`.
5. If `zed` is not in PATH or the file doesn't exist locally, logs a warning;
   no fallback to a cloud URL is attempted (Axiom 8 — zero telemetry).

The JS hook `window.__diatom_open_in_zed(url, line)` is also available in the
page context (injected by `diatom-devpanel-hooks.js`) for DevPanel UI code.

**Setting the project root:**

```bash
# Before launching Diatom:
export DIATOM_PROJECT_ROOT=/path/to/your/project
```

Or pass `projectRoot` when opening the DevPanel from JS:

```js
window.__TAURI__.invoke("dev_panel_open", { projectRoot: "/path/to/project" });
```

---

## Resonance context sharing (Axiom 16)

Diatom's local AI (Resonance, served from `slm.rs` at `:11435`) pushes a
`ResonanceContext` snapshot to `~/.diatom/resonance.sock` on every navigation.

External Zed reads this socket and receives:

| Field | Content |
|---|---|
| `page_url` | Current page URL |
| `page_title` | Document title |
| `console_errors` | Last 20 console errors |
| `dom_root` | Depth-limited DOM snapshot (3 levels) |
| `active_source` | Active source file URL + first 4 KB |

The socket is write-only from Zed's side — Zed cannot push back through it.
Socket permissions are `0600` (owner read/write only).

---

## Fingerprint normalisation (Axiom 10)

`fingerprint_norm.rs::FingerprintNorm::generate()` produces a JavaScript
snippet that overrides fingerprint-bearing browser APIs with normalised
constants matching the statistical mode of common desktop hardware (2025).

Overridden APIs:
- `navigator.hardwareConcurrency` → 8
- `navigator.deviceMemory` → 8 GB
- `navigator.maxTouchPoints` → 0
- `screen.colorDepth` / `screen.pixelDepth` → 24
- `WebGLRenderingContext.getParameter(VENDOR)` → Intel via ANGLE
- `WebGLRenderingContext.getParameter(RENDERER)` → UHD Graphics ANGLE D3D11
- `AudioContext` sample rate → 44100 Hz
- Canvas `toDataURL` / `toBlob` → deterministic 1-bit LSB shift keyed by hostname

This is injected via Tauri's `initialization_script` before any page content.
The `diatom-api.js` file no longer contains any fingerprint overrides.

---

## URL stripping (Axiom 9)

`url_stripper.rs` strips tracking parameters using static curated lists:

- `PROTECTED_PARAMS` — never stripped (session IDs, OAuth tokens, CSRF state)
- `STRIP_PARAMS` — exact-match strip (UTM, fbclid, gclid, msclkid, ttclid, …)
- `STRIP_PREFIXES` — prefix-match strip (`utm_`, `hsa_`, `fb_`, `ga_`, `iterable`)

Parameters on the protected list take precedence over all strip rules.
AI-generated strip rules are prohibited by Axiom 9.

---

## Building

```bash
# Strip Zed telemetry/collab crates first
bash scripts/strip-zed.sh

# Build the DevPanel binary
cd zed-integration/zed-core
cargo build --release -p diatom_devtools

# Build the Tauri shell with DevPanel integration
cd ../src-tauri
cargo tauri build
```

The DevPanel binary is bundled as a sidecar resource; Tauri packages it
alongside the main application binary.
