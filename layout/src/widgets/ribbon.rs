//! Microsoft Office-style ribbon widget.
//!
//! A [`Ribbon`] organizes controls into a tabbed toolbar where each tab
//! contains one or more [`RibbonSection`]s, each with a title and arbitrary
//! content.  Unlike the simpler [`super::tabs`] widget, each tab is further
//! subdivided into titled, visually separated sections — matching the ribbon
//! pattern found in Office applications.

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions as Cond, CssPropertyWithConditionsVec},
    props::{
        basic::{color::ColorU, font::{StyleFontFamily, StyleFontFamilyVec}, *},
        layout::*,
        property::CssProperty as P,
        style::*,
    },
    *,
};

use azul_css::{impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_partialeq, impl_vec_mut};

use crate::callbacks::{Callback, CallbackInfo};

// -- Callback --

/// Callback signature invoked when a ribbon tab is clicked.
pub type RibbonOnTabClickCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    RibbonOnTabClick, OptionRibbonOnTabClick,
    RibbonOnTabClickCallback, RibbonOnTabClickCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        RibbonOnTabClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: RIBBON_ON_TAB_CLICK_INVOKER,
    invoker_ty:     AzRibbonOnTabClickCallbackInvoker,
    thunk_fn:       az_ribbon_on_tab_click_callback_thunk,
    setter_fn:      AzApp_setRibbonOnTabClickCallbackInvoker,
    from_handle_fn: AzRibbonOnTabClickCallback_createFromHostHandle,
    extra_args:     [ tab_index: usize ],
}

// -- Font --

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

// -- Colors --

const WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
const LIGHT_GRAY: ColorU = ColorU { r: 240, g: 240, b: 240, a: 255 };
const BORDER_GRAY: ColorU = ColorU { r: 200, g: 200, b: 200, a: 255 };
const TEXT_GRAY: ColorU = ColorU { r: 100, g: 100, b: 100, a: 255 };
const ACTIVE_BLUE: ColorU = ColorU { r: 0, g: 114, b: 198, a: 255 };
const BG_WHITE: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(WHITE)];
const BG_LIGHT_GRAY: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(LIGHT_GRAY)];

static RIBBON_CONTAINER_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
    Cond::simple(P::const_font_family(SYSTEM_UI_FAMILY)),
    Cond::simple(P::const_font_size(StyleFontSize::const_px(12))),
];

static TAB_BAR_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
    Cond::simple(P::const_background_content(StyleBackgroundContentVec::from_const_slice(BG_LIGHT_GRAY))),
    Cond::simple(P::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
    Cond::simple(P::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })),
    Cond::simple(P::const_border_bottom_color(StyleBorderBottomColor { inner: BORDER_GRAY })),
];

static TAB_INACTIVE_STYLE: &[Cond] = &[
    Cond::simple(P::const_padding_left(LayoutPaddingLeft::const_px(12))),
    Cond::simple(P::const_padding_right(LayoutPaddingRight::const_px(12))),
    Cond::simple(P::const_padding_top(LayoutPaddingTop::const_px(6))),
    Cond::simple(P::const_padding_bottom(LayoutPaddingBottom::const_px(6))),
    Cond::simple(P::const_cursor(StyleCursor::Pointer)),
    Cond::simple(P::const_text_color(StyleTextColor { inner: TEXT_GRAY })),
];

static TAB_ACTIVE_STYLE: &[Cond] = &[
    Cond::simple(P::const_padding_left(LayoutPaddingLeft::const_px(12))),
    Cond::simple(P::const_padding_right(LayoutPaddingRight::const_px(12))),
    Cond::simple(P::const_padding_top(LayoutPaddingTop::const_px(6))),
    Cond::simple(P::const_padding_bottom(LayoutPaddingBottom::const_px(6))),
    Cond::simple(P::const_cursor(StyleCursor::Pointer)),
    Cond::simple(P::const_background_content(StyleBackgroundContentVec::from_const_slice(BG_WHITE))),
    Cond::simple(P::const_border_bottom_width(LayoutBorderBottomWidth::const_px(2))),
    Cond::simple(P::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })),
    Cond::simple(P::const_border_bottom_color(StyleBorderBottomColor { inner: ACTIVE_BLUE })),
];

static SECTIONS_CONTAINER_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
    Cond::simple(P::const_background_content(StyleBackgroundContentVec::from_const_slice(BG_WHITE))),
    Cond::simple(P::const_padding_top(LayoutPaddingTop::const_px(4))),
    Cond::simple(P::const_padding_bottom(LayoutPaddingBottom::const_px(4))),
    Cond::simple(P::const_padding_left(LayoutPaddingLeft::const_px(4))),
    Cond::simple(P::const_padding_right(LayoutPaddingRight::const_px(4))),
    Cond::simple(P::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
    Cond::simple(P::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })),
    Cond::simple(P::const_border_bottom_color(StyleBorderBottomColor { inner: BORDER_GRAY })),
];

static SECTION_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
    Cond::simple(P::const_padding_left(LayoutPaddingLeft::const_px(6))),
    Cond::simple(P::const_padding_right(LayoutPaddingRight::const_px(6))),
    Cond::simple(P::const_border_right_width(LayoutBorderRightWidth::const_px(1))),
    Cond::simple(P::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid })),
    Cond::simple(P::const_border_right_color(StyleBorderRightColor { inner: BORDER_GRAY })),
];

static SECTION_CONTENT_STYLE: &[Cond] = &[
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
];

static SECTION_TITLE_STYLE: &[Cond] = &[
    Cond::simple(P::const_font_size(StyleFontSize::const_px(11))),
    Cond::simple(P::const_text_color(StyleTextColor { inner: TEXT_GRAY })),
    Cond::simple(P::const_text_align(StyleTextAlign::Center)),
    Cond::simple(P::const_padding_top(LayoutPaddingTop::const_px(2))),
];

/// Top-level ribbon widget containing multiple tabs.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Ribbon {
    /// Tabs displayed in the ribbon tab bar.
    pub tabs: RibbonTabVec,
    /// Index of the currently active tab.
    pub active_tab: usize,
    /// Optional callback fired when a tab is clicked.
    pub on_tab_click: OptionRibbonOnTabClick,
}

/// A single tab within a [`Ribbon`], containing a label and sections.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct RibbonTab {
    /// Display label shown in the tab bar.
    pub label: AzString,
    /// Sections rendered when this tab is active.
    pub sections: RibbonSectionVec,
}

/// A titled section within a [`RibbonTab`], holding arbitrary content.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct RibbonSection {
    /// Title displayed below the section content.
    pub title: AzString,
    /// Content DOM rendered inside this section.
    pub content: Dom,
}

impl_option!(RibbonSection, OptionRibbonSection, copy = false, [Debug, Clone]);
impl_vec!(RibbonSection, RibbonSectionVec, RibbonSectionVecDestructor, RibbonSectionVecDestructorType, RibbonSectionVecSlice, OptionRibbonSection);
impl_vec_clone!(RibbonSection, RibbonSectionVec, RibbonSectionVecDestructor);
impl_vec_debug!(RibbonSection, RibbonSectionVec);
impl_vec_mut!(RibbonSection, RibbonSectionVec);

impl_option!(RibbonTab, OptionRibbonTab, copy = false, [Debug, Clone]);
impl_vec!(RibbonTab, RibbonTabVec, RibbonTabVecDestructor, RibbonTabVecDestructorType, RibbonTabVecSlice, OptionRibbonTab);
impl_vec_clone!(RibbonTab, RibbonTabVec, RibbonTabVecDestructor);
impl_vec_debug!(RibbonTab, RibbonTabVec);
impl_vec_mut!(RibbonTab, RibbonTabVec);

impl RibbonTab {
    /// Creates a new tab with the given label and no sections.
    #[must_use] pub const fn new(label: AzString) -> Self {
        Self { label, sections: RibbonSectionVec::from_const_slice(&[]) }
    }

    /// Appends a section to this tab.
    pub fn add_section(&mut self, section: RibbonSection) {
        self.sections.push(section);
    }

    /// Builder method: appends a section and returns `self`.
    #[must_use] pub fn with_section(mut self, section: RibbonSection) -> Self {
        self.add_section(section);
        self
    }
}

impl RibbonSection {
    /// Creates a new section with the given title and content DOM.
    #[must_use] pub const fn new(title: AzString, content: Dom) -> Self {
        Self { title, content }
    }
}

impl Ribbon {
    /// Creates a new ribbon with the given tabs, defaulting to the first tab active.
    #[must_use] pub fn new(tabs: RibbonTabVec) -> Self {
        Self { tabs, active_tab: 0, on_tab_click: None.into() }
    }

    /// Sets the active tab by index, clamping to the last valid tab.
    pub const fn set_active_tab(&mut self, index: usize) {
        let max = self.tabs.len().saturating_sub(1);
        self.active_tab = if index > max { max } else { index };
    }

    /// Registers a callback invoked when a tab is clicked.
    pub fn set_on_tab_click<C: Into<RibbonOnTabClickCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_tab_click = Some(RibbonOnTabClick {
            callback: cb.into(), refany: data,
        }).into();
    }

    /// Builder method: registers a tab-click callback and returns `self`.
    pub fn with_on_tab_click<C: Into<RibbonOnTabClickCallback>>(mut self, data: RefAny, cb: C) -> Self {
        self.set_on_tab_click(data, cb);
        self
    }

    /// Builds the ribbon DOM, rendering the tab bar and the active tab's sections.
    #[must_use] pub fn dom(self) -> Dom {
        let active_tab = self.active_tab;
        let has_callback = self.on_tab_click.is_some();

        let tab_items: Vec<Dom> = self.tabs.as_slice().iter().enumerate().map(|(idx, tab)| {
            let style = if idx == active_tab { TAB_ACTIVE_STYLE } else { TAB_INACTIVE_STYLE };
            let mut d = Dom::create_text(tab.label.clone())
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(style));
            if has_callback {
                d = d.with_callbacks(vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    callback: CoreCallback {
                        cb: on_ribbon_tab_click as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(TabClickData {
                        tab_idx: idx, on_tab_click: self.on_tab_click.clone(),
                    }),
                }].into());
            }
            d
        }).collect();

        let tab_bar = Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(TAB_BAR_STYLE))
            .with_children(DomVec::from_vec(tab_items));

        let sections_dom = if let Some(active) = self.tabs.into_library_owned_vec().into_iter().nth(active_tab) {
            let items: Vec<Dom> = active.sections.into_library_owned_vec().into_iter().map(|s| {
                let content = Dom::create_div()
                    .with_css_props(CssPropertyWithConditionsVec::from_const_slice(SECTION_CONTENT_STYLE))
                    .with_children(DomVec::from_vec(vec![s.content]));
                let title = Dom::create_text(s.title)
                    .with_css_props(CssPropertyWithConditionsVec::from_const_slice(SECTION_TITLE_STYLE));
                Dom::create_div()
                    .with_css_props(CssPropertyWithConditionsVec::from_const_slice(SECTION_STYLE))
                    .with_children(DomVec::from_vec(vec![content, title]))
            }).collect();
            Dom::create_div()
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(SECTIONS_CONTAINER_STYLE))
                .with_children(DomVec::from_vec(items))
        } else {
            Dom::create_div()
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(SECTIONS_CONTAINER_STYLE))
        };

        Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(RIBBON_CONTAINER_STYLE))
            .with_ids_and_classes({
                const CLS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-ribbon"))];
                IdOrClassVec::from_const_slice(CLS)
            })
            .with_children(DomVec::from_vec(vec![tab_bar, sections_dom]))
    }
}

struct TabClickData {
    tab_idx: usize,
    on_tab_click: OptionRibbonOnTabClick,
}

extern "C" fn on_ribbon_tab_click(mut refany: RefAny, info: CallbackInfo) -> Update {
    let mut data = match refany.downcast_mut::<TabClickData>() {
        Some(d) => d,
        None => return Update::DoNothing,
    };
    let idx = data.tab_idx;
    match data.on_tab_click.as_mut() {
        Some(RibbonOnTabClick { refany, callback }) => {
            (callback.cb)(refany.clone(), info, idx)
        }
        None => Update::DoNothing,
    }
}

impl From<Ribbon> for Dom {
    fn from(r: Ribbon) -> Self { r.dom() }
}
