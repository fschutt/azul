//! Ribbon file menu widget - Windows 8 File Explorer's File menu: what the
//! ribbon's File tab opens when the app has no full-window backstage. Not
//! Office's backstage ([`crate::widgets::backstage::Backstage`]) but its
//! MINI form: a panel in a transient window anchored under the File tab, over
//! the ribbon, that a press outside or Escape closes.
//!
//! ```text
//! <transient-window>        under the File tab; the app's window hears the pick here
//!  └ menu                   two columns
//!     ├ commands            a large icon, the label, ▸ for a command with sub-commands
//!     │   [rule] command ...
//!     └ side                one panel at a time:
//!         ├ panel 0         the places: their title ("Frequent places"), "1 Downloads [pin]" ...
//!         └ panel k         the k-th command with sub-commands: its title and its sub-commands,
//!                           an icon beside a label over its description
//! ```
//!
//! Pointing at a command with sub-commands shows them in the side column
//! (Windows 8's "Open new window ▸", "Delete history ▸", "Help ▸"); pointing
//! at any other command shows the places again - no app relayout, the panels
//! are shown and hidden in place. A click on a command, a sub-command or a
//! place does NOT call the app inside the popup: it notes the pick and closes
//! the popup, and the app hears it from the `<transient-window>` node in its
//! OWN window once the popup has closed - so "Close" closes the app's window,
//! a new window, a dialog or a thread start where they would from the ribbon
//! (the way `Dialog::close_from` hands its close to the app's window). The
//! pin beside a place tells the app at once and the menu stays open, as
//! Windows 8's does.
//!
//! Every row is the toolkit's [`crate::widgets::button::Button`] dressed in
//! the menu's look: it takes the keyboard (Up / Down walk the commands, Right
//! goes into the side panel, Left comes back, Enter or Space clicks), a
//! disabled one keeps its stop and shows why it cannot run.
//!
//! The ribbon hangs the menu on its application button
//! ([`crate::widgets::ribbon::RibbonAppButton::menu`],
//! [`hang_on_app_button`]): the button's click opens and closes it.
//!
//! Key types: [`RibbonFileMenu`], [`RibbonFileMenuCommand`],
//! [`RibbonFileMenuPlace`], [`RibbonFileMenuEvent`].

use alloc::{format, string::String, vec::Vec};

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::{FocusTarget, Update},
    dom::{
        ComponentEventFilter, Dom, DomNodeId, DomVec, EventFilter, HoverEventFilter, IdOrClass,
        IdOrClass::Class, IdOrClassVec, NodeData, NodeType, OptionDom, TabIndex,
    },
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    transient::{TransientAnchor, TransientDismiss, TransientWindowConfig},
    window::{VirtualKeyCode, WindowBackgroundMaterial},
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, impl_vec_partialeq,
    props::{
        basic::length::PercentageValue,
        layout::{
            LayoutAlignItems, LayoutBoxSizing, LayoutDisplay, LayoutFlexDirection,
            LayoutJustifyContent, LayoutMinWidth, StyleTextOverflow,
        },
        property::{CssProperty, StyleTextOverflowValue},
        style::{effects::StyleOpacity, StyleCursor, StyleUserSelect},
    },
    AzString,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::{
        button::{styled_button, ButtonOnClick, ButtonOnClickCallbackType, OptionButtonOnClick},
        roving::{self, Step},
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

/// The `<transient-window>` the menu shows in (the ribbon finds it among the
/// application button's children by this class).
pub const WINDOW_CLASS: &str = "__azul-native-ribbon-file-menu-window";
/// The panel: the two columns.
pub const MENU_CLASS: &str = "__azul-native-ribbon-file-menu";
/// The left column of commands.
pub const COMMANDS_CLASS: &str = "__azul-native-ribbon-file-menu-commands";
/// A command of the left column.
pub const COMMAND_CLASS: &str = "__azul-native-ribbon-file-menu-command";
/// Added to a command that cannot run now.
pub const COMMAND_DISABLED_CLASS: &str = "__azul-native-ribbon-file-menu-command-disabled";
/// The ▸ of a command with sub-commands.
pub const ARROW_CLASS: &str = "__azul-native-ribbon-file-menu-arrow";
/// A rule between two groups of commands.
pub const RULE_CLASS: &str = "__azul-native-ribbon-file-menu-rule";
/// The right column.
pub const SIDE_CLASS: &str = "__azul-native-ribbon-file-menu-side";
/// A panel of the right column: the places (the first) or a command's
/// sub-commands.
pub const PANEL_CLASS: &str = "__azul-native-ribbon-file-menu-panel";
/// A panel's title.
pub const TITLE_CLASS: &str = "__azul-native-ribbon-file-menu-title";
/// A place's row: its open button and its pin.
pub const PLACE_CLASS: &str = "__azul-native-ribbon-file-menu-place";
/// A place's open button (its number and its label).
pub const OPEN_CLASS: &str = "__azul-native-ribbon-file-menu-open";
/// A place's number, 1 to 9.
pub const NUMBER_CLASS: &str = "__azul-native-ribbon-file-menu-number";
/// A place's pin.
pub const PIN_CLASS: &str = "__azul-native-ribbon-file-menu-pin";
/// Added to the pin of a pinned place.
pub const PINNED_CLASS: &str = "__azul-native-ribbon-file-menu-pinned";
/// A sub-command of a command's panel.
pub const SUB_CLASS: &str = "__azul-native-ribbon-file-menu-sub";

/// How many places carry a number (Windows 8's access keys 1 to 9).
const NUMBERED_PLACES: usize = 9;

// ==== the public types ====

/// One command of the File menu's left column - or, among a command's
/// [`Self::children`], one of its sub-commands in the side column.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct RibbonFileMenuCommand {
    /// A Material icon name ("open_in_new", "terminal", "help"); empty draws
    /// none.
    pub icon: AzString,
    /// What the command says ("Open new window").
    pub label: AzString,
    /// A sub-command's line under its label ("Open a new window of the same
    /// folder"); a top command shows none.
    pub description: AzString,
    /// Why the command cannot run now; empty = it can. A disabled command
    /// is dimmed, runs nothing, keeps its keyboard stop and shows the reason.
    pub disabled_reason: AzString,
    /// The sub-commands: with any, the command shows a ▸ and pointing at it
    /// (or clicking it) shows them in the side column. A sub-command's own
    /// children are ignored.
    pub children: RibbonFileMenuCommandVec,
    /// A rule above this command (Windows 8 groups its commands: Open new
    /// window | the prompts | Delete history | Help | Close).
    pub separator_before: bool,
}

/// A place of the side column's list ("Frequent places").
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct RibbonFileMenuPlace {
    /// What the row says ("Downloads").
    pub label: AzString,
    /// Where it is, for assistive technology ("Home/Downloads"); may be empty.
    pub detail: AzString,
    /// Whether it is pinned: its pin shows lit.
    pub pinned: bool,
}

impl_option!(
    RibbonFileMenuCommand,
    OptionRibbonFileMenuCommand,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    RibbonFileMenuCommand,
    RibbonFileMenuCommandVec,
    RibbonFileMenuCommandVecDestructor,
    RibbonFileMenuCommandVecDestructorType,
    RibbonFileMenuCommandVecSlice,
    OptionRibbonFileMenuCommand
);
impl_vec_clone!(
    RibbonFileMenuCommand,
    RibbonFileMenuCommandVec,
    RibbonFileMenuCommandVecDestructor
);
impl_vec_debug!(RibbonFileMenuCommand, RibbonFileMenuCommandVec);
impl_vec_partialeq!(RibbonFileMenuCommand, RibbonFileMenuCommandVec);
impl_vec_mut!(RibbonFileMenuCommand, RibbonFileMenuCommandVec);

impl_option!(
    RibbonFileMenuPlace,
    OptionRibbonFileMenuPlace,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    RibbonFileMenuPlace,
    RibbonFileMenuPlaceVec,
    RibbonFileMenuPlaceVecDestructor,
    RibbonFileMenuPlaceVecDestructorType,
    RibbonFileMenuPlaceVecSlice,
    OptionRibbonFileMenuPlace
);
impl_vec_clone!(
    RibbonFileMenuPlace,
    RibbonFileMenuPlaceVec,
    RibbonFileMenuPlaceVecDestructor
);
impl_vec_debug!(RibbonFileMenuPlace, RibbonFileMenuPlaceVec);
impl_vec_partialeq!(RibbonFileMenuPlace, RibbonFileMenuPlaceVec);
impl_vec_mut!(RibbonFileMenuPlace, RibbonFileMenuPlaceVec);

/// What was picked in the File menu.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RibbonFileMenuEventKind {
    /// A command without sub-commands (`index`): heard in the app's window
    /// once the menu has closed.
    Command,
    /// Sub-command `sub_index` of command `index`: heard in the app's
    /// window once the menu has closed.
    SubCommand,
    /// Place `index` of the side column: heard in the app's window once the
    /// menu has closed.
    OpenPlace,
    /// The pin of place `index` was clicked: heard at once, the menu stays
    /// open; the app pins or unpins it and rebuilds.
    TogglePin,
}

/// One pick in the File menu: what, and which.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RibbonFileMenuEvent {
    /// What was picked.
    pub kind: RibbonFileMenuEventKind,
    /// The command (`Command`, `SubCommand`) or the place (`OpenPlace`,
    /// `TogglePin`).
    pub index: usize,
    /// The sub-command of command `index` (`SubCommand`); 0 otherwise.
    pub sub_index: usize,
}

/// Callback invoked for every pick in the File menu.
pub type RibbonFileMenuOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, RibbonFileMenuEvent) -> Update;
impl_widget_callback!(
    RibbonFileMenuOnEvent,
    OptionRibbonFileMenuOnEvent,
    RibbonFileMenuOnEventCallback,
    RibbonFileMenuOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        RibbonFileMenuOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: RIBBON_FILE_MENU_ON_EVENT_INVOKER,
    invoker_ty:     AzRibbonFileMenuOnEventCallbackInvoker,
    thunk_fn:       az_ribbon_file_menu_on_event_callback_thunk,
    setter_fn:      AzApp_setRibbonFileMenuOnEventCallbackInvoker,
    from_handle_fn: AzRibbonFileMenuOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzRibbonFileMenuOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: RibbonFileMenuEvent ],
}

/// The ribbon's File menu: the commands of its left column, the places of
/// its side column and the one callback that hears every pick.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct RibbonFileMenu {
    /// The left column, top first.
    pub commands: RibbonFileMenuCommandVec,
    /// The side column's title while it shows the places ("Frequent
    /// places"); empty draws none.
    pub places_title: AzString,
    /// The side column's places, numbered 1 to 9 from the top.
    pub places: RibbonFileMenuPlaceVec,
    /// Hears every pick.
    pub on_event: OptionRibbonFileMenuOnEvent,
    /// The widget theme this menu is PINNED to (`with_theme`), or `None` to
    /// follow the app theme.
    pub theme: OptionUiTheme,
}

impl RibbonFileMenuCommand {
    /// A command with `icon` and `label`, no sub-commands, enabled.
    #[must_use]
    pub const fn create(icon: AzString, label: AzString) -> Self {
        Self {
            icon,
            label,
            description: AzString::from_const_str(""),
            disabled_reason: AzString::from_const_str(""),
            children: RibbonFileMenuCommandVec::from_const_slice(&[]),
            separator_before: false,
        }
    }

    /// The line under a sub-command's label.
    pub fn set_description(&mut self, description: AzString) {
        self.description = description;
    }

    /// [`Self::set_description`] for the builder chain.
    #[must_use]
    pub fn with_description(mut self, description: AzString) -> Self {
        self.set_description(description);
        self
    }

    /// Disables the command: `reason` says why it cannot run now (empty
    /// enables it again).
    pub fn set_disabled(&mut self, reason: AzString) {
        self.disabled_reason = reason;
    }

    /// [`Self::set_disabled`] for the builder chain.
    #[must_use]
    pub fn with_disabled(mut self, reason: AzString) -> Self {
        self.set_disabled(reason);
        self
    }

    /// Whether the command is disabled (it has a reason).
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        !self.disabled_reason.as_str().is_empty()
    }

    /// Appends a sub-command (see [`Self::children`]).
    pub fn add_child(&mut self, child: RibbonFileMenuCommand) {
        self.children.push(child);
    }

    /// [`Self::add_child`] for the builder chain.
    #[must_use]
    pub fn with_child(mut self, child: RibbonFileMenuCommand) -> Self {
        self.add_child(child);
        self
    }

    /// A rule above this command (or none).
    pub const fn set_separator_before(&mut self, separator_before: bool) {
        self.separator_before = separator_before;
    }

    /// [`Self::set_separator_before`] for the builder chain.
    #[must_use]
    pub const fn with_separator_before(mut self, separator_before: bool) -> Self {
        self.set_separator_before(separator_before);
        self
    }
}

impl Default for RibbonFileMenuCommand {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""), AzString::from_const_str(""))
    }
}

impl RibbonFileMenuPlace {
    /// A place called `label`, not pinned.
    #[must_use]
    pub const fn create(label: AzString) -> Self {
        Self {
            label,
            detail: AzString::from_const_str(""),
            pinned: false,
        }
    }

    /// Where the place is (see [`Self::detail`]).
    pub fn set_detail(&mut self, detail: AzString) {
        self.detail = detail;
    }

    /// [`Self::set_detail`] for the builder chain.
    #[must_use]
    pub fn with_detail(mut self, detail: AzString) -> Self {
        self.set_detail(detail);
        self
    }

    /// Whether the place is pinned.
    pub const fn set_pinned(&mut self, pinned: bool) {
        self.pinned = pinned;
    }

    /// [`Self::set_pinned`] for the builder chain.
    #[must_use]
    pub const fn with_pinned(mut self, pinned: bool) -> Self {
        self.set_pinned(pinned);
        self
    }
}

impl Default for RibbonFileMenuPlace {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""))
    }
}

impl RibbonFileMenu {
    /// A menu of `commands`, no places, no callback, following the app theme.
    #[must_use]
    pub fn create(commands: RibbonFileMenuCommandVec) -> Self {
        Self {
            commands,
            places_title: AzString::from_const_str(""),
            places: RibbonFileMenuPlaceVec::from_const_slice(&[]),
            on_event: None.into(),
            theme: OptionUiTheme::None,
        }
    }

    /// Appends a command to the left column.
    pub fn add_command(&mut self, command: RibbonFileMenuCommand) {
        self.commands.push(command);
    }

    /// [`Self::add_command`] for the builder chain.
    #[must_use]
    pub fn with_command(mut self, command: RibbonFileMenuCommand) -> Self {
        self.add_command(command);
        self
    }

    /// The side column's places and their title.
    pub fn set_places(&mut self, title: AzString, places: RibbonFileMenuPlaceVec) {
        self.places_title = title;
        self.places = places;
    }

    /// [`Self::set_places`] for the builder chain.
    #[must_use]
    pub fn with_places(mut self, title: AzString, places: RibbonFileMenuPlaceVec) -> Self {
        self.set_places(title, places);
        self
    }

    /// The callback that hears every pick.
    pub fn set_on_event<C: Into<RibbonFileMenuOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_event = Some(RibbonFileMenuOnEvent {
            callback: cb.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<RibbonFileMenuOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_event(data, cb);
        self
    }

    /// Pin the widget theme; unset, the menu follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty menu and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut m = Self::create(RibbonFileMenuCommandVec::from_const_slice(&[]));
        core::mem::swap(&mut m, self);
        m
    }

    /// The menu's DOM: the `<transient-window>` holding the panel, closed -
    /// hand it to `RibbonAppButton::with_menu`, whose button opens it. The
    /// look comes from the theme module (`themes::flat::ribbon_file_menu` /
    /// `themes::flora::ribbon_file_menu`); `None` carries both looks, each
    /// in its `@theme(<name>)` block, and the app theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::ribbon_file_menu(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::ribbon_file_menu(self),
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                self,
                crate::widgets::themes::flat::ribbon_file_menu,
                crate::widgets::themes::flora::ribbon_file_menu,
            ),
        }
    }
}

impl Default for RibbonFileMenu {
    fn default() -> Self {
        Self::create(RibbonFileMenuCommandVec::from_const_slice(&[]))
    }
}

impl From<RibbonFileMenu> for Dom {
    fn from(m: RibbonFileMenu) -> Self {
        m.dom()
    }
}

// ==== the look ====

/// What a theme decides about the File menu: the SKIN of each part, laid
/// over the part's base (the menu's structure, the same in every theme) by
/// [`build`]. Built by `themes::flat::ribbon_file_menu_look` and
/// `themes::flora::ribbon_file_menu_look`.
pub(crate) struct RibbonFileMenuLook {
    /// The theme the rows' buttons are built in.
    pub theme: UiTheme,
    /// The panel: its paper, rule, shadow and type.
    pub menu: Vec<Cond>,
    /// The left column.
    pub commands: Vec<Cond>,
    /// A command row: its size, padding and faces.
    pub command: Vec<Cond>,
    /// A command's large icon.
    pub command_icon: Vec<Cond>,
    /// A command's label.
    pub command_label: Vec<Cond>,
    /// The ▸ of a command with sub-commands.
    pub arrow: Vec<Cond>,
    /// A rule between two groups of commands.
    pub rule: Vec<Cond>,
    /// The right column.
    pub side: Vec<Cond>,
    /// A panel's title.
    pub title: Vec<Cond>,
    /// A place's row.
    pub place: Vec<Cond>,
    /// A place's open button.
    pub open: Vec<Cond>,
    /// A place's number.
    pub number: Vec<Cond>,
    /// A place's label.
    pub place_label: Vec<Cond>,
    /// A place's pin button.
    pub pin: Vec<Cond>,
    /// The pin's glyph.
    pub pin_icon: Vec<Cond>,
    /// A sub-command row.
    pub sub: Vec<Cond>,
    /// A sub-command's icon.
    pub sub_icon: Vec<Cond>,
    /// A sub-command's label.
    pub sub_label: Vec<Cond>,
    /// The line under a sub-command's label.
    pub sub_description: Vec<Cond>,
    /// The theme's marker class on the panel, if it has one.
    pub marker: Option<&'static str>,
}

// ---- the base: the menu's structure, in every theme ----

/// The panel: the two columns side by side, as tall as the taller.
pub(crate) static MENU_BASE: &[Cond] = &[
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    simple(CssProperty::const_align_items(LayoutAlignItems::Stretch)),
];

/// A column: its rows one under the other, its width its own.
pub(crate) static COLUMN_BASE: &[Cond] = &[
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    display_flex(),
    flex_direction(LayoutFlexDirection::Column),
    grow(0),
    no_shrink(),
];

/// A row button - a command, a sub-command, a place's open button: its
/// content from the left on the midline, the arrow pointer, nothing to
/// select.
pub(crate) static ROW_BASE: &[Cond] = &[
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_justify_content(LayoutJustifyContent::Start)),
    grow(0),
    no_shrink(),
    simple(CssProperty::const_cursor(StyleCursor::Default)),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// A place's open button takes its row's room and gives way to the pin.
pub(crate) static OPEN_BASE: &[Cond] = &[
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_justify_content(LayoutJustifyContent::Start)),
    grow(1),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    simple(CssProperty::const_cursor(StyleCursor::Default)),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// What a row button holds: its parts side by side on the midline, in the
/// button's room.
pub(crate) static CONTENT_BASE: &[Cond] = &[
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    grow(1),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// A glyph keeps its size and is never selected.
pub(crate) static GLYPH_BASE: &[Cond] = &[
    grow(0),
    no_shrink(),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// A label takes the room left and ends in an ellipsis on one line.
pub(crate) static LABEL_BASE: &[Cond] = &[
    grow(1),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    nowrap(),
    overflow_x_hidden(),
    simple(CssProperty::TextOverflow(StyleTextOverflowValue::Exact(
        StyleTextOverflow::Ellipsis,
    ))),
];

/// A place's number keeps its width.
pub(crate) static NUMBER_BASE: &[Cond] = &[grow(0), no_shrink(), nowrap()];

/// A sub-command's words: its label over its description.
pub(crate) static SUB_TEXT_BASE: &[Cond] = &[
    display_flex(),
    flex_direction(LayoutFlexDirection::Column),
    grow(1),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// A description may wrap.
pub(crate) static DESCRIPTION_BASE: &[Cond] = &[grow(0)];

/// A rule keeps its line.
pub(crate) static RULE_BASE: &[Cond] = &[grow(0), no_shrink()];

/// The panel shown in the side column.
pub(crate) static PANEL_SHOWN_BASE: &[Cond] = &[
    display_flex(),
    flex_direction(LayoutFlexDirection::Column),
    grow(0),
];

/// A panel of the side column that waits for its command.
pub(crate) static PANEL_HIDDEN_BASE: &[Cond] = &[
    simple(CssProperty::const_display(LayoutDisplay::None)),
    flex_direction(LayoutFlexDirection::Column),
    grow(0),
];

/// A panel's title: one line, nothing to select.
pub(crate) static TITLE_BASE: &[Cond] = &[
    grow(0),
    no_shrink(),
    nowrap(),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// A place's row: its open button beside its pin.
pub(crate) static PLACE_BASE: &[Cond] = &[
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    grow(0),
    no_shrink(),
];

/// A pin: its glyph centred; faint until it is pinned or pointed at (the same
/// in every theme - it is the pin's state, not its paint).
pub(crate) static PIN_BASE: &[Cond] = &[
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    display_flex(),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_justify_content(LayoutJustifyContent::Center)),
    grow(0),
    no_shrink(),
    simple(CssProperty::const_cursor(StyleCursor::Default)),
    simple(CssProperty::user_select(StyleUserSelect::None)),
    simple(CssProperty::const_opacity(StyleOpacity {
        inner: PercentageValue::const_new(UNPINNED_OPACITY),
    })),
    CssPropertyWithConditions::on_hover(CssProperty::const_opacity(StyleOpacity {
        inner: PercentageValue::const_new(100),
    })),
];

/// Stacked on the pin of a pinned place: lit.
pub(crate) static PIN_PINNED_BASE: &[Cond] = &[simple(CssProperty::const_opacity(StyleOpacity {
    inner: PercentageValue::const_new(100),
}))];

/// An unpinned pin's opacity, in percent.
const UNPINNED_OPACITY: isize = 45;

/// The window the menu shows in: closed, under its anchor (the File tab),
/// closed by a press outside or Escape, per-pixel alpha so the panel's
/// rounded corners are real ones.
const fn window_config() -> TransientWindowConfig {
    TransientWindowConfig::closed()
        .with_anchor(TransientAnchor::Bottom)
        .with_dismiss(TransientDismiss::Outside)
        .with_material(WindowBackgroundMaterial::Transparent)
}

// ==== the shared state and the parts' data ====

/// What every part of one File menu shares, in the app's window (the
/// `<transient-window>` node's dataset and its `Dismissed` handler) and in
/// the popup (every row): the app's callback, and the pick a click made,
/// waiting for the popup to close.
struct FileMenuShared {
    on_event: OptionRibbonFileMenuOnEvent,
    pending: Option<RibbonFileMenuEvent>,
}

/// A row that picks something: the menu's shared part and the pick.
struct PickData {
    shared: RefAny,
    event: RibbonFileMenuEvent,
}

/// A command's panel in the side column: 0 the places, `k` the k-th command
/// with sub-commands'.
struct PanelData {
    panel: usize,
}

/// A row of the side column: the command whose panel it is in (its place
/// among the commands; the places answer to the first).
struct SideData {
    owner: usize,
}

/// A pin: the menu's shared part, its place, whether the place is pinned.
struct PinData {
    shared: RefAny,
    place: usize,
    pinned: bool,
}

const fn no_text() -> AzString {
    AzString::from_const_str("")
}

/// A rebuild the app asked for from the popup must reach the app's window
/// too (the menu is a subtree of that window's DOM).
const fn everywhere(update: Update) -> Update {
    match update {
        Update::RefreshDom => Update::RefreshDomAllWindows,
        other => other,
    }
}

/// Hands `event` to the app's callback. The hook is taken out first: the
/// app's callback runs without the menu's state borrowed.
fn tell_app(shared: &mut RefAny, info: CallbackInfo, event: RibbonFileMenuEvent) -> Update {
    let hook = match shared.downcast_ref::<FileMenuShared>() {
        Some(s) => s.on_event.clone(),
        None => return Update::DoNothing,
    };
    match hook.into_option() {
        Some(RibbonFileMenuOnEvent { callback, refany }) => callback.invoke(refany, info, event),
        None => Update::DoNothing,
    }
}

// ==== the DOM ====

/// Gives a built row button the menu item's role (it is a Button for its
/// keys, its disabled reason and its name).
fn as_menu_item(dom: &mut Dom) {
    let mut a11y = dom
        .root
        .get_accessibility_info()
        .cloned()
        .unwrap_or_default();
    a11y.role = AccessibilityRole::MenuItem;
    dom.root.set_accessibility_info(a11y);
}

/// A label in `style` on its base.
fn label_p(text: AzString, base: &[Cond], skin: &[Cond], class: Option<&'static str>) -> Dom {
    let mut p = crate::widgets::widget_p_with_text(text)
        .with_css_props(CssPropertyWithConditionsVec::from_vec(on_base(base, skin)));
    if let Some(class) = class {
        p.root.add_class(AzString::from_const_str(class));
    }
    p
}

/// The menu's DOM in `look` (module docs): the `<transient-window>`, closed,
/// holding the panel.
pub(crate) fn build(menu: RibbonFileMenu, look: &RibbonFileMenuLook) -> Dom {
    let part =
        |base: &[Cond], skin: &[Cond]| CssPropertyWithConditionsVec::from_vec(on_base(base, skin));
    let RibbonFileMenu {
        commands,
        places_title,
        places,
        on_event,
        // The look to build is `look.theme`: the field is the caller's pin,
        // already resolved by `dom()`.
        theme: _,
    } = menu;
    let theme = OptionUiTheme::Some(look.theme);
    let shared = RefAny::new(FileMenuShared {
        on_event,
        pending: None,
    });
    let click = |data: RefAny, cb: ButtonOnClickCallbackType| {
        OptionButtonOnClick::Some(ButtonOnClick::create(data, cb))
    };
    // A row button: `face` its container, `content` what it shows (icon,
    // words, arrow), `name` what assistive technology calls it.
    let row = |base: &[Cond],
               face: &[Cond],
               content: Dom,
               on_click: OptionButtonOnClick,
               disabled_reason: AzString,
               name: AzString| {
        let mut button = styled_button(
            no_text(),
            no_text(),
            no_text(),
            part(base, face),
            part(GLYPH_BASE, &[]),
            part(LABEL_BASE, &[]),
            part(GLYPH_BASE, &[]),
            on_click,
            disabled_reason,
            name,
            theme,
        );
        button.icon_dom = OptionDom::Some(content);
        let mut dom = button.dom();
        as_menu_item(&mut dom);
        dom
    };
    let commands: &[RibbonFileMenuCommand] = commands.as_ref();

    // ---- the left column ----
    let mut column: Vec<Dom> = Vec::with_capacity(commands.len() * 2);
    let mut next_panel = 0;
    for (index, command) in commands.iter().enumerate() {
        if command.separator_before && index > 0 {
            column.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![Class(
                        AzString::from_const_str(RULE_CLASS)
                    )]))
                    .with_css_props(part(RULE_BASE, &look.rule))
                    .with_accessibility_info(AccessibilityInfo {
                        role: AccessibilityRole::Separator,
                        ..Default::default()
                    }),
            );
        }
        let has_children = !command.children.as_ref().is_empty();
        let panel = if has_children {
            next_panel += 1;
            next_panel
        } else {
            0
        };
        let disabled = command.is_disabled();
        let mut content: Vec<Dom> = Vec::with_capacity(3);
        if !command.icon.as_str().is_empty() {
            content.push(
                Dom::create_icon(command.icon.clone())
                    .with_css_props(part(GLYPH_BASE, &look.command_icon)),
            );
        }
        content.push(label_p(
            command.label.clone(),
            LABEL_BASE,
            &look.command_label,
            None,
        ));
        if has_children {
            content.push(
                Dom::create_icon(AzString::from_const_str("chevron_right"))
                    .with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![Class(
                        AzString::from_const_str(ARROW_CLASS)
                    )]))
                    .with_css_props(part(GLYPH_BASE, &look.arrow)),
            );
        }
        let content = Dom::create_div()
            .with_css_props(part(CONTENT_BASE, &[]))
            .with_children(DomVec::from_vec(content));
        // A command with sub-commands opens its panel; any other one is a pick.
        let on_click = if disabled {
            OptionButtonOnClick::None
        } else if has_children {
            click(RefAny::new(PanelData { panel }), on_show_panel)
        } else {
            click(
                RefAny::new(PickData {
                    shared: shared.clone(),
                    event: RibbonFileMenuEvent {
                        kind: RibbonFileMenuEventKind::Command,
                        index,
                        sub_index: 0,
                    },
                }),
                on_pick,
            )
        };
        let mut dom = row(
            ROW_BASE,
            &look.command,
            content,
            on_click,
            command.disabled_reason.clone(),
            command.label.clone(),
        );
        dom.root.add_class(AzString::from_const_str(COMMAND_CLASS));
        if disabled {
            dom.root
                .add_class(AzString::from_const_str(COMMAND_DISABLED_CLASS));
        } else {
            dom.root.add_callback(
                EventFilter::Hover(HoverEventFilter::MouseEnter),
                RefAny::new(PanelData { panel }),
                on_show_panel as usize,
            );
        }
        dom.root.add_callback(
            EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
            RefAny::new(PanelData { panel }),
            on_command_key as usize,
        );
        column.push(dom);
    }
    let commands_column = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![Class(
            AzString::from_const_str(COMMANDS_CLASS)
        )]))
        .with_css_props(part(COLUMN_BASE, &look.commands))
        .with_children(DomVec::from_vec(column));

    // ---- the side column: the places, then each command's sub-commands ----
    let title = |text: AzString| label_p(text, TITLE_BASE, &look.title, Some(TITLE_CLASS));
    let panel_box = |shown: bool, children: Vec<Dom>| {
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![Class(
                AzString::from_const_str(PANEL_CLASS)
            )]))
            .with_css_props(part(
                if shown {
                    PANEL_SHOWN_BASE
                } else {
                    PANEL_HIDDEN_BASE
                },
                &[],
            ))
            .with_children(DomVec::from_vec(children))
    };

    let places: &[RibbonFileMenuPlace] = places.as_ref();
    let mut place_rows: Vec<Dom> = Vec::with_capacity(places.len() + 1);
    if !places_title.as_str().is_empty() {
        place_rows.push(title(places_title));
    }
    for (index, place) in places.iter().enumerate() {
        let number = if index < NUMBERED_PLACES {
            AzString::from(format!("{}", index + 1))
        } else {
            AzString::from(String::new())
        };
        let content = Dom::create_div()
            .with_css_props(part(CONTENT_BASE, &[]))
            .with_children(DomVec::from_vec(alloc::vec![
                label_p(number, NUMBER_BASE, &look.number, Some(NUMBER_CLASS)),
                label_p(place.label.clone(), LABEL_BASE, &look.place_label, None),
            ]));
        let name = if place.detail.as_str().is_empty() {
            place.label.clone()
        } else {
            AzString::from(format!("{} ({})", place.label.as_str(), place.detail.as_str()))
        };
        let mut open = row(
            OPEN_BASE,
            &look.open,
            content,
            click(
                RefAny::new(PickData {
                    shared: shared.clone(),
                    event: RibbonFileMenuEvent {
                        kind: RibbonFileMenuEventKind::OpenPlace,
                        index,
                        sub_index: 0,
                    },
                }),
                on_pick,
            ),
            no_text(),
            name,
        );
        open.root.add_class(AzString::from_const_str(OPEN_CLASS));
        open.root.add_callback(
            EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
            RefAny::new(SideData { owner: 0 }),
            on_side_key as usize,
        );

        let mut pin_base = PIN_BASE.to_vec();
        if place.pinned {
            pin_base.extend(PIN_PINNED_BASE.iter().cloned());
        }
        let mut pin = styled_button(
            AzString::from_const_str("push_pin"),
            no_text(),
            no_text(),
            part(&pin_base, &look.pin),
            part(GLYPH_BASE, &look.pin_icon),
            part(LABEL_BASE, &[]),
            part(GLYPH_BASE, &look.pin_icon),
            click(
                RefAny::new(PinData {
                    shared: shared.clone(),
                    place: index,
                    pinned: place.pinned,
                }),
                on_pin,
            ),
            no_text(),
            AzString::from(format!(
                "{} {}",
                if place.pinned { "Unpin" } else { "Pin" },
                place.label.as_str()
            )),
            theme,
        );
        pin.set_toggled(place.pinned);
        let mut pin = pin.dom();
        pin.root.add_class(AzString::from_const_str(PIN_CLASS));
        if place.pinned {
            pin.root.add_class(AzString::from_const_str(PINNED_CLASS));
        }
        place_rows.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![Class(
                    AzString::from_const_str(PLACE_CLASS)
                )]))
                .with_css_props(part(PLACE_BASE, &look.place))
                .with_children(DomVec::from_vec(alloc::vec![open, pin])),
        );
    }
    let mut panels: Vec<Dom> = alloc::vec![panel_box(true, place_rows)];

    for (owner, command) in commands.iter().enumerate() {
        let children: &[RibbonFileMenuCommand] = command.children.as_ref();
        if children.is_empty() {
            continue;
        }
        let mut rows: Vec<Dom> = Vec::with_capacity(children.len() + 1);
        rows.push(title(command.label.clone()));
        for (sub_index, sub) in children.iter().enumerate() {
            let mut words = alloc::vec![label_p(
                sub.label.clone(),
                LABEL_BASE,
                &look.sub_label,
                None
            )];
            if !sub.description.as_str().is_empty() {
                words.push(label_p(
                    sub.description.clone(),
                    DESCRIPTION_BASE,
                    &look.sub_description,
                    None,
                ));
            }
            let mut content: Vec<Dom> = Vec::with_capacity(2);
            if !sub.icon.as_str().is_empty() {
                content.push(
                    Dom::create_icon(sub.icon.clone())
                        .with_css_props(part(GLYPH_BASE, &look.sub_icon)),
                );
            }
            content.push(
                Dom::create_div()
                    .with_css_props(part(SUB_TEXT_BASE, &[]))
                    .with_children(DomVec::from_vec(words)),
            );
            let content = Dom::create_div()
                .with_css_props(part(CONTENT_BASE, &[]))
                .with_children(DomVec::from_vec(content));
            let on_click = if sub.is_disabled() || command.is_disabled() {
                OptionButtonOnClick::None
            } else {
                click(
                    RefAny::new(PickData {
                        shared: shared.clone(),
                        event: RibbonFileMenuEvent {
                            kind: RibbonFileMenuEventKind::SubCommand,
                            index: owner,
                            sub_index,
                        },
                    }),
                    on_pick,
                )
            };
            let mut dom = row(
                ROW_BASE,
                &look.sub,
                content,
                on_click,
                sub.disabled_reason.clone(),
                sub.label.clone(),
            );
            dom.root.add_class(AzString::from_const_str(SUB_CLASS));
            dom.root.add_callback(
                EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                RefAny::new(SideData { owner }),
                on_side_key as usize,
            );
            rows.push(dom);
        }
        panels.push(panel_box(false, rows));
    }
    let side = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![Class(
            AzString::from_const_str(SIDE_CLASS)
        )]))
        .with_css_props(part(COLUMN_BASE, &look.side))
        .with_children(DomVec::from_vec(panels));

    // ---- the panel and its window ----
    let mut classes: Vec<IdOrClass> = alloc::vec![Class(AzString::from_const_str(MENU_CLASS))];
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    let panel = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(MENU_BASE, &look.menu))
        // A panel the user works in, not a list picked from: the popup
        // takes the keyboard and its first command the focus
        // (`transient::transient_takes_focus`).
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Grouping,
            accessibility_name: Some(AzString::from_const_str("File")).into(),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(alloc::vec![commands_column, side]));
    menu_window(panel, shared)
}

/// `content` in the menu's `<transient-window>`: closed, its dataset the
/// menu's shared part (the application button clears a stale pick through
/// it), its `Dismissed` handler the one that hands a pick to the app in the
/// app's own window.
fn menu_window(content: Dom, shared: RefAny) -> Dom {
    let mut window = NodeData::create_node(NodeType::TransientWindow(window_config()));
    // The popup's root holds the keys, never a Tab stop of its own: the
    // first command takes the focus.
    window.set_tab_index(TabIndex::NoKeyboardFocus);
    window.set_dataset(OptionRefAny::Some(shared.clone()));
    window.set_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(merge_shared));
    window.add_callback(
        EventFilter::Component(ComponentEventFilter::Dismissed),
        shared,
        Callback::from_ptr(on_dismissed).to_core(),
    );
    Dom::create_from_data(window)
        .with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![Class(
            AzString::from_const_str(WINDOW_CLASS)
        )]))
        .with_child(content)
}

/// Hangs `menu` on the ribbon's application button `button` (what
/// `Ribbon::build_chrome` does with `RibbonAppButton::menu`): a DOM whose root
/// is not a `<transient-window>` goes into the menu's window first; the window
/// is the button's last child - its parent is its anchor, so it opens under
/// the File tab, and a press on the anchor is not taken as an outside press
/// - and the button's click opens and closes it, next to whatever click of
/// its own the app gave it.
pub(crate) fn hang_on_app_button(button: &mut Dom, menu: Dom) {
    let popup = if matches!(menu.root.get_node_type(), NodeType::TransientWindow(_)) {
        menu
    } else {
        // A plain DOM has no pick of its own to hand over: an empty shared
        // part, no callback.
        menu_window(
            menu,
            RefAny::new(FileMenuShared {
                on_event: OptionRibbonFileMenuOnEvent::None,
                pending: None,
            }),
        )
    };
    button.add_child(popup);
    button.root.add_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        RefAny::new(PanelData { panel: 0 }),
        on_app_button_toggle as usize,
    );
}

// ==== the handlers ====

/// The node carrying `class` from `start` up (`start` itself first).
fn ancestor_with_class(info: &CallbackInfo, start: DomNodeId, class: &str) -> Option<DomNodeId> {
    let mut current = Some(start);
    for _ in 0..32 {
        let node = current?;
        if roving::has_class(info, node, class) {
            return Some(node);
        }
        current = info.get_parent(node);
    }
    None
}

/// The top of the tree `node` is in: inside the popup, the popup's root
/// (the former `<transient-window>` node).
fn top_of(info: &CallbackInfo, node: DomNodeId) -> DomNodeId {
    let mut top = node;
    for _ in 0..256 {
        match info.get_parent(top) {
            Some(parent) => top = parent,
            None => break,
        }
    }
    top
}

/// The menu's commands, in order.
fn commands_of(info: &CallbackInfo, menu: DomNodeId) -> Vec<DomNodeId> {
    info.get_first_child(menu)
        .map(|column| roving::items_of(info, column, COMMAND_CLASS))
        .unwrap_or_default()
}

/// The panels of the menu's side column, in order (the places first).
fn panels_of(info: &CallbackInfo, menu: DomNodeId) -> Vec<DomNodeId> {
    info.get_last_child(menu)
        .map(|side| roving::items_of(info, side, PANEL_CLASS))
        .unwrap_or_default()
}

/// The focusable rows of a side panel: its sub-commands, or its places'
/// open buttons.
fn rows_of(info: &CallbackInfo, panel: DomNodeId) -> Vec<DomNodeId> {
    roving::children_of(info, panel)
        .into_iter()
        .filter_map(|child| {
            if roving::has_class(info, child, SUB_CLASS) {
                Some(child)
            } else if roving::has_class(info, child, PLACE_CLASS) {
                info.get_first_child(child)
            } else {
                None
            }
        })
        .collect()
}

/// Shows panel `shown` of the side column, hides the others - in place, no
/// app relayout.
fn show_panel(info: &mut CallbackInfo, menu: DomNodeId, shown: usize) {
    for (i, panel) in panels_of(info, menu).into_iter().enumerate() {
        let display = if i == shown {
            LayoutDisplay::Flex
        } else {
            LayoutDisplay::None
        };
        info.set_css_property(panel, CssProperty::const_display(display));
    }
}

/// A command was pointed at (or a command with sub-commands clicked): the
/// side column shows its panel - its sub-commands, or the places.
extern "C" fn on_show_panel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(panel) = data.downcast_ref::<PanelData>().map(|p| p.panel) else {
        return Update::DoNothing;
    };
    let Some(menu) = ancestor_with_class(&info, info.get_hit_node(), MENU_CLASS) else {
        return Update::DoNothing;
    };
    show_panel(&mut info, menu, panel);
    Update::DoNothing
}

/// A command, a sub-command or a place was clicked: the pick is noted and
/// the popup closes; the app hears it in its own window once the popup is
/// gone ([`on_dismissed`]).
extern "C" fn on_pick(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    let Some((mut shared, event)) = data
        .downcast_ref::<PickData>()
        .map(|p| (p.shared.clone(), p.event))
    else {
        return Update::DoNothing;
    };
    if let Some(mut s) = shared.downcast_mut::<FileMenuShared>() {
        s.pending = Some(event);
    }
    let popup = top_of(&info, info.get_hit_node());
    info.set_transient_window_open(popup, false);
    Update::DoNothing
}

/// Reconcile: a pick noted in the popup survives the app's rebuild between the
/// pick and the dismissal. The popup closes itself from its own window, which
/// wakes every window - the app's window rebuilds its DOM (a fresh menu, a
/// fresh shared part) before it hears the `Dismissed`, and the pick was left
/// in the old shared part: AzDrive's Delete history > Recent places ran
/// nothing. The new build's shared part (the one its `Dismissed` handler
/// holds) takes the old one's pick over.
extern "C" fn merge_shared(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    let pending = old_data
        .downcast_mut::<FileMenuShared>()
        .and_then(|mut old| old.pending.take());
    if let Some(mut new) = new_data.downcast_mut::<FileMenuShared>() {
        if new.pending.is_none() {
            new.pending = pending;
        }
    }
    new_data
}

/// The menu closed (a pick closed it, or a press outside, or Escape), in the
/// app's window: a pick waiting is handed to the app now, once.
extern "C" fn on_dismissed(mut data: RefAny, info: CallbackInfo) -> Update {
    let event = match data.downcast_mut::<FileMenuShared>() {
        Some(mut s) => s.pending.take(),
        None => return Update::DoNothing,
    };
    match event {
        Some(event) => tell_app(&mut data, info, event),
        None => Update::DoNothing,
    }
}

/// A place's pin was clicked: the app hears it at once and the menu stays
/// open; the pin shows its new state before the app's rebuild reaches it.
extern "C" fn on_pin(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    let Some((mut shared, place, pinned)) = data.downcast_mut::<PinData>().map(|mut p| {
        p.pinned = !p.pinned;
        (p.shared.clone(), p.place, !p.pinned)
    }) else {
        return Update::DoNothing;
    };
    let opacity = if pinned { UNPINNED_OPACITY } else { 100 };
    let pin = info.get_hit_node();
    info.set_css_property(
        pin,
        CssProperty::const_opacity(StyleOpacity {
            inner: PercentageValue::const_new(opacity),
        }),
    );
    everywhere(tell_app(
        &mut shared,
        info,
        RibbonFileMenuEvent {
            kind: RibbonFileMenuEventKind::TogglePin,
            index: place,
            sub_index: 0,
        },
    ))
}

/// A key on a command: Up / Down / Home / End walk the commands (the rules
/// are skipped), Right goes into the side panel the command shows (its
/// sub-commands, or the places) and focuses its first row. Enter and Space
/// are the Button's (a click).
extern "C" fn on_command_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(key) = roving::plain_key(&info.get_current_keyboard_state()) else {
        return Update::DoNothing;
    };
    let Some(panel) = data.downcast_ref::<PanelData>().map(|p| p.panel) else {
        return Update::DoNothing;
    };
    let command = info.get_hit_node();
    let Some(menu) = ancestor_with_class(&info, command, MENU_CLASS) else {
        return Update::DoNothing;
    };
    let step = match key {
        VirtualKeyCode::Up => Step::Previous,
        VirtualKeyCode::Down => Step::Next,
        VirtualKeyCode::Home => Step::First,
        VirtualKeyCode::End => Step::Last,
        VirtualKeyCode::Right => {
            let Some(target) = panels_of(&info, menu)
                .get(panel)
                .and_then(|p| rows_of(&info, *p).first().copied())
            else {
                return Update::DoNothing;
            };
            info.prevent_default();
            show_panel(&mut info, menu, panel);
            info.set_focus(FocusTarget::Id(target));
            return Update::DoNothing;
        }
        _ => return Update::DoNothing,
    };
    let commands = commands_of(&info, menu);
    let Some(current) = commands.iter().position(|n| *n == command) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    if let Some(next) = roving::step_target(current, commands.len(), step, true) {
        if next != current {
            info.set_focus(FocusTarget::Id(commands[next]));
        }
    }
    Update::DoNothing
}

/// A key on a row of the side column: Up / Down / Home / End walk the panel's
/// rows, Left goes back to the command whose panel it is.
extern "C" fn on_side_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(key) = roving::plain_key(&info.get_current_keyboard_state()) else {
        return Update::DoNothing;
    };
    let Some(owner) = data.downcast_ref::<SideData>().map(|d| d.owner) else {
        return Update::DoNothing;
    };
    let row = info.get_hit_node();
    let Some(menu) = ancestor_with_class(&info, row, MENU_CLASS) else {
        return Update::DoNothing;
    };
    let step = match key {
        VirtualKeyCode::Up => Step::Previous,
        VirtualKeyCode::Down => Step::Next,
        VirtualKeyCode::Home => Step::First,
        VirtualKeyCode::End => Step::Last,
        VirtualKeyCode::Left => {
            let Some(target) = commands_of(&info, menu).get(owner).copied() else {
                return Update::DoNothing;
            };
            info.prevent_default();
            info.set_focus(FocusTarget::Id(target));
            return Update::DoNothing;
        }
        _ => return Update::DoNothing,
    };
    let Some(panel) = ancestor_with_class(&info, row, PANEL_CLASS) else {
        return Update::DoNothing;
    };
    let rows = rows_of(&info, panel);
    let Some(current) = rows.iter().position(|n| *n == row) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    if let Some(next) = roving::step_target(current, rows.len(), step, false) {
        if next != current {
            info.set_focus(FocusTarget::Id(rows[next]));
        }
    }
    Update::DoNothing
}

/// The application button was clicked: its menu's window opens (or closes,
/// when it shows). The ENGINE knows whether it shows - its latch survives
/// the app's rebuilds and sees the closes the button did not cause (a press
/// outside, Escape). A fresh showing forgets any pick an earlier one left.
extern "C" fn on_app_button_toggle(_data: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let button = ancestor_with_class(&info, hit, "__azul-native-ribbon-appbutton").unwrap_or(hit);
    let Some(popup) = roving::items_of(&info, button, WINDOW_CLASS).last().copied() else {
        return Update::DoNothing;
    };
    let open = !info.is_transient_window_open(popup);
    if open {
        if let Some(mut shared) = info.get_dataset(popup) {
            if let Some(mut s) = shared.downcast_mut::<FileMenuShared>() {
                s.pending = None;
            }
        }
    }
    info.set_transient_window_open(popup, open);
    Update::DoNothing
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, NodeId},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };
    use azul_css::props::property::CssPropertyType;

    use super::*;
    use crate::{
        callbacks::CallbackChange,
        widgets::{
            ribbon::{Ribbon, RibbonAppButton, RibbonTab, RibbonTabVec},
            roving::test_support as rv,
            themes::{theme_blocks::checks, theme_checks},
        },
    };

    type Log = Arc<Mutex<Vec<RibbonFileMenuEvent>>>;

    extern "C" fn record(mut data: RefAny, _info: CallbackInfo, event: RibbonFileMenuEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(event);
        }
        Update::RefreshDom
    }

    fn events(log: &Log) -> Vec<RibbonFileMenuEvent> {
        log.lock().expect("log").clone()
    }

    fn s(text: &str) -> AzString {
        AzString::from(text)
    }

    fn ev(kind: RibbonFileMenuEventKind, index: usize, sub_index: usize) -> RibbonFileMenuEvent {
        RibbonFileMenuEvent {
            kind,
            index,
            sub_index,
        }
    }

    /// Windows 8's File menu, as AzDrive fills it: Open new window, Open
    /// terminal here (two sub-commands), Delete history (three, the second
    /// disabled), Help (two), a disabled Print, Close; and eleven places.
    fn menu(log: &Log) -> RibbonFileMenu {
        let commands = alloc::vec![
            RibbonFileMenuCommand::create(s("open_in_new"), s("Open new window")),
            RibbonFileMenuCommand::create(s("terminal"), s("Open terminal here"))
                .with_separator_before(true)
                .with_child(RibbonFileMenuCommand::create(s("terminal"), s("Terminal")))
                .with_child(
                    RibbonFileMenuCommand::create(s("code"), s("AzTerm"))
                        .with_description(s("Azlin's terminal")),
                ),
            RibbonFileMenuCommand::create(s("history"), s("Delete history"))
                .with_separator_before(true)
                .with_child(RibbonFileMenuCommand::create(s(""), s("Recent places")))
                .with_child(
                    RibbonFileMenuCommand::create(s(""), s("Address history"))
                        .with_disabled(s("AzDrive keeps no typed addresses.")),
                )
                .with_child(RibbonFileMenuCommand::create(s(""), s("Everything"))),
            RibbonFileMenuCommand::create(s("help"), s("Help"))
                .with_separator_before(true)
                .with_child(RibbonFileMenuCommand::create(s("keyboard"), s("Keyboard shortcuts")))
                .with_child(RibbonFileMenuCommand::create(s("info"), s("About"))),
            RibbonFileMenuCommand::create(s("print"), s("Print"))
                .with_disabled(s("Nothing to print here.")),
            RibbonFileMenuCommand::create(s("close"), s("Close")).with_separator_before(true),
        ];
        let places: Vec<RibbonFileMenuPlace> = (0..11)
            .map(|i| {
                RibbonFileMenuPlace::create(AzString::from(format!("Folder {i}")))
                    .with_detail(AzString::from(format!("Home/Folder {i}")))
                    .with_pinned(i < 2)
            })
            .collect();
        RibbonFileMenu::create(RibbonFileMenuCommandVec::from_vec(commands))
            .with_places(s("Frequent places"), RibbonFileMenuPlaceVec::from_vec(places))
            .with_on_event(RefAny::new(log.clone()), record as RibbonFileMenuOnEventCallbackType)
    }

    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
    }

    /// Every node of the subtree carrying `class`, depth first.
    fn find_all<'a>(node: &'a Dom, class: &str, out: &mut Vec<&'a Dom>) {
        if has_class(node, class) {
            out.push(node);
        }
        for child in node.children.as_ref() {
            find_all(child, class, out);
        }
    }

    fn all<'a>(node: &'a Dom, class: &str) -> Vec<&'a Dom> {
        let mut out = Vec::new();
        find_all(node, class, &mut out);
        out
    }

    /// Every text of the subtree, in document order.
    fn texts(node: &Dom, out: &mut Vec<String>) {
        if let NodeType::Text(t) = node.root.get_node_type() {
            out.push(t.as_ref().as_str().to_string());
        }
        for c in node.children.as_ref() {
            texts(c, out);
        }
    }

    /// The display a node declares last, unconditioned.
    fn display_of(node: &Dom) -> Option<LayoutDisplay> {
        node.root
            .style
            .iter_inline_properties()
            .filter(|(_, c)| c.as_ref().is_empty())
            .filter_map(|(p, _)| match p {
                CssProperty::Display(d) => d.get_property().copied(),
                _ => None,
            })
            .last()
    }

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

    fn id(n: NodeId) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(n)),
        }
    }

    fn fire(styled: &StyledDom, node: NodeId, event: EventFilter) -> Option<(Update, Vec<CallbackChange>)> {
        rv::fire(styled, id(node), event)
    }

    fn click(styled: &StyledDom, node: NodeId) -> Option<(Update, Vec<CallbackChange>)> {
        fire(styled, node, EventFilter::Hover(HoverEventFilter::Click))
    }

    fn dismissed(styled: &StyledDom) -> Option<(Update, Vec<CallbackChange>)> {
        fire(
            styled,
            NodeId::new(0),
            EventFilter::Component(ComponentEventFilter::Dismissed),
        )
    }

    /// `(node, open)` of every `SetTransientWindowOpen` in `changes`.
    fn window_writes(changes: &[CallbackChange]) -> Vec<(NodeId, bool)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::SetTransientWindowOpen { node, open } => {
                    node.node.into_crate_internal().map(|n| (n, *open))
                }
                _ => None,
            })
            .collect()
    }

    /// `(node, display)` of every display patch in `changes`.
    fn display_writes(changes: &[CallbackChange]) -> Vec<(NodeId, LayoutDisplay)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeCssProperties {
                    node_id,
                    properties,
                    ..
                } => properties.as_ref().iter().find_map(|p| match p {
                    CssProperty::Display(d) => d.get_property().map(|d| (*node_id, *d)),
                    _ => None,
                }),
                _ => None,
            })
            .collect()
    }

    fn stopped(changes: &[CallbackChange]) -> bool {
        changes
            .iter()
            .any(|c| matches!(c, CallbackChange::StopPropagation))
    }

    #[test]
    fn the_file_menu_is_a_transient_window_that_opens_under_the_file_tab() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = menu(&log).with_theme(UiTheme::Flat).dom();
        let NodeType::TransientWindow(cfg) = dom.root.get_node_type() else {
            panic!("the root is the menu's window");
        };
        assert!(!cfg.open, "it opens when the File tab is clicked, not when built");
        assert_eq!(cfg.anchor, TransientAnchor::Bottom, "under the tab");
        assert_eq!(cfg.dismiss, TransientDismiss::Outside, "a press outside or Escape closes it");
        assert!(has_class(&dom, WINDOW_CLASS));
        assert!(
            dom.root
                .get_callbacks()
                .as_ref()
                .iter()
                .any(|cb| cb.event == EventFilter::Component(ComponentEventFilter::Dismissed)),
            "the app's window hears the pick when the menu closes"
        );
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 1);
        assert!(has_class(&kids[0], MENU_CLASS));
    }

    #[test]
    fn the_menu_is_the_commands_column_and_the_side_column() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            let dom = menu(&log).with_theme(theme).dom();
            let panel = &dom.children.as_ref()[0];
            let columns = panel.children.as_ref();
            assert_eq!(columns.len(), 2, "{}", theme.name());
            assert!(has_class(&columns[0], COMMANDS_CLASS), "{}", theme.name());
            assert!(has_class(&columns[1], SIDE_CLASS), "{}", theme.name());
            let commands = all(&columns[0], COMMAND_CLASS);
            assert_eq!(commands.len(), 6, "{}: one row per command", theme.name());
            assert_eq!(
                all(&columns[0], RULE_CLASS).len(),
                4,
                "{}: a rule before each later group",
                theme.name()
            );
            let arrows: Vec<usize> = commands
                .iter()
                .enumerate()
                .filter(|(_, c)| !all(c, ARROW_CLASS).is_empty())
                .map(|(i, _)| i)
                .collect();
            assert_eq!(arrows, vec![1, 2, 3], "{}: ▸ on the commands with sub-commands", theme.name());
            assert_eq!(
                commands[0].root.get_accessibility_info().map(|a| a.role),
                Some(AccessibilityRole::MenuItem)
            );
            let panels = columns[1].children.as_ref();
            assert_eq!(panels.len(), 4, "{}: the places and three sub-command panels", theme.name());
            assert_eq!(display_of(&panels[0]), Some(LayoutDisplay::Flex), "the places show");
            for (i, p) in panels.iter().enumerate().skip(1) {
                assert_eq!(display_of(p), Some(LayoutDisplay::None), "{}: panel {i} waits", theme.name());
            }
            assert_eq!(all(&panels[2], SUB_CLASS).len(), 3, "Delete history's three");
        }
    }

    #[test]
    fn the_frequent_places_are_numbered_one_to_nine() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = menu(&log).with_theme(UiTheme::Flat).dom();
        let numbers: Vec<String> = all(&dom, NUMBER_CLASS)
            .iter()
            .map(|n| {
                let mut found = Vec::new();
                texts(n, &mut found);
                found.concat()
            })
            .collect();
        let mut want: Vec<String> = (1..=9).map(|i| i.to_string()).collect();
        want.extend([String::new(), String::new()]);
        assert_eq!(numbers, want);
        let mut found = Vec::new();
        texts(&dom, &mut found);
        assert!(found.iter().any(|t| t == "Frequent places"), "the places' title");
        let pins = all(&dom, PIN_CLASS);
        assert_eq!(pins.len(), 11, "a pin per place");
        assert!(has_class(pins[0], PINNED_CLASS) && has_class(pins[1], PINNED_CLASS));
        assert!(!has_class(pins[2], PINNED_CLASS), "an unpinned place's pin is not lit");
    }

    #[test]
    fn pointing_at_a_command_shows_its_sub_commands_and_the_others_bring_back_the_places() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(menu(&log).with_theme(UiTheme::Flat).dom());
        let commands = nodes_with_class(&styled, COMMAND_CLASS);
        let panels = nodes_with_class(&styled, PANEL_CLASS);
        let enter = EventFilter::Hover(HoverEventFilter::MouseEnter);
        // "Delete history" is the second command with sub-commands: panel 2.
        let (_, changes) = fire(&styled, commands[2], enter).expect("the command hears the pointer");
        let writes = display_writes(&changes);
        assert_eq!(writes.len(), panels.len());
        for (node, display) in writes {
            let want = if node == panels[2] {
                LayoutDisplay::Flex
            } else {
                LayoutDisplay::None
            };
            assert_eq!(display, want, "{node:?}");
        }
        // "Open new window" has none: the places again.
        let (_, changes) = fire(&styled, commands[0], enter).expect("the command hears the pointer");
        assert!(display_writes(&changes).contains(&(panels[0], LayoutDisplay::Flex)));
        // A click on a command with sub-commands shows them too - and picks nothing.
        let (_, changes) = click(&styled, commands[3]).expect("Help takes the click");
        assert!(display_writes(&changes).contains(&(panels[3], LayoutDisplay::Flex)));
        assert!(window_writes(&changes).is_empty(), "the menu stays open");
        assert!(events(&log).is_empty());
    }

    #[test]
    fn a_pick_closes_the_menu_and_the_app_hears_it_in_its_own_window_once() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(menu(&log).with_theme(UiTheme::Flat).dom());
        let commands = nodes_with_class(&styled, COMMAND_CLASS);
        // Close: the last command.
        let (_, changes) = click(&styled, commands[5]).expect("Close takes the click");
        assert_eq!(
            window_writes(&changes),
            vec![(NodeId::new(0), false)],
            "the popup closes itself"
        );
        assert!(events(&log).is_empty(), "nothing runs inside the popup");
        // The app's window hears it when the menu has closed - once.
        let (update, _) = dismissed(&styled).expect("the window hears its dismissal");
        assert_eq!(events(&log), vec![ev(RibbonFileMenuEventKind::Command, 5, 0)]);
        assert_eq!(update, Update::RefreshDom, "the app's verdict, in its own window");
        dismissed(&styled);
        assert_eq!(events(&log).len(), 1, "a pick is handed over once");
    }

    #[test]
    fn a_pick_survives_the_apps_rebuild_between_the_pick_and_the_dismissal() {
        // The popup closes itself from its own window, and that wakes every window: the app's
        // window rebuilds its DOM - a fresh menu, a fresh shared part - before it hears the
        // `Dismissed`. The engine carries the old shared part over the rebuild (the window
        // node's dataset merge), and the pick with it.
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let mut old = StyledDom::create_from_dom(menu(&log).with_theme(UiTheme::Flat).dom());
        let commands = nodes_with_class(&old, COMMAND_CLASS);
        click(&old, commands[5]).expect("Close takes the click");
        let mut new = StyledDom::create_from_dom(menu(&log).with_theme(UiTheme::Flat).dom());
        azul_core::diff::transfer_states(
            old.node_data.as_container_mut().internal,
            new.node_data.as_container_mut().internal,
            &[azul_core::diff::NodeMove {
                old_node_id: NodeId::new(0),
                new_node_id: NodeId::new(0),
            }],
        );
        dismissed(&new).expect("the rebuilt window hears its dismissal");
        assert_eq!(
            events(&log),
            vec![ev(RibbonFileMenuEventKind::Command, 5, 0)],
            "the pick made before the rebuild reaches the app"
        );
    }

    #[test]
    fn a_dismissal_without_a_pick_tells_the_app_nothing() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(menu(&log).with_theme(UiTheme::Flora).dom());
        dismissed(&styled).expect("the window hears its dismissal");
        assert!(events(&log).is_empty());
    }

    #[test]
    fn a_sub_command_and_a_place_report_what_they_are() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(menu(&log).with_theme(UiTheme::Flat).dom());
        let subs = nodes_with_class(&styled, SUB_CLASS);
        // Delete history > Everything: command 2, sub-command 2 (after
        // Terminal and AzTerm of command 1).
        let (_, changes) = click(&styled, subs[4]).expect("a sub-command takes the click");
        assert_eq!(window_writes(&changes), vec![(NodeId::new(0), false)]);
        dismissed(&styled);
        let opens = nodes_with_class(&styled, OPEN_CLASS);
        click(&styled, opens[3]).expect("a place takes the click");
        dismissed(&styled);
        assert_eq!(
            events(&log),
            vec![
                ev(RibbonFileMenuEventKind::SubCommand, 2, 2),
                ev(RibbonFileMenuEventKind::OpenPlace, 3, 0),
            ]
        );
    }

    #[test]
    fn the_pin_tells_the_app_at_once_and_the_menu_stays_open() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(menu(&log).with_theme(UiTheme::Flat).dom());
        let pins = nodes_with_class(&styled, PIN_CLASS);
        let (update, changes) = click(&styled, pins[4]).expect("the pin takes the click");
        assert_eq!(events(&log), vec![ev(RibbonFileMenuEventKind::TogglePin, 4, 0)]);
        assert!(stopped(&changes), "the pin's row does not open the place");
        assert!(window_writes(&changes).is_empty(), "the menu stays open");
        assert_eq!(update, Update::RefreshDomAllWindows, "the app's window rebuilds too");
        assert!(
            changes.iter().any(|c| matches!(
                c,
                CallbackChange::ChangeNodeCssProperties { properties, .. }
                    if properties.as_ref().iter().any(|p| p.get_type() == CssPropertyType::Opacity)
            )),
            "the pin lights at once"
        );
        dismissed(&styled);
        assert_eq!(events(&log).len(), 1, "the pin left no pick behind");
    }

    #[test]
    fn a_disabled_command_runs_nothing_and_says_why() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = menu(&log).with_theme(UiTheme::Flat).dom();
        let commands = all(&dom, COMMAND_CLASS);
        assert!(has_class(commands[4], COMMAND_DISABLED_CLASS), "Print is disabled");
        let unavailable = commands[4].root.get_accessibility_info().is_some_and(|a| {
            a.states
                .as_ref()
                .contains(&azul_core::a11y::AccessibilityState::Unavailable)
        });
        assert!(unavailable, "announced unavailable");
        let styled = StyledDom::create_from_dom(dom);
        let commands = nodes_with_class(&styled, COMMAND_CLASS);
        if let Some((_, changes)) = click(&styled, commands[4]) {
            assert!(window_writes(&changes).is_empty(), "a disabled command keeps the menu open");
        }
        dismissed(&styled);
        assert!(events(&log).is_empty(), "and picks nothing");
    }

    #[test]
    fn down_and_up_walk_the_commands_and_right_goes_into_the_sub_commands() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(menu(&log).with_theme(UiTheme::Flat).dom());
        let commands = nodes_with_class(&styled, COMMAND_CLASS);
        let (_, changes) =
            rv::press(&styled, id(commands[0]), VirtualKeyCode::Down, &[]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), Some(id(commands[1])), "the rule is skipped");
        let (_, changes) =
            rv::press(&styled, id(commands[0]), VirtualKeyCode::Up, &[]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), Some(id(commands[5])), "Up wraps to Close");
        // Right on "Open terminal here": its panel shows, its first row takes the focus.
        let subs = nodes_with_class(&styled, SUB_CLASS);
        let (_, changes) =
            rv::press(&styled, id(commands[1]), VirtualKeyCode::Right, &[]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), Some(id(subs[0])));
        // Left from there goes back to it.
        let (_, changes) =
            rv::press(&styled, id(subs[0]), VirtualKeyCode::Left, &[]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), Some(id(commands[1])));
        let (_, changes) =
            rv::press(&styled, id(subs[0]), VirtualKeyCode::Down, &[]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), Some(id(subs[1])));
    }

    fn tabs() -> RibbonTabVec {
        RibbonTabVec::from_vec(alloc::vec![RibbonTab::new(s("Home"))])
    }

    #[test]
    fn the_ribbon_hangs_the_file_menu_on_its_application_button_and_its_click_opens_it() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let ribbon = Ribbon::new(tabs())
            .with_app_button(RibbonAppButton::new(s("File")).with_menu(menu(&log).dom()))
            .with_theme(UiTheme::Flat)
            .dom_desktop();
        let styled = StyledDom::create_from_dom(ribbon);
        let button = nodes_with_class(&styled, "__azul-native-ribbon-appbutton");
        assert_eq!(button.len(), 1);
        let windows = nodes_with_class(&styled, WINDOW_CLASS);
        assert_eq!(windows.len(), 1);
        let hierarchy = styled.node_hierarchy.as_ref();
        assert_eq!(
            hierarchy[button[0].index()].last_child_id(),
            Some(windows[0]),
            "the window is the button's last child: the button is its anchor"
        );
        let (_, changes) = click(&styled, button[0]).expect("the button takes the click");
        assert_eq!(window_writes(&changes), vec![(windows[0], true)], "the click opens it");
    }

    #[test]
    fn an_application_button_without_a_menu_hangs_nothing() {
        let ribbon = Ribbon::new(tabs())
            .with_app_button(RibbonAppButton::new(s("File")))
            .with_theme(UiTheme::Flat)
            .dom_desktop();
        assert!(all(&ribbon, WINDOW_CLASS).is_empty());
    }

    #[test]
    fn a_plain_dom_as_the_menu_gets_a_window_of_its_own() {
        let mut button = Dom::create_div();
        hang_on_app_button(&mut button, Dom::create_div().with_child(crate::widgets::widget_p_with_text("Hi")));
        let popup = button.children.as_ref().last().expect("the window");
        assert!(matches!(popup.root.get_node_type(), NodeType::TransientWindow(_)));
        assert!(has_class(popup, WINDOW_CLASS));
        assert_eq!(popup.children.as_ref().len(), 1, "the DOM inside it");
    }

    #[test]
    fn a_file_menu_without_a_theme_follows_the_app_theme() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        checks::assert_follows_the_app_theme(
            "ribbon_file_menu",
            || menu(&log).dom(),
            |t: UiTheme| menu(&log).with_theme(t).dom(),
        );
    }

    /// What lays the menu out is the widget's own; the themes only paint it.
    #[test]
    fn the_file_menu_declares_its_structure_once_for_every_theme() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for t in checks::BOTH {
            let dom = checks::under(t, || menu(&log).dom());
            theme_checks::assert_structure_is_shared(
                &format!("ribbon file menu, built for {}", t.name()),
                &dom,
                &[],
            );
        }
    }
}
