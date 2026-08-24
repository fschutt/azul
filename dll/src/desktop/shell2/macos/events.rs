//! macOS Event handling - converts NSEvent to Azul events and dispatches callbacks.

use super::super::common::debug_server::LogCategory;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::{DomId, NodeId, ScrollbarOrientation},
    events::{EventFilter, MouseButton, ProcessEventResult},
    geom::{LogicalPosition, PhysicalPositionI32},
    hit_test::{CursorTypeHitTest, FullHitTest},
    window::{
        CursorPosition, KeyboardState, MouseCursorType, MouseState, OptionMouseCursorType,
        VirtualKeyCode, WindowFrame,
    },
};
use azul_layout::{
    callbacks::CallbackInfo,
    managers::{
        hover::InputPointId,
        scroll_state::{ScrollbarComponent, ScrollbarHit},
    },
    solver3::display_list::DisplayList,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
use objc2_foundation::NSPoint;

use super::MacOSWindow;
// Re-export common types
pub use crate::desktop::shell2::common::event::HitTestNode;
// Import V2 cross-platform event processing trait
use crate::desktop::shell2::common::event::{
    PlatformWindow, BUTTON_STATE_LEFT, BUTTON_STATE_RIGHT, BUTTON_STATE_MIDDLE, BUTTON_STATE_NONE,
};
// The keycode table lives in `common` so it is tested on every host: nothing in
// CI compiles this module, so a test next to the table would never run.
use crate::desktop::shell2::common::event::macos_keycode_to_virtual_key as convert_keycode;

// macOS hardware keycodes for modifier keys
const MACOS_KEYCODE_LSHIFT: u16 = 0x38;
const MACOS_KEYCODE_LCONTROL: u16 = 0x3B;
const MACOS_KEYCODE_LALT: u16 = 0x3A;
const MACOS_KEYCODE_LWIN: u16 = 0x37;
const MACOS_KEYCODE_RSHIFT: u16 = 0x3C;
const MACOS_KEYCODE_RCONTROL: u16 = 0x3E;
const MACOS_KEYCODE_RALT: u16 = 0x3D;
const MACOS_KEYCODE_RWIN: u16 = 0x36;
const MACOS_KEYCODE_CAPSLOCK: u16 = 0x39;

/// Device-DEPENDENT modifier masks (IOKit `IOLLEvent.h`), carried in the low
/// 16 bits of `modifierFlags` next to the device-independent class flags
/// (`Shift`, `Control`, …, all `1<<16` and above). They are the only way to
/// tell a left modifier from its right twin AND the only way to read a
/// release correctly: letting go of LShift while RShift is held leaves the
/// `Shift` class flag set, so the class flag alone reports "still down".
const NX_DEVICE_LCONTROL: usize = 0x0000_0001;
const NX_DEVICE_LSHIFT: usize = 0x0000_0002;
const NX_DEVICE_RSHIFT: usize = 0x0000_0004;
const NX_DEVICE_LWIN: usize = 0x0000_0008;
const NX_DEVICE_RWIN: usize = 0x0000_0010;
const NX_DEVICE_LALT: usize = 0x0000_0020;
const NX_DEVICE_RALT: usize = 0x0000_0040;
const NX_DEVICE_RCONTROL: usize = 0x0000_2000;

fn button_to_flags(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => BUTTON_STATE_LEFT,
        MouseButton::Right => BUTTON_STATE_RIGHT,
        MouseButton::Middle => BUTTON_STATE_MIDDLE,
        _ => BUTTON_STATE_NONE,
    }
}

/// Convert macOS window coordinates to Azul logical coordinates.
///
/// macOS uses a bottom-left origin coordinate system where Y=0 is at the bottom.
/// Azul/WebRender uses a top-left origin coordinate system where Y=0 is at the top.
/// This function converts from macOS to Azul coordinates.
#[inline]
pub(super) fn macos_to_azul_coords(location: NSPoint, window_height: f32) -> LogicalPosition {
    LogicalPosition::new(location.x as f32, window_height - location.y as f32)
}

/// Result of processing an event - determines whether to redraw, update layout, etc.
///
/// TODO(superplan g6): this macOS-only enum is a (now less) lossy projection of
/// `azul_core::events::ProcessEventResult` (see `convert_process_result` below —
/// `UpdateHitTesterAndProcessAgain` and `ShouldRegenerateDomAllWindows` still
/// collapse into coarser variants; `ShouldIncrementalRelayout` now maps 1:1 to
/// `RegenerateLayoutIncremental`). It should be dropped in favour of the core
/// type, but ~40 match sites in `macos/mod.rs` (owned by Group 7) consume these
/// variants, so the conversion has to be removed there in the same pass. Left
/// as-is to avoid breaking mod.rs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventProcessResult {
    /// No action needed
    DoNothing,
    /// Request redraw with existing display list (scroll offset change, etc.)
    RequestRedraw,
    /// Regenerate display list from existing layout tree (text edit, no DOM rebuild)
    UpdateDisplayList,
    /// Restyle / runtime edit changed layout-affecting properties (hover/focus CSS,
    /// `set_css_property`, `set_node_text`): re-run layout on the EXISTING StyledDom
    /// via `incremental_relayout()` — NO DOM rebuild (the `layout_callback` is NOT
    /// re-invoked). This is the non-lossy projection of
    /// `ProcessEventResult::ShouldIncrementalRelayout` (previously collapsed into
    /// `UpdateDisplayList`, which only rebuilt the display list and never re-ran
    /// layout). Mapped to `incremental_relayout()` + `request_relayout_only()` in the
    /// main-window input handlers.
    RegenerateLayoutIncremental,
    /// Full DOM rebuild needed (layout callback will be re-invoked)
    RegenerateDisplayList,
    /// Window should close
    CloseWindow,
}

impl MacOSWindow {
    /// Convert ProcessEventResult to platform-specific EventProcessResult
    #[inline]
    fn convert_process_result(result: azul_core::events::ProcessEventResult) -> EventProcessResult {
        use azul_core::events::ProcessEventResult as PER;
        match result {
            PER::DoNothing => EventProcessResult::DoNothing,
            PER::ShouldReRenderCurrentWindow => EventProcessResult::RequestRedraw,
            PER::ShouldUpdateDisplayListCurrentWindow => EventProcessResult::UpdateDisplayList,
            PER::UpdateHitTesterAndProcessAgain => EventProcessResult::RegenerateDisplayList,
            // Restyle that needs re-layout (not a DOM rebuild) now maps to its own
            // variant instead of collapsing into UpdateDisplayList, so the main-window
            // input handlers can route it to incremental_relayout() (the fast path)
            // rather than forcing the full regenerate_layout().
            PER::ShouldIncrementalRelayout => EventProcessResult::RegenerateLayoutIncremental,
            PER::ShouldRegenerateDomCurrentWindow => EventProcessResult::RegenerateDisplayList,
            PER::ShouldRegenerateDomAllWindows => EventProcessResult::RegenerateDisplayList,
        }
    }

    /// Convert a `ProcessEventResult`, fanning `ShouldRegenerateDomAllWindows`
    /// out to every OTHER registered window first (mirrors the X11 fan-out).
    /// The conversion to the macOS-local `EventProcessResult` erases the
    /// all-windows information, so the fan-out must happen here — without it,
    /// app-global changes (undo/redo, shared-state mutations from a second
    /// window) refreshed only the window that received the event; every other
    /// window kept rendering the stale DOM until its own next input.
    fn convert_result_with_fanout(
        &mut self,
        result: azul_core::events::ProcessEventResult,
    ) -> EventProcessResult {
        use azul_core::events::ProcessEventResult as PER;
        if result == PER::ShouldRegenerateDomAllWindows {
            let self_ptr = self as *mut MacOSWindow;
            for wptr in super::registry::get_all_window_ptrs() {
                if wptr == self_ptr {
                    continue; // self is handled by the returned result
                }
                let w = unsafe { &mut *wptr };
                w.common.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
                w.request_redraw();
            }
        }
        Self::convert_process_result(result)
    }

    // NOTE: perform_scrollbar_hit_test(), handle_scrollbar_click(), and handle_scrollbar_drag()
    // are now provided by the PlatformWindow trait as default methods.
    // The trait methods are cross-platform and work identically.
    // See dll/src/desktop/shell2/common/event.rs for the implementation.

    /// Process a mouse button down event.
    pub fn handle_mouse_down(
        &mut self,
        event: &NSEvent,
        button: MouseButton,
    ) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.common.current_window_state().size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Check for scrollbar hit FIRST (before state changes)
        // Use trait method from PlatformWindow
        if let Some(scrollbar_hit_id) = PlatformWindow::perform_scrollbar_hit_test(self, position)
        {
            // The scrollbar consumes the press, but the button is still
            // PHYSICALLY DOWN: returning before writing the mouse state left
            // `left_down == false` and `cursor_position` stale for the whole
            // thumb drag, so the live pointer state disagreed with the hardware
            // for as long as the user held the thumb. The headless backend
            // (which the E2E suite scripts against) always wrote them; the
            // write plus its sanctioned swallow now live in the shared trait so
            // every backend gets the same answer.
            let result = PlatformWindow::handle_scrollbar_press(
                self,
                scrollbar_hit_id,
                position,
                button,
                "macos.handle_mouse_down.scrollbar_click",
            );
            return self.convert_result_with_fanout(result);
        }

        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("macos.handle_mouse_down");

        // Update mouse state
        self.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(position);

        // Set appropriate button flag
        match button {
            MouseButton::Left => self.common.mouse_state_mut().left_down = true,
            MouseButton::Right => self.common.mouse_state_mut().right_down = true,
            MouseButton::Middle => self.common.mouse_state_mut().middle_down = true,
            _ => {}
        }

        // Record input sample for gesture detection (button down starts new session)
        self.record_input_sample(position, button_to_flags(button), true, false, None);

        // Perform hit testing and update last_hit_test
        self.update_hit_test(position);

        // Feed Wacom/pen pressure+tilt (tablet events arrive as mouse events).
        self.feed_tablet_pen(event, position, true);

        // Use V2 cross-platform event system - it will automatically:
        // - Detect MouseDown event (left/right/middle)
        // - Dispatch to hovered nodes (including CSD buttons with callbacks)
        // - Handle event propagation
        // - Process callback results recursively
        let result = self.process_window_events(0);

        self.convert_result_with_fanout(result)
    }

    /// Process a mouse button up event.
    pub fn handle_mouse_up(&mut self, event: &NSEvent, button: MouseButton) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.common.current_window_state().size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // End scrollbar drag if active (before state changes).
        //
        // The release must CLEAR what the press latched: `handle_mouse_down`
        // records the button on a scrollbar hit, and a release that skipped the
        // write would leave it down forever after the first thumb drag.
        let scrollbar_release = PlatformWindow::end_scrollbar_drag(
            self,
            position,
            button,
            "macos.handle_mouse_up.scrollbar_drag",
        );
        if let Some(result) = scrollbar_release {
            return self.convert_result_with_fanout(result);
        }

        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("macos.handle_mouse_up");

        // Update mouse state - clear appropriate button flag
        match button {
            MouseButton::Left => self.common.mouse_state_mut().left_down = false,
            MouseButton::Right => self.common.mouse_state_mut().right_down = false,
            MouseButton::Middle => self.common.mouse_state_mut().middle_down = false,
            _ => {}
        }

        // Record input sample for gesture detection (button up ends session)
        self.record_input_sample(position, button_to_flags(button), false, true, None);

        // Perform hit testing and update last_hit_test
        self.update_hit_test(position);

        // Feed Wacom/pen state on tablet events (pen tip lifted = not in contact).
        self.feed_tablet_pen(event, position, false);

        // Resolve a right-click context menu, but do NOT present it here: a
        // native menu spins a synchronous nested tracking runloop, and we are
        // holding a `&mut MacOSWindow` reconstructed from the registry's raw
        // pointer. The tick timers run in `kCFRunLoopCommonModes` (deliberately,
        // so blink/tweens survive menu tracking) and would take a SECOND `&mut`
        // to this same window — aliased `&mut` plus logical re-entrancy. The
        // view handler presents `pending_context_menu` after the borrow ends.
        //
        // Returning early here ALSO destroyed the mouse-up delta this handler
        // had just recorded (`right_down = false`), so MouseUp(Right) callbacks
        // never fired on a node that carried a context menu.
        if button == MouseButton::Right {
            if let Some(hit_node) = self.get_first_hovered_node() {
                self.resolve_context_menu(hit_node, position);
            }
        }

        // Use V2 cross-platform event system - automatically detects MouseUp
        let result = self.process_window_events(0);
        self.convert_result_with_fanout(result)
    }

    /// Feed Wacom/pen pressure + tilt into the gesture manager's pen state if
    /// `event` is a tablet event. macOS delivers tablet input as regular mouse
    /// events whose subtype is `TabletPoint` (with pressure/tilt/rotation fields
    /// on the NSEvent). `in_contact` = the pen tip is touching (tip == the mouse
    /// button). This populates the W3C PointerEvent fields (pressure, tilt,
    /// tangentialPressure, twist, eraser) so apps can read `get_pen_state()`;
    /// the cursor + mouse events already drive the pointer. Mirrors the iOS /
    /// Android pen feed.
    fn feed_tablet_pen(&mut self, event: &NSEvent, position: LogicalPosition, in_contact: bool) {
        use objc2_app_kit::{NSEventSubtype, NSPointingDeviceType};
        if unsafe { event.subtype() } != NSEventSubtype::TabletPoint {
            return;
        }
        let pressure = unsafe { event.pressure() };
        let tilt = unsafe { event.tilt() }; // NSPoint components in -1.0..=1.0
        let is_eraser = unsafe { event.pointingDeviceType() } == NSPointingDeviceType::Eraser;
        let tangential = unsafe { event.tangentialPressure() };
        let rotation_deg = unsafe { event.rotation() };
        let device_id = unsafe { event.pointingDeviceID() } as u64;
        if let Some(lw) = self.common.layout_window.as_mut() {
            lw.gesture_drag_manager.update_pen_state_full(
                position,
                pressure,
                // NSEvent tilt is a -1..1 fraction; approximate to degrees (±90°).
                (tilt.x as f32 * 90.0, tilt.y as f32 * 90.0),
                in_contact,
                is_eraser,
                false, // barrel button not reported on the mouse-event path
                device_id,
                tangential,
                rotation_deg * core::f32::consts::PI / 180.0,
                0,
            );
        }
    }

    /// Process a mouse move event.
    pub fn handle_mouse_move(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.common.current_window_state().size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Handle active scrollbar drag (special case - not part of normal event system)
        // Use trait method from PlatformWindow
        if self.common.scrollbar_drag_state.is_some() {
            let result = PlatformWindow::handle_scrollbar_drag(self, position);
            return self.convert_result_with_fanout(result);
        }

        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("macos.handle_mouse_move");

        // Update mouse state
        self.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(position);

        // Record input sample for gesture detection (movement during button press)
        let button_state = if self.common.current_window_state().mouse_state.left_down {
            BUTTON_STATE_LEFT
        } else {
            BUTTON_STATE_NONE
        } | if self.common.current_window_state().mouse_state.right_down {
            BUTTON_STATE_RIGHT
        } else {
            BUTTON_STATE_NONE
        } | if self.common.current_window_state().mouse_state.middle_down {
            BUTTON_STATE_MIDDLE
        } else {
            BUTTON_STATE_NONE
        };
        self.record_input_sample(position, button_state, false, false, None);

        // Update hit test
        self.update_hit_test(position);

        // Feed Wacom/pen state on tablet events (in contact iff the tip/button is down).
        let pen_in_contact = self.common.current_window_state().mouse_state.left_down;
        self.feed_tablet_pen(event, position, pen_in_contact);

        // Update cursor based on CSS cursor properties
        // This is done BEFORE callbacks so callbacks can override the cursor
        if let Some(layout_window) = self.common.layout_window.as_ref() {
            if let Some(hit_test) = layout_window
                .hover_manager
                .get_current(&InputPointId::Mouse)
            {
                let cursor_test = layout_window.compute_cursor_type_hit_test(hit_test);
                // Update the window state cursor type
                self.common.mouse_state_mut().mouse_cursor_type =
                    Some(cursor_test.cursor_icon).into();
                // Set the actual OS cursor
                let cursor_name = self.map_cursor_type_to_macos(cursor_test.cursor_icon);
                self.set_cursor(cursor_name);
            }
        }

        // V2 system will detect MouseOver/MouseEnter/MouseLeave/Drag from state diff
        let result = self.process_window_events(0);
        self.convert_result_with_fanout(result)
    }

    /// Process mouse entered window event.
    pub fn handle_mouse_entered(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.common.current_window_state().size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("macos.handle_mouse_entered");

        // Update mouse state - cursor is now in window
        self.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(position);

        // Update hit test
        self.update_hit_test(position);

        // V2 system will detect MouseEnter events from state diff
        let result = self.process_window_events(0);
        self.convert_result_with_fanout(result)
    }

    /// Process mouse exited window event.
    pub fn handle_mouse_exited(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.common.current_window_state().size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("macos.handle_mouse_exited");

        // Update mouse state - cursor left window
        self.common.mouse_state_mut().cursor_position =
            CursorPosition::OutOfWindow(position);

        // Clear last hit test since mouse is out
        if let Some(ref mut layout_window) = self.common.layout_window {
            layout_window
                .hover_manager
                .push_hit_test(InputPointId::Mouse, FullHitTest::empty(None));
        }

        // V2 system will detect MouseLeave events from state diff
        let result = self.process_window_events(0);
        self.convert_result_with_fanout(result)
    }

    /// Process a scroll wheel event.
    pub fn handle_scroll_wheel(&mut self, event: &NSEvent) -> EventProcessResult {
        // scrollingDeltaX/Y and hasPreciseScrollingDeltas are macOS 10.7+.
        // On pre-10.7, fall back to deltaX/Y (which return discrete wheel ticks).
        let has_modern_scroll = unsafe {
            use objc2::sel;
            let sel = sel!(scrollingDeltaX);
            objc2::msg_send![event, respondsToSelector: sel]
        };
        let (raw_delta_x, raw_delta_y, has_precise) = if has_modern_scroll {
            let dx = unsafe { event.scrollingDeltaX() };
            let dy = unsafe { event.scrollingDeltaY() };
            let precise = unsafe { event.hasPreciseScrollingDeltas() };
            (dx, dy, precise)
        } else {
            // Pre-10.7: use legacy deltaX/deltaY (discrete wheel events)
            let dx = unsafe { event.deltaX() };
            let dy = unsafe { event.deltaY() };
            (dx, dy, false)
        };

        // Trackpad/precise deltas are already pixels; a ratcheting wheel (and the
        // pre-10.7 fallback) reports LINES and must be scaled to match X11/Win32.
        use crate::desktop::shell2::common::event::discrete_scroll_delta_to_pixels;
        let delta_x = discrete_scroll_delta_to_pixels(raw_delta_x, has_precise);
        let delta_y = discrete_scroll_delta_to_pixels(raw_delta_y, has_precise);

        let location = unsafe { event.locationInWindow() };
        let window_height = self.common.current_window_state().size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("macos.handle_scroll_wheel");

        // Update hit test FIRST (required for scroll manager)
        self.update_hit_test(position);

        // Trace the scroll-target resolution: the wheel only scrolls when the
        // hover hit test exposes scroll_hit_test_nodes under the cursor.
        if let Some(layout_window) = self.get_layout_window_mut() {
            use azul_layout::managers::hover::InputPointId;
            let (hovered, scrollable) = layout_window
                .hover_manager
                .get_current(&InputPointId::Mouse)
                .map(|ht| {
                    (
                        ht.hovered_nodes
                            .values()
                            .map(|h| h.regular_hit_test_nodes.len())
                            .sum::<usize>(),
                        ht.hovered_nodes
                            .values()
                            .map(|h| h.scroll_hit_test_nodes.len())
                            .sum::<usize>(),
                    )
                })
                .unwrap_or((0, 0));
            crate::log_debug!(
                crate::desktop::shell2::common::debug_server::LogCategory::Input,
                "[scrollWheel] hit test at ({:.1},{:.1}): {} hovered, {} scrollable",
                position.x, position.y, hovered, scrollable
            );
        }

        // Determine scroll input source based on event phase and precision.
        // Trackpad gestures have hasPreciseScrollingDeltas() == true.
        // When the gesture ends (fingers lifted), we send TrackpadEnd
        // so the scroll timer can trigger spring-back for rubber-banding.
        let source = {
            use azul_layout::managers::scroll_state::ScrollInputSource;
            if has_precise {
                let phase = unsafe { event.phase() };
                let momentum_phase = unsafe { event.momentumPhase() };
                // momentumPhase == Cancelled means the momentum scroll was
                // interrupted (a finger landed on the trackpad). Like Ended it
                // is the LAST event of the gesture and carries a zero delta, so
                // omitting it left the physics timer in TrackpadContinuous and
                // the rubber-band spring-back was never triggered.
                if phase == objc2_app_kit::NSEventPhase::Ended
                    || phase == objc2_app_kit::NSEventPhase::Cancelled
                    || momentum_phase == objc2_app_kit::NSEventPhase::Ended
                    || momentum_phase == objc2_app_kit::NSEventPhase::Cancelled
                {
                    ScrollInputSource::TrackpadEnd
                } else if momentum_phase == objc2_app_kit::NSEventPhase::Began
                    || momentum_phase == objc2_app_kit::NSEventPhase::Changed
                {
                    // The fingers are up: these deltas are the OS's momentum
                    // tail. Classified apart so the physics timer can let the
                    // rubber-band spring own an edge instead of being killed
                    // by every one of them (the user never saw a spring-back).
                    ScrollInputSource::TrackpadMomentum
                } else {
                    ScrollInputSource::TrackpadContinuous
                }
            } else {
                ScrollInputSource::WheelDiscrete
            }
        };

        // Queue scroll input for the physics timer instead of directly setting offsets.
        // The timer will consume these via ScrollInputQueue and push CallbackChange::ScrollTo.
        //
        // TrackpadEnd must pass this gate even though the phase-Ended /
        // momentum-Ended NSEvent carries a ZERO delta: it is the signal the
        // physics timer needs to run rubber-band spring-back after an
        // overshoot. Gating it behind the delta threshold dropped it on every
        // gesture end (the Wayland backend sends the equivalent explicit
        // 0-delta TrackpadEnd from wl_pointer.axis_stop for the same reason).
        let is_trackpad_end = source
            == azul_layout::managers::scroll_state::ScrollInputSource::TrackpadEnd;
        if delta_x.abs() > 0.01 || delta_y.abs() > 0.01 || is_trackpad_end {
            let mut should_start_timer = false;
            let mut input_queue_clone = None;

            if let Some(layout_window) = self.get_layout_window_mut() {
                use azul_core::task::Instant;
                use azul_layout::managers::hover::InputPointId;

                let now = Instant::from(std::time::Instant::now());

                if let Some((_dom_id, _node_id, start_timer)) =
                    layout_window.scroll_manager.record_scroll_from_hit_test(
                        // Raw delta; direction sign applied centrally in
                        // ScrollManager (natural-scroll flag). NOTE: macOS already
                        // applies the system natural-scroll setting to scrollingDelta
                        // before we see it, so the flag stays at its default here.
                        delta_x as f32,
                        delta_y as f32,
                        source,
                        // Provenance: the phase-classified source already
                        // tells wheel and trackpad apart on macOS (trackpad
                        // gestures carry phases; a ratcheting wheel doesn't).
                        match source {
                            azul_layout::managers::scroll_state::ScrollInputSource::TrackpadContinuous
                            | azul_layout::managers::scroll_state::ScrollInputSource::TrackpadMomentum
                            | azul_layout::managers::scroll_state::ScrollInputSource::TrackpadEnd => {
                                azul_layout::managers::scroll_state::ScrollInputDevice::Touchpad
                            }
                            _ => azul_layout::managers::scroll_state::ScrollInputDevice::MouseWheel,
                        },
                        &layout_window.hover_manager,
                        &InputPointId::Mouse,
                        now,
                    )
                {
                    crate::log_debug!(
                        crate::desktop::shell2::common::debug_server::LogCategory::Input,
                        "[scrollWheel] queued for node {:?}/{:?}, start_timer={}",
                        _dom_id, _node_id, start_timer
                    );
                    // GUARD: `start_timer` only means "the input queue was drained
                    // when this event arrived", which the 16 ms physics tick
                    // makes true for almost every event of a gesture. Without
                    // also checking that the timer is not already registered,
                    // `start_timer` below REPLACED the live `ScrollPhysicsState`
                    // — throwing away velocity, animate targets and pending
                    // positions mid-gesture, and resetting the tick phase. The
                    // shared arming site in `common/event.rs` has always had
                    // this check.
                    should_start_timer = start_timer
                        && !layout_window
                            .timers
                            .contains_key(&azul_core::task::SCROLL_MOMENTUM_TIMER_ID);
                    if start_timer {
                        input_queue_clone = Some(
                            layout_window.scroll_manager.get_input_queue()
                        );
                    }
                }
            }

            // Start the scroll momentum timer if this is the first input
            // (must be done outside the borrow of layout_window)
            if should_start_timer {
                if let Some(queue) = input_queue_clone {
                    use azul_core::task::{TimerId, SCROLL_MOMENTUM_TIMER_ID};
                    use azul_layout::scroll_timer::{ScrollPhysicsState, scroll_physics_timer_callback};
                    use azul_layout::timer::{Timer, TimerCallbackType};
                    use azul_core::refany::RefAny;
                    use azul_core::task::Duration;

                    let physics_state = ScrollPhysicsState::new(queue, self.common.system_style.scroll_physics.clone());
                    let interval_ms = self.common.system_style.scroll_physics.timer_interval_ms;
                    let data = RefAny::new(physics_state);
                    let timer = Timer::create(
                        data,
                        scroll_physics_timer_callback as TimerCallbackType,
                        azul_layout::callbacks::ExternalSystemCallbacks::rust_internal()
                            .get_system_time_fn,
                    )
                    .with_interval(Duration::System(
                        azul_core::task::SystemTimeDiff::from_millis(interval_ms as u64),
                    ));

                    self.start_timer(SCROLL_MOMENTUM_TIMER_ID.id, timer);
                }
            }
        }

        // V2 system will detect Scroll event from ScrollManager state
        let result = self.process_window_events(0);
        self.convert_result_with_fanout(result)
    }

    /// Process a key down event.
    pub fn handle_key_down(&mut self, event: &NSEvent) -> EventProcessResult {
        let key_code = unsafe { event.keyCode() };
        let modifiers = unsafe { event.modifierFlags() };

        // Extract Unicode character from event
        let character = unsafe {
            event.characters().and_then(|s| {
                let s_str = s.to_string();
                s_str.chars().next()
            })
        };

        let focused = self.common.layout_window.as_ref()
            .and_then(|lw| lw.focus_manager.get_focused_node().copied());
        crate::log_debug!(
            crate::desktop::shell2::common::debug_server::LogCategory::Input,
            "[handle_key_down] keyCode={}, char={:?}, cmd={}, ctrl={}, focused={:?}",
            key_code,
            character,
            modifiers.contains(objc2_app_kit::NSEventModifierFlags::Command),
            modifiers.contains(objc2_app_kit::NSEventModifierFlags::Control),
            focused
        );

        // Save previous state BEFORE making changes.
        // For key repeats (holding a key down), macOS sends repeated keyDown events.
        // To ensure the state-diff system detects each repeat as a new KeyDown,
        // temporarily clear current_virtual_keycode in the snapshot so the diff
        // sees None → Some(key) instead of Some(key) → Some(key).
        let is_repeat = unsafe { event.isARepeat() };
        let mut prev_snapshot = self.common.current_window_state().clone();
        if is_repeat {
            prev_snapshot.keyboard_state.current_virtual_keycode =
                azul_core::window::OptionVirtualKeyCode::None;
        }
        self.set_previous_window_state(prev_snapshot);

        // RECORD (do not apply) the text this key produces, BEFORE the pass —
        // the same order X11 and Wayland use. `record_text_input` only stages a
        // pending changeset in the TextInputManager; the pass dispatches
        // VirtualKeyDown first and the shared post-callback filter then emits
        // `ApplyPendingTextInput` → `ApplyTextChangeset` →
        // `ScrollCursorIntoViewAfterTextInput`, but ONLY when no callback called
        // `prevent_default()`.
        //
        // This used to call `handle_text_input()` — which takes its OWN
        // `previous = current.clone()` snapshot — AFTER the keycode had already
        // been written into `current`. That left `previous == current`, so the
        // None→Some(key) diff was gone and NO VirtualKeyDown ever fired for a
        // printable key (Shift+letter included); only control chars, PUA
        // function keys and Cmd/Ctrl combos, which skip the branch, kept theirs.
        // It also inserted the text BEFORE dispatch, so a KeyDown handler could
        // never suppress the insertion.
        //
        // Note: interpretKeyEvents → insertText: does NOT work reliably with objc2's
        // protocol conformance. The insertText:replacementRange: method may not be called
        // by the ObjC runtime. So we stage text here for printable characters.
        // Control characters and modified keys (Cmd+X, Ctrl+C) are NOT inserted as text.
        if let Some(ch) = character {
            let is_control_char = ch.is_control();
            let has_cmd = modifiers.contains(objc2_app_kit::NSEventModifierFlags::Command);
            let has_ctrl = modifiers.contains(objc2_app_kit::NSEventModifierFlags::Control);
            // macOS function keys (arrows, F1-F12, etc.) produce Unicode chars in the
            // Private Use Area (U+F700-U+F7FF). These are NOT is_control() but must
            // not be inserted as text — they are navigation/action keys.
            let is_function_key = ('\u{F700}'..='\u{F7FF}').contains(&ch);

            if !is_control_char && !is_function_key && !has_cmd && !has_ctrl {
                let text_input = ch.to_string();
                crate::log_debug!(
                    crate::desktop::shell2::common::debug_server::LogCategory::Input,
                    "[handle_key_down] recording text '{}' for this pass", text_input
                );
                if let Some(layout_window) = self.get_layout_window_mut() {
                    layout_window.record_text_input(&text_input);
                }
            }
        }

        // Update keyboard state with keycode
        self.update_keyboard_state(key_code, modifiers, true);

        // V2 system will detect VirtualKeyDown from state diff
        let result = self.process_window_events(0);
        crate::log_debug!(
            crate::desktop::shell2::common::debug_server::LogCategory::Input,
            "[handle_key_down] process_window_events result={:?}", result
        );
        self.convert_result_with_fanout(result)
    }

    /// Process a key up event.
    pub fn handle_key_up(&mut self, event: &NSEvent) -> EventProcessResult {
        let key_code = unsafe { event.keyCode() };
        let modifiers = unsafe { event.modifierFlags() };

        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("macos.handle_key_up");

        // Update keyboard state
        self.update_keyboard_state(key_code, modifiers, false);

        // V2 system will detect VirtualKeyUp from state diff
        let result = self.process_window_events(0);
        self.convert_result_with_fanout(result)
    }

    /// Process text input from IME (called from insertText:replacementRange:)
    ///
    /// This is the proper way to handle text input on macOS, as it respects
    /// the IME composition system for non-ASCII characters (accents, CJK, etc.)
    ///
    /// Routed through the SHARED `CallbackChange::CreateTextInput` arm (the same
    /// one the headless runner and the debug server use) instead of a bespoke
    /// re-implementation: the local copy dispatched the synthetic Input events
    /// and applied the changeset correctly but omitted the arm's closing
    /// `scroll_selection_into_view(Cursor, Instant)`, so typing ran off the
    /// bottom of a scrollable text node without ever following the caret.
    ///
    /// NOT used by the key-down path any more — `handle_key_down` stages the
    /// text with `record_text_input` and lets its own pass apply it, which is
    /// what makes a KeyDown callback able to `prevent_default()` the insertion.
    pub fn handle_text_input(&mut self, text: &str) {
        use azul_layout::callbacks::CallbackChange;

        let focused = self.common.layout_window.as_ref()
            .and_then(|lw| lw.focus_manager.get_focused_node().copied());
        let has_cursor = self.common.layout_window.as_ref()
            .map(|lw| lw.text_edit_manager.has_active_editing())
            .unwrap_or(false);
        crate::log_debug!(
            crate::desktop::shell2::common::debug_server::LogCategory::Input,
            "[handle_text_input] text='{}', focused={:?}, has_cursor={}", text, focused, has_cursor
        );

        // Save previous state BEFORE making changes: the arm dispatches its own
        // synthetic Input events and a user callback inside that dispatch may
        // re-enter `process_window_events`, which needs a baseline.
        self.snapshot_window_state_baseline("macos.handle_text_input");

        let result = self.apply_user_change(&CallbackChange::CreateTextInput {
            text: text.into(),
        });

        // Apply the result: text edits go through the incremental path
        // (display_list_dirty / incremental_relayout), NOT the full DOM rebuild
        // path (a regeneration request). The display list was already regenerated
        // inside apply_text_changeset() → update_text_cache_after_edit() →
        // regenerate_display_list_for_dom().
        let event_result = self.convert_result_with_fanout(result);
        match event_result {
            EventProcessResult::RegenerateDisplayList => {
                self.common.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
                self.request_redraw();
            }
            EventProcessResult::UpdateDisplayList => {
                self.common.display_list_dirty = true;
                self.request_redraw();
            }
            // A text edit that changed the line count returns
            // ShouldIncrementalRelayout. This arm was MISSING — it fell into
            // `_ => {}`, so a reflowing edit ran no relayout and requested no
            // frame at all and the window kept showing the pre-edit layout.
            EventProcessResult::RegenerateLayoutIncremental => {
                self.apply_incremental_relayout_result();
            }
            EventProcessResult::RequestRedraw => {
                self.request_redraw();
            }
            _ => {}
        }
    }

    /// Process a flags changed event (modifier keys).
    ///
    /// A `flagsChanged:` event carries the keyCode of the ONE physical key whose
    /// state flipped, so that is what decides which `VirtualKeyCode` to report —
    /// the previous version only ever synthesized the `L*` twins from the
    /// device-independent class flags, so a physical RShift/RAlt/RControl/RCmd
    /// arrived as its left counterpart and Caps Lock produced nothing at all.
    pub fn handle_flags_changed(&mut self, event: &NSEvent) -> EventProcessResult {
        let modifiers = unsafe { event.modifierFlags() };
        let key_code = unsafe { event.keyCode() };
        let raw = modifiers.0 as usize;

        // Is the key that changed now DOWN? Read its device-DEPENDENT bit, not
        // the class flag: releasing one Shift while the other is held leaves the
        // `Shift` class flag set. Caps Lock is a LOCK — the class flag IS its
        // state and the keyboard sends a flagsChanged on each toggle.
        let is_down = match key_code {
            MACOS_KEYCODE_LSHIFT => raw & NX_DEVICE_LSHIFT != 0,
            MACOS_KEYCODE_RSHIFT => raw & NX_DEVICE_RSHIFT != 0,
            MACOS_KEYCODE_LCONTROL => raw & NX_DEVICE_LCONTROL != 0,
            MACOS_KEYCODE_RCONTROL => raw & NX_DEVICE_RCONTROL != 0,
            MACOS_KEYCODE_LALT => raw & NX_DEVICE_LALT != 0,
            MACOS_KEYCODE_RALT => raw & NX_DEVICE_RALT != 0,
            MACOS_KEYCODE_LWIN => raw & NX_DEVICE_LWIN != 0,
            MACOS_KEYCODE_RWIN => raw & NX_DEVICE_RWIN != 0,
            MACOS_KEYCODE_CAPSLOCK => modifiers.contains(NSEventModifierFlags::CapsLock),
            // Fn and anything else this table doesn't map: no engine key.
            _ => return EventProcessResult::DoNothing,
        };

        // Nothing to dispatch if the engine already agrees with the OS (a
        // duplicate flagsChanged, or a modifier we re-synced on focus).
        let Some(vk) = self.convert_keycode(key_code) else {
            return EventProcessResult::DoNothing;
        };
        if self.common.current_window_state().keyboard_state.is_key_down(vk) == is_down {
            return EventProcessResult::DoNothing;
        }

        // Save previous state BEFORE making changes: this handler used to mutate
        // in place and rely on the next handler's snapshot, which destroyed the
        // very transition it had just produced.
        self.snapshot_window_state_baseline("macos.handle_flags_changed");

        self.update_keyboard_state(key_code, modifiers, is_down);

        let result = self.process_window_events(0);
        self.convert_result_with_fanout(result)
    }

    /// Process a window resize event.
    pub fn handle_resize(&mut self, new_width: f64, new_height: f64) -> EventProcessResult {
        use azul_core::geom::LogicalSize;

        let new_size = LogicalSize {
            width: new_width as f32,
            height: new_height as f32,
        };

        // Store old context for comparison
        let old_context = self.dynamic_selector_context.clone();

        // Save previous state BEFORE making changes. The size delta is what the
        // shared pass turns into a `WindowResize` dispatch — this handler used to
        // mutate in place with no snapshot and no pass, so the delta simply sat
        // there until the NEXT handler's snapshot overwrote it and the resize
        // was never dispatched to any callback.
        self.snapshot_window_state_baseline("macos.handle_resize");

        // Update window state. Size REPORTED by the OS (source = Os): the
        // OS-sync baseline must advance in lockstep, or the next
        // `sync_window_state()` diffs a stale baseline against the new size and
        // echoes a `setContentSize` back at AppKit — mid-live-resize that echo
        // carries the PREVIOUS frame's size and fights the user's drag.
        self.common.update_window_state(
            crate::desktop::shell2::common::event::WindowStateSource::Os,
            |ws| ws.size.dimensions = new_size,
        );

        // Update dynamic selector context with new viewport dimensions
        self.dynamic_selector_context.viewport_width = new_width as f32;
        self.dynamic_selector_context.viewport_height = new_height as f32;
        self.dynamic_selector_context.orientation = if new_width > new_height {
            azul_css::dynamic_selector::OrientationType::Landscape
        } else {
            azul_css::dynamic_selector::OrientationType::Portrait
        };

        // Check if DPI changed (window may have moved to different display)
        let current_hidpi = self.get_hidpi_factor();
        let old_hidpi = self.common.current_window_state().size.get_hidpi_factor();

        if (current_hidpi.inner.get() - old_hidpi.inner.get()).abs() > 0.001 {
            log_info!(
                LogCategory::Window,
                "[Resize] DPI changed: {} -> {}",
                old_hidpi.inner.get(),
                current_hidpi.inner.get()
            );
            // Same Os-source routing as the dimension write above (the scale is
            // an OS fact; keep the OS-sync baseline in lockstep).
            let new_dpi = (current_hidpi.inner.get() * 96.0) as u32;
            self.common.update_window_state(
                crate::desktop::shell2::common::event::WindowStateSource::Os,
                |ws| ws.size.dpi = new_dpi,
            );
        }

        // Notify compositor of resize (this is private in mod.rs, so we inline it here)
        if let Err(e) = self.handle_compositor_resize() {
            log_error!(LogCategory::Rendering, "Compositor resize failed: {}", e);
        }

        // Check if viewport dimensions actually changed (debounce rapid resize events)
        let viewport_changed =
            (old_context.viewport_width - self.dynamic_selector_context.viewport_width).abs() > 0.5
                || (old_context.viewport_height - self.dynamic_selector_context.viewport_height)
                    .abs()
                    > 0.5;

        if !viewport_changed {
            // No significant change, just update compositor. The pass still runs
            // so the (sub-pixel) delta is consumed here instead of being left
            // live for an unrelated later pass to re-detect as a resize.
            let result = self.process_window_events(0);
            return self
                .convert_result_with_fanout(result)
                .max(EventProcessResult::RequestRedraw);
        }

        // RESIZE POLICY (user ruling 2026-08-08, same as Wayland/X11/Win32):
        // a live-resize drag delivers one windowDidResize per frame, and the
        // app's layout() is only re-invoked when a recorded window-size
        // query answer flips or a CSS breakpoint / orientation is crossed —
        // `request_regeneration_for_resize` folds all three signals. This
        // replaces "the platform path requests a full DOM rebuild on every
        // resize so that window-size checks always reflect the live size":
        // the recorded-query channel exists precisely so the engine can tell
        // WHICH sizes the callback cares about, instead of assuming all of
        // them.
        let old_logical = azul_core::geom::LogicalSize::new(
            old_context.viewport_width,
            old_context.viewport_height,
        );
        let new_logical = azul_core::geom::LogicalSize::new(
            self.dynamic_selector_context.viewport_width,
            self.dynamic_selector_context.viewport_height,
        );
        let full = self
            .common
            .request_regeneration_for_resize(old_logical, new_logical);
        if full {
            log_debug!(
                LogCategory::Layout,
                "[Resize] boundary crossed: {}x{} -> {}x{} — re-running layout()",
                old_context.viewport_width,
                old_context.viewport_height,
                self.dynamic_selector_context.viewport_width,
                self.dynamic_selector_context.viewport_height
            );
        }

        // Dispatch the size transition (WindowResize + any size-conditional
        // restyle) and consume the delta.
        let result = self.process_window_events(0);

        // Either way the compositor needs a frame; the caller's RequestRedraw
        // arm sets surface_needs_update + request_redraw, and the fast path's
        // latch is consumed (once, coalesced) at the top of build_atomic_txn —
        // so the pass result is floored at RequestRedraw, never below it.
        self.convert_result_with_fanout(result)
            .max(EventProcessResult::RequestRedraw)
    }

    /// Process a file drop event (the user released the dragged files over the
    /// window — emits `EventType::FileDrop`).
    ///
    /// The `NSDraggingDestination` delegate IS wired in `macos/mod.rs` on both
    /// `GLView` and `CPUView` (registerForDraggedTypes at view creation;
    /// draggingEntered:/draggingUpdated:/draggingExited:/performDragOperation:
    /// forward here) — a stale TODO here previously claimed otherwise
    /// (MWA-C-file_drop). The three `handle_file_*` methods below own all
    /// manager mutation + one-shot clearing, so the delegate only extracts
    /// paths + the drag location and forwards.
    pub fn handle_file_drop(
        &mut self,
        position: LogicalPosition,
        paths: Vec<String>,
    ) -> EventProcessResult {
        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("macos.handle_file_drop");
        // MWA-B7: the OS drag location is the ONLY fresh position — no
        // mouse-move events arrive during an OS drag, so the cached cursor
        // is stale (wherever the pointer was before the drag started).
        self.common.mouse_state_mut().cursor_position =
            CursorPosition::InWindow(position);

        // Update cursor manager with dropped file
        if !paths.is_empty() {
            if let Some(layout_window) = self.common.layout_window.as_mut() {
                // MWA-B7: pass EVERY path — multi-file drops were silently
                // truncated to the first file at this ingress.
                layout_window
                    .file_drop_manager
                    .set_dropped_files(paths.iter().map(|p| p.clone().into()).collect());
            }
        }

        // Update hit test at the OS-provided drop location (MWA-B7).
        self.update_hit_test(position);

        // V2 system will detect FileDrop event from state diff
        let result = self.process_window_events(0);

        // Clear dropped file after processing (one-shot event)
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.file_drop_manager.set_dropped_file(None);
        }

        self.convert_result_with_fanout(result)
    }

    /// Process a file drag entering / moving over the window (emits
    /// `EventType::FileHover`). Called by the `NSDraggingDestination`
    /// `draggingEntered:` / `draggingUpdated:` delegate (see `handle_file_drop`).
    pub fn handle_file_drag_entered(
        &mut self,
        position: LogicalPosition,
        paths: Vec<String>,
    ) -> EventProcessResult {
        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("macos.handle_file_drag_entered");

        // Record ALL hovered files (MWA-B7).
        if !paths.is_empty() {
            if let Some(layout_window) = self.common.layout_window.as_mut() {
                // MWA-B7: pass EVERY path — multi-file drops were silently
                // truncated to the first file at this ingress.
                layout_window
                    .file_drop_manager
                    .set_hovered_files(paths.iter().map(|p| p.clone().into()).collect());
            }
        }

        // Update hit test at the OS-provided drag location (MWA-B7).
        self.common.mouse_state_mut().cursor_position =
            CursorPosition::InWindow(position);
        self.update_hit_test(position);

        // V2 system detects FileHover from the file_drop_manager state.
        let result = self.process_window_events(0);
        self.convert_result_with_fanout(result)
    }

    /// Process a file drag leaving the window without a drop (emits
    /// `EventType::FileHoverCancel`). Called by the `NSDraggingDestination`
    /// `draggingExited:` delegate (see `handle_file_drop`).
    pub fn handle_file_drag_exited(&mut self) -> EventProcessResult {
        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("macos.handle_file_drag_exited");

        // Clear the hovered file; the Some -> None transition latches the
        // one-shot hover-cancel flag so FileHoverCancel can fire this pass.
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.file_drop_manager.set_hovered_file(None);
        }

        // V2 system detects FileHoverCancel from the latched flag.
        let result = self.process_window_events(0);

        // Clear the one-shot hover-cancel flag after processing.
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.file_drop_manager.clear_hover_cancelled();
        }

        self.convert_result_with_fanout(result)
    }

    /// Convert macOS keycode to VirtualKeyCode.
    fn convert_keycode(&self, keycode: u16) -> Option<VirtualKeyCode> {
        convert_keycode(keycode)
    }
}

impl MacOSWindow {
    /// Update keyboard state from event.
    fn update_keyboard_state(
        &mut self,
        keycode: u16,
        modifiers: NSEventModifierFlags,
        is_down: bool,
    ) {
        use azul_core::window::VirtualKeyCode;

        // Convert keycode to VirtualKeyCode first (before borrowing)
        let vk = match self.convert_keycode(keycode) {
            Some(k) => k,
            None => return,
        };

        let keyboard_state = self.common.keyboard_state_mut();

        if is_down {
            // Add to pressed keys if not already present
            let mut already_pressed = false;
            for pressed_key in keyboard_state.pressed_virtual_keycodes.as_ref() {
                if *pressed_key == vk {
                    already_pressed = true;
                    break;
                }
            }
            if !already_pressed {
                // Convert to Vec, add, convert back
                let mut pressed_vec: Vec<VirtualKeyCode> =
                    keyboard_state.pressed_virtual_keycodes.as_ref().to_vec();
                pressed_vec.push(vk);
                keyboard_state.pressed_virtual_keycodes =
                    azul_core::window::VirtualKeyCodeVec::from_vec(pressed_vec);
            }
            keyboard_state.current_virtual_keycode =
                azul_core::window::OptionVirtualKeyCode::Some(vk);
        } else {
            // Remove from pressed keys
            let pressed_vec: Vec<VirtualKeyCode> = keyboard_state
                .pressed_virtual_keycodes
                .as_ref()
                .iter()
                .copied()
                .filter(|k| *k != vk)
                .collect();
            keyboard_state.pressed_virtual_keycodes =
                azul_core::window::VirtualKeyCodeVec::from_vec(pressed_vec);
            keyboard_state.current_virtual_keycode = azul_core::window::OptionVirtualKeyCode::None;
        }
    }

    /// Handle compositor resize notification.
    fn handle_compositor_resize(&mut self) -> Result<(), String> {
        use webrender::api::units::{DeviceIntRect, DeviceIntSize, DevicePixelScale};

        // Get new physical size
        let physical_size = self.common.current_window_state().size.get_physical_size();
        let new_size = DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
        let hidpi_factor = self.common.current_window_state().size.get_hidpi_factor();

        // Update WebRender document size
        let mut txn = webrender::Transaction::new();
        let device_rect = DeviceIntRect::from_size(new_size);
        // NOTE: azul_layout outputs coordinates in CSS pixels (logical pixels).
        txn.set_document_view(device_rect, DevicePixelScale::new(hidpi_factor.inner.get()));

        // Send transaction (GPU backend only — in CPU mode `render_api` is None and
        // the WebRender document does not exist; the CPU framebuffer resize below is
        // what matters there. This guard is C1: unconditional unwrap here aborted every
        // macOS demo on the first resize when running on the CPU backend.)
        if let Some(ref layout_window) = self.common.layout_window {
            if let Some(render_api) = self.common.render_api.as_mut() {
                let document_id = crate::desktop::wr_translate2::wr_translate_document_id(
                    layout_window.document_id,
                );
                crate::plog_trace!("[compositor] macOS resize: sending WebRender set_document_view txn");
                render_api.send_transaction(document_id, txn);
            } else {
                crate::plog_trace!("[compositor] macOS resize: CPU backend, skipping WebRender txn");
            }
        }

        // Resize GL viewport (if OpenGL backend)
        if let Some(ref gl_context) = self.gl_context {
            // Make context current
            unsafe {
                gl_context.makeCurrentContext();
            }

            // Resize viewport
            if let Some(ref gl) = self.gl_functions {
                use azul_core::gl as gl_types;
                gl.functions.viewport(
                    0,
                    0,
                    physical_size.width as gl_types::GLint,
                    physical_size.height as gl_types::GLint,
                );
            }
        }

        // Resize CPU framebuffer if using CPU backend
        if let Some(cpu_view) = &self.cpu_view {
            unsafe {
                // Force the CPU view to resize its framebuffer on next draw
                // The actual resize happens in CPUView::drawRect when bounds change
                cpu_view.setNeedsDisplay(true);
            }
        }

        Ok(())
    }

    /// Resolve a context menu for the given node at position and QUEUE it.
    /// Returns Some if a menu was queued, None otherwise.
    ///
    /// Presentation is deliberately not done here — see `pending_context_menu`
    /// and `take_pending_context_menu` in `macos/mod.rs`.
    fn resolve_context_menu(
        &mut self,
        node: HitTestNode,
        position: LogicalPosition,
    ) -> Option<()> {
        use azul_core::dom::DomId;

        let layout_window = self.common.layout_window.as_ref()?;
        let dom_id = DomId {
            inner: node.dom_id as usize,
        };

        // Get layout result for this DOM
        let layout_result = layout_window.layout_results.get(&dom_id)?;

        // Check if this node has a context menu
        let node_id = azul_core::id::NodeId::from_usize(node.node_id as usize)?;
        let binding = layout_result.styled_dom.node_data.as_container();
        let node_data = binding.get(node_id)?;

        // Context menus are stored directly on NodeData. A right-click on a
        // CHILD of the node that carries the menu opens it too (every OS does
        // this) - walk up from the hit node to the first ancestor with one,
        // the same walk the keyboard-accelerator lookup already does.
        let hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let mut current = Some(node_id);
        let mut context_menu = None;
        for _ in 0..256 {
            let Some(n) = current else { break };
            if let Some(menu) = binding.get(n).and_then(azul_core::dom::NodeData::get_context_menu) {
                context_menu = Some(menu.clone());
                break;
            }
            current = hierarchy.get(n).and_then(|h| h.parent_id());
        }
        let context_menu = context_menu?;
        let _ = node_data;

        log_debug!(
            LogCategory::Input,
            "[Context Menu] Queuing context menu at ({}, {}) for node {:?} with {} items",
            position.x,
            position.y,
            node,
            context_menu.items.as_slice().len()
        );

        // Check if native context menus are enabled
        if self.common.current_window_state().flags.use_native_context_menus {
            self.queue_native_context_menu_at_position(&context_menu, position);
        } else {
            self.show_window_based_context_menu(&context_menu, position);
        }

        Some(())
    }

    /// Build the NSMenu for a context menu and park it in `pending_context_menu`
    /// together with the view + view-space point it must pop up at.
    ///
    /// `pub(super)` because `show_menu_from_callback` (in `macos/mod.rs`, the
    /// `info.open_menu()` route) parks through here too — it used to pop the
    /// menu up synchronously while holding `&mut MacOSWindow`.
    pub(super) fn queue_native_context_menu_at_position(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: LogicalPosition,
    ) {
        use objc2_app_kit::NSMenu;
        use objc2_foundation::{MainThreadMarker, NSPoint};

        let mtm = match MainThreadMarker::new() {
            Some(m) => m,
            None => {
                log_warn!(
                    LogCategory::Platform,
                    "[Context Menu] Not on main thread, cannot show menu"
                );
                return;
            }
        };

        let ns_menu = NSMenu::new(mtm);

        // Build menu items recursively from Azul menu structure
        Self::recursive_build_nsmenu(&ns_menu, menu.items.as_slice(), &mtm, &mut self.menu_state);

        // Show the menu at the specified position. `position` is azul-logical
        // (top-left origin, y-down); popUpMenuPositioningItem:atLocation:inView:
        // takes the point in the VIEW's coordinate system, which is BOTTOM-left
        // origin (neither view overrides isFlipped) — flip y once, the inverse
        // of macos_to_azul_coords. Unflipped, the menu opened vertically
        // mirrored (right-click near the top popped the menu near the bottom).
        let window_height = self.common.current_window_state().size.dimensions.height;
        let view_point = NSPoint {
            x: position.x as f64,
            y: (window_height - position.y) as f64,
        };

        // Retain the view: the pending menu outlives this `&mut self` borrow by
        // design, so it cannot hold a reference back into `self`.
        use objc2::Message;
        let view: Option<objc2::rc::Retained<objc2_app_kit::NSView>> =
            if let Some(ref gl_view) = self.gl_view {
                let v: &objc2_app_kit::NSView = &**gl_view;
                Some(v.retain())
            } else {
                self.cpu_view.as_ref().map(|cpu_view| {
                    let v: &objc2_app_kit::NSView = &**cpu_view;
                    v.retain()
                })
            };

        if let Some(view) = view {
            log_debug!(
                LogCategory::Input,
                "[Context Menu] Queued native menu at position ({}, {}) with {} items",
                position.x,
                position.y,
                menu.items.as_slice().len()
            );

            self.pending_context_menu = Some(super::PendingContextMenu {
                menu: ns_menu,
                view,
                location: view_point,
            });
        }
    }

    /// Show a context menu using Azul window-based menu system
    ///
    /// This uses the same unified menu system as regular menus (crate::desktop::menu::show_menu)
    /// but spawns at cursor position instead of below a trigger rect.
    ///
    /// The menu window creation is queued and will be processed in Phase 3 of the event loop.
    fn show_window_based_context_menu(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: LogicalPosition,
    ) {
        // Get parent window position
        let parent_pos = match self.common.current_window_state().position {
            azul_core::window::WindowPosition::Initialized(pos) => {
                LogicalPosition::new(pos.x as f32, pos.y as f32)
            }
            _ => LogicalPosition::new(0.0, 0.0),
        };

        // Create menu window options using the unified menu system
        // This is identical to how menu bar menus work, but with cursor_pos instead of trigger_rect
        let menu_options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.common.system_style.clone(),
            parent_pos,
            None,           // No trigger rect for context menus (they spawn at cursor)
            Some(position), // Cursor position for menu positioning
            None,           // No parent menu
        );

        // Queue window creation request for processing in Phase 3 of the event loop
        // The event loop will create the window with MacOSWindow::new_with_fc_cache()
        log_debug!(
            LogCategory::Window,
            "[macOS] Queuing window-based context menu at screen ({}, {}) - will be created in \
             event loop Phase 3",
            position.x,
            position.y
        );

        self.pending_window_creates.push(menu_options);
    }

    /// Recursively builds an NSMenu from Azul MenuItem array
    ///
    /// Delegates to `menu::build_menu_items` which handles leaf items,
    /// submenus, separators, and MenuItemState.
    pub(crate) fn recursive_build_nsmenu(
        menu: &objc2_app_kit::NSMenu,
        items: &[azul_core::menu::MenuItem],
        mtm: &objc2::MainThreadMarker,
        menu_state: &mut crate::desktop::shell2::macos::menu::MenuState,
    ) {
        let mut command_map = std::collections::HashMap::new();
        let mut next_tag = menu_state.next_tag();
        crate::desktop::shell2::macos::menu::build_menu_items(
            items,
            menu,
            &mut command_map,
            &mut next_tag,
            *mtm,
        );
        menu_state.merge_callbacks(command_map, next_tag);
    }

    // Helper Functions for V2 Event System

    /// Update hit test at given position and store in current_window_state.
    fn update_hit_test(&mut self, position: LogicalPosition) {
        // Delegate to the unified cross-platform hit test in PlatformWindow trait
        use crate::desktop::shell2::common::event::PlatformWindow;
        self.update_hit_test_at(position);
    }

    /// Get the first hovered node from current mouse hit test.
    fn get_first_hovered_node(&self) -> Option<HitTestNode> {
        use azul_layout::managers::hover::InputPointId;
        self.common.layout_window
            .as_ref()?
            .hover_manager
            .get_current(&InputPointId::Mouse)?
            .hovered_nodes
            .iter()
            .flat_map(|(dom_id, ht)| {
                ht.regular_hit_test_nodes
                    .keys()
                    .next()
                    .map(|node_id| HitTestNode {
                        dom_id: dom_id.inner as u64,
                        node_id: node_id.index() as u64,
                    })
            })
            .next()
    }

    // V2 Cross-Platform Event Processing
    // NOTE: All V2 event processing methods are now provided by the
    // PlatformWindow trait in common/event.rs. The trait provides:
    // - process_window_events_v2() - Entry point (public API)
    // - process_window_events() - Recursive processing
    // - dispatch_events_propagated() - W3C Capture→Target→Bubble dispatch
    // - apply_user_change() - Result handling
    // This eliminates ~336 lines of platform-specific duplicated code.
}
