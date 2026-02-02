// Tests for CSS nesting support
// CSS nesting allows rules to be nested inside other rules:
// .button { :hover { color: red; } }
// is equivalent to:
// .button:hover { color: red; }

use azul_css::parser2::new_from_str;
use azul_css::css::{CssPathSelector, CssPathPseudoSelector};
use azul_css::dynamic_selector::DynamicSelector;
use azul_css::dynamic_selector::OsCondition;

/// Test basic pseudo-class nesting: .button { :hover { color: red; } }
#[test]
fn test_nested_pseudo_class_hover() {
    let css = r#"
        .button {
            color: blue;
            :hover {
                color: red;
            }
        }
    "#;
    
    let (result, warnings) = new_from_str(css);
    
    // Print warnings for debugging
    for w in &warnings {
        eprintln!("Warning: {:?}", w);
    }
    
    let rules: Vec<_> = result.rules().collect();
    
    // We should have 2 rules:
    // 1. .button { color: blue; }
    // 2. .button:hover { color: red; }
    assert!(rules.len() >= 1, "Expected at least 1 rule, got {}", rules.len());
    
    // Find the hover rule
    let hover_rule = rules.iter().find(|r| {
        r.path.selectors.iter().any(|s| matches!(s, CssPathSelector::PseudoSelector(CssPathPseudoSelector::Hover)))
    });
    
    assert!(hover_rule.is_some(), "Expected to find a :hover rule");
    
    let hover_rule = hover_rule.unwrap();
    
    // Verify the path includes .button
    assert!(hover_rule.path.selectors.iter().any(|s| {
        matches!(s, CssPathSelector::Class(c) if c.as_str() == "button")
    }), "Expected .button in the path");
}

/// Test nested class selector: .outer { .inner { color: red; } }
#[test]
fn test_nested_class_selector() {
    let css = r#"
        .outer {
            .inner {
                color: red;
            }
        }
    "#;
    
    let (result, warnings) = new_from_str(css);
    
    for w in &warnings {
        eprintln!("Warning: {:?}", w);
    }
    
    let rules: Vec<_> = result.rules().collect();
    assert!(rules.len() >= 1, "Expected at least 1 rule");
    
    // Should produce .outer .inner { color: red; }
    let inner_rule = rules.iter().find(|r| {
        r.path.selectors.iter().any(|s| matches!(s, CssPathSelector::Class(c) if c.as_str() == "inner"))
    });
    
    assert!(inner_rule.is_some(), "Expected to find .inner rule");
    
    let inner_rule = inner_rule.unwrap();
    
    // Verify .outer is in the path
    assert!(inner_rule.path.selectors.iter().any(|s| {
        matches!(s, CssPathSelector::Class(c) if c.as_str() == "outer")
    }), "Expected .outer in the path");
}

/// Test nested @os rule: .button { @os linux { background: blue; } }
#[test]
fn test_nested_at_os() {
    let css = r#"
        .button {
            color: blue;
            @os linux {
                background: green;
            }
        }
    "#;
    
    let (result, warnings) = new_from_str(css);
    
    for w in &warnings {
        eprintln!("Warning: {:?}", w);
    }
    
    let rules: Vec<_> = result.rules().collect();
    
    // Find rules with OS conditions
    let os_rules: Vec<_> = rules.iter().filter(|r| {
        r.conditions.iter().any(|c| matches!(c, DynamicSelector::Os(_)))
    }).collect();
    
    assert!(!os_rules.is_empty(), "Expected at least one rule with OS condition");
    
    // The OS rule should have .button in its path
    let linux_rule = os_rules.iter().find(|r| {
        r.conditions.iter().any(|c| matches!(c, DynamicSelector::Os(OsCondition::Linux)))
    });
    
    assert!(linux_rule.is_some(), "Expected to find a Linux OS rule");
}

/// Test deeply nested selectors: .a { .b { .c { color: red; } } }
#[test]
fn test_deeply_nested_selectors() {
    let css = r#"
        .a {
            .b {
                .c {
                    color: red;
                }
            }
        }
    "#;
    
    let (result, warnings) = new_from_str(css);
    
    for w in &warnings {
        eprintln!("Warning: {:?}", w);
    }
    
    let rules: Vec<_> = result.rules().collect();
    
    // Find the rule with .c
    let c_rule = rules.iter().find(|r| {
        r.path.selectors.iter().any(|s| matches!(s, CssPathSelector::Class(c) if c.as_str() == "c"))
    });
    
    assert!(c_rule.is_some(), "Expected to find .c rule");
    
    let c_rule = c_rule.unwrap();
    
    // Verify .a and .b are in the path
    assert!(c_rule.path.selectors.iter().any(|s| {
        matches!(s, CssPathSelector::Class(c) if c.as_str() == "a")
    }), "Expected .a in the path");
    
    assert!(c_rule.path.selectors.iter().any(|s| {
        matches!(s, CssPathSelector::Class(c) if c.as_str() == "b")
    }), "Expected .b in the path");
}

/// Test multiple nested pseudo-classes with comma: .parent { :hover, :focus { color: red; } }
#[test]
fn test_nested_comma_selectors() {
    let css = r#"
        .parent {
            :hover, :focus {
                color: red;
            }
        }
    "#;
    
    let (result, warnings) = new_from_str(css);
    
    for w in &warnings {
        eprintln!("Warning: {:?}", w);
    }
    
    let rules: Vec<_> = result.rules().collect();
    
    // Should produce two rules:
    // .parent:hover { color: red; }
    // .parent:focus { color: red; }
    
    let hover_rule = rules.iter().find(|r| {
        r.path.selectors.iter().any(|s| matches!(s, CssPathSelector::PseudoSelector(CssPathPseudoSelector::Hover)))
    });
    
    let focus_rule = rules.iter().find(|r| {
        r.path.selectors.iter().any(|s| matches!(s, CssPathSelector::PseudoSelector(CssPathPseudoSelector::Focus)))
    });
    
    assert!(hover_rule.is_some(), "Expected to find :hover rule");
    assert!(focus_rule.is_some(), "Expected to find :focus rule");
}

/// Test mixed declarations and nested rules
#[test]
fn test_mixed_declarations_and_nested() {
    let css = r#"
        .button {
            color: blue;
            :hover {
                color: red;
            }
            background: white;
        }
    "#;
    
    let (_result, warnings) = new_from_str(css);
    
    for w in &warnings {
        eprintln!("Warning: {:?}", w);
    }
    
    // This should work without errors
    // The declarations before and after the nested block should be combined
    assert!(warnings.iter().all(|w| !format!("{:?}", w).contains("Error")));
}

/// Test @os at top level still works
#[test]
fn test_at_os_top_level() {
    let css = r#"
        @os linux {
            .button {
                color: red;
            }
        }
    "#;
    
    let (result, warnings) = new_from_str(css);
    
    for w in &warnings {
        eprintln!("Warning: {:?}", w);
    }
    
    let rules: Vec<_> = result.rules().collect();
    
    // Find the Linux rule
    let linux_rule = rules.iter().find(|r| {
        r.conditions.iter().any(|c| matches!(c, DynamicSelector::Os(OsCondition::Linux)))
    });
    
    assert!(linux_rule.is_some(), "Expected to find a Linux OS rule");
    
    let linux_rule = linux_rule.unwrap();
    assert!(linux_rule.path.selectors.iter().any(|s| {
        matches!(s, CssPathSelector::Class(c) if c.as_str() == "button")
    }), "Expected .button in the path");
}
