#![allow(unused_variables)]

use std::{fmt, collections::BTreeMap, path::Path};
use {
    callbacks::Callback,
    dom::Dom,
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
/// fn render_component_test<'a, T>(args: &TestRendererArgs<'a>) -> Dom<T> {
///     Button::with_label(format!("Is this true? Scientists say: {:?}", args.b)).with_class(format!("test_{}", args.a))
/// }
/// ```
///
/// For this to work, a component has to note all its arguments and types that it can take.
/// If a type is not `str` or `String`, it will be formatted using the `{:?}` formatter
/// in the generated source code, otherwise the compiler will use the `{}` formatter.
pub type ComponentArguments = BTreeMap<ComponentArgumentName, ComponentArgumentType>;

type ComponentName = String;
type CompiledComponent = String;

/// Specifies a component that reacts to a parsed XML node
pub trait XmlComponent<T> {

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
    fn render_dom(&self, components: &XmlComponentMap<T>, arguments: &FilteredComponentArguments, content: &XmlTextContent) -> Result<Dom<T>, RenderDomError>;
    /// Used to compile the XML component to Rust code - input
    fn compile_to_rust_code(&self, components: &XmlComponentMap<T>, attributes: &FilteredComponentArguments, content: &XmlTextContent) -> Result<String, CompileError>;
}

pub struct DomXml<T> {
    pub original_string: String,
    pub parsed_dom: Dom<T>,
}

impl<T> DomXml<T> {

    /// Parses and loads a DOM from an XML string
    ///
    /// Note: Needs at least one `<app></app>` node in order to not fail
    #[inline]
    pub fn new(xml: &str, component_map: &mut XmlComponentMap<T>) -> Result<Self, XmlParseError> {
        let dom = str_to_dom(xml, component_map)?;
        Ok(Self {
            original_string: xml.to_string(),
            parsed_dom: dom,
        })
    }

    /// Creates a mock `<app></app>` wrapper, so that the `Self::new()` function doesn't fail
    #[cfg(test)]
    pub fn mock(xml: &str) -> Self {
        let actual_xml = format!("<app>{}</app>", xml);
        Self::new(&actual_xml, &mut XmlComponentMap::default()).unwrap()
    }

    /// Loads, parses and builds a DOM from an XML file
    ///
    /// **Warning**: The file is reloaded from disk on every function call - do not
    /// use this in release builds! This function deliberately never fails: In an error case,
    /// the error gets rendered as a `NodeType::Label`.
    pub fn from_file<I: AsRef<Path>>(file_path: I, component_map: &mut XmlComponentMap<T>) -> Self {

        use std::fs;

        let xml = match fs::read_to_string(file_path) {
            Ok(xml) => xml,
            Err(e) => return Self {
                original_string: format!("{}", e),
                parsed_dom: Dom::label(format!("{}", e)),
            },
        };

        match Self::new(&xml, component_map) {
            Ok(o) => o,
            Err(e) =>  Self {
                original_string: format!("{}", e),
                parsed_dom: Dom::label(format!("{}", e)),
            },
        }
    }

    /// Convenience function, only available in tests, useful for quickly writing UI tests.
    /// Wraps the XML string in the required `<app></app>` braces, panics if the XML couldn't be parsed.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use azul::dom::Dom;
    /// # use azul::xml::DomXml;
    /// let dom = DomXml::mock("<div id='test' />");
    /// dom.assert_eq(Dom::div().with_id("test"));
    /// ```
    #[cfg(test)]
    pub fn assert_eq(self, other: Dom<T>) {
        let fixed = Dom::div().with_child(other);
        let expected = self.into_dom();
        if expected != fixed {
            panic!("\r\nExpected DOM did not match:\r\n\r\nexpected: ----------\r\n{}\r\ngot: ----------\r\n{}\r\n",
                expected.debug_dump(), fixed.debug_dump()
            );
        }
    }

    pub fn into_dom(self) -> Dom<T> {
        self.into()
    }
}

impl<T> Into<Dom<T>> for DomXml<T> {
    fn into(self) -> Dom<T> {
        self.parsed_dom
    }
}

/// Component that was created from a XML node (instead of being registered from Rust code).
/// Necessary to
struct DynamicXmlComponent {
    /// What the name of this component is, i.e. "test" for `<component name="test" />`
    name: String,
    /// Whether this component has any `args="a: String"` arguments
    arguments: Option<ComponentArguments>,
    /// Root XML node of this component (the `<component />` Node)
    root: XmlNode,
}

impl DynamicXmlComponent {
    /// Parses a `component` from an XML node
    pub fn new(root: XmlNode) -> Result<Self, ComponentParseError> {
        let name = root.attributes.get("name").cloned().ok_or(ComponentParseError::NotAComponent)?;
        let arguments = match root.attributes.get("args") {
            Some(s) => Some(parse_component_arguments(s)?),
            None => None,
        };

        Ok(Self {
            name: normalize_casing(&name),
            arguments,
            root,
        })
    }
}

impl<T> XmlComponent<T> for DynamicXmlComponent {

    fn get_available_arguments(&self) -> ComponentArguments {
        self.arguments.clone().unwrap_or_default()
    }

    fn render_dom(
        &self,
        components: &XmlComponentMap<T>,
        arguments: &FilteredComponentArguments,
        content: &XmlTextContent,
    ) -> Result<Dom<T>, RenderDomError> {

        let mut dom = Dom::div();
        for child_node in &self.root.children {
            dom.add_child(render_dom_from_app_node_inner(child_node, components, arguments)?);
        }

        Ok(dom)
    }

    fn compile_to_rust_code(
        &self,
        components: &XmlComponentMap<T>,
        attributes: &FilteredComponentArguments,
        content: &XmlTextContent,
    ) -> Result<String, CompileError> {
        Ok("Dom::div()".into())
    }
}

/// Represents one XML node tag
#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
pub struct XmlComponentMap<T> {
    /// Stores all known components that can be used during DOM rendering
    /// + whether this component should inherit variables from the parent scope
    components: BTreeMap<String, (Box<dyn XmlComponent<T>>, bool)>,
    /// Stores "onclick='do_this'" mappings from the string `do_this` to the actual function pointer
    callbacks: BTreeMap<String, Callback<T>>,
}

impl<T> Default for XmlComponentMap<T> {
    fn default() -> Self {
        let mut map = Self { components: BTreeMap::new(), callbacks: BTreeMap::new() };
        map.register_component("div", Box::new(DivRenderer { }), true);
        map.register_component("p", Box::new(TextRenderer { }), true);
        map
    }
}

impl<T> XmlComponentMap<T> {
    pub fn register_component<S: AsRef<str>>(&mut self, id: S, component: Box<dyn XmlComponent<T>>, inherit_variables: bool) {
        self.components.insert(normalize_casing(id.as_ref()), (component, inherit_variables));
    }
    pub fn register_callback<S: AsRef<str>>(&mut self, id: S, callback: Callback<T>) {
        self.callbacks.insert(normalize_casing(id.as_ref()), callback);
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
    /// A component raised an error while rendering the DOM - holds the component name + error string
    RenderDom(RenderDomError),
    /// Something went wrong while parsing an XML component
    Component(ComponentParseError),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RenderDomError {
    /// While instantiating a component, a function argument was encountered that the component won't use or react to.
    UselessFunctionArgument(String, String, Vec<String>),
    /// A certain node type can't be rendered, because the renderer isn't available
    UnknownComponent(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
}

impl_from!{ ComponentParseError, XmlParseError::Component }
impl_from!{ RenderDomError, XmlParseError::RenderDom }

impl fmt::Display for XmlParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::XmlParseError::*;
        match self {
            NoRootComponent => write!(f, "No <app></app> component present - empty DOM"),
            MultipleRootComponents => write!(f, "Multiple <app/> components present, only one root node is allowed"),
            ParseError(e) => write!(f, "XML parsing error: {}", e),
            MalformedHierarchy(got, expected) => write!(f, "Invalid </{}> tag: expected </{}>", got, expected),
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
/// # use azul::xml::{XmlNode, parse_xml_string};
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
                        node_type: normalize_casing(open_value.to_str()),
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
                let close_value = normalize_casing(close_value.to_str());
                if let Some(last) = get_item(&current_hierarchy, &mut root_node) {
                    if last.node_type != close_value {
                        return Err(MalformedHierarchy(close_value, last.node_type.clone()));
                    }
                }
                current_hierarchy.pop();
            },
            Attribute((_, key), value) => {
                if let Some(last) = get_item(&current_hierarchy, &mut root_node) {
                    // NOTE: Only lowercase the key, not the value!
                    last.attributes.insert(normalize_casing(key.to_str()), value.to_str().to_string());
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

/// Compiles a XML `args="a: String, b: bool"` into a `["a" => "String", "b" => "bool"]` map
fn parse_component_arguments(input: &str) -> Result<ComponentArguments, ComponentParseError> {

    use self::ComponentParseError::*;

    let mut args = ComponentArguments::default();

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

        args.insert(normalize_casing(arg_name), arg_type.to_string());
    }

    Ok(args)
}

pub type FilteredComponentArguments = ComponentArguments;

/// Filters the XML attributes of a component given XmlAttributeMap
fn validate_and_filter_component_args(xml_attributes: &XmlAttributeMap, valid_args: &FilteredComponentArguments)
-> Result<FilteredComponentArguments, RenderDomError> {

    const DEFAULT_ARGS: [&str;5] = ["id", "class", "tabindex", "draggable", "focusable"];

    let mut map = FilteredComponentArguments::default();

    for (xml_attribute_name, xml_attribute_value) in xml_attributes.iter() {

        let arg_value = match valid_args.get(xml_attribute_name) {
            Some(s) => Some(s),
            None => {
                if DEFAULT_ARGS.contains(&xml_attribute_name.as_str()) {
                    None // no error, but don't insert the attribute name
                } else {
                    let keys = valid_args.keys().cloned().collect();
                    return Err(RenderDomError::UselessFunctionArgument(xml_attribute_name.clone(), xml_attribute_value.clone(), keys));
                }
            }
        };

        if let Some(value) = arg_value {
            map.insert(xml_attribute_name.clone(), value.clone());
        }
    }

    Ok(map)
}

/// Normalizes input such as `abcDef`, `AbcDef`, `abc-def` to the normalized form of `abc_def`
fn normalize_casing(input: &str) -> String {

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

/// Find the one and only <app /> node, return error if
/// there is no app node or there are multiple app nodes
fn get_app_node(root_nodes: &[XmlNode]) -> Result<XmlNode, XmlParseError> {

    let mut app_node_iterator = root_nodes.iter().filter(|node| {
        let node_type_normalized = normalize_casing(&node.node_type);
        &node_type_normalized == "app"
    }).cloned();

    let app_node = app_node_iterator.next().ok_or(XmlParseError::NoRootComponent)?;
    if app_node_iterator.next().is_some() {
        Err(XmlParseError::MultipleRootComponents)
    } else {
        Ok(app_node)
    }
}

/// Filter all `<component />` nodes and insert them into the `components` node
fn get_xml_components<T>(root_nodes: &[XmlNode], components: &mut XmlComponentMap<T>) -> Result<(), ComponentParseError> {

    for node in root_nodes {
        match DynamicXmlComponent::new(node.clone()) {
            Ok(node) => { components.register_component(node.name.clone(), Box::new(node), false); },
            Err(ComponentParseError::NotAComponent) => { }, // not a <component /> node, ignore
            Err(e) => return Err(e), // Error during parsing the XML component, bail
        }
    }

    Ok(())
}

/// Parses an XML string and returns a `Dom` with the components instantiated in the `<app></app>`
pub fn str_to_dom<T>(xml: &str, component_map: &mut XmlComponentMap<T>) -> Result<Dom<T>, XmlParseError> {
    let root_nodes = parse_xml_string(xml)?;
    get_xml_components(&root_nodes, component_map)?;
    let app_node = get_app_node(&root_nodes)?;
    render_dom_from_app_node(&app_node, component_map).map_err(|e| e.into())
}

/// Parses an XML string and returns a `String`, which contains the Rust source code
/// (i.e. it compiles the XML to valid Rust)
pub fn str_to_rust_code<T>(
    xml: &str,
    imports: &str,
    component_map: &mut XmlComponentMap<T>,
) -> Result<String, CompileError> {

    const HEADER_WARNING: &str = "/// Auto-generated UI source code";

    let root_nodes = parse_xml_string(xml).map_err(|e| format!("XML parse error: {}", e))?;
    get_xml_components(&root_nodes, component_map).map_err(|e| format!("Error parsing component: {}", e))?;
    let app_node = get_app_node(&root_nodes).map_err(|e| format!("Could not find <app /> node: {}", e))?;
    let components_source = compile_components_to_rust_code(&component_map)?;
    let app_source = compile_app_node_to_rust_code(&app_node, &component_map)?;

    Ok(
        format!("{}\r\n{}\r\n{}\r\n{}",
            HEADER_WARNING,
            imports,
            compile_components(components_source),
            app_source,
        )
    )
}

fn format_component_args(component_args: &FilteredComponentArguments) -> String {
    let mut args = Vec::new();
    for (arg_name, arg_type) in component_args {
        args.push(format!("{}: {}", arg_name, arg_type));
    }
    args.join(" ")
}

fn compile_components(components: BTreeMap<ComponentName, (CompiledComponent, FilteredComponentArguments)>) -> String {
    components.iter().map(|(name, (function_body, function_args))| {
        compile_component(name, function_args, function_body)
    }).collect::<Vec<String>>().join("\r\n")
}

fn compile_component(component_name: &str, component_args: &FilteredComponentArguments, component_function_body: &str) -> String {
    format!(
        "fn render_component_{}({}) {{\r\n{}\r\n}}",
        normalize_casing(component_name),
        format_component_args(component_args),
        component_function_body,
    )
}

fn render_dom_from_app_node<T>(
    app_node: &XmlNode,
    component_map: &XmlComponentMap<T>
) -> Result<Dom<T>, RenderDomError> {

    // Don't actually render the <app></app> node itself
    let mut dom = Dom::div();
    for child_node in &app_node.children {
        dom.add_child(render_dom_from_app_node_inner(child_node, component_map, &FilteredComponentArguments::default())?);
    }
    Ok(dom)
}

/// Takes a single (expanded) app node and renders the DOM or returns an error
fn render_dom_from_app_node_inner<T>(
    xml_node: &XmlNode,
    component_map: &XmlComponentMap<T>,
    parent_xml_attributes: &FilteredComponentArguments,
) -> Result<Dom<T>, RenderDomError> {

    let component_name = normalize_casing(&xml_node.node_type);

    let (renderer, inherit_variables) = component_map.components.get(&component_name)
        .ok_or(RenderDomError::UnknownComponent(component_name.clone()))?;

    // Arguments of the current node
    let available_function_args = renderer.get_available_arguments();
    let mut filtered_xml_attributes = validate_and_filter_component_args(&xml_node.attributes, &available_function_args)?;

    if *inherit_variables {
        // Append all variables that are in scope for the parent node
        filtered_xml_attributes.extend(parent_xml_attributes.clone().into_iter());
    }

    // Instantiate the parent arguments in the current child arguments
    for v in filtered_xml_attributes.values_mut() {
        *v = format_args_dynamic(v, &parent_xml_attributes);
    }

    let text = xml_node.text.as_ref().map(|t| format_args_dynamic(t, &filtered_xml_attributes));

    let mut dom = renderer.render_dom(component_map, &filtered_xml_attributes, &text)?;
    set_attributes(&mut dom, &xml_node.attributes, &filtered_xml_attributes);

    for child_node in &xml_node.children {
        dom.add_child(render_dom_from_app_node_inner(child_node, component_map, &filtered_xml_attributes)?);
    }

    Ok(dom)
}

fn set_attributes<T>(dom: &mut Dom<T>, xml_attributes: &XmlAttributeMap, filtered_xml_attributes: &FilteredComponentArguments) {

    use dom::{TabIndex, DomString};

    if let Some(ids) = xml_attributes.get("id") {
        for id in ids.split_whitespace() {
            dom.add_id(DomString::Heap(format_args_dynamic(id, &filtered_xml_attributes)));
        }
    }

    if let Some(classes) = xml_attributes.get("class") {
        for class in classes.split_whitespace() {
            dom.add_class(DomString::Heap(format_args_dynamic(class, &filtered_xml_attributes)));
        }
    }

    if let Some(drag) = xml_attributes.get("draggable")
        .map(|d| format_args_dynamic(d, &filtered_xml_attributes))
        .and_then(|d| parse_bool(&d))
    {
        dom.set_draggable(drag);
    }

    if let Some(focusable) = xml_attributes.get("focusable")
        .map(|f| format_args_dynamic(f, &filtered_xml_attributes))
        .and_then(|f| parse_bool(&f))
    {
        match focusable {
            true => dom.set_tab_index(TabIndex::Auto),
            false => dom.set_tab_index(TabIndex::Auto), // TODO
        }
    }

    if let Some(tab_index) = xml_attributes.get("tabindex")
        .map(|val| format_args_dynamic(val, &filtered_xml_attributes))
        .and_then(|val| val.parse::<isize>().ok())
    {
        match tab_index {
            0 => dom.set_tab_index(TabIndex::Auto),
            i if i > 0 => dom.set_tab_index(TabIndex::OverrideInParent(i as usize)),
            _ => dom.set_tab_index(TabIndex::NoKeyboardFocus),
        }
    }
}

/// Given a string and a key => value mapping, replaces parts of the string with the value, i.e.:
///
/// ```rust,no_run,ignore
/// let variables = btreemap!{ "a" => "value1", "b" => "value2" };
/// let initial = "hello {a}, {b}{{ {c} }}";
/// let expected = "hello value1, value2{ {c} }";
/// assert_eq!(format_args_dynamic(initial, &variables), expected.to_string());
/// ```
pub fn format_args_dynamic(input: &str, variables: &FilteredComponentArguments) -> String {

    let mut opening_braces = Vec::new();
    let mut final_str = String::new();
    let input: Vec<char> = input.chars().collect();

    for (ch_idx, ch) in input.iter().enumerate() {
        match ch {
            '{' => {
                if input.get(ch_idx + 1) == Some(&'{') {
                    final_str.push('{');
                } else if ch_idx != 0 && input.get(ch_idx - 1) == Some(&'{') {
                    // second "{", do nothing
                } else {
                    // idx + 1 is not a "{"
                    opening_braces.push(ch_idx);
                }
            },
            '}' => {
                if input.get(ch_idx + 1) == Some(&'}') {
                    final_str.push('}');
                } else if ch_idx != 0 && input.get(ch_idx - 1) == Some(&'}') {
                    // second "}", do nothing
                } else {
                    // idx + 1 is not a "}"
                    match opening_braces.pop() {
                        Some(last_open) => {
                            let variable_name: String = input[(last_open + 1)..ch_idx].iter().collect();
                            let variable_name = normalize_casing(variable_name.trim());
                            match variables.get(&variable_name) {
                                Some(s) => final_str.push_str(s),
                                None => {
                                    final_str.push('{');
                                    final_str.push_str(&variable_name);
                                    final_str.push('}');
                                },
                            }
                        },
                        None => {
                            final_str.push('}');
                        },
                    }
                }
            },
            _ => {
                if opening_braces.last().is_none() {
                    final_str.push(*ch);
                }
            },
        }
    }

    final_str
}

/// Parses a string ("true" or "false")
fn parse_bool(input: &str) -> Option<bool> {
    match input {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Takes all components and generates the source code function from them
fn compile_components_to_rust_code<T>(components: &XmlComponentMap<T>)
-> Result<BTreeMap<ComponentName, (CompiledComponent, FilteredComponentArguments)>, CompileError>
{
    let mut map = BTreeMap::new();

    for (name, (component, should_inherit_variables)) in components.components.iter() {
        let mut rust_source_code = String::from("Dom::div()"); // TODO
        // let mut rust_source_code = component.compile_to_rust_code()?;
        let args = component.get_available_arguments();

        // components: &XmlComponentMap<T>, attributes: &FilteredComponentArguments, content: &XmlTextContent
        // warning: args = ComponentArguments, not yet filtered
        // fn get_available_arguments(&self) -> ComponentArguments;

        // let dom = component.render_dom(&components, &xml_node.args, &xml_node.text_content);
        // render_single_dom_node_to_string(&dom, &mut rust_string);

        map.insert(name.clone(), (rust_source_code, args));
    }

    Ok(map)
}

fn compile_app_node_to_rust_code<T>(app_node: &XmlNode, component_map: &XmlComponentMap<T>) -> Result<String, CompileError> {
    compile_app_node_to_rust_code_inner(app_node, component_map)
}

fn compile_app_node_to_rust_code_inner<T>(app_node: &XmlNode, component_map: &XmlComponentMap<T>) -> Result<String, CompileError> {
    // TODO!
    Err("unimplemented".into())
}

/// Takes a DOM node and appends the necessary `.with_id().with_class()`, etc. to the DOMs HEAD
fn render_single_dom_node_to_string<T>(dom: &Dom<T>, existing_str: &mut String) {

    let head = dom.get_head_node();

    for id in head.get_ids().iter() {
        existing_str.push_str(&format!(".with_id({})", id));
    }

    for class in head.get_classes().iter() {
        existing_str.push_str(&format!(".with_class({})", class));
    }

    if let Some(tab_index) = head.get_tab_index() {
        use dom::TabIndex::*;
        existing_str.push_str(&format!(".with_tab_index({})", match tab_index {
            Auto => format!("TabIndex::Auto"),
            OverrideInParent(u) => format!("TabIndex::OverrideInParent({})", u),
            NoKeyboardFocus => format!("TabIndex::NoKeyboardFocus"),
        }));
    }

    if head.get_is_draggable() {
        *existing_str += ".is_draggable(true)";
    }
}

#[test]
fn test_compile_dom_1() {

    struct Dummy;

    // Test the output of a certain component
    fn test_component_source_code(input: &str, component_name: &str, expected: &str) {
        let mut component_map = XmlComponentMap::<Dummy>::default();
        let root_nodes = parse_xml_string(input).unwrap();
        get_xml_components(&root_nodes, &mut component_map).unwrap();
        let app_node = get_app_node(&root_nodes).unwrap();
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
        let app_node = get_app_node(&root_nodes).unwrap();
        let app_source = compile_app_node_to_rust_code(&app_node, &component_map).unwrap();

        // TODO!
        // assert_eq!(app_source, expected);
    }

    let s1 = r#"
        <component name="test">
            <div id="a" class="b" draggable="true"></div>
        </component>

        <app>
            <Test />
        </app>
    "#;
    let s1_expected = r#"
        fn render_component_test<T>() -> Dom<T> {
            Dom::div().with_id("a").with_class("b").is_draggable(true)
        }
    "#;

    test_component_source_code(&s1, "test", &s1_expected);
}

// --- Renderers for various built-in types

/// Render for a `div` component
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DivRenderer { }

impl<T> XmlComponent<T> for DivRenderer {

    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments::new()
    }

    fn render_dom(&self, _: &XmlComponentMap<T>, _: &FilteredComponentArguments, _: &XmlTextContent) -> Result<Dom<T>, RenderDomError> {
        Ok(Dom::div())
    }

    fn compile_to_rust_code(&self, _: &XmlComponentMap<T>, _: &FilteredComponentArguments, _: &XmlTextContent) -> Result<String, CompileError> {
        Ok("Dom::div()".into())
    }
}

/// Render for a `p` component
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextRenderer { }

impl<T> XmlComponent<T> for TextRenderer {

    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments::new()
    }

    fn render_dom(&self, _: &XmlComponentMap<T>, _: &FilteredComponentArguments, content: &XmlTextContent) -> Result<Dom<T>, RenderDomError> {
        let content = content.as_ref().map(|s| prepare_string(&s)).unwrap_or_default();
        Ok(Dom::label(content))
    }

    fn compile_to_rust_code(&self, _: &XmlComponentMap<T>, args: &FilteredComponentArguments, content: &XmlTextContent) -> Result<String, CompileError> {
        Ok(match content {
            Some(c) => format!("Dom::label(format!(\"{}\", {}))", c, args.keys().map(|s| s.as_str()).collect::<Vec<&str>>().join(", ")),
            None => format!("Dom::label(\"\")"),
        })
    }
}

// NOTE: Two sequential returns count as a single return, while single returns get ignored.
fn prepare_string(input: &str) -> String {

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