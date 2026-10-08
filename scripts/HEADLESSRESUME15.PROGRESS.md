# HEADLESSRESUME15 progress (a resume's RefreshDom lost in the headless pump)

Worktree `.claude/worktrees/agent-add396cf81aa202c7` (branch worktree-agent-add396cf81aa202c7,
fast-forwarded to fix/input-bugs-2026-09-19 @ b46726f3d). Nothing compiled or run here.

## Root cause (found by reading; the lead's AzCalendar run matches it step by step)
- The request queue is PROCESS-wide (`layout/src/request.rs`): a deferred completion (every HTTP
  request: `layout/src/http.rs` `resume_without_blocking` -> `request::defer`) is delivered by
  WHICHEVER window's pass calls `take_completed()` first after the answer arrived.
- AzCalendar's Save (a11y press via the debug server, in the ROOT) closes the draft popover. The
  root's frame drops the `<transient-window>` node -> `sync_parent` marks the popup's mailbox
  closed -> `pump_children` -> popup `pump_once` Phase 0 -> `regenerate_layout` ->
  `poll_transient_mailbox` (event.rs ~5347) -> `request_window_close` -> `process_window_events(0)`
  -> `invoke_completed_requests` (event.rs ~11457): the POST answer (tens of ms after the root's
  Phase-2 poll) is delivered HERE, in the closing popup's pass.
- Its `Update::RefreshDom` became a regeneration of the POPUP only (event.rs 11459-11466), the
  pass result is dropped (`let _ = self.request_window_close(..)` in `poll_transient_mailbox`),
  the popup closes: the root (which shows the event) is never asked to rebuild.
- Secondary (suspects 1/3): headless has no frame gate - `pump_once` services a frame only when a
  phase REPORTS work; Phase 1b's `request_redraw()` (and any request raised outside a reporting
  phase) sits until an unrelated event. Every desktop loop has a `regeneration_pending()` gate.

## DONE
- 2262aeb21 RED test A: headless/tests/request_resumes.rs
  `a_request_answered_while_its_popover_closes_rebuilds_the_window_that_shows_the_answer`
- 3312a6cea fix 1: `invoke_completed_requests` raises a resume's rebuild for this window AND
  every other one (event.rs); request.rs module doc corrected (doc only)
- c79b29f4d RED test B
  `a_screen_readers_press_that_asks_for_a_rebuild_is_shown_after_one_turn_of_the_loop`
- 1d40c53c1 fix 2: headless `pump_once` Phase 2c frame gate (`regeneration_pending()` /
  `resize_relayout_pending()`, open windows only); `run()` / `start_as_child()` retire the
  born-with request with the initial layout (epoch + `clear_regeneration_unless_reraised`)
- test fn renamed `registered` -> `on_registered` (no shadowing), with this file's last update

## IN PROGRESS
- nothing: report handed to the lead

## Open questions / notes
- Other backends: fix 1 is shared trait code, so the cross-window loss (a popup's close pass or
  any secondary window delivering another window's resume) is fixed on macOS / Windows / X11 /
  Wayland too; mobile is single-window (default no-op fan-out). The frame-gate gap is
  headless-only (every desktop loop already gates on `regeneration_pending()`).
- Not done (latency only): a child's all-windows bump does not wake the root's condvar in
  `run()`; the root rebuilds at its next wake (a timer, the debug poll, the next request) -
  Phase 0 runs before the debug timer answers, so the next `get_node_hierarchy` is fresh.

## For the lead (build / run)
- `cargo test --release -p azul-dll --lib request_resumes` (default features carry a11y +
  debug-server; `--features build-dll` works too). Both tests RED on b46726f3d, GREEN with the fixes.
- RED pass: reverse-apply fix 1 (3312a6cea) -> test A red; reverse-apply fix 2 -> test B red.
- Then the suites the gate touches: dll `--lib --features build-dll` (headless e2e_host / idle_cpu /
  child_window_tests) and `--test close_requested_headless`.
