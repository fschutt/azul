use azul_desktop::css::*;
use azul_desktop::css::AzString;
use azul_desktop::dom::{
    Dom, DomVec, IdOrClass, TabIndex, HoverEventFilter,
    IdOrClass::{Id, Class}, IdOrClassVec, CallbackData,
    EventFilter, NodeDataInlineCssProperty,
    NodeDataInlineCssPropertyVec,
};
use azul_desktop::callbacks::{
    Callback, CallbackInfo, Update, RefAny,
};

const STRING_16146701490593874959: AzString = AzString::from_const_str("sans-serif");
const STYLE_BACKGROUND_CONTENT_4878363956973295354_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 173, g: 216, b: 230, a: 255 })
];
const STYLE_BACKGROUND_CONTENT_4967804087795204988_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 250, g: 128, b: 114, a: 255 })
];
const STYLE_BACKGROUND_CONTENT_8568982142085024634_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 250, g: 235, b: 215, a: 255 })
];
const STYLE_BACKGROUND_CONTENT_12869309920691526943_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 240, g: 248, b: 255, a: 255 })
];
const STYLE_BACKGROUND_CONTENT_14573424550548235545_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 33, g: 114, b: 69, a: 255 })
];
const STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 255, g: 255, b: 255, a: 255 })
];
const STYLE_FONT_FAMILY_8122988506401935406_ITEMS: &[StyleFontFamily] = &[
    StyleFontFamily::System(STRING_16146701490593874959)
];

const CSS_MATCH_10111026547520801912_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-table-container .column-wrapper .line-numbers
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(25) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(14) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } })))
];
const CSS_MATCH_10111026547520801912: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_10111026547520801912_PROPERTIES);

const CSS_MATCH_10537637882082253178_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-formula-container .formula-commit .btn-2
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_12869309920691526943_ITEMS))))
];
const CSS_MATCH_10537637882082253178: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_10537637882082253178_PROPERTIES);

const CSS_MATCH_11184921220530473733_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-tabs div.after-tabs
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 213, g: 213, b: 213, a: 255 } })))
];
const CSS_MATCH_11184921220530473733: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_11184921220530473733_PROPERTIES);

const CSS_MATCH_11324334306954975636_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section.2
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(210) }))),
    // .__azul_native-ribbon-section
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 225, g: 225, b: 225, a: 255 } })))
];
const CSS_MATCH_11324334306954975636: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_11324334306954975636_PROPERTIES);

const CSS_MATCH_11749096093730352054_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-formula-container
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(3) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(3) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(10) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(10) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row)))
];
const CSS_MATCH_11749096093730352054: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_11749096093730352054_PROPERTIES);

const CSS_MATCH_11805228191975472988_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-formula-container .formula-entry
    NodeDataInlineCssProperty::Normal(CssProperty::MarginRight(LayoutMarginRightValue::Exact(LayoutMarginRight { inner: PixelValue::const_px(3) }))),
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
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } })))
];
const CSS_MATCH_11805228191975472988: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_11805228191975472988_PROPERTIES);

const CSS_MATCH_11894410514907408907_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section-content
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) })))
];
const CSS_MATCH_11894410514907408907: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_11894410514907408907_PROPERTIES);

const CSS_MATCH_12543025518776072814_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section-name
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(11) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 68, g: 68, b: 68, a: 255 } })))
];
const CSS_MATCH_12543025518776072814: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_12543025518776072814_PROPERTIES);

const CSS_MATCH_12657755885219626491_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-table-container .column-wrapper .line-numbers p
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(13) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 229, g: 229, b: 229, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center)))
];
const CSS_MATCH_12657755885219626491: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_12657755885219626491_PROPERTIES);

const CSS_MATCH_12860013474863056225_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section.1
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(135) }))),
    // .__azul_native-ribbon-section
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 225, g: 225, b: 225, a: 255 } })))
];
const CSS_MATCH_12860013474863056225: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_12860013474863056225_PROPERTIES);

const CSS_MATCH_14371786645818370801_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-tabs p.home
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(19) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(19) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 33, g: 114, b: 69, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 33, g: 114, b: 69, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 33, g: 114, b: 69, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 33, g: 114, b: 69, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_14573424550548235545_ITEMS)))),
    // .__azul_native-ribbon-tabs p
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(14) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(14) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(12) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 101, g: 101, b: 101, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 213, g: 213, b: 213, a: 255 } }))),
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
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center)))
];
const CSS_MATCH_14371786645818370801: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_14371786645818370801_PROPERTIES);

const CSS_MATCH_14675068197785310311_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-table-container
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS))))
];
const CSS_MATCH_14675068197785310311: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_14675068197785310311_PROPERTIES);

const CSS_MATCH_14701061083766788292_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-action-vertical-large .icon-wrapper
    NodeDataInlineCssProperty::Normal(CssProperty::JustifyContent(LayoutJustifyContentValue::Exact(LayoutJustifyContent::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignContent(LayoutAlignContentValue::Exact(LayoutAlignContent::Center)))
];
const CSS_MATCH_14701061083766788292: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_14701061083766788292_PROPERTIES);

const CSS_MATCH_14707506486468900090_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section-content
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) })))
];
const CSS_MATCH_14707506486468900090: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_14707506486468900090_PROPERTIES);

const CSS_MATCH_14738982339524920711_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section-content
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) })))
];
const CSS_MATCH_14738982339524920711: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_14738982339524920711_PROPERTIES);

const CSS_MATCH_15716718910432952660_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-action-vertical-large .icon-wrapper .icon
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(32) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(32) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_4878363956973295354_ITEMS))))
];
const CSS_MATCH_15716718910432952660: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_15716718910432952660_PROPERTIES);

const CSS_MATCH_15943161397910029460_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-formula-container .formula-commit .btn-1
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_4967804087795204988_ITEMS))))
];
const CSS_MATCH_15943161397910029460: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_15943161397910029460_PROPERTIES);

const CSS_MATCH_16851364358900804450_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section-name
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(11) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 68, g: 68, b: 68, a: 255 } })))
];
const CSS_MATCH_16851364358900804450: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_16851364358900804450_PROPERTIES);

const CSS_MATCH_17089226259487272686_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section.7
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(185) }))),
    // .__azul_native-ribbon-section
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 225, g: 225, b: 225, a: 255 } })))
];
const CSS_MATCH_17089226259487272686: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_17089226259487272686_PROPERTIES);

const CSS_MATCH_17283019665138187991_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-formula-container .formula-commit .btn-3
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(30) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_8568982142085024634_ITEMS))))
];
const CSS_MATCH_17283019665138187991: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_17283019665138187991_PROPERTIES);

const CSS_MATCH_17524132644355033702_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-tabs p.active
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 33, g: 114, b: 69, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::None)),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::None)),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::None)),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 213, g: 213, b: 213, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 213, g: 213, b: 213, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 213, g: 213, b: 213, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 213, g: 213, b: 213, a: 255 } }))),
    // .__azul_native-ribbon-tabs p
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(14) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(14) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(12) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 101, g: 101, b: 101, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 213, g: 213, b: 213, a: 255 } }))),
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
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center)))
];
const CSS_MATCH_17524132644355033702: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_17524132644355033702_PROPERTIES);

const CSS_MATCH_1934381104964361563_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-action-vertical-large .dropdown
    NodeDataInlineCssProperty::Normal(CssProperty::JustifyContent(LayoutJustifyContentValue::Exact(LayoutJustifyContent::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignContent(LayoutAlignContentValue::Exact(LayoutAlignContent::Center)))
];
const CSS_MATCH_1934381104964361563: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_1934381104964361563_PROPERTIES);

const CSS_MATCH_2161661208916302443_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-formula-container .formula-entry .dropdown-sm
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(10) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_12869309920691526943_ITEMS))))
];
const CSS_MATCH_2161661208916302443: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_2161661208916302443_PROPERTIES);

const CSS_MATCH_2233073185823558635_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section-name
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(11) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 68, g: 68, b: 68, a: 255 } })))
];
const CSS_MATCH_2233073185823558635: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_2233073185823558635_PROPERTIES);

const CSS_MATCH_2258738109329535793_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-tabs
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row))),
    NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(LayoutDisplay::Flex)))
];
const CSS_MATCH_2258738109329535793: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_2258738109329535793_PROPERTIES);

const CSS_MATCH_2310038472753606232_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-tabs p
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(14) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(14) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(12) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 101, g: 101, b: 101, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 213, g: 213, b: 213, a: 255 } }))),
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
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center)))
];
const CSS_MATCH_2310038472753606232: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_2310038472753606232_PROPERTIES);

const CSS_MATCH_3221151331850347044_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-body
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(90) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(12) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row))),
    NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(LayoutDisplay::Flex))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 213, g: 213, b: 213, a: 255 } })))
];
const CSS_MATCH_3221151331850347044: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_3221151331850347044_PROPERTIES);

const CSS_MATCH_3888401522023951407_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section.5
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(180) }))),
    // .__azul_native-ribbon-section
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 225, g: 225, b: 225, a: 255 } })))
];
const CSS_MATCH_3888401522023951407: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_3888401522023951407_PROPERTIES);

const CSS_MATCH_4060245836920688376_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section.6
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(135) }))),
    // .__azul_native-ribbon-section
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 225, g: 225, b: 225, a: 255 } })))
];
const CSS_MATCH_4060245836920688376: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_4060245836920688376_PROPERTIES);

const CSS_MATCH_4538658364223133674_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section-content
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) })))
];
const CSS_MATCH_4538658364223133674: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_4538658364223133674_PROPERTIES);

const CSS_MATCH_4856252049803891913_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section-name
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(11) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 68, g: 68, b: 68, a: 255 } })))
];
const CSS_MATCH_4856252049803891913: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_4856252049803891913_PROPERTIES);

const CSS_MATCH_489944609689083320_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-table-container .header-row .select-all
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(25) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } })))
];
const CSS_MATCH_489944609689083320: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_489944609689083320_PROPERTIES);

const CSS_MATCH_491594124841839797_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-action-vertical-large .dropdown .icon
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_4967804087795204988_ITEMS))))
];
const CSS_MATCH_491594124841839797: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_491594124841839797_PROPERTIES);

const CSS_MATCH_5884971763667172938_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-table-container .header-row p
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(65) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::JustifyContent(LayoutJustifyContentValue::Exact(LayoutJustifyContent::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(14) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 229, g: 229, b: 229, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } })))
];
const CSS_MATCH_5884971763667172938: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_5884971763667172938_PROPERTIES);

const CSS_MATCH_6328747057139953245_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section-name
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(11) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 68, g: 68, b: 68, a: 255 } })))
];
const CSS_MATCH_6328747057139953245: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_6328747057139953245_PROPERTIES);

const CSS_MATCH_6727848633830580264_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-table-container .header-row
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(20) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row)))
];
const CSS_MATCH_6727848633830580264: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_6727848633830580264_PROPERTIES);

const CSS_MATCH_6736299128913213977_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section.4
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(140) }))),
    // .__azul_native-ribbon-section
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 225, g: 225, b: 225, a: 255 } })))
];
const CSS_MATCH_6736299128913213977: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_6736299128913213977_PROPERTIES);

const CSS_MATCH_6737656294326280219_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-formula-container .formula-entry .formula-text
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(10) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(10) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::JustifyContent(LayoutJustifyContentValue::Exact(LayoutJustifyContent::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(13) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS))))
];
const CSS_MATCH_6737656294326280219: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_6737656294326280219_PROPERTIES);

const CSS_MATCH_6756514148882865175_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-action-vertical-large p
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center)))
];
const CSS_MATCH_6756514148882865175: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_6756514148882865175_PROPERTIES);

const CSS_MATCH_681808671153488983_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-formula-container .formula-dropdown
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(100) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(6) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(6) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(3) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(3) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::MarginRight(LayoutMarginRightValue::Exact(LayoutMarginRight { inner: PixelValue::const_px(30) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::JustifyContent(LayoutJustifyContentValue::Exact(LayoutJustifyContent::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(13) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 34, g: 34, b: 34, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS))))
];
const CSS_MATCH_681808671153488983: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_681808671153488983_PROPERTIES);

const CSS_MATCH_7952568575592251546_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-action-vertical-large
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(4) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(4) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(4) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(4) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Column))),
    NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(LayoutDisplay::Flex)))
];
const CSS_MATCH_7952568575592251546: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_7952568575592251546_PROPERTIES);

const CSS_MATCH_8539348830707080062_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .minixel-formula-container .formula-commit
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(110) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::MarginRight(LayoutMarginRightValue::Exact(LayoutMarginRight { inner: PixelValue::const_px(3) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 171, g: 171, b: 171, a: 255 } })))
];
const CSS_MATCH_8539348830707080062: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_8539348830707080062_PROPERTIES);

const CSS_MATCH_8561962837455305444_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section-content
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) })))
];
const CSS_MATCH_8561962837455305444: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_8561962837455305444_PROPERTIES);

const CSS_MATCH_8787113990689659847_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section-name
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(11) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 68, g: 68, b: 68, a: 255 } })))
];
const CSS_MATCH_8787113990689659847: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_8787113990689659847_PROPERTIES);

const CSS_MATCH_8808521992961481081_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section-name
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(11) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 68, g: 68, b: 68, a: 255 } })))
];
const CSS_MATCH_8808521992961481081: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_8808521992961481081_PROPERTIES);

const CSS_MATCH_9123706516995286623_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section-content
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) })))
];
const CSS_MATCH_9123706516995286623: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_9123706516995286623_PROPERTIES);

const CSS_MATCH_9206206203058145671_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section-content
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) })))
];
const CSS_MATCH_9206206203058145671: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_9206206203058145671_PROPERTIES);

const CSS_MATCH_970131228357345953_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-section.3
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(265) }))),
    // .__azul_native-ribbon-section
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 225, g: 225, b: 225, a: 255 } })))
];
const CSS_MATCH_970131228357345953: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_970131228357345953_PROPERTIES);

const CSS_MATCH_9926913261609802002_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-ribbon-tabs div.between-tabs
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(3) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 213, g: 213, b: 213, a: 255 } })))
];
const CSS_MATCH_9926913261609802002: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_9926913261609802002_PROPERTIES);

#[repr(C)]
pub struct Ribbon {
    pub tab_active: i32,
}

pub type RibbonOnTabClickedCallbackType = extern "C" fn(&mut RefAny, &mut CallbackInfo, i32) -> Update;
impl_callback!(RibbonOnTabClicked, OptionRibbonOnTabClicked, RibbonOnTabClickedCallback, RibbonOnTabClickedCallbackType);

impl Ribbon {
    pub fn dom(&self, callback: RibbonOnTabClickedCallback, data: RefAny) -> Dom {
        Dom::div()
        .with_ids_and_classes({
            const IDS_AND_CLASSES_9612282517634156717: &[IdOrClass] = &[
                    Class(AzString::from_const_str("__azul_native-ribbon-container")),

            ];
            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_9612282517634156717)
        })
        .with_children(DomVec::from_vec(vec![

            Dom::div()
            .with_inline_css_props(CSS_MATCH_2258738109329535793)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_9041457122899952067: &[IdOrClass] = &[
                            Class(AzString::from_const_str("__azul_native-ribbon-tabs")),

                ];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_9041457122899952067)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::text(AzString::from_const_str("FILE"))
                .with_inline_css_props(CSS_MATCH_14371786645818370801)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_4826288409200248071: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("home")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_4826288409200248071)
                }),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_9926913261609802002)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_9410866575549354381: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("between-tabs")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_9410866575549354381)
                }),
                render_tab_element("HOME", self.tab_active == 0, 0, callback, data.clone()),
                render_tab_element("INSERT", self.tab_active == 1, 1, callback, data.clone()),
                render_tab_element("PAGE LAYOUT", self.tab_active == 2, 2, callback, data.clone()),
                render_tab_element("FORMULAS", self.tab_active == 3, 3, callback, data.clone()),
                render_tab_element("DATA", self.tab_active == 4, 4, callback, data.clone()),
                render_tab_element("REVIEW", self.tab_active == 5, 5, callback, data.clone()),
                render_tab_element("VIEW", self.tab_active == 6, 6, callback, data.clone()),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_11184921220530473733)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_16912306910777040419: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("after-tabs")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_16912306910777040419)
                })
            ])),

            // tab content
            Dom::div()
            .with_inline_css_props(CSS_MATCH_3221151331850347044)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_2825694991725398553: &[IdOrClass] = &[
                            Class(AzString::from_const_str("__azul_native-ribbon-body")),

                ];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2825694991725398553)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::div()
                .with_inline_css_props(CSS_MATCH_12860013474863056225)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_10025392060247617630: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul_native-ribbon-section")),
                    Class(AzString::from_const_str("1")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_10025392060247617630)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_9123706516995286623)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_2004408468416758999: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-ribbon-section-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2004408468416758999)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::div()
                        .with_inline_css_props(CSS_MATCH_7952568575592251546)
                        .with_ids_and_classes({
                            const IDS_AND_CLASSES_6126546624613363847: &[IdOrClass] = &[
                                                    Class(AzString::from_const_str("__azul_native-ribbon-action-vertical-large")),

                            ];
                            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6126546624613363847)
                        })
                        .with_children(DomVec::from_vec(vec![
                            Dom::div()
                            .with_inline_css_props(CSS_MATCH_14701061083766788292)
                            .with_ids_and_classes({
                                const IDS_AND_CLASSES_4343297541786025485: &[IdOrClass] = &[
                                                            Class(AzString::from_const_str("icon-wrapper")),

                                ];
                                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_4343297541786025485)
                            })
                            .with_children(DomVec::from_vec(vec![
                                Dom::div()
                                .with_inline_css_props(CSS_MATCH_15716718910432952660)
                                .with_ids_and_classes({
                                    const IDS_AND_CLASSES_638783468819161744: &[IdOrClass] = &[
                                                                    Class(AzString::from_const_str("icon")),

                                    ];
                                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_638783468819161744)
                                })
                            ])),
                            Dom::text(AzString::from_const_str("Paste"))
                            .with_inline_css_props(CSS_MATCH_6756514148882865175),
                            Dom::div()
                            .with_inline_css_props(CSS_MATCH_1934381104964361563)
                            .with_ids_and_classes({
                                const IDS_AND_CLASSES_17000242124219500924: &[IdOrClass] = &[
                                                            Class(AzString::from_const_str("dropdown")),

                                ];
                                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_17000242124219500924)
                            })
                            .with_children(DomVec::from_vec(vec![
                                Dom::div()
                                .with_inline_css_props(CSS_MATCH_491594124841839797)
                                .with_ids_and_classes({
                                    const IDS_AND_CLASSES_638783468819161744: &[IdOrClass] = &[
                                                                    Class(AzString::from_const_str("icon")),

                                    ];
                                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_638783468819161744)
                                })
                            ]))
                        ]))
                    ])),
                    Dom::text(AzString::from_const_str("Clipboard"))
                    .with_inline_css_props(CSS_MATCH_2233073185823558635)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_6233255149722984275: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-ribbon-section-name")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6233255149722984275)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_11324334306954975636)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_16234433965518568113: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul_native-ribbon-section")),
                    Class(AzString::from_const_str("2")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_16234433965518568113)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_4538658364223133674)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_2004408468416758999: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-ribbon-section-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2004408468416758999)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str(""))
                    ])),
                    Dom::text(AzString::from_const_str("Font"))
                    .with_inline_css_props(CSS_MATCH_12543025518776072814)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_6233255149722984275: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-ribbon-section-name")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6233255149722984275)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_970131228357345953)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_8769206706192203364: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul_native-ribbon-section")),
                    Class(AzString::from_const_str("3")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_8769206706192203364)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_8561962837455305444)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_2004408468416758999: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-ribbon-section-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2004408468416758999)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str(""))
                    ])),
                    Dom::text(AzString::from_const_str("Alignment"))
                    .with_inline_css_props(CSS_MATCH_8808521992961481081)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_6233255149722984275: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-ribbon-section-name")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6233255149722984275)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_6736299128913213977)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_8980483043948686304: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul_native-ribbon-section")),
                    Class(AzString::from_const_str("4")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_8980483043948686304)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_9206206203058145671)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_2004408468416758999: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-ribbon-section-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2004408468416758999)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str(""))
                    ])),
                    Dom::text(AzString::from_const_str("Number"))
                    .with_inline_css_props(CSS_MATCH_16851364358900804450)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_6233255149722984275: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-ribbon-section-name")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6233255149722984275)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_3888401522023951407)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_6781594546968350058: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul_native-ribbon-section")),
                    Class(AzString::from_const_str("5")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6781594546968350058)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_14738982339524920711)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_2004408468416758999: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-ribbon-section-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2004408468416758999)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str(""))
                    ])),
                    Dom::text(AzString::from_const_str("Styles"))
                    .with_inline_css_props(CSS_MATCH_8787113990689659847)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_6233255149722984275: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-ribbon-section-name")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6233255149722984275)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_4060245836920688376)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_11618651107626783359: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul_native-ribbon-section")),
                    Class(AzString::from_const_str("6")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_11618651107626783359)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_11894410514907408907)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_2004408468416758999: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-ribbon-section-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2004408468416758999)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str(""))
                    ])),
                    Dom::text(AzString::from_const_str("Cells"))
                    .with_inline_css_props(CSS_MATCH_6328747057139953245)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_6233255149722984275: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-ribbon-section-name")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6233255149722984275)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_17089226259487272686)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_4188199152450384868: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul_native-ribbon-section")),
                    Class(AzString::from_const_str("7")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_4188199152450384868)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_14707506486468900090)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_2004408468416758999: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-ribbon-section-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2004408468416758999)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str(""))
                    ])),
                    Dom::text(AzString::from_const_str("Editing"))
                    .with_inline_css_props(CSS_MATCH_4856252049803891913)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_6233255149722984275: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul_native-ribbon-section-name")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6233255149722984275)
                    })
                ]))
            ]))
        ]))
    }
}

struct MyCustomStruct {
    which_tab_to_activate_on_click: i32,
    on_tab_change_callback: RibbonOnTabClickedCallback,
    on_tab_change_data: RefAny,
}

fn render_tab_element(
    text: &'static str,
    active: bool,
    which_tab_to_activate_on_click: i32,
    callback: RibbonOnTabClickedCallback,
    data: RefAny,
) -> Dom {
    Dom::text(AzString::from_const_str(text))
    .with_inline_css_props(if active { CSS_MATCH_17524132644355033702 } else { CSS_MATCH_2310038472753606232 })
    .with_callbacks(vec![
        CallbackData {
            event: EventFilter::Hover(HoverEventFilter::MouseUp), // onmouseup
            callback: Callback { cb: my_callback },
            data: RefAny::new(MyCustomStruct {
                which_tab_to_activate_on_click,
                on_tab_change_callback: callback,
                on_tab_change_data: data,
            }),
        }
    ].into())
}

extern "C" fn my_callback(
    data: &mut RefAny,
    info: &mut CallbackInfo
) -> Update {

    let mut data = match data.downcast_mut::<MyCustomStruct>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let which_tab_to_activate_on_click = data.which_tab_to_activate_on_click;

    (&mut data.on_tab_change_callback.cb)(&mut data.on_tab_change_data, info, which_tab_to_activate_on_click)
}