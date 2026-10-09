//! `<webview>` on Windows: a window probes for Microsoft Edge `WebView2` (the
//! loader the app ships and the runtime the system has) when its first web
//! view appears, and the probe's answer is what the view shows and the app's
//! `WebViewLoadFailed` callback reads.

use azul_layout::managers::webview::WebViewPlatform;

use super::webview::{platform, probe, unavailable_reason, WebView2, WEBVIEW2_LOADER};
use crate::desktop::shell2::common::DlError;

#[test]
fn a_windows_window_probes_for_webview2_instead_of_saying_there_is_no_backend() {
    assert!(matches!(platform(), WebViewPlatform::Probe(_)));
}

#[test]
fn a_missing_loader_says_the_app_ships_it_next_to_the_program() {
    let reason = unavailable_reason(&WebView2::LoaderMissing(DlError::LibraryNotFound {
        name: WEBVIEW2_LOADER.to_string(),
        tried: vec![WEBVIEW2_LOADER.to_string()],
        suggestion: "LoadLibraryW failed".to_string(),
    }));
    assert!(reason.contains(WEBVIEW2_LOADER), "{reason}");
    assert!(reason.contains("next to the program"), "{reason}");
    assert!(!reason.contains("LoadLibraryW"), "{reason}");
}

#[test]
fn a_loader_without_the_runtime_says_the_runtime_is_missing() {
    let reason = unavailable_reason(&WebView2::NoRuntime);
    assert!(reason.contains("WebView2 Runtime"), "{reason}");
    assert!(reason.contains("not installed"), "{reason}");
}

#[test]
fn an_installed_runtime_names_its_version_and_says_embedding_comes_later() {
    let reason = unavailable_reason(&WebView2::Runtime("129.0.2792.65".to_string()));
    assert!(reason.contains("129.0.2792.65"), "{reason}");
    assert!(reason.contains("later"), "{reason}");
}

#[test]
fn the_probe_names_webview2_and_answers_the_same_every_time() {
    let first = probe();
    assert!(first.as_str().contains("WebView2"), "{}", first.as_str());
    assert_eq!(first.as_str(), probe().as_str());
}
