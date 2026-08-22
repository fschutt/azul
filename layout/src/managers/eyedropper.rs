//! Eyedropper manager - the state behind `CallbackInfo::pick_screen_color`.
//!
//! **Request-driven**, like the biometric manager, and shaped the same way:
//!
//! - A **callback** (the colour picker's eyedropper icon) calls
//!   `CallbackInfo::pick_screen_color()`. That allocates a request id,
//!   records it on THIS window's manager and parks the request in the
//!   process-global channel [`push_request`].
//!
//! - The dll's event pass drains the channel ([`drain_requests`]) and hands
//!   each request to the platform eyedropper: macOS's system sampler
//!   (`NSColorSampler`, no screen-recording permission needed), or - on X11,
//!   Windows and Wayland - a screenshot (Wayland asks the user through the
//!   desktop portal first; the others can read the screen freely) shown in a
//!   fullscreen loupe window where the user picks a pixel.
//!
//! - When the user picks or cancels, the backend parks the outcome in
//!   [`push_result`]. Every window's pass drains the results addressed TO
//!   IT ([`drain_results_for`]) - results are routed by request id, so the
//!   window whose callback asked is the window whose callbacks hear
//!   `EventType::ScreenColorPicked` (window-level, target = root) and read
//!   the colour with `CallbackInfo::get_picked_screen_color()`.
//!
//! While any pick is in flight ([`in_flight_anywhere`]) a popup must not
//! light-dismiss on focus loss: the loupe window (or the system sampler)
//! takes the focus, and the colour picker popup that asked has to still be
//! there when the answer comes back.
//!
//! No platform deps: `azul-layout` stays platform-free; the dll owns the
//! screen reading.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use azul_core::dom::DomNodeId;
use azul_core::events::{
    EventData, EventProvider, EventSource as CoreEventSource, EventType, SyntheticEvent,
};
use azul_core::task::Instant;
use azul_css::props::basic::color::ColorU;

/// Per-window eyedropper state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EyedropperManager {
    /// Request ids this window issued whose outcome is still outstanding.
    /// Non-empty keeps the capability pump's timer armed so the reply
    /// reaches callbacks in an otherwise idle app.
    issued: Vec<u64>,
    /// The outcome of the most recently completed pick: `Some(None)` =
    /// cancelled, `Some(Some(c))` = picked, `None` = never completed.
    last_result: Option<Option<ColorU>>,
    /// A pick completed since the last event pass - yield the event once.
    pending_event: bool,
}

impl EyedropperManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a request id and record it as outstanding on this window.
    pub fn begin_request(&mut self) -> u64 {
        let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        self.issued.push(id);
        IN_FLIGHT.fetch_add(1, Ordering::Relaxed);
        id
    }

    /// The ids this window is waiting on.
    #[must_use]
    pub fn issued(&self) -> &[u64] {
        &self.issued
    }

    /// `true` while this window waits on a pick (the pump's arming signal).
    #[must_use]
    pub fn has_pending_async(&self) -> bool {
        !self.issued.is_empty()
    }

    /// Fold a result addressed to one of this window's requests. Returns
    /// whether it was ours (and therefore folded).
    pub fn fold_result(&mut self, request_id: u64, color: Option<ColorU>) -> bool {
        let Some(i) = self.issued.iter().position(|id| *id == request_id) else {
            return false;
        };
        self.issued.remove(i);
        IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
        self.last_result = Some(color);
        self.pending_event = true;
        true
    }

    /// The most recently completed pick: `None` until one completes,
    /// `Some(None)` when the user cancelled.
    #[must_use]
    pub fn last_result(&self) -> Option<Option<ColorU>> {
        self.last_result
    }

    /// The dll clears this after the event pass collected the event.
    pub fn clear_pending_event(&mut self) {
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

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
/// Picks in flight across every window of the process.
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// `true` while any window waits on a pick. Popups read this to stay open
/// through the focus loss the loupe / system sampler causes.
#[must_use]
pub fn in_flight_anywhere() -> bool {
    IN_FLIGHT.load(Ordering::Relaxed) > 0
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

static PENDING_REQUESTS: std::sync::Mutex<Vec<EyedropperRequest>> = std::sync::Mutex::new(Vec::new());
static PENDING_RESULTS: std::sync::Mutex<Vec<EyedropperResult>> = std::sync::Mutex::new(Vec::new());

/// Queue a pick for the platform backend (from a callback, any thread).
pub fn push_request(request: EyedropperRequest) {
    let mut q = PENDING_REQUESTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    q.push(request);
}

/// Take every queued pick. The dll dispatches each to the platform.
pub fn drain_requests() -> Vec<EyedropperRequest> {
    let mut q = PENDING_REQUESTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

/// Park a finished pick (from the platform backend - the loupe window's
/// click handler, or the system sampler's completion block).
pub fn push_result(result: EyedropperResult) {
    let mut q = PENDING_RESULTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    q.push(result);
}

/// Take the parked results addressed to `ids`, leaving the others for the
/// window that issued them.
pub fn drain_results_for(ids: &[u64]) -> Vec<EyedropperResult> {
    let mut q = PENDING_RESULTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let (mine, others): (Vec<_>, Vec<_>) = core::mem::take(&mut *q)
        .into_iter()
        .partition(|r| ids.contains(&r.request_id));
    *q = others;
    mine
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_are_routed_to_the_window_that_asked() {
        let mut a = EyedropperManager::new();
        let mut b = EyedropperManager::new();
        let ra = a.begin_request();
        let rb = b.begin_request();
        assert!(in_flight_anywhere());
        push_result(EyedropperResult { request_id: rb, color: None });
        push_result(EyedropperResult {
            request_id: ra,
            color: Some(ColorU { r: 1, g: 2, b: 3, a: 255 }),
        });

        let for_a = drain_results_for(a.issued());
        assert_eq!(for_a.len(), 1);
        assert_eq!(for_a[0].request_id, ra);
        for r in for_a {
            assert!(a.fold_result(r.request_id, r.color));
        }
        assert_eq!(a.last_result(), Some(Some(ColorU { r: 1, g: 2, b: 3, a: 255 })));
        assert!(!a.has_pending_async());
        assert_eq!(a.get_pending_events(Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 })).len(), 1);

        // b's result was left in the channel for b.
        let for_b = drain_results_for(b.issued());
        assert_eq!(for_b.len(), 1);
        assert!(b.fold_result(rb, None));
        assert_eq!(b.last_result(), Some(None), "cancelled");
        assert!(!in_flight_anywhere());

        // A result for nobody is not ours.
        assert!(!a.fold_result(999, None));
    }
}
