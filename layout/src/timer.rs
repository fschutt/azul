//! Timer callback information and utilities for azul-layout
//!
//! This module provides Timer, TimerCallbackInfo and related types for
//! managing timers that run on the main UI thread.

use core::ffi::c_void;

use azul_core::{
    callbacks::Update,
    dom::OptionDomNodeId,
    geom::LogicalSize,
    refany::RefAny,
    task::{
        Duration, GetSystemTimeCallback, Instant, OptionDuration, OptionInstant, TerminateTimer,
    },
};

use crate::callbacks::CallbackInfo;

/// Callback type for timers
pub type TimerCallbackType =
    extern "C" fn(/* timer internal data */ RefAny, TimerCallbackInfo) -> TimerCallbackReturn;

/// Callback that runs on every frame on the main thread
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Timer {
    pub data: RefAny,
    pub node_id: OptionDomNodeId,
    pub created: Instant,
    pub last_run: OptionInstant,
    pub run_count: usize,
    pub delay: OptionDuration,
    pub interval: OptionDuration,
    pub timeout: OptionDuration,
    pub callback: TimerCallback,
}

impl Timer {
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
            None => 10,
        }
    }

    pub fn is_about_to_finish(&self, instant_now: &Instant) -> bool {
        let mut finish = false;
        if let OptionDuration::Some(timeout) = self.timeout {
            finish = instant_now
                .duration_since(&self.created)
                .greater_than(&timeout);
        }
        finish
    }

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

    #[inline]
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = OptionDuration::Some(delay);
        self
    }

    #[inline]
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = OptionDuration::Some(interval);
        self
    }

    #[inline]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = OptionDuration::Some(timeout);
        self
    }

    /// Invoke the timer callback and update internal state
    pub fn invoke(
        &mut self,
        callback_info: &CallbackInfo,
        get_system_time_fn: &GetSystemTimeCallback,
    ) -> TimerCallbackReturn {
        let now = (get_system_time_fn.cb)();
        let is_about_to_finish = self.is_about_to_finish(&now);

        // Create a new TimerCallbackInfo wrapping the callback_info
        // We need to use unsafe to create a copy of the pointer-based CallbackInfo
        let timer_callback_info_inner =
            unsafe { core::ptr::read(callback_info as *const CallbackInfo) };

        let mut timer_callback_info = TimerCallbackInfo {
            callback_info: timer_callback_info_inner,
            node_id: self.node_id,
            frame_start: now.clone(),
            call_count: self.run_count,
            is_about_to_finish,
            _abi_ref: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        };

        let result = (self.callback.cb)(self.data.clone(), timer_callback_info);

        self.run_count += 1;
        self.last_run = OptionInstant::Some(now);

        result
    }
}

impl Default for Timer {
    fn default() -> Self {
        extern "C" fn default_callback(_: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
            TimerCallbackReturn::terminate_unchanged()
        }

        extern "C" fn default_time() -> Instant {
            Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 })
        }

        Timer::new(
            RefAny::new(()),
            default_callback,
            GetSystemTimeCallback { cb: default_time },
        )
    }
}

/// Information passed to timer callbacks
///
/// This wraps `CallbackInfo` and adds timer-specific fields like call_count and frame_start.
/// Through `Deref<Target = CallbackInfo>`, all methods from `CallbackInfo` are available,
/// including the transactional `push_change()` API.
///
/// # Example
/// ```ignore
/// extern "C" fn animate_timer(
///     data: &mut RefAny,
///     info: &mut TimerCallbackInfo,
/// ) -> TimerCallbackReturn {
///     // Timer-specific fields
///     let call_count = info.call_count;
///     let frame_start = info.frame_start;
///     
///     // All CallbackInfo methods are available via Deref:
///     info.update_image_callback(dom_id, node_id);  // ← from CallbackInfo
///     info.add_timer(timer_id, timer);              // ← from CallbackInfo
///     
///     TimerCallbackReturn::continue_unchanged()
/// }
/// ```
#[repr(C)]
pub struct TimerCallbackInfo {
    pub callback_info: CallbackInfo,
    pub node_id: OptionDomNodeId,
    pub frame_start: Instant,
    pub call_count: usize,
    pub is_about_to_finish: bool,
    pub _abi_ref: *const c_void,
    pub _abi_mut: *mut c_void,
}

impl TimerCallbackInfo {
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

    pub fn get_attached_node_size(&self) -> Option<LogicalSize> {
        let node_id = self.node_id.into_option()?;
        self.callback_info.get_node_size(node_id)
    }

    pub fn get_attached_node_position(&self) -> Option<azul_core::geom::LogicalPosition> {
        let node_id = self.node_id.into_option()?;
        self.callback_info.get_node_position(node_id)
    }

    pub fn get_callback_info(&self) -> &CallbackInfo {
        &self.callback_info
    }

    pub fn get_callback_info_mut(&mut self) -> &mut CallbackInfo {
        &mut self.callback_info
    }
}

// Implement Deref so all CallbackInfo methods are available
impl core::ops::Deref for TimerCallbackInfo {
    type Target = CallbackInfo;

    fn deref(&self) -> &Self::Target {
        &self.callback_info
    }
}

// Implement DerefMut so mutable CallbackInfo methods are available
impl core::ops::DerefMut for TimerCallbackInfo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.callback_info
    }
}

/// Return value from a timer callback
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TimerCallbackReturn {
    pub should_update: Update,
    pub should_terminate: TerminateTimer,
}

impl TimerCallbackReturn {
    pub fn new(should_update: Update, should_terminate: TerminateTimer) -> Self {
        Self {
            should_update,
            should_terminate,
        }
    }

    pub fn continue_unchanged() -> Self {
        Self {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Continue,
        }
    }

    pub fn continue_and_update() -> Self {
        Self {
            should_update: Update::RefreshDom,
            should_terminate: TerminateTimer::Continue,
        }
    }

    pub fn terminate_unchanged() -> Self {
        Self {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Terminate,
        }
    }

    pub fn terminate_and_update() -> Self {
        Self {
            should_update: Update::RefreshDom,
            should_terminate: TerminateTimer::Terminate,
        }
    }
}

/// Invokes the timer if it should run
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
    let mut res = (timer.callback.cb)(timer.data.clone(), timer_callback_info);

    if is_about_to_finish {
        res.should_terminate = TerminateTimer::Terminate;
    }

    timer.last_run = OptionInstant::Some(instant_now);
    timer.run_count += 1;

    res
}
