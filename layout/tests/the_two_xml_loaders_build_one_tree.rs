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

/// What a node of an SVG document carries besides its kind: its SVG data
/// (`ViewBox` / `Path`) and, for an image, the `src` its placeholder holds.
fn svg_shape(node: &azul_core::dom::NodeData) -> (String, Option<&'static str>, Option<String>) {
    use azul_core::dom::SvgNodeData;
    let data = node.get_svg_data().map(|d| match d {
        SvgNodeData::ViewBox { .. } => "viewbox",
        SvgNodeData::Path(_) => "path",
        _ => "other",
    });
    let src = match &node.node_type {
        NodeType::Image(image) => image.source_tag().map(String::from),
        _ => None,
    };
    (kind(&node.node_type), data, src)
}

#[test]
fn an_svg_and_an_img_are_built_alike_by_both_loaders() {
    // Every loader instantiates its elements through the ONE set of builtin
    // renderers (`azul_core::xml::element`). The document loader had a copy
    // of its own, without SVG geometry or `<img src>`: its shapes were boxes
    // with nothing to clip to, its images had no source.
    let document = "<html><body><svg width=\"20\" height=\"10\"><rect x=\"1\" y=\"1\" \
                    width=\"5\" height=\"5\" fill=\"red\"/><circle cx=\"5\" cy=\"5\" r=\"2\"/>\
                    </svg><img src=\"pic.png\" width=\"4\" height=\"3\"/></body></html>";
    let mut tree = Vec::new();
    fn walk<'a>(dom: &'a Dom, out: &mut Vec<&'a azul_core::dom::NodeData>) {
        out.push(&dom.root);
        for child in dom.children.as_ref() {
            walk(child, out);
        }
    }
    let tree_dom = dom_from_parsed_xml(parse_xml(document).expect("parses"));
    walk(&tree_dom, &mut tree);
    let tree: Vec<_> = tree.into_iter().map(svg_shape).collect();
    let styled = parse_xml_to_styled_dom(document).expect("parses");
    let doc: Vec<_> = styled.node_data.as_ref().iter().map(svg_shape).collect();
    assert_eq!(doc, tree, "the two loaders build different nodes");
    assert!(
        doc.iter().filter(|(_, data, _)| *data == Some("path")).count() == 2,
        "both shapes have their geometry: {doc:?}"
    );
    assert!(doc.iter().any(|(_, data, _)| *data == Some("viewbox")), "{doc:?}");
    assert!(
        doc.iter().any(|(_, _, src)| src.as_deref() == Some("pic.png")),
        "the image knows its source: {doc:?}"
    );
}

#[test]
fn svg_elements_keep_their_attributes_and_a_text_is_its_characters() {
    // A page as printpdf writes it: a group with a transform, a path, a text
    // whose characters are its Text children (one of them in a tspan), an
    // image. Each builtin renderer keeps the element's own attributes on its
    // node (the SVG can be written back from the DOM, and layout and paint
    // read them there).
    let document = "<html><body><svg width=\"612\" height=\"792\" viewBox=\"0 0 612 792\">\
                    <g transform=\"matrix(1 0 0 1 10 20)\"><path d=\"M0,0 L10,0 L10,10 Z\" \
                    fill=\"#ff0000\" fill-rule=\"evenodd\"/></g>\
                    <text x=\"72\" y=\"700\" font-family=\"F1\" font-size=\"12\" \
                    transform=\"matrix(1 0 0 -1 0 792)\">Hello<tspan dx=\"2\">World</tspan></text>\
                    <image x=\"0\" y=\"0\" width=\"4\" height=\"2\" href=\"data:image/png;base64,AAAA\"/>\
                    </svg></body></html>";
    for (loader, nodes) in [
        ("tree loader", {
            let mut out = Vec::new();
            fn walk(dom: &Dom, out: &mut Vec<azul_core::dom::NodeData>) {
                out.push(dom.root.clone());
                for child in dom.children.as_ref() {
                    walk(child, out);
                }
            }
            walk(&dom_from_parsed_xml(parse_xml(document).expect("parses")), &mut out);
            out
        }),
        (
            "document loader",
            parse_xml_to_styled_dom(document)
                .expect("parses")
                .node_data
                .as_ref()
                .to_vec(),
        ),
    ] {
        let find = |kind_of: fn(&NodeType) -> bool| {
            nodes
                .iter()
                .find(|n| kind_of(&n.node_type))
                .unwrap_or_else(|| panic!("{loader}: no such node in {nodes:?}"))
        };
        let attr = |node: &azul_core::dom::NodeData, name: &str| {
            node.get_attribute(name).map(|v| v.as_str().to_string())
        };
        let group = find(|t| matches!(t, NodeType::SvgG));
        assert_eq!(attr(group, "transform").as_deref(), Some("matrix(1 0 0 1 10 20)"), "{loader}");
        let path = find(|t| matches!(t, NodeType::SvgPath));
        assert_eq!(attr(path, "d").as_deref(), Some("M0,0 L10,0 L10,10 Z"), "{loader}");
        assert_eq!(attr(path, "fill-rule").as_deref(), Some("evenodd"), "{loader}");
        let text = find(|t| matches!(t, NodeType::SvgText));
        assert_eq!(attr(text, "font-family").as_deref(), Some("F1"), "{loader}");
        assert_eq!(attr(text, "y").as_deref(), Some("700"), "{loader}");
        let tspan = find(|t| matches!(t, NodeType::SvgTspan));
        assert_eq!(attr(tspan, "dx").as_deref(), Some("2"), "{loader}");
        let texts: Vec<String> = nodes
            .iter()
            .filter_map(|n| match &n.node_type {
                NodeType::Text(t) => Some(t.as_str().to_string()),
                _ => None,
            })
            .collect();
        assert!(
            texts.contains(&"Hello".to_string()) && texts.contains(&"World".to_string()),
            "{loader}: the text's characters are Text nodes: {texts:?}"
        );
        let image = find(|t| matches!(t, NodeType::SvgImage(_)));
        assert!(
            matches!(&image.node_type, NodeType::SvgImage(i)
                if i.source_tag() == Some("data:image/png;base64,AAAA")),
            "{loader}: the image carries its href"
        );
    }
}
