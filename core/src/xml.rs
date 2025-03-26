//! XML structure definitions

use alloc::{
    boxed::Box,
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use core::{fmt, hash::Hash};

use azul_css::{
    parser::{CssApiWrapper, CssParseErrorOwned, ErrorLocation},
    AzString, Css, CssDeclaration, CssPath, CssPathPseudoSelector, CssPathSelector, CssProperty,
    CssRuleBlock, NodeTypeTag, NormalizedLinearColorStopVec, NormalizedRadialColorStopVec,
    OptionAzString, StyleBackgroundContentVec, StyleBackgroundPositionVec,
    StyleBackgroundRepeatVec, StyleBackgroundSizeVec, StyleFontFamilyVec, StyleTransformVec, U8Vec,
};

use crate::{
    css::VecContents,
    dom::Dom,
    styled_dom::StyledDom,
    window::{AzStringPair, StringPairVec},
};

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
pub type ComponentArgumentTypes = Vec<(ComponentArgumentName, ComponentArgumentType)>;
pub type ComponentName = String;
pub type CompiledComponent = String;

pub const DEFAULT_ARGS: [&str; 8] = [
    "id",
    "class",
    "tabindex",
    "focusable",
    "accepts_text",
    "name",
    "style",
    "args",
];

#[allow(non_camel_case_types)]
pub enum c_void {}

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
            NonXmlChar(nx) => write!(
                f,
                "Non-XML character: {:?} at {}",
                core::char::from_u32(nx.ch),
                nx.pos
            ),
            InvalidChar(ic) => write!(
                f,
                "Invalid character: expected: {}, got: {} at {}",
                ic.expected as char, ic.got as char, ic.pos
            ),
            InvalidCharMultiple(imc) => write!(
                f,
                "Multiple invalid characters: expected: {}, got: {:?} at {}",
                imc.expected,
                imc.got.as_ref(),
                imc.pos
            ),
            InvalidQuote(iq) => write!(f, "Invalid quote: got {} at {}", iq.got as char, iq.pos),
            InvalidSpace(is) => write!(f, "Invalid space: got {} at {}", is.got as char, is.pos),
            InvalidString(ise) => write!(
                f,
                "Invalid string: got \"{}\" at {}",
                ise.got.as_str(),
                ise.pos
            ),
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
    pub pos: XmlTextPos,
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
            InvalidDeclaration(e) => {
                write!(f, "Invalid declaraction: {} at {}", e.stream_error, e.pos)
            }
            InvalidComment(e) => write!(f, "Invalid comment: {} at {}", e.stream_error, e.pos),
            InvalidPI(e) => write!(
                f,
                "Invalid processing instruction: {} at {}",
                e.stream_error, e.pos
            ),
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

impl_result!(
    Xml,
    XmlError,
    ResultXmlXmlError,
    copy = false,
    [Debug, PartialEq, PartialOrd, Clone]
);

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
            NoParserAvailable => write!(
                f,
                "Library was compiled without XML parser (XML parser not available)"
            ),
            InvalidXmlPrefixUri(pos) => {
                write!(f, "Invalid XML Prefix URI at line {}:{}", pos.row, pos.col)
            }
            UnexpectedXmlUri(pos) => {
                write!(f, "Unexpected XML URI at at line {}:{}", pos.row, pos.col)
            }
            UnexpectedXmlnsUri(pos) => write!(
                f,
                "Unexpected XML namespace URI at line {}:{}",
                pos.row, pos.col
            ),
            InvalidElementNamePrefix(pos) => write!(
                f,
                "Invalid element name prefix at line {}:{}",
                pos.row, pos.col
            ),
            DuplicatedNamespace(ns) => write!(
                f,
                "Duplicated namespace: \"{}\" at {}",
                ns.ns.as_str(),
                ns.pos
            ),
            UnknownNamespace(uns) => write!(
                f,
                "Unknown namespace: \"{}\" at {}",
                uns.ns.as_str(),
                uns.pos
            ),
            UnexpectedCloseTag(ct) => write!(
                f,
                "Unexpected close tag: expected \"{}\", got \"{}\" at {}",
                ct.expected.as_str(),
                ct.actual.as_str(),
                ct.pos
            ),
            UnexpectedEntityCloseTag(pos) => write!(
                f,
                "Unexpected entity close tag at line {}:{}",
                pos.row, pos.col
            ),
            UnknownEntityReference(uer) => write!(
                f,
                "Unexpected entity reference: \"{}\" at {}",
                uer.entity, uer.pos
            ),
            MalformedEntityReference(pos) => write!(
                f,
                "Malformed entity reference at line {}:{}",
                pos.row, pos.col
            ),
            EntityReferenceLoop(pos) => write!(
                f,
                "Entity reference loop (recursive entity reference) at line {}:{}",
                pos.row, pos.col
            ),
            InvalidAttributeValue(pos) => {
                write!(f, "Invalid attribute value at line {}:{}", pos.row, pos.col)
            }
            DuplicatedAttribute(ae) => write!(
                f,
                "Duplicated attribute \"{}\" at line {}:{}",
                ae.attribute.as_str(),
                ae.pos.row,
                ae.pos.col
            ),
            NoRootNode => write!(f, "No root node found"),
            SizeLimit => write!(f, "XML file too large (size limit reached)"),
            DtdDetected => write!(f, "Document type descriptor detected"),
            MalformedHierarchy(expected, got) => write!(
                f,
                "Malformed hierarchy: expected <{}/> closing tag, got <{}/>",
                expected.as_str(),
                got.as_str()
            ),
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
    pub args: ComponentArgumentTypes,
    /// Whether this widget accepts text. Note that this will be passed as the first
    /// argument when rendering the Rust code.
    pub accepts_text: bool,
}

impl Default for ComponentArguments {
    fn default() -> Self {
        Self {
            args: ComponentArgumentTypes::default(),
            accepts_text: false,
        }
    }
}

impl ComponentArguments {
    fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FilteredComponentArguments {
    /// The types of the component, i.e. `date => String`, in order
    pub types: ComponentArgumentTypes,
    /// The types of the component, i.e. `date => "01.01.1998"`
    pub values: BTreeMap<String, String>,
    /// Whether this widget accepts text. Note that this will be passed as the first
    /// argument when rendering the Rust code.
    pub accepts_text: bool,
}

impl Default for FilteredComponentArguments {
    fn default() -> Self {
        Self {
            types: Vec::new(),
            values: BTreeMap::default(),
            accepts_text: false,
        }
    }
}

impl FilteredComponentArguments {
    fn new() -> Self {
        Self::default()
    }
}

/// Specifies a component that reacts to a parsed XML node
pub trait XmlComponentTrait {
    /// Returns the type ID of this component, default = `div`
    fn get_type_id(&self) -> String {
        "div".to_string()
    }

    /// Returns the XML node for this component, used in the `get_html_string` debugging code
    /// (necessary to compile the component into a function during the Rust compilation stage)
    fn get_xml_node(&self) -> XmlNode {
        XmlNode::new(self.get_type_id())
    }

    /// (Optional): Should return all arguments that this component can take - for example if you
    /// have a component called `Calendar`, which can take a `selectedDate` argument:
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
    /// impl XmlComponentTrait for CalendarRenderer {
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
    /// If a user instantiates a component with an invalid argument (i.e. `<Calendar
    /// asdf="false">`), the user will get an error that the component can't handle this
    /// argument. The types are not checked, but they are necessary for the XML-to-Rust
    /// compiler.
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
    /// `Type::from` to make the conversion. You can then take that generated Rust code and clean it
    /// up, put it somewhere else and create another component out of it - XML should only be
    /// seen as a high-level prototyping tool (to get around the problem of compile times), not
    /// as the final data format.
    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments::new()
    }

    // - necessary functions

    /// Given a root node and a list of possible arguments, returns a DOM or a syntax error
    fn render_dom(
        &self,
        components: &XmlComponentMap,
        arguments: &FilteredComponentArguments,
        content: &XmlTextContent,
    ) -> Result<StyledDom, RenderDomError>;

    /// (Optional): Used to compile the XML component to Rust code - input
    fn compile_to_rust_code(
        &self,
        components: &XmlComponentMap,
        attributes: &ComponentArguments,
        content: &XmlTextContent,
    ) -> Result<String, CompileError> {
        Ok(String::new())
    }
}

/// Wrapper for the XML parser - necessary to easily create a Dom from
/// XML without putting an XML solver into `azul-core`.
#[derive(Default)]
pub struct DomXml {
    pub parsed_dom: StyledDom,
}

impl DomXml {
    /// Convenience function, only available in tests, useful for quickly writing UI tests.
    /// Wraps the XML string in the required `<app></app>` braces, panics if the XML couldn't be
    /// parsed.
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
        let mut fixed = Dom::body().style(CssApiWrapper::empty());
        fixed.append_child(other);
        if self.parsed_dom != fixed {
            panic!(
                "\r\nExpected DOM did not match:\r\n\r\nexpected: ----------\r\n{}\r\ngot: \
                 ----------\r\n{}\r\n",
                self.parsed_dom.get_html_string("", "", true),
                fixed.get_html_string("", "", true)
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
        XmlNode {
            node_type: node_type.into(),
            ..Default::default()
        }
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

pub struct XmlComponent {
    pub id: String,
    /// DOM rendering component (boxed trait)
    pub renderer: Box<dyn XmlComponentTrait>,
    /// Whether this component should inherit variables from the parent scope
    pub inherit_vars: bool,
}

impl core::fmt::Debug for XmlComponent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XmlComponent")
            .field("id", &self.id)
            .field("args", &self.renderer.get_available_arguments())
            .field("inherit_vars", &self.inherit_vars)
            .finish()
    }
}

/// Holds all XML components - builtin components
pub struct XmlComponentMap {
    /// Stores all known components that can be used during DOM rendering
    /// + whether this component should inherit variables from the parent scope
    pub components: Vec<XmlComponent>,
}

impl Default for XmlComponentMap {
    fn default() -> Self {
        let mut map = Self {
            components: Vec::new(),
        };
        map.register_component(XmlComponent {
            id: normalize_casing("body"),
            renderer: Box::new(BodyRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("div"),
            renderer: Box::new(DivRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("p"),
            renderer: Box::new(TextRenderer::new()),
            inherit_vars: true,
        });
        map
    }
}

impl XmlComponentMap {
    pub fn register_component(&mut self, comp: XmlComponent) {
        self.components.push(comp);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DomXmlParseError {
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
    /// A component raised an error while rendering the DOM - holds the component name + error
    /// string
    RenderDom(RenderDomError),
    /// Something went wrong while parsing an XML component
    Component(ComponentParseError),
    /// Error parsing global CSS in head node
    Css(CssParseErrorOwned),
}

impl From<XmlError> for DomXmlParseError {
    fn from(e: XmlError) -> Self {
        Self::Xml(e)
    }
}

impl From<ComponentParseError> for DomXmlParseError {
    fn from(e: ComponentParseError) -> Self {
        Self::Component(e)
    }
}

impl From<RenderDomError> for DomXmlParseError {
    fn from(e: RenderDomError) -> Self {
        Self::RenderDom(e)
    }
}

impl From<CssParseErrorOwned> for DomXmlParseError {
    fn from(e: CssParseErrorOwned) -> Self {
        Self::Css(e)
    }
}

/// Error that can happen from the translation from XML code to Rust code -
/// stringified, since it is only used for printing and is not exposed in the public API
#[derive(Debug, Clone, PartialEq)]
pub enum CompileError {
    Dom(RenderDomError),
    Xml(DomXmlParseError),
    Css(CssParseErrorOwned),
}

impl From<ComponentError> for CompileError {
    fn from(e: ComponentError) -> Self {
        CompileError::Dom(RenderDomError::Component(e))
    }
}

impl From<CssParseErrorOwned> for CompileError {
    fn from(e: CssParseErrorOwned) -> Self {
        CompileError::Css(e)
    }
}

impl<'a> fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::CompileError::*;
        match self {
            Dom(d) => write!(f, "{}", d),
            Xml(s) => write!(f, "{}", s),
            Css(s) => write!(f, "{}", s.to_shared()),
        }
    }
}

impl From<RenderDomError> for CompileError {
    fn from(e: RenderDomError) -> Self {
        CompileError::Dom(e)
    }
}

impl From<DomXmlParseError> for CompileError {
    fn from(e: DomXmlParseError) -> Self {
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
pub enum RenderDomError {
    Component(ComponentError),
    /// Error parsing the CSS on the component style
    CssError(CssParseErrorOwned),
}

impl From<ComponentError> for RenderDomError {
    fn from(e: ComponentError) -> Self {
        Self::Component(e)
    }
}

impl From<CssParseErrorOwned> for RenderDomError {
    fn from(e: CssParseErrorOwned) -> Self {
        Self::CssError(e)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComponentParseError {
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
    CssError(CssParseErrorOwned),
}

impl<'a> fmt::Display for DomXmlParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::DomXmlParseError::*;
        match self {
            NoHtmlNode => write!(
                f,
                "No <html> node found as the root of the file - empty file?"
            ),
            MultipleHtmlRootNodes => write!(
                f,
                "Multiple <html> nodes found as the root of the file - only one root node allowed"
            ),
            NoBodyInHtml => write!(
                f,
                "No <body> node found as a direct child of an <html> node - malformed DOM \
                 hierarchy?"
            ),
            MultipleBodyNodes => write!(
                f,
                "Multiple <body> nodes present, only one <body> node is allowed"
            ),
            Xml(e) => write!(f, "Error parsing XML: {}", e),
            MalformedHierarchy(got, expected) => write!(
                f,
                "Invalid </{}> tag: expected </{}>",
                got.as_str(),
                expected.as_str()
            ),
            RenderDom(e) => write!(f, "Error rendering DOM: {}", e),
            Component(c) => write!(f, "Error parsing component in <head> node:\r\n{}", c),
            Css(c) => write!(f, "Error parsing CSS in <head> node:\r\n{}", c.to_shared()),
        }
    }
}

impl<'a> fmt::Display for ComponentParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ComponentParseError::*;
        match self {
            NotAComponent => write!(f, "Expected <component/> node, found no such node"),
            UnnamedComponent => write!(
                f,
                "Found <component/> tag with out a \"name\" attribute, component must have a name"
            ),
            MissingName(arg_pos) => write!(
                f,
                "Argument at position {} is either empty or has no name",
                arg_pos
            ),
            MissingType(arg_pos, arg_name) => write!(
                f,
                "Argument \"{}\" at position {} doesn't have a `: type`",
                arg_pos, arg_name
            ),
            WhiteSpaceInComponentName(arg_pos, arg_name_unparsed) => {
                write!(
                    f,
                    "Missing `:` between the name and the type in argument {} (around \"{}\")",
                    arg_pos, arg_name_unparsed
                )
            }
            WhiteSpaceInComponentType(arg_pos, arg_name, arg_type_unparsed) => {
                write!(
                    f,
                    "Missing `,` between two arguments (in argument {}, position {}, around \
                     \"{}\")",
                    arg_name, arg_pos, arg_type_unparsed
                )
            }
            CssError(lsf) => write!(f, "Error parsing <style> tag: {}", lsf.to_shared()),
        }
    }
}

impl fmt::Display for ComponentError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ComponentError::*;
        match self {
            UselessFunctionArgument(k, v, available_args) => {
                write!(
                    f,
                    "Useless component argument \"{}\": \"{}\" - available args are: {:#?}",
                    k, v, available_args
                )
            }
            UnknownComponent(name) => write!(f, "Unknown component: \"{}\"", name),
        }
    }
}

impl<'a> fmt::Display for RenderDomError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::RenderDomError::*;
        match self {
            Component(c) => write!(f, "{}", c),
            CssError(e) => write!(f, "Error parsing CSS in component: {}", e.to_shared()),
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
        Self {
            node: XmlNode::new("div"),
        }
    }
}

impl XmlComponentTrait for DivRenderer {
    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments::new()
    }

    fn render_dom(
        &self,
        _: &XmlComponentMap,
        _: &FilteredComponentArguments,
        _: &XmlTextContent,
    ) -> Result<StyledDom, RenderDomError> {
        Ok(Dom::div().style(CssApiWrapper::empty()))
    }

    fn compile_to_rust_code(
        &self,
        _: &XmlComponentMap,
        _: &ComponentArguments,
        _: &XmlTextContent,
    ) -> Result<String, CompileError> {
        Ok("Dom::div()".into())
    }

    fn get_xml_node(&self) -> XmlNode {
        self.node.clone()
    }
}

/// Render for a `body` component
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BodyRenderer {
    node: XmlNode,
}

impl BodyRenderer {
    pub fn new() -> Self {
        Self {
            node: XmlNode::new("body"),
        }
    }
}

impl XmlComponentTrait for BodyRenderer {
    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments::new()
    }

    fn render_dom(
        &self,
        _: &XmlComponentMap,
        _: &FilteredComponentArguments,
        _: &XmlTextContent,
    ) -> Result<StyledDom, RenderDomError> {
        Ok(Dom::body().style(CssApiWrapper::empty()))
    }

    fn compile_to_rust_code(
        &self,
        _: &XmlComponentMap,
        _: &ComponentArguments,
        _: &XmlTextContent,
    ) -> Result<String, CompileError> {
        Ok("Dom::body()".into())
    }

    fn get_xml_node(&self) -> XmlNode {
        self.node.clone()
    }
}

/// Render for a `p` component
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextRenderer {
    node: XmlNode,
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            node: XmlNode::new("p"),
        }
    }
}

impl XmlComponentTrait for TextRenderer {
    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments {
            args: ComponentArgumentTypes::default(),
            accepts_text: true, // important!
        }
    }

    fn render_dom(
        &self,
        _: &XmlComponentMap,
        _: &FilteredComponentArguments,
        content: &XmlTextContent,
    ) -> Result<StyledDom, RenderDomError> {
        let content = content
            .as_ref()
            .map(|s| prepare_string(&s))
            .unwrap_or_default();
        Ok(Dom::text(content).style(CssApiWrapper::empty()))
    }

    fn compile_to_rust_code(
        &self,
        _: &XmlComponentMap,
        args: &ComponentArguments,
        content: &XmlTextContent,
    ) -> Result<String, CompileError> {
        Ok(String::from("Dom::text(text)"))
    }

    fn get_xml_node(&self) -> XmlNode {
        self.node.clone()
    }
}

/// Compiles a XML `args="a: String, b: bool"` into a `["a" => "String", "b" => "bool"]` map
pub fn parse_component_arguments<'a>(
    input: &'a str,
) -> Result<ComponentArgumentTypes, ComponentParseError> {
    use self::ComponentParseError::*;

    let mut args = ComponentArgumentTypes::default();

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

        let arg_type = colon_iterator
            .next()
            .ok_or(MissingType(arg_idx, arg_name.into()))?;
        let arg_type = arg_type.trim();

        if arg_type.is_empty() {
            return Err(MissingType(arg_idx, arg_name.into()));
        }

        if arg_type.chars().any(char::is_whitespace) {
            return Err(WhiteSpaceInComponentType(
                arg_idx,
                arg_name.into(),
                arg_type.into(),
            ));
        }

        let arg_name = normalize_casing(arg_name);
        let arg_type = arg_type.to_string();

        args.push((arg_name, arg_type));
    }

    Ok(args)
}

/// Filters the XML attributes of a component given XmlAttributeMap
pub fn validate_and_filter_component_args(
    xml_attributes: &XmlAttributeMap,
    valid_args: &ComponentArguments,
) -> Result<FilteredComponentArguments, ComponentError> {
    let mut map = FilteredComponentArguments {
        types: ComponentArgumentTypes::default(),
        values: BTreeMap::new(),
        accepts_text: valid_args.accepts_text,
    };

    for AzStringPair { key, value } in xml_attributes.as_ref().iter() {
        let xml_attribute_name = key;
        let xml_attribute_value = value;
        if let Some(valid_arg_type) = valid_args
            .args
            .iter()
            .find(|s| s.0 == xml_attribute_name.as_str())
            .map(|q| &q.1)
        {
            map.types.push((
                xml_attribute_name.as_str().to_string(),
                valid_arg_type.clone(),
            ));
            map.values.insert(
                xml_attribute_name.as_str().to_string(),
                xml_attribute_value.as_str().to_string(),
            );
        } else if DEFAULT_ARGS.contains(&xml_attribute_name.as_str()) {
            // no error, but don't insert the attribute name
            map.values.insert(
                xml_attribute_name.as_str().to_string(),
                xml_attribute_value.as_str().to_string(),
            );
        } else {
            // key was not expected for this component
            let keys = valid_args.args.iter().map(|s| s.0.clone()).collect();
            return Err(ComponentError::UselessFunctionArgument(
                xml_attribute_name.clone(),
                xml_attribute_value.clone(),
                keys,
            ));
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

    let html_node = html_node_iterator
        .next()
        .ok_or(DomXmlParseError::NoHtmlNode)?;
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

    let body_node = body_node_iterator
        .next()
        .ok_or(DomXmlParseError::NoBodyInHtml)?;
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
    root_nodes
        .iter()
        .find(|n| normalize_casing(&n.node_type).as_str() == node_type)
}

pub fn find_attribute<'a>(node: &'a XmlNode, attribute: &str) -> Option<&'a AzString> {
    node.attributes
        .iter()
        .find(|n| normalize_casing(&n.key.as_str()).as_str() == attribute)
        .map(|s| &s.value)
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
#[allow(trivial_casts)]
pub fn get_item<'a>(hierarchy: &[usize], root_node: &'a mut XmlNode) -> Option<&'a mut XmlNode> {
    let mut hierarchy = hierarchy.to_vec();
    hierarchy.reverse();
    let item = match hierarchy.pop() {
        Some(s) => s,
        None => return Some(root_node),
    };
    let node = root_node.children.as_mut().get_mut(item)?;
    get_item_internal(&mut hierarchy, node)
}

fn get_item_internal<'a>(
    hierarchy: &mut Vec<usize>,
    root_node: &'a mut XmlNode,
) -> Option<&'a mut XmlNode> {
    if hierarchy.is_empty() {
        return Some(root_node);
    }
    let cur_item = match hierarchy.pop() {
        Some(s) => s,
        None => return Some(root_node),
    };
    let node = root_node.children.as_mut().get_mut(cur_item)?;
    get_item_internal(hierarchy, node)
}

/// Parses an XML string and returns a `StyledDom` with the components instantiated in the
/// `<app></app>`
pub fn str_to_dom<'a>(
    root_nodes: &'a [XmlNode],
    component_map: &'a mut XmlComponentMap,
    max_width: Option<f32>,
) -> Result<StyledDom, DomXmlParseError> {
    let html_node = get_html_node(root_nodes)?;
    let body_node = get_body_node(html_node.children.as_ref())?;

    let mut global_style = None;

    if let Some(head_node) = find_node_by_type(html_node.children.as_ref(), "head") {
        println!("head node present!");

        // parse all dynamic XML components from the head node
        for node in head_node.children.as_ref() {
            match DynamicXmlComponent::new(node) {
                Ok(node) => {
                    let node_name = node.name.clone();
                    component_map.register_component(XmlComponent {
                        id: normalize_casing(&node_name),
                        renderer: Box::new(node),
                        inherit_vars: false,
                    });
                }
                Err(ComponentParseError::NotAComponent) => {} // not a <component /> node, ignore
                Err(e) => return Err(e.into()),               /* Error during parsing the XML
                                                                * component, bail */
            }
        }

        // parse the <style></style> tag contents, if present
        if let Some(style_node) = find_node_by_type(head_node.children.as_ref(), "style") {
            if let Some(text) = style_node.text.as_ref().map(|s| s.as_str()) {
                println!("found css: {text}");
                let parsed_css = CssApiWrapper::from_string(text.clone().into());
                global_style = Some(parsed_css);
            }
        }
    }

    println!("rendering body node");

    render_dom_from_body_node(&body_node, global_style, component_map, max_width)
        .map_err(|e| e.into())
}

/// Parses an XML string and returns a `String`, which contains the Rust source code
/// (i.e. it compiles the XML to valid Rust)
pub fn str_to_rust_code<'a>(
    root_nodes: &'a [XmlNode],
    imports: &str,
    component_map: &'a mut XmlComponentMap,
) -> Result<String, CompileError> {
    let html_node = get_html_node(&root_nodes)?;
    let body_node = get_body_node(html_node.children.as_ref())?;
    let mut global_style = Css::empty();

    if let Some(head_node) = html_node
        .children
        .as_ref()
        .iter()
        .find(|n| normalize_casing(&n.node_type).as_str() == "head")
    {
        for node in head_node.children.as_ref() {
            match DynamicXmlComponent::new(node) {
                Ok(node) => {
                    let node_name = node.name.clone();
                    component_map.register_component(XmlComponent {
                        id: normalize_casing(&node_name),
                        renderer: Box::new(node),
                        inherit_vars: false,
                    });
                }
                Err(ComponentParseError::NotAComponent) => {} // not a <component /> node, ignore
                Err(e) => return Err(CompileError::Xml(e.into())), /* Error during parsing the XML
                                                                * component, bail */
            }
        }

        if let Some(style_node) = find_node_by_type(head_node.children.as_ref(), "style") {
            if let Some(text) = style_node.text.as_ref().map(|s| s.as_str()) {
                let parsed_css = azul_css::parser::new_from_str(&text).0;
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
        },
    )?;

    let app_source = app_source
        .lines()
        .map(|l| format!("        {}", l))
        .collect::<Vec<String>>()
        .join("\r\n");

    let t = "    ";
    let css_blocks = css_blocks
        .iter()
        .map(|(k, v)| {
            let v = v
                .lines()
                .map(|l| format!("{}{}{}", t, t, l))
                .collect::<Vec<String>>()
                .join("\r\n");

            format!(
                "    const {}_PROPERTIES: &[NodeDataInlineCssProperty] = \
                 &[\r\n{}\r\n{}];\r\n{}const {}: NodeDataInlineCssPropertyVec = \
                 NodeDataInlineCssPropertyVec::from_const_slice({}_PROPERTIES);",
                k, v, t, t, k, k
            )
        })
        .collect::<Vec<_>>()
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

extern \"C\" fn render(_: &mut RefAny, _: &mut LayoutCallbackInfo) -> StyledDom {
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
        "#![windows_subsystem = \"windows\"]\r\n//! Auto-generated UI source \
         code\r\n{}\r\n{}\r\n\r\n{}{}",
        imports,
        compile_components(compile_components_to_rust_code(component_map)?),
        format!(
            "#[allow(unused_imports)]\r\npub mod ui {{

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
        Dom, IdOrClass, TabIndex,
        IdOrClass::{{Id, Class}},
        NodeDataInlineCssProperty,
    }};\r\n\r\n{}\r\n\r\n{}

    pub fn render() -> Dom {{\r\n{}\r\n    }}\r\n}}",
            extra_block_string, css_blocks, app_source
        ),
        main_func,
    );

    Ok(source_code)
}

// Compile all components to source code
pub fn compile_components(
    components: Vec<(
        ComponentName,
        CompiledComponent,
        ComponentArguments,
        BTreeMap<String, String>,
    )>,
) -> String {
    let cs = components
        .iter()
        .map(|(name, function_body, function_args, css_blocks)| {
            let name = &normalize_casing(&name);
            let f = compile_component(name, function_args, function_body)
                .lines()
                .map(|l| format!("    {}", l))
                .collect::<Vec<String>>()
                .join("\r\n");

            // let css_blocks = ...

            format!(
                "#[allow(unused_imports)]\r\npub mod {} {{\r\n    use azul::dom::Dom;\r\n    use \
                 azul::str::String as AzString;\r\n{}\r\n}}",
                name, f
            )
        })
        .collect::<Vec<String>>()
        .join("\r\n\r\n");

    let cs = cs
        .lines()
        .map(|l| format!("    {}", l))
        .collect::<Vec<String>>()
        .join("\r\n");

    if cs.is_empty() {
        cs
    } else {
        format!("pub mod components {{\r\n{}\r\n}}", cs)
    }
}

pub fn format_component_args(component_args: &ComponentArgumentTypes) -> String {
    let mut args = component_args
        .iter()
        .map(|(arg_name, arg_type)| format!("{}: {}", arg_name, arg_type))
        .collect::<Vec<String>>();

    args.sort_by(|a, b| b.cmp(&a));

    args.join(", ")
}

pub fn compile_component(
    component_name: &str,
    component_args: &ComponentArguments,
    component_function_body: &str,
) -> String {
    let component_name = &normalize_casing(&component_name);
    let function_args = format_component_args(&component_args.args);
    let component_function_body = component_function_body
        .lines()
        .map(|l| format!("    {}", l))
        .collect::<Vec<String>>()
        .join("\r\n");
    let should_inline = component_function_body.lines().count() == 1;
    format!(
        "{}pub fn render({}{}{}) -> Dom {{\r\n{}\r\n}}",
        if should_inline { "#[inline]\r\n" } else { "" },
        // pass the text content as the first
        if component_args.accepts_text {
            "text: AzString"
        } else {
            ""
        },
        if function_args.is_empty() || !component_args.accepts_text {
            ""
        } else {
            ", "
        },
        function_args,
        component_function_body,
    )
}

pub fn render_dom_from_body_node<'a>(
    body_node: &'a XmlNode,
    mut global_css: Option<CssApiWrapper>,
    component_map: &'a XmlComponentMap,
    max_width: Option<f32>,
) -> Result<StyledDom, RenderDomError> {
    // Don't actually render the <body></body> node itself
    let mut dom = StyledDom::default();

    if let Some(max_width) = max_width {
        dom.restyle(CssApiWrapper::from_string(
            format!("body, html {{ max-width: {max_width}px; }}").into(),
        ));
    }

    for child_node in body_node.children.as_ref() {
        dom.append_child(render_dom_from_body_node_inner(
            child_node,
            component_map,
            &FilteredComponentArguments::default(),
        )?);
    }

    if let Some(global_css) = global_css.clone() {
        dom.restyle(global_css); // apply the CSS again
    }

    Ok(dom)
}

/// Takes a single (expanded) app node and renders the DOM or returns an error
pub fn render_dom_from_body_node_inner<'a>(
    xml_node: &'a XmlNode,
    component_map: &'a XmlComponentMap,
    parent_xml_attributes: &FilteredComponentArguments,
) -> Result<StyledDom, RenderDomError> {
    let component_name = normalize_casing(&xml_node.node_type);

    let xml_component = component_map
        .components
        .iter()
        .find(|s| normalize_casing(&s.id) == component_name)
        .ok_or(ComponentError::UnknownComponent(
            component_name.clone().into(),
        ))?;

    // Arguments of the current node
    let available_function_args = xml_component.renderer.get_available_arguments();
    let mut filtered_xml_attributes =
        validate_and_filter_component_args(&xml_node.attributes, &available_function_args)?;

    if xml_component.inherit_vars {
        // Append all variables that are in scope for the parent node
        filtered_xml_attributes
            .types
            .extend(parent_xml_attributes.types.clone().into_iter());
    }

    // Instantiate the parent arguments in the current child arguments
    for v in filtered_xml_attributes.types.iter_mut() {
        v.1 = format_args_dynamic(&v.1, &parent_xml_attributes.types).to_string();
    }

    let text = xml_node
        .text
        .as_ref()
        .map(|t| AzString::from(format_args_dynamic(t, &filtered_xml_attributes.types)));

    let mut dom =
        xml_component
            .renderer
            .render_dom(component_map, &filtered_xml_attributes, &text.into())?;
    set_attributes(&mut dom, &xml_node.attributes, &filtered_xml_attributes);

    for child_node in xml_node.children.as_ref() {
        dom.append_child(render_dom_from_body_node_inner(
            child_node,
            component_map,
            &filtered_xml_attributes,
        )?);
    }

    Ok(dom)
}

pub fn set_attributes(
    dom: &mut StyledDom,
    xml_attributes: &XmlAttributeMap,
    filtered_xml_attributes: &FilteredComponentArguments,
) {
    use crate::dom::{
        IdOrClass::{Class, Id},
        TabIndex,
    };

    let mut ids_and_classes = Vec::new();
    let dom_root = match dom.root.into_crate_internal() {
        Some(s) => s,
        None => return,
    };
    let node_data = &mut dom.node_data.as_container_mut()[dom_root];

    if let Some(ids) = xml_attributes.get_key("id") {
        for id in ids.split_whitespace() {
            ids_and_classes.push(Id(
                format_args_dynamic(id, &filtered_xml_attributes.types).into()
            ));
        }
    }

    if let Some(classes) = xml_attributes.get_key("class") {
        for class in classes.split_whitespace() {
            ids_and_classes.push(Class(
                format_args_dynamic(class, &filtered_xml_attributes.types).into(),
            ));
        }
    }

    node_data.set_ids_and_classes(ids_and_classes.into());

    if let Some(focusable) = xml_attributes
        .get_key("focusable")
        .map(|f| format_args_dynamic(f.as_str(), &filtered_xml_attributes.types))
        .and_then(|f| parse_bool(&f))
    {
        match focusable {
            true => node_data.set_tab_index(TabIndex::Auto),
            false => node_data.set_tab_index(TabIndex::NoKeyboardFocus.into()),
        }
    }

    if let Some(tab_index) = xml_attributes
        .get_key("tabindex")
        .map(|val| format_args_dynamic(val, &filtered_xml_attributes.types))
        .and_then(|val| val.parse::<isize>().ok())
    {
        match tab_index {
            0 => node_data.set_tab_index(TabIndex::Auto),
            i if i > 0 => node_data.set_tab_index(TabIndex::OverrideInParent(i as u32)),
            _ => node_data.set_tab_index(TabIndex::NoKeyboardFocus),
        }
    }

    if let Some(style) = xml_attributes.get_key("style") {
        let css_key_map = azul_css::get_css_key_map();
        let mut attributes = Vec::new();
        for s in style.as_str().split(";") {
            let mut s = s.split(":");
            let key = match s.next() {
                Some(s) => s,
                None => continue,
            };
            let value = match s.next() {
                Some(s) => s,
                None => continue,
            };
            azul_css::parser::parse_css_declaration(
                key.trim(),
                value.trim(),
                (ErrorLocation::default(), ErrorLocation::default()),
                &css_key_map,
                &mut Vec::new(),
                &mut attributes,
            );
        }

        let props = attributes
            .into_iter()
            .filter_map(|s| {
                use crate::dom::NodeDataInlineCssProperty::*;
                match s {
                    CssDeclaration::Static(s) => Some(Normal(s)),
                    _ => return None,
                }
            })
            .collect::<Vec<_>>();

        node_data.set_inline_css_props(props.into());
    }
}

pub fn set_stringified_attributes(
    dom_string: &mut String,
    xml_attributes: &XmlAttributeMap,
    filtered_xml_attributes: &ComponentArgumentTypes,
    tabs: usize,
) {
    let t0 = String::from("    ").repeat(tabs);
    let t = String::from("    ").repeat(tabs + 1);

    // push ids and classes
    let mut ids_and_classes = String::new();

    for id in xml_attributes
        .get_key("id")
        .map(|s| s.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default()
    {
        ids_and_classes.push_str(&format!(
            "{}    Id(AzString::from_const_str(\"{}\")),\r\n",
            t0,
            format_args_dynamic(id, &filtered_xml_attributes)
        ));
    }

    for class in xml_attributes
        .get_key("class")
        .map(|s| s.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default()
    {
        ids_and_classes.push_str(&format!(
            "{}    Class(AzString::from_const_str(\"{}\")),\r\n",
            t0,
            format_args_dynamic(class, &filtered_xml_attributes)
        ));
    }

    if !ids_and_classes.is_empty() {
        use crate::css::GetHash;
        let id = ids_and_classes.get_hash();
        dom_string.push_str(&format!(
            "\r\n{t0}.with_ids_and_classes({{\r\n{t}const IDS_AND_CLASSES_{id}: &[IdOrClass] = \
             &[\r\n{t}{ids_and_classes}\r\n{t}];\r\\
             n{t}IdOrClassVec::from_const_slice(IDS_AND_CLASSES_{id})\r\n{t0}}})",
            t0 = t0,
            t = t,
            ids_and_classes = ids_and_classes,
            id = id
        ));
    }

    if let Some(focusable) = xml_attributes
        .get_key("focusable")
        .map(|f| format_args_dynamic(f, &filtered_xml_attributes))
        .and_then(|f| parse_bool(&f))
    {
        match focusable {
            true => dom_string.push_str(&format!("\r\n{}.with_tab_index(TabIndex::Auto)", t)),
            false => dom_string.push_str(&format!(
                "\r\n{}.with_tab_index(TabIndex::NoKeyboardFocus)",
                t
            )),
        }
    }

    if let Some(tab_index) = xml_attributes
        .get_key("tabindex")
        .map(|val| format_args_dynamic(val, &filtered_xml_attributes))
        .and_then(|val| val.parse::<isize>().ok())
    {
        match tab_index {
            0 => dom_string.push_str(&format!("\r\n{}.with_tab_index(TabIndex::Auto)", t)),
            i if i > 0 => dom_string.push_str(&format!(
                "\r\n{}.with_tab_index(TabIndex::OverrideInParent({}))",
                t, i as usize
            )),
            _ => dom_string.push_str(&format!(
                "\r\n{}.with_tab_index(TabIndex::NoKeyboardFocus)",
                t
            )),
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
/// # use azul_core::xml::DynamicItem::*;
/// # use azul_core::xml::split_dynamic_string;
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
                    if c.is_whitespace() {
                        break;
                    }
                    if *c == '}' && input.get(current_idx + start_offset + 1).copied() != Some('}')
                    {
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
                    items.push(Var(input
                        [(current_idx + 1)..(current_idx + start_offset - 1)]
                        .iter()
                        .collect()));
                    current_idx = current_idx + start_offset;
                    last_idx = current_idx;
                } else {
                    current_idx += start_offset;
                }
            }
            _ => {
                current_idx += 1;
            }
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

/// Combines the split string back into its original form while replacing the variables with their
/// values
///
/// let variables = btreemap!{ "a" => "value1", "b" => "value2" };
/// [Str("hello "), Var("a"), Str(", "), Var("b"), Str("{ "), Var("c"), Str(" }}")]
/// => "hello value1, valuec{ {c} }"
pub fn combine_and_replace_dynamic_items(
    input: &[DynamicItem],
    variables: &ComponentArgumentTypes,
) -> String {
    let mut s = String::new();

    for item in input {
        match item {
            DynamicItem::Var(v) => {
                let variable_name = normalize_casing(v.trim());
                match variables
                    .iter()
                    .find(|s| s.0 == variable_name)
                    .map(|q| &q.1)
                {
                    Some(resolved_var) => {
                        s.push_str(&resolved_var);
                    }
                    None => {
                        s.push('{');
                        s.push_str(v);
                        s.push('}');
                    }
                }
            }
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
/// # use azul_core::xml::format_args_dynamic;
/// let mut variables = vec![
///     (String::from("a"), String::from("value1")),
///     (String::from("b"), String::from("value2")),
/// ];
///
/// let initial = "hello {a}, {b}{{ {c} }}";
/// let expected = "hello value1, value2{ {c} }".to_string();
/// assert_eq!(format_args_dynamic(initial, &variables), expected);
/// ```
///
/// Note: the number (0, 1, etc.) is the order of the argument, it is irrelevant for
/// runtime formatting, only important for keeping the component / function arguments
/// in order when compiling the arguments to Rust code
pub fn format_args_dynamic(input: &str, variables: &ComponentArgumentTypes) -> String {
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
    map: &mut Vec<(
        ComponentName,
        CompiledComponent,
        ComponentArguments,
        BTreeMap<String, String>,
    )>,
    component_name: String,
    xml_component: &'a XmlComponent,
    component_map: &'a XmlComponentMap,
    parent_xml_attributes: &ComponentArguments,
    tabs: usize,
) -> Result<(), CompileError> {
    let t = String::from("    ").repeat(tabs - 1);
    let t1 = String::from("    ").repeat(tabs);

    let component_name = normalize_casing(&component_name);
    let xml_node = xml_component.renderer.get_xml_node();

    let mut css = match find_node_by_type(xml_node.children.as_ref(), "style")
        .and_then(|style_node| style_node.text.as_ref().map(|s| s.as_str()))
    {
        Some(text) => azul_css::parser::new_from_str(&text).0,
        None => Css::empty(),
    };

    css.sort_by_specificity();

    // Arguments of the current node
    let available_function_arg_types = xml_component.renderer.get_available_arguments();
    // Types of the filtered xml arguments, important, only for Rust code compilation
    let mut filtered_xml_attributes = available_function_arg_types.clone();

    if xml_component.inherit_vars {
        // Append all variables that are in scope for the parent node
        filtered_xml_attributes
            .args
            .extend(parent_xml_attributes.args.clone().into_iter());
    }

    // Instantiate the parent arguments in the current child arguments
    for v in filtered_xml_attributes.args.iter_mut() {
        v.1 = format_args_dynamic(&v.1, &parent_xml_attributes.args).to_string();
    }

    let text = xml_node
        .text
        .as_ref()
        .map(|t| AzString::from(format_args_dynamic(t, &filtered_xml_attributes.args)));

    let mut dom_string = xml_component.renderer.compile_to_rust_code(
        component_map,
        &filtered_xml_attributes,
        &text.into(),
    )?;

    set_stringified_attributes(
        &mut dom_string,
        &xml_node.attributes,
        &filtered_xml_attributes.args,
        tabs,
    );

    // TODO
    let matcher = CssMatcher {
        path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
        indices_in_parent: Vec::new(),
        children_length: Vec::new(),
    };

    let mut css_blocks = BTreeMap::new();
    let mut extra_blocks = VecContents::default();

    if !xml_node.children.as_ref().is_empty() {
        dom_string.push_str(&format!(
            "\r\n{}.with_children(DomVec::from_vec(vec![\r\n",
            t
        ));
        for (child_idx, child_node) in xml_node.children.as_ref().iter().enumerate() {
            let mut matcher = matcher.clone();
            matcher.indices_in_parent.push(child_idx);
            matcher
                .children_length
                .push(xml_node.children.as_ref().len());

            dom_string.push_str(&format!(
                "{}{},",
                t1,
                compile_node_to_rust_code_inner(
                    child_node,
                    component_map,
                    &filtered_xml_attributes,
                    tabs + 1,
                    &mut extra_blocks,
                    &mut css_blocks,
                    &css,
                    matcher,
                )?
            ));
        }
        dom_string.push_str(&format!("\r\n{}]))", t));
    }

    map.push((
        component_name,
        dom_string,
        filtered_xml_attributes,
        css_blocks,
    ));

    Ok(())
}

/// Takes all components and generates the source code function from them
pub fn compile_components_to_rust_code(
    components: &XmlComponentMap,
) -> Result<
    Vec<(
        ComponentName,
        CompiledComponent,
        ComponentArguments,
        BTreeMap<String, String>,
    )>,
    CompileError,
> {
    let mut map = Vec::new();

    for xml_component in &components.components {
        render_component_inner(
            &mut map,
            normalize_casing(&xml_component.id),
            xml_component,
            &components,
            &ComponentArguments::default(),
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
        use core::hash::Hash;

        use highway::{HighwayHash, HighwayHasher, Key};

        let mut hasher = HighwayHasher::new(Key([0; 4]));
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

        if self.path.is_empty() {
            return false;
        }
        if path.selectors.as_ref().is_empty() {
            return false;
        }

        // self_matcher is only ever going to contain "Children" selectors, never "DirectChildren"
        let mut path_groups = CssGroupIterator::new(path.selectors.as_ref()).collect::<Vec<_>>();
        path_groups.reverse();

        if path_groups.is_empty() {
            return false;
        }
        let mut self_groups = CssGroupIterator::new(self.path.as_ref()).collect::<Vec<_>>();
        self_groups.reverse();
        if self_groups.is_empty() {
            return false;
        }

        if self.indices_in_parent.len() != self_groups.len() {
            return false;
        }
        if self.children_length.len() != self_groups.len() {
            return false;
        }

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
                    advance = Some(id);
                    break;
                }
            }

            match advance {
                Some(n) => {
                    // group was found in remaining items
                    // advance cur_pathgroup_scan by 1 and cur_selfgroup_scan by n
                    if cur_pathgroup_scan == path_groups.len() - 1 {
                        // last path group
                        return cur_selfgroup_scan + n == self_groups.len() - 1;
                    } else {
                        cur_pathgroup_scan += 1;
                        cur_selfgroup_scan += n;
                        path_group = path_groups[cur_pathgroup_scan].clone();
                    }
                }
                None => return false, // group was not found in remaining items
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
    parent_children: usize,
) -> bool {
    use azul_css::{CssNthChildSelector, CssPathPseudoSelector, CssPathSelector::*};

    for selector in a {
        match selector {
            // always matches
            Global => {}
            PseudoSelector(CssPathPseudoSelector::Hover) => {}
            PseudoSelector(CssPathPseudoSelector::Active) => {}
            PseudoSelector(CssPathPseudoSelector::Focus) => {}

            Type(tag) => {
                if !b.iter().any(|t| **t == Type(tag.clone())) {
                    return false;
                }
            }
            Class(class) => {
                if !b.iter().any(|t| **t == Class(class.clone())) {
                    return false;
                }
            }
            Id(id) => {
                if !b.iter().any(|t| **t == Id(id.clone())) {
                    return false;
                }
            }
            PseudoSelector(CssPathPseudoSelector::First) => {
                if idx_in_parent != 0 {
                    return false;
                }
            }
            PseudoSelector(CssPathPseudoSelector::Last) => {
                if idx_in_parent != parent_children.saturating_sub(1) {
                    return false;
                }
            }
            PseudoSelector(CssPathPseudoSelector::NthChild(CssNthChildSelector::Number(i))) => {
                if idx_in_parent != *i as usize {
                    return false;
                }
            }
            PseudoSelector(CssPathPseudoSelector::NthChild(CssNthChildSelector::Even)) => {
                if idx_in_parent % 2 != 0 {
                    return false;
                }
            }
            PseudoSelector(CssPathPseudoSelector::NthChild(CssNthChildSelector::Odd)) => {
                if idx_in_parent % 2 == 0 {
                    return false;
                }
            }
            PseudoSelector(CssPathPseudoSelector::NthChild(CssNthChildSelector::Pattern(p))) => {
                if idx_in_parent.saturating_sub(p.offset as usize) % p.repeat as usize != 0 {
                    return false;
                }
            }

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
) -> Result<String, CompileError> {
    use azul_css::CssDeclaration;

    let t = "";
    let t2 = "    ";
    let mut dom_string = String::from("Dom::body()");
    let node_type = CssPathSelector::Type(NodeTypeTag::Body);
    matcher.path.push(node_type);

    let ids = body_node
        .attributes
        .get_key("id")
        .map(|s| s.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default();
    matcher.path.extend(
        ids.into_iter()
            .map(|id| CssPathSelector::Id(id.to_string().into())),
    );
    let classes = body_node
        .attributes
        .get_key("class")
        .map(|s| s.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default();
    matcher.path.extend(
        classes
            .into_iter()
            .map(|class| CssPathSelector::Class(class.to_string().into())),
    );

    let matcher_hash = matcher.get_hash();
    let css_blocks_for_this_node = get_css_blocks(css, &matcher);
    if !css_blocks_for_this_node.is_empty() {
        use crate::css::format_static_css_prop;

        let css_strings = css_blocks_for_this_node
            .iter()
            .rev()
            .map(|css_block| {
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

                let formatted = css_block
                    .block
                    .declarations
                    .as_ref()
                    .iter()
                    .rev()
                    .map(|s| match &s {
                        CssDeclaration::Static(s) => format!(
                            "NodeDataInlineCssProperty::{}({})",
                            wrapper,
                            format_static_css_prop(s, 1)
                        ),
                        CssDeclaration::Dynamic(d) => format!(
                            "NodeDataInlineCssProperty::{}({})",
                            wrapper,
                            format_static_css_prop(&d.default_value, 1)
                        ),
                    })
                    .collect::<Vec<String>>();

                format!("// {}\r\n{}", css_block.block.path, formatted.join(",\r\n"))
            })
            .collect::<Vec<_>>()
            .join(",\r\n");

        css_blocks.insert(format!("CSS_MATCH_{:09}", matcher_hash), css_strings);
        dom_string.push_str(&format!(
            "\r\n{}.with_inline_css_props(CSS_MATCH_{:09})",
            t2, matcher_hash
        ));
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

            dom_string.push_str(&format!(
                "{}{},\r\n",
                t,
                compile_node_to_rust_code_inner(
                    child_node,
                    component_map,
                    &ComponentArguments::default(),
                    1,
                    extra_blocks,
                    css_blocks,
                    css,
                    matcher,
                )?
            ));
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
                let mut ending = None;

                if let Some(CssPathSelector::PseudoSelector(p)) =
                    css_block.path.selectors.as_ref().last()
                {
                    ending = Some(*p);
                }

                blocks.push(CssBlock {
                    ending,
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
                }
                Str(s) => {
                    let s = s.replace("\"", "\\\"");
                    formatted_str.push_str(&s);
                }
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
    node: &XmlNode,
    component_map: &'a XmlComponentMap,
    parent_xml_attributes: &ComponentArguments,
    tabs: usize,
    extra_blocks: &mut VecContents,
    css_blocks: &mut BTreeMap<String, String>,
    css: &Css,
    mut matcher: CssMatcher,
) -> Result<String, CompileError> {
    use azul_css::CssDeclaration;

    let t = String::from("    ").repeat(tabs - 1);
    let t2 = String::from("    ").repeat(tabs);

    let component_name = normalize_casing(&node.node_type);

    let xml_component = component_map
        .components
        .iter()
        .find(|s| normalize_casing(&s.id) == component_name)
        .ok_or(ComponentError::UnknownComponent(
            component_name.clone().into(),
        ))?;

    // Arguments of the current node
    let available_function_args = xml_component.renderer.get_available_arguments();
    let mut filtered_xml_attributes =
        validate_and_filter_component_args(&node.attributes, &available_function_args)?;

    if xml_component.inherit_vars {
        // Append all variables that are in scope for the parent node
        filtered_xml_attributes
            .types
            .extend(parent_xml_attributes.args.clone().into_iter());
    }

    // Instantiate the parent arguments in the current child arguments
    for v in filtered_xml_attributes.types.iter_mut() {
        v.1 = format_args_dynamic(&v.1, &parent_xml_attributes.args).to_string();
    }

    let instantiated_function_arguments = {
        let mut args = filtered_xml_attributes
            .types
            .iter()
            .filter_map(|(xml_attribute_key, _xml_attribute_type)| {
                match node.attributes.get_key(xml_attribute_key).cloned() {
                    Some(s) => Some(format_args_for_rust_code(&s)),
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
            .collect::<Vec<String>>();

        args.sort_by(|a, b| a.cmp(&b));

        args.join(", ")
    };

    let text_as_first_arg = if filtered_xml_attributes.accepts_text {
        let node_text = node.text.clone().into_option().unwrap_or_default();
        let node_text = format_args_for_rust_code(node_text.trim());
        let trailing_comma = if !instantiated_function_arguments.is_empty() {
            ", "
        } else {
            ""
        };

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
        other => {
            return Err(CompileError::Dom(RenderDomError::Component(
                ComponentError::UnknownComponent(other.to_string().into()),
            )));
        }
    });

    // The dom string is the function name
    let mut dom_string = format!(
        "{}{}::render({}{})",
        t2, component_name, text_as_first_arg, instantiated_function_arguments
    );

    matcher.path.push(node_type);
    let ids = node
        .attributes
        .get_key("id")
        .map(|s| s.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default();

    matcher.path.extend(
        ids.into_iter()
            .map(|id| CssPathSelector::Id(id.to_string().into())),
    );

    let classes = node
        .attributes
        .get_key("class")
        .map(|s| s.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default();

    matcher.path.extend(
        classes
            .into_iter()
            .map(|class| CssPathSelector::Class(class.to_string().into())),
    );

    let matcher_hash = matcher.get_hash();
    let css_blocks_for_this_node = get_css_blocks(css, &matcher);
    if !css_blocks_for_this_node.is_empty() {
        use crate::css::format_static_css_prop;

        let css_strings = css_blocks_for_this_node
            .iter()
            .rev()
            .map(|css_block| {
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

                let formatted = css_block
                    .block
                    .declarations
                    .as_ref()
                    .iter()
                    .rev()
                    .map(|s| match &s {
                        CssDeclaration::Static(s) => format!(
                            "NodeDataInlineCssProperty::{}({})",
                            wrapper,
                            format_static_css_prop(s, 1)
                        ),
                        CssDeclaration::Dynamic(d) => format!(
                            "NodeDataInlineCssProperty::{}({})",
                            wrapper,
                            format_static_css_prop(&d.default_value, 1)
                        ),
                    })
                    .collect::<Vec<String>>();

                format!("// {}\r\n{}", css_block.block.path, formatted.join(",\r\n"))
            })
            .collect::<Vec<_>>()
            .join(",\r\n");

        css_blocks.insert(format!("CSS_MATCH_{:09}", matcher_hash), css_strings);
        dom_string.push_str(&format!(
            "\r\n{}.with_inline_css_props(CSS_MATCH_{:09})",
            t2, matcher_hash
        ));
    }

    set_stringified_attributes(
        &mut dom_string,
        &node.attributes,
        &filtered_xml_attributes.types,
        tabs,
    );

    let mut children_string = node
        .children
        .as_ref()
        .iter()
        .enumerate()
        .map(|(child_idx, c)| {
            let mut matcher = matcher.clone();
            matcher.path.push(CssPathSelector::Children);
            matcher.indices_in_parent.push(child_idx);
            matcher.children_length.push(node.children.len());

            compile_node_to_rust_code_inner(
                c,
                component_map,
                &ComponentArguments {
                    args: filtered_xml_attributes.types.clone(),
                    accepts_text: filtered_xml_attributes.accepts_text,
                },
                tabs + 1,
                extra_blocks,
                css_blocks,
                css,
                matcher,
            )
        })
        .collect::<Result<Vec<_>, _>>()?
        .join(&format!(",\r\n"));

    if !children_string.is_empty() {
        dom_string.push_str(&format!(
            "\r\n{}.with_children(DomVec::from_vec(vec![\r\n{}\r\n{}]))",
            t2, children_string, t2
        ));
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
    pub fn new<'a>(root: &'a XmlNode) -> Result<Self, ComponentParseError> {
        let node_type = normalize_casing(&root.node_type);

        if node_type.as_str() != "component" {
            return Err(ComponentParseError::NotAComponent);
        }

        let name = root
            .attributes
            .get_key("name")
            .cloned()
            .ok_or(ComponentParseError::NotAComponent)?;
        let accepts_text = root
            .attributes
            .get_key("accepts_text")
            .and_then(|p| parse_bool(p.as_str()))
            .unwrap_or(false);

        let args = match root.attributes.get_key("args") {
            Some(s) => parse_component_arguments(s)?,
            None => ComponentArgumentTypes::default(),
        };

        Ok(Self {
            name: normalize_casing(&name),
            arguments: ComponentArguments { args, accepts_text },
            root: root.clone(),
        })
    }
}

impl XmlComponentTrait for DynamicXmlComponent {
    fn get_available_arguments(&self) -> ComponentArguments {
        self.arguments.clone()
    }

    fn get_xml_node(&self) -> XmlNode {
        self.root.clone()
    }

    fn render_dom<'a>(
        &'a self,
        components: &'a XmlComponentMap,
        arguments: &FilteredComponentArguments,
        content: &XmlTextContent,
    ) -> Result<StyledDom, RenderDomError> {
        let mut component_css = match find_node_by_type(self.root.children.as_ref(), "style") {
            Some(style_node) => {
                if let Some(text) = style_node.text.as_ref().map(|s| s.as_str()) {
                    let parsed_css = CssApiWrapper::from_string(text.to_string().into());
                    Some(parsed_css)
                } else {
                    None
                }
            }
            None => None,
        };

        let mut dom = StyledDom::default();

        for child_node in self.root.children.as_ref() {
            dom.append_child(render_dom_from_body_node_inner(
                child_node, components, arguments,
            )?);
        }

        if let Some(css) = component_css.clone() {
            dom.restyle(css);
        }

        Ok(dom)
    }

    fn compile_to_rust_code(
        &self,
        components: &XmlComponentMap,
        attributes: &ComponentArguments,
        content: &XmlTextContent,
    ) -> Result<String, CompileError> {
        Ok("Dom::div()".into()) // TODO!s
    }
}
