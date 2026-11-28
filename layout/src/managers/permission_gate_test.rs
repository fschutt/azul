use azul_core::dom::{DomId, DomNodeId, NodeId};

use super::*;

fn node(idx: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeId::from_usize(idx).into(),
    }
}

fn subscribed(capability: Capability) -> PermissionManager {
    let mut mgr = PermissionManager::new();
    mgr.subscribe(capability, node(1));
    mgr
}

fn subscribe_count(events: &[PermissionDiffEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, PermissionDiffEvent::Subscribe { .. }))
        .count()
}

#[test]
fn every_capability_is_in_the_list_of_all_of_them() {
    assert_eq!(Capability::ALL.len(), Capability::InputMonitoring as usize + 1);
    for (index, capability) in Capability::ALL.iter().enumerate() {
        assert_eq!(*capability as usize, index, "in declaration order");
    }
}

#[test]
fn a_subscribe_before_any_gesture_never_reaches_the_platform() {
    // A probe in the first DOM: it asks nobody at launch.
    let mut mgr = subscribed(Capability::Geolocation);
    assert!(mgr.take_dispatchable().is_empty());
    assert_eq!(mgr.held_capabilities(), vec![Capability::Geolocation]);
    // Passes without a gesture keep it held, once.
    assert!(mgr.take_dispatchable().is_empty());
    assert_eq!(mgr.held_capabilities(), vec![Capability::Geolocation]);
}

#[test]
fn the_users_gesture_lets_the_held_subscribe_go_once() {
    let mut mgr = subscribed(Capability::Geolocation);
    assert!(mgr.take_dispatchable().is_empty());
    mgr.note_gesture();
    let events = mgr.take_dispatchable();
    assert_eq!(subscribe_count(&events), 1);
    assert!(mgr.held_capabilities().is_empty());
    // The gesture is used up: nothing goes twice.
    assert!(mgr.take_dispatchable().is_empty());
}

#[test]
fn a_subscribe_in_the_pass_of_the_gesture_goes_at_once() {
    // The button's callback mounts the probe: the gesture and the Subscribe are of one pass.
    let mut mgr = PermissionManager::new();
    mgr.note_gesture();
    mgr.subscribe(Capability::Camera, node(2));
    assert_eq!(subscribe_count(&mgr.take_dispatchable()), 1);
}

#[test]
fn one_capability_is_asked_per_gesture() {
    let mut mgr = PermissionManager::new();
    mgr.subscribe(Capability::Camera, node(1));
    mgr.subscribe(Capability::Microphone, node(2));
    mgr.note_gesture();
    let first = mgr.take_dispatchable();
    assert_eq!(subscribe_count(&first), 1, "never two prompts at once");
    assert_eq!(mgr.held_capabilities().len(), 1);
    // The other waits for the user's next gesture.
    assert!(mgr.take_dispatchable().is_empty());
    mgr.note_gesture();
    assert_eq!(subscribe_count(&mgr.take_dispatchable()), 1);
    assert!(mgr.held_capabilities().is_empty());
}

#[test]
fn a_denied_capability_is_not_asked_again() {
    for decided in [PermissionState::Denied, PermissionState::Restricted] {
        let mut mgr = PermissionManager::new();
        mgr.set_status(Capability::Camera, decided);
        mgr.subscribe(Capability::Camera, node(1));
        mgr.note_gesture();
        assert!(mgr.take_dispatchable().is_empty(), "{decided:?}");
        assert!(mgr.held_capabilities().is_empty(), "{decided:?}: dropped, not held");
        assert_eq!(mgr.admit(Capability::Camera), Admission::Skip);
    }
}

#[test]
fn a_granted_capability_needs_no_gesture_because_nothing_can_be_asked() {
    let mut mgr = PermissionManager::new();
    mgr.set_status(Capability::Geolocation, PermissionState::Granted(PermissionQuality::Full));
    mgr.subscribe(Capability::Geolocation, node(1));
    assert_eq!(subscribe_count(&mgr.take_dispatchable()), 1);
}

#[test]
fn a_request_in_flight_holds_the_next_one() {
    let mut mgr = PermissionManager::new();
    mgr.set_status(Capability::Camera, PermissionState::Requested);
    mgr.note_gesture();
    assert_eq!(mgr.admit(Capability::Camera), Admission::Hold);
    assert_eq!(mgr.admit(Capability::Microphone), Admission::Go, "another capability may go");
}

#[test]
fn a_probe_that_leaves_the_dom_drops_its_held_subscribe() {
    let mut mgr = subscribed(Capability::Geolocation);
    assert!(mgr.take_dispatchable().is_empty());
    mgr.release(Capability::Geolocation);
    let events = mgr.take_dispatchable();
    assert_eq!(events.len(), 1, "the Release goes out");
    assert!(matches!(events[0], PermissionDiffEvent::Release { .. }));
    assert!(mgr.held_capabilities().is_empty());
    mgr.note_gesture();
    assert!(mgr.take_dispatchable().is_empty(), "nothing left to ask");
}

#[test]
fn the_platforms_quiet_read_is_adopted_but_never_a_step_back() {
    let mut mgr = PermissionManager::new();
    let granted = PermissionState::Granted(PermissionQuality::Full);
    assert!(mgr.observe(Capability::Camera, granted));
    assert!(!mgr.observe(Capability::Camera, granted), "no change, no event");
    assert!(!mgr.observe(Capability::Camera, PermissionState::NotDetermined), "no word is no step back");
    assert_eq!(mgr.get_status(Capability::Camera), granted);
    // The user turned it off in the settings.
    assert!(mgr.observe(Capability::Camera, PermissionState::Denied));
    assert_eq!(mgr.get_status(Capability::Camera), PermissionState::Denied);
}

#[test]
fn the_quiet_read_is_due_at_the_start_and_then_not_for_a_while() {
    let mut mgr = PermissionManager::new();
    assert!(mgr.probe_due(60_000), "the first time");
    assert!(!mgr.probe_due(60_000), "not again at once");
    assert!(mgr.probe_due(0), "an interval of nothing is always due");
}

#[test]
fn a_held_geolocation_subscribe_goes_with_the_gesture_and_a_release_drops_it() {
    use crate::managers::geolocation::{GeolocationDiffEvent, GeolocationManager};
    let cfg = azul_core::geolocation::GeolocationProbeConfig::default();
    let mut geo = GeolocationManager::new();
    geo.diff_layout(|emit| emit(cfg));
    let mut perm = PermissionManager::new();
    // No gesture: held, nothing starts.
    let none = geo.take_admitted_events(|| perm.admit(Capability::Geolocation));
    assert!(none.is_empty());
    // The gesture: the Subscribe goes, once.
    perm.note_gesture();
    let go = geo.take_admitted_events(|| perm.admit(Capability::Geolocation));
    assert!(matches!(go.as_slice(), [GeolocationDiffEvent::Subscribe { .. }]));
    assert!(geo.take_admitted_events(|| perm.admit(Capability::Geolocation)).is_empty());
    // A denied user is not asked: the probe leaves (its Release goes out) and a new probe's
    // Subscribe is dropped.
    perm.set_status(Capability::Geolocation, PermissionState::Denied);
    geo.diff_layout(|_| {});
    geo.diff_layout(|emit| emit(cfg));
    let after = geo.take_admitted_events(|| perm.admit(Capability::Geolocation));
    assert!(matches!(after.as_slice(), [GeolocationDiffEvent::Release]), "{after:?}");
}

#[test]
fn the_settings_target_is_one_page_per_platform_and_none_on_linux() {
    use SettingsPlatform::*;
    for capability in Capability::ALL {
        assert_eq!(settings_url(capability, Ios), Some("app-settings:"));
        assert_eq!(settings_url(capability, Android), Some("android-settings:app-details"));
        assert_eq!(settings_url(capability, Other), None, "Linux has no per-app permission page");
        assert!(settings_url(capability, Macos).is_some_and(|url| url.starts_with("x-apple.systempreferences:")));
        assert!(settings_url(capability, Windows).is_some_and(|url| url.starts_with("ms-settings:")));
    }
    assert_eq!(
        settings_url(Capability::Camera, Windows),
        Some("ms-settings:privacy-webcam")
    );
    assert_eq!(
        settings_url(Capability::Microphone, Macos),
        Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
    );
    assert_eq!(SettingsOpened::default(), SettingsOpened::Unsupported);
}
