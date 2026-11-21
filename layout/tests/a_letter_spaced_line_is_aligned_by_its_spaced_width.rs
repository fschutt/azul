//! A letter-spaced line is aligned by its spaced width.
//!
//! The flora tab probe's glyphs (the lead's run, 2026-10-10): the unselected "Memory" tab's
//! content box is 59.8 px wide - exactly its capitals' max-content width, six 12px capitals
//! tracked .08em (0.96 px after each) - and its label is `text-align: center`, so it should
//! start on the content box's left edge. It started 2.88 px in: half the 5.76 px its six
//! letter-spacings add. The selected tab's label sat 3.36 px right of centre: half of 7 x 0.96.
//!
//! text3 measures a line's width WITH its letter- and word-spacing where it breaks lines and
//! where it sizes a box to its max-content (`fold_line_width`,
//! `get_item_measure_with_spacing`), but `position_one_line` took the room left for
//! `text-align` from the bare advances, so a centred line went half its spacing to the right
//! and a right-aligned one all of it, out of its box.
//!
//! CSS Text 3 section 7.1 aligns a line's inline content - which includes the letter-spacing
//! after every character (section 10.1; Chrome adds it after the last one too, and counts it
//! in the max-content width) - in the line box. So in Chrome a centred or right-aligned
//! letter-spaced label in a box as wide as its max-content starts on the box's left edge, as a
//! left-aligned one does; in a box 300 px wide its spaced width W is centred: it starts
//! (300 - W) / 2 in.
//!
//! The paragraphs: "CAPITALS" at 16px, `letter-spacing: 4px` (32 px of spacing), as flex items
//! sized to their max-content (left, centre, right), and centred in a 300 px box. The left one's
//! box is the label's spaced width W.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn label(id: &str, css: &str) -> Dom {
    Dom::create_p_with_text("CAPITALS")
        .with_id(AzString::from(id))
        .with_css(&format!(
            "margin: 0px; padding: 0px; font-size: 16px; letter-spacing: 4px; {css}"
        ))
}

fn lay_out(dom: Dom) -> LayoutWindow {
    let styled = StyledDom::create_from_dom(dom);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 200.0);
    lw.current_window_state = ws.clone();
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .expect("the labels lay out");
    lw
}

fn node_of_id(lw: &LayoutWindow, id: &str) -> NodeId {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let index = result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .position(|n| n.has_id(id))
        .unwrap_or_else(|| panic!("no node #{id}"));
    NodeId::new(index)
}

fn rect_of(lw: &LayoutWindow, node: NodeId) -> LogicalRect {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    })
    .expect("the label is laid out")
}

/// The x of every glyph painted for the text inside the DOM node `p`.
fn glyph_xs(lw: &LayoutWindow, p: NodeId) -> Vec<f32> {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let hierarchy = result.styled_dom.node_hierarchy.as_container();
    let mut out = Vec::new();
    for item in result.display_list.items.iter() {
        if let DisplayListItem::Text {
            glyphs,
            source_node_index: Some(src),
            ..
        } = item
        {
            let Some(mut node) = result.layout_tree.nodes.get(*src).and_then(|n| n.dom_node_id)
            else {
                continue;
            };
            let mut inside = node == p;
            while !inside {
                match hierarchy[node].parent_id() {
                    Some(up) => {
                        node = up;
                        inside = node == p;
                    }
                    None => break,
                }
            }
            if inside {
                out.extend(glyphs.iter().map(|g| g.point.x));
            }
        }
    }
    out
}

/// The label `id`'s box and where its first glyph starts, from the box's left edge.
fn start_of(lw: &LayoutWindow, id: &str) -> (LogicalRect, f32, Vec<f32>) {
    let node = node_of_id(lw, id);
    let rect = rect_of(lw, node);
    let xs = glyph_xs(lw, node);
    assert_eq!(xs.len(), 8, "#{id} paints its eight capitals: {xs:?} in {rect:?}");
    let first = xs.iter().copied().fold(f32::INFINITY, f32::min);
    (rect, first - rect.origin.x, xs)
}

#[test]
fn a_letter_spaced_line_is_aligned_by_its_spaced_width() {
    let row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: flex-start; gap: 40px;")
        .with_child(label("left", "text-align: left;"))
        .with_child(label("centre", "text-align: center;"))
        .with_child(label("right", "text-align: right;"));
    let wide = label("wide", "text-align: center; width: 300px;");
    let lw = lay_out(
        Dom::create_body()
            .with_css("margin: 0px;")
            .with_child(row)
            .with_child(wide),
    );

    let (left, left_in, left_xs) = start_of(&lw, "left");
    let spaced_w = left.size.width;
    assert!(
        left_in.abs() < 0.5,
        "a left-aligned label starts on its box's edge: {left_in:.2} px in ({left:?}, glyph x \
         {left_xs:?})"
    );
    for id in ["centre", "right"] {
        let (rect, start, xs) = start_of(&lw, id);
        assert!(
            (rect.size.width - spaced_w).abs() < 0.5,
            "#{id} is as wide as the left one, the label's spaced max-content width \
             {spaced_w:.2}: {rect:?}"
        );
        assert!(
            start.abs() < 0.5,
            "#{id}'s box is as wide as its spaced line, so the line starts on its edge, not \
             {start:.2} px in ({} of its 32 px of letter-spacing): {rect:?}, glyph x {xs:?}",
            if id == "centre" { "half" } else { "all" }
        );
    }
    let (wide, wide_in, wide_xs) = start_of(&lw, "wide");
    let centred_in = (300.0 - spaced_w) / 2.0;
    assert!(
        (wide_in - centred_in).abs() < 0.5,
        "in a 300 px box the spaced line ({spaced_w:.2} px) is centred: it starts \
         {centred_in:.2} px in, not {wide_in:.2} ({wide:?}, glyph x {wide_xs:?})"
    );
}
