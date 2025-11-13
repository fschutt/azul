Of course. To ensure the "default stuff" is robust and prevent regressions, we need a comprehensive set of unit tests. These tests will validate not only the primary bug fixes but also common related scenarios like percentage dimensions, margin collapsing, and deeper inheritance.

Here are more unit tests to add to `layout/src/solver3/tests.rs`. This single code block represents the **entire updated test file**, including the original tests plus the new ones, ensuring you can replace the file completely.

The new tests cover:
*   **Percentage-based width** to ensure it resolves against the container.
*   **`box-sizing: border-box`** to verify it interacts correctly with width and padding.
*   **Nested block stacking** to confirm children stack correctly within a parent.
*   **Margin collapsing** between siblings, a critical part of the Block Formatting Context fix.
*   **Deep font-size inheritance** through multiple levels of the DOM tree.
*   **Relative `em` font sizes** to test cascading with relative units.

---

### `layout/src/solver3/tests.rs` (Complete Updated File)

```rust
//! Comprehensive tests for solver3 layout engine

use std::{collections::BTreeMap, sync::Arc};
use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::{css::Css, parser2::CssApiWrapper};
use rust_fontconfig::FcFontCache;
use crate::{
    solver3::{
        layout_document,
        layout_tree::LayoutTree,
        LayoutError,
        cache::LayoutCache
    },
    text3::cache::{FontManager, LayoutCache as TextLayoutCache},
    window::LayoutWindow,
    window_state::FullWindowState,
};

// Test setup helper to run layout on HTML + CSS strings
fn layout_test_html(
    html_body: &str,
    extra_css: &str,
    viewport_size: LogicalSize,
) -> Result<(LayoutTree<azul_css::props::basic::FontRef>, BTreeMap<usize, LogicalPosition>), LayoutError> {
    // 1. Create DOM from HTML - Note: azul_core::xml::dom_from_str is a mock helper
    // In a real scenario you would parse this properly.
    let full_html = format!("<style>{}</style>{}", extra_css, html_body);
    let mut dom = azul_core::dom::Dom::div()
        .with_child(azul_core::dom::Dom::text(full_html.chars().collect::<String>()));

    // 2. Create CSS
    let css = Css::new_from_string(extra_css).unwrap_or_default();

    // 3. Create StyledDom
    let styled_dom = StyledDom::new(&mut dom, CssApiWrapper::new(css, &Default::default()));

    // 4. Set up layout context
    let mut layout_cache = LayoutCache::default();
    let mut text_cache = TextLayoutCache::new();
    let font_manager = FontManager::new(FcFontCache::default()).unwrap();
    let viewport = LogicalRect::new(LogicalPosition::zero(), viewport_size);

    // 5. Run layout (we pass an empty counter_manager for these tests)
    layout_document(
        &mut layout_cache,
        &mut text_cache,
        styled_dom,
        viewport,
        &font_manager,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &mut crate::managers::counters::CounterManager::new(),
        &mut None,
        None,
        &RendererResources::default(),
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
    )?;

    // 6. Return results
    let tree = layout_cache.tree.ok_or(LayoutError::InvalidTree)?;
    Ok((tree, layout_cache.calculated_positions))
}

// --- EXISTING TESTS ---

#[test]
fn test_auto_sizing() {
    let (tree, positions) = layout_test_html(
        r#"<div style="width: auto; height: auto; font-size: 20px;">Auto Sized</div>"#,
        "",
        LogicalSize::new(800.0, 600.0),
    ).unwrap();

    let div_node = &tree.nodes[2]; // body -> div
    let div_size = div_node.used_size.unwrap();

    assert!(div_size.width > 0.0, "Auto width should be based on content, not zero.");
    assert!(div_size.height > 0.0, "Auto height should be based on content, not zero.");
}

#[test]
fn test_explicit_zero_sizing() {
    let (tree, positions) = layout_test_html(
        r#"<div style="width: 0px; height: 0px;">Hidden</div>"#,
        "",
        LogicalSize::new(800.0, 600.0),
    ).unwrap();

    let div_node = &tree.nodes[2]; // body -> div
    let div_size = div_node.used_size.unwrap();

    assert_eq!(div_size.width, 0.0, "Explicit width: 0px should be respected.");
    assert_eq!(div_size.height, 0.0, "Explicit height: 0px should be respected.");
}

#[test]
fn test_block_layout_spacing() {
    let (tree, positions) = layout_test_html(
        r#"<h1>Header</h1><p>Paragraph</p>"#,
        "h1, p { height: 40px; }",
        LogicalSize::new(800.0, 600.0),
    ).unwrap();

    let h1_pos = positions.get(&4).unwrap();
    let p_pos = positions.get(&5).unwrap();

    println!("H1 position: {:?}", h1_pos);
    println!("P position: {:?}", p_pos);

    assert!(p_pos.y > h1_pos.y, "Paragraph (y={}) should be positioned below the H1 element (y={})", p_pos.y, h1_pos.y);
    assert!(p_pos.y >= h1_pos.y + 40.0, "Paragraph should be at least 40px below H1's start.");
}

#[test]
fn test_font_size_from_style_tag() {
    let (tree, _) = layout_test_html(
        r#"<h1>Header Text</h1>"#,
        "h1 { font-size: 32px; }",
        LogicalSize::new(800.0, 600.0),
    ).unwrap();

    let text_node_layout = &tree.nodes[3].inline_layout_result.as_ref().unwrap();
    let first_glyph_run = text_node_layout.items.first().unwrap();

    let font_size = match &first_glyph_run.item {
        crate::text3::cache::ShapedItem::Cluster(c) => c.style.font_size_px,
        _ => 0.0,
    };

    assert_eq!(font_size, 32.0, "Font size from <style> tag should be 32px, not default 16px.");
}

// --- NEW REGRESSION TESTS ---

#[test]
fn test_percentage_width_resolution() {
    let (tree, _) = layout_test_html(
        r#"<div style="width: 400px;"><p style="width: 50%;"></p></div>"#,
        "",
        LogicalSize::new(800.0, 600.0),
    ).unwrap();

    // Node indices: 0=root, 1=style, 2=div, 3=p
    let p_node = &tree.nodes[3];
    let p_size = p_node.used_size.unwrap();

    // The <p> tag's width should be 50% of its container (the 400px div).
    assert_eq!(p_size.width, 200.0, "Percentage width should be resolved against the container's width.");
}

#[test]
fn test_box_sizing_border_box() {
    let (tree, _) = layout_test_html(
        r#"<div style="width: 200px; padding: 20px; box-sizing: border-box;">Text</div>"#,
        "",
        LogicalSize::new(800.0, 600.0),
    ).unwrap();

    let div_node = &tree.nodes[2];
    let div_size = div_node.used_size.unwrap();

    // With border-box, the padding is included in the total width.
    assert_eq!(div_size.width, 200.0, "With border-box, padding should not increase the final used width.");
}

#[test]
fn test_nested_block_stacking() {
    let (tree, positions) = layout_test_html(
        r#"<div style="padding: 10px;">
                <p style="height: 50px;"></p>
                <p style="height: 50px;"></p>
           </div>"#,
        "",
        LogicalSize::new(800.0, 600.0),
    ).unwrap();
    
    // Node indices: 0=root, 1=style, 2=div, 3=p1, 4=p2
    let div_pos = positions.get(&2).unwrap();
    let p1_pos = positions.get(&3).unwrap();
    let p2_pos = positions.get(&4).unwrap();

    // p1 should be positioned at the start of the div's content box (after padding).
    assert_eq!(p1_pos.y, div_pos.y + 10.0, "First child should be offset by parent's padding.");
    
    // p2 should be positioned directly after p1.
    assert_eq!(p2_pos.y, p1_pos.y + 50.0, "Second child should stack directly after the first.");
}

#[test]
fn test_margin_collapsing_between_siblings() {
    let (tree, positions) = layout_test_html(
        r#"<p style="height: 20px; margin-bottom: 20px;"></p>
           <p style="height: 20px; margin-top: 30px;"></p>"#,
        "",
        LogicalSize::new(800.0, 600.0),
    ).unwrap();

    // Node indices: 0=root, 1=style, 2=p1, 3=p2
    let p1_pos = positions.get(&2).unwrap();
    let p2_pos = positions.get(&3).unwrap();

    // The space between the elements should be max(margin-bottom, margin-top), which is 30px.
    // So p2 should start at p1.y + p1.height + collapsed_margin.
    let expected_p2_y = p1_pos.y + 20.0 + 30.0;
    
    assert_eq!(
        p2_pos.y,
        expected_p2_y,
        "Margins between siblings should collapse to the larger of the two."
    );
}

#[test]
fn test_deep_font_size_inheritance() {
     let (tree, _) = layout_test_html(
        r#"<div><p><span>Deep Text</span></p></div>"#,
        "body { font-size: 24px; }",
        LogicalSize::new(800.0, 600.0),
    ).unwrap();

    // The text content is inside the <span>, which is inside the <p>, inside the <div>.
    // We expect the final text run to inherit the font-size from the body.
    // Node indices: 0=root, 1=style, 2=div, 3=p, 4=span, 5=text
    // The inline layout will be on the block container, which is the <p> tag (node 3)
    let text_node_layout = &tree.nodes[3].inline_layout_result.as_ref().unwrap();
    let first_glyph_run = text_node_layout.items.first().unwrap();

    let font_size = match &first_glyph_run.item {
        crate::text3::cache::ShapedItem::Cluster(c) => c.style.font_size_px,
        _ => 0.0,
    };
    
    assert_eq!(font_size, 24.0, "Font size should be inherited through multiple levels from the body.");
}

#[test]
fn test_em_font_size_resolution() {
     let (tree, _) = layout_test_html(
        r#"<div style="font-size: 20px;"><p style="font-size: 1.5em;">Bigger Text</p></div>"#,
        "",
        LogicalSize::new(800.0, 600.0),
    ).unwrap();

    // The <p> tag's font-size is relative to its parent `div`.
    // We expect 20px * 1.5 = 30px.
    // The inline layout is on the <p> tag itself (node 3)
    let text_node_layout = &tree.nodes[3].inline_layout_result.as_ref().unwrap();
    let first_glyph_run = text_node_layout.items.first().unwrap();

    let font_size = match &first_glyph_run.item {
        crate::text3::cache::ShapedItem::Cluster(c) => c.style.font_size_px,
        _ => 0.0,
    };
    
    assert_eq!(font_size, 30.0, "1.5em font size on a 20px parent should resolve to 30px.");
}
```