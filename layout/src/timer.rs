//! Timer callback information and utilities for azul-layout
//!
//! This module provides Timer, TimerCallbackInfo and related types for
//! managing timers that run on the main UI thread.

use core::ffi::c_void;

use azul_core::{
    callbacks::{TimerCallbackReturn, Update},
    dom::{DomId, OptionDomNodeId},
    geom::{LogicalPosition, LogicalSize, OptionLogicalPosition, OptionCursorNodePosition, CursorNodePosition},
    id::NodeId,
    menu::Menu,
    refany::{OptionRefAny, RefAny},
    resources::ImageRef,
    task::{
        Duration, GetSystemTimeCallback, Instant, OptionDuration, OptionInstant, TerminateTimer,
        ThreadId, TimerId,
    },
    window::{KeyboardState, MouseState, WindowFlags},
};

use azul_css::AzString;

use crate::{
    callbacks::CallbackInfo,
    thread::Thread,
    window_state::{FullWindowState, WindowCreateOptions},
};

/// Callback type for timers
pub type TimerCallbackType = extern "C" fn(
    /* timer internal refany */ RefAny,
    TimerCallbackInfo,
) -> TimerCallbackReturn;

/// Callback that runs on every frame on the main thread
#[repr(C)]
pub struct TimerCallback {
    pub cb: TimerCallbackType,
    /// For FFI: stores the foreign callable (e.g., PyFunction)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

impl TimerCallback {
    pub fn create(cb: TimerCallbackType) -> Self {
        Self {
            cb,
            ctx: OptionRefAny::None,
        }
    }
}

impl core::fmt::Debug for TimerCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "TimerCallback {{ cb: {:p} }}", self.cb as *const ())
    }
}

impl Clone for TimerCallback {
    fn clone(&self) -> Self {
        Self {
            cb: self.cb,
            ctx: self.ctx.clone(),
        }
    }
}

impl From<TimerCallbackType> for TimerCallback {
    fn from(cb: TimerCallbackType) -> Self {
        Self {
            cb,
            ctx: OptionRefAny::None,
        }
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
    pub refany: RefAny,
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
    pub fn create<C: Into<TimerCallback>>(
        refany: RefAny,
        callback: C,
        get_system_time_fn: GetSystemTimeCallback,
    ) -> Self {
        Timer {
            refany,
            node_id: None.into(),
            created: (get_system_time_fn.cb)(),
            run_count: 0,
            last_run: OptionInstant::None,
            delay: OptionDuration::None,
            interval: OptionDuration::None,
            timeout: OptionDuration::None,
            callback: callback.into(),
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
    ///
    /// Returns `DoNothing` + `Continue` if the timer is not ready to run yet
    /// (delay not elapsed for first run, or interval not elapsed for subsequent runs).
    pub fn invoke(
        &mut self,
        callback_info: &CallbackInfo,
        get_system_time_fn: &GetSystemTimeCallback,
    ) -> TimerCallbackReturn {
        let now = (get_system_time_fn.cb)();

        // Check if timer should run based on last_run, delay, and interval
        match self.last_run.as_ref() {
            Some(last_run) => {
                // Timer has run before - check interval
                if let OptionDuration::Some(interval) = self.interval {
                    if now.duration_since(last_run).smaller_than(&interval) {
                        return TimerCallbackReturn {
                            should_update: Update::DoNothing,
                            should_terminate: TerminateTimer::Continue,
                        };
                    }
                }
            }
            None => {
                // Timer has never run - check delay (first run)
                if let OptionDuration::Some(delay) = self.delay {
                    if now.duration_since(&self.created).smaller_than(&delay) {
                        return TimerCallbackReturn {
                            should_update: Update::DoNothing,
                            should_terminate: TerminateTimer::Continue,
                        };
                    }
                }
            }
        }

        let is_about_to_finish = self.is_about_to_finish(&now);

        // Create a new TimerCallbackInfo wrapping the callback_info
        // CallbackInfo is Copy, so we can just copy it directly
        let mut timer_callback_info = TimerCallbackInfo {
            callback_info: *callback_info,
            node_id: self.node_id,
            frame_start: now.clone(),
            call_count: self.run_count,
            is_about_to_finish,
            _abi_ref: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        };

        let mut result = (self.callback.cb)(self.refany.clone(), timer_callback_info);

        if is_about_to_finish {
            result.should_terminate = TerminateTimer::Terminate;
        }

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

        Timer::create(
            RefAny::new(()),
            default_callback as TimerCallbackType,
            GetSystemTimeCallback { cb: default_time },
        )
    }
}

/// Information passed to timer callbacks
///
/// This wraps `CallbackInfo` and adds timer-specific fields like call_count and frame_start.
/// Through `Deref<Target = CallbackInfo>`, all methods from `CallbackInfo` are available,
/// including the transactional `push_change()` API.
#[derive(Clone)]
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
    pub fn create(
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

    // ==================== Delegated CallbackInfo methods ====================
    // These methods delegate to the inner callback_info to provide the same API
    // as CallbackInfo without using Deref (which causes issues with FFI codegen)

    /// Get the callable for FFI language bindings (Python, etc.)
    pub fn get_ctx(&self) -> OptionRefAny {
        self.callback_info.get_ctx()
    }

    /// Add a timer to this window (applied after callback returns)
    pub fn add_timer(&mut self, timer_id: TimerId, timer: Timer) {
        self.callback_info.add_timer(timer_id, timer);
    }

    /// Remove a timer from this window (applied after callback returns)
    pub fn remove_timer(&mut self, timer_id: TimerId) {
        self.callback_info.remove_timer(timer_id);
    }

    /// Add a thread to this window (applied after callback returns)
    pub fn add_thread(&mut self, thread_id: ThreadId, thread: Thread) {
        self.callback_info.add_thread(thread_id, thread);
    }

    /// Remove a thread from this window (applied after callback returns)
    pub fn remove_thread(&mut self, thread_id: ThreadId) {
        self.callback_info.remove_thread(thread_id);
    }

    /// Stop event propagation (applied after callback returns)
    pub fn stop_propagation(&mut self) {
        self.callback_info.stop_propagation();
    }

    /// Create a new window (applied after callback returns)
    pub fn create_window(&mut self, options: WindowCreateOptions) {
        self.callback_info.create_window(options);
    }

    /// Close the current window (applied after callback returns)
    pub fn close_window(&mut self) {
        self.callback_info.close_window();
    }

    /// Modify the window state (applied after callback returns)
    pub fn modify_window_state(&mut self, state: FullWindowState) {
        self.callback_info.modify_window_state(state);
    }

    /// Add an image to the image cache (applied after callback returns)
    pub fn add_image_to_cache(&mut self, id: AzString, image: ImageRef) {
        self.callback_info.add_image_to_cache(id, image);
    }

    /// Remove an image from the image cache (applied after callback returns)
    pub fn remove_image_from_cache(&mut self, id: AzString) {
        self.callback_info.remove_image_from_cache(id);
    }

    /// Re-render ALL image callbacks across all DOMs (applied after callback returns)
    ///
    /// This is the most efficient way to update animated GL textures from a timer.
    /// Triggers only texture re-rendering - no DOM rebuild or display list resubmission.
    pub fn update_all_image_callbacks(&mut self) {
        self.callback_info.update_all_image_callbacks();
    }

    /// Trigger re-rendering of a VirtualizedView (applied after callback returns)
    pub fn trigger_virtualized_view_rerender(&mut self, dom_id: DomId, node_id: NodeId) {
        self.callback_info.trigger_virtualized_view_rerender(dom_id, node_id);
    }

    /// Reload system fonts (applied after callback returns)
    pub fn reload_system_fonts(&mut self) {
        self.callback_info.reload_system_fonts();
    }

    /// Prevent the default action
    pub fn prevent_default(&mut self) {
        self.callback_info.prevent_default();
    }

    /// Open a menu
    pub fn open_menu(&mut self, menu: Menu) {
        self.callback_info.open_menu(menu);
    }

    /// Open a menu at a specific position
    pub fn open_menu_at(&mut self, menu: Menu, position: LogicalPosition) {
        self.callback_info.open_menu_at(menu, position);
    }

    /// Show a tooltip at the current cursor position
    pub fn show_tooltip(&mut self, text: AzString) {
        self.callback_info.show_tooltip(text);
    }

    /// Show a tooltip at a specific position
    pub fn show_tooltip_at(&mut self, text: AzString, position: LogicalPosition) {
        self.callback_info.show_tooltip_at(text, position);
    }

    /// Hide the currently displayed tooltip
    pub fn hide_tooltip(&mut self) {
        self.callback_info.hide_tooltip();
    }

    /// Open a menu positioned relative to the currently hit node
    pub fn open_menu_for_hit_node(&mut self, menu: Menu) -> bool {
        self.callback_info.open_menu_for_hit_node(menu)
    }

    /// Get current window flags
    pub fn get_current_window_flags(&self) -> WindowFlags {
        self.callback_info.get_current_window_flags()
    }

    /// Get current keyboard state
    pub fn get_current_keyboard_state(&self) -> KeyboardState {
        self.callback_info.get_current_keyboard_state()
    }

    /// Get current mouse state
    pub fn get_current_mouse_state(&self) -> MouseState {
        self.callback_info.get_current_mouse_state()
    }

    /// Get the cursor position relative to the hit node
    pub fn get_cursor_relative_to_node(&self) -> azul_core::geom::OptionCursorNodePosition {
        self.callback_info.get_cursor_relative_to_node()
    }

    /// Get the cursor position relative to the viewport
    pub fn get_cursor_relative_to_viewport(&self) -> OptionLogicalPosition {
        self.callback_info.get_cursor_relative_to_viewport()
    }

    /// Get the current cursor position
    pub fn get_cursor_position(&self) -> Option<LogicalPosition> {
        self.callback_info.get_cursor_position()
    }

    /// Get the current time (when the timer callback started)
    pub fn get_current_time(&self) -> Instant {
        self.frame_start.clone()
    }

    /// Check if the DOM is focused
    pub fn is_dom_focused(&self) -> bool {
        // TimerCallbackInfo doesn't have direct focus info
        true // Timers run regardless of focus
    }

    /// Check if pen is in contact
    pub fn is_pen_in_contact(&self) -> bool {
        false // Not available in timer context
    }

    /// Check if pen eraser is active
    pub fn is_pen_eraser(&self) -> bool {
        false // Not available in timer context
    }

    /// Check if pen barrel button is pressed
    pub fn is_pen_barrel_button_pressed(&self) -> bool {
        false // Not available in timer context
    }

    /// Check if dragging is active
    pub fn is_dragging(&self) -> bool {
        self.callback_info.get_current_mouse_state().left_down
    }

    /// Check if drag is active
    pub fn is_drag_active(&self) -> bool {
        self.callback_info.get_current_mouse_state().left_down
    }

    /// Check if node drag is active
    pub fn is_node_drag_active(&self) -> bool {
        self.callback_info.get_current_mouse_state().left_down
    }

    /// Check if file drag is active
    pub fn is_file_drag_active(&self) -> bool {
        false // Timers don't track file drags
    }

    /// Check if there's sufficient history for gestures
    pub fn has_sufficient_history_for_gestures(&self) -> bool {
        false // Timers don't track gesture history
    }

    // ==================== Scroll Management (timer architecture) ====================

    /// Get a read-only snapshot of a scroll node's bounds and position.
    ///
    /// Timer callbacks use this to read current scroll state for physics calculation.
    pub fn get_scroll_node_info(
        &self,
        dom_id: azul_core::dom::DomId,
        node_id: azul_core::id::NodeId,
    ) -> Option<crate::managers::scroll_state::ScrollNodeInfo> {
        self.callback_info.get_scroll_node_info(dom_id, node_id)
    }

    /// Find the closest scrollable ancestor of a node.
    ///
    /// Used by auto-scroll timer to find which container to scroll when
    /// the user drags beyond the container edge.
    pub fn find_scroll_parent(
        &self,
        dom_id: azul_core::dom::DomId,
        node_id: azul_core::id::NodeId,
    ) -> Option<azul_core::id::NodeId> {
        self.callback_info.find_scroll_parent(dom_id, node_id)
    }

    /// Get the scroll input queue for consuming pending scroll inputs.
    ///
    /// The physics timer calls `take_all()` each tick to drain inputs
    /// recorded by platform event handlers.
    #[cfg(feature = "std")]
    pub fn get_scroll_input_queue(
        &self,
    ) -> crate::managers::scroll_state::ScrollInputQueue {
        self.callback_info.get_scroll_input_queue()
    }

    /// Scroll a node to a specific position (via transactional CallbackChange).
    ///
    /// This is the primary way for timer callbacks to update scroll positions.
    /// The change is applied after the callback returns.
    pub fn scroll_to(
        &mut self,
        dom_id: azul_core::dom::DomId,
        node_id: azul_core::styled_dom::NodeHierarchyItemId,
        position: azul_core::geom::LogicalPosition,
    ) {
        self.callback_info.scroll_to(dom_id, node_id, position);
    }

    // Cursor blink timer methods
    
    /// Set cursor visibility state (for cursor blink timer)
    pub fn set_cursor_visibility(&mut self, visible: bool) {
        self.callback_info.set_cursor_visibility(visible);
    }
    
    /// Toggle cursor visibility (for cursor blink timer)
    ///
    /// This is a shortcut that reads the current visibility state,
    /// toggles it, and queues the change. Used by the cursor blink timer.
    pub fn set_cursor_visibility_toggle(&mut self) {
        // We can't read the current state from here, so we queue a special toggle action
        // The actual toggle will be handled in apply_user_change using CursorManager.toggle_visibility()
        use crate::callbacks::CallbackChange;
        // Use SetCursorVisibility with a special sentinel value to indicate toggle
        // Actually, let's just add a separate toggle method or use the existing ones smartly
        
        // For simplicity, we'll queue both a reset_cursor_blink (to handle idle detection)
        // and let apply_user_change handle the visibility toggle based on should_blink()
        self.callback_info.push_change(CallbackChange::SetCursorVisibility { visible: true });
    }
    
    /// Reset cursor blink state on user input
    pub fn reset_cursor_blink(&mut self) {
        self.callback_info.reset_cursor_blink();
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

    // Check if timer should run based on last_run, delay, and interval
    match timer.last_run.as_ref() {
        Some(last_run) => {
            // Timer has run before - check interval
            if let OptionDuration::Some(interval) = timer.interval {
                if instant_now.duration_since(last_run).smaller_than(&interval) {
                    return TimerCallbackReturn {
                        should_update: Update::DoNothing,
                        should_terminate: TerminateTimer::Continue,
                    };
                }
            }
        }
        None => {
            // Timer has never run - check delay (first run)
            if let OptionDuration::Some(delay) = timer.delay {
                if instant_now
                    .duration_since(&timer.created)
                    .smaller_than(&delay)
                {
                    return TimerCallbackReturn {
                        should_update: Update::DoNothing,
                        should_terminate: TerminateTimer::Continue,
                    };
                }
            }
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
    let mut res = (timer.callback.cb)(timer.refany.clone(), timer_callback_info);

    if is_about_to_finish {
        res.should_terminate = TerminateTimer::Terminate;
    }

    timer.last_run = OptionInstant::Some(instant_now);
    timer.run_count += 1;

    res
}

/// Optional Timer type for API compatibility
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum OptionTimer {
    None,
    Some(Timer),
}

impl From<Option<Timer>> for OptionTimer {
    fn from(o: Option<Timer>) -> Self {
        match o {
            None => OptionTimer::None,
            Some(t) => OptionTimer::Some(t),
        }
    }
}

impl OptionTimer {
    pub fn into_option(self) -> Option<Timer> {
        match self {
            OptionTimer::None => None,
            OptionTimer::Some(t) => Some(t),
        }
    }
}
