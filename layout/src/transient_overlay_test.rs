//! A window with ONE surface (a phone) shows every popup and dialog INSIDE itself.
//!
//! The manager decides it, per window, from the `ReconcileEnv` the engine builds
//! (`LayoutWindow::reconcile_transients`): a popup (`dock="popup"`, the default) gets an `OverlayBox`
//! instead of a surface, placed by the one placement rule within the room the window leaves it. Nothing
//! else changes: the same `OpenTransientWindow`, the same focus bookkeeping, the same dismissals.

use azul_core::{
    dom::DomId,
    geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalSize},
    id::NodeId,
    transient::{TransientAnchor, TransientDismiss, TransientDock, TransientTearoff},
};

use super::{
    overlay_box_for, OverlayBox, ReconcileEnv, TransientPlacement, TransientWindowManager,
    WindowPresentation,
};

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
}

/// A Pixel 8 in portrait, less its status bar and gesture bar, less nothing for the keyboard.
fn phone() -> ReconcileEnv {
    ReconcileEnv {
        presentation: WindowPresentation::OneSurface,
        window: LogicalSize::new(412.0, 915.0),
        bounds: rect(0.0, 48.0, 412.0, 915.0 - 48.0 - 24.0),
        cursor: None,
    }
}

fn desktop() -> ReconcileEnv {
    ReconcileEnv::default()
}

fn placement(node: usize, anchor: TransientAnchor, anchor_rect: LogicalRect) -> TransientPlacement {
    TransientPlacement {
        node: NodeId::new(node),
        anchor_rect,
        anchor,
        dismiss: TransientDismiss::Outside,
        size: OptionLogicalSize::None,
        tearoff: TransientTearoff::None,
        torn: false,
        material: azul_core::window::WindowBackgroundMaterial::Opaque,
        dock: TransientDock::Popup,
    }
}

fn panel(width: f32, height: f32) -> impl FnMut(DomId, &TransientPlacement) -> Option<LogicalSize> {
    move |_, _| Some(LogicalSize::new(width, height))
}

fn overlay_of(m: &TransientWindowManager, node: usize) -> OverlayBox {
    m.open_windows()
        .iter()
        .find(|w| w.source_node == NodeId::new(node))
        .and_then(|w| w.overlay)
        .unwrap_or_else(|| panic!("node {node} is no overlay: {:?}", m.open_windows()))
}

/// THE decision: on a window with one surface a popup is an overlay (no surface for the shell to
/// make), on any other window it is what it always was.
#[test]
fn a_popup_on_a_one_surface_window_is_an_overlay_and_on_any_other_a_window() {
    let invoker = rect(20.0, 100.0, 120.0, 40.0);

    let mut windows = TransientWindowManager::new();
    let d = windows.reconcile_in(
        &[placement(4, TransientAnchor::Bottom, invoker)],
        &desktop(),
        panel(200.0, 150.0),
    );
    assert_eq!(d.opened.len(), 1);
    let w = &windows.open_windows()[0];
    assert!(w.overlay.is_none() && !w.has_no_surface(), "a desktop popup is a window: {w:?}");

    let mut phone_windows = TransientWindowManager::new();
    let d = phone_windows.reconcile_in(
        &[placement(4, TransientAnchor::Bottom, invoker)],
        &phone(),
        panel(200.0, 150.0),
    );
    assert_eq!(d.opened.len(), 1);
    let w = &phone_windows.open_windows()[0];
    assert!(w.overlay.is_some(), "a phone popup is an overlay: {w:?}");
    assert!(w.has_no_surface(), "the shell has no surface to make for it");
    assert!(!w.is_inline(), "an overlay is not an inline dock");
}

/// An overlay hangs off its anchor like the popup it stands for: below it, left edges lined up.
#[test]
fn an_overlay_opens_below_its_invoker_with_the_left_edges_lined_up() {
    let mut m = TransientWindowManager::new();
    m.reconcile_in(
        &[placement(4, TransientAnchor::Bottom, rect(20.0, 100.0, 120.0, 40.0))],
        &phone(),
        panel(200.0, 150.0),
    );
    let o = overlay_of(&m, 4);
    assert_eq!(o.rect, rect(20.0, 140.0, 200.0, 150.0));
    assert!(!o.cover);
}

/// One placement rule for every floating surface: no room below, it opens above; no room right,
/// it slides back onto the window.
#[test]
fn an_overlay_flips_and_slides_to_stay_inside_the_window() {
    let mut m = TransientWindowManager::new();
    // An invoker near the bottom right of the room.
    m.reconcile_in(
        &[placement(4, TransientAnchor::Bottom, rect(330.0, 800.0, 70.0, 40.0))],
        &phone(),
        panel(200.0, 150.0),
    );
    let o = overlay_of(&m, 4);
    assert_eq!(o.rect.origin.y, 800.0 - 150.0, "it opens above the invoker: no room below");
    assert!(
        o.rect.origin.x + o.rect.size.width <= 412.0,
        "it is slid back onto the window: {o:?}"
    );
}

/// The status popover of the sidebar's rail: the invoker is at the very BOTTOM of the window's left
/// edge, the popover opens to its right with the top edges lined up - and would run off the window.
/// It stays on the window (SIDEBAR's constraint 1, the status slot is the last group of the rail).
#[test]
fn a_popover_beside_the_bottom_of_the_rail_slides_up_instead_of_running_off() {
    let mut m = TransientWindowManager::new();
    m.reconcile_in(
        &[placement(4, TransientAnchor::Right, rect(0.0, 860.0, 18.0, 38.0))],
        &phone(),
        panel(240.0, 120.0),
    );
    let o = overlay_of(&m, 4);
    assert_eq!(o.rect.origin.x, 18.0, "beside the rail");
    assert!(
        o.rect.origin.y + o.rect.size.height <= 915.0 - 24.0,
        "its bottom is above the gesture bar: {o:?}"
    );
    assert!(o.rect.origin.y >= 48.0, "and below the status bar: {o:?}");
}

/// A panel taller than the room (a list of properties behind a (?)) comes up at the room's height
/// and scrolls inside it.
#[test]
fn an_overlay_taller_than_the_window_is_shortened_to_it() {
    let mut m = TransientWindowManager::new();
    m.reconcile_in(
        &[placement(4, TransientAnchor::Top, rect(20.0, 880.0, 120.0, 30.0))],
        &phone(),
        panel(300.0, 2000.0),
    );
    let o = overlay_of(&m, 4);
    let bounds = phone().bounds;
    assert_eq!(o.rect.size.height, bounds.size.height);
    assert!(o.rect.origin.y >= bounds.origin.y);
    assert!(o.rect.origin.y + o.rect.size.height <= bounds.origin.y + bounds.size.height);
}

/// With the keyboard up the room is shorter: the overlay is placed in what is left.
#[test]
fn an_overlay_keeps_clear_of_the_keyboard() {
    let mut env = phone();
    env.bounds = rect(0.0, 48.0, 412.0, 400.0); // the keyboard takes the lower half
    let mut m = TransientWindowManager::new();
    m.reconcile_in(
        &[placement(4, TransientAnchor::Bottom, rect(20.0, 380.0, 120.0, 40.0))],
        &env,
        panel(200.0, 150.0),
    );
    let o = overlay_of(&m, 4);
    assert!(o.rect.origin.y + o.rect.size.height <= 48.0 + 400.0, "{o:?}");
}

/// A dialog and a viewport cover are the window's whole top layer, whatever the node's own rect.
#[test]
fn a_dialog_overlay_covers_the_window() {
    let mut m = TransientWindowManager::new();
    let dialog = placement(4, TransientAnchor::Dialog, rect(20.0, 100.0, 120.0, 40.0));
    let d = m.reconcile_in(&[dialog], &phone(), |_, _| panic!("a cover is not measured"));
    assert_eq!(d.opened.len(), 1);
    let o = overlay_of(&m, 4);
    assert!(o.cover);
    assert_eq!(o.rect, rect(0.0, 0.0, 412.0, 915.0));
    assert_eq!(m.open_windows()[0].content_size, LogicalSize::new(412.0, 915.0));
}

/// The cover follows the window (a rotation): it is a MOVE of the overlay, never a new window.
#[test]
fn a_cover_follows_its_window_when_it_is_resized() {
    let mut m = TransientWindowManager::new();
    let dialog = placement(4, TransientAnchor::Dialog, rect(0.0, 0.0, 1.0, 1.0));
    m.reconcile_in(&[dialog], &phone(), |_, _| None);
    let id = m.open_windows()[0].content_dom;

    let mut sideways = phone();
    sideways.window = LogicalSize::new(915.0, 412.0);
    let d = m.reconcile_in(&[dialog], &sideways, |_, _| None);
    assert_eq!(d.moved, vec![id]);
    assert!(d.opened.is_empty() && d.closed.is_empty());
    assert_eq!(overlay_of(&m, 4).rect, rect(0.0, 0.0, 915.0, 412.0));
}

/// An overlay opening, closing or moving asks the root for a pass that KEEPS the page's cache: it is
/// no dock change (that lays the whole page out cold).
#[test]
fn an_overlay_is_no_docking_change() {
    let mut m = TransientWindowManager::new();
    let start = m.dock_generation();
    m.reconcile_in(
        &[placement(4, TransientAnchor::Bottom, rect(20.0, 100.0, 120.0, 40.0))],
        &phone(),
        panel(200.0, 150.0),
    );
    assert_eq!(m.dock_generation(), start, "opening an overlay");
    assert!(m.overlays_changed(), "but the root has to be laid out again");

    // Still open, the anchor moved with the page (it scrolled): a move of the overlay.
    let _ = m.take_overlay_dirt();
    m.reconcile_in(
        &[placement(4, TransientAnchor::Bottom, rect(20.0, 160.0, 120.0, 40.0))],
        &phone(),
        panel(200.0, 150.0),
    );
    assert_eq!(m.dock_generation(), start, "moving an overlay");
    assert!(m.overlays_changed());

    let _ = m.take_overlay_dirt();
    m.reconcile_in(&[], &phone(), panel(200.0, 150.0));
    assert_eq!(m.dock_generation(), start, "closing an overlay");
    assert!(m.overlays_changed());
}

/// The pass that lays the overlays out takes them as laid out, and names what to build fresh: the
/// overlays that opened, moved or closed - nothing else.
#[test]
fn the_overlay_dirt_is_what_opened_moved_or_closed_since_the_last_pass() {
    let mut m = TransientWindowManager::new();
    let a = placement(4, TransientAnchor::Bottom, rect(20.0, 100.0, 120.0, 40.0));
    let b = placement(7, TransientAnchor::Bottom, rect(20.0, 300.0, 120.0, 40.0));
    m.reconcile_in(&[a, b], &phone(), panel(200.0, 150.0));
    assert_eq!(m.take_overlay_dirt(), vec![NodeId::new(4), NodeId::new(7)], "both opened");
    assert!(!m.overlays_changed());
    assert!(m.take_overlay_dirt().is_empty(), "nothing since");

    // `b` moves, `a` stays, nothing else happens.
    let moved = placement(7, TransientAnchor::Bottom, rect(20.0, 320.0, 120.0, 40.0));
    m.reconcile_in(&[a, moved], &phone(), panel(200.0, 150.0));
    assert_eq!(m.take_overlay_dirt(), vec![NodeId::new(7)]);

    // `a` closes.
    m.reconcile_in(&[moved], &phone(), panel(200.0, 150.0));
    assert_eq!(m.take_overlay_dirt(), vec![NodeId::new(4)]);
}

/// A rebuild that shifts the node ids shifts what the last pass laid out with them: the overlay is
/// the same overlay, not one more pass.
#[cfg(feature = "text_layout")]
#[test]
fn what_the_last_pass_laid_out_follows_its_nodes_through_a_rebuild() {
    use crate::managers::{NodeIdMap, NodeIdRemap};

    let mut m = TransientWindowManager::new();
    m.reconcile_in(
        &[placement(4, TransientAnchor::Bottom, rect(20.0, 100.0, 120.0, 40.0))],
        &phone(),
        panel(200.0, 150.0),
    );
    let _ = m.take_overlay_dirt();
    assert!(!m.overlays_changed());

    // Two nodes were inserted above it: node 4 is node 6 now.
    let map = NodeIdMap::from_pairs([(NodeId::new(4), NodeId::new(6))]);
    m.remap_node_ids(DomId::ROOT_ID, &map);
    assert_eq!(m.open_windows()[0].source_node, NodeId::new(6));
    assert!(!m.overlays_changed(), "the same overlay at the same box: no pass owed");
    assert!(m.take_overlay_dirt().is_empty());
}

/// An overlay is shown, not docked: it is no part of the docking STRUCTURE the layout tree builder
/// is handed, but the layout shows the node.
#[test]
fn an_overlay_is_shown_but_is_not_part_of_the_docking_structure() {
    let mut m = TransientWindowManager::new();
    m.reconcile_in(
        &[placement(4, TransientAnchor::Bottom, rect(20.0, 100.0, 120.0, 40.0))],
        &phone(),
        panel(200.0, 150.0),
    );
    let docks = m.layout_docks();
    let cfg = azul_core::transient::TransientWindowConfig::opened();
    assert!(docks.shows(NodeId::new(4), &cfg), "shown");
    assert!(!docks.is_inline(NodeId::new(4), &cfg), "not inline");
    assert!(docks.overlay_box(NodeId::new(4)).is_some());
    let structure = m.layout_structure();
    assert!(structure.overlay.is_empty() && structure.known.is_empty() && structure.inline.is_empty());
    // A node the manager has not met is closed, as before.
    assert!(!docks.shows(NodeId::new(9), &azul_core::transient::TransientWindowConfig::closed()));
}

/// The insets the solver reads make the overlay's box the box laid out: each axis has both, against
/// the viewport the root is laid out in (the window less its safe area).
#[test]
fn the_insets_of_an_overlay_are_those_of_its_box_against_the_viewport() {
    let mut m = TransientWindowManager::new();
    m.reconcile_in(
        &[placement(4, TransientAnchor::Bottom, rect(20.0, 100.0, 120.0, 40.0))],
        &phone(),
        panel(200.0, 150.0),
    );
    let mut docks = m.layout_docks();
    // The viewport of the root starts under the status bar (48 px).
    docks.viewport_origin = LogicalPosition::new(0.0, 48.0);
    let viewport = LogicalSize::new(412.0, 915.0 - 48.0 - 24.0);
    let [top, right, bottom, left] = docks.overlay_insets(NodeId::new(4), viewport).unwrap();
    // The box is (20, 140) 200 x 150 in the window: (20, 92) in the viewport.
    assert_eq!((left, top), (20.0, 92.0));
    assert_eq!(left + 200.0 + right, viewport.width, "the width is what is left between the insets");
    assert_eq!(top + 150.0 + bottom, viewport.height);
    assert!(docks.overlay_insets(NodeId::new(5), viewport).is_none(), "another node is none");
}

/// A window whose presentation changes under an open popup (a desktop window put on a phone preset,
/// and back) changes KIND like a popup that is torn off: its OS window goes, an overlay takes its
/// place - and the other way. The app's `open` stays what it was.
#[test]
fn a_popup_changes_kind_when_the_presentation_changes_under_it() {
    let invoker = rect(20.0, 100.0, 120.0, 40.0);
    let popup = placement(4, TransientAnchor::Bottom, invoker);
    let mut m = TransientWindowManager::new();
    m.reconcile_in(&[popup], &desktop(), panel(200.0, 150.0));
    let window_dom = m.open_windows()[0].content_dom;

    let d = m.reconcile_in(&[popup], &phone(), panel(200.0, 150.0));
    assert_eq!(d.closed, vec![window_dom], "the window goes");
    assert_eq!(d.opened.len(), 1, "the overlay is new to the shell");
    assert_ne!(d.opened[0], window_dom, "a content id is never reused");
    assert!(m.open_windows()[0].overlay.is_some());
    assert_eq!(m.take_closed_surfaces().len(), 1, "the shell is handed the surface to close");

    let overlay_dom = m.open_windows()[0].content_dom;
    let d = m.reconcile_in(&[popup], &desktop(), panel(200.0, 150.0));
    assert_eq!(d.closed, vec![overlay_dom]);
    assert_eq!(d.opened.len(), 1);
    assert!(m.open_windows()[0].overlay.is_none(), "a window again");
}

/// An inline panel (`dock="inline"`) is content of the page already: a window with one surface leaves
/// it where it is, no overlay.
#[test]
fn an_inline_dock_is_no_overlay() {
    let mut m = TransientWindowManager::new();
    let panel_in_page = TransientPlacement {
        dock: TransientDock::Inline,
        ..placement(4, TransientAnchor::Bottom, rect(0.0, 0.0, 10.0, 10.0))
    };
    m.reconcile_in(&[panel_in_page], &phone(), panel(200.0, 150.0));
    let w = &m.open_windows()[0];
    assert!(w.overlay.is_none() && w.is_inline());
}

/// A window with one surface has no second one to tear a palette off into.
#[test]
fn an_overlay_is_never_torn_off() {
    let mut m = TransientWindowManager::new();
    let palette = TransientPlacement {
        tearoff: TransientTearoff::Free,
        torn: true,
        ..placement(4, TransientAnchor::Bottom, rect(20.0, 100.0, 120.0, 40.0))
    };
    m.reconcile_in(&[palette], &phone(), panel(200.0, 150.0));
    let w = &m.open_windows()[0];
    assert!(w.overlay.is_some() && w.torn.is_none(), "{w:?}");
}

/// What a closed popup leaves behind is nothing: closing releases the layout of the overlay.
#[test]
fn a_closed_overlay_is_gone_from_the_layout() {
    let mut m = TransientWindowManager::new();
    m.reconcile_in(
        &[placement(4, TransientAnchor::Bottom, rect(20.0, 100.0, 120.0, 40.0))],
        &phone(),
        panel(200.0, 150.0),
    );
    assert!(m.layout_docks().overlay.contains_key(&NodeId::new(4)));
    m.reconcile_in(&[], &phone(), panel(200.0, 150.0));
    assert!(m.open_windows().is_empty());
    assert!(m.layout_docks().overlay.is_empty());
}

/// `overlay_box_for` is the pure placement: the same box for the same inputs.
#[test]
fn overlay_box_for_is_the_placement_rule_clamped_to_the_bounds() {
    let p = placement(4, TransientAnchor::Left, rect(380.0, 300.0, 20.0, 40.0));
    let env = phone();
    let o = overlay_box_for(&p, LogicalSize::new(240.0, 100.0), &env);
    // Left of the anchor with the top edges lined up.
    assert_eq!(o.rect, rect(380.0 - 240.0, 300.0, 240.0, 100.0));
    assert!(!o.cover);
}
