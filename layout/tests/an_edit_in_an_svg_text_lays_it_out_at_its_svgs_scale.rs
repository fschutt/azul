//! An edit in an editable SVG `<text>` lays it out at its `<svg>`'s scale,
//! as a fresh layout of the edited text does.
//!
//! An SVG text's sizes are its `<svg>`'s user units, and its layout scales
//! them by the viewBox onto the `<svg>`'s box (`solver3::svg`): drawn at
//! twice its viewBox, a 20-unit font is 40 px. A keystroke lays the edited
//! text out again, and its letters have to stand where a fresh window puts
//! them, at the same scale.

use azul_core::{
    dom::{DomId, DomNodeId, NodeId, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

use crate::an_edit_in_a_vertical_paragraph_lays_out_as_a_fresh_layout_does::same_layout;

/// A window that has laid out an `<svg>` drawn at twice its viewBox, with
/// an editable `<text>` holding `text`, once.
fn window(text: &str) -> LayoutWindow {
    window_at(text, 400)
}

/// A window that has laid out an `<svg>` `width` px wide with a 200-unit
/// viewBox, with an editable `<text>` holding `text`, once.
fn window_at(text: &str, width: u32) -> LayoutWindow {
    let height = width / 2;
    let markup = format!(
        "<html><body style=\"margin: 0px\"><svg width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 200 100\"><text contenteditable=\"true\" x=\"10\" y=\"50\" \
         font-size=\"20\">{text}</text></svg></body></html>"
    );
    let parsed = azul_layout::xml::parse_xml(&markup).expect("the markup parses");
    let mut dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw
}

/// The `<text>` element.
fn svg_text(lw: &LayoutWindow) -> NodeId {
    let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|&id| {
            matches!(
                styled.node_data.as_container()[id].get_node_type(),
                NodeType::SvgText
            )
        })
        .expect("the page has an SVG text")
}

/// The `<text>`'s letters: line, text and where its inline layout puts them.
fn items_of(lw: &LayoutWindow) -> Vec<(usize, String, f32, f32)> {
    let text = svg_text(lw);
    let tree = &lw.layout_results[&DomId::ROOT_ID].layout_tree;
    let layout = (0..tree.nodes.len())
        .filter(|&index| tree.nodes[index].dom_node_id == Some(text))
        .find_map(|index| tree.materialized_inline_layout_for_node(index))
        .expect("the SVG text has an inline layout");
    layout
        .items
        .iter()
        .filter_map(|item| {
            let cluster = item.item.as_cluster()?;
            Some((
                item.line_index,
                cluster.text().to_string(),
                item.position.x,
                item.position.y,
            ))
        })
        .collect()
}

/// Types `letter` in front of the `<text>`'s characters.
fn type_in_front(lw: &mut LayoutWindow, letter: &str) {
    let text = svg_text(lw);
    lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(text)),
    }));
    let at_start = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 0,
        },
        affinity: CursorAffinity::Leading,
    };
    lw.start_editing_at(at_start, DomId::ROOT_ID, text, 0);
    let _ = lw.record_text_input(letter);
    let _ = lw.apply_text_changeset();
}

#[test]
fn typing_in_an_svg_text_keeps_its_svgs_scale() {
    let fresh = items_of(&window("bHello"));
    let unscaled = items_of(&window_at("bHello", 200));
    fn last_x(items: &[(usize, String, f32, f32)]) -> f32 {
        items.iter().map(|item| item.2).fold(0.0_f32, f32::max)
    }
    assert!(
        last_x(&fresh) > 1.5 * last_x(&unscaled),
        "harness: drawn at twice its viewBox the text is laid out twice as wide: {fresh:?}, \
         {unscaled:?}"
    );

    let mut lw = window("Hello");
    type_in_front(&mut lw, "b");
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, svg_text(&lw));
    assert_eq!(
        lw.extract_text_from_inline_content(&content),
        "bHello",
        "harness: the letter is typed in"
    );
    let got = items_of(&lw);
    assert!(
        same_layout(&got, &fresh),
        "after an edit the SVG text's letters stand at {got:?}, a fresh layout of the same text \
         puts them at {fresh:?}: the edit laid the text out in its user units"
    );
}
