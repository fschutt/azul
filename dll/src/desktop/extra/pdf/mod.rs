//! PDF export for AzulDoc (SUPER_PLAN_2 §4 P5.1), via `printpdf`.
//!
//! Like `Db` (P4.3), the export API is **always present** and the engine
//! sits behind the `pdf` feature — so it flows through normal api.json
//! codegen with no feature-gating. Without `pdf`, `export_to_pdf` returns
//! `false` (printpdf isn't compiled).
//!
//! `printpdf` is pulled with `default-features = false`: its default `html`
//! feature would drag in `azul-layout` (printpdf's own layout integration)
//! and cycle with our local crate. We use only printpdf's core
//! `PdfDocument`/`Op` API and walk **Azul's** display list ourselves.
//!
//! Dispatch status: `DisplayListItem::Rect` (solid fills / backgrounds) →
//! `Op::DrawRectangle`. Text / Image / Border (research/06 §2.3.2's table;
//! `TextLayout` is half-wired) land in follow-ups.

use azul_core::json::Json;
use azul_css::U8Vec;
use azul_layout::solver3::display_list::DisplayListItem;

/// Export the display-list `items` to a PDF at `path`. Returns `true` on
/// success; `false` without the `pdf` feature or on a write error.
pub fn export_to_pdf(path: &str, items: &[DisplayListItem]) -> bool {
    #[cfg(feature = "pdf")]
    {
        engine::export(path, items)
    }
    #[cfg(not(feature = "pdf"))]
    {
        let _ = (path, items);
        false
    }
}

/// Write a PDF from a JSON document model → PDF bytes. The JSON is
/// printpdf's `PdfDocument` serde schema as an [`azul_core::json::Json`]
/// (the same model printpdf's wasm api uses). Returns empty bytes on a
/// malformed model or without the `pdf` feature.
///
/// ABI-stable read/write: the document schema lives in the JSON value, so
/// the PDF model can evolve without changing the C ABI.
pub fn pdf_write_json(json: &Json) -> U8Vec {
    #[cfg(feature = "pdf")]
    {
        engine::write_json(json)
    }
    #[cfg(not(feature = "pdf"))]
    {
        let _ = json;
        U8Vec::from_vec(Vec::new())
    }
}

/// Read a PDF (`bytes`) into the JSON document model ([`Json`]). Returns
/// JSON `null` on a parse error or without the `pdf` feature.
pub fn pdf_read_json(bytes: &[u8]) -> Json {
    #[cfg(feature = "pdf")]
    {
        engine::read_json(bytes)
    }
    #[cfg(not(feature = "pdf"))]
    {
        let _ = bytes;
        Json::null()
    }
}

#[cfg(feature = "pdf")]
mod engine {
    use super::{DisplayListItem, Json, U8Vec};
    use printpdf::{
        Color, Mm, Op, PaintMode, PdfDocument, PdfPage, PdfParseOptions, PdfSaveOptions,
        PdfWarnMsg, Pt, Rect, Rgb, WindingOrder,
    };

    /// JSON document model → PDF bytes (printpdf `PdfDocument` via serde).
    pub fn write_json(json: &Json) -> U8Vec {
        let json_str = json.to_string_pretty();
        let doc: PdfDocument = match serde_json::from_str(json_str.as_str()) {
            Ok(d) => d,
            Err(_) => return U8Vec::from_vec(Vec::new()),
        };
        let mut warnings: Vec<PdfWarnMsg> = Vec::new();
        U8Vec::from_vec(doc.save(&PdfSaveOptions::default(), &mut warnings))
    }

    /// PDF bytes → JSON document model (printpdf parse → serde → `Json`).
    pub fn read_json(bytes: &[u8]) -> Json {
        let mut warnings: Vec<PdfWarnMsg> = Vec::new();
        let doc = match PdfDocument::parse(bytes, &PdfParseOptions::default(), &mut warnings) {
            Ok(d) => d,
            Err(_) => return Json::null(),
        };
        match serde_json::to_string(&doc) {
            Ok(s) => Json::parse(&s).unwrap_or_else(|_| Json::null()),
            Err(_) => Json::null(),
        }
    }

    // A4 portrait. Azul logical px are assumed at 96 DPI (CSS reference px).
    const PAGE_W_MM: f32 = 210.0;
    const PAGE_H_MM: f32 = 297.0;
    const PX_TO_PT: f32 = 72.0 / 96.0;
    const MM_TO_PT: f32 = 72.0 / 25.4;

    pub fn export(path: &str, items: &[DisplayListItem]) -> bool {
        let page_h_pt = PAGE_H_MM * MM_TO_PT;
        let mut ops: Vec<Op> = Vec::new();

        for item in items {
            // v1: solid-fill rectangles (backgrounds / colored boxes). Other
            // variants are skipped until their dispatch lands.
            if let DisplayListItem::Rect { bounds, color, .. } = item {
                let r = bounds.inner();
                let x = r.origin.x * PX_TO_PT;
                let w = r.size.width * PX_TO_PT;
                let h = r.size.height * PX_TO_PT;
                // Azul: top-left origin, y grows down. PDF: bottom-left, y up.
                let y = page_h_pt - (r.origin.y * PX_TO_PT) - h;
                ops.push(Op::SetFillColor {
                    col: Color::Rgb(Rgb {
                        r: color.r as f32 / 255.0,
                        g: color.g as f32 / 255.0,
                        b: color.b as f32 / 255.0,
                        icc_profile: None,
                    }),
                });
                ops.push(Op::DrawRectangle {
                    rectangle: Rect {
                        x: Pt(x),
                        y: Pt(y),
                        width: Pt(w),
                        height: Pt(h),
                        mode: Some(PaintMode::Fill),
                        winding_order: Some(WindingOrder::NonZero),
                    },
                });
            }
        }

        let mut doc = PdfDocument::new("AzulDoc export");
        let page = PdfPage::new(Mm(PAGE_W_MM), Mm(PAGE_H_MM), ops);
        doc.with_pages(vec![page]);
        let mut warnings: Vec<PdfWarnMsg> = Vec::new();
        let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);
        std::fs::write(path, bytes).is_ok()
    }
}
