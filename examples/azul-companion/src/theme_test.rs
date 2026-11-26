//! The colours.

use super::*;

#[test]
fn a_tint_is_the_colour_at_an_alpha_as_rgba() {
    assert_eq!(tint("#0a84ff", 0.5), "rgba(10, 132, 255, 0.50)");
    assert_eq!(tint("#ffffff", 1.0), "rgba(255, 255, 255, 1.00)");
    assert_eq!(tint("#000000", 0.0), "rgba(0, 0, 0, 0.00)");
}

#[test]
fn an_alpha_outside_zero_to_one_is_held() {
    assert_eq!(tint("#102030", 7.0), "rgba(16, 32, 48, 1.00)");
    assert_eq!(tint("#102030", -1.0), "rgba(16, 32, 48, 0.00)");
}

#[test]
fn a_colour_that_is_not_one_is_black_and_never_panics() {
    assert_eq!(tint("", 0.5), "rgba(0, 0, 0, 0.50)");
    assert_eq!(tint("#zz", 0.5), "rgba(0, 0, 0, 0.50)");
    assert_eq!(tint("#12", 0.5), "rgba(18, 0, 0, 0.50)");
}

#[test]
fn the_pens_are_the_five_of_azreview_and_the_buttons_take_the_first_four_in_turn() {
    assert_eq!(PENS.len(), 5);
    assert_eq!(PENS[0], "#ff4098", "AzReview's scope pen is (255, 64, 152)");
    assert_eq!(PENS[1], "#d62d20");
    assert_eq!(PENS[2], "#205cd6");
    assert_eq!(PENS[3], "#963cc8");
    assert_eq!(PENS[4], "#188c46");
    assert_eq!([button_colour(0), button_colour(1), button_colour(2), button_colour(3)], [PENS[0], PENS[1], PENS[2], PENS[3]]);
    assert_eq!(button_colour(4), PENS[0], "a fifth would start over");
}

#[test]
fn every_colour_is_a_hex_colour() {
    for colour in [BG, SURFACE, SURFACE_HI, SEPARATOR, TEXT, TEXT_DIM, ON_ACCENT, ACCENT, WARN, DANGER, STATE_WORKING, STATE_SUGGESTED, STATE_DONE, PEN_DOT]
        .into_iter()
        .chain(PENS)
    {
        assert!(colour.len() == 7 && colour.starts_with('#') && colour[1..].chars().all(|c| c.is_ascii_hexdigit()), "{colour}");
    }
}
