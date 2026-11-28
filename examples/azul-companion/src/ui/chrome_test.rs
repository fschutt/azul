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

#[test]
fn a_card_over_the_screen_stands_clear_of_the_bars_at_the_sides_in_every_phone_and_way_round() {
    // the study found it: at a flat 24 px from the edge the banner of an agent that finished lay under the notch of an iPhone held sideways and
    // under the navigation bar of an Android phone held sideways ("text-outside-safe-area")
    for device in &DEVICES {
        for landscape in [false, true] {
            let env = device.env(landscape, Lang::En, true);
            let data = RefAny::new(Root::new(Box::new(OfflinePhone::new()), EnvSource::Fixed(env)));
            let mut shared = data.clone();
            let root = shared.downcast_ref::<Root>().expect("the app state");
            let cx = Cx { root: &root, env: &env, data: &data, z: crate::layout::zones_with(&env, false) };
            let (left, right) = card_sides(&cx);
            assert_eq!(left, env.insets.left + CARD_SIDE, "{} landscape {landscape}", device.id);
            assert_eq!(right, env.insets.right + CARD_SIDE, "{} landscape {landscape}", device.id);
            assert!(left >= env.insets.left + 24.0 && right >= env.insets.right + 24.0);
        }
    }
    // the two phones with something at a side: the notch of the iPhone and the navigation bar of the small Android, both sideways
    let phone = DEVICES[0].env(true, Lang::En, true);
    assert_eq!((phone.insets.left, phone.insets.right), (59.0, 59.0));
    let small = DEVICES[1].env(true, Lang::En, true);
    assert_eq!(small.insets.right, 48.0);
}
