#!/usr/bin/env bash
# zellaude-hook.sh — Claude Code hook → zellij pipe bridge
# Forwards hook events to the zellaude Zellij plugin via pipe.
#
# Usage in ~/.claude/settings.json hooks:
#   "command": "/path/to/zellaude-hook.sh"

# Exit silently if not running inside Zellij
[ -z "$ZELLIJ_SESSION_NAME" ] && exit 0
[ -z "$ZELLIJ_PANE_ID" ] && exit 0

# Capture send-time immediately so the plugin can order events
# that race through parallel hook subprocesses.
TS_MS=$(jq -nc 'now * 1000 | floor')

# Read hook JSON from stdin
INPUT=$(cat)

# Extract fields with jq (required dependency)
HOOK_EVENT=$(echo "$INPUT" | jq -r '.hook_event_name // empty')
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // empty')
CWD=$(echo "$INPUT" | jq -r '.cwd // empty')

[ -z "$HOOK_EVENT" ] && exit 0

# Build compact JSON payload
PAYLOAD=$(jq -nc \
  --arg pane_id "$ZELLIJ_PANE_ID" \
  --arg session_id "$SESSION_ID" \
  --arg hook_event "$HOOK_EVENT" \
  --arg tool_name "$TOOL_NAME" \
  --arg cwd "$CWD" \
  --arg zellij_session "$ZELLIJ_SESSION_NAME" \
  --arg term_program "${TERM_PROGRAM:-}" \
  --arg ts_ms "$TS_MS" \
  '{
    pane_id: ($pane_id | tonumber),
    session_id: $session_id,
    hook_event: $hook_event,
    tool_name: (if $tool_name == "" then null else $tool_name end),
    cwd: (if $cwd == "" then null else $cwd end),
    zellij_session: $zellij_session,
    term_program: (if $term_program == "" then null else $term_program end),
    ts_ms: ($ts_ms | tonumber)
  }')

# Permission request: bell + desktop notification
if [ "$HOOK_EVENT" = "PermissionRequest" ]; then
  printf '\a' > /dev/tty 2>/dev/null || true

  # Read notification setting (default: Always)
  SETTINGS_FILE="$HOME/.config/zellij/plugins/zellaude.json"
  NOTIFY_MODE="Always"
  if [ -f "$SETTINGS_FILE" ]; then
    NOTIFY_MODE=$(jq -r '.notifications // "Always"' "$SETTINGS_FILE" 2>/dev/null)
  fi

  # Returns 0 if the terminal emulator window is frontmost on the OS, 1 otherwise.
  # Graceful-degrades to 1 when the required OS tools aren't available.
  is_terminal_frontmost() {
    case "$(uname)" in
      Darwin)
        local expected="${TERM_PROGRAM:-}"
        case "$expected" in
          Apple_Terminal) expected="Terminal" ;;
          iTerm.app)     expected="iTerm2" ;;
        esac
        local front_app
        front_app=$(osascript -e 'tell application "System Events" to get name of first application process whose frontmost is true' 2>/dev/null)
        [ "$front_app" = "$expected" ]
        ;;
      Linux)
        # X11 only: Wayland has no standard API.
        command -v xdotool >/dev/null 2>&1 || return 1
        local active_pid
        active_pid=$(xdotool getactivewindow getwindowpid 2>/dev/null)
        [ -n "$active_pid" ] || return 1
        # Walk up the process tree; if the focused window's process is an
        # ancestor of our shell, the terminal is frontmost.
        local pid=$$
        while [ "$pid" -gt 1 ] 2>/dev/null; do
          [ "$pid" = "$active_pid" ] && return 0
          pid=$(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ')
        done
        return 1
        ;;
      *) return 1 ;;
    esac
  }

  # Returns 0 if a zellij client in this session is focused on the pane that
  # fired the hook, 1 otherwise. Returns 2 if zellij couldn't answer — callers
  # treat that as "unknown, fall back to the OS-level frontmost check".
  is_this_pane_focused_in_session() {
    command -v zellij >/dev/null 2>&1 || return 2
    local clients
    clients=$(zellij -s "$ZELLIJ_SESSION_NAME" action list-clients 2>/dev/null) || return 2
    # Drop header, select the ZELLIJ_PANE_ID column (col 2), match `terminal_<id>`.
    echo "$clients" | tail -n +2 | awk '{print $2}' | grep -qx "terminal_${ZELLIJ_PANE_ID}"
  }

  SHOULD_NOTIFY=false
  case "$NOTIFY_MODE" in
    Always) SHOULD_NOTIFY=true ;;
    Unfocused)
      is_this_pane_focused_in_session
      case $? in
        0)
          # Pane is focused in zellij — suppress only if the terminal window
          # itself is also frontmost on the OS.
          is_terminal_frontmost || SHOULD_NOTIFY=true
          ;;
        1)
          # Session detached, or user is on a different pane in this session.
          SHOULD_NOTIFY=true
          ;;
        *)
          # zellij couldn't answer — fall back to Phase 1 behavior.
          is_terminal_frontmost || SHOULD_NOTIFY=true
          ;;
      esac
      ;;
  esac

  if [ "$SHOULD_NOTIFY" = true ]; then
    TITLE="⚠ Claude Code — session \"${ZELLIJ_SESSION_NAME}\""
    MESSAGE="Permission requested"
    [ -n "$TOOL_NAME" ] && MESSAGE="${MESSAGE} — ${TOOL_NAME}"
    if [ -n "$CWD" ]; then
      CWD_DISPLAY="$CWD"
      case "$CWD_DISPLAY" in
        "$HOME") CWD_DISPLAY="~" ;;
        "$HOME"/*) CWD_DISPLAY="~/${CWD_DISPLAY#"$HOME"/}" ;;
      esac
      MESSAGE="${MESSAGE} in ${CWD_DISPLAY}"
    fi

    # Rate-limit: one notification per pane per 10 seconds
    LOCK="/tmp/zellaude-notify-${ZELLIJ_PANE_ID}"
    NOW=$(date +%s)
    LAST=0
    [ -f "$LOCK" ] && LAST=$(cat "$LOCK" 2>/dev/null)
    if [ $((NOW - LAST)) -ge 10 ]; then
      echo "$NOW" > "$LOCK"

      # Click callback: activate terminal + focus the pane
      ZELLIJ_BIN=$(command -v zellij)
      FOCUS_CMD="${ZELLIJ_BIN} -s '${ZELLIJ_SESSION_NAME}' pipe --name zellaude:focus -- ${ZELLIJ_PANE_ID}"

      case "$(uname)" in
        Darwin)
          [ -n "${TERM_PROGRAM:-}" ] && FOCUS_CMD="open -a '${TERM_PROGRAM}' && ${FOCUS_CMD}"
          if command -v terminal-notifier >/dev/null 2>&1; then
            terminal-notifier \
              -title "$TITLE" \
              -message "$MESSAGE" \
              -execute "$FOCUS_CMD" &
          else
            # Pass TITLE/MESSAGE as argv so AppleScript treats them as data,
            # not source — session names and cwds are user-controlled and
            # may contain quotes, backslashes, or newlines.
            osascript \
              -e 'on run argv' \
              -e 'display notification (item 1 of argv) with title (item 2 of argv)' \
              -e 'end run' \
              -- "$MESSAGE" "$TITLE" &
          fi
          ;;
        Linux)
          if command -v notify-send >/dev/null 2>&1; then
            notify-send "$TITLE" "$MESSAGE" &
          fi
          ;;
      esac
    fi
  fi
fi

# Send to plugin (hook is already async, no need to background)
zellij pipe --name "zellaude" -- "$PAYLOAD"
