//! A marked status-bar segment is a live label: text-sized on its first
//! frame, rewritten in place by `StatusBar::update_segment_label` from any
//! callback that can find it (`get_node_id_by_marker`), and re-laid out at
//! the new text's width without a `RefreshDom` or a full relayout.
//!
//! The contract under test is the engine side of "the word count keeps up
//! with typing": a widget label that can be updated by marker, cheaply, and
//! whose box follows its content (`measure_dom_shrink_to_fit` behind a
//! content-sized `VirtualView`).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId, NodeType},
    geom::{LogicalRect, LogicalSize, OptionLogicalPosition},
    gl::OptionGlContextPtr,
    hit_test::ScrollPosition,
    refany::OptionRefAny,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{MonitorVec, RawWindowHandle},
    FastBTreeSet,
};
use azul_css::system::SystemStyle;
use azul_css::AzString;
use azul_layout::{
    callbacks::{CallbackChange, CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks},
    widgets::statusbar::{StatusBar, StatusBarSegment, StatusBarSegmentVec},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const MARKER: &str = "words-7f3a";

/// A status bar with a static segment and a marked one, laid out once.
fn window_with_bar(label: &str) -> LayoutWindow {
    let bar = StatusBar::new(StatusBarSegmentVec::from_vec(vec![
        StatusBarSegment::new(AzString::from("Page 1 of 3")),
        StatusBarSegment::new(AzString::from(label)).with_marker(AzString::from(MARKER)),
    ]));
    let mut dom = Dom::create_body().with_child(bar.dom());
    let (css, _) = azul_css::parser2::new_from_str("body { margin: 0; }");
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state.clone();
    layout(&mut lw, styled_dom, &window_state);
    lw
}

fn layout(lw: &mut LayoutWindow, styled_dom: StyledDom, window_state: &FullWindowState) {
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
}

/// The node in the root dom that carries `MARKER` - what
/// `CallbackInfo::get_node_id_by_marker` finds, read straight from the
/// styled dom here so the lookup and the widget can be checked separately.
fn marked_node(lw: &LayoutWindow) -> NodeId {
    let lr = lw.layout_results.get(&DomId::ROOT_ID).expect("root laid out");
    let nodes = lr.styled_dom.node_data.as_container();
    (0..nodes.len())
        .map(NodeId::new)
        .find(|n| {
            nodes
                .get(*n)
                .and_then(|d| d.get_marker())
                .is_some_and(|m| m.as_str() == MARKER)
        })
        .expect("the marked segment is in the dom")
}

fn dom_node(node: NodeId) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    }
}

fn rect_of(lw: &LayoutWindow, node: NodeId) -> LogicalRect {
    lw.get_node_layout_rect(dom_node(node)).expect("the node has a rect")
}

/// The text the marked segment's view currently renders (its nested dom's
/// text nodes, concatenated).
fn rendered_label(lw: &LayoutWindow, view: NodeId) -> String {
    let nested = lw
        .virtual_view_manager
        .get_nested_dom_id(DomId::ROOT_ID, view)
        .expect("the label view mounted a nested dom");
    let lr = lw.layout_results.get(&nested).expect("nested dom laid out");
    let nodes = lr.styled_dom.node_data.as_container();
    (0..nodes.len())
        .filter_map(|i| match nodes.get(NodeId::new(i)).map(|d| d.get_node_type()) {
            Some(NodeType::Text(t)) => Some(t.as_str().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Run `f` against a `CallbackInfo` over the laid-out window, the way a
/// user callback sees it, and return what it queued.
fn with_info<R>(
    lw: &LayoutWindow,
    hit: DomNodeId,
    f: impl FnOnce(&mut CallbackInfo) -> R,
) -> (R, Vec<CallbackChange>) {
    let renderer_resources = RendererResources::default();
    let previous_window_state: Option<FullWindowState> = None;
    let current_window_state = lw.current_window_state.clone();
    let gl_context = OptionGlContextPtr::None;
    let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
        BTreeMap::new();
    let window_handle = RawWindowHandle::Unsupported;
    let system_callbacks = ExternalSystemCallbacks::rust_internal();

    let ref_data = CallbackInfoRefData {
        layout_window: lw,
        renderer_resources: &renderer_resources,
        previous_window_state: &previous_window_state,
        current_window_state: &current_window_state,
        gl_context: &gl_context,
        current_scroll_manager: &scroll_states,
        current_window_handle: &window_handle,
        system_callbacks: &system_callbacks,
        system_style: Arc::new(SystemStyle::default()),
        monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
        #[cfg(feature = "icu")]
        icu_localizer: azul_layout::icu::IcuLocalizerHandle::default(),
        ctx: OptionRefAny::None,
    };
    let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
    let mut info = CallbackInfo::new(
        &ref_data,
        &changes,
        hit,
        OptionLogicalPosition::None,
        OptionLogicalPosition::None,
    );
    let out = f(&mut info);
    let queued = info.take_changes();
    (out, queued)
}

/// Apply a queued `UpdateVirtualView` the way the shell's drain does.
fn apply_rerender(lw: &mut LayoutWindow, node: NodeId) {
    let mut set = FastBTreeSet::new();
    set.insert(node);
    let mut updates = BTreeMap::new();
    updates.insert(DomId::ROOT_ID, set);
    lw.queue_virtual_view_updates(updates);
    let window_state = lw.current_window_state.clone();
    let updated = lw.process_pending_virtual_view_updates(
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
    );
    assert_eq!(updated.len(), 1, "the queued label was re-invoked");
}

#[test]
fn a_marked_segment_label_is_text_sized_on_the_first_frame() {
    let lw = window_with_bar("0 WORDS");
    let view = marked_node(&lw);
    assert_eq!(rendered_label(&lw, view), "0 WORDS", "the view renders the segment text");

    let rect = rect_of(&lw, view);
    assert!(
        rect.size.width > 20.0 && rect.size.width < 200.0,
        "the label is as wide as its text, not the 300px replaced-element default: {rect:?}"
    );
    assert!(
        rect.size.height > 8.0 && rect.size.height < 23.0,
        "one line of 11px text, inside the 23px bar: {rect:?}"
    );
    assert_eq!(
        lw.frame_report.virtual_view_size_passes, 1,
        "the reported size costs exactly one extra pass"
    );
}

#[test]
fn update_segment_label_rewrites_the_text_in_place_at_its_new_width() {
    let mut lw = window_with_bar("0 WORDS");
    let view = marked_node(&lw);
    let narrow = rect_of(&lw, view).size.width;
    let layouts_before = lw.frame_report.layout_passes;
    let size_passes_before = lw.frame_report.virtual_view_size_passes;

    // A callback anywhere finds the label by its marker and rewrites it.
    let (found, _) = with_info(&lw, dom_node(NodeId::ZERO), |info| {
        info.get_node_id_by_marker(AzString::from(MARKER))
    });
    assert_eq!(found, Some(dom_node(view)), "the marker names the label's view node");

    let (updated, changes) = with_info(&lw, dom_node(NodeId::ZERO), |info| {
        StatusBar::update_segment_label(info, dom_node(view), AzString::from("12345 WORDS"))
    });
    assert!(updated, "the node is a live marked label");
    assert_eq!(changes.len(), 1, "one re-render, nothing else");
    assert!(
        matches!(
            changes[0],
            CallbackChange::UpdateVirtualView { dom_id, node_id }
                if dom_id == DomId::ROOT_ID && node_id == view
        ),
        "the queued change names the label that has to re-render, got {:?}",
        changes[0]
    );

    apply_rerender(&mut lw, view);
    assert_eq!(rendered_label(&lw, view), "12345 WORDS");
    let wide = rect_of(&lw, view).size.width;
    assert!(
        wide > narrow + 10.0,
        "the box follows the longer text ({narrow} -> {wide}), it is not clipped to the old width"
    );
    assert_eq!(
        lw.frame_report.layout_passes, layouts_before,
        "an in-place label update is not a full relayout"
    );
    assert_eq!(
        lw.frame_report.virtual_view_size_passes,
        size_passes_before + 1,
        "the new width costs one size pass"
    );

    // Shorter again: shrinks, never stays at the widest text it ever had.
    let (updated, _) = with_info(&lw, dom_node(NodeId::ZERO), |info| {
        StatusBar::update_segment_label(info, dom_node(view), AzString::from("1 WORD"))
    });
    assert!(updated);
    apply_rerender(&mut lw, view);
    assert_eq!(rendered_label(&lw, view), "1 WORD");
    assert!(rect_of(&lw, view).size.width < wide - 10.0, "shrunk to the shorter text");
}

#[test]
fn update_segment_label_with_the_same_text_queues_nothing() {
    let lw = window_with_bar("0 WORDS");
    let view = marked_node(&lw);
    // Called on every keystroke, an unchanged count must not damage the bar.
    let (updated, changes) = with_info(&lw, dom_node(NodeId::ZERO), |info| {
        StatusBar::update_segment_label(info, dom_node(view), AzString::from("0 WORDS"))
    });
    assert!(updated, "the label is live even when unchanged");
    assert!(changes.is_empty(), "no re-render for an identical text");
}

#[test]
fn update_segment_label_leaves_a_node_that_is_not_a_marked_label_alone() {
    let lw = window_with_bar("0 WORDS");
    // The body, and a node that does not exist: no-ops, not panics.
    for target in [dom_node(NodeId::ZERO), dom_node(NodeId::new(999))] {
        let (updated, changes) = with_info(&lw, dom_node(NodeId::ZERO), |info| {
            StatusBar::update_segment_label(info, target, AzString::from("x"))
        });
        assert!(!updated, "{target:?} is not a marked label");
        assert!(changes.is_empty());
    }
}

#[test]
fn an_unmarked_segment_stays_a_plain_paragraph() {
    // No marker, no view: the static segment keeps the `div > p > text` shape
    // and mounts nothing that could be re-rendered.
    let bar = StatusBar::new(StatusBarSegmentVec::from_vec(vec![StatusBarSegment::new(
        AzString::from("Page 1 of 3"),
    )]));
    let mut dom = Dom::create_body().with_child(bar.dom());
    let (css, _) = azul_css::parser2::new_from_str("body { margin: 0; }");
    let styled_dom = StyledDom::create(&mut dom, css);
    let nodes = styled_dom.node_data.as_container();
    let views = (0..nodes.len())
        .filter(|i| matches!(nodes.get(NodeId::new(*i)).map(|d| d.get_node_type()), Some(NodeType::VirtualView)))
        .count();
    assert_eq!(views, 0);
}
