//! What the parser skips of an `@page` rule, and says so: a margin box
//! (`@top-center`, `@bottom-center`, ...) is parsed and dropped, and the style
//! sheet's author is told (a header, a footer or a page number is set up in the
//! page setup of the layout, not in CSS, yet).

use super::*;

/// The `MalformedStructure` messages among `warnings`.
fn structure_messages<'a>(warnings: &[CssParseWarnMsg<'a>]) -> Vec<&'a str> {
    warnings
        .iter()
        .filter_map(|w| match &w.warning {
            CssParseWarnMsgInner::MalformedStructure { message } => Some(*message),
            _ => None,
        })
        .collect()
}

/// The silent skip this replaces: `@page { @top-center { content: "x" } }`
/// drew nothing and said nothing.
#[test]
fn a_margin_box_in_a_page_rule_is_skipped_with_a_warning() {
    let (css, warnings) = new_from_str(
        "p { width: 1px; } @page { @top-center { content: \"x\"; } } div { height: 2px; }",
    );

    assert_eq!(css.rules.len(), 2, "the rules around the @page rule stand");
    let messages = structure_messages(&warnings);
    assert_eq!(messages.len(), 1, "{warnings:?}");
    assert!(
        messages[0].contains("margin boxes") && messages[0].contains("not supported"),
        "{}",
        messages[0]
    );
}

/// Every margin box is a rule of its own that is skipped: each is reported,
/// whichever of the sixteen it is.
#[test]
fn every_margin_box_is_reported() {
    let (css, warnings) = new_from_str(
        "@page { @top-left { content: \"a\"; } @bottom-center { content: \"b\"; } \
         @left-middle { content: \"c\"; } @right-top { content: \"d\"; } \
         @bottom-right-corner { content: \"e\"; } } div { height: 2px; }",
    );

    assert_eq!(css.rules.len(), 1);
    assert_eq!(structure_messages(&warnings).len(), 5, "{warnings:?}");
}

/// What is honoured (a footnote area) and what is skipped on purpose and
/// harmlessly (the page's own margins and size) is no finding.
#[test]
fn a_page_rule_with_its_margins_a_size_and_a_footnote_area_is_not_reported() {
    let (_css, warnings) = new_from_str(
        "@page { margin: 2cm; size: A4; @footnote { border-top: 1px solid red; } }",
    );

    assert!(warnings.is_empty(), "{warnings:?}");
}

/// A rule with a page selector is skipped whole, and says so once: its margin
/// boxes are not a second finding.
#[test]
fn a_margin_box_of_a_rule_skipped_whole_is_not_reported_again() {
    let (_css, warnings) =
        new_from_str("@page chapter { @top-center { content: \"x\"; } } div { height: 2px; }");

    let messages = structure_messages(&warnings);
    assert_eq!(messages.len(), 1, "{warnings:?}");
    assert!(messages[0].contains("page selector"), "{}", messages[0]);
}
