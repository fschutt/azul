//! Regression pins for the caret-reveal / session-identity seam.
//!
//! Every one of these used to answer for the ROOT dom, or for a node it had
//! matched by bare arena index, or through an a11y-only copy of a path the
//! keyboard already had:
//!
//! - `find_scrollable_ancestor` walked `layout_cache.tree` (root dom only), so
//!   a node inside a virtualized view got the scroll container of whatever
//!   unrelated root node shared its index.
//! - `get_focused_cursor_rect_viewport` — the accessor all four native shells
//!   use to place the IME candidate window — did the same, anchored on the
//!   FOCUSED node rather than the editing session.
//! - The same editable opened by click, by focus and by a screen reader
//!   carried three different session keys (two of them a literal `0`).
//! - An assistive-technology focus revealed the caret through a private,
//!   glide-blind, zero-padded copy that called a caret clipped at the bottom
//!   edge "visible"; a node reveal went through a copy that adjusted exactly
//!   one scroll container.

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{AccessibilityAction, Dom, DomId, DomNodeId, IdOrClass, NodeId, OptionDom},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::{RendererResources, SystemAnimations},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    window::{LayoutWindow, ScrollMode, SelectionScrollType},
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn with_class(dom: Dom, class: &'static str) -> Dom {
    let ids: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class(class.into())].into();
    dom.with_ids_and_classes(ids)
}

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
}

fn dom_node(dom: DomId, node: usize) -> DomNodeId {
    DomNodeId {
        dom,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
    }
}

fn now() -> Instant {
    Instant::from(std::time::Instant::now())
}

fn laid_out(mut dom: Dom, css_src: &str) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(css_src);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.system_animations_override = Some(SystemAnimations::default());
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
    lw
}

/// Register a scroll container the way the shells' `register_scroll_nodes`
/// does after layout, and park it at `offset_y`.
fn seed_scroll(
    lw: &mut LayoutWindow,
    dom: DomId,
    node: usize,
    container_h: f32,
    content_h: f32,
    offset_y: f32,
) {
    let n = NodeId::new(node);
    lw.scroll_manager.update_node_bounds(
        dom,
        n,
        LogicalRect::new(
            LogicalPosition::zero(),
            LogicalSize::new(600.0, container_h),
        ),
        LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, content_h)),
        now(),
    );
    lw.scroll_manager.set_scroll_position_unclamped(
        dom,
        n,
        LogicalPosition::new(0.0, offset_y),
        now(),
    );
}

fn last_cluster_cursor(lw: &LayoutWindow, node: usize) -> azul_core::selection::TextCursor {
    last_cluster_cursor_in(lw, DomId::ROOT_ID, node)
}

/// The cursor at the very end of `node`'s inline layout — the one an a11y
/// focus seeds. Goes through the MATERIALIZED layout: the stored one may be
/// the retirement sentinel, whose `items` are empty.
fn last_cluster_cursor_in(
    lw: &LayoutWindow,
    dom: DomId,
    node: usize,
) -> azul_core::selection::TextCursor {
    let tree = &lw
        .get_layout_result(&dom)
        .expect("layout result")
        .layout_tree;
    let index = tree
        .dom_to_layout
        .get(&NodeId::new(node))
        .and_then(|indices| indices.first())
        .expect("the node has a layout box");
    tree.materialized_inline_layout_for_node(index.index())
        .expect("the node establishes an inline layout")
        .get_last_cluster_cursor()
        .expect("the node has clusters")
}

fn session_key(lw: &LayoutWindow) -> Option<u64> {
    lw.text_edit_manager
        .multi_cursor
        .as_ref()
        .map(|mc| mc.contenteditable_key)
}

/// The key `find_host_by_contenteditable_key` would resolve `node` by — the
/// oracle every session-opening path has to agree with.
fn host_key(lw: &LayoutWindow, dom: DomId, host: usize) -> u64 {
    let lr = lw.get_layout_result(&dom).expect("layout result");
    azul_core::diff::calculate_contenteditable_key(
        lr.styled_dom.node_data.as_ref(),
        lr.styled_dom.node_hierarchy.as_ref(),
        NodeId::new(host),
    )
}

// ---------------------------------------------------------------------------
// Fixture A: a contenteditable inside a VirtualView's nested DOM
// ---------------------------------------------------------------------------

// The nested arena, in document order:
//   root(0) > scroller(1) > [ sidebar(2) > text(3), editor(4) > text(5) ]
//
// `sidebar` is NOT an ancestor of `editor`, and its index (2) sits BETWEEN the
// editor's (4) and its real scroll container's (1). That is what makes the
// root-dom index walk answer a non-None WRONG node: walking the ROOT chain
// down from index 4 reaches index 2 before index 1.
const NESTED_SCROLLER: usize = 1;
const NESTED_SIDEBAR: usize = 2;
const NESTED_EDITOR: usize = 4;
const NESTED_TEXT: usize = 5;

/// The root arena is a plain chain deep enough to have a node at every nested
/// index the walk could reach: body(0) > w(1) > w(2) > w(3) > vvhost(4).
const ROOT_CSS: &str = "* { margin: 0; padding: 0; } \
                        body { font-size: 14px; } \
                        .w { display: block; } \
                        .vv { display: block; width: 600px; height: 500px; overflow: hidden; }";

extern "C" fn nested_view(_data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let mut editor = Dom::create_div().with_css("display: block; width: 280px;");
    editor.set_contenteditable(true);
    let long = "the caret sits far below the fold of its own container ".repeat(20);
    let editor = editor.with_child(text(long.as_str()));

    let sidebar = Dom::create_div()
        .with_css("display: block; width: 120px; height: 60px; overflow-y: scroll;")
        .with_child(text("sidebar"));

    let scroller = Dom::create_div()
        .with_css("display: block; width: 300px; height: 80px; overflow-y: scroll;")
        .with_child(sidebar)
        .with_child(editor);

    VirtualViewReturn {
        dom: OptionDom::Some(
            Dom::create_div()
                .with_css("display: block;")
                .with_child(scroller),
        ),
        materialized: LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 500.0)),
        virtual_rect: LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 500.0)),
    }
}

/// Returns the window and the nested `DomId` the VirtualView mounted.
fn nested_fixture() -> (LayoutWindow, DomId) {
    let host = with_class(
        Dom::create_virtual_view(RefAny::new(0u32), VirtualViewCallback::create(nested_view)),
        "vv",
    );
    let chain = with_class(Dom::create_div(), "w").with_child(
        with_class(Dom::create_div(), "w")
            .with_child(with_class(Dom::create_div(), "w").with_child(host)),
    );
    let dom = Dom::create_body().with_child(chain);

    let lw = laid_out(dom, ROOT_CSS);
    let nested = lw
        .virtual_view_manager
        .get_nested_dom_id(DomId::ROOT_ID, NodeId::new(4))
        .expect("the virtual view mounted a nested dom");
    assert!(
        lw.get_layout_result(&nested).is_some(),
        "premise: the nested dom has its own layout result"
    );
    assert!(
        lw.get_layout_result(&DomId::ROOT_ID)
            .expect("root layout result")
            .layout_tree
            .dom_to_layout
            .contains_key(&NodeId::new(NESTED_EDITOR)),
        "premise: the ROOT arena has a node at the editor's nested index — the \
         index collision the old walk resolved against"
    );
    (lw, nested)
}

#[test]
fn a_scrollable_ancestor_is_searched_in_the_nodes_own_dom() {
    let (mut lw, nested) = nested_fixture();
    seed_scroll(&mut lw, nested, NESTED_SCROLLER, 80.0, 900.0, 0.0);
    seed_scroll(&mut lw, nested, NESTED_SIDEBAR, 60.0, 400.0, 0.0);

    let ancestor = lw.find_scrollable_ancestor(dom_node(nested, NESTED_EDITOR));

    assert_eq!(
        ancestor,
        Some(dom_node(nested, NESTED_SCROLLER)),
        "the editor's scroll container is the node above it in ITS OWN dom, not \
         whichever registered scroll node the ROOT arena's ancestor chain \
         happened to name first — that answer is the sidebar, which is not an \
         ancestor of the editor at all"
    );
}

#[test]
fn the_ime_caret_rect_is_corrected_by_the_nested_doms_own_scroll() {
    const SCROLLER_OFFSET: f32 = 40.0;
    const SIDEBAR_OFFSET: f32 = 17.0;

    let (mut lw, nested) = nested_fixture();
    seed_scroll(
        &mut lw,
        nested,
        NESTED_SCROLLER,
        80.0,
        900.0,
        SCROLLER_OFFSET,
    );
    seed_scroll(&mut lw, nested, NESTED_SIDEBAR, 60.0, 400.0, SIDEBAR_OFFSET);

    // Focus lands on the contenteditable container, the session is anchored on
    // the text node inside it — the split that made anchoring on
    // `focus_manager` wrong in the first place.
    lw.focus_manager
        .set_focused_node(Some(dom_node(nested, NESTED_EDITOR)));
    lw.text_edit_manager.initialize_editing(
        azul_core::selection::TextCursor {
            cluster_id: azul_core::selection::GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: azul_core::selection::CursorAffinity::Leading,
        },
        nested,
        NodeId::new(NESTED_TEXT),
        0,
    );

    let absolute = lw
        .get_focused_cursor_rect()
        .expect("the session resolves a caret in its own dom");
    let viewport = lw
        .get_focused_cursor_rect_viewport()
        .expect("the IME accessor resolves the same caret");

    assert!(
        (viewport.origin.y - (absolute.origin.y - SCROLLER_OFFSET)).abs() < 0.01,
        "the IME rect must be the absolute caret minus the scroll of the \
         caret's OWN scroll ancestry ({SCROLLER_OFFSET}px), not minus every \
         registered scroll node the root arena's chain collided with \
         (got {:?}, absolute {:?})",
        viewport.origin,
        absolute.origin,
    );
    assert!(
        (viewport.origin.x - absolute.origin.x).abs() < 0.01,
        "nothing scrolled horizontally"
    );
}

#[test]
fn a_caret_reveal_inside_a_virtual_view_moves_the_nested_container() {
    let (mut lw, nested) = nested_fixture();
    seed_scroll(&mut lw, nested, NESTED_SCROLLER, 80.0, 900.0, 0.0);
    seed_scroll(&mut lw, nested, NESTED_SIDEBAR, 60.0, 400.0, 0.0);

    lw.focus_manager
        .set_focused_node(Some(dom_node(nested, NESTED_EDITOR)));
    lw.text_edit_manager.initialize_editing(
        last_cluster_cursor_in(&lw, nested, NESTED_EDITOR),
        nested,
        NodeId::new(NESTED_TEXT),
        0,
    );

    let caret = lw
        .get_focused_cursor_rect()
        .expect("the session resolves a caret");
    let container = lw
        .get_node_bounds(nested, NodeId::new(NESTED_SCROLLER))
        .expect("the nested scroll container has bounds");
    let container_y = container.origin.y as f32;
    let container_h = container.size.height as f32;
    assert!(
        caret.origin.y > container_y + container_h,
        "premise: the caret is below the nested container's box"
    );

    assert!(
        lw.scroll_selection_into_view(SelectionScrollType::Cursor, ScrollMode::Instant),
        "the caret is far below the nested scroller: a reveal must happen"
    );

    let queued = lw.scroll_manager.scroll_input_queue.take_all();
    let [input] = queued.as_slice() else {
        panic!("exactly one scroll input must be queued, got {queued:?}");
    };
    assert_eq!(
        (input.dom_id, input.node_id),
        (nested, NodeId::new(NESTED_SCROLLER)),
        "the glide has to be queued on the NESTED scroll container"
    );

    // Naming the container in the nested dom is only half the job: its
    // RECTANGLE has to be read from that dom too. Measuring it in the root
    // arena hands the reveal a box belonging to another element, and the
    // offset it computes leaves the caret outside the container it moved.
    const TOL: f32 = 0.5;
    let visible_top = container_y + input.delta.y;
    let visible_bottom = visible_top + container_h;
    assert!(
        caret.origin.y >= visible_top - TOL
            && caret.origin.y + caret.size.height <= visible_bottom + TOL,
        "after the reveal the caret (y {}..{}) must sit inside the NESTED \
         container's visible box (y {visible_top}..{visible_bottom}, from its \
         own dom's geometry at y {container_y} height {container_h})",
        caret.origin.y,
        caret.origin.y + caret.size.height,
    );
}

// ---------------------------------------------------------------------------
// Fixture B: session identity across the three ways a session opens
// ---------------------------------------------------------------------------

const EDITABLE_CSS: &str = "* { margin: 0; padding: 0; } \
                            body { font-size: 14px; width: 600px; } \
                            .p { display: block; }";

/// `body(0) > host(1)[contenteditable] > text(2)`
fn flat_editable() -> LayoutWindow {
    laid_out(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_contenteditable(true)
                .with_child(text("hello world")),
        ),
        EDITABLE_CSS,
    )
}

/// `body(0) > host(1)[contenteditable] > div.p(2) > text(3)` — the P-wrap
/// convention, where the click path keys on node 2 and the focus path lands on
/// node 3, but the HOST is node 1.
fn p_wrapped_editable() -> LayoutWindow {
    laid_out(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_contenteditable(true)
                .with_child(with_class(Dom::create_div(), "p").with_child(text("hello world"))),
        ),
        EDITABLE_CSS,
    )
}

#[test]
fn click_focus_and_assistive_tech_open_the_same_session_identity() {
    const HOST: usize = 1;
    const TEXT: usize = 2;

    let mut lw = flat_editable();
    let expected = host_key(&lw, DomId::ROOT_ID, HOST);
    assert_ne!(
        expected, 0,
        "premise: a real editable has a real stable key"
    );

    lw.process_mouse_click_for_selection(LogicalPosition::new(6.0, 6.0), 0)
        .expect("the click lands in the editable");
    let by_click = session_key(&lw).expect("the click opened a session");

    lw.text_edit_manager.clear_editing();
    lw.focus_manager.set_pending_contenteditable_focus(
        DomId::ROOT_ID,
        NodeId::new(HOST),
        NodeId::new(TEXT),
    );
    assert!(
        lw.finalize_pending_focus_changes(),
        "the focus path opened a session"
    );
    let by_focus = session_key(&lw).expect("the focus path opened a session");

    lw.text_edit_manager.clear_editing();
    lw.process_accessibility_action(
        DomId::ROOT_ID,
        NodeId::new(HOST),
        AccessibilityAction::Focus,
        now(),
    );
    let by_a11y = session_key(&lw).expect("the accessibility path opened a session");

    assert_eq!(
        by_click, expected,
        "the click path must key the session on the contenteditable HOST"
    );
    assert_eq!(
        by_focus, expected,
        "the same editable opened by FOCUS must carry the same session identity \
         as one opened by click — it used to carry a literal 0"
    );
    assert_eq!(
        by_a11y, expected,
        "the same editable opened by a SCREEN READER must carry the same \
         session identity — it used to carry a literal 0"
    );
}

#[test]
fn a_p_wrapped_editable_keys_its_session_on_the_host_not_the_ifc_root() {
    const HOST: usize = 1;
    const IFC_ROOT: usize = 2;
    const TEXT: usize = 3;

    let mut lw = p_wrapped_editable();
    let expected = host_key(&lw, DomId::ROOT_ID, HOST);
    let ifc_root_key = host_key(&lw, DomId::ROOT_ID, IFC_ROOT);
    assert_ne!(
        expected, ifc_root_key,
        "premise: the host and the IFC root inside it have different keys"
    );

    lw.process_mouse_click_for_selection(LogicalPosition::new(6.0, 6.0), 0)
        .expect("the click lands in the paragraph");
    let by_click = session_key(&lw).expect("the click opened a session");

    lw.text_edit_manager.clear_editing();
    lw.focus_manager.set_pending_contenteditable_focus(
        DomId::ROOT_ID,
        NodeId::new(HOST),
        NodeId::new(TEXT),
    );
    assert!(lw.finalize_pending_focus_changes());
    let by_focus = session_key(&lw).expect("the focus path opened a session");

    assert_eq!(
        by_click, expected,
        "only the HOST's key is the one `find_host_by_contenteditable_key` can \
         resolve — the IFC root is not contenteditable, so a session keyed on \
         it can never be found again"
    );
    assert_eq!(
        by_focus, by_click,
        "click and focus into the same editable are the same session"
    );
}

// ---------------------------------------------------------------------------
// Fixture C: the a11y reveals go through the canonical paths
// ---------------------------------------------------------------------------

const CLIP_CSS: &str = "* { margin: 0; padding: 0; } \
                        body { font-size: 16px; } \
                        .clip { display: block; width: 300px; height: 100px; overflow: auto; } \
                        .editor { display: block; width: 280px; }";

/// `body(0) > div.clip(1) > div.editor(2)[contenteditable] > text(3)`
fn clipped_editor() -> LayoutWindow {
    let mut editor = Dom::create_div();
    editor.set_contenteditable(true);
    let long = "line of text that wraps and wraps to make many lines ".repeat(20);
    laid_out(
        Dom::create_body().with_child(
            with_class(Dom::create_div(), "clip")
                .with_child(with_class(editor, "editor").with_child(text(long.as_str()))),
        ),
        CLIP_CSS,
    )
}

#[test]
fn an_assistive_technology_focus_reveals_a_bottom_clipped_caret() {
    const CLIP: usize = 1;
    const EDITOR: usize = 2;

    let mut lw = clipped_editor();

    // Where the a11y Focus action will put the caret: the end of the text.
    let at_end = last_cluster_cursor(&lw, EDITOR);
    lw.text_edit_manager
        .initialize_editing(at_end, DomId::ROOT_ID, NodeId::new(EDITOR), 0);
    let caret = lw
        .get_focused_cursor_rect()
        .expect("the caret resolves before the reveal");
    lw.text_edit_manager.clear_editing();

    // Park the 100px-tall clip so the caret's TOP edge is 96px down inside it —
    // inside by the old code's top-left-corner test, but hanging past the
    // bottom edge by its whole height.
    seed_scroll(
        &mut lw,
        DomId::ROOT_ID,
        CLIP,
        100.0,
        caret.origin.y + 400.0,
        caret.origin.y - 96.0,
    );
    assert!(
        caret.size.height > 5.0,
        "premise: the caret is taller than the reveal's 5px padding, so a caret \
         whose top is 96px into a 100px box really is clipped (got {:?})",
        caret.size
    );

    lw.process_accessibility_action(
        DomId::ROOT_ID,
        NodeId::new(EDITOR),
        AccessibilityAction::Focus,
        now(),
    );

    // The canonical path either queues an AnimateTo glide (a navigation JUMP)
    // or writes the offset directly (a small FOLLOW, which is this case: the
    // caret hangs past the bottom by roughly its own height, far less than the
    // half-container that separates a follow from a jump). Both are the
    // canonical path; the wrong outcome is neither.
    let moved = lw
        .scroll_manager
        .get_scroll_state(DomId::ROOT_ID, NodeId::new(CLIP))
        .is_some_and(|s| (s.current_offset.y - (caret.origin.y - 96.0)).abs() > 0.5);
    assert!(
        lw.scroll_manager.scroll_input_queue.has_pending() || moved,
        "an assistive-technology focus must reveal the caret exactly the way a \
         keyboard one does: through the canonical session-anchored path, which \
         pads by 5px and measures the caret's WHOLE box rather than testing its \
         top-left corner"
    );
}

/// A reveal that only has to FOLLOW the caret must land it immediately.
///
/// `calculate_instant_scroll_delta` is a minimal reveal — it moves the view
/// just far enough to clear the 5px padding — so following a caret asks for a
/// few pixels at a time. Gliding that puts the bounce spring between the caret
/// and the view: every keystroke pushes the caret out of sight and the field
/// then animates it back in, which is what "typing past the right edge makes
/// the text slide around under the cursor" looked like on a real device.
#[test]
fn a_small_caret_reveal_follows_immediately_instead_of_gliding() {
    const CLIP: usize = 1;
    const EDITOR: usize = 2;

    let mut lw = clipped_editor();
    let at_end = last_cluster_cursor(&lw, EDITOR);
    lw.text_edit_manager
        .initialize_editing(at_end, DomId::ROOT_ID, NodeId::new(EDITOR), 0);
    let caret = lw
        .get_focused_cursor_rect()
        .expect("the caret resolves before the reveal");

    // Clipped by about its own height — a follow, not a jump.
    let seeded = caret.origin.y - 96.0;
    seed_scroll(
        &mut lw,
        DomId::ROOT_ID,
        CLIP,
        100.0,
        caret.origin.y + 400.0,
        seeded,
    );

    assert!(
        lw.scroll_selection_into_view(SelectionScrollType::Cursor, ScrollMode::Instant),
        "premise: the caret really is clipped, so the reveal has work to do"
    );

    assert!(
        !lw.scroll_manager.scroll_input_queue.has_pending(),
        "a few-pixel follow must NOT be handed to the AnimateTo spring — that is \
         the 400ms lag between the caret and the view"
    );
    let landed = lw
        .scroll_manager
        .get_scroll_state(DomId::ROOT_ID, NodeId::new(CLIP))
        .expect("the clip has scroll state");
    assert!(
        (landed.current_offset.y - seeded).abs() > 0.5,
        "the follow must move the view NOW, not next frame: {} -> {}",
        seeded,
        landed.current_offset.y
    );
}

#[test]
fn an_assistive_technology_scroll_into_view_adjusts_the_whole_ancestry() {
    const OUTER: usize = 1;
    const INNER: usize = 2;
    const TARGET: usize = 4;

    let css = "* { margin: 0; padding: 0; } \
               body { font-size: 14px; } \
               .outer { display: block; width: 600px; height: 120px; overflow-y: scroll; } \
               .inner { display: block; width: 600px; height: 600px; overflow-y: scroll; } \
               .spacer { display: block; height: 900px; } \
               .target { display: block; height: 20px; }";
    let mut lw = laid_out(
        Dom::create_body().with_child(
            with_class(Dom::create_div(), "outer").with_child(
                with_class(Dom::create_div(), "inner")
                    .with_child(with_class(Dom::create_div(), "spacer"))
                    .with_child(with_class(Dom::create_div(), "target")),
            ),
        ),
        css,
    );

    seed_scroll(&mut lw, DomId::ROOT_ID, OUTER, 120.0, 600.0, 0.0);
    seed_scroll(&mut lw, DomId::ROOT_ID, INNER, 600.0, 920.0, 0.0);

    lw.process_accessibility_action(
        DomId::ROOT_ID,
        NodeId::new(TARGET),
        AccessibilityAction::ScrollIntoView,
        now(),
    );

    let inner_offset = lw
        .scroll_manager
        .get_current_offset(DomId::ROOT_ID, NodeId::new(INNER))
        .expect("the inner container has scroll state")
        .y;
    let outer_offset = lw
        .scroll_manager
        .get_current_offset(DomId::ROOT_ID, NodeId::new(OUTER))
        .expect("the outer container has scroll state")
        .y;

    assert!(
        inner_offset > 0.0,
        "the innermost scroll container must move (got {inner_offset})"
    );
    assert!(
        outer_offset > 0.0,
        "revealing a node nested in TWO scroll containers has to adjust BOTH — \
         moving only the inner one leaves the target as invisible as it was \
         (outer stayed at {outer_offset})"
    );
}
