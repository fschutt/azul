//! Timer callback information and utilities for azul-layout
//!
//! This module provides Timer, TimerCallbackInfo and related types for
//! managing timers that run on the main UI thread.

use alloc::boxed::Box;
use core::ffi::c_void;

use azul_core::{
    callbacks::{DomNodeId, OptionDomNodeId, RefAny, Update},
    task::{Duration, GetSystemTimeCallback, Instant, OptionDuration, OptionInstant, TerminateTimer, TimerId},
    window::LogicalSize,
};

use crate::callbacks::CallbackInfo;

/// Information passed to timer callbacks
///
/// This structure provides timer callbacks with:
/// - Access to the CallbackInfo for general window/DOM queries
/// - Timer-specific metadata (node_id, call_count, etc.)
/// - Layout information for the attached node (if any)
#[repr(C)]
pub struct TimerCallbackInfo {
    /// General callback info for window/DOM queries
    pub callback_info: CallbackInfo,
    /// If the timer is attached to a DOM node, this will contain the node ID
    pub node_id: OptionDomNodeId,
    /// Time when the frame was started rendering
    pub frame_start: Instant,
    /// How many times this callback has been called
    pub call_count: usize,
    /// Set to true ONCE on the LAST invocation of the timer (if the timer has a timeout set)
    /// This is useful to rebuild the DOM once the timer (usually an animation) has finished.
    pub is_about_to_finish: bool,
    /// Extension for future ABI stability (referenced data)
    pub(crate) _abi_ref: *const c_void,
    /// Extension for future ABI stability (mutable data)
    pub(crate) _abi_mut: *mut c_void,
}

impl Clone for TimerCallbackInfo {
    fn clone(&self) -> Self {
        Self {
            callback_info: self.callback_info.clone(),
            node_id: self.node_id,
            frame_start: self.frame_start.clone(),
            call_count: self.call_count,
            is_about_to_finish: self.is_about_to_finish,
            _abi_ref: self._abi_ref,
            _abi_mut: self._abi_mut,
        }
    }
}

impl TimerCallbackInfo {
    /// Create a new TimerCallbackInfo
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        callback_info: CallbackInfo,
        node_id: OptionDomNodeId,
        frame_start: Instant,
        call_count: usize,
        is_about_to_finish: bool,
    ) -> Self {
        Self {
            callback_info,
            node_id,
            frame_start,
            call_count,
            is_about_to_finish,
            _abi_ref: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        }
    }

    /// Get the size of the node this timer is attached to (if any)
    ///
    /// This queries the LayoutWindow for the computed layout of the timer's node.
    /// Returns None if the timer is not attached to a node or if layout data is unavailable.
    pub fn get_attached_node_size(&self) -> Option<LogicalSize> {
        let node_id = self.node_id.into_option()?;
        self.callback_info.get_node_size(node_id)
    }

    /// Get the computed position of the node this timer is attached to (if any)
    pub fn get_attached_node_position(&self) -> Option<azul_core::geom::LogicalPosition> {
        let node_id = self.node_id.into_option()?;
        self.callback_info.get_node_position(node_id)
    }

    /// Access the general callback info
    pub fn get_callback_info(&self) -> &CallbackInfo {
        &self.callback_info
    }

    /// Access the general callback info mutably
    pub fn get_callback_info_mut(&mut self) -> &mut CallbackInfo {
        &mut self.callback_info
    }
}

/// Callback type for timers
pub type TimerCallbackType = extern "C" fn(
    /* timer internal data */ &mut RefAny,
    &mut TimerCallbackInfo,
) -> TimerCallbackReturn;

/// Callback that runs on every frame on the main thread - can modify the app data model
#[repr(C)]
pub struct TimerCallback {
    pub cb: TimerCallbackType,
}

impl TimerCallback {
    pub fn new(cb: TimerCallbackType) -> Self {
        Self { cb }
    }
}

impl core::fmt::Debug for TimerCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "TimerCallback {{ cb: {:p} }}", self.cb as *const ())
    }
}

impl Clone for TimerCallback {
    fn clone(&self) -> Self {
        Self { cb: self.cb }
    }
}

impl PartialEq for TimerCallback {
    fn eq(&self, other: &Self) -> bool {
        self.cb as usize == other.cb as usize
    }
}

impl Eq for TimerCallback {}

impl PartialOrd for TimerCallback {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        (self.cb as usize).partial_cmp(&(other.cb as usize))
    }
}

impl Ord for TimerCallback {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.cb as usize).cmp(&(other.cb as usize))
    }
}

impl core::hash::Hash for TimerCallback {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (self.cb as usize).hash(state);
    }
}

/// A `Timer` is a function that runs on every frame or at intervals.
///
/// Timers are useful for animations, polling, or any visual updates that need
/// to run frequently but aren't heavy enough to warrant a separate thread.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Timer {
    /// Data that is internal to the timer
    pub data: RefAny,
    /// Optional node that the timer is attached to - timers attached to a DOM node
    /// will be automatically stopped when the UI is recreated.
    pub node_id: OptionDomNodeId,
    /// Stores when the timer was created (usually acquired by `Instant::now()`)
    pub created: Instant,
    /// When the timer was last called (`None` only when the timer hasn't been called yet).
    pub last_run: OptionInstant,
    /// How many times the callback was run
    pub run_count: usize,
    /// If the timer shouldn't start instantly, but rather be delayed by a certain timeframe
    pub delay: OptionDuration,
    /// How frequently the timer should run
    pub interval: OptionDuration,
    /// When to stop the timer
    pub timeout: OptionDuration,
    /// Callback to be called for this timer
    pub callback: TimerCallback,
}

impl Timer {
    /// Create a new timer
    pub fn new(
        data: RefAny,
        callback: TimerCallbackType,
        get_system_time_fn: GetSystemTimeCallback,
    ) -> Self {
        Timer {
            data,
            node_id: None.into(),
            created: (get_system_time_fn.cb)(),
            run_count: 0,
            last_run: OptionInstant::None,
            delay: OptionDuration::None,
            interval: OptionDuration::None,
            timeout: OptionDuration::None,
            callback: TimerCallback { cb: callback },
        }
    }

    pub fn tick_millis(&self) -> u64 {
        match self.interval.as_ref() {
            Some(Duration::System(s)) => s.millis(),
            Some(Duration::Tick(s)) => s.tick_diff,
            None => 10, // ms
        }
    }

    /// Returns true ONCE on the LAST invocation of the timer
    pub fn is_about_to_finish(&self, instant_now: &Instant) -> bool {
        let mut finish = false;
        if let OptionDuration::Some(timeout) = self.timeout {
            finish = instant_now
                .duration_since(&self.created)
                .greater_than(&timeout);
        }
        finish
    }

    /// Returns when the timer needs to run again
    pub fn instant_of_next_run(&self) -> Instant {
        let last_run = match self.last_run.as_ref() {
            Some(s) => s,
            None => &self.created,
        };

        last_run
            .clone()
            .add_optional_duration(self.delay.as_ref())
            .add_optional_duration(self.interval.as_ref())
    }

    /// Delays the timer to not start immediately
    #[inline]
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = OptionDuration::Some(delay);
        self
    }

    /// Sets the interval for the timer
    #[inline]
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = OptionDuration::Some(interval);
        self
    }

    /// Sets a timeout for the timer
    #[inline]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = OptionDuration::Some(timeout);
        self
    }
}

impl Default for Timer {
    fn default() -> Self {
        extern "C" fn default_callback(_: &mut RefAny, _: &mut TimerCallbackInfo) -> TimerCallbackReturn {
            TimerCallbackReturn::terminate_unchanged()
        }
        
        extern "C" fn default_time() -> Instant {
            Instant::System(azul_core::task::SystemTime { secs: 0, nanos: 0 })
        }
        
        Timer::new(
            RefAny::new(()),
            default_callback,
            GetSystemTimeCallback { cb: default_time },
        )
    }
}

/// Return value from a timer callback
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TimerCallbackReturn {
    /// Whether the UI should be updated after this timer callback
    pub should_update: Update,
    /// Whether the timer should be terminated after this invocation
    pub should_terminate: TerminateTimer,
}

impl TimerCallbackReturn {
    /// Create a new TimerCallbackReturn
    pub fn new(should_update: Update, should_terminate: TerminateTimer) -> Self {
        Self {
            should_update,
            should_terminate,
        }
    }

    /// Continue running the timer without updating the UI
    pub fn continue_unchanged() -> Self {
        Self {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Continue,
        }
    }

    /// Continue running the timer and update the UI
    pub fn continue_and_update() -> Self {
        Self {
            should_update: Update::RefreshDom,
            should_terminate: TerminateTimer::Continue,
        }
    }

    /// Terminate the timer without updating the UI
    pub fn terminate_unchanged() -> Self {
        Self {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Terminate,
        }
    }

    /// Terminate the timer and update the UI
    pub fn terminate_and_update() -> Self {
        Self {
            should_update: Update::RefreshDom,
            should_terminate: TerminateTimer::Terminate,
        }
    }
}

/// Invokes the timer if the timer should run. Otherwise returns
/// `Update::DoNothing`
pub fn invoke_timer(
    timer: &mut Timer,
    callback_info: CallbackInfo,
    frame_start: Instant,
    get_system_time_fn: GetSystemTimeCallback,
) -> TimerCallbackReturn {
    let instant_now = (get_system_time_fn.cb)();

    if let OptionDuration::Some(interval) = timer.interval {
        let last_run = match timer.last_run.as_ref() {
            Some(s) => s.clone(),
            None => timer.created.add_optional_duration(timer.delay.as_ref()),
        };

        if instant_now
            .duration_since(&last_run)
            .smaller_than(&interval)
        {
            return TimerCallbackReturn {
                should_update: Update::DoNothing,
                should_terminate: TerminateTimer::Continue,
            };
        }
    }

    let run_count = timer.run_count;
    let is_about_to_finish = timer.is_about_to_finish(&instant_now);
    let mut timer_callback_info = TimerCallbackInfo {
        callback_info,
        node_id: timer.node_id,
        frame_start,
        call_count: run_count,
        is_about_to_finish,
        _abi_ref: core::ptr::null(),
        _abi_mut: core::ptr::null_mut(),
    };
    let mut res = (timer.callback.cb)(&mut timer.data, &mut timer_callback_info);

    // Check if the timers timeout is reached
    if is_about_to_finish {
        res.should_terminate = TerminateTimer::Terminate;
    }

    timer.last_run = OptionInstant::Some(instant_now);
    timer.run_count += 1;

    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer_callback_return_constructors() {
        let cont = TimerCallbackReturn::continue_unchanged();
        assert_eq!(cont.should_update, Update::DoNothing);
        assert_eq!(cont.should_terminate, TerminateTimer::Continue);

        let term = TimerCallbackReturn::terminate_and_update();
        assert_eq!(term.should_update, Update::RefreshDom);
        assert_eq!(term.should_terminate, TerminateTimer::Terminate);
    }
}
