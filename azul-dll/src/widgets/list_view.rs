use alloc::vec::Vec;
use azul_desktop::css::*;
use azul_desktop::css::AzString;
use azul_desktop::dom::{
    Dom, IdOrClass, TabIndex,
    IdOrClass::{Id, Class},
    NodeDataInlineCssProperty,
    DomVec, IdOrClassVec, NodeDataInlineCssPropertyVec,
};

const STRING_16146701490593874959: AzString = AzString::from_const_str("sans-serif");
const STYLE_BACKGROUND_CONTENT_661302523448178568_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 209, g: 232, b: 255, a: 255 })
];
const STYLE_BACKGROUND_CONTENT_2444935983575427872_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 252, g: 252, b: 252, a: 255 })
];
const STYLE_BACKGROUND_CONTENT_3010057533077499049_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 229, g: 243, b: 251, a: 255 })
];
const STYLE_BACKGROUND_CONTENT_3839348353894170136_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 249, g: 250, b: 251, a: 255 })
];
const STYLE_BACKGROUND_CONTENT_6112684430356720596_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners { from: DirectionCorner::Top, to: DirectionCorner::Bottom }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(LINEAR_COLOR_STOP_10827796861537038040_ITEMS),
    })
];
const STYLE_BACKGROUND_CONTENT_7422581697888665934_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners { from: DirectionCorner::Top, to: DirectionCorner::Bottom }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(LINEAR_COLOR_STOP_513857305091467054_ITEMS),
    })
];
const STYLE_BACKGROUND_CONTENT_11062356617965867290_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 240, g: 240, b: 240, a: 255 })
];
const STYLE_BACKGROUND_CONTENT_11098930083828139815_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 184, g: 224, b: 243, a: 255 })
];
const STYLE_TRANSFORM_6162542744002865382_ITEMS: &[StyleTransform] = &[
    StyleTransform::Translate(StyleTransformTranslate2D { x: PixelValue::const_px(7), y: PixelValue::const_px(0) })
];
const STYLE_TRANSFORM_16978981723642914576_ITEMS: &[StyleTransform] = &[
    StyleTransform::Rotate(AngleValue::const_deg(45))
];
const STYLE_TRANSFORM_17732691695785266054_ITEMS: &[StyleTransform] = &[
    StyleTransform::Rotate(AngleValue::const_deg(315)),
    StyleTransform::Translate(StyleTransformTranslate2D { x: PixelValue::const_px(0), y: PixelValue::const_px(2) })
];
const STYLE_FONT_FAMILY_8122988506401935406_ITEMS: &[StyleFontFamily] = &[
    StyleFontFamily::System(STRING_16146701490593874959)
];
const LINEAR_COLOR_STOP_513857305091467054_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop { offset: PercentageValue::const_new(0), color: ColorU { r: 255, g: 255, b: 255, a: 255 } },
NormalizedLinearColorStop { offset: PercentageValue::const_new(50), color: ColorU { r: 255, g: 255, b: 255, a: 255 } },
NormalizedLinearColorStop { offset: PercentageValue::const_new(51), color: ColorU { r: 247, g: 248, b: 250, a: 255 } },
NormalizedLinearColorStop { offset: PercentageValue::const_new(100), color: ColorU { r: 243, g: 244, b: 246, a: 255 } }
];
const LINEAR_COLOR_STOP_10827796861537038040_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop { offset: PercentageValue::const_new(0), color: ColorU { r: 247, g: 252, b: 254, a: 255 } },
NormalizedLinearColorStop { offset: PercentageValue::const_new(50), color: ColorU { r: 247, g: 252, b: 254, a: 255 } },
NormalizedLinearColorStop { offset: PercentageValue::const_new(51), color: ColorU { r: 232, g: 246, b: 254, a: 255 } },
NormalizedLinearColorStop { offset: PercentageValue::const_new(100), color: ColorU { r: 206, g: 231, b: 244, a: 255 } }
];

const CSS_MATCH_1085706216385961159_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header-arrow-down
    NodeDataInlineCssProperty::Normal(CssProperty::Transform(StyleTransformVecValue::Exact(StyleTransformVec::from_const_slice(STYLE_TRANSFORM_6162542744002865382_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Absolute))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(3) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(3) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(3) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(3) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::JustifyContent(LayoutJustifyContentValue::Exact(LayoutJustifyContent::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row)))
];
const CSS_MATCH_1085706216385961159: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_1085706216385961159_PROPERTIES);

const CSS_MATCH_12498280255863106397_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header-item:hover
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 154, g: 223, b: 254, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_6112684430356720596_ITEMS)))),
    // .__azul_native-list-header-item:active
    NodeDataInlineCssProperty::Active(CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(StyleBoxShadow {
            offset: [PixelValueNoPercent { inner: PixelValue::const_px(0) }, PixelValueNoPercent { inner: PixelValue::const_px(0) }],
            color: ColorU { r: 206, g: 231, b: 244, a: 255 },
            blur_radius: PixelValueNoPercent { inner: PixelValue::const_px(5) },
            spread_radius: PixelValueNoPercent { inner: PixelValue::const_px(0) },
            clip_mode: BoxShadowClipMode::Inset,
        }))),
    NodeDataInlineCssProperty::Active(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(StyleBoxShadow {
            offset: [PixelValueNoPercent { inner: PixelValue::const_px(0) }, PixelValueNoPercent { inner: PixelValue::const_px(0) }],
            color: ColorU { r: 206, g: 231, b: 244, a: 255 },
            blur_radius: PixelValueNoPercent { inner: PixelValue::const_px(5) },
            spread_radius: PixelValueNoPercent { inner: PixelValue::const_px(0) },
            clip_mode: BoxShadowClipMode::Inset,
        }))),
    NodeDataInlineCssProperty::Active(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(StyleBoxShadow {
            offset: [PixelValueNoPercent { inner: PixelValue::const_px(0) }, PixelValueNoPercent { inner: PixelValue::const_px(0) }],
            color: ColorU { r: 206, g: 231, b: 244, a: 255 },
            blur_radius: PixelValueNoPercent { inner: PixelValue::const_px(5) },
            spread_radius: PixelValueNoPercent { inner: PixelValue::const_px(0) },
            clip_mode: BoxShadowClipMode::Inset,
        }))),
    NodeDataInlineCssProperty::Active(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(StyleBoxShadow {
            offset: [PixelValueNoPercent { inner: PixelValue::const_px(0) }, PixelValueNoPercent { inner: PixelValue::const_px(0) }],
            color: ColorU { r: 206, g: 231, b: 244, a: 255 },
            blur_radius: PixelValueNoPercent { inner: PixelValue::const_px(5) },
            spread_radius: PixelValueNoPercent { inner: PixelValue::const_px(0) },
            clip_mode: BoxShadowClipMode::Inset,
        }))),
    NodeDataInlineCssProperty::Active(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Active(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Active(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Active(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Active(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Active(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Active(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Active(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Active(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 194, g: 205, b: 219, a: 255 } }))),
    NodeDataInlineCssProperty::Active(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 194, g: 205, b: 219, a: 255 } }))),
    NodeDataInlineCssProperty::Active(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 194, g: 205, b: 219, a: 255 } }))),
    NodeDataInlineCssProperty::Active(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 194, g: 205, b: 219, a: 255 } }))),
    NodeDataInlineCssProperty::Active(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_3839348353894170136_ITEMS)))),
    // .__azul_native-list-header-item
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Relative))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(7) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::MinWidth(LayoutMinWidthValue::Exact(LayoutMinWidth { inner: PixelValue::const_px(100) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Column))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 243, g: 244, b: 246, a: 255 } })))
];
const CSS_MATCH_12498280255863106397: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_12498280255863106397_PROPERTIES);

const CSS_MATCH_12980082330151137475_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-rows-row-cell
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(7) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::MinWidth(LayoutMinWidthValue::Exact(LayoutMinWidth { inner: PixelValue::const_px(100) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(11) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS))))
];
const CSS_MATCH_12980082330151137475: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_12980082330151137475_PROPERTIES);

const CSS_MATCH_13758717721055992976_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header-arrow-down-inner
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(6) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Transform(StyleTransformVecValue::Exact(StyleTransformVec::from_const_slice(STYLE_TRANSFORM_16978981723642914576_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowY(LayoutOverflowValue::Exact(LayoutOverflow::Hidden))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowX(LayoutOverflowValue::Exact(LayoutOverflow::Hidden))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(6) })))
];
const CSS_MATCH_13758717721055992976: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_13758717721055992976_PROPERTIES);

const CSS_MATCH_15295293133676720691_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header-dragwidth-drag
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Absolute)))
];
const CSS_MATCH_15295293133676720691: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_15295293133676720691_PROPERTIES);

const CSS_MATCH_15315949193378715186_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(25) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_7422581697888665934_ITEMS))))
];
const CSS_MATCH_15315949193378715186: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_15315949193378715186_PROPERTIES);

const CSS_MATCH_15673486787900743642_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header .__azul_native-list-header-item p
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(11) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Column))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center)))
];
const CSS_MATCH_15673486787900743642: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_15673486787900743642_PROPERTIES);

const CSS_MATCH_1574792189506859253_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header-arrow-down-inner-deco
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(12) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Transform(StyleTransformVecValue::Exact(StyleTransformVec::from_const_slice(STYLE_TRANSFORM_17732691695785266054_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(12) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(StyleBoxShadow {
            offset: [PixelValueNoPercent { inner: PixelValue::const_px(3) }, PixelValueNoPercent { inner: PixelValue::const_px(3) }],
            color: ColorU { r: 60, g: 94, b: 114, a: 255 },
            blur_radius: PixelValueNoPercent { inner: PixelValue::const_px(10) },
            spread_radius: PixelValueNoPercent { inner: PixelValue::const_px(0) },
            clip_mode: BoxShadowClipMode::Inset,
        }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(StyleBoxShadow {
            offset: [PixelValueNoPercent { inner: PixelValue::const_px(3) }, PixelValueNoPercent { inner: PixelValue::const_px(3) }],
            color: ColorU { r: 60, g: 94, b: 114, a: 255 },
            blur_radius: PixelValueNoPercent { inner: PixelValue::const_px(10) },
            spread_radius: PixelValueNoPercent { inner: PixelValue::const_px(0) },
            clip_mode: BoxShadowClipMode::Inset,
        }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(StyleBoxShadow {
            offset: [PixelValueNoPercent { inner: PixelValue::const_px(3) }, PixelValueNoPercent { inner: PixelValue::const_px(3) }],
            color: ColorU { r: 60, g: 94, b: 114, a: 255 },
            blur_radius: PixelValueNoPercent { inner: PixelValue::const_px(10) },
            spread_radius: PixelValueNoPercent { inner: PixelValue::const_px(0) },
            clip_mode: BoxShadowClipMode::Inset,
        }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(StyleBoxShadow {
            offset: [PixelValueNoPercent { inner: PixelValue::const_px(3) }, PixelValueNoPercent { inner: PixelValue::const_px(3) }],
            color: ColorU { r: 60, g: 94, b: 114, a: 255 },
            blur_radius: PixelValueNoPercent { inner: PixelValue::const_px(10) },
            spread_radius: PixelValueNoPercent { inner: PixelValue::const_px(0) },
            clip_mode: BoxShadowClipMode::Inset,
        })))
];
const CSS_MATCH_1574792189506859253: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_1574792189506859253_PROPERTIES);

const CSS_MATCH_16730007321218551135_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-rows-row.focused
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 38, g: 160, b: 218, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 38, g: 160, b: 218, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 38, g: 160, b: 218, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 38, g: 160, b: 218, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_11098930083828139815_ITEMS)))),
    // .__azul_native-list-rows-row:hover
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 101, g: 181, b: 220, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 101, g: 181, b: 220, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 101, g: 181, b: 220, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 101, g: 181, b: 220, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_3010057533077499049_ITEMS)))),
    // .__azul_native-list-rows-row
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } })))
];
const CSS_MATCH_16730007321218551135: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_16730007321218551135_PROPERTIES);

const CSS_MATCH_17553577885456905601_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native_list-container
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 130, g: 135, b: 144, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 130, g: 135, b: 144, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 130, g: 135, b: 144, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 130, g: 135, b: 144, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_2444935983575427872_ITEMS))))
];
const CSS_MATCH_17553577885456905601: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_17553577885456905601_PROPERTIES);

const CSS_MATCH_2883986488332352590_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // body
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_11062356617965867290_ITEMS))))
];
const CSS_MATCH_2883986488332352590: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_2883986488332352590_PROPERTIES);

const CSS_MATCH_4852927511892172364_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-rows
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Column)))
];
const CSS_MATCH_4852927511892172364: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_4852927511892172364_PROPERTIES);

const CSS_MATCH_6002662151290653203_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header-dragwidth
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Relative))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) })))
];
const CSS_MATCH_6002662151290653203: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_6002662151290653203_PROPERTIES);

const CSS_MATCH_6827198030119836081_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-rows-row.selected
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 102, g: 167, b: 232, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 102, g: 167, b: 232, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 102, g: 167, b: 232, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 102, g: 167, b: 232, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_661302523448178568_ITEMS)))),
    // .__azul_native-list-rows-row:hover
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 101, g: 181, b: 220, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 101, g: 181, b: 220, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 101, g: 181, b: 220, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 101, g: 181, b: 220, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_3010057533077499049_ITEMS)))),
    // .__azul_native-list-rows-row
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } })))
];
const CSS_MATCH_6827198030119836081: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_6827198030119836081_PROPERTIES);

const CSS_MATCH_7894335449545988724_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-rows-row:hover
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 101, g: 181, b: 220, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 101, g: 181, b: 220, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 101, g: 181, b: 220, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 101, g: 181, b: 220, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_3010057533077499049_ITEMS)))),
    // .__azul_native-list-rows-row
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } })))
];
const CSS_MATCH_7894335449545988724: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_7894335449545988724_PROPERTIES);

const CSS_MATCH_7937682281721781688_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-rows-row-cell
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(7) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::MinWidth(LayoutMinWidthValue::Exact(LayoutMinWidth { inner: PixelValue::const_px(100) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(11) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS))))
];
const CSS_MATCH_7937682281721781688: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_7937682281721781688_PROPERTIES);

const CSS_MATCH_8793836789597026811_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-rows-row-cell
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(7) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::MinWidth(LayoutMinWidthValue::Exact(LayoutMinWidth { inner: PixelValue::const_px(100) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(11) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS))))
];
const CSS_MATCH_8793836789597026811: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_8793836789597026811_PROPERTIES);

#[repr(C)]
pub struct ListView {
    pub columns: StringVec,
    // pub content: IFrameCallback,
}

impl Default for ListView {
    fn default() -> Self {
        Self {
            columns: StringVec::from_const_slice(&[]),
        }
    }
}

#[repr(C)]
pub struct ListViewRow {
    pub cells: StringVec,
}

#[repr(C, u8)]
pub enum ListViewCellContent {
    StaticString(AzString),
    TextInput(AzString),
    ColorInput(ColorU),
    CheckBox(bool),
}

impl ListView {
    pub fn new(columns: StringVec) -> Self {
        ListView { columns }
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut m = Self::default();
        core::mem::swap(&mut m, self);
        m
    }

    pub fn dom(self) -> Dom {
        Dom::div()
        .with_inline_css_props(CSS_MATCH_17553577885456905601)
        .with_ids_and_classes({
            const IDS_AND_CLASSES_9205819539370539587: &[IdOrClass] = &[
                Class(AzString::from_const_str("__azul_native_list-container")),
            ];
            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_9205819539370539587)
        })
        .with_children(DomVec::from_vec(vec![
            Dom::div()
            .with_inline_css_props(CSS_MATCH_15315949193378715186)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_10742579426112804392: &[IdOrClass] = &[
                    Class(AzString::from_const_str("__azul_native-list-header")),
                ];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_10742579426112804392)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::div()
                .with_inline_css_props(CSS_MATCH_12498280255863106397)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_18330792117162403422: &[IdOrClass] = &[
                        Class(AzString::from_const_str("__azul_native-list-header-item")),
                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_18330792117162403422)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::text(AzString::from_const_str("Name"))
                    .with_inline_css_props(CSS_MATCH_15673486787900743642)
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_6002662151290653203)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_12994132510473144861: &[IdOrClass] = &[
                        Class(AzString::from_const_str("__azul_native-list-header-dragwidth")),
                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_12994132510473144861)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_15295293133676720691)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_8310527934462917253: &[IdOrClass] = &[
                            Class(AzString::from_const_str("__azul_native-list-header-dragwidth-drag")),
                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_8310527934462917253)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_12498280255863106397)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_18330792117162403422: &[IdOrClass] = &[
                        Class(AzString::from_const_str("__azul_native-list-header-item")),
                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_18330792117162403422)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_1085706216385961159)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3976305383432953403: &[IdOrClass] = &[
                            Class(AzString::from_const_str("__azul_native-list-header-arrow-down")),
                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3976305383432953403)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::div()
                        .with_inline_css_props(CSS_MATCH_13758717721055992976)
                        .with_ids_and_classes({
                            const IDS_AND_CLASSES_2783291116377660812: &[IdOrClass] = &[
                                Class(AzString::from_const_str("__azul_native-list-header-arrow-down-inner")),
                            ];
                            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2783291116377660812)
                        })
                        .with_children(DomVec::from_vec(vec![
                            Dom::div()
                            .with_inline_css_props(CSS_MATCH_1574792189506859253)
                            .with_ids_and_classes({
                                const IDS_AND_CLASSES_1647599740783303526: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul_native-list-header-arrow-down-inner-deco")),
                                ];
                                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1647599740783303526)
                            })
                        ]))
                    ])),
                    Dom::text(AzString::from_const_str("Breed"))
                    .with_inline_css_props(CSS_MATCH_15673486787900743642)
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_12498280255863106397)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_18330792117162403422: &[IdOrClass] = &[
                        Class(AzString::from_const_str("__azul_native-list-header-item")),
                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_18330792117162403422)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::text(AzString::from_const_str("Gender"))
                    .with_inline_css_props(CSS_MATCH_15673486787900743642)
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_12498280255863106397)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_18330792117162403422: &[IdOrClass] = &[
                        Class(AzString::from_const_str("__azul_native-list-header-item")),
                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_18330792117162403422)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::text(AzString::from_const_str("Price"))
                    .with_inline_css_props(CSS_MATCH_15673486787900743642)
                ]))
            ])),
            Dom::div()
            .with_inline_css_props(CSS_MATCH_4852927511892172364)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_6012478019077291002: &[IdOrClass] = &[
                    Class(AzString::from_const_str("__azul_native-list-rows")),
                ];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6012478019077291002)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::div()
                .with_inline_css_props(CSS_MATCH_7894335449545988724)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_790316832563530605: &[IdOrClass] = &[
                        Class(AzString::from_const_str("__azul_native-list-rows-row")),
                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_790316832563530605)
                })
                    .with_tab_index(TabIndex::Auto)
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_12980082330151137475)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),
                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_12980082330151137475)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),
                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_12980082330151137475)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_12980082330151137475)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ]))
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_6827198030119836081)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_12400284507819476404: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul_native-list-rows-row")),
                    Class(AzString::from_const_str("selected")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_12400284507819476404)
                })
                    .with_tab_index(TabIndex::Auto)
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_8793836789597026811)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("widgets.exe"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_8793836789597026811)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_8793836789597026811)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_8793836789597026811)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ]))
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_16730007321218551135)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_5514228343186538819: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul_native-list-rows-row")),
                    Class(AzString::from_const_str("focused")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_5514228343186538819)
                })
                    .with_tab_index(TabIndex::Auto)
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_7937682281721781688)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_7937682281721781688)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_7937682281721781688)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_7937682281721781688)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ]))
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_7894335449545988724)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_790316832563530605: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul_native-list-rows-row")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_790316832563530605)
                })
                    .with_tab_index(TabIndex::Auto)
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_12980082330151137475)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_12980082330151137475)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_12980082330151137475)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_12980082330151137475)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ]))
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_7894335449545988724)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_790316832563530605: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul_native-list-rows-row")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_790316832563530605)
                })
                    .with_tab_index(TabIndex::Auto)
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_12980082330151137475)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_12980082330151137475)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_12980082330151137475)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ])),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_12980082330151137475)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-list-rows-row-cell")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ]))
                ]))
            ]))
        ]))
    }
}
