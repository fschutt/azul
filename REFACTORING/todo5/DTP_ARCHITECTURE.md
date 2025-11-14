# DTP Architecture Plan for Azul Layout Engine

## Executive Summary

This document outlines the architectural plan for implementing professional Desktop Publishing (DTP) features in the Azul layout engine, focusing on:
1. **Vertical Writing Modes** (for Mongolian, Japanese, Chinese, etc.)
2. **CSS Shapes** (shape-inside, shape-outside, exclusions) - 🟡 **IN PROGRESS (Nov 2024)**
3. **CSS Regions** (named flows for complex text chains)
4. **PDF Vertical Text Support** (W2 mode for proper text selection)

**Implementation Status (November 2024):**
- ✅ CSS Shape Parser (11 tests passing)
- ✅ C-Compatible Shape Structures  
- ✅ CSS Properties (ShapeInside/Outside/ClipPath)
- ✅ Layout Engine Bridge (ShapeBoundary::from_css_shape)
- ❌ CSS Property Getters (styled_dom integration)
- ❌ printpdf Test Integration

## Current Capabilities (✅)

The text3 engine already supports:
- ✅ **Multi-column layout** (`columns`, `column-gap`)
- ✅ **Flow across fragments** (text continuation between layout containers)
- ✅ **BiDi text** (Unicode BiDi algorithm with CSS direction)
- ✅ **Complex script shaping** (Arabic, Indic, Hebrew, etc. via allsorts)
- ✅ **Line breaking** (Unicode line break algorithm)
- ✅ **Text justification** (including kashida for Arabic)
- ✅ **Inline layout** with proper metrics and baseline alignment

## Missing Critical DTP Features (❌)

### 1. Vertical Writing Modes (❌)
**CSS Properties:**
- `writing-mode: vertical-rl` (top-to-bottom, right-to-left)
- `writing-mode: vertical-lr` (top-to-bottom, left-to-right)
- `text-orientation: mixed | upright | sideways`

**Use Cases:**
- Mongolian traditional script (vertical-lr)
- Japanese/Chinese (vertical-rl)
- Sideways Latin text in vertical context
- Rotated labels and headers

### 2. CSS Shapes (🟡 In Progress)
**CSS Properties:**
- `shape-inside: circle() | ellipse() | polygon() | path()` ✅ Parser implemented
- `shape-outside: circle() | ellipse() | polygon() | path()` ✅ Parser implemented  
- `clip-path: circle() | ellipse() | polygon() | path()` ✅ Parser implemented
- `shape-margin: <length>` ✅ Exists
- `shape-image-threshold: <number>` ✅ Exists

**Implementation Status (November 2024):**
- ✅ **CSS Shape Parser** (`azul-css/src/shape_parser.rs`) - 11 unit tests passing
- ✅ **C-Compatible Shape Structures** (`azul-css/src/shape.rs`) - repr(C) with Eq/Hash/Ord traits
- ✅ **CSS Properties** (`azul-css/src/props/layout/shape.rs`) - ShapeInside, ShapeOutside, ClipPath enums
- ✅ **Layout Engine Bridge** (`azul/layout/src/text3/cache.rs`) - `ShapeBoundary::from_css_shape()`
- ❌ **CSS Property Getters** - Need styled_dom integration
- ❌ **Constraints Population** - Need to populate UnifiedConstraints from styled_dom
- ❌ **PDF Integration** - Need printpdf text layout tests

**Use Cases:**
- Text flowing inside circular/star/custom shapes
- Text wrapping around images or decorative elements
- Magazine layouts with non-rectangular text columns
- Artistic text layouts

### 3. Shape Exclusions (❌)
**CSS Properties:**
- `float: left | right` (exists but needs shape integration)
- Rectangular/shaped obstacles within containers
- Multiple exclusions with z-order handling

**Use Cases:**
- Text flowing around pulled quotes
- Image placement with text wrap
- Multiple exclusions (e.g., circular photo + rectangular caption)

### 4. CSS Regions (❌)
**CSS Properties:**
- `flow-into: <identifier>`
- `flow-from: <identifier>`
- `region-fragment: auto | break`

**Use Cases:**
- Magazine articles spanning multiple non-contiguous areas
- Headers/footers that consume from named flows
- Complex multi-column layouts with varying widths
- Text threading through arbitrary shapes

### 5. PDF Vertical Text Support (❌)
**PDF Features:**
- W2 array (vertical glyph widths)
- Proper text extraction for vertical text
- Writing mode metadata

**Use Cases:**
- Searchable vertical text in PDFs
- Proper copy-paste from vertical text
- Accessibility for screen readers

---

## Architectural Design

### Phase 1: Vertical Writing Mode Foundation

#### 1.1 Data Structures

**Add to `text3/cache.rs`:**

```rust
/// Defines the block flow direction (line stacking) and inline flow direction (text flow)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritingMode {
    /// Horizontal text, top-to-bottom block flow (default)
    HorizontalTb,
    /// Vertical text, right-to-left block flow (Japanese/Chinese)
    VerticalRl,
    /// Vertical text, left-to-right block flow (Mongolian)
    VerticalLr,
}

/// Controls glyph orientation in vertical writing modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOrientation {
    /// Mixed orientation based on Unicode properties (default for vertical)
    Mixed,
    /// Force upright orientation (no rotation)
    Upright,
    /// Force sideways orientation (90° clockwise rotation)
    Sideways,
}

/// Extends UnifiedConstraints
pub struct UnifiedConstraints {
    // ... existing fields ...
    
    /// Writing mode affects both line progression and glyph orientation
    pub writing_mode: WritingMode,
    
    /// Text orientation (only relevant for vertical writing modes)
    pub text_orientation: TextOrientation,
}
```

#### 1.2 Coordinate System Abstraction

**Problem:** Current code assumes horizontal layout (x = inline, y = block)

**Solution:** Abstract coordinate system based on writing mode

```rust
/// Logical coordinates independent of writing mode
#[derive(Debug, Clone, Copy)]
pub struct LogicalPosition {
    /// Position along the inline axis (text flow direction)
    pub inline: f32,
    /// Position along the block axis (line stacking direction)
    pub block: f32,
}

/// Physical coordinates in the rendered output
#[derive(Debug, Clone, Copy)]
pub struct PhysicalPosition {
    pub x: f32,
    pub y: f32,
}

impl WritingMode {
    /// Converts logical position to physical based on writing mode
    pub fn to_physical(&self, logical: LogicalPosition, container_size: PhysicalSize) -> PhysicalPosition {
        match self {
            WritingMode::HorizontalTb => PhysicalPosition {
                x: logical.inline,
                y: logical.block,
            },
            WritingMode::VerticalRl => PhysicalPosition {
                x: container_size.width - logical.block,
                y: logical.inline,
            },
            WritingMode::VerticalLr => PhysicalPosition {
                x: logical.block,
                y: logical.inline,
            },
        }
    }
    
    /// Returns true if this is a vertical writing mode
    pub fn is_vertical(&self) -> bool {
        matches!(self, WritingMode::VerticalRl | WritingMode::VerticalLr)
    }
    
    /// Returns the line progression direction (block axis)
    pub fn line_progression(&self) -> LineProgression {
        match self {
            WritingMode::HorizontalTb => LineProgression::TopToBottom,
            WritingMode::VerticalRl => LineProgression::RightToLeft,
            WritingMode::VerticalLr => LineProgression::LeftToRight,
        }
    }
}
```

#### 1.3 Modified Layout Algorithm

**Current:** `perform_fragment_layout()` assumes horizontal layout

**Changes Required:**

1. **Line breaking direction:**
   - Horizontal: break on width
   - Vertical: break on height (advance vertically, not horizontally)

2. **Line stacking:**
   - Horizontal: lines stack downward (y += line_height)
   - Vertical-RL: lines stack leftward (x -= line_width)
   - Vertical-LR: lines stack rightward (x += line_width)

3. **Available space calculation:**
   ```rust
   let available_size = match constraints.writing_mode {
       WritingMode::HorizontalTb => constraints.available_width,
       WritingMode::VerticalRl | WritingMode::VerticalLr => {
           constraints.available_height.unwrap_or(f32::MAX)
       }
   };
   ```

#### 1.4 Glyph Rotation

**For vertical text with `text-orientation: mixed`:**

- CJK ideographs → upright (0°)
- Latin letters → sideways (90° CW)
- Arabic/Hebrew → sideways
- Punctuation → context-dependent

**Implementation in `PositionedGlyph`:**

```rust
pub struct PositionedGlyph {
    // ... existing fields ...
    
    /// Rotation angle in degrees (0, 90, 180, 270)
    pub rotation: f32,
    
    /// Vertical advance (only used in vertical writing modes)
    pub vertical_advance: f32,
}
```

---

### Phase 2: CSS Shapes (shape-inside)

#### 2.1 Shape Definitions

```rust
/// Defines the shape of a container for text to flow inside
#[derive(Debug, Clone)]
pub enum ShapeInside {
    /// Rectangular shape (default, entire container)
    Rectangle {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    
    /// Circular shape
    Circle {
        center_x: f32,
        center_y: f32,
        radius: f32,
    },
    
    /// Elliptical shape
    Ellipse {
        center_x: f32,
        center_y: f32,
        radius_x: f32,
        radius_y: f32,
    },
    
    /// Polygonal shape (e.g., star)
    Polygon {
        points: Vec<(f32, f32)>,
    },
    
    /// Arbitrary SVG-like path
    Path {
        commands: Vec<PathCommand>,
    },
}

impl ShapeInside {
    /// Computes available horizontal line segments at a given y-position
    /// Returns segments sorted by start_x
    pub fn compute_line_segments(&self, y: f32, margin: f32) -> Vec<LineSegment> {
        match self {
            ShapeInside::Circle { center_x, center_y, radius } => {
                let dy = y - center_y;
                let r_with_margin = radius - margin;
                
                if dy.abs() > r_with_margin {
                    return vec![]; // Outside circle
                }
                
                // Chord width at y: w = 2*sqrt(r²-dy²)
                let half_width = (r_with_margin.powi(2) - dy.powi(2)).sqrt();
                
                vec![LineSegment {
                    start_x: center_x - half_width,
                    width: 2.0 * half_width,
                    priority: 0,
                }]
            }
            
            ShapeInside::Polygon { points } => {
                compute_polygon_intersections(points, y, margin)
            }
            
            // ... other shapes
        }
    }
}
```

#### 2.2 Integration with Line Breaking

**Current:** `position_one_line()` uses simple `available_width`

**New:** Use `LineConstraints` with multiple segments per line

```rust
pub struct LineConstraints {
    /// Available segments at this line's y-position
    /// Sorted by start_x, non-overlapping
    pub segments: Vec<LineSegment>,
    
    /// Total available width (sum of all segments)
    pub total_available: f32,
}

impl LineConstraints {
    /// Creates constraints from a shape at a given y-position
    pub fn from_shape(shape: &ShapeInside, y: f32, shape_margin: f32) -> Self {
        let segments = shape.compute_line_segments(y, shape_margin);
        let total_available = segments.iter().map(|s| s.width).sum();
        Self { segments, total_available }
    }
    
    /// Subtracts exclusion shapes from available segments
    pub fn subtract_exclusions(&mut self, exclusions: &[ShapeExclusion]) {
        for exclusion in exclusions {
            self.segments = subtract_segments(&self.segments, exclusion.segments());
        }
        self.total_available = self.segments.iter().map(|s| s.width).sum();
    }
}
```

**Modified `perform_fragment_layout()`:**

```rust
fn perform_fragment_layout(
    cursor: &mut BreakCursor,
    logical_items: &[LogicalItem],
    constraints: &UnifiedConstraints,
) -> Result<FragmentLayout, LayoutError> {
    let mut lines = Vec::new();
    let mut current_y = 0.0;
    
    while !cursor.is_done() {
        // Compute available space at this y-position
        let mut line_constraints = if let Some(shape) = &constraints.shape_inside {
            LineConstraints::from_shape(shape, current_y, constraints.shape_margin)
        } else {
            // Default: full width
            LineConstraints {
                segments: vec![LineSegment {
                    start_x: 0.0,
                    width: constraints.available_width,
                    priority: 0,
                }],
                total_available: constraints.available_width,
            }
        };
        
        // Apply exclusions
        if !constraints.exclusions.is_empty() {
            line_constraints.subtract_exclusions(&constraints.exclusions);
        }
        
        // Break and position line using available segments
        let line = break_and_position_line(cursor, &line_constraints, constraints)?;
        
        current_y += line.height;
        lines.push(line);
        
        // Check available height
        if let Some(max_height) = constraints.available_height {
            if current_y >= max_height {
                break;
            }
        }
    }
    
    // ...
}
```

---

### Phase 3: Shape Exclusions

#### 3.1 Exclusion Definition

```rust
/// A shape that excludes text from flowing through it
#[derive(Debug, Clone)]
pub struct ShapeExclusion {
    /// The shape geometry
    pub shape: ShapeInside,
    
    /// Position offset of the shape
    pub offset_x: f32,
    pub offset_y: f32,
    
    /// Z-order for overlapping exclusions (higher = later processing)
    pub z_index: i32,
}

impl ShapeExclusion {
    /// Computes the segments that this exclusion blocks at y-position
    pub fn segments_at_y(&self, y: f32) -> Vec<LineSegment> {
        let relative_y = y - self.offset_y;
        self.shape.compute_line_segments(relative_y, 0.0)
            .into_iter()
            .map(|mut seg| {
                seg.start_x += self.offset_x;
                seg
            })
            .collect()
    }
}
```

#### 3.2 Segment Subtraction Algorithm

```rust
/// Subtracts exclusion segments from available segments
/// Returns new non-overlapping segments
fn subtract_segments(
    available: &[LineSegment],
    exclusions: &[LineSegment],
) -> Vec<LineSegment> {
    let mut result = available.to_vec();
    
    for exclusion in exclusions {
        let excl_start = exclusion.start_x;
        let excl_end = excl_start + exclusion.width;
        
        result = result.into_iter().flat_map(|seg| {
            let seg_start = seg.start_x;
            let seg_end = seg_start + seg.width;
            
            if excl_end <= seg_start || excl_start >= seg_end {
                // No overlap
                vec![seg]
            } else if excl_start <= seg_start && excl_end >= seg_end {
                // Exclusion covers entire segment
                vec![]
            } else if excl_start > seg_start && excl_end < seg_end {
                // Exclusion splits segment in two
                vec![
                    LineSegment {
                        start_x: seg_start,
                        width: excl_start - seg_start,
                        priority: seg.priority,
                    },
                    LineSegment {
                        start_x: excl_end,
                        width: seg_end - excl_end,
                        priority: seg.priority,
                    },
                ]
            } else if excl_start > seg_start {
                // Exclusion clips right side
                vec![LineSegment {
                    start_x: seg_start,
                    width: excl_start - seg_start,
                    priority: seg.priority,
                }]
            } else {
                // Exclusion clips left side
                vec![LineSegment {
                    start_x: excl_end,
                    width: seg_end - excl_end,
                    priority: seg.priority,
                }]
            }
        }).collect();
    }
    
    result
}
```

---

### Phase 4: CSS Regions (Named Flows)

#### 4.1 Flow Manager

```rust
/// Manages named flows of content across multiple region containers
pub struct FlowManager {
    /// Named flows: identifier -> content
    flows: HashMap<String, FlowContent>,
    
    /// Region chains: identifier -> ordered list of region containers
    regions: HashMap<String, Vec<RegionContainer>>,
}

pub struct FlowContent {
    /// The content to flow (already shaped and processed)
    pub items: Vec<ShapedItem>,
    
    /// Current position in the flow (for incremental layout)
    pub cursor_position: usize,
}

pub struct RegionContainer {
    /// Fragment ID or box identifier
    pub id: String,
    
    /// Layout constraints for this region
    pub constraints: UnifiedConstraints,
    
    /// Optional shape (if not shape, uses rectangular bounds)
    pub shape: Option<ShapeInside>,
}
```

#### 4.2 Flow Layout Algorithm

```rust
impl FlowManager {
    /// Lays out a named flow across all its region containers
    pub fn layout_flow(
        &mut self,
        flow_name: &str,
        font_manager: &dyn FontManager,
    ) -> Result<HashMap<String, FragmentLayout>, LayoutError> {
        let flow = self.flows.get(flow_name)
            .ok_or(LayoutError::FlowNotFound)?;
        
        let regions = self.regions.get(flow_name)
            .ok_or(LayoutError::RegionsNotFound)?;
        
        let mut results = HashMap::new();
        let mut cursor = BreakCursor::new(&flow.items);
        
        for region in regions {
            if cursor.is_done() {
                break;
            }
            
            let mut constraints = region.constraints.clone();
            if let Some(shape) = &region.shape {
                constraints.shape_inside = Some(shape.clone());
            }
            
            // Layout as much as fits in this region
            let layout = perform_fragment_layout(
                &mut cursor,
                &flow.items, // Logical items needed for bidi
                &constraints,
            )?;
            
            results.insert(region.id.clone(), layout);
        }
        
        // Remaining content goes to overflow
        // (caller can handle by creating additional regions or truncating)
        
        Ok(results)
    }
}
```

#### 4.3 Integration with Existing Fragment Chains

**Current:** `layout_flow()` takes a `Vec<LayoutFragment>`

**Enhancement:** Support named flow references

```rust
pub enum FragmentContent {
    /// Direct inline content (current approach)
    Inline(Vec<InlineContent>),
    
    /// Reference to a named flow
    NamedFlow(String),
}

pub struct LayoutFragment {
    pub id: String,
    pub constraints: UnifiedConstraints,
    
    /// Content source for this fragment
    pub content: FragmentContent,
}
```

---

### Phase 5: PDF W2 Vertical Writing Support

#### 5.1 PDF Text Positioning

**Current:** PDF text uses horizontal metrics only

**Vertical text requires:**
1. **W2 array** in font descriptor (vertical advance widths)
2. **TJ operator positioning** must account for vertical advance
3. **Text matrix rotation** for proper glyph orientation

#### 5.2 Implementation in printpdf

```rust
// In printpdf/src/text.rs or similar

/// Generates PDF TJ operator with proper positioning for vertical text
pub fn generate_vertical_text_operator(
    glyphs: &[PositionedGlyph],
    writing_mode: WritingMode,
) -> Vec<u8> {
    if !writing_mode.is_vertical() {
        // Use existing horizontal logic
        return generate_horizontal_text_operator(glyphs);
    }
    
    let mut tj_array = Vec::new();
    let mut prev_y = 0.0;
    
    for glyph in glyphs {
        // Vertical positioning uses dy instead of dx
        let dy = glyph.position.y - prev_y;
        
        if dy.abs() > 0.01 {
            // Add positioning adjustment
            let pdf_units = -(dy * 1000.0 / font_size);
            tj_array.push(TjArrayElement::Offset(pdf_units as i32));
        }
        
        tj_array.push(TjArrayElement::Glyph(glyph.glyph_id));
        prev_y = glyph.position.y;
    }
    
    encode_tj_array(tj_array)
}

/// Adds W2 array to font descriptor for vertical metrics
pub fn add_vertical_metrics_to_font(
    font_dict: &mut lopdf::Dictionary,
    vertical_advances: &[f32],
) {
    let w2_array: Vec<lopdf::Object> = vertical_advances
        .iter()
        .map(|&advance| lopdf::Object::Real(advance))
        .collect();
    
    font_dict.set("W2", lopdf::Object::Array(w2_array));
}
```

---

## Implementation Roadmap

### Milestone 1: Vertical Writing Mode MVP (2-3 weeks)
- [ ] Add `WritingMode` and `TextOrientation` to `UnifiedConstraints`
- [ ] Implement coordinate system abstraction (`LogicalPosition` ↔ `PhysicalPosition`)
- [ ] Modify `perform_fragment_layout()` to handle vertical line stacking
- [ ] Add basic glyph rotation for vertical text
- [ ] Test with simple vertical text (no shapes)

### Milestone 2: Basic CSS Shapes (2-3 weeks)
- [ ] Implement `ShapeInside` enum (Circle, Rectangle, Polygon)
- [ ] Add `compute_line_segments()` for each shape type
- [ ] Integrate `LineConstraints` into line breaking
- [ ] Test with circular text container

### Milestone 3: Shape Exclusions (1-2 weeks)
- [ ] Implement `ShapeExclusion` struct
- [ ] Add segment subtraction algorithm
- [ ] Test with rectangular exclusion in circle
- [ ] Test with multiple overlapping exclusions

### Milestone 4: Named Flows (CSS Regions) (2-3 weeks)
- [ ] Implement `FlowManager` and `FlowContent`
- [ ] Extend `LayoutFragment` to support named flows
- [ ] Implement flow layout algorithm
- [ ] Test with multi-region flow

### Milestone 5: PDF Vertical Text (1 week)
- [ ] Implement W2 array generation
- [ ] Modify TJ operator for vertical positioning
- [ ] Test PDF text selection in vertical text

### Milestone 6: Mongolian Test Case (1 week)
- [ ] Add Mongolian to `Script` enum
- [ ] Verify Mongolian shaping with Noto Sans Mongolian
- [ ] Create comprehensive test: circle + exclusion + star overflow + vertical-lr
- [ ] Validate PDF output with W2 mode

**Total Estimated Time: 10-15 weeks for full implementation**

---

## Testing Strategy

### Unit Tests
- `test_vertical_rl_line_stacking()` - Lines stack right-to-left
- `test_vertical_lr_line_stacking()` - Lines stack left-to-right
- `test_circle_line_segments()` - Compute chord widths correctly
- `test_segment_subtraction()` - Exclusion algorithm
- `test_polygon_intersection()` - Star shape boundaries
- `test_named_flow_layout()` - Multi-region flow

### Integration Tests
- `test_mongolian_vertical_in_circle()` - Full DTP scenario
- `test_japanese_vertical_with_latin()` - Mixed scripts
- `test_magazine_layout()` - Complex multi-column + shapes
- `test_pdf_vertical_text_selection()` - W2 mode validation

### Visual Regression Tests
- Compare rendered output of shaped text layouts
- Validate PDF text extraction matches source content

---

## API Design (CSS Property Mapping)

```rust
// In azul/layout/src/solver3/fc.rs or similar

impl CssPropertyReader {
    fn read_writing_mode(&self, node_id: NodeId) -> WritingMode {
        match self.get_property(node_id, "writing-mode") {
            Some("vertical-rl") => WritingMode::VerticalRl,
            Some("vertical-lr") => WritingMode::VerticalLr,
            _ => WritingMode::HorizontalTb,
        }
    }
    
    fn read_text_orientation(&self, node_id: NodeId) -> TextOrientation {
        match self.get_property(node_id, "text-orientation") {
            Some("upright") => TextOrientation::Upright,
            Some("sideways") => TextOrientation::Sideways,
            _ => TextOrientation::Mixed,
        }
    }
    
    fn read_shape_inside(&self, node_id: NodeId) -> Option<ShapeInside> {
        let value = self.get_property(node_id, "shape-inside")?;
        parse_shape_value(value)
    }
}

fn parse_shape_value(css_value: &str) -> Option<ShapeInside> {
    // Parse CSS shape functions:
    // - circle(50px at 100px 100px)
    // - ellipse(50px 75px at 100px 100px)
    // - polygon(50% 0%, 100% 50%, 50% 100%, 0% 50%)
    // - path("M 0 0 L 100 0 L 100 100 Z")
    
    // TODO: Implement CSS shape parser
    None
}
```

---

## Performance Considerations

### Optimization Opportunities

1. **Shape Segment Caching:**
   - Pre-compute line segments for common y-positions
   - Use spatial indexing for complex polygons

2. **Incremental Layout:**
   - Only re-layout affected regions when exclusions change
   - Cache shaped items between layout passes

3. **Parallel Processing:**
   - Independent regions can be laid out in parallel
   - Shape segment computation can be parallelized

4. **Memory Efficiency:**
   - Share shaped items across fragments
   - Use arc-swapping for large flow content

### Expected Performance Impact

- **Vertical text:** ~10-15% slower than horizontal (rotation overhead)
- **Simple shapes (circle):** ~5-10% slower (segment computation)
- **Complex shapes (star):** ~20-30% slower (polygon intersection)
- **Named flows:** Negligible if regions are pre-defined

---

## Backward Compatibility

All new features are **opt-in** via CSS properties:
- Default `writing-mode: horizontal-tb` maintains current behavior
- No `shape-inside` means full container width (current behavior)
- No `flow-into/flow-from` means direct content (current behavior)

**No breaking changes to existing APIs.**

---

## Risks and Mitigations

### Risk 1: Complexity Explosion
**Mitigation:** Implement incrementally, test at each phase, maintain clear abstractions

### Risk 2: PDF Renderer Limitations
**Mitigation:** Research PDF 1.7 spec for W2 support, implement fallback for older PDF versions

### Risk 3: Performance Degradation
**Mitigation:** Profile early, cache aggressively, optimize hot paths

### Risk 4: Browser Compatibility
**Mitigation:** Follow CSS Shapes Level 1/2 spec closely, test against reference implementations

---

## Related Standards

- **CSS Writing Modes Level 3:** https://www.w3.org/TR/css-writing-modes-3/
- **CSS Shapes Level 1:** https://www.w3.org/TR/css-shapes-1/
- **CSS Shapes Level 2:** https://drafts.csswg.org/css-shapes-2/
- **CSS Regions Module:** https://www.w3.org/TR/css-regions-1/
- **PDF Reference 1.7:** ISO 32000-1:2008 (Section 9.7 - Text)
- **OpenType Vertical Metrics:** https://docs.microsoft.com/en-us/typography/opentype/spec/vhea

---

## Conclusion

This architecture provides a **comprehensive path** to professional DTP capabilities while maintaining:
- ✅ **Backward compatibility** with existing layouts
- ✅ **Performance** through caching and incremental layout
- ✅ **Standards compliance** with CSS specifications
- ✅ **Testability** with clear unit and integration tests

The modular design allows **incremental implementation** over multiple releases, with each phase delivering tangible value independently.
