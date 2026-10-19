//! `display: inline flow-root list-item` (CSS Display 3 s2.4) is an
//! inline-block that is a list item: it sits in its line like an
//! inline-block, counts in its list's `list-item` counter, and carries its
//! marker in the block inside it (WPT css-lists/inline-block-list, whose
//! reference wraps a block list item in an inline-block). The value did not
//! parse, so the declaration was dropped and the items stayed block list
//! items, one per line.

use azul_core::dom::DomId;
use azul_layout::solver3::layout_tree::PseudoElement;

use crate::table_markup::{laid_out, near, node, rect};

fn page(body: &str) -> azul_layout::window::LayoutWindow {
    laid_out(
        &format!(
            "<html><head><style>li {{ display: inline flow-root list-item; width: 60px; \
             border: 1px solid; }}</style></head><body style=\"margin: 0 0 0 40px; font-size: \
             16px; line-height: 20px\">{body}</body></html>"
        ),
        800.0,
        600.0,
    )
}

#[test]
fn inline_flow_root_list_item_parses_in_any_keyword_order() {
    use azul_css::props::layout::parse_layout_display;
    let value = parse_layout_display("inline flow-root list-item").expect("a display");
    assert_eq!(parse_layout_display("list-item inline flow-root"), Ok(value));
    assert_eq!(parse_layout_display("flow-root list-item inline"), Ok(value));
    assert!(value.is_inline_level() && value.is_atomic_inline(), "{value:?}");
}

#[test]
fn inline_flow_root_list_items_share_a_line_and_carry_markers() {
    let lw = page(r#"<ol style="margin: 0; padding: 0"><li id="a">A</li><li id="b">B</li></ol>"#);
    let a = rect(&lw, "a");
    let b = rect(&lw, "b");
    assert!(near(a.origin.y, b.origin.y, 0.5), "one line: {a:?} {b:?}");
    assert!(b.origin.x > a.origin.x + 60.0, "side by side: {a:?} {b:?}");
    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    let tree = &result.layout_tree;
    for id in ["a", "b"] {
        let dom = node(&lw, id).node.into_crate_internal().expect("a DOM node");
        let markers = tree.dom_to_layout.get(&dom).map_or(0, |boxes| {
            boxes
                .iter()
                .filter(|&&b| {
                    tree.warm(b)
                        .is_some_and(|w| w.pseudo_element == Some(PseudoElement::Marker))
                })
                .count()
        });
        assert_eq!(markers, 1, "#{id} carries its marker");
    }
}
