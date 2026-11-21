use azul_core::{events::SessionChange, window::VirtualKeyCode};
use rust_fontconfig::FcFontCache;

use super::*;
use crate::managers::{biometric::BiometricPrompt, notification::NotificationRequest};

fn window() -> LayoutWindow {
    LayoutWindow::new(FcFontCache::default()).expect("a layout window")
}

#[test]
fn two_sets_share_nothing() {
    let ours = AppServices::new();
    let theirs = AppServices::new();
    ours.biometric
        .requests
        .push(BiometricPrompt::new("Unlock".into()));
    ours.media_keys.push_media_key(VirtualKeyCode::PlayPause);
    ours.session.push(SessionChange::Locked);
    assert!(theirs.biometric.requests.is_empty());
    assert!(theirs.media_keys.drain_media_keys().is_empty());
    assert_eq!(theirs.session.position(), 0);
    assert!(!theirs.hotkeys.ptr_eq(&ours.hotkeys));
}

#[test]
fn a_clone_is_the_same_set() {
    let services = AppServices::new();
    let clone = services.clone();
    clone.raise.request_raise(9);
    assert!(services.raise.take_raise_request(9));
    assert!(services.hotkeys.ptr_eq(&clone.hotkeys));
}

#[test]
fn a_window_that_joined_uses_its_apps_queues() {
    let services = AppServices::new();
    let mut first = window();
    let mut second = window();
    first.join_app(&services);
    second.join_app(&services);
    first
        .app_services
        .biometric
        .requests
        .push(BiometricPrompt::new("From the first window".into()));
    // The app's queue, so a pump on the second window sees the first's request.
    assert_eq!(second.app_services.biometric.requests.drain().len(), 1);
    assert!(services.biometric.requests.is_empty());
    assert!(first
        .app_services
        .hotkeys
        .ptr_eq(&second.app_services.hotkeys));
    assert!(first.global_hotkeys.shared().ptr_eq(&services.hotkeys));
    assert_ne!(
        first.global_hotkeys.seq(),
        second.global_hotkeys.seq(),
        "the app numbers its windows"
    );
}

#[test]
fn a_window_that_never_joined_has_queues_of_its_own() {
    let mut loner = window();
    let other = window();
    assert!(loner
        .app_services
        .notifications
        .push_notification_request(NotificationRequest::Withdraw("a".into())));
    assert!(!other.app_services.notifications.has_queued_requests());
    assert!(loner.app_services.notifications.has_queued_requests());
    assert!(!loner
        .global_hotkeys
        .shared()
        .ptr_eq(other.global_hotkeys.shared()));
    // Joining drops what it queued before, as its docs say.
    loner.join_app(&AppServices::new());
    assert!(!loner.app_services.notifications.has_queued_requests());
}

#[test]
fn a_window_hears_the_session_changes_after_it_joined_only() {
    let services = AppServices::new();
    services.session.push(SessionChange::Locked);
    let mut late = window();
    late.join_app(&services);
    services.session.push(SessionChange::Unlocked);
    late.device_event_manager
        .read_session_log(&late.app_services.session);
    assert_eq!(
        late.device_event_manager.take_pending_session(),
        vec![SessionChange::Unlocked]
    );
}
