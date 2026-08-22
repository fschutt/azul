//! Frame widget — a titled border container similar to an HTML `<fieldset>`
//! or a Windows group box.
//!
//! Renders a header with a centered title flanked by horizontal border lines,
//! and a bordered content area below.

use azul_core::dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec};
#[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

const BORDER_COLOR: ColorU = ColorU {
    r: 221,
    g: 221,
    b: 221,
    a: 255,
};

const STRING_16146701490593874959: AzString = AzString::from_const_str("system:ui");
const STYLE_FONT_FAMILY_8122988506401935406_ITEMS: &[StyleFontFamily] =
    &[StyleFontFamily::System(STRING_16146701490593874959)];

const CSS_MATCH_15775557796860201720_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-frame .__azul-native-frame-header .__azul-native-frame-header-before div
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(8)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: BORDER_COLOR,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: BORDER_COLOR,
        }),
    )),
    // .__azul-native-frame .__azul-native-frame-header .__azul-native-frame-header-before
    CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
        LayoutWidth::Px(PixelValue::const_px(5)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
        LayoutMarginTop {
            inner: PixelValue::const_px(6),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
];
const CSS_MATCH_15775557796860201720: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_15775557796860201720_PROPERTIES);

const CSS_MATCH_16739370686243728873_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-frame .__azul-native-frame-header
    CssPropertyWithConditions::simple(CssProperty::FontSize(StyleFontSizeValue::Exact(
        StyleFontSize {
            inner: PixelValue::const_px(11),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
        StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
    ))),
    CssPropertyWithConditions::simple(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
        LayoutAlignItems::End,
    ))),
];
const CSS_MATCH_16739370686243728873: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_16739370686243728873_PROPERTIES);

const CSS_MATCH_4236783900531286611_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-frame .__azul-native-frame-header p
    CssPropertyWithConditions::simple(CssProperty::TextAlign(StyleTextAlignValue::Exact(
        StyleTextAlign::Center,
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(3),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(0),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(0),
        },
    ))),
];
const CSS_MATCH_4236783900531286611: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_4236783900531286611_PROPERTIES);

const CSS_MATCH_8602559445190067154_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-frame
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(3),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(3),
        },
    ))),
];
const CSS_MATCH_8602559445190067154: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_8602559445190067154_PROPERTIES);

const CSS_MATCH_9156589477016488419_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-frame .__azul-native-frame-header .__azul-native-frame-header-after div
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(8)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: BORDER_COLOR,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: BORDER_COLOR,
        }),
    )),
    // .__azul-native-frame .__azul-native-frame-header .__azul-native-frame-header-after
    CssPropertyWithConditions::simple(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
        LayoutMarginTop {
            inner: PixelValue::const_px(6),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
];
const CSS_MATCH_9156589477016488419: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_9156589477016488419_PROPERTIES);

const CSS_MATCH_CONTENT_AREA_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-frame .__azul-native-frame-content (static properties)
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(
        LayoutPaddingRightValue::Exact(LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(
        LayoutPaddingLeftValue::Exact(LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(
        LayoutPaddingBottomValue::Exact(LayoutPaddingBottom {
            inner: PixelValue::const_px(5),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(
        LayoutPaddingTopValue::Exact(LayoutPaddingTop {
            inner: PixelValue::const_px(5),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::FontSize(
        StyleFontSizeValue::Exact(StyleFontSize {
            inner: PixelValue::const_px(11),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::FontFamily(
        StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(
            STYLE_FONT_FAMILY_8122988506401935406_ITEMS,
        )),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: BORDER_COLOR,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: BORDER_COLOR,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: BORDER_COLOR,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: BORDER_COLOR,
        }),
    )),
];

/// A titled border container widget, similar to an HTML `<fieldset>` or
/// a Windows group box. Displays a header with a centered title and a
/// bordered content area below.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Frame {
    pub title: AzString,
    pub flex_grow: f32,
    pub content: Dom,
}

impl Frame {
    /// Creates a new `Frame` with the given title and content DOM.
    #[must_use] pub const fn create(title: AzString, content: Dom) -> Self {
        Self {
            title,
            content,
            flex_grow: 0.0,
        }
    }

    /// Replaces `self` with a default frame and returns the original.
    #[must_use]
    pub const fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""), Dom::create_div());
        core::mem::swap(&mut s, self);
        s
    }

    /// Sets the flex-grow factor for the content area.
    pub const fn set_flex_grow(&mut self, flex_grow: f32) {
        self.flex_grow = flex_grow;
    }

    /// Builder-style setter for the flex-grow factor.
    #[must_use] pub const fn with_flex_grow(mut self, flex_grow: f32) -> Self {
        self.set_flex_grow(flex_grow);
        self
    }

    #[must_use] pub fn dom(self) -> Dom {
        Dom::create_div()
            .with_css_props(CSS_MATCH_8602559445190067154)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_14615537625743340639: &[IdOrClass] =
                    &[Class(AzString::from_const_str("__azul-native-frame"))];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_14615537625743340639)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::create_div()
                    .with_css_props(CSS_MATCH_16739370686243728873)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_17776797146874875377: &[IdOrClass] = &[Class(
                            AzString::from_const_str("__azul-native-frame-header"),
                        )];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_17776797146874875377)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::create_div()
                            .with_css_props(CSS_MATCH_15775557796860201720)
                            .with_ids_and_classes({
                                const IDS_AND_CLASSES_15264202958442287530: &[IdOrClass] =
                                    &[Class(AzString::from_const_str(
                                        "__azul-native-frame-header-before",
                                    ))];
                                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_15264202958442287530)
                            })
                            .with_children(DomVec::from_vec(vec![Dom::create_div()])),
                        crate::widgets::widget_p_with_text(self.title)
                            .with_css_props(CSS_MATCH_4236783900531286611),
                        Dom::create_div()
                            .with_css_props(CSS_MATCH_9156589477016488419)
                            .with_ids_and_classes({
                                const IDS_AND_CLASSES_5689091102265932280: &[IdOrClass] = &[Class(
                                    AzString::from_const_str("__azul-native-frame-header-after"),
                                )];
                                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_5689091102265932280)
                            })
                            .with_children(DomVec::from_vec(vec![Dom::create_div()])),
                    ])),
                Dom::create_div()
                    .with_css_props({
                        let mut props = vec![
                            CssPropertyWithConditions::simple(CssProperty::FlexGrow(
                                LayoutFlexGrowValue::Exact(LayoutFlexGrow {
                                    inner: FloatValue::new(self.flex_grow),
                                }),
                            )),
                        ];
                        props.extend_from_slice(CSS_MATCH_CONTENT_AREA_PROPERTIES);
                        CssPropertyWithConditionsVec::from_vec(props)
                    })
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_9898887665724137124: &[IdOrClass] = &[Class(
                            AzString::from_const_str("__azul-native-frame-content"),
                        )];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_9898887665724137124)
                    })
                    .with_children(vec![self.content].into()),
            ]))
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
    use std::collections::HashSet;

    use azul_core::dom::{NodeData, NodeType};

    use super::*;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Titles a caller can realistically hand to a frame: empty, an interior NUL,
    /// stacked combining marks, a ZWJ emoji sequence, an RTL override run and a
    /// replacement-char/BOM/control mix. The frame never parses or normalises its
    /// title — every one of these has to reach the DOM byte-for-byte.
    const ADVERSARIAL_TEXT: [&str; 6] = [
        "",
        "a\0b",
        "e\u{0301}\u{0301}\u{0301}",
        "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        "\u{202E}gnirts desrever\u{202C}",
        "\u{FFFD}\u{FEFF}\t\n",
    ];

    /// Every f32 the numeric surface of `set_flex_grow` has to survive.
    /// `FloatValue::new` multiplies by 1000 and casts to `isize` — NaN, the
    /// infinities and `f32::MAX` all hit the saturating-cast path, and anything
    /// below 0.001 truncates away.
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

    /// A frame with the given title over the given content.
    fn frame(title: &str, content: Dom) -> Frame {
        Frame::create(AzString::from(title), content)
    }

    /// The frame `swap_with_default` leaves behind: empty title, empty div, no growth.
    fn default_frame() -> Frame {
        Frame::create(AzString::from_const_str(""), Dom::create_div())
    }

    fn kids(dom: &Dom) -> &[Dom] {
        dom.children.as_ref()
    }

    /// The header row (`.__azul-native-frame-header`).
    fn header(dom: &Dom) -> &Dom {
        &kids(dom)[0]
    }

    /// The bordered content area (`.__azul-native-frame-content`).
    fn content_area(dom: &Dom) -> &Dom {
        &kids(dom)[1]
    }

    /// The left rule of the header (`.__azul-native-frame-header-before`).
    fn rule_before(dom: &Dom) -> &Dom {
        &kids(header(dom))[0]
    }

    /// The title `<p>` box, wedged between the two header rules.
    fn title_node(dom: &Dom) -> &Dom {
        &kids(header(dom))[1]
    }

    /// The right rule of the header (`.__azul-native-frame-header-after`).
    fn rule_after(dom: &Dom) -> &Dom {
        &kids(header(dom))[2]
    }

    /// The declared properties of a node's inline style, in declaration order.
    fn props_of(node: &NodeData) -> Vec<CssProperty> {
        node.style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    fn inline_props(dom: &Dom) -> Vec<CssProperty> {
        props_of(&dom.root)
    }

    /// The properties of a build-time constant, in declaration order.
    fn declared(v: &[CssPropertyWithConditions]) -> Vec<CssProperty> {
        v.iter().map(|p| p.property.clone()).collect()
    }

    /// The CSS classes of a node, in declaration order.
    fn classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    /// The text of a text node, looking through the `<p>` block wrapper the
    /// label convention mandates (`p > text`).
    fn text_of(dom: &Dom) -> Option<&str> {
        match dom.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            NodeType::P => match dom.children.as_ref() {
                [only] => match only.root.get_node_type() {
                    NodeType::Text(s) => Some(s.as_ref().as_str()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    /// The `flex-grow` factor as it actually lands in a node's style — i.e. *after*
    /// the lossy `f32 -> isize` encoding inside `FloatValue::new`.
    fn flex_grow_of(dom: &Dom) -> Option<f32> {
        dom.root
            .style
            .iter_inline_properties()
            .find_map(|(p, _)| match p {
                CssProperty::FlexGrow(v) => v.get_property().map(|f| f.inner.get()),
                _ => None,
            })
    }

    /// The `flex-grow` of the content area — the only place the caller's factor
    /// is allowed to surface.
    fn content_flex_grow(dom: &Dom) -> Option<f32> {
        flex_grow_of(content_area(dom))
    }

    fn count_flex_grow(dom: &Dom) -> usize {
        inline_props(dom)
            .iter()
            .filter(|p| matches!(p, CssProperty::FlexGrow(_)))
            .count()
    }

    /// The `f32` of a `PixelValue`, asserting the length is an absolute `px`. An
    /// `em`/`%` slipping into the frame chrome would resolve against the parent
    /// font/box instead of staying a fixed border, padding or rule size.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "frame geometry must be absolute px, got {:?}",
            pv.metric
        );
        pv.number.get()
    }

    /// The length a property declares, if it declares one at all.
    fn length_of(p: &CssProperty) -> Option<PixelValue> {
        match p {
            CssProperty::Height(v) => match v.get_property() {
                Some(LayoutHeight::Px(pv)) => Some(*pv),
                _ => None,
            },
            CssProperty::Width(v) => match v.get_property() {
                Some(LayoutWidth::Px(pv)) => Some(*pv),
                _ => None,
            },
            CssProperty::FontSize(v) => v.get_property().map(|x| x.inner),
            CssProperty::MarginTop(v) => v.get_property().map(|x| x.inner),
            CssProperty::PaddingTop(v) => v.get_property().map(|x| x.inner),
            CssProperty::PaddingBottom(v) => v.get_property().map(|x| x.inner),
            CssProperty::PaddingLeft(v) => v.get_property().map(|x| x.inner),
            CssProperty::PaddingRight(v) => v.get_property().map(|x| x.inner),
            CssProperty::BorderTopWidth(v) => v.get_property().map(|x| x.inner),
            CssProperty::BorderBottomWidth(v) => v.get_property().map(|x| x.inner),
            CssProperty::BorderLeftWidth(v) => v.get_property().map(|x| x.inner),
            CssProperty::BorderRightWidth(v) => v.get_property().map(|x| x.inner),
            _ => None,
        }
    }

    /// The border colour a property declares, if it declares one at all.
    fn border_color_of(p: &CssProperty) -> Option<ColorU> {
        match p {
            CssProperty::BorderTopColor(v) => v.get_property().map(|x| x.inner),
            CssProperty::BorderBottomColor(v) => v.get_property().map(|x| x.inner),
            CssProperty::BorderLeftColor(v) => v.get_property().map(|x| x.inner),
            CssProperty::BorderRightColor(v) => v.get_property().map(|x| x.inner),
            _ => None,
        }
    }

    fn walk<'a>(dom: &'a Dom, out: &mut Vec<&'a Dom>) {
        out.push(dom);
        for c in dom.children.as_ref() {
            walk(c, out);
        }
    }

    /// Every node of the tree, root first.
    fn all_nodes(dom: &Dom) -> Vec<&Dom> {
        let mut v = Vec::new();
        walk(dom, &mut v);
        v
    }

    /// The recursive descendant count. `Dom::estimated_total_children` is a
    /// *cached* value that, if too small, makes `convert_dom_into_compact_dom`
    /// under-allocate its arenas and panic on out-of-bounds writes — so it has to
    /// match this exactly.
    fn count_descendants(dom: &Dom) -> usize {
        dom.children
            .as_ref()
            .iter()
            .map(|c| 1 + count_descendants(c))
            .sum()
    }

    /// A chain of `depth` nested divs (depth kept modest: `Dom`'s `Drop`/`PartialEq`
    /// recurse).
    fn nested_divs(depth: usize) -> Dom {
        let mut d = Dom::create_div();
        for _ in 0..depth {
            d = Dom::create_div().with_child(d);
        }
        d
    }

    // ------------------------------------------------------------------
    // Frame::create
    // ------------------------------------------------------------------

    #[test]
    fn create_zeroes_flex_grow_and_stores_both_arguments_verbatim() {
        let f = frame("Settings", Dom::create_text_do_not_use_without_block_level_wrapper("body"));

        // Positive zero, not -0.0: a negative zero would flip the sign of the encoded
        // isize on some paths and is not what "no growth" means.
        assert_eq!(f.flex_grow.to_bits(), 0_u32, "flex_grow must start at +0.0");
        assert_eq!(f.title.as_str(), "Settings", "the title was not stored verbatim");
        assert_eq!(
            text_of(&f.content),
            Some("body"),
            "the content was not stored verbatim",
        );
    }

    #[test]
    fn create_stores_pathological_titles_byte_for_byte() {
        for t in ADVERSARIAL_TEXT {
            let f = frame(t, Dom::create_div());
            assert_eq!(f.title.as_str(), t, "the frame mangled or normalised its title");
            assert_eq!(
                f.title.as_str().len(),
                t.len(),
                "the byte length of the title changed (NUL truncation or re-encoding?)",
            );

            // ... and it survives the trip into the rendered DOM.
            let dom = f.dom();
            assert_eq!(
                text_of(title_node(&dom)),
                Some(t),
                "the title was corrupted on the way into the DOM",
            );
        }
    }

    #[test]
    fn create_accepts_a_very_long_title_without_truncating_it() {
        // 100k astral-plane chars: 400 KB of UTF-8 through the AzString/BoxOrStatic path.
        let huge: String = "\u{1F600}".repeat(100_000);
        let f = frame(&huge, Dom::create_div());
        assert_eq!(f.title.as_str().len(), 400_000, "the long title was truncated on store");

        let dom = f.dom();
        assert_eq!(
            text_of(title_node(&dom)).map(str::len),
            Some(400_000),
            "the long title was truncated on the way into the DOM",
        );
    }

    #[test]
    fn create_accepts_deeply_nested_and_very_wide_content() {
        let deep = frame("deep", nested_divs(64)).dom();
        assert_eq!(
            deep.estimated_total_children,
            count_descendants(&deep),
            "the cached child count desynced for deeply nested content",
        );
        assert_eq!(deep.estimated_total_children, 9 + 64, "frame chrome is 8 nodes + the content");

        let wide = frame(
            "wide",
            Dom::create_div().with_children(
                (0..2000).map(|_| Dom::create_div()).collect::<Vec<_>>().into(),
            ),
        )
        .dom();
        assert_eq!(
            wide.estimated_total_children,
            count_descendants(&wide),
            "the cached child count desynced for very wide content",
        );
        assert_eq!(wide.estimated_total_children, 9 + 2000);
    }

    #[test]
    fn frames_nest_without_desyncing_the_child_count_cache() {
        let inner = frame("inner", Dom::create_div()).dom();
        assert_eq!(inner.estimated_total_children, 9, "a bare frame is 9 nodes below the root");

        let outer = frame("outer", inner).dom();
        assert_eq!(
            outer.estimated_total_children,
            count_descendants(&outer),
            "nesting a frame inside a frame desynced the cached child count",
        );
        assert_eq!(outer.estimated_total_children, 18);
    }

    // ------------------------------------------------------------------
    // Frame::swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_moves_every_field_out_and_leaves_a_default() {
        let mut f = frame("payload", Dom::create_text_do_not_use_without_block_level_wrapper("body")).with_flex_grow(2.5);

        let taken = f.swap_with_default();

        assert_eq!(taken.title.as_str(), "payload", "the title did not travel out");
        assert_eq!(text_of(&taken.content), Some("body"), "the content did not travel out");
        assert_eq!(taken.flex_grow, 2.5, "flex_grow did not travel out");

        assert_eq!(f.title.as_str(), "", "the swapped-in default kept a title");
        assert_eq!(f.content, Dom::create_div(), "the swapped-in default is not an empty div");
        assert_eq!(
            f.flex_grow.to_bits(),
            0_u32,
            "the swapped-in default has a non-zero flex_grow",
        );
    }

    #[test]
    fn repeated_swap_with_default_never_accumulates_state() {
        let mut f = frame("x", Dom::create_text_do_not_use_without_block_level_wrapper("y")).with_flex_grow(1.0);
        let first = f.swap_with_default();
        assert_eq!(first.title.as_str(), "x");

        for i in 0..8 {
            let taken = f.swap_with_default();
            assert_eq!(taken.title.as_str(), "", "swap #{i} handed back a non-default title");
            assert_eq!(taken.content, Dom::create_div(), "swap #{i} handed back non-default content");
            assert_eq!(taken.flex_grow.to_bits(), 0_u32, "swap #{i} handed back a non-zero factor");
            assert_eq!(f.title.as_str(), "", "swap #{i} left a non-default title behind");
        }

        // The DOM of the drained frame is still a well-formed, empty-titled frame.
        let dom = f.dom();
        assert_eq!(kids(&dom).len(), 2);
        assert_eq!(text_of(title_node(&dom)), Some(""));
        assert_eq!(content_flex_grow(&dom), Some(0.0));
    }

    #[test]
    fn swap_with_default_preserves_a_nan_factor_in_the_value_it_hands_back() {
        // The setter never sanitises, so a NaN has to survive the swap unchanged —
        // only the *encoding* into the DOM is allowed to collapse it (see below).
        let mut f = default_frame().with_flex_grow(f32::NAN);
        let taken = f.swap_with_default();
        assert!(taken.flex_grow.is_nan(), "the NaN factor was rewritten by the swap");
        assert_eq!(f.flex_grow.to_bits(), 0_u32, "the NaN leaked into the swapped-in default");
    }

    #[test]
    fn swap_with_default_leaves_a_frame_that_still_renders() {
        // Guards against the swapped-in default sharing/aliasing the moved-out DOM:
        // both halves have to render independently, in either order.
        let mut f = frame("original", nested_divs(8));
        let taken = f.swap_with_default();

        let left_behind = f.dom();
        let moved_out = taken.dom();

        assert_eq!(text_of(title_node(&left_behind)), Some(""));
        assert_eq!(text_of(title_node(&moved_out)), Some("original"));
        assert_eq!(left_behind.estimated_total_children, 9);
        assert_eq!(moved_out.estimated_total_children, 9 + 8);
    }

    // ------------------------------------------------------------------
    // Frame::set_flex_grow / Frame::with_flex_grow  (numeric)
    // ------------------------------------------------------------------

    #[test]
    fn set_flex_grow_stores_the_bit_pattern_verbatim_without_sanitising() {
        // The setter is a plain assignment — it must not clamp, round or NaN-scrub.
        // (Sanitisation happens later, at encode time; see the tests below.)
        for v in ADVERSARIAL_FLOATS {
            let mut f = default_frame();
            f.set_flex_grow(v);
            if v.is_nan() {
                assert!(f.flex_grow.is_nan(), "a NaN flex_grow was silently rewritten");
            } else {
                assert_eq!(
                    f.flex_grow.to_bits(),
                    v.to_bits(),
                    "flex_grow {v} was not stored verbatim",
                );
            }
        }
    }

    #[test]
    fn set_flex_grow_is_last_write_wins() {
        let mut f = default_frame();
        for v in ADVERSARIAL_FLOATS {
            f.set_flex_grow(v);
        }
        assert!(f.flex_grow.is_nan(), "the final write (NaN) did not win");

        f.set_flex_grow(1.5);
        assert_eq!(f.flex_grow, 1.5, "a later write did not overwrite the NaN");
        assert_eq!(count_flex_grow(content_area(&f.dom())), 1, "flex-grow was declared twice");
    }

    #[test]
    fn with_flex_grow_is_exactly_set_flex_grow() {
        for v in ADVERSARIAL_FLOATS {
            let mut by_setter = default_frame();
            by_setter.set_flex_grow(v);
            let by_builder = default_frame().with_flex_grow(v);

            assert_eq!(
                by_setter.flex_grow.to_bits(),
                by_builder.flex_grow.to_bits(),
                "the builder and the setter disagree for {v}",
            );
        }
    }

    #[test]
    fn with_flex_grow_touches_only_the_numeric_field() {
        let f = frame("title", Dom::create_text_do_not_use_without_block_level_wrapper("body")).with_flex_grow(f32::NAN);

        assert_eq!(f.title.as_str(), "title", "with_flex_grow clobbered the title");
        assert_eq!(text_of(&f.content), Some("body"), "with_flex_grow clobbered the content");
    }

    #[test]
    fn flex_grow_zero_encodes_to_positive_zero_even_from_negative_zero() {
        for v in [0.0_f32, -0.0_f32] {
            let dom = default_frame().with_flex_grow(v).dom();
            let got = content_flex_grow(&dom).expect("flex-grow must always be declared");
            assert_eq!(got.to_bits(), 0_u32, "flex_grow {v} did not encode to +0.0 (got {got})");
        }
    }

    #[test]
    fn flex_grow_round_trips_through_the_dom_at_milli_precision() {
        // FloatValue keeps 3 decimal places (x1000, truncating cast), so any factor that
        // is a whole multiple of 0.001 must come back out of the DOM unchanged.
        for v in [0.0_f32, 0.001, 0.5, 1.0, 2.5, 3.0, -1.5, 1000.0, 65536.0] {
            let got = content_flex_grow(&default_frame().with_flex_grow(v).dom())
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
        for v in [
            f32::EPSILON,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            1e-4,
            -1e-4,
            0.0005,
            -0.0009,
        ] {
            let got = content_flex_grow(&default_frame().with_flex_grow(v).dom())
                .expect("flex-grow must always be declared");
            assert_eq!(
                got.to_bits(),
                0_u32,
                "sub-milli flex_grow {v} did not truncate to +0.0 (got {got})",
            );
        }

        // ... and 0.001 is genuinely the smallest factor that still registers.
        let smallest = content_flex_grow(&default_frame().with_flex_grow(0.001).dom()).unwrap();
        assert!(smallest > 0.0, "0.001 is supposed to be the smallest representable factor");
    }

    #[test]
    fn a_nan_flex_grow_does_not_panic_and_lands_on_zero() {
        let dom = default_frame().with_flex_grow(f32::NAN).dom();
        let got = content_flex_grow(&dom).expect("flex-grow must always be declared");
        assert!(!got.is_nan(), "a NaN flex-grow reached the style tree");
        assert_eq!(got.to_bits(), 0_u32, "NaN must encode to +0.0 (saturating cast), got {got}");
    }

    #[test]
    fn infinite_and_maximal_flex_grow_saturates_instead_of_overflowing() {
        // `FloatValue::new` does `(v * 1000.0) as isize`; f32::MAX * 1000 already overflows
        // to +inf, so MAX and INFINITY have to land on the same saturated bound. The point
        // is that nothing wraps around into a factor of the opposite sign.
        let max_encoded = (isize::MAX as f32) / 1000.0;
        let min_encoded = (isize::MIN as f32) / 1000.0;

        for v in [f32::INFINITY, f32::MAX] {
            let got = content_flex_grow(&default_frame().with_flex_grow(v).dom()).unwrap();
            assert!(got.is_finite(), "an infinite flex-grow reached the style tree for {v}");
            assert_eq!(got, max_encoded, "{v} did not saturate at the isize upper bound");
            assert!(got > 0.0, "{v} wrapped around into a non-positive factor");
        }

        for v in [f32::NEG_INFINITY, f32::MIN] {
            let got = content_flex_grow(&default_frame().with_flex_grow(v).dom()).unwrap();
            assert!(got.is_finite(), "an infinite flex-grow reached the style tree for {v}");
            assert_eq!(got, min_encoded, "{v} did not saturate at the isize lower bound");
            assert!(got < 0.0, "{v} wrapped around into a non-negative factor");
        }
    }

    #[test]
    fn no_flex_grow_input_can_put_a_nan_or_an_infinity_into_the_style_tree() {
        for v in ADVERSARIAL_FLOATS {
            let dom = default_frame().with_flex_grow(v).dom();
            let got = content_flex_grow(&dom)
                .unwrap_or_else(|| panic!("flex-grow disappeared for input {v}"));
            assert!(
                got.is_finite(),
                "input {v} produced a non-finite flex-grow ({got}) — the layout solver would \
                 NaN out",
            );
        }
    }

    #[test]
    fn flex_grow_encoding_is_monotonic() {
        // A sign- or rounding-bug in the x1000 cast would show up as an inversion here.
        let ascending = [-1000.0_f32, -1.5, -0.001, 0.0, 0.001, 1.5, 1000.0];
        let mut prev = f32::NEG_INFINITY;
        for v in ascending {
            let got = content_flex_grow(&default_frame().with_flex_grow(v).dom()).unwrap();
            assert!(
                got >= prev,
                "encoding is not monotonic: {v} encoded to {got}, below the previous {prev}",
            );
            prev = got;
        }
    }

    #[test]
    fn the_callers_factor_only_ever_reaches_the_content_area() {
        // The two header rules carry a *fixed* flex-grow of 1 so they split the leftover
        // header width evenly; a caller's factor bleeding into them would collapse or
        // stretch the rules on either side of the title.
        for v in ADVERSARIAL_FLOATS {
            let dom = default_frame().with_flex_grow(v).dom();

            assert_eq!(count_flex_grow(&dom), 0, "the frame root declared a flex-grow");
            assert_eq!(count_flex_grow(header(&dom)), 0, "the header declared a flex-grow");
            assert_eq!(count_flex_grow(title_node(&dom)), 0, "the title declared a flex-grow");
            assert_eq!(flex_grow_of(rule_before(&dom)), Some(1.0), "left rule factor changed");
            assert_eq!(flex_grow_of(rule_after(&dom)), Some(1.0), "right rule factor changed");
            assert_eq!(
                count_flex_grow(content_area(&dom)),
                1,
                "the content area must declare flex-grow exactly once",
            );
        }
    }

    // ------------------------------------------------------------------
    // Frame::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_has_the_documented_shape() {
        let dom = frame("Title", Dom::create_text_do_not_use_without_block_level_wrapper("content")).dom();

        assert_eq!(kids(&dom).len(), 2, "a frame is a header plus a content area");
        assert_eq!(kids(header(&dom)).len(), 3, "the header is rule / title / rule");
        assert_eq!(kids(rule_before(&dom)).len(), 1, "the left rule wraps exactly one div");
        assert_eq!(kids(rule_after(&dom)).len(), 1, "the right rule wraps exactly one div");
        assert_eq!(
            kids(title_node(&dom)).len(),
            1,
            "the title is a <p> wrapping exactly one bare text node"
        );
        assert!(
            kids(&kids(title_node(&dom))[0]).is_empty(),
            "the text node itself must be a leaf"
        );
        assert_eq!(kids(content_area(&dom)).len(), 1, "the content area holds exactly one child");

        assert_eq!(text_of(title_node(&dom)), Some("Title"));
        assert_eq!(text_of(&kids(content_area(&dom))[0]), Some("content"));
        assert_eq!(dom.estimated_total_children, count_descendants(&dom));
    }

    #[test]
    fn an_empty_title_still_produces_a_text_node_between_the_two_rules() {
        // Dropping the node for an empty title would change the header's flex layout
        // (two rules instead of three items) and silently re-centre the rules.
        let dom = default_frame().dom();
        assert_eq!(kids(header(&dom)).len(), 3, "an empty title collapsed the header");
        assert_eq!(text_of(title_node(&dom)), Some(""), "the empty title node vanished");
    }

    #[test]
    fn dom_carries_every_class_the_stylesheet_selects_on() {
        let dom = frame("t", Dom::create_div()).dom();

        assert_eq!(classes(&dom), vec!["__azul-native-frame".to_string()]);
        assert_eq!(classes(header(&dom)), vec!["__azul-native-frame-header".to_string()]);
        assert_eq!(
            classes(rule_before(&dom)),
            vec!["__azul-native-frame-header-before".to_string()],
        );
        assert_eq!(
            classes(rule_after(&dom)),
            vec!["__azul-native-frame-header-after".to_string()],
        );
        assert_eq!(classes(content_area(&dom)), vec!["__azul-native-frame-content".to_string()]);

        // The title node and the two inner rule divs are addressed by element/descendant
        // selectors only — they carry no class of their own.
        assert!(classes(title_node(&dom)).is_empty(), "the title node grew a class");
        assert!(classes(&kids(rule_before(&dom))[0]).is_empty());
        assert!(classes(&kids(rule_after(&dom))[0]).is_empty());
    }

    #[test]
    fn dom_declares_exactly_the_build_time_constants_on_the_chrome_nodes() {
        let dom = frame("t", Dom::create_div()).dom();

        assert_eq!(inline_props(&dom), declared(CSS_MATCH_8602559445190067154.as_ref()));
        assert_eq!(inline_props(header(&dom)), declared(CSS_MATCH_16739370686243728873.as_ref()));
        assert_eq!(
            inline_props(rule_before(&dom)),
            declared(CSS_MATCH_15775557796860201720.as_ref()),
        );
        assert_eq!(
            inline_props(rule_after(&dom)),
            declared(CSS_MATCH_9156589477016488419.as_ref()),
        );
        assert_eq!(inline_props(title_node(&dom)), declared(CSS_MATCH_4236783900531286611.as_ref()));
    }

    #[test]
    fn the_content_area_declares_flex_grow_first_then_the_static_block_verbatim() {
        let dom = default_frame().with_flex_grow(2.5).dom();

        let mut expected = vec![CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
            LayoutFlexGrow {
                inner: FloatValue::new(2.5),
            },
        ))];
        expected.extend(declared(CSS_MATCH_CONTENT_AREA_PROPERTIES));

        assert_eq!(
            inline_props(content_area(&dom)),
            expected,
            "the content-area style block drifted from FlexGrow + the static constant",
        );
    }

    #[test]
    fn no_node_declares_the_same_property_twice() {
        // A duplicate declaration means one of the two is silently dead — and which one
        // wins depends on cascade order, so the widget would style differently per node.
        let dom = frame("t", Dom::create_div()).with_flex_grow(1.0).dom();
        for (i, node) in all_nodes(&dom).into_iter().enumerate() {
            let props = inline_props(node);
            let mut seen = HashSet::new();
            for p in &props {
                assert!(
                    seen.insert(p.get_type()),
                    "node {i} declares {:?} twice",
                    p.get_type(),
                );
            }
        }
    }

    #[test]
    fn every_declaration_is_unconditional() {
        // The frame chrome has no :hover/@media/@os variants — a stray condition would
        // make part of the border render only in one state.
        let dom = frame("t", Dom::create_div()).dom();
        for (i, node) in all_nodes(&dom).into_iter().enumerate() {
            for (p, conditions) in node.root.style.iter_inline_properties() {
                assert!(
                    conditions.as_ref().is_empty(),
                    "node {i} gates {:?} behind a dynamic selector",
                    p.get_type(),
                );
            }
        }
    }

    #[test]
    fn all_frame_lengths_are_absolute_px() {
        let dom = frame("t", Dom::create_div()).dom();
        for node in all_nodes(&dom) {
            for p in inline_props(node) {
                if let Some(pv) = length_of(&p) {
                    let v = px(&pv); // asserts SizeMetric::Px
                    assert!(
                        v.is_finite() && (0.0..=64.0).contains(&v),
                        "{:?} = {v}px is outside the plausible frame-chrome range",
                        p.get_type(),
                    );
                }
            }
        }
    }

    #[test]
    fn every_border_in_the_frame_uses_the_one_border_colour() {
        let dom = frame("t", Dom::create_div()).dom();
        let mut seen = 0_usize;
        for node in all_nodes(&dom) {
            for p in inline_props(node) {
                if let Some(c) = border_color_of(&p) {
                    assert_eq!(c, BORDER_COLOR, "{:?} uses an off-palette colour", p.get_type());
                    seen += 1;
                }
            }
        }
        // top+left on the left rule, top+right on the right rule, all four on the content
        // area.
        assert_eq!(seen, 8, "the number of coloured borders in the frame changed");
    }

    #[test]
    fn the_content_area_is_fully_boxed_in_on_all_four_sides() {
        // A missing side would leave the group box visibly open.
        let props = inline_props(content_area(&frame("t", Dom::create_div()).dom()));
        let widths = [
            CssPropertyType::BorderTopWidth,
            CssPropertyType::BorderBottomWidth,
            CssPropertyType::BorderLeftWidth,
            CssPropertyType::BorderRightWidth,
        ];
        for want in widths {
            let found = props
                .iter()
                .find(|p| p.get_type() == want)
                .unwrap_or_else(|| panic!("the content area is missing {want:?}"));
            assert_eq!(
                length_of(found).map(|pv| px(&pv)),
                Some(1.0),
                "{want:?} is not a 1px hairline",
            );
        }
    }

    #[test]
    fn the_two_header_rules_are_symmetric_apart_from_the_left_rule_carrying_a_width() {
        // Pinned as-is: `.__azul-native-frame-header-before` declares `width: 5px` while
        // `-after` declares no width at all, so the stub of rule left of the title is
        // capped at 5px while the right-hand rule is only bounded by flex-grow. This is
        // an asymmetry in the widget's style constants, not in the code under test —
        // recorded here so a deliberate change to either side shows up as a failure.
        let dom = frame("t", Dom::create_div()).dom();
        let before: Vec<CssPropertyType> =
            inline_props(rule_before(&dom)).iter().map(CssProperty::get_type).collect();
        let after: Vec<CssPropertyType> =
            inline_props(rule_after(&dom)).iter().map(CssProperty::get_type).collect();

        assert!(before.contains(&CssPropertyType::Width), "the left rule lost its width cap");
        assert!(
            !after.contains(&CssPropertyType::Width),
            "the right rule gained a width — the known before/after asymmetry was fixed, \
             update this test",
        );

        // Mirrored side: left draws a left border, right draws a right border.
        assert!(before.contains(&CssPropertyType::BorderLeftWidth));
        assert!(after.contains(&CssPropertyType::BorderRightWidth));

        // Everything else is shared, in the same order.
        for shared in [
            CssPropertyType::Height,
            CssPropertyType::BorderTopWidth,
            CssPropertyType::BorderTopStyle,
            CssPropertyType::BorderTopColor,
            CssPropertyType::MarginTop,
            CssPropertyType::FlexGrow,
            CssPropertyType::FlexDirection,
        ] {
            assert!(before.contains(&shared), "the left rule is missing {shared:?}");
            assert!(after.contains(&shared), "the right rule is missing {shared:?}");
        }
    }

    #[test]
    fn the_inner_rule_divs_carry_no_style_of_their_own() {
        // Pinned as-is: the constants comment their `... -header-before div` properties
        // (height + borders) separately from the container's, but both blocks are applied
        // to the *container* — the inner div is an unstyled, empty leaf. Recorded so that
        // re-splitting the two selectors shows up here instead of silently changing the
        // rendered header.
        let dom = frame("t", Dom::create_div()).dom();
        for inner in [&kids(rule_before(&dom))[0], &kids(rule_after(&dom))[0]] {
            assert!(
                inline_props(inner).is_empty(),
                "the inner rule div grew inline properties",
            );
            assert!(kids(inner).is_empty(), "the inner rule div grew children");
            assert_eq!(inner.root.get_node_type(), &NodeType::Div);
        }

        // ... and the height/border pair really does sit on the containers.
        for outer in [rule_before(&dom), rule_after(&dom)] {
            let types: Vec<CssPropertyType> =
                inline_props(outer).iter().map(CssProperty::get_type).collect();
            assert!(types.contains(&CssPropertyType::Height));
            assert!(types.contains(&CssPropertyType::BorderTopWidth));
        }
    }

    #[test]
    fn the_frame_is_inert_and_attaches_no_callbacks_or_stylesheets() {
        let dom = frame("t", Dom::create_div()).with_flex_grow(1.0).dom();
        for (i, node) in all_nodes(&dom).into_iter().enumerate() {
            assert!(
                node.root.callbacks.as_ref().is_empty(),
                "node {i} of a stateless frame carries a callback",
            );
            assert!(
                node.css.as_ref().is_empty(),
                "node {i} attached a whole stylesheet instead of inline props",
            );
        }
    }

    #[test]
    fn dom_moves_the_content_in_verbatim_including_its_own_classes_and_children() {
        let content = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(AzString::from(
                "user-content",
            ))]))
            .with_children(DomVec::from_vec(vec![
                Dom::create_text_do_not_use_without_block_level_wrapper("a"),
                Dom::create_text_do_not_use_without_block_level_wrapper("b"),
            ]));
        let expected = content.clone();

        let dom = frame("t", content).dom();
        let placed = &kids(content_area(&dom))[0];

        assert_eq!(*placed, expected, "the content DOM was rewritten on the way in");
        assert_eq!(classes(placed), vec!["user-content".to_string()]);
        assert_eq!(kids(placed).len(), 2);
    }

    #[test]
    fn dom_is_deterministic_for_equal_inputs() {
        for t in ADVERSARIAL_TEXT {
            for v in ADVERSARIAL_FLOATS {
                let a = frame(t, Dom::create_text_do_not_use_without_block_level_wrapper(t)).with_flex_grow(v).dom();
                let b = frame(t, Dom::create_text_do_not_use_without_block_level_wrapper(t)).with_flex_grow(v).dom();
                assert_eq!(a, b, "two identical frames rendered differently for {t:?} / {v}");
            }
        }
    }

    #[test]
    fn distinct_factors_that_encode_differently_produce_distinct_doms() {
        // The counterpart to the determinism test: the flex factor must actually reach
        // the tree, so a change in it has to be observable in the rendered DOM.
        let a = default_frame().with_flex_grow(1.0).dom();
        let b = default_frame().with_flex_grow(2.0).dom();
        assert_ne!(a, b, "the flex factor never reached the rendered DOM");

        // ... while two factors that collapse to the same encoding are indistinguishable.
        let nan = default_frame().with_flex_grow(f32::NAN).dom();
        let zero = default_frame().with_flex_grow(0.0).dom();
        assert_eq!(nan, zero, "NaN and 0.0 stopped encoding to the same style tree");
    }

    #[test]
    fn the_title_is_styled_the_same_no_matter_what_it_contains() {
        let baseline = declared(CSS_MATCH_4236783900531286611.as_ref());
        for t in ADVERSARIAL_TEXT {
            let dom = frame(t, Dom::create_div()).dom();
            assert_eq!(
                inline_props(title_node(&dom)),
                baseline,
                "the title style changed for input {t:?}",
            );
            assert_eq!(kids(header(&dom)).len(), 3, "the header shape changed for input {t:?}");
            assert_eq!(dom.estimated_total_children, 9, "the node count changed for {t:?}");
        }
    }
}
