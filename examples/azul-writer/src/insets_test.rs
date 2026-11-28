//! The padding that keeps AzWriter's content clear of the system bars when the window draws under them. No window.

use azul::dom::Dom;

use super::Insets;

/// A Pixel 8 held upright: the status bar on top, the gesture bar at the bottom.
fn pixel() -> Insets {
    Insets::new(24.0, 0.0, 16.0, 0.0)
}

/// An iPhone on its side: the notch is at the left, the home indicator at the bottom.
fn iphone_on_its_side() -> Insets {
    Insets::new(0.0, 0.0, 21.0, 47.0)
}

#[test]
fn a_desktop_window_has_no_inset_and_nothing_is_wrapped() {
    let none = Insets::default();
    assert!(none.is_none());
    // two children: a box around the content would have one
    let content = Dom::create_div().with_child(Dom::create_div()).with_child(Dom::create_div());
    let wrapped = none.around(content, true);
    assert_eq!(wrapped.children.as_ref().len(), 2, "the content is returned as it is");
}

#[test]
fn a_side_that_is_not_a_number_of_pixels_is_no_inset() {
    let insets = Insets::new(f32::NAN, -4.0, f32::INFINITY, 12.0);
    assert_eq!(insets, Insets { top: 0.0, right: 0.0, bottom: 0.0, left: 12.0 });
}

#[test]
fn everything_but_the_strip_keeps_clear_of_all_four_sides() {
    assert_eq!(pixel().padding(true), "padding: 24px 0px 16px 0px;");
    assert_eq!(iphone_on_its_side().padding(true), "padding: 0px 0px 21px 47px;");
}

#[test]
fn what_stands_under_the_strip_leaves_the_top_to_it() {
    // the strip stands in the status bar's band: padding the top again would push everything down by the band's height
    assert_eq!(pixel().padding(false), "padding: 0px 0px 16px 0px;");
}

#[test]
fn a_window_with_only_a_top_inset_needs_no_wrapper_under_the_strip() {
    // the strip is the top; there is nothing else to keep clear of
    let only_top = Insets::new(24.0, 0.0, 0.0, 0.0);
    let content = Dom::create_div();
    let wrapped = only_top.around(content, false);
    assert!(wrapped.children.as_ref().is_empty(), "the content itself, not a box around it");
}

#[test]
fn content_with_insets_is_wrapped_in_a_box_that_holds_it() {
    let wrapped = pixel().around(Dom::create_div(), false);
    assert_eq!(wrapped.children.as_ref().len(), 1, "one box, the content in it");
}
