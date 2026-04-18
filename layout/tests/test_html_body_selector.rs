//! Verify CSS `body { ... }` selector matches `Dom::create_body()`.
//!
//! Reads the resolved font-family via the compact cache reverse map (the same path
//! production layout code uses for Normal-state compact-encoded properties).

use azul_core::{dom::Dom, styled_dom::StyledDom};

/// Fetch the font-family hash for the given node from the compact cache.
fn font_family_hash_for(
    styled_dom: &StyledDom,
    node_id: azul_core::id::NodeId,
) -> u64 {
    let cache = &styled_dom.css_property_cache.ptr;
    let cc = cache
        .compact_cache
        .as_ref()
        .expect("compact cache should be built by StyledDom::create");
    cc.tier2b_text[node_id.index()].font_family_hash
}

fn font_families_for(
    styled_dom: &StyledDom,
    node_id: azul_core::id::NodeId,
) -> Option<azul_css::props::basic::font::StyleFontFamilyVec> {
    let cache = &styled_dom.css_property_cache.ptr;
    let cc = cache.compact_cache.as_ref()?;
    let fh = cc.tier2b_text[node_id.index()].font_family_hash;
    if fh == 0 {
        return None;
    }
    cc.font_hash_to_families.get(&fh).cloned()
}

#[test]
fn test_html_style_tag_with_body_selector() {
    let css_str = r#"
        body {
            font-family: "NotoSansJP", sans-serif;
            padding: 20px;
        }
    "#;

    let (css, _errors) = azul_css::parser2::new_from_str(css_str);
    let mut dom = Dom::create_body();
    let styled_dom = StyledDom::create(&mut dom, css);

    let families = font_families_for(&styled_dom, azul_core::id::NodeId::ZERO)
        .expect("font-family should be found for body selector");
    assert_eq!(families.len(), 2, "Should have 2 font families");
}

#[test]
fn test_html_vs_body_node_type() {
    let css_str = r#"
        body {
            font-family: Arial, sans-serif;
        }
    "#;

    let (css, _errors) = azul_css::parser2::new_from_str(css_str);

    // Body element should match body selector
    let mut dom_body = Dom::create_body();
    let styled_dom_body = StyledDom::create(&mut dom_body, css.clone());
    let body_fh = font_family_hash_for(&styled_dom_body, azul_core::id::NodeId::ZERO);
    assert_ne!(body_fh, 0, "Body should have font-family from CSS");

    // Div element should NOT match body selector (div doesn't inherit here,
    // it's the root node)
    let mut dom_div = Dom::create_div();
    let styled_dom_div = StyledDom::create(&mut dom_div, css);
    let div_fh = font_family_hash_for(&styled_dom_div, azul_core::id::NodeId::ZERO);
    assert_eq!(
        div_fh, 0,
        "Div (not body) should NOT match `body {{ font-family }}` selector"
    );
}
