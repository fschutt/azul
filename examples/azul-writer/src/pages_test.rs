//! The document area on a window narrower than a sheet of paper.

use super::shows_as_web;
use crate::app::View;

#[test]
fn a_phone_shows_the_web_layout_because_an_a4_sheet_does_not_fit_it() {
    // The sheet is 794 px at 100 %: it ran out of a 411 dp phone and its text with it.
    assert!(shows_as_web(View::Print, 411.0, 1.0));
    assert!(shows_as_web(View::Print, 360.0, 1.0));
    assert!(shows_as_web(View::Web, 1920.0, 1.0), "the web layout is always the web layout");
}

#[test]
fn a_window_wide_enough_for_the_sheet_shows_the_layout_that_was_asked_for() {
    assert!(!shows_as_web(View::Print, 1280.0, 1.0));
    assert!(!shows_as_web(View::Print, 842.0, 1.0), "794 px of sheet and 48 of canvas");
    assert!(shows_as_web(View::Print, 841.0, 1.0));
}

#[test]
fn zooming_in_makes_the_sheet_wider_and_a_window_that_held_it_may_not() {
    assert!(!shows_as_web(View::Print, 1280.0, 1.0));
    assert!(shows_as_web(View::Print, 1280.0, 1.6), "1270 px of sheet and 48 of canvas");
}

#[test]
fn the_web_sheet_is_the_window_less_the_canvas_sides_and_fits_the_narrowest_phone() {
    use super::web_sheet_width;
    // a phone's sheet fills the screen: 8 dp of canvas on each side show its edge
    assert!((web_sheet_width(411.0, 1.0) - 395.0).abs() < 0.01, "a 411 dp phone leaves the sheet 395");
    assert!((web_sheet_width(360.0, 1.0) - 344.0).abs() < 0.01);
    assert!((web_sheet_width(320.0, 1.0) - 304.0).abs() < 0.01, "a 320 dp phone: the sheet was 320 wide and ran out of it");
    assert!((web_sheet_width(100.0, 1.0) - 240.0).abs() < 0.01, "never narrower than the least");
    // a window the phone layout does not take keeps 24 px of canvas on each side
    assert!((web_sheet_width(700.0, 1.0) - 652.0).abs() < 0.01);
    assert!((web_sheet_width(1920.0, 1.0) - 1200.0).abs() < 0.01, "at most 1200 px at 100 %");
    assert!((web_sheet_width(1920.0, 0.5) - 600.0).abs() < 0.01, "and the zoom's share of it");
}

#[test]
fn only_a_blank_document_says_where_to_write() {
    use super::shows_placeholder;
    assert!(shows_placeholder(0, 1), "a new document: no word, its one empty block");
    assert!(shows_placeholder(0, 0));
    assert!(!shows_placeholder(1, 1), "the first word takes the placeholder away");
    assert!(!shows_placeholder(0, 2), "two blocks (a heading, a table) are not blank even without a word");
}

mod structure {
    use std::path::PathBuf;

    use azul::{callbacks::RefAny, dom::Dom, widgets::RichTextDoc};

    use crate::{app::AppState, model::DocumentModel, pages::document_area};

    const PLACEHOLDER: &str = "Start writing...";

    fn state(markdown: &str) -> AppState {
        crate::l10n::in_english();
        let mut st = AppState::new(RefAny::new(()), PathBuf::from("data"), false);
        st.doc = Some(DocumentModel::from_doc(
            crate::model::new_document_id(),
            RichTextDoc::create_from_markdown(markdown),
            String::new(),
        ));
        st
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
    fn a_blank_documents_phone_sheet_is_one_page_that_says_where_to_write() {
        let dom = document_area(&RefAny::new(()), &state(""), 411.0);
        assert_eq!(dom.children.as_ref().len(), 1, "one sheet on the canvas: the page of a writer");
        assert!(says_where_to_write(&dom), "it says Start writing...");
    }

    #[test]
    fn a_document_with_text_has_no_placeholder() {
        let dom = document_area(&RefAny::new(()), &state("Dear Ada,\n\nthe engine is ready."), 411.0);
        assert_eq!(dom.children.as_ref().len(), 1, "the sheet is still the one page");
        assert!(!says_where_to_write(&dom), "the text is the page now");
    }

    #[test]
    fn the_print_layout_of_a_blank_document_has_no_overlay() {
        // a wide window shows the print layout: the A4 sheet, with the caret of the focused editor
        let dom = document_area(&RefAny::new(()), &state(""), 1280.0);
        assert!(!says_where_to_write(&dom));
    }
}
