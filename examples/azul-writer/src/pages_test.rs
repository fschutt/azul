//! The document area: the flow by default, the page layout where it was asked for and a sheet fits,
//! the placeholder of a blank page.

use super::{shows_pages, Area};
use crate::app::View;

#[test]
fn the_text_flows_by_default_and_everywhere_unless_the_page_layout_was_asked_for() {
    assert!(!shows_pages(View::Flow, 1920.0, 1.0), "the flow is the flow on any window");
    assert!(!shows_pages(View::Flow, 411.0, 1.0));
    assert_eq!(View::default(), View::Flow);
}

#[test]
fn a_phone_shows_the_flow_because_an_a4_sheet_does_not_fit_it() {
    // The sheet is 794 px at 100 %: it ran out of a 411 dp phone and its text with it.
    assert!(!shows_pages(View::Pages, 411.0, 1.0));
    assert!(!shows_pages(View::Pages, 360.0, 1.0));
}

#[test]
fn a_window_wide_enough_for_the_sheet_shows_the_page_layout_that_was_asked_for() {
    assert!(shows_pages(View::Pages, 1280.0, 1.0));
    assert!(shows_pages(View::Pages, 842.0, 1.0), "794 px of sheet and 48 of canvas");
    assert!(!shows_pages(View::Pages, 841.0, 1.0));
}

#[test]
fn zooming_in_makes_the_sheet_wider_and_a_window_that_held_it_may_not() {
    assert!(shows_pages(View::Pages, 1280.0, 1.0));
    assert!(!shows_pages(View::Pages, 1280.0, 1.6), "1270 px of sheet and 48 of canvas");
}

#[test]
fn only_a_blank_document_says_where_to_write() {
    use super::shows_placeholder;
    assert!(shows_placeholder(0, 1), "a new document: no word, its one empty block");
    assert!(shows_placeholder(0, 0));
    assert!(!shows_placeholder(1, 1), "the first word takes the placeholder away");
    assert!(!shows_placeholder(0, 2), "two blocks (a heading, a table) are not blank even without a word");
}

#[test]
fn an_area_is_plain_data_about_its_window() {
    let area = Area { width: 411.0, phone: true, lift: 300.0 };
    assert_eq!(area, Area { lift: 300.0, ..area });
    assert_ne!(area, Area { phone: false, ..area });
}

mod structure {
    use std::path::PathBuf;

    use azul::{callbacks::RefAny, dom::Dom, widgets::RichTextDoc};

    use super::super::Area;
    use crate::{
        app::{AppState, View},
        model::DocumentModel,
        pages::document_area,
    };

    const PLACEHOLDER: &str = "Start writing...";

    fn state(markdown: &str) -> AppState {
        crate::l10n::in_english();
        let mut st = AppState::new(RefAny::new(()), PathBuf::from("data"), false);
        st.open.push(DocumentModel::from_doc(
            crate::model::new_document_id(),
            RichTextDoc::create_from_markdown(markdown),
            String::new(),
        ));
        st
    }

    fn phone() -> Area {
        Area { width: 411.0, phone: true, lift: 0.0 }
    }

    fn desktop() -> Area {
        Area { width: 1280.0, phone: false, lift: 0.0 }
    }

    /// Every text node under `dom` (itself first).
    fn texts(dom: &Dom, out: &mut Vec<String>) {
        if let Some(t) = dom.root.node_type.get_text().into_option() {
            out.push(t.as_str().to_string());
        }
        for child in dom.children.as_ref() {
            texts(child, out);
        }
    }

    fn says_where_to_write(dom: &Dom) -> bool {
        let mut all = Vec::new();
        texts(dom, &mut all);
        all.iter().any(|t| t == PLACEHOLDER)
    }

    #[test]
    fn a_blank_documents_phone_page_is_one_page_that_says_where_to_write() {
        let dom = document_area(&RefAny::new(()), &state(""), phone());
        assert_eq!(dom.children.as_ref().len(), 1, "one page in the scroller: the page of a writer");
        assert!(says_where_to_write(&dom), "it says Start writing...");
    }

    #[test]
    fn a_document_with_text_has_no_placeholder() {
        let dom = document_area(&RefAny::new(()), &state("Dear Ada,\n\nthe engine is ready."), phone());
        assert_eq!(dom.children.as_ref().len(), 1, "the page is still the one page");
        assert!(!says_where_to_write(&dom), "the text is the page now");
    }

    #[test]
    fn the_flow_of_a_blank_document_on_a_desktop_says_where_to_write_too() {
        let dom = document_area(&RefAny::new(()), &state(""), desktop());
        assert_eq!(dom.children.as_ref().len(), 1);
        assert!(says_where_to_write(&dom));
    }

    #[test]
    fn the_page_layout_of_a_blank_document_has_no_overlay() {
        // a wide window with the page layout asked for shows the A4 sheet, with the caret of the focused editor
        let mut st = state("");
        st.view = View::Pages;
        let dom = document_area(&RefAny::new(()), &st, desktop());
        assert!(!says_where_to_write(&dom));
    }

    #[test]
    fn no_open_document_is_the_empty_state_with_its_action() {
        let st = AppState::new(RefAny::new(()), PathBuf::from("data"), false);
        let dom = document_area(&RefAny::new(()), &st, desktop());
        let mut all = Vec::new();
        texts(&dom, &mut all);
        assert!(all.iter().any(|t| t == "No document is open"), "{all:?}");
    }
}
