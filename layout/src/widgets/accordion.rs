//! Accordion / expander widget — one or more collapsible titled sections. Each
//! section is a clickable header row plus a body that shows or hides. Combines
//! the expand/collapse state of [`crate::widgets::tree_view::TreeView`] with a
//! flat list of sections (each carrying an arbitrary content [`Dom`]).
//!
//! Sections toggle independently (any number may be open at once). Clicking a
//! header flips that section's `is_open` flag in a per-header [`RefAny`] (the
//! self-contained per-row data pattern of `tree_view`), invokes the optional
//! user `on_toggle(section_index)`, and shows/hides the section body by setting
//! `display: block | none` on it via `set_css_property` (mirroring tree_view /
//! check_box live restyling).
//!
//! TODO2: the header is a plain styled clickable bar with no animated disclosure
//! chevron — a glyph cannot be re-textured via `set_css_property` without a
//! relayout, so an indicator that flips on toggle is deferred. The `display`
//! toggle itself follows the proven live-restyle pattern but the `display:none`
//! relayout is not GUI-verified in this build.
//!
//! Key types: [`Accordion`], [`AccordionSection`], [`AccordionOnToggle`].

use std::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec,
        TabIndex,
    },
    refany::{OptionRefAny, RefAny},
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, impl_vec_partialeq,
    props::{
        basic::{color::ColorU, font::{StyleFontFamily, StyleFontFamilyVec}, *},
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::callbacks::{Callback, CallbackInfo};

static ACCORDION_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-accordion"))];
static ACCORDION_SECTION_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-accordion-section",
))];
static ACCORDION_HEADER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-accordion-header",
))];
static ACCORDION_TITLE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-accordion-title",
))];
static ACCORDION_BODY_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-accordion-body"))];

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

/// Callback invoked when a section header is clicked. The `usize` is the
/// zero-based index of the toggled section.
pub type AccordionOnToggleCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    AccordionOnToggle,
    OptionAccordionOnToggle,
    AccordionOnToggleCallback,
    AccordionOnToggleCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        AccordionOnToggleCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: ACCORDION_ON_TOGGLE_INVOKER,
    invoker_ty:     AzAccordionOnToggleCallbackInvoker,
    thunk_fn:       az_accordion_on_toggle_callback_thunk,
    setter_fn:      AzApp_setAccordionOnToggleCallbackInvoker,
    from_handle_fn: AzAccordionOnToggleCallback_createFromHostHandle,
    extra_args:     [ section_index: usize ],
}

// ---- colours ----
const BORDER_COLOR: ColorU = ColorU { r: 222, g: 226, b: 230, a: 255 }; // #dee2e6
const HEADER_BG: ColorU = ColorU { r: 248, g: 249, b: 250, a: 255 }; // #f8f9fa
const TEXT_COLOR: ColorU = ColorU { r: 33, g: 37, b: 41, a: 255 }; // #212529

const HEADER_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(HEADER_BG)];
const HEADER_BG_VEC: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(HEADER_BG_ITEMS);

/// One collapsible section: a header title and an arbitrary content body.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct AccordionSection {
    /// The header text shown for this section.
    pub title: AzString,
    /// The body content revealed when the section is open.
    pub content: Dom,
    /// Whether this section starts open (body visible).
    pub is_open: bool,
}

impl AccordionSection {
    /// Creates a new collapsed section with the given title and content.
    pub fn new<S: Into<AzString>>(title: S, content: Dom) -> Self {
        Self {
            title: title.into(),
            content,
            is_open: false,
        }
    }

    /// Builder method: sets the initial open state.
    pub fn with_open(mut self, open: bool) -> Self {
        self.is_open = open;
        self
    }
}

impl_option!(AccordionSection, OptionAccordionSection, copy = false, [Debug, Clone, PartialEq]);
impl_vec!(
    AccordionSection,
    AccordionSectionVec,
    AccordionSectionVecDestructor,
    AccordionSectionVecDestructorType,
    AccordionSectionVecSlice,
    OptionAccordionSection
);
impl_vec_clone!(AccordionSection, AccordionSectionVec, AccordionSectionVecDestructor);
impl_vec_debug!(AccordionSection, AccordionSectionVec);
impl_vec_partialeq!(AccordionSection, AccordionSectionVec);
impl_vec_mut!(AccordionSection, AccordionSectionVec);

/// A vertical stack of collapsible titled sections.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct Accordion {
    /// The sections, in display order.
    pub sections: AccordionSectionVec,
    /// Optional callback fired when any section header is toggled.
    pub on_toggle: OptionAccordionOnToggle,
}

// ---- styles ----

static ACCORDION_CONTAINER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: TEXT_COLOR,
    })),
    // border: 1px solid #dee2e6
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: BORDER_COLOR,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: BORDER_COLOR,
        },
    )),
    // rounded corners, clipping the per-section separators
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
];

static ACCORDION_SECTION_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    // a thin separator between stacked sections
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: BORDER_COLOR,
        },
    )),
];

static ACCORDION_HEADER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        10,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(10),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        12,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(12),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(HEADER_BG_VEC)),
];

static ACCORDION_TITLE_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Left)),
];

/// Body style when the section is OPEN: a padded block with a top separator.
static ACCORDION_BODY_STYLE_OPEN: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        12,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(12),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        12,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(12),
    )),
];

/// Body style when the section is CLOSED: not laid out at all.
static ACCORDION_BODY_STYLE_CLOSED: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::None)),
];

impl Accordion {
    /// Creates a new accordion from the given sections, with no toggle callback.
    pub fn new(sections: AccordionSectionVec) -> Self {
        Self {
            sections,
            on_toggle: None.into(),
        }
    }

    /// Creates an empty accordion.
    pub fn create() -> Self {
        Self::new(AccordionSectionVec::from_const_slice(&[]))
    }

    /// Sets the callback invoked when any section header is toggled.
    pub fn set_on_toggle<C: Into<AccordionOnToggleCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_toggle = Some(AccordionOnToggle {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// Builder method: sets the toggle callback.
    pub fn with_on_toggle<C: Into<AccordionOnToggleCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_toggle(data, callback);
        self
    }

    /// Replaces `self` with an empty default accordion and returns the original.
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the accordion into a [`Dom`] subtree.
    pub fn dom(self) -> Dom {
        let on_toggle = self.on_toggle;
        let sections = self.sections;

        let mut section_doms: Vec<Dom> = Vec::with_capacity(sections.as_ref().len());

        for (index, section) in sections.as_ref().iter().enumerate() {
            let title = Dom::create_text(section.title.clone())
                .with_ids_and_classes(IdOrClassVec::from_const_slice(ACCORDION_TITLE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                    ACCORDION_TITLE_STYLE,
                ));

            // Per-header self-contained click data (mirrors tree_view's NodeClickData).
            let header_data = HeaderClickData {
                index,
                is_open: section.is_open,
                on_toggle: clone_option_on_toggle(&on_toggle),
            };

            let header = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(ACCORDION_HEADER_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                    ACCORDION_HEADER_STYLE,
                ))
                .with_tab_index(TabIndex::Auto)
                .with_callbacks(
                    alloc::vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseUp),
                        callback: CoreCallback {
                            cb: on_accordion_header_click as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: RefAny::new(header_data),
                    }]
                    .into(),
                )
                .with_children(DomVec::from_vec(alloc::vec![title]));

            let body_style = if section.is_open {
                ACCORDION_BODY_STYLE_OPEN
            } else {
                ACCORDION_BODY_STYLE_CLOSED
            };
            let body = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(ACCORDION_BODY_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(body_style))
                .with_children(DomVec::from_vec(alloc::vec![section.content.clone()]));

            section_doms.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(ACCORDION_SECTION_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                        ACCORDION_SECTION_STYLE,
                    ))
                    .with_children(DomVec::from_vec(alloc::vec![header, body])),
            );
        }

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(ACCORDION_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                ACCORDION_CONTAINER_STYLE,
            ))
            .with_children(DomVec::from_vec(section_doms))
    }
}

impl Default for Accordion {
    fn default() -> Self {
        Self::create()
    }
}

/// Clones an `OptionAccordionOnToggle` (the callback wrapper is not `Copy`).
fn clone_option_on_toggle(opt: &OptionAccordionOnToggle) -> OptionAccordionOnToggle {
    match opt.as_ref() {
        Some(AccordionOnToggle { callback, refany }) => Some(AccordionOnToggle {
            callback: callback.clone(),
            refany: refany.clone(),
        })
        .into(),
        None => None.into(),
    }
}

/// Per-header callback payload (kept internal, like `tree_view::NodeClickData`).
struct HeaderClickData {
    index: usize,
    is_open: bool,
    on_toggle: OptionAccordionOnToggle,
}

/// Header click handler. The hit node is the header (the callback-bearing node,
/// per `currentTarget` semantics — see `radio_group`); its next sibling is the
/// body. Flips this section's `is_open`, invokes the optional user callback with
/// the section index, then shows/hides the body via `display`.
extern "C" fn on_accordion_header_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let header = info.get_hit_node();
    let body = match info.get_next_sibling(header) {
        Some(b) => b,
        None => return Update::DoNothing,
    };

    let (now_open, result) = {
        let mut hd = match data.downcast_mut::<HeaderClickData>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };
        hd.is_open = !hd.is_open;
        let now_open = hd.is_open;
        let index = hd.index;
        let result = match hd.on_toggle.as_mut() {
            Some(AccordionOnToggle { callback, refany }) => {
                (callback.cb)(refany.clone(), info, index)
            }
            None => Update::DoNothing,
        };
        (now_open, result)
    };

    let display = if now_open {
        LayoutDisplay::Block
    } else {
        LayoutDisplay::None
    };
    info.set_css_property(body, CssProperty::const_display(display));

    result
}

impl From<Accordion> for Dom {
    fn from(a: Accordion) -> Dom {
        a.dom()
    }
}
