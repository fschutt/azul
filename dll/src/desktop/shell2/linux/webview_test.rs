//! `<webview>` on Linux (X11 and Wayland): WPE `WebKit`, composited - the
//! page is rendered offscreen into shared memory, the window draws each
//! frame and routes the page's input to it.
//!
//! These are the backend's own conversions (pixels, input, failures) and
//! what a window says when WPE `WebKit` does not load: the window side is
//! the engine's (`layout/tests/a_composited_webview*`).

use azul_core::{
    events::{KeyModifiers, MouseButton},
    geom::LogicalPosition,
};
use azul_layout::managers::webview::WebViewPlatform;

use super::webview::{
    bgra_from_shm, device_point, is_quiet_failure, platform, unavailable_reason, wheel_axes,
    wpe_button, wpe_modifiers, SHM_FORMAT_ARGB8888, SHM_FORMAT_XRGB8888, WPE_WEBKIT,
};
use crate::desktop::shell2::common::DlError;

fn not_found(tried: &[&str]) -> DlError {
    DlError::LibraryNotFound {
        name: tried[0].to_string(),
        tried: tried.iter().map(|name| (*name).to_string()).collect(),
        suggestion: "dlopen failed: cannot open shared object file".to_string(),
    }
}

#[test]
fn a_linux_window_composites_its_web_views() {
    assert!(matches!(platform(), WebViewPlatform::Composited));
}

#[test]
fn a_missing_wpe_webkit_names_every_library_it_tried() {
    let reason = unavailable_reason(&not_found(WPE_WEBKIT));
    assert!(reason.contains("WPE WebKit"), "{reason}");
    for name in WPE_WEBKIT {
        assert!(reason.contains(name), "{reason:?} does not name {name}");
    }
}

#[test]
fn a_missing_wpe_webkit_says_which_packages_install_it() {
    let reason = unavailable_reason(&not_found(WPE_WEBKIT));
    assert!(reason.contains("libwpewebkit-2.0-1"), "{reason}");
    assert!(reason.contains("wpebackend-fdo"), "{reason}");
}

#[test]
fn the_loader_error_text_is_not_shown_in_the_view() {
    let reason = unavailable_reason(&not_found(WPE_WEBKIT));
    assert!(!reason.contains("cannot open shared object file"), "{reason}");
    assert!(!reason.contains('\n'), "{reason:?}");
}

#[test]
fn a_wpe_webkit_without_a_function_azul_needs_names_the_function_and_the_library() {
    let reason = unavailable_reason(&DlError::SymbolNotFound {
        symbol: "webkit_navigation_action_is_redirect".to_string(),
        library: "libWPEWebKit-1.0.so.3".to_string(),
        suggestion: String::new(),
    });
    assert!(reason.contains("webkit_navigation_action_is_redirect"), "{reason}");
    assert!(reason.contains("libWPEWebKit-1.0.so.3"), "{reason}");
}

/// A 2x2 frame whose rows are 12 bytes apart (4 bytes of padding each).
fn padded_frame() -> Vec<u8> {
    vec![
        1, 2, 3, 4, 5, 6, 7, 8, 0xEE, 0xEE, 0xEE, 0xEE, //
        9, 10, 11, 12, 13, 14, 15, 16, 0xEE, 0xEE, 0xEE, 0xEE,
    ]
}

#[test]
fn a_shared_memory_frame_is_copied_row_by_row_without_its_padding() {
    let pixels = bgra_from_shm(2, 2, 12, SHM_FORMAT_ARGB8888, &padded_frame())
        .expect("a well-formed frame");
    assert_eq!(
        pixels,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        "ARGB8888 in memory is B, G, R, A: kept as it is"
    );
}

#[test]
fn an_opaque_frame_gets_an_opaque_alpha() {
    let pixels = bgra_from_shm(2, 2, 12, SHM_FORMAT_XRGB8888, &padded_frame())
        .expect("a well-formed frame");
    assert_eq!(
        pixels,
        vec![1, 2, 3, 255, 5, 6, 7, 255, 9, 10, 11, 255, 13, 14, 15, 255],
        "XRGB8888's fourth byte is undefined: it becomes 255"
    );
}

#[test]
fn a_frame_shorter_than_it_says_is_dropped() {
    assert_eq!(
        bgra_from_shm(2, 2, 12, SHM_FORMAT_ARGB8888, &padded_frame()[..16]),
        None
    );
    assert_eq!(bgra_from_shm(2, 2, 4, SHM_FORMAT_ARGB8888, &padded_frame()), None);
    assert_eq!(bgra_from_shm(2, 2, 12, 0x3432_5258, &padded_frame()), None);
}

#[test]
fn page_points_are_device_pixels_for_wpe() {
    assert_eq!(device_point(LogicalPosition::new(10.5, 20.0), 2.0), (21, 40));
    assert_eq!(device_point(LogicalPosition::new(10.5, 20.0), 1.0), (10, 20));
}

#[test]
fn wpe_numbers_the_buttons_left_right_middle() {
    assert_eq!(wpe_button(MouseButton::Left), 1);
    assert_eq!(wpe_button(MouseButton::Right), 2);
    assert_eq!(wpe_button(MouseButton::Middle), 3);
}

#[test]
fn modifiers_and_held_buttons_share_wpes_mask() {
    let held = KeyModifiers {
        shift: true,
        ctrl: true,
        alt: false,
        meta: true,
    };
    assert_eq!(
        wpe_modifiers(held, &[MouseButton::Left, MouseButton::Middle]),
        (1 << 0) | (1 << 1) | (1 << 3) | (1 << 20) | (1 << 22)
    );
    assert_eq!(wpe_modifiers(KeyModifiers::default(), &[]), 0);
}

#[test]
fn a_wheel_towards_the_user_scrolls_the_page_down() {
    // The window's delta: positive y scrolls the content up (the page
    // down); WebKit's: positive y scrolls towards the top.
    let (x, y) = wheel_axes(LogicalPosition::new(0.0, 30.0), 2.0);
    assert!(x == 0.0 && y < 0.0, "({x}, {y})");
    assert!((y + 60.0).abs() < 1e-9, "in device pixels: {y}");
}

#[test]
fn a_navigation_somebody_cancelled_is_no_failure() {
    assert!(is_quiet_failure("WebKitNetworkError", 302), "cancelled");
    assert!(
        is_quiet_failure("WebKitPolicyError", 102),
        "interrupted by a policy decision (the app said no)"
    );
    assert!(!is_quiet_failure("WebKitNetworkError", 399), "a transport failure");
    assert!(!is_quiet_failure("g-resolver-error-quark", 0), "no such host");
}
