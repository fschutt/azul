//! Types and methods used to describe the style of an application

#[cfg(debug_assertions)]
use std::collections::BTreeMap;
use {
    style_properties::StyleProperty,
    traits::Layout,
    ui_description::{UiDescription, StyledNode},
    dom::{NodeTypePath, NodeData},
    ui_state::UiState,
    id_tree::{NodeId, NodeHierarchy, NodeDataContainer},
};

/// Wrapper for a `Vec<StyleRuleSet>` - the style is immutable at runtime, it can only be
/// created once. Animations / conditional styling is implemented using dynamic fields.
#[derive(Debug, Default, PartialEq, Clone)]
pub struct AppStyle {
    /// The style rules making up the document - for example, de-duplicated CSS rules
    pub rules: Vec<StyleRuleSet>,
}

impl std::convert::From<Vec<StyleRuleSet>> for AppStyle {
    fn from(rules: Vec<StyleRuleSet>) -> Self {
        Self { rules }
    }
}

/// Contains one parsed `key: value` pair, static or dynamic
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StyleDeclaration {
    /// Static key-value pair, such as `width: 500px`
    Static(StyleProperty),
    /// Dynamic key-value pair with default value, such as `width: [[ my_id | 500px ]]`
    Dynamic(DynamicStyleProperty),
}

impl StyleDeclaration {
    /// Determines if the property will be inherited (applied to the children)
    /// during the recursive application of the style on the DOM tree
    pub fn is_inheritable(&self) -> bool {
        use self::StyleDeclaration::*;
        match self {
            Static(s) => s.is_inheritable(),
            Dynamic(d) => d.is_inheritable(),
        }
    }
}

/// A `DynamicStyleProperty` is a type of style property that can be changed on possibly
/// every frame by the Rust code - for example to implement an `On::Hover` behaviour.
///
/// The syntax for such a property looks like this:
///
/// ```no_run,ignore
/// #my_div {
///    padding: [[ my_dynamic_property_id | 400px ]];
/// }
/// ```
///
/// Azul will register a dynamic property with the key "my_dynamic_property_id"
/// and the default value of 400px. If the property gets overridden during one frame,
/// the overridden property takes precedence.
///
/// At runtime the style is immutable (which is a performance optimization - if we
/// can assume that the property never changes at runtime), we can do some optimizations on it.
/// Dynamic style properties can also be used for animations and conditional styles
/// (i.e. `hover`, `focus`, etc.), thereby leading to cleaner code, since all of these
/// special cases now use one single API.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DynamicStyleProperty {
    /// The stringified ID of this property, i.e. the `"my_id"` in `width: [[ my_id | 500px ]]`.
    pub dynamic_id: String,
    /// Default value, used if the style property isn't overridden in this frame
    /// i.e. the `500px` in `width: [[ my_id | 500px ]]`.
    pub default: DynamicStylePropertyDefault,
}

/// If this value is set to default, the style property will not exist if it isn't overriden.
/// An example where this is useful is when you want to say something like this:
///
/// `width: [[ 400px | auto ]];`
///
/// "If I set this property to width: 400px, then use exactly 400px. Otherwise use whatever the default width is."
/// If this property wouldn't exist, you could only set the default to "0px" or something like
/// that, meaning that if you don't override the property, then you'd set it to 0px - which is
/// different from `auto`, since `auto` has its width determined by how much space there is
/// available in the parent.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DynamicStylePropertyDefault  {
    Exact(StyleProperty),
    Auto,
}

impl DynamicStyleProperty {
    pub fn is_inheritable(&self) -> bool {
        // Dynamic style properties should not be inheritable,
        // since that could lead to bugs - you set a property in Rust, suddenly
        // the wrong UI component starts to react because it was inherited.
        false
    }
}

/// One block of rules that applies a bunch of rules to an "xpath" in the style, i.e.
/// `div#myid.myclass -> { ("justify-content", "center") }`
#[derive(Debug, Clone, PartialEq)]
pub struct StyleRuleSet {
    /// The xpath (full selector) of the style ruleset
    pub path: XPath,
    /// `"justify-content: center"` =>
    /// `StyleDeclaration::Static(StyleProperty::JustifyContent(LayoutJustifyContent::Center))`
    pub declarations: Vec<StyleDeclaration>,
}

/// Represents a full style xpath:
/// `#div > .my_class:focus` =>
/// `[XPathSelector::Type(NodeTypePath::Div), DirectChildren, XPathSelector::Class("my_class"), XPathSelector::PseudoSelector]`
#[derive(Debug, Clone, Hash, Default, PartialEq)]
pub struct XPath {
    pub selectors: Vec<XPathSelector>,
}

/// Has all the necessary information about the style xpath
pub struct HtmlCascadeInfo<'a, T: 'a + Layout> {
    node_data: &'a NodeData<T>,
    index_in_parent: usize,
    is_last_child: bool,
    is_hovered_over: bool,
    is_focused: bool,
    is_active: bool,
}

impl XPath {

    /// Returns if the style xpath matches the DOM node (i.e. if the DOM node should be styled by that element)
    pub fn matches_html_element<'a, T: Layout>(
        &self,
        node_id: NodeId,
        node_hierarchy: &NodeHierarchy,
        html_node_tree: &NodeDataContainer<HtmlCascadeInfo<'a, T>>)
    -> bool
    {
        use self::CssGroupSplitReason::*;
        if self.selectors.is_empty() {
            return false;
        }

        let mut current_node = Some(node_id);
        let mut direct_parent_has_to_match = false;
        let mut last_selector_matched = false;

        for (content_group, reason) in CssGroupIterator::new(&self.selectors) {
            let cur_node_id = match current_node {
                Some(c) => c,
                None => {
                    // The node has no parent, but the CSS path
                    // still has an extra limitation - only valid if the
                    // next content group is a "*" element
                    return *content_group == [&XPathSelector::Global];
                },
            };
            let current_selector_matches = selector_group_matches(&content_group, &html_node_tree[cur_node_id]);
            if direct_parent_has_to_match && !current_selector_matches {
                // If the element was a ">" element and the current,
                // direct parent does not match, return false
                return false; // not executed (maybe this is the bug)
            }
            // Important: Set if the current selector has matched the element
            last_selector_matched = current_selector_matches;
            // Select if the next content group has to exactly match or if it can potentially be skipped
            direct_parent_has_to_match = reason == DirectChildren;
            current_node = node_hierarchy[cur_node_id].parent;
        }

        last_selector_matched
    }
}

type CssContentGroup<'a> = Vec<&'a XPathSelector>;

struct CssGroupIterator<'a> {
    pub css_path: &'a Vec<XPathSelector>,
    pub current_idx: usize,
    pub last_reason: CssGroupSplitReason,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum CssGroupSplitReason {
    Children,
    DirectChildren,
}

impl<'a> CssGroupIterator<'a> {
    pub fn new(css_path: &'a Vec<XPathSelector>) -> Self {
        let initial_len = css_path.len();
        Self {
            css_path,
            current_idx: initial_len,
            last_reason: CssGroupSplitReason::Children,
        }
    }
}

impl<'a> Iterator for CssGroupIterator<'a> {
    type Item = (CssContentGroup<'a>, CssGroupSplitReason);

    fn next(&mut self) -> Option<(CssContentGroup<'a>, CssGroupSplitReason)> {
        use self::XPathSelector::*;

        let mut new_idx = self.current_idx;

        if new_idx == 0 {
            return None;
        }

        let mut current_path = Vec::new();

        while new_idx != 0 {
            match self.css_path.get(new_idx - 1)? {
                Children => {
                    self.last_reason = CssGroupSplitReason::Children;
                    break;
                },
                DirectChildren => {
                    self.last_reason = CssGroupSplitReason::DirectChildren;
                    break;
                },
                other => current_path.push(other),
            }
            new_idx -= 1;
        }

        current_path.reverse();

        if new_idx == 0 {
            if current_path.is_empty() {
                None
            } else {
                // Last element of path
                self.current_idx = 0;
                Some((current_path, self.last_reason))
            }
        } else {
            // skip the "Children | DirectChildren" element itself
            self.current_idx = new_idx - 1;
            Some((current_path, self.last_reason))
        }
    }
}


#[test]
fn test_css_group_iterator() {

    use self::XPathSelector::*;

    // ".hello > #id_text.new_class div.content"
    // -> ["div.content", "#id_text.new_class", ".hello"]
    let selectors = vec![
        Class("hello".into()),
        DirectChildren,
        Id("id_test".into()),
        Class("new_class".into()),
        Children,
        Type(NodeTypePath::Div),
        Class("content".into()),
    ];

    let mut it = CssGroupIterator::new(&selectors);

    assert_eq!(it.next(), Some((vec![
       &Type(NodeTypePath::Div),
       &Class("content".into()),
    ], CssGroupSplitReason::Children)));

    assert_eq!(it.next(), Some((vec![
       &Id("id_test".into()),
       &Class("new_class".into()),
    ], CssGroupSplitReason::DirectChildren)));

    assert_eq!(it.next(), Some((vec![
        &Class("hello".into()),
    ], CssGroupSplitReason::DirectChildren))); // technically not correct

    assert_eq!(it.next(), None);

    // Test single class
    let selectors_2 = vec![
        Class("content".into()),
    ];

    let mut it = CssGroupIterator::new(&selectors_2);

    assert_eq!(it.next(), Some((vec![
       &Class("content".into()),
    ], CssGroupSplitReason::Children)));

    assert_eq!(it.next(), None);
}


fn construct_html_cascade_tree<'a, T: Layout>(
    input: &'a NodeDataContainer<NodeData<T>>,
    node_hierarchy: &NodeHierarchy,
    node_depths_sorted: &[(usize, NodeId)])
-> NodeDataContainer<HtmlCascadeInfo<'a, T>>
{
    let mut nodes = (0..node_hierarchy.len()).map(|_| HtmlCascadeInfo {
        node_data: &input[NodeId::new(0)],
        index_in_parent: 0,
        is_last_child: false,
        is_hovered_over: false,
        is_active: false,
        is_focused: false,
    }).collect::<Vec<_>>();

    for (_depth, parent_id) in node_depths_sorted {

        // Note: starts at 1 instead of 0
        let index_in_parent = parent_id.preceding_siblings(node_hierarchy).count();

        let parent_html_matcher = HtmlCascadeInfo {
            node_data: &input[*parent_id],
            index_in_parent: index_in_parent, // necessary for nth-child
            is_last_child: node_hierarchy[*parent_id].next_sibling.is_none(), // Necessary for :last selectors
            is_hovered_over: false, // TODO
            is_active: false, // TODO
            is_focused: false, // TODO
        };

        nodes[parent_id.index()] = parent_html_matcher;

        for (child_idx, child_id) in parent_id.children(node_hierarchy).enumerate() {
            let child_html_matcher = HtmlCascadeInfo {
                node_data: &input[child_id],
                index_in_parent: child_idx + 1, // necessary for nth-child
                is_last_child: node_hierarchy[child_id].next_sibling.is_none(),
                is_hovered_over: false, // TODO
                is_active: false, // TODO
                is_focused: false, // TODO
            };

            nodes[child_id.index()] = child_html_matcher;
        }
    }

    NodeDataContainer { internal: nodes }
}

/// Matches a single groupt of items, panics on Children or DirectChildren selectors
///
/// The intent is to "split" the CSS path into groups by selectors, then store and cache
/// whether the direct or any parent has matched the path correctly
fn selector_group_matches<'a, T: Layout>(selectors: &[&XPathSelector], html_node: &HtmlCascadeInfo<'a, T>) -> bool {
    use self::XPathSelector::*;

    for selector in selectors {
        match selector {
            Global => { },
            Type(t) => {
                if html_node.node_data.node_type.get_path() != *t {
                    return false;
                }
            },
            Class(c) => {
                if !html_node.node_data.classes.contains(c) {
                    return false;
                }
            },
            Id(id) => {
                if !html_node.node_data.ids.contains(id) {
                    return false;
                }
            },
            PseudoSelector(XPathPseudoSelector::First) => {
                // Notice: index_in_parent is 1-indexed
                if html_node.index_in_parent != 1 { return false; }
            },
            PseudoSelector(XPathPseudoSelector::Last) => {
                // Notice: index_in_parent is 1-indexed
                if !html_node.is_last_child { return false; }
            },
            PseudoSelector(XPathPseudoSelector::NthChild(x)) => {
                if html_node.index_in_parent != *x { return false; }
            },
            PseudoSelector(XPathPseudoSelector::Hover) => {
                if !html_node.is_hovered_over { return false; }
            },
            PseudoSelector(XPathPseudoSelector::Active) => {
                if !html_node.is_active { return false; }
            },
            PseudoSelector(XPathPseudoSelector::Focus) => {
                if !html_node.is_focused { return false; }
            },
            DirectChildren | Children => {
                panic!("Unreachable: DirectChildren or Children in CSS path!");
            },
        }
    }

    true
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum XPathSelector {
    /// Represents the `*` selector
    Global,
    /// `div`, `p`, etc.
    Type(NodeTypePath),
    /// `.something`
    Class(String),
    /// `#something`
    Id(String),
    /// `:something`
    PseudoSelector(XPathPseudoSelector),
    /// Represents the `>` selector
    DirectChildren,
    /// Represents the ` ` selector
    Children
}

impl Default for XPathSelector { fn default() -> Self { XPathSelector::Global } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum XPathPseudoSelector {
    /// `:first`
    First,
    /// `:last`
    Last,
    /// `:nth-child`
    NthChild(usize),
    /// `:hover` - mouse is over element
    Hover,
    /// `:active` - mouse is pressed and over element
    Active,
    /// `:focus` - element has received focus
    Focus,
}

impl AppStyle {
    /// Creates a new AppStyle with no style rules.
    pub fn new() -> Self {
        Default::default()
    }

    // Combines two parsed stylesheets into one, appending the rules of
    // `other` after the rules of `self`.
    pub fn merge(&mut self, mut other: Self) {
        self.rules.append(&mut other.rules);
    }

    /// Sort the style rules by their weight, so that the rules are applied in the correct order.
    /// Should always be called when a new style is loaded from an external source.
    pub(crate) fn sort_by_specificity(&mut self) {
        self.rules.sort_by(|a, b| get_specificity(&a.path).cmp(&get_specificity(&b.path)));
    }
}

/// Returns specificity of the given style path. Further information can be found on
/// [the w3 website](http://www.w3.org/TR/selectors/#specificity).
fn get_specificity(path: &XPath) -> (usize, usize, usize) {
    let id_count = path.selectors.iter().filter(|x|     if let XPathSelector::Id(_) = x {     true } else { false }).count();
    let class_count = path.selectors.iter().filter(|x|  if let XPathSelector::Class(_) = x {  true } else { false }).count();
    let div_count = path.selectors.iter().filter(|x|    if let XPathSelector::Type(_) = x {   true } else { false }).count();
    (id_count, class_count, div_count)
}

pub(crate) fn match_dom_selectors<T: Layout>(
    ui_state: &UiState<T>,
    style: &AppStyle)
-> UiDescription<T>
{
    use ui_solver::get_non_leaf_nodes_sorted_by_depth;

    let root = ui_state.dom.root;
    let arena_borrow = &*ui_state.dom.arena.borrow();
    let non_leaf_nodes = get_non_leaf_nodes_sorted_by_depth(&arena_borrow.node_layout);

    let mut styled_nodes = BTreeMap::<NodeId, StyledNode>::new();

    let html_tree = construct_html_cascade_tree(&arena_borrow.node_data, &arena_borrow.node_layout, &non_leaf_nodes);

    for (_depth, parent_id) in non_leaf_nodes {

        let mut parent_rules = styled_nodes.get(&parent_id).cloned().unwrap_or_default();

        // Iterate through all style rules, test if they match
        // This is technically O(n ^ 2), however, there are usually not that many style blocks,
        // so the cost of this should be insignificant.
        for applying_rule in style.rules.iter().filter(|rule| rule.path.matches_html_element(parent_id, &arena_borrow.node_layout, &html_tree)) {
            parent_rules.style_constraints.list.extend(applying_rule.declarations.clone());
        }

        let inheritable_rules: Vec<StyleDeclaration> = parent_rules.style_constraints.list.iter().filter(|prop| prop.is_inheritable()).cloned().collect();

        // For children: inherit from parents - filter children that themselves are not parents!
        for child_id in parent_id.children(&arena_borrow.node_layout) {
            let child_node = &arena_borrow.node_layout[child_id];
            match child_node.first_child {
                None => {

                    // Style children that themselves aren't parents
                    let mut child_rules = inheritable_rules.clone();

                    // Iterate through all style rules, test if they match
                    // This is technically O(n ^ 2), however, there are usually not that many style blocks,
                    // so the cost of this should be insignificant.
                    for applying_rule in style.rules.iter().filter(|rule| rule.path.matches_html_element(child_id, &arena_borrow.node_layout, &html_tree)) {
                        child_rules.extend(applying_rule.declarations.clone());
                    }

                    styled_nodes.insert(child_id, StyledNode { style_constraints: StyleConstraintList { list: child_rules }});
                },
                Some(_) => {
                    // For all children that themselves are parents, simply copy the inheritable rules
                    styled_nodes.insert(child_id, StyledNode { style_constraints: StyleConstraintList { list: inheritable_rules.clone() } });
                },
            }
        }

        styled_nodes.insert(parent_id, parent_rules);
    }

    UiDescription {
        // Note: this clone is necessary, otherwise,
        // we wouldn't be able to update the UiState
        //
        // WARNING: The UIState can modify the `arena` with its copy of the Rc !
        // Be careful about appending things to the arena, since that could modify
        // the UiDescription without you knowing!
        ui_descr_arena: ui_state.dom.arena.clone(),
        ui_descr_root: root,
        styled_nodes: styled_nodes,
        default_style_of_node: StyledNode::default(),
        dynamic_style_overrides: ui_state.dynamic_style_overrides.clone(),
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct StyleConstraintList {
    pub(crate) list: Vec<StyleDeclaration>
}

#[test]
fn test_specificity() {
    use self::XPathSelector::*;
    assert_eq!(get_specificity(&XPath { selectors: vec![Id("hello".into())] }), (1, 0, 0));
    assert_eq!(get_specificity(&XPath { selectors: vec![Class("hello".into())] }), (0, 1, 0));
    assert_eq!(get_specificity(&XPath { selectors: vec![Type(NodeTypePath::Div)] }), (0, 0, 1));
    assert_eq!(get_specificity(&XPath { selectors: vec![Id("hello".into()), Type(NodeTypePath::Div)] }), (1, 0, 1));
}

// Assert that order of the style items is correct (in order of xpath specificity, lowest-to-highest)
#[test]
fn test_specificity_sort() {
    use prelude::*;
    use self::XPathSelector::*;
    use dom::NodeTypePath::*;

    let mut input_style = AppStyle {
        rules: vec![
            // Rules are sorted from lowest-specificity to highest specificity
            StyleRuleSet { path: XPath { selectors: vec![Global] }, declarations: Vec::new() },
            StyleRuleSet { path: XPath { selectors: vec![Global, Type(Div), Class("my_class".into()), Id("my_id".into())] }, declarations: Vec::new() },
            StyleRuleSet { path: XPath { selectors: vec![Global, Type(Div), Id("my_id".into())] }, declarations: Vec::new() },
            StyleRuleSet { path: XPath { selectors: vec![Global, Id("my_id".into())] }, declarations: Vec::new() },
            StyleRuleSet { path: XPath { selectors: vec![Type(Div), Class("my_class".into()), Class("specific".into()), Id("my_id".into())] }, declarations: Vec::new() },
        ],
    };

    input_style.sort_by_specificity();

    let expected_style = AppStyle {
        rules: vec![
            // Rules are sorted from lowest-specificity to highest specificity
            StyleRuleSet { path: XPath { selectors: vec![Global] }, declarations: Vec::new() },
            StyleRuleSet { path: XPath { selectors: vec![Global, Id("my_id".into())] }, declarations: Vec::new() },
            StyleRuleSet { path: XPath { selectors: vec![Global, Type(Div), Id("my_id".into())] }, declarations: Vec::new() },
            StyleRuleSet { path: XPath { selectors: vec![Global, Type(Div), Class("my_class".into()), Id("my_id".into())] }, declarations: Vec::new() },
            StyleRuleSet { path: XPath { selectors: vec![Type(Div), Class("my_class".into()), Class("specific".into()), Id("my_id".into())] }, declarations: Vec::new() },
        ],
    };

    assert_eq!(input_style, expected_style);
}
