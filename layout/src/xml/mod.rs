#![allow(unused_variables)]

use alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec};
use core::fmt;
#[cfg(feature = "std")]
use std::path::Path;

#[cfg(feature = "svg")]
pub mod svg;

pub use azul_core::xml::*;
use azul_core::{dom::Dom, impl_from, styled_dom::StyledDom, window::StringPairVec};
use azul_css::{
    parser::{CssApiWrapper, CssParseError},
    AzString, Css, OptionAzString, U8Vec,
};
use xmlparser::Tokenizer;

#[cfg(feature = "xml")]
pub fn domxml_from_str(xml: &str, component_map: &mut XmlComponentMap) -> DomXml {
    let mut error_css = CssApiWrapper::empty();

    let parsed = match parse_xml_string(&xml) {
        Ok(parsed) => parsed,
        Err(e) => {
            return DomXml {
                parsed_dom: Dom::body()
                    .with_children(vec![Dom::text(format!("{}", e))].into())
                    .style(error_css.clone()),
            };
        }
    };

    let parsed_dom = match str_to_dom(parsed.as_ref(), component_map, None) {
        Ok(o) => o,
        Err(e) => {
            return DomXml {
                parsed_dom: Dom::body()
                    .with_children(vec![Dom::text(format!("{}", e))].into())
                    .style(error_css.clone()),
            };
        }
    };

    DomXml { parsed_dom }
}

/// Loads, parses and builds a DOM from an XML file
///
/// **Warning**: The file is reloaded from disk on every function call - do not
/// use this in release builds! This function deliberately never fails: In an error case,
/// the error gets rendered as a `NodeType::Label`.
#[cfg(all(feature = "std", feature = "xml"))]
pub fn domxml_from_file<I: AsRef<Path>>(
    file_path: I,
    component_map: &mut XmlComponentMap,
) -> DomXml {
    use std::fs;

    let mut error_css = CssApiWrapper::empty();

    let xml = match fs::read_to_string(file_path.as_ref()) {
        Ok(xml) => xml,
        Err(e) => {
            return DomXml {
                parsed_dom: Dom::body()
                    .with_children(
                        vec![Dom::text(format!(
                            "Error reading: \"{}\": {}",
                            file_path.as_ref().to_string_lossy(),
                            e
                        ))]
                        .into(),
                    )
                    .style(error_css.clone()),
            };
        }
    };

    domxml_from_str(&xml, component_map)
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
/// # use azul_layout::xml::{XmlNode, parse_xml_string};
/// assert_eq!(
///     parse_xml_string("<app><p /><div id='thing' /></app>").unwrap(),
///     vec![XmlNode::new("app").with_children(vec![
///         XmlNode::new("p"),
///         XmlNode::new("div").with_attribute("id", "thing"),
///     ])]
/// )
/// ```
#[cfg(feature = "xml")]
pub fn parse_xml_string(xml: &str) -> Result<Vec<XmlNode>, XmlError> {
    use xmlparser::{ElementEnd::*, Token::*, Tokenizer};

    use self::XmlParseError::*;

    let mut root_node = XmlNode::default();

    // Search for "<?xml" and "?>" tags and delete them from the XML
    let mut xml = xml.trim();
    if xml.starts_with("<?") {
        let pos = xml
            .find("?>")
            .ok_or(XmlError::MalformedHierarchy("<?xml".into(), "?>".into()))?;
        xml = &xml[(pos + 2)..];
    }

    // Delete <!doctype if necessary
    let mut xml = xml.trim();
    if xml.starts_with("<!") {
        let pos = xml
            .find(">")
            .ok_or(XmlError::MalformedHierarchy("<!doctype".into(), ">".into()))?;
        xml = &xml[(pos + 1)..];
    }

    let tokenizer = Tokenizer::from_fragment(xml, 0..xml.len());

    // In order to insert where the item is, let's say
    // [0 -> 1st element, 5th-element -> node]
    // we need to trach the index of the item in the parent.
    let mut current_hierarchy: Vec<usize> = Vec::new();

    for token in tokenizer {
        let token = token.map_err(|e| XmlError::ParserError(translate_xmlparser_error(e)))?;
        match token {
            ElementStart { local, .. } => {
                if let Some(current_parent) = get_item(&current_hierarchy, &mut root_node) {
                    let children_len = current_parent.children.len();
                    current_parent.children.push(XmlNode {
                        node_type: local.to_string().into(),
                        attributes: StringPairVec::new(),
                        children: Vec::new().into(),
                        text: None.into(),
                    });
                    current_hierarchy.push(children_len);
                }
            }
            ElementEnd { end: Empty, .. } => {
                current_hierarchy.pop();
            }
            ElementEnd {
                end: Close(_, close_value),
                ..
            } => {
                let i = get_item(&current_hierarchy, &mut root_node);
                if let Some(last) = i {
                    if last.node_type.as_str() != close_value.as_str() {
                        return Err(XmlError::MalformedHierarchy(
                            close_value.to_string().into(),
                            last.node_type.clone(),
                        ));
                    }
                }
                current_hierarchy.pop();
            }
            Attribute { local, value, .. } => {
                if let Some(last) = get_item(&current_hierarchy, &mut root_node) {
                    // NOTE: Only lowercase the key ("local"), not the value!
                    last.attributes.push(azul_core::window::AzStringPair {
                        key: local.to_string().into(),
                        value: value.as_str().to_string().into(),
                    });
                }
            }
            Text { text } => {
                let text = text.trim();
                if !text.is_empty() {
                    if let Some(last) = get_item(&current_hierarchy, &mut root_node) {
                        if let Some(s) = last.text.as_mut() {
                            let mut newstr = s.as_str().to_string();
                            newstr.push_str(text);
                            *s = newstr.into();
                        }
                        if last.text.is_none() {
                            last.text = Some(text.to_string().into()).into();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok(root_node.children.into())
}

#[cfg(feature = "xml")]
pub fn parse_xml(s: &str) -> Result<Xml, XmlError> {
    Ok(Xml {
        root: parse_xml_string(s)?.into(),
    })
}

#[cfg(not(feature = "xml"))]
pub fn parse_xml(s: &str) -> Result<Xml, XmlError> {
    Err(XmlError::NoParserAvailable)
}

// to_string(&self) -> String

#[cfg(feature = "xml")]
pub fn translate_roxmltree_expandedname<'a, 'b>(
    e: roxmltree::ExpandedName<'a, 'b>,
) -> XmlQualifiedName {
    let ns: Option<AzString> = e.namespace().map(|e| e.to_string().into());
    XmlQualifiedName {
        name: e.name().to_string().into(),
        namespace: ns.into(),
    }
}

#[cfg(feature = "xml")]
fn translate_roxmltree_attribute<'a>(e: roxmltree::Attribute<'a>) -> XmlQualifiedName {
    XmlQualifiedName {
        name: e.name().to_string().into(),
        namespace: e.namespace().map(|e| e.to_string().into()).into(),
    }
}

#[cfg(feature = "xml")]
fn translate_xmlparser_streamerror(e: xmlparser::StreamError) -> XmlStreamError {
    match e {
        xmlparser::StreamError::UnexpectedEndOfStream => XmlStreamError::UnexpectedEndOfStream,
        xmlparser::StreamError::InvalidName => XmlStreamError::InvalidName,
        xmlparser::StreamError::InvalidReference => XmlStreamError::InvalidReference,
        xmlparser::StreamError::InvalidExternalID => XmlStreamError::InvalidExternalID,
        xmlparser::StreamError::InvalidCommentData => XmlStreamError::InvalidCommentData,
        xmlparser::StreamError::InvalidCommentEnd => XmlStreamError::InvalidCommentEnd,
        xmlparser::StreamError::InvalidCharacterData => XmlStreamError::InvalidCharacterData,
        xmlparser::StreamError::NonXmlChar(c, tp) => XmlStreamError::NonXmlChar(NonXmlCharError {
            ch: c.into(),
            pos: translate_roxml_textpos(tp),
        }),
        xmlparser::StreamError::InvalidChar(a, b, tp) => {
            XmlStreamError::InvalidChar(InvalidCharError {
                expected: a,
                got: b,
                pos: translate_roxml_textpos(tp),
            })
        }
        xmlparser::StreamError::InvalidCharMultiple(a, b, tp) => {
            XmlStreamError::InvalidCharMultiple(InvalidCharMultipleError {
                expected: a,
                got: b.to_vec().into(),
                pos: translate_roxml_textpos(tp),
            })
        }
        xmlparser::StreamError::InvalidQuote(a, tp) => {
            XmlStreamError::InvalidQuote(InvalidQuoteError {
                got: a.into(),
                pos: translate_roxml_textpos(tp),
            })
        }
        xmlparser::StreamError::InvalidSpace(a, tp) => {
            XmlStreamError::InvalidSpace(InvalidSpaceError {
                got: a.into(),
                pos: translate_roxml_textpos(tp),
            })
        }
        xmlparser::StreamError::InvalidString(a, tp) => {
            XmlStreamError::InvalidString(InvalidStringError {
                got: a.to_string().into(),
                pos: translate_roxml_textpos(tp),
            })
        }
    }
}

#[cfg(feature = "xml")]
fn translate_xmlparser_error(e: xmlparser::Error) -> XmlParseError {
    match e {
        xmlparser::Error::InvalidDeclaration(se, tp) => {
            XmlParseError::InvalidDeclaration(XmlTextError {
                stream_error: translate_xmlparser_streamerror(se),
                pos: translate_roxml_textpos(tp),
            })
        }
        xmlparser::Error::InvalidComment(se, tp) => XmlParseError::InvalidComment(XmlTextError {
            stream_error: translate_xmlparser_streamerror(se),
            pos: translate_roxml_textpos(tp),
        }),
        xmlparser::Error::InvalidPI(se, tp) => XmlParseError::InvalidPI(XmlTextError {
            stream_error: translate_xmlparser_streamerror(se),
            pos: translate_roxml_textpos(tp),
        }),
        xmlparser::Error::InvalidDoctype(se, tp) => XmlParseError::InvalidDoctype(XmlTextError {
            stream_error: translate_xmlparser_streamerror(se),
            pos: translate_roxml_textpos(tp),
        }),
        xmlparser::Error::InvalidEntity(se, tp) => XmlParseError::InvalidEntity(XmlTextError {
            stream_error: translate_xmlparser_streamerror(se),
            pos: translate_roxml_textpos(tp),
        }),
        xmlparser::Error::InvalidElement(se, tp) => XmlParseError::InvalidElement(XmlTextError {
            stream_error: translate_xmlparser_streamerror(se),
            pos: translate_roxml_textpos(tp),
        }),
        xmlparser::Error::InvalidAttribute(se, tp) => {
            XmlParseError::InvalidAttribute(XmlTextError {
                stream_error: translate_xmlparser_streamerror(se),
                pos: translate_roxml_textpos(tp),
            })
        }
        xmlparser::Error::InvalidCdata(se, tp) => XmlParseError::InvalidCdata(XmlTextError {
            stream_error: translate_xmlparser_streamerror(se),
            pos: translate_roxml_textpos(tp),
        }),
        xmlparser::Error::InvalidCharData(se, tp) => XmlParseError::InvalidCharData(XmlTextError {
            stream_error: translate_xmlparser_streamerror(se),
            pos: translate_roxml_textpos(tp),
        }),
        xmlparser::Error::UnknownToken(tp) => {
            XmlParseError::UnknownToken(translate_roxml_textpos(tp))
        }
    }
}

#[cfg(feature = "xml")]
pub(crate) fn translate_roxmltree_error(e: roxmltree::Error) -> XmlError {
    match e {
        roxmltree::Error::InvalidXmlPrefixUri(s) => {
            XmlError::InvalidXmlPrefixUri(translate_roxml_textpos(s))
        }
        roxmltree::Error::UnexpectedXmlUri(s) => {
            XmlError::UnexpectedXmlUri(translate_roxml_textpos(s))
        }
        roxmltree::Error::UnexpectedXmlnsUri(s) => {
            XmlError::UnexpectedXmlnsUri(translate_roxml_textpos(s))
        }
        roxmltree::Error::InvalidElementNamePrefix(s) => {
            XmlError::InvalidElementNamePrefix(translate_roxml_textpos(s))
        }
        roxmltree::Error::DuplicatedNamespace(s, tp) => {
            XmlError::DuplicatedNamespace(DuplicatedNamespaceError {
                ns: s.into(),
                pos: translate_roxml_textpos(tp),
            })
        }
        roxmltree::Error::UnknownNamespace(s, tp) => {
            XmlError::UnknownNamespace(UnknownNamespaceError {
                ns: s.into(),
                pos: translate_roxml_textpos(tp),
            })
        }
        roxmltree::Error::UnexpectedCloseTag {
            expected,
            actual,
            pos,
        } => XmlError::UnexpectedCloseTag(UnexpectedCloseTagError {
            expected: expected.into(),
            actual: actual.into(),
            pos: translate_roxml_textpos(pos),
        }),
        roxmltree::Error::UnexpectedEntityCloseTag(s) => {
            XmlError::UnexpectedEntityCloseTag(translate_roxml_textpos(s))
        }
        roxmltree::Error::UnknownEntityReference(s, tp) => {
            XmlError::UnknownEntityReference(UnknownEntityReferenceError {
                entity: s.into(),
                pos: translate_roxml_textpos(tp),
            })
        }
        roxmltree::Error::MalformedEntityReference(s) => {
            XmlError::MalformedEntityReference(translate_roxml_textpos(s))
        }
        roxmltree::Error::EntityReferenceLoop(s) => {
            XmlError::EntityReferenceLoop(translate_roxml_textpos(s))
        }
        roxmltree::Error::InvalidAttributeValue(s) => {
            XmlError::InvalidAttributeValue(translate_roxml_textpos(s))
        }
        roxmltree::Error::DuplicatedAttribute(s, tp) => {
            XmlError::DuplicatedAttribute(DuplicatedAttributeError {
                attribute: s.into(),
                pos: translate_roxml_textpos(tp),
            })
        }
        roxmltree::Error::NoRootNode => XmlError::NoRootNode,
        roxmltree::Error::SizeLimit => XmlError::SizeLimit,
        roxmltree::Error::DtdDetected => XmlError::DtdDetected,
        roxmltree::Error::ParserError(s) => XmlError::ParserError(translate_xmlparser_error(s)),
    }
}

#[cfg(feature = "xml")]
#[inline(always)]
const fn translate_roxml_textpos(o: roxmltree::TextPos) -> XmlTextPos {
    XmlTextPos {
        row: o.row,
        col: o.col,
    }
}
