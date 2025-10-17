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

mod rasterizer;
mod types;
mod gamma_lut;
pub mod profiler;

pub use rasterizer::*;
pub use types::*;
pub use gamma_lut::*;

#[macro_use]
extern crate malloc_size_of_derive;

/// The platform module contains the font rasterization backend.
/// In this crate, there is only one backend: the pure-Rust "azul" implementation.
pub mod platform {
    /// The `azul` module provides the font context using `azul-layout` and `tiny-skia`.
    pub mod azul {
        pub mod font;
    }
    /// Re-export the `azul` backend as the default `font` module for API compatibility.
    pub use crate::platform::azul::font;
}