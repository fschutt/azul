use azul_desktop::{
    css::*,
    css::AzString,
    dom::{
        TabIndex, Dom, IdOrClass, IdOrClass::Class, NodeDataInlineCssPropertyVec, IdOrClassVec,
        NodeDataInlineCssProperty, NodeDataInlineCssProperty::{Normal, Active, Hover, Focus},
    },
    resources::{ImageRef, OptionImageRef},
    callbacks::{RefAny, Update, CallbackInfo},
};
use std::vec::Vec;

#[repr(C)]
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Button {
    /// Content (image or text) of this button, centered by default
    pub label: AzString,
    /// Optional image that is displayed next to the label
    pub image: OptionImageRef,
    /// Style for this button container
    pub container_style: NodeDataInlineCssPropertyVec,
    /// Style of the label
    pub label_style: NodeDataInlineCssPropertyVec,
    /// Style of the image
    pub image_style: NodeDataInlineCssPropertyVec,
    /// Optional: Function to call when the button is clicked
    pub on_click: OptionButtonOnClick,
}

pub type ButtonOnClickCallbackType = extern "C" fn(&mut RefAny, &mut CallbackInfo) -> Update;
impl_callback!(ButtonOnClick, OptionButtonOnClick, ButtonOnClickCallback, ButtonOnClickCallbackType);

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
    Normal(CssProperty::const_display(LayoutDisplay::Block)),
    Normal(CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(BUTTON_NORMAL_BACKGROUND))),
    Normal(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    Normal(CssProperty::const_justify_content(LayoutJustifyContent::Center)),
    Normal(CssProperty::const_cursor(StyleCursor::Pointer)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),

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
    Normal(CssProperty::const_padding_top(LayoutPaddingTop::const_px(3))),
    Normal(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(3))),

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

            label,
            image: None.into(),
            on_click: None.into(),

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
        }
    }

    #[inline(always)]
    pub fn swap_with_default(&mut self) -> Self {
        let mut m = Self::new(AzString::from_const_str(""));
        core::mem::swap(&mut m, self);
        m
    }

    #[inline]
    pub fn set_image(&mut self, image: ImageRef) {
        self.image = Some(image).into();
    }

    #[inline]
    pub fn set_on_click(&mut self, data: RefAny, on_click: ButtonOnClickCallbackType) {
        self.on_click = Some(ButtonOnClick {
            data,
            callback: ButtonOnClickCallback { cb: on_click },
        }).into();
    }

    #[inline]
    pub fn with_on_click(mut self, data: RefAny, on_click: ButtonOnClickCallbackType) -> Self {
        self.set_on_click(data, on_click);
        self
    }

    #[inline]
    pub fn dom(self) -> Dom {

        use azul_desktop::dom::{
            EventFilter, HoverEventFilter,
            CallbackData,
        };
        use azul_desktop::callbacks::Callback;

        let callbacks = match self.on_click.into_option() {
            Some(ButtonOnClick { data, callback }) => vec![
                CallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    callback: Callback { cb: callback.cb },
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
            Dom::text(self.label)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(LABEL_CLASS))
            .with_inline_css_props(self.label_style)
        ].into())
    }
}

#[cfg(test)]
mod ui_test {

    static EXPECTED_1: &str = "
<div data-az-node-id=\"0\"  class=\"__azul-native-button-container\"  tabindex=\"0\"  style=\"background: linear-gradient(from top to bottom, 0%#efefefff, 100%#e5e5e5ff);cursor: pointer;border-top-color: #acacacff;border-left-color: #acacacff;border-right-color: #acacacff;border-bottom-color: #acacacff;border-top-style: solid;border-left-style: solid;border-right-style: solid;border-bottom-style: solid;display: flex;padding-top: 5px;padding-bottom: 5px;padding-left: 5px;padding-right: 5px;border-top-width: 1px;border-left-width: 1px;border-right-width: 1px;border-bottom-width: 1px;flex-direction: column;flex-grow: 1;justify-content: center;\">
    <p data-az-node-id=\"1\"  class=\"__azul-native-button-content\"  style=\"font-size: 13px;font-family: sans-serif;text-align: center;\">Hello</p>
</div>";

    #[test]
    fn test_button_ui_1() {

        use crate::widgets::button::Button;
        use azul_desktop::css::Css;

        let button = Button::new("Hello".into()).dom().style(&mut Css::empty());
        let button_html = button.get_html_string("", "", true);

        assert_lines(EXPECTED_1.trim(), button_html.as_str().trim());
    }

    // assert that two strings are the same, independent of line ending format
    fn assert_lines(a: &str, b: &str) {
        for (line_a, line_b) in a.lines().zip(b.lines()) {
            assert_eq!(line_a, line_b);
        }
    }
}
