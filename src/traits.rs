use dom::{NodeData, Dom};
use ui_description::{StyledNode, CssConstraintList, UiDescription};
use css::{Css, CssRule};
use window::WindowId;
use id_tree::{NodeId, Arena};
use std::rc::Rc;
use std::cell::RefCell;

pub trait LayoutScreen {
    /// Updates the DOM, must be provided by the final application.
    ///
    /// On each frame, a completely new DOM tree is generated. The final
    /// application can cache the DOM tree, but this isn't in the scope of `azul`.
    ///
    /// The `style_dom` looks through the given DOM rules, applies the style and
    /// recalculates the layout. This is done on each frame (except there are shortcuts
    /// when the DOM doesn't have to be recalculated).
    fn get_dom(&self, window_id: WindowId) -> Dom<Self> where Self: Sized;
    /// Provide access to the Css style for the application
    fn get_css(&mut self, window_id: WindowId) -> &mut Css;
    /// Applies the CSS styles to the nodes calculated from the `layout_screen`
    /// function and calculates the final display list that is submitted to the
    /// renderer.
    fn style_dom(dom: &Dom<Self>, css: &mut Css) -> UiDescription<Self> where Self: Sized {
        css.dirty = true;
        match_dom_css_selectors(dom.root, &dom.arena, &ParsedCss::from_css(css), css, &CssConstraintList::empty(), 0)
    }
}

pub(crate) struct ParsedCss<'a> {
    pub(crate) pure_global_rules: Vec<&'a CssRule>,
    pub(crate) pure_div_rules: Vec<&'a CssRule>,
    pub(crate) pure_class_rules: Vec<&'a CssRule>,
    pub(crate) pure_id_rules: Vec<&'a CssRule>,
}

impl<'a> ParsedCss<'a> {
    pub(crate) fn from_css(css: &'a Css) -> Self {

        // Parse the CSS nodes cascading by their importance
        // 1. global rules
        // 2. div-type ("html { }") specific rules
        // 3. class-based rules
        // 4. ID-based rules

        /*
            CssRule { html_type: "div", id: Some("main"), classes: [], declaration: ("direction", "row") }
            CssRule { html_type: "div", id: Some("main"), classes: [], declaration: ("justify-content", "center") }
            CssRule { html_type: "div", id: Some("main"), classes: [], declaration: ("align-items", "center") }
            CssRule { html_type: "div", id: Some("main"), classes: [], declaration: ("align-content", "center") }
        */

        // note: the following passes can be done in parallel ...

        // Global rules
        // * {
        //    background-color: blue;
        // }
        let pure_global_rules: Vec<&CssRule> = css.rules.iter().filter(|rule|
            rule.html_type == "*" && rule.id.is_none() && rule.classes.is_empty()
        ).collect();

        // Pure-div-type specific rules
        // button {
        //    justify-content: center;
        // }
        let pure_div_rules: Vec<&CssRule> = css.rules.iter().filter(|rule|
            rule.html_type != "*" && rule.id.is_none() && rule.classes.is_empty()
        ).collect();

        // Pure-class rules
        // NOTE: These classes are sorted alphabetically and are not duplicated
        //
        // .something .otherclass {
        //    text-color: red;
        // }
        let pure_class_rules: Vec<&CssRule> = css.rules.iter().filter(|rule|
            rule.id.is_none() && !rule.classes.is_empty()
        ).collect();

        // Pure-id rules
        // #something {
        //    background-color: red;
        // }
        let pure_id_rules: Vec<&CssRule> = css.rules.iter().filter(|rule|
            rule.id.is_some() && rule.classes.is_empty()
        ).collect();

        Self {
            pure_global_rules: pure_global_rules,
            pure_div_rules: pure_div_rules,
            pure_class_rules: pure_class_rules,
            pure_id_rules: pure_id_rules,
        }
    }
}

fn match_dom_css_selectors<T: LayoutScreen>(root: NodeId, arena: &Rc<RefCell<Arena<NodeData<T>>>>, parsed_css: &ParsedCss, css: &Css, parent_constraints: &CssConstraintList, parent_z_level: u32)
-> UiDescription<T>
{
    let arena_borrow = &*(*arena).borrow();
    let mut styled_nodes = Vec::<StyledNode>::new();
    let sibling_iterator = root.following_siblings(arena_borrow);
    // skip the root node itself, see documentation for `following_siblings` in id_tree.rs
    // sibling_iterator.next().unwrap();

    for sibling in sibling_iterator {
        styled_nodes.append(&mut match_dom_css_selectors_inner(sibling, arena_borrow, parsed_css, css, &parent_constraints, parent_z_level));
    }

    // match_dom_css_selectors_inner(root, &*(*arena).borrow(), parsed_css, css, parent_constraints, parent_z_level);
    UiDescription {
        // note: this clone is neccessary, otherwise, 
        // we wouldn't be able to update the UiState
        arena: (*arena).clone(),
        styled_nodes: styled_nodes,
    }
}

fn match_dom_css_selectors_inner<T: LayoutScreen>(root: NodeId, arena: &Arena<NodeData<T>>, parsed_css: &ParsedCss, css: &Css, parent_constraints: &CssConstraintList, parent_z_level: u32)
-> Vec<StyledNode>
{
    let mut styled_nodes = Vec::<StyledNode>::new();

    let mut current_constraints = CssConstraintList::empty();
    cascade_constraints(&arena[root].data, &mut current_constraints, parsed_css, css);

    let current_node = StyledNode {
        id: root,
        z_level: parent_z_level,
        css_constraints: current_constraints,
    };

    // DFS tree
    for child in root.children(arena) {
        styled_nodes.append(&mut match_dom_css_selectors_inner(child, arena, parsed_css, css, &current_node.css_constraints, parent_z_level + 1));
    }

    styled_nodes.push(current_node);
    styled_nodes
}

/// Cascade the rules, put them into the list
fn cascade_constraints<T: LayoutScreen>(node: &NodeData<T>, list: &mut CssConstraintList, parsed_css: &ParsedCss, css: &Css) {

    for global_rule in &parsed_css.pure_global_rules {
        push_rule(list, global_rule);
    }

    for div_rule in &parsed_css.pure_div_rules {
        if *node.node_type.get_css_id() == div_rule.html_type {
            push_rule(list, div_rule);
        }
    }

    let mut node_classes: Vec<&String> = node.classes.iter().map(|x| x).collect();
    node_classes.sort();
    node_classes.dedup_by(|a, b| *a == *b);

    // for all classes that this node has
    for class_rule in &parsed_css.pure_class_rules {
        // NOTE: class_rule is sorted and de-duplicated
        // If the selector matches, the node classes must be identical
        let mut should_insert_rule = true;
        if class_rule.classes.len() != node_classes.len() {
            should_insert_rule = false;
        } else {
            for i in 0..class_rule.classes.len() {
                // we verified that the length of the two classes is the same
                if *node_classes[i] != class_rule.classes[i] {
                    should_insert_rule = false;
                    break;
                }
            }
        }

        if should_insert_rule {
            push_rule(list, class_rule);
        }
    }

    // first attribute for "id = something"
    let node_id = &node.id;

    if let Some(ref node_id) = *node_id {
        // if the node has an ID
        for id_rule in &parsed_css.pure_id_rules {
            if *id_rule.id.as_ref().unwrap() == *node_id {
                push_rule(list, id_rule);
            }
        }
    }

    // TODO: all the mixed rules
}

#[inline]
fn push_rule(list: &mut CssConstraintList, rule: &CssRule) {
    list.list.insert(rule.declaration.0.clone(), rule.declaration.1.clone());
}