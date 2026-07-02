#!/usr/bin/env bash
# zellaude-trigger-test.sh — Fire a fake PermissionRequest at the installed
# zellaude hook for manual testing (bypasses Claude Code entirely).
#
# Run from inside a zellij pane in the session you want to be the "originating
# pane" — env vars ZELLIJ_SESSION_NAME and ZELLIJ_PANE_ID are picked up from
# here. Sleeps for DELAY seconds first so you can rearrange focus state (switch
# pane, switch session) before the hook fires.
#
# Usage:
#   ./scripts/zellaude-trigger-test.sh                      # default: 10s delay
#   DELAY=0 ./scripts/zellaude-trigger-test.sh              # fire immediately
#   DELAY=60 ./scripts/zellaude-trigger-test.sh             # longer window
#   HOOK=/path/to/hook ./scripts/zellaude-trigger-test.sh
#   PATH=/usr/bin:/bin ./scripts/zellaude-trigger-test.sh   # simulate missing zellij

set -euo pipefail

HOOK="${HOOK:-$HOME/.config/zellij/plugins/zellaude-hook.sh}"
DELAY="${DELAY:-10}"

if [ -z "${ZELLIJ_SESSION_NAME:-}" ] || [ -z "${ZELLIJ_PANE_ID:-}" ]; then
  echo "Run this from inside a zellij pane (need ZELLIJ_SESSION_NAME and ZELLIJ_PANE_ID)." >&2
  exit 1
fi

if [ ! -x "$HOOK" ]; then
  echo "Hook not found or not executable: $HOOK" >&2
  exit 1
fi

# Defeat the 10s rate-limit so reruns fire reliably.
rm -f "/tmp/zellaude-notify-${ZELLIJ_PANE_ID}"

if [ "$DELAY" -gt 0 ]; then
  echo "Will trigger PermissionRequest in ${DELAY}s — arrange focus state now."
  echo "  session=$ZELLIJ_SESSION_NAME pane=$ZELLIJ_PANE_ID"
  echo "  hook=$HOOK"
  sleep "$DELAY"
else
  echo "Triggering PermissionRequest now (session=$ZELLIJ_SESSION_NAME pane=$ZELLIJ_PANE_ID)."
fi

jq -nc \
  --arg hook PermissionRequest \
  --arg sid "test-$(date +%s)" \
  --arg tool Bash \
  --arg cwd "$PWD" \
  '{hook_event_name: $hook, session_id: $sid, tool_name: $tool, cwd: $cwd}' \
  | "$HOOK"

echo "Fired."
