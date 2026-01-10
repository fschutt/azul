//! CSS @-rule and :lang() Pseudo-Class Parsing Tests
//!
//! Tests for @media CSS at-rule parsing and :lang() pseudo-class.
//!
//! NOTE: The azul_simplecss tokenizer has limited support for @-rules:
//! - `@media screen { ... }` works (simple identifier after @media)
//! - `@media (min-width: 800px) { ... }` does NOT work (parentheses not supported)
//!
//! For language support, use the standard CSS `:lang()` pseudo-class:
//! - `div:lang(de) { ... }` works correctly

use azul_css::css::{CssPathPseudoSelector, CssPathSelector};
#[allow(unused_imports)]
use azul_css::dynamic_selector::{
    DynamicSelector, LanguageCondition, MediaType, MinMaxRange, OrientationType,
};
use azul_css::parser2::new_from_str;

// ============================================================================
// @media parsing tests
// ============================================================================

#[test]
fn test_media_screen() {
    let css = r#"
        @media screen {
            div { color: red; }
        }
    "#;
    let (result, warnings) = new_from_str(css);

    // Check that we parsed one rule
    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1, "Expected 1 rule, got {}", rules.len());

    // Check that the rule has a Media(Screen) condition
    let rule = &rules[0];
    let conditions: Vec<_> = rule.conditions.iter().collect();
    assert_eq!(
        conditions.len(),
        1,
        "Expected 1 condition, got {:?}",
        conditions
    );

    match &conditions[0] {
        DynamicSelector::Media(MediaType::Screen) => {}
        other => panic!("Expected Media(Screen), got {:?}", other),
    }
}

#[test]
fn test_media_print() {
    let css = r#"
        @media print {
            div { color: black; }
        }
    "#;
    let (result, _warnings) = new_from_str(css);

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    let conditions: Vec<_> = rule.conditions.iter().collect();
    assert_eq!(conditions.len(), 1);

    match &conditions[0] {
        DynamicSelector::Media(MediaType::Print) => {}
        other => panic!("Expected Media(Print), got {:?}", other),
    }
}

#[test]
fn test_media_all() {
    let css = r#"
        @media all {
            div { color: blue; }
        }
    "#;
    let (result, _warnings) = new_from_str(css);

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    let conditions: Vec<_> = rule.conditions.iter().collect();
    assert_eq!(conditions.len(), 1);

    match &conditions[0] {
        DynamicSelector::Media(MediaType::All) => {}
        other => panic!("Expected Media(All), got {:?}", other),
    }
}

#[test]
#[ignore = "azul_simplecss tokenizer does not support parentheses in @media queries"]
fn test_media_min_width() {
    let css = r#"
        @media (min-width: 800px) {
            div { display: flex; }
        }
    "#;
    let (result, _warnings) = new_from_str(css);

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    let conditions: Vec<_> = rule.conditions.iter().collect();
    assert_eq!(conditions.len(), 1);

    match &conditions[0] {
        DynamicSelector::ViewportWidth(range) => {
            assert_eq!(range.min(), Some(800.0));
            assert!(range.max().is_none());
        }
        other => panic!("Expected ViewportWidth, got {:?}", other),
    }
}

#[test]
#[ignore = "azul_simplecss tokenizer does not support parentheses in @media queries"]
fn test_media_max_width() {
    let css = r#"
        @media (max-width: 600px) {
            div { display: block; }
        }
    "#;
    let (result, _warnings) = new_from_str(css);

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    let conditions: Vec<_> = rule.conditions.iter().collect();
    assert_eq!(conditions.len(), 1);

    match &conditions[0] {
        DynamicSelector::ViewportWidth(range) => {
            assert!(range.min().is_none());
            assert_eq!(range.max(), Some(600.0));
        }
        other => panic!("Expected ViewportWidth, got {:?}", other),
    }
}

#[test]
#[ignore = "azul_simplecss tokenizer does not support parentheses in @media queries"]
fn test_media_min_height() {
    let css = r#"
        @media (min-height: 500px) {
            div { height: 100%; }
        }
    "#;
    let (result, _warnings) = new_from_str(css);

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    let conditions: Vec<_> = rule.conditions.iter().collect();
    assert_eq!(conditions.len(), 1);

    match &conditions[0] {
        DynamicSelector::ViewportHeight(range) => {
            assert_eq!(range.min(), Some(500.0));
            assert!(range.max().is_none());
        }
        other => panic!("Expected ViewportHeight, got {:?}", other),
    }
}

#[test]
#[ignore = "azul_simplecss tokenizer does not support parentheses in @media queries"]
fn test_media_max_height() {
    let css = r#"
        @media (max-height: 1200px) {
            div { overflow: auto; }
        }
    "#;
    let (result, _warnings) = new_from_str(css);

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    let conditions: Vec<_> = rule.conditions.iter().collect();
    assert_eq!(conditions.len(), 1);

    match &conditions[0] {
        DynamicSelector::ViewportHeight(range) => {
            assert!(range.min().is_none());
            assert_eq!(range.max(), Some(1200.0));
        }
        other => panic!("Expected ViewportHeight, got {:?}", other),
    }
}

#[test]
#[ignore = "azul_simplecss tokenizer does not support parentheses in @media queries"]
fn test_media_orientation_portrait() {
    let css = r#"
        @media (orientation: portrait) {
            div { flex-direction: column; }
        }
    "#;
    let (result, _warnings) = new_from_str(css);

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    let conditions: Vec<_> = rule.conditions.iter().collect();
    assert_eq!(conditions.len(), 1);

    match &conditions[0] {
        DynamicSelector::Orientation(OrientationType::Portrait) => {}
        other => panic!("Expected Orientation(Portrait), got {:?}", other),
    }
}

#[test]
#[ignore = "azul_simplecss tokenizer does not support parentheses in @media queries"]
fn test_media_orientation_landscape() {
    let css = r#"
        @media (orientation: landscape) {
            div { flex-direction: row; }
        }
    "#;
    let (result, _warnings) = new_from_str(css);

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    let conditions: Vec<_> = rule.conditions.iter().collect();
    assert_eq!(conditions.len(), 1);

    match &conditions[0] {
        DynamicSelector::Orientation(OrientationType::Landscape) => {}
        other => panic!("Expected Orientation(Landscape), got {:?}", other),
    }
}

#[test]
#[ignore = "azul_simplecss tokenizer does not support parentheses in @media queries"]
fn test_media_compound_screen_and_min_width() {
    let css = r#"
        @media screen and (min-width: 1024px) {
            div { width: 960px; }
        }
    "#;
    let (result, _warnings) = new_from_str(css);

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    let conditions: Vec<_> = rule.conditions.iter().collect();
    assert_eq!(
        conditions.len(),
        2,
        "Expected 2 conditions for compound query"
    );

    // Should have both Media(Screen) and ViewportWidth
    let has_screen = conditions
        .iter()
        .any(|c| matches!(c, DynamicSelector::Media(MediaType::Screen)));
    let has_viewport = conditions
        .iter()
        .any(|c| matches!(c, DynamicSelector::ViewportWidth(r) if r.min() == Some(1024.0)));

    assert!(has_screen, "Expected Media(Screen) condition");
    assert!(has_viewport, "Expected ViewportWidth(min: 1024) condition");
}

#[test]
fn test_media_multiple_rules_in_block() {
    let css = r#"
        @media screen {
            div { color: red; }
            p { color: blue; }
        }
    "#;
    let (result, _warnings) = new_from_str(css);

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 2, "Expected 2 rules inside @media block");

    // Both rules should have the same condition
    for rule in &rules {
        let conditions: Vec<_> = rule.conditions.iter().collect();
        assert_eq!(conditions.len(), 1);
        match &conditions[0] {
            DynamicSelector::Media(MediaType::Screen) => {}
            other => panic!("Expected Media(Screen), got {:?}", other),
        }
    }
}

#[test]
fn test_no_conditions_for_regular_rules() {
    let css = r#"
        div { color: red; }
        p { color: blue; }
    "#;
    let (result, _warnings) = new_from_str(css);

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 2);

    for rule in &rules {
        let conditions: Vec<_> = rule.conditions.iter().collect();
        assert!(
            conditions.is_empty(),
            "Regular rules should have no conditions"
        );
    }
}

// ============================================================================
// :lang() pseudo-class parsing tests
// ============================================================================

#[test]
fn test_lang_pseudo_class_simple() {
    // Standard CSS :lang() pseudo-class selector
    let css = r#"div:lang(de) { color: red; }"#;
    let (result, warnings) = new_from_str(css);

    // Should parse without warnings
    assert!(
        warnings.is_empty(),
        "Expected no warnings, got: {:?}",
        warnings
    );

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1, "Expected 1 rule");

    let rule = &rules[0];
    let path_selectors: Vec<_> = rule.path.selectors.iter().collect();

    // Check that the selector contains a Lang pseudo-class
    let has_lang = path_selectors.iter().any(|sel| {
        matches!(sel, CssPathSelector::PseudoSelector(CssPathPseudoSelector::Lang(lang)) if lang.as_str() == "de")
    });
    assert!(
        has_lang,
        "Expected :lang(de) pseudo-selector in path: {:?}",
        path_selectors
    );
}

#[test]
fn test_lang_pseudo_class_with_region() {
    // BCP 47 tag with region (de-DE, en-US, etc.)
    let css = r#"p:lang(de-DE) { font-family: Arial; }"#;
    let (result, warnings) = new_from_str(css);

    assert!(
        warnings.is_empty(),
        "Expected no warnings, got: {:?}",
        warnings
    );

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    let path_selectors: Vec<_> = rule.path.selectors.iter().collect();

    let has_lang = path_selectors.iter().any(|sel| {
        matches!(sel, CssPathSelector::PseudoSelector(CssPathPseudoSelector::Lang(lang)) if lang.as_str() == "de-DE")
    });
    assert!(
        has_lang,
        "Expected :lang(de-DE) pseudo-selector in path: {:?}",
        path_selectors
    );
}

#[test]
fn test_lang_pseudo_class_quoted() {
    // :lang() with quoted value
    let css = r#"span:lang("en-US") { font-family: Helvetica; }"#;
    let (result, warnings) = new_from_str(css);

    assert!(
        warnings.is_empty(),
        "Expected no warnings, got: {:?}",
        warnings
    );

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    let path_selectors: Vec<_> = rule.path.selectors.iter().collect();

    let has_lang = path_selectors.iter().any(|sel| {
        matches!(sel, CssPathSelector::PseudoSelector(CssPathPseudoSelector::Lang(lang)) if lang.as_str() == "en-US")
    });
    assert!(
        has_lang,
        "Expected :lang(en-US) pseudo-selector (quotes stripped) in path: {:?}",
        path_selectors
    );
}

#[test]
fn test_lang_pseudo_class_single_quoted() {
    // :lang() with single-quoted value
    let css = r#"div:lang('fr') { color: blue; }"#;
    let (result, warnings) = new_from_str(css);

    assert!(
        warnings.is_empty(),
        "Expected no warnings, got: {:?}",
        warnings
    );

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    let path_selectors: Vec<_> = rule.path.selectors.iter().collect();

    let has_lang = path_selectors.iter().any(|sel| {
        matches!(sel, CssPathSelector::PseudoSelector(CssPathPseudoSelector::Lang(lang)) if lang.as_str() == "fr")
    });
    assert!(
        has_lang,
        "Expected :lang(fr) pseudo-selector (quotes stripped) in path: {:?}",
        path_selectors
    );
}

#[test]
fn test_lang_pseudo_class_multiple_rules() {
    // Multiple rules with different :lang() values
    let css = r#"
        div:lang(de) { color: black; }
        div:lang(en) { color: white; }
        div:lang(ja) { font-family: "MS Gothic"; }
    "#;
    let (result, warnings) = new_from_str(css);

    assert!(
        warnings.is_empty(),
        "Expected no warnings, got: {:?}",
        warnings
    );

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 3, "Expected 3 rules");

    // Check each rule has the correct :lang() selector
    let expected_langs = ["de", "en", "ja"];
    for (i, rule) in rules.iter().enumerate() {
        let path_selectors: Vec<_> = rule.path.selectors.iter().collect();

        let has_expected_lang = path_selectors.iter().any(|sel| {
            matches!(sel, CssPathSelector::PseudoSelector(CssPathPseudoSelector::Lang(lang)) if lang.as_str() == expected_langs[i])
        });
        assert!(
            has_expected_lang,
            "Rule {} should have :lang({}), got: {:?}",
            i, expected_langs[i], path_selectors
        );
    }
}

#[test]
fn test_lang_pseudo_class_combined_with_class() {
    // :lang() combined with class selector
    let css = r#".content:lang(de) { padding: 10px; }"#;
    let (result, warnings) = new_from_str(css);

    assert!(
        warnings.is_empty(),
        "Expected no warnings, got: {:?}",
        warnings
    );

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    let path_selectors: Vec<_> = rule.path.selectors.iter().collect();

    // Should have class "content" and :lang(de)
    let has_class = path_selectors
        .iter()
        .any(|sel| matches!(sel, CssPathSelector::Class(c) if c.as_str() == "content"));
    let has_lang = path_selectors.iter().any(|sel| {
        matches!(sel, CssPathSelector::PseudoSelector(CssPathPseudoSelector::Lang(lang)) if lang.as_str() == "de")
    });

    assert!(
        has_class,
        "Expected .content class in selector, got: {:?}",
        path_selectors
    );
    assert!(
        has_lang,
        "Expected :lang(de) pseudo-selector, got: {:?}",
        path_selectors
    );
}

#[test]
fn test_lang_pseudo_class_combined_with_other_pseudo() {
    // :lang() combined with :hover
    let css = r#"a:lang(en):hover { color: green; }"#;
    let (result, warnings) = new_from_str(css);

    assert!(
        warnings.is_empty(),
        "Expected no warnings, got: {:?}",
        warnings
    );

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    let path_selectors: Vec<_> = rule.path.selectors.iter().collect();

    // Should have both :lang(en) and :hover
    let has_lang = path_selectors.iter().any(|sel| {
        matches!(sel, CssPathSelector::PseudoSelector(CssPathPseudoSelector::Lang(lang)) if lang.as_str() == "en")
    });
    let has_hover = path_selectors.iter().any(|sel| {
        matches!(
            sel,
            CssPathSelector::PseudoSelector(CssPathPseudoSelector::Hover)
        )
    });

    assert!(
        has_lang,
        "Expected :lang(en) pseudo-selector, got: {:?}",
        path_selectors
    );
    assert!(
        has_hover,
        "Expected :hover pseudo-selector, got: {:?}",
        path_selectors
    );
}

// ============================================================================
// LanguageCondition matching tests
// ============================================================================

#[test]
fn test_language_condition_exact_match() {
    use azul_css::AzString;

    let cond = LanguageCondition::Exact(AzString::from("de-DE".to_string()));

    assert!(cond.matches("de-DE"));
    assert!(cond.matches("DE-de")); // case-insensitive
    assert!(!cond.matches("de"));
    assert!(!cond.matches("de-AT"));
    assert!(!cond.matches("en-US"));
}

#[test]
fn test_language_condition_prefix_match() {
    use azul_css::AzString;

    let cond = LanguageCondition::Prefix(AzString::from("de".to_string()));

    assert!(cond.matches("de"));
    assert!(cond.matches("de-DE"));
    assert!(cond.matches("de-AT"));
    assert!(cond.matches("de-CH"));
    assert!(cond.matches("DE-de")); // case-insensitive
    assert!(!cond.matches("d"));
    assert!(!cond.matches("deu"));
    assert!(!cond.matches("en"));
    assert!(!cond.matches("en-US"));
}

#[test]
fn test_language_condition_prefix_exact_language() {
    use azul_css::AzString;

    let cond = LanguageCondition::Prefix(AzString::from("de-DE".to_string()));

    assert!(cond.matches("de-DE"));
    assert!(cond.matches("de-DE-formal")); // Extended subtag
    assert!(!cond.matches("de"));
    assert!(!cond.matches("de-AT"));
}

// ============================================================================
// Mixed @media and regular rules
// ============================================================================

#[test]
fn test_mixed_media_and_regular_rules() {
    let css = r#"
        div { color: black; }

        @media screen {
            div { color: blue; }
        }

        p { color: green; }
    "#;
    let (result, _warnings) = new_from_str(css);

    let rules: Vec<_> = result.rules().collect();
    assert_eq!(rules.len(), 3);

    // First rule: no conditions
    assert!(rules[0].conditions.iter().count() == 0);

    // Second rule: @media screen condition
    let cond1: Vec<_> = rules[1].conditions.iter().collect();
    assert_eq!(cond1.len(), 1);
    assert!(matches!(
        cond1[0],
        DynamicSelector::Media(MediaType::Screen)
    ));

    // Third rule: no conditions
    assert!(rules[2].conditions.iter().count() == 0);
}

// ============================================================================
// MinMaxRange tests
// ============================================================================

#[test]
fn test_min_max_range_matches() {
    let range = MinMaxRange::new(Some(100.0), Some(500.0));

    assert!(!range.matches(99.0));
    assert!(range.matches(100.0));
    assert!(range.matches(300.0));
    assert!(range.matches(500.0));
    assert!(!range.matches(501.0));
}

#[test]
fn test_min_max_range_min_only() {
    let range = MinMaxRange::new(Some(100.0), None);

    assert!(!range.matches(99.0));
    assert!(range.matches(100.0));
    assert!(range.matches(1000000.0));
}

#[test]
fn test_min_max_range_max_only() {
    let range = MinMaxRange::new(None, Some(500.0));

    assert!(range.matches(0.0));
    assert!(range.matches(500.0));
    assert!(!range.matches(501.0));
}

#[test]
fn test_min_max_range_unbounded() {
    let range = MinMaxRange::new(None, None);

    assert!(range.matches(f32::MIN));
    assert!(range.matches(0.0));
    assert!(range.matches(f32::MAX));
}
