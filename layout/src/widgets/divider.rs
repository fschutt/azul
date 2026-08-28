//! Divider (separator) widget — a thin rule line. A stateless single styled
//! node with no callback, a near-clone of [`crate::widgets::label::Label`].
//! Supports a horizontal (default) or vertical orientation.
//!
//! Key types: [`Divider`], [`DividerOrientation`].

use azul_core::dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::ColorU,
        layout::{
            LayoutAlignSelf, LayoutDisplay, LayoutFlexGrow, LayoutHeight, LayoutMarginBottom,
            LayoutMarginLeft, LayoutMarginRight, LayoutMarginTop, LayoutWidth,
        },
        property::{CssProperty, *},
        style::{StyleBackgroundContent, StyleBackgroundContentVec},
    },
    AzString,
};

/// Orientation of a [`Divider`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub enum DividerOrientation {
    /// A full-width horizontal rule (1px tall) — the default.
    #[default]
    Horizontal,
    /// A full-height vertical rule (1px wide).
    Vertical,
}

/// A thin separator rule. Stateless; renders a single styled `div`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Divider {
    pub orientation: DividerOrientation,
    pub divider_style: CssPropertyWithConditionsVec,
}

/// Default rule colour (#dddddd), matching the frame widget's border colour.
const DIVIDER_COLOR: ColorU = ColorU {
    r: 221,
    g: 221,
    b: 221,
    a: 255,
};
const DIVIDER_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(DIVIDER_COLOR)];
const DIVIDER_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(DIVIDER_BG_ITEMS);

static DIVIDER_STYLE_HORIZONTAL: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(1))),
    // Stretch across the parent's cross axis so the rule spans the full width.
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Stretch)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_margin_top(LayoutMarginTop::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_margin_bottom(
        LayoutMarginBottom::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(DIVIDER_BG)),
];

static DIVIDER_STYLE_VERTICAL: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(1))),
    // Stretch across the parent's cross axis so the rule spans the full height.
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Stretch)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(
        4,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_margin_right(
        LayoutMarginRight::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(DIVIDER_BG)),
];

impl Divider {
    /// Creates a new horizontal divider with default styling.
    #[inline]
    #[must_use]
    pub fn create() -> Self {
        Self::create_with_orientation(DividerOrientation::Horizontal)
    }

    /// Creates a new divider with the given orientation and default styling.
    #[inline]
    #[must_use]
    pub fn create_with_orientation(orientation: DividerOrientation) -> Self {
        let divider_style = match orientation {
            DividerOrientation::Horizontal => {
                CssPropertyWithConditionsVec::from_const_slice(DIVIDER_STYLE_HORIZONTAL)
            }
            DividerOrientation::Vertical => {
                CssPropertyWithConditionsVec::from_const_slice(DIVIDER_STYLE_VERTICAL)
            }
        };
        Self {
            orientation,
            divider_style,
        }
    }

    /// Sets the orientation, resetting the style to the matching default.
    #[inline]
    pub fn set_orientation(&mut self, orientation: DividerOrientation) {
        *self = Self::create_with_orientation(orientation);
    }

    /// Builder-style setter for the orientation.
    #[inline]
    #[must_use]
    pub fn with_orientation(mut self, orientation: DividerOrientation) -> Self {
        self.set_orientation(orientation);
        self
    }

    /// Replaces `self` with a default horizontal divider and returns the original.
    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this divider into a DOM node with the `__azul-native-divider` class.
    #[inline]
    #[must_use]
    pub fn dom(self) -> Dom {
        static DIVIDER_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-divider"))];

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(DIVIDER_CLASS))
            .with_css_props(self.divider_style)
    }
}

impl Default for Divider {
    fn default() -> Self {
        Self::create()
    }
}

impl From<Divider> for Dom {
    fn from(d: Divider) -> Self {
        d.dom()
    }
}

#[cfg(test)]
mod autotest_generated {
    use std::collections::HashSet;

    use azul_core::dom::NodeType;
    use azul_css::props::basic::{length::SizeMetric, pixel::PixelValue};

    use super::*;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Every variant of `DividerOrientation` — the complete input domain of
    /// `create_with_orientation`, `set_orientation` and `with_orientation`.
    const ALL_ORIENTATIONS: [DividerOrientation; 2] =
        [DividerOrientation::Horizontal, DividerOrientation::Vertical];

    /// The number of declarations each built-in style is expected to carry.
    const DECL_COUNT: usize = 7;

    /// The declared properties of a style vec, in declaration order.
    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length. A
    /// rule declared in `em`/`%` would resolve against the parent font/box and
    /// either vanish or blow up instead of staying a hairline.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "divider lengths must be absolute px, got {:?}",
            pv.metric
        );
        pv.number.get()
    }

    fn height_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::Height(h) => match h.get_property() {
                Some(LayoutHeight::Px(pv)) => Some(px(pv)),
                Some(other) => panic!("divider height must be a px length, got {other:?}"),
                None => None,
            },
            _ => None,
        })
    }

    fn width_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::Width(w) => match w.get_property() {
                Some(LayoutWidth::Px(pv)) => Some(px(pv)),
                Some(other) => panic!("divider width must be a px length, got {other:?}"),
                None => None,
            },
            _ => None,
        })
    }

    /// The four margins in `(top, bottom, left, right)` order.
    fn margins_px(
        v: &CssPropertyWithConditionsVec,
    ) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
        let find = |f: &dyn Fn(&CssProperty) -> Option<f32>| {
            v.as_ref().iter().find_map(|p| f(&p.property))
        };
        (
            find(&|p| match p {
                CssProperty::MarginTop(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::MarginBottom(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::MarginLeft(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::MarginRight(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
        )
    }

    fn flex_grow(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::FlexGrow(f) => f.get_property().map(|f| f.inner.get()),
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
            "a divider must declare exactly one background layer"
        );
        match &bg.as_ref()[0] {
            StyleBackgroundContent::Color(c) => Some(*c),
            other => panic!("divider background is not a flat colour: {other:?}"),
        }
    }

    /// Every `PixelValue` a style vec mentions (thickness + margins).
    fn all_pixel_values(v: &CssPropertyWithConditionsVec) -> Vec<PixelValue> {
        v.as_ref()
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::Height(h) => match h.get_property() {
                    Some(LayoutHeight::Px(pv)) => Some(*pv),
                    _ => None,
                },
                CssProperty::Width(w) => match w.get_property() {
                    Some(LayoutWidth::Px(pv)) => Some(*pv),
                    _ => None,
                },
                CssProperty::MarginTop(x) => x.get_property().map(|x| x.inner),
                CssProperty::MarginBottom(x) => x.get_property().map(|x| x.inner),
                CssProperty::MarginLeft(x) => x.get_property().map(|x| x.inner),
                CssProperty::MarginRight(x) => x.get_property().map(|x| x.inner),
                _ => None,
            })
            .collect()
    }

    /// True if `node` carries the CSS class `name`.
    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
    }

    /// The properties of a rendered node's *inline* style, in declaration order.
    fn inline_properties(node: &Dom) -> Vec<CssProperty> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// `(property, number-of-conditions)` for a rendered node, in declaration order.
    fn inline_properties_with_condition_counts(node: &Dom) -> Vec<(CssProperty, usize)> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, c)| (p.clone(), c.as_ref().len()))
            .collect()
    }

    // ------------------------------------------------------------------
    // DividerOrientation
    // ------------------------------------------------------------------

    #[test]
    fn orientation_default_is_horizontal_and_the_two_variants_are_distinct() {
        assert_eq!(
            DividerOrientation::default(),
            DividerOrientation::Horizontal
        );
        assert_ne!(DividerOrientation::Horizontal, DividerOrientation::Vertical);
        // `Copy` must not alias into something else: a copy compares equal to the
        // original and to itself.
        for o in ALL_ORIENTATIONS {
            let copy = o;
            assert_eq!(o, copy, "{o:?}: a copy diverged from the original");
        }
        assert_eq!(
            ALL_ORIENTATIONS.len(),
            2,
            "a new orientation was added without updating these tests"
        );
    }

    // ------------------------------------------------------------------
    // Divider::create / Divider::create_with_orientation  (constructors)
    // ------------------------------------------------------------------

    #[test]
    fn create_is_the_horizontal_constructor_and_the_default() {
        let created = Divider::create();
        assert_eq!(created.orientation, DividerOrientation::Horizontal);
        assert_eq!(
            created,
            Divider::create_with_orientation(DividerOrientation::Horizontal)
        );
        assert_eq!(created, Divider::default());
        // `Clone` must preserve equality — the style vec is backed by a `'static`
        // slice, so a shallow/deep clone mix-up would show up here first.
        assert_eq!(created.clone(), created);
    }

    #[test]
    fn create_with_orientation_stores_exactly_the_orientation_it_was_given() {
        for o in ALL_ORIENTATIONS {
            let d = Divider::create_with_orientation(o);
            assert_eq!(
                d.orientation, o,
                "{o:?}: the orientation field does not match the argument"
            );
            // Two builds of the same orientation must be indistinguishable.
            assert_eq!(
                d,
                Divider::create_with_orientation(o),
                "{o:?}: construction is not deterministic"
            );
        }
    }

    #[test]
    fn constructed_style_vecs_have_consistent_length_and_capacity() {
        for o in ALL_ORIENTATIONS {
            let d = Divider::create_with_orientation(o);
            let v = &d.divider_style;
            assert_eq!(
                v.len(),
                v.as_ref().len(),
                "{o:?}: len() disagrees with the slice view"
            );
            assert!(
                v.capacity() >= v.len(),
                "{o:?}: capacity {} < len {}",
                v.capacity(),
                v.len()
            );
            assert!(
                !v.is_empty(),
                "{o:?}: a divider with no declarations paints nothing"
            );
            assert_eq!(
                v.len(),
                DECL_COUNT,
                "{o:?}: unexpected number of declarations"
            );
        }
    }

    // ------------------------------------------------------------------
    // The two built-in styles
    // ------------------------------------------------------------------

    #[test]
    fn horizontal_style_is_a_one_pixel_tall_rule_with_vertical_breathing_room() {
        let style = Divider::create().divider_style;

        assert_eq!(
            height_px(&style),
            Some(1.0),
            "a horizontal rule must be exactly 1px tall"
        );
        // A declared width would pin the rule to a fixed size and defeat the
        // `align-self: stretch` that is supposed to span the parent.
        assert_eq!(
            width_px(&style),
            None,
            "a horizontal rule must not declare a width"
        );
        assert_eq!(
            margins_px(&style),
            (Some(4.0), Some(4.0), None, None),
            "a horizontal rule takes 4px above/below and nothing on the sides"
        );
    }

    #[test]
    fn vertical_style_is_a_one_pixel_wide_rule_with_horizontal_breathing_room() {
        let style = Divider::create_with_orientation(DividerOrientation::Vertical).divider_style;

        assert_eq!(
            width_px(&style),
            Some(1.0),
            "a vertical rule must be exactly 1px wide"
        );
        // The copy-paste hazard this widget is most exposed to: a `height: 1px`
        // left over from the horizontal style would collapse the vertical rule
        // into a 1x1 dot.
        assert_eq!(
            height_px(&style),
            None,
            "a vertical rule must not declare a height"
        );
        assert_eq!(
            margins_px(&style),
            (None, None, Some(4.0), Some(4.0)),
            "a vertical rule takes 4px left/right and nothing above/below"
        );
    }

    #[test]
    fn both_orientations_share_the_colour_and_the_box_model_flags() {
        for o in ALL_ORIENTATIONS {
            let style = Divider::create_with_orientation(o).divider_style;
            let props = properties(&style);
            let has = |p: &CssProperty| props.contains(p);

            assert!(
                has(&CssProperty::const_display(LayoutDisplay::Block)),
                "{o:?}: not a block box"
            );
            // Both halves of the "span the parent" contract: stretch on the cross
            // axis, no growth on the main axis.
            assert!(
                has(&CssProperty::align_self(LayoutAlignSelf::Stretch)),
                "{o:?}: rule does not stretch"
            );
            assert!(
                has(&CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
                "{o:?}: rule grows on the main axis and would eat sibling space"
            );
            assert_eq!(
                background_color(&style),
                Some(DIVIDER_COLOR),
                "{o:?}: wrong rule colour"
            );
        }
    }

    #[test]
    fn the_rule_colour_is_the_documented_opaque_grey() {
        // A translucent or non-grey rule is a visible regression, and the doc
        // comment pins it to #dddddd.
        assert_eq!(
            DIVIDER_COLOR,
            ColorU {
                r: 221,
                g: 221,
                b: 221,
                a: 255
            }
        );
        assert_eq!(
            DIVIDER_COLOR.a, 255,
            "a translucent rule lets the background bleed through"
        );
        assert_eq!(
            DIVIDER_COLOR.r, DIVIDER_COLOR.g,
            "the rule colour is not neutral grey"
        );
        assert_eq!(
            DIVIDER_COLOR.g, DIVIDER_COLOR.b,
            "the rule colour is not neutral grey"
        );
        assert_eq!(
            DIVIDER_BG_ITEMS.len(),
            1,
            "the rule must be a single flat layer"
        );
    }

    #[test]
    fn orientation_actually_changes_the_emitted_style() {
        // If the two static tables were ever wired to the same slice the
        // `orientation` argument would silently become a no-op.
        let h = Divider::create_with_orientation(DividerOrientation::Horizontal).divider_style;
        let v = Divider::create_with_orientation(DividerOrientation::Vertical).divider_style;
        assert_ne!(
            properties(&h),
            properties(&v),
            "both orientations produce an identical style"
        );
        assert_eq!(
            h.len(),
            v.len(),
            "the two orientations declare a different number of properties"
        );

        // The axis-defining declarations must be mirror images of each other.
        assert!(height_px(&h).is_some() && width_px(&h).is_none());
        assert!(width_px(&v).is_some() && height_px(&v).is_none());
        assert_eq!(
            height_px(&h),
            width_px(&v),
            "the two rules are not the same thickness"
        );
    }

    #[test]
    fn every_declaration_is_unconditional() {
        // A divider is stateless — a declaration gated on `:hover`/`:active`
        // would simply never paint.
        for o in ALL_ORIENTATIONS {
            for p in Divider::create_with_orientation(o).divider_style.as_ref() {
                assert!(
                    p.apply_if.as_ref().is_empty(),
                    "{o:?}: {:?} is conditional on a stateless widget",
                    p.property
                );
            }
        }
    }

    #[test]
    fn no_property_is_declared_twice() {
        // A duplicated declaration is a last-one-wins ambiguity: two heights or
        // two backgrounds would make one of them silently dead.
        for o in ALL_ORIENTATIONS {
            let props = properties(&Divider::create_with_orientation(o).divider_style);
            let mut seen = HashSet::new();
            for p in &props {
                assert!(
                    seen.insert(core::mem::discriminant(p)),
                    "{o:?}: duplicate declaration of {p:?}"
                );
            }
            assert_eq!(seen.len(), props.len());
        }
    }

    #[test]
    fn every_length_is_a_finite_non_negative_absolute_px() {
        // Guard the `isize` -> `PixelValue` conversions: a NaN/inf/negative
        // length must never reach the layout solver.
        for o in ALL_ORIENTATIONS {
            let values = all_pixel_values(&Divider::create_with_orientation(o).divider_style);
            assert_eq!(
                values.len(),
                3,
                "{o:?}: expected one thickness + two margins"
            );
            for pv in values {
                let n = px(&pv); // also asserts SizeMetric::Px
                assert!(n.is_finite(), "{o:?}: non-finite length {n}");
                assert!(!n.is_nan(), "{o:?}: NaN length");
                assert!(n >= 0.0, "{o:?}: negative length {n}");
                assert!(
                    n <= 64.0,
                    "{o:?}: implausibly large length {n} for a hairline rule"
                );
            }
        }
    }

    #[test]
    fn the_rule_is_thick_enough_to_be_visible_and_thin_enough_to_be_a_rule() {
        for o in ALL_ORIENTATIONS {
            let style = Divider::create_with_orientation(o).divider_style;
            let thickness = height_px(&style)
                .or_else(|| width_px(&style))
                .expect("a divider must declare a thickness on one axis");
            assert!(thickness > 0.0, "{o:?}: a 0px rule is invisible");
            assert!(
                thickness <= 4.0,
                "{o:?}: {thickness}px is a bar, not a rule"
            );
        }
    }

    #[test]
    fn flex_grow_is_exactly_zero_and_not_a_rounding_artefact() {
        // `FloatValue` stores a fixed-point `isize`; a botched encode/decode
        // would show up as 0.001 or -0.0 rather than a clean 0.
        for o in ALL_ORIENTATIONS {
            let g = flex_grow(&Divider::create_with_orientation(o).divider_style)
                .expect("flex-grow must be declared");
            assert!(g.is_finite(), "{o:?}: non-finite flex-grow {g}");
            assert_eq!(g, 0.0, "{o:?}: flex-grow is {g}, not 0");
            assert!(g.is_sign_positive(), "{o:?}: flex-grow decoded as -0.0");
        }
    }

    #[test]
    fn the_fixed_point_length_encoding_round_trips() {
        // encode == decode for every constant this file bakes in.
        assert_eq!(PixelValue::const_px(1).number.get(), 1.0);
        assert_eq!(PixelValue::const_px(4).number.get(), 4.0);
        assert_eq!(LayoutFlexGrow::const_new(0).inner.get(), 0.0);

        // ...and the values that actually landed in the built styles are the
        // ones the constructors asked for.
        let h = properties(&Divider::create().divider_style);
        assert!(h.contains(&CssProperty::const_height(LayoutHeight::const_px(1))));
        assert!(h.contains(&CssProperty::const_margin_top(LayoutMarginTop::const_px(4))));
        assert!(h.contains(&CssProperty::const_margin_bottom(
            LayoutMarginBottom::const_px(4)
        )));

        let v = properties(
            &Divider::create_with_orientation(DividerOrientation::Vertical).divider_style,
        );
        assert!(v.contains(&CssProperty::const_width(LayoutWidth::const_px(1))));
        assert!(
            v.contains(&CssProperty::const_margin_left(LayoutMarginLeft::const_px(
                4
            )))
        );
        assert!(v.contains(&CssProperty::const_margin_right(
            LayoutMarginRight::const_px(4)
        )));
    }

    #[test]
    fn cloning_and_dropping_never_corrupts_the_shared_static_style() {
        // The style vec borrows a `'static` slice (`NoDestructor`). A clone that
        // wrongly claims ownership, or a `Drop` that frees the static, would
        // corrupt every future divider — so churn hard and re-check the source.
        for o in ALL_ORIENTATIONS {
            let base = Divider::create_with_orientation(o);
            let expected = properties(&base.divider_style);
            for round in 0..1000 {
                let c = base.clone();
                assert_eq!(
                    properties(&c.divider_style),
                    expected,
                    "{o:?}: clone {round} diverged"
                );
                drop(c);
            }
            assert_eq!(
                properties(&base.divider_style),
                expected,
                "{o:?}: the original was damaged"
            );
            assert_eq!(
                properties(&Divider::create_with_orientation(o).divider_style),
                expected,
                "{o:?}: a freshly built divider disagrees after 1000 clone/drop cycles"
            );
        }
    }

    // ------------------------------------------------------------------
    // Divider::set_orientation
    // ------------------------------------------------------------------

    #[test]
    fn set_orientation_replaces_the_style_and_never_grows_it() {
        // A push-instead-of-replace bug would grow the vec on every flip and
        // leave the previous axis's `height`/`width` behind, turning the rule
        // into a 1x1 dot.
        let mut d = Divider::create();
        for round in 0..200 {
            let o = ALL_ORIENTATIONS[round % ALL_ORIENTATIONS.len()];
            d.set_orientation(o);

            assert_eq!(
                d.orientation, o,
                "round {round}: orientation field not updated"
            );
            assert_eq!(
                d.divider_style.len(),
                DECL_COUNT,
                "round {round}: style vec changed length"
            );
            assert_eq!(
                d,
                Divider::create_with_orientation(o),
                "round {round}: not equal to a fresh build"
            );
            match o {
                DividerOrientation::Horizontal => {
                    assert_eq!(
                        width_px(&d.divider_style),
                        None,
                        "round {round}: stale vertical width"
                    );
                    assert_eq!(height_px(&d.divider_style), Some(1.0), "round {round}");
                }
                DividerOrientation::Vertical => {
                    assert_eq!(
                        height_px(&d.divider_style),
                        None,
                        "round {round}: stale horizontal height"
                    );
                    assert_eq!(width_px(&d.divider_style), Some(1.0), "round {round}");
                }
            }
        }
    }

    #[test]
    fn set_orientation_is_idempotent() {
        for o in ALL_ORIENTATIONS {
            let mut d = Divider::create_with_orientation(o);
            let before = d.clone();
            d.set_orientation(o);
            assert_eq!(
                d, before,
                "{o:?}: re-setting the same orientation changed the divider"
            );
            d.set_orientation(o);
            assert_eq!(d, before, "{o:?}: the second re-set changed the divider");
        }
    }

    #[test]
    fn set_orientation_discards_a_custom_style_as_documented() {
        // Documented behaviour: "resetting the style to the matching default".
        // A caller who styled the rule loses that styling even when the
        // orientation does not change — assert it so the contract is explicit.
        let custom =
            CssPropertyWithConditionsVec::from_vec(vec![CssPropertyWithConditions::simple(
                CssProperty::const_height(LayoutHeight::const_px(42)),
            )]);
        let mut d = Divider {
            orientation: DividerOrientation::Horizontal,
            divider_style: custom,
        };
        d.set_orientation(DividerOrientation::Horizontal);
        assert_eq!(
            d,
            Divider::create(),
            "the custom style survived a same-orientation reset"
        );
        assert_eq!(
            height_px(&d.divider_style),
            Some(1.0),
            "the 42px override was not discarded"
        );
    }

    #[test]
    fn set_orientation_heals_a_hand_built_inconsistent_divider() {
        // Both fields are `pub`, so a caller can construct a divider whose
        // `orientation` contradicts its `divider_style`. The setter must
        // rebuild both, not just stamp the enum.
        let mut desynced = Divider {
            orientation: DividerOrientation::Vertical,
            divider_style: CssPropertyWithConditionsVec::from_const_slice(DIVIDER_STYLE_HORIZONTAL),
        };
        assert_ne!(
            desynced,
            Divider::create_with_orientation(DividerOrientation::Vertical),
            "the desynced divider was expected to differ from a canonical one"
        );
        desynced.set_orientation(DividerOrientation::Vertical);
        assert_eq!(
            desynced,
            Divider::create_with_orientation(DividerOrientation::Vertical)
        );
        assert_eq!(
            height_px(&desynced.divider_style),
            None,
            "the horizontal height survived"
        );
    }

    // ------------------------------------------------------------------
    // Divider::with_orientation  (constructor / builder)
    // ------------------------------------------------------------------

    #[test]
    fn with_orientation_invariants_hold_for_every_orientation() {
        for o in ALL_ORIENTATIONS {
            let d = Divider::create().with_orientation(o);
            assert_eq!(d.orientation, o, "{o:?}: field does not match the argument");
            assert_eq!(
                d,
                Divider::create_with_orientation(o),
                "{o:?}: builder != constructor"
            );
            assert_eq!(d.divider_style.len(), d.divider_style.as_ref().len());
            assert!(d.divider_style.capacity() >= d.divider_style.len());
            assert_eq!(d.divider_style.len(), DECL_COUNT);
        }
    }

    #[test]
    fn with_orientation_agrees_with_set_orientation_and_is_last_call_wins() {
        let chained = Divider::create()
            .with_orientation(DividerOrientation::Vertical)
            .with_orientation(DividerOrientation::Horizontal)
            .with_orientation(DividerOrientation::Vertical);

        let mut mutated = Divider::create();
        mutated.set_orientation(DividerOrientation::Vertical);
        mutated.set_orientation(DividerOrientation::Horizontal);
        mutated.set_orientation(DividerOrientation::Vertical);

        assert_eq!(chained, mutated, "the builder and the mutator must agree");
        assert_eq!(chained.orientation, DividerOrientation::Vertical);
        // In particular the intermediate horizontal `height` must be gone.
        assert_eq!(
            height_px(&chained.divider_style),
            None,
            "a stale horizontal height survived the chain"
        );
        assert_eq!(width_px(&chained.divider_style), Some(1.0));
    }

    #[test]
    fn a_long_builder_chain_does_not_accumulate_declarations() {
        let mut d = Divider::create();
        for round in 0..500 {
            d = d.with_orientation(ALL_ORIENTATIONS[round % ALL_ORIENTATIONS.len()]);
            assert_eq!(
                d.divider_style.len(),
                DECL_COUNT,
                "round {round}: the style vec grew"
            );
        }
        assert_eq!(
            d,
            Divider::create_with_orientation(DividerOrientation::Vertical)
        );
    }

    // ------------------------------------------------------------------
    // Divider::swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_returns_the_original_and_leaves_a_horizontal_default() {
        let mut d = Divider::create_with_orientation(DividerOrientation::Vertical);
        let taken = d.swap_with_default();

        // The returned value is the *original*, intact.
        assert_eq!(taken.orientation, DividerOrientation::Vertical);
        assert_eq!(width_px(&taken.divider_style), Some(1.0));
        assert_eq!(
            taken,
            Divider::create_with_orientation(DividerOrientation::Vertical)
        );

        // What is left behind is a *default* — in particular the vertical
        // `width` must not have survived the swap.
        assert_eq!(d, Divider::default());
        assert_eq!(d.orientation, DividerOrientation::Horizontal);
        assert_eq!(
            width_px(&d.divider_style),
            None,
            "the vertical width survived the swap"
        );
        assert_eq!(height_px(&d.divider_style), Some(1.0));
    }

    #[test]
    fn swap_with_default_is_idempotent_on_an_already_default_divider() {
        let mut d = Divider::default();
        let first = d.swap_with_default();
        let second = d.swap_with_default();
        assert_eq!(first, Divider::default());
        assert_eq!(second, Divider::default());
        assert_eq!(d, Divider::default());
    }

    #[test]
    fn repeated_swaps_never_corrupt_the_static_backed_style() {
        // `mem::swap` moves a vec that borrows a `'static` slice; 100 rounds of
        // swap-and-drop would surface a double free or a dangling `ptr`.
        let mut d = Divider::create_with_orientation(DividerOrientation::Vertical);
        for round in 0..100 {
            let taken = d.swap_with_default();
            if round == 0 {
                assert_eq!(
                    taken.orientation,
                    DividerOrientation::Vertical,
                    "round 0: wrong value returned"
                );
            } else {
                assert_eq!(
                    taken,
                    Divider::default(),
                    "round {round}: the emptied slot was not a default"
                );
            }
            assert_eq!(
                d,
                Divider::default(),
                "round {round}: what was left behind is not a default"
            );
            assert_eq!(
                d.divider_style.len(),
                DECL_COUNT,
                "round {round}: the style vec changed length"
            );
        }
    }

    #[test]
    fn swap_with_default_returns_a_custom_style_untouched() {
        let custom =
            CssPropertyWithConditionsVec::from_vec(vec![CssPropertyWithConditions::simple(
                CssProperty::const_width(LayoutWidth::const_px(9)),
            )]);
        let mut d = Divider {
            orientation: DividerOrientation::Vertical,
            divider_style: custom,
        };
        let taken = d.swap_with_default();
        assert_eq!(taken.orientation, DividerOrientation::Vertical);
        assert_eq!(
            taken.divider_style.len(),
            1,
            "the custom style was rewritten on the way out"
        );
        assert_eq!(width_px(&taken.divider_style), Some(9.0));
        assert_eq!(d, Divider::default());
    }

    // ------------------------------------------------------------------
    // Divider::dom  (round-trip: divider -> DOM)
    // ------------------------------------------------------------------

    #[test]
    fn dom_is_a_single_classed_div_with_no_children_or_callbacks() {
        for o in ALL_ORIENTATIONS {
            let divider = Divider::create_with_orientation(o);
            let expected = properties(&divider.divider_style);
            let dom = divider.dom();

            assert!(
                has_class(&dom, "__azul-native-divider"),
                "{o:?}: missing the widget class"
            );
            assert_eq!(
                dom.root.get_node_type(),
                &NodeType::Div,
                "{o:?}: a rule must be a plain div"
            );
            assert!(
                dom.children.as_ref().is_empty(),
                "{o:?}: a divider is a leaf, not a subtree"
            );
            assert!(
                dom.root.callbacks.as_ref().is_empty(),
                "{o:?}: a stateless widget must not bind callbacks"
            );
            assert_eq!(
                inline_properties(&dom),
                expected,
                "{o:?}: the rule lost its computed style"
            );
            assert_eq!(
                dom.root.get_ids_and_classes().as_ref().len(),
                1,
                "{o:?}: expected exactly one class and no ids"
            );
        }
    }

    #[test]
    fn dom_renders_the_orientation_the_divider_was_last_set_to() {
        // `dom()` consumes the *cached* style, so a `set_orientation` that forgot
        // to recompute would paint the previous axis here and nowhere else.
        for o in ALL_ORIENTATIONS {
            let mut divider = Divider::create();
            divider.set_orientation(DividerOrientation::Vertical);
            divider.set_orientation(o);
            let expected = properties(&Divider::create_with_orientation(o).divider_style);
            assert_eq!(
                inline_properties(&divider.dom()),
                expected,
                "{o:?}: the DOM shows a stale axis"
            );
        }
    }

    #[test]
    fn dom_of_the_two_orientations_differ_but_carry_the_same_class() {
        let h = Divider::create().dom();
        let v = Divider::create_with_orientation(DividerOrientation::Vertical).dom();

        assert_ne!(
            inline_properties(&h),
            inline_properties(&v),
            "both orientations render identically"
        );
        // Both share one class, so a stylesheet cannot tell them apart by class
        // alone — pinned here so a future split is a deliberate change.
        assert!(has_class(&h, "__azul-native-divider"));
        assert!(has_class(&v, "__azul-native-divider"));
        assert_eq!(
            h.root.get_ids_and_classes().as_ref(),
            v.root.get_ids_and_classes().as_ref(),
            "the two orientations were expected to share their class list"
        );
    }

    #[test]
    fn the_widget_class_is_a_namespaced_ascii_css_identifier() {
        let dom = Divider::create().dom();
        let classes = dom.root.get_ids_and_classes();
        let name = classes
            .as_ref()
            .iter()
            .find_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .expect("the divider must carry a class");

        assert_eq!(name, "__azul-native-divider");
        assert!(!name.is_empty(), "empty class name");
        assert!(name.is_ascii(), "non-ASCII class name {name:?}");
        assert!(
            name.starts_with("__azul-native-"),
            "unnamespaced class {name:?}"
        );
        // A space, a dot or a `#` would silently split/re-target the selector.
        assert!(
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "class name {name:?} contains a CSS-significant character"
        );
    }

    #[test]
    fn from_divider_for_dom_is_exactly_dom() {
        for o in ALL_ORIENTATIONS {
            let divider = Divider::create_with_orientation(o);
            let via_into: Dom = divider.clone().into();
            let via_dom = divider.dom();
            assert_eq!(
                inline_properties(&via_into),
                inline_properties(&via_dom),
                "{o:?}: `From` diverges from `dom()`"
            );
            assert_eq!(
                via_into.root.get_node_type(),
                via_dom.root.get_node_type(),
                "{o:?}: `From` built a different node"
            );
            assert_eq!(
                via_into.root.get_ids_and_classes().as_ref(),
                via_dom.root.get_ids_and_classes().as_ref(),
                "{o:?}: `From` produced a different class list"
            );
        }
    }

    #[test]
    fn dom_renders_the_style_field_and_ignores_the_orientation_field() {
        // `dom()` only consumes `divider_style`. A hand-built divider whose
        // `orientation` contradicts its style therefore renders the *style* —
        // pinned so the divergence is documented rather than surprising.
        let desynced = Divider {
            orientation: DividerOrientation::Vertical,
            divider_style: CssPropertyWithConditionsVec::from_const_slice(DIVIDER_STYLE_HORIZONTAL),
        };
        let rendered = inline_properties(&desynced.dom());
        assert_eq!(
            rendered,
            properties(&Divider::create().divider_style),
            "dom() did not render the style it was handed"
        );
    }

    #[test]
    fn dom_of_an_empty_style_is_still_a_classed_div() {
        // Boundary case: zero declarations. Must not panic and must keep the
        // class, otherwise the node becomes untargetable *and* invisible.
        let d = Divider {
            orientation: DividerOrientation::Horizontal,
            divider_style: CssPropertyWithConditionsVec::new(),
        };
        let dom = d.dom();
        assert!(has_class(&dom, "__azul-native-divider"));
        assert_eq!(dom.root.get_node_type(), &NodeType::Div);
        assert!(
            inline_properties(&dom).is_empty(),
            "properties appeared out of an empty style vec"
        );
        assert!(dom.children.as_ref().is_empty());
    }

    #[test]
    fn dom_preserves_a_huge_custom_style_verbatim_and_in_order() {
        // 10k declarations through the `CssPropertyWithConditionsVec` -> `Css`
        // bridge: a bug that dropped, truncated or reordered entries (the very
        // failure that bridge's comment describes) would show up here.
        let big: Vec<CssPropertyWithConditions> = (0..10_000_isize)
            .map(|i| {
                CssPropertyWithConditions::simple(CssProperty::const_margin_top(
                    LayoutMarginTop::const_px(i),
                ))
            })
            .collect();
        let expected: Vec<CssProperty> = big.iter().map(|p| p.property.clone()).collect();

        let d = Divider {
            orientation: DividerOrientation::Horizontal,
            divider_style: CssPropertyWithConditionsVec::from_vec(big),
        };
        let rendered = inline_properties(&d.dom());
        assert_eq!(
            rendered.len(),
            10_000,
            "declarations were dropped on the way into the DOM"
        );
        assert_eq!(rendered, expected, "declaration order was not preserved");
    }

    #[test]
    fn dom_preserves_the_conditions_attached_to_each_declaration() {
        // The bridge promises "the original conditions intact"; a lost condition
        // would turn a `:hover` override into an unconditional one.
        let props = vec![
            CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
            CssPropertyWithConditions::on_hover(CssProperty::const_height(LayoutHeight::const_px(
                3,
            ))),
        ];
        let d = Divider {
            orientation: DividerOrientation::Horizontal,
            divider_style: CssPropertyWithConditionsVec::from_vec(props),
        };
        let pairs = inline_properties_with_condition_counts(&d.dom());
        assert_eq!(pairs.len(), 2, "a declaration was dropped");
        assert_eq!(pairs[0].0, CssProperty::const_display(LayoutDisplay::Block));
        assert_eq!(
            pairs[0].1, 0,
            "an unconditional declaration gained a condition"
        );
        assert_eq!(
            pairs[1].0,
            CssProperty::const_height(LayoutHeight::const_px(3))
        );
        assert_eq!(pairs[1].1, 1, "the :hover condition was dropped");
    }

    #[test]
    fn building_many_doms_is_stable() {
        // Every divider borrows the same `'static` style table; building and
        // dropping many DOMs from it must not perturb later ones.
        let expected = properties(&Divider::create().divider_style);
        for round in 0..500 {
            let dom = Divider::create().dom();
            assert_eq!(
                inline_properties(&dom),
                expected,
                "round {round}: the shared style drifted"
            );
            drop(dom);
        }
        assert_eq!(properties(&Divider::create().divider_style), expected);
    }

    // ------------------------------------------------------------------
    // Equality
    // ------------------------------------------------------------------

    #[test]
    fn equality_sees_both_fields() {
        assert_ne!(
            Divider::create_with_orientation(DividerOrientation::Horizontal),
            Divider::create_with_orientation(DividerOrientation::Vertical),
            "dividers of different orientation must not compare equal"
        );
        // Same orientation, different style => not equal.
        let styled = Divider {
            orientation: DividerOrientation::Horizontal,
            divider_style: CssPropertyWithConditionsVec::new(),
        };
        assert_ne!(
            styled,
            Divider::create(),
            "the style field must affect equality"
        );
        // Same style, different orientation => not equal.
        let desynced = Divider {
            orientation: DividerOrientation::Vertical,
            divider_style: CssPropertyWithConditionsVec::from_const_slice(DIVIDER_STYLE_HORIZONTAL),
        };
        assert_ne!(
            desynced,
            Divider::create(),
            "the orientation field must affect equality"
        );
    }
}
