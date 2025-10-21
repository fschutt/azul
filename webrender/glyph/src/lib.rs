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
//! - **Font Parsing**: Uses `azul-layout`'s fork of `allsorts` to parse font files and extract
//!   vector glyph outlines.
//! - **Rasterization**: Uses the `tiny-skia` 2D graphics library to render the vector outlines into
//!   anti-aliased bitmap masks.
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
//! - **API Compatible**: Aims to match the public API of `wr_glyph_rasterizer` to ensure it can be
//!   used as a direct dependency replacement.
//! - **Grayscale AA Only**: The current implementation produces high-quality grayscale anti-aliased
//!   glyphs (alpha masks). It does not support platform-specific subpixel anti-aliasing (e.g.,
//!   ClearType) or advanced hinting that relies on native OS libraries. The `gamma_lut` module is
//!   included for API compatibility but its pre-blending logic is not utilized by the rasterization
//!   backend.

pub mod font;
pub mod rasterizer;
pub mod types;

// Re-exports for compatibility with WebRender core
use std::sync::atomic::AtomicBool;

pub use font::*;
pub use rasterizer::*;
pub use types::*;

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

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use api::{FontInstanceKey, FontKey, IdNamespace};

/// Shared font resources structure
#[derive(Clone)]
pub struct SharedFontResources {
    pub font_keys: FontKeyMap,
    pub templates: FontTemplates,
    pub instance_keys: FontInstanceMap,
    pub instances: FontInstanceData,
}

impl SharedFontResources {
    pub fn new(_namespace: IdNamespace) -> Self {
        SharedFontResources {
            font_keys: FontKeyMap::new(),
            templates: FontTemplates::new(),
            instance_keys: FontInstanceMap::new(),
            instances: FontInstanceData::new(),
        }
    }
}

/// Font key mapping (stub)
#[derive(Clone)]
pub struct FontKeyMap;

impl FontKeyMap {
    pub fn new() -> Self {
        FontKeyMap
    }

    pub fn map_key(&self, key: &FontKey) -> FontKey {
        *key
    }

    pub fn add_key(&mut self, _key: FontKey) -> Option<FontKey> {
        Some(_key)
    }

    pub fn delete_key(&mut self, _key: &FontKey) -> Option<FontKey> {
        Some(*_key)
    }

    pub fn clear_namespace(&mut self, _namespace: IdNamespace) -> Vec<FontKey> {
        Vec::new() // Stub - would filter by namespace and return deleted keys
    }
}

/// Font templates storage - stores pre-parsed fonts
#[derive(Clone)]
pub struct FontTemplates {
    parsed_fonts: Arc<RwLock<HashMap<FontKey, Arc<azul_layout::font::parsed::ParsedFont>>>>,
}

impl FontTemplates {
    pub fn new() -> Self {
        FontTemplates {
            parsed_fonts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn add_parsed_font(
        &mut self,
        key: FontKey,
        parsed_font: Arc<azul_layout::font::parsed::ParsedFont>,
    ) {
        self.parsed_fonts.write().unwrap().insert(key, parsed_font);
    }

    pub fn delete_font(&mut self, key: &FontKey) {
        self.parsed_fonts.write().unwrap().remove(key);
    }

    pub fn delete_fonts(&mut self, keys: &[FontKey]) {
        let mut fonts = self.parsed_fonts.write().unwrap();
        for key in keys {
            fonts.remove(key);
        }
    }

    pub fn clear_namespace(&mut self, _namespace: IdNamespace) -> Vec<FontKey> {
        Vec::new()
    }

    pub fn has_font(&self, key: &FontKey) -> bool {
        self.parsed_fonts.read().unwrap().contains_key(key)
    }

    pub fn get_font(&self, key: &FontKey) -> Option<Arc<azul_layout::font::parsed::ParsedFont>> {
        self.parsed_fonts.read().unwrap().get(key).cloned()
    }

    pub fn lock(
        &self,
    ) -> std::sync::RwLockReadGuard<HashMap<FontKey, Arc<azul_layout::font::parsed::ParsedFont>>>
    {
        self.parsed_fonts.read().unwrap()
    }

    pub fn len(&self) -> usize {
        self.parsed_fonts.read().unwrap().len()
    }
}

/// Font instance mapping (stub)
#[derive(Clone)]
pub struct FontInstanceMap;

impl FontInstanceMap {
    pub fn new() -> Self {
        FontInstanceMap
    }

    pub fn map_key(&self, key: &FontInstanceKey) -> FontInstanceKey {
        *key
    }

    pub fn add_key(&mut self, _base: Arc<BaseFontInstance>) -> Option<FontInstanceKey> {
        Some(_base.instance_key)
    }

    pub fn delete_key(&mut self, _key: &FontInstanceKey) -> Option<FontInstanceKey> {
        Some(*_key)
    }

    pub fn clear_namespace(&mut self, _namespace: IdNamespace) -> Vec<FontInstanceKey> {
        Vec::new()
    }
}

/// Font instance data storage (stub)
#[derive(Clone)]
pub struct FontInstanceData {
    instances: Arc<RwLock<HashMap<FontInstanceKey, Arc<BaseFontInstance>>>>,
}

impl FontInstanceData {
    pub fn new() -> Self {
        FontInstanceData {
            instances: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get_font_instance(&self, key: FontInstanceKey) -> Option<Arc<BaseFontInstance>> {
        self.instances.read().unwrap().get(&key).cloned()
    }

    pub fn add_font_instance(&mut self, base: Arc<BaseFontInstance>) {
        self.instances
            .write()
            .unwrap()
            .insert(base.instance_key, base);
    }

    pub fn delete_font_instance(&mut self, key: FontInstanceKey) {
        self.instances.write().unwrap().remove(&key);
    }

    pub fn delete_font_instances(&mut self, keys: &[FontInstanceKey]) {
        let mut instances = self.instances.write().unwrap();
        for key in keys {
            instances.remove(key);
        }
    }

    pub fn clear_namespace(&mut self, _namespace: IdNamespace) {
        // Stub - would filter by namespace
    }
}

/// Glyph raster thread (compatibility type alias)
pub type GlyphRasterThread = ();
