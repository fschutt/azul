//! The File tab's location: the document's title, "Documents" over it.

use std::path::PathBuf;

use azul::{callbacks::RefAny, dom::Dom};

use super::*;
use crate::model::DocumentModel;

fn texts(dom: &Dom, out: &mut Vec<String>) {
    if let Some(t) = dom.root.node_type.get_text().into_option() {
        out.push(t.as_str().to_string());
    }
    for child in dom.children.as_ref() {
        texts(child, out);
    }
}

fn state() -> AppState {
    AppState::new(RefAny::new(()), PathBuf::from("data"), false)
}

#[test]
fn with_nothing_open_the_location_is_the_apps_own_name() {
    assert_eq!(place(&state()), "AzWriter");
}

#[test]
fn the_location_is_the_title_of_the_shown_document() {
    let mut st = state();
    st.open.push(DocumentModel::from_markdown("a".to_string(), "# Quarterly report\n\ntext\n"));
    st.open.push(DocumentModel::from_markdown("b".to_string(), "# Minutes\n"));
    st.active = 1;
    assert_eq!(place(&st), "Minutes");
}

#[test]
fn the_block_shows_the_title_big_and_documents_as_its_parent() {
    crate::l10n::in_english();
    let mut st = state();
    st.open.push(DocumentModel::from_markdown("a".to_string(), "# Quarterly report\n"));
    let dom = location(&RefAny::new(()), &st);
    let mut all = Vec::new();
    texts(&dom, &mut all);
    assert!(all.iter().any(|t| t == "Quarterly report"), "{all:?}");
    assert!(all.iter().any(|t| t == "Documents"), "{all:?}");
}
