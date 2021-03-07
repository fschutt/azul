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

extern crate strfmt;
#[macro_use]
extern crate azul_css;
extern crate rust_fontconfig;
#[macro_use(impl_from, impl_display)]
extern crate azul_core;
extern crate azulc_lib;
extern crate raw_window_handle;
extern crate glutin;
extern crate webrender;
extern crate tinyfiledialogs;
extern crate clipboard2;
extern crate gleam;
#[cfg(feature = "css_parser")]
extern crate azul_css_parser;
#[cfg(feature = "logging")]
#[cfg_attr(feature = "logging", macro_use(error, warn))]
extern crate log;
#[cfg(feature = "logging")]
extern crate fern;
#[cfg(feature = "logging")]
extern crate backtrace;
#[cfg(target_os = "macos")]
extern crate core_foundation;

/// Manages application state (`App` / `AppState` / `AppResources`), wrapping resources and app state
pub mod app;
pub use azul_core::task;
pub use azul_core::callbacks;
/// CSS type definitions / CSS parsing functions
#[cfg(any(feature = "css_parser", feature = "native_style"))]
pub mod css;
/// Bindings to the native file-chooser, color picker, etc. dialogs
pub mod dialogs;
pub use azul_core::dom;
pub use azul_core::gl;
pub use azul_core::styled_dom;
pub use azul_core::style;
/// Window state handling and window-related information
pub mod window;
/// Font & image resource handling, lookup and caching
pub mod resources {
    pub use azul_core::app_resources::*;
    pub use azulc_lib::image::*;
    pub use azulc_lib::font::*;
}
mod compositor;
#[cfg(feature = "logging")]
mod logging;
mod wr_translate;
mod display_shader;

/// `GetTextLayout` trait definition
pub mod traits {
    pub use azul_core::traits::GetTextLayout;
}

/// Handles text layout (modularized, can be used as a standalone module)
pub mod text_layout {
    pub use azulc_lib::layout::text_layout::text_layout::*;
    pub use azulc_lib::layout::text_layout::text_shaping::*;
    pub use azulc_lib::layout::text_layout::InlineText;
}

/// SVG parsing + rendering
pub mod svg {
    pub use azul_core::svg::*;
    pub use azulc_lib::svg::*;
}

/// XML parsing
pub mod xml {
    pub use azulc_lib::xml::*;
}

/// Quick exports of common types
pub mod prelude {
    pub use azul_css::*;
    pub use azul_core::{
        ui_solver::{TextLayoutOptions, ResolvedTextLayoutOptions},
        window::{
            WindowCreateOptions, RendererType,
            LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize,
            WindowState, KeyboardState, MouseState, DebugState, AcceleratorKey,
            VirtualKeyCode, ScanCode,
        },
        display_list::GlyphInstance,
        app_resources::{
            AppResources, RawImageFormat, ImageId, FontId,
            FontSource, ImageSource, AppConfig,
        },
        styled_dom::StyledDom,
        callbacks::*,
        gl::{
            GLuint, Texture, VertexLayout, VertexAttribute, VertexAttributeType,
            VertexLayoutDescription, VertexBuffer, GlApiVersion, IndexBufferFormat,
            Uniform, UniformType, GlShader, VertexShaderCompileError,
            FragmentShaderCompileError, GlShaderLinkError, GlShaderCreateError,
        },
        dom::{
            Dom, DomHash, NodeType, NodeData, On, TabIndex,
            EventFilter, HoverEventFilter, FocusEventFilter, NotEventFilter, WindowEventFilter,
        },
        task::{TimerId, Timer, TerminateTimer, ThreadId, Thread, ThreadSender, ThreadReceiver, ThreadSendMsg, ThreadReceiveMsg, ThreadWriteBackMsg},
        traits::*,
    };
    pub use crate::app::App;
    pub use crate::window::{Window, Monitor};
    #[cfg(any(feature = "css_parser", feature = "native_style"))]
    pub use crate::css;
    #[cfg(feature = "logging")]
    pub use log::LevelFilter;
}

/// Re-exports of errors
pub mod errors {
    // TODO: re-export the sub-types of ClipboardError!
    pub use clipboard2::ClipboardError;
    pub use glutin::CreationError;
    pub use azulc_lib::image_loading::ImageReloadError;
    pub use azulc_lib::font_loading::FontReloadError;
    #[derive(Debug)]
    pub enum Error {
        Resource(ResourceReloadError),
        Clipboard(ClipboardError),
        WindowCreate(CreationError),
    }

    impl_from!(ResourceReloadError, Error::Resource);
    impl_from!(ClipboardError, Error::Clipboard);
    impl_from!(CreationError, Error::WindowCreate);

    #[derive(Debug)]
    pub enum ResourceReloadError {
        Image(ImageReloadError),
        Font(FontReloadError),
    }

    impl_from!(ImageReloadError, ResourceReloadError::Image);
    impl_from!(FontReloadError, ResourceReloadError::Font);

    impl_display!(ResourceReloadError, {
        Image(e) => format!("Failed to load image: {}", e),
        Font(e) => format!("Failed to load font: {}", e),
    });

    impl_display!(Error, {
        Resource(e) => format!("{}", e),
        Clipboard(e) => format!("Clipboard error: {}", e),
        WindowCreate(e) => format!("Window creation error: {}", e),
    });
}
