//! Application-level hotplug events — devices and monitors arriving or leaving.
//!
//! These differ from every other manager in this directory: they are not
//! per-node and not hit-tested. A monitor being unplugged belongs to the
//! application, not to whichever node happened to be under the cursor, so the
//! events dispatch at the root and reach subscribers by propagation. That is
//! what the `ApplicationEventFilter` family is for.
//!
//! The family was structurally unreachable before this arc — `matches_filter_phase`
//! returned a flat `false` for it — so nothing had ever needed a producer and
//! no queue existed for the shells to push into. This is that queue.
//!
//! Gamepads do NOT go through here: `GamepadManager` already owns pad state and
//! detects its own arrive/leave edges, so routing them here as well would
//! double-fire.

use alloc::vec::Vec;

use azul_core::{
    dom::DomNodeId,
    events::{EventData, EventProvider, EventSource, EventType, SyntheticEvent},
    task::Instant,
};

/// What kind of thing was plugged or unplugged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotplugKind {
    /// An input device (keyboard, pointer, touch, tablet tool or pad).
    Device,
    /// A display.
    Monitor,
}

/// One hotplug transition awaiting emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotplug {
    /// Device or monitor.
    pub kind: HotplugKind,
    /// `true` = arrived, `false` = left.
    pub connected: bool,
}

/// Raw pointer motion awaiting dispatch, in device units.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PendingRawMotion {
    pub dx: f64,
    pub dy: f64,
    pub device_id: u64,
}

/// Collects hotplug transitions from the platform backends and yields them as
/// `ApplicationEventFilter` events.
// No Eq: raw motion is f64, and a float has no total equality.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceEventManager {
    pending: Vec<Hotplug>,
    /// Raw motion accumulated since the last drain. Coalesced rather than
    /// queued: a gaming mouse reports up to 1000 times a second, and one
    /// callback per report would swamp an app that only wants to know how far
    /// the pointer moved this frame.
    pending_raw_motion: Option<PendingRawMotion>,
}

impl DeviceEventManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that an input device arrived or left.
    ///
    /// Called from the backends that can actually observe it: Wayland
    /// `wl_seat.capabilities` and the `zwp_tablet_seat_v2` add/remove events,
    /// X11 `XI_HierarchyChanged`, Win32 `WM_DEVICECHANGE`.
    pub fn note_device(&mut self, connected: bool) {
        self.pending.push(Hotplug {
            kind: HotplugKind::Device,
            connected,
        });
    }

    /// Record that a monitor arrived or left.
    ///
    /// Wayland gets this from `wl_registry.global`/`global_remove` for
    /// `wl_output`, Win32 from `WM_DISPLAYCHANGE`, macOS from
    /// `NSApplicationDidChangeScreenParameters`, X11 from RandR.
    pub fn note_monitor(&mut self, connected: bool) {
        self.pending.push(Hotplug {
            kind: HotplugKind::Monitor,
            connected,
        });
    }

    /// Record a monitor-count change as the arrivals/departures it implies.
    ///
    /// The backends mostly learn "the display topology changed, here is the
    /// new list" rather than "monitor X left", so this turns a before/after
    /// count into events. Equal counts emit nothing — a resolution or
    /// position change is `WindowMonitorChanged`, not a hotplug.
    pub fn note_monitor_count_change(&mut self, before: usize, after: usize) {
        for _ in before..after {
            self.note_monitor(true);
        }
        for _ in after..before {
            self.note_monitor(false);
        }
    }

    /// Accumulate raw pointer motion. Called by the backends while the
    /// pointer is locked.
    pub fn note_raw_motion(&mut self, dx: f64, dy: f64, device_id: u64) {
        let e = self.pending_raw_motion.get_or_insert(PendingRawMotion {
            dx: 0.0,
            dy: 0.0,
            device_id,
        });
        e.dx += dx;
        e.dy += dy;
        e.device_id = device_id;
    }

    /// Read the accumulated raw motion without consuming it — what a
    /// callback does while the event is being dispatched.
    #[must_use]
    pub const fn peek_raw_motion(&self) -> Option<PendingRawMotion> {
        self.pending_raw_motion
    }

    /// Drain the accumulated raw motion.
    pub fn take_raw_motion(&mut self) -> Option<PendingRawMotion> {
        self.pending_raw_motion.take()
    }

    /// Whether anything is queued (lets a backend skip the drain entirely).
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Drain the queue.
    pub fn take_pending(&mut self) -> Vec<Hotplug> {
        core::mem::take(&mut self.pending)
    }
}

impl EventProvider for DeviceEventManager {
    /// Yield `DeviceConnected` / `DeviceDisconnected` / `MonitorConnected` /
    /// `MonitorDisconnected` at the root for each queued transition.
    fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
        let mut events: Vec<SyntheticEvent> = Vec::new();
        if let Some(m) = self.pending_raw_motion {
            events.push(SyntheticEvent::new(
                EventType::RawMouseMotion,
                EventSource::User,
                DomNodeId::ROOT,
                timestamp.clone(),
                EventData::RawMotion(azul_core::events::RawMotionEventData {
                    dx: m.dx,
                    dy: m.dy,
                    device_id: m.device_id,
                }),
            ));
        }
        events.extend(
            self.pending
            .iter()
            .map(|h| {
                let event_type = match (h.kind, h.connected) {
                    (HotplugKind::Device, true) => EventType::DeviceConnected,
                    (HotplugKind::Device, false) => EventType::DeviceDisconnected,
                    (HotplugKind::Monitor, true) => EventType::MonitorConnected,
                    (HotplugKind::Monitor, false) => EventType::MonitorDisconnected,
                };
                SyntheticEvent::new(
                    event_type,
                    EventSource::User,
                    DomNodeId::ROOT,
                    timestamp.clone(),
                    EventData::None,
                )
            }),
        );
        events
    }
}

#[cfg(test)]
mod monitor_hotplug_tests {
    use azul_core::events::{EventProvider, EventType};

    use super::DeviceEventManager;

    fn kinds(m: &DeviceEventManager) -> Vec<EventType> {
        let ts = azul_core::task::Instant::Tick(azul_core::task::SystemTick::new(0));
        m.get_pending_events(ts)
            .iter()
            .map(|e| e.event_type)
            .collect()
    }

    /// A count diff becomes the arrivals and departures it implies.
    ///
    /// The backends mostly learn "the display topology changed, here is the
    /// new list" rather than "output HDMI-1 left" — RandR, `WM_DISPLAYCHANGE`
    /// and `didChangeScreenParameters` all report it that way — so the count
    /// is what there is to work from.
    #[test]
    fn a_monitor_count_change_becomes_arrivals_and_departures() {
        let mut m = DeviceEventManager::default();
        m.note_monitor_count_change(1, 3);
        assert_eq!(
            kinds(&m),
            vec![EventType::MonitorConnected, EventType::MonitorConnected],
            "two monitors arrived",
        );

        let mut m = DeviceEventManager::default();
        m.note_monitor_count_change(3, 1);
        assert_eq!(
            kinds(&m),
            vec![
                EventType::MonitorDisconnected,
                EventType::MonitorDisconnected
            ],
            "two monitors left",
        );
    }

    /// An UNCHANGED count is not a hotplug.
    ///
    /// A screen-configuration event fires for a resolution or position change
    /// too, and reporting those as a disconnect-plus-connect would make an app
    /// tear down and rebuild per-monitor state on every mode switch. That is
    /// `WindowMonitorChanged`, not a hotplug.
    #[test]
    fn an_unchanged_monitor_count_emits_nothing() {
        let mut m = DeviceEventManager::default();
        m.note_monitor_count_change(2, 2);
        assert!(kinds(&m).is_empty());
        m.note_monitor_count_change(0, 0);
        assert!(kinds(&m).is_empty());
    }
}

