//! Address bar widget - the navigation row of a file manager, as Windows 8's
//! File Explorer draws it: the round Back and Forward arrows, the "Recent
//! locations" chevron, Up, the BREADCRUMB BOX - the location's icon, the path
//! as a row of segments, Refresh at the box's end - and the search box.
//!
//! The breadcrumb box is not a field with links in it. Each folder of the
//! path is a SEGMENT: a button with the folder's name (a click goes there, the
//! current folder's included) followed by a chevron button of its own (a
//! click drops that folder's subfolders - the app opens the menu). A path too
//! long for the box folds its leading segments into a « button, which opens
//! a menu of the hidden ones itself. A click on the box's empty part turns the
//! trail into the editable path; the field takes the keyboard as it opens,
//! Enter goes to the typed path and Escape (or a click elsewhere) gives the
//! edit up. The web [`crate::widgets::breadcrumb::Breadcrumb`] - links with a
//! slash between them - is the wrong shape for this: Explorer's segments are
//! split buttons, and the web trail drew AzDrive's path bar as text typed into
//! a white box ("a poor text field"), so the bar builds its own segments.
//!
//! The bar owns nothing: the app keeps the history, the path and the search
//! text, hears every action through ONE callback ([`AddressBar::on_event`],
//! an [`AddressBarEvent`] naming what happened) and rebuilds. The parts are
//! the toolkit's own widgets: [`crate::widgets::button::Button`]s dressed in
//! the bar's look for the arrows, the segments and Refresh, and
//! [`TextInput`]s for the editable path and the search box.
//!
//! Key types: [`AddressBar`], [`AddressBarEvent`], [`AddressBarEventKind`],
//! [`AddressBarOnEvent`].

use alloc::{format, vec::Vec};

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, FocusTarget, Update},
    dom::{
        ComponentEventFilter, Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass,
        IdOrClass::Class, IdOrClassVec,
    },
    menu::{Menu, MenuItem, MenuItemVec, MenuPopupPosition, StringMenuItem},
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::length::PercentageValue,
        layout::{
            LayoutAlignItems, LayoutBoxSizing, LayoutFlexDirection, LayoutJustifyContent,
            LayoutMinWidth,
        },
        property::CssProperty,
        style::{effects::StyleOpacity, StyleCursor, StyleUserSelect},
    },
    AzString, StringVec,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::{
        button::{styled_button, ButtonOnClick, ButtonOnClickCallbackType, OptionButtonOnClick},
        text_input::{
            OnTextInputReturn, TextInput, TextInputOnFocusLostCallbackType,
            TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallbackType,
            TextInputState, TextInputValid,
        },
        themes::{
            decl::{
                display_flex, flex_direction, grow, no_shrink, nowrap, on_base, overflow_x_hidden,
                simple,
            },
            OptionUiTheme, UiTheme,
        },
    },
};

/// A part's declarations, under the short name the base lists use.
type Cond = CssPropertyWithConditions;

static BAR_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-address-bar"))];
static NAV_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-address-bar-nav",
))];
static BOX_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-address-bar-box",
))];
static ICON_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-address-bar-icon",
))];
static FIELD_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-address-bar-field",
))];
static EDIT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-address-bar-edit",
))];
static SEARCH_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-address-bar-search",
))];

/// Added to Back's and Forward's boxes: the round arrows.
pub const ROUND_CLASS: &str = "__azul-native-address-bar-round";
/// Added to the "Recent locations" chevron's box.
pub const RECENT_CLASS: &str = "__azul-native-address-bar-recent";
/// A segment of the trail: a folder of the path, a button that goes there.
pub const CRUMB_CLASS: &str = "__azul-native-address-bar-crumb";
/// Added to the last segment: the folder the window shows.
pub const CURRENT_CLASS: &str = "__azul-native-address-bar-current";
/// The chevron after a segment: a button that drops the folder's subfolders.
pub const CHEVRON_CLASS: &str = "__azul-native-address-bar-chevron";
/// The « before the trail of a path too long for the box: a menu of the
/// segments folded into it.
pub const OVERFLOW_CLASS: &str = "__azul-native-address-bar-overflow";
/// Refresh, at the breadcrumb box's end.
pub const REFRESH_CLASS: &str = "__azul-native-address-bar-refresh";
/// Added to the box of an arrow that cannot go: dimmed, announced unavailable.
pub const NAV_DISABLED_CLASS: &str = "__azul-native-address-bar-nav-disabled";

/// An arrow that cannot go is dimmed (the same in every theme).
pub(crate) static ADDRESS_BAR_NAV_DISABLED_BASE: &[CssPropertyWithConditions] =
    &[CssPropertyWithConditions::simple(CssProperty::const_opacity(
        StyleOpacity {
            inner: PercentageValue::const_new(40),
        },
    ))];

/// What happened on the bar.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AddressBarEventKind {
    /// The Back arrow.
    Back,
    /// The Forward arrow.
    Forward,
    /// The Up arrow.
    Up,
    /// The Refresh button.
    Refresh,
    /// A segment of the trail (`index`, in the whole trail - a folded one
    /// picked from the « menu too): go there.
    Crumb,
    /// The chevron after a segment (`index`): the app opens a menu of that
    /// folder's subfolders, at the hit node
    /// (`CallbackInfo::open_menu_for_hit_node`).
    CrumbMenu,
    /// A click on the breadcrumb box's empty part: the app rebuilds with
    /// `editing` set and the path in `path`.
    EditStarted,
    /// Enter in the path field: `text` is the path typed.
    PathEntered,
    /// Escape in the path field, or it lost focus: the app rebuilds with
    /// `editing` unset.
    EditCancelled,
    /// The search box changed: `text` is its text.
    Search,
    /// The "Recent locations" chevron right of Forward (`with_recent`): the
    /// app opens a menu of the places visited, at the hit node.
    Recent,
}

/// One action on the bar: what, which segment, what text.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressBarEvent {
    /// The path typed (`PathEntered`) or the search text (`Search`); empty
    /// otherwise.
    pub text: AzString,
    /// The segment (`Crumb`, `CrumbMenu`); 0 otherwise.
    pub index: usize,
    /// What happened.
    pub kind: AddressBarEventKind,
}

/// Callback invoked for every action on the bar.
pub type AddressBarOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, AddressBarEvent) -> Update;
impl_widget_callback!(
    AddressBarOnEvent,
    OptionAddressBarOnEvent,
    AddressBarOnEventCallback,
    AddressBarOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        AddressBarOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: ADDRESS_BAR_ON_EVENT_INVOKER,
    invoker_ty:     AzAddressBarOnEventCallbackInvoker,
    thunk_fn:       az_address_bar_on_event_callback_thunk,
    setter_fn:      AzApp_setAddressBarOnEventCallbackInvoker,
    from_handle_fn: AzAddressBarOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzAddressBarOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: AddressBarEvent ],
}

/// A file manager's address bar: navigation arrows, the path as a trail of
/// segments or a field, Refresh and a search box.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct AddressBar {
    /// The trail, the drive first, the open folder last.
    pub crumbs: StringVec,
    /// The path shown in the field while `editing`.
    pub path: AzString,
    /// The search box's text.
    pub search: AzString,
    /// The search box's prompt when it is empty ("Search Documents").
    pub search_placeholder: AzString,
    /// Hears every action on the bar.
    pub on_event: OptionAddressBarOnEvent,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
    /// Whether Back does anything (a history behind); an arrow that cannot
    /// go reports nothing.
    pub can_go_back: bool,
    /// Whether Forward does anything.
    pub can_go_forward: bool,
    /// Whether Up does anything (not at a drive's root).
    pub can_go_up: bool,
    /// Show the path as an editable field instead of the trail.
    pub editing: bool,
    /// Show Explorer's "Recent locations" chevron right of Forward, which
    /// reports [`AddressBarEventKind::Recent`].
    pub show_recent: bool,
    /// The location's icon at the start of the breadcrumb box, a Material
    /// icon name ("folder", "computer", "cloud"); empty draws none.
    pub icon: AzString,
    /// How wide the bar is, in px: what the trail's segments may take is
    /// estimated from it, and the leading ones that do not fit fold into the
    /// « menu. 0 (the default) never folds.
    pub available_width: f32,
}

/// What a theme decides about an address bar: the SKIN of each part, laid
/// over the part's base (the bar's structure, the same in every theme:
/// `ADDRESS_BAR_*_BASE`) by [`build`].
pub(crate) struct AddressBarLook {
    /// The theme the bar's buttons and inputs are built in.
    pub theme: UiTheme,
    /// The bar.
    pub bar: Vec<Cond>,
    /// The box around each arrow (its spacing).
    pub nav: Vec<Cond>,
    /// Back and Forward: the round arrows.
    pub round: Vec<Cond>,
    /// Up: a flat arrow button.
    pub arrow: Vec<Cond>,
    /// The "Recent locations" chevron: a narrow flat button.
    pub recent: Vec<Cond>,
    /// An arrow's glyph.
    pub arrow_icon: Vec<Cond>,
    /// The breadcrumb box holding the trail.
    pub field_box: Vec<Cond>,
    /// The breadcrumb box holding the path field, which draws its own
    /// frame: the box keeps its place, without one.
    pub field_box_editing: Vec<Cond>,
    /// The location's icon.
    pub icon: Vec<Cond>,
    /// The row of segments (or the path field) inside the box.
    pub field: Vec<Cond>,
    /// The path field's wrapper.
    pub edit: Vec<Cond>,
    /// A segment.
    pub crumb: Vec<Cond>,
    /// Stacked on the current folder's segment.
    pub current: Vec<Cond>,
    /// A segment's label (the « too).
    pub label: Vec<Cond>,
    /// A chevron after a segment.
    pub chevron: Vec<Cond>,
    /// A chevron's glyph.
    pub chevron_icon: Vec<Cond>,
    /// The « of a folded trail.
    pub overflow: Vec<Cond>,
    /// Refresh at the box's end.
    pub refresh: Vec<Cond>,
    /// Refresh's glyph.
    pub refresh_icon: Vec<Cond>,
    /// The box around the search input.
    pub search: Vec<Cond>,
    /// The theme's marker class on the bar, if it has one.
    pub marker: Option<&'static str>,
}

// ---- the base: the bar's structure, in every theme ----

/// The bar: one row, its parts centred on the midline.
pub(crate) static ADDRESS_BAR_BASE: &[Cond] = &[
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    grow(0),
];

/// An arrow's box keeps its size.
pub(crate) static ADDRESS_BAR_NAV_BASE: &[Cond] = &[display_flex(), grow(0), no_shrink()];

/// A button of the bar - an arrow, a segment, a chevron, the «, Refresh: its
/// glyph or label centred on the midline, its size its own (a segment that
/// does not fit is folded, never squeezed), the arrow pointer of a toolbar
/// button, nothing to select, one line.
pub(crate) static ADDRESS_BAR_BUTTON_BASE: &[Cond] = &[
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_justify_content(LayoutJustifyContent::Center)),
    grow(0),
    no_shrink(),
    simple(CssProperty::const_cursor(StyleCursor::Default)),
    simple(CssProperty::user_select(StyleUserSelect::None)),
    nowrap(),
];

/// A button's glyph keeps its size and is never selected.
pub(crate) static ADDRESS_BAR_GLYPH_BASE: &[Cond] = &[
    grow(0),
    no_shrink(),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// A segment's label: one line, its own width.
pub(crate) static ADDRESS_BAR_LABEL_BASE: &[Cond] = &[
    grow(0),
    nowrap(),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The breadcrumb box takes the rest of the bar; its parts sit on the
/// midline, a drag across it never selects, and over its empty part the
/// pointer is the text cursor - a click there types the path.
pub(crate) static ADDRESS_BAR_BOX_BASE: &[Cond] = &[
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    grow(1),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    simple(CssProperty::const_cursor(StyleCursor::Text)),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The location's icon keeps its size.
pub(crate) static ADDRESS_BAR_ICON_BASE: &[Cond] = &[
    display_flex(),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    grow(0),
    no_shrink(),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The segments' row (or the path field) takes the box's room; what still
/// does not fit there is clipped, on one line.
pub(crate) static ADDRESS_BAR_FIELD_BASE: &[Cond] = &[
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    grow(1),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    overflow_x_hidden(),
    nowrap(),
];

/// The path field's wrapper takes the row (the input in it grows to fill it).
pub(crate) static ADDRESS_BAR_EDIT_BASE: &[Cond] = &[
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    grow(1),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// The search box keeps its width.
pub(crate) static ADDRESS_BAR_SEARCH_BASE: &[Cond] = &[display_flex(), grow(0), no_shrink()];

// ---- folding a long trail ----
//
// No text is measured while a DOM is built, so how wide a segment is comes
// from its label's length at the bar's 13 px - an estimate that errs wide, so
// a trail folds a segment early rather than clipping its end.

/// A label's character, in px.
const CHAR_PX: f32 = 7.0;
/// A segment's padding and rim.
const CRUMB_PAD_PX: f32 = 12.0;
/// The chevron after a segment.
const CHEVRON_PX: f32 = 16.0;
/// The « button.
const OVERFLOW_PX: f32 = 22.0;

/// How wide the segment of `label` is, with its chevron.
fn crumb_px(label: &str) -> f32 {
    label.chars().count() as f32 * CHAR_PX + CRUMB_PAD_PX + CHEVRON_PX
}

/// What the trail may take in a bar `available_width` px wide: the bar less
/// Back, Forward and Up (28 px with their gaps), the Recent chevron (18), the
/// box's frame and Refresh (36), the location's icon (24), the search box
/// and its gap (228) and the bar's padding (16). 0 when nothing is known.
fn trail_room(available_width: f32, show_recent: bool, has_icon: bool) -> f32 {
    if available_width <= 0.0 || available_width.is_nan() {
        return 0.0;
    }
    let mut fixed = 3.0 * 28.0 + 36.0 + 228.0 + 16.0;
    if show_recent {
        fixed += 18.0;
    }
    if has_icon {
        fixed += 24.0;
    }
    (available_width - fixed).max(1.0)
}

/// How many LEADING segments of `labels` fold into the « menu so the rest
/// fits `room_px`: none when the whole trail fits (or nothing is known, a
/// room of 0), never the last one - the folder the window shows always has
/// its segment. Explorer folds from the drive end, the same way.
#[must_use]
pub(crate) fn folded_crumbs(labels: &[&str], room_px: f32) -> usize {
    if room_px <= 0.0 || room_px.is_nan() || labels.len() < 2 {
        return 0;
    }
    let widths: Vec<f32> = labels.iter().map(|label| crumb_px(label)).collect();
    let mut used: f32 = widths.iter().sum();
    if used <= room_px {
        return 0;
    }
    used += OVERFLOW_PX;
    let mut folded = 0;
    while folded + 1 < labels.len() && used > room_px {
        used -= widths[folded];
        folded += 1;
    }
    folded
}

impl AddressBar {
    /// A bar showing `crumbs`, nothing to go back, forward or up to, no
    /// search text.
    #[must_use]
    pub fn create(crumbs: StringVec) -> Self {
        Self {
            crumbs,
            path: AzString::from_const_str(""),
            search: AzString::from_const_str(""),
            search_placeholder: AzString::from_const_str("Search"),
            on_event: None.into(),
            theme: OptionUiTheme::None,
            can_go_back: false,
            can_go_forward: false,
            can_go_up: false,
            editing: false,
            show_recent: false,
            icon: AzString::from_const_str(""),
            available_width: 0.0,
        }
    }

    /// The path the field shows while editing.
    pub fn set_path(&mut self, path: AzString) {
        self.path = path;
    }

    /// [`Self::set_path`] for the builder chain.
    #[must_use]
    pub fn with_path(mut self, path: AzString) -> Self {
        self.set_path(path);
        self
    }

    /// The search box's text.
    pub fn set_search(&mut self, search: AzString) {
        self.search = search;
    }

    /// [`Self::set_search`] for the builder chain.
    #[must_use]
    pub fn with_search(mut self, search: AzString) -> Self {
        self.set_search(search);
        self
    }

    /// The search box's prompt.
    pub fn set_search_placeholder(&mut self, placeholder: AzString) {
        self.search_placeholder = placeholder;
    }

    /// [`Self::set_search_placeholder`] for the builder chain.
    #[must_use]
    pub fn with_search_placeholder(mut self, placeholder: AzString) -> Self {
        self.set_search_placeholder(placeholder);
        self
    }

    /// Whether Back, Forward and Up do anything.
    pub const fn set_can_go(&mut self, back: bool, forward: bool, up: bool) {
        self.can_go_back = back;
        self.can_go_forward = forward;
        self.can_go_up = up;
    }

    /// [`Self::set_can_go`] for the builder chain.
    #[must_use]
    pub const fn with_can_go(mut self, back: bool, forward: bool, up: bool) -> Self {
        self.set_can_go(back, forward, up);
        self
    }

    /// Show the path field instead of the trail.
    pub const fn set_editing(&mut self, editing: bool) {
        self.editing = editing;
    }

    /// [`Self::set_editing`] for the builder chain.
    #[must_use]
    pub const fn with_editing(mut self, editing: bool) -> Self {
        self.set_editing(editing);
        self
    }

    /// Show (or hide) the "Recent locations" chevron right of Forward.
    pub const fn set_recent(&mut self, show_recent: bool) {
        self.show_recent = show_recent;
    }

    /// [`Self::set_recent`] for the builder chain.
    #[must_use]
    pub const fn with_recent(mut self, show_recent: bool) -> Self {
        self.set_recent(show_recent);
        self
    }

    /// The location's icon at the start of the breadcrumb box (a Material
    /// icon name; empty draws none) - see [`Self::icon`].
    pub fn set_icon(&mut self, icon: AzString) {
        self.icon = icon;
    }

    /// [`Self::set_icon`] for the builder chain.
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.set_icon(icon);
        self
    }

    /// How wide the bar is, in px: the trail's leading segments that do not
    /// fit fold into the overflow menu at its start (0 never folds) - see
    /// [`Self::available_width`].
    pub const fn set_available_width(&mut self, px: f32) {
        self.available_width = px;
    }

    /// [`Self::set_available_width`] for the builder chain.
    #[must_use]
    pub const fn with_available_width(mut self, px: f32) -> Self {
        self.set_available_width(px);
        self
    }

    /// Pin the widget theme; unset, the bar follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// The callback that hears every action on the bar.
    pub fn set_on_event<C: Into<AddressBarOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = Some(AddressBarOnEvent {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<AddressBarOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// Replaces `self` with an empty bar and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(StringVec::from_const_slice(&[]));
        core::mem::swap(&mut s, self);
        s
    }

    /// The bar's DOM. The look comes from the theme module
    /// (`themes::flat::address_bar` / `themes::flora::address_bar`); `None`
    /// carries both looks, each in its `@theme(<name>)` block, and the app
    /// theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::address_bar(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::address_bar(self),
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                self,
                crate::widgets::themes::flat::address_bar,
                crate::widgets::themes::flora::address_bar,
            ),
        }
    }
}

impl Default for AddressBar {
    fn default() -> Self {
        Self::create(StringVec::from_const_slice(&[]))
    }
}

impl From<AddressBar> for Dom {
    fn from(b: AddressBar) -> Self {
        b.dom()
    }
}

/// What every part of one bar shares: the app's callback.
struct AddressBarShared {
    on_event: OptionAddressBarOnEvent,
}

/// What a segment, a chevron or a « menu entry carries: the bar's shared
/// part and the segment's place in the WHOLE trail (the folded ones count
/// too, so the app hears the index of its own crumbs).
struct CrumbData {
    shared: RefAny,
    index: usize,
}

/// What the « carries: the bar's shared part and the segments folded into
/// it, with their places in the trail.
struct OverflowData {
    shared: RefAny,
    hidden: Vec<(usize, AzString)>,
}

/// The shared part and the segment `data` names.
fn crumb_of(data: &mut RefAny) -> Option<(RefAny, usize)> {
    data.downcast_ref::<CrumbData>()
        .map(|c| (c.shared.clone(), c.index))
}

/// Hands `kind` to the app's callback. The hook is taken out of the shared
/// part first: the app's callback runs without the bar's state borrowed.
fn emit(
    shared: &mut RefAny,
    info: CallbackInfo,
    kind: AddressBarEventKind,
    index: usize,
    text: AzString,
) -> Update {
    let hook = match shared.downcast_ref::<AddressBarShared>() {
        Some(s) => s.on_event.clone(),
        None => return Update::DoNothing,
    };
    match hook.into_option() {
        Some(AddressBarOnEvent { callback, refany }) => {
            callback.invoke(refany, info, AddressBarEvent { text, index, kind })
        }
        None => Update::DoNothing,
    }
}

const fn no_text() -> AzString {
    AzString::from_const_str("")
}

extern "C" fn on_back(mut data: RefAny, info: CallbackInfo) -> Update {
    emit(&mut data, info, AddressBarEventKind::Back, 0, no_text())
}

extern "C" fn on_forward(mut data: RefAny, info: CallbackInfo) -> Update {
    emit(&mut data, info, AddressBarEventKind::Forward, 0, no_text())
}

extern "C" fn on_recent(mut data: RefAny, info: CallbackInfo) -> Update {
    emit(&mut data, info, AddressBarEventKind::Recent, 0, no_text())
}

extern "C" fn on_up(mut data: RefAny, info: CallbackInfo) -> Update {
    emit(&mut data, info, AddressBarEventKind::Up, 0, no_text())
}

/// Refresh sits in the breadcrumb box: its click stops there, so the box
/// does not start an edit too.
extern "C" fn on_refresh(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    emit(&mut data, info, AddressBarEventKind::Refresh, 0, no_text())
}

/// A click on the box's empty part (or its icon): the trail turns into the
/// path field.
extern "C" fn on_box_click(mut data: RefAny, info: CallbackInfo) -> Update {
    emit(&mut data, info, AddressBarEventKind::EditStarted, 0, no_text())
}

/// A segment was clicked: go there. The click stops at the segment, so the
/// box under it does not start an edit.
extern "C" fn on_crumb(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    let Some((mut shared, index)) = crumb_of(&mut data) else {
        return Update::DoNothing;
    };
    emit(&mut shared, info, AddressBarEventKind::Crumb, index, no_text())
}

/// A segment's chevron was clicked: the app opens that folder's menu.
extern "C" fn on_chevron(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    let Some((mut shared, index)) = crumb_of(&mut data) else {
        return Update::DoNothing;
    };
    emit(&mut shared, info, AddressBarEventKind::CrumbMenu, index, no_text())
}

/// The menu of the segments folded into the «: one entry each, in the
/// trail's order, each reporting its segment as a click on it would.
fn overflow_menu(shared: &RefAny, hidden: &[(usize, AzString)]) -> Menu {
    let entries: Vec<MenuItem> = hidden
        .iter()
        .map(|(index, label)| {
            MenuItem::String(StringMenuItem::create(label.clone()).with_callback(
                RefAny::new(CrumbData {
                    shared: shared.clone(),
                    index: *index,
                }),
                on_overflow_pick as usize,
            ))
        })
        .collect();
    Menu::create(MenuItemVec::from_vec(entries)).with_position(MenuPopupPosition::BottomOfHitRect)
}

/// The « was clicked: the widget opens the menu of the hidden segments
/// itself (it knows them; the app hears the pick as a `Crumb`), under the «
/// - or, without its rect (a headless test, not laid out yet), where the
/// menu places itself.
extern "C" fn on_overflow(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    let Some((shared, hidden)) = data
        .downcast_ref::<OverflowData>()
        .map(|o| (o.shared.clone(), o.hidden.clone()))
    else {
        return Update::DoNothing;
    };
    if hidden.is_empty() {
        return Update::DoNothing;
    }
    let menu = overflow_menu(&shared, &hidden);
    if !info.open_menu_for_hit_node(menu.clone()) {
        info.open_menu(menu);
    }
    Update::DoNothing
}

/// A hidden segment picked from the « menu: go there.
extern "C" fn on_overflow_pick(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((mut shared, index)) = crumb_of(&mut data) else {
        return Update::DoNothing;
    };
    emit(&mut shared, info, AddressBarEventKind::Crumb, index, no_text())
}

/// The path field appeared: it takes the keyboard (the wrapper hands the
/// focus to its first focusable node, the input). Without it neither Escape
/// nor a click elsewhere could end the edit: both need the field focused.
extern "C" fn on_path_mounted(_data: RefAny, mut info: CallbackInfo) -> Update {
    let node = info.get_hit_node();
    info.set_focus(FocusTarget::Id(node));
    Update::DoNothing
}

/// What a key in the path field asks for: Enter takes the path, Escape
/// gives the edit up, every other key is the field's own.
#[must_use]
pub(crate) const fn path_key_event(key: Option<VirtualKeyCode>) -> Option<AddressBarEventKind> {
    match key {
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
            Some(AddressBarEventKind::PathEntered)
        }
        Some(VirtualKeyCode::Escape) => Some(AddressBarEventKind::EditCancelled),
        _ => None,
    }
}

extern "C" fn on_path_key(
    mut data: RefAny,
    info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let update = match path_key_event(key) {
        Some(kind) => emit(&mut data, info, kind, 0, AzString::from(state.get_text())),
        None => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// The path field lost focus: the edit is given up.
extern "C" fn on_path_focus_lost(
    mut data: RefAny,
    info: CallbackInfo,
    _state: TextInputState,
) -> Update {
    emit(&mut data, info, AddressBarEventKind::EditCancelled, 0, no_text())
}

/// The search box changed.
extern "C" fn on_search_text(
    mut data: RefAny,
    info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let update = emit(
        &mut data,
        info,
        AddressBarEventKind::Search,
        0,
        AzString::from(state.get_text()),
    );
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// Marks a built button unavailable for assistive technology.
fn mark_unavailable(button: &mut Dom) {
    let mut a11y = button
        .root
        .get_accessibility_info()
        .cloned()
        .unwrap_or_default();
    let mut states = a11y.states.clone().into_library_owned_vec();
    states.push(azul_core::a11y::AccessibilityState::Unavailable);
    a11y.states = azul_core::a11y::AccessibilityStateVec::from_vec(states);
    button.root.set_accessibility_info(a11y);
}

/// The bar's DOM in `look`:
///
/// ```text
/// bar        Back, Forward (round), [Recent], Up, box, search
///  box       [icon] field refresh            click on its empty part -> EditStarted
///   field    [«] segment chevron ... segment chevron     (or: edit > path input)
/// ```
///
/// Every part is its base (the structure), then the look's skin; the
/// buttons and inputs are the toolkit's own widgets, built in the look's
/// theme (a bar that follows the app theme is built once per theme and the
/// two merged, `themes::theme_blocks`).
pub(crate) fn build(bar: AddressBar, look: &AddressBarLook) -> Dom {
    let part = |base: &[Cond], skin: &[Cond]| CssPropertyWithConditionsVec::from_vec(on_base(base, skin));
    let AddressBar {
        crumbs,
        path,
        search,
        search_placeholder,
        on_event,
        // The look to build is `look.theme`: the field is the caller's pin,
        // already resolved by `dom()`.
        theme: _,
        can_go_back,
        can_go_forward,
        can_go_up,
        editing,
        show_recent,
        icon,
        available_width,
    } = bar;
    let theme = OptionUiTheme::Some(look.theme);
    let shared = RefAny::new(AddressBarShared { on_event });

    // A button of the bar in `face`: `glyph` an icon's style, `label` a
    // segment's name, `name` what assistive technology calls an icon-only
    // one. The bar's look owns the whole face - rest, hover, pressed, focus -
    // so the Button theme's own faces are not used.
    let button = |icon: AzString,
                  label: AzString,
                  face: &[Cond],
                  glyph: &[Cond],
                  on_click: OptionButtonOnClick,
                  name: AzString| {
        styled_button(
            icon,
            label,
            no_text(),
            part(ADDRESS_BAR_BUTTON_BASE, face),
            part(ADDRESS_BAR_GLYPH_BASE, glyph),
            part(ADDRESS_BAR_LABEL_BASE, &look.label),
            part(ADDRESS_BAR_GLYPH_BASE, glyph),
            on_click,
            no_text(),
            name,
            theme,
        )
        .dom()
    };
    let click = |data: RefAny, cb: ButtonOnClickCallbackType| {
        OptionButtonOnClick::Some(ButtonOnClick::create(data, cb))
    };

    // An arrow in its box. One that cannot go has no click, is dimmed and is
    // announced unavailable.
    let nav = |glyph: &'static str,
               name: &'static str,
               enabled: bool,
               face: &[Cond],
               kind: Option<&'static str>,
               on_click: ButtonOnClickCallbackType| {
        let mut arrow = button(
            AzString::from_const_str(glyph),
            no_text(),
            face,
            &look.arrow_icon,
            if enabled {
                click(shared.clone(), on_click)
            } else {
                OptionButtonOnClick::None
            },
            AzString::from_const_str(name),
        );
        let mut classes: Vec<IdOrClass> = NAV_CLASS.to_vec();
        if let Some(kind) = kind {
            classes.push(Class(AzString::from_const_str(kind)));
        }
        let mut style = part(ADDRESS_BAR_NAV_BASE, &look.nav);
        if !enabled {
            classes.push(Class(AzString::from_const_str(NAV_DISABLED_CLASS)));
            style = crate::widgets::themes::theme_blocks::stack_parts(
                &style,
                &CssPropertyWithConditionsVec::from_const_slice(ADDRESS_BAR_NAV_DISABLED_BASE),
            );
            mark_unavailable(&mut arrow);
        }
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(classes))
            .with_css_props(style)
            .with_child(arrow)
    };

    // ---- the field: the trail of segments, or the path field ----
    let has_icon = !icon.as_str().is_empty();
    let field_children = if editing {
        let on_key: TextInputOnVirtualKeyDownCallbackType = on_path_key;
        let on_focus_lost: TextInputOnFocusLostCallbackType = on_path_focus_lost;
        let input = TextInput::create()
            .with_text(path)
            .with_accessibility_name(AzString::from_const_str("Address"))
            .with_on_virtual_key_down(shared.clone(), on_key)
            .with_on_focus_lost(shared.clone(), on_focus_lost)
            .with_theme(look.theme);
        // A node of its own around the input, so its mount is the field's
        // opening (the trail's row is the same node in both modes).
        alloc::vec![Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(EDIT_CLASS))
            .with_css_props(part(ADDRESS_BAR_EDIT_BASE, &look.edit))
            .with_callbacks(
                alloc::vec![CoreCallbackData {
                    event: EventFilter::Component(ComponentEventFilter::AfterMount),
                    callback: CoreCallback {
                        cb: on_path_mounted as usize,
                        ctx: OptionRefAny::None,
                    },
                    refany: shared.clone(),
                }]
                .into(),
            )
            .with_child(input.dom())]
    } else {
        let labels: &[AzString] = crumbs.as_ref();
        let folded = {
            let names: Vec<&str> = labels.iter().map(AzString::as_str).collect();
            folded_crumbs(&names, trail_room(available_width, show_recent, has_icon))
        };
        let mut trail: Vec<Dom> = Vec::with_capacity(2 * labels.len() + 1);
        if folded > 0 {
            let hidden: Vec<(usize, AzString)> =
                labels[..folded].iter().cloned().enumerate().collect();
            let mut overflow = button(
                no_text(),
                AzString::from_const_str("\u{00AB}"),
                &look.overflow,
                &look.chevron_icon,
                click(
                    RefAny::new(OverflowData {
                        shared: shared.clone(),
                        hidden,
                    }),
                    on_overflow,
                ),
                AzString::from_const_str("Hidden locations"),
            );
            overflow.root.add_class(AzString::from_const_str(OVERFLOW_CLASS));
            trail.push(overflow);
        }
        let last = labels.len().saturating_sub(1);
        for (index, label) in labels.iter().enumerate().skip(folded) {
            let mut face = look.crumb.clone();
            if index == last {
                face.extend(look.current.iter().cloned());
            }
            let mut crumb = button(
                no_text(),
                label.clone(),
                &face,
                &look.chevron_icon,
                click(
                    RefAny::new(CrumbData {
                        shared: shared.clone(),
                        index,
                    }),
                    on_crumb,
                ),
                no_text(),
            );
            crumb.root.add_class(AzString::from_const_str(CRUMB_CLASS));
            if index == last {
                crumb.root.add_class(AzString::from_const_str(CURRENT_CLASS));
            }
            trail.push(crumb);
            let mut chevron = button(
                AzString::from_const_str("chevron_right"),
                no_text(),
                &look.chevron,
                &look.chevron_icon,
                click(
                    RefAny::new(CrumbData {
                        shared: shared.clone(),
                        index,
                    }),
                    on_chevron,
                ),
                AzString::from(format!("{} menu", label.as_str())),
            );
            chevron.root.add_class(AzString::from_const_str(CHEVRON_CLASS));
            trail.push(chevron);
        }
        trail
    };
    let field = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(FIELD_CLASS))
        .with_css_props(part(ADDRESS_BAR_FIELD_BASE, &look.field))
        .with_children(DomVec::from_vec(field_children));

    // ---- the breadcrumb box: [icon] field refresh ----
    let mut box_children: Vec<Dom> = Vec::with_capacity(3);
    if has_icon {
        box_children.push(
            Dom::create_icon(icon)
                .with_ids_and_classes(IdOrClassVec::from_const_slice(ICON_CLASS))
                .with_css_props(part(ADDRESS_BAR_ICON_BASE, &look.icon)),
        );
    }
    box_children.push(field);
    let mut refresh = button(
        AzString::from_const_str("refresh"),
        no_text(),
        &look.refresh,
        &look.refresh_icon,
        click(shared.clone(), on_refresh),
        AzString::from_const_str("Refresh"),
    );
    refresh.root.add_class(AzString::from_const_str(REFRESH_CLASS));
    box_children.push(refresh);
    let mut field_box = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(BOX_CLASS))
        .with_css_props(part(
            ADDRESS_BAR_BOX_BASE,
            if editing {
                &look.field_box_editing
            } else {
                &look.field_box
            },
        ))
        .with_children(DomVec::from_vec(box_children));
    if !editing {
        // While the path field shows, the input takes the clicks.
        field_box = field_box.with_callbacks(
            alloc::vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::Click),
                callback: CoreCallback {
                    cb: on_box_click as usize,
                    ctx: OptionRefAny::None,
                },
                refany: shared.clone(),
            }]
            .into(),
        );
    }

    // ---- the search box ----
    let on_search: TextInputOnTextInputCallbackType = on_search_text;
    let search_input = TextInput::create_search()
        .with_text(search)
        .with_placeholder(search_placeholder)
        .with_on_text_input(shared.clone(), on_search)
        .with_theme(look.theme);
    let search_box = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(SEARCH_CLASS))
        .with_css_props(part(ADDRESS_BAR_SEARCH_BASE, &look.search))
        .with_child(search_input.dom());

    let mut parts = alloc::vec![
        nav(
            "arrow_back",
            "Back",
            can_go_back,
            &look.round,
            Some(ROUND_CLASS),
            on_back,
        ),
        nav(
            "arrow_forward",
            "Forward",
            can_go_forward,
            &look.round,
            Some(ROUND_CLASS),
            on_forward,
        ),
    ];
    if show_recent {
        parts.push(nav(
            "expand_more",
            "Recent locations",
            true,
            &look.recent,
            Some(RECENT_CLASS),
            on_recent,
        ));
    }
    parts.push(nav("arrow_upward", "Up", can_go_up, &look.arrow, None, on_up));
    parts.push(field_box);
    parts.push(search_box);

    let mut classes: Vec<IdOrClass> = BAR_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(ADDRESS_BAR_BASE, &look.bar))
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Toolbar,
            ..Default::default()
        })
        .with_children(DomVec::from_vec(parts))
}

#[cfg(test)]
mod address_bar_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::{
        callbacks::CallbackChange,
        widgets::{
            roving::test_support as rv,
            themes::{theme_blocks::checks, theme_checks, UiTheme},
        },
    };

    type Log = Arc<Mutex<Vec<(AddressBarEventKind, usize, String)>>>;

    extern "C" fn record(mut data: RefAny, _info: CallbackInfo, event: AddressBarEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push((
                event.kind,
                event.index,
                event.text.as_str().to_string(),
            ));
        }
        Update::RefreshDom
    }

    fn events(log: &Log) -> Vec<(AddressBarEventKind, usize, String)> {
        log.lock().expect("log").clone()
    }

    fn labels(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
    }

    fn bar(log: &Log) -> AddressBar {
        AddressBar::create(labels(&["This PC", "Home", "Documents"]))
            .with_path(AzString::from("/Users/me/Documents"))
            .with_search_placeholder(AzString::from("Search Documents"))
            .with_can_go(true, false, true)
            .with_on_event(RefAny::new(log.clone()), record as AddressBarOnEventCallbackType)
    }

    /// A trail too long for a 520 px bar.
    const LONG: [&str; 8] = [
        "This PC", "Home", "Documents", "Projects", "azul", "layout", "src", "widgets",
    ];

    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
    }

    /// Whether any node of the subtree carries `class`.
    fn subtree_has_class(node: &Dom, class: &str) -> bool {
        has_class(node, class) || node.children.as_ref().iter().any(|c| subtree_has_class(c, class))
    }

    /// Every text of the subtree, in document order.
    fn texts(node: &Dom, out: &mut Vec<String>) {
        if let NodeType::Text(s) = node.root.get_node_type() {
            out.push(s.as_ref().as_str().to_string());
        }
        for c in node.children.as_ref() {
            texts(c, out);
        }
    }

    fn children(styled: &StyledDom, parent: NodeId) -> Vec<NodeId> {
        let hierarchy = styled.node_hierarchy.as_ref();
        let mut out = Vec::new();
        let mut cur = hierarchy[parent.index()].first_child_id(parent);
        while let Some(n) = cur {
            out.push(n);
            cur = hierarchy[n.index()].next_sibling_id();
        }
        out
    }

    fn id(n: NodeId) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(n)),
        }
    }

    /// The node whose direct text child reads `label`.
    fn node_labelled(styled: &StyledDom, label: &str) -> NodeId {
        let hierarchy = styled.node_hierarchy.as_ref();
        for (i, nd) in styled.node_data.as_ref().iter().enumerate() {
            if let NodeType::Text(s) = nd.get_node_type() {
                if s.as_ref().as_str() == label {
                    return hierarchy[i].parent_id().expect("a label sits in its block");
                }
            }
        }
        panic!("no node is labelled {label:?}");
    }

    fn click(
        styled: &StyledDom,
        node: NodeId,
    ) -> Option<(Update, Vec<CallbackChange>)> {
        rv::fire(styled, id(node), EventFilter::Hover(HoverEventFilter::Click))
    }

    fn stopped(changes: &[CallbackChange]) -> bool {
        changes
            .iter()
            .any(|c| matches!(c, CallbackChange::StopPropagation))
    }

    /// The first node carrying `class`.
    fn node_with_class(styled: &StyledDom, class: &str) -> Option<NodeId> {
        nodes_with_class(styled, class).into_iter().next()
    }

    /// Every node carrying `class`, in document order.
    fn nodes_with_class(styled: &StyledDom, class: &str) -> Vec<NodeId> {
        styled
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, nd)| {
                nd.get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, Class(s) if s.as_str() == class))
            })
            .map(|(i, _)| NodeId::new(i))
            .collect()
    }

    /// `root` and every node under it, in document order.
    fn subtree(styled: &StyledDom, root: NodeId) -> Vec<NodeId> {
        let mut out = vec![root];
        for child in children(styled, root) {
            out.extend(subtree(styled, child));
        }
        out
    }

    /// Whether `node` registered a handler for `event`.
    fn has_callback(styled: &StyledDom, node: NodeId, event: EventFilter) -> bool {
        styled.node_data.as_ref()[node.index()]
            .get_callbacks()
            .as_ref()
            .iter()
            .any(|cb| cb.event == event)
    }

    /// The node a click on `node` reaches first: `node` itself or its
    /// nearest ancestor with a click handler (the test runner fires one
    /// node's handler, it does not bubble).
    fn clickable_from(styled: &StyledDom, node: NodeId) -> Option<NodeId> {
        let click = EventFilter::Hover(HoverEventFilter::Click);
        let hierarchy = styled.node_hierarchy.as_ref();
        let mut at = Some(node);
        while let Some(n) = at {
            if has_callback(styled, n, click) {
                return Some(n);
            }
            at = hierarchy[n.index()].parent_id();
        }
        None
    }

    #[test]
    fn the_bar_is_back_forward_up_the_breadcrumb_box_and_search() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            let dom = bar(&log).with_theme(theme).dom();
            let parts = dom.children.as_ref();
            assert_eq!(parts.len(), 5, "{}", theme.name());
            for i in [0, 1, 2] {
                assert!(
                    has_class(&parts[i], "__azul-native-address-bar-nav"),
                    "{}: part {i} is a navigation button",
                    theme.name()
                );
            }
            assert!(has_class(&parts[0], ROUND_CLASS), "{}: Back is round", theme.name());
            assert!(has_class(&parts[1], ROUND_CLASS), "{}: Forward is round", theme.name());
            assert!(!has_class(&parts[2], ROUND_CLASS), "{}: Up is flat", theme.name());
            let field_box = &parts[3];
            assert!(has_class(field_box, "__azul-native-address-bar-box"));
            assert!(
                subtree_has_class(field_box, CRUMB_CLASS),
                "{}: the box holds the segments",
                theme.name()
            );
            let inside = field_box.children.as_ref();
            assert!(
                has_class(&inside[0], "__azul-native-address-bar-field"),
                "{}: without an icon the box starts with the trail",
                theme.name()
            );
            assert!(
                has_class(inside.last().expect("the box has parts"), REFRESH_CLASS),
                "{}: Refresh is at the box's end",
                theme.name()
            );
            assert!(has_class(&parts[4], "__azul-native-address-bar-search"));
            assert_eq!(parts[4].children.as_ref().len(), 1, "the search input");
            assert_eq!(
                dom.root.get_accessibility_info().map(|i| i.role),
                Some(azul_core::a11y::AccessibilityRole::Toolbar)
            );
        }
    }

    /// The trail is Explorer's segments, not the web's links: a segment and
    /// a chevron per folder, the last one marked current - no slash, no
    /// breadcrumb links.
    #[test]
    fn the_trail_is_a_segment_and_a_chevron_per_folder() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            let dom = bar(&log).with_theme(theme).dom();
            let field = &dom.children.as_ref()[3].children.as_ref()[0];
            let kids = field.children.as_ref();
            assert_eq!(kids.len(), 6, "{}: three segments, three chevrons", theme.name());
            for (i, kid) in kids.iter().enumerate() {
                let class = if i % 2 == 0 { CRUMB_CLASS } else { CHEVRON_CLASS };
                assert!(has_class(kid, class), "{}: child {i} is a {class}", theme.name());
            }
            assert!(has_class(&kids[4], CURRENT_CLASS), "{}: the last segment", theme.name());
            assert!(!has_class(&kids[2], CURRENT_CLASS), "{}", theme.name());
            assert!(
                !subtree_has_class(&dom, "__azul-native-breadcrumb"),
                "{}: no web breadcrumb in the bar",
                theme.name()
            );
            let mut found = Vec::new();
            texts(field, &mut found);
            assert!(!found.iter().any(|t| t.trim() == "/"), "no slashes: {found:?}");
        }
    }

    #[test]
    fn the_crumbs_are_the_trail_and_the_search_box_carries_its_prompt() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = bar(&log).with_theme(UiTheme::Flat).dom();
        let mut found = Vec::new();
        texts(&dom.children.as_ref()[3], &mut found);
        for label in ["This PC", "Home", "Documents"] {
            assert!(found.iter().any(|t| t == label), "{label} in {found:?}");
        }
        fn prompted(node: &Dom, prompt: &str) -> bool {
            node.root.get_placeholder() == Some(prompt)
                || node.children.as_ref().iter().any(|c| prompted(c, prompt))
        }
        assert!(
            prompted(&dom.children.as_ref()[4], "Search Documents"),
            "the search input carries its prompt"
        );
    }

    #[test]
    fn a_click_on_the_box_starts_editing_and_the_editing_bar_shows_a_path_input() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(bar(&log).with_theme(UiTheme::Flat).dom());
        let field_box = children(&styled, NodeId::new(0))[3];
        let (update, changes) = click(&styled, field_box).expect("the box takes the click");
        assert_eq!(events(&log), vec![(AddressBarEventKind::EditStarted, 0, String::new())]);
        assert_eq!(update, Update::RefreshDom, "the app's verdict is forwarded");
        assert!(!stopped(&changes), "the box's own click needs no stop");

        let editing = bar(&log)
            .with_editing(true)
            .with_theme(UiTheme::Flat)
            .dom();
        let field_box = &editing.children.as_ref()[3];
        assert!(
            !subtree_has_class(field_box, CRUMB_CLASS),
            "no segments while editing"
        );
        assert!(
            field_box.root.get_callbacks().as_ref().is_empty(),
            "the input takes the clicks now"
        );
        let field = &field_box.children.as_ref()[0];
        assert!(has_class(field, "__azul-native-address-bar-field"));
        assert_eq!(field.children.as_ref().len(), 1, "the path field's wrapper");
        let edit = &field.children.as_ref()[0];
        assert!(has_class(edit, "__azul-native-address-bar-edit"));
        assert_eq!(edit.children.as_ref().len(), 1, "the path input");
        assert!(
            has_class(field_box.children.as_ref().last().expect("parts"), REFRESH_CLASS),
            "Refresh stays at the box's end while editing"
        );
    }

    #[test]
    fn back_up_and_refresh_report_and_an_arrow_that_cannot_go_is_inert() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(bar(&log).with_theme(UiTheme::Flat).dom());
        let parts = children(&styled, NodeId::new(0));
        let button = |i: usize| children(&styled, parts[i])[0];
        assert!(click(&styled, button(0)).is_some(), "Back");
        assert!(click(&styled, button(1)).is_none(), "Forward cannot go: no click");
        assert!(click(&styled, button(2)).is_some(), "Up");
        let refresh = node_with_class(&styled, REFRESH_CLASS).expect("Refresh");
        let (_, changes) = click(&styled, refresh).expect("Refresh takes the click");
        assert!(stopped(&changes), "Refresh does not also start an edit");
        assert_eq!(
            events(&log)
                .into_iter()
                .map(|(k, _, _)| k)
                .collect::<Vec<_>>(),
            vec![
                AddressBarEventKind::Back,
                AddressBarEventKind::Up,
                AddressBarEventKind::Refresh
            ]
        );
    }

    /// Explorer's "Recent locations" chevron sits right of Forward: with
    /// `with_recent(true)` the bar grows that button, which reports `Recent`
    /// (the app opens its menu of places); without it the bar is unchanged.
    #[test]
    fn the_recent_locations_chevron_follows_forward_and_reports_recent() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let plain = bar(&log).with_theme(UiTheme::Flat).dom();
        assert_eq!(plain.children.as_ref().len(), 5, "no chevron unless asked");
        assert!(!bar(&log).show_recent);

        let styled = StyledDom::create_from_dom(
            bar(&log)
                .with_recent(true)
                .with_theme(UiTheme::Flat)
                .dom(),
        );
        let parts = children(&styled, NodeId::new(0));
        assert_eq!(parts.len(), 6, "back, forward, recent, up, box, search");
        assert!(has_callback(
            &styled,
            children(&styled, parts[2])[0],
            EventFilter::Hover(HoverEventFilter::Click)
        ));
        assert_eq!(
            node_with_class(&styled, RECENT_CLASS),
            Some(parts[2]),
            "the chevron is the third part"
        );
        let chevron = children(&styled, parts[2])[0];
        assert!(click(&styled, chevron).is_some(), "the chevron takes the click");
        assert_eq!(
            events(&log),
            vec![(AddressBarEventKind::Recent, 0, String::new())]
        );
    }

    /// An arrow that cannot go is not only inert: it LOOKS unavailable
    /// (dimmed) and is announced so, like Explorer's greyed Back arrow.
    #[test]
    fn an_arrow_that_cannot_go_is_dimmed_and_announced_unavailable() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            // Back and Up can go, Forward cannot.
            let dom = bar(&log).with_theme(theme).dom();
            let parts = dom.children.as_ref();
            let dimmed = |node: &Dom| {
                node.root
                    .style
                    .iter_inline_properties()
                    .any(|(p, _)| match p {
                        CssProperty::Opacity(o) => {
                            o.get_property().is_some_and(|o| o.inner.normalized() < 0.75)
                        }
                        _ => false,
                    })
            };
            let unavailable = |node: &Dom| {
                node.children.as_ref()[0]
                    .root
                    .get_accessibility_info()
                    .is_some_and(|a| {
                        a.states
                            .as_ref()
                            .contains(&azul_core::a11y::AccessibilityState::Unavailable)
                    })
            };
            assert!(has_class(&parts[1], NAV_DISABLED_CLASS), "{}", theme.name());
            assert!(dimmed(&parts[1]), "{}: Forward is dimmed", theme.name());
            assert!(unavailable(&parts[1]), "{}: Forward is unavailable", theme.name());
            for i in [0, 2] {
                assert!(!has_class(&parts[i], NAV_DISABLED_CLASS), "{}: part {i}", theme.name());
                assert!(!dimmed(&parts[i]), "{}: part {i} is not dimmed", theme.name());
                assert!(!unavailable(&parts[i]), "{}: part {i}", theme.name());
            }
        }
    }

    #[test]
    fn a_crumb_click_goes_there_and_its_chevron_opens_the_menu_without_starting_an_edit() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(bar(&log).with_theme(UiTheme::Flat).dom());
        let home = clickable_from(&styled, node_labelled(&styled, "Home"))
            .expect("the segment takes the click");
        let (_, changes) = click(&styled, home).expect("a segment takes the click");
        assert!(stopped(&changes), "the click stops at the segment");
        // The chevron after "Home" is the segment's next sibling.
        let chevron = styled.node_hierarchy.as_ref()[home.index()]
            .next_sibling_id()
            .expect("a chevron follows the segment");
        let (_, changes) = click(&styled, chevron).expect("a chevron takes the click");
        assert!(stopped(&changes), "the click stops at the chevron");
        assert_eq!(
            events(&log),
            vec![
                (AddressBarEventKind::Crumb, 1, String::new()),
                (AddressBarEventKind::CrumbMenu, 1, String::new()),
            ]
        );
    }

    /// Every segment reports its place in the trail, and so does every
    /// chevron - the current folder's too (Explorer's last chevron drops the
    /// open folder's subfolders).
    #[test]
    fn every_crumb_and_every_chevron_report_their_place_in_the_trail() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(bar(&log).with_theme(UiTheme::Flora).dom());
        let crumbs = nodes_with_class(&styled, CRUMB_CLASS);
        let chevrons = nodes_with_class(&styled, CHEVRON_CLASS);
        assert_eq!((crumbs.len(), chevrons.len()), (3, 3));
        for (crumb, chevron) in crumbs.iter().zip(&chevrons) {
            let (_, changes) = click(&styled, *crumb).expect("a segment takes the click");
            assert!(stopped(&changes));
            let (_, changes) = click(&styled, *chevron).expect("a chevron takes the click");
            assert!(stopped(&changes));
        }
        let mut want = Vec::new();
        for i in 0..3 {
            want.push((AddressBarEventKind::Crumb, i, String::new()));
            want.push((AddressBarEventKind::CrumbMenu, i, String::new()));
        }
        assert_eq!(events(&log), want);
    }

    #[test]
    fn with_an_icon_the_box_begins_with_the_locations_icon() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            let dom = bar(&log)
                .with_icon(AzString::from("folder"))
                .with_theme(theme)
                .dom();
            let inside = dom.children.as_ref()[3].children.as_ref();
            assert_eq!(inside.len(), 3, "{}: icon, field, Refresh", theme.name());
            assert!(has_class(&inside[0], "__azul-native-address-bar-icon"), "{}", theme.name());
            assert!(has_class(&inside[1], "__azul-native-address-bar-field"), "{}", theme.name());
        }
        // A click on the icon is a click on the box: it has no handler of
        // its own, so it starts the edit as the empty part does.
        let styled = StyledDom::create_from_dom(
            bar(&log)
                .with_icon(AzString::from("folder"))
                .with_theme(UiTheme::Flat)
                .dom(),
        );
        let icon = node_with_class(&styled, "__azul-native-address-bar-icon").expect("the icon");
        let target = clickable_from(&styled, icon).expect("the click lands somewhere");
        assert_eq!(Some(target), node_with_class(&styled, "__azul-native-address-bar-box"));
    }

    #[test]
    fn the_leading_crumbs_fold_when_the_trail_does_not_fit() {
        let short = ["This PC", "Home", "Documents"];
        // "This PC" 77 px, "Home" 56, "Documents" 91 with their chevrons.
        assert_eq!(folded_crumbs(&short, 1000.0), 0, "room for all of it");
        assert_eq!(folded_crumbs(&short, 224.0), 0, "exactly room for all of it");
        assert_eq!(folded_crumbs(&short, 223.0), 1, "the drive's crumb goes first");
        assert_eq!(folded_crumbs(&short, 150.0), 2);
        assert_eq!(folded_crumbs(&short, 10.0), 2, "never the open folder's own");
        assert_eq!(folded_crumbs(&short, 0.0), 0, "nothing known: nothing folds");
        assert_eq!(folded_crumbs(&short, f32::NAN), 0);
        assert_eq!(folded_crumbs(&["Home"], 1.0), 0, "one crumb never folds");
        assert_eq!(trail_room(0.0, true, true), 0.0, "an unknown width folds nothing");
        assert!(trail_room(400.0, false, false) >= 1.0, "a narrow bar still has a room");
        assert!(trail_room(1200.0, true, true) < trail_room(1200.0, false, false));
    }

    #[test]
    fn a_folded_trail_keeps_its_last_crumbs_and_the_overflow_menu_reaches_the_hidden_ones() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(
            AddressBar::create(labels(&LONG))
                .with_available_width(520.0)
                .with_on_event(RefAny::new(log.clone()), record as AddressBarOnEventCallbackType)
                .with_theme(UiTheme::Flat)
                .dom(),
        );
        let folded = folded_crumbs(&LONG, trail_room(520.0, false, false));
        assert!(folded > 0 && folded < LONG.len(), "the bar folds some: {folded}");
        let field = node_with_class(&styled, "__azul-native-address-bar-field").expect("field");
        let first = children(&styled, field)[0];
        assert_eq!(node_with_class(&styled, OVERFLOW_CLASS), Some(first), "« comes first");
        let crumbs = nodes_with_class(&styled, CRUMB_CLASS);
        assert_eq!(crumbs.len(), LONG.len() - folded, "the rest keep their segments");

        // The first segment shown still reports its index in the WHOLE trail.
        click(&styled, crumbs[0]).expect("the segment takes the click");
        assert_eq!(events(&log), vec![(AddressBarEventKind::Crumb, folded, String::new())]);

        // The « opens the menu of the hidden segments, in order.
        let (_, changes) = click(&styled, first).expect("the « takes the click");
        assert!(stopped(&changes), "the « does not also start an edit");
        let menu = changes
            .iter()
            .find_map(|c| match c {
                CallbackChange::OpenMenu { menu, .. } => Some(menu.clone()),
                _ => None,
            })
            .expect("the « opens a menu");
        let entries: Vec<StringMenuItem> = menu
            .items
            .as_ref()
            .iter()
            .filter_map(|m| match m {
                MenuItem::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        let names: Vec<&str> = entries.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(names, LONG[..folded].to_vec());

        // A pick goes to the hidden segment it names.
        let pick = entries[1].callback.as_ref().expect("an entry runs a callback").clone();
        let carrier = StyledDom::create_from_dom(Dom::create_div().with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            pick.refany.clone(),
            pick.callback.clone(),
        ));
        click(&carrier, NodeId::new(0)).expect("the pick runs");
        assert_eq!(
            events(&log).last(),
            Some(&(AddressBarEventKind::Crumb, 1, String::new()))
        );
    }

    #[test]
    fn enter_in_the_path_field_takes_the_path_and_escape_gives_the_edit_up() {
        assert_eq!(
            path_key_event(Some(VirtualKeyCode::Return)),
            Some(AddressBarEventKind::PathEntered)
        );
        assert_eq!(
            path_key_event(Some(VirtualKeyCode::NumpadEnter)),
            Some(AddressBarEventKind::PathEntered)
        );
        assert_eq!(
            path_key_event(Some(VirtualKeyCode::Escape)),
            Some(AddressBarEventKind::EditCancelled)
        );
        assert_eq!(path_key_event(Some(VirtualKeyCode::A)), None);
        assert_eq!(path_key_event(None), None);
    }

    /// The path field takes the keyboard as it opens. A click on the trail's
    /// empty space swaps the trail for the field; unless the field then
    /// holds the focus, neither Escape nor a click elsewhere (its focus
    /// loss) can ever end the edit, and the bar stays a text field until the
    /// next navigation - the "poor text field" AzDrive showed.
    #[test]
    fn the_path_field_takes_the_keyboard_when_it_opens() {
        use azul_core::dom::ComponentEventFilter;
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(
            bar(&log)
                .with_editing(true)
                .with_theme(UiTheme::Flat)
                .dom(),
        );
        let mount = EventFilter::Component(ComponentEventFilter::AfterMount);
        let field = node_with_class(&styled, "__azul-native-address-bar-field")
            .expect("the bar has a path field");
        let mounted = subtree(&styled, field)
            .into_iter()
            .find(|n| has_callback(&styled, *n, mount))
            .expect("a node of the path field takes the keyboard when it mounts");
        let (_, changes) = rv::fire(&styled, id(mounted), mount).expect("the mount handler runs");
        assert!(
            changes
                .iter()
                .any(|c| matches!(c, CallbackChange::SetFocusTarget { .. })),
            "the mount asks for the keyboard focus: {changes:?}"
        );
    }

    /// Every crumb goes where it names, the current folder's too: Explorer's
    /// last segment is a button like the others. A click on it used to fall
    /// through to the field under it and turn the trail into a text field.
    #[test]
    fn a_click_on_the_current_crumb_goes_there_instead_of_starting_an_edit() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(bar(&log).with_theme(UiTheme::Flat).dom());
        let label = node_labelled(&styled, "Documents");
        let target = clickable_from(&styled, label).expect("something takes the click");
        click(&styled, target).expect("the click runs a handler");
        assert_eq!(
            events(&log),
            vec![(AddressBarEventKind::Crumb, 2, String::new())],
            "the current crumb reports itself, it does not start an edit"
        );
    }

    #[test]
    fn an_address_bar_without_a_theme_follows_the_app_theme() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        checks::assert_follows_the_app_theme(
            "address_bar",
            || bar(&log).dom(),
            |t: UiTheme| bar(&log).with_theme(t).dom(),
        );
        checks::assert_follows_the_app_theme(
            "address_bar (editing)",
            || bar(&log).with_editing(true).dom(),
            |t: UiTheme| bar(&log).with_editing(true).with_theme(t).dom(),
        );
        checks::assert_follows_the_app_theme(
            "address_bar (icon, folded)",
            || {
                AddressBar::create(labels(&LONG))
                    .with_icon(AzString::from("folder"))
                    .with_available_width(520.0)
                    .dom()
            },
            |t: UiTheme| {
                AddressBar::create(labels(&LONG))
                    .with_icon(AzString::from("folder"))
                    .with_available_width(520.0)
                    .with_theme(t)
                    .dom()
            },
        );
    }

    /// `dom` without the text inputs' own trees: built in the bar's look,
    /// those carry the TextInput's structure, which that widget answers for
    /// (the ribbon's structure test leaves its embedded widgets out the same
    /// way).
    fn without_inputs(mut dom: Dom) -> Dom {
        if has_class(&dom, "__azul-native-address-bar-search")
            || has_class(&dom, "__azul-native-address-bar-edit")
        {
            dom.children = DomVec::from_vec(Vec::new());
            return dom;
        }
        let kids: Vec<Dom> = core::mem::replace(&mut dom.children, DomVec::from_vec(Vec::new()))
            .into_library_owned_vec()
            .into_iter()
            .map(without_inputs)
            .collect();
        dom.children = DomVec::from_vec(kids);
        dom
    }

    /// What lays the bar out is the widget's own; the themes only paint it.
    #[test]
    fn the_address_bar_declares_its_structure_once_for_every_theme() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for t in checks::BOTH {
            for editing in [false, true] {
                let dom = checks::under(t, || {
                    bar(&log)
                        .with_editing(editing)
                        .with_recent(true)
                        .with_icon(AzString::from("folder"))
                        .dom()
                });
                theme_checks::assert_structure_is_shared(
                    &format!("address bar (editing: {editing}), built for {}", t.name()),
                    &without_inputs(dom),
                    &[],
                );
            }
        }
    }
}
