//! `<webview>` on Windows: Microsoft Edge `WebView2`, a controller per view
//! on a clip child window of the azul window. These are the backend's own
//! conversions and what a window says when `WebView2` is missing: the
//! window side is the engine's (`layout/tests/a_webview*`).

use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    webview::WebViewStorage,
};
use azul_layout::managers::webview::{WebViewPlacement, WebViewPlatform};

use super::{
    dlopen::RECT,
    webview::{
        is_quiet_failure, layout_in_container, physical, platform, unavailable_reason,
        user_data_folder, wide, WebView2, WEBVIEW2_LOADER,
    },
};
use crate::desktop::shell2::common::DlError;

fn edges(r: RECT) -> (i32, i32, i32, i32) {
    (r.left, r.top, r.right, r.bottom)
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
}

#[test]
fn a_windows_window_hosts_its_web_views_in_webview2() {
    assert!(matches!(platform(), WebViewPlatform::Backend));
}

#[test]
fn a_missing_loader_says_the_app_ships_it_next_to_the_program() {
    let reason = unavailable_reason(&WebView2::LoaderMissing(DlError::LibraryNotFound {
        name: WEBVIEW2_LOADER.to_string(),
        tried: vec![WEBVIEW2_LOADER.to_string()],
        suggestion: "LoadLibraryW failed".to_string(),
    }))
    .expect("missing");
    assert!(reason.contains(WEBVIEW2_LOADER), "{reason}");
    assert!(reason.contains("next to the program"), "{reason}");
    assert!(!reason.contains("LoadLibraryW"), "{reason}");
}

#[test]
fn a_loader_without_the_runtime_says_the_runtime_is_missing() {
    let reason = unavailable_reason(&WebView2::NoRuntime).expect("missing");
    assert!(reason.contains("WebView2 Runtime"), "{reason}");
    assert!(reason.contains("not installed"), "{reason}");
}

#[test]
fn an_installed_runtime_is_nothing_missing() {
    assert_eq!(unavailable_reason(&WebView2::Runtime("129.0.2792.65".to_string())), None);
}

#[test]
fn a_logical_rect_becomes_whole_device_pixels_edge_by_edge() {
    assert_eq!(edges(physical(rect(10.0, 20.0, 100.0, 50.0), 1.5)), (15, 30, 165, 105));
    // Edges round on their own, so two boxes that touch still touch.
    assert_eq!(edges(physical(rect(0.3, 0.0, 0.4, 1.0), 1.0)), (0, 0, 1, 1));
}

#[test]
fn the_clip_window_is_the_visible_part_and_the_page_keeps_its_box_inside_it() {
    // A 300x150 box scrolled 100px up under a clip at (0, 0) 300x100.
    let placement = WebViewPlacement {
        rect: rect(0.0, -50.0, 300.0, 150.0),
        clip: rect(0.0, 0.0, 300.0, 100.0),
        visible: true,
    };
    let (container, page) = layout_in_container(&placement, 2.0);
    assert_eq!(edges(container), (0, 0, 600, 200), "the clip, in the window");
    assert_eq!(edges(page), (0, -100, 600, 200), "the whole box, in the clip window");
}

#[test]
fn a_navigation_somebody_cancelled_is_no_failure() {
    assert!(is_quiet_failure(14), "COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED");
    assert!(!is_quiet_failure(13), "a host name that does not resolve is one");
    assert!(!is_quiet_failure(0), "an unknown error is one");
}

#[test]
fn a_wide_string_ends_in_a_nul() {
    assert_eq!(wide("ab"), vec![u16::from(b'a'), u16::from(b'b'), 0]);
    assert_eq!(wide(""), vec![0]);
}

#[test]
fn an_ephemeral_store_is_a_folder_of_its_own_and_a_persistent_one_is_the_apps() {
    let persistent = user_data_folder(
        WebViewStorage::Persistent,
        r"C:\Users\u\AppData\Local",
        r"C:\Temp",
        "AzDrive",
        42,
    );
    assert_eq!(persistent, r"C:\Users\u\AppData\Local\AzDrive\WebView2");
    let ephemeral = user_data_folder(
        WebViewStorage::Ephemeral,
        r"C:\Users\u\AppData\Local",
        r"C:\Temp",
        "AzDrive",
        42,
    );
    assert_eq!(ephemeral, r"C:\Temp\azul-webview-AzDrive-42");
}
