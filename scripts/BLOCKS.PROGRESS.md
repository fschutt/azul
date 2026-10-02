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
- 8a3857223 progress file
- 33dde6146 U64Vec in css/src/corety.rs
- b8e059b96 RED ListSelection tests (layout/src/widgets/list_selection.rs, registered in widgets/mod.rs)
- c2a336835 GREEN ListSelection
- 5c9a968a5 MessageListSelection removed; MessageList docs/test, AzMail, showcase use ListSelection
- 567c5e175 ListSelection keyed orders take an owned U64Vec (FFI-friendly)
- d552f5523 AzShow rail + canvas on ListSelection
- 893610c1c AzTasks on ListSelection (key_of(task id))
- 52d7636d2 AzDrive Selection = thin adapter over ListSelection
- a06621bbc RED appkit history.rs (UndoHistory) ; a2567bbba GREEN
- 7369b65ff AzPhoto on UndoHistory (raster/history.rs removed) ; 724b9a147 AzVideoCut ; fd3623512 AzShow
- a94735330 RED pane modules wrap ; f30151254 GREEN ; 24a191049 ModuleSwitcher removed, showcase on the pane
- f3b516785 RED CloseGuard + prevent_window_close ; b994e2617 GREEN
- db3bba2bc Document+Pim ; 377d85810 Browser/Developer/Media/Records/Timeline ; 28806100b Canvas+Call office_shell()
- 15a0ef180 shells fixtures ; bca3dddf0 apps set chrome after office_shell()
- 5dbba28b8 / 6bd07653b / b006cda93 report scripts/BLOCKS_2026_10_02.md

## IN PROGRESS

## NEXT
- Nothing: all five items and the report are committed. The parent: autofix api.json (list in the
  report), regenerate css/src/codegen/lower_types.rs, build, run the report's test commands.

## Open questions for the user
- `FullWindowState::close_callback` is never invoked by any backend: remove it or wire it?
- MessageList rename (waits for the user, untouched).
