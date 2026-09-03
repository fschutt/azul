//! Icons through the layout pipeline: the ONE path that resolves them, and the
//! view that can swap one at runtime.
//!
//! There are exactly two producers of an application DOM — the `layout()`
//! callback and a VirtualView callback — plus the headless measure path that
//! sizes items for the second. Only the first resolved icons, because only the
//! shell's `regenerate_layout` had a `SharedIconProvider` in scope; the others
//! cascaded with a bare `StyledDom::create_from_dom`. An `<icon>` inside a
//! virtual view therefore stayed an `Icon` node forever — nothing downstream
//! resolves one after the cascade — and an item containing an icon measured as
//! though the icon were not there.
//!
//! `LayoutWindow` now HOLDS the provider (an `Arc` inside, shared with the app
//! and the tray) and `LayoutWindow::style_user_dom` is the single path from a
//! user `Dom` to a `StyledDom`. These tests never touch the shell, so they only
//! pass while the resolution lives on the window.

use std::sync::{Arc, Mutex};

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, DomNodeId, NodeData, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalPosition},
    gl::OptionGlContextPtr,
    hit_test::ScrollPosition,
    icon::{IconProviderHandle, SharedIconProvider},
    refany::{OptionRefAny, RefAny},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{MonitorVec, RawWindowHandle},
};
use azul_css::system::SystemStyle;
use azul_layout::{
    callbacks::{CallbackChange, CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Prefix of the text a resolved icon carries. The spec follows it, so a test
/// can tell WHICH icon rendered, not merely that one did.
const MARK: &str = "resolved:";

/// The size the resolver gives every icon, in px - kept in step by hand with
/// the literal in [`marker_resolver`]'s stylesheet, which cannot interpolate.
/// Deliberately not 300x150 (a replaced element's default) and not the window
/// size, so a measurement can only match it by actually resolving.
const ICON_PX: f32 = 40.0;

/// Resolves any icon to a fixed-size block naming the spec it resolved.
/// Deliberately a SUBTREE, not one node: a single-node icon would also survive
/// the old flatten-to-root path, so it could not tell the two apart.
extern "C" fn marker_resolver(
    _icon_data: OptionRefAny,
    original_icon_node: &NodeData,
    _system_style: &SystemStyle,
) -> Dom {
    let name = match original_icon_node.get_node_type() {
        NodeType::Icon(n) => n.as_str().to_string(),
        _ => String::from("<not-an-icon>"),
    };
    Dom::create_div()
        .with_css("display: block; width: 40px; height: 40px;")
        // A span, not a bare text-in-a-div: the a11y lint rightly objects to
        // the latter, and its warning would be printed on every render here.
        .with_child(Dom::create_span_with_text(format!("{MARK}{name}")))
}

fn provider() -> SharedIconProvider {
    let mut handle = IconProviderHandle::with_resolver(marker_resolver);
    handle.register_icon("test", "home", RefAny::new(0u32));
    handle.register_icon("test", "settings", RefAny::new(1u32));
    SharedIconProvider::from_handle(handle)
}

fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("LayoutWindow::new");
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;
    lw.set_icon_provider(provider());
    lw
}

/// `(surviving Icon nodes, specs that resolved)` in one laid-out DOM.
fn icons_and_resolved(lw: &LayoutWindow, dom_id: DomId) -> (usize, Vec<String>) {
    let lr = lw
        .get_layout_result(&dom_id)
        .expect("dom has a layout result");
    let node_data = lr.styled_dom.node_data.as_container();
    let mut icons = 0;
    let mut resolved = Vec::new();
    for i in 0..node_data.len() {
        match node_data[NodeId::new(i)].get_node_type() {
            NodeType::Icon(_) => icons += 1,
            NodeType::Text(t) => {
                if let Some(spec) = t.as_str().strip_prefix(MARK) {
                    resolved.push(spec.to_string());
                }
            }
            _ => {}
        }
    }
    (icons, resolved)
}

fn lay_out(lw: &mut LayoutWindow, mut dom: Dom) {
    let (css, _) = azul_css::parser2::new_from_str("* { margin: 0; padding: 0; }");
    let styled_dom = StyledDom::create(&mut dom, css);
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .expect("layout");
}

/// Re-invoke every view IN PLACE on the existing DOM — no relayout, no DOM
/// rebuild. This is what the shell's `drain_virtual_view_updates` does after a
/// callback queued an `UpdateVirtualView`, and it is the whole point of the
/// fast path: re-laying out instead would build a NEW DOM with a NEW dataset,
/// which is a different thing entirely (and would quietly pass a test that the
/// swap never reached).
fn rerender_views_in_place(lw: &mut LayoutWindow) {
    lw.queue_all_virtual_view_reinvoke();
    let window_state = lw.current_window_state.clone();
    let updated = lw.process_pending_virtual_view_updates(
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
    );
    assert!(!updated.is_empty(), "a view re-materialized");
}

// ── The VirtualView path ────────────────────────────────────────────────────

/// What a hand-written view renders, so a test can change it between passes.
type SharedSpec = Arc<Mutex<String>>;

extern "C" fn spec_view(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let spec = data.downcast_ref::<SharedSpec>().expect("spec").clone();
    let spec = spec.lock().unwrap().clone();
    let rect = LogicalRect::new(
        LogicalPosition::zero(),
        LogicalSize::new(ICON_PX, ICON_PX),
    );
    VirtualViewReturn::with_dom(
        Dom::create_div()
            .with_css("display: block;")
            .with_child(Dom::create_icon(spec.as_str())),
        rect,
        rect,
    )
}

fn hand_written_view(spec: &SharedSpec) -> Dom {
    Dom::create_body().with_child(
        Dom::create_virtual_view(
            RefAny::new(spec.clone()),
            VirtualViewCallback::create(spec_view),
        )
        .with_css("width: 200px; height: 200px; overflow: hidden;"),
    )
}

#[test]
fn an_icon_inside_a_virtual_view_resolves() {
    let mut lw = window();
    let spec: SharedSpec = Arc::new(Mutex::new(String::from("home")));
    lay_out(&mut lw, hand_written_view(&spec));

    let nested = lw
        .virtual_view_manager
        .get_nested_dom_id(DomId::ROOT_ID, NodeId::new(1))
        .expect("the virtual view mounted a nested dom");

    let (icons, resolved) = icons_and_resolved(&lw, nested);
    assert_eq!(icons, 0, "no Icon node may survive inside a virtual view");
    assert_eq!(resolved, vec![String::from("home")]);
}

#[test]
fn re_rendering_a_virtual_view_resolves_the_new_icon() {
    // Resolution is on the PATH, not something the first materialization did
    // once: change what the view renders and the new spec resolves too.
    let mut lw = window();
    let spec: SharedSpec = Arc::new(Mutex::new(String::from("home")));
    lay_out(&mut lw, hand_written_view(&spec));

    let nested = lw
        .virtual_view_manager
        .get_nested_dom_id(DomId::ROOT_ID, NodeId::new(1))
        .expect("the virtual view mounted a nested dom");
    assert_eq!(icons_and_resolved(&lw, nested).1, vec![String::from("home")]);

    *spec.lock().unwrap() = String::from("settings");
    rerender_views_in_place(&mut lw);

    let (icons, resolved) = icons_and_resolved(&lw, nested);
    assert_eq!(icons, 0);
    assert_eq!(resolved, vec![String::from("settings")]);
}

// ── The measure path ────────────────────────────────────────────────────────

#[test]
fn measure_dom_measures_the_resolved_icon_not_the_empty_node() {
    // `measure_dom` is how a VirtualView callback sizes an item DOM. It also
    // cascaded without resolving, so an item containing an icon measured as
    // though the icon were not there — and was then laid out WITH it, at a
    // different size than the measurement promised.
    let lw = window();
    let available = LogicalSize::new(1000.0, 1_000_000.0);
    let item = || {
        Dom::create_div()
            .with_css("display: block;")
            .with_child(Dom::create_icon("home"))
    };

    let measured = lw.measure_dom(item(), available);
    assert!(
        measured.height >= ICON_PX,
        "the resolver's {ICON_PX}px block has to be in the measurement, got {measured:?}"
    );

    // Control: the same window with no provider cannot resolve, so it measures
    // the bare icon node. Were the two equal, the assertion above would pass
    // for a reason that has nothing to do with resolution.
    let mut bare = LayoutWindow::new(FcFontCache::build()).expect("LayoutWindow::new");
    bare.current_window_state = lw.current_window_state.clone();
    let unresolved = bare.measure_dom(item(), available);
    assert!(
        unresolved.height < measured.height,
        "unresolved {unresolved:?} must be smaller than resolved {measured:?}"
    );
}

#[test]
fn a_window_without_a_provider_cascades_unchanged() {
    // Headless windows and apps that registered no icons must still lay out;
    // an unresolved icon is an empty inline, not a panic and not a lost tree.
    let lw = LayoutWindow::new(FcFontCache::build()).expect("LayoutWindow::new");
    let styled = lw.style_user_dom(
        Dom::create_body()
            .with_child(Dom::create_icon("home"))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "text",
            )),
    );
    let node_data = styled.node_data.as_container();
    assert!(
        (0..node_data.len())
            .any(|i| matches!(node_data[NodeId::new(i)].get_node_type(), NodeType::Icon(_))),
        "with no provider the icon node survives as itself"
    );
}

// ── The swappable icon view ─────────────────────────────────────────────────

fn view_page(spec: &str) -> Dom {
    Dom::create_body()
        .with_child(Dom::create_icon_view(spec).with_css("width: 24px; height: 24px;"))
}

#[test]
fn an_icon_view_renders_the_icon_it_was_given() {
    let mut lw = window();
    lay_out(&mut lw, view_page("home"));

    let nested = lw
        .virtual_view_manager
        .get_nested_dom_id(DomId::ROOT_ID, NodeId::new(1))
        .expect("the icon view mounted a nested dom");
    let (icons, resolved) = icons_and_resolved(&lw, nested);
    assert_eq!(icons, 0);
    assert_eq!(resolved, vec![String::from("home")]);
}

#[test]
fn an_icon_view_reports_the_icons_measured_size() {
    // The view measures its own icon through the window (which resolves it),
    // so `width: auto` on a view can mean what it means on an `<img>`. Pinning
    // the REPORT rather than the box: the box only follows on the next pass,
    // by design — a view is sized from the outside first and by its content
    // afterwards, which is the only order that terminates.
    let mut lw = window();
    lay_out(
        &mut lw,
        Dom::create_body().with_child(Dom::create_icon_view("home").with_css("display: block;")),
    );

    let sizes = lw.virtual_view_manager.materialized_sizes();
    let reported = sizes
        .get(&(DomId::ROOT_ID, NodeId::new(1)))
        .copied()
        .expect("the view reported a materialized size");
    assert!(
        (reported.width - ICON_PX).abs() < 1.0 && (reported.height - ICON_PX).abs() < 1.0,
        "expected the icon's own {ICON_PX}px, got {reported:?}"
    );
}

/// Runs `f` with a `CallbackInfo` over `lw` - the same construction the
/// engine does before invoking a callback - and returns the changes it queued
/// alongside `f`'s value. The window is borrowed immutably for the duration,
/// which is why applying those changes happens after this returns.
fn with_info<R>(
    lw: &LayoutWindow,
    hit: DomNodeId,
    f: impl FnOnce(&mut CallbackInfo) -> R,
) -> (R, Vec<CallbackChange>) {
    let renderer_resources = RendererResources::default();
    let previous_window_state: Option<FullWindowState> = None;
    let current_window_state = lw.current_window_state.clone();
    let gl_context = OptionGlContextPtr::None;
    let scroll_states: std::collections::BTreeMap<
        DomId,
        std::collections::BTreeMap<NodeHierarchyItemId, ScrollPosition>,
    > = std::collections::BTreeMap::new();
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

fn node(id: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(id))),
    }
}

#[test]
fn set_icon_swaps_what_the_view_renders() {
    let mut lw = window();
    lay_out(&mut lw, view_page("home"));

    // The swap the window controls need: rewrite the spec the view owns and
    // queue a re-render of THAT view - one change, naming that one node.
    let (swapped, changes) = with_info(&lw, node(1), |info| {
        info.set_icon(node(1), "settings".into())
    });
    assert!(swapped, "the node is a live icon view");
    assert_eq!(changes.len(), 1, "one re-render, nothing else");
    assert!(
        matches!(
            changes[0],
            CallbackChange::UpdateVirtualView { dom_id, node_id }
                if dom_id == DomId::ROOT_ID && node_id == NodeId::new(1)
        ),
        "the queued change names the view that has to re-render, got {:?}",
        changes[0]
    );

    // Setting the SAME spec queues nothing: an unchanged glyph must not damage
    // its box on every hover tick.
    let (again, changes) = with_info(&lw, node(1), |info| {
        info.set_icon(node(1), "settings".into())
    });
    assert!(!again);
    assert!(changes.is_empty());

    // And the view now renders the new spec. (Applying the queued change is
    // the shell's `drain_virtual_view_updates`; re-invoking directly is the
    // same thing minus the shell.)
    rerender_views_in_place(&mut lw);
    let nested = lw
        .virtual_view_manager
        .get_nested_dom_id(DomId::ROOT_ID, NodeId::new(1))
        .expect("the icon view mounted a nested dom");
    let (icons, resolved) = icons_and_resolved(&lw, nested);
    assert_eq!(icons, 0);
    assert_eq!(
        resolved,
        vec![String::from("settings")],
        "the view renders the spec its dataset now holds, not the one it was built with"
    );
}

#[test]
fn set_icon_leaves_a_node_that_is_not_an_icon_view_alone() {
    // A foreign dataset, a node with none, and a node that does not exist all
    // have to be no-ops rather than panics or half-applied swaps: `set_icon`
    // takes a `DomNodeId` from the app, which can name anything.
    let mut lw = window();
    lay_out(&mut lw, view_page("home"));

    for target in [node(0), node(2), node(999)] {
        let (swapped, changes) = with_info(&lw, node(1), |info| {
            info.set_icon(target, "settings".into())
        });
        assert!(!swapped, "{target:?} is not an icon view");
        assert!(changes.is_empty(), "and nothing may be queued for it");
    }
}
