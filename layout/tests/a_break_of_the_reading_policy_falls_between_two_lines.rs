//! A break of the reading policy falls between two LINES of a paragraph, whatever the paragraph's height.
//!
//! `BreakPolicy::atomic_lines` is documented as "never tear a line box across pages (snap to line boundaries)", and a reader
//! shows a page as a window over ONE laid-out column: a break through a line shows half of it at the foot of a page and the
//! other half at the head of the next (AzReader's `a_page_of_text_never_ends_inside_a_line`).
//!
//! The analysis took the rect of a text item for a line. But the `line_box` of a text item is the rect of the whole inline
//! formatting context (its lines together), so a paragraph of twelve lines was ONE range: a break inside it moved up to the
//! paragraph's top when that was within the push budget (a third of a page), else it stood where the naive interval put it,
//! in the middle of a line. A paged display list holds the layout of every paragraph, and the layout knows its lines: each
//! line is a range now.
//!
//! Every line is 20 px (`line-height: 20px`), a paragraph is twelve of them (`<br>` between words, so the font does not
//! change how many lines there are) and a page 105 px: no break of the plain interval is on a line.
//!
//! Needs a system font (a machine without one lays the text out as nothing): skipped there, a failure where the CI says it
//! has them (`AZ_REQUIRE_TEST_FONTS=1`).
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use azul_layout::solver3::{page_breaks::BreakPolicy, pagination::FakePageConfig};

use crate::pagination_dom_breaks::paginate_with_config;

const LINE: f32 = 20.0;
const PAGE: f32 = 105.0;
const LINES: usize = 12;

fn html(paragraphs: usize, lines: usize) -> String {
    let body: String = (0..paragraphs)
        .map(|p| {
            let words: Vec<String> = (0..lines).map(|l| format!("word{p}x{l}")).collect();
            format!("<p>{}</p>", words.join("<br>"))
        })
        .collect();
    format!(
        "<html><head><style>* {{ margin: 0; padding: 0; }} \
         body {{ width: 300px; font-size: 10px; line-height: {LINE}px; }}</style></head><body>{body}</body></html>"
    )
}

fn breaks(policy: BreakPolicy, paragraphs: usize, lines: usize) -> Vec<f32> {
    let config = FakePageConfig {
        break_policy: policy,
        ..FakePageConfig::new()
    };
    let (_cache, _dom, pagination) = paginate_with_config(&html(paragraphs, lines), 300.0, PAGE, config);
    pagination.breaks.as_ref().iter().map(|b| b.y).collect()
}

/// `true` when the text was laid out at all (the fixtures are 960 px tall or more: 8 breaks at least on pages of 105).
fn laid_out(breaks: &[f32]) -> bool {
    if breaks.len() >= 8 {
        return true;
    }
    assert!(
        std::env::var_os("AZ_REQUIRE_TEST_FONTS").is_none(),
        "AZ_REQUIRE_TEST_FONTS=1 but the text made {} breaks: no system font?",
        breaks.len()
    );
    eprintln!("skipped: the text made {} breaks: no system font on this machine", breaks.len());
    false
}

#[test]
fn a_break_inside_a_tall_paragraph_is_between_two_lines() {
    let policy = BreakPolicy {
        atomic_lines: true,
        ..BreakPolicy::default()
    };
    let breaks = breaks(policy, 4, LINES);
    if !laid_out(&breaks) {
        return;
    }
    for y in &breaks {
        assert!(
            (y / LINE - (y / LINE).round()).abs() < 0.01,
            "a break at {y} is {} px into a line: {breaks:?}",
            y % LINE
        );
    }
    // No page is taller than the page.
    let mut top = 0.0_f32;
    for y in &breaks {
        assert!(y - top <= PAGE + 0.01, "the page from {top} to {y} is taller than {PAGE}: {breaks:?}");
        top = *y;
    }
}

#[test]
fn the_plain_interval_still_cuts_through_a_line() {
    // The same text without the policy: the breaks are the naive 105, 210, ... (the policy is what the fix is about).
    let breaks = breaks(BreakPolicy::default(), 4, LINES);
    if !laid_out(&breaks) {
        return;
    }
    assert!(
        breaks.iter().any(|y| (y / LINE - (y / LINE).round()).abs() > 0.01),
        "the default policy is the plain interval and a break of it is inside a line: {breaks:?}"
    );
}

#[test]
fn widows_and_orphans_are_counted_in_lines_of_the_paragraph() {
    // Eight paragraphs of six lines (120 px each, 960 px in all). The default widows and orphans are two: a break inside a
    // paragraph leaves at least two of its lines above it and at least two below it. Page 105 puts the plain break at 105,
    // inside the first paragraph at its fifth line, which would leave ONE line (the sixth) for the next page.
    const SIX: usize = 6;
    let policy = BreakPolicy {
        atomic_lines: true,
        widows_orphans: true,
        ..BreakPolicy::default()
    };
    let breaks = breaks(policy, 8, SIX);
    if !laid_out(&breaks) {
        return;
    }
    let paragraph = SIX as f32 * LINE;
    for y in &breaks {
        let above = (y % paragraph) / LINE;
        // On a paragraph's edge there is nothing to count.
        if above < 0.01 {
            continue;
        }
        let below = SIX as f32 - above;
        assert!(
            (above - above.round()).abs() < 0.01,
            "a break at {y} is {above} lines into its paragraph: {breaks:?}"
        );
        assert!(
            above.round() >= 2.0 && below.round() >= 2.0,
            "a break at {y} leaves {above} lines above it and {below} below it in a paragraph of {SIX}: {breaks:?}"
        );
    }
}
