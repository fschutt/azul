//! Label widget for displaying static text with platform-specific default styling.

use azul_core::dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use azul_css::{
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

/// A static text label widget with platform-appropriate default styling.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Label {
    pub string: AzString,
    pub label_style: CssPropertyWithConditionsVec,
}

const SANS_SERIF_STR: &str = "system:ui";
const SANS_SERIF: AzString = AzString::from_const_str(SANS_SERIF_STR);
const SANS_SERIF_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SANS_SERIF)];
const SANS_SERIF_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SANS_SERIF_FAMILIES);

/// Standard label text color (#4C4C4C), matching platform UI defaults.
const COLOR_4C4C4C: ColorU = ColorU {
    r: 76,
    g: 76,
    b: 76,
    a: 255,
};

static LABEL_STYLE_DEFAULT: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

static LABEL_STYLE_MAC: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(12))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

/// No default styling on unsupported platforms (e.g. WASM, FreeBSD);
/// callers should provide explicit styles via `label_style`.
static LABEL_STYLE_OTHER: &[CssPropertyWithConditions] = &[];

impl Label {
    /// Creates a new label with the given text and platform-specific default styling.
    #[inline]
    #[must_use]
    pub fn create(string: AzString) -> Self {
        Self {
            string,
            #[cfg(target_os = "windows")]
            label_style: CssPropertyWithConditionsVec::from_const_slice(LABEL_STYLE_DEFAULT),
            #[cfg(target_os = "linux")]
            label_style: CssPropertyWithConditionsVec::from_const_slice(LABEL_STYLE_DEFAULT),
            #[cfg(target_os = "macos")]
            label_style: CssPropertyWithConditionsVec::from_const_slice(LABEL_STYLE_MAC),
            #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
            label_style: CssPropertyWithConditionsVec::from_const_slice(LABEL_STYLE_OTHER),
        }
    }

    /// Replaces `self` with an empty default label, returning the original.
    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this label into a `<p>` block carrying the
    /// `__azul-native-label` class and wrapping a bare text node.
    ///
    /// The `<p>` is the styled box: a `NodeType::Text` node is always
    /// inline-level and owns no rect, so every box-model property here would
    /// be inert on a raw text node.
    #[inline]
    #[must_use]
    pub fn dom(self) -> Dom {
        static LABEL_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-label"))];

        crate::widgets::widget_p_with_text(self.string)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(LABEL_CLASS))
            .with_css_props(self.label_style)
    }
}

impl From<Label> for Dom {
    fn from(l: Label) -> Self {
        l.dom()
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

    /// The number of declarations each *populated* platform table carries.
    const DECL_COUNT: usize = 9;

    /// The class `dom()` stamps onto the label `<p>`.
    const LABEL_CLASS_NAME: &str = "__azul-native-label";

    /// The style table `Label::create` is expected to pick on *this* target.
    ///
    /// Uses `cfg!` rather than `#[cfg]` so every branch still type-checks on
    /// every platform — a table that stopped compiling on macOS would otherwise
    /// only be caught by a macOS CI run.
    fn expected_table() -> &'static [CssPropertyWithConditions] {
        if cfg!(target_os = "macos") {
            LABEL_STYLE_MAC
        } else if cfg!(any(target_os = "windows", target_os = "linux")) {
            LABEL_STYLE_DEFAULT
        } else {
            LABEL_STYLE_OTHER
        }
    }

    /// The font size `Label::create` bakes in on this target — `None` on the
    /// platforms that are documented to get no default styling at all.
    fn expected_font_size() -> Option<f32> {
        if cfg!(target_os = "macos") {
            Some(12.0)
        } else if cfg!(any(target_os = "windows", target_os = "linux")) {
            Some(13.0)
        } else {
            None
        }
    }

    /// True when this target is one of the platforms that gets a real table.
    fn target_is_styled() -> bool {
        !expected_table().is_empty()
    }

    /// The declared properties of a style vec, in declaration order.
    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length. A
    /// font size declared in `em`/`%` would resolve against the *parent* font
    /// and make the "platform default" scale with whatever it is dropped into.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "label lengths must be absolute px, got {:?}",
            pv.metric
        );
        pv.number.get()
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

    fn flex_grow(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::FlexGrow(f) => f.get_property().map(|f| f.inner.get()),
            _ => None,
        })
    }

    fn font_families(v: &CssPropertyWithConditionsVec) -> Option<Vec<StyleFontFamily>> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::FontFamily(f) => f.get_property().map(|f| f.as_ref().to_vec()),
            _ => None,
        })
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

    /// The text carried by a text node, looking through the `<p>` block
    /// wrapper the label convention mandates (`p > text`).
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

    /// `Label` derives neither `PartialEq` nor `Default`, so compare field-wise.
    fn assert_same_label(a: &Label, b: &Label, ctx: &str) {
        assert_eq!(a.string, b.string, "{ctx}: the string differs");
        assert_eq!(a.label_style, b.label_style, "{ctx}: the style differs");
    }

    /// Inputs a label must survive verbatim. Empty, whitespace-only, embedded
    /// NUL, combining marks, ZWJ sequences, bidi overrides, strings that look
    /// like this widget's own sentinels, and one very large allocation.
    fn adversarial_strings() -> Vec<String> {
        let mut v: Vec<String> = [
            "",
            "Label",
            " ",
            "\t\n\r",
            "e\u{0301}",                                   // e + combining acute
            "\u{212B}",                                    // ANGSTROM SIGN (NFC-folds to U+00C5)
            "\u{C5}",                                      // the folded form — must stay distinct
            "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}", // ZWJ family emoji
            "\u{5E9}\u{5DC}\u{5D5}\u{5DD}",                // RTL Hebrew
            "\0",                                          // a single NUL
            "a\0b",                                        // embedded NUL
            "\u{FFFD}\u{202E}\u{200B}",                    // replacement, RTL override, ZWSP
            "system:ui",                                   // the font sentinel this file bakes in
            "__azul-native-label",                         // this widget's own class name
            "}\n#pwned { color: red; }",                   // CSS-injection shaped
            "<div>&amp;</div>",                            // markup shaped
            "-9223372036854775808",                        // i64::MIN
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        v.push("x".repeat(200_000));
        v.push("\0".repeat(1_000));
        v
    }

    // ------------------------------------------------------------------
    // Label::create  (constructor)
    // ------------------------------------------------------------------

    #[test]
    fn create_stores_every_adversarial_string_verbatim() {
        // The string travels through `AzString` (an FFI `U8Vec`), so a
        // C-string-style NUL truncation or a lossy UTF-8 round-trip would show
        // up as a shortened or mangled label here first.
        for s in adversarial_strings() {
            let label = Label::create(AzString::from(s.clone()));
            assert_eq!(
                label.string.as_str(),
                s.as_str(),
                "the string was rewritten"
            );
            assert_eq!(
                label.string.len(),
                s.len(),
                "byte length changed (NUL truncation?)"
            );
            assert_eq!(
                label.string.chars().count(),
                s.chars().count(),
                "char count changed (lossy UTF-8 round-trip?)"
            );
        }
    }

    #[test]
    fn create_never_normalises_or_folds_the_string() {
        // U+212B and U+00C5 render identically but are different code points;
        // a normalisation pass hidden in the string plumbing would silently
        // merge them and break byte-exact round-trips through FFI.
        let angstrom = Label::create(AzString::from("\u{212B}".to_string()));
        let a_ring = Label::create(AzString::from("\u{C5}".to_string()));
        assert_ne!(
            angstrom.string, a_ring.string,
            "the two forms were normalised into one"
        );
        assert_eq!(
            angstrom.string.len(),
            3,
            "U+212B must stay a 3-byte sequence"
        );
        assert_eq!(a_ring.string.len(), 2, "U+00C5 must stay a 2-byte sequence");
    }

    #[test]
    fn create_uses_the_platform_table_for_this_target() {
        let label = Label::create(AzString::from_const_str("hello"));
        assert_eq!(
            properties(&label.label_style),
            expected_table()
                .iter()
                .map(|p| p.property.clone())
                .collect::<Vec<_>>(),
            "the constructor picked the wrong platform style table"
        );
        assert_eq!(font_size_px(&label.label_style), expected_font_size());
    }

    #[test]
    fn create_is_deterministic_and_independent_of_the_string() {
        // The style must not depend on the text: two labels built from wildly
        // different strings must carry byte-identical styling.
        let a = Label::create(AzString::from_const_str(""));
        let b = Label::create(AzString::from("x".repeat(100_000)));
        assert_eq!(
            a.label_style, b.label_style,
            "the style varies with the label text"
        );
        assert_same_label(
            &a,
            &Label::create(AzString::from_const_str("")),
            "repeat construction",
        );
    }

    #[test]
    fn constructed_style_vec_has_consistent_length_and_capacity() {
        let v = Label::create(AzString::from_const_str("hi")).label_style;
        assert_eq!(
            v.len(),
            v.as_ref().len(),
            "len() disagrees with the slice view"
        );
        assert!(
            v.capacity() >= v.len(),
            "capacity {} < len {}",
            v.capacity(),
            v.len()
        );
        if target_is_styled() {
            assert_eq!(v.len(), DECL_COUNT, "unexpected number of declarations");
        } else {
            assert!(
                v.is_empty(),
                "unsupported platforms are documented to get no styling"
            );
        }
    }

    #[test]
    fn cloning_and_dropping_never_corrupts_the_shared_static_style() {
        // `label_style` borrows a `'static` slice (`NoDestructor`) while
        // `string` may be heap-owned. A clone that wrongly claims ownership of
        // the static, or a `Drop` that frees it, would corrupt every future
        // label — so churn hard and re-check both the original and a fresh build.
        let base = Label::create(AzString::from("churn \u{1F600}".to_string()));
        let expected_props = properties(&base.label_style);
        for round in 0..1000 {
            let c = base.clone();
            assert_eq!(
                c.string.as_str(),
                "churn \u{1F600}",
                "clone {round}: the string diverged"
            );
            assert_eq!(
                properties(&c.label_style),
                expected_props,
                "clone {round}: the style diverged"
            );
            drop(c);
        }
        assert_eq!(
            base.string.as_str(),
            "churn \u{1F600}",
            "the original string was damaged"
        );
        assert_eq!(
            properties(&base.label_style),
            expected_props,
            "the original style was damaged"
        );
        assert_eq!(
            properties(&Label::create(AzString::from_const_str("x")).label_style),
            expected_props,
            "a freshly built label disagrees after 1000 clone/drop cycles"
        );
    }

    #[test]
    fn a_const_str_label_survives_clone_churn_of_its_static_backing() {
        // `AzString::from_const_str` is `NoDestructor`-backed; `clone_self`
        // copies it. A shallow clone plus a `Drop` that freed the `'static`
        // would be a use-after-free the very next read.
        let base = Label::create(AzString::from_const_str(LABEL_CLASS_NAME));
        for round in 0..1000 {
            let c = base.clone();
            assert_eq!(
                c.string.as_str(),
                LABEL_CLASS_NAME,
                "clone {round}: static backing corrupted"
            );
            drop(c);
        }
        assert_eq!(base.string.as_str(), LABEL_CLASS_NAME);
    }

    // ------------------------------------------------------------------
    // The platform style tables
    // ------------------------------------------------------------------

    #[test]
    fn the_two_populated_tables_differ_only_in_font_size() {
        // They are hand-duplicated blocks: a fix applied to one and not the
        // other is the failure mode this file is most exposed to.
        assert_eq!(LABEL_STYLE_DEFAULT.len(), DECL_COUNT);
        assert_eq!(
            LABEL_STYLE_MAC.len(),
            DECL_COUNT,
            "the two tables have diverged in length"
        );
        for (i, (d, m)) in LABEL_STYLE_DEFAULT
            .iter()
            .zip(LABEL_STYLE_MAC.iter())
            .enumerate()
        {
            assert_eq!(
                core::mem::discriminant(&d.property),
                core::mem::discriminant(&m.property),
                "declaration {i} is a different property on the two platforms"
            );
            if matches!(d.property, CssProperty::FontSize(_)) {
                assert_ne!(
                    d.property, m.property,
                    "the two tables were expected to differ in font size"
                );
            } else {
                assert_eq!(
                    d.property, m.property,
                    "declaration {i} drifted between the two tables"
                );
            }
        }
    }

    #[test]
    fn label_style_other_is_empty_as_documented() {
        // The doc comment promises "no default styling on unsupported
        // platforms"; a stray declaration here would style WASM/FreeBSD
        // differently from everything the other two tables were tuned against.
        assert!(
            LABEL_STYLE_OTHER.is_empty(),
            "the fallback table is no longer empty"
        );
    }

    #[test]
    fn every_declaration_is_unconditional() {
        // A label is stateless — a declaration gated on `:hover`/`:active`
        // would simply never paint.
        for (name, table) in [("default", LABEL_STYLE_DEFAULT), ("mac", LABEL_STYLE_MAC)] {
            for p in table {
                assert!(
                    p.apply_if.as_ref().is_empty(),
                    "{name}: {:?} is conditional on a stateless widget",
                    p.property
                );
            }
        }
    }

    #[test]
    fn no_property_is_declared_twice() {
        // A duplicated declaration is a last-one-wins ambiguity: two font sizes
        // would make one of them silently dead.
        for (name, table) in [("default", LABEL_STYLE_DEFAULT), ("mac", LABEL_STYLE_MAC)] {
            let mut seen = HashSet::new();
            for p in table {
                assert!(
                    seen.insert(core::mem::discriminant(&p.property)),
                    "{name}: duplicate declaration of {:?}",
                    p.property
                );
            }
            assert_eq!(seen.len(), table.len());
        }
    }

    #[test]
    fn the_text_colour_is_the_documented_opaque_grey() {
        // The doc comment pins it to #4C4C4C; a translucent label would let the
        // background bleed through the glyphs.
        assert_eq!(
            COLOR_4C4C4C,
            ColorU {
                r: 76,
                g: 76,
                b: 76,
                a: 255
            }
        );
        assert_eq!(
            COLOR_4C4C4C.a, 255,
            "a translucent label lets the background bleed through"
        );
        assert_eq!(
            COLOR_4C4C4C.r, COLOR_4C4C4C.g,
            "the label colour is not neutral grey"
        );
        assert_eq!(
            COLOR_4C4C4C.g, COLOR_4C4C4C.b,
            "the label colour is not neutral grey"
        );

        for (name, table) in [("default", LABEL_STYLE_DEFAULT), ("mac", LABEL_STYLE_MAC)] {
            let v = CssPropertyWithConditionsVec::from_const_slice(table);
            assert_eq!(
                text_color(&v),
                Some(COLOR_4C4C4C),
                "{name}: wrong text colour"
            );
        }
    }

    #[test]
    fn every_font_size_is_a_finite_positive_absolute_px() {
        // Guard the `isize` -> `PixelValue` fixed-point conversion: a NaN, an
        // infinity, a zero or a negative size must never reach the shaper.
        for (name, table, want) in [
            ("default", LABEL_STYLE_DEFAULT, 13.0),
            ("mac", LABEL_STYLE_MAC, 12.0),
        ] {
            let v = CssPropertyWithConditionsVec::from_const_slice(table);
            let size = font_size_px(&v).expect("a label must declare a font size");
            assert!(size.is_finite(), "{name}: non-finite font size {size}");
            assert!(!size.is_nan(), "{name}: NaN font size");
            assert!(size > 0.0, "{name}: a {size}px label is invisible");
            assert!(
                size <= 128.0,
                "{name}: {size}px is implausible for a UI label"
            );
            assert_eq!(size, want, "{name}: unexpected font size");
        }
    }

    #[test]
    fn the_fixed_point_length_encoding_round_trips() {
        // encode == decode for every numeric constant this file bakes in.
        assert_eq!(PixelValue::const_px(13).number.get(), 13.0);
        assert_eq!(PixelValue::const_px(12).number.get(), 12.0);
        assert_eq!(StyleFontSize::const_px(13).inner.number.get(), 13.0);
        assert_eq!(StyleFontSize::const_px(12).inner.number.get(), 12.0);
        assert_eq!(LayoutFlexGrow::const_new(1).inner.get(), 1.0);

        // ...and the values that actually landed in the tables are the ones the
        // declarations asked for.
        let d: Vec<CssProperty> = LABEL_STYLE_DEFAULT
            .iter()
            .map(|p| p.property.clone())
            .collect();
        assert!(d.contains(&CssProperty::const_font_size(StyleFontSize::const_px(13))));
        let m: Vec<CssProperty> = LABEL_STYLE_MAC.iter().map(|p| p.property.clone()).collect();
        assert!(m.contains(&CssProperty::const_font_size(StyleFontSize::const_px(12))));
    }

    #[test]
    fn flex_grow_is_exactly_one_and_not_a_rounding_artefact() {
        // `FloatValue` stores a fixed-point `isize`; a botched encode/decode
        // would show up as 0.999 or -0.0 rather than a clean 1.
        for (name, table) in [("default", LABEL_STYLE_DEFAULT), ("mac", LABEL_STYLE_MAC)] {
            let v = CssPropertyWithConditionsVec::from_const_slice(table);
            let g = flex_grow(&v).expect("flex-grow must be declared");
            assert!(g.is_finite(), "{name}: non-finite flex-grow {g}");
            assert_eq!(g, 1.0, "{name}: flex-grow is {g}, not 1");
            assert!(g.is_sign_positive(), "{name}: flex-grow decoded as -0.0");
        }
    }

    #[test]
    fn the_font_family_is_a_single_system_ui_entry() {
        // `SANS_SERIF` / `SANS_SERIF_FAMILY` are `const`, so each mention
        // materialises a fresh value — bind them once and read them back.
        let sentinel: AzString = SANS_SERIF;
        let family_vec: StyleFontFamilyVec = SANS_SERIF_FAMILY;

        assert_eq!(SANS_SERIF_STR, "system:ui");
        assert_eq!(
            sentinel.as_str(),
            SANS_SERIF_STR,
            "the const AzString lost its backing str"
        );
        assert_eq!(sentinel.as_str().len(), SANS_SERIF_STR.len());
        assert_eq!(
            SANS_SERIF_FAMILIES.len(),
            1,
            "the fallback chain changed length"
        );
        assert_eq!(
            family_vec.as_ref().len(),
            1,
            "the const vec disagrees with its slice"
        );

        for (name, table) in [("default", LABEL_STYLE_DEFAULT), ("mac", LABEL_STYLE_MAC)] {
            let v = CssPropertyWithConditionsVec::from_const_slice(table);
            let fams = font_families(&v).expect("a label must declare a font family");
            assert_eq!(fams.len(), 1, "{name}: unexpected fallback chain length");
            match &fams[0] {
                // Pinned as-is: this file spells the UI font as a *named*
                // family whose name happens to be the `system:ui` sentinel,
                // not as `StyleFontFamily::SystemType(SystemFontType::Ui)`.
                StyleFontFamily::System(s) => {
                    assert_eq!(s.as_str(), "system:ui", "{name}: wrong family name")
                }
                other => panic!("{name}: unexpected font family variant {other:?}"),
            }
        }
    }

    #[test]
    fn the_centering_declarations_are_mutually_consistent() {
        // A label centres its text both ways; each half of that contract lives
        // in a different declaration, so assert them together.
        for (name, table) in [("default", LABEL_STYLE_DEFAULT), ("mac", LABEL_STYLE_MAC)] {
            let props: Vec<CssProperty> = table.iter().map(|p| p.property.clone()).collect();
            let has = |p: &CssProperty| props.contains(p);
            assert!(
                has(&CssProperty::const_display(LayoutDisplay::Flex)),
                "{name}: not a flex box"
            );
            assert!(
                has(&CssProperty::const_flex_direction(
                    LayoutFlexDirection::Column
                )),
                "{name}: not a column"
            );
            assert!(
                has(&CssProperty::const_justify_content(
                    LayoutJustifyContent::Center
                )),
                "{name}: not centred on the main axis"
            );
            assert!(
                has(&CssProperty::const_align_items(LayoutAlignItems::Center)),
                "{name}: not centred on the cross axis"
            );
            assert!(
                has(&CssProperty::const_text_align(StyleTextAlign::Center)),
                "{name}: the text itself is not centred"
            );
        }
    }

    // ------------------------------------------------------------------
    // Label::swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_returns_the_original_and_leaves_an_empty_label() {
        let mut label = Label::create(AzString::from("original \u{1F600}".to_string()));
        let expected_style = label.label_style.clone();
        let taken = label.swap_with_default();

        // The returned value is the *original*, intact.
        assert_eq!(
            taken.string.as_str(),
            "original \u{1F600}",
            "the wrong value was returned"
        );
        assert_eq!(
            taken.label_style, expected_style,
            "the returned label lost its style"
        );

        // What is left behind is a freshly created empty label — not a hollowed
        // out husk with a dangling string or an empty style vec.
        assert_eq!(
            label.string.as_str(),
            "",
            "what was left behind is not empty"
        );
        assert!(label.string.is_empty());
        assert_same_label(
            &label,
            &Label::create(AzString::from_const_str("")),
            "left-behind label",
        );
    }

    #[test]
    fn swap_with_default_is_idempotent_on_an_already_empty_label() {
        let mut label = Label::create(AzString::from_const_str(""));
        let first = label.swap_with_default();
        let second = label.swap_with_default();
        assert_eq!(first.string.as_str(), "");
        assert_eq!(second.string.as_str(), "");
        assert_same_label(
            &label,
            &Label::create(AzString::from_const_str("")),
            "after two swaps",
        );
    }

    #[test]
    fn repeated_swaps_never_corrupt_the_static_backed_style() {
        // `mem::swap` moves a vec that borrows a `'static` slice; 200 rounds of
        // swap-and-drop would surface a double free or a dangling `ptr`.
        let mut label = Label::create(AzString::from("swap me".to_string()));
        for round in 0..200 {
            let taken = label.swap_with_default();
            if round == 0 {
                assert_eq!(
                    taken.string.as_str(),
                    "swap me",
                    "round 0: wrong value returned"
                );
            } else {
                assert_eq!(
                    taken.string.as_str(),
                    "",
                    "round {round}: the emptied slot was not empty"
                );
            }
            assert_eq!(
                properties(&taken.label_style),
                properties(&label.label_style),
                "round {round}"
            );
            assert_eq!(
                label.string.as_str(),
                "",
                "round {round}: what was left behind is not empty"
            );
            drop(taken);
        }
        assert_same_label(
            &label,
            &Label::create(AzString::from_const_str("")),
            "after 200 swaps",
        );
    }

    #[test]
    fn swap_with_default_returns_a_custom_style_untouched() {
        // Both fields are `pub`, so a caller can hand-build a label. The swap
        // must hand that style back rather than rewriting it on the way out.
        let custom =
            CssPropertyWithConditionsVec::from_vec(vec![CssPropertyWithConditions::simple(
                CssProperty::const_font_size(StyleFontSize::const_px(42)),
            )]);
        let mut label = Label {
            string: AzString::from_const_str("custom"),
            label_style: custom,
        };
        let taken = label.swap_with_default();
        assert_eq!(taken.string.as_str(), "custom");
        assert_eq!(
            taken.label_style.len(),
            1,
            "the custom style was rewritten on the way out"
        );
        assert_eq!(font_size_px(&taken.label_style), Some(42.0));
        // ...and the slot is refilled with the platform default, not the custom one.
        assert_eq!(
            font_size_px(&label.label_style),
            expected_font_size(),
            "the 42px override survived"
        );
    }

    #[test]
    fn swap_with_default_hands_back_a_huge_string_without_truncating_it() {
        let huge = "\u{1F600}".repeat(50_000);
        let mut label = Label::create(AzString::from(huge.clone()));
        let taken = label.swap_with_default();
        assert_eq!(
            taken.string.len(),
            huge.len(),
            "the huge string was truncated on the way out"
        );
        assert_eq!(taken.string.as_str(), huge.as_str());
        assert!(label.string.is_empty());
    }

    // ------------------------------------------------------------------
    // Label::dom  (round-trip: label -> DOM)
    // ------------------------------------------------------------------

    #[test]
    fn dom_is_a_single_classed_text_node_carrying_the_computed_style() {
        let label = Label::create(AzString::from_const_str("Hello"));
        let expected = properties(&label.label_style);
        let dom = label.dom();

        assert_eq!(
            text_of(&dom),
            Some("Hello"),
            "the label is not a text node, or was mangled"
        );
        assert!(
            has_class(&dom, LABEL_CLASS_NAME),
            "missing the widget class"
        );
        assert_eq!(
            dom.root.get_ids_and_classes().as_ref().len(),
            1,
            "expected exactly one class and no ids"
        );
        assert_eq!(
            dom.children.as_ref().len(),
            1,
            "a label is a <p> wrapping exactly one bare text node"
        );
        assert!(
            dom.children.as_ref()[0].children.as_ref().is_empty(),
            "the text node itself is a leaf, not a subtree"
        );
        assert_eq!(
            dom.estimated_total_children, 1,
            "the <p> owns exactly its text node"
        );
        // The ONLY sheet a <p> widget carries is the UA-margin reset
        // (`widgets::widget_p_margin_reset`), scoped to the node itself.
        assert_eq!(
            dom.css.as_ref().len(),
            1,
            "a label attaches exactly the <p> margin reset, nothing else"
        );
        assert!(
            dom.root.callbacks.as_ref().is_empty(),
            "a static widget must not bind callbacks"
        );
        assert_eq!(
            inline_properties(&dom),
            expected,
            "the label lost its computed style"
        );
    }

    #[test]
    fn dom_preserves_adversarial_labels_verbatim() {
        for s in adversarial_strings() {
            let dom = Label::create(AzString::from(s.clone())).dom();
            let t = text_of(&dom).expect("expected a text node");
            assert_eq!(t, s.as_str(), "the label changed on its way into the DOM");
            assert_eq!(t.len(), s.len(), "byte length changed (NUL truncation?)");
            assert!(
                has_class(&dom, LABEL_CLASS_NAME),
                "the class was lost for {s:?}"
            );
            // The text must never leak into the style, and the style must never
            // vary with the text.
            assert_eq!(
                inline_properties(&dom).len(),
                expected_table().len(),
                "style varies with the text"
            );
        }
    }

    #[test]
    fn dom_of_an_empty_label_is_still_a_classed_text_node() {
        // The empty label is what `swap_with_default` leaves behind, so it is
        // the one input guaranteed to be rendered somewhere.
        let dom = Label::create(AzString::from_const_str("")).dom();
        assert_eq!(
            text_of(&dom),
            Some(""),
            "the empty label is not a text node"
        );
        assert!(has_class(&dom, LABEL_CLASS_NAME));
        assert_eq!(inline_properties(&dom).len(), expected_table().len());
    }

    #[test]
    fn dom_renders_the_style_field_verbatim_even_when_hand_built() {
        // `dom()` consumes `label_style` as-is. A hand-built label must be
        // rendered with exactly the style it was given — no re-derivation.
        let custom = CssPropertyWithConditionsVec::from_vec(vec![
            CssPropertyWithConditions::simple(CssProperty::const_font_size(
                StyleFontSize::const_px(99),
            )),
            CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Left)),
        ]);
        let dom = Label {
            string: AzString::from_const_str("hand built"),
            label_style: custom.clone(),
        }
        .dom();
        assert_eq!(
            inline_properties(&dom),
            properties(&custom),
            "the custom style was rewritten"
        );
        assert!(
            has_class(&dom, LABEL_CLASS_NAME),
            "a hand-built label lost the widget class"
        );
        assert_eq!(text_of(&dom), Some("hand built"));
    }

    #[test]
    fn dom_of_a_style_less_label_carries_no_inline_declarations() {
        // The `LABEL_STYLE_OTHER` shape: an empty style vec must produce an
        // empty inline style, not a phantom rule block.
        let dom = Label {
            string: AzString::from_const_str("bare"),
            label_style: CssPropertyWithConditionsVec::from_const_slice(LABEL_STYLE_OTHER),
        }
        .dom();
        assert!(
            inline_properties(&dom).is_empty(),
            "an empty style vec produced declarations"
        );
        assert!(has_class(&dom, LABEL_CLASS_NAME));
        assert_eq!(text_of(&dom), Some("bare"));
    }

    #[test]
    fn the_widget_class_is_a_namespaced_ascii_css_identifier() {
        let dom = Label::create(AzString::from_const_str("x")).dom();
        let classes = dom.root.get_ids_and_classes();
        let name = classes
            .as_ref()
            .iter()
            .find_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .expect("the label must carry a class");

        assert_eq!(name, LABEL_CLASS_NAME);
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
    fn a_label_whose_text_is_the_class_name_does_not_gain_a_second_class() {
        // `dom()` sets ids-and-classes *after* the text node is built; a
        // confusion between the two would show up as an extra attribute here.
        let dom = Label::create(AzString::from_const_str(LABEL_CLASS_NAME)).dom();
        assert_eq!(
            dom.root.get_ids_and_classes().as_ref().len(),
            1,
            "the text leaked into the class list"
        );
        assert_eq!(text_of(&dom), Some(LABEL_CLASS_NAME));
    }

    #[test]
    fn from_label_for_dom_is_exactly_dom() {
        for s in ["", "ok", "\u{1F600}\0"] {
            let label = Label::create(AzString::from(s.to_string()));
            let via_into: Dom = label.clone().into();
            let via_dom = label.dom();
            assert_eq!(via_into, via_dom, "{s:?}: `From` diverges from `dom()`");
        }
    }

    #[test]
    fn building_many_doms_never_corrupts_the_shared_static_class_list() {
        // `dom()` hands a `'static`-backed `IdOrClassVec` to each node it
        // builds; 1000 build-and-drop rounds would surface a double free.
        for round in 0..1000 {
            let dom = Label::create(AzString::from_const_str("churn")).dom();
            assert!(
                has_class(&dom, LABEL_CLASS_NAME),
                "round {round}: the static class list was corrupted"
            );
            drop(dom);
        }
        let dom = Label::create(AzString::from_const_str("churn")).dom();
        assert!(
            has_class(&dom, LABEL_CLASS_NAME),
            "the static class list did not survive the churn"
        );
        assert_eq!(inline_properties(&dom).len(), expected_table().len());
    }
}
