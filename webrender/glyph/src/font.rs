/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, you can obtain one at http://mozilla.org/MPL/2.0/. */

use std::sync::Arc;

use api::{FontKey, FontRenderMode, GlyphDimensions};
use azul_core::resources::{
    GlyphOutlineOperation, OutlineCubicTo, OutlineLineTo, OutlineMoveTo, OutlineQuadTo,
};
use azul_layout::font::parsed::{OwnedGlyph, ParsedFont};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

use crate::{
    rasterizer::{
        FontInstance, GlyphFormat, GlyphKey, GlyphRasterError, GlyphRasterResult, RasterizedGlyph,
    },
    types::FastHashMap,
};

/// A pure-Rust font context that uses `azul-layout` for font parsing
/// and `tiny-skia` for glyph rasterization.
pub struct FontContext {
    fonts: FastHashMap<FontKey, Arc<ParsedFont>>,
}

impl FontContext {
    /// Creates a new, empty font context.
    pub fn new() -> Self {
        FontContext {
            fonts: FastHashMap::default(),
        }
    }

    /// Adds a font directly from a parsed font.
    ///
    /// This avoids re-parsing the font since azul-layout has already parsed it.
    pub fn add_font(&mut self, font_key: FontKey, parsed_font: Arc<ParsedFont>) {
        if !self.fonts.contains_key(&font_key) {
            self.fonts.insert(font_key, parsed_font);
        }
    }

    /// Adds a font from raw bytes (for backward compatibility).
    ///
    /// The font is parsed by `azul-layout` and its outlines are stored for later rasterization.
    pub fn add_font_from_bytes(&mut self, font_key: FontKey, bytes: &[u8], index: u32) {
        if self.fonts.contains_key(&font_key) {
            return;
        }

        if let Some(parsed_font) = ParsedFont::from_bytes(bytes, index as usize, true) {
            self.fonts.insert(font_key, Arc::new(parsed_font));
        } else {
            log::error!("Failed to parse font for key {:?}", font_key);
        }
    }

    /// Removes a font from the context.
    pub fn delete_font(&mut self, font_key: &FontKey) {
        self.fonts.remove(font_key);
    }

    /// Looks up the glyph index for a given character.
    pub fn get_glyph_index(&self, font_key: FontKey, ch: char) -> Option<u32> {
        self.fonts
            .get(&font_key)?
            .lookup_glyph_index(ch as u32)
            .map(|id| id as u32)
    }

    /// Calculates the rasterized dimensions of a glyph without actually rasterizing it.
    pub fn get_glyph_dimensions(
        &self,
        font: &FontInstance,
        key: &GlyphKey,
    ) -> Option<GlyphDimensions> {
        let parsed_font = self.fonts.get(&font.font_key)?;
        let glyph_id = key.index() as u16;
        let glyph = parsed_font.glyph_records_decoded.get(&glyph_id)?;

        let units_per_em = parsed_font.font_metrics.units_per_em as f32;
        if units_per_em == 0.0 {
            return None;
        }

        // Calculate the pixel scale from font units.
        let scale = font.size.to_f32_px() / units_per_em;

        let bb = &glyph.bounding_box;
        let width = ((bb.max_x - bb.min_x) as f32 * scale).ceil() as i32;
        let height = ((bb.max_y - bb.min_y) as f32 * scale).ceil() as i32;
        let advance = glyph.horz_advance as f32 * scale;

        Some(GlyphDimensions {
            left: (bb.min_x as f32 * scale).floor() as i32,
            top: (bb.max_y as f32 * scale).ceil() as i32, // Note: Y is up in font coordinates
            width: width.max(0),
            height: height.max(0),
            advance,
        })
    }

    /// Prepares a font instance for rasterization.
    /// This backend only supports alpha masks, so we simplify the render mode.
    pub fn prepare_font(font: &mut FontInstance) {
        font.render_mode = FontRenderMode::Alpha;
        // Color is irrelevant for alpha masks which are tinted in the shader.
        font.color = api::ColorU::new(255, 255, 255, 255);
    }

    /// Rasterizes a single glyph into an alpha mask.
    pub fn rasterize_glyph(&self, font: &FontInstance, key: &GlyphKey) -> GlyphRasterResult {
        let parsed_font = self
            .fonts
            .get(&font.font_key)
            .ok_or(GlyphRasterError::LoadFailed)?;
        let glyph_id = key.index() as u16;
        let owned_glyph = parsed_font
            .glyph_records_decoded
            .get(&glyph_id)
            .ok_or(GlyphRasterError::LoadFailed)?;

        let units_per_em = parsed_font.font_metrics.units_per_em as f32;
        if units_per_em <= 0.0 {
            return Err(GlyphRasterError::LoadFailed);
        }

        let scale = font.size.to_f32_px() / units_per_em;

        let Some(path) = build_path_from_outline(owned_glyph) else {
            return Err(GlyphRasterError::LoadFailed);
        };

        let bb = &owned_glyph.bounding_box;
        // Add 1px padding on each side to prevent clipping from anti-aliasing.
        let padding = 1.0;
        let pixel_width =
            ((bb.max_x - bb.min_x) as f32 * scale).ceil() as u32 + (padding * 2.0) as u32;
        let pixel_height =
            ((bb.max_y - bb.min_y) as f32 * scale).ceil() as u32 + (padding * 2.0) as u32;

        // The top-left corner of the glyph's bounding box in pixel space.
        let left = (bb.min_x as f32 * scale).floor();
        let top = (bb.max_y as f32 * scale).ceil();

        if pixel_width == 0 || pixel_height == 0 || pixel_width > 4096 || pixel_height > 4096 {
            return Err(GlyphRasterError::LoadFailed);
        }

        let mut pixmap =
            Pixmap::new(pixel_width, pixel_height).ok_or(GlyphRasterError::LoadFailed)?;

        let mut paint = Paint::default();
        paint.set_color_rgba8(255, 255, 255, 255);
        paint.anti_alias = true;

        // Transform to scale from font units and translate to the pixmap's origin.
        let (sub_dx, sub_dy) = font.get_subpx_offset(key);
        let transform = Transform::from_scale(scale, scale).post_translate(
            -left + padding + sub_dx as f32,
            top + padding - sub_dy as f32,
        );

        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);

        // Convert the BGRA pixmap data into a pure alpha mask.
        // WebRender will later convert this to R8 or BGRA8 as needed.
        let alpha_bytes: Vec<u8> = pixmap.pixels().iter().map(|p| p.alpha()).collect();

        Ok(RasterizedGlyph {
            left: left - padding,
            top: -top - padding, // WebRender expects Y-down for bitmap top coordinate
            width: pixel_width as i32,
            height: pixel_height as i32,
            scale: 1.0, // The rasterized glyph is already at the correct scale.
            format: GlyphFormat::Alpha,
            bytes: alpha_bytes,
        })
    }
}

/// Converts an `azul-layout` `OwnedGlyph` outline into a `tiny-skia::Path`.
fn build_path_from_outline(glyph: &OwnedGlyph) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    let mut has_ops = false;
    for outline in &glyph.outline {
        for op in outline.operations.as_slice() {
            has_ops = true;
            match op {
                // Font coordinates are Y-up, tiny-skia is Y-down, so we negate Y.
                GlyphOutlineOperation::MoveTo(OutlineMoveTo { x, y }) => {
                    pb.move_to(*x as f32, -(*y as f32));
                }
                GlyphOutlineOperation::LineTo(OutlineLineTo { x, y }) => {
                    pb.line_to(*x as f32, -(*y as f32));
                }
                GlyphOutlineOperation::QuadraticCurveTo(OutlineQuadTo {
                    ctrl_1_x,
                    ctrl_1_y,
                    end_x,
                    end_y,
                }) => {
                    pb.quad_to(
                        *ctrl_1_x as f32,
                        -(*ctrl_1_y as f32),
                        *end_x as f32,
                        -(*end_y as f32),
                    );
                }
                GlyphOutlineOperation::CubicCurveTo(OutlineCubicTo {
                    ctrl_1_x,
                    ctrl_1_y,
                    ctrl_2_x,
                    ctrl_2_y,
                    end_x,
                    end_y,
                }) => {
                    pb.cubic_to(
                        *ctrl_1_x as f32,
                        -(*ctrl_1_y as f32),
                        *ctrl_2_x as f32,
                        -(*ctrl_2_y as f32),
                        *end_x as f32,
                        -(*end_y as f32),
                    );
                }
                GlyphOutlineOperation::ClosePath => {
                    pb.close();
                }
            }
        }
    }
    if has_ops {
        pb.finish()
    } else {
        None
    }
}
