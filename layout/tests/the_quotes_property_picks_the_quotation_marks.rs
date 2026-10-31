//! `open-quote` and `close-quote` take their marks from `quotes` (CSS
//! Generated Content 3 s3): a list of pairs, one per nesting level, `none`
//! for no marks, and `auto` (the initial value) for the marks of the
//! element's language, so a German quotation opens low.

use azul_core::{dom::NodeType, id::NodeId, styled_dom::StyledDom};

fn styled(markup: &str) -> StyledDom {
    let parsed = azul_layout::xml::parse_xml(markup).expect("parses");
    StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed))
}

fn children(styled: &StyledDom, node: NodeId) -> Vec<NodeId> {
    let hierarchy = styled.node_hierarchy.as_container();
    let mut out = Vec::new();
    let mut next = hierarchy[node].first_child_id(node);
    while let Some(c) = next {
        out.push(c);
        next = hierarchy[c].next_sibling_id();
    }
    out
}

/// `(::before, ::after)` text of the first node with `class`.
fn marks(styled: &StyledDom, class: &str) -> (String, String) {
    let host = (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| styled.node_data.as_container()[*id].has_class(class))
        .unwrap_or_else(|| panic!("no .{class}"));
    let text = |kind_before: bool| {
        let node = children(styled, host)
            .into_iter()
            .find(|c| match styled.node_data.as_container()[*c].get_node_type() {
                NodeType::Before => kind_before,
                NodeType::After => !kind_before,
                _ => false,
            })
            .expect("a generated node");
        match styled.node_data.as_container()[children(styled, node)[0]].get_node_type() {
            NodeType::Text(s) => s.as_ref().as_str().to_string(),
            other => panic!("not text: {other:?}"),
        }
    };
    (text(true), text(false))
}

const QUOTED: &str = r#"
    .q::before { content: open-quote; }
    .q::after { content: close-quote; }
"#;

#[test]
fn the_quotes_pairs_give_the_marks_of_each_level() {
    let styled = styled(&format!(
        r#"<style>{QUOTED} .fr {{ quotes: "«" "»" "‹" "›"; }}</style>
        <div class="fr"><span class="q outer">a<span class="q inner">b</span></span></div>"#
    ));
    assert_eq!(marks(&styled, "outer"), ("«".to_string(), "»".to_string()));
    assert_eq!(marks(&styled, "inner"), ("‹".to_string(), "›".to_string()));
}

#[test]
fn quotes_none_makes_no_marks() {
    let styled = styled(&format!(
        r#"<style>{QUOTED} .n {{ quotes: none; }}</style><span class="q n">a</span>"#
    ));
    assert_eq!(marks(&styled, "n"), (String::new(), String::new()));
}

#[test]
fn quotes_auto_takes_the_marks_of_the_language() {
    let styled = styled(&format!(
        r#"<style>{QUOTED}</style>
        <div lang="de"><span class="q de">a</span></div>
        <div lang="en"><span class="q en">a</span></div>"#
    ));
    assert_eq!(marks(&styled, "de"), ("\u{201e}".to_string(), "\u{201c}".to_string()));
    assert_eq!(marks(&styled, "en"), ("\u{201c}".to_string(), "\u{201d}".to_string()));
}
