//! The Inspector: ONE widget with TWO docks. The pane an app shows beside what the user is
//! working on - AzDrive's preview, AzMail's message, AzCalendar's event, AzWriter's styles.
//!
//! ```text
//!  desktop: docked RIGHT, below the tab ribbon        phone: docked at the BOTTOM
//! ┌─ tab ribbon, the full width ─────────────────┐    ┌──────────────────────┐
//! ├────────────────────────────┬─┬───────────────┤    │ main view            │
//! │ body                       │▐│ Preview     × │    │ ...                  │
//! │                            │▐│               │    ├────── ▬▬▬ ───────────┤  the grip
//! │                            │▐│               │    │ Preview              │
//! │                            │▐│               │    │ the row at the top   │
//! └────────────────────────────┴─┴───────────────┘    │ edge of the list     │
//!   drag the left border (the width)                    └──────────────────────┘
//!                                                         drag the grip (the height)
//! ```
//!
//! The Inspector wraps the BODY it sits beside and the PANE it shows: a desktop gets a pane at the
//! right below whatever the app puts above it (the tab ribbon runs over its full width), a phone a
//! pane at the bottom (the footer line stays under it) - never both, never an overlay: it is a
//! split. [`InspectorDock::Auto`] chooses by the form factor; an app names a dock to fix it.
//!
//! Both docks are resized by dragging: the right pane's left border (its width, between 200 px
//! and 60 % of the container, leaving the body 240 px), the bottom pane's grip (its height,
//! between the PEEK of 56 px - the grip and one line of header - and 85 % of the container). A
//! pane released within 28 px of an end settles on it; between the ends it stays where it was
//! left, so a phone's preview can be dragged to any height. A double tap on the grip flips
//! between the peek and the default height. The arrow keys on the focused grip do the same by
//! 16 px (64 with Ctrl / Cmd), Home and End go to the ends - the APG window splitter.
//!
//! The Inspector owns nothing: the app keeps `size` and `open`, hears every change through ONE
//! callback ([`InspectorOnEvent`]) and rebuilds. A drag in flight survives a rebuild
//! (`merge_inspector_state`): the pointer wins while it is down.
//!
//! Key types: [`Inspector`], [`InspectorEvent`], [`InspectorEventKind`], [`InspectorOnEvent`],
//! [`InspectorDock`].

use alloc::{format, string::String, vec::Vec};

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec, OptionDom, TabIndex,
    },
    events::FocusEventFilter,
    geom::CursorNodePosition,
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    impl_option, props::layout::LayoutHeight, props::layout::LayoutWidth, props::property::CssProperty, AzString,
};

pub use super::sidebar_layout::InspectorDock;
use super::{
    sidebar::SidebarSurface,
    sidebar_layout::{
        clamp_inspector_width, clamp_pane_height, dragged_inspector_width, dragged_pane_height,
        pane_max, settle_pane_height, step_size, FormFactor, GripKey, BORDER_GRAB, GRIP_HEIGHT,
        INSPECTOR_WIDTH_DEFAULT, PANE_HEIGHT_DEFAULT, PANE_PEEK,
    },
    themes::{OptionUiTheme, UiTheme},
    widget_p_with_text,
};
use crate::callbacks::CallbackInfo;

/// The Inspector's root: the body and the pane.
pub const INSPECTOR_CLASS: &str = "__azul-native-inspector";
/// Added to the root while the pane is docked at the right.
pub const INSPECTOR_RIGHT_CLASS: &str = "__azul-native-inspector-right";
/// Added to the root while the pane is docked at the bottom.
pub const INSPECTOR_BOTTOM_CLASS: &str = "__azul-native-inspector-bottom";
/// The body beside or above the pane.
pub const INSPECTOR_BODY_CLASS: &str = "__azul-native-inspector-body";
/// The pane.
pub const INSPECTOR_PANE_CLASS: &str = "__azul-native-inspector-pane";
/// The pane's grab zone: the grip row (bottom) or the left border (right).
pub const INSPECTOR_GRIP_CLASS: &str = "__azul-native-inspector-grip";
/// The grip's pill.
pub const INSPECTOR_PILL_CLASS: &str = "__azul-native-inspector-pill";
/// The pane's header row: the title and what the app puts beside it.
pub const INSPECTOR_HEADER_CLASS: &str = "__azul-native-inspector-header";
/// The pane's content.
pub const INSPECTOR_CONTENT_CLASS: &str = "__azul-native-inspector-content";

/// What happened on the Inspector.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum InspectorEventKind {
    /// The pane is being dragged (or a key moved it): `size` is its size now. Fires for every
    /// move; an app that keeps the size stores it, one that does not loses nothing.
    Resized,
    /// The pane was let go (or a key, a double tap on the grip, settled it): `size` is where it
    /// stays, the ends of its range already snapped to.
    Settled,
}

/// One report of the Inspector.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct InspectorEvent {
    /// What happened.
    pub kind: InspectorEventKind,
    /// The pane's width (right dock) or height (bottom dock), px.
    pub size: f32,
    /// The dock the pane is in (never `Auto`).
    pub dock: InspectorDock,
}

/// Callback invoked for every change of the pane's size.
pub type InspectorOnEventCallbackType = extern "C" fn(RefAny, CallbackInfo, InspectorEvent) -> Update;
impl_widget_callback!(
    InspectorOnEvent,
    OptionInspectorOnEvent,
    InspectorOnEventCallback,
    InspectorOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        InspectorOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: INSPECTOR_ON_EVENT_INVOKER,
    invoker_ty:     AzInspectorOnEventCallbackInvoker,
    thunk_fn:       az_inspector_on_event_callback_thunk,
    setter_fn:      AzApp_setInspectorOnEventCallbackInvoker,
    from_handle_fn: AzInspectorOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzInspectorOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: InspectorEvent ],
}

/// The Inspector: a body and the pane that is docked to it.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Inspector {
    /// What the pane sits beside (right dock) or under (bottom dock): the app's main view.
    pub body: Dom,
    /// The pane's content: the preview, the properties, the message.
    pub pane: Dom,
    /// The pane's title ("Preview"): its header and its accessible name.
    pub title: AzString,
    /// What stands at the end of the header row (a close button, a mini player): the pane at its
    /// peek still shows it.
    pub header_end: OptionDom,
    /// Where the pane is docked; `Auto` is the right on a desktop and the bottom on a phone.
    pub dock: InspectorDock,
    /// Desktop or phone: `Auto` asks the window.
    pub form: FormFactor,
    /// The pane's width (right) or height (bottom) in px; 0 is the default (320 wide, 168 tall).
    pub size: f32,
    /// Whether the pane shows. Closed, the body fills everything.
    pub open: bool,
    /// What the pane is painted with.
    pub surface: SidebarSurface,
    /// Every change of the size.
    pub on_event: OptionInspectorOnEvent,
    /// The widget theme this Inspector is PINNED to, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
}

impl_option!(Inspector, OptionInspector, copy = false, [Debug, Clone]);

impl Inspector {
    /// An open Inspector of `pane` beside `body`, docked by the form factor.
    #[must_use]
    pub fn create(body: Dom, pane: Dom) -> Self {
        Self {
            body,
            pane,
            title: AzString::from_const_str("Inspector"),
            header_end: OptionDom::None,
            dock: InspectorDock::Auto,
            form: FormFactor::Auto,
            size: 0.0,
            open: true,
            surface: SidebarSurface::Theme,
            on_event: OptionInspectorOnEvent::None,
            theme: OptionUiTheme::None,
        }
    }

    /// The pane's title.
    pub fn set_title(&mut self, title: AzString) {
        self.title = title;
    }

    /// [`Self::set_title`] for the builder chain.
    #[must_use]
    pub fn with_title(mut self, title: AzString) -> Self {
        self.set_title(title);
        self
    }

    /// What stands at the end of the pane's header row.
    pub fn set_header_end(&mut self, dom: Dom) {
        self.header_end = OptionDom::Some(dom);
    }

    /// [`Self::set_header_end`] for the builder chain.
    #[must_use]
    pub fn with_header_end(mut self, dom: Dom) -> Self {
        self.set_header_end(dom);
        self
    }

    /// Where the pane is docked.
    pub const fn set_dock(&mut self, dock: InspectorDock) {
        self.dock = dock;
    }

    /// [`Self::set_dock`] for the builder chain.
    #[must_use]
    pub const fn with_dock(mut self, dock: InspectorDock) -> Self {
        self.set_dock(dock);
        self
    }

    /// Desktop or phone: `Auto` (the default) asks the window.
    pub const fn set_form(&mut self, form: FormFactor) {
        self.form = form;
    }

    /// [`Self::set_form`] for the builder chain.
    #[must_use]
    pub const fn with_form(mut self, form: FormFactor) -> Self {
        self.set_form(form);
        self
    }

    /// The pane's width or height in px.
    pub const fn set_size(&mut self, size: f32) {
        self.size = size;
    }

    /// [`Self::set_size`] for the builder chain.
    #[must_use]
    pub const fn with_size(mut self, size: f32) -> Self {
        self.set_size(size);
        self
    }

    /// Whether the pane shows.
    pub const fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    /// [`Self::set_open`] for the builder chain.
    #[must_use]
    pub const fn with_open(mut self, open: bool) -> Self {
        self.set_open(open);
        self
    }

    /// What the pane is painted with.
    pub const fn set_surface(&mut self, surface: SidebarSurface) {
        self.surface = surface;
    }

    /// [`Self::set_surface`] for the builder chain.
    #[must_use]
    pub const fn with_surface(mut self, surface: SidebarSurface) -> Self {
        self.set_surface(surface);
        self
    }

    /// Pin the widget theme; unset, the Inspector follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Every change of the pane's size.
    pub fn set_on_event<C: Into<InspectorOnEventCallback>>(&mut self, data: RefAny, on_event: C) {
        self.on_event = OptionInspectorOnEvent::Some(InspectorOnEvent {
            refany: data,
            callback: on_event.into(),
        });
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<InspectorOnEventCallback>>(mut self, data: RefAny, on_event: C) -> Self {
        self.set_on_event(data, on_event);
        self
    }

    /// The dock this Inspector is docked in now: never `Auto`.
    #[must_use]
    pub fn resolved_dock(&self) -> InspectorDock {
        self.dock.resolved(self.form)
    }

    /// The pane's size as it is built: its width or height held to the limits (the container is
    /// not known at build time: the upper limit is applied while it is dragged).
    #[must_use]
    pub fn built_size(&self) -> f32 {
        match self.resolved_dock() {
            InspectorDock::Bottom => clamp_pane_height(self.size, f32::INFINITY),
            _ => clamp_inspector_width(self.size, 0.0),
        }
    }

    /// The Inspector's DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        build(self)
    }
}

impl Default for Inspector {
    fn default() -> Self {
        Self::create(Dom::create_div(), Dom::create_div())
    }
}

impl From<Inspector> for Dom {
    fn from(i: Inspector) -> Self {
        i.dom()
    }
}

// ---------------------------------------------------------------------------
// The drag
// ---------------------------------------------------------------------------

/// What the drag handlers share: the app's callback, the dock, the size now and the anchor of the
/// drag in flight. The container's dataset, so a rebuild can carry the drag across.
#[derive(Clone)]
struct DragState {
    on_event: OptionInspectorOnEvent,
    dock: InspectorDock,
    size: f32,
    dragging: bool,
    /// The pointer's place along the drag axis at the press, container coordinates.
    start_main: f32,
    /// The pane's size at the press.
    start_size: f32,
}

impl DragState {
    /// Whether a press at (`x`, `y`) in a container `w` x `h` is on the grab zone of the pane.
    fn grabs(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        match self.dock {
            InspectorDock::Bottom => {
                let top = h - self.size;
                y >= top - 2.0 && y <= top + GRIP_HEIGHT
            }
            _ => {
                let border = w - self.size;
                (x - border).abs() <= BORDER_GRAB / 2.0 + 3.0
            }
        }
    }

    /// The pointer's place along the drag axis.
    const fn main_of(&self, pos: CursorNodePosition) -> f32 {
        match self.dock {
            InspectorDock::Bottom => pos.y,
            _ => pos.x,
        }
    }

    /// The size the pointer at `now` makes of the pane in a container `w` x `h`.
    fn size_at(&self, now: f32, w: f32, h: f32) -> f32 {
        match self.dock {
            InspectorDock::Bottom => dragged_pane_height(self.start_size, self.start_main, now, h),
            _ => dragged_inspector_width(self.start_size, self.start_main, now, w),
        }
    }
}

fn size_property(dock: InspectorDock, px: f32) -> CssProperty {
    match dock {
        InspectorDock::Bottom => CssProperty::const_height(LayoutHeight::px(px)),
        _ => CssProperty::const_width(LayoutWidth::px(px)),
    }
}

/// Tells the app: the event as the state has it now.
fn report(state: &mut DragState, info: CallbackInfo, kind: InspectorEventKind) -> Update {
    let ev = InspectorEvent {
        kind,
        size: state.size,
        dock: state.dock,
    };
    match state.on_event.as_mut() {
        Some(InspectorOnEvent { callback, refany }) => callback.invoke(refany.clone(), info, ev),
        None => Update::DoNothing,
    }
}

/// The pane of the container the callback is on: the body is its first child, the pane the next.
fn pane_of(info: &CallbackInfo, container: azul_core::dom::DomNodeId) -> Option<azul_core::dom::DomNodeId> {
    info.get_first_child(container).and_then(|body| info.get_next_sibling(body))
}

extern "C" fn on_press(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(pos) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
    };
    let Some(rect) = info.get_hit_node_rect() else {
        return Update::DoNothing;
    };
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    if !st.grabs(pos.x, pos.y, rect.size.width, rect.size.height) {
        return Update::DoNothing;
    }
    st.dragging = true;
    st.start_main = st.main_of(pos);
    st.start_size = st.size;
    let container = info.get_hit_node();
    info.capture_pointer(container);
    Update::DoNothing
}

extern "C" fn on_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    if !st.dragging {
        return Update::DoNothing;
    }
    let Some(pos) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
    };
    let Some(rect) = info.get_hit_node_rect() else {
        return Update::DoNothing;
    };
    let size = st.size_at(st.main_of(pos), rect.size.width, rect.size.height);
    if !size.is_finite() {
        return Update::DoNothing;
    }
    st.size = size;
    let container = info.get_hit_node();
    if let Some(pane) = pane_of(&info, container) {
        info.set_css_property(pane, size_property(st.dock, size));
    }
    report(&mut st, info, InspectorEventKind::Resized)
}

extern "C" fn on_release(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    if !st.dragging {
        return Update::DoNothing;
    }
    st.dragging = false;
    let container = info.get_hit_node();
    if st.dock == InspectorDock::Bottom {
        if let Some(rect) = info.get_hit_node_rect() {
            st.size = settle_pane_height(st.size, rect.size.height);
        }
    }
    if let Some(pane) = pane_of(&info, container) {
        info.set_css_property(pane, size_property(st.dock, st.size));
    }
    report(&mut st, info, InspectorEventKind::Settled)
}

/// A leave with the button already up is a release that never came: the drag ends.
extern "C" fn on_leave(mut data: RefAny, info: CallbackInfo) -> Update {
    if info.get_current_mouse_state().left_down {
        return Update::DoNothing;
    }
    if let Some(mut st) = data.downcast_mut::<DragState>() {
        st.dragging = false;
    }
    Update::DoNothing
}

/// The arrow keys on the focused grip (the APG window splitter): by 16 px, 64 with Ctrl / Cmd,
/// Home / End to the ends.
extern "C" fn on_grip_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let ks = info.get_current_keyboard_state();
    let Some(code) = ks.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    let bottom = st.dock == InspectorDock::Bottom;
    let key = match code {
        VirtualKeyCode::Up if bottom => GripKey::Grow,
        VirtualKeyCode::Down if bottom => GripKey::Shrink,
        VirtualKeyCode::Left if !bottom => GripKey::Grow,
        VirtualKeyCode::Right if !bottom => GripKey::Shrink,
        VirtualKeyCode::Home => GripKey::ToMin,
        VirtualKeyCode::End => GripKey::ToMax,
        _ => return Update::DoNothing,
    };
    let grip = info.get_hit_node();
    let Some(pane) = info.get_parent(grip) else {
        return Update::DoNothing;
    };
    let Some(container) = info.get_parent(pane) else {
        return Update::DoNothing;
    };
    let Some(rect) = info.get_node_size(container) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    st.size = step_size(st.dock, key, st.size, ks.primary_down(), rect.width, rect.height);
    info.set_css_property(pane, size_property(st.dock, st.size));
    report(&mut st, info, InspectorEventKind::Settled)
}

/// A double click or tap on the grip: the pane goes to its peek, or back to its default.
extern "C" fn on_grip_double(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    let grip = info.get_hit_node();
    let Some(pane) = info.get_parent(grip) else {
        return Update::DoNothing;
    };
    st.size = match st.dock {
        InspectorDock::Bottom if st.size > PANE_PEEK + 1.0 => PANE_PEEK,
        InspectorDock::Bottom => PANE_HEIGHT_DEFAULT,
        _ => INSPECTOR_WIDTH_DEFAULT,
    };
    info.set_css_property(pane, size_property(st.dock, st.size));
    report(&mut st, info, InspectorEventKind::Settled)
}

/// Carries a drag across a rebuild of the app's DOM: while the pointer is down the drag's anchor
/// and the live size stay (the app was told the size through `on_event`; one that stores it
/// builds the same number, one that does not would otherwise yank the pane from under the
/// pointer). Once it is up the app's size is the truth again. A rebuild that changed the dock
/// starts over. The callback is always the FRESH build's.
#[must_use]
pub extern "C" fn merge_inspector_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let new_guard = new_data.downcast_mut::<DragState>();
        let old_guard = old_data.downcast_ref::<DragState>();
        if let (Some(mut new_g), Some(old_g)) = (new_guard, old_guard) {
            if old_g.dragging && old_g.dock == new_g.dock {
                new_g.dragging = true;
                new_g.start_main = old_g.start_main;
                new_g.start_size = old_g.start_size;
                new_g.size = old_g.size;
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

fn cb(filter: EventFilter, data: RefAny, f: extern "C" fn(RefAny, CallbackInfo) -> Update) -> CoreCallbackData {
    CoreCallbackData {
        event: filter,
        callback: CoreCallback {
            cb: f as usize,
            ctx: OptionRefAny::None,
        },
        refany: data,
    }
}

/// The pane's ground and hairline in this surface.
fn ground(surface: SidebarSurface) -> String {
    match surface {
        SidebarSurface::Theme => String::from("background: system:window-background; color: system:text;"),
        other => String::from(other.root_css()),
    }
}

fn build(i: Inspector) -> Dom {
    let dock = i.resolved_dock();
    let size = i.built_size();
    let Inspector {
        body,
        pane,
        title,
        header_end,
        open,
        surface,
        on_event,
        ..
    } = i;
    let bottom = dock == InspectorDock::Bottom;
    let root_css = format!(
        "display: flex; flex-direction: {}; flex-grow: 1; min-width: 0px; min-height: 0px; width: 100%; \
         height: 100%; overflow: hidden; position: relative; box-sizing: border-box;",
        if bottom { "column" } else { "row" }
    );
    let body = styled(
        Dom::create_div().with_child(body),
        "display: flex; flex-direction: column; flex-grow: 1; flex-shrink: 1; flex-basis: 0px; min-width: 0px; \
         min-height: 0px; overflow: hidden;",
    )
    .with_ids_and_classes(classes(&[INSPECTOR_BODY_CLASS]));
    let dock_class = if bottom { INSPECTOR_BOTTOM_CLASS } else { INSPECTOR_RIGHT_CLASS };

    if !open {
        return styled(Dom::create_div().with_child(body), &root_css)
            .with_ids_and_classes(classes(&[INSPECTOR_CLASS, dock_class]));
    }

    let state = RefAny::new(DragState {
        on_event: on_event.clone(),
        dock,
        size,
        dragging: false,
        start_main: 0.0,
        start_size: size,
    });

    // the grab zone: the grip row at the top of a bottom pane, the border strip at the left of a
    // right pane (it reaches over the body's edge, so it is clear of the pane's clipped content)
    let grip = if bottom {
        let pill = styled(
            Dom::create_div(),
            "width: 40px; height: 4px; border-radius: 2px; background: currentColor; opacity: 0.45;",
        )
        .with_ids_and_classes(classes(&[INSPECTOR_PILL_CLASS]));
        styled(
            Dom::create_div().with_child(pill),
            &format!(
                "display: flex; align-items: center; justify-content: center; flex-shrink: 0; width: 100%; \
                 height: {GRIP_HEIGHT}px; cursor: row-resize;"
            ),
        )
    } else {
        styled(
            Dom::create_div(),
            &format!(
                "position: absolute; left: -{half}px; top: 0px; bottom: 0px; width: {BORDER_GRAB}px; \
                 cursor: col-resize; z-index: 3; :hover {{ background: rgba(127, 127, 127, 0.22); }}",
                half = BORDER_GRAB / 2.0
            ),
        )
    }
    .with_ids_and_classes(classes(&[INSPECTOR_GRIP_CLASS]))
    .with_accessibility_info(AccessibilityInfo {
        role: AccessibilityRole::Grip,
        accessibility_name: azul_css::OptionString::Some(AzString::from(format!("Resize {}", title.as_str()))),
        accessibility_value: azul_css::OptionString::Some(AzString::from(format!("{}", size.round()))),
        ..Default::default()
    })
    .with_tab_index(TabIndex::Auto)
    .with_callbacks(
        alloc::vec![
            cb(EventFilter::Focus(FocusEventFilter::VirtualKeyDown), state.clone(), on_grip_key),
            cb(EventFilter::Hover(HoverEventFilter::DoubleClick), state.clone(), on_grip_double),
        ]
        .into(),
    );

    let mut header_kids: Vec<Dom> = alloc::vec![styled(
        widget_p_with_text(title.clone()),
        "flex-grow: 1; min-width: 0px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; margin: 0px;",
    )];
    if let Some(end) = header_end.into_option() {
        header_kids.push(styled(Dom::create_div().with_child(end), "flex-shrink: 0; display: flex; align-items: center;"));
    }
    let header = styled(
        Dom::create_div().with_children(DomVec::from_vec(header_kids)),
        &format!(
            "display: flex; flex-direction: row; align-items: center; gap: 8px; flex-shrink: 0; \
             box-sizing: border-box; height: {}px; padding: 0px 12px; font-size: 12px; font-weight: 700;",
            if bottom { 28 } else { 36 }
        ),
    )
    .with_ids_and_classes(classes(&[INSPECTOR_HEADER_CLASS]));

    let content = styled(
        Dom::create_div().with_child(pane),
        "display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; min-height: 0px; min-width: 0px; \
         overflow: auto;",
    )
    .with_ids_and_classes(classes(&[INSPECTOR_CONTENT_CLASS]));

    let pane_css = if bottom {
        format!(
            "display: flex; flex-direction: column; flex-grow: 0; flex-shrink: 0; width: 100%; height: {size}px; \
             box-sizing: border-box; position: relative; border-top: 1px solid system:separator; \
             border-radius: 12px 12px 0px 0px; overflow: hidden; {}",
            ground(surface)
        )
    } else {
        format!(
            "display: flex; flex-direction: column; flex-grow: 0; flex-shrink: 0; width: {size}px; height: 100%; \
             box-sizing: border-box; position: relative; border-left: 1px solid system:separator; {}",
            ground(surface)
        )
    };
    let pane = styled(
        Dom::create_div().with_children(DomVec::from_vec(alloc::vec![grip, header, content])),
        &pane_css,
    )
    .with_ids_and_classes(classes(&[INSPECTOR_PANE_CLASS]))
    .with_accessibility_info(AccessibilityInfo::named(title, AccessibilityRole::Pane));

    styled(Dom::create_div().with_children(DomVec::from_vec(alloc::vec![body, pane])), &root_css)
        .with_ids_and_classes(classes(&[INSPECTOR_CLASS, dock_class]))
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
        .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(merge_inspector_state))
}

/// `pane_max` for the callers that name the whole range of a pane in a container `h` tall.
#[must_use]
pub fn pane_range(container_h: f32) -> (f32, f32) {
    (PANE_PEEK, pane_max(container_h))
}

#[cfg(test)]
#[path = "inspector_test.rs"]
mod inspector_test;
