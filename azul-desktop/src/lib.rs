//! Desktop implementation of the Azul GUI toolkit

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

#![allow(dead_code)]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(missing_copy_implementations)]
#![deny(clippy::all)]

extern crate core;
#[macro_use]
extern crate alloc;

extern crate libm;

#[cfg(target_os = "windows")]
extern crate winapi;
extern crate gl_context_loader;

#[macro_use]
extern crate azul_css;
#[macro_use(impl_from)]
extern crate azul_core;
#[cfg(feature = "css_parser")]
extern crate azul_css_parser;
#[cfg(feature = "logging")]
#[cfg_attr(feature = "logging", macro_use(error, warn))]
extern crate log;
#[cfg(all(feature = "use_fern_logger"))]
extern crate fern;
#[cfg(feature = "logging")]
extern crate backtrace;
#[cfg(target_os = "macos")]
extern crate core_foundation;

extern crate rust_fontconfig;
extern crate strfmt;
extern crate azul_text_layout;
extern crate azulc_lib;
extern crate raw_window_handle;
extern crate webrender;
extern crate tinyfiledialogs;
extern crate clipboard2;

/// Manages application state (`App` / `AppState` / `AppResources`), wrapping resources and app state
pub mod app;
/// Windowing backend for the platforms window manager (Win32, NSView, X11, Wayland)
pub mod shell;
pub use azul_core::task;
pub use azul_core::callbacks;
/// CSS type definitions / CSS parsing functions
#[cfg(any(feature = "css_parser", feature = "native_style"))]
pub mod css;
/// Extra functions for string handling (for C / C++ developers)
pub mod str;
/// Extra functions for file IO (for C / C++ developers)
pub mod file;
/// Bindings to the native file-chooser, color picker, etc. dialogs
pub mod dialogs;
pub use azul_core::dom;
pub use azul_core::gl;
pub use azul_core::styled_dom;
pub use azul_core::style;
/// Font & image resource handling, lookup and caching
pub mod resources {
    pub use azul_core::app_resources::*;
    pub use azulc_lib::image::*;
    pub use azulc_lib::font::*;
}

pub mod ui_solver {
    pub use azul_core::ui_solver::{
        PositionInfo, OptionPositionInfo,
        PositionInfoInner,
        ResolvedTextLayoutOptions,
        OptionResolvedTextLayoutOptions,
    };
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
    pub use azul_layout::callback_info_shape_text;
    pub use azul_text_layout::*;
    pub use azul_text_layout::text_layout::*;
    pub use azul_text_layout::text_shaping::*;
    pub use azul_text_layout::InlineText;
}

/// SVG parsing + rendering
pub mod svg {
    pub use azulc_lib::svg::*;
}

/// XML parsing
pub mod xml {
    pub use azulc_lib::xml::*;
}

/// Re-exports of errors
pub mod errors {
    // TODO: re-export the sub-types of ClipboardError!
    pub use clipboard2::ClipboardError;
    pub use azulc_lib::font_loading::FontReloadError;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreText", kind = "framework")]
fn __macos() {}
