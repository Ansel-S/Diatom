#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ZED_CORE_DIR="$REPO_ROOT/zed-core"
REPORT_DIR="$REPO_ROOT/scripts/strip-reports"

RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'; NC='\033[0m'
info()  { echo -e "${GREEN}[strip-zed]${NC} $*"; }
warn()  { echo -e "${YELLOW}[strip-zed]${NC} $*"; }
error() { echo -e "${RED}[strip-zed]${NC} $*" >&2; }

ZED_TAG="${1:-main}"
TIMESTAMP="$(date '+%Y%m%d-%H%M%S')"
WORK_DIR="/tmp/zed-strip-$TIMESTAMP"
REPORT="$REPORT_DIR/strip-report-${ZED_TAG//\//-}.md"

mkdir -p "$REPORT_DIR"

BLACKLIST_CRATES=(
    telemetry
    telemetry_events
    zlog
    zlog_settings
    ztracing
    ztracing_macro
    action_log
    crashes
    miniprofiler_ui

    collab
    collab_ui
    channel
    call
    livekit_api
    livekit_client
    proto
    rpc
    notifications

    anthropic
    bedrock
    cloud_api_client
    cloud_api_types
    cloud_llm_client
    codestral
    deepseek
    google_ai
    mistral
    open_ai
    open_router
    vercel
    x_ai
    lmstudio
    acp_thread
    acp_tools

    copilot
    copilot_chat
    copilot_ui

    agent
    agent_ui
    agent_servers
    agent_settings
    ai_onboarding
    language_onboarding
    onboarding
    edit_prediction
    edit_prediction_cli
    edit_prediction_context
    edit_prediction_types
    edit_prediction_ui
    eval_cli
    eval_utils
    zeta_prompt
    prompt_store
    rules_library
    context_server

    auto_update
    auto_update_helper
    auto_update_ui

    feedback

    remote
    remote_connection
    remote_server
    dev_container
    askpass

    db
    migrator
    sqlez
    sqlez_macros

    client
    credentials_provider
    zed_credentials_provider
    session
    feature_flags

    story
    storybook
    component_preview

    audio
    denoise
    media

    web_search
    web_search_providers
)

BLACKLIST_DIRS=(
    ".cloudflare"
    ".factory"          # brand-writing AI prompts
    "Procfile"
    "Procfile.all"
    "Procfile.web"
    "Dockerfile-collab"
    "compose.yml"
    "livekit.yaml"
    "legal"             # Zed commercial terms (Diatom uses BUSL-1.1)
)

BLACKLIST_GITHUB_WORKFLOWS=(
    "background_agent_mvp.yml"
    "bump_collab_staging.yml"
    "deploy_collab.yml"
    "congrats.yml"
    "add_commented_closed_issue_to_project.yml"
    "community_champion_auto_labeler.yml"
    "community_close_stale_issues.yml"
    "community_update_all_top_ranking_issues.yml"
    "community_update_weekly_top_ranking_issues.yml"
)

WHITELIST_CRATES=(
    editor text rope multi_buffer buffer_diff
    language language_core language_extension languages grammars lsp
    diagnostics outline outline_panel repl prettier
    gpui gpui_macos gpui_linux gpui_windows gpui_wgpu gpui_tokio gpui_util gpui_macros
    ui ui_input ui_macros ui_prompt component menu picker
    theme syntax_theme theme_settings theme_extension theme_selector theme_importer
    search fuzzy command_palette command_palette_hooks
    workspace sidebar panel breadcrumbs tab_switcher go_to_line platform_title_bar
    settings settings_content settings_macros settings_json settings_ui keymap_editor
    vim vim_mode_setting which_key
    terminal terminal_view
    dap dap_adapters debugger_ui debugger_tools debug_adapter_extension
    git git_ui git_graph git_hosting_providers
    task tasks_ui snippet snippet_provider snippets_ui
    fs worktree project project_panel project_symbols file_finder recent_projects node_runtime
    extension extension_api extension_host extension_cli extensions_ui
    language_selector language_tools toolchain_selector
    markdown markdown_preview svg_preview image_viewer csv_preview
    inspector_ui
    collections util util_macros clock sum_tree
    http_client http_client_tls net
    assets file_icons icons paths scheduler refineable
    encoding_selector line_ending_selector time_format env_var shell_command_parser
    streaming_diff zed_actions language_model
    diatom_bridge diatom_devtools
)

info "=== strip-zed.sh — Zed tag: $ZED_TAG ==="
info "Work dir: $WORK_DIR"
info "Report:   $REPORT"

echo "# Strip Report — Zed $ZED_TAG" > "$REPORT"
echo "Generated: $(date)" >> "$REPORT"
echo "" >> "$REPORT"

info "Step 1/5: Fetching Zed $ZED_TAG..."
mkdir -p "$WORK_DIR"

if [ "$ZED_TAG" = "main" ]; then
    ZED_ARCHIVE_URL="https://github.com/zed-industries/zed/archive/refs/heads/main.tar.gz"
else
    ZED_ARCHIVE_URL="https://github.com/zed-industries/zed/archive/refs/tags/${ZED_TAG}.tar.gz"
fi

curl -fsSL "$ZED_ARCHIVE_URL" | tar -xz -C "$WORK_DIR" --strip-components=1
info "  Fetched into $WORK_DIR"

info "Step 2/5: Checking for new Zed crates not in whitelist or blacklist..."

NEW_CRATES=()
if [ -d "$WORK_DIR/crates" ]; then
    while IFS= read -r -d '' crate_dir; do
        crate_name="$(basename "$crate_dir")"
        in_whitelist=0
        in_blacklist=0
        for w in "${WHITELIST_CRATES[@]}"; do
            [ "$w" = "$crate_name" ] && { in_whitelist=1; break; }
        done
        for b in "${BLACKLIST_CRATES[@]}"; do
            [ "$b" = "$crate_name" ] && { in_blacklist=1; break; }
        done
        if [ "$in_whitelist" -eq 0 ] && [ "$in_blacklist" -eq 0 ]; then
            NEW_CRATES+=("$crate_name")
        fi
    done < <(find "$WORK_DIR/crates" -maxdepth 1 -mindepth 1 -type d -print0)
fi

if [ ${#NEW_CRATES[@]} -gt 0 ]; then
    warn "  NEW crates detected (not in whitelist or blacklist) — MANUAL REVIEW REQUIRED:"
    echo "## ⚠️  New Crates (require manual triage)" >> "$REPORT"
    for c in "${NEW_CRATES[@]}"; do
        warn "    • $c"
        echo "- \`$c\`" >> "$REPORT"
    done
    echo "" >> "$REPORT"
else
    info "  No new crates. ✓"
    echo "## New Crates: none ✓" >> "$REPORT"
    echo "" >> "$REPORT"
fi

info "Step 3/5: Removing blacklisted crates..."
echo "## Deleted Crates" >> "$REPORT"

deleted_count=0
for crate in "${BLACKLIST_CRATES[@]}"; do
    target="$WORK_DIR/crates/$crate"
    if [ -d "$target" ]; then
        rm -rf "$target"
        echo "- \`$crate\`" >> "$REPORT"
        (( deleted_count++ )) || true
    fi
done
info "  Deleted $deleted_count crate directories."
echo "" >> "$REPORT"

info "  Removing blacklisted top-level dirs/files..."
echo "## Deleted Top-Level Dirs/Files" >> "$REPORT"
for item in "${BLACKLIST_DIRS[@]}"; do
    target="$WORK_DIR/$item"
    if [ -e "$target" ]; then
        rm -rf "$target"
        echo "- \`$item\`" >> "$REPORT"
    fi
done
echo "" >> "$REPORT"

info "  Removing blacklisted GitHub workflows..."
echo "## Deleted GitHub Workflows" >> "$REPORT"
for wf in "${BLACKLIST_GITHUB_WORKFLOWS[@]}"; do
    target="$WORK_DIR/.github/workflows/$wf"
    if [ -f "$target" ]; then
        rm -f "$target"
        echo "- \`$wf\`" >> "$REPORT"
    fi
done
echo "" >> "$REPORT"

info "Step 4/5: Patching zed-core/Cargo.toml and scanning for residual telemetry..."

cp "$REPO_ROOT/zed-core/Cargo.toml" "$WORK_DIR/Cargo.toml"

TELEM_HITS=$(rg -l 'telemetry::event!|sentry::capture|analytics\.' "$WORK_DIR/crates" 2>/dev/null || true)
if [ -n "$TELEM_HITS" ]; then
    warn "  Residual telemetry call-sites found (manual patch needed):"
    echo "## ⚠️  Residual Telemetry Call-Sites" >> "$REPORT"
    while IFS= read -r f; do
        warn "    $f"
        echo "- \`${f#$WORK_DIR/}\`" >> "$REPORT"
    done <<< "$TELEM_HITS"
    echo "" >> "$REPORT"
else
    info "  No residual telemetry. ✓"
fi

find "$WORK_DIR/crates" -name "*.toml" -exec \
    sed -i.bak \
        -e '/^\s*telemetry\s*=/d' \
        -e '/^\s*client\s*=/d' \
        -e '/feature.*zed-pro/d' \
    {} \;
find "$WORK_DIR/crates" -name "*.toml.bak" -delete

info "Step 5/5: Syncing stripped source into zed-core/crates/..."

rsync -a --delete \
    --exclude "diatom_bridge" \
    --exclude "diatom_devtools" \
    "$WORK_DIR/crates/" \
    "$ZED_CORE_DIR/crates/"

info "  Diatom bridge crates preserved (excluded from upstream sync)."

rm -rf "$WORK_DIR"

echo "" >> "$REPORT"
echo "## Summary" >> "$REPORT"
echo "- Zed tag: \`$ZED_TAG\`" >> "$REPORT"
echo "- Timestamp: $TIMESTAMP" >> "$REPORT"
echo "- Deleted crates: $deleted_count" >> "$REPORT"
if [ ${#NEW_CRATES[@]} -gt 0 ]; then
    echo "- **Action required**: ${#NEW_CRATES[@]} new crate(s) need triage" >> "$REPORT"
fi

echo ""
info "=== Done ==="
info "Report saved to: $REPORT"
if [ ${#NEW_CRATES[@]} -gt 0 ]; then
    echo ""
    warn "ACTION REQUIRED: ${#NEW_CRATES[@]} new Zed crate(s) detected."
    warn "Review them in $REPORT and add each to WHITELIST or BLACKLIST."
    exit 1
fi
