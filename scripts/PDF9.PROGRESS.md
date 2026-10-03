# PDF9 progress (wave 9: PDF -> SVG in azul's API, AzPdf, the Chrome probe)

Branch `wt/pdf9` from `e537ddbe2`. Report: `scripts/PDF9_2026_10_03.md`.

## DONE
- (none yet)

## IN PROGRESS
- reading: house rules, wave9 PLAN (PDF9), ../azul-apps/planning/core/pdf.md, printpdf 84dce8c
  (~/.cargo/git/checkouts/printpdf-d5271e4ecfa3944f/84dce8c), dll/src/desktop/extra/pdf/mod.rs

## NEXT
1. RED: tests for the parsed-PDF handle (page count, page sizes, page N as SVG, page text).
2. GREEN: `ParsedPdf` handle in dll/src/desktop/extra/pdf/mod.rs (+ wasm stub in dll/src/unified/pdf.rs).
3. AzPdf app (examples/azul-pdf) on the DocumentShell.
4. Chrome comparison probe (scripts/pdf_chrome_probe.py).
5. Report.

## Findings so far
- printpdf already has `PdfDocument::parse(bytes, &PdfParseOptions, &mut warnings)`, `page_to_svg(page_1_based,
  &PdfToSvgOptions, &mut warnings)`, `PdfPage { media_box, trim_box, crop_box, ops }`, `PdfPage::extract_text`.
  render.rs/deserialize.rs are always compiled (no feature needed).
- azul already has `Pdf::to_svg_pages(bytes) -> StringVec` (renders EVERY page in one call) and Rust-only
  `Pdf::svg_page_to_dom`. AzDrive renders page 1 via to_svg_pages + ParsedSvg::render.
- azul's CPU SVG renderer (layout/src/cpurender/svg.rs) draws g/svg/path/rect/circle/... only: no `<text>`, no
  `<image>`; printpdf's page SVG carries text as `<text>` + `@font-face` data URIs and images as `<image href=data:>`.

## Decisions
- (see report)

## Open questions
- (none)
