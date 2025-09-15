use std::sync::Arc;
use std::error::Error;

// Re-creating the necessary imports and structs from the provided context.
use rust_fontconfig::{FcFont, FcFontCache, FcPattern, FcWeight, UnicodeRange};
use azul_core::app_resources::GlyphOutlineOperation;
use azul_layout::{
    parsedfont::ParsedFont,
    text3::{
        Color, FontRef, FontStyle, InlineContent, LayoutCache, ParsedFontTrait,
        Point, ShapeBoundary, ShapeExclusion, ShapedItem, StyleProperties, StyledRun, 
        TextDecoration, TextOrientation, UnifiedConstraints, UnifiedLayout, 
        UnifiedLayoutEngine, WritingMode, FontManager,
        default::PathLoader,
    },
};

/// Main function to set up and run the Mongolian text layout example.
fn main() -> Result<(), Box<dyn Error>> {
    render_mongolian_vertical_example()?;
    Ok(())
}

/// Main rendering example function.
///
/// This function sets up the font cache with an in-memory Mongolian font, defines the text
/// and styling, specifies a circular layout, performs the text layout, and renders the
/// result to a PNG image.
pub fn render_mongolian_vertical_example() -> Result<(), Box<dyn Error>> {
    // 1. Include the Mongolian font bytes directly into the binary.
    const MONGO_FONT_BYTES: &[u8] = include_bytes!("NotoSansMongolian-Regular.ttf");
    const MONGO_FONT_FAMILY_NAME: &str = "Noto Sans Mongolian";

    // 2. Create a FontSource and a pattern for the in-memory font.
    let font_source = FcFont {
        bytes: MONGO_FONT_BYTES.into(),
        font_index: 0,
        id: MONGO_FONT_FAMILY_NAME.to_string(),
    };
    let mut pattern = FcPattern::default();
    pattern.family = Some(MONGO_FONT_FAMILY_NAME.to_string());
    pattern.weight = FcWeight::Normal;

    // 3. Configure FcFontCache to use the provided in-memory font.
    let mut fc_cache = FcFontCache::default();
    fc_cache.with_memory_fonts(vec![(pattern, font_source)]);

    // 4. Set up the FontManager with the real ParsedFont and PathLoader.
    let font_manager = FontManager::<ParsedFont, PathLoader>::with_loader(
        fc_cache,
        Arc::new(PathLoader::new()),
    )?;

    // 5. Create Mongolian text content.
    let mongolian_texts = vec![
        "ᠮᠣᠩᠭᠣᠯ ᠬᠡᠯᠡ",        // "Mongolian Language"
        "垂直文本",          // some CJK to test font fallback
        "Hello",            // some Latin
        "ᠶᠡᠬᠡ ᠮᠣᠩᠭᠣᠯ ᠤᠯᠤᠰ",  // "Great Mongol Nation"
    ];

    // 6. Create styled runs for the layout engine.
    let mut content = Vec::new();
    let mut byte_offset = 0;

    for text in mongolian_texts {
        content.push(InlineContent::Text(StyledRun {
            text: text.to_string(),
            logical_start_byte: byte_offset,
            style: Arc::new(StyleProperties {
                font_ref: FontRef {
                    family: MONGO_FONT_FAMILY_NAME.to_string(),
                    weight: FcWeight::Normal,
                    style: FontStyle::Normal,
                    // Define the Unicode range for Mongolian script
                    unicode_ranges: vec![UnicodeRange { start: 0x1800, end: 0x18AF }],
                },
                font_size_px: 24.0,
                color: Color { r: 0, g: 0, b: 0, a: 255 },
                letter_spacing: 1.0,
                word_spacing: 12.0,
                line_height: 32.0,
                text_decoration: TextDecoration::default(),
                // "vert" feature is crucial for correct vertical Mongolian glyphs
                font_features: vec!["vert".to_string()],
                // Vertical, left-to-right writing mode for Mongolian
                writing_mode: WritingMode::VerticalLr,
                text_orientation: TextOrientation::Upright,
                text_combine_upright: None,
            }),
        }));
        byte_offset += text.len();
    }

    // 7. Define the layout constraints for a circular/ring shape.
    let constraints = UnifiedConstraints {
        shape_boundaries: vec![
            ShapeBoundary::Circle {
                center: Point { x: 400.0, y: 400.0 },
                radius: 350.0,
            }
        ],
        shape_exclusions: vec![
            ShapeExclusion::Circle {
                center: Point { x: 400.0, y: 400.0 },
                radius: 150.0,
            }
        ],
        // The available width/height should encompass the shape
        available_width: 800.0,
        available_height: Some(800.0),
        writing_mode: Some(WritingMode::VerticalLr),
        text_orientation: TextOrientation::Upright,
        ..Default::default()
    };
    
    // 8. Create a cache for layout results.
    let cache = LayoutCache::new(10);
    
    // 9. Perform the layout.
    println!("Performing layout...");
    let layout = UnifiedLayoutEngine::layout(
        content,
        constraints,
        &font_manager,
        &cache,
    )?;
    
    // 10. Render the layout to a PNG bitmap.
    println!("Rendering layout to PNG...");
    render_layout_to_png(&layout, "mongolian_circle.png")?;
    
    println!("Successfully rendered Mongolian text in a circular layout to mongolian_circle.png!");
    println!("Layout bounds: {:?}", layout.bounds);
    println!("Total items positioned: {}", layout.items.len());
    
    Ok(())
}

/// Renders a `UnifiedLayout` to a PNG file using `tiny-skia`.
///
/// This function iterates through the positioned glyphs in the layout, converts their
/// vector outlines to `tiny-skia` paths, and renders them onto a pixmap, which is then
/// saved as a PNG image.
fn render_layout_to_png(
    layout: &UnifiedLayout<ParsedFont>,
    output_path: &str,
) -> Result<(), Box<dyn Error>> {

    use image::{ImageBuffer, Rgba};
    use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};

    const IMG_WIDTH: u32 = 800;
    const IMG_HEIGHT: u32 = 800;

    let mut pixmap = Pixmap::new(IMG_WIDTH, IMG_HEIGHT)
        .ok_or("Failed to create a pixmap")?;
    
    // Fill background with white
    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));
    
    // Render each positioned glyph from the layout result
    for item in &layout.items {
        if let ShapedItem::Glyph(glyph) = &item.item {
            // Get font metrics to calculate scaling
            let font_metrics = glyph.font.get_font_metrics();
            let scale_factor = if font_metrics.units_per_em > 0 {
                glyph.style.font_size_px / (font_metrics.units_per_em as f32)
            } else {
                0.01 // Default scale to avoid division by zero
            };

            // Get the glyph's vector outline from the parsed font data
            if let Some(owned_glyph) = glyph.font.glyph_records_decoded.get(&glyph.glyph_id) {
                for outline in &owned_glyph.outline {
                    let mut pb = PathBuilder::new();
                    let mut is_first = true;
                    // Convert the glyph's outline operations into a tiny-skia path
                    for op in outline.operations.as_ref() {
                        match op {
                            GlyphOutlineOperation::MoveTo(pt) => {
                                if is_first {
                                    pb.move_to(pt.x as f32, pt.y as f32);
                                    is_first = false;
                                } else {
                                    pb.line_to(pt.x as f32, pt.y as f32);
                                }
                            }
                            GlyphOutlineOperation::LineTo(pt) => pb.line_to(pt.x as f32, pt.y as f32),
                            GlyphOutlineOperation::QuadraticCurveTo(q) => {
                                pb.quad_to(q.ctrl_1_x as f32, q.ctrl_1_y as f32, q.end_x as f32, q.end_y as f32)
                            }
                            GlyphOutlineOperation::CubicCurveTo(c) => {
                                pb.cubic_to(c.ctrl_1_x as f32, c.ctrl_1_y as f32, c.ctrl_2_x as f32, c.ctrl_2_y as f32, c.end_x as f32, c.end_y as f32)
                            }
                            GlyphOutlineOperation::ClosePath => pb.close(),
                        }
                    }

                    if let Some(path) = pb.finish() {
                        let mut paint = Paint::default();
                        paint.set_color_rgba8(glyph.style.color.r, glyph.style.color.g, glyph.style.color.b, glyph.style.color.a);
                        paint.anti_alias = true;

                        // Create the transformation matrix for the glyph:
                        // 1. Translate to the final position on the canvas.
                        // 2. Translate upwards by the font's ascent to align the baseline correctly.
                        // 3. Scale the glyph from font units to pixels (and flip the Y-axis).
                        let ascent = font_metrics.ascent;
                        let transform = Transform::from_translate(item.position.x, item.position.y)
                            .pre_translate(0.0, ascent * scale_factor)
                            .pre_scale(scale_factor, -scale_factor);

                        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
                    }
                }
            }
        }
    }

    // Save the pixmap data to a PNG file using the image crate
    let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_raw(IMG_WIDTH, IMG_HEIGHT, pixmap.take())
        .ok_or("Failed to create image buffer from pixmap data")?;
    
    img.save(output_path)?;

    Ok(())
}