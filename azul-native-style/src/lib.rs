//! Provides azul-compatible approximations of OS-native styles.

extern crate azul_css;
use azul_css::Css;

extern crate azul_css_parser;

/// CSS mimicking the OS-native look - Windows: `styles/native_windows.css`
#[cfg(target_os="windows")]
pub const NATIVE_CSS: &str = concat!(
    include_str!("styles/native_windows.css"),
    include_str!("styles/shared/table.css"),
);

/// CSS mimicking the OS-native look - Linux: `styles/native_linux.css`
#[cfg(target_os="linux")]
pub const NATIVE_CSS: &str = concat!(
    include_str!("styles/native_linux.css"),
    include_str!("styles/shared/table.css"),
);

/// CSS mimicking the OS-native look - Mac: `styles/native_macos.css`
#[cfg(target_os="macos")]
pub const NATIVE_CSS: &str = concat!(
    include_str!("styles/native_macos.css"),
    include_str!("styles/shared/table.css"),
);

/// Returns the native style for the OS
pub fn native() -> Css {
    azul_css_parser::new_from_str(NATIVE_CSS).unwrap()
}
