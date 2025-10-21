//! Convert DisplayList to PDF operations.
//!
//! This module converts azul's DisplayList into an intermediate PDF operation format
//! that can be consumed by any PDF library without creating a dependency.

use azul_core::{geom::LogicalSize, resources::ImageKey};

use super::{
    pdf_ops::{FontId, PdfColor, PdfOp, PdfPoint, PdfTextMatrix, TextItem},
    resources::PdfRenderResources,
};
use crate::{
    solver3::display_list::{DisplayList, DisplayListItem},
    text3::cache::{FontRef, ParsedFontTrait},
};

/// Result of converting a display list to PDF operations.
#[derive(Debug, Clone)]
pub struct PdfPageRender {
    /// The PDF operations to render this page
    pub ops: Vec<PdfOp>,

    /// Resources (fonts, images) needed for this page
    pub resources: PdfRenderResources,

    /// The page size
    pub page_size: LogicalSize,
}

/// Convert a display list to PDF rendering operations.
///
/// # Arguments
/// * `display_list` - The display list to convert
/// * `page_size` - The size of the page in logical units (points)
///
/// # Returns
/// A `PdfPageRender` containing the PDF operations and resource information.
pub fn display_list_to_pdf_ops(
    display_list: &DisplayList,
    page_size: LogicalSize,
) -> PdfPageRender {
    let mut ops = Vec::new();
    let mut resources = PdfRenderResources::new();

    // PDF uses bottom-left origin, azul uses top-left
    // We'll add a transform to flip the Y axis
    let page_height = page_size.height;

    for item in &display_list.items {
        convert_display_list_item(item, &mut ops, &mut resources, page_height);
    }

    PdfPageRender {
        ops,
        resources,
        page_size,
    }
}

fn convert_display_list_item(
    item: &DisplayListItem,
    ops: &mut Vec<PdfOp>,
    resources: &mut PdfRenderResources,
    page_height: f32,
) {
    match item {
        DisplayListItem::Rect {
            bounds,
            color,
            border_radius,
        } => {
            // Convert rectangle to PDF path
            ops.push(PdfOp::SaveState);

            let y = page_height - bounds.origin.y - bounds.size.height;

            if border_radius.is_zero() {
                // Simple rectangle
                ops.push(PdfOp::BeginPath);
                ops.push(PdfOp::MoveTo {
                    point: PdfPoint::new(bounds.origin.x, y),
                });
                ops.push(PdfOp::LineTo {
                    point: PdfPoint::new(bounds.origin.x + bounds.size.width, y),
                });
                ops.push(PdfOp::LineTo {
                    point: PdfPoint::new(
                        bounds.origin.x + bounds.size.width,
                        y + bounds.size.height,
                    ),
                });
                ops.push(PdfOp::LineTo {
                    point: PdfPoint::new(bounds.origin.x, y + bounds.size.height),
                });
                ops.push(PdfOp::ClosePath);
            } else {
                // Rounded rectangle - simplified, just use regular rect for now
                // TODO: Implement proper rounded corners with curves
                ops.push(PdfOp::BeginPath);
                ops.push(PdfOp::MoveTo {
                    point: PdfPoint::new(bounds.origin.x, y),
                });
                ops.push(PdfOp::LineTo {
                    point: PdfPoint::new(bounds.origin.x + bounds.size.width, y),
                });
                ops.push(PdfOp::LineTo {
                    point: PdfPoint::new(
                        bounds.origin.x + bounds.size.width,
                        y + bounds.size.height,
                    ),
                });
                ops.push(PdfOp::LineTo {
                    point: PdfPoint::new(bounds.origin.x, y + bounds.size.height),
                });
                ops.push(PdfOp::ClosePath);
            }

            ops.push(PdfOp::SetFillColor {
                color: PdfColor::from(*color),
            });
            ops.push(PdfOp::Fill);

            ops.push(PdfOp::RestoreState);
        }

        DisplayListItem::Text {
            glyphs,
            font,
            color,
            clip_rect,
        } => {
            // Text rendering
            ops.push(PdfOp::BeginText);

            // Register the font
            resources.register_font(font.clone());

            // Note: This is a simplified version. Real implementation would need to:
            // 1. Position individual glyphs based on their positions
            // 2. Handle font transformations
            // 3. Apply clipping

            ops.push(PdfOp::SetFillColor {
                color: PdfColor::from(*color),
            });

            ops.push(PdfOp::EndText);
        }

        DisplayListItem::Image { bounds, key } => {
            // Image rendering
            resources.register_image(*key);

            // Note: Actual image embedding would be handled by the PDF library
            // This just marks where the image should be placed
        }

        DisplayListItem::Border {
            bounds,
            widths,
            colors,
            styles,
            border_radius,
        } => {
            // Simplified: Use top border as representative for PDF rendering
            use azul_css::css::CssPropertyValue;

            let width = widths
                .top
                .and_then(|w| w.get_property().cloned())
                .map(|w| w.inner.to_pixels(0.0))
                .unwrap_or(0.0);

            let color = colors
                .top
                .and_then(|c| c.get_property().cloned())
                .map(|c| c.inner)
                .unwrap_or(ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                });

            // Border rendering
            ops.push(PdfOp::SaveState);

            let y = page_height - bounds.origin.y - bounds.size.height;

            // Simplified border - just draw a stroked rectangle
            ops.push(PdfOp::BeginPath);
            ops.push(PdfOp::MoveTo {
                point: PdfPoint::new(bounds.origin.x, y),
            });
            ops.push(PdfOp::LineTo {
                point: PdfPoint::new(bounds.origin.x + bounds.size.width, y),
            });
            ops.push(PdfOp::LineTo {
                point: PdfPoint::new(bounds.origin.x + bounds.size.width, y + bounds.size.height),
            });
            ops.push(PdfOp::LineTo {
                point: PdfPoint::new(bounds.origin.x, y + bounds.size.height),
            });
            ops.push(PdfOp::ClosePath);

            ops.push(PdfOp::SetLineWidth { width });
            ops.push(PdfOp::SetStrokeColor {
                color: PdfColor::from(color),
            });
            ops.push(PdfOp::Stroke);

            ops.push(PdfOp::RestoreState);
        }

        _ => {
            // Other display list items not yet implemented
        }
    }
}
