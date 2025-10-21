/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, you can obtain one at http://mozilla.org/MPL/2.0/. */

//! A pure-Rust glyph rasterizer for WebRender.
//!
//! This crate serves as an API-compatible, drop-in replacement for the official
//! `wr_glyph_rasterizer`. It is designed to eliminate all C dependencies (such as
//! FreeType, CoreText, and DirectWrite) from the text rendering pipeline.
//!
//! ## Core Technologies
//!
//! - **Font Parsing**: Uses `azul-layout`'s fork of `allsorts` to parse font files
//!   and extract vector glyph outlines.
//! - **Rasterization**: Uses the `tiny-skia` 2D graphics library to render the
//!   vector outlines into anti-aliased bitmap masks.
//!
//! ## Goal
//!
//! The primary motivation is to enable WebRender-based applications, like Azul,
//! to compile and run seamlessly in pure-Rust environments such as **Redox OS**
//! and **wasm32**, where C libraries are often unavailable or difficult to integrate.
//!
//! ## Features and Limitations
//!
//! - **Pure Rust**: Contains no C code and requires no system font libraries.
//! - **API Compatible**: Aims to match the public API of `wr_glyph_rasterizer`
//!   to ensure it can be used as a direct dependency replacement.
//! - **Grayscale AA Only**: The current implementation produces high-quality grayscale
//!   anti-aliased glyphs (alpha masks). It does not support platform-specific
//!   subpixel anti-aliasing (e.g., ClearType) or advanced hinting that relies
//!   on native OS libraries. The `gamma_lut` module is included for API compatibility
//!   but its pre-blending logic is not utilized by the rasterization backend.

pub mod font;
pub mod rasterizer;
pub mod types;

pub use font::*;
pub use rasterizer::*;
pub use types::*;

// Re-exports for compatibility with WebRender core
use std::sync::atomic::AtomicBool;

/// Debug flag for glyph flashing (compatibility stub)
pub static GLYPH_FLASHING: AtomicBool = AtomicBool::new(false);

/// Maximum font size that can be rasterized
pub const FONT_SIZE_LIMIT: f32 = 320.0;

/// Profiler module (compatibility stub)
pub mod profiler {
    pub trait GlyphRasterizeProfiler: Send {
        fn start_time(&mut self, _label: &str) {}
        fn end_time(&mut self, _label: &str) {}
    }
    
    // Empty implementation for when profiling is not needed
    impl GlyphRasterizeProfiler for () {}
}

/// Shared font resources (compatibility type alias)
pub type SharedFontResources = ();

/// Glyph raster thread (compatibility type alias) 
pub type GlyphRasterThread = ();
