//! When a vsync-driven frame pump runs: the ONE decision behind it.
//!
//! A display-synchronised pump (macOS `CVDisplayLink`) wakes its own thread
//! and the main thread once per refresh. Left running while nothing changes,
//! it keeps an idle app from ever going idle: measured 2026-09-30, every azul
//! app sat at ~0.7% CPU doing nothing but this. The pump must run while
//! frames are WANTED and stop shortly after the last one.
//!
//! [`FramePacer`] is that decision, pure and platform-free, so it is tested
//! here rather than on a real display. The shell asks it two questions:
//!
//! * a frame is wanted ([`FramePacer::on_request`]) - start the pump, or let
//!   the running one deliver it, or (a window that shows nothing) deliver it
//!   now without a pump;
//! * the pump ticked ([`FramePacer::on_tick`]) - render the pending work,
//!   idle, or stop the pump after [`FramePacer::IDLE_TICKS_BEFORE_STOP`]
//!   ticks with nothing to do.
//!
//! A couple of idle ticks, not zero: a request usually follows a frame
//! (a scroll, a drag, a caret), and restarting the pump for each would pay
//! the start latency every frame.

/// What the shell does with a frame request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameRequest {
    /// The pump is stopped: start it. Its next tick delivers the frame.
    Start,
    /// The pump is running: its next tick delivers the frame.
    AlreadyRunning,
    /// The window cannot show a frame right now (occluded, minimized): the
    /// pump stays off, the frame is delivered without it.
    DeliverNow,
}

/// What the shell does with a pump tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickAction {
    /// Work is pending: render or deliver this frame.
    Render,
    /// Nothing pending this tick; keep pumping a little longer.
    Idle,
    /// Nothing pending for [`FramePacer::IDLE_TICKS_BEFORE_STOP`] ticks:
    /// stop the pump. The next request starts it again.
    Stop,
}

/// The state behind [`FrameRequest`] / [`TickAction`]. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FramePacer {
    /// The pump runs (as far as this pacer asked for).
    running: bool,
    /// Ticks in a row with nothing pending.
    idle_ticks: u8,
    /// The window shows nothing (occluded / minimized): no pump at all.
    suspended: bool,
}

impl FramePacer {
    /// Idle ticks before the pump stops.
    pub const IDLE_TICKS_BEFORE_STOP: u8 = 2;

    /// A stopped pump, nothing suspended.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            running: false,
            idle_ticks: 0,
            suspended: false,
        }
    }

    /// Whether the pump runs as far as the pacer knows.
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running
    }

    /// A frame is wanted. Every request restarts the idle count.
    pub fn on_request(&mut self) -> FrameRequest {
        self.idle_ticks = 0;
        if self.suspended {
            return FrameRequest::DeliverNow;
        }
        if self.running {
            FrameRequest::AlreadyRunning
        } else {
            self.running = true;
            FrameRequest::Start
        }
    }

    /// The pump ticked; `pending` says whether a frame is owed.
    ///
    /// A tick that arrives after the pump was stopped (the pump thread had
    /// already queued it) renders pending work but touches nothing else.
    pub fn on_tick(&mut self, pending: bool) -> TickAction {
        if pending {
            if self.running {
                self.idle_ticks = 0;
            }
            return TickAction::Render;
        }
        if !self.running {
            return TickAction::Idle;
        }
        self.idle_ticks = self.idle_ticks.saturating_add(1);
        if self.idle_ticks >= Self::IDLE_TICKS_BEFORE_STOP {
            self.running = false;
            self.idle_ticks = 0;
            TickAction::Stop
        } else {
            TickAction::Idle
        }
    }

    /// The window stopped showing (occluded / minimized). Returns whether
    /// the pump was running, so the caller stops it.
    pub fn suspend(&mut self) -> bool {
        let was_running = self.running;
        self.suspended = true;
        self.running = false;
        self.idle_ticks = 0;
        was_running
    }

    /// The window shows again. The pump stays off until the next request.
    pub fn resume(&mut self) {
        self.suspended = false;
    }

    /// The pump could not start, or was torn down and rebuilt stopped:
    /// forget that it ran.
    pub fn reset(&mut self) {
        self.running = false;
        self.idle_ticks = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The measured bug: an idle app's pump ran forever.
    #[test]
    fn a_pump_with_nothing_to_do_stops_after_the_idle_ticks() {
        let mut p = FramePacer::new();
        assert_eq!(
            p.on_request(),
            FrameRequest::Start,
            "a stopped pump is started"
        );
        assert!(p.is_running());
        assert_eq!(p.on_tick(true), TickAction::Render, "the requested frame");
        for n in 1..FramePacer::IDLE_TICKS_BEFORE_STOP {
            assert_eq!(
                p.on_tick(false),
                TickAction::Idle,
                "idle tick {n} keeps pumping"
            );
        }
        assert_eq!(
            p.on_tick(false),
            TickAction::Stop,
            "after {} idle ticks the pump stops",
            FramePacer::IDLE_TICKS_BEFORE_STOP
        );
        assert!(!p.is_running());
    }

    #[test]
    fn a_request_while_running_is_delivered_by_the_next_tick() {
        let mut p = FramePacer::new();
        assert_eq!(p.on_request(), FrameRequest::Start);
        assert_eq!(p.on_request(), FrameRequest::AlreadyRunning);
        assert_eq!(p.on_tick(true), TickAction::Render);
    }

    /// Requests that keep coming (an animation, a drag) keep the pump alive:
    /// every request resets the idle count.
    #[test]
    fn steady_requests_never_stop_the_pump() {
        let mut p = FramePacer::new();
        let _ = p.on_request();
        for _ in 0..100 {
            assert_eq!(p.on_tick(false), TickAction::Idle);
            assert_eq!(p.on_request(), FrameRequest::AlreadyRunning);
            assert_eq!(p.on_tick(true), TickAction::Render);
        }
        assert!(p.is_running());
    }

    #[test]
    fn a_stopped_pump_starts_again_on_the_next_request() {
        let mut p = FramePacer::new();
        let _ = p.on_request();
        while p.on_tick(false) != TickAction::Stop {}
        assert_eq!(p.on_request(), FrameRequest::Start);
        assert!(p.is_running());
    }

    /// The pump thread may already have queued a tick when the main thread
    /// stopped it. That tick must not stop anything twice nor count toward
    /// the next run's idle budget - and pending work it finds is still
    /// rendered.
    #[test]
    fn a_tick_that_arrives_after_the_stop_changes_nothing() {
        let mut p = FramePacer::new();
        let _ = p.on_request();
        while p.on_tick(false) != TickAction::Stop {}
        assert_eq!(p.on_tick(false), TickAction::Idle);
        assert!(!p.is_running());
        assert_eq!(p.on_tick(true), TickAction::Render);
        assert!(!p.is_running(), "a stray tick does not restart the pump");
    }

    #[test]
    fn a_window_that_shows_nothing_runs_no_pump() {
        let mut p = FramePacer::new();
        let _ = p.on_request();
        assert!(
            p.suspend(),
            "the running pump is reported, so the shell stops it"
        );
        assert!(!p.is_running());
        assert_eq!(
            p.on_request(),
            FrameRequest::DeliverNow,
            "a hidden window's frame is delivered without a pump"
        );
        assert!(!p.is_running());
        p.resume();
        assert_eq!(
            p.on_request(),
            FrameRequest::Start,
            "shown again, the pump returns"
        );
    }

    #[test]
    fn a_reset_pump_is_started_by_the_next_request() {
        let mut p = FramePacer::new();
        let _ = p.on_request();
        p.reset();
        assert!(!p.is_running());
        assert_eq!(p.on_request(), FrameRequest::Start);
    }
}
