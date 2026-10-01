# TASKS progress (AzTasks: the to-do / reminders app, build ledger A5)

Branch `wt/tasks` from `16d19442c`. Worktree
`/Users/fschutt/Development/azul/.claude/worktrees/agent-aede1413f7046aa6a`. Brief: scratchpad
`TASKS_go.md`, rules `wave4_common.md`. Nothing is compiled here (house rule); Rust files are
parse-checked with `rustfmt --edition 2021 --check`. Report: `scripts/TASKS_2026_10_01.md`.

## DONE (commit hashes)

- `8a1582114` progress file
- `7f1da5f98` model RED (crate scaffolding, registration: workspace member, test members, CI step)
- `446a1411e` model GREEN (recur, model, parse, views, reminders, store, sample, args)

## IN PROGRESS

- App UI on PimShell, one module per file, each committed when written:
  `state.rs` (the app state + mutations) -> `jobs.rs` (Thread jobs, the write queue pump)
  -> `nav.rs` (ShellNavigationPane) -> `list.rs` (quick add, chips, sections, rows)
  -> `detail.rs` (detail pane, list settings) -> `chrome.rs` (title, ribbon, backstage with
  ShellSettingsLayout / shortcuts / about, status bar, To-Do bar, palette) -> `lib.rs`
  (start, layout, window keys, reminder timer, notifications).
- Text fields: every field stores its text on each keystroke (no rebuild) and is built
  with it; Enter / commit acks the window's text revision (`mark_text_revision_synced`) so
  a cleared field rebuilds empty (scripts/G2_FORM_FOLLOWUPS_2026_09_29.md, "What is left").
  The quick-add line rebuilds only when its recognised parts change (the chips).

## NEXT (in order)

1. (done) Model RED then GREEN.
2. App UI on PimShell (`lib.rs`): navigation pane (smart lists, my lists tree, tags), task list
   (quick add + parse chips, sections, rows, selection, drag reorder), detail pane, ribbon,
   backstage (Settings on ShellSettingsLayout, Shortcuts, About), To-Do bar, status bar with
   sync, reminders (timer + InfoBar + OS notification), command palette, keyboard shortcuts.
3. Registration (workspace, test members, CI step), `scripts/aztasks_e2e.py`.
4. Engine gaps found on the way: RED then GREEN in azul.
5. Report `scripts/TASKS_2026_10_01.md`.

## Decisions (unattended run)

- D1 Shell: `PimShell` (S4): navigation | task list | detail (reading slot) + To-Do bar.
- D2 Smart-list counts in the navigation tree use `TreeViewNode::with_badge` - added by MAIL2
  (`wt/mail2`, `layout/src/widgets/tree_view.rs`) tonight for the mail folders' unread counts;
  not duplicated here. AzTasks needs MAIL2 merged first (named in the report).
- D3 Data root = `<user data dir>/Azlin` (override `AZTASKS_DATA` / `--data`), a `LocalDrive` on
  it; files `tasks/<list-uuid>/list.json`, `tasks/<list-uuid>/<task-uuid>.json`, attachments
  `tasks/<list-uuid>/<task-uuid>/<file>`, settings `tasks/settings.json`.
- D4 The TaskRow, QuickAddParser, RecurrenceEditor and SmartListRules are app-local, as
  `planning/core/todo.md` section 5 says (promote RecurrenceEditor with AzCalendar's later).

## Open questions

- (none yet)
