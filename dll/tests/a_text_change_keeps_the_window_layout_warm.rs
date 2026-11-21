//! An app's text change re-lays out that text, not the window from scratch.
//!
//! `CallbackInfo::change_node_text` is how a clock, a counter or a video's
//! time label moves - four times a second while a video plays. The desktop
//! shell answered each one by dropping the solver's whole warm layout and
//! rebuilding the display list: a cold layout of every node of the window for
//! one label. The layout's reconcile already finds the text whose content
//! changed and re-shapes exactly its paragraph, which the E2E runner has
//! relied on since text changes are patched per paragraph.

use std::{cell::RefCell, sync::Arc};

use azul::desktop::shell2::{common::PlatformWindow, headless::HeadlessWindow};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    dom::{Dom, DomId, DomNodeId, NodeType},
    events::ProcessEventResult,
    icon::{IconProviderHandle, SharedIconProvider},
    id::NodeId,
    refany::RefAny,
    resources::AppConfig,
    styled_dom::NodeHierarchyItemId,
};
use azul_layout::{callbacks::CallbackChange, window_state::WindowCreateOptions};
use rust_fontconfig::FcFontCache;

/// A time label above forty paragraphs.
extern "C" fn layout_cb(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let mut body = Dom::create_body().with_child(Dom::create_p_with_text("0:00"));
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

#[test]
fn a_text_change_keeps_the_window_layout_warm() {
    let mut window = make_window();
    window.regenerate_layout().expect("the page lays out");

    let label = {
        let lw = window
            .common
            .layout_window
            .as_ref()
            .expect("a layout window");
        assert!(
            lw.layout_cache.tree.is_some(),
            "harness: the layout is warm"
        );
        let nodes = lw.layout_results[&DomId::ROOT_ID]
            .styled_dom
            .node_data
            .as_container();
        let index = nodes
            .iter()
            .position(|n| matches!(n.get_node_type(), NodeType::Text(t) if t.as_str() == "0:00"))
            .expect("the time label");
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }
    };

    let result = window.apply_user_change(&CallbackChange::ChangeNodeText {
        node_id: label,
        text: "0:01".into(),
    });

    assert_eq!(
        result,
        ProcessEventResult::ShouldIncrementalRelayout,
        "the new text is laid out"
    );
    let lw = window
        .common
        .layout_window
        .as_ref()
        .expect("a layout window");
    assert!(
        lw.layout_cache.tree.is_some(),
        "a text change dropped the window's warm layout: the next pass lays every node out cold \
         for one label"
    );
}
