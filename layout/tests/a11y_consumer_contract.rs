//! Headless CI contract for the accessibility tree.
//!
//! Every `TreeUpdate` the manager parks is fed through the REAL
//! `accesskit_consumer` — the crate whose invariant `panic!`s abort a
//! `panic = "abort"` release build (the shells' `catch_unwind` cannot save
//! it). Under the test profile a violation is an ordinary test failure, so
//! the whole "a11y update aborts the app" class is caught here, in CI,
//! instead of on a user's desk.
//!
//! The 2026-08-29 AzWriter abort is pinned verbatim: focus parked in a DOM
//! the tree does not hold, then an INCREMENTAL update (text edit) naming it
//! as `focus` — `Focused ID #8589934595 is not in the node list`.
#![cfg(feature = "a11y")]

use std::time::{Duration, Instant as StdInstant};

use accesskit::{Node, NodeId as A11yNodeId, Role, Tree as A11yTree, TreeId, TreeUpdate};
use accesskit_consumer::{Tree, TreeChangeHandler};
use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    selection::{CursorAffinity, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    managers::a11y::{A11yManager, A11yTreeMirror, A11yUpdateError},
    widgets::text_area::TextArea,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body(0) > container(1) > placeholder-p(2) > text(3), label-p(4) > text(5).
const CONTAINER: usize = 1;
const LABEL_P: usize = 4;
const LABEL_TEXT: usize = 5;

fn dnid(dom: usize, node: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId { inner: dom },
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
    }
}

fn a11y_id(dom: u64, node: u64) -> A11yNodeId {
    A11yNodeId((dom << 32) | (node + 1))
}

struct Noop;
impl TreeChangeHandler for Noop {
    fn node_added(&mut self, _: &accesskit_consumer::Node<'_>) {}
    fn node_updated(&mut self, _: &accesskit_consumer::Node<'_>, _: &accesskit_consumer::Node<'_>) {}
    fn focus_moved(
        &mut self,
        _: Option<&accesskit_consumer::Node<'_>>,
        _: Option<&accesskit_consumer::Node<'_>>,
    ) {
    }
    fn node_removed(&mut self, _: &accesskit_consumer::Node<'_>) {}
}

struct Harness {
    lw: LayoutWindow,
    renderer_resources: RendererResources,
    system_callbacks: ExternalSystemCallbacks,
    window_state: FullWindowState,
}

impl Harness {
    fn new_with_text_area(width: f32, height: f32, text: &str) -> Self {
        let mut dom =
            Dom::create_body().with_child(TextArea::create().with_text(AzString::from(text)).dom());
        let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
        let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
        lw.system_animations_override = Some(azul_core::resources::SystemAnimations::disabled());
        let mut window_state = FullWindowState::default();
        window_state.size.dimensions = LogicalSize::new(width, height);
        lw.current_window_state = window_state.clone();
        let renderer_resources = RendererResources::default();
        let system_callbacks = ExternalSystemCallbacks::rust_internal();
        let mut dbg = Some(Vec::new());
        lw.layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut dbg,
        )
        .unwrap();
        Self {
            lw,
            renderer_resources,
            system_callbacks: ExternalSystemCallbacks::rust_internal(),
            window_state,
        }
    }

    fn register_scroll_nodes(&mut self) {
        let now = Instant::from(std::time::Instant::now());
        azul_layout::managers::scroll_registration::register_scroll_nodes(&mut self.lw, &now);
    }

    fn start_editing(&mut self, cursor: TextCursor) {
        self.lw
            .focus_manager
            .set_focused_node(Some(dnid(0, CONTAINER)));
        self.lw.text_edit_manager.initialize_editing(
            cursor,
            DomId::ROOT_ID,
            NodeId::new(LABEL_P),
            0,
        );
        self.lw.text_edit_manager.blink.set_visibility(true);
    }

    fn end_of_text_cursor(&self) -> TextCursor {
        let tree = &self
            .lw
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

    fn type_str(&mut self, s: &str) {
        let affected = self.lw.record_text_input(s);
        assert!(!affected.is_empty(), "the input was recorded against the focused node");
        let _ = self.lw.apply_text_changeset();
    }

    fn relayout(&mut self) {
        let Some(lr) = self.lw.layout_results.remove(&DomId::ROOT_ID) else {
            return;
        };
        let mut dbg = Some(Vec::new());
        self.lw
            .layout_and_generate_display_list(
                lr.styled_dom,
                &self.window_state,
                &self.renderer_resources,
                &self.system_callbacks,
                &mut dbg,
            )
            .unwrap();
    }

    /// Drain the manager the way a shell does and push through the REAL
    /// consumer. Returns the update for inspection.
    fn deliver(&mut self, tree: &mut Option<Tree>) -> Option<TreeUpdate> {
        let update = self.lw.a11y_manager.take_pending()?;
        match tree {
            None => *tree = Some(Tree::new(update.clone(), true)),
            Some(t) => t.update_and_process_changes(update.clone(), &mut Noop),
        }
        Some(update)
    }
}

fn bounds_y1(update: &TreeUpdate, id: A11yNodeId) -> Option<f64> {
    update
        .nodes
        .iter()
        .find(|(i, _)| *i == id)
        .and_then(|(_, n)| n.bounds())
        .map(|r| r.y1)
}

const TEN_LINES: &str =
    "line one\nline two\nline three\nline four\nline five\nline six\nline seven\nline \
     eight\nline nine\nline ten";

// =========================================================================
// The real consumer accepts everything the manager parks
// =========================================================================

#[test]
fn every_parked_update_survives_the_real_consumer_across_typing_and_relayout() {
    let mut h = Harness::new_with_text_area(400.0, 300.0, "alpha");
    let mut tree = None;
    let first = h.deliver(&mut tree).expect("layout parks a full tree");
    assert!(first.tree.is_some(), "the first update is a full tree");

    let end = h.end_of_text_cursor();
    h.start_editing(end);
    for _ in 0..3 {
        h.type_str("x");
        h.deliver(&mut tree);
    }
    h.relayout();
    h.deliver(&mut tree);
    h.type_str("y");
    h.deliver(&mut tree);

    assert!(
        h.lw.a11y_manager.last_rejection.is_none(),
        "a benign sequence needs no refusal, got {:?}",
        h.lw.a11y_manager.last_rejection
    );
}

// =========================================================================
// THE AzWriter abort: focus in a DOM the tree does not hold
// =========================================================================

#[test]
fn a_focus_in_a_vanished_dom_never_reaches_the_consumer() {
    let mut h = Harness::new_with_text_area(400.0, 300.0, "alpha");
    let mut tree = None;
    h.deliver(&mut tree);

    let end = h.end_of_text_cursor();
    h.start_editing(end);
    // The device state: the FocusManager still points into a child DOM (a
    // transient window / VirtualView page) that two RefreshDom relayouts have
    // rebuilt — `Focused ID #8589934595` = (dom 2, node 2).
    h.lw.focus_manager.set_focused_node(Some(dnid(2, 2)));

    // The text-edit path parks an INCREMENTAL update naming that focus.
    h.lw.update_a11y_tree_incremental();

    assert_eq!(
        h.lw.a11y_manager.last_rejection,
        Some(A11yUpdateError::FocusNotInTree(a11y_id(2, 2))),
        "the manager must refuse the incremental update"
    );
    let parked = h
        .deliver(&mut tree)
        .expect("the refusal falls back to a full rebuild");
    assert!(
        parked.tree.is_some(),
        "the fallback is a FULL tree, not the refused increment"
    );
    assert_ne!(
        parked.focus,
        a11y_id(2, 2),
        "the full rebuild degrades an unresolvable focus"
    );
    // The consumer accepted it (or this test would have panicked above).
}

// =========================================================================
// A parked full tree is not clobbered by a later incremental update
// =========================================================================

#[test]
fn a_parked_full_tree_absorbs_a_later_incremental_update() {
    let mut h = Harness::new_with_text_area(400.0, 300.0, "alpha");
    let mut tree = None;
    h.deliver(&mut tree);
    let end = h.end_of_text_cursor();
    h.start_editing(end);

    // Relayout parks a full tree; the keystroke parks an increment BEFORE a
    // shell drains the slot (the platform flush runs at end of pass).
    h.relayout();
    h.type_str("z");

    let merged = h.deliver(&mut tree).expect("something is parked");
    assert!(
        merged.tree.is_some(),
        "the increment must fold INTO the parked full tree, not replace it"
    );
    let label = merged
        .nodes
        .iter()
        .find(|(id, _)| *id == a11y_id(0, LABEL_P as u64))
        .map(|(_, n)| n.value().unwrap_or_default().to_string())
        .expect("the edited node is in the merged tree");
    assert!(
        label.contains('z'),
        "the merged tree carries the increment's fresh value, got {label:?}"
    );
}

// =========================================================================
// Scrolling updates the delivered tree
// =========================================================================

#[test]
fn scrolling_moves_the_delivered_bounds_and_is_rebuilt_on_the_next_due_tick() {
    let mut h = Harness::new_with_text_area(400.0, 300.0, TEN_LINES);
    h.register_scroll_nodes();
    let mut tree = None;
    let before = h.deliver(&mut tree).expect("full tree");
    let label = a11y_id(0, LABEL_P as u64);
    let y1_before = bounds_y1(&before, label).expect("the label has bounds");

    h.lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        NodeId::new(CONTAINER),
        LogicalPosition::new(0.0, 40.0),
        Instant::from(std::time::Instant::now()),
    );
    // What the ScrollTo handler does, then what a shell tick does.
    h.lw.a11y_manager.mark_scroll_dirty();
    assert!(
        h.lw.a11y_manager
            .scroll_rebuild_due(StdInstant::now(), Duration::from_millis(100)),
        "a dirty tree is due on the first tick"
    );
    h.lw.update_a11y_tree();
    let after = h.deliver(&mut tree).expect("the scroll rebuild parks a full tree");

    let y1_after = bounds_y1(&after, label).expect("the label still has bounds");
    assert!(
        (y1_before - y1_after - 40.0).abs() < 0.5,
        "the label must move up by the scroll offset: before y1={y1_before}, after y1={y1_after}"
    );
    let container = after
        .nodes
        .iter()
        .find(|(id, _)| *id == a11y_id(0, CONTAINER as u64))
        .map(|(_, n)| n.scroll_y())
        .expect("the scroll container is in the tree");
    assert!(
        (container.unwrap_or(0.0) - 40.0).abs() < 0.5,
        "the scroller advertises its new offset, got {container:?}"
    );
}

#[test]
fn scroll_rebuilds_are_throttled_but_never_lost() {
    let mut m = A11yManager::new();
    let t0 = StdInstant::now();
    let interval = Duration::from_millis(100);
    assert!(!m.scroll_rebuild_due(t0, interval), "clean: nothing due");
    m.mark_scroll_dirty();
    assert!(m.scroll_rebuild_due(t0, interval), "first dirty tick is due");
    m.mark_scroll_dirty();
    assert!(
        !m.scroll_rebuild_due(t0 + Duration::from_millis(10), interval),
        "10ms later: throttled"
    );
    assert!(m.scroll_dirty, "a throttled tick keeps the flag");
    assert!(
        m.scroll_rebuild_due(t0 + interval, interval),
        "one interval later the glide's final state lands"
    );
    assert!(!m.scroll_rebuild_due(t0 + Duration::from_secs(1), interval));
}

// =========================================================================
// The mirror replays the consumer's rules (unit)
// =========================================================================

fn container(children: &[u64]) -> Node {
    let mut n = Node::new(Role::GenericContainer);
    n.set_children(children.iter().map(|c| A11yNodeId(*c)).collect::<Vec<_>>());
    n
}

fn full(nodes: Vec<(u64, Node)>, focus: u64) -> TreeUpdate {
    TreeUpdate {
        nodes: nodes.into_iter().map(|(i, n)| (A11yNodeId(i), n)).collect(),
        tree: Some(A11yTree::new(A11yNodeId(0))),
        focus: A11yNodeId(focus),
        tree_id: TreeId::ROOT,
    }
}

fn incremental(nodes: Vec<(u64, Node)>, focus: u64) -> TreeUpdate {
    TreeUpdate {
        nodes: nodes.into_iter().map(|(i, n)| (A11yNodeId(i), n)).collect(),
        tree: None,
        focus: A11yNodeId(focus),
        tree_id: TreeId::ROOT,
    }
}

fn delivered_three() -> A11yTreeMirror {
    A11yTreeMirror::default()
        .apply(&full(
            vec![(0, container(&[1, 2])), (1, container(&[])), (2, container(&[]))],
            1,
        ))
        .expect("a well-formed full tree")
}

#[test]
fn mirror_accepts_a_well_formed_full_tree() {
    let m = delivered_three();
    assert_eq!(m.root, Some(A11yNodeId(0)));
    assert_eq!(m.children.len(), 3);
}

#[test]
fn mirror_refuses_what_the_consumer_panics_on() {
    let m = delivered_three();
    assert_eq!(
        m.apply(&incremental(vec![(1, container(&[]))], 99)),
        Err(A11yUpdateError::FocusNotInTree(A11yNodeId(99)))
    );
    assert_eq!(
        m.apply(&incremental(vec![(7, container(&[]))], 0)),
        Err(A11yUpdateError::OrphanNode(A11yNodeId(7)))
    );
    assert_eq!(
        A11yTreeMirror::default().apply(&full(
            vec![(0, container(&[1, 1])), (1, container(&[]))],
            0
        )),
        Err(A11yUpdateError::DuplicateChild(A11yNodeId(1)))
    );
    assert_eq!(
        A11yTreeMirror::default().apply(&full(
            vec![(0, container(&[1, 9])), (1, container(&[]))],
            0
        )),
        Err(A11yUpdateError::UnknownChild {
            parent: A11yNodeId(0),
            child: A11yNodeId(9)
        })
    );
    assert_eq!(
        A11yTreeMirror::default().apply(&incremental(vec![(1, container(&[]))], 1)),
        Err(A11yUpdateError::NoTreeYet)
    );
}

#[test]
fn mirror_prunes_a_dropped_subtree_exactly_like_the_consumer() {
    let m = delivered_three();
    // Root no longer lists node 2 → it becomes unreachable and is removed;
    // a focus that still points at it is the classic abort.
    assert_eq!(
        m.apply(&incremental(vec![(0, container(&[1]))], 2)),
        Err(A11yUpdateError::FocusNotInTree(A11yNodeId(2)))
    );
    let pruned = m
        .apply(&incremental(vec![(0, container(&[1]))], 1))
        .expect("focus on a surviving node is fine");
    assert!(!pruned.children.contains_key(&A11yNodeId(2)));
    assert_eq!(pruned.children.len(), 2);
}

#[test]
fn publish_refuses_and_keeps_the_previously_parked_update() {
    let mut m = A11yManager::new();
    m.publish(full(
        vec![(0, container(&[1])), (1, container(&[]))],
        1,
    ))
    .expect("full tree publishes");
    // Not yet taken: the slot holds the full tree. A bad increment must not
    // disturb it.
    assert_eq!(
        m.publish(incremental(vec![(1, container(&[]))], 42)),
        Err(A11yUpdateError::FocusNotInTree(A11yNodeId(42)))
    );
    let still = m.take_pending().expect("the good update is still parked");
    assert!(still.tree.is_some());
    assert_eq!(still.focus, A11yNodeId(1));
    assert_eq!(m.delivered.children.len(), 2, "take advances the mirror");
}
