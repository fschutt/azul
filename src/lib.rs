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


#![deny(unused_must_use)]
#![deny(missing_copy_implementations)]
#![allow(dead_code)]
#![allow(unused_imports)]

#![windows_subsystem = "windows"]

#[macro_use]
pub extern crate glium;
pub extern crate gleam;
pub extern crate image;

#[macro_use]
extern crate lazy_static;
extern crate euclid;
extern crate lyon;
extern crate svg as svg_crate;
extern crate webrender;
extern crate cassowary;
extern crate twox_hash;
extern crate simplecss;
extern crate rusttype;
extern crate app_units;
extern crate unicode_normalization;
extern crate harfbuzz_rs;

/// DOM / HTML node handling
pub mod dom;
/// The layout traits for creating a layout-able application
pub mod traits;
/// Window handling
pub mod window;
/// Async IO / task system
pub mod task;
/// SVG / path flattering module (lyon)
pub mod svg;
/// Built-in widgets
pub mod widgets;
/// Global application (Initialization starts here)
mod app;
/// Wrapper for the application data & application state
mod app_state;
/// Styling & CSS parsing
mod css;
/// Text layout
mod text_layout;
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
/// Platform extensions (non-portable window extensions for Win32, Wayland, X11, Cocoa)
mod platform_ext;
/// Module for caching long texts (including their layout / character positions) across multiple frames
mod text_cache;

/// Faster implementation of a HashMap
type FastHashMap<T, U> = ::std::collections::HashMap<T, U, ::std::hash::BuildHasherDefault<::twox_hash::XxHash>>;
type FastHashSet<T> = ::std::collections::HashSet<T, ::std::hash::BuildHasherDefault<::twox_hash::XxHash>>;

/// Quick exports of common types
pub mod prelude {
    pub use app::App;
    pub use app_state::AppState;
    pub use css::{Css, FakeCss};
    pub use dom::{Dom, NodeType, Callback, CheckboxState, On, UpdateScreen};
    pub use traits::{Layout, ModifyAppState, GetDom};
    pub use window::{MonitorIter, Window, WindowCreateOptions, WindowId,
                     MouseMode, UpdateBehaviour, UpdateMode, WindowCreateError,
                     WindowMonitorTarget, RendererType, WindowEvent, WindowInfo, ReadOnlyWindow};
    pub use window_state::WindowState;
    pub use font::FontError;
    pub use images::ImageType;
    pub use css_parser::{
        ParsedCssProperty, BorderRadius, BackgroundColor, TextColor,
        BorderWidths, BorderDetails, Background, FontSize,
        FontFamily, TextOverflowBehaviour, TextOverflowBehaviourInner, TextAlignmentHorz,
        BoxShadowPreDisplayItem, LayoutWidth, LayoutHeight,
        LayoutMinWidth, LayoutMinHeight, LayoutMaxWidth,
        LayoutMaxHeight, LayoutWrap, LayoutDirection,
        LayoutJustifyContent, LayoutAlignItems, LayoutAlignContent,
        LinearGradientPreInfo, RadialGradientPreInfo, CssImageId,

        LayoutPixel, TypedSize2D, BoxShadowClipMode, ColorU, ColorF, LayoutVector2D,
        Gradient, SideOffsets2D, RadialGradient, LayoutPoint, LayoutSize,
        ExtendMode, PixelValue, PercentageValue,
    };

    pub use svg::{SvgLayerId, SvgLayer};

    // from the extern crate image
    pub use image::ImageError;
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
    pub use svg::SvgParseError;
}
