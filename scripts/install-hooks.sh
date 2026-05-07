#!/usr/bin/env bash
# install-hooks.sh — Add zellaude hook entries to ~/.claude/settings.json
#
# Usage: ./scripts/install-hooks.sh [--uninstall]
set -euo pipefail

SETTINGS="$HOME/.claude/settings.json"
HOOK_SCRIPT="$(cd "$(dirname "$0")" && pwd)/zellaude-hook.sh"
HOOK_CMD='${HOME}/.config/zellij/plugins/zellaude-hook.sh'

resolve_file_symlink() {
  local path dir target
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

if [ -L "$SETTINGS" ]; then
  SETTINGS="$(resolve_file_symlink "$SETTINGS")"
fi

if ! command -v jq &>/dev/null; then
  echo "Error: jq is required. Install with: brew install jq" >&2
  exit 1
fi

if [ ! -f "$HOOK_SCRIPT" ]; then
  echo "Error: Hook script not found at $HOOK_SCRIPT" >&2
  exit 1
fi

# The hook entry shared by all events
HOOK_ENTRY=$(jq -nc --arg cmd "$HOOK_CMD" '[{
  "hooks": [{
    "type": "command",
    "command": $cmd,
    "timeout": 5,
    "async": true
  }]
}]')

EVENTS='["PreToolUse","PostToolUse","PostToolUseFailure","UserPromptSubmit","PermissionRequest","Notification","Stop","SubagentStop","SessionStart","SessionEnd"]'

backup_settings() {
  if [ -f "$SETTINGS" ]; then
    cp "$SETTINGS" "$SETTINGS.bak"
    echo "Backed up $SETTINGS to $SETTINGS.bak"
  fi
}

uninstall() {
  if [ ! -f "$SETTINGS" ]; then
    echo "No settings file found at $SETTINGS"
    exit 0
  fi

  backup_settings

  # Remove only zellaude hook entries
  local tmp
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
  ' "$SETTINGS" > "$tmp"
  mv "$tmp" "$SETTINGS"
  echo "Uninstalled zellaude hooks from $SETTINGS"
}

install() {
  # Create settings file if it doesn't exist
  if [ ! -f "$SETTINGS" ]; then
    mkdir -p "$(dirname "$SETTINGS")"
    echo '{}' > "$SETTINGS"
  fi

  backup_settings

  # First uninstall any existing zellaude hooks to avoid duplicates
  uninstall 2>/dev/null || true

  # Add hook entries for each event
  local tmp
  tmp=$(mktemp)
  jq --argjson events "$EVENTS" --argjson entry "$HOOK_ENTRY" '
    .hooks //= {} |
    reduce ($events[]) as $event (.; .hooks[$event] = (.hooks[$event] // []) + $entry)
  ' "$SETTINGS" > "$tmp"
  mv "$tmp" "$SETTINGS"
  echo "Installed zellaude hooks into $SETTINGS"
  echo "Hook script: $HOOK_SCRIPT"
  echo "Events: PreToolUse, PostToolUse, UserPromptSubmit, PermissionRequest, Notification, Stop, SubagentStop, SessionStart, SessionEnd"
}

case "${1:-}" in
  --uninstall)
    uninstall
    ;;
  *)
    install
    ;;
esac
