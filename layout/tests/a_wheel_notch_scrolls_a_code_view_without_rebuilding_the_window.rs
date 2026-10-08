//! A wheel notch over a code view scrolls it without rebuilding the window:
//! the view re-renders its own lines - a `VirtualView` inside the view node -
//! and the app only keeps the view it is handed.
//!
//! Found by CODESCROLL13 (2026-10-08) on AzCode: "AzCode is heavily lagging
//! when scrolling in the code view, while AzWidgets (which has a similar, if
//! not larger Dom) is perfectly smooth." Measured headless (1280x800, 40
//! notches of 100 px over huge.rs): ONE FULL DOM REGENERATION PER NOTCH
//! (`dom_regenerations` 40 of 40; AzWidgets' page: 0 of 40) and ~49 ms of
//! every notch in `regenerate_layout` - the app's layout callback, the cascade
//! of the whole 777-node window, the reconciliation, the intrinsic sizes of
//! all of its nodes (730 of them per notch), the explorer's `VirtualView`
//! rendered again - to bring six lines into view. The view scrolled by
//! handing every notch to the app as a `Scroll` event, and the app did what
//! the view's contract asked ("store `event.view` and rebuild"): it rebuilt
//! the window to show the new top line. A browser scrolls a box without
//! touching the document, and a virtualized list - VSCode's editor, the
//! terminal view here - re-renders only its own rows.
//!
//! RED before the fix: the view node's children were its lines (no
//! `VirtualView`), and the wheel queued no `UpdateVirtualView` - the only way
//! the new top line ever reached the screen was the app's `RefreshDom`.

use std::sync::{Arc, Mutex};

use azul_core::{
    callbacks::Update,
    dom::{Dom, DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
    geom::{LogicalPosition, LogicalSize},
    gl::OptionGlContextPtr,
    refany::RefAny,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::RawWindowHandle,
    FastBTreeSet,
};
use azul_css::{system::SystemStyle, AzString};
use azul_layout::{
    callbacks::{Callback, CallbackChange, CallbackInfo, ExternalSystemCallbacks},
    widgets::code_view::{
        CodeView, CodeViewDataSourceCallbackType, CodeViewEvent, CodeViewEventKind, CodeViewLine,
        CodeViewOnEventCallbackType,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The view node's class (`code_view::VIEW_CLASS_NAME`).
const VIEW_CLASS: &str = "__azul-native-code-view";
/// The window: a 400 px view holds 21 whole 19 px lines and part of one more.
const WIN_W: f32 = 600.0;
const WIN_H: f32 = 400.0;

/// The text's data: nothing - every line is generated.
struct Text;

/// Line `line` of a thousand: `line <n>`.
extern "C" fn line_of(_data: RefAny, line: u32) -> CodeViewLine {
    CodeViewLine::create_plain(AzString::from(format!("line {line}")))
}

/// What the app heard from the view.
struct Heard {
    events: Arc<Mutex<Vec<CodeViewEvent>>>,
}

/// The app's answer to a scroll, as AzCode gives it: keep the view, rebuild
/// nothing.
extern "C" fn on_event(mut data: RefAny, _info: CallbackInfo, event: CodeViewEvent) -> Update {
    if let Some(heard) = data.downcast_ref::<Heard>() {
        heard.events.lock().expect("the log").push(event);
    }
    Update::DoNothing
}

/// A thousand-line code view filling the window, laid out once.
fn window(events: &Arc<Mutex<Vec<CodeViewEvent>>>) -> LayoutWindow {
    let view = CodeView::create(1000)
        .with_viewport(WIN_W, WIN_H)
        .with_data_source(RefAny::new(Text), line_of as CodeViewDataSourceCallbackType)
        .with_on_event(
            RefAny::new(Heard {
                events: events.clone(),
            }),
            on_event as CodeViewOnEventCallbackType,
        );
    let dom = Dom::create_body()
        .with_css("margin: 0; display: flex; flex-direction: column; height: 100%;")
        .with_child(view.dom());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIN_W, WIN_H);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .expect("the window lays out");
    lw
}

/// The root DOM's node wearing `class`.
fn node_with_class(lw: &LayoutWindow, class: &str) -> NodeId {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = sd.node_data.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .find(|n| {
            node_data[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some(s) if s == class))
        })
        .expect("harness: the window has the code view")
}

/// One wheel notch of `raw_dy` px over the view: the platform records the
/// delta for the pass (`ScrollManager::record_scroll_from_hit_test`), then
/// the view's `Scroll` handler runs the way the shell runs it.
fn wheel(lw: &mut LayoutWindow, view: NodeId, raw_dy: f32) -> (Vec<CallbackChange>, Update) {
    let (core_callback, mut data) = {
        let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
        let node_data = sd.node_data.as_container();
        let registered = node_data[view]
            .get_callbacks()
            .as_ref()
            .iter()
            .find(|cb| cb.event == EventFilter::Hover(HoverEventFilter::Scroll))
            .expect("harness: the view takes the wheel");
        (registered.callback.clone(), registered.refany.clone())
    };
    let mut callback = Callback::from_core(core_callback);
    lw.scroll_manager.pending_wheel_event = Some(LogicalPosition::new(0.0, raw_dy));
    let state = lw.current_window_state.clone();
    let hit = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(view)),
    };
    let result = lw.invoke_single_callback_at(
        hit,
        &mut callback,
        &mut data,
        &RawWindowHandle::Unsupported,
        &OptionGlContextPtr::None,
        Arc::new(SystemStyle::default()),
        &ExternalSystemCallbacks::rust_internal(),
        &None,
        &state,
        &RendererResources::default(),
    );
    lw.scroll_manager.pending_wheel_event = None;
    result
}

/// A queued `UpdateVirtualView` applied the way the shell's frame drain
/// applies it: that one view's callback runs again, nothing else.
fn apply_rerender(lw: &mut LayoutWindow, node: NodeId) {
    let mut set = FastBTreeSet::new();
    set.insert(node);
    let mut updates = std::collections::BTreeMap::new();
    updates.insert(DomId::ROOT_ID, set);
    lw.queue_virtual_view_updates(updates);
    let ws = lw.current_window_state.clone();
    let updated = lw.process_pending_virtual_view_updates(
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
    );
    assert_eq!(updated.len(), 1, "the view's lines were rendered again");
}

/// Every text the view's `VirtualView` shows, in document order.
fn shown_texts(lw: &LayoutWindow, lines_view: NodeId) -> Vec<String> {
    let nested = lw
        .virtual_view_manager
        .get_nested_dom_id(DomId::ROOT_ID, lines_view)
        .expect("the view's lines are a DOM of their own");
    let sd = &lw.layout_results[&nested].styled_dom;
    let nodes = sd.node_data.as_container();
    (0..nodes.len())
        .filter_map(
            |i| match nodes.get(NodeId::new(i)).map(|d| d.get_node_type()) {
                Some(NodeType::Text(t)) => Some(t.as_str().to_string()),
                _ => None,
            },
        )
        .collect()
}

#[test]
fn a_wheel_notch_scrolls_a_code_view_without_rebuilding_the_window() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut lw = window(&events);
    let view = node_with_class(&lw, VIEW_CLASS);

    // The view node hosts its lines' `VirtualView`: a DOM of their own, which
    // a scroll can render again without the window around it.
    let lines_view = {
        let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
        let child = sd.node_hierarchy.as_ref()[view.index()]
            .first_child_id(view)
            .expect("the view node has children");
        let node_data = sd.node_data.as_container();
        let child_data = &node_data[child];
        assert!(
            child_data.is_virtual_view_node(),
            "the view's lines are a VirtualView inside the view node - its first child is a {:?}, \
             so the only way a scroll reached the screen was a rebuild of the whole window",
            child_data.get_node_type()
        );
        child
    };
    let first_frame = shown_texts(&lw, lines_view);
    assert!(
        first_frame.iter().any(|t| t == "line 0"),
        "the first frame shows the first line: {first_frame:?}"
    );

    // One notch down: 95 px, five 19 px lines.
    let (changes, update) = wheel(&mut lw, view, -95.0);
    let heard = events.lock().expect("the log").clone();
    assert_eq!(heard.len(), 1, "the app hears the notch once: {heard:?}");
    assert_eq!(heard[0].kind, CodeViewEventKind::Scroll);
    let top = heard[0].view.top_line;
    assert!(
        (4..=6).contains(&top),
        "a 95 px notch scrolls five whole lines from the first: the top line is {top}"
    );
    assert_eq!(
        update,
        Update::DoNothing,
        "the app answers a scroll with no rebuild"
    );
    assert!(
        changes.iter().any(|c| matches!(
            c,
            CallbackChange::UpdateVirtualView { dom_id, node_id }
                if *dom_id == DomId::ROOT_ID && *node_id == lines_view
        )),
        "the view renders its own lines again - with the app rebuilding nothing for a scroll, \
         nothing else would move them; queued: {changes:?}"
    );

    // The frame drain renders the view's lines - and only them - again.
    apply_rerender(&mut lw, lines_view);
    let scrolled = shown_texts(&lw, lines_view);
    let new_top = format!("line {top}");
    assert!(
        scrolled.iter().any(|t| *t == new_top),
        "the view shows its new top line, {new_top}: {scrolled:?}"
    );
    assert!(
        !scrolled.iter().any(|t| t == "line 0"),
        "the line scrolled out of view is gone: {scrolled:?}"
    );
}
