use alloc::vec::Vec;

use azul_css::*;

use crate::desktop::{
    css::AzString,
    dom::{
        Dom, DomVec, IdOrClass,
        IdOrClass::{Class, Id},
        IdOrClassVec, NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec, TabIndex,
    },
};

const STRING_16146701490593874959: AzString = AzString::from_const_str("sans-serif");
const STYLE_BACKGROUND_CONTENT_11062356617965867290_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ColorU {
        r: 240,
        g: 240,
        b: 240,
        a: 255,
    })];
const STYLE_FONT_FAMILY_8122988506401935406_ITEMS: &[StyleFontFamily] =
    &[StyleFontFamily::System(STRING_16146701490593874959)];

const CSS_MATCH_15775557796860201720_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-frame .__azul-native-frame-header .__azul-native-frame-header-before div
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight {
            inner: PixelValue::const_px(8),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 221,
                g: 221,
                b: 221,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 221,
                g: 221,
                b: 221,
                a: 255,
            },
        }),
    )),
    // .__azul-native-frame .__azul-native-frame-header .__azul-native-frame-header-before
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth {
        inner: PixelValue::const_px(5),
    }))),
    NodeDataInlineCssProperty::Normal(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
        LayoutMarginTop {
            inner: PixelValue::const_px(6),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
];
const CSS_MATCH_15775557796860201720: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_15775557796860201720_PROPERTIES);

const CSS_MATCH_16739370686243728873_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-frame .__azul-native-frame-header
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(
        StyleFontSize {
            inner: PixelValue::const_px(11),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
        StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
        LayoutAlignItems::FlexEnd,
    ))),
];
const CSS_MATCH_16739370686243728873: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_16739370686243728873_PROPERTIES);

const CSS_MATCH_4236783900531286611_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-frame .__azul-native-frame-header p
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(
        StyleTextAlign::Center,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(3),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(0),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(0),
        },
    ))),
];
const CSS_MATCH_4236783900531286611: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_4236783900531286611_PROPERTIES);

const CSS_MATCH_8602559445190067154_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-frame
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(3),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(3),
        },
    ))),
];
const CSS_MATCH_8602559445190067154: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_8602559445190067154_PROPERTIES);

const CSS_MATCH_9156589477016488419_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-frame .__azul-native-frame-header .__azul-native-frame-header-after div
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight {
            inner: PixelValue::const_px(8),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 221,
                g: 221,
                b: 221,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 221,
                g: 221,
                b: 221,
                a: 255,
            },
        }),
    )),
    // .__azul-native-frame .__azul-native-frame-header .__azul-native-frame-header-after
    NodeDataInlineCssProperty::Normal(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
        LayoutMarginTop {
            inner: PixelValue::const_px(6),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
];
const CSS_MATCH_9156589477016488419: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_9156589477016488419_PROPERTIES);

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Frame {
    pub title: AzString,
    pub flex_grow: f32,
    pub content: Dom,
}

impl Frame {
    pub fn new(title: AzString, content: Dom) -> Self {
        Self {
            title,
            content,
            flex_grow: 0.0,
        }
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::new(AzString::from_const_str(""), Dom::div());
        core::mem::swap(&mut s, self);
        s
    }

    pub fn set_flex_grow(&mut self, flex_grow: f32) {
        self.flex_grow = flex_grow;
    }

    pub fn dom(self) -> Dom {
        Dom::div()
            .with_inline_css_props(CSS_MATCH_8602559445190067154)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_14615537625743340639: &[IdOrClass] =
                    &[Class(AzString::from_const_str("__azul-native-frame"))];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_14615537625743340639)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::div()
                    .with_inline_css_props(CSS_MATCH_16739370686243728873)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_17776797146874875377: &[IdOrClass] = &[Class(
                            AzString::from_const_str("__azul-native-frame-header"),
                        )];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_17776797146874875377)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::div()
                            .with_inline_css_props(CSS_MATCH_15775557796860201720)
                            .with_ids_and_classes({
                                const IDS_AND_CLASSES_15264202958442287530: &[IdOrClass] =
                                    &[Class(AzString::from_const_str(
                                        "__azul-native-frame-header-before",
                                    ))];
                                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_15264202958442287530)
                            })
                            .with_children(DomVec::from_vec(vec![Dom::div()])),
                        Dom::text(self.title).with_inline_css_props(CSS_MATCH_4236783900531286611),
                        Dom::div()
                            .with_inline_css_props(CSS_MATCH_9156589477016488419)
                            .with_ids_and_classes({
                                const IDS_AND_CLASSES_5689091102265932280: &[IdOrClass] = &[Class(
                                    AzString::from_const_str("__azul-native-frame-header-after"),
                                )];
                                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_5689091102265932280)
                            })
                            .with_children(DomVec::from_vec(vec![Dom::div()])),
                    ])),
                Dom::div()
                    .with_inline_css_props(NodeDataInlineCssPropertyVec::from_vec(vec![
                        // .__azul-native-frame .__azul-native-frame-content
                        NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(
                            LayoutFlexGrowValue::Exact(LayoutFlexGrow::new(self.flex_grow)),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(
                            LayoutPaddingRightValue::Exact(LayoutPaddingRight {
                                inner: PixelValue::const_px(5),
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(
                            LayoutPaddingLeftValue::Exact(LayoutPaddingLeft {
                                inner: PixelValue::const_px(5),
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(
                            LayoutPaddingBottomValue::Exact(LayoutPaddingBottom {
                                inner: PixelValue::const_px(5),
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(
                            LayoutPaddingTopValue::Exact(LayoutPaddingTop {
                                inner: PixelValue::const_px(5),
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::FontSize(
                            StyleFontSizeValue::Exact(StyleFontSize {
                                inner: PixelValue::const_px(11),
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(
                            StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(
                                STYLE_FONT_FAMILY_8122988506401935406_ITEMS,
                            )),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(
                            LayoutBorderTopWidthValue::None,
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(
                            StyleBorderTopStyleValue::None,
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
                            StyleBorderTopColorValue::None,
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(
                            LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(
                            LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(
                            LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(
                            LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(
                            StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
                                inner: BorderStyle::Solid,
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(
                            StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
                                inner: BorderStyle::Solid,
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(
                            StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
                                inner: BorderStyle::Solid,
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(
                            StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
                                inner: BorderStyle::Solid,
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(
                            StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
                                inner: ColorU {
                                    r: 221,
                                    g: 221,
                                    b: 221,
                                    a: 255,
                                },
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
                            StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
                                inner: ColorU {
                                    r: 221,
                                    g: 221,
                                    b: 221,
                                    a: 255,
                                },
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
                            StyleBorderRightColorValue::Exact(StyleBorderRightColor {
                                inner: ColorU {
                                    r: 221,
                                    g: 221,
                                    b: 221,
                                    a: 255,
                                },
                            }),
                        )),
                        NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
                            StyleBorderTopColorValue::Exact(StyleBorderTopColor {
                                inner: ColorU {
                                    r: 221,
                                    g: 221,
                                    b: 221,
                                    a: 255,
                                },
                            }),
                        )),
                    ]))
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
