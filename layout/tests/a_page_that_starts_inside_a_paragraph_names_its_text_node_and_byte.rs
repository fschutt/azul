//! A page that starts inside a paragraph names where in the DOM it starts:
//! the text node and the byte of that node's text the page's first line
//! begins at (L9, the engine half; `RichTextEditor::page_doms` and AzWriter
//! start their pages there).
//!
//! The break already named the block it splits and the first line that
//! moves, as a run and a byte (`break_line_start_run` / `_byte`). Those
//! count in the line layout's runs: text after whitespace collapsing, a
//! node cut into several runs at its forced breaks. An app holds the DOM
//! text, so the handle now maps the line start back to it.

use azul_core::dom::NodeType;
use azul_layout::{
    resource_handles::{PaginationAnalysis, PaginationSnapshot},
    solver3::paged_layout::pagination_to_dom_breaks,
};

use crate::pagination_dom_breaks::paginate;

/// The text of the DOM node at child-index `path` (from the root).
fn text_at(styled_dom: &azul_core::styled_dom::StyledDom, path: &[u32]) -> String {
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut node = azul_core::dom::NodeId::new(0);
    for &i in path {
        node = node
            .az_children(&hierarchy)
            .nth(i as usize)
            .expect("the path names a child");
    }
    match styled_dom.node_data.as_container()[node].get_node_type() {
        NodeType::Text(t) => t.as_str().to_string(),
        other => panic!("the path names a text node, got {other:?}"),
    }
}

#[test]
fn every_page_after_a_split_starts_at_a_word_of_the_paragraphs_own_text() {
    // Numbered words with runs of whitespace between them, which the line
    // layout collapses: the DOM bytes and the run bytes differ.
    let words: Vec<String> = (0..300).map(|i| format!("w{i}")).collect();
    let long = words.join("  \n   ");
    let html = format!(
        "<html><head><style>* {{ margin: 0; padding: 0; }} body {{ font-size: 16px; \
         line-height: 20px; width: 300px; }} .p {{ display: block; width: 300px; }}\
         </style></head><body><div class=\"p\">short</div><div class=\"p\">{long}</div>\
         </body></html>"
    );
    let (cache, styled_dom, pagination) = paginate(&html, 300.0, 200.0);
    assert!(pagination.page_count >= 3, "the paragraph spans pages");
    let structural =
        pagination_to_dom_breaks(&cache, &styled_dom, &pagination).expect("tree and positions");
    let snapshot = PaginationSnapshot::from_analysis(PaginationAnalysis {
        info: pagination.clone(),
        structural,
    });
    let mut previous = None;
    for i in 0..snapshot.break_count() {
        let path = snapshot.break_line_text_path(i);
        let path = path.as_ref().to_vec();
        let byte = snapshot
            .break_line_text_byte(i)
            .into_option()
            .unwrap_or_else(|| panic!("break {i} splits the paragraph: it names a byte"));
        let block = snapshot.break_line_path(i);
        assert_eq!(
            &path[..path.len() - 1],
            block.as_ref(),
            "break {i}: the text node is a child of the block the break splits"
        );
        let text = text_at(&styled_dom, &path);
        let rest = &text[byte as usize..];
        let word = rest.split_whitespace().next().unwrap_or_default();
        assert!(
            rest.starts_with('w') && words.iter().any(|w| w == word),
            "break {i}: the page starts at a word of the DOM text, got {:?}",
            &rest[..rest.len().min(12)]
        );
        let n: usize = word[1..].parse().expect("a numbered word");
        if let Some(prev) = previous {
            assert!(n > prev, "page after page the start moves down the text: {prev} then {n}");
        }
        previous = Some(n);
    }
    assert!(previous.is_some(), "at least one break splits the paragraph");
}
