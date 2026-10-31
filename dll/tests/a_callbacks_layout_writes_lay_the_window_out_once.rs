//! A callback's layout-property writes lay the window out once.
//!
//! `CallbackInfo::set_css_property` of a layout property (a seek bar's fill
//! `width` and thumb `left` while a video plays, a panel's `display`) goes
//! through the content chokepoint. It laid the whole window out at once for
//! every write, and then returned the tier on which the shell lays the window
//! out again: a seek bar moved by two writes four times a second laid the
//! window out three times per move. Where the shell acts on the callbacks'
//! result (the user-event dispatch, the timers and threads:
//! `defer_content_layout`) the writes only write, and the one relayout the
//! result asks for lays them all out. Elsewhere (a lifecycle callback after a
//! rebuild, whose result nothing acts on) a write still lays out at once.

use std::{cell::RefCell, sync::Arc};

use azul::desktop::shell2::{
    common::{event::IncrementalRelayout, PlatformWindow},
    headless::HeadlessWindow,
};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    dom::{Dom, DomId, IdOrClass, IdOrClassVec, NodeData},
    events::ProcessEventResult,
    icon::{IconProviderHandle, SharedIconProvider},
    id::NodeId,
    refany::RefAny,
    resources::AppConfig,
};
use azul_css::props::{
    basic::PixelValue,
    layout::dimensions::LayoutWidth,
    property::{CssProperty, CssPropertyVec},
};
use azul_layout::{callbacks::CallbackChange, window_state::WindowCreateOptions};
use rust_fontconfig::FcFontCache;

/// Two boxes, `fill` and `thumb`, beside forty paragraphs.
extern "C" fn layout_cb(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let boxed = |class: &str| {
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class(class.into())]))
            .with_css("width: 10px; height: 10px;")
    };
    let mut body = Dom::create_body()
        .with_child(boxed("fill"))
        .with_child(boxed("thumb"));
    for i in 0..40 {
        body = body.with_child(Dom::create_p_with_text(format!("Paragraph {i}").as_str()));
    }
    body
}

fn make_window() -> HeadlessWindow {
    let mut options = WindowCreateOptions::default();
    options.window_state.layout_callback = LayoutCallback {
        cb: layout_cb,
        ctx: azul_core::refany::OptionRefAny::None,
    };
    HeadlessWindow::new(
        options,
        Arc::new(RefCell::new(RefAny::new(()))),
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        AppConfig::default(),
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::default()),
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

fn node_with_class(window: &HeadlessWindow, class: &str) -> NodeId {
    let lw = window
        .common
        .layout_window
        .as_ref()
        .expect("a layout window");
    let nodes = lw.layout_results[&DomId::ROOT_ID]
        .styled_dom
        .node_data
        .as_container();
    let index = nodes
        .iter()
        .position(|n: &NodeData| n.has_class(class))
        .unwrap_or_else(|| panic!("a node of class {class}"));
    NodeId::new(index)
}

fn width_write(node: NodeId, px: f32) -> CallbackChange {
    CallbackChange::ChangeNodeCssProperties {
        dom_id: DomId::ROOT_ID,
        node_id: node,
        properties: CssPropertyVec::from_vec(vec![CssProperty::const_width(LayoutWidth::Px(
            PixelValue::px(px),
        ))]),
    }
}

#[test]
fn a_callbacks_layout_writes_lay_the_window_out_once() {
    let mut window = make_window();
    window.regenerate_layout().expect("the page lays out");
    let fill = node_with_class(&window, "fill");
    let thumb = node_with_class(&window, "thumb");
    window
        .common
        .layout_window
        .as_mut()
        .expect("a layout window")
        .frame_report
        .reset_counters();

    // The changes of one callback, applied as the user-event dispatch and
    // the timers apply them.
    window.common.defer_content_layout = true;
    let tier = window
        .apply_user_change(&width_write(fill, 40.0))
        .max(window.apply_user_change(&width_write(thumb, 30.0)));
    window.common.defer_content_layout = false;

    assert_eq!(
        tier,
        ProcessEventResult::ShouldIncrementalRelayout,
        "the writes ask the shell for a relayout"
    );
    let passes = |window: &HeadlessWindow| {
        window
            .common
            .layout_window
            .as_ref()
            .expect("a layout window")
            .frame_report
            .layout_passes
    };
    assert_eq!(
        passes(&window),
        0,
        "each write laid the window out itself; the shell lays it out again for the tier"
    );

    // The one relayout the tier asks for lays both writes out.
    window
        .incremental_relayout_dispatching(IncrementalRelayout::Restyle, &mut None)
        .expect("the relayout");
    assert_eq!(passes(&window), 1);
    let lw = window
        .common
        .layout_window
        .as_ref()
        .expect("a layout window");
    let width_of = |node: NodeId| {
        lw.get_node_layout_rect(azul_core::dom::DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node)),
        })
        .map(|r| r.size.width)
    };
    assert_eq!(width_of(fill), Some(40.0), "the fill has its new width");
    assert_eq!(width_of(thumb), Some(30.0), "the thumb has its new width");
}

#[test]
fn a_layout_write_whose_result_nothing_acts_on_lays_out_at_once() {
    let mut window = make_window();
    window.regenerate_layout().expect("the page lays out");
    let fill = node_with_class(&window, "fill");
    let _ = window.apply_user_change(&width_write(fill, 40.0));
    let lw = window
        .common
        .layout_window
        .as_ref()
        .expect("a layout window");
    let width = lw
        .get_node_layout_rect(azul_core::dom::DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(fill)),
        })
        .map(|r| r.size.width);
    assert_eq!(
        width,
        Some(40.0),
        "the write is laid out before anything acts on its tier"
    );
}
