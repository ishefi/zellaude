use std::collections::BTreeMap;
use zellij_tile::prelude::run_command;

const HOOK_VERSION_TAG: &str = concat!("# zellaude v", env!("CARGO_PKG_VERSION"));

/// Generate hook script content with version tag inserted after the shebang.
fn hook_script_content() -> String {
    let original = include_str!("../scripts/zellaude-hook.sh");
    if let Some(pos) = original.find('\n') {
        let (shebang, rest) = original.split_at(pos);
        format!("{shebang}\n{HOOK_VERSION_TAG}{rest}")
    } else {
        original.to_string()
    }
}

const CLAUDE_EVENTS: &str = r#"["PreToolUse","PostToolUse","PostToolUseFailure","UserPromptSubmit","PermissionRequest","Notification","Stop","SubagentStop","SessionStart","SessionEnd"]"#;
const CODEX_EVENTS: &str =
    r#"["SessionStart","PreToolUse","PostToolUse","UserPromptSubmit","PermissionRequest","Stop"]"#;

const INSTALL_TEMPLATE: &str = r##"set -e
HOOK_PATH="$HOME/.config/zellij/plugins/zellaude-hook.sh"
CLAUDE_SETTINGS="$HOME/.claude/settings.json"
CODEX_HOOKS="$HOME/.codex/hooks.json"

# Write (or refresh) the hook script itself. Always do this so version tags
# stay in sync even if only one agent is installed.
if ! grep -qF '__VERSION_TAG__' "$HOOK_PATH" 2>/dev/null; then
  mkdir -p "$(dirname "$HOOK_PATH")"
  cat > "$HOOK_PATH" << 'ZELLAUDE_HOOK_EOF'
__HOOK_SCRIPT__
ZELLAUDE_HOOK_EOF
  chmod +x "$HOOK_PATH"
fi

# Hook registration requires jq. Without it we just bail quietly so zellij
# still loads; the user can install jq and restart to pick up hooks later.
if ! command -v jq >/dev/null 2>&1; then
  echo "no_jq"
  exit 0
fi

# Register hooks for a given agent config file + event list + argv tag.
register_hooks() {
  local file="$1"        # e.g. ~/.claude/settings.json
  local agent_home="$2"  # e.g. ~/.claude (must exist; skip otherwise)
  local events_json="$3"
  local argv="$4"        # "claude" or "codex" (passed to the hook script)

  [ ! -d "$agent_home" ] && return 0

  mkdir -p "$(dirname "$file")"
  [ ! -f "$file" ] && echo '{}' > "$file"

  cp "$file" "$file.bak"

  local cmd="$HOOK_PATH $argv"

  # Remove any prior zellaude entries for THIS agent (argv), regardless of
  # which path the hook script lives at. This avoids duplicates when users
  # flip between the repo-local install.sh and the plugin auto-installer.
  local tmp
  tmp=$(mktemp)
  jq --arg argv "$argv" '
    def is_zellaude($argv):
      (.command // "") as $cmd |
      if $argv == "claude" then
        ($cmd | test("zellaude-hook\\.sh(\\s+claude)?\\s*$"))
      else
        ($cmd | test("zellaude-hook\\.sh\\s+" + $argv + "\\s*$"))
      end;
    if .hooks and (.hooks | type == "object") then
      .hooks |= with_entries(
        .value |= [
          .[] | . as $group |
          ($group.hooks // []) | map(select(is_zellaude($argv) | not)) |
          . as $filtered |
          if length > 0 then ($group | .hooks = $filtered) else empty end
        ]
      ) | .hooks |= with_entries(select(.value | length > 0)) |
      if .hooks == {} then del(.hooks) else . end
    else . end
  ' "$file" > "$tmp" && mv "$tmp" "$file"

  local entry
  if [ "$argv" = "claude" ]; then
    entry=$(jq -nc --arg cmd "$cmd" '[{"hooks": [{"type": "command", "command": $cmd, "timeout": 5, "async": true}]}]')
  else
    entry=$(jq -nc --arg cmd "$cmd" '[{"hooks": [{"type": "command", "command": $cmd, "timeout": 5}]}]')
  fi

  tmp=$(mktemp)
  jq --argjson events "$events_json" --argjson entry "$entry" '
    .hooks //= {} |
    reduce ($events[]) as $event (.; .hooks[$event] = (.hooks[$event] // []) + $entry)
  ' "$file" > "$tmp" && mv "$tmp" "$file"
}

register_hooks "$CLAUDE_SETTINGS" "$HOME/.claude" '__CLAUDE_EVENTS__' "claude"
register_hooks "$CODEX_HOOKS"     "$HOME/.codex"  '__CODEX_EVENTS__'  "codex"

echo "installed"
"##;

/// Run the idempotent hook installation command.
/// Registers hooks for both Claude Code and Codex CLI if their home dirs exist.
pub fn run_install() {
    let cmd = INSTALL_TEMPLATE
        .replace("__VERSION_TAG__", HOOK_VERSION_TAG)
        .replace("__HOOK_SCRIPT__", &hook_script_content())
        .replace("__CLAUDE_EVENTS__", CLAUDE_EVENTS)
        .replace("__CODEX_EVENTS__", CODEX_EVENTS);

    let mut ctx = BTreeMap::new();
    ctx.insert("type".into(), "install_hooks".into());
    run_command(&["sh", "-c", &cmd], ctx);
}
