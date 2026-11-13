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
    events::{EventData, EventProvider, EventSource, EventType, SessionChange, SyntheticEvent},
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
    /// Session changes (the screen locked, the machine sleeps) awaiting
    /// emission, oldest first: a lock then an unlock in one batch are two
    /// things the app must do.
    pending_session: Vec<SessionChange>,
    /// The session change delivered last, which
    /// `CallbackInfo::get_session_change` answers.
    last_session: Option<SessionChange>,
    /// How far into the process's session log this window has read
    /// ([`Self::read_session_log`]).
    session_seen: u64,
}

impl DeviceEventManager {
    /// A window's manager; it hears the session changes from now on, not
    /// the ones before it was made.
    #[must_use]
    pub fn new() -> Self {
        Self {
            session_seen: session_log_position(),
            ..Self::default()
        }
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
    /// `NSApplicationDidChangeScreenParameters`, X11 from `RandR`.
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
    pub const fn take_raw_motion(&mut self) -> Option<PendingRawMotion> {
        self.pending_raw_motion.take()
    }

    /// Whether anything is queued (lets a backend skip the drain entirely).
    #[must_use]
    pub const fn has_pending(&self) -> bool {
        !self.pending.is_empty() || !self.pending_session.is_empty()
    }

    /// Drain the queue.
    pub fn take_pending(&mut self) -> Vec<Hotplug> {
        core::mem::take(&mut self.pending)
    }

    /// Record that the session changed (the platform backends push into
    /// [`push_session_change`]; the pass moves those here).
    pub fn note_session(&mut self, change: SessionChange) {
        self.pending_session.push(change);
        self.last_session = Some(change);
    }

    /// The session change delivered last, `None` before the first.
    #[must_use]
    pub const fn last_session(&self) -> Option<SessionChange> {
        self.last_session
    }

    /// Drain the session changes emitted this pass.
    pub fn take_pending_session(&mut self) -> Vec<SessionChange> {
        core::mem::take(&mut self.pending_session)
    }

    /// Take the session changes logged since this window last read the log
    /// ([`push_session_change`]), for this pass's events. Every window reads
    /// the log for itself: a lock is the application's, not one window's.
    pub fn read_session_log(&mut self) {
        let (changes, position) = session_changes_after(self.session_seen);
        self.session_seen = position;
        for change in changes {
            self.note_session(change);
        }
    }
}

/// How many session changes the log keeps for windows that have not read
/// them yet; a window misses only changes older than its last pass by more.
const SESSION_LOG_LEN: usize = 32;

/// The process's session changes, as the platform's observers report them
/// from their own threads (a workspace notification, a D-Bus signal, a
/// window message): the position of the next one, and the last few with
/// their positions.
static SESSION_LOG: std::sync::Mutex<(u64, Vec<(u64, SessionChange)>)> =
    std::sync::Mutex::new((0, Vec::new()));

/// Log one session change for every window's next pass.
pub fn push_session_change(change: SessionChange) {
    if let Ok(mut log) = SESSION_LOG.lock() {
        let position = log.0;
        log.0 += 1;
        log.1.push((position, change));
        if log.1.len() > SESSION_LOG_LEN {
            log.1.remove(0);
        }
    }
}

/// The position the next session change will take.
#[must_use]
pub fn session_log_position() -> u64 {
    SESSION_LOG.lock().map_or(0, |log| log.0)
}

/// The session changes at or after `seen`, oldest first, and the position
/// after the last.
#[must_use]
pub fn session_changes_after(seen: u64) -> (Vec<SessionChange>, u64) {
    SESSION_LOG.lock().map_or((Vec::new(), seen), |log| {
        let changes = log
            .1
            .iter()
            .filter(|(position, _)| *position >= seen)
            .map(|(_, change)| *change)
            .collect();
        (changes, log.0)
    })
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
        events.extend(self.pending_session.iter().map(|_| {
            SyntheticEvent::new(
                EventType::SessionChange,
                EventSource::User,
                DomNodeId::ROOT,
                timestamp.clone(),
                EventData::None,
            )
        }));
        events.extend(self.pending.iter().map(|h| {
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
        }));
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

/// The session's changes reach the app as `SessionChange` events at the root,
/// one per change, and the last one stays readable.
#[cfg(test)]
mod session_tests {
    use azul_core::events::{EventProvider, EventType, SessionChange};

    use super::DeviceEventManager;

    #[test]
    fn a_lock_and_a_sleep_are_two_session_events() {
        let mut m = DeviceEventManager::new();
        m.note_session(SessionChange::Locked);
        m.note_session(SessionChange::WillSleep);
        let ts = azul_core::task::Instant::Tick(azul_core::task::SystemTick::new(0));
        let kinds: Vec<EventType> = m
            .get_pending_events(ts)
            .iter()
            .map(|e| e.event_type)
            .collect();
        assert_eq!(kinds, vec![EventType::SessionChange, EventType::SessionChange]);
        assert_eq!(m.last_session(), Some(SessionChange::WillSleep));
        assert_eq!(m.take_pending_session().len(), 2);
        assert_eq!(m.last_session(), Some(SessionChange::WillSleep), "still readable");
    }

    /// The log is the process's: two windows each hear a change once, in
    /// arrival order, and a window made later hears only what came after.
    #[test]
    fn every_window_hears_each_session_change_once() {
        let mut first = DeviceEventManager::new();
        let mut second = DeviceEventManager::new();
        super::push_session_change(SessionChange::Locked);
        super::push_session_change(SessionChange::Unlocked);
        for m in [&mut first, &mut second] {
            m.read_session_log();
            let seen = m.take_pending_session();
            assert!(
                seen.ends_with(&[SessionChange::Locked, SessionChange::Unlocked]),
                "{seen:?}"
            );
            m.read_session_log();
            assert!(m.take_pending_session().is_empty(), "heard once");
        }
        let mut late = DeviceEventManager::new();
        late.read_session_log();
        assert!(late.take_pending_session().is_empty(), "nothing from before");
    }
}

