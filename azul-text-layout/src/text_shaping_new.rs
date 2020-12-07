use azul_core::app_resources::FontMetrics;

pub fn get_font_metrics(font_bytes: &[u8], font_index: usize) -> FontMetrics {

    use allsorts::binary::read::ReadScope;
    use allsorts::fontfile::FontFile;
    use allsorts::font_data_impl::FontDataImpl;

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

/*
fn shape<'a, P: FontTableProvider>(mut font: FontDataImpl<P>, script: u32, lang: u32, text: &str) -> Result<(), ShapingError> {

    let opt_gsub_cache = font.gsub_cache()?;
    let opt_gpos_cache = font.gpos_cache()?;
    let opt_gdef_table = font.gdef_table()?;
    let opt_gdef_table = opt_gdef_table.as_ref().map(Rc::as_ref);

    // Map glyphs
    //
    // We look ahead in the char stream for variation selectors. If one is found it is used for
    // mapping the current glyph. When a variation selector is reached in the stream it is skipped
    // as it was handled as part of the preceding character.
    let mut chars_iter = text.chars().peekable();
    let mut glyphs = Vec::new();
    while let Some(ch) = chars_iter.next() {
        match VariationSelector::try_from(ch) {
            Ok(_) => {} // filter out variation selectors
            Err(()) => {
                let vs = chars_iter
                    .peek()
                    .and_then(|&next| VariationSelector::try_from(next).ok());
                // TODO: Remove cast when lookup_glyph_index returns u16
                let glyph_index = font.lookup_glyph_index(ch as u32) as u16;
                let glyph = glyph::make(ch, glyph_index, vs);
                glyphs.push(glyph);
            }
        }
    }

    // Apply gsub if table is present
    println!("glyphs before: {:#?}", glyphs);
    if let Some(gsub_cache) = opt_gsub_cache {
        gsub_apply_default(
            &|| make_dotted_circle(&font),
            &gsub_cache,
            opt_gdef_table,
            script,
            lang,
            GsubFeatureMask::default(),
            font.num_glyphs(),
            &mut glyphs,
        )?;

        // Apply gpos if table is present
        if let Some(gpos_cache) = opt_gpos_cache {
            let kerning = true;
            let mut infos = Info::init_from_glyphs(opt_gdef_table, glyphs)?;
            gpos_apply(
                &gpos_cache,
                opt_gdef_table,
                kerning,
                script,
                lang,
                &mut infos,
            )?;
        }
    }

    Ok(glyphs)
}

#[inline]
fn make_dotted_circle<P: FontTableProvider>(font_data_impl: &FontDataImpl<P>) -> Vec<RawGlyph<()>> {
    // TODO: Remove cast when lookup_glyph_index returns u16
    let glyph_index = font_data_impl.lookup_glyph_index(DOTTED_CIRCLE as u32) as u16;
    vec![glyph::make(DOTTED_CIRCLE, glyph_index, None)]
}
*/