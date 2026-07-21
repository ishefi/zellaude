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
- **Desktop notifications** — macOS notification on permission requests (rate-limited to once per 10s per tab), with click-to-focus support via [terminal-notifier](https://github.com/julienXX/terminal-notifier)
- **Elapsed time** — shows how long a session has been in its current state (after 30s), making it easy to spot stuck sessions
- **Multi-instance sync** — all Zellij tabs show a unified view of all sessions

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

| Setting | Options | Default | Description |
|---------|---------|---------|-------------|
| Notifications | Always / Unfocused / Off | Always | Desktop notifications on permission requests. "Unfocused" only notifies when the requesting pane is on a different tab. |
| Flash | Persist / Brief / Off | Brief | Yellow flash on permission requests. "Persist" keeps flashing until resolved, "Brief" flashes for 2 seconds. |
| Elapsed time | On / Off | On | Show time since last activity (appears after 30s). |

### Theming

Zellaude ships with the default palette shown above. You can override any color,
or have the bar follow your active Zellij theme, by adding configuration to the
plugin block in your layout:

```kdl
plugin location="file:~/.config/zellij/plugins/zellaude.wasm" {
    // Optional: derive surfaces/text from the active Zellij theme.
    // "builtin" (default) keeps Zellaude's own colors.
    theme_source "zellij"

    // Override individual roles. Values are hex (#rrggbb or #rgb) or
    // r,g,b / r g b triplets.
    thinking      "#b48cff"
    tab_active_bg "140,100,200"
}
```

Resolution order, lowest to highest precedence: built-in defaults → Zellij theme
(only when `theme_source "zellij"`) → explicit overrides. Unknown keys are
ignored; an invalid color keeps that role's default.

When `theme_source "zellij"` is set, surfaces and text follow the theme while the
semantic status hues (e.g. red = waiting, green = done) keep their meaning unless
you override them.

Overridable roles:

| Role | Default | Drives |
|------|---------|--------|
| `thinking` | `#b48cff` | Thinking glyph · Tab/Session mode pill |
| `tool` | `#ffaa32` | Tool glyphs · Resize/Move mode pill |
| `waiting` | `#ff3c3c` | Waiting glyph · Locked mode pill · menu close |
| `success` | `#50c878` | Prompting/Done glyphs · Normal/Prompt/Tmux pill · menu "on" |
| `notification` | `#c8c864` | Notification glyph · Scroll/Search/Rename pill |
| `accent_blue` | `#50b4ff` | Pane mode pill |
| `neutral` | `#b4afc3` | Init / Idle glyphs |
| `bar_bg` | `#1e1e2e` | Bar background · dark mode-pill text |
| `prefix_bg` | `#3c3250` | "Zellaude" prefix |
| `prefix_bg_active` | `#64468c` | Prefix while the settings menu is open |
| `tab_active_bg` | `#8c64c8` | Active tab background |
| `tab_inactive_bg` | `#504b6e` | Inactive tab background |
| `flash_bg` | `#50501e` | Tab background during a permission flash |
| `text` | `#ffffff` | Prefix label · active tab names |
| `text_dim` | `#78dcdc` | Inactive Claude tab name |
| `text_muted` | `#aaa5b9` | Inactive non-Claude tab name |
| `disabled` | `#646464` | Settings-menu "off" items |
| `elapsed` | `#a5a0b4` | Elapsed-time suffix |
| `flash_text` | `#ffff50` | Tab text during a flash |
| `fullscreen` | `#ffc83c` | Fullscreen `F` · menu amber accents |

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
The registered hook command uses `${HOME}/.config/zellij/plugins/zellaude-hook.sh`; Claude Code expands `${HOME}` when it runs hooks, keeping the settings entry portable across machines.

All state lives in WASM memory. No temp files, no race conditions. Multiple plugin instances (one per tab) sync state automatically via inter-plugin messaging. Sessions are cleaned up automatically when tabs are closed.

## License

MIT
