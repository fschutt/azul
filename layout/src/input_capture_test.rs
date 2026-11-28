//! The two captures of the pointer, applied in the order the shells and the runner share: the explicit capture a node asked
//! for first, the press-target capture second - so a node that holds the pointer is told of the release ONCE.

use azul_core::{
    dom::{DomId, DomNodeId},
    events::{
        EventData, EventSource, EventType, KeyModifiers, MouseButton, MouseEventData, SyntheticEvent,
    },
    geom::LogicalPosition,
    id::NodeId,
    styled_dom::NodeHierarchyItemId,
};

use super::apply_input_captures;
use crate::managers::hover::{HoverManager, PointerCapture};

fn node(index: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId { inner: 0 },
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    }
}

fn mouse(event_type: EventType, target: DomNodeId) -> SyntheticEvent {
    SyntheticEvent::new(
        event_type,
        EventSource::User,
        target,
        azul_core::task::Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 }),
        EventData::Mouse(MouseEventData {
            position: LogicalPosition::zero(),
            button: MouseButton::Left,
            buttons: 0,
            modifiers: KeyModifiers::default(),
            ..Default::default()
        }),
    )
}

/// How many releases `target` is told of by `events`.
fn releases_at(events: &[SyntheticEvent], target: DomNodeId) -> usize {
    events
        .iter()
        .filter(|e| e.event_type == EventType::MouseUp && e.target == target)
        .count()
}

/// A tree where nothing is anybody's ancestor: only the same node is on its own path.
fn flat(press: DomNodeId, release: DomNodeId) -> bool {
    press == release
}

/// A press on node 3 (the thumb), which captures the pointer from its press handler, and a release over node 9, off the
/// thumb's box: the thumb is told of the release once. It was twice: the press-target capture added its own release for the
/// pressed node and the capture then retargeted the real one to the same node.
#[test]
fn a_node_that_holds_the_pointer_is_told_of_a_release_off_its_box_once() {
    let mut hover = HoverManager::new();
    let mut capture = None;
    let mut down = vec![mouse(EventType::MouseDown, node(3))];
    apply_input_captures(&mut hover, &mut capture, &mut down, &flat);
    // The press handler asked for the capture.
    capture = Some(PointerCapture {
        seat_id: 0,
        node: node(3),
    });

    let mut up = vec![mouse(EventType::MouseUp, node(9))];
    apply_input_captures(&mut hover, &mut capture, &mut up, &flat);

    assert_eq!(releases_at(&up, node(3)), 1, "the thumb hears the release once: {up:?}");
    assert_eq!(releases_at(&up, node(9)), 0, "and the node under the pointer does not");
    assert!(capture.is_none(), "the release ends the capture");
    assert!(
        hover.press_target(MouseButton::Left).is_none(),
        "and the press target is spent with it"
    );
}

/// The release is the same one when it lands on the node: nothing is added by either capture.
#[test]
fn a_release_on_the_node_that_holds_the_pointer_is_one_release() {
    let mut hover = HoverManager::new();
    let mut capture = None;
    let mut down = vec![mouse(EventType::MouseDown, node(3))];
    apply_input_captures(&mut hover, &mut capture, &mut down, &flat);
    capture = Some(PointerCapture {
        seat_id: 0,
        node: node(3),
    });

    let mut up = vec![mouse(EventType::MouseUp, node(3))];
    apply_input_captures(&mut hover, &mut capture, &mut up, &flat);

    assert_eq!(up.len(), 1, "{up:?}");
    assert_eq!(releases_at(&up, node(3)), 1);
}

/// With no explicit capture the press-target capture is what it was: the pressed node is given the release the pointer made
/// elsewhere, once, and the real release stays where it was.
#[test]
fn without_a_capture_the_pressed_node_is_still_given_its_release() {
    let mut hover = HoverManager::new();
    let mut capture = None;
    let mut down = vec![mouse(EventType::MouseDown, node(3))];
    apply_input_captures(&mut hover, &mut capture, &mut down, &flat);

    let mut up = vec![mouse(EventType::MouseUp, node(9))];
    apply_input_captures(&mut hover, &mut capture, &mut up, &flat);

    assert_eq!(releases_at(&up, node(3)), 1, "{up:?}");
    assert_eq!(releases_at(&up, node(9)), 1, "the hovered node's release is untouched");
    assert!(
        up.iter()
            .any(|e| e.target == node(3) && e.at_target_only),
        "the one given to the pressed node is delivered at it alone"
    );
}

/// A capture held by a node other than the pressed one: each is told once - the pressed node by the implicit capture (its
/// release is not on the capturing node's path), the capturing node by the explicit one.
#[test]
fn a_node_pressed_and_another_that_holds_the_pointer_are_each_told_once() {
    let mut hover = HoverManager::new();
    let mut capture = None;
    let mut down = vec![mouse(EventType::MouseDown, node(3))];
    apply_input_captures(&mut hover, &mut capture, &mut down, &flat);
    capture = Some(PointerCapture {
        seat_id: 0,
        node: node(5),
    });

    let mut up = vec![mouse(EventType::MouseUp, node(9))];
    apply_input_captures(&mut hover, &mut capture, &mut up, &flat);

    assert_eq!(releases_at(&up, node(5)), 1, "{up:?}");
    assert_eq!(releases_at(&up, node(3)), 1, "{up:?}");
    assert_eq!(releases_at(&up, node(9)), 0);
}

/// The pressed node is an ancestor of the node that holds the pointer: the release the capture moved to the holder
/// bubbles through the pressed node, so the pressed node is given nothing of its own.
#[test]
fn a_pressed_ancestor_of_the_node_that_holds_the_pointer_hears_the_release_through_it() {
    // node 1 is the parent of node 3
    let ancestors = |press: DomNodeId, release: DomNodeId| press == release || (press == node(1) && release == node(3));
    let mut hover = HoverManager::new();
    let mut capture = None;
    let mut down = vec![mouse(EventType::MouseDown, node(1))];
    apply_input_captures(&mut hover, &mut capture, &mut down, &ancestors);
    capture = Some(PointerCapture {
        seat_id: 0,
        node: node(3),
    });

    let mut up = vec![mouse(EventType::MouseUp, node(9))];
    apply_input_captures(&mut hover, &mut capture, &mut up, &ancestors);

    let releases = up.iter().filter(|e| e.event_type == EventType::MouseUp).count();
    assert_eq!(releases, 1, "one release, at the holder, bubbling through the pressed node: {up:?}");
    assert_eq!(releases_at(&up, node(3)), 1);
    assert_eq!(releases_at(&up, node(1)), 0, "none of the pressed node's own");
}

/// While the button is down the moves are the holder's, and the capture stays until the release.
#[test]
fn the_moves_go_to_the_holder_and_the_capture_stays_until_the_release() {
    let mut hover = HoverManager::new();
    let mut capture = Some(PointerCapture {
        seat_id: 0,
        node: node(3),
    });
    let mut moves = vec![mouse(EventType::MouseMove, node(9))];
    apply_input_captures(&mut hover, &mut capture, &mut moves, &flat);
    assert_eq!(moves[0].target, node(3));
    assert!(capture.is_some());
}
