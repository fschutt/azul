// Tests for CSS attribute selectors: [attr], [attr="v"], [attr~="v"], etc.

use azul_css::css::{AttributeMatchOp, CssPathSelector};
use azul_css::parser2::{new_from_str, parse_attribute_selector};

fn extract_attr(css: &str) -> Vec<(String, AttributeMatchOp, Option<String>)> {
    let (result, _) = new_from_str(css);
    let mut out = Vec::new();
    for rule in result.rules() {
        for selector in rule.path.selectors.iter() {
            if let CssPathSelector::Attribute(a) = selector {
                out.push((
                    a.name.as_str().to_string(),
                    a.op,
                    a.value.as_ref().map(|v| v.as_str().to_string()),
                ));
            }
        }
    }
    out
}

#[test]
fn parse_exists() {
    let s = parse_attribute_selector("data-foo").expect("should parse");
    assert_eq!(s.name.as_str(), "data-foo");
    assert_eq!(s.op, AttributeMatchOp::Exists);
    assert!(s.value.as_ref().is_none());
}

#[test]
fn parse_eq_quoted() {
    let s = parse_attribute_selector("type=\"text\"").expect("should parse");
    assert_eq!(s.name.as_str(), "type");
    assert_eq!(s.op, AttributeMatchOp::Eq);
    assert_eq!(s.value.as_ref().unwrap().as_str(), "text");
}

#[test]
fn parse_eq_single_quoted() {
    let s = parse_attribute_selector("type='text'").expect("should parse");
    assert_eq!(s.value.as_ref().unwrap().as_str(), "text");
}

#[test]
fn parse_eq_unquoted() {
    let s = parse_attribute_selector("disabled=true").expect("should parse");
    assert_eq!(s.op, AttributeMatchOp::Eq);
    assert_eq!(s.value.as_ref().unwrap().as_str(), "true");
}

#[test]
fn parse_includes() {
    let s = parse_attribute_selector("class~=\"primary\"").expect("should parse");
    assert_eq!(s.op, AttributeMatchOp::Includes);
    assert_eq!(s.value.as_ref().unwrap().as_str(), "primary");
}

#[test]
fn parse_dashmatch() {
    let s = parse_attribute_selector("lang|=\"en\"").expect("should parse");
    assert_eq!(s.op, AttributeMatchOp::DashMatch);
    assert_eq!(s.value.as_ref().unwrap().as_str(), "en");
}

#[test]
fn parse_prefix() {
    let s = parse_attribute_selector("href^=\"https://\"").expect("should parse");
    assert_eq!(s.op, AttributeMatchOp::Prefix);
    assert_eq!(s.value.as_ref().unwrap().as_str(), "https://");
}

#[test]
fn parse_suffix() {
    let s = parse_attribute_selector("href$=\".pdf\"").expect("should parse");
    assert_eq!(s.op, AttributeMatchOp::Suffix);
    assert_eq!(s.value.as_ref().unwrap().as_str(), ".pdf");
}

#[test]
fn parse_substring() {
    let s = parse_attribute_selector("href*=\"example\"").expect("should parse");
    assert_eq!(s.op, AttributeMatchOp::Substring);
    assert_eq!(s.value.as_ref().unwrap().as_str(), "example");
}

#[test]
fn parse_with_whitespace_around() {
    let s = parse_attribute_selector("  type  =  \"text\"  ").expect("should parse");
    assert_eq!(s.name.as_str(), "type");
    assert_eq!(s.op, AttributeMatchOp::Eq);
    assert_eq!(s.value.as_ref().unwrap().as_str(), "text");
}

#[test]
fn parse_empty_rejected() {
    assert!(parse_attribute_selector("").is_none());
    assert!(parse_attribute_selector("   ").is_none());
}

#[test]
fn parse_unbalanced_quote_rejected() {
    assert!(parse_attribute_selector("type=\"text").is_none());
    assert!(parse_attribute_selector("type=text\"").is_none());
}

#[test]
fn full_css_attribute_exists() {
    let css = "div[data-foo] { color: red; }";
    let attrs = extract_attr(css);
    assert_eq!(attrs.len(), 1);
    assert_eq!(attrs[0].0, "data-foo");
    assert_eq!(attrs[0].1, AttributeMatchOp::Exists);
    assert!(attrs[0].2.is_none());
}

#[test]
fn full_css_attribute_eq() {
    let css = "input[type=\"text\"] { color: red; }";
    let attrs = extract_attr(css);
    assert_eq!(attrs.len(), 1);
    assert_eq!(attrs[0].0, "type");
    assert_eq!(attrs[0].1, AttributeMatchOp::Eq);
    assert_eq!(attrs[0].2.as_deref(), Some("text"));
}

#[test]
fn full_css_attribute_includes() {
    let css = "p[class~=\"primary\"] { color: red; }";
    let attrs = extract_attr(css);
    assert_eq!(attrs.len(), 1);
    assert_eq!(attrs[0].1, AttributeMatchOp::Includes);
}

#[test]
fn full_css_attribute_dashmatch() {
    let css = "p[lang|=\"en\"] { color: red; }";
    let attrs = extract_attr(css);
    assert_eq!(attrs.len(), 1);
    assert_eq!(attrs[0].1, AttributeMatchOp::DashMatch);
}
