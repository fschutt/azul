# CODE9 progress - AzCode + CodeView (wave 9)

Branch: wt/code9 (base e537ddbe2). Brief: scripts/waves/wave9/PLAN.md "CODE9", planning
../azul-apps/planning/core/code-editor.md. Never compile; RED first; commit after every unit.

## DONE
- 8876a258e progress file + decisions
- 68bc1d474 RED: layout/src/widgets/code_view.rs skeleton (types, callbacks, CodeView + builders, pure fns
  as `todo!("GREEN: ...")` stubs, fixtures, class-name consts) + layout/src/widgets/code_view_tests.rs
  (31 tests) + `pub mod code_view;` in widgets/mod.rs

- GREEN code_view.rs (no todo!() left): d72494c31 columns, 910520ddc window + pieces, 448d88861 edit
  engine, ae70e2b8d keys, af901ab31 pointer, fe48c679d build + flat / flora looks + handlers + manifest

## IN PROGRESS
- AzCode app: examples/azul-code (see NEXT 4). Next file to write: examples/azul-code/Cargo.toml, then
  src/buffer.rs RED tests.

## (done) design notes of the CodeView GREEN, kept for review:
  Design notes for each (so a resumed agent need not re-derive them):
  - columns: visual_column/byte_at_visual (tie in a tab goes left), clamp_to_char, next/prev_char,
    word_left/right (blank, word = alnum|_, punct runs), word_at, first_non_blank, expand_tabs.
  - geometry: fit = view.visible_lines if >0 else floor(viewport_h/lh); rows = ceil(viewport_h/lh)
    min(count-top); top = min(view.top, count-1); gutter = max(3,digits(count))*cw + 2*GUTTER_PAD;
    text_left = gutter + TEXT_PAD; vbar when count > fit: track (w-12,0,12,h), thumb len
    max(24, h*fit/(count-1+fit)), start = (h-len)*top/(count-1).
  - change a `reveal` signature to (view, lines, tab, fit_lines, fit_columns) (no test calls it).
  - apply_changes: sort by start, drop overlapping / same-start; AfterInsert = caret at the new end of
    the cursor's last change (one pass `new_ends`), Carry = map anchor/head (`map_position`: a position
    at an insertion point goes after it, inside a replaced range to its new end); dedup heads keeping
    the later; edits reversed.
  - key_event: primary branch first (A select all, C copy, X cut (read-only: copy), Z/Shift+Z/Y
    undo/redo (read-only: None), D next occurrence (whole-word when the needle is all word chars, wraps,
    None when all taken), Home/End = text ends), then arrows/Home/End/Page (Mods.line = mac Cmd),
    Back/Delete (word / line), Return (indent + one unit after `{([`), Tab (multi-line selection or
    Shift: indent/outdent lines, Carry; else spaces to the next stop), Escape. Page = fit-1, moves top too.
  - pointer: hit_test (scroll bar first, gutter x<gutter_width, row = floor(y/lh), col =
    round((x-text_left)/cw)+left), press_event (Thumb: drag ScrollBar + start px/line; Track: page;
    Gutter: line + break; Text: set / Shift extend / Alt add; drag Select), drag_event (ScrollBar:
    top = start + (y-start_px)/(h-thumb)*max_top; Select: edge auto-scroll, move head),
    double_click_event (word_at), scroll_event (clamp, None when unchanged vs geo.top / left).
  - resolve (clamp_view, geometry, ask the lines once), dom() (flat / flora / follow_app_theme),
    build(resolved, &CodeViewLook) + CodeViewLook + bases (monospace SystemFontType::Monospace),
    then handlers (DataTable pattern: CodeViewShared{cv,geo}, shared_of/store_view/fire/deliver,
    `measured()` = char width via info.measure_dom_shrink_to_fit of 64 '0's + visible size via
    info.get_node_size(info.get_hit_node()); on_key (leave Ctrl/Cmd+V to the Paste event; Copy / cut ->
    info.set_clipboard_content), on_text, on_paste, on_mouse_down (set_focus + capture_pointer),
    on_mouse_move, on_mouse_up (drag None + release_pointer_capture), on_double_click, on_wheel
    (cell_grid::take_wheel, prevent_default + stop_propagation)).

## NEXT (in order)
3. themes: APPEND `// ==== code_view ====` to themes/flat.rs and flora.rs: `code_view_look()` +
   `code_view(resolved) -> Dom` (flora = the ink panel --fl-code-bg in both modes, per the planning doc);
   manifest in widgets/mod.rs: every_widget_dom push ("code_view", fixtures::sample().dom()), CHROME
   group, wheel takers list (after "data_table")
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
