//! The paged (PDF) path: a document laid out into pages as printpdf lays it
//! out, and the `TextLayout` payloads printpdf draws its text from, read
//! back as positioned clusters.

use std::{collections::BTreeMap, sync::Arc};

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
        display_list::{DisplayListItem, WindowLogicalRect},
        layout_tree::TextPayload,
        paged_layout::layout_document_paged_with_config,
        pagination::FakePageConfig,
    },
    text3::{
        cache::{PositionedItem, ShapedItem, UnifiedLayout},
        default::PathLoader,
    },
    xml::DomXmlExt,
    Solver3LayoutCache,
};

/// The items of a `TextLayout` payload: a bare layout, or the cached
/// payload's sparse half (expanded from its dense half when retired).
fn payload_items(payload: &Arc<dyn std::any::Any + Send + Sync>) -> Option<Vec<PositionedItem>> {
    if let Some(layout) = payload.downcast_ref::<UnifiedLayout>() {
        return Some(layout.items.clone());
    }
    let p = payload.downcast_ref::<TextPayload>()?;
    Some(if p.sparse.items.is_empty() {
        p.dense.to_unified_items()
    } else {
        p.sparse.items.clone()
    })
}

/// `html` laid out into 800 x 600 pages: every `TextLayout` payload's
/// items with the top of its bounds, top to bottom. Each payload is one
/// block container's text, positioned relative to its own box.
pub fn paged_text_layouts(html: &str) -> Vec<(f32, Vec<PositionedItem>)> {
    let styled_dom = Dom::from_xml_string(html);
    let mut font_manager =
        FontManager::new(build_font_cache()).expect("Failed to create font manager");
    let mut layout_cache = Solver3LayoutCache::default();
    let mut text_cache = TextLayoutCache::new();
    let content_size = LogicalSize::new(800.0, 600.0);
    let loader = PathLoader::new();
    let font_loader = |bytes: Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };
    let display_lists = layout_document_paged_with_config(
        &mut layout_cache,
        &mut text_cache,
        FragmentationContext::new_paged(content_size),
        &styled_dom,
        LogicalRect::new(LogicalPosition::zero(), content_size),
        &mut font_manager,
        &BTreeMap::new(),
        &mut Some(Vec::new()),
        None,
        &RendererResources::default(),
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        FakePageConfig::new(),
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
        false,
    )
    .expect("the document lays out");
    let mut layouts: Vec<(f32, Vec<PositionedItem>)> = display_lists
        .iter()
        .flat_map(|dl| dl.items.iter())
        .filter_map(|item| match item {
            DisplayListItem::TextLayout {
                layout,
                bounds: WindowLogicalRect(bounds),
                ..
            } => payload_items(layout).map(|items| (bounds.origin.y, items)),
            _ => None,
        })
        .collect();
    layouts.sort_by(|a, b| a.0.total_cmp(&b.0));
    layouts
}

/// Whether `items` hold a cluster of text `text`.
pub fn has_cluster(items: &[PositionedItem], text: &str) -> bool {
    items
        .iter()
        .any(|it| matches!(&it.item, ShapedItem::Cluster(c) if c.text() == text))
}

/// Where the first cluster of text `text` sits in `items` (its top left).
pub fn cluster_at(items: &[PositionedItem], text: &str) -> LogicalPosition {
    items
        .iter()
        .find_map(|it| match &it.item {
            ShapedItem::Cluster(c) if c.text() == text => {
                Some(LogicalPosition::new(it.position.x, it.position.y))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no cluster {text:?} in the layout"))
}
