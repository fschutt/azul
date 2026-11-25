//! The empty page and the window around it, built without a window.

use std::path::PathBuf;

use azul::{dom::VirtualKeyCode, menu::{MenuItem, StringMenuItem}, prelude::*, str::String as AzString};

use super::*;

fn nodes(dom: &Dom) -> Vec<&Dom> {
    let mut out = vec![dom];
    for child in dom.children.as_slice() {
        out.extend(nodes(child));
    }
    out
}

fn texts(dom: &Dom) -> Vec<String> {
    nodes(dom)
        .into_iter()
        .filter_map(|n| n.root.node_type.get_text().into_option())
        .map(|t| t.as_str().to_string())
        .filter(|t| !t.trim().is_empty())
        .collect()
}

fn has_id(dom: &Dom, id: &str) -> bool {
    let id = AzString::from(id);
    nodes(dom).into_iter().any(|n| n.root.has_id(id.clone()))
}

fn empty(root: &str) -> AppState {
    crate::l10n::in_english();
    AppState::create(PathBuf::from(root), Vec::new(), RefAny::new(0u8), PathBuf::new())
}

#[test]
fn with_no_folder_the_empty_page_says_to_choose_one_and_has_the_button() {
    let page = sheet(&empty(""), &RefAny::new(0u8));
    let said = texts(&page).join(" ");
    assert!(said.contains("Choose a folder"), "{said}");
    assert!(!said.contains("No reviewable files under"), "no folder to look under: {said}");
    assert!(has_id(&page, "__azreview_open-folder-empty"), "the way out is in the empty page");
}

#[test]
fn a_folder_without_reviewable_files_is_named_in_the_empty_page() {
    let page = sheet(&empty("/work/empty"), &RefAny::new(0u8));
    let said = texts(&page).join(" ");
    assert!(said.contains("No reviewable files under /work/empty"), "{said}");
    assert!(has_id(&page, "__azreview_open-folder-empty"));
}

// ---- the title row: the way to a folder, and which folder it is ----

fn reviewing(root: &str) -> AppState {
    crate::l10n::in_english();
    let file = crate::code::SourceFile {
        path: PathBuf::from(root).join("src/lib.rs"),
        display: "src/lib.rs".to_string(),
        lines: vec!["fn a() {}".to_string()],
    };
    AppState::create(PathBuf::from(root), vec![file], RefAny::new(0u8), PathBuf::new())
}

#[test]
fn the_title_row_has_a_button_that_says_open_folder() {
    let row = top_bar(&reviewing("/work/proj"), &RefAny::new(0u8), 0.0);
    assert!(has_id(&row, "__azreview_open-folder-bar"), "the button is in the bar the pen and the pages are in");
    assert!(
        texts(&row).iter().any(|t| t.contains("Open folder")),
        "it says what it does, not only a folder icon: {:?}",
        texts(&row)
    );
}

#[test]
fn the_title_row_names_the_folder_and_the_file_under_review() {
    let row = top_bar(&reviewing("/work/proj"), &RefAny::new(0u8), 0.0);
    assert!(has_id(&row, "__azreview_folder-chip"));
    let said = texts(&row);
    assert!(said.iter().any(|t| t == "proj"), "the folder's own name: {said:?}");
    assert!(said.iter().any(|t| t.contains("src/lib.rs")), "the file open: {said:?}");
}

#[test]
fn with_no_folder_the_title_row_names_none_but_still_offers_to_open_one() {
    let row = top_bar(&empty(""), &RefAny::new(0u8), 0.0);
    assert!(!has_id(&row, "__azreview_folder-chip"));
    assert!(has_id(&row, "__azreview_open-folder-bar"));
}

#[test]
fn a_folder_is_named_by_its_own_name_and_its_tooltip_is_the_whole_path() {
    assert_eq!(
        folder_names(std::path::Path::new("/work/proj")),
        Some(("proj".to_string(), "/work/proj".to_string()))
    );
    assert_eq!(folder_names(std::path::Path::new("")), None, "no folder");
    assert_eq!(
        folder_names(std::path::Path::new("/")),
        Some(("/".to_string(), "/".to_string())),
        "a root has no name of its own: it is its own name"
    );
}

#[test]
fn the_session_menu_opens_a_folder_with_the_primary_modifier_and_o() {
    crate::l10n::in_english();
    let menu = menu_bar(&RefAny::new(0u8));
    let item = find_item(menu.items.as_ref(), "Open folder...").expect("Session > Open folder...");
    let OptionVirtualKeyCodeCombo::Some(combo) = &item.accelerator else {
        panic!("the item shows its key");
    };
    let keys = combo.keys.as_ref().to_vec();
    assert_eq!(keys.len(), 2, "{keys:?}");
    assert!(matches!(keys[1], VirtualKeyCode::O), "{keys:?}");
    let primary = if cfg!(target_os = "macos") { VirtualKeyCode::LWin } else { VirtualKeyCode::LControl };
    assert_eq!(format!("{:?}", keys[0]), format!("{primary:?}"), "Cmd on a Mac, Ctrl elsewhere");
}

fn find_item<'a>(items: &'a [MenuItem], label: &str) -> Option<&'a StringMenuItem> {
    for item in items {
        if let MenuItem::String(s) = item {
            if s.label.as_str() == label {
                return Some(s);
            }
            if let Some(found) = find_item(s.children.as_ref(), label) {
                return Some(found);
            }
        }
    }
    None
}

#[test]
fn the_empty_page_explains_what_the_app_does_and_has_a_large_open_folder_button() {
    let page = sheet(&empty(""), &RefAny::new(0u8));
    let said = texts(&page);
    assert!(said.iter().any(|t| t.contains("pages")), "a word about what AzReview shows: {said:?}");
    assert!(said.iter().any(|t| t.contains("Open folder")), "the button says so: {said:?}");
    assert!(has_id(&page, "__azreview_open-folder-empty"));
}

// ---- the pens in the top bar ----

fn pen_keys(dom: &Dom) -> Vec<&Dom> {
    nodes(dom)
        .into_iter()
        .filter(|n| {
            n.root.callbacks.as_slice().iter().any(|c| c.callback.cb == crate::on_pick_semantic as usize)
        })
        .collect()
}

#[test]
fn the_top_bar_has_one_key_per_pen_and_no_button_for_the_marker_or_the_comments() {
    let row = top_bar(&reviewing("/work/proj"), &RefAny::new(0u8), 0.0);
    assert_eq!(pen_keys(&row).len(), crate::pens::PENS.len(), "a key for each pen");
    let said = texts(&row);
    assert!(!said.iter().any(|t| t == "marker" || t == "pen" || t == "audio pen"), "no nib button: {said:?}");
    assert!(!has_id(&row, "__azreview_notes-toggle"), "no comment button");
}

#[test]
fn a_pen_key_takes_the_pen_on_a_click_and_the_next_nib_on_a_double_click() {
    let row = top_bar(&reviewing("/work/proj"), &RefAny::new(0u8), 0.0);
    for key in pen_keys(&row) {
        let handlers: Vec<(EventFilter, usize)> =
            key.root.callbacks.as_slice().iter().map(|c| (c.event, c.callback.cb)).collect();
        assert!(
            handlers.contains(&(EventFilter::Hover(HoverEventFilter::Click), crate::on_pick_semantic as usize)),
            "a click takes the pen: {handlers:?}"
        );
        assert!(
            handlers.contains(&(
                EventFilter::Hover(HoverEventFilter::DoubleClick),
                crate::on_pen_double_click as usize
            )),
            "a double click cycles its nib: {handlers:?}"
        );
    }
}

#[test]
fn only_the_pen_in_hand_shows_its_nib_and_every_key_is_named_with_its_meaning_and_nib() {
    let mut s = reviewing("/work/proj");
    s.pens.double_click(2);
    let row = top_bar(&s, &RefAny::new(0u8), 0.0);
    let keys = pen_keys(&row);
    // a pen's key is its meaning's icon and its digit; the pen in hand has the icon of its nib besides
    let parts: Vec<usize> = keys.iter().map(|k| k.children.as_slice().len()).collect();
    assert_eq!(parts, vec![2, 2, 3, 2, 2], "only the question pen, in hand, shows its nib");
    for key in keys {
        assert!(!key.root.accessibility.is_null(), "a pen key without a name");
    }
}

// ---- the code column the whiteboard's cutouts share with the page ----

#[test]
fn the_code_column_is_a_row_per_line_numbered_from_the_pages_first_line() {
    let lines = vec!["fn a() {}".to_string(), "fn b() {}".to_string(), "fn c() {}".to_string()];
    let col = page_code_column(41, &lines);
    assert_eq!(col.children.as_slice().len(), 3, "a row for each line");
    let said = texts(&col);
    assert_eq!(said, vec!["41", "fn a() {}", "42", "fn b() {}", "43", "fn c() {}"], "the number, then the line");
}

#[test]
fn the_numbers_the_cutout_renderer_lays_out_by_are_the_pages() {
    // One place for the page's measures: a cutout is as wide and as high per line as the page it was taken from.
    assert_eq!(LINE_H, 15.0);
    assert_eq!(PAGE_W, 1000.0);
    assert_eq!(GUTTER_W, 52.0);
    assert_eq!(PAGE_PAD, 32.0);
    assert_eq!(CODE_W, PAGE_W - MARGIN_GUTTER_W - PAGE_PAD);
}
