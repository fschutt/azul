//! `<webview>` on Linux (X11 and Wayland): a window probes for WPE WebKit
//! when its first web view appears, and the probe's answer is what the view
//! shows and the app's `WebViewLoadFailed` callback reads.

use azul_layout::managers::webview::WebViewPlatform;

use super::webview::{platform, probe, unavailable_reason, WpeVersion, WPE_WEBKIT};
use crate::desktop::shell2::common::DlError;

fn not_found(tried: &[&str]) -> DlError {
    DlError::LibraryNotFound {
        name: tried[0].to_string(),
        tried: tried.iter().map(|name| (*name).to_string()).collect(),
        suggestion: "dlopen failed: cannot open shared object file".to_string(),
    }
}

#[test]
fn a_linux_window_probes_for_wpe_webkit_instead_of_saying_there_is_no_backend() {
    assert!(matches!(platform(), WebViewPlatform::Probe(_)));
}

#[test]
fn a_missing_wpe_webkit_names_every_library_it_tried() {
    let reason = unavailable_reason(&Err(not_found(WPE_WEBKIT)));
    assert!(reason.contains("WPE WebKit"), "{reason}");
    for name in WPE_WEBKIT {
        assert!(reason.contains(name), "{reason:?} does not name {name}");
    }
}

#[test]
fn a_missing_wpe_webkit_says_which_packages_install_it() {
    let reason = unavailable_reason(&Err(not_found(WPE_WEBKIT)));
    assert!(reason.contains("libwpewebkit-2.0-1"), "{reason}");
    assert!(reason.contains("wpebackend-fdo"), "{reason}");
}

#[test]
fn the_loader_error_text_is_not_shown_in_the_view() {
    let reason = unavailable_reason(&Err(not_found(WPE_WEBKIT)));
    assert!(!reason.contains("cannot open shared object file"), "{reason}");
    assert!(!reason.contains('\n'), "{reason:?}");
}

#[test]
fn a_wpe_webkit_without_a_function_azul_needs_names_the_function_and_the_library() {
    let reason = unavailable_reason(&Err(DlError::SymbolNotFound {
        symbol: "webkit_navigation_action_is_redirect".to_string(),
        library: "libWPEWebKit-1.0.so.3".to_string(),
        suggestion: String::new(),
    }));
    assert!(reason.contains("webkit_navigation_action_is_redirect"), "{reason}");
    assert!(reason.contains("libWPEWebKit-1.0.so.3"), "{reason}");
}

#[test]
fn an_installed_wpe_webkit_names_its_version_and_says_embedding_comes_later() {
    let reason = unavailable_reason(&Ok(WpeVersion {
        major: 2,
        minor: 50,
        micro: 1,
    }));
    assert!(reason.contains("WPE WebKit 2.50.1"), "{reason}");
    assert!(reason.contains("later"), "{reason}");
}

#[test]
fn the_probe_names_wpe_webkit_and_answers_the_same_every_time() {
    // Whether or not this machine has WPE WebKit: the answer is about it,
    // and the second call is the cached first one.
    let first = probe();
    assert!(first.as_str().contains("WPE WebKit"), "{}", first.as_str());
    assert_eq!(first.as_str(), probe().as_str());
}
