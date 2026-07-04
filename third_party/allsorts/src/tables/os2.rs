//! Parsing of the `OS/2` table.
//!
//! > The OS/2 table consists of a set of metrics and other data that are required in OpenType fonts.
//!
//! — <https://docs.microsoft.com/en-us/typography/opentype/spec/os2>

use std::convert::TryInto;

use bitflags::bitflags;

use crate::binary::read::{ReadBinaryDep, ReadCtxt};
use crate::binary::write::{WriteBinary, WriteContext};
use crate::binary::{I16Be, U16Be, U32Be};
use crate::error::{ParseError, WriteError};
use crate::tables::Fixed;

/// `OS/2` table
///
/// <https://docs.microsoft.com/en-us/typography/opentype/spec/os2>
#[derive(Clone)]
pub struct Os2 {
    pub version: u16,
    pub x_avg_char_width: i16,
    pub us_weight_class: u16,
    pub us_width_class: u16,
    pub fs_type: u16,
    pub y_subscript_x_size: i16,
    pub y_subscript_y_size: i16,
    pub y_subscript_x_offset: i16,
    pub y_subscript_y_offset: i16,
    pub y_superscript_x_size: i16,
    pub y_superscript_y_size: i16,
    pub y_superscript_x_offset: i16,
    pub y_superscript_y_offset: i16,
    pub y_strikeout_size: i16,
    pub y_strikeout_position: i16,
    pub s_family_class: i16,
    pub panose: [u8; 10],
    pub ul_unicode_range1: u32,
    pub ul_unicode_range2: u32,
    pub ul_unicode_range3: u32,
    pub ul_unicode_range4: u32,
    pub ach_vend_id: u32, // tag
    pub fs_selection: FsSelection,
    pub us_first_char_index: u16,
    pub us_last_char_index: u16,
    // Note: Documentation for OS/2 version 0 in Apple’s TrueType Reference Manual stops at the
    // usLastCharIndex field and does not include the last five fields of the table as it was
    // defined by Microsoft. Some legacy TrueType fonts may have been built with a shortened
    // version 0 OS/2 table. Applications should check the table length for a version 0 OS/2 table
    // before reading these fields.
    pub version0: Option<Version0>,
    pub version1: Option<Version1>,
    pub version2to4: Option<Version2to4>,
    pub version5: Option<Version5>,
}

#[derive(Clone)]
pub struct Version0 {
    pub s_typo_ascender: i16,
    pub s_typo_descender: i16,
    pub s_typo_line_gap: i16,
    pub us_win_ascent: u16,
    pub us_win_descent: u16,
}

#[derive(Clone)]
pub struct Version1 {
    pub ul_code_page_range1: u32,
    pub ul_code_page_range2: u32,
}

#[derive(Clone)]
pub struct Version2to4 {
    pub s_x_height: i16,
    pub s_cap_height: i16,
    pub us_default_char: u16,
    pub us_break_char: u16,
    pub us_max_context: u16,
}

#[derive(Clone)]
pub struct Version5 {
    pub us_lower_optical_point_size: u16,
    pub us_upper_optical_point_size: u16,
}

bitflags! {
    /// fsSelection field in `OS/2`
    ///
    /// ```text
    /// Bit #  macStyle bit  C definition     Description
    /// 0      bit 1         ITALIC           Font contains italic or oblique glyphs, otherwise
    ///                                       they are upright.
    /// 1                    UNDERSCORE       glyphs are underscored.
    /// 2                    NEGATIVE         glyphs have their foreground and background reversed.
    /// 3                    OUTLINED         Outline (hollow) glyphs, otherwise they are solid.
    /// 4                    STRIKEOUT        glyphs are overstruck.
    /// 5      bit 0         BOLD             glyphs are emboldened.
    /// 6                    REGULAR          glyphs are in the standard weight/style for the font.
    /// 7                    USE_TYPO_METRICS If set, it is strongly recommended that applications
    ///                                       use OS/2.sTypoAscender - OS/2.sTypoDescender +
    ///                                       OS/2.sTypoLineGap as the default line spacing for
    ///                                       this font.
    /// 8                    WWS              The font has 'name' table strings consistent with a
    ///                                       weight/width/slope family without requiring use of
    ///                                       name IDs 21 and 22.
    /// 9                    OBLIQUE          Font contains oblique glyphs.
    /// 10–15                <reserved>       Reserved; set to 0.
    /// ```
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct FsSelection: u16 {
        const ITALIC = 1 << 0;
        const UNDERSCORE = 1 << 1;
        const NEGATIVE = 1 << 2;
        const OUTLINED = 1 << 3;
        const STRIKEOUT = 1 << 4;
        const BOLD = 1 << 5;
        const REGULAR = 1 << 6;
        const USE_TYPO_METRICS = 1 << 7;
        const WWS = 1 << 8;
        const OBLIQUE = 1 << 9;
        // 10–15 Reserved; set to 0.
    }
}

impl Os2 {
    /// Map a width value to an OS/2 width class
    pub(crate) fn value_to_width_class(value: Fixed) -> u16 {
        const WIDTH_CLASS_MAP: &[(Fixed, u16)] = &[
            // using from_raw so it works in const context
            (Fixed::from_raw(3276800), 1),  // 50.0
            (Fixed::from_raw(4096000), 2),  // 62.5
            (Fixed::from_raw(4915200), 3),  // 75.0
            (Fixed::from_raw(5734400), 4),  // 87.5
            (Fixed::from_raw(6553600), 5),  // 100.0
            (Fixed::from_raw(7372800), 6),  // 112.5
            (Fixed::from_raw(8192000), 7),  // 125.0
            (Fixed::from_raw(9830400), 8),  // 150.0
            (Fixed::from_raw(13107200), 9), // 200.0
        ];

        // Map the value to one of the width classes. Width is a percentage and can be
        // anything > 1 but classes are only defined for 50% to 200%.
        match WIDTH_CLASS_MAP.binary_search_by_key(&value, |&(val, _cls)| val) {
            Ok(i) => WIDTH_CLASS_MAP[i].1,
            Err(i) => {
                // get the values at i and i-1, choose the one it's closer to
                if i < 1 {
                    return WIDTH_CLASS_MAP[0].1;
                }
                if i >= WIDTH_CLASS_MAP.len() {
                    return WIDTH_CLASS_MAP.last().unwrap().1;
                }
                let (a, clsa) = WIDTH_CLASS_MAP[i - 1];
                let (b, clsb) = WIDTH_CLASS_MAP[i];
                if (value - a) > (b - value) {
                    clsb
                } else {
                    clsa
                }
            }
        }
    }
}

impl ReadBinaryDep for Os2 {
    type HostType<'a> = Self;
    type Args<'a> = usize;

    // The format of this table has changed over time. The original TrueType specification had this
    // table at 68 bytes long. The first OpenType version had it at 78 bytes long, and the current
    // OpenType version is even larger. To determine which kind of table your software is dealing
    // with, it's best both to consider the table's version and its size.
    fn read_dep(ctxt: &mut ReadCtxt<'_>, table_size: usize) -> Result<Self, ParseError> {
        let version = ctxt.read::<U16Be>()?;
        let x_avg_char_width = ctxt.read::<I16Be>()?;
        let us_weight_class = ctxt.read::<U16Be>()?;
        let us_width_class = ctxt.read::<U16Be>()?;
        let fs_type = ctxt.read::<U16Be>()?;
        let y_subscript_x_size = ctxt.read::<I16Be>()?;
        let y_subscript_y_size = ctxt.read::<I16Be>()?;
        let y_subscript_x_offset = ctxt.read::<I16Be>()?;
        let y_subscript_y_offset = ctxt.read::<I16Be>()?;
        let y_superscript_x_size = ctxt.read::<I16Be>()?;
        let y_superscript_y_size = ctxt.read::<I16Be>()?;
        let y_superscript_x_offset = ctxt.read::<I16Be>()?;
        let y_superscript_y_offset = ctxt.read::<I16Be>()?;
        let y_strikeout_size = ctxt.read::<I16Be>()?;
        let y_strikeout_position = ctxt.read::<I16Be>()?;
        let s_family_class = ctxt.read::<I16Be>()?;
        // NOTE(unwrap): Safe as slice is guaranteed to have 10 elements
        let panose: [u8; 10] = ctxt.read_slice(10)?.try_into().unwrap();
        let ul_unicode_range1 = ctxt.read::<U32Be>()?;
        let ul_unicode_range2 = ctxt.read::<U32Be>()?;
        let ul_unicode_range3 = ctxt.read::<U32Be>()?;
        let ul_unicode_range4 = ctxt.read::<U32Be>()?;
        let ach_vend_id = ctxt.read::<U32Be>()?;
        let fs_selection = ctxt.read::<U16Be>().map(FsSelection::from_bits_truncate)?;
        let us_first_char_index = ctxt.read::<U16Be>()?;
        let us_last_char_index = ctxt.read::<U16Be>()?;

        // Read version specific fields
        let version0 = if table_size >= 78 {
            let s_typo_ascender = ctxt.read::<I16Be>()?;
            let s_typo_descender = ctxt.read::<I16Be>()?;
            let s_typo_line_gap = ctxt.read::<I16Be>()?;
            let us_win_ascent = ctxt.read::<U16Be>()?;
            let us_win_descent = ctxt.read::<U16Be>()?;
            Some(Version0 {
                s_typo_ascender,
                s_typo_descender,
                s_typo_line_gap,
                us_win_ascent,
                us_win_descent,
            })
        } else {
            None
        };

        let version1 = if version >= 1 {
            let ul_code_page_range1 = ctxt.read::<U32Be>()?;
            let ul_code_page_range2 = ctxt.read::<U32Be>()?;
            Some(Version1 {
                ul_code_page_range1,
                ul_code_page_range2,
            })
        } else {
            None
        };

        let version2to4 = if version >= 2 {
            let s_x_height = ctxt.read::<I16Be>()?;
            let s_cap_height = ctxt.read::<I16Be>()?;
            let us_default_char = ctxt.read::<U16Be>()?;
            let us_break_char = ctxt.read::<U16Be>()?;
            let us_max_context = ctxt.read::<U16Be>()?;
            Some(Version2to4 {
                s_x_height,
                s_cap_height,
                us_default_char,
                us_break_char,
                us_max_context,
            })
        } else {
            None
        };

        let version5 = if version >= 5 {
            let us_lower_optical_point_size = ctxt.read::<U16Be>()?;
            let us_upper_optical_point_size = ctxt.read::<U16Be>()?;
            Some(Version5 {
                us_lower_optical_point_size,
                us_upper_optical_point_size,
            })
        } else {
            None
        };

        Ok(Os2 {
            version,
            x_avg_char_width,
            us_weight_class,
            us_width_class,
            fs_type,
            y_subscript_x_size,
            y_subscript_y_size,
            y_subscript_x_offset,
            y_subscript_y_offset,
            y_superscript_x_size,
            y_superscript_y_size,
            y_superscript_x_offset,
            y_superscript_y_offset,
            y_strikeout_size,
            y_strikeout_position,
            s_family_class,
            panose,
            ul_unicode_range1,
            ul_unicode_range2,
            ul_unicode_range3,
            ul_unicode_range4,
            ach_vend_id,
            fs_selection,
            us_first_char_index,
            us_last_char_index,
            version0,
            version1,
            version2to4,
            version5,
        })
    }
}

impl WriteBinary<&Self> for Os2 {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, table: &Self) -> Result<Self::Output, WriteError> {
        // TODO: make impossible states unrepresentable
        // The way the data is structured means that it's possible to have say a v5 struct present
        // but the preceding ones absent, which is invalid.
        let version = if table.version5.is_some() {
            5_u16
        } else if table.version2to4.is_some() {
            4
        } else if table.version1.is_some() {
            1
        } else {
            0
        };

        U16Be::write(ctxt, version)?;
        I16Be::write(ctxt, table.x_avg_char_width)?;
        U16Be::write(ctxt, table.us_weight_class)?;
        U16Be::write(ctxt, table.us_width_class)?;
        U16Be::write(ctxt, table.fs_type)?;
        I16Be::write(ctxt, table.y_subscript_x_size)?;
        I16Be::write(ctxt, table.y_subscript_y_size)?;
        I16Be::write(ctxt, table.y_subscript_x_offset)?;
        I16Be::write(ctxt, table.y_subscript_y_offset)?;
        I16Be::write(ctxt, table.y_superscript_x_size)?;
        I16Be::write(ctxt, table.y_superscript_y_size)?;
        I16Be::write(ctxt, table.y_superscript_x_offset)?;
        I16Be::write(ctxt, table.y_superscript_y_offset)?;
        I16Be::write(ctxt, table.y_strikeout_size)?;
        I16Be::write(ctxt, table.y_strikeout_position)?;
        I16Be::write(ctxt, table.s_family_class)?;
        ctxt.write_bytes(&table.panose)?;
        U32Be::write(ctxt, table.ul_unicode_range1)?;
        U32Be::write(ctxt, table.ul_unicode_range2)?;
        U32Be::write(ctxt, table.ul_unicode_range3)?;
        U32Be::write(ctxt, table.ul_unicode_range4)?;
        U32Be::write(ctxt, table.ach_vend_id)?;
        U16Be::write(ctxt, table.fs_selection.bits())?;
        U16Be::write(ctxt, table.us_first_char_index)?;
        U16Be::write(ctxt, table.us_last_char_index)?;

        if let Some(v0) = &table.version0 {
            Version0::write(ctxt, v0)?;
        }
        if let Some(v1) = &table.version1 {
            Version1::write(ctxt, v1)?;
        }
        if let Some(v2) = &table.version2to4 {
            Version2to4::write(ctxt, v2)?;
        }
        if let Some(v5) = &table.version5 {
            Version5::write(ctxt, v5)?;
        }
        Ok(())
    }
}

impl WriteBinary<&Self> for Version0 {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, table: &Self) -> Result<Self::Output, WriteError> {
        I16Be::write(ctxt, table.s_typo_ascender)?;
        I16Be::write(ctxt, table.s_typo_descender)?;
        I16Be::write(ctxt, table.s_typo_line_gap)?;
        U16Be::write(ctxt, table.us_win_ascent)?;
        U16Be::write(ctxt, table.us_win_descent)?;
        Ok(())
    }
}

impl WriteBinary<&Self> for Version1 {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, table: &Self) -> Result<Self::Output, WriteError> {
        U32Be::write(ctxt, table.ul_code_page_range1)?;
        U32Be::write(ctxt, table.ul_code_page_range2)?;
        Ok(())
    }
}

impl WriteBinary<&Self> for Version2to4 {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, table: &Self) -> Result<Self::Output, WriteError> {
        I16Be::write(ctxt, table.s_x_height)?;
        I16Be::write(ctxt, table.s_cap_height)?;
        U16Be::write(ctxt, table.us_default_char)?;
        U16Be::write(ctxt, table.us_break_char)?;
        U16Be::write(ctxt, table.us_max_context)?;
        Ok(())
    }
}

impl WriteBinary<&Self> for Version5 {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, table: &Self) -> Result<Self::Output, WriteError> {
        U16Be::write(ctxt, table.us_lower_optical_point_size)?;
        U16Be::write(ctxt, table.us_upper_optical_point_size)?;
        Ok(())
    }
}

/// Constant array defining Unicode ranges and their corresponding bit index in the 128-bit mask.
///
/// Each tuple contains:
/// - the start of the range (inclusive),
/// - the end of the range (inclusive),
/// - the bit index (i.e. which bit should be set).
const UNICODE_RANGES: &[(u32, u32, u32)] = &[
    (0x0000, 0x007F, 0),  // Basic Latin
    (0x0080, 0x00FF, 1),  // Latin-1 Supplement
    (0x0100, 0x017F, 2),  // Latin Extended-A
    (0x0180, 0x024F, 3),  // Latin Extended-B
    (0x0250, 0x02AF, 4),  // IPA Extensions
    (0x02B0, 0x02FF, 5),  // Spacing Modifier Letters
    (0x0300, 0x036F, 6),  // Combining Diacritical Marks
    (0x0370, 0x03FF, 7),  // Greek and Coptic
    (0x0400, 0x04FF, 9),  // Cyrillic
    (0x0530, 0x058F, 10), // Armenian
    (0x0590, 0x05FF, 11), // Hebrew
    (0x0600, 0x06FF, 12), // Arabic
    (0x0700, 0x074F, 13), // Syriac
    (0x0750, 0x077F, 14), // Arabic Supplement
    (0x0780, 0x07BF, 15), // Thaana
    (0x07C0, 0x07FF, 16), // NKo
    (0x0800, 0x083F, 17), // Samaritan
    (0x0840, 0x085F, 18), // Mandaic
    (0x0860, 0x086F, 19), // Syriac Supplement
    (0x08A0, 0x08FF, 20), // Arabic Extended-A
    (0x0900, 0x097F, 21), // Devanagari
    (0x0980, 0x09FF, 22), // Bengali
    (0x0A00, 0x0A7F, 23), // Gurmukhi
    (0x0A80, 0x0AFF, 24), // Gujarati
    (0x0B00, 0x0B7F, 25), // Oriya
    (0x0B80, 0x0BFF, 26), // Tamil
    (0x0C00, 0x0C7F, 27), // Telugu
    (0x0C80, 0x0CFF, 28), // Kannada
    (0x0D00, 0x0D7F, 29), // Malayalam
    (0x0D80, 0x0DFF, 30), // Sinhala
    (0x0E00, 0x0E7F, 31), // Thai
    (0x0E80, 0x0EFF, 32), // Lao
    (0x0F00, 0x0FFF, 33), // Tibetan
    (0x1000, 0x109F, 34), // Myanmar
    (0x10A0, 0x10FF, 35), // Georgian
    (0x1100, 0x11FF, 36), // Hangul Jamo
    (0x1E00, 0x1EFF, 37), // Latin Extended Additional
    (0x1F00, 0x1FFF, 38), // Greek Extended
];

/// Map a Unicode codepoint to a 128-bit mask for the ulUnicodeRange field.
///
/// This function iterates over the array of defined ranges and returns a mask with the bit
/// corresponding to the Unicode block set if the input codepoint falls within that range.
/// If no range matches, it returns 0.
pub(crate) fn unicode_range_mask(ch: u32) -> u128 {
    for &(start, end, bit) in UNICODE_RANGES.iter() {
        if (start..=end).contains(&ch) {
            return 1 << bit;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "prince")]
    fn test_read() {
        // Imports are in here to stop warnings when prince feature is not enabled
        use crate::binary::read::ReadScope;
        use crate::tables::{FontTableProvider, OpenTypeFont};
        use crate::tag;
        use crate::tests::read_fixture;

        let buffer = read_fixture("../../../tests/data/fonts/HardGothicNormal.ttf");
        let opentype_file = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();
        let provider = opentype_file.table_provider(0).unwrap();
        let os_2_data = provider.read_table_data(tag::OS_2).unwrap();

        let os_2 = ReadScope::new(&os_2_data)
            .read_dep::<Os2>(os_2_data.len())
            .expect("unable to parse OS/2 table");
        assert_eq!(os_2.version, 1);
        assert!(os_2.version0.is_some());
        assert!(os_2.version1.is_some());
        assert!(os_2.version2to4.is_none());
        assert!(os_2.version5.is_none());
    }

    #[test]
    #[cfg(feature = "prince")]
    fn test_write() {
        // Imports are in here to stop warnings when prince feature is not enabled
        use crate::binary::read::ReadScope;
        use crate::binary::write::WriteBuffer;
        use crate::tables::{FontTableProvider, OpenTypeFont};
        use crate::tag;
        use crate::tests::read_fixture;

        let buffer = read_fixture("../../../tests/data/fonts/HardGothicNormal.ttf");
        let opentype_file = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();
        let provider = opentype_file.table_provider(0).unwrap();
        let os_2_data = provider.read_table_data(tag::OS_2).unwrap();

        let os_2 = ReadScope::new(&os_2_data)
            .read_dep::<Os2>(os_2_data.len())
            .expect("unable to parse OS/2 table");

        let mut out = WriteBuffer::new();
        Os2::write(&mut out, &os_2).unwrap();
        let written = out.into_inner();
        assert_eq!(written.as_slice(), &*os_2_data);
    }

    #[test]
    fn map_weight_class() {
        assert_eq!(Os2::value_to_width_class(Fixed::from(0.)), 1);
        assert_eq!(Os2::value_to_width_class(Fixed::from(1.)), 1);
        assert_eq!(Os2::value_to_width_class(Fixed::from(50.)), 1);
        assert_eq!(Os2::value_to_width_class(Fixed::from(51.)), 1);
        assert_eq!(Os2::value_to_width_class(Fixed::from(60.)), 2);
        assert_eq!(Os2::value_to_width_class(Fixed::from(150.)), 8);
        assert_eq!(Os2::value_to_width_class(Fixed::from(300.)), 9);
    }
}
