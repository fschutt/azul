//! Desktop implementation of the Azul GUI toolkit

#![allow(dead_code)]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(missing_copy_implementations)]
#![deny(clippy::all)]
#![allow(warnings)]

/// Manages application state (`App` / `AppState` / `AppResources`), wrapping resources and app
/// state
pub mod app;
/// Windowing backend for the platforms window manager (Win32, NSView, X11, Wayland)
pub mod shell;
pub use azul_core::{callbacks, task};
/// CSS type definitions / CSS parsing functions
#[cfg(any(feature = "css_parser", feature = "native_style"))]
pub mod css;
/// Bindings to the native file-chooser, color picker, etc. dialogs
pub mod dialogs;
/// Extra functions for file IO (for C / C++ developers)
pub mod file;
pub use azul_core::{dom, gl, style, styled_dom};
/// Font & image resource handling, lookup and caching
pub mod resources {
    pub use azul_core::resources::*;
    pub use azul_layout::{font::*, image::*};
}


mod compositor;
#[cfg(feature = "logging")]
mod logging;
mod wr_translate;

/// `GetTextLayout` trait definition
pub mod traits {
    pub use azul_core::traits::GetTextLayout;
}

/// Handles text layout (modularized, can be used as a standalone module)
pub mod text_layout {
    pub use azul_layout::text3::*;
}

/// SVG parsing + rendering
pub mod svg {
    pub use azul_layout::xml::svg::*;
}

/// XML parsing
pub mod xml {
    pub use azul_layout::xml::*;
}

/// Re-exports of errors
pub mod errors {
    // TODO: re-export the sub-types of ClipboardError!
    #[cfg(all(feature = "font_loading", feature = "std"))]
    pub use azul_layout::font::loading::FontReloadError;
    pub use clipboard2::ClipboardError;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreText", kind = "framework")]
fn __macos() {}
