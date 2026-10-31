//! Only a page break starts a new page: `break-before: column`, `avoid` and
//! the other values that force no page break keep the box on its page.
//!
//! CSS Fragmentation 3, 3.1: `page`, `left`, `right`, `recto`, `verso` (and
//! the legacy `always`) force a page break; `column` and `region` force a
//! break only in a column or a region, and `avoid` and its kin force none.
//! The paged layout broke the page before every box whose `break-before` was
//! anything but `auto`.

use crate::break_token_pages::run;

/// Two 100px blocks on 400px pages, the second with `break-before: {value}`.
fn pages_with_a_break_before(value: &str) -> usize {
    let html = format!(
        "<html><head><style>* {{ margin: 0; padding: 0; }} .p {{ height: 100px; }} .b {{ \
         height: 100px; break-before: {value}; }}</style></head><body><div class=\"p\">one</div>\
         <div class=\"b\">two</div></body></html>"
    );
    run(&html, 400.0).len()
}

#[test]
fn a_break_that_forces_no_page_break_keeps_the_box_on_its_page() {
    for value in ["column", "avoid", "avoid-page", "avoid-column", "region", "avoid-region"] {
        assert_eq!(
            pages_with_a_break_before(value),
            1,
            "`break-before: {value}` starts no page: both blocks fit one"
        );
    }
}

#[test]
fn a_page_break_starts_a_page() {
    for value in ["page", "always"] {
        assert_eq!(
            pages_with_a_break_before(value),
            2,
            "`break-before: {value}` starts the second block on a page of its own"
        );
    }
}
