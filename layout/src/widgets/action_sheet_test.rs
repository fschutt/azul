//! The quick-action sheet: phone only, rows of icon and label under a grab handle, a destructive row
//! in red, what each part reports.

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

fn texts(dom: &Dom, out: &mut Vec<String>) {
    if let azul_core::dom::NodeType::Text(t) = dom.root.get_node_type() {
        out.push(t.as_ref().as_str().to_string());
    }
    for child in dom.children.as_ref() {
        texts(child, out);
    }
}

fn text_of(dom: &Dom) -> String {
    let mut v = Vec::new();
    texts(dom, &mut v);
    v.join(" ")
}

fn report(dom: &Dom) -> Option<ActionSheetEvent> {
    dom.root.get_callbacks().as_ref().iter().find_map(|cb| {
        let mut data = cb.refany.clone();
        data.downcast_ref::<PartRef>().map(|p| p.event)
    })
}

fn files() -> ActionSheet {
    ActionSheet::create()
        .with_title(AzString::from("report_2026.pdf"), AzString::from("last modified 18 Jun"))
        .with_item(ActionSheetItem::create(AzString::from("Preview"), AzString::from("visibility"), 1))
        .with_item(ActionSheetItem::create(AzString::from("Download"), AzString::from("download"), 2))
        .with_item(ActionSheetItem::create(AzString::from("Rename"), AzString::from("edit"), 3))
        .with_item(ActionSheetItem::create(AzString::from("Move to trash"), AzString::from("delete"), 4).with_danger(true))
        .with_form(FormFactor::Phone)
        .with_open(true)
}

#[test]
fn a_sheet_is_closed_until_the_app_opens_it() {
    let sheet = ActionSheet::create();
    assert!(!sheet.open);
    assert!(!sheet.shows());
    let dom = sheet.with_form(FormFactor::Phone).dom();
    assert!(first(&dom, SHEET_PANEL_CLASS).is_none() && first(&dom, SHEET_SCRIM_CLASS).is_none());
}

#[test]
fn a_desktop_draws_no_sheet_even_when_it_is_open() {
    let dom = files().with_form(FormFactor::Desktop).dom();
    assert!(first(&dom, SHEET_PANEL_CLASS).is_none());
    assert!(dom.children.as_ref().is_empty());
    assert!(!files().with_form(FormFactor::Desktop).shows());
}

#[test]
fn an_open_sheet_on_a_phone_is_a_scrim_and_a_panel() {
    let dom = files().dom();
    assert!(has_class(&dom, SHEET_CLASS));
    assert_eq!(dom.children.as_ref().len(), 2);
    assert!(has_class(&dom.children.as_ref()[0], SHEET_SCRIM_CLASS));
    assert!(has_class(&dom.children.as_ref()[1], SHEET_PANEL_CLASS));
}

#[test]
fn the_panel_is_a_grab_handle_the_subject_and_the_rows() {
    let dom = files().dom();
    let panel = first(&dom, SHEET_PANEL_CLASS).expect("the panel");
    let kids = panel.children.as_ref();
    assert!(has_class(&kids[0], SHEET_HANDLE_CLASS));
    assert!(has_class(&kids[1], SHEET_HEADER_CLASS));
    assert_eq!(all(&dom, SHEET_ITEM_CLASS).len(), 4);
    let said = text_of(&dom);
    for word in ["report_2026.pdf", "last modified 18 Jun", "Preview", "Download", "Rename", "Move to trash"] {
        assert!(said.contains(word), "{word}: {said}");
    }
}

#[test]
fn a_sheet_of_rows_only_has_no_header() {
    let dom = ActionSheet::create()
        .with_item(ActionSheetItem::create(AzString::from("A"), AzString::from(""), 1))
        .with_form(FormFactor::Phone)
        .with_open(true)
        .dom();
    assert!(first(&dom, SHEET_HEADER_CLASS).is_none());
    assert_eq!(all(&dom, SHEET_ITEM_CLASS).len(), 1);
}

#[test]
fn only_a_destructive_row_is_red() {
    let dom = files().dom();
    let danger = all(&dom, SHEET_DANGER_CLASS);
    assert_eq!(danger.len(), 1);
    assert!(text_of(danger[0]).contains("Move to trash"));
}

#[test]
fn a_chosen_row_reports_its_place_and_its_tag() {
    let dom = files().dom();
    let rows = all(&dom, SHEET_ITEM_CLASS);
    let ev = report(rows[2]).expect("a row reports");
    assert_eq!((ev.kind, ev.index, ev.tag), (ActionSheetEventKind::ItemChosen, 2, 3));
}

#[test]
fn the_scrim_and_the_handle_dismiss_the_sheet() {
    let dom = files().dom();
    let scrim = report(first(&dom, SHEET_SCRIM_CLASS).expect("a scrim")).expect("reports");
    assert_eq!(scrim.kind, ActionSheetEventKind::Dismissed);
    let handle = report(first(&dom, SHEET_HANDLE_CLASS).expect("a handle")).expect("reports");
    assert_eq!(handle.kind, ActionSheetEventKind::Dismissed);
}

#[test]
fn escape_dismisses_it_too() {
    let dom = files().dom();
    let has_key = dom
        .root
        .get_callbacks()
        .as_ref()
        .iter()
        .any(|c| c.event == EventFilter::Focus(FocusEventFilter::VirtualKeyDown));
    assert!(has_key);
}

#[test]
fn a_row_that_cannot_run_reports_nothing_and_says_why() {
    let sheet = files().with_item(
        ActionSheetItem::create(AzString::from("See versions"), AzString::from("history"), 9)
            .with_disabled_reason(AzString::from("This file has no versions")),
    );
    let dom = sheet.dom();
    let rows = all(&dom, SHEET_ITEM_CLASS);
    let last = rows[4];
    assert!(report(last).is_none());
    let a11y = last.root.get_accessibility_info().expect("an a11y info");
    assert_eq!(
        a11y.description.as_ref().map(|d| d.as_str()),
        Some("This file has no versions")
    );
}

#[test]
fn every_row_is_a_named_button_and_the_sheet_a_named_dialog() {
    let dom = files().dom();
    for row in all(&dom, SHEET_ITEM_CLASS) {
        let a11y = row.root.get_accessibility_info().expect("an a11y info");
        assert_eq!(a11y.role, AccessibilityRole::PushButton);
        assert!(a11y.accessibility_name.as_ref().is_some_and(|n| !n.as_str().is_empty()));
    }
    let panel = first(&dom, SHEET_PANEL_CLASS).expect("the panel");
    let a11y = panel.root.get_accessibility_info().expect("an a11y info");
    assert_eq!(a11y.role, AccessibilityRole::Dialog);
    assert_eq!(a11y.accessibility_name.as_ref().map(|n| n.as_str()), Some("report_2026.pdf"));
}

#[test]
fn the_sheet_has_a_row_limit() {
    let mut sheet = ActionSheet::create().with_form(FormFactor::Phone).with_open(true);
    for i in 0..MAX_ITEMS + 6 {
        sheet.add_item(ActionSheetItem::create(AzString::from(format!("row {i}")), AzString::from(""), i));
    }
    let dom = sheet.dom();
    assert_eq!(all(&dom, SHEET_ITEM_CLASS).len(), MAX_ITEMS);
}

#[test]
fn a_row_is_a_finger_tall() {
    assert!(ITEM_HEIGHT >= 44.0);
}
