# NOTES progress (AzNotes)

Branch `wt/notes` from `39092feee`. Worktree
`/Users/fschutt/Development/azul/.claude/worktrees/agent-a15ab00b973aabdc4`. Report:
`scripts/NOTES_2026_10_01.md`. Nothing is compiled here (house rule); Rust files are
parse-checked with `rustfmt --edition 2021 --check`.

## DONE
- `149a2cf36` RED / `853792345` GREEN E1: `compute_counters` skipped counter-reset in any
  pseudo-state (a hovered `ol` renumbered its items) - unit test in `solver3/cache.rs`.
- `546d645eb` RED / `9d6a3e80d` GREEN E2: `MessageListMark` (Flag / Pin / None) on MessageList.
- `6271b43ca` azul-storage `time::iso8601` (+ test).
- examples/azul-notes/src/{doc,markdown,model}.rs: the flat block document with its edits,
  front matter + Markdown both ways, the library / queries / keys (unit tests inside).

- store.rs: the storage jobs (Load, Rescan, Save with move + version, Delete, PutText,
  History, Version, Import, Images, Seed) over `&dyn Drive`, tests on a LocalDrive in a temp
  folder; Note gained `generation` + `file_modified`.

## IN PROGRESS (precise next steps, in order)
1. Cargo.toml + .cargo/config.toml + main.rs + args.rs (flags: --sample, --data, --screen,
   --theme, --mode, --size).
2. sample.rs (the sample library as (key, text) files).
3. editor.rs: Doc -> DOM of the contenteditable host (one child per block, one per run; check
   box as an abspos contenteditable=false island AFTER the runs), the node -> block mapping,
   and the pure edit-application helpers.
4. lib.rs in pieces: AppState + start; threads/write-back; layout (PimShell: nav pane, message
   list, editor pane with title/tags/toolbar, status bar); callbacks (editor sync, structural
   edits, keys, toolbar, list, nav, palette, settings, history, export, autosave timer, close).

## NEXT
- registration (workspace, test members, CI), scripts/aznotes_e2e.py, the report.

## Decisions (made unattended, for the report)
- D1 The editor is app-local for this pass (`examples/azul-notes/src/editor.rs` +
  `doc.rs`), written so it can be promoted to `azul::widgets::RichTextEditor`. Reason: no
  compiler tonight, and the widget's api surface (blocks, runs, events, the sync glue) needs a
  design pass with the parent. AzWriter's `ir.rs` is the twin (named in the report).
- D2 The editor model is a FLAT block list (paragraph, heading, bullet/numbered/check item with
  an indent level, quote, code block, rule, image), one DOM child of the contenteditable host
  per block; list items are `li` (display: list-item, the engine's `::marker`) directly under
  the host; a numbered item's number is set with `counter-reset: list-item <n-1>` on the item;
  a check item's box is an absolutely positioned `contenteditable=false` island (out of the
  IFC, walled off from the text the engine edits), so the item's runs start at DOM child 1.
- D3 The note list is azul's `MessageList` with a new row MARK (`MessageListMark::Pin`): the
  same widget AzMail uses, so search row, sort header, sections, keyboard and virtualisation
  are not duplicated.
- D4 Typing: TextChanged -> sync the block text into the model (prefix/suffix diff keeps the
  runs), ack the revision, no rebuild. Enter / Backspace merges: DocumentEdit -> apply the
  split / merge to the model, ack, RefreshDom. Markdown shortcuts, toolbar block kinds and
  inline formats: change the model, ack, RefreshDom (the new generation's text wins:
  `gc_app_set_text` / the acked GC). Switching notes / restoring a version / an external
  edit: `reset_editor_content`.
- D5 Dates: `azul_storage::time` (one place for ISO 8601; a formatter is added there) for the
  front matter, chrono `Local` for the list's dates (as AzDrive / AzMail).
- D6 Empty notebooks persist as a marker object `notes/<notebook>/.notebook` (S3 has no
  folders).

## Open questions
- (none yet)
