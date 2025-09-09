use std::sync::Arc;
use std::collections::HashMap;

// Enhanced content model supporting mixed inline content
#[derive(Debug, Clone)]
pub enum InlineContent {
    Text(StyledRun),
    Image(InlineImage),
    Shape(InlineShape),
    Space(InlineSpace),
    LineBreak(InlineBreak),
    Custom(Box<dyn CustomInlineContent>),
}

#[derive(Debug, Clone)]
pub struct InlineImage {
    pub source: ImageSource,
    pub intrinsic_size: Size,
    pub display_size: Option<Size>,
    pub baseline_offset: f32,     // How much to shift baseline
    pub alignment: VerticalAlign,
    pub object_fit: ObjectFit,
    pub alt_text: String,         // Fallback text if image fails
}

#[derive(Debug, Clone)]
pub enum ImageSource {
    Url(String),
    Data(Arc<[u8]>),
    Svg(Arc<str>),
    Placeholder(Size), // For layout without actual image
}

#[derive(Debug, Clone, Copy)]
pub enum VerticalAlign {
    Baseline,      // Align image baseline with text baseline
    Bottom,        // Align image bottom with line bottom  
    Top,           // Align image top with line top
    Middle,        // Align image middle with text middle
    TextTop,       // Align with tallest text in line
    TextBottom,    // Align with lowest text in line
    Sub,           // Subscript alignment
    Super,         // Superscript alignment
    Offset(f32),   // Custom offset from baseline
}

#[derive(Debug, Clone, Copy)]
pub enum ObjectFit {
    Fill,          // Stretch to fit display size
    Contain,       // Scale to fit within display size
    Cover,         // Scale to cover display size
    None,          // Use intrinsic size
    ScaleDown,     // Like contain but never scale up
}

#[derive(Debug, Clone)]
pub struct InlineShape {
    pub shape_def: ShapeDefinition,
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
    pub size: Size,
    pub baseline_offset: f32,
}

#[derive(Debug, Clone)]
pub struct InlineSpace {
    pub width: f32,
    pub is_breaking: bool,    // Can line break here
    pub is_stretchy: bool,    // Can be expanded for justification
}

#[derive(Debug, Clone)]
pub struct InlineBreak {
    pub break_type: BreakType,
    pub clear: ClearType,
}

#[derive(Debug, Clone, Copy)]
pub enum BreakType {
    Soft,      // Preferred break (like <wbr>)
    Hard,      // Forced break (like <br>)
    Page,      // Page break
    Column,    // Column break
}

#[derive(Debug, Clone, Copy)]
pub enum ClearType {
    None,
    Left,
    Right, 
    Both,
}

pub trait CustomInlineContent: std::fmt::Debug + Send + Sync {
    fn measure(&self, constraints: &InlineConstraints) -> InlineSize;
    fn render(&self, position: Point, size: Size) -> RenderCommand;
    fn baseline_offset(&self) -> f32;
    fn can_break_after(&self) -> bool;
}

// Complex shape constraints for non-rectangular text flow
#[derive(Debug, Clone)]
pub struct ShapeConstraints {
    pub boundaries: Vec<ShapeBoundary>,
    pub exclusions: Vec<ShapeExclusion>,
    pub writing_mode: WritingMode,
    pub text_align: TextAlign,
    pub line_height: f32,
}

#[derive(Debug, Clone)]
pub enum ShapeBoundary {
    Rectangle(Rect),
    Circle { center: Point, radius: f32 },
    Ellipse { center: Point, radii: Size },
    Polygon { points: Vec<Point> },
    Path { segments: Vec<PathSegment> },
    Custom(Box<dyn CustomShape>),
}

#[derive(Debug, Clone)]
pub enum ShapeExclusion {
    Rectangle(Rect),
    Circle { center: Point, radius: f32 },
    Ellipse { center: Point, radii: Size },
    Polygon { points: Vec<Point> },
    Path { segments: Vec<PathSegment> },
    Image { bounds: Rect, shape: ImageShape },
}

#[derive(Debug, Clone)]
pub enum ImageShape {
    Rectangle,                    // Normal rectangular image
    AlphaMask(Arc<[u8]>),        // Use alpha channel as exclusion mask
    VectorMask(Vec<PathSegment>), // Vector clipping path
}

pub trait CustomShape: std::fmt::Debug + Send + Sync {
    /// Get available width for a line at given y position and height
    fn line_constraints(&self, y: f32, line_height: f32) -> LineShapeConstraints;
    
    /// Check if a point is inside the shape
    fn contains_point(&self, point: Point) -> bool;
    
    /// Get the bounds of this shape
    fn bounds(&self) -> Rect;
}

#[derive(Debug, Clone)]
pub struct LineShapeConstraints {
    pub segments: Vec<LineSegment>,
    pub total_width: f32,
}

#[derive(Debug, Clone)]
pub struct LineSegment {
    pub start_x: f32,
    pub width: f32,
    pub priority: u8, // For choosing best segment when multiple available
}

// Enhanced layout constraints supporting arbitrary shapes
#[derive(Debug, Clone)]
pub struct AdvancedLayoutConstraints {
    pub shape: ShapeConstraints,
    pub justify_content: JustifyContent,
    pub vertical_align: VerticalAlign,
    pub overflow_behavior: OverflowBehavior,
}

#[derive(Debug, Clone, Copy)]
pub enum OverflowBehavior {
    Visible,       // Content extends outside shape
    Hidden,        // Content is clipped to shape
    Scroll,        // Scrollable overflow
    Auto,          // Browser/system decides
    Break,         // Break into next shape/page
}

// Shape-aware line breaking engine
#[derive(Debug)]
pub struct ShapeAwareLayoutEngine;

impl ShapeAwareLayoutEngine {
    pub fn layout_in_shape(
        content: Vec<InlineContent>,
        constraints: AdvancedLayoutConstraints,
        font_manager: &mut FontManager,
    ) -> Result<Arc<ShapedLayout>, LayoutError> {
        let mut shaped_content = Self::shape_inline_content(&content, font_manager)?;
        let lines = Self::fit_content_to_shape(&mut shaped_content, &constraints)?;
        let positioned_content = Self::position_content_in_lines(lines, &constraints)?;
        
        Ok(Arc::new(ShapedLayout {
            content: positioned_content,
            bounds: constraints.shape.boundaries.first()
                .map(|b| Self::shape_bounds(b))
                .unwrap_or_default(),
            overflow: Self::calculate_overflow(&positioned_content, &constraints),
        }))
    }

    fn shape_inline_content(
        content: &[InlineContent],
        font_manager: &mut FontManager,
    ) -> Result<Vec<ShapedInlineItem>, LayoutError> {
        let mut shaped_items = Vec::new();
        
        for item in content {
            match item {
                InlineContent::Text(run) => {
                    // Convert text runs to shaped glyphs (reuse existing text shaping)
                    let visual_runs = Self::analyze_text_run(run)?;
                    let glyphs = shape_visual_runs_with_fallback(&visual_runs, font_manager)?;
                    
                    for glyph in glyphs {
                        shaped_items.push(ShapedInlineItem::Glyph(glyph));
                    }
                }
                InlineContent::Image(img) => {
                    shaped_items.push(ShapedInlineItem::Image(Self::measure_image(img)?));
                }
                InlineContent::Shape(shape) => {
                    shaped_items.push(ShapedInlineItem::Shape(Self::measure_shape(shape)?));
                }
                InlineContent::Space(space) => {
                    shaped_items.push(ShapedInlineItem::Space(space.clone()));
                }
                InlineContent::LineBreak(br) => {
                    shaped_items.push(ShapedInlineItem::Break(br.clone()));
                }
                InlineContent::Custom(custom) => {
                    shaped_items.push(ShapedInlineItem::Custom(
                        custom.measure(&Default::default())
                    ));
                }
            }
        }
        
        Ok(shaped_items)
    }

    fn fit_content_to_shape(
        content: &mut [ShapedInlineItem],
        constraints: &AdvancedLayoutConstraints,
    ) -> Result<Vec<ShapedLine>, LayoutError> {
        let mut lines = Vec::new();
        let mut current_y = 0.0;
        let mut content_cursor = 0;
        
        while content_cursor < content.len() {
            // Get line constraints for current Y position
            let line_constraints = Self::get_line_constraints_for_shape(
                &constraints.shape,
                current_y,
                constraints.shape.line_height,
            )?;
            
            if line_constraints.segments.is_empty() {
                // No space available at this Y, move down
                current_y += constraints.shape.line_height;
                continue;
            }
            
            // Fit content to the best available segment(s)
            let (line_end, line_content) = Self::fit_line_to_segments(
                &content[content_cursor..],
                &line_constraints,
                constraints,
            )?;
            
            if line_content.is_empty() {
                // Nothing fits, move to next line or handle overflow
                if matches!(constraints.overflow_behavior, OverflowBehavior::Break) {
                    break;
                }
                current_y += constraints.shape.line_height;
                continue;
            }
            
            lines.push(ShapedLine {
                y: current_y,
                content: line_content,
                constraints: line_constraints,
                baseline_y: current_y + constraints.shape.line_height * 0.8,
            });
            
            current_y += constraints.shape.line_height;
            content_cursor += line_end;
        }
        
        Ok(lines)
    }

    fn get_line_constraints_for_shape(
        shape: &ShapeConstraints,
        y: f32,
        line_height: f32,
    ) -> Result<LineShapeConstraints, LayoutError> {
        let mut all_segments = Vec::new();
        
        // Process each boundary shape
        for boundary in &shape.boundaries {
            let boundary_segments = Self::get_boundary_segments(boundary, y, line_height)?;
            all_segments.extend(boundary_segments);
        }
        
        // Subtract exclusions
        for exclusion in &shape.exclusions {
            all_segments = Self::subtract_exclusion(all_segments, exclusion, y, line_height)?;
        }
        
        // Merge overlapping segments and sort by priority/position
        all_segments.sort_by(|a, b| {
            a.priority.cmp(&b.priority)
                .then_with(|| a.start_x.partial_cmp(&b.start_x).unwrap())
        });
        
        let merged_segments = Self::merge_segments(all_segments);
        let total_width = merged_segments.iter().map(|s| s.width).sum();
        
        Ok(LineShapeConstraints {
            segments: merged_segments,
            total_width,
        })
    }

    fn get_boundary_segments(
        boundary: &ShapeBoundary,
        y: f32,
        line_height: f32,
    ) -> Result<Vec<LineSegment>, LayoutError> {
        match boundary {
            ShapeBoundary::Rectangle(rect) => {
                if y >= rect.y && y + line_height <= rect.y + rect.height {
                    Ok(vec![LineSegment {
                        start_x: rect.x,
                        width: rect.width,
                        priority: 0,
                    }])
                } else {
                    Ok(vec![])
                }
            }
            
            ShapeBoundary::Circle { center, radius } => {
                Self::circle_line_intersection(*center, *radius, y, line_height)
            }
            
            ShapeBoundary::Ellipse { center, radii } => {
                Self::ellipse_line_intersection(*center, *radii, y, line_height)
            }
            
            ShapeBoundary::Polygon { points } => {
                Self::polygon_line_intersection(points, y, line_height)
            }
            
            ShapeBoundary::Path { segments } => {
                Self::path_line_intersection(segments, y, line_height)
            }
            
            ShapeBoundary::Custom(shape) => {
                Ok(shape.line_constraints(y, line_height).segments)
            }
        }
    }

    fn circle_line_intersection(
        center: Point,
        radius: f32,
        y: f32,
        line_height: f32,
    ) -> Result<Vec<LineSegment>, LayoutError> {
        let line_center_y = y + line_height / 2.0;
        let dy = (line_center_y - center.y).abs();
        
        if dy > radius {
            return Ok(vec![]); // Line doesn't intersect circle
        }
        
        // Calculate intersection width using Pythagorean theorem
        let half_width = (radius * radius - dy * dy).sqrt();
        
        Ok(vec![LineSegment {
            start_x: center.x - half_width,
            width: half_width * 2.0,
            priority: 0,
        }])
    }

    fn ellipse_line_intersection(
        center: Point,
        radii: Size,
        y: f32,
        line_height: f32,
    ) -> Result<Vec<LineSegment>, LayoutError> {
        let line_center_y = y + line_height / 2.0;
        let dy = line_center_y - center.y;
        
        if dy.abs() > radii.height {
            return Ok(vec![]);
        }
        
        // Ellipse equation: (x-h)²/a² + (y-k)²/b² = 1
        // Solve for x: x = h ± a*sqrt(1 - (y-k)²/b²)
        let normalized_y = dy / radii.height;
        let x_factor = (1.0 - normalized_y * normalized_y).sqrt();
        let half_width = radii.width * x_factor;
        
        Ok(vec![LineSegment {
            start_x: center.x - half_width,
            width: half_width * 2.0,
            priority: 0,
        }])
    }

    fn polygon_line_intersection(
        points: &[Point],
        y: f32,
        line_height: f32,
    ) -> Result<Vec<LineSegment>, LayoutError> {
        if points.len() < 3 {
            return Ok(vec![]);
        }
        
        let mut intersections = Vec::new();
        let line_center_y = y + line_height / 2.0;
        
        // Find all intersections with polygon edges
        for i in 0..points.len() {
            let p1 = points[i];
            let p2 = points[(i + 1) % points.len()];
            
            // Check if line intersects this edge
            if (p1.y <= line_center_y && p2.y >= line_center_y) ||
               (p1.y >= line_center_y && p2.y <= line_center_y) {
                
                if (p2.y - p1.y).abs() < f32::EPSILON {
                    // Horizontal edge
                    continue;
                }
                
                // Calculate intersection x coordinate
                let t = (line_center_y - p1.y) / (p2.y - p1.y);
                let x = p1.x + t * (p2.x - p1.x);
                intersections.push(x);
            }
        }
        
        // Sort intersections and pair them up
        intersections.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let mut segments = Vec::new();
        for chunk in intersections.chunks(2) {
            if chunk.len() == 2 {
                let start_x = chunk[0];
                let end_x = chunk[1];
                if end_x > start_x {
                    segments.push(LineSegment {
                        start_x,
                        width: end_x - start_x,
                        priority: 0,
                    });
                }
            }
        }
        
        Ok(segments)
    }

    fn path_line_intersection(
        segments: &[PathSegment],
        y: f32,
        line_height: f32,
    ) -> Result<Vec<LineSegment>, LayoutError> {
        // This would implement intersection with Bezier curves, arcs, etc.
        // For now, simplified to bounding box
        unimplemented!("Path intersection - requires curve math")
    }

    fn fit_line_to_segments(
        content: &[ShapedInlineItem],
        line_constraints: &LineShapeConstraints,
        layout_constraints: &AdvancedLayoutConstraints,
    ) -> Result<(usize, Vec<PositionedInlineItem>), LayoutError> {
        if line_constraints.segments.is_empty() {
            return Ok((0, vec![]));
        }
        
        // Choose the best segment (largest, highest priority)
        let best_segment = line_constraints.segments
            .iter()
            .max_by(|a, b| {
                a.priority.cmp(&b.priority)
                    .then_with(|| a.width.partial_cmp(&b.width).unwrap())
            })
            .unwrap();
        
        // Fit content to this segment
        let mut current_width = 0.0;
        let mut fitted_content = Vec::new();
        let mut content_end = 0;
        
        for (i, item) in content.iter().enumerate() {
            let item_width = Self::get_item_width(item);
            
            if current_width + item_width > best_segment.width {
                // Try to break here
                if Self::can_break_before(item) {
                    break;
                }
                
                // Check for hyphenation opportunities
                if let Some(hyphenated) = Self::try_hyphenation(item, best_segment.width - current_width) {
                    fitted_content.push(Self::position_item(
                        hyphenated,
                        best_segment.start_x + current_width,
                        0.0, // Y will be set by line
                    ));
                    content_end = i + 1;
                    break;
                }
                
                // Force break
                break;
            }
            
            fitted_content.push(Self::position_item(
                item.clone(),
                best_segment.start_x + current_width,
                0.0,
            ));
            
            current_width += item_width;
            content_end = i + 1;
            
            if Self::is_hard_break(item) {
                break;
            }
        }
        
        // Apply justification within the segment
        if layout_constraints.justify_content != JustifyContent::None {
            Self::justify_line_content(&mut fitted_content, best_segment.width, current_width)?;
        }
        
        Ok((content_end, fitted_content))
    }
    
    // ... Additional helper methods for shape intersection math, positioning, etc.
}

// Enhanced content representation for shaped layout
#[derive(Debug, Clone)]
pub enum ShapedInlineItem {
    Glyph(ShapedGlyph),
    Image(MeasuredImage),
    Shape(MeasuredShape),
    Space(InlineSpace),
    Break(InlineBreak),
    Custom(InlineSize),
}

#[derive(Debug, Clone)]
pub struct MeasuredImage {
    pub source: ImageSource,
    pub size: Size,
    pub baseline_offset: f32,
    pub alignment: VerticalAlign,
}

#[derive(Debug, Clone)]
pub struct MeasuredShape {
    pub shape_def: ShapeDefinition,
    pub size: Size,
    pub baseline_offset: f32,
}

#[derive(Debug, Clone)]
pub struct InlineSize {
    pub width: f32,
    pub height: f32,
    pub baseline_offset: f32,
}

#[derive(Debug, Clone)]
pub struct PositionedInlineItem {
    pub content: ShapedInlineItem,
    pub position: Point,
    pub bounds: Rect,
}

#[derive(Debug, Clone)]
pub struct ShapedLine {
    pub y: f32,
    pub content: Vec<PositionedInlineItem>,
    pub constraints: LineShapeConstraints,
    pub baseline_y: f32,
}

#[derive(Debug, Clone)]
pub struct ShapedLayout {
    pub content: Vec<ShapedLine>,
    pub bounds: Rect,
    pub overflow: OverflowInfo,
}

#[derive(Debug, Clone)]
pub struct OverflowInfo {
    pub has_overflow: bool,
    pub overflow_bounds: Option<Rect>,
    pub clipped_content: Vec<ShapedInlineItem>,
}

// Path and shape definitions
#[derive(Debug, Clone)]
pub enum PathSegment {
    MoveTo(Point),
    LineTo(Point),
    CurveTo { control1: Point, control2: Point, end: Point },
    QuadTo { control: Point, end: Point },
    Arc { center: Point, radius: f32, start_angle: f32, end_angle: f32 },
    Close,
}

#[derive(Debug, Clone)]
pub enum ShapeDefinition {
    Rectangle { size: Size, corner_radius: Option<f32> },
    Circle { radius: f32 },
    Ellipse { radii: Size },
    Polygon { points: Vec<Point> },
    Path { segments: Vec<PathSegment> },
}

#[derive(Debug, Clone)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
    pub dash_pattern: Option<Vec<f32>>,
}

// Usage examples and integration
impl AdvancedLayoutEngine {
    /// Layout text in a circle (Instagram story style)
    pub fn layout_text_in_circle(
        text: &str,
        center: Point,
        radius: f32,
        font: FontRef,
        font_size: f32,
    ) -> Result<Arc<ShapedLayout>, LayoutError> {
        let content = vec![InlineContent::Text(StyledRun {
            text: text.to_string(),
            style: StyleProperties {
                font_ref: font,
                font_size_px: font_size,
                ..Default::default()
            },
            logical_start_byte: 0,
        })];

        let constraints = AdvancedLayoutConstraints {
            shape: ShapeConstraints {
                boundaries: vec![ShapeBoundary::Circle { center, radius }],
                exclusions: vec![],
                writing_mode: WritingMode::HorizontalTb,
                text_align: TextAlign::Center,
                line_height: font_size * 1.2,
            },
            justify_content: JustifyContent::InterWord,
            vertical_align: VerticalAlign::Middle,
            overflow_behavior: OverflowBehavior::Hidden,
        };

        let mut font_manager = FontManager::new()?;
        ShapeAwareLayoutEngine::layout_in_shape(content, constraints, &mut font_manager)
    }

    /// Layout text with inline image (like a document)
    pub fn layout_mixed_content_in_rectangle(
        content: Vec<InlineContent>,
        bounds: Rect,
        align: TextAlign,
    ) -> Result<Arc<ShapedLayout>, LayoutError> {
        let constraints = AdvancedLayoutConstraints {
            shape: ShapeConstraints {
                boundaries: vec![ShapeBoundary::Rectangle(bounds)],
                exclusions: vec![],
                writing_mode: WritingMode::HorizontalTb,
                text_align: align,
                line_height: 16.0 * 1.4,
            },
            justify_content: JustifyContent::InterWord,
            vertical_align: VerticalAlign::Baseline,
            overflow_behavior: OverflowBehavior::Visible,
        };

        let mut font_manager = FontManager::new()?;
        ShapeAwareLayoutEngine::layout_in_shape(content, constraints, &mut font_manager)
    }

    /// Layout text flowing around an image (magazine style)
    pub fn layout_text_around_image(
        text: &str,
        image_bounds: Rect,
        container_bounds: Rect,
        font: FontRef,
    ) -> Result<Arc<ShapedLayout>, LayoutError> {
        let content = vec![InlineContent::Text(StyledRun {
            text: text.to_string(),
            style: StyleProperties {
                font_ref: font,
                font_size_px: 16.0,
                ..Default::default()
            },
            logical_start_byte: 0,
        })];

        let constraints = AdvancedLayoutConstraints {
            shape: ShapeConstraints {
                boundaries: vec![ShapeBoundary::Rectangle(container_bounds)],
                exclusions: vec![ShapeExclusion::Rectangle(image_bounds)],
                writing_mode: WritingMode::HorizontalTb,
                text_align: TextAlign::Justify,
                line_height: 16.0 * 1.4,
            },
            justify_content: JustifyContent::InterWord,
            vertical_align: VerticalAlign::Baseline,
            overflow_behavior: OverflowBehavior::Visible,
        };

        let mut font_manager = FontManager::new()?;
        ShapeAwareLayoutEngine::layout_in_shape(content, constraints, &mut font_manager)
    }
}

// Integration with existing layout system
impl From<ParagraphLayout> for ShapedLayout {
    fn from(para: ParagraphLayout) -> Self {
        let lines = para.lines.into_iter()
            .map(|line| ShapedLine {
                y: line.bounds.y,
                content: para.glyphs[line.glyph_start..line.glyph_start + line.glyph_count]
                    .iter()
                    .map(|g| PositionedInlineItem {
                        content: ShapedInlineItem::Glyph(ShapedGlyph {
                            glyph_id: g.glyph_id,
                            style: g.style.clone(),
                            advance: g.advance,
                            // ... convert other fields
                            ..Default::default()
                        }),
                        position: Point { x: g.x, y: g.y },
                        bounds: g.bounds,
                    })
                    .collect(),
                constraints: LineShapeConstraints {
                    segments: vec![LineSegment {
                        start_x: line.bounds.x,
                        width: line.bounds.width,
                        priority: 0,
                    }],
                    total_width: line.bounds.width,
                },
                baseline_y: line.baseline_y,
            })
            .collect();

        ShapedLayout {
            content: lines,
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: para.content_size.width,
                height: para.content_size.height,
            },
            overflow: OverflowInfo {
                has_overflow: false,
                overflow_bounds: None,
                clipped_content: vec![],
            },
        }
    }
}