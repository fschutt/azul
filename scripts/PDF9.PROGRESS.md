# PDF9 progress (wave 9: PDF -> SVG in azul's API, AzPdf, the Chrome probe)

Branch `wt/pdf9` from `e537ddbe2`. Report: `scripts/PDF9_2026_10_03.md`.

## DONE
- 6a2db82c5 progress file
- 68ea55c03 RED: ParsedPdf tests (dll/src/desktop/extra/pdf/parsed_tests.rs)
- 41a010357 GREEN: ParsedPdf + PdfPageSize (dll/src/desktop/extra/pdf/parsed.rs, wasm stub in dll/src/unified/pdf.rs);
  Pdf::to_svg_pages now goes through ParsedPdf (engine twin removed)
- 88398b1da RED: layout/tests/an_svg_paints_any_css_colour_and_nests_its_transforms.rs,
  layout/tests/an_svg_renders_at_the_size_its_fit_asks_for.rs (appended to layout/tests/all.rs)
- b7e776176 GREEN: parse_svg_color = ColorU::parse_css (rgb() etc.)
- 87e070dd0 GREEN: transform order (own.multiply(parent)), transform lists, rotate centre, skew; svg_render honours fit

## IN PROGRESS
- AzPdf app (examples/azul-pdf)

## NEXT
1. AzPdf app skeleton: Cargo.toml, src/main.rs, src/lib.rs, src/ids.rs, model, args; register (root Cargo.toml members,
   scripts/workspace_test_members.txt, .github/workflows/rust.yml dll_tests step).
2. Chrome comparison probe (scripts/pdf_chrome_probe.py on scripts/refci/cdp.py + azul_debug.py).
3. Engine: `<text>` (fonts from @font-face data URLs) and `<image>` (data URLs) in layout/src/cpurender/svg.rs.
4. Report.

## Findings so far
- printpdf: `PdfDocument::parse`, `page_to_svg(1-based)`, `PdfPage::extract_text`, bookmarks (top level only).
- printpdf render.rs bugs (NOT fixable here - the user's crate, git dep, no [patch]):
  (a) cubic curves: points [c1(bezier), c2(bezier), end] are written `Q c1 c2` + `L end` (should be `C c1 c2 end`),
      render.rs render_line_to_svg / render_polygon_to_svg;
  (b) a path's CTM is written `matrix(a -b -c d e H-f)` on ALREADY y-flipped coordinates; correct is
      `matrix(a -b -c d e+c*H H-d*H-f)` (get_svg_transform) - every PDF with a `cm` (most real PDFs) is misplaced;
  (c) colour spaces TODO, inline images, shadings skipped.
- azul CPU SVG renderer (fixed here): rgb() paints, transform order + lists, fit. Still missing: `<text>`, `<image>`.

## Decisions
- API: `ParsedPdf` (Arc handle like `Db`), constructor `create_from_bytes(U8VecRef)`; 0-based page indices; outline via
  count/title/page accessors (no new Vec type); `PdfPageSize { width_pt, height_pt }` + `to_logical_size()`.

## Open questions
- (none)
