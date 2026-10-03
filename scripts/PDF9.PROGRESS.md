# PDF9 progress (wave 9: PDF -> SVG in azul's API, AzPdf, the Chrome probe)

Branch `wt/pdf9` from `e537ddbe2`. Report: `scripts/PDF9_2026_10_03.md` (not written yet).

## DONE
- 6a2db82c5 progress file
- 68ea55c03 RED: ParsedPdf tests (dll/src/desktop/extra/pdf/parsed_tests.rs)
- 41a010357 GREEN: ParsedPdf + PdfPageSize (dll/src/desktop/extra/pdf/parsed.rs, wasm stub in dll/src/unified/pdf.rs);
  Pdf::to_svg_pages now goes through ParsedPdf (engine twin removed)
- 88398b1da RED: layout/tests/an_svg_paints_any_css_colour_and_nests_its_transforms.rs,
  layout/tests/an_svg_renders_at_the_size_its_fit_asks_for.rs (appended to layout/tests/all.rs)
- b7e776176 GREEN: parse_svg_color = ColorU::parse_css (rgb() etc.)
- 87e070dd0 GREEN: transform order (own.multiply(parent)), transform lists, rotate centre, skew; svg_render honours fit
- da684d01a RED: examples/azul-pdf skeleton (Cargo.toml, main.rs, lib.rs stub) + src/model_tests.rs
- 52cff2660 GREEN: examples/azul-pdf/src/model.rs
- f09856dbe registered AzPdf (root Cargo.toml members, scripts/workspace_test_members.txt, rust.yml dll_tests step)
- 2a986919e ids.rs; 3bd89abb2 RED export-switch test (+ jobs.rs); b4339c1db parse_export; fe54679a4 lib.rs;
  3a2efd8de ui.rs - the AzPdf app is written (not compiled).

## IN PROGRESS
- scripts/pdf_chrome_probe.py, scripts/azpdf_e2e.py, then the report.

## NEXT (exact)
(1-3 DONE: jobs.rs, lib.rs, ui.rs written)
1. (done) examples/azul-pdf/src/jobs.rs: `Kind {Page, Thumb}`, `Doc {path,title,pdf: azul::pdf::ParsedPdf,sizes,outline,warnings}`,
   `Job {Open{generation,path}, Render{generation,pdf,pages:Vec<(Kind,usize,u32)>}, Texts{generation,pdf}, Sample}`,
   `Outcome {Opened, Rendered, RenderDone, Texts, Sample}`, `job_thread` (pattern: examples/azul-drive/src/jobs.rs
   send()/job_thread), `open(path)` via azul_appkit::files::read_outside + ParsedPdf::create_from_bytes(U8VecRef),
   `render_page_raw(pdf, page, width)` = page_to_svg -> ParsedSvg::from_string -> SvgRenderOptions{fit: Width(w),
   background white} -> render; `sample_pdf()` = azul::pdf::Pdf::create().from_dom(dom, 794, 1123).
2. lib.rs: AppState {kit, data_root, doc, loading, generation, zoom, view_w/h, dpi, current_page, visible, thumbs_visible,
   pages: PageCache<ImageRef>, thumbs: PageCache<ImageRef>, running, threads, nav_tab, recent, search, pending_scroll,
   status}; run() (handles `--export-png OUT [--page N] [--width W] FILE` and `--export-svg` BEFORE AppArgs::from_env,
   for the probe); on_window_created (kit + recent.json load via kit::spawn_file_jobs + 120 ms pump timer);
   on_pump (pending scroll via info.scroll_to on ids::PAGES_NAME, plan renders, save recent); on_job_done; keys
   (Mod+O, Mod+=/-/0, PageUp/Down, Home/End); FileDialog::open_file; DroppedFile.
3. ui.rs: start screen (ShellEmptyState + recent list) / DocumentShell (nav: Segmented Pages|Outline + thumbs
   VirtualView; document: toolbar (TODO(WIDGETS9A): Toolbar) + pages VirtualView with absolutely positioned page frames;
   side pane: search hits; status bar: page N of M, size label, zoom, status). Pattern: examples/azul-review/src/ui.rs.
4. scripts/azpdf_e2e.py (writes a minimal 2-page PDF itself, runs AzPdf <file>, asserts nodes) and
   scripts/pdf_chrome_probe.py (Chrome headless PDF viewer needs scripts ON: cdp.Chrome(extra_args=
   ["--blink-settings=scriptEnabled=true"]), URL `file://X.pdf#toolbar=0`; crop the page; compare with
   `AzPdf --export-png`, and with Chrome rendering the page SVG (`--export-svg`); pdftoppm as an extra column).
5. Engine (optional, after the app): `<text>` (fonts from @font-face data URLs) and `<image>` in
   layout/src/cpurender/svg.rs.
6. Report scripts/PDF9_2026_10_03.md (api.json list below, least-sure spots, test commands).

## api.json list (for the report)
- module `pdf`: class `ParsedPdf` (external azul_dll::unified::pdf::ParsedPdf, repr C, fields ptr: *mut c_void,
  run_destructor: bool; custom_impls Clone/Default/Drop): constructor `create_from_bytes(bytes: U8VecRef)` fn_body
  `azul_dll::unified::pdf::ParsedPdf::from_bytes(bytes.as_slice())`; functions (self ref) `is_valid -> bool`,
  `get_error -> String`, `get_warnings -> StringVec`, `get_title -> String`, `page_count -> usize`,
  `page_size(index: usize) -> PdfPageSize`, `page_to_svg(index: usize) -> OptionString`, `page_text(index: usize) ->
  StringVec`, `outline_count -> usize`, `outline_title(index: usize) -> String`, `outline_page(index: usize) -> usize`.
- module `pdf`: struct `PdfPageSize { width_pt: f32, height_pt: f32 }` (Debug, Default, Clone, Copy, PartialEq),
  function `to_logical_size(self ref) -> LogicalSize`.

## Findings so far
- printpdf render.rs bugs (user's crate; not fixable here, no [patch]):
  (a) cubic curves written `Q c1 c2` + `L end` (should be `C c1 c2 end`) in render_line_to_svg / render_polygon_to_svg;
  (b) a path's CTM written `matrix(a -b -c d e H-f)` on already y-flipped coordinates; correct is
      `matrix(a -b -c d e+c*H H-d*H-f)` (get_svg_transform) - every PDF with a `cm` is misplaced;
  (c) colour spaces TODO, inline images, shadings skipped.
- Chrome headless renders PDFs only with scripts enabled (refci's cdp.py disables them by default).
- azul CPU SVG renderer (fixed here): rgb() paints, transform order + lists, fit. Still missing: `<text>`, `<image>`.

## Decisions
- API: `ParsedPdf` (Arc handle like `Db`), 0-based page indices; outline via count/title/page accessors (no new Vec
  type); `PdfPageSize` + `to_logical_size()`.
- AzPdf is a viewer: no CloseGuard (nothing to save). Opened PDFs stay where they are; the data tree holds
  `pdf/recent.json` (and `pdf/samples/` for --sample).

## Open questions
- (none)
