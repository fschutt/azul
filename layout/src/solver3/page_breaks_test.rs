//! The page cap of the break analysis: the interval breaks of a document of an
//! absurd height are not made past the pages a layout may have, in the plain
//! pass and in the pass that moves breaks around what must not be cut.

use azul_core::{
    dom::Dom,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    styled_dom::StyledDom,
};

use super::*;
use crate::solver3::display_list::{BorderRadius, DisplayListItem, ForcedBreak};

fn constraints(first: f32, normal: f32) -> PageConstraints {
    PageConstraints {
        first_page_content_height: first,
        normal_page_content_height: normal,
    }
}

fn ys(breaks: &[PageBreakPosition]) -> Vec<f32> {
    breaks.iter().map(|b| b.y).collect()
}

/// A DOM with no styles and a display list of one filled box `height` tall.
fn one_tall_box(height: f32) -> (StyledDom, DisplayList) {
    let mut dom = Dom::create_div();
    let (css, _warnings) = azul_css::parser2::new_from_str("");
    let styled = StyledDom::create(&mut dom, css);
    let mut list = DisplayList::default();
    list.items.push(DisplayListItem::Rect {
        bounds: LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(100.0, height)).into(),
        color: azul_css::props::basic::ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
        border_radius: BorderRadius::default(),
    });
    list.node_mapping.push(None);
    (styled, list)
}

/// The plain pass makes the first breaks and no more: the cap does not move
/// them, it only stops the pass, however tall the document is.
#[test]
fn the_interval_breaks_stop_at_the_cap() {
    let breaks = compute_page_breaks_from_forced_capped(&[], &constraints(100.0, 100.0), 1.0e12, 50);
    assert_eq!(breaks.len(), 50);
    let expected: Vec<f32> = (1..=50).map(|page| 100.0 * page as f32).collect();
    assert_eq!(ys(&breaks), expected);
}

/// A cap above the breaks a document needs changes nothing, and the unlimited
/// functions are the capped ones with no cap.
#[test]
fn a_cap_above_the_breaks_the_document_needs_changes_nothing() {
    let forced = [ForcedBreak {
        y: 250.0,
        causing_node: None,
    }];
    let plain = compute_page_breaks_from_forced(&forced, &constraints(100.0, 100.0), 1000.0);
    for max in [plain.len(), plain.len() + 1, 1000, usize::MAX] {
        assert_eq!(
            compute_page_breaks_from_forced_capped(&forced, &constraints(100.0, 100.0), 1000.0, max),
            plain,
            "a cap of {max}"
        );
    }
    assert!(plain.len() >= 9, "harness: the document needs some breaks");
}

/// Of the forced and the interval breaks together the first ones, in page order,
/// are kept: a forced break past the cap is not part of the pages.
#[test]
fn of_forced_and_interval_breaks_the_first_ones_are_kept() {
    let forced = [
        ForcedBreak {
            y: 150.0,
            causing_node: None,
        },
        ForcedBreak {
            y: 5000.0,
            causing_node: None,
        },
    ];
    let breaks = compute_page_breaks_from_forced_capped(&forced, &constraints(100.0, 100.0), 1.0e9, 4);
    assert_eq!(ys(&breaks), vec![100.0, 150.0, 200.0, 300.0]);
    assert_eq!(breaks[1].kind, BreakKind::Forced);
}

/// The pass that moves breaks around what must not be cut stops at the cap too.
#[test]
fn the_pass_with_avoid_rules_stops_at_the_cap() {
    let (styled, list) = one_tall_box(1.0e6);
    let policy = BreakPolicy {
        honor_break_inside: true,
        ..Default::default()
    };
    let input = PageBreakInput {
        display_list: &list,
        layout_tree: None,
        styled_dom: &styled,
        table_headers: None,
    };
    let breaks =
        compute_page_breaks_impl(&input, constraints(100.0, 100.0), &policy, None, None, None, 7);
    assert_eq!(ys(&breaks), (1..=7).map(|page| 100.0 * page as f32).collect::<Vec<_>>());
    // And with no cap it is the breaks it always was: the capped ones are the first
    // of them.
    let uncapped = compute_page_breaks(&input, &constraints(100.0, 100.0), &policy);
    assert!(uncapped.len() > 7);
    assert_eq!(uncapped[..7], breaks[..]);
}

/// The slicer config says how many breaks to look for, and the answer to "does
/// the document need more pages" is the one the cap gives.
#[test]
fn the_cap_is_as_many_breaks_as_pages_and_the_last_break_says_the_document_goes_on() {
    assert_eq!(max_breaks_for(0), usize::MAX);
    assert_eq!(max_breaks_for(1), 1);
    assert_eq!(max_breaks_for(10_000), 10_000);

    let at = |y: f32| PageBreakPosition {
        y,
        kind: BreakKind::Interval,
        causing_node: OptionNodeId::None,
    };
    let two = [at(100.0), at(200.0)];
    assert!(!needs_more_pages(&two, 0), "no cap");
    assert!(!needs_more_pages(&two, 3), "three pages are two breaks: the document is done");
    assert!(needs_more_pages(&two, 2), "two pages and a break after the second");
    assert!(needs_more_pages(&two, 1));
    assert!(!needs_more_pages(&[], 1));

    assert_eq!(SlicerConfig::simple(100.0).max_breaks(), 10_000);
    assert_eq!(SlicerConfig::simple(100.0).with_max_pages(0).max_breaks(), usize::MAX);
    assert_eq!(SlicerConfig::simple(100.0).with_max_pages(5).max_breaks(), 5);
}
