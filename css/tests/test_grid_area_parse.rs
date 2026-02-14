use azul_css::css::Css;
use azul_css::props::property::{CssProperty, CssPropertyType};

#[test]
fn test_grid_template_areas_parsing() {
    let css_text = r#"
.container {
    display: grid;
    width: 100%;
    height: 560px;
    grid-template-columns: 200px 1fr 200px;
    grid-template-rows: 80px 1fr 60px;
    grid-template-areas:
        "header header header"
        "sidebar main aside"
        "footer footer footer";
}
.header {
    grid-area: header;
    background-color: #f093fb;
}
"#;

    let (css, warnings) = Css::from_string_with_warnings(css_text.to_string().into());

    for w in &warnings {
        eprintln!("WARNING: {:?}", w);
    }

    let warning_strs: Vec<String> = warnings.iter().map(|w| format!("{:?}", w)).collect();
    for w in &warning_strs {
        eprintln!("  WARNING: {}", w);
    }

    let has_grid_area_warning = warning_strs.iter().any(|w| {
        w.contains("grid-area") || w.contains("grid-template-areas")
    });
    assert!(
        !has_grid_area_warning,
        "grid-area or grid-template-areas should parse without warnings! Warnings: {:?}",
        warning_strs
    );

    // Check that grid-template-areas was actually parsed
    let rules: Vec<_> = css.stylesheets.as_ref().iter().flat_map(|s| s.rules.as_ref().iter()).collect();
    eprintln!("Parsed {} rules", rules.len());
    for rule in &rules {
        eprintln!("Rule: {:?}", rule.path);
        for decl in rule.declarations.as_ref() {
            eprintln!("  Decl: {:?}", decl);
        }
    }

    // Check the .container rule has grid-template-areas
    let container_decls: Vec<_> = rules.iter()
        .filter(|r| r.path.to_string().contains("container"))
        .flat_map(|r| r.declarations.as_ref().iter())
        .collect();
    let has_grid_template_areas = container_decls.iter().any(|d| {
        if let azul_css::css::CssDeclaration::Static(prop) = d {
            matches!(prop, CssProperty::GridTemplateAreas(_))
        } else {
            false
        }
    });
    assert!(has_grid_template_areas, "Container should have GridTemplateAreas property!");

    // Check the .header rule has grid-row and grid-column (from grid-area shorthand)
    let header_decls: Vec<_> = rules.iter()
        .filter(|r| r.path.to_string().contains("header"))
        .flat_map(|r| r.declarations.as_ref().iter())
        .collect();
    eprintln!("Header has {} declarations", header_decls.len());
    let has_grid_row = header_decls.iter().any(|d| {
        if let azul_css::css::CssDeclaration::Static(prop) = d {
            matches!(prop, CssProperty::GridRow(_))
        } else {
            false
        }
    });
    assert!(has_grid_row, "Header should have GridRow property (from grid-area shorthand)!");
}

#[test]
fn test_grid_area_key_lookup() {
    // Test the exact key lookup for grid-template-areas
    let map = azul_css::props::property::get_css_key_map();
    let found = azul_css::props::property::CssPropertyType::from_str("grid-template-areas", &map);
    assert_eq!(found, Some(CssPropertyType::GridTemplateAreas), "grid-template-areas should be found in CSS key map");

    let found_combined = azul_css::props::property::CombinedCssPropertyType::from_str("grid-area", &map);
    assert!(found_combined.is_some(), "grid-area should be found as combined CSS property");
}

/// Test that exact CSS from the XHTML file parses correctly
/// (This mimics what the XML parser's get_text_content returns)
#[test]
fn test_exact_xhtml_style_content() {
    // Content exactly as it appears between <style type="text/css">...</style> in the XHTML
    let css_text = r#"            * { margin: 0; padding: 0; }
            body {
                width: 800px;
                height: 600px;
                padding: 20px;
                background: #1a1a2e;
            }
            .container {
                display: grid;
                width: 100%;
                height: 560px;
                grid-template-columns: 200px 1fr 200px;
                grid-template-rows: 80px 1fr 60px;
                grid-template-areas:
                    "header header header"
                    "sidebar main aside"
                    "footer footer footer";/* gap: 15px; */ margin-right: 15px; margin-bottom: 15px;
            }
            .header {
                grid-area: header;
                background: #f093fb;
            }
            .sidebar {
                grid-area: sidebar;
                background: #4facfe;
            }
            .main {
                grid-area: main;
                background: #43e97b;
                display: grid;
                grid-template-columns: repeat(3, 1fr);
                grid-template-rows: repeat(2, 1fr);/* gap: 10px; */ margin-right: 10px; margin-bottom: 10px;
                padding: 10px;
            }
            .card {
                background: #ffffff;
            }
            .aside {
                grid-area: aside;
                background: #fa709a;
            }
            .footer {
                grid-area: footer;
                background: #30cfd0;
            }"#;

    let (css, warnings) = Css::from_string_with_warnings(css_text.to_string().into());

    let warning_strs: Vec<String> = warnings.iter().map(|w| format!("{:?}", w)).collect();
    for w in &warning_strs {
        eprintln!("  WARNING: {}", w);
    }

    let has_grid_warning = warning_strs.iter().any(|w| {
        w.contains("grid-area") || w.contains("grid-template-areas")
    });
    assert!(
        !has_grid_warning,
        "Should parse without grid warnings! Got: {:?}",
        warning_strs
    );
}
