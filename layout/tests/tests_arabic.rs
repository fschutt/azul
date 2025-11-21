// Test for Arabic text shaping and layout

#[test]
fn test_arabic_unicode_bidi_detection() {
    // Test what BiDi level the unicode_bidi crate detects
    use unicode_bidi::{BidiInfo, Direction as UBidiDirection};
    
    let arabic_text = "مرحبا بالعالم";
    println!("\n=== BiDi Detection Test ===");
    println!("Text: {}", arabic_text);
    
    // Test unicode_bidi's base direction detection
    let bidi_dir = unicode_bidi::get_base_direction(arabic_text);
    println!("unicode_bidi::get_base_direction: {:?}", bidi_dir);
    
    // Test BidiInfo
    let bidi_info = BidiInfo::new(arabic_text, None);
    let para = &bidi_info.paragraphs[0];
    println!("Paragraph level is RTL: {}", para.level.is_rtl());
    println!("Paragraph level number: {}", para.level.number());
    
    // Expected: Should detect RTL
    assert!(matches!(bidi_dir, UBidiDirection::Rtl), 
            "Arabic text should be detected as RTL, got {:?}", bidi_dir);
}

#[test]
fn test_arabic_text_shaping() {
    // Original Arabic text: "مرحبا بالعالم"
    // This should be shaped with proper ligatures and RTL ordering
    
    let arabic_text = "مرحبا بالعالم";
    
    println!("\n=== Arabic Text Analysis ===");
    println!("Original text: {}", arabic_text);
    println!("Length: {} chars", arabic_text.chars().count());
    println!("\nCharacter breakdown:");
    for (i, ch) in arabic_text.chars().enumerate() {
        println!("  [{}] U+{:04X} '{}'", i, ch as u32, ch);
    }
    
    // Test script detection
    use crate::text3::script::detect_script;
    if let Some(script) = detect_script(arabic_text) {
        println!("\nDetected script: {:?}", script);
    }
}

#[test]
fn test_arabic_unicode_order() {
    // Test the actual Unicode order
    let arabic = "مرحبا بالعالم";
    
    println!("\n=== Unicode Character Analysis ===");
    println!("Text: {}", arabic);
    println!("Chars: {}", arabic.chars().count());
    println!("Bytes: {}", arabic.len());
    
    println!("\nLogical order (as stored in memory):");
    for (i, ch) in arabic.chars().enumerate() {
        let name = match ch {
            'م' => "MEEM",
            'ر' => "REH", 
            'ح' => "HAH",
            'ب' => "BEH",
            'ا' => "ALEF",
            'ل' => "LAM",
            'ع' => "AIN",
            ' ' => "SPACE",
            _ => "UNKNOWN",
        };
        println!("  [{}] U+{:04X} '{}' {}", i, ch as u32, ch, name);
    }
    
    println!("\nExpected visual order (RTL, shaped):");
    println!("  Right-to-left: م ل ا ع ل ا ب   ا ب ح ر م");
    println!("  With ligatures and contextual forms");
    
    // Test with Latin text for comparison
    let latin = "Hello";
    println!("\n=== Latin Comparison ===");
    println!("Text: {}", latin);
    for (i, ch) in latin.chars().enumerate() {
        println!("  [{}] U+{:04X} '{}'", i, ch as u32, ch);
    }
}

#[test]
fn test_mixed_arabic_latin() {
    // This is what's in the actual test: "مرحبا بالعالم - Arabic text"
    let mixed = "مرحبا بالعالم - Arabic text requiring proper shaping";
    
    println!("\n=== Mixed Arabic/Latin Analysis ===");
    println!("Text: {}", mixed);
    
    println!("\nCharacter-by-character breakdown:");
    for (i, ch) in mixed.chars().enumerate() {
        let script = if ch >= '\u{0600}' && ch <= '\u{06FF}' {
            "Arabic"
        } else if ch.is_ascii_alphabetic() {
            "Latin"
        } else if ch.is_whitespace() {
            "Whitespace"
        } else {
            "Other"
        };
        println!("  [{}] U+{:04X} '{}' ({})", i, ch as u32, ch, script);
    }
    
    println!("\nExpected layout:");
    println!("  - Arabic portion: RTL, shaped");
    println!("  - Latin portion: LTR, normal");
    println!("  - Hyphen should be between them");
    
    // Test BiDi on just the Arabic portion
    use unicode_bidi::BidiInfo;
    let arabic_only = "مرحبا بالعالم";
    let bidi_info = BidiInfo::new(arabic_only, None);
    let para = &bidi_info.paragraphs[0];
    println!("\nBiDi analysis of Arabic portion:");
    println!("  Paragraph level is RTL: {}", para.level.is_rtl());
    println!("  Paragraph level number: {}", para.level.number());
    
    // Test BiDi on the mixed text
    let bidi_info_mixed = BidiInfo::new(mixed, None);
    let para_mixed = &bidi_info_mixed.paragraphs[0];
    println!("\nBiDi analysis of mixed text:");
    println!("  Paragraph level is RTL: {}", para_mixed.level.is_rtl());
    println!("  Paragraph level number: {}", para_mixed.level.number());
    println!("  First strong character: {:?}", unicode_bidi::get_base_direction(mixed));
}

#[test]
fn test_bidi_with_css_direction_ltr() {
    // Test that our layout system uses CSS direction (LTR) instead of auto-detection
    // for mixed Arabic/Latin text
    use crate::text3::cache::{Direction, reorder_logical_items, LogicalItem, ContentIndex};
    use std::sync::Arc;
    
    let mixed_text = "مرحبا - Hello";
    
    println!("\n=== Testing BiDi with CSS direction: LTR ===");
    println!("Text: {}", mixed_text);
    
    // Create logical items
    let logical_items = vec![
        LogicalItem::Text {
            source: ContentIndex { run_index: 0, item_index: 0 },
            text: mixed_text.to_string(),
            style: Arc::new(Default::default()),
        }
    ];
    
    // Test with LTR base direction (CSS default)
    println!("\nReordering with base_direction = LTR (CSS default):");
    let visual_items_ltr = reorder_logical_items(&logical_items, Direction::Ltr)
        .expect("BiDi reordering should succeed");
    
    println!("Visual items count: {}", visual_items_ltr.len());
    for (i, item) in visual_items_ltr.iter().enumerate() {
        println!("  Visual item {}: text='{}', level={}", 
                 i, item.text, item.bidi_level.level());
    }
    
    // Test with RTL base direction (for comparison)
    println!("\nReordering with base_direction = RTL (for comparison):");
    let visual_items_rtl = reorder_logical_items(&logical_items, Direction::Rtl)
        .expect("BiDi reordering should succeed");
    
    println!("Visual items count: {}", visual_items_rtl.len());
    for (i, item) in visual_items_rtl.iter().enumerate() {
        println!("  Visual item {}: text='{}', level={}", 
                 i, item.text, item.bidi_level.level());
    }
    
    // Assertions:
    // With LTR base direction, the paragraph level should be 0 (LTR)
    // The Arabic portion should have level 1 (RTL run within LTR paragraph)
    // The Latin portion should have level 0 (LTR)
    
    // Find the Arabic portion
    let arabic_item = visual_items_ltr.iter()
        .find(|item| item.text.contains('م'))
        .expect("Should find Arabic text");
    
    println!("\nAssertion checks:");
    println!("  Arabic portion BiDi level: {}", arabic_item.bidi_level.level());
    
    // With LTR base direction, Arabic should be embedded at level 1
    assert_eq!(arabic_item.bidi_level.level(), 1, 
               "Arabic text in LTR paragraph should have BiDi level 1");
    
    // Find the Latin portion
    let latin_item = visual_items_ltr.iter()
        .find(|item| item.text.contains('H'))
        .expect("Should find Latin text");
    
    println!("  Latin portion BiDi level: {}", latin_item.bidi_level.level());
    
    // With LTR base direction, Latin should be at level 0
    assert_eq!(latin_item.bidi_level.level(), 0,
               "Latin text in LTR paragraph should have BiDi level 0");
    
    println!("\n✓ Test passed: CSS direction (LTR) correctly overrides text-based auto-detection");
}

#[test]
fn test_bidi_visual_order_with_ltr_base() {
    // Test the actual visual ordering with LTR base direction
    use crate::text3::cache::{Direction, reorder_logical_items, LogicalItem, ContentIndex};
    use std::sync::Arc;
    
    // Simple case: Arabic text followed by Latin text
    let text = "مرحبا Hello";
    
    println!("\n=== Testing Visual Order with LTR Base Direction ===");
    println!("Text: {}", text);
    
    let logical_items = vec![
        LogicalItem::Text {
            source: ContentIndex { run_index: 0, item_index: 0 },
            text: text.to_string(),
            style: Arc::new(Default::default()),
        }
    ];
    
    let visual_items = reorder_logical_items(&logical_items, Direction::Ltr)
        .expect("BiDi reordering should succeed");
    
    println!("\nVisual items (in display order):");
    for (i, item) in visual_items.iter().enumerate() {
        println!("  [{}] text='{}', level={}", 
                 i, item.text, item.bidi_level.level());
    }
    
    // In LTR paragraph with Arabic first:
    // - The Arabic text should be visually reordered RTL within itself
    // - But the overall order should be: Arabic section first (on the left), then Latin section
    // This is different from RTL paragraph where everything would be right-aligned
    
    assert!(!visual_items.is_empty(), "Should have visual items");
    
    println!("\n✓ Visual reordering completed successfully with LTR base direction");
}
