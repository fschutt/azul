use azul_css::{U8Vec, AzString, OptionAzString};
use core::fmt;
#[cfg(feature = "xml")]
pub use crate::xml_parser::*;

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

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct Xml {
    root: XmlNodeVec,
}

impl Xml {
    pub fn parse(s: &str) -> Result<Xml, XmlError> {
        Ok(Self {
            root: crate::xml_parser::parse_xml_string(s)?,
        })
    }
    // to_string(&self) -> String
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
    DtdDetected,
    /// Invalid hierarchy close tags, i.e `<app></p></app>`
    MalformedHierarchy(AzString, AzString),
    ParserError(XmlParseError),
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::XmlError::*;
        match self {
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
            xmlparser::StreamError::NonXmlChar(c, tp) => XmlStreamError::NonXmlChar(NonXmlCharError { ch: c.into(), pos: tp.into() }),
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
            roxmltree::Error::DtdDetected => XmlError::DtdDetected,
            roxmltree::Error::ParserError(s) => XmlError::ParserError(s.into()),
        }
    }
}
