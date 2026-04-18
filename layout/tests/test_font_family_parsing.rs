//! Verify that font-family CSS values propagate into the compact cache.
//!
//! After the compact-cache refactor, `css_property_cache.get_font_family()` is a
//! slow-path API that returns `None` post-pruning for Normal-state compact-encoded
//! properties. Production code reads the font-family via the compact cache reverse
//! map — these tests do the same.

use azul_core::{
    dom::{Dom, IdOrClass},
    styled_dom::StyledDom,
};

/// Fetch the `StyleFontFamilyVec` for the given node via the compact cache.
fn font_families_for(
    styled_dom: &StyledDom,
    node_id: azul_core::id::NodeId,
) -> azul_css::props::basic::font::StyleFontFamilyVec {
    let cache = &styled_dom.css_property_cache.ptr;
    let cc = cache
        .compact_cache
        .as_ref()
        .expect("compact cache should be built by StyledDom::create");
    let fh = cc.tier2b_text[node_id.index()].font_family_hash;
    assert_ne!(fh, 0, "font-family hash should not be sentinel");
    cc.font_hash_to_families
        .get(&fh)
        .cloned()
        .expect("font-family should be in reverse map")
}

#[test]
fn test_font_family_parsing_simple() {
    let css_str = r#"
        .test-body {
            font-family: Arial, sans-serif;
        }
    "#;

    let (css, _errors) = azul_css::parser2::new_from_str(css_str);
    let mut dom =
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("test-body".into())].into());
    let styled_dom = StyledDom::create(&mut dom, css);

    let families = font_families_for(&styled_dom, azul_core::id::NodeId::ZERO);

    assert_eq!(families.len(), 2, "Should have 2 font families");

    let arial = families.get(0).expect("Should have first family");
    let arial_string = arial.as_string();
    assert!(
        arial_string.contains("Arial") || arial_string.contains("arial"),
        "First font should be Arial, got: {}",
        arial_string
    );

    let sans_serif = families.get(1).expect("Should have second family");
    let sans_string = sans_serif.as_string();
    assert!(
        sans_string.contains("sans-serif"),
        "Second font should be sans-serif, got: {}",
        sans_string
    );
}

#[test]
fn test_font_family_parsing_quoted() {
    let css_str = r#"
        .heading {
            font-family: "Times New Roman", Georgia, serif;
        }
    "#;

    let (css, _errors) = azul_css::parser2::new_from_str(css_str);
    let mut dom =
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("heading".into())].into());
    let styled_dom = StyledDom::create(&mut dom, css);

    let families = font_families_for(&styled_dom, azul_core::id::NodeId::ZERO);
    assert_eq!(families.len(), 3, "Should have 3 font families");
}

#[test]
fn test_font_family_parsing_japanese() {
    let css_str = r#"
        .recipe-body {
            font-family: "NotoSansJP", sans-serif;
        }
    "#;

    let (css, _errors) = azul_css::parser2::new_from_str(css_str);
    let mut dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("recipe-body".into())].into());
    let styled_dom = StyledDom::create(&mut dom, css);

    let families = font_families_for(&styled_dom, azul_core::id::NodeId::ZERO);
    assert_eq!(families.len(), 2, "Should have 2 font families");

    let noto = families.get(0).expect("Should have first family");
    let noto_string = noto.as_string();
    assert!(
        noto_string.contains("NotoSansJP") || noto_string.contains("notosansjp"),
        "First font should be NotoSansJP, got: {}",
        noto_string
    );
}
