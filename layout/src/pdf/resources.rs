//! Resource tracking for PDF generation.
//!
//! This module tracks which fonts and images are referenced in a PDF document,
//! allowing the actual PDF renderer to load and embed only the necessary resources.

use std::collections::{BTreeMap, BTreeSet};

use azul_core::resources::ImageKey;
use azul_css::props::basic::FontRef;

use super::pdf_ops::FontId;

/// Tracks all resources (fonts, images) needed to render a PDF document.
#[derive(Debug, Clone, Default)]
pub struct PdfRenderResources {
    /// Set of unique fonts referenced in the document
    pub fonts: BTreeSet<FontRef>,

    /// Map from font references to their assigned PDF font IDs
    pub font_ids: BTreeMap<FontRef, FontId>,

    /// Set of images referenced in the document
    pub images: BTreeSet<ImageKey>,

    /// Counter for generating unique font IDs
    font_id_counter: usize,
}

impl PdfRenderResources {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a font and get its PDF font ID.
    /// If the font is already registered, returns the existing ID.
    pub fn register_font(&mut self, font_ref: FontRef) -> FontId {
        if let Some(id) = self.font_ids.get(&font_ref) {
            return id.clone();
        }

        let font_id = FontId::new(format!("F{}", self.font_id_counter));
        self.font_id_counter += 1;

        self.fonts.insert(font_ref.clone());
        self.font_ids.insert(font_ref, font_id.clone());

        font_id
    }

    /// Register an image resource
    pub fn register_image(&mut self, image_key: ImageKey) {
        self.images.insert(image_key);
    }

    /// Get the font ID for a registered font
    pub fn get_font_id(&self, font_ref: &FontRef) -> Option<&FontId> {
        self.font_ids.get(font_ref)
    }

    /// Check if a font is registered
    pub fn has_font(&self, font_ref: &FontRef) -> bool {
        self.fonts.contains(font_ref)
    }

    /// Check if an image is registered
    pub fn has_image(&self, image_key: &ImageKey) -> bool {
        self.images.contains(image_key)
    }

    /// Get all registered fonts
    pub fn get_fonts(&self) -> &BTreeSet<FontRef> {
        &self.fonts
    }

    /// Get all registered images
    pub fn get_images(&self) -> &BTreeSet<ImageKey> {
        &self.images
    }
}
