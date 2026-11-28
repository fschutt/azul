//! The notice that the system's style changed (`SystemStyleNotice`): one `SystemStyleChange` event on
//! the root per change, nothing while nothing changed, nothing again once a pass has collected it.

use azul_core::{
    events::{EventProvider, EventType},
    task::{Instant, SystemTick},
};

use crate::event_determination::SystemStyleNotice;

fn now() -> Instant {
    Instant::Tick(SystemTick::new(0))
}

#[test]
fn a_fresh_notice_says_nothing() {
    let notice = SystemStyleNotice::default();
    assert!(!notice.is_pending());
    assert!(notice.get_pending_events(now()).is_empty());
}

#[test]
fn a_raised_notice_is_one_event_on_the_root_until_a_pass_collects_it() {
    let mut notice = SystemStyleNotice::default();
    notice.raise();
    assert!(notice.is_pending());
    let events = notice.get_pending_events(now());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::SystemStyleChange);
    assert_eq!(events[0].target.dom.inner, 0, "the window's one DOM");
    assert_eq!(
        events[0].target.node.into_crate_internal(),
        Some(azul_core::id::NodeId::ZERO),
        "the root: the window's listeners hear it"
    );
    // `get_pending_events` takes `&self`: asking again before the pass is through says the same
    assert_eq!(notice.get_pending_events(now()).len(), 1);
    // raised twice in one frame is still one event
    notice.raise();
    assert_eq!(notice.get_pending_events(now()).len(), 1);

    notice.clear_pending_event();
    assert!(!notice.is_pending());
    assert!(notice.get_pending_events(now()).is_empty(), "told once: the next pass starts silent");
}
