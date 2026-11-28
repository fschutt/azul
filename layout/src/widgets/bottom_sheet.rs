//! The BottomSheet: a panel that floats over the lower part of the box it is in and is dragged
//! between DETENTS - a map's place card and its results, a code editor's terminal panel.
//!
//! ```text
//!   the container (the app's window)
//!  ┌──────────────────────────────────┐
//!  │ the map, the editor ...          │
//!  │                                  │
//!  ├────────────── ▬▬▬ ───────────────┤  Full:      85 % of the container
//!  │ the sheet's content              │  Half:      half of the container
//!  │                                  │  Peek:      a summary (130 px)
//!  ├──────────────────────────────────┤  Collapsed: the grip and one line (30 px)
//!  └──────────────────────────────────┘
//! ```
//!
//! The sheet is an OVERLAY, not a split (the Inspector's bottom pane is a split): what is under it
//! keeps its size, so a map under a sheet being dragged does not re-render its tiles for every
//! step. Put it as a child of a `position: relative` box that spans the window and tell it that
//! box's height ([`BottomSheet::with_container_height`]); it lies over the box's lower edge.
//!
//! Dragging. The GRIP at the top of the sheet (a 32 px strip with a pill) takes a finger or a
//! mouse; the band an app names with [`BottomSheet::with_grab_height`] (a title row under the grip)
//! does too, once the pointer has moved past the tap slop - a press and release in that band
//! without moving reach the buttons in it as always. On release the sheet settles on the detent
//! nearest to where a short fling would land it. A tap on the grip itself steps to the next
//! detent up (from the highest back to the lowest), and the focused grip answers the keys: Up /
//! Down step a detent, Home / End go to the highest / lowest, Enter and Space step as a tap does.
//!
//! A press that began on the sheet is followed wherever the pointer goes: the moves and the release are window-level events
//! (a finger or a mouse that outruns the sheet's top edge before the layout catches up keeps the drag; a touch is delivered to the
//! node under it only, and pointer capture is of the mouse's events only).
//!
//! The sheet owns nothing: the app keeps the detent, hears every drag step and every settling
//! through ONE callback ([`BottomSheetOnEvent`]) and rebuilds with the detent it heard (a sheet whose
//! app does not store it goes back to the detent it was built at, when the app next builds). A drag
//! in flight survives a rebuild (`merge_sheet_state`): while the pointer is down it wins. The live
//! height of a drag is a runtime override of the sheet's node (the engine's override channel: cheap
//! for every step); when the pointer is up the node's own style takes the settled height and the
//! override is handed back, so the app's next build is the truth again and a detent the app sets
//! later is obeyed. The node is keyed, so it keeps its identity (and the override of a drag in
//! flight) through a rebuild whatever the app builds around it.
//!
//! Heights. `Peek` (130 px) and `Collapsed` (30 px) are px ([`BottomSheet::with_peek_height`],
//! [`BottomSheet::with_collapsed_height`]), `Half` is half the container, `Full` 85 % of it or the
//! container less the top margin the app names (the room above the keyboard, when the app names
//! one: [`BottomSheet::with_bottom_inset`] lifts the sheet's lower edge and takes its room from the
//! detents); the four are kept in order whatever the numbers, and
//! a detent the app rules out ([`BottomSheet::with_lowest`] / [`BottomSheet::with_highest`]) is never
//! settled on. A number that is not finite is replaced by its default, never an error. The pure
//! part - the detents, where a drag settles, what the keys do - is `widgets::detents`, shared with
//! the Preview's bottom pane.
//!
//! Key types: [`BottomSheet`], [`SheetDetent`], [`BottomSheetEvent`], [`BottomSheetEventKind`],
//! [`BottomSheetOnEvent`].

use alloc::{format, string::String, vec::Vec};

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomNodeId, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec, TabIndex},
    events::{FocusEventFilter, WindowEventFilter},
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    impl_option,
    props::layout::LayoutHeight,
    props::property::{CssProperty, CssPropertyType},
    AzString,
};

use super::detents::{detent_after_key, length, step_round, Heights, CONTAINER_HEIGHT_DEFAULT, FLING_STALE_MS};
pub use super::detents::SheetDetent;
use crate::callbacks::CallbackInfo;

/// The sheet's root.
pub const BOTTOM_SHEET_CLASS: &str = "__azul-native-bottom-sheet";
/// The grip: the strip at the top of the sheet that is dragged.
pub const BOTTOM_SHEET_GRIP_CLASS: &str = "__azul-native-bottom-sheet-grip";
/// The pill in the grip.
pub const BOTTOM_SHEET_PILL_CLASS: &str = "__azul-native-bottom-sheet-pill";
/// The sheet's content.
pub const BOTTOM_SHEET_CONTENT_CLASS: &str = "__azul-native-bottom-sheet-content";
/// Added to the root while the sheet is at its collapsed detent.
pub const BOTTOM_SHEET_COLLAPSED_CLASS: &str = "__azul-native-bottom-sheet-collapsed";
/// Added to the root while the sheet is at its peek detent.
pub const BOTTOM_SHEET_PEEK_CLASS: &str = "__azul-native-bottom-sheet-peek";
/// Added to the root while the sheet is at its half detent.
pub const BOTTOM_SHEET_HALF_CLASS: &str = "__azul-native-bottom-sheet-half";
/// Added to the root while the sheet is at its full detent.
pub const BOTTOM_SHEET_FULL_CLASS: &str = "__azul-native-bottom-sheet-full";

/// The grip's height, px: a finger's strip.
pub const GRIP_HEIGHT: f32 = 32.0;
/// How far the pointer travels before a press is a drag and not a tap, px.
pub const TAP_SLOP: f32 = 8.0;
/// A move of less than this many px is no move: the same report twice.
const MOVED_LESS_THAN: f32 = 0.01;
/// What happened to the sheet.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum BottomSheetEventKind {
    /// The sheet is being dragged: `height` is where it is now. Fires for every step; an app that
    /// does not follow the drag loses nothing.
    Dragged,
    /// The sheet was let go (a tap, a key, a release): `detent` is where it rests, `height` its
    /// height there.
    Settled,
}

/// One report of the sheet.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct BottomSheetEvent {
    /// What happened.
    pub kind: BottomSheetEventKind,
    /// The detent the sheet rests at, or, while it is dragged, the one it is nearest to.
    pub detent: SheetDetent,
    /// The sheet's height now, px.
    pub height: f32,
}

/// Callback invoked for every step of a drag and for every settling.
pub type BottomSheetOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, BottomSheetEvent) -> Update;
impl_widget_callback!(
    BottomSheetOnEvent,
    OptionBottomSheetOnEvent,
    BottomSheetOnEventCallback,
    BottomSheetOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        BottomSheetOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: BOTTOM_SHEET_ON_EVENT_INVOKER,
    invoker_ty:     AzBottomSheetOnEventCallbackInvoker,
    thunk_fn:       az_bottom_sheet_on_event_callback_thunk,
    setter_fn:      AzApp_setBottomSheetOnEventCallbackInvoker,
    from_handle_fn: AzBottomSheetOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzBottomSheetOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: BottomSheetEvent ],
}

/// The `BottomSheet`: its content under a grip, at a detent.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct BottomSheet {
    /// What the sheet shows under the grip. It scrolls when it is taller than the sheet.
    pub content: Dom,
    /// The sheet's accessible name ("Place", "Panel").
    pub label: AzString,
    /// The grip's accessible name ("Resize the place sheet"); empty: the sheet's label.
    pub grip_label: AzString,
    /// Where the sheet rests.
    pub detent: SheetDetent,
    /// The lowest detent a drag may settle on.
    pub lowest: SheetDetent,
    /// The highest detent a drag may settle on.
    pub highest: SheetDetent,
    /// The height of the box the sheet floats in, px (the window's, when the sheet covers it):
    /// 0 is 800.
    pub container_height: f32,
    /// The peek detent's height, px; 0 is 130.
    pub peek_height: f32,
    /// The collapsed detent's height, px; 0 is 30.
    pub collapsed_height: f32,
    /// The room the full detent leaves above the sheet, px (the status bar, the search bar a map
    /// keeps in view); 0 is 15 % of the container (the full detent is 85 % of it).
    pub top_margin: f32,
    /// How far the sheet's lower edge lies above the container's: the on-screen keyboard. The detents are heights of the room above
    /// it (half of it, the room less the top margin): a sheet drawn up to its full height ends under the app's bar, not under the
    /// keyboard.
    pub bottom_inset: f32,
    /// The room the content keeps at its end: the home indicator, the navigation bar.
    pub safe_bottom: f32,
    /// The height of the band at the top that drags, px, the grip included; 0 is the grip alone.
    pub grab_height: f32,
    /// Every step of a drag and every settling.
    pub on_event: OptionBottomSheetOnEvent,
}

impl_option!(BottomSheet, OptionBottomSheet, copy = false, [Debug, Clone]);

impl BottomSheet {
    /// A sheet of `content` at its peek.
    #[must_use]
    pub const fn create(content: Dom) -> Self {
        Self {
            content,
            label: AzString::from_const_str(""),
            grip_label: AzString::from_const_str(""),
            detent: SheetDetent::Peek,
            lowest: SheetDetent::Collapsed,
            highest: SheetDetent::Full,
            container_height: 0.0,
            peek_height: 0.0,
            collapsed_height: 0.0,
            top_margin: 0.0,
            bottom_inset: 0.0,
            safe_bottom: 0.0,
            grab_height: 0.0,
            on_event: OptionBottomSheetOnEvent::None,
        }
    }

    /// The sheet's accessible name.
    pub fn set_label(&mut self, label: AzString) {
        self.label = label;
    }

    /// [`Self::set_label`] for the builder chain.
    #[must_use]
    pub fn with_label(mut self, label: AzString) -> Self {
        self.set_label(label);
        self
    }

    /// The grip's accessible name.
    pub fn set_grip_label(&mut self, label: AzString) {
        self.grip_label = label;
    }

    /// [`Self::set_grip_label`] for the builder chain.
    #[must_use]
    pub fn with_grip_label(mut self, label: AzString) -> Self {
        self.set_grip_label(label);
        self
    }

    /// Where the sheet rests.
    pub const fn set_detent(&mut self, detent: SheetDetent) {
        self.detent = detent;
    }

    /// [`Self::set_detent`] for the builder chain.
    #[must_use]
    pub const fn with_detent(mut self, detent: SheetDetent) -> Self {
        self.set_detent(detent);
        self
    }

    /// The lowest detent a drag may settle on.
    pub const fn set_lowest(&mut self, detent: SheetDetent) {
        self.lowest = detent;
    }

    /// [`Self::set_lowest`] for the builder chain.
    #[must_use]
    pub const fn with_lowest(mut self, detent: SheetDetent) -> Self {
        self.set_lowest(detent);
        self
    }

    /// The highest detent a drag may settle on.
    pub const fn set_highest(&mut self, detent: SheetDetent) {
        self.highest = detent;
    }

    /// [`Self::set_highest`] for the builder chain.
    #[must_use]
    pub const fn with_highest(mut self, detent: SheetDetent) -> Self {
        self.set_highest(detent);
        self
    }

    /// The height of the box the sheet floats in, px.
    pub const fn set_container_height(&mut self, height: f32) {
        self.container_height = height;
    }

    /// [`Self::set_container_height`] for the builder chain.
    #[must_use]
    pub const fn with_container_height(mut self, height: f32) -> Self {
        self.set_container_height(height);
        self
    }

    /// The peek detent's height, px.
    pub const fn set_peek_height(&mut self, height: f32) {
        self.peek_height = height;
    }

    /// [`Self::set_peek_height`] for the builder chain.
    #[must_use]
    pub const fn with_peek_height(mut self, height: f32) -> Self {
        self.set_peek_height(height);
        self
    }

    /// The collapsed detent's height, px.
    pub const fn set_collapsed_height(&mut self, height: f32) {
        self.collapsed_height = height;
    }

    /// [`Self::set_collapsed_height`] for the builder chain.
    #[must_use]
    pub const fn with_collapsed_height(mut self, height: f32) -> Self {
        self.set_collapsed_height(height);
        self
    }

    /// The room the full detent leaves above the sheet, px.
    pub const fn set_top_margin(&mut self, margin: f32) {
        self.top_margin = margin;
    }

    /// [`Self::set_top_margin`] for the builder chain.
    #[must_use]
    pub const fn with_top_margin(mut self, margin: f32) -> Self {
        self.set_top_margin(margin);
        self
    }

    /// How far the sheet's lower edge lies above the container's, px (the keyboard).
    pub const fn set_bottom_inset(&mut self, inset: f32) {
        self.bottom_inset = inset;
    }

    /// [`Self::set_bottom_inset`] for the builder chain.
    #[must_use]
    pub const fn with_bottom_inset(mut self, inset: f32) -> Self {
        self.set_bottom_inset(inset);
        self
    }

    /// The room the content keeps at its end, px (the home indicator).
    pub const fn set_safe_bottom(&mut self, inset: f32) {
        self.safe_bottom = inset;
    }

    /// [`Self::set_safe_bottom`] for the builder chain.
    #[must_use]
    pub const fn with_safe_bottom(mut self, inset: f32) -> Self {
        self.set_safe_bottom(inset);
        self
    }

    /// The height of the band at the top that drags, px, the grip included.
    pub const fn set_grab_height(&mut self, height: f32) {
        self.grab_height = height;
    }

    /// [`Self::set_grab_height`] for the builder chain.
    #[must_use]
    pub const fn with_grab_height(mut self, height: f32) -> Self {
        self.set_grab_height(height);
        self
    }

    /// Every step of a drag and every settling.
    pub fn set_on_event<C: Into<BottomSheetOnEventCallback>>(&mut self, data: RefAny, on_event: C) {
        self.on_event = OptionBottomSheetOnEvent::Some(BottomSheetOnEvent {
            refany: data,
            callback: on_event.into(),
        });
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<BottomSheetOnEventCallback>>(
        mut self,
        data: RefAny,
        on_event: C,
    ) -> Self {
        self.set_on_event(data, on_event);
        self
    }

    /// The height of the sheet at `detent`, px, with this sheet's numbers: where an app puts what
    /// floats above the sheet (a map's locate key).
    #[must_use]
    pub fn height_of(&self, detent: SheetDetent) -> f32 {
        self.heights().of(detent.clamp(self.lowest_allowed(), self.highest_allowed()))
    }

    /// The sheet's DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        build(self)
    }

    /// Takes the sheet out, leaving an empty one: the move the C ABI makes.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        core::mem::take(self)
    }

    fn heights(&self) -> Heights {
        Heights::new(
            room(self.container_height, self.bottom_inset),
            self.collapsed_height,
            self.peek_height,
            self.top_margin,
        )
    }

    /// The lowest and the highest detent, in order whichever way they were named.
    fn lowest_allowed(&self) -> SheetDetent {
        self.lowest.min(self.highest)
    }

    fn highest_allowed(&self) -> SheetDetent {
        self.lowest.max(self.highest)
    }
}

impl Default for BottomSheet {
    fn default() -> Self {
        Self::create(Dom::create_div())
    }
}

impl From<BottomSheet> for Dom {
    fn from(s: BottomSheet) -> Self {
        s.dom()
    }
}

// ---------------------------------------------------------------------------
// The drag
// ---------------------------------------------------------------------------

/// What the handlers share: the app's callback, the numbers, and the drag in flight. The root's
/// dataset, so a rebuild can carry the drag across.
#[derive(Clone)]
struct DragState {
    on_event: OptionBottomSheetOnEvent,
    detent: SheetDetent,
    lowest: SheetDetent,
    highest: SheetDetent,
    collapsed: f32,
    peek: f32,
    top_margin: f32,
    container: f32,
    /// How far the sheet's lower edge lies above the container's (the keyboard): the detents are heights of the room above it.
    inset: f32,
    grab: f32,
    /// The sheet's height now, px.
    height: f32,
    /// A press landed in the band that drags and has not been let go.
    armed: bool,
    /// The press was on the grip itself (a tap on it steps a detent).
    on_grip: bool,
    /// The press has moved past the slop: the sheet follows the pointer.
    dragging: bool,
    /// The pointer's window y at the press, and the sheet's height then.
    start_y: f32,
    start_height: f32,
    /// The last window y, and the step it made from the one before (up is positive), for the fling.
    last_y: f32,
    step: f32,
    /// When the last of them came, ms on the press's clock: a pointer that rested before it was lifted does not fling.
    last_ms: u64,
    /// The clock's zero (the first press); the pure rules below only see milliseconds.
    origin: Option<azul_core::task::Instant>,
}

/// One step of a drag: whether it began the drag, and the height the pointer makes of the sheet.
#[derive(Debug, Copy, Clone, PartialEq)]
struct DragStep {
    started: bool,
    height: f32,
}

impl DragState {
    fn heights(&self) -> Heights {
        Heights::new(room(self.container, self.inset), self.collapsed, self.peek, self.top_margin)
    }

    /// A press at window `y` on the sheet whose top edge is at `top` and which is `height` tall, in a box `container` tall (as the
    /// layout has it now): the sheet's when it lies in the band that drags (the grip, and the band the app named under it), else
    /// it belongs to what is there and nothing is armed.
    fn press(&mut self, y: f32, top: f32, height: f32, container: f32, now_ms: u64) -> bool {
        let in_sheet = y - top;
        if !in_sheet.is_finite() || in_sheet > self.grab.max(GRIP_HEIGHT) || in_sheet < 0.0 {
            return false;
        }
        self.container = container;
        self.armed = true;
        self.dragging = false;
        self.on_grip = in_sheet <= GRIP_HEIGHT;
        self.start_y = y;
        self.last_y = y;
        self.step = 0.0;
        self.last_ms = now_ms;
        if height.is_finite() && height > 0.0 {
            self.height = height;
        }
        self.start_height = self.height;
        true
    }

    /// The pointer at window `y` (at `now_ms`) with a press down: `None` before it has moved past the tap slop (and with no press),
    /// and when it has not moved since the last step (a phone reports one finger twice, as a touch and as a mouse: the second report
    /// must not count as a stop); else the step - the sheet follows the pointer from here until it is let go.
    fn drag_to(&mut self, y: f32, now_ms: u64) -> Option<DragStep> {
        if !self.armed {
            return None;
        }
        let started = !self.dragging;
        if started && (self.start_y - y).abs() <= TAP_SLOP {
            return None;
        }
        if !started && (self.last_y - y).abs() < MOVED_LESS_THAN {
            return None;
        }
        self.dragging = true;
        self.step = self.last_y - y;
        self.last_y = y;
        self.last_ms = now_ms;
        let heights = self.heights();
        let height = heights.dragged(self.start_height, self.start_y, y, self.lowest, self.highest);
        self.height = height;
        self.detent = heights.nearest(height, self.lowest, self.highest);
        Some(DragStep { started, height })
    }

    /// The pointer is up (`leaving`: it left the sheet with the button already up, a release that never came): where the sheet
    /// comes to rest - a drag settles on the nearest detent after a fling, a tap on the grip steps to the next one up. `None`:
    /// nothing of the sheet's (no press, a press in the band under the grip - it belongs to what is there - or a press on the
    /// grip that merely left).
    fn lift(&mut self, leaving: bool, now_ms: u64) -> Option<SheetDetent> {
        if !self.armed {
            return None;
        }
        self.armed = false;
        if self.dragging {
            self.dragging = false;
            // A pointer that rested before it was lifted has no speed: the settling is the nearest detent to where it is.
            let step = if now_ms.saturating_sub(self.last_ms) > FLING_STALE_MS { 0.0 } else { self.step };
            Some(self.heights().settle(self.height, step, self.lowest, self.highest))
        } else if self.on_grip && !leaving {
            Some(step_round(self.detent, self.lowest, self.highest))
        } else {
            None
        }
    }

    /// The sheet rests at `detent`: its height, px.
    fn rest(&mut self, detent: SheetDetent) -> f32 {
        self.detent = detent;
        self.height = self.heights().of(detent);
        self.height
    }
}

/// The room the detents are heights of: the container less what lies under the sheet (the keyboard). A container that is no number is
/// the default one; an inset that is no number, or negative, is none; the room is never less than a pixel.
fn room(container: f32, inset: f32) -> f32 {
    let container = length(container, CONTAINER_HEIGHT_DEFAULT);
    let inset = if inset.is_finite() { inset.max(0.0) } else { 0.0 };
    (container - inset).max(1.0)
}

fn height_property(px: f32) -> CssProperty {
    CssProperty::const_height(LayoutHeight::px(px))
}

/// A step of a drag: the live height, on the engine's override channel (cheap for every frame, and it follows the node through a
/// rebuild while the pointer is down).
fn follow_pointer(info: &mut CallbackInfo, root: DomNodeId, px: f32) {
    info.override_css_property(root, height_property(px));
}

/// The sheet rests at `px`: the node's own style says so, and the drag's override is handed back, so the app's next build is the
/// truth again (the engine's way for a widget to end a gesture: `set_css_property`, then `override_css_property` with `initial`).
fn rest_at(info: &mut CallbackInfo, root: DomNodeId, px: f32) {
    info.set_css_property(root, height_property(px));
    info.override_css_property(root, CssProperty::initial(CssPropertyType::Height));
}

/// Tells the app: the event as the state has it now.
fn report(state: &mut DragState, info: CallbackInfo, kind: BottomSheetEventKind) -> Update {
    let ev = BottomSheetEvent {
        kind,
        detent: state.detent,
        height: state.height,
    };
    match state.on_event.as_mut() {
        Some(BottomSheetOnEvent { callback, refany }) => callback.invoke(refany.clone(), info, ev),
        None => Update::DoNothing,
    }
}

/// The pointer's y in the window: the first finger while one is down (the events of a finger do not move the mouse's cursor
/// everywhere: a scripted touch, a second touch screen), else the mouse's cursor.
fn window_y(info: &CallbackInfo) -> Option<f32> {
    let touch = info
        .get_current_window_state()
        .touch_state
        .touch_points
        .as_ref()
        .first()
        .map(|point| point.position.y);
    touch
        .or_else(|| info.get_cursor_relative_to_viewport().into_option().map(|p| p.y))
        .filter(|y| y.is_finite())
}

/// Milliseconds on the sheet's clock (zero at its first press): the engine's clock, which a scenario's `tick_ms` advances.
fn millis(info: &CallbackInfo, st: &mut DragState) -> u64 {
    let now = info.get_current_time();
    let origin = st.origin.get_or_insert_with(|| now.clone()).clone();
    now.duration_since(&origin).as_millis_u64()
}

/// The height of the box the sheet floats in, as the layout has it now (the build's number when
/// the layout has none): the height of the parent of the sheet's `root`, the positioned box.
fn measured_container(info: &CallbackInfo, root: DomNodeId, built: f32) -> f32 {
    info.get_parent(root)
        .and_then(|parent| info.get_node_size(parent))
        .map(|size| size.height)
        .filter(|h| h.is_finite() && *h > 0.0)
        .unwrap_or(built)
}

extern "C" fn on_press(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(y) = window_y(&info) else {
        return Update::DoNothing;
    };
    let Some(rect) = info.get_hit_node_rect() else {
        return Update::DoNothing;
    };
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    let container = measured_container(&info, info.get_hit_node(), st.container);
    let now = millis(&info, &mut st);
    st.press(y, rect.origin.y, rect.size.height, container, now);
    Update::DoNothing
}

extern "C" fn on_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    if !st.armed {
        return Update::DoNothing;
    }
    let Some(y) = window_y(&info) else {
        return Update::DoNothing;
    };
    let now = millis(&info, &mut st);
    let Some(step) = st.drag_to(y, now) else {
        return Update::DoNothing;
    };
    // A press that began on the sheet is followed wherever the pointer goes (the moves and the release are window-level events, see
    // `build`): a finger or a mouse that outruns the sheet's top edge does not lose the drag.
    let root = info.get_hit_node();
    follow_pointer(&mut info, root, step.height);
    report(&mut st, info, BottomSheetEventKind::Dragged)
}

/// The pointer is up: a drag settles on a detent, a tap on the grip steps to the next one, a press in the band under the grip
/// belongs to what is there.
fn finish(st: &mut DragState, mut info: CallbackInfo, leaving: bool) -> Update {
    let now = millis(&info, st);
    let Some(detent) = st.lift(leaving, now) else {
        return Update::DoNothing;
    };
    let px = st.rest(detent);
    let root = info.get_hit_node();
    rest_at(&mut info, root, px);
    report(st, info, BottomSheetEventKind::Settled)
}

extern "C" fn on_release(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    finish(&mut st, info, false)
}

/// The pointer left with the button already up: a release that never came (a phone's system took the touch, a pan that lifts is
/// reported as a cancel and the pointer leaves): a drag in flight ends where it was and settles, as a release would; a press that
/// never moved is let go.
extern "C" fn on_leave(mut data: RefAny, info: CallbackInfo) -> Update {
    if info.get_current_mouse_state().left_down {
        return Update::DoNothing;
    }
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    finish(&mut st, info, true)
}

/// The system took the touch (a swipe from the edge, an incoming call): a drag in flight ends where it was and settles as a release
/// would, a press that never moved is let go.
extern "C" fn on_cancel(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    finish(&mut st, info, true)
}

/// The keys on the focused grip.
extern "C" fn on_grip_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(code) = info.get_current_keyboard_state().current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    let Some(wanted) = detent_after_key(code, st.detent, st.lowest, st.highest) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    // The grip's parent is the sheet; the keys come to the grip, the height goes to the sheet.
    let grip = info.get_hit_node();
    let Some(root) = info.get_parent(grip) else {
        return Update::DoNothing;
    };
    st.container = measured_container(&info, root, st.container);
    let px = st.rest(wanted);
    rest_at(&mut info, root, px);
    report(&mut st, info, BottomSheetEventKind::Settled)
}

/// Carries a drag across a rebuild of the app's DOM: while the pointer is down the drag's anchor
/// and the live height stay (the app was told the height through `on_event`; one that stores a
/// detent builds the same number, one that does not would otherwise yank the sheet from under the
/// pointer). Once it is up the app's detent is the truth again. The callback is always the FRESH
/// build's.
#[must_use]
pub extern "C" fn merge_sheet_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let new_guard = new_data.downcast_mut::<DragState>();
        let old_guard = old_data.downcast_ref::<DragState>();
        if let (Some(mut new_g), Some(old_g)) = (new_guard, old_guard) {
            if old_g.armed {
                new_g.armed = true;
                new_g.dragging = old_g.dragging;
                new_g.on_grip = old_g.on_grip;
                new_g.start_y = old_g.start_y;
                new_g.start_height = old_g.start_height;
                new_g.last_y = old_g.last_y;
                new_g.step = old_g.step;
                new_g.last_ms = old_g.last_ms;
                new_g.origin.clone_from(&old_g.origin);
                if old_g.dragging {
                    new_g.height = old_g.height;
                    new_g.detent = old_g.detent;
                }
            }
        }
    }
    new_data
}

// ---------------------------------------------------------------------------
// The build
// ---------------------------------------------------------------------------

fn classes(list: &[&'static str]) -> IdOrClassVec {
    IdOrClassVec::from_vec(
        list.iter()
            .map(|c| IdOrClass::Class(AzString::from_const_str(c)))
            .collect::<Vec<_>>(),
    )
}

fn styled(mut dom: Dom, css: &str) -> Dom {
    dom.root.set_css(css);
    dom
}

fn cb(
    filter: EventFilter,
    data: RefAny,
    f: extern "C" fn(RefAny, CallbackInfo) -> Update,
) -> CoreCallbackData {
    CoreCallbackData {
        event: filter,
        callback: CoreCallback {
            cb: f as usize,
            ctx: OptionRefAny::None,
        },
        refany: data,
    }
}

const fn detent_class(detent: SheetDetent) -> &'static str {
    match detent {
        SheetDetent::Collapsed => BOTTOM_SHEET_COLLAPSED_CLASS,
        SheetDetent::Peek => BOTTOM_SHEET_PEEK_CLASS,
        SheetDetent::Half => BOTTOM_SHEET_HALF_CLASS,
        SheetDetent::Full => BOTTOM_SHEET_FULL_CLASS,
    }
}

/// The sheet's root look: a ground that follows the window, rounded at the top under a shadow, flora's
/// own corners and warm shadow under the flora theme.
fn root_css(height: f32, bottom_inset: f32) -> String {
    format!(
        "position: absolute; left: 0px; right: 0px; bottom: {bottom}px; height: {height}px; \
         box-sizing: border-box; display: flex; flex-direction: column; overflow: hidden; \
         background: system:window-background; color: system:text; border-radius: 16px 16px 0px 0px; \
         box-shadow: 0px -2px 12px rgba(0, 0, 0, 0.28); \
         @theme(flora) {{ border-radius: 5px 5px 0px 0px; box-shadow: 0px -2px 10px rgba(48, 45, 38, 0.22); \
         @media (prefers-color-scheme: dark) {{ box-shadow: 0px -2px 10px rgba(0, 0, 0, 0.5); }} }}",
        bottom = bottom_inset.max(0.0),
        height = height,
    )
}

fn build(sheet: BottomSheet) -> Dom {
    let heights = sheet.heights();
    let lowest = sheet.lowest_allowed();
    let highest = sheet.highest_allowed();
    let detent = sheet.detent.clamp(lowest, highest);
    let height = heights.of(detent);
    let BottomSheet {
        content,
        label,
        grip_label,
        container_height,
        peek_height,
        collapsed_height,
        top_margin,
        bottom_inset,
        safe_bottom,
        grab_height,
        on_event,
        ..
    } = sheet;
    let bottom_inset = if bottom_inset.is_finite() { bottom_inset.max(0.0) } else { 0.0 };
    let safe_bottom = if safe_bottom.is_finite() { safe_bottom.max(0.0) } else { 0.0 };
    let grab = if grab_height.is_finite() { grab_height.max(GRIP_HEIGHT) } else { GRIP_HEIGHT };

    let state = RefAny::new(DragState {
        on_event,
        detent,
        lowest,
        highest,
        collapsed: collapsed_height,
        peek: peek_height,
        top_margin,
        container: container_height,
        inset: bottom_inset,
        grab,
        height,
        armed: false,
        on_grip: false,
        dragging: false,
        start_y: 0.0,
        start_height: height,
        last_y: 0.0,
        step: 0.0,
        last_ms: 0,
        origin: None,
    });

    let pill = styled(
        Dom::create_div(),
        "width: 40px; height: 4px; border-radius: 2px; background: currentColor; opacity: 0.45;",
    )
    .with_ids_and_classes(classes(&[BOTTOM_SHEET_PILL_CLASS]));
    let percent = (height / length(container_height, CONTAINER_HEIGHT_DEFAULT) * 100.0).round();
    let name = if grip_label.as_str().is_empty() { label.clone() } else { grip_label };
    let grip = styled(
        Dom::create_div().with_child(pill),
        &format!(
            "display: flex; align-items: center; justify-content: center; flex-shrink: 0; width: 100%; \
             height: {GRIP_HEIGHT}px; cursor: row-resize;"
        ),
    )
    .with_ids_and_classes(classes(&[BOTTOM_SHEET_GRIP_CLASS]))
    .with_accessibility_info(AccessibilityInfo {
        role: AccessibilityRole::Grip,
        accessibility_name: azul_css::OptionString::Some(name),
        accessibility_value: azul_css::OptionString::Some(AzString::from(format!("{percent}"))),
        ..Default::default()
    })
    .with_tab_index(TabIndex::Auto)
    .with_callbacks(
        alloc::vec![cb(
            EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
            state.clone(),
            on_grip_key
        )]
        .into(),
    );

    let content = styled(
        Dom::create_div().with_child(content),
        &format!(
            "display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; min-height: 0px; \
             min-width: 0px; overflow-x: hidden; overflow-y: auto; padding-bottom: {safe_bottom}px; box-sizing: border-box;"
        ),
    )
    .with_ids_and_classes(classes(&[BOTTOM_SHEET_CONTENT_CLASS]));

    // A runtime override (the height a drag writes) follows a node's IDENTITY across a rebuild and wins over the fresh cascade; a
    // key that does not change keeps the sheet one node whatever the app builds around it (and whatever detent it builds the sheet
    // at), so a rebuild in the middle of a drag keeps the drag's override. The override is handed back when the pointer is up
    // (`rest_at`), so nothing of a drag is carried onto the next build.
    styled(
        Dom::create_div().with_children(DomVec::from_vec(alloc::vec![grip, content])),
        &root_css(height, bottom_inset),
    )
    .with_key(BOTTOM_SHEET_CLASS)
    .with_ids_and_classes(classes(&[BOTTOM_SHEET_CLASS, detent_class(detent)]))
    .with_accessibility_info(AccessibilityInfo::named(label, AccessibilityRole::Pane))
    .with_callbacks(
        alloc::vec![
            // the press must land on the sheet; everything after it is followed wherever the pointer goes
            cb(EventFilter::Hover(HoverEventFilter::MouseDown), state.clone(), on_press),
            cb(EventFilter::Hover(HoverEventFilter::TouchStart), state.clone(), on_press),
            cb(EventFilter::Window(WindowEventFilter::MouseOver), state.clone(), on_move),
            cb(EventFilter::Window(WindowEventFilter::TouchMove), state.clone(), on_move),
            cb(EventFilter::Window(WindowEventFilter::MouseUp), state.clone(), on_release),
            cb(EventFilter::Window(WindowEventFilter::TouchEnd), state.clone(), on_release),
            cb(EventFilter::Window(WindowEventFilter::TouchCancel), state.clone(), on_cancel),
            cb(EventFilter::Hover(HoverEventFilter::MouseLeave), state.clone(), on_leave),
        ]
        .into(),
    )
    .with_dataset(OptionRefAny::Some(state))
    .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(merge_sheet_state))
}

#[cfg(test)]
#[path = "bottom_sheet_test.rs"]
mod bottom_sheet_test;
