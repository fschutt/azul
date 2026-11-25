//! The empty page and the window around it, built without a window.

use std::path::PathBuf;

use azul::{prelude::*, str::String as AzString};

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
