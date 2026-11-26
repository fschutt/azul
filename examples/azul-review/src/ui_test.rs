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
    let row = title_row(&reviewing("/work/proj"), &RefAny::new(0u8));
    assert!(has_id(&row, "__azreview_open-folder-bar"), "the button is in the bar the pen and the pages are in");
    assert!(
        texts(&row).iter().any(|t| t.contains("Open folder")),
        "it says what it does, not only a folder icon: {:?}",
        texts(&row)
    );
}

#[test]
fn the_title_row_names_the_folder_and_the_file_under_review() {
    let row = title_row(&reviewing("/work/proj"), &RefAny::new(0u8));
    assert!(has_id(&row, "__azreview_folder-chip"));
    let said = texts(&row);
    assert!(said.iter().any(|t| t == "proj"), "the folder's own name: {said:?}");
    assert!(said.iter().any(|t| t.contains("src/lib.rs")), "the file open: {said:?}");
}

#[test]
fn with_no_folder_the_title_row_names_none_but_still_offers_to_open_one() {
    let row = title_row(&empty(""), &RefAny::new(0u8));
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

// ---- what the window as a whole listens to ----

fn window_events(dom: &Dom, handler: usize) -> Vec<EventFilter> {
    dom.root.callbacks.as_slice().iter().filter(|c| c.callback.cb == handler).map(|c| c.event).collect()
}

#[test]
fn a_stroke_ends_when_the_window_loses_the_focus_not_when_an_element_does() {
    // `FocusLost` of a window listener is the BLUR of the element that had the focus: a press on the page blurs the file
    // that was picked in the tree (or the pen key that was clicked), and ended the stroke at its first point. The window
    // losing the focus is `WindowFocusLost`.
    let body = with_window_listeners(Dom::create_body(), &RefAny::new(0u8));
    let events = window_events(&body, crate::on_focus_lost as usize);
    assert_eq!(events, vec![EventFilter::Window(WindowEventFilter::WindowFocusLost)], "{events:?}");
}

#[test]
fn the_window_hears_the_keys_the_pointer_leaving_and_the_button_let_go_anywhere() {
    let body = with_window_listeners(Dom::create_body(), &RefAny::new(0u8));
    assert_eq!(
        window_events(&body, crate::on_key as usize),
        vec![EventFilter::Window(WindowEventFilter::VirtualKeyDown)]
    );
    assert_eq!(
        window_events(&body, crate::on_window_up as usize),
        vec![EventFilter::Window(WindowEventFilter::LeftMouseUp)]
    );
    assert_eq!(
        window_events(&body, crate::on_window_left as usize),
        vec![EventFilter::Window(WindowEventFilter::MouseLeave)]
    );
}
