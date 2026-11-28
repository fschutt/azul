//! The rules of the flow editor: the size steps, the heading cycle and the vocabulary of the
//! formatting quick access.

use super::*;

#[test]
fn the_size_buttons_step_through_the_list_from_the_body_size() {
    // 0 is the body's own size, 11 pt
    assert_eq!(step_size(0, true), 24);
    assert_eq!(step_size(0, false), 21);
    assert_eq!(step_size(22, true), 24);
    assert_eq!(step_size(22, false), 21);
    assert_eq!(step_size(21, true), 22);
    assert_eq!(step_size(21, false), 20);
    // a long way up and down visits every size once
    let mut size = SIZES_HALF_POINTS[0];
    let mut seen = vec![size];
    loop {
        let next = step_size(size, true);
        if next == size {
            break;
        }
        seen.push(next);
        size = next;
    }
    assert_eq!(seen, SIZES_HALF_POINTS.to_vec());
}

#[test]
fn the_size_steps_hold_at_both_ends_and_between_listed_sizes() {
    let first = SIZES_HALF_POINTS[0];
    let last = SIZES_HALF_POINTS[SIZES_HALF_POINTS.len() - 1];
    assert_eq!(step_size(first, false), first);
    assert_eq!(step_size(last, true), last);
    // a size the list does not have steps to its neighbour in the direction asked
    assert_eq!(step_size(23, true), 24);
    assert_eq!(step_size(23, false), 22);
    // outside the list a step never jumps into it from the wrong side
    assert_eq!(step_size(200, true), 200);
    assert_eq!(step_size(200, false), last);
    assert_eq!(step_size(10, false), 10);
    assert_eq!(step_size(10, true), first);
    assert_eq!(step_size(u16::MAX, true), u16::MAX);
}

#[test]
fn the_list_of_sizes_is_ascending_and_holds_the_body_size() {
    assert!(SIZES_HALF_POINTS.windows(2).all(|w| w[0] < w[1]));
    assert!(SIZES_HALF_POINTS.contains(&BODY_HALF_POINTS));
}

#[test]
fn the_heading_button_cycles_paragraph_h1_h2_h3_and_back() {
    assert_eq!(heading_target(None), 1);
    assert_eq!(heading_target(Some(1)), 2);
    assert_eq!(heading_target(Some(2)), 3);
    // from 3 it asks for 3 again: the editor's toggle turns that back into a paragraph
    assert_eq!(heading_target(Some(3)), 3);
    assert_eq!(heading_target(Some(6)), 3);
}

#[test]
fn every_button_has_its_own_tag_below_the_limit_and_the_tag_finds_it_again() {
    let mut seen = std::collections::BTreeSet::new();
    for quick in QUICK_ALL.iter().copied() {
        let tag = quick.tag();
        assert!(tag < TAG_LIMIT, "{quick:?}: {tag}");
        assert!(seen.insert(tag), "{quick:?}: tag {tag} is used twice");
        assert_eq!(Quick::of_tag(tag), Some(quick));
    }
    assert_eq!(Quick::of_tag(QUICK_ALL.len()), None);
    assert_eq!(Quick::of_tag(TAG_LIMIT), None, "an app's own tags start at the limit");
    assert_eq!(Quick::of_tag(usize::MAX), None);
}

#[test]
fn every_button_names_itself_and_its_icon_once() {
    let mut icons = std::collections::BTreeSet::new();
    let mut keys = std::collections::BTreeSet::new();
    for quick in QUICK_ALL.iter().copied() {
        assert!(!quick.icon().is_empty());
        assert!(quick.key().starts_with("kit-flow-"), "{quick:?}: {}", quick.key());
        assert!(icons.insert(quick.icon()), "{quick:?}: icon {} is used twice", quick.icon());
        assert!(keys.insert(quick.key()), "{quick:?}: key {} is used twice", quick.key());
    }
}

#[test]
fn the_toggles_are_the_formats_the_lists_and_the_quote_and_the_actions_are_not() {
    for quick in [Quick::Bold, Quick::Italic, Quick::Underline, Quick::Strike, Quick::Code] {
        assert!(quick.is_toggle(), "{quick:?}");
    }
    for quick in [Quick::Bullets, Quick::Numbers, Quick::Checklist, Quick::Heading, Quick::Quote] {
        assert!(quick.is_toggle(), "{quick:?}");
    }
    for quick in [Quick::Undo, Quick::Redo, Quick::SizeDown, Quick::SizeUp, Quick::Indent, Quick::Outdent] {
        assert!(!quick.is_toggle(), "{quick:?}");
    }
}

#[test]
fn a_writers_rail_ends_with_bold_nearest_the_thumb() {
    assert_eq!(WRITER_QUICK.last(), Some(&Quick::Bold));
    assert_eq!(WRITER_QUICK.first(), Some(&Quick::Undo));
    let unique: std::collections::BTreeSet<usize> = WRITER_QUICK.iter().map(|q| q.tag()).collect();
    assert_eq!(unique.len(), WRITER_QUICK.len(), "no button twice");
}

// ---- the canvas ----

#[test]
fn the_margin_is_the_apps_else_the_form_factors() {
    let mut o = FlowOptions::default();
    assert_eq!(o.margin_px(), MARGIN_DESKTOP);
    o.phone = true;
    assert_eq!(o.margin_px(), MARGIN_PHONE);
    o.margin = 12.0;
    assert_eq!(o.margin_px(), 12.0, "the app's own wins");
    for bad in [f32::NAN, f32::INFINITY, -4.0, 0.0] {
        o.margin = bad;
        assert_eq!(o.margin_px(), MARGIN_PHONE, "{bad} is no margin");
    }
}

#[test]
fn the_lift_is_room_under_the_last_line_and_never_more_than_a_keyboard_is_tall() {
    let mut o = FlowOptions { phone: true, ..FlowOptions::default() };
    assert_eq!(o.bottom_px(), MARGIN_PHONE);
    o.lift = 320.0;
    assert_eq!(o.bottom_px(), MARGIN_PHONE + 320.0);
    o.lift = 1.0e9;
    assert_eq!(o.bottom_px(), MARGIN_PHONE + LIFT_MAX);
    for bad in [f32::NAN, f32::NEG_INFINITY, -50.0] {
        o.lift = bad;
        assert_eq!(o.bottom_px(), MARGIN_PHONE, "{bad} lifts nothing");
    }
}

#[test]
fn a_plain_page_is_a_column_on_the_windows_own_ground_and_a_sheet_lies_on_a_canvas() {
    let plain = FlowOptions::default();
    let scroller = plain.scroller_css();
    assert!(scroller.contains("overflow-y: auto"), "{scroller}");
    assert!(scroller.contains("background: system:background;"), "{scroller}");
    assert!(!scroller.contains("under-page"), "{scroller}");
    let paper = plain.paper_css();
    assert!(paper.contains("max-width: 760px"), "{paper}");
    assert!(paper.contains("width: 100%"), "{paper}");
    assert!(!paper.contains("box-shadow"), "a plain page has no edge: {paper}");

    let sheet = FlowOptions { paper: FlowPaper::Sheet, width: 794.0, min_height: 1123.0, ..FlowOptions::default() };
    let canvas = sheet.scroller_css();
    assert!(canvas.contains("system:under-page-background"), "{canvas}");
    let paper = sheet.paper_css();
    assert!(paper.contains("width: 794px"), "{paper}");
    assert!(paper.contains("min-height: 1123px"), "{paper}");
    assert!(paper.contains("box-shadow"), "{paper}");
}

#[test]
fn a_width_that_is_none_leaves_the_plain_column_as_wide_as_its_window() {
    for width in [0.0, -1.0, f32::NAN] {
        let o = FlowOptions { width, ..FlowOptions::default() };
        assert!(!o.paper_css().contains("max-width"), "{width}: {}", o.paper_css());
    }
}

#[test]
fn the_pages_font_reaches_the_box_and_the_placeholder_and_the_lift_the_bottom_padding() {
    let o = FlowOptions {
        font_css: "font-family: 'EB Garamond', serif;".to_string(),
        phone: true,
        lift: 300.0,
        blank: true,
        ..FlowOptions::default()
    };
    assert!(o.paper_css().contains("font-family: 'EB Garamond', serif;"));
    assert!(o.placeholder_css().contains("font-family: 'EB Garamond', serif;"));
    assert!(o.placeholder_css().contains(&format!("left: {MARGIN_PHONE}px")));
    // padding: top right bottom left-ish shorthand: margin margin bottom
    assert!(o.paper_css().contains(&format!("padding: {MARGIN_PHONE}px {MARGIN_PHONE}px {}px", MARGIN_PHONE + 300.0)));
}
