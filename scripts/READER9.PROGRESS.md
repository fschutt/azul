# READER9 progress - AzReader (e-reader)

Branch: wt/reader9 (base e537ddbe2). Brief: scripts/waves/wave9/PLAN.md "READER9";
planning: ../azul-apps/planning/mobile/e-reader.md.

## DONE
- bb562b34f progress file; 6f58b087b decisions
- 8c4052cd7 crate skeleton + registration (root Cargo.toml, workspace_test_members, rust.yml)
- ced5220d2 / 330b24e1e xmltree.rs (parse_document: XML first, HTML fallback; helpers)
- 79ad10812 RED / 3409a9418 GREEN epub.rs (container, package, spine, cover, TOC nav/NCX/titles)
- 00e3a0196 RED / 909bba3bf GREEN position.rs (PageMap, Position, turn, progress)

## IN PROGRESS
- css.rs (book stylesheet filter) RED.

## NEXT
4. RED/GREEN: css filter, then content policy (chapter XHTML -> reading Xml tree).
6. RED (engine): Pdf::compute_pagination_with_policy - lines never torn; GREEN in dll.
7. Library + storage (keys, info.json, state.json), settings.
8. UI: library (BrowserShell-free: DocumentShell + covers grid), reader (DocumentShell, TOC / bookmarks pane, page view, status bar), settings section.
9. E2E script scripts/azreader_e2e.py, report.

## Decisions
- PAGES: the chapter is laid out ONCE as one continuous column at the page's text width; the
  engine's pagination (Pdf pagination -> break ys in the continuous canvas, "items are never
  shifted, only clipped" - paged_layout.rs) gives the page starts; each page on screen is a clip
  window [y_i, y_i+1) over the same column (`position: relative; top: -y_i` inside an
  `overflow: hidden` box of height y_i+1 - y_i). The same thing the engine's own slicer does.
- ENGINE GAP (root cause, fix in dll): `Pdf::compute_pagination` runs the DEFAULT BreakPolicy
  (all off = plain interval slicing), which tears a text line across two pages. Add
  `Pdf::compute_pagination_with_policy(.., policy: BreakPolicy)` in
  dll/src/desktop/extra/pdf/mod.rs (+ wasm stub); page_breaks.rs (owned elsewhere) untouched.
  The reader asks for atomic lines, widows/orphans, break-inside avoid, atomic rows.
- PARSER: an EPUB content document is XHTML = XML, so it is read with `Xml::from_str` first
  (`<a id="x"/>`, `<title/>`, `<div/>` are EMPTY elements there; the HTML tree construction
  would open them and swallow the rest of the chapter - `<title/>` as RCDATA eats the whole
  body); when the strict parse fails (a malformed book) it falls back to the HTML5-like parser
  `Xml::create_from_html`, which also reads `.html` items and plain HTML files.
- POSITION: (chapter index, fraction of the chapter's laid-out height at the page start).
  Survives a reflow (font size, window size) approximately; book progress weights chapters by
  their text length (characters), independent of layout.
- PAPER: the reading surface's colours are "paper" (Auto = follows the mode, White, Sepia,
  Night) - never "theme" (naming rule: theme = flat/flora, mode = light/dark).
- DATA: reader/books/<uuid>/book.<epub|txt|html> (the imported file), info.json (title,
  author, format, added), state.json (position, bookmarks, last read) - small frequent writes
  apart from the big file; reading settings in the appkit settings file.
- CSS: the book's sheets are kept, filtered: no @font-face / @import / url(); colours and
  backgrounds dropped (the paper decides), font-family dropped (monospace kept), absolute
  font sizes / line heights dropped (the reader's size decides), rem -> em, position fixed /
  absolute dropped.
- IMAGES: decoded on the chapter thread for their size; every <img> gets an explicit CSS size
  that fits the page (the same box in the pagination and on screen); registered in the image
  cache under `azreader:<book>/<path>` (the `<img src>` the content writes).

## Open questions
- (none)
