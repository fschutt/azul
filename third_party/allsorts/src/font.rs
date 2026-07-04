//! Central font handling support.

use std::borrow::Cow;
use std::convert::{self};
use std::sync::Arc;

use bitflags::bitflags;
use pathfinder_geometry::line_segment::LineSegment2F;
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::Vector2F;
use tinyvec::tiny_vec;

use crate::big5::unicode_to_big5;
use crate::binary::read::ReadScope;
use crate::bitmap::cbdt::{CBDTTable, CBLCTable};
use crate::bitmap::sbix::Sbix as SbixTable;
use crate::bitmap::{BitDepth, BitmapGlyph};
use crate::cff::cff2::CFF2;
use crate::cff::outline::{CFF2Outlines, CFFOutlines};
use crate::cff::{CFFError, CFF};
use crate::context::Glyph;
use crate::error::{ParseError, ShapingError};
use crate::font::tables::ColrCpalTryBuilder;
use crate::glyph_info::GlyphNames;
use crate::gpos::Info;
use crate::gsub::{Features, GlyphOrigin, RawGlyph, RawGlyphFlags};
use crate::layout::morx;
use crate::layout::{new_layout_cache, GDEFTable, LayoutCache, LayoutTable, GPOS, GSUB};
use crate::macroman::char_to_macroman;
use crate::outline::{BoundingBoxSink, OutlineBuilder, OutlineSink};
use crate::scripts::preprocess_text;
use crate::tables::aat::DELETED_GLYPH;
use crate::tables::cmap::{Cmap, CmapSubtable, EncodingId, EncodingRecord, PlatformId};
use crate::tables::colr::{ColrTable, Painter};
use crate::tables::cpal::CpalTable;
use crate::tables::glyf::{BoundingBox, GlyfVisitorContext, LocaGlyf};
use crate::tables::kern::owned::KernTable;
use crate::tables::loca::{owned, LocaTable};
use crate::tables::morx::MorxTable;
use crate::tables::os2::Os2;
use crate::tables::svg::SvgTable;
use crate::tables::variable_fonts::fvar::{FvarAxisCount, FvarTable, Tuple, VariationAxisRecord};
use crate::tables::{
    kern, read_and_box_optional_table, read_and_box_table, FontTableProvider, HeadTable, HheaTable,
    MaxpTable,
};
use crate::unicode::{self, VariationSelector};
use crate::variations::{AxisNamesError, NamedAxis};
use crate::{cff, glyph_info, tag, variations, GlyphId, SafeFrom};
use crate::{gpos, gsub, DOTTED_CIRCLE};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Encoding {
    Unicode = 1,
    Symbol = 2,
    AppleRoman = 3,
    Big5 = 4,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum OutlineFormat {
    Glyf,
    Cff,
    Svg,
    None,
}

enum LazyLoad<T> {
    NotLoaded,
    Loaded(Option<T>),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MatchingPresentation {
    Required,
    NotRequired,
}

/// For now `GlyphCache` only stores the index of U+25CC DOTTED CIRCLE. The intention is for this
/// to become a more general cache in the future.
///
/// `None` indicates that dotted circle has never been looked up. A value otherwise is the index of
/// the glyph.
struct GlyphCache(Option<(u16, VariationSelector)>);

/// Core type for loading a font in order to perform glyph mapping and font shaping.
pub struct Font<T: FontTableProvider> {
    pub font_table_provider: T,
    pub head_table: HeadTable,
    cmap_table: Box<[u8]>,
    pub maxp_table: MaxpTable,
    hmtx_table: Box<[u8]>,
    pub hhea_table: HheaTable,
    vmtx_table: LazyLoad<Arc<[u8]>>,
    vhea_table: LazyLoad<Arc<HheaTable>>,
    cmap_subtable_offset: usize,
    pub cmap_subtable_encoding: Encoding,
    gdef_cache: LazyLoad<Arc<GDEFTable>>,
    morx_cache: LazyLoad<Arc<tables::Morx>>,
    gsub_cache: LazyLoad<LayoutCache<GSUB>>,
    gpos_cache: LazyLoad<LayoutCache<GPOS>>,
    kern_cache: LazyLoad<Arc<KernTable>>,
    os2_us_first_char_index: LazyLoad<u16>,
    glyph_cache: GlyphCache,
    pub glyph_table_flags: GlyphTableFlags,
    loca_glyf: LocaGlyf,
    cff_cache: LazyLoad<Arc<tables::CFF>>,
    cff2_cache: LazyLoad<Arc<tables::CFF2>>,
    embedded_image_filter: GlyphTableFlags,
    embedded_images: LazyLoad<Arc<Images>>,
    axis_count: u16,
}

pub enum Images {
    Embedded {
        cblc: tables::CBLC,
        cbdt: tables::CBDT,
    },
    Colr(tables::ColrCpal),
    Sbix(tables::Sbix),
    Svg(tables::Svg),
}

// Inhibit warning: field `data` is never read on CFF and CFF2 structs
#[allow(unused)]
mod tables {
    use ouroboros::self_referencing;

    use crate::bitmap::cbdt::{CBDTTable, CBLCTable};
    use crate::bitmap::sbix::Sbix as SbixTable;
    use crate::cff::cff2::CFF2 as CFF2Table;
    use crate::cff::CFF as CFFTable;
    use crate::tables::colr::ColrTable;
    use crate::tables::cpal::CpalTable;
    use crate::tables::morx::MorxTable;
    use crate::tables::svg::SvgTable;

    #[self_referencing(pub_extras)]
    pub struct CFF {
        data: Box<[u8]>,
        #[borrows(data)]
        #[not_covariant]
        table: CFFTable<'this>,
    }

    #[self_referencing(pub_extras)]
    pub struct CFF2 {
        data: Box<[u8]>,
        #[borrows(data)]
        #[not_covariant]
        table: CFF2Table<'this>,
    }

    #[self_referencing(pub_extras)]
    pub struct CBLC {
        data: Box<[u8]>,
        #[borrows(data)]
        #[not_covariant]
        pub(crate) table: CBLCTable<'this>,
    }

    #[self_referencing(pub_extras)]
    pub struct CBDT {
        data: Box<[u8]>,
        #[borrows(data)]
        #[covariant]
        pub(crate) table: CBDTTable<'this>,
    }

    #[self_referencing(pub_extras)]
    pub struct ColrCpal {
        colr_data: Box<[u8]>,
        cpal_data: Box<[u8]>,
        #[borrows(colr_data)]
        #[not_covariant]
        pub(crate) colr: ColrTable<'this>,
        #[borrows(cpal_data)]
        #[not_covariant]
        pub(crate) cpal: CpalTable<'this>,
    }

    #[self_referencing(pub_extras)]
    pub struct Sbix {
        data: Box<[u8]>,
        #[borrows(data)]
        #[not_covariant]
        pub(crate) table: SbixTable<'this>,
    }

    #[self_referencing(pub_extras)]
    pub struct Svg {
        data: Box<[u8]>,
        #[borrows(data)]
        #[not_covariant]
        pub(crate) table: SvgTable<'this>,
    }

    #[self_referencing(pub_extras)]
    pub struct Morx {
        data: Box<[u8]>,
        #[borrows(data)]
        #[not_covariant]
        pub(crate) table: MorxTable<'this>,
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct GlyphTableFlags: u8 {
        const GLYF = 1 << 0;
        const CFF  = 1 << 1;
        const SVG  = 1 << 2;
        const SBIX = 1 << 3;
        const CBDT = 1 << 4;
        const EBDT = 1 << 5;
        const CFF2 = 1 << 6;
        const COLR = 1 << 7;
    }
}

const TABLE_TAG_FLAGS: &[(u32, GlyphTableFlags)] = &[
    (tag::GLYF, GlyphTableFlags::GLYF),
    (tag::CFF, GlyphTableFlags::CFF),
    (tag::CFF2, GlyphTableFlags::CFF2),
    (tag::COLR, GlyphTableFlags::COLR),
    (tag::SVG, GlyphTableFlags::SVG),
    (tag::SBIX, GlyphTableFlags::SBIX),
    (tag::CBDT, GlyphTableFlags::CBDT),
    (tag::EBDT, GlyphTableFlags::EBDT),
];

impl<T: FontTableProvider> Font<T> {
    /// Construct a new instance from a type that can supply font tables.
    pub fn new(provider: T) -> Result<Font<T>, ParseError> {
        let cmap_table = read_and_box_table(&provider, tag::CMAP)?;

        match charmap_info(&cmap_table)? {
            Some((cmap_subtable_encoding, cmap_subtable_offset)) => {
                let head_table =
                    ReadScope::new(&provider.read_table_data(tag::HEAD)?).read::<HeadTable>()?;
                let maxp_table =
                    ReadScope::new(&provider.read_table_data(tag::MAXP)?).read::<MaxpTable>()?;
                let hmtx_table = read_and_box_table(&provider, tag::HMTX)?;
                let hhea_table =
                    ReadScope::new(&provider.read_table_data(tag::HHEA)?).read::<HheaTable>()?;
                let fvar_data = provider.table_data(tag::FVAR)?;
                let fvar_axis_count = fvar_data
                    .as_deref()
                    .map(|data| ReadScope::new(data).read::<FvarAxisCount>())
                    .transpose()?
                    .unwrap_or(0);

                let embedded_image_filter = GlyphTableFlags::SVG
                    | GlyphTableFlags::SBIX
                    | GlyphTableFlags::CBDT
                    | GlyphTableFlags::COLR;
                let mut glyph_table_flags = GlyphTableFlags::empty();
                for &(table, flag) in TABLE_TAG_FLAGS {
                    if provider.has_table(table) {
                        glyph_table_flags |= flag
                    }
                }

                Ok(Font {
                    font_table_provider: provider,
                    head_table,
                    cmap_table,
                    maxp_table,
                    hmtx_table,
                    hhea_table,
                    vmtx_table: LazyLoad::NotLoaded,
                    vhea_table: LazyLoad::NotLoaded,
                    cmap_subtable_offset: usize::safe_from(cmap_subtable_offset),
                    cmap_subtable_encoding,
                    gdef_cache: LazyLoad::NotLoaded,
                    morx_cache: LazyLoad::NotLoaded,
                    gsub_cache: LazyLoad::NotLoaded,
                    gpos_cache: LazyLoad::NotLoaded,
                    kern_cache: LazyLoad::NotLoaded,
                    os2_us_first_char_index: LazyLoad::NotLoaded,
                    glyph_cache: GlyphCache::new(),
                    glyph_table_flags,
                    loca_glyf: LocaGlyf::new(),
                    cff_cache: LazyLoad::NotLoaded,
                    cff2_cache: LazyLoad::NotLoaded,
                    embedded_image_filter,
                    embedded_images: LazyLoad::NotLoaded,
                    axis_count: fvar_axis_count,
                })
            }
            None => Err(ParseError::UnsuitableCmap),
        }
    }

    /// Returns the number of glyphs in the font.
    pub fn num_glyphs(&self) -> u16 {
        self.maxp_table.num_glyphs
    }

    /// Set the embedded image table filter.
    ///
    /// When determining if a font contains embedded images, as well as retrieving images this
    /// value is used to set which tables are consulted. By default, it is set to only consult
    /// tables that can contain colour images (`CBDT`/`CBLC`, `sbix`, and `SVG`). You can change
    /// the value to exclude certain tables or opt into tables that can only contain B&W images
    /// (`EBDT`/`EBLC`).
    pub fn set_embedded_image_filter(&mut self, flags: GlyphTableFlags) {
        self.embedded_image_filter = flags;
    }

    /// Look up the glyph index for the supplied character in the font.
    pub fn lookup_glyph_index(
        &mut self,
        ch: char,
        match_presentation: MatchingPresentation,
        variation_selector: Option<VariationSelector>,
    ) -> (u16, VariationSelector) {
        self.glyph_cache.get(ch).unwrap_or_else(|| {
            let (glyph_index, used_variation) =
                self.map_unicode_to_glyph(ch, match_presentation, variation_selector);
            self.glyph_cache.put(ch, glyph_index, used_variation);
            (glyph_index, used_variation)
        })
    }

    /// Convenience method to shape the supplied glyphs.
    ///
    /// The method maps applies glyph substitution (`gsub`) and glyph positioning (`gpos`). Use
    /// `map_glyphs` to turn text into glyphs that can be accepted by this method.
    ///
    /// **Arguments:**
    ///
    /// * `glyphs`: the glyphs to be shaped in logical order.
    /// * `script_tag`: the [OpenType script tag](https://docs.microsoft.com/en-us/typography/opentype/spec/scripttags) of the text.
    /// * `opt_lang_tag`: the [OpenType language tag](https://docs.microsoft.com/en-us/typography/opentype/spec/languagetags) of the text.
    /// * `features`: the [OpenType features](https://docs.microsoft.com/en-us/typography/opentype/spec/featuretags) to enable.
    /// * `kerning`: when applying `gpos` if this argument is `true` the `kern` OpenType feature
    ///   is enabled for non-complex scripts. If it is `false` then the `kern` feature is not
    ///   enabled for non-complex scripts.
    ///
    /// **Error Handling:**
    ///
    /// This method will continue on in the face of errors, applying what it can. If no errors are
    /// encountered this method returns `Ok(Vec<Info>)`. If one or more errors are encountered
    /// `Err((ShapingError, Vec<Info>))` is returned. The first error encountered is returned
    /// as the first item of the tuple. `Vec<Info>` is returned as the second item of the tuple,
    /// allowing consumers of the shaping output to proceed even if errors are encountered.
    /// However, in the error case the glyphs might not have had GPOS or GSUB applied.
    ///
    /// ## Example
    ///
    /// ```
    /// use allsorts::binary::read::ReadScope;
    /// use allsorts::font::MatchingPresentation;
    /// use allsorts::font_data::FontData;
    /// use allsorts::gsub::{self, FeatureMask, Features};
    /// use allsorts::DOTTED_CIRCLE;
    /// use allsorts::{tag, Font};
    ///
    /// let script = tag::LATN;
    /// let lang = tag::DFLT;
    /// let variation_tuple = None;
    /// let buffer = std::fs::read("tests/fonts/opentype/Klei.otf").expect("unable to read Klei.otf");
    /// let scope = ReadScope::new(&buffer);
    /// let font_file = scope.read::<FontData<'_>>().expect("unable to parse font");
    /// // Use a different index to access other fonts in a font collection (E.g. TTC)
    /// let provider = font_file
    ///     .table_provider(0)
    ///     .expect("unable to create table provider");
    /// let mut font = Font::new(provider).expect("unable to load font tables");
    ///
    /// // Klei ligates ff
    /// let glyphs = font.map_glyphs(
    ///     "Shaping in a jiffy.",
    ///     script,
    ///     MatchingPresentation::NotRequired,
    /// );
    /// let glyph_infos = font
    ///     .shape(
    ///         glyphs,
    ///         script,
    ///         Some(lang),
    ///         &Features::Mask(FeatureMask::default()),
    ///         variation_tuple,
    ///         true,
    ///     )
    ///     .expect("error shaping text");
    /// // We expect ff to be ligated so the number of glyphs (18) should be one less than the
    /// // number of input characters (19).
    /// assert_eq!(glyph_infos.len(), 18);
    /// ```
    pub fn shape(
        &mut self,
        mut glyphs: Vec<RawGlyph<()>>,
        script_tag: u32,
        opt_lang_tag: Option<u32>,
        features: &Features,
        tuple: Option<Tuple<'_>>,
        kerning: bool,
    ) -> Result<Vec<Info>, (ShapingError, Vec<Info>)> {
        // We forge ahead in the face of errors applying what we can, returning the first error
        // encountered.
        let mut err: Option<ShapingError> = None;
        let opt_gsub_cache = check_set_err(self.gsub_cache(), &mut err);
        let opt_gpos_cache = check_set_err(self.gpos_cache(), &mut err);
        let opt_gdef_table = check_set_err(self.gdef_table(), &mut err);
        let opt_morx_table = check_set_err(self.morx_table(), &mut err);
        let opt_gdef_table = opt_gdef_table.as_ref().map(Arc::as_ref);
        let opt_kern_table = check_set_err(self.kern_table(), &mut err);
        let opt_kern_table = opt_kern_table
            .as_ref()
            .map(|x| kern::KernTable::from(x.as_ref()));
        let (dotted_circle_index, _) =
            self.lookup_glyph_index(DOTTED_CIRCLE, MatchingPresentation::NotRequired, None);

        let num_glyphs = self.num_glyphs();
        let mut applied_morx = false;
        if let Some(gsub_cache) = opt_gsub_cache {
            // Apply gsub if table is present
            let res = gsub::apply(
                dotted_circle_index,
                &gsub_cache,
                opt_gdef_table,
                script_tag,
                opt_lang_tag,
                features,
                tuple,
                num_glyphs,
                &mut glyphs,
            );
            check_set_err(res, &mut err);
        } else if let Some(morx_cache) = opt_morx_table {
            // Otherwise apply morx if table is present
            morx_cache.with_table(|morx_table: &MorxTable<'_>| {
                let res = morx::apply(morx_table, &mut glyphs, features, script_tag);
                check_set_err(res, &mut err);

                applied_morx = true;
            })
        }

        // Apply gpos if table is present
        let mut infos = Info::init_from_glyphs(opt_gdef_table, glyphs);
        if let Some(gpos_cache) = opt_gpos_cache {
            // Remove residual DELETED_GLYPHs created during morx processing, _before_ applying
            // GPOS. These glyphs interfere with mark-to-base attachment, causing certain Apple
            // Color Emoji to be rendered incorrectly.
            if applied_morx {
                infos.retain(|i| i.get_glyph_index() != DELETED_GLYPH);
            }

            let res = gpos::apply(
                &gpos_cache,
                opt_gdef_table,
                opt_kern_table,
                kerning,
                features,
                tuple,
                script_tag,
                opt_lang_tag,
                &mut infos,
            );
            check_set_err(res, &mut err);
        } else {
            let mut has_cross_stream = false;
            if let Some(kern_table) = opt_kern_table {
                let res = kern::apply(&kern_table, script_tag, &mut infos);
                has_cross_stream = check_set_err(res, &mut err);
            }

            // Remove residual DELETED_GLYPHs created during morx processing, _after_ applying kern,
            // as these glyphs seem to be necessary for correct kerning (e.g. Apple's Geeza Pro and
            // Waseem fonts).
            if applied_morx {
                infos.retain(|i| i.get_glyph_index() != DELETED_GLYPH);
            }

            // Positioning glyphs on the cross-axis implies mark positioning of sorts, so disable
            // fallback mark positioning in those cases.
            if !has_cross_stream {
                gpos::apply_fallback_mark_positioning(&mut infos);
            }
        }

        match err {
            Some(err) => Err((err, infos)),
            None => Ok(infos),
        }
    }

    /// Map text to glyphs.
    ///
    /// This method maps text into glyphs, which can then be passed to `shape`. The text is
    /// processed in logical order, there is no need to reorder text before calling this method.
    ///
    /// The `match_presentation` argument controls glyph mapping in the presence of emoji/text
    /// variation selectors. If `MatchingPresentation::NotRequired` is passed then glyph mapping
    /// will succeed if the font contains a mapping for a given character, regardless of whether
    /// it has the tables necessary to support the requested presentation. If
    /// `MatchingPresentation::Required` is passed then a character with emoji presentation,
    /// either by default or requested via variation selector will only map to a glyph if the font
    /// has mapping for the character, and it has the necessary tables for color emoji.
    pub fn map_glyphs(
        &mut self,
        text: &str,
        script_tag: u32,
        match_presentation: MatchingPresentation,
    ) -> Vec<RawGlyph<()>> {
        let mut chars = text.chars().collect();
        preprocess_text(&mut chars, script_tag);

        // We look ahead in the char stream for variation selectors. If one is found it is used for
        // mapping the current glyph. When a variation selector is reached in the stream it is
        // skipped as it was handled as part of the preceding character.
        let mut glyphs = Vec::with_capacity(chars.len());
        let mut chars_iter = chars.into_iter().peekable();
        while let Some(ch) = chars_iter.next() {
            match VariationSelector::try_from(ch) {
                Ok(_) => {} // filter out variation selectors
                Err(()) => {
                    let vs = chars_iter
                        .peek()
                        .and_then(|&next| VariationSelector::try_from(next).ok());
                    let (glyph_index, used_variation) =
                        self.lookup_glyph_index(ch, match_presentation, vs);
                    let glyph = RawGlyph {
                        unicodes: tiny_vec![[char; 1] => ch],
                        glyph_index,
                        liga_component_pos: 0,
                        glyph_origin: GlyphOrigin::Char(ch),
                        flags: RawGlyphFlags::empty(),
                        extra_data: (),
                        variation: Some(used_variation),
                    };
                    glyphs.push(glyph);
                }
            }
        }
        glyphs.shrink_to_fit();
        glyphs
    }

    /// True if the font has one or more variation axes.
    pub fn is_variable(&self) -> bool {
        self.axis_count > 0
    }

    pub fn variation_axes(&self) -> Result<Vec<VariationAxisRecord>, ParseError> {
        let table = self.font_table_provider.read_table_data(tag::FVAR)?;
        let fvar = ReadScope::new(&table).read::<FvarTable<'_>>()?;
        Ok(fvar.axes().collect())
    }

    fn map_glyph(&self, char_code: u32) -> u16 {
        match ReadScope::new(self.cmap_subtable_data()).read::<CmapSubtable<'_>>() {
            // TODO: Cache the parsed CmapSubtable
            Ok(cmap_subtable) => match cmap_subtable.map_glyph(char_code) {
                Ok(Some(glyph_index)) => glyph_index,
                _ => 0,
            },
            Err(_err) => 0,
        }
    }

    fn map_unicode_to_glyph(
        &mut self,
        ch: char,
        match_presentation: MatchingPresentation,
        variation_selector: Option<VariationSelector>,
    ) -> (u16, VariationSelector) {
        // If emoji or text presentation are explicitly requested then require matching presentation
        let match_presentation = if matches!(
            variation_selector,
            Some(VariationSelector::VS15 | VariationSelector::VS16)
        ) {
            MatchingPresentation::Required
        } else {
            match_presentation
        };

        let used_selector = Self::resolve_default_presentation(ch, variation_selector);
        let glyph_index = match self.cmap_subtable_encoding {
            Encoding::Unicode => {
                self.lookup_glyph_index_with_variation(ch as u32, match_presentation, used_selector)
            }
            Encoding::Symbol => {
                let char_code = self.legacy_symbol_char_code(ch);
                self.lookup_glyph_index_with_variation(char_code, match_presentation, used_selector)
            }
            Encoding::AppleRoman => match char_to_macroman(ch) {
                Some(char_code) => self.lookup_glyph_index_with_variation(
                    u32::from(char_code),
                    match_presentation,
                    used_selector,
                ),
                None => {
                    let char_code = self.legacy_symbol_char_code(ch);
                    self.lookup_glyph_index_with_variation(
                        char_code,
                        match_presentation,
                        used_selector,
                    )
                }
            },
            Encoding::Big5 => match unicode_to_big5(ch) {
                Some(char_code) => self.lookup_glyph_index_with_variation(
                    u32::from(char_code),
                    match_presentation,
                    used_selector,
                ),
                None => 0,
            },
        };
        (glyph_index, used_selector)
    }

    // The symbol encoding was created to support fonts with arbitrary ornaments or symbols not
    // supported in Unicode or other standard encodings. A format 4 subtable would be used,
    // typically with up to 224 graphic characters assigned at code positions beginning with
    // 0xF020. This corresponds to a sub-range within the Unicode Private-Use Area (PUA), though
    // this is not a Unicode encoding. In legacy usage, some applications would represent the
    // symbol characters in text using a single-byte encoding, and then map 0x20 to the
    // OS/2.usFirstCharIndex value in the font.
    // — https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#encoding-records-and-encodings
    fn legacy_symbol_char_code(&mut self, ch: char) -> u32 {
        let char_code0 = if ch < '\u{F000}' || ch > '\u{F0FF}' {
            ch as u32
        } else {
            ch as u32 - 0xF000
        };
        let provider = &self.font_table_provider;
        let first_char = if let Ok(Some(us_first_char_index)) =
            self.os2_us_first_char_index.get_or_load(|| {
                load_os2_table(provider)?
                    .map(|os2| Ok(os2.us_first_char_index))
                    .transpose()
            }) {
            u32::from(us_first_char_index)
        } else {
            0x20
        };
        (char_code0 + first_char) - 0x20 // Perform subtraction last to avoid underflow.
    }

    fn lookup_glyph_index_with_variation(
        &mut self,
        char_code: u32,
        match_presentation: MatchingPresentation,
        variation_selector: VariationSelector,
    ) -> u16 {
        if match_presentation == MatchingPresentation::Required {
            // This match aims to only return a non-zero index if the font supports the requested
            // presentation. So, if you want the glyph index for a code point using emoji presentation,
            // the font must have suitable tables. On the flip side, if you want a glyph with text
            // presentation then the font must have glyf or CFF outlines.
            if (variation_selector == VariationSelector::VS16 && self.has_embedded_images())
                || (variation_selector == VariationSelector::VS15 && self.has_glyph_outlines())
            {
                self.map_glyph(char_code)
            } else {
                0
            }
        } else {
            self.map_glyph(char_code)
        }
    }

    fn resolve_default_presentation(
        ch: char,
        variation_selector: Option<VariationSelector>,
    ) -> VariationSelector {
        variation_selector.unwrap_or_else(|| {
            // `None` indicates no selector present so for emoji determine the default presentation.
            if unicode::bool_prop_emoji_presentation(ch) {
                VariationSelector::VS16
            } else {
                VariationSelector::VS15
            }
        })
    }

    pub fn glyph_names<'a>(&self, ids: &[u16]) -> Vec<Cow<'a, str>> {
        let post = read_and_box_optional_table(&self.font_table_provider, tag::POST)
            .ok()
            .and_then(convert::identity);
        let cmap = ReadScope::new(self.cmap_subtable_data())
            .read::<CmapSubtable<'_>>()
            .ok()
            .map(|table| (self.cmap_subtable_encoding, table));
        let glyph_namer = GlyphNames::new(&cmap, post);
        glyph_namer.unique_glyph_names(ids)
    }

    /// Returns the names of the variation axes in the font.
    pub fn axis_names<'a>(&self) -> Result<Vec<NamedAxis<'a>>, AxisNamesError> {
        variations::axis_names(&self.font_table_provider)
    }

    /// Find an image matching the supplied criteria.
    ///
    /// * `glyph_index` is the glyph to lookup.
    /// * `target_ppem` is the desired size. If an exact match can't be found the nearest one will
    ///    be returned, favouring being oversize vs. undersized.
    /// * `max_bit_depth` is the maximum accepted bit depth of the bitmap to return. If you accept
    ///   all bit depths then use `BitDepth::ThirtyTwo`.
    pub fn lookup_glyph_image(
        &mut self,
        glyph_index: GlyphId,
        target_ppem: u16,
        max_bit_depth: BitDepth,
    ) -> Result<Option<BitmapGlyph>, ParseError> {
        let embedded_bitmaps = match self.embedded_images()? {
            Some(embedded_bitmaps) => embedded_bitmaps,
            None => return Ok(None),
        };
        match embedded_bitmaps.as_ref() {
            Images::Embedded { cblc, cbdt } => cblc.with_table(|cblc: &CBLCTable<'_>| {
                let target_ppem = if target_ppem > u16::from(u8::MAX) {
                    u8::MAX
                } else {
                    target_ppem as u8
                };
                let bitmap = match cblc.find_strike(glyph_index, target_ppem, max_bit_depth) {
                    Some(matching_strike) => {
                        let cbdt = cbdt.borrow_table();
                        matching_strike.bitmap(cbdt)?.map(|bitmap| {
                            BitmapGlyph::try_from((
                                &matching_strike.bitmap_size.inner,
                                bitmap,
                                glyph_index,
                            ))
                        })
                    }
                    None => None,
                };
                bitmap.transpose()
            }),
            Images::Colr { .. } => Ok(None),
            Images::Sbix(sbix) => self.lookup_sbix_glyph_bitmap(
                sbix,
                false,
                false,
                glyph_index,
                target_ppem,
                max_bit_depth,
            ),
            Images::Svg(svg) => self.lookup_svg_glyph(svg, glyph_index),
        }
    }

    pub fn visit_colr_glyph<P>(
        &mut self,
        glyph_id: u16,
        palette_index: u16,
        painter: &mut P,
    ) -> Result<(), P::Error>
    where
        P: Painter,
        P::Error: From<ParseError> + From<cff::CFFError>,
    {
        let Some(embedded_images) = self.embedded_images()? else {
            return Err(ParseError::MissingValue.into());
        };

        if self.glyph_table_flags.contains(GlyphTableFlags::CFF2) {
            let cff2 = self
                .cff2_table()?
                .ok_or(ParseError::MissingTable(tag::CFF2))?;

            cff2.with(|cff2| {
                let mut cff2_outlines = CFF2Outlines { table: cff2.table };
                Self::visit_colr_glyph_inner(
                    glyph_id,
                    palette_index,
                    painter,
                    &mut cff2_outlines,
                    &embedded_images,
                )
            })
        } else if self.glyph_table_flags.contains(GlyphTableFlags::CFF) {
            let cff = self
                .cff_table()?
                .ok_or(ParseError::MissingTable(tag::CFF))?;

            cff.with(|cff| {
                let mut cff_outlines = CFFOutlines { table: cff.table };
                Self::visit_colr_glyph_inner(
                    glyph_id,
                    palette_index,
                    painter,
                    &mut cff_outlines,
                    &embedded_images,
                )
            })
        } else if self.glyph_table_flags.contains(GlyphTableFlags::GLYF) {
            self.maybe_load_loca_glyf()?;
            let mut context = GlyfVisitorContext::new(&mut self.loca_glyf, None);

            Self::visit_colr_glyph_inner(
                glyph_id,
                palette_index,
                painter,
                &mut context,
                &embedded_images,
            )
        } else {
            Err(ParseError::MissingValue.into())
        }
    }

    fn visit_colr_glyph_inner<P, G>(
        glyph_id: u16,
        palette_index: u16,
        painter: &mut P,
        glyphs: &mut G,
        embedded_images: &Images,
    ) -> Result<(), P::Error>
    where
        P: Painter,
        P::Error: From<ParseError> + From<G::Error>,
        G: OutlineBuilder,
    {
        match embedded_images {
            Images::Colr(tables) => tables.with(|fields| {
                let palette = fields
                    .cpal
                    .palette(palette_index)
                    .ok_or(ParseError::BadIndex)?;
                let Some(glyph) = fields.colr.lookup(glyph_id)? else {
                    return Ok(());
                };
                glyph.visit(painter, glyphs, palette)
            }),
            _ => Err(ParseError::MissingTable(tag::COLR).into()),
        }
    }

    /// Check if there is COLR data for a glyph
    pub fn has_colr(&mut self, glyph_id: u16) -> Result<bool, ParseError> {
        let Some(embedded_images) = self.embedded_images()? else {
            return Ok(false);
        };

        match embedded_images.as_ref() {
            Images::Colr(tables) => tables.with_colr(|colr| Ok(colr.lookup(glyph_id)?.is_some())),
            _ => Ok(false),
        }
    }

    /// Retrieve the clip box for a COLR glyph.
    ///
    /// If the `COLR` table does not supply a clip list or there is no clip box for this
    /// glyph, then `Ok(None)` is returned.
    pub fn colr_clip_box(&mut self, glyph_id: u16) -> Result<Option<RectF>, ParseError> {
        let Some(embedded_images) = self.embedded_images()? else {
            return Err(ParseError::MissingValue);
        };

        match embedded_images.as_ref() {
            Images::Colr(tables) => tables.with_colr(|colr| {
                let glyph = colr.lookup(glyph_id)?.ok_or(ParseError::BadIndex)?;
                glyph.clip_box()
            }),
            _ => Err(ParseError::MissingValue),
        }
    }

    /// Perform sbix lookup with `dupe` and `flip` handling.
    ///
    /// The `dupe` flag indicates if this is a dupe lookup or not. The (Apple-specific) `flip`
    /// flag indicates that a glyph's bitmap data should be flipped horizontally. In both cases,
    /// to avoid potential infinite recursion we only follow one level of indirection.
    fn lookup_sbix_glyph_bitmap(
        &self,
        sbix: &tables::Sbix,
        dupe: bool,
        flip: bool,
        glyph_index: GlyphId,
        target_ppem: u16,
        max_bit_depth: BitDepth,
    ) -> Result<Option<BitmapGlyph>, ParseError> {
        sbix.with_table(|sbix_table: &SbixTable<'_>| {
            match sbix_table.find_strike(glyph_index, target_ppem, max_bit_depth) {
                Some(strike) => {
                    match strike.read_glyph(glyph_index)? {
                        Some(ref glyph) if glyph.graphic_type == tag::DUPE => {
                            // The special graphicType of 'dupe' indicates that the data field
                            // contains a uint16, big-endian glyph ID. The bitmap data for the
                            // indicated glyph should be used for the current glyph.
                            // — https://docs.microsoft.com/en-us/typography/opentype/spec/sbix#glyph-data
                            if dupe {
                                // We're already inside a `dupe` lookup and have encountered another
                                Ok(None)
                            } else {
                                // Try again with the glyph id stored in data
                                let dupe_glyph_index =
                                    ReadScope::new(glyph.data).ctxt().read_u16be()?;
                                self.lookup_sbix_glyph_bitmap(
                                    sbix,
                                    true,
                                    flip,
                                    dupe_glyph_index,
                                    target_ppem,
                                    max_bit_depth,
                                )
                            }
                        }
                        Some(ref glyph) if glyph.graphic_type == tag::FLIP => {
                            // The special graphicType of 'flip' indicates that the bitmap data
                            // for a glyph should be flipped horizontally. This feature is
                            // undocumented by Apple, but is already in use in Apple Color Emoji.
                            if flip {
                                // We're already inside a `flip` lookup and have encountered another
                                Ok(None)
                            } else {
                                // Try again with the glyph id stored in data
                                let flip_glyph_index =
                                    ReadScope::new(glyph.data).ctxt().read_u16be()?;
                                self.lookup_sbix_glyph_bitmap(
                                    sbix,
                                    dupe,
                                    true,
                                    flip_glyph_index,
                                    target_ppem,
                                    max_bit_depth,
                                )
                            }
                        }
                        Some(glyph) => {
                            Ok(Some(BitmapGlyph::from((strike, &glyph, glyph_index, flip))))
                        }
                        None => Ok(None),
                    }
                }
                None => Ok(None),
            }
        })
    }

    fn lookup_svg_glyph(
        &self,
        svg: &tables::Svg,
        glyph_index: GlyphId,
    ) -> Result<Option<BitmapGlyph>, ParseError> {
        svg.with_table(
            |svg_table: &SvgTable<'_>| match svg_table.lookup_glyph(glyph_index)? {
                Some(svg_record) => BitmapGlyph::try_from((&svg_record, glyph_index)).map(Some),
                None => Ok(None),
            },
        )
    }

    fn embedded_images(&mut self) -> Result<Option<Arc<Images>>, ParseError> {
        let provider = &self.font_table_provider;
        let num_glyphs = usize::from(self.maxp_table.num_glyphs);
        let tables_to_check = self.glyph_table_flags & self.embedded_image_filter;
        self.embedded_images.get_or_load(|| {
            if tables_to_check.contains(GlyphTableFlags::COLR) {
                let images = load_colr_cpal(provider).map(Images::Colr)?;
                Ok(Some(Arc::new(images)))
            } else if tables_to_check.contains(GlyphTableFlags::SVG) {
                let images = load_svg(provider).map(Images::Svg)?;
                Ok(Some(Arc::new(images)))
            } else if tables_to_check.contains(GlyphTableFlags::CBDT) {
                let images = load_cblc_cbdt(provider, tag::CBLC, tag::CBDT)
                    .map(|(cblc, cbdt)| Images::Embedded { cblc, cbdt })?;
                Ok(Some(Arc::new(images)))
            } else if tables_to_check.contains(GlyphTableFlags::SBIX) {
                let images = load_sbix(provider, num_glyphs).map(Images::Sbix)?;
                Ok(Some(Arc::new(images)))
            } else if tables_to_check.contains(GlyphTableFlags::EBDT) {
                let images =
                    load_cblc_cbdt(provider, tag::EBLC, tag::EBDT).map(|(eblc, ebdt)| {
                        Images::Embedded {
                            cblc: eblc,
                            cbdt: ebdt,
                        }
                    })?;
                Ok(Some(Arc::new(images)))
            } else {
                Ok(None)
            }
        })
    }

    /// Returns `true` if the font contains embedded images in supported tables.
    ///
    /// Allsorts supports extracting images from `CBDT`/`CBLC`, `sbix`, and `SVG` tables.
    /// If any of these tables are present and parsable then this method returns `true`.
    pub fn has_embedded_images(&mut self) -> bool {
        matches!(self.embedded_images(), Ok(Some(_)))
    }

    /// Returns `true` if the font contains vector glyph outlines in supported tables.
    ///
    /// Supported tables are `glyf`, `CFF`, `CFF2`.
    pub fn has_glyph_outlines(&self) -> bool {
        self.glyph_table_flags
            .intersects(GlyphTableFlags::GLYF | GlyphTableFlags::CFF | GlyphTableFlags::CFF2)
    }

    /// Returns the horizontal advance of the supplied glyph index.
    ///
    /// Will return `None` if there are errors encountered reading the `hmtx` table or there is
    /// no entry for the glyph index.
    pub fn horizontal_advance(&mut self, glyph: GlyphId) -> Option<u16> {
        glyph_info::advance(&self.maxp_table, &self.hhea_table, &self.hmtx_table, glyph).ok()
    }

    pub fn vertical_advance(&mut self, glyph: GlyphId) -> Option<u16> {
        let provider = &self.font_table_provider;
        let vmtx = self
            .vmtx_table
            .get_or_load(|| {
                read_and_box_optional_table(provider, tag::VMTX).map(|ok| ok.map(Arc::from))
            })
            .ok()?;
        let vhea = self.vhea_table().ok()?;

        if let (Some(vhea), Some(vmtx_table)) = (vhea, vmtx) {
            Some(glyph_info::advance(&self.maxp_table, &vhea, &vmtx_table, glyph).unwrap())
        } else {
            None
        }
    }

    /// Calculates the bounding box of a glyph
    ///
    /// Returns `None` if the glyph is empty.
    ///
    /// The error type is `CFFError` because CFF errors can be encountered when traversing
    /// CharStrings. Errors encountered when processing `glyf` based fonts use the
    /// `CFFError::ParseError` variant, with the error contained within.
    ///
    /// **Note:** This method currently only returns the bounding box for the default instance
    /// for variable fonts.
    pub fn bounding_box(&mut self, glyph_id: u16) -> Result<Option<BoundingBox>, CFFError> {
        if self.glyph_table_flags.contains(GlyphTableFlags::CFF2) {
            let cff2 = self
                .cff2_table()?
                .ok_or(ParseError::MissingTable(tag::CFF2))?;

            cff2.with(|cff2| {
                let mut cff2_outlines = CFF2Outlines { table: cff2.table };

                // TODO: Support bounding box of variable font instances
                cff2_outlines.visit(glyph_id, None, &mut NullSink)
            })
        } else if self.glyph_table_flags.contains(GlyphTableFlags::CFF) {
            let cff = self
                .cff_table()?
                .ok_or(ParseError::MissingTable(tag::CFF))?;

            cff.with(|cff| {
                let mut cff_outlines = CFFOutlines { table: cff.table };
                cff_outlines.visit(glyph_id, None, &mut NullSink)
            })
        } else if self.glyph_table_flags.contains(GlyphTableFlags::GLYF) {
            self.maybe_load_loca_glyf()?;

            // TODO: Support bounding box of variable font instances
            let mut context = GlyfVisitorContext::new(&mut self.loca_glyf, None);
            let mut sink = BoundingBoxSink::new();
            context.visit(glyph_id, None, &mut sink)?;

            Ok(sink.to_bounding_box())
        } else {
            Err(ParseError::MissingValue.into())
        }
    }

    pub fn cff_table(&mut self) -> Result<Option<Arc<tables::CFF>>, ParseError> {
        let provider = &self.font_table_provider;
        self.cff_cache.get_or_load(|| {
            let cff_data = provider.read_table_data(tag::CFF).map(Box::from)?;
            let cff = tables::CFFTryBuilder {
                data: cff_data,
                table_builder: |data: &Box<[u8]>| ReadScope::new(data).read::<CFF<'_>>(),
            }
            .try_build()?;
            Ok(Some(Arc::new(cff)))
        })
    }

    pub fn cff2_table(&mut self) -> Result<Option<Arc<tables::CFF2>>, ParseError> {
        let provider = &self.font_table_provider;
        self.cff2_cache.get_or_load(|| {
            let cff_data = provider.read_table_data(tag::CFF2).map(Box::from)?;
            let cff = tables::CFF2TryBuilder {
                data: cff_data,
                table_builder: |data: &Box<[u8]>| ReadScope::new(data).read::<CFF2<'_>>(),
            }
            .try_build()?;
            Ok(Some(Arc::new(cff)))
        })
    }

    pub fn os2_table(&self) -> Result<Option<Os2>, ParseError> {
        load_os2_table(&self.font_table_provider)
    }

    pub fn maybe_load_loca_glyf(&mut self) -> Result<(), ParseError> {
        if !self.loca_glyf.is_loaded() {
            let provider = &self.font_table_provider;
            let loca_data = self.font_table_provider.read_table_data(tag::LOCA)?;
            let loca = ReadScope::new(&loca_data).read_dep::<LocaTable<'_>>((
                self.num_glyphs(),
                self.head_table.index_to_loc_format,
            ))?;
            let loca = owned::LocaTable::from(&loca);
            let glyf = read_and_box_table(provider, tag::GLYF)?;
            self.loca_glyf = LocaGlyf::loaded(loca, glyf);
        }

        Ok(())
    }

    pub fn gdef_table(&mut self) -> Result<Option<Arc<GDEFTable>>, ParseError> {
        let provider = &self.font_table_provider;
        self.gdef_cache.get_or_load(|| {
            if let Some(gdef_data) = provider.table_data(tag::GDEF)? {
                let gdef = ReadScope::new(&gdef_data).read::<GDEFTable>()?;
                Ok(Some(Arc::new(gdef)))
            } else {
                Ok(None)
            }
        })
    }

    pub fn morx_table(&mut self) -> Result<Option<Arc<tables::Morx>>, ParseError> {
        let provider = &self.font_table_provider;
        let num_glyphs = self.num_glyphs();
        self.morx_cache.get_or_load(|| {
            if let Some(morx_data) = provider.table_data(tag::MORX)? {
                let morx = tables::Morx::try_new(morx_data.into(), |data| {
                    ReadScope::new(data).read_dep::<MorxTable<'_>>(num_glyphs)
                })?;
                Ok(Some(Arc::new(morx)))
            } else {
                Ok(None)
            }
        })
    }

    pub fn gsub_cache(&mut self) -> Result<Option<LayoutCache<GSUB>>, ParseError> {
        let provider = &self.font_table_provider;
        self.gsub_cache.get_or_load(|| {
            if let Some(gsub_data) = provider.table_data(tag::GSUB)? {
                let gsub = ReadScope::new(&gsub_data).read::<LayoutTable<GSUB>>()?;
                let cache = new_layout_cache::<GSUB>(gsub);
                Ok(Some(cache))
            } else {
                Ok(None)
            }
        })
    }

    pub fn gpos_cache(&mut self) -> Result<Option<LayoutCache<GPOS>>, ParseError> {
        let provider = &self.font_table_provider;
        self.gpos_cache.get_or_load(|| {
            if let Some(gpos_data) = provider.table_data(tag::GPOS)? {
                let gpos = ReadScope::new(&gpos_data).read::<LayoutTable<GPOS>>()?;
                let cache = new_layout_cache::<GPOS>(gpos);
                Ok(Some(cache))
            } else {
                Ok(None)
            }
        })
    }

    pub fn kern_table(&mut self) -> Result<Option<Arc<KernTable>>, ParseError> {
        let provider = &self.font_table_provider;
        self.kern_cache.get_or_load(|| {
            if let Some(kern_data) = provider.table_data(tag::KERN)? {
                match ReadScope::new(&kern_data).read::<kern::KernTable<'_>>() {
                    Ok(kern) => Ok(Some(Arc::new(kern.to_owned()))),
                    // This error may be encountered because there is a kern 1.0 version defined by
                    // Apple that is not in the OpenType spec. It only works on macOS. We don't
                    // support it so return None instead of returning an error.
                    Err(ParseError::BadVersion) => Ok(None),
                    Err(err) => Err(err),
                }
            } else {
                Ok(None)
            }
        })
    }

    pub fn vhea_table(&mut self) -> Result<Option<Arc<HheaTable>>, ParseError> {
        let provider = &self.font_table_provider;
        self.vhea_table.get_or_load(|| {
            if let Some(vhea_data) = provider.table_data(tag::VHEA)? {
                let vhea = ReadScope::new(&vhea_data).read::<HheaTable>()?;
                Ok(Some(Arc::new(vhea)))
            } else {
                Ok(None)
            }
        })
    }

    pub fn cmap_subtable_data(&self) -> &[u8] {
        &self.cmap_table[self.cmap_subtable_offset..]
    }
}

impl<T> LazyLoad<T> {
    /// Return loaded value, calls the supplied closure if not already loaded.
    ///
    /// It's expected that `T` is cheap to clone, either because it's wrapped in an `Rc`
    /// or is `Copy`.
    fn get_or_load(
        &mut self,
        do_load: impl FnOnce() -> Result<Option<T>, ParseError>,
    ) -> Result<Option<T>, ParseError>
    where
        T: Clone,
    {
        match self {
            LazyLoad::Loaded(Some(ref data)) => Ok(Some(data.clone())),
            LazyLoad::Loaded(None) => Ok(None),
            LazyLoad::NotLoaded => {
                let data = do_load()?;
                *self = LazyLoad::Loaded(data.clone());
                Ok(data)
            }
        }
    }
}

impl GlyphCache {
    fn new() -> Self {
        GlyphCache(None)
    }

    fn get(&self, ch: char) -> Option<(u16, VariationSelector)> {
        if ch == DOTTED_CIRCLE {
            self.0
        } else {
            None
        }
    }

    fn put(&mut self, ch: char, glyph_index: GlyphId, variation_selector: VariationSelector) {
        if ch == DOTTED_CIRCLE {
            match self.0 {
                Some(_) => panic!("duplicate entry"),
                None => self.0 = Some((glyph_index, variation_selector)),
            }
        }
    }
}

struct NullSink;

impl OutlineSink for NullSink {
    fn move_to(&mut self, _to: Vector2F) {}

    fn line_to(&mut self, _to: Vector2F) {}

    fn quadratic_curve_to(&mut self, _ctrl: Vector2F, _to: Vector2F) {}

    fn cubic_curve_to(&mut self, _ctrl: LineSegment2F, _to: Vector2F) {}

    fn close(&mut self) {}
}

fn load_os2_table(provider: &impl FontTableProvider) -> Result<Option<Os2>, ParseError> {
    provider
        .table_data(tag::OS_2)?
        .map(|data| ReadScope::new(&data).read_dep::<Os2>(data.len()))
        .transpose()
}

fn load_cblc_cbdt(
    provider: &impl FontTableProvider,
    bitmap_location_table_tag: u32,
    bitmap_data_table_tag: u32,
) -> Result<(tables::CBLC, tables::CBDT), ParseError> {
    let cblc_data = read_and_box_table(provider, bitmap_location_table_tag)?;
    let cbdt_data = read_and_box_table(provider, bitmap_data_table_tag)?;

    let cblc = tables::CBLC::try_new(cblc_data, |data| {
        ReadScope::new(data).read::<CBLCTable<'_>>()
    })?;
    let cbdt = tables::CBDT::try_new(cbdt_data, |data| {
        ReadScope::new(data).read::<CBDTTable<'_>>()
    })?;

    Ok((cblc, cbdt))
}

fn load_colr_cpal(provider: &impl FontTableProvider) -> Result<tables::ColrCpal, ParseError> {
    let colr_data = read_and_box_table(provider, tag::COLR)?;
    let cpal_data = read_and_box_table(provider, tag::CPAL)?;

    let colr_cpal = ColrCpalTryBuilder {
        colr_data,
        cpal_data,
        colr_builder: |data: &Box<[u8]>| ReadScope::new(data).read::<ColrTable<'_>>(),
        cpal_builder: |data: &Box<[u8]>| ReadScope::new(data).read::<CpalTable<'_>>(),
    }
    .try_build()?;

    Ok(colr_cpal)
}

fn load_sbix(
    provider: &impl FontTableProvider,
    num_glyphs: usize,
) -> Result<tables::Sbix, ParseError> {
    let sbix_data = read_and_box_table(provider, tag::SBIX)?;
    tables::Sbix::try_new(sbix_data, |data| {
        ReadScope::new(data).read_dep::<SbixTable<'_>>(num_glyphs)
    })
}

fn load_svg(provider: &impl FontTableProvider) -> Result<tables::Svg, ParseError> {
    let svg_data = read_and_box_table(provider, tag::SVG)?;
    tables::Svg::try_new(svg_data, |data| ReadScope::new(data).read::<SvgTable<'_>>())
}

fn charmap_info(cmap_buf: &[u8]) -> Result<Option<(Encoding, u32)>, ParseError> {
    let cmap = ReadScope::new(cmap_buf).read::<Cmap<'_>>()?;
    Ok(find_good_cmap_subtable(&cmap)
        .map(|(encoding, encoding_record)| (encoding, encoding_record.offset)))
}

pub fn read_cmap_subtable<'a>(
    cmap: &Cmap<'a>,
) -> Result<Option<(Encoding, CmapSubtable<'a>)>, ParseError> {
    if let Some((encoding, encoding_record)) = find_good_cmap_subtable(cmap) {
        let subtable = cmap
            .scope
            .offset(usize::try_from(encoding_record.offset)?)
            .read::<CmapSubtable<'_>>()?;
        Ok(Some((encoding, subtable)))
    } else {
        Ok(None)
    }
}

pub fn find_good_cmap_subtable(cmap: &Cmap<'_>) -> Option<(Encoding, EncodingRecord)> {
    // MS UNICODE, UCS-4 (32 bit)
    if let Some(encoding_record) =
        cmap.find_subtable(PlatformId::WINDOWS, EncodingId::WINDOWS_UNICODE_UCS4)
    {
        return Some((Encoding::Unicode, encoding_record));
    }

    // MS UNICODE, UCS-2 (16 bit)
    if let Some(encoding_record) =
        cmap.find_subtable(PlatformId::WINDOWS, EncodingId::WINDOWS_UNICODE_BMP_UCS2)
    {
        return Some((Encoding::Unicode, encoding_record));
    }

    // Apple UNICODE, UCS-4 (32 bit)
    if let Some(encoding_record) =
        cmap.find_subtable(PlatformId::UNICODE, EncodingId::MACINTOSH_UNICODE_UCS4)
    {
        return Some((Encoding::Unicode, encoding_record));
    }

    // Any UNICODE table
    if let Some(encoding_record) = cmap.find_subtable_for_platform(PlatformId::UNICODE) {
        return Some((Encoding::Unicode, encoding_record));
    }

    // MS Symbol
    if let Some(encoding_record) =
        cmap.find_subtable(PlatformId::WINDOWS, EncodingId::WINDOWS_SYMBOL)
    {
        return Some((Encoding::Symbol, encoding_record));
    }

    // Apple Roman
    if let Some(encoding_record) =
        cmap.find_subtable(PlatformId::MACINTOSH, EncodingId::MACINTOSH_APPLE_ROMAN)
    {
        return Some((Encoding::AppleRoman, encoding_record));
    }

    // Big5
    if let Some(encoding_record) = cmap.find_subtable(PlatformId::WINDOWS, EncodingId::WINDOWS_BIG5)
    {
        return Some((Encoding::Big5, encoding_record));
    }

    None
}

// Unwrap the supplied result returning `T` if `Ok` or `T::default` otherwise. If `Err` then set
// `err` unless it's already set.
fn check_set_err<T, E>(res: Result<T, E>, err: &mut Option<ShapingError>) -> T
where
    E: Into<ShapingError>,
    T: Default,
{
    match res {
        Ok(table) => table,
        Err(e) => {
            if err.is_none() {
                *err = Some(e.into())
            }
            T::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitmap::{Bitmap, EncapsulatedBitmap};
    use crate::cff::CFFError;
    use crate::font_data::{DynamicFontTableProvider, FontData};
    use crate::tables::OpenTypeFont;
    use crate::tests::read_fixture;
    use std::error::Error;

    #[test]
    fn test_glyph_names() {
        let font_buffer = read_fixture("tests/fonts/opentype/TwitterColorEmoji-SVGinOT.ttf");
        let opentype_file = ReadScope::new(&font_buffer)
            .read::<OpenTypeFont<'_>>()
            .unwrap();
        let font_table_provider = opentype_file
            .table_provider(0)
            .expect("error reading font file");
        let font = Font::new(Box::new(font_table_provider)).expect("error reading font data");

        let names = font.glyph_names(&[0, 5, 45, 71, 1311, 3086]);
        assert_eq!(
            names,
            &[
                Cow::from(".notdef"),
                Cow::from("copyright"),
                Cow::from("uni25B6"),
                Cow::from("smileface"),
                Cow::from("u1FA95"),
                Cow::from("1f468-200d-1f33e")
            ]
        );
    }

    #[test]
    fn test_glyph_names_post_v3() {
        // This font is a CFF font with a version 3 post table (no names in table).
        let font_buffer = read_fixture("tests/fonts/opentype/Klei.otf");
        let opentype_file = ReadScope::new(&font_buffer)
            .read::<OpenTypeFont<'_>>()
            .unwrap();
        let font_table_provider = opentype_file
            .table_provider(0)
            .expect("error reading font file");
        let font = Font::new(Box::new(font_table_provider)).expect("error reading font data");

        let names = font.glyph_names(&[0, 5, 45, 100, 763, 1000 /* out of range */]);
        assert_eq!(
            names,
            &[
                Cow::from(".notdef"),
                Cow::from("dollar"),
                Cow::from("L"),
                Cow::from("yen"),
                Cow::from("uniFB00"),
                Cow::from("g1000") // out of range gid is assigned fallback name
            ]
        );
    }

    #[test]
    fn test_lookup_sbix() {
        let font_buffer = read_fixture("tests/fonts/sbix/sbix-dupe.ttf");
        let opentype_file = ReadScope::new(&font_buffer)
            .read::<OpenTypeFont<'_>>()
            .unwrap();
        let font_table_provider = opentype_file
            .table_provider(0)
            .expect("error reading font file");
        let mut font = Font::new(Box::new(font_table_provider)).expect("error reading font data");

        // Successfully read bitmap
        match font.lookup_glyph_image(1, 100, BitDepth::ThirtyTwo) {
            Ok(Some(BitmapGlyph {
                bitmap: Bitmap::Encapsulated(EncapsulatedBitmap { data, .. }),
                ..
            })) => {
                assert_eq!(data.len(), 224);
            }
            _ => panic!("Expected encapsulated bitmap, got something else."),
        }

        // Successfully read bitmap pointed at by `dupe` record. Should end up returning data for
        // glyph 1.
        match font.lookup_glyph_image(2, 100, BitDepth::ThirtyTwo) {
            Ok(Some(BitmapGlyph {
                bitmap: Bitmap::Encapsulated(EncapsulatedBitmap { data, .. }),
                ..
            })) => {
                assert_eq!(data.len(), 224);
            }
            _ => panic!("Expected encapsulated bitmap, got something else."),
        }

        // Handle recursive `dupe` record. Should return Ok(None) as recursion is stopped at one
        // level.
        match font.lookup_glyph_image(3, 100, BitDepth::ThirtyTwo) {
            Ok(None) => {}
            _ => panic!("Expected Ok(None) got something else"),
        }
    }

    // Test that Font is only tied to the lifetime of the ReadScope and not any
    // intermediate types.
    #[test]
    fn table_provider_independent_of_font() {
        // Prior to code changes this function did not compile
        fn load_font<'a>(
            scope: ReadScope<'a>,
        ) -> Result<Font<DynamicFontTableProvider<'a>>, Box<dyn Error>> {
            let font_file = scope.read::<FontData<'_>>()?;
            let provider = font_file.table_provider(0)?;
            Font::new(provider).map_err(Box::from)
        }

        let buffer =
            std::fs::read("tests/fonts/opentype/Klei.otf").expect("unable to read Klei.otf");
        let scope = ReadScope::new(&buffer);
        assert!(load_font(scope).is_ok());
    }

    #[test]
    fn bounding_box_cff() -> Result<(), CFFError> {
        let buffer = std::fs::read("tests/fonts/opentype/Klei.otf").unwrap();
        let opentype_file = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>()?;
        let font_table_provider = opentype_file
            .table_provider(0)
            .expect("error reading font file");
        let mut font = Font::new(Box::new(font_table_provider))?;

        // Values verified in FontForge
        let bbox = font.bounding_box(1)?; // 1 is space
        assert_eq!(bbox, None);

        let bbox = font.bounding_box(80)?; // 80 is 'o'
        assert_eq!(
            bbox,
            Some(BoundingBox {
                x_min: 40,
                x_max: 488,
                y_min: -11,
                y_max: 511
            })
        );

        let bbox = font.bounding_box(109)?; // 109 is U+00AF MACRON
        assert_eq!(
            bbox,
            Some(BoundingBox {
                x_min: 67,
                x_max: 341,
                y_min: 506,
                y_max: 550
            })
        );

        Ok(())
    }
}
