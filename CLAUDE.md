# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

**CCM (Claude Code Manager)** — a terminal UI (TUI) for browsing, managing, and resuming Claude Code sessions. It reads JSONL session files from `~/.claude/projects/`, generates AI titles via the Anthropic API, and lets users resume sessions by exec'ing `claude --resume <uuid>`.

## Commands

```bash
cargo build                          # build
cargo run                            # run
cargo test                           # run all tests
cargo test <test_name>               # run a single test (substring match)
cargo test -p ccm -- <module>::      # run tests in a specific module
cargo clippy -- -D warnings          # lint (strict: warnings = errors)
cargo install --path .               # install the binary
```

CI runs clippy, build, and test on pushes to `main` and on PRs targeting `main`.

## Architecture

Seven source modules with clear responsibilities:

| Module | Role |
|---|---|
| `main.rs` | Startup: load config, resolve theme, load sessions, start title service, run TUI event loop. On resume, `exec()` into `claude --resume <uuid>` (Unix) or spawn+wait (Windows). |
| `app.rs` | All application state and business logic. **Single entry point:** `dispatch(Action) -> Response`. Manages pane selection, modal state (None / EditTitle / ConfirmDelete), deletion, title editing, clipboard. ~25 unit tests. |
| `config.rs` | Loads `~/.config/ccm/config.toml` (TOML). `Config` has a `theme: Option<String>` and `[colors]` overrides (`ColorOverrides`). Silently returns defaults on missing file or parse error. |
| `data.rs` | Domain models (`Session`, `Project`) and JSONL parsing. `SessionTitle` enum: `Absent | Loaded | Unreadable`. `parse_header_from_reader()` extracts cwd, git branch, and first user message. Cleans XML tags and skill markup. ~20 unit tests. |
| `session_store.rs` | `SessionStore` trait + `FsSessionStore` impl. Three-phase idempotent deletion (jsonl → uuid subdir → title cache). `NullSessionStore` for tests. Held as `Arc<dyn SessionStore>`. |
| `title_service.rs` | Async title generation. Semaphore caps concurrency at 6. Delivers `(uuid, title)` pairs via mpsc channel. `TitleHandle` aborts all in-flight tasks on drop. |
| `titles.rs` | Anthropic API call (Claude Haiku). Returns `None` if `ANTHROPIC_API_KEY` unset or call fails. |
| `ui.rs` | `render(frame, app, &theme)` — 28% projects / 55% sessions / 45% preview layout. Computes `ListState` fresh each frame. ~20 unit tests for formatting helpers. |

**Data flow:** `FsSessionStore::load()` → `App::new()` → title service spawns background tasks → TUI event loop calls `dispatch()` → `ui::render()` each frame.

**Key patterns:**
- `App::dispatch()` is the only mutation point — all actions funnel through it
- `ListState` is derived at render time (never stored stale)
- Title updates skip any session currently open in the edit buffer
- Disk deletion happens before in-memory removal (safe on partial failure)
- `ANTHROPIC_API_KEY` env var required for title generation; app degrades gracefully without it
- Theme resolution order: `--theme <name>` CLI flag > `theme` in config file > default (gruvbox-dark). `[colors]` overrides in config are applied on top of whichever theme is selected.
