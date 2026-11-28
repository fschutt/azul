//! The outline, the tags of the sidebar's own rows and the page a row names.

use azul::widgets::RichTextDoc;
use azul_appkit::docs::{APP_TAG_BASE, DOC_TAG_BASE, MAX_OPEN_DOCS, NEW_TAG};

use super::*;

fn doc(markdown: &str) -> RichTextDoc {
    RichTextDoc::create_from_markdown(markdown)
}

#[test]
fn the_outline_lists_the_headings_in_the_order_of_the_document() {
    let outline = outline(&doc("# Title\n\ntext\n\n## One\n\nmore\n\n### Deeper\n\n## Two\n"), "Untitled");
    let texts: Vec<&str> = outline.rows.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(texts, vec!["Title", "One", "Deeper", "Two"]);
    assert_eq!(outline.hidden, 0);
    let depths: Vec<usize> = outline.rows.iter().map(|r| r.depth).collect();
    assert_eq!(depths, vec![0, 1, 2, 1]);
}

#[test]
fn a_row_names_the_block_its_heading_is_in() {
    let outline = outline(&doc("# Title\n\ntext\n\n## One\n"), "Untitled");
    assert_eq!(outline.rows[0].block, 0);
    assert_eq!(outline.rows[1].block, 2, "the paragraph between them is block 1");
}

#[test]
fn a_document_that_starts_at_a_second_level_is_not_indented_from_the_start() {
    let outline = outline(&doc("## Scope\n\ntext\n\n### Detail\n"), "Untitled");
    let depths: Vec<usize> = outline.rows.iter().map(|r| r.depth).collect();
    assert_eq!(depths, vec![0, 1]);
}

#[test]
fn a_document_without_headings_has_an_empty_outline() {
    let outline = outline(&doc("just words\n\n- and a list\n"), "Untitled");
    assert!(outline.rows.is_empty());
    assert_eq!(outline.hidden, 0);
    assert_eq!(super::outline(&RichTextDoc::create(), "Untitled"), Outline::default());
}

#[test]
fn a_long_heading_is_cut_with_an_ellipsis() {
    let outline = outline(&doc(&format!("# {}\n", "w".repeat(200))), "Untitled");
    assert_eq!(outline.rows[0].text.chars().count(), azul_appkit::docs::NAME_CHARS);
    assert!(outline.rows[0].text.ends_with('\u{2026}'));
}

#[test]
fn a_document_of_thousands_of_headings_lists_the_cap_and_says_how_many_it_leaves_out() {
    let mut markdown = String::new();
    for i in 0..(MAX_OUTLINE + 37) {
        markdown.push_str(&format!("## Heading {i}\n\n"));
    }
    let outline = outline(&doc(&markdown), "Untitled");
    assert_eq!(outline.rows.len(), MAX_OUTLINE);
    assert_eq!(outline.hidden, 37);
    assert_eq!(outline.rows[MAX_OUTLINE - 1].text, format!("Heading {}", MAX_OUTLINE - 1));
}

#[test]
fn every_range_of_tags_is_its_own_and_none_meets_the_open_documents_or_the_new_row() {
    assert!(STYLE_BASE >= APP_TAG_BASE);
    assert!(STYLE_BASE < OUTLINE_BASE && OUTLINE_BASE + MAX_OUTLINE <= PAGE_BASE);
    assert!(NEW_TAG < DOC_TAG_BASE && DOC_TAG_BASE + MAX_OPEN_DOCS <= STYLE_BASE);
}

#[test]
fn a_tag_names_a_style_an_outline_row_or_a_page_only_inside_its_own_range() {
    assert_eq!(style_of_tag(style_tag(0), 6), Some(0));
    assert_eq!(style_of_tag(style_tag(5), 6), Some(5));
    assert_eq!(style_of_tag(style_tag(6), 6), None, "past the last style");
    assert_eq!(style_of_tag(outline_tag(0), 6), None);
    assert_eq!(style_of_tag(DOC_TAG_BASE, 6), None);

    assert_eq!(outline_of_tag(outline_tag(0)), Some(0));
    assert_eq!(outline_of_tag(outline_tag(MAX_OUTLINE - 1)), Some(MAX_OUTLINE - 1));
    assert_eq!(outline_of_tag(outline_tag(MAX_OUTLINE)), None);
    assert_eq!(outline_of_tag(style_tag(0)), None);
    assert_eq!(outline_of_tag(page_tag(0)), None);

    assert_eq!(page_of_tag(page_tag(3)), Some(3));
    assert_eq!(page_of_tag(page_tag(MAX_PAGE_ROWS)), None);
    assert_eq!(page_of_tag(outline_tag(0)), None);
    for tag in [0, 1, NEW_TAG, usize::MAX] {
        assert_eq!(style_of_tag(tag, 6), None);
        assert_eq!(outline_of_tag(tag), None);
        assert_eq!(page_of_tag(tag), None);
    }
}

#[test]
fn a_page_row_names_the_block_its_page_starts_with() {
    let starts = vec![(0, 0), (4, 0), (9, 12)];
    assert_eq!(page_block(&starts, 0), Some(0));
    assert_eq!(page_block(&starts, 2), Some(9));
    assert_eq!(page_block(&starts, 3), None);
    assert_eq!(page_block(&[], 0), None);
}
