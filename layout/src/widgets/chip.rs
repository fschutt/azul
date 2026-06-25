//! Chip / tag widget — a compact rounded "pill" holding a short label plus an
//! optional removable "×" affordance. A blend of
//! [`crate::widgets::badge::Badge`] (the coloured pill visual + [`ChipKind`]
//! colour variants) and [`crate::widgets::alert::Alert`] (the dismiss pattern:
//! a stateful close affordance that hides the widget on click).
//!
//! When made removable (`with_removable(true)` or `set_on_remove`), the chip
//! mirrors the stateful pattern of [`crate::widgets::alert::Alert`]: it carries a
//! [`ChipStateWrapper`] (`{ visible } + on_remove`) in a [`RefAny`] attached to
//! the "×" node. Clicking "×" flips `visible` to `false`, invokes the optional
//! user `on_remove`, and hides the whole chip by setting `display: none` on the
//! container via `set_css_property` (mirroring alert's live restyle). A
//! non-removable chip renders no "×" and carries no live callback — it is then
//! just a stateless styled pill (a near-clone of [`Badge`]).
//!
//! Key types: [`Chip`], [`ChipKind`], [`ChipState`], [`ChipOnRemove`].

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, font::{StyleFontFamily, StyleFontFamilyVec}, StyleFontSize},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutAlignItems, LayoutAlignSelf, LayoutFlexGrow, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutMarginLeft},
        property::{CssProperty, *},
        style::{StyleBackgroundContentVec, StyleBackgroundContent, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleTextColor, StyleTextAlign, StyleUserSelect, StyleCursor},
    },
    impl_option_inner, AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

static CHIP_CONTAINER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-chip"))];
static CHIP_LABEL_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-chip-label"))];
static CHIP_REMOVE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-chip-remove"))];

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

/// Callback function type invoked when a removable chip's "×" is clicked.
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
        const WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
        const DARK: ColorU = ColorU { r: 33, g: 37, b: 41, a: 255 };
        match self {
            // The default chip is a light neutral pill with dark text (the
            // common "tag" look), unlike Badge's solid grey.
            Self::Default => (ColorU { r: 233, g: 236, b: 239, a: 255 }, DARK),
            Self::Primary => (ColorU { r: 13, g: 110, b: 253, a: 255 }, WHITE),
            Self::Success => (ColorU { r: 25, g: 135, b: 84, a: 255 }, WHITE),
            Self::Danger => (ColorU { r: 220, g: 53, b: 69, a: 255 }, WHITE),
            Self::Warning => (ColorU { r: 255, g: 193, b: 7, a: 255 }, DARK),
            Self::Info => (ColorU { r: 13, g: 202, b: 240, a: 255 }, DARK),
        }
    }

    /// CSS class name for this chip kind (mirrors `BadgeKind::class_name`).
    #[must_use] pub const fn class_name(&self) -> &'static str {
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

/// A compact rounded pill holding a label plus an optional removable "×".
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Chip {
    /// Runtime state (`visible`) plus the optional remove callback.
    pub chip_state: ChipStateWrapper,
    /// The text shown inside the pill.
    pub label: AzString,
    /// The colour variant.
    pub kind: ChipKind,
    /// Whether to render the "×" remove affordance (hides the chip on click).
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
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
            4,
        ))),
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
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
        // Text colour is inherited by the label + "×" children.
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

/// "×" remove-affordance style: a small pointer-cursor box on the right.
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
    #[must_use] pub fn create(label: AzString) -> Self {
        Self::with_kind(label, ChipKind::Default)
    }

    /// Creates a new chip with the given label and colour variant.
    #[inline]
    #[must_use] pub fn with_kind(label: AzString, kind: ChipKind) -> Self {
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
    #[must_use] pub fn with_chip_kind(mut self, kind: ChipKind) -> Self {
        self.set_kind(kind);
        self
    }

    /// Sets whether the chip shows a "×" remove affordance.
    #[inline]
    pub const fn set_removable(&mut self, removable: bool) {
        self.removable = removable;
    }

    /// Builder-style setter for the removable flag.
    #[inline]
    #[must_use] pub const fn with_removable(mut self, removable: bool) -> Self {
        self.set_removable(removable);
        self
    }

    /// Sets the remove callback. Implies `removable = true` so the "×" is rendered.
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
    #[must_use] pub fn with_on_remove<C: Into<ChipOnRemoveCallback>>(
        mut self,
        data: RefAny,
        on_remove: C,
    ) -> Self {
        self.set_on_remove(data, on_remove);
        self
    }

    /// Replaces `self` with an empty default chip and returns the original.
    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this chip into a DOM subtree with the `__azul-native-chip` class.
    #[inline]
    #[must_use] pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::CoreCallback,
            dom::{EventFilter, HoverEventFilter},
            refany::OptionRefAny,
        };

        let label = Dom::create_text(self.label)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CHIP_LABEL_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(CHIP_LABEL_STYLE));

        let mut children = alloc::vec![label];

        if self.removable {
            let remove = Dom::create_text(AzString::from_const_str("\u{00D7}"))
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CHIP_REMOVE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(CHIP_REMOVE_STYLE))
                .with_tab_index(TabIndex::Auto)
                .with_callbacks(
                    alloc::vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseUp),
                        callback: CoreCallback {
                            cb: default_on_chip_remove as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: RefAny::new(self.chip_state),
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

/// "×" click handler. The hit node is the "×" (the callback-bearing node, per
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

impl From<Chip> for Dom {
    fn from(c: Chip) -> Self {
        c.dom()
    }
}
