use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec},
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
use azul_css::css::BoxOrStatic;

use crate::callbacks::Callback;

const STYLE_BACKGROUND_CONTENT_2688422633177340412_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            LINEAR_COLOR_STOP_12009347504665939_ITEMS,
        ),
    })];
const STYLE_BACKGROUND_CONTENT_11062356617965867290_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ColorU {
        r: 240,
        g: 240,
        b: 240,
        a: 255,
    })];
const STYLE_BACKGROUND_CONTENT_14586281004485141058_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            LINEAR_COLOR_STOP_3104396762583413726_ITEMS,
        ),
    })];
const LINEAR_COLOR_STOP_12009347504665939_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(ColorU {
            r: 193,
            g: 255,
            b: 187,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(10),
        color: ColorOrSystem::color(ColorU {
            r: 205,
            g: 255,
            b: 205,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(15),
        color: ColorOrSystem::color(ColorU {
            r: 156,
            g: 238,
            b: 172,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(20),
        color: ColorOrSystem::color(ColorU {
            r: 0,
            g: 211,
            b: 40,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(30),
        color: ColorOrSystem::color(ColorU {
            r: 0,
            g: 211,
            b: 40,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(70),
        color: ColorOrSystem::color(ColorU {
            r: 32,
            g: 219,
            b: 65,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 32,
            g: 219,
            b: 65,
            a: 255,
        }),
    },
];
const LINEAR_COLOR_STOP_3104396762583413726_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(ColorU {
            r: 243,
            g: 243,
            b: 243,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(10),
        color: ColorOrSystem::color(ColorU {
            r: 252,
            g: 252,
            b: 252,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(15),
        color: ColorOrSystem::color(ColorU {
            r: 218,
            g: 218,
            b: 218,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(20),
        color: ColorOrSystem::color(ColorU {
            r: 201,
            g: 201,
            b: 201,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(30),
        color: ColorOrSystem::color(ColorU {
            r: 218,
            g: 218,
            b: 218,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(70),
        color: ColorOrSystem::color(ColorU {
            r: 203,
            g: 203,
            b: 203,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 203,
            g: 203,
            b: 203,
            a: 255,
        }),
    },
];

#[derive(Debug, Clone)]
#[repr(C)]
pub struct ProgressBar {
    pub progressbar_state: ProgressBarState,
    pub height: PixelValue,
    pub bar_background: StyleBackgroundContentVec,
    pub container_background: StyleBackgroundContentVec,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct ProgressBarState {
    pub percent_done: f32,
    pub display_percentage: bool,
}

impl ProgressBar {
    #[inline]
    pub fn create(percent_done: f32) -> Self {
        Self {
            progressbar_state: ProgressBarState {
                percent_done,
                display_percentage: false,
            },
            height: PixelValue::const_px(15),
            bar_background: StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_2688422633177340412_ITEMS,
            ),
            container_background: StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_14586281004485141058_ITEMS,
            ),
        }
    }

    #[inline]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(0.0);
        core::mem::swap(&mut s, self);
        s
    }

    pub fn set_container_background(&mut self, background: StyleBackgroundContentVec) {
        self.container_background = background;
    }

    pub fn with_container_background(mut self, background: StyleBackgroundContentVec) -> Self {
        self.set_container_background(background);
        self
    }

    pub fn set_bar_background(&mut self, background: StyleBackgroundContentVec) {
        self.bar_background = background;
    }

    pub fn with_bar_background(mut self, background: StyleBackgroundContentVec) -> Self {
        self.set_bar_background(background);
        self
    }

    pub fn set_height(&mut self, height: PixelValue) {
        self.height = height;
    }

    pub fn with_height(mut self, height: PixelValue) -> Self {
        self.set_height(height);
        self
    }

    pub fn dom(self) -> Dom {
        use azul_core::dom::DomVec;

        // Use percentage widths for the progress bar and remaining space.
        // The container uses flex-direction: row, and we set explicit widths
        // on the children using CSS percentages.
        let percent_done = self.progressbar_state.percent_done.max(0.0).min(100.0);

        Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                // .__azul-native-progress-bar-container
                CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
                    LayoutHeight::Px(self.height.clone()),
                ))),
                CssPropertyWithConditions::simple(CssProperty::FlexDirection(
                    LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row),
                )),
                CssPropertyWithConditions::simple(CssProperty::BoxShadowBottom(
                    StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                        offset_x: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        offset_y: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        color: ColorU {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 9,
                        },
                        blur_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(15),
                        },
                        spread_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(2),
                        },
                        clip_mode: BoxShadowClipMode::Inset,
                    })),
                )),
                CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(
                    StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                        offset_x: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        offset_y: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        color: ColorU {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 9,
                        },
                        blur_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(15),
                        },
                        spread_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(2),
                        },
                        clip_mode: BoxShadowClipMode::Inset,
                    })),
                )),
                CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(
                    StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                        offset_x: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        offset_y: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        color: ColorU {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 9,
                        },
                        blur_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(15),
                        },
                        spread_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(2),
                        },
                        clip_mode: BoxShadowClipMode::Inset,
                    })),
                )),
                CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(
                    StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                        offset_x: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        offset_y: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        color: ColorU {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 9,
                        },
                        blur_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(15),
                        },
                        spread_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(2),
                        },
                        clip_mode: BoxShadowClipMode::Inset,
                    })),
                )),
                CssPropertyWithConditions::simple(CssProperty::BorderBottomRightRadius(
                    StyleBorderBottomRightRadiusValue::Exact(StyleBorderBottomRightRadius {
                        inner: PixelValue::const_px(3),
                    }),
                )),
                CssPropertyWithConditions::simple(CssProperty::BorderBottomLeftRadius(
                    StyleBorderBottomLeftRadiusValue::Exact(StyleBorderBottomLeftRadius {
                        inner: PixelValue::const_px(3),
                    }),
                )),
                CssPropertyWithConditions::simple(CssProperty::BorderTopRightRadius(
                    StyleBorderTopRightRadiusValue::Exact(StyleBorderTopRightRadius {
                        inner: PixelValue::const_px(3),
                    }),
                )),
                CssPropertyWithConditions::simple(CssProperty::BorderTopLeftRadius(
                    StyleBorderTopLeftRadiusValue::Exact(StyleBorderTopLeftRadius {
                        inner: PixelValue::const_px(3),
                    }),
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
                            r: 178,
                            g: 178,
                            b: 178,
                            a: 255,
                        },
                    }),
                )),
                CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
                    StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
                        inner: ColorU {
                            r: 178,
                            g: 178,
                            b: 178,
                            a: 255,
                        },
                    }),
                )),
                CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
                    StyleBorderRightColorValue::Exact(StyleBorderRightColor {
                        inner: ColorU {
                            r: 178,
                            g: 178,
                            b: 178,
                            a: 255,
                        },
                    }),
                )),
                CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
                    StyleBorderTopColorValue::Exact(StyleBorderTopColor {
                        inner: ColorU {
                            r: 178,
                            g: 178,
                            b: 178,
                            a: 255,
                        },
                    }),
                )),
                CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
                    StyleBackgroundContentVecValue::Exact(self.container_background.clone()),
                )),
            ]))
            .with_ids_and_classes({
                const IDS_AND_CLASSES_10874511710181900075: &[IdOrClass] = &[Class(
                    AzString::from_const_str("__azul-native-progress-bar-container"),
                )];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_10874511710181900075)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::create_div()
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                        // .__azul-native-progress-bar-bar
                        // Use percentage width instead of flex-grow hack
                        CssPropertyWithConditions::simple(CssProperty::Width(
                            LayoutWidthValue::Exact(LayoutWidth::Px(
                                PixelValue::percent(percent_done),
                            )),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowBottom(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderBottomRightRadius(
                            StyleBorderBottomRightRadiusValue::Exact(
                                StyleBorderBottomRightRadius {
                                    inner: PixelValue::const_px(1),
                                },
                            ),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderBottomLeftRadius(
                            StyleBorderBottomLeftRadiusValue::Exact(StyleBorderBottomLeftRadius {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderTopRightRadius(
                            StyleBorderTopRightRadiusValue::Exact(StyleBorderTopRightRadius {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderTopLeftRadius(
                            StyleBorderTopLeftRadiusValue::Exact(StyleBorderTopLeftRadius {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
                            StyleBackgroundContentVecValue::Exact(self.bar_background.clone()),
                        )),
                    ]))
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_16512648314570682783: &[IdOrClass] = &[Class(
                            AzString::from_const_str("__azul-native-progress-bar-bar"),
                        )];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_16512648314570682783)
                    }),
                Dom::create_div()
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                        // .__azul-native-progress-bar-remaining
                        // Use percentage width for the remaining space
                        CssPropertyWithConditions::simple(CssProperty::Width(
                            LayoutWidthValue::Exact(LayoutWidth::Px(
                                PixelValue::percent(100.0 - percent_done),
                            )),
                        )),
                    ]))
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_2492405364126620395: &[IdOrClass] = &[Class(
                            AzString::from_const_str("__azul-native-progress-bar-remaining"),
                        )];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2492405364126620395)
                    }),
            ]))
    }
}
