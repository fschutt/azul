//! A `<span>` that is a flex item is blockified (CSS Display 3, 2.7): it is
//! a block container, and its padding and border are its box's. Its width
//! must be its text plus its padding and border, counted once - as Chrome
//! lays out `<div style="display:flex"><span style="padding: 2px 9px">1</span></div>`.
//!
//! A page number pill in AzReview's page rail (a padded span in a flex row)
//! came out 18px too wide: the box counted the padding, and the text run
//! counted it again as if the span were still the inline box around it.
//! The text is placed once, so the surplus showed after the number.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 11px; width: 600px; }
    .row { display: flex; flex-direction: row; align-items: center; }
    .plain { flex-shrink: 0; }
    .pill { flex-shrink: 0; padding: 2px 9px; border: 1px solid black; }
    .item { flex-shrink: 0; }
"#;

/// The padding and border across a pill: 9 + 9 + 1 + 1.
const PILL_EDGES: f32 = 20.0;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn span(class_name: &str, text: &str) -> Dom {
    Dom::create_span_with_text(text).with_ids_and_classes(class(class_name))
}

fn layout(mut dom: Dom) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled_dom, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    lw
}

fn width_of(lw: &LayoutWindow, n: usize) -> f32 {
    let node = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    };
    lw.get_node_layout_rect(node)
        .unwrap_or_else(|| panic!("node {n} is laid out"))
        .size
        .width
}

#[test]
fn a_padded_span_in_a_flex_row_is_as_wide_as_its_text_and_its_padding() {
    // `body(0) > div.row(1) > [span.plain(2) > "1"(3), span.pill(4) > "1"(5)]`
    let lw = layout(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("row"))
                .with_child(span("plain", "1"))
                .with_child(span("pill", "1")),
        ),
    );
    let (plain, pill) = (width_of(&lw, 2), width_of(&lw, 4));
    assert!(plain > 0.0, "premise: the digit has a width");
    assert!(
        (pill - plain - PILL_EDGES).abs() < 0.5,
        "the pill is its digit ({plain}px) plus {PILL_EDGES}px of padding and border, got {pill}px"
    );
}

#[test]
fn a_padded_inline_span_inside_a_flex_item_counts_its_padding_once() {
    // The span stays an inline box here (its parent div is the flex item):
    // its padding sits on its text run, and the div is as wide as both.
    // `body(0) > div.row(1) > [div.item(2) > span.plain(3) > "1"(4),
    // div.item(5) > span.pill(6) > "1"(7)]`
    let lw = layout(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("row"))
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(class("item"))
                        .with_child(span("plain", "1")),
                )
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(class("item"))
                        .with_child(span("pill", "1")),
                ),
        ),
    );
    let (plain, pill) = (width_of(&lw, 2), width_of(&lw, 5));
    assert!(plain > 0.0, "premise: the digit has a width");
    assert!(
        (pill - plain - PILL_EDGES).abs() < 0.5,
        "the item is its digit ({plain}px) plus the span's {PILL_EDGES}px, got {pill}px"
    );
}
