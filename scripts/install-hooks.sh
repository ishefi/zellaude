#!/usr/bin/env bash
# install-hooks.sh — Register zellaude hooks in Claude Code and Codex CLI configs.
#
# Adds entries for whichever agents are installed (detected by ~/.claude or
# ~/.codex existing). Each agent gets a distinct argv on the hook command so
# the plugin can tell them apart.
#
# Usage: ./scripts/install-hooks.sh [--uninstall]
set -euo pipefail

HOOK_SCRIPT="$(cd "$(dirname "$0")" && pwd)/zellaude-hook.sh"

CLAUDE_SETTINGS="$HOME/.claude/settings.json"
CODEX_HOOKS="$HOME/.codex/hooks.json"

CLAUDE_EVENTS='["PreToolUse","PostToolUse","PostToolUseFailure","UserPromptSubmit","PermissionRequest","Notification","Stop","SubagentStop","SessionStart","SessionEnd"]'
CODEX_EVENTS='["SessionStart","PreToolUse","PostToolUse","UserPromptSubmit","PermissionRequest","Notification","Stop","SessionEnd"]'

if ! command -v jq &>/dev/null; then
  echo "Error: jq is required. Install with: brew install jq" >&2
  exit 1
fi

if [ ! -f "$HOOK_SCRIPT" ]; then
  echo "Error: Hook script not found at $HOOK_SCRIPT" >&2
  exit 1
fi

backup() {
  local file="$1"
  [ -f "$file" ] && cp "$file" "$file.bak" && echo "Backed up $file to $file.bak"
}

remove_entries() {
  local file="$1"
  local argv="$2"
  [ ! -f "$file" ] && return 0
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
  ' "$file" > "$tmp"
  mv "$tmp" "$file"
}

hook_entry() {
  local cmd="$1"
  local argv="$2"

  if [ "$argv" = "claude" ]; then
    jq -nc --arg cmd "$cmd" '[{
      "hooks": [{
        "type": "command",
        "command": $cmd,
        "timeout": 5,
        "async": true
      }]
    }]'
  else
    jq -nc --arg cmd "$cmd" '[{
      "hooks": [{
        "type": "command",
        "command": $cmd,
        "timeout": 5
      }]
    }]'
  fi
}

install_for_agent() {
  local file="$1"
  local home_dir="$2"
  local events="$3"
  local argv="$4"

  if [ ! -d "$home_dir" ]; then
    echo "Skipping: $home_dir does not exist"
    return 0
  fi

  local cmd="$HOOK_SCRIPT $argv"
  mkdir -p "$(dirname "$file")"
  [ ! -f "$file" ] && echo '{}' > "$file"

  backup "$file"
  remove_entries "$file" "$argv"

  local entry
  entry=$(hook_entry "$cmd" "$argv")

  local tmp
  tmp=$(mktemp)
  jq --argjson events "$events" --argjson entry "$entry" '
    .hooks //= {} |
    reduce ($events[]) as $event (.; .hooks[$event] = (.hooks[$event] // []) + $entry)
  ' "$file" > "$tmp"
  mv "$tmp" "$file"
  echo "Installed zellaude hooks ($argv) into $file"
}

uninstall_for_agent() {
  local file="$1"
  local argv="$2"

  if [ ! -f "$file" ]; then
    echo "No config at $file — nothing to remove"
    return 0
  fi

  backup "$file"
  remove_entries "$file" "$argv"
  echo "Uninstalled zellaude hooks ($argv) from $file"
}

case "${1:-}" in
  --uninstall)
    uninstall_for_agent "$CLAUDE_SETTINGS" "claude"
    uninstall_for_agent "$CODEX_HOOKS"     "codex"
    ;;
  *)
    install_for_agent "$CLAUDE_SETTINGS" "$HOME/.claude" "$CLAUDE_EVENTS" "claude"
    install_for_agent "$CODEX_HOOKS"     "$HOME/.codex"  "$CODEX_EVENTS"  "codex"
    echo "Hook script: $HOOK_SCRIPT"
    ;;
esac
