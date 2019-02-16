//! Module for parsing and loading a `Dom<T>` from a XML file

use std::{fmt, collections::BTreeMap};
use {
    dom::{Dom, Callback},
    traits::Layout,
};
use xmlparser::Tokenizer;
pub use xmlparser::{Error as XmlError, TokenType, TextPos, StreamError};

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

/// Trait that has to be implemented by all types
pub trait XmlComponent<T: Layout> {
    /// Given a root node and a component map, returns a DOM or a syntax error
    fn render_dom(&self, node: &XmlNode, component_map: &XmlComponentMap<T>) -> Result<Dom<T>, SyntaxError>;
    /// Used to compile the XML component to Rust code - input
    fn compile_to_rust_code(&self, node: &XmlNode, component_map: &XmlComponentMap<T>) -> Result<String, CompileError>;
}

/// Represents one XML node tag
#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct XmlNode {
    /// Type of the node
    pub tag_name: XmlTagName,
    /// Attributes of an XML node
    pub attributes: BTreeMap<XmlAttributeKey, XmlAttributeValue>,
    /// Direct children of this node
    pub children: Vec<XmlNode>,
    /// String content of the node, i.e the "Hello" in `<p>Hello</p>`
    pub text: Option<String>,
}

pub struct XmlComponentMap<T: Layout> {
    components: BTreeMap<String, Box<XmlComponent<T>>>,
    callbacks: BTreeMap<String, Callback<T>>,
}

impl<T: Layout> XmlComponentMap<T> {
    pub fn register_callback<S: Into<String>>(&mut self, id: S, callback: Callback<T>) {
        self.callbacks.insert(id.into(), callback);
    }
}

#[derive(Debug)]
pub enum XmlParseError {
    /// No `<app></app>` root component present
    NoRootComponent,
    /// The DOM can only have one root component, not multiple.
    MultipleRootComponents,
    /// **Note**: Sadly, the error type can only be a string because xmlparser
    /// returns all errors as strings. There is an open PR to fix
    /// this deficiency, but since the XML parsing is only needed for
    /// hot-reloading and compiling, it doesn't matter that much.
    ParseError(XmlError),
    /// Invalid hierarchy close tags, i.e `<app></p></app>`
    MalformedHierarchy(String, String),
}

impl fmt::Display for XmlParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::XmlParseError::*;
        match self {
            NoRootComponent => write!(f, "No <app></app> component present - empty DOM"),
            MultipleRootComponents => write!(f, "Multiple <app/> components present, only one root node is allowed"),
            ParseError(e) => write!(f, "XML parsing error: {}", e),
            MalformedHierarchy(got, expected) => write!(f, "Invalid </{}> tag: expected </{}>", got, expected),
        }
    }
}

/// Parses the XML string into an XML tree, returns
/// the root `<app></app>` node, with the children attached to it.
pub fn parse_xml(xml: &str) -> Result<XmlNode, XmlParseError> {

    use xmlparser::Token::*;
    use xmlparser::ElementEnd::*;
    use self::XmlParseError::*;

    let mut root_node = XmlNode {
        tag_name: "app".into(),
        attributes: BTreeMap::new(),
        children: Vec::new(),
        text: None,
    };

    let mut tokenizer = Tokenizer::from(xml);
    tokenizer.enable_fragment_mode();

    let mut stack = Vec::new();

    for token in tokenizer {

        let token = token.map_err(|e| ParseError(e))?;
        match token {
            ElementStart(_, open_value) => {
                stack.push(XmlNode {
                    tag_name: open_value.to_str().into(),
                    attributes: BTreeMap::new(),
                    children: Vec::new(),
                    text: None,
                });
            },
            ElementEnd(Empty) => {
                if let Some(top) = stack.pop() {
                    if stack.is_empty() {
                        // element is a top-level element
                        root_node.children.push(top);
                    } else {
                        // element has a parent, this is hard
                    }
                }
            },
            ElementEnd(Close(_, close_value)) => {
                if let Some(last) = stack.pop() {
                    let close_value = close_value.to_str();
                    if last.tag_name != close_value {
                        return Err(MalformedHierarchy(close_value.into(), last.tag_name.clone()));
                    }
                    if stack.is_empty() {
                        // element is a top-level element
                        root_node.children.push(last);
                    } else {
                        // element has a parent, this is hard
                    }
                }
            },
            Attribute((_, key), value) => {
                if let Some(last) = stack.last_mut() {
                    last.attributes.insert(key.to_str().into(), value.to_str().into());
                }
            },
            Text(t) => {
                if let Some(last) = stack.last_mut() {
                    if let Some(s) = last.text.as_mut() {
                        s.push_str(t.to_str());
                    }
                    if last.text.is_none() {
                        last.text = Some(t.to_str().into());
                    }
                }
            }
            _ => { },
        }
    }

    Ok(root_node)
}

/*
pub fn dom_from_xml<T: Layout>(
    xml: &str,
    component_map: &XmlComponentMap<T>
) -> Result<Dom<T>, XmlParseError> {

}
*/