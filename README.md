# Zellaude

A Zellij status bar plugin that replaces the default tab bar with Claude Code activity awareness.

![Zellaude status bar example](assets/bar-example.svg)

## Features

- **Full tab bar** — shows all Zellij tabs (not just Claude sessions), replacing the native tab bar
- **Session & mode display** — shows the Zellij session name and current input mode (NORMAL, LOCKED, PANE, etc.) with color-coded indicators
- **Live activity indicators** — see what every Claude Code session is doing at a glance; non-Claude tabs shown dimly
- **Clickable tabs** — click any tab to switch to it
- **Smart pane focus** — clicking a waiting (⚠) session focuses the exact pane so you can respond to the permission prompt immediately
- **Permission flash** — sessions pulse bright yellow for 2 seconds when a permission request arrives
- **Audible bell** — terminal bell (`\x07`) on permission requests and when a session finishes a turn, so you don't miss notifications while focused elsewhere
- **Desktop notifications** — macOS notification on permission requests (rate-limited to once per 10s per tab), with click-to-focus support via [terminal-notifier](https://github.com/julienXX/terminal-notifier)
- **Elapsed time** — shows how long a session has been in its current state (after 30s), making it easy to spot stuck sessions
- **Multi-instance sync** — all Zellij tabs show a unified view of all sessions
- **Cross-session presence** — when attached to one Zellij session, the right edge of the bar shows a `↗ <session> ⚠` indicator for *other* Zellij sessions whose Claude pane is awaiting permission (red) or has just finished a turn (green), so SSH/headless users get an in-bar signal even where pop-ups can't reach. Multiple remotes stack side-by-side up to a configurable cap (default 1, was unbounded prior to this version); extras roll up into a `+N` overflow chip. Optionally **persist** tags so they remain visible after the remote resolves until you click them to dismiss. Persisted tags do *not* survive a Zellij restart — they live in plugin memory only.

### Activity symbols

| Symbol | Meaning |
|--------|---------|
| $\color{#b4afc3}{◆}$ | Session starting |
| $\color{#b48cff}{●}$ | Thinking |
| $\color{#ffaa32}{⚡}$ | Running Bash |
| $\color{#ffaa32}{◉}$ | Reading / searching files |
| $\color{#ffaa32}{✎}$ | Editing / writing files |
| $\color{#ffaa32}{⊜}$ | Spawning subagent |
| $\color{#ffaa32}{◈}$ | Web search / fetch |
| $\color{#ffaa32}{⚙}$ | Other tool |
| $\color{#50c878}{▶}$ | Waiting for user prompt |
| $\color{#ff3c3c}{⚠}$ | Waiting for permission |
| $\color{#50c878}{✓}$ | Done |
| $\color{#b4afc3}{○}$ | Idle |

### Settings

Click the **Zellaude** prefix on the left side of the bar to open the settings menu. Click it again (or the `×` button) to close. Settings are persisted to `~/.config/zellij/plugins/zellaude.json`.

| Setting | JSON key | Options | Default | Description |
|---------|----------|---------|---------|-------------|
| Notifications | `notifications` | `Always` / `Unfocused` / `Never` | `Always` | Desktop notifications on permission requests. "Unfocused" only notifies when the requesting pane is on a different tab. |
| Flash | `flash` | `Persist` / `Once` / `Off` | `Once` | Yellow flash on permission requests. "Persist" keeps flashing until resolved, "Once" flashes for 2 seconds. |
| Beep | `beep` | `On` / `CrossSession` / `Off` | `On` | Terminal bell. `On` beeps on local Waiting/Done events **and** on a new cross-session tag arriving from another Zellij server; `CrossSession` beeps only on cross-session tags (skips local events you can already see); `Off` disables. |
| Elapsed time | `elapsed_time` | `true` / `false` | `true` | Show time since last activity (appears after 30s). |
| Mode indicator | `mode_indicator` | `true` / `false` | `true` | Show the Zellij input-mode pill (NORMAL/LOCKED/PANE/…) next to the Zellaude prefix. |
| Persist tags | `persist_cross_session_tags` | `true` / `false` | `false` | When on, cross-session tags stay visible after the remote leaves the Waiting/Done state until you click the tag to dismiss it. |
| Max tags | `max_cross_session_tags` | `1` / `2` / `3` / `4` | `1` | Maximum number of cross-session tags rendered side-by-side. Extras collapse into a `+N` overflow chip until a slot opens. |
| Tag name max length | `cross_session_tag_max_len` | positive integer | `12` | Maximum characters of a remote session name shown in a cross-session tag before truncation. JSON-only — no menu toggle. |

## Install

### Prerequisites

- [Zellij](https://zellij.dev)
- [jq](https://jqlang.github.io/jq/) — used by the hook script at runtime

### Quick install

Add the plugin to your Zellij layout — that's it:

```kdl
default_tab_template {
    pane size=1 borderless=true {
        plugin location="https://github.com/ishefi/zellaude/releases/latest/download/zellaude.wasm"
    }
    children
}
```

On first load, the plugin automatically installs the hook script and registers it with Claude Code. No cloning, no install scripts.

### Build from source

Prerequisites: [Rust](https://rustup.rs) (in addition to the above)

```bash
git clone https://github.com/ishefi/zellaude.git
cd zellaude
./install.sh
```

This builds the WASM plugin and copies it to `~/.config/zellij/plugins/`. Hook registration happens automatically when the plugin loads.

Then add the plugin to your Zellij layout (replaces the default tab bar):

```kdl
default_tab_template {
    pane size=1 borderless=true {
        plugin location="file:~/.config/zellij/plugins/zellaude.wasm"
    }
    children
}
```

Or try the included layout directly:

```bash
zellij --layout layout.kdl
```

### Optional: click-to-focus notifications

For desktop notifications that focus the right pane when clicked, install [terminal-notifier](https://github.com/julienXX/terminal-notifier):

```bash
brew install terminal-notifier
```

Without it, notifications still appear via osascript but clicking them won't focus the pane.

## Uninstall

```bash
./install.sh --uninstall
```

## How it works

Two components:

1. **WASM plugin** — runs inside Zellij, receives events, maintains state in memory, renders the status bar, sends desktop notifications. On first load, writes the hook script to `~/.config/zellij/plugins/zellaude-hook.sh` and registers it in `~/.claude/settings.json`.
2. **Hook script** — a thin bash bridge that forwards Claude Code hook events to the plugin via `zellij pipe`

```
Claude Code hook → zellaude-hook.sh → zellij pipe → plugin → render
```

The hook script and registration are version-tagged and updated automatically when the plugin version changes.

Plugin instances within the same Zellij server sync via inter-plugin messaging. Across Zellij servers (different sessions), each instance writes its own state to `~/.config/zellij/plugins/zellaude-state.d/<session>.json` and polls peers' files every second — that's how the right-edge cross-session indicator works. Stale entries (>30s) are ignored; writes are coalesced (≤4/sec). Sessions are cleaned up automatically when tabs are closed.

**Note**: Dead-session files (from `kill -9`, crashes, or non-graceful exits) remain in `zellaude-state.d/` indefinitely. Peers correctly ignore them via the 30 s staleness filter, but the files accumulate. Periodic cleanup: `find ~/.config/zellij/plugins/zellaude-state.d -name '*.json' -mtime +1 -delete`.

## License

MIT
