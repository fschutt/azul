//! Single-pass pagination gate.
//!
//! `layout_document_paged_streaming` must hand the sink exactly the pages
//! `layout_document_paged_v2` collects — same count, same order, same content
//! geometry — because both run the same slicer; the only difference is that
//! the streaming path drops each page instead of accumulating it. It must also
//! stop at the first sink error, which is what lets a PDF backend abort on a
//! write failure without finishing the layout.

use std::collections::{BTreeMap, HashMap};

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
};
use azul_layout::{
    font::loading::build_font_cache,
    font_traits::{FontManager, TextLayoutCache},
    paged::FragmentationContext,
    solver3::{
        display_list::{DisplayList, DisplayListItem},
        paged_layout::{
            layout_document_paged_streaming, layout_document_paged_v2, PagedLayoutResult,
            StreamingLayoutResult,
        },
        pagination::FakePageConfig,
        LayoutError,
    },
    text3::default::PathLoader,
    xml::DomXmlExt,
    Solver3LayoutCache,
};

fn fresh_cache() -> Solver3LayoutCache {
    Solver3LayoutCache {
        tree: None,
        calculated_positions: Vec::new(),
        viewport: None,
        scroll_ids: HashMap::new(),
        scroll_id_to_node_id: HashMap::new(),
        counters: HashMap::new(),
        float_cache: HashMap::new(),
        cache_map: Default::default(),
        previous_positions: Vec::new(),
        cached_display_list: None,
        prev_dom_ptr: 0,
        prev_viewport: LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize::zero(),
        },
        ..Default::default()
    }
}

fn run_collected(html: &str, page_h: f32) -> PagedLayoutResult {
    let styled_dom = Dom::from_xml_string(html);
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("fm");
    let mut cache = fresh_cache();
    let mut text_cache = TextLayoutCache::new();
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: LogicalSize::new(800.0, page_h),
    };
    let rr = RendererResources::default();
    let mut dbg = None;
    let loader = PathLoader::new();
    layout_document_paged_v2(
        &mut cache,
        &mut text_cache,
        FragmentationContext::new_paged(LogicalSize::new(800.0, page_h)),
        &styled_dom,
        viewport,
        &mut font_manager,
        &BTreeMap::new(),
        &mut dbg,
        None,
        &rr,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
            loader.load_font_shared(bytes, index)
        },
        FakePageConfig::new(),
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
        false,
    )
    .expect("collected pages")
}

/// Runs the streaming entry, collecting what the sink saw so the pages can be
/// compared. `abort_after` makes the sink fail on that page index.
fn run_streamed(
    html: &str,
    page_h: f32,
    abort_after: Option<usize>,
) -> (Result<StreamingLayoutResult, LayoutError>, Vec<DisplayList>) {
    let styled_dom = Dom::from_xml_string(html);
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("fm");
    let mut cache = fresh_cache();
    let mut text_cache = TextLayoutCache::new();
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: LogicalSize::new(800.0, page_h),
    };
    let rr = RendererResources::default();
    let mut dbg = None;
    let loader = PathLoader::new();

    let mut seen = Vec::new();
    let result = layout_document_paged_streaming(
        &mut cache,
        &mut text_cache,
        FragmentationContext::new_paged(LogicalSize::new(800.0, page_h)),
        &styled_dom,
        viewport,
        &mut font_manager,
        &BTreeMap::new(),
        &mut dbg,
        None,
        &rr,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
            loader.load_font_shared(bytes, index)
        },
        FakePageConfig::new(),
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
        false,
        &mut |page| {
            let index = seen.len();
            seen.push(page);
            if abort_after == Some(index) {
                return Err(LayoutError::InvalidTree);
            }
            Ok(())
        },
    );
    (result, seen)
}

/// Content-item digest: (kind tag, rounded bounds). Markers and zero-size
/// items are structure, not content.
fn content_digest(dl: &DisplayList) -> Vec<(&'static str, i64, i64, i64, i64)> {
    dl.items
        .iter()
        .filter_map(|item| {
            if item.is_push_marker() || item.is_pop_marker() {
                return None;
            }
            let kind = match item {
                DisplayListItem::Rect { .. } => "rect",
                DisplayListItem::Border { .. } => "border",
                DisplayListItem::Text { .. } => "text",
                DisplayListItem::TextLayout { .. } => "textlayout",
                DisplayListItem::Image { .. } => "image",
                _ => return None,
            };
            let b = item.visual_bounds()?;
            if b.size.width <= 0.0 || b.size.height <= 0.0 {
                return None;
            }
            let q = |v: f32| (v / 1.5).round() as i64;
            Some((
                kind,
                q(b.origin.x),
                q(b.origin.y),
                q(b.size.width),
                q(b.size.height),
            ))
        })
        .collect()
}

const SIX_BLOCKS: &str = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .p { height: 150px; background: #ddd; }
    </style></head>
    <body>
        <div class="p">a</div><div class="p">b</div><div class="p">c</div>
        <div class="p">d</div><div class="p">e</div><div class="p">f</div>
    </body></html>"#;

#[test]
fn streaming_emits_the_same_pages_as_collecting() {
    let collected = run_collected(SIX_BLOCKS, 300.0);
    let (streamed, pages) = run_streamed(SIX_BLOCKS, 300.0, None);
    let streamed = streamed.expect("streamed pages");

    assert!(
        collected.pages.len() > 1,
        "fixture must paginate, got {} page(s)",
        collected.pages.len()
    );
    assert_eq!(
        collected.pages.len(),
        pages.len(),
        "streaming must emit the same number of pages"
    );
    assert_eq!(
        streamed.page_count,
        pages.len(),
        "the reported page count must match what the sink saw"
    );
    for (i, (c, s)) in collected.pages.iter().zip(pages.iter()).enumerate() {
        assert_eq!(
            c.items.len(),
            s.items.len(),
            "page {i}: item count diverges"
        );
        assert_eq!(
            content_digest(c),
            content_digest(s),
            "page {i}: content diverges"
        );
    }
}

#[test]
fn streaming_reports_the_page_count_and_analysis() {
    let collected = run_collected(SIX_BLOCKS, 300.0);
    let (streamed, pages) = run_streamed(SIX_BLOCKS, 300.0, None);
    let streamed = streamed.expect("streamed pages");

    assert_eq!(streamed.page_count, pages.len());
    assert_eq!(streamed.page_count, collected.pages.len());
    assert_eq!(streamed.breaks.len(), collected.breaks.len());
    assert_eq!(
        streamed.total_content_height,
        collected.total_content_height
    );
}

#[test]
fn streaming_stops_at_the_first_sink_error() {
    let (result, seen) = run_streamed(SIX_BLOCKS, 300.0, Some(1));

    assert!(
        matches!(result, Err(LayoutError::InvalidTree)),
        "the sink error must propagate, got {result:?}"
    );
    assert_eq!(
        seen.len(),
        2,
        "pagination must stop right after the failing page"
    );
}

#[test]
fn streaming_emits_one_page_for_a_single_page_document() {
    let html = r#"
        <html><head><style>* { margin: 0; padding: 0; }</style></head>
        <body><div style="height: 50px">only</div></body></html>"#;
    let (streamed, pages) = run_streamed(html, 300.0, None);
    let streamed = streamed.expect("streamed pages");

    assert_eq!(pages.len(), 1);
    assert_eq!(streamed.page_count, pages.len());
}
