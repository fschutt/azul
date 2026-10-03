# CODE9 progress - AzCode + CodeView (wave 9)

Branch: wt/code9 (base e537ddbe2). Brief: scripts/waves/wave9/PLAN.md "CODE9", planning
../azul-apps/planning/core/code-editor.md. Never compile; RED first; commit after every unit.

## DONE
- progress file

## IN PROGRESS
- CodeView widget skeleton (layout/src/widgets/code_view.rs)

## NEXT (in order)
1. code_view.rs skeleton: types, callbacks, struct, builders, stub pure fns; mod.rs registration
2. RED code_view_tests.rs: line windowing, line pieces (spans / selection / caret / tabs / clip),
   key edits (typing, Enter indent, Backspace join, multi-cursor), moves, reveal
3. GREEN pure fns, then build + looks (flat / flora APPEND), then handlers, then manifest
4. examples/azul-code: buffer.rs (piece table) RED -> GREEN; highlight.rs (syntect, invalidation) RED -> GREEN;
   workspace / tabs / search / go to line / CloseGuard / storage / ui; registration; E2E script
5. report scripts/CODE9_2026_10_03.md

## Decisions
- D1 CodeView is GENERIC OVER ITS DATA like DataTable / CellGrid: the app answers `line -> CodeViewLine
  { text, spans }` through a data callback; edits come back as `CodeViewEdit`s (replace [start, end) with text,
  ordered last-first) plus the next `CodeViewView` in a `CodeViewEvent`. The app owns text and view. Why: FFI
  (no handle type wrapping a RefAny exists in api.json; a 40 MB buffer cannot be a value type cloned per
  event), and the house pattern (DataTable / CellGrid / RichTextEditor: the app owns the state).
- D2 Virtualised in WHOLE LINES (the scroll window of CellGrid / DataTable), not a VirtualView: only the lines
  in view are DOM, `view.top_line: u32` / `left_column` move, never a pixel offset. The f64 question: scroll
  offsets are still f32 (`LogicalPosition`), exact to 2^24 px = 16.7 M px (~880k lines at 19 px); a 1M-line
  file in pixel space (19 M px) steps in 2 px at its end. Line-indexed scrolling is exact for any length, so
  CodeView needs no f64. Also: no nested DOM (DATATABLE7 D1: events / focus / style inheritance stop at a
  VirtualView boundary), and the one-focus-stop key handling of DataTable is proven.
- D3 The text model is a PIECE TABLE (original + add buffer, piece list with cumulative newline counts, line
  starts of both buffers) in the app crate (`examples/azul-code/src/buffer.rs`, plain Rust, no azul): the
  CodeView data source's backing store. A second app that needs it moves it to azul-appkit (one move). Line
  endings: detected at load (LF / CRLF), normalised to LF inside, written back as found.
- D4 Highlighting: SYNTECT (not tree-sitter). Why: pure Rust with `regex-fancy` (fancy-regex is already in
  Cargo.lock), 50+ grammars in one crate, a LINE-ORIENTED parse state (`ParseState` + `ScopeStack`, Clone +
  Eq) that maps 1:1 onto line virtualisation: checkpoints every N lines, an edit at line L invalidates from
  L, re-highlighting stops where the state converges; the state is Send so a big jump is parsed on an azul
  Thread. Tree-sitter needs a C toolchain per grammar (mobile / wasm targets), a whole-document tree
  (memory on 1M lines) and churns its API; it is the later upgrade for folding / outline. The scopes map to
  CodeView's `CodeTokenKind`s, coloured by azul's themes (flat / flora, light / dark) - not syntect themes.
- D5 Caret drawn in the flow (a zero-width box with a 2 px bar) and selections as segment backgrounds, so they
  sit exactly on the glyphs; the char width (measured once with `measure_dom_shrink_to_fit` in a handler,
  kept in `view.char_width`) is only needed for pointer -> column.
- D6 Copy / cut taken in the key handler (DataTable's rule: no Copy event on a non-editing focus); paste
  through the Paste event (CellGrid's on_grid_paste), undo / redo reported to the app (its history).

## Open questions
- (none yet)
