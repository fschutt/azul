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
    hit_test::{FullHitTest, HitTest},
    window::{CursorPosition, VirtualKeyCode},
};
use crate::desktop::shell2::common::event::{
    HitTestNode, BUTTON_STATE_LEFT, BUTTON_STATE_RIGHT, BUTTON_STATE_MIDDLE, BUTTON_STATE_NONE,
};
use azul_layout::{
    managers::hover::InputPointId,
};

use super::{defines::*, dlopen::Xlib, X11Window};
use super::super::common::compose::ComposeAction;
use crate::desktop::shell2::common::event::PlatformWindow;

use super::super::super::common::debug_server::LogCategory;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

/// Pixels per discrete X11 scroll tick (button 4/5). X11 scroll events are
/// unitless discrete steps; this constant converts them to pixel deltas for
/// the scroll physics system.
pub(super) const X11_SCROLL_TICK_PIXELS: f32 =
    crate::desktop::shell2::common::event::WHEEL_SCROLL_PIXELS_PER_LINE;

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

    /// MWA-C-text_input: focus/unfocus the input context depending on
    /// whether an editable node is active — XSetICFocus previously ran once
    /// at IC creation and never toggled, so the IM could pop its candidate
    /// window while nothing editable was focused (Wayland/macOS both gate
    /// their IME on editable focus).
    pub(super) fn set_ic_focused(&self, focused: bool) {
        unsafe {
            if focused {
                (self.xlib.XSetICFocus)(self.xic);
            } else {
                (self.xlib.XUnsetICFocus)(self.xic);
            }
        }
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
        // Xutf8LookupString (not XmbLookupString): the committed bytes are
        // guaranteed UTF-8 regardless of the locale codeset, so accented and
        // CJK commit strings decode correctly even under a non-UTF-8 locale.
        // (X11 API audit, finding 6.)
        let lookup = self.xlib.Xutf8LookupString;
        let xic = self.xic;
        let event: *mut XKeyEvent = event;
        lookup_string_with(|buf, len, keysym, status| unsafe {
            (lookup)(xic, event, buf, len, keysym, status)
        })
    }
}

/// The overflow-retry protocol of the `X*LookupString` family, with the Xlib
/// call itself injected so it can be exercised without an input method.
///
/// `XBufferOverflow` means NOTHING was written and the return value is the
/// buffer size the commit needs. The fixed 32-byte stack buffer overflows on
/// any commit past ~11 CJK characters — an ordinary phrase — and leaving the
/// status untested made the caller see a `count` it could not use, so the whole
/// composed sentence silently vanished. On overflow the lookup is repeated into
/// a heap buffer of exactly the requested size.
pub(super) fn lookup_string_with<F>(mut lookup: F) -> (Option<String>, Option<KeySym>)
where
    F: FnMut(*mut c_char, i32, *mut KeySym, *mut i32) -> i32,
{
    let mut keysym: KeySym = 0;
    let mut status: i32 = 0;
    let mut stack: [c_char; 32] = [0; 32];

    let mut count = lookup(
        stack.as_mut_ptr(),
        stack.len() as i32,
        &mut keysym as *mut KeySym,
        &mut status as *mut i32,
    );

    let mut heap: Vec<c_char> = Vec::new();
    if status == XBufferOverflow && count > 0 {
        heap = vec![0; count as usize];
        count = lookup(
            heap.as_mut_ptr(),
            heap.len() as i32,
            &mut keysym as *mut KeySym,
            &mut status as *mut i32,
        );
    }

    // XLookupNone / XLookupKeySym leave the buffer untouched (count == 0);
    // a second overflow would mean the IM lied about the size.
    let has_text = count > 0 && !matches!(status, XBufferOverflow | XLookupNone | XLookupKeySym);
    let chars = if has_text {
        // Use count to slice the buffer rather than CStr::from_ptr, which would
        // read past the buffer if X11 fills it with no null terminator.
        let src: &[c_char] = if heap.is_empty() { &stack } else { &heap };
        let end = (count as usize).min(src.len());
        let bytes: Vec<u8> = src[..end].iter().map(|b| *b as u8).collect();
        Some(String::from_utf8_lossy(&bytes).into_owned())
    } else {
        None
    };

    let keysym = if keysym != 0 { Some(keysym) } else { None };

    (chars, keysym)
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
            && self.common.current_window_state().flags.window_type
                == azul_core::window::WindowType::Menu
        {
            let size = self.common.current_window_state().size.dimensions;
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
            4..=7 => {
                // Wheel RELEASE: X11 delivers scroll as press+release PAIRS of
                // buttons 4–7 (one pair per detent; XI2 emulation does the
                // same). Only the press carries the scroll; the release must
                // be swallowed. It used to fall through to Other(4..7) and run
                // the whole button pipeline — state snapshot, an is_up
                // input-sample for gesture detection, hit test and a state
                // diff — once per detent, for an event that means nothing.
                return ProcessEventResult::DoNothing;
            }
            _ => MouseButton::Other(event.button as u8),
        };

        // MWA-B11: CSD resize edges — frameless windows previously had NO
        // way to resize. A left press in the border band hands the resize
        // to the WM via _NET_WM_MOVERESIZE (root coords come straight from
        // the button event; the implicit grab must be released first or the
        // WM cannot take over the pointer).
        if is_down
            && button == MouseButton::Left
            && self.common.current_window_state().flags.decorations
                == azul_core::window::WindowDecorations::None
        {
            use crate::desktop::shell2::common::event::{
                csd_resize_edge_at, CsdResizeEdge, CSD_RESIZE_BAND_PX,
            };
            let size = self.common.current_window_state().size.dimensions;
            if let Some(edge) = csd_resize_edge_at(position, size, CSD_RESIZE_BAND_PX) {
                // _NET_WM_MOVERESIZE directions: TOPLEFT=0 TOP=1 TOPRIGHT=2
                // RIGHT=3 BOTTOMRIGHT=4 BOTTOM=5 BOTTOMLEFT=6 LEFT=7.
                let direction: std::os::raw::c_long = match edge {
                    CsdResizeEdge::TopLeft => 0,
                    CsdResizeEdge::Top => 1,
                    CsdResizeEdge::TopRight => 2,
                    CsdResizeEdge::Right => 3,
                    CsdResizeEdge::BottomRight => 4,
                    CsdResizeEdge::Bottom => 5,
                    CsdResizeEdge::BottomLeft => 6,
                    CsdResizeEdge::Left => 7,
                };
                self.begin_net_wm_moveresize(
                    event.x_root as std::os::raw::c_long,
                    event.y_root as std::os::raw::c_long,
                    direction,
                );
                return ProcessEventResult::DoNothing;
            }
        }

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
        self.snapshot_window_state_baseline("x11.handle_mouse_button");

        // Update modifier state from X11 event state field
        self.update_modifiers_from_x11_state(event.state);

        // Update mouse state
        self.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(position);

        // Set appropriate button flag
        match button {
            MouseButton::Left => self.common.mouse_state_mut().left_down = is_down,
            MouseButton::Right => self.common.mouse_state_mut().right_down = is_down,
            MouseButton::Middle => self.common.mouse_state_mut().middle_down = is_down,
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

        // Check for right-click context menu (before event processing).
        // The pass below runs EITHER WAY: returning early here left
        // `right_down: true -> false` sitting in the un-consumed delta, so
        // RightMouseUp / Hover(RightMouseUp) never fired for that click and
        // the next handler's snapshot destroyed the transition.
        if !is_down && button == MouseButton::Right {
            if let Some(hit_node) = self.get_first_hovered_node() {
                self.try_show_context_menu(hit_node, position);
            }
        }

        // X11 middle-click paste: the PRIMARY selection is inserted at the
        // caret, which the button-2 PRESS already moved to the click point.
        // Recorded before the pass so the changeset is applied by it, exactly
        // like typed text. (The other half of the idiom — claiming PRIMARY on
        // selection — is below.)
        //
        // Only asked for when something editable actually has focus: the read
        // waits on the selection OWNER, so a middle click anywhere else must
        // not pay for it (and `record_text_input` would drop the text anyway).
        if !is_down
            && button == MouseButton::Middle
            && self
                .common
                .layout_window
                .as_ref()
                .is_some_and(|lw| lw.text_edit_manager.has_active_editing())
        {
            if let Some(text) = super::clipboard::get_primary_content() {
                if !text.is_empty() {
                    if let Some(ref mut layout_window) = self.common.layout_window {
                        layout_window.record_text_input(&text);
                    }
                }
            }
        }

        // V2 system will automatically detect MouseDown/MouseUp and dispatch callbacks
        let result = self.process_window_events(0);

        // The release that ends a selection gesture claims PRIMARY (run after
        // the pass, which is what finalizes the selection).
        if !is_down && button == MouseButton::Left {
            self.publish_primary_selection();
        }

        result
    }

    /// Claim the X11 PRIMARY selection for the current text selection.
    ///
    /// On X11, *selecting* text is itself a PRIMARY claim — no copy involved —
    /// and middle-click pastes it. PRIMARY was only ever written by an explicit
    /// Ctrl+C, so both halves of the idiom were missing.
    fn publish_primary_selection(&mut self) {
        let text = {
            let Some(lw) = self.common.layout_window.as_ref() else {
                return;
            };
            if !lw.text_edit_manager.has_active_editing() {
                return;
            }
            let dom_id = lw
                .text_edit_manager
                .get_editing_dom_id()
                .unwrap_or(DomId { inner: 0 });
            match lw.get_selected_content_for_clipboard(&dom_id) {
                Some(content) => content.plain_text.as_str().to_string(),
                None => return,
            }
        };
        if text.is_empty() {
            return;
        }
        if let Err(e) = super::clipboard::write_to_primary(&text) {
            log_warn!(
                LogCategory::Resources,
                "[X11] failed to claim the PRIMARY selection: {e}"
            );
        }
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
        self.snapshot_window_state_baseline("x11.handle_mouse_move");

        // Update modifier state from X11 event state field
        self.update_modifiers_from_x11_state(event.state);

        // Update mouse state
        self.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(position);

        // Record input sample for gesture detection (movement during button press)
        // X11 provides x_root/y_root as native screen-absolute coordinates
        let ms = &self.common.current_window_state().mouse_state;
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
                self.common.mouse_state_mut().mouse_cursor_type =
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
        // A grab activating or releasing synthesizes a Leave/Enter pair that
        // does NOT mean the pointer moved — and this app grabs the pointer for
        // its own menus. Acting on it pushed an EMPTY hit test and
        // OutOfWindow, so opening a context menu wiped the parent's hover
        // state (and the matching ungrab crossing re-ran the whole pass).
        if event.mode == NotifyGrab || event.mode == NotifyUngrab {
            return ProcessEventResult::DoNothing;
        }

        // Physical (X11 wire) → logical.
        let position = self.to_logical_pos(event.x as f32, event.y as f32);

        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("x11.handle_mouse_crossing");

        // Update modifier state from X11 event state field
        self.update_modifiers_from_x11_state(event.state);

        // Update mouse state based on enter/leave
        if event.type_ == EnterNotify {
            self.common.mouse_state_mut().cursor_position =
                CursorPosition::InWindow(position);
            self.update_hit_test(position);
        } else if event.type_ == LeaveNotify {
            self.common.mouse_state_mut().cursor_position =
                CursorPosition::OutOfWindow(position);
            // Clear hit test since mouse is out — unless a drag is in flight,
            // in which case the latch keeps the target (see
            // push_hit_test_latched). A drag past the window edge is an
            // ordinary NotifyNormal crossing, which the grab-mode filter above
            // does not and must not catch.
            self.push_hit_test_latched(FullHitTest::empty(None));
        }

        // V2 system will detect MouseEnter/MouseLeave from state diff
        self.process_window_events(0)
    }

    /// Handle a discrete wheel detent (core / XI2-emulated buttons 4-7).
    ///
    /// `delta_x` / `delta_y` are ratcheting tick counts in the engine's
    /// canonical X11 sign convention (up = +1, left = +1).
    fn handle_scroll(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        position: LogicalPosition,
    ) -> ProcessEventResult {
        self.handle_scroll_input(
            delta_x * X11_SCROLL_TICK_PIXELS,
            delta_y * X11_SCROLL_TICK_PIXELS,
            position,
            false,
        )
    }

    /// Shared scroll ingress, in PIXELS, in the canonical X11 sign convention.
    ///
    /// `continuous` marks an XI2 smooth-scroll valuator delta (touchpad /
    /// kinetic trackpoint): those are position deltas, not wheel ticks, and
    /// feeding them in as `WheelDiscrete` stacks one velocity impulse per
    /// event — the jerky touchpad scrolling Wayland already fixed via
    /// `axis_source` classification.
    pub(super) fn handle_scroll_input(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        position: LogicalPosition,
        continuous: bool,
    ) -> ProcessEventResult {
        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("x11.handle_scroll_input");

        // Arm / re-arm the synthetic gesture-end deadline. XI2 has no
        // gesture-end event, so the fingers coming off the touchpad is only
        // ever visible as this going quiet — see `trackpad_gesture_ended`.
        // A wheel is not a gesture and must not arm it: it would fire a
        // TrackpadEnd 100 ms after every detent.
        if continuous {
            self.last_continuous_scroll = Some(std::time::Instant::now());
        }

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

                let (source, device) = if continuous {
                    (
                        ScrollInputSource::TrackpadContinuous,
                        azul_layout::managers::scroll_state::ScrollInputDevice::Touchpad,
                    )
                } else {
                    (
                        ScrollInputSource::WheelDiscrete,
                        azul_layout::managers::scroll_state::ScrollInputDevice::MouseWheel,
                    )
                };

                if let Some((_dom_id, _node_id, start_timer)) =
                    layout_window.scroll_manager.record_scroll_from_hit_test(
                        // Raw delta; direction sign is applied centrally in
                        // ScrollManager::record_scroll_input (natural-scroll flag).
                        delta_x,
                        delta_y,
                        source,
                        device,
                        &layout_window.hover_manager,
                        &InputPointId::Mouse,
                        now,
                    )
                {
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
        //
        // KeyPress ONLY: the Xmb/Xwc/Xutf8LookupString family is defined
        // solely for KeyPress events — "it is essential that the client pass
        // only KeyPress events…; their behavior when a client passes a
        // KeyRelease event is undefined" (XmbLookupString(3)). KeyRelease
        // takes the core-XLookupString branch below, which is defined for
        // both and recovers the keysym for the pressed-keys bookkeeping.
        let (char_str, keysym) = if let (true, Some(ime)) = (is_down, self.ime_manager.as_ref()) {
            let result = ime.lookup_string(event);
            if let Some((preedit, caret)) = ime.drain_preedit() {
                if let Some(ref mut lw) = self.common.layout_window {
                    match preedit {
                        Some(t) if !t.is_empty() => {
                            lw.text_edit_manager.set_preedit(t, caret, caret);
                        }
                        _ => lw.text_edit_manager.clear_preedit(),
                    }
                    // MWA-C-text_input: splice/restore the composition glyphs
                    // in the text cache (macOS-only before) — X11 CJK
                    // composition showed only an approximate-width underline.
                    if let Some((dom_id, node_id)) = lw
                        .text_edit_manager
                        .get_editing_dom_id()
                        .zip(lw.text_edit_manager.get_editing_node_id())
                    {
                        lw.apply_preedit_to_text_cache(dom_id, node_id);
                    }
                }
            }
            result
        } else {
            // No IME available, or a KeyRelease (see above): core lookup.
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

        // Compose sequences (dead keys, the Compose key) — the half the core
        // lookup has never had. `XLookupString` resolves `dead_acute` to its
        // own accent character and `e` to `e`; only the compose table turns
        // the pair into `é`. Applied ONLY on the no-XIM path: with an input
        // method the `Xutf8LookupString` above already composed, and feeding
        // the same keysyms in again would compose twice.
        //
        // `None` text means the keystroke belongs to the sequence and produces
        // no text of its own — which is exactly what makes the dead key stop
        // typing a stray accent.
        let char_str = match (is_down, self.compose.as_mut(), keysym) {
            (true, Some(sequencer), Some(sym)) => match sequencer.feed(sym as u32) {
                ComposeAction::Commit(text) => Some(text),
                ComposeAction::Composing | ComposeAction::Cancelled => None,
                ComposeAction::Pass => char_str,
            },
            _ => char_str,
        };

        // Escape dismisses an open menu/popup (close() ungrabs the pointer; the
        // run loop drops the window on !is_open).
        if is_down
            && keysym == Some(XK_Escape as KeySym)
            && self.common.current_window_state().flags.window_type
                == azul_core::window::WindowType::Menu
        {
            // close() ungrabs + XDestroyWindow's the popup; setting is_open=false
            // directly would leak the X window (see the click-outside path).
            self.close();
            return ProcessEventResult::DoNothing;
        }

        // Resolve the VirtualKeyCode from the PHYSICAL key (group 0 / level 0),
        // NOT from the composed keysym: `1` pressed, Shift pressed, `1`
        // released reports XK_exclam for the release, and a release that
        // resolves to a different code — or to none — leaves the key stuck in
        // `pressed_virtual_keycodes` forever. Falls back to the looked-up
        // keysym when XKB is unavailable; the table folds the common shifted
        // forms onto their base key for exactly that case.
        let vk_pressed = self
            .unmodified_keysym(event.keycode)
            .and_then(keysym_to_virtual_keycode)
            .or_else(|| keysym.and_then(keysym_to_virtual_keycode));

        // Save previous state BEFORE making changes.
        // Detect key repeat: if the key is already in pressed_virtual_keycodes,
        // this is a repeat. Clear current_virtual_keycode in the snapshot
        // so the state-diff system sees None → Some(key).
        let is_repeat = is_down && vk_pressed.map(|vk| {
            self.common.current_window_state().keyboard_state
                .pressed_virtual_keycodes.as_ref().iter().any(|k| *k == vk)
        }).unwrap_or(false);

        let mut prev_snapshot = self.common.current_window_state().clone();
        if is_repeat {
            prev_snapshot.keyboard_state.current_virtual_keycode =
                azul_core::window::OptionVirtualKeyCode::None;
        }
        self.set_previous_window_state(prev_snapshot);

        // Resync the modifier bits the SERVER reports for this event. Key
        // events never did this — only pointer events did — so a modifier
        // released while another window held focus stayed latched until the
        // user happened to move the mouse. Runs BEFORE the keysym bookkeeping
        // below so that a modifier key's OWN press/release still wins: the
        // `state` field describes the moment BEFORE this event.
        self.update_modifiers_from_x11_state(event.state);

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
        apply_key_state_change(
            self.common.keyboard_state_mut(),
            &mut self.pressed_key_vks,
            event.keycode as u32,
            vk_pressed,
            is_down,
        );

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
    ///
    /// Alt / Super / AltGr are read from `self.modifier_masks` — queried from
    /// `XGetModifierMapping` and refreshed on `MappingNotify` — instead of the
    /// hardcoded Mod1/Mod4 defaults, which are wrong on any remapped keyboard
    /// and never carried AltGr at all.
    pub(super) fn update_modifiers_from_x11_state(&mut self, state: std::ffi::c_uint) {
        let masks = self.modifier_masks;
        apply_modifier_mask_state(
            self.common.keyboard_state_mut(),
            masks,
            state,
        );
    }

    /// Drop every key the window still believes is held.
    ///
    /// Keys released while ANOTHER window had focus are never delivered here,
    /// so their entries survive a focus round-trip — the classic stuck Alt
    /// after Alt-Tab. Only pointer events incidentally repaired the modifiers
    /// (`update_modifiers_from_x11_state`); non-modifier keys never recovered
    /// at all. Called on focus loss; `resync_keyboard_state_from_vector`
    /// re-establishes the truth on focus gain.
    pub(super) fn clear_keyboard_state(&mut self) {
        use azul_core::window::{OptionVirtualKeyCode, ScanCodeVec, VirtualKeyCodeVec};

        let keyboard_state = self.common.keyboard_state_mut();
        keyboard_state.pressed_virtual_keycodes = VirtualKeyCodeVec::from_vec(Vec::new());
        keyboard_state.pressed_scancodes = ScanCodeVec::from_vec(Vec::new());
        keyboard_state.current_virtual_keycode = OptionVirtualKeyCode::None;
        // The press→code record mirrors the lists above and has to die with
        // them: an entry that outlived its list would make a much later release
        // of the same physical key remove a code nobody pressed.
        self.pressed_key_vks.clear();
    }

    /// The group-0 / level-0 keysym of a physical keycode — the key's
    /// UNMODIFIED symbol.
    ///
    /// Pressed-key bookkeeping must be keyed by the PHYSICAL key, never by the
    /// composed symbol: `XLookupString` reports `XK_exclam` for Shift+`1` but
    /// `XK_1` for the release once Shift is up (AltGr shifts to a third level,
    /// a second layout group shifts again), and a press/release pair that
    /// disagrees leaves the key stuck in `pressed_virtual_keycodes` forever.
    /// Same translation `resync_keyboard_state_from_vector` uses, for the same
    /// reason.
    ///
    /// `None` when XKB is unavailable or the keycode is unbound, so callers
    /// fall back to the looked-up keysym.
    fn unmodified_keysym(&self, keycode: u32) -> Option<KeySym> {
        let to_keysym = self.xlib.XkbKeycodeToKeysym?;
        // X11 keycodes are 8-bit; the c_uint field is protocol padding.
        let keysym = unsafe { (to_keysym)(self.display, keycode as KeyCode, 0, 0) };
        if keysym == NoSymbol {
            None
        } else {
            Some(keysym)
        }
    }

    /// Rebuild `pressed_virtual_keycodes` / `pressed_scancodes` from a 32-byte
    /// X11 keycode bit vector (`KeymapNotify.key_vector`, or `XQueryKeymap`).
    ///
    /// This is the X11-designed remedy for the stuck-key problem: the server
    /// reports the FULL keyboard state right after every FocusIn, so the client
    /// can replace its guess with the truth instead of waiting for a release it
    /// will never receive.
    pub(super) fn resync_keyboard_state_from_vector(&mut self, key_vector: &[c_char; 32]) {
        let held = self
            .common
            .current_window_state()
            .keyboard_state
            .current_virtual_keycode
            .into_option();
        self.clear_keyboard_state();

        let Some(to_keysym) = self.xlib.XkbKeycodeToKeysym else {
            // No translation available: leaving the state cleared is still
            // strictly better than leaving it stale.
            return;
        };

        // Keycodes 8..=255 (0..8 are unused by the X protocol).
        for keycode in 8u32..256 {
            let byte = key_vector[(keycode >> 3) as usize] as u8;
            if byte & (1 << (keycode & 7)) == 0 {
                continue;
            }
            self.common
                .keyboard_state_mut()
                .pressed_scancodes
                .insert_hm_item(keycode);
            // Group 0 / level 0: the unshifted keysym, which is what
            // keysym_to_virtual_keycode folds shifted variants back onto.
            let keysym = unsafe { (to_keysym)(self.display, keycode as KeyCode, 0, 0) };
            if let Some(vk) = keysym_to_virtual_keycode(keysym) {
                self.common
                    .keyboard_state_mut()
                    .pressed_virtual_keycodes
                    .insert_hm_item(vk);
                // Record what was seeded, so the release the server has not sent
                // yet removes exactly this code.
                self.pressed_key_vks.insert(keycode, vk);
            }
        }

        // Keep the key that fired the last KeyDown if it is STILL held. Event
        // determination derives KeyUp from `current_virtual_keycode` dropping
        // to None, and KeymapNotify also follows every EnterNotify — clearing
        // it unconditionally would fake a release of a key the user is holding.
        if let Some(vk) = held {
            if self
                .common
                .current_window_state()
                .keyboard_state
                .is_key_down(vk)
            {
                self.common.keyboard_state_mut().current_virtual_keycode = Some(vk).into();
            }
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
        self.push_hit_test_latched(hit_test);
    }

    /// Push a hit test into the hover manager THROUGH THE DRAG LATCH.
    ///
    /// A button press puts the pointer under an X11 implicit passive grab, so
    /// motion and the release keep arriving after the cursor crosses the
    /// window edge — but the server ALSO sends a `LeaveNotify`, and hit-testing
    /// a position outside the window answers with nothing. Either one landing
    /// in the hover manager retargets every remaining Drag / DragOver /
    /// DragEnd / Drop of the gesture at the root node and fires `MouseLeave`
    /// down the whole hovered chain: as far as the app can tell the drag ended,
    /// while the user is still holding the button. Dragging a selection or a
    /// node one pixel past the edge — the normal way to drag onto something
    /// near the border, and the normal way to trigger drag-autoscroll — was
    /// enough.
    ///
    /// The latch is deliberately narrow. It only refuses an EMPTY hit test,
    /// and only while a button is held: a drag INSIDE the window still
    /// re-targets on every move (the drop target has to follow the cursor),
    /// and an ordinary pointer-out with no button held still clears the hover
    /// chain as before.
    fn push_hit_test_latched(&mut self, hit_test: FullHitTest) {
        let ms = &self.common.current_window_state().mouse_state;
        let any_button_down = ms.left_down || ms.right_down || ms.middle_down;
        let Some(ref mut layout_window) = self.common.layout_window else {
            return;
        };
        let standing = layout_window.hover_manager.get_current(&InputPointId::Mouse);
        if let Some(next) = latched_hit_test(any_button_down, standing, hit_test) {
            layout_window
                .hover_manager
                .push_hit_test(InputPointId::Mouse, next);
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
        self.snapshot_window_state_baseline("x11.handle_file_drag_entered");
        self.common.mouse_state_mut().cursor_position =
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
        self.snapshot_window_state_baseline("x11.handle_file_drag_exited");
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
        self.snapshot_window_state_baseline("x11.handle_file_drop");
        self.common.mouse_state_mut().cursor_position =
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
        let parent_pos = match self.common.current_window_state().position {
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

// Pressed-key bookkeeping

/// Apply one KeyPress/KeyRelease to the pressed-key lists, keyed by the
/// PHYSICAL keycode.
///
/// A press records the code it inserted in `pressed_key_vks`; the release
/// removes THAT code rather than re-translating its own keysym, because the two
/// halves of one physical key do not have to agree on a keysym (see the field's
/// doc on `X11Window`). `.or(vk)` covers a key whose press this window never
/// saw — held across a focus change, or pressed before the map existed.
///
/// A keycode that resolves to no `VirtualKeyCode` inserts nothing and therefore
/// needs nothing removed, which is also why the scancode is tracked inside the
/// same branch: the release of an unresolvable key must not delete state the
/// press never wrote.
/// Fold the `state` bitmask every X event carries into the engine's held-key
/// list, using the modifier bits THIS keyboard actually maps.
///
/// `Mod1 = Alt` / `Mod4 = Super` are only the common case and AltGr had no
/// default at all, so the masks come from `XGetModifierMapping`
/// (`query_modifier_masks`) and are refreshed on `MappingNotify`.
///
/// The `state` bits say WHETHER a modifier is held, never WHICH side — so the
/// left key is only synthesized when neither side is already recorded.
/// Inserting it unconditionally left a phantom `LShift`/`LControl` behind
/// whenever the user actually held the RIGHT key and released it: the keysym
/// path removed `RShift`, and this one had just re-added `LShift`.
fn apply_modifier_mask_state(
    keyboard_state: &mut azul_core::window::KeyboardState,
    masks: super::ModifierMasks,
    state: std::ffi::c_uint,
) {
    let alt_down = masks.alt != 0 && (state & masks.alt) != 0;
    {
        let mut sync = |down: bool, left: VirtualKeyCode, right: VirtualKeyCode| {
            let already = keyboard_state.is_key_down(left) || keyboard_state.is_key_down(right);
            if down {
                if !already {
                    keyboard_state.pressed_virtual_keycodes.insert_hm_item(left);
                }
            } else {
                keyboard_state.pressed_virtual_keycodes.remove_hm_item(&left);
                keyboard_state.pressed_virtual_keycodes.remove_hm_item(&right);
            }
        };

        sync(
            (state & SHIFT_MASK) != 0,
            VirtualKeyCode::LShift,
            VirtualKeyCode::RShift,
        );
        sync(
            (state & CONTROL_MASK) != 0,
            VirtualKeyCode::LControl,
            VirtualKeyCode::RControl,
        );
        sync(alt_down, VirtualKeyCode::LAlt, VirtualKeyCode::RAlt);
        sync(
            masks.super_key != 0 && (state & masks.super_key) != 0,
            VirtualKeyCode::LWin,
            VirtualKeyCode::RWin,
        );
    }

    // AltGr (ISO_Level3_Shift) lives on its own modifier bit (usually Mod5) and
    // was invisible here, so AltGr-composed accelerators saw no modifier at
    // all. It shares RAlt with plain right-Alt (as everywhere else in the
    // engine), so only touch it when Alt itself is not the source of that key.
    if masks.altgr != 0 && (state & masks.altgr) != 0 {
        if !keyboard_state.is_key_down(VirtualKeyCode::RAlt) {
            keyboard_state
                .pressed_virtual_keycodes
                .insert_hm_item(VirtualKeyCode::RAlt);
        }
    } else if !alt_down {
        keyboard_state
            .pressed_virtual_keycodes
            .remove_hm_item(&VirtualKeyCode::RAlt);
    }
}

/// Does this hit test name any node at all?
///
/// Stricter than [`FullHitTest::is_empty`], which only asks whether the
/// per-DOM map has entries: a hit test carrying a DOM whose own node maps are
/// all empty names nothing either, and treating it as a target would latch the
/// drag onto nothing.
fn hit_test_names_nothing(hit_test: &FullHitTest) -> bool {
    hit_test.hovered_nodes.values().all(HitTest::is_empty)
}

/// THE drag latch: what to push into the hover manager, or `None` to keep the
/// hit test already standing.
///
/// See [`X11Window::push_hit_test_latched`] for why.
pub(super) fn latched_hit_test(
    any_button_down: bool,
    standing: Option<&FullHitTest>,
    incoming: FullHitTest,
) -> Option<FullHitTest> {
    if !any_button_down || !hit_test_names_nothing(&incoming) {
        return Some(incoming);
    }
    // A drag is in flight and the incoming hit test names nothing. Hold the
    // standing target — but only if there IS one; latching onto an equally
    // empty predecessor would just hide the fact that nothing was ever hit.
    match standing {
        Some(standing) if !hit_test_names_nothing(standing) => None,
        _ => Some(incoming),
    }
}

pub(super) fn apply_key_state_change(
    keyboard_state: &mut azul_core::window::KeyboardState,
    pressed_key_vks: &mut std::collections::BTreeMap<u32, VirtualKeyCode>,
    keycode: u32,
    vk: Option<VirtualKeyCode>,
    is_down: bool,
) {
    if is_down {
        if let Some(vk) = vk {
            keyboard_state.pressed_virtual_keycodes.insert_hm_item(vk);
            keyboard_state.current_virtual_keycode = Some(vk).into();

            // Track scancode (X11 keycode is the scancode)
            keyboard_state.pressed_scancodes.insert_hm_item(keycode);
            pressed_key_vks.insert(keycode, vk);
        }
    } else if let Some(vk) = pressed_key_vks.remove(&keycode).or(vk) {
        keyboard_state.pressed_virtual_keycodes.remove_hm_item(&vk);
        keyboard_state.current_virtual_keycode = None.into();

        // Remove scancode
        keyboard_state.pressed_scancodes.remove_hm_item(&keycode);
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
        // The digit row folds its shifted form onto the same code for the same
        // press/release-symmetry reason as the punctuation block below: the
        // release of `1` arrives as XK_exclam once Shift is down, and an
        // unmapped release leaves Key1 stuck in `pressed_virtual_keycodes`.
        XK_0 | XK_parenright => Some(VirtualKeyCode::Key0),
        XK_1 | XK_exclam => Some(VirtualKeyCode::Key1),
        XK_2 | XK_at => Some(VirtualKeyCode::Key2),
        XK_3 | XK_numbersign => Some(VirtualKeyCode::Key3),
        XK_4 | XK_dollar => Some(VirtualKeyCode::Key4),
        XK_5 | XK_percent => Some(VirtualKeyCode::Key5),
        XK_6 | XK_asciicircum => Some(VirtualKeyCode::Key6),
        XK_7 | XK_ampersand => Some(VirtualKeyCode::Key7),
        XK_8 | XK_asterisk => Some(VirtualKeyCode::Key8),
        XK_9 | XK_parenleft => Some(VirtualKeyCode::Key9),
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
        XK_Alt_L | XK_Meta_L => Some(VirtualKeyCode::LAlt),
        XK_Alt_R | XK_Meta_R => Some(VirtualKeyCode::RAlt),
        XK_Super_L | XK_Hyper_L => Some(VirtualKeyCode::LWin),
        XK_Super_R | XK_Hyper_R => Some(VirtualKeyCode::RWin),
        // AltGr. X11 has no dedicated code for it; RAlt is where every other
        // backend lands the third-level shift.
        XK_ISO_Level3_Shift | XK_Mode_switch => Some(VirtualKeyCode::RAlt),
        XK_Caps_Lock | XK_Shift_Lock => Some(VirtualKeyCode::Capital),
        XK_Num_Lock => Some(VirtualKeyCode::Numlock),
        XK_Menu => Some(VirtualKeyCode::Apps),
        XK_Print => Some(VirtualKeyCode::Snapshot),
        XK_Sys_Req => Some(VirtualKeyCode::Sysrq),

        // Punctuation / OEM keys. Both the plain AND the shifted keysym of one
        // physical key must map to the SAME code: the press is recorded with
        // Shift held and the release usually is not, and a mismatch leaves the
        // key stuck in `pressed_virtual_keycodes` forever. Without these,
        // VirtualKeyDown never fired for them at all — Ctrl+`-` / Ctrl+`=`
        // (zoom) were dead on X11 while passing headlessly.
        XK_minus | XK_underscore => Some(VirtualKeyCode::Minus),
        XK_equal | XK_plus => Some(VirtualKeyCode::Equals),
        XK_comma | XK_less => Some(VirtualKeyCode::Comma),
        XK_period | XK_greater => Some(VirtualKeyCode::Period),
        XK_semicolon | XK_colon => Some(VirtualKeyCode::Semicolon),
        XK_apostrophe | XK_quotedbl => Some(VirtualKeyCode::Apostrophe),
        XK_grave | XK_asciitilde => Some(VirtualKeyCode::Grave),
        XK_bracketleft | XK_braceleft => Some(VirtualKeyCode::LBracket),
        XK_bracketright | XK_braceright => Some(VirtualKeyCode::RBracket),
        XK_backslash | XK_bar => Some(VirtualKeyCode::Backslash),
        XK_slash | XK_question => Some(VirtualKeyCode::Slash),

        // Keypad. Each key has two keysyms — the digit with Num Lock on, the
        // navigation function with it off — and both must fold to one code for
        // the same press/release symmetry reason.
        XK_KP_0 | XK_KP_Insert => Some(VirtualKeyCode::Numpad0),
        XK_KP_1 | XK_KP_End => Some(VirtualKeyCode::Numpad1),
        XK_KP_2 | XK_KP_Down => Some(VirtualKeyCode::Numpad2),
        XK_KP_3 | XK_KP_Page_Down => Some(VirtualKeyCode::Numpad3),
        XK_KP_4 | XK_KP_Left => Some(VirtualKeyCode::Numpad4),
        XK_KP_5 | XK_KP_Begin => Some(VirtualKeyCode::Numpad5),
        XK_KP_6 | XK_KP_Right => Some(VirtualKeyCode::Numpad6),
        XK_KP_7 | XK_KP_Home => Some(VirtualKeyCode::Numpad7),
        XK_KP_8 | XK_KP_Up => Some(VirtualKeyCode::Numpad8),
        XK_KP_9 | XK_KP_Page_Up => Some(VirtualKeyCode::Numpad9),
        XK_KP_Decimal | XK_KP_Delete => Some(VirtualKeyCode::NumpadDecimal),
        XK_KP_Separator => Some(VirtualKeyCode::NumpadComma),
        XK_KP_Enter => Some(VirtualKeyCode::NumpadEnter),
        XK_KP_Add => Some(VirtualKeyCode::NumpadAdd),
        XK_KP_Subtract => Some(VirtualKeyCode::NumpadSubtract),
        XK_KP_Multiply => Some(VirtualKeyCode::NumpadMultiply),
        XK_KP_Divide => Some(VirtualKeyCode::NumpadDivide),
        XK_KP_Equal => Some(VirtualKeyCode::NumpadEquals),
        XK_KP_Space => Some(VirtualKeyCode::Space),
        XK_KP_Tab => Some(VirtualKeyCode::Tab),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use azul_core::window::KeyboardState;

    use super::*;

    type PressedKeyVks = std::collections::BTreeMap<u32, VirtualKeyCode>;

    // --- drag latch (C2) ---

    /// A hit test naming one node of DOM 0 — a drag target.
    fn hit_on(node: usize) -> FullHitTest {
        use azul_core::hit_test::HitTestItem;
        let mut nodes = azul_core::hit_test::HitTest::empty();
        nodes.regular_hit_test_nodes.insert(
            NodeId::new(node),
            HitTestItem {
                point_in_viewport: LogicalPosition::zero(),
                point_relative_to_item: LogicalPosition::zero(),
                is_focusable: false,
                is_virtual_view_hit: None,
                hit_depth: 0,
            },
        );
        let mut hit = FullHitTest::empty(None);
        hit.hovered_nodes.insert(DomId { inner: 0 }, nodes);
        hit
    }

    /// A hit test that carries a DOM but names no node in it — what an
    /// out-of-bounds position can answer with.
    fn hit_on_nothing_but_carrying_a_dom() -> FullHitTest {
        let mut hit = FullHitTest::empty(None);
        hit.hovered_nodes
            .insert(DomId { inner: 0 }, azul_core::hit_test::HitTest::empty());
        hit
    }

    /// THE defect: dragging one pixel past the window edge is a NotifyNormal
    /// crossing (the grab-mode filter cannot catch it, and must not), and the
    /// empty hit test it brings used to replace the drag's target — retargeting
    /// every remaining Drag / DragEnd / Drop at the root node while the user
    /// was still holding the button.
    ///
    /// NEGATIVE CONTROL: the pre-fix behaviour — make `latched_hit_test`
    /// return `Some(incoming)` unconditionally, so nothing is ever latched.
    #[test]
    fn a_drag_leaving_the_window_keeps_its_target() {
        let standing = hit_on(7);
        assert_eq!(
            latched_hit_test(true, Some(&standing), FullHitTest::empty(None)),
            None,
            "the drag must keep the node it started on"
        );
        assert_eq!(
            latched_hit_test(true, Some(&standing), hit_on_nothing_but_carrying_a_dom()),
            None,
            "a hit test naming no node is just as empty as one carrying no DOM"
        );
    }

    /// The latch must not freeze the target: a drag INSIDE the window still
    /// re-targets on every move, or the drop target would never follow the
    /// cursor and text selection could not extend.
    ///
    /// NEGATIVE CONTROL: make `latched_hit_test` return `None` whenever
    /// `any_button_down` — this reports `None` instead of the new target.
    #[test]
    fn a_drag_inside_the_window_still_follows_the_cursor() {
        let standing = hit_on(7);
        assert_eq!(
            latched_hit_test(true, Some(&standing), hit_on(9)),
            Some(hit_on(9))
        );
    }

    /// With no button held, a pointer-out clears the hover chain exactly as
    /// before — `MouseLeave` and the `:hover` restyle depend on it.
    #[test]
    fn a_plain_pointer_out_still_clears_the_hover_chain() {
        let standing = hit_on(7);
        assert_eq!(
            latched_hit_test(false, Some(&standing), FullHitTest::empty(None)),
            Some(FullHitTest::empty(None))
        );
    }

    /// Nothing to hold on to: a drag that never had a target must not latch
    /// onto an equally empty predecessor and hide that fact.
    #[test]
    fn a_drag_with_no_standing_target_does_not_latch() {
        assert_eq!(
            latched_hit_test(true, None, FullHitTest::empty(None)),
            Some(FullHitTest::empty(None))
        );
        let empty = FullHitTest::empty(None);
        assert_eq!(
            latched_hit_test(true, Some(&empty), FullHitTest::empty(None)),
            Some(FullHitTest::empty(None))
        );
    }

    /// Both of the backend's hover-manager writes go through the latch. A
    /// second, unlatched `push_hit_test` would reopen the defect at whichever
    /// site it lives on, and the crossing handler is exactly where it used to.
    ///
    /// NEGATIVE CONTROL: restore the inline
    /// `layout_window.hover_manager.push_hit_test(InputPointId::Mouse,
    /// FullHitTest::empty(None))` in `handle_mouse_crossing`.
    #[test]
    fn every_hover_manager_write_in_this_file_goes_through_the_latch() {
        let source = include_str!("events.rs");
        let body = source
            .split_once("mod tests {")
            .map_or(source, |(before, _)| before);

        assert!(
            body.matches("push_hit_test_latched").count() >= 3,
            "the latch must be defined and used at BOTH the hit-test update \
             and the crossing-leave"
        );
        assert_eq!(
            body.matches(".push_hit_test(").count(),
            1,
            "exactly one raw hover_manager.push_hit_test may remain — the one \
             INSIDE push_hit_test_latched"
        );
    }

    fn press(state: &mut KeyboardState, map: &mut PressedKeyVks, keycode: u32, keysym: u32) {
        apply_key_state_change(
            state,
            map,
            keycode,
            keysym_to_virtual_keycode(keysym as KeySym),
            true,
        );
    }

    fn release(state: &mut KeyboardState, map: &mut PressedKeyVks, keycode: u32, keysym: u32) {
        apply_key_state_change(
            state,
            map,
            keycode,
            keysym_to_virtual_keycode(keysym as KeySym),
            false,
        );
    }

    /// The premise of everything below: one physical key legitimately reports
    /// two unrelated keysyms, which the table maps to two unrelated codes.
    /// `XLookupString` returns the EFFECTIVE symbol, so German AltGr+Q is
    /// `XK_at` while AltGr is down and `XK_q` after it comes up — and the
    /// folding that rescues Shift+digit (`XK_1`/`XK_exclam` → one code) cannot
    /// help here, because `XK_at` is already folded onto the digit row.
    #[test]
    fn one_physical_key_can_report_two_different_codes() {
        assert_eq!(
            keysym_to_virtual_keycode(XK_at as KeySym),
            Some(VirtualKeyCode::Key2)
        );
        assert_eq!(
            keysym_to_virtual_keycode(XK_q as KeySym),
            Some(VirtualKeyCode::Q)
        );
        assert_eq!(
            keysym_to_virtual_keycode(XK_1 as KeySym),
            keysym_to_virtual_keycode(XK_exclam as KeySym),
            "the shifted digit row folds onto one code, which is why the digit \
             case never needed the press→code map"
        );
    }

    /// AltGr+Q pressed, AltGr released first, Q released: the release must undo
    /// the press even though the two keysyms disagree.
    ///
    /// NEGATIVE CONTROL: resolving the release from its own keysym
    /// (`apply_key_state_change`'s `pressed_key_vks.remove(&keycode).or(vk)`
    /// reduced to `vk`) removes Q — which was never pressed — and leaves Key2
    /// latched for the life of the window.
    #[test]
    fn a_level_three_press_and_release_cancel_out() {
        // German keycode 24 = the Q key.
        const Q_KEYCODE: u32 = 24;
        let mut state = KeyboardState::default();
        let mut map = std::collections::BTreeMap::new();

        press(&mut state, &mut map, Q_KEYCODE, XK_at);
        assert!(state.is_key_down(VirtualKeyCode::Key2));

        release(&mut state, &mut map, Q_KEYCODE, XK_q);
        assert!(
            state.pressed_virtual_keycodes.as_ref().is_empty(),
            "the release must remove the code the PRESS inserted, not the one \
             its own keysym resolves to: {:?}",
            state.pressed_virtual_keycodes.as_ref()
        );
        assert!(state.pressed_scancodes.as_ref().is_empty());
        assert!(map.is_empty(), "the record must not outlive the key");
    }

    /// The harm the asymmetry does: engine modifiers are DERIVED from
    /// `pressed_virtual_keycodes`, so a modifier that is never removed makes the
    /// whole app behave as if it were held down forever.
    ///
    /// NEGATIVE CONTROL: same reduction as above — the release resolves to no
    /// code at all, removes nothing, and `ctrl_down()` stays true.
    #[test]
    fn a_release_that_resolves_to_nothing_still_lifts_the_modifier() {
        const CONTROL_KEYCODE: u32 = 37;
        let mut state = KeyboardState::default();
        let mut map = std::collections::BTreeMap::new();

        press(&mut state, &mut map, CONTROL_KEYCODE, XK_Control_L);
        assert!(state.ctrl_down());

        // A remapped/unbound level on the same physical key: no keysym the
        // table knows.
        release(&mut state, &mut map, CONTROL_KEYCODE, 0);
        assert!(
            !state.ctrl_down(),
            "Ctrl must come up when its physical key does — a latched modifier \
             rewrites every subsequent click and keystroke"
        );
    }

    /// Two physical keys held at once are tracked independently: releasing one
    /// must not disturb the other, whatever keysym either release carries.
    ///
    /// NEGATIVE CONTROL: same reduction as above — the `1` release resolves to
    /// Key1 and the `q` release to Q, so Key2 survives both.
    #[test]
    fn two_keys_held_at_once_are_tracked_per_keycode() {
        const Q_KEYCODE: u32 = 24;
        const ONE_KEYCODE: u32 = 10;
        let mut state = KeyboardState::default();
        let mut map = std::collections::BTreeMap::new();

        press(&mut state, &mut map, Q_KEYCODE, XK_at);
        press(&mut state, &mut map, ONE_KEYCODE, XK_exclam);
        assert_eq!(state.pressed_virtual_keycodes.as_ref().len(), 2);

        release(&mut state, &mut map, ONE_KEYCODE, XK_1);
        assert!(state.is_key_down(VirtualKeyCode::Key2));
        assert!(!state.is_key_down(VirtualKeyCode::Key1));

        release(&mut state, &mut map, Q_KEYCODE, XK_q);
        assert!(state.pressed_virtual_keycodes.as_ref().is_empty());
    }

    /// A keysym with no `VirtualKeyCode` (`ö`, a dead key, F13 on a keyboard
    /// that has one) must not invent state on the press, and must therefore
    /// find nothing to undo on the release — including the scancode, which the
    /// press never wrote either.
    #[test]
    fn an_unresolvable_key_writes_nothing_and_undoes_nothing() {
        const SPARE_KEYCODE: u32 = 47;
        let mut state = KeyboardState::default();
        let mut map = std::collections::BTreeMap::new();

        press(&mut state, &mut map, SPARE_KEYCODE, 0);
        assert!(state.pressed_virtual_keycodes.as_ref().is_empty());
        assert!(state.pressed_scancodes.as_ref().is_empty());
        assert!(map.is_empty());

        // A live key held alongside it must survive the unresolvable release.
        press(&mut state, &mut map, 24, XK_q);
        release(&mut state, &mut map, SPARE_KEYCODE, 0);
        assert!(state.is_key_down(VirtualKeyCode::Q));
    }

    /// A key whose press this window never saw — held across a focus change, or
    /// down before `resync_keyboard_state_from_vector` seeded the map — still
    /// has to be removable, so the release falls back to its own keysym when
    /// the map has no entry.
    #[test]
    fn a_release_without_a_recorded_press_falls_back_to_its_keysym() {
        let mut state = KeyboardState::default();
        let mut map = std::collections::BTreeMap::new();

        state
            .pressed_virtual_keycodes
            .insert_hm_item(VirtualKeyCode::LShift);
        release(&mut state, &mut map, 50, XK_Shift_L);
        assert!(!state.is_key_down(VirtualKeyCode::LShift));
    }

    fn vk(keysym: u32) -> Option<VirtualKeyCode> {
        keysym_to_virtual_keycode(keysym as KeySym)
    }

    /// Punctuation carries the accelerators users actually press: `Ctrl+-` /
    /// `Ctrl+=` (zoom), `Ctrl+,` (preferences), `Ctrl+/` (comment). With the
    /// row unmapped, `VirtualKeyDown` never fired for any of them on X11 while
    /// the same shortcut passed headlessly.
    ///
    /// NEGATIVE CONTROL: delete the `XK_minus | XK_underscore =>` arm (or any
    /// other single arm named here) — that pair resolves to `None` and this
    /// fails.
    #[test]
    fn the_punctuation_row_is_mapped() {
        assert_eq!(vk(XK_minus), Some(VirtualKeyCode::Minus));
        assert_eq!(vk(XK_equal), Some(VirtualKeyCode::Equals));
        assert_eq!(vk(XK_comma), Some(VirtualKeyCode::Comma));
        assert_eq!(vk(XK_period), Some(VirtualKeyCode::Period));
        assert_eq!(vk(XK_semicolon), Some(VirtualKeyCode::Semicolon));
        assert_eq!(vk(XK_apostrophe), Some(VirtualKeyCode::Apostrophe));
        assert_eq!(vk(XK_grave), Some(VirtualKeyCode::Grave));
        assert_eq!(vk(XK_bracketleft), Some(VirtualKeyCode::LBracket));
        assert_eq!(vk(XK_bracketright), Some(VirtualKeyCode::RBracket));
        assert_eq!(vk(XK_backslash), Some(VirtualKeyCode::Backslash));
        assert_eq!(vk(XK_slash), Some(VirtualKeyCode::Slash));
    }

    /// X11 keysyms are shift-DEPENDENT: the press of `-` with Shift held
    /// arrives as `XK_underscore` and its release as `XK_minus` once Shift is
    /// up. Mapping the two forms to different codes would insert one and remove
    /// the other, leaving the key held forever in `pressed_virtual_keycodes` —
    /// from which the engine derives ctrl/shift/alt.
    ///
    /// NEGATIVE CONTROL: split any pair, e.g. `XK_plus => Some(VirtualKeyCode::Plus)`
    /// as its own arm — the `equal`/`plus` assertion fails.
    #[test]
    fn every_shifted_form_folds_onto_its_unshifted_code() {
        for (plain, shifted, name) in [
            (XK_minus, XK_underscore, "minus/underscore"),
            (XK_equal, XK_plus, "equal/plus"),
            (XK_comma, XK_less, "comma/less"),
            (XK_period, XK_greater, "period/greater"),
            (XK_semicolon, XK_colon, "semicolon/colon"),
            (XK_apostrophe, XK_quotedbl, "apostrophe/quotedbl"),
            (XK_grave, XK_asciitilde, "grave/asciitilde"),
            (XK_bracketleft, XK_braceleft, "bracketleft/braceleft"),
            (XK_bracketright, XK_braceright, "bracketright/braceright"),
            (XK_backslash, XK_bar, "backslash/bar"),
            (XK_slash, XK_question, "slash/question"),
            (XK_1, XK_exclam, "1/exclam"),
            (XK_9, XK_parenleft, "9/parenleft"),
        ] {
            let (a, b) = (vk(plain), vk(shifted));
            assert!(a.is_some(), "{name}: the unshifted form must map");
            assert_eq!(a, b, "{name}: press and release must resolve to one code");
        }
    }

    /// The keypad has two keysym families — the digit with Num Lock on, the
    /// navigation function with it off — and the same press/release argument
    /// applies, so both fold onto one code.
    ///
    /// NEGATIVE CONTROL: drop `| XK_KP_End` from the `XK_KP_1` arm — the
    /// numlock-off form resolves to `None`.
    #[test]
    fn the_keypad_is_mapped_at_both_numlock_levels() {
        assert_eq!(vk(XK_KP_0), Some(VirtualKeyCode::Numpad0));
        assert_eq!(vk(XK_KP_Insert), Some(VirtualKeyCode::Numpad0));
        assert_eq!(vk(XK_KP_1), Some(VirtualKeyCode::Numpad1));
        assert_eq!(vk(XK_KP_End), Some(VirtualKeyCode::Numpad1));
        assert_eq!(vk(XK_KP_5), Some(VirtualKeyCode::Numpad5));
        assert_eq!(vk(XK_KP_Begin), Some(VirtualKeyCode::Numpad5));
        assert_eq!(vk(XK_KP_9), Some(VirtualKeyCode::Numpad9));
        assert_eq!(vk(XK_KP_Page_Up), Some(VirtualKeyCode::Numpad9));
        assert_eq!(vk(XK_KP_Decimal), Some(VirtualKeyCode::NumpadDecimal));
        assert_eq!(vk(XK_KP_Delete), Some(VirtualKeyCode::NumpadDecimal));
        assert_eq!(vk(XK_KP_Enter), Some(VirtualKeyCode::NumpadEnter));
        assert_eq!(vk(XK_KP_Add), Some(VirtualKeyCode::NumpadAdd));
        assert_eq!(vk(XK_KP_Subtract), Some(VirtualKeyCode::NumpadSubtract));
        assert_eq!(vk(XK_KP_Multiply), Some(VirtualKeyCode::NumpadMultiply));
        assert_eq!(vk(XK_KP_Divide), Some(VirtualKeyCode::NumpadDivide));
    }

    /// AltGr is the third-level shift every non-US layout types `@`, `€` and
    /// `\` with; X11 has no dedicated code for it, so it lands on RAlt like
    /// everywhere else in the engine. Lock keys and Menu were likewise absent.
    ///
    /// NEGATIVE CONTROL: delete the
    /// `XK_ISO_Level3_Shift | XK_Mode_switch => Some(VirtualKeyCode::RAlt)` arm.
    #[test]
    fn altgr_lock_keys_and_menu_are_mapped() {
        assert_eq!(vk(XK_ISO_Level3_Shift), Some(VirtualKeyCode::RAlt));
        assert_eq!(vk(XK_Mode_switch), Some(VirtualKeyCode::RAlt));
        assert_eq!(vk(XK_Num_Lock), Some(VirtualKeyCode::Numlock));
        assert_eq!(vk(XK_Caps_Lock), Some(VirtualKeyCode::Capital));
        assert_eq!(vk(XK_Shift_Lock), Some(VirtualKeyCode::Capital));
        assert_eq!(vk(XK_Menu), Some(VirtualKeyCode::Apps));
        assert_eq!(vk(XK_Print), Some(VirtualKeyCode::Snapshot));
    }

    /// A keysym the table does not know must stay `None`. Wayland's deleted
    /// copy of this table answered `Escape` instead, so every unmapped key
    /// dismissed menus.
    #[test]
    fn an_unknown_keysym_is_none_not_escape() {
        assert_eq!(vk(0), None);
        assert_eq!(vk(0x0100_0000), None);
    }

    fn masks(alt: std::ffi::c_uint, super_key: std::ffi::c_uint, altgr: std::ffi::c_uint)
        -> super::super::ModifierMasks
    {
        super::super::ModifierMasks { alt, super_key, altgr }
    }

    /// The `state` bitmask says a modifier is held, never which side. A user
    /// holding RIGHT Shift used to end up with a phantom LShift that no release
    /// could ever lift — the keysym path removed RShift, and the next pointer
    /// event re-added LShift.
    ///
    /// NEGATIVE CONTROL: drop the `if !already` guard around the insert —
    /// `LShift` appears and the first assertion fails.
    #[test]
    fn a_held_right_modifier_grows_no_phantom_left_one() {
        let mut state = KeyboardState::default();
        state
            .pressed_virtual_keycodes
            .insert_hm_item(VirtualKeyCode::RShift);

        apply_modifier_mask_state(&mut state, masks(MOD1_MASK, MOD4_MASK, 0), SHIFT_MASK);
        assert!(
            !state.is_key_down(VirtualKeyCode::LShift),
            "the bitmask must not invent the side"
        );
        assert!(state.is_key_down(VirtualKeyCode::RShift));

        apply_modifier_mask_state(&mut state, masks(MOD1_MASK, MOD4_MASK, 0), 0);
        assert!(!state.shift_down(), "a cleared bit lifts BOTH sides");
    }

    /// Nothing held and the bit set: the left key stands in, so accelerators
    /// still see the modifier.
    #[test]
    fn a_set_bit_with_nothing_recorded_synthesizes_the_left_key() {
        let mut state = KeyboardState::default();
        apply_modifier_mask_state(&mut state, masks(MOD1_MASK, MOD4_MASK, 0), CONTROL_MASK);
        assert!(state.ctrl_down());
        assert!(state.is_key_down(VirtualKeyCode::LControl));
    }

    /// AltGr (usually Mod5) had no mask at all, so an AltGr-composed
    /// accelerator saw no modifier.
    ///
    /// NEGATIVE CONTROL: replace the `masks.altgr != 0 && (state & masks.altgr) != 0`
    /// condition with `false` — RAlt never appears.
    #[test]
    fn altgr_is_read_from_its_own_modifier_bit() {
        let m = masks(MOD1_MASK, MOD4_MASK, MOD5_MASK);
        let mut state = KeyboardState::default();

        apply_modifier_mask_state(&mut state, m, MOD5_MASK);
        assert!(state.is_key_down(VirtualKeyCode::RAlt));

        apply_modifier_mask_state(&mut state, m, 0);
        assert!(!state.alt_down());
    }

    /// WHICH `ModN` bit carries Alt / Super is a per-keyboard mapping;
    /// Mod1/Mod4 are only the common case. Hardcoding them made every
    /// remapped keyboard report the wrong modifiers.
    ///
    /// NEGATIVE CONTROL: substitute the literal `MOD1_MASK` for `masks.alt` in
    /// `apply_modifier_mask_state` — Mod1 is read as Alt and the first
    /// assertion fails.
    #[test]
    fn alt_follows_the_keyboards_own_modifier_map() {
        let m = masks(MOD3_MASK, MOD4_MASK, 0);
        let mut state = KeyboardState::default();

        apply_modifier_mask_state(&mut state, m, MOD1_MASK);
        assert!(!state.alt_down(), "Mod1 is not Alt on this keyboard");

        apply_modifier_mask_state(&mut state, m, MOD3_MASK);
        assert!(state.alt_down());
    }

    /// `XBufferOverflow` writes NOTHING and returns the size the commit needs.
    /// The fixed 32-byte buffer overflows on any phrase past ~11 CJK
    /// characters, and the untested status left the caller with a count it
    /// never used — the whole composed sentence vanished.
    ///
    /// NEGATIVE CONTROL: delete the `if status == XBufferOverflow && count > 0`
    /// retry block — `lookup_string_with` returns `None` and this fails.
    #[test]
    fn an_ime_commit_larger_than_the_stack_buffer_is_not_dropped() {
        let phrase = "\u{3053}\u{308c}\u{306f}\u{65e5}\u{672c}\u{8a9e}\u{306e}\
                      \u{9577}\u{3044}\u{6587}\u{7ae0}\u{3067}\u{3059}";
        let bytes = phrase.as_bytes().to_vec();
        assert!(bytes.len() > 32, "the premise: the commit must overflow");

        let mut calls = 0usize;
        let (text, keysym) = lookup_string_with(|buf, len, ks, st| {
            calls += 1;
            unsafe {
                *ks = XK_a as KeySym;
                if (len as usize) < bytes.len() {
                    *st = XBufferOverflow;
                    return bytes.len() as i32;
                }
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr() as *const c_char,
                    buf,
                    bytes.len(),
                );
                *st = XLookupChars;
            }
            bytes.len() as i32
        });

        assert_eq!(calls, 2, "the overflow must be retried into a heap buffer");
        assert_eq!(text.as_deref(), Some(phrase));
        assert_eq!(keysym, Some(XK_a as KeySym));
    }

    /// The ordinary case must not pay for the retry, and the text is sliced by
    /// the returned count rather than by a NUL that Xlib need not write.
    #[test]
    fn a_short_commit_takes_one_call_and_is_sliced_by_count() {
        let mut calls = 0usize;
        let (text, _) = lookup_string_with(|buf, _len, ks, st| {
            calls += 1;
            unsafe {
                *ks = XK_a as KeySym;
                std::ptr::copy_nonoverlapping(b"ab".as_ptr() as *const c_char, buf, 2);
                *st = XLookupChars;
            }
            2
        });
        assert_eq!(calls, 1);
        assert_eq!(text.as_deref(), Some("ab"));
    }

    /// A keysym-only lookup (Escape, arrows) reports no text — taking the
    /// untouched buffer as a string would insert tofu into the document.
    #[test]
    fn a_keysym_only_lookup_yields_no_text() {
        let (text, keysym) = lookup_string_with(|_buf, _len, ks, st| {
            unsafe {
                *ks = XK_Escape as KeySym;
                *st = XLookupKeySym;
            }
            0
        });
        assert_eq!(text, None);
        assert_eq!(keysym, Some(XK_Escape as KeySym));
    }

    /// An input method that reports a second overflow lied about the size;
    /// returning the untouched heap buffer as text would emit NUL bytes.
    #[test]
    fn a_lying_input_method_produces_no_text() {
        let (text, _) = lookup_string_with(|_buf, _len, ks, st| {
            unsafe {
                *ks = XK_a as KeySym;
                *st = XBufferOverflow;
            }
            64
        });
        assert_eq!(text, None);
    }
}
