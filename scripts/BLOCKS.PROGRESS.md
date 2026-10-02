# BLOCKS progress (wave 5, 2026-10-02) - branch wt/blocks from 2e92c759b

Task brief: scratchpad wave5/BLOCKS.md; house rules wave5_common.md. Findings: DEDUP_OFFICE D10, D20, N14,
A16; DEDUP_WIDGETS_API F2, F6, F29.

## Decisions (unattended)
- D1 selection model: `ListSelection` in `layout/src/widgets/list_selection.rs` (exported, module widgets),
  a SORTED SET of u64 keys + anchor + focus + `keep_one`. Keys are u64 (MessageRow / ToDoBar / Timeline ids
  are u64): a positional list uses the row index as the key (range = numeric anchor..=key, `count` for
  select-all / step); a keyed list passes its visible `order` (`*_in` methods). String-id apps (Drive, Tasks)
  key by `ListSelection::key_of(String)` (the same hasher `NodeData::set_key` uses). Needs a new primitive
  `U64Vec` in `css/src/corety.rs` (next to U32Vec). `MessageListSelection` goes (replaced, not aliased).
- D2 undo: `UndoHistory<T>` in `examples/azul-appkit/src/history.rs` (Rust generic, no FFI - a snapshot of
  an app type cannot cross the FFI): app owns the current state, `checkpoint(label, before)` before an edit,
  `undo(&mut current)` / `redo` / `jump` swap, labels for menus and a History panel, coalescing key + `seal`,
  VecDeque with a limit. Photo / VideoCut / Show depend on azul-appkit (no `azul` feature).
- D3 switcher: the pane's switcher survives and wraps at the ends (APG tabs); `ModuleSwitcher` /
  `SwitcherModule` GO (file, theme sections, gallery entries, showcase -> ShellNavigationPane).
- D4 presets: chrome (title row, ribbon, backstage, status bar, F6 / splitter hooks, theme) lives on
  OfficeShell; presets with `office_shell()` drop their copies; callers write `.office_shell().with_x(..)`.
- D5 close guard: `CallbackInfo::prevent_window_close()` + `CloseGuard` widget (`widgets/close_guard.rs`):
  wraps the window content, vetoes CloseRequested while `dirty`, asks with MessageBox (Question:
  Save / Don't Save / Cancel) in a Modal while `asking`, one `on_event` (Asked / Save / Discard / Cancel).

## DONE (commit hashes)

## IN PROGRESS

## NEXT
- A1 U64Vec in css corety.rs; A2 RED list_selection.rs; A3 GREEN; A4 MessageListSelection -> ListSelection
  (AzMail, showcase); A5 AzShow; A6 AzTasks; A7 AzDrive.
- B undo (appkit history.rs RED/GREEN, Photo, VideoCut, Show).
- C switcher; D close guard; E presets; F report scripts/BLOCKS_2026_10_02.md.

## Open questions for the user
