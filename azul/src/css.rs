//! Provides convenience wrappers around some of azul's helper crates, when the appropriate
//! features are enabled.

/// Returns a style with the native appearance for the operating system. Convenience wrapper
/// for functionality from the the `azul-native-style` crate.
#[cfg(feature = "native_style")]
pub fn native() -> azul_style::AppStyle {
    azul_native_style::native()
}

/// Parses CSS from a string. Convenience wrapper for functionality from the `azul-css-parser`
/// crate.
#[cfg(feature = "css_parser")]
pub fn from_str(input: &str) -> Result<azul_style::AppStyle, azul_css_parser::CssParseError> {
    azul_css_parser::new_from_str(input)
}

/// Allows dynamic reloading of a CSS file during an application's runtime; useful for
/// iterating over multiple styles without recompiling every time.
///
/// Setting `override_native` to `true` will cause reloaded styles to be applied on top of the
/// native appearance for the operating system.
#[cfg(all(debug_assertions, feature = "css_parser", feature = "native_style"))]
pub fn hot_reload(file_path: &str, override_native: bool) -> Box<dyn azul_style::HotReloadHandler> {
    let file_path = file_path.to_owned();
    let hot_reloader = azul_css_parser::HotReloader::new(file_path);
    if override_native {
        azul_style::HotReloadOverride::new(azul_native_style::native(), hot_reloader)
    } else {
        hot_reloader
    }
}
