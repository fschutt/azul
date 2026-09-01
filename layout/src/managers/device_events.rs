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

/// Collects hotplug transitions from the platform backends and yields them as
/// `ApplicationEventFilter` events.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceEventManager {
    pending: Vec<Hotplug>,
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
            })
            .collect()
    }
}
