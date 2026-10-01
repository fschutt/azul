//! A chat field keeps its width while text is typed into it.
//!
//! The user, on the Mac: "the <input> in the chat must not resize". AzMeet's chat is a row of a
//! real `TextInput` and a `Send` button in the call's side panel; FB1 fixed one cause of a field
//! that "forgets to stretch" (a memoised final layout served after a measure rewrote it). Here
//! the row is laid out empty, then again - in the same window, as a keystroke rebuilds it - with
//! a message far longer than the field, then empty again: the field's width, the button's place
//! and the row's height must not move. Once with the field as any app writes it (`flex-grow:
//! 1`: the text field is a scroll container, so its automatic minimum width is 0 and its content
//! never widens it), once with AzMeet's own style (a zero flex basis).
//!
//! Not compiled by the author (house rule); expected GREEN - a verification of the engine.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{button::Button, text_input::TextInput},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const LONG: &str = "This message is a good deal longer than the chat field is wide, so it \
                    scrolls inside the field instead of pushing the Send button out of the panel";

/// The side panel's chat row: the field (`field_css`) holding `text`, and Send.
fn chat_row(text: &str, field_css: &str) -> StyledDom {
    let mut dom = Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px;")
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center; width: 280px; padding: 8px;")
                .with_id("row".into())
                .with_child(
                    TextInput::create()
                        .with_text(text.into())
                        .with_placeholder("Message everyone".into())
                        .dom()
                        .with_css(field_css)
                        .with_id("field".into()),
                )
                .with_child(Button::create("Send".into()).dom().with_id("send".into())),
        );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

/// Lays `dom` out in `lw` at 800 x 600.
fn lay_out(lw: &mut LayoutWindow, dom: StyledDom) {
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the row lays out");
}

/// The laid-out border box of the node with the id `id`.
fn rect(lw: &LayoutWindow, id: &str) -> LogicalRect {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let index = result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .position(|node| node.has_id(id))
        .unwrap_or_else(|| panic!("no node #{id}"));
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    })
    .unwrap_or_else(|| panic!("#{id} has no layout rect"))
}

fn check(field_css: &str) {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut seen = Vec::new();
    for text in ["", "Hi", LONG, ""] {
        lay_out(&mut lw, chat_row(text, field_css));
        seen.push((text.len(), rect(&lw, "field"), rect(&lw, "send"), rect(&lw, "row")));
    }
    let (_, field0, send0, row0) = seen[0].clone();
    assert!(field0.size.width > 100.0, "{field_css}: the field takes the row: {field0:?}");
    for (len, field, send, row) in &seen {
        assert!(
            (field.size.width - field0.size.width).abs() < 0.5
                && (field.size.height - field0.size.height).abs() < 0.5,
            "{field_css}: with {len} bytes typed the field is {:?}, empty it was {:?}",
            field.size,
            field0.size
        );
        assert!(
            (send.origin.x - send0.origin.x).abs() < 0.5,
            "{field_css}: with {len} bytes typed Send moved from x {} to {}",
            send0.origin.x,
            send.origin.x
        );
        assert!(
            send.origin.x + send.size.width <= row.origin.x + row.size.width + 0.5,
            "{field_css}: with {len} bytes typed Send left its row: {send:?} in {row:?}"
        );
        assert!(
            (row.size.height - row0.size.height).abs() < 0.5,
            "{field_css}: with {len} bytes typed the row's height moved"
        );
    }
}

#[test]
fn a_growing_text_field_keeps_its_width_while_text_is_typed() {
    check("flex-grow: 1; margin-right: 6px;");
}

#[test]
fn azmeets_chat_field_keeps_its_width_while_text_is_typed() {
    check("flex-grow: 1; flex-shrink: 1; flex-basis: 0px; min-width: 0px; margin-right: 6px;");
}
