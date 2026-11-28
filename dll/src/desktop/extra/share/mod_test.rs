//! The desktop answer of the share backend: unsupported, with the reason, never a silent nothing. (The Android and iOS
//! backends are compiled for their targets only; their pure parts are azul-layout's `managers::share*` tests.)

use azul_core::share::{ShareRequest, ShareStatus};
use azul_layout::managers::app_services::AppServices;

use super::*;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[test]
fn a_desktop_says_it_has_no_share_sheet_and_why() {
    let services = AppServices::new();
    install(&services);
    assert!(!services.share.can_share());
    let reason = services.share.unavailable_reason().expect("a reason");
    assert!(reason.contains("no system share sheet"), "{reason}");
    match services.share.share(&ShareRequest::text("hello".into())) {
        ShareStatus::Unsupported(why) => assert_eq!(why.as_str(), reason),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_backend_is_installed_into_the_app_it_is_given_and_no_other() {
    let ours = AppServices::new();
    let theirs = AppServices::new();
    install(&ours);
    // the other app has no backend: its answer is the "no backend installed" one
    assert!(theirs.share.unavailable_reason().expect("a reason").contains("no platform backend"));
    assert!(ours.share.unavailable_reason().is_none() == cfg!(any(target_os = "android", target_os = "ios")));
}
