use std::ffi::c_void;
use azul_css::{U8Vec, AzString};
use azul_core::window::OptionAzString;

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

impl<'a, 'b> From<roxmltree::ExpandedName<'a, 'b>> for XmlQualifiedName {
    fn from(e: roxmltree::ExpandedName<'a, 'b>) -> XmlQualifiedName {
        let ns: Option<AzString> = e.namespace().map(|e| e.to_string().into());
        XmlQualifiedName {
            name: e.name().to_string().into(),
            namespace: ns.into()
        }
    }
}

impl<'a> From<roxmltree::Attribute<'a>> for XmlQualifiedName {
    fn from(e: roxmltree::Attribute<'a>) -> XmlQualifiedName {
        XmlQualifiedName {
            name: e.name().to_string().into(),
            namespace: e.namespace().map(|e| e.to_string().into()).into()
        }
    }
}

#[repr(C)]
pub struct Xml {
    ptr: *mut c_void, // *mut roxmltree::Document
}

impl Xml {
    fn new(doc: roxmltree::Document) -> Self { Self { ptr: Box::into_raw(Box::new(doc)) as *mut c_void } }
    fn get_doc<'a>(&'a self) -> &'a roxmltree::Document { unsafe { &*(self.ptr as *mut roxmltree::Document) } }
    pub fn parse(s: &str) -> Result<Xml, XmlError> { Ok(Self::new(roxmltree::Document::parse(s)?)) }
    pub fn root(&self) -> XmlNode { XmlNode::new(self.get_doc().root_element().clone()) }
}

impl Drop for Xml { fn drop(&mut self) { let _ = unsafe { Box::from_raw(self.ptr as *mut roxmltree::Document) }; } }

#[repr(C)]
pub struct XmlNode {
    ptr: *mut c_void, // *mut roxmltree::Node
}

impl XmlNode {
    fn new(doc: roxmltree::Node) -> Self { Self { ptr: Box::into_raw(Box::new(doc)) as *mut c_void } }
    fn get_node<'a>(&'a self) -> &'a roxmltree::Node { unsafe { &*(self.ptr as *mut roxmltree::Node) } }
    pub fn get_attribute(&self, attribute_key: &str) -> Option<AzString> { self.get_node().attribute(attribute_key).map(|v| v.to_string().into()) }
    pub fn attributes(&self) -> Vec<XmlQualifiedName> { self.get_node().attributes().iter().map(|v| v.clone().into()).collect() }
    pub fn text(&self) -> Option<String> { self.get_node().text().map(|s| s.to_string()) }
    pub fn children(&self) -> Vec<XmlNode> { self.get_node().children().map(|c| XmlNode::new(c.clone())).collect() }
}

impl Drop for XmlNode { fn drop(&mut self) { let _ = unsafe { Box::from_raw(self.ptr as *mut roxmltree::Node) }; } }

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct NonXmlCharError {
    pub ch: char,
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

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct XmlTextPos { pub row: u32, pub col: u32 }

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

impl From<roxmltree::TextPos> for XmlTextPos {
    fn from(o: roxmltree::TextPos) -> XmlTextPos {
        XmlTextPos { row: o.row, col: o.col }
    }
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C, u8)]
pub enum XmlError {
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
    ParserError(XmlParseError),
}

impl From<xmlparser::StreamError> for XmlStreamError {
    fn from(e: xmlparser::StreamError) -> XmlStreamError {
        match e {
            xmlparser::StreamError::UnexpectedEndOfStream => XmlStreamError::UnexpectedEndOfStream,
            xmlparser::StreamError::InvalidName => XmlStreamError::InvalidName,
            xmlparser::StreamError::InvalidReference => XmlStreamError::InvalidReference,
            xmlparser::StreamError::InvalidExternalID => XmlStreamError::InvalidExternalID,
            xmlparser::StreamError::InvalidCommentData => XmlStreamError::InvalidCommentData,
            xmlparser::StreamError::InvalidCommentEnd => XmlStreamError::InvalidCommentEnd,
            xmlparser::StreamError::InvalidCharacterData => XmlStreamError::InvalidCharacterData,
            xmlparser::StreamError::NonXmlChar(c, tp) => XmlStreamError::NonXmlChar(NonXmlCharError { ch: c, pos: tp.into() }),
            xmlparser::StreamError::InvalidChar(a, b, tp) => XmlStreamError::InvalidChar(InvalidCharError { expected: a, got: b, pos: tp.into() }),
            xmlparser::StreamError::InvalidCharMultiple(a, b, tp) => XmlStreamError::InvalidCharMultiple(InvalidCharMultipleError { expected: a, got: b.to_vec().into(), pos: tp.into() }),
            xmlparser::StreamError::InvalidQuote(a, tp) => XmlStreamError::InvalidQuote(InvalidQuoteError { got: a.into(), pos: tp.into() }),
            xmlparser::StreamError::InvalidSpace(a, tp) => XmlStreamError::InvalidSpace(InvalidSpaceError { got: a.into(), pos: tp.into() }),
            xmlparser::StreamError::InvalidString(a, tp) => XmlStreamError::InvalidString(InvalidStringError { got: a.to_string().into(), pos: tp.into() }),
        }
    }
}

impl From<xmlparser::Error> for XmlParseError {
    fn from(e: xmlparser::Error) -> XmlParseError {
        match e {
            xmlparser::Error::InvalidDeclaration(se, tp) => XmlParseError::InvalidDeclaration(XmlTextError { stream_error: se.into(), pos: tp.into() }),
            xmlparser::Error::InvalidComment(se, tp) => XmlParseError::InvalidComment(XmlTextError { stream_error: se.into(), pos: tp.into() }),
            xmlparser::Error::InvalidPI(se, tp) => XmlParseError::InvalidPI(XmlTextError { stream_error: se.into(), pos: tp.into() }),
            xmlparser::Error::InvalidDoctype(se, tp) => XmlParseError::InvalidDoctype(XmlTextError { stream_error: se.into(), pos: tp.into() }),
            xmlparser::Error::InvalidEntity(se, tp) => XmlParseError::InvalidEntity(XmlTextError { stream_error: se.into(), pos: tp.into() }),
            xmlparser::Error::InvalidElement(se, tp) => XmlParseError::InvalidElement(XmlTextError { stream_error: se.into(), pos: tp.into() }),
            xmlparser::Error::InvalidAttribute(se, tp) => XmlParseError::InvalidAttribute(XmlTextError { stream_error: se.into(), pos: tp.into() }),
            xmlparser::Error::InvalidCdata(se, tp) => XmlParseError::InvalidCdata(XmlTextError { stream_error: se.into(), pos: tp.into() }),
            xmlparser::Error::InvalidCharData(se, tp) => XmlParseError::InvalidCharData(XmlTextError { stream_error: se.into(), pos: tp.into() }),
            xmlparser::Error::UnknownToken(tp) => XmlParseError::UnknownToken(tp.into()),
        }
    }
}

impl From<roxmltree::Error> for XmlError {
    fn from(e: roxmltree::Error) -> XmlError {
        match e {
            roxmltree::Error::InvalidXmlPrefixUri(s) => XmlError::InvalidXmlPrefixUri(s.into()),
            roxmltree::Error::UnexpectedXmlUri(s) => XmlError::UnexpectedXmlUri(s.into()),
            roxmltree::Error::UnexpectedXmlnsUri(s) => XmlError::UnexpectedXmlnsUri(s.into()),
            roxmltree::Error::InvalidElementNamePrefix(s) => XmlError::InvalidElementNamePrefix(s.into()),
            roxmltree::Error::DuplicatedNamespace(s, tp) => XmlError::DuplicatedNamespace(DuplicatedNamespaceError { ns: s.into(), pos: tp.into() }),
            roxmltree::Error::UnknownNamespace(s, tp) => XmlError::UnknownNamespace(UnknownNamespaceError { ns: s.into(), pos: tp.into() }),
            roxmltree::Error::UnexpectedCloseTag { expected, actual, pos } => XmlError::UnexpectedCloseTag(UnexpectedCloseTagError { expected: expected.into(), actual: actual.into(), pos: pos.into() }),
            roxmltree::Error::UnexpectedEntityCloseTag(s) => XmlError::UnexpectedEntityCloseTag(s.into()),
            roxmltree::Error::UnknownEntityReference(s, tp) => XmlError::UnknownEntityReference(UnknownEntityReferenceError { entity: s.into(), pos: tp.into() }),
            roxmltree::Error::MalformedEntityReference(s) => XmlError::MalformedEntityReference(s.into()),
            roxmltree::Error::EntityReferenceLoop(s) => XmlError::EntityReferenceLoop(s.into()),
            roxmltree::Error::InvalidAttributeValue(s) => XmlError::InvalidAttributeValue(s.into()),
            roxmltree::Error::DuplicatedAttribute(s, tp) => XmlError::DuplicatedAttribute(DuplicatedAttributeError { attribute: s.into(), pos: tp.into() }),
            roxmltree::Error::NoRootNode => XmlError::NoRootNode,
            roxmltree::Error::SizeLimit => XmlError::SizeLimit,
            roxmltree::Error::ParserError(s) => XmlError::ParserError(s.into()),
        }
    }
}
