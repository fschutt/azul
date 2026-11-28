//! The BottomSheet: a panel that floats over the lower part of the box it is in and is dragged
//! between DETENTS - a map's place card and its results, a code editor's terminal panel.
//!
//! ```text
//!   the container (the app's window)
//!  ┌──────────────────────────────────┐
//!  │ the map, the editor ...          │
//!  │                                  │
//!  ├────────────── ▬▬▬ ───────────────┤  Full:      the container less the top margin
//!  │ the sheet's content              │  Half:      half of the container
//!  │                                  │  Peek:      a summary (148 px)
//!  ├──────────────────────────────────┤  Collapsed: the grip and one line (44 px)
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
//! The sheet owns nothing: the app keeps the detent, hears every drag step and every settling
//! through ONE callback ([`BottomSheetOnEvent`]) and rebuilds. A drag in flight survives a rebuild
//! (`merge_sheet_state`): while the pointer is down it wins.
//!
//! Heights. `Peek` and `Collapsed` are px ([`BottomSheet::with_peek_height`],
//! [`BottomSheet::with_collapsed_height`]), `Half` is half the container, `Full` the container
//! less the top margin; the four are kept in order whatever the numbers, and a detent the app
//! rules out ([`BottomSheet::with_lowest`] / [`BottomSheet::with_highest`]) is never settled on.
//! A number that is not finite is replaced by its default, never an error.
//!
//! Key types: [`BottomSheet`], [`SheetDetent`], [`BottomSheetEvent`], [`BottomSheetEventKind`],
//! [`BottomSheetOnEvent`].

use alloc::{format, string::String, vec::Vec};

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec, TabIndex},
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    impl_option, props::layout::LayoutHeight, props::property::CssProperty, AzString,
};

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
/// The peek detent's height when the app names none, px.
pub const PEEK_HEIGHT_DEFAULT: f32 = 148.0;
/// The collapsed detent's height when the app names none, px: the grip and one line.
pub const COLLAPSED_HEIGHT_DEFAULT: f32 = 44.0;
/// The room the full detent leaves above the sheet when the app names none, px.
pub const TOP_MARGIN_DEFAULT: f32 = 56.0;
/// What a container is taken to be when the app names none (or a number that is not one), px.
pub const CONTAINER_HEIGHT_DEFAULT: f32 = 800.0;
/// How far the pointer travels before a press is a drag and not a tap, px.
pub const TAP_SLOP: f32 = 8.0;
/// A last step of this many px (per event, about a frame) or more is a fling.
pub const FLING_STEP: f32 = 6.0;
/// How many events of that step a fling carries on: where it would end, px = step x this.
pub const FLING_EVENTS: f32 = 12.0;

/// Where a sheet rests.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum SheetDetent {
    /// Only the grip and one line (a tab strip, a title) show.
    Collapsed,
    /// A summary: a place's name and its actions.
    #[default]
    Peek,
    /// Half of the container.
    Half,
    /// The container less the top margin.
    Full,
}

impl SheetDetent {
    /// The detent above this one (the highest stays).
    #[must_use]
    pub const fn higher(self) -> Self {
        match self {
            Self::Collapsed => Self::Peek,
            Self::Peek => Self::Half,
            Self::Half | Self::Full => Self::Full,
        }
    }

    /// The detent below this one (the lowest stays).
    #[must_use]
    pub const fn lower(self) -> Self {
        match self {
            Self::Collapsed | Self::Peek => Self::Collapsed,
            Self::Half => Self::Peek,
            Self::Full => Self::Half,
        }
    }
}

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

/// The BottomSheet: its content under a grip, at a detent.
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
    /// The peek detent's height, px; 0 is 148.
    pub peek_height: f32,
    /// The collapsed detent's height, px; 0 is 44.
    pub collapsed_height: f32,
    /// The room the full detent leaves above the sheet, px (the status bar, the search bar a map
    /// keeps in view); 0 is 56.
    pub top_margin: f32,
    /// How far the sheet's lower edge lies above the container's: the on-screen keyboard.
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
    pub fn swap_with_default(&mut self) -> Self {
        core::mem::take(self)
    }

    fn heights(&self) -> Heights {
        Heights::new(
            self.container_height,
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
// The numbers
// ---------------------------------------------------------------------------

/// A number that must be a finite, positive length, else `default`.
fn length(value: f32, default: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        default
    }
}

/// The four heights of a sheet, px, in order: collapsed <= peek <= half <= full <= the container.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Heights {
    /// The collapsed detent.
    pub collapsed: f32,
    /// The peek detent.
    pub peek: f32,
    /// The half detent.
    pub half: f32,
    /// The full detent.
    pub full: f32,
}

impl Heights {
    /// The heights for a container `container` tall: a number that is not a positive finite one
    /// is its default, and the four are held in order (a container shorter than the peek makes
    /// every detent the container).
    #[must_use]
    pub fn new(container: f32, collapsed: f32, peek: f32, top_margin: f32) -> Self {
        let container = length(container, CONTAINER_HEIGHT_DEFAULT);
        let peek = length(peek, PEEK_HEIGHT_DEFAULT).min(container);
        let collapsed = length(collapsed, COLLAPSED_HEIGHT_DEFAULT).min(peek);
        let margin = length(top_margin, TOP_MARGIN_DEFAULT);
        let half = (container * 0.5).max(peek);
        let full = (container - margin).max(half);
        Self {
            collapsed,
            peek,
            half,
            full,
        }
    }

    /// The height of `detent`.
    #[must_use]
    pub const fn of(&self, detent: SheetDetent) -> f32 {
        match detent {
            SheetDetent::Collapsed => self.collapsed,
            SheetDetent::Peek => self.peek,
            SheetDetent::Half => self.half,
            SheetDetent::Full => self.full,
        }
    }

    /// The detent, between `lowest` and `highest`, whose height is nearest to `height` (the
    /// higher one when two are as near).
    #[must_use]
    pub fn nearest(&self, height: f32, lowest: SheetDetent, highest: SheetDetent) -> SheetDetent {
        let mut best = highest;
        let mut at = highest;
        let mut distance = (self.of(highest) - height).abs();
        while at > lowest {
            at = at.lower();
            let d = (self.of(at) - height).abs();
            if d < distance {
                best = at;
                distance = d;
            }
        }
        best
    }

    /// Where a drag that ended at `height`, last moving `step` px per event (up is positive),
    /// settles: a fling (`step` of [`FLING_STEP`] or more) carries on for [`FLING_EVENTS`] events,
    /// then the nearest detent in range wins.
    #[must_use]
    pub fn settle(
        &self,
        height: f32,
        step: f32,
        lowest: SheetDetent,
        highest: SheetDetent,
    ) -> SheetDetent {
        let carried = if step.is_finite() && step.abs() >= FLING_STEP {
            step * FLING_EVENTS
        } else {
            0.0
        };
        self.nearest(height + carried, lowest, highest)
    }

    /// The height a drag that began at `start_height` with the pointer at `start_y` makes of the
    /// sheet with the pointer at `now_y` (window coordinates: y grows downwards, so a pointer
    /// going up grows the sheet), held between the lowest and the highest detent's heights.
    #[must_use]
    pub fn dragged(
        &self,
        start_height: f32,
        start_y: f32,
        now_y: f32,
        lowest: SheetDetent,
        highest: SheetDetent,
    ) -> f32 {
        let wanted = start_height + (start_y - now_y);
        let (a, b) = (self.of(lowest), self.of(highest));
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        let wanted = if wanted.is_nan() { start_height } else { wanted };
        wanted.max(low).min(high)
    }
}

/// What a key on the focused grip does to the detent: `None` for a key the grip ignores.
#[must_use]
pub fn detent_after_key(
    key: VirtualKeyCode,
    detent: SheetDetent,
    lowest: SheetDetent,
    highest: SheetDetent,
) -> Option<SheetDetent> {
    let (lowest, highest) = (lowest.min(highest), lowest.max(highest));
    let wanted = match key {
        VirtualKeyCode::Up | VirtualKeyCode::PageUp => detent.higher(),
        VirtualKeyCode::Down | VirtualKeyCode::PageDown => detent.lower(),
        VirtualKeyCode::Home => highest,
        VirtualKeyCode::End => lowest,
        VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter | VirtualKeyCode::Space => {
            return Some(step_round(detent, lowest, highest))
        }
        _ => return None,
    };
    Some(wanted.clamp(lowest, highest))
}

/// The tap's step: the next detent up, from the highest back to the lowest.
#[must_use]
pub fn step_round(detent: SheetDetent, lowest: SheetDetent, highest: SheetDetent) -> SheetDetent {
    let (lowest, highest) = (lowest.min(highest), lowest.max(highest));
    if detent >= highest {
        lowest
    } else {
        detent.higher().clamp(lowest, highest)
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
    /// The last two window ys, for the fling.
    last_y: f32,
    step: f32,
}

impl DragState {
    fn heights(&self) -> Heights {
        Heights::new(self.container, self.collapsed, self.peek, self.top_margin)
    }
}

fn height_property(px: f32) -> CssProperty {
    CssProperty::const_height(LayoutHeight::px(px))
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

/// The pointer's y in the window.
fn window_y(info: &CallbackInfo) -> Option<f32> {
    info.get_cursor_relative_to_viewport()
        .into_option()
        .map(|p| p.y)
        .filter(|y| y.is_finite())
}

/// The height of the box the sheet floats in, as the layout has it now (the build's number when
/// the layout has none: the sheet's parent is the positioned box).
fn measured_container(info: &CallbackInfo, built: f32) -> f32 {
    let root = info.get_hit_node();
    info.get_parent(root)
        .and_then(|parent| info.get_node_size(parent))
        .map(|size| size.height)
        .filter(|h| h.is_finite() && *h > 0.0)
        .unwrap_or(built)
}

extern "C" fn on_press(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(pos) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
    };
    let Some(y) = window_y(&info) else {
        return Update::DoNothing;
    };
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    if pos.y > st.grab.max(GRIP_HEIGHT) || pos.y < 0.0 {
        return Update::DoNothing;
    }
    st.container = measured_container(&info, st.container);
    st.armed = true;
    st.dragging = false;
    st.on_grip = pos.y <= GRIP_HEIGHT;
    st.start_y = y;
    st.last_y = y;
    st.step = 0.0;
    if let Some(rect) = info.get_hit_node_rect() {
        if rect.size.height.is_finite() && rect.size.height > 0.0 {
            st.height = rect.size.height;
        }
    }
    st.start_height = st.height;
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
    if !st.dragging {
        if (st.start_y - y).abs() <= TAP_SLOP {
            return Update::DoNothing;
        }
        // Past the slop the press is a drag: the pointer is the sheet's until it is let go.
        st.dragging = true;
        let root = info.get_hit_node();
        info.capture_pointer(root);
    }
    st.step = st.last_y - y;
    st.last_y = y;
    let heights = st.heights();
    let height = heights.dragged(st.start_height, st.start_y, y, st.lowest, st.highest);
    st.height = height;
    st.detent = heights.nearest(height, st.lowest, st.highest);
    let root = info.get_hit_node();
    info.set_css_property(root, height_property(height));
    report(&mut st, info, BottomSheetEventKind::Dragged)
}

extern "C" fn on_release(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    if !st.armed {
        return Update::DoNothing;
    }
    st.armed = false;
    let heights = st.heights();
    let wanted = if st.dragging {
        st.dragging = false;
        heights.settle(st.height, st.step, st.lowest, st.highest)
    } else if st.on_grip {
        // A tap on the grip: the next detent up.
        step_round(st.detent, st.lowest, st.highest)
    } else {
        // A tap in the band under the grip belongs to what is there.
        return Update::DoNothing;
    };
    settle_at(&mut st, info, wanted)
}

/// A leave with the button already up is a release that never came: the press ends.
extern "C" fn on_leave(mut data: RefAny, info: CallbackInfo) -> Update {
    if info.get_current_mouse_state().left_down {
        return Update::DoNothing;
    }
    if let Some(mut st) = data.downcast_mut::<DragState>() {
        st.armed = false;
        st.dragging = false;
    }
    Update::DoNothing
}

/// The sheet rests at `detent`: its height set, the app told.
fn settle_at(st: &mut DragState, mut info: CallbackInfo, detent: SheetDetent) -> Update {
    let heights = st.heights();
    st.detent = detent;
    st.height = heights.of(detent);
    let root = info.get_hit_node();
    info.set_css_property(root, height_property(st.height));
    report(st, info, BottomSheetEventKind::Settled)
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
    st.container = measured_container(&info, st.container);
    let heights = st.heights();
    st.detent = wanted;
    st.height = heights.of(wanted);
    info.set_css_property(root, height_property(st.height));
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
        grab,
        height,
        armed: false,
        on_grip: false,
        dragging: false,
        start_y: 0.0,
        start_height: height,
        last_y: 0.0,
        step: 0.0,
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
             min-width: 0px; overflow-y: auto; padding-bottom: {safe_bottom}px; box-sizing: border-box;"
        ),
    )
    .with_ids_and_classes(classes(&[BOTTOM_SHEET_CONTENT_CLASS]));

    styled(
        Dom::create_div().with_children(DomVec::from_vec(alloc::vec![grip, content])),
        &root_css(height, bottom_inset),
    )
    .with_ids_and_classes(classes(&[BOTTOM_SHEET_CLASS, detent_class(detent)]))
    .with_accessibility_info(AccessibilityInfo::named(label, AccessibilityRole::Pane))
    .with_callbacks(
        alloc::vec![
            cb(EventFilter::Hover(HoverEventFilter::MouseDown), state.clone(), on_press),
            cb(EventFilter::Hover(HoverEventFilter::MouseOver), state.clone(), on_move),
            cb(EventFilter::Hover(HoverEventFilter::MouseUp), state.clone(), on_release),
            cb(EventFilter::Hover(HoverEventFilter::MouseLeave), state.clone(), on_leave),
            cb(EventFilter::Hover(HoverEventFilter::TouchStart), state.clone(), on_press),
            cb(EventFilter::Hover(HoverEventFilter::TouchMove), state.clone(), on_move),
            cb(EventFilter::Hover(HoverEventFilter::TouchEnd), state.clone(), on_release),
        ]
        .into(),
    )
    .with_dataset(OptionRefAny::Some(state))
    .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(merge_sheet_state))
}

#[cfg(test)]
#[path = "bottom_sheet_test.rs"]
mod bottom_sheet_test;
