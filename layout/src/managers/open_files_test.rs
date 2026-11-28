//! The open queue: held until the app listens, delivered in order, bounded, the current request while a handler runs.

use azul_core::{
    callbacks::CoreCallback,
    menu::CoreMenuCallback,
    open_files::{OpenAction, OpenRequest, OpenedItem, OpenedItemVec},
    refany::{OptionRefAny, RefAny},
};
use azul_css::{AzString, OptionU64};

use super::*;

fn file(name: &str) -> OpenedItem {
    OpenedItem::file(
        AzString::from(name),
        AzString::from("text/markdown"),
        OptionU64::Some(5),
        AzString::from(format!("/data/inbox/t/{name}")),
        AzString::from(format!("content://x/{name}")),
        false,
    )
}

fn request(names: &[&str], launched: bool) -> OpenRequest {
    OpenRequest {
        action: OpenAction::View,
        items: OpenedItemVec::from_vec(names.iter().map(|n| file(n)).collect()),
        launched_app: launched,
        skipped: 0,
    }
}

fn handler() -> CoreMenuCallback {
    CoreMenuCallback {
        refany: RefAny::new(0_u8),
        callback: CoreCallback { cb: 1, ctx: OptionRefAny::None },
    }
}

fn names_of(delivery: &OpenDelivery) -> Vec<String> {
    delivery.request.items.as_ref().iter().map(|i| i.name.as_str().to_string()).collect()
}

#[test]
fn a_request_with_nothing_in_it_is_not_a_request() {
    let queue = OpenQueue::new();
    assert!(!queue.push(request(&[], false)));
    assert_eq!(queue.held(), 0);
}

#[test]
fn requests_wait_for_the_handler_and_are_then_delivered_in_arrival_order() {
    let queue = OpenQueue::new();
    assert!(queue.push(request(&["first.md"], true)));
    assert!(queue.push(request(&["second.md", "third.md"], false)));
    // no handler yet: nothing is delivered and nothing is lost
    assert!(queue.take_deliveries().is_empty());
    assert_eq!(queue.held(), 2);
    queue.set_handler(handler());
    let deliveries = queue.take_deliveries();
    assert_eq!(deliveries.len(), 2);
    assert_eq!(names_of(&deliveries[0]), ["first.md"]);
    assert!(deliveries[0].request.launched_app);
    assert_eq!(names_of(&deliveries[1]), ["second.md", "third.md"]);
    assert_eq!(queue.held(), 0, "delivered once");
    assert!(queue.take_deliveries().is_empty());
}

#[test]
fn an_app_that_never_listens_keeps_the_newest_requests_and_counts_the_rest() {
    let queue = OpenQueue::new();
    for i in 0..(MAX_HELD_REQUESTS + 3) {
        assert!(queue.push(request(&[&format!("f{i}.md")], false)));
    }
    assert_eq!(queue.held(), MAX_HELD_REQUESTS);
    assert_eq!(queue.dropped(), 3);
    queue.set_handler(handler());
    let first = &queue.take_deliveries()[0];
    assert_eq!(names_of(first), ["f3.md"], "the oldest went");
}

#[test]
fn deliveries_without_a_window_go_back_to_the_front_in_their_order() {
    let queue = OpenQueue::new();
    queue.set_handler(handler());
    queue.push(request(&["a.md"], true));
    queue.push(request(&["b.md"], false));
    let taken = queue.take_deliveries();
    assert_eq!(taken.len(), 2);
    // a newer request came in while those had no window to run against
    queue.push(request(&["c.md"], false));
    queue.defer(taken);
    let again: Vec<Vec<String>> = queue.take_deliveries().iter().map(names_of).collect();
    assert_eq!(again, [vec!["a.md"], vec!["b.md"], vec!["c.md"]]);
}

#[test]
fn a_full_queue_that_gets_deliveries_back_drops_the_newest_to_keep_the_order_of_what_was_sent() {
    let queue = OpenQueue::new();
    queue.set_handler(handler());
    queue.push(request(&["old.md"], false));
    let taken = queue.take_deliveries();
    for i in 0..MAX_HELD_REQUESTS {
        queue.push(request(&[&format!("n{i}.md")], false));
    }
    queue.defer(taken);
    assert_eq!(queue.held(), MAX_HELD_REQUESTS);
    assert_eq!(queue.dropped(), 1);
    let all = queue.take_deliveries();
    assert_eq!(names_of(&all[0]), ["old.md"], "what was sent first is still first");
}

#[test]
fn the_current_request_is_the_one_a_delivery_runs_for_and_is_restored() {
    let queue = OpenQueue::new();
    assert!(queue.current().is_none());
    let outer = request(&["outer.md"], false);
    let inner = request(&["inner.md"], true);
    let seen = queue.with_current(&outer, || {
        let nested = queue.with_current(&inner, || queue.current());
        (nested, queue.current())
    });
    assert_eq!(seen.0.unwrap(), inner);
    assert_eq!(seen.1.unwrap(), outer);
    assert!(queue.current().is_none(), "nothing outside a delivery");
}

#[test]
fn the_current_request_is_restored_when_the_handler_panics() {
    let queue = OpenQueue::new();
    let boom = request(&["boom.md"], false);
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        queue.with_current(&boom, || panic!("a handler failed"));
    }));
    assert!(caught.is_err());
    assert!(queue.current().is_none());
}

#[test]
fn clones_are_one_queue() {
    let queue = OpenQueue::new();
    let other = queue.clone();
    other.set_handler(handler());
    queue.push(request(&["x.md"], false));
    assert_eq!(other.take_deliveries().len(), 1);
    assert!(queue.handler().as_ref().is_some());
}
