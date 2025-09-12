// replace: get_available_width_for_line

/// Calculates the available horizontal segments for a line at a given vertical position,
/// considering both shape boundaries and exclusions.
fn get_line_constraints(
    line_y: f32,
    line_height: f32, // The height of the line is needed for accurate intersection tests
    constraints: &UnifiedConstraints,
) -> Result<LineConstraints, LayoutError> {
    // 1. Determine the initial available segments from the boundaries.
    let mut available_segments = Vec::new();

    if constraints.shape_boundaries.is_empty() {
        // Fallback to simple rectangular available_width if no complex shapes are defined.
        available_segments.push(LineSegment {
            start_x: 0.0,
            width: constraints.available_width,
            priority: 0,
        });
    } else {
        // Get segments from all defined boundaries.
        for boundary in &constraints.shape_boundaries {
            let boundary_spans = get_shape_horizontal_spans(boundary, line_y, line_height)?;
            for (start, end) in boundary_spans {
                available_segments.push(LineSegment {
                    start_x: start,
                    width: end - start,
                    priority: 0,
                });
            }
        }
        // Merge potentially overlapping segments from different boundary shapes.
        available_segments = merge_segments(available_segments);
    }

    // 2. Iteratively subtract each exclusion from the current set of available segments.
    for exclusion in &constraints.shape_exclusions {
        let exclusion_spans = get_shape_horizontal_spans(exclusion, line_y, line_height)?;
        if exclusion_spans.is_empty() {
            continue; // This exclusion is not on the current line.
        }

        let mut next_segments = Vec::new();
        for (excl_start, excl_end) in exclusion_spans {
            // Apply this exclusion span to all current segments.
            for segment in &available_segments {
                let seg_start = segment.start_x;
                let seg_end = segment.start_x + segment.width;

                // Case 1: The segment is entirely to the left of the exclusion.
                if seg_end <= excl_start {
                    next_segments.push(segment.clone());
                    continue;
                }
                // Case 2: The segment is entirely to the right of the exclusion.
                if seg_start >= excl_end {
                    next_segments.push(segment.clone());
                    continue;
                }

                // Case 3: The segment is split by the exclusion.
                if seg_start < excl_start && seg_end > excl_end {
                    // Left part
                    next_segments.push(LineSegment {
                        start_x: seg_start,
                        width: excl_start - seg_start,
                        priority: segment.priority,
                    });
                    // Right part
                    next_segments.push(LineSegment {
                        start_x: excl_end,
                        width: seg_end - excl_end,
                        priority: segment.priority,
                    });
                    continue;
                }

                // Case 4: The exclusion truncates the right side of the segment.
                if seg_start < excl_start {
                     next_segments.push(LineSegment {
                        start_x: seg_start,
                        width: excl_start - seg_start,
                        priority: segment.priority,
                    });
                }

                // Case 5: The exclusion truncates the left side of the segment.
                if seg_end > excl_end {
                     next_segments.push(LineSegment {
                        start_x: excl_end,
                        width: seg_end - excl_end,
                        priority: segment.priority,
                    });
                }
                
                // Case 6 (Implicit): The segment is completely contained within the exclusion.
                // In this case, nothing is added to next_segments.
            }
            // The result of this exclusion becomes the input for the next one.
            available_segments = merge_segments(next_segments);
            next_segments = Vec::new();
        }
    }

    let total_width = available_segments.iter().map(|s| s.width).sum();

    Ok(LineConstraints {
        segments: available_segments,
        total_available: total_width,
    })
}

/// Helper function to get the horizontal spans of any shape at a given y-coordinate.
/// Returns a list of (start_x, end_x) tuples.
fn get_shape_horizontal_spans<S: ShapeLike>(
    shape: &S,
    y: f32,
    line_height: f32,
) -> Result<Vec<(f32, f32)>, LayoutError> {
    let line_top = y;
    let line_bottom = y + line_height;
    
    // For simplicity in intersection, we can test against the center of the line.
    // A more advanced implementation might check the entire band [line_top, line_bottom].
    let line_center_y = y + line_height / 2.0;

    match shape.as_enum() {
        ShapeEnum::Rectangle(rect) => {
            if line_bottom > rect.y && line_top < rect.y + rect.height {
                Ok(vec![(rect.x, rect.x + rect.width)])
            } else {
                Ok(vec![])
            }
        }
        ShapeEnum::Circle { center, radius } => {
            let dy = (line_center_y - center.y).abs();
            if dy <= *radius {
                let dx = (radius.powi(2) - dy.powi(2)).sqrt();
                Ok(vec![(center.x - dx, center.x + dx)])
            } else {
                Ok(vec![])
            }
        }
        ShapeEnum::Polygon { points } => {
            // Assumes a polygon_line_intersection function exists as in the original context.
            let segments = polygon_line_intersection(points, y, line_height)?;
            Ok(segments.iter().map(|s| (s.start_x, s.start_x + s.width)).collect())
        }
        // Add Ellipse, Path, etc. cases here
        _ => unimplemented!("Shape type not yet supported for line intersection"),
    }
}

/// Merges overlapping or adjacent line segments into larger ones.
fn merge_segments(mut segments: Vec<LineSegment>) -> Vec<LineSegment> {
    if segments.len() <= 1 {
        return segments;
    }
    segments.sort_by(|a, b| a.start_x.partial_cmp(&b.start_x).unwrap());
    let mut merged = vec![segments[0].clone()];
    for next_seg in segments.iter().skip(1) {
        let last = merged.last_mut().unwrap();
        if next_seg.start_x <= last.start_x + last.width {
            let new_width = (next_seg.start_x + next_seg.width) - last.start_x;
            last.width = last.width.max(new_width);
        } else {
            merged.push(next_seg.clone());
        }
    }
    merged
}


// --- Helper traits to avoid duplicating the get_shape_horizontal_spans logic ---

enum ShapeEnum<'a> {
    Rectangle(&'a Rect),
    Circle { center: &'a Point, radius: &'a f32 },
    Ellipse { center: &'a Point, radii: &'a Size },
    Polygon { points: &'a Vec<Point> },
    Path { segments: &'a Vec<PathSegment> },
    Image { bounds: &'a Rect, shape: &'a ImageShape },
}

trait ShapeLike {
    fn as_enum(&self) -> ShapeEnum;
}

impl ShapeLike for ShapeBoundary {
    fn as_enum(&self) -> ShapeEnum {
        match self {
            ShapeBoundary::Rectangle(r) => ShapeEnum::Rectangle(r),
            ShapeBoundary::Circle { center, radius } => ShapeEnum::Circle { center, radius },
            ShapeBoundary::Polygon { points } => ShapeEnum::Polygon { points },
            _ => unimplemented!(),
        }
    }
}

impl ShapeLike for ShapeExclusion {
    fn as_enum(&self) -> ShapeEnum {
        match self {
            ShapeExclusion::Rectangle(r) => ShapeEnum::Rectangle(r),
            ShapeExclusion::Circle { center, radius } => ShapeEnum::Circle { center, radius },
            ShapeExclusion::Polygon { points } => ShapeEnum::Polygon { points },
            _ => unimplemented!(),
        }
    }
}

// Dummy polygon function to make it compile
fn polygon_line_intersection(points: &[Point], y: f32, line_height: f32) -> Result<Vec<LineSegment>, LayoutError> { Ok(vec![]) }