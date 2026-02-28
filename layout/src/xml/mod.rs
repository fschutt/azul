#![allow(unused_variables)]

use alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec};
use core::fmt;
#[cfg(feature = "std")]
use std::path::Path;

#[cfg(feature = "svg")]
pub mod svg;

/// Decodes XML/HTML entities in a string.
/// Handles standard XML entities: &lt; &gt; &amp; &apos; &quot;
/// and numeric character references: &#60; &#x3C;
fn decode_xml_entities(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '&' {
            // Collect the entity reference
            let mut entity = String::new();
            let mut found_semicolon = false;
            
            while let Some(&next) = chars.peek() {
                if next == ';' {
                    chars.next();
                    found_semicolon = true;
                    break;
                }
                if !next.is_alphanumeric() && next != '#' {
                    break;
                }
                entity.push(chars.next().unwrap());
                if entity.len() > 10 {
                    // Entity too long, not a valid entity
                    break;
                }
            }
            
            if found_semicolon {
                // Try to decode the entity
                match entity.as_str() {
                    "lt" => result.push('<'),
                    "gt" => result.push('>'),
                    "amp" => result.push('&'),
                    "apos" => result.push('\''),
                    "quot" => result.push('"'),
                    "nbsp" => result.push('\u{00A0}'),
                    s if s.starts_with('#') => {
                        // Numeric character reference
                        let num_str = &s[1..];
                        let code_point = if num_str.starts_with('x') || num_str.starts_with('X') {
                            // Hexadecimal
                            u32::from_str_radix(&num_str[1..], 16).ok()
                        } else {
                            // Decimal
                            num_str.parse::<u32>().ok()
                        };
                        if let Some(cp) = code_point {
                            if let Some(ch) = char::from_u32(cp) {
                                result.push(ch);
                            } else {
                                // Invalid code point, keep original
                                result.push('&');
                                result.push_str(&entity);
                                result.push(';');
                            }
                        } else {
                            // Parse failed, keep original
                            result.push('&');
                            result.push_str(&entity);
                            result.push(';');
                        }
                    }
                    _ => {
                        // Unknown entity, keep original
                        result.push('&');
                        result.push_str(&entity);
                        result.push(';');
                    }
                }
            } else {
                // No semicolon found, not a valid entity reference
                result.push('&');
                result.push_str(&entity);
            }
        } else {
            result.push(c);
        }
    }
    
    result
}

pub use azul_core::xml::*;
use azul_core::{dom::Dom, impl_from, styled_dom::StyledDom, window::StringPairVec};
#[cfg(feature = "parser")]
use azul_css::parser2::CssParseError;
use azul_css::{css::Css, AzString, OptionString, U8Vec};
use xmlparser::Tokenizer;

#[cfg(feature = "xml")]
pub fn domxml_from_str(xml: &str, component_map: &ComponentMap) -> DomXml {
    let error_css = Css::empty();

    let parsed = match parse_xml_string(&xml) {
        Ok(parsed) => parsed,
        Err(e) => {
            return DomXml {
                parsed_dom: {
                    let mut dom = Dom::create_body()
                        .with_children(vec![Dom::create_text(format!("{}", e))].into());
                    StyledDom::create(&mut dom, error_css.clone())
                },
            };
        }
    };

    let parsed_dom = match str_to_dom(parsed.as_ref(), component_map, None) {
        Ok(o) => o,
        Err(e) => {
            return DomXml {
                parsed_dom: {
                    let mut dom = Dom::create_body()
                        .with_children(vec![Dom::create_text(format!("{}", e))].into());
                    StyledDom::create(&mut dom, error_css.clone())
                },
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
    component_map: &ComponentMap,
) -> DomXml {
    use std::fs;

    let error_css = Css::empty();

    let xml = match fs::read_to_string(file_path.as_ref()) {
        Ok(xml) => xml,
        Err(e) => {
            return DomXml {
                parsed_dom: {
                    let mut dom = Dom::create_body()
                        .with_children(
                            vec![Dom::create_text(format!(
                                "Error reading: \"{}\": {}",
                                file_path.as_ref().to_string_lossy(),
                                e
                            ))]
                            .into(),
                        );
                    StyledDom::create(&mut dom, error_css.clone())
                },
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
#[cfg(feature = "xml")]
pub fn parse_xml_string(xml: &str) -> Result<Vec<XmlNodeChild>, XmlError> {
    use xmlparser::{ElementEnd::*, Token::*, Tokenizer};

    use self::XmlParseError::*;

    let mut root_node = XmlNode::default();

    // Search for "<?xml" and "?>" tags and delete them from the XML
    let mut xml = xml.trim();
    if xml.starts_with("<?") {
        let pos = xml.find("?>").ok_or(XmlError::MalformedHierarchy(
            azul_core::xml::MalformedHierarchyError {
                expected: "<?xml".into(),
                got: "?>".into(),
            },
        ))?;
        xml = &xml[(pos + 2)..];
    }

    // Delete <!DOCTYPE ...> if necessary (case-insensitive)
    let mut xml = xml.trim();
    if xml.len() > 9 && xml[..9].to_ascii_lowercase().starts_with("<!doctype") {
        let pos = xml.find(">").ok_or(XmlError::MalformedHierarchy(
            azul_core::xml::MalformedHierarchyError {
                expected: "<!DOCTYPE".into(),
                got: ">".into(),
            },
        ))?;
        xml = &xml[(pos + 1)..];
    } else if xml.starts_with("<!--") {
        // Skip HTML comments at the start
        if let Some(end) = xml.find("-->") {
            xml = &xml[(end + 3)..];
            xml = xml.trim();
        }
    }

    let tokenizer = Tokenizer::from_fragment(xml, 0..xml.len());

    // OPTIMIZED: Use a stack of raw pointers to avoid O(n*d) traversal on every token.
    // This is safe because:
    // 1. All pointers point into `root_node` which is owned and not moved
    // 2. We never hold multiple mutable references simultaneously
    // 3. The stack is only used within this function
    let mut node_stack: Vec<*mut XmlNode> = vec![&mut root_node as *mut XmlNode];

    // HTML5-lite parser: List of void elements that should auto-close
    // See: https://developer.mozilla.org/en-US/docs/Glossary/Void_element
    const VOID_ELEMENTS: &[&str] = &[
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
        "source", "track", "wbr",
    ];

    // HTML5-lite parser: Elements that auto-close when certain other elements are encountered
    // Format: (element_name, closes_when_encountering)
    const AUTO_CLOSE_RULES: &[(&str, &[&str])] = &[
        // List items close when encountering another list item or when parent closes
        ("li", &["li"]),
        // Table cells/rows have complex closing rules
        ("td", &["td", "th", "tr"]),
        ("th", &["td", "th", "tr"]),
        ("tr", &["tr"]),
        // Paragraphs close on block-level elements
        (
            "p",
            &[
                "address",
                "article",
                "aside",
                "blockquote",
                "div",
                "dl",
                "fieldset",
                "footer",
                "form",
                "h1",
                "h2",
                "h3",
                "h4",
                "h5",
                "h6",
                "header",
                "hr",
                "main",
                "nav",
                "ol",
                "p",
                "pre",
                "section",
                "table",
                "ul",
            ],
        ),
        // Option closes on another option or optgroup
        ("option", &["option", "optgroup"]),
        ("optgroup", &["optgroup"]),
        // DD/DT close on each other
        ("dd", &["dd", "dt"]),
        ("dt", &["dd", "dt"]),
    ];

    // Track which hierarchy level is a void element (shouldn't be pushed to hierarchy)
    let mut last_was_void = false;

    for token in tokenizer {
        let token = token.map_err(|e| XmlError::ParserError(translate_xmlparser_error(e)))?;
        match token {
            ElementStart { local, .. } => {
                let tag_name = local.to_string();
                let is_void_element = VOID_ELEMENTS.contains(&tag_name.as_str());

                // HTML5-lite: If last element was a void element (like <img src="...">),
                // pop it from hierarchy before processing the new element
                if last_was_void {
                    node_stack.pop();
                    last_was_void = false;
                }

                // HTML5-lite: Check if we need to auto-close the current element
                if node_stack.len() > 1 {
                    // SAFETY: We only access the last element, which is valid
                    let current_element = unsafe { &*node_stack[node_stack.len() - 1] };
                    let current_tag = current_element.node_type.as_str();

                    // Check if current element should auto-close when encountering this new tag
                    for (element, closes_on) in AUTO_CLOSE_RULES {
                        if current_tag == *element && closes_on.contains(&tag_name.as_str()) {
                            // Auto-close the current element
                            node_stack.pop();
                            break;
                        }
                    }
                }

                // SAFETY: We access the last element which is valid
                if let Some(&current_parent_ptr) = node_stack.last() {
                    let current_parent = unsafe { &mut *current_parent_ptr };
                    
                    current_parent.children.push(XmlNodeChild::Element(XmlNode {
                        node_type: tag_name.into(),
                        attributes: StringPairVec::new().into(),
                        children: Vec::new().into(),
                    }));

                    // Get pointer to the newly added child
                    let children_len = current_parent.children.len();
                    if let Some(XmlNodeChild::Element(ref mut new_child)) = current_parent.children.as_mut().get_mut(children_len - 1) {
                        node_stack.push(new_child as *mut XmlNode);
                    }
                    
                    last_was_void = is_void_element;
                }
            }
            ElementEnd { end: Empty, .. } => {
                // Pop hierarchy for all elements (including void elements after their attributes)
                if node_stack.len() > 1 {
                    node_stack.pop();
                }
                last_was_void = false;
            }
            ElementEnd {
                end: Close(_, close_value),
                ..
            } => {
                // HTML5-lite: If last element was a void element, pop it first
                if last_was_void {
                    node_stack.pop();
                    last_was_void = false;
                }

                // HTML5-lite: Check if this is a void element - if so, ignore the closing tag
                let is_void_element = VOID_ELEMENTS.contains(&close_value.as_str());
                if is_void_element {
                    // Void elements shouldn't have closing tags, but tolerate them
                    continue;
                }

                // HTML5-lite: Auto-close any elements that should be closed
                // Walk up the hierarchy and auto-close elements until we find a match
                let close_value_str = close_value.as_str();

                // Find matching element in stack (skip root at index 0)
                let mut found_idx = None;
                for i in (1..node_stack.len()).rev() {
                    // SAFETY: All pointers in stack are valid
                    let node = unsafe { &*node_stack[i] };
                    if node.node_type.as_str() == close_value_str {
                        found_idx = Some(i);
                        break;
                    }
                }

                if let Some(idx) = found_idx {
                    // Pop all elements from current position to the matching element (inclusive)
                    node_stack.truncate(idx);
                }
                // If no match found, just ignore (lenient HTML parsing)

                last_was_void = false;
            }
            Attribute { local, value, .. } => {
                // SAFETY: Last element in stack is valid
                if let Some(&last_ptr) = node_stack.last() {
                    let last = unsafe { &mut *last_ptr };
                    // NOTE: Only lowercase the key ("local"), not the value!
                    // Decode XML entities in attribute values as well
                    last.attributes.push(azul_core::window::AzStringPair {
                        key: local.to_string().into(),
                        value: decode_xml_entities(value.as_str()).into(),
                    });
                }
            }
            Text { text } => {
                // HTML5-lite: If last element was a void element, pop it before adding text
                if last_was_void {
                    node_stack.pop();
                    last_was_void = false;
                }

                // IMPORTANT: Preserve ALL text nodes including whitespace-only nodes.
                // Whether whitespace is significant depends on the CSS `white-space` property,
                // which is determined during layout, not during parsing.
                // 
                // For example: <pre><span>    </span></pre> must preserve the 4 spaces.
                // 
                // We only skip completely EMPTY text nodes (zero-length strings).
                let text_str = text.as_str();

                if !text_str.is_empty() {
                    // SAFETY: Last element in stack is valid
                    if let Some(&current_parent_ptr) = node_stack.last() {
                        let current_parent = unsafe { &mut *current_parent_ptr };
                        // Decode XML entities (e.g., &lt; -> <, &gt; -> >, etc.)
                        let decoded_text = decode_xml_entities(text_str);
                        // Add text as a child node
                        current_parent
                            .children
                            .push(XmlNodeChild::Text(decoded_text.into()));
                    }
                }
            }
            _ => {}
        }
    }

    // Clean up: if we ended with a void element, pop it
    if last_was_void {
        node_stack.pop();
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
        local_name: e.name().to_string().into(),
        namespace: ns.into(),
    }
}

#[cfg(feature = "xml")]
fn translate_roxmltree_attribute(e: roxmltree::Attribute) -> XmlQualifiedName {
    XmlQualifiedName {
        local_name: e.name().to_string().into(),
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
            pos: translate_xmlparser_textpos(tp),
        }),
        xmlparser::StreamError::InvalidChar(a, b, tp) => {
            XmlStreamError::InvalidChar(InvalidCharError {
                expected: a,
                got: b,
                pos: translate_xmlparser_textpos(tp),
            })
        }
        xmlparser::StreamError::InvalidCharMultiple(a, b, tp) => {
            XmlStreamError::InvalidCharMultiple(InvalidCharMultipleError {
                expected: a,
                got: b.to_vec().into(),
                pos: translate_xmlparser_textpos(tp),
            })
        }
        xmlparser::StreamError::InvalidQuote(a, tp) => {
            XmlStreamError::InvalidQuote(InvalidQuoteError {
                got: a.into(),
                pos: translate_xmlparser_textpos(tp),
            })
        }
        xmlparser::StreamError::InvalidSpace(a, tp) => {
            XmlStreamError::InvalidSpace(InvalidSpaceError {
                got: a.into(),
                pos: translate_xmlparser_textpos(tp),
            })
        }
        xmlparser::StreamError::InvalidString(a, tp) => {
            XmlStreamError::InvalidString(InvalidStringError {
                got: a.to_string().into(),
                pos: translate_xmlparser_textpos(tp),
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
                pos: translate_xmlparser_textpos(tp),
            })
        }
        xmlparser::Error::InvalidComment(se, tp) => XmlParseError::InvalidComment(XmlTextError {
            stream_error: translate_xmlparser_streamerror(se),
            pos: translate_xmlparser_textpos(tp),
        }),
        xmlparser::Error::InvalidPI(se, tp) => XmlParseError::InvalidPI(XmlTextError {
            stream_error: translate_xmlparser_streamerror(se),
            pos: translate_xmlparser_textpos(tp),
        }),
        xmlparser::Error::InvalidDoctype(se, tp) => XmlParseError::InvalidDoctype(XmlTextError {
            stream_error: translate_xmlparser_streamerror(se),
            pos: translate_xmlparser_textpos(tp),
        }),
        xmlparser::Error::InvalidEntity(se, tp) => XmlParseError::InvalidEntity(XmlTextError {
            stream_error: translate_xmlparser_streamerror(se),
            pos: translate_xmlparser_textpos(tp),
        }),
        xmlparser::Error::InvalidElement(se, tp) => XmlParseError::InvalidElement(XmlTextError {
            stream_error: translate_xmlparser_streamerror(se),
            pos: translate_xmlparser_textpos(tp),
        }),
        xmlparser::Error::InvalidAttribute(se, tp) => {
            XmlParseError::InvalidAttribute(XmlTextError {
                stream_error: translate_xmlparser_streamerror(se),
                pos: translate_xmlparser_textpos(tp),
            })
        }
        xmlparser::Error::InvalidCdata(se, tp) => XmlParseError::InvalidCdata(XmlTextError {
            stream_error: translate_xmlparser_streamerror(se),
            pos: translate_xmlparser_textpos(tp),
        }),
        xmlparser::Error::InvalidCharData(se, tp) => XmlParseError::InvalidCharData(XmlTextError {
            stream_error: translate_xmlparser_streamerror(se),
            pos: translate_xmlparser_textpos(tp),
        }),
        xmlparser::Error::UnknownToken(tp) => {
            XmlParseError::UnknownToken(translate_xmlparser_textpos(tp))
        }
    }
}

#[cfg(feature = "xml")]
pub fn translate_roxmltree_error(e: roxmltree::Error) -> XmlError {
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
        roxmltree::Error::UnexpectedCloseTag(expected, actual, pos) => {
            XmlError::UnexpectedCloseTag(UnexpectedCloseTagError {
                expected: expected.into(),
                actual: actual.into(),
                pos: translate_roxml_textpos(pos),
            })
        }
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
        roxmltree::Error::DtdDetected => XmlError::DtdDetected,
        roxmltree::Error::UnclosedRootNode => XmlError::UnclosedRootNode,
        roxmltree::Error::UnexpectedDeclaration(tp) => {
            XmlError::UnexpectedDeclaration(translate_roxml_textpos(tp))
        }
        roxmltree::Error::NodesLimitReached => XmlError::NodesLimitReached,
        roxmltree::Error::AttributesLimitReached => XmlError::AttributesLimitReached,
        roxmltree::Error::NamespacesLimitReached => XmlError::NamespacesLimitReached,
        roxmltree::Error::InvalidName(tp) => XmlError::InvalidName(translate_roxml_textpos(tp)),
        roxmltree::Error::NonXmlChar(_, tp) => XmlError::NonXmlChar(translate_roxml_textpos(tp)),
        roxmltree::Error::InvalidChar(_, _, tp) => {
            XmlError::InvalidChar(translate_roxml_textpos(tp))
        }
        roxmltree::Error::InvalidChar2(_, _, tp) => {
            XmlError::InvalidChar2(translate_roxml_textpos(tp))
        }
        roxmltree::Error::InvalidString(_, tp) => {
            XmlError::InvalidString(translate_roxml_textpos(tp))
        }
        roxmltree::Error::InvalidExternalID(tp) => {
            XmlError::InvalidExternalID(translate_roxml_textpos(tp))
        }
        roxmltree::Error::InvalidComment(tp) => {
            XmlError::InvalidComment(translate_roxml_textpos(tp))
        }
        roxmltree::Error::InvalidCharacterData(tp) => {
            XmlError::InvalidCharacterData(translate_roxml_textpos(tp))
        }
        roxmltree::Error::UnknownToken(tp) => XmlError::UnknownToken(translate_roxml_textpos(tp)),
        roxmltree::Error::UnexpectedEndOfStream => XmlError::UnexpectedEndOfStream,
        roxmltree::Error::EntityResolver(tp, s) => {
            // New in roxmltree 0.21: EntityResolver error variant
            // For now, treat as a generic entity reference error
            XmlError::UnknownEntityReference(UnknownEntityReferenceError {
                entity: s.into(),
                pos: translate_roxml_textpos(tp),
            })
        }
    }
}

#[cfg(feature = "xml")]
#[inline(always)]
const fn translate_xmlparser_textpos(o: xmlparser::TextPos) -> XmlTextPos {
    XmlTextPos {
        row: o.row,
        col: o.col,
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

/// Extension trait to add XML parsing capabilities to Dom
///
/// This trait provides methods to parse XML/XHTML strings and convert them
/// into Azul DOM trees. It's implemented as a trait to avoid circular dependencies
/// between azul-core and azul-layout.
#[cfg(feature = "xml")]
pub trait DomXmlExt {
    /// Parse XML/XHTML string into a DOM tree
    ///
    /// This method parses the XML string and converts it to an Azul StyledDom.
    /// On error, it returns a StyledDom displaying the error message.
    ///
    /// # Arguments
    /// * `xml` - The XML/XHTML string to parse
    ///
    /// # Returns
    /// A `StyledDom` tree representing the parsed XML, or an error DOM on parse failure
    fn from_xml_string<S: AsRef<str>>(xml: S) -> StyledDom;
}

#[cfg(feature = "xml")]
impl DomXmlExt for Dom {
    fn from_xml_string<S: AsRef<str>>(xml: S) -> StyledDom {
        let component_map = ComponentMap::with_builtin();
        let dom_xml = domxml_from_str(xml.as_ref(), &component_map);
        dom_xml.parsed_dom
    }
}
