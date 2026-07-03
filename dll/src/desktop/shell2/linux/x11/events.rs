//! X11 Event handling - Cross-platform V2 event system with state-diffing
//!
//! This module implements the same event processing architecture as Windows and macOS:
//! 1. Save previous_window_state before modifying current_window_state
//! 2. Update current_window_state based on X11 events
//! 3. Use create_events_from_states() to detect changes via state diffing
//! 4. Use dispatch_events() to determine which callbacks to invoke
//! 5. Invoke callbacks recursively with depth limit
//! 6. Process callback results (DOM regeneration, window state changes, etc.)
//!
//! Includes full IME (XIM) support for international text input.
//! Also provides `keysym_to_virtual_keycode()` for X11 keysym → VirtualKeyCode mapping (shared with Wayland).

use std::{
    cell::{Cell, RefCell},
    ffi::{CStr, CString, c_char, c_ulong, c_void},
    rc::Rc,
};

use azul_core::{
    callbacks::Update,
    dom::{DomId, NodeId},
    events::{EventFilter, MouseButton, ProcessEventResult},
    geom::{LogicalPosition, PhysicalPosition},
    hit_test::FullHitTest,
    window::{CursorPosition, VirtualKeyCode},
};
use crate::desktop::shell2::common::event::{
    HitTestNode, BUTTON_STATE_LEFT, BUTTON_STATE_RIGHT, BUTTON_STATE_MIDDLE, BUTTON_STATE_NONE,
};
use azul_layout::{
    managers::hover::InputPointId,
};

use super::{defines::*, dlopen::Xlib, X11Window};
use crate::desktop::shell2::common::event::PlatformWindow;

use super::super::super::common::debug_server::LogCategory;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

/// Pixels per discrete X11 scroll tick (button 4/5). X11 scroll events are
/// unitless discrete steps; this constant converts them to pixel deltas for
/// the scroll physics system.
const X11_SCROLL_TICK_PIXELS: f32 = 20.0;

// IME Support (X Input Method)

/// Negotiated XIM input style.
///
/// XIM clients must declare *one* preedit + *one* status style at IC creation
/// time. The choice determines who renders the composition string:
///
/// - `Callbacks`: the app renders preedit inline via XIM draw callbacks. This
///   is what we need to display CJK candidates *inside* the contenteditable.
/// - `OverTheSpot`: the IM renders preedit in a floating window positioned by
///   `XNSpotLocation` (updated from `sync_ime_position_to_os`).
/// - `Rooted`: the IM renders preedit in its own window with no app input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum ImeStyle {
    Callbacks,
    OverTheSpot,
    Rooted,
}

/// Shared state populated by the XIM preedit callbacks and drained from the
/// main event loop. Callbacks fire synchronously inside `XFilterEvent`, on the
/// same thread; `RefCell`/`Cell` is enough — no cross-thread access.
pub(super) struct ImePreeditSink {
    /// Current preedit string. `None` means no active composition.
    pub text: RefCell<Option<String>>,
    /// Caret offset (in characters) within the preedit string.
    pub caret: Cell<i32>,
    /// Set by callbacks, cleared by `ImeManager::drain_preedit`.
    pub dirty: Cell<bool>,
}

impl ImePreeditSink {
    fn new() -> Self {
        Self {
            text: RefCell::new(None),
            caret: Cell::new(0),
            dirty: Cell::new(false),
        }
    }
}

pub(super) struct ImeManager {
    xlib: Rc<Xlib>,
    xim: XIM,
    xic: XIC,
    pub(super) style: ImeStyle,
    /// Boxed so its address is stable across `ImeManager` moves — XIM
    /// callbacks hold a raw pointer to it via `XIMCallback::client_data`.
    sink: Box<ImePreeditSink>,
}

impl ImeManager {
    pub(super) fn new(xlib: &Rc<Xlib>, display: *mut Display, window: Window) -> Option<Self> {
        unsafe {
            // Set the locale. This is crucial for XIM to work correctly.
            let locale = CString::new("").unwrap();
            (xlib.XSetLocaleModifiers)(locale.as_ptr());

            let xim = (xlib.XOpenIM)(
                display,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if xim.is_null() {
                log_warn!(
                    LogCategory::Input,
                    "[X11 IME] Could not open input method. IME will not be available."
                );
                return None;
            }

            // Negotiate the best input style supported by the IM. We prefer
            // on-the-spot (`*Callbacks`) — that's what gives us inline preedit
            // inside the contenteditable — falling through to over-the-spot
            // (`Position`) and finally rooted (`Nothing`) when the IM doesn't
            // advertise the richer style.
            let mut styles_ptr: *mut XIMStyles = std::ptr::null_mut();
            let _ = (xlib.XGetIMValues)(
                xim,
                XN_QUERY_INPUT_STYLE.as_ptr() as *const i8,
                &mut styles_ptr as *mut *mut XIMStyles,
                std::ptr::null::<i8>(),
            );

            let want_callbacks = XIMPreeditCallbacks | XIMStatusCallbacks;
            let want_callbacks_no_status = XIMPreeditCallbacks | XIMStatusNothing;
            let want_over_spot = XIMPreeditPosition | XIMStatusNothing;
            let want_rooted = XIMPreeditNothing | XIMStatusNothing;

            let (chosen_style, style_kind) = if !styles_ptr.is_null() {
                let count = (*styles_ptr).count_styles as usize;
                let supported =
                    std::slice::from_raw_parts((*styles_ptr).supported_styles, count);

                let has = |mask: c_ulong| supported.iter().any(|&s| s == mask);

                let result = if has(want_callbacks) {
                    (want_callbacks, ImeStyle::Callbacks)
                } else if has(want_callbacks_no_status) {
                    (want_callbacks_no_status, ImeStyle::Callbacks)
                } else if has(want_over_spot) {
                    (want_over_spot, ImeStyle::OverTheSpot)
                } else {
                    (want_rooted, ImeStyle::Rooted)
                };

                (xlib.XFree)(styles_ptr as *mut c_void);
                result
            } else {
                // IM did not advertise any styles — fall back to rooted.
                (want_rooted, ImeStyle::Rooted)
            };

            let sink = Box::new(ImePreeditSink::new());

            let xic = match style_kind {
                ImeStyle::Callbacks => {
                    // Build XIMCallback structs pointing at our sink, then
                    // bundle them in a XVaNestedList for XNPreeditAttributes.
                    let sink_ptr = &*sink as *const ImePreeditSink as *mut c_void;
                    let start_cb = XIMCallback {
                        client_data: sink_ptr,
                        callback: Some(preedit_start_cb),
                    };
                    let done_cb = XIMCallback {
                        client_data: sink_ptr,
                        callback: Some(preedit_done_cb),
                    };
                    let draw_cb = XIMCallback {
                        client_data: sink_ptr,
                        callback: Some(preedit_draw_cb),
                    };
                    let caret_cb = XIMCallback {
                        client_data: sink_ptr,
                        callback: Some(preedit_caret_cb),
                    };

                    let preedit_attrs = (xlib.XVaCreateNestedList)(
                        0,
                        XN_PREEDIT_START_CALLBACK.as_ptr() as *const i8,
                        &start_cb as *const XIMCallback,
                        XN_PREEDIT_DONE_CALLBACK.as_ptr() as *const i8,
                        &done_cb as *const XIMCallback,
                        XN_PREEDIT_DRAW_CALLBACK.as_ptr() as *const i8,
                        &draw_cb as *const XIMCallback,
                        XN_PREEDIT_CARET_CALLBACK.as_ptr() as *const i8,
                        &caret_cb as *const XIMCallback,
                        std::ptr::null::<i8>(),
                    );

                    let xic = (xlib.XCreateIC)(
                        xim,
                        XN_INPUT_STYLE.as_ptr() as *const i8,
                        chosen_style,
                        XN_CLIENT_WINDOW.as_ptr() as *const i8,
                        window,
                        XN_FOCUS_WINDOW.as_ptr() as *const i8,
                        window,
                        XN_PREEDIT_ATTRIBUTES.as_ptr() as *const i8,
                        preedit_attrs,
                        std::ptr::null::<i8>(),
                    );

                    // XVaCreateNestedList allocates with Xmalloc — free it.
                    if !preedit_attrs.is_null() {
                        (xlib.XFree)(preedit_attrs);
                    }

                    xic
                }
                ImeStyle::OverTheSpot | ImeStyle::Rooted => (xlib.XCreateIC)(
                    xim,
                    XN_INPUT_STYLE.as_ptr() as *const i8,
                    chosen_style,
                    XN_CLIENT_WINDOW.as_ptr() as *const i8,
                    window,
                    XN_FOCUS_WINDOW.as_ptr() as *const i8,
                    window,
                    std::ptr::null::<i8>(),
                ),
            };

            if xic.is_null() {
                log_warn!(
                    LogCategory::Input,
                    "[X11 IME] XCreateIC failed for style {:?}; IME unavailable.",
                    style_kind
                );
                (xlib.XCloseIM)(xim);
                return None;
            }

            (xlib.XSetICFocus)(xic);

            log_debug!(
                LogCategory::Input,
                "[X11 IME] Initialized with style {:?}",
                style_kind
            );

            Some(Self {
                xlib: xlib.clone(),
                xim,
                xic,
                style: style_kind,
                sink,
            })
        }
    }

    /// Get the XIC (X Input Context) for setting IME properties
    pub(super) fn get_xic(&self) -> XIC {
        self.xic
    }

    /// True when the negotiated style is `OverTheSpot`: callers should push
    /// `XNSpotLocation` updates on caret moves so the IM can position its
    /// candidate window.
    pub(super) fn wants_spot_location_updates(&self) -> bool {
        matches!(self.style, ImeStyle::OverTheSpot)
    }

    /// Drain any pending preedit update produced by the XIM callbacks since
    /// the last call. Returns `Some((text, caret))` if state changed,
    /// otherwise `None`. `text == None` means composition ended.
    pub(super) fn drain_preedit(&self) -> Option<(Option<String>, i32)> {
        if !self.sink.dirty.get() {
            return None;
        }
        self.sink.dirty.set(false);
        let text = self.sink.text.borrow().clone();
        Some((text, self.sink.caret.get()))
    }

    /// Filters an event through the IME.
    /// Returns `true` if the event was consumed by the IME.
    pub(super) fn filter_event(&self, event: &mut XEvent) -> bool {
        unsafe { (self.xlib.XFilterEvent)(event, 0) != 0 }
    }

    /// Translates a key event into a character and a keysym, considering the IME.
    pub(super) fn lookup_string(&self, event: &mut XKeyEvent) -> (Option<String>, Option<KeySym>) {
        let mut keysym: KeySym = 0;
        let mut status: i32 = 0;
        let mut buffer: [c_char; 32] = [0; 32];

        let count = unsafe {
            // Xutf8LookupString (not XmbLookupString): the committed bytes are
            // guaranteed UTF-8 regardless of the locale codeset, so accented and
            // CJK commit strings decode correctly even under a non-UTF-8 locale.
            // (X11 API audit, finding 6.)
            (self.xlib.Xutf8LookupString)(
                self.xic,
                event,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
                &mut keysym,
                &mut status,
            )
        };

        let chars = if count > 0 {
            // Use count to slice the buffer rather than CStr::from_ptr, which would
            // read past the buffer if X11 fills all 32 bytes with no null terminator.
            let bytes: Vec<u8> = buffer[..count as usize].iter().map(|b| *b as u8).collect();
            Some(String::from_utf8_lossy(&bytes).into_owned())
        } else {
            None
        };

        let keysym = if keysym != 0 { Some(keysym) } else { None };

        (chars, keysym)
    }
}

// XIM preedit callbacks — invoked synchronously from `XFilterEvent` on the
// main thread. They write into the `ImePreeditSink` referenced by
// `client_data`; the event loop drains the sink right after `XFilterEvent`
// returns and forwards it to `text_edit_manager`.
//
// We model `XIMText.string` as a single `*mut c_char` (multi_byte side of the
// original union). The locale is forced to UTF-8 by `XSetLocaleModifiers`, so
// `encoding_is_wchar` is false in practice; if a misbehaving IM sets the wide
// side we treat the text as empty rather than misparse it.

unsafe extern "C" fn preedit_start_cb(
    _xic: XIC,
    client_data: *mut c_void,
    _call_data: *mut c_void,
) {
    if client_data.is_null() {
        return;
    }
    let sink = &*(client_data as *const ImePreeditSink);
    sink.text.borrow_mut().replace(String::new());
    sink.caret.set(0);
    sink.dirty.set(true);
}

unsafe extern "C" fn preedit_done_cb(
    _xic: XIC,
    client_data: *mut c_void,
    _call_data: *mut c_void,
) {
    if client_data.is_null() {
        return;
    }
    let sink = &*(client_data as *const ImePreeditSink);
    *sink.text.borrow_mut() = None;
    sink.caret.set(0);
    sink.dirty.set(true);
}

unsafe extern "C" fn preedit_draw_cb(
    _xic: XIC,
    client_data: *mut c_void,
    call_data: *mut c_void,
) {
    if client_data.is_null() || call_data.is_null() {
        return;
    }
    let sink = &*(client_data as *const ImePreeditSink);
    let draw = &*(call_data as *const XIMPreeditDrawCallbackStruct);

    // Read the replacement substring out of XIMText. If `text` is null, the
    // IM is asking us to delete `chg_length` chars at `chg_first` (string
    // shrinking — common when backspacing in preedit).
    let replacement = if draw.text.is_null() {
        String::new()
    } else {
        let text = &*draw.text;
        if text.encoding_is_wchar != 0 || text.string.is_null() {
            String::new()
        } else {
            CStr::from_ptr(text.string).to_string_lossy().into_owned()
        }
    };

    let mut current = sink.text.borrow_mut();
    let mut buf = current.take().unwrap_or_default();

    // The XIM spec says chg_first / chg_length are in characters, not bytes.
    // Work in chars and collect back to a String.
    let mut chars: Vec<char> = buf.chars().collect();
    let chg_first = draw.chg_first.max(0) as usize;
    let chg_length = draw.chg_length.max(0) as usize;
    let end = chg_first.saturating_add(chg_length).min(chars.len());
    let start = chg_first.min(chars.len());
    let new_chars: Vec<char> = replacement.chars().collect();
    chars.splice(start..end, new_chars.iter().cloned());
    buf = chars.into_iter().collect();

    sink.caret.set(draw.caret);
    *current = Some(buf);
    sink.dirty.set(true);
}

unsafe extern "C" fn preedit_caret_cb(
    _xic: XIC,
    client_data: *mut c_void,
    call_data: *mut c_void,
) {
    if client_data.is_null() || call_data.is_null() {
        return;
    }
    let sink = &*(client_data as *const ImePreeditSink);
    let caret = &*(call_data as *const XIMPreeditCaretCallbackStruct);
    sink.caret.set(caret.position);
    sink.dirty.set(true);
}

impl Drop for ImeManager {
    fn drop(&mut self) {
        unsafe {
            (self.xlib.XDestroyIC)(self.xic);
            (self.xlib.XCloseIM)(self.xim);
        }
    }
}

// Event Handler - Main Implementation

impl X11Window {
    // V2 Cross-Platform Event Processing (from macOS/Windows)

    // Event Handlers (State-Diffing Pattern)

    /// Handle mouse button press/release events
    pub fn handle_mouse_button(&mut self, event: &XButtonEvent) -> ProcessEventResult {
        let is_down = event.type_ == ButtonPress;
        // X11 event coords are PHYSICAL px; everything downstream (hit test,
        // mouse_state, menu bounds check) is LOGICAL.
        let position = self.to_logical_pos(event.x as f32, event.y as f32);

        // Menu/popup dismissal: the menu grabbed the pointer (owner_events=False),
        // so a press whose coords fall OUTSIDE the menu's own bounds is a "click
        // outside" → dismiss it (the run loop drops it on !is_open; close()
        // ungrabs). A press inside is an item click → fall through. event.x/y are
        // relative to the grab (menu) window, so outside = negative or >= size.
        if is_down
            && self.common.current_window_state.flags.window_type
                == azul_core::window::WindowType::Menu
        {
            let size = self.common.current_window_state.size.dimensions;
            if position.x < 0.0
                || position.y < 0.0
                || position.x >= size.width
                || position.y >= size.height
            {
                // close() ungrabs the pointer (for Menu windows) AND XDestroyWindow's
                // the popup. Setting is_open=false directly would leave the later
                // Drop→close() to skip XDestroyWindow (its `if self.is_open` guard is
                // now false), so the dismissed menu's X window would leak — stay
                // mapped and keep grabbing — and the menu would never disappear.
                self.close();
                return ProcessEventResult::DoNothing;
            }
        }

        // Map X11 button to MouseButton
        let button = match event.button {
            1 => MouseButton::Left,
            2 => MouseButton::Middle,
            3 => MouseButton::Right,
            4 if is_down => {
                // Scroll up - handle separately
                return self.handle_scroll(0.0, 1.0, position);
            }
            5 if is_down => {
                // Scroll down - handle separately
                return self.handle_scroll(0.0, -1.0, position);
            }
            6 if is_down => {
                // MWA-B1: horizontal wheel LEFT (X11 button 6) — was unmapped,
                // so tilt-wheel / trackpad horizontal scrolling was completely
                // dead on X11. Sign follows the vertical convention above
                // (4 = +1, 5 = −1); direction normalization happens centrally
                // in ScrollManager. NEEDS-RUNTIME-VERIFY: sign on real hw.
                return self.handle_scroll(1.0, 0.0, position);
            }
            7 if is_down => {
                // MWA-B1: horizontal wheel RIGHT (X11 button 7).
                return self.handle_scroll(-1.0, 0.0, position);
            }
            _ => MouseButton::Other(event.button as u8),
        };

        // Check for scrollbar hit FIRST (before state changes)
        if is_down {
            if let Some(scrollbar_hit_id) =
                PlatformWindow::perform_scrollbar_hit_test(self, position)
            {
                return PlatformWindow::handle_scrollbar_click(self, scrollbar_hit_id, position);
            }
        } else {
            // End scrollbar drag if active
            if self.common.scrollbar_drag_state.is_some() {
                self.common.scrollbar_drag_state = None;
                return ProcessEventResult::ShouldReRenderCurrentWindow;
            }
        }

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update modifier state from X11 event state field
        self.update_modifiers_from_x11_state(event.state);

        // Update mouse state
        self.common.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Set appropriate button flag
        match button {
            MouseButton::Left => self.common.current_window_state.mouse_state.left_down = is_down,
            MouseButton::Right => self.common.current_window_state.mouse_state.right_down = is_down,
            MouseButton::Middle => self.common.current_window_state.mouse_state.middle_down = is_down,
            _ => {}
        }

        // Record input sample for gesture detection
        // X11 provides x_root/y_root as native screen-absolute coordinates
        let button_state = match button {
            MouseButton::Left => BUTTON_STATE_LEFT,
            MouseButton::Right => BUTTON_STATE_RIGHT,
            MouseButton::Middle => BUTTON_STATE_MIDDLE,
            _ => BUTTON_STATE_NONE,
        };
        let screen_pos = self.to_logical_pos(event.x_root as f32, event.y_root as f32);
        self.record_input_sample(position, button_state, is_down, !is_down, Some(screen_pos));

        // Update hit test
        self.update_hit_test(position);

        // Check for right-click context menu (before event processing)
        if !is_down && button == MouseButton::Right {
            if let Some(hit_node) = self.get_first_hovered_node() {
                if self.try_show_context_menu(hit_node, position) {
                    return ProcessEventResult::DoNothing;
                }
            }
        }

        // V2 system will automatically detect MouseDown/MouseUp and dispatch callbacks
        self.process_window_events(0)
    }

    /// Handle mouse motion events
    pub fn handle_mouse_move(&mut self, event: &XMotionEvent) -> ProcessEventResult {
        // Physical (X11 wire) → logical.
        let position = self.to_logical_pos(event.x as f32, event.y as f32);

        // Handle active scrollbar drag (special case - not part of normal event system)
        if self.common.scrollbar_drag_state.is_some() {
            return PlatformWindow::handle_scrollbar_drag(self, position);
        }

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update modifier state from X11 event state field
        self.update_modifiers_from_x11_state(event.state);

        // Update mouse state
        self.common.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Record input sample for gesture detection (movement during button press)
        // X11 provides x_root/y_root as native screen-absolute coordinates
        let ms = &self.common.current_window_state.mouse_state;
        let button_state =
            (ms.left_down as u8) | ((ms.right_down as u8) << 1) | ((ms.middle_down as u8) << 2);
        let screen_pos = self.to_logical_pos(event.x_root as f32, event.y_root as f32);
        self.record_input_sample(position, button_state, false, false, Some(screen_pos));

        // Update hit test
        self.update_hit_test(position);

        // Update cursor based on CSS cursor properties
        // This is done BEFORE callbacks so callbacks can override the cursor
        if let Some(layout_window) = self.common.layout_window.as_ref() {
            if let Some(hit_test) = layout_window
                .hover_manager
                .get_current(&InputPointId::Mouse)
            {
                let cursor_test = layout_window.compute_cursor_type_hit_test(hit_test);
                // Update the window state cursor type
                self.common.current_window_state.mouse_state.mouse_cursor_type =
                    Some(cursor_test.cursor_icon).into();
                // Set the actual OS cursor
                self.set_cursor(cursor_test.cursor_icon);
            }
        }

        // V2 system will detect MouseOver/MouseEnter/MouseLeave/Drag from state diff
        self.process_window_events(0)
    }

    /// Handle mouse entering/leaving window
    pub fn handle_mouse_crossing(&mut self, event: &XCrossingEvent) -> ProcessEventResult {
        // Physical (X11 wire) → logical.
        let position = self.to_logical_pos(event.x as f32, event.y as f32);

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update modifier state from X11 event state field
        self.update_modifiers_from_x11_state(event.state);

        // Update mouse state based on enter/leave
        if event.type_ == EnterNotify {
            self.common.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(position);
            self.update_hit_test(position);
        } else if event.type_ == LeaveNotify {
            self.common.current_window_state.mouse_state.cursor_position =
                CursorPosition::OutOfWindow(position);
            // Clear hit test since mouse is out
            if let Some(ref mut layout_window) = self.common.layout_window {
                layout_window
                    .hover_manager
                    .push_hit_test(InputPointId::Mouse, FullHitTest::empty(None));
            }
        }

        // V2 system will detect MouseEnter/MouseLeave from state diff
        self.process_window_events(0)
    }

    /// Handle scroll wheel events (X11 button 4/5)
    fn handle_scroll(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        position: LogicalPosition,
    ) -> ProcessEventResult {
        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update hit test
        self.update_hit_test(position);

        // Queue scroll input for the physics timer instead of directly setting offsets.
        {
            let mut should_start_timer = false;
            let mut input_queue_clone = None;

            if let Some(ref mut layout_window) = self.common.layout_window {
                use azul_core::task::Instant;
                use azul_layout::managers::scroll_state::ScrollInputSource;

                let now = Instant::from(std::time::Instant::now());

                if let Some((_dom_id, _node_id, start_timer)) =
                    layout_window.scroll_manager.record_scroll_from_hit_test(
                        // Raw delta; direction sign is applied centrally in
                        // ScrollManager::record_scroll_input (natural-scroll flag).
                        delta_x * X11_SCROLL_TICK_PIXELS,
                        delta_y * X11_SCROLL_TICK_PIXELS,
                        ScrollInputSource::WheelDiscrete,
                        &layout_window.hover_manager,
                        &InputPointId::Mouse,
                        now,
                    )
                {
                    should_start_timer = start_timer;
                    if start_timer {
                        input_queue_clone = Some(
                            layout_window.scroll_manager.get_input_queue()
                        );
                    }
                }
            }

            // Start the scroll momentum timer if this is the first input
            if should_start_timer {
                if let Some(queue) = input_queue_clone {
                    use azul_core::task::SCROLL_MOMENTUM_TIMER_ID;
                    use azul_layout::scroll_timer::{ScrollPhysicsState, scroll_physics_timer_callback};
                    use azul_layout::timer::{Timer, TimerCallbackType};
                    use azul_core::refany::RefAny;
                    use azul_core::task::Duration;

                    let physics_state = ScrollPhysicsState::new(queue, self.resources.system_style.scroll_physics.clone());
                    let interval_ms = self.resources.system_style.scroll_physics.timer_interval_ms;
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

        // V2 system will detect Scroll event from recorded state
        self.process_window_events(0)
    }

    /// Handle keyboard events (key press/release)
    pub fn handle_keyboard(&mut self, event: &mut XKeyEvent) -> ProcessEventResult {
        let is_down = event.type_ == KeyPress;

        // Use IME for character translation. XmbLookupString can fire the
        // XIM preedit callbacks (e.g. when the IM updates the composition in
        // response to this keystroke), so after the lookup we drain any new
        // preedit state into text_edit_manager.
        let (char_str, keysym) = if let Some(ime) = &self.ime_manager {
            let result = ime.lookup_string(event);
            if let Some((preedit, caret)) = ime.drain_preedit() {
                if let Some(ref mut lw) = self.common.layout_window {
                    match preedit {
                        Some(t) if !t.is_empty() => {
                            lw.text_edit_manager.set_preedit(t, caret, caret);
                        }
                        _ => lw.text_edit_manager.clear_preedit(),
                    }
                }
            }
            result
        } else {
            // Fallback for when IME is not available
            let mut keysym: KeySym = 0;
            let mut buffer = [0; 32];
            let count = unsafe {
                (self.xlib.XLookupString)(
                    event,
                    buffer.as_mut_ptr(),
                    buffer.len() as i32,
                    &mut keysym,
                    std::ptr::null_mut(),
                )
            };
            let chars = if count > 0 {
                // Use count to slice the buffer rather than CStr::from_ptr, which would
                // read past the buffer if all 32 bytes are filled with no null terminator.
                let bytes: Vec<u8> = buffer[..count as usize].iter().map(|b| *b as u8).collect();
                String::from_utf8_lossy(&bytes).into_owned()
            } else {
                String::new()
            };
            (Some(chars), Some(keysym))
        };

        // Escape dismisses an open menu/popup (close() ungrabs the pointer; the
        // run loop drops the window on !is_open).
        if is_down
            && keysym == Some(XK_Escape as KeySym)
            && self.common.current_window_state.flags.window_type
                == azul_core::window::WindowType::Menu
        {
            // close() ungrabs + XDestroyWindow's the popup; setting is_open=false
            // directly would leak the X window (see the click-outside path).
            self.close();
            return ProcessEventResult::DoNothing;
        }

        // Save previous state BEFORE making changes.
        // Detect key repeat: if the key is already in pressed_virtual_keycodes,
        // this is a repeat. Clear current_virtual_keycode in the snapshot
        // so the state-diff system sees None → Some(key).
        let vk_for_repeat = keysym.and_then(keysym_to_virtual_keycode);
        let is_repeat = is_down && vk_for_repeat.map(|vk| {
            self.common.current_window_state.keyboard_state
                .pressed_virtual_keycodes.as_ref().iter().any(|k| *k == vk)
        }).unwrap_or(false);

        let mut prev_snapshot = self.common.current_window_state.clone();
        if is_repeat {
            prev_snapshot.keyboard_state.current_virtual_keycode =
                azul_core::window::OptionVirtualKeyCode::None;
        }
        self.common.previous_window_state = Some(prev_snapshot);

        // Record text input if we have a character and it's a key press.
        // Don't feed CONTROL characters into text input. XLookupString returns a
        // byte for keys like Backspace (0x08), Tab (0x09), Enter (0x0d), Escape
        // (0x1b) and Delete (0x7f) with count > 0; recording those inserts a
        // glyphless "tofu" rect. The edit commands themselves (delete a char /
        // newline / etc.) are driven by the VirtualKeyCode path in
        // process_window_events below — only PRINTABLE text belongs here.
        // Mirrors the Wayland fix (40da9e554).
        if is_down {
            if let Some(ref text) = char_str {
                let is_control_only = text.chars().all(|c| c.is_control());
                if !text.is_empty() && !is_control_only {
                    if let Some(ref mut layout_window) = self.common.layout_window {
                        layout_window.record_text_input(text);
                    }
                }
            }
        }

        // Update keyboard state with virtual key and scancode
        if let Some(vk) = keysym.and_then(keysym_to_virtual_keycode) {
            if is_down {
                self.common.current_window_state
                    .keyboard_state
                    .pressed_virtual_keycodes
                    .insert_hm_item(vk);
                self.common.current_window_state
                    .keyboard_state
                    .current_virtual_keycode = Some(vk).into();

                // Track scancode (X11 keycode is the scancode)
                self.common.current_window_state
                    .keyboard_state
                    .pressed_scancodes
                    .insert_hm_item(event.keycode as u32);
            } else {
                self.common.current_window_state
                    .keyboard_state
                    .pressed_virtual_keycodes
                    .remove_hm_item(&vk);
                self.common.current_window_state
                    .keyboard_state
                    .current_virtual_keycode = None.into();

                // Remove scancode
                self.common.current_window_state
                    .keyboard_state
                    .pressed_scancodes
                    .remove_hm_item(&(event.keycode as u32));
            }
        }

        // Character input is now handled by V2 event system
        // current_char field has been removed from KeyboardState

        // V2 system will detect VirtualKeyDown/VirtualKeyUp/TextInput from state diff
        self.process_window_events(0)
    }

    // Helper Functions for V2 Event System

    /// Update keyboard state based on X11 event state field.
    ///
    /// X11 events (XButtonEvent, XMotionEvent, XCrossingEvent, XKeyEvent) contain a `state`
    /// field that indicates which modifier keys were held when the event occurred.
    /// This function synchronizes the KeyboardState with that information.
    fn update_modifiers_from_x11_state(&mut self, state: std::ffi::c_uint) {
        use azul_core::window::VirtualKeyCode;

        // Check each modifier mask and update the keyboard state accordingly
        let keyboard_state = &mut self.common.current_window_state.keyboard_state;

        // Shift
        let shift_down = (state & SHIFT_MASK) != 0;
        if shift_down {
            keyboard_state
                .pressed_virtual_keycodes
                .insert_hm_item(VirtualKeyCode::LShift);
        } else {
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::LShift);
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::RShift);
        }

        // Control
        let ctrl_down = (state & CONTROL_MASK) != 0;
        if ctrl_down {
            keyboard_state
                .pressed_virtual_keycodes
                .insert_hm_item(VirtualKeyCode::LControl);
        } else {
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::LControl);
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::RControl);
        }

        // Alt (Mod1)
        let alt_down = (state & MOD1_MASK) != 0;
        if alt_down {
            keyboard_state
                .pressed_virtual_keycodes
                .insert_hm_item(VirtualKeyCode::LAlt);
        } else {
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::LAlt);
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::RAlt);
        }

        // Super/Windows (Mod4)
        let super_down = (state & MOD4_MASK) != 0;
        if super_down {
            keyboard_state
                .pressed_virtual_keycodes
                .insert_hm_item(VirtualKeyCode::LWin);
        } else {
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::LWin);
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::RWin);
        }
    }

    /// Update hit test at given position and store in current_window_state
    fn update_hit_test(&mut self, position: LogicalPosition) {
        // Delegate to the shared CommonWindowState::perform_hit_test, which uses the
        // WebRender hit-tester in GPU mode and the cpu_hit_tester in CPU mode (returning
        // an empty hit-test if neither is ready). The previous inline logic
        // unconditionally `.unwrap()`'d self.common.hit_tester — which is None in CPU
        // mode — so the first mouse-crossing event (handle_mouse_crossing) panicked and
        // aborted the process. (Mirrors the Wayland update_hit_test.)
        let hit_test = self.common.perform_hit_test(position);
        if let Some(ref mut layout_window) = self.common.layout_window {
            layout_window
                .hover_manager
                .push_hit_test(InputPointId::Mouse, hit_test);
        }
    }

    /// XDND drag entering / moving over the window (emits `EventType::FileHover`).
    /// `position` is window-local (translated from the XDND root coords); XDND
    /// does not expose file paths until the drop, so `paths` is a placeholder
    /// marker so the hover transition fires. Mirrors the macOS
    /// `handle_file_drag_entered`.
    pub fn handle_file_drag_entered(
        &mut self,
        position: LogicalPosition,
        paths: Vec<String>,
    ) -> ProcessEventResult {
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
        self.common.current_window_state.mouse_state.cursor_position =
            CursorPosition::InWindow(position);
        if !paths.is_empty() {
            if let Some(layout_window) = self.common.layout_window.as_mut() {
                // MWA-B7: pass EVERY path — multi-file drops were silently
                // truncated to the first file at this ingress.
                layout_window
                    .file_drop_manager
                    .set_hovered_files(paths.iter().map(|p| p.clone().into()).collect());
            }
        }
        self.update_hit_test(position);
        self.process_window_events(0)
    }

    /// XDND drag leaving the window without a drop (emits
    /// `EventType::FileHoverCancel`). Mirrors the macOS `handle_file_drag_exited`.
    pub fn handle_file_drag_exited(&mut self) -> ProcessEventResult {
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.file_drop_manager.set_hovered_file(None);
        }
        let result = self.process_window_events(0);
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.file_drop_manager.clear_hover_cancelled();
        }
        result
    }

    /// XDND drop completed: the real file paths (parsed from `text/uri-list`)
    /// dropped at window-local `position` (emits `EventType::FileDrop`). Mirrors
    /// the macOS `handle_file_drop`.
    pub fn handle_file_drop(
        &mut self,
        position: LogicalPosition,
        paths: Vec<String>,
    ) -> ProcessEventResult {
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
        self.common.current_window_state.mouse_state.cursor_position =
            CursorPosition::InWindow(position);
        if !paths.is_empty() {
            if let Some(layout_window) = self.common.layout_window.as_mut() {
                // MWA-B7: pass EVERY path — multi-file drops were silently
                // truncated to the first file at this ingress.
                layout_window
                    .file_drop_manager
                    .set_dropped_files(paths.iter().map(|p| p.clone().into()).collect());
            }
        }
        self.update_hit_test(position);
        let result = self.process_window_events(0);
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.file_drop_manager.set_dropped_file(None);
        }
        result
    }

    /// Get the first hovered node from current hit test
    fn get_first_hovered_node(&self) -> Option<HitTestNode> {
        self.common.layout_window
            .as_ref()?
            .hover_manager
            .get_current(&InputPointId::Mouse)?
            .hovered_nodes
            .iter()
            .flat_map(|(dom_id, ht)| {
                ht.regular_hit_test_nodes
                    .keys()
                    .next_back()
                    .map(|node_id| HitTestNode {
                        dom_id: dom_id.inner as u64,
                        node_id: node_id.index() as u64,
                    })
            })
            .next()
    }

    // Scrollbar methods provided by PlatformWindow trait (see common/event.rs)

    // Context Menu Support

    /// Try to show context menu for the given node at position
    ///
    /// Uses the unified menu system (crate::desktop::menu::show_menu) which is identical
    /// to how menu bar menus work, but spawns at cursor position instead of below a trigger rect.
    /// Returns true if a menu was shown
    fn try_show_context_menu(&mut self, node: HitTestNode, position: LogicalPosition) -> bool {
        let layout_window = match self.common.layout_window.as_ref() {
            Some(lw) => lw,
            None => return false,
        };

        let dom_id = DomId {
            inner: node.dom_id as usize,
        };

        // Get layout result for this DOM
        let layout_result = match layout_window.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return false,
        };

        // `node.node_id` is a 0-based index (as emitted by get_first_hovered_node).
        // Walk UP the ancestor chain from the hit node to find the nearest node
        // carrying a context menu — standard "inherit the nearest ancestor's menu"
        // semantics, so a right-click on a child still finds a parent's menu.
        let binding = layout_result.styled_dom.node_data.as_container();
        let hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let mut cur = Some(azul_core::id::NodeId::new(node.node_id as usize));
        let context_menu = loop {
            let nid = match cur {
                Some(n) => n,
                None => return false,
            };
            if let Some(menu) = binding.get(nid).and_then(|nd| nd.get_context_menu()) {
                break menu.clone();
            }
            cur = hierarchy.get(nid).and_then(|h| h.parent_id());
        };

        log_debug!(
            LogCategory::Input,
            "[X11 Context Menu] Showing context menu at ({}, {}) for node {:?} with {} items",
            position.x,
            position.y,
            node,
            context_menu.items.as_slice().len()
        );

        // Queue the window creation instead of creating immediately
        self.show_window_based_context_menu(&context_menu, position);
        true
    }

    /// Queue a window-based context menu for creation in the event loop
    /// This is part of the unified multi-window menu system (Shell2 V2)
    fn show_window_based_context_menu(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: LogicalPosition,
    ) {
        // Get parent window position
        let parent_pos = match self.common.current_window_state.position {
            azul_core::window::WindowPosition::Initialized(pos) => {
                azul_core::geom::LogicalPosition::new(pos.x as f32, pos.y as f32)
            }
            _ => azul_core::geom::LogicalPosition::new(0.0, 0.0),
        };

        // show_menu's screen-space math is consumed as PHYSICAL px on X11
        // (parent_pos above is physical); scale the logical cursor to match.
        let scale = self.hidpi();
        let physical_cursor = LogicalPosition::new(position.x * scale, position.y * scale);

        // Create menu window options using unified menu system
        let mut menu_options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.resources.system_style.clone(),
            parent_pos,
            None,                   // No trigger rect for context menus
            Some(physical_cursor), // Cursor position (physical px)
            None,                   // No parent menu
        );
        // Parent the menu to THIS window so it reuses our X display (single
        // shared event pump) and is positioned relative to us.
        menu_options.parent_window_id = self.window as u64;

        log_debug!(
            LogCategory::Window,
            "[X11] Queuing window-based context menu at screen ({}, {})",
            position.x,
            position.y
        );
        self.pending_window_creates.push(menu_options);
    }
}

// Keycode Conversion

pub fn keysym_to_virtual_keycode(keysym: KeySym) -> Option<VirtualKeyCode> {
    // This is a partial mapping based on X11/keysymdef.h
    match keysym as u32 {
        XK_BackSpace => Some(VirtualKeyCode::Back),
        XK_Tab => Some(VirtualKeyCode::Tab),
        XK_Return => Some(VirtualKeyCode::Return),
        XK_Pause => Some(VirtualKeyCode::Pause),
        XK_Scroll_Lock => Some(VirtualKeyCode::Scroll),
        XK_Escape => Some(VirtualKeyCode::Escape),
        XK_Home => Some(VirtualKeyCode::Home),
        XK_Left => Some(VirtualKeyCode::Left),
        XK_Up => Some(VirtualKeyCode::Up),
        XK_Right => Some(VirtualKeyCode::Right),
        XK_Down => Some(VirtualKeyCode::Down),
        XK_Page_Up => Some(VirtualKeyCode::PageUp),
        XK_Page_Down => Some(VirtualKeyCode::PageDown),
        XK_End => Some(VirtualKeyCode::End),
        XK_Insert => Some(VirtualKeyCode::Insert),
        XK_Delete => Some(VirtualKeyCode::Delete),
        XK_space => Some(VirtualKeyCode::Space),
        XK_0 => Some(VirtualKeyCode::Key0),
        XK_1 => Some(VirtualKeyCode::Key1),
        XK_2 => Some(VirtualKeyCode::Key2),
        XK_3 => Some(VirtualKeyCode::Key3),
        XK_4 => Some(VirtualKeyCode::Key4),
        XK_5 => Some(VirtualKeyCode::Key5),
        XK_6 => Some(VirtualKeyCode::Key6),
        XK_7 => Some(VirtualKeyCode::Key7),
        XK_8 => Some(VirtualKeyCode::Key8),
        XK_9 => Some(VirtualKeyCode::Key9),
        XK_a | XK_A => Some(VirtualKeyCode::A),
        XK_b | XK_B => Some(VirtualKeyCode::B),
        XK_c | XK_C => Some(VirtualKeyCode::C),
        XK_d | XK_D => Some(VirtualKeyCode::D),
        XK_e | XK_E => Some(VirtualKeyCode::E),
        XK_f | XK_F => Some(VirtualKeyCode::F),
        XK_g | XK_G => Some(VirtualKeyCode::G),
        XK_h | XK_H => Some(VirtualKeyCode::H),
        XK_i | XK_I => Some(VirtualKeyCode::I),
        XK_j | XK_J => Some(VirtualKeyCode::J),
        XK_k | XK_K => Some(VirtualKeyCode::K),
        XK_l | XK_L => Some(VirtualKeyCode::L),
        XK_m | XK_M => Some(VirtualKeyCode::M),
        XK_n | XK_N => Some(VirtualKeyCode::N),
        XK_o | XK_O => Some(VirtualKeyCode::O),
        XK_p | XK_P => Some(VirtualKeyCode::P),
        XK_q | XK_Q => Some(VirtualKeyCode::Q),
        XK_r | XK_R => Some(VirtualKeyCode::R),
        XK_s | XK_S => Some(VirtualKeyCode::S),
        XK_t | XK_T => Some(VirtualKeyCode::T),
        XK_u | XK_U => Some(VirtualKeyCode::U),
        XK_v | XK_V => Some(VirtualKeyCode::V),
        XK_w | XK_W => Some(VirtualKeyCode::W),
        XK_x | XK_X => Some(VirtualKeyCode::X),
        XK_y | XK_Y => Some(VirtualKeyCode::Y),
        XK_z | XK_Z => Some(VirtualKeyCode::Z),
        XK_F1 => Some(VirtualKeyCode::F1),
        XK_F2 => Some(VirtualKeyCode::F2),
        XK_F3 => Some(VirtualKeyCode::F3),
        XK_F4 => Some(VirtualKeyCode::F4),
        XK_F5 => Some(VirtualKeyCode::F5),
        XK_F6 => Some(VirtualKeyCode::F6),
        XK_F7 => Some(VirtualKeyCode::F7),
        XK_F8 => Some(VirtualKeyCode::F8),
        XK_F9 => Some(VirtualKeyCode::F9),
        XK_F10 => Some(VirtualKeyCode::F10),
        XK_F11 => Some(VirtualKeyCode::F11),
        XK_F12 => Some(VirtualKeyCode::F12),
        XK_Shift_L => Some(VirtualKeyCode::LShift),
        XK_Shift_R => Some(VirtualKeyCode::RShift),
        XK_Control_L => Some(VirtualKeyCode::LControl),
        XK_Control_R => Some(VirtualKeyCode::RControl),
        XK_Alt_L => Some(VirtualKeyCode::LAlt),
        XK_Alt_R => Some(VirtualKeyCode::RAlt),
        XK_Super_L => Some(VirtualKeyCode::LWin),
        XK_Super_R => Some(VirtualKeyCode::RWin),
        _ => None,
    }
}
