#!/usr/bin/env bash
# check-zed-deps.sh — CI guard for Zed dependency whitelist (Axiom 19)
#
# Run automatically on any PR that touches zed-vendor/ or shell/Cargo.toml.
# Also run manually before merging a quarterly Zed sync window.
#
# Exit codes:
#   0  all checks passed
#   1  one or more checks failed (PR must not merge)
#
# Usage:
#   bash scripts/check-zed-deps.sh [--target diatom_devtools]
#
# Requires: cargo, cargo-tree (ships with cargo >= 1.42)

set -euo pipefail

TARGET="${1:-diatom_devtools}"
KNOWN_CRATES_FILE="scripts/known-zed-crates.txt"
BANNED_PATTERNS=(
    "telemetry"
    "telemetry_events"
    "zlog"
    "ztracing"
    "action_log"
    "crashes"
    "collab"
    "livekit"
    "anthropic"
    "bedrock"
    "cloud_"
    "google_ai"
    "open_ai"
    "open_router"
    "copilot"
    "auto_update"
    "feedback"
    "remote_server"
    "sqlez"
)

FAIL=0

echo "=== check-zed-deps.sh ==="
echo "Target crate : $TARGET"
echo "Known crates : $KNOWN_CRATES_FILE"
echo ""

# ── Step 1: scan for banned crate families ────────────────────────────────────
echo "Step 1 — Scanning for banned crate families..."

cd "$(dirname "$0")/.."
TREE_OUTPUT=$(cargo tree --prefix none -p "$TARGET" 2>/dev/null || true)

for pattern in "${BANNED_PATTERNS[@]}"; do
    MATCHES=$(echo "$TREE_OUTPUT" | grep -i "$pattern" | grep -v "^#" || true)
    if [[ -n "$MATCHES" ]]; then
        echo "FAIL: banned pattern '$pattern' found in dependency tree:"
        echo "$MATCHES" | sed 's/^/  /'
        FAIL=1
    fi
done

if [[ $FAIL -eq 0 ]]; then
    echo "  OK — no banned crate families detected"
fi

echo ""

# ── Step 2: detect newly introduced crates ────────────────────────────────────
echo "Step 2 — Checking for new crates not in the approved list..."

if [[ ! -f "$KNOWN_CRATES_FILE" ]]; then
    echo "  WARNING: $KNOWN_CRATES_FILE not found — creating from current tree."
    echo "           Commit this file and re-run to enable new-crate detection."
    cargo tree --prefix none -p "$TARGET" 2>/dev/null \
        | awk '{print $1}' \
        | sort -u \
        > "$KNOWN_CRATES_FILE"
    echo "  Created $KNOWN_CRATES_FILE with $(wc -l < "$KNOWN_CRATES_FILE") entries."
else
    CURRENT_CRATES=$(cargo tree --prefix none -p "$TARGET" 2>/dev/null \
        | awk '{print $1}' | sort -u)
    KNOWN_CRATES=$(sort -u "$KNOWN_CRATES_FILE")

    NEW_CRATES=$(comm -23 \
        <(echo "$CURRENT_CRATES") \
        <(echo "$KNOWN_CRATES"))

    if [[ -n "$NEW_CRATES" ]]; then
        echo "  ATTENTION: New crates detected (human review required):"
        echo "$NEW_CRATES" | sed 's/^/    + /'
        echo ""
        echo "  For each new crate:"
        echo "    1. Verify it does not contain telemetry, analytics, or outbound HTTP"
        echo "       to non-user-controlled endpoints."
        echo "    2. If approved, add it to $KNOWN_CRATES_FILE."
        echo "    3. Re-run this script to confirm clean."
        echo ""
        echo "  Quick pattern scan on new crates:"
        for crate in $NEW_CRATES; do
            SRC=$(find "$HOME/.cargo/registry/src" -name "*.rs" \
                       -path "*/${crate}-*/*" 2>/dev/null | head -20)
            if [[ -n "$SRC" ]]; then
                HITS=$(grep -lE \
                    "telemetry::|sentry::|analytics|reqwest.*(anthropic|openai|bedrock|googleapis)" \
                    $SRC 2>/dev/null || true)
                if [[ -n "$HITS" ]]; then
                    echo "    SUSPICIOUS: $crate — pattern match in source:"
                    echo "$HITS" | sed 's/^/      /'
                    FAIL=1
                else
                    echo "    OK (no suspicious patterns): $crate"
                fi
            else
                echo "    SKIP (source not in cargo cache): $crate — manual review needed"
            fi
        done
    else
        echo "  OK — no new crates introduced"
    fi
fi

echo ""

# ── Step 3: confirm stripped crates are absent from build graph ───────────────
echo "Step 3 — Confirming stripped Zed crates are not in the build graph..."

STRIP_BANNED=$(echo "$TREE_OUTPUT" | grep -E \
    "^(telemetry|collab|livekit|anthropic|bedrock|copilot|auto_update|remote_server)" \
    || true)

if [[ -n "$STRIP_BANNED" ]]; then
    echo "  FAIL: strip-zed.sh targets found in build graph:"
    echo "$STRIP_BANNED" | sed 's/^/  /'
    FAIL=1
else
    echo "  OK — all strip-zed.sh targets absent"
fi

echo ""

# ── Result ────────────────────────────────────────────────────────────────────
if [[ $FAIL -ne 0 ]]; then
    echo "RESULT: FAILED — see above. PR must not merge until issues are resolved."
    exit 1
else
    echo "RESULT: PASSED — all Zed dependency checks clean."
    exit 0
fi
