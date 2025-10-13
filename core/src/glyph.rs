use crate::window::OptionChar;

#[derive(Debug, Default, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub struct Advance {
    pub advance_x: u16,
    pub size_x: i32,
    pub size_y: i32,
    pub kerning: i16,
}

impl Advance {
    #[inline]
    pub const fn get_x_advance_total_unscaled(&self) -> i32 {
        self.advance_x as i32 + self.kerning as i32
    }
    #[inline]
    pub const fn get_x_advance_unscaled(&self) -> u16 {
        self.advance_x
    }
    #[inline]
    pub const fn get_x_size_unscaled(&self) -> i32 {
        self.size_x
    }
    #[inline]
    pub const fn get_y_size_unscaled(&self) -> i32 {
        self.size_y
    }
    #[inline]
    pub const fn get_kerning_unscaled(&self) -> i16 {
        self.kerning
    }

    #[inline]
    pub fn get_x_advance_total_scaled(&self, units_per_em: u16, target_font_size: f32) -> f32 {
        self.get_x_advance_total_unscaled() as f32 / units_per_em as f32 * target_font_size
    }
    #[inline]
    pub fn get_x_advance_scaled(&self, units_per_em: u16, target_font_size: f32) -> f32 {
        self.get_x_advance_unscaled() as f32 / units_per_em as f32 * target_font_size
    }
    #[inline]
    pub fn get_x_size_scaled(&self, units_per_em: u16, target_font_size: f32) -> f32 {
        self.get_x_size_unscaled() as f32 / units_per_em as f32 * target_font_size
    }
    #[inline]
    pub fn get_y_size_scaled(&self, units_per_em: u16, target_font_size: f32) -> f32 {
        self.get_y_size_unscaled() as f32 / units_per_em as f32 * target_font_size
    }
    #[inline]
    pub fn get_kerning_scaled(&self, units_per_em: u16, target_font_size: f32) -> f32 {
        self.get_kerning_unscaled() as f32 / units_per_em as f32 * target_font_size
    }
}

/// A Unicode variation selector.
///
/// VS04-VS14 are omitted as they aren't currently used.
#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub enum VariationSelector {
    /// VARIATION SELECTOR-1
    VS01 = 1,
    /// VARIATION SELECTOR-2
    VS02 = 2,
    /// VARIATION SELECTOR-3
    VS03 = 3,
    /// Text presentation
    VS15 = 15,
    /// Emoji presentation
    VS16 = 16,
}

impl_option!(
    VariationSelector,
    OptionVariationSelector,
    [Debug, Copy, PartialEq, PartialOrd, Clone, Hash]
);

#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C, u8)]
pub enum GlyphOrigin {
    Char(char),
    Direct,
}

#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub struct PlacementDistance {
    pub x: i32,
    pub y: i32,
}

/// When not Attachment::None indicates that this glyph
/// is an attachment with placement indicated by the variant.
#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C, u8)]
pub enum Placement {
    None,
    Distance(PlacementDistance),
    MarkAnchor(MarkAnchorPlacement),
    /// An overprint mark.
    ///
    /// This mark is shown at the same position as the base glyph.
    ///
    /// Fields: (base glyph index in `Vec<GlyphInfo>`)
    MarkOverprint(usize),
    CursiveAnchor(CursiveAnchorPlacement),
}

/// Cursive anchored placement.
///
/// https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#lookup-type-3-cursive-attachment-positioning-subtable
#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub struct CursiveAnchorPlacement {
    /// exit glyph index in the `Vec<GlyphInfo>`
    pub exit_glyph_index: usize,
    /// RIGHT_TO_LEFT flag from lookup table
    pub right_to_left: bool,
    /// exit glyph anchor
    pub exit_glyph_anchor: Anchor,
    /// entry glyph anchor
    pub entry_glyph_anchor: Anchor,
}

/// An anchored mark.
///
/// This is a mark where its anchor is aligned with the base glyph anchor.
#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub struct MarkAnchorPlacement {
    /// base glyph index in `Vec<GlyphInfo>`
    pub base_glyph_index: usize,
    /// base glyph anchor
    pub base_glyph_anchor: Anchor,
    /// mark anchor
    pub mark_anchor: Anchor,
}

#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub struct Anchor {
    pub x: i16,
    pub y: i16,
}

#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub struct RawGlyph {
    pub unicode_codepoint: OptionChar, // Option<char>
    pub glyph_index: u16,
    pub liga_component_pos: u16,
    pub glyph_origin: GlyphOrigin,
    pub small_caps: bool,
    pub multi_subst_dup: bool,
    pub is_vert_alt: bool,
    pub fake_bold: bool,
    pub fake_italic: bool,
    pub variation: OptionVariationSelector,
}

impl RawGlyph {
    pub fn has_codepoint(&self) -> bool {
        self.unicode_codepoint.is_some()
    }

    pub fn get_codepoint(&self) -> Option<char> {
        self.unicode_codepoint
            .as_ref()
            .and_then(|u| core::char::from_u32(*u))
    }
}

#[derive(Debug, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub struct GlyphInfo {
    pub glyph: RawGlyph,
    pub size: Advance,
    pub kerning: i16,
    pub placement: Placement,
}

impl_vec!(GlyphInfo, GlyphInfoVec, GlyphInfoVecDestructor);
impl_vec_clone!(GlyphInfo, GlyphInfoVec, GlyphInfoVecDestructor);
impl_vec_debug!(GlyphInfo, GlyphInfoVec);
impl_vec_partialeq!(GlyphInfo, GlyphInfoVec);
impl_vec_partialord!(GlyphInfo, GlyphInfoVec);
impl_vec_hash!(GlyphInfo, GlyphInfoVec);
