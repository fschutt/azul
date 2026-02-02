# Text Underline Skip-Ink Analysis

## Executive Summary

This document analyzes the implementation of proper `text-decoration-skip-ink` support for underlines in Azul. The CSS property `text-decoration-skip-ink: auto` (default in modern browsers) prevents underlines from intersecting with glyph descenders (e.g., "y", "g", "p", "q", "j").

## Current Implementation Status

### What's Already Working

1. **Basic Underline Rendering**: The display list already supports `DisplayListItem::Underline` with bounds, color, and thickness ([display_list.rs#L484-L488](../layout/src/solver3/display_list.rs#L484-L488))

2. **Underline Generation**: The code in [display_list.rs#L2771-L2800](../layout/src/solver3/display_list.rs#L2771-L2800) correctly generates underlines:
   ```rust
   if needs_underline {
       // Underline is typically 10-15% below baseline
       let underline_y = baseline_y + (font_size * 0.12);
       let underline_bounds = LogicalRect::new(
           LogicalPosition::new(decoration_start_x, underline_y),
           LogicalSize::new(decoration_width, thickness),
       );
       builder.push_underline(underline_bounds, glyph_run.color, thickness);
   }
   ```

3. **Glyph Bounding Boxes**: `OwnedGlyph` contains `bounding_box: OwnedGlyphBoundingBox` with `min_x`, `min_y`, `max_x`, `max_y` in font units ([font.rs#L1097-L1101](../layout/src/font.rs#L1097-L1101))

4. **Font Metrics**: `FontMetrics` includes descender information ([font.rs#L245-L246](../layout/src/font.rs#L245-L246))

### What's Missing

1. **No Skip-Ink CSS Property**: The `text-decoration-skip-ink` CSS property is not parsed or supported

2. **No Descender Detection**: Per-glyph descender analysis to detect where underlines should "skip"

3. **No Segmented Underlines**: Currently only one continuous underline per glyph run; need to split into multiple segments

4. **No UnderlineSegment Display List Item**: Need to either:
   - Generate multiple `Underline` items per glyph run, OR
   - Create a new `UnderlineWithGaps` item that stores gap ranges

## Proposed Implementation

### Phase 1: Glyph Descender Detection (Font Layer)

Add a helper function to detect if a glyph has a descender that intersects with the underline position:

```rust
// In layout/src/font.rs or a new layout/src/text3/descender.rs

/// Information about where a glyph's outline intersects the underline zone
pub struct GlyphUnderlineIntersection {
    /// Start X position of the intersection (in glyph-local coordinates)
    pub start_x: f32,
    /// End X position of the intersection
    pub end_x: f32,
}

impl OwnedGlyph {
    /// Check if this glyph has portions that extend below the baseline
    /// into the underline zone.
    /// 
    /// # Arguments
    /// * `underline_y` - Y position of underline (distance below baseline, positive = down)
    /// * `underline_thickness` - Thickness of the underline
    /// * `units_per_em` - Font units per em
    /// 
    /// # Returns
    /// * `None` if the glyph doesn't intersect the underline zone
    /// * `Some(intersection)` with the x-range where intersection occurs
    pub fn get_underline_intersection(
        &self,
        underline_y: i16,
        underline_thickness: i16,
    ) -> Option<GlyphUnderlineIntersection> {
        // The underline zone is from underline_y to underline_y + thickness
        // In font coordinates, Y increases upward, so descenders have negative Y
        let underline_top = -underline_y; // Convert to font coordinates
        let underline_bottom = underline_top - underline_thickness as i16;
        
        // Quick check: if glyph's min_y is above underline zone, no intersection
        if self.bounding_box.min_y >= underline_top {
            return None;
        }
        
        // The glyph descends into the underline zone
        // Return the full horizontal extent of the glyph as the intersection zone
        // (This is a conservative approximation - could be refined with actual outline analysis)
        Some(GlyphUnderlineIntersection {
            start_x: self.bounding_box.min_x as f32,
            end_x: self.bounding_box.max_x as f32,
        })
    }
    
    /// Returns true if this glyph is a descender character (extends below baseline)
    pub fn has_descender(&self) -> bool {
        self.bounding_box.min_y < 0
    }
}
```

### Phase 2: Underline Segment Generation (Display List)

Modify the underline generation in [display_list.rs](../layout/src/solver3/display_list.rs) to generate segmented underlines:

```rust
/// Represents a segment of an underline (used for skip-ink support)
pub struct UnderlineSegment {
    pub start_x: f32,
    pub end_x: f32,
}

/// Calculate underline segments that skip over descenders
fn calculate_underline_segments(
    glyph_run: &SimpleGlyphRun,
    decoration_start_x: f32,
    decoration_end_x: f32,
    underline_y: f32,
    thickness: f32,
    font: &ParsedFont,
) -> Vec<UnderlineSegment> {
    let units_per_em = font.font_metrics.units_per_em as f32;
    let scale = glyph_run.font_size_px / units_per_em;
    
    // Collect gap regions from descender glyphs
    let mut gaps: Vec<(f32, f32)> = Vec::new();
    
    for glyph in &glyph_run.glyphs {
        if let Some(owned_glyph) = font.glyph_records_decoded.get(&glyph.index) {
            // Check if glyph has a descender
            if owned_glyph.has_descender() {
                // Calculate the gap region for this glyph
                let glyph_x = glyph.point.x;
                let gap_start = glyph_x + (owned_glyph.bounding_box.min_x as f32 * scale);
                let gap_end = glyph_x + (owned_glyph.bounding_box.max_x as f32 * scale);
                
                // Add small padding around the gap for aesthetics
                let padding = thickness * 0.5;
                gaps.push((gap_start - padding, gap_end + padding));
            }
        }
    }
    
    // If no gaps, return single continuous segment
    if gaps.is_empty() {
        return vec![UnderlineSegment {
            start_x: decoration_start_x,
            end_x: decoration_end_x,
        }];
    }
    
    // Sort and merge overlapping gaps
    gaps.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let merged_gaps = merge_overlapping_ranges(&gaps);
    
    // Generate segments between gaps
    let mut segments = Vec::new();
    let mut current_x = decoration_start_x;
    
    for (gap_start, gap_end) in merged_gaps {
        if gap_start > current_x {
            segments.push(UnderlineSegment {
                start_x: current_x,
                end_x: gap_start,
            });
        }
        current_x = gap_end;
    }
    
    // Final segment after last gap
    if current_x < decoration_end_x {
        segments.push(UnderlineSegment {
            start_x: current_x,
            end_x: decoration_end_x,
        });
    }
    
    segments
}

fn merge_overlapping_ranges(ranges: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if ranges.is_empty() {
        return Vec::new();
    }
    
    let mut merged = vec![ranges[0]];
    for &(start, end) in &ranges[1..] {
        let last = merged.last_mut().unwrap();
        if start <= last.1 {
            last.1 = last.1.max(end);
        } else {
            merged.push((start, end));
        }
    }
    merged
}
```

### Phase 3: Update Display List Generation

Modify the underline generation code in [display_list.rs#L2793-L2801](../layout/src/solver3/display_list.rs#L2793-L2801):

```rust
if needs_underline {
    let underline_y = baseline_y + (font_size * 0.12);
    
    // Check if we should skip over descenders
    // For now, use conservative approach: detect descender glyphs
    let segments = calculate_underline_segments(
        &glyph_run,
        decoration_start_x,
        decoration_end_x,
        underline_y,
        thickness,
        fonts,
    );
    
    // Push each segment as a separate underline
    for segment in segments {
        let segment_width = segment.end_x - segment.start_x;
        if segment_width > 0.0 {
            let underline_bounds = LogicalRect::new(
                LogicalPosition::new(segment.start_x, underline_y),
                LogicalSize::new(segment_width, thickness),
            );
            builder.push_underline(underline_bounds, glyph_run.color, thickness);
        }
    }
}
```

### Phase 4: Add CSS Property Support (Optional Enhancement)

Add `text-decoration-skip-ink` CSS property parsing:

```rust
// In css/src/props/style/text.rs

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum TextDecorationSkipInk {
    /// Don't skip over descenders
    None,
    /// Automatically skip over descenders (default behavior)
    #[default]
    Auto,
    /// Always skip (same as auto for most cases)
    All,
}
```

## Implementation Complexity Analysis

| Phase | Complexity | Lines of Code | Dependencies |
|-------|------------|---------------|--------------|
| Phase 1 (Descender Detection) | Low | ~50 lines | None |
| Phase 2 (Segment Generation) | Medium | ~80 lines | Phase 1 |
| Phase 3 (DL Update) | Low | ~30 lines | Phase 2 |
| Phase 4 (CSS Property) | Medium | ~100 lines | CSS parser |

**Total Estimated: ~260 lines of new code**

## Easy TODO Fixes from the TODO List

Based on the TODO_LIST.md analysis, here are quick wins that can be fixed immediately:

### 1. ua_css.rs TEXT_DECORATION_UNDERLINE (Already Supported!)

**Location**: [core/src/ua_css.rs#L466-L467](../core/src/ua_css.rs#L466-L467)

The TODO says "Uncomment when TextDecoration is implemented" - but it IS implemented! This can be fixed now:

```rust
// BEFORE (commented out):
// const TEXT_DECORATION_UNDERLINE: CssProperty = CssProperty::TextDecoration(

// AFTER:
const TEXT_DECORATION_UNDERLINE: CssProperty = 
    CssProperty::text_decoration(StyleTextDecoration::Underline);
```

### 2. Easy Comment Cleanups

These TODOs are just documentation/aspirational and don't need code changes:

- `api.json#L28227`: Comment only, no action needed
- `core/src/gl.rs#L763`: Low priority epoch overflow - leave as is
- `core/src/id.rs#L546`: Rayon parallelization - optimization, not bug

### 3. display_list.rs TODOs (Lines 2660-2664)

These TODOs in [display_list.rs](../layout/src/solver3/display_list.rs#L2660-L2664) can be partially addressed:

```rust
// TODO: This will always paint images over the glyphs
// TODO: Handle z-index within inline content (e.g. background images)
// TODO: Handle text decorations (underline, strikethrough, etc.)  <- BEING ADDRESSED
// TODO: Handle text shadows
// TODO: Handle text overflowing (based on container_rect and overflow behavior)
```

The "Handle text decorations" TODO is already partially implemented (basic underlines work). After implementing skip-ink, this TODO can be updated.

## Common Descender Characters

For reference, here are the common Latin characters with descenders that would trigger skip-ink:

| Character | Unicode | Description |
|-----------|---------|-------------|
| g | U+0067 | Latin small letter g |
| j | U+006A | Latin small letter j |
| p | U+0070 | Latin small letter p |
| q | U+0071 | Latin small letter q |
| y | U+0079 | Latin small letter y |
| Q | U+0051 | Latin capital letter Q (in some fonts) |
| ç | U+00E7 | Latin small letter c with cedilla |
| ß | U+00DF | Latin small letter sharp s (in some fonts) |

## Performance Considerations

1. **Caching**: Descender information per glyph ID can be cached in `ParsedFont`
2. **Early Exit**: Most glyphs don't have descenders; use bounding box quick-check
3. **Conservative Approach**: Using bounding box (not outline analysis) is faster

## Recommended Implementation Order

1. **Commit 1**: Enable TEXT_DECORATION_UNDERLINE in ua_css.rs (5 min fix)
2. **Commit 2**: Add `has_descender()` method to `OwnedGlyph`
3. **Commit 3**: Implement segment generation with gaps
4. **Commit 4**: Update display list generation to use segments
5. **Commit 5**: (Optional) Add CSS property parsing

## Test Cases

```rust
#[test]
fn test_underline_skip_ink_basic() {
    // Text "gyp" should have gaps under each letter
    let text = "gyp";
    // ... verify 4 underline segments are generated
}

#[test]
fn test_underline_no_descenders() {
    // Text "abc" should have 1 continuous underline
    let text = "abc";
    // ... verify 1 underline segment is generated
}

#[test]
fn test_underline_mixed() {
    // Text "apbqc" should have gaps under 'p' and 'q'
    let text = "apbqc";
    // ... verify 3 underline segments are generated
}
```

## References

- CSS Text Decoration Level 4: https://www.w3.org/TR/css-text-decor-4/#text-decoration-skip-ink-property
- Firefox implementation: Uses glyph outlines for precise skip detection
- Chrome implementation: Uses glyph bounding boxes (our proposed approach)
