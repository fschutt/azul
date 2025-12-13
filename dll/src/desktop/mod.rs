//! Desktop implementation of the Azul GUI toolkit.
//!
//! Cross-platform windowing for Windows, macOS, X11, and Wayland.
//!
//! # Event Pipeline
//!
//! ```text
//! OS Event → poll_event() → State Diffing → dispatch_events() → Callbacks → Layout → Redraw
//! ```
//!
//! # State Management
//!
//! Windows track `previous_window_state` and `current_window_state`. Events are detected
//! by diffing states via `create_events_from_states()`, not by tracking individual events.
//!
//! # Platform Details
//!
//! | Platform | Event Model | CSD | Menus |
//! |----------|-------------|-----|-------|
//! | Windows  | WndProc message pump | Optional | Native HMENU |
//! | macOS    | NSApplication loop | Optional | Native NSMenu |
//! | X11      | XNextEvent polling | Optional | CSD windows |
//! | Wayland  | Protocol listeners | Mandatory | CSD windows |
//!
//! # Key Types
//!
//! - `ProcessEventResult`: DoNothing / RequestRedraw / RegenerateAndRedraw
//! - `CsdAction`: TitlebarDrag / Minimize / Maximize / Close
//! - `ScrollbarDragState`: Tracks scrollbar dragging across events

#![allow(dead_code)]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(missing_copy_implementations)]
#![deny(clippy::all)]
#![allow(warnings)]

/// Clipboard error type
pub mod clipboard_error;

/// Manages application state (`App` / `AppState` / `AppResources`), wrapping resources and app
/// state
pub mod app;
/// New compositor integration for shell2 - WebRender bridge
pub mod compositor2;
/// Extensions for LayoutCallbackInfo to support SystemStyle
/// Client-Side Decorations (CSD) - Custom window titlebar
pub mod csd;
/// CSS type definitions / CSS parsing functions
#[cfg(any(feature = "css_parser", feature = "native_style"))]
pub mod css;
/// Bindings to the native file-chooser, color picker, etc. dialogs
pub mod dialogs;
/// Display/Monitor management for menu positioning
pub mod display;
/// Extra functions for file IO (for C / C++ developers)
pub mod file {
    pub use azul_layout::desktop::file::*;
}
/// OpenGL texture cache for external image support
pub mod gl_texture_cache;
/// Integration layer for OpenGL texture management
pub mod gl_texture_integration;
#[cfg(feature = "logging")]
mod logging;
/// Unified menu system using window-based approach
pub mod menu;
/// Menu rendering - Converts Menu structures to StyledDom
pub mod menu_renderer;
/// New windowing backend (shell2) - modern, clean architecture
pub mod shell2;
/// WebRender type translations and hit-testing for shell2
pub mod wr_translate2;
/// Font & image resource handling, lookup and caching
pub mod resources {
    pub use azul_core::resources::*;
    pub use azul_layout::{font::*, image::*};
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

    pub use crate::desktop::clipboard_error::ClipboardError;
}

pub use azul_core::{callbacks, dom, gl, style, styled_dom, task};

#[cfg(target_os = "macos")]
#[link(name = "CoreText", kind = "framework")]
fn __macos() {}
