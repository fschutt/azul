//! Card widget — an elevated, bordered content container with rounded corners,
//! a soft drop shadow and padding, holding arbitrary child content. A near-clone
//! of [`crate::widgets::frame::Frame`] (a container) but without the fieldset
//! title/header — just a single styled box wrapping the body content.
//!
//! Key types: [`Card`], [`CardOnClick`].

use azul_core::{
    callbacks::Update,
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
use azul_css::css::BoxOrStatic;
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{ColorU, PixelValueNoPercent, PixelValue, FloatValue},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutFlexGrow},
        property::{CssProperty, StyleBoxShadowValue, LayoutFlexGrowValue},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, StyleBoxShadow, BoxShadowClipMode, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderLeftStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius},
    },
    impl_option_inner, AzString,
};

use crate::callbacks::CallbackInfo;

/// Card border colour (#dee2e6).
const CARD_BORDER_COLOR: ColorU = ColorU {
    r: 222,
    g: 226,
    b: 230,
    a: 255,
};
/// Card background colour (white).
const CARD_BG_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
/// Soft drop-shadow colour (black @ ~15% alpha).
const CARD_SHADOW_COLOR: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 38,
};

const CARD_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(CARD_BG_COLOR)];
const CARD_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(CARD_BG_ITEMS);

/// Shared drop-shadow descriptor referenced by all four edge box-shadows.
static CARD_SHADOW: StyleBoxShadow = StyleBoxShadow {
    offset_x: PixelValueNoPercent {
        inner: PixelValue::const_px(0),
    },
    offset_y: PixelValueNoPercent {
        inner: PixelValue::const_px(2),
    },
    blur_radius: PixelValueNoPercent {
        inner: PixelValue::const_px(6),
    },
    spread_radius: PixelValueNoPercent {
        inner: PixelValue::const_px(0),
    },
    clip_mode: BoxShadowClipMode::Outset,
    color: CARD_SHADOW_COLOR,
};

const CARD_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(CARD_BG)),
    // padding: 12px
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
        inner: CARD_BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: CARD_BORDER_COLOR,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: CARD_BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: CARD_BORDER_COLOR,
        },
    )),
    // border-radius: 8px
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(8),
    )),
    // soft drop shadow on all four edges
    CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&raw const CARD_SHADOW),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&raw const CARD_SHADOW),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&raw const CARD_SHADOW),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&raw const CARD_SHADOW),
    ))),
];

/// An elevated, bordered content container with rounded corners, a soft drop
/// shadow and padding. Holds arbitrary child content (`content`).
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct Card {
    /// The body content rendered inside the card.
    pub content: Dom,
    /// `flex-grow` factor applied to the card container.
    pub flex_grow: f32,
    /// Optional: Function to call when the card is clicked
    pub on_click: OptionCardOnClick,
}

/// Callback function type invoked when the card container is clicked.
pub type CardOnClickCallbackType = extern "C" fn(RefAny, CallbackInfo) -> Update;
impl_widget_callback!(
    CardOnClick,
    OptionCardOnClick,
    CardOnClickCallback,
    CardOnClickCallbackType
);

// Host-invoker plumbing for managed-FFI bindings — see core/src/host_invoker.rs.
azul_core::impl_managed_callback! {
    wrapper:        CardOnClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: CARD_ON_CLICK_INVOKER,
    invoker_ty:     AzCardOnClickCallbackInvoker,
    thunk_fn:       az_card_on_click_callback_thunk,
    setter_fn:      AzApp_setCardOnClickCallbackInvoker,
    from_handle_fn: AzCardOnClickCallback_createFromHostHandle,
}

impl Card {
    /// Creates a new `Card` wrapping the given content DOM.
    #[must_use] pub const fn create(content: Dom) -> Self {
        Self {
            content,
            flex_grow: 0.0,
            on_click: OptionCardOnClick::None,
        }
    }

    /// Replaces `self` with an empty default card and returns the original.
    #[must_use] pub const fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(Dom::create_div());
        core::mem::swap(&mut s, self);
        s
    }

    /// Sets the body content.
    pub fn set_content(&mut self, content: Dom) {
        self.content = content;
    }

    /// Builder-style setter for the body content.
    #[must_use] pub fn with_content(mut self, content: Dom) -> Self {
        self.set_content(content);
        self
    }

    /// Sets the flex-grow factor for the card container.
    pub const fn set_flex_grow(&mut self, flex_grow: f32) {
        self.flex_grow = flex_grow;
    }

    /// Builder-style setter for the flex-grow factor.
    #[must_use] pub const fn with_flex_grow(mut self, flex_grow: f32) -> Self {
        self.set_flex_grow(flex_grow);
        self
    }

    /// Sets the click callback, invoked when the card container is clicked.
    pub fn set_on_click<C: Into<CardOnClickCallback>>(&mut self, data: RefAny, on_click: C) {
        self.on_click = Some(CardOnClick {
            refany: data,
            callback: on_click.into(),
        })
        .into();
    }

    /// Builder-style setter for the click callback.
    #[must_use] pub fn with_on_click<C: Into<CardOnClickCallback>>(
        mut self,
        data: RefAny,
        on_click: C,
    ) -> Self {
        self.set_on_click(data, on_click);
        self
    }

    #[must_use] pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{EventFilter, HoverEventFilter},
        };

        static CARD_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-card"))];

        // Optional click callback on the card's root container (same wiring
        // as button's on_click).
        let callbacks = match self.on_click.into_option() {
            Some(CardOnClick {
                refany: data,
                callback,
            }) => vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                callback: CoreCallback {
                    cb: callback.cb as *const () as usize,
                    ctx: callback.ctx,
                },
                refany: data,
            }],
            None => Vec::new(),
        };

        // Prepend the (param-dependent) flex-grow, then the static card style.
        let mut props = vec![CssPropertyWithConditions::simple(CssProperty::FlexGrow(
            LayoutFlexGrowValue::Exact(LayoutFlexGrow {
                inner: FloatValue::new(self.flex_grow),
            }),
        ))];
        props.extend_from_slice(CARD_STYLE);

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CARD_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(props))
            .with_callbacks(callbacks.into())
            .with_children(vec![self.content].into())
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::create(Dom::create_div())
    }
}

impl From<Card> for Dom {
    fn from(c: Card) -> Self {
        c.dom()
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::unreadable_literal,
    clippy::too_many_lines
)]
mod autotest_generated {
    use azul_core::dom::{EventFilter, HoverEventFilter, NodeType};
    use azul_css::props::basic::length::SizeMetric;

    use super::*;
    use crate::callbacks::Callback;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    extern "C" fn click_a(_data: RefAny, _info: CallbackInfo) -> Update {
        Update::DoNothing
    }

    extern "C" fn click_b(_data: RefAny, _info: CallbackInfo) -> Update {
        Update::RefreshDom
    }

    /// Content DOMs a caller can realistically hand to a card: empty text, a NUL byte,
    /// combining marks, a ZWJ emoji sequence, RTL text and a lone surrogate-ish escape.
    /// The card must carry all of them through to the DOM byte-for-byte — it never parses
    /// or normalises its content.
    const ADVERSARIAL_TEXT: [&str; 6] = [
        "",
        "a\0b",
        "e\u{0301}\u{0301}\u{0301}",
        "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        "\u{202E}gnirts desrever\u{202C}",
        "\u{FFFD}\u{FEFF}\t\n",
    ];

    /// Every f32 the numeric surface of `set_flex_grow` has to survive. `FloatValue::new`
    /// multiplies by 1000 and casts to `isize` — NaN, the infinities and `f32::MAX` all
    /// hit the saturating-cast path, and anything under 0.001 truncates away.
    const ADVERSARIAL_FLOATS: [f32; 14] = [
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.001,
        -0.001,
        f32::EPSILON,
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        f32::MAX,
        f32::MIN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];

    /// The declared properties of a rendered node's inline style, in declaration order.
    fn inline_props(dom: &Dom) -> Vec<CssProperty> {
        dom.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// The CSS classes of a rendered node, in declaration order.
    fn classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                IdOrClass::Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    /// The `flex-grow` factor as it actually lands in the style tree — i.e. *after* the
    /// lossy `f32 -> isize` encoding inside `FloatValue::new`.
    fn dom_flex_grow(dom: &Dom) -> Option<f32> {
        dom.root
            .style
            .iter_inline_properties()
            .find_map(|(p, _)| match p {
                CssProperty::FlexGrow(v) => v.get_property().map(|f| f.inner.get()),
                _ => None,
            })
    }

    /// The `f32` of a `PixelValue`, asserting the length is an absolute `px`. An `em`/`%`
    /// slipping into the card geometry would resolve against the parent font/box instead
    /// of the intended fixed padding, border or radius.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "card geometry must be absolute px, got {:?}",
            pv.metric
        );
        pv.number.get()
    }

    /// The recursive descendant count. `Dom::estimated_total_children` is a *cached* value
    /// that, if too small, makes `convert_dom_into_compact_dom` under-allocate its arenas
    /// and panic on out-of-bounds writes — so it has to match this exactly.
    fn count_descendants(dom: &Dom) -> usize {
        dom.children
            .as_ref()
            .iter()
            .map(|c| 1 + count_descendants(c))
            .sum()
    }

    /// A chain of `depth` nested divs (depth kept modest: `Dom`'s `Drop`/`PartialEq` recurse).
    fn nested_divs(depth: usize) -> Dom {
        let mut d = Dom::create_div();
        for _ in 0..depth {
            d = Dom::create_div().with_child(d);
        }
        d
    }

    fn text_of(dom: &Dom) -> Option<&str> {
        match dom.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            _ => None,
        }
    }

    // ------------------------------------------------------------------
    // Card::create
    // ------------------------------------------------------------------

    #[test]
    fn create_zeroes_the_numeric_field_and_leaves_the_callback_unset() {
        let c = Card::create(Dom::create_div());

        // Positive zero, not -0.0: a negative zero would flip the sign of the encoded
        // isize on some paths and is not what "no growth" means.
        assert_eq!(c.flex_grow.to_bits(), 0_u32, "flex_grow must start at +0.0");
        assert!(c.on_click.is_none(), "a fresh card must have no callback");
        assert_eq!(c.content, Dom::create_div(), "the content was not stored verbatim");
    }

    #[test]
    fn create_with_a_div_is_exactly_the_default_card() {
        assert_eq!(
            Card::create(Dom::create_div()),
            Card::default(),
            "Default and create(div) drifted apart",
        );
    }

    #[test]
    fn create_stores_pathological_content_verbatim() {
        for t in ADVERSARIAL_TEXT {
            let c = Card::create(Dom::create_text(t));
            assert_eq!(
                text_of(&c.content),
                Some(t),
                "the card mangled or normalised its text content",
            );

            // ... and it survives the trip into the rendered DOM.
            let dom = c.dom();
            let child = &dom.children.as_ref()[0];
            assert_eq!(text_of(child), Some(t), "the text was corrupted on the way into the DOM");
        }
    }

    #[test]
    fn create_accepts_deeply_nested_and_very_wide_content() {
        let deep = Card::create(nested_divs(64)).dom();
        assert_eq!(
            deep.estimated_total_children,
            count_descendants(&deep),
            "the cached child count desynced for deeply nested content",
        );

        let wide = Card::create(
            Dom::create_div().with_children((0..2000).map(|_| Dom::create_div()).collect::<Vec<_>>().into()),
        )
        .dom();
        assert_eq!(
            wide.estimated_total_children,
            count_descendants(&wide),
            "the cached child count desynced for very wide content",
        );
        assert_eq!(wide.estimated_total_children, 2001, "card div + 2000 grandchildren expected");
    }

    // ------------------------------------------------------------------
    // Card::swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_moves_every_field_out_and_leaves_a_default() {
        let mut c = Card::create(Dom::create_text("payload")).with_flex_grow(2.5);
        c.set_on_click(RefAny::new(7u32), click_a as CardOnClickCallbackType);

        let taken = c.swap_with_default();

        assert_eq!(text_of(&taken.content), Some("payload"), "the content did not travel out");
        assert_eq!(taken.flex_grow, 2.5, "flex_grow did not travel out");
        assert!(taken.on_click.is_some(), "the callback did not travel out");

        assert_eq!(c, Card::default(), "what was left behind is not a default card");
        assert!(c.on_click.is_none(), "the swapped-in default still carries a callback");
        assert_eq!(c.flex_grow.to_bits(), 0_u32, "the swapped-in default has a non-zero flex_grow");
    }

    #[test]
    fn repeated_swap_with_default_never_accumulates_state() {
        let mut c = Card::create(Dom::create_text("x")).with_flex_grow(1.0);
        let _first = c.swap_with_default();

        for i in 0..8 {
            let taken = c.swap_with_default();
            assert_eq!(taken, Card::default(), "swap #{i} handed back something other than a default");
            assert_eq!(c, Card::default(), "swap #{i} left something other than a default behind");
        }

        // The DOM of the drained card is still a well-formed one-child card.
        let dom = c.dom();
        assert_eq!(dom.children.as_ref().len(), 1);
        assert_eq!(dom_flex_grow(&dom), Some(0.0));
    }

    // ------------------------------------------------------------------
    // Card::set_content / Card::with_content
    // ------------------------------------------------------------------

    #[test]
    fn set_content_replaces_rather_than_appends() {
        let mut c = Card::create(Dom::create_text("first"));
        c.set_content(Dom::create_text("second"));
        c.set_content(Dom::create_text("third"));

        assert_eq!(text_of(&c.content), Some("third"), "the last content did not win");

        let dom = c.dom();
        assert_eq!(
            dom.children.as_ref().len(),
            1,
            "re-setting the content appended a child instead of replacing it",
        );
        assert_eq!(text_of(&dom.children.as_ref()[0]), Some("third"));
    }

    #[test]
    fn with_content_touches_only_the_content_field() {
        let base = Card::create(Dom::create_text("old")).with_flex_grow(3.0);
        let c = base.with_content(Dom::create_text("new"));

        assert_eq!(text_of(&c.content), Some("new"));
        assert_eq!(c.flex_grow, 3.0, "with_content clobbered flex_grow");
        assert!(c.on_click.is_none(), "with_content invented a callback");
    }

    #[test]
    fn with_content_preserves_an_already_installed_callback() {
        let c = Card::default()
            .with_on_click(RefAny::new(1u8), click_a as CardOnClickCallbackType)
            .with_content(Dom::create_text("late content"));

        assert!(c.on_click.is_some(), "with_content dropped the callback");
        let dom = c.dom();
        assert_eq!(dom.root.callbacks.as_ref().len(), 1, "the callback was lost on the way into the DOM");
        assert_eq!(text_of(&dom.children.as_ref()[0]), Some("late content"));
    }

    // ------------------------------------------------------------------
    // Card::set_flex_grow / Card::with_flex_grow  (numeric)
    // ------------------------------------------------------------------

    #[test]
    fn set_flex_grow_stores_the_bit_pattern_verbatim_without_sanitising() {
        // The setter is a plain assignment — it must not clamp, round or NaN-scrub.
        // (Sanitisation happens later, at encode time; see the tests below.)
        for v in ADVERSARIAL_FLOATS {
            let mut c = Card::default();
            c.set_flex_grow(v);
            if v.is_nan() {
                assert!(c.flex_grow.is_nan(), "a NaN flex_grow was silently rewritten");
            } else {
                assert_eq!(c.flex_grow.to_bits(), v.to_bits(), "flex_grow {v} was not stored verbatim");
            }
        }
    }

    #[test]
    fn with_flex_grow_is_exactly_set_flex_grow() {
        for v in ADVERSARIAL_FLOATS {
            let mut by_setter = Card::default();
            by_setter.set_flex_grow(v);
            let by_builder = Card::default().with_flex_grow(v);

            assert_eq!(
                by_setter.flex_grow.to_bits(),
                by_builder.flex_grow.to_bits(),
                "the builder and the setter disagree for {v}",
            );
        }
    }

    #[test]
    fn with_flex_grow_touches_only_the_numeric_field() {
        let c = Card::create(Dom::create_text("body"))
            .with_on_click(RefAny::new(1u8), click_a as CardOnClickCallbackType)
            .with_flex_grow(f32::NAN);

        assert_eq!(text_of(&c.content), Some("body"), "with_flex_grow clobbered the content");
        assert!(c.on_click.is_some(), "with_flex_grow dropped the callback");
    }

    #[test]
    fn a_nan_flex_grow_breaks_the_derived_equality_of_the_card() {
        // `Card` derives `PartialEq` over a raw `f32`, so a NaN factor makes a card
        // unequal to its own clone. Callers cannot use `==` as a "was it modified?"
        // probe once a NaN is in there — pinned so nobody relies on the opposite.
        let c = Card::default().with_flex_grow(f32::NAN);
        assert_ne!(c, c.clone(), "NaN equality semantics changed");

        // Every non-NaN factor keeps equality reflexive.
        for v in ADVERSARIAL_FLOATS.iter().copied().filter(|v| !v.is_nan()) {
            let c = Card::default().with_flex_grow(v);
            assert_eq!(c, c.clone(), "a card with flex_grow {v} is not equal to its own clone");
        }
    }

    #[test]
    fn flex_grow_zero_encodes_to_positive_zero_even_from_negative_zero() {
        for v in [0.0_f32, -0.0_f32] {
            let dom = Card::default().with_flex_grow(v).dom();
            let got = dom_flex_grow(&dom).expect("flex-grow must always be declared");
            assert_eq!(got.to_bits(), 0_u32, "flex_grow {v} did not encode to +0.0 (got {got})");
        }
    }

    #[test]
    fn flex_grow_round_trips_through_the_dom_at_milli_precision() {
        // FloatValue keeps 3 decimal places (x1000, truncating cast), so any factor that is
        // a whole multiple of 0.001 must come back out of the DOM unchanged.
        for v in [0.0_f32, 0.001, 0.5, 1.0, 2.5, 3.0, -1.5, 1000.0, 65536.0] {
            let got = dom_flex_grow(&Card::default().with_flex_grow(v).dom())
                .expect("flex-grow must always be declared");
            assert!(
                (got - v).abs() <= 0.001,
                "flex_grow {v} did not survive the FloatValue encoding (got {got})",
            );
        }
    }

    #[test]
    fn flex_grow_below_the_encoding_precision_truncates_to_zero() {
        // Everything under one milli-unit is quantised away — including the sign, because
        // the truncating cast of -0.0001 * 1000 = -0.1 lands on integer 0.
        for v in [f32::EPSILON, f32::MIN_POSITIVE, -f32::MIN_POSITIVE, 1e-4, -1e-4, 0.0005, -0.0009] {
            let got = dom_flex_grow(&Card::default().with_flex_grow(v).dom())
                .expect("flex-grow must always be declared");
            assert_eq!(got.to_bits(), 0_u32, "sub-milli flex_grow {v} did not truncate to +0.0 (got {got})");
        }

        // ... and 0.001 is genuinely the smallest factor that still registers.
        let smallest = dom_flex_grow(&Card::default().with_flex_grow(0.001).dom()).unwrap();
        assert!(smallest > 0.0, "0.001 is supposed to be the smallest representable factor");
    }

    #[test]
    fn a_nan_flex_grow_does_not_panic_and_lands_on_zero() {
        let dom = Card::default().with_flex_grow(f32::NAN).dom();
        let got = dom_flex_grow(&dom).expect("flex-grow must always be declared");
        assert!(!got.is_nan(), "a NaN flex-grow reached the style tree");
        assert_eq!(got.to_bits(), 0_u32, "NaN must encode to +0.0 (saturating cast), got {got}");
    }

    #[test]
    fn infinite_and_maximal_flex_grow_saturates_instead_of_overflowing() {
        // `FloatValue::new` does `(v * 1000.0) as isize`; f32::MAX * 1000 already overflows to
        // +inf, so MAX and INFINITY have to land on the same saturated bound. The point is that
        // nothing wraps around into a *negative* factor.
        let max_encoded = (isize::MAX as f32) / 1000.0;
        let min_encoded = (isize::MIN as f32) / 1000.0;

        for v in [f32::INFINITY, f32::MAX] {
            let got = dom_flex_grow(&Card::default().with_flex_grow(v).dom()).unwrap();
            assert!(got.is_finite(), "an infinite flex-grow reached the style tree for {v}");
            assert_eq!(got, max_encoded, "{v} did not saturate at the isize upper bound");
            assert!(got > 0.0, "{v} wrapped around into a non-positive factor");
        }

        for v in [f32::NEG_INFINITY, f32::MIN] {
            let got = dom_flex_grow(&Card::default().with_flex_grow(v).dom()).unwrap();
            assert!(got.is_finite(), "an infinite flex-grow reached the style tree for {v}");
            assert_eq!(got, min_encoded, "{v} did not saturate at the isize lower bound");
            assert!(got < 0.0, "{v} wrapped around into a non-negative factor");
        }
    }

    #[test]
    fn no_flex_grow_input_can_put_a_nan_or_an_infinity_into_the_style_tree() {
        for v in ADVERSARIAL_FLOATS {
            let dom = Card::default().with_flex_grow(v).dom();
            let got = dom_flex_grow(&dom).unwrap_or_else(|| panic!("flex-grow disappeared for input {v}"));
            assert!(
                got.is_finite(),
                "input {v} produced a non-finite flex-grow ({got}) — the layout solver would NaN out",
            );
        }
    }

    #[test]
    fn flex_grow_encoding_is_monotonic() {
        // A sign- or rounding-bug in the x1000 cast would show up as an inversion here.
        let ascending = [-1000.0_f32, -1.5, -0.001, 0.0, 0.001, 1.5, 1000.0];
        let mut prev = f32::NEG_INFINITY;
        for v in ascending {
            let got = dom_flex_grow(&Card::default().with_flex_grow(v).dom()).unwrap();
            assert!(got >= prev, "encoding is not monotonic: {v} encoded to {got}, below the previous {prev}");
            prev = got;
        }
    }

    // ------------------------------------------------------------------
    // Card::set_on_click / Card::with_on_click
    // ------------------------------------------------------------------

    #[test]
    fn set_on_click_replaces_the_previous_callback_instead_of_appending() {
        let mut c = Card::default();
        assert!(c.on_click.is_none());

        c.set_on_click(RefAny::new(1u32), click_a as CardOnClickCallbackType);
        assert!(c.on_click.is_some(), "set_on_click did not store the callback");

        c.set_on_click(RefAny::new(2u32), click_b as CardOnClickCallbackType);
        let stored = c.on_click.as_ref().expect("callback must still be present");
        assert_eq!(
            stored.callback.cb as *const () as usize,
            click_b as CardOnClickCallbackType as *const () as usize,
            "the second set_on_click did not replace the first",
        );

        let dom = c.dom();
        assert_eq!(
            dom.root.callbacks.as_ref().len(),
            1,
            "a re-set callback was appended instead of replaced — the card would fire twice",
        );
    }

    #[test]
    fn with_on_click_round_trips_the_function_pointer_and_the_payload_into_the_dom() {
        let cb: CardOnClickCallbackType = click_a;
        let expected_ptr = cb as *const () as usize;

        let dom = Card::create(Dom::create_text("clickable"))
            .with_on_click(RefAny::new(0xDEAD_BEEF_u32), cb)
            .dom();

        let callbacks = dom.root.callbacks.as_ref();
        assert_eq!(callbacks.len(), 1, "exactly one click callback is expected");
        assert_eq!(
            callbacks[0].event,
            EventFilter::Hover(HoverEventFilter::MouseUp),
            "the card must fire on mouse-up, not on any other filter",
        );
        assert_eq!(
            callbacks[0].callback.cb, expected_ptr,
            "the fn pointer was corrupted on the way into the DOM",
        );

        // The RefAny payload survives the move into the DOM, and a wrong-type downcast
        // must fail rather than reinterpret the bytes.
        let mut data = callbacks[0].refany.clone();
        assert_eq!(
            *data.downcast_ref::<u32>().expect("payload changed type"),
            0xDEAD_BEEF,
            "payload was corrupted",
        );
        assert!(data.downcast_ref::<u64>().is_none(), "a wrong-type downcast reinterpreted the payload");
    }

    #[test]
    fn with_on_click_accepts_a_generic_callback_without_mangling_the_pointer() {
        // The `From<Callback>` arm transmutes the fn pointer — this is the FFI path
        // (Python/C) into the same slot, so the pointer must come out untouched.
        let generic = Callback {
            cb: click_a,
            ctx: azul_core::refany::OptionRefAny::None,
        };
        let raw: CardOnClickCallbackType = click_a;
        let expected_ptr = raw as *const () as usize;

        let dom = Card::default().with_on_click(RefAny::new(1u8), generic).dom();
        let callbacks = dom.root.callbacks.as_ref();
        assert_eq!(callbacks.len(), 1);
        assert_eq!(
            callbacks[0].callback.cb, expected_ptr,
            "the Callback -> CardOnClickCallback transmute mangled the pointer",
        );
        assert!(callbacks[0].callback.ctx.is_none(), "a native callback must carry no FFI context");
    }

    #[test]
    fn a_card_without_a_callback_registers_no_callbacks() {
        let dom = Card::create(Dom::create_text("inert")).with_flex_grow(1.0).dom();
        assert!(
            dom.root.callbacks.as_ref().is_empty(),
            "a callback appeared on a card that was never given one",
        );
    }

    #[test]
    fn set_on_click_does_not_disturb_the_other_fields() {
        let mut c = Card::create(Dom::create_text("body")).with_flex_grow(2.0);
        c.set_on_click(RefAny::new("payload".to_string()), click_a as CardOnClickCallbackType);

        assert_eq!(text_of(&c.content), Some("body"), "set_on_click clobbered the content");
        assert_eq!(c.flex_grow, 2.0, "set_on_click clobbered flex_grow");

        let dom = c.dom();
        assert_eq!(dom_flex_grow(&dom), Some(2.0));
        let mut data = dom.root.callbacks.as_ref()[0].refany.clone();
        let payload = data.downcast_ref::<String>().expect("payload changed type");
        assert_eq!(payload.as_str(), "payload", "the RefAny payload was corrupted");
    }

    // ------------------------------------------------------------------
    // Card::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_builds_a_single_card_div_holding_the_content_as_its_only_child() {
        let dom = Card::create(Dom::create_text("body")).dom();

        assert_eq!(*dom.root.get_node_type(), NodeType::Div, "the card container must be a div");
        assert_eq!(
            classes(&dom),
            vec!["__azul-native-card".to_string()],
            "the card class is what the UA stylesheet and tests key off",
        );
        assert_eq!(dom.children.as_ref().len(), 1, "the card must wrap exactly one child");
        assert_eq!(text_of(&dom.children.as_ref()[0]), Some("body"));
        assert_eq!(
            dom.estimated_total_children,
            count_descendants(&dom),
            "the cached child count is inconsistent — compact-DOM conversion would over/under-allocate",
        );
    }

    #[test]
    fn dom_prepends_the_flex_grow_declaration_to_the_static_card_style() {
        let dom = Card::default().with_flex_grow(4.0).dom();
        let props = inline_props(&dom);

        assert_eq!(
            props.len(),
            1 + CARD_STYLE.len(),
            "the card's inline style must be exactly flex-grow + the static card style",
        );
        assert!(
            matches!(props[0], CssProperty::FlexGrow(_)),
            "flex-grow must come first, so nothing in the static style can shadow it",
        );
        assert_eq!(
            props.iter().filter(|p| matches!(p, CssProperty::FlexGrow(_))).count(),
            1,
            "flex-grow was declared more than once",
        );
    }

    #[test]
    fn dom_carries_the_static_card_geometry() {
        let props = inline_props(&Card::default().dom());

        let mut paddings = Vec::new();
        let mut border_widths = Vec::new();
        let mut radii = Vec::new();
        let mut border_colors = Vec::new();
        let mut border_styles = Vec::new();
        let mut display = None;
        let mut direction = None;
        let mut background = None;

        for p in &props {
            match p {
                CssProperty::PaddingTop(v) => paddings.push(v.get_property().map(|x| px(&x.inner))),
                CssProperty::PaddingBottom(v) => paddings.push(v.get_property().map(|x| px(&x.inner))),
                CssProperty::PaddingLeft(v) => paddings.push(v.get_property().map(|x| px(&x.inner))),
                CssProperty::PaddingRight(v) => paddings.push(v.get_property().map(|x| px(&x.inner))),
                CssProperty::BorderTopWidth(v) => border_widths.push(v.get_property().map(|x| px(&x.inner))),
                CssProperty::BorderBottomWidth(v) => border_widths.push(v.get_property().map(|x| px(&x.inner))),
                CssProperty::BorderLeftWidth(v) => border_widths.push(v.get_property().map(|x| px(&x.inner))),
                CssProperty::BorderRightWidth(v) => border_widths.push(v.get_property().map(|x| px(&x.inner))),
                CssProperty::BorderTopLeftRadius(v) => radii.push(v.get_property().map(|x| px(&x.inner))),
                CssProperty::BorderTopRightRadius(v) => radii.push(v.get_property().map(|x| px(&x.inner))),
                CssProperty::BorderBottomLeftRadius(v) => radii.push(v.get_property().map(|x| px(&x.inner))),
                CssProperty::BorderBottomRightRadius(v) => radii.push(v.get_property().map(|x| px(&x.inner))),
                CssProperty::BorderTopColor(v) => border_colors.push(v.get_property().map(|x| x.inner)),
                CssProperty::BorderBottomColor(v) => border_colors.push(v.get_property().map(|x| x.inner)),
                CssProperty::BorderLeftColor(v) => border_colors.push(v.get_property().map(|x| x.inner)),
                CssProperty::BorderRightColor(v) => border_colors.push(v.get_property().map(|x| x.inner)),
                CssProperty::BorderTopStyle(v) => border_styles.push(v.get_property().map(|x| x.inner)),
                CssProperty::BorderBottomStyle(v) => border_styles.push(v.get_property().map(|x| x.inner)),
                CssProperty::BorderLeftStyle(v) => border_styles.push(v.get_property().map(|x| x.inner)),
                CssProperty::BorderRightStyle(v) => border_styles.push(v.get_property().map(|x| x.inner)),
                CssProperty::Display(v) => display = v.get_property().copied(),
                CssProperty::FlexDirection(v) => direction = v.get_property().copied(),
                CssProperty::BackgroundContent(v) => {
                    background = v.get_property().map(|bg| bg.as_ref().to_vec());
                }
                _ => {}
            }
        }

        assert_eq!(paddings, vec![Some(12.0); 4], "all four paddings must be 12px");
        assert_eq!(border_widths, vec![Some(1.0); 4], "all four borders must be 1px");
        assert_eq!(radii, vec![Some(8.0); 4], "all four corners must be 8px");
        assert_eq!(border_colors, vec![Some(CARD_BORDER_COLOR); 4], "all four border colours must match");
        assert_eq!(border_styles, vec![Some(BorderStyle::Solid); 4], "all four border styles must be solid");
        assert_eq!(display, Some(LayoutDisplay::Flex), "the card container must be a flex box");
        assert_eq!(direction, Some(LayoutFlexDirection::Column), "the card must stack its content in a column");

        let bg = background.expect("the card must declare a background");
        assert_eq!(bg.len(), 1, "exactly one background layer expected");
        match &bg[0] {
            StyleBackgroundContent::Color(c) => assert_eq!(*c, CARD_BG_COLOR, "the card background is not white"),
            other => panic!("the card background must be a flat colour, got {other:?}"),
        }
    }

    #[test]
    fn dom_box_shadows_dereference_the_shared_static_descriptor() {
        let dom = Card::default().dom();

        let mut shadows = 0_usize;
        for (p, _) in dom.root.style.iter_inline_properties() {
            let value = match p {
                CssProperty::BoxShadowTop(v)
                | CssProperty::BoxShadowBottom(v)
                | CssProperty::BoxShadowLeft(v)
                | CssProperty::BoxShadowRight(v) => v.get_property(),
                _ => None,
            };
            let Some(boxed) = value else { continue };

            // This is the interesting bit: the property holds a raw `*const StyleBoxShadow`
            // taken with `&raw const CARD_SHADOW`. Dereferencing it must yield the static.
            let s: &StyleBoxShadow = boxed.as_ref();
            assert_eq!(px(&s.offset_x.inner), 0.0);
            assert_eq!(px(&s.offset_y.inner), 2.0);
            assert_eq!(px(&s.blur_radius.inner), 6.0);
            assert_eq!(px(&s.spread_radius.inner), 0.0);
            assert_eq!(s.clip_mode, BoxShadowClipMode::Outset);
            assert_eq!(s.color, CARD_SHADOW_COLOR);
            shadows += 1;
        }
        assert_eq!(shadows, 4, "the card must declare a shadow on all four edges");
    }

    #[test]
    fn dropping_card_doms_never_frees_the_static_shadow() {
        // Every card's four box-shadows are `BoxOrStatic::Static` pointers into the *same*
        // `CARD_SHADOW`. If a `Drop` ever treated one as `Boxed`, the second card's shadow
        // would be a use-after-free (and the third a double-free). Build, drop, re-read.
        let survivor = Card::default().dom();
        for _ in 0..64 {
            drop(Card::create(Dom::create_text("throwaway")).with_flex_grow(1.0).dom());
        }

        assert_eq!(CARD_SHADOW.offset_y.inner.number.get(), 2.0, "the static shadow was mutated or freed");
        assert_eq!(CARD_SHADOW.color, CARD_SHADOW_COLOR);

        let shadows = survivor
            .root
            .style
            .iter_inline_properties()
            .filter_map(|(p, _)| match p {
                CssProperty::BoxShadowTop(v)
                | CssProperty::BoxShadowBottom(v)
                | CssProperty::BoxShadowLeft(v)
                | CssProperty::BoxShadowRight(v) => v.get_property(),
                _ => None,
            })
            .map(|b| px(&b.as_ref().blur_radius.inner))
            .collect::<Vec<_>>();
        assert_eq!(shadows, vec![6.0; 4], "a surviving card's shadows were corrupted by other cards' drops");
    }

    #[test]
    fn dom_is_deterministic_and_never_mutates_the_shared_static_style() {
        let baseline = inline_props(&Card::default().dom());

        for i in 0..32 {
            let props = inline_props(&Card::default().dom());
            assert_eq!(props.len(), baseline.len(), "build #{i} produced a different number of properties");
            assert_eq!(props, baseline, "build #{i} diverged — the static CARD_STYLE was mutated");
        }
    }

    #[test]
    fn from_card_for_dom_is_exactly_dom() {
        let c = Card::create(Dom::create_text("body")).with_flex_grow(1.5);
        assert_eq!(Dom::from(c.clone()), c.dom(), "the From impl diverged from Card::dom");
    }

    #[test]
    fn cards_nest_without_desyncing_the_child_counts() {
        let inner = Card::create(Dom::create_text("inner")).with_flex_grow(1.0).dom();
        let outer = Card::create(inner).with_flex_grow(2.0).dom();

        assert_eq!(
            outer.estimated_total_children,
            count_descendants(&outer),
            "nesting cards desynced the cached child count",
        );
        assert_eq!(dom_flex_grow(&outer), Some(2.0), "the outer card's flex-grow was overwritten");
        assert_eq!(
            dom_flex_grow(&outer.children.as_ref()[0]),
            Some(1.0),
            "the inner card's flex-grow was overwritten",
        );
        assert_eq!(classes(&outer.children.as_ref()[0]), vec!["__azul-native-card".to_string()]);
    }
}
