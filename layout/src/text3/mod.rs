//! Professional Text Layout Engine
//!
//! This is a complete architecture for a robust text layout engine that handles:
//! - Unicode bidirectional text (Bidi)
//! - Complex script shaping
//! - Proper hyphenation
//! - Inline elements with mixed styling
//! - Color and SVG emoji support
//! - Thread-safe caching
//! - Professional typography features

use std::sync::{Arc, OnceLock};
use std::collections::HashMap;

// Core types for layout constraints and measurements
#[derive(Debug, Clone)]
pub struct LayoutConstraints {
    pub available_width: f32,
    pub exclusion_areas: Vec<ExclusionRect>,
}

#[derive(Debug, Clone)]
pub struct ExclusionRect {
    pub rect: Rect,
    pub side: ExclusionSide,
}

#[derive(Debug, Clone)]
pub enum ExclusionSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

// Font and styling types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontRef {
    pub family: String,
    pub weight: u16,
    pub style: FontStyle,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextDecoration {
    pub underline: bool,
    pub strikethrough: bool,
    pub overline: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
    Start,  // Logical start (left for LTR, right for RTL)
    End,    // Logical end (right for LTR, left for RTL)
}

// Bidi and script detection
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Ltr,
    Rtl,
}

#[derive(Debug, Clone)]
pub struct BidiLevel(u8);

impl BidiLevel {
    pub fn new(level: u8) -> Self { Self(level) }
    pub fn is_rtl(&self) -> bool { self.0 % 2 == 1 }
    pub fn level(&self) -> u8 { self.0 }
}

// Stage 1: Collection - Styled runs from DOM traversal
#[derive(Debug, Clone)]
pub struct StyledRun {
    pub text: String,
    pub style: StyleProperties,
    /// Byte index in the original logical paragraph text
    pub logical_start_byte: usize,
}

// Stage 2: Bidi Analysis - Visual runs in display order
#[derive(Debug)]
pub struct VisualRun<'a> {
    pub text_slice: &'a str,
    pub style: &'a StyleProperties,
    pub logical_start_byte: usize,
    pub bidi_level: BidiLevel,
    pub script: Script,
    pub language: Language,
}

#[derive(Debug, Clone, Copy)]
pub struct Script(pub u32);

#[derive(Debug, Clone, Copy)]
pub struct Language(pub u32);

// Stage 3: Shaping - Individual glyphs with positioning info
#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    // Identity & Font
    pub glyph_id: u16,
     // Cloned for ownership during line breaking
    pub style: StyleProperties,

    // Metrics & Positioning from Shaper
    pub advance: f32,
    pub x_offset: f32,
    pub y_offset: f32,

    // Mapping to Source Text
    pub logical_byte_start: usize,
    pub logical_byte_len: u8,
    pub cluster: u32, // Absolute byte index of the start of the cluster

    // Line Breaking Information
    pub source: GlyphSource,
    pub is_whitespace: bool,
    /// Indicates a valid line break can occur *after* this glyph.
    pub break_opportunity_after: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphSource {
    /// Glyph generated from a character in the source text.
    Char,
    /// Glyph inserted dynamically by the layout engine (e.g., a hyphen).
    Hyphen,
}

// Stage 4: Positioning - Final positioned glyphs ready for rendering
#[derive(Debug, Clone)]
pub struct PositionedGlyph {
    // Rendering information
    pub glyph_id: u16,
    pub style: StyleProperties,
    pub x: f32,
    pub y: f32,
    pub bounds: Rect,
    
    // Layout information  
    pub advance: f32,
    pub line_index: usize,
    
    // Source mapping for editing/selection
    pub logical_char_byte_index: usize,
    pub logical_char_byte_count: u8,
    pub visual_index: usize,
    
    // Bidi information
    pub bidi_level: BidiLevel,
}

// Final layout result
#[derive(Debug, Clone)]
pub struct ParagraphLayout {
    pub glyphs: Vec<PositionedGlyph>,
    pub lines: Vec<LineLayout>,
    pub content_size: Size,
    pub source_text: String,
    pub base_direction: Direction,
}

#[derive(Debug, Clone)]
pub struct LineLayout {
    pub bounds: Rect,
    pub baseline_y: f32,
    pub glyph_start: usize,
    pub glyph_count: usize,
    pub logical_start_byte: usize,
    pub logical_end_byte: usize,
}

// Font metrics and glyph representation
#[derive(Debug, Clone)]
pub struct FontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub units_per_em: u16,
    pub x_height: f32,
    pub cap_height: f32,
}

#[derive(Debug, Clone)]
pub enum GlyphRepresentation {
    Outline,
    Colr { layers: Vec<ColorLayer> },
    Svg { document: Arc<str>, viewbox: Rect },
    Bitmap { data: Arc<[u8]>, size: Size },
}

#[derive(Debug, Clone)]
pub struct ColorLayer {
    pub glyph_id: u16,
    pub color: Color,
}

// Error handling
#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("Bidi analysis failed: {0}")]
    BidiError(String),
    #[error("Shaping failed: {0}")]
    ShapingError(String),
    #[error("Font not found: {0:?}")]
    FontNotFound(FontRef),
    #[error("Invalid text input: {0}")]
    InvalidText(String),
    #[error("Hyphenation failed: {0}")]
    HyphenationError(String),
}

#[derive(Debug)]
struct LineState {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    baseline_y: f32,
    glyph_start: usize,
    logical_start_byte: usize,
    logical_end_byte: usize,
}

impl LineState {
    fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            baseline_y: 0.0,
            glyph_start: 0,
            logical_start_byte: 0,
            logical_end_byte: 0,
        }
    }
}

// Hit testing and cursor positioning

impl ParagraphLayout {
    /// Convert visual position to logical cursor position
    pub fn hit_test(&self, point: Point) -> Option<usize> {
        // Find the glyph at the given point
        for glyph in &self.glyphs {
            if point_in_rect(point, glyph.bounds) {
                // Check if point is in left or right half of glyph
                let glyph_center = glyph.bounds.x + glyph.bounds.width / 2.0;
                
                return if point.x < glyph_center {
                    // Left half - cursor before this glyph
                    Some(glyph.logical_char_byte_index)
                } else {
                    // Right half - cursor after this glyph
                    Some(glyph.logical_char_byte_index + glyph.logical_char_byte_count as usize)
                };
            }
        }
        
        // If no glyph hit, return end of text
        Some(self.source_text.len())
    }
    
    /// Convert logical cursor position to visual coordinates
    pub fn cursor_position(&self, logical_index: usize) -> Option<Point> {
        // Find glyph at or before this logical position
        for glyph in &self.glyphs {
            let glyph_start = glyph.logical_char_byte_index;
            let glyph_end = glyph_start + glyph.logical_char_byte_count as usize;
            
            if logical_index >= glyph_start && logical_index < glyph_end {
                // Cursor is within this glyph
                let offset_ratio = (logical_index - glyph_start) as f32 / 
                                  glyph.logical_char_byte_count as f32;
                
                return Some(Point {
                    x: glyph.x + glyph.advance * offset_ratio,
                    y: glyph.y,
                });
            } else if logical_index == glyph_end {
                // Cursor is right after this glyph
                return Some(Point {
                    x: glyph.x + glyph.advance,
                    y: glyph.y,
                });
            }
        }
        
        None
    }
    
    /// Get text selection bounds
    pub fn selection_bounds(&self, start: usize, end: usize) -> Vec<Rect> {
        let mut bounds = Vec::new();
        
        // Group consecutive glyphs on same line into selection rectangles
        let mut current_rect: Option<Rect> = None;
        
        for glyph in &self.glyphs {
            let glyph_start = glyph.logical_char_byte_index;
            let glyph_end = glyph_start + glyph.logical_char_byte_count as usize;
            
            // Check if this glyph is in selection
            if glyph_start < end && glyph_end > start {
                match &mut current_rect {
                    Some(rect) if rect.y == glyph.bounds.y => {
                        // Extend current rectangle
                        let right = (glyph.bounds.x + glyph.bounds.width).max(rect.x + rect.width);
                        rect.width = right - rect.x;
                    }
                    _ => {
                        // Start new rectangle
                        if let Some(rect) = current_rect.take() {
                            bounds.push(rect);
                        }
                        current_rect = Some(glyph.bounds);
                    }
                }
            }
        }
        
        if let Some(rect) = current_rect {
            bounds.push(rect);
        }
        
        bounds
    }
}

// Font loading and management (stub)
pub trait FontProvider {
    fn load_font(&self, font_ref: &FontRef) -> Result<Arc<ParsedFont>, LayoutError>;
    fn get_fallback_chain(&self, font_ref: &FontRef, script: Script) -> Vec<FontRef>;
}

pub struct ParsedFont {
    pub metrics: FontMetrics,
    pub glyph_representations: HashMap<u16, GlyphRepresentation>,
    // Font tables and other data...
}

impl ParsedFont {
    pub fn shape_text(
        &self,
        text: &str,
        script: Script,
        language: Language,
        direction: Direction,
    ) -> Result<Vec<ShapedGlyph>, LayoutError> {
        // Use rustybuzz or similar for actual shaping
        unimplemented!("Text shaping implementation")
    }
    
    pub fn get_glyph_advance(&self, glyph_id: u16, font_size: f32) -> f32 {
        // Return advance width for glyph at given size
        unimplemented!("Glyph metrics lookup")
    }
}

// Advanced typography features
impl ParagraphLayout {
    pub fn apply_kerning(&mut self) {
        // Adjust glyph positions based on kerning pairs
        unimplemented!("Kerning adjustment")
    }
    
    pub fn apply_text_decoration(&mut self) {
        // Add underlines, strikethroughs, etc.
        unimplemented!("Text decoration rendering")
    }
    
    pub fn justify_text(&mut self, target_width: f32) {
        // Adjust spacing for justified text
        unimplemented!("Text justification")
    }
}

// --- IMPLEMENTATION --- // 


// --- IMPLEMENTATION START ---

use unicode_bidi::{BidiInfo, Level};
use hyphenation::{Language, Load, Standard, Hyphenator};
use std::collections::HashSet;

// We assume the ParsedFont and other structs from your azul codebase are available.
// For demonstration, let's use the ones you've provided.
// If your original ParsedFont from azul is different, you may need to adjust the shaping calls.
use crate::ParsedFont; // Assuming this is the ParsedFont from your original code.
use crate::FontProvider; // Assuming you have this trait defined.

// Assume a global or passed-in hyphenator cache for performance.
// For this example, we'll initialize it inside the function.
fn get_hyphenator() -> Result<Standard, LayoutError> {
    Standard::from_embedded(Language::EnglishUS)
        .map_err(|e| LayoutError::HyphenationError(e.to_string()))
}

fn concatenate_runs_text(runs: &[StyledRun]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

fn perform_bidi_analysis<'a>(
    styled_runs: &'a [StyledRun],
    full_text: &str,
) -> Result<(Vec<VisualRun<'a>>, Direction), LayoutError> {
    if full_text.is_empty() {
        return Ok((Vec::new(), Direction::Ltr));
    }

    let bidi_info = BidiInfo::new(full_text, None);
    let para = &bidi_info.paragraphs[0];
    let base_direction = if para.level.is_rtl() { Direction::Rtl } else { Direction::Ltr };

    // Create a map from each byte index to its original styled run.
    let mut byte_to_run_index: Vec<usize> = vec![0; full_text.len()];
    for (run_idx, run) in styled_runs.iter().enumerate() {
        let start = run.logical_start_byte;
        let end = start + run.text.len();
        for i in start..end {
            byte_to_run_index[i] = run_idx;
        }
    }

    let mut final_visual_runs = Vec::new();
    let (levels, visual_run_ranges) = bidi_info.visual_runs(para, ..);

    for range in visual_run_ranges {
        let bidi_level = levels[range.start];
        let mut sub_run_start = range.start;

        // Iterate through the bytes of the visual run to detect style changes.
        for i in (range.start + 1)..range.end {
            if byte_to_run_index[i] != byte_to_run_index[sub_run_start] {
                // Style boundary found. Finalize the previous sub-run.
                let original_run_idx = byte_to_run_index[sub_run_start];
                final_visual_runs.push(VisualRun {
                    text_slice: &full_text[sub_run_start..i],
                    style: &styled_runs[original_run_idx].style,
                    logical_start_byte: sub_run_start,
                    bidi_level: BidiLevel::new(bidi_level.number()),
                    // In a real engine, these would be detected per-run.
                    script: Script(0),
                    language: Language(0),
                });
                // Start a new sub-run.
                sub_run_start = i;
            }
        }

        // Add the last sub-run (or the only one if no style change occurred).
        let original_run_idx = byte_to_run_index[sub_run_start];
        final_visual_runs.push(VisualRun {
            text_slice: &full_text[sub_run_start..range.end],
            style: &styled_runs[original_run_idx].style,
            logical_start_byte: sub_run_start,
            bidi_level: BidiLevel::new(bidi_level.number()),
            script: Script(0),
            language: Language(0),
        });
    }

    Ok((final_visual_runs, base_direction))
}

fn shape_visual_runs(
    visual_runs: &[VisualRun],
    font_provider: &impl FontProvider,
) -> Result<Vec<ShapedGlyph>, LayoutError> {
    let mut all_shaped_glyphs = Vec::new();

    for run in visual_runs {
        let font = font_provider.load_font(&run.style.font_ref)?;
        
        let direction = if run.bidi_level.is_rtl() { Direction::Rtl } else { Direction::Ltr };
        let mut shaped_output = font.shape_text(run.text_slice, run.script, run.language, direction)?;
        
        if direction == Direction::Rtl {
            shaped_output.reverse();
        }

        for shaped_in_run in shaped_output {
            let source_char = run.text_slice[shaped_in_run.logical_byte_start..]
                .chars().next().unwrap_or('\0');
            
            let is_whitespace = source_char.is_whitespace();
            // A soft hyphen is a break opportunity but not whitespace.
            let is_soft_hyphen = source_char == '\u{00AD}';
            
            all_shaped_glyphs.push(ShapedGlyph {
                glyph_id: shaped_in_run.glyph_id,
                style: run.style.clone(),
                advance: shaped_in_run.advance,
                x_offset: shaped_in_run.x_offset,
                y_offset: shaped_in_run.y_offset,
                // Make cluster and byte start absolute to the entire paragraph.
                logical_byte_start: shaped_in_run.logical_byte_start + run.logical_start_byte,
                logical_byte_len: shaped_in_run.logical_byte_len,
                cluster: shaped_in_run.cluster + run.logical_start_byte as u32,
                source: GlyphSource::Char,
                is_whitespace,
                break_opportunity_after: is_whitespace || is_soft_hyphen,
            });
        }
    }
    
    Ok(all_shaped_glyphs)
}

fn position_glyphs(
    mut shaped_glyphs: Vec<ShapedGlyph>,
    constraints: LayoutConstraints,
    source_text: &str,
    base_direction: Direction,
    font_provider: &impl FontProvider,
) -> Result<ParagraphLayout, LayoutError> {
    let hyphenator = get_hyphenator()?;
    let mut positioned_glyphs = Vec::new();
    let mut lines = Vec::new();
    let mut line_y = 0.0;
    let mut glyph_cursor = 0;

    // The main line breaking loop.
    while glyph_cursor < shaped_glyphs.len() {
        let (line_end_idx, needs_hyphen) = find_line_break(
            &shaped_glyphs,
            glyph_cursor,
            line_y,
            &constraints,
            &hyphenator,
            source_text,
        );

        let mut glyphs_for_this_line = shaped_glyphs[glyph_cursor..line_end_idx].to_vec();

        // Handle hyphen insertion.
        let hyphen_glyph = if needs_hyphen {
            // Get the style from the last character of the word being broken.
            let style = glyphs_for_this_line.last().unwrap().style.clone();
            let font = font_provider.load_font(&style.font_ref)?;
            let (glyph_id, advance) = font.get_hyphen_glyph_and_advance(style.font_size_px);
            
            Some(ShapedGlyph {
                glyph_id,
                style,
                advance,
                x_offset: 0.0,
                y_offset: 0.0,
                logical_byte_start: 0, // No direct mapping to source
                logical_byte_len: 0,
                cluster: 0,
                source: GlyphSource::Hyphen,
                is_whitespace: false,
                break_opportunity_after: false,
            })
        } else {
            None
        };
        
        if let Some(hyphen) = hyphen_glyph.as_ref() {
            glyphs_for_this_line.push(hyphen.clone());
        }

        // Finalize the line's geometry, applying alignment and justification.
        let (mut finalized_line_glyphs, line_layout) = finalize_line(
            &glyphs_for_this_line,
            glyph_cursor,
            line_y,
            constraints.available_width, // TODO: Use per-line width with floats
            TextAlign::Justify, // TODO: Get from style
            base_direction,
            needs_hyphen,
        );
        
        positioned_glyphs.append(&mut finalized_line_glyphs);
        lines.push(line_layout.clone());

        line_y += line_layout.bounds.height;
        glyph_cursor = line_end_idx;
    }

    let content_size = calculate_content_size(&lines);

    Ok(ParagraphLayout {
        glyphs: positioned_glyphs,
        lines,
        content_size,
        source_text: source_text.to_string(),
        base_direction,
    })
}

/// Finds the index in `shaped_glyphs` where the current line should break.
/// Returns `(break_index, needs_hyphen)`.
fn find_line_break(
    glyphs: &[ShapedGlyph],
    start_idx: usize,
    line_y: f32,
    constraints: &LayoutConstraints,
    hyphenator: &Standard,
    source_text: &str,
) -> (usize, bool) {
    let available_width = constraints.available_width; // TODO: handle floats
    let mut current_width = 0.0;
    let mut last_opportunity = start_idx;

    for i in start_idx..glyphs.len() {
        let glyph = &glyphs[i];
        
        // Don't start a line with whitespace.
        if i == start_idx && glyph.is_whitespace {
            continue;
        }

        if current_width + glyph.advance > available_width {
            // We have overflowed. Break at the last known opportunity.
            if last_opportunity > start_idx {
                return (last_opportunity, false);
            }
            
            // No break opportunity found (long word). We must hyphenate or force break.
            let (word_start, word_end) = find_word_boundaries(glyphs, i);
            let word_text = &source_text[glyphs[word_start].logical_byte_start..glyphs[word_end-1].logical_byte_start + glyphs[word_end-1].logical_byte_len as usize];
            
            let breaks = hyphenator.opportunities(word_text);
            
            let mut best_hyphen_break = start_idx;
            let mut hyphen_width = 0.0;
            
            // Find the last possible hyphenation point that fits.
            for &break_byte_offset in breaks.iter().rev() {
                let mut width_at_hyphen = 0.0;
                let mut glyph_idx_at_hyphen = word_start;
                
                for j in word_start..word_end {
                    width_at_hyphen += glyphs[j].advance;
                    // Check if the glyph's logical end corresponds to the hyphen break point.
                    let logical_break_pos = glyphs[word_start].logical_byte_start + break_byte_offset;
                    if glyphs[j].logical_byte_start + glyphs[j].logical_byte_len as usize == logical_break_pos {
                        glyph_idx_at_hyphen = j + 1;
                        break;
                    }
                }

                if width_at_hyphen < available_width {
                    best_hyphen_break = glyph_idx_at_hyphen;
                    break;
                }
            }

            if best_hyphen_break > start_idx {
                return (best_hyphen_break, true);
            }

            // Cannot hyphenate, force break before the overflowing glyph.
            return (i.max(start_idx + 1), false);
        }

        current_width += glyph.advance;

        if glyph.break_opportunity_after {
            last_opportunity = i + 1;
        }
    }

    // Reached the end of the text.
    (glyphs.len(), false)
}

// --- HELPER FUNCTIONS FOR POSITIONING ---

fn find_word_boundaries(glyphs: &[ShapedGlyph], current_idx: usize) -> (usize, usize) {
    let mut start = current_idx;
    while start > 0 && !glyphs[start - 1].is_whitespace {
        start -= 1;
    }
    let mut end = current_idx;
    while end < glyphs.len() && !glyphs[end].is_whitespace {
        end += 1;
    }
    (start, end)
}

fn get_available_width_for_line(line_y: f32, constraints: &LayoutConstraints) -> f32 {
    // This is a simplified check. A full implementation would handle multiple floats.
    let mut available = constraints.available_width;
    for exclusion in &constraints.exclusion_areas {
        if line_y >= exclusion.rect.y && line_y < exclusion.rect.y + exclusion.rect.height {
            available -= exclusion.rect.width;
        }
    }
    available.max(0.0)
}

#[derive(Debug)]
struct LineMetrics {
    ascent: f32,
    descent: f32,
    line_gap: f32,
    total_height: f32,
}

// These helpers are placeholders; a real implementation would be more detailed.
#[derive(Debug)]
struct LineMetrics { ascent: f32, descent: f32, line_gap: f32, total_height: f32 }

fn calculate_line_metrics(glyphs: &[ShapedGlyph]) -> LineMetrics {
    let font_size = glyphs.first().map_or(16.0, |g| g.style.font_size_px);
    let line_height = font_size * 1.4; // Common default
    let ascent = font_size;
    LineMetrics { ascent, descent: line_height - ascent, line_gap: 0.0, total_height: line_height }
}

fn finalize_line(
    glyphs_on_line: &[ShapedGlyph],
    line_start_visual_index: usize,
    line_y: f32,
    available_width: f32,
    align: TextAlign,
    base_direction: Direction,
    is_hyphenated: bool,
) -> (Vec<PositionedGlyph>, LineLayout) {
    if glyphs_on_line.is_empty() {
        // Handle empty lines (e.g., from double newlines).
        let height = 16.0 * 1.2; // A default line height.
        return (vec![], LineLayout {
            bounds: Rect { x: 0.0, y: line_y, width: 0.0, height },
            baseline_y: line_y + 16.0,
            glyph_start: 0, glyph_count: 0, logical_start_byte: 0, logical_end_byte: 0,
        });
    }

    let metrics = calculate_line_metrics(glyphs_on_line);
    let line_width: f32 = glyphs_on_line.iter().map(|g| g.advance).sum();
    
    let mut start_x = 0.0;
    let mut space_expansion = 0.0;
    let logical_align = resolve_logical_align(align, base_direction);

    // Don't justify the last line of a paragraph or a hyphenated line.
    let is_last_line = false; // This requires lookahead, omitted for now.
    if logical_align == TextAlign::Justify && !is_last_line && !is_hyphenated {
        let space_count = glyphs_on_line.iter().filter(|g| g.is_whitespace).count();
        if space_count > 0 {
            space_expansion = (available_width - line_width) / space_count as f32;
        }
    } else {
        match logical_align {
            TextAlign::Center => start_x = (available_width - line_width) / 2.0,
            TextAlign::Right => start_x = available_width - line_width,
            _ => {} // Left is default.
        }
    }
    
    let mut current_x = start_x;
    let baseline_y = line_y + metrics.ascent;
    let mut positioned_glyphs = Vec::new();
    
    for (i, glyph) in glyphs_on_line.iter().enumerate() {
        positioned_glyphs.push(PositionedGlyph {
            glyph_id: glyph.glyph_id,
            style: glyph.style.clone(),
            x: current_x + glyph.x_offset,
            y: baseline_y - glyph.y_offset,
            bounds: Rect { x: current_x, y: line_y, width: glyph.advance, height: metrics.total_height },
            advance: glyph.advance,
            line_index: 0, // This can be set in a final pass.
            logical_char_byte_index: glyph.logical_byte_start,
            logical_char_byte_count: glyph.logical_byte_len,
            visual_index: line_start_visual_index + i,
            bidi_level: BidiLevel::new(0), // TODO: Propagate from VisualRun.
        });
        current_x += glyph.advance;
        if glyph.is_whitespace {
            current_x += space_expansion;
        }
    }

    let (logical_start, logical_end) = glyphs_on_line.iter()
        .filter(|g| g.source == GlyphSource::Char)
        .fold((usize::MAX, 0), |(min, max), g| {
            (min.min(g.logical_byte_start), max.max(g.logical_byte_start + g.logical_byte_len as usize))
        });
    
    let line_layout = LineLayout {
        bounds: Rect { x: start_x, y: line_y, width: current_x - start_x, height: metrics.total_height },
        baseline_y,
        glyph_start: line_start_visual_index,
        glyph_count: glyphs_on_line.len(),
        logical_start_byte: if logical_start == usize::MAX { 0 } else { logical_start },
        logical_end_byte: logical_end,
    };

    (positioned_glyphs, line_layout)
}

fn resolve_logical_align(align: TextAlign, direction: Direction) -> TextAlign {
    match (align, direction) {
        (TextAlign::Start, Direction::Ltr) => TextAlign::Left,
        (TextAlign::Start, Direction::Rtl) => TextAlign::Right,
        (TextAlign::End, Direction::Ltr) => TextAlign::Right,
        (TextAlign::End, Direction::Rtl) => TextAlign::Left,
        (other, _) => other,
    }
}

fn calculate_content_size(lines: &[LineLayout]) -> Size {
    let max_width = lines.iter()
        .map(|line| line.bounds.x + line.bounds.width)
        .fold(0.0f32, f32::max);
    
    let total_height = lines.last()
        .map(|line| line.bounds.y + line.bounds.height)
        .unwrap_or(0.0);
    
    Size { width: max_width, height: total_height }
}

fn point_in_rect(point: Point, rect: Rect) -> bool {
    point.x >= rect.x && 
    point.x <= rect.x + rect.width &&
    point.y >= rect.y &&
    point.y <= rect.y + rect.height
}

// --- MAIN ENTRY POINT AND CACHING ---

// Your main entry point function needs to be updated to orchestrate these stages.
// It also needs access to a FontProvider.

pub fn layout_paragraph(
    styled_runs: Vec<StyledRun>,
    constraints: LayoutConstraints,
    font_provider: &impl FontProvider,
) -> Result<Arc<ParagraphLayout>, LayoutError> {
    
    // Check cache first
    
    // Note: The cache key would need to be more sophisticated to 
    // include the font provider state if fonts could change. For 
    // simplicity, we omit it here.

    /*
    let cache_key = (styled_runs.clone(), constraints.clone());
    {
        let cache = get_layout_cache().read();
        if let Some(cached) = cache.get(&cache_key) {
            return Ok(cached.clone());
        }
    }
    */
    
    // Stage 1: Concatenate text
    let full_logical_text = concatenate_runs_text(&styled_runs);
    
    // Stage 2: Bidi analysis
    let (visual_runs, base_direction) = perform_bidi_analysis(&styled_runs, &full_logical_text)?;
    
    // Stage 3: Script itemization and shaping
    let shaped_glyphs = shape_visual_runs(&visual_runs, font_provider)?;
    
    // Stage 3.5: Insert hyphenation opportunities
    let shaped_glyphs = insert_hyphenation_points(&full_logical_text, shaped_glyphs, font_provider)?;
    
    // Stage 4: Line breaking and positioning
    let layout = position_glyphs(shaped_glyphs, constraints, &full_logical_text, base_direction)?;
    
    let layout = Arc::new(layout);
    
    // Cache the result
    /*
    {
        let mut cache = get_layout_cache().write();
        cache.insert(cache_key, layout.clone());
    }
    */
    
    Ok(layout)
}
