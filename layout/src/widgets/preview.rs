//! The Preview: ONE widget with TWO docks. The pane an app shows beside what the user is
//! working on, and what it shows is the ITEM ITSELF - AzDrive's file, AzMail's message,
//! AzCalendar's event - and nothing else.
//!
//! ```text
//!  desktop: docked RIGHT, below the tab ribbon        phone: docked at the BOTTOM
//! ┌─ tab ribbon, the full width ─────────────────┐    ┌──────────────────────┐
//! ├────────────────────────────┬─┬───────────────┤    │ main view            │
//! │ body                       │▐│ ⚠ banner  (?) │    │ ...                  │
//! │                            │▐│ Preview  (?) × │    ├────────── ▬▬ ────────┤  the grip bar
//! │                            │▐│               │    │ Preview  ▬▬   (?)    │
//! │                            │▐│  the item     │    │ ⚠ banner (when set)  │
//! └────────────────────────────┴─┴───────────────┘    │ the item             │
//!   drag the left border (the width)                    └──────────────────────┘
//!                                                         drag the grip (the height)
//! ```
//!
//! The Preview is OPT-IN: an app that does not add one (AzWriter and AzNotes give their text all
//! the room) has no dock, no split handle, no grip and no bottom pane. When the app adds it, the
//! Preview wraps the BODY it sits beside and the PANE it shows: a desktop gets a pane at the right
//! below whatever the app puts above it (the tab ribbon runs over its full width), a phone a pane
//! at the bottom (the footer line stays under it) - never both, never an overlay: it is a split.
//! [`PreviewDock::Auto`] chooses by the form factor; an app names a dock to fix it.
//!
//! # What it shows, and what it does not
//!
//! The pane is for the item itself. Everything else about the item (properties, history,
//! comments, warnings) is behind small (?) icon buttons in its header ([`PreviewDetail`], one
//! popover each): nothing of it is dumped by default. When something needs saying at once - a
//! security warning, a link-strength badge - a one-line BANNER ([`PreviewBanner`]) stands at the
//! very top of the pane (under the grip bar of a bottom pane) and its details are behind its own
//! (?); with no banner the slot does not exist.
//!
//! # The docks
//!
//! The right pane is resized by dragging its left border (its width, between 200 px and 60 % of
//! the container, leaving the body 240 px). The bottom pane has four DETENTS - the collapsed bar
//! (30 px: the grip with the title in it), the peek (130 px), the half and the full height (85 %
//! of the container) - with a smooth drag between them: a pane released within 28 px of a detent
//! settles on it, between them it stays where it was left. A tap on the grip toggles the peek and
//! the half. The arrow keys on the focused grip move it by 16 px (64 with Ctrl / Cmd), Home and
//! End go to the collapsed bar and the full height, Enter and Space toggle the peek and the half
//! - the APG window splitter. A drag never closes the pane: only the app does (`open`).
//!
//! The Preview owns nothing: the app keeps `size` and `open`, hears every change through ONE
//! callback ([`PreviewOnEvent`]) and rebuilds. A drag in flight survives a rebuild
//! (`merge_preview_state`): the pointer wins while it is down.
//!
//! Key types: [`Preview`], [`PreviewDetail`], [`PreviewBanner`], [`PreviewEvent`],
//! [`PreviewEventKind`], [`PreviewOnEvent`], [`PreviewDock`].

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
    transient::TransientAnchor,
    window::VirtualKeyCode,
};
use azul_css::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, props::layout::LayoutHeight,
    props::layout::LayoutWidth, props::property::{CssProperty, CssPropertyType}, AzString,
};

pub use super::sidebar_layout::PreviewDock;
use super::{
    dialog::{Dialog, DialogClosedBy},
    sidebar::SidebarSurface,
    sidebar_layout::{
        clamp_pane_height, clamp_preview_width, dragged_pane_height, dragged_preview_width_at, pane_max,
        preview_edge, settle_pane_height, step_size, toggle_pane_height, Edge, FormFactor, GripKey,
        BORDER_GRAB, BUILT_FOR, GRIP_HEIGHT, GRIP_SLOP, PANE_COLLAPSED, PREVIEW_WIDTH_DEFAULT,
    },
    themes::{OptionUiTheme, UiTheme},
    widget_p_with_text,
};
use crate::callbacks::CallbackInfo;

/// The Preview's root: the body and the pane.
pub const PREVIEW_CLASS: &str = "__azul-native-preview";
/// Added to the root while the pane is docked at the right.
pub const PREVIEW_RIGHT_CLASS: &str = "__azul-native-preview-right";
/// Added to the root while the pane is docked at the bottom.
pub const PREVIEW_BOTTOM_CLASS: &str = "__azul-native-preview-bottom";
/// The body beside or above the pane.
pub const PREVIEW_BODY_CLASS: &str = "__azul-native-preview-body";
/// The pane.
pub const PREVIEW_PANE_CLASS: &str = "__azul-native-preview-pane";
/// The pane's grab zone: the grip bar (bottom) or the left border (right).
pub const PREVIEW_GRIP_CLASS: &str = "__azul-native-preview-grip";
/// The grip's pill.
pub const PREVIEW_PILL_CLASS: &str = "__azul-native-preview-pill";
/// The pane's header row: the title and the (?) buttons (the grip bar of a bottom pane).
pub const PREVIEW_HEADER_CLASS: &str = "__azul-native-preview-header";
/// The pane's content.
pub const PREVIEW_CONTENT_CLASS: &str = "__azul-native-preview-content";
/// A (?) button of the header: it opens the popover of one [`PreviewDetail`].
pub const PREVIEW_HELP_CLASS: &str = "__azul-native-preview-help";
/// The banner slot, present only while the app sets a banner.
pub const PREVIEW_BANNER_CLASS: &str = "__azul-native-preview-banner";
/// The popover of a detail or of a banner's details.
pub const PREVIEW_POPOVER_CLASS: &str = "__azul-native-preview-popover";

/// What happened on the Preview.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum PreviewEventKind {
    /// The pane is being dragged (or a key moved it): `size` is its size now. Fires for every
    /// move; an app that keeps the size stores it, one that does not loses nothing.
    Resized,
    /// The pane was let go (or a key, a tap on the grip, settled it): `size` is where it stays,
    /// the detents already snapped to.
    Settled,
}

/// One report of the Preview.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct PreviewEvent {
    /// What happened.
    pub kind: PreviewEventKind,
    /// The pane's width (right dock) or height (bottom dock), px.
    pub size: f32,
    /// The dock the pane is in (never `Auto`).
    pub dock: PreviewDock,
}

/// Callback invoked for every change of the pane's size.
pub type PreviewOnEventCallbackType = extern "C" fn(RefAny, CallbackInfo, PreviewEvent) -> Update;
impl_widget_callback!(
    PreviewOnEvent,
    OptionPreviewOnEvent,
    PreviewOnEventCallback,
    PreviewOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        PreviewOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: PREVIEW_ON_EVENT_INVOKER,
    invoker_ty:     AzPreviewOnEventCallbackInvoker,
    thunk_fn:       az_preview_on_event_callback_thunk,
    setter_fn:      AzApp_setPreviewOnEventCallbackInvoker,
    from_handle_fn: AzPreviewOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzPreviewOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: PreviewEvent ],
}

// ---------------------------------------------------------------------------
// The details behind the (?) and the banner
// ---------------------------------------------------------------------------

/// What a banner says: how urgent.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum PreviewBannerTone {
    /// A plain remark (a link-strength badge: key, server, app).
    #[default]
    Info,
    /// A warning (an unverified signer).
    Warning,
    /// Danger (a phishing form, "shared by X: contains .git/hooks").
    Danger,
}

impl PreviewBannerTone {
    /// The icon of the tone when the banner names none.
    const fn icon(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Danger => "error",
        }
    }

    /// The wash behind the banner and the ink of its icon.
    const fn ink(self) -> (&'static str, &'static str) {
        match self {
            Self::Info => ("rgba(74, 139, 232, 0.16)", "#4a8be8"),
            Self::Warning => ("rgba(232, 163, 23, 0.22)", "#c98a0a"),
            Self::Danger => ("rgba(214, 69, 65, 0.20)", "#d64541"),
        }
    }
}

/// The one-line banner at the very top of the Preview: shown only when something needs saying; its
/// details are behind its own (?).
#[derive(Debug, Clone)]
#[repr(C)]
pub struct PreviewBanner {
    /// The line ("Unverified signer"): one line, cut with an ellipsis where it does not fit.
    pub text: AzString,
    /// The icon, a Material name; empty: the tone's icon.
    pub icon: AzString,
    /// What the banner's (?) opens: the details (who signed it, what it means); none: no (?).
    pub details: OptionDom,
    /// How urgent.
    pub tone: PreviewBannerTone,
}

impl_option!(PreviewBanner, OptionPreviewBanner, copy = false, [Debug, Clone]);

impl PreviewBanner {
    /// A banner saying `text` in `tone`.
    #[must_use]
    pub const fn create(text: AzString, tone: PreviewBannerTone) -> Self {
        Self {
            text,
            icon: AzString::from_const_str(""),
            details: OptionDom::None,
            tone,
        }
    }

    /// The icon, a Material name.
    pub fn set_icon(&mut self, icon: AzString) {
        self.icon = icon;
    }

    /// [`Self::set_icon`] for the builder chain.
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.set_icon(icon);
        self
    }

    /// What the banner's (?) opens.
    pub fn set_details(&mut self, details: Dom) {
        self.details = OptionDom::Some(details);
    }

    /// [`Self::set_details`] for the builder chain.
    #[must_use]
    pub fn with_details(mut self, details: Dom) -> Self {
        self.set_details(details);
        self
    }
}

/// One thing behind a (?) in the Preview's header: the properties of the item, its history, its
/// comments. Each is a small icon button; a click opens its content as a short popover.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct PreviewDetail {
    /// What opens: the popover's content.
    pub content: Dom,
    /// What the button is called ("Properties"): its tooltip and its accessible name.
    pub label: AzString,
    /// The button's icon, a Material name; empty: the (?) mark (`help_outline`).
    pub icon: AzString,
}

impl_option!(PreviewDetail, OptionPreviewDetail, copy = false, [Debug, Clone]);
impl_vec!(
    PreviewDetail,
    PreviewDetailVec,
    PreviewDetailVecDestructor,
    PreviewDetailVecDestructorType,
    PreviewDetailVecSlice,
    OptionPreviewDetail
);
impl_vec_clone!(PreviewDetail, PreviewDetailVec, PreviewDetailVecDestructor);
impl_vec_debug!(PreviewDetail, PreviewDetailVec);
impl_vec_mut!(PreviewDetail, PreviewDetailVec);

impl PreviewDetail {
    /// A (?) button named `label` that opens `content`.
    #[must_use]
    pub const fn create(label: AzString, content: Dom) -> Self {
        Self {
            content,
            label,
            icon: AzString::from_const_str(""),
        }
    }

    /// The button's icon instead of the (?) mark.
    pub fn set_icon(&mut self, icon: AzString) {
        self.icon = icon;
    }

    /// [`Self::set_icon`] for the builder chain.
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.set_icon(icon);
        self
    }
}

/// The most (?) buttons the header shows: the rest is dropped with a diagnostic.
pub const MAX_DETAILS: usize = 6;

// ---------------------------------------------------------------------------
// The widget
// ---------------------------------------------------------------------------

/// The Preview: a body and the pane that is docked to it.
///
/// The fields stand in the order of their alignment (the ABI check wants no padding between them).
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Preview {
    /// What the pane sits beside (right dock) or under (bottom dock): the app's main view.
    pub body: Dom,
    /// The pane's content: the item itself, the picture, the page, the message.
    pub pane: Dom,
    /// What stands at the end of the header row (a close button, a mini player): the pane at its
    /// collapsed bar still shows it.
    pub header_end: OptionDom,
    /// Every change of the size.
    pub on_event: OptionPreviewOnEvent,
    /// The banner slot at the very top: none, no slot.
    pub banner: OptionPreviewBanner,
    /// What sits behind the (?) buttons of the header (held to `MAX_DETAILS`); none, no button.
    pub details: PreviewDetailVec,
    /// The pane's title ("Preview", the item's name): its header and its accessible name.
    pub title: AzString,
    /// The DOM id of the Preview's root (`#id` in a selector, an e2e script finds it by it); empty
    /// for none.
    pub dom_id: AzString,
    /// The widget theme this Preview is PINNED to, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
    /// Where the pane is docked; `Auto` is the right on a desktop and the bottom on a phone.
    pub dock: PreviewDock,
    /// Desktop or phone: `Auto` asks the window.
    pub form: FormFactor,
    /// What the pane is painted with.
    pub surface: SidebarSurface,
    /// The pane's width (right) or height (bottom) in px; 0 is the default (320 wide, the 130 px
    /// peek tall).
    pub size: f32,
    /// Whether the pane shows. Closed, the body fills everything.
    pub open: bool,
}

impl_option!(Preview, OptionPreview, copy = false, [Debug, Clone]);

impl Preview {
    /// An open Preview of `pane` beside `body`, docked by the form factor.
    #[must_use]
    pub const fn create(body: Dom, pane: Dom) -> Self {
        Self {
            body,
            pane,
            header_end: OptionDom::None,
            on_event: OptionPreviewOnEvent::None,
            banner: OptionPreviewBanner::None,
            details: PreviewDetailVec::from_const_slice(&[]),
            title: AzString::from_const_str("Preview"),
            dom_id: AzString::from_const_str(""),
            theme: OptionUiTheme::None,
            dock: PreviewDock::Auto,
            form: FormFactor::Auto,
            surface: SidebarSurface::Theme,
            size: 0.0,
            open: true,
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

    /// The DOM id of the Preview's root, so that a selector (`#mail-preview`) names it.
    pub fn set_dom_id(&mut self, dom_id: AzString) {
        self.dom_id = dom_id;
    }

    /// [`Self::set_dom_id`] for the builder chain.
    #[must_use]
    pub fn with_dom_id(mut self, dom_id: AzString) -> Self {
        self.set_dom_id(dom_id);
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

    /// The banner slot at the very top of the pane.
    pub fn set_banner(&mut self, banner: PreviewBanner) {
        self.banner = OptionPreviewBanner::Some(banner);
    }

    /// [`Self::set_banner`] for the builder chain.
    #[must_use]
    pub fn with_banner(mut self, banner: PreviewBanner) -> Self {
        self.set_banner(banner);
        self
    }

    /// Replaces the details behind the (?) buttons of the header.
    pub fn set_details(&mut self, details: PreviewDetailVec) {
        self.details = details;
    }

    /// [`Self::set_details`] for the builder chain.
    #[must_use]
    pub fn with_details(mut self, details: PreviewDetailVec) -> Self {
        self.set_details(details);
        self
    }

    /// Appends one detail behind a (?) button of the header.
    pub fn add_detail(&mut self, detail: PreviewDetail) {
        let mut v = self.details.clone().into_library_owned_vec();
        v.push(detail);
        self.details = PreviewDetailVec::from_vec(v);
    }

    /// [`Self::add_detail`] for the builder chain.
    #[must_use]
    pub fn with_detail(mut self, detail: PreviewDetail) -> Self {
        self.add_detail(detail);
        self
    }

    /// Where the pane is docked.
    pub const fn set_dock(&mut self, dock: PreviewDock) {
        self.dock = dock;
    }

    /// [`Self::set_dock`] for the builder chain.
    #[must_use]
    pub const fn with_dock(mut self, dock: PreviewDock) -> Self {
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

    /// Pin the widget theme; unset, the Preview follows the app theme.
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
    pub fn set_on_event<C: Into<PreviewOnEventCallback>>(&mut self, data: RefAny, on_event: C) {
        self.on_event = OptionPreviewOnEvent::Some(PreviewOnEvent {
            refany: data,
            callback: on_event.into(),
        });
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<PreviewOnEventCallback>>(mut self, data: RefAny, on_event: C) -> Self {
        self.set_on_event(data, on_event);
        self
    }

    /// The dock this Preview is docked in now: never `Auto`.
    #[must_use]
    pub fn resolved_dock(&self) -> PreviewDock {
        self.dock.resolved(self.form)
    }

    /// The pane's size as it is built: its width or height held to the limits (the container is
    /// not known at build time: the upper limit is applied while it is dragged).
    #[must_use]
    pub fn built_size(&self) -> f32 {
        match self.resolved_dock() {
            PreviewDock::Bottom => clamp_pane_height(self.size, f32::INFINITY),
            _ => clamp_preview_width(self.size, 0.0),
        }
    }

    /// The Preview's DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        build(self)
    }
}

impl Default for Preview {
    fn default() -> Self {
        Self::create(Dom::create_div(), Dom::create_div())
    }
}

impl From<Preview> for Dom {
    fn from(i: Preview) -> Self {
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
    on_event: OptionPreviewOnEvent,
    /// The edge a right dock stands at (`preview_edge`: the one place that decides it).
    edge: Edge,
    dock: PreviewDock,
    size: f32,
    dragging: bool,
    /// Whether the pointer has moved past the slop since the press: a press that has not is a
    /// TAP (it toggles the peek and the half).
    moved: bool,
    /// The pointer's place along the drag axis at the press, container coordinates.
    start_main: f32,
    /// The pane's size at the press.
    start_size: f32,
}

impl DragState {
    /// Whether a press at (`x`, `y`) in a container `w` x `h` is on the grab zone of the pane.
    fn grabs(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        if self.dock == PreviewDock::Bottom {
            let top = h - self.size;
            y >= top - 2.0 && y <= top + GRIP_HEIGHT
        } else {
            let border = if self.edge == Edge::Right { w - self.size } else { self.size };
            (x - border).abs() <= BORDER_GRAB / 2.0 + 3.0
        }
    }

    /// The pointer's place along the drag axis.
    const fn main_of(&self, pos: CursorNodePosition) -> f32 {
        match self.dock {
            PreviewDock::Bottom => pos.y,
            _ => pos.x,
        }
    }

    /// The size the pointer at `now` makes of the pane in a container `w` x `h`.
    fn size_at(&self, now: f32, w: f32, h: f32) -> f32 {
        match self.dock {
            PreviewDock::Bottom => dragged_pane_height(self.start_size, self.start_main, now, h),
            _ => dragged_preview_width_at(self.edge, self.start_size, self.start_main, now, w),
        }
    }
}

fn size_property(dock: PreviewDock, px: f32) -> CssProperty {
    match dock {
        PreviewDock::Bottom => CssProperty::const_height(LayoutHeight::px(px)),
        _ => CssProperty::const_width(LayoutWidth::px(px)),
    }
}

/// The pane's size property type: the one the drag writes.
const fn size_type(dock: PreviewDock) -> CssPropertyType {
    match dock {
        PreviewDock::Bottom => CssPropertyType::Height,
        _ => CssPropertyType::Width,
    }
}

/// While the pointer or a key is moving the pane: a cascade-only write of its size (the override
/// channel, cheap enough for every move of a drag).
fn size_while_moving(info: &mut CallbackInfo, pane: azul_core::dom::DomNodeId, dock: PreviewDock, px: f32) {
    info.override_css_property(pane, size_property(dock, px));
}

/// When the size is settled: written into the pane's own style, and the override of the drag
/// DROPPED. An override follows the node's identity across the app's next rebuild and outranks
/// what the app builds, so one left behind would hold the pane at the dragged size when the app
/// later moves it by itself (a command, a new selection): the pane keeps the size until the
/// app's next build, and from then on what the app builds is the truth again.
fn size_settled(info: &mut CallbackInfo, pane: azul_core::dom::DomNodeId, dock: PreviewDock, px: f32) {
    info.set_css_property(pane, size_property(dock, px));
    info.override_css_property(pane, CssProperty::initial(size_type(dock)));
}

/// Tells the app: the event as the state has it now.
fn report(state: &mut DragState, info: CallbackInfo, kind: PreviewEventKind) -> Update {
    let ev = PreviewEvent {
        kind,
        size: state.size,
        dock: state.dock,
    };
    match state.on_event.as_mut() {
        Some(PreviewOnEvent { callback, refany }) => callback.invoke(refany.clone(), info, ev),
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
    st.moved = false;
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
    let main = st.main_of(pos);
    if !st.moved && (main - st.start_main).abs() <= GRIP_SLOP {
        // still a press, not yet a drag: the pane stays where it is
        return Update::DoNothing;
    }
    st.moved = true;
    let size = st.size_at(main, rect.size.width, rect.size.height);
    if !size.is_finite() {
        return Update::DoNothing;
    }
    st.size = size;
    let container = info.get_hit_node();
    if let Some(pane) = pane_of(&info, container) {
        size_while_moving(&mut info, pane, st.dock, size);
    }
    report(&mut st, info, PreviewEventKind::Resized)
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
    if st.dock == PreviewDock::Bottom {
        if let Some(rect) = info.get_hit_node_rect() {
            // a tap (no drag) toggles the peek and the half, a drag settles on a detent it is near
            st.size = if st.moved {
                settle_pane_height(st.size, rect.size.height)
            } else {
                toggle_pane_height(st.size, rect.size.height)
            };
        }
    } else if !st.moved {
        // a press on the border that did not move is nothing
        return Update::DoNothing;
    }
    if let Some(pane) = pane_of(&info, container) {
        size_settled(&mut info, pane, st.dock, st.size);
    }
    report(&mut st, info, PreviewEventKind::Settled)
}

/// A leave with the button already up is a release that never came: the drag ends where it is
/// (the override of the drag is dropped, the size stays in the pane's own style).
extern "C" fn on_leave(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if info.get_current_mouse_state().left_down {
        return Update::DoNothing;
    }
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    if !st.dragging {
        return Update::DoNothing;
    }
    st.dragging = false;
    if !st.moved {
        return Update::DoNothing;
    }
    let container = info.get_hit_node();
    if let Some(pane) = pane_of(&info, container) {
        size_settled(&mut info, pane, st.dock, st.size);
    }
    report(&mut st, info, PreviewEventKind::Settled)
}

/// The arrow keys on the focused grip (the APG window splitter): by 16 px, 64 with Ctrl / Cmd,
/// Home / End to the ends; Enter and Space toggle the peek and the half of a bottom pane.
extern "C" fn on_grip_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let ks = info.get_current_keyboard_state();
    let Some(code) = ks.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let Some(mut st) = data.downcast_mut::<DragState>() else {
        return Update::DoNothing;
    };
    let bottom = st.dock == PreviewDock::Bottom;
    let key = match code {
        VirtualKeyCode::Up if bottom => GripKey::Grow,
        VirtualKeyCode::Down if bottom => GripKey::Shrink,
        // a dock grows toward the inside: to the left from the right edge, to the right from the left
        VirtualKeyCode::Left if !bottom => {
            if st.edge == Edge::Right { GripKey::Grow } else { GripKey::Shrink }
        }
        VirtualKeyCode::Right if !bottom => {
            if st.edge == Edge::Right { GripKey::Shrink } else { GripKey::Grow }
        }
        VirtualKeyCode::Home => GripKey::ToMin,
        VirtualKeyCode::End => GripKey::ToMax,
        VirtualKeyCode::Return | VirtualKeyCode::Space if bottom => {
            return toggled_by_key(&mut st, info);
        }
        _ => return Update::DoNothing,
    };
    let grip = info.get_hit_node();
    let Some(pane) = grip_pane(&info, grip, bottom) else {
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
    size_settled(&mut info, pane, st.dock, st.size);
    report(&mut st, info, PreviewEventKind::Settled)
}

/// The pane a grip belongs to: the grip's parent (the right dock's strip) or its grandparent (the
/// bottom dock's grip is a layer of the header bar).
fn grip_pane(info: &CallbackInfo, grip: azul_core::dom::DomNodeId, bottom: bool) -> Option<azul_core::dom::DomNodeId> {
    let parent = info.get_parent(grip)?;
    if bottom {
        info.get_parent(parent)
    } else {
        Some(parent)
    }
}

/// Enter or Space on the focused grip of a bottom pane: the peek and the half toggle.
fn toggled_by_key(st: &mut DragState, mut info: CallbackInfo) -> Update {
    let grip = info.get_hit_node();
    let Some(pane) = grip_pane(&info, grip, true) else {
        return Update::DoNothing;
    };
    let Some(container) = info.get_parent(pane) else {
        return Update::DoNothing;
    };
    let Some(rect) = info.get_node_size(container) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    st.size = toggle_pane_height(st.size, rect.height);
    size_settled(&mut info, pane, st.dock, st.size);
    report(st, info, PreviewEventKind::Settled)
}

/// A press on a button of the header is the button's: the drag under it must not begin.
extern "C" fn on_button_press(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    Update::DoNothing
}

/// The tooltip of a (?) button: the name under the pointer.
struct Tip(AzString);

extern "C" fn on_tip_show(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(tip) = data.downcast_ref::<Tip>().map(|t| t.0.clone()) {
        info.show_tooltip(tip);
    }
    Update::DoNothing
}

extern "C" fn on_tip_hide(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.hide_tooltip();
    Update::DoNothing
}

/// Carries a drag across a rebuild of the app's DOM: while the pointer is down the drag's anchor
/// and the live size stay (the app was told the size through `on_event`; one that stores it
/// builds the same number, one that does not would otherwise yank the pane from under the
/// pointer). Once it is up the app's size is the truth again. A rebuild that changed the dock
/// starts over. The callback is always the FRESH build's.
#[must_use]
pub extern "C" fn merge_preview_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let new_guard = new_data.downcast_mut::<DragState>();
        let old_guard = old_data.downcast_ref::<DragState>();
        if let (Some(mut new_g), Some(old_g)) = (new_guard, old_guard) {
            if old_g.dragging && old_g.dock == new_g.dock {
                new_g.dragging = true;
                new_g.moved = old_g.moved;
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

/// A (?) button: a small icon button that opens `content` as a short popover next to it.
fn help_button(
    label: &AzString,
    icon: &str,
    content: Dom,
    anchor: TransientAnchor,
    theme: OptionUiTheme,
) -> Dom {
    let glyph = styled(
        Dom::create_icon(AzString::from(if icon.is_empty() { "help_outline" } else { icon })),
        "font-size: 16px; width: 18px; text-align: center; flex-shrink: 0;",
    );
    let invoker = styled(
        Dom::create_div().with_child(glyph),
        "position: relative; z-index: 2; display: flex; align-items: center; justify-content: center; \
         flex-shrink: 0; width: 26px; height: 24px; border-radius: 6px; cursor: pointer; opacity: 0.72; \
         :hover { background: rgba(127, 127, 127, 0.20); opacity: 1; } \
         :focus-visible { box-shadow: inset 0px 0px 0px 2px #4a8be8; }",
    )
    .with_ids_and_classes(classes(&[PREVIEW_HELP_CLASS]))
    .with_accessibility_info(AccessibilityInfo::named(label.as_str(), AccessibilityRole::PushButton))
    .with_tab_index(TabIndex::Auto)
    .with_callbacks(
        alloc::vec![
            cb(EventFilter::Hover(HoverEventFilter::MouseDown), RefAny::new(()), on_button_press),
            cb(EventFilter::Hover(HoverEventFilter::TouchStart), RefAny::new(()), on_button_press),
            cb(EventFilter::Hover(HoverEventFilter::MouseEnter), RefAny::new(Tip(label.clone())), on_tip_show),
            cb(EventFilter::Hover(HoverEventFilter::MouseLeave), RefAny::new(()), on_tip_hide),
        ]
        .into(),
    );
    let popover = styled(
        Dom::create_div().with_child(content),
        "box-sizing: border-box; padding: 10px 12px; min-width: 200px; max-width: 360px;",
    )
    .with_ids_and_classes(classes(&[PREVIEW_POPOVER_CLASS]));
    let mut dialog = Dialog::create(popover)
        .with_invoker(invoker)
        .with_modal(false)
        .with_anchor(anchor)
        .with_closed_by(DialogClosedBy::Any)
        .with_close_button(false);
    if let Some(t) = theme.into_option() {
        dialog = dialog.with_theme(t);
    }
    styled(Dom::create_div().with_child(dialog.dom()), "display: flex; flex-shrink: 0;")
}

/// The one-line banner: the tone's icon, the line, and a (?) for its details.
fn banner_dom(banner: PreviewBanner, anchor: TransientAnchor, theme: OptionUiTheme) -> Dom {
    let PreviewBanner { text, icon, details, tone } = banner;
    let (wash, ink) = tone.ink();
    let icon_name = if icon.as_str().is_empty() { tone.icon() } else { icon.as_str() };
    let mut kids: Vec<Dom> = alloc::vec![
        styled(
            Dom::create_icon(AzString::from(icon_name)),
            &format!("font-size: 16px; width: 18px; text-align: center; flex-shrink: 0; color: {ink};"),
        ),
        styled(
            widget_p_with_text(text.clone()),
            "flex-grow: 1; flex-shrink: 1; min-width: 0px; overflow: hidden; white-space: nowrap; \
             text-overflow: ellipsis; margin: 0px;",
        ),
    ];
    if let Some(details) = details.into_option() {
        kids.push(help_button(&AzString::from(format!("Details: {}", text.as_str())), "", details, anchor, theme));
    }
    styled(
        Dom::create_div().with_children(DomVec::from_vec(kids)),
        &format!(
            "display: flex; flex-direction: row; align-items: center; gap: 8px; flex-shrink: 0; \
             box-sizing: border-box; width: 100%; min-height: 28px; padding: 2px 8px 2px 12px; \
             font-size: 12px; background: {wash};"
        ),
    )
    .with_ids_and_classes(classes(&[PREVIEW_BANNER_CLASS]))
    .with_accessibility_info(AccessibilityInfo::named(text.as_str(), AccessibilityRole::Alert))
}

/// The most (?) buttons the header shows (with one diagnostic).
fn limited_details(mut details: Vec<PreviewDetail>) -> Vec<PreviewDetail> {
    if details.len() > MAX_DETAILS {
        let extra = details.len() - MAX_DETAILS;
        details.truncate(MAX_DETAILS);
        azul_core::diagnostics::emit_once(0x9e71_ec01, || {
            format!("Preview: {extra} detail button(s) past the limit of {MAX_DETAILS} were dropped")
        });
    }
    details
}

#[allow(clippy::too_many_lines)] // one build: the root, the grip, the header, the banner, the content
fn build(i: Preview) -> Dom {
    let dock = i.resolved_dock();
    let size = i.built_size();
    let Preview {
        body,
        pane,
        header_end,
        on_event,
        banner,
        details,
        title,
        dom_id,
        theme,
        surface,
        open,
        ..
    } = i;
    let bottom = dock == PreviewDock::Bottom;
    // the edge a right dock stands at (the one place that decides it, `preview_edge`)
    let edge = preview_edge(BUILT_FOR);
    let root_css = format!(
        "display: flex; flex-direction: {}; flex-grow: 1; min-width: 0px; min-height: 0px; width: 100%; \
         height: 100%; overflow: hidden; position: relative; box-sizing: border-box;",
        if bottom {
            "column"
        } else if edge == Edge::Right {
            "row"
        } else {
            "row-reverse"
        }
    );
    let body = styled(
        Dom::create_div().with_child(body),
        "display: flex; flex-direction: column; flex-grow: 1; flex-shrink: 1; flex-basis: 0px; min-width: 0px; \
         min-height: 0px; overflow: hidden;",
    )
    .with_ids_and_classes(classes(&[PREVIEW_BODY_CLASS]));
    let dock_class = if bottom { PREVIEW_BOTTOM_CLASS } else { PREVIEW_RIGHT_CLASS };

    if !open {
        return with_dom_id(
            styled(Dom::create_div().with_child(body), &root_css)
                .with_ids_and_classes(classes(&[PREVIEW_CLASS, dock_class])),
            &dom_id,
        );
    }

    let state = RefAny::new(DragState {
        on_event,
        edge,
        dock,
        size,
        dragging: false,
        moved: false,
        start_main: 0.0,
        start_size: size,
    });

    // the popovers open towards the room there is: left of a right pane, above a bottom one
    let anchor = if bottom { TransientAnchor::Top } else { TransientAnchor::Left };
    let details = limited_details(details.into_library_owned_vec());
    let banner = banner.into_option();
    let banner_tone = banner.as_ref().map(|b| b.tone);

    // the grab zone: a layer of the header bar of a bottom pane (the pill in the middle), the
    // border strip at the left of a right pane (it reaches over the body's edge, so it is clear
    // of the pane's clipped content)
    let grip = if bottom {
        let pill = styled(
            Dom::create_div(),
            "width: 40px; height: 4px; border-radius: 2px; background: currentColor; opacity: 0.45;",
        )
        .with_ids_and_classes(classes(&[PREVIEW_PILL_CLASS]));
        styled(
            Dom::create_div().with_child(pill),
            "position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px; z-index: 1; display: flex; \
             align-items: center; justify-content: center; cursor: row-resize;",
        )
    } else {
        styled(
            Dom::create_div(),
            &format!(
                "position: absolute; {side}: -{half}px; top: 0px; bottom: 0px; width: {BORDER_GRAB}px; \
                 cursor: col-resize; z-index: 3; :hover {{ background: rgba(127, 127, 127, 0.22); }}",
                side = edge.inward_offset(),
                half = BORDER_GRAB / 2.0
            ),
        )
    }
    .with_ids_and_classes(classes(&[PREVIEW_GRIP_CLASS]))
    .with_accessibility_info(AccessibilityInfo {
        role: AccessibilityRole::Grip,
        accessibility_name: azul_css::OptionString::Some(AzString::from(format!("Resize {}", title.as_str()))),
        accessibility_value: azul_css::OptionString::Some(AzString::from(format!("{}", size.round()))),
        ..Default::default()
    })
    .with_tab_index(TabIndex::Auto)
    .with_callbacks(
        alloc::vec![cb(EventFilter::Focus(FocusEventFilter::VirtualKeyDown), state.clone(), on_grip_key)].into(),
    );

    // the header: the title, the (?) buttons, what the app puts at its end
    let (bar_grip, strip_grip) = if bottom { (Some(grip), None) } else { (None, Some(grip)) };
    let mut header_kids: Vec<Dom> = Vec::new();
    header_kids.extend(bar_grip);
    // a banner is seen even while the bar is collapsed: its tone's icon stands before the title
    if let (true, Some(tone)) = (bottom, banner_tone) {
        let (_, ink) = tone.ink();
        header_kids.push(styled(
            Dom::create_icon(AzString::from(tone.icon())),
            &format!("position: relative; font-size: 14px; width: 16px; text-align: center; flex-shrink: 0; color: {ink};"),
        ));
    }
    header_kids.push(styled(
        widget_p_with_text(title.clone()),
        if bottom {
            "position: relative; flex-grow: 0; flex-shrink: 1; min-width: 0px; max-width: calc(50% - 44px); \
             overflow: hidden; white-space: nowrap; text-overflow: ellipsis; margin: 0px;"
        } else {
            "flex-grow: 1; min-width: 0px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; margin: 0px;"
        },
    ));
    header_kids.push(styled(Dom::create_div(), "flex-grow: 1; flex-shrink: 1; min-width: 0px;"));
    for detail in details {
        let PreviewDetail { content, label, icon } = detail;
        header_kids.push(help_button(&label, icon.as_str(), content, anchor, theme));
    }
    if let Some(end) = header_end.into_option() {
        header_kids.push(styled(
            Dom::create_div().with_child(end),
            "position: relative; z-index: 2; flex-shrink: 0; display: flex; align-items: center;",
        ));
    }
    let header = styled(
        Dom::create_div().with_children(DomVec::from_vec(header_kids)),
        &format!(
            "display: flex; flex-direction: row; align-items: center; gap: 6px; flex-shrink: 0; position: relative; \
             box-sizing: border-box; height: {}px; padding: 0px 8px 0px 12px; font-size: 12px; font-weight: 700;",
            if bottom { GRIP_HEIGHT } else { 36.0 }
        ),
    )
    .with_ids_and_classes(classes(&[PREVIEW_HEADER_CLASS]));

    let content = styled(
        Dom::create_div().with_child(pane),
        "display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; min-height: 0px; min-width: 0px; \
         overflow: auto;",
    )
    .with_ids_and_classes(classes(&[PREVIEW_CONTENT_CLASS]));

    let banner = banner.map(|b| banner_dom(b, anchor, theme));
    let mut pane_kids: Vec<Dom> = Vec::new();
    if bottom {
        // the grip bar, then the banner under it, then the item
        pane_kids.push(header);
        pane_kids.extend(banner);
    } else {
        // the banner at the very top, then the header, then the item; the border strip is the grip
        pane_kids.extend(banner);
        pane_kids.push(header);
        pane_kids.extend(strip_grip);
    }
    pane_kids.push(content);

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
             box-sizing: border-box; position: relative; {}: 1px solid system:separator; {}",
            edge.inner_border(),
            ground(surface)
        )
    };
    let pane = styled(Dom::create_div().with_children(DomVec::from_vec(pane_kids)), &pane_css)
        .with_ids_and_classes(classes(&[PREVIEW_PANE_CLASS]))
        .with_accessibility_info(AccessibilityInfo::named(title, AccessibilityRole::Pane));

    with_dom_id(
        styled(Dom::create_div().with_children(DomVec::from_vec(alloc::vec![body, pane])), &root_css)
            .with_ids_and_classes(classes(&[PREVIEW_CLASS, dock_class])),
        &dom_id,
    )
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
        .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(merge_preview_state))
}

/// `dom` with the DOM id the app asked for (none: as it is).
fn with_dom_id(dom: Dom, dom_id: &AzString) -> Dom {
    if dom_id.as_str().is_empty() {
        dom
    } else {
        dom.with_id(dom_id.clone())
    }
}

/// `pane_max` for the callers that name the whole range of a pane in a container `h` tall.
#[must_use]
pub fn pane_range(container_h: f32) -> (f32, f32) {
    (PANE_COLLAPSED, pane_max(container_h))
}

#[cfg(test)]
#[path = "preview_test.rs"]
mod preview_test;
