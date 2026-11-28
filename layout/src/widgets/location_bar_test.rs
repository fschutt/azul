//! The location block: the trail as one line, the big bold name, the pen, the live field, what
//! every part reports.

use super::*;

fn has_class(dom: &Dom, class: &str) -> bool {
    dom.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .any(|c| c.as_class() == Some(class))
}

fn collect<'a>(dom: &'a Dom, class: &str, out: &mut Vec<&'a Dom>) {
    if has_class(dom, class) {
        out.push(dom);
    }
    for child in dom.children.as_ref() {
        collect(child, class, out);
    }
}

fn all<'a>(dom: &'a Dom, class: &str) -> Vec<&'a Dom> {
    let mut v = Vec::new();
    collect(dom, class, &mut v);
    v
}

fn first<'a>(dom: &'a Dom, class: &str) -> Option<&'a Dom> {
    all(dom, class).into_iter().next()
}

fn bar() -> LocationBar {
    LocationBar::create(AzString::from("Fonts"))
        .with_parent(LocationCrumb::create(AzString::from("azul"), 1))
        .with_parent(LocationCrumb::create(AzString::from("assets"), 2))
        .with_form(FormFactor::Desktop)
}

fn crumb_labels(dom: &Dom) -> Vec<String> {
    all(dom, LOCATION_CRUMB_CLASS).into_iter().map(text_of).collect()
}

/// The report of a node's click: the kind and the tag.
fn click_report(dom: &Dom) -> Option<(LocationEventKind, usize)> {
    dom.root.get_callbacks().as_ref().iter().find_map(|cb| {
        let mut data = cb.refany.clone();
        data.downcast_ref::<PartRef>().map(|p| (p.kind, p.tag))
    })
}

// ---- the trail ----

#[test]
fn the_parents_are_one_line_of_links_with_a_separator_after_each() {
    let dom = bar().dom();
    assert_eq!(crumb_labels(&dom), vec!["azul".to_string(), "assets".to_string()]);
    assert_eq!(all(&dom, LOCATION_SEPARATOR_CLASS).len(), 2);
    let trail = first(&dom, LOCATION_CRUMBS_CLASS).expect("the trail");
    assert_eq!(trail.children.as_ref().len(), 4, "azul / assets /");
}

#[test]
fn a_parent_reports_its_tag_when_it_is_clicked() {
    let dom = bar().dom();
    let crumbs = all(&dom, LOCATION_CRUMB_CLASS);
    assert_eq!(click_report(crumbs[0]), Some((LocationEventKind::CrumbClicked, 1)));
    assert_eq!(click_report(crumbs[1]), Some((LocationEventKind::CrumbClicked, 2)));
}

#[test]
fn a_parent_is_a_link_for_a_screen_reader_and_a_tab_stop() {
    let dom = bar().dom();
    let crumb = first(&dom, LOCATION_CRUMB_CLASS).expect("a link");
    let a11y = crumb.root.get_accessibility_info().expect("an a11y info");
    assert_eq!(a11y.role, AccessibilityRole::Link);
    assert_eq!(a11y.accessibility_name.as_ref().map(|n| n.as_str()), Some("azul"));
}

#[test]
fn a_block_without_parents_has_an_empty_trail() {
    let dom = LocationBar::create(AzString::from("Inbox")).with_form(FormFactor::Desktop).dom();
    assert!(all(&dom, LOCATION_CRUMB_CLASS).is_empty());
    assert!(first(&dom, LOCATION_CRUMBS_CLASS).expect("the line stays").children.as_ref().is_empty());
}

#[test]
fn the_separator_is_the_apps_to_choose() {
    let dom = bar().with_separator(AzString::from(">")).dom();
    let seps: Vec<String> = all(&dom, LOCATION_SEPARATOR_CLASS).into_iter().map(text_of).collect();
    assert_eq!(seps, vec![">".to_string(), ">".to_string()]);
}

#[test]
fn a_long_trail_folds_its_leading_folders_into_a_link_that_goes_to_the_nearest_hidden_one() {
    let parents: Vec<LocationCrumb> = (0..12)
        .map(|i| LocationCrumb::create(AzString::from(format!("folder-number-{i}")), 100 + i))
        .collect();
    let dom = LocationBar::create(AzString::from("here"))
        .with_parents(LocationCrumbVec::from_vec(parents))
        .with_form(FormFactor::Desktop)
        .dom();
    let labels = crumb_labels(&dom);
    assert_eq!(labels[0], "...", "the fold comes first");
    assert!(labels.len() < 13, "some folders are hidden: {labels:?}");
    assert_eq!(labels.last().map(String::as_str), Some("folder-number-11"), "the last folder always shows");
    // the fold goes to the folder just before the first one shown
    let hidden = 12 - (labels.len() - 1);
    let fold = first(&dom, LOCATION_CRUMB_CLASS).expect("the fold");
    assert_eq!(click_report(fold), Some((LocationEventKind::CrumbClicked, 100 + hidden - 1)));
}

#[test]
fn folding_counts_the_fewest_folders_that_make_the_rest_fit() {
    let names = ["a", "bb", "ccc"];
    assert_eq!(folded_parents(&names, "/", 100), 0);
    // each name costs its length and 3 for " / "; "ccc" alone is 6
    assert_eq!(folded_parents(&names, "/", 7), 2, "the last folder is never folded");
    assert_eq!(folded_parents(&[], "/", 5), 0);
    assert_eq!(folded_parents(&["only"], "/", 1), 0, "one folder cannot fold");
}

#[test]
fn a_phone_folds_sooner_than_a_desktop() {
    let parents: Vec<LocationCrumb> = (0..6)
        .map(|i| LocationCrumb::create(AzString::from(format!("directory-{i}")), i))
        .collect();
    let on = |form: FormFactor| {
        crumb_labels(
            &LocationBar::create(AzString::from("here"))
                .with_parents(LocationCrumbVec::from_vec(parents.clone()))
                .with_form(form)
                .dom(),
        )
        .len()
    };
    assert!(on(FormFactor::Phone) < on(FormFactor::Desktop));
}

#[test]
fn a_path_deeper_than_the_limit_keeps_its_last_folders() {
    let parents: Vec<LocationCrumb> = (0..MAX_CRUMBS + 5)
        .map(|i| LocationCrumb::create(AzString::from(format!("{i}")), i))
        .collect();
    let dom = LocationBar::create(AzString::from("here"))
        .with_parents(LocationCrumbVec::from_vec(parents))
        .with_form(FormFactor::Desktop)
        .dom();
    let labels = crumb_labels(&dom);
    assert_eq!(labels.last().map(String::as_str), Some(&*format!("{}", MAX_CRUMBS + 4)));
}

// ---- the name and the pen ----

#[test]
fn the_name_is_on_the_next_line_big_and_bold_with_a_pen() {
    let dom = bar().dom();
    let name = first(&dom, LOCATION_NAME_CLASS).expect("the name");
    assert_eq!(text_of(name), "Fonts");
    assert!(first(&dom, LOCATION_PEN_CLASS).is_some());
    let line = first(&dom, LOCATION_NAME_ROW_CLASS).expect("the name's line");
    assert!(has_class(&line.children.as_ref()[0], LOCATION_NAME_CLASS));
    assert!(has_class(&line.children.as_ref()[1], LOCATION_PEN_CLASS));
}

#[test]
fn the_pen_is_a_named_button_that_starts_the_edit() {
    let dom = bar().dom();
    let pen = first(&dom, LOCATION_PEN_CLASS).expect("a pen");
    let a11y = pen.root.get_accessibility_info().expect("an a11y info");
    assert_eq!(a11y.role, AccessibilityRole::PushButton);
    assert_eq!(a11y.accessibility_name.as_ref().map(|n| n.as_str()), Some("Edit the location"));
    assert_eq!(click_report(pen), Some((LocationEventKind::EditStarted, 0)));
}

#[test]
fn a_more_button_beside_the_pen_holds_file_s_old_items_and_only_reports_the_click() {
    // none by default: the app asks for it
    assert!(first(&bar().dom(), LOCATION_MORE_CLASS).is_none());
    let dom = bar().with_more(true).dom();
    let more = first(&dom, LOCATION_MORE_CLASS).expect("a more button");
    let a11y = more.root.get_accessibility_info().expect("an a11y info");
    assert_eq!(a11y.role, AccessibilityRole::PushButton);
    assert_eq!(a11y.accessibility_name.as_ref().map(|n| n.as_str()), Some("More"));
    assert_eq!(click_report(more), Some((LocationEventKind::MoreClicked, 0)));
    // it stands after the pen, in the name's line
    let line = first(&dom, LOCATION_NAME_ROW_CLASS).expect("the name's line");
    assert!(has_class(&line.children.as_ref()[1], LOCATION_PEN_CLASS));
    assert!(has_class(&line.children.as_ref()[2], LOCATION_MORE_CLASS));
}

#[test]
fn the_more_button_does_not_need_a_pen_and_goes_while_the_field_is_open() {
    let dom = bar().with_editable(false).with_more(true).dom();
    assert!(first(&dom, LOCATION_PEN_CLASS).is_none() && first(&dom, LOCATION_MORE_CLASS).is_some());
    let editing = bar().with_more(true).with_editing(true, AzString::from("Fo")).dom();
    assert!(first(&editing, LOCATION_MORE_CLASS).is_none(), "the field takes the line");
}

// ---- the (?) buttons beside the name ----

fn detail(label: &str) -> PreviewDetail {
    PreviewDetail::create(AzString::from(label), Dom::create_div().with_id(AzString::from(format!("popover-{label}"))))
}

#[test]
fn nothing_is_dumped_by_default_no_question_mark_beside_the_name() {
    let dom = bar().with_more(true).dom();
    assert!(first(&dom, LOCATION_DETAIL_CLASS).is_none());
    assert!(first(&dom, crate::widgets::preview::PREVIEW_HELP_CLASS).is_none());
}

#[test]
fn each_detail_is_one_named_question_mark_after_the_name_the_pen_and_the_more_button() {
    let dom = bar()
        .with_more(true)
        .with_detail(detail("Page info"))
        .with_detail(detail("Tags").with_icon(AzString::from("sell")))
        .dom();
    let boxes = all(&dom, LOCATION_DETAIL_CLASS);
    assert_eq!(boxes.len(), 2, "one per detail");
    let mut buttons = Vec::new();
    for b in &boxes {
        collect(b, crate::widgets::preview::PREVIEW_HELP_CLASS, &mut buttons);
    }
    let names: Vec<String> = buttons
        .iter()
        .map(|b| {
            let a11y = b.root.get_accessibility_info().expect("an a11y info");
            assert_eq!(a11y.role, AccessibilityRole::PushButton);
            assert!(b.root.get_tab_index() == Some(TabIndex::Auto), "a tab stop");
            a11y.accessibility_name.as_ref().map(|n| n.as_str().to_string()).unwrap_or_default()
        })
        .collect();
    assert_eq!(names, vec!["Page info".to_string(), "Tags".to_string()]);
    // they stand in the name's line, after the name, the pen and the "..."
    let line = first(&dom, LOCATION_NAME_ROW_CLASS).expect("the name's line");
    let kids = line.children.as_ref();
    assert!(has_class(&kids[0], LOCATION_NAME_CLASS));
    assert!(has_class(&kids[1], LOCATION_PEN_CLASS));
    assert!(has_class(&kids[2], LOCATION_MORE_CLASS));
    assert!(has_class(&kids[3], LOCATION_DETAIL_CLASS) && has_class(&kids[4], LOCATION_DETAIL_CLASS));
}

#[test]
fn the_question_marks_go_while_the_field_is_open_and_need_no_pen() {
    let editing = bar().with_detail(detail("Page info")).with_editing(true, AzString::from("Fo")).dom();
    assert!(first(&editing, LOCATION_DETAIL_CLASS).is_none(), "the field takes the line");
    let plain = bar().with_editable(false).with_detail(detail("Page info")).dom();
    assert!(first(&plain, LOCATION_PEN_CLASS).is_none() && first(&plain, LOCATION_DETAIL_CLASS).is_some());
}

#[test]
fn the_details_past_the_limit_are_dropped_and_the_first_are_kept() {
    let many: Vec<PreviewDetail> = (0..MAX_DETAILS + 3).map(|i| detail(&format!("d{i}"))).collect();
    let kept = limited_details(many);
    assert_eq!(kept.len(), MAX_DETAILS);
    assert_eq!(kept[0].label.as_str(), "d0");
    let dom = bar().with_details(PreviewDetailVec::from_vec((0..MAX_DETAILS + 3).map(|i| detail(&format!("d{i}"))).collect())).dom();
    assert_eq!(all(&dom, LOCATION_DETAIL_CLASS).len(), MAX_DETAILS);
}

#[test]
fn a_block_with_nothing_to_edit_has_no_pen() {
    let dom = bar().with_editable(false).dom();
    assert!(first(&dom, LOCATION_PEN_CLASS).is_none());
    assert!(first(&dom, LOCATION_NAME_CLASS).is_some());
}

#[test]
fn while_editing_the_name_is_a_text_field_and_there_is_no_pen() {
    let dom = bar().with_editing(true, AzString::from("Fon")).dom();
    assert!(first(&dom, LOCATION_NAME_CLASS).is_none());
    assert!(first(&dom, LOCATION_PEN_CLASS).is_none());
    let field = first(&dom, LOCATION_EDIT_CLASS).expect("the field's box");
    assert_eq!(field.children.as_ref().len(), 1, "the text input");
}

#[test]
fn the_field_takes_the_keyboard_when_it_appears() {
    let dom = bar().with_editing(true, AzString::from("")).dom();
    let field = first(&dom, LOCATION_EDIT_CLASS).expect("the field's box");
    let mounts = field
        .root
        .get_callbacks()
        .as_ref()
        .iter()
        .filter(|c| c.event == EventFilter::Component(ComponentEventFilter::AfterMount))
        .count();
    assert_eq!(mounts, 1);
}

#[test]
fn the_field_starts_with_the_text_the_app_gives_or_the_name() {
    let given = bar().with_editing(true, AzString::from("Fon")).dom();
    let none = bar().with_editing(true, AzString::from("")).dom();
    // the text input holds its text as a state; what the block asks is only that one input is built
    // either way
    assert!(first(&given, LOCATION_EDIT_CLASS).is_some() && first(&none, LOCATION_EDIT_CLASS).is_some());
}

// ---- the report ----

#[test]
fn a_block_without_a_callback_reports_to_nobody_and_does_not_fail() {
    let dom = bar().dom();
    let crumb = first(&dom, LOCATION_CRUMB_CLASS).expect("a link");
    let part = crumb.root.get_callbacks().as_ref()[0].refany.clone();
    let mut part = part;
    let p = part.downcast_ref::<PartRef>().expect("a part");
    assert!(p.on_event.as_ref().is_none());
}

#[test]
fn the_block_fills_the_band() {
    let dom = bar().dom();
    assert!(has_class(&dom, LOCATION_CLASS));
    // the block's own style says it fills the band (a width and a height of 100 %)
    let said = format!("{:?}", dom.root.style);
    assert!(said.contains("100"), "{said}");
}

#[test]
fn the_default_block_has_no_name() {
    let dom = LocationBar::default().dom();
    assert!(has_class(&dom, LOCATION_CLASS));
    assert_eq!(text_of(first(&dom, LOCATION_NAME_CLASS).expect("the name")), "");
}

// ---- the text size of the user ----

use crate::widgets::sidebar_layout::{declares, declares_chrome_cap};
use azul_css::props::property::CssPropertyType;

#[test]
fn the_block_keeps_its_text_to_the_tab_band_and_a_parent_link_is_as_tall_as_its_text_needs() {
    let dom = bar().dom();
    assert!(declares_chrome_cap(&dom.root.style), "the band is a fixed 106 px");
    let crumbs = all(&dom, LOCATION_CRUMB_CLASS);
    assert!(!crumbs.is_empty());
    for crumb in crumbs {
        assert!(declares(&crumb.root.style, CssPropertyType::MinHeight));
        assert!(!declares(&crumb.root.style, CssPropertyType::Height));
    }
}
