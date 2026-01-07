use std::vec::Vec;

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{
        Dom, IdOrClass,
        IdOrClass::Class,
        IdOrClassVec, NodeDataInlineCssProperty,
        NodeDataInlineCssProperty::{Active, Focus, Hover, Normal},
        NodeDataInlineCssPropertyVec, TabIndex,
    },
    refany::RefAny,
    resources::{ImageRef, OptionImageRef},
};
use azul_css::{
    props::{
        basic::{
            color::ColorU,
            font::{StyleFontFamily, StyleFontFamilyVec},
            *,
        },
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::callbacks::{Callback, CallbackInfo};

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

pub type ButtonOnClickCallbackType = extern "C" fn(RefAny, CallbackInfo) -> Update;
impl_widget_callback!(
    ButtonOnClick,
    OptionButtonOnClick,
    ButtonOnClickCallback,
    ButtonOnClickCallbackType
);

const SANS_SERIF_STR: &str = "sans-serif";
const SANS_SERIF: AzString = AzString::from_const_str(SANS_SERIF_STR);
const SANS_SERIF_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SANS_SERIF)];
const SANS_SERIF_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SANS_SERIF_FAMILIES);

// macOS: Helvetica with sans-serif fallback
const HELVETICA_STR: &str = "Helvetica Neue";
const HELVETICA: AzString = AzString::from_const_str(HELVETICA_STR);
const MAC_FONT_FAMILIES: &[StyleFontFamily] = &[
    StyleFontFamily::System(HELVETICA),
    StyleFontFamily::System(SANS_SERIF),
];
const MAC_FONT_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(MAC_FONT_FAMILIES);

const RGB_172: ColorU = ColorU {
    r: 172,
    g: 172,
    b: 172,
    a: 255,
};
const RGB_239: ColorU = ColorU {
    r: 239,
    g: 239,
    b: 239,
    a: 255,
};
const RGB_229: ColorU = ColorU {
    r: 229,
    g: 229,
    b: 229,
    a: 255,
};

const WINDOWS_HOVER_START: ColorU = ColorU {
    r: 234,
    g: 243,
    b: 252,
    a: 255,
};
const WINDOWS_HOVER_END: ColorU = ColorU {
    r: 126,
    g: 180,
    b: 234,
    a: 255,
};
const WINDOWS_HOVER_BORDER: ColorU = ColorU {
    r: 126,
    g: 180,
    b: 234,
    a: 255,
};

const WINDOWS_ACTIVE_START: ColorU = ColorU {
    r: 217,
    g: 235,
    b: 252,
    a: 255,
};
const WINDOWS_ACTIVE_END: ColorU = ColorU {
    r: 86,
    g: 157,
    b: 229,
    a: 255,
};
const WINDOWS_ACTIVE_BORDER: ColorU = ColorU {
    r: 86,
    g: 157,
    b: 229,
    a: 255,
};

const WINDOWS_FOCUS_BORDER: ColorU = ColorU {
    r: 51,
    g: 153,
    b: 255,
    a: 255,
};

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
// Temporarily use simple color for testing inline rendering
const BUTTON_NORMAL_BACKGROUND: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(RGB_229)];

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
const BUTTON_HOVER_BACKGROUND_WINDOWS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            BUTTON_HOVER_BACKGROUND_WINDOWS_COLOR_STOPS,
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
const BUTTON_ACTIVE_BACKGROUND_WINDOWS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            BUTTON_ACTIVE_BACKGROUND_WINDOWS_COLOR_STOPS,
        ),
    })];

static BUTTON_CONTAINER_WINDOWS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    Normal(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(BUTTON_NORMAL_BACKGROUND),
    )),
    Normal(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    Normal(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    Normal(CssProperty::const_cursor(StyleCursor::Pointer)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    //     border: 1px solid rgb(172, 172, 172);
    Normal(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )),
    Normal(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    Normal(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
    )),
    Normal(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(1),
    )),
    Normal(CssProperty::const_border_top_style(StyleBorderTopStyle {
        inner: BorderStyle::Solid,
    })),
    Normal(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )),
    Normal(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Solid,
    })),
    Normal(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    )),
    Normal(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: RGB_172,
    })),
    Normal(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: RGB_172 },
    )),
    Normal(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: RGB_172,
    })),
    Normal(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: RGB_172 },
    )),
    // padding: 5px
    Normal(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(5),
    )),
    Normal(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(5),
    )),
    Normal(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        3,
    ))),
    Normal(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(3),
    )),
    Hover(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(BUTTON_HOVER_BACKGROUND_WINDOWS),
    )),
    Hover(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: WINDOWS_HOVER_BORDER,
    })),
    Hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: WINDOWS_HOVER_BORDER,
        },
    )),
    Hover(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: WINDOWS_HOVER_BORDER,
    })),
    Hover(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: WINDOWS_HOVER_BORDER,
        },
    )),
    Active(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(BUTTON_ACTIVE_BACKGROUND_WINDOWS),
    )),
    Active(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: WINDOWS_ACTIVE_BORDER,
    })),
    Active(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: WINDOWS_ACTIVE_BORDER,
        },
    )),
    Active(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: WINDOWS_ACTIVE_BORDER,
    })),
    Active(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: WINDOWS_ACTIVE_BORDER,
        },
    )),
    Focus(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: WINDOWS_FOCUS_BORDER,
    })),
    Focus(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: WINDOWS_FOCUS_BORDER,
        },
    )),
    Focus(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: WINDOWS_FOCUS_BORDER,
    })),
    Focus(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: WINDOWS_FOCUS_BORDER,
        },
    )),
];

// Linux button background gradients
const LINUX_NORMAL_GRADIENT_STOPS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorU { r: 252, g: 252, b: 252, a: 255 },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorU { r: 239, g: 239, b: 239, a: 255 },
    },
];
const LINUX_NORMAL_BACKGROUND: &[StyleBackgroundContent] = &[StyleBackgroundContent::LinearGradient(LinearGradient {
    direction: Direction::FromTo(DirectionCorners {
        dir_from: DirectionCorner::Top,
        dir_to: DirectionCorner::Bottom,
    }),
    extend_mode: ExtendMode::Clamp,
    stops: NormalizedLinearColorStopVec::from_const_slice(LINUX_NORMAL_GRADIENT_STOPS),
})];

const LINUX_HOVER_GRADIENT_STOPS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorU { r: 255, g: 255, b: 255, a: 255 },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorU { r: 245, g: 245, b: 245, a: 255 },
    },
];
const LINUX_HOVER_BACKGROUND: &[StyleBackgroundContent] = &[StyleBackgroundContent::LinearGradient(LinearGradient {
    direction: Direction::FromTo(DirectionCorners {
        dir_from: DirectionCorner::Top,
        dir_to: DirectionCorner::Bottom,
    }),
    extend_mode: ExtendMode::Clamp,
    stops: NormalizedLinearColorStopVec::from_const_slice(LINUX_HOVER_GRADIENT_STOPS),
})];

const LINUX_ACTIVE_GRADIENT_STOPS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorU { r: 220, g: 220, b: 220, a: 255 },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorU { r: 200, g: 200, b: 200, a: 255 },
    },
];
const LINUX_ACTIVE_BACKGROUND: &[StyleBackgroundContent] = &[StyleBackgroundContent::LinearGradient(LinearGradient {
    direction: Direction::FromTo(DirectionCorners {
        dir_from: DirectionCorner::Top,
        dir_to: DirectionCorner::Bottom,
    }),
    extend_mode: ExtendMode::Clamp,
    stops: NormalizedLinearColorStopVec::from_const_slice(LINUX_ACTIVE_GRADIENT_STOPS),
})];

const LINUX_BORDER_COLOR: ColorU = ColorU { r: 183, g: 183, b: 183, a: 255 };

static BUTTON_CONTAINER_LINUX: &[NodeDataInlineCssProperty] = &[
    // Linux/GTK-style button styling
    Normal(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    Normal(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    Normal(CssProperty::const_justify_content(LayoutJustifyContent::Center)),
    Normal(CssProperty::const_cursor(StyleCursor::Pointer)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    // background: linear-gradient(#fcfcfc, #efefef)
    Normal(CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(LINUX_NORMAL_BACKGROUND))),
    // border: 1px solid #b7b7b7
    Normal(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(1))),
    Normal(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
    Normal(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(1))),
    Normal(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(1))),
    Normal(CssProperty::const_border_top_style(StyleBorderTopStyle { inner: BorderStyle::Solid })),
    Normal(CssProperty::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })),
    Normal(CssProperty::const_border_left_style(StyleBorderLeftStyle { inner: BorderStyle::Solid })),
    Normal(CssProperty::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid })),
    Normal(CssProperty::const_border_top_color(StyleBorderTopColor { inner: LINUX_BORDER_COLOR })),
    Normal(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: LINUX_BORDER_COLOR })),
    Normal(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: LINUX_BORDER_COLOR })),
    Normal(CssProperty::const_border_right_color(StyleBorderRightColor { inner: LINUX_BORDER_COLOR })),
    // border-radius: 4px
    Normal(CssProperty::const_border_top_left_radius(StyleBorderTopLeftRadius::const_px(4))),
    Normal(CssProperty::const_border_top_right_radius(StyleBorderTopRightRadius::const_px(4))),
    Normal(CssProperty::const_border_bottom_left_radius(StyleBorderBottomLeftRadius::const_px(4))),
    Normal(CssProperty::const_border_bottom_right_radius(StyleBorderBottomRightRadius::const_px(4))),
    // padding: 5px 10px
    Normal(CssProperty::const_padding_top(LayoutPaddingTop::const_px(5))),
    Normal(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(5))),
    Normal(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(10))),
    Normal(CssProperty::const_padding_right(LayoutPaddingRight::const_px(10))),
    // Hover state
    Hover(CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(LINUX_HOVER_BACKGROUND))),
    // Active state
    Active(CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(LINUX_ACTIVE_BACKGROUND))),
];

// macOS button background gradients
const MAC_NORMAL_GRADIENT_STOPS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorU { r: 252, g: 252, b: 252, a: 255 },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorU { r: 239, g: 239, b: 239, a: 255 },
    },
];
// Temporarily use simple color for testing inline rendering on macOS
const MAC_NORMAL_BACKGROUND: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(
    ColorU { r: 239, g: 239, b: 239, a: 255 }
)];

const MAC_HOVER_GRADIENT_STOPS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorU { r: 255, g: 255, b: 255, a: 255 },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorU { r: 245, g: 245, b: 245, a: 255 },
    },
];
const MAC_HOVER_BACKGROUND: &[StyleBackgroundContent] = &[StyleBackgroundContent::LinearGradient(LinearGradient {
    direction: Direction::FromTo(DirectionCorners {
        dir_from: DirectionCorner::Top,
        dir_to: DirectionCorner::Bottom,
    }),
    extend_mode: ExtendMode::Clamp,
    stops: NormalizedLinearColorStopVec::from_const_slice(MAC_HOVER_GRADIENT_STOPS),
})];

const MAC_ACTIVE_GRADIENT_STOPS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorU { r: 220, g: 220, b: 220, a: 255 },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorU { r: 200, g: 200, b: 200, a: 255 },
    },
];
const MAC_ACTIVE_BACKGROUND: &[StyleBackgroundContent] = &[StyleBackgroundContent::LinearGradient(LinearGradient {
    direction: Direction::FromTo(DirectionCorners {
        dir_from: DirectionCorner::Top,
        dir_to: DirectionCorner::Bottom,
    }),
    extend_mode: ExtendMode::Clamp,
    stops: NormalizedLinearColorStopVec::from_const_slice(MAC_ACTIVE_GRADIENT_STOPS),
})];

const MAC_BORDER_COLOR: ColorU = ColorU { r: 183, g: 183, b: 183, a: 255 };

static BUTTON_CONTAINER_MAC: &[NodeDataInlineCssProperty] = &[
    // macOS native button styling
    Normal(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    Normal(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    Normal(CssProperty::const_justify_content(LayoutJustifyContent::Center)),
    Normal(CssProperty::const_cursor(StyleCursor::Pointer)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    // background: linear-gradient(#fcfcfc, #efefef)
    Normal(CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(MAC_NORMAL_BACKGROUND))),
    // border: 1px solid #b7b7b7
    Normal(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(1))),
    Normal(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
    Normal(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(1))),
    Normal(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(1))),
    Normal(CssProperty::const_border_top_style(StyleBorderTopStyle { inner: BorderStyle::Solid })),
    Normal(CssProperty::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })),
    Normal(CssProperty::const_border_left_style(StyleBorderLeftStyle { inner: BorderStyle::Solid })),
    Normal(CssProperty::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid })),
    Normal(CssProperty::const_border_top_color(StyleBorderTopColor { inner: MAC_BORDER_COLOR })),
    Normal(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: MAC_BORDER_COLOR })),
    Normal(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: MAC_BORDER_COLOR })),
    Normal(CssProperty::const_border_right_color(StyleBorderRightColor { inner: MAC_BORDER_COLOR })),
    // border-radius: 4px
    Normal(CssProperty::const_border_top_left_radius(StyleBorderTopLeftRadius::const_px(4))),
    Normal(CssProperty::const_border_top_right_radius(StyleBorderTopRightRadius::const_px(4))),
    Normal(CssProperty::const_border_bottom_left_radius(StyleBorderBottomLeftRadius::const_px(4))),
    Normal(CssProperty::const_border_bottom_right_radius(StyleBorderBottomRightRadius::const_px(4))),
    // padding: 5px 10px
    Normal(CssProperty::const_padding_top(LayoutPaddingTop::const_px(5))),
    Normal(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(5))),
    Normal(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(10))),
    Normal(CssProperty::const_padding_right(LayoutPaddingRight::const_px(10))),
    // Hover state
    Hover(CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(MAC_HOVER_BACKGROUND))),
    // Active state
    Active(CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(MAC_ACTIVE_BACKGROUND))),
];

static BUTTON_CONTAINER_OTHER: &[NodeDataInlineCssProperty] = &[];

static BUTTON_LABEL_WINDOWS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    Normal(CssProperty::const_text_align(StyleTextAlign::Center)),
    Normal(CssProperty::const_text_color(StyleTextColor {
        inner: ColorU::BLACK,
    })),
    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

static BUTTON_LABEL_LINUX: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    Normal(CssProperty::const_text_align(StyleTextAlign::Center)),
    Normal(CssProperty::const_text_color(StyleTextColor {
        inner: ColorU { r: 76, g: 76, b: 76, a: 255 },
    })),
    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

static BUTTON_LABEL_MAC: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    Normal(CssProperty::const_text_align(StyleTextAlign::Center)),
    Normal(CssProperty::const_text_color(StyleTextColor {
        inner: ColorU { r: 76, g: 76, b: 76, a: 255 },
    })),
    Normal(CssProperty::const_font_family(MAC_FONT_FAMILY)),
];

static BUTTON_LABEL_OTHER: &[NodeDataInlineCssProperty] = &[];

impl Button {
    #[inline]
    pub fn create(label: AzString) -> Self {
        Self {
            label,
            image: None.into(),
            on_click: None.into(),

            #[cfg(target_os = "windows")]
            container_style: NodeDataInlineCssPropertyVec::from_const_slice(
                BUTTON_CONTAINER_WINDOWS,
            ),
            #[cfg(target_os = "linux")]
            container_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_CONTAINER_LINUX),
            #[cfg(target_os = "macos")]
            container_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_CONTAINER_MAC),
            #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
            container_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_CONTAINER_OTHER),

            #[cfg(target_os = "windows")]
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_WINDOWS),
            #[cfg(target_os = "linux")]
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_LINUX),
            #[cfg(target_os = "macos")]
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_MAC),
            #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_OTHER),

            #[cfg(target_os = "windows")]
            image_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_WINDOWS),
            #[cfg(target_os = "linux")]
            image_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_LINUX),
            #[cfg(target_os = "macos")]
            image_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_MAC),
            #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
            image_style: NodeDataInlineCssPropertyVec::from_const_slice(BUTTON_LABEL_OTHER),
        }
    }

    #[inline(always)]
    pub fn swap_with_default(&mut self) -> Self {
        let mut m = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut m, self);
        m
    }

    #[inline]
    pub fn set_image(&mut self, image: ImageRef) {
        self.image = Some(image).into();
    }

    #[inline]
    pub fn set_on_click<C: Into<ButtonOnClickCallback>>(&mut self, data: RefAny, on_click: C) {
        self.on_click = Some(ButtonOnClick {
            refany: data,
            callback: on_click.into(),
        })
        .into();
    }

    #[inline]
    pub fn with_on_click<C: Into<ButtonOnClickCallback>>(mut self, data: RefAny, on_click: C) -> Self {
        self.set_on_click(data, on_click);
        self
    }

    #[inline]
    pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{EventFilter, HoverEventFilter},
        };

        let callbacks = match self.on_click.into_option() {
            Some(ButtonOnClick { refany: data, callback }) => vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                callback: CoreCallback {
                    cb: callback.cb as usize,
                    ctx: callback.ctx,
                },
                refany: data,
            }],
            None => Vec::new(),
        };

        static CONTAINER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
            "__azul-native-button-container",
        ))];
        static LABEL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
            "__azul-native-button-content",
        ))];

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CONTAINER_CLASS))
            .with_inline_css_props(self.container_style)
            .with_callbacks(callbacks.into())
            .with_tab_index(TabIndex::Auto)
            .with_children(
                vec![Dom::create_text(self.label)
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(LABEL_CLASS))
                    .with_inline_css_props(self.label_style)]
                .into(),
            )
    }
}
