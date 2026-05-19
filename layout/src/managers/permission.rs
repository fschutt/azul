//! Permission manager — the cross-platform piece of the "permission-as-DOM"
//! architecture (`SUPER_PLAN_2.md` §1.5 and `scripts/research/08_permission_dom_nodes.md`).
//!
//! Stores per-capability state + a refcount keyed on bearing DOM nodes. Three
//! callers drive it:
//!
//! - The **layout pass** scans the styled DOM for permission-bearing
//!   NodeTypes (`GeolocationProbe`, `CameraPreview`, `SensorProbe`, etc.) and
//!   calls `subscribe` / `release` to maintain the refcount. The diff
//!   between consecutive layouts yields the [`PermissionDiffEvent`]s the
//!   platform backend translates into native subscribe/release operations.
//!
//! - The **platform backend** (`dll/src/desktop/extra/permission/<plat>.rs`)
//!   observes the diff events and issues the matching native call
//!   (`AVCaptureDevice.requestAccess` on iOS, `ActivityCompat.requestPermissions`
//!   on Android, etc.). When the OS callback fires it calls `set_status`,
//!   which is mirrored back into callback land via the `CallbackInfo`
//!   accessor `get_permission_status`.
//!
//! - **Callbacks** read `get_status(...)` synchronously to decide whether
//!   to mount a permission-bearing node or show a fallback (the
//!   "user-gesture-first" pattern in the research brief §8.3).
//!
//! The manager has no platform dependencies and is `no_std`-friendly (uses
//! `alloc::collections::BTreeMap` + `alloc::vec::Vec`).

use alloc::collections::btree_map::BTreeMap;
use alloc::vec::Vec;

use azul_core::dom::DomNodeId;

/// One closed enum covering every capability the framework can request.
///
/// The variant set deliberately omits fields like `facing` / `accuracy` /
/// `mode` from the research brief — those parameters belong on the bearing
/// `NodeType` (e.g. `NodeType::CameraPreview(CameraSource::Front)`) so they
/// can change between layout passes without forcing a re-prompt. The
/// `Reconfigure` diff event carries the new params when a node mutates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum Capability {
    /// Camera access (front or back, declared per node).
    Camera,
    /// Microphone access. iOS gates this separately from camera.
    Microphone,
    /// Entire-screen or per-window capture.
    ScreenCapture,
    /// Geolocation (precise vs approximate is per-node, not per-capability).
    Geolocation,
    /// Background geolocation. A separate iOS / Android permission gate.
    GeolocationBackground,
    /// FaceID / TouchID / Hello / `BiometricPrompt`.
    Biometric,
    /// Motion sensor data (accelerometer + gyro + magnetometer).
    Motion,
    /// PhotoKit / MediaStore read.
    PhotoLibrary,
    /// PhotoKit add-only / MediaStore write.
    PhotoLibraryWrite,
    /// Contacts list.
    Contacts,
    /// Calendar entries.
    Calendars,
    /// Reminders (iOS only — Android collapses into Calendars).
    Reminders,
    /// Push / local notification scheduling.
    Notifications,
    /// Bluetooth foreground.
    Bluetooth,
    /// Bluetooth background. Separate iOS Info.plist key + Android permission.
    BluetoothBackground,
    /// Nearby Wi-Fi (Android 13+).
    NearbyWifi,
    /// Local network multicast (iOS 14+).
    LocalNetwork,
    /// iOS App Tracking Transparency (`IDFA` consent, iOS 14.5+).
    AppTrackingTransparency,
}

/// Quality of a granted permission. Matches research/08 §2's quality split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum PermissionQuality {
    /// Full: precise location, full photo library, etc.
    Full,
    /// Reduced: approximate location, "Selected Photos" partial access, etc.
    Reduced,
}

/// State machine the manager tracks per-capability.
///
/// The five canonical states (`NotDetermined` / `Requested` / `Granted` /
/// `Denied` / `Restricted`) cover what every supported platform reports.
/// `EphemeralGranted` is the iOS 14+ "Allow Once" / Android 11+ one-time grant
/// — semantically a Granted that the OS will reset to `NotDetermined` at the
/// next activity launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum PermissionState {
    /// Initial — no prompt has been shown.
    NotDetermined,
    /// OS prompt is currently visible / in-flight.
    Requested,
    /// User granted access.
    Granted {
        quality: PermissionQuality,
    },
    /// User denied access (with or without "don't ask again").
    Denied,
    /// MDM / parental controls / kiosk policy blocks the prompt entirely.
    Restricted,
    /// iOS "Allow Once" / Android one-time. Reverts on next app launch.
    EphemeralGranted {
        until_app_close: bool,
    },
}

impl PermissionState {
    /// `true` if the capability is currently usable, regardless of quality.
    pub fn is_granted(self) -> bool {
        matches!(
            self,
            PermissionState::Granted { .. } | PermissionState::EphemeralGranted { .. }
        )
    }

    /// `true` if a re-prompt could plausibly flip this to `Granted`.
    pub fn could_re_prompt(self) -> bool {
        matches!(self, PermissionState::NotDetermined)
    }
}

/// Diff event emitted at the end of each layout pass for the platform
/// backend to translate into native subscribe / release / reconfigure calls.
///
/// `Subscribe` fires the first time a capability's refcount transitions from
/// zero to one (i.e. the first permission-bearing node of its kind appears).
/// `Release` fires when the refcount drops back to zero. `Reconfigure` is
/// reserved for in-place parameter changes (e.g. camera-facing front → back)
/// once `CameraPreview` lands as a NodeType — kept in the enum so platform
/// backends can ignore it cleanly until then.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum PermissionDiffEvent {
    /// First appearance of `capability` in the layout. Refcount went 0 → 1.
    Subscribe {
        capability: Capability,
        node_id: DomNodeId,
    },
    /// Last bearing node left the layout. Refcount went 1 → 0.
    Release {
        capability: Capability,
    },
    /// Reserved for future use — currently never emitted. The diff path will
    /// fire it once `CameraPreview` etc. land with parameter fields.
    Reconfigure {
        capability: Capability,
    },
}

/// Per-capability state held across frames.
///
/// `refcount` is the number of distinct DOM nodes currently in the layout
/// that subscribed to this capability. `last_subscriber` is the node that
/// caused the most recent 0 → 1 transition; the platform backend uses it
/// to anchor permission-related events back to a node (so an
/// `On::CameraPermissionDenied` callback fires on the right `CameraPreview`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityEntry {
    pub state: PermissionState,
    pub refcount: u32,
    pub last_subscriber: Option<DomNodeId>,
}

impl CapabilityEntry {
    fn new() -> Self {
        Self {
            state: PermissionState::NotDetermined,
            refcount: 0,
            last_subscriber: None,
        }
    }
}

/// Cross-platform permission manager.
///
/// One per `App` (capabilities live at process scope, not per-window — a
/// camera session backing two windows multiplexes via a single capture
/// stream; cf. research/08 §8.6). `LayoutWindow` holds a borrow / `Arc`
/// reference, not an owned copy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PermissionManager {
    /// Latest known state + refcount per capability.
    pub statuses: BTreeMap<Capability, CapabilityEntry>,
    /// Diff events emitted since the last call to `take_pending_events`.
    ///
    /// Held as a queue so the platform backend can drain it once per frame
    /// instead of receiving callbacks during the layout pass itself (the
    /// layout pass is on a hot path that should not block on FFI).
    pending_events: Vec<PermissionDiffEvent>,
}

impl PermissionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the most recently observed state for `capability`.
    pub fn get_status(&self, capability: Capability) -> PermissionState {
        self.statuses
            .get(&capability)
            .map(|e| e.state)
            .unwrap_or(PermissionState::NotDetermined)
    }

    /// Record that `node_id` now needs `capability`. The first subscriber
    /// (refcount 0 → 1) enqueues a `Subscribe` event for the platform layer
    /// to translate into a native prompt.
    pub fn subscribe(&mut self, capability: Capability, node_id: DomNodeId) {
        let entry = self
            .statuses
            .entry(capability)
            .or_insert_with(CapabilityEntry::new);
        entry.last_subscriber = Some(node_id);
        entry.refcount = entry.refcount.saturating_add(1);
        if entry.refcount == 1 {
            self.pending_events.push(PermissionDiffEvent::Subscribe {
                capability,
                node_id,
            });
        }
    }

    /// Drop one subscription. The last release (refcount 1 → 0) enqueues a
    /// `Release` event so the platform backend can tear the session down.
    pub fn release(&mut self, capability: Capability) {
        let Some(entry) = self.statuses.get_mut(&capability) else {
            return;
        };
        if entry.refcount == 0 {
            return;
        }
        entry.refcount -= 1;
        if entry.refcount == 0 {
            entry.last_subscriber = None;
            self.pending_events
                .push(PermissionDiffEvent::Release { capability });
        }
    }

    /// Force `capability`'s refcount down to zero. Used by `recheck_all` when
    /// the OS revokes a permission out from under us — we have to tear down
    /// the subscription regardless of how many DOM nodes still reference it.
    pub fn force_release(&mut self, capability: Capability) {
        let Some(entry) = self.statuses.get_mut(&capability) else {
            return;
        };
        if entry.refcount == 0 {
            return;
        }
        entry.refcount = 0;
        entry.last_subscriber = None;
        self.pending_events
            .push(PermissionDiffEvent::Release { capability });
    }

    /// Platform backend writes the OS-observed state back into the manager.
    ///
    /// Returns true if the state actually changed — the caller can use this
    /// signal to mark the window dirty for relayout (so a permission-aware
    /// callback gets a chance to render the new state).
    pub fn set_status(&mut self, capability: Capability, state: PermissionState) -> bool {
        let entry = self
            .statuses
            .entry(capability)
            .or_insert_with(CapabilityEntry::new);
        if entry.state == state {
            return false;
        }
        entry.state = state;
        true
    }

    /// Drain queued diff events. Platform backend calls this once per frame.
    pub fn take_pending_events(&mut self) -> Vec<PermissionDiffEvent> {
        core::mem::take(&mut self.pending_events)
    }

    /// Refcount snapshot — primarily for diagnostics and tests.
    pub fn refcount(&self, capability: Capability) -> u32 {
        self.statuses
            .get(&capability)
            .map(|e| e.refcount)
            .unwrap_or(0)
    }

    /// Pre-compute the next-frame refcount map from a closure that yields
    /// `(capability, node_id)` pairs for every permission-bearing node in
    /// the current styled DOM. Then diff against the existing refcounts and
    /// enqueue the matching Subscribe / Release events.
    ///
    /// This is the entry point the layout pass calls. It exists as a closure
    /// rather than a direct `StyledDom` walker because `StyledDom` lives in
    /// `azul_core::styled_dom` and would otherwise force a (tiny) cycle.
    pub fn diff_layout<F>(&mut self, mut for_each_bearing_node: F)
    where
        F: FnMut(&mut dyn FnMut(Capability, DomNodeId)),
    {
        // 1. Drain the new layout into (capability → (count, first_node)).
        let mut next: BTreeMap<Capability, (u32, Option<DomNodeId>)> = BTreeMap::new();
        for_each_bearing_node(&mut |cap, node| {
            let slot = next.entry(cap).or_insert((0, None));
            slot.0 = slot.0.saturating_add(1);
            if slot.1.is_none() {
                slot.1 = Some(node);
            }
        });

        // 2. Compute the new state map from the old one + the next layout.
        // Iterate every capability we know about plus any new ones.
        let mut all_caps: Vec<Capability> = self.statuses.keys().copied().collect();
        for cap in next.keys() {
            if !all_caps.contains(cap) {
                all_caps.push(*cap);
            }
        }

        for cap in all_caps {
            let (new_count, first_node) = next.get(&cap).copied().unwrap_or((0, None));
            let entry = self
                .statuses
                .entry(cap)
                .or_insert_with(CapabilityEntry::new);
            let old_count = entry.refcount;
            entry.refcount = new_count;
            if new_count == 0 && old_count > 0 {
                entry.last_subscriber = None;
                self.pending_events
                    .push(PermissionDiffEvent::Release { capability: cap });
            } else if new_count > 0 && old_count == 0 {
                let node = first_node.unwrap_or(DomNodeId::ROOT);
                entry.last_subscriber = first_node;
                self.pending_events.push(PermissionDiffEvent::Subscribe {
                    capability: cap,
                    node_id: node,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_core::dom::{DomId, NodeId};

    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeId::from_usize(idx).into(),
        }
    }

    #[test]
    fn subscribe_release_round_trip_emits_paired_events() {
        let mut mgr = PermissionManager::new();
        assert_eq!(mgr.get_status(Capability::Geolocation), PermissionState::NotDetermined);
        assert_eq!(mgr.refcount(Capability::Geolocation), 0);

        mgr.subscribe(Capability::Geolocation, node(1));
        assert_eq!(mgr.refcount(Capability::Geolocation), 1);
        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            PermissionDiffEvent::Subscribe { capability: Capability::Geolocation, .. }
        ));

        mgr.release(Capability::Geolocation);
        assert_eq!(mgr.refcount(Capability::Geolocation), 0);
        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            PermissionDiffEvent::Release { capability: Capability::Geolocation }
        ));
    }

    #[test]
    fn second_subscriber_does_not_re_emit_subscribe() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Camera, node(1));
        mgr.subscribe(Capability::Camera, node(2));
        assert_eq!(mgr.refcount(Capability::Camera), 2);
        let events = mgr.take_pending_events();
        // Exactly one Subscribe should have been emitted across both subscribes.
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn release_only_after_last_subscriber_drops() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Microphone, node(1));
        mgr.subscribe(Capability::Microphone, node(2));
        // Drain the initial Subscribe so the assertion below isolates Release.
        let _ = mgr.take_pending_events();

        mgr.release(Capability::Microphone);
        assert_eq!(mgr.refcount(Capability::Microphone), 1);
        assert!(mgr.take_pending_events().is_empty());

        mgr.release(Capability::Microphone);
        assert_eq!(mgr.refcount(Capability::Microphone), 0);
        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            PermissionDiffEvent::Release { capability: Capability::Microphone }
        ));
    }

    #[test]
    fn force_release_drops_refcount_and_emits_event() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Camera, node(1));
        mgr.subscribe(Capability::Camera, node(2));
        let _ = mgr.take_pending_events();

        mgr.force_release(Capability::Camera);
        assert_eq!(mgr.refcount(Capability::Camera), 0);
        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            PermissionDiffEvent::Release { capability: Capability::Camera }
        ));
    }

    #[test]
    fn set_status_returns_change_flag() {
        let mut mgr = PermissionManager::new();
        assert!(mgr.set_status(Capability::Camera, PermissionState::Requested));
        assert!(!mgr.set_status(Capability::Camera, PermissionState::Requested));
        assert!(mgr.set_status(
            Capability::Camera,
            PermissionState::Granted { quality: PermissionQuality::Full }
        ));
        assert!(mgr.get_status(Capability::Camera).is_granted());
    }

    #[test]
    fn diff_layout_picks_up_appearing_node_and_releases_it_next_frame() {
        let mut mgr = PermissionManager::new();

        // Frame 1: GeolocationProbe present.
        mgr.diff_layout(|emit| {
            emit(Capability::Geolocation, node(7));
        });
        assert_eq!(mgr.refcount(Capability::Geolocation), 1);
        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            PermissionDiffEvent::Subscribe { capability: Capability::Geolocation, .. }
        ));

        // Frame 2: probe removed.
        mgr.diff_layout(|_emit| { /* no bearing nodes this frame */ });
        assert_eq!(mgr.refcount(Capability::Geolocation), 0);
        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            PermissionDiffEvent::Release { capability: Capability::Geolocation }
        ));
    }

    #[test]
    fn diff_layout_re_emits_subscribe_after_release_cycle() {
        let mut mgr = PermissionManager::new();

        mgr.diff_layout(|emit| emit(Capability::Camera, node(1)));
        let _ = mgr.take_pending_events();

        mgr.diff_layout(|_emit| {});
        let _ = mgr.take_pending_events();

        // Same capability reappears — must emit Subscribe again because the
        // platform tore the session down on the prior Release.
        mgr.diff_layout(|emit| emit(Capability::Camera, node(2)));
        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            PermissionDiffEvent::Subscribe { capability: Capability::Camera, .. }
        ));
    }
}
