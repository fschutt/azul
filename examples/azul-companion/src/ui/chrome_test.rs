//! The phone's block as a tree: what it asks the engine to tell it. (The pictures and the boxes are the layout study's.)

use super::*;
use crate::env::{Lang, DEVICES};
use crate::offline::OfflinePhone;
use crate::ui::{EnvSource, Root};

/// How many callbacks of the node ask to be told that it was mounted.
fn mount_callbacks(dom: &Dom) -> usize {
    dom.root.callbacks.iter().filter(|c| matches!(c.event, EventFilter::Component(ComponentEventFilter::AfterMount))).count()
}

#[test]
fn the_phones_block_asks_to_be_told_when_it_is_mounted_because_the_phones_shells_never_say_the_window_was_created() {
    let env = DEVICES[0].env(false, Lang::En, true);
    let data = RefAny::new(Root::new(Box::new(OfflinePhone::new()), EnvSource::Fixed(env)));
    let mut shared = data.clone();
    let root = shared.downcast_ref::<Root>().expect("the app state");
    let cx = Cx { root: &root, env: &env, data: &data, z: crate::layout::zones_with(&env, false) };
    let dom = phone(&cx);
    assert_eq!(mount_callbacks(&dom), 1, "one, not none (the timer and the connection would never start) and not two");
}
