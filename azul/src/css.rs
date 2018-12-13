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

// Type translation functions (from azul-css to webrender)
//
// The reason for doing this is so that azul-css doesn't depend on webrender or euclid
// (since webrender is a huge dependency) just to use the types. Only if you depend on azul,
// you have to depend on webrender


pub(crate) mod webrender_translate {
    use azul_css::StyleBorderRadius as CssStyleBorderRadius;
    use webrender::api::BorderRadius as WrBorderRadius;
    pub fn wr_translate_border_radius(input: CssStyleBorderRadius) -> WrBorderRadius {
        use webrender::api::LayoutSize;
        let CssStyleBorderRadius { top_left, top_right, bottom_left, bottom_right } = input;
        WrBorderRadius {
            top_left: LayoutSize::new(top_left.x.to_pixels(), top_left.y.to_pixels()),
            top_right: LayoutSize::new(top_right.x.to_pixels(), top_right.y.to_pixels()),
            bottom_left: LayoutSize::new(bottom_left.x.to_pixels(), bottom_left.y.to_pixels()),
            bottom_right: LayoutSize::new(bottom_right.x.to_pixels(), bottom_right.y.to_pixels()),
        }
    }
}
