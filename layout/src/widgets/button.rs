use std::vec::Vec;

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, NodeType, TabIndex},
    refany::RefAny,
    resources::{ImageRef, OptionImageRef},
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{
            color::{ColorU, ColorOrSystem, SystemColorRef},
            font::{StyleFontFamily, StyleFontFamilyVec},
            *,
        },
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    system::SystemFontType,
    *,
};

use crate::callbacks::{Callback, CallbackInfo};

/// The semantic type/role of a button.
/// 
/// Each type has distinct styling to indicate its purpose to the user.
/// Colors are based on Bootstrap's button variants for familiarity.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ButtonType {
    /// Default button style - neutral/gray appearance
    #[default]
    Default,
    /// Primary action button - blue, uses system accent color on macOS
    Primary,
    /// Secondary button - gray, less prominent than primary
    Secondary,
    /// Success/confirmation button - green with white text
    Success,
    /// Danger/destructive button - red with white text
    Danger,
    /// Warning button - yellow with BLACK text
    Warning,
    /// Informational button - teal/cyan with white text
    Info,
    /// Link-style button - appears as a hyperlink, no background
    Link,
}

impl ButtonType {
    /// Get the CSS class name for this button type
    pub fn class_name(&self) -> &'static str {
        match self {
            ButtonType::Default => "__azul-btn-default",
            ButtonType::Primary => "__azul-btn-primary",
            ButtonType::Secondary => "__azul-btn-secondary",
            ButtonType::Success => "__azul-btn-success",
            ButtonType::Danger => "__azul-btn-danger",
            ButtonType::Warning => "__azul-btn-warning",
            ButtonType::Info => "__azul-btn-info",
            ButtonType::Link => "__azul-btn-link",
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Button {
    /// Content (image or text) of this button, centered by default
    pub label: AzString,
    /// Optional image that is displayed next to the label
    pub image: OptionImageRef,
    /// The semantic type of this button (Primary, Success, Danger, etc.)
    pub button_type: ButtonType,
    /// Style for this button container
    pub container_style: CssPropertyWithConditionsVec,
    /// Style of the label
    pub label_style: CssPropertyWithConditionsVec,
    /// Style of the image
    pub image_style: CssPropertyWithConditionsVec,
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
const MAC_FONT_FAMILY: StyleFontFamilyVec = StyleFontFamilyVec::from_const_slice(MAC_FONT_FAMILIES);

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
        color: ColorOrSystem::color(RGB_239),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(RGB_229),
    },
];
// Temporarily use simple color for testing inline rendering
const BUTTON_NORMAL_BACKGROUND: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(RGB_229)];

const BUTTON_HOVER_BACKGROUND_WINDOWS_COLOR_STOPS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(WINDOWS_HOVER_START),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(WINDOWS_HOVER_END),
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
        color: ColorOrSystem::color(WINDOWS_ACTIVE_START),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(WINDOWS_ACTIVE_END),
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

static BUTTON_CONTAINER_WINDOWS: &[CssPropertyWithConditions] = &[
    // Use InlineFlex so flex properties work correctly
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineFlex)),
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(BUTTON_NORMAL_BACKGROUND),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Row,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(
        LayoutAlignItems::Center,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    //     border: 1px solid rgb(172, 172, 172);
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: RGB_172,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: RGB_172 },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: RGB_172,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: RGB_172 },
    )),
    // padding: 5px
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(5),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(5),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        3,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(3),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(BUTTON_HOVER_BACKGROUND_WINDOWS),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: WINDOWS_HOVER_BORDER,
    })),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: WINDOWS_HOVER_BORDER,
        },
    )),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: WINDOWS_HOVER_BORDER,
        },
    )),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: WINDOWS_HOVER_BORDER,
        },
    )),
    CssPropertyWithConditions::on_active(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(BUTTON_ACTIVE_BACKGROUND_WINDOWS),
    )),
    CssPropertyWithConditions::on_active(CssProperty::const_border_top_color(
        StyleBorderTopColor {
            inner: WINDOWS_ACTIVE_BORDER,
        },
    )),
    CssPropertyWithConditions::on_active(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: WINDOWS_ACTIVE_BORDER,
        },
    )),
    CssPropertyWithConditions::on_active(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: WINDOWS_ACTIVE_BORDER,
        },
    )),
    CssPropertyWithConditions::on_active(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: WINDOWS_ACTIVE_BORDER,
        },
    )),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: WINDOWS_FOCUS_BORDER,
    })),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: WINDOWS_FOCUS_BORDER,
        },
    )),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: WINDOWS_FOCUS_BORDER,
        },
    )),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: WINDOWS_FOCUS_BORDER,
        },
    )),
];

// Linux button background gradients
const LINUX_NORMAL_GRADIENT_STOPS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(ColorU {
            r: 252,
            g: 252,
            b: 252,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 239,
            g: 239,
            b: 239,
            a: 255,
        }),
    },
];
const LINUX_NORMAL_BACKGROUND: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
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
        color: ColorOrSystem::color(ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 245,
            g: 245,
            b: 245,
            a: 255,
        }),
    },
];
const LINUX_HOVER_BACKGROUND: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
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
        color: ColorOrSystem::color(ColorU {
            r: 220,
            g: 220,
            b: 220,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 200,
            g: 200,
            b: 200,
            a: 255,
        }),
    },
];
const LINUX_ACTIVE_BACKGROUND: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(LINUX_ACTIVE_GRADIENT_STOPS),
    })];

const LINUX_BORDER_COLOR: ColorU = ColorU {
    r: 183,
    g: 183,
    b: 183,
    a: 255,
};

static BUTTON_CONTAINER_LINUX: &[CssPropertyWithConditions] = &[
    // Linux/GTK-style button styling - use InlineFlex so flex properties work
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineFlex)),
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Row,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(
        LayoutAlignItems::Center,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    // background: linear-gradient(#fcfcfc, #efefef)
    CssPropertyWithConditions::simple(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(LINUX_NORMAL_BACKGROUND),
    )),
    // border: 1px solid #b7b7b7
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: LINUX_BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: LINUX_BORDER_COLOR,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: LINUX_BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: LINUX_BORDER_COLOR,
        },
    )),
    // border-radius: 4px
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(4),
    )),
    // padding: 5px 10px
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        5,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(5),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(10),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(10),
    )),
    // Hover state
    CssPropertyWithConditions::on_hover(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(LINUX_HOVER_BACKGROUND),
    )),
    // Active state
    CssPropertyWithConditions::on_active(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(LINUX_ACTIVE_BACKGROUND),
    )),
];

// macOS Big Sur+ native button styling
// Normal state: light gray background with subtle shadow
const MAC_NORMAL_BACKGROUND: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
})];

// Hover state: slightly brighter
const MAC_HOVER_BACKGROUND: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(ColorU {
    r: 250,
    g: 250,
    b: 250,
    a: 255,
})];

// Active/pressed state: darker gray
const MAC_ACTIVE_BACKGROUND: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(ColorU {
    r: 220,
    g: 220,
    b: 220,
    a: 255,
})];

// macOS uses a subtle gray border
const MAC_BORDER_COLOR: ColorU = ColorU {
    r: 200,
    g: 200,
    b: 200,
    a: 255,
};

// macOS box shadow for depth: 0 1px 1px rgba(0,0,0,0.06)
const MAC_BOX_SHADOW: &[StyleBoxShadow] = &[StyleBoxShadow {
    offset_x: PixelValueNoPercent { inner: PixelValue::const_px(0) },
    offset_y: PixelValueNoPercent { inner: PixelValue::const_px(1) },
    color: ColorU { r: 0, g: 0, b: 0, a: 15 },
    blur_radius: PixelValueNoPercent { inner: PixelValue::const_px(1) },
    spread_radius: PixelValueNoPercent { inner: PixelValue::const_px(0) },
    clip_mode: BoxShadowClipMode::Outset,
}];

static BUTTON_CONTAINER_MAC: &[CssPropertyWithConditions] = &[
    // macOS native button styling - use InlineFlex so flex properties work
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineFlex)),
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Row,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(
        LayoutAlignItems::Center,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    // background: linear-gradient(#fcfcfc, #efefef)
    CssPropertyWithConditions::simple(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(MAC_NORMAL_BACKGROUND),
    )),
    // border: 1px solid #b7b7b7
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: MAC_BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: MAC_BORDER_COLOR,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: MAC_BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: MAC_BORDER_COLOR,
        },
    )),
    // border-radius: 4px
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(4),
    )),
    // padding: 5px 10px
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        5,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(5),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(10),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(10),
    )),
    // Hover state
    CssPropertyWithConditions::on_hover(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(MAC_HOVER_BACKGROUND),
    )),
    // Active state
    CssPropertyWithConditions::on_active(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(MAC_ACTIVE_BACKGROUND),
    )),
];

static BUTTON_CONTAINER_OTHER: &[CssPropertyWithConditions] = &[];

static BUTTON_LABEL_WINDOWS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: ColorU::BLACK,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

static BUTTON_LABEL_LINUX: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: ColorU {
            r: 76,
            g: 76,
            b: 76,
            a: 255,
        },
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

static BUTTON_LABEL_MAC: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: ColorU {
            r: 76,
            g: 76,
            b: 76,
            a: 255,
        },
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(MAC_FONT_FAMILY)),
];

static BUTTON_LABEL_OTHER: &[CssPropertyWithConditions] = &[];

// ============================================================
// ButtonType-specific styling
// ============================================================

/// Get the background color for a button type
fn get_button_colors(button_type: ButtonType) -> (ColorU, ColorU, ColorU) {
    // Returns (normal, hover, active) colors
    match button_type {
        ButtonType::Default => (
            ColorU::rgb(248, 249, 250), // Light gray
            ColorU::rgb(233, 236, 239), // Darker gray on hover
            ColorU::rgb(218, 222, 226), // Even darker on active
        ),
        ButtonType::Primary => (
            ColorU::bootstrap_primary(),
            ColorU::bootstrap_primary_hover(),
            ColorU::bootstrap_primary_active(),
        ),
        ButtonType::Secondary => (
            ColorU::bootstrap_secondary(),
            ColorU::bootstrap_secondary_hover(),
            ColorU::bootstrap_secondary_active(),
        ),
        ButtonType::Success => (
            ColorU::bootstrap_success(),
            ColorU::bootstrap_success_hover(),
            ColorU::bootstrap_success_active(),
        ),
        ButtonType::Danger => (
            ColorU::bootstrap_danger(),
            ColorU::bootstrap_danger_hover(),
            ColorU::bootstrap_danger_active(),
        ),
        ButtonType::Warning => (
            ColorU::bootstrap_warning(),
            ColorU::bootstrap_warning_hover(),
            ColorU::bootstrap_warning_active(),
        ),
        ButtonType::Info => (
            ColorU::bootstrap_info(),
            ColorU::bootstrap_info_hover(),
            ColorU::bootstrap_info_active(),
        ),
        ButtonType::Link => (
            ColorU::TRANSPARENT,
            ColorU::TRANSPARENT,
            ColorU::TRANSPARENT,
        ),
    }
}

/// Get the text color for a button type
fn get_button_text_color(button_type: ButtonType) -> ColorU {
    match button_type {
        ButtonType::Default => ColorU::rgb(33, 37, 41),   // Dark text
        ButtonType::Warning => ColorU::BLACK,             // Black text on yellow
        ButtonType::Link => ColorU::bootstrap_link(),     // Blue link color
        _ => ColorU::WHITE,                               // White text on colored buttons
    }
}

/// Build container style properties for a button type
fn build_button_container_style(button_type: ButtonType) -> Vec<CssPropertyWithConditions> {
    let (bg_normal, bg_hover, bg_active) = get_button_colors(button_type);
    let text_color = get_button_text_color(button_type);
    
    // Focus outline uses system accent color
    let focus_outline_color = ColorU::bootstrap_primary();
    
    let mut props = Vec::with_capacity(40);
    
    // Basic layout - use InlineFlex so flex properties (justify-content, align-items) work
    props.push(CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineFlex)));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_justify_content(LayoutJustifyContent::Center)));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)));
    // Prevent stretching when inside a flex column container
    props.push(CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))));
    
    // Text color
    props.push(CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor { inner: text_color })));
    
    // Padding (Bootstrap-like)
    props.push(CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(6))));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(6))));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(12))));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_padding_right(LayoutPaddingRight::const_px(12))));
    
    // Border radius
    props.push(CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(StyleBorderTopLeftRadius::const_px(4))));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(StyleBorderTopRightRadius::const_px(4))));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(StyleBorderBottomLeftRadius::const_px(4))));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(StyleBorderBottomRightRadius::const_px(4))));
    
    if button_type == ButtonType::Link {
        // Link buttons have no background or border
        props.push(CssPropertyWithConditions::simple(CssProperty::const_background_content(
            StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(ColorU::TRANSPARENT)]),
        )));
        
        // Underline on hover - use TextDecoration::Underline variant
        props.push(CssPropertyWithConditions::on_hover(CssProperty::TextDecoration(StyleTextDecoration::Underline.into())));
    } else {
        // Normal background
        props.push(CssPropertyWithConditions::simple(CssProperty::const_background_content(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(bg_normal)]),
        )));
        
        // Border (subtle for Default, transparent for others to maintain size)
        let border_color = if button_type == ButtonType::Default {
            ColorU::rgb(206, 212, 218)
        } else {
            bg_normal
        };
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(1))));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(1))));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(1))));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle { inner: BorderStyle::Solid })));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle { inner: BorderStyle::Solid })));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid })));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor { inner: border_color })));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: border_color })));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: border_color })));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_right_color(StyleBorderRightColor { inner: border_color })));
        
        // Hover state
        props.push(CssPropertyWithConditions::on_hover(CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(bg_hover)]).into(),
        )));
        if button_type == ButtonType::Default {
            let hover_border = ColorU::rgb(173, 181, 189);
            props.push(CssPropertyWithConditions::on_hover(CssProperty::BorderTopColor(StyleBorderTopColor { inner: hover_border }.into())));
            props.push(CssPropertyWithConditions::on_hover(CssProperty::BorderBottomColor(StyleBorderBottomColor { inner: hover_border }.into())));
            props.push(CssPropertyWithConditions::on_hover(CssProperty::BorderLeftColor(StyleBorderLeftColor { inner: hover_border }.into())));
            props.push(CssPropertyWithConditions::on_hover(CssProperty::BorderRightColor(StyleBorderRightColor { inner: hover_border }.into())));
        }
        
        // Active (pressed) state
        props.push(CssPropertyWithConditions::on_active(CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(bg_active)]).into(),
        )));
        
        // Focus state - uses accent color for outline
        // This makes the button feel "native" as it uses the system accent
        props.push(CssPropertyWithConditions::on_focus(CssProperty::BorderTopColor(StyleBorderTopColor { inner: focus_outline_color }.into())));
        props.push(CssPropertyWithConditions::on_focus(CssProperty::BorderBottomColor(StyleBorderBottomColor { inner: focus_outline_color }.into())));
        props.push(CssPropertyWithConditions::on_focus(CssProperty::BorderLeftColor(StyleBorderLeftColor { inner: focus_outline_color }.into())));
        props.push(CssPropertyWithConditions::on_focus(CssProperty::BorderRightColor(StyleBorderRightColor { inner: focus_outline_color }.into())));
    }
    
    props
}

/// Build label style properties
fn build_button_label_style() -> Vec<CssPropertyWithConditions> {
    // Use system UI font
    let font_family = StyleFontFamilyVec::from_vec(vec![
        StyleFontFamily::SystemType(SystemFontType::Ui),
    ]);
    
    vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
        CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(font_family)),
    ]
}

impl Button {
    #[inline]
    pub fn create(label: AzString) -> Self {
        Self::with_type(label, ButtonType::Default)
    }
    
    /// Create a button with a specific type (Primary, Success, Danger, etc.)
    #[inline]
    pub fn with_type(label: AzString, button_type: ButtonType) -> Self {
        let container_style = build_button_container_style(button_type);
        let label_style = build_button_label_style();
        
        Self {
            label,
            image: None.into(),
            button_type,
            on_click: None.into(),
            container_style: CssPropertyWithConditionsVec::from_vec(container_style),
            label_style: CssPropertyWithConditionsVec::from_vec(label_style.clone()),
            image_style: CssPropertyWithConditionsVec::from_vec(label_style),
        }
    }
    
    /// Set the button type and update styling accordingly
    #[inline]
    pub fn set_button_type(&mut self, button_type: ButtonType) {
        self.button_type = button_type;
        self.container_style = CssPropertyWithConditionsVec::from_vec(build_button_container_style(button_type));
    }
    
    /// Builder method to set the button type
    #[inline]
    pub fn with_button_type(mut self, button_type: ButtonType) -> Self {
        self.set_button_type(button_type);
        self
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
    pub fn with_on_click<C: Into<ButtonOnClickCallback>>(
        mut self,
        data: RefAny,
        on_click: C,
    ) -> Self {
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
            Some(ButtonOnClick {
                refany: data,
                callback,
            }) => vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                callback: CoreCallback {
                    cb: callback.cb as usize,
                    ctx: callback.ctx,
                },
                refany: data,
            }],
            None => Vec::new(),
        };

        // Add both the base class and the type-specific class
        let type_class = self.button_type.class_name();
        let classes: Vec<IdOrClass> = vec![
            Class(AzString::from_const_str("__azul-native-button")),
            Class(AzString::from_const_str(type_class)),
        ];

        // Create label element with styling
        let label_dom = Dom::create_text(self.label)
            .with_css_props(self.label_style);

        // Create button container with label as child
        Dom::create_node(NodeType::Button)
            .with_child(label_dom)
            .with_ids_and_classes(IdOrClassVec::from_vec(classes))
            .with_css_props(self.container_style)
            .with_callbacks(callbacks.into())
            .with_tab_index(TabIndex::Auto)
    }
}
