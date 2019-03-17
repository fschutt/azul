//! Azul is a free, functional, IMGUI-oriented GUI framework for rapid prototyping
//! of desktop applications written in Rust, supported by the Mozilla WebRender rendering
//! engine, using a CSS / DOM model for layout and styling.
//!
//! ## Concepts
//!
//! Azul is largely based on the IMGUI principle, in that you redraw the entire
//! screen every frame. To not make this too performance intensive, Azul provides
//! diffing and caching, as well as efficient callback handling and hit-testing.
//!
//! Managing your code can be done by creating "widgets", i.e. reusable components
//! that can register "default callbacks", for example a checkbox that toggles a
//! certain field if it is checked.
//!
//! Azul also has a standard library of widgets to use, see the [widgets] module.
//! Further, it provides a library for CSS parsing and handling (which takes care
//! of the layouting part) as well as DOM handling.
//!
//! ## Documentation
//!
//! Explaining all concepts and examples is too much material to be included in
//! this API reference. Please refer to the [wiki](https://github.com/maps4print/azul/wiki)
//! or use the links below to learn about how to use Azul.
//!
//! - [Getting Started](https://github.com/maps4print/azul/wiki/Getting-Started)
//! - [A simple counter](https://github.com/maps4print/azul/wiki/A-simple-counter)
//! - [Styling your app with CSS](https://github.com/maps4print/azul/wiki/Styling-your-application-with-CSS)
//! - [SVG drawing](https://github.com/maps4print/azul/wiki/SVG-drawing)
//! - [OpenGL drawing](https://github.com/maps4print/azul/wiki/OpenGL-drawing)
//! - [Timers, timers, tasks and async IO](https://github.com/maps4print/azul/wiki/Timers,-timers,-tasks-and-async-IO)
//! - [Two-way data binding](https://github.com/maps4print/azul/wiki/Two-way-data-binding)
//! - [Unit testing](https://github.com/maps4print/azul/wiki/Unit-testing)
//!
//! ## Hello world
//!
//! Note: Can currently not be tested on CI, since it opens a graphical window.
//!
//! ```no_run
//! extern crate azul;
//!
//! use azul::prelude::*;
//!
//! struct MyDataModel { }
//!
//! impl Layout for MyDataModel {
//!     fn layout(&self, _: LayoutInfo<Self>) -> Dom<Self> {
//!         Dom::new(NodeType::Div)
//!     }
//! }
//!
//! fn main() {
//!     let mut app = App::new(MyDataModel { }, AppConfig::default()).unwrap();
//!     let window = app.create_window(WindowCreateOptions::default(), css::native()).unwrap();
//!     app.run(window).unwrap();
//! }
//! ```
//!
//! If you run this code, you should get a window like this:
//!
//! ![Opening a blank window](https://raw.githubusercontent.com/maps4print/azul/master/doc/azul_tutorial_empty_window.png)
//!

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(missing_copy_implementations)]
#![allow(dead_code)]

#[macro_use(warn, error, lazy_static)]
#[cfg_attr(feature = "svg", macro_use(implement_vertex, uniform))]
pub extern crate azul_dependencies;
#[cfg(feature = "serde_serialization")]
#[cfg_attr(feature = "serde_serialization", macro_use)]
extern crate serde;
#[cfg(feature = "serde_serialization")]
#[cfg_attr(feature = "serde_serialization", macro_use)]
extern crate serde_derive;

pub(crate) use azul_dependencies::glium as glium;
pub(crate) use azul_dependencies::gleam as gleam;
pub(crate) use azul_dependencies::euclid;
pub(crate) use azul_dependencies::webrender;
pub(crate) use azul_dependencies::app_units;
pub(crate) use azul_dependencies::unicode_normalization;
pub(crate) use azul_dependencies::tinyfiledialogs;
pub(crate) use azul_dependencies::clipboard2;
pub(crate) use azul_dependencies::font_loader;
pub(crate) use azul_dependencies::xmlparser;
pub(crate) use azul_dependencies::harfbuzz_sys;

#[cfg(feature = "logging")]
pub(crate) use azul_dependencies::log;
#[cfg(feature = "svg")]
pub(crate) use azul_dependencies::stb_truetype;
#[cfg(feature = "logging")]
pub(crate) use azul_dependencies::fern;
#[cfg(feature = "logging")]
pub(crate) use azul_dependencies::backtrace;
#[cfg(feature = "image_loading")]
pub(crate) use azul_dependencies::image;
#[cfg(feature = "svg")]
pub(crate) use azul_dependencies::lyon;
#[cfg(feature = "svg_parsing")]
pub(crate) use azul_dependencies::usvg;
#[cfg(feature = "faster-hashing")]
pub(crate) use azul_dependencies::twox_hash;

#[cfg(feature = "css-parser")]
extern crate azul_css;
extern crate azul_native_style;
extern crate azul_css_parser;

// Crate-internal macros
#[macro_use]
mod macros;

/// Manages application state (`App` / `AppState` / `AppResources`), wrapping resources and app state
pub mod app;
/// Async IO helpers / (`Task` / `Timer` / `Thread`)
pub mod async;
/// Focus tracking / input tracking related functions
pub mod focus;
#[cfg(any(feature = "css-parser", feature = "native-style"))]
pub mod css;
/// XML-based DOM serialization
pub mod xml;
/// Handles default callbacks (such as an automatic text field update) via unsafe code
pub mod default_callbacks;
/// Bindings to the native file-chooser, color picker, etc. dialogs
pub mod dialogs;
/// DOM / HTML node handling
pub mod dom;
/// Re-exports of errors
pub mod error;
/// Module for caching long texts (including their layout / character positions) across multiple frames
pub mod text_cache;
/// Text layout functions - useful for text layout outside of standard containers
pub mod text_layout;
/// The layout traits for creating a layout-able application
pub mod traits;
/// Built-in widgets
pub mod widgets;
/// Window handling
pub mod window;
/// Window state handling, event filtering
pub mod window_state;

/// UI Description & display list handling (webrender)
mod ui_description;
/// HarfBuzz text shaping utilities
mod text_shaping;
/// Converts the UI description (the styled HTML nodes)
/// to an actual display list (+ layout)
mod display_list;
/// Slab allocator for nodes, based on IDs (replaces kuchiki + markup5ever)
mod id_tree;
/// State handling for user interfaces
mod ui_state;
/// The compositor takes all textures (user-defined + the UI texture(s)) and draws them on
/// top of each other
mod compositor;
/// Default logger, can be turned off with `feature = "logging"`
#[cfg(feature = "logging")]
mod logging;
/// Flexbox-based UI solver
mod ui_solver;
/// DOM styling module
mod style;
/// DOM diffing
mod diff;

pub(crate) mod app_resources;
/// Font & image resource handling, lookup and caching
pub mod resources {
    // re-export everything *except* the AppResources (which are exported under the "app" module)
    pub use app_resources::{
        FontId, ImageId, LoadedFont, RawImage, FontReloadError, FontSource, ImageReloadError,
        ImageSource, RawImageFormat, CssFontId, CssImageId,
    };
}

// Faster implementation of a HashMap (optional, disabled by default, turn on with --feature="faster-hashing")

#[cfg(feature = "faster-hashing")]
type FastHashMap<T, U> = ::std::collections::HashMap<T, U, ::std::hash::BuildHasherDefault<::twox_hash::XxHash>>;
#[cfg(feature = "faster-hashing")]
type FastHashSet<T> = ::std::collections::HashSet<T, ::std::hash::BuildHasherDefault<::twox_hash::XxHash>>;
#[cfg(not(feature = "faster-hashing"))]
type FastHashMap<T, U> = ::std::collections::HashMap<T, U>;
#[cfg(not(feature = "faster-hashing"))]
type FastHashSet<T> = ::std::collections::HashSet<T>;

/// Quick exports of common types
pub mod prelude {
    pub use azul_css::ColorU;
    pub use app::{App, AppConfig, AppState};
    pub use dom::{
        Dom, DomHash, NodeType, NodeData, Callback, On, DomString,
        UpdateScreen, Redraw, DontRedraw, Texture, GlTextureCallback,
        IFrameCallback, TabIndex, EventFilter, HoverEventFilter, FocusEventFilter,
        NotEventFilter, WindowEventFilter,
    };
    pub use traits::{Layout, Modify};
    pub use window::{
        MonitorIter, Window, WindowCreateOptions, HidpiAdjustedBounds,
        WindowMonitorTarget, RendererType, CallbackInfo, LayoutInfo, ReadOnlyWindow
    };
    pub use window_state::{WindowState, KeyboardState, MouseState, DebugState, keymap, AcceleratorKey};
    pub use app_resources::{AppResources, RawImageFormat, ImageId, FontId, FontSource, ImageSource};
    pub use text_cache::{TextCache, TextId};
    pub use glium::glutin::{
        dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize},
        VirtualKeyCode, ScanCode, Icon,
    };
    pub use azul_css::*;
    pub use async::{Task, TerminateTimer, TimerId, TimerCallback, Timer};
    pub use default_callbacks::StackCheckedPointer;
    pub use text_layout::{TextLayoutOptions, GlyphInstance};
    pub use xml::{XmlComponent, XmlComponentMap};
    #[cfg(any(feature = "css-parser", feature = "native-style"))]
    pub use css;

    #[cfg(feature = "logging")]
    pub use log::LevelFilter;
}
