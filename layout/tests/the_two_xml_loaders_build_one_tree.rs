//! The two XML loaders build one tree from one document (E-XML-3).
//!
//! azul has two strict XML loaders: the TREE loader (`parse_xml` ->
//! `dom_from_parsed_xml`, what AzMail, `Dom::from_xml_string` and the
//! builder use) and the DOCUMENT loader (`parse_xml_to_styled_dom`, the arena
//! path of the reftests and the debug server's `mount`). They had two tree
//! constructions, and MAILVIEW's report listed where they disagreed:
//!
//! - tag case: the tree loader kept `TABLE` and its DOM builder read it with `normalize_casing`
//!   (`t_a_b_l_e`, an unknown `div`); the same builder read `linearGradient` as
//!   `linear_gradient` and `transient-window` as `transient_window`;
//! - implied end tags: only the tree loader closed an `<li>` at the next `<li>` or a `<p>` at
//!   a `<div>`; the document loader nested them;
//! - a stray end tag (`</font>` with no `<font>` open) closed EVERY element in the document
//!   loader, the rest of the document landing outside `<html>`;
//! - Outlook's `<o:p>`: dropped with its text by the tree loader, a block in the document
//!   loader - and both closed it at its end tag's LOCAL name, so `</o:p>` closed the
//!   enclosing `<p>`;
//! - inside an `<svg>`, `<metadata>` and a foreign namespace's elements: dropped by the tree
//!   loader, drawn by the document loader.
//!
//! Now both run the one tree construction (`azul_core::xml::html::TreeBuilder`)
//! and their DOM builders read element names the same way.
//!
//! Not compiled by the author (house rule); RED before the fix.

use azul_core::dom::{Dom, NodeId, NodeType};
use azul_layout::xml::{dom_from_parsed_xml, parse_xml, parse_xml_to_styled_dom};

/// Documents both loaders read (well-formed XML tokens; the trees differ).
const DOCUMENTS: &[(&str, &str)] = &[
    (
        "upper-case tags",
        "<html><body><TABLE><TR><TD>a</TD></TR></TABLE><DIV>b</DIV></body></html>",
    ),
    (
        "implied end tags",
        "<html><body><ul><li>a<li>b</ul><p>c<div>d</div><dl><dt>e<dd>f</dl></body></html>",
    ),
    (
        "a stray end tag",
        "<html><body><div>a</font>b</div><p>c</p></body></html>",
    ),
    (
        "Outlook's o:p",
        "<html><body><p>a<o:p>b</o:p>c</p><p>d</p></body></html>",
    ),
    (
        "an svg's camelCase and editor metadata",
        "<html><body><svg width=\"8\" height=\"8\"><metadata><dc:format>image/svg+xml</dc:format>\
         </metadata><sodipodi:namedview/><defs><linearGradient id=\"g\"><stop offset=\"0\"/>\
         </linearGradient></defs><rect width=\"8\" height=\"8\"/></svg></body></html>",
    ),
    (
        "a transient window",
        "<html><body><div><transient-window open=\"true\"><p>x</p></transient-window></div>\
         </body></html>",
    ),
];

fn kind(node_type: &NodeType) -> String {
    match node_type {
        NodeType::Text(text) => format!("{:?}", text.as_str()),
        other => format!("{:?}", other.get_path()),
    }
}

fn dom_nodes(dom: &Dom, depth: usize, out: &mut Vec<(usize, String)>) {
    out.push((depth, kind(&dom.root.node_type)));
    for child in dom.children.as_ref() {
        dom_nodes(child, depth + 1, out);
    }
}

/// `(depth, what)` of every node the tree loader builds, depth first.
fn tree_loader(document: &str) -> Vec<(usize, String)> {
    let xml = parse_xml(document).unwrap_or_else(|e| panic!("{document}: {e:?}"));
    let mut out = Vec::new();
    dom_nodes(&dom_from_parsed_xml(xml), 0, &mut out);
    out
}

/// `(depth, what)` of every node the document loader builds (its arena is
/// depth first).
fn document_loader(document: &str) -> Vec<(usize, String)> {
    let styled = parse_xml_to_styled_dom(document).unwrap_or_else(|e| panic!("{document}: {e:?}"));
    let hierarchy = styled.node_hierarchy.as_container();
    let mut depths: Vec<usize> = Vec::new();
    let mut out = Vec::new();
    for (i, node) in styled.node_data.as_ref().iter().enumerate() {
        let depth = hierarchy[NodeId::new(i)]
            .parent_id()
            .map_or(0, |p| depths[p.index()] + 1);
        depths.push(depth);
        out.push((depth, kind(&node.node_type)));
    }
    out
}

fn tag(node_type: NodeType) -> String {
    kind(&node_type)
}

fn text(s: &str) -> String {
    format!("{s:?}")
}

#[test]
fn the_tree_loader_and_the_document_loader_build_the_same_dom() {
    let mut failures = Vec::new();
    for (name, document) in DOCUMENTS {
        let tree = tree_loader(document);
        let arena = document_loader(document);
        if tree != arena {
            failures.push(format!(
                "{name}:\n  tree loader     {tree:?}\n  document loader {arena:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn both_loaders_read_element_names_in_any_case() {
    for nodes in [tree_loader(DOCUMENTS[0].1), document_loader(DOCUMENTS[0].1)] {
        let kinds: Vec<&str> = nodes.iter().map(|(_, k)| k.as_str()).collect();
        for expected in [tag(NodeType::Table), tag(NodeType::Tr), tag(NodeType::Td)] {
            assert!(
                kinds.contains(&expected.as_str()),
                "{expected} missing in {kinds:?}"
            );
        }
    }
    for nodes in [tree_loader(DOCUMENTS[4].1), document_loader(DOCUMENTS[4].1)] {
        let kinds: Vec<&str> = nodes.iter().map(|(_, k)| k.as_str()).collect();
        let gradient = tag(NodeType::SvgLinearGradient);
        assert!(
            kinds.contains(&gradient.as_str()),
            "{gradient} missing in {kinds:?}"
        );
        assert!(
            !kinds.iter().any(|k| k.contains("image/svg+xml")),
            "the metadata is not drawn: {kinds:?}"
        );
    }
}

#[test]
fn implied_end_tags_close_list_items_and_paragraphs_in_both_loaders() {
    // html(0) body(1) ul(2) li(3) "a" li(3) "b" p(2) "c" div(2) "d" dl(2) dt(3) "e" dd(3) "f"
    let expected = vec![
        (0, tag(NodeType::Html)),
        (1, tag(NodeType::Body)),
        (2, tag(NodeType::Ul)),
        (3, tag(NodeType::Li)),
        (4, text("a")),
        (3, tag(NodeType::Li)),
        (4, text("b")),
        (2, tag(NodeType::P)),
        (3, text("c")),
        (2, tag(NodeType::Div)),
        (3, text("d")),
        (2, tag(NodeType::Dl)),
        (3, tag(NodeType::Dt)),
        (4, text("e")),
        (3, tag(NodeType::Dd)),
        (4, text("f")),
    ];
    assert_eq!(tree_loader(DOCUMENTS[1].1), expected, "tree loader");
    assert_eq!(document_loader(DOCUMENTS[1].1), expected, "document loader");
}

#[test]
fn a_stray_end_tag_is_ignored_by_both_loaders() {
    let expected = vec![
        (0, tag(NodeType::Html)),
        (1, tag(NodeType::Body)),
        (2, tag(NodeType::Div)),
        (3, text("ab")),
        (2, tag(NodeType::P)),
        (3, text("c")),
    ];
    assert_eq!(tree_loader(DOCUMENTS[2].1), expected, "tree loader");
    assert_eq!(document_loader(DOCUMENTS[2].1), expected, "document loader");
}

#[test]
fn an_outlook_o_p_is_an_inline_element_closed_by_its_own_end_tag() {
    // `<p>a<o:p>b</o:p>c</p>`: an unknown element of a foreign vocabulary is
    // inline in HTML, keeps its text (Outlook's `<o:p>&nbsp;</o:p>` is what
    // makes an empty paragraph one line tall), and `</o:p>` closes it - not
    // the paragraph.
    let expected = vec![
        (0, tag(NodeType::Html)),
        (1, tag(NodeType::Body)),
        (2, tag(NodeType::P)),
        (3, text("a")),
        (3, tag(NodeType::Span)),
        (4, text("b")),
        (3, text("c")),
        (2, tag(NodeType::P)),
        (3, text("d")),
    ];
    assert_eq!(tree_loader(DOCUMENTS[3].1), expected, "tree loader");
    assert_eq!(document_loader(DOCUMENTS[3].1), expected, "document loader");
}
