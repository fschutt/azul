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
- (nothing half-done) Report written: scripts/PDF9_2026_10_03.md. Also done: 78c17f334 probe, 176f5b03b E2E.

## NEXT (exact, optional engine work after the report)
1. RED: layout/tests/an_svg_text_is_drawn_in_its_embedded_font.rs (append to layout/tests/all.rs): a printpdf-style
   page SVG with `<style>@font-face{font-family:"F1";src:url("data:font/otf;base64,...")}</style>` and
   `<text font-family="F1" font-size="24" fill="rgb(0, 0, 0)" transform="matrix(1 0 0 1 72 100)">Hello</text>`
   rendered via ParsedSvg::render must have dark pixels in the text's box (use a test font from
   layout/tests/common/ or the system font fallback `sans-serif`); and `<image href="data:image/png;base64,...">`
   with width/height/transform must paint its pixels.
2. GREEN in layout/src/cpurender/svg.rs `render_svg_group_inner`: a `"text"` arm (shape with
   text3::default::shape_text_for_parsed_font as cpurender/text_raster.rs does, fill glyph outlines through the
   element matrix; fonts from the root's `<style>` @font-face data URLs decoded once per render, else the system
   family) and an `"image"` arm (decode data: URL PNG/JPEG, draw through the matrix). Then update the report's
   "Seen broken / left" and add the commits.
3. When the parent has built AzPdf: run scripts/pdf_chrome_probe.py and put the numbers into the report.

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
