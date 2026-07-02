#!/usr/bin/env bash
# zellaude-fake-cross-session.sh — Simulate a remote Zellij session entering
# Waiting (or Done) by writing a peer state file directly. Useful for testing
# the cross-session tag + beep without spinning up another Zellij server and
# Claude Code instance.
#
# How it works: the plugin polls $HOME/.config/zellij/plugins/zellaude-state.d/
# every ~1s. The cross-session beep fires on a transition into Waiting/Done.
# We write Idle first, sleep through one poll, then flip to Waiting — that
# guarantees a clean transition even if a stale file from a previous run
# already had us parked in Waiting.
#
# Usage:
#   ./scripts/zellaude-fake-cross-session.sh              # 10s delay, Idle -> Waiting
#   DELAY=0 ./scripts/zellaude-fake-cross-session.sh      # fire immediately
#   DELAY=30 ./scripts/zellaude-fake-cross-session.sh     # longer window
#   KIND=Done ./scripts/zellaude-fake-cross-session.sh    # Idle -> Done
#   SESSION=ghost ./scripts/zellaude-fake-cross-session.sh
#   ./scripts/zellaude-fake-cross-session.sh --cleanup    # remove the fake file

set -euo pipefail

DIR="$HOME/.config/zellij/plugins/zellaude-state.d"
SESSION="${SESSION:-fake-peer}"
KIND="${KIND:-Waiting}"
DELAY="${DELAY:-10}"
FILE="$DIR/$SESSION.json"

if [ "${1:-}" = "--cleanup" ]; then
  rm -f "$FILE"
  echo "Removed $FILE"
  exit 0
fi

case "$KIND" in
  Waiting|Done|AgentDone) ;;
  *) echo "KIND must be Waiting, Done, or AgentDone (got: $KIND)" >&2; exit 1 ;;
esac

mkdir -p "$DIR"

write_state() {
  local activity="$1"
  local now_ms now_s
  now_ms=$(jq -nc 'now * 1000 | floor')
  now_s=$(jq -nc 'now | floor')
  jq -nc \
    --arg session "$SESSION" \
    --arg activity "$activity" \
    --argjson ts_s "$now_s" \
    --argjson ts_ms "$now_ms" \
    '{
      session_name: $session,
      sessions: {
        "0": {
          session_id: "fake-cross-session-test",
          pane_id: 0,
          activity: $activity,
          tab_name: "fake",
          tab_index: 0,
          last_event_ts: $ts_s,
          cwd: "/tmp",
          last_ts_ms: $ts_ms
        }
      },
      wrote_at_ms: $ts_ms
    }' > "$FILE.tmp.$$"
  mv "$FILE.tmp.$$" "$FILE"
}

echo "Step 1: writing Idle for session=\"$SESSION\""
write_state Idle
echo "  $FILE"

# Wait long enough for the plugin to observe Idle, and also long enough for
# the user to switch sessions/panes if they want to verify cross-session
# notification from a different focus. Min 2s for the plugin's poll cycle;
# default 10s for a comfortable switching window.
WAIT_S="$DELAY"
if [ "$WAIT_S" -lt 2 ]; then
  WAIT_S=2
fi
if [ "$DELAY" -gt 0 ]; then
  echo "Sleeping ${WAIT_S}s — switch session/pane now if you want to test from a different focus."
else
  echo "Sleeping ${WAIT_S}s for the plugin to poll Idle first..."
fi
sleep "$WAIT_S"

echo "Step 2: flipping to $KIND — this is the transition the plugin beeps on"
write_state "$KIND"
echo "  $FILE"
echo
echo "Watch the status bar for a ↗ tag, and listen for the bell."
echo "Cleanup when done:  $0 --cleanup"
