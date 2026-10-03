# READER9 progress - AzReader (e-reader)

Branch: wt/reader9 (base e537ddbe2). Brief: scripts/waves/wave9/PLAN.md "READER9";
planning: ../azul-apps/planning/mobile/e-reader.md. Crate: examples/azul-reader (package
AzReader, lib azreader). Never compile; rustfmt --edition 2021 <file> as the parse check.

## DONE
- bb562b34f progress file; 6f58b087b decisions
- 8c4052cd7 crate skeleton + registration (root Cargo.toml, workspace_test_members, rust.yml)
- ced5220d2 / 330b24e1e src/xmltree.rs (parse_document: XML first, HTML fallback; helpers)
- 79ad10812 RED / 3409a9418 GREEN src/epub.rs (Container, package, spine, cover, TOC nav/NCX/titles)
- 00e3a0196 RED / 909bba3bf GREEN src/position.rs (PageMap, Position, turn, progress)
- 19897d743 RED / d26067a4d GREEN examples/azul-appkit/src/css.rs (shared sheet reader:
  strip_comments, block_body, for_each_item, parse_declarations, safe_value) - twin of
  AzMail html.rs sanitize_style_sheet/block_body/parse_style/safe_style_value (MAIL9 owns
  AzMail this wave: switch left for later, say so in the report)
- d3f521c08 RED / e8dd94ced GREEN src/bookcss.rs (fit_sheet / fit_inline / fit_declaration)
- 90c2c22eb RED / 0c560ea11 GREEN src/content.rs (read_chapter -> Chapter{xml, images,
  anchors, text}, fit_image, image_src "azreader:<book>/<path>", BASE_SHEET)

## IN PROGRESS
- (nothing half-done; every file above is committed)

## NEXT (exact)
1-2. DONE: fbd7a91d4 RED (src/paginate.rs test a_page_of_text_never_ends_inside_a_line +
   reading_policy + page_map) / d4d6798fc GREEN (dll: Pdf::compute_pagination_with_policy,
   engine::styled_dom_pagination_with_policy replaces styled_dom_pagination; wasm stub).
   api.json entry for the report: Pdf.compute_pagination_with_policy(self ref, styled_dom
   StyledDom, page_width_px f32, page_height_px f32, font_cache FontCacheSnapshot,
   image_cache ImageCacheSnapshot, policy BreakPolicy) -> PaginationSnapshot, fn_body
   `object.compute_pagination_with_policy(styled_dom, page_width_px, page_height_px,
   &font_cache, &image_cache, policy)`.
3. src/paginate.rs (append below page_map): reading column Dom (root css from settings: width, font-family, font-size,
   line-height, text-align) + chapter Dom (Dom::create_from_parsed_xml(chapter.xml)), the
   chapter thread (read_chapter + decode pictures with RawImage::decode_image_bytes_any,
   thumbnail to <= 2x page, then pagination) -> write-back {book, chapter, generation, Chapter,
   PageMap, images: Vec<(src, ImageRef)>}.
4. DONE 7f66dfcff src/plainbook.rs (and 96cda63f5 src/settings.rs = step 6): .txt / .html files as a Container + Book (split text at CHAPTER headings
   or every ~60 KB; TOC from headings).
5. src/library.rs + src/storage.rs: keys reader/books/<uuid>/{book.<ext>, info.json,
   state.json, cover.png}; BookInfo, BookState (position, bookmarks, last_read, finished);
   the library scan thread (info + state + cover decode); the import thread.
6. src/settings.rs: ReadingSettings (font_px, line_height, margin_px, font, paper Auto/White/
   Sepia/Night, layout Single/Spread/Auto, justify) <-> appkit settings values.
7. UI: src/ids.rs (const AzString `__azreader_*`), src/app.rs (AppState, Command), src/lib.rs
   start/layout/callbacks, library screen (DocumentShell, covers grid; TODO(WIDGETS9A) IconGrid),
   reader screen (DocumentShell: TOC + bookmarks pane, page view of clip windows, status bar
   with progress), keys (arrows / PageUp / PageDown / Space, Mod+D bookmark, Mod+T toc),
   settings section, About, CloseGuard not needed (no unsaved document - state saves at once).
8. scripts/azreader_e2e.py; report scripts/READER9_2026_10_03.md (or the finishing date).

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
  their file size, independent of layout. A TOC #fragment is placed by the text before it
  (Chapter::anchor_fraction); an exact refinement via get_node_id_by_id_attribute +
  get_node_position is a later step.
- PAPER: the reading surface's colours are "paper" (Auto = follows the mode, White, Sepia,
  Night) - never "theme" (naming rule: theme = flat/flora, mode = light/dark).
- DATA: reader/books/<uuid>/book.<epub|txt|html> (the imported file), info.json (title,
  author, format, added), state.json (position, bookmarks, last read) - small frequent writes
  apart from the big file; reading settings in the appkit settings file.
- CSS: the book's sheets are kept, filtered (bookcss.rs): no @font-face / @import / url();
  colours and backgrounds dropped (the paper decides), font-family dropped (monospace kept),
  absolute font sizes / line heights / widths dropped, rem -> em, positioning dropped; @media
  for screen unwrapped, print / amzn / kindle dropped.
- IMAGES: decoded on the chapter thread for their size; every <img> gets an explicit CSS size
  that fits the page (the same box in the pagination and on screen); registered in the image
  cache under `azreader:<book>/<path>` (the `<img src>` the content writes).
- LINKS: `href` is dropped in v1 (parsed links are not clickable in azul); footnote / internal
  link following is a later step (walk the Dom for `a` nodes, add a callback).

## Open questions
- (none)
