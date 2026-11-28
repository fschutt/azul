//! The panel's three views and the way between them, the chips, the styles, the window's cut.

use super::*;

fn card(name: &str, pin: Option<usize>) -> PlaceCard {
    PlaceCard { name: name.to_string(), kind: "azmaps-kind-city", at: (48.52, 9.05), pin }
}

#[test]
fn a_window_from_560_px_has_the_panel_at_the_left_and_a_narrower_one_the_sheet() {
    assert!(!is_wide(411.0), "a phone upright");
    assert!(!is_wide(559.9));
    assert!(is_wide(560.0));
    assert!(is_wide(914.0), "a phone on its side");
    assert!(is_wide(1280.0), "a desktop");
    assert!(!is_wide(f32::NAN) && !is_wide(f32::NEG_INFINITY), "a width that is no number gets the layout that fits every screen");
}

#[test]
fn the_flow_starts_on_the_search() {
    let f = Flow::default();
    assert_eq!(f.panel, Panel::Browse);
    assert!(f.place.is_none() && !f.collapsed);
}

#[test]
fn a_place_is_shown_and_the_panel_unfolds_for_it() {
    let mut f = Flow { collapsed: true, ..Flow::default() };
    f.show_place(card("Tübingen", None));
    assert_eq!(f.panel, Panel::Place);
    assert_eq!(f.place.as_ref().map(|p| p.name.as_str()), Some("Tübingen"));
    assert!(!f.collapsed);
}

#[test]
fn directions_to_a_place_and_back_to_it_and_back_to_the_search() {
    let mut f = Flow::default();
    f.show_place(card("Tübingen", None));
    f.directions();
    assert_eq!(f.panel, Panel::Directions);
    f.back();
    assert_eq!(f.panel, Panel::Place, "back from the way is the place it goes to");
    assert!(f.place.is_some());
    f.back();
    assert_eq!(f.panel, Panel::Browse, "back from the place is the search");
    assert!(f.place.is_none());
    f.back();
    assert_eq!(f.panel, Panel::Browse, "nothing is behind the search");
}

#[test]
fn directions_without_a_place_go_back_to_the_search() {
    let mut f = Flow::default();
    f.directions();
    assert_eq!(f.panel, Panel::Directions);
    f.back();
    assert_eq!(f.panel, Panel::Browse);
}

#[test]
fn clearing_goes_to_the_search_from_anywhere() {
    for start in [Panel::Browse, Panel::Place, Panel::Directions] {
        let mut f = Flow::default();
        f.show_place(card("x", Some(0)));
        f.panel = start;
        f.clear();
        assert_eq!(f.panel, Panel::Browse, "{start:?}");
        assert!(f.place.is_none(), "{start:?}");
    }
}

#[test]
fn the_panel_folds_and_comes_back() {
    let mut f = Flow::default();
    f.toggle_collapsed();
    assert!(f.collapsed);
    f.toggle_collapsed();
    assert!(!f.collapsed);
}

#[test]
fn the_card_of_a_removed_pin_goes_with_it() {
    let mut f = Flow::default();
    f.show_place(card("", Some(2)));
    f.pin_removed(Some(2));
    assert_eq!((f.panel, f.place.is_none()), (Panel::Browse, true));
}

#[test]
fn the_card_of_another_pin_follows_the_pins_that_moved_down() {
    let mut f = Flow::default();
    f.show_place(card("", Some(3)));
    f.pin_removed(Some(1));
    assert_eq!(f.place.as_ref().and_then(|p| p.pin), Some(2), "pin 3 is pin 2 once pin 1 is gone");
    f.pin_removed(Some(5));
    assert_eq!(f.place.as_ref().and_then(|p| p.pin), Some(2), "a pin after it moves nothing");
    assert_eq!(f.panel, Panel::Place);
}

#[test]
fn clearing_every_pin_takes_a_pin_card_and_leaves_a_searched_place() {
    let mut pinned = Flow::default();
    pinned.show_place(card("", Some(0)));
    pinned.pin_removed(None);
    assert_eq!(pinned.panel, Panel::Browse);
    let mut searched = Flow::default();
    searched.show_place(card("Tübingen", None));
    searched.pin_removed(None);
    assert_eq!(searched.panel, Panel::Place, "a place that is no pin stays when the pins are cleared");
}

#[test]
fn every_chip_has_a_name_a_query_and_an_icon_and_none_twice() {
    for (i, chip) in CHIPS.iter().enumerate() {
        assert!(chip.name.starts_with("azmaps-chip-") && !chip.name.ends_with("-query"), "{chip:?}");
        assert_eq!(chip.query, format!("{}-query", chip.name), "{chip:?}: the words searched sit beside the name");
        assert!(!chip.icon.is_empty());
        for other in &CHIPS[i + 1..] {
            assert_ne!(chip.name, other.name);
            assert_ne!(chip.icon, other.icon);
        }
    }
}

#[test]
fn the_styles_are_listed_with_the_default_first_and_found_by_their_settings_key() {
    assert_eq!(STYLES[0].key, "standard");
    assert_eq!(style_index(None), 0);
    assert_eq!(style_index(Some("nonsense")), 0, "a key that names no style is the default");
    for (i, style) in STYLES.iter().enumerate() {
        assert_eq!(style_index(Some(style.key)), i);
        assert!(style.name.starts_with("azmaps-style-"));
    }
}

#[test]
fn a_shared_place_is_a_link_to_the_map_it_is_drawn_on() {
    assert_eq!(
        share_link(48.5216, 9.0576),
        "https://www.openstreetmap.org/?mlat=48.521600&mlon=9.057600#map=16/48.52160/9.05760"
    );
    assert!(share_link(-33.8688, 151.2093).contains("mlat=-33.868800&mlon=151.209300"));
}
