Looking at this font engine implementation, I see several architectural and correctness issues that need addressing. Let me analyze the major problems first, then propose a better indexing structure.

## Major Issues

### 1. Bidi Reordering is Fundamentally Broken
You're correct that the current implementation can't properly handle mixed content. The reordering operates on `&str` but needs to handle a unified data source containing text, images, and other inline objects. The current approach loses non-text items during reordering.

### 2. Indexing is Overly Complex and Error-Prone
The current system maintains:
- `logical_byte_index` + `logical_byte_len` in glyphs
- `codepoint` stored in each glyph
- A concatenated `full_text` string
- Manual byte-to-run mapping

This creates multiple sources of truth and complex maintenance overhead.

### 3. Missing Font Fallback During Shaping
The font fallback logic tries fonts sequentially but doesn't handle the case where different parts of the same run need different fonts (common with mixed scripts).

### 4. Incomplete Vertical Text Support
Vertical metrics calculation is stubbed out and text orientation logic is incomplete.

## Proposed Smart Indexing Structure## Structural Tree Analysis

The proposed architecture creates this logical flow:

```
Original Content → LogicalItems → VisualItems → ShapedItems → PositionedItems
      ↓               ↓              ↓             ↓              ↓
 InlineContent → ContentIndex → BidiReordered → Clusters → LayoutResult

Each stage maintains precise indexing:
- ContentIndex: {run_index, item_index} - tracks original position  
- ClusterIndex: ContentIndex + glyph_offset - tracks within-cluster position
- No string concatenation needed
- No codepoint storage in glyphs
- Unified handling of text and non-text content
```

## Key Architectural Improvements

### 1. **Eliminates `full_text` Concatenation**

Instead of concatenating all text into a single string, each logical item 
maintains its own content with precise indexing back to the original `InlineContent`.

### 2. **Unified Bidi Data Source**

The `BidiDataSource` trait allows the bidi algorithm to work on structured data 
containing both text and non-text items, using object replacement 
characters (`\u{FFFC}`) as placeholders.

### 3. **Granular Font Fallback**

The `FontProvider` trait can return multiple fonts for different ranges within 
the same text, enabling proper handling of mixed scripts.

### 4. **Cluster-Based Shaping**

Groups related glyphs into clusters that maintain logical cohesion, making 
selection and cursor positioning more accurate.

## Additional Fixes Needed

### 1. **Cache Architecture**

The current cache is too coarse. You should cache at multiple levels:

```rust
// Cache shaped clusters by text + style + script
struct ClusterCache(HashMap<ClusterKey, Vec<ShapedCluster>>);

// Cache line breaks by content + constraints  
struct LineBreakCache(HashMap<LineBreakKey, Vec<BreakPoint>>);

// Cache final positioned lines
struct LayoutCache(HashMap<LayoutKey, Arc<UnifiedLayout>>);
```

### 2. **Vertical Text Implementation**

The current vertical text support is incomplete. You need:

- Proper VHEA/VMTX table parsing
- Correct baseline calculations
- Text orientation transforms
- Vertical justification algorithms

### 3. **Missing Features**

Several important features are stubbed or missing:

- Complex script support (Indic, Arabic contextual forms)
- Proper hyphenation integration
- Advanced OpenType features (ligatures, kerning)
- Font variation support
- Color emoji handling

The smart indexing approach eliminates most of the byte-level complexity while 
providing precise mapping for text selection and editing operations. This architecture 
would be much more maintainable and performant than the current implementation.

```rust
// Smart indexing system that eliminates the need for full_text concatenation
// and provides precise mapping between logical and visual content

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentIndex {
    pub run_index: u16,      // Which InlineContent item
    pub item_index: u16,     // Which item within the run (for text: char index)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClusterIndex {
    pub content: ContentIndex,
    pub glyph_offset: u8,    // Which glyph within the cluster
}

#[derive(Debug, Clone)]
pub enum LogicalItem {
    Text {
        run_index: u16,
        char_range: std::ops::Range<u16>, // Character indices within the run
        style: Arc<StyleProperties>,
    },
    Image {
        run_index: u16,
        image: InlineImage,
    },
    Shape {
        run_index: u16, 
        shape: InlineShape,
    },
    Space {
        run_index: u16,
        space: InlineSpace,
    },
    Break {
        run_index: u16,
        break_info: InlineBreak,
    },
}

#[derive(Debug, Clone)]
pub struct VisualItem {
    pub logical_source: ContentIndex,
    pub bidi_level: BidiLevel,
    pub content: VisualContent,
}

#[derive(Debug, Clone)]
pub enum VisualContent {
    TextRun {
        text: String,  // Reordered text for this visual run
        script: Script,
        language: Language,
        direction: Direction,
        style: Arc<StyleProperties>,
    },
    NonText(InlineContent), // Images, shapes etc. - no reordering needed
}

#[derive(Debug, Clone)]
pub struct ShapedCluster {
    pub logical_source: ContentIndex,
    pub glyphs: Vec<ShapedGlyph>,
    pub advance: f32,
    pub vertical_advance: f32,
}

#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    pub glyph_id: u16,
    pub font: Arc<dyn ParsedFontTrait>,
    pub cluster_offset: u8,  // Position within cluster
    pub advance: f32,
    pub offset: Point,
    pub orientation: GlyphOrientation,
}

#[derive(Debug, Clone)]
pub enum ShapedItem {
    Cluster(ShapedCluster),
    NonText {
        source: ContentIndex,
        bounds: Rect,
        baseline_offset: f32,
    },
}

// Main pipeline stages
pub trait BidiDataSource {
    fn len(&self) -> usize;
    fn get_item(&self, index: usize) -> &LogicalItem;
    fn get_base_direction(&self) -> Direction;
    fn items(&self) -> &[LogicalItem];
}

pub struct LogicalContent {
    items: Vec<LogicalItem>,
    base_direction: Direction,
}

impl BidiDataSource for LogicalContent {
    fn len(&self) -> usize { self.items.len() }
    fn get_item(&self, index: usize) -> &LogicalItem { &self.items[index] }
    fn get_base_direction(&self) -> Direction { self.base_direction }
    fn items(&self) -> &[LogicalItem] { &self.items }
}

// Enhanced pipeline with proper separation of concerns
pub struct SmartLayoutEngine;

impl SmartLayoutEngine {
    // Stage 1: Convert InlineContent to LogicalItems with proper indexing
    pub fn create_logical_content(
        content: &[InlineContent]
    ) -> Result<LogicalContent, LayoutError> {
        let mut logical_items = Vec::new();
        let mut has_strong_rtl = false;
        let mut has_strong_ltr = false;
        
        for (run_idx, item) in content.iter().enumerate() {
            match item {
                InlineContent::Text(text_run) => {
                    // Segment text by script/direction for better bidi handling
                    let segments = segment_text_by_script(&text_run.text);
                    
                    for segment in segments {
                        logical_items.push(LogicalItem::Text {
                            run_index: run_idx as u16,
                            char_range: segment.char_range,
                            style: text_run.style.clone(),
                        });
                        
                        // Update direction tracking
                        if segment.has_strong_rtl { has_strong_rtl = true; }
                        if segment.has_strong_ltr { has_strong_ltr = true; }
                    }
                }
                InlineContent::Image(img) => {
                    logical_items.push(LogicalItem::Image {
                        run_index: run_idx as u16,
                        image: img.clone(),
                    });
                }
                InlineContent::Shape(shape) => {
                    logical_items.push(LogicalItem::Shape {
                        run_index: run_idx as u16,
                        shape: shape.clone(),
                    });
                }
                InlineContent::Space(space) => {
                    logical_items.push(LogicalItem::Space {
                        run_index: run_idx as u16,
                        space: space.clone(),
                    });
                }
                InlineContent::LineBreak(br) => {
                    logical_items.push(LogicalItem::Break {
                        run_index: run_idx as u16,
                        break_info: br.clone(),
                    });
                }
            }
        }
        
        let base_direction = if has_strong_rtl && !has_strong_ltr {
            Direction::Rtl
        } else {
            Direction::Ltr
        };
        
        Ok(LogicalContent {
            items: logical_items,
            base_direction,
        })
    }
    
    // Stage 2: Bidi reordering that works on structured data
    pub fn apply_bidi_reordering(
        logical: &LogicalContent
    ) -> Result<Vec<VisualItem>, LayoutError> {
        // Create a temporary string representation for unicode_bidi crate
        let mut bidi_string = String::new();
        let mut item_boundaries = Vec::new();
        
        for item in &logical.items {
            let start_len = bidi_string.len();
            match item {
                LogicalItem::Text { char_range, .. } => {
                    // Add actual text characters
                    let original_text = get_original_text(item)?;
                    bidi_string.push_str(&original_text);
                }
                _ => {
                    // Use object replacement character for non-text
                    bidi_string.push('\u{FFFC}');
                }
            }
            item_boundaries.push(start_len..bidi_string.len());
        }
        
        if bidi_string.is_empty() {
            return Ok(Vec::new());
        }
        
        let bidi_info = BidiInfo::new(&bidi_string, None);
        let para = &bidi_info.paragraphs[0];
        let (levels, visual_runs) = bidi_info.visual_runs(para, para.range.clone());
        
        let mut visual_items = Vec::new();
        
        for run_range in visual_runs {
            let run_level = levels[run_range.start];
            
            // Find which logical items overlap with this visual run
            for (item_idx, boundary) in item_boundaries.iter().enumerate() {
                if ranges_overlap(&run_range, boundary) {
                    let logical_item = &logical.items[item_idx];
                    let content_index = ContentIndex {
                        run_index: get_run_index(logical_item),
                        item_index: item_idx as u16,
                    };
                    
                    let visual_content = match logical_item {
                        LogicalItem::Text { style, .. } => {
                            let text_slice = &bidi_string[
                                run_range.start.max(boundary.start)..
                                run_range.end.min(boundary.end)
                            ];
                            
                            VisualContent::TextRun {
                                text: text_slice.to_string(),
                                script: detect_script(text_slice).unwrap_or(Script::Latin),
                                language: Language::EnglishUS, // TODO: proper detection
                                direction: if run_level.is_rtl() { Direction::Rtl } else { Direction::Ltr },
                                style: style.clone(),
                            }
                        }
                        _ => VisualContent::NonText(convert_logical_to_inline(logical_item)?),
                    };
                    
                    visual_items.push(VisualItem {
                        logical_source: content_index,
                        bidi_level: BidiLevel::new(run_level.number()),
                        content: visual_content,
                    });
                }
            }
        }
        
        Ok(visual_items)
    }
    
    // Stage 3: Shape visual items with proper font fallback
    pub fn shape_visual_items<T: ParsedFontTrait>(
        visual_items: &[VisualItem],
        font_manager: &dyn FontProvider<T>
    ) -> Result<Vec<ShapedItem>, LayoutError> {
        let mut shaped_items = Vec::new();
        
        for visual_item in visual_items {
            match &visual_item.content {
                VisualContent::TextRun { text, script, language, direction, style } => {
                    let clusters = shape_text_with_fallback(
                        text,
                        *script,
                        *language,
                        *direction,
                        style,
                        font_manager,
                        visual_item.logical_source,
                    )?;
                    
                    shaped_items.extend(clusters.into_iter().map(ShapedItem::Cluster));
                }
                VisualContent::NonText(content) => {
                    let bounds = measure_inline_content(content)?;
                    shaped_items.push(ShapedItem::NonText {
                        source: visual_item.logical_source,
                        bounds,
                        baseline_offset: get_baseline_offset(content),
                    });
                }
            }
        }
        
        Ok(shaped_items)
    }
}

// Helper functions (stubs - would need full implementation)
fn segment_text_by_script(text: &str) -> Vec<TextSegment> {
    // TODO: Implement proper script segmentation
    vec![]
}

fn get_original_text(item: &LogicalItem) -> Result<String, LayoutError> {
    // TODO: Get text from original content
    Ok(String::new())
}

fn ranges_overlap(a: &std::ops::Range<usize>, b: &std::ops::Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

fn get_run_index(item: &LogicalItem) -> u16 {
    match item {
        LogicalItem::Text { run_index, .. } => *run_index,
        LogicalItem::Image { run_index, .. } => *run_index,
        LogicalItem::Shape { run_index, .. } => *run_index,
        LogicalItem::Space { run_index, .. } => *run_index,
        LogicalItem::Break { run_index, .. } => *run_index,
    }
}

fn convert_logical_to_inline(item: &LogicalItem) -> Result<InlineContent, LayoutError> {
    // TODO: Convert back to InlineContent
    Err(LayoutError::InvalidText("Not implemented".to_string()))
}

fn shape_text_with_fallback<T: ParsedFontTrait>(
    text: &str,
    script: Script,
    language: Language,
    direction: Direction,
    style: &StyleProperties,
    font_manager: &dyn FontProvider<T>,
    source: ContentIndex,
) -> Result<Vec<ShapedCluster>, LayoutError> {
    // TODO: Implement proper font fallback shaping
    Ok(Vec::new())
}

fn measure_inline_content(content: &InlineContent) -> Result<Rect, LayoutError> {
    // TODO: Measure non-text content
    Ok(Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 })
}

fn get_baseline_offset(content: &InlineContent) -> f32 {
    // TODO: Calculate baseline offset
    0.0
}

#[derive(Debug, Clone)]
struct TextSegment {
    char_range: std::ops::Range<u16>,
    has_strong_ltr: bool,
    has_strong_rtl: bool,
}

// Enhanced trait for font providers
pub trait FontProvider<T: ParsedFontTrait> {
    fn get_font_for_text(
        &self,
        text: &str,
        script: Script,
        style: &StyleProperties
    ) -> Result<Vec<(Arc<T>, std::ops::Range<usize>)>, LayoutError>;
}
```