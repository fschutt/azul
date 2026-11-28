use azul_core::dom::{DomId, DomNodeId, NodeId};
use azul_core::task::SystemTick;

use super::*;

/// A moment on the engine's tick scale (60 ticks are one second): the permission manager runs on
/// the engine's own `Instant`, so a test can say exactly how long ago a gesture was.
fn tick(counter: u64) -> Instant {
    Instant::Tick(SystemTick::new(counter))
}

/// The gesture window in ticks.
fn window_ticks() -> u64 {
    GESTURE_WINDOW.as_secs() * azul_css::props::basic::time::TICKS_PER_SECOND
}

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
        assert_eq!(
            mgr.held_capabilities(),
            vec![Capability::Camera],
            "{decided:?}: kept, never asked"
        );
        assert_eq!(mgr.admit(Capability::Camera), Admission::Skip);
        // The user grants it in the settings: the probe that waited starts, nothing is asked.
        assert!(mgr.observe(
            Capability::Camera,
            PermissionState::Granted(PermissionQuality::Full)
        ));
        assert_eq!(subscribe_count(&mgr.take_dispatchable()), 1, "{decided:?}");
        assert!(mgr.held_capabilities().is_empty());
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
    // Subscribe is held, not started.
    perm.set_status(Capability::Geolocation, PermissionState::Denied);
    geo.diff_layout(|_| {});
    geo.diff_layout(|emit| emit(cfg));
    let after = geo.take_admitted_events(|| perm.admit(Capability::Geolocation));
    assert!(matches!(after.as_slice(), [GeolocationDiffEvent::Release]), "{after:?}");
    assert!(geo.has_held_subscribe());
    // The user grants it in the settings: the probe starts, with no gesture and no question.
    assert!(perm.observe(
        Capability::Geolocation,
        PermissionState::Granted(PermissionQuality::Full)
    ));
    let started = geo.take_admitted_events(|| perm.admit(Capability::Geolocation));
    assert!(matches!(started.as_slice(), [GeolocationDiffEvent::Subscribe { .. }]), "{started:?}");
}

#[test]
fn the_gesture_that_admitted_a_capability_admits_it_again_and_no_other() {
    let at = tick(100);
    let mut mgr = PermissionManager::new();
    mgr.note_gesture_at(at.clone());
    assert_eq!(mgr.admit_at(Capability::Geolocation, at.clone()), Admission::Go);
    assert_eq!(
        mgr.admit_at(Capability::Geolocation, at.clone()),
        Admission::Go,
        "the same click, the same capability"
    );
    assert_eq!(
        mgr.admit_at(Capability::Camera, at.clone()),
        Admission::Hold,
        "another capability waits for a click of its own"
    );
    // A new click starts over.
    mgr.note_gesture_at(at.clone());
    assert_eq!(mgr.admit_at(Capability::Camera, at.clone()), Admission::Go);
    assert_eq!(mgr.admit_at(Capability::Geolocation, at), Admission::Hold);
}

#[test]
fn one_gesture_starts_a_probe_through_both_of_its_managers() {
    use crate::managers::geolocation::{GeolocationDiffEvent, GeolocationManager};
    // The probe of the location is a Subscribe of the permission manager AND of the location
    // manager: the click that mounts it goes through both, not through the first one only.
    let cfg = azul_core::geolocation::GeolocationProbeConfig::default();
    let mut perm = subscribed(Capability::Geolocation);
    let mut geo = GeolocationManager::new();
    geo.diff_layout(|emit| emit(cfg));
    perm.note_gesture();
    assert_eq!(subscribe_count(&perm.take_dispatchable()), 1);
    let go = geo.take_admitted_events(|| perm.admit(Capability::Geolocation));
    assert!(matches!(go.as_slice(), [GeolocationDiffEvent::Subscribe { .. }]), "{go:?}");
    assert_eq!(perm.admit(Capability::Camera), Admission::Hold, "another capability needs its own click");
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

#[test]
fn the_pump_reads_the_platform_state_of_what_was_asked_or_is_subscribed_only() {
    let mut mgr = PermissionManager::new();
    assert!(mgr.capabilities_in_use().is_empty(), "an app that asks nothing reads nothing");
    // The app asks the state of the camera; a probe node subscribes the location.
    let _ = mgr.get_status(Capability::Camera);
    mgr.subscribe(Capability::Geolocation, node(1));
    assert_eq!(
        mgr.capabilities_in_use(),
        vec![Capability::Camera, Capability::Geolocation]
    );
    // The manager's own reads are no question of the app's.
    mgr.note_gesture();
    let _ = mgr.admit(Capability::Microphone);
    assert!(!mgr.capabilities_in_use().contains(&Capability::Microphone));
    // Two managers compare equal whatever they were asked.
    assert_eq!(PermissionManager::new(), PermissionManager::new());
}

#[test]
fn a_gesture_of_long_ago_is_no_cause_of_a_request() {
    let now = tick(100 * window_ticks());
    let long_ago = tick(100 * window_ticks() - 4 * window_ticks());
    let mut mgr = PermissionManager::new();
    mgr.note_gesture_at(long_ago);
    assert_eq!(mgr.admit_at(Capability::Camera, now.clone()), Admission::Hold, "a stale gesture asks nobody");
    // The stale gesture is gone; a fresh one admits again.
    mgr.note_gesture_at(now.clone());
    assert_eq!(mgr.admit_at(Capability::Camera, now.clone()), Admission::Go);
    // And it is used up.
    assert_eq!(mgr.admit_at(Capability::Microphone, now.clone()), Admission::Hold);
    // A gesture just inside the window still counts.
    let recent = tick(100 * window_ticks() - window_ticks() / 2);
    mgr.note_gesture_at(recent);
    assert_eq!(mgr.admit_at(Capability::Microphone, now), Admission::Go);
}

#[test]
fn the_gesture_window_ends_exactly_one_window_after_the_gesture() {
    // Exactly the window ago is no longer inside it; one tick less is.
    let now = tick(10 * window_ticks());
    let mut mgr = PermissionManager::new();
    mgr.note_gesture_at(tick(10 * window_ticks() - window_ticks()));
    assert_eq!(mgr.admit_at(Capability::Camera, now.clone()), Admission::Hold);
    mgr.note_gesture_at(tick(10 * window_ticks() - window_ticks() + 1));
    assert_eq!(mgr.admit_at(Capability::Camera, now), Admission::Go);
}
