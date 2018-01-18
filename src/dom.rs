use kuchiki::NodeRef;

/// List of allowed DOM node types
///
/// The reason for this is because the markup5ever crate has
/// special macros for these node types, so either I need to expose the
/// whole markup5ever crate to the end user or I need to build a
/// wrapper type
pub enum NodeType {
	Div,
	Button,
	Ul,
	Li,
	Ol,
	Label,
	Input,
	Form,
}

impl Default for NodeType {
	fn default() -> Self {
		NodeType::Div
	}
}

pub struct DomNode {
	/// `div`
	pub node_type: NodeType,
	/// `#main`
	pub id: Option<String>,
	/// `.myclass .otherclass`
	pub classes: Vec<String>,
	/// `Hello World`
	pub text: Option<String>,
}

impl DomNode {

	/// Creates an empty node
	pub fn new(node_type: NodeType) -> Self {
		Self {
			node_type: node_type,
			id: None,
			classes: Vec::new(),
			text: None,
		}
	}

	#[inline]
	pub fn id<S: Into<String>>(self, id: S) -> Self {
		Self {
			node_type: self.node_type,
			id: Some(id.into()),
			classes: self.classes,
			text: self.text,
		}
	}

	#[inline]
	pub fn class<S: Into<String>>(self, class: S) -> Self {
		let mut classes = self.classes;
		classes.push(class.into());
		Self {
			node_type: self.node_type,
			id: self.id,
			classes: classes,
			text: self.text,
		}
	}

	#[inline]
	pub fn with_text<S: Into<String>>(self, text: S) -> Self {
		Self {
			node_type: self.node_type,
			id: self.id,
			classes: self.classes,
			text: Some(text.into()),
		}
	}
}

impl Into<NodeRef> for DomNode {
	fn into(self) -> NodeRef {

		use std::cell::RefCell;
		use std::collections::HashMap;
		use markup5ever::interface::QualName;
		use kuchiki::{NodeRef, Attributes, NodeData, ElementData};

		let mut attributes = HashMap::new();
		if let Some(id) = self.id {
			attributes.insert(QualName::new(None, ns!(html), local_name!("id")), id);
		}
		let attributes = RefCell::new(Attributes { map: attributes });

		use self::NodeType::*;
		let name = match self.node_type {
			Div => QualName::new(None, ns!(html), local_name!("div")),
			Button => QualName::new(None, ns!(html), local_name!("button")),
			Ul => QualName::new(None, ns!(html), local_name!("ul")),
			Li => QualName::new(None, ns!(html), local_name!("li")),
			Ol => QualName::new(None, ns!(html), local_name!("ol")),
			Label => QualName::new(None, ns!(html), local_name!("label")),
			Input => QualName::new(None, ns!(html), local_name!("input")),
			Form => QualName::new(None, ns!(html), local_name!("form")),
		};

		NodeRef::new(NodeData::Element(ElementData {
			name: name,
			attributes: attributes,
			template_contents: None,
		}))
	}
}