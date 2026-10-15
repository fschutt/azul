//! The tab overview: the location tabs of a window as a list over a grey
//! scrim, anchored under the button that opened it at the top right of the
//! tab row ([`TabOverviewButton`], which shows their count).
//!
//! The list has a caps header, one row per open tab (its place's icon, its
//! folder's name, its path in soft ink, its Ctrl+number and a close button),
//! the current tab marked by a 3px accent bar and its name in the accent, a
//! "New tab" row and a footer of shortcut hints. The pointer lights a row;
//! the arrows move the keyboard's highlight, Return picks, Ctrl+W closes
//! the highlighted tab, Escape or a click on the scrim dismisses it.
//!
//! The tabs, which one is current and which is highlighted are the app's
//! (each tab keeps its own place, history, selection and scroll): the
//! overview reports what the user did ([`TabOverviewEvent`]) and the app
//! rebuilds it.

use alloc::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        AttributeType, Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass::Class,
        IdOrClassVec, TabIndex,
    },
    events::FocusEventFilter,
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions as C, CssPropertyWithConditionsVec},
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    props::{
        basic::{
            color::ColorU,
            font::{StyleFontFamily, StyleFontFamilyVec},
            length::FloatValue,
            StyleFontSize, PixelValue, PercentageValue,
        },
        layout::{LayoutOverflow, StyleTextOverflow, LayoutFlexDirection, LayoutWidth, LayoutFlexShrink, LayoutPosition, LayoutLeft, LayoutTop, LayoutRight, LayoutInsetBottom, LayoutZIndex, LayoutJustifyContent, LayoutAlignItems, LayoutColumnGap, LayoutMinWidth, LayoutRowGap, LayoutHeight, LayoutBoxSizing},
        property::CssProperty as P,
        style::{StyleTextTransform, StyleCursor, LayoutBorderLeftWidth, StyleBorderLeftStyle, BorderStyle, StyleBorderLeftColor, StyleOpacity, StyleLineHeight, StyleTextAlign},
    },
    AzString,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        button::{ButtonOnClick, ButtonOnClickCallback, OptionButtonOnClick},
        themes::{decl, flat, flora, OptionUiTheme, UiTheme},
    },
};

/// One open location tab as the overview lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct TabOverviewItem {
    /// The icon of its place (a folder, a drive).
    pub icon: AzString,
    /// Its folder's name.
    pub name: AzString,
    /// Where that folder is, shown under the name in soft ink.
    pub path: AzString,
    /// Its shortcut ("⌃1" .. "⌃9"), empty past the ninth.
    pub key: AzString,
}

impl TabOverviewItem {
    /// A tab of `name` at `path`, its place's `icon` and its `key`.
    #[must_use]
    pub const fn create(icon: AzString, name: AzString, path: AzString, key: AzString) -> Self {
        Self {
            icon,
            name,
            path,
            key,
        }
    }
}

impl_vec!(
    TabOverviewItem,
    TabOverviewItemVec,
    TabOverviewItemVecDestructor,
    TabOverviewItemVecDestructorType,
    TabOverviewItemVecSlice,
    OptionTabOverviewItem
);
impl_vec_clone!(TabOverviewItem, TabOverviewItemVec, TabOverviewItemVecDestructor);
impl_vec_debug!(TabOverviewItem, TabOverviewItemVec);
impl_vec_mut!(TabOverviewItem, TabOverviewItemVec);
impl_option!(TabOverviewItem, OptionTabOverviewItem, copy = false, [Debug, Clone, PartialEq, Eq]);

/// What the user did in the overview.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum TabOverviewEventKind {
    /// Picked the tab at `index` (a click on its row, Return on it): the app
    /// switches to it and closes the overview.
    Pick,
    /// Closed the tab at `index` (its x, Ctrl+W on it).
    Close,
    /// The "New tab" row: the app opens a tab on the current folder.
    NewTab,
    /// The keyboard moved the highlight to row `index` (the "New tab" row is
    /// the one after the last tab): the app rebuilds with it highlighted.
    Highlight,
    /// Escape, or a click on the scrim: the app closes the overview.
    Dismiss,
}

/// One action in the overview: what, and on which row.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct TabOverviewEvent {
    /// The row (`Pick`, `Close`, `Highlight`); 0 otherwise.
    pub index: usize,
    /// What happened.
    pub kind: TabOverviewEventKind,
}

/// Callback invoked for every action in the overview.
pub type TabOverviewOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, TabOverviewEvent) -> Update;
impl_widget_callback!(
    TabOverviewOnEvent,
    OptionTabOverviewOnEvent,
    TabOverviewOnEventCallback,
    TabOverviewOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TabOverviewOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TAB_OVERVIEW_ON_EVENT_INVOKER,
    invoker_ty:     AzTabOverviewOnEventCallbackInvoker,
    thunk_fn:       az_tab_overview_on_event_callback_thunk,
    setter_fn:      AzApp_setTabOverviewOnEventCallbackInvoker,
    from_handle_fn: AzTabOverviewOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzTabOverviewOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: TabOverviewEvent ],
}

/// The overview ([`TabOverview::dom`]): put it last in the window's body
/// while it is open, so it lies over everything.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TabOverview {
    /// The open tabs, in their order.
    pub items: TabOverviewItemVec,
    /// The current tab: its row carries the accent bar.
    pub current: usize,
    /// The row the keyboard highlights (the one after the last tab is the
    /// "New tab" row); past the end, none.
    pub highlighted: usize,
    /// The header ("TABS").
    pub title: AzString,
    /// The "New tab" row's words.
    pub new_tab: AzString,
    /// The "New tab" row's shortcut ("⌃T").
    pub new_tab_key: AzString,
    /// The footer's shortcut hints; empty: no footer.
    pub footer: AzString,
    /// The widget theme, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
    /// What the overview reports the user did.
    pub on_event: OptionTabOverviewOnEvent,
}

/// The scrim over the window.
pub const TAB_OVERVIEW_CLASS: &str = "__azul-tab-overview";
/// The list on it.
pub const TAB_OVERVIEW_LIST_CLASS: &str = "__azul-tab-overview-list";
/// A tab's row.
pub const TAB_OVERVIEW_ROW_CLASS: &str = "__azul-tab-overview-row";
/// Added to the current tab's row.
pub const TAB_OVERVIEW_CURRENT_CLASS: &str = "__azul-tab-overview-current";
/// Added to the highlighted row.
pub const TAB_OVERVIEW_HOT_CLASS: &str = "__azul-tab-overview-hot";
/// A row's close button.
pub const TAB_OVERVIEW_CLOSE_CLASS: &str = "__azul-tab-overview-close";
/// The "New tab" row.
pub const TAB_OVERVIEW_NEW_CLASS: &str = "__azul-tab-overview-new";
/// The tab button at the tab row's end.
pub const TAB_OVERVIEW_BUTTON_CLASS: &str = "__azul-tab-overview-button";
/// The count on it.
pub const TAB_OVERVIEW_BADGE_CLASS: &str = "__azul-tab-overview-badge";

/// The list's width, in px.
pub const TAB_OVERVIEW_WIDTH: isize = 330;
/// How far below the window's top the list hangs, in px: under the tab
/// row (28px) and its golden line, 4px clear.
pub const TAB_OVERVIEW_TOP: isize = 32;
/// How far from the window's right edge, in px: under its button.
pub const TAB_OVERVIEW_RIGHT: isize = 10;

/// The colours the overview is drawn in, light and dark, for a theme.
struct Palette {
    sur: (ColorU, ColorU),
    bd: (ColorU, ColorU),
    sep2: (ColorU, ColorU),
    ink: (ColorU, ColorU),
    soft1: (ColorU, ColorU),
    soft2: (ColorU, ColorU),
    icon: (ColorU, ColorU),
    hover: (ColorU, ColorU),
    hb: (ColorU, ColorU),
    accent: (ColorU, ColorU),
    on_accent: ColorU,
}

impl Palette {
    const fn of(theme: UiTheme) -> Self {
        match theme {
            UiTheme::Flora => Self {
                sur: (flora::LIGHT_SUR, flora::DARK_SUR),
                bd: (flora::LIGHT_BD, flora::DARK_BD),
                sep2: (flora::LIGHT_SEP2, flora::DARK_SEP2),
                ink: (flora::LIGHT_INK, flora::DARK_INK),
                soft1: (flora::LIGHT_SOFT1, flora::DARK_SOFT1),
                soft2: (flora::LIGHT_SOFT2, flora::DARK_SOFT2),
                icon: (flora::LIGHT_ICON, flora::DARK_ICON),
                hover: (ColorU::rgb(0xED, 0xEF, 0xF4), ColorU::rgb(0x2B, 0x2F, 0x37)),
                hb: (flora::LIGHT_HB, flora::DARK_HB),
                accent: (flora::LIGHT_ACC, flora::DARK_GLOW),
                on_accent: flora::LIGHT_ON_ACC,
            },
            UiTheme::Flat => Self {
                sur: (flat::LIGHT_SUR, flat::DARK_SUR),
                bd: (flat::LIGHT_BD, flat::DARK_BD),
                sep2: (flat::LIGHT_SEP2, flat::DARK_SEP2),
                ink: (flat::LIGHT_INK, flat::DARK_INK),
                soft1: (flat::LIGHT_SOFT1, flat::DARK_SOFT1),
                soft2: (flat::LIGHT_SOFT2, flat::DARK_SOFT2),
                icon: (flat::LIGHT_ICON, flat::DARK_ICON),
                hover: (flat::LIGHT_ROW_HOVER, flat::DARK_ROW_HOVER),
                hb: (flat::LIGHT_HB, flat::DARK_HB),
                // Flat's glow is a translucent halo: the night accent, opaque,
                // is what reads as words on its paper.
                accent: (flat::LIGHT_ACC, flat::DARK_ACC),
                on_accent: flat::LIGHT_ON_ACC,
            },
        }
    }
}

/// The scrim's grey, by day and by night.
const SCRIM: (ColorU, ColorU) = (ColorU::new(40, 38, 34, 82), ColorU::new(0, 0, 0, 128));
/// The list's cast shadow (`--shadow-pop`'s far layer), by day and night.
const POP_SHADOW: (ColorU, ColorU) = (ColorU::new(48, 45, 38, 46), ColorU::new(0, 0, 0, 140));

const SYSTEM_UI: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI)];
/// The system font, for the shortcut keys (the UI hand has no ⌃ ⌘ ⇧).
const SYSTEM_FONT: StyleFontFamilyVec = StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

/// What every callback of the overview carries: the row it sits on, how
/// many rows there are, the highlighted one and the app's hook.
#[derive(Clone)]
struct OverviewData {
    index: usize,
    rows: usize,
    highlighted: usize,
    on_event: OptionTabOverviewOnEvent,
}

/// Hands `kind` on row `index` to the app.
fn emit(data: &mut RefAny, info: CallbackInfo, kind: TabOverviewEventKind, index: usize) -> Update {
    let hook = match data.downcast_ref::<OverviewData>() {
        Some(d) => d.on_event.clone(),
        None => return Update::DoNothing,
    };
    match hook.into_option() {
        Some(TabOverviewOnEvent { callback, refany }) => {
            callback.invoke(refany, info, TabOverviewEvent { index, kind })
        }
        None => Update::DoNothing,
    }
}

fn row_of(data: &mut RefAny) -> Option<OverviewData> {
    data.downcast_ref::<OverviewData>().map(|d| d.clone())
}

extern "C" fn on_pick(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(d) = row_of(&mut data) else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    emit(&mut data, info, TabOverviewEventKind::Pick, d.index)
}

extern "C" fn on_close(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(d) = row_of(&mut data) else {
        return Update::DoNothing;
    };
    // The row under the x would pick the tab it closes.
    info.stop_propagation();
    emit(&mut data, info, TabOverviewEventKind::Close, d.index)
}

extern "C" fn on_new(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    emit(&mut data, info, TabOverviewEventKind::NewTab, 0)
}

/// A click on the list itself (its header, its footer) stays in the list:
/// only the scrim around it dismisses.
extern "C" fn on_list_click(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    Update::DoNothing
}

extern "C" fn on_scrim_click(mut data: RefAny, info: CallbackInfo) -> Update {
    emit(&mut data, info, TabOverviewEventKind::Dismiss, 0)
}

/// The list's keys: the arrows move the highlight (wrapping), Return picks
/// the highlighted row, Ctrl+W closes its tab, Escape dismisses.
extern "C" fn on_list_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::window::VirtualKeyCode as K;
    let Some(d) = row_of(&mut data) else {
        return Update::DoNothing;
    };
    let keys = info.get_current_keyboard_state();
    let rows = d.rows.max(1);
    let tabs = rows - 1;
    let at = d.highlighted.min(rows - 1);
    let (kind, index) = match keys.current_virtual_keycode.into_option() {
        Some(K::Down) => (TabOverviewEventKind::Highlight, (at + 1) % rows),
        Some(K::Up) => (TabOverviewEventKind::Highlight, (at + rows - 1) % rows),
        Some(K::Return) if at == tabs => (TabOverviewEventKind::NewTab, 0),
        Some(K::Return) => (TabOverviewEventKind::Pick, at),
        Some(K::W) if keys.ctrl_down() && at < tabs => (TabOverviewEventKind::Close, at),
        Some(K::Escape) => (TabOverviewEventKind::Dismiss, 0),
        _ => return Update::DoNothing,
    };
    info.prevent_default();
    emit(&mut data, info, kind, index)
}

/// One callback on `filter`.
fn on(filter: EventFilter, cb: usize, data: &RefAny) -> CoreCallbackData {
    CoreCallbackData {
        event: filter,
        callback: CoreCallback {
            cb,
            ctx: azul_core::refany::OptionRefAny::None,
        },
        refany: data.clone(),
    }
}

fn classes(names: &[&'static str]) -> IdOrClassVec {
    IdOrClassVec::from_vec(names.iter().map(|n| Class(AzString::from_const_str(n))).collect())
}

/// A line of text in `ink` at `px`, one line, cut with an ellipsis.
fn text(words: AzString, ink: (ColorU, ColorU), px: isize, extra: Vec<C>) -> Dom {
    let mut v = alloc::vec![
        decl::font_size(px),
        decl::nowrap(),
        C::simple(P::const_overflow_x(LayoutOverflow::Hidden)),
        C::simple(P::TextOverflow(StyleTextOverflow::Ellipsis.into())),
    ];
    v.extend(decl::themed_ink(ink.0, ink.1));
    v.extend(extra);
    crate::widgets::widget_p_with_text(words).with_css_props(CssPropertyWithConditionsVec::from_vec(v))
}

impl TabOverview {
    /// The overview of `items`, `current` being the window's tab, nothing
    /// highlighted, its words in English (set them from the app's own).
    #[must_use]
    pub fn create(items: TabOverviewItemVec, current: usize) -> Self {
        Self {
            items,
            current,
            highlighted: usize::MAX,
            title: AzString::from_const_str("Tabs"),
            new_tab: AzString::from_const_str("New tab"),
            new_tab_key: AzString::from_const_str("\u{2303}T"),
            footer: AzString::from_const_str(""),
            theme: OptionUiTheme::None,
            on_event: None.into(),
        }
    }

    /// The row the keyboard highlights.
    pub const fn set_highlighted(&mut self, row: usize) {
        self.highlighted = row;
    }

    /// [`Self::set_highlighted`] for the builder chain.
    #[must_use]
    pub const fn with_highlighted(mut self, row: usize) -> Self {
        self.set_highlighted(row);
        self
    }

    /// The header, the "New tab" row's words and shortcut, and the footer.
    pub fn set_words(&mut self, title: AzString, new_tab: AzString, new_tab_key: AzString, footer: AzString) {
        self.title = title;
        self.new_tab = new_tab;
        self.new_tab_key = new_tab_key;
        self.footer = footer;
    }

    /// [`Self::set_words`] for the builder chain.
    #[must_use]
    pub fn with_words(
        mut self,
        title: AzString,
        new_tab: AzString,
        new_tab_key: AzString,
        footer: AzString,
    ) -> Self {
        self.set_words(title, new_tab, new_tab_key, footer);
        self
    }

    /// Pin the widget theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// What the overview reports the user did.
    pub fn set_on_event<C2: Into<TabOverviewOnEventCallback>>(&mut self, data: RefAny, cb: C2) {
        self.on_event = Some(TabOverviewOnEvent {
            refany: data,
            callback: cb.into(),
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C2: Into<TabOverviewOnEventCallback>>(mut self, data: RefAny, cb: C2) -> Self {
        self.set_on_event(data, cb);
        self
    }

    /// The scrim over the window with the list on it, under the tab button.
    #[must_use]
    pub fn dom(self) -> Dom {
        let theme = self.theme.into_option().unwrap_or_else(UiTheme::current);
        let pal = Palette::of(theme);
        let tabs = self.items.as_ref().len();
        let rows = tabs + 1;
        let data = |index: usize| {
            RefAny::new(OverviewData {
                index,
                rows,
                highlighted: self.highlighted,
                on_event: self.on_event.clone(),
            })
        };

        let mut list_children: Vec<Dom> = Vec::with_capacity(tabs + 3);
        // The header, in the UI hand's capitals.
        let mut head = alloc::vec![
            C::simple(P::TextTransform(StyleTextTransform::Uppercase.into())),
            decl::letter_spacing_em(0.12),
            decl::bold(),
        ];
        head.extend(decl::padding(9, 14, 6, 14));
        if theme == UiTheme::Flora {
            head.push(C::simple(P::const_font_family(flora::font_caps())));
        }
        // The capitals a size step under the tab buttons' 11.5px.
        head.push(C::simple(P::const_font_size(StyleFontSize::px(10.5))));
        list_children.push(text(self.title.clone(), pal.soft1, 10, head));

        for (i, item) in self.items.as_ref().iter().enumerate() {
            list_children.push(self.row(i, item, &pal, data(i)));
        }
        list_children.push(self.new_row(tabs, &pal, data(tabs)));

        if !self.footer.as_str().is_empty() {
            let mut foot = alloc::vec![C::simple(P::const_font_family(SYSTEM_FONT))];
            foot.extend(decl::padding(7, 14, 9, 14));
            foot.extend(decl::border_top(1));
            foot.extend(decl::themed_border_top_color(pal.sep2.0, pal.sep2.1));
            list_children.push(text(self.footer.clone(), pal.soft2, 11, foot));
        }

        let mut list = alloc::vec![
            decl::display_flex(),
            decl::flex_direction(LayoutFlexDirection::Column),
            C::simple(P::const_width(LayoutWidth::const_px(TAB_OVERVIEW_WIDTH))),
            C::simple(P::const_flex_shrink(LayoutFlexShrink {
                inner: FloatValue::const_new(0),
            })),
            C::simple(P::const_overflow_x(LayoutOverflow::Hidden)),
            C::simple(P::const_overflow_y(LayoutOverflow::Hidden)),
        ];
        list.extend(decl::radius(8));
        list.extend(decl::border(1));
        list.extend(decl::themed_border_color(pal.bd.0, pal.bd.1));
        list.extend(decl::themed_fill(pal.sur.0, pal.sur.1));
        list.extend(decl::themed_shadow(9, 32, POP_SHADOW.0, POP_SHADOW.1));
        let list_data = data(self.highlighted);
        let list = Dom::create_div()
            .with_ids_and_classes(classes(&[TAB_OVERVIEW_LIST_CLASS]))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(list))
            .with_tab_index(TabIndex::Auto)
            .with_attribute(AttributeType::Autofocus)
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::List,
                accessibility_name: azul_css::OptionString::Some(self.title.clone()),
                ..Default::default()
            })
            .with_callbacks(
                alloc::vec![
                    on(EventFilter::Hover(HoverEventFilter::Click), on_list_click as usize, &list_data),
                    on(
                        EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                        on_list_key as usize,
                        &list_data,
                    ),
                ]
                .into(),
            )
            .with_children(DomVec::from_vec(list_children));

        let mut scrim = alloc::vec![
            decl::position(LayoutPosition::Absolute),
            C::simple(P::const_left(LayoutLeft::const_px(0))),
            C::simple(P::const_top(LayoutTop::const_px(0))),
            C::simple(P::const_right(LayoutRight::const_px(0))),
            C::simple(P::const_bottom(LayoutInsetBottom::const_px(0))),
            C::simple(P::const_z_index(LayoutZIndex::Integer(80))),
            decl::display_flex(),
            decl::flex_direction(LayoutFlexDirection::Row),
            C::simple(P::const_justify_content(LayoutJustifyContent::End)),
            C::simple(P::const_align_items(LayoutAlignItems::Start)),
        ];
        scrim.extend(decl::padding(TAB_OVERVIEW_TOP, TAB_OVERVIEW_RIGHT, 0, 0));
        scrim.extend(decl::themed_fill(SCRIM.0, SCRIM.1));
        Dom::create_div()
            .with_ids_and_classes(classes(&[TAB_OVERVIEW_CLASS]))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(scrim))
            .with_callbacks(
                alloc::vec![on(
                    EventFilter::Hover(HoverEventFilter::Click),
                    on_scrim_click as usize,
                    &data(0)
                )]
                .into(),
            )
            .with_child(list)
    }

    /// A row's box: icon, words, key; the 3px bar on its left, lit under the
    /// pointer and when highlighted.
    fn row_style(&self, i: usize, current: bool, pal: &Palette) -> Vec<C> {
        let mut v = alloc::vec![
            decl::display_flex(),
            decl::flex_direction(LayoutFlexDirection::Row),
            C::simple(P::const_align_items(LayoutAlignItems::Center)),
            C::simple(P::ColumnGap(azul_css::props::property::LayoutColumnGapValue::Exact(LayoutColumnGap {
                inner: PixelValue::const_px(11),
            }))),
            C::simple(P::const_cursor(StyleCursor::Pointer)),
            C::simple(P::const_border_left_width(LayoutBorderLeftWidth::const_px(3))),
            C::simple(P::const_border_left_style(StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            })),
        ];
        v.extend(decl::padding(7, 14, 7, 14));
        if current {
            v.extend(decl::themed_border_left_color(pal.accent.0, pal.accent.1));
        } else {
            v.push(C::simple(P::const_border_left_color(StyleBorderLeftColor {
                inner: ColorU::TRANSPARENT,
            })));
        }
        if i == self.highlighted {
            v.extend(decl::themed_fill(pal.hover.0, pal.hover.1));
        }
        v.extend(decl::hover_fill(pal.hover.0, pal.hover.1));
        v
    }

    /// A tab's row: its place's icon, its name over its path, its key and
    /// its close button.
    fn row(&self, i: usize, item: &TabOverviewItem, pal: &Palette, data: RefAny) -> Dom {
        let current = i == self.current;
        let hot = i == self.highlighted;
        let mut icon = alloc::vec![decl::font_size(20), decl::no_shrink()];
        icon.extend(decl::themed_ink(pal.icon.0, pal.icon.1));
        let name_ink = if current { pal.accent } else { pal.ink };
        let words = Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_vec(alloc::vec![
                decl::display_flex(),
                decl::flex_direction(LayoutFlexDirection::Column),
                decl::grow(1),
                C::simple(P::const_min_width(LayoutMinWidth::const_px(0))),
                C::simple(P::RowGap(azul_css::props::property::LayoutRowGapValue::Exact(LayoutRowGap {
                    inner: PixelValue::const_px(1),
                }))),
            ]))
            .with_children(DomVec::from_vec(alloc::vec![
                text(item.name.clone(), name_ink, 15, Vec::new()),
                text(item.path.clone(), pal.soft1, 12, Vec::new()),
            ]));
        let mut children = alloc::vec![
            Dom::create_icon(item.icon.clone())
                .with_css_props(CssPropertyWithConditionsVec::from_vec(icon)),
            words,
        ];
        if !item.key.as_str().is_empty() {
            children.push(text(
                item.key.clone(),
                pal.soft2,
                11,
                alloc::vec![C::simple(P::const_font_family(SYSTEM_FONT)), decl::no_shrink()],
            ));
        }
        // The x shows on the highlighted row and lights under the pointer.
        let mut x = alloc::vec![
            decl::display_flex(),
            C::simple(P::const_align_items(LayoutAlignItems::Center)),
            C::simple(P::const_justify_content(LayoutJustifyContent::Center)),
            C::simple(P::const_width(LayoutWidth::const_px(20))),
            C::simple(P::const_height(LayoutHeight::const_px(20))),
            decl::no_shrink(),
            decl::font_size(14),
            C::simple(P::const_opacity(StyleOpacity {
                inner: PercentageValue::const_new(if hot { 100 } else { 0 }),
            })),
            C::on_hover(P::const_opacity(StyleOpacity {
                inner: PercentageValue::const_new(100),
            })),
        ];
        x.extend(decl::radius(4));
        x.extend(decl::themed_ink(pal.soft1.0, pal.soft1.1));
        x.extend(decl::hover_fill(pal.hb.0, pal.hb.1));
        x.extend(decl::hover_ink(pal.ink.0, pal.ink.1));
        children.push(
            Dom::create_div()
                .with_ids_and_classes(classes(&[TAB_OVERVIEW_CLOSE_CLASS]))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(x))
                .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                    role: azul_core::a11y::AccessibilityRole::PushButton,
                    accessibility_name: azul_css::OptionString::Some(AzString::from(alloc::format!(
                        "Close {}",
                        item.name.as_str()
                    ))),
                    ..Default::default()
                })
                .with_callbacks(
                    alloc::vec![on(EventFilter::Hover(HoverEventFilter::Click), on_close as usize, &data)]
                        .into(),
                )
                .with_child(Dom::create_icon(AzString::from_const_str("close"))),
        );
        let mut names = alloc::vec![TAB_OVERVIEW_ROW_CLASS];
        if current {
            names.push(TAB_OVERVIEW_CURRENT_CLASS);
        }
        if hot {
            names.push(TAB_OVERVIEW_HOT_CLASS);
        }
        let mut a11y = azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::ListItem,
            accessibility_name: azul_css::OptionString::Some(item.name.clone()),
            ..Default::default()
        };
        if current {
            a11y.states = azul_core::a11y::AccessibilityStateVec::from_vec(alloc::vec![
                azul_core::a11y::AccessibilityState::Selected
            ]);
        }
        Dom::create_div()
            .with_ids_and_classes(classes(&names))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(self.row_style(i, current, pal)))
            .with_accessibility_info(a11y)
            .with_callbacks(
                alloc::vec![on(EventFilter::Hover(HoverEventFilter::Click), on_pick as usize, &data)]
                    .into(),
            )
            .with_children(DomVec::from_vec(children))
    }

    /// The "New tab" row: a plus and its words in soft ink, and its key.
    fn new_row(&self, i: usize, pal: &Palette, data: RefAny) -> Dom {
        let mut icon = alloc::vec![decl::font_size(20), decl::no_shrink()];
        icon.extend(decl::themed_ink(pal.soft1.0, pal.soft1.1));
        let mut words = text(self.new_tab.clone(), pal.soft1, 15, alloc::vec![decl::grow(1)]);
        words.root.add_class(AzString::from_const_str("__azul-tab-overview-new-name"));
        let mut children = alloc::vec![
            Dom::create_icon(AzString::from_const_str("add"))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(icon)),
            words,
        ];
        if !self.new_tab_key.as_str().is_empty() {
            children.push(text(
                self.new_tab_key.clone(),
                pal.soft2,
                11,
                alloc::vec![C::simple(P::const_font_family(SYSTEM_FONT)), decl::no_shrink()],
            ));
        }
        let mut names = alloc::vec![TAB_OVERVIEW_NEW_CLASS];
        if i == self.highlighted {
            names.push(TAB_OVERVIEW_HOT_CLASS);
        }
        Dom::create_div()
            .with_ids_and_classes(classes(&names))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(self.row_style(i, false, pal)))
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::ListItem,
                accessibility_name: azul_css::OptionString::Some(self.new_tab.clone()),
                ..Default::default()
            })
            .with_callbacks(
                alloc::vec![on(EventFilter::Hover(HoverEventFilter::Click), on_new as usize, &data)]
                    .into(),
            )
            .with_children(DomVec::from_vec(children))
    }
}

/// The button at the tab row's end that opens the overview: the `tab`
/// icon on a small button face, the number of open tabs on a badge when
/// there is more than one.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TabOverviewButton {
    /// How many location tabs are open.
    pub count: usize,
    /// What a click runs (the app opens or closes the overview).
    pub on_click: OptionButtonOnClick,
    /// The widget theme, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
}

impl TabOverviewButton {
    /// The button for `count` open tabs.
    #[must_use]
    pub const fn create(count: usize) -> Self {
        Self {
            count,
            on_click: OptionButtonOnClick::None,
            theme: OptionUiTheme::None,
        }
    }

    /// What a click runs.
    pub fn set_on_click<C2: Into<ButtonOnClickCallback>>(&mut self, data: RefAny, on_click: C2) {
        self.on_click = Some(ButtonOnClick {
            refany: data,
            callback: on_click.into(),
        })
        .into();
    }

    /// [`Self::set_on_click`] for the builder chain.
    #[must_use]
    pub fn with_on_click<C2: Into<ButtonOnClickCallback>>(mut self, data: RefAny, on_click: C2) -> Self {
        self.set_on_click(data, on_click);
        self
    }

    /// Pin the widget theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// The button, its badge over its top right corner.
    #[must_use]
    pub fn dom(self) -> Dom {
        let theme = self.theme.into_option().unwrap_or_else(UiTheme::current);
        let pal = Palette::of(theme);
        let mut style = crate::widgets::ribbon::strip_button_style(theme).into_library_owned_vec();
        style.push(decl::position(LayoutPosition::Relative));
        let mut button = Dom::create_div()
            .with_ids_and_classes(classes(&[TAB_OVERVIEW_BUTTON_CLASS]))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(style))
            .with_tab_index(TabIndex::Auto)
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::PushButton,
                accessibility_name: azul_css::OptionString::Some(AzString::from(alloc::format!(
                    "Tabs ({})",
                    self.count
                ))),
                ..Default::default()
            })
            .with_child(Dom::create_icon(AzString::from_const_str("tab")));
        if self.count > 1 {
            let mut badge = alloc::vec![
                decl::position(LayoutPosition::Absolute),
                C::simple(P::const_right(LayoutRight::const_px(-4))),
                C::simple(P::const_top(LayoutTop::const_px(-4))),
                C::simple(P::const_box_sizing(LayoutBoxSizing::BorderBox)),
                C::simple(P::const_min_width(LayoutMinWidth::const_px(13))),
                C::simple(P::const_height(LayoutHeight::const_px(13))),
                C::simple(P::const_line_height(StyleLineHeight::Length(PixelValue::const_px(13)))),
                C::simple(P::const_font_family(SYSTEM_FONT)),
                decl::font_size(9),
                decl::semibold(),
                C::simple(P::const_text_align(StyleTextAlign::Center)),
                C::simple(decl::ink(pal.on_accent)),
            ];
            badge.extend(decl::padding(0, 3, 0, 3));
            badge.extend(decl::radius(7));
            badge.extend(decl::themed_fill(pal.accent.0, pal.accent.1));
            badge.extend(decl::themed_shadow(0, 0, pal.sur.0, pal.sur.1));
            let mut badge = crate::widgets::widget_p_with_text(AzString::from(alloc::format!(
                "{}",
                self.count
            )))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(badge));
            badge.root.add_class(AzString::from_const_str(TAB_OVERVIEW_BADGE_CLASS));
            button.add_child(badge);
        }
        if let Some(click) = self.on_click.into_option() {
            button = button.with_callbacks(
                alloc::vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::Click),
                    callback: CoreCallback {
                        cb: click.callback.cb as *const () as usize,
                        ctx: click.callback.ctx,
                    },
                    refany: click.refany,
                }]
                .into(),
            );
        }
        button
    }
}

#[cfg(test)]
mod tests {
    //! The overview's structure and its keys against the design.

    use super::*;

    fn items(n: usize) -> TabOverviewItemVec {
        TabOverviewItemVec::from_vec(
            (0..n)
                .map(|i| {
                    TabOverviewItem::create(
                        AzString::from_const_str("folder"),
                        AzString::from(alloc::format!("Folder {i}")),
                        AzString::from(alloc::format!("Home / Folder {i}")),
                        AzString::from(alloc::format!("\u{2303}{}", i + 1)),
                    )
                })
                .collect(),
        )
    }

    fn has(dom: &Dom, class: &str) -> bool {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| c.as_class() == Some(class))
    }

    fn all<'a>(dom: &'a Dom, class: &str, out: &mut Vec<&'a Dom>) {
        if has(dom, class) {
            out.push(dom);
        }
        for c in dom.children.as_ref() {
            all(c, class, out);
        }
    }

    #[test]
    fn the_overview_lists_every_tab_and_a_new_tab_row_on_a_scrim() {
        let dom = TabOverview::create(items(3), 1).with_theme(UiTheme::Flora).dom();
        assert!(has(&dom, TAB_OVERVIEW_CLASS), "the scrim");
        let mut rows = Vec::new();
        all(&dom, TAB_OVERVIEW_ROW_CLASS, &mut rows);
        assert_eq!(rows.len(), 3, "a row per tab");
        let mut current = Vec::new();
        all(&dom, TAB_OVERVIEW_CURRENT_CLASS, &mut current);
        assert_eq!(current.len(), 1, "one current tab");
        let mut new = Vec::new();
        all(&dom, TAB_OVERVIEW_NEW_CLASS, &mut new);
        assert_eq!(new.len(), 1, "the New tab row");
        let mut x = Vec::new();
        all(&dom, TAB_OVERVIEW_CLOSE_CLASS, &mut x);
        assert_eq!(x.len(), 3, "a close button per tab");
    }

    #[test]
    fn the_highlighted_row_is_marked_hot() {
        let dom = TabOverview::create(items(3), 0).with_highlighted(3).dom();
        let mut hot = Vec::new();
        all(&dom, TAB_OVERVIEW_HOT_CLASS, &mut hot);
        assert_eq!(hot.len(), 1);
        assert!(has(hot[0], TAB_OVERVIEW_NEW_CLASS), "row 3 of 3 tabs is the New tab row");
    }

    #[test]
    fn the_tab_button_shows_a_badge_only_for_more_than_one_tab() {
        let one = TabOverviewButton::create(1).with_theme(UiTheme::Flora).dom();
        let mut badges = Vec::new();
        all(&one, TAB_OVERVIEW_BADGE_CLASS, &mut badges);
        assert!(badges.is_empty(), "one tab: no count");
        let three = TabOverviewButton::create(3).with_theme(UiTheme::Flora).dom();
        let mut badges = Vec::new();
        all(&three, TAB_OVERVIEW_BADGE_CLASS, &mut badges);
        assert_eq!(badges.len(), 1, "three tabs: the count");
    }
}
