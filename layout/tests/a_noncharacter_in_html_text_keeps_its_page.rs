//! A noncharacter in HTML text keeps its page.
//!
//! A table cell of a scanned book was read as U+FFFE followed by "38020", and the page it
//! stood on did not parse: the strict loaders' tokenizer rejects every character XML 1.0 has
//! no `Char` for (U+FFFE, U+FFFF, NUL, the C0 controls but tab, line feed and carriage
//! return), so a PDF made from the page had no page at all. A browser keeps such a page (HTML
//! reads the character as a parse error and goes on), and so does every loader: it reads the
//! text by the one input rule (`azul_css::input_chars`), which writes U+FFFD for what it
//! replaces and keeps the noncharacters XML can carry (U+FDD0, U+1FFFE, U+10FFFF). The
//! shaper then draws the cell's line with the text around the character, every cluster at
//! its own place in the text.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use azul_core::{
    dom::{Dom, DomId, IdOrClass, NodeData, NodeId, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A character the cell holds, and what the cell's text holds in its place.
const CASES: &[(char, &str)] = &[
    ('\u{FFFE}', "\u{FFFD}"),
    ('\u{FFFF}', "\u{FFFD}"),
    ('\u{0}', "\u{FFFD}"),
    ('\u{1}', "\u{FFFD}"),
    ('\u{8}', "\u{FFFD}"),
    ('\u{B}', "\u{FFFD}"),
    ('\u{1F}', "\u{FFFD}"),
    ('\u{FDD0}', "\u{FDD0}"),
    ('\u{1FFFE}', "\u{1FFFE}"),
    ('\u{10FFFF}', "\u{10FFFF}"),
];

/// The page the cell stands on, positioned as a scanned page is.
fn page(cell: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"></head>\
         <body style=\"margin:0\"><div style=\"position:relative; width:300pt; height:200pt\">\
         <div style=\"position:absolute; left:20pt; top:20pt; font-family: serif; \
         font-size: 24pt;\"><p id=\"cell\" style=\"margin:0\">{cell}</p></div></div>\
         </body></html>"
    )
}

/// The code point, for the messages.
fn code(c: char) -> String {
    format!("U+{:04X}", u32::from(c))
}

fn texts_of(dom: &Dom, out: &mut String) {
    if let NodeType::Text(text) = dom.root.get_node_type() {
        out.push_str(text.as_ref().as_str());
    }
    for child in dom.children.as_ref() {
        texts_of(child, out);
    }
}

/// The text the tree loader reads from the page.
fn tree_loader_text(page: &str) -> String {
    let parsed = azul_layout::xml::parse_xml(page)
        .unwrap_or_else(|e| panic!("the page does not parse: {e}"));
    let mut text = String::new();
    texts_of(&azul_layout::xml::dom_from_parsed_xml(parsed), &mut text);
    text
}

#[test]
fn the_tree_loader_keeps_the_page_and_the_text_around_the_character() {
    for &(c, kept) in CASES {
        let text = tree_loader_text(&page(&format!("a{c}38020")));
        assert!(
            text.contains(&format!("a{kept}38020")),
            "{}: the text is {text:?}",
            code(c)
        );
    }
}

#[test]
fn the_document_loader_keeps_the_page_and_the_text_around_the_character() {
    for &(c, kept) in CASES {
        let styled = azul_layout::xml::parse_xml_to_styled_dom(&page(&format!("a{c}38020")))
            .unwrap_or_else(|e| panic!("{}: the page does not parse: {e}", code(c)));
        let text: String = styled
            .node_data
            .as_ref()
            .iter()
            .filter_map(|nd| match nd.get_node_type() {
                NodeType::Text(t) => Some(t.as_ref().as_str().to_string()),
                _ => None,
            })
            .collect();
        assert!(
            text.contains(&format!("a{kept}38020")),
            "{}: the text is {text:?}",
            code(c)
        );
    }
}

/// A character reference to such a character is read as the character itself is.
#[test]
fn a_reference_to_such_a_character_is_read_as_the_character_is() {
    for (reference, kept) in [
        ("&#xFFFE;", "\u{FFFD}"),
        ("&#xFFFF;", "\u{FFFD}"),
        ("&#0;", "\u{FFFD}"),
        ("&#1;", "\u{FFFD}"),
        ("&#x1FFFE;", "\u{1FFFE}"),
    ] {
        let text = tree_loader_text(&page(&format!("a{reference}38020")));
        assert!(
            text.contains(&format!("a{kept}38020")),
            "{reference}: the text is {text:?}"
        );
    }
}

/// The clusters of the cell's lines: per line, each cluster's text and where it starts in
/// the cell's text.
fn clusters_of_the_cell(cell: &str) -> Vec<Vec<(String, u32)>> {
    let parsed = azul_layout::xml::parse_xml(&page(cell)).expect("the page parses");
    let styled = StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed));
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 400.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let result = &lw.layout_results[&DomId::ROOT_ID];
    let cell = result
        .styled_dom
        .node_data
        .as_ref()
        .iter()
        .position(|nd: &NodeData| {
            nd.get_ids_and_classes()
                .iter()
                .any(|c| matches!(c, IdOrClass::Id(s) if s.as_str() == "cell"))
        })
        .expect("the page has its cell");
    let tree = &result.layout_tree;
    let layout = (0..tree.nodes.len())
        .filter(|&index| tree.nodes[index].dom_node_id == Some(NodeId::new(cell)))
        .find_map(|index| tree.materialized_inline_layout_for_node(index))
        .expect("the cell has an inline layout");
    let mut lines: Vec<Vec<(String, u32)>> = Vec::new();
    for item in &layout.items {
        while lines.len() <= item.line_index {
            lines.push(Vec::new());
        }
        if let Some(cluster) = item.item.as_cluster() {
            lines[item.line_index].push((
                cluster.text().to_string(),
                cluster.source_cluster_id.start_byte_in_run,
            ));
        }
    }
    lines
}

#[test]
fn the_cell_is_one_line_holding_the_text_around_the_character() {
    for &(c, kept) in CASES {
        let lines = clusters_of_the_cell(&format!("a{c}38020"));
        let texts: Vec<String> = lines
            .iter()
            .map(|line| line.iter().map(|(text, _)| text.as_str()).collect())
            .collect();
        assert_eq!(
            texts,
            vec![format!("a{kept}38020")],
            "{}: the cell's lines",
            code(c)
        );
        // Each cluster starts where the one before it ends: the replaced character is one
        // cluster of the text, not a gap in it.
        let mut next = 0;
        for (text, start) in &lines[0] {
            assert_eq!(*start, next, "{}: the cluster {text:?}", code(c));
            next += u32::try_from(text.len()).expect("a short cluster");
        }
    }
}
