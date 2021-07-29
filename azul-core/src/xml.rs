//! XML structure definitions

use core::fmt;
use alloc::collections::BTreeMap;
use azul_css::{
    AzString, Css, U8Vec, OptionAzString,
    NodeTypeTag, CssPathSelector, CssRuleBlock,
    CssPath, CssPathPseudoSelector,
    StyleBackgroundSizeVec,
    StyleBackgroundRepeatVec,
    StyleBackgroundContentVec,
    StyleBackgroundPositionVec,
    StyleTransformVec,
    StyleFontFamilyVec,
    NormalizedLinearColorStopVec,
    NormalizedRadialColorStopVec,
};
use crate::window::{AzStringPair, StringPairVec};
use crate::styled_dom::StyledDom;
use crate::css::VecContents;
use crate::dom::Dom;
#[cfg(feature = "css_parser")]
use azul_css_parser::CssParseError;

/// Error that can happen during hot-reload -
/// stringified, since it is only used for printing and is not exposed in the public API
pub type SyntaxError = String;
/// Tag of an XML node, such as the "button" in `<button>Hello</button>`.
pub type XmlTagName = AzString;
/// (Unparsed) text content of an XML node, such as the "Hello" in `<button>Hello</button>`.
pub type XmlTextContent = OptionAzString;
/// Attributes of an XML node, such as `["color" => "blue"]` in `<button color="blue" />`.
pub type XmlAttributeMap = StringPairVec;

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

#[allow(non_camel_case_types)]
pub enum c_void { }

#[repr(C)]
pub enum XmlNodeType {
    Root,
    Element,
    PI,
    Comment,
    Text,
}

#[repr(C)]
pub struct XmlQualifiedName {
    pub name: AzString,
    pub namespace: OptionAzString,
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct Xml {
    pub root: XmlNodeVec,
}


#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct NonXmlCharError {
    pub ch: u32, /* u32 = char, but ABI stable */
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct InvalidCharError {
    pub expected: u8,
    pub got: u8,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct InvalidCharMultipleError {
    pub expected: u8,
    pub got: U8Vec,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct InvalidQuoteError {
    pub got: u8,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct InvalidSpaceError {
    pub got: u8,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct InvalidStringError {
    pub got: AzString,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C, u8)]
pub enum XmlStreamError {
    UnexpectedEndOfStream,
    InvalidName,
    NonXmlChar(NonXmlCharError),
    InvalidChar(InvalidCharError),
    InvalidCharMultiple(InvalidCharMultipleError),
    InvalidQuote(InvalidQuoteError),
    InvalidSpace(InvalidSpaceError),
    InvalidString(InvalidStringError),
    InvalidReference,
    InvalidExternalID,
    InvalidCommentData,
    InvalidCommentEnd,
    InvalidCharacterData,
}

impl fmt::Display for XmlStreamError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::XmlStreamError::*;
        match self {
            UnexpectedEndOfStream => write!(f, "Unexpected end of stream"),
            InvalidName => write!(f, "Invalid name"),
            NonXmlChar(nx) => write!(f, "Non-XML character: {:?} at {}", core::char::from_u32(nx.ch), nx.pos),
            InvalidChar(ic) => write!(f, "Invalid character: expected: {}, got: {} at {}", ic.expected as char, ic.got as char, ic.pos),
            InvalidCharMultiple(imc) => write!(f, "Multiple invalid characters: expected: {}, got: {:?} at {}", imc.expected, imc.got.as_ref(), imc.pos),
            InvalidQuote(iq) => write!(f, "Invalid quote: got {} at {}", iq.got as char, iq.pos),
            InvalidSpace(is) => write!(f, "Invalid space: got {} at {}", is.got as char, is.pos),
            InvalidString(ise) => write!(f, "Invalid string: got \"{}\" at {}", ise.got.as_str(), ise.pos),
            InvalidReference => write!(f, "Invalid reference"),
            InvalidExternalID => write!(f, "Invalid external ID"),
            InvalidCommentData => write!(f, "Invalid comment data"),
            InvalidCommentEnd => write!(f, "Invalid comment end"),
            InvalidCharacterData => write!(f, "Invalid character data"),
        }
    }
}

#[derive(Debug, PartialEq, PartialOrd, Clone, Ord, Hash, Eq)]
#[repr(C)]
pub struct XmlTextPos {
    pub row: u32,
    pub col: u32,
}

impl fmt::Display for XmlTextPos {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "line {}:{}", self.row, self.col)
    }
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct XmlTextError {
    pub stream_error: XmlStreamError,
    pub pos: XmlTextPos
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C, u8)]
pub enum XmlParseError {
    InvalidDeclaration(XmlTextError),
    InvalidComment(XmlTextError),
    InvalidPI(XmlTextError),
    InvalidDoctype(XmlTextError),
    InvalidEntity(XmlTextError),
    InvalidElement(XmlTextError),
    InvalidAttribute(XmlTextError),
    InvalidCdata(XmlTextError),
    InvalidCharData(XmlTextError),
    UnknownToken(XmlTextPos),
}

impl fmt::Display for XmlParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::XmlParseError::*;
        match self {
            InvalidDeclaration(e) => write!(f, "Invalid declaraction: {} at {}", e.stream_error, e.pos),
            InvalidComment(e) => write!(f, "Invalid comment: {} at {}", e.stream_error, e.pos),
            InvalidPI(e) => write!(f, "Invalid processing instruction: {} at {}", e.stream_error, e.pos),
            InvalidDoctype(e) => write!(f, "Invalid doctype: {} at {}", e.stream_error, e.pos),
            InvalidEntity(e) => write!(f, "Invalid entity: {} at {}", e.stream_error, e.pos),
            InvalidElement(e) => write!(f, "Invalid element: {} at {}", e.stream_error, e.pos),
            InvalidAttribute(e) => write!(f, "Invalid attribute: {} at {}", e.stream_error, e.pos),
            InvalidCdata(e) => write!(f, "Invalid CDATA: {} at {}", e.stream_error, e.pos),
            InvalidCharData(e) => write!(f, "Invalid char data: {} at {}", e.stream_error, e.pos),
            UnknownToken(e) => write!(f, "Unknown token at {}", e),
        }
    }
}

impl_result!(Xml, XmlError, ResultXmlXmlError, copy = false, [Debug, PartialEq, PartialOrd, Clone]);

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct DuplicatedNamespaceError {
    pub ns: AzString,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct UnknownNamespaceError {
    pub ns: AzString,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct UnexpectedCloseTagError {
    pub expected: AzString,
    pub actual: AzString,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct UnknownEntityReferenceError {
    pub entity: AzString,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct DuplicatedAttributeError {
    pub attribute: AzString,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C, u8)]
pub enum XmlError {
    NoParserAvailable,
    InvalidXmlPrefixUri(XmlTextPos),
    UnexpectedXmlUri(XmlTextPos),
    UnexpectedXmlnsUri(XmlTextPos),
    InvalidElementNamePrefix(XmlTextPos),
    DuplicatedNamespace(DuplicatedNamespaceError),
    UnknownNamespace(UnknownNamespaceError),
    UnexpectedCloseTag(UnexpectedCloseTagError),
    UnexpectedEntityCloseTag(XmlTextPos),
    UnknownEntityReference(UnknownEntityReferenceError),
    MalformedEntityReference(XmlTextPos),
    EntityReferenceLoop(XmlTextPos),
    InvalidAttributeValue(XmlTextPos),
    DuplicatedAttribute(DuplicatedAttributeError),
    NoRootNode,
    SizeLimit,
    DtdDetected,
    /// Invalid hierarchy close tags, i.e `<app></p></app>`
    MalformedHierarchy(AzString, AzString),
    ParserError(XmlParseError),
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::XmlError::*;
        match self {
            NoParserAvailable => write!(f, "Library was compiled without XML parser (XML parser not available)"),
            InvalidXmlPrefixUri(pos) => write!(f, "Invalid XML Prefix URI at line {}:{}", pos.row, pos.col),
            UnexpectedXmlUri(pos) => write!(f, "Unexpected XML URI at at line {}:{}", pos.row, pos.col),
            UnexpectedXmlnsUri(pos) => write!(f, "Unexpected XML namespace URI at line {}:{}", pos.row, pos.col),
            InvalidElementNamePrefix(pos) => write!(f, "Invalid element name prefix at line {}:{}", pos.row, pos.col),
            DuplicatedNamespace(ns) => write!(f, "Duplicated namespace: \"{}\" at {}", ns.ns.as_str(), ns.pos),
            UnknownNamespace(uns) => write!(f, "Unknown namespace: \"{}\" at {}", uns.ns.as_str(), uns.pos),
            UnexpectedCloseTag(ct) => write!(f, "Unexpected close tag: expected \"{}\", got \"{}\" at {}", ct.expected.as_str(), ct.actual.as_str(), ct.pos),
            UnexpectedEntityCloseTag(pos) => write!(f, "Unexpected entity close tag at line {}:{}", pos.row, pos.col),
            UnknownEntityReference(uer) => write!(f, "Unexpected entity reference: \"{}\" at {}", uer.entity, uer.pos),
            MalformedEntityReference(pos) => write!(f, "Malformed entity reference at line {}:{}", pos.row, pos.col),
            EntityReferenceLoop(pos) => write!(f, "Entity reference loop (recursive entity reference) at line {}:{}", pos.row, pos.col),
            InvalidAttributeValue(pos) => write!(f, "Invalid attribute value at line {}:{}", pos.row, pos.col),
            DuplicatedAttribute(ae) => write!(f, "Duplicated attribute \"{}\" at line {}:{}", ae.attribute.as_str(), ae.pos.row, ae.pos.col),
            NoRootNode => write!(f, "No root node found"),
            SizeLimit => write!(f, "XML file too large (size limit reached)"),
            DtdDetected => write!(f, "Document type descriptor detected"),
            MalformedHierarchy(expected, got) => write!(f, "Malformed hierarchy: expected <{}/> closing tag, got <{}/>", expected.as_str(), got.as_str()),
            ParserError(p) => write!(f, "{}", p),
        }
    }
}

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
/// fn render_component_test<'a, T>(args: &TestRendererArgs<'a>) -> StyledDom {
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
    /// calendar(&CalendarRendererArgs {
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
    fn render_dom<'a>(&'a self, components: &'a XmlComponentMap, arguments: &FilteredComponentArguments, content: &XmlTextContent) -> Result<StyledDom, RenderDomError<'a>>;
    /// Used to compile the XML component to Rust code - input
    fn compile_to_rust_code(&self, components: &XmlComponentMap, attributes: &FilteredComponentArguments, content: &XmlTextContent) -> Result<String, CompileError>;
    /// Returns the XML node for this component (necessary to compile the component into a function
    /// during the Rust compilation stage)
    fn get_xml_node<'a>(&'a self) -> &'a XmlNode;
}

/// Wrapper for the XML parser - necessary to easily create a Dom from
/// XML without putting an XML solver into `azul-core`.
#[derive(Default)]
pub struct DomXml {
    pub parsed_dom: StyledDom,
}

impl DomXml {

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

/// Represents one XML node tag
#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct XmlNode {
    /// Type of the node
    pub node_type: XmlTagName,
    /// Attributes of an XML node (note: not yet filtered and / or broken into function arguments!)
    pub attributes: XmlAttributeMap,
    /// Direct children of this node
    pub children: XmlNodeVec,
    /// String content of the node, i.e the "Hello" in `<p>Hello</p>`
    pub text: XmlTextContent,
}

impl XmlNode {
    pub fn new<I: Into<XmlTagName>>(node_type: I) -> Self {
        XmlNode { node_type: node_type.into(), .. Default::default() }
    }
}

impl_vec!(XmlNode, XmlNodeVec, XmlNodeVecDestructor);
impl_vec_mut!(XmlNode, XmlNodeVec);
impl_vec_debug!(XmlNode, XmlNodeVec);
impl_vec_partialeq!(XmlNode, XmlNodeVec);
impl_vec_eq!(XmlNode, XmlNodeVec);
impl_vec_partialord!(XmlNode, XmlNodeVec);
impl_vec_ord!(XmlNode, XmlNodeVec);
impl_vec_hash!(XmlNode, XmlNodeVec);
impl_vec_clone!(XmlNode, XmlNodeVec, XmlNodeVecDestructor);

/// Holds all XML components - builtin components
pub struct XmlComponentMap {
    /// Stores all known components that can be used during DOM rendering
    /// + whether this component should inherit variables from the parent scope
    components: BTreeMap<String, (Box<dyn XmlComponent>, bool)>,
}


impl Default for XmlComponentMap {
    fn default() -> Self {
        let mut map = Self { components: BTreeMap::new() };
        map.register_component("body", Box::new(BodyRenderer::new()), true);
        map.register_component("div", Box::new(DivRenderer::new()), true);
        map.register_component("p", Box::new(TextRenderer::new()), true);
        map
    }
}

impl XmlComponentMap {
    pub fn register_component(
        &mut self,
        id: &str,
        component: Box<dyn XmlComponent>,
        inherit_variables: bool
    ) {
        self.components.insert(
            normalize_casing(id),
            (component, inherit_variables)
        );
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DomXmlParseError<'a> {
    /// No `<html></html>` node component present
    NoHtmlNode,
    /// Multiple `<html>` nodes
    MultipleHtmlRootNodes,
    /// No ´<body></body>´ node in the root HTML
    NoBodyInHtml,
    /// The DOM can only have one <body> node, not multiple.
    MultipleBodyNodes,
    /// Note: Sadly, the error type can only be a string because xmlparser
    /// returns all errors as strings. There is an open PR to fix
    /// this deficiency, but since the XML parsing is only needed for
    /// hot-reloading and compiling, it doesn't matter that much.
    Xml(XmlError),
    /// Invalid hierarchy close tags, i.e `<app></p></app>`
    MalformedHierarchy(AzString, AzString),
    /// A component raised an error while rendering the DOM - holds the component name + error string
    RenderDom(RenderDomError<'a>),
    /// Something went wrong while parsing an XML component
    Component(ComponentParseError<'a>),
    /// Error parsing global CSS in head node
    Css(CssParseError<'a>),
}

impl<'a> From<XmlError> for DomXmlParseError<'a> {
    fn from(e: XmlError) -> Self {
        Self::Xml(e)
    }
}

impl<'a> From<ComponentParseError<'a>> for DomXmlParseError<'a> {
    fn from(e: ComponentParseError<'a>) -> Self {
        Self::Component(e)
    }
}

impl<'a> From<RenderDomError<'a>> for DomXmlParseError<'a> {
    fn from(e: RenderDomError<'a>) -> Self {
        Self::RenderDom(e)
    }
}

impl<'a> From<CssParseError<'a>> for DomXmlParseError<'a> {
    fn from(e: CssParseError<'a>) -> Self {
        Self::Css(e)
    }
}

/// Error that can happen from the translation from XML code to Rust code -
/// stringified, since it is only used for printing and is not exposed in the public API
#[derive(Debug, Clone, PartialEq)]
pub enum CompileError<'a> {
    Dom(RenderDomError<'a>),
    Xml(DomXmlParseError<'a>),
    Css(CssParseError<'a>),
}

impl<'a> From<ComponentError> for CompileError<'a> {
    fn from(e: ComponentError) -> Self {
        CompileError::Dom(RenderDomError::Component(e))
    }
}

impl<'a> From<CssParseError<'a>> for CompileError<'a> {
    fn from(e: CssParseError<'a>) -> Self {
        CompileError::Css(e)
    }
}

impl<'a> fmt::Display for CompileError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::CompileError::*;
        match self {
            Dom(d) => write!(f, "{}", d),
            Xml(s) => write!(f, "{}", s),
            Css(s) => write!(f, "{}", s),
        }
    }
}

impl<'a> From<RenderDomError<'a>> for CompileError<'a> {
    fn from(e: RenderDomError<'a>) -> Self {
        CompileError::Dom(e)
    }
}

impl<'a> From<DomXmlParseError<'a>> for CompileError<'a> {
    fn from(e: DomXmlParseError<'a>) -> Self {
        CompileError::Xml(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentError {
    /// While instantiating a component, a function argument
    /// was encountered that the component won't use or react to.
    UselessFunctionArgument(AzString, AzString, Vec<String>),
    /// A certain node type can't be rendered, because the
    /// renderer for this node is not available isn't available
    ///
    /// UnknownComponent(component_name)
    UnknownComponent(AzString),
}

#[derive(Debug, Clone, PartialEq)]
pub enum RenderDomError<'a> {
    Component(ComponentError),
    /// Error parsing the CSS on the component style
    CssError(CssParseError<'a>),
}

impl<'a> From<ComponentError> for RenderDomError<'a> {
    fn from(e: ComponentError) -> Self {
        Self::Component(e)
    }
}

impl<'a> From<CssParseError<'a>> for RenderDomError<'a> {
    fn from(e: CssParseError<'a>) -> Self {
        Self::CssError(e)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComponentParseError<'a> {
    /// Given XmlNode is not a `<component />` node.
    NotAComponent,
    /// A `<component>` node does not have a `name` attribute.
    UnnamedComponent,
    /// Argument at position `usize` is either empty or has no name
    MissingName(usize),
    /// Argument at position `usize` with the name
    /// `String` doesn't have a `: type`
    MissingType(usize, AzString),
    /// Component name may not contain a whitespace
    /// (probably missing a `:` between the name and the type)
    WhiteSpaceInComponentName(usize, AzString),
    /// Component type may not contain a whitespace
    /// (probably missing a `,` between the type and the next name)
    WhiteSpaceInComponentType(usize, AzString, AzString),
    /// Error parsing the <style> tag / CSS
    CssError(CssParseError<'a>),
}

impl<'a> fmt::Display for DomXmlParseError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::DomXmlParseError::*;
        match self {
            NoHtmlNode => write!(f, "No <html> node found as the root of the file - empty file?"),
            MultipleHtmlRootNodes => write!(f, "Multiple <html> nodes found as the root of the file - only one root node allowed"),
            NoBodyInHtml => write!(f, "No <body> node found as a direct child of an <html> node - malformed DOM hierarchy?"),
            MultipleBodyNodes => write!(f, "Multiple <body> nodes present, only one <body> node is allowed"),
            Xml(e) => write!(f, "Error parsing XML: {}", e),
            MalformedHierarchy(got, expected) => write!(f, "Invalid </{}> tag: expected </{}>", got.as_str(), expected.as_str()),
            RenderDom(e) => write!(f, "Error rendering DOM: {}", e),
            Component(c) => write!(f, "Error parsing component in <head> node:\r\n{}", c),
            Css(c) => write!(f, "Error parsing CSS in <head> node:\r\n{}", c),
        }
    }
}

impl<'a> fmt::Display for ComponentParseError<'a> {
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

impl fmt::Display for ComponentError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ComponentError::*;
        match self {
            UselessFunctionArgument(k, v, available_args) => {
                write!(f, "Useless component argument \"{}\": \"{}\" - available args are: {:#?}",
                    k, v, available_args
                )
            },
            UnknownComponent(name) => write!(f, "Unknown component: \"{}\"", name),
        }
    }
}

impl<'a> fmt::Display for RenderDomError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::RenderDomError::*;
        match self {
            Component(c) => write!(f, "{}", c),
            CssError(e) => write!(f, "Error parsing CSS in component: {}", e),
        }
    }
}


// --- Renderers for various built-in types

/// Render for a `div` component
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DivRenderer {
    node: XmlNode,
}

impl DivRenderer {
    pub fn new() -> Self {
        Self { node: XmlNode::new("div") }
    }
}

impl XmlComponent for DivRenderer {

    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments::new()
    }

    fn render_dom(&self, _: &XmlComponentMap, _: &FilteredComponentArguments, _: &XmlTextContent) -> Result<StyledDom, RenderDomError> {
        Ok(Dom::div().style(&mut Css::empty()))
    }

    fn compile_to_rust_code(&self, _: &XmlComponentMap, _: &FilteredComponentArguments, _: &XmlTextContent) -> Result<String, CompileError> {
        Ok("Dom::div()".into())
    }

    fn get_xml_node<'a>(&'a self) -> &'a XmlNode { &self.node }
}

/// Render for a `body` component
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BodyRenderer {
    node: XmlNode,
}

impl BodyRenderer {
    pub fn new() -> Self {
        Self { node: XmlNode::new("body") }
    }
}

impl XmlComponent for BodyRenderer {

    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments::new()
    }

    fn render_dom(&self, _: &XmlComponentMap, _: &FilteredComponentArguments, _: &XmlTextContent) -> Result<StyledDom, RenderDomError> {
        Ok(Dom::body().style(&mut Css::empty()))
    }

    fn compile_to_rust_code(&self, _: &XmlComponentMap, _: &FilteredComponentArguments, _: &XmlTextContent) -> Result<String, CompileError> {
        Ok("Dom::body()".into())
    }

    fn get_xml_node<'a>(&'a self) -> &'a XmlNode { &self.node }
}

/// Render for a `p` component
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextRenderer {
    node: XmlNode,
}

impl TextRenderer {
    pub fn new() -> Self {
        Self { node: XmlNode::new("p") }
    }
}

impl XmlComponent for TextRenderer {

    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments {
            args: ComponentArgumentsMap::default(),
            accepts_text: true, // important!
        }
    }

    fn render_dom(&self, _: &XmlComponentMap, _: &FilteredComponentArguments, content: &XmlTextContent) -> Result<StyledDom, RenderDomError> {
        let content = content.as_ref().map(|s| prepare_string(&s)).unwrap_or_default();
        Ok(Dom::text(content).style(&mut Css::empty()))
    }

    fn compile_to_rust_code(&self, _: &XmlComponentMap, args: &FilteredComponentArguments, content: &XmlTextContent) -> Result<String, CompileError> {
        Ok(String::from("Dom::text(text)"))
    }

    fn get_xml_node<'a>(&'a self) -> &'a XmlNode { &self.node }
}

/// Compiles a XML `args="a: String, b: bool"` into a `["a" => "String", "b" => "bool"]` map
pub fn parse_component_arguments<'a>(input: &'a str) -> Result<ComponentArgumentsMap, ComponentParseError<'a>> {

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
-> Result<FilteredComponentArguments, ComponentError> {

    let mut map = FilteredComponentArguments {
        args: ComponentArgumentsMap::default(),
        accepts_text: valid_args.accepts_text,
    };

    for AzStringPair { key, value } in xml_attributes.as_ref().iter() {
        let xml_attribute_name = key;
        let xml_attribute_value = value;
        if let Some((valid_arg_type, valid_arg_index)) = valid_args.args.get(xml_attribute_name.as_str()) {
            map.args.insert(xml_attribute_name.clone().into_library_owned_string(), (valid_arg_type.clone(), *valid_arg_index));
        } else if DEFAULT_ARGS.contains(&xml_attribute_name.as_str()) {
            // no error, but don't insert the attribute name
        } else {
            // key was not expected for this component
            let keys = valid_args.args.keys().cloned().collect();
            return Err(ComponentError::UselessFunctionArgument(
                    xml_attribute_name.clone(),
                    xml_attribute_value.clone(),
                    keys
                )
            );
        }
    }

    Ok(map)
}

/// Find the one and only `<body>` node, return error if
/// there is no app node or there are multiple app nodes
pub fn get_html_node<'a>(root_nodes: &'a [XmlNode]) -> Result<&'a XmlNode, DomXmlParseError> {

    let mut html_node_iterator = root_nodes.iter().filter(|node| {
        let node_type_normalized = normalize_casing(&node.node_type);
        &node_type_normalized == "html"
    });

    let html_node = html_node_iterator.next().ok_or(DomXmlParseError::NoHtmlNode)?;
    if html_node_iterator.next().is_some() {
        Err(DomXmlParseError::MultipleHtmlRootNodes)
    } else {
        Ok(html_node)
    }
}

/// Find the one and only `<body>` node, return error if
/// there is no app node or there are multiple app nodes
pub fn get_body_node<'a>(root_nodes: &'a [XmlNode]) -> Result<&'a XmlNode, DomXmlParseError> {

    let mut body_node_iterator = root_nodes.iter().filter(|node| {
        let node_type_normalized = normalize_casing(&node.node_type);
        &node_type_normalized == "body"
    });

    let body_node = body_node_iterator.next().ok_or(DomXmlParseError::NoBodyInHtml)?;
    if body_node_iterator.next().is_some() {
        Err(DomXmlParseError::MultipleBodyNodes)
    } else {
        Ok(body_node)
    }
}

static DEFAULT_STR: &str = "";

/// Searches in the the `root_nodes` for a `node_type`, convenience function in order to
/// for example find the first <blah /> node in all these nodes.
pub fn find_node_by_type<'a>(root_nodes: &'a [XmlNode], node_type: &str) -> Option<&'a XmlNode> {
    root_nodes.iter().find(|n| normalize_casing(&n.node_type).as_str() == node_type)
}

pub fn find_attribute<'a>(node: &'a XmlNode, attribute: &str) -> Option<&'a AzString> {
    node.attributes.iter().find(|n| normalize_casing(&n.key.as_str()).as_str() == attribute).map(|s| &s.value)
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
    Some(unsafe { &mut *mut_node_ptr }) // safe because we hold a &'a mut XmlNode
}


/// Parses an XML string and returns a `StyledDom` with the components instantiated in the `<app></app>`
pub fn str_to_dom<'a>(
    root_nodes: &'a [XmlNode],
    component_map: &'a mut XmlComponentMap
) -> Result<StyledDom, DomXmlParseError<'a>> {

    let html_node = get_html_node(root_nodes)?;
    let body_node = get_body_node(html_node.children.as_ref())?;

    let mut global_style = None;

    if let Some(head_node) = find_node_by_type(html_node.children.as_ref(), "head") {

        // parse all dynamic XML components from the head node
        for node in head_node.children.as_ref() {
            match DynamicXmlComponent::new(node) {
                Ok(node) => {
                    let node_name = node.name.clone();
                    component_map.register_component(node_name.as_str(), Box::new(node), false);
                },
                Err(ComponentParseError::NotAComponent) => { }, // not a <component /> node, ignore
                Err(e) => return Err(e.into()), // Error during parsing the XML component, bail
            }
        }

        // parse the <style></style> tag contents, if present
        if let Some(style_node) = find_node_by_type(head_node.children.as_ref(), "style") {
            if let Some(text) = style_node.text.as_ref().map(|s| s.as_str()) {
                let parsed_css = azul_css_parser::new_from_str(&text)?;
                global_style = Some(parsed_css);
            }
        }
    }

    render_dom_from_body_node(
        &body_node,
        global_style,
        component_map,
    ).map_err(|e| e.into())
}

/// Parses an XML string and returns a `String`, which contains the Rust source code
/// (i.e. it compiles the XML to valid Rust)
pub fn str_to_rust_code<'a>(
    root_nodes: &'a [XmlNode],
    imports: &str,
    component_map: &'a mut XmlComponentMap
) -> Result<String, CompileError<'a>> {

    let html_node = get_html_node(&root_nodes)?;
    let body_node = get_body_node(html_node.children.as_ref())?;
    let mut global_style = Css::empty();

    if let Some(head_node) = html_node.children.as_ref().iter().find(|n| normalize_casing(&n.node_type).as_str() == "head") {
        for node in head_node.children.as_ref() {
            match DynamicXmlComponent::new(node) {
                Ok(node) => {
                    let node_name = node.name.clone();
                    component_map.register_component(node_name.as_str(), Box::new(node), false);
                },
                Err(ComponentParseError::NotAComponent) => { }, // not a <component /> node, ignore
                Err(e) => return Err(CompileError::Xml(e.into())), // Error during parsing the XML component, bail
            }
        }

        if let Some(style_node) = find_node_by_type(head_node.children.as_ref(), "style") {
            if let Some(text) = style_node.text.as_ref().map(|s| s.as_str()) {
                let parsed_css = azul_css_parser::new_from_str(&text)?;
                global_style = parsed_css;
            }
        }
    }

    global_style.sort_by_specificity();

    let mut css_blocks = BTreeMap::new();
    let mut extra_blocks = VecContents::default();
    let app_source = compile_body_node_to_rust_code(
        &body_node,
        component_map,
        &mut extra_blocks,
        &mut css_blocks,
        &global_style,
        CssMatcher {
            path: Vec::new(),
            indices_in_parent: vec![0],
            children_length: vec![body_node.children.as_ref().len()],
        }
    )?;

    let app_source = app_source
    .lines()
    .map(|l| format!("        {}", l))
    .collect::<Vec<String>>()
    .join("\r\n");

    let t = "    ";
    let css_blocks = css_blocks.iter().map(|(k, v)| {

        let v = v
        .lines()
        .map(|l| format!("{}{}{}", t, t, l))
        .collect::<Vec<String>>()
        .join("\r\n");

        format!("    const {}_PROPERTIES: &[NodeDataInlineCssProperty] = &[\r\n{}\r\n{}];\r\n{}const {}: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice({}_PROPERTIES);", k, v, t, t, k, k)
    }).collect::<Vec<_>>()
    .join(&format!("{}\r\n\r\n", t));

    let mut extra_block_string = extra_blocks.format(1);

    let main_func = "

use azul::{
    app::{App, AppConfig, LayoutSolver},
    css::Css,
    style::StyledDom,
    callbacks::{RefAny, LayoutCallbackInfo},
    window::{WindowCreateOptions, WindowFrame},
};

struct Data { }

extern \"C\" fn render(_: &mut RefAny, _: LayoutCallbackInfo) -> StyledDom {
    crate::ui::render()
    .style(Css::empty()) // styles are applied inline
}

fn main() {
    let app = App::new(RefAny::new(Data { }), AppConfig::new(LayoutSolver::Default));
    let mut window = WindowCreateOptions::new(render);
    window.state.flags.frame = WindowFrame::Maximized;
    app.run(window);
}";

    let source_code = format!(
        "//! Auto-generated UI source code\r\n{}\r\n{}\r\n\r\n{}{}",
        imports,
        compile_components(compile_components_to_rust_code(component_map)?),
        format!("#[allow(unused_imports)]\r\npub mod ui {{

    pub use crate::components::*;

    use azul::css::*;
    use azul::str::String as AzString;
    use azul::vec::{{
        DomVec, IdOrClassVec, NodeDataInlineCssPropertyVec,
        StyleBackgroundSizeVec, StyleBackgroundRepeatVec,
        StyleBackgroundContentVec, StyleTransformVec,
        StyleFontFamilyVec, StyleBackgroundPositionVec,
        NormalizedLinearColorStopVec, NormalizedRadialColorStopVec,
    }};
    use azul::dom::{{
        Dom, IdOrClass,
        IdOrClass::{{Id, Class}},
        NodeDataInlineCssProperty,
    }};\r\n\r\n{}\r\n\r\n{}

    pub fn render() -> Dom {{\r\n{}\r\n    }}\r\n}}", extra_block_string, css_blocks, app_source),
        main_func,
    );

    Ok(source_code)
}

// Compile all components to source code
pub fn compile_components(
    components: BTreeMap<ComponentName, (CompiledComponent, FilteredComponentArguments, BTreeMap<String, String>)>
) -> String {

    let cs = components.iter().map(|(name, (function_body, function_args, css_blocks))| {
        let f = compile_component(name, function_args, function_body)
        .lines()
        .map(|l| format!("    {}", l))
        .collect::<Vec<String>>()
        .join("\r\n");

        // let css_blocks = ...

        format!("#[allow(unused_imports)]\r\npub mod {} {{\r\n    use azul::dom::Dom;\r\n    use azul::str::String as AzString;\r\n{}\r\n}}", name, f)
    }).collect::<Vec<String>>()
    .join("\r\n\r\n");

    let cs = cs
    .lines()
    .map(|l| format!("    {}", l))
    .collect::<Vec<String>>()
    .join("\r\n");

    if cs.is_empty() { cs } else { format!("pub mod components {{\r\n{}\r\n}}", cs)}
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
        "{}pub fn render({}{}{}) -> Dom {{\r\n{}\r\n}}",
        if should_inline { "#[inline]\r\n" } else { "" },
        // pass the text content as the first
        if component_args.accepts_text { "text: AzString" } else { "" },
        if function_args.is_empty() || !component_args.accepts_text { "" } else { ", " },
        function_args,
        component_function_body,
    )
}

pub fn render_dom_from_body_node<'a>(
    body_node: &'a XmlNode,
    mut global_css: Option<Css>,
    component_map: &'a XmlComponentMap
) -> Result<StyledDom, RenderDomError<'a>> {

    // Don't actually render the <body></body> node itself
    let mut dom = StyledDom::default();

    for child_node in body_node.children.as_ref() {
        dom.append_child(render_dom_from_body_node_inner(child_node, component_map, &FilteredComponentArguments::default())?);
    }

    if let Some(global_css) = global_css.as_mut() {
        dom.restyle(global_css); // apply the CSS again
    }

    Ok(dom)
}

/// Takes a single (expanded) app node and renders the DOM or returns an error
pub fn render_dom_from_body_node_inner<'a>(
    xml_node: &'a XmlNode,
    component_map: &'a XmlComponentMap,
    parent_xml_attributes: &FilteredComponentArguments,
) -> Result<StyledDom, RenderDomError<'a>> {

    let component_name = normalize_casing(&xml_node.node_type);

    let (renderer, inherit_variables) = component_map.components.get(&component_name)
        .ok_or(ComponentError::UnknownComponent(component_name.clone().into()))?;

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

    let text = xml_node.text.as_ref()
    .map(|t| AzString::from(format_args_dynamic(t, &filtered_xml_attributes.args)));

    let mut dom = renderer.render_dom(component_map, &filtered_xml_attributes, &text.into())?;
    set_attributes(&mut dom, &xml_node.attributes, &filtered_xml_attributes);

    for child_node in xml_node.children.as_ref() {
        dom.append_child(render_dom_from_body_node_inner(child_node, component_map, &filtered_xml_attributes)?);
    }

    Ok(dom)
}

pub fn set_attributes(dom: &mut StyledDom, xml_attributes: &XmlAttributeMap, filtered_xml_attributes: &FilteredComponentArguments) {

    use crate::dom::TabIndex;
    use crate::dom::IdOrClass::{Id, Class};

    let mut ids_and_classes = Vec::new();
    let dom_root = match dom.root.into_crate_internal() {
        Some(s) => s,
        None => return,
    };
    let node_data = &mut dom.node_data.as_container_mut()[dom_root];

    if let Some(ids) = xml_attributes.get_key("id") {
        for id in ids.split_whitespace() {
            ids_and_classes.push(Id(format_args_dynamic(id, &filtered_xml_attributes.args).into()));
        }
    }

    if let Some(classes) = xml_attributes.get_key("class") {
        for class in classes.split_whitespace() {
            ids_and_classes.push(Class(format_args_dynamic(class, &filtered_xml_attributes.args).into()));
        }
    }

    node_data.set_ids_and_classes(ids_and_classes.into());

    if let Some(focusable) = xml_attributes.get_key("focusable")
        .map(|f| format_args_dynamic(f.as_str(), &filtered_xml_attributes.args))
        .and_then(|f| parse_bool(&f))
    {
        match focusable {
            true => node_data.set_tab_index(TabIndex::Auto),
            false => node_data.set_tab_index(TabIndex::NoKeyboardFocus.into()),
        }
    }

    if let Some(tab_index) = xml_attributes.get_key("tabindex")
        .map(|val| format_args_dynamic(val, &filtered_xml_attributes.args))
        .and_then(|val| val.parse::<isize>().ok())
    {
        match tab_index {
            0 => node_data.set_tab_index(TabIndex::Auto),
            i if i > 0 => node_data.set_tab_index(TabIndex::OverrideInParent(i as u32)),
            _ => node_data.set_tab_index(TabIndex::NoKeyboardFocus),
        }
    }
}

pub fn set_stringified_attributes(
    dom_string: &mut String,
    xml_attributes: &XmlAttributeMap,
    filtered_xml_attributes: &ComponentArgumentsMap,
    tabs: usize,
) {

    let t0 = String::from("    ").repeat(tabs);
    let t = String::from("    ").repeat(tabs + 1);

    // push ids and classes
    let mut ids_and_classes = String::new();

    for id in xml_attributes.get_key("id").map(|s| s.split_whitespace().collect::<Vec<_>>()).unwrap_or_default() {
        ids_and_classes.push_str(&format!("{}    Id(AzString::from_const_str(\"{}\")),\r\n", t0, format_args_dynamic(id, &filtered_xml_attributes)));
    }

    for class in xml_attributes.get_key("class").map(|s| s.split_whitespace().collect::<Vec<_>>()).unwrap_or_default() {
        ids_and_classes.push_str(&format!("{}    Class(AzString::from_const_str(\"{}\")),\r\n", t0, format_args_dynamic(class, &filtered_xml_attributes)));
    }

    if !ids_and_classes.is_empty() {
        use crate::css::GetHash;
        let id = ids_and_classes.get_hash();
        dom_string.push_str(
            &format!("\r\n{t0}.with_ids_and_classes({{\r\n{t}const IDS_AND_CLASSES_{id}: &[IdOrClass] = &[\r\n{t}{ids_and_classes}\r\n{t}];\r\n{t}IdOrClassVec::from_const_slice(IDS_AND_CLASSES_{id})\r\n{t0}}})",
            t0=t0,
            t=t,
            ids_and_classes=ids_and_classes,
            id=id
        ));
    }

    if let Some(focusable) = xml_attributes.get_key("focusable")
        .map(|f| format_args_dynamic(f, &filtered_xml_attributes))
        .and_then(|f| parse_bool(&f))
    {
        match focusable {
            true => dom_string.push_str(&format!("\r\n{}.with_tab_index(Some(TabIndex::Auto).into())", t)),
            false => dom_string.push_str(&format!("\r\n{}.with_tab_index(Some(TabIndex::NoKeyboardFocus).into())", t)),
        }
    }

    if let Some(tab_index) = xml_attributes.get_key("tabindex")
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

pub fn render_component_inner<'a>(
    map: &mut BTreeMap<ComponentName, (CompiledComponent, FilteredComponentArguments, BTreeMap<String, String>)>,
    component_name: String,
    (renderer, inherit_variables): &'a (Box<dyn XmlComponent>, bool),
    component_map: &'a XmlComponentMap,
    parent_xml_attributes: &FilteredComponentArguments,
    tabs: usize,
) -> Result<(), CompileError<'a>> {

    let t = String::from("    ").repeat(tabs - 1);
    let t1 = String::from("    ").repeat(tabs);

    let component_name = normalize_casing(&component_name);
    let xml_node = renderer.get_xml_node();

    let mut css = match find_node_by_type(xml_node.children.as_ref(), "style")
    .and_then(|style_node| style_node.text.as_ref().map(|s| s.as_str())) {
        Some(text) => azul_css_parser::new_from_str(&text)?,
        None => Css::empty(),
    };

    css.sort_by_specificity();

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

    let text = xml_node.text.as_ref()
    .map(|t| AzString::from(format_args_dynamic(t, &filtered_xml_attributes.args)));

    let mut dom_string = renderer.compile_to_rust_code(component_map, &filtered_xml_attributes, &text.into())?;
    set_stringified_attributes(&mut dom_string, &xml_node.attributes, &filtered_xml_attributes.args, tabs);

    // TODO
    let matcher = CssMatcher {
        path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
        indices_in_parent: Vec::new(),
        children_length: Vec::new(),
    };

    let mut css_blocks = BTreeMap::new();
    let mut extra_blocks = VecContents::default();
    if !xml_node.children.as_ref().is_empty() {
        dom_string.push_str(&format!("\r\n{}.with_children(DomVec::from_vec(vec![\r\n", t));
        for (child_idx, child_node) in xml_node.children.as_ref().iter().enumerate() {

            let mut matcher = matcher.clone();
            matcher.indices_in_parent.push(child_idx);
            matcher.children_length.push(xml_node.children.as_ref().len());

            dom_string.push_str(
                &format!("{}{},", t1,
                compile_node_to_rust_code_inner(
                    child_node,
                    component_map,
                    &filtered_xml_attributes,
                    tabs + 1,
                    &mut extra_blocks,
                    &mut css_blocks,
                    &css,
                    matcher,
                )?));
        }
        dom_string.push_str(&format!("\r\n{}]))", t));
    }

    map.insert(component_name, (dom_string, filtered_xml_attributes, css_blocks));

    Ok(())
}

/// Takes all components and generates the source code function from them
pub fn compile_components_to_rust_code(
    components: &XmlComponentMap
) -> Result<BTreeMap<
    ComponentName,
    (CompiledComponent, FilteredComponentArguments, BTreeMap<String, String>)
>, CompileError> {

    let mut map = BTreeMap::new();

    for (xml_node_name, xml_component) in &components.components {
        render_component_inner(
            &mut map,
            xml_node_name.clone(),
            xml_component,
            &components,
            &FilteredComponentArguments::default(),
            1,
        )?;
    }

    Ok(map)
}

#[derive(Clone)]
pub struct CssMatcher {
    path: Vec<CssPathSelector>,
    indices_in_parent: Vec<usize>,
    children_length: Vec<usize>,
}

impl CssMatcher {
    fn get_hash(&self) -> u64 {
        use std::hash::Hash;
        use highway::{HighwayHasher, HighwayHash, Key};

        let mut hasher = HighwayHasher::new(Key([0;4]));
        for p in self.path.iter() {
            p.hash(&mut hasher);
        }
        hasher.finalize64()
    }
}

impl CssMatcher {
    fn matches(&self, path: &CssPath) -> bool {

        use azul_css::CssPathSelector::*;
        use crate::style::{CssGroupIterator, CssGroupSplitReason};

        if self.path.is_empty() { return false; }
        if path.selectors.as_ref().is_empty() { return false; }

        // self_matcher is only ever going to contain "Children" selectors, never "DirectChildren"
        let mut path_groups = CssGroupIterator::new(path.selectors.as_ref()).collect::<Vec<_>>();
        path_groups.reverse();

        if path_groups.is_empty() { return false; }
        let mut self_groups = CssGroupIterator::new(self.path.as_ref()).collect::<Vec<_>>();
        self_groups.reverse();
        if self_groups.is_empty() { return false; }

        if self.indices_in_parent.len() != self_groups.len() { return false; }
        if self.children_length.len() != self_groups.len() { return false; }

        // self_groups = [ // HTML
        //     "body",
        //     "div.__azul_native-ribbon-container"
        //     "div.__azul_native-ribbon-tabs"
        //     "p.home"
        // ]
        //
        // path_groups = [ // CSS
        //     ".__azul_native-ribbon-tabs"
        //     "div.after-tabs"
        // ]

        // get the first path group and see if it matches anywhere in the self group
        let mut cur_selfgroup_scan = 0;
        let mut cur_pathgroup_scan = 0;
        let mut valid = false;
        let mut path_group = path_groups[cur_pathgroup_scan].clone();

        while cur_selfgroup_scan < self_groups.len() {
            let mut advance = None;

            // scan all remaining path groups
            for (id, cg) in self_groups[cur_selfgroup_scan..].iter().enumerate() {

                let gm = group_matches(
                    &path_group.0,
                    &self_groups[cur_selfgroup_scan + id].0,
                    self.indices_in_parent[cur_selfgroup_scan + id],
                    self.children_length[cur_selfgroup_scan + id],
                );

                if gm {
                    // ok: ".__azul_native-ribbon-tabs" was found within self_groups
                    // advance the self_groups by n
                    advance = Some(id + 1);
                    break;
                }
            }

            match advance {
                Some(n) => {
                    // group was found in remaining items
                    // advance cur_pathgroup_scan by 1 and cur_selfgroup_scan by n
                    cur_pathgroup_scan += 1;
                    if cur_pathgroup_scan >= path_groups.len() {
                        // last group was found
                        return cur_selfgroup_scan + n >= self_groups.len();
                    } else {
                        path_group = path_groups[cur_pathgroup_scan].clone();
                    }

                    cur_selfgroup_scan += n;
                },
                None => {
                    return false;
                }, // group was not found in remaining items
            }
        }

        // only return true if all path_groups matched
        return cur_pathgroup_scan == path_groups.len() - 1;
    }
}

// does p.home match div.after-tabs?
// a: div.after-tabs
fn group_matches(
    a: &[&CssPathSelector],
    b: &[&CssPathSelector],
    idx_in_parent: usize,
    parent_children: usize
) -> bool {

    use azul_css::CssPathSelector::*;
    use azul_css::CssPathPseudoSelector;
    use azul_css::CssNthChildSelector;

    for selector in a {
        match selector {

            // always matches
            Global => { }
            PseudoSelector(CssPathPseudoSelector::Hover) => { },
            PseudoSelector(CssPathPseudoSelector::Active) => { },
            PseudoSelector(CssPathPseudoSelector::Focus) => { },

            Type(tag) => {
                if !b.iter().any(|t| **t == Type(tag.clone())) { return false; }
            },
            Class(class) => {
                if !b.iter().any(|t| **t == Class(class.clone())) { return false; }
            },
            Id(id) => {
                if !b.iter().any(|t| **t == Id(id.clone())) { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::First) => {
                if idx_in_parent != 0 { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::Last) => {
                if idx_in_parent != parent_children.saturating_sub(1) { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::NthChild(CssNthChildSelector::Number(i))) => {
                if idx_in_parent != *i as usize { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::NthChild(CssNthChildSelector::Even)) => {
                if idx_in_parent % 2 != 0 { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::NthChild(CssNthChildSelector::Odd)) => {
                if idx_in_parent % 2 == 0 { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::NthChild(CssNthChildSelector::Pattern(p))) => {
                if idx_in_parent.saturating_sub(p.offset as usize) % p.repeat as usize != 0 { return false; }
            },

            _ => return false, // can't happen
        }
    }

    true
}

struct CssBlock {
    ending: Option<CssPathPseudoSelector>,
    block: CssRuleBlock,
}

pub fn compile_body_node_to_rust_code<'a>(
    body_node: &'a XmlNode,
    component_map: &'a XmlComponentMap,
    extra_blocks: &mut VecContents,
    css_blocks: &mut BTreeMap<String, String>,
    css: &Css,
    mut matcher: CssMatcher,
) -> Result<String, CompileError<'a>> {

    use azul_css::CssDeclaration;

    let t = "";
    let t2 = "    ";
    let mut dom_string = String::from("Dom::body()");
    let node_type = CssPathSelector::Type(NodeTypeTag::Body);
    matcher.path.push(node_type);

    let ids = body_node.attributes.get_key("id").map(|s| s.split_whitespace().collect::<Vec<_>>()).unwrap_or_default();
    matcher.path.extend(ids.into_iter().map(|id| CssPathSelector::Id(id.to_string().into())));
    let classes = body_node.attributes.get_key("class").map(|s| s.split_whitespace().collect::<Vec<_>>()).unwrap_or_default();
    matcher.path.extend(classes.into_iter().map(|class| CssPathSelector::Class(class.to_string().into())));

    let matcher_hash = matcher.get_hash();
    let css_blocks_for_this_node = get_css_blocks(css, &matcher);
    if !css_blocks_for_this_node.is_empty() {

        use crate::css::format_static_css_prop;

        let css_strings = css_blocks_for_this_node.iter().map(|css_block| {

            let wrapper = match css_block.ending {
                Some(CssPathPseudoSelector::Hover) => "Hover",
                Some(CssPathPseudoSelector::Active) => "Active",
                Some(CssPathPseudoSelector::Focus) => "Focus",
                _ => "Normal",
            };

            for declaration in css_block.block.declarations.as_ref().iter() {
                let prop = match declaration {
                    CssDeclaration::Static(s) => s,
                    CssDeclaration::Dynamic(d) => &d.default_value,
                };
                extra_blocks.insert_from_css_property(prop);
            }

            let formatted = css_block.block.declarations.as_ref().iter().map(|s| match &s {
                CssDeclaration::Static(s) => format!("NodeDataInlineCssProperty::{}({})", wrapper, format_static_css_prop(s, 1)),
                CssDeclaration::Dynamic(d) => format!("NodeDataInlineCssProperty::{}({})", wrapper, format_static_css_prop(&d.default_value, 1)),
            }).collect::<Vec<String>>();

            format!("// {}\r\n{}", css_block.block.path, formatted.join(",\r\n"))
        })
        .collect::<Vec<_>>()
        .join(",\r\n");

        css_blocks.insert(format!("CSS_MATCH_{:09}", matcher_hash), css_strings);
        dom_string.push_str(&format!("\r\n{}.with_inline_css_props(CSS_MATCH_{:09})", t2, matcher_hash));
    }

    if !body_node.children.as_ref().is_empty() {
        use crate::css::GetHash;
        let children_hash = body_node.children.as_ref().get_hash();
        dom_string.push_str(&format!("\r\n.with_children(DomVec::from_vec(vec![\r\n"));

        for (child_idx, child_node) in body_node.children.as_ref().iter().enumerate() {

            let mut matcher = matcher.clone();
            matcher.path.push(CssPathSelector::Children);
            matcher.indices_in_parent.push(child_idx);
            matcher.children_length.push(body_node.children.len());

            dom_string.push_str(&format!("{}{},\r\n", t, compile_node_to_rust_code_inner(
                child_node,
                component_map,
                &FilteredComponentArguments::default(),
                1,
                extra_blocks,
                css_blocks,
                css,
                matcher,
            )?));
        }
        dom_string.push_str(&format!("\r\n{}]))", t));
    }

    let dom_string = dom_string.trim();
    Ok(dom_string.to_string())
}

fn get_css_blocks(css: &Css, matcher: &CssMatcher) -> Vec<CssBlock> {

    let mut blocks = Vec::new();

    for stylesheet in css.stylesheets.as_ref() {
        for css_block in stylesheet.rules.as_ref() {
            if matcher.matches(&css_block.path) {
                blocks.push(CssBlock {
                    ending: None, // TODO
                    block: css_block.clone(),
                });
            }
        }
    }

    blocks
}

fn compile_and_format_dynamic_items(input: &[DynamicItem]) -> String {
    use self::DynamicItem::*;
    if input.is_empty() {
        String::from("AzString::from_const_str(\"\")")
    } else if input.len() == 1 {
        // common: there is only one "dynamic item" - skip the "format!()" macro
        match &input[0] {
            Var(v) => normalize_casing(v.trim()),
            Str(s) => format!("AzString::from_const_str(\"{}\")", s),
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
        formatted_str.push_str(").into()");
        formatted_str
    }
}

fn format_args_for_rust_code(input: &str) -> String {
    let dynamic_str_items = split_dynamic_string(input);
    compile_and_format_dynamic_items(&dynamic_str_items)
}

pub fn compile_node_to_rust_code_inner<'a>(
    node: &'a XmlNode,
    component_map: &'a XmlComponentMap,
    parent_xml_attributes: &FilteredComponentArguments,
    tabs: usize,
    extra_blocks: &mut VecContents,
    css_blocks: &mut BTreeMap<String, String>,
    css: &Css,
    mut matcher: CssMatcher,
) -> Result<String, CompileError<'a>> {

    use azul_css::CssDeclaration;

    let t = String::from("    ").repeat(tabs - 1);
    let t2 = String::from("    ").repeat(tabs);

    let component_name = normalize_casing(&node.node_type);

    let (renderer, inherit_variables) = component_map.components.get(&component_name)
        .ok_or(ComponentError::UnknownComponent(component_name.clone().into()))?;

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
            match node.attributes.get_key(xml_attribute_key).cloned() {
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

    let text_as_first_arg = if filtered_xml_attributes.accepts_text {
        let node_text = node.text.clone().into_option().unwrap_or_default();
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

    let node_type = CssPathSelector::Type(match component_name.as_str() {
        "body" => NodeTypeTag::Body,
        "div" => NodeTypeTag::Div,
        "br" => NodeTypeTag::Br,
        "p" => NodeTypeTag::P,
        "img" => NodeTypeTag::Img,
        other => return Err(CompileError::Dom(RenderDomError::Component(ComponentError::UnknownComponent(other.to_string().into())))),
    });

    // The dom string is the function name
    let mut dom_string = format!("{}{}::render({}{})", t2, component_name, text_as_first_arg, instantiated_function_arguments);

    matcher.path.push(node_type);
    let ids = node.attributes.get_key("id").map(|s| s.split_whitespace().collect::<Vec<_>>()).unwrap_or_default();
    matcher.path.extend(ids.into_iter().map(|id| CssPathSelector::Id(id.to_string().into())));
    let classes = node.attributes.get_key("class").map(|s| s.split_whitespace().collect::<Vec<_>>()).unwrap_or_default();
    matcher.path.extend(classes.into_iter().map(|class| CssPathSelector::Class(class.to_string().into())));

    let matcher_hash = matcher.get_hash();
    let css_blocks_for_this_node = get_css_blocks(css, &matcher);
    if !css_blocks_for_this_node.is_empty() {
        use crate::css::format_static_css_prop;

        let css_strings = css_blocks_for_this_node.iter().map(|css_block| {

            let wrapper = match css_block.ending {
                Some(CssPathPseudoSelector::Hover) => "Hover",
                Some(CssPathPseudoSelector::Active) => "Active",
                Some(CssPathPseudoSelector::Focus) => "Focus",
                _ => "Normal",
            };

            for declaration in css_block.block.declarations.as_ref().iter() {
                let prop = match declaration {
                    CssDeclaration::Static(s) => s,
                    CssDeclaration::Dynamic(d) => &d.default_value,
                };
                extra_blocks.insert_from_css_property(prop);
            }

            let formatted = css_block.block.declarations.as_ref().iter().map(|s| match &s {
                CssDeclaration::Static(s) => format!("NodeDataInlineCssProperty::{}({})", wrapper, format_static_css_prop(s, 1)),
                CssDeclaration::Dynamic(d) => format!("NodeDataInlineCssProperty::{}({})", wrapper, format_static_css_prop(&d.default_value, 1)),
            }).collect::<Vec<String>>();

            format!("// {}\r\n{}", css_block.block.path, formatted.join(",\r\n"))
        })
        .collect::<Vec<_>>()
        .join(",\r\n");

        css_blocks.insert(format!("CSS_MATCH_{:09}", matcher_hash), css_strings);
        dom_string.push_str(&format!("\r\n{}.with_inline_css_props(CSS_MATCH_{:09})", t2, matcher_hash));
    }

    set_stringified_attributes(&mut dom_string, &node.attributes, &filtered_xml_attributes.args, tabs);

    let mut children_string = node.children.as_ref()
    .iter()
    .enumerate()
    .map(|(child_idx, c)| {

        let mut matcher = matcher.clone();
        matcher.path.push(CssPathSelector::Children);
        matcher.indices_in_parent.push(child_idx);
        matcher.children_length.push(node.children.len());

        compile_node_to_rust_code_inner(
            c, component_map,
            &filtered_xml_attributes, tabs + 1,
            extra_blocks, css_blocks, css, matcher
        )
    })
    .collect::<Result<Vec<_>, _>>()?
    .join(&format!(",\r\n"));

    if !children_string.is_empty() {
        dom_string.push_str(&format!("\r\n{}.with_children(DomVec::from_vec(vec![\r\n{}\r\n{}]))", t2, children_string, t2));
    }

    Ok(dom_string)
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
    pub fn new<'a>(root: &'a XmlNode) -> Result<Self, ComponentParseError<'a>> {

        let node_type = normalize_casing(&root.node_type);

        if node_type.as_str() != "component" {
            return Err(ComponentParseError::NotAComponent);
        }

        let name = root.attributes.get_key("name").cloned().ok_or(ComponentParseError::NotAComponent)?;
        let accepts_text = root.attributes.get_key("accepts_text").and_then(|p| parse_bool(p.as_str())).unwrap_or(false);

        let args = match root.attributes.get_key("args") {
            Some(s) => parse_component_arguments(s)?,
            None => ComponentArgumentsMap::default(),
        };

        Ok(Self {
            name: normalize_casing(&name),
            arguments: ComponentArguments {
                args,
                accepts_text,
            },
            root: root.clone(),
        })
    }
}

impl XmlComponent for DynamicXmlComponent {

    fn get_available_arguments(&self) -> ComponentArguments {
        self.arguments.clone()
    }

    fn get_xml_node<'a>(&'a self) -> &'a XmlNode {
        &self.root
    }

    fn render_dom<'a>(
        &'a self,
        components: &'a XmlComponentMap,
        arguments: &FilteredComponentArguments,
        content: &XmlTextContent,
    ) -> Result<StyledDom, RenderDomError<'a>> {

        let mut component_css = match find_node_by_type(self.root.children.as_ref(), "style") {
            Some(style_node) => {
                if let Some(text) = style_node.text.as_ref().map(|s| s.as_str()) {
                    let parsed_css = azul_css_parser::new_from_str(&text)?;
                    Some(parsed_css)
                } else {
                    None
                }
            },
            None => None,
        };

        let mut dom = StyledDom::default();

        for child_node in self.root.children.as_ref() {
            dom.append_child(render_dom_from_body_node_inner(child_node, components, arguments)?);
        }

        if let Some(css) = component_css.as_mut() {
            dom.restyle(css);
        }

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

// -- Tests
#[cfg(test)] mod tests {

    use super::*;

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
            fn test() -> StyledDom {
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
}