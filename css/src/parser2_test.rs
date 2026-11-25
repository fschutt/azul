//! What the parser skips of an `@page` rule, and says so: a margin box
//! (`@top-center`, `@bottom-center`, ...) is parsed and dropped, and the style
//! sheet's author is told (a header, a footer or a page number is set up in the
//! page setup of the layout, not in CSS, yet).
//!
//! And what it caps and says so: a `font-variation-settings` value keeps at most
//! `MAX_VARIATION_AXES` axes, the style sheet's author is told when it had more.

use super::*;
use crate::props::basic::font_variation::MAX_VARIATION_AXES;

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

// ---------------------------------------------------------------------------
// a `font-variation-settings` value keeps at most `MAX_VARIATION_AXES` axes,
// and the style sheet's author is told when a value had more
// ---------------------------------------------------------------------------

/// `count` distinct axes, `"a000" 0, "a001" 1, ...` (at most a thousand).
fn axes_text(count: usize) -> String {
    (0..count)
        .map(|index| format!("\"a{index:03}\" {index}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The axis counts of the `TooManyVariationAxes` warnings among `warnings`.
fn variation_axes_counts(warnings: &[CssParseWarnMsg<'_>]) -> Vec<usize> {
    warnings
        .iter()
        .filter_map(|w| match &w.warning {
            CssParseWarnMsgInner::TooManyVariationAxes { count, .. } => Some(*count),
            _ => None,
        })
        .collect()
}

/// How many axes each `font-variation-settings` declaration of `css` holds.
fn kept_axes(css: &Css) -> Vec<usize> {
    css.rules
        .as_ref()
        .iter()
        .flat_map(|rule| rule.declarations.as_ref().iter())
        .filter_map(|declaration| match declaration {
            CssDeclaration::Static(CssProperty::FontVariationSettings(value)) => {
                value.get_property().map(|settings| settings.variations().len())
            }
            _ => None,
        })
        .collect()
}

/// The warning the cap earns: the declaration stands with its first axes, and the author is
/// told, once, how many it had and how many a node keeps.
#[test]
fn a_font_variation_list_over_the_cap_is_capped_and_warned_about_once() {
    let written = MAX_VARIATION_AXES + 6;
    let source = format!("p {{ font-variation-settings: {}; }}", axes_text(written));
    let (css, warnings) = new_from_str(&source);

    assert_eq!(variation_axes_counts(&warnings), [written], "{warnings:?}");
    assert_eq!(kept_axes(&css), [MAX_VARIATION_AXES]);
    let message = warnings
        .iter()
        .find(|w| matches!(w.warning, CssParseWarnMsgInner::TooManyVariationAxes { .. }))
        .map(|w| w.warning.to_string())
        .unwrap();
    assert!(
        message.contains("font-variation-settings")
            && message.contains(&written.to_string())
            && message.contains(&MAX_VARIATION_AXES.to_string()),
        "{message}"
    );
    // the warning is no copy of the value: a list of thousands of axes must not be printed
    assert!(message.len() < 200, "{} bytes", message.len());
}

/// The count is of DISTINCT tags, and `!important` is no part of the value.
#[test]
fn the_cap_counts_distinct_tags_and_reads_the_value_without_important() {
    let written = MAX_VARIATION_AXES + 1;
    let source = format!(
        "p {{ font-variation-settings: {} !important; }}",
        axes_text(written)
    );
    let (css, warnings) = new_from_str(&source);
    assert_eq!(variation_axes_counts(&warnings), [written], "{warnings:?}");
    assert_eq!(kept_axes(&css), [MAX_VARIATION_AXES]);

    // a hundred entries of three tags are three axes: nothing to say
    let repeated = (0..100)
        .map(|index| format!("\"a00{}\" {index}", index % 3))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!("p {{ font-variation-settings: {repeated}; }}");
    let (css, warnings) = new_from_str(&source);
    assert!(variation_axes_counts(&warnings).is_empty(), "{warnings:?}");
    assert_eq!(kept_axes(&css), [3]);
}

/// A list at the cap, `normal`, and a value that is no list say nothing about axes.
#[test]
fn a_font_variation_list_within_the_cap_and_the_values_that_are_no_list_earn_no_warning() {
    for value in [
        axes_text(MAX_VARIATION_AXES),
        axes_text(1),
        "normal".to_string(),
        "inherit".to_string(),
        "\"wght\"".to_string(),
        "var(--axes, \"wght\" 400)".to_string(),
    ] {
        let source = format!("p {{ font-variation-settings: {value}; }}");
        let (_css, warnings) = new_from_str(&source);
        assert!(variation_axes_counts(&warnings).is_empty(), "{value}: {warnings:?}");
    }
}

/// Only `font-variation-settings` has the cap: the same text in another property is not looked at.
#[test]
fn another_property_with_a_long_value_earns_no_axes_warning() {
    let source = format!(
        "p {{ font-feature-settings: {}; }}",
        axes_text(MAX_VARIATION_AXES + 6)
    );
    let (_css, warnings) = new_from_str(&source);
    assert!(variation_axes_counts(&warnings).is_empty(), "{warnings:?}");
}

/// A whole style sheet with the hostile value reads in time proportional to its length.
#[test]
fn a_style_sheet_with_fifty_thousand_axes_in_one_value_is_read_in_bounded_time() {
    let tags: String = (0..50_000_u32)
        .map(|index| {
            let (a, b, c) = (index / 2704, (index / 52) % 52, index % 52);
            let letter = |n: u32| {
                char::from(if n < 26 {
                    b'a' + u8::try_from(n).unwrap()
                } else {
                    b'A' + u8::try_from(n - 26).unwrap()
                })
            };
            format!("\"z{}{}{}\" 1", letter(a), letter(b), letter(c))
        })
        .collect::<Vec<_>>()
        .join(", ");

    let source = format!("p {{ font-variation-settings: {tags}; }}");
    let start = std::time::Instant::now();
    let (css, warnings) = new_from_str(&source);
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "50,000 axes took {elapsed:?}"
    );
    assert_eq!(variation_axes_counts(&warnings), [50_000], "{} warnings", warnings.len());
    assert_eq!(kept_axes(&css), [MAX_VARIATION_AXES]);
}
