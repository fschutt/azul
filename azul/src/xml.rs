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
/// fn render_component_test<'a, T: Layout>(args: &TestRendererArgs<'a>) -> Dom<T> {
///     Button::with_label(format!("Is this true? Scientists say: {:?}", args.b)).with_class(format!("test_{}", args.a))
/// }
/// ```
///
/// For this to work, a component has to note all its arguments and types that it can take.
/// If a type is not `str` or `String`, it will be formatted using the `{:?}` formatter
/// in the generated source code, otherwise the compiler will use the `{}` formatter.
pub type ComponentArguments = BTreeMap<ComponentArgumentName, ComponentArgumentType>;

/// Specifies a component that reacts to a parsed XML node
pub trait XmlComponent<T: Layout> {

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
    fn render_dom(&self, arguments: &ComponentArguments, content: &XmlTextContent) -> Result<Dom<T>, SyntaxError>;
    /// Used to compile the XML component to Rust code - input
    fn compile_to_rust_code(&self, attributes: &ComponentArguments, content: &XmlTextContent) -> Result<String, CompileError>;
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
pub struct XmlComponentMap<T: Layout> {
    components: BTreeMap<String, Box<XmlComponent<T>>>,
    /// Stores "onclick='do_this'" mappings from the string `do_this` to the actual function pointer
    callbacks: BTreeMap<String, Callback<T>>,
}

impl<T: Layout> Default for XmlComponentMap<T> {
    fn default() -> Self {
        let mut map = Self { components: BTreeMap::new(), callbacks: BTreeMap::new() };
        map.register_component("div", Box::new(DivRenderer { }));
        map.register_component("p", Box::new(TextRenderer { }));
        map
    }
}

impl<T: Layout> XmlComponentMap<T> {
    pub fn register_component<S: Into<String>>(&mut self, id: S, component: Box<XmlComponent<T>>) {
        self.components.insert(id.into(), component);
    }
    pub fn register_callback<S: Into<String>>(&mut self, id: S, callback: Callback<T>) {
        self.callbacks.insert(id.into(), callback);
    }
}

pub enum XmlParseError {
    /// No `<app></app>` root component present
    NoRootComponent,
    /// The DOM can only have one root component, not multiple.
    MultipleRootComponents,
    UnknownComponent(String),
    /// **Note**: Sadly, the error type can only be a string because xmlparser
    /// returns all errors as strings. There is an open PR to fix
    /// this deficiency, but since the XML parsing is only needed for
    /// hot-reloading and compiling, it doesn't matter that much.
    ParseError(XmlError),
    /// Invalid hierarchy close tags, i.e `<app></p></app>`
    MalformedHierarchy(String, String),
    /// A component raised an error while rendering the DOM - holds the component name + error string
    RenderDomError(String, String),
    /// Something went wrong while parsing an XML component
    Component(ComponentParseError),
}

#[derive(Clone, PartialOrd, PartialEq, Ord, Eq)]
pub enum ComponentParseError {
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
            UnknownComponent(name) => write!(f, "Unknown component: \"{}\"", name),
            RenderDomError(name, e) => write!(f, "Component \"{}\" raised an error while rendering DOM: \"{}\"", name, e),
            Component(c) => write!(f, "Error while parsing XML component: \"{}\"", c),
        }
    }
}

impl fmt::Debug for ComponentParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl fmt::Display for ComponentParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ComponentParseError::*;
        match self {
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
                    let lowercase_key = normalize_casing(key.to_str());
                    let lowercase_value = value.to_str().to_string().to_lowercase();
                    last.attributes.insert(lowercase_key, lowercase_value);
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

        args.insert(arg_name.to_string(), arg_type.to_string());
    }

    Ok(args)
}

/// Expands / instantiates all XML `<component />`s in the `<app />`
fn expand_xml_components(root_nodes: &[XmlNode]) -> Result<XmlNode, XmlParseError> {

    // Find the root <app /> node
    let mut root_node: XmlNode = get_app_node(root_nodes)?;
    let component_map = get_xml_components(root_nodes)?;

    if component_map.is_empty() {
        return Ok(root_node);
    }

    // Search all nodes of the app, expand them to the proper component
    for child in &mut root_node.children {
        *child = expand_component(child.clone(), &component_map);
    }

    Ok(root_node)
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
    let app_node: Vec<&XmlNode> = root_nodes.iter().filter(|node| &node.node_type == "app").collect();
    match app_node.len() {
        0 => return Err(XmlParseError::NoRootComponent),
        1 => Ok(app_node[0].clone()),
        _ => return Err(XmlParseError::MultipleRootComponents),
    }
}

/// Filter all <component /> nodes, error when a component doesn't have a name attribute
fn get_xml_components(root_nodes: &[XmlNode]) -> Result<BTreeMap<&String, &Vec<XmlNode>>, XmlParseError> {
    root_nodes
    .iter()
    .filter(|node| &node.node_type == "component")
    .map(|component| {
        match component.attributes.get("name") {
            None => Err(XmlParseError::Component(ComponentParseError::UnnamedComponent)),
            Some(s) => Ok((s, &component.children)),
        }
    })
    .collect()
}

/// Expands a single node during hot-reload to a DOM tree (warning: recursive!)
fn expand_component(node: XmlNode, component_map: &BTreeMap<&String, &Vec<XmlNode>>) -> XmlNode {
    match component_map.get(&node.node_type) {
        Some(s) => {
            // Turn the node to a div node with the original nodes attributes,
            // replace the children by the components children
            XmlNode {
                node_type: "div".into(),
                attributes: node.attributes.clone(),
                children: s.iter().map(|node| expand_component(node.clone(), component_map)).collect(),
                text: node.text,
            }
        },
        None => {
            XmlNode {
                children: node.children.iter().map(|n| expand_component(n.clone(), component_map)).collect(),
                .. node
            }
        },
    }
}

/// Parses an XML string and returns a `Dom` with the components instantiated in the `<app></app>`
pub fn str_to_dom<T: Layout>(xml: &str, component_map: &XmlComponentMap<T>) -> Result<Dom<T>, XmlParseError> {
    let parsed_xml = parse_xml_string(xml)?;
    let expanded_xml = expand_xml_components(&parsed_xml)?;
    render_dom_from_app_node(&expanded_xml, component_map)
}

/// Parses an XML string and returns a `String`, which contains the Rust source code (i.e. it compiles the XML to valid Rust)
pub fn str_to_rust_code<T: Layout>(xml: &str, component_map: &XmlComponentMap<T>) -> Result<String, CompileError> {
    let parsed_xml = parse_xml_string(xml).map_err(|e| format!("XML parse error: {}", e))?;
    compile_app_node_to_rust_code(&parsed_xml, component_map)
}

fn render_dom_from_app_node<T: Layout>(
    app_node: &XmlNode,
    component_map: &XmlComponentMap<T>
) -> Result<Dom<T>, XmlParseError> {

    // Don't actually render the <app></app> node itself
    let mut dom = Dom::div();
    for child_node in &app_node.children {
        dom.add_child(render_dom_from_app_node_inner(child_node, component_map)?);
    }
    Ok(dom)
}

/// Takes a single (expanded) app node and renders the DOM or returns an error
fn render_dom_from_app_node_inner<T: Layout>(
    xml_node: &XmlNode,
    component_map: &XmlComponentMap<T>
) -> Result<Dom<T>, XmlParseError> {

    use dom::{TabIndex, DomString};

    let self_node_renderer = component_map.components.get(&xml_node.node_type)
        .ok_or(XmlParseError::UnknownComponent(xml_node.node_type.clone()))?;

    let mut dom = self_node_renderer.render_dom(&xml_node.attributes, &xml_node.text)
        .map_err(|e| XmlParseError::RenderDomError(xml_node.node_type.clone(), e))?;

    if let Some(ids) = xml_node.attributes.get("id") {
        for id in ids.split_whitespace() {
            dom.add_id(DomString::Heap(id.to_string()));
        }
    }

    if let Some(classes) = xml_node.attributes.get("class") {
        for class in classes.split_whitespace() {
            dom.add_class(DomString::Heap(class.to_string()));
        }
    }

    if let Some(drag) = xml_node.attributes.get("draggable") {
        match drag.as_ref() {
            "true" => dom.set_draggable(true),
            "false" => dom.set_draggable(false),
            _ => { },
        }
    }

    if let Some(focusable) = xml_node.attributes.get("focusable") {
        match focusable.as_ref() {
            "true" => dom.set_tab_index(TabIndex::Auto),
            "false" => dom.set_tab_index(TabIndex::Auto),
            _ => { },
        }
    }

    if let Some(tab_index) = xml_node.attributes.get("tabindex").and_then(|val| val.parse::<isize>().ok()) {
        match tab_index {
            0 => dom.set_tab_index(TabIndex::Auto),
            i if i > 0 => dom.set_tab_index(TabIndex::OverrideInParent(i as usize)),
            _ => dom.set_tab_index(TabIndex::NoKeyboardFocus),
        }
    }

    for child_node in &xml_node.children {
        dom.add_child(render_dom_from_app_node_inner(child_node, component_map)?);
    }

    Ok(dom)
}

fn compile_app_node_to_rust_code<T: Layout>(
    root_nodes: &[XmlNode],
    component_map: &XmlComponentMap<T>
) -> Result<String, CompileError> {

    Err("unimplemented".into())

    /*
    // Find the root <app /> node
    let mut app_node: XmlNode = get_app_node(root_nodes)?;
    let component_nodes = get_xml_components(root_nodes)?;

    let mut root_string = String::new();

    // Render all built-in component nodes
    for (component_node_key, component_nodes) in component_map {
        match component_map.get(&node.node_type) {
            Some(s) => {
                // Turn the node to a div node with the original nodes attributes,
                // replace the children by the components children
                XmlNode {
                    node_type: "div".into(),
                    attributes: node.attributes.clone(),
                    children: s.iter().map(|node| expand_component(node.clone(), component_map)).collect(),
                    text: node.text,
                }
            },
            None => {
                XmlNode {
                    children: node.children.iter().map(|n| expand_component(n.clone(), component_map)).collect(),
                    .. node
                }
            },
        }
    }

    // Search all nodes of the app, expand them to the proper component
    for child in &mut root_node.children {
        *child = expand_component(child.clone(), &component_map);
    }

    // Don't actually render the <app></app> node itself
    let mut dom = "Dom::div()";
    for child_node in &app_node.children {
        dom.append(compile_app_node_to_rust_code_inner(child_node, component_map)?);
    }

    Ok(dom)
    */
}

fn compile_app_node_to_rust_code_inner<T: Layout>(
    app_node: &XmlNode,
    component_map: &XmlComponentMap<T>
) -> Result<String, CompileError> {
    // TODO!
    Err("unimplemented".into())
}

// --- Renderers for various built-in types

/// Render for a `div` component
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DivRenderer { }

impl<T: Layout> XmlComponent<T> for DivRenderer {

    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments::new()
    }

    fn render_dom(&self, _: &ComponentArguments, _: &XmlTextContent) -> Result<Dom<T>, SyntaxError> {
        Ok(Dom::div())
    }

    fn compile_to_rust_code(&self, _: &ComponentArguments, _: &XmlTextContent) -> Result<String, CompileError> {
        Ok("Dom::div()".into())
    }
}

/// Render for a `p` component
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextRenderer { }

impl<T: Layout> XmlComponent<T> for TextRenderer {

    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments::new()
    }

    fn render_dom(&self, _: &ComponentArguments, content: &XmlTextContent) -> Result<Dom<T>, SyntaxError> {
        let content = content.as_ref().map(|s| prepare_string(&s)).unwrap_or_default();
        Ok(Dom::label(content))
    }

    fn compile_to_rust_code(&self, _: &ComponentArguments, content: &XmlTextContent) -> Result<String, CompileError> {
        Ok(match content {
            Some(s) => format!("Dom::label(\"{}\")", content.as_ref().map(|s| prepare_string(&s)).unwrap_or_default()),
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

    let mut line_len = final_lines.len();
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
fn test_normalize_casing() {
    assert_eq!(normalize_casing("abcDef"), String::from("abc_def"));
    assert_eq!(normalize_casing("abc_Def"), String::from("abc_def"));
    assert_eq!(normalize_casing("abc-Def"), String::from("abc_def"));
    assert_eq!(normalize_casing("abc-def"), String::from("abc_def"));
    assert_eq!(normalize_casing("AbcDef"), String::from("abc_def"));
    assert_eq!(normalize_casing("Abc-Def"), String::from("abc_def"));
    assert_eq!(normalize_casing("Abc_Def"), String::from("abc_def"));
    assert_eq!(normalize_casing("aBc_Def"), String::from("a_bc_def")); // wrong, but whatever
}

#[test]
fn test_parse_component_arguments() {

    let mut args_1_expected = ComponentArguments::new();
    args_1_expected.insert("selectedDate".to_string(), "DateTime".to_string());
    args_1_expected.insert("minimumDate".to_string(), "DateTime".to_string());
    args_1_expected.insert("gridVisible".to_string(), "bool".to_string());

    /// Everything OK
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
    println!("{:?}", output);
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
    println!("{:?}", output);
    assert_eq!(output, String::from("Hello, 123\nTest Test2\nTest3\nTest4"));
}