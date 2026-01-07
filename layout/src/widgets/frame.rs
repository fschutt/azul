use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
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

use crate::callbacks::Callback;

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
            inner: ColorU {
                r: 221,
                g: 221,
                b: 221,
                a: 255,
            },
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
            inner: ColorU {
                r: 221,
                g: 221,
                b: 221,
                a: 255,
            },
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
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
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
            inner: ColorU {
                r: 221,
                g: 221,
                b: 221,
                a: 255,
            },
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
            inner: ColorU {
                r: 221,
                g: 221,
                b: 221,
                a: 255,
            },
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

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Frame {
    pub title: AzString,
    pub flex_grow: f32,
    pub content: Dom,
}

impl Frame {
    pub fn create(title: AzString, content: Dom) -> Self {
        Self {
            title,
            content,
            flex_grow: 0.0,
        }
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""), Dom::create_div());
        core::mem::swap(&mut s, self);
        s
    }

    pub fn set_flex_grow(&mut self, flex_grow: f32) {
        self.flex_grow = flex_grow;
    }

    pub fn with_flex_grow(mut self, flex_grow: f32) -> Self {
        self.set_flex_grow(flex_grow);
        self
    }

    pub fn dom(self) -> Dom {
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
                        Dom::create_text(self.title).with_css_props(CSS_MATCH_4236783900531286611),
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
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                        // .__azul-native-frame .__azul-native-frame-content
                        CssPropertyWithConditions::simple(CssProperty::FlexGrow(
                            LayoutFlexGrowValue::Exact(LayoutFlexGrow::new(
                                self.flex_grow as isize,
                            )),
                        )),
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
                        CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
                            LayoutBorderTopWidthValue::None,
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
                            StyleBorderTopStyleValue::None,
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
                            StyleBorderTopColorValue::None,
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
                                inner: ColorU {
                                    r: 221,
                                    g: 221,
                                    b: 221,
                                    a: 255,
                                },
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
                            StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
                                inner: ColorU {
                                    r: 221,
                                    g: 221,
                                    b: 221,
                                    a: 255,
                                },
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
                            StyleBorderRightColorValue::Exact(StyleBorderRightColor {
                                inner: ColorU {
                                    r: 221,
                                    g: 221,
                                    b: 221,
                                    a: 255,
                                },
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
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
