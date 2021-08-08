#![windows_subsystem = "windows"]

use crate::logic::OperandStack;
use azul::prelude::FontRef;

macro_rules! FONT_PATH {() => { concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/assets/fonts/KoHo-Light.ttf")};}
static FONT: &[u8] = include_bytes!(FONT_PATH!());

pub struct Calculator {
    pub current_operator: Option<OperandStack>,
    pub current_operand_stack: OperandStack,
    pub division_by_zero: bool,
    pub expression: String,
    pub last_event: Option<Event>,
    pub font: FontRef,
}

impl Calculator {
    pub fn new(font: FontRef) -> Self {
        Self {
            current_operator: None,
            current_operand_stack: OperandStack::default(),
            division_by_zero: false,
            expression: String::new(),
            last_event: None,
            font,
        }
    }
    pub fn reset(&mut self) {
        self.current_operator = None;
        self.current_operand_stack = OperandStack::default();
        self.division_by_zero = false;
        self.expression = String::new();
        self.last_event = None;
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    Clear,
    InvertSign,
    Percent,
    Divide,
    Multiply,
    Subtract,
    Plus,
    EqualSign,
    Dot,
    Number(u8),
}

/// Handles UI rendering and callback definition
pub mod ui {

    use azul::prelude::*;
    use crate::{Event, Calculator};

    struct ButtonLocalDataset {
        backref: RefAny, // Ref<Calculator>,
        event: Event,
    }

    pub extern "C" fn layout(data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {

        let (result, expression, font) = match data.downcast_ref::<Calculator>() {
            Some(s) => {
                let r = match s.division_by_zero {
                    true => format!("Cannot divide by zero."),
                    false => s.current_operand_stack.get_display(),
                };
                (r, "", s.font.clone())
            },
            None => return StyledDom::default(),
        };

        use azul::css::*;
        use azul::str::String as AzString;
        use azul::vec::{
            DomVec, IdOrClassVec, NodeDataInlineCssPropertyVec,
            StyleBackgroundSizeVec, StyleBackgroundRepeatVec,
            StyleBackgroundContentVec, StyleTransformVec,
            StyleFontFamilyVec, StyleBackgroundPositionVec,
            NormalizedLinearColorStopVec, NormalizedRadialColorStopVec,
        };
        use azul::dom::{
            Dom, IdOrClass, TabIndex,
            IdOrClass::{Id, Class},
            NodeDataInlineCssProperty,
        };

        const STRING_16146701490593874959: AzString = AzString::from_const_str("sans-serif");
        const STYLE_BACKGROUND_CONTENT_4154864923475193136_ITEMS: &[StyleBackgroundContent] = &[
            StyleBackgroundContent::Color(ColorU { r: 214, g: 214, b: 214, a: 255 })
        ];
        const STYLE_BACKGROUND_CONTENT_7327435497123668670_ITEMS: &[StyleBackgroundContent] = &[
            StyleBackgroundContent::Color(ColorU { r: 68, g: 68, b: 68, a: 255 })
        ];
        const STYLE_BACKGROUND_CONTENT_9344791489195694459_ITEMS: &[StyleBackgroundContent] = &[
            StyleBackgroundContent::LinearGradient(LinearGradient {
                direction: Direction::FromTo(DirectionCorners { from: DirectionCorner::Top, to: DirectionCorner::Bottom }),
                extend_mode: ExtendMode::Clamp,
                stops: NormalizedLinearColorStopVec::from_const_slice(LINEAR_COLOR_STOP_8988125810324520145_ITEMS),
            })
        ];
        const STYLE_BACKGROUND_CONTENT_13274507731280044099_ITEMS: &[StyleBackgroundContent] = &[
            StyleBackgroundContent::LinearGradient(LinearGradient {
                direction: Direction::FromTo(DirectionCorners { from: DirectionCorner::Bottom, to: DirectionCorner::Top }),
                extend_mode: ExtendMode::Clamp,
                stops: NormalizedLinearColorStopVec::from_const_slice(LINEAR_COLOR_STOP_14496794322506994097_ITEMS),
            })
        ];
        const LINEAR_COLOR_STOP_8988125810324520145_ITEMS: &[NormalizedLinearColorStop] = &[
            NormalizedLinearColorStop { offset: PercentageValue::const_new(0), color: ColorU { r: 246, g: 145, b: 53, a: 255 } },
        NormalizedLinearColorStop { offset: PercentageValue::const_new(100), color: ColorU { r: 243, g: 115, b: 53, a: 255 } }
        ];
        const LINEAR_COLOR_STOP_14496794322506994097_ITEMS: &[NormalizedLinearColorStop] = &[
            NormalizedLinearColorStop { offset: PercentageValue::const_new(0), color: ColorU { r: 17, g: 17, b: 17, a: 255 } },
        NormalizedLinearColorStop { offset: PercentageValue::const_new(100), color: ColorU { r: 68, g: 68, b: 68, a: 255 } }
        ];

        let STYLE_FONT_FAMILY_12348921234331816595_ITEMS = StyleFontFamilyVec::from_vec(vec![
            StyleFontFamily::Ref(font.clone()),
            StyleFontFamily::System(STRING_16146701490593874959)
        ]);

        let CSS_MATCH_13227231438257162063 = NodeDataInlineCssPropertyVec::from_vec(vec![
            // .zero
            NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(2) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::None)),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::None)),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::None)),
            // .numpad-button
            NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
            NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(27) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(STYLE_FONT_FAMILY_12348921234331816595_ITEMS.clone()))),
            NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Column))),
            NodeDataInlineCssProperty::Normal(CssProperty::BoxSizing(LayoutBoxSizingValue::Exact(LayoutBoxSizing::BorderBox))),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 141, g: 141, b: 141, a: 255 } }))),
            NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center)))
        ]);

        let CSS_MATCH_15463971630940472146 = NodeDataInlineCssPropertyVec::from_vec(vec![
            // .result
            NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Right))),
            NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_pt(16) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::MaxHeight(LayoutMaxHeightValue::Exact(LayoutMaxHeight { inner: PixelValue::const_pt(81) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::JustifyContent(LayoutJustifyContentValue::Exact(LayoutJustifyContent::End))),
            NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(60) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(STYLE_FONT_FAMILY_12348921234331816595_ITEMS.clone()))),
            NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row))),
            NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
            NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_13274507731280044099_ITEMS))))
        ]);

        const CSS_MATCH_15575492078751046510_PROPERTIES: &[NodeDataInlineCssProperty] = &[
            // .row
            NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(78) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row))),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 141, g: 141, b: 141, a: 255 } })))
        ];
        const CSS_MATCH_15575492078751046510: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_15575492078751046510_PROPERTIES);

        let CSS_MATCH_17546825476105236973 = NodeDataInlineCssPropertyVec::from_vec(vec![
            // .orange:focus
            NodeDataInlineCssProperty::Focus(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(3) }))),
            NodeDataInlineCssProperty::Focus(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(3) }))),
            NodeDataInlineCssProperty::Focus(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(3) }))),
            NodeDataInlineCssProperty::Focus(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(3) }))),
            NodeDataInlineCssProperty::Focus(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
            NodeDataInlineCssProperty::Focus(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
            NodeDataInlineCssProperty::Focus(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
            NodeDataInlineCssProperty::Focus(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
            NodeDataInlineCssProperty::Focus(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 0, g: 0, b: 255, a: 255 } }))),
            NodeDataInlineCssProperty::Focus(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 0, g: 0, b: 255, a: 255 } }))),
            NodeDataInlineCssProperty::Focus(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 0, g: 0, b: 255, a: 255 } }))),
            NodeDataInlineCssProperty::Focus(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 0, g: 0, b: 255, a: 255 } }))),
            // .orange
            NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(98) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
            NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(27) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(STYLE_FONT_FAMILY_12348921234331816595_ITEMS.clone()))),
            NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Column))),
            NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 141, g: 141, b: 141, a: 255 } }))),
            NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_9344791489195694459_ITEMS)))),
            NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center)))
        ]);

        let CSS_MATCH_2138985759714704825 = NodeDataInlineCssPropertyVec::from_vec(vec![
            // .expression
            NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Right))),
            NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_pt(40) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::MaxHeight(LayoutMaxHeightValue::Exact(LayoutMaxHeight { inner: PixelValue::const_pt(50) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::JustifyContent(LayoutJustifyContentValue::Exact(LayoutJustifyContent::End))),
            NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(27) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(STYLE_FONT_FAMILY_12348921234331816595_ITEMS.clone()))),
            NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row))),
            NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
            NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_7327435497123668670_ITEMS))))
        ]);

        const CSS_MATCH_3485639429117624417_PROPERTIES: &[NodeDataInlineCssProperty] = &[
            // .numpad-container
            NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_4154864923475193136_ITEMS))))
        ];
        const CSS_MATCH_3485639429117624417: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_3485639429117624417_PROPERTIES);

        let CSS_MATCH_8712209192727909676 = NodeDataInlineCssPropertyVec::from_vec(vec![
            // .numpad-button
            NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
            NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(27) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(STYLE_FONT_FAMILY_12348921234331816595_ITEMS.clone()))),
            NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Column))),
            NodeDataInlineCssProperty::Normal(CssProperty::BoxSizing(LayoutBoxSizingValue::Exact(LayoutBoxSizing::BorderBox))),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
            NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 141, g: 141, b: 141, a: 255 } }))),
            NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center)))
        ]);

        Dom::body()
        .with_callback(EventFilter::Window(WindowEventFilter::TextInput), data.clone(), handle_text_input)
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), data.clone(), handle_virtual_key_input)
        .with_children(DomVec::from_vec(vec![
            Dom::div()
            .with_inline_css_props(CSS_MATCH_2138985759714704825)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_5369347371275349724: &[IdOrClass] = &[
                        Class(AzString::from_const_str("expression")),

                ];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_5369347371275349724)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::text(AzString::from_const_str("expression"))
            ])),
            Dom::div()
            .with_inline_css_props(CSS_MATCH_15463971630940472146)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_13535062482561510656: &[IdOrClass] = &[
                        Class(AzString::from_const_str("result")),

                ];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_13535062482561510656)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::text(result.into())
            ])),
            Dom::div()
            .with_inline_css_props(CSS_MATCH_3485639429117624417)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_11193070369341949283: &[IdOrClass] = &[
                        Class(AzString::from_const_str("numpad-container")),

                ];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_11193070369341949283)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::div()
                .with_inline_css_props(CSS_MATCH_15575492078751046510)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_6148463700465089087: &[IdOrClass] = &[
                                Class(AzString::from_const_str("row")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6148463700465089087)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::text(AzString::from_const_str("C"))
                    .with_inline_css_props(CSS_MATCH_8712209192727909676.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_1759473807768823455: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("numpad-button")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1759473807768823455)
                    }),
                    Dom::text(AzString::from_const_str("+/-"))
                    .with_inline_css_props(CSS_MATCH_8712209192727909676.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_1759473807768823455: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("numpad-button")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1759473807768823455)
                    }),
                    Dom::text(AzString::from_const_str("%"))
                    .with_inline_css_props(CSS_MATCH_8712209192727909676.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_1759473807768823455: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("numpad-button")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1759473807768823455)
                    }),
                    Dom::text(AzString::from_const_str("/"))
                    .with_inline_css_props(CSS_MATCH_17546825476105236973.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_8758059606014746022: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("orange")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_8758059606014746022)
                    })
                        .with_tab_index(TabIndex::Auto)
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_15575492078751046510)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_6148463700465089087: &[IdOrClass] = &[
                                Class(AzString::from_const_str("row")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6148463700465089087)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::text(AzString::from_const_str("7"))
                    .with_inline_css_props(CSS_MATCH_8712209192727909676.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_1759473807768823455: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("numpad-button")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1759473807768823455)
                    }),
                    Dom::text(AzString::from_const_str("8"))
                    .with_inline_css_props(CSS_MATCH_8712209192727909676.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_1759473807768823455: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("numpad-button")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1759473807768823455)
                    }),
                    Dom::text(AzString::from_const_str("9"))
                    .with_inline_css_props(CSS_MATCH_8712209192727909676.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_1759473807768823455: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("numpad-button")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1759473807768823455)
                    }),
                    Dom::text(AzString::from_const_str("x"))
                    .with_inline_css_props(CSS_MATCH_17546825476105236973.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_8758059606014746022: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("orange")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_8758059606014746022)
                    })
                        .with_tab_index(TabIndex::Auto)
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_15575492078751046510)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_6148463700465089087: &[IdOrClass] = &[
                                Class(AzString::from_const_str("row")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6148463700465089087)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::text(AzString::from_const_str("4"))
                    .with_inline_css_props(CSS_MATCH_8712209192727909676.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_1759473807768823455: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("numpad-button")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1759473807768823455)
                    }),
                    Dom::text(AzString::from_const_str("5"))
                    .with_inline_css_props(CSS_MATCH_8712209192727909676.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_1759473807768823455: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("numpad-button")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1759473807768823455)
                    }),
                    Dom::text(AzString::from_const_str("6"))
                    .with_inline_css_props(CSS_MATCH_8712209192727909676.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_1759473807768823455: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("numpad-button")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1759473807768823455)
                    }),
                    Dom::text(AzString::from_const_str("-"))
                    .with_inline_css_props(CSS_MATCH_17546825476105236973.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_8758059606014746022: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("orange")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_8758059606014746022)
                    })
                        .with_tab_index(TabIndex::Auto)
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_15575492078751046510)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_6148463700465089087: &[IdOrClass] = &[
                                Class(AzString::from_const_str("row")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6148463700465089087)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::text(AzString::from_const_str("1"))
                    .with_inline_css_props(CSS_MATCH_8712209192727909676.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_1759473807768823455: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("numpad-button")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1759473807768823455)
                    }),
                    Dom::text(AzString::from_const_str("2"))
                    .with_inline_css_props(CSS_MATCH_8712209192727909676.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_1759473807768823455: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("numpad-button")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1759473807768823455)
                    }),
                    Dom::text(AzString::from_const_str("3"))
                    .with_inline_css_props(CSS_MATCH_8712209192727909676.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_1759473807768823455: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("numpad-button")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1759473807768823455)
                    }),
                    Dom::text(AzString::from_const_str("+"))
                    .with_inline_css_props(CSS_MATCH_17546825476105236973.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_8758059606014746022: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("orange")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_8758059606014746022)
                    })
                        .with_tab_index(TabIndex::Auto)
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_15575492078751046510)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_6148463700465089087: &[IdOrClass] = &[
                                Class(AzString::from_const_str("row")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6148463700465089087)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::text(AzString::from_const_str("0"))
                    .with_inline_css_props(CSS_MATCH_13227231438257162063)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_8661706322006749636: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("numpad-button")),
                        Class(AzString::from_const_str("zero")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_8661706322006749636)
                    }),
                    Dom::text(AzString::from_const_str("."))
                    .with_inline_css_props(CSS_MATCH_8712209192727909676.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_1759473807768823455: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("numpad-button")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1759473807768823455)
                    }),
                    Dom::text(AzString::from_const_str("="))
                    .with_inline_css_props(CSS_MATCH_17546825476105236973.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_8758059606014746022: &[IdOrClass] = &[
                                        Class(AzString::from_const_str("orange")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_8758059606014746022)
                    })
                        .with_tab_index(TabIndex::Auto)
                ]))
            ])),
        ])).style(Css::empty())
    }

    extern "C" fn handle_mouseclick_numpad_btn(data: &mut RefAny, info:  &mut CallbackInfo) -> Update {

        let mut data = match data.downcast_mut::<ButtonLocalDataset>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };

        let event = data.event.clone();

        let mut calculator = match data.backref.downcast_mut::<Calculator>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };

        return calculator.process_event(event);
    }

    extern "C" fn handle_text_input(data: &mut RefAny, info:  &mut CallbackInfo) -> Update {
        let current_char: Option<char> = info
            .get_current_keyboard_state().current_char
            .into_option()
            .and_then(|u| char::from_u32(u));

        let event = match current_char {
            Some('0') => Event::Number(0),
            Some('1') => Event::Number(1),
            Some('2') => Event::Number(2),
            Some('3') => Event::Number(3),
            Some('4') => Event::Number(4),
            Some('5') => Event::Number(5),
            Some('6') => Event::Number(6),
            Some('7') => Event::Number(7),
            Some('8') => Event::Number(8),
            Some('9') => Event::Number(9),
            Some('*') => Event::Multiply,
            Some('-') => Event::Subtract,
            Some('+') => Event::Plus,
            Some('/') => Event::Divide,
            Some('%') => Event::Percent,
            Some('.') | Some(',') => Event::Dot,
            _ => return Update::DoNothing,
        };

        let mut calculator = match data.downcast_mut::<Calculator>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };

        return calculator.process_event(event);
    }

    extern "C" fn handle_virtual_key_input(data: &mut RefAny, info:  &mut CallbackInfo) -> Update {
        let mut event = match info.get_current_keyboard_state().current_virtual_keycode.into_option() {
            Some(VirtualKeyCode::Return) => Event::EqualSign,
            Some(VirtualKeyCode::Back) => Event::Clear,
            _ => return Update::DoNothing,
        };

        let mut calculator = match data.downcast_mut::<Calculator>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };

        return calculator.process_event(event);
    }
}

/// Handles the application logic
pub mod logic {

    use azul::prelude::Update;
    use crate::{Event, Calculator};

    #[derive(Debug, Clone, Default)]
    pub struct OperandStack {
        pub stack: Vec<Number>,
        pub negative_number: bool,
    }

    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub enum Number {
        Value(u8),
        Dot,
    }

    impl OperandStack {
        /// Returns the displayable string, i.e for:
        /// `[3, 4, Dot, 5]` => `"34.5"`
        pub fn get_display(&self) -> String {
            let mut display_string = String::new();

            if self.negative_number {
                display_string.push('-');
            }

            if self.stack.is_empty() {
                display_string.push('0');
            } else {
                // If we get a dot at the end of the stack, i.e. "35." - store it,
                // but don't display it
                let mut first_dot_found = false;
                for num in &self.stack {
                    match num {
                        Number::Value(v) => display_string.push((v + 48) as char),
                        Number::Dot => {
                            if !first_dot_found {
                                display_string.push('.');
                                first_dot_found = true;
                            }
                        }
                    }
                }
            }

            display_string
        }

        /// Returns the number which you can use to calculate things with
        pub fn get_number(&self) -> f32 {
            let stack_size = self.stack.len();
            if stack_size == 0 {
                return 0.0;
            }

            // Iterate the stack until the first Dot is found
            let first_dot_position = self.stack.iter()
                .position(|x| *x == Number::Dot)
                .and_then(|x| Some(x - 1))
                .unwrap_or(stack_size - 1) as i32;

            let mut final_number = 0.0;

            for (number_position, number) in self.stack.iter().filter_map(|x| match x {
                    Number::Dot => None,
                    Number::Value(v) => Some(v),
                })
                .enumerate()
            {
                // i.e. the 5 in 5432.1 has a distance of 3 to the first dot (meaning 3 zeros)
                let diff_to_first_dot = first_dot_position - number_position as i32;
                final_number += (*number as f32) * 10.0_f32.powi(diff_to_first_dot);
            }

            if self.negative_number {
                final_number = -final_number;
            }
            final_number
        }

        fn from_f32(value: f32) -> Self {
            let mut result = OperandStack::default();
            for c in value.to_string().chars() {
                if c == '-' {
                    result.negative_number = true;
                } else if c == '.' {
                    result.stack.push(Number::Dot);
                } else {
                    result.stack.push(Number::Value((c as u8 - 48) as u8))
                }
            }
            result
        }
    }

    impl Calculator {
        /// Act on the event accordingly
        pub fn process_event(&mut self, event: Event) -> Update {
            match event {
                Event::Clear => {
                    self.reset();
                    Update::RefreshDom
                }
                Event::InvertSign => {
                    if !self.division_by_zero {
                        self.current_operand_stack.negative_number = !self.current_operand_stack.negative_number;
                    }
                    Update::RefreshDom
                }
                Event::Percent => {

                    if self.division_by_zero {
                        return Update::DoNothing;
                    }

                    if let Some(operation) = &self.last_event.clone() {
                        if let Some(operand) = self.current_operator.clone() {
                            let num = self.current_operand_stack.get_number();
                            let op = operand.get_number();
                            let result = match operation {
                                Event::Plus | Event::Subtract => op / 100.0 * num,
                                Event::Multiply | Event::Divide => num / 100.0,
                                _ => unreachable!(),
                            };
                            self.current_operand_stack = OperandStack::from_f32(result);
                        }
                    }

                    Update::RefreshDom
                }
                Event::EqualSign => {

                    if self.division_by_zero {
                        return Update::DoNothing;
                    }

                    if let Some(Event::EqualSign) = self.last_event {
                        self.expression = format!("{} =", self.current_operand_stack.get_display());
                    } else {
                        self.expression.push_str(&format!("{} =", self.current_operand_stack.get_display()));
                        if let Some(operation) = &self.last_event.clone() {
                            if let Some(operand) = self.current_operator.clone() {
                                let num = self.current_operand_stack.get_number();
                                let op = operand.get_number();
                                match operation.perform_operation(op, num) {
                                    Some(r) => self.current_operand_stack = OperandStack::from_f32(r),
                                    None => self.division_by_zero = true,
                                }
                            }
                        }
                    }

                    self.current_operator = None;
                    self.last_event = Some(Event::EqualSign);

                    Update::RefreshDom
                }
                Event::Dot => {

                    if self.division_by_zero {
                        return Update::DoNothing;
                    }

                    if self.current_operand_stack.stack.iter().position(|x| *x == Number::Dot).is_none() {
                        if self.current_operand_stack.stack.len() == 0 {
                            self.current_operand_stack.stack.push(Number::Value(0));
                        }
                        self.current_operand_stack.stack.push(Number::Dot);
                    }

                    Update::RefreshDom
                }
                Event::Number(v) => {
                    if let Some(Event::EqualSign) = self.last_event {
                        self.reset();
                    }
                    self.current_operand_stack.stack.push(Number::Value(v));
                    Update::RefreshDom
                }
                operation => {

                    if self.division_by_zero {
                        return Update::DoNothing;
                    }

                    if let Some(Event::EqualSign) = self.last_event {
                        self.expression = String::new();
                    }

                    self.expression.push_str(&self.current_operand_stack.get_display());

                    if let Some(Event::EqualSign) = self.last_event {
                        self.current_operator = Some(self.current_operand_stack.clone());
                    } else if let Some(last_operation) = &self.last_event.clone() {
                        if let Some(operand) = self.current_operator.clone() {
                            let num = self.current_operand_stack.get_number();
                            let op = operand.get_number();
                            match last_operation.perform_operation(op, num) {
                                Some(r) => self.current_operator = Some(OperandStack::from_f32(r)),
                                None => self.division_by_zero = true,
                            }
                        }
                    } else {
                        self.current_operator = Some(self.current_operand_stack.clone());
                    }

                    self.current_operand_stack = OperandStack::default();
                    self.expression.push_str(match operation {
                        Event::Plus => " + ",
                        Event::Subtract => " - ",
                        Event::Multiply => " x ",
                        Event::Divide => " / ",
                        _ => unreachable!(),
                    });
                    self.last_event = Some(operation);

                    Update::RefreshDom
                }
            }
        }
    }

    impl Event {
        /// Performs an arithmetic operation.
        /// Returns None when trying to divide by zero.
        fn perform_operation(&self, left_operand: f32, right_operand: f32) -> Option<f32> {
            match self {
                Event::Multiply => Some(left_operand * right_operand),
                Event::Subtract => Some(left_operand - right_operand),
                Event::Plus => Some(left_operand + right_operand),
                Event::Divide => if right_operand == 0.0 { None } else { Some(left_operand / right_operand) },
                _ => None, // unreachable
            }
        }
    }
}

fn main() {

    use azul::{
        app::{App, AppConfig, LayoutSolver},
        css::Css,
        vec::U8Vec,
        style::StyledDom,
        option::OptionFontRef,
        font::{FontRef, FontSource},
        callbacks::{RefAny, LayoutCallbackInfo},
        window::{WindowCreateOptions, WindowFrame},
    };

    let font = match FontRef::parse(FontSource {
        data: U8Vec::from_const_slice(FONT),
        font_index: 0,
        parse_glyph_outlines: false,
    }) {
        OptionFontRef::Some(s) => s,
        OptionFontRef::None => return,
    };

    let data = RefAny::new(Calculator::new(font));
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    app.run(WindowCreateOptions::new(ui::layout));
}
