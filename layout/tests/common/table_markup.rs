//! Shared helpers for the table layout tests (TABLE_A): lay a piece of XHTML
//! markup out in a window of a given size and read back element rects and
//! display-list items by element id.
//!
//! Box sizes in these tests come from fixed-size inline-blocks
//! (`<i style="display: inline-block; width: 100px; height: 10px"></i>`)
//! wherever a number is asserted, so the results do not depend on the fonts
//! of the machine; text is only used where the assertion is about wrapping.

use azul_core::{
    dom::{DomId, DomNodeId, IdOrClass, NodeData, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::{DisplayListItem, WindowLogicalRect},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `markup` parsed by the XML loader, styled and laid out in a
/// `width` x `height` window.
pub fn laid_out(markup: &str, width: f32, height: f32) -> LayoutWindow {
    let parsed = azul_layout::xml::parse_xml(markup).expect("the markup parses");
    let styled = StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed));
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(width, height);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the markup lays out");
    lw
}

/// `<html><head></head><body style="margin: 0">{body}</body></html>` laid
/// out in an 800 x 600 window.
pub fn body(body: &str) -> LayoutWindow {
    laid_out(
        &format!("<html><head></head><body style=\"margin: 0\">{body}</body></html>"),
        800.0,
        600.0,
    )
}

/// The element with `id`.
pub fn node(lw: &LayoutWindow, id: &str) -> DomNodeId {
    let sd = &lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom;
    let n = sd
        .node_data
        .as_ref()
        .iter()
        .position(|nd: &NodeData| {
            nd.get_ids_and_classes()
                .iter()
                .any(|c| matches!(c, IdOrClass::Id(s) if s.as_str() == id))
        })
        .unwrap_or_else(|| panic!("no element with id {id}"));
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// The border-box rect of the element with `id` (its placed position and
/// its used size). Panics if the element has no size: every box the tests
/// ask for is expected to have one.
pub fn rect(lw: &LayoutWindow, id: &str) -> LogicalRect {
    let n = node(lw, id);
    let size = lw
        .get_node_size(n)
        .unwrap_or_else(|| panic!("#{id} has no size (no box)"));
    let origin = lw
        .get_node_position(n)
        .unwrap_or_else(|| panic!("#{id} has no position"));
    LogicalRect::new(origin, size)
}

/// Every display-list item of the root DOM.
pub fn items(lw: &LayoutWindow) -> Vec<DisplayListItem> {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .items
        .clone()
}

/// The bounds of every solid `Rect` painted in exactly `color` (r, g, b).
pub fn rects_of_color(lw: &LayoutWindow, (r, g, b): (u8, u8, u8)) -> Vec<LogicalRect> {
    items(lw)
        .into_iter()
        .filter_map(|item| match item {
            DisplayListItem::Rect {
                bounds: WindowLogicalRect(bounds),
                color,
                ..
            } if color.r == r && color.g == g && color.b == b && color.a > 0 => Some(bounds),
            _ => None,
        })
        .collect()
}

/// The bounds of every `Border` item.
pub fn borders(lw: &LayoutWindow) -> Vec<LogicalRect> {
    items(lw)
        .into_iter()
        .filter_map(|item| match item {
            DisplayListItem::Border {
                bounds: WindowLogicalRect(bounds),
                ..
            } => Some(bounds),
            _ => None,
        })
        .collect()
}

/// `a` and `b` within `tolerance`.
pub fn near(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance
}

/// A fixed-size inline box: content whose min- and max-content widths are
/// both exactly `w` px, whatever the fonts.
pub fn block(w: u32) -> String {
    format!("<i style=\"display: inline-block; width: {w}px; height: 10px\"></i>")
}

/// `n` words of prose: a max-content of several hundred px per ten words
/// and a min-content of one word, whatever the font - for the tests that
/// only need "wider than the container" and "narrower than it".
///
/// Not [`words`]: the intrinsic min-content of inline-blocks separated by
/// whitespace-only text came out as the SUM of the boxes on the parent's
/// run (2026-10-01: `words(10, 100)` measured 1000 px, `words(10, 50)`
/// 500 px, spaces included in neither), so a table of them had no smaller
/// minimum to shrink to (scripts/TABLE_A_2026_10_01.md, "engine findings").
pub fn prose(n: usize) -> String {
    const WORDS: [&str; 10] = [
        "lorem",
        "ipsum",
        "dolor",
        "sit",
        "amet",
        "consectetur",
        "adipiscing",
        "elit",
        "sed",
        "eiusmod",
    ];
    (0..n)
        .map(|i| WORDS[i % WORDS.len()])
        .collect::<Vec<_>>()
        .join(" ")
}

/// `n` fixed-size inline boxes of `w` px separated by spaces: min-content
/// `w`, max-content about `n * w` (plus the spaces) in a browser.
pub fn words(n: usize, w: u32) -> String {
    (0..n).map(|_| block(w)).collect::<Vec<_>>().join(" ")
}

/// The right edge of a rect.
pub fn right(r: &LogicalRect) -> f32 {
    r.origin.x + r.size.width
}

/// The bottom edge of a rect.
pub fn bottom(r: &LogicalRect) -> f32 {
    r.origin.y + r.size.height
}

/// Every text run's glyph pens `(x, y)` (baseline points), in paint order.
pub fn glyph_runs(lw: &LayoutWindow) -> Vec<Vec<(f32, f32)>> {
    items(lw)
        .into_iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } if !glyphs.is_empty() => {
                Some(glyphs.iter().map(|g| (g.point.x, g.point.y)).collect())
            }
            _ => None,
        })
        .collect()
}
