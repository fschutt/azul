//! Eyedropper manager - the state behind `CallbackInfo::pick_screen_color`.
//!
//! **Request-driven**, like the biometric manager, and shaped the same way:
//!
//! - A **callback** (the colour picker's eyedropper icon) calls
//!   `CallbackInfo::pick_screen_color()`. That allocates a request id, records it on THIS window's
//!   manager and parks the request in the app's [`EyedropperChannel`]
//!   ([`EyedropperChannel::push_request`]).
//!
//! - The dll's event pass drains the channel ([`EyedropperChannel::drain_requests`]) and hands
//!   each request to the platform eyedropper: macOS's system sampler (`NSColorSampler`, no
//!   screen-recording permission needed), or - on X11, Windows and Wayland - a screenshot (Wayland
//!   asks the user through the desktop portal first; the others can read the screen freely) shown
//!   in a fullscreen loupe window where the user picks a pixel.
//!
//! - When the user picks or cancels, the backend parks the outcome in
//!   [`EyedropperChannel::push_result`] (it is handed a clone of the channel with the request).
//!   Every window's pass drains the results addressed TO IT
//!   ([`EyedropperChannel::drain_results_for`]) - results are routed by request id, so the window
//!   whose callback asked is the window whose callbacks hear `EventType::ScreenColorPicked`
//!   (window-level, target = root) and read the colour with
//!   `CallbackInfo::get_picked_screen_color()`.
//!
//! While any pick is in flight ([`EyedropperChannel::in_flight_anywhere`]) a popup must not
//! light-dismiss on focus loss: the loupe window (or the system sampler)
//! takes the focus, and the colour picker popup that asked has to still be
//! there when the answer comes back.
//!
//! No platform deps: `azul-layout` stays platform-free; the dll owns the
//! screen reading. The channel belongs to the app (`AppServices`), not to the process.

use alloc::{sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use super::shared_queue::SharedQueue;

use azul_core::{
    dom::DomNodeId,
    events::{EventData, EventProvider, EventSource as CoreEventSource, EventType, SyntheticEvent},
    task::Instant,
};
use azul_css::props::basic::color::ColorU;

/// Per-window eyedropper state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EyedropperManager {
    /// Request ids this window issued whose outcome is still outstanding.
    /// Non-empty keeps the capability pump's timer armed so the reply
    /// reaches callbacks in an otherwise idle app.
    issued: Vec<u64>,
    /// The outcome of the most recently completed pick: `Some(None)` =
    /// cancelled, `Some(Some(c))` = picked, `None` = never completed. The
    /// three states are the point — the nested option is deliberate.
    #[allow(clippy::option_option)]
    last_result: Option<Option<ColorU>>,
    /// A pick completed since the last event pass - yield the event once.
    pending_event: bool,
}

impl EyedropperManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a request id on the app's `channel` and record it as
    /// outstanding on this window.
    pub fn begin_request(&mut self, channel: &EyedropperChannel) -> u64 {
        let id = channel.begin();
        self.issued.push(id);
        id
    }

    /// The ids this window is waiting on.
    #[must_use]
    pub fn issued(&self) -> &[u64] {
        &self.issued
    }

    /// `true` while this window waits on a pick (the pump's arming signal).
    #[must_use]
    pub const fn has_pending_async(&self) -> bool {
        !self.issued.is_empty()
    }

    /// Fold a result addressed to one of this window's requests; the pick is
    /// no longer in flight on the app's `channel`. Returns whether it was ours
    /// (and therefore folded).
    pub fn fold_result(
        &mut self,
        channel: &EyedropperChannel,
        request_id: u64,
        color: Option<ColorU>,
    ) -> bool {
        let Some(i) = self.issued.iter().position(|id| *id == request_id) else {
            return false;
        };
        self.issued.remove(i);
        channel.finish();
        self.last_result = Some(color);
        self.pending_event = true;
        true
    }

    /// The most recently completed pick: `None` until one completes,
    /// `Some(None)` when the user cancelled.
    #[must_use]
    pub const fn last_result(&self) -> Option<Option<ColorU>> {
        self.last_result
    }

    /// Move every outstanding request id out of this manager — for a window
    /// that is closing while a pick it asked for is still in flight. The
    /// app's in-flight count is deliberately untouched: the ids MUST be
    /// handed to another window's manager ([`Self::adopt_issued`]), or the
    /// count leaks and popups stop light-dismissing forever.
    pub fn take_issued(&mut self) -> Vec<u64> {
        core::mem::take(&mut self.issued)
    }

    /// Adopt outstanding request ids taken from a closing window — the color
    /// picker popup that asked for a screen pick and died (compositor
    /// `popup_done`, the loupe's grab break) before the answer arrived. Results
    /// are routed by request id, so after adoption THIS window's pass drains
    /// them and its `ScreenColorPicked` fires here instead of nowhere.
    pub fn adopt_issued(&mut self, ids: Vec<u64>) {
        self.issued.extend(ids);
    }

    /// The dll clears this after the event pass collected the event.
    pub const fn clear_pending_event(&mut self) {
        self.pending_event = false;
    }
}

impl EventProvider for EyedropperManager {
    /// One window-level `ScreenColorPicked` per completed pick.
    fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
        if self.pending_event {
            alloc::vec![SyntheticEvent::new(
                EventType::ScreenColorPicked,
                CoreEventSource::User,
                DomNodeId::ROOT,
                timestamp,
                EventData::None,
            )]
        } else {
            Vec::new()
        }
    }
}

/// A pick the platform backend has to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EyedropperRequest {
    /// Routes the result back to the window that asked.
    pub request_id: u64,
}

/// A finished pick: the colour under the pointer, or `None` if cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EyedropperResult {
    pub request_id: u64,
    pub color: Option<ColorU>,
}

/// The ids and the picks in flight of one app, shared by its windows so that a result finds the
/// window that asked.
#[derive(Debug)]
struct EyedropperCounters {
    /// The id of the next request (starts at 1, so `0` is never an id).
    next_request_id: AtomicU64,
    /// Picks in flight across every window of the app.
    in_flight: AtomicUsize,
}

/// The picks of one app: the requests the windows' callbacks queued for the platform, the
/// results the platform parked, the request ids and the count of picks in flight. Every window
/// of an app holds a clone (`LayoutWindow::join_app`), and the backend is handed one with each
/// request it runs. A clone is another handle on the SAME channel.
#[derive(Debug, Clone)]
pub struct EyedropperChannel {
    requests: SharedQueue<EyedropperRequest>,
    results: SharedQueue<EyedropperResult>,
    counters: Arc<EyedropperCounters>,
}

impl Default for EyedropperChannel {
    fn default() -> Self {
        Self {
            requests: SharedQueue::new(),
            results: SharedQueue::new(),
            counters: Arc::new(EyedropperCounters {
                next_request_id: AtomicU64::new(1),
                in_flight: AtomicUsize::new(0),
            }),
        }
    }
}

impl EyedropperChannel {
    /// A new channel of its own, shared with nobody.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a request id and count the pick as in flight.
    fn begin(&self) -> u64 {
        self.counters.in_flight.fetch_add(1, Ordering::Relaxed);
        self.counters.next_request_id.fetch_add(1, Ordering::Relaxed)
    }

    /// A pick's result was folded: it is no longer in flight.
    fn finish(&self) {
        self.counters.in_flight.fetch_sub(1, Ordering::Relaxed);
    }

    /// `true` while any window of the app waits on a pick. Popups read this to stay open through
    /// the focus loss the loupe / system sampler causes.
    #[must_use]
    pub fn in_flight_anywhere(&self) -> bool {
        self.counters.in_flight.load(Ordering::Relaxed) > 0
    }

    /// Queue a pick for the platform backend (from a callback, any thread).
    pub fn push_request(&self, request: EyedropperRequest) {
        self.requests.push(request);
    }

    /// Take every queued pick. The dll dispatches each to the platform.
    pub fn drain_requests(&self) -> Vec<EyedropperRequest> {
        self.requests.drain()
    }

    /// Park a finished pick (from the platform backend - the loupe window's click handler, or
    /// the system sampler's completion block).
    pub fn push_result(&self, result: EyedropperResult) {
        self.results.push(result);
    }

    /// Take the parked results addressed to `ids`, leaving the others for the window that issued
    /// them.
    pub fn drain_results_for(&self, ids: &[u64]) -> Vec<EyedropperResult> {
        self.results.drain_matching(|r| ids.contains(&r.request_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_are_routed_to_the_window_that_asked() {
        // Two windows of one app share its channel.
        let channel = EyedropperChannel::new();
        let mut a = EyedropperManager::new();
        let mut b = EyedropperManager::new();
        let ra = a.begin_request(&channel);
        let rb = b.begin_request(&channel);
        assert!(channel.in_flight_anywhere());
        channel.push_result(EyedropperResult {
            request_id: rb,
            color: None,
        });
        channel.push_result(EyedropperResult {
            request_id: ra,
            color: Some(ColorU {
                r: 1,
                g: 2,
                b: 3,
                a: 255,
            }),
        });

        let for_a = channel.drain_results_for(a.issued());
        assert_eq!(for_a.len(), 1);
        assert_eq!(for_a[0].request_id, ra);
        for r in for_a {
            assert!(a.fold_result(&channel, r.request_id, r.color));
        }
        assert_eq!(
            a.last_result(),
            Some(Some(ColorU {
                r: 1,
                g: 2,
                b: 3,
                a: 255
            }))
        );
        assert!(!a.has_pending_async());
        assert_eq!(
            a.get_pending_events(Instant::Tick(azul_core::task::SystemTick {
                tick_counter: 0
            }))
            .len(),
            1
        );

        // b's result was left in the channel for b.
        let for_b = channel.drain_results_for(b.issued());
        assert_eq!(for_b.len(), 1);
        assert!(b.fold_result(&channel, rb, None));
        assert_eq!(b.last_result(), Some(None), "cancelled");
        assert!(!channel.in_flight_anywhere());

        // A result for nobody is not ours.
        assert!(!a.fold_result(&channel, 999, None));
    }

    #[test]
    fn the_picks_of_another_app_are_not_in_flight_here() {
        let ours = EyedropperChannel::new();
        let theirs = EyedropperChannel::new();
        let mut manager = EyedropperManager::new();
        let id = manager.begin_request(&ours);
        assert!(ours.in_flight_anywhere());
        assert!(!theirs.in_flight_anywhere(), "another app has its own count");
        ours.push_request(EyedropperRequest { request_id: id });
        assert!(theirs.drain_requests().is_empty());
        assert_eq!(ours.drain_requests(), vec![EyedropperRequest { request_id: id }]);
    }

    #[test]
    fn request_ids_are_the_apps_own_and_start_at_one() {
        let first = EyedropperChannel::new();
        let second = EyedropperChannel::new();
        let mut manager = EyedropperManager::new();
        assert_eq!(manager.begin_request(&first), 1);
        assert_eq!(manager.begin_request(&first), 2);
        assert_eq!(manager.begin_request(&second), 1, "another app counts from one again");
    }
}
