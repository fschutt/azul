/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, you can obtain one at http://mozilla.org/MPL/2.0/. */
#![allow(unused)]
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

/// Font key mapping with namespace tracking
#[derive(Clone)]
pub struct FontKeyMap {
    // Maps external key to internal shared key
    key_map: Arc<RwLock<HashMap<FontKey, FontKey>>>,
    // Tracks which namespace owns which keys
    namespace_map: Arc<RwLock<HashMap<IdNamespace, Vec<FontKey>>>>,
}

impl FontKeyMap {
    pub fn new() -> Self {
        FontKeyMap {
            key_map: Arc::new(RwLock::new(HashMap::new())),
            namespace_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn map_key(&self, key: &FontKey) -> FontKey {
        self.key_map
            .read()
            .unwrap()
            .get(key)
            .copied()
            .unwrap_or(*key)
    }

    pub fn add_key(&mut self, key: FontKey) -> Option<FontKey> {
        let mut map = self.key_map.write().unwrap();
        if map.contains_key(&key) {
            return None; // Already exists
        }

        // Track namespace ownership
        let namespace = key.0;
        let mut ns_map = self.namespace_map.write().unwrap();
        ns_map.entry(namespace).or_insert_with(Vec::new).push(key);

        map.insert(key, key);
        Some(key)
    }

    pub fn delete_key(&mut self, key: &FontKey) -> Option<FontKey> {
        let removed = self.key_map.write().unwrap().remove(key);
        if removed.is_some() {
            // Remove from namespace tracking
            let namespace = key.0;
            if let Some(keys) = self.namespace_map.write().unwrap().get_mut(&namespace) {
                keys.retain(|k| k != key);
            }
        }
        removed
    }

    pub fn clear_namespace(&mut self, namespace: IdNamespace) -> Vec<FontKey> {
        let mut ns_map = self.namespace_map.write().unwrap();
        let keys = ns_map.remove(&namespace).unwrap_or_default();

        // Remove all keys from this namespace
        let mut map = self.key_map.write().unwrap();
        for key in &keys {
            map.remove(key);
        }

        keys
    }
}

/// Font templates storage - stores pre-parsed fonts with namespace tracking
#[derive(Clone)]
pub struct FontTemplates {
    parsed_fonts: Arc<RwLock<HashMap<FontKey, Arc<azul_layout::font::parsed::ParsedFont>>>>,
    namespace_map: Arc<RwLock<HashMap<IdNamespace, Vec<FontKey>>>>,
}

impl FontTemplates {
    pub fn new() -> Self {
        FontTemplates {
            parsed_fonts: Arc::new(RwLock::new(HashMap::new())),
            namespace_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn add_parsed_font(
        &mut self,
        key: FontKey,
        parsed_font: Arc<azul_layout::font::parsed::ParsedFont>,
    ) {
        // Track namespace ownership
        let namespace = key.0;
        let mut ns_map = self.namespace_map.write().unwrap();
        ns_map.entry(namespace).or_insert_with(Vec::new).push(key);

        self.parsed_fonts.write().unwrap().insert(key, parsed_font);
    }

    pub fn delete_font(&mut self, key: &FontKey) {
        if self.parsed_fonts.write().unwrap().remove(key).is_some() {
            // Remove from namespace tracking
            let namespace = key.0;
            if let Some(keys) = self.namespace_map.write().unwrap().get_mut(&namespace) {
                keys.retain(|k| k != key);
            }
        }
    }

    pub fn delete_fonts(&mut self, keys: &[FontKey]) {
        let mut fonts = self.parsed_fonts.write().unwrap();
        let mut ns_map = self.namespace_map.write().unwrap();

        for key in keys {
            if fonts.remove(key).is_some() {
                // Remove from namespace tracking
                let namespace = key.0;
                if let Some(ns_keys) = ns_map.get_mut(&namespace) {
                    ns_keys.retain(|k| k != key);
                }
            }
        }
    }

    pub fn clear_namespace(&mut self, namespace: IdNamespace) -> Vec<FontKey> {
        let mut ns_map = self.namespace_map.write().unwrap();
        let keys = ns_map.remove(&namespace).unwrap_or_default();

        // Remove all fonts from this namespace
        let mut fonts = self.parsed_fonts.write().unwrap();
        for key in &keys {
            fonts.remove(key);
        }

        keys
    }

    pub fn has_font(&self, key: &FontKey) -> bool {
        self.parsed_fonts.read().unwrap().contains_key(key)
    }

    pub fn get_font(&self, key: &FontKey) -> Option<Arc<azul_layout::font::parsed::ParsedFont>> {
        self.parsed_fonts.read().unwrap().get(key).cloned()
    }

    pub fn lock(
        &self,
    ) -> std::sync::RwLockReadGuard<'_, HashMap<FontKey, Arc<azul_layout::font::parsed::ParsedFont>>>
    {
        self.parsed_fonts.read().unwrap()
    }

    pub fn len(&self) -> usize {
        self.parsed_fonts.read().unwrap().len()
    }
}

/// Font instance mapping with namespace tracking
#[derive(Clone)]
pub struct FontInstanceMap {
    // Maps external key to internal shared key
    key_map: Arc<RwLock<HashMap<FontInstanceKey, FontInstanceKey>>>,
    // Tracks which namespace owns which instance keys
    namespace_map: Arc<RwLock<HashMap<IdNamespace, Vec<FontInstanceKey>>>>,
}

impl FontInstanceMap {
    pub fn new() -> Self {
        FontInstanceMap {
            key_map: Arc::new(RwLock::new(HashMap::new())),
            namespace_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn map_key(&self, key: &FontInstanceKey) -> FontInstanceKey {
        self.key_map
            .read()
            .unwrap()
            .get(key)
            .copied()
            .unwrap_or(*key)
    }

    pub fn add_key(&mut self, base: Arc<BaseFontInstance>) -> Option<FontInstanceKey> {
        let key = base.instance_key;
        let mut map = self.key_map.write().unwrap();

        if map.contains_key(&key) {
            return None; // Already exists
        }

        // Track namespace ownership
        let namespace = key.0;
        let mut ns_map = self.namespace_map.write().unwrap();
        ns_map.entry(namespace).or_insert_with(Vec::new).push(key);

        map.insert(key, key);
        Some(key)
    }

    pub fn delete_key(&mut self, key: &FontInstanceKey) -> Option<FontInstanceKey> {
        let removed = self.key_map.write().unwrap().remove(key);
        if removed.is_some() {
            // Remove from namespace tracking
            let namespace = key.0;
            if let Some(keys) = self.namespace_map.write().unwrap().get_mut(&namespace) {
                keys.retain(|k| k != key);
            }
        }
        removed
    }

    pub fn clear_namespace(&mut self, namespace: IdNamespace) -> Vec<FontInstanceKey> {
        let mut ns_map = self.namespace_map.write().unwrap();
        let keys = ns_map.remove(&namespace).unwrap_or_default();

        // Remove all keys from this namespace
        let mut map = self.key_map.write().unwrap();
        for key in &keys {
            map.remove(key);
        }

        keys
    }
}

/// Font instance data storage with namespace tracking
#[derive(Clone)]
pub struct FontInstanceData {
    instances: Arc<RwLock<HashMap<FontInstanceKey, Arc<BaseFontInstance>>>>,
    namespace_map: Arc<RwLock<HashMap<IdNamespace, Vec<FontInstanceKey>>>>,
}

impl FontInstanceData {
    pub fn new() -> Self {
        FontInstanceData {
            instances: Arc::new(RwLock::new(HashMap::new())),
            namespace_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get_font_instance(&self, key: FontInstanceKey) -> Option<Arc<BaseFontInstance>> {
        self.instances.read().unwrap().get(&key).cloned()
    }

    pub fn add_font_instance(&mut self, base: Arc<BaseFontInstance>) {
        let key = base.instance_key;
        let namespace = key.0;

        // Track namespace ownership
        let mut ns_map = self.namespace_map.write().unwrap();
        ns_map.entry(namespace).or_insert_with(Vec::new).push(key);

        self.instances.write().unwrap().insert(key, base);
    }

    pub fn delete_font_instance(&mut self, key: FontInstanceKey) {
        if self.instances.write().unwrap().remove(&key).is_some() {
            // Remove from namespace tracking
            let namespace = key.0;
            if let Some(keys) = self.namespace_map.write().unwrap().get_mut(&namespace) {
                keys.retain(|k| k != &key);
            }
        }
    }

    pub fn delete_font_instances(&mut self, keys: &[FontInstanceKey]) {
        let mut instances = self.instances.write().unwrap();
        let mut ns_map = self.namespace_map.write().unwrap();

        for key in keys {
            if instances.remove(key).is_some() {
                // Remove from namespace tracking
                let namespace = key.0;
                if let Some(ns_keys) = ns_map.get_mut(&namespace) {
                    ns_keys.retain(|k| k != key);
                }
            }
        }
    }

    pub fn clear_namespace(&mut self, namespace: IdNamespace) {
        let mut ns_map = self.namespace_map.write().unwrap();
        if let Some(keys) = ns_map.remove(&namespace) {
            // Remove all instances from this namespace
            let mut instances = self.instances.write().unwrap();
            for key in keys {
                instances.remove(&key);
            }
        }
    }
}

// Implement BlobImageResources trait for SharedFontResources
impl api::BlobImageResources for SharedFontResources {
    fn get_font_data(&self, key: FontKey) -> Option<api::FontTemplate> {
        // Return parsed font as Raw template for compatibility
        self.templates.get_font(&key).map(|parsed_font| {
            // For blob rasterization, we need to provide the font data
            // Since we have Arc<ParsedFont>, we can't easily convert back to bytes
            // Return a dummy template - blob rasterization should use parsed fonts directly
            api::FontTemplate::Raw(Arc::new(Vec::new()), 0)
        })
    }

    fn get_font_instance_data(&self, key: FontInstanceKey) -> Option<api::FontInstanceData> {
        self.instances
            .get_font_instance(key)
            .map(|base| api::FontInstanceData {
                font_key: base.font_key,
                size: base.size.into(),
                options: Some(base.options.clone()),
                platform_options: base.platform_options.clone(),
                variations: base.variations.clone(),
            })
    }
}

/// Glyph raster thread (compatibility type alias)
pub type GlyphRasterThread = ();
