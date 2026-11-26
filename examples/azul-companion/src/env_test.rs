//! The phone's surroundings: sizes, safe areas, languages.

use super::*;

#[test]
fn a_device_in_landscape_swaps_its_sides_and_takes_its_landscape_insets() {
    let phone = &DEVICES[0];
    let portrait = phone.env(false, Lang::En, true);
    let landscape = phone.env(true, Lang::En, true);
    assert_eq!((portrait.width, portrait.height), (393.0, 852.0));
    assert_eq!((landscape.width, landscape.height), (852.0, 393.0));
    assert!(!portrait.landscape() && landscape.landscape());
    assert_eq!(portrait.insets.top, 59.0);
    assert_eq!(landscape.insets.top, 0.0);
    assert_eq!((landscape.insets.left, landscape.insets.right), (59.0, 59.0), "the notch sits at a side");
}

#[test]
fn without_the_safe_area_a_device_has_no_insets() {
    for device in &DEVICES {
        for landscape in [false, true] {
            assert_eq!(device.env(landscape, Lang::De, false).insets, Insets::NONE);
        }
    }
}

#[test]
fn the_room_inside_the_safe_area_is_what_is_left() {
    let env = DEVICES[0].env(false, Lang::En, true);
    assert_eq!(env.usable(), (393.0, 852.0 - 59.0 - 34.0));
    let env = DEVICES[0].env(true, Lang::En, true);
    assert_eq!(env.usable(), (852.0 - 118.0, 393.0 - 21.0));
}

#[test]
fn the_four_sizes_of_the_study_are_there() {
    let sizes: Vec<(f32, f32)> = DEVICES.iter().flat_map(|d| [d.env(false, Lang::En, true), d.env(true, Lang::En, true)]).map(|e| (e.width, e.height)).collect();
    for wanted in [(393.0, 852.0), (852.0, 393.0), (360.0, 800.0), (412.0, 915.0)] {
        assert!(sizes.contains(&wanted), "{wanted:?} is missing");
    }
    assert!(Device::by_id("android-s").is_some() && Device::by_id("nokia").is_none());
}

#[test]
fn insets_that_are_nonsense_are_held_and_never_eat_the_screen() {
    let held = Insets { top: 9999.0, right: -4.0, bottom: f32::NAN, left: f32::INFINITY }.held(400.0, 800.0);
    assert_eq!(held, Insets { top: 200.0, right: 0.0, bottom: 0.0, left: 0.0 });
    let fine = Insets { top: 59.0, right: 0.0, bottom: 34.0, left: 0.0 };
    assert_eq!(fine.held(393.0, 852.0), fine);
}

#[test]
fn a_locale_tag_is_german_or_english() {
    assert_eq!(Lang::from_tag("de-DE"), Lang::De);
    assert_eq!(Lang::from_tag("de"), Lang::De);
    assert_eq!(Lang::from_tag(" DE_at "), Lang::De);
    assert_eq!(Lang::from_tag("en-US"), Lang::En);
    assert_eq!(Lang::from_tag("fr-FR"), Lang::En, "any other language is English for now");
    assert_eq!(Lang::from_tag(""), Lang::En);
    assert_eq!(Lang::from_tag(Lang::De.tag()), Lang::De);
}

#[test]
fn a_short_screen_is_one_with_less_than_640_logical_pixels_to_use() {
    assert!(!DEVICES[0].env(false, Lang::En, true).short());
    assert!(DEVICES[0].env(true, Lang::En, true).short());
}
