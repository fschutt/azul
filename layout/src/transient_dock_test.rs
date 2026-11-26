//! What the docking of an inline `<transient-window>` says about a layout that already ran.
//!
//! The layout pass that finds a window the manager has not met yet draws it by its CONFIG alone
//! (`TransientDocks::is_inline`: "inline on its very first frame instead of one frame late"). The
//! reconcile that opens it afterwards therefore changes nothing that pass read - unless the window
//! is registered differently from what its config said - and must not ask for another pass: that
//! pass lays the whole page out again, cold. An app whose first DOM holds an opened inline panel
//! (the AzWidgets gallery's "Tools" panel) laid itself out twice before its first frame.

use azul_core::{
    dom::DomId,
    geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalSize},
    id::NodeId,
    transient::{TransientAnchor, TransientDismiss, TransientDock, TransientTearoff},
};

use super::{TransientPlacement, TransientWindowManager};

fn placement(node: usize, dock: TransientDock) -> TransientPlacement {
    TransientPlacement {
        node: NodeId::new(node),
        anchor_rect: LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(10.0, 10.0)),
        anchor: TransientAnchor::Bottom,
        dismiss: TransientDismiss::Outside,
        size: OptionLogicalSize::None,
        tearoff: TransientTearoff::None,
        torn: false,
        material: azul_core::window::WindowBackgroundMaterial::Opaque,
        dock,
    }
}

fn sized(_: DomId, _: &TransientPlacement) -> Option<LogicalSize> {
    Some(LogicalSize::new(100.0, 50.0))
}

/// THE case: the first DOM of a window opens an inline panel. The pass that laid the DOM out grafted
/// the panel (its config says `dock="inline"`, `open`), so registering it asks for no pass.
#[test]
fn an_inline_window_the_pass_already_grafted_by_its_config_is_no_docking_change() {
    let mut m = TransientWindowManager::new();
    let start = m.dock_generation();

    m.reconcile(&[placement(4, TransientDock::Inline)], sized);

    assert_eq!(
        m.dock_generation(),
        start,
        "the panel was laid out inline by the pass that found it; a second pass lays the page out again, cold"
    );
    let docks = m.layout_docks();
    assert!(
        docks.known.contains(&NodeId::new(4)) && docks.inline.contains_key(&NodeId::new(4)),
        "premise: the window is registered, and inline: {docks:?}"
    );
}

/// The same for a panel that can be torn off onto drop zones (the AzWidgets panel): the tear-off
/// capability does not change how it sits at home.
#[test]
fn an_inline_panel_that_can_tear_off_is_no_docking_change_either() {
    let mut m = TransientWindowManager::new();
    let start = m.dock_generation();
    let panel = TransientPlacement {
        tearoff: TransientTearoff::Zone,
        ..placement(4, TransientDock::Inline)
    };

    m.reconcile(&[panel], sized);

    assert_eq!(m.dock_generation(), start);
}

/// And when the panel stays open across rebuilds nothing moves either (the identical-DOM fast path
/// must stay available: it is skipped only while the docking differs from the laid-out one).
#[test]
fn an_inline_window_that_stays_open_keeps_the_generation_the_pass_laid_out() {
    let mut m = TransientWindowManager::new();
    m.reconcile(&[placement(4, TransientDock::Inline)], sized);
    let docked = m.dock_generation();

    m.reconcile(&[placement(4, TransientDock::Inline)], sized);
    m.reconcile(&[placement(4, TransientDock::Inline)], sized);

    assert_eq!(m.dock_generation(), docked);
}

/// A window a callback opened (`set_transient_window_open`): its node says `open=false`, so the pass
/// that ran before the callback drew it as nothing. Grafting it IS a layout change.
#[test]
fn an_inline_window_a_callback_opened_is_a_docking_change() {
    let mut m = TransientWindowManager::new();
    m.set_forced_open(NodeId::new(4), true);
    let start = m.dock_generation();

    m.reconcile(&[placement(4, TransientDock::Inline)], sized);

    assert_ne!(
        m.dock_generation(),
        start,
        "the layout drew the node as closed (its config says so)"
    );
}

/// The app's `torn` attribute on a window that cannot be torn off: the config said "torn" (not
/// inline), the registry says inline. Another pass grafts it.
#[test]
fn an_inline_window_whose_torn_attribute_changes_nothing_is_a_docking_change() {
    let mut m = TransientWindowManager::new();
    let start = m.dock_generation();
    let panel = TransientPlacement {
        torn: true,
        tearoff: TransientTearoff::None,
        ..placement(4, TransientDock::Inline)
    };

    m.reconcile(&[panel], sized);

    assert_ne!(m.dock_generation(), start);
}

/// A panel that stops being wanted closes: the pass that found it still drew it inline, the next
/// one must not. A change, as before.
#[test]
fn closing_an_inline_window_is_a_docking_change() {
    let mut m = TransientWindowManager::new();
    m.reconcile(&[placement(4, TransientDock::Inline)], sized);
    let open = m.dock_generation();

    m.reconcile(&[], sized);

    assert_ne!(m.dock_generation(), open);
}

/// A popup (not inline) is unchanged by this: its opening still counts, as the manager's own tests
/// say (`opening_closing_or_inlining_a_window_changes_the_docking`).
#[test]
fn a_popup_opening_is_still_a_docking_change() {
    let mut m = TransientWindowManager::new();
    let start = m.dock_generation();

    m.reconcile(&[placement(4, TransientDock::Popup)], sized);

    assert_ne!(m.dock_generation(), start);
}

/// A window whose content cannot be laid out does not open: the registry holds nothing new, so
/// there is nothing to compare (as before).
#[test]
fn an_inline_window_that_does_not_open_changes_nothing() {
    let mut m = TransientWindowManager::new();
    let start = m.dock_generation();

    m.reconcile(&[placement(4, TransientDock::Inline)], |_, _| None);

    assert_eq!(m.dock_generation(), start);
    assert!(m.open_windows().is_empty());
}

/// Two inline panels, one already open and one new: only the one the pass could not have known
/// counts, and it does not.
#[test]
fn a_second_inline_panel_opening_is_no_change_either() {
    let mut m = TransientWindowManager::new();
    m.reconcile(&[placement(4, TransientDock::Inline)], sized);
    let docked = m.dock_generation();

    m.reconcile(
        &[placement(4, TransientDock::Inline), placement(9, TransientDock::Inline)],
        sized,
    );

    assert_eq!(m.dock_generation(), docked);
    assert_eq!(m.open_windows().len(), 2);
}
