//! Generated content: the `::before` and `::after` of an element.
//!
//! A rule with a `content` value (`.tag::before { content: "#" attr(data-n) }`)
//! gives its host a first or a last child. That child is a real node of the
//! styled DOM, made before the cascade ([`insert_generated_nodes`]), so the
//! ordinary cascade styles it (the css parser writes the rule as
//! `.tag > ::before`), it inherits from its host, it takes its host's
//! interaction states (it is a part of it), and the layout lays it out like any
//! inline child. It holds one text node.
//!
//! An inline list item (`display: inline list-item`) gets its `::marker` the
//! same way, as its first child: a block list item's marker sits outside its
//! box and the layout makes it, an inline one's starts its inline content.
//!
//! After every cascade, [`fill_generated_text`] writes that text from the
//! node's `content` (a marker's from its list item's `list-style-type`):
//! strings, `attr()`, `counter()` / `counters()` and quotes.
//! The counters count there, in document order: `counter-reset`,
//! `counter-increment`, `counter-set` and the `list-item` counter every list
//! item steps, with HTML's `<ol start reversed>` and `<li value>`.

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};

use azul_css::{
    css::{
        BoxOrStatic, Css, CssDeclaration, CssPath, CssPathSelector, CssPropertyValue,
        CssScopeRange, NodeTypeTag,
    },
    dynamic_selector::PseudoStateType,
    props::{
        layout::LayoutDisplay,
        property::{CssProperty, CssPropertyType},
        style::{
            content::Content,
            lists::{format_counter, parse_style_list_style_type, StyleListStyleType},
        },
    },
    AzString,
};

use crate::{
    dom::{NodeData, NodeType},
    id::{Node, NodeDataContainer, NodeDataContainerRef, NodeHierarchy, NodeId},
    prop_cache::{CssPropertyCache, StatefulCssProperty},
    style::{construct_html_cascade_tree, matches_html_element_in, AncestorStates},
    styled_dom::{CompactDom, NodeHierarchyItem, StyledDom, StyledNodeState},
};

/// Whether a `content` value generates a box: anything but `none`, `normal`
/// and the keywords that compute to them.
#[must_use]
pub fn content_makes_box(value: &CssPropertyValue<Content>) -> bool {
    value
        .get_property()
        .is_some_and(|c| !matches!(c.inner.as_str().trim(), "" | "none" | "normal"))
}

const fn is_combinator(selector: &CssPathSelector) -> bool {
    matches!(
        selector,
        CssPathSelector::Children
            | CssPathSelector::DirectChildren
            | CssPathSelector::AdjacentSibling
            | CssPathSelector::GeneralSibling
    )
}

/// Which generated node a rule styles, `true` for a `::before`, and the index
/// of the child combinator its host part ends at (`div > ::before`).
fn generated_subject(path: &CssPath) -> Option<(bool, usize)> {
    let selectors = path.selectors.as_ref();
    let combinator = selectors.iter().rposition(is_combinator)?;
    if !matches!(selectors[combinator], CssPathSelector::DirectChildren) {
        return None;
    }
    selectors[combinator + 1..].iter().find_map(|s| match s {
        CssPathSelector::Type(NodeTypeTag::Before) => Some((true, combinator)),
        CssPathSelector::Type(NodeTypeTag::After) => Some((false, combinator)),
        _ => None,
    })
}

/// Whether a rule makes content: it declares `content` with a value other
/// than `none` / `normal`. A `var()` counts, its value is not known yet.
fn makes_content(declarations: &[CssDeclaration]) -> bool {
    declarations.iter().any(|d| match d {
        CssDeclaration::Static(CssProperty::Content(value)) => content_makes_box(value),
        CssDeclaration::Dynamic(dynamic) => {
            dynamic.default_value.get_type() == CssPropertyType::Content
        }
        _ => false,
    })
}

/// Whether a rule makes its subject an inline list item, which carries a
/// `::marker` node.
fn makes_inline_list_item(declarations: &[CssDeclaration]) -> bool {
    declarations.iter().any(|d| {
        matches!(
            d,
            CssDeclaration::Static(CssProperty::Display(CssPropertyValue::Exact(
                LayoutDisplay::InlineListItem
            )))
        )
    })
}

/// The host part of a generated-content rule: everything before `end` (its
/// `> ::before`, or the whole path for a marker), without the host's own
/// interaction pseudo-classes. The node exists in every state; the cascade
/// decides when it shows.
fn host_path(path: &CssPath, end: usize) -> CssPath {
    let selectors = &path.selectors.as_ref()[..end];
    let compound = selectors
        .iter()
        .rposition(is_combinator)
        .map_or(0, |i| i + 1);
    let kept: Vec<CssPathSelector> = selectors
        .iter()
        .enumerate()
        .filter(|(i, s)| {
            *i < compound
                || !matches!(s, CssPathSelector::PseudoSelector(p) if p.dynamic_state().is_some())
        })
        .map(|(_, s)| s.clone())
        .collect();
    CssPath::new(kept)
}

/// Whether a node can have generated content: an element that lays out
/// children of its own. Text, replaced elements (images, form fields,
/// embedded views), metadata and generated content itself have none, as in
/// browsers; SVG content is excluded by the caller.
const fn can_host(node: &NodeData) -> bool {
    !node.is_anonymous()
        && !matches!(
            node.get_node_type(),
            NodeType::Text(_)
                | NodeType::Image(_)
                | NodeType::Icon(_)
                | NodeType::Br
                | NodeType::Wbr
                | NodeType::Input
                | NodeType::TextArea
                | NodeType::Select
                | NodeType::VirtualView
                | NodeType::WebView(_)
                | NodeType::TransientWindow(_)
                | NodeType::GeolocationProbe(_)
                | NodeType::Svg
                | NodeType::Head
                | NodeType::Title
                | NodeType::Meta
                | NodeType::Link
                | NodeType::Script
                | NodeType::Style
                | NodeType::Base
                | NodeType::Before
                | NodeType::After
                | NodeType::Marker
                | NodeType::Placeholder
        )
}

/// Appends a generated node of `kind` under `host` (a new index) and the text
/// node it holds.
fn push_generated(
    kind: NodeType,
    host: usize,
    data: &mut Vec<NodeData>,
    parents: &mut Vec<Option<usize>>,
) {
    let mut node = NodeData::create_node(kind);
    node.set_part_of_parent(true);
    let mut text = NodeData::create_text_do_not_use_without_block_level_wrapper("");
    text.set_part_of_parent(true);
    parents.push(Some(host));
    data.push(node);
    parents.push(Some(data.len() - 1));
    data.push(text);
}

/// Where the nodes of the old arena went: what a scope range of the
/// stylesheet needs to follow them.
struct Moved {
    /// The new index of each old node.
    old_to_new: Vec<usize>,
    /// The last old index in each old node's subtree.
    old_end: Vec<usize>,
    /// The last new index in each old node's subtree, its generated nodes
    /// included.
    new_end: Vec<usize>,
    /// How many nodes the new arena has.
    new_len: usize,
}

impl Moved {
    /// `range` over the new arena: a node's subtree stays its subtree (now
    /// with its generated nodes), a single node stays that node.
    fn range(&self, range: CssScopeRange) -> CssScopeRange {
        let n = self.old_to_new.len();
        if range.start >= n {
            // No node: it stays past the end.
            let shift = self.new_len - n;
            return CssScopeRange {
                start: range.start + shift,
                end: range.end + shift,
            };
        }
        let start = self.old_to_new[range.start];
        let end = if range.end == self.old_end[range.start] {
            self.new_end[range.start]
        } else if range.end <= range.start {
            start
        } else if range.end + 1 < n {
            self.old_to_new[range.end + 1] - 1
        } else {
            self.new_len - 1
        };
        CssScopeRange { start, end }
    }
}

/// Inserts the generated nodes `css` asks for into `dom`, and moves the node
/// indices the stylesheet's scopes name along with their nodes. Returns
/// `false`, changing nothing, when no rule makes content.
///
/// A host gets a `::before` (its first child) or a `::after` (its last) when
/// any rule with a `content` value can select one for it, in any state and
/// under any condition: the node set stays as it is until the DOM is
/// rebuilt. Whether a node shows is the cascade's answer - under
/// `content: none` it is `display: none`
/// ([`CssPropertyCache::hide_contentless_generated_nodes`]).
#[allow(clippy::too_many_lines)] // one pass: decide the hosts, rebuild the arena, move the scopes
pub fn insert_generated_nodes(dom: &mut CompactDom, css: &mut Css) -> bool {
    let rules: Vec<(bool, CssPath)> = css
        .rules
        .as_ref()
        .iter()
        .filter_map(|rule| {
            let (before, combinator) = generated_subject(&rule.path)?;
            makes_content(rule.declarations.as_ref())
                .then(|| (before, host_path(&rule.path, combinator)))
        })
        .collect();
    let marker_rules: Vec<CssPath> = css
        .rules
        .as_ref()
        .iter()
        .filter(|rule| {
            generated_subject(&rule.path).is_none()
                && makes_inline_list_item(rule.declarations.as_ref())
        })
        .map(|rule| host_path(&rule.path, rule.path.selectors.as_ref().len()))
        .collect();
    let inline_list_items_inline = dom.node_data.internal.iter().any(|node| {
        node.style.iter_inline_properties().any(|(prop, _)| {
            matches!(
                prop,
                CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::InlineListItem))
            )
        })
    });
    if rules.is_empty() && marker_rules.is_empty() && !inline_list_items_inline {
        return false;
    }

    // Which nodes get which generated node, decided on the old arena.
    let n = dom.node_data.internal.len().min(dom.node_hierarchy.internal.len());
    let mut before = alloc::vec![false; n];
    let mut after = alloc::vec![false; n];
    let mut marker = alloc::vec![false; n];
    {
        let items: Vec<NodeHierarchyItem> = dom
            .node_hierarchy
            .internal
            .iter()
            .map(|node| (*node).into())
            .collect();
        let hierarchy = NodeDataContainerRef::from_slice(&items);
        let depths = dom.node_hierarchy.as_ref().get_parents_sorted_by_depth();
        let cascade = construct_html_cascade_tree(
            &dom.node_hierarchy.as_ref(),
            &depths,
            &dom.node_data.as_ref(),
        );
        let data = dom.node_data.as_ref();
        let in_any_state = AncestorStates {
            nodes: &[],
            window_inactive: false,
            every_state: true,
        };
        let mut in_svg = alloc::vec![false; n];
        for (i, node) in data.internal.iter().enumerate().take(n) {
            let id = NodeId::new(i);
            in_svg[i] = matches!(node.get_node_type(), NodeType::Svg)
                || dom.node_hierarchy.internal[i]
                    .parent
                    .is_some_and(|p| p.index() < i && in_svg[p.index()]);
            if in_svg[i] || !can_host(node) {
                continue;
            }
            marker[i] = node.style.iter_inline_properties().any(|(prop, _)| {
                matches!(
                    prop,
                    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::InlineListItem))
                )
            }) || marker_rules.iter().any(|host| {
                matches_html_element_in(
                    host,
                    id,
                    &hierarchy,
                    &data,
                    &cascade.as_ref(),
                    None,
                    Some(in_any_state),
                )
            });
            for (is_before, host) in &rules {
                let slot = if *is_before {
                    &mut before[i]
                } else {
                    &mut after[i]
                };
                if !*slot
                    && matches_html_element_in(
                        host,
                        id,
                        &hierarchy,
                        &data,
                        &cascade.as_ref(),
                        None,
                        Some(in_any_state),
                    )
                {
                    *slot = true;
                }
            }
        }
    }
    let generated = before
        .iter()
        .chain(after.iter())
        .chain(marker.iter())
        .filter(|b| **b)
        .count();
    if generated == 0 {
        return false;
    }

    // The new arena in document order: every old node, a `::before` right
    // after its host, a `::after` once the host's subtree has ended.
    let old_nodes = &dom.node_hierarchy.internal;
    let mut old_end: Vec<usize> = (0..n).collect();
    for i in (0..n).rev() {
        if let Some(last) = old_nodes[i].last_child {
            if last.index() < n {
                old_end[i] = old_end[last.index()];
            }
        }
    }
    let new_len = n + 2 * generated;
    let mut data: Vec<NodeData> = Vec::with_capacity(new_len);
    let mut parents: Vec<Option<usize>> = Vec::with_capacity(new_len);
    let mut old_to_new = alloc::vec![0usize; n];
    let mut new_end = alloc::vec![0usize; n];
    // The nodes whose subtree is still open, innermost last.
    let mut open: Vec<usize> = Vec::new();
    let old_data = core::mem::take(&mut dom.node_data.internal);
    for (i, node) in old_data.into_iter().enumerate().take(n) {
        while let Some(&top) = open.last() {
            if old_end[top] >= i {
                break;
            }
            open.pop();
            if after[top] {
                push_generated(NodeType::After, old_to_new[top], &mut data, &mut parents);
            }
            new_end[top] = data.len() - 1;
        }
        let here = data.len();
        old_to_new[i] = here;
        parents.push(
            old_nodes[i]
                .parent
                .filter(|p| p.index() < i)
                .map(|p| old_to_new[p.index()]),
        );
        data.push(node);
        // CSS Pseudo 4 s3: `::marker` comes first, then `::before`.
        if marker[i] {
            push_generated(NodeType::Marker, here, &mut data, &mut parents);
        }
        if before[i] {
            push_generated(NodeType::Before, here, &mut data, &mut parents);
        }
        open.push(i);
    }
    while let Some(top) = open.pop() {
        if after[top] {
            push_generated(NodeType::After, old_to_new[top], &mut data, &mut parents);
        }
        new_end[top] = data.len() - 1;
    }

    // Parent, sibling and last-child links of the new arena: children come
    // in document order, so each one is appended to its parent's list.
    let mut nodes = alloc::vec![Node::ROOT; data.len()];
    let mut last_child: Vec<Option<usize>> = alloc::vec![None; data.len()];
    for (child, parent) in parents.iter().enumerate() {
        let Some(parent) = *parent else {
            continue;
        };
        nodes[child].parent = Some(NodeId::new(parent));
        if let Some(previous) = last_child[parent] {
            nodes[child].previous_sibling = Some(NodeId::new(previous));
            nodes[previous].next_sibling = Some(NodeId::new(child));
        }
        last_child[parent] = Some(child);
        nodes[parent].last_child = Some(NodeId::new(child));
    }
    let new_len = data.len();
    dom.node_hierarchy = NodeHierarchy { internal: nodes };
    dom.node_data = NodeDataContainer { internal: data };

    // The stylesheet's scopes name node indices: they follow their nodes.
    let moved = Moved {
        old_to_new,
        old_end,
        new_end,
        new_len,
    };
    for rule in css.rules.as_mut() {
        let scoped = rule
            .path
            .selectors
            .as_ref()
            .iter()
            .any(|s| matches!(s, CssPathSelector::Root(_)));
        if !scoped {
            continue;
        }
        let mut selectors = rule.path.selectors.as_ref().to_vec();
        for selector in &mut selectors {
            if let CssPathSelector::Root(range) = selector {
                *range = moved.range(*range);
            }
        }
        rule.path.selectors = selectors.into();
    }
    true
}

impl CssPropertyCache {
    /// Makes a generated node `display: none` in every state its `content`
    /// makes nothing in (`none`, `normal`, or no `content` at all), as a
    /// browser makes no box for it then. In a state that has content while the
    /// resting state has none, the node keeps the display it would have had.
    ///
    /// Runs inside the cascade on the matched rules (`css_props` still in its
    /// build phase, the last entry of a type winning), before they are sorted.
    /// A generated node has no style of its own besides them.
    pub(crate) fn hide_contentless_generated_nodes(
        &mut self,
        node_data: &[NodeData],
        hierarchy: &[NodeHierarchyItem],
    ) {
        let display = |d: LayoutDisplay| CssProperty::Display(CssPropertyValue::Exact(d));
        for (index, node) in node_data.iter().enumerate() {
            if !node.is_generated_content() {
                continue;
            }
            if matches!(node.get_node_type(), NodeType::Marker) {
                // A marker shows while its host is an inline list item.
                let host = hierarchy
                    .get(index)
                    .and_then(NodeHierarchyItem::parent_id)
                    .map(|n| n.index());
                let host_display = host.and_then(|h| self.declared_display(node_data, h));
                if host_display != Some(LayoutDisplay::InlineListItem) {
                    self.css_props.push_to(
                        index,
                        StatefulCssProperty {
                            state: PseudoStateType::Normal,
                            prop_type: CssPropertyType::Display,
                            property: display(LayoutDisplay::None),
                            ua_origin: false,
                            priority: crate::prop_cache::ENGINE_PRIORITY,
                        },
                    );
                }
                continue;
            }
            let add: Vec<(PseudoStateType, CssProperty)> = {
                let Some(props) = self.css_props.build_get(index) else {
                    continue;
                };
                let last = |state: PseudoStateType, ty: CssPropertyType| {
                    props
                        .iter()
                        .rev()
                        .find(|p| p.state == state && p.prop_type == ty)
                        .map(|p| &p.property)
                };
                let shows = |state: PseudoStateType| {
                    last(state, CssPropertyType::Content)
                        .and_then(CssProperty::as_content)
                        .is_some_and(content_makes_box)
                };
                let resting = shows(PseudoStateType::Normal);
                let resting_display = last(PseudoStateType::Normal, CssPropertyType::Display)
                    .cloned()
                    .unwrap_or_else(|| display(LayoutDisplay::Inline));
                let mut states: Vec<PseudoStateType> = props
                    .iter()
                    .filter(|p| {
                        p.prop_type == CssPropertyType::Content
                            && p.state != PseudoStateType::Normal
                    })
                    .map(|p| p.state)
                    .collect();
                states.sort();
                states.dedup();
                let mut add = Vec::new();
                if !resting {
                    add.push((PseudoStateType::Normal, display(LayoutDisplay::None)));
                }
                for state in states {
                    let here = shows(state);
                    if here == resting {
                        continue;
                    }
                    if here {
                        // Its own display, or the one it would have had at
                        // rest, at the engine's priority: the hidden resting
                        // state must not outrank it.
                        let shown = last(state, CssPropertyType::Display)
                            .cloned()
                            .unwrap_or_else(|| resting_display.clone());
                        add.push((state, shown));
                    } else {
                        add.push((state, display(LayoutDisplay::None)));
                    }
                }
                add
            };
            for (state, property) in add {
                self.css_props.push_to(
                    index,
                    StatefulCssProperty {
                        state,
                        prop_type: CssPropertyType::Display,
                        property,
                        ua_origin: false,
                        priority: crate::prop_cache::ENGINE_PRIORITY,
                    },
                );
            }
        }
    }
}

impl CssPropertyCache {
    /// The `display` `node`'s own style or its matched rules declare for its
    /// resting state, during the cascade (`css_props` in its build phase, the
    /// last entry winning; the node's own declarations over the rules).
    fn declared_display(&self, node_data: &[NodeData], node: usize) -> Option<LayoutDisplay> {
        let own = node_data.get(node).and_then(|nd| {
            nd.style
                .iter_inline_properties()
                .filter(|(_, conditions)| conditions.as_slice().is_empty())
                .filter_map(|(prop, _)| match prop {
                    CssProperty::Display(CssPropertyValue::Exact(d)) => Some(*d),
                    _ => None,
                })
                .last()
        });
        own.or_else(|| {
            self.css_props.build_get(node).and_then(|props| {
                props
                    .iter()
                    .rev()
                    .find(|p| {
                        p.state == PseudoStateType::Normal
                            && p.prop_type == CssPropertyType::Display
                    })
                    .and_then(|p| match &p.property {
                        CssProperty::Display(CssPropertyValue::Exact(d)) => Some(*d),
                        _ => None,
                    })
            })
        })
    }
}

/// The text of the `::marker` of an inline list item: its number in its
/// list style followed by `. `, or its bullet followed by a space (CSS Lists
/// 3 s3.1, the `::marker` UA content); nothing for `list-style-type: none`.
fn marker_text(value: i32, style: StyleListStyleType) -> String {
    match style {
        StyleListStyleType::None => String::new(),
        StyleListStyleType::Disc | StyleListStyleType::Circle | StyleListStyleType::Square => {
            let mut text = format_counter(value, style);
            text.push(' ');
            text
        }
        _ => {
            let mut text = format_counter(value, style);
            text.push_str(". ");
            text
        }
    }
}

/// One counter (CSS Lists 3 s4.5): its value, the element that made it, and
/// the element whose subtree ends it - the maker's parent, so the maker's
/// following siblings see it too (CSS 2.2 s12.4.1). `None` for the root's.
struct Counter {
    name: String,
    value: i32,
    origin: usize,
    scope: Option<usize>,
    /// Made by an `<ol reversed>`: its list items count down.
    reversed: bool,
}

/// The counters in scope at a point of the document walk, innermost last.
#[derive(Default)]
struct Counters {
    stack: Vec<Counter>,
}

impl Counters {
    fn innermost(&self, name: &str) -> Option<usize> {
        self.stack.iter().rposition(|c| c.name == name)
    }

    /// `counter-reset`: a new counter made by `node`. One its previous sibling
    /// (or `node` itself) made is replaced rather than nested (CSS Lists 3
    /// s4.5, "instantiate a counter").
    fn reset(&mut self, name: &str, value: i32, node: usize, parent: Option<usize>) {
        if let Some(i) = self.innermost(name) {
            let counter = &self.stack[i];
            if counter.origin == node || counter.scope == parent {
                self.stack.remove(i);
            }
        }
        self.stack.push(Counter {
            name: name.to_string(),
            value,
            origin: node,
            scope: parent,
            reversed: false,
        });
    }

    /// The innermost counter `name`, made at `node` with 0 when none is in
    /// scope (as `counter-increment` and `counter()` make one).
    fn innermost_or_new(&mut self, name: &str, node: usize, parent: Option<usize>) -> usize {
        if let Some(i) = self.innermost(name) {
            return i;
        }
        self.reset(name, 0, node, parent);
        self.stack.len() - 1
    }

    fn increment(&mut self, name: &str, by: i32, node: usize, parent: Option<usize>) {
        let i = self.innermost_or_new(name, node, parent);
        self.stack[i].value = self.stack[i].value.saturating_add(by);
    }

    fn set(&mut self, name: &str, value: i32, node: usize, parent: Option<usize>) {
        let i = self.innermost_or_new(name, node, parent);
        self.stack[i].value = value;
    }

    fn value(&self, name: &str) -> i32 {
        self.innermost(name).map_or(0, |i| self.stack[i].value)
    }

    /// Every counter `name` in scope, outermost first (`counters()`).
    fn values(&self, name: &str) -> Vec<i32> {
        self.stack
            .iter()
            .filter(|c| c.name == name)
            .map(|c| c.value)
            .collect()
    }

    fn counts_down(&self, name: &str) -> bool {
        self.innermost(name).is_some_and(|i| self.stack[i].reversed)
    }

    /// `node`'s subtree ended: the counters its children made end with it.
    fn leave(&mut self, node: usize) {
        self.stack.retain(|c| c.scope != Some(node));
    }
}

/// An `<li value>`'s number: the item's own, the next items count on from it.
fn list_item_value(node: &NodeData) -> Option<i32> {
    if !matches!(node.get_node_type(), NodeType::Li) {
        return None;
    }
    node.get_attribute("value")
        .and_then(|v| v.as_str().trim().parse::<i32>().ok())
}

/// The value the `list-item` counter of an `<ol reversed>` starts at: its
/// `start`, else the number its items count down from - their number, or,
/// when an item sets its own (`<li value>`), that value plus the items before
/// it, so they count down into it (`a b c=30` numbers 32 31 30) - plus one,
/// which the first item's step takes away (CSS Lists 3 s4.4.2, as Chrome
/// numbers). `None` for a list that counts up.
fn reversed_list_start(
    node_data: &[NodeData],
    hierarchy: &[NodeHierarchyItem],
    node: usize,
) -> Option<i32> {
    let list = &node_data[node];
    if !matches!(list.get_node_type(), NodeType::Ol) {
        return None;
    }
    list.get_attribute("reversed")?;
    let start = list
        .get_attribute("start")
        .and_then(|s| s.as_str().trim().parse::<i32>().ok())
        .unwrap_or_else(|| {
            let mut items = 0_i32;
            let mut child = hierarchy[node].first_child_id(NodeId::new(node));
            while let Some(c) = child {
                let item = &node_data[c.index()];
                if matches!(item.get_node_type(), NodeType::Li) {
                    if let Some(value) = list_item_value(item) {
                        return value.saturating_add(items);
                    }
                    items = items.saturating_add(1);
                }
                child = hierarchy[c.index()].next_sibling_id();
            }
            items
        });
    Some(start.saturating_add(1))
}

/// The resolved `display` of a node: the compact cache in the resting state
/// (the cascade's own entries for it may be pruned), the cascade otherwise.
fn display_of(
    cache: &CssPropertyCache,
    node: &NodeData,
    id: NodeId,
    state: &StyledNodeState,
) -> Option<LayoutDisplay> {
    if state.is_normal() {
        if let Some(compact) = cache.compact_cache.as_ref() {
            return Some(compact.get_display(id.index()));
        }
    }
    cache
        .get_display(node, &id, state)
        .and_then(|v| v.get_property().copied())
}

/// Walks the document in order, counting its counters (CSS Lists 3 s4), and
/// hands every element to `visit` with the counters in scope once it has
/// reset, incremented and set its own (`counter-reset`, `counter-increment`
/// and `counter-set`, each a list): `(index, id, node, parent, counters)`.
/// An element with `display: none` makes no box, so it and its subtree count
/// nothing and are not visited.
#[allow(clippy::too_many_lines)] // one walk: scopes, resets, increments, the visit
fn walk_counters(
    node_data: &[NodeData],
    hierarchy: &[NodeHierarchyItem],
    cache: &CssPropertyCache,
    states: &[StyledNodeState],
    mut visit: impl FnMut(usize, NodeId, &NodeData, Option<usize>, &Counters),
) {
    let n = node_data.len().min(hierarchy.len());
    // The last node of every subtree: where the counters its children made end.
    let mut end: Vec<usize> = (0..n).collect();
    for i in (0..n).rev() {
        if let Some(last) = hierarchy[i].last_child_id() {
            if last.index() < n && last.index() > i {
                end[i] = end[last.index()];
            }
        }
    }

    let mut counters = Counters::default();
    let mut open: Vec<usize> = Vec::new();
    // The end of the `display: none` subtree the walk is in, if any.
    let mut hidden_until: Option<usize> = None;
    for (i, node) in node_data.iter().enumerate().take(n) {
        if hidden_until.is_some_and(|last| i <= last) {
            continue;
        }
        hidden_until = None;
        while let Some(&top) = open.last() {
            if end[top] >= i {
                break;
            }
            open.pop();
            counters.leave(top);
        }
        open.push(i);

        if node.is_text_node() {
            continue;
        }
        let id = NodeId::new(i);
        let state = states.get(i).copied().unwrap_or_default();
        let display = display_of(cache, node, id, &state);
        if display == Some(LayoutDisplay::None) {
            hidden_until = Some(end[i]);
            open.pop();
            continue;
        }
        let parent = hierarchy[i].parent_id().map(|n| n.index());

        // Almost no element declares a counter: the compact cache's bit
        // skips the two cascade lookups (the resting state only, like every
        // compact fast path).
        let may_count = !state.is_normal()
            || cache
                .compact_cache
                .as_ref()
                .is_none_or(|cc| cc.has_counter(i));
        let reset = if may_count {
            cache
                .get_counter_reset(node, &id, &state)
                .and_then(CssPropertyValue::get_property)
        } else {
            None
        };
        // A counter the list names twice is made once with its last value:
        // a reset replaces the counter the same element made.
        for c in reset.iter().flat_map(|r| r.counters.iter()) {
            counters.reset(c.counter_name.as_str(), c.value, i, parent);
        }
        if let Some(initial) = reversed_list_start(node_data, hierarchy, i) {
            let own = reset.is_some_and(|r| r.names("list-item"));
            if !own {
                counters.reset("list-item", initial, i, parent);
            }
            if let Some(k) = counters.innermost("list-item") {
                counters.stack[k].value = initial;
                counters.stack[k].reversed = true;
            }
        }

        let increment = if may_count {
            cache
                .get_counter_increment(node, &id, &state)
                .and_then(CssPropertyValue::get_property)
        } else {
            None
        };
        // A counter the list names twice steps once per name.
        for c in increment.iter().flat_map(|c| c.counters.iter()) {
            counters.increment(c.counter_name.as_str(), c.value, i, parent);
        }
        // A list item steps the `list-item` counter unless its own
        // `counter-increment` names that counter (CSS Lists 3 s4.6).
        let list_item = display.is_some_and(|d| d.is_list_item());
        if list_item && increment.is_none_or(|c| !c.names("list-item")) {
            let step = if counters.counts_down("list-item") { -1 } else { 1 };
            counters.increment("list-item", step, i, parent);
        }
        // `counter-set` comes last (CSS Lists 3 s4.3). HTML's `<li value>`
        // sets the item's number first, so an author's `counter-set` of
        // `list-item` still has the last word, as a declaration beats the
        // attribute's hint.
        if let Some(value) = list_item_value(node) {
            counters.set("list-item", value, i, parent);
        }
        let set = if may_count {
            cache
                .get_counter_set(node, &id, &state)
                .and_then(CssPropertyValue::get_property)
        } else {
            None
        };
        for c in set.iter().flat_map(|s| s.counters.iter()) {
            counters.set(c.counter_name.as_str(), c.value, i, parent);
        }

        visit(i, id, node, parent, &counters);
    }
}

/// The text of every generated node, `(its text node, the text)`, counting
/// the document's counters in document order on the way.
fn generated_texts(
    node_data: &[NodeData],
    hierarchy: &[NodeHierarchyItem],
    cache: &CssPropertyCache,
    states: &[StyledNodeState],
) -> Vec<(usize, String)> {
    let mut quote_depth = 0_usize;
    let mut out = Vec::new();
    walk_counters(
        node_data,
        hierarchy,
        cache,
        states,
        |i, id, node, parent, counters| {
            if matches!(node.get_node_type(), NodeType::Marker) {
                let Some(text_node) = hierarchy[i].first_child_id(id) else {
                    return;
                };
                let text = parent
                    .filter(|p| {
                        let host_state = states.get(*p).copied().unwrap_or_default();
                        display_of(cache, &node_data[*p], NodeId::new(*p), &host_state)
                            == Some(LayoutDisplay::InlineListItem)
                    })
                    .map_or_else(String::new, |p| {
                        let host_state = states.get(p).copied().unwrap_or_default();
                        let style = cache
                            .get_list_style_type(&node_data[p], &NodeId::new(p), &host_state)
                            .and_then(CssPropertyValue::get_property)
                            .copied()
                            .unwrap_or_default();
                        marker_text(counters.value("list-item"), style)
                    });
                out.push((text_node.index(), text));
            } else if node.is_generated_content() {
                let Some(text_node) = hierarchy[i].first_child_id(id) else {
                    return;
                };
                let host = parent.map(|p| &node_data[p]);
                let text = cache
                    .get_content(node, &id, &states.get(i).copied().unwrap_or_default())
                    .and_then(CssPropertyValue::get_property)
                    .map_or_else(String::new, |content| {
                        let value = content.inner.as_str();
                        let quotes = if value.contains("quote") {
                            quote_marks(node_data, hierarchy, cache, states, i)
                        } else {
                            Vec::new()
                        };
                        content_text(value, host, counters, &quotes, &mut quote_depth)
                    });
                out.push((text_node.index(), text));
            }
        },
    );
    out
}

/// The number every list item shows, by DOM node.
///
/// Its `list-item` counter (CSS Lists 3 s4.6: `counter-reset`, `<ol start /
/// reversed>`, `<li value>` and `counter-increment` taken into account), for
/// block list items, whose marker the layout makes, and inline ones alike.
/// One walk of the document; an element with `display: none` counts nothing.
#[must_use]
pub fn list_item_numbers(styled_dom: &StyledDom) -> BTreeMap<NodeId, i32> {
    let states: Vec<StyledNodeState> = styled_dom
        .styled_nodes
        .as_container()
        .internal
        .iter()
        .map(|n| n.styled_node_state)
        .collect();
    let node_data = styled_dom.node_data.as_container().internal;
    let hierarchy = styled_dom.node_hierarchy.as_container().internal;
    let cache = &styled_dom.css_property_cache.ptr;
    let mut numbers = BTreeMap::new();
    walk_counters(node_data, hierarchy, cache, &states, |i, id, node, _, counters| {
        let state = states.get(i).copied().unwrap_or_default();
        if display_of(cache, node, id, &state).is_some_and(|d| d.is_list_item()) {
            numbers.insert(id, counters.value("list-item"));
        }
    });
    numbers
}

/// Writes the text of every generated node from its `content`, counting the
/// document's counters on the way (CSS Lists 3 s4). `states` are the nodes'
/// interaction states (empty: none raised).
///
/// Runs after a cascade and reads what it resolved; walks the DOM once in
/// document order, the order counters count in. A DOM without generated
/// nodes returns after one scan. A text that did not change is left as it
/// is; the layout finds a changed one by its content, and a state change that
/// moves it also changes its generated node's `content`, which relays it out.
pub fn fill_generated_text(
    node_data: &mut [NodeData],
    hierarchy: &[NodeHierarchyItem],
    cache: &CssPropertyCache,
    states: &[StyledNodeState],
) {
    if !node_data.iter().any(NodeData::is_generated_content) {
        return;
    }
    let texts = generated_texts(node_data, hierarchy, cache, states);
    for (text_node, text) in texts {
        let Some(node) = node_data.get_mut(text_node) else {
            continue;
        };
        let same = matches!(
            node.get_node_type(),
            NodeType::Text(old) if old.as_ref().as_str() == text
        );
        if !same {
            node.set_node_type(NodeType::Text(BoxOrStatic::heap(AzString::from(text))));
        }
    }
}

impl StyledDom {
    /// Re-writes the generated text from the current cascade and interaction
    /// states: after a restyle, or when a state change switched a generated
    /// node's `content`.
    pub fn refill_generated_text(&mut self) {
        let states: Vec<StyledNodeState> = self
            .styled_nodes
            .as_container()
            .internal
            .iter()
            .map(|n| n.styled_node_state)
            .collect();
        let hierarchy = self.node_hierarchy.as_container().internal;
        let node_data = self.node_data.as_container_mut().internal;
        fill_generated_text(node_data, hierarchy, &self.css_property_cache.ptr, &states);
    }
}

/// The marks `open-quote` and `close-quote` write at node `i`, one pair per
/// nesting level: the strings of its `quotes`, none for `none`, and for
/// `auto` those of its language.
fn quote_marks(
    node_data: &[NodeData],
    hierarchy: &[NodeHierarchyItem],
    cache: &CssPropertyCache,
    states: &[StyledNodeState],
    i: usize,
) -> Vec<(String, String)> {
    let state = states.get(i).copied().unwrap_or_default();
    let quotes = cache
        .get_quotes(&node_data[i], &NodeId::new(i), &state)
        .and_then(CssPropertyValue::get_property)
        .cloned()
        .unwrap_or_default();
    if quotes.is_none() {
        return Vec::new();
    }
    if quotes.is_auto() {
        return owned_pairs(language_quotes(&language_of(node_data, hierarchy, i)));
    }
    let chars: Vec<char> = quotes.inner.as_str().chars().collect();
    let mut strings = Vec::new();
    let mut k = 0;
    while k < chars.len() {
        if chars[k] == '"' || chars[k] == '\'' {
            let (text, next) = read_string(&chars, k);
            strings.push(text);
            k = next;
        } else {
            k += 1;
        }
    }
    strings
        .chunks_exact(2)
        .map(|pair| (pair[0].clone(), pair[1].clone()))
        .collect()
}

fn owned_pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(open, close)| ((*open).to_string(), (*close).to_string()))
        .collect()
}

/// The primary language subtag of the nearest `lang` attribute at or above
/// node `i`, lowercase; empty when none is set.
fn language_of(node_data: &[NodeData], hierarchy: &[NodeHierarchyItem], mut i: usize) -> String {
    loop {
        if let Some(lang) = node_data.get(i).and_then(|n| n.get_attribute("lang")) {
            return lang
                .as_str()
                .split(['-', '_'])
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
        }
        match hierarchy.get(i).and_then(NodeHierarchyItem::parent_id) {
            Some(parent) => i = parent.index(),
            None => return String::new(),
        }
    }
}

/// The quotation marks of a language, outer pair first (the HTML rendering
/// section's table); English ones for a language it does not list.
fn language_quotes(lang: &str) -> &'static [(&'static str, &'static str)] {
    match lang {
        "de" => &[("\u{201e}", "\u{201c}"), ("\u{201a}", "\u{2018}")],
        "fr" => &[("\u{ab}\u{a0}", "\u{a0}\u{bb}"), ("\u{ab}\u{a0}", "\u{a0}\u{bb}")],
        "es" | "it" | "pt" => &[("\u{ab}", "\u{bb}"), ("\u{201c}", "\u{201d}")],
        "ru" => &[("\u{ab}", "\u{bb}"), ("\u{201e}", "\u{201c}")],
        "pl" => &[("\u{201e}", "\u{201d}"), ("\u{ab}", "\u{bb}")],
        "sv" | "fi" => &[("\u{201d}", "\u{201d}"), ("\u{2019}", "\u{2019}")],
        "ja" => &[("\u{300c}", "\u{300d}"), ("\u{300e}", "\u{300f}")],
        _ => &[("\u{201c}", "\u{201d}"), ("\u{2018}", "\u{2019}")],
    }
}

/// The text `content` makes for a generated node: its strings, the
/// attributes of `host`, its counters and its quotes (`quotes`: one mark
/// pair per nesting level, the last standing for deeper levels), in order.
/// `none` and `normal` make nothing; images (`url()`) are not text, and what
/// follows a `/` (the alternative text, for assistive technology) is not
/// shown.
fn content_text(
    value: &str,
    host: Option<&NodeData>,
    counters: &Counters,
    quotes: &[(String, String)],
    quote_depth: &mut usize,
) -> String {
    let mut out = String::new();
    if matches!(value.trim(), "" | "none" | "normal") {
        return out;
    }
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' {
            break;
        }
        if c == '"' || c == '\'' {
            let (text, next) = read_string(&chars, i);
            out.push_str(&text);
            i = next;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_')
        {
            i += 1;
        }
        let name: String = chars[start..i].iter().collect::<String>().to_ascii_lowercase();
        if i < chars.len() && chars[i] == '(' {
            let (args, next) = read_arguments(&chars, i + 1);
            i = next;
            out.push_str(&function_text(&name, &args, host, counters));
            continue;
        }
        match name.as_str() {
            "open-quote" => {
                if let Some((open, _)) = quotes.get(*quote_depth).or_else(|| quotes.last()) {
                    out.push_str(open);
                }
                *quote_depth += 1;
            }
            // A close-quote with no quotation open writes nothing (CSS 2.2
            // s12.3.2).
            "close-quote" if *quote_depth > 0 => {
                *quote_depth -= 1;
                if let Some((_, close)) = quotes.get(*quote_depth).or_else(|| quotes.last()) {
                    out.push_str(close);
                }
            }
            "no-open-quote" => *quote_depth += 1,
            "no-close-quote" => *quote_depth = quote_depth.saturating_sub(1),
            _ => {}
        }
        if i == start {
            // A character that starts no token: skip it.
            i += 1;
        }
    }
    out
}

/// The text of one function of a `content` value: `attr(name)`,
/// `counter(name, style)`, `counters(name, "separator", style)`. Anything
/// else (`url()`, gradients) makes no text.
fn function_text(
    name: &str,
    args: &[String],
    host: Option<&NodeData>,
    counters: &Counters,
) -> String {
    let style_at = |i: usize| {
        args.get(i)
            .and_then(|s| parse_style_list_style_type(s.trim()).ok())
            .unwrap_or(StyleListStyleType::Decimal)
    };
    let counter_name = args.first().map_or("", |s| s.trim());
    match name {
        "attr" => {
            let attribute = counter_name.split_whitespace().next().unwrap_or("");
            host.and_then(|h| h.get_attribute(attribute))
                .map_or_else(String::new, |v| v.as_str().to_string())
        }
        "counter" => format_counter(counters.value(counter_name), style_at(1)),
        "counters" => {
            let separator = args.get(1).map_or_else(String::new, |s| unquote(s));
            let style = style_at(2);
            let values = counters.values(counter_name);
            let values = if values.is_empty() { alloc::vec![0] } else { values };
            values
                .into_iter()
                .map(|v| format_counter(v, style))
                .collect::<Vec<_>>()
                .join(&separator)
        }
        _ => String::new(),
    }
}

/// A function argument that is a CSS string, without its quotes and escapes;
/// any other argument as it stands.
fn unquote(arg: &str) -> String {
    let arg = arg.trim();
    if arg.starts_with('"') || arg.starts_with('\'') {
        let chars: Vec<char> = arg.chars().collect();
        read_string(&chars, 0).0
    } else {
        arg.to_string()
    }
}

/// The CSS string whose opening quote is `chars[start]`: its text with the
/// escapes undone (CSS Syntax 3 s4.3.7: `\"`, `\\`, `\A`, `\2022 `), and
/// the index after its closing quote.
fn read_string(chars: &[char], start: usize) -> (String, usize) {
    let quote = chars[start];
    let mut out = String::new();
    let mut i = start + 1;
    while i < chars.len() {
        let c = chars[i];
        if c == quote {
            return (out, i + 1);
        }
        if c != '\\' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let Some(&next) = chars.get(i) else {
            break;
        };
        if next == '\n' {
            // An escaped newline continues the string on the next line.
            i += 1;
            continue;
        }
        if next.is_ascii_hexdigit() {
            let mut code = 0_u32;
            let mut digits = 0;
            while digits < 6 && i < chars.len() {
                let Some(d) = chars[i].to_digit(16) else {
                    break;
                };
                code = code * 16 + d;
                i += 1;
                digits += 1;
            }
            // One white space after the digits belongs to the escape.
            if i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            out.push(
                char::from_u32(code)
                    .filter(|c| *c != '\0')
                    .unwrap_or('\u{fffd}'),
            );
            continue;
        }
        out.push(next);
        i += 1;
    }
    (out, i)
}

/// The comma-separated arguments of a function whose `(` is just before
/// `start`, strings kept with their quotes, and the index after its `)`.
fn read_arguments(chars: &[char], start: usize) -> (Vec<String>, usize) {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut i = start;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            current.push(c);
            if c == '\\' {
                if let Some(&escaped) = chars.get(i + 1) {
                    current.push(escaped);
                    i += 1;
                }
            } else if c == q {
                quote = None;
            }
        } else {
            match c {
                '"' | '\'' => {
                    quote = Some(c);
                    current.push(c);
                }
                ',' => args.push(core::mem::take(&mut current)),
                ')' => {
                    args.push(current);
                    return (args, i + 1);
                }
                _ => current.push(c),
            }
        }
        i += 1;
    }
    args.push(current);
    (args, i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn english() -> Vec<(String, String)> {
        owned_pairs(language_quotes("en"))
    }

    fn text(value: &str) -> String {
        content_text(value, None, &Counters::default(), &english(), &mut 0)
    }

    #[test]
    fn strings_join_and_their_escapes_are_undone() {
        assert_eq!(text(r#""a" "b""#), "ab");
        assert_eq!(text(r#""\2022  x""#), "\u{2022} x", "one space ends the escape");
        assert_eq!(text(r#""say \"hi\"""#), "say \"hi\"");
        assert_eq!(text(r#"'it\'s'"#), "it's");
        assert_eq!(text(r#""line\A next""#), "line\nnext");
    }

    #[test]
    fn none_and_normal_make_nothing_and_the_alternative_text_is_not_shown() {
        assert_eq!(text("none"), "");
        assert_eq!(text("normal"), "");
        assert_eq!(text(r#""\2605" / "star""#), "\u{2605}");
        assert_eq!(text(r#"url(star.png) "x""#), "x");
    }

    #[test]
    fn quotes_nest() {
        let mut depth = 0;
        let counters = Counters::default();
        let open = content_text("open-quote", None, &counters, &english(), &mut depth);
        let inner = content_text("open-quote", None, &counters, &english(), &mut depth);
        let close_inner = content_text("close-quote", None, &counters, &english(), &mut depth);
        let close = content_text("close-quote", None, &counters, &english(), &mut depth);
        assert_eq!(
            (open.as_str(), inner.as_str(), close_inner.as_str(), close.as_str()),
            ("\u{201c}", "\u{2018}", "\u{2019}", "\u{201d}")
        );
        assert_eq!(depth, 0);
    }

    #[test]
    fn a_close_quote_without_an_open_one_writes_nothing() {
        let mut depth = 0;
        let counters = Counters::default();
        assert_eq!(content_text("close-quote", None, &counters, &english(), &mut depth), "");
        assert_eq!(depth, 0);
    }

    #[test]
    fn a_level_deeper_than_the_pairs_repeats_the_last_pair() {
        let mut depth = 0;
        let counters = Counters::default();
        let pairs = owned_pairs(&[("<", ">")]);
        let marks = content_text("open-quote open-quote", None, &counters, &pairs, &mut depth);
        assert_eq!(marks, "<<");
        assert_eq!(depth, 2);
    }

    #[test]
    fn a_language_without_marks_of_its_own_quotes_in_english() {
        assert_eq!(language_quotes("de")[0], ("\u{201e}", "\u{201c}"));
        assert_eq!(language_quotes("xx"), language_quotes("en"));
    }

    #[test]
    fn counters_write_every_level_in_their_style() {
        let mut counters = Counters::default();
        counters.reset("item", 1, 0, None);
        counters.reset("item", 2, 1, Some(0));
        let mut depth = 0;
        assert_eq!(
            content_text(r#"counters(item, ".")"#, None, &counters, &english(), &mut depth),
            "1.2"
        );
        assert_eq!(
            content_text("counter(item, upper-roman)", None, &counters, &english(), &mut depth),
            "II"
        );
        assert_eq!(
            content_text("counter(missing)", None, &counters, &english(), &mut depth),
            "0",
            "a counter no element made is 0"
        );
    }

    #[test]
    fn a_scope_follows_its_nodes_and_takes_in_their_generated_nodes() {
        // Old arena: 0 (root) > 1 (leaf), 2 > 3. Node 1 got a `::before` (two
        // new nodes after it), node 2 an `::after` (two new nodes at the end).
        let moved = Moved {
            old_to_new: alloc::vec![0, 1, 4, 5],
            old_end: alloc::vec![3, 1, 3, 3],
            new_end: alloc::vec![7, 3, 7, 5],
            new_len: 8,
        };
        let range = |start, end| CssScopeRange { start, end };
        assert_eq!(moved.range(range(0, 3)), range(0, 7), "the whole document");
        assert_eq!(
            moved.range(range(1, 1)),
            range(1, 3),
            "a leaf's subtree takes in its generated nodes"
        );
        assert_eq!(moved.range(range(2, 2)), range(4, 4), "a single node stays one");
        assert_eq!(moved.range(range(2, 3)), range(4, 7));
        assert_eq!(moved.range(range(9, 9)), range(13, 13), "past the end stays past it");
    }

    #[test]
    fn a_reset_by_a_sibling_replaces_the_counter_and_a_childs_ends_with_its_parent() {
        let mut counters = Counters::default();
        // Siblings 1 and 2 under node 0 both reset `c`: one counter, not two.
        counters.reset("c", 1, 1, Some(0));
        counters.reset("c", 5, 2, Some(0));
        assert_eq!(counters.values("c"), [5]);
        // Node 3, a child of 2, nests a new one ...
        counters.reset("c", 7, 3, Some(2));
        assert_eq!(counters.values("c"), [5, 7]);
        // ... which ends with node 2's subtree.
        counters.leave(2);
        assert_eq!(counters.values("c"), [5]);
    }
}
