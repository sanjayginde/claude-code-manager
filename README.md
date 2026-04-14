# ccm — Claude Code Manager

A terminal UI for browsing and resuming [Claude Code](https://claude.ai/code) sessions across all your projects.

![Rust](https://img.shields.io/badge/built_with-Rust-orange)

## Features

- Browse all Claude Code projects and sessions in a split-pane TUI
- See session age, git branch, and file size at a glance
- AI-generated concise titles for each session (cached locally, powered by Claude Haiku)
- Full first message preview when navigating sessions
- Resume any session — `ccm` `cd`s to the original project directory and hands off to `claude --resume`
- Edit session titles inline
- Copy the first message to the clipboard
- Delete old sessions with confirmation
- Themeable: built-in named themes and per-color config file overrides

## Installation

Requires [Rust](https://rustup.rs) and the `claude` CLI.

```bash
git clone https://github.com/sanjayginde/claude-code-manager
cd claude-code-manager
cargo install --path .
```

## Usage

```bash
ccm [--theme <name>]
```

### Keys

| Key | Action |
|-----|--------|
| `↑↓` / `jk` | Navigate |
| `Tab` / `←→` / `h` / `l` | Switch pane |
| `Enter` | Resume session / switch to sessions pane |
| `e` | Edit session title inline |
| `y` | Copy first message to clipboard |
| `d` | Delete session (with confirmation) |
| `q` / `Ctrl+C` | Quit |

### Theming

Pass `--theme <name>` on the command line or set `theme` in `~/.config/ccm/config.toml`.

Available themes: `gruvbox-dark` (default), `catppuccin-mocha`, `catppuccin-macchiato`, `catppuccin-frappe`, `catppuccin-latte`

Individual colors can be overridden in the config file regardless of which theme is active:

```toml
# ~/.config/ccm/config.toml
theme = "catppuccin-mocha"

[colors]
active = "#ff9900"   # active pane border, edit cursor
danger = "#ff0000"   # delete confirmation
```

Available color keys: `active`, `inactive`, `meta`, `preview_text`, `status_msg`, `hint`, `danger`

Precedence: `--theme` flag > config `theme` > `[colors]` overrides > default (`gruvbox-dark`)

### AI Titles

If `ANTHROPIC_API_KEY` is set, `ccm` generates a short title for each session using Claude Haiku on first view. Titles are cached as `.title` files alongside the session data in `~/.claude/projects/` and loaded instantly on subsequent runs.

If no API key is present, the first message is truncated and shown as the title instead.

## How it works

Claude Code stores sessions as JSONL files under `~/.claude/projects/<encoded-path>/<uuid>.jsonl`. `ccm` reads these directly, extracting the working directory, git branch, first user message, and file metadata to build the UI.

## Built with Claude Code

This app was built entirely through an interactive session with [Claude Code](https://claude.ai/code) — from design (via `/grill-me`) through implementation. The TUI is powered by [ratatui](https://ratatui.rs).
