use azul_core::{
    app_resources::{FontMetrics, WordType},
    ui_solver::ResolvedTextLayoutOptions,
};

use crate::text::{
    layout::{position_words, shape_words, split_text_into_words},
    mock::MockFont,
};

#[test]
fn test_split_text_into_words() {
    let text = "Hello World";
    let words = split_text_into_words(text);

    assert_eq!(words.items.len(), 3); // "Hello", " " (space), "World"
    assert_eq!(words.internal_str.as_str(), "Hello World");

    assert_eq!(words.items.as_slice()[0].word_type, WordType::Word);
    assert_eq!(words.items.as_slice()[1].word_type, WordType::Space);
    assert_eq!(words.items.as_slice()[2].word_type, WordType::Word);
}

#[test]
fn test_shape_words() {
    let text = "Hello";
    let words = split_text_into_words(text);

    let font_metrics = FontMetrics {
        units_per_em: 1000,
        ascender: 800,
        descender: -200,
        line_gap: 200,
        // Other fields with default values
        ..Default::default()
    };

    let mock_font = MockFont::new(font_metrics)
        .with_glyph_index('H' as u32, 1)
        .with_glyph_index('e' as u32, 2)
        .with_glyph_index('l' as u32, 3)
        .with_glyph_index('o' as u32, 4)
        .with_glyph_advance(1, 10)
        .with_glyph_advance(2, 8)
        .with_glyph_advance(3, 5)
        .with_glyph_advance(4, 9)
        .with_glyph_size(1, (10, 20))
        .with_glyph_size(2, (8, 15))
        .with_glyph_size(3, (5, 18))
        .with_glyph_size(4, (9, 16));

    let shaped_words = shape_words(&words, &mock_font);

    assert_eq!(shaped_words.items.len(), 1); // One word: "Hello"
    assert_eq!(shaped_words.space_advance, 10); // Default space width
    assert_eq!(shaped_words.font_metrics_units_per_em, 1000);
    assert_eq!(shaped_words.font_metrics_ascender, 800);
    assert_eq!(shaped_words.font_metrics_descender, -200);
    assert_eq!(shaped_words.font_metrics_line_gap, 200);

    // Check the shaped word
    let shaped_word = &shaped_words.items.as_slice()[0];
    assert_eq!(shaped_word.word_width, 10 + 8 + 5 + 5 + 9); // Sum of glyph advances: H+e+l+l+o
    assert_eq!(shaped_word.glyph_infos.len(), 5); // H, e, l, l, o
}

#[test]
fn test_position_words() {
    let text = "Hello World";
    let words = split_text_into_words(text);

    let font_metrics = FontMetrics {
        units_per_em: 1000,
        ascender: 800,
        descender: -200,
        line_gap: 200,
        // Other fields with default values
        ..Default::default()
    };

    let mock_font = MockFont::new(font_metrics)
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
        .with_glyph_advance(5, 100)  // space
        .with_glyph_advance(6, 350)  // W
        .with_glyph_advance(7, 200)  // r
        .with_glyph_advance(8, 250)  // d
        .with_glyph_size(1, (10, 20))
        .with_glyph_size(2, (8, 15))
        .with_glyph_size(3, (5, 18))
        .with_glyph_size(4, (9, 16))
        .with_glyph_size(5, (4, 5))
        .with_glyph_size(6, (12, 22))
        .with_glyph_size(7, (6, 14))
        .with_glyph_size(8, (8, 19));

    let shaped_words = shape_words(&words, &mock_font);

    let options = ResolvedTextLayoutOptions {
        font_size_px: 16.0,
        line_height: None.into(),
        letter_spacing: None.into(),
        word_spacing: None.into(),
        tab_width: None.into(),
        max_horizontal_width: None.into(),
        leading: None.into(),
        holes: Vec::new().into(),
    };

    let word_positions = position_words(&words, &shaped_words, &options);

    // Verify word positions were calculated correctly
    assert_eq!(word_positions.word_positions.len(), 3); // "Hello", space, "World"

    // Verify line breaks
    assert_eq!(word_positions.number_of_lines, 1); // Single line since no max width

    // Test with constrained width that forces line break
    let constrained_options = ResolvedTextLayoutOptions {
        max_horizontal_width: Some(30.0).into(), // Force line break
        ..options
    };

    let constrained_word_positions = position_words(&words, &shaped_words, &constrained_options);

    // With constrained width, "World" should go to the next line
    assert_eq!(constrained_word_positions.number_of_lines, 2);
}

#[test]
fn test_with_line_breaks() {
    let text = "Hello\nWorld";
    let words = split_text_into_words(text);

    let font_metrics = FontMetrics {
        units_per_em: 1000,
        ascender: 800,
        descender: -200,
        line_gap: 200,
        ..Default::default()
    };

    let mock_font = MockFont::new(font_metrics)
        .with_glyph_index('H' as u32, 1)
        .with_glyph_index('e' as u32, 2)
        .with_glyph_index('l' as u32, 3)
        .with_glyph_index('o' as u32, 4)
        .with_glyph_index('W' as u32, 5)
        .with_glyph_index('r' as u32, 6)
        .with_glyph_index('d' as u32, 7)
        .with_glyph_advance(1, 10)
        .with_glyph_advance(2, 8)
        .with_glyph_advance(3, 5)
        .with_glyph_advance(4, 9)
        .with_glyph_advance(5, 12)
        .with_glyph_advance(6, 6)
        .with_glyph_advance(7, 8);

    // Verify the return character is properly detected
    assert_eq!(words.items.len(), 3); // "Hello", return, "World"

    let shaped_words = shape_words(&words, &mock_font);
    let options = ResolvedTextLayoutOptions {
        font_size_px: 16.0,
        line_height: None.into(),
        letter_spacing: None.into(),
        word_spacing: None.into(),
        tab_width: None.into(),
        max_horizontal_width: None.into(),
        leading: None.into(),
        holes: Vec::new().into(),
    };

    let word_positions = position_words(&words, &shaped_words, &options);

    // Verify newline forced a line break
    assert_eq!(word_positions.number_of_lines, 2);

    // Verify y-position of second line is below the first line
    assert!(
        word_positions.word_positions[2].position.y > word_positions.word_positions[0].position.y
    );
}
