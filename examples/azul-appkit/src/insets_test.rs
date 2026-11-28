use super::*;

#[test]
fn a_side_that_is_no_pixel_count_is_no_inset() {
    let insets = SafeInsets::new(f32::NAN, -4.0, f32::INFINITY, 12.0);
    assert_eq!(insets, SafeInsets { top: 0.0, right: 0.0, bottom: 0.0, left: 12.0 });
    assert!(!insets.is_none());
    assert!(SafeInsets::new(0.0, 0.0, 0.0, 0.0).is_none());
    assert!(SafeInsets::default().is_none());
}

#[test]
fn the_padding_keeps_content_clear_of_every_side() {
    assert_eq!(
        SafeInsets::new(47.0, 0.0, 34.0, 8.0).padding(),
        "padding: 47px 0px 34px 8px;"
    );
}

#[test]
fn a_phone_is_a_touch_window_that_is_narrow_or_short() {
    // Portrait, landscape, a small phone, a tablet in both ways, a desktop.
    assert!(is_phone_window(true, 412.0, 915.0));
    assert!(is_phone_window(true, 915.0, 412.0), "a phone on its side is short");
    assert!(is_phone_window(true, 360.0, 640.0));
    assert!(!is_phone_window(true, 1024.0, 768.0));
    assert!(!is_phone_window(true, 768.0, 1024.0));
    assert!(!is_phone_window(false, 412.0, 915.0), "a desktop window of a phone's size is a desktop's");
}
