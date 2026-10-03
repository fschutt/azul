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

- a02cb89dc AzCode crate registered (root Cargo.toml, workspace_test_members.txt, rust.yml dll_tests step),
  Cargo.toml (syntect 5.2 parsing + default-syntaxes + regex-fancy), main.rs, lib.rs stub (`start()` empty),
  buffer.rs RED; f9ee7aa18 buffer.rs GREEN (TextBuffer: from_text, line, slice, offset_of, pos_of,
  apply(&[Edit]) -> Vec<LineChange>, undo / redo -> Undone { caret, changes }, is_dirty, mark_saved,
  to_file_text)

- 442b5e237 highlight.rs RED, 676a7e72a highlight.rs GREEN (Highlighter: line_spans / edited / job_for /
  adopt, HighlightJob::run, classify; syntax_for(file, first line))

- 76ca185ef search.rs RED, f5e24d9d4 search.rs GREEN (item 1 below is DONE)

- 2e0140544 workspace.rs RED, 26ffdcb42 workspace.rs GREEN (item 2 DONE: Workspace { root: Root, rows(),
  toggle(folder, open) -> needs listing, set_listing(folder, folders, files), drive_key / relative_key },
  file_name, tab_label, Tabs<D: TabDoc> { open, close, find, active })

- f23ee25a6 sample.rs (MAIN_RS, LIB_RS, CARGO_TOML, README_MD, huge_rs(n), prefix() = "code/sample/",
  jobs() -> Vec<appkit FileJob::Put>); fa300f227 storage.rs (DriveJob List/Read/Write, run_jobs, drive_of,
  spawn_drive_jobs(info, &Root, jobs, reply_to, on_done) / take_drive_reply; spawn_highlight(info, doc, job,
  reply_to, on_done) / take_highlight_reply) - items 3 and 4 DONE

- 54cc2715b ids.rs; da8e023b8 app.rs (DocText, Doc { open, with_text, refresh, apply_edits, apply_own,
  undo_redo, caret_label, file_bytes }, doc_line data callback, token_kind, AppState { refresh_find,
  select_match, step_match, replace_current, replace_all, go_to, title, any_dirty }) + buffer depth() /
  mark_saved_at()

## IN PROGRESS
- NEXT STEP: src/ui.rs (DOM: activity bar, explorer TreeView (rows -> TreeViewNode, click index i ->
  rows[i-1]), search side panel, editor = TabHeader + find bar / go-to bar + CodeView (data_source =
  doc.text with app::doc_line, on_event = app with on_code_event) or ShellEmptyState, status bar; and the
  callbacks) then lib.rs start() / layout / on_window_created (highlight Timer 250 ms: docs with walk_to ->
  storage::spawn_highlight) / on_drive_done / on_highlight_done / on_files_done (sample written) /
  CloseGuard / keys; then scripts/azcode_e2e.py and the report.
  1. (DONE) src/search.rs: `find_all(&TextBuffer, needle, TextMatch) -> Vec<(line, start, end)>` (azul_appkit::find::
     matches per line), `next_after(matches, Pos) -> Option<..>` (wraps), `replace_all_edits(...) -> Vec<Edit>`
     (last-first) + `go_to_line(input "120" / "120:5", line_count) -> Option<Pos>`; tests.
  2. src/workspace.rs: `Root { drive_root, prefix, data_tree, name }`, listings per folder key (lazy, from
     `LocalDrive::without_manifest(root).list(ListRequest::folder(prefix))`), `expanded`, `tree_rows()` in the
     TreeView's depth-first order (index -> key), `Doc { key, name, text: RefAny(DocText{buffer, highlighter}),
     view: CodeViewView }`, tabs (open / activate / close, dirty marks "name *"), recent workspaces; tests.
  3. src/sample.rs: sample files (src/main.rs, src/lib.rs, Cargo.toml, README.md, huge.rs = 100,000 generated
     lines) written under the data tree `code/sample/` through the Drive (appkit spawn_file_jobs) on --sample.
  4. src/storage.rs: azul Thread jobs on a drive at the workspace root (list a folder, read a file, write a
     file; `LocalDrive::without_manifest` for a user folder, `LocalDrive::new(data_root)` for the sample) ->
     write-back `on_files_done`; the highlight job thread (HighlightJob::run) -> `on_highlight_done`.
  5. src/ids.rs (`__azcode_` const AzStrings), src/ui.rs + lib.rs: DeveloperShell (activity bar buttons:
     Explorer / Search; side bar: TreeView explorer or the search panel (TextInput find + replace, Aa / whole
     word toggles, results list); editor: TabHeader (names, "*" when dirty) + find bar + CodeView (data source
     = DocText RefAny: buffer.line(i) + highlighter spans -> CodeViewSpanVec; on_event applies edits to the
     buffer, highlighter.edited(change) for each LineChange, undo/redo via buffer.undo() -> view.set_cursor)
     or ShellEmptyState; status bar "Ln x, Col y | Spaces: 4 | UTF-8 | LF/CRLF | <language>"), Go-to-line
     modal (TextInput), CloseGuard (dirty = any doc dirty; Save saves all then closes), keys: Mod+S save,
     Mod+F find, Mod+G go to line, Mod+W close tab, Mod+P? (no), Escape closes bars. TODO(WIDGETS9A): Toolbar
     for the find bar; IconGrid not needed.
  6. scripts/azcode_e2e.py (model on scripts/azdashboard_e2e.py / azwriter_e2e.py), then the report.
- (done) highlight.rs design, kept for review:
  - `static SYNTAXES: OnceLock<SyntaxSet>` = `SyntaxSet::load_defaults_newlines()` (parse `line + "\n"`);
    `syntax_for(file_name, first_line) -> &'static SyntaxReference` (by extension, then first line, else
    plain text).
  - `LineState { parse: ParseState, scopes: ScopeStack }` (Clone + Eq).
  - `Highlighter { syntax: &'static SyntaxReference, checkpoints: Vec<(usize, LineState)>` sorted, line 0
    always; `stale: Vec<(usize, LineState)>` (checkpoints after an edit, lines shifted by the delta; adopted
    when a parse reaches one with an EQUAL state - the convergence), `cache: BTreeMap<usize, (Vec<CodeSpan>,
    LineState /*after the line*/)>` capped ~4000, `lines_parsed: usize` counter for tests, `generation: u64`
    bumped per edit }`.
  - `line_spans(line, &dyn Fn(usize)->String) -> Option<Vec<CodeSpan>>`: cached -> Some; start = max(cached
    line below with its end state, checkpoint <= line); `line - start > SYNC_LIMIT (1500)` -> None (the app
    starts the job); else parse forward caching, appending a checkpoint every EVERY (64) lines past the last.
  - `edited(LineChange)`: keep checkpoints <= first; those > first + removed -> stale shifted by
    added - removed; cache >= first dropped; generation += 1.
  - `HighlightJob { syntax, start: (usize, LineState), lines: Vec<String>, every, generation }` ->
    `run() -> Vec<(usize, LineState)>`, `Highlighter::adopt(job_result, generation)`.
  - scope -> CodeTokenKind: any `comment*` in the stack -> Comment, any `string*` -> StringLiteral, else the
    innermost scope matched against prefixes (constant.numeric -> Number, constant.character -> String,
    constant -> Constant, keyword.operator -> Operator, keyword / storage -> Keyword, entity.name.function /
    support.function / variable.function -> Function, entity.name.type|struct|enum|class / support.type /
    support.class -> Type, entity.name.tag -> Tag, entity.other.attribute-name / meta.annotation /
    meta.attribute -> Attribute, support.macro / entity.name.macro -> Macro, markup.heading /
    entity.name.section -> Heading, markup.underline.link -> Link, invalid -> Invalid, variable -> Variable,
    punctuation -> Punctuation). `CodeSpan { start: u32, end: u32, kind: TokenKind }` app-side, mapped to
    azul's CodeViewSpan in the UI.
  - tests: keyword / function / string spans of a Rust line; a block comment carries to the next lines;
    an edit invalidates from its line (cache below kept); opening a comment recolours the lines after it;
    a far line returns None, the job's checkpoints are adopted, then Some; after an edit that keeps the
    state, re-highlighting line 300 parses < 80 lines (convergence).
- then: lib.rs app (see NEXT 4).

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
