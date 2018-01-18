use kuchiki::NodeRef;
use ui_state::UiState;
use ui_description::{StyledNode, CssConstraintList, UiDescription};
use css::{Css, CssRule};
use window::WindowId;

pub trait LayoutScreen {
	/// Updates the DOM, must be provided by the final application.
	///
	/// On each frame, a completely new DOM tree is generated. The final
	/// application can cache the DOM tree, but this isn't in the scope of `azul`.
	///
	/// The `style_dom` looks through the given DOM rules, applies the style and
	/// recalculates the layout. This is done on each frame (except there are shortcuts
	/// when the DOM doesn't have to be recalculated).
	fn update_dom(&self, old_ui_state: Option<&UiState>) -> NodeRef;
	/// Provide access to the Css style for the application
	fn get_css(&mut self, window_id: WindowId) -> &mut Css;
	/// Applies the CSS styles to the nodes calculated from the `layout_screen`
	/// function and calculates the final display list that is submitted to the
	/// renderer.
	fn style_dom(nodes: &NodeRef, css: &mut Css) -> UiDescription {
		css.dirty = true;
		match_dom_css_selectors(nodes, &ParsedCss::from_css(css), css, &CssConstraintList::empty(), 0)
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

fn match_dom_css_selectors(root: &NodeRef, parsed_css: &ParsedCss, css: &Css, parent_constraints: &CssConstraintList, parent_z_level: u32)
-> UiDescription
{
	let mut rectangles = Vec::<StyledNode>::new();

	let mut current_constraints = CssConstraintList::empty();
	cascade_constraints(root, &mut current_constraints, parsed_css, css);

	let current_node = StyledNode {
		node: root.clone(),
		z_level: parent_z_level,
		css_constraints: current_constraints,
	};

	// DFS tree
	for child in root.children() {
		let mut child_ui = match_dom_css_selectors(&child, parsed_css, css, &current_node.css_constraints, parent_z_level + 1);
		rectangles.append(&mut child_ui.rectangles);
	}

	for sibling in root.following_siblings() {
		let mut sibling_ui = match_dom_css_selectors(&sibling, parsed_css, css, &parent_constraints, parent_z_level);
		rectangles.append(&mut sibling_ui.rectangles);
	}

	rectangles.push(current_node);

	UiDescription {
		rectangles: rectangles,
	}
}

/// Cascade the rules, put them into the list
fn cascade_constraints(node: &NodeRef, list: &mut CssConstraintList, parsed_css: &ParsedCss, css: &Css) {
	use dom::{HTML_CLASS, HTML_ID};

	let node = match node.as_element() {
		None => return,
		Some(e) => e,
	};

	for global_rule in &parsed_css.pure_global_rules {
		push_rule(list, global_rule);
	}

	for div_rule in &parsed_css.pure_div_rules {
		if *node.name.local == div_rule.html_type {
			push_rule(list, div_rule);
		}
	}

	let node_attributes = node.attributes.borrow();

	// attributes for this node that have a "class = something"
	// TODO: I am not sure if the node.attributes allows for duplicated keys
	let mut node_classes: Vec<&String> = node_attributes.map.iter().filter_map(|e|
		if *e.0 == HTML_CLASS { Some(e.1) } else { None }
	).collect();

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
	let node_id: Option<&String> = node_attributes.map.iter().find(|e|
		*e.0 == HTML_ID
	).map(|e| e.1);

	if let Some(node_id) = node_id {
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