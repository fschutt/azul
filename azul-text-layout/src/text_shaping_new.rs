use azul_core::app_resources::{
    FontMetrics, VariationSelector, Anchor,
    GlyphOrigin, RawGlyph, Placement, MarkPlacement,
    Info, HorizontalAdvance,
};
use tinyvec::tiny_vec;
use allsorts::binary::read::ReadScope;
use allsorts::fontfile::FontFile;
use allsorts::font_data_impl::FontDataImpl;
use allsorts::fontfile::FileTableProvider;
use std::rc::Rc;
use allsorts::layout::{LayoutCache, GDEFTable, GPOS, GSUB};

pub fn get_font_metrics(font_bytes: &[u8], font_index: usize) -> FontMetrics {

    use std::num::NonZeroU16;

    #[derive(Default)]
    struct Os2Info {
        x_avg_char_width: i16,
        us_weight_class: u16,
        us_width_class: u16,
        fs_type: u16,
        y_subscript_x_size: i16,
        y_subscript_y_size: i16,
        y_subscript_x_offset: i16,
        y_subscript_y_offset: i16,
        y_superscript_x_size: i16,
        y_superscript_y_size: i16,
        y_superscript_x_offset: i16,
        y_superscript_y_offset: i16,
        y_strikeout_size: i16,
        y_strikeout_position: i16,
        s_family_class: i16,
        panose: [u8; 10],
        ul_unicode_range1: u32,
        ul_unicode_range2: u32,
        ul_unicode_range3: u32,
        ul_unicode_range4: u32,
        ach_vend_id: u32,
        fs_selection: u16,
        us_first_char_index: u16,
        us_last_char_index: u16,
        s_typo_ascender: Option<i16>,
        s_typo_descender: Option<i16>,
        s_typo_line_gap: Option<i16>,
        us_win_ascent: Option<u16>,
        us_win_descent: Option<u16>,
        ul_code_page_range1: Option<u32>,
        ul_code_page_range2: Option<u32>,
        sx_height: Option<i16>,
        s_cap_height: Option<i16>,
        us_default_char: Option<u16>,
        us_break_char: Option<u16>,
        us_max_context: Option<u16>,
        us_lower_optical_point_size: Option<u16>,
        us_upper_optical_point_size: Option<u16>,
    }

    let scope = ReadScope::new(font_bytes);
    let font_file = match scope.read::<FontFile<'_>>() {
        Ok(o) => o,
        Err(_) => return FontMetrics::default(),
    };
    let provider = match font_file.table_provider(font_index) {
        Ok(o) => o,
        Err(_) => return FontMetrics::default(),
    };
    let font_data_impl = match FontDataImpl::new(Box::new(provider)) {
        Ok(Some(font_data_impl)) => font_data_impl,
        _ => return FontMetrics::default(),
    };

    // read the HHEA table to get the metrics for horizontal layout
    let hhea_table = &font_data_impl.hhea_table;
    let head_table = match font_data_impl.head_table() {
        Ok(Some(s)) => s,
        _ => return FontMetrics::default(),
    };

    let os2_table = match font_data_impl.os2_table() {
        Ok(Some(s)) => {
            Os2Info {
                x_avg_char_width: s.x_avg_char_width,
                us_weight_class: s.us_weight_class,
                us_width_class: s.us_width_class,
                fs_type: s.fs_type,
                y_subscript_x_size: s.y_subscript_x_size,
                y_subscript_y_size: s.y_subscript_y_size,
                y_subscript_x_offset: s.y_subscript_x_offset,
                y_subscript_y_offset: s.y_subscript_y_offset,
                y_superscript_x_size: s.y_superscript_x_size,
                y_superscript_y_size: s.y_superscript_y_size,
                y_superscript_x_offset: s.y_superscript_x_offset,
                y_superscript_y_offset: s.y_superscript_y_offset,
                y_strikeout_size: s.y_strikeout_size,
                y_strikeout_position: s.y_strikeout_position,
                s_family_class: s.s_family_class,
                panose: s.panose,
                ul_unicode_range1: s.ul_unicode_range1,
                ul_unicode_range2: s.ul_unicode_range2,
                ul_unicode_range3: s.ul_unicode_range3,
                ul_unicode_range4: s.ul_unicode_range4,
                ach_vend_id: s.ach_vend_id,
                fs_selection: s.fs_selection,
                us_first_char_index: s.us_first_char_index,
                us_last_char_index: s.us_last_char_index,

                s_typo_ascender: s.version0.map(|q| q.s_typo_ascender),
                s_typo_descender: s.version0.map(|q| q.s_typo_descender),
                s_typo_line_gap: s.version0.map(|q| q.s_typo_line_gap),
                us_win_ascent: s.version0.map(|q| q.us_win_ascent),
                us_win_descent: s.version0.map(|q| q.us_win_descent),

                ul_code_page_range1: s.version1.map(|q| q.ul_code_page_range1),
                ul_code_page_range2: s.version1.map(|q| q.ul_code_page_range2),

                sx_height: s.version2to4.map(|q| q.sx_height),
                s_cap_height: s.version2to4.map(|q| q.s_cap_height),
                us_default_char: s.version2to4.map(|q| q.us_default_char),
                us_break_char: s.version2to4.map(|q| q.us_break_char),
                us_max_context: s.version2to4.map(|q| q.us_max_context),

                us_lower_optical_point_size: s.version5.map(|q| q.us_lower_optical_point_size),
                us_upper_optical_point_size: s.version5.map(|q| q.us_upper_optical_point_size),
            }
        },
        _ => Os2Info::default(),
    };

    FontMetrics {

        // head table
        units_per_em: NonZeroU16::new(head_table.units_per_em).unwrap_or(unsafe { NonZeroU16::new_unchecked(1000) }),
        font_flags: head_table.flags,
        x_min: head_table.x_min,
        y_min: head_table.y_min,
        x_max: head_table.x_max,
        y_max: head_table.y_max,

        // hhea table
        ascender: hhea_table.ascender,
        descender: hhea_table.descender,
        line_gap: hhea_table.line_gap,
        advance_width_max: hhea_table.advance_width_max,
        min_left_side_bearing: hhea_table.min_left_side_bearing,
        min_right_side_bearing: hhea_table.min_right_side_bearing,
        x_max_extent: hhea_table.x_max_extent,
        caret_slope_rise: hhea_table.caret_slope_rise,
        caret_slope_run: hhea_table.caret_slope_run,
        caret_offset: hhea_table.caret_offset,
        num_h_metrics: hhea_table.num_h_metrics,

        // os/2 table

        x_avg_char_width: os2_table.x_avg_char_width,
        us_weight_class: os2_table.us_weight_class,
        us_width_class: os2_table.us_width_class,
        fs_type: os2_table.fs_type,
        y_subscript_x_size: os2_table.y_subscript_x_size,
        y_subscript_y_size: os2_table.y_subscript_y_size,
        y_subscript_x_offset: os2_table.y_subscript_x_offset,
        y_subscript_y_offset: os2_table.y_subscript_y_offset,
        y_superscript_x_size: os2_table.y_superscript_x_size,
        y_superscript_y_size: os2_table.y_superscript_y_size,
        y_superscript_x_offset: os2_table.y_superscript_x_offset,
        y_superscript_y_offset: os2_table.y_superscript_y_offset,
        y_strikeout_size: os2_table.y_strikeout_size,
        y_strikeout_position: os2_table.y_strikeout_position,
        s_family_class: os2_table.s_family_class,
        panose: os2_table.panose,
        ul_unicode_range1: os2_table.ul_unicode_range1,
        ul_unicode_range2: os2_table.ul_unicode_range2,
        ul_unicode_range3: os2_table.ul_unicode_range3,
        ul_unicode_range4: os2_table.ul_unicode_range4,
        ach_vend_id: os2_table.ach_vend_id,
        fs_selection: os2_table.fs_selection,
        us_first_char_index: os2_table.us_first_char_index,
        us_last_char_index: os2_table.us_last_char_index,
        s_typo_ascender: os2_table.s_typo_ascender,
        s_typo_descender: os2_table.s_typo_descender,
        s_typo_line_gap: os2_table.s_typo_line_gap,
        us_win_ascent: os2_table.us_win_ascent,
        us_win_descent: os2_table.us_win_descent,
        ul_code_page_range1: os2_table.ul_code_page_range1,
        ul_code_page_range2: os2_table.ul_code_page_range2,
        sx_height: os2_table.sx_height,
        s_cap_height: os2_table.s_cap_height,
        us_default_char: os2_table.us_default_char,
        us_break_char: os2_table.us_break_char,
        us_max_context: os2_table.us_max_context,
        us_lower_optical_point_size: os2_table.us_lower_optical_point_size,
        us_upper_optical_point_size: os2_table.us_upper_optical_point_size,
    }
}

pub struct Font<'a> {
    pub scope: ReadScope<'a>,
    pub font_file: FontFile<'a>,
    pub font_data_impl: FontDataImpl<FileTableProvider<'a>>,
    pub font_metrics: FontMetrics,
    pub gsub_cache: LayoutCache<GSUB>,
    pub gpos_cache: LayoutCache<GPOS>,
    pub gdef_table: Rc<GDEFTable>,
}

impl<'a> Font<'a> {
    pub fn from_bytes(font_bytes: &'a [u8], font_index: usize) -> Option<Self> {

        let scope = ReadScope::new(font_bytes);
        let font_file = scope.read::<FontFile<'_>>().ok()?;
        let provider = font_file.table_provider(font_index).ok()?;
        let font_data_impl = FontDataImpl::new(Box::new(provider)).ok()??;
        let font_metrics = get_font_metrics(font_bytes, font_index);

        // required for font layout: gsub_cache, gpos_cache and gdef_table
        let gsub_cache = font_data_impl.gsub_cache().ok()??;
        let gpos_cache = font_data_impl.gpos_cache().ok()??;
        let gdef_table = font_data_impl.gdef_table().ok()??;

        Some(Font {
            scope,
            font_file,
            font_data_impl,
            font_metrics,
            gsub_cache,
            gpos_cache,
            gdef_table,
        })
    }

    pub fn shape(&mut self, text: &[char], script: u32, lang: u32) -> ShapedTextBufferUnsized {
        shape(self, text, script, lang).unwrap_or_default()
    }
}

#[derive(Debug, PartialEq, Default)]
pub struct ShapedTextBufferUnsized {
    pub infos: Vec<Info>,
    pub horizontal_advances: Vec<HorizontalAdvance>,
}

impl ShapedTextBufferUnsized {
    pub fn get_word_visual_width_unscaled(&self) -> isize {
        self.horizontal_advances.iter().map(|s| s.total_unscaled() as isize).sum()
    }
}

pub fn estimate_script_and_language(text: &str) -> (u32, u32) {

    use allsorts::tag;
    use whatlang::{Script, Lang};

    // auto-detect script + language from text (todo: performance!)
    let (lang, script) = whatlang::detect(text)
        .map(|info| (info.lang(), info.script()))
        .unwrap_or((Lang::Eng, Script::Latin));

    let lang = tag::from_string(&lang.code().to_string().to_uppercase()).unwrap();

    let script = match script {
        Script::Arabic          => tag::ARAB,
        Script::Bengali         => tag::BENG,
        Script::Cyrillic        => tag::CYRL,
        Script::Devanagari      => tag::DEVA,
        Script::Ethiopic        => tag::LATN, // ??
        Script::Georgian        => tag::LATN, // ??
        Script::Greek           => tag::GREK,
        Script::Gujarati        => tag::GUJR,
        Script::Gurmukhi        => tag::GURU, // can also be GUR2
        Script::Hangul          => tag::LATN, // ??
        Script::Hebrew          => tag::LATN, // ??
        Script::Hiragana        => tag::LATN, // ??
        Script::Kannada         => tag::KNDA,
        Script::Katakana        => tag::LATN, // ??
        Script::Khmer           => tag::LATN, // TODO?? - unsupported?
        Script::Latin           => tag::LATN,
        Script::Malayalam       => tag::MLYM,
        Script::Mandarin        => tag::LATN, // ??
        Script::Myanmar         => tag::LATN, // ??
        Script::Oriya           => tag::ORYA,
        Script::Sinhala         => tag::SINH,
        Script::Tamil           => tag::TAML,
        Script::Telugu          => tag::TELU,
        Script::Thai            => tag::LATN, // ?? - Khmer, not supported?
    };

    (script, lang)
}

// shape_word(text: &str, &font) -> TextBuffer
// get_word_visual_width(word: &TextBuffer) ->
// get_glyph_instances(infos: &GlyphInfos, positions: &GlyphPositions) -> PositionedGlyphBuffer

fn shape<'a>(font: &mut Font, text: &[char], script: u32, lang: u32) -> Option<ShapedTextBufferUnsized> {

    use std::convert::TryFrom;
    use allsorts::gpos::gpos_apply;
    use allsorts::gsub::gsub_apply_default;

    // Map glyphs
    //
    // We look ahead in the char stream for variation selectors. If one is found it is used for
    // mapping the current glyph. When a variation selector is reached in the stream it is skipped
    // as it was handled as part of the preceding character.
    let mut chars_iter = text.iter().peekable();
    let mut glyphs = Vec::new();

    while let Some(ch) = chars_iter.next() {
        match allsorts::unicode::VariationSelector::try_from(*ch) {
            Ok(_) => {} // filter out variation selectors
            Err(()) => {
                let vs = chars_iter
                    .peek()
                    .and_then(|&next| allsorts::unicode::VariationSelector::try_from(*next).ok());

                // TODO: Remove cast when lookup_glyph_index returns u16
                let glyph_index = font.font_data_impl.lookup_glyph_index(*ch as u32) as u16;
                let glyph = make_raw_glyph(*ch, glyph_index, vs);
                glyphs.push(glyph);
            }
        }
    }

    // Apply glyph substitution if table is present
    gsub_apply_default(
        &|| make_dotted_circle(&font.font_data_impl),
        &font.gsub_cache,
        Some(Rc::as_ref(&font.gdef_table)),
        script,
        lang,
        allsorts::gsub::GsubFeatureMask::default(),
        font.font_data_impl.num_glyphs(),
        &mut glyphs,
    ).ok()?;

    // Apply glyph positioning if table is present

    let kerning = true;
    let mut infos = allsorts::gpos::Info::init_from_glyphs(Some(&font.gdef_table), glyphs).ok()?;
    gpos_apply(
        &font.gpos_cache,
        Some(Rc::as_ref(&font.gdef_table)),
        kerning,
        script,
        lang,
        &mut infos,
    ).ok()?;

    // calculate the horizontal advance for each char
    let horizontal_advances = infos.iter().map(|info| {
        match info.glyph.glyph_index {
            Some(s) => {
                HorizontalAdvance {
                    advance: font.font_data_impl.horizontal_advance(s),
                    kerning: info.kerning,
                }
            },
            None => {
                HorizontalAdvance {
                    advance: 0,
                    kerning: 0,
                }
            }
        }
    }).collect();

    let infos = infos.into_iter().map(|i| translate_info(&i)).collect();

    Some(ShapedTextBufferUnsized { infos, horizontal_advances })
}

fn make_raw_glyph(ch: char, glyph_index: u16, variation: Option<allsorts::unicode::VariationSelector>) -> allsorts::gsub::RawGlyph<()> {
    allsorts::gsub::RawGlyph {
        unicodes: tiny_vec![[char; 1], ch],
        glyph_index: Some(glyph_index),
        liga_component_pos: 0,
        glyph_origin: allsorts::gsub::GlyphOrigin::Char(ch),
        small_caps: false,
        multi_subst_dup: false,
        is_vert_alt: false,
        fake_bold: false,
        fake_italic: false,
        extra_data: (),
        variation,
    }
}

#[inline]
fn make_dotted_circle<'a>(font_data_impl: &FontDataImpl<FileTableProvider<'a>>) -> Vec<allsorts::gsub::RawGlyph<()>> {
    const DOTTED_CIRCLE: char = '\u{25cc}';
    // TODO: Remove cast when lookup_glyph_index returns u16
    let glyph_index = font_data_impl.lookup_glyph_index(DOTTED_CIRCLE as u32) as u16;
    vec![make_raw_glyph(DOTTED_CIRCLE, glyph_index, None)]
}

#[inline]
fn translate_info(i: &allsorts::gpos::Info) -> Info {
    Info {
        glyph: translate_raw_glyph(&i.glyph),
        kerning: i.kerning,
        placement: translate_placement(&i.placement),
        mark_placement: translate_mark_placement(&i.mark_placement),
        is_mark: i.is_mark,
    }
}

#[inline]
fn translate_raw_glyph(rg: &allsorts::gsub::RawGlyph<()>) -> RawGlyph {
    RawGlyph {
        unicodes: [rg.unicodes[0]],
        glyph_index: rg.glyph_index,
        liga_component_pos: rg.liga_component_pos,
        glyph_origin: translate_glyph_origin(&rg.glyph_origin),
        small_caps: rg.small_caps,
        multi_subst_dup: rg.multi_subst_dup,
        is_vert_alt: rg.is_vert_alt,
        fake_bold: rg.fake_bold,
        fake_italic: rg.fake_italic,
        variation: rg.variation.as_ref().map(translate_variation_selector),
        extra_data: (),
    }
}

#[inline]
fn translate_glyph_origin(g: &allsorts::gsub::GlyphOrigin) -> GlyphOrigin {
    use allsorts::gsub::GlyphOrigin::*;
    match g {
        Char(c) => GlyphOrigin::Char(*c),
        Direct => GlyphOrigin::Direct,
    }
}

#[inline]
fn translate_placement(p: &allsorts::gpos::Placement) -> Placement {
    use allsorts::gpos::Placement::*;
    match p {
        None => Placement::None,
        Distance(x, y) => Placement::Distance(*x, *y),
        Anchor(a, b) => Placement::Anchor(translate_anchor(a), translate_anchor(b)),
    }
}

#[inline]
fn translate_mark_placement(mp: &allsorts::gpos::MarkPlacement) -> MarkPlacement {
    use allsorts::gpos::MarkPlacement::*;
    match mp {
        None => MarkPlacement::None,
        MarkAnchor(a, b, c) => MarkPlacement::MarkAnchor(*a, translate_anchor(b), translate_anchor(c)),
    }
}

fn translate_variation_selector(v: &allsorts::unicode::VariationSelector) -> VariationSelector {
    use allsorts::unicode::VariationSelector::*;
    match v {
        VS01 => VariationSelector::VS01,
        VS02 => VariationSelector::VS02,
        VS03 => VariationSelector::VS03,
        VS15 => VariationSelector::VS15,
        VS16 => VariationSelector::VS16,
    }
}

#[inline]
fn translate_anchor(anchor: &allsorts::layout::Anchor) -> Anchor { Anchor { x: anchor.x, y: anchor.y } }