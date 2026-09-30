//! A link in mail markup keeps where it points.
//!
//! AzMail exploration (scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md, 1.2
//! and gap E-XML-2): every XML attribute goes through ONE table
//! (`azul_core::xml::attributes`), and `href` was not in it. `<a href="..">`
//! parsed into an `A` node that did not know where it points, so a mail
//! client could not open a link the user clicked. `AttributeType::Href`
//! existed all along (`Dom::create_a` sets it); the table never produced it.
//!
//! Both loaders read the table: the tree loader AzMail uses (`parse_xml` +
//! `dom_from_parsed_xml`, i.e. `Dom::create_from_parsed_xml`) and the fast
//! path (`parse_xml_to_styled_dom`).
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use azul_core::dom::{AttributeType, Dom, NodeData, NodeType};

const MAIL: &str = "<html><head></head><body><div>\
<p>Read the <a href=\"https://example.org/report?id=7&amp;v=2\">report</a> or write to \
<a href=\"mailto:anna@example.org\">Anna</a>.</p>\
<p><a name=\"top\">an anchor, not a link</a></p>\
</div></body></html>";

fn hrefs_of(links: &[&NodeData]) -> Vec<Option<String>> {
    links
        .iter()
        .map(|nd| {
            nd.attributes().as_ref().iter().find_map(|a| match a {
                AttributeType::Href(h) => Some(h.as_str().to_string()),
                _ => None,
            })
        })
        .collect()
}

fn collect_links<'a>(dom: &'a Dom, out: &mut Vec<&'a NodeData>) {
    if dom.root.node_type == NodeType::A {
        out.push(&dom.root);
    }
    for child in dom.children.as_ref() {
        collect_links(child, out);
    }
}

fn expected() -> Vec<Option<String>> {
    vec![
        // The entity in the attribute value is decoded, as in a browser.
        Some("https://example.org/report?id=7&v=2".to_string()),
        Some("mailto:anna@example.org".to_string()),
        // `<a name>` has no `href`: it is an anchor, not a link.
        None,
    ]
}

#[test]
fn the_tree_loader_gives_each_link_its_href() {
    let parsed = azul_layout::xml::parse_xml(MAIL).expect("the mail parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let mut links = Vec::new();
    collect_links(&dom, &mut links);
    assert_eq!(links.len(), 3, "three <a> elements");
    assert_eq!(hrefs_of(&links), expected());
}

#[test]
fn the_fast_loader_gives_each_link_its_href() {
    let styled = azul_layout::xml::parse_xml_to_styled_dom(MAIL).expect("the mail parses");
    let links: Vec<&NodeData> = styled
        .node_data
        .as_ref()
        .iter()
        .filter(|nd| nd.node_type == NodeType::A)
        .collect();
    assert_eq!(links.len(), 3, "three <a> elements");
    assert_eq!(hrefs_of(&links), expected());
}
