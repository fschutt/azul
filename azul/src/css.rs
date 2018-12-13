//! Provides convenience wrappers around some of azul's helper crates, when the appropriate
//! features are enabled.

/// Returns a style with the native appearance for the operating system. Convenience wrapper
/// for functionality from the the `azul-native-style` crate.
#[cfg(feature = "native_style")]
pub fn native() -> azul_css::Css {
    azul_native_style::native()
}

/// Parses CSS from a string. Convenience wrapper for functionality from the `azul-css-parser`
/// crate.
#[cfg(feature = "css_parser")]
pub fn from_str(input: &str) -> Result<azul_css::Css, azul_css_parser::CssParseError> {
    azul_css_parser::new_from_str(input)
}

/// Allows dynamic reloading of a CSS file during an application's runtime; useful for
/// iterating over multiple styles without recompiling every time.
///
/// Setting `override_native` to `true` will cause reloaded styles to be applied on top of the
/// native appearance for the operating system.
#[cfg(all(debug_assertions, feature = "css_parser", feature = "native_style"))]
pub fn hot_reload(file_path: &str, override_native: bool) -> Box<dyn azul_css::HotReloadHandler> {
    let file_path = file_path.to_owned();
    let hot_reloader = azul_css_parser::HotReloader::new(file_path);
    if override_native {
        azul_css::HotReloadOverride::new(azul_native_style::native(), hot_reloader)
    } else {
        hot_reloader
    }
}

/// Type translation functions (from azul-css to webrender types)
///
/// The reason for doing this is so that azul-css doesn't depend on webrender or euclid
/// (since webrender is a huge dependency) just to use the types. Only if you depend on
/// azul, you have to depend on webrender.
pub(crate) mod webrender_translate {

    // NOTE: In rustc 1.31, most or all of these functions can be const

    use webrender::api::BoxShadowClipMode as WrBoxShadowClipMode;
    use azul_css::BoxShadowClipMode as CssBoxShadowClipMode;

    #[inline(always)]
    pub fn wr_translate_box_shadow_clip_mode(input: CssBoxShadowClipMode) -> WrBoxShadowClipMode {
        match input {
            CssBoxShadowClipMode::Outset => WrBoxShadowClipMode::Outset,
            CssBoxShadowClipMode::Inset => WrBoxShadowClipMode::Inset,
        }
    }

    use webrender::api::ExtendMode as WrExtendMode;
    use azul_css::ExtendMode as CssExtendMode;

    #[inline(always)]
    pub fn wr_translate_extend_mode(input: CssExtendMode) -> WrExtendMode {
        match input {
            CssExtendMode::Clamp => WrExtendMode::Clamp,
            CssExtendMode::Repeat => WrExtendMode::Repeat,
        }
    }

    use webrender::api::BorderStyle as WrBorderStyle;
    use azul_css::BorderStyle as CssBorderStyle;

    #[inline(always)]
    pub fn wr_translate_border_style(input: CssBorderStyle) -> WrBorderStyle {
        match input {
            CssBorderStyle::None => WrBorderStyle::None,
            CssBorderStyle::Solid => WrBorderStyle::Solid,
            CssBorderStyle::Double => WrBorderStyle::Double,
            CssBorderStyle::Dotted => WrBorderStyle::Dotted,
            CssBorderStyle::Dashed => WrBorderStyle::Dashed,
            CssBorderStyle::Hidden => WrBorderStyle::Hidden,
            CssBorderStyle::Groove => WrBorderStyle::Groove,
            CssBorderStyle::Ridge => WrBorderStyle::Ridge,
            CssBorderStyle::Inset => WrBorderStyle::Inset,
            CssBorderStyle::Outset => WrBorderStyle::Outset,
        }
    }

    use webrender::api::LayoutSideOffsets as WrLayoutSideOffsets;
    use azul_css::LayoutSideOffsets as CssLayoutSideOffsets;

    #[inline(always)]
    pub fn wr_translate_layout_side_offsets(input: CssLayoutSideOffsets) -> WrLayoutSideOffsets {
        WrLayoutSideOffsets::new(
            input.top.get(),
            input.right.get(),
            input.bottom.get(),
            input.left.get(),
        )
    }

    use webrender::api::ColorU as WrColorU;
    use azul_css::ColorU as CssColorU;

    #[inline(always)]
    pub fn wr_translate_color_u(input: CssColorU) -> WrColorU {
        WrColorU { r: input.r, g: input.g, b: input.b, a: input.a }
    }

    use azul_css::BorderRadius as CssBorderRadius;
    use webrender::api::BorderRadius as WrBorderRadius;

    #[inline(always)]
    pub fn wr_translate_border_radius(input: CssBorderRadius) -> WrBorderRadius {
        use webrender::api::LayoutSize;
        let CssBorderRadius { top_left, top_right, bottom_left, bottom_right } = input;
        WrBorderRadius {
            top_left: LayoutSize::new(top_left.width.to_pixels(), top_left.height.to_pixels()),
            top_right: LayoutSize::new(top_right.width.to_pixels(), top_right.height.to_pixels()),
            bottom_left: LayoutSize::new(bottom_left.width.to_pixels(), bottom_left.height.to_pixels()),
            bottom_right: LayoutSize::new(bottom_right.width.to_pixels(), bottom_right.height.to_pixels()),
        }
    }

    use azul_css::BorderSide as CssBorderSide;
    use webrender::api::BorderSide as WrBorderSide;

    #[inline(always)]
    pub fn wr_translate_border_side(input: CssBorderSide) -> WrBorderSide {
        WrBorderSide {
            color: wr_translate_color_u(input.color).into(),
            style: wr_translate_border_style(input.style),
        }
    }

    use azul_css::NormalBorder as CssNormalBorder;
    use webrender::api::NormalBorder as WrNormalBorder;

    #[inline(always)]
    pub fn wr_translate_normal_border(input: CssNormalBorder) -> WrNormalBorder {
        WrNormalBorder {
            left: wr_translate_border_side(input.left),
            right: wr_translate_border_side(input.right),
            top: wr_translate_border_side(input.top),
            bottom: wr_translate_border_side(input.bottom),
            radius: wr_translate_border_radius(input.radius),
            do_aa: input.do_aa,
        }
    }

    use azul_css::LayoutPoint as CssLayoutPoint;
    use webrender::api::LayoutPoint as WrLayoutPoint;

    #[inline(always)]
    pub fn wr_translate_layout_point(input: CssLayoutPoint) -> WrLayoutPoint {
        WrLayoutPoint::new(input.x, input.y)
    }

    use azul_css::LayoutRect as CssLayoutRect;
    use azul_css::LayoutSize as CssLayoutSize;
    use webrender::api::LayoutRect as WrLayoutRect;

    // NOTE: Reverse direction: Translate from webrender::LayoutRect to css::LayoutRect
    #[inline(always)]
    pub fn wr_translate_layout_rect(input: WrLayoutRect) -> CssLayoutRect {
        CssLayoutRect {
            origin: CssLayoutPoint { x: input.origin.x, y: input.origin.y },
            size: CssLayoutSize { width: input.size.width, height: input.size.height },
        }
    }

    use azul_css::BorderDetails as CssBorderDetails;
    use webrender::api::BorderDetails as WrBorderDetails;

    // NOTE: Reverse direction: Translate from webrender::LayoutRect to css::LayoutRect
    #[inline(always)]
    pub fn wr_translate_border_details(input: CssBorderDetails) -> WrBorderDetails {
        let zero_border_side = WrBorderSide { color: WrColorU { r: 0, g: 0, b: 0, a: 0 }.into(), style: WrBorderStyle::None };
        match input {
            CssBorderDetails::Normal(normal) => WrBorderDetails::Normal(wr_translate_normal_border(normal)),
            // TODO: Do 9patch border properly - currently this can't be reached since there is no parsing for 9patch border yet!
            CssBorderDetails::NinePatch(_) => WrBorderDetails::Normal(WrNormalBorder {
                left: zero_border_side,
                right: zero_border_side,
                bottom: zero_border_side,
                top: zero_border_side,
                radius: WrBorderRadius::zero(),
                do_aa: false,
            })
        }
    }
}
