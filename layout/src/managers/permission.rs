//! Permission manager — the cross-platform piece of the "permission-as-DOM"
//! architecture (`SUPER_PLAN_2.md` §1.5 and `scripts/research/08_permission_dom_nodes.md`).
//!
//! Stores per-capability state + a refcount keyed on bearing DOM nodes. Three
//! callers drive it:
//!
//! - The **layout pass** scans the styled DOM for permission-bearing
//!   `NodeTypes` (`GeolocationProbe`, `CameraPreview`, `SensorProbe`, etc.) and
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
use azul_core::events::{
    EventData, EventProvider, EventSource as CoreEventSource, EventType, SyntheticEvent,
};
use azul_core::task::Instant;

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
    /// `FaceID` / `TouchID` / Hello / `BiometricPrompt`.
    Biometric,
    /// Motion sensor data (accelerometer + gyro + magnetometer).
    Motion,
    /// `PhotoKit` / `MediaStore` read.
    PhotoLibrary,
    /// `PhotoKit` add-only / `MediaStore` write.
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
    Granted(PermissionQuality),
    /// User denied access (with or without "don't ask again").
    Denied,
    /// MDM / parental controls / kiosk policy blocks the prompt entirely.
    Restricted,
    /// iOS "Allow Once" / Android one-time. Reverts on next app launch.
    EphemeralGranted(bool),
}

impl PermissionState {
    /// `true` if the capability is currently usable, regardless of quality.
    #[must_use] pub const fn is_granted(self) -> bool {
        matches!(
            self,
            Self::Granted(..) | Self::EphemeralGranted(..)
        )
    }

    /// `true` if a re-prompt could plausibly flip this to `Granted`.
    #[must_use] pub const fn could_re_prompt(self) -> bool {
        matches!(self, Self::NotDetermined)
    }
}

/// Diff event emitted at the end of each layout pass for the platform
/// backend to translate into native subscribe / release / reconfigure calls.
///
/// `Subscribe` fires the first time a capability's refcount transitions from
/// zero to one (i.e. the first permission-bearing node of its kind appears).
/// `Release` fires when the refcount drops back to zero. `Reconfigure` is
/// reserved for in-place parameter changes (e.g. camera-facing front → back)
/// once `CameraPreview` lands as a `NodeType` — kept in the enum so platform
/// backends can ignore it cleanly until then.
#[derive(Copy, Debug, Clone, PartialEq, Eq)]
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
#[derive(Copy, Debug, Clone, PartialEq, Eq)]
pub struct CapabilityEntry {
    pub state: PermissionState,
    pub refcount: u32,
    pub last_subscriber: Option<DomNodeId>,
}

impl CapabilityEntry {
    const fn new() -> Self {
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
    /// State flips folded since the last event pass, with the capability's
    /// most recent subscriber node (MWA-A1b). Read by the `EventProvider`
    /// impl to synthesize targeted `PermissionChanged` events; cleared by
    /// [`clear_pending_changed`](Self::clear_pending_changed) after dispatch.
    pending_changed: Vec<(Capability, Option<DomNodeId>)>,
}

impl EventProvider for PermissionManager {
    /// Yield one `PermissionChanged` event per state flip folded since the
    /// last pass — targeted at the capability's most recent subscriber node
    /// when known (so a probe node's Hover callback fires), else the root
    /// (window-level filters match either way).
    fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
        self.pending_changed
            .iter()
            .map(|(_capability, node)| {
                SyntheticEvent::new(
                    EventType::PermissionChanged,
                    CoreEventSource::User,
                    node.unwrap_or(DomNodeId::ROOT),
                    timestamp.clone(),
                    EventData::None,
                )
            })
            .collect()
    }
}

impl PermissionManager {
    #[must_use] pub fn new() -> Self {
        Self::default()
    }

    /// Read the most recently observed state for `capability`.
    #[must_use] pub fn get_status(&self, capability: Capability) -> PermissionState {
        self.statuses
            .get(&capability)
            .map_or(PermissionState::NotDetermined, |e| e.state)
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
        // MWA-A1b: remember the flip so the EventProvider can synthesize a
        // PermissionChanged event, targeted at the subscriber node when known.
        self.pending_changed.push((capability, entry.last_subscriber));
        true
    }

    /// Clear the pending state-flip queue. The dll calls this after the
    /// event pass has collected the `PermissionChanged` events.
    pub fn clear_pending_changed(&mut self) {
        self.pending_changed.clear();
    }

    /// `true` while any capability sits in [`PermissionState::Requested`]
    /// (an OS prompt is in flight and its outcome will arrive through the
    /// async channel) — the capability pump keeps its timer armed so the
    /// outcome reaches callbacks even in an otherwise idle app (MWA-A1b
    /// arming signal).
    #[must_use] pub fn has_pending_async(&self) -> bool {
        self.statuses
            .values()
            .any(|e| e.state == PermissionState::Requested)
    }

    /// Drain queued diff events. Platform backend calls this once per frame.
    pub fn take_pending_events(&mut self) -> Vec<PermissionDiffEvent> {
        core::mem::take(&mut self.pending_events)
    }

    /// Refcount snapshot — primarily for diagnostics and tests.
    #[must_use] pub fn refcount(&self, capability: Capability) -> u32 {
        self.statuses
            .get(&capability)
            .map_or(0, |e| e.refcount)
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

// ────────── Async result channel (platform backend → manager) ─────────
//
// When a `Subscribe` fires an OS prompt, the result arrives later on an
// arbitrary thread (an iOS completion handler / Android
// `onRequestPermissionsResult`) where there's no handle to the live
// `PermissionManager` (it lives inside the window's `LayoutWindow`). The
// platform backend parks the resolved state here; the layout pass drains
// it once per frame via [`drain_async_results`] and applies each through
// [`PermissionManager::set_status`]. Pure Rust — no platform dependency,
// so it satisfies SUPER_PLAN_2 §0.5's "no platform deps in azul-layout".

static ASYNC_RESULTS: std::sync::Mutex<Vec<(Capability, PermissionState)>> =
    std::sync::Mutex::new(Vec::new());

/// Park an async permission result. Called by a platform backend (in the
/// dll) when an OS prompt resolves. Thread-safe; recovers from a poisoned
/// lock so one panicking applier can't wedge delivery forever.
pub fn push_async_result(capability: Capability, state: PermissionState) {
    let mut q = ASYNC_RESULTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    q.push((capability, state));
}

/// Drain everything parked by [`push_async_result`], in arrival order.
/// Called once per layout pass; the caller applies each result through
/// [`PermissionManager::set_status`] and relayouts if any changed.
pub fn drain_async_results() -> Vec<(Capability, PermissionState)> {
    let mut q = ASYNC_RESULTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
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
        drop(mgr.take_pending_events());

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
        drop(mgr.take_pending_events());

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
            PermissionState::Granted(PermissionQuality::Full)
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
        drop(mgr.take_pending_events());

        mgr.diff_layout(|_emit| {});
        drop(mgr.take_pending_events());

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

    #[test]
    fn async_results_round_trip_through_manager() {
        // The channel is a process-global and libtest runs tests in parallel:
        // serialize against every other test that touches it.
        let _serialize = super::autotest_generated::lock_async_channel();
        // The channel is a process-global; clear anything a prior test or
        // ordering left behind so this test is self-contained.
        drop(drain_async_results());

        push_async_result(
            Capability::Camera,
            PermissionState::Granted(PermissionQuality::Full),
        );
        push_async_result(Capability::Geolocation, PermissionState::Denied);

        let drained = drain_async_results();
        assert_eq!(drained.len(), 2, "both parked results drain in order");
        // Arrival order preserved.
        assert_eq!(drained[0].0, Capability::Camera);
        assert_eq!(drained[1].0, Capability::Geolocation);

        // Applying them through the manager reflects in get_status — this is
        // exactly what the dll layout pass does each frame.
        let mut mgr = PermissionManager::new();
        for (cap, state) in drained {
            mgr.set_status(cap, state);
        }
        assert!(mgr.get_status(Capability::Camera).is_granted());
        assert_eq!(mgr.get_status(Capability::Geolocation), PermissionState::Denied);

        // A second drain is empty — the queue was taken, not copied.
        assert!(drain_async_results().is_empty());
    }
}

#[cfg(test)]
mod pump_provider_tests {
    use super::*;
    use azul_core::task::{Instant, SystemTick};

    fn ts() -> Instant {
        Instant::Tick(SystemTick::new(0))
    }

    #[test]
    fn status_flip_yields_targeted_permission_changed_event() {
        let node = DomNodeId::ROOT;
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Geolocation, node);
        drop(mgr.take_pending_events());
        assert!(mgr.get_pending_events(ts()).is_empty(), "no flip yet");

        assert!(mgr.set_status(Capability::Geolocation, PermissionState::Requested));
        assert!(mgr.has_pending_async(), "Requested = OS prompt in flight");
        let events = mgr.get_pending_events(ts());
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].event_type,
            EventType::PermissionChanged
        );
        assert_eq!(events[0].target, node, "targeted at the subscriber node");

        mgr.clear_pending_changed();
        assert!(mgr.get_pending_events(ts()).is_empty(), "cleared after dispatch");

        assert!(mgr.set_status(
            Capability::Geolocation,
            PermissionState::Granted(PermissionQuality::Full),
        ));
        assert!(!mgr.has_pending_async(), "prompt resolved");
        assert_eq!(mgr.get_pending_events(ts()).len(), 1);
    }

    #[test]
    fn unchanged_status_emits_no_event() {
        let mut mgr = PermissionManager::new();
        assert!(mgr.set_status(Capability::Geolocation, PermissionState::Denied));
        mgr.clear_pending_changed();
        assert!(!mgr.set_status(Capability::Geolocation, PermissionState::Denied));
        assert!(mgr.get_pending_events(ts()).is_empty());
    }
}

#[cfg(test)]
mod autotest_generated {
    use alloc::collections::BTreeSet;

    use azul_core::dom::{DomId, NodeId};
    use azul_core::task::{Instant, SystemTick};

    use super::*;
    use crate::managers::{NodeIdMap, NodeIdRemap};

    // ── fixtures ────────────────────────────────────────────────────────

    /// Every `Capability` variant, in declaration order. Kept honest by
    /// `all_capabilities_are_distinct_and_totally_ordered` below, whose
    /// exhaustive `match` fails to compile if a variant is ever added.
    const ALL_CAPS: [Capability; 18] = [
        Capability::Camera,
        Capability::Microphone,
        Capability::ScreenCapture,
        Capability::Geolocation,
        Capability::GeolocationBackground,
        Capability::Biometric,
        Capability::Motion,
        Capability::PhotoLibrary,
        Capability::PhotoLibraryWrite,
        Capability::Contacts,
        Capability::Calendars,
        Capability::Reminders,
        Capability::Notifications,
        Capability::Bluetooth,
        Capability::BluetoothBackground,
        Capability::NearbyWifi,
        Capability::LocalNetwork,
        Capability::AppTrackingTransparency,
    ];

    /// Every distinct `PermissionState` value, including both payloads of the
    /// two data-carrying variants.
    const ALL_STATES: [PermissionState; 8] = [
        PermissionState::NotDetermined,
        PermissionState::Requested,
        PermissionState::Granted(PermissionQuality::Full),
        PermissionState::Granted(PermissionQuality::Reduced),
        PermissionState::Denied,
        PermissionState::Restricted,
        PermissionState::EphemeralGranted(true),
        PermissionState::EphemeralGranted(false),
    ];

    /// `NodeId::from_usize` is 1-based: `node(0)` is the `None` sentinel (==
    /// `DomNodeId::ROOT`), `node(1)` is `NodeId(0)`.
    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeId::from_usize(idx).into(),
        }
    }

    fn node_in_dom(dom: usize, idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: dom },
            node: NodeId::from_usize(idx).into(),
        }
    }

    fn ts(tick: u64) -> Instant {
        Instant::Tick(SystemTick::new(tick))
    }

    /// Serializes every test that touches the process-global `ASYNC_RESULTS`
    /// queue. libtest runs tests in parallel threads inside one process, so
    /// without this a concurrent `push_async_result` would corrupt another
    /// test's drain. Recovers from poisoning so one failing test cannot wedge
    /// the rest of the suite.
    pub(super) fn lock_async_channel() -> std::sync::MutexGuard<'static, ()> {
        static ASYNC_CHANNEL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        ASYNC_CHANNEL_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    // ── Capability: enum hygiene ────────────────────────────────────────

    #[test]
    fn all_capabilities_are_distinct_and_totally_ordered() {
        // Exhaustiveness guard: adding a variant to `Capability` makes this
        // match non-exhaustive, which is the compile error that says "add it
        // to ALL_CAPS too". Combined with the distinctness check below, that
        // means ALL_CAPS provably covers every variant.
        for cap in ALL_CAPS {
            match cap {
                Capability::Camera
                | Capability::Microphone
                | Capability::ScreenCapture
                | Capability::Geolocation
                | Capability::GeolocationBackground
                | Capability::Biometric
                | Capability::Motion
                | Capability::PhotoLibrary
                | Capability::PhotoLibraryWrite
                | Capability::Contacts
                | Capability::Calendars
                | Capability::Reminders
                | Capability::Notifications
                | Capability::Bluetooth
                | Capability::BluetoothBackground
                | Capability::NearbyWifi
                | Capability::LocalNetwork
                | Capability::AppTrackingTransparency => {}
            }
        }

        let set: BTreeSet<Capability> = ALL_CAPS.iter().copied().collect();
        assert_eq!(set.len(), ALL_CAPS.len(), "ALL_CAPS contains a duplicate");

        // The derived Ord follows declaration order — the BTreeMap key order
        // (and therefore the diff-event order) depends on it.
        let mut sorted = ALL_CAPS.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, ALL_CAPS.to_vec());
    }

    #[test]
    fn all_states_are_distinct() {
        // Same guard for the state machine: a new variant fails to compile.
        for state in ALL_STATES {
            match state {
                PermissionState::NotDetermined
                | PermissionState::Requested
                | PermissionState::Granted(_)
                | PermissionState::Denied
                | PermissionState::Restricted
                | PermissionState::EphemeralGranted(_) => {}
            }
        }
        let set: BTreeSet<PermissionState> = ALL_STATES.iter().copied().collect();
        assert_eq!(set.len(), ALL_STATES.len(), "ALL_STATES contains a duplicate");
    }

    // ── PermissionState predicates ──────────────────────────────────────

    #[test]
    fn is_granted_matches_the_documented_state_matrix() {
        assert!(!PermissionState::NotDetermined.is_granted());
        assert!(!PermissionState::Requested.is_granted());
        assert!(PermissionState::Granted(PermissionQuality::Full).is_granted());
        assert!(
            PermissionState::Granted(PermissionQuality::Reduced).is_granted(),
            "reduced quality is still usable — `is_granted` ignores quality"
        );
        assert!(!PermissionState::Denied.is_granted());
        assert!(!PermissionState::Restricted.is_granted());
        // NOTE: the bool payload of EphemeralGranted is ignored by `is_granted`
        // (the `matches!` uses `..`), so BOTH payloads report granted.
        assert!(PermissionState::EphemeralGranted(true).is_granted());
        assert!(PermissionState::EphemeralGranted(false).is_granted());
    }

    #[test]
    fn could_re_prompt_is_true_only_for_not_determined() {
        for state in ALL_STATES {
            let expected = state == PermissionState::NotDetermined;
            assert_eq!(
                state.could_re_prompt(),
                expected,
                "could_re_prompt({state:?}) should be {expected}"
            );
        }
        // Denied is explicitly NOT re-promptable — the OS suppresses the
        // second prompt, so callers must deep-link to settings instead.
        assert!(!PermissionState::Denied.could_re_prompt());
        assert!(!PermissionState::Restricted.could_re_prompt());
    }

    #[test]
    fn granted_and_re_promptable_are_mutually_exclusive() {
        for state in ALL_STATES {
            assert!(
                !(state.is_granted() && state.could_re_prompt()),
                "{state:?} claims to be both granted and re-promptable"
            );
        }
    }

    #[test]
    fn predicates_are_usable_in_const_context() {
        // Both are `const fn`; a regression to a non-const body would fail to
        // compile here rather than silently break `no_std` callers.
        const GRANTED: bool = PermissionState::Granted(PermissionQuality::Reduced).is_granted();
        const RE_PROMPT: bool = PermissionState::NotDetermined.could_re_prompt();
        const DENIED_GRANTED: bool = PermissionState::Denied.is_granted();
        assert!(GRANTED);
        assert!(RE_PROMPT);
        assert!(!DENIED_GRANTED);
    }

    // ── constructors ────────────────────────────────────────────────────

    #[test]
    fn capability_entry_new_is_the_zero_state() {
        let e = CapabilityEntry::new();
        assert_eq!(e.state, PermissionState::NotDetermined);
        assert_eq!(e.refcount, 0);
        assert_eq!(e.last_subscriber, None);
        // Deterministic: two constructions are indistinguishable.
        assert_eq!(e, CapabilityEntry::new());
        // A fresh entry is re-promptable and not usable — the invariant every
        // caller assumes before the first prompt.
        assert!(!e.state.is_granted());
        assert!(e.state.could_re_prompt());
    }

    #[test]
    fn new_manager_is_empty_for_every_capability() {
        let mgr = PermissionManager::new();
        assert_eq!(mgr, PermissionManager::default());
        assert!(mgr.statuses.is_empty());
        assert!(!mgr.has_pending_async());
        assert!(mgr.get_pending_events(ts(0)).is_empty());

        for cap in ALL_CAPS {
            assert_eq!(mgr.get_status(cap), PermissionState::NotDetermined);
            assert_eq!(mgr.refcount(cap), 0);
        }
        // Reading must not lazily create entries — an entry with refcount 0
        // would make `diff_layout` iterate capabilities nobody ever used.
        assert!(mgr.statuses.is_empty(), "get_status/refcount created entries");
    }

    #[test]
    fn take_pending_events_on_a_fresh_manager_is_empty_and_idempotent() {
        let mut mgr = PermissionManager::new();
        assert!(mgr.take_pending_events().is_empty());
        assert!(mgr.take_pending_events().is_empty());
        mgr.clear_pending_changed();
        mgr.clear_pending_changed();
        assert_eq!(mgr, PermissionManager::new());
    }

    // ── refcount arithmetic: underflow / overflow / saturation ──────────

    #[test]
    fn release_on_unknown_capability_is_a_noop() {
        let mut mgr = PermissionManager::new();
        for cap in ALL_CAPS {
            mgr.release(cap);
            mgr.force_release(cap);
        }
        assert!(mgr.statuses.is_empty(), "release must not create entries");
        assert!(mgr.take_pending_events().is_empty());
        assert!(mgr.get_pending_events(ts(0)).is_empty());
    }

    #[test]
    fn release_at_zero_refcount_does_not_underflow() {
        let mut mgr = PermissionManager::new();
        // set_status creates the entry with refcount 0 — the exact shape that
        // would panic on `refcount -= 1` in debug builds if unguarded.
        mgr.set_status(Capability::Camera, PermissionState::Denied);
        assert_eq!(mgr.refcount(Capability::Camera), 0);

        for _ in 0..8 {
            mgr.release(Capability::Camera);
        }
        assert_eq!(mgr.refcount(Capability::Camera), 0, "refcount wrapped around");
        assert!(
            mgr.take_pending_events().is_empty(),
            "a release that never had a subscriber must not emit Release"
        );
    }

    #[test]
    fn double_release_emits_exactly_one_release_event() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Motion, node(1));
        drop(mgr.take_pending_events());

        mgr.release(Capability::Motion);
        mgr.release(Capability::Motion);
        mgr.release(Capability::Motion);

        assert_eq!(mgr.refcount(Capability::Motion), 0);
        let events = mgr.take_pending_events();
        assert_eq!(
            events.len(),
            1,
            "over-releasing must not double-tear-down the native session: {events:?}"
        );
    }

    #[test]
    fn subscribe_saturates_at_u32_max_instead_of_overflowing() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Bluetooth, node(1));
        drop(mgr.take_pending_events());
        // Reach the boundary directly — 4 billion subscribe() calls is not a
        // test. `statuses` is a pub field, so this is a supported shortcut.
        mgr.statuses.get_mut(&Capability::Bluetooth).unwrap().refcount = u32::MAX;

        mgr.subscribe(Capability::Bluetooth, node(2));

        assert_eq!(mgr.refcount(Capability::Bluetooth), u32::MAX, "refcount wrapped to 0");
        assert!(
            mgr.take_pending_events().is_empty(),
            "a saturating subscribe must not look like a 0 -> 1 transition"
        );
        // The subscriber is still tracked even when the count saturates.
        assert_eq!(
            mgr.statuses[&Capability::Bluetooth].last_subscriber,
            Some(node(2))
        );
    }

    #[test]
    fn release_from_u32_max_does_not_wrap_or_emit() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Contacts, node(1));
        drop(mgr.take_pending_events());
        mgr.statuses.get_mut(&Capability::Contacts).unwrap().refcount = u32::MAX;

        mgr.release(Capability::Contacts);

        assert_eq!(mgr.refcount(Capability::Contacts), u32::MAX - 1);
        assert!(mgr.take_pending_events().is_empty());
    }

    #[test]
    fn force_release_from_saturated_refcount_emits_one_release() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::ScreenCapture, node(1));
        drop(mgr.take_pending_events());
        mgr.statuses.get_mut(&Capability::ScreenCapture).unwrap().refcount = u32::MAX;

        mgr.force_release(Capability::ScreenCapture);
        assert_eq!(mgr.refcount(Capability::ScreenCapture), 0);
        assert_eq!(
            mgr.statuses[&Capability::ScreenCapture].last_subscriber,
            None
        );
        assert_eq!(mgr.take_pending_events().len(), 1);

        // Second force_release: refcount is already 0, nothing to tear down.
        mgr.force_release(Capability::ScreenCapture);
        assert!(
            mgr.take_pending_events().is_empty(),
            "force_release on a zero refcount must be a no-op"
        );
    }

    #[test]
    fn subscribe_after_a_full_release_re_emits_subscribe() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Camera, node(1));
        mgr.release(Capability::Camera);
        drop(mgr.take_pending_events());

        // The platform tore the session down on Release, so the reappearing
        // node must produce a fresh Subscribe (0 -> 1 again).
        mgr.subscribe(Capability::Camera, node(2));
        let events = mgr.take_pending_events();
        assert_eq!(
            events,
            [PermissionDiffEvent::Subscribe {
                capability: Capability::Camera,
                node_id: node(2),
            }]
            .to_vec()
        );
    }

    #[test]
    fn release_cycle_preserves_the_os_observed_state() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Geolocation, node(1));
        mgr.set_status(
            Capability::Geolocation,
            PermissionState::Granted(PermissionQuality::Reduced),
        );
        mgr.release(Capability::Geolocation);

        // Refcount is gone but the grant is NOT forgotten — otherwise every
        // layout pass that drops the node would re-prompt the user.
        assert_eq!(mgr.refcount(Capability::Geolocation), 0);
        assert_eq!(
            mgr.get_status(Capability::Geolocation),
            PermissionState::Granted(PermissionQuality::Reduced)
        );
        assert_eq!(mgr.statuses[&Capability::Geolocation].last_subscriber, None);
    }

    #[test]
    fn subscribe_accepts_boundary_node_ids() {
        let mut mgr = PermissionManager::new();
        // The `None` sentinel (== DomNodeId::ROOT) and the largest encodable
        // NodeId must both round-trip through the event queue untouched.
        mgr.subscribe(Capability::Notifications, DomNodeId::ROOT);
        mgr.subscribe(Capability::Biometric, node(usize::MAX));

        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 2);
        assert!(events.contains(&PermissionDiffEvent::Subscribe {
            capability: Capability::Notifications,
            node_id: DomNodeId::ROOT,
        }));
        assert!(events.contains(&PermissionDiffEvent::Subscribe {
            capability: Capability::Biometric,
            node_id: node(usize::MAX),
        }));
    }

    // ── set_status / pending_changed ────────────────────────────────────

    #[test]
    fn set_status_get_status_round_trips_over_the_whole_matrix() {
        for cap in ALL_CAPS {
            for state in ALL_STATES {
                let mut mgr = PermissionManager::new();
                mgr.set_status(cap, state);
                assert_eq!(mgr.get_status(cap), state, "{cap:?} / {state:?} did not round-trip");
                // Writing one capability must not leak into any other.
                for other in ALL_CAPS.iter().copied().filter(|c| *c != cap) {
                    assert_eq!(mgr.get_status(other), PermissionState::NotDetermined);
                }
            }
        }
    }

    #[test]
    fn set_status_change_flag_is_exact_over_every_transition() {
        for a in ALL_STATES {
            for b in ALL_STATES {
                let mut mgr = PermissionManager::new();
                let cap = Capability::Calendars;

                // The entry starts at NotDetermined, so writing NotDetermined
                // first is a no-change write.
                let changed_a = mgr.set_status(cap, a);
                assert_eq!(changed_a, a != PermissionState::NotDetermined, "{a:?}");

                let changed_b = mgr.set_status(cap, b);
                assert_eq!(changed_b, b != a, "{a:?} -> {b:?} reported the wrong flag");

                assert_eq!(mgr.get_status(cap), b);
                assert_eq!(mgr.has_pending_async(), b == PermissionState::Requested);

                // One PermissionChanged event per *actual* flip, no more.
                let expected = usize::from(changed_a) + usize::from(changed_b);
                assert_eq!(mgr.get_pending_events(ts(0)).len(), expected, "{a:?} -> {b:?}");
            }
        }
    }

    #[test]
    fn set_status_creates_an_entry_with_a_zero_refcount() {
        let mut mgr = PermissionManager::new();
        assert!(mgr.set_status(Capability::Reminders, PermissionState::Restricted));
        let entry = mgr.statuses[&Capability::Reminders];
        assert_eq!(entry.refcount, 0, "a status write is not a subscription");
        assert_eq!(entry.last_subscriber, None);
        assert!(mgr.take_pending_events().is_empty(), "set_status is not a diff event");
    }

    #[test]
    fn quality_and_payload_changes_count_as_state_changes() {
        let mut mgr = PermissionManager::new();
        let cap = Capability::PhotoLibrary;
        assert!(mgr.set_status(cap, PermissionState::Granted(PermissionQuality::Full)));
        // Full -> Reduced ("Selected Photos") is a real change the UI must see.
        assert!(mgr.set_status(cap, PermissionState::Granted(PermissionQuality::Reduced)));
        assert!(!mgr.set_status(cap, PermissionState::Granted(PermissionQuality::Reduced)));
        // Both EphemeralGranted payloads are distinct states.
        assert!(mgr.set_status(cap, PermissionState::EphemeralGranted(true)));
        assert!(mgr.set_status(cap, PermissionState::EphemeralGranted(false)));
        assert!(!mgr.set_status(cap, PermissionState::EphemeralGranted(false)));
        assert!(mgr.get_status(cap).is_granted());
    }

    #[test]
    fn has_pending_async_tracks_only_the_requested_state() {
        for state in ALL_STATES {
            let mut mgr = PermissionManager::new();
            mgr.set_status(Capability::Camera, state);
            assert_eq!(
                mgr.has_pending_async(),
                state == PermissionState::Requested,
                "has_pending_async({state:?})"
            );
        }

        // One in-flight prompt among many resolved capabilities still arms the
        // pump — otherwise the outcome never reaches callbacks in an idle app.
        let mut mgr = PermissionManager::new();
        for cap in ALL_CAPS {
            mgr.set_status(cap, PermissionState::Granted(PermissionQuality::Full));
        }
        assert!(!mgr.has_pending_async());
        mgr.set_status(Capability::AppTrackingTransparency, PermissionState::Requested);
        assert!(mgr.has_pending_async());
        mgr.set_status(Capability::AppTrackingTransparency, PermissionState::Denied);
        assert!(!mgr.has_pending_async());
    }

    #[test]
    fn pending_changed_targets_the_root_when_there_is_no_subscriber() {
        let mut mgr = PermissionManager::new();
        // A status flip with no bearing node (OS revoked it while nothing was
        // mounted) must still dispatch — targeted at the window root.
        mgr.set_status(Capability::Microphone, PermissionState::Denied);
        let events = mgr.get_pending_events(ts(42));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::PermissionChanged);
        assert_eq!(events[0].source, CoreEventSource::User);
        assert_eq!(events[0].target, DomNodeId::ROOT);
        assert_eq!(events[0].timestamp, ts(42), "the caller's timestamp is preserved");
    }

    #[test]
    fn pending_changed_falls_back_to_root_after_the_subscriber_leaves() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Microphone, node(4));
        mgr.set_status(Capability::Microphone, PermissionState::Requested);
        assert_eq!(mgr.get_pending_events(ts(0))[0].target, node(4));
        mgr.clear_pending_changed();

        // The node unmounts, then the OS answer lands: last_subscriber is
        // cleared, so the event must NOT point at a stale node index.
        mgr.release(Capability::Microphone);
        mgr.set_status(Capability::Microphone, PermissionState::Denied);
        let events = mgr.get_pending_events(ts(0));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].target, DomNodeId::ROOT);
    }

    #[test]
    fn repeated_flips_accumulate_one_event_each_until_cleared() {
        let mut mgr = PermissionManager::new();
        let cap = Capability::LocalNetwork;
        mgr.set_status(cap, PermissionState::Requested);
        mgr.set_status(cap, PermissionState::Granted(PermissionQuality::Full));
        mgr.set_status(cap, PermissionState::Denied);
        assert_eq!(mgr.get_pending_events(ts(0)).len(), 3);
        // get_pending_events is a read, not a drain.
        assert_eq!(mgr.get_pending_events(ts(0)).len(), 3);

        mgr.clear_pending_changed();
        assert!(mgr.get_pending_events(ts(0)).is_empty());
        mgr.clear_pending_changed();
        assert!(mgr.get_pending_events(ts(0)).is_empty(), "clear is idempotent");
    }

    #[test]
    fn the_two_queues_drain_independently() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::NearbyWifi, node(1)); // -> pending_events
        mgr.set_status(Capability::NearbyWifi, PermissionState::Requested); // -> pending_changed

        // Clearing the state-flip queue must not swallow the diff events the
        // platform backend has not drained yet.
        mgr.clear_pending_changed();
        assert!(mgr.get_pending_events(ts(0)).is_empty());
        assert_eq!(mgr.take_pending_events().len(), 1);

        // ... and vice versa.
        mgr.set_status(Capability::NearbyWifi, PermissionState::Denied);
        mgr.subscribe(Capability::Contacts, node(2));
        assert_eq!(mgr.take_pending_events().len(), 1);
        assert!(mgr.take_pending_events().is_empty(), "take drains");
        assert_eq!(
            mgr.get_pending_events(ts(0)).len(),
            1,
            "take_pending_events must not clear pending_changed"
        );
    }

    // ── diff_layout ─────────────────────────────────────────────────────

    #[test]
    fn diff_layout_on_an_empty_manager_with_no_nodes_is_a_noop() {
        let mut mgr = PermissionManager::new();
        mgr.diff_layout(|_emit| {});
        assert!(mgr.statuses.is_empty());
        assert!(mgr.take_pending_events().is_empty());
        assert!(mgr.get_pending_events(ts(0)).is_empty());
    }

    #[test]
    fn diff_layout_counts_duplicates_and_anchors_the_first_node() {
        let mut mgr = PermissionManager::new();
        mgr.diff_layout(|emit| {
            emit(Capability::Camera, node(3));
            emit(Capability::Camera, node(4));
            emit(Capability::Camera, node(5));
        });

        assert_eq!(mgr.refcount(Capability::Camera), 3);
        assert_eq!(
            mgr.statuses[&Capability::Camera].last_subscriber,
            Some(node(3)),
            "the FIRST emitted node anchors the capability"
        );
        assert_eq!(
            mgr.take_pending_events(),
            [PermissionDiffEvent::Subscribe {
                capability: Capability::Camera,
                node_id: node(3),
            }]
            .to_vec(),
            "three bearing nodes still mean exactly one native subscribe"
        );
    }

    #[test]
    fn diff_layout_is_idempotent_for_a_stable_layout() {
        let mut mgr = PermissionManager::new();
        for _ in 0..5 {
            mgr.diff_layout(|emit| {
                emit(Capability::Camera, node(1));
                emit(Capability::Microphone, node(2));
            });
        }
        assert_eq!(mgr.refcount(Capability::Camera), 1);
        assert_eq!(mgr.refcount(Capability::Microphone), 1);
        assert_eq!(
            mgr.take_pending_events().len(),
            2,
            "an unchanged layout must not re-emit subscribes every frame"
        );
    }

    #[test]
    fn diff_layout_emits_release_and_subscribe_in_the_same_frame() {
        let mut mgr = PermissionManager::new();
        mgr.diff_layout(|emit| emit(Capability::Camera, node(1)));
        drop(mgr.take_pending_events());

        // The camera node is swapped for a geolocation node in one pass.
        mgr.diff_layout(|emit| emit(Capability::Geolocation, node(2)));

        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(events.contains(&PermissionDiffEvent::Release {
            capability: Capability::Camera
        }));
        assert!(events.contains(&PermissionDiffEvent::Subscribe {
            capability: Capability::Geolocation,
            node_id: node(2),
        }));
        assert_eq!(mgr.refcount(Capability::Camera), 0);
        assert_eq!(mgr.refcount(Capability::Geolocation), 1);
    }

    #[test]
    fn diff_layout_reconciles_a_manually_subscribed_capability() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Camera, node(1));
        mgr.subscribe(Capability::Camera, node(2));
        drop(mgr.take_pending_events());

        // The layout pass is authoritative: no bearing nodes this frame means
        // the refcount goes to 0 outright, not 2 -> 1.
        mgr.diff_layout(|_emit| {});
        assert_eq!(mgr.refcount(Capability::Camera), 0);
        assert_eq!(mgr.statuses[&Capability::Camera].last_subscriber, None);
        assert_eq!(
            mgr.take_pending_events(),
            [PermissionDiffEvent::Release {
                capability: Capability::Camera
            }]
            .to_vec()
        );
    }

    #[test]
    fn diff_layout_ignores_capabilities_that_only_have_a_status() {
        let mut mgr = PermissionManager::new();
        // set_status leaves a refcount-0 entry behind; diff_layout iterates
        // every known capability, so it must not emit a spurious Release.
        for cap in ALL_CAPS {
            mgr.set_status(cap, PermissionState::Denied);
        }
        mgr.diff_layout(|_emit| {});
        assert!(
            mgr.take_pending_events().is_empty(),
            "0 -> 0 is not a transition"
        );
        for cap in ALL_CAPS {
            assert_eq!(mgr.refcount(cap), 0);
            assert_eq!(mgr.get_status(cap), PermissionState::Denied);
        }
    }

    #[test]
    fn diff_layout_from_a_saturated_refcount_does_not_emit() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Motion, node(1));
        drop(mgr.take_pending_events());
        mgr.statuses.get_mut(&Capability::Motion).unwrap().refcount = u32::MAX;

        // MAX -> 1 is neither a 0 -> 1 nor an n -> 0 transition: the session
        // stays up and nothing is emitted.
        mgr.diff_layout(|emit| emit(Capability::Motion, node(1)));
        assert_eq!(mgr.refcount(Capability::Motion), 1);
        assert!(mgr.take_pending_events().is_empty());
    }

    #[test]
    fn diff_layout_handles_every_capability_at_once() {
        let mut mgr = PermissionManager::new();
        mgr.diff_layout(|emit| {
            for (i, cap) in ALL_CAPS.iter().enumerate() {
                emit(*cap, node(i + 1));
            }
        });

        let events = mgr.take_pending_events();
        assert_eq!(events.len(), ALL_CAPS.len(), "one Subscribe per capability");
        for (i, cap) in ALL_CAPS.iter().enumerate() {
            assert_eq!(mgr.refcount(*cap), 1);
            assert!(
                events.contains(&PermissionDiffEvent::Subscribe {
                    capability: *cap,
                    node_id: node(i + 1),
                }),
                "missing Subscribe for {cap:?}"
            );
        }

        // ... and tears them all down again.
        mgr.diff_layout(|_emit| {});
        assert_eq!(mgr.take_pending_events().len(), ALL_CAPS.len());
    }

    #[test]
    fn diff_layout_event_order_is_deterministic() {
        let run = || {
            let mut mgr = PermissionManager::new();
            mgr.diff_layout(|emit| {
                emit(Capability::Notifications, node(1));
                emit(Capability::Camera, node(2));
                emit(Capability::Geolocation, node(3));
            });
            let first = mgr.take_pending_events();
            mgr.diff_layout(|emit| emit(Capability::Camera, node(2)));
            let second = mgr.take_pending_events();
            (first, second)
        };
        assert_eq!(run(), run(), "the diff-event order must not depend on run order");
    }

    #[test]
    fn diff_layout_accepts_the_root_sentinel_as_a_bearing_node() {
        let mut mgr = PermissionManager::new();
        // A bearing node whose NodeHierarchyItemId is NONE encodes to exactly
        // the same value as the `unwrap_or(ROOT)` fallback — assert we still
        // subscribe rather than panicking or skipping it.
        mgr.diff_layout(|emit| emit(Capability::Biometric, DomNodeId::ROOT));
        assert_eq!(mgr.refcount(Capability::Biometric), 1);
        assert_eq!(
            mgr.take_pending_events(),
            [PermissionDiffEvent::Subscribe {
                capability: Capability::Biometric,
                node_id: DomNodeId::ROOT,
            }]
            .to_vec()
        );
    }

    #[test]
    fn diff_layout_handles_a_large_bearing_node_set() {
        let mut mgr = PermissionManager::new();
        const N: usize = 10_000;
        mgr.diff_layout(|emit| {
            for i in 1..=N {
                emit(Capability::PhotoLibraryWrite, node(i));
            }
        });
        assert_eq!(mgr.refcount(Capability::PhotoLibraryWrite), N as u32);
        assert_eq!(mgr.take_pending_events().len(), 1);

        mgr.diff_layout(|_emit| {});
        assert_eq!(mgr.refcount(Capability::PhotoLibraryWrite), 0);
        assert_eq!(mgr.take_pending_events().len(), 1);
    }

    #[test]
    fn diff_layout_does_not_disturb_the_state_machine() {
        let mut mgr = PermissionManager::new();
        mgr.set_status(
            Capability::Geolocation,
            PermissionState::EphemeralGranted(true),
        );
        mgr.clear_pending_changed();

        mgr.diff_layout(|emit| emit(Capability::Geolocation, node(1)));
        mgr.diff_layout(|_emit| {});

        // Subscribe/Release churn must never mutate the OS-observed state or
        // synthesize a PermissionChanged event out of thin air.
        assert_eq!(
            mgr.get_status(Capability::Geolocation),
            PermissionState::EphemeralGranted(true)
        );
        assert!(mgr.get_pending_events(ts(0)).is_empty());
    }

    // ── clone / equality ────────────────────────────────────────────────

    #[test]
    fn cloning_a_manager_deep_copies_its_queues() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Camera, node(1));
        mgr.set_status(Capability::Camera, PermissionState::Requested);

        let mut clone = mgr.clone();
        assert_eq!(clone, mgr);

        clone.subscribe(Capability::Camera, node(2));
        clone.force_release(Capability::Microphone);
        drop(clone.take_pending_events());
        clone.clear_pending_changed();

        assert_ne!(clone, mgr, "the clone shares state with the original");
        assert_eq!(mgr.refcount(Capability::Camera), 1);
        assert_eq!(mgr.take_pending_events().len(), 1, "original's queue was drained");
        assert_eq!(mgr.get_pending_events(ts(0)).len(), 1);
    }

    // ── NodeIdRemap ─────────────────────────────────────────────────────

    #[test]
    fn remap_rewrites_the_subscriber_and_the_queued_target() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Camera, node(3)); // node(3) == NodeId(2)
        mgr.set_status(Capability::Camera, PermissionState::Requested);

        // The DOM was rebuilt and the bearing node moved 2 -> 9.
        let map = NodeIdMap::from_pairs([(NodeId::new(2), NodeId::new(9))]);
        mgr.remap_node_ids(DomId::ROOT_ID, &map);

        assert_eq!(
            mgr.statuses[&Capability::Camera].last_subscriber,
            Some(node(10)), // node(10) == NodeId(9)
        );
        assert_eq!(mgr.get_pending_events(ts(0))[0].target, node(10));
    }

    #[test]
    fn remap_drops_an_unmounted_subscriber_instead_of_recycling_the_index() {
        let mut mgr = PermissionManager::new();
        mgr.subscribe(Capability::Camera, node(3));
        mgr.set_status(Capability::Camera, PermissionState::Requested);

        // Empty map == everything was unmounted.
        mgr.remap_node_ids(DomId::ROOT_ID, &NodeIdMap::default());

        assert_eq!(
            mgr.statuses[&Capability::Camera].last_subscriber,
            None,
            "an unmounted subscriber must fall back to None, never to a live-but-wrong node"
        );
        assert_eq!(
            mgr.get_pending_events(ts(0))[0].target,
            DomNodeId::ROOT,
            "the queued event retargets to the window root"
        );
        // The permission state itself survives the DOM rebuild.
        assert_eq!(mgr.get_status(Capability::Camera), PermissionState::Requested);
    }

    #[test]
    fn remap_leaves_nodes_from_other_doms_untouched() {
        let mut mgr = PermissionManager::new();
        let foreign = node_in_dom(7, 3);
        mgr.subscribe(Capability::Camera, foreign);
        mgr.set_status(Capability::Camera, PermissionState::Requested);

        // A reconciliation of DOM 0 says nothing about DOM 7 — dropping the
        // subscriber here would silently retarget an iframe's prompt at the
        // root window.
        mgr.remap_node_ids(DomId::ROOT_ID, &NodeIdMap::default());

        assert_eq!(
            mgr.statuses[&Capability::Camera].last_subscriber,
            Some(foreign)
        );
        assert_eq!(mgr.get_pending_events(ts(0))[0].target, foreign);
    }

    // ── async channel (process-global) ──────────────────────────────────

    #[test]
    fn async_channel_preserves_arrival_order_across_all_states() {
        let _serialize = lock_async_channel();
        drop(drain_async_results());

        for state in ALL_STATES {
            push_async_result(Capability::Camera, state);
        }
        let drained = drain_async_results();
        assert_eq!(drained.len(), ALL_STATES.len());
        for (i, state) in ALL_STATES.iter().enumerate() {
            assert_eq!(drained[i], (Capability::Camera, *state));
        }
        assert!(drain_async_results().is_empty(), "the queue is taken, not copied");
    }

    #[test]
    fn async_channel_recovers_from_a_poisoned_lock() {
        let _serialize = lock_async_channel();
        drop(drain_async_results());

        // Poison the global mutex the way a panicking applier would: unwind
        // out of a live guard. (The panic message below is expected output if
        // this test ever fails; libtest swallows it while it passes.)
        let unwound = std::panic::catch_unwind(|| {
            let _guard = ASYNC_RESULTS.lock().unwrap();
            panic!("intentional: poisoning ASYNC_RESULTS");
        });
        assert!(unwound.is_err(), "the panic must have unwound through the guard");
        assert!(ASYNC_RESULTS.is_poisoned(), "the lock should now be poisoned");

        // Documented contract: delivery keeps working after a poisoning.
        push_async_result(Capability::Geolocation, PermissionState::Denied);
        let drained = drain_async_results();
        assert_eq!(
            drained,
            [(Capability::Geolocation, PermissionState::Denied)].to_vec(),
            "a poisoned lock must not wedge permission delivery forever"
        );
        assert!(drain_async_results().is_empty());
    }

    #[test]
    fn async_channel_survives_concurrent_pushers() {
        let _serialize = lock_async_channel();
        drop(drain_async_results());

        const THREADS: usize = 8;
        const PER_THREAD: usize = 50;

        let handles: Vec<_> = (0..THREADS)
            .map(|t| {
                let cap = ALL_CAPS[t];
                std::thread::spawn(move || {
                    for _ in 0..PER_THREAD {
                        push_async_result(cap, PermissionState::Granted(PermissionQuality::Full));
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().expect("a pusher thread panicked");
        }

        let drained = drain_async_results();
        assert_eq!(drained.len(), THREADS * PER_THREAD, "results were lost");
        for t in 0..THREADS {
            let count = drained.iter().filter(|(c, _)| *c == ALL_CAPS[t]).count();
            assert_eq!(count, PER_THREAD, "{:?} lost results", ALL_CAPS[t]);
        }
        assert!(drain_async_results().is_empty());
    }

    #[test]
    fn draining_an_empty_async_channel_is_safe_and_repeatable() {
        let _serialize = lock_async_channel();
        for _ in 0..3 {
            assert!(drain_async_results().is_empty());
        }
    }
}

impl crate::managers::NodeIdRemap for PermissionManager {
    /// Remap the `last_subscriber` node of each capability (the node a
    /// `PermissionChanged` event is targeted at) and the queued
    /// `pending_changed` targets. An unmounted subscriber falls back to `None`
    /// (→ the event targets the window root), never to a recycled index.
    fn remap_node_ids(&mut self, dom: azul_core::dom::DomId, map: &crate::managers::NodeIdMap) {
        for entry in self.statuses.values_mut() {
            if let Some(node) = entry.last_subscriber {
                entry.last_subscriber = map.resolve_dom_node_id(dom, node);
            }
        }
        for (_capability, node) in &mut self.pending_changed {
            if let Some(n) = *node {
                *node = map.resolve_dom_node_id(dom, n);
            }
        }
    }
}
