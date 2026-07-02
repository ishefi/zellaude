use std::collections::BTreeMap;
use zellij_tile::prelude::run_command;

const HOOK_VERSION_TAG: &str = concat!("# zellaude v", env!("CARGO_PKG_VERSION"));

/// Generate hook script content with version tag inserted after the shebang.
fn hook_script_content() -> String {
    let original = include_str!("../scripts/zellaude-hook.sh");
    // Insert version tag after the shebang line
    if let Some(pos) = original.find('\n') {
        let (shebang, rest) = original.split_at(pos);
        format!("{shebang}\n{HOOK_VERSION_TAG}{rest}")
    } else {
        original.to_string()
    }
}

const INSTALL_TEMPLATE: &str = r##"set -e
HOOK_PATH="$HOME/.config/zellij/plugins/zellaude-hook.sh"
HOOK_CMD='${HOME}/.config/zellij/plugins/zellaude-hook.sh'
SETTINGS="$HOME/.claude/settings.json"

resolve_file_symlink() {
  path=$1
  while [ -L "$path" ]; do
    dir=$(cd "$(dirname "$path")" && pwd -P)
    target=$(readlink "$path")
    case "$target" in
      /*) path=$target ;;
      *) path=$dir/$target ;;
    esac
  done
  dir=$(cd "$(dirname "$path")" && pwd -P)
  printf '%s/%s\n' "$dir" "$(basename "$path")"
}

# Resolve symlink so mv doesn't replace the link with a regular file
if [ -L "$SETTINGS" ]; then
  SETTINGS="$(resolve_file_symlink "$SETTINGS")"
fi

# Check if already current
if grep -qF '__VERSION_TAG__' "$HOOK_PATH" 2>/dev/null; then
  if [ -f "$SETTINGS" ] && grep -qF "$HOOK_CMD" "$SETTINGS" 2>/dev/null; then
    echo "current"
    exit 0
  fi
fi

# Write hook script
mkdir -p "$(dirname "$HOOK_PATH")"
cat > "$HOOK_PATH" << 'ZELLAUDE_HOOK_EOF'
__HOOK_SCRIPT__
ZELLAUDE_HOOK_EOF
chmod +x "$HOOK_PATH"

# Register hooks (requires jq)
if ! command -v jq >/dev/null 2>&1; then
  echo "no_jq"
  exit 0
fi

if [ ! -f "$SETTINGS" ]; then
  mkdir -p "$HOME/.claude"
  echo '{}' > "$SETTINGS"
fi

# Back up settings before modifying
cp "$SETTINGS" "$SETTINGS.bak"

# Remove ALL existing zellaude hook entries (any path ending in zellaude-hook.sh)
tmp=$(mktemp)
jq '
  if .hooks and (.hooks | type == "object") then
    .hooks |= with_entries(
      .value |= [
        .[] | . as $group |
        ($group.hooks // []) | map(select((.command // "") | endswith("zellaude-hook.sh") | not)) |
        . as $filtered |
        if length > 0 then ($group | .hooks = $filtered) else empty end
      ]
    ) | .hooks |= with_entries(select(.value | length > 0)) |
    if .hooks == {} then del(.hooks) else . end
  else . end
' "$SETTINGS" > "$tmp" && mv "$tmp" "$SETTINGS"

# Add new hook entries with literal ${HOME} to keep settings portable
EVENTS='["PreToolUse","PostToolUse","PostToolUseFailure","UserPromptSubmit","PermissionRequest","Notification","Stop","SubagentStop","SessionStart","SessionEnd"]'
ENTRY=$(jq -nc --arg cmd "$HOOK_CMD" '[{"hooks": [{"type": "command", "command": $cmd, "timeout": 5, "async": true}]}]')
tmp=$(mktemp)
jq --argjson events "$EVENTS" --argjson entry "$ENTRY" '
  .hooks //= {} |
  reduce ($events[]) as $event (.; .hooks[$event] = (.hooks[$event] // []) + $entry)
' "$SETTINGS" > "$tmp" && mv "$tmp" "$SETTINGS"

echo "installed"
"##;

/// Run the idempotent hook installation command.
/// Checks if hooks are current, writes the hook script, and registers hooks.
pub fn run_install() {
    let cmd = INSTALL_TEMPLATE
        .replace("__VERSION_TAG__", HOOK_VERSION_TAG)
        .replace("__HOOK_SCRIPT__", &hook_script_content());

    let mut ctx = BTreeMap::new();
    ctx.insert("type".into(), "install_hooks".into());
    run_command(&["sh", "-c", &cmd], ctx);
}
