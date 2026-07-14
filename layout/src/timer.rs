//! Timer callback information and utilities for azul-layout
//!
//! This module provides Timer, TimerCallbackInfo and related types for
//! managing timers that run on the main UI thread.

use core::ffi::c_void;

use azul_core::{
    callbacks::{TimerCallbackReturn, Update},
    dom::{DomId, OptionDomNodeId},
    geom::{LogicalPosition, LogicalSize, OptionLogicalPosition},
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

/// Default timer tick interval in milliseconds when no interval is configured.
const DEFAULT_TIMER_TICK_MS: u64 = 10;

/// Callback type for timers
pub type TimerCallbackType = extern "C" fn(
    /* timer internal refany */ RefAny,
    TimerCallbackInfo,
) -> TimerCallbackReturn;

/// Callback that runs on every frame on the main thread
#[repr(C)]
pub struct TimerCallback {
    pub cb: TimerCallbackType,
    /// For FFI: stores the foreign callable (e.g., `PyFunction`)
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
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
        std::ptr::eq(self.cb as *const (), other.cb as *const ())
    }
}

impl Eq for TimerCallback {}

impl PartialOrd for TimerCallback {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        (self.cb as *const () as usize).partial_cmp(&(other.cb as *const () as usize))
    }
}

impl Ord for TimerCallback {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.cb as *const () as usize).cmp(&(other.cb as *const () as usize))
    }
}

impl core::hash::Hash for TimerCallback {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (self.cb as *const () as usize).hash(state);
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
        Self {
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

    #[must_use] pub const fn tick_millis(&self) -> u64 {
        match self.interval.as_ref() {
            Some(Duration::System(s)) => s.millis(),
            Some(Duration::Tick(s)) => s.tick_diff,
            None => DEFAULT_TIMER_TICK_MS,
        }
    }

    #[must_use] pub fn is_about_to_finish(&self, instant_now: &Instant) -> bool {
        match self.timeout {
            OptionDuration::Some(timeout) => {
                instant_now.duration_since(&self.created).greater_than(&timeout)
            }
            OptionDuration::None => false,
        }
    }

    #[must_use] pub fn instant_of_next_run(&self) -> Instant {
        let last_run = self.last_run.as_ref().map_or(&self.created, |s| s);

        last_run
            .clone()
            .add_optional_duration(self.delay.as_ref())
            .add_optional_duration(self.interval.as_ref())
    }

    #[inline]
    #[must_use] pub const fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = OptionDuration::Some(delay);
        self
    }

    #[inline]
    #[must_use] pub const fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = OptionDuration::Some(interval);
        self
    }

    #[inline]
    #[must_use] pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = OptionDuration::Some(timeout);
        self
    }

    /// Invoke the timer callback and update internal state.
    ///
    /// Returns a `TimerCallbackReturn` with `DoNothing` + `Continue` if the timer
    /// is not ready to run yet (delay not elapsed for first run, or interval not
    /// elapsed for subsequent runs). Forces `Terminate` when the timeout expires.
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

        const extern "C" fn default_time() -> Instant {
            Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 })
        }

        let cb: TimerCallbackType = default_callback;
        Self::create(
            RefAny::new(()),
            cb,
            GetSystemTimeCallback { cb: default_time },
        )
    }
}

/// Information passed to timer callbacks.
///
/// This wraps `CallbackInfo` and adds timer-specific fields like `call_count` and `frame_start`.
/// `CallbackInfo` methods are available via explicit delegation methods below.
#[derive(Debug, Clone)]
#[repr(C)]
#[allow(clippy::pub_underscore_fields)] // _abi_ref/_abi_mut: intentional FFI/api.json ABI-stability placeholder fields
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
    #[must_use] pub const fn create(
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

    #[must_use] pub fn get_attached_node_size(&self) -> Option<LogicalSize> {
        let node_id = self.node_id.into_option()?;
        self.callback_info.get_node_size(node_id)
    }

    #[must_use] pub fn get_attached_node_position(&self) -> Option<LogicalPosition> {
        let node_id = self.node_id.into_option()?;
        self.callback_info.get_node_position(node_id)
    }

    #[must_use] pub const fn get_callback_info(&self) -> &CallbackInfo {
        &self.callback_info
    }

    pub const fn get_callback_info_mut(&mut self) -> &mut CallbackInfo {
        &mut self.callback_info
    }

    // ==================== Delegated CallbackInfo methods ====================
    // These methods delegate to the inner callback_info to provide the same API
    // as CallbackInfo without using Deref (which causes issues with FFI codegen)

    /// Get the callable for FFI language bindings (Python, etc.)
    #[must_use] pub fn get_ctx(&self) -> OptionRefAny {
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

    /// Trigger re-rendering of a `VirtualView` (applied after callback returns)
    pub fn trigger_virtual_view_rerender(&mut self, dom_id: DomId, node_id: NodeId) {
        self.callback_info.trigger_virtual_view_rerender(dom_id, node_id);
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
    #[must_use] pub const fn get_current_window_flags(&self) -> WindowFlags {
        self.callback_info.get_current_window_flags()
    }

    /// Get current keyboard state
    #[must_use] pub fn get_current_keyboard_state(&self) -> KeyboardState {
        self.callback_info.get_current_keyboard_state()
    }

    /// Get current mouse state
    #[must_use] pub const fn get_current_mouse_state(&self) -> MouseState {
        self.callback_info.get_current_mouse_state()
    }

    /// Get the cursor position relative to the hit node
    #[must_use] pub const fn get_cursor_relative_to_node(&self) -> azul_core::geom::OptionCursorNodePosition {
        self.callback_info.get_cursor_relative_to_node()
    }

    /// Get the cursor position relative to the viewport
    #[must_use] pub const fn get_cursor_relative_to_viewport(&self) -> OptionLogicalPosition {
        self.callback_info.get_cursor_relative_to_viewport()
    }

    /// Get the current cursor position
    #[must_use] pub fn get_cursor_position(&self) -> Option<LogicalPosition> {
        self.callback_info.get_cursor_position()
    }

    /// Get the current time (when the timer callback started)
    #[must_use] pub fn get_current_time(&self) -> Instant {
        self.frame_start.clone()
    }

    /// Check if any node in a specific DOM is focused
    #[must_use] pub fn is_dom_focused(&self, dom_id: DomId) -> bool {
        self.callback_info.is_dom_focused(dom_id)
    }

    /// Check if pen is in contact
    #[must_use] pub fn is_pen_in_contact(&self) -> bool {
        self.callback_info.is_pen_in_contact()
    }

    /// Check if pen eraser is active
    #[must_use] pub fn is_pen_eraser(&self) -> bool {
        self.callback_info.is_pen_eraser()
    }

    /// Check if pen barrel button is pressed
    #[must_use] pub fn is_pen_barrel_button_pressed(&self) -> bool {
        self.callback_info.is_pen_barrel_button_pressed()
    }

    /// Check if dragging is active
    #[must_use] pub const fn is_dragging(&self) -> bool {
        self.callback_info.get_current_mouse_state().left_down
    }

    /// Check if drag is active
    #[must_use] pub const fn is_drag_active(&self) -> bool {
        self.callback_info.get_current_mouse_state().left_down
    }

    /// Check if node drag is active
    #[must_use] pub const fn is_node_drag_active(&self) -> bool {
        self.callback_info.get_current_mouse_state().left_down
    }

    /// Check if file drag is active
    #[must_use] pub fn is_file_drag_active(&self) -> bool {
        self.callback_info.is_file_drag_active()
    }

    /// Check if there's sufficient history for gestures
    #[must_use] pub fn has_sufficient_history_for_gestures(&self) -> bool {
        self.callback_info.has_sufficient_history_for_gestures()
    }

    // ==================== Scroll Management (timer architecture) ====================

    /// Get a read-only snapshot of a scroll node's bounds and position.
    ///
    /// Timer callbacks use this to read current scroll state for physics calculation.
    #[must_use] pub fn get_scroll_node_info(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<crate::managers::scroll_state::ScrollNodeInfo> {
        self.callback_info.get_scroll_node_info(dom_id, node_id)
    }

    /// Find the closest scrollable ancestor of a node.
    ///
    /// Used by auto-scroll timer to find which container to scroll when
    /// the user drags beyond the container edge.
    #[must_use] pub fn find_scroll_parent(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<NodeId> {
        self.callback_info.find_scroll_parent(dom_id, node_id)
    }

    /// Get the scroll input queue for consuming pending scroll inputs.
    ///
    /// The physics timer calls `take_all()` each tick to drain inputs
    /// recorded by platform event handlers.
    #[cfg(feature = "std")]
    #[must_use] pub fn get_scroll_input_queue(
        &self,
    ) -> crate::managers::scroll_state::ScrollInputQueue {
        self.callback_info.get_scroll_input_queue()
    }

    /// Scroll a node to a specific position (via transactional `CallbackChange`).
    ///
    /// This is the primary way for timer callbacks to update scroll positions.
    /// The change is applied after the callback returns.
    pub fn scroll_to(
        &mut self,
        dom_id: DomId,
        node_id: azul_core::styled_dom::NodeHierarchyItemId,
        position: LogicalPosition,
    ) {
        self.callback_info.scroll_to(dom_id, node_id, position);
    }

    /// Scroll to position without clamping (for rubber-banding/overscroll).
    pub fn scroll_to_unclamped(
        &mut self,
        dom_id: DomId,
        node_id: azul_core::styled_dom::NodeHierarchyItemId,
        position: LogicalPosition,
    ) {
        self.callback_info.scroll_to_unclamped(dom_id, node_id, position);
    }

    // Cursor blink timer methods
    
    /// Set cursor visibility state (for cursor blink timer)
    pub fn set_cursor_visibility(&mut self, visible: bool) {
        self.callback_info.set_cursor_visibility(visible);
    }
    
    /// Toggle cursor visibility (for cursor blink timer).
    pub fn set_cursor_visibility_toggle(&mut self) {
        use crate::callbacks::CallbackChange;
        self.callback_info.push_change(CallbackChange::ToggleCursorVisibility);
    }
    
    /// Reset cursor blink state on user input
    pub fn reset_cursor_blink(&mut self) {
        self.callback_info.reset_cursor_blink();
    }
}

/// Optional Timer type for API compatibility
#[derive(Debug, Clone)]
#[repr(C, u8)]
// FFI Option enum; boxing the Some variant would break the #[repr(C, u8)] C ABI / api.json.
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
#[allow(clippy::large_enum_variant)]
pub enum OptionTimer {
    None,
    Some(Timer),
}

impl From<Option<Timer>> for OptionTimer {
    fn from(o: Option<Timer>) -> Self {
        o.map_or_else(|| Self::None, Self::Some)
    }
}

impl OptionTimer {
    #[must_use] pub fn into_option(self) -> Option<Timer> {
        match self {
            Self::None => None,
            Self::Some(t) => Some(t),
        }
    }
}

#[cfg(all(test, feature = "std"))]
#[allow(
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::unreadable_literal,
    clippy::cognitive_complexity
)]
mod autotest_generated {
    use std::{
        collections::BTreeMap,
        sync::{
            atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
            Arc, Mutex, MutexGuard, PoisonError,
        },
    };

    use azul_core::{
        dom::DomNodeId,
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        menu::MenuItemVec,
        resources::{RawImageFormat, RendererResources},
        styled_dom::NodeHierarchyItemId,
        task::{SystemTick, SystemTickDiff, SystemTimeDiff, ThreadReceiver},
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::system::SystemStyle;
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        thread::{ThreadCallbackType, ThreadSender},
        window::LayoutWindow,
    };

    // ------------------------------------------------------------------
    // Time helpers
    // ------------------------------------------------------------------

    /// A tick-based `Instant` — the only kind constructible without a real clock,
    /// and the only kind whose arithmetic is fully deterministic.
    fn tick(t: u64) -> Instant {
        Instant::Tick(SystemTick::new(t))
    }

    /// A tick-based `Duration`.
    const fn tick_dur(d: u64) -> Duration {
        Duration::Tick(SystemTickDiff { tick_diff: d })
    }

    /// A wall-clock-based `Duration` (deliberately the *wrong kind* to pair with
    /// a `Tick` instant — several tests below pin the saturating behaviour of
    /// exactly that mismatch).
    const fn sys_dur_millis(ms: u64) -> Duration {
        Duration::System(SystemTimeDiff::from_millis(ms))
    }

    /// Extract the tick counter, asserting the instant really is tick-based.
    fn tick_of(i: &Instant) -> u64 {
        match i {
            Instant::Tick(t) => t.tick_counter,
            Instant::System(_) => panic!("expected a Tick instant, got a System one"),
        }
    }

    // ------------------------------------------------------------------
    // Fake clock + recording callback
    //
    // `GetSystemTimeCallbackType` is a bare `extern "C" fn()` with no context
    // pointer, so the fake clock has to live in statics. Every test that touches
    // them takes `clock_guard()` first, which serialises them against the rest of
    // the (parallel) test binary.
    // ------------------------------------------------------------------

    static CLOCK_LOCK: Mutex<()> = Mutex::new(());
    static FAKE_TICK: AtomicU64 = AtomicU64::new(0);
    static CB_INVOCATIONS: AtomicUsize = AtomicUsize::new(0);
    static CB_SEEN_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
    static CB_SEEN_FRAME_START: AtomicU64 = AtomicU64::new(0);
    static CB_SEEN_ABOUT_TO_FINISH: AtomicBool = AtomicBool::new(false);
    static CB_RETURN_TERMINATE: AtomicBool = AtomicBool::new(false);

    /// Serialises access to the fake-clock / recorder statics. Ignores poisoning:
    /// a `#[should_panic]`-free suite still panics on assertion failure, and a
    /// poisoned lock must not cascade into unrelated test failures.
    fn clock_guard() -> MutexGuard<'static, ()> {
        let guard = CLOCK_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        FAKE_TICK.store(0, Ordering::SeqCst);
        CB_INVOCATIONS.store(0, Ordering::SeqCst);
        CB_SEEN_CALL_COUNT.store(0, Ordering::SeqCst);
        CB_SEEN_FRAME_START.store(0, Ordering::SeqCst);
        CB_SEEN_ABOUT_TO_FINISH.store(false, Ordering::SeqCst);
        CB_RETURN_TERMINATE.store(false, Ordering::SeqCst);
        guard
    }

    fn set_now(t: u64) {
        FAKE_TICK.store(t, Ordering::SeqCst);
    }

    extern "C" fn fake_clock() -> Instant {
        Instant::Tick(SystemTick::new(FAKE_TICK.load(Ordering::SeqCst)))
    }

    fn fake_clock_cb() -> GetSystemTimeCallback {
        GetSystemTimeCallback { cb: fake_clock }
    }

    /// Records everything the timer machinery handed it, and returns whatever
    /// `CB_RETURN_TERMINATE` currently says.
    extern "C" fn recording_cb(_data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
        CB_INVOCATIONS.fetch_add(1, Ordering::SeqCst);
        CB_SEEN_CALL_COUNT.store(info.call_count, Ordering::SeqCst);
        CB_SEEN_ABOUT_TO_FINISH.store(info.is_about_to_finish, Ordering::SeqCst);
        if let Instant::Tick(t) = &info.frame_start {
            CB_SEEN_FRAME_START.store(t.tick_counter, Ordering::SeqCst);
        }
        TimerCallbackReturn {
            should_update: Update::DoNothing,
            should_terminate: if CB_RETURN_TERMINATE.load(Ordering::SeqCst) {
                TerminateTimer::Terminate
            } else {
                TerminateTimer::Continue
            },
        }
    }

    // Two callbacks with *different bodies* — identical bodies are legal prey for
    // identical-code folding, which would silently merge their addresses and make
    // the `TimerCallback` Eq/Ord/Hash tests below vacuous.
    extern "C" fn cb_alpha(_d: RefAny, _i: TimerCallbackInfo) -> TimerCallbackReturn {
        TimerCallbackReturn {
            should_update: Update::RefreshDom,
            should_terminate: TerminateTimer::Terminate,
        }
    }
    extern "C" fn cb_beta(_d: RefAny, _i: TimerCallbackInfo) -> TimerCallbackReturn {
        TimerCallbackReturn {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Continue,
        }
    }

    /// A timer created at tick `created`, driven by the fake clock.
    fn timer_at(created: u64, cb: TimerCallbackType) -> Timer {
        set_now(created);
        Timer::create(RefAny::new(0_usize), cb, fake_clock_cb())
    }

    // ------------------------------------------------------------------
    // CallbackInfo harness (mirrors the one in `scroll_timer.rs`)
    // ------------------------------------------------------------------

    struct Env<'a> {
        ref_data: &'a CallbackInfoRefData<'a>,
        changes: &'a Arc<Mutex<Vec<CallbackChange>>>,
    }

    impl Env<'_> {
        fn info(&self) -> CallbackInfo {
            self.info_with(OptionLogicalPosition::None, OptionLogicalPosition::None)
        }

        fn info_with(
            &self,
            cursor_relative_to_item: OptionLogicalPosition,
            cursor_in_viewport: OptionLogicalPosition,
        ) -> CallbackInfo {
            CallbackInfo::new(
                self.ref_data,
                self.changes,
                DomNodeId {
                    dom: DomId::ROOT_ID,
                    node: NodeHierarchyItemId::NONE,
                },
                cursor_relative_to_item,
                cursor_in_viewport,
            )
        }

        /// A `TimerCallbackInfo` with no attached node, frame_start = tick 0.
        fn timer_info(&self) -> TimerCallbackInfo {
            TimerCallbackInfo::create(self.info(), OptionDomNodeId::None, tick(0), 0, false)
        }

        fn take_changes(&self) -> Vec<CallbackChange> {
            self.changes
                .lock()
                .map(|mut c| core::mem::take(&mut *c))
                .unwrap_or_default()
        }

        /// Drain the log, asserting it holds exactly one change, and return it.
        fn take_one(&self) -> CallbackChange {
            let mut changes = self.take_changes();
            assert_eq!(changes.len(), 1, "expected exactly one change: {changes:?}");
            changes.remove(0)
        }
    }

    fn with_env<R>(f: impl FnOnce(&Env<'_>) -> R) -> R {
        with_env_cfg(false, OptionRefAny::None, f)
    }

    /// Builds a callback environment over an empty `LayoutWindow`. `left_down`
    /// drives the mouse state the drag predicates read; `ctx` is what `get_ctx`
    /// hands back to FFI bindings.
    fn with_env_cfg<R>(left_down: bool, ctx: OptionRefAny, f: impl FnOnce(&Env<'_>) -> R) -> R {
        let layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let mut current_window_state = FullWindowState::default();
        current_window_state.mouse_state.left_down = left_down;
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
        let env = Env {
            ref_data: &ref_data,
            changes: &changes,
        };
        f(&env)
    }

    fn empty_menu() -> Menu {
        Menu::create(MenuItemVec::from_const_slice(&[]))
    }

    // ==================================================================
    // Timer::create / Default — constructor invariants
    // ==================================================================

    #[test]
    fn timer_create_starts_completely_unarmed() {
        let _g = clock_guard();
        let t = timer_at(12_345, recording_cb as TimerCallbackType);

        assert_eq!(tick_of(&t.created), 12_345, "created must come from the clock");
        assert_eq!(t.run_count, 0);
        assert_eq!(t.last_run, OptionInstant::None);
        assert_eq!(t.delay, OptionDuration::None);
        assert_eq!(t.interval, OptionDuration::None);
        assert_eq!(t.timeout, OptionDuration::None);
        assert_eq!(t.node_id, OptionDomNodeId::None);
    }

    #[test]
    fn timer_create_at_max_tick_does_not_panic() {
        let _g = clock_guard();
        let t = timer_at(u64::MAX, recording_cb as TimerCallbackType);
        assert_eq!(tick_of(&t.created), u64::MAX);
        // Nothing is armed, so nothing can overflow off the end of time.
        assert!(!t.is_about_to_finish(&tick(u64::MAX)));
        assert_eq!(tick_of(&t.instant_of_next_run()), u64::MAX);
    }

    #[test]
    fn timer_default_is_a_zero_tick_timer() {
        let t = Timer::default();
        assert_eq!(tick_of(&t.created), 0);
        assert_eq!(t.run_count, 0);
        assert_eq!(t.last_run, OptionInstant::None);
        assert_eq!(t.tick_millis(), DEFAULT_TIMER_TICK_MS);
    }

    #[test]
    fn timer_clone_equals_original() {
        let _g = clock_guard();
        let t = timer_at(7, recording_cb as TimerCallbackType)
            .with_delay(tick_dur(1))
            .with_interval(tick_dur(2))
            .with_timeout(tick_dur(3));
        let c = t.clone();
        assert_eq!(t, c, "Clone must be value-preserving");
    }

    // ==================================================================
    // Timer::tick_millis — numeric limits / round-trip
    // ==================================================================

    #[test]
    fn tick_millis_falls_back_to_default_without_interval() {
        let _g = clock_guard();
        let t = timer_at(0, recording_cb as TimerCallbackType);
        assert_eq!(t.tick_millis(), DEFAULT_TIMER_TICK_MS);
        assert_eq!(t.tick_millis(), 10);

        // A delay/timeout must NOT be mistaken for an interval.
        let t = t.with_delay(tick_dur(999)).with_timeout(tick_dur(888));
        assert_eq!(t.tick_millis(), DEFAULT_TIMER_TICK_MS);
    }

    #[test]
    fn tick_millis_passes_tick_intervals_through_unclamped() {
        let _g = clock_guard();
        for raw in [0_u64, 1, 10, u64::from(u32::MAX), u64::MAX] {
            let t = timer_at(0, recording_cb as TimerCallbackType).with_interval(tick_dur(raw));
            assert_eq!(t.tick_millis(), raw, "tick interval {raw} must round-trip");
        }
    }

    #[test]
    fn tick_millis_system_interval_round_trips_whole_millis() {
        let _g = clock_guard();
        // from_millis -> millis() is an exact round-trip, even at u64::MAX
        // (secs*1000 + 615 lands exactly on u64::MAX without saturating).
        for ms in [0_u64, 1, 999, 1_000, 1_001, 86_400_000, u64::MAX] {
            let t = timer_at(0, recording_cb as TimerCallbackType).with_interval(sys_dur_millis(ms));
            assert_eq!(t.tick_millis(), ms, "millis {ms} must round-trip");
        }
    }

    #[test]
    fn tick_millis_saturates_instead_of_overflowing() {
        let _g = clock_guard();
        // secs::MAX * 1000 overflows u64 — `millis()` must saturate, not panic.
        let huge = Duration::System(SystemTimeDiff {
            secs: u64::MAX,
            nanos: 999_999_999,
        });
        let t = timer_at(0, recording_cb as TimerCallbackType).with_interval(huge);
        assert_eq!(t.tick_millis(), u64::MAX);
    }

    #[test]
    fn tick_millis_truncates_sub_millisecond_intervals_to_zero() {
        let _g = clock_guard();
        // A 999_999ns interval is a *sub-millisecond* tick request; it truncates
        // to 0, i.e. "tick as fast as possible", not to 1.
        let t = timer_at(0, recording_cb as TimerCallbackType)
            .with_interval(Duration::System(SystemTimeDiff::from_nanos(999_999)));
        assert_eq!(t.tick_millis(), 0);
    }

    // ==================================================================
    // Timer::is_about_to_finish — predicate boundaries
    // ==================================================================

    #[test]
    fn is_about_to_finish_is_false_without_a_timeout() {
        let _g = clock_guard();
        let t = timer_at(0, recording_cb as TimerCallbackType);
        assert!(!t.is_about_to_finish(&tick(0)));
        assert!(!t.is_about_to_finish(&tick(u64::MAX)), "no timeout = never finishes");
    }

    #[test]
    fn is_about_to_finish_boundary_is_strictly_greater() {
        let _g = clock_guard();
        let t = timer_at(100, recording_cb as TimerCallbackType).with_timeout(tick_dur(50));

        assert!(!t.is_about_to_finish(&tick(149)), "1 tick early");
        // Elapsed == timeout is NOT "about to finish" — the comparison is `>`.
        assert!(!t.is_about_to_finish(&tick(150)), "exactly at the timeout");
        assert!(t.is_about_to_finish(&tick(151)), "1 tick past the timeout");
    }

    #[test]
    fn is_about_to_finish_saturates_when_the_clock_runs_backwards() {
        let _g = clock_guard();
        let t = timer_at(1_000, recording_cb as TimerCallbackType).with_timeout(tick_dur(10));
        // `now` older than `created`: duration_since saturates to 0 rather than
        // underflowing, so the timer is simply "not finished".
        assert!(!t.is_about_to_finish(&tick(0)));
    }

    #[test]
    fn is_about_to_finish_at_the_u64_ceiling() {
        let _g = clock_guard();
        let t = timer_at(0, recording_cb as TimerCallbackType);

        let max_timeout = t.clone().with_timeout(tick_dur(u64::MAX));
        assert!(
            !max_timeout.is_about_to_finish(&tick(u64::MAX)),
            "MAX elapsed is not > MAX timeout"
        );

        let near_max = t.with_timeout(tick_dur(u64::MAX - 1));
        assert!(near_max.is_about_to_finish(&tick(u64::MAX)));
    }

    #[test]
    fn is_about_to_finish_never_fires_on_a_mismatched_clock_kind() {
        let _g = clock_guard();
        // A wall-clock timeout on a tick-driven timer: `duration_since` yields a
        // Tick duration, and Tick-vs-System comparison saturates to false. The
        // timeout can therefore NEVER expire — it degrades to "no timeout"
        // instead of panicking or firing immediately.
        let t = timer_at(0, recording_cb as TimerCallbackType).with_timeout(sys_dur_millis(1));
        assert!(!t.is_about_to_finish(&tick(u64::MAX)));
    }

    // ==================================================================
    // Timer::instant_of_next_run — getter invariants
    // ==================================================================

    #[test]
    fn instant_of_next_run_is_created_when_nothing_is_armed() {
        let _g = clock_guard();
        let t = timer_at(42, recording_cb as TimerCallbackType);
        assert_eq!(tick_of(&t.instant_of_next_run()), 42);
    }

    #[test]
    fn instant_of_next_run_prefers_last_run_over_created() {
        let _g = clock_guard();
        let mut t = timer_at(100, recording_cb as TimerCallbackType).with_interval(tick_dur(7));
        assert_eq!(tick_of(&t.instant_of_next_run()), 107, "no run yet: created + interval");

        t.last_run = OptionInstant::Some(tick(500));
        assert_eq!(tick_of(&t.instant_of_next_run()), 507, "after a run: last_run + interval");
    }

    #[test]
    fn instant_of_next_run_sums_delay_and_interval() {
        let _g = clock_guard();
        // NOTE: when BOTH are set, the schedule point is `base + delay + interval`
        // — the delay is re-added on every subsequent run, even though `invoke`
        // only gates the *first* run on the delay. Pinned here as current
        // behaviour; see the report.
        let mut t = timer_at(100, recording_cb as TimerCallbackType)
            .with_delay(tick_dur(5))
            .with_interval(tick_dur(7));
        assert_eq!(tick_of(&t.instant_of_next_run()), 112);

        t.last_run = OptionInstant::Some(tick(200));
        assert_eq!(tick_of(&t.instant_of_next_run()), 212);
    }

    #[test]
    fn instant_of_next_run_saturates_at_the_end_of_time() {
        let _g = clock_guard();
        let t = timer_at(u64::MAX, recording_cb as TimerCallbackType)
            .with_delay(tick_dur(u64::MAX))
            .with_interval(tick_dur(u64::MAX));
        // Three MAXes added together: saturating_add, not an overflow panic.
        assert_eq!(tick_of(&t.instant_of_next_run()), u64::MAX);
    }

    #[test]
    fn instant_of_next_run_ignores_mismatched_duration_kinds() {
        let _g = clock_guard();
        // System durations on a Tick instant are an undefined combination and are
        // dropped (saturate to `self`) rather than panicking.
        let t = timer_at(42, recording_cb as TimerCallbackType)
            .with_delay(sys_dur_millis(1_000))
            .with_interval(sys_dur_millis(1_000));
        assert_eq!(tick_of(&t.instant_of_next_run()), 42);
    }

    // ==================================================================
    // with_delay / with_interval / with_timeout — builder invariants
    // ==================================================================

    #[test]
    fn with_setters_are_independent_and_preserve_the_rest() {
        let _g = clock_guard();
        let t = timer_at(9, recording_cb as TimerCallbackType)
            .with_delay(tick_dur(1))
            .with_interval(tick_dur(2))
            .with_timeout(tick_dur(3));

        assert_eq!(t.delay, OptionDuration::Some(tick_dur(1)));
        assert_eq!(t.interval, OptionDuration::Some(tick_dur(2)));
        assert_eq!(t.timeout, OptionDuration::Some(tick_dur(3)));
        // The builders must not disturb identity/progress fields.
        assert_eq!(tick_of(&t.created), 9);
        assert_eq!(t.run_count, 0);
        assert_eq!(t.last_run, OptionInstant::None);
    }

    #[test]
    fn with_setters_are_last_write_wins() {
        let _g = clock_guard();
        let t = timer_at(0, recording_cb as TimerCallbackType)
            .with_delay(tick_dur(1))
            .with_delay(tick_dur(2))
            .with_interval(tick_dur(3))
            .with_interval(tick_dur(4))
            .with_timeout(tick_dur(5))
            .with_timeout(tick_dur(6));

        assert_eq!(t.delay, OptionDuration::Some(tick_dur(2)));
        assert_eq!(t.interval, OptionDuration::Some(tick_dur(4)));
        assert_eq!(t.timeout, OptionDuration::Some(tick_dur(6)));
    }

    #[test]
    fn with_setters_accept_extreme_durations() {
        let _g = clock_guard();
        let t = timer_at(0, recording_cb as TimerCallbackType)
            .with_delay(tick_dur(0))
            .with_interval(tick_dur(u64::MAX))
            .with_timeout(Duration::max());

        assert_eq!(t.delay, OptionDuration::Some(tick_dur(0)));
        assert_eq!(t.tick_millis(), u64::MAX);
        // Duration::max() is a System duration -> never expires on a tick clock.
        assert!(!t.is_about_to_finish(&tick(u64::MAX)));
    }

    // ==================================================================
    // TimerCallback — identity, ordering, hashing
    // ==================================================================

    #[test]
    fn timer_callback_create_has_no_ffi_ctx() {
        let cb = TimerCallback::create(cb_alpha as TimerCallbackType);
        assert_eq!(cb.ctx, OptionRefAny::None);

        // `create` and the `From` impl must agree.
        let from: TimerCallback = (cb_alpha as TimerCallbackType).into();
        assert_eq!(cb, from);
    }

    #[test]
    fn timer_callback_identity_is_by_function_pointer() {
        let a1 = TimerCallback::create(cb_alpha as TimerCallbackType);
        let a2 = TimerCallback::create(cb_alpha as TimerCallbackType);
        let b = TimerCallback::create(cb_beta as TimerCallbackType);

        assert_eq!(a1, a2, "same fn -> equal");
        assert_ne!(a1, b, "different fn -> not equal");
        assert_eq!(a1, a1.clone(), "Clone preserves identity");
    }

    #[test]
    fn timer_callback_ord_and_hash_agree_with_eq() {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        fn hash_of(cb: &TimerCallback) -> u64 {
            let mut h = DefaultHasher::new();
            cb.hash(&mut h);
            h.finish()
        }

        let a = TimerCallback::create(cb_alpha as TimerCallbackType);
        let a2 = a.clone();
        let b = TimerCallback::create(cb_beta as TimerCallbackType);

        assert_eq!(a.cmp(&a2), core::cmp::Ordering::Equal);
        assert_eq!(hash_of(&a), hash_of(&a2), "Eq values must hash equal");

        // Ord must be antisymmetric and consistent with partial_cmp.
        assert_eq!(a.cmp(&b), a.partial_cmp(&b).unwrap());
        assert_eq!(a.cmp(&b).reverse(), b.cmp(&a));
        assert_ne!(a.cmp(&b), core::cmp::Ordering::Equal, "distinct fns must order strictly");
    }

    #[test]
    fn timer_callback_debug_does_not_panic() {
        let s = format!("{:?}", TimerCallback::create(cb_alpha as TimerCallbackType));
        assert!(s.starts_with("TimerCallback"), "got {s}");
    }

    // ==================================================================
    // OptionTimer — round-trip
    // ==================================================================

    #[test]
    fn option_timer_round_trips_both_variants() {
        let _g = clock_guard();
        assert!(OptionTimer::None.into_option().is_none());
        assert!(OptionTimer::from(None).into_option().is_none());

        let t = timer_at(3, recording_cb as TimerCallbackType).with_interval(tick_dur(4));
        let round_tripped = OptionTimer::from(Some(t.clone()))
            .into_option()
            .expect("Some must survive the round-trip");
        assert_eq!(round_tripped, t, "encode == decode");
    }

    // ==================================================================
    // TimerCallbackInfo::create + getters
    // ==================================================================

    #[test]
    fn timer_callback_info_create_preserves_extremes() {
        with_env(|env| {
            let info = TimerCallbackInfo::create(
                env.info(),
                OptionDomNodeId::None,
                tick(u64::MAX),
                usize::MAX,
                true,
            );
            assert_eq!(info.call_count, usize::MAX, "no wrap at usize::MAX");
            assert!(info.is_about_to_finish);
            assert_eq!(tick_of(&info.frame_start), u64::MAX);
            assert!(info._abi_ref.is_null());
            assert!(info._abi_mut.is_null());

            let zero =
                TimerCallbackInfo::create(env.info(), OptionDomNodeId::None, tick(0), 0, false);
            assert_eq!(zero.call_count, 0);
            assert!(!zero.is_about_to_finish);
            assert_eq!(tick_of(&zero.frame_start), 0);
        });
    }

    #[test]
    fn get_current_time_returns_frame_start_verbatim() {
        with_env(|env| {
            for t in [0_u64, 1, u64::MAX] {
                let info =
                    TimerCallbackInfo::create(env.info(), OptionDomNodeId::None, tick(t), 0, false);
                assert_eq!(info.get_current_time(), tick(t));
            }
        });
    }

    #[test]
    fn attached_node_queries_are_none_without_an_attached_node() {
        with_env(|env| {
            let info = env.timer_info();
            assert!(info.get_attached_node_size().is_none());
            assert!(info.get_attached_node_position().is_none());
        });
    }

    #[test]
    fn attached_node_queries_are_none_for_a_bogus_node() {
        with_env(|env| {
            // Largest node index that survives the +1 encoding, in a DOM that
            // doesn't exist, on a window with no layout results at all.
            let bogus = DomNodeId {
                dom: DomId { inner: usize::MAX },
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(usize::MAX - 1))),
            };
            let info = TimerCallbackInfo::create(
                env.info(),
                OptionDomNodeId::Some(bogus),
                tick(0),
                0,
                false,
            );
            assert!(info.get_attached_node_size().is_none(), "must not panic or index OOB");
            assert!(info.get_attached_node_position().is_none());
        });
    }

    #[test]
    fn get_callback_info_and_mut_alias_the_same_inner_info() {
        with_env(|env| {
            let mut info = env.timer_info();
            let addr_shared = std::ptr::from_ref(info.get_callback_info());
            let addr_mut = std::ptr::from_mut(info.get_callback_info_mut()).cast_const();
            assert!(std::ptr::eq(addr_shared, addr_mut), "both must alias the inner CallbackInfo");

            // A change pushed through the &mut view lands in the shared log.
            info.get_callback_info_mut().prevent_default();
            assert!(matches!(env.take_one(), CallbackChange::PreventDefault));
        });
    }

    #[test]
    fn get_ctx_is_none_for_native_rust_callbacks() {
        with_env(|env| {
            assert_eq!(env.timer_info().get_ctx(), OptionRefAny::None);
        });
    }

    #[test]
    fn get_ctx_hands_back_the_ffi_callable() {
        let ctx = RefAny::new(0xDEAD_BEEF_u32);
        with_env_cfg(false, OptionRefAny::Some(ctx.clone()), |env| {
            let got = env.timer_info().get_ctx().into_option().expect("ctx must survive");
            assert_eq!(got, ctx, "get_ctx must hand back the same RefAny");
        });
    }

    // ==================================================================
    // Delegated mutators — every one must land in the transaction log
    // ==================================================================

    #[test]
    fn add_and_remove_timer_push_the_matching_changes() {
        let _g = clock_guard();
        with_env(|env| {
            let mut info = env.timer_info();
            let id = TimerId { id: usize::MAX };
            info.add_timer(id, timer_at(1, recording_cb as TimerCallbackType));
            let CallbackChange::AddTimer { timer_id, timer } = env.take_one() else {
                panic!("expected AddTimer");
            };
            assert_eq!(timer_id, id);
            assert_eq!(tick_of(&timer.created), 1, "the timer must be stored verbatim");

            info.remove_timer(TimerId { id: 0 });
            let CallbackChange::RemoveTimer { timer_id } = env.take_one() else {
                panic!("expected RemoveTimer");
            };
            assert_eq!(timer_id.id, 0, "id 0 (a reserved system id) is still accepted");
        });
    }

    #[test]
    fn add_and_remove_thread_push_the_matching_changes() {
        extern "C" fn noop_worker(_d: RefAny, _s: ThreadSender, _r: ThreadReceiver) {}

        with_env(|env| {
            let mut info = env.timer_info();
            let id = ThreadId::unique();
            let thread = Thread::create(
                RefAny::new(0_usize),
                RefAny::new(0_usize),
                noop_worker as ThreadCallbackType,
            );
            info.add_thread(id, thread);
            let CallbackChange::AddThread { thread_id, .. } = env.take_one() else {
                panic!("expected AddThread");
            };
            assert_eq!(thread_id, id);

            info.remove_thread(id);
            let CallbackChange::RemoveThread { thread_id } = env.take_one() else {
                panic!("expected RemoveThread");
            };
            assert_eq!(thread_id, id);
        });
    }

    #[test]
    fn nullary_mutators_push_exactly_one_change_each_in_order() {
        with_env(|env| {
            let mut info = env.timer_info();
            info.stop_propagation();
            info.prevent_default();
            info.close_window();
            info.hide_tooltip();
            info.reload_system_fonts();
            info.update_all_image_callbacks();
            info.reset_cursor_blink();
            info.set_cursor_visibility_toggle();

            let changes = env.take_changes();
            assert_eq!(changes.len(), 8, "one change per call, no drops: {changes:?}");
            assert!(matches!(changes[0], CallbackChange::StopPropagation));
            assert!(matches!(changes[1], CallbackChange::PreventDefault));
            assert!(matches!(changes[2], CallbackChange::CloseWindow));
            assert!(matches!(changes[3], CallbackChange::HideTooltip));
            assert!(matches!(changes[4], CallbackChange::ReloadSystemFonts));
            assert!(matches!(changes[5], CallbackChange::UpdateAllImageCallbacks));
            assert!(matches!(changes[6], CallbackChange::ResetCursorBlink));
            assert!(matches!(changes[7], CallbackChange::ToggleCursorVisibility));
        });
    }

    #[test]
    fn set_cursor_visibility_records_both_polarities() {
        with_env(|env| {
            let mut info = env.timer_info();
            info.set_cursor_visibility(true);
            info.set_cursor_visibility(false);

            let changes = env.take_changes();
            assert_eq!(changes.len(), 2);
            let visibilities: Vec<bool> = changes
                .iter()
                .map(|c| match c {
                    CallbackChange::SetCursorVisibility { visible } => *visible,
                    other => panic!("expected SetCursorVisibility, got {other:?}"),
                })
                .collect();
            assert_eq!(visibilities, vec![true, false]);
        });
    }

    #[test]
    fn create_window_and_modify_window_state_push_changes() {
        with_env(|env| {
            let mut info = env.timer_info();
            info.create_window(WindowCreateOptions::default());
            assert!(matches!(env.take_one(), CallbackChange::CreateNewWindow { .. }));

            info.modify_window_state(FullWindowState::default());
            assert!(matches!(env.take_one(), CallbackChange::ModifyWindowState { .. }));
        });
    }

    #[test]
    fn image_cache_mutators_accept_degenerate_and_unicode_ids() {
        with_env(|env| {
            let mut info = env.timer_info();

            // A 0x0 null image with an empty tag is degenerate but legal.
            let img = ImageRef::null_image(0, 0, RawImageFormat::RGBA8, Vec::new());
            let id: AzString = String::new().into();
            info.add_image_to_cache(id.clone(), img);
            let CallbackChange::AddImageToCache { id: got, .. } = env.take_one() else {
                panic!("expected AddImageToCache");
            };
            assert_eq!(got, id, "an empty id is passed through, not rejected");

            // Embedded NUL, an RTL override and astral-plane chars must survive
            // the AzString round-trip byte-for-byte.
            let nasty: AzString = String::from("🚀\u{0}\u{202E}id\u{1F600}").into();
            info.remove_image_from_cache(nasty.clone());
            let CallbackChange::RemoveImageFromCache { id: got } = env.take_one() else {
                panic!("expected RemoveImageFromCache");
            };
            assert_eq!(got.as_str(), nasty.as_str());
        });
    }

    #[test]
    fn trigger_virtual_view_rerender_accepts_out_of_range_ids() {
        with_env(|env| {
            let mut info = env.timer_info();
            info.trigger_virtual_view_rerender(DomId { inner: usize::MAX }, NodeId::new(usize::MAX));
            let CallbackChange::UpdateVirtualView { dom_id, node_id } = env.take_one() else {
                panic!("expected UpdateVirtualView");
            };
            // Recorded verbatim — validation happens when the change is applied,
            // not here, and neither id may overflow on the way in.
            assert_eq!(dom_id.inner, usize::MAX);
            assert_eq!(node_id, NodeId::new(usize::MAX));
        });
    }

    #[test]
    fn open_menu_has_no_position_and_open_menu_at_carries_one() {
        with_env(|env| {
            let mut info = env.timer_info();

            info.open_menu(empty_menu());
            let CallbackChange::OpenMenu { position, .. } = env.take_one() else {
                panic!("expected OpenMenu");
            };
            assert!(position.is_none(), "open_menu must defer to menu.position");

            info.open_menu_at(empty_menu(), LogicalPosition::new(-1.5, 2.5));
            let CallbackChange::OpenMenu { position, .. } = env.take_one() else {
                panic!("expected OpenMenu");
            };
            let p = position.expect("open_menu_at must pin a position");
            assert_eq!((p.x, p.y), (-1.5, 2.5), "negative coordinates are legal");
        });
    }

    #[test]
    fn open_menu_at_passes_non_finite_coordinates_through_unchanged() {
        with_env(|env| {
            let mut info = env.timer_info();
            info.open_menu_at(
                empty_menu(),
                LogicalPosition::new(f32::NAN, f32::INFINITY),
            );
            let CallbackChange::OpenMenu { position, .. } = env.take_one() else {
                panic!("expected OpenMenu");
            };
            let p = position.expect("position must be recorded");
            // No clamping/sanitising at this layer — but it must not panic either.
            assert!(p.x.is_nan());
            assert!(p.y.is_infinite() && p.y.is_sign_positive());

            info.open_menu_at(empty_menu(), LogicalPosition::new(f32::MAX, f32::MIN));
            let CallbackChange::OpenMenu { position, .. } = env.take_one() else {
                panic!("expected OpenMenu");
            };
            let p = position.expect("position must be recorded");
            assert_eq!((p.x, p.y), (f32::MAX, f32::MIN));
        });
    }

    #[test]
    fn open_menu_for_hit_node_is_false_and_silent_without_a_hit_node() {
        with_env(|env| {
            let mut info = env.timer_info();
            // Hit node is NONE and the window has no layout results: the menu has
            // nothing to anchor to.
            assert!(!info.open_menu_for_hit_node(empty_menu()));
            assert!(
                env.take_changes().is_empty(),
                "a failed anchor must not queue a half-open menu"
            );
        });
    }

    #[test]
    fn show_tooltip_falls_back_to_the_origin_without_a_cursor() {
        with_env(|env| {
            let mut info = env.timer_info();
            info.show_tooltip(String::from("hi").into());
            let CallbackChange::ShowTooltip { text, position } = env.take_one() else {
                panic!("expected ShowTooltip");
            };
            assert_eq!(text.as_str(), "hi");
            assert_eq!((position.x, position.y), (0.0, 0.0), "no cursor -> origin");
        });
    }

    #[test]
    fn show_tooltip_uses_the_viewport_cursor_when_there_is_one() {
        with_env(|env| {
            let cursor = LogicalPosition::new(3.0, 4.0);
            let mut info = TimerCallbackInfo::create(
                env.info_with(OptionLogicalPosition::None, OptionLogicalPosition::Some(cursor)),
                OptionDomNodeId::None,
                tick(0),
                0,
                false,
            );
            info.show_tooltip(String::from("t").into());
            let CallbackChange::ShowTooltip { position, .. } = env.take_one() else {
                panic!("expected ShowTooltip");
            };
            assert_eq!((position.x, position.y), (3.0, 4.0));
        });
    }

    #[test]
    fn show_tooltip_at_records_empty_text_and_non_finite_positions() {
        with_env(|env| {
            let mut info = env.timer_info();
            info.show_tooltip_at(String::new().into(), LogicalPosition::new(f32::NAN, -0.0));
            let CallbackChange::ShowTooltip { text, position } = env.take_one() else {
                panic!("expected ShowTooltip");
            };
            assert_eq!(text.as_str(), "", "empty tooltip text is not rejected");
            assert!(position.x.is_nan());
            assert!(position.y.is_sign_negative());
        });
    }

    // ==================================================================
    // Scroll delegation — numeric edges
    // ==================================================================

    #[test]
    fn scroll_to_and_unclamped_differ_only_in_the_clamp_flag() {
        with_env(|env| {
            let mut info = env.timer_info();
            let node = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0)));
            let pos = LogicalPosition::new(10.0, 20.0);

            info.scroll_to(DomId::ROOT_ID, node, pos);
            info.scroll_to_unclamped(DomId::ROOT_ID, node, pos);

            let changes = env.take_changes();
            assert_eq!(changes.len(), 2);
            let flags: Vec<bool> = changes
                .iter()
                .map(|c| match c {
                    CallbackChange::ScrollTo {
                        dom_id,
                        node_id,
                        position,
                        unclamped,
                    } => {
                        assert_eq!(*dom_id, DomId::ROOT_ID);
                        assert_eq!(*node_id, node);
                        assert_eq!((position.x, position.y), (10.0, 20.0));
                        *unclamped
                    }
                    other => panic!("expected ScrollTo, got {other:?}"),
                })
                .collect();
            assert_eq!(flags, vec![false, true], "only the overscroll flag differs");
        });
    }

    #[test]
    fn scroll_to_records_zero_negative_and_non_finite_positions() {
        with_env(|env| {
            let mut info = env.timer_info();
            let node = NodeHierarchyItemId::NONE;

            for pos in [
                LogicalPosition::new(0.0, 0.0),
                LogicalPosition::new(-1.0, -f32::MAX),
                LogicalPosition::new(f32::MAX, f32::INFINITY),
            ] {
                info.scroll_to(DomId::ROOT_ID, node, pos);
                let CallbackChange::ScrollTo { position, .. } = env.take_one() else {
                    panic!("expected ScrollTo");
                };
                assert_eq!(position.x.to_bits(), pos.x.to_bits(), "x must be recorded bit-exact");
                assert_eq!(position.y.to_bits(), pos.y.to_bits(), "y must be recorded bit-exact");
            }

            // NaN separately — it is never == to itself.
            info.scroll_to_unclamped(
                DomId { inner: usize::MAX },
                node,
                LogicalPosition::new(f32::NAN, f32::NAN),
            );
            let CallbackChange::ScrollTo {
                position, unclamped, ..
            } = env.take_one()
            else {
                panic!("expected ScrollTo");
            };
            assert!(position.x.is_nan() && position.y.is_nan(), "NaN is passed through, not zeroed");
            assert!(unclamped);
        });
    }

    #[test]
    fn scroll_queries_are_none_on_an_empty_window() {
        with_env(|env| {
            let info = env.timer_info();
            assert!(info.get_scroll_node_info(DomId::ROOT_ID, NodeId::new(0)).is_none());
            assert!(
                info.get_scroll_node_info(DomId { inner: usize::MAX }, NodeId::new(usize::MAX))
                    .is_none(),
                "an out-of-range dom/node must return None, not panic"
            );
            assert!(info.find_scroll_parent(DomId::ROOT_ID, NodeId::new(0)).is_none());
            assert!(
                info.find_scroll_parent(DomId { inner: usize::MAX }, NodeId::new(usize::MAX))
                    .is_none()
            );
        });
    }

    #[test]
    fn scroll_input_queue_starts_empty_and_draining_is_idempotent() {
        with_env(|env| {
            let info = env.timer_info();
            let queue = info.get_scroll_input_queue();
            assert!(queue.take_all().is_empty());
            assert!(queue.take_all().is_empty(), "draining twice must stay empty");
        });
    }

    // ==================================================================
    // Predicates / state getters
    // ==================================================================

    #[test]
    fn the_three_drag_predicates_are_aliases_of_left_down() {
        for left_down in [false, true] {
            with_env_cfg(left_down, OptionRefAny::None, |env| {
                let info = env.timer_info();
                assert_eq!(info.get_current_mouse_state().left_down, left_down);
                assert_eq!(info.is_dragging(), left_down);
                assert_eq!(info.is_drag_active(), left_down);
                assert_eq!(info.is_node_drag_active(), left_down);
            });
        }
    }

    #[test]
    fn pen_predicates_are_false_without_a_pen() {
        with_env(|env| {
            let info = env.timer_info();
            assert!(!info.is_pen_in_contact());
            assert!(!info.is_pen_eraser());
            assert!(!info.is_pen_barrel_button_pressed());
        });
    }

    #[test]
    fn drag_and_gesture_predicates_are_false_on_a_fresh_window() {
        with_env(|env| {
            let info = env.timer_info();
            assert!(!info.is_file_drag_active());
            assert!(!info.has_sufficient_history_for_gestures());
        });
    }

    #[test]
    fn is_dom_focused_is_false_for_every_dom_when_nothing_is_focused() {
        with_env(|env| {
            let info = env.timer_info();
            assert!(!info.is_dom_focused(DomId::ROOT_ID));
            assert!(!info.is_dom_focused(DomId { inner: usize::MAX }));
        });
    }

    #[test]
    fn window_state_getters_mirror_the_current_window_state() {
        with_env(|env| {
            let info = env.timer_info();
            let default_state = FullWindowState::default();
            assert_eq!(info.get_current_window_flags(), default_state.flags);
            assert_eq!(info.get_current_keyboard_state(), default_state.keyboard_state);
            assert_eq!(info.get_current_mouse_state(), default_state.mouse_state);
        });
    }

    #[test]
    fn cursor_getters_round_trip_including_nan() {
        with_env(|env| {
            let info = env.timer_info();
            assert!(info.get_cursor_position().is_none());
            assert_eq!(info.get_cursor_relative_to_viewport(), OptionLogicalPosition::None);
            assert!(info.get_cursor_relative_to_node().is_none());

            let viewport = LogicalPosition::new(f32::NAN, 7.5);
            let relative = LogicalPosition::new(-3.0, f32::INFINITY);
            let info = TimerCallbackInfo::create(
                env.info_with(
                    OptionLogicalPosition::Some(relative),
                    OptionLogicalPosition::Some(viewport),
                ),
                OptionDomNodeId::None,
                tick(0),
                0,
                false,
            );

            let got = info.get_cursor_position().expect("cursor must be Some");
            assert!(got.x.is_nan() && got.y == 7.5);

            let node_rel = info
                .get_cursor_relative_to_node()
                .into_option()
                .expect("relative cursor must be Some");
            assert_eq!(node_rel.x, -3.0);
            assert!(node_rel.y.is_infinite());
        });
    }

    // ==================================================================
    // Timer::invoke — the scheduling state machine
    // ==================================================================

    #[test]
    fn invoke_does_not_run_the_callback_before_the_delay_elapses() {
        let _g = clock_guard();
        with_env(|env| {
            let mut t =
                timer_at(0, recording_cb as TimerCallbackType).with_delay(tick_dur(100));
            let info = env.info();

            set_now(99);
            let r = t.invoke(&info, &fake_clock_cb());
            assert_eq!(CB_INVOCATIONS.load(Ordering::SeqCst), 0, "callback must not fire early");
            assert_eq!(r.should_update, Update::DoNothing);
            assert_eq!(r.should_terminate, TerminateTimer::Continue);
            // A skipped tick must leave the timer's progress untouched.
            assert_eq!(t.run_count, 0);
            assert_eq!(t.last_run, OptionInstant::None);

            // The boundary is inclusive: elapsed == delay runs.
            set_now(100);
            let r = t.invoke(&info, &fake_clock_cb());
            assert_eq!(CB_INVOCATIONS.load(Ordering::SeqCst), 1);
            assert_eq!(r.should_terminate, TerminateTimer::Continue);
            assert_eq!(t.run_count, 1);
            assert_eq!(t.last_run, OptionInstant::Some(tick(100)));
        });
    }

    #[test]
    fn invoke_gates_subsequent_runs_on_the_interval() {
        let _g = clock_guard();
        with_env(|env| {
            let mut t =
                timer_at(0, recording_cb as TimerCallbackType).with_interval(tick_dur(10));
            let info = env.info();

            // No delay -> the first invoke runs immediately.
            let r = t.invoke(&info, &fake_clock_cb());
            assert_eq!(CB_INVOCATIONS.load(Ordering::SeqCst), 1);
            assert_eq!(r.should_terminate, TerminateTimer::Continue);
            assert_eq!(t.run_count, 1);

            set_now(9);
            let r = t.invoke(&info, &fake_clock_cb());
            assert_eq!(CB_INVOCATIONS.load(Ordering::SeqCst), 1, "1 tick short of the interval");
            assert_eq!(r.should_update, Update::DoNothing);
            assert_eq!(t.run_count, 1, "a skipped tick must not count as a run");

            set_now(10);
            let _ = t.invoke(&info, &fake_clock_cb());
            assert_eq!(CB_INVOCATIONS.load(Ordering::SeqCst), 2);
            assert_eq!(t.run_count, 2);
            assert_eq!(t.last_run, OptionInstant::Some(tick(10)));
        });
    }

    #[test]
    fn invoke_hands_the_callback_the_run_count_and_frame_start() {
        let _g = clock_guard();
        with_env(|env| {
            let mut t = timer_at(0, recording_cb as TimerCallbackType);
            let info = env.info();

            set_now(5);
            let _ = t.invoke(&info, &fake_clock_cb());
            assert_eq!(CB_SEEN_CALL_COUNT.load(Ordering::SeqCst), 0, "first run is call 0");
            assert_eq!(CB_SEEN_FRAME_START.load(Ordering::SeqCst), 5, "frame_start == now");
            assert!(!CB_SEEN_ABOUT_TO_FINISH.load(Ordering::SeqCst));

            set_now(6);
            let _ = t.invoke(&info, &fake_clock_cb());
            assert_eq!(CB_SEEN_CALL_COUNT.load(Ordering::SeqCst), 1, "run_count increments by 1");
            assert_eq!(CB_SEEN_FRAME_START.load(Ordering::SeqCst), 6);
        });
    }

    #[test]
    fn invoke_forces_terminate_once_the_timeout_expires() {
        let _g = clock_guard();
        with_env(|env| {
            let mut t = timer_at(0, recording_cb as TimerCallbackType).with_timeout(tick_dur(5));
            let info = env.info();
            // The callback insists on Continue...
            CB_RETURN_TERMINATE.store(false, Ordering::SeqCst);

            set_now(5);
            let r = t.invoke(&info, &fake_clock_cb());
            assert_eq!(r.should_terminate, TerminateTimer::Continue, "elapsed == timeout: alive");
            assert!(!CB_SEEN_ABOUT_TO_FINISH.load(Ordering::SeqCst));

            set_now(6);
            let r = t.invoke(&info, &fake_clock_cb());
            // ...but the timeout overrides it, and the callback is told so.
            assert!(CB_SEEN_ABOUT_TO_FINISH.load(Ordering::SeqCst), "last-call flag must be set");
            assert_eq!(r.should_terminate, TerminateTimer::Terminate);
            assert_eq!(CB_INVOCATIONS.load(Ordering::SeqCst), 2, "the final run still happens");
        });
    }

    #[test]
    fn invoke_honours_a_callback_requested_terminate() {
        let _g = clock_guard();
        with_env(|env| {
            let mut t = timer_at(0, recording_cb as TimerCallbackType);
            let info = env.info();
            CB_RETURN_TERMINATE.store(true, Ordering::SeqCst);

            let r = t.invoke(&info, &fake_clock_cb());
            assert_eq!(r.should_terminate, TerminateTimer::Terminate);
            // Termination is the caller's job; invoke still records the run.
            assert_eq!(t.run_count, 1);
            assert_eq!(t.last_run, OptionInstant::Some(tick(0)));
        });
    }

    #[test]
    fn invoke_skips_deterministically_when_the_clock_runs_backwards() {
        let _g = clock_guard();
        with_env(|env| {
            let mut t =
                timer_at(0, recording_cb as TimerCallbackType).with_interval(tick_dur(10));
            t.last_run = OptionInstant::Some(tick(1_000));
            let info = env.info();

            // now(0) is *older* than last_run(1000): duration_since saturates to 0,
            // 0 < 10, so the tick is skipped — no panic, no spurious run.
            set_now(0);
            let r = t.invoke(&info, &fake_clock_cb());
            assert_eq!(CB_INVOCATIONS.load(Ordering::SeqCst), 0);
            assert_eq!(r.should_terminate, TerminateTimer::Continue);
            assert_eq!(t.run_count, 0);
            assert_eq!(t.last_run, OptionInstant::Some(tick(1_000)), "last_run is untouched");
        });
    }

    #[test]
    fn invoke_with_a_mismatched_interval_kind_runs_every_tick() {
        let _g = clock_guard();
        with_env(|env| {
            // A wall-clock interval on a tick-driven timer: the Tick-vs-System
            // comparison saturates to false, so the "not yet" branch is never
            // taken and the interval degrades to "run on every invoke".
            let mut t =
                timer_at(0, recording_cb as TimerCallbackType).with_interval(sys_dur_millis(60_000));
            let info = env.info();

            let _ = t.invoke(&info, &fake_clock_cb());
            set_now(1);
            let _ = t.invoke(&info, &fake_clock_cb());
            assert_eq!(
                CB_INVOCATIONS.load(Ordering::SeqCst),
                2,
                "a 60s interval does not throttle a tick clock"
            );
            assert_eq!(t.run_count, 2);
        });
    }

    #[test]
    fn invoke_at_the_end_of_time_does_not_panic() {
        let _g = clock_guard();
        with_env(|env| {
            let mut t = timer_at(u64::MAX, recording_cb as TimerCallbackType)
                .with_delay(tick_dur(u64::MAX))
                .with_interval(tick_dur(u64::MAX))
                .with_timeout(tick_dur(u64::MAX));
            let info = env.info();

            set_now(u64::MAX);
            // elapsed = 0, delay = MAX -> 0 < MAX -> skipped, no overflow anywhere.
            let r = t.invoke(&info, &fake_clock_cb());
            assert_eq!(CB_INVOCATIONS.load(Ordering::SeqCst), 0);
            assert_eq!(r.should_terminate, TerminateTimer::Continue);
            assert_eq!(t.run_count, 0);
        });
    }
}
