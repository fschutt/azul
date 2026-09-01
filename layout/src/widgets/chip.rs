//! Chip / tag widget — a compact rounded "pill" holding a short label plus an
//! optional removable "x" affordance. A blend of
//! [`crate::widgets::badge::Badge`] (the coloured pill visual + [`ChipKind`]
//! colour variants) and [`crate::widgets::alert::Alert`] (the dismiss pattern:
//! a stateful close affordance that hides the widget on click).
//!
//! When made removable (`with_removable(true)` or `set_on_remove`), the chip
//! mirrors the stateful pattern of [`crate::widgets::alert::Alert`]: it carries a
//! [`ChipStateWrapper`] (`{ visible } + on_remove`) in a [`RefAny`] attached to
//! the "x" node. Clicking "x" flips `visible` to `false`, invokes the optional
//! user `on_remove`, and hides the whole chip by setting `display: none` on the
//! container via `set_css_property` (mirroring alert's live restyle). A
//! non-removable chip renders no "x" and carries no live callback — it is then
//! just a stateless styled pill (a near-clone of [`Badge`]).
//!
//! Key types: [`Chip`], [`ChipKind`], [`ChipState`], [`ChipOnRemove`],
//! [`ChipOnClick`].

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    impl_option_inner,
    props::{
        basic::{
            color::ColorU,
            font::{StyleFontFamily, StyleFontFamilyVec},
            StyleFontSize,
        },
        layout::{
            LayoutAlignItems, LayoutAlignSelf, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutMarginLeft, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight,
            LayoutPaddingTop,
        },
        property::{CssProperty, *},
        style::{
            StyleBackgroundContent, StyleBackgroundContentVec, StyleBorderBottomLeftRadius,
            StyleBorderBottomRightRadius, StyleBorderTopLeftRadius, StyleBorderTopRightRadius,
            StyleCursor, StyleTextAlign, StyleTextColor, StyleUserSelect,
        },
    },
    AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

static CHIP_CONTAINER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-chip"))];
static CHIP_LABEL_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-chip-label"))];
static CHIP_REMOVE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-chip-remove"))];

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

/// Callback function type invoked when a removable chip's "x" is clicked.
pub type ChipOnRemoveCallbackType = extern "C" fn(RefAny, CallbackInfo, ChipState) -> Update;
impl_widget_callback!(
    ChipOnRemove,
    OptionChipOnRemove,
    ChipOnRemoveCallback,
    ChipOnRemoveCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ChipOnRemoveCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: CHIP_ON_REMOVE_INVOKER,
    invoker_ty:     AzChipOnRemoveCallbackInvoker,
    thunk_fn:       az_chip_on_remove_callback_thunk,
    setter_fn:      AzApp_setChipOnRemoveCallbackInvoker,
    from_handle_fn: AzChipOnRemoveCallback_createFromHostHandle,
    extra_args:     [ state: ChipState ],
}

/// Callback function type invoked when the chip's label area is clicked.
pub type ChipOnClickCallbackType = extern "C" fn(RefAny, CallbackInfo, ChipState) -> Update;
impl_widget_callback!(
    ChipOnClick,
    OptionChipOnClick,
    ChipOnClickCallback,
    ChipOnClickCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ChipOnClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: CHIP_ON_CLICK_INVOKER,
    invoker_ty:     AzChipOnClickCallbackInvoker,
    thunk_fn:       az_chip_on_click_callback_thunk,
    setter_fn:      AzApp_setChipOnClickCallbackInvoker,
    from_handle_fn: AzChipOnClickCallback_createFromHostHandle,
    extra_args:     [ state: ChipState ],
}

/// The semantic colour variant of a [`Chip`] (mirrors `badge::BadgeKind`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub enum ChipKind {
    /// Neutral light-grey chip — the default.
    #[default]
    Default,
    /// Blue "primary" chip.
    Primary,
    /// Green "success" chip.
    Success,
    /// Red "danger" chip.
    Danger,
    /// Yellow "warning" chip (uses dark text).
    Warning,
    /// Cyan "info" chip (uses dark text).
    Info,
}

impl ChipKind {
    /// Returns the `(background, text)` colours for this chip kind.
    #[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
    const fn colors(&self) -> (ColorU, ColorU) {
        const WHITE: ColorU = ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        const DARK: ColorU = ColorU {
            r: 33,
            g: 37,
            b: 41,
            a: 255,
        };
        match self {
            // The default chip is a light neutral pill with dark text (the
            // common "tag" look), unlike Badge's solid grey.
            Self::Default => (
                ColorU {
                    r: 233,
                    g: 236,
                    b: 239,
                    a: 255,
                },
                DARK,
            ),
            Self::Primary => (
                ColorU {
                    r: 13,
                    g: 110,
                    b: 253,
                    a: 255,
                },
                WHITE,
            ),
            Self::Success => (
                ColorU {
                    r: 25,
                    g: 135,
                    b: 84,
                    a: 255,
                },
                WHITE,
            ),
            Self::Danger => (
                ColorU {
                    r: 220,
                    g: 53,
                    b: 69,
                    a: 255,
                },
                WHITE,
            ),
            Self::Warning => (
                ColorU {
                    r: 255,
                    g: 193,
                    b: 7,
                    a: 255,
                },
                DARK,
            ),
            Self::Info => (
                ColorU {
                    r: 13,
                    g: 202,
                    b: 240,
                    a: 255,
                },
                DARK,
            ),
        }
    }

    /// CSS class name for this chip kind (mirrors `BadgeKind::class_name`).
    #[must_use]
    pub const fn class_name(&self) -> &'static str {
        match self {
            Self::Default => "__azul-chip-default",
            Self::Primary => "__azul-chip-primary",
            Self::Success => "__azul-chip-success",
            Self::Danger => "__azul-chip-danger",
            Self::Warning => "__azul-chip-warning",
            Self::Info => "__azul-chip-info",
        }
    }
}

/// A compact rounded pill holding a label plus an optional removable "x".
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Chip {
    /// Runtime state (`visible`) plus the optional remove callback.
    pub chip_state: ChipStateWrapper,
    /// The text shown inside the pill.
    pub label: AzString,
    /// The colour variant.
    pub kind: ChipKind,
    /// Whether to render the "x" remove affordance (hides the chip on click).
    pub removable: bool,
    /// The computed inline style for the pill container.
    pub container_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ChipStateWrapper {
    /// Whether the chip is currently visible.
    pub inner: ChipState,
    /// Optional: function to call when the chip is removed.
    pub on_remove: OptionChipOnRemove,
    /// Optional: function to call when the chip's label area is clicked.
    pub on_click: OptionChipOnClick,
}

/// The visible/hidden state of a [`Chip`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ChipState {
    /// `true` (default) = shown, `false` = removed/hidden.
    pub visible: bool,
}

impl Default for ChipState {
    fn default() -> Self {
        Self { visible: true }
    }
}

/// Builds the pill container style for a given [`ChipKind`]. The colours are the
/// only kind-dependent properties, so the style is built at runtime per the
/// recipe's "runtime vec when param-dependent" path (see `badge::build_badge_style`).
fn build_chip_style(kind: ChipKind) -> CssPropertyWithConditionsVec {
    let (bg, text) = kind.colors();
    let bg_vec =
        StyleBackgroundContentVec::from_vec(alloc::vec![StyleBackgroundContent::Color(bg)]);
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Row,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        // Hug the content rather than stretch across a flex parent's cross axis.
        CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        // padding: 4px 10px
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(
            LayoutPaddingTop::const_px(4,)
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
            LayoutPaddingBottom::const_px(4),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_left(
            LayoutPaddingLeft::const_px(10),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_right(
            LayoutPaddingRight::const_px(10),
        )),
        // border-radius: 12px (pill)
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(12),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(12),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(12),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(12),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
        // Text colour is inherited by the label + "x" children.
        CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
            inner: text,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(bg_vec)),
    ])
}

/// Label style: left-aligned, hugs its content.
static CHIP_LABEL_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Left)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// "x" remove-affordance style: a small pointer-cursor box on the right.
static CHIP_REMOVE_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(
        6,
    ))),
];

impl Chip {
    /// Creates a new chip with the given label and the default (light-grey) kind.
    #[inline]
    #[must_use]
    pub fn create(label: AzString) -> Self {
        Self::with_kind(label, ChipKind::Default)
    }

    /// Creates a new chip with the given label and colour variant.
    #[inline]
    #[must_use]
    pub fn with_kind(label: AzString, kind: ChipKind) -> Self {
        Self {
            chip_state: ChipStateWrapper::default(),
            label,
            kind,
            removable: false,
            container_style: build_chip_style(kind),
        }
    }

    /// Sets the colour variant, recomputing the container style.
    #[inline]
    pub fn set_kind(&mut self, kind: ChipKind) {
        self.kind = kind;
        self.container_style = build_chip_style(kind);
    }

    /// Builder-style setter for the colour variant.
    #[inline]
    #[must_use]
    pub fn with_chip_kind(mut self, kind: ChipKind) -> Self {
        self.set_kind(kind);
        self
    }

    /// Sets whether the chip shows a "x" remove affordance.
    #[inline]
    pub const fn set_removable(&mut self, removable: bool) {
        self.removable = removable;
    }

    /// Builder-style setter for the removable flag.
    #[inline]
    #[must_use]
    pub const fn with_removable(mut self, removable: bool) -> Self {
        self.set_removable(removable);
        self
    }

    /// Sets the remove callback. Implies `removable = true` so the "x" is rendered.
    #[inline]
    pub fn set_on_remove<C: Into<ChipOnRemoveCallback>>(&mut self, data: RefAny, on_remove: C) {
        self.removable = true;
        self.chip_state.on_remove = Some(ChipOnRemove {
            callback: on_remove.into(),
            refany: data,
        })
        .into();
    }

    /// Builder-style setter for the remove callback (implies removable).
    #[inline]
    #[must_use]
    pub fn with_on_remove<C: Into<ChipOnRemoveCallback>>(
        mut self,
        data: RefAny,
        on_remove: C,
    ) -> Self {
        self.set_on_remove(data, on_remove);
        self
    }

    /// Sets the click callback, invoked when the chip's label area is clicked.
    #[inline]
    pub fn set_on_click<C: Into<ChipOnClickCallback>>(&mut self, data: RefAny, on_click: C) {
        self.chip_state.on_click = Some(ChipOnClick {
            callback: on_click.into(),
            refany: data,
        })
        .into();
    }

    /// Builder-style setter for the click callback.
    #[inline]
    #[must_use]
    pub fn with_on_click<C: Into<ChipOnClickCallback>>(
        mut self,
        data: RefAny,
        on_click: C,
    ) -> Self {
        self.set_on_click(data, on_click);
        self
    }

    /// Replaces `self` with an empty default chip and returns the original.
    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this chip into a DOM subtree with the `__azul-native-chip` class.
    #[inline]
    #[must_use]
    pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::CoreCallback,
            dom::{EventFilter, HoverEventFilter},
            refany::OptionRefAny,
        };

        let has_on_click = matches!(self.chip_state.on_click, OptionChipOnClick::Some(_));

        // The remove ("x") and the label-click callbacks share the same state
        // RefAny so both handlers observe the same ChipState.
        let state_ref = RefAny::new(self.chip_state);

        let mut label = crate::widgets::widget_p_with_text(self.label)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CHIP_LABEL_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                CHIP_LABEL_STYLE,
            ));

        // The click callback is attached to the LABEL node rather than the
        // pill container: a container-level MouseUp would also fire when the
        // remove "x" (a child of the container) is clicked, double-firing
        // alongside on_remove. Attaching per-child sidesteps that (same
        // wiring as list_view's row/column callbacks); clicks on the pill's
        // padding therefore do not trigger on_click.
        if has_on_click {
            // A clickable chip is a compact button.
            label = label
                .with_tab_index(TabIndex::Auto)
                .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                    role: azul_core::a11y::AccessibilityRole::PushButton,
                    ..Default::default()
                })
                .with_callbacks(
                    alloc::vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::Click),
                        callback: CoreCallback {
                            cb: default_on_chip_click as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: state_ref.clone(),
                    }]
                    .into(),
                );
        }

        let mut children = alloc::vec![label];

        if self.removable {
            let remove = crate::widgets::widget_p_with_text(AzString::from_const_str("\u{00D7}"))
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CHIP_REMOVE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(CHIP_REMOVE_STYLE))
                .with_tab_index(TabIndex::Auto)
                // The remove affordance is its own button, not part of the chip's label.
                .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                    role: azul_core::a11y::AccessibilityRole::PushButton,
                    ..Default::default()
                })
                .with_callbacks(
                    alloc::vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::Click),
                        callback: CoreCallback {
                            cb: default_on_chip_remove as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: state_ref,
                    }]
                    .into(),
                );
            children.push(remove);
        }

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CHIP_CONTAINER_CLASS))
            .with_css_props(self.container_style)
            .with_children(children.into())
    }
}

impl Default for Chip {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""))
    }
}

/// "x" click handler. The hit node is the "x" (the callback-bearing node, per
/// `currentTarget` semantics — see `radio_group`); its parent is the chip
/// container. Flips `visible` to `false`, invokes the optional user callback,
/// then hides the whole chip via `display: none`.
extern "C" fn default_on_chip_remove(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let remove_node = info.get_hit_node();
    let Some(container) = info.get_parent(remove_node) else {
        return Update::DoNothing;
    };

    let result = {
        let Some(mut chip) = data.downcast_mut::<ChipStateWrapper>() else {
            return Update::DoNothing;
        };
        chip.inner.visible = false;
        let inner = chip.inner;
        let chip = &mut *chip;
        match chip.on_remove.as_mut() {
            Some(ChipOnRemove { callback, refany }) => (callback.cb)(refany.clone(), info, inner),
            None => Update::DoNothing,
        }
    };

    // TODO2: hides the chip by toggling `display: none` via set_css_property.
    // This follows the proven live-restyle pattern of alert/check_box/radio_group
    // (which toggle display/opacity/background); the display:none relayout itself
    // is not GUI-verified in this build.
    info.set_css_property(container, CssProperty::const_display(LayoutDisplay::None));

    result
}

/// Label click handler. Invokes the optional user `on_click` with the current
/// [`ChipState`] (mirrors `default_on_chip_remove`, minus the state flip/hide).
extern "C" fn default_on_chip_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(mut chip) = data.downcast_mut::<ChipStateWrapper>() else {
        return Update::DoNothing;
    };
    let inner = chip.inner;
    let chip = &mut *chip;
    match chip.on_click.as_mut() {
        Some(ChipOnClick { callback, refany }) => (callback.cb)(refany.clone(), info, inner),
        None => Update::DoNothing,
    }
}

impl From<Chip> for Dom {
    fn from(c: Chip) -> Self {
        c.dom()
    }
}

#[cfg(test)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap, HashSet},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::{
        props::basic::{length::SizeMetric, pixel::PixelValue},
        system::SystemStyle,
    };
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

    /// Every variant of `ChipKind` — the complete input domain of `colors`,
    /// `class_name` and `build_chip_style`.
    const ALL_KINDS: [ChipKind; 6] = [
        ChipKind::Default,
        ChipKind::Primary,
        ChipKind::Success,
        ChipKind::Danger,
        ChipKind::Warning,
        ChipKind::Info,
    ];

    const WHITE: ColorU = ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    const DARK: ColorU = ColorU {
        r: 33,
        g: 37,
        b: 41,
        a: 255,
    };

    /// The declared properties of a style vec, in declaration order.
    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    /// The *kind* of every declared property, in order (ignores the values).
    fn property_types(
        v: &CssPropertyWithConditionsVec,
    ) -> Vec<core::mem::Discriminant<CssProperty>> {
        v.as_ref()
            .iter()
            .map(|p| core::mem::discriminant(&p.property))
            .collect()
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length — an
    /// `em`/`%` slipping into the pill geometry would resolve against the parent
    /// font/box instead of the intended fixed padding or radius.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "chip geometry must be absolute px, got {:?}",
            pv.metric
        );
        pv.number.get()
    }

    /// The four paddings in `(top, bottom, left, right)` order.
    fn padding_px(
        v: &CssPropertyWithConditionsVec,
    ) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
        let find = |f: &dyn Fn(&CssProperty) -> Option<f32>| {
            v.as_ref().iter().find_map(|p| f(&p.property))
        };
        (
            find(&|p| match p {
                CssProperty::PaddingTop(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::PaddingBottom(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::PaddingLeft(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::PaddingRight(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
        )
    }

    /// The four corner radii, in declaration order.
    fn radii_px(v: &CssPropertyWithConditionsVec) -> Vec<f32> {
        v.as_ref()
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::BorderTopLeftRadius(r) => r.get_property().map(|r| px(&r.inner)),
                CssProperty::BorderTopRightRadius(r) => r.get_property().map(|r| px(&r.inner)),
                CssProperty::BorderBottomLeftRadius(r) => r.get_property().map(|r| px(&r.inner)),
                CssProperty::BorderBottomRightRadius(r) => r.get_property().map(|r| px(&r.inner)),
                _ => None,
            })
            .collect()
    }

    fn font_size_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::FontSize(f) => f.get_property().map(|f| px(&f.inner)),
            _ => None,
        })
    }

    fn text_color(v: &CssPropertyWithConditionsVec) -> Option<ColorU> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::TextColor(c) => c.get_property().map(|c| c.inner),
            _ => None,
        })
    }

    /// The single background layer of a style vec, asserting there is exactly one
    /// and that it is a flat colour (a gradient would not be a `Color`).
    fn background_color(v: &CssPropertyWithConditionsVec) -> Option<ColorU> {
        let bg = v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::BackgroundContent(b) => b.get_property(),
            _ => None,
        })?;
        assert_eq!(
            bg.as_ref().len(),
            1,
            "a chip must declare exactly one background layer"
        );
        match &bg.as_ref()[0] {
            StyleBackgroundContent::Color(c) => Some(*c),
            other => panic!("chip background is not a flat colour: {other:?}"),
        }
    }

    /// Every `PixelValue` a style vec mentions (paddings, radii, font size).
    fn all_pixel_values(v: &CssPropertyWithConditionsVec) -> Vec<PixelValue> {
        v.as_ref()
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::PaddingTop(x) => x.get_property().map(|x| x.inner),
                CssProperty::PaddingBottom(x) => x.get_property().map(|x| x.inner),
                CssProperty::PaddingLeft(x) => x.get_property().map(|x| x.inner),
                CssProperty::PaddingRight(x) => x.get_property().map(|x| x.inner),
                CssProperty::BorderTopLeftRadius(r) => r.get_property().map(|r| r.inner),
                CssProperty::BorderTopRightRadius(r) => r.get_property().map(|r| r.inner),
                CssProperty::BorderBottomLeftRadius(r) => r.get_property().map(|r| r.inner),
                CssProperty::BorderBottomRightRadius(r) => r.get_property().map(|r| r.inner),
                CssProperty::FontSize(f) => f.get_property().map(|f| f.inner),
                _ => None,
            })
            .collect()
    }

    /// Perceived brightness (0..=255) of an sRGB colour, Rec.709 weights. Kept to
    /// plain `+`/`*` (no gamma expansion) so the readability assertions below stay
    /// exact and toolchain-independent.
    fn luma(c: ColorU) -> f32 {
        0.2126 * f32::from(c.r) + 0.7152 * f32::from(c.g) + 0.0722 * f32::from(c.b)
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

    /// The properties of a rendered node's *inline* style, in declaration order.
    fn inline_properties(node: &Dom) -> Vec<CssProperty> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// Adversarial chip labels: empty, whitespace, combining marks, ZWJ emoji,
    /// RTL, embedded NULs (`AzString` is length-based, so a NUL must not
    /// truncate), the remove glyph itself, and a string far longer than any
    /// plausible tag.
    fn adversarial_strings() -> Vec<String> {
        let mut v: Vec<String> = [
            "",
            "tag",
            " ",
            "e\u{0301}",                                   // e + combining acute
            "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}", // ZWJ family emoji
            "\u{5E9}\u{5DC}\u{5D5}\u{5DD}",                // RTL Hebrew
            "\0",                                          // a single NUL
            "a\0b",                                        // embedded NUL
            "\u{FFFD}\u{202E}\u{200B}",                    // replacement char, RTL override, ZWSP
            "\u{00D7}",                                    // the remove glyph as a label
            "line\nbreak\ttab",                            // control characters
            "-9223372036854775808",                        // i64::MIN as a "count"
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        v.push("x".repeat(100_000));
        v
    }

    fn remove_cb(f: ChipOnRemoveCallbackType) -> ChipOnRemoveCallback {
        f.into()
    }

    fn click_cb(f: ChipOnClickCallbackType) -> ChipOnClickCallback {
        f.into()
    }

    /// A `RefAny` payload recording every `ChipState` a user callback observes.
    struct StateLog {
        calls: Vec<bool>,
    }

    extern "C" fn record_remove(mut data: RefAny, _: CallbackInfo, state: ChipState) -> Update {
        if let Some(mut log) = data.downcast_mut::<StateLog>() {
            log.calls.push(state.visible);
        }
        Update::RefreshDom
    }

    extern "C" fn record_click(mut data: RefAny, _: CallbackInfo, state: ChipState) -> Update {
        if let Some(mut log) = data.downcast_mut::<StateLog>() {
            log.calls.push(state.visible);
            log.calls.push(state.visible); // keeps this body distinct from record_remove
        }
        Update::RefreshDom
    }

    extern "C" fn remove_do_nothing(_: RefAny, _: CallbackInfo, _: ChipState) -> Update {
        Update::DoNothing
    }

    extern "C" fn click_do_nothing(_: RefAny, _: CallbackInfo, state: ChipState) -> Update {
        // `state.visible` is read (and discarded) purely so this body cannot be
        // identical-code-folded onto `remove_do_nothing`; the tests below compare
        // callback function pointers for inequality.
        let _ = state.visible;
        Update::DoNothing
    }

    /// A payload whose callback tries to read the *same* `ChipStateWrapper`
    /// `RefAny` that the handler is currently holding a mutable borrow on.
    struct ReentrantProbe {
        /// A clone of the state `RefAny` the handler was invoked with.
        state: RefAny,
        /// `Some(visible)` if the re-entrant read succeeded, `None` if it was
        /// refused. Starts as `Some(true)` so "never ran" is distinguishable.
        saw_state: Option<bool>,
        calls: usize,
    }

    extern "C" fn probe_state_reentrantly(
        mut data: RefAny,
        _: CallbackInfo,
        _: ChipState,
    ) -> Update {
        if let Some(mut probe) = data.downcast_mut::<ReentrantProbe>() {
            probe.calls += 1;
            let mut state = probe.state.clone();
            probe.saw_state = state
                .downcast_ref::<ChipStateWrapper>()
                .map(|w| w.inner.visible);
        }
        Update::DoNothing
    }

    /// `visible` of a `ChipStateWrapper` payload.
    fn wrapper_visible(data: &mut RefAny) -> bool {
        data.downcast_ref::<ChipStateWrapper>()
            .expect("payload must still be a ChipStateWrapper")
            .inner
            .visible
    }

    /// The `visible` flags recorded by a `StateLog` payload.
    fn log_calls(data: &mut RefAny) -> Vec<bool> {
        data.downcast_ref::<StateLog>()
            .expect("payload must still be a StateLog")
            .calls
            .clone()
    }

    /// A `DomLayoutResult` with an *empty* layout tree: the chip handlers only
    /// walk `styled_dom.node_hierarchy`, so no real layout (and no font) is needed.
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

    /// Flat indices of a removable chip in depth-first pre-order:
    /// `0 container / 1 label <p> / 2 label text / 3 remove <p> / 4 remove text`.
    /// Both callbacks sit on the `<p>`s — a text node owns no rect and could
    /// never be hit-tested.
    const LABEL_NODE: usize = 1;
    const REMOVE_NODE: usize = 3;

    /// The flattened DOM of a removable chip — exactly the hierarchy
    /// `default_on_chip_remove` walks (hit node -> parent).
    fn removable_styled_dom() -> StyledDom {
        let chip = Chip::create(AzString::from("tag")).with_removable(true);
        let styled = StyledDom::create_from_dom(chip.dom());
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            5,
            "fixture must flatten to container / label <p> + text / remove <p> + text"
        );
        styled
    }

    /// Builds a `CallbackInfo` pointing at node `hit` of `styled` (or at a window
    /// with no layout result at all when `styled` is `None`), hands it to `f`, and
    /// returns `f`'s result plus every recorded `CallbackChange`.
    fn with_callback_info<R>(
        styled: Option<StyledDom>,
        hit: usize,
        f: impl FnOnce(CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
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
            ctx: OptionRefAny::None,
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

        let out = f(info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (out, recorded)
    }

    fn run_remove(
        styled: Option<StyledDom>,
        hit: usize,
        data: RefAny,
    ) -> (Update, Vec<CallbackChange>) {
        with_callback_info(styled, hit, move |info| default_on_chip_remove(data, info))
    }

    fn run_click(
        styled: Option<StyledDom>,
        hit: usize,
        data: RefAny,
    ) -> (Update, Vec<CallbackChange>) {
        with_callback_info(styled, hit, move |info| default_on_chip_click(data, info))
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

    /// A `ChipStateWrapper` wired to `record_remove`, plus the log it writes to.
    fn state_with_remove_log() -> (RefAny, RefAny) {
        let log = RefAny::new(StateLog { calls: Vec::new() });
        let state = RefAny::new(ChipStateWrapper {
            inner: ChipState { visible: true },
            on_remove: Some(ChipOnRemove {
                callback: remove_cb(record_remove),
                refany: log.clone(),
            })
            .into(),
            on_click: OptionChipOnClick::None,
        });
        (state, log)
    }

    /// A `ChipStateWrapper` wired to `record_click`, plus the log it writes to.
    fn state_with_click_log(visible: bool) -> (RefAny, RefAny) {
        let log = RefAny::new(StateLog { calls: Vec::new() });
        let state = RefAny::new(ChipStateWrapper {
            inner: ChipState { visible },
            on_remove: OptionChipOnRemove::None,
            on_click: Some(ChipOnClick {
                callback: click_cb(record_click),
                refany: log.clone(),
            })
            .into(),
        });
        (state, log)
    }

    // ------------------------------------------------------------------
    // ChipKind::colors  (getter)
    // ------------------------------------------------------------------

    #[test]
    fn colors_returns_the_documented_constants_for_every_kind() {
        let expected = [
            // NOTE: unlike `BadgeKind::Default` (solid grey + white text), the
            // default *chip* is the light neutral "tag" pill with dark text.
            (
                ChipKind::Default,
                ColorU {
                    r: 233,
                    g: 236,
                    b: 239,
                    a: 255,
                },
                DARK,
            ),
            (
                ChipKind::Primary,
                ColorU {
                    r: 13,
                    g: 110,
                    b: 253,
                    a: 255,
                },
                WHITE,
            ),
            (
                ChipKind::Success,
                ColorU {
                    r: 25,
                    g: 135,
                    b: 84,
                    a: 255,
                },
                WHITE,
            ),
            (
                ChipKind::Danger,
                ColorU {
                    r: 220,
                    g: 53,
                    b: 69,
                    a: 255,
                },
                WHITE,
            ),
            (
                ChipKind::Warning,
                ColorU {
                    r: 255,
                    g: 193,
                    b: 7,
                    a: 255,
                },
                DARK,
            ),
            (
                ChipKind::Info,
                ColorU {
                    r: 13,
                    g: 202,
                    b: 240,
                    a: 255,
                },
                DARK,
            ),
        ];
        for (kind, bg, text) in expected {
            assert_eq!(
                kind.colors(),
                (bg, text),
                "{kind:?}: wrong (background, text) pair"
            );
        }
        // The doc comments promise Default/Warning/Info are the dark-text kinds and
        // no others: a fourth dark-text kind sneaking in here is a regression.
        for kind in ALL_KINDS {
            let (_, text) = kind.colors();
            let dark_text = matches!(kind, ChipKind::Default | ChipKind::Warning | ChipKind::Info);
            assert_eq!(
                text == DARK,
                dark_text,
                "{kind:?}: text colour contradicts the documented variant"
            );
        }
    }

    #[test]
    fn colors_only_ever_returns_one_of_the_two_documented_text_colours() {
        for kind in ALL_KINDS {
            let (_, text) = kind.colors();
            assert!(
                text == WHITE || text == DARK,
                "{kind:?}: text colour {text:?} is neither the documented WHITE nor DARK"
            );
        }
    }

    #[test]
    fn colors_are_fully_opaque_on_every_kind() {
        // A non-opaque pill would let the page background bleed through and
        // silently destroy the contrast the kind was chosen for.
        for kind in ALL_KINDS {
            let (bg, text) = kind.colors();
            assert_eq!(bg.a, 255, "{kind:?}: translucent background {bg:?}");
            assert_eq!(text.a, 255, "{kind:?}: translucent text colour {text:?}");
            assert_ne!(bg, text, "{kind:?}: an invisible label is not a chip");
        }
    }

    #[test]
    fn colors_give_every_kind_a_distinguishable_background() {
        // Two kinds that render identically make the semantic variant useless.
        let mut seen = HashSet::new();
        for kind in ALL_KINDS {
            let (bg, _) = kind.colors();
            assert!(
                seen.insert((bg.r, bg.g, bg.b, bg.a)),
                "{kind:?}: duplicate background colour {bg:?}"
            );
        }
        assert_eq!(seen.len(), ALL_KINDS.len());
    }

    #[test]
    fn colors_pick_the_more_readable_of_the_two_text_colours() {
        // The only real invariant of `colors()`: the text must be legible on the
        // pill. For each kind the chosen text colour must be further from the
        // background (in perceived brightness) than the rejected alternative,
        // and light backgrounds must take the dark text.
        for kind in ALL_KINDS {
            let (bg, text) = kind.colors();
            let other = if text == WHITE { DARK } else { WHITE };

            let chosen = (luma(bg) - luma(text)).abs();
            let rejected = (luma(bg) - luma(other)).abs();
            assert!(
                chosen > rejected,
                "{kind:?}: text {text:?} (delta luma {chosen:.1}) is less readable on {bg:?} than \
                 {other:?} (delta luma {rejected:.1})"
            );
            assert!(
                chosen >= 60.0,
                "{kind:?}: text/background brightness gap {chosen:.1} is too low to read"
            );

            // Mid-grey split: a light pill must not carry white text.
            let light_bg = luma(bg) >= 128.0;
            assert_eq!(
                text == DARK,
                light_bg,
                "{kind:?}: bg luma {:.1} but text is {text:?}",
                luma(bg)
            );
        }
    }

    #[test]
    fn colors_is_pure_and_the_default_kind_is_the_neutral_tag() {
        assert_eq!(ChipKind::default(), ChipKind::Default);
        assert_eq!(ChipKind::default().colors(), ChipKind::Default.colors());
        // `colors()` takes `&self` on a `Copy` enum: repeated calls, and calls
        // through a copy, must be side-effect free and identical.
        for kind in ALL_KINDS {
            let copy = kind;
            assert_eq!(
                kind.colors(),
                kind.colors(),
                "{kind:?}: colors() is not pure"
            );
            assert_eq!(
                kind.colors(),
                copy.colors(),
                "{kind:?}: a copy disagrees with the original"
            );
        }
    }

    #[test]
    fn colors_is_const_evaluable() {
        const DEFAULT: (ColorU, ColorU) = ChipKind::Default.colors();
        assert_eq!(
            DEFAULT.0,
            ColorU {
                r: 233,
                g: 236,
                b: 239,
                a: 255
            }
        );
        assert_eq!(DEFAULT.1, DARK);
    }

    // ------------------------------------------------------------------
    // ChipKind::class_name  (getter)
    // ------------------------------------------------------------------

    #[test]
    fn class_name_returns_the_documented_string_for_every_kind() {
        assert_eq!(ChipKind::Default.class_name(), "__azul-chip-default");
        assert_eq!(ChipKind::Primary.class_name(), "__azul-chip-primary");
        assert_eq!(ChipKind::Success.class_name(), "__azul-chip-success");
        assert_eq!(ChipKind::Danger.class_name(), "__azul-chip-danger");
        assert_eq!(ChipKind::Warning.class_name(), "__azul-chip-warning");
        assert_eq!(ChipKind::Info.class_name(), "__azul-chip-info");
        assert_eq!(ChipKind::default().class_name(), "__azul-chip-default");
    }

    #[test]
    fn class_name_is_unique_per_kind_and_a_usable_css_identifier() {
        let mut seen = HashSet::new();
        for kind in ALL_KINDS {
            let name = kind.class_name();
            assert!(
                seen.insert(name),
                "{kind:?}: class name {name:?} collides with another kind"
            );
            assert!(!name.is_empty(), "{kind:?}: empty class name");
            assert!(
                name.starts_with("__azul-chip-"),
                "{kind:?}: unnamespaced class {name:?}"
            );
            assert!(name.is_ascii(), "{kind:?}: non-ASCII class name {name:?}");
            // A space, a dot or a `#` would silently split/re-target the selector.
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "{kind:?}: class name {name:?} contains a CSS-significant character"
            );
            // The returned `&'static str` must be stable across calls.
            assert_eq!(
                name.as_ptr(),
                kind.class_name().as_ptr(),
                "{kind:?}: class_name() is not a stable constant"
            );
        }
        assert_eq!(seen.len(), ALL_KINDS.len());
    }

    #[test]
    fn class_name_does_not_collide_with_the_widget_element_classes() {
        // `__azul-native-chip` is a *prefix* of `__azul-native-chip-label` and
        // `__azul-native-chip-remove`; the kind classes live in their own
        // `__azul-chip-` namespace and must not alias any of the three.
        let element_classes = [
            "__azul-native-chip",
            "__azul-native-chip-label",
            "__azul-native-chip-remove",
        ];
        for kind in ALL_KINDS {
            let name = kind.class_name();
            for element in element_classes {
                assert_ne!(
                    name, element,
                    "{kind:?}: kind class shadows the element class {element:?}"
                );
            }
        }
    }

    #[test]
    fn class_name_is_const_evaluable() {
        const DANGER: &str = ChipKind::Danger.class_name();
        assert_eq!(DANGER, "__azul-chip-danger");
    }

    // ------------------------------------------------------------------
    // build_chip_style
    // ------------------------------------------------------------------

    #[test]
    fn build_chip_style_emits_the_documented_pill_geometry() {
        for kind in ALL_KINDS {
            let style = build_chip_style(kind);
            assert_eq!(
                padding_px(&style),
                (Some(4.0), Some(4.0), Some(10.0), Some(10.0)),
                "{kind:?}: padding is not 4px 10px"
            );
            assert_eq!(
                radii_px(&style),
                vec![12.0, 12.0, 12.0, 12.0],
                "{kind:?}: all four corners must carry a 12px radius"
            );
            assert_eq!(
                font_size_px(&style),
                Some(13.0),
                "{kind:?}: wrong font size"
            );
        }
    }

    #[test]
    fn build_chip_style_radius_actually_rounds_the_chip_to_a_pill() {
        // The widget's premise: the corner radius must reach at least half the
        // content height (font + vertical padding), otherwise it renders as a
        // rounded rectangle rather than a pill.
        for kind in ALL_KINDS {
            let style = build_chip_style(kind);
            let (top, bottom, ..) = padding_px(&style);
            let height = font_size_px(&style).expect("a font size must be declared")
                + top.expect("padding-top")
                + bottom.expect("padding-bottom");
            for r in radii_px(&style) {
                assert!(
                    r * 2.0 >= height,
                    "{kind:?}: radius {r} does not reach half of the {height}px pill height"
                );
            }
        }
    }

    #[test]
    fn build_chip_style_is_a_row_flexbox_that_hugs_its_content() {
        for kind in ALL_KINDS {
            let props = properties(&build_chip_style(kind));
            let has = |p: &CssProperty| props.contains(p);

            assert!(
                has(&CssProperty::const_display(LayoutDisplay::Flex)),
                "{kind:?}: not a flex box"
            );
            assert!(
                has(&CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
                "{kind:?}: the label and the x must sit side by side"
            );
            assert!(
                has(&CssProperty::const_align_items(LayoutAlignItems::Center)),
                "{kind:?}: label and x not vertically centred"
            );
            // align-self: start + flex-grow: 0 — without both, the pill stretches
            // across a flex parent instead of hugging its label.
            assert!(
                has(&CssProperty::align_self(LayoutAlignSelf::Start)),
                "{kind:?}: chip stretches on the cross axis"
            );
            assert!(
                has(&CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
                "{kind:?}: chip grows on the main axis"
            );
        }
    }

    #[test]
    fn build_chip_style_leaves_text_alignment_to_the_label() {
        // Unlike `badge`, the chip container declares neither `text-align` nor
        // `justify-content` — the label child owns `text-align: left`. A
        // container-level declaration here would fight the label's own style.
        for kind in ALL_KINDS {
            for p in build_chip_style(kind).as_ref() {
                assert!(
                    !matches!(p.property, CssProperty::TextAlign(_)),
                    "{kind:?}: the container must not declare text-align"
                );
                assert!(
                    !matches!(p.property, CssProperty::JustifyContent(_)),
                    "{kind:?}: the container must not declare justify-content"
                );
            }
        }
    }

    #[test]
    fn build_chip_style_declares_the_inheritable_text_style_once() {
        // The label and the "x" carry no colour/family of their own — both are
        // inherited from the container, so the container must declare them.
        for kind in ALL_KINDS {
            let style = build_chip_style(kind);
            let families: Vec<_> = style
                .as_ref()
                .iter()
                .filter_map(|p| match &p.property {
                    CssProperty::FontFamily(f) => f.get_property(),
                    _ => None,
                })
                .collect();
            assert_eq!(
                families.len(),
                1,
                "{kind:?}: exactly one font-family declaration"
            );
            let fams = families[0].as_ref();
            assert_eq!(
                fams.len(),
                1,
                "{kind:?}: expected a single system-ui family"
            );
            match &fams[0] {
                StyleFontFamily::System(name) => {
                    assert_eq!(
                        name.as_str(),
                        "system:ui",
                        "{kind:?}: wrong system font family"
                    )
                }
                other => panic!("{kind:?}: chip must use the system UI font, got {other:?}"),
            }
            assert!(
                text_color(&style).is_some(),
                "{kind:?}: no inheritable text colour declared"
            );
        }
    }

    #[test]
    fn build_chip_style_colours_track_the_kind() {
        for kind in ALL_KINDS {
            let style = build_chip_style(kind);
            let (bg, text) = kind.colors();
            assert_eq!(
                background_color(&style),
                Some(bg),
                "{kind:?}: emitted background != colors().0"
            );
            assert_eq!(
                text_color(&style),
                Some(text),
                "{kind:?}: emitted text colour != colors().1"
            );
        }
    }

    #[test]
    fn build_chip_style_declares_every_property_at_most_once() {
        // A duplicated declaration is a last-one-wins ambiguity: two backgrounds
        // would make one of them silently dead.
        for kind in ALL_KINDS {
            let types = property_types(&build_chip_style(kind));
            let mut seen = HashSet::new();
            for t in &types {
                assert!(
                    seen.insert(*t),
                    "{kind:?}: the container style declares the same property twice"
                );
            }
            assert_eq!(seen.len(), types.len());
        }
    }

    #[test]
    fn build_chip_style_properties_are_all_unconditional() {
        // The chip container is stateless — a declaration gated on
        // `:hover`/`:active` would simply never paint.
        for kind in ALL_KINDS {
            for p in build_chip_style(kind).as_ref() {
                assert!(
                    p.apply_if.as_ref().is_empty(),
                    "{kind:?}: {:?} is conditional on a stateless container",
                    p.property
                );
            }
        }
    }

    #[test]
    fn build_chip_style_is_deterministic_and_kind_dependent() {
        let baseline = property_types(&build_chip_style(ChipKind::Default));
        assert!(
            !baseline.is_empty(),
            "the container style must not be empty"
        );

        for kind in ALL_KINDS {
            assert_eq!(
                properties(&build_chip_style(kind)),
                properties(&build_chip_style(kind)),
                "{kind:?}: two builds of the same kind disagree"
            );
            assert_eq!(
                property_types(&build_chip_style(kind)),
                baseline,
                "{kind:?}: declares different properties, or in a different order, than Default"
            );
        }
        // No two kinds may collapse onto the same style, or the variant is a no-op.
        for (i, a) in ALL_KINDS.iter().enumerate() {
            for b in &ALL_KINDS[i + 1..] {
                assert_ne!(
                    properties(&build_chip_style(*a)),
                    properties(&build_chip_style(*b)),
                    "{a:?} and {b:?} produce an identical style"
                );
            }
        }
    }

    #[test]
    fn build_chip_style_differs_only_in_the_colours() {
        // Exactly two declarations may depend on the kind: the background and the
        // text colour. Kinds that share a text colour (Default/Warning/Info are
        // all dark-on-light) must therefore differ in the background *alone* —
        // any third differing declaration means geometry leaked into the palette.
        for (i, a) in ALL_KINDS.iter().enumerate() {
            for b in &ALL_KINDS[i + 1..] {
                let (style_a, style_b) = (build_chip_style(*a), build_chip_style(*b));
                let differing: Vec<_> = style_a
                    .as_ref()
                    .iter()
                    .zip(style_b.as_ref().iter())
                    .filter(|(x, y)| x != y)
                    .map(|(x, _)| core::mem::discriminant(&x.property))
                    .collect();

                let same_text = a.colors().1 == b.colors().1;
                let expected = if same_text { 1 } else { 2 };
                assert_eq!(
                    differing.len(),
                    expected,
                    "{a:?} vs {b:?}: only the background{} may depend on the kind",
                    if same_text {
                        ""
                    } else {
                        " and the text colour"
                    }
                );
                let bg_differs =
                    style_a
                        .as_ref()
                        .iter()
                        .zip(style_b.as_ref().iter())
                        .any(|(x, y)| {
                            x != y && matches!(x.property, CssProperty::BackgroundContent(_))
                        });
                assert!(
                    bg_differs,
                    "{a:?} vs {b:?}: the background must be one of the differing declarations"
                );
            }
        }
    }

    #[test]
    fn build_chip_style_emits_only_finite_non_negative_px_lengths() {
        // Guard the numeric conversions in this file (`isize` -> `PixelValue`):
        // a NaN/inf/negative length must never reach the layout solver.
        for kind in ALL_KINDS {
            let values = all_pixel_values(&build_chip_style(kind));
            assert_eq!(
                values.len(),
                9,
                "{kind:?}: expected 4 paddings + 4 radii + 1 font size"
            );
            for pv in values {
                let n = px(&pv); // also asserts SizeMetric::Px
                assert!(n.is_finite(), "{kind:?}: non-finite length {n}");
                assert!(n >= 0.0, "{kind:?}: negative length {n}");
                assert!(
                    n <= 128.0,
                    "{kind:?}: implausibly large length {n} for a chip"
                );
            }
        }
    }

    #[test]
    fn label_and_remove_static_styles_are_finite_unconditional_and_non_growing() {
        for (name, style) in [("label", CHIP_LABEL_STYLE), ("remove", CHIP_REMOVE_STYLE)] {
            let vec = CssPropertyWithConditionsVec::from_const_slice(style);
            for p in vec.as_ref() {
                assert!(
                    p.apply_if.as_ref().is_empty(),
                    "{name}: {:?} must be unconditional",
                    p.property
                );
            }
            // Both children must hug their content, or the "x" is pushed off the pill.
            assert!(
                properties(&vec)
                    .contains(&CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
                "{name}: child must not grow inside the pill"
            );
            for pv in all_pixel_values(&vec) {
                let n = px(&pv);
                assert!(n.is_finite(), "{name}: non-finite length {n}");
                assert!((0.0..=64.0).contains(&n), "{name}: implausible length {n}");
            }
            let types = property_types(&vec);
            let mut seen = HashSet::new();
            for t in &types {
                assert!(seen.insert(*t), "{name}: declares the same property twice");
            }
        }
    }

    #[test]
    fn the_remove_affordance_is_styled_as_a_clickable_target() {
        let remove = CssPropertyWithConditionsVec::from_const_slice(CHIP_REMOVE_STYLE);
        let props = properties(&remove);
        assert!(
            props.contains(&CssProperty::const_cursor(StyleCursor::Pointer)),
            "the x must advertise itself as clickable"
        );
        assert!(
            props.contains(&CssProperty::user_select(StyleUserSelect::None)),
            "dragging across the x must not select the glyph"
        );
        assert!(
            props.contains(&CssProperty::const_margin_left(LayoutMarginLeft::const_px(
                6
            ))),
            "the x needs breathing room from the label"
        );

        let label = CssPropertyWithConditionsVec::from_const_slice(CHIP_LABEL_STYLE);
        let label_props = properties(&label);
        assert!(
            label_props.contains(&CssProperty::const_text_align(StyleTextAlign::Left)),
            "the label owns text-align, not the container"
        );
        assert!(
            label_props.contains(&CssProperty::user_select(StyleUserSelect::None)),
            "the label must not be selectable (it is a click target)"
        );
        assert!(
            !label_props
                .iter()
                .any(|p| matches!(p, CssProperty::Cursor(_))),
            "only the x declares a pointer cursor"
        );
    }

    // ------------------------------------------------------------------
    // Chip::create / Chip::with_kind  (constructors)
    // ------------------------------------------------------------------

    #[test]
    fn create_defaults_to_the_neutral_tag_and_keeps_the_label_verbatim() {
        for s in adversarial_strings() {
            let c = Chip::create(AzString::from(s.clone()));
            assert_eq!(
                c.label.as_str(),
                s.as_str(),
                "the label was not preserved verbatim"
            );
            assert_eq!(
                c.label.len(),
                s.len(),
                "byte length changed (NUL truncation?)"
            );
            assert_eq!(
                c.kind,
                ChipKind::Default,
                "create() must use the neutral default kind"
            );
            assert!(!c.removable, "a fresh chip renders no x");
            assert!(c.chip_state.inner.visible, "a fresh chip is visible");
            assert!(
                c.chip_state.on_remove.is_none(),
                "a fresh chip carries no remove callback"
            );
            assert!(
                c.chip_state.on_click.is_none(),
                "a fresh chip carries no click callback"
            );
            assert_eq!(
                properties(&c.container_style),
                properties(&build_chip_style(ChipKind::Default))
            );
        }
    }

    #[test]
    fn with_kind_stores_both_arguments_and_the_matching_style() {
        for kind in ALL_KINDS {
            for s in adversarial_strings() {
                let c = Chip::with_kind(AzString::from(s.clone()), kind);
                assert_eq!(
                    c.label.as_str(),
                    s.as_str(),
                    "{kind:?}: label not preserved"
                );
                assert_eq!(c.label.len(), s.len(), "{kind:?}: byte length changed");
                assert_eq!(
                    c.kind, kind,
                    "{kind:?}: kind field does not match the argument"
                );
                assert!(!c.removable, "{kind:?}: with_kind must not turn on the x");
                // The invariant that makes `container_style` a cache and not a lie.
                assert_eq!(
                    properties(&c.container_style),
                    properties(&build_chip_style(kind))
                );
                assert_eq!(background_color(&c.container_style), Some(kind.colors().0));
            }
        }
    }

    #[test]
    fn create_is_with_kind_default() {
        for s in ["", "tag", "\u{1F600}", "\u{00D7}"] {
            assert_eq!(
                Chip::create(AzString::from_const_str(s)),
                Chip::with_kind(AzString::from_const_str(s), ChipKind::Default)
            );
        }
    }

    #[test]
    fn default_chip_is_an_empty_neutral_chip_and_equality_sees_every_field() {
        let d = Chip::default();
        assert_eq!(d, Chip::create(AzString::from_const_str("")));
        assert_eq!(d.label.as_str(), "");
        assert_eq!(d.kind, ChipKind::Default);
        assert!(!d.removable);
        assert_eq!(d.clone(), d, "Clone must preserve equality");

        assert_ne!(
            d,
            Chip::create(AzString::from_const_str("tag")),
            "the label must affect equality"
        );
        assert_ne!(
            Chip::with_kind(AzString::from_const_str("t"), ChipKind::Danger),
            Chip::with_kind(AzString::from_const_str("t"), ChipKind::Success),
            "chips of different kinds must not compare equal"
        );
        assert_ne!(
            Chip::default(),
            Chip::default().with_removable(true),
            "the removable flag must affect equality"
        );
    }

    #[test]
    fn chips_wired_to_separate_payloads_are_not_equal() {
        // `RefAny` equality is "same data", not "same value": two independently
        // allocated payloads holding the same `u8` are distinct, so the chips
        // that carry them must not compare equal either.
        let shared = RefAny::new(0u8);
        let a = Chip::create(AzString::from("t"))
            .with_on_click(shared.clone(), click_cb(click_do_nothing));
        let b = Chip::create(AzString::from("t"))
            .with_on_click(shared.clone(), click_cb(click_do_nothing));
        assert_eq!(
            a, b,
            "chips sharing one payload and one callback must be equal"
        );

        let c = Chip::create(AzString::from("t"))
            .with_on_click(RefAny::new(0u8), click_cb(click_do_nothing));
        assert_ne!(a, c, "a separately allocated payload is a different chip");
    }

    // ------------------------------------------------------------------
    // Chip::set_kind / with_chip_kind
    // ------------------------------------------------------------------

    #[test]
    fn set_kind_recomputes_the_style_without_growing_it() {
        // A push-instead-of-replace bug would grow the style vec on every call and
        // leave stale (earlier-kind) colour declarations behind, which then win or
        // lose the cascade by accident.
        let mut c = Chip::create(AzString::from_const_str("tag"));
        let expected_len = build_chip_style(ChipKind::Default).as_ref().len();

        for round in 0..50 {
            let kind = ALL_KINDS[round % ALL_KINDS.len()];
            c.set_kind(kind);

            assert_eq!(c.kind, kind, "round {round}: kind field not updated");
            assert_eq!(
                c.container_style.as_ref().len(),
                expected_len,
                "round {round}: style vec changed length — stale declarations?"
            );
            assert_eq!(
                properties(&c.container_style),
                properties(&build_chip_style(kind)),
                "round {round}: style does not match a freshly built one"
            );
            assert_eq!(
                background_color(&c.container_style),
                Some(kind.colors().0),
                "round {round}: stale background"
            );
            assert_eq!(
                c.label.as_str(),
                "tag",
                "round {round}: set_kind ate the label"
            );
        }
    }

    #[test]
    fn set_kind_leaves_label_removable_and_callbacks_alone() {
        let mut c = Chip::create(AzString::from("keep me"));
        c.set_on_remove(
            RefAny::new(StateLog { calls: Vec::new() }),
            remove_cb(remove_do_nothing),
        );
        c.set_on_click(RefAny::new(0u8), click_cb(click_do_nothing));
        c.chip_state.inner.visible = false;

        c.set_kind(ChipKind::Warning);

        assert_eq!(c.label.as_str(), "keep me");
        assert!(c.removable, "set_kind must not clear the x");
        assert!(
            c.chip_state.on_remove.is_some(),
            "set_kind must not drop the remove callback"
        );
        assert!(
            c.chip_state.on_click.is_some(),
            "set_kind must not drop the click callback"
        );
        assert!(
            !c.chip_state.inner.visible,
            "set_kind must not resurrect a removed chip"
        );
    }

    #[test]
    fn with_chip_kind_agrees_with_set_kind_and_is_last_call_wins() {
        let chained = Chip::create(AzString::from_const_str("t"))
            .with_chip_kind(ChipKind::Danger)
            .with_chip_kind(ChipKind::Warning)
            .with_chip_kind(ChipKind::Info);

        let mut mutated = Chip::create(AzString::from_const_str("t"));
        mutated.set_kind(ChipKind::Danger);
        mutated.set_kind(ChipKind::Warning);
        mutated.set_kind(ChipKind::Info);

        assert_eq!(chained, mutated, "the builder and the mutator must agree");
        assert_eq!(chained.kind, ChipKind::Info);
        assert_eq!(chained.label.as_str(), "t");
        assert_eq!(
            properties(&chained.container_style),
            properties(&build_chip_style(ChipKind::Info))
        );
        // In particular the Danger red must be completely gone.
        assert_eq!(
            background_color(&chained.container_style),
            Some(ChipKind::Info.colors().0)
        );
    }

    #[test]
    fn setting_the_same_kind_twice_is_idempotent() {
        for kind in ALL_KINDS {
            let once = Chip::with_kind(AzString::from_const_str("x"), kind);
            let twice = once.clone().with_chip_kind(kind);
            assert_eq!(
                once, twice,
                "{kind:?}: re-setting the same kind changed the chip"
            );
        }
        // A full cycle through every kind and back must not accumulate state.
        let original = Chip::create(AzString::from("t"));
        let mut cycled = original.clone();
        for kind in ALL_KINDS {
            cycled.set_kind(kind);
        }
        cycled.set_kind(ChipKind::Default);
        assert_eq!(cycled, original, "kind cycling must not accumulate state");
    }

    // ------------------------------------------------------------------
    // Chip::set_removable / with_removable
    // ------------------------------------------------------------------

    #[test]
    fn set_removable_last_write_wins_and_touches_nothing_else() {
        let mut c = Chip::with_kind(AzString::from("t"), ChipKind::Warning);
        let style_before = c.container_style.clone();

        for flag in [true, true, false, true, false, false] {
            c.set_removable(flag);
            assert_eq!(c.removable, flag);
        }

        assert_eq!(c.kind, ChipKind::Warning);
        assert_eq!(c.label.as_str(), "t");
        assert_eq!(
            c.container_style, style_before,
            "toggling must not restyle the pill"
        );
        assert!(
            c.chip_state.on_remove.is_none(),
            "toggling must not invent a callback"
        );
        assert!(
            c.chip_state.inner.visible,
            "toggling must not hide the chip"
        );
    }

    #[test]
    fn with_removable_toggle_sequence_ends_on_the_last_value() {
        assert!(Chip::default().with_removable(true).removable);
        assert!(!Chip::default().with_removable(false).removable);
        assert!(
            !Chip::default()
                .with_removable(true)
                .with_removable(false)
                .removable
        );
        assert!(
            Chip::default()
                .with_removable(false)
                .with_removable(true)
                .removable
        );

        // builder == setter
        let mut mutated = Chip::default();
        mutated.set_removable(true);
        assert_eq!(Chip::default().with_removable(true), mutated);
    }

    #[test]
    fn removable_only_changes_the_child_count_not_the_pill() {
        let plain = Chip::create(AzString::from("t"));
        let style = plain.container_style.clone();
        let removable = plain.clone().with_removable(true);

        assert_eq!(
            removable.container_style, style,
            "the x must not restyle the container"
        );
        assert_eq!(plain.dom().children.as_ref().len(), 1);
        assert_eq!(removable.dom().children.as_ref().len(), 2);
    }

    // ------------------------------------------------------------------
    // Chip::set_on_remove / with_on_remove
    // ------------------------------------------------------------------

    #[test]
    fn set_on_remove_implies_removable() {
        let mut c = Chip::create(AzString::from("t"));
        assert!(!c.removable);

        c.set_on_remove(RefAny::new(1u8), remove_cb(remove_do_nothing));

        assert!(c.removable, "a remove callback must render an x");
        assert!(c.chip_state.on_remove.is_some());
        assert!(
            c.chip_state.on_click.is_none(),
            "wiring on_remove must not invent an on_click"
        );
        assert!(
            c.chip_state.inner.visible,
            "wiring a callback must not hide the chip"
        );
        assert_eq!(
            c.dom().children.as_ref().len(),
            2,
            "the x must actually be rendered"
        );
    }

    #[test]
    fn set_on_remove_replaces_rather_than_appends() {
        let mut c = Chip::create(AzString::from("t"));

        c.set_on_remove(RefAny::new(1u8), remove_cb(remove_do_nothing));
        let first = c
            .chip_state
            .on_remove
            .as_ref()
            .expect("first callback")
            .refany
            .get_type_id();
        assert_eq!(first, RefAny::new(1u8).get_type_id());

        // a second call must *replace* the payload + function, not stack another one
        c.set_on_remove(RefAny::new(9i64), remove_cb(record_remove));
        let second = c.chip_state.on_remove.as_ref().expect("second callback");
        assert_eq!(second.refany.get_type_id(), RefAny::new(9i64).get_type_id());
        assert_eq!(second.callback, remove_cb(record_remove));
        assert_ne!(second.callback, remove_cb(remove_do_nothing));

        // still exactly one x in the DOM
        assert_eq!(c.dom().children.as_ref().len(), 2);
    }

    #[test]
    fn with_on_remove_keeps_the_label_kind_and_style() {
        let c = Chip::with_kind(AzString::from("boom"), ChipKind::Danger)
            .with_on_remove(RefAny::new(0u8), remove_cb(remove_do_nothing));

        assert_eq!(c.label.as_str(), "boom");
        assert_eq!(c.kind, ChipKind::Danger);
        assert_eq!(c.container_style, build_chip_style(ChipKind::Danger));
        assert!(c.removable);
        assert!(c.chip_state.on_remove.is_some());
    }

    #[test]
    fn set_removable_false_after_set_on_remove_silently_drops_the_x() {
        // Footgun, pinned as the *current* behaviour: `set_on_remove` implies
        // `removable = true`, but a later `set_removable(false)` wins and the
        // wired-up callback becomes unreachable (no x is rendered).
        let mut c = Chip::create(AzString::from("t"));
        c.set_on_remove(RefAny::new(0u8), remove_cb(record_remove));
        c.set_removable(false);

        assert!(
            c.chip_state.on_remove.is_some(),
            "the callback is still stored"
        );
        let dom = c.dom();
        assert_eq!(
            dom.children.as_ref().len(),
            1,
            "no x is rendered, so the remove callback can never fire"
        );
    }

    // ------------------------------------------------------------------
    // Chip::set_on_click / with_on_click
    // ------------------------------------------------------------------

    #[test]
    fn set_on_click_does_not_imply_removable() {
        // Deliberate asymmetry with `set_on_remove`: a clickable chip is not
        // automatically a removable one.
        let mut c = Chip::create(AzString::from("t"));
        c.set_on_click(RefAny::new(1u8), click_cb(click_do_nothing));

        assert!(!c.removable, "on_click must not turn on the x");
        assert!(c.chip_state.on_click.is_some());
        assert!(c.chip_state.on_remove.is_none());
        assert_eq!(c.dom().children.as_ref().len(), 1, "still just the label");
    }

    #[test]
    fn set_on_click_replaces_rather_than_appends() {
        let mut c = Chip::create(AzString::from("t"));

        c.set_on_click(RefAny::new(1u8), click_cb(click_do_nothing));
        c.set_on_click(RefAny::new(9i64), click_cb(record_click));

        let second = c.chip_state.on_click.as_ref().expect("second callback");
        assert_eq!(second.refany.get_type_id(), RefAny::new(9i64).get_type_id());
        assert_eq!(second.callback, click_cb(record_click));
        assert_ne!(second.callback, click_cb(click_do_nothing));

        // exactly one handler reaches the label
        let dom = c.dom();
        assert_eq!(
            dom.children.as_ref()[0].root.get_callbacks().as_ref().len(),
            1
        );
    }

    #[test]
    fn with_on_click_keeps_the_label_kind_and_style() {
        let c = Chip::with_kind(AzString::from("click me"), ChipKind::Primary)
            .with_on_click(RefAny::new(0u8), click_cb(click_do_nothing));

        assert_eq!(c.label.as_str(), "click me");
        assert_eq!(c.kind, ChipKind::Primary);
        assert_eq!(c.container_style, build_chip_style(ChipKind::Primary));
        assert!(c.chip_state.on_click.is_some());
    }

    #[test]
    fn both_callbacks_can_be_wired_at_once() {
        let c = Chip::create(AzString::from("t"))
            .with_on_click(RefAny::new(1u8), click_cb(record_click))
            .with_on_remove(RefAny::new(2u8), remove_cb(record_remove));

        assert!(
            c.removable,
            "on_remove still implies removable when on_click is set"
        );
        assert!(
            c.chip_state.on_click.is_some(),
            "on_remove must not clobber on_click"
        );
        assert!(c.chip_state.on_remove.is_some());
        assert_eq!(
            c.chip_state.on_click.as_ref().expect("on_click").callback,
            click_cb(record_click)
        );
        assert_eq!(
            c.chip_state.on_remove.as_ref().expect("on_remove").callback,
            remove_cb(record_remove)
        );

        // Setting them in the opposite order must produce the same wiring.
        let reversed = Chip::create(AzString::from("t"))
            .with_on_remove(RefAny::new(2u8), remove_cb(record_remove))
            .with_on_click(RefAny::new(1u8), click_cb(record_click));
        assert!(reversed.removable);
        assert!(reversed.chip_state.on_click.is_some());
        assert!(reversed.chip_state.on_remove.is_some());
    }

    // ------------------------------------------------------------------
    // Chip::swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_returns_the_original_and_leaves_a_default_behind() {
        let mut c =
            Chip::with_kind(AzString::from_const_str("tag"), ChipKind::Danger).with_removable(true);
        let taken = c.swap_with_default();

        // The returned value is the *original*, intact.
        assert_eq!(taken.label.as_str(), "tag");
        assert_eq!(taken.kind, ChipKind::Danger);
        assert!(taken.removable);
        assert_eq!(
            properties(&taken.container_style),
            properties(&build_chip_style(ChipKind::Danger))
        );

        // What is left behind is a *default* chip — in particular its style must
        // be the neutral one and not a stale Danger red.
        assert_eq!(c, Chip::default());
        assert_eq!(c.label.as_str(), "");
        assert_eq!(c.kind, ChipKind::Default);
        assert!(!c.removable, "the x must not survive the swap");
        assert_eq!(
            background_color(&c.container_style),
            Some(ChipKind::Default.colors().0),
            "the red survived the swap"
        );
    }

    #[test]
    fn swap_with_default_is_idempotent_on_an_already_default_chip() {
        let mut c = Chip::default();
        for _ in 0..3 {
            let taken = c.swap_with_default();
            assert_eq!(taken, Chip::default());
            assert_eq!(c, Chip::default());
        }
    }

    #[test]
    fn swap_with_default_moves_the_callbacks_out_of_self() {
        let mut c = Chip::create(AzString::from("t"))
            .with_on_remove(RefAny::new(7u32), remove_cb(record_remove))
            .with_on_click(RefAny::new(8u32), click_cb(record_click));

        let taken = c.swap_with_default();

        assert!(
            taken.chip_state.on_remove.is_some(),
            "the remove callback moves out"
        );
        assert!(
            taken.chip_state.on_click.is_some(),
            "the click callback moves out"
        );
        assert!(
            c.chip_state.on_remove.is_none(),
            "the reset chip must not keep a reference to the old callback"
        );
        assert!(c.chip_state.on_click.is_none());
        assert!(!c.removable);
    }

    #[test]
    fn swap_with_default_survives_a_huge_label_and_repeated_swaps() {
        let long = "x".repeat(100_000);
        let mut c = Chip::with_kind(AzString::from(long.clone()), ChipKind::Success);
        for round in 0..10 {
            let taken = c.swap_with_default();
            if round == 0 {
                assert_eq!(
                    taken.label.len(),
                    long.len(),
                    "the long label was truncated"
                );
                assert_eq!(taken.kind, ChipKind::Success);
            } else {
                assert_eq!(
                    taken,
                    Chip::default(),
                    "round {round}: the emptied chip is not a default"
                );
            }
            assert_eq!(
                c,
                Chip::default(),
                "round {round}: what was left behind is not a default"
            );
        }
    }

    #[test]
    fn swap_with_default_preserves_a_hidden_state_on_the_returned_chip() {
        let mut c = Chip::create(AzString::from("t")).with_removable(true);
        c.chip_state.inner.visible = false;

        let taken = c.swap_with_default();
        assert!(
            !taken.chip_state.inner.visible,
            "the removed state travels with the original"
        );
        assert!(
            c.chip_state.inner.visible,
            "the fresh chip left behind must be visible"
        );
    }

    // ------------------------------------------------------------------
    // Chip::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_of_a_plain_chip_is_a_container_with_one_inert_label() {
        for kind in ALL_KINDS {
            let chip = Chip::with_kind(AzString::from("tag"), kind);
            let expected = properties(&chip.container_style);
            let dom = chip.dom();

            assert!(
                dom.root.has_class("__azul-native-chip"),
                "{kind:?}: missing the widget class"
            );
            assert!(
                dom.root.get_callbacks().as_ref().is_empty(),
                "{kind:?}: a stateless chip must carry no container callback"
            );
            assert_eq!(
                inline_properties(&dom),
                expected,
                "{kind:?}: the pill lost its computed style"
            );

            let children = dom.children.as_ref();
            assert_eq!(children.len(), 1, "{kind:?}: no x without `removable`");
            let label = &children[0];
            assert!(label.root.has_class("__azul-native-chip-label"));
            assert_eq!(
                text_of(label),
                Some("tag"),
                "{kind:?}: the label was mangled"
            );
            assert!(
                label.root.get_callbacks().as_ref().is_empty(),
                "{kind:?}: no on_click means no handler on the label"
            );
            assert!(
                label.root.get_tab_index().is_none(),
                "{kind:?}: an inert label must not be keyboard-focusable"
            );
            assert_eq!(
                label.children.as_ref().len(),
                1,
                "{kind:?}: the label is a <p> wrapping exactly one bare text node"
            );
        }
    }

    #[test]
    fn dom_of_a_removable_chip_appends_a_focusable_x() {
        let dom = Chip::create(AzString::from("tag"))
            .with_removable(true)
            .dom();

        let children = dom.children.as_ref();
        assert_eq!(children.len(), 2, "[label, remove]");

        let remove = &children[1];
        assert!(remove.root.has_class("__azul-native-chip-remove"));
        assert_eq!(
            text_of(remove),
            Some("\u{00D7}"),
            "the remove glyph is U+00D7 MULTIPLICATION SIGN, not an ASCII 'x'"
        );
        assert!(
            matches!(remove.root.get_tab_index(), Some(TabIndex::Auto)),
            "the x must be keyboard-reachable"
        );

        let callbacks = remove.root.get_callbacks();
        assert_eq!(callbacks.as_ref().len(), 1, "exactly one remove handler");
        let cb = &callbacks.as_ref()[0];
        assert!(matches!(
            &cb.event,
            EventFilter::Hover(HoverEventFilter::Click)
        ));
        assert_eq!(cb.callback.cb, default_on_chip_remove as usize);
        assert!(matches!(&cb.callback.ctx, OptionRefAny::None));

        // The container itself must stay handler-free, or a click on the x would
        // bubble and double-fire.
        assert!(dom.root.get_callbacks().as_ref().is_empty());
    }

    #[test]
    fn dom_attaches_the_click_handler_to_the_label_not_the_container() {
        // Documented rationale: a container-level MouseUp would also fire when the
        // x (a child) is clicked, double-firing alongside on_remove.
        let dom = Chip::create(AzString::from("tag"))
            .with_on_click(RefAny::new(0u8), click_cb(record_click))
            .dom();

        assert!(
            dom.root.get_callbacks().as_ref().is_empty(),
            "the pill container must never carry the click handler"
        );

        let label = &dom.children.as_ref()[0];
        assert!(
            matches!(label.root.get_tab_index(), Some(TabIndex::Auto)),
            "a clickable label must be keyboard-reachable"
        );
        let callbacks = label.root.get_callbacks();
        assert_eq!(callbacks.as_ref().len(), 1, "exactly one click handler");
        let cb = &callbacks.as_ref()[0];
        assert!(matches!(
            &cb.event,
            EventFilter::Hover(HoverEventFilter::Click)
        ));
        assert_eq!(cb.callback.cb, default_on_chip_click as usize);
        assert!(matches!(&cb.callback.ctx, OptionRefAny::None));
    }

    #[test]
    fn dom_gives_the_label_and_the_x_one_shared_state() {
        // The doc promises both handlers observe the same `ChipState`.
        let dom = Chip::create(AzString::from("tag"))
            .with_removable(true)
            .with_on_click(RefAny::new(0u8), click_cb(record_click))
            .dom();

        let children = dom.children.as_ref();
        assert_eq!(children.len(), 2);
        let mut label_state = children[0].root.get_callbacks().as_ref()[0].refany.clone();
        let mut remove_state = children[1].root.get_callbacks().as_ref()[0].refany.clone();

        assert_eq!(
            label_state, remove_state,
            "the two handlers must share one state RefAny"
        );

        // ...and prove it is genuinely shared, not merely equal.
        {
            let mut w = label_state
                .downcast_mut::<ChipStateWrapper>()
                .expect("the label payload must be a ChipStateWrapper");
            w.inner.visible = false;
        }
        assert!(
            !wrapper_visible(&mut remove_state),
            "a write through the label's handle must be visible through the x's handle"
        );
    }

    #[test]
    fn dom_of_a_removable_chip_without_on_click_leaves_the_label_inert() {
        let dom = Chip::create(AzString::from("tag"))
            .with_removable(true)
            .dom();
        let label = &dom.children.as_ref()[0];
        assert!(
            label.root.get_callbacks().as_ref().is_empty(),
            "removable alone must not make the label clickable"
        );
        assert!(label.root.get_tab_index().is_none());
    }

    #[test]
    fn dom_preserves_adversarial_labels_verbatim() {
        for s in adversarial_strings() {
            let dom = Chip::create(AzString::from(s.clone()))
                .with_removable(true)
                .dom();
            let label = &dom.children.as_ref()[0];
            let t = text_of(label).expect("the label must be a text node");
            assert_eq!(t, s.as_str(), "the label changed on its way into the DOM");
            assert_eq!(t.len(), s.len(), "byte length changed (NUL truncation?)");
            assert!(dom.root.has_class("__azul-native-chip"));

            // A label that *is* the remove glyph must not be confusable with the x:
            // the two are told apart by class, never by text.
            let remove = &dom.children.as_ref()[1];
            assert!(label.root.has_class("__azul-native-chip-label"));
            assert!(remove.root.has_class("__azul-native-chip-remove"));
            assert!(!label.root.has_class("__azul-native-chip-remove"));
        }
    }

    #[test]
    fn dom_does_not_emit_the_kind_class() {
        // Current behaviour, pinned: `ChipKind::class_name()` is never applied to
        // the DOM — the container only carries the generic container class, and
        // the kind travels as inline style instead.
        for kind in ALL_KINDS {
            let dom = Chip::with_kind(AzString::from("t"), kind)
                .with_removable(true)
                .dom();
            assert!(dom.root.has_class("__azul-native-chip"));
            assert!(
                !dom.root.has_class(kind.class_name()),
                "{kind:?}: the kind class is not emitted (kind is carried inline)"
            );
        }
    }

    #[test]
    fn dom_renders_the_kind_the_chip_was_last_set_to() {
        // `dom()` consumes the *cached* style, so a `set_kind` that forgot to
        // recompute would paint the previous colour here and nowhere else.
        for kind in ALL_KINDS {
            let mut chip = Chip::create(AzString::from_const_str("t"));
            chip.set_kind(ChipKind::Danger);
            chip.set_kind(kind);
            let expected = properties(&build_chip_style(kind));
            assert_eq!(
                inline_properties(&chip.dom()),
                expected,
                "{kind:?}: the DOM does not show the current kind"
            );
        }
    }

    #[test]
    fn from_chip_for_dom_is_exactly_dom() {
        for kind in ALL_KINDS {
            let chip = Chip::with_kind(AzString::from_const_str("ok"), kind).with_removable(true);
            let via_into: Dom = chip.clone().into();
            let via_dom = chip.dom();
            assert_eq!(
                inline_properties(&via_into),
                inline_properties(&via_dom),
                "{kind:?}: `From` diverges from `dom()`"
            );
            assert_eq!(
                via_into.root.get_node_type(),
                via_dom.root.get_node_type(),
                "{kind:?}: `From` built a different node"
            );
            assert_eq!(
                via_into.children.as_ref().len(),
                via_dom.children.as_ref().len()
            );
        }
    }

    #[test]
    fn dom_flattens_to_the_hierarchy_the_remove_handler_expects() {
        // `default_on_chip_remove` hard-codes "parent of the hit node is the
        // container". That only holds while the x is a *direct* child.
        let styled = removable_styled_dom();
        let hierarchy = styled.node_hierarchy.as_ref();
        assert_eq!(
            hierarchy.len(),
            5,
            "container(0), label <p>(1)+text(2), remove <p>(3)+text(4)"
        );
        assert_eq!(
            hierarchy[REMOVE_NODE].parent_id(),
            Some(NodeId::new(0)),
            "the x's parent must be the pill container"
        );
        assert_eq!(hierarchy[0].parent_id(), None, "the container is the root");
    }

    // ------------------------------------------------------------------
    // default_on_chip_remove
    // ------------------------------------------------------------------

    #[test]
    fn remove_hides_the_container_and_flips_visible() {
        let mut data = RefAny::new(ChipStateWrapper::default());

        // REMOVE_NODE == the x <p>, its parent (node 0) is the container
        let (update, changes) = run_remove(Some(removable_styled_dom()), REMOVE_NODE, data.clone());

        assert_eq!(update, Update::DoNothing, "no user callback -> DoNothing");
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)],
            "the *container* (not the x) must be hidden"
        );
        assert_eq!(changes.len(), 1, "exactly one restyle per click");
        assert!(!wrapper_visible(&mut data), "state must flip to hidden");
    }

    #[test]
    fn remove_invokes_the_user_callback_with_the_already_flipped_state() {
        let (mut data, mut log) = state_with_remove_log();

        let (update, changes) = run_remove(Some(removable_styled_dom()), REMOVE_NODE, data.clone());

        assert_eq!(
            update,
            Update::RefreshDom,
            "the user callback's Update is returned"
        );
        assert_eq!(
            log_calls(&mut log),
            alloc::vec![false],
            "the callback must see `visible == false` (already removed)"
        );
        assert!(!wrapper_visible(&mut data));
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)],
            "the container is hidden even after a user callback ran"
        );
    }

    #[test]
    fn remove_twice_is_idempotent() {
        let (mut data, mut log) = state_with_remove_log();

        for _ in 0..2 {
            let (update, changes) =
                run_remove(Some(removable_styled_dom()), REMOVE_NODE, data.clone());
            assert_eq!(update, Update::RefreshDom);
            assert_eq!(
                display_writes(&changes),
                alloc::vec![(0usize, LayoutDisplay::None)]
            );
        }

        assert!(
            !wrapper_visible(&mut data),
            "a second remove must not un-hide"
        );
        assert_eq!(
            log_calls(&mut log),
            alloc::vec![false, false],
            "each click fires the callback exactly once, always with visible == false"
        );
    }

    #[test]
    fn remove_on_a_root_hit_node_is_a_noop() {
        // node 0 has no parent -> there is no container to hide, and the early
        // return happens *before* the state is touched
        let mut data = RefAny::new(ChipStateWrapper::default());

        let (update, changes) = run_remove(Some(removable_styled_dom()), 0, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "nothing may be restyled without a parent"
        );
        assert!(wrapper_visible(&mut data), "state must not flip");
    }

    #[test]
    fn remove_with_a_stale_hit_node_is_a_noop() {
        // node 999 does not exist in the 3-node fixture
        let mut data = RefAny::new(ChipStateWrapper::default());

        let (update, changes) = run_remove(Some(removable_styled_dom()), 999, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(wrapper_visible(&mut data));
    }

    #[test]
    fn remove_without_any_layout_result_is_a_noop() {
        let mut data = RefAny::new(ChipStateWrapper::default());

        let (update, changes) = run_remove(None, REMOVE_NODE, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(wrapper_visible(&mut data), "state must not flip");
    }

    #[test]
    fn remove_with_a_foreign_payload_is_a_noop() {
        // the callback-bearing node carries a RefAny of the *wrong* type
        let data = RefAny::new(0xdead_beef_u64);

        let (update, changes) = run_remove(Some(removable_styled_dom()), REMOVE_NODE, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a foreign payload must not hide the container"
        );
    }

    #[test]
    fn remove_fired_from_the_label_still_hides_the_container() {
        // Current behaviour, pinned: the handler trusts its wiring — it hides
        // whatever the hit node's parent is and never checks that the hit node is
        // actually the x. Firing it from the label <p> therefore hides the
        // container just the same.
        let mut data = RefAny::new(ChipStateWrapper::default());

        let (update, changes) = run_remove(Some(removable_styled_dom()), LABEL_NODE, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)]
        );
        assert!(!wrapper_visible(&mut data));
    }

    #[test]
    fn remove_holds_the_state_borrow_across_the_user_callback() {
        // The handler invokes the user callback while its own `downcast_mut` on
        // the state is still live. A user callback that re-enters the *same*
        // state `RefAny` is therefore refused — it must get `None` back rather
        // than a second aliasing borrow (or a panic).
        //
        // NOTE: probe <-> state form a RefAny reference cycle, so this fixture
        // leaks. That is deliberate and harmless for a single test.
        let mut probe = RefAny::new(ReentrantProbe {
            state: RefAny::new(0u8),
            saw_state: Some(true),
            calls: 0,
        });
        let state = RefAny::new(ChipStateWrapper {
            inner: ChipState { visible: true },
            on_remove: Some(ChipOnRemove {
                callback: remove_cb(probe_state_reentrantly),
                refany: probe.clone(),
            })
            .into(),
            on_click: OptionChipOnClick::None,
        });
        {
            let mut p = probe
                .downcast_mut::<ReentrantProbe>()
                .expect("ReentrantProbe");
            p.state = state.clone();
        }

        let (update, changes) =
            run_remove(Some(removable_styled_dom()), REMOVE_NODE, state.clone());

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)]
        );

        let p = probe
            .downcast_ref::<ReentrantProbe>()
            .expect("ReentrantProbe");
        assert_eq!(p.calls, 1, "the user callback must have run exactly once");
        assert_eq!(
            p.saw_state, None,
            "a re-entrant read of the state must be refused, not aliased"
        );
    }

    #[test]
    fn remove_end_to_end_through_the_real_dom_payload() {
        // Take the *actual* RefAny the widget wired into its x and drive the
        // *actual* handler the widget registered against it.
        let chip = Chip::create(AzString::from("bye")).with_removable(true);
        let dom = chip.dom();
        let remove = &dom.children.as_ref()[1];
        let entry = &remove.root.get_callbacks().as_ref()[0];
        assert_eq!(entry.callback.cb, default_on_chip_remove as usize);
        let mut payload = entry.refany.clone();

        let styled = StyledDom::create_from_dom(dom);
        let (update, changes) = run_remove(Some(styled), REMOVE_NODE, payload.clone());

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)]
        );
        assert!(
            !wrapper_visible(&mut payload),
            "the state living in the DOM must be flipped to hidden"
        );
    }

    // ------------------------------------------------------------------
    // default_on_chip_click
    // ------------------------------------------------------------------

    #[test]
    fn click_without_a_user_callback_is_a_noop() {
        let mut data = RefAny::new(ChipStateWrapper::default());

        let (update, changes) = run_click(Some(removable_styled_dom()), LABEL_NODE, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "a click must never restyle anything");
        assert!(wrapper_visible(&mut data), "a click must not hide the chip");
    }

    #[test]
    fn click_invokes_the_user_callback_with_the_current_state() {
        let (mut data, mut log) = state_with_click_log(true);

        let (update, changes) = run_click(Some(removable_styled_dom()), LABEL_NODE, data.clone());

        assert_eq!(
            update,
            Update::RefreshDom,
            "the user callback's Update is returned"
        );
        assert_eq!(
            log_calls(&mut log),
            alloc::vec![true, true],
            "the callback must see the *unmodified* state (still visible)"
        );
        assert!(
            wrapper_visible(&mut data),
            "on_click must not flip `visible`"
        );
        assert!(
            changes.is_empty(),
            "on_click must not write any CSS property"
        );
    }

    #[test]
    fn click_reports_a_hidden_chip_as_hidden() {
        // The label and the x share one state, so a click after a remove must
        // observe `visible == false`.
        let (data, mut log) = state_with_click_log(false);

        let (update, _) = run_click(Some(removable_styled_dom()), LABEL_NODE, data);

        assert_eq!(update, Update::RefreshDom);
        assert_eq!(log_calls(&mut log), alloc::vec![false, false]);
    }

    #[test]
    fn click_does_not_need_the_node_hierarchy() {
        // Unlike the remove handler, `default_on_chip_click` never walks the tree:
        // it must still fire with no layout result at all, and from the root node.
        let (data, mut log) = state_with_click_log(true);

        let (update, changes) = run_click(None, 0, data);

        assert_eq!(
            update,
            Update::RefreshDom,
            "a missing layout result must not suppress on_click"
        );
        assert!(changes.is_empty());
        assert_eq!(log_calls(&mut log), alloc::vec![true, true]);
    }

    #[test]
    fn click_with_a_foreign_payload_is_a_noop() {
        let data = RefAny::new(0xdead_beef_u64);

        let (update, changes) = run_click(Some(removable_styled_dom()), LABEL_NODE, data);

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn click_twice_is_stable_and_never_mutates_the_state() {
        let (mut data, mut log) = state_with_click_log(true);

        for _ in 0..3 {
            let (update, changes) =
                run_click(Some(removable_styled_dom()), LABEL_NODE, data.clone());
            assert_eq!(update, Update::RefreshDom);
            assert!(changes.is_empty());
        }

        assert!(
            wrapper_visible(&mut data),
            "repeated clicks must not hide the chip"
        );
        assert_eq!(
            log_calls(&mut log).len(),
            6,
            "each click fires the callback exactly once"
        );
    }

    #[test]
    fn click_end_to_end_through_the_real_dom_payload() {
        let log = RefAny::new(StateLog { calls: Vec::new() });
        let mut log_handle = log.clone();
        let chip = Chip::create(AzString::from("tag"))
            .with_removable(true)
            .with_on_click(log, click_cb(record_click));
        let dom = chip.dom();

        let label = &dom.children.as_ref()[0];
        let entry = &label.root.get_callbacks().as_ref()[0];
        assert_eq!(entry.callback.cb, default_on_chip_click as usize);
        let mut payload = entry.refany.clone();

        let styled = StyledDom::create_from_dom(dom);
        let (update, changes) = run_click(Some(styled), LABEL_NODE, payload.clone());

        assert_eq!(update, Update::RefreshDom);
        assert!(changes.is_empty());
        assert_eq!(log_calls(&mut log_handle), alloc::vec![true, true]);
        assert!(
            wrapper_visible(&mut payload),
            "the shared state must be untouched by a click"
        );
    }

    #[test]
    fn remove_then_click_through_the_shared_dom_state_sees_the_hidden_chip() {
        // The end-to-end consequence of the shared `RefAny`: once the x has been
        // clicked, the label's handler observes `visible == false`.
        let log = RefAny::new(StateLog { calls: Vec::new() });
        let mut log_handle = log.clone();
        let chip = Chip::create(AzString::from("tag"))
            .with_removable(true)
            .with_on_click(log, click_cb(record_click));
        let dom = chip.dom();

        let click_payload = dom.children.as_ref()[0].root.get_callbacks().as_ref()[0]
            .refany
            .clone();
        let remove_payload = dom.children.as_ref()[1].root.get_callbacks().as_ref()[0]
            .refany
            .clone();

        let styled = StyledDom::create_from_dom(dom);
        let (_, changes) = run_remove(Some(styled), REMOVE_NODE, remove_payload);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)]
        );

        let (update, _) = run_click(Some(removable_styled_dom()), LABEL_NODE, click_payload);
        assert_eq!(update, Update::RefreshDom);
        assert_eq!(
            log_calls(&mut log_handle),
            alloc::vec![false, false],
            "after the x was clicked, the label handler must see a hidden chip"
        );
    }
}
