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

#[cfg(feature = "native_style")]
extern crate azul_native_style;
extern crate azul_css;
#[macro_use(impl_task_api, impl_font_api, impl_image_api, impl_from, impl_display)]
extern crate azul_core;
extern crate azulc;
extern crate glutin;
extern crate webrender;
extern crate app_units;
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

pub use azulc::xml;
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
pub use azul_core::diff;
pub use azul_core::gl;
/// Window state handling and window-related information
pub mod window;
/// Font & image resource handling, lookup and caching
#[path = "./app_resources.rs"]
pub mod resources;
mod compositor;
#[cfg(feature = "logging")]
mod logging;
mod wr_translate;

pub use azul_core::{FastHashMap, FastHashSet};

/// Traits `Layout`, `GetTextLayout` and `GetStyle` definitions
pub mod traits {
    pub use azul_core::traits::*;
    pub use azulc::layout::GetStyle;
}

/// Handles text layout (modularized, can be used as a standalone module)
pub mod text_layout {
    pub use azulc::layout::text_layout::text_layout::*;
    pub use azulc::layout::text_layout::text_shaping::*;
    pub use azulc::layout::text_layout::InlineText;
}

/// SVG parsing + rendering
pub mod svg {
    pub use azul_core::svg::*;
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
        window_state::keymap,
        display_list::GlyphInstance,
        app_resources::{
            AppResources, RawImageFormat, ImageId, FontId,
            FontSource, ImageSource, TextCache, TextId,
        },
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
        task::{Task, TerminateTimer, TimerId, Timer, DropCheck},
        traits::*,
    };
    pub use crate::app::{App, AppConfig};
    pub use crate::window::{Window, MonitorHandle, Monitor};
    pub use crate::xml::{XmlComponent, XmlComponentMap, DomXml};
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
    pub use azulc::image_loading::ImageReloadError;
    pub use azulc::font_loading::FontReloadError;
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
