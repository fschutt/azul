use azul_desktop::{
    dom::{
        Dom, NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec,
        NodeDataInlineCssProperty::{Normal, Focus}, IdOrClassVec,
        IdOrClass::Class, IdOrClass
    },
    css::*,
    css::AzString,
};

// usually the default size for the progress bar is 25x450px

const SANS_SERIF_STR: &str = "sans-serif";
const SANS_SERIF: AzString = AzString::from_const_str(SANS_SERIF_STR);
const SANS_SERIF_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SANS_SERIF)];
const SANS_SERIF_FAMILY: StyleFontFamilyVec = StyleFontFamilyVec::from_const_slice(SANS_SERIF_FAMILIES);

const COLOR_E6E6E6: ColorU = ColorU::new_rgb(230, 230, 230); // background color
const COLOR_BCBCBC: ColorU = ColorU::new_rgb(188, 188, 188); // border color
const COLOR_6FE075: ColorU = ColorU::new_rgb(111, 224, 117); // light green
const COLOR_06B025: ColorU = ColorU::new_rgb(6, 176, 37); // dark green

const PROGRESS_BAR_BACKGROUND: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(COLOR_E6E6E6)];

//    .__azul-native-progressbar-container {
//        flex-grow: 1;
//        border: 1px solid #BCBCBC;
//        background: #E6E6E6;
//        position: relative;
//        width: 100%;
//    }
static PROGRESS_BAR_CONTAINER_STYLE_WINDOWS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::Flex)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    Normal(CssProperty::background_content(StyleBackgroundContentVec::from_const_slice(PROGRESS_BAR_BACKGROUND))),

    //     border: 1px solid #BCBCBC;

    Normal(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(1))),
    Normal(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
    Normal(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(1))),
    Normal(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(1))),

    Normal(CssProperty::const_border_top_style(StyleBorderTopStyle { inner: BorderStyle::Solid })),
    Normal(CssProperty::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })),
    Normal(CssProperty::const_border_left_style(StyleBorderLeftStyle { inner: BorderStyle::Solid })),
    Normal(CssProperty::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid })),

    Normal(CssProperty::const_border_top_color(StyleBorderTopColor { inner: COLOR_BCBCBC })),
    Normal(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: COLOR_BCBCBC })),
    Normal(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: COLOR_BCBCBC })),
    Normal(CssProperty::const_border_right_color(StyleBorderRightColor { inner: COLOR_BCBCBC })),

    Normal(CssProperty::const_position(LayoutPosition::Relative)),
    Normal(CssProperty::const_width(LayoutWidth::const_percent(100))),
];

const PROGRESS_BAR_COLOR_STOPS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: COLOR_06B025,
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(75),
        color: COLOR_06B025,
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: COLOR_6FE075,
    },
];

const PROGRESS_BAR_BAR_WINDOWS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            from: DirectionCorner::Left,
            to: DirectionCorner::Right,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            PROGRESS_BAR_COLOR_STOPS
        ),
    })
];

// .__azul-native-progressbar-content {
//     flex-grow: 1;
//     background: linear-gradient(to right, #06B025, #06B025, #06B025, #6FE075);
// }
static PROGRESS_BAR_BAR_STYLE_WINDOWS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::Flex)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    Normal(CssProperty::background_content(StyleBackgroundContentVec::from_const_slice(PROGRESS_BAR_BAR_WINDOWS))),
];


// .__azul-native-progressbar-label {
//     flex-grow: 1;
//     position: absolute;
//     font-family: sans-serif;
//     font-size: 13px;
//     text-align: center;
//     justify-content: center;
// }
static PROGRESS_BAR_LABEL_STYLE_WINDOWS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::Flex)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    Normal(CssProperty::const_position(LayoutPosition::Absolute)),

    Normal(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    Normal(CssProperty::const_text_align(StyleTextAlign::Center)),
    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
    Normal(CssProperty::const_justify_content(LayoutJustifyContent::Center)),
];

#[derive(Debug, Clone)]
#[repr(C)]
pub struct ProgressBar {
    pub state: ProgressBarState,
    pub container_style: NodeDataInlineCssPropertyVec,
    pub bar_style: NodeDataInlineCssPropertyVec,
    pub label_style: NodeDataInlineCssPropertyVec,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct ProgressBarState {
    pub percent_done: f32,
    pub display_percentage: bool,
}

impl ProgressBar {

    #[inline]
    pub fn new(percent_done: f32) -> Self {
        Self {
            state: ProgressBarState { percent_done, display_percentage: false },
            container_style: NodeDataInlineCssPropertyVec::from_const_slice(PROGRESS_BAR_CONTAINER_STYLE_WINDOWS),
            bar_style: NodeDataInlineCssPropertyVec::from_const_slice(PROGRESS_BAR_BAR_STYLE_WINDOWS),
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(PROGRESS_BAR_LABEL_STYLE_WINDOWS),
        }
    }

    #[inline]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::new(0.0);
        core::mem::swap(&mut s, self);
        s
    }

    pub fn set_container_style(&mut self, style: NodeDataInlineCssPropertyVec) {
        self.container_style = style;
    }

    pub fn set_bar_style(&mut self, style: NodeDataInlineCssPropertyVec) {
        self.bar_style = style;
    }

    pub fn set_label_style(&mut self, style: NodeDataInlineCssPropertyVec) {
        self.label_style = style;
    }

    pub fn dom(mut self) -> Dom {
        use azul_core::callbacks::RefAny;

        let text = if self.state.display_percentage {
            Dom::text(AzString::from(format!("{}%", self.state.percent_done)))
        } else {
            Dom::text(AzString::from_const_str(""))
        };

        let mut bar_style = self.bar_style.into_library_owned_vec();
        bar_style.push(Normal(CssProperty::const_width(LayoutWidth::const_percent(self.state.percent_done as isize))));

        Dom::div()
        .with_dataset(Some(RefAny::new(self.state.clone())).into())
        .with_ids_and_classes(vec![Class("__azul-native-progressbar-container".into())].into())
        .with_inline_css_props(self.container_style)
        .with_children(vec![
            Dom::div()
            .with_ids_and_classes(vec![Class("__azul-native-progressbar-bar".into())].into())
            .with_inline_css_props(bar_style.into()),
            text
            .with_ids_and_classes(vec![Class("__azul-native-progressbar-label".into())].into())
            .with_inline_css_props(self.label_style)
        ].into())
    }
}