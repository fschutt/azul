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

impl XmlNode {

    pub fn new(tag_name: &str) -> Self {
        Self {
            tag_name: tag_name.into(),
            .. Default::default()
        }
    }

    pub fn with_attribute(mut self, key: &str, value: &str) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    pub fn with_children(mut self, children: Vec<XmlNode>) -> Self {
        self.children = children;
        self
    }
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
impl fmt::Debug for XmlParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
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
///
/// Since the XML allows multiple root nodes, this function returns
/// a `Vec<XmlNode>` - which are the "root" nodes, containing all their
/// children recursively.
///
/// # Example
///
/// ```rust
/// # use azul::xml::{XmlNode, parse_xml};
/// assert_eq!(
///     parse_xml("<app><p /><div id='thing' /></app>").unwrap(),
///     vec![
///          XmlNode::new("app").with_children(vec![
///             XmlNode::new("p"),
///             XmlNode::new("div").with_attribute("id", "thing"),
///         ])
///     ]
/// )
/// ```
pub fn parse_xml_string(xml: &str) -> Result<Vec<XmlNode>, XmlParseError> {

    use xmlparser::Token::*;
    use xmlparser::ElementEnd::*;
    use self::XmlParseError::*;

    let mut root_node = XmlNode::default();

    let mut tokenizer = Tokenizer::from(xml);
    tokenizer.enable_fragment_mode();

    // In order to insert where the item is, let's say
    // [0 -> 1st element, 5th-element -> node]
    // we need to trach the index of the item in the parent.
    let mut current_hierarchy: Vec<usize> = Vec::new();

    for token in tokenizer {

        let token = token.map_err(|e| ParseError(e))?;
        match token {
            ElementStart(_, open_value) => {
                if let Some(current_parent) = get_item(&current_hierarchy, &mut root_node) {
                    let children_len = current_parent.children.len();
                    current_parent.children.push(XmlNode {
                        tag_name: open_value.to_str().into(),
                        attributes: BTreeMap::new(),
                        children: Vec::new(),
                        text: None,
                    });
                    current_hierarchy.push(children_len);
                }
            },
            ElementEnd(Empty) => {
                current_hierarchy.pop();
            },
            ElementEnd(Close(_, close_value)) => {
                let close_value = close_value.to_str();
                if let Some(last) = get_item(&current_hierarchy, &mut root_node) {
                    if last.tag_name != close_value {
                        return Err(MalformedHierarchy(close_value.into(), last.tag_name.clone()));
                    }
                }
                current_hierarchy.pop();
            },
            Attribute((_, key), value) => {
                if let Some(last) = get_item(&current_hierarchy, &mut root_node) {
                    last.attributes.insert(key.to_str().into(), value.to_str().into());
                }
            },
            Text(t) => {
                if let Some(last) = get_item(&current_hierarchy, &mut root_node) {
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

    Ok(root_node.children)
}

/// Given a root node, traverses along the hierarchy, and returns a
/// mutable reference to a child of the node if
fn get_item<'a>(hierarchy: &[usize], root_node: &'a mut XmlNode) -> Option<&'a mut XmlNode> {
    let mut current_node = &*root_node;
    let mut iter = hierarchy.iter();

    while let Some(item) = iter.next() {
        current_node = current_node.children.get(*item).as_ref()?;
    }

    // Safe because we only ever have one mutable reference, but
    // the borrow checker doesn't allow recursive mutable borrowing
    let node_ptr = current_node as *const XmlNode;
    let mut_node_ptr = node_ptr as *mut XmlNode;
    Some(unsafe { &mut *mut_node_ptr })
}

#[test]
fn test_xml_get_item() {

    // <a>
    //     <b/>
    //     <c/>
    //     <d/>
    //     <e/>
    // </a>
    // <f>
    //     <g>
    //         <h/>
    //     </g>
    //     <i/>
    // </f>
    // <j/>

    let mut tree = XmlNode::new("component")
    .with_children(vec![
        XmlNode::new("a")
        .with_children(vec![
            XmlNode::new("b"),
            XmlNode::new("c"),
            XmlNode::new("d"),
            XmlNode::new("e"),
        ]),
        XmlNode::new("f")
        .with_children(vec![
            XmlNode::new("g")
            .with_children(vec![XmlNode::new("h")]),
            XmlNode::new("i"),
        ]),
        XmlNode::new("j"),
    ]);

    assert_eq!(&get_item(&[], &mut tree).unwrap().tag_name, "component");
    assert_eq!(&get_item(&[0], &mut tree).unwrap().tag_name, "a");
    assert_eq!(&get_item(&[0, 0], &mut tree).unwrap().tag_name, "b");
    assert_eq!(&get_item(&[0, 1], &mut tree).unwrap().tag_name, "c");
    assert_eq!(&get_item(&[0, 2], &mut tree).unwrap().tag_name, "d");
    assert_eq!(&get_item(&[0, 3], &mut tree).unwrap().tag_name, "e");
    assert_eq!(&get_item(&[1], &mut tree).unwrap().tag_name, "f");
    assert_eq!(&get_item(&[1, 0], &mut tree).unwrap().tag_name, "g");
    assert_eq!(&get_item(&[1, 0, 0], &mut tree).unwrap().tag_name, "h");
    assert_eq!(&get_item(&[1, 1], &mut tree).unwrap().tag_name, "i");
    assert_eq!(&get_item(&[2], &mut tree).unwrap().tag_name, "j");

    assert_eq!(get_item(&[123213], &mut tree), None);
    assert_eq!(get_item(&[0, 1, 2], &mut tree), None);
}

/*
pub fn dom_from_xml<T: Layout>(
    xml: &str,
    component_map: &XmlComponentMap<T>
) -> Result<Dom<T>, XmlParseError> {

}
*/