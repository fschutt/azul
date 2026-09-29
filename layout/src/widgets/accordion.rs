//! Accordion / expander widget — one or more collapsible titled sections. Each
//! section is a clickable header row plus a body that shows or hides. Combines
//! the expand/collapse state of [`crate::widgets::tree_view::TreeView`] with a
//! flat list of sections (each carrying an arbitrary content [`Dom`]).
//!
//! Sections toggle independently (any number may be open at once). Clicking a
//! header flips that section's `is_open` flag in a per-header [`RefAny`] (the
//! self-contained per-row data pattern of `tree_view`), invokes the optional
//! user `on_toggle(section_index)`, and opens or closes the section body with a
//! height tween: a closed body is laid out at `height: 0` (clipped), and the
//! click writes its new height and vertical padding through `set_css_property`,
//! which the body's declared `animation` turns into a transition - the
//! mechanism that slides the switch's knob. With reduced motion the body
//! declares no animation and snaps.
//!
//! TODO2: the header is a plain styled clickable bar with no animated disclosure
//! chevron — a glyph cannot be re-textured via `set_css_property` without a
//! relayout, so an indicator that flips on toggle is deferred.
//!
//! Key types: [`Accordion`], [`AccordionSection`], [`AccordionOnToggle`].

use std::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomNodeId, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class,
        IdOrClassVec, TabIndex,
    },
    refany::{OptionRefAny, RefAny},
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq,
    props::{
        basic::{
            color::ColorU,
            font::{StyleFontFamily, StyleFontFamilyVec},
            StyleFontSize,
        },
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow, LayoutHeight,
            LayoutMinHeight, LayoutOverflow, LayoutPaddingBottom, LayoutPaddingLeft,
            LayoutPaddingRight, LayoutPaddingTop,
        },
        property::{CssProperty, CssPropertyType},
        style::{
            BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, StyleBackgroundContent, StyleBackgroundContentVec,
            StyleBorderBottomColor, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius,
            StyleBorderBottomStyle, StyleBorderLeftColor, StyleBorderLeftStyle,
            StyleBorderRightColor, StyleBorderRightStyle, StyleBorderTopColor,
            StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderTopStyle, StyleCursor,
            StyleTextAlign, StyleTextColor, StyleUserSelect,
        },
    },
    AzString,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::themes::system_palette,
};

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
static ACCORDION_BODY_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-accordion-body",
))];

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
    from_handle_byref_fn: AzAccordionOnToggleCallback_createFromHostHandleByref,
    extra_args:     [ section_index: usize ],
}

// ---- colours ----
const BORDER_COLOR: ColorU = ColorU {
    r: 222,
    g: 226,
    b: 230,
    a: 255,
}; // #dee2e6
const HEADER_BG: ColorU = ColorU {
    r: 248,
    g: 249,
    b: 250,
    a: 255,
}; // #f8f9fa
const TEXT_COLOR: ColorU = ColorU {
    r: 33,
    g: 37,
    b: 41,
    a: 255,
}; // #212529

const HEADER_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(HEADER_BG)];
const HEADER_BG_VEC: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(HEADER_BG_ITEMS);

/// One collapsible section: a header title and an arbitrary content body.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    #[must_use]
    pub const fn with_open(mut self, open: bool) -> Self {
        self.is_open = open;
        self
    }
}

impl_option!(
    AccordionSection,
    OptionAccordionSection,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);
impl_vec!(
    AccordionSection,
    AccordionSectionVec,
    AccordionSectionVecDestructor,
    AccordionSectionVecDestructorType,
    AccordionSectionVecSlice,
    OptionAccordionSection
);
impl_vec_clone!(
    AccordionSection,
    AccordionSectionVec,
    AccordionSectionVecDestructor
);
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
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme (`AppConfig::with_theme`,
    /// `CallbackInfo::set_theme`; flat unless the app chose another).
    pub theme: crate::widgets::themes::OptionUiTheme,
}

/// What a theme decides about an accordion: the style of each part. [`build`]
/// assembles them with the section bodies' own open / closed geometry; built
/// by `themes::flat::accordion` and `themes::flora::accordion`.
pub(crate) struct AccordionLook {
    /// The panel around every section.
    pub container: Vec<CssPropertyWithConditions>,
    /// One section (its separator from the next).
    pub section: Vec<CssPropertyWithConditions>,
    /// A section's header bar - a keyboard stop: its focus ring included.
    pub header: Vec<CssPropertyWithConditions>,
    /// The title inside a header.
    pub title: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the panel, if it has one.
    pub marker: Option<&'static str>,
}

// ---- styles ----

pub(crate) static ACCORDION_CONTAINER_STYLE: &[CssPropertyWithConditions] = &[
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
    // Dark theme: the titles and every section body inherit this, so the
    // application content inside the accordion follows the theme too.
    system_palette::DARK_TEXT,
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
    system_palette::DARK_SEPARATOR_BORDER_TOP,
    system_palette::DARK_SEPARATOR_BORDER_BOTTOM,
    system_palette::DARK_SEPARATOR_BORDER_LEFT,
    system_palette::DARK_SEPARATOR_BORDER_RIGHT,
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

pub(crate) static ACCORDION_SECTION_STYLE: &[CssPropertyWithConditions] = &[
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
    system_palette::DARK_SEPARATOR_BORDER_BOTTOM,
];

pub(crate) static ACCORDION_HEADER_STYLE: &[CssPropertyWithConditions] = &[
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
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(12),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(12),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(HEADER_BG_VEC)),
    // Dark theme: the header bar is part of the panel, not a light strip.
    system_palette::DARK_WINDOW_BACKGROUND,
];

pub(crate) static ACCORDION_TITLE_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Left)),
];

/// An open body's padding, on every side; a closed body keeps it on the
/// left and right only.
const BODY_PADDING: isize = 12;

/// How long a section takes to open or close.
const BODY_TWEEN_MS: u32 = 220;

/// What a body declares so opening and closing TWEEN instead of snapping: its
/// `height` and its vertical padding, the properties the click handler
/// writes - the same mechanism that slides the switch's knob (an imperative
/// write honours a declared `animation`).
///
/// Declared only under `prefers-reduced-motion: no-preference`: with reduced
/// motion the body has no animation, nothing is seeded, and a section opens
/// and closes at once (the handler asks the same question,
/// `body_animates`).
fn body_animation() -> CssPropertyWithConditions {
    use azul_css::{
        dynamic_selector::{BoolCondition, DynamicSelector},
        props::basic::{
            animation::{AnimationIterationCount, AnimationTiming, StyleAnimation, StyleAnimationVec},
            time::CssDuration,
        },
    };
    let tween = |property: &'static str| StyleAnimation {
        name: AzString::from_const_str(property),
        duration: CssDuration::from_millis(BODY_TWEEN_MS),
        delay: CssDuration::from_millis(0),
        iterations: AnimationIterationCount::Count(1),
        timing: AnimationTiming::EaseInOut,
        clip: true,
    };
    CssPropertyWithConditions::with_condition(
        CssProperty::Animation(azul_css::props::property::StyleAnimationVecValue::Exact(
            StyleAnimationVec::from_vec(alloc::vec![
                tween("height"),
                tween("padding-top"),
                tween("padding-bottom"),
            ]),
        )),
        DynamicSelector::PrefersReducedMotion(BoolCondition::False),
    )
}

/// A section's body: a block formatting context that CLIPS its content,
/// collapsed to zero height when closed.
///
/// A closed body is laid out - `height: 0` and no vertical padding, not
/// `display: none` - so the height its content needs is known the moment
/// its header is clicked, and the click can tween the body from 0 to it
/// (`on_accordion_header_click`). `display: none` was a discrete switch with
/// nothing between its two values: the section snapped open and shut.
///
/// `flow-root` keeps the content's margins inside the body in both states
/// (a zero-padding block would let them collapse through its edges), and
/// `overflow: clip` hides what does not fit yet without making the body a
/// scroll container. `min-height: 0` lets a flex column shrink it below its
/// content.
///
/// Both states declare the same properties apart from `height` and the
/// vertical padding - the ones the click handler writes - so a body that
/// reached a state by click looks like one built in it.
fn body_style(open: bool) -> CssPropertyWithConditionsVec {
    let vertical_padding = if open { BODY_PADDING } else { 0 };
    let mut style = alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::FlowRoot)),
        CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Clip)),
        CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Clip)),
        CssPropertyWithConditions::simple(CssProperty::const_min_height(
            LayoutMinHeight::const_px(0),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(
            LayoutPaddingTop::const_px(vertical_padding),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
            LayoutPaddingBottom::const_px(vertical_padding),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_left(
            LayoutPaddingLeft::const_px(BODY_PADDING),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_right(
            LayoutPaddingRight::const_px(BODY_PADDING),
        )),
        body_animation(),
    ];
    if !open {
        style.push(CssPropertyWithConditions::simple(CssProperty::const_height(
            LayoutHeight::const_px(0),
        )));
    }
    CssPropertyWithConditionsVec::from_vec(style)
}

impl Accordion {
    /// Creates a new accordion from the given sections, with no toggle callback.
    #[must_use]
    pub fn new(sections: AccordionSectionVec) -> Self {
        Self {
            sections,
            on_toggle: None.into(),
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// Pin the widget theme: the accordion keeps this look whatever the app
    /// theme is. Unset (`None`), it follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Creates an empty accordion.
    #[must_use]
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
    #[must_use]
    pub fn with_on_toggle<C: Into<AccordionOnToggleCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_toggle(data, callback);
        self
    }

    /// Replaces `self` with an empty default accordion and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the accordion into a [`Dom`] subtree. The look comes from the
    /// theme module (`themes::flat::accordion` / `themes::flora::accordion`);
    /// `None` carries both
    /// looks, each in its `@theme(<name>)` block, and the app theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::accordion(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::accordion(self),
            // No theme: follow the app theme - both looks in one DOM, each
            // inside its `@theme(<name>)` block, and the app theme picks.
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                self,
                crate::widgets::themes::flat::accordion,
                crate::widgets::themes::flora::accordion,
            ),
        }
    }
}

/// The accordion's DOM in `look`: per section a header (the keyboard stop and
/// click target) over a body whose open / closed geometry is the widget's own
/// (`body_style`), so the header's click handler can tween it in any theme.
pub(crate) fn build(accordion: Accordion, look: &AccordionLook) -> Dom {
    {
        let on_toggle = accordion.on_toggle;
        let sections = accordion.sections;

        let mut section_doms: Vec<Dom> = Vec::with_capacity(sections.as_ref().len());

        for (index, section) in sections.as_ref().iter().enumerate() {
            let title = crate::widgets::widget_p_with_text(section.title.clone())
                .with_ids_and_classes(IdOrClassVec::from_const_slice(ACCORDION_TITLE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(look.title.clone()));

            // Read the open state before it is moved into the click data.
            let section_is_open = section.is_open;

            // Per-header self-contained click data (mirrors tree_view's NodeClickData).
            let header_data = HeaderClickData {
                index,
                is_open: section.is_open,
                on_toggle: clone_option_on_toggle(&on_toggle),
            };

            let header = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(ACCORDION_HEADER_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(look.header.clone()))
                .with_tab_index(TabIndex::Auto)
                // A section header must report whether it is open. Expanded /
                // Collapsed is the difference between "Details" and "Details,
                // collapsed, activate to expand" — without it the header reads
                // identically in both states and the control appears inert.
                .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                    role: azul_core::a11y::AccessibilityRole::OutlineItem,
                    states: azul_core::a11y::AccessibilityStateVec::from_vec(vec![
                        if section_is_open {
                            azul_core::a11y::AccessibilityState::Expanded
                        } else {
                            azul_core::a11y::AccessibilityState::Collapsed
                        },
                    ]),
                    ..Default::default()
                })
                .with_callbacks(
                    alloc::vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::Click),
                        callback: CoreCallback {
                            cb: on_accordion_header_click as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: RefAny::new(header_data),
                    }]
                    .into(),
                )
                .with_children(DomVec::from_vec(alloc::vec![title]));

            let body = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(ACCORDION_BODY_CLASS))
                .with_css_props(body_style(section.is_open))
                .with_children(DomVec::from_vec(alloc::vec![section.content.clone()]));

            section_doms.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(ACCORDION_SECTION_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(look.section.clone()))
                    .with_children(DomVec::from_vec(alloc::vec![header, body])),
            );
        }

        let mut classes: Vec<IdOrClass> = ACCORDION_CLASS.to_vec();
        if let Some(marker) = look.marker {
            classes.push(Class(AzString::from_const_str(marker)));
        }

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(classes))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(look.container.clone()))
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
    let Some(body) = info.get_next_sibling(header) else {
        return Update::DoNothing;
    };
    // Read off the layout on screen, before anything changes: how tall the
    // body's content is (a closed body lays it out at zero height), and
    // whether the body tweens at all.
    let content_height = body_content_height(&info, body);
    let animated = body_animates(&info, body);

    let (now_open, result) = {
        let Some(mut hd) = data.downcast_mut::<HeaderClickData>() else {
            return Update::DoNothing;
        };
        hd.is_open = !hd.is_open;
        let now_open = hd.is_open;
        let index = hd.index;
        let result = match hd.on_toggle.as_mut() {
            Some(AccordionOnToggle { callback, refany }) => {
                callback.invoke(refany.clone(), info, index)
            }
            None => Update::DoNothing,
        };
        (now_open, result)
    };

    // WHO OWNS THE VISUAL STATE decides what we write here.
    //
    // `set_css_property` does not just restyle this frame: it records a USER
    // OVERRIDE that `migrate_user_overrides_from` copies onto the matched node
    // of every later rebuild, and an override outranks the freshly cascaded
    // inline style. So if the host rebuilds the accordion from its own flag,
    // an override left behind here wins forever — the section that was clicked
    // open could never close again, and vice versa. That latch, not the toggle
    // itself, is what made the accordion "not properly expand/collapse".
    //
    // - The body tweens (`animated`): write the target state. Each write seeds a transition
    //   (the body declares an `animation` for exactly these properties), and a transition that
    //   settles REMOVES its override: a host that rebuilds ends up with its rebuilt DOM's own
    //   style, a widget that owns its flag with the written inline values. Opening writes
    //   `height: auto` - the open state - and then, on the override channel only, the content
    //   height the tween walks to: an `auto` target does not interpolate. Closing starts from
    //   `auto`, which the engine resolves to the laid-out height.
    // - No tween (reduced motion) and the host rebuilds: it owns the flag. CLEAR the overrides
    //   (`initial` removes one) and let the rebuilt DOM's own style decide.
    // - No tween and nobody rebuilds: the widget owns the flag, so write the new state - that is
    //   what makes a self-contained accordion work with no host state.
    let host_rebuilds = matches!(result, Update::RefreshDom | Update::RefreshDomAllWindows);
    if let Some(body_node) = body.node.into_crate_internal() {
        let vertical_padding = |px: isize| {
            [
                CssProperty::const_padding_top(LayoutPaddingTop::const_px(px)),
                CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(px)),
            ]
        };
        if !animated && host_rebuilds {
            info.change_node_css_properties(
                body.dom,
                body_node,
                vec![
                    CssProperty::initial(CssPropertyType::Height),
                    CssProperty::initial(CssPropertyType::PaddingTop),
                    CssProperty::initial(CssPropertyType::PaddingBottom),
                ]
                .into(),
            );
        } else if now_open {
            let [top, bottom] = vertical_padding(BODY_PADDING);
            info.change_node_css_properties(
                body.dom,
                body_node,
                vec![CssProperty::const_height(LayoutHeight::Auto), top, bottom].into(),
            );
            if let (true, Some(height)) = (animated, content_height) {
                info.override_node_css_properties(
                    body.dom,
                    body_node,
                    vec![CssProperty::height(LayoutHeight::px(height))].into(),
                );
            }
        } else {
            let [top, bottom] = vertical_padding(0);
            // A body with nothing in it has no height to walk down from: no
            // transition is seeded for it, so nothing would remove the
            // written override, and a host's rebuild would inherit it for
            // good. Clear it instead - the rebuilt DOM says 0 anyway.
            let height = if host_rebuilds && content_height.is_none() {
                CssProperty::initial(CssPropertyType::Height)
            } else {
                CssProperty::const_height(LayoutHeight::const_px(0))
            };
            info.change_node_css_properties(
                body.dom,
                body_node,
                vec![height, top, bottom].into(),
            );
        }
    }

    // The header's ANNOUNCED state must follow the rendered one. This toggle
    // changes a css property and returns Update::DoNothing — no rebuild — so
    // the Expanded/Collapsed published when the DOM was built would be frozen
    // at whatever it was then. A sighted user sees the section close; a screen
    // reader would still say "expanded". Applying this marks the a11y tree
    // dirty, so the platform adapter re-reads the node.
    info.set_accessibility_state(
        info.get_hit_node(),
        azul_core::a11y::AccessibilityStateVec::from_vec(vec![if now_open {
            azul_core::a11y::AccessibilityState::Expanded
        } else {
            azul_core::a11y::AccessibilityState::Collapsed
        }]),
    );

    result
}

/// The height `body`'s content needs - what the body grows to when it opens -
/// read off the layout on screen. A closed body is laid out at zero height
/// with its content laid out inside it (`body_style`), so this is known before
/// the section ever opened. `None` without a layout (or with nothing inside).
fn body_content_height(info: &CallbackInfo, body: DomNodeId) -> Option<f32> {
    let node = body.node.into_crate_internal()?;
    let result = info.get_layout_window().get_layout_result(&body.dom)?;
    let index = *result.layout_tree.dom_to_layout.get(&node)?.first()?;
    let height = result.layout_tree.get_content_size(index).height;
    (height.is_finite() && height > 0.0).then_some(height)
}

/// Does `body` tween its height? Only while it declares the animation - which
/// it does unless the user asked for reduced motion (`body_animation`). Asked
/// through the same cascade the engine seeds transitions from.
fn body_animates(info: &CallbackInfo, body: DomNodeId) -> bool {
    matches!(
        info.get_computed_css_property(body, CssPropertyType::Animation),
        Some(CssProperty::Animation(value))
            if value.get_property().is_some_and(|anims| !anims.as_ref().is_empty())
    )
}

impl From<Accordion> for Dom {
    fn from(a: Accordion) -> Self {
        a.dom()
    }
}

#[cfg(all(test, feature = "std"))]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::system::SystemStyle;
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// True if `node` carries the CSS class `name`.
    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
    }

    /// The text of a text node, looking through the `<p>` block wrapper the
    /// label convention mandates (`p > text`).
    fn text_of(node: &Dom) -> Option<&str> {
        match node.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            NodeType::P => match node.children.as_ref() {
                [only] => match only.root.get_node_type() {
                    NodeType::Text(s) => Some(s.as_ref().as_str()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    /// The `display` value in a node's *inline* style, if it sets one.
    fn inline_display(node: &Dom) -> Option<LayoutDisplay> {
        crate::widgets::themes::theme_blocks::checks::live_inline(&node).iter()
            .find_map(|(p, _)| match p {
                CssProperty::Display(v) => v.get_property().copied(),
                _ => None,
            })
    }

    /// The `height` value in a node's *inline* style, if it sets one.
    fn inline_height(node: &Dom) -> Option<LayoutHeight> {
        crate::widgets::themes::theme_blocks::checks::live_inline(&node).iter()
            .find_map(|(p, _)| match p {
                CssProperty::Height(v) => v.get_property().cloned(),
                _ => None,
            })
    }

    /// `(header, body)` of the `n`-th section of a rendered accordion DOM.
    fn section_parts(dom: &Dom, n: usize) -> (&Dom, &Dom) {
        let section = &dom.children.as_ref()[n];
        assert!(has_class(section, "__azul-native-accordion-section"));
        let children = section.children.as_ref();
        assert_eq!(children.len(), 2, "a section is exactly [header, body]");
        (&children[0], &children[1])
    }

    /// A three-node styled DOM — `root(0)` with children `header(1)` and
    /// `body(2)` — i.e. the exact hierarchy `on_accordion_header_click` walks
    /// (`hit node` -> `next sibling`).
    fn header_body_dom() -> StyledDom {
        let styled = StyledDom::create_from_dom(
            Dom::create_div()
                .with_child(Dom::create_div())
                .with_child(Dom::create_div()),
        );
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            3,
            "fixture must flatten to exactly root/header/body"
        );
        styled
    }

    /// A `DomLayoutResult` with an *empty* layout tree: the click handler only
    /// walks `styled_dom.node_hierarchy`, so no real layout (and no font) is needed.
    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(DisplayList::default()),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// Invokes `on_accordion_header_click` against a `LayoutWindow` holding
    /// `styled` (or nothing at all, when `styled` is `None`), with `hit` as the
    /// hit node. Returns the `Update` plus every recorded `CallbackChange`.
    fn run_click(
        styled: Option<StyledDom>,
        hit: usize,
        data: RefAny,
    ) -> (Update, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        if let Some(sd) = styled {
            layout_window
                .layout_results
                .insert(DomId::ROOT_ID, layout_result(sd));
        }

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: core::cell::RefCell::new(OptionRefAny::None),
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(hit))),
            },
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let update = on_accordion_header_click(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (update, recorded)
    }

    /// Every `display` write recorded in the change log, as `(node index, display)`.
    fn display_writes(changes: &[CallbackChange]) -> Vec<(usize, LayoutDisplay)> {
        let mut out = Vec::new();
        for change in changes {
            if let CallbackChange::ChangeNodeCssProperties {
                node_id,
                properties,
                ..
            } = change
            {
                for p in properties.as_ref() {
                    if let CssProperty::Display(v) = p {
                        if let Some(d) = v.get_property() {
                            out.push((node_id.index(), *d));
                        }
                    }
                }
            }
        }
        out
    }

    /// Every concrete `height` write recorded in the change log (the full
    /// channel, not the override one), as `(node index, height)`.
    fn height_writes(changes: &[CallbackChange]) -> Vec<(usize, LayoutHeight)> {
        let mut out = Vec::new();
        for change in changes {
            if let CallbackChange::ChangeNodeCssProperties {
                node_id,
                properties,
                ..
            } = change
            {
                for p in properties.as_ref() {
                    if let CssProperty::Height(v) = p {
                        if let Some(h) = v.get_property() {
                            out.push((node_id.index(), h.clone()));
                        }
                    }
                }
            }
        }
        out
    }

    /// `is_open` of a `HeaderClickData` payload.
    fn payload_is_open(data: &mut RefAny) -> bool {
        data.downcast_ref::<HeaderClickData>()
            .expect("payload must still be a HeaderClickData")
            .is_open
    }

    /// Records the section indices it is invoked with; used as a user `on_toggle`.
    struct ToggleLog {
        calls: Vec<usize>,
    }

    extern "C" fn record_toggle(mut data: RefAny, _: CallbackInfo, index: usize) -> Update {
        if let Some(mut log) = data.downcast_mut::<ToggleLog>() {
            log.calls.push(index);
        }
        Update::RefreshDom
    }

    extern "C" fn toggle_do_nothing(_: RefAny, _: CallbackInfo, _: usize) -> Update {
        Update::DoNothing
    }

    fn toggle_cb(f: AccordionOnToggleCallbackType) -> AccordionOnToggleCallback {
        f.into()
    }

    // ------------------------------------------------------------------
    // AccordionSection::new / with_open  (constructor, invariants)
    // ------------------------------------------------------------------

    #[test]
    fn section_new_stores_args_and_starts_closed() {
        let content = Dom::create_div().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("body"),
        );
        let sec = AccordionSection::new("Title", content.clone());

        assert_eq!(sec.title.as_str(), "Title");
        assert_eq!(sec.content, content);
        assert!(!sec.is_open, "a fresh section must start collapsed");
    }

    #[test]
    fn section_new_survives_extreme_titles() {
        // empty, interior NUL, emoji + combining marks + RTL, and a 100k-char title
        let long = "ab".repeat(50_000);
        let cases: Vec<AzString> = alloc::vec![
            AzString::from(""),
            AzString::from("a\0b"),
            AzString::from("👨‍👩‍👧‍👦 e\u{0301}\u{0327} مرحبا שלום 🇩🇪"),
            AzString::from("\u{feff}\u{202e}rtl-override"),
            AzString::from(long.as_str()),
        ];

        for title in cases {
            let sec = AccordionSection::new(title.clone(), Dom::create_div());
            assert_eq!(sec.title.as_str(), title.as_str());
            assert!(!sec.is_open);

            // and the title survives the trip through the DOM unchanged
            let dom = Accordion::new(AccordionSectionVec::from_vec(alloc::vec![sec])).dom();
            let (header, _) = section_parts(&dom, 0);
            let title_node = &header.children.as_ref()[0];
            assert_eq!(text_of(title_node), Some(title.as_str()));
        }
    }

    #[test]
    fn section_with_open_sets_flag_without_touching_other_fields() {
        let content = Dom::create_text_do_not_use_without_block_level_wrapper("x");
        let base = AccordionSection::new("t", content.clone());

        let opened = base.clone().with_open(true);
        assert!(opened.is_open);
        assert_eq!(opened.title.as_str(), "t");
        assert_eq!(opened.content, content);

        // last write wins; applying the same value twice is idempotent
        assert!(!base.clone().with_open(true).with_open(false).is_open);
        assert!(base.clone().with_open(false).with_open(true).is_open);
        assert!(base.clone().with_open(true).with_open(true).is_open);
        assert!(!base.with_open(false).is_open);
    }

    // ------------------------------------------------------------------
    // Accordion::new / create / Default
    // ------------------------------------------------------------------

    #[test]
    fn accordion_new_preserves_section_count_and_has_no_callback() {
        for count in [0usize, 1, 3, 1000] {
            let mut sections = Vec::with_capacity(count);
            for i in 0..count {
                sections.push(
                    AccordionSection::new(alloc::format!("s{i}"), Dom::create_div())
                        .with_open(i % 2 == 0),
                );
            }
            let acc = Accordion::new(AccordionSectionVec::from_vec(sections));

            assert_eq!(acc.sections.len(), count);
            assert!(acc.on_toggle.is_none(), "Accordion::new sets no callback");
            for (i, s) in acc.sections.as_ref().iter().enumerate() {
                assert_eq!(s.title.as_str(), alloc::format!("s{i}"));
                assert_eq!(s.is_open, i % 2 == 0);
            }
        }
    }

    #[test]
    fn accordion_create_is_empty_and_equals_default() {
        let acc = Accordion::create();
        assert!(acc.sections.is_empty());
        assert!(acc.on_toggle.is_none());
        assert_eq!(acc, Accordion::default());
    }

    // ------------------------------------------------------------------
    // set_on_toggle / with_on_toggle / swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn set_on_toggle_last_call_wins() {
        let mut acc = Accordion::create();

        acc.set_on_toggle(RefAny::new(1u8), toggle_cb(toggle_do_nothing));
        assert!(acc.on_toggle.is_some());
        assert_eq!(
            acc.on_toggle.as_ref().unwrap().refany.get_type_id(),
            RefAny::new(1u8).get_type_id()
        );

        // a second call must *replace* (not append / leak / panic)
        acc.set_on_toggle(RefAny::new(9i64), toggle_cb(record_toggle));
        let set = acc.on_toggle.as_ref().expect("still Some");
        assert_eq!(set.refany.get_type_id(), RefAny::new(0i64).get_type_id());
        assert_eq!(set.callback, toggle_cb(record_toggle));
        assert_ne!(set.callback, toggle_cb(toggle_do_nothing));
    }

    #[test]
    fn with_on_toggle_matches_set_on_toggle() {
        let built = Accordion::create().with_on_toggle(RefAny::new(7u32), toggle_cb(record_toggle));

        let mut mutated = Accordion::create();
        mutated.set_on_toggle(RefAny::new(7u32), toggle_cb(record_toggle));

        assert!(built.on_toggle.is_some());
        assert_eq!(
            built.on_toggle.as_ref().unwrap().callback,
            mutated.on_toggle.as_ref().unwrap().callback
        );
        // the builder form must not disturb the sections
        assert!(built.sections.is_empty());
    }

    #[test]
    fn swap_with_default_moves_all_state_out() {
        let sections = AccordionSectionVec::from_vec(alloc::vec![
            AccordionSection::new("a", Dom::create_div()),
            AccordionSection::new("b", Dom::create_div()).with_open(true),
        ]);
        let mut acc =
            Accordion::new(sections).with_on_toggle(RefAny::new(5u8), toggle_cb(record_toggle));

        let original = acc.swap_with_default();

        assert_eq!(original.sections.len(), 2);
        assert!(original.on_toggle.is_some());
        assert!(original.sections.as_ref()[1].is_open);

        assert!(acc.sections.is_empty(), "self must be left empty");
        assert!(acc.on_toggle.is_none(), "self must lose the callback");
        assert_eq!(acc, Accordion::create());

        // swapping an already-empty accordion is a no-op, not a panic
        let second = acc.swap_with_default();
        assert_eq!(second, Accordion::create());
        assert_eq!(acc, Accordion::create());
    }

    // ------------------------------------------------------------------
    // Accordion::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_of_empty_accordion_has_no_children() {
        let dom = Accordion::create().dom();
        assert!(has_class(&dom, "__azul-native-accordion"));
        assert!(dom.children.as_ref().is_empty());
        assert_eq!(dom.estimated_total_children, 0);
    }

    #[test]
    fn dom_height_follows_is_open() {
        let acc = Accordion::new(AccordionSectionVec::from_vec(alloc::vec![
            AccordionSection::new(
                "closed",
                Dom::create_text_do_not_use_without_block_level_wrapper("c0")
            ),
            AccordionSection::new(
                "open",
                Dom::create_text_do_not_use_without_block_level_wrapper("c1")
            )
            .with_open(true),
        ]));
        let dom = acc.dom();
        assert_eq!(dom.children.as_ref().len(), 2);

        let (h0, b0) = section_parts(&dom, 0);
        let (h1, b1) = section_parts(&dom, 1);

        assert!(has_class(h0, "__azul-native-accordion-header"));
        assert!(has_class(b0, "__azul-native-accordion-body"));

        // Both bodies are laid out (a closed one must be measurable for its
        // opening tween); a closed body is collapsed to zero height, an open
        // one is as tall as its content.
        assert_eq!(inline_display(b0), Some(LayoutDisplay::FlowRoot));
        assert_eq!(inline_display(b1), Some(LayoutDisplay::FlowRoot));
        assert_eq!(inline_height(b0), Some(LayoutHeight::const_px(0)));
        assert_eq!(inline_height(b1), None);

        // the body wraps exactly the caller's content
        assert_eq!(text_of(&b0.children.as_ref()[0]), Some("c0"));
        assert_eq!(text_of(&b1.children.as_ref()[0]), Some("c1"));

        // the header is focusable and carries exactly one MouseUp callback
        for h in [h0, h1] {
            assert!(matches!(h.root.get_tab_index(), Some(TabIndex::Auto)));
            let cbs = h.root.get_callbacks();
            assert_eq!(cbs.len(), 1);
            assert_eq!(
                cbs.as_ref()[0].event,
                EventFilter::Hover(HoverEventFilter::Click)
            );
            assert_eq!(
                cbs.as_ref()[0].callback.cb,
                on_accordion_header_click as usize
            );
        }
    }

    #[test]
    fn dom_header_payload_carries_the_section_index_and_open_state() {
        let count = 64usize;
        let mut sections = Vec::with_capacity(count);
        for i in 0..count {
            sections.push(
                AccordionSection::new(alloc::format!("s{i}"), Dom::create_div())
                    .with_open(i % 3 == 0),
            );
        }
        let dom = Accordion::new(AccordionSectionVec::from_vec(sections)).dom();

        for i in 0..count {
            let (header, body) = section_parts(&dom, i);
            let mut payload = header.root.get_callbacks().as_ref()[0].refany.clone();
            let hd = payload
                .downcast_ref::<HeaderClickData>()
                .expect("header payload is a HeaderClickData");

            assert_eq!(hd.index, i, "each header must know its own section index");
            assert_eq!(hd.is_open, i % 3 == 0);
            assert!(hd.on_toggle.is_none(), "no user callback was set");
            assert_eq!(
                inline_height(body),
                if i % 3 == 0 {
                    None
                } else {
                    Some(LayoutHeight::const_px(0))
                }
            );
        }
    }

    #[test]
    fn a_body_declares_its_tween_only_without_reduced_motion() {
        use azul_css::dynamic_selector::{BoolCondition, DynamicSelector};

        for open in [false, true] {
            let style = body_style(open);
            let animations: Vec<&CssPropertyWithConditions> = style
                .as_ref()
                .iter()
                .filter(|p| matches!(p.property, CssProperty::Animation(_)))
                .collect();
            assert_eq!(animations.len(), 1, "open={open}: one animation declaration");
            assert_eq!(
                animations[0].apply_if.as_ref(),
                &[DynamicSelector::PrefersReducedMotion(BoolCondition::False)][..],
                "open={open}: the tween must be conditional on no reduced motion"
            );
            let CssProperty::Animation(value) = &animations[0].property else {
                unreachable!("filtered on Animation above");
            };
            let names: Vec<&str> = value
                .get_property()
                .expect("an exact animation list")
                .as_ref()
                .iter()
                .map(|a| a.name.as_str())
                .collect();
            assert_eq!(
                names,
                ["height", "padding-top", "padding-bottom"],
                "open={open}: the tween covers what the click handler writes"
            );
        }
    }

    #[test]
    fn dom_child_count_cache_stays_consistent() {
        // deeply nested content + many sections: `estimated_total_children` must
        // still equal the real descendant count, otherwise the compact-DOM arena
        // under-allocates and panics later.
        let mut deep = Dom::create_text_do_not_use_without_block_level_wrapper("leaf");
        for _ in 0..64 {
            deep = Dom::create_div().with_child(deep);
        }

        let sections = AccordionSectionVec::from_vec(alloc::vec![
            AccordionSection::new("deep", deep),
            AccordionSection::new("flat", Dom::create_div()).with_open(true),
            AccordionSection::new("", Dom::create_div()),
        ]);
        let dom = Accordion::new(sections).dom();

        assert_eq!(
            dom.estimated_total_children,
            dom.recompute_estimated_total_children(),
            "cached descendant count desynced from the real tree"
        );
    }

    #[test]
    fn from_accordion_for_dom_matches_dom() {
        // Only meaningful for a section-less accordion: every `dom()` call mints
        // fresh per-header `RefAny`s, and two distinct `RefAny`s never compare equal.
        assert_eq!(Dom::from(Accordion::create()), Accordion::create().dom());
    }

    #[test]
    fn dom_leaves_the_original_on_toggle_payload_alive() {
        let log = RefAny::new(ToggleLog { calls: Vec::new() });
        let mut kept = log.clone();

        let acc = Accordion::new(AccordionSectionVec::from_vec(alloc::vec![
            AccordionSection::new("a", Dom::create_div()),
            AccordionSection::new("b", Dom::create_div()),
        ]))
        .with_on_toggle(log, toggle_cb(record_toggle));

        let dom = acc.dom();

        // every header got its own clone of the callback...
        for i in 0..2 {
            let (header, _) = section_parts(&dom, i);
            let mut payload = header.root.get_callbacks().as_ref()[0].refany.clone();
            let hd = payload.downcast_ref::<HeaderClickData>().unwrap();
            assert!(hd.on_toggle.is_some());
        }

        // ...and the caller's handle to the shared payload is still valid (no free)
        assert!(kept.downcast_ref::<ToggleLog>().unwrap().calls.is_empty());
    }

    // ------------------------------------------------------------------
    // clone_option_on_toggle
    // ------------------------------------------------------------------

    #[test]
    fn clone_option_on_toggle_of_none_is_none() {
        let none: OptionAccordionOnToggle = None.into();
        assert!(clone_option_on_toggle(&none).is_none());
        // cloning the clone stays None
        assert!(clone_option_on_toggle(&clone_option_on_toggle(&none)).is_none());
    }

    #[test]
    fn clone_option_on_toggle_shares_the_payload() {
        let mut some: OptionAccordionOnToggle = Some(AccordionOnToggle {
            callback: toggle_cb(record_toggle),
            refany: RefAny::new(0usize),
        })
        .into();

        let mut cloned = clone_option_on_toggle(&some);
        let cloned_inner = cloned.as_mut().expect("clone of Some must be Some");
        assert_eq!(cloned_inner.callback, toggle_cb(record_toggle));

        // the RefAny is shared, not deep-copied: a write through the clone is
        // visible through the original.
        *cloned_inner
            .refany
            .downcast_mut::<usize>()
            .expect("payload type is preserved") = 42;

        let original_inner = some.as_mut().unwrap();
        assert_eq!(*original_inner.refany.downcast_ref::<usize>().unwrap(), 42);
    }

    // ------------------------------------------------------------------
    // on_accordion_header_click
    // ------------------------------------------------------------------

    #[test]
    fn header_click_without_any_layout_result_is_a_noop() {
        let mut data = RefAny::new(HeaderClickData {
            index: 0,
            is_open: false,
            on_toggle: None.into(),
        });

        let (update, changes) = run_click(None, 0, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "nothing may be restyled without a body");
        assert!(!payload_is_open(&mut data), "state must not flip");
    }

    #[test]
    fn header_click_without_next_sibling_does_not_flip_state() {
        // node 2 is the *last* child -> no next sibling -> early return, and
        // crucially `is_open` must NOT have been toggled.
        let mut data = RefAny::new(HeaderClickData {
            index: 3,
            is_open: true,
            on_toggle: None.into(),
        });

        let (update, changes) = run_click(Some(header_body_dom()), 2, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(payload_is_open(&mut data), "state must be untouched");
    }

    #[test]
    fn header_click_with_stale_hit_node_is_a_noop() {
        let mut data = RefAny::new(HeaderClickData {
            index: 0,
            is_open: false,
            on_toggle: None.into(),
        });

        // node 999 does not exist in the 3-node fixture
        let (update, changes) = run_click(Some(header_body_dom()), 999, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(!payload_is_open(&mut data));
    }

    #[test]
    fn header_click_with_foreign_payload_is_a_noop() {
        // the callback-bearing node carries a RefAny of the *wrong* type
        let data = RefAny::new(0xdead_beef_u64);

        let (update, changes) = run_click(Some(header_body_dom()), 1, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a foreign payload must not restyle the body"
        );
    }

    #[test]
    fn header_click_toggles_body_height_and_flips_state() {
        // The fixture's body declares no animation (and has no layout to
        // measure), so this is the reduced-motion path: the new state is
        // written at once. The tween itself is pinned end to end in
        // layout/tests/accordion_animation.rs.
        let mut data = RefAny::new(HeaderClickData {
            index: 0,
            is_open: false,
            on_toggle: None.into(),
        });

        // closed -> open: as tall as the content
        let (update, changes) = run_click(Some(header_body_dom()), 1, data.clone());
        assert_eq!(update, Update::DoNothing, "no user callback -> DoNothing");
        assert_eq!(
            height_writes(&changes),
            alloc::vec![(2usize, LayoutHeight::Auto)]
        );
        assert!(
            display_writes(&changes).is_empty(),
            "the body stays laid out"
        );
        assert!(payload_is_open(&mut data));

        // open -> closed (same payload, so the flip must be stateful)
        let (update, changes) = run_click(Some(header_body_dom()), 1, data.clone());
        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            height_writes(&changes),
            alloc::vec![(2usize, LayoutHeight::const_px(0))]
        );
        assert!(!payload_is_open(&mut data));
    }

    #[test]
    fn header_click_invokes_user_callback_and_propagates_its_update() {
        let mut log = RefAny::new(ToggleLog { calls: Vec::new() });
        let data = RefAny::new(HeaderClickData {
            index: 17,
            is_open: false,
            on_toggle: Some(AccordionOnToggle {
                callback: toggle_cb(record_toggle),
                refany: log.clone(),
            })
            .into(),
        });

        let (update, changes) = run_click(Some(header_body_dom()), 1, data.clone());

        // the user's return value wins over the internal DoNothing
        assert_eq!(update, Update::RefreshDom);
        // The host asked for a rebuild, so it owns the open flag. Without a
        // tween to settle it (the fixture's body declares none, as under
        // reduced motion) the widget must CLEAR its overrides instead of
        // writing values. A written override survives the rebuild
        // (`migrate_user_overrides_from`) and outranks the freshly cascaded
        // style, which latched the section open (or shut) forever — the
        // "accordion doesn't properly expand/collapse" bug. `initial` is what
        // removes an override (`restyle_user_property`).
        let writes: Vec<_> = changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeCssProperties {
                    node_id,
                    properties,
                    ..
                } => Some((
                    node_id.index(),
                    properties
                        .as_ref()
                        .iter()
                        .map(|p| p.get_type())
                        .collect::<Vec<_>>(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            writes,
            alloc::vec![(
                2usize,
                alloc::vec![
                    CssPropertyType::Height,
                    CssPropertyType::PaddingTop,
                    CssPropertyType::PaddingBottom,
                ]
            )],
            "a rebuild-requesting toggle must still address what the click writes",
        );
        assert!(
            height_writes(&changes).is_empty() && display_writes(&changes).is_empty(),
            "…but as `initial` (override cleared), never as a concrete value that would outrank \
             the rebuilt DOM: {:?}",
            height_writes(&changes),
        );
        assert_eq!(
            log.downcast_ref::<ToggleLog>().unwrap().calls.as_slice(),
            &[17],
            "the user callback must receive this section's index"
        );

        // a second click reports the same index again
        let (_, _) = run_click(Some(header_body_dom()), 1, data);
        assert_eq!(
            log.downcast_ref::<ToggleLog>().unwrap().calls.as_slice(),
            &[17, 17]
        );
    }
}

/// The theme option: which look an accordion renders in, and what each look
/// is.
#[cfg(test)]
mod theme_tests {
    use azul_css::{
        dynamic_selector::PseudoStateType,
        props::{basic::pixel::PixelValue, style::BoxShadowClipMode},
    };

    use super::*;
    use crate::widgets::{
        theme_probe,
        themes::{flora, OptionUiTheme, UiTheme},
    };

    fn accordion(theme: UiTheme) -> Dom {
        Accordion::new(AccordionSectionVec::from_vec(alloc::vec![
            AccordionSection::new("Open section", Dom::create_p_with_text("Body text"))
                .with_open(true),
            AccordionSection::new("Closed section", Dom::create_p_with_text("Body text")),
        ]))
        .with_theme(theme)
        .dom()
    }

    fn sections(dom: &Dom) -> &[Dom] {
        dom.children.as_ref()
    }

    fn header(section: &Dom) -> &Dom {
        &section.children.as_ref()[0]
    }

    fn body(section: &Dom) -> &Dom {
        &section.children.as_ref()[1]
    }

    fn declarations(node: &Dom) -> Vec<CssPropertyWithConditions> {
        crate::widgets::themes::theme_blocks::checks::live_inline(&node).iter()
            .map(|(p, c)| CssPropertyWithConditions {
                property: p.clone(),
                apply_if: c.clone(),
            })
            .collect()
    }

    /// The `(light, dark)` value `pick` finds among the declarations for
    /// exactly `state`.
    fn in_state<T>(
        node: &Dom,
        state: PseudoStateType,
        pick: impl Fn(&CssProperty) -> Option<T>,
    ) -> (Option<T>, Option<T>) {
        let mut light = None;
        let mut dark = None;
        for d in declarations(node) {
            if d.pseudo_state_conditions() != [state] {
                continue;
            }
            let Some(v) = pick(&d.property) else {
                continue;
            };
            if d.is_dark_twin() {
                dark = Some(v);
            } else {
                light = Some(v);
            }
        }
        (light, dark)
    }

    /// The `(light, dark)` value `pick` finds among the RESTING declarations
    /// (no pseudo-state) - what the node shows when nothing happens to it.
    /// `theme_probe::dark` would also return the `:hover` / `:active` /
    /// `:focus` dark twins, declared after the resting pair.
    fn at_rest<T>(node: &Dom, pick: impl Fn(&CssProperty) -> Option<T>) -> (Option<T>, Option<T>) {
        let mut light = None;
        let mut dark = None;
        for d in declarations(node) {
            if !d.pseudo_state_conditions().is_empty() {
                continue;
            }
            let Some(v) = pick(&d.property) else {
                continue;
            };
            if d.is_dark_twin() {
                dark = Some(v);
            } else {
                light = Some(v);
            }
        }
        (light, dark)
    }

    /// A shadow's colour, and whether it is drawn inside the box.
    fn shadow(p: &CssProperty) -> Option<(ColorU, bool)> {
        match p {
            CssProperty::BoxShadowTop(v)
            | CssProperty::BoxShadowRight(v)
            | CssProperty::BoxShadowBottom(v)
            | CssProperty::BoxShadowLeft(v) => v.get_property().map(|s| {
                let s = s.as_ref();
                (s.color, s.clip_mode == BoxShadowClipMode::Inset)
            }),
            _ => None,
        }
    }

    fn bg(p: &CssProperty) -> Option<Vec<StyleBackgroundContent>> {
        match p {
            CssProperty::BackgroundContent(v) => v.get_property().map(|v| v.as_ref().to_vec()),
            _ => None,
        }
    }

    fn ink(p: &CssProperty) -> Option<ColorU> {
        match p {
            CssProperty::TextColor(v) => v.get_property().map(|c| c.inner),
            _ => None,
        }
    }

    fn top_edge(p: &CssProperty) -> Option<ColorU> {
        match p {
            CssProperty::BorderTopColor(v) => v.get_property().map(|c| c.inner),
            _ => None,
        }
    }

    fn radius(p: &CssProperty) -> Option<PixelValue> {
        match p {
            CssProperty::BorderTopLeftRadius(v) => v.get_property().map(|r| r.inner),
            _ => None,
        }
    }

    fn last<T>(props: &[CssProperty], f: impl Fn(&CssProperty) -> Option<T>) -> Option<T> {
        props.iter().rev().find_map(f)
    }

    #[test]
    fn an_accordion_without_a_theme_renders_flat() {
        let plain = Accordion::create();
        assert_eq!(plain.theme, OptionUiTheme::None, "no opinion by default");
        assert_eq!(
            theme_probe::unconditional(&plain.clone().dom()),
            theme_probe::unconditional(&plain.with_theme(UiTheme::Flat).dom())
        );
    }

    #[test]
    fn set_theme_and_with_theme_record_the_same_theme() {
        let mut set = Accordion::create();
        set.set_theme(UiTheme::Flora);
        assert_eq!(set.theme, OptionUiTheme::Some(UiTheme::Flora));
        assert_eq!(Accordion::create().with_theme(UiTheme::Flora), set);
    }

    #[test]
    fn a_flat_header_rings_inside_its_panel_on_focus_and_lights_under_the_pointer() {
        let dom = accordion(UiTheme::Flat);
        for s in sections(&dom) {
            let (light, dark) = in_state(header(s), PseudoStateType::Focus, shadow);
            assert!(
                light.is_some_and(|(_, inset)| inset) && dark.is_some_and(|(_, inset)| inset),
                "a keyboard stop needs a ring, drawn inside: the panel clips its edges"
            );
            let (hl, hd) = in_state(header(s), PseudoStateType::Hover, bg);
            assert!(hl.is_some() && hd.is_some(), "no hover face, or none at night");
        }
    }

    #[test]
    fn a_flora_accordion_is_a_leaf_ruled_in_flora_s_hairline() {
        let dom = accordion(UiTheme::Flora);
        let rest = theme_probe::unconditional(&dom);
        assert_eq!(
            last(&rest, bg),
            Some(alloc::vec![StyleBackgroundContent::Color(flora::LIGHT_SUR)])
        );
        assert_eq!(last(&rest, top_edge), Some(flora::LIGHT_BD));
        assert_eq!(last(&rest, ink), Some(flora::LIGHT_INK));
        assert_eq!(last(&rest, radius), Some(PixelValue::const_px(3)));
        let dark = theme_probe::dark(&dom);
        assert_eq!(
            last(&dark, bg),
            Some(alloc::vec![StyleBackgroundContent::Color(flora::DARK_SUR)])
        );
        assert_eq!(last(&dark, top_edge), Some(flora::DARK_BD));
        assert_eq!(last(&dark, ink), Some(flora::DARK_INK));
    }

    #[test]
    fn a_flora_header_is_raised_paper_that_lifts_under_the_pointer() {
        let dom = accordion(UiTheme::Flora);
        for s in sections(&dom) {
            let h = header(s);
            assert_eq!(
                at_rest(h, bg),
                (
                    Some(alloc::vec![flora::RAISED_FACE_LIGHT]),
                    Some(alloc::vec![flora::RAISED_FACE_DARK])
                ),
                "at rest: flora.css's raised face, --fl-rT over --fl-rB, by day and night"
            );
            assert_eq!(
                in_state(h, PseudoStateType::Active, bg),
                (
                    Some(alloc::vec![flora::PRESSED_FACE_LIGHT]),
                    Some(alloc::vec![flora::PRESSED_FACE_DARK])
                ),
                "held: the pressed face, --fl-pT over --fl-pB"
            );
            assert_eq!(
                in_state(h, PseudoStateType::Hover, bg),
                (
                    Some(alloc::vec![flora::HOVER_FACE_LIGHT]),
                    Some(alloc::vec![flora::HOVER_FACE_DARK])
                )
            );
            assert_eq!(
                in_state(h, PseudoStateType::Hover, ink),
                (Some(flora::LIGHT_QT), Some(flora::DARK_QT)),
                "flora.css `.faq-question:hover`: the brass accent"
            );
            assert_eq!(
                in_state(h, PseudoStateType::Focus, shadow),
                (
                    Some((flora::LIGHT_ACC, true)),
                    Some((flora::DARK_GLOW, true))
                ),
                "flora's focus colour, inside the panel"
            );
        }
    }

    #[test]
    fn a_flora_accordion_keeps_the_headers_behaviour_and_the_bodies_geometry() {
        let flora = accordion(UiTheme::Flora);
        let flat = accordion(UiTheme::Flat);
        for (a, b) in sections(&flora).iter().zip(sections(&flat)) {
            assert!(header(a).root.get_tab_index().is_some(), "a keyboard stop");
            assert_eq!(header(a).root.get_callbacks().as_ref().len(), 1, "the toggle");
            assert_eq!(
                header(a).root.get_accessibility_info().map(|i| i.role),
                header(b).root.get_accessibility_info().map(|i| i.role)
            );
            assert_eq!(
                theme_probe::unconditional(body(a)),
                theme_probe::unconditional(body(b)),
                "the open / closed geometry the click handler tweens"
            );
        }
    }

    #[test]
    fn a_flora_accordion_carries_the_flora_theme_marker() {
        let dom = accordion(UiTheme::Flora);
        assert!(dom
            .root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == "__azul-theme-flora")));
    }
}

/// Following the app theme (`theme: None`): the DOM carries every widget
/// theme's `@theme(<name>)` block and renders the app theme's; a pinned
/// widget (`with_theme`) ignores the app theme (T2 migration, T1 report
/// section 4).
#[cfg(test)]
mod app_theme_tests {
    use super::*;
    use crate::widgets::themes::{theme_blocks::checks, UiTheme};

    fn accordion() -> Accordion {
        Accordion::new(AccordionSectionVec::from_vec(alloc::vec![
            AccordionSection::new("Open section", Dom::create_div()).with_open(true),
            AccordionSection::new("Closed section", Dom::create_div()),
        ]))
    }

    #[test]
    fn an_accordion_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "accordion",
            || accordion().dom(),
            |t: UiTheme| accordion().with_theme(t).dom(),
        );
    }
}
