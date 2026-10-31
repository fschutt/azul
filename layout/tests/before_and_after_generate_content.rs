//! `::before` and `::after` generate content (CSS 2.2 s12.1, CSS Lists 3):
//! a rule with a `content` value gives its host a first and a last child
//! whose text the value makes - strings, the host's attributes, counters
//! (`counter-reset`, `counter-increment`, the `list-item` counter of lists).
//!
//! The generated nodes are ordinary nodes of the styled DOM, made before the
//! cascade, so the layout lays them out like any inline child.

use azul_core::{
    dom::NodeType,
    id::NodeId,
    styled_dom::{HoverChange, StyledDom},
};
use azul_css::props::{
    basic::color::ColorU, layout::LayoutDisplay, style::StyleBackgroundContent,
};

fn styled(markup: &str) -> StyledDom {
    let parsed = azul_layout::xml::parse_xml(markup).expect("parses");
    StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed))
}

fn all_with_class(styled: &StyledDom, class: &str) -> Vec<NodeId> {
    (0..styled.node_data.len())
        .map(NodeId::new)
        .filter(|id| styled.node_data.as_container()[*id].has_class(class))
        .collect()
}

fn with_class(styled: &StyledDom, class: &str) -> NodeId {
    *all_with_class(styled, class)
        .first()
        .unwrap_or_else(|| panic!("no .{class} in the document"))
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

fn text_of(styled: &StyledDom, node: NodeId) -> String {
    match styled.node_data.as_container()[node].get_node_type() {
        NodeType::Text(s) => s.as_ref().as_str().to_string(),
        other => panic!("node {} is a {other:?}, not text", node.index()),
    }
}

/// The generated node of `host` of the kind `before` (else `::after`).
fn generated_node(styled: &StyledDom, host: NodeId, before: bool) -> Option<NodeId> {
    children(styled, host).into_iter().find(|c| {
        match styled.node_data.as_container()[*c].get_node_type() {
            NodeType::Before => before,
            NodeType::After => !before,
            _ => false,
        }
    })
}

/// The text the `::before` (or `::after`) of `host` holds.
fn generated_text(styled: &StyledDom, host: NodeId, before: bool) -> String {
    let node = generated_node(styled, host, before).unwrap_or_else(|| {
        panic!(
            "node {} has no {}",
            host.index(),
            if before { "::before" } else { "::after" }
        )
    });
    let kids = children(styled, node);
    assert_eq!(kids.len(), 1, "a generated node holds one text node");
    text_of(styled, kids[0])
}

fn display(styled: &StyledDom, node: NodeId) -> Option<LayoutDisplay> {
    azul_layout::solver3::getters::get_display_property(styled, Some(node)).exact()
}

#[test]
fn a_before_and_an_after_are_the_first_and_last_child_of_their_host() {
    let styled = styled(
        r#"<style>
            .box::before { content: "["; }
            .box::after { content: "]"; }
        </style>
        <div class="box">x</div>"#,
    );
    let host = with_class(&styled, "box");
    let kids = children(&styled, host);
    assert_eq!(kids.len(), 3, "::before, the text, ::after");
    let node_data = styled.node_data.as_container();
    assert!(matches!(node_data[kids[0]].get_node_type(), NodeType::Before));
    assert_eq!(text_of(&styled, kids[1]), "x");
    assert!(matches!(node_data[kids[2]].get_node_type(), NodeType::After));
    assert_eq!(generated_text(&styled, host, true), "[");
    assert_eq!(generated_text(&styled, host, false), "]");
    assert_eq!(
        display(&styled, kids[0]),
        Some(LayoutDisplay::Inline),
        "a generated box is inline unless styled otherwise"
    );
}

#[test]
fn css2_spells_the_pseudo_elements_with_one_colon() {
    let styled = styled(
        r#"<style>
            .a:before { content: "1"; }
            .a:after { content: "2"; }
        </style>
        <div class="a">x</div>"#,
    );
    let host = with_class(&styled, "a");
    assert_eq!(generated_text(&styled, host, true), "1");
    assert_eq!(generated_text(&styled, host, false), "2");
}

#[test]
fn content_joins_strings_and_the_hosts_attributes() {
    let styled = styled(
        r##"<style>
            .tag::before { content: "#" attr(data-n) " - " attr(data-missing) "\2022"; }
        </style>
        <div class="tag" data-n="42">x</div>"##,
    );
    let host = with_class(&styled, "tag");
    assert_eq!(
        generated_text(&styled, host, true),
        "#42 - \u{2022}",
        "a missing attribute is the empty string, a CSS escape is its code point"
    );
}

#[test]
fn a_counter_counts_in_document_order() {
    let styled = styled(
        r#"<style>
            .c { counter-increment: ident; }
            .c::before { content: counter(ident); }
        </style>
        <div class="c"></div><div class="c"></div><div class="c"></div>"#,
    );
    let texts: Vec<String> = all_with_class(&styled, "c")
        .into_iter()
        .map(|h| generated_text(&styled, h, true))
        .collect();
    assert_eq!(texts, ["1", "2", "3"]);
}

#[test]
fn a_reset_counter_starts_at_its_value_and_a_generated_node_counts_too() {
    // WPT css/CSS2/lists counter-reset-055, reset-counter-001 and
    // increment-counter-001.
    let reset_only = styled(
        r#"<style>
            .r { counter-reset: ident; }
            .r::before { content: counter(ident); }
        </style>
        <div class="r"></div>"#,
    );
    assert_eq!(
        generated_text(&reset_only, with_class(&reset_only, "r"), true),
        "0"
    );

    let reset_on_pseudo = styled(
        r#"<style>
            .p::before { content: counter(chapter); counter-reset: chapter 5; }
        </style>
        <div class="p"></div>"#,
    );
    assert_eq!(
        generated_text(&reset_on_pseudo, with_class(&reset_on_pseudo, "p"), true),
        "5"
    );

    let increment_on_pseudo = styled(
        r#"<style>
            .i::before { content: counter(chapter); counter-increment: chapter 5; }
        </style>
        <div class="i"></div>"#,
    );
    assert_eq!(
        generated_text(
            &increment_on_pseudo,
            with_class(&increment_on_pseudo, "i"),
            true
        ),
        "5"
    );
}

#[test]
fn list_items_number_their_generated_content_as_html_numbers_them() {
    // WPT css/css-lists/counter-list-item.
    let styled = styled(
        r#"<style>
            li::before { content: counter(list-item) " "; }
        </style>
        <ol class="plain"><li>a</li><li>b</li><li>c</li></ol>
        <ol class="start" start="30"><li>a</li><li>b</li><li>c</li></ol>
        <ol class="value"><li>a</li><li>b</li><li value="30">c</li><li>d</li></ol>
        <ol class="rev" reversed=""><li>a</li><li>b</li><li>c</li></ol>
        <ol class="rev-start" start="30" reversed=""><li>a</li><li>b</li><li>c</li></ol>
        <ol class="rev-value" reversed=""><li>a</li><li>b</li><li value="30">c</li><li>d</li><li value="35">e</li><li>f</li></ol>
        <ul class="ul"><li>a</li><li>b</li><li>c</li></ul>"#,
    );
    let numbers = |class: &str| -> Vec<String> {
        let list = with_class(&styled, class);
        children(&styled, list)
            .into_iter()
            .filter(|c| matches!(styled.node_data.as_container()[*c].get_node_type(), NodeType::Li))
            .map(|li| generated_text(&styled, li, true).trim().to_string())
            .collect()
    };
    assert_eq!(numbers("plain"), ["1", "2", "3"]);
    assert_eq!(numbers("start"), ["30", "31", "32"]);
    assert_eq!(numbers("value"), ["1", "2", "30", "31"]);
    assert_eq!(numbers("rev"), ["3", "2", "1"]);
    assert_eq!(numbers("rev-start"), ["30", "29", "28"]);
    assert_eq!(numbers("rev-value"), ["32", "31", "30", "29", "35", "34"]);
    assert_eq!(numbers("ul"), ["1", "2", "3"]);
}

#[test]
fn nested_counters_join_their_levels() {
    let styled = styled(
        r#"<style>
            ol { counter-reset: item; }
            li { counter-increment: item; }
            li::before { content: counters(item, ".") " "; }
        </style>
        <ol><li class="one">a<ol><li class="one-one">b</li><li class="one-two">c</li></ol></li><li class="two">d</li></ol>"#,
    );
    let text = |class: &str| generated_text(&styled, with_class(&styled, class), true);
    assert_eq!(text("one"), "1 ");
    assert_eq!(text("one-one"), "1.1 ");
    assert_eq!(text("one-two"), "1.2 ");
    assert_eq!(text("two"), "2 ");
}

#[test]
fn a_reset_on_an_element_reaches_the_siblings_after_it() {
    // CSS 2.2 s12.4.1: a counter's scope is the element that resets it, its
    // descendants and its following siblings with theirs.
    let styled = styled(
        r#"<style>
            h2 { counter-reset: sub; }
            h3 { counter-increment: sub; }
            h3::before { content: counter(sub); }
        </style>
        <div><h2>A</h2><h3 class="a1">x</h3><h3 class="a2">y</h3><h2>B</h2><h3 class="b1">z</h3></div>"#,
    );
    let text = |class: &str| generated_text(&styled, with_class(&styled, class), true);
    assert_eq!(text("a1"), "1");
    assert_eq!(text("a2"), "2");
    assert_eq!(text("b1"), "1", "the second h2 starts the counter again");
}

#[test]
fn content_none_generates_no_box() {
    let styled = styled(
        r#"<style>
            .x::before { content: "a"; }
            .x.quiet::before { content: none; }
        </style>
        <div class="x loud">1</div><div class="x quiet">2</div>"#,
    );
    let loud = generated_node(&styled, with_class(&styled, "loud"), true)
        .expect("a ::before with content");
    assert_eq!(display(&styled, loud), Some(LayoutDisplay::Inline));
    if let Some(quiet) = generated_node(&styled, with_class(&styled, "quiet"), true) {
        assert_eq!(
            display(&styled, quiet),
            Some(LayoutDisplay::None),
            "content: none makes no box, as in every browser"
        );
    }
}

#[test]
fn a_generated_node_appears_on_hover_when_only_its_hover_rule_has_content() {
    let mut styled = styled(
        r#"<style>
            .go:hover::after { content: "->"; }
        </style>
        <div class="go">open</div>"#,
    );
    let host = with_class(&styled, "go");
    let after = generated_node(&styled, host, false).expect("the hover rule makes the node");
    assert_eq!(display(&styled, after), Some(LayoutDisplay::None), "resting");

    let result = styled.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: Vec::new(),
            entered_nodes: vec![host],
        }),
        None,
    );
    assert!(
        styled.styled_nodes.as_container()[after]
            .styled_node_state
            .hover,
        "a generated node takes its host's state"
    );
    assert_eq!(display(&styled, after), Some(LayoutDisplay::Inline));
    assert_eq!(generated_text(&styled, host, false), "->");
    assert!(result.needs_layout, "a box appeared");
}

#[test]
fn generated_nodes_leave_the_structural_pseudo_classes_alone() {
    let styled = styled(
        r#"<style>
            ul::before { content: "list"; }
            ul::after { content: "end"; }
            li { background: #00ff00; }
            li:first-child { background: #ff0000; }
            li:last-child { background: #0000ff; }
        </style>
        <ul><li class="first">a</li><li class="second">b</li><li class="third">c</li></ul>"#,
    );
    let background = |class: &str| {
        let node = with_class(&styled, class);
        let state = styled.styled_nodes.as_container()[node].styled_node_state;
        azul_layout::solver3::getters::get_background_contents(&styled, node, &state)
            .into_iter()
            .find_map(|bg| match bg {
                StyleBackgroundContent::Color(c) => Some(c),
                _ => None,
            })
    };
    let rgb = |r, g, b| {
        Some(ColorU {
            r,
            g,
            b,
            a: 255,
        })
    };
    assert_eq!(background("first"), rgb(255, 0, 0), "still the first child");
    assert_eq!(background("second"), rgb(0, 255, 0));
    assert_eq!(background("third"), rgb(0, 0, 255), "still the last child");
}

#[test]
fn the_generated_text_paints() {
    let markup = r#"<style>
            body { margin: 0; padding: 0; }
            .x::before { content: "MMMM"; color: #000000; font-size: 20px; }
        </style>
        <div class="x"></div>"#;
    let parsed = azul_layout::xml::parse_xml(markup).expect("parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let r = azul_layout::cpurender::render_dom_to_rgba(
        dom,
        azul_css::css::Css::empty(),
        120.0,
        40.0,
        1.0,
        ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        },
    )
    .expect("renders");
    let inked = r.rgba.chunks_exact(4).filter(|p| p[3] > 128).count();
    assert!(
        inked > 40,
        "the div is empty, so every inked pixel is the generated text: got {inked}"
    );
}

#[test]
fn a_nodes_own_with_css_after_decorates_that_node_alone() {
    use azul_core::dom::Dom;
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_class("owner".into())
            .with_css("::after { content: \"!\"; }")
            .with_child(Dom::create_div().with_class("inner".into())),
    );
    let styled = StyledDom::create_from_dom(dom);
    let owner = with_class(&styled, "owner");
    let inner = with_class(&styled, "inner");
    assert_eq!(generated_text(&styled, owner, false), "!");
    assert!(
        generated_node(&styled, inner, false).is_none(),
        "the owner's ::after is not every descendant's"
    );
}

/// `*` selects elements. A `::before` is styled by the rules that name it
/// and inherits from its host; a text node only inherits. So an unscoped
/// `* { .. }` sheet (a rice, say) leaves both alone, in the compact values
/// the layout reads (`display`) and in the full cascade the paint reads
/// (`background`), as in Chrome.
#[test]
fn an_unscoped_star_rule_styles_elements_and_not_their_generated_content_or_text() {
    let styled = styled(
        r#"<style>
            * { display: block; background: #ff0000; }
            .x::before { content: "a"; }
        </style>
        <div class="x">b</div>"#,
    );
    let host = with_class(&styled, "x");
    let before = generated_node(&styled, host, true).expect("the rule makes the node");
    let text = *children(&styled, host)
        .iter()
        .find(|c| styled.node_data.as_container()[**c].is_text_node())
        .expect("the host's own text");
    let red = |node: NodeId| {
        let state = styled.styled_nodes.as_container()[node].styled_node_state;
        azul_layout::solver3::getters::get_background_contents(&styled, node, &state)
            .into_iter()
            .any(|bg| matches!(bg, StyleBackgroundContent::Color(c) if c.r == 255 && c.g == 0))
    };
    assert!(red(host), "harness: the element takes the `*` rule");
    assert_eq!(display(&styled, host), Some(LayoutDisplay::Block));
    assert_eq!(
        display(&styled, before),
        Some(LayoutDisplay::Inline),
        "a ::before keeps its own display"
    );
    assert!(!red(before), "a ::before is no element `*` selects");
    assert!(!red(text), "a text node is no element `*` selects");
}
