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
#[derive(Default, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct XmlNode {
    /// Type of the node
    pub tag_name: XmlTagName,
    /// Attributes of an XML node
    pub attributes: BTreeMap<XmlAttributeKey, XmlAttributeValue>,
    /// Direct children of this node
    pub children: Vec<XmlNode>,
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
}

impl fmt::Display for XmlParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::XmlParseError::*;
        match self {
            NoRootComponent => write!(f, "No <app></app> component present - empty DOM"),
            MultipleRootComponents => write!(f, "Multiple <app/> components present, only one root node is allowed"),
            ParseError(e) => write!(f, "XML parsing error: {}", e),
        }
    }
}

/// Parses the XML string into an XML tree, returns
/// the root `<app></app>` node, with the children attached to it.
pub fn parse_xml(xml: &str) -> Result<XmlNode, XmlParseError> {

    let root_node = XmlNode {
        tag_name: "app".into(),
        attributes: BTreeMap::new(),
        children: Vec::new(),
    };

    let mut tokenizer = Tokenizer::from(xml);
    tokenizer.enable_fragment_mode();

    for token in tokenizer {
        use xmlparser::Token::*;
        use xmlparser::ElementEnd::*;
        let token = token.map_err(|e| XmlParseError::ParseError(e))?;
        match token {
            ElementStart(_, open_value) => { println!("element start: {}", open_value); },
            ElementEnd(Empty) => { println!("element />"); },
            ElementEnd(Close(_, close_value)) => { println!("element end: {}", close_value); },
            Attribute((_, key), value) => { println!("attribute: {} - {}", key, value);},
            Text(t) => { println!("text: {}", t); }
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