/// Test case to reproduce the bounds.width=0 bug
/// 
/// ISSUE: When layout_flow is called with valid available_width (e.g., 800.0),
/// the returned FragmentLayout has bounds.width=0, causing text to be positioned
/// vertically instead of horizontally.
/// 
/// This test should FAIL initially, demonstrating the bug.

use crate::text3::{
    cache::{
        InlineContent, LayoutCache, LayoutFragment, StyleProperties, StyledRun,
        UnifiedConstraints,
    },
    tests::{create_mock_font_manager, default_style},
};

#[test]
fn test_available_width_should_produce_nonzero_bounds() {
    println!("TEST: available_width -> bounds");

    let font_manager = create_mock_font_manager();
    let mut text_cache = LayoutCache::new();

    // Create a simple text content
    let text = "Hello World";
    let content = vec![InlineContent::Text(StyledRun {
        text: text.to_string(),
        style: default_style(),
        logical_start_byte: 0,
    })];

    println!("Input text: '{}'", text);
    println!("Text length: {} characters", text.len());

    // Create constraints with a reasonable available_width
    let available_width = 800.0;
    let constraints = UnifiedConstraints {
        available_width,
        available_height: Some(600.0),
        ..Default::default()
    };

    println!("\nUnifiedConstraints:");
    println!("  available_width: {}", constraints.available_width);
    println!("  available_height: {:?}", constraints.available_height);

    // Create a single fragment
    let fragments = vec![LayoutFragment {
        id: "test".to_string(),
        constraints,
    }];

    println!("\nCalling layout_flow()...");

    // Perform layout
    let result = text_cache
        .layout_flow(&content, &[], &fragments, &font_manager)
        .expect("layout_flow should succeed");

    println!("layout_flow() completed");

    // Get the fragment layout
    let fragment = result
        .fragment_layouts
        .get("test")
        .expect("Should have 'test' fragment");

    let frag_bounds = fragment.bounds();
    println!("\nFragment Layout Result:");
    println!("  bounds.width: {}", frag_bounds.width);
    println!("  bounds.height: {}", frag_bounds.height);
    println!("  number of items: {}", fragment.items.len());

    // Print item positions
    if !fragment.items.is_empty() {
        println!("\nItem positions:");
        for (i, item) in fragment.items.iter().take(5).enumerate() {
            println!(
                "    [{}] pos=({}, {})",
                i, item.position.x, item.position.y
            );
        }
        if fragment.items.len() > 5 {
            println!("    ... ({} more items)", fragment.items.len() - 5);
        }
    }

    println!("TEST ASSERTIONS");

    // CRITICAL ASSERTION: bounds.width should NOT be zero!
    // With available_width=800.0 and text "Hello World" (11 chars),
    // we expect bounds.width to be > 0 (approximately the width of the text)
    
    println!("Checking: bounds.width > 0");
    assert!(
        frag_bounds.width > 0.0,
        "FAIL: bounds.width is {}, expected > 0. \
         With available_width={}, the text '{}' should have a measurable width.",
        frag_bounds.width,
        available_width,
        text
    );

    // Also check that height is reasonable
    println!("Checking: bounds.height > 0");
    assert!(
        frag_bounds.height > 0.0,
        "FAIL: bounds.height is {}, expected > 0",
        frag_bounds.height
    );

    // Check that we have items
    println!("Checking: items.len() > 0");
    assert!(
        !fragment.items.is_empty(),
        "FAIL: No items in fragment, expected glyphs for '{}'",
        text
    );

    // For horizontal text, glyphs should be positioned horizontally (increasing x)
    // not vertically (x=0, increasing y)
    if fragment.items.len() > 1 {
        let first_x = fragment.items[0].position.x;
        let last_x = fragment.items[fragment.items.len() - 1].position.x;
        
        println!("Checking: horizontal layout (last_x > first_x)");
        println!("  first_x: {}", first_x);
        println!("  last_x: {}", last_x);
        
        assert!(
            last_x > first_x,
            "FAIL: Text appears to be laid out vertically (first_x={}, last_x={}). \
             For horizontal text, last_x should be > first_x.",
            first_x,
            last_x
        );
    }

    println!("\n✓ All assertions passed!");
}

#[test]
fn test_available_width_zero_should_produce_zero_bounds() {
    // This is the expected behavior: if available_width=0, bounds should be 0
    let font_manager = create_mock_font_manager();
    let mut text_cache = LayoutCache::new();

    let content = vec![InlineContent::Text(StyledRun {
        text: "Hello".to_string(),
        style: default_style(),
        logical_start_byte: 0,
    })];

    let constraints = UnifiedConstraints {
        available_width: 0.0, // Zero width
        available_height: Some(600.0),
        ..Default::default()
    };

    let fragments = vec![LayoutFragment {
        id: "test".to_string(),
        constraints,
    }];

    let result = text_cache
        .layout_flow(&content, &[], &fragments, &font_manager)
        .expect("layout_flow should succeed");

    let fragment = result.fragment_layouts.get("test").unwrap();

    let frag_bounds = fragment.bounds();
    // With zero available width, it's acceptable to have bounds.width=0
    // (all text is forced to break, each character on its own line)
    println!("available_width=0 -> bounds.width={}", frag_bounds.width);
    // This test just documents the behavior, no assertion needed
}

#[test]
fn test_available_width_infinite_should_produce_full_width() {
    let font_manager = create_mock_font_manager();
    let mut text_cache = LayoutCache::new();

    let text = "Hello World";
    let content = vec![InlineContent::Text(StyledRun {
        text: text.to_string(),
        style: default_style(),
        logical_start_byte: 0,
    })];

    let constraints = UnifiedConstraints {
        available_width: f32::INFINITY,
        available_height: Some(600.0),
        ..Default::default()
    };

    let fragments = vec![LayoutFragment {
        id: "test".to_string(),
        constraints,
    }];

    let result = text_cache
        .layout_flow(&content, &[], &fragments, &font_manager)
        .expect("layout_flow should succeed");

    let fragment = result.fragment_layouts.get("test").unwrap();

    let frag_bounds = fragment.bounds();
    println!("\nInfinite width test:");
    println!("  available_width: inf");
    println!("  bounds.width: {}", frag_bounds.width);
    println!("  bounds.height: {}", frag_bounds.height);

    // With infinite width, text should be on a single line
    // and bounds.width should be the full text width (> 0)
    assert!(
        frag_bounds.width > 0.0,
        "With infinite available_width, bounds.width should be > 0, got {}",
        frag_bounds.width
    );
}
