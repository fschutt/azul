//! A dirty subtree re-solved on its own is laid out where, and against what,
//! the full layout laid it out.
//!
//! A dirty flex item is promoted up through its flex containers, so a restyle
//! deep inside nested flex boxes becomes a layout root at the first ancestor
//! that is not a flex item — typically the `<body>`. That root used to be laid
//! out from its PARENT's content-box origin and against its parent's final used
//! size. Under an `<html>` that also holds a menu bar, both are wrong: the body
//! sits below the bar and inside its own margin, and the html's used height
//! includes what the body overflows by. In AzWidgets every frame of a switch
//! knob's glide moved the whole page up by the menu bar and left by the body's
//! margin, and grew the body by both.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId, NodeType},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{
    layout::LayoutWidth,
    property::{CssProperty, CssPropertyType},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, overlay::ContentChange, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn relayout(lw: &mut LayoutWindow) {
    let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        result.styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();
}

fn find(lw: &LayoutWindow, matches: impl Fn(&azul_core::dom::NodeData) -> bool) -> NodeId {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = sd.node_data.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .find(|n| matches(&node_data[*n]))
        .expect("the node exists")
}

fn with_class(lw: &LayoutWindow, class: &str) -> NodeId {
    find(lw, |nd| {
        nd.get_ids_and_classes()
            .iter()
            .any(|c| c.as_class() == Some(class))
    })
}

fn rect(lw: &LayoutWindow, node: NodeId) -> LogicalRect {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: Some(node).into(),
    })
    .expect("laid out")
}

#[test]
fn a_restyle_deep_in_nested_flex_boxes_keeps_the_page_where_the_full_layout_put_it() {
    // A menu bar above the app's body, the body a flex column filling the
    // window, and flex containers all the way down to the restyled box.
    let body = Dom::create_body()
        .with_css("display: flex; flex-direction: column; height: 100%;")
        .with_child(Dom::create_div().with_css("height: 38px; flex-shrink: 0;"))
        .with_child(
            Dom::create_div()
                .with_class("content".into())
                .with_css(
                    "display: flex; flex-direction: column; flex-grow: 1; overflow-y: auto; \
                     padding: 24px;",
                )
                .with_child(
                    Dom::create_div()
                        .with_class("card".into())
                        .with_css("display: flex; flex-direction: column; padding: 18px;")
                        .with_child(
                            Dom::create_div()
                                .with_class("track".into())
                                .with_css("display: flex; width: 40px; height: 24px;")
                                .with_child(
                                    Dom::create_div()
                                        .with_class("knob".into())
                                        .with_css("width: 16px; height: 16px;"),
                                ),
                        ),
                ),
        );
    let mut dom = Dom::create_html()
        .with_child(
            Dom::create_div()
                .with_class("menubar".into())
                .with_css("height: 26px;"),
        )
        .with_child(body);
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();

    // A window lays out many times before anyone clicks: the pass under test
    // is a warm incremental one, not the first after a cold layout.
    relayout(&mut lw);

    let body = find(&lw, |nd| matches!(nd.get_node_type(), NodeType::Body));
    let [menubar, content, card, track, knob] =
        ["menubar", "content", "card", "track", "knob"].map(|c| with_class(&lw, c));
    let page = |lw: &LayoutWindow| [menubar, body, content, card, track].map(|n| rect(lw, n));
    let before = page(&lw);
    assert!(
        before[1].origin.y >= 26.0,
        "harness: the body sits below the menu bar, at {:?}",
        before[1]
    );

    // Resize the knob inside its fixed-size track: nothing outside the track
    // has a reason to move or change size. Written the way an animation frame
    // writes (`tick_animations`): an override, plus the knob staged as layout
    // dirt. No inline-style change for the reconcile to notice, so the dirty
    // knob alone decides what is re-solved.
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: knob,
        props: vec![CssProperty::width(LayoutWidth::px(12.0))],
        override_only: true,
    });
    lw.pending_css_dirty = Some((
        DomId::ROOT_ID,
        vec![(knob, CssPropertyType::Width.relayout_scope(true))],
    ));
    relayout(&mut lw);

    assert!(
        (rect(&lw, knob).size.width - 12.0).abs() < 0.5,
        "harness: the relayout applied the restyle, the knob is {:?}",
        rect(&lw, knob)
    );
    let after = page(&lw);
    for ((name, was), is) in ["menu bar", "body", "content", "card", "track"]
        .iter()
        .zip(before)
        .zip(after)
    {
        assert_eq!(
            is, was,
            "the {name} moved or resized when only the knob changed: {was:?} -> {is:?}"
        );
    }
}
