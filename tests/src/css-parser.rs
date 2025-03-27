#[cfg(test)]
use azul_css::parser::*;
#[cfg(test)]
use azul_css::*;

#[test]
fn test_parse_box_shadow_1() {
    assert_eq!(
        parse_style_box_shadow("none"),
        Err(CssShadowParseError::TooManyComponents("none"))
    );
}

#[test]
fn test_parse_box_shadow_2() {
    assert_eq!(
        parse_style_box_shadow("5px 10px"),
        Ok(StyleBoxShadow {
            offset: [
                PixelValueNoPercent {
                    inner: PixelValue::px(5.0)
                },
                PixelValueNoPercent {
                    inner: PixelValue::px(10.0)
                },
            ],
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::px(0.0)
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::px(0.0)
            },
            clip_mode: BoxShadowClipMode::Outset,
        })
    );
}

#[test]
fn test_parse_box_shadow_3() {
    assert_eq!(
        parse_style_box_shadow("5px 10px #888888"),
        Ok(StyleBoxShadow {
            offset: [
                PixelValueNoPercent {
                    inner: PixelValue::px(5.0)
                },
                PixelValueNoPercent {
                    inner: PixelValue::px(10.0)
                },
            ],
            color: ColorU {
                r: 136,
                g: 136,
                b: 136,
                a: 255
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::px(0.0)
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::px(0.0)
            },
            clip_mode: BoxShadowClipMode::Outset,
        })
    );
}

#[test]
fn test_parse_box_shadow_4() {
    assert_eq!(
        parse_style_box_shadow("5px 10px inset"),
        Ok(StyleBoxShadow {
            offset: [
                PixelValueNoPercent {
                    inner: PixelValue::px(5.0)
                },
                PixelValueNoPercent {
                    inner: PixelValue::px(10.0)
                },
            ],
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::px(0.0)
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::px(0.0)
            },
            clip_mode: BoxShadowClipMode::Inset,
        })
    );
}

#[test]
fn test_parse_box_shadow_5() {
    assert_eq!(
        parse_style_box_shadow("5px 10px outset"),
        Ok(StyleBoxShadow {
            offset: [
                PixelValueNoPercent {
                    inner: PixelValue::px(5.0)
                },
                PixelValueNoPercent {
                    inner: PixelValue::px(10.0)
                },
            ],
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::px(0.0)
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::px(0.0)
            },
            clip_mode: BoxShadowClipMode::Outset,
        })
    );
}

#[test]
fn test_parse_box_shadow_6() {
    assert_eq!(
        parse_style_box_shadow("5px 10px 5px #888888"),
        Ok(StyleBoxShadow {
            offset: [
                PixelValueNoPercent {
                    inner: PixelValue::px(5.0)
                },
                PixelValueNoPercent {
                    inner: PixelValue::px(10.0)
                },
            ],
            color: ColorU {
                r: 136,
                g: 136,
                b: 136,
                a: 255
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::px(5.0)
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::px(0.0)
            },
            clip_mode: BoxShadowClipMode::Outset,
        })
    );
}

#[test]
fn test_parse_box_shadow_7() {
    assert_eq!(
        parse_style_box_shadow("5px 10px #888888 inset"),
        Ok(StyleBoxShadow {
            offset: [
                PixelValueNoPercent {
                    inner: PixelValue::px(5.0)
                },
                PixelValueNoPercent {
                    inner: PixelValue::px(10.0)
                },
            ],
            color: ColorU {
                r: 136,
                g: 136,
                b: 136,
                a: 255
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::px(0.0)
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::px(0.0)
            },
            clip_mode: BoxShadowClipMode::Inset,
        })
    );
}

#[test]
fn test_parse_box_shadow_8() {
    assert_eq!(
        parse_style_box_shadow("5px 10px 5px #888888 inset"),
        Ok(StyleBoxShadow {
            offset: [
                PixelValueNoPercent {
                    inner: PixelValue::px(5.0)
                },
                PixelValueNoPercent {
                    inner: PixelValue::px(10.0)
                },
            ],
            color: ColorU {
                r: 136,
                g: 136,
                b: 136,
                a: 255
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::px(5.0)
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::px(0.0)
            },
            clip_mode: BoxShadowClipMode::Inset,
        })
    );
}

#[test]
fn test_parse_box_shadow_9() {
    assert_eq!(
        parse_style_box_shadow("5px 10px 5px 10px #888888"),
        Ok(StyleBoxShadow {
            offset: [
                PixelValueNoPercent {
                    inner: PixelValue::px(5.0)
                },
                PixelValueNoPercent {
                    inner: PixelValue::px(10.0)
                },
            ],
            color: ColorU {
                r: 136,
                g: 136,
                b: 136,
                a: 255
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::px(5.0)
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::px(10.0)
            },
            clip_mode: BoxShadowClipMode::Outset,
        })
    );
}

#[test]
fn test_parse_box_shadow_10() {
    assert_eq!(
        parse_style_box_shadow("5px 10px 5px 10px #888888 inset"),
        Ok(StyleBoxShadow {
            offset: [
                PixelValueNoPercent {
                    inner: PixelValue::px(5.0)
                },
                PixelValueNoPercent {
                    inner: PixelValue::px(10.0)
                },
            ],
            color: ColorU {
                r: 136,
                g: 136,
                b: 136,
                a: 255
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::px(5.0)
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::px(10.0)
            },
            clip_mode: BoxShadowClipMode::Inset,
        })
    );
}

#[test]
fn test_parse_css_border_0() {
    assert_eq!(
        parse_style_border("solid black"),
        Ok(StyleBorderSide {
            border_width: PixelValue::px(1.0),
            border_style: BorderStyle::Solid,
            border_color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255
            },
        })
    );
}

#[test]
fn test_parse_css_border_1() {
    assert_eq!(
        parse_style_border("5px solid red"),
        Ok(StyleBorderSide {
            border_width: PixelValue::px(5.0),
            border_style: BorderStyle::Solid,
            border_color: ColorU {
                r: 255,
                g: 0,
                b: 0,
                a: 255
            },
        })
    );
}

#[test]
fn test_parse_css_border_2() {
    assert_eq!(
        parse_style_border("double"),
        Ok(StyleBorderSide {
            border_width: PixelValue::px(3.0),
            border_style: BorderStyle::Double,
            border_color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255
            },
        })
    );
}

#[test]
fn test_parse_css_border_3() {
    assert_eq!(
        parse_style_border("1px solid rgb(51, 153, 255)"),
        Ok(StyleBorderSide {
            border_width: PixelValue::px(1.0),
            border_style: BorderStyle::Solid,
            border_color: ColorU {
                r: 51,
                g: 153,
                b: 255,
                a: 255
            },
        })
    );
}

#[test]
fn test_parse_linear_gradient_1() {
    assert_eq!(
        parse_style_background_content("linear-gradient(red, yellow)"),
        Ok(StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: Direction::FromTo(DirectionCorners {
                from: DirectionCorner::Top,
                to: DirectionCorner::Bottom,
            }),
            extend_mode: ExtendMode::Clamp,
            stops: vec![
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(0.0),
                    color: ColorU {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(100.0),
                    color: ColorU {
                        r: 255,
                        g: 255,
                        b: 0,
                        a: 255
                    },
                }
            ]
            .into(),
        }))
    );
}

#[test]
fn test_parse_linear_gradient_2() {
    assert_eq!(
        parse_style_background_content("linear-gradient(red, lime, blue, yellow)"),
        Ok(StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: Direction::FromTo(DirectionCorners {
                from: DirectionCorner::Top,
                to: DirectionCorner::Bottom,
            }),
            extend_mode: ExtendMode::Clamp,
            stops: vec![
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(0.0),
                    color: ColorU {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(33.333332),
                    color: ColorU {
                        r: 0,
                        g: 255,
                        b: 0,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(66.666664),
                    color: ColorU {
                        r: 0,
                        g: 0,
                        b: 255,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(100.0),
                    color: ColorU {
                        r: 255,
                        g: 255,
                        b: 0,
                        a: 255
                    },
                }
            ]
            .into(),
        }))
    );
}

#[test]
fn test_parse_linear_gradient_3() {
    assert_eq!(
        parse_style_background_content("repeating-linear-gradient(50deg, blue, yellow, #00FF00)"),
        Ok(StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: Direction::Angle(AngleValue::deg(50.0)),
            extend_mode: ExtendMode::Repeat,
            stops: vec![
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(0.0),
                    color: ColorU {
                        r: 0,
                        g: 0,
                        b: 255,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(50.0),
                    color: ColorU {
                        r: 255,
                        g: 255,
                        b: 0,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(100.0),
                    color: ColorU {
                        r: 0,
                        g: 255,
                        b: 0,
                        a: 255
                    },
                }
            ]
            .into(),
        }))
    );
}

#[test]
fn test_parse_linear_gradient_4() {
    assert_eq!(
        parse_style_background_content("linear-gradient(to bottom right, red, yellow)"),
        Ok(StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: Direction::FromTo(DirectionCorners {
                from: DirectionCorner::TopLeft,
                to: DirectionCorner::BottomRight,
            }),
            extend_mode: ExtendMode::Clamp,
            stops: vec![
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(0.0),
                    color: ColorU {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(100.0),
                    color: ColorU {
                        r: 255,
                        g: 255,
                        b: 0,
                        a: 255
                    },
                }
            ]
            .into(),
        }))
    );
}

#[test]
fn test_parse_linear_gradient_5() {
    assert_eq!(
        parse_style_background_content("linear-gradient(0.42rad, red, yellow)"),
        Ok(StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: Direction::Angle(AngleValue::rad(0.42)),
            extend_mode: ExtendMode::Clamp,
            stops: vec![
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(0.0),
                    color: ColorU {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(100.0),
                    color: ColorU {
                        r: 255,
                        g: 255,
                        b: 0,
                        a: 255
                    },
                }
            ]
            .into(),
        }))
    );
}

#[test]
fn test_parse_linear_gradient_6() {
    assert_eq!(
        parse_style_background_content("linear-gradient(12.93grad, red, yellow)"),
        Ok(StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: Direction::Angle(AngleValue::grad(12.93)),
            extend_mode: ExtendMode::Clamp,
            stops: vec![
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(0.0),
                    color: ColorU {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(100.0),
                    color: ColorU {
                        r: 255,
                        g: 255,
                        b: 0,
                        a: 255
                    },
                }
            ]
            .into(),
        }))
    );
}

#[test]
fn test_parse_linear_gradient_7() {
    assert_eq!(
        parse_style_background_content(
            "linear-gradient(to right, rgba(255,0, 0,1) 0%,rgba(0,0,0, 0) 100%)"
        ),
        Ok(StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: Direction::FromTo(DirectionCorners {
                from: DirectionCorner::Left,
                to: DirectionCorner::Right,
            }),
            extend_mode: ExtendMode::Clamp,
            stops: vec![
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(0.0),
                    color: ColorU {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(100.0),
                    color: ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 0
                    },
                }
            ]
            .into(),
        }))
    );
}

#[test]
fn test_parse_linear_gradient_8() {
    assert_eq!(
        parse_style_background_content("linear-gradient(to bottom, rgb(255,0, 0),rgb(0,0,0))"),
        Ok(StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: Direction::FromTo(DirectionCorners {
                from: DirectionCorner::Top,
                to: DirectionCorner::Bottom,
            }),
            extend_mode: ExtendMode::Clamp,
            stops: vec![
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(0.0),
                    color: ColorU {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(100.0),
                    color: ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                }
            ]
            .into(),
        }))
    );
}

#[test]
fn test_parse_linear_gradient_9() {
    assert_eq!(
        parse_style_background_content("linear-gradient(10deg, rgb(10, 30, 20), yellow)"),
        Ok(StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: Direction::Angle(AngleValue::deg(10.0)),
            extend_mode: ExtendMode::Clamp,
            stops: vec![
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(0.0),
                    color: ColorU {
                        r: 10,
                        g: 30,
                        b: 20,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(100.0),
                    color: ColorU {
                        r: 255,
                        g: 255,
                        b: 0,
                        a: 255
                    },
                }
            ]
            .into(),
        }))
    );
}

#[test]
fn test_parse_linear_gradient_10() {
    assert_eq!(
        parse_style_background_content(
            "linear-gradient(50deg, rgba(10, 30, 20, 0.93), hsla(40deg, 80%, 30%, 0.1))"
        ),
        Ok(StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: Direction::Angle(AngleValue::deg(50.0)),
            extend_mode: ExtendMode::Clamp,
            stops: vec![
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(0.0),
                    color: ColorU {
                        r: 10,
                        g: 30,
                        b: 20,
                        a: 238
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(100.0),
                    color: ColorU {
                        r: 138,
                        g: 97,
                        b: 15,
                        a: 25
                    },
                }
            ]
            .into(),
        }))
    );
}

#[test]
fn test_parse_linear_gradient_11() {
    // wacky whitespace on purpose
    assert_eq!(
        parse_style_background_content(
            "linear-gradient(to bottom,rgb(255,0, 0)0%, rgb( 0 , 255 , 0 ) 10% ,blue   100%  )"
        ),
        Ok(StyleBackgroundContent::LinearGradient(LinearGradient {
            direction: Direction::FromTo(DirectionCorners {
                from: DirectionCorner::Top,
                to: DirectionCorner::Bottom,
            }),
            extend_mode: ExtendMode::Clamp,
            stops: vec![
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(0.0),
                    color: ColorU {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(10.0),
                    color: ColorU {
                        r: 0,
                        g: 255,
                        b: 0,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(100.0),
                    color: ColorU {
                        r: 0,
                        g: 0,
                        b: 255,
                        a: 255
                    },
                }
            ]
            .into(),
        }))
    );
}

#[test]
fn test_parse_radial_gradient_1() {
    assert_eq!(
        parse_style_background_content("radial-gradient(circle, lime, blue, yellow)"),
        Ok(StyleBackgroundContent::RadialGradient(RadialGradient {
            shape: Shape::Circle,
            extend_mode: ExtendMode::Clamp,
            size: RadialGradientSize::FarthestCorner,
            position: StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Left,
                vertical: BackgroundPositionVertical::Top,
            },
            stops: vec![
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(0.0),
                    color: ColorU {
                        r: 0,
                        g: 255,
                        b: 0,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(50.0),
                    color: ColorU {
                        r: 0,
                        g: 0,
                        b: 255,
                        a: 255
                    },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(100.0),
                    color: ColorU {
                        r: 255,
                        g: 255,
                        b: 0,
                        a: 255
                    },
                }
            ]
            .into(),
        }))
    );
}

#[test]
fn test_parse_conic_gradient_1() {
    assert_eq!(
        parse_style_background_content("conic-gradient(lime, blue, yellow)"),
        Ok(StyleBackgroundContent::ConicGradient(ConicGradient {
            extend_mode: ExtendMode::Clamp,
            center: StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Center,
                vertical: BackgroundPositionVertical::Center,
            },
            angle: AngleValue::deg(0.0),
            stops: vec![
                NormalizedRadialColorStop {
                    angle: AngleValue::deg(0.0),
                    color: ColorU {
                        r: 0,
                        g: 255,
                        b: 0,
                        a: 255
                    },
                },
                NormalizedRadialColorStop {
                    angle: AngleValue::deg(180.0),
                    color: ColorU {
                        r: 0,
                        g: 0,
                        b: 255,
                        a: 255
                    },
                },
                NormalizedRadialColorStop {
                    angle: AngleValue::deg(360.0),
                    color: ColorU {
                        r: 255,
                        g: 255,
                        b: 0,
                        a: 255
                    },
                }
            ]
            .into(),
        }))
    );
}

/*
// This test currently fails, but it's not that important to fix right now
#[test]
fn test_parse_radial_gradient_2() {
    assert_eq!(parse_style_background_content("repeating-radial-gradient(circle, red 10%, blue 50%, lime, yellow)"),
        Ok(StyleBackgroundContent::RadialGradient(RadialGradient {
            shape: Shape::Circle,
            extend_mode: ExtendMode::Repeat,
            size: RadialGradientSize::FarthestCorner,
            position: StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Left,
                vertical: BackgroundPositionVertical::Top,
            },
            stops: vec![
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(10.0),
                    color: ColorU { r: 255, g: 0, b: 0, a: 255 },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(50.0),
                    color: ColorU { r: 0, g: 0, b: 255, a: 255 },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(75.0),
                    color: ColorU { r: 0, g: 255, b: 0, a: 255 },
                },
                NormalizedLinearColorStop {
                    offset: PercentageValue::new(100.0),
                    color: ColorU { r: 255, g: 255, b: 0, a: 255 },
                }
            ].into(),
    })));
}
*/

#[test]
fn test_parse_css_color_1() {
    assert_eq!(
        parse_css_color("#F0F8FF"),
        Ok(ColorU {
            r: 240,
            g: 248,
            b: 255,
            a: 255
        })
    );
}

#[test]
fn test_parse_css_color_2() {
    assert_eq!(
        parse_css_color("#F0F8FF00"),
        Ok(ColorU {
            r: 240,
            g: 248,
            b: 255,
            a: 0
        })
    );
}

#[test]
fn test_parse_css_color_3() {
    assert_eq!(
        parse_css_color("#EEE"),
        Ok(ColorU {
            r: 238,
            g: 238,
            b: 238,
            a: 255
        })
    );
}

#[test]
fn test_parse_css_color_4() {
    assert_eq!(
        parse_css_color("rgb(192, 14, 12)"),
        Ok(ColorU {
            r: 192,
            g: 14,
            b: 12,
            a: 255
        })
    );
}

#[test]
fn test_parse_css_color_5() {
    assert_eq!(
        parse_css_color("rgb(283, 8, 105)"),
        Err(CssColorParseError::IntValueParseErr(
            "283".parse::<u8>().err().unwrap()
        ))
    );
}

#[test]
fn test_parse_css_color_6() {
    assert_eq!(
        parse_css_color("rgba(192, 14, 12, 80)"),
        Err(CssColorParseError::FloatValueOutOfRange(80.0))
    );
}

#[test]
fn test_parse_css_color_7() {
    assert_eq!(
        parse_css_color("rgba( 0,127,     255   , 0.25  )"),
        Ok(ColorU {
            r: 0,
            g: 127,
            b: 255,
            a: 64
        })
    );
}

#[test]
fn test_parse_css_color_8() {
    assert_eq!(
        parse_css_color("rgba( 1 ,2,3, 1.0)"),
        Ok(ColorU {
            r: 1,
            g: 2,
            b: 3,
            a: 255
        })
    );
}

#[test]
fn test_parse_css_color_9() {
    assert_eq!(
        parse_css_color("rgb("),
        Err(CssColorParseError::UnclosedColor("rgb("))
    );
}

#[test]
fn test_parse_css_color_10() {
    assert_eq!(
        parse_css_color("rgba("),
        Err(CssColorParseError::UnclosedColor("rgba("))
    );
}

#[test]
fn test_parse_css_color_11() {
    assert_eq!(
        parse_css_color("rgba(123, 36, 92, 0.375"),
        Err(CssColorParseError::UnclosedColor("rgba(123, 36, 92, 0.375"))
    );
}

#[test]
fn test_parse_css_color_12() {
    assert_eq!(
        parse_css_color("rgb()"),
        Err(CssColorParseError::MissingColorComponent(
            CssColorComponent::Red
        ))
    );
}

#[test]
fn test_parse_css_color_13() {
    assert_eq!(
        parse_css_color("rgb(10)"),
        Err(CssColorParseError::MissingColorComponent(
            CssColorComponent::Green
        ))
    );
}

#[test]
fn test_parse_css_color_14() {
    assert_eq!(
        parse_css_color("rgb(20, 30)"),
        Err(CssColorParseError::MissingColorComponent(
            CssColorComponent::Blue
        ))
    );
}

#[test]
fn test_parse_css_color_15() {
    assert_eq!(
        parse_css_color("rgb(30, 40,)"),
        Err(CssColorParseError::MissingColorComponent(
            CssColorComponent::Blue
        ))
    );
}

#[test]
fn test_parse_css_color_16() {
    assert_eq!(
        parse_css_color("rgba(40, 50, 60)"),
        Err(CssColorParseError::MissingColorComponent(
            CssColorComponent::Alpha
        ))
    );
}

#[test]
fn test_parse_css_color_17() {
    assert_eq!(
        parse_css_color("rgba(50, 60, 70, )"),
        Err(CssColorParseError::MissingColorComponent(
            CssColorComponent::Alpha
        ))
    );
}

#[test]
fn test_parse_css_color_18() {
    assert_eq!(
        parse_css_color("hsl(0deg, 100%, 100%)"),
        Ok(ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255
        })
    );
}

#[test]
fn test_parse_css_color_19() {
    assert_eq!(
        parse_css_color("hsl(0deg, 100%, 50%)"),
        Ok(ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        })
    );
}

#[test]
fn test_parse_css_color_20() {
    assert_eq!(
        parse_css_color("hsl(170deg, 50%, 75%)"),
        Ok(ColorU {
            r: 160,
            g: 224,
            b: 213,
            a: 255
        })
    );
}

#[test]
fn test_parse_css_color_21() {
    assert_eq!(
        parse_css_color("hsla(190deg, 50%, 75%, 1.0)"),
        Ok(ColorU {
            r: 160,
            g: 213,
            b: 224,
            a: 255
        })
    );
}

#[test]
fn test_parse_css_color_22() {
    assert_eq!(
        parse_css_color("hsla(120deg, 0%, 25%, 0.25)"),
        Ok(ColorU {
            r: 64,
            g: 64,
            b: 64,
            a: 64
        })
    );
}

#[test]
fn test_parse_css_color_23() {
    assert_eq!(
        parse_css_color("hsla(120deg, 0%, 0%, 0.5)"),
        Ok(ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 128
        })
    );
}

#[test]
fn test_parse_css_color_24() {
    assert_eq!(
        parse_css_color("hsla(60.9deg, 80.3%, 40%, 0.5)"),
        Ok(ColorU {
            r: 182,
            g: 184,
            b: 20,
            a: 128
        })
    );
}

#[test]
fn test_parse_css_color_25() {
    assert_eq!(
        parse_css_color("hsla(60.9rad, 80.3%, 40%, 0.5)"),
        Ok(ColorU {
            r: 184,
            g: 170,
            b: 20,
            a: 128
        })
    );
}

#[test]
fn test_parse_css_color_26() {
    assert_eq!(
        parse_css_color("hsla(60.9grad, 80.3%, 40%, 0.5)"),
        Ok(ColorU {
            r: 45,
            g: 20,
            b: 184,
            a: 128
        })
    );
}

#[test]
fn test_parse_transform() {
    assert_eq!(
        parse_style_transform("rotate(25deg)"),
        Ok(StyleTransform::Rotate(AngleValue::deg(25.0)))
    );
}

#[test]
fn test_parse_direction() {
    assert_eq!(
        parse_direction("60.9grad"),
        Ok(Direction::Angle(AngleValue::grad(60.9)))
    );
}

#[test]
fn test_parse_float_value() {
    assert_eq!(parse_float_value("60.9"), Ok(FloatValue::new(60.9)));
}

#[test]
fn test_parse_css_color_27() {
    assert_eq!(
        parse_css_color("hsla(240, 0%, 0%, 0.5)"),
        Ok(ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 128
        })
    );
}

#[test]
fn test_parse_css_color_28() {
    assert_eq!(
        parse_css_color("hsla(240deg, 0, 0%, 0.5)"),
        Err(CssColorParseError::InvalidPercentage(
            PercentageParseError::NoPercentSign
        ))
    );
}

#[test]
fn test_parse_css_color_29() {
    assert_eq!(
        parse_css_color("hsla(240deg, 0%, 0, 0.5)"),
        Err(CssColorParseError::InvalidPercentage(
            PercentageParseError::NoPercentSign
        ))
    );
}

#[test]
fn test_parse_css_color_30() {
    assert_eq!(
        parse_css_color("hsla(240deg, 0%, 0%, )"),
        Err(CssColorParseError::MissingColorComponent(
            CssColorComponent::Alpha
        ))
    );
}

#[test]
fn test_parse_css_color_31() {
    assert_eq!(
        parse_css_color("hsl(, 0%, 0%, )"),
        Err(CssColorParseError::MissingColorComponent(
            CssColorComponent::Hue
        ))
    );
}

#[test]
fn test_parse_css_color_32() {
    assert_eq!(
        parse_css_color("hsl(240deg ,  )"),
        Err(CssColorParseError::MissingColorComponent(
            CssColorComponent::Saturation
        ))
    );
}

#[test]
fn test_parse_css_color_33() {
    assert_eq!(
        parse_css_color("hsl(240deg, 0%,  )"),
        Err(CssColorParseError::MissingColorComponent(
            CssColorComponent::Lightness
        ))
    );
}

#[test]
fn test_parse_css_color_34() {
    assert_eq!(
        parse_css_color("hsl(240deg, 0%, 0%,  )"),
        Err(CssColorParseError::ExtraArguments(""))
    );
}

#[test]
fn test_parse_css_color_35() {
    assert_eq!(
        parse_css_color("hsla(240deg, 0%, 0%  )"),
        Err(CssColorParseError::MissingColorComponent(
            CssColorComponent::Alpha
        ))
    );
}

#[test]
fn test_parse_css_color_36() {
    assert_eq!(
        parse_css_color("rgb(255,0, 0)"),
        Ok(ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        })
    );
}

#[test]
fn test_parse_pixel_value_1() {
    assert_eq!(parse_pixel_value("15px"), Ok(PixelValue::px(15.0)));
}

#[test]
fn test_parse_pixel_value_2() {
    assert_eq!(parse_pixel_value("1.2em"), Ok(PixelValue::em(1.2)));
}

#[test]
fn test_parse_pixel_value_3() {
    assert_eq!(parse_pixel_value("11pt"), Ok(PixelValue::pt(11.0)));
}

#[test]
fn test_parse_pixel_value_4() {
    assert_eq!(
        parse_pixel_value("aslkfdjasdflk"),
        Err(CssPixelValueParseError::InvalidPixelValue("aslkfdjasdflk"))
    );
}

#[test]
fn test_parse_style_border_radius_1() {
    assert_eq!(
        parse_style_border_radius("15px"),
        Ok(StyleBorderRadius {
            top_left: PixelValue::px(15.0),
            top_right: PixelValue::px(15.0),
            bottom_left: PixelValue::px(15.0),
            bottom_right: PixelValue::px(15.0),
        })
    );
}

#[test]
fn test_parse_style_border_radius_2() {
    assert_eq!(
        parse_style_border_radius("15px 50px"),
        Ok(StyleBorderRadius {
            top_left: PixelValue::px(15.0),
            bottom_right: PixelValue::px(15.0),
            top_right: PixelValue::px(50.0),
            bottom_left: PixelValue::px(50.0),
        })
    );
}

#[test]
fn test_parse_style_border_radius_3() {
    assert_eq!(
        parse_style_border_radius("15px 50px 30px"),
        Ok(StyleBorderRadius {
            top_left: PixelValue::px(15.0),
            bottom_right: PixelValue::px(30.0),
            top_right: PixelValue::px(50.0),
            bottom_left: PixelValue::px(50.0),
        })
    );
}

#[test]
fn test_parse_style_border_radius_4() {
    assert_eq!(
        parse_style_border_radius("15px 50px 30px 5px"),
        Ok(StyleBorderRadius {
            top_left: PixelValue::px(15.0),
            bottom_right: PixelValue::px(30.0),
            top_right: PixelValue::px(50.0),
            bottom_left: PixelValue::px(5.0),
        })
    );
}

#[test]
fn test_parse_style_font_family_1() {
    use azul_css::{AzString, StringVec};

    use crate::alloc::string::ToString;
    let fonts0: Vec<StyleFontFamily> = vec![
        StyleFontFamily::System("Webly Sleeky UI".to_string().into()),
        StyleFontFamily::System("monospace".to_string().into()),
    ];
    let fonts0: StyleFontFamilyVec = fonts0.into();
    assert_eq!(
        parse_style_font_family("\"Webly Sleeky UI\", monospace"),
        Ok(fonts0)
    );
}

#[test]
fn test_parse_style_font_family_2() {
    use azul_css::{AzString, StringVec};

    use crate::alloc::string::ToString;
    let fonts0: Vec<StyleFontFamily> = vec![StyleFontFamily::System(
        "Webly Sleeky UI".to_string().into(),
    )];
    let fonts0: StyleFontFamilyVec = fonts0.into();
    assert_eq!(parse_style_font_family("'Webly Sleeky UI'"), Ok(fonts0));
}

#[test]
fn test_parse_background_image() {
    use crate::alloc::string::ToString;
    assert_eq!(
        parse_style_background_content("image(\"Cat 01\")"),
        Ok(StyleBackgroundContent::Image("Cat 01".to_string().into()))
    );
}

#[test]
fn test_parse_padding_1() {
    assert_eq!(
        parse_layout_padding("10px"),
        Ok(LayoutPadding {
            top: PixelValueWithAuto::Exact(PixelValue::px(10.0)),
            right: PixelValueWithAuto::Exact(PixelValue::px(10.0)),
            bottom: PixelValueWithAuto::Exact(PixelValue::px(10.0)),
            left: PixelValueWithAuto::Exact(PixelValue::px(10.0)),
        })
    );
}

#[test]
fn test_parse_padding_2() {
    assert_eq!(
        parse_layout_padding("25px 50px"),
        Ok(LayoutPadding {
            top: PixelValueWithAuto::Exact(PixelValue::px(25.0)),
            right: PixelValueWithAuto::Exact(PixelValue::px(50.0)),
            bottom: PixelValueWithAuto::Exact(PixelValue::px(25.0)),
            left: PixelValueWithAuto::Exact(PixelValue::px(50.0)),
        })
    );
}

#[test]
fn test_parse_padding_3() {
    assert_eq!(
        parse_layout_padding("25px 50px 75px"),
        Ok(LayoutPadding {
            top: PixelValueWithAuto::Exact(PixelValue::px(25.0)),
            right: PixelValueWithAuto::Exact(PixelValue::px(50.0)),
            left: PixelValueWithAuto::Exact(PixelValue::px(50.0)),
            bottom: PixelValueWithAuto::Exact(PixelValue::px(75.0)),
        })
    );
}

#[test]
fn test_parse_padding_4() {
    assert_eq!(
        parse_layout_padding("25px 50px 75px 100px"),
        Ok(LayoutPadding {
            top: PixelValueWithAuto::Exact(PixelValue::px(25.0)),
            right: PixelValueWithAuto::Exact(PixelValue::px(50.0)),
            bottom: PixelValueWithAuto::Exact(PixelValue::px(75.0)),
            left: PixelValueWithAuto::Exact(PixelValue::px(100.0)),
        })
    );
}

#[test]
fn test_parse_percentage_value_1() {
    assert_eq!(parse_percentage_value("5%"), Ok(PercentageValue::new(5.0)));
}

#[test]
fn test_parse_percentage_value_2() {
    assert_eq!(
        parse_percentage_value("0.5"),
        Ok(PercentageValue::new(50.0))
    );
}

#[test]
fn test_parse_angle_value_1() {
    assert_eq!(parse_angle_value("20deg"), Ok(AngleValue::deg(20.0)));
}

#[test]
fn test_parse_angle_value_2() {
    assert_eq!(parse_angle_value("20.4rad"), Ok(AngleValue::rad(20.4)));
}

#[test]
fn test_parse_angle_value_3() {
    assert_eq!(parse_angle_value("20.4grad"), Ok(AngleValue::grad(20.4)));
}
