//!
//! # Supported CSS properties
//!
//! | CSS Key                                            | Value syntax | Description | Example(s) | Parsing function |
//! |----------------------------------------------------|--------------|-------------|------------|------------------|
//! | `border-radius`                                    |              |             |            |                  |
//! | `background`                                       |              |             |            |                  |
//! | `background-color`                                 |              |             |            |                  |
//! | `background-size`                                  |              |             |            |                  |
//! | `background-repeat`                                |              |             |            |                  |
//! | `color`                                            |              |             |            |                  |
//! | `font-size`                                        |              |             |            |                  |
//! | `font-family`                                      |              |             |            |                  |
//! | `text-align`                                       |              |             |            |                  |
//! | `letter-spacing`                                   |              |             |            |                  |
//! | `line-height`                                      |              |             |            |                  |
//! | `word-spacing`                                     |              |             |            |                  |
//! | `tab-width`                                        |              |             |            |                  |
//! | `cursor`                                           |              |             |            |                  |
//! | `width`, `min-width`, `max-width`                  |              |             |            |                  |
//! | `height`, `min-height`, `max-height`               |              |             |            |                  |
//! | `position`                                         |              |             |            |                  |
//! | `top`, `right`, `left`, `bottom`                   |              |             |            |                  |
//! | `flex-wrap`                                        |              |             |            |                  |
//! | `flex-direction`                                   |              |             |            |                  |
//! | `flex-grow`                                        |              |             |            |                  |
//! | `flex-shrink`                                      |              |             |            |                  |
//! | `justify-content`                                  |              |             |            |                  |
//! | `align-items`                                      |              |             |            |                  |
//! | `align-content`                                    |              |             |            |                  |
//! | `overflow`, `overflow-x`, `overflow-y`             |              |             |            |                  |
//! | `padding`, `-top`, `-left`, `-right`, `-bottom`    |              |             |            |                  |
//! | `margin`,  `-top`, `-left`, `-right`, `-bottom`    |              |             |            |                  |
//! | `border`,  `-top`, `-left`, `-right`, `-bottom`    |              |             |            |                  |
//! | `box-shadow`, `-top`, `-left`, `-right`, `-bottom` |              |             |            |                  |

#[cfg(debug_assertions)]
use std::time::Duration;
#[cfg(debug_assertions)]
use std::path::PathBuf;

pub use azul_css::*;
#[cfg(feature = "css_parser")]
pub mod css_parser {
    pub use azul_css_parser::*;
}

#[cfg(feature = "css_parser")]
pub use azul_css_parser::CssColor;

#[cfg(feature = "native_style")]
pub mod native_style {
    pub use azul_native_style::*;
}

#[cfg(feature = "css_parser")]
use azul_css_parser::{self, CssParseError};

/// Returns a style with the native appearance for the operating system. Convenience wrapper
/// for functionality from the the `azul-native-style` crate.
#[cfg(feature = "native_style")]
pub fn native() -> Css {
    azul_native_style::native()
}

/// Parses CSS stylesheet from a string. Convenience wrapper for `azul-css-parser::new_from_str`.
#[cfg(feature = "css_parser")]
pub fn from_str(input: &str) -> Result<Css, CssParseError> {
    azul_css_parser::new_from_str(input)
}

/// Appends a custom stylesheet to `css::native()`.
#[cfg(all(feature = "css_parser", feature = "native_style"))]
pub fn override_native(input: &str) -> Result<Css, CssParseError> {
    let mut css = native();
    css.append(from_str(input)?);
    Ok(css)
}

/// Allows dynamic reloading of a CSS file during an applications runtime, useful for
/// changing the look & feel while the application is running.
#[cfg(all(debug_assertions, feature = "css_parser"))]
pub fn hot_reload<P: Into<PathBuf>>(file_path: P, reload_interval: Duration) -> Box<dyn HotReloadHandler> {
    Box::new(azul_css_parser::HotReloader::new(file_path).with_reload_interval(reload_interval))
}

/// Same as `Self::hot_reload`, but appends the given file to the
/// `Self::native()` style before the hot-reloaded styles, similar to `override_native`.
#[cfg(all(debug_assertions, feature = "css_parser", feature = "native_style"))]
pub fn hot_reload_override_native<P: Into<PathBuf>>(file_path: P, reload_interval: Duration) -> Box<dyn HotReloadHandler> {
    Box::new(HotReloadOverrideHandler::new(native(), hot_reload(file_path, reload_interval)))
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

    use webrender::api::ColorF as WrColorF;
    use azul_css::ColorF as CssColorF;

    #[inline(always)]
    pub fn wr_translate_color_f(input: CssColorF) -> WrColorF {
        WrColorF { r: input.r, g: input.g, b: input.b, a: input.a }
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

        // Webrender crashes if anti-aliasing is disabled and the border isn't pure-solid
        let is_not_solid = [input.top.style, input.bottom.style, input.left.style, input.right.style].iter().any(|style| {
            *style != CssBorderStyle::Solid
        });
        let do_aa = input.radius.is_some() || is_not_solid;

        WrNormalBorder {
            left: wr_translate_border_side(input.left),
            right: wr_translate_border_side(input.right),
            top: wr_translate_border_side(input.top),
            bottom: wr_translate_border_side(input.bottom),
            radius: wr_translate_border_radius(input.radius.unwrap_or_default()),
            do_aa,
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
        let zero_border_side = WrBorderSide {
            color: WrColorU { r: 0, g: 0, b: 0, a: 0 }.into(),
            style: WrBorderStyle::None
        };

        match input {
            CssBorderDetails::Normal(normal) => WrBorderDetails::Normal(wr_translate_normal_border(normal)),
            // TODO: Do 9patch border properly - currently this can't be reached since there
            // is no parsing for 9patch border yet!
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

    use azul_css::StyleCursor as CssCursor;
    use glium::glutin::MouseCursor as WinitCursor;

    #[inline(always)]
    pub fn winit_translate_cursor(input: CssCursor) -> WinitCursor {
        match input {
            CssCursor::Alias             => WinitCursor::Alias,
            CssCursor::AllScroll         => WinitCursor::AllScroll,
            CssCursor::Cell              => WinitCursor::Cell,
            CssCursor::ColResize         => WinitCursor::ColResize,
            CssCursor::ContextMenu       => WinitCursor::ContextMenu,
            CssCursor::Copy              => WinitCursor::Copy,
            CssCursor::Crosshair         => WinitCursor::Crosshair,
            CssCursor::Default           => WinitCursor::Arrow,         /* note: default -> arrow */
            CssCursor::EResize           => WinitCursor::EResize,
            CssCursor::EwResize          => WinitCursor::EwResize,
            CssCursor::Grab              => WinitCursor::Grab,
            CssCursor::Grabbing          => WinitCursor::Grabbing,
            CssCursor::Help              => WinitCursor::Help,
            CssCursor::Move              => WinitCursor::Move,
            CssCursor::NResize           => WinitCursor::NResize,
            CssCursor::NsResize          => WinitCursor::NsResize,
            CssCursor::NeswResize        => WinitCursor::NeswResize,
            CssCursor::NwseResize        => WinitCursor::NwseResize,
            CssCursor::Pointer           => WinitCursor::Hand,          /* note: pointer -> hand */
            CssCursor::Progress          => WinitCursor::Progress,
            CssCursor::RowResize         => WinitCursor::RowResize,
            CssCursor::SResize           => WinitCursor::SResize,
            CssCursor::SeResize          => WinitCursor::SeResize,
            CssCursor::Text              => WinitCursor::Text,
            CssCursor::Unset             => WinitCursor::Arrow,         /* note: pointer -> hand */
            CssCursor::VerticalText      => WinitCursor::VerticalText,
            CssCursor::WResize           => WinitCursor::WResize,
            CssCursor::Wait              => WinitCursor::Wait,
            CssCursor::ZoomIn            => WinitCursor::ZoomIn,
            CssCursor::ZoomOut           => WinitCursor::ZoomOut,
        }
    }
}
