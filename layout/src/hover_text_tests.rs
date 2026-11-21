//! A hover over a label's text restyles paint, and nothing else.
//!
//! The map editor's label is an absolutely placed, turned span with its text and a hover
//! block last (`:hover { background }`). The pointer enters the text and the span around it
//! (the hover manager enters every node of the hit chain): the restyle that follows
//! reports the span's background and nothing for the text, in the scope `None` - a repaint,
//! no layout. (What the shells made of that report is the dll's: its paint restyles rebuild
//! the touched DOM's list and ask for a repaint.)

use azul_core::{
    dom::Dom,
    id::NodeId,
    styled_dom::{HoverChange, RestyleResult, StyledDom},
};
use azul_css::props::{
    basic::color::ColorU,
    property::{CssPropertyType, RelayoutScope},
};

/// The editor's label: an absolutely placed, turned span with its text, a hover block last.
fn label(text: &str, halo: bool, background: bool) -> Dom {
    let halo = if halo {
        "text-shadow: 0px 0px 0px 2px #ffffff;"
    } else {
        ""
    };
    let background = if background { "background: #ffee99;" } else { "" };
    let css = format!(
        "position: absolute; left: 20px; top: 30px; transform-origin: 0px 9px; \
         transform: rotate(12deg); font-size: 12px; line-height: 1; letter-spacing: 0px; \
         color: #222222; font-family: sans-serif; white-space: pre; cursor: pointer; \
         {halo} {background} :hover {{ background: rgba(66,134,244,0.35); }}"
    );
    Dom::create_span_with_text(text).with_css(&css)
}

/// The restyle of the pointer entering `entered` of `body > span > text`.
fn enter(halo: bool, background: bool, entered: &[usize]) -> RestyleResult {
    let mut dom = Dom::create_body().with_child(label("Main Street", halo, background));
    let mut styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    assert_eq!(styled.node_data.as_container().len(), 3, "harness: body, span, text");
    styled.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: Vec::new(),
            entered_nodes: entered.iter().map(|n| NodeId::new(*n)).collect(),
        }),
        None,
    )
}

#[test]
fn a_hover_over_a_labels_text_is_a_repaint_of_its_background() {
    for (halo, background) in [(false, false), (true, false), (false, true), (true, true)] {
        // The hit chain: the text, the span, the body.
        let r = enter(halo, background, &[2, 1, 0]);
        assert!(
            !r.needs_layout && r.max_relayout_scope == RelayoutScope::None,
            "a colour is not a layout (halo {halo}, background {background}): {:?}",
            r.max_relayout_scope
        );
        assert!(r.needs_display_list, "but it is a repaint");
        assert!(!r.gpu_only_changes, "and not a value the GPU holds");
        let nodes: Vec<usize> = r.changed_nodes.keys().map(|n| n.index()).collect();
        assert_eq!(nodes, vec![1], "only the span restyles: the text and the body hold no rule");
        for change in &r.changed_nodes[&NodeId::new(1)] {
            let t = change.current_prop.get_type();
            assert!(
                matches!(t, CssPropertyType::BackgroundContent | CssPropertyType::BackgroundColor),
                "the span's background and nothing else: {t:?}"
            );
        }
    }
}

#[test]
fn the_hover_colour_replaces_the_labels_own_background() {
    let r = enter(false, true, &[1]);
    let change = r.changed_nodes[&NodeId::new(1)]
        .iter()
        .find(|c| c.current_prop.get_type() == CssPropertyType::BackgroundColor)
        .expect("the background colour changed");
    let hover = ColorU::new(66, 134, 244, 89);
    assert!(
        format!("{:?}", change.current_prop).contains(&format!("{hover:?}")),
        "to the hover colour: {:?}",
        change.current_prop
    );
}

#[test]
fn the_text_alone_restyles_nothing() {
    let r = enter(true, true, &[2]);
    assert!(r.changed_nodes.is_empty(), "{:?}", r.changed_nodes);
    assert!(!r.needs_display_list && !r.needs_layout);
}
