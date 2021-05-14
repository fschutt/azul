use azul_core::{
    dom::{
        TabIndex, Dom, IdOrClass, IdOrClass::Class,
        NodeDataInlineCssProperty, NodeDataInlineCssProperty::{Normal, Active, Hover, Focus},
    },
    image::ImageRef,
    str::String as AzString,
    callbacks::{CallbackType, Callback, RefAny},
    vec::{
        IdOrClassVec, StyleFontFamilyVec,
        StyleBackgroundContentVec, NodeDataInlineCssPropertyVec,
        NormalizedLinearColorStopVec,
    },
};
use azul_css::*;
use alloc::vec::Vec;

pub type OnClickFn = CallbackType;

#[repr(C)]
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Button {
    /// Content (image or text) of this button, centered by default
    pub label: AzString,
    /// Optional image that is displayed next to the label
    pub image: OptionImageRef,
    /// Style for this button
    pub container_style: NodeDataInlineCssPropertyVec,
    pub label_style: NodeDataInlineCssPropertyVec,
    pub image_style: NodeDataInlineCssPropertyVec,
    /// Optional: Function to call when the button is clicked
    pub on_click: OptionButtonOnClick,
}

#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct ButtonOnClick {
    pub data: RefAny,
    pub callback: Callback,
}

impl_option!(ButtonOnClick, OptionButtonOnClick, [Debug, Clone, PartialEq, PartialOrd]);

const SANS_SERIF_STR: &str = "sans-serif";
const SANS_SERIF: AzString = AzString::from_const_str(SANS_SERIF_STR);
const SANS_SERIF_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SANS_SERIF)];
const SANS_SERIF_FAMILY: StyleFontFamilyVec = StyleFontFamilyVec::from_const_slice(SANS_SERIF_FAMILIES);

const RGB_172: ColorU = ColorU { r: 172, g: 172, b: 172, a: 255 };
const RGB_239: ColorU = ColorU { r: 239, g: 239, b: 239, a: 255 };
const RGB_229: ColorU = ColorU { r: 229, g: 229, b: 229, a: 255 };

const WINDOWS_HOVER_START: ColorU = ColorU { r: 234, g: 243, b: 252, a: 255 };
const WINDOWS_HOVER_END: ColorU = ColorU { r: 126, g: 180, b: 234, a: 255 };
const WINDOWS_HOVER_BORDER: ColorU = ColorU { r: 126, g: 180, b: 234, a: 255 };

const WINDOWS_ACTIVE_START: ColorU = ColorU { r: 217, g: 235, b: 252, a: 255 };
const WINDOWS_ACTIVE_END: ColorU = ColorU { r: 86, g: 157, b: 229, a: 255 };
const WINDOWS_ACTIVE_BORDER: ColorU = ColorU { r: 86, g: 157, b: 229, a: 255 };

const WINDOWS_FOCUS_BORDER: ColorU = ColorU { r: 51, g: 153, b: 255, a: 255 };

const BUTTON_NOMRAL_BACKGROUND_COLOR_STOPS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: RGB_239,
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: RGB_229,
    },
];
const BUTTON_NORMAL_BACKGROUND: &[StyleBackgroundContent] = &[
StyleBackgroundContent::LinearGradient(LinearGradient {
    direction: Direction::FromTo(DirectionCorners {
        from: DirectionCorner::Top,
        to: DirectionCorner::Bottom,
    }),
    extend_mode: ExtendMode::Clamp,
    stops: NormalizedLinearColorStopVec::from_const_slice(
        BUTTON_NOMRAL_BACKGROUND_COLOR_STOPS
    ),
})];

const BUTTON_HOVER_BACKGROUND_WINDOWS_COLOR_STOPS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: WINDOWS_HOVER_START,
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: WINDOWS_HOVER_END,
    },
];
const BUTTON_HOVER_BACKGROUND_WINDOWS: &[StyleBackgroundContent] = &[
StyleBackgroundContent::LinearGradient(LinearGradient {
    direction: Direction::FromTo(DirectionCorners {
        from: DirectionCorner::Top,
        to: DirectionCorner::Bottom,
    }),
    extend_mode: ExtendMode::Clamp,
    stops: NormalizedLinearColorStopVec::from_const_slice(
        BUTTON_HOVER_BACKGROUND_WINDOWS_COLOR_STOPS
    ),
})];
const BUTTON_ACTIVE_BACKGROUND_WINDOWS_COLOR_STOPS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: WINDOWS_ACTIVE_START,
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: WINDOWS_ACTIVE_END,
    },
];
const BUTTON_ACTIVE_BACKGROUND_WINDOWS: &[StyleBackgroundContent] = &[
StyleBackgroundContent::LinearGradient(LinearGradient {
    direction: Direction::FromTo(DirectionCorners {
        from: DirectionCorner::Top,
        to: DirectionCorner::Bottom,
    }),
    extend_mode: ExtendMode::Clamp,
    stops: NormalizedLinearColorStopVec::from_const_slice(
        BUTTON_ACTIVE_BACKGROUND_WINDOWS_COLOR_STOPS
    ),
})];

static BUTTON_CONTAINER_WINDOWS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::Flex)),
    Normal(CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(BUTTON_NORMAL_BACKGROUND))),
    Normal(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    Normal(CssProperty::const_justify_content(LayoutJustifyContent::Center)),
    Normal(CssProperty::const_cursor(StyleCursor::Pointer)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),

    //     border: 1px solid rgb(172, 172, 172);

    Normal(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(1))),
    Normal(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
    Normal(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(1))),
    Normal(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(1))),

    Normal(CssProperty::const_border_top_style(StyleBorderTopStyle { inner: BorderStyle::Solid })),
    Normal(CssProperty::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })),
    Normal(CssProperty::const_border_left_style(StyleBorderLeftStyle { inner: BorderStyle::Solid })),
    Normal(CssProperty::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid })),

    Normal(CssProperty::const_border_top_color(StyleBorderTopColor { inner: RGB_172 })),
    Normal(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: RGB_172 })),
    Normal(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: RGB_172 })),
    Normal(CssProperty::const_border_right_color(StyleBorderRightColor { inner: RGB_172 })),

    // padding: 5px

    Normal(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(5))),
    Normal(CssProperty::const_padding_right(LayoutPaddingRight::const_px(5))),
    Normal(CssProperty::const_padding_top(LayoutPaddingTop::const_px(5))),
    Normal(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(5))),

    Hover(CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(BUTTON_HOVER_BACKGROUND_WINDOWS))),
    Hover(CssProperty::const_border_top_color(StyleBorderTopColor { inner: WINDOWS_HOVER_BORDER })),
    Hover(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: WINDOWS_HOVER_BORDER })),
    Hover(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: WINDOWS_HOVER_BORDER })),
    Hover(CssProperty::const_border_right_color(StyleBorderRightColor { inner: WINDOWS_HOVER_BORDER })),

    Active(CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(BUTTON_ACTIVE_BACKGROUND_WINDOWS))),
    Active(CssProperty::const_border_top_color(StyleBorderTopColor { inner: WINDOWS_ACTIVE_BORDER })),
    Active(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: WINDOWS_ACTIVE_BORDER })),
    Active(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: WINDOWS_ACTIVE_BORDER })),
    Active(CssProperty::const_border_right_color(StyleBorderRightColor { inner: WINDOWS_ACTIVE_BORDER })),

    Focus(CssProperty::const_border_top_color(StyleBorderTopColor { inner: WINDOWS_FOCUS_BORDER })),
    Focus(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: WINDOWS_FOCUS_BORDER })),
    Focus(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: WINDOWS_FOCUS_BORDER })),
    Focus(CssProperty::const_border_right_color(StyleBorderRightColor { inner: WINDOWS_FOCUS_BORDER })),
];

static BUTTON_CONTAINER_LINUX: &[NodeDataInlineCssProperty] = &[
    /*
    .__azul-native-button {
        font-size: 13px;
        font-family: sans-serif;
        color: #4c4c4c;
        display: flex;
        flex-grow: 1;
        border: 1px solid #b7b7b7;
        border-radius: 4px;
        box-shadow: 0px 0px 3px #c5c5c5ad;
        background: linear-gradient(#fcfcfc, #efefef);
        text-align: center;
        flex-direction: column;
        justify-content: center;
        flex-grow: 1;
    }

    .__azul-native-button:hover {
        background: linear-gradient(red, black);
    }

    .__azul-native-button:active {
        background: linear-gradient(blue, green);
    }
    */
];

static BUTTON_CONTAINER_MAC: &[NodeDataInlineCssProperty] = &[
    /*
    .__azul-native-button {
        font-size: 12px;
        font-family: \"Helvetica\";
        color: #4c4c4c;
        background-color: #e7e7e7;
        border: 1px solid #b7b7b7;
        border-radius: 4px;
        box-shadow: 0px 0px 3px #c5c5c5ad;
        background: linear-gradient(#fcfcfc, #efefef);
        text-align: center;
        flex-direction: column;
        justify-content: center;
    }
    */
];

static BUTTON_CONTAINER_OTHER: &[NodeDataInlineCssProperty] = &[
];

static BUTTON_LABEL_WINDOWS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    Normal(CssProperty::const_text_align(StyleTextAlign::Center)),
    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

static BUTTON_LABEL_LINUX: &[NodeDataInlineCssProperty] = &[
];

static BUTTON_LABEL_MAC: &[NodeDataInlineCssProperty] = &[
];

static BUTTON_LABEL_OTHER: &[NodeDataInlineCssProperty] = &[
];


impl Button {
    #[inline]
    pub fn new(label: AzString) -> Self {
        Self {
            label: text.into(),

            image: None.into(),

            #[cfg(target_os = "windows")]
            container_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_CONTAINER_WINDOWS),
            #[cfg(target_os = "linux")]
            container_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_CONTAINER_LINUX),
            #[cfg(target_os = "macos")]
            container_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_CONTAINER_MAC),
            #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "mac")))]
            container_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_CONTAINER_OTHER),

            #[cfg(target_os = "windows")]
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_WINDOWS),
            #[cfg(target_os = "linux")]
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_LINUX),
            #[cfg(target_os = "macos")]
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_MAC),
            #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "mac")))]
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_OTHER),

            #[cfg(target_os = "windows")]
            image_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_WINDOWS),
            #[cfg(target_os = "linux")]
            image_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_LINUX),
            #[cfg(target_os = "macos")]
            image_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_MAC),
            #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "mac")))]
            image_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_OTHER),

            on_click: None,
        }
    }

    #[inline(always)]
    pub fn swap_with_default(&mut self) -> Self {
        let mut m = Self::new(AzString::from_const_str(""));
        core::mem::swap(&mut m, self);
        m
    }

    #[inline]
    pub fn set_image(&mut self, image: ImageRef) -> Self {
        self.image = Some(image).into();
    }

    #[inline]
    pub fn set_on_click(&mut self, data: RefAny, on_click: OnClickFn) {
        self.on_click = Some(ButtonOnClick {
            data,
            callback: Callback { cb: on_click },
        });
    }

    #[inline]
    pub fn dom(self) -> Dom {

        use self::ButtonContent::*;
        use azul::dom::{
            Dom, EventFilter, HoverEventFilter,
            CallbackData,
        };

        let callbacks = match self.on_click {
            Some((data, callback)) => vec![
                CallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    callback,
                    data,
                }
            ],
            None => Vec::new(),
        };

        static CONTAINER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-button-container"))];
        static LABEL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-button-content"))];

        Dom::div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CONTAINER_CLASS))
        .with_inline_css_props(self.container_style)
        .with_callbacks(callbacks.into())
        .with_tab_index(TabIndex::Auto)
        .with_children(vec![
            match self.content {
                Text(s) => {
                    Dom::text(s.into())
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(LABEL_CLASS))
                    .with_inline_css_props(self.label_style)
                },
                Image(i) => {
                    Dom::image(i)
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(LABEL_CLASS))
                    .with_inline_css_props(self.image_style)
                },
            }
        ].into())
    }
}

#[test]
fn test_button_ui_1() {
    let expected_html = "<div class=\"__azul-native-button\" tabindex=\"0\">\r\n    <p>\r\n        Hello\r\n    </p>\r\n</div>";

    let button = Button::label("Hello").dom();
    let button_html = button.get_html_string();

    if expected_html != button_html.as_str() {
        panic!("expected:\r\n{}\r\ngot:\r\n{}", expected_html, button_html);
    }
}
