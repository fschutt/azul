//! Calculate glyph positions.
//!
//! [GlyphLayout] is used to obtain the positions for a collection of shaped glyphs. The position
//! for each glyph includes its horizontal and vertical advance as well as any `(x, y)` offset from
//! the origin. Horizontal layout in left-to-right and right-to-left directions is supported. Basic
//! (but incomplete) support for vertical text is present too.
//!
//! The position of a series of glyphs is determined from an initial pen position, which is
//! incremented by the advance of each glyph as they are processed. The position of a particular
//! glyph is the current pen position plus `x_offset` and `y_offset`.

use crate::context::Glyph;
use crate::error::ParseError;
use crate::gpos::{Info, Placement};
use crate::tables::FontTableProvider;
use crate::unicode::codepoint::is_upright_char;
use crate::Font;

/// Used to calculate the position of shaped glyphs.
pub struct GlyphLayout<'f, 'i, T>
where
    T: FontTableProvider,
{
    font: &'f mut Font<T>,
    infos: &'i [Info],
    direction: TextDirection,
    vertical: bool,
}

/// The position and advance of a glyph.
#[derive(Copy, Clone, Eq, Debug, Default)]
pub struct GlyphPosition {
    /// Horizontal advance
    pub hori_advance: i32,
    /// Vertical advance
    pub vert_advance: i32,
    /// Offset in the X (horizontal) direction of this glyph
    pub x_offset: i32,
    /// Offset in the Y (vertical) direction of this glyph
    pub y_offset: i32,
    cursive_attachment: Option<u16>,
}

/// The horizontal text layout direction.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TextDirection {
    LeftToRight,
    RightToLeft,
}

impl<'f, 'i, T: FontTableProvider> GlyphLayout<'f, 'i, T> {
    /// Construct a new `GlyphLayout` instance.
    ///
    /// **Arguments**
    ///
    /// * `font` — the font that the glyphs belong to.
    /// * `infos` — the shaped glyphs to lay out.
    /// * `direction` — the horizontal text layout direction.
    /// * `vertical` — `true` if the text is being laid out top to bottom.
    pub fn new(
        font: &'f mut Font<T>,
        infos: &'i [Info],
        direction: TextDirection,
        vertical: bool,
    ) -> Self {
        GlyphLayout {
            font,
            infos,
            direction,
            vertical,
        }
    }

    /// Retrieve the glyphs positions.
    pub fn glyph_positions(&mut self) -> Result<Vec<GlyphPosition>, ParseError> {
        let mut has_marks = false;
        let mut has_cursive_connection = false;
        let mut positions = vec![GlyphPosition::default(); self.infos.len()];

        for (i, info) in self.infos.iter().enumerate() {
            let (hori_advance, vert_advance) = glyph_advance(self.font, info, self.vertical)?;
            match info.placement {
                Placement::None => positions[i].update(hori_advance, vert_advance, 0, 0),
                Placement::Distance(dx, dy) => {
                    positions[i].update(hori_advance, vert_advance, dx, dy)
                }
                Placement::MarkAnchor(base_index, base_anchor, mark_anchor) => {
                    has_marks = true;
                    match self.infos.get(base_index) {
                        Some(base_info) => {
                            let (dx, dy) = match base_info.placement {
                                Placement::Distance(dx, dy) => (dx, dy),
                                _ => (0, 0),
                            };
                            let offset_x = i32::from(base_anchor.x) - i32::from(mark_anchor.x) + dx;
                            let offset_y = i32::from(base_anchor.y) - i32::from(mark_anchor.y) + dy;
                            positions[i].update(hori_advance, vert_advance, offset_x, offset_y);
                        }
                        None => {
                            return Err(ParseError::BadIndex);
                        }
                    }
                }
                Placement::MarkOverprint(base_index) => {
                    has_marks = true;
                    positions[i].update_advance(0, 0);
                    self.infos.get(base_index).ok_or(ParseError::BadIndex)?;
                }
                Placement::CursiveAnchor(exit_glyph_index, _, _, _) => {
                    has_cursive_connection = true;
                    // Validate index
                    self.infos
                        .get(exit_glyph_index)
                        .ok_or(ParseError::BadIndex)?;

                    // Link to exit glyph
                    positions[exit_glyph_index].cursive_attachment = Some(u16::try_from(i)?);
                    let new_glyph = GlyphPosition {
                        hori_advance,
                        vert_advance,
                        ..positions[i]
                    };
                    positions[i] = new_glyph;
                }
            };
        }

        if has_cursive_connection {
            // Now that we know all base glyphs are positioned we do a second pass to apply
            // cursive attachment adjustments
            self.adjust_cursive_connections(&mut positions);
        }

        if has_marks {
            // Now that cursive connected glyphs are positioned, ensure marks are positioned on their
            // base properly.
            self.position_marks(&mut positions);
        }

        Ok(positions)
    }

    fn adjust_cursive_connections(&self, positions: &mut [GlyphPosition]) {
        for (i, info) in self.infos.iter().enumerate() {
            match info.placement {
                Placement::None
                | Placement::Distance(_, _)
                | Placement::MarkAnchor(_, _, _)
                | Placement::MarkOverprint(_) => {}
                Placement::CursiveAnchor(
                    exit_glyph_index,
                    rtl_flag,
                    exit_glyph_anchor,
                    entry_glyph_anchor,
                ) => {
                    // Anchor alignment can result in horizontal or vertical positioning adjustments,
                    // or both. Note that the positioning effects in the text-layout direction
                    // (horizontal, for horizontal layout) work differently than for the cross-stream
                    // direction (vertical, in horizontal layout):
                    //
                    // * For adjustments in the line-layout direction, the layout engine adjusts the
                    //   advance of the first glyph (in logical order). This effectively moves the
                    //   second glyph relative to the first so that the anchors are aligned in that
                    //   direction.
                    // * For the cross-stream direction, placement of one glyph is adjusted to make
                    //   the anchors align. Which glyph is adjusted is determined by the RIGHT_TO_LEFT
                    //   flag in the parent lookup table: if the RIGHT_TO_LEFT flag is clear, the
                    //   second glyph is adjusted to align anchors with the first glyph; if the
                    //   RIGHT_TO_LEFT flag is set, the first glyph is adjusted to align anchors with
                    //   the second glyph.
                    //
                    // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#lookup-type-3-cursive-attachment-positioning-subtable

                    // First glyph in logical order is the one with the lower index
                    let (first_glyph_index, second_glyph_index) = if i < exit_glyph_index {
                        (i, exit_glyph_index)
                    } else {
                        (exit_glyph_index, i)
                    };

                    // Line-layout direction
                    // TODO: Handle vertical text
                    match self.direction {
                        TextDirection::LeftToRight => {
                            positions[first_glyph_index].hori_advance =
                                i32::from(entry_glyph_anchor.x)
                        }
                        TextDirection::RightToLeft => {
                            positions[first_glyph_index].hori_advance +=
                                i32::from(entry_glyph_anchor.x)
                        }
                    }

                    // Cross-stream direction
                    let dy = i32::from(exit_glyph_anchor.y) - i32::from(entry_glyph_anchor.y);
                    if rtl_flag {
                        positions[first_glyph_index].y_offset +=
                            dy + positions[second_glyph_index].y_offset;
                        if let Some(linked_index) = positions[first_glyph_index].cursive_attachment
                        {
                            adjust_cursive_chain(
                                dy,
                                self.direction,
                                usize::from(linked_index),
                                self.infos,
                                positions,
                            );
                        }
                    } else {
                        positions[second_glyph_index].y_offset +=
                            dy + positions[first_glyph_index].y_offset;
                        if let Some(linked_index) = positions[second_glyph_index].cursive_attachment
                        {
                            adjust_cursive_chain(
                                dy,
                                self.direction,
                                usize::from(linked_index),
                                self.infos,
                                positions,
                            );
                        }
                    }
                }
            }
        }
    }

    fn position_marks(&self, positions: &mut [GlyphPosition]) {
        for (i, info) in self.infos.iter().enumerate() {
            match info.placement {
                Placement::None
                | Placement::Distance(_, _)
                | Placement::CursiveAnchor(_, _, _, _) => {}
                Placement::MarkAnchor(base_index, _, _) => {
                    let base_pos = positions[base_index];
                    let (hori_advance_offset, vert_advance_offset) = match self.direction {
                        TextDirection::LeftToRight => sum_advance(positions.get(base_index..i)),
                        TextDirection::RightToLeft => sum_advance(positions.get(i..base_index)),
                    };

                    // Add the x & y offset of the base glyph to the mark
                    let position = &mut positions[i];
                    position.x_offset += base_pos.x_offset;
                    position.y_offset += base_pos.y_offset;

                    // Shift the mark back the advance of the base glyph and glyphs leading to it
                    // so that it is positioned above it
                    match self.direction {
                        TextDirection::LeftToRight => {
                            position.x_offset -= hori_advance_offset;
                            position.y_offset -= vert_advance_offset;
                        }
                        TextDirection::RightToLeft => {
                            position.x_offset += hori_advance_offset;
                            position.y_offset += vert_advance_offset;
                        }
                    }
                }
                Placement::MarkOverprint(base_index) => {
                    let base_pos = positions[base_index];
                    let position = &mut positions[i];
                    position.x_offset = base_pos.x_offset;
                    position.y_offset = base_pos.y_offset;
                }
            }
        }
    }
}

impl GlyphPosition {
    pub const fn new(hori_advance: i32, vert_advance: i32, x_offset: i32, y_offset: i32) -> Self {
        GlyphPosition {
            hori_advance,
            vert_advance,
            x_offset,
            y_offset,
            cursive_attachment: None,
        }
    }

    pub fn update(&mut self, hori_advance: i32, vert_advance: i32, x_offset: i32, y_offset: i32) {
        self.hori_advance = hori_advance;
        self.vert_advance = vert_advance;
        self.x_offset = x_offset;
        self.y_offset = y_offset;
    }

    pub fn update_advance(&mut self, hori_advance: i32, vert_advance: i32) {
        self.hori_advance = hori_advance;
        self.vert_advance = vert_advance;
    }
}

impl PartialEq for GlyphPosition {
    fn eq(&self, other: &Self) -> bool {
        self.hori_advance == other.hori_advance
            && self.vert_advance == other.vert_advance
            && self.x_offset == other.x_offset
            && self.y_offset == other.y_offset
    }
}

fn adjust_cursive_chain(
    delta: i32,
    direction: TextDirection,
    index: usize,
    infos: &[Info],
    positions: &mut [GlyphPosition],
) {
    let position = &mut positions[index];
    position.y_offset += delta;
    if let Some(next_index) = position.cursive_attachment {
        // TODO: prevent cycles
        adjust_cursive_chain(delta, direction, usize::from(next_index), infos, positions)
    }
}

fn sum_advance(positions: Option<&[GlyphPosition]>) -> (i32, i32) {
    positions.map_or((0, 0), |p| {
        p.iter().fold((0, 0), |(hori, vert), &pos| {
            (hori + pos.hori_advance, vert + pos.vert_advance)
        })
    })
}

fn glyph_advance<T: FontTableProvider>(
    font: &mut Font<T>,
    info: &Info,
    vertical: bool,
) -> Result<(i32, i32), ParseError> {
    let advance = if vertical && is_upright_glyph(info) {
        font.vertical_advance(info.get_glyph_index())
            .map(i32::from)
            .unwrap_or_else(|| {
                i32::from(font.hhea_table.ascender) - i32::from(font.hhea_table.descender)
            })
            + i32::from(info.kerning)
    } else {
        font.horizontal_advance(info.get_glyph_index())
            .map(i32::from)
            .ok_or(ParseError::MissingValue)?
            + i32::from(info.kerning)
    };
    Ok(if vertical { (0, advance) } else { (advance, 0) })
}

fn is_upright_glyph(info: &Info) -> bool {
    info.glyph.is_vert_alt()
        || info
            .glyph
            .unicodes
            .first()
            .is_some_and(|&ch| is_upright_char(ch))
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::path::Path;

    use super::*;
    use crate::binary::read::ReadScope;
    use crate::font::MatchingPresentation;
    use crate::font_data::FontData;
    use crate::gsub::{FeatureMask, Features};
    use crate::tag;
    use crate::tests::read_fixture;

    fn get_positions(
        text: &str,
        font: &str,
        script: u32,
        lang: u32,
        direction: TextDirection,
    ) -> Result<Vec<GlyphPosition>, Box<dyn Error>> {
        get_positions_with_gpos_features(
            text,
            font,
            script,
            lang,
            &Features::Mask(FeatureMask::default()),
            direction,
            false,
        )
    }

    fn get_positions_with_gpos_features(
        text: &str,
        font: &str,
        script: u32,
        lang: u32,
        features: &Features,
        direction: TextDirection,
        vertical: bool,
    ) -> Result<Vec<GlyphPosition>, Box<dyn Error>> {
        let path = Path::new("tests/fonts").join(font);
        let data = read_fixture(&path);
        let scope = ReadScope::new(&data);
        let font_file = scope.read::<FontData<'_>>()?;
        let provider = font_file.table_provider(0)?;
        let mut font = Font::new(provider)?;

        // Map text to glyphs and then apply font shaping
        let glyphs = font.map_glyphs(text, script, MatchingPresentation::NotRequired);
        let infos = font
            .shape(glyphs, script, Some(lang), features, None, true)
            .map_err(|(err, _info)| err)?;

        let mut layout = GlyphLayout::new(&mut font, &infos, direction, vertical);
        layout.glyph_positions().map_err(|err| err.into())
    }

    #[test]
    fn ltr_kerning() -> Result<(), Box<dyn Error>> {
        let script = tag::LATN;
        let lang = tag!(b"ENG ");
        // V gets kerned closer to A in AV
        let positions = get_positions(
            "AV AA",
            "opentype/Klei.otf",
            script,
            lang,
            TextDirection::LeftToRight,
        )?;
        let expected = &[
            GlyphPosition {
                hori_advance: 597,
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 758,
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 280,
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 777,
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 777,
                ..Default::default()
            },
        ];
        assert_eq!(positions, expected);
        Ok(())
    }

    #[test]
    fn ltr_mark_attach() -> Result<(), Box<dyn Error>> {
        let script = tag::KNDA;
        let lang = tag!(b"KAN ");
        // U+0CBC is a mark on U+0C9F
        let positions = get_positions(
            "\u{0C9F}\u{0CBC}",
            "noto/NotoSansKannada-Regular.ttf",
            script,
            lang,
            TextDirection::LeftToRight,
        )?;
        let expected = &[
            GlyphPosition {
                hori_advance: 1669,
                ..Default::default()
            },
            GlyphPosition {
                x_offset: -260,
                ..Default::default()
            },
        ];
        assert_eq!(positions, expected);
        Ok(())
    }

    #[test]
    fn ltr_attach_distance() -> Result<(), Box<dyn Error>> {
        let script = tag!(b"latn");
        let lang = tag!(b"ENG ");
        let features = Features::Mask(FeatureMask::default() | FeatureMask::FRAC);
        // '⁄' is U+2044 FRACTION SLASH, which when the `frac` GPOS feature is enabled is
        // positioned to be under the previous character and above the next.
        let positions = get_positions_with_gpos_features(
            "1⁄99",
            "opentype/SourceCodePro-Regular.otf",
            script,
            lang,
            &features,
            TextDirection::LeftToRight,
            false,
        )?;
        let expected = &[
            GlyphPosition {
                hori_advance: 600,
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 0,
                x_offset: -300,
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 600,
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 600,
                ..Default::default()
            },
        ];
        assert_eq!(positions, expected);
        Ok(())
    }

    #[test]
    fn ltr_mark_overprint() -> Result<(), Box<dyn Error>> {
        let script = tag::LATN;
        let lang = tag!(b"ENG ");
        // TerminusTTF does not have GPOS or GSUB tables. As a result it hits the fallback mark
        // handling code that results in characters belonging to the Nonspacing Mark General
        // Category to be Mark::Overprint. Combining characters are examples of such characters.
        // This test is 'a' followed by COMBINING TILDE.
        let positions = get_positions(
            "a\u{0303}",
            "opentype/TerminusTTF-4.47.0.ttf",
            script,
            lang,
            TextDirection::LeftToRight,
        )?;
        let expected = &[
            GlyphPosition {
                hori_advance: 500,
                ..Default::default()
            },
            GlyphPosition::default(),
        ];
        assert_eq!(positions, expected);
        Ok(())
    }

    #[test]
    fn ltr_cursive() -> Result<(), Box<dyn Error>> {
        let script = tag::KNDA;
        let lang = tag!(b"KAN ");
        // Text is RTL Arabic with cursive connections
        let positions = get_positions(
            "ಇನ್ಫ್ಲೆಕ್ಷನ್",
            "noto/NotoSansKannada-Regular.ttf",
            script,
            lang,
            TextDirection::LeftToRight,
        )?;
        // [8=0+1457|256=1+1456|118=1+346|335=1+791|282=7+1176|186=10+2096]
        let expected = &[
            GlyphPosition {
                hori_advance: 1457,
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 1456,
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 346, // This glyph's advance gets adjusted to align with the next one
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 791,
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 1176,
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 2096,
                ..Default::default()
            },
        ];
        assert_eq!(positions, expected);
        Ok(())
    }

    #[test]
    fn rtl_cursive() -> Result<(), Box<dyn Error>> {
        let script = tag::ARAB;
        let lang = tag!(b"URD ");
        // Text is RTL Arabic with cursive connections
        let positions = get_positions(
            "لسان",
            "arabic/NafeesNastaleeq.ttf",
            script,
            lang,
            TextDirection::RightToLeft,
        )?;
        let expected = &[
            GlyphPosition {
                hori_advance: 391,
                y_offset: -409,
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 989,
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 213,
                ..Default::default()
            },
            GlyphPosition {
                hori_advance: 1561,
                ..Default::default()
            },
        ];
        assert_eq!(positions, expected);
        Ok(())
    }
}
