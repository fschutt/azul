# RTE progress (wave 5, 2026-10-02) - shared rich-text editor

Branch `wt/rte` from `2e92c759b`. Worktree `.claude/worktrees/agent-ac6805ca5f7cbc4a1`.

## DONE
- progress file

## IN PROGRESS
- step 1: dependency (pulldown-cmark optional in azul-layout) + module skeleton

## NEXT (in order; commit after every unit)
1. layout/Cargo.toml: `pulldown-cmark` optional, feature `rich_text_markdown` (in `widgets`); justification lines.
2. layout/src/widgets/rich_text/{mod,doc}.rs: FFI model types + run helpers + edits (from AzNotes doc.rs) + tests.
3. rich_text/markdown.rs (AzNotes reader/writer, quote_depth, tables, align as HTML? no) + tests.
4. rich_text/html.rs: HTML + plain-text writers (AzMail compose.rs, generalized), HTML reader + tests.
5. rich_text/history.rs: ONE snapshot undo stack + tests.
6. layout/tests/a_rich_text_editor_*.rs RED tests (4 bugs) + all.rs registration.
7. rich_text_editor.rs widget: state, callbacks, dom, sync/structural/keys/toolbar/commands; themes flat/flora; manifest.
8. AzNotes adoption. 9. AzMail compose adoption. 10. report.

## Decisions (2026-10-02)
- D1 Model = FFI types (repr(C)) directly (RichTextDoc/RichBlock/RichRun/RichBlockKind...), no private twin;
  edits convert a block's RichRunVec to Vec<RichRun> and back.
- D2 Quote is NOT a block kind: every block has `quote_depth: u8` (Mail's nested quotes); Notes' `> ` shortcut and
  Quote button set depth 0<->1. Markdown writes `> ` per level.
- D3 Writer features in the model: `align: RichAlign`, `RichBlockKind::PageBreak`, `RichBlockKind::Table(RichTable)`
  (plain-string cells; a cell's text syncs by path [block,row,cell]; Enter/Backspace/Delete at a cell edge vetoed).
- D4 Markdown READER needs a CommonMark parser: `pulldown-cmark 0.9` (already in Cargo.lock via AzNotes/AzWriter,
  vetted in supply-chain/config.toml + build-script-policy.toml) becomes an OPTIONAL dependency of azul-layout behind
  feature `rich_text_markdown`, enabled by `widgets`. Two lines added to scripts/dependency-justifications.toml
  (pulldown-cmark, unicase). Parent: if rejected, `from_markdown` is cfg'd out and AzNotes keeps its reader.
- D5 ONE undo history: the widget's snapshot stack (`RichTextHistory`) covers typing (coalesced), structural edits,
  formats, kinds; undo/redo restore a snapshot and call `reset_editor_content` (which clears the engine's per-host
  text stacks AND its structural stack). Structural edits are acked WITHOUT an inverse (no engine structural
  history). Engine gap (report): Ctrl/Cmd+Z is AddAndSkip in core/src/events.rs (never reaches app callbacks), so the
  keyboard undo is still the engine's text undo between resets; wave 6: let a host that owns its history receive
  Undo/Redo.
- D6 State ownership = TextInput pattern: the app holds `RichTextEditorState` (doc + history + typing style + caret
  block + host id); the widget clones it into ONE shared RefAny all its callbacks use; every change fires
  `on_change(data, info, RichTextEditorState)`; app commands (ribbons) call `RichTextEditorState::apply_command(info,
  RichTextCommand)` on the app's copy.
- D7 Content styles use system colours (follow the mode); only the frame + toolbar are themed (flat/flora via
  `follow_app_theme` on a small chrome struct; the content host is built once, not twice).
- D8 Typing style (Ctrl+B at a caret): mirrored widget-side like AzNotes (`RichTypingStyle`) until TEXTENG's
  format-carrying edit report exists (named in report).
- D9 Built-in toolbar optional (default off); pressed = ButtonType::Primary until APIEXPORT's Button toggled state.

## Open questions
- (none blocking)
