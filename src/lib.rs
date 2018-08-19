//! azul is a library for creating graphical user interfaces in Rust.
//!
//! ## How it works
//!
//! azul requires your app data to "serialize" itself into a UI.
//! This is different from how other GUI frameworks work, so it requires a bit of explanation:
//!
//! Your app data is one global struct for your whole application. This is the "model".
//! azul takes your model and requires you to build a DOM tree to translate the model into a view.
//! This (layouting, restyling, constraint solving) is done every 2 milliseconds. However, if your
//! UI doesn't change, nothing is done (in order to not stress the CPU too much).
//!
//! This model makes conditional UI elements and conditional styling very easy. azul takes care
//! of caching for you - your CSS and DOM elements are cached and diffed for changes, in order to
//! maximize performance. A full screen redraw should not take longer than 16 milliseconds
//! (currently the frame time is around 1 - 2 milliseconds).
//!
//! ## Hello world example
//!
//! For more examples, please look in the `/examples` folder.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

#![deny(unused_must_use)]
#![deny(missing_copy_implementations)]
#![allow(dead_code)]

#![windows_subsystem = "windows"]

#[macro_use]
pub extern crate glium;
pub extern crate gleam;

#[macro_use]
extern crate lazy_static;
extern crate euclid;
#[cfg(feature = "svg")]
extern crate lyon;
#[cfg(feature = "svg")]
extern crate usvg;
extern crate webrender;
extern crate cassowary;
extern crate twox_hash;
extern crate simplecss;
extern crate rusttype;
extern crate app_units;
extern crate unicode_normalization;
extern crate tinyfiledialogs;
extern crate stb_truetype;
extern crate clipboard2;
extern crate font_loader;
#[macro_use]
extern crate log;
#[cfg(feature = "logging")]
extern crate fern;
#[cfg(feature = "logging")]
extern crate backtrace;
extern crate image;

#[cfg(not(target_os = "linux"))]
extern crate nfd;

/// DOM / HTML node handling
pub mod dom;
/// Bindings to the native file-chooser, color picker, etc. dialogs
pub mod dialogs;
/// Async IO / task system
pub mod task;
/// Module for caching long texts (including their layout / character positions) across multiple frames
pub mod text_cache;
/// Text layout helper functions - useful for text layout outside of standard containers
pub mod text_layout;
/// The layout traits for creating a layout-able application
pub mod traits;
/// Built-in widgets
pub mod widgets;
/// Window handling
pub mod window;
/// Global application (Initialization starts here)
mod app;
/// Wrapper for the application data & application state
mod app_state;
/// Styling & CSS parsing
mod css;
/// Font & image resource handling, lookup and caching
mod resources;
/// UI Description & display list handling (webrender)
mod ui_description;
/// Constraint handling
mod constraints;
/// Converts the UI description (the styled HTML nodes)
/// to an actual display list (+ layout)
mod display_list;
/// CSS parser
mod css_parser;
/// Slab allocator for nodes, based on IDs (replaces kuchiki + markup5ever)
mod id_tree;
/// State handling for user interfaces
mod ui_state;
/// Dom / CSS caching
mod cache;
/// Image handling
mod images;
/// Font handling
mod font;
/// Window state handling, event filtering
mod window_state;
/// Application / context menu handling. Currently Win32 only. Also has parsing functions
mod menu;
/// The compositor takes all textures (user-defined + the UI texture(s)) and draws them on
/// top of each other
mod compositor;
// /// Platform extensions (non-portable window extensions for Win32, Wayland, X11, Cocoa)
// mod platform_ext;
/// Default logger, can be turned off with `feature = "logging"`
#[cfg(feature = "logging")]
mod logging;

/// Faster implementation of a HashMap
type FastHashMap<T, U> = ::std::collections::HashMap<T, U, ::std::hash::BuildHasherDefault<::twox_hash::XxHash>>;
type FastHashSet<T> = ::std::collections::HashSet<T, ::std::hash::BuildHasherDefault<::twox_hash::XxHash>>;

/// Quick exports of common types
pub mod prelude {
    pub use app::{App, AppConfig};
    pub use app_state::AppState;
    pub use css::{Css, FakeCss};
    pub use dom::{Dom, NodeType, NodeData, Callback, On, UpdateScreen};
    pub use traits::{Layout, ModifyAppState};
    pub use window::{MonitorIter, Window, WindowCreateOptions, WindowId,
                     MouseMode, UpdateBehaviour, UpdateMode,
                     WindowMonitorTarget, RendererType, WindowEvent, WindowInfo, ReadOnlyWindow};
    pub use window_state::WindowState;
    pub use images::ImageType;
    pub use text_cache::{TextCache, TextId};
    pub use css_parser::{
        ParsedCssProperty, BorderRadius, BackgroundColor, TextColor,
        BorderWidths, BorderDetails, Background, FontSize,
        FontFamily, TextOverflowBehaviour, TextOverflowBehaviourInner, TextAlignmentHorz,
        BoxShadowPreDisplayItem, LayoutWidth, LayoutHeight,
        LayoutMinWidth, LayoutMinHeight, LayoutMaxWidth,
        LayoutMaxHeight, LayoutWrap, LayoutDirection,
        LayoutJustifyContent, LayoutAlignItems, LayoutAlignContent,
        LinearGradientPreInfo, RadialGradientPreInfo, CssImageId, FontId, CssColor,

        LayoutPixel, TypedSize2D, BoxShadowClipMode, ColorU, ColorF, LayoutVector2D,
        Gradient, SideOffsets2D, RadialGradient, LayoutPoint, LayoutSize,
        ExtendMode, PixelValue, PercentageValue,
    };
    pub use glium::glutin::{
        dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize},
        VirtualKeyCode, ScanCode,
    };
    pub use rusttype::Font;
    pub use resources::AppResources;
    pub use task::TerminateDaemon;

    #[cfg(feature = "logging")]
    pub use log::LevelFilter;
}

/// Re-exports of errors
pub mod errors {
    pub use css_parser::{
        CssParsingError, CssBorderParseError, CssShadowParseError, InvalidValueErr,
        PixelParseError, CssImageParseError, CssFontFamilyParseError, CssMetric,
        PercentageParseError,
        CssBackgroundParseError, CssColorParseError, CssBorderRadiusParseError,
        CssDirectionParseError, CssGradientStopParseError, CssShapeParseError,
    };
    pub use simplecss::Error as CssSyntaxError;
    pub use css::{CssParseError, DynamicCssParseError};
    pub use font::FontError;
    pub use image::ImageError;

    // TODO: re-export the sub-types of ClipboardError!
    pub use clipboard2::ClipboardError;

    pub use window::WindowCreateError;
    pub use widgets::errors::*;
}
