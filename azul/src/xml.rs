//! Module for parsing and loading a `Dom<T>` from a XML file

use std::collections::BTreeMap;
use {
    dom::{Dom, Callback},
    traits::Layout,
};

/// Error that can happen during hot-reload -
/// stringified, since it is only used for printing and is not exposed in the public API
pub type SyntaxError = String;
/// Error that can happen from the translation from XML code to Rust code -
/// stringified, since it is only used for printing and is not exposed in the public API
pub type CompileError = String;

/// Tag of an XML node, such as the "button" in `<button>Hello</button>`.
pub type XmlTagName = String;
/// Unparsed content of an XML node, such as the "Hello" in `<button>Hello</button>`.
pub type XmlNodeContent = String;
/// Key of an attribute, such as the "color" in `<button color="blue">Hello</button>`.
pub type XmlAttributeKey = String;
/// Value of an attribute, such as the "blue" in `<button color="blue">Hello</button>`.
pub type XmlAttributeValue = String;

/// Represents one tag
#[derive(Default, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct XmlNode {
    pub tag_name: XmlTagName,
    pub attributes: BTreeMap<XmlAttributeKey, XmlAttributeValue>,
    pub children: Vec<XmlNode>,
    pub content: Option<XmlNodeContent>,
}

/// Parses an XML style sheet and returns the root XML nodes
/// (which, recursively, contain all children in a tree-like fashion)
fn parse_tree(_input: &str) -> Result<XmlNode, XmlParseError> {
    Ok(XmlNode::default())
}

/// Trait that has to be implemented by all types
pub trait XmlComponent<T: Layout> {
    /// Given a li
    fn render_dom(&self, node: &XmlNode) -> Result<Dom<T>, SyntaxError>;
    /// Used to compile the XML component to Rust code - input
    fn compile_to_rust_code(&self, node: &XmlNode) -> Result<String, CompileError>;
}

/*
impl<T: Layout> XmlComponent for Spreadsheet {
    fn render_dom() -> Dom<T> {
        let columns = kv.get("cols").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        let rows = kv.get("rows").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        Ok(Spreadsheet::new(columns, rows).dom())
    }

    fn compile_to_rust_code(kv: &HashMap<String, String>) -> Result<String, String> {
        let columns = kv.get("cols").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        let rows = kv.get("rows").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        format!("Spreadsheet::new({}, {}).dom()", columns, rows)
    }
}
*/

pub struct XmlComponentMap<T: Layout> {
    components: BTreeMap<String, Box<XmlComponent<T>>>,
    callbacks: BTreeMap<String, Callback<T>>,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum XmlParseError {
    /// The DOM can only have one root component, not multiple.
    MultipleRootComponents,
}

/*
pub fn dom_from_xml<T: Layout>(
    xml: &str,
    component_map: &XmlComponentMap<T>
) -> Result<Dom<T>, XmlParseError> {

}
*/