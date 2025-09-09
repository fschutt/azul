use std::collections::HashMap;

// Enhanced layout constraints with writing mode support
#[derive(Debug, Clone)]
pub struct LayoutConstraints {
    pub available_width: f32,
    pub available_height: f32,
    pub exclusion_areas: Vec<ExclusionRect>,
    pub writing_mode: WritingMode,
    pub text_align: TextAlign,
    pub justify_content: JustifyContent,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WritingMode {
    HorizontalTb,  // horizontal-tb (normal horizontal)
    VerticalRl,    // vertical-rl (vertical right-to-left)
    VerticalLr,    // vertical-lr (vertical left-to-right)
    SidewaysRl,    // sideways-rl (rotated horizontal in vertical context)
    SidewaysLr,    // sideways-lr (rotated horizontal in vertical context)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JustifyContent {
    None,
    InterWord,     // Expand spaces between words
    InterCharacter, // Expand spaces between all characters (for CJK)
    Distribute,    // Distribute space evenly including start/end
}

// Enhanced text alignment with logical directions
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left, Right, Center,
    Justify,
    Start, End,        // Logical start/end
    JustifyAll,        // Justify including last line
}

// Vertical text orientation for individual characters
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextOrientation {
    Mixed,      // Default: upright for scripts, rotated for others
    Upright,    // All characters upright
    Sideways,   // All characters rotated 90 degrees
}

// Enhanced style properties with vertical text support
#[derive(Debug, Clone, PartialEq)]
pub struct StyleProperties {
    pub font_ref: FontRef,
    pub font_size_px: f32,
    pub color: Color,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    pub line_height: f32,
    pub text_decoration: TextDecoration,
    pub font_features: Vec<String>,
    
    // Vertical text properties
    pub writing_mode: WritingMode,
    pub text_orientation: TextOrientation,
    pub text_combine_upright: Option<TextCombineUpright>, // tate-chu-yoko
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextCombineUpright {
    None,
    All,           // Combine all characters in horizontal layout
    Digits(u8),    // Combine up to N digits
}

// Enhanced glyph with vertical metrics and justification info
#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    pub glyph_id: u16,
    pub style: StyleProperties,
    
    // Horizontal metrics
    pub advance: f32,
    pub x_offset: f32,
    pub y_offset: f32,
    
    // Vertical metrics (for vertical text)
    pub vertical_advance: f32,
    pub vertical_x_offset: f32,
    pub vertical_y_offset: f32,
    pub vertical_origin_y: f32,  // From VORG table
    
    // Source mapping
    pub logical_byte_start: usize,
    pub logical_byte_len: u8,
    pub cluster: u32,
    
    // Layout properties
    pub source: GlyphSource,
    pub is_whitespace: bool,
    pub break_opportunity_after: bool,
    pub can_justify: bool,           // Can this glyph be expanded for justification?
    pub justification_priority: u8,  // 0 = highest priority (spaces), 255 = lowest
    pub character_class: CharacterClass, // For justification rules
    pub text_orientation: GlyphOrientation, // How this glyph should be oriented
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CharacterClass {
    Space,          // Regular spaces - highest justification priority
    Punctuation,    // Can sometimes be adjusted
    Letter,         // Normal letters
    Ideograph,      // CJK characters - can be justified between
    Symbol,         // Symbols, emojis
    Combining,      // Combining marks - never justified
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphOrientation {
    Horizontal,     // Keep horizontal (normal in horizontal text)
    Vertical,       // Rotate to vertical (normal in vertical text)  
    Upright,        // Keep upright regardless of writing mode
    Mixed,          // Use script-specific default orientation
}

// Justification engine
#[derive(Debug)]
pub struct JustificationEngine;

impl JustificationEngine {
    pub fn justify_line(
        glyphs: &mut [ShapedGlyph],
        target_width: f32,
        justify_content: JustifyContent,
        writing_mode: WritingMode,
        is_last_line: bool,
    ) -> Result<(), LayoutError> {
        if is_last_line && justify_content != JustifyContent::Distribute {
            return Ok(()); // Don't justify last line unless explicitly requested
        }

        let current_width = Self::calculate_line_width(glyphs, writing_mode);
        if current_width >= target_width {
            return Ok(()); // Already fits or overflows
        }

        let available_space = target_width - current_width;
        
        match justify_content {
            JustifyContent::None => Ok(()),
            JustifyContent::InterWord => Self::justify_inter_word(glyphs, available_space),
            JustifyContent::InterCharacter => Self::justify_inter_character(glyphs, available_space),
            JustifyContent::Distribute => Self::justify_distribute(glyphs, available_space),
        }
    }

    fn calculate_line_width(glyphs: &[ShapedGlyph], writing_mode: WritingMode) -> f32 {
        glyphs.iter()
            .map(|g| Self::get_glyph_advance(g, writing_mode))
            .sum()
    }

    fn get_glyph_advance(glyph: &ShapedGlyph, writing_mode: WritingMode) -> f32 {
        match writing_mode {
            WritingMode::HorizontalTb => glyph.advance,
            WritingMode::VerticalRl | WritingMode::VerticalLr => glyph.vertical_advance,
            WritingMode::SidewaysRl | WritingMode::SidewaysLr => glyph.advance,
        }
    }

    fn justify_inter_word(glyphs: &mut [ShapedGlyph], available_space: f32) -> Result<(), LayoutError> {
        // Find all word boundaries (spaces and break opportunities)
        let space_indices: Vec<usize> = glyphs.iter()
            .enumerate()
            .filter_map(|(i, g)| {
                if g.character_class == CharacterClass::Space || 
                   (g.break_opportunity_after && g.character_class != CharacterClass::Combining) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        if space_indices.is_empty() {
            return Ok(());
        }

        let space_per_gap = available_space / space_indices.len() as f32;

        // Distribute space by expanding advances
        for &idx in &space_indices {
            glyphs[idx].advance += space_per_gap;
        }

        Ok(())
    }

    fn justify_inter_character(glyphs: &mut [ShapedGlyph], available_space: f32) -> Result<(), LayoutError> {
        // For CJK text - expand space between all characters
        let justifiable_gaps: Vec<usize> = glyphs.iter()
            .enumerate()
            .filter_map(|(i, g)| {
                if g.can_justify && 
                   g.character_class != CharacterClass::Combining &&
                   i < glyphs.len() - 1 { // Don't justify after last glyph
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        if justifiable_gaps.is_empty() {
            return Self::justify_inter_word(glyphs, available_space);
        }

        let space_per_gap = available_space / justifiable_gaps.len() as f32;

        for &idx in &justifiable_gaps {
            glyphs[idx].advance += space_per_gap;
        }

        Ok(())
    }

    fn justify_distribute(glyphs: &mut [ShapedGlyph], available_space: f32) -> Result<(), LayoutError> {
        // CSS text-align: justify - distribute space including at edges
        if glyphs.is_empty() {
            return Ok(());
        }

        // Add space at start, between characters, and at end
        let gaps = glyphs.len() + 1;
        let space_per_gap = available_space / gaps as f32;

        // Add space to each glyph's advance (except maybe the last)
        for glyph in glyphs.iter_mut() {
            glyph.advance += space_per_gap;
        }

        // The remaining space goes before the first character (handled in positioning)
        Ok(())
    }
}

// Vertical text layout engine
#[derive(Debug)]
pub struct VerticalLayoutEngine;

impl VerticalLayoutEngine {
    pub fn determine_glyph_orientation(
        codepoint: u32,
        script: Script,
        text_orientation: TextOrientation,
        writing_mode: WritingMode,
    ) -> GlyphOrientation {
        match text_orientation {
            TextOrientation::Upright => GlyphOrientation::Upright,
            TextOrientation::Sideways => GlyphOrientation::Horizontal,
            TextOrientation::Mixed => {
                Self::get_default_orientation(codepoint, script, writing_mode)
            }
        }
    }

    fn get_default_orientation(codepoint: u32, script: Script, writing_mode: WritingMode) -> GlyphOrientation {
        // Based on Unicode Vertical Orientation property
        match codepoint {
            // CJK ideographs, symbols - upright in vertical text
            0x4E00..=0x9FFF | // CJK Unified Ideographs
            0x3400..=0x4DBF | // CJK Extension A
            0x20000..=0x2A6DF => GlyphOrientation::Upright,
            
            // Latin, Arabic, etc. - rotated in vertical text
            0x0020..=0x007F => GlyphOrientation::Horizontal,
            
            // Punctuation - context dependent
            0x3000..=0x303F => Self::get_punctuation_orientation(codepoint, writing_mode),
            
            // Default: use script-based heuristic
            _ => Self::get_script_default_orientation(script, writing_mode)
        }
    }

    fn get_punctuation_orientation(codepoint: u32, writing_mode: WritingMode) -> GlyphOrientation {
        match codepoint {
            // Vertical forms of punctuation
            0x3001 | 0x3002 | // Ideographic comma, full stop
            0x300C | 0x300D | // Corner brackets
            0x300E | 0x300F |
            0x3010 | 0x3011 => GlyphOrientation::Upright,
            
            _ => GlyphOrientation::Horizontal,
        }
    }

    fn get_script_default_orientation(script: Script, writing_mode: WritingMode) -> GlyphOrientation {
        // Simplified script classification
        match script.0 {
            // Scripts that are traditionally vertical
            17 | 18 | 19 => GlyphOrientation::Upright, // Han, Hiragana, Katakana
            _ => GlyphOrientation::Horizontal,
        }
    }

    pub fn apply_vertical_metrics(glyph: &mut ShapedGlyph, font: &ParsedFont) {
        // Get vertical metrics from VMTX, VORG tables
        if let Some(v_metrics) = font.get_vertical_metrics(glyph.glyph_id) {
            glyph.vertical_advance = v_metrics.advance;
            glyph.vertical_x_offset = v_metrics.bearing_x;
            glyph.vertical_y_offset = v_metrics.bearing_y;
            glyph.vertical_origin_y = v_metrics.origin_y;
        } else {
            // Fallback: derive from horizontal metrics
            glyph.vertical_advance = glyph.style.line_height;
            glyph.vertical_x_offset = -glyph.advance / 2.0;
            glyph.vertical_y_offset = 0.0;
            glyph.vertical_origin_y = glyph.style.font_size_px * 0.88; // Approximate
        }
    }
}

// Enhanced positioning for vertical text and justification
impl ParagraphLayout {
    pub fn position_glyphs_advanced(
        mut shaped_glyphs: Vec<ShapedGlyph>,
        constraints: LayoutConstraints,
        source_text: &str,
        base_direction: Direction,
        font_manager: &mut FontManager,
    ) -> Result<ParagraphLayout, LayoutError> {
        let mut positioned_glyphs = Vec::new();
        let mut lines = Vec::new();
        
        let is_vertical = matches!(constraints.writing_mode, 
            WritingMode::VerticalRl | WritingMode::VerticalLr);
        
        let mut line_position = if is_vertical { 0.0 } else { 0.0 };
        let mut glyph_cursor = 0;

        while glyph_cursor < shaped_glyphs.len() {
            let (line_end_idx, _) = Self::find_line_break_advanced(
                &shaped_glyphs,
                glyph_cursor,
                &constraints,
                source_text,
                font_manager,
            )?;

            let mut line_glyphs = shaped_glyphs[glyph_cursor..line_end_idx].to_vec();
            
            // Apply justification
            let line_measure = if is_vertical {
                constraints.available_height
            } else {
                constraints.available_width
            };
            
            let is_last_line = line_end_idx >= shaped_glyphs.len();
            JustificationEngine::justify_line(
                &mut line_glyphs,
                line_measure,
                constraints.justify_content,
                constraints.writing_mode,
                is_last_line,
            )?;

            // Position glyphs in the line
            let (mut positioned_line_glyphs, line_layout) = Self::finalize_line_advanced(
                &line_glyphs,
                line_position,
                &constraints,
                glyph_cursor,
            )?;

            positioned_glyphs.append(&mut positioned_line_glyphs);
            lines.push(line_layout.clone());

            // Advance to next line
            if is_vertical {
                line_position -= line_layout.bounds.width; // Move left/right in vertical
            } else {
                line_position += line_layout.bounds.height; // Move down in horizontal
            }

            glyph_cursor = line_end_idx;
        }

        let content_size = Self::calculate_content_size_advanced(&lines, constraints.writing_mode);

        Ok(ParagraphLayout {
            glyphs: positioned_glyphs,
            lines,
            content_size,
            source_text: source_text.to_string(),
            base_direction,
        })
    }

    fn finalize_line_advanced(
        glyphs_on_line: &[ShapedGlyph],
        line_position: f32,
        constraints: &LayoutConstraints,
        glyph_start_index: usize,
    ) -> Result<(Vec<PositionedGlyph>, LineLayout), LayoutError> {
        if glyphs_on_line.is_empty() {
            return Ok((vec![], LineLayout::empty(line_position, constraints.writing_mode)));
        }

        let is_vertical = matches!(constraints.writing_mode, 
            WritingMode::VerticalRl | WritingMode::VerticalLr);

        let mut positioned_glyphs = Vec::new();
        let mut current_inline_pos = 0.0; // Position along the line direction
        let mut max_block_size = 0.0;     // Maximum size perpendicular to line

        // Calculate alignment offset
        let line_measure = Self::calculate_line_measure(glyphs_on_line, constraints.writing_mode);
        let available_measure = if is_vertical {
            constraints.available_height
        } else {
            constraints.available_width
        };

        let alignment_offset = Self::calculate_alignment_offset(
            constraints.text_align,
            line_measure,
            available_measure,
            base_direction,
        );

        for (i, glyph) in glyphs_on_line.iter().enumerate() {
            let (x, y, bounds) = if is_vertical {
                // Vertical text layout
                let glyph_height = glyph.vertical_advance;
                let glyph_width = glyph.style.font_size_px; // Approximate
                
                max_block_size = max_block_size.max(glyph_width);
                
                let x = match constraints.writing_mode {
                    WritingMode::VerticalRl => line_position - glyph_width + glyph.vertical_x_offset,
                    WritingMode::VerticalLr => line_position + glyph.vertical_x_offset,
                    _ => unreachable!(),
                };
                
                let y = alignment_offset + current_inline_pos + glyph.vertical_y_offset;
                
                current_inline_pos += glyph_height;
                
                (x, y, Rect {
                    x: x - glyph_width / 2.0,
                    y,
                    width: glyph_width,
                    height: glyph_height,
                })
            } else {
                // Horizontal text layout
                let glyph_width = glyph.advance;
                let glyph_height = glyph.style.line_height;
                
                max_block_size = max_block_size.max(glyph_height);
                
                let x = alignment_offset + current_inline_pos + glyph.x_offset;
                let y = line_position + glyph.style.font_size_px + glyph.y_offset; // Baseline
                
                current_inline_pos += glyph_width;
                
                (x, y, Rect {
                    x: x,
                    y: line_position,
                    width: glyph_width,
                    height: glyph_height,
                })
            };

            positioned_glyphs.push(PositionedGlyph {
                glyph_id: glyph.glyph_id,
                style: glyph.style.clone(),
                x,
                y,
                bounds,
                advance: if is_vertical { glyph.vertical_advance } else { glyph.advance },
                line_index: 0, // Set in final pass
                logical_char_byte_index: glyph.logical_byte_start,
                logical_char_byte_count: glyph.logical_byte_len,
                visual_index: glyph_start_index + i,
                bidi_level: BidiLevel::new(0), // TODO: Preserve from analysis
            });
        }

        let line_bounds = if is_vertical {
            Rect {
                x: line_position - max_block_size,
                y: alignment_offset,
                width: max_block_size,
                height: current_inline_pos,
            }
        } else {
            Rect {
                x: alignment_offset,
                y: line_position,
                width: current_inline_pos,
                height: max_block_size,
            }
        };

        let line_layout = LineLayout {
            bounds: line_bounds,
            baseline_y: if is_vertical { 0.0 } else { line_position + max_block_size * 0.8 },
            glyph_start: glyph_start_index,
            glyph_count: glyphs_on_line.len(),
            logical_start_byte: glyphs_on_line.first().unwrap().logical_byte_start,
            logical_end_byte: glyphs_on_line.last().unwrap().logical_byte_start + 
                             glyphs_on_line.last().unwrap().logical_byte_len as usize,
        };

        Ok((positioned_glyphs, line_layout))
    }

    fn calculate_line_measure(glyphs: &[ShapedGlyph], writing_mode: WritingMode) -> f32 {
        glyphs.iter()
            .map(|g| JustificationEngine::get_glyph_advance(g, writing_mode))
            .sum()
    }

    fn calculate_alignment_offset(
        align: TextAlign,
        line_measure: f32,
        available_measure: f32,
        base_direction: Direction,
    ) -> f32 {
        let logical_align = resolve_logical_align(align, base_direction);
        
        match logical_align {
            TextAlign::Left | TextAlign::Start => 0.0,
            TextAlign::Right | TextAlign::End => (available_measure - line_measure).max(0.0),
            TextAlign::Center => ((available_measure - line_measure) / 2.0).max(0.0),
            TextAlign::Justify | TextAlign::JustifyAll => 0.0, // Already justified
        }
    }

    fn calculate_content_size_advanced(lines: &[LineLayout], writing_mode: WritingMode) -> Size {
        if lines.is_empty() {
            return Size { width: 0.0, height: 0.0 };
        }

        match writing_mode {
            WritingMode::HorizontalTb => {
                let width = lines.iter()
                    .map(|line| line.bounds.x + line.bounds.width)
                    .fold(0.0f32, f32::max);
                let height = lines.last().unwrap().bounds.y + lines.last().unwrap().bounds.height;
                Size { width, height }
            }
            WritingMode::VerticalRl | WritingMode::VerticalLr => {
                let width = lines.iter()
                    .map(|line| line.bounds.width)
                    .sum::<f32>();
                let height = lines.iter()
                    .map(|line| line.bounds.y + line.bounds.height)
                    .fold(0.0f32, f32::max);
                Size { width, height }
            }
            WritingMode::SidewaysRl | WritingMode::SidewaysLr => {
                // Similar to vertical but with different orientation handling
                Self::calculate_content_size_advanced(lines, WritingMode::VerticalRl)
            }
        }
    }

    fn find_line_break_advanced(
        glyphs: &[ShapedGlyph],
        start_idx: usize,
        constraints: &LayoutConstraints,
        source_text: &str,
        font_manager: &mut FontManager,
    ) -> Result<(usize, bool), LayoutError> {
        // This would be similar to the existing find_line_break but accounting for
        // vertical text constraints and different line break opportunities
        find_line_break(glyphs, start_idx, 0.0, constraints, &get_hyphenator()?, source_text)
            .map_err(|_| LayoutError::InvalidText("Line breaking failed".to_string()))
            .map(|(idx, needs_hyphen)| Ok((idx, needs_hyphen)))?
    }
}

impl LineLayout {
    fn empty(position: f32, writing_mode: WritingMode) -> Self {
        let bounds = match writing_mode {
            WritingMode::HorizontalTb => Rect { x: 0.0, y: position, width: 0.0, height: 16.0 },
            _ => Rect { x: position, y: 0.0, width: 16.0, height: 0.0 },
        };

        LineLayout {
            bounds,
            baseline_y: position,
            glyph_start: 0,
            glyph_count: 0,
            logical_start_byte: 0,
            logical_end_byte: 0,
        }
    }
}

// Enhanced font metrics for vertical text support
pub struct VerticalMetrics {
    pub advance: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub origin_y: f32,
}

impl ParsedFont {
    pub fn get_vertical_metrics(&self, glyph_id: u16) -> Option<VerticalMetrics> {
        // Read from VMTX and VORG tables
        unimplemented!("Vertical metrics from font tables")
    }
}

// Main entry point with advanced layout support
pub fn layout_paragraph_advanced(
    styled_runs: Vec<StyledRun>,
    constraints: LayoutConstraints,
    font_manager: &mut FontManager,
) -> Result<Arc<ParagraphLayout>, LayoutError> {
    let full_logical_text = concatenate_runs_text(&styled_runs);
    
    // Analyze text and build fallback chains
    for run in &styled_runs {
        font_manager.build_fallback_chain(&run.style.font_ref, &full_logical_text)?;
    }
    
    let (visual_runs, base_direction) = perform_bidi_analysis(&styled_runs, &full_logical_text)?;
    
    // Enhanced shaping with vertical text orientation
    let shaped_glyphs = shape_visual_runs_with_vertical_support(&visual_runs, font_manager)?;
    
    // Position with justification and vertical layout support
    let layout = ParagraphLayout::position_glyphs_advanced(
        shaped_glyphs,
        constraints,
        &full_logical_text,
        base_direction,
        font_manager,
    )?;
    
    Ok(Arc::new(layout))
}

fn shape_visual_runs_with_vertical_support(
    visual_runs: &[VisualRun],
    font_manager: &mut FontManager,
) -> Result<Vec<ShapedGlyph>, LayoutError> {
    let mut all_shaped_glyphs = Vec::new();

    for run in visual_runs {
        let direction = if run.bidi_level.is_rtl() { Direction::Rtl } else { Direction::Ltr };
        let mut shaped_glyphs = shape_run_with_fallback(run, font_manager, direction)?;
        
        // Apply vertical text processing
        for glyph in &mut shaped_glyphs {
            // Determine glyph orientation
            let codepoint = run.text_slice[glyph.logical_byte_start - run.logical_start_byte..]
                .chars().next().unwrap_or('\0') as u32;
            
            glyph.text_orientation = VerticalLayoutEngine::determine_glyph_orientation(
                codepoint,
                run.script,
                run.style.text_orientation,
                run.style.writing_mode,
            );
            
            // Set character class for justification
            glyph.character_class = Self::classify_character(codepoint);
            glyph.can_justify = glyph.character_class != CharacterClass::Combining;
            glyph.justification_priority = Self::get_justification_priority(glyph.character_class);
            
            // Apply vertical metrics if needed
            if matches!(run.style.writing_mode, WritingMode::VerticalRl | WritingMode::VerticalLr) {
                let font = font_manager.load_font(&run.style.font_ref)?;
                VerticalLayoutEngine::apply_vertical_metrics(glyph, &font);
            }
        }
        
        all_shaped_glyphs.extend(shaped_glyphs);
    }
    
    Ok(all_shaped_glyphs)
}

impl Self {
    fn classify_character(codepoint: u32) -> CharacterClass {
        match codepoint {
            0x0020 | 0x00A0 | 0x3000 => CharacterClass::Space,
            0x0021..=0x002F | 0x003A..=0x0040 | 0x005B..=0x0060 | 0x007B..=0x007E => CharacterClass::Punctuation,
            0x4E00..=0x9FFF | 0x3400..=0x4DBF => CharacterClass::Ideograph,
            0x0300..=0x036F | 0x1AB0..=0x1AFF => CharacterClass::Combining,
            _ => CharacterClass::Letter,
        }
    }
    
    fn get_justification_priority(class: CharacterClass) -> u8 {
        match class {
            CharacterClass::Space => 0,        // Highest priority
            CharacterClass::Punctuation => 64,
            CharacterClass::Ideograph => 128,  // Medium priority for CJK
            CharacterClass::Letter => 192,
            CharacterClass::Symbol => 224,
            CharacterClass::Combining => 255,  // Never justify
        }
    }
}