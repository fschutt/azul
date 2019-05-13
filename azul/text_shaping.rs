//! Contains functions for laying out single words (uses HarfBuzz for context-aware font shaping).
//! Right now, words are laid out on a word-per-word basis, no inter-word font shaping is done.

use std::{slice, ptr, u32, ops::Deref, os::raw::{c_char, c_uint}};
use harfbuzz_sys::{
    hb_blob_create, hb_blob_destroy,
    hb_font_create, hb_font_destroy,
    hb_face_create, hb_face_destroy,
    hb_buffer_create, hb_buffer_destroy,
    hb_shape, hb_font_set_scale, hb_buffer_add_utf8, hb_ot_font_set_funcs,
    hb_buffer_get_glyph_infos, hb_buffer_get_glyph_positions,
    hb_buffer_guess_segment_properties, hb_buffer_allocation_successful,
    hb_blob_t, hb_memory_mode_t, hb_buffer_t,
    hb_glyph_position_t, hb_glyph_info_t, hb_font_t, hb_face_t,
    hb_feature_t, hb_tag_t,
    HB_MEMORY_MODE_READONLY,
};
use azul_core::{window::LogicalPosition, display_list::GlyphInstance};

pub type GlyphInfo = hb_glyph_info_t;
pub type GlyphPosition = hb_glyph_position_t;

const MEMORY_MODE_READONLY: hb_memory_mode_t = HB_MEMORY_MODE_READONLY;
const HB_SCALE_FACTOR: f32 = 128.0;

// NOTE: hb_tag_t = u32
// See: https://github.com/tangrams/harfbuzz-example/blob/master/src/hbshaper.h
//
// Translation of the original HB_TAG macro, defined in:
// https://github.com/harfbuzz/harfbuzz/blob/90dd255e570bf8ea3436e2f29242068845256e55/src/hb-common.h#L89
//
// NOTE: Minimum required rustc version for const fn is 1.31.
const fn create_hb_tag(tag: (char, char, char, char)) -> hb_tag_t {
    (((tag.0 as hb_tag_t) & 0xFF) << 24) |
    (((tag.1 as hb_tag_t) & 0xFF) << 16) |
    (((tag.2 as hb_tag_t) & 0xFF) << 8)  |
    (((tag.3 as hb_tag_t) & 0xFF) << 0)
}

// Kerning operations
const KERN_TAG: hb_tag_t = create_hb_tag(('k', 'e', 'r', 'n'));
// Standard ligature substitution
const LIGA_TAG: hb_tag_t = create_hb_tag(('l', 'i', 'g', 'a'));
// Contextual ligature substitution
const CLIG_TAG: hb_tag_t = create_hb_tag(('c', 'l', 'i', 'g'));

const FEATURE_KERNING_OFF: hb_feature_t  = hb_feature_t { tag: KERN_TAG, value: 0, start: 0, end: u32::MAX };
const FEATURE_KERNING_ON: hb_feature_t   = hb_feature_t { tag: KERN_TAG, value: 1, start: 0, end: u32::MAX };
const FEATURE_LIGATURE_OFF: hb_feature_t = hb_feature_t { tag: LIGA_TAG, value: 0, start: 0, end: u32::MAX };
const FEATURE_LIGATURE_ON: hb_feature_t  = hb_feature_t { tag: LIGA_TAG, value: 1, start: 0, end: u32::MAX };
const FEATURE_CLIG_OFF: hb_feature_t     = hb_feature_t { tag: CLIG_TAG, value: 0, start: 0, end: u32::MAX };
const FEATURE_CLIG_ON: hb_feature_t      = hb_feature_t { tag: CLIG_TAG, value: 1, start: 0, end: u32::MAX };

// NOTE: kerning is a "feature" and has to be specifically turned on.
static ACTIVE_HB_FEATURES: [hb_feature_t;3] = [
    FEATURE_KERNING_ON,
    FEATURE_LIGATURE_ON,
    FEATURE_CLIG_ON,
];

#[derive(Debug, Clone)]
pub struct ShapedWord {
    pub glyph_infos: Vec<GlyphInfo>,
    pub glyph_positions: Vec<GlyphPosition>,
}

#[derive(Debug)]
pub struct HbFont<'a> {
    font_bytes: &'a [u8],
    font_index: u32,
    hb_face_bytes: *mut hb_blob_t,
    hb_face: *mut hb_face_t,
    hb_font: *mut hb_font_t,
}

impl<'a> HbFont<'a> {
    pub fn from_bytes(font_bytes: &'a [u8], font_index: u32) -> Self {

        // Create a HbFont with no destroy function (font is cleaned up by Rust destructor)

        let user_data_ptr = ptr::null_mut();
        let destroy_func = None;

        let font_ptr = font_bytes.as_ptr() as *const i8;
        let hb_face_bytes = unsafe {
            hb_blob_create(font_ptr, font_bytes.len() as u32, MEMORY_MODE_READONLY, user_data_ptr, destroy_func)
        };
        let hb_face = unsafe { hb_face_create(hb_face_bytes, font_index as c_uint) };
        let hb_font = unsafe { hb_font_create(hb_face) };
        unsafe { hb_ot_font_set_funcs(hb_font) };

        Self {
            font_bytes,
            font_index,
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
        // TODO: Is this safe - memory may be deleted twice?
        unsafe { hb_blob_destroy(self.hb_face_bytes) };
    }
}

#[derive(Debug)]
pub struct HbScaledFont<'a> {
    pub font: &'a HbFont<'a>,
    pub font_size_px: f32,
}

impl<'a> HbScaledFont<'a> {
    /// Create a `HbScaledFont` from a
    pub fn from_font(font: &'a HbFont<'a>, font_size_px: f32) -> Self {
        let px = (font_size_px * HB_SCALE_FACTOR) as i32;
        unsafe { hb_font_set_scale(font.hb_font, px, px) };
        Self {
            font,
            font_size_px,
        }
    }
}

#[derive(Debug)]
pub struct HbBuffer<'a> {
    words: &'a str,
    hb_buffer: *mut hb_buffer_t,
}

impl<'a> HbBuffer<'a> {
    pub fn from_str(words: &'a str) -> Self {

        let hb_buffer = unsafe { hb_buffer_create() };
        unsafe { hb_buffer_allocation_successful(hb_buffer); };
        let word_ptr = words.as_ptr() as *const c_char; // HB handles UTF-8

        let word_len = words.len() as i32;

        // NOTE: It's not possible to take a sub-string into a UTF-8 buffer!

        unsafe {
            hb_buffer_add_utf8(hb_buffer, word_ptr, word_len, 0, word_len);
            // Guess the script, language and direction from the buffer
            hb_buffer_guess_segment_properties(hb_buffer);
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
#[derive(Debug)]
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
#[derive(Debug)]
pub struct HbShapedWord<'a> {
    pub buf: &'a HbBuffer<'a>,
    pub scaled_font: &'a HbScaledFont<'a>,
    pub glyph_infos: CVec<HbGlyphInfo>,
    pub glyph_positions: CVec<HbGlyphPosition>,
}

pub(crate) fn shape_word_hb<'a>(
    text: &'a HbBuffer<'a>,
    scaled_font: &'a HbScaledFont<'a>,
) -> HbShapedWord<'a> {

    let features = if ACTIVE_HB_FEATURES.is_empty() {
        ptr::null()
    } else {
        &ACTIVE_HB_FEATURES as *const _
    };

    let num_features = ACTIVE_HB_FEATURES.len() as u32;

    unsafe { hb_shape(scaled_font.font.hb_font, text.hb_buffer, features, num_features) };

    let mut glyph_count = 0;
    let glyph_infos = unsafe { hb_buffer_get_glyph_infos(text.hb_buffer, &mut glyph_count) };

    let mut position_count = glyph_count;
    let glyph_positions = unsafe { hb_buffer_get_glyph_positions(text.hb_buffer, &mut position_count) };

    // Assert that there are as many glyph infos as there are glyph positions
    assert_eq!(glyph_count, position_count);

    HbShapedWord {
        buf: text,
        scaled_font,
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

pub(crate) fn get_word_visual_width_hb(glyph_positions: &[GlyphPosition]) -> f32 {
    glyph_positions.iter().map(|pos| pos.x_advance as f32 / HB_SCALE_FACTOR).sum()
}

pub(crate) fn get_glyph_infos_hb(glyph_infos: &[GlyphInfo]) -> Vec<GlyphInfo> {
    glyph_infos.iter().cloned().collect()
}

pub(crate) fn get_glyph_positions_hb(glyph_positions: &[GlyphPosition]) -> Vec<GlyphPosition> {
    glyph_positions.iter().cloned().collect()
}

pub(crate) fn get_glyph_instances_hb(
    glyph_infos: &[GlyphInfo],
    glyph_positions: &[GlyphPosition],
) -> Vec<GlyphInstance> {

    let mut current_cursor_x = 0.0;
    let mut current_cursor_y = 0.0;

    glyph_infos.iter().zip(glyph_positions.iter()).map(|(glyph_info, glyph_pos)| {
        let glyph_index = glyph_info.codepoint;

        let x_offset = glyph_pos.x_offset as f32 / HB_SCALE_FACTOR;
        let y_offset = glyph_pos.y_offset as f32 / HB_SCALE_FACTOR;
        let x_advance = glyph_pos.x_advance as f32 / HB_SCALE_FACTOR;
        let y_advance = glyph_pos.y_advance as f32 / HB_SCALE_FACTOR;

        let point = LogicalPosition::new(current_cursor_x + x_offset, current_cursor_y + y_offset);

        current_cursor_x += x_advance;
        current_cursor_y += y_advance;

        GlyphInstance {
            index: glyph_index,
            point,
        }
    }).collect()
}
