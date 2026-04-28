# Contributing to Diatom

## Before you start

Read `AXIOMS.md`. Every contribution is measured against it. The axioms are not
guidelines — they are load-bearing constraints. If your change conflicts with
any axiom, the change does not land; the axiom is not adjusted.

Pay particular attention to the **Preamble — Affirmative Boundary**: Diatom is
a tool for browsing the open web without being tracked, analysed, or manipulated.
If your feature cannot be justified against that sentence, it does not belong here.

---

## Feature lifecycle in Labs

Any genuinely experimental feature should enter through the Labs system
(`src/ui/labs.html`, `src-tauri/src/features/labs.rs`). Labs features are
subject to a mandatory lifecycle:

1. **Enter Labs** — clearly marked as experimental, default off.
2. **Stay in Labs** for at most two stable minor releases.
3. **Decision point** — the feature must either:
   - Graduate to a stable feature (requires full review, axiom check, docs), or
   - Be marked `deprecated` and removed in the next minor release.

A feature that never leaves Labs is a liability. "Permanent Lab" is not a valid
state. This rule exists to keep the product honest about what it actually is.

---

## `diatom://` URL discipline

`diatom://` URLs (e.g. `diatom://labs`, `diatom://onboarding`) are internal
navigation addresses. They must **never appear in external-facing documentation,
tutorials, or blog posts**. Documentation says "open the Labs page", never
"type `diatom://labs` in the address bar". This prevents the internal namespace
from becoming an external dependency.

---

## Data portability

Any feature that stores user data must ship with an export path to a standard
open format (Axiom 20). If you are adding a new data type, add a row to the
table in `AXIOMS.md §VII` and implement the exporter before the feature lands
in a stable release.

---

## Sidecar / plugin extensions

The sidecar mechanism is available for **user-authored tools only**. Official
Diatom features are implemented in the main process or the DevPanel, not as
sidecars. Do not add official capabilities behind the sidecar boundary; it
creates the infrastructure prerequisites for a plugin registry, which violates
Axiom 21.

---

## Resonance / command syntax

New Resonance capabilities must be expressed as standard MCP tool definitions,
not as new slash-command syntax. Proprietary command syntax (`/scholar`,
`/oracle`, etc.) locks in user muscle memory and cannot be composed with other
tools. Map new capabilities to tools; the address bar or DevPanel UI is just
one entry point.

---

## Zed dependency policy

- Diatom syncs the Zed editor core on a **quarterly schedule**, not on every
  Zed release. Do not open PRs that bump the Zed pin outside the quarterly
  window unless a security issue requires it.
- Any PR touching `zed-vendor/` must pass `scripts/check-zed-deps.sh` before
  merge.
- Newly introduced crates require explicit human approval and a justification
  comment in `scripts/known-zed-crates.txt`.
- The DevPanel (`diatom_devtools`) is currently scoped to Console, Network, and
  Sources panels. DAP debugger and Git UI integration are deferred to a later
  milestone when the dependency management tooling is more mature.

---

## Security vulnerabilities

Report security issues privately via the GitHub Security Advisory mechanism.
Do not open public issues for unpatched vulnerabilities.

For the `EvalJs` bridge in particular: the authentication handshake
(`HandshakeMessage::Challenge/Response/Accepted`) is the primary guard against
same-user process injection. Any change that weakens or bypasses this handshake
is a P0 security regression and must not land.
