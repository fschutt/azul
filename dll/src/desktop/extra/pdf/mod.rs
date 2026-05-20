//! PDF export for AzulDoc (SUPER_PLAN_2 §4 P5.1), via `printpdf`.
//!
//! Like `Db` (P4.3), the export API is **always present** and the engine
//! sits behind the `pdf` feature - so it flows through normal api.json
//! codegen with no feature-gating. Without `pdf`, `export_to_pdf` returns
//! `false` (printpdf isn't compiled).
//!
//! `printpdf` is pulled with `default-features = false`: its default `html`
//! feature would drag in `azul-layout` (printpdf's own layout integration)
//! and cycle with our local crate. We use only printpdf's core
//! `PdfDocument`/`Op` API and walk **Azul's** display list ourselves.
//!
//! Dispatch status: `DisplayListItem::Rect` (solid fills / backgrounds) ->
//! `Op::DrawRectangle`. Text / Image / Border (research/06 §2.3.2's table;
//! `TextLayout` is half-wired) land in follow-ups.

use azul_core::dom::Dom;
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

/// Write a PDF from a JSON document model -> PDF bytes. The JSON is
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

/// Lay out `styled_dom` at `page_width_px` x `page_height_px` (logical px) and
/// render it to PDF bytes - the printpdf-WASM-style "HTML/DOM -> PDF" path.
/// Headless: no window, no file I/O. Multi-page (Azul's paged layout fragments
/// the DOM into one page per sheet). The caller saves the returned bytes.
/// Empty without the `pdf` feature or on layout failure.
pub fn dom_to_pdf(dom: Dom, page_width_px: f32, page_height_px: f32) -> U8Vec {
    #[cfg(feature = "pdf")]
    {
        U8Vec::from_vec(engine::dom_to_bytes(dom, page_width_px, page_height_px))
    }
    #[cfg(not(feature = "pdf"))]
    {
        let _ = (dom, page_width_px, page_height_px);
        U8Vec::from_vec(Vec::new())
    }
}

/// JSON-based PDF read/write handle - the public, ABI-stable surface for
/// AzulDoc. `write_json`/`read_json` exchange printpdf's `PdfDocument` model
/// as an [`azul_core::json::Json`] (the same schema printpdf's wasm api
/// uses), so the document model can evolve without breaking the C ABI.
/// Carries no state; construct with [`Pdf::new`] and call the methods.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pdf {
    /// Reserved namespace marker (`Pdf` is stateless).
    pub _reserved: u8,
}

impl Default for Pdf {
    fn default() -> Self {
        Pdf::new()
    }
}

impl Pdf {
    /// Construct the PDF API handle.
    pub fn new() -> Self {
        Pdf { _reserved: 0 }
    }

    /// Write a PDF from a JSON document model -> PDF bytes. Empty on a
    /// malformed model or without the `pdf` feature.
    pub fn write_json(&self, json: Json) -> U8Vec {
        pdf_write_json(&json)
    }

    /// Read a PDF (`bytes`) into the JSON document model. JSON `null` on a
    /// parse error or without the `pdf` feature.
    pub fn read_json(&self, bytes: U8Vec) -> Json {
        pdf_read_json(bytes.as_slice())
    }

    /// Render `dom` to PDF bytes at the given page size (logical px) - the
    /// "HTML/DOM -> PDF" path. Headless, multi-page, no file I/O; save the
    /// returned bytes yourself. Empty without the `pdf` feature or on failure.
    pub fn from_dom(&self, dom: Dom, page_width_px: f32, page_height_px: f32) -> U8Vec {
        dom_to_pdf(dom, page_width_px, page_height_px)
    }
}

#[cfg(feature = "pdf")]
mod engine {
    use super::{DisplayListItem, Dom, Json, U8Vec};
    use printpdf::{
        Color, Mm, Op, PaintMode, PdfDocument, PdfPage, PdfParseOptions, PdfSaveOptions,
        PdfWarnMsg, Pt, Rect, Rgb, WindingOrder,
    };

    /// JSON document model -> PDF bytes (printpdf `PdfDocument` via serde).
    pub fn write_json(json: &Json) -> U8Vec {
        let json_str = json.to_string_pretty();
        let doc: PdfDocument = match serde_json::from_str(json_str.as_str()) {
            Ok(d) => d,
            Err(_) => return U8Vec::from_vec(Vec::new()),
        };
        let mut warnings: Vec<PdfWarnMsg> = Vec::new();
        U8Vec::from_vec(doc.save(&PdfSaveOptions::default(), &mut warnings))
    }

    /// PDF bytes -> JSON document model (printpdf parse -> serde -> `Json`).
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

    /// Walk one page's display-list items into printpdf draw ops. v1: solid-fill
    /// rectangles (backgrounds / colored boxes); other variants are skipped
    /// until their dispatch lands. `page_h_pt` flips Azul's top-left-origin
    /// y-down coords into PDF's bottom-left y-up.
    fn rect_ops(items: &[DisplayListItem], page_h_pt: f32) -> Vec<Op> {
        let mut ops: Vec<Op> = Vec::new();
        for item in items {
            if let DisplayListItem::Rect { bounds, color, .. } = item {
                let r = bounds.inner();
                let x = r.origin.x * PX_TO_PT;
                let w = r.size.width * PX_TO_PT;
                let h = r.size.height * PX_TO_PT;
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
        ops
    }

    /// Build a single-A4-page PDF from `items` and write it to `path`.
    /// (The legacy window-coupled export; the standalone `dom_to_bytes` below
    /// is the no-file-I/O replacement.)
    pub fn export(path: &str, items: &[DisplayListItem]) -> bool {
        let ops = rect_ops(items, PAGE_H_MM * MM_TO_PT);
        let mut doc = PdfDocument::new("AzulDoc export");
        let page = PdfPage::new(Mm(PAGE_W_MM), Mm(PAGE_H_MM), ops);
        doc.with_pages(vec![page]);
        let mut warnings: Vec<PdfWarnMsg> = Vec::new();
        let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);
        std::fs::write(path, bytes).is_ok()
    }

    /// Headless `dom -> paged PDF bytes`. Lays out `styled_dom` at
    /// `page_w_px` x `page_h_px` (logical px) via Azul's paged layout (one
    /// DisplayList per sheet), walks each page's display list into printpdf
    /// ops, and saves the multi-page document to bytes. No window, no file I/O.
    /// Empty on font-manager / layout failure. Headless layout context mirrors
    /// `layout/tests/*` (build_font_cache + FontManager + paged layout).
    pub fn dom_to_bytes(dom: Dom, page_w_px: f32, page_h_px: f32) -> Vec<u8> {
        use azul_core::dom::DomId;
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
        use azul_core::resources::{IdNamespace, ImageCache, RendererResources};
        use azul_layout::font::loading::build_font_cache;
        use azul_layout::font_traits::{FontManager, TextLayoutCache};
        use azul_layout::paged::FragmentationContext;
        use azul_layout::solver3::paged_layout::layout_document_paged_with_config;
        use azul_layout::solver3::pagination::FakePageConfig;
        use azul_layout::text3::default::PathLoader;
        use std::collections::BTreeMap;

        // Dom -> StyledDom (CSS cascade), the same conversion the window does
        // each frame. Done here so callers pass a Dom (what they build) rather
        // than a StyledDom (which has no public constructor).
        let styled_dom = azul_core::styled_dom::StyledDom::create_from_dom(dom);
        let content_size = LogicalSize::new(page_w_px, page_h_px);
        let fc_cache = build_font_cache();
        let mut font_manager = match FontManager::new(fc_cache) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        let mut layout_cache = azul_layout::Solver3LayoutCache::default();
        let mut text_cache = TextLayoutCache::new();
        let fragmentation_context = FragmentationContext::new_paged(content_size);
        let viewport = LogicalRect {
            origin: LogicalPosition::zero(),
            size: content_size,
        };
        let renderer_resources = RendererResources::default();
        let mut debug_messages = None;
        let loader = PathLoader::new();
        let font_loader = |bytes, index| loader.load_font_shared(bytes, index);
        let page_config = FakePageConfig::new();

        let display_lists = match layout_document_paged_with_config(
            &mut layout_cache,
            &mut text_cache,
            fragmentation_context,
            &styled_dom,
            viewport,
            &mut font_manager,
            &BTreeMap::new(),
            &mut debug_messages,
            None,
            &renderer_resources,
            IdNamespace(0),
            DomId::ROOT_ID,
            font_loader,
            page_config,
            &ImageCache::default(),
            azul_core::task::GetSystemTimeCallback {
                cb: azul_core::task::get_system_time_libstd,
            },
            false,
        ) {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };

        let page_w_mm = page_w_px * 25.4 / 96.0;
        let page_h_mm = page_h_px * 25.4 / 96.0;
        let page_h_pt = page_h_px * PX_TO_PT;
        let pages: Vec<PdfPage> = display_lists
            .iter()
            .map(|dl| PdfPage::new(Mm(page_w_mm), Mm(page_h_mm), rect_ops(&dl.items, page_h_pt)))
            .collect();
        let mut doc = PdfDocument::new("AzulDoc");
        doc.with_pages(pages);
        let mut warnings: Vec<PdfWarnMsg> = Vec::new();
        doc.save(&PdfSaveOptions::default(), &mut warnings)
    }
}
