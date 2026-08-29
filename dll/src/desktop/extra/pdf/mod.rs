//! PDF for AzulDoc (SUPER_PLAN_2 §4 P5.1), via `printpdf`.
//!
//! Like `Db` (P4.3), the `Pdf` API is **always present** and the engine
//! sits behind the `pdf` feature - so it flows through normal api.json
//! codegen with no feature-gating. Without `pdf`, `Pdf::from_dom` /
//! `write_json` return empty bytes (printpdf isn't compiled).
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

/// Say once per process that the `pdf` feature is compiled out. Every stub
/// arm below returns an empty/none result that is byte-for-byte
/// indistinguishable from "the document rendered to nothing" — the same
/// silent-degradation trap as the video pipeline stub.
#[cfg(not(feature = "pdf"))]
fn announce_pdf_stub(what: &str) {
    static ANNOUNCE: std::sync::Once = std::sync::Once::new();
    ANNOUNCE.call_once(|| {
        eprintln!(
            "[azul][pdf] {what} called, but this build has no `pdf` feature — the \
             whole PDF surface returns EMPTY results. Rebuild with: cargo build -p \
             azul-dll --features build-dll,pdf"
        );
    });
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
        announce_pdf_stub("pdf_write_json");
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
        announce_pdf_stub("pdf_read_json");
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
        announce_pdf_stub("dom_to_pdf");
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

    /// Like [`Pdf::from_dom`], but called from INSIDE a running app's
    /// callback: reuses the window's live font manager and image cache
    /// instead of building fresh ones, so the PDF resolves exactly the fonts
    /// (same fallback chains, same embedded/in-memory fonts) and images the
    /// screen did. This is the path for print-preview parity — `from_dom`
    /// standalone can legitimately pick different system fonts than the
    /// window and knows nothing about app-registered images.
    pub fn from_dom_in_callback(
        &self,
        callback_info: &azul_layout::callbacks::CallbackInfo,
        dom: Dom,
        page_width_px: f32,
        page_height_px: f32,
    ) -> U8Vec {
        #[cfg(feature = "pdf")]
        {
            let lw = callback_info.get_layout_window();
            // Share-derived: parsed fonts/registry are shared handles, chain
            // caches copied — the window's resolution state, not a re-scan.
            let mut font_manager = lw.font_manager.clone_shared();
            U8Vec::from_vec(engine::dom_to_bytes_with(
                dom,
                page_width_px,
                page_height_px,
                &mut font_manager,
                &lw.image_cache,
            ))
        }
        #[cfg(not(feature = "pdf"))]
        {
            announce_pdf_stub("Pdf::from_dom_in_callback");
            let _ = (callback_info, dom, page_width_px, page_height_px);
            U8Vec::from_vec(Vec::new())
        }
    }

    /// The fully TYPED export path: an already-styled DOM plus resource
    /// snapshots, no JSON / base64 / re-decode round-trips.
    ///
    /// Take the three inputs from a callback —
    /// `CallbackInfo::get_styled_dom_clone()` (cascade state as on screen),
    /// `CallbackInfo::get_font_cache_clone()` (shared parsed-font pool) and
    /// `CallbackInfo::get_image_cache_clone()` (refcounted decoded images) —
    /// and the produced PDF is the window's content re-laid-out at page
    /// size. All three snapshots stay valid after the callback returns, so
    /// this can run on a worker thread. Empty handles fall back to fresh
    /// system resources (the `from_dom` behavior).
    pub fn from_styled_dom_with_resources(
        &self,
        styled_dom: azul_core::styled_dom::StyledDom,
        page_width_px: f32,
        page_height_px: f32,
        font_cache: &azul_layout::resource_handles::FontCacheSnapshot,
        image_cache: &azul_layout::resource_handles::ImageCacheSnapshot,
    ) -> U8Vec {
        #[cfg(feature = "pdf")]
        {
            let mut font_manager = match font_cache.as_font_manager() {
                Some(fm) => fm.clone_shared(),
                None => {
                    let fc_cache = azul_layout::font::loading::build_font_cache();
                    match azul_layout::font_traits::FontManager::new(fc_cache) {
                        Ok(f) => f,
                        Err(_) => return U8Vec::from_vec(Vec::new()),
                    }
                }
            };
            let empty_images = azul_core::resources::ImageCache::default();
            let images = image_cache.as_image_cache().unwrap_or(&empty_images);
            U8Vec::from_vec(engine::styled_dom_to_bytes_with(
                styled_dom,
                page_width_px,
                page_height_px,
                &mut font_manager,
                images,
            ))
        }
        #[cfg(not(feature = "pdf"))]
        {
            announce_pdf_stub("Pdf::from_styled_dom_with_resources");
            let _ = (
                styled_dom,
                page_width_px,
                page_height_px,
                font_cache,
                image_cache,
            );
            U8Vec::from_vec(Vec::new())
        }
    }

    /// PRECALCULATION-ONLY pagination (the document-editor query): full
    /// layout + break analysis at the given page size, but NO per-page
    /// display list is ever materialized. The returned snapshot answers
    /// page-count / page-of-Y / break positions; pair with the paged export
    /// to materialize only what is visible. Empty handle without the `pdf`
    /// feature or on failure.
    pub fn compute_pagination(
        &self,
        styled_dom: azul_core::styled_dom::StyledDom,
        page_width_px: f32,
        page_height_px: f32,
        font_cache: &azul_layout::resource_handles::FontCacheSnapshot,
        image_cache: &azul_layout::resource_handles::ImageCacheSnapshot,
    ) -> azul_layout::resource_handles::PaginationSnapshot {
        #[cfg(feature = "pdf")]
        {
            let mut font_manager = match font_cache.as_font_manager() {
                Some(fm) => fm.clone_shared(),
                None => {
                    let fc_cache = azul_layout::font::loading::build_font_cache();
                    match azul_layout::font_traits::FontManager::new(fc_cache) {
                        Ok(f) => f,
                        Err(_) => {
                            return azul_layout::resource_handles::PaginationSnapshot::empty()
                        }
                    }
                }
            };
            let empty_images = azul_core::resources::ImageCache::default();
            let images = image_cache.as_image_cache().unwrap_or(&empty_images);
            engine::styled_dom_pagination(
                &styled_dom,
                page_width_px,
                page_height_px,
                &mut font_manager,
                images,
            )
            .map_or_else(
                azul_layout::resource_handles::PaginationSnapshot::empty,
                azul_layout::resource_handles::PaginationSnapshot::from_analysis,
            )
        }
        #[cfg(not(feature = "pdf"))]
        {
            announce_pdf_stub("Pdf::compute_pagination");
            let _ = (
                styled_dom,
                page_width_px,
                page_height_px,
                font_cache,
                image_cache,
            );
            azul_layout::resource_handles::PaginationSnapshot::empty()
        }
    }

    /// REVERSE path: PDF bytes -> one standalone SVG string per page.
    /// Empty without the `pdf` feature or on parse failure.
    pub fn to_svg_pages(&self, bytes: &[u8]) -> Vec<String> {
        pdf_to_svg_pages(bytes)
    }

    /// Turn one page-SVG (from [`Pdf::to_svg_pages`]) into a `Dom` subtree,
    /// via the framework's existing svg-to-dom path (the map-tile pipeline).
    pub fn svg_page_to_dom(&self, svg: &str) -> Option<Dom> {
        svg_page_to_dom(svg)
    }
}

/// PDF bytes -> per-page SVG strings (see [`Pdf::to_svg_pages`]).
pub fn pdf_to_svg_pages(bytes: &[u8]) -> Vec<String> {
    #[cfg(feature = "pdf")]
    {
        engine::pdf_to_svg_pages(bytes)
    }
    #[cfg(not(feature = "pdf"))]
    {
        announce_pdf_stub("pdf_to_svg_pages");
        let _ = bytes;
        Vec::new()
    }
}

/// One page-SVG -> Dom (see [`Pdf::svg_page_to_dom`]).
pub fn svg_page_to_dom(svg: &str) -> Option<Dom> {
    #[cfg(all(feature = "pdf", feature = "xml"))]
    {
        azul_layout::widgets::map::svg_string_to_dom(svg)
    }
    #[cfg(not(all(feature = "pdf", feature = "xml")))]
    {
        // Different gate than the arms above: needs BOTH `pdf` and `xml`.
        static ANNOUNCE: std::sync::Once = std::sync::Once::new();
        ANNOUNCE.call_once(|| {
            eprintln!(
                "[azul][pdf] svg_page_to_dom called, but this build lacks the `pdf` \
                 and/or `xml` feature — it always returns None. Rebuild with: cargo \
                 build -p azul-dll --features build-dll,pdf,xml"
            );
        });
        let _ = svg;
        None
    }
}

#[cfg(feature = "pdf")]
mod engine {
    use super::{DisplayListItem, Dom, Json, U8Vec};
    use printpdf::{Mm, Op, PdfDocument, PdfPage, PdfParseOptions, PdfSaveOptions, PdfWarnMsg};

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

    // Azul logical px are assumed at 96 DPI (CSS reference px).
    const PX_TO_PT: f32 = 72.0 / 96.0;

    /// Collect every RAW image in the display lists into printpdf XObjects and
    /// rewrite the items so the bridge can reference them. The bridge's Image
    /// arm only understands `NullImage {{ tag: <key> }}` lookups into a
    /// `ResolvedImages` map (the HTML `<img src>` pattern); a Dom built in
    /// code carries real `DecodedImage::Raw` refs, which it silently skips —
    /// so images never reached the PDF. We key each raw image by its
    /// `ImageRef` hash, register the pixels under `image_xobject_id(key)`,
    /// and swap the item's ref for a tagged NullImage of the same size.
    fn resolve_raw_images(
        display_lists: &mut Vec<azul_layout::solver3::display_list::DisplayList>,
        images: &mut printpdf::html::bridge::ResolvedImages,
    ) {
        use azul_core::resources::{ImageRef, RawImageData};
        use azul_layout::solver3::display_list::DisplayListItem;

        for dl in display_lists.iter_mut() {
            for item in dl.items.iter_mut() {
                let DisplayListItem::Image { image, .. } = item else {
                    continue;
                };
                let Some(raw) = image.get_rawimage() else {
                    continue;
                };
                let key = alloc::format!("rawimg-{:016x}", image.get_hash().inner);
                if !images.contains_key(&key) {
                    let pixels = match raw.pixels {
                        RawImageData::U8(v) => printpdf::RawImageData::U8(v.as_ref().to_vec()),
                        _ => continue, // U16/F32 raws: not produced by the Dom API today
                    };
                    let pp_raw = printpdf::RawImage {
                        pixels,
                        width: raw.width,
                        height: raw.height,
                        data_format: convert_image_format(raw.data_format),
                        tag: Vec::new(),
                    };
                    images.insert(
                        key.clone(),
                        (printpdf::html::bridge::image_xobject_id(&key), pp_raw),
                    );
                }
                *image =
                    ImageRef::null_image(raw.width, raw.height, raw.data_format, key.into_bytes());
            }
        }
    }

    /// azul -> printpdf pixel format (same variant vocabulary).
    fn convert_image_format(f: azul_core::resources::RawImageFormat) -> printpdf::RawImageFormat {
        use azul_core::resources::RawImageFormat as A;
        use printpdf::RawImageFormat as P;
        match f {
            A::R8 => P::R8,
            A::RG8 => P::RG8,
            A::RGB8 => P::RGB8,
            A::RGBA8 => P::RGBA8,
            A::BGR8 => P::BGR8,
            A::BGRA8 => P::BGRA8,
            A::R16 => P::R16,
            A::RG16 => P::RG16,
            A::RGB16 => P::RGB16,
            A::RGBA16 => P::RGBA16,
            A::RGBF32 => P::RGBF32,
            A::RGBAF32 => P::RGBAF32,
        }
    }

    /// REVERSE path: parse PDF bytes and render every page to a standalone
    /// SVG string (printpdf's `page_to_svg`; pages are 1-indexed there).
    /// Each SVG can then be turned into a Dom via
    /// `azul_layout::widgets::map::svg_string_to_dom` (the same svg-to-dom
    /// path the map tiles use) — see `Pdf::svg_page_to_dom`.
    pub fn pdf_to_svg_pages(bytes: &[u8]) -> Vec<String> {
        let mut warnings: Vec<PdfWarnMsg> = Vec::new();
        let Ok(doc) = PdfDocument::parse(bytes, &PdfParseOptions::default(), &mut warnings) else {
            return Vec::new();
        };
        let opts = printpdf::PdfToSvgOptions::default();
        (1..=doc.pages.len())
            .filter_map(|n| doc.page_to_svg(n, &opts, &mut warnings))
            .collect()
    }

    /// Walk one page's display list into printpdf draw ops via printpdf's own
    /// html bridge — rectangles, borders, images AND TEXT (SetFont/ShowText
    /// from `azul_layout::text3::glyphs::get_glyph_runs_pdf`). Replaces the
    /// old `rect_ops` walk that handled `DisplayListItem::Rect` only and
    /// produced textless PDFs (the azul-writer "writer without text" bug).
    ///
    /// Also extracts every font used by the page's `TextLayout` items into
    /// `font_data` (keyed by glyph font-hash) so the caller can register them
    /// on the `PdfDocument` under the same `F{hash}` ids the bridge's
    /// `SetFont` ops reference.
    fn page_ops(
        dl: &azul_layout::solver3::display_list::DisplayList,
        // POINTS, not px. printpdf's CoordTransform computes
        // `page_height - layout_y * (72/96)`: it scales the LAYOUT
        // coordinate from CSS px into pt and subtracts from a page height
        // that must already BE in pt. Handing it the pixel height put every
        // drawing op ~1.33x too high — on A4 the whole content landed above
        // the 842pt page box, so exported PDFs carried correct text ops,
        // embedded fonts and page objects yet rendered BLANK.
        page_size_pt: azul_core::geom::LogicalSize,
        font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
        images: &printpdf::html::bridge::ResolvedImages,
        bridge_res: &mut printpdf::html::bridge::BridgeResources,
        font_data: &mut std::collections::BTreeMap<
            azul_layout::text3::cache::FontHash,
            azul_layout::font::parsed::ParsedFont,
        >,
    ) -> Vec<Op> {
        use azul_layout::solver3::display_list::DisplayListItem;
        use azul_layout::text3::cache::{FontHash, ShapedItem, UnifiedLayout};

        let ops = printpdf::html::bridge::display_list_to_printpdf_ops_with_margins(
            dl,
            page_size_pt,
            0.0,
            0.0,
            font_manager,
            images,
            bridge_res,
        )
        .unwrap_or_default();

        // Collect the fonts the bridge's SetFont ops reference (mirrors
        // printpdf's xml_to_pdf_pages).
        for item in dl.items.iter() {
            if let DisplayListItem::TextLayout { layout, .. } = item {
                // (d5) TextPayload carries both forms; the font-collection
                // walk reads the sparse half until d6, when the dense
                // run.font_hash walk replaces it.
                // (d6h) Post-retirement the payload's sparse half is the
                // EMPTY sentinel — the font hashes come straight from the
                // dense runs (one per run instead of one per glyph).
                if let Some(p) =
                    layout.downcast_ref::<azul_layout::solver3::layout_tree::TextPayload>()
                {
                    if p.sparse.items.is_empty() && !p.dense.clusters.is_empty() {
                        for run in &p.dense.runs {
                            let key = FontHash {
                                font_hash: run.font_hash,
                            };
                            if font_data.contains_key(&key) {
                                continue;
                            }
                            if let Some(font_ref) = font_manager.resolve_font_by_hash(run.font_hash)
                            {
                                let parsed =
                                    azul_layout::font_ref_to_parsed_font(&font_ref).clone();
                                font_data.insert(key, parsed);
                            }
                        }
                        continue;
                    }
                }
                let unified_from_payload = layout
                    .downcast_ref::<azul_layout::solver3::layout_tree::TextPayload>()
                    .map(|p| p.sparse.as_ref());
                if let Some(unified) =
                    unified_from_payload.or_else(|| layout.downcast_ref::<UnifiedLayout>())
                {
                    for positioned in &unified.items {
                        if let ShapedItem::Cluster(cluster) = &positioned.item {
                            for glyph in &cluster.glyphs {
                                let key = FontHash {
                                    font_hash: glyph.font_hash,
                                };
                                if font_data.contains_key(&key) {
                                    continue;
                                }
                                // The ONE lookup: PDF export of icon text (a
                                // `StyleFontFamily::Ref` face) used to find nothing
                                // here and silently drop the glyphs.
                                if let Some(font_ref) =
                                    font_manager.resolve_font_by_hash(glyph.font_hash)
                                {
                                    // Safe reborrow, lifetime tied to `font_ref`.
                                    let parsed =
                                        azul_layout::font_ref_to_parsed_font(&font_ref).clone();
                                    font_data.insert(key, parsed);
                                }
                            }
                        }
                    }
                }
            }
        }

        ops
    }

    /// Headless `dom -> paged PDF bytes` with a FRESH font manager and an
    /// empty image cache. Standalone conversions (no running window) get the
    /// system fonts via `build_font_cache`; a callback that wants the PDF to
    /// match the window pixel-for-pixel should use
    /// [`dom_to_bytes_with`] via [`super::Pdf::from_dom_in_callback`], which
    /// shares the WINDOW's font manager and image cache instead.
    pub fn dom_to_bytes(dom: Dom, page_w_px: f32, page_h_px: f32) -> Vec<u8> {
        use azul_layout::font::loading::build_font_cache;
        use azul_layout::font_traits::FontManager;

        let fc_cache = build_font_cache();
        let mut font_manager = match FontManager::new(fc_cache) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        dom_to_bytes_with(
            dom,
            page_w_px,
            page_h_px,
            &mut font_manager,
            &azul_core::resources::ImageCache::default(),
        )
    }

    /// Headless `dom -> paged PDF bytes`. Lays out `styled_dom` at
    /// `page_w_px` x `page_h_px` (logical px) via Azul's paged layout (one
    /// DisplayList per sheet), walks each page's display list into printpdf
    /// ops, and saves the multi-page document to bytes. No window, no file I/O.
    /// Empty on layout failure.
    ///
    /// `font_manager` / `image_cache` are the resolution context: pass the
    /// window's own (see [`super::Pdf::from_dom_in_callback`]) and the PDF
    /// resolves exactly the fonts and images the screen did — the print
    /// preview cannot drift from the window.
    pub fn dom_to_bytes_with(
        dom: Dom,
        page_w_px: f32,
        page_h_px: f32,
        font_manager: &mut azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
        image_cache: &azul_core::resources::ImageCache,
    ) -> Vec<u8> {
        // Dom -> StyledDom (CSS cascade), the same conversion the window does
        // each frame. Done here so callers pass a Dom (what they build) rather
        // than a StyledDom (which has no public constructor).
        let styled_dom = azul_core::styled_dom::StyledDom::create_from_dom(dom);
        styled_dom_to_bytes_with(styled_dom, page_w_px, page_h_px, font_manager, image_cache)
    }

    /// Like [`dom_to_bytes_with`], but takes an ALREADY-STYLED DOM — the
    /// cascade state is used as-is. This is the print-parity entry: a
    /// callback clones the window's live `StyledDom`
    /// (`CallbackInfo::get_styled_dom_clone`) and the PDF re-lays-out
    /// exactly the styled content the screen shows, with the resources it
    /// resolved.
    /// The precalculation twin of [`styled_dom_to_bytes_with`]: same layout
    /// pipeline, stops after the break analysis (no page is sliced).
    pub fn styled_dom_pagination(
        styled_dom: &azul_core::styled_dom::StyledDom,
        page_w_px: f32,
        page_h_px: f32,
        font_manager: &mut azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
        image_cache: &azul_core::resources::ImageCache,
    ) -> Option<azul_layout::resource_handles::PaginationAnalysis> {
        use azul_core::dom::DomId;
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
        use azul_core::resources::{IdNamespace, RendererResources};
        use azul_layout::font_traits::TextLayoutCache;
        use azul_layout::paged::FragmentationContext;
        use azul_layout::solver3::paged_layout::compute_document_pagination;
        use azul_layout::solver3::pagination::FakePageConfig;
        use azul_layout::text3::default::PathLoader;
        use std::collections::BTreeMap;

        let content_size = LogicalSize::new(page_w_px, page_h_px);
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

        let info = compute_document_pagination(
            &mut layout_cache,
            &mut text_cache,
            fragmentation_context,
            styled_dom,
            viewport,
            font_manager,
            &BTreeMap::new(),
            &mut debug_messages,
            None,
            &renderer_resources,
            IdNamespace(0),
            DomId::ROOT_ID,
            font_loader,
            page_config,
            image_cache,
            azul_core::task::GetSystemTimeCallback {
                cb: azul_core::task::get_system_time_libstd,
            },
        )
        .ok()?;
        // Map every break to its STRUCTURAL DOM position while the cache
        // that computed it is still alive — this is the only moment the
        // paths can be derived, and it is what `PaginationSnapshot::
        // break_path` hands to API consumers (the document editor's
        // page-split addresses).
        let structural = azul_layout::solver3::paged_layout::pagination_to_dom_breaks(
            &layout_cache,
            styled_dom,
            &info,
        )
        .unwrap_or_default();
        Some(azul_layout::resource_handles::PaginationAnalysis { info, structural })
    }

    pub fn styled_dom_to_bytes_with(
        styled_dom: azul_core::styled_dom::StyledDom,
        page_w_px: f32,
        page_h_px: f32,
        font_manager: &mut azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
        image_cache: &azul_core::resources::ImageCache,
    ) -> Vec<u8> {
        use azul_core::dom::DomId;
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
        use azul_core::resources::{IdNamespace, RendererResources};
        use azul_layout::font_traits::TextLayoutCache;
        use azul_layout::paged::FragmentationContext;
        use azul_layout::solver3::paged_layout::layout_document_paged_with_config;
        use azul_layout::solver3::pagination::FakePageConfig;
        use azul_layout::text3::default::PathLoader;
        use std::collections::BTreeMap;

        let content_size = LogicalSize::new(page_w_px, page_h_px);
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

        // K30c: runtime engine opt-in (design doc §6.1/§6.4 — printpdf is
        // the FIRST consumer of the token engine; the slicer stays the
        // default until the differential gate has covered the full corpus).
        // Both engines produce the SAME thing — one display list per page —
        // and both flow into the shared PDF-op path below. (This branch used
        // to `return Ok(..)`/`return Err(..)` from a `-> Vec<u8>` function,
        // which broke every `pdf`-feature build including CI's `build-dll`
        // check; the token path had simply never been compiled.)
        let tokens_requested = std::env::var("AZ_PAGINATION_ENGINE").as_deref() == Ok("tokens");
        let tokenized: Option<Vec<azul_layout::solver3::display_list::DisplayList>> =
            if tokens_requested {
                let mut dbg2 = None;
                match azul_layout::solver3::paged_layout::layout_document_tokenized(
                    &mut layout_cache,
                    &mut text_cache,
                    &styled_dom,
                    viewport,
                    font_manager,
                    &mut dbg2,
                    image_cache,
                    azul_core::task::GetSystemTimeCallback {
                        cb: azul_core::task::get_system_time_libstd,
                    },
                    font_loader,
                    &renderer_resources,
                    IdNamespace(0),
                    DomId::ROOT_ID,
                    content_size.height,
                    4096,
                ) {
                    Ok(pages) => Some(pages.into_iter().map(|p| p.display_list).collect()),
                    Err(_) => return Vec::new(),
                }
            } else {
                None
            };

        let display_lists = match tokenized {
            Some(dls) => dls,
            None => match layout_document_paged_with_config(
                &mut layout_cache,
                &mut text_cache,
                fragmentation_context,
                &styled_dom,
                viewport,
                font_manager,
                &BTreeMap::new(),
                &mut debug_messages,
                None,
                &renderer_resources,
                IdNamespace(0),
                DomId::ROOT_ID,
                font_loader,
                page_config,
                image_cache,
                azul_core::task::GetSystemTimeCallback {
                    cb: azul_core::task::get_system_time_libstd,
                },
                false,
            ) {
                Ok(d) => d,
                Err(_) => return Vec::new(),
            },
        };

        let page_w_mm = page_w_px * 25.4 / 96.0;
        let page_h_mm = page_h_px * 25.4 / 96.0;
        // The page BOX is mm-derived (A4 = 210x297mm = 595x842pt) while the
        // display lists are laid out in CSS px, so the bridge needs the page
        // extent in pt to invert the Y axis consistently.
        let page_size_pt = LogicalSize::new(page_w_px * PX_TO_PT, page_h_px * PX_TO_PT);
        let mut display_lists = display_lists;
        let mut images = printpdf::html::bridge::ResolvedImages::new();
        resolve_raw_images(&mut display_lists, &mut images);
        let mut bridge_res = printpdf::html::bridge::BridgeResources::default();
        let mut font_data: std::collections::BTreeMap<
            azul_layout::text3::cache::FontHash,
            azul_layout::font::parsed::ParsedFont,
        > = Default::default();

        let pages: Vec<PdfPage> = display_lists
            .iter()
            .map(|dl| {
                let ops = page_ops(
                    dl,
                    page_size_pt,
                    &font_manager,
                    &images,
                    &mut bridge_res,
                    &mut font_data,
                );
                PdfPage::new(Mm(page_w_mm), Mm(page_h_mm), ops)
            })
            .collect();

        let mut doc = PdfDocument::new("AzulDoc");
        // Register the fonts under the SAME ids the bridge's SetFont ops use
        // (FontId("F{hash}")) — mirrors printpdf::html::add_xml_to_document.
        for (font_hash, parsed_font) in font_data.into_iter() {
            let font_id = printpdf::FontId(format!("F{}", font_hash.font_hash));
            // printpdf 0.11 wraps fonts in its own ParsedFont newtype; from_azul adopts
            // the cache-built face (which already carries its source bytes for embedding).
            let pdf_font =
                printpdf::font::PdfFont::new(printpdf::font::ParsedFont::from_azul(parsed_font));
            doc.resources.fonts.map.insert(font_id, pdf_font);
        }
        // Register the raw images the pre-pass collected, under the SAME
        // deterministic ids the bridge's UseXobject ops reference.
        for (_key, (xobject_id, raw_image)) in images.into_iter() {
            doc.resources
                .xobjects
                .map
                .insert(xobject_id, printpdf::XObject::Image(raw_image));
        }
        bridge_res.register_into(&mut doc.resources);
        doc.with_pages(pages);
        let mut warnings: Vec<PdfWarnMsg> = Vec::new();
        doc.save(&PdfSaveOptions::default(), &mut warnings)
    }
}

#[cfg(all(test, feature = "pdf"))]
mod tests {
    use super::*;
    use azul_core::dom::Dom;

    /// The whole point of the bridge walk: PDFs produced from a Dom with text
    /// must EMBED the fonts the SetFont ops reference. The old rect_ops walk
    /// produced rectangle-only PDFs (azul-writer exported textless documents).
    #[test]
    fn dom_to_pdf_embeds_text_fonts() {
        let dom =
            Dom::create_body().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "Hello PDF text — glyphs must embed",
            ));
        let bytes = dom_to_pdf(dom, 595.0, 842.0);
        assert!(
            !bytes.as_ref().is_empty(),
            "PDF generation produced no bytes"
        );

        let mut warnings = Vec::new();
        let doc = printpdf::PdfDocument::parse(
            bytes.as_ref(),
            &printpdf::PdfParseOptions::default(),
            &mut warnings,
        )
        .expect("generated PDF must parse back");
        assert!(
            !doc.resources.fonts.map.is_empty(),
            "no fonts embedded in the PDF — the text walk regressed to rect-only"
        );
        assert!(!doc.pages.is_empty(), "PDF has no pages");
    }

    /// Raw images placed in the Dom must land in the PDF as Image XObjects
    /// (the bridge only resolves tagged NullImages, so the resolve_raw_images
    /// pre-pass keys + registers the pixels; without it images silently
    /// vanished from Dom-built PDFs).
    #[test]
    fn dom_to_pdf_embeds_raw_images() {
        use azul_core::resources::{ImageRef, RawImage, RawImageData, RawImageFormat};

        let px: Vec<u8> = (0..16 * 16 * 4).map(|i| (i % 255) as u8).collect();
        let raw = RawImage {
            pixels: RawImageData::U8(px.into()),
            width: 16,
            height: 16,
            premultiplied_alpha: false,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        };
        let img = ImageRef::new_rawimage(raw).expect("raw image ref");
        let dom = Dom::create_body().with_child(Dom::create_image(img));

        let bytes = dom_to_pdf(dom, 595.0, 842.0);
        assert!(!bytes.as_ref().is_empty());
        let mut warnings = Vec::new();
        let _doc = printpdf::PdfDocument::parse(
            bytes.as_ref(),
            &printpdf::PdfParseOptions::default(),
            &mut warnings,
        )
        .expect("PDF with image must parse back");
        // Assert at the BYTE level: the saved PDF must contain an Image
        // XObject dictionary (uncompressed dictionary keys are plaintext).
        // (The parser does not reconstruct resources.xobjects, so asserting
        // on the parsed doc tests the wrong layer.)
        let hay = String::from_utf8_lossy(bytes.as_ref());
        assert!(
            hay.contains("/Image"),
            "no /Image XObject in the PDF bytes — raw images were dropped"
        );
    }

    /// REVERSE path: PDF bytes -> per-page SVG -> Dom.
    #[test]
    fn pdf_roundtrips_to_svg_pages_and_dom() {
        let dom =
            Dom::create_body().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "Reverse-path text for SVG extraction",
            ));
        let bytes = dom_to_pdf(dom, 595.0, 842.0);
        let pages = pdf_to_svg_pages(bytes.as_ref());
        assert_eq!(pages.len(), 1, "expected exactly one SVG page");
        assert!(
            pages[0].contains("<svg"),
            "page render must be a standalone SVG document"
        );
        let dom = svg_page_to_dom(&pages[0]);
        assert!(
            dom.is_some(),
            "the page SVG must convert into a Dom via the svg-to-dom path"
        );
    }
}
