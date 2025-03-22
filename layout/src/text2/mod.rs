//! Text layout with float integration
//!
//! This module extends the basic text layout to support text flowing around floated elements

use std::collections::BTreeMap;

use azul_core::{
    app_resources::{ShapedWords, WordPosition, Words},
    ui_solver::{
        InlineTextLayout, InlineTextLayoutRustInternal, InlineTextLine, LayoutDebugMessage,
        ResolvedTextLayoutOptions,
    },
    window::{LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::StyleTextAlign;

pub mod layout;
pub mod mock;
pub mod script;
pub mod shaping;
#[cfg(test)]
pub mod tests;

use azul_core::app_resources::{ExclusionSide, TextExclusionArea};

use self::layout::{position_words, word_positions_to_inline_text_layout};
use crate::solver2::layout::{adjust_rect_for_floats, get_relevant_floats};

/// Data structure representing padding for text layout
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLayoutOffsets {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl TextLayoutOffsets {
    pub fn zero() -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        }
    }
}

/// Trait for font implementations that can be used for text shaping and layout.
/// This abstraction allows for mocking fonts during testing.
pub trait FontImpl {
    /// Returns the width of the space character, if available
    fn get_space_width(&self) -> Option<usize>;

    /// Returns the horizontal advance of a glyph
    fn get_horizontal_advance(&self, glyph_index: u16) -> u16;

    /// Returns the size (width, height) of a glyph, if available
    fn get_glyph_size(&self, glyph_index: u16) -> Option<(i32, i32)>;

    /// Shapes text using the font
    fn shape(
        &self,
        text: &[u32],
        script: u32,
        lang: Option<u32>,
    ) -> shaping::ShapedTextBufferUnsized;

    /// Looks up a glyph index from a Unicode codepoint
    fn lookup_glyph_index(&self, c: u32) -> Option<u16>;

    /// Returns a reference to the font metrics
    fn get_font_metrics(&self) -> &azul_core::app_resources::FontMetrics;
}

/// Layout text with exclusion areas for floats
pub fn layout_text_with_floats(
    words: &Words,
    shaped_words: &ShapedWords,
    text_layout_options: &ResolvedTextLayoutOptions,
    exclusion_areas: &[TextExclusionArea],
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> InlineTextLayout {
    // If no exclusion areas, use standard text layout
    if exclusion_areas.is_empty() {
        let word_positions =
            position_words(words, shaped_words, text_layout_options, debug_messages);
        let mut inline_text_layout = word_positions_to_inline_text_layout(&word_positions);

        // Apply text alignment if needed
        if let Some(text_align) = text_layout_options.text_justify.into_option() {
            if text_align != StyleTextAlign::Left {
                if let Some(max_width) = text_layout_options.max_horizontal_width.into_option() {
                    let parent_size = LogicalSize::new(max_width, 0.0); // Height doesn't matter for horizontal alignment
                    inline_text_layout.align_children_horizontal(&parent_size, text_align);
                }
            }
        }

        return inline_text_layout;
    }

    // Create modified layout options with holes for the exclusion areas
    let mut modified_options = text_layout_options.clone();
    modified_options.holes = exclusion_areas
        .iter()
        .map(|area| area.rect)
        .collect::<Vec<_>>()
        .into();

    // Perform text layout with the modified options
    let word_positions = position_words(words, shaped_words, &modified_options, debug_messages);

    // Create line boxes
    let mut line_boxes = Vec::new();

    // Adjust line boxes for exclusion areas
    for line in word_positions.line_breaks.as_slice() {
        let mut adjusted_line = line.clone();

        // Find exclusions that intersect with this line
        let line_y = line.bounds.origin.y;
        let line_height = line.bounds.size.height;

        let relevant_floats = get_relevant_floats(exclusion_areas, (line_y, line_y + line_height));

        if !relevant_floats.is_empty() {
            // Adjust line width based on exclusions
            adjusted_line.bounds =
                adjust_rect_for_floats(adjusted_line.bounds, &relevant_floats, debug_messages);
        }

        line_boxes.push(adjusted_line);
    }

    // Apply text alignment if needed
    if let Some(text_align) = text_layout_options.text_justify.into_option() {
        if text_align != StyleTextAlign::Left {
            for line in &mut line_boxes {
                // For each line, adjust word positions according to line bounds and alignment
                adjust_line_alignment(
                    line,
                    &word_positions.word_positions,
                    text_align,
                    debug_messages,
                );
            }
        }
    }

    // Create the final inline text layout
    InlineTextLayout {
        lines: line_boxes.into(),
        content_size: word_positions.content_size,
    }
}

/// Adjust the alignment of words within a line
fn adjust_line_alignment(
    line: &mut InlineTextLine,
    word_positions: &[WordPosition],
    text_align: StyleTextAlign,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LogicalRect {
    // Only handle words that are in this line
    let line_words: Vec<&WordPosition> = word_positions
        .iter()
        .skip(line.word_start)
        .take(line.word_end - line.word_start + 1)
        .collect();

    if line_words.is_empty() {
        return line.bounds;
    }

    // Calculate the current line width based on the rightmost word
    let rightmost_word = line_words
        .iter()
        .max_by(|a, b| {
            let a_right = a.position.x + a.size.width;
            let b_right = b.position.x + b.size.width;
            a_right.partial_cmp(&b_right).unwrap()
        })
        .unwrap();

    let line_width = rightmost_word.position.x + rightmost_word.size.width - line.bounds.origin.x;
    let available_width = line.bounds.size.width;

    // Don't adjust if there's no room
    if line_width >= available_width {
        return line.bounds;
    }

    // Calculate the offset based on alignment
    let offset = match text_align {
        StyleTextAlign::Left => 0.0, // No adjustment needed
        StyleTextAlign::Center => (available_width - line_width) / 2.0,
        StyleTextAlign::Right => available_width - line_width,
        StyleTextAlign::Justify => {
            // For justify, we'd need to adjust spacing between words
            // This is more complex and would require modifying the WordPositions
            // We'll ignore this for now
            0.0
        }
    };

    if offset > 0.0 {
        // Update the line's horizontal position
        line.bounds.origin.x += offset;
    }

    line.bounds
}

#[cfg(test)]
mod text_layout_tests {
    use std::{collections::BTreeMap, sync::atomic::AtomicUsize};

    use allsorts_subset_browser::tables::{HheaTable, MaxpTable};
    use azul_core::{
        app_resources::{
            Au, DpiScaleFactor, FontInstanceKey, FontKey, FontMetrics, IdNamespace,
            ImageDescriptor, ImageRefHash, RendererResourcesTrait, ResolvedImage,
        },
        dom::{NodeData, NodeType},
        id_tree::NodeId,
        styled_dom::{
            CssPropertyCache, CssPropertyCachePtr, StyleFontFamiliesHash, StyleFontFamilyHash,
            StyledDom, StyledNode,
        },
        ui_solver::ResolvedTextLayoutOptions,
        window::{LogicalPosition, LogicalRect, LogicalSize},
        FastHashMap,
    };
    use azul_css::{
        AzString, CssProperty, CssPropertyType, CssPropertyValue, FontData, FontRef,
        LayoutBoxSizing, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight,
        LayoutPaddingTop, StyleFontFamily, StyleFontSize, StyleTextAlign,
    };

    use crate::{
        solver2::context::{determine_formatting_contexts, FormattingContext},
        text2::{layout::layout_text_node, mock::MockFont, shaping::ParsedFont},
    };

    #[derive(Debug)]
    struct MockRendererResources {
        font_ref: FontRef,
        style_font_family_hash: StyleFontFamilyHash,
        font_key: FontKey,
    }

    impl MockRendererResources {
        fn new() -> Self {
            // Create a mock font with basic metrics
            let font_metrics = FontMetrics {
                units_per_em: 1000,
                ascender: 800,
                descender: -200,
                line_gap: 200,
                ..Default::default()
            };

            let mock_font = MockFont::new(font_metrics)
            .with_space_width(250)
            .with_glyph_index('H' as u32, 1)
            .with_glyph_index('e' as u32, 2)
            .with_glyph_index('l' as u32, 3)
            .with_glyph_index('o' as u32, 4)
            .with_glyph_index(' ' as u32, 5)
            .with_glyph_index('W' as u32, 6)
            .with_glyph_index('r' as u32, 7)
            .with_glyph_index('d' as u32, 8)
            .with_glyph_advance(1, 300)  // H
            .with_glyph_advance(2, 250)  // e
            .with_glyph_advance(3, 200)  // l
            .with_glyph_advance(4, 250)  // o
            .with_glyph_advance(5, 250)  // space
            .with_glyph_advance(6, 350)  // W
            .with_glyph_advance(7, 200)  // r
            .with_glyph_advance(8, 250)  // d
            .with_glyph_size(1, (300, 700))
            .with_glyph_size(2, (250, 500))
            .with_glyph_size(3, (200, 700))
            .with_glyph_size(4, (250, 500))
            .with_glyph_size(5, (250, 100))
            .with_glyph_size(6, (350, 700))
            .with_glyph_size(7, (200, 500))
            .with_glyph_size(8, (250, 700));

            // Create a font_ref from our mock font
            let font_ref = create_font_ref_from_mock(&mock_font);

            // Generate a unique font key
            let font_key = FontKey {
                namespace: IdNamespace(1),
                key: 1,
            };

            // Create a font family hash
            let style_font_family_hash =
                StyleFontFamilyHash::new(&StyleFontFamily::System("serif".into()));

            Self {
                font_ref,
                style_font_family_hash,
                font_key,
            }
        }
    }

    impl RendererResourcesTrait for MockRendererResources {
        fn get_font_family(
            &self,
            _style_font_families_hash: &StyleFontFamiliesHash,
        ) -> Option<&StyleFontFamilyHash> {
            Some(&self.style_font_family_hash)
        }

        fn get_font_key(&self, _style_font_family_hash: &StyleFontFamilyHash) -> Option<&FontKey> {
            Some(&self.font_key)
        }

        fn get_registered_font(
            &self,
            _font_key: &FontKey,
        ) -> Option<&(FontRef, FastHashMap<(Au, DpiScaleFactor), FontInstanceKey>)> {
            // Return a reference to the static instance
            static mut FONT_INSTANCES: Option<(
                FontRef,
                FastHashMap<(Au, DpiScaleFactor), FontInstanceKey>,
            )> = None;

            unsafe {
                if FONT_INSTANCES.is_none() {
                    let instances = FastHashMap::default();
                    // Clone the font_ref - this is safe since it's reference-counted internally
                    FONT_INSTANCES = Some((self.font_ref.clone(), instances));
                }
                FONT_INSTANCES.as_ref()
            }
        }

        fn get_image(&self, _hash: &ImageRefHash) -> Option<&ResolvedImage> {
            None
        }

        fn update_image(&mut self, _image_ref_hash: &ImageRefHash, _descriptor: ImageDescriptor) {
            // No-op for tests
        }
    }

    /// Create a ParsedFont from a MockFont
    pub fn create_parsed_font_from_mock(mock_font: &MockFont) -> ParsedFont {
        use crate::text2::FontImpl;
        ParsedFont {
            font_metrics: mock_font.get_font_metrics().clone(),
            num_glyphs: 256, // Reasonable default for tests
            hhea_table: default_hhea_table(),
            hmtx_data: Vec::new(),
            maxp_table: default_maxp_table(),
            gsub_cache: None,
            gpos_cache: None,
            opt_gdef_table: None,
            glyph_records_decoded: BTreeMap::new(),
            space_width: mock_font.get_space_width(),
            cmap_subtable: None,
            mock: Some(Box::new(mock_font.clone())),
        }
    }

    fn default_maxp_table() -> MaxpTable {
        MaxpTable {
            num_glyphs: 256,
            version1_sub_table: None,
        }
    }

    fn default_hhea_table() -> HheaTable {
        HheaTable {
            ascender: 0,
            descender: 0,
            line_gap: 0,
            advance_width_max: 0,
            min_left_side_bearing: 0,
            min_right_side_bearing: 0,
            x_max_extent: 0,
            caret_slope_rise: 0,
            caret_slope_run: 0,
            caret_offset: 0,
            num_h_metrics: 0,
        }
    }
    /// Create a FontRef from a MockFont
    pub fn create_font_ref_from_mock(mock_font: &MockFont) -> FontRef {
        use std::ffi::c_void;

        fn parsed_font_destructor(ptr: *mut c_void) {
            unsafe {
                let _ = Box::from_raw(ptr as *mut ParsedFont);
            }
        }

        let parsed_font = create_parsed_font_from_mock(mock_font);

        // Create empty byte vector as font data - not used for mock fonts
        let bytes = Vec::<u8>::new().into();

        FontRef::new(FontData {
            bytes,
            font_index: 0,
            parsed: Box::into_raw(Box::new(parsed_font)) as *const c_void,
            parsed_destructor: parsed_font_destructor,
        })
    }

    // Helper to create a simple text node DOM
    fn create_test_dom(text: &str, properties: Vec<(CssPropertyType, CssProperty)>) -> StyledDom {
        let mut styled_dom = StyledDom::default();

        // Create node data
        let mut node_data = NodeData::default();
        node_data.set_node_type(NodeType::Text(AzString::from(text)));

        // Apply CSS properties
        let mut property_cache = CssPropertyCache::default();
        property_cache.node_count = 1;

        let mut props_map = BTreeMap::new();
        for (property_type, property_value) in properties {
            props_map.insert(property_type, property_value);
        }

        property_cache
            .css_normal_props
            .insert(NodeId::new(0), props_map);

        // Create styled node
        let styled_node = StyledNode::default();

        // Set up DOM
        styled_dom.node_data = vec![node_data].into();
        styled_dom.styled_nodes = vec![styled_node].into();
        styled_dom.css_property_cache = CssPropertyCachePtr::new(property_cache);

        styled_dom
    }

    #[test]
    fn test_basic_text_layout() {
        // Create a simple text DOM with basic styling
        let text = "Hello World";
        let properties = vec![
            (
                CssPropertyType::FontSize,
                CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize::px(16.0))),
            ),
            (
                CssPropertyType::FontFamily,
                CssProperty::FontFamily(CssPropertyValue::Exact(
                    vec![StyleFontFamily::System("serif".into())].into(),
                )),
            ),
            (
                CssPropertyType::TextAlign,
                CssProperty::TextAlign(CssPropertyValue::Exact(StyleTextAlign::Left)),
            ),
        ];

        let styled_dom = create_test_dom(text, properties);
        let formatting_contexts = determine_formatting_contexts(&styled_dom);

        // Create mock renderer resources
        let renderer_resources = MockRendererResources::new();

        // Create available space
        let available_rect =
            LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(400.0, 300.0));

        // Add debug messages container
        let mut debug_messages = Some(Vec::new());

        // Layout text node
        let text_layout = layout_text_node(
            NodeId::new(0),
            &styled_dom,
            &formatting_contexts.as_ref()[NodeId::new(0)],
            available_rect,
            &renderer_resources,
            &mut debug_messages,
        );

        // Print debug messages to help identify any issues
        if let Some(messages) = debug_messages {
            for msg in messages {
                println!("[{}] {}", msg.location, msg.message);
            }
        }

        assert!(text_layout.is_some(), "Text layout should be successful");

        let text_layout = text_layout.unwrap();

        // Verify we have one line
        assert_eq!(
            text_layout.lines.len(),
            1,
            "Text layout should have one line"
        );

        // Verify the line contains our text
        let line = &text_layout.lines.as_slice()[0];
        assert!(
            line.bounds.size.width > 0.0,
            "Line should have positive width"
        );
    }

    #[test]
    fn test_text_layout_with_padding() {
        // Create text DOM with padding
        let text = "Hello World";
        let properties = vec![
            (
                CssPropertyType::FontSize,
                CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize::px(16.0))),
            ),
            (
                CssPropertyType::FontFamily,
                CssProperty::FontFamily(CssPropertyValue::Exact(
                    vec![StyleFontFamily::System("serif".into())].into(),
                )),
            ),
            (
                CssPropertyType::PaddingLeft,
                CssProperty::PaddingLeft(CssPropertyValue::Exact(LayoutPaddingLeft::px(20.0))),
            ),
            (
                CssPropertyType::PaddingRight,
                CssProperty::PaddingRight(CssPropertyValue::Exact(LayoutPaddingRight::px(20.0))),
            ),
            (
                CssPropertyType::PaddingTop,
                CssProperty::PaddingTop(CssPropertyValue::Exact(LayoutPaddingTop::px(10.0))),
            ),
            (
                CssPropertyType::PaddingBottom,
                CssProperty::PaddingBottom(CssPropertyValue::Exact(LayoutPaddingBottom::px(10.0))),
            ),
        ];

        let styled_dom = create_test_dom(text, properties);
        let formatting_contexts = determine_formatting_contexts(&styled_dom);

        // Create mock renderer resources
        let renderer_resources = MockRendererResources::new();

        // Create available space
        let available_rect =
            LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(400.0, 300.0));

        // Layout text node
        let mut debug = Some(Vec::new());
        let text_layout = layout_text_node(
            NodeId::new(0),
            &styled_dom,
            &formatting_contexts.as_ref()[NodeId::new(0)],
            available_rect,
            &renderer_resources,
            &mut debug,
        );

        // Verify text layout accounts for padding
        assert!(text_layout.is_some(), "Text layout should be successful");
        let text_layout = text_layout.unwrap();

        // Should have one line
        assert_eq!(
            text_layout.lines.len(),
            1,
            "Text layout should have one line"
        );

        // First line should be positioned with padding in mind
        let first_line = &text_layout.lines.as_slice()[0];
        assert!(
            first_line.bounds.origin.x >= 0.0,
            "Line origin.x should be >= 0"
        );
        assert!(
            first_line.bounds.origin.y >= 0.0,
            "Line origin.y should be >= 0"
        );
    }

    #[test]
    fn test_text_layout_with_constrained_width() {
        // Create a DOM with a long text
        let text = "This is a long text that should wrap to multiple lines when constrained";
        let properties = vec![
            (
                CssPropertyType::FontSize,
                CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize::px(16.0))),
            ),
            (
                CssPropertyType::FontFamily,
                CssProperty::FontFamily(CssPropertyValue::Exact(
                    vec![StyleFontFamily::System("serif".into())].into(),
                )),
            ),
        ];

        let styled_dom = create_test_dom(text, properties);
        println!("styled dom: {}", styled_dom.get_html_string("", "", true));

        let formatting_contexts = determine_formatting_contexts(&styled_dom);

        println!("formatting_contexts: {formatting_contexts:#?}");

        // Create mock renderer resources
        let renderer_resources = MockRendererResources::new();

        println!("renderer_resources: {renderer_resources:#?}");

        // Create narrow available space to force line breaks
        let narrow_rect = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(150.0, 300.0));

        println!("narrow_rect: {narrow_rect:#?}");

        // Layout text node with constrained width
        let mut debug = Some(Vec::new());
        let text_layout = layout_text_node(
            NodeId::new(0),
            &styled_dom,
            &formatting_contexts.as_ref()[NodeId::new(0)],
            narrow_rect,
            &renderer_resources,
            &mut debug,
        );

        println!("text_layout: {text_layout:#?}");
        println!("debug: {debug:#?}");

        assert!(text_layout.is_some(), "Text layout should be successful");
        let text_layout = text_layout.unwrap();

        // Should have multiple lines due to constrained width
        assert!(
            text_layout.lines.len() > 1,
            "Text layout should have multiple lines"
        );

        // Get the heights of the first two lines to verify vertical spacing
        let first_line = &text_layout.lines.as_slice()[0];
        let second_line = &text_layout.lines.as_slice()[1];

        // The second line should be below the first line
        assert!(
            second_line.bounds.origin.y > first_line.bounds.origin.y,
            "Second line should be below first line"
        );
    }

    #[test]
    fn test_text_layout_with_alignment() {
        // Test different text alignments
        for alignment in &[
            StyleTextAlign::Left,
            StyleTextAlign::Center,
            StyleTextAlign::Right,
        ] {
            let text = "Aligned Text";
            let properties = vec![
                (
                    CssPropertyType::FontSize,
                    CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize::px(16.0))),
                ),
                (
                    CssPropertyType::FontFamily,
                    CssProperty::FontFamily(CssPropertyValue::Exact(
                        vec![StyleFontFamily::System("serif".into())].into(),
                    )),
                ),
                (
                    CssPropertyType::TextAlign,
                    CssProperty::TextAlign(CssPropertyValue::Exact(*alignment)),
                ),
            ];

            let styled_dom = create_test_dom(text, properties);
            let formatting_contexts = determine_formatting_contexts(&styled_dom);

            // Create mock renderer resources
            let renderer_resources = MockRendererResources::new();

            // Create available space
            let available_rect =
                LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(400.0, 300.0));

            // Layout text node with alignment
            let mut debug = Some(Vec::new());
            let text_layout = layout_text_node(
                NodeId::new(0),
                &styled_dom,
                &formatting_contexts.as_ref()[NodeId::new(0)],
                available_rect,
                &renderer_resources,
                &mut debug,
            );

            assert!(text_layout.is_some(), "Text layout should be successful");
            let text_layout = text_layout.unwrap();

            // Verify text alignment produces a valid layout
            assert_eq!(
                text_layout.lines.len(),
                1,
                "Text layout should have one line"
            );
        }
    }
}
