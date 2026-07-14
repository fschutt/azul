//! Badge widget — a small rounded "pill" showing a short count or status string
//! (e.g. a notification count or a status label). A stateless, single styled
//! text node with no callback — a near-clone of [`crate::widgets::label::Label`]
//! restyled as a coloured pill, with an optional [`BadgeKind`] colour variant
//! (mirroring `button::ButtonType`).
//!
//! Key types: [`Badge`], [`BadgeKind`].

use azul_core::dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, StyleFontSize},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutJustifyContent, LayoutAlignItems, LayoutAlignSelf, LayoutFlexGrow, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight},
        property::{CssProperty, *},
        style::{StyleBackgroundContentVec, StyleBackgroundContent, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleTextAlign, StyleTextColor},
    },
    AzString,
};

/// The semantic colour variant of a [`Badge`] (mirrors `button::ButtonType`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub enum BadgeKind {
    /// Neutral grey badge — the default.
    #[default]
    Default,
    /// Blue "primary" badge.
    Primary,
    /// Green "success" badge.
    Success,
    /// Red "danger" badge.
    Danger,
    /// Yellow "warning" badge (uses dark text).
    Warning,
    /// Cyan "info" badge (uses dark text).
    Info,
}

impl BadgeKind {
    /// Returns the `(background, text)` colours for this badge kind.
    #[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
    const fn colors(&self) -> (ColorU, ColorU) {
        const WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
        const DARK: ColorU = ColorU { r: 33, g: 37, b: 41, a: 255 };
        match self {
            Self::Default => (ColorU { r: 108, g: 117, b: 125, a: 255 }, WHITE),
            Self::Primary => (ColorU { r: 13, g: 110, b: 253, a: 255 }, WHITE),
            Self::Success => (ColorU { r: 25, g: 135, b: 84, a: 255 }, WHITE),
            Self::Danger => (ColorU { r: 220, g: 53, b: 69, a: 255 }, WHITE),
            Self::Warning => (ColorU { r: 255, g: 193, b: 7, a: 255 }, DARK),
            Self::Info => (ColorU { r: 13, g: 202, b: 240, a: 255 }, DARK),
        }
    }

    /// CSS class name for this badge kind (mirrors `ButtonType::class_name`).
    #[must_use] pub const fn class_name(&self) -> &'static str {
        match self {
            Self::Default => "__azul-badge-default",
            Self::Primary => "__azul-badge-primary",
            Self::Success => "__azul-badge-success",
            Self::Danger => "__azul-badge-danger",
            Self::Warning => "__azul-badge-warning",
            Self::Info => "__azul-badge-info",
        }
    }
}

/// A small rounded pill showing a short status/count string. Stateless;
/// renders a single styled text node.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Badge {
    /// The text shown inside the pill.
    pub string: AzString,
    /// The colour variant.
    pub kind: BadgeKind,
    /// The computed inline style for the pill.
    pub badge_style: CssPropertyWithConditionsVec,
}

/// Builds the pill style for a given [`BadgeKind`]. The colours are the only
/// kind-dependent properties, so the style is built at runtime per the recipe's
/// "runtime vec when param-dependent" path (see `switch::build_track_style`).
fn build_badge_style(kind: BadgeKind) -> CssPropertyWithConditionsVec {
    let (bg, text) = kind.colors();
    let bg_vec =
        StyleBackgroundContentVec::from_vec(alloc::vec![StyleBackgroundContent::Color(bg)]);
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Row,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_justify_content(
            LayoutJustifyContent::Center,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        // Hug the content rather than stretch across a flex parent's cross axis.
        CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        // padding: 2px 8px
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
            2,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
            LayoutPaddingBottom::const_px(2),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_left(
            LayoutPaddingLeft::const_px(8),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_right(
            LayoutPaddingRight::const_px(8),
        )),
        // border-radius: 10px (pill)
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(10),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(10),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(10),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(10),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(12))),
        CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
            inner: text,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(bg_vec)),
    ])
}

impl Badge {
    /// Creates a new badge with the given text and the default (grey) kind.
    #[inline]
    #[must_use] pub fn create(string: AzString) -> Self {
        Self::with_kind(string, BadgeKind::Default)
    }

    /// Creates a new badge with the given text and colour variant.
    #[inline]
    #[must_use] pub fn with_kind(string: AzString, kind: BadgeKind) -> Self {
        Self {
            string,
            kind,
            badge_style: build_badge_style(kind),
        }
    }

    /// Sets the colour variant, recomputing the style.
    #[inline]
    pub fn set_kind(&mut self, kind: BadgeKind) {
        self.kind = kind;
        self.badge_style = build_badge_style(kind);
    }

    /// Builder-style setter for the colour variant.
    #[inline]
    #[must_use] pub fn with_badge_kind(mut self, kind: BadgeKind) -> Self {
        self.set_kind(kind);
        self
    }

    /// Replaces `self` with an empty default badge and returns the original.
    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this badge into a DOM text node with the `__azul-native-badge` class.
    #[inline]
    #[must_use] pub fn dom(self) -> Dom {
        static BADGE_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-badge"))];

        Dom::create_text(self.string)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(BADGE_CLASS))
            .with_css_props(self.badge_style)
    }
}

impl Default for Badge {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""))
    }
}

impl From<Badge> for Dom {
    fn from(b: Badge) -> Self {
        b.dom()
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

    /// Every variant of `BadgeKind` — the complete input domain of `colors`,
    /// `class_name` and `build_badge_style`.
    const ALL_KINDS: [BadgeKind; 6] = [
        BadgeKind::Default,
        BadgeKind::Primary,
        BadgeKind::Success,
        BadgeKind::Danger,
        BadgeKind::Warning,
        BadgeKind::Info,
    ];

    const WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
    const DARK: ColorU = ColorU { r: 33, g: 37, b: 41, a: 255 };

    /// The declared properties of a style vec, in declaration order.
    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length — an
    /// `em`/`%` slipping into the pill geometry would resolve against the parent
    /// font/box instead of the intended fixed padding or radius.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(pv.metric, SizeMetric::Px, "badge geometry must be absolute px, got {:?}", pv.metric);
        pv.number.get()
    }

    /// The four paddings in `(top, bottom, left, right)` order.
    fn padding_px(v: &CssPropertyWithConditionsVec) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
        let find = |f: &dyn Fn(&CssProperty) -> Option<f32>| v.as_ref().iter().find_map(|p| f(&p.property));
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
        assert_eq!(bg.as_ref().len(), 1, "a badge must declare exactly one background layer");
        match &bg.as_ref()[0] {
            StyleBackgroundContent::Color(c) => Some(*c),
            other => panic!("badge background is not a flat colour: {other:?}"),
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

    /// True if `node` carries the CSS class `name`.
    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == name))
    }

    /// The properties of a rendered node's *inline* style, in declaration order.
    fn inline_properties(node: &Dom) -> Vec<CssProperty> {
        node.root.style.iter_inline_properties().map(|(p, _)| p.clone()).collect()
    }

    /// Adversarial badge texts: empty, whitespace, combining marks, ZWJ emoji,
    /// RTL, embedded NULs (`AzString` is length-based, so a NUL must not
    /// truncate) and a string far longer than any plausible badge label.
    fn adversarial_strings() -> Vec<String> {
        let mut v: Vec<String> = [
            "",
            "9",
            "99+",
            " ",
            "e\u{0301}",                                   // e + combining acute
            "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}", // ZWJ family emoji
            "\u{5E9}\u{5DC}\u{5D5}\u{5DD}",                // RTL Hebrew
            "\0",                                          // a single NUL
            "a\0b",                                        // embedded NUL
            "\u{FFFD}\u{202E}\u{200B}",                    // replacement char, RTL override, ZWSP
            "-9223372036854775808",                        // i64::MIN as a "count"
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        v.push("x".repeat(100_000));
        v
    }

    // ------------------------------------------------------------------
    // BadgeKind::colors  (getter)
    // ------------------------------------------------------------------

    #[test]
    fn colors_returns_the_documented_constants_for_every_kind() {
        let expected = [
            (BadgeKind::Default, ColorU { r: 108, g: 117, b: 125, a: 255 }, WHITE),
            (BadgeKind::Primary, ColorU { r: 13, g: 110, b: 253, a: 255 }, WHITE),
            (BadgeKind::Success, ColorU { r: 25, g: 135, b: 84, a: 255 }, WHITE),
            (BadgeKind::Danger, ColorU { r: 220, g: 53, b: 69, a: 255 }, WHITE),
            (BadgeKind::Warning, ColorU { r: 255, g: 193, b: 7, a: 255 }, DARK),
            (BadgeKind::Info, ColorU { r: 13, g: 202, b: 240, a: 255 }, DARK),
        ];
        for (kind, bg, text) in expected {
            assert_eq!(kind.colors(), (bg, text), "{kind:?}: wrong (background, text) pair");
        }
        // The doc comments promise Warning/Info are the dark-text kinds and no
        // others: a fifth white-text kind sneaking in here is a regression.
        for kind in ALL_KINDS {
            let (_, text) = kind.colors();
            let dark_text = matches!(kind, BadgeKind::Warning | BadgeKind::Info);
            assert_eq!(text == DARK, dark_text, "{kind:?}: text colour contradicts the documented variant");
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
        }
    }

    #[test]
    fn colors_give_every_kind_a_distinguishable_background() {
        // Two kinds that render identically make the semantic variant useless.
        let mut seen = HashSet::new();
        for kind in ALL_KINDS {
            let (bg, _) = kind.colors();
            assert!(seen.insert((bg.r, bg.g, bg.b, bg.a)), "{kind:?}: duplicate background colour {bg:?}");
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
                "{kind:?}: text {text:?} (Δluma {chosen:.1}) is less readable on {bg:?} than {other:?} (Δluma {rejected:.1})"
            );
            assert!(chosen >= 60.0, "{kind:?}: text/background brightness gap {chosen:.1} is too low to read");

            // Mid-grey split: a light pill must not carry white text.
            let light_bg = luma(bg) >= 128.0;
            assert_eq!(text == DARK, light_bg, "{kind:?}: bg luma {:.1} but text is {text:?}", luma(bg));
        }
    }

    #[test]
    fn colors_is_pure_and_the_default_kind_is_grey() {
        assert_eq!(BadgeKind::default(), BadgeKind::Default);
        assert_eq!(BadgeKind::default().colors(), BadgeKind::Default.colors());
        // `colors()` takes `&self` on a `Copy` enum: repeated calls, and calls
        // through a copy, must be side-effect free and identical.
        for kind in ALL_KINDS {
            let copy = kind;
            assert_eq!(kind.colors(), kind.colors(), "{kind:?}: colors() is not pure");
            assert_eq!(kind.colors(), copy.colors(), "{kind:?}: a copy disagrees with the original");
        }
    }

    // ------------------------------------------------------------------
    // BadgeKind::class_name  (getter)
    // ------------------------------------------------------------------

    #[test]
    fn class_name_returns_the_documented_string_for_every_kind() {
        assert_eq!(BadgeKind::Default.class_name(), "__azul-badge-default");
        assert_eq!(BadgeKind::Primary.class_name(), "__azul-badge-primary");
        assert_eq!(BadgeKind::Success.class_name(), "__azul-badge-success");
        assert_eq!(BadgeKind::Danger.class_name(), "__azul-badge-danger");
        assert_eq!(BadgeKind::Warning.class_name(), "__azul-badge-warning");
        assert_eq!(BadgeKind::Info.class_name(), "__azul-badge-info");
        assert_eq!(BadgeKind::default().class_name(), "__azul-badge-default");
    }

    #[test]
    fn class_name_is_unique_per_kind_and_a_usable_css_identifier() {
        let mut seen = HashSet::new();
        for kind in ALL_KINDS {
            let name = kind.class_name();
            assert!(seen.insert(name), "{kind:?}: class name {name:?} collides with another kind");
            assert!(!name.is_empty(), "{kind:?}: empty class name");
            assert!(name.starts_with("__azul-badge-"), "{kind:?}: unnamespaced class {name:?}");
            assert!(name.is_ascii(), "{kind:?}: non-ASCII class name {name:?}");
            // A space, a dot or a `#` would silently split/re-target the selector.
            assert!(
                name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "{kind:?}: class name {name:?} contains a CSS-significant character"
            );
            // The returned `&'static str` must be stable across calls.
            assert_eq!(name.as_ptr(), kind.class_name().as_ptr(), "{kind:?}: class_name() is not a stable constant");
        }
        assert_eq!(seen.len(), ALL_KINDS.len());
    }

    // ------------------------------------------------------------------
    // build_badge_style
    // ------------------------------------------------------------------

    #[test]
    fn build_badge_style_emits_the_documented_pill_geometry() {
        for kind in ALL_KINDS {
            let style = build_badge_style(kind);
            assert_eq!(padding_px(&style), (Some(2.0), Some(2.0), Some(8.0), Some(8.0)), "{kind:?}: padding is not 2px 8px");
            assert_eq!(radii_px(&style), vec![10.0, 10.0, 10.0, 10.0], "{kind:?}: all four corners must carry a 10px radius");
            assert_eq!(font_size_px(&style), Some(12.0), "{kind:?}: wrong font size");
        }
    }

    #[test]
    fn build_badge_style_radius_actually_rounds_the_pill_to_a_semicircle() {
        // The widget's premise: the corner radius must reach at least half the
        // content height (font + vertical padding), otherwise it renders as a
        // rounded rectangle rather than a pill.
        for kind in ALL_KINDS {
            let style = build_badge_style(kind);
            let (top, bottom, ..) = padding_px(&style);
            let height = font_size_px(&style).expect("a font size must be declared")
                + top.expect("padding-top")
                + bottom.expect("padding-bottom");
            for r in radii_px(&style) {
                assert!(r * 2.0 >= height, "{kind:?}: radius {r} does not reach half of the {height}px pill height");
            }
        }
    }

    #[test]
    fn build_badge_style_hugs_its_content_and_centres_the_text() {
        for kind in ALL_KINDS {
            let props = properties(&build_badge_style(kind));
            let has = |p: &CssProperty| props.contains(p);

            assert!(has(&CssProperty::const_display(LayoutDisplay::Flex)), "{kind:?}: not a flex box");
            assert!(has(&CssProperty::const_flex_direction(LayoutFlexDirection::Row)), "{kind:?}: wrong flex direction");
            assert!(has(&CssProperty::const_justify_content(LayoutJustifyContent::Center)), "{kind:?}: text not centred");
            assert!(has(&CssProperty::const_align_items(LayoutAlignItems::Center)), "{kind:?}: text not centred");
            assert!(has(&CssProperty::const_text_align(StyleTextAlign::Center)), "{kind:?}: text not centred");
            // align-self: start + flex-grow: 0 — without both, the pill stretches
            // across a flex parent instead of hugging its label.
            assert!(has(&CssProperty::align_self(LayoutAlignSelf::Start)), "{kind:?}: badge stretches on the cross axis");
            assert!(
                has(&CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
                "{kind:?}: badge grows on the main axis"
            );
        }
    }

    #[test]
    fn build_badge_style_colours_track_the_kind() {
        for kind in ALL_KINDS {
            let style = build_badge_style(kind);
            let (bg, text) = kind.colors();
            assert_eq!(background_color(&style), Some(bg), "{kind:?}: emitted background != colors().0");
            assert_eq!(text_color(&style), Some(text), "{kind:?}: emitted text colour != colors().1");
        }
    }

    #[test]
    fn build_badge_style_declares_every_property_at_most_once() {
        // A duplicated declaration is a last-one-wins ambiguity: two backgrounds
        // would make one of them silently dead.
        for kind in ALL_KINDS {
            let props = properties(&build_badge_style(kind));
            let mut seen = HashSet::new();
            for p in &props {
                assert!(seen.insert(core::mem::discriminant(p)), "{kind:?}: duplicate declaration of {p:?}");
            }
            assert_eq!(seen.len(), props.len());
        }
    }

    #[test]
    fn build_badge_style_properties_are_all_unconditional() {
        // A badge is stateless — a declaration gated on `:hover`/`:active` would
        // simply never paint.
        for kind in ALL_KINDS {
            for p in build_badge_style(kind).as_ref() {
                assert!(
                    p.apply_if.as_ref().is_empty(),
                    "{kind:?}: {:?} is conditional on a stateless widget",
                    p.property
                );
            }
        }
    }

    #[test]
    fn build_badge_style_is_deterministic_and_kind_dependent() {
        let len = build_badge_style(BadgeKind::Default).as_ref().len();
        for kind in ALL_KINDS {
            assert_eq!(
                properties(&build_badge_style(kind)),
                properties(&build_badge_style(kind)),
                "{kind:?}: two builds of the same kind disagree"
            );
            assert_eq!(
                build_badge_style(kind).as_ref().len(),
                len,
                "{kind:?}: emits a different number of declarations than Default"
            );
        }
        // No two kinds may collapse onto the same style, or the variant is a no-op.
        for (i, a) in ALL_KINDS.iter().enumerate() {
            for b in &ALL_KINDS[i + 1..] {
                assert_ne!(
                    properties(&build_badge_style(*a)),
                    properties(&build_badge_style(*b)),
                    "{a:?} and {b:?} produce an identical style"
                );
            }
        }
    }

    #[test]
    fn build_badge_style_emits_only_finite_non_negative_px_lengths() {
        // Guard the one numeric conversion in this file (`isize` -> `PixelValue`):
        // a NaN/inf/negative length must never reach the layout solver.
        for kind in ALL_KINDS {
            let values = all_pixel_values(&build_badge_style(kind));
            assert_eq!(values.len(), 9, "{kind:?}: expected 4 paddings + 4 radii + 1 font size");
            for pv in values {
                let n = px(&pv); // also asserts SizeMetric::Px
                assert!(n.is_finite(), "{kind:?}: non-finite length {n}");
                assert!(n >= 0.0, "{kind:?}: negative length {n}");
                assert!(n <= 128.0, "{kind:?}: implausibly large length {n} for a badge");
            }
        }
    }

    // ------------------------------------------------------------------
    // Badge::create / Badge::with_kind  (constructors)
    // ------------------------------------------------------------------

    #[test]
    fn create_defaults_to_grey_and_keeps_the_text_verbatim() {
        for s in adversarial_strings() {
            let b = Badge::create(AzString::from(s.clone()));
            assert_eq!(b.string.as_str(), s.as_str(), "the label was not preserved verbatim");
            assert_eq!(b.string.len(), s.len(), "byte length changed (NUL truncation?)");
            assert_eq!(b.kind, BadgeKind::Default, "create() must use the grey default kind");
            assert_eq!(properties(&b.badge_style), properties(&build_badge_style(BadgeKind::Default)));
        }
    }

    #[test]
    fn with_kind_stores_both_arguments_and_the_matching_style() {
        for kind in ALL_KINDS {
            for s in adversarial_strings() {
                let b = Badge::with_kind(AzString::from(s.clone()), kind);
                assert_eq!(b.string.as_str(), s.as_str(), "{kind:?}: label not preserved");
                assert_eq!(b.string.len(), s.len(), "{kind:?}: byte length changed");
                assert_eq!(b.kind, kind, "{kind:?}: kind field does not match the argument");
                // The invariant that makes `badge_style` a cache and not a lie.
                assert_eq!(properties(&b.badge_style), properties(&build_badge_style(kind)));
                assert_eq!(background_color(&b.badge_style), Some(kind.colors().0));
            }
        }
    }

    #[test]
    fn create_is_with_kind_default() {
        for s in ["", "99+", "\u{1F600}"] {
            assert_eq!(
                Badge::create(AzString::from_const_str(s)),
                Badge::with_kind(AzString::from_const_str(s), BadgeKind::Default)
            );
        }
    }

    #[test]
    fn default_badge_is_an_empty_grey_badge_and_equality_sees_every_field() {
        let d = Badge::default();
        assert_eq!(d, Badge::create(AzString::from_const_str("")));
        assert_eq!(d.string.as_str(), "");
        assert_eq!(d.kind, BadgeKind::Default);
        assert_eq!(d.clone(), d, "Clone must preserve equality");

        assert_ne!(d, Badge::create(AzString::from_const_str("9")), "the label must affect equality");
        assert_ne!(
            Badge::with_kind(AzString::from_const_str("9"), BadgeKind::Danger),
            Badge::with_kind(AzString::from_const_str("9"), BadgeKind::Success),
            "badges of different kinds must not compare equal"
        );
    }

    // ------------------------------------------------------------------
    // Badge::set_kind / with_badge_kind  (setters)
    // ------------------------------------------------------------------

    #[test]
    fn set_kind_recomputes_the_style_without_growing_it() {
        // A push-instead-of-replace bug would grow the style vec on every call and
        // leave stale (earlier-kind) colour declarations behind, which then win or
        // lose the cascade by accident.
        let mut b = Badge::create(AzString::from_const_str("99+"));
        let expected_len = build_badge_style(BadgeKind::Default).as_ref().len();

        for round in 0..50 {
            let kind = ALL_KINDS[round % ALL_KINDS.len()];
            b.set_kind(kind);

            assert_eq!(b.kind, kind, "round {round}: kind field not updated");
            assert_eq!(
                b.badge_style.as_ref().len(),
                expected_len,
                "round {round}: style vec changed length — stale declarations?"
            );
            assert_eq!(
                properties(&b.badge_style),
                properties(&build_badge_style(kind)),
                "round {round}: style does not match a freshly built one"
            );
            assert_eq!(background_color(&b.badge_style), Some(kind.colors().0), "round {round}: stale background");
            assert_eq!(b.string.as_str(), "99+", "round {round}: set_kind ate the label");
        }
    }

    #[test]
    fn with_badge_kind_agrees_with_set_kind_and_is_last_call_wins() {
        let chained = Badge::create(AzString::from_const_str("9"))
            .with_badge_kind(BadgeKind::Danger)
            .with_badge_kind(BadgeKind::Warning)
            .with_badge_kind(BadgeKind::Info);

        let mut mutated = Badge::create(AzString::from_const_str("9"));
        mutated.set_kind(BadgeKind::Danger);
        mutated.set_kind(BadgeKind::Warning);
        mutated.set_kind(BadgeKind::Info);

        assert_eq!(chained, mutated, "the builder and the mutator must agree");
        assert_eq!(chained.kind, BadgeKind::Info);
        assert_eq!(chained.string.as_str(), "9");
        assert_eq!(properties(&chained.badge_style), properties(&build_badge_style(BadgeKind::Info)));
        // In particular the Danger red must be completely gone.
        assert_eq!(background_color(&chained.badge_style), Some(BadgeKind::Info.colors().0));
    }

    #[test]
    fn setting_the_same_kind_twice_is_idempotent() {
        for kind in ALL_KINDS {
            let once = Badge::with_kind(AzString::from_const_str("x"), kind);
            let twice = once.clone().with_badge_kind(kind);
            assert_eq!(once, twice, "{kind:?}: re-setting the same kind changed the badge");
        }
    }

    // ------------------------------------------------------------------
    // Badge::swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_returns_the_original_and_leaves_a_default_behind() {
        let mut b = Badge::with_kind(AzString::from_const_str("99+"), BadgeKind::Danger);
        let taken = b.swap_with_default();

        // The returned value is the *original*, intact.
        assert_eq!(taken.string.as_str(), "99+");
        assert_eq!(taken.kind, BadgeKind::Danger);
        assert_eq!(properties(&taken.badge_style), properties(&build_badge_style(BadgeKind::Danger)));

        // What is left behind is a *default* badge — in particular its style must
        // be the grey one and not a stale Danger red.
        assert_eq!(b, Badge::default());
        assert_eq!(b.string.as_str(), "");
        assert_eq!(b.kind, BadgeKind::Default);
        assert_eq!(background_color(&b.badge_style), Some(BadgeKind::Default.colors().0), "the red survived the swap");
    }

    #[test]
    fn swap_with_default_is_idempotent_on_an_already_default_badge() {
        let mut b = Badge::default();
        let first = b.swap_with_default();
        let second = b.swap_with_default();
        assert_eq!(first, Badge::default());
        assert_eq!(second, Badge::default());
        assert_eq!(b, Badge::default());
    }

    #[test]
    fn swap_with_default_survives_a_huge_label_and_repeated_swaps() {
        let long = "x".repeat(100_000);
        let mut b = Badge::with_kind(AzString::from(long.clone()), BadgeKind::Success);
        for round in 0..10 {
            let taken = b.swap_with_default();
            if round == 0 {
                assert_eq!(taken.string.len(), long.len(), "the long label was truncated");
                assert_eq!(taken.kind, BadgeKind::Success);
            } else {
                assert_eq!(taken, Badge::default(), "round {round}: the emptied badge is not a default");
            }
            assert_eq!(b, Badge::default(), "round {round}: what was left behind is not a default");
        }
    }

    // ------------------------------------------------------------------
    // Badge::dom  (round-trip: badge -> DOM)
    // ------------------------------------------------------------------

    #[test]
    fn dom_is_a_single_classed_text_node_carrying_the_computed_style() {
        for kind in ALL_KINDS {
            let badge = Badge::with_kind(AzString::from_const_str("99+"), kind);
            let expected = properties(&badge.badge_style);
            let dom = badge.dom();

            assert!(has_class(&dom, "__azul-native-badge"), "{kind:?}: missing the widget class");
            assert!(dom.children.as_ref().is_empty(), "{kind:?}: a badge is a single text node, not a subtree");
            assert_eq!(inline_properties(&dom), expected, "{kind:?}: the pill lost its computed style");

            match dom.root.get_node_type() {
                NodeType::Text(s) => assert_eq!(s.as_ref().as_str(), "99+", "{kind:?}: the label was mangled"),
                other => panic!("{kind:?}: expected a text node, got {other:?}"),
            }
        }
    }

    #[test]
    fn dom_renders_the_kind_the_badge_was_last_set_to() {
        // `dom()` consumes the *cached* style, so a `set_kind` that forgot to
        // recompute would paint the previous colour here and nowhere else.
        for kind in ALL_KINDS {
            let mut badge = Badge::create(AzString::from_const_str("9"));
            badge.set_kind(BadgeKind::Danger);
            badge.set_kind(kind);
            let expected = properties(&build_badge_style(kind));
            assert_eq!(inline_properties(&badge.dom()), expected, "{kind:?}: the DOM does not show the current kind");
        }
    }

    #[test]
    fn dom_preserves_adversarial_labels_verbatim() {
        for s in adversarial_strings() {
            let dom = Badge::create(AzString::from(s.clone())).dom();
            match dom.root.get_node_type() {
                NodeType::Text(t) => {
                    assert_eq!(t.as_ref().as_str(), s.as_str(), "the label changed on its way into the DOM");
                    assert_eq!(t.as_ref().len(), s.len(), "byte length changed (NUL truncation?)");
                }
                other => panic!("expected a text node, got {other:?}"),
            }
            assert!(has_class(&dom, "__azul-native-badge"));
        }
    }

    #[test]
    fn from_badge_for_dom_is_exactly_dom() {
        for kind in ALL_KINDS {
            let badge = Badge::with_kind(AzString::from_const_str("ok"), kind);
            let via_into: Dom = badge.clone().into();
            let via_dom = badge.dom();
            assert_eq!(inline_properties(&via_into), inline_properties(&via_dom), "{kind:?}: `From` diverges from `dom()`");
            assert_eq!(via_into.root.get_node_type(), via_dom.root.get_node_type(), "{kind:?}: `From` built a different node");
        }
    }
}
