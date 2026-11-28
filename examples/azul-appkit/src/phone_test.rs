use super::*;

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
