//! A footnote's call and marker are generated, and show the footnote
//! counter.
//!
//! CSS Generated Content for Paged Media, "Creating footnotes": "The
//! footnote element is removed from the flow, and a ::footnote-call
//! pseudo-element is inserted in its place [...]. A ::footnote-marker
//! pseudo-element, identifying the footnote, is placed at the beginning of
//! the footnote element. [...] The footnote counter is incremented."
//! Defaults: `::footnote-call { content: counter(footnote) }`,
//! `::footnote-marker { content: counter(footnote) }` followed by ". ".
//!
//! Neither was generated: the author had to write both marks.
//!
//! Pages are 800 x 200; lines are 20px, a `<br/>` ends each.

use std::collections::BTreeMap;

use azul_layout::{
    solver3::display_list::{DisplayList, DisplayListItem},
    text3::cache::{ShapedItem, UnifiedLayout},
};

use crate::a_footnote_is_drawn_at_the_bottom_of_the_page_of_its_anchor::{blocks, pages};

fn html(style: &str, body: &str) -> String {
    format!(
        "<html><head><style>* {{ margin: 0; padding: 0; }} p {{ line-height: 20px }} \
         .b {{ background: #0000ff }} .fn {{ float: footnote; line-height: 20px }} \
         .cols {{ column-count: 2; column-gap: 20px; width: 420px }} {style}</style></head>\
         <body>{body}</body></html>"
    )
}

/// The lines of every text block of `page`, top to bottom: the block's top,
/// and per line its text and where it starts across the page.
fn lines(page: &DisplayList) -> Vec<(f32, Vec<(String, f32)>)> {
    let mut blocks: Vec<(f32, Vec<(String, f32)>)> = page
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::TextLayout { layout, bounds, .. } => {
                let layout = layout.downcast_ref::<UnifiedLayout>()?;
                let mut by_line: BTreeMap<usize, (String, f32)> = BTreeMap::new();
                for positioned in &layout.items {
                    if let ShapedItem::Cluster(c) = &positioned.item {
                        let line = by_line
                            .entry(positioned.line_index)
                            .or_insert_with(|| (String::new(), f32::MAX));
                        line.0.push_str(c.text());
                        line.1 = line.1.min(bounds.inner().origin.x + positioned.position.x);
                    }
                }
                Some((bounds.inner().origin.y, by_line.into_values().collect()))
            }
            _ => None,
        })
        .collect();
    blocks.sort_by(|a, b| a.0.total_cmp(&b.0));
    blocks
}

/// The texts of the lines of `page`, top to bottom.
fn texts(page: &DisplayList) -> Vec<String> {
    lines(page)
        .into_iter()
        .flat_map(|(_, lines)| lines.into_iter().map(|(text, _)| text))
        .collect()
}

#[test]
fn the_call_and_the_marker_show_the_footnote_counter_in_document_order() {
    let (result, _, _) = pages(&html(
        "",
        "<p>one<span class=\"fn\">first</span><br/>two<span class=\"fn\">second</span></p>",
    ));
    assert_eq!(result.pages.len(), 1);
    assert_eq!(
        texts(&result.pages[0]),
        vec!["one1", "two2", "1. first", "2. second"],
        "each call on the line its note was taken from, each note under its number"
    );
    // The notes stand at the bottom of the page.
    let tops: Vec<f32> = lines(&result.pages[0]).into_iter().map(|(top, _)| top).collect();
    assert_eq!(tops, vec![0.0, 160.0, 180.0]);
}

#[test]
fn the_call_stands_on_the_line_of_a_column_its_note_was_taken_from() {
    // Four lines in two columns, two a column; the note is written at the
    // end of the fourth.
    let (result, _, _) = pages(&html(
        "",
        "<div class=\"cols\"><p>one<br/>two<br/>three<br/>four<span class=\"fn\">note</span></p>\
         </div>",
    ));
    let paragraph: Vec<(String, f32)> = lines(&result.pages[0])
        .into_iter()
        .filter(|(top, _)| *top < 100.0)
        .flat_map(|(_, lines)| lines)
        .collect();
    let (text, x) = paragraph.last().expect("the paragraph's last line");
    assert_eq!(text, "four1");
    assert!(*x >= 220.0, "in the right column: {paragraph:?}");
    assert_eq!(texts(&result.pages[0]).last().map(String::as_str), Some("1. note"));
}

#[test]
fn content_none_on_both_marks_leaves_the_document_as_its_author_wrote_it() {
    let body = "<p>one<span class=\"fn\">first</span><br/>two</p>";
    let (result, _, _) = pages(&html(
        ".fn::footnote-call { content: none } .fn::footnote-marker { content: none }",
        body,
    ));
    assert_eq!(texts(&result.pages[0]), vec!["one", "two", "first"]);
    // One of them alone is turned off alone.
    let (result, _, _) = pages(&html(".fn::footnote-call { content: none }", body));
    assert_eq!(texts(&result.pages[0]), vec!["one", "two", "1. first"]);
}

#[test]
fn a_content_of_the_authors_is_what_the_call_shows() {
    let (result, _, _) = pages(&html(
        ".fn::footnote-call { content: \"[\" counter(footnote) \"]\" } \
         .fn::footnote-marker { content: counter(footnote) \") \" }",
        "<p>one<span class=\"fn\">first</span></p>",
    ));
    assert_eq!(texts(&result.pages[0]), vec!["one[1]", "1) first"]);
}

#[test]
fn a_counter_reset_restarts_the_numbering() {
    let (result, _, _) = pages(&html(
        ".chapter { counter-reset: footnote }",
        "<div class=\"chapter\"><p>one<span class=\"fn\">a</span><br/>two<span class=\"fn\">b\
         </span></p></div><div class=\"chapter\"><p>three<span class=\"fn\">c</span></p></div>",
    ));
    assert_eq!(
        texts(&result.pages[0]),
        vec!["one1", "two2", "three1", "1. a", "2. b", "1. c"]
    );
}

#[test]
fn the_call_goes_to_the_next_page_with_its_line_and_its_note() {
    // The line at 150..170 does not fit above its 60px note: the call is
    // on the second page with it.
    let (result, _, _) = pages(&html(
        "",
        &format!(
            "{}<p>one<span class=\"fn\" style=\"height: 60px\">note</span></p>",
            blocks(3, 50)
        ),
    ));
    assert_eq!(result.pages.len(), 2);
    assert!(texts(&result.pages[0]).is_empty());
    assert_eq!(texts(&result.pages[1]), vec!["one1", "1. note"]);
}

#[test]
fn in_a_window_no_mark_is_generated() {
    // Continuous media: the element stays in the flow (`float: none`), and
    // there is nothing to call and nothing to mark.
    let lw = crate::table_markup::laid_out(
        &html("", "<p id=\"p\">one<span class=\"fn\">first</span></p>"),
        800.0,
        600.0,
    );
    let text: String = crate::table_markup::items(&lw)
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => Some(glyphs.len()),
            _ => None,
        })
        .sum::<usize>()
        .to_string();
    assert_eq!(text, "8", "`one` and `first`, no `1` and no `1. `");
}

#[test]
fn the_call_of_a_block_level_note_is_a_line_of_its_own_between_the_blocks() {
    // "A ::footnote-call pseudo-element is inserted in its place": for a
    // footnote element written between two blocks that place is the block
    // flow, and the call is a line there. An author who wants none writes
    // `content: none`.
    let (result, _, _) = pages(&html(
        "",
        "<div class=\"b\" style=\"height: 40px\"></div><div class=\"fn\">note</div>\
         <div class=\"b\" style=\"height: 40px\"></div>",
    ));
    assert_eq!(texts(&result.pages[0]), vec!["1", "1. note"]);
    let call_top = lines(&result.pages[0])[0].0;
    assert!((40.0..60.0).contains(&call_top), "between the blocks: {call_top}");
}
