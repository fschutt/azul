//! Test for list item line wrapping behavior
//!
//! This test verifies proper line breaking behavior when there is sufficient space.
//!
//! Bug: Text was wrapping prematurely even when there was enough horizontal space
//! for the remaining words to fit on the same line.

use azul_css::props::basic::pixel::DEFAULT_FONT_SIZE;
use azul_layout::text3::{
    cache::{GlyphKind, ParsedFontTrait, Point, ShapedCluster, ShapedItem, UnifiedConstraints},
    knuth_plass::kp_layout,
};

/// Mock font for testing
#[derive(Debug, Clone)]
struct MockFont;

impl ParsedFontTrait for MockFont {
    fn shallow_clone(&self) -> Self {
        MockFont
    }

    fn get_glyph_index(&self, _codepoint: u32) -> Option<u16> {
        Some(1)
    }

    fn get_advance_width(&self, _glyph_index: u16) -> i16 {
        10 // Each character is 10 units wide
    }

    fn get_units_per_em(&self) -> u16 {
        1000
    }
}

#[test]
fn test_no_premature_line_break_with_sufficient_space() {
    // Simulate the text "Basic text formatting with paragraphs and headings"
    // At font-size 16px with 10 units per character, this is roughly:
    // 52 characters * 10 units/char * (16/1000) = ~8.3px per char = ~431px total

    // This should easily fit in 555px width (the width paragraphs get)
    // But list items were getting only 363px, causing premature wrapping

    let text = "Basic text formatting with paragraphs and headings";
    let mut items = Vec::new();

    for (i, ch) in text.chars().enumerate() {
        if ch == ' ' {
            // Space character - this is where line breaks can occur
            items.push(ShapedItem::Cluster(ShapedCluster {
                glyph: azul_layout::text3::cache::ShapedGlyph {
                    codepoint: ' ' as u32,
                    glyph_index: 1,
                    advance: 10.0,
                    offset_x: 0.0,
                    offset_y: 0.0,
                },
                advance: 10.0,
                item_kind: GlyphKind::Whitespace,
                position: Point { x: 0.0, y: 0.0 },
            }));
        } else {
            // Regular character
            items.push(ShapedItem::Cluster(ShapedCluster {
                glyph: azul_layout::text3::cache::ShapedGlyph {
                    codepoint: ch as u32,
                    glyph_index: 1,
                    advance: 10.0,
                    offset_x: 0.0,
                    offset_y: 0.0,
                },
                advance: 10.0,
                item_kind: GlyphKind::Normal,
                position: Point { x: 0.0, y: 0.0 },
            }));
        }
    }

    // Test 1: With sufficient width (555px like paragraphs), text should fit on one line
    let constraints_wide = UnifiedConstraints {
        available_width: 555.0,
        ..Default::default()
    };

    let logical_items = vec![]; // Not needed for this basic test
    let layout_wide = kp_layout(&items, &logical_items, &constraints_wide, None)
        .expect("Layout failed with wide constraints");

    // Count how many lines were created
    let max_y_wide = layout_wide
        .items
        .iter()
        .filter_map(|item| {
            if let azul_layout::text3::cache::PositionedItem::Cluster(cluster) = item {
                Some(cluster.position.y)
            } else {
                None
            }
        })
        .fold(0.0f32, |max, y| max.max(y));

    println!("Wide layout (555px): max_y = {}", max_y_wide);

    // Test 2: With narrow width (363px like list items), text might wrap
    let constraints_narrow = UnifiedConstraints {
        available_width: 363.0,
        ..Default::default()
    };

    let layout_narrow = kp_layout(&items, &logical_items, &constraints_narrow, None)
        .expect("Layout failed with narrow constraints");

    let max_y_narrow = layout_narrow
        .items
        .iter()
        .filter_map(|item| {
            if let azul_layout::text3::cache::PositionedItem::Cluster(cluster) = item {
                Some(cluster.position.y)
            } else {
                None
            }
        })
        .fold(0.0f32, |max, y| max.max(y));

    println!("Narrow layout (363px): max_y = {}", max_y_narrow);

    // Assert: With 555px width, all text should fit on a single line (y=0)
    assert!(
        max_y_wide < 1.0,
        "Text wrapped to multiple lines with 555px width! max_y = {}, expected 0 (single line)",
        max_y_wide
    );

    // The narrow layout (363px) will legitimately need to wrap,
    // but we're testing that the wide one doesn't
}
