//! Contains functions for laying out single words (uses HarfBuzz for context-aware font shaping).
//! Right now, words are laid out on a word-per-word basis, no inter-word font shaping is done.

use webrender::api::{
    LayoutPoint, RenderApi, GlyphDimensions,
    GlyphInstance as WrGlyphInstance,
};
use std::ops::Deref;
use app_units::Au;
use app_resources::LoadedFont;
use harfbuzz_sys::{
    hb_blob_create, hb_blob_destroy,
    hb_font_create, hb_font_destroy,
    hb_face_create, hb_face_destroy,
    hb_buffer_create, hb_buffer_destroy,
    hb_shape, hb_buffer_set_language, hb_buffer_set_script,
    hb_buffer_set_direction, hb_buffer_add_utf8, hb_language_from_string,
    hb_buffer_get_glyph_infos, hb_buffer_get_glyph_positions,
    hb_blob_t, hb_memory_mode_t, hb_buffer_t,
    hb_glyph_position_t, hb_glyph_info_t, hb_font_t, hb_face_t,
    HB_MEMORY_MODE_READONLY, HB_SCRIPT_LATIN, HB_DIRECTION_LTR,
};
use std::{slice, ptr, os::raw::{c_char, c_uint}};

// Translates to the ".codepoint" in HarfBuzz
pub type GlyphIndex = u32;
pub type GlyphInfo = GlyphIndex; // TODO: hb_glyph_info_t
pub type GlyphPosition = GlyphDimensions; // TODO: hb_glyph_position_t

pub struct ShapedWord {
    pub glyph_infos: Vec<GlyphInfo>,
    pub glyph_positions: Vec<GlyphPosition>,
}

pub struct HbFont<'a> {
    font: &'a LoadedFont,
    hb_face_bytes: *mut hb_blob_t,
    hb_face: *mut hb_face_t,
    hb_font: *mut hb_font_t,
}

impl<'a> HbFont<'a> {
    pub fn from_loaded_font(font: &'a LoadedFont) -> Self {
        const MEMORY_MODE: hb_memory_mode_t = HB_MEMORY_MODE_READONLY;

        // Create a HbFont with no destroy function (font is cleaned up by Rust destructor)

        let user_data_ptr = ptr::null_mut();
        let destroy_func = None;

        let font_ptr = font.font_bytes.as_ptr() as *const i8;
        let hb_face_bytes = unsafe { hb_blob_create(font_ptr, font.font_bytes.len() as u32, MEMORY_MODE, user_data_ptr, destroy_func) };
        let hb_face = unsafe { hb_face_create(hb_face_bytes, font.font_index as c_uint) };
        let hb_font = unsafe { hb_font_create(hb_face) };

        Self {
            font,
            hb_face_bytes,
            hb_face,
            hb_font,
        }
    }
}

impl<'a> Drop for HbFont<'a> {
    fn drop(&mut self) {
        unsafe { hb_font_destroy(self.hb_font) };
        unsafe { hb_face_destroy(self.hb_face) };
        unsafe { hb_blob_destroy(self.hb_face_bytes) };
    }
}

pub struct HbBuffer<'a> {
    words: &'a str,
    hb_buffer: *mut hb_buffer_t,
}

impl<'a> HbBuffer<'a> {
    pub fn from_str(words: &'a str) -> Self {

        // TODO: caching / etc.
        const LANG: &[u8;2] = b"en";
        let lang_ptr = LANG as *const u8 as *const i8;
        let lang = unsafe { hb_language_from_string(lang_ptr, -1) };

        let hb_buffer = unsafe { hb_buffer_create() };
        let word_ptr = words.as_ptr() as *const c_char; // HB handles UTF-8
        let offset = 0;
        let item_length = 0; // ??? - what is "item length" - undocumented in HarfBuzz?

        unsafe {
            hb_buffer_add_utf8(hb_buffer, word_ptr, words.len() as i32, offset, item_length);
            hb_buffer_set_direction(hb_buffer, HB_DIRECTION_LTR);
            hb_buffer_set_script(hb_buffer, HB_SCRIPT_LATIN);
            hb_buffer_set_language(hb_buffer, lang);
        }

        Self {
            words,
            hb_buffer,
        }
    }
}

impl<'a> Drop for HbBuffer<'a> {
    fn drop(&mut self) {
        unsafe { hb_buffer_destroy(self.hb_buffer) };
    }
}

// The glyph infos are allocated by HarfBuzz and freed
// when the font is destroyed. This is a convenience wrapper that
// directly dereferences the internal hb_glyph_info_t and
// hb_glyph_position_t, to avoid extra allocations.
pub struct CVec<T> {
    ptr: *const T,
    len: usize,
}

impl<T> Deref for CVec<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.ptr, self.len) }
    }
}

pub type HbGlyphInfo = hb_glyph_info_t;
pub type HbGlyphPosition = hb_glyph_position_t;

/// Shaped word - memory of the glyph_infos and glyph_positions is owned by HarfBuzz,
/// therefore the `buf` and `font` have to live as least as long as the word is in use.
pub struct HbShapedWord<'a> {
    pub buf: &'a HbBuffer<'a>,
    pub font: &'a HbFont<'a>,
    pub glyph_infos: CVec<HbGlyphInfo>,
    pub glyph_positions: CVec<HbGlyphPosition>,
}

pub(crate) fn shape_word_harfbuzz<'a>(
    text: &'a HbBuffer<'a>,
    font: &'a HbFont<'a>,
    font_size: Au,
) -> HbShapedWord<'a> {

    let features = ptr::null();
    let num_features = 0;

    unsafe { hb_shape(font.hb_font, text.hb_buffer, features, num_features) };

    let mut glyph_count = 0;
    let glyph_infos = unsafe { hb_buffer_get_glyph_infos(text.hb_buffer, &mut glyph_count) };

    let mut position_count = glyph_count;
    let glyph_positions = unsafe { hb_buffer_get_glyph_positions(text.hb_buffer, &mut position_count) };

    // Assert that there are as many glyph infos as there are glyph positions
    assert_eq!(glyph_count, position_count);

    HbShapedWord {
        buf: text,
        font,
        glyph_infos: CVec {
            ptr: glyph_infos,
            len: glyph_count as usize,
        },
        glyph_positions: CVec {
            ptr: glyph_positions,
            len: glyph_count as usize,
        },
    }
}

pub(crate) fn get_word_visual_width_hb(shaped_word: &HbShapedWord) -> f32 {
    let glyph_positions = &*shaped_word.glyph_positions;
    glyph_positions.iter().map(|pos| pos.x_advance as f32 / 64.0).sum()
}

pub(crate) fn get_glyph_instances_hb(
    shaped_word: &HbShapedWord
) -> Vec<WrGlyphInstance> {

    let glyph_infos = &*shaped_word.glyph_infos;
    let glyph_positions = &*shaped_word.glyph_positions;

    let mut current_cursor_x = 0.0;
    let mut current_cursor_y = 0.0;

    glyph_infos.iter().zip(glyph_positions.iter()).map(|(glyph_info, glyph_pos)| {
        let glyph_index = glyph_info.codepoint;

        let x_offset = glyph_pos.x_offset as f32 / 64.0;
        let y_offset = glyph_pos.y_offset as f32 / 64.0;
        let x_advance = glyph_pos.x_advance as f32 / 64.0;
        let y_advance = glyph_pos.y_advance as f32 / 64.0;

        let point = LayoutPoint::new(current_cursor_x + x_offset, current_cursor_y + y_offset);

        current_cursor_x += x_advance;
        current_cursor_y += y_advance;

        WrGlyphInstance {
            index: glyph_index,
            point,
        }
    }).collect()
}

pub(crate) fn shape_word(
    word: &str,
    font: &LoadedFont,
    font_size: Au,
    render_api: &RenderApi,
) -> ShapedWord {

    let font_instance_key = font.font_instances[&font_size];
    let space_glyph_indices = render_api.get_glyph_indices(font.key, word);
    let space_glyph_indices = space_glyph_indices.into_iter().filter_map(|e| e).collect::<Vec<u32>>();
    let space_glyph_dimensions = render_api.get_glyph_dimensions(font_instance_key, space_glyph_indices.clone());
    let space_glyph_dimensions = space_glyph_dimensions.into_iter().filter_map(|dim| dim).collect::<Vec<GlyphDimensions>>();

    ShapedWord {
        glyph_infos: space_glyph_indices,
        glyph_positions: space_glyph_dimensions,
    }
}

/// Return the sum of all the GlyphDimension advances.
/// Note for HarfBuzz migration: This is the "visual" word width, not the sum of the advances!
pub(crate) fn get_word_visual_width(glyph_dimensions: &[GlyphPosition]) -> f32 {
    glyph_dimensions.iter().map(|g| g.advance).sum()
}

/// Transform the indices (of the glyphs) and the dimensions to the final instances
pub(crate) fn get_glyph_instances(
    shaped_word: &ShapedWord,
) -> Vec<WrGlyphInstance> {

    let mut glyph_instances = Vec::with_capacity(shaped_word.glyph_positions.len());
    let mut current_cursor = 0.0;

    for (g_info, g_position) in shaped_word.glyph_infos.iter().zip(shaped_word.glyph_positions.iter()) {
        glyph_instances.push(WrGlyphInstance {
            index: *g_info,
            point: LayoutPoint::new(current_cursor, 0.0),
        });
        current_cursor += g_position.advance;
    }

    glyph_instances
}