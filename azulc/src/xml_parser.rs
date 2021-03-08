#![allow(unused_variables)]

use core::fmt;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::prelude::v1::Box;
use azul_core::{
    impl_from,
    dom::Dom,
    styled_dom::StyledDom,
};
use azul_css::{AzString, Css};
use azul_css_parser::CssParseError;
use xmlparser::Tokenizer;

#[cfg(feature = "std")]
use std::path::Path;

use crate::xml::XmlError;
use crate::xml::XmlParseError;

/// Error that can happen during hot-reload -
/// stringified, since it is only used for printing and is not exposed in the public API
pub type SyntaxError = String;

/// Tag of an XML node, such as the "button" in `<button>Hello</button>`.
pub type XmlTagName = String;
/// Key of an attribute, such as the "color" in `<button color="blue">Hello</button>`.
pub type XmlAttributeKey = String;
/// Value of an attribute, such as the "blue" in `<button color="blue">Hello</button>`.
pub type XmlAttributeValue = String;
/// (Unparsed) text content of an XML node, such as the "Hello" in `<button>Hello</button>`.
pub type XmlTextContent = Option<String>;
/// Attributes of an XML node, such as `["color" => "blue"]` in `<button color="blue" />`.
pub type XmlAttributeMap = BTreeMap<XmlAttributeKey, XmlAttributeValue>;

pub type ComponentArgumentName = String;
pub type ComponentArgumentType = String;
pub type ComponentArgumentOrder = usize;
pub type ComponentArgumentsMap = BTreeMap<ComponentArgumentName, (ComponentArgumentType, ComponentArgumentOrder)>;
pub type ComponentName = String;
pub type CompiledComponent = String;
pub type FilteredComponentArguments = ComponentArguments;

pub const DEFAULT_ARGS: [&str;7] = [
    "id",
    "class",
    "tabindex",
    "focusable",
    "accepts_text",
    "name",
    "args"
];

/// A component can take various arguments (to pass down to its children), which are then
/// later compiled into Rust function arguments - for example
///
/// ```xml,no_run,ignore
/// <component name="test" args="a: String, b: bool, c: HashMap<X, Y>">
///     <Button id="my_button" class="test_{{ a }}"> Is this true? Scientists say: {{ b }}</Button>
/// </component>
/// ```
///
/// ... will turn into the following (generated) Rust code:
///
/// ```rust,no_run,ignore
/// struct TestRendererArgs<'a> {
///     a: &'a String,
///     b: &'a bool,
///     c: &'a HashMap<X, Y>,
/// }
///
/// fn render_component_test<'a, T>(args: &TestRendererArgs<'a>) -> Dom {
///     Button::with_label(format!("Is this true? Scientists say: {:?}", args.b)).with_class(format!("test_{}", args.a))
/// }
/// ```
///
/// For this to work, a component has to note all its arguments and types that it can take.
/// If a type is not `str` or `String`, it will be formatted using the `{:?}` formatter
/// in the generated source code, otherwise the compiler will use the `{}` formatter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentArguments {
    /// The arguments of the component, i.e. `date => String`
    pub args: ComponentArgumentsMap,
    /// Whether this widget accepts text. Note that this will be passed as the first
    /// argument when rendering the Rust code.
    pub accepts_text: bool,
}

impl Default for ComponentArguments {
    fn default() -> Self {
        Self {
            args: ComponentArgumentsMap::default(),
            accepts_text: false,
        }
    }
}

impl ComponentArguments {
    fn new() -> Self {
        Self::default()
    }
}

/// Specifies a component that reacts to a parsed XML node
pub trait XmlComponent {

    /// Should return all arguments that this component can take - for example if you have a
    /// component called `Calendar`, which can take a `selectedDate` argument:
    ///
    /// ```xml,no_run,ignore
    /// <Calendar
    ///     selectedDate='01.01.2018'
    ///     minimumDate='01.01.1970'
    ///     maximumDate='31.12.2034'
    ///     firstDayOfWeek='sunday'
    ///     gridVisible='false'
    /// />
    /// ```
    /// ... then the `ComponentArguments` returned by this function should look something like this:
    ///
    /// ```rust,no_run,ignore
    /// impl XmlComponent for CalendarRenderer {
    ///     fn get_available_arguments(&self) -> ComponentArguments {
    ///         btreemap![
    ///             "selected_date" => "DateTime",
    ///             "minimum_date" => "DateTime",
    ///             "maximum_date" => "DateTime",
    ///             "first_day_of_week" => "WeekDay",
    ///             "grid_visible" => "bool",
    ///             /* ... */
    ///         ]
    ///     }
    /// }
    /// ```
    ///
    /// If a user instantiates a component with an invalid argument (i.e. `<Calendar asdf="false">`),
    /// the user will get an error that the component can't handle this argument. The types are not checked,
    /// but they are necessary for the XML-to-Rust compiler.
    ///
    /// When the XML is then compiled to Rust, the generated Rust code will look like this:
    ///
    /// ```rust,no_run,ignore
    /// render_component_calendar(&CalendarRendererArgs {
    ///     selected_date: DateTime::from("01.01.2018")
    ///     minimum_date: DateTime::from("01.01.2018")
    ///     maximum_date: DateTime::from("01.01.2018")
    ///     first_day_of_week: WeekDay::from("sunday")
    ///     grid_visible: false,
    ///     .. Default::default()
    /// })
    /// ```
    ///
    /// Of course, the code generation isn't perfect: For non-builtin types, the compiler will use
    /// `Type::from` to make the conversion. You can then take that generated Rust code and clean it up,
    /// put it somewhere else and create another component out of it - XML should only be seen as a
    /// high-level prototyping tool (to get around the problem of compile times), not as the final
    /// data format.
    fn get_available_arguments(&self) -> ComponentArguments;
    /// Given a root node and a list of possible arguments, returns a DOM or a syntax error
    fn render_dom(&self, components: &XmlComponentMap, arguments: &FilteredComponentArguments, content: &XmlTextContent) -> Result<StyledDom, RenderDomError>;
    /// Used to compile the XML component to Rust code - input
    fn compile_to_rust_code(&self, components: &XmlComponentMap, attributes: &FilteredComponentArguments, content: &XmlTextContent) -> Result<String, CompileError>;
    /// Returns the XML node for this component (necessary to compile the component into a function
    /// during the Rust compilation stage)
    fn get_xml_node(&self) -> XmlNode;
}

/// Wrapper for the XML parser - necessary to easily create a Dom from
/// XML without putting an XML solver into `azul-core`.
pub struct DomXml {
    pub parsed_dom: StyledDom,
}

impl DomXml {

    /// Parses and loads a DOM from an XML string
    ///
    /// Note: Needs at least one `<app></app>` node in order to not fail
    #[inline]
    pub fn new(xml: &str, component_map: &mut XmlComponentMap) -> Result<Self, DomXmlParseError> {
        let parsed_dom = parse_xml_string(xml)?;
        let dom = str_to_dom(&parsed_dom, component_map)?;
        Ok(Self {
            parsed_dom: dom,
        })
    }

    /// Creates a mock `<app></app>` wrapper, so that the `Self::new()` function doesn't fail
    pub fn mock(xml: &str) -> Self {
        let actual_xml = format!("<app>{}</app>", xml);
        Self::new(&actual_xml, &mut XmlComponentMap::default()).unwrap()
    }

    /// Loads, parses and builds a DOM from an XML file
    ///
    /// **Warning**: The file is reloaded from disk on every function call - do not
    /// use this in release builds! This function deliberately never fails: In an error case,
    /// the error gets rendered as a `NodeType::Label`.
    #[cfg(feature = "std")]
    pub fn from_file<I: AsRef<Path>>(file_path: I, component_map: &mut XmlComponentMap) -> Self {

        use std::fs;

        let xml = match fs::read_to_string(file_path) {
            Ok(xml) => xml,
            Err(e) => return Self {
                parsed_dom: Dom::label(format!("{:?}", e)).style(Css::empty()),
            },
        };

        match Self::new(&xml, component_map) {
            Ok(o) => o,
            Err(e) =>  Self {
                parsed_dom: Dom::label(format!("{:?}", e)).style(Css::empty()),
            },
        }
    }

    /// Convenience function, only available in tests, useful for quickly writing UI tests.
    /// Wraps the XML string in the required `<app></app>` braces, panics if the XML couldn't be parsed.
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// # use azul::dom::Dom;
    /// # use azul::xml::DomXml;
    /// let dom = DomXml::mock("<div id='test' />");
    /// dom.assert_eq(Dom::div().with_id("test"));
    /// ```
    #[cfg(test)]
    pub fn assert_eq(self, other: StyledDom) {
        let fixed = StyledDom::body().append(other);
        if self.parsed_dom != fixed {
            panic!("\r\nExpected DOM did not match:\r\n\r\nexpected: ----------\r\n{}\r\ngot: ----------\r\n{}\r\n",
                expected.get_html_string(), fixed.get_html_string()
            );
        }
    }

    pub fn into_styled_dom(self) -> StyledDom {
        self.into()
    }
}

impl Into<StyledDom> for DomXml {
    fn into(self) -> StyledDom {
        self.parsed_dom
    }
}

/// Component that was created from a XML node (instead of being registered from Rust code).
/// Necessary to
pub struct DynamicXmlComponent {
    /// What the name of this component is, i.e. "test" for `<component name="test" />`
    pub name: String,
    /// Whether this component has any `args="a: String"` arguments
    pub arguments: ComponentArguments,
    /// Root XML node of this component (the `<component />` Node)
    pub root: XmlNode,
}

impl DynamicXmlComponent {

    /// Parses a `component` from an XML node
    pub fn new(root: XmlNode) -> Result<Self, ComponentParseError> {

        let node_type = normalize_casing(&root.node_type);

        if node_type.as_str() != "component" {
            return Err(ComponentParseError::NotAComponent);
        }

        let name = root.attributes.get("name").cloned().ok_or(ComponentParseError::NotAComponent)?;
        let accepts_text = root.attributes.get("accepts_text").and_then(|p| parse_bool(p.as_str())).unwrap_or(false);

        let args = match root.attributes.get("args") {
            Some(s) => parse_component_arguments(s)?,
            None => ComponentArgumentsMap::default(),
        };

        Ok(Self {
            name: normalize_casing(&name),
            arguments: ComponentArguments {
                args,
                accepts_text,
            },
            root,
        })
    }
}

impl XmlComponent for DynamicXmlComponent {

    fn get_available_arguments(&self) -> ComponentArguments {
        self.arguments.clone()
    }

    fn get_xml_node(&self) -> XmlNode {
        self.root.clone()
    }

    fn render_dom(
        &self,
        components: &XmlComponentMap,
        arguments: &FilteredComponentArguments,
        content: &XmlTextContent,
    ) -> Result<StyledDom, RenderDomError> {

        let component_style = parse_style(&self.root.children).unwrap_or(Css::empty());

        let mut dom = Dom::div().style(Css::empty());

        for child_node in &self.root.children {
            dom.append(render_dom_from_body_node_inner(child_node, components, arguments)?);
        }

        dom.restyle(component_style);

        Ok(dom)
    }

    fn compile_to_rust_code(
        &self,
        components: &XmlComponentMap,
        attributes: &FilteredComponentArguments,
        content: &XmlTextContent,
    ) -> Result<String, CompileError> {
        Ok("Dom::div()".into()) // TODO!s
    }
}

/// Represents one XML node tag
#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct XmlNode {
    /// Type of the node
    pub node_type: XmlTagName,
    /// Attributes of an XML node (note: not yet filtered and / or broken into function arguments!)
    pub attributes: XmlAttributeMap,
    /// Direct children of this node
    pub children: Vec<XmlNode>,
    /// String content of the node, i.e the "Hello" in `<p>Hello</p>`
    pub text: XmlTextContent,
}

impl XmlNode {

    pub fn new<S: Into<String>>(node_type: S) -> Self {
        Self {
            node_type: node_type.into(),
            .. Default::default()
        }
    }

    pub fn with_attribute<S: Into<String>>(mut self, key: S, value: S) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    pub fn with_children(mut self, children: Vec<XmlNode>) -> Self {
        self.children = children;
        self
    }

    pub fn with_text<S: Into<String>>(mut self, text: S) -> Self {
        self.text = Some(text.into());
        self
    }
}

/// Holds all XML components - builtin components
pub struct XmlComponentMap {
    /// Stores all known components that can be used during DOM rendering
    /// + whether this component should inherit variables from the parent scope
    components: BTreeMap<String, (Box<dyn XmlComponent>, bool)>,
}

impl Default for XmlComponentMap {
    fn default() -> Self {
        let mut map = Self { components: BTreeMap::new() };
        map.register_component("body", Box::new(BodyRenderer { }), true);
        map.register_component("div", Box::new(DivRenderer { }), true);
        map.register_component("p", Box::new(TextRenderer { }), true);
        map
    }
}

impl XmlComponentMap {
    pub fn register_component<S: AsRef<str>>(&mut self, id: S, component: Box<dyn XmlComponent>, inherit_variables: bool) {
        self.components.insert(normalize_casing(id.as_ref()), (component, inherit_variables));
    }
}

#[derive(Debug)]
pub enum DomXmlParseError {
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
    MalformedHierarchy(AzString, AzString),
    /// A component raised an error while rendering the DOM - holds the component name + error string
    RenderDom(RenderDomError),
    /// Something went wrong while parsing an XML component
    Component(ComponentParseError),
}

impl_from!{ XmlError, DomXmlParseError::ParseError }
impl_from!{ ComponentParseError, DomXmlParseError::Component }
impl_from!{ RenderDomError, DomXmlParseError::RenderDom }

/// Error that can happen from the translation from XML code to Rust code -
/// stringified, since it is only used for printing and is not exposed in the public API
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompileError {
    Dom(RenderDomError),
    Other(String),
}

impl From<RenderDomError> for CompileError {
    fn from(e: RenderDomError) -> Self {
        CompileError::Dom(e)
    }
}

impl From<String> for CompileError {
    fn from(e: String) -> Self {
        CompileError::Other(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RenderDomError {
    /// While instantiating a component, a function argument was encountered that the component won't use or react to.
    UselessFunctionArgument(String, String, Vec<String>),
    /// A certain node type can't be rendered, because the renderer isn't available
    UnknownComponent(String),
}

#[derive(Debug)]
pub enum ComponentParseError {
    /// Given XmlNode is not a `<component />` node.
    NotAComponent,
    /// A `<component>` node does not have a `name` attribute.
    UnnamedComponent,
    /// Argument at position `usize` is either empty or has no name
    MissingName(usize),
    /// Argument at position `usize` with the name `String` doesn't have a `: type`
    MissingType(usize, String),
    /// Component name may not contain a whitespace (probably missing a `:` between the name and the type)
    WhiteSpaceInComponentName(usize, String),
    /// Component type may not contain a whitespace (probably missing a `,` between the type and the next name)
    WhiteSpaceInComponentType(usize, String, String),
    /// Error parsing the <style> tag / CSS
    CssError(String),
}

impl fmt::Display for DomXmlParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::DomXmlParseError::*;
        match self {
            NoRootComponent => write!(f, "No <app></app> component present - empty DOM"),
            MultipleRootComponents => write!(f, "Multiple <app/> components present, only one root node is allowed"),
            ParseError(e) => write!(f, "XML parsing error: {:?}", e),
            MalformedHierarchy(got, expected) => write!(f, "Invalid </{}> tag: expected </{}>", got.as_str(), expected.as_str()),
            RenderDom(e) => write!(f, "Error while rendering DOM: \"{}\"", e),
            Component(c) => write!(f, "Error while parsing XML component: \"{}\"", c),
        }
    }
}

impl fmt::Display for ComponentParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ComponentParseError::*;
        match self {
            NotAComponent => write!(f, "Expected <component/> node, found no such node"),
            UnnamedComponent => write!(f, "Found <component/> tag with out a \"name\" attribute, component must have a name"),
            MissingName(arg_pos) => write!(f, "Argument at position {} is either empty or has no name", arg_pos),
            MissingType(arg_pos, arg_name) => write!(f, "Argument \"{}\" at position {} doesn't have a `: type`", arg_pos, arg_name),
            WhiteSpaceInComponentName(arg_pos, arg_name_unparsed) => {
                write!(f, "Missing `:` between the name and the type in argument {} (around \"{}\")", arg_pos, arg_name_unparsed)
            },
            WhiteSpaceInComponentType(arg_pos, arg_name, arg_type_unparsed) => {
                write!(f,
                       "Missing `,` between two arguments (in argument {}, position {}, around \"{}\")",
                       arg_name, arg_pos, arg_type_unparsed
                )
            },
            CssError(lsf) => write!(f, "Error parsing <style> tag: {}", lsf),
        }
    }
}

impl fmt::Display for RenderDomError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::RenderDomError::*;
        match self {
            UselessFunctionArgument(k, v, available_args) => {
                write!(f, "Useless component argument \"{}\": \"{}\" - available args are: {:#?}", k, v, available_args)
            },
            UnknownComponent(name) => write!(f, "Unknown component: \"{}\"", name),
        }
    }
}

/*
#[cfg(all(feature = "image_loading", feature = "font_loading"))]
use azul_core::{
    window::LogicalSize,
    display_list::CachedDisplayList
};

#[cfg(all(feature = "image_loading", feature = "font_loading"))]

*/

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
/// # use azulc::xml::{XmlNode, parse_xml_string};
/// assert_eq!(
///     parse_xml_string("<app><p /><div id='thing' /></app>").unwrap(),
///     vec![
///          XmlNode::new("app").with_children(vec![
///             XmlNode::new("p"),
///             XmlNode::new("div").with_attribute("id", "thing"),
///         ])
///     ]
/// )
/// ```
pub fn parse_xml_string(xml: &str) -> Result<Vec<XmlNode>, XmlError> {

    use xmlparser::Token::*;
    use xmlparser::ElementEnd::*;

    let mut root_node = XmlNode::default();

    let tokenizer = Tokenizer::from_fragment(xml, 0..xml.len());

    // In order to insert where the item is, let's say
    // [0 -> 1st element, 5th-element -> node]
    // we need to trach the index of the item in the parent.
    let mut current_hierarchy: Vec<usize> = Vec::new();

    for token in tokenizer {

        let token = token.map_err(|e| XmlError::ParserError(e.into()))?;
        match token {
            ElementStart { local, .. } => {
                if let Some(current_parent) = get_item(&current_hierarchy, &mut root_node) {
                    let children_len = current_parent.children.len();
                    current_parent.children.push(XmlNode {
                        node_type: normalize_casing(local.as_str()),
                        attributes: BTreeMap::new(),
                        children: Vec::new(),
                        text: None,
                    });
                    current_hierarchy.push(children_len);
                }
            },
            ElementEnd { end: Empty, .. } => {
                current_hierarchy.pop();
            },
            ElementEnd { end: Close(_, close_value), .. } => {
                let close_value = normalize_casing(close_value.as_str());
                if let Some(last) = get_item(&current_hierarchy, &mut root_node) {
                    if last.node_type != close_value {
                        return Err(XmlError::MalformedHierarchy(close_value.into(), last.node_type.clone().into()));
                    }
                }
                current_hierarchy.pop();
            },
            Attribute { local, value, .. } => {
                if let Some(last) = get_item(&current_hierarchy, &mut root_node) {
                    // NOTE: Only lowercase the key ("local"), not the value!
                    last.attributes.insert(normalize_casing(local.as_str()), value.as_str().to_string());
                }
            },
            Text { text } => {
                if let Some(last) = get_item(&current_hierarchy, &mut root_node) {
                    if let Some(s) = last.text.as_mut() {
                        s.push_str(text.as_str());
                    }
                    if last.text.is_none() {
                        last.text = Some(text.as_str().into());
                    }
                }
            }
            _ => { },
        }
    }

    Ok(root_node.children)
}

/// Given a root node, traverses along the hierarchy, and returns a
/// mutable reference to the last child node of the root node
pub fn get_item<'a>(hierarchy: &[usize], root_node: &'a mut XmlNode) -> Option<&'a mut XmlNode> {
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

/// Compiles a XML `args="a: String, b: bool"` into a `["a" => "String", "b" => "bool"]` map
pub fn parse_component_arguments(input: &str) -> Result<ComponentArgumentsMap, ComponentParseError> {

    use self::ComponentParseError::*;

    let mut args = ComponentArgumentsMap::default();

    for (arg_idx, arg) in input.split(",").enumerate() {

        let mut colon_iterator = arg.split(":");

        let arg_name = colon_iterator.next().ok_or(MissingName(arg_idx))?;
        let arg_name = arg_name.trim();

        if arg_name.is_empty() {
            return Err(MissingName(arg_idx));
        }
        if arg_name.chars().any(char::is_whitespace) {
            return Err(WhiteSpaceInComponentName(arg_idx, arg_name.into()));
        }

        let arg_type = colon_iterator.next().ok_or(MissingType(arg_idx, arg_name.into()))?;
        let arg_type = arg_type.trim();

        if arg_type.is_empty() {
            return Err(MissingType(arg_idx, arg_name.into()));
        }

        if arg_type.chars().any(char::is_whitespace) {
            return Err(WhiteSpaceInComponentType(arg_idx, arg_name.into(), arg_type.into()));
        }

        let arg_name = normalize_casing(arg_name);
        let arg_type = arg_type.to_string();

        args.insert(arg_name, (arg_type, arg_idx));
    }

    Ok(args)
}

/// Filters the XML attributes of a component given XmlAttributeMap
pub fn validate_and_filter_component_args(xml_attributes: &XmlAttributeMap, valid_args: &ComponentArguments)
-> Result<FilteredComponentArguments, RenderDomError> {

    let mut map = FilteredComponentArguments {
        args: ComponentArgumentsMap::default(),
        accepts_text: valid_args.accepts_text,
    };

    for (xml_attribute_name, xml_attribute_value) in xml_attributes.iter() {

        if let Some((valid_arg_type, valid_arg_index)) = valid_args.args.get(xml_attribute_name) {
            map.args.insert(xml_attribute_name.clone(), (valid_arg_type.clone(), *valid_arg_index));
        } else if DEFAULT_ARGS.contains(&xml_attribute_name.as_str()) {
            // no error, but don't insert the attribute name
        } else {
            // key was not expected for this component
            let keys = valid_args.args.keys().cloned().collect();
            return Err(RenderDomError::UselessFunctionArgument(xml_attribute_name.clone(), xml_attribute_value.clone(), keys));
        }
    }

    Ok(map)
}

/// Normalizes input such as `abcDef`, `AbcDef`, `abc-def` to the normalized form of `abc_def`
pub fn normalize_casing(input: &str) -> String {

    let mut words: Vec<String> = Vec::new();
    let mut cur_str = Vec::new();

    for ch in input.chars() {
        if ch.is_uppercase() || ch == '_' || ch == '-' {
            if !cur_str.is_empty() {
                words.push(cur_str.iter().collect());
                cur_str.clear();
            }
            if ch.is_uppercase() {
                cur_str.extend(ch.to_lowercase());
            }
        } else {
            cur_str.extend(ch.to_lowercase());
        }
    }

    if !cur_str.is_empty() {
        words.push(cur_str.iter().collect());
        cur_str.clear();
    }

    words.join("_")
}

/// Find the one and only `<body>` node, return error if
/// there is no app node or there are multiple app nodes
pub fn get_body_node(root_nodes: &[XmlNode]) -> Result<XmlNode, DomXmlParseError> {

    let mut body_node_iterator = root_nodes.iter().filter(|node| {
        let node_type_normalized = normalize_casing(&node.node_type);
        &node_type_normalized == "body"
    }).cloned();

    let body_node = body_node_iterator.next().ok_or(DomXmlParseError::NoRootComponent)?;
    if body_node_iterator.next().is_some() {
        Err(DomXmlParseError::MultipleRootComponents)
    } else {
        Ok(body_node)
    }
}

static DEFAULT_STR: &str = "";

/// Find the <style> node and parse the contents of it as a CSS files
pub fn parse_style<'a>(root_nodes: &'a [XmlNode]) -> Result<Css, CssParseError<'a>> {
    match find_node_by_type(root_nodes, "style") {
        Some(s) => {
            let text = s.text.as_ref().map(|s| s.as_str()).unwrap_or(DEFAULT_STR);
            azul_css_parser::new_from_str(&text)
        },
        None => Ok(Css::empty())
    }
}

/// Filter all `<component />` nodes and insert them into the `components` node
pub fn get_xml_components(root_nodes: &[XmlNode], components: &mut XmlComponentMap) -> Result<(), ComponentParseError> {

    for node in root_nodes {
        match DynamicXmlComponent::new(node.clone()) {
            Ok(node) => { components.register_component(node.name.clone(), Box::new(node), false); },
            Err(ComponentParseError::NotAComponent) => { }, // not a <component /> node, ignore
            Err(e) => return Err(e), // Error during parsing the XML component, bail
        }
    }

    Ok(())
}

/// Searches in the the `root_nodes` for a `node_type`, convenience function in order to
/// for example find the first <blah /> node in all these nodes.
pub fn find_node_by_type<'a>(root_nodes: &'a [XmlNode], node_type: &str) -> Option<&'a XmlNode> {
    root_nodes.iter().find(|n| normalize_casing(&n.node_type).as_str() == node_type)
}

pub fn find_attribute<'a>(node: &'a XmlNode, attribute: &str) -> Option<&'a String> {
    node.attributes.iter().find(|n| normalize_casing(&n.0).as_str() == attribute).map(|s| s.1)
}

/// Parses an XML string and returns a `StyledDom` with the components instantiated in the `<app></app>`
pub fn str_to_dom(root_nodes: &[XmlNode], component_map: &mut XmlComponentMap) -> Result<StyledDom, DomXmlParseError> {
    let mut global_style = Css::empty();
    if let Some(head_node) = find_node_by_type(root_nodes, "head") {
        get_xml_components(&head_node.children, component_map)?;
        global_style = match parse_style(&head_node.children) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("error parsing global CSS: {:?}", e);
                Css::empty()
            }
        };
    }
    let body_node = get_body_node(&root_nodes)?;
    render_dom_from_body_node(&body_node, global_style, component_map).map_err(|e| e.into())
}

/// Parses an XML string and returns a `String`, which contains the Rust source code
/// (i.e. it compiles the XML to valid Rust)
pub fn str_to_rust_code(root_nodes: &[XmlNode], imports: &str, component_map: &mut XmlComponentMap) -> Result<String, CompileError> {

    const HEADER_WARNING: &str = "//! Auto-generated UI source code";

    let source_code = HEADER_WARNING.to_string();
/*
    if let Some(head_node) = root_nodes.iter().find(|n| normalize_casing(&n.node_type).as_str() == "head") {
        get_xml_components(&head_node.children, component_map).map_err(|e| format!("Error parsing component: {}", e))?;
    }

    let body_node = get_body_node(&root_nodes).map_err(|e| format!("Could not find <body /> node: {}", e))?;
    let app_source = compile_body_node_to_rust_code(&body_node, &component_map)?;
    let app_source = app_source.lines().map(|l| format!("        {}", l)).collect::<Vec<String>>().join("\r\n");

    let source_code = format!("{}\r\n\r\n{}\r\n\r\n{}\r\n\r\n{}", HEADER_WARNING, imports,
        compile_components(compile_components_to_rust_code(&component_map)?),
        format!("impl Layout for YourType {{\r\n    fn layout(&self, _info: LayoutInfo) -> Dom<YourType> {{\r\n{}\r\n    }}\r\n}}",
            app_source
        ),
    );
*/

    Ok(source_code)
}

// Compile all components to source code
pub fn compile_components(components: BTreeMap<ComponentName, (CompiledComponent, FilteredComponentArguments)>) -> String {
    components.iter().map(|(name, (function_body, function_args))| {
        compile_component(name, function_args, function_body)
    }).collect::<Vec<String>>().join("\r\n\r\n")
}

pub fn format_component_args(component_args: &ComponentArgumentsMap) -> String {

    let mut args = component_args.iter().map(|(arg_name, (arg_type, arg_index))| {
        (*arg_index, format!("{}: {}", arg_name, arg_type))
    }).collect::<Vec<(usize, String)>>();

    args.sort_by(|(_, a), (_, b)| b.cmp(&a));

    args.iter().map(|(k, v)| v.clone()).collect::<Vec<String>>().join(", ")
}

pub fn compile_component(
    component_name: &str,
    component_args: &ComponentArguments,
    component_function_body: &str,
) -> String {

    let function_args = format_component_args(&component_args.args);
    let component_function_body = component_function_body.lines().map(|l| format!("    {}", l)).collect::<Vec<String>>().join("\r\n");
    let should_inline = component_function_body.lines().count() == 1;
    format!(
        "{}fn render_component_{}{}({}{}{}) -> Dom {{\r\n{}\r\n}}",
        if should_inline { "#[inline]\r\n" } else { "" },
        normalize_casing(component_name),
        // pass the text content as the first
        if component_args.accepts_text { "<T: Layout, I: Into<String>>" } else { "<T: Layout>" },
        if component_args.accepts_text { "text: I" } else { "" },
        if function_args.is_empty() || !component_args.accepts_text { "" } else { ", " },
        function_args,
        component_function_body,
    )
}

pub fn render_dom_from_body_node(
    body_node: &XmlNode,
    global_css: Css,
    component_map: &XmlComponentMap
) -> Result<StyledDom, RenderDomError> {

    // Don't actually render the <body></body> node itself
    let mut dom = Dom::body().style(Css::empty());

    for child_node in &body_node.children {
        dom.append(render_dom_from_body_node_inner(child_node, component_map, &FilteredComponentArguments::default())?);
    }

    dom.restyle(global_css); // apply the CSS again

    Ok(dom)
}

/// Takes a single (expanded) app node and renders the DOM or returns an error
pub fn render_dom_from_body_node_inner(
    xml_node: &XmlNode,
    component_map: &XmlComponentMap,
    parent_xml_attributes: &FilteredComponentArguments,
) -> Result<StyledDom, RenderDomError> {

    let component_name = normalize_casing(&xml_node.node_type);

    let (renderer, inherit_variables) = component_map.components.get(&component_name)
        .ok_or(RenderDomError::UnknownComponent(component_name.clone()))?;

    // Arguments of the current node
    let available_function_args = renderer.get_available_arguments();
    let mut filtered_xml_attributes = validate_and_filter_component_args(&xml_node.attributes, &available_function_args)?;

    if *inherit_variables {
        // Append all variables that are in scope for the parent node
        filtered_xml_attributes.args.extend(parent_xml_attributes.args.clone().into_iter());
    }

    // Instantiate the parent arguments in the current child arguments
    for v in filtered_xml_attributes.args.values_mut() {
        v.0 = format_args_dynamic(&v.0, &parent_xml_attributes.args).to_string();
    }

    let text = xml_node.text.as_ref().map(|t| format_args_dynamic(t, &filtered_xml_attributes.args));

    let mut dom = renderer.render_dom(component_map, &filtered_xml_attributes, &text)?;
    set_attributes(&mut dom, &xml_node.attributes, &filtered_xml_attributes);

    for child_node in &xml_node.children {
        dom.append(render_dom_from_body_node_inner(child_node, component_map, &filtered_xml_attributes)?);
    }

    Ok(dom)
}

pub fn set_attributes(dom: &mut StyledDom, xml_attributes: &XmlAttributeMap, filtered_xml_attributes: &FilteredComponentArguments) {

    use azul_core::dom::TabIndex;
    use azul_core::dom::IdOrClass::{Id, Class};

    let mut ids_and_classes = Vec::new();
    let dom_root = match dom.root.into_crate_internal() {
        Some(s) => s,
        None => return,
    };
    let node_data = &mut dom.node_data.as_container_mut()[dom_root];

    if let Some(ids) = xml_attributes.get("id") {
        for id in ids.split_whitespace() {
            ids_and_classes.push(Id(format_args_dynamic(id, &filtered_xml_attributes.args).into()));
        }
    }

    if let Some(classes) = xml_attributes.get("class") {
        for class in classes.split_whitespace() {
            ids_and_classes.push(Class(format_args_dynamic(class, &filtered_xml_attributes.args).into()));
        }
    }

    node_data.set_ids_and_classes(ids_and_classes.into());

    if let Some(focusable) = xml_attributes.get("focusable")
        .map(|f| format_args_dynamic(f, &filtered_xml_attributes.args))
        .and_then(|f| parse_bool(&f))
    {
        match focusable {
            true => node_data.set_tab_index(Some(TabIndex::Auto).into()),
            false => node_data.set_tab_index(Some(TabIndex::NoKeyboardFocus.into()).into()),
        }
    }

    if let Some(tab_index) = xml_attributes.get("tabindex")
        .map(|val| format_args_dynamic(val, &filtered_xml_attributes.args))
        .and_then(|val| val.parse::<isize>().ok())
    {
        match tab_index {
            0 => node_data.set_tab_index(Some(TabIndex::Auto).into()),
            i if i > 0 => node_data.set_tab_index(Some(TabIndex::OverrideInParent(i as u32)).into()),
            _ => node_data.set_tab_index(Some(TabIndex::NoKeyboardFocus).into()),
        }
    }
}

pub fn set_stringified_attributes(
    dom_string: &mut String,
    xml_attributes: &XmlAttributeMap,
    filtered_xml_attributes: &ComponentArgumentsMap,
    tabs: usize,
) {

    let t = String::from("    ").repeat(tabs);

    if let Some(ids) = xml_attributes.get("id") {
        let ids = ids
            .split_whitespace()
            .map(|id| format!("{}.with_id(\"{}\")", t, format_args_dynamic(id, &filtered_xml_attributes)))
            .collect::<Vec<String>>()
            .join("\r\n");

        dom_string.push_str(&format!("\r\n{}", ids));
    }

    if let Some(classes) = xml_attributes.get("class") {
        let classes = classes
            .split_whitespace()
            .map(|class| format!("{}.with_class(\"{}\")", t, format_args_dynamic(class, &filtered_xml_attributes)))
            .collect::<Vec<String>>()
            .join("\r\n");

        dom_string.push_str(&format!("\r\n{}", classes));
    }

    if let Some(focusable) = xml_attributes.get("focusable")
        .map(|f| format_args_dynamic(f, &filtered_xml_attributes))
        .and_then(|f| parse_bool(&f))
    {
        match focusable {
            true => dom_string.push_str(&format!("\r\n{}.with_tab_index(Some(TabIndex::Auto).into())", t)),
            false => dom_string.push_str(&format!("\r\n{}.with_tab_index(Some(TabIndex::NoKeyboardFocus).into())", t)),
        }
    }

    if let Some(tab_index) = xml_attributes.get("tabindex")
        .map(|val| format_args_dynamic(val, &filtered_xml_attributes))
        .and_then(|val| val.parse::<isize>().ok())
    {
        match tab_index {
            0 => dom_string.push_str(&format!("\r\n{}.with_tab_index(Some(TabIndex::Auto).into())", t)),
            i if i > 0 => dom_string.push_str(&format!("\r\n{}.with_tab_index(Some(TabIndex::OverrideInParent({})).into())", t, i as usize)),
            _ => dom_string.push_str(&format!("\r\n{}.with_tab_index(Some(TabIndex::NoKeyboardFocus).into())", t)),
        }
    }
}

/// Item of a split string - either a variable name or a string
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum DynamicItem {
    Var(String),
    Str(String),
}

/// Splits a string into formatting arguments
/// ```rust
/// # use azulc::xml::DynamicItem::*;
/// # use azulc::xml::split_dynamic_string;
/// let s = "hello {a}, {b}{{ {c} }}";
/// let split = split_dynamic_string(s);
/// let output = vec![
///     Str("hello ".to_string()),
///     Var("a".to_string()),
///     Str(", ".to_string()),
///     Var("b".to_string()),
///     Str("{ ".to_string()),
///     Var("c".to_string()),
///     Str(" }".to_string()),
/// ];
/// assert_eq!(output, split);
/// ```
pub fn split_dynamic_string(input: &str) -> Vec<DynamicItem> {

    use self::DynamicItem::*;

    let input: Vec<char> = input.chars().collect();
    let input_chars_len = input.len();

    let mut items = Vec::new();
    let mut current_idx = 0;
    let mut last_idx = 0;

    while current_idx < input_chars_len {
        let c = input[current_idx];
        match c {
            '{' if input.get(current_idx + 1).copied() != Some('{') => {

                // variable start, search until next closing brace or whitespace or end of string
                let mut start_offset = 1;
                let mut has_found_variable = false;
                while let Some(c) = input.get(current_idx + start_offset) {
                    if c.is_whitespace() { break; }
                    if *c == '}' && input.get(current_idx + start_offset + 1).copied() != Some('}') {
                        start_offset += 1;
                        has_found_variable = true;
                        break;
                    }
                    start_offset += 1;
                }

                // advance current_idx accordingly
                // on fail, set cursor to end
                // set last_idx accordingly
                if has_found_variable {

                    if last_idx != current_idx {
                        items.push(Str(input[last_idx..current_idx].iter().collect()));
                    }

                    // subtract 1 from start for opening brace, one from end for closing brace
                    items.push(Var(input[(current_idx + 1)..(current_idx + start_offset - 1)].iter().collect()));
                    current_idx = current_idx + start_offset;
                    last_idx = current_idx;
                } else {
                    current_idx += start_offset;
                }
            },
            _ => { current_idx += 1; },
        }
    }

    if current_idx != last_idx {
        items.push(Str(input[last_idx..].iter().collect()));
    }

    for item in &mut items {
        // replace {{ with { in strings
        if let Str(s) = item {
            *s = s.replace("{{", "{").replace("}}", "}");
        }
    }

    items
}

/// Combines the split string back into its original form while replacing the variables with their values
///
/// let variables = btreemap!{ "a" => "value1", "b" => "value2" };
/// [Str("hello "), Var("a"), Str(", "), Var("b"), Str("{ "), Var("c"), Str(" }}")]
/// => "hello value1, valuec{ {c} }"
pub fn combine_and_replace_dynamic_items(input: &[DynamicItem], variables: &ComponentArgumentsMap) -> String {
    let mut s = String::new();

    for item in input {
        match item {
            DynamicItem::Var(v) => {
                let variable_name = normalize_casing(v.trim());
                match variables.get(&variable_name) {
                    Some((resolved_var, _)) => { s.push_str(&resolved_var); },
                    None => {
                        s.push('{');
                        s.push_str(v);
                        s.push('}');
                    },
                }
            },
            DynamicItem::Str(dynamic_str) => {
                s.push_str(&dynamic_str);
            }
        }
    }

    s
}

/// Given a string and a key => value mapping, replaces parts of the string with the value, i.e.:
///
/// ```rust
/// # use std::collections::BTreeMap;
/// # use azulc::xml::format_args_dynamic;
/// let mut variables = BTreeMap::new();
/// variables.insert(String::from("a"), (String::from("value1"), 0));
/// variables.insert(String::from("b"), (String::from("value2"), 1));
///
/// let initial = "hello {a}, {b}{{ {c} }}";
/// let expected = "hello value1, value2{ {c} }".to_string();
/// assert_eq!(format_args_dynamic(initial, &variables), expected);
/// ```
///
/// Note: the number (0, 1, etc.) is the order of the argument, it is irrelevant for
/// runtime formatting, only important for keeping the component / function arguments
/// in order when compiling the arguments to Rust code
pub fn format_args_dynamic(input: &str, variables: &ComponentArgumentsMap) -> String {
    let dynamic_str_items = split_dynamic_string(input);
    combine_and_replace_dynamic_items(&dynamic_str_items, variables)
}

// NOTE: Two sequential returns count as a single return, while single returns get ignored.
pub fn prepare_string(input: &str) -> String {

    const SPACE: &str = " ";
    const RETURN: &str = "\n";

    let input = input.trim();

    if input.is_empty() {
        return String::new();
    }

    let input = input.replace("&lt;", "<");
    let input = input.replace("&gt;", ">");

    let input_len = input.len();
    let mut final_lines: Vec<String> = Vec::new();
    let mut last_line_was_empty = false;

    for line in input.lines() {

        let line = line.trim();
        let line = line.replace("&nbsp;", " ");
        let current_line_is_empty = line.is_empty();

        if !current_line_is_empty {
            if last_line_was_empty {
                final_lines.push(format!("{}{}", RETURN, line));
            } else {
                final_lines.push(line.to_string());
            }
        }

        last_line_was_empty = current_line_is_empty;
    }

    let line_len = final_lines.len();
    let mut target = String::with_capacity(input_len);
    for (line_idx, line) in final_lines.iter().enumerate() {
        if !(line.starts_with(RETURN) || line_idx == 0 || line_idx == line_len.saturating_sub(1)) {
            target.push_str(SPACE);
        }
        target.push_str(line);
    }
    target
}

/// Parses a string ("true" or "false")
pub fn parse_bool(input: &str) -> Option<bool> {
    match input {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

pub fn render_component_inner(
    map: &mut BTreeMap<ComponentName, (CompiledComponent, FilteredComponentArguments)>,
    component_name: String,
    (renderer, inherit_variables): &(Box<dyn XmlComponent>, bool),
    component_map: &XmlComponentMap,
    parent_xml_attributes: &FilteredComponentArguments,
    tabs: usize,
) -> Result<(), CompileError> {

    let t = String::from("    ").repeat(tabs);
    let t1 = String::from("    ").repeat(tabs + 1);

    let component_name = normalize_casing(&component_name);
    let xml_node = renderer.get_xml_node();

    // Arguments of the current node
    let available_function_args = renderer.get_available_arguments();
    let mut filtered_xml_attributes = available_function_args.clone(); // <- important, only for Rust code compilation

    if *inherit_variables {
        // Append all variables that are in scope for the parent node
        filtered_xml_attributes.args.extend(parent_xml_attributes.args.clone().into_iter());
    }

    // Instantiate the parent arguments in the current child arguments
    for v in filtered_xml_attributes.args.values_mut() {
        v.0 = format_args_dynamic(&v.0, &parent_xml_attributes.args).to_string();
    }

    let text = xml_node.text.as_ref().map(|t| format_args_dynamic(t, &filtered_xml_attributes.args));

    let mut dom_string = renderer.compile_to_rust_code(component_map, &filtered_xml_attributes, &text)?;
    set_stringified_attributes(&mut dom_string, &xml_node.attributes, &filtered_xml_attributes.args, tabs + 1);

    for child_node in &xml_node.children {
        dom_string.push_str(&format!("\r\n{}.with_child(\r\n{}{}\r\n{})",
            t, t1, compile_node_to_rust_code_inner(child_node, component_map, &filtered_xml_attributes, tabs + 1)?, t,
        ));
    }

    map.insert(component_name, (dom_string, filtered_xml_attributes));

    Ok(())
}

/// Takes all components and generates the source code function from them
pub fn compile_components_to_rust_code(
    components: &XmlComponentMap
) -> Result<BTreeMap<ComponentName, (CompiledComponent, FilteredComponentArguments)>, CompileError> {

    let mut map = BTreeMap::new();

    for (xml_node_name, xml_component) in &components.components {
        render_component_inner(&mut map, xml_node_name.clone(), xml_component, &components, &FilteredComponentArguments::default(), 1)?;
    }

    Ok(map)
}

pub fn compile_body_node_to_rust_code(body_node: &XmlNode, component_map: &XmlComponentMap) -> Result<String, CompileError> {
    let t = "    ";
    let t2 = "        ";
    let mut dom_string = String::from("Dom::body()");
    for child_node in &body_node.children {
        dom_string.push_str(&format!("\r\n{}.with_child(\r\n{}{}\r\n{})",
            t, t2, compile_node_to_rust_code_inner(child_node, component_map, &FilteredComponentArguments::default(), 2)?, t
        ));
    }
    let dom_string = dom_string.trim();
    Ok(dom_string.to_string())
}

fn compile_and_format_dynamic_items(input: &[DynamicItem]) -> String {
    use self::DynamicItem::*;
    if input.is_empty() {
        String::from("\"\"")
    } else if input.len() == 1 {
        // common: there is only one "dynamic item" - skip the "format!()" macro
        match &input[0] {
            Var(v) => normalize_casing(v.trim()),
            Str(s) => format!("{:?}", s),
        }
    } else {
        // build a "format!("{var}, blah", var)" string
        let mut formatted_str = String::from("format!(\"");
        let mut variables = Vec::new();
        for item in input {
            match item {
                Var(v) => {
                    let variable_name = normalize_casing(v.trim());
                    formatted_str.push_str(&format!("{{{}}}", variable_name));
                    variables.push(variable_name.clone());
                },
                Str(s) => {
                    let s = s.replace("\"", "\\\"");
                    formatted_str.push_str(&s);
                },
            }
        }

        formatted_str.push('\"');
        if !variables.is_empty() {
            formatted_str.push_str(", ");
        }

        formatted_str.push_str(&variables.join(", "));
        formatted_str.push(')');
        formatted_str
    }
}

fn format_args_for_rust_code(input: &str) -> String {
    let dynamic_str_items = split_dynamic_string(input);
    compile_and_format_dynamic_items(&dynamic_str_items)
}

pub fn compile_node_to_rust_code_inner(
    node: &XmlNode,
    component_map: &XmlComponentMap,
    parent_xml_attributes: &FilteredComponentArguments,
    tabs: usize,
) -> Result<String, CompileError> {

    let t = String::from("    ").repeat(tabs);
    let t2 = String::from("    ").repeat(tabs + 1);

    let component_name = normalize_casing(&node.node_type);

    let (renderer, inherit_variables) = component_map.components.get(&component_name)
        .ok_or(RenderDomError::UnknownComponent(component_name.clone()))?;

    // Arguments of the current node
    let available_function_args = renderer.get_available_arguments();
    let mut filtered_xml_attributes = validate_and_filter_component_args(&node.attributes, &available_function_args)?;

    if *inherit_variables {
        // Append all variables that are in scope for the parent node
        filtered_xml_attributes.args.extend(parent_xml_attributes.args.clone().into_iter());
    }

    // Instantiate the parent arguments in the current child arguments
    for v in filtered_xml_attributes.args.values_mut() {
        v.0 = format_args_dynamic(&v.0, &parent_xml_attributes.args).to_string();
    }

    let instantiated_function_arguments = {

        let mut args = filtered_xml_attributes.args.iter()
        .filter_map(|(xml_attribute_key, (_xml_attribute_type, xml_attribute_order))| {
            match node.attributes.get(xml_attribute_key).cloned() {
                Some(s) => Some((*xml_attribute_order, format_args_for_rust_code(&s))),
                None => {
                    // __TODO__
                    // let node_text = format_args_for_rust_code(&xml_attribute_key);
                    //   "{text}" => "text"
                    //   "{href}" => "href"
                    //   "{blah}_the_thing" => "format!(\"{blah}_the_thing\", blah)"
                    None
                }
            }
        })
        .collect::<Vec<(usize, String)>>();

        args.sort_by(|(_, a), (_, b)| a.cmp(&b));

        args.into_iter().map(|(k, v)| v.clone()).collect::<Vec<String>>().join(", ")
    };

    let text_as_first_arg =
        if filtered_xml_attributes.accepts_text {
            let node_text = node.text.clone().unwrap_or_default();
            let node_text = format_args_for_rust_code(node_text.trim());
            let trailing_comma = if !instantiated_function_arguments.is_empty() { ", " } else { "" };

            // __TODO__
            // let node_text = format_args_for_rust_code(&node_text, &parent_xml_attributes.args);
            //   "{text}" => "text"
            //   "{href}" => "href"
            //   "{blah}_the_thing" => "format!(\"{blah}_the_thing\", blah)"

            format!("{}{}", node_text, trailing_comma)
        } else {
            String::new()
        };

    // The dom string is the function name
    let mut dom_string = format!("render_component_{}({}{})", component_name, text_as_first_arg, instantiated_function_arguments);
    set_stringified_attributes(&mut dom_string, &node.attributes, &filtered_xml_attributes.args, tabs + 1);

    for child_node in &node.children {
        dom_string.push_str(
            &format!("\r\n{}.with_child(\r\n{}{}\r\n{})",
                t, t2, compile_node_to_rust_code_inner(child_node, component_map, &filtered_xml_attributes, tabs + 1)?, t
            )
        );
    }

    Ok(dom_string)
}

// --- Renderers for various built-in types

/// Render for a `div` component
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DivRenderer { }

impl XmlComponent for DivRenderer {

    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments::new()
    }

    fn render_dom(&self, _: &XmlComponentMap, _: &FilteredComponentArguments, _: &XmlTextContent) -> Result<StyledDom, RenderDomError> {
        Ok(Dom::div().style(Css::empty()))
    }

    fn compile_to_rust_code(&self, _: &XmlComponentMap, _: &FilteredComponentArguments, _: &XmlTextContent) -> Result<String, CompileError> {
        Ok("Dom::div()".into())
    }

    fn get_xml_node(&self) -> XmlNode {
        XmlNode::new("div")
    }
}

/// Render for a `body` component
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BodyRenderer { }

impl XmlComponent for BodyRenderer {

    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments::new()
    }

    fn render_dom(&self, _: &XmlComponentMap, _: &FilteredComponentArguments, _: &XmlTextContent) -> Result<StyledDom, RenderDomError> {
        Ok(Dom::body().style(Css::empty()))
    }

    fn compile_to_rust_code(&self, _: &XmlComponentMap, _: &FilteredComponentArguments, _: &XmlTextContent) -> Result<String, CompileError> {
        Ok("Dom::body()".into())
    }

    fn get_xml_node(&self) -> XmlNode {
        XmlNode::new("body")
    }
}

/// Render for a `p` component
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextRenderer { }

impl XmlComponent for TextRenderer {

    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments {
            args: ComponentArgumentsMap::default(),
            accepts_text: true, // important!
        }
    }

    fn render_dom(&self, _: &XmlComponentMap, _: &FilteredComponentArguments, content: &XmlTextContent) -> Result<StyledDom, RenderDomError> {
        let content = content.as_ref().map(|s| prepare_string(&s)).unwrap_or_default();
        Ok(Dom::label(content).style(Css::empty()))
    }

    fn compile_to_rust_code(&self, _: &XmlComponentMap, args: &FilteredComponentArguments, content: &XmlTextContent) -> Result<String, CompileError> {
        Ok(String::from("Dom::label(text)"))
    }

    fn get_xml_node(&self) -> XmlNode {
        XmlNode::new("p")
    }
}

// -- Tests

#[test]
fn test_compile_dom_1() {

    use crate::Dummy;

    // Test the output of a certain component
    fn test_component_source_code(input: &str, component_name: &str, expected: &str) {
        let mut component_map = XmlComponentMap::<Dummy>::default();
        let root_nodes = parse_xml_string(input).unwrap();
        get_xml_components(&root_nodes, &mut component_map).unwrap();
        let body_node = get_body_node(&root_nodes).unwrap();
        let components = compile_components_to_rust_code(&component_map).unwrap();
        let (searched_component_source, searched_component_args) = components.get(component_name).unwrap();
        let component_string = compile_component(component_name, searched_component_args, searched_component_source);

        // TODO!
        // assert_eq!(component_string, expected);
    }

    fn test_app_source_code(input: &str, expected: &str) {
        let mut component_map = XmlComponentMap::<Dummy>::default();
        let root_nodes = parse_xml_string(input).unwrap();
        get_xml_components(&root_nodes, &mut component_map).unwrap();
        let body_node = get_body_node(&root_nodes).unwrap();
        let app_source = compile_body_node_to_rust_code(&body_node, &component_map).unwrap();

        // TODO!
        // assert_eq!(app_source, expected);
    }

    let s1 = r#"
        <component name="test">
            <div id="a" class="b"></div>
        </component>

        <body>
            <Test />
        </body>
    "#;
    let s1_expected = r#"
        fn render_component_test<T>() -> Dom {
            Dom::div().with_id("a").with_class("b")
        }
    "#;

    test_component_source_code(&s1, "test", &s1_expected);
}

#[test]
fn test_format_args_dynamic() {
    let mut variables = FilteredComponentArguments::new();
    variables.insert("a".to_string(), "value1".to_string());
    variables.insert("b".to_string(), "value2".to_string());
    assert_eq!(
        format_args_dynamic("hello {a}, {b}{{ {c} }}", &variables),
        String::from("hello value1, value2{ {c} }"),
    );
    assert_eq!(
        format_args_dynamic("hello {{a}, {b}{{ {c} }}", &variables),
        String::from("hello {a}, value2{ {c} }"),
    );
    assert_eq!(
        format_args_dynamic("hello {{{{{{{ a   }}, {b}{{ {c} }}", &variables),
        String::from("hello {{{{{{ a   }, value2{ {c} }"),
    );
}

#[test]
fn test_normalize_casing() {
    assert_eq!(normalize_casing("abcDef"), String::from("abc_def"));
    assert_eq!(normalize_casing("abc_Def"), String::from("abc_def"));
    assert_eq!(normalize_casing("abc-Def"), String::from("abc_def"));
    assert_eq!(normalize_casing("abc-def"), String::from("abc_def"));
    assert_eq!(normalize_casing("AbcDef"), String::from("abc_def"));
    assert_eq!(normalize_casing("Abc-Def"), String::from("abc_def"));
    assert_eq!(normalize_casing("Abc_Def"), String::from("abc_def"));
    assert_eq!(normalize_casing("aBc_Def"), String::from("a_bc_def")); // wrong, but whatever
    assert_eq!(normalize_casing("StartScreen"), String::from("start_screen"));
}

#[test]
fn test_parse_component_arguments() {

    let mut args_1_expected = ComponentArguments::new();
    args_1_expected.insert("selected_date".to_string(), "DateTime".to_string());
    args_1_expected.insert("minimum_date".to_string(), "DateTime".to_string());
    args_1_expected.insert("grid_visible".to_string(), "bool".to_string());

    // Everything OK
    assert_eq!(
        parse_component_arguments("gridVisible: bool, selectedDate: DateTime, minimumDate: DateTime"),
        Ok(args_1_expected)
    );

    // Missing type for selectedDate
    assert_eq!(
        parse_component_arguments("gridVisible: bool, selectedDate: , minimumDate: DateTime"),
        Err(ComponentParseError::MissingType(1, "selectedDate".to_string()))
    );

    // Missing name for first argument
    assert_eq!(
        parse_component_arguments(": bool, selectedDate: DateTime, minimumDate: DateTime"),
        Err(ComponentParseError::MissingName(0))
    );

    // Missing comma after DateTime
    assert_eq!(
        parse_component_arguments("gridVisible: bool, selectedDate: DateTime  minimumDate: DateTime"),
        Err(ComponentParseError::WhiteSpaceInComponentType(1, "selectedDate".to_string(), "DateTime  minimumDate".to_string()))
    );

    // Missing colon after gridVisible
    assert_eq!(
        parse_component_arguments("gridVisible: bool, selectedDate DateTime, minimumDate: DateTime"),
        Err(ComponentParseError::WhiteSpaceInComponentName(1, "selectedDate DateTime".to_string()))
    );
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

    assert_eq!(&get_item(&[], &mut tree).unwrap().node_type, "component");
    assert_eq!(&get_item(&[0], &mut tree).unwrap().node_type, "a");
    assert_eq!(&get_item(&[0, 0], &mut tree).unwrap().node_type, "b");
    assert_eq!(&get_item(&[0, 1], &mut tree).unwrap().node_type, "c");
    assert_eq!(&get_item(&[0, 2], &mut tree).unwrap().node_type, "d");
    assert_eq!(&get_item(&[0, 3], &mut tree).unwrap().node_type, "e");
    assert_eq!(&get_item(&[1], &mut tree).unwrap().node_type, "f");
    assert_eq!(&get_item(&[1, 0], &mut tree).unwrap().node_type, "g");
    assert_eq!(&get_item(&[1, 0, 0], &mut tree).unwrap().node_type, "h");
    assert_eq!(&get_item(&[1, 1], &mut tree).unwrap().node_type, "i");
    assert_eq!(&get_item(&[2], &mut tree).unwrap().node_type, "j");

    assert_eq!(get_item(&[123213], &mut tree), None);
    assert_eq!(get_item(&[0, 1, 2], &mut tree), None);
}

#[test]
fn test_prepare_string_1() {
    let input1 = r#"Test"#;
    let output = prepare_string(input1);
    assert_eq!(output, String::from("Test"));
}

#[test]
fn test_prepare_string_2() {
    let input1 = r#"
    Hello,
    123


    Test Test2

    Test3




    Test4
    "#;

    let output = prepare_string(input1);
    assert_eq!(output, String::from("Hello, 123\nTest Test2\nTest3\nTest4"));
}
