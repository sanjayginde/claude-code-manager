# Architecture Improvement Backlog

Candidates identified during the `/improve-codebase-architecture` session (2026-04-08).
Candidates 1 (Session Lifecycle) and 2 (SessionStore) are done — see PRs #25 and #27.
Candidate 3 (Data Loading Pipeline) is done — see PR #28.
Candidate 4 (Title Service) is done — see PR #30.
Candidate 5 (TUI Event Loop) is done — see PR #32.

---

## Candidate 3 — Data Loading Pipeline (`data.rs`)

**Cluster:** `load_projects_from` in `data.rs`

**Problem:** A single function conflates three concerns — filesystem traversal
(`read_dir`), JSONL header parsing, and display label derivation (`last_two`,
`decode_label`). Parse errors are silently swallowed via
`parse_header().unwrap_or_default()`. A session with a missing `cwd` field
returns an empty path, which causes `main.rs` to silently resume from the
current directory instead of the original project directory.

**Dependency category:** Mixed concerns at the same abstraction level — I/O
and business logic interleaved in one function.

**Key bugs enabled by current structure:**
- Sessions with failed parses show `?` for branch and empty message preview
  with no signal to the user that something went wrong
- Corrupted or empty `.title` cache files are silently accepted as titles
- The 150-line parse limit is hardcoded with no validation against file size

**Test impact:** Currently no tests for malformed/truncated JSONL, missing
`cwd`, or corrupted title cache. The fix would allow `FsSessionStore` tests
with a `tempdir` to cover these failure paths.

**Suggested approach:** Split `load_projects_from` into pure sub-functions with
explicit `Result` types rather than `unwrap_or_default`. Surface parse failures
as `Option` with a distinct `ParseError` variant so the UI can show a degraded
state rather than silently dropping data.

---

## Candidate 4 — Title Service (`title_service.rs`, `titles.rs`)

**Cluster:** `AnthropicTitleService` + `titles::generate_title`

**Problem:** The service spawns one unbounded `tokio::spawn` task per untitled
session with no rate limiting, backpressure, or cancellation handle. Each task
does a blocking `store.save_title()` call (wraps `std::fs::write`) inside an
async context — technically correct since `SessionStore` is sync, but it blocks
a Tokio worker thread. If 50 sessions need titles, 50 tasks are spawned
simultaneously.

**Dependency category:** Cross-boundary side effects — async tasks mutate disk
state independently of the app's in-memory lifecycle.

**Key bugs enabled by current structure:**
- No way to cancel in-flight title tasks when the user quits (tasks outlive the
  TUI on slow API responses)
- `save_title` errors are silently discarded (`let _ = store.save_title(...)`)
  with no retry or user notification
- Out-of-order title arrivals (slow task A completes after fast task B) are
  possible but harmless today — could matter if ordering is ever relied upon

**Test impact:** `NoopTitleService` is used in all tests, so the real
`AnthropicTitleService` path (spawn + save + send) has zero test coverage.

**Suggested approach:** Replace unbounded spawning with a bounded semaphore
(e.g. `tokio::sync::Semaphore` with limit 4-8). Return a `JoinSet` or
`AbortHandle` from `start()` so `main` can cancel on quit. Use
`tokio::task::spawn_blocking` for the `save_title` call to be explicit about
the blocking boundary.

---

## Candidate 5 — TUI Event Loop (`main.rs`)

**Cluster:** `tui_loop` in `main.rs`

**Problem:** Modal state (normal / edit-title / delete-confirm) is implemented
as nested `if` branches in the event loop rather than as part of the `App`
state machine. Adding any new modal (e.g. a rename-project dialog) requires
touching both `main.rs` (key routing) and `app.rs` (state + dispatch) in
lockstep, with no type-level enforcement that the two stay in sync.

**Dependency category:** Shallow orchestration — `main.rs` encodes input policy
that belongs in `app.rs`.

**Key bugs enabled by current structure:**
- Key routing logic is completely untested; there is no way to assert that `e`
  is blocked during delete confirmation or that nav keys are blocked during edit
  mode without running the full TUI
- A future third modal introduced in only one file would silently misbehave

**Suggested approach:** Add a `modal(&self) -> Modal` accessor to `App` that
returns an enum (`Modal::None | Modal::EditTitle | Modal::ConfirmDelete`).
`tui_loop` dispatches based solely on `app.modal()` with no knowledge of
individual fields. Key routing tests can then be written against `App` directly
without spinning up a terminal.
