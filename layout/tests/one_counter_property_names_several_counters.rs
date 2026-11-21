//! `counter-reset`, `counter-increment` and `counter-set` each take a list of
//! counters (CSS Lists 3 s4.2): `counter-reset: a 1 b 2` makes both, a
//! counter named twice in `counter-increment` steps once per name, and
//! `counter-set` gives the counter in scope a value without making a new one
//! (WPT css-lists li-list-item-counter-001 and -002).

use azul_core::{dom::NodeType, id::NodeId, styled_dom::StyledDom};

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

/// The text of the `::before` of `host`.
fn before_text(styled: &StyledDom, host: NodeId) -> String {
    let before = children(styled, host)
        .into_iter()
        .find(|c| matches!(styled.node_data.as_container()[*c].get_node_type(), NodeType::Before))
        .unwrap_or_else(|| panic!("node {} has no ::before", host.index()));
    let text = children(styled, before);
    match styled.node_data.as_container()[text[0]].get_node_type() {
        NodeType::Text(s) => s.as_ref().as_str().to_string(),
        other => panic!("a ::before holds a text node, not a {other:?}"),
    }
}

fn befores(styled: &StyledDom, class: &str) -> Vec<String> {
    all_with_class(styled, class)
        .into_iter()
        .map(|host| before_text(styled, host))
        .collect()
}

/// The number of every `<li>` of the list with `class`, in order.
fn numbers(styled: &StyledDom, class: &str) -> Vec<i32> {
    let numbers = azul_core::generated_content::list_item_numbers(styled);
    let list = all_with_class(styled, class)[0];
    children(styled, list)
        .into_iter()
        .filter(|c| matches!(styled.node_data.as_container()[*c].get_node_type(), NodeType::Li))
        .map(|li| *numbers.get(&li).unwrap_or_else(|| panic!("li {} has no number", li.index())))
        .collect()
}

#[test]
fn counter_reset_resets_every_counter_it_names() {
    let styled = styled(
        r#"<style>
            .r { counter-reset: a 1 b 2; }
            .r::before { content: counter(a) "-" counter(b); }
        </style>
        <div class="r"></div>"#,
    );
    assert_eq!(befores(&styled, "r"), ["1-2"]);
}

#[test]
fn counter_reset_keeps_the_last_value_of_a_counter_it_names_twice() {
    let styled = styled(
        r#"<style>
            .r { counter-reset: a 1 a 5; }
            .r::before { content: counter(a); }
        </style>
        <div class="r"></div>"#,
    );
    assert_eq!(befores(&styled, "r"), ["5"]);
}

#[test]
fn counter_increment_steps_once_for_every_name_it_lists() {
    let styled = styled(
        r#"<style>
            .c { counter-increment: a 3 a -1 b; }
            .c::before { content: counter(a) "/" counter(b); }
        </style>
        <div class="c"></div><div class="c"></div>"#,
    );
    assert_eq!(befores(&styled, "c"), ["2/1", "4/2"]);
}

#[test]
fn counter_set_makes_a_counter_where_none_is_in_scope() {
    let styled = styled(
        r#"<style>
            .s { counter-set: n 7; }
            .s::before { content: counter(n); }
        </style>
        <div class="s"></div>"#,
    );
    assert_eq!(befores(&styled, "s"), ["7"]);
}

#[test]
fn counter_set_comes_after_the_reset_and_the_increment_of_its_element() {
    let styled = styled(
        r#"<style>
            .s { counter-reset: n 1; counter-increment: n 10; counter-set: n 4; }
            .s::before { content: counter(n); }
        </style>
        <div class="s"></div>"#,
    );
    assert_eq!(befores(&styled, "s"), ["4"]);
}

/// WPT li-list-item-counter-002: a `counter-set` between two list items sets
/// the list's own `list-item` counter, which the next item steps on from.
#[test]
fn counter_set_between_list_items_moves_the_list_s_numbers() {
    let styled = styled(
        r#"<style>
            .set { counter-set: list-item 4; }
            .back { counter-increment: list-item -1; }
        </style>
        <ol class="l"><li value="2">Two</li><div class="set"></div><div class="back"></div><li>Four</li></ol>"#,
    );
    assert_eq!(numbers(&styled, "l"), [2, 4]);
}

/// WPT li-list-item-counter-001: a list item's own `counter-increment` of
/// `list-item` replaces its step, and every `list-item` it names counts.
#[test]
fn a_list_items_counter_increment_of_list_item_replaces_its_step() {
    let styled = styled(
        r#"<style>
            li { counter-increment: list-item 0; }
            .dec { counter-increment: list-item -1; }
            .zero { counter-increment: list-item 1 list-item -1; }
            .two { counter-increment: list-item 3 list-item -1; }
        </style>
        <ol class="plain"><li>a</li><li value="4">b</li><li>c</li></ol>
        <ol class="dec-list"><li class="dec">a</li><li value="4" class="dec">b</li><li class="dec">c</li></ol>
        <ol class="zero-list"><li class="zero">a</li><li value="4" class="zero">b</li><li class="zero">c</li></ol>
        <ol class="two-list"><li class="two">a</li><li value="4" class="two">b</li><li class="two">c</li></ol>"#,
    );
    assert_eq!(numbers(&styled, "plain"), [0, 4, 4]);
    assert_eq!(numbers(&styled, "dec-list"), [-1, 4, 3]);
    assert_eq!(numbers(&styled, "zero-list"), [0, 4, 4]);
    assert_eq!(numbers(&styled, "two-list"), [2, 4, 6]);
}
