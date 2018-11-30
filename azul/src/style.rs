//! CSS parsing and styling

#[cfg(debug_assertions)]
use std::io::Error as IoError;
use std::collections::BTreeMap;
use {
    css_parser::StyleProperty,
    traits::Layout,
    ui_description::{UiDescription, StyledNode},
    dom::{NodeTypePath, NodeData},
    ui_state::UiState,
    id_tree::{NodeId, NodeHierarchy, NodeDataContainer},
};

/// Wrapper for a `Vec<CssRule>` - the CSS is immutable at runtime, it can only be
/// created once. Animations / conditional styling is implemented using dynamic fields
#[derive(Debug, Default, PartialEq, Clone)]
pub struct AppStyle {
    /// Path to hot-reload the CSS file from
    #[cfg(debug_assertions)]
    pub hot_reload_path: Option<String>,
    /// When hot-reloading, should the CSS file be appended to the built-in, native styles
    /// (equivalent to `NATIVE_CSS + include_str!(hot_reload_path)`)? Default: false
    #[cfg(debug_assertions)]
    pub hot_reload_override_native: bool,
    /// The CSS rules making up the document - i.e the rules of the CSS sheet de-duplicated
    pub rules: Vec<CssRuleBlock>,
    /// Has the CSS changed in a way where it needs a re-layout? - default:
    /// `true` in order to force a re-layout on the first frame
    ///
    /// Ex. if only a background color has changed, we need to redraw, but we
    /// don't need to re-layout the frame.
    pub needs_relayout: bool,
}

/// Contains one parsed `key: value` pair, static or dynamic
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CssDeclaration {
    /// Static key-value pair, such as `width: 500px`
    Static(StyleProperty),
    /// Dynamic key-value pair with default value, such as `width: [[ my_id | 500px ]]`
    Dynamic(DynamicCssProperty),
}

impl CssDeclaration {
    /// Determines if the property will be inherited (applied to the children)
    /// during the recursive application of the CSS on the DOM tree
    pub fn is_inheritable(&self) -> bool {
        use self::CssDeclaration::*;
        match self {
            Static(s) => s.is_inheritable(),
            Dynamic(d) => d.is_inheritable(),
        }
    }
}

/// A `DynamicCssProperty` is a type of CSS rule that can be changed on possibly
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
/// At runtime the CSS is immutable (which is a performance optimization - if we
/// can assume that the CSS never changes at runtime), we can do some optimizations on it.
/// Dynamic CSS properties can also be used for animations and conditional CSS
/// (i.e. `hover`, `focus`, etc.), thereby leading to cleaner code, since all of these
/// special cases now use one single API.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DynamicCssProperty {
    /// The stringified ID of this property, i.e. the `"my_id"` in `width: [[ my_id | 500px ]]`.
    pub dynamic_id: String,
    /// Default value, used if the CSS property isn't overridden in this frame
    /// i.e. the `500px` in `width: [[ my_id | 500px ]]`.
    pub default: DynamicCssPropertyDefault,
}

/// If this value is set to default, the CSS property will not exist if it isn't overriden.
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
pub enum DynamicCssPropertyDefault  {
    Exact(StyleProperty),
    Auto,
}

impl DynamicCssProperty {
    pub fn is_inheritable(&self) -> bool {
        // Dynamic CSS properties should not be inheritable,
        // since that could lead to bugs - you set a property in Rust, suddenly
        // the wrong UI component starts to react because it was inherited.
        false
    }
}

#[cfg(debug_assertions)]
#[derive(Debug)]
pub enum HotReloadError {
    Io(IoError, String),
    FailedToReload,
}

#[cfg(debug_assertions)]
impl_display! { HotReloadError, {
    Io(e, file) => format!("Failed to hot-reload CSS file: Io error: {} when loading file: \"{}\"", e, file),
    FailedToReload => "Failed to hot-reload CSS file",
}}

/// One block of rules that applies a bunch of rules to a "path" in the CSS, i.e.
/// `div#myid.myclass -> { ("justify-content", "center") }`
#[derive(Debug, Clone, PartialEq)]
pub struct CssRuleBlock {
    /// The path (full selector) of the CSS block
    pub path: CssPath,
    /// `"justify-content: center"` =>
    /// `CssDeclaration::Static(StyleProperty::JustifyContent(LayoutJustifyContent::Center))`
    pub declarations: Vec<CssDeclaration>,
}

/// Represents a full CSS path:
/// `#div > .my_class:focus` =>
/// `[CssPathSelector::Type(NodeTypePath::Div), DirectChildren, CssPathSelector::Class("my_class"), CssPathSelector::PseudoSelector]`
#[derive(Debug, Clone, Hash, Default, PartialEq)]
pub struct CssPath {
    pub selectors: Vec<CssPathSelector>,
}

/// Has all the necessary information about the CSS path
pub struct HtmlCascadeInfo<'a, T: 'a + Layout> {
    node_data: &'a NodeData<T>,
    index_in_parent: usize,
    is_last_child: bool,
    is_hovered_over: bool,
    is_focused: bool,
    is_active: bool,
}

impl CssPath {

    /// Returns if the CSS path matches the DOM node (i.e. if the DOM node should be styled by that element)
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
                    return *content_group == [&CssPathSelector::Global];
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

type CssContentGroup<'a> = Vec<&'a CssPathSelector>;

struct CssGroupIterator<'a> {
    pub css_path: &'a Vec<CssPathSelector>,
    pub current_idx: usize,
    pub last_reason: CssGroupSplitReason,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum CssGroupSplitReason {
    Children,
    DirectChildren,
}

impl<'a> CssGroupIterator<'a> {
    pub fn new(css_path: &'a Vec<CssPathSelector>) -> Self {
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
        use self::CssPathSelector::*;

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

    use self::CssPathSelector::*;

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
fn selector_group_matches<'a, T: Layout>(selectors: &[&CssPathSelector], html_node: &HtmlCascadeInfo<'a, T>) -> bool {
    use self::CssPathSelector::*;

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
            PseudoSelector(CssPathPseudoSelector::First) => {
                // Notice: index_in_parent is 1-indexed
                if html_node.index_in_parent != 1 { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::Last) => {
                // Notice: index_in_parent is 1-indexed
                if !html_node.is_last_child { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::NthChild(x)) => {
                if html_node.index_in_parent != *x { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::Hover) => {
                if !html_node.is_hovered_over { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::Active) => {
                if !html_node.is_active { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::Focus) => {
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
pub enum CssPathSelector {
    /// Represents the `*` selector
    Global,
    /// `div`, `p`, etc.
    Type(NodeTypePath),
    /// `.something`
    Class(String),
    /// `#something`
    Id(String),
    /// `:something`
    PseudoSelector(CssPathPseudoSelector),
    /// Represents the `>` selector
    DirectChildren,
    /// Represents the ` ` selector
    Children
}

impl Default for CssPathSelector { fn default() -> Self { CssPathSelector::Global } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum CssPathPseudoSelector {
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
    /// Sort the CSS rules by their weight, so that the rules are applied in the correct order
    pub fn sort_by_specificity(&mut self) {
        self.rules.sort_by(|a, b| get_specificity(&a.path).cmp(&get_specificity(&b.path)));
    }

    // Combines two parsed stylesheets into one, appending the rules of
    // `other` after the rules of `self`. Overrides `self.hot_reload_path` with
    // `other.hot_reload_path`
    pub fn merge(&mut self, mut other: Self) {
        self.rules.append(&mut other.rules);
        self.needs_relayout = self.needs_relayout || other.needs_relayout;

        #[cfg(debug_assertions)] {
            self.hot_reload_path = other.hot_reload_path;
            self.hot_reload_override_native = other.hot_reload_override_native;
        }
    }
/*
    /// **NOTE**: Only available in debug mode, can crash if the file isn't found
    #[cfg(debug_assertions)]
    pub fn hot_reload(file_path: &str) -> Result<Self, HotReloadError>  {
        use std::fs;
        let initial_css = fs::read_to_string(&file_path).map_err(|e| HotReloadError::Io(e, file_path.to_string()))?;
        let mut css = match Self::new_from_str(&initial_css) {
            Ok(o) => o,
            Err(e) => panic!("Hot reload CSS: Parsing error in file {}:\n{}\n", file_path, e),
        };
        css.hot_reload_path = Some(file_path.into());

        Ok(css)
    }*/
/*
    /// Same as `hot_reload`, but applies the OS-native styles first, before
    /// applying the user styles on top.
    #[cfg(debug_assertions)]
    pub fn hot_reload_override_native(file_path: &str) -> Result<Self, HotReloadError> {
        use std::fs;
        let initial_css = fs::read_to_string(&file_path).map_err(|e| HotReloadError::Io(e, file_path.to_string()))?;
        let mut css = match Self::override_native(&initial_css) {
            Ok(o) => o,
            Err(e) => panic!("Hot reload CSS: Parsing error in file {}:\n{}\n", file_path, e),
        };
        css.hot_reload_path = Some(file_path.into());
        css.hot_reload_override_native = true;

        Ok(css)
    }*/

    #[cfg(debug_assertions)]
    pub(crate) fn reload_css(&mut self) {
/*
        use std::fs;

        let file_path = if let Some(f) = &self.hot_reload_path {
            f.clone()
        } else {
            #[cfg(feature = "logging")] {
               error!("No file to hot-reload the CSS from!");
            }
            return;
        };

        #[allow(unused_variables)]
        let reloaded_css = match fs::read_to_string(&file_path) {
            Ok(o) => o,
            Err(e) => {
                #[cfg(feature = "logging")] {
                    error!("Failed to hot-reload \"{}\":\r\n{}\n", file_path, e);
                }
                return;
            },
        };

        let target_css = if self.hot_reload_override_native {
            format!("{}\r\n{}\n", NATIVE_CSS, reloaded_css)
        } else {
            reloaded_css
        };

        #[allow(unused_variables)]
        let mut css = match Self::new_from_str(&target_css) {
            Ok(o) => o,
            Err(e) => {
                #[cfg(feature = "logging")] {
                    error!("Failed to reload - parse error \"{}\":\r\n{}\n", file_path, e);
                }
                return;
            },
        };

        css.hot_reload_path = self.hot_reload_path.clone();
        css.hot_reload_override_native = self.hot_reload_override_native;

        *self = css;*/
    }
}

fn get_specificity(path: &CssPath) -> (usize, usize, usize) {
    // http://www.w3.org/TR/selectors/#specificity
    let id_count = path.selectors.iter().filter(|x|     if let CssPathSelector::Id(_) = x {     true } else { false }).count();
    let class_count = path.selectors.iter().filter(|x|  if let CssPathSelector::Class(_) = x {  true } else { false }).count();
    let div_count = path.selectors.iter().filter(|x|    if let CssPathSelector::Type(_) = x {   true } else { false }).count();
    (id_count, class_count, div_count)
}

pub(crate) fn match_dom_css_selectors<T: Layout>(
    ui_state: &UiState<T>,
    css: &AppStyle)
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

        // Iterate through all rules in the CSS style sheet, test if the
        // This is technically O(n ^ 2), however, there are usually not that many CSS blocks,
        // so the cost of this should be insignificant.
        for applying_rule in css.rules.iter().filter(|rule| rule.path.matches_html_element(parent_id, &arena_borrow.node_layout, &html_tree)) {
            parent_rules.css_constraints.list.extend(applying_rule.declarations.clone());
        }

        let inheritable_rules: Vec<CssDeclaration> = parent_rules.css_constraints.list.iter().filter(|prop| prop.is_inheritable()).cloned().collect();

        // For children: inherit from parents - filter children that themselves are not parents!
        for child_id in parent_id.children(&arena_borrow.node_layout) {
            let child_node = &arena_borrow.node_layout[child_id];
            match child_node.first_child {
                None => {

                    // Style children that themselves aren't parents
                    let mut child_rules = inheritable_rules.clone();

                    // Iterate through all rules in the CSS style sheet, test if the
                    // This is technically O(n ^ 2), however, there are usually not that many CSS blocks,
                    // so the cost of this should be insignificant.
                    for applying_rule in css.rules.iter().filter(|rule| rule.path.matches_html_element(child_id, &arena_borrow.node_layout, &html_tree)) {
                        child_rules.extend(applying_rule.declarations.clone());
                    }

                    styled_nodes.insert(child_id, StyledNode { css_constraints:  CssConstraintList { list: child_rules }});
                },
                Some(_) => {
                    // For all children that themselves are parents, simply copy the inheritable rules
                    styled_nodes.insert(child_id, StyledNode { css_constraints:  CssConstraintList { list: inheritable_rules.clone() } });
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
        dynamic_css_overrides: ui_state.dynamic_css_overrides.clone(),
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct CssConstraintList {
    pub(crate) list: Vec<CssDeclaration>
}

#[test]
fn test_specificity() {
    use self::CssPathSelector::*;
    assert_eq!(get_specificity(&CssPath { selectors: vec![Id("hello".into())] }), (1, 0, 0));
    assert_eq!(get_specificity(&CssPath { selectors: vec![Class("hello".into())] }), (0, 1, 0));
    assert_eq!(get_specificity(&CssPath { selectors: vec![Type(NodeTypePath::Div)] }), (0, 0, 1));
    assert_eq!(get_specificity(&CssPath { selectors: vec![Id("hello".into()), Type(NodeTypePath::Div)] }), (1, 0, 1));
}

// Assert that order of the CSS items is correct (in order of specificity, lowest-to-highest)
#[test]
fn test_specificity_sort() {
    use prelude::*;
    use self::CssPathSelector::*;
    use dom::NodeTypePath::*;

    let mut input_style = AppStyle {
        rules: vec![
            // Rules are sorted from lowest-specificity to highest specificity
            CssRuleBlock { path: CssPath { selectors: vec![Global] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Type(Div), Class("my_class".into()), Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Type(Div), Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Type(Div), Class("my_class".into()), Class("specific".into()), Id("my_id".into())] }, declarations: Vec::new() },
        ],
        needs_relayout: true,
        #[cfg(debug_assertions)]
        hot_reload_path: None,
        #[cfg(debug_assertions)]
        hot_reload_override_native: false,
    };

    input_style.sort_by_specificity();

    let expected_style = AppStyle {
        rules: vec![
            // Rules are sorted from lowest-specificity to highest specificity
            CssRuleBlock { path: CssPath { selectors: vec![Global] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Type(Div), Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Type(Div), Class("my_class".into()), Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Type(Div), Class("my_class".into()), Class("specific".into()), Id("my_id".into())] }, declarations: Vec::new() },
        ],
        needs_relayout: true,
        #[cfg(debug_assertions)]
        hot_reload_path: None,
        #[cfg(debug_assertions)]
        hot_reload_override_native: false,
    };

    assert_eq!(input_style, expected_style);
}
