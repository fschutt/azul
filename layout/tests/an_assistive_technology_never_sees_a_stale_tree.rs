//! An assistive technology never sees a stale accessibility tree.
//!
//! The platform adapter answers a screen reader from the tree it was last
//! handed, and it hands the screen reader's requests back with the ids of
//! that tree. So the tree must follow everything that moves what a screen
//! reader sees (a scroll nobody announced, a keystroke), and a request that
//! names a node the DOM no longer has must be answered as "gone": it must
//! never park a focus on it or reach for it by index. Historically such
//! desyncs were the crashes on invalid ids.
#![cfg(feature = "a11y")]

use std::time::Duration;

use accesskit::{NodeId as A11yNodeId, TreeUpdate};
use accesskit_consumer::{Tree, TreeChangeHandler};
use azul_core::{
    dom::{AccessibilityAction, Dom, DomId, DomNodeId, IdOrClass, NodeId, TabIndex},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    selection::TextCursor,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, widgets::text_area::TextArea, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

struct Noop;
impl TreeChangeHandler for Noop {
    fn node_added(&mut self, _: &accesskit_consumer::Node<'_>) {}
    fn node_updated(&mut self, _: &accesskit_consumer::Node<'_>, _: &accesskit_consumer::Node<'_>) {
    }
    fn focus_moved(
        &mut self,
        _: Option<&accesskit_consumer::Node<'_>>,
        _: Option<&accesskit_consumer::Node<'_>>,
    ) {
    }
    fn node_removed(&mut self, _: &accesskit_consumer::Node<'_>) {}
}

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn para(text: &str) -> Dom {
    Dom::create_p().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text))
}

fn dnid(dom: usize, node: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId { inner: dom },
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
    }
}

fn now() -> Instant {
    Instant::from(std::time::Instant::now())
}

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { width: 600px; height: 400px; font-size: 14px; }
    .list { height: 200px; overflow-y: auto; }
    .item { height: 40px; }
"#;

fn new_window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 400.0);
    lw.current_window_state = ws;
    lw
}

/// Lay `dom` out in `lw`, the way a shell does after a `RefreshDom`.
fn show(lw: &mut LayoutWindow, mut dom: Dom) {
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let ws = lw.current_window_state.clone();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .unwrap();
    azul_layout::managers::scroll_registration::register_scroll_nodes(lw, &now());
}

/// Hand whatever is parked to the REAL consumer, as a shell does; a consumer
/// panic (an invalid id, a dangling focus) fails the test.
fn deliver(lw: &mut LayoutWindow, tree: &mut Option<Tree>) -> Option<TreeUpdate> {
    let update = lw.a11y_manager.take_pending()?;
    match tree {
        None => *tree = Some(Tree::new(update.clone(), true)),
        Some(t) => t.update_and_process_changes(update.clone(), &mut Noop),
    }
    Some(update)
}

/// `body(0) > div.list(1) > 12 x p.item(2 + 2k) > "item k"(3 + 2k)`.
const LIST: usize = 1;

fn list_page() -> Dom {
    let mut list = Dom::create_div().with_ids_and_classes(class("list"));
    for k in 0..12 {
        list = list.with_child(para(&format!("item {k}")).with_ids_and_classes(class("item")));
    }
    Dom::create_body().with_child(list)
}

// =========================================================================
// A scroll nobody announced
// =========================================================================

#[test]
fn a_scroll_nobody_announced_still_reaches_the_screen_reader() {
    let mut lw = new_window();
    show(&mut lw, list_page());
    lw.update_a11y_tree();
    let interval = Duration::from_millis(100);
    let t0 = std::time::Instant::now();
    assert!(
        !lw.a11y_rebuild_after_scroll_due(t0, interval),
        "nothing scrolled since the last pass: nothing is due"
    );

    // An offset written the way a scroll animation's tick, a focus reveal or
    // a keyboard page-down writes it: straight into the scroll manager, with
    // no `ScrollTo` change to flag the tree.
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        NodeId::new(LIST),
        LogicalPosition::new(0.0, 80.0),
        now(),
    );
    assert!(
        lw.a11y_rebuild_after_scroll_due(t0 + interval, interval),
        "a moved offset makes the scroll rebuild due: the bounds the screen reader holds are \
         stale until it runs"
    );
    lw.update_a11y_tree();
    assert!(
        !lw.a11y_rebuild_after_scroll_due(t0 + interval * 2, interval),
        "once rebuilt, the same offset is not due again"
    );
}

// =========================================================================
// A keystroke
// =========================================================================

/// body(0) > container(1) > label-p(2) > text(3).
const CONTAINER: usize = 1;
const LABEL_P: usize = 2;

fn end_of_text_cursor(lw: &LayoutWindow) -> TextCursor {
    let tree = &lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("layout result")
        .layout_tree;
    let idx = tree
        .dom_to_layout
        .get(&NodeId::new(LABEL_P))
        .and_then(|v| v.first())
        .expect("label <p> has a layout box");
    tree.materialized_inline_layout_for_node(idx.index())
        .expect("label <p> establishes an inline layout")
        .get_last_cluster_cursor()
        .expect("the label has at least one cluster")
}

#[test]
fn a_keystroke_republishes_the_edited_field_the_way_the_full_tree_builds_it() {
    let mut lw = new_window();
    show(
        &mut lw,
        Dom::create_body().with_child(TextArea::create().with_text(AzString::from("alpha")).dom()),
    );
    let mut tree = None;
    let full = deliver(&mut lw, &mut tree).expect("the first layout parks the tree");
    let host = A11yNodeId(CONTAINER as u64 + 1);
    let before = full
        .nodes
        .iter()
        .find(|(id, _)| *id == host)
        .map(|(_, n)| n.clone())
        .expect("the text field is in the tree");

    let cursor = end_of_text_cursor(&lw);
    lw.focus_manager.set_focused_node(Some(dnid(0, CONTAINER)));
    lw.start_editing_at(cursor, DomId::ROOT_ID, NodeId::new(LABEL_P), 0);
    lw.text_edit_manager.blink.set_visibility(true);
    let affected = lw.record_text_input("x");
    assert!(!affected.is_empty(), "the keystroke reached the field");
    let _ = lw.apply_text_changeset();
    lw.update_a11y_tree_incremental();
    let patch = deliver(&mut lw, &mut tree).expect("the keystroke publishes the field");
    let after = patch
        .nodes
        .iter()
        .find(|(id, _)| *id == host)
        .map(|(_, n)| n.clone())
        .expect("the patch carries the edited field");

    assert_eq!(after.role(), before.role(), "a keystroke must not change what the field is");
    assert!(
        after.bounds().is_some(),
        "the edited field keeps its bounds (the VoiceOver cursor is drawn around them)"
    );
    assert_eq!(after.html_tag(), before.html_tag());
    assert!(
        after.value().is_some_and(|v| v.contains('x')),
        "the patch carries the new text"
    );
}

// =========================================================================
// Requests that name a node the DOM no longer has
// =========================================================================

fn focused(lw: &LayoutWindow) -> Option<(usize, usize)> {
    lw.focus_manager
        .get_focused_node()
        .and_then(|f| Some((f.dom.inner, f.node.into_crate_internal()?.index())))
}

/// Whether `(dom, node)` names a node the window has now.
fn exists(lw: &LayoutWindow, (dom, node): (usize, usize)) -> bool {
    lw.layout_results
        .get(&DomId { inner: dom })
        .is_some_and(|lr| node < lr.styled_dom.node_data.as_ref().len())
}

#[test]
fn an_action_on_a_node_that_is_gone_is_answered_as_gone() {
    let mut lw = new_window();
    show(&mut lw, list_page());
    let mut tree = None;
    deliver(&mut lw, &mut tree);
    // The screen reader holds item 11 (node 24); the app re-renders a short list.
    show(&mut lw, Dom::create_body().with_child(para("only")));
    for (dom, node) in [(0usize, 24usize), (0, 9_999), (7, 2)] {
        let affected = lw.process_accessibility_action(
            DomId { inner: dom },
            NodeId::new(node),
            AccessibilityAction::Focus,
            now(),
        );
        assert!(
            affected.is_empty(),
            "an action on ({dom}, {node}), which the DOM no longer has, does nothing"
        );
        assert!(
            focused(&lw).is_none_or(|f| exists(&lw, f)),
            "no focus may be parked on a node that does not exist: {:?}",
            focused(&lw)
        );
    }
    lw.update_a11y_tree();
    deliver(&mut lw, &mut tree);
}

/// A small deterministic generator (xorshift), so a failure reproduces.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// A random page: paragraphs, buttons, focusable boxes and nested lists.
fn random_page(rng: &mut Rng) -> Dom {
    let mut body = Dom::create_body();
    for i in 0..(1 + rng.below(12)) {
        let child = match rng.below(4) {
            0 => para(&format!("text {i}")),
            1 => Dom::create_button_no_a11y(AzString::from(format!("button {i}"))),
            2 => Dom::create_div()
                .with_tab_index(TabIndex::Auto)
                .with_accessibility_name(format!("box {i}")),
            _ => {
                let mut list = Dom::create_div().with_ids_and_classes(class("list"));
                for k in 0..rng.below(6) {
                    list = list.with_child(para(&format!("row {i}.{k}")).with_ids_and_classes(class("item")));
                }
                list
            }
        };
        body = body.with_child(child);
    }
    body
}

fn random_action(rng: &mut Rng) -> AccessibilityAction {
    match rng.below(7) {
        0 => AccessibilityAction::Focus,
        1 => AccessibilityAction::Default,
        2 => AccessibilityAction::ScrollIntoView,
        3 => AccessibilityAction::Blur,
        4 => AccessibilityAction::ScrollDown,
        5 => AccessibilityAction::SetValue(AzString::from("typed")),
        _ => AccessibilityAction::SetSequentialFocusNavigationStartingPoint,
    }
}

#[test]
fn random_rebuilds_and_requests_with_old_ids_never_desync_the_tree() {
    for seed in [0x9E37_79B9_7F4A_7C15_u64, 42, 7_777, 1_234_567] {
        let mut rng = Rng(seed);
        let mut lw = new_window();
        let mut tree = None;
        // Every id any tree published so far: what a screen reader may still hold.
        let mut seen: Vec<A11yNodeId> = Vec::new();
        for step in 0..12 {
            show(&mut lw, random_page(&mut rng));
            lw.update_a11y_tree();
            if let Some(update) = deliver(&mut lw, &mut tree) {
                seen.extend(update.nodes.iter().map(|(id, _)| *id));
            }
            assert!(
                lw.a11y_manager.last_rejection.is_none(),
                "seed {seed} step {step}: a rebuild needed a refusal: {:?}",
                lw.a11y_manager.last_rejection
            );
            for _ in 0..6 {
                let id = seen[rng.below(seen.len())];
                let (dom, node) = azul_layout::managers::a11y::decode_a11y_node_id(id);
                let _ = lw.process_accessibility_action(dom, node, random_action(&mut rng), now());
                assert!(
                    focused(&lw).is_none_or(|f| exists(&lw, f)),
                    "seed {seed} step {step}: a request on {id:?} parked the focus on a node \
                     that does not exist: {:?}",
                    focused(&lw)
                );
                lw.update_a11y_tree();
                deliver(&mut lw, &mut tree);
            }
        }
    }
}
