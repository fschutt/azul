use std::sync::Arc;
use std::collections::HashMap;
use rust_fontconfig::{FcFontCache, FcPattern, FcFontMatch};

// Enhanced font management with fallback chains
#[derive(Debug, Clone)]
pub struct FontFallbackChain {
    pub primary: FontRef,
    pub fallbacks: Vec<FontRef>,
    pub script_specific: HashMap<Script, Vec<FontRef>>,
}

#[derive(Debug)]
pub struct FontManager {
    fc_cache: FcFontCache,
    parsed_fonts: HashMap<FontId, Arc<ParsedFont>>,
    fallback_chains: HashMap<FontRef, FontFallbackChain>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub [u8; 16]); // From rust-fontconfig

impl FontManager {
    pub fn new() -> Result<Self, LayoutError> {
        let fc_cache = FcFontCache::build();
        Ok(Self {
            fc_cache,
            parsed_fonts: HashMap::new(),
            fallback_chains: HashMap::new(),
        })
    }

    // Build fallback chain for a given font request and text content
    pub fn build_fallback_chain(&mut self, font_ref: &FontRef, text: &str) -> Result<FontFallbackChain, LayoutError> {
        if let Some(cached) = self.fallback_chains.get(font_ref) {
            return Ok(cached.clone());
        }

        let mut trace = Vec::new();
        
        // First try exact match
        let pattern = FcPattern {
            name: Some(font_ref.family.clone()),
            weight: Some(weight_to_fc_weight(font_ref.weight)),
            slant: Some(style_to_fc_slant(font_ref.style)),
            ..Default::default()
        };

        let primary_match = self.fc_cache.query(&pattern, &mut trace)
            .ok_or_else(|| LayoutError::FontNotFound(font_ref.clone()))?;

        // Then find fallbacks for the specific text
        let fallback_matches = self.fc_cache.query_for_text(&pattern, text, &mut trace);
        
        // Convert to our FontRef format and filter out duplicates
        let mut fallbacks = Vec::new();
        let mut script_specific = HashMap::new();

        for fc_match in fallback_matches {
            let fallback_ref = self.fc_match_to_font_ref(&fc_match)?;
            if fallback_ref != *font_ref && !fallbacks.contains(&fallback_ref) {
                fallbacks.push(fallback_ref.clone());
                
                // Group by script for efficient lookup
                for &script in &fc_match.scripts {
                    script_specific.entry(Script(script))
                        .or_insert_with(Vec::new)
                        .push(fallback_ref.clone());
                }
            }
        }

        let chain = FontFallbackChain {
            primary: font_ref.clone(),
            fallbacks,
            script_specific,
        };

        self.fallback_chains.insert(font_ref.clone(), chain.clone());
        Ok(chain)
    }

    pub fn get_font_for_text(&mut self, font_ref: &FontRef, text: &str, script: Script) -> Result<Arc<ParsedFont>, LayoutError> {
        // Try primary font first
        if let Ok(font) = self.load_font(font_ref) {
            if self.font_supports_text(&font, text) {
                return Ok(font);
            }
        }

        // Build fallback chain if needed
        let chain = self.build_fallback_chain(font_ref, text)?;

        // Try script-specific fallbacks first
        if let Some(script_fonts) = chain.script_specific.get(&script) {
            for fallback_ref in script_fonts {
                if let Ok(font) = self.load_font(fallback_ref) {
                    if self.font_supports_text(&font, text) {
                        return Ok(font);
                    }
                }
            }
        }

        // Try general fallbacks
        for fallback_ref in &chain.fallbacks {
            if let Ok(font) = self.load_font(fallback_ref) {
                if self.font_supports_text(&font, text) {
                    return Ok(font);
                }
            }
        }

        Err(LayoutError::FontNotFound(font_ref.clone()))
    }

    fn font_supports_text(&self, font: &ParsedFont, text: &str) -> bool {
        // Quick check using cmap table
        text.chars().all(|c| font.has_glyph(c as u32))
    }

    fn fc_match_to_font_ref(&self, fc_match: &FcFontMatch) -> Result<FontRef, LayoutError> {
        // Convert rust-fontconfig match to our FontRef
        let font_path = self.fc_cache.get_font_path(&fc_match.id)
            .ok_or_else(|| LayoutError::FontNotFound(FontRef { family: "unknown".to_string(), weight: 400, style: FontStyle::Normal }))?;
        
        // Extract family name from the font file
        let family = self.extract_family_name(&font_path.path)?;
        
        Ok(FontRef {
            family,
            weight: fc_match.weight.unwrap_or(400),
            style: fc_slant_to_style(fc_match.slant.unwrap_or(0)),
        })
    }

    fn extract_family_name(&self, font_path: &str) -> Result<String, LayoutError> {
        // This would parse the font file to get the actual family name
        // For now, use a simplified approach
        std::path::Path::new(font_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| LayoutError::InvalidText("Cannot extract font family".to_string()))
    }
}

impl FontProvider for FontManager {
    fn load_font(&mut self, font_ref: &FontRef) -> Result<Arc<ParsedFont>, LayoutError> {
        // Try to find font ID from fontconfig
        let mut trace = Vec::new();
        let pattern = FcPattern {
            name: Some(font_ref.family.clone()),
            weight: Some(weight_to_fc_weight(font_ref.weight)),
            slant: Some(style_to_fc_slant(font_ref.style)),
            ..Default::default()
        };

        let fc_match = self.fc_cache.query(&pattern, &mut trace)
            .ok_or_else(|| LayoutError::FontNotFound(font_ref.clone()))?;

        // Check if already loaded
        if let Some(font) = self.parsed_fonts.get(&fc_match.id) {
            return Ok(font.clone());
        }

        // Load and parse the font file
        let font_path = self.fc_cache.get_font_path(&fc_match.id)
            .ok_or_else(|| LayoutError::FontNotFound(font_ref.clone()))?;

        let parsed_font = Arc::new(ParsedFont::from_file(&font_path.path, font_path.font_index)?);
        self.parsed_fonts.insert(fc_match.id, parsed_font.clone());

        Ok(parsed_font)
    }

    fn get_fallback_chain(&mut self, font_ref: &FontRef, script: Script) -> Vec<FontRef> {
        // This is now handled by build_fallback_chain, but we keep the interface
        // for compatibility. Build a minimal fallback chain without text analysis.
        let mut trace = Vec::new();
        let pattern = FcPattern {
            name: Some(font_ref.family.clone()),
            ..Default::default()
        };

        self.fc_cache.query_for_text(&pattern, "", &mut trace)
            .into_iter()
            .filter_map(|fc_match| self.fc_match_to_font_ref(&fc_match).ok())
            .take(5) // Limit fallback chain length
            .collect()
    }
}

// Enhanced shaping that handles font fallback within runs
fn shape_visual_runs_with_fallback(
    visual_runs: &[VisualRun],
    font_manager: &mut FontManager,
) -> Result<Vec<ShapedGlyph>, LayoutError> {
    let mut all_shaped_glyphs = Vec::new();

    for run in visual_runs {
        let direction = if run.bidi_level.is_rtl() { Direction::Rtl } else { Direction::Ltr };
        
        // Shape with fallback - this is the key enhancement
        let shaped_glyphs = shape_run_with_fallback(run, font_manager, direction)?;
        
        if direction == Direction::Rtl {
            // Note: Only reverse the glyphs, not the entire vec structure
            let mut reversed_glyphs = shaped_glyphs;
            reversed_glyphs.reverse();
            all_shaped_glyphs.extend(reversed_glyphs);
        } else {
            all_shaped_glyphs.extend(shaped_glyphs);
        }
    }
    
    Ok(all_shaped_glyphs)
}

fn shape_run_with_fallback(
    run: &VisualRun,
    font_manager: &mut FontManager,
    direction: Direction,
) -> Result<Vec<ShapedGlyph>, LayoutError> {
    let mut result = Vec::new();
    let mut char_indices = run.text_slice.char_indices().peekable();
    
    while let Some((byte_offset, ch)) = char_indices.next() {
        // Collect a sequence of characters that can be shaped together
        let mut segment_end = byte_offset + ch.len_utf8();
        let mut segment_chars = vec![ch];
        
        // Look ahead to group characters that likely use the same font
        while let Some(&(next_byte_offset, next_ch)) = char_indices.peek() {
            if should_group_chars(ch, next_ch, run.script) {
                segment_chars.push(next_ch);
                char_indices.next();
                segment_end = next_byte_offset + next_ch.len_utf8();
            } else {
                break;
            }
        }
        
        let segment_text = &run.text_slice[byte_offset..segment_end];
        
        // Find appropriate font for this segment
        let font = font_manager.get_font_for_text(&run.style.font_ref, segment_text, run.script)?;
        
        // Shape the segment
        let mut shaped_segment = font.shape_text(segment_text, run.script, run.language, direction)?;
        
        // Adjust byte indices to be relative to the full run
        for glyph in &mut shaped_segment {
            glyph.logical_byte_start += run.logical_start_byte + byte_offset;
            glyph.cluster += (run.logical_start_byte + byte_offset) as u32;
        }
        
        result.extend(shaped_segment);
    }
    
    Ok(result)
}

fn should_group_chars(ch1: char, ch2: char, script: Script) -> bool {
    // Group characters that are likely to use the same font
    // This is a simplified heuristic
    let script1 = unicode_script::get_script(ch1);
    let script2 = unicode_script::get_script(ch2);
    
    script1 == script2 || 
    (ch1.is_ascii() && ch2.is_ascii()) ||
    (ch1.is_whitespace() || ch2.is_whitespace())
}

// Helper conversion functions
fn weight_to_fc_weight(weight: u16) -> i32 {
    weight as i32
}

fn style_to_fc_slant(style: FontStyle) -> i32 {
    match style {
        FontStyle::Normal => 0,
        FontStyle::Italic => 100,
        FontStyle::Oblique => 110,
    }
}

fn fc_slant_to_style(slant: i32) -> FontStyle {
    match slant {
        0 => FontStyle::Normal,
        100 => FontStyle::Italic,
        _ => FontStyle::Oblique,
    }
}

// Updated main layout function
pub fn layout_paragraph_with_fallback(
    styled_runs: Vec<StyledRun>,
    constraints: LayoutConstraints,
    font_manager: &mut FontManager,
) -> Result<Arc<ParagraphLayout>, LayoutError> {
    let full_logical_text = concatenate_runs_text(&styled_runs);
    
    // Pre-build fallback chains for all unique fonts in the runs
    let mut unique_fonts: std::collections::HashSet<FontRef> = styled_runs
        .iter()
        .map(|run| &run.style.font_ref)
        .cloned()
        .collect();
    
    for font_ref in &unique_fonts {
        font_manager.build_fallback_chain(font_ref, &full_logical_text)?;
    }
    
    let (visual_runs, base_direction) = perform_bidi_analysis(&styled_runs, &full_logical_text)?;
    
    // Use the enhanced shaping with fallback
    let shaped_glyphs = shape_visual_runs_with_fallback(&visual_runs, font_manager)?;
    
    let shaped_glyphs = insert_hyphenation_points(&full_logical_text, shaped_glyphs, font_manager)?;
    
    let layout = position_glyphs(shaped_glyphs, constraints, &full_logical_text, base_direction, font_manager)?;
    
    Ok(Arc::new(layout))
}

// Enhanced ParsedFont with glyph coverage checking
impl ParsedFont {
    pub fn has_glyph(&self, codepoint: u32) -> bool {
        // Check if font has a glyph for this codepoint
        // This would use the cmap table from the font
        unimplemented!("Glyph coverage check using cmap table")
    }
    
    pub fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> (u16, f32) {
        // Get hyphen glyph ID and its advance width
        let hyphen_glyph_id = self.get_glyph_id('-' as u32).unwrap_or(0);
        let advance = self.get_glyph_advance(hyphen_glyph_id, font_size);
        (hyphen_glyph_id, advance)
    }
    
    pub fn get_glyph_id(&self, codepoint: u32) -> Option<u16> {
        // Lookup glyph ID from codepoint using cmap table
        unimplemented!("Codepoint to glyph ID mapping")
    }
    
    pub fn from_file(path: &str, font_index: usize) -> Result<Self, LayoutError> {
        // Load and parse font file using allsorts or similar
        unimplemented!("Font file parsing")
    }
}