//! Timer callback information and utilities for azul-layout
//!
//! This module provides the TimerCallbackInfo structure which gives timer callbacks
//! access to layout information, window state, and DOM query capabilities.

use core::ffi::c_void;

use azul_core::{
    callbacks::{DomNodeId, OptionDomNodeId, Update},
    task::{Instant, TerminateTimer},
    window::LogicalSize,
};

use crate::{callbacks::CallbackInfo, window::LayoutWindow};

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
    pub fn get_attached_node_position(&self) -> Option<azul_core::window::LogicalPosition> {
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

    if let OptionDuration::Some(interval) = self.interval {
        let last_run = match self.last_run.as_ref() {
            Some(s) => s.clone(),
            None => self.created.add_optional_duration(self.delay.as_ref()),
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

    let run_count = self.run_count;
    let is_about_to_finish = self.is_about_to_finish(&instant_now);
    let mut timer_callback_info = TimerCallbackInfo {
        callback_info,
        node_id: self.node_id,
        frame_start,
        call_count: run_count,
        is_about_to_finish,
        _abi_ref: core::ptr::null(),
        _abi_mut: core::ptr::null_mut(),
    };
    let mut res = (self.callback.cb)(&mut self.data, &mut timer_callback_info);

    // Check if the timers timeout is reached
    if is_about_to_finish {
        res.should_terminate = TerminateTimer::Terminate;
    }

    self.last_run = OptionInstant::Some(instant_now);
    self.run_count += 1;

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
