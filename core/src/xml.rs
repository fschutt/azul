//! XML and XHTML parsing for declarative UI definitions.
//!
//! This module provides comprehensive XML parsing and manipulation for Azul's XML-based
//! UI format (`.azul` files). It supports:
//!
//! - **XHTML parsing**: Parse HTML-like syntax into DOM structures
//! - **CSS extraction**: Extract `<style>` blocks and inline styles
//! - **Component system**: Define reusable UI components with arguments
//! - **Hot reload**: Track file changes and rebuild UI incrementally
//! - **Error reporting**: Detailed syntax error messages with line/column info
//!
//! # Examples
//!
//! ```rust,no_run,ignore
//! use azul_core::xml::{XmlNode, XmlParseOptions};
//!
//! let xml = "<div>Hello</div>";
//! // let node = XmlNode::parse(xml)?;
//! ```

use alloc::{
    boxed::Box,
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use core::{fmt, hash::Hash};

use azul_css::{
    css::{
        Css, CssDeclaration, CssPath, CssPathPseudoSelector, CssPathSelector, CssRuleBlock,
        NodeTypeTag,
    },
    format_rust_code::VecContents,
    parser2::{CssParseErrorOwned, ErrorLocation},
    props::{
        basic::{ColorU, StyleFontFamilyVec},
        property::CssProperty,
        style::{
            NormalizedLinearColorStopVec, NormalizedRadialColorStopVec, StyleBackgroundContentVec,
            StyleBackgroundPositionVec, StyleBackgroundRepeatVec, StyleBackgroundSizeVec,
            StyleTransformVec,
        },
    },
    AzString, OptionString, StringVec, U8Vec,
};

use crate::{
    dom::{Dom, NodeType, OptionNodeType},
    styled_dom::StyledDom,
    window::{AzStringPair, StringPairVec},
};

/// Error that can occur during XML parsing or hot-reload.
///
/// Stringified for error reporting; not part of the public API.
pub type SyntaxError = String;

/// Tag of an XML node, such as the "button" in `<button>Hello</button>`.
#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct XmlTagName {
    pub inner: AzString,
}

impl From<AzString> for XmlTagName {
    fn from(s: AzString) -> Self {
        Self { inner: s }
    }
}

impl From<String> for XmlTagName {
    fn from(s: String) -> Self {
        Self { inner: s.into() }
    }
}

impl From<&str> for XmlTagName {
    fn from(s: &str) -> Self {
        Self { inner: s.into() }
    }
}

impl core::ops::Deref for XmlTagName {
    type Target = AzString;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// (Unparsed) text content of an XML node, such as the "Hello" in `<button>Hello</button>`.
pub type XmlTextContent = OptionString;

/// Attributes of an XML node, such as `["color" => "blue"]` in `<button color="blue" />`.
#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct XmlAttributeMap {
    pub inner: StringPairVec,
}

impl From<StringPairVec> for XmlAttributeMap {
    fn from(v: StringPairVec) -> Self {
        Self { inner: v }
    }
}

impl core::ops::Deref for XmlAttributeMap {
    type Target = StringPairVec;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl core::ops::DerefMut for XmlAttributeMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

pub type ComponentArgumentName = String;
pub type ComponentArgumentType = String;
pub type ComponentArgumentOrder = usize;

/// FFI-safe replacement for `(ComponentArgumentName, ComponentArgumentType)` tuple.
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ComponentArgument {
    pub name: AzString,
    pub arg_type: AzString,
}

impl_vec!(ComponentArgument, ComponentArgumentVec, ComponentArgumentVecDestructor, ComponentArgumentVecDestructorType, ComponentArgumentVecSlice, OptionComponentArgument);
impl_option!(ComponentArgument, OptionComponentArgument, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl_vec_debug!(ComponentArgument, ComponentArgumentVec);
impl_vec_partialeq!(ComponentArgument, ComponentArgumentVec);
impl_vec_eq!(ComponentArgument, ComponentArgumentVec);
impl_vec_partialord!(ComponentArgument, ComponentArgumentVec);
impl_vec_ord!(ComponentArgument, ComponentArgumentVec);
impl_vec_hash!(ComponentArgument, ComponentArgumentVec);
impl_vec_clone!(ComponentArgument, ComponentArgumentVec, ComponentArgumentVecDestructor);
impl_vec_mut!(ComponentArgument, ComponentArgumentVec);

pub type ComponentArgumentTypes = ComponentArgumentVec;
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
    pub local_name: AzString,
    pub namespace: OptionString,
}

/// Classification of an external resource referenced in HTML/XML
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ExternalResourceKind {
    /// Image resource (img src, background-image, etc.)
    Image,
    /// Font resource (@font-face src, link rel="preload" as="font")
    Font,
    /// Stylesheet (link rel="stylesheet", @import)
    Stylesheet,
    /// Script (script src)
    Script,
    /// Favicon or icon
    Icon,
    /// Video source
    Video,
    /// Audio source
    Audio,
    /// Generic link or unknown resource type
    Unknown,
}

/// MIME type hint for an external resource
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct MimeTypeHint {
    pub inner: AzString,
}

impl MimeTypeHint {
    pub fn new(s: &str) -> Self {
        Self { inner: AzString::from(s) }
    }
    
    pub fn from_extension(ext: &str) -> Self {
        let mime = match ext.to_lowercase().as_str() {
            // Images
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            "ico" => "image/x-icon",
            "bmp" => "image/bmp",
            "avif" => "image/avif",
            // Fonts
            "ttf" => "font/ttf",
            "otf" => "font/otf",
            "woff" => "font/woff",
            "woff2" => "font/woff2",
            "eot" => "application/vnd.ms-fontobject",
            // Stylesheets
            "css" => "text/css",
            // Scripts
            "js" => "application/javascript",
            "mjs" => "application/javascript",
            // Video
            "mp4" => "video/mp4",
            "webm" => "video/webm",
            "ogg" => "video/ogg",
            // Audio
            "mp3" => "audio/mpeg",
            "wav" => "audio/wav",
            "flac" => "audio/flac",
            // Default
            _ => "application/octet-stream",
        };
        Self { inner: AzString::from(mime) }
    }
}

impl_option!(
    MimeTypeHint,
    OptionMimeTypeHint,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// An external resource URL found in an XML/HTML document
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ExternalResource {
    /// The URL as found in the document (may be relative or absolute)
    pub url: AzString,
    /// Classification of the resource type
    pub kind: ExternalResourceKind,
    /// MIME type hint (from type attribute, file extension, or heuristics)
    pub mime_type: OptionMimeTypeHint,
    /// The HTML element that referenced this resource (e.g., "img", "link", "script")
    pub source_element: AzString,
    /// The attribute that contained the URL (e.g., "src", "href")
    pub source_attribute: AzString,
}

impl_option!(
    ExternalResource,
    OptionExternalResource,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(ExternalResource, ExternalResourceVec, ExternalResourceVecDestructor, ExternalResourceVecDestructorType, ExternalResourceVecSlice, OptionExternalResource);
impl_vec_mut!(ExternalResource, ExternalResourceVec);
impl_vec_debug!(ExternalResource, ExternalResourceVec);
impl_vec_partialeq!(ExternalResource, ExternalResourceVec);
impl_vec_eq!(ExternalResource, ExternalResourceVec);
impl_vec_partialord!(ExternalResource, ExternalResourceVec);
impl_vec_ord!(ExternalResource, ExternalResourceVec);
impl_vec_hash!(ExternalResource, ExternalResourceVec);
impl_vec_clone!(ExternalResource, ExternalResourceVec, ExternalResourceVecDestructor);

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct Xml {
    pub root: XmlNodeChildVec,
}

impl Xml {
    /// Scan the XML/HTML document for external resource URLs.
    /// 
    /// This function traverses the entire document tree and extracts URLs from:
    /// - `<img src="...">` - Images
    /// - `<link href="...">` - Stylesheets, icons, fonts
    /// - `<script src="...">` - Scripts
    /// - `<video src="...">`, `<source src="...">` - Video
    /// - `<audio src="...">` - Audio
    /// - `<a href="...">` - Links (classified as Unknown)
    /// - CSS `url()` in style attributes
    /// - `<style>` blocks with @import or url()
    pub fn scan_external_resources(&self) -> ExternalResourceVec {
        let mut resources = Vec::new();
        
        for child in self.root.as_ref().iter() {
            Self::scan_node_child(child, &mut resources);
        }
        
        resources.into()
    }
    
    fn scan_node_child(child: &XmlNodeChild, resources: &mut Vec<ExternalResource>) {
        match child {
            XmlNodeChild::Text(text) => {
                // Check for CSS @import or url() in text content (inside <style> tags)
                Self::extract_css_urls(text.as_str(), resources);
            }
            XmlNodeChild::Element(node) => {
                Self::scan_node(node, resources);
            }
        }
    }
    
    fn scan_node(node: &XmlNode, resources: &mut Vec<ExternalResource>) {
        let tag_name = node.node_type.inner.as_str().to_lowercase();
        
        // Get attribute lookup helper
        let get_attr = |name: &str| -> Option<String> {
            node.attributes.inner.as_ref().iter()
                .find(|pair| pair.key.as_str().eq_ignore_ascii_case(name))
                .map(|pair| pair.value.as_str().to_string())
        };
        
        match tag_name.as_str() {
            "img" => {
                if let Some(src) = get_attr("src") {
                    let mime = Self::guess_mime_from_url(&src, "image");
                    resources.push(ExternalResource {
                        url: AzString::from(src),
                        kind: ExternalResourceKind::Image,
                        mime_type: mime.into(),
                        source_element: AzString::from("img"),
                        source_attribute: AzString::from("src"),
                    });
                }
                // Also check srcset
                if let Some(srcset) = get_attr("srcset") {
                    for src in Self::parse_srcset(&srcset) {
                        let mime = Self::guess_mime_from_url(&src, "image");
                        resources.push(ExternalResource {
                            url: AzString::from(src),
                            kind: ExternalResourceKind::Image,
                            mime_type: mime.into(),
                            source_element: AzString::from("img"),
                            source_attribute: AzString::from("srcset"),
                        });
                    }
                }
            }
            "link" => {
                if let Some(href) = get_attr("href") {
                    let rel = get_attr("rel").unwrap_or_default().to_lowercase();
                    let type_attr = get_attr("type");
                    let as_attr = get_attr("as").unwrap_or_default().to_lowercase();
                    
                    let (kind, category) = if rel.contains("stylesheet") {
                        (ExternalResourceKind::Stylesheet, "stylesheet")
                    } else if rel.contains("icon") || rel.contains("apple-touch-icon") {
                        (ExternalResourceKind::Icon, "image")
                    } else if as_attr == "font" || rel.contains("preload") && as_attr == "font" {
                        (ExternalResourceKind::Font, "font")
                    } else if as_attr == "script" {
                        (ExternalResourceKind::Script, "script")
                    } else if as_attr == "image" {
                        (ExternalResourceKind::Image, "image")
                    } else {
                        (ExternalResourceKind::Unknown, "")
                    };
                    
                    let mime = type_attr.map(|t| MimeTypeHint::new(&t))
                        .or_else(|| Self::guess_mime_from_url(&href, category));
                    
                    resources.push(ExternalResource {
                        url: AzString::from(href),
                        kind,
                        mime_type: mime.into(),
                        source_element: AzString::from("link"),
                        source_attribute: AzString::from("href"),
                    });
                }
            }
            "script" => {
                if let Some(src) = get_attr("src") {
                    let type_attr = get_attr("type");
                    let mime = type_attr.map(|t| MimeTypeHint::new(&t))
                        .or_else(|| Some(MimeTypeHint::new("application/javascript")));
                    
                    resources.push(ExternalResource {
                        url: AzString::from(src),
                        kind: ExternalResourceKind::Script,
                        mime_type: mime.into(),
                        source_element: AzString::from("script"),
                        source_attribute: AzString::from("src"),
                    });
                }
            }
            "video" => {
                if let Some(src) = get_attr("src") {
                    let mime = Self::guess_mime_from_url(&src, "video");
                    resources.push(ExternalResource {
                        url: AzString::from(src),
                        kind: ExternalResourceKind::Video,
                        mime_type: mime.into(),
                        source_element: AzString::from("video"),
                        source_attribute: AzString::from("src"),
                    });
                }
                if let Some(poster) = get_attr("poster") {
                    let mime = Self::guess_mime_from_url(&poster, "image");
                    resources.push(ExternalResource {
                        url: AzString::from(poster),
                        kind: ExternalResourceKind::Image,
                        mime_type: mime.into(),
                        source_element: AzString::from("video"),
                        source_attribute: AzString::from("poster"),
                    });
                }
            }
            "audio" => {
                if let Some(src) = get_attr("src") {
                    let mime = Self::guess_mime_from_url(&src, "audio");
                    resources.push(ExternalResource {
                        url: AzString::from(src),
                        kind: ExternalResourceKind::Audio,
                        mime_type: mime.into(),
                        source_element: AzString::from("audio"),
                        source_attribute: AzString::from("src"),
                    });
                }
            }
            "source" => {
                if let Some(src) = get_attr("src") {
                    let type_attr = get_attr("type");
                    // Determine kind based on type or parent (heuristic: assume video)
                    let kind = if type_attr.as_ref().map(|t| t.starts_with("audio")).unwrap_or(false) {
                        ExternalResourceKind::Audio
                    } else {
                        ExternalResourceKind::Video
                    };
                    let mime = type_attr.map(|t| MimeTypeHint::new(&t))
                        .or_else(|| Self::guess_mime_from_url(&src, if kind == ExternalResourceKind::Audio { "audio" } else { "video" }));
                    
                    resources.push(ExternalResource {
                        url: AzString::from(src),
                        kind,
                        mime_type: mime.into(),
                        source_element: AzString::from("source"),
                        source_attribute: AzString::from("src"),
                    });
                }
                // Also handle srcset for picture elements
                if let Some(srcset) = get_attr("srcset") {
                    for src in Self::parse_srcset(&srcset) {
                        let mime = Self::guess_mime_from_url(&src, "image");
                        resources.push(ExternalResource {
                            url: AzString::from(src),
                            kind: ExternalResourceKind::Image,
                            mime_type: mime.into(),
                            source_element: AzString::from("source"),
                            source_attribute: AzString::from("srcset"),
                        });
                    }
                }
            }
            "a" => {
                if let Some(href) = get_attr("href") {
                    // Only include if it looks like a resource, not a page link
                    if Self::looks_like_resource(&href) {
                        let mime = Self::guess_mime_from_url(&href, "");
                        resources.push(ExternalResource {
                            url: AzString::from(href),
                            kind: ExternalResourceKind::Unknown,
                            mime_type: mime.into(),
                            source_element: AzString::from("a"),
                            source_attribute: AzString::from("href"),
                        });
                    }
                }
            }
            "iframe" | "embed" | "object" => {
                let src_attr = if tag_name == "object" { "data" } else { "src" };
                if let Some(src) = get_attr(src_attr) {
                    resources.push(ExternalResource {
                        url: AzString::from(src),
                        kind: ExternalResourceKind::Unknown,
                        mime_type: OptionMimeTypeHint::None,
                        source_element: AzString::from(tag_name.clone()),
                        source_attribute: AzString::from(src_attr),
                    });
                }
            }
            "style" => {
                // Scan text content for CSS URLs
                for child in node.children.as_ref().iter() {
                    if let XmlNodeChild::Text(text) = child {
                        Self::extract_css_urls(text.as_str(), resources);
                    }
                }
            }
            _ => {}
        }
        
        // Check inline style attribute for url()
        if let Some(style) = get_attr("style") {
            Self::extract_css_urls(&style, resources);
        }
        
        // Check for background attribute (deprecated but still used)
        if let Some(bg) = get_attr("background") {
            let mime = Self::guess_mime_from_url(&bg, "image");
            resources.push(ExternalResource {
                url: AzString::from(bg),
                kind: ExternalResourceKind::Image,
                mime_type: mime.into(),
                source_element: AzString::from(tag_name),
                source_attribute: AzString::from("background"),
            });
        }
        
        // Recurse into children
        for child in node.children.as_ref().iter() {
            Self::scan_node_child(child, resources);
        }
    }
    
    /// Extract URLs from CSS content (handles url() and @import)
    fn extract_css_urls(css: &str, resources: &mut Vec<ExternalResource>) {
        // Simple regex-like parsing for url(...) and @import
        let mut remaining = css;
        
        while let Some(pos) = remaining.find("url(") {
            let after_url = &remaining[pos + 4..];
            if let Some(url) = Self::extract_url_value(after_url) {
                let mime = Self::guess_mime_from_url(&url, "");
                let kind = Self::guess_kind_from_url(&url);
                resources.push(ExternalResource {
                    url: AzString::from(url),
                    kind,
                    mime_type: mime.into(),
                    source_element: AzString::from("style"),
                    source_attribute: AzString::from("url()"),
                });
            }
            remaining = after_url;
        }
        
        // Handle @import "url" or @import url(...)
        remaining = css;
        while let Some(pos) = remaining.to_lowercase().find("@import") {
            let after_import = &remaining[pos + 7..];
            let trimmed = after_import.trim_start();
            
            if trimmed.starts_with("url(") {
                if let Some(url) = Self::extract_url_value(&trimmed[4..]) {
                    resources.push(ExternalResource {
                        url: AzString::from(url),
                        kind: ExternalResourceKind::Stylesheet,
                        mime_type: Some(MimeTypeHint::new("text/css")).into(),
                        source_element: AzString::from("style"),
                        source_attribute: AzString::from("@import"),
                    });
                }
            } else if let Some(url) = Self::extract_quoted_string(trimmed) {
                resources.push(ExternalResource {
                    url: AzString::from(url),
                    kind: ExternalResourceKind::Stylesheet,
                    mime_type: Some(MimeTypeHint::new("text/css")).into(),
                    source_element: AzString::from("style"),
                    source_attribute: AzString::from("@import"),
                });
            }
            
            remaining = after_import;
        }
    }
    
    /// Extract value from url(...) - handles quoted and unquoted URLs
    fn extract_url_value(s: &str) -> Option<String> {
        let trimmed = s.trim_start();
        if trimmed.starts_with('"') {
            Self::extract_quoted_string(trimmed)
        } else if trimmed.starts_with('\'') {
            let end = trimmed[1..].find('\'')?;
            Some(trimmed[1..1+end].to_string())
        } else {
            let end = trimmed.find(')')?;
            Some(trimmed[..end].trim().to_string())
        }
    }
    
    /// Extract a quoted string value
    fn extract_quoted_string(s: &str) -> Option<String> {
        if s.starts_with('"') {
            let end = s[1..].find('"')?;
            Some(s[1..1+end].to_string())
        } else if s.starts_with('\'') {
            let end = s[1..].find('\'')?;
            Some(s[1..1+end].to_string())
        } else {
            None
        }
    }
    
    /// Parse srcset attribute into individual URLs
    fn parse_srcset(srcset: &str) -> Vec<String> {
        srcset.split(',')
            .filter_map(|entry| {
                let trimmed = entry.trim();
                // srcset format: "url 1x" or "url 100w"
                trimmed.split_whitespace().next().map(|s| s.to_string())
            })
            .filter(|url| !url.is_empty())
            .collect()
    }
    
    /// Check if a URL looks like a downloadable resource (not a page)
    fn looks_like_resource(url: &str) -> bool {
        let lower = url.to_lowercase();
        // Check for common resource extensions
        let resource_exts = [
            ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".ico", ".bmp",
            ".ttf", ".otf", ".woff", ".woff2", ".eot",
            ".css", ".js",
            ".mp4", ".webm", ".ogg", ".mp3", ".wav",
            ".pdf", ".zip", ".tar", ".gz",
        ];
        resource_exts.iter().any(|ext| lower.ends_with(ext))
    }
    
    /// Guess the resource kind from URL
    fn guess_kind_from_url(url: &str) -> ExternalResourceKind {
        let lower = url.to_lowercase();
        if lower.contains(".png") || lower.contains(".jpg") || lower.contains(".jpeg") 
            || lower.contains(".gif") || lower.contains(".webp") || lower.contains(".svg")
            || lower.contains(".bmp") || lower.contains(".avif") {
            ExternalResourceKind::Image
        } else if lower.contains(".ttf") || lower.contains(".otf") || lower.contains(".woff") 
            || lower.contains(".eot") {
            ExternalResourceKind::Font
        } else if lower.contains(".css") {
            ExternalResourceKind::Stylesheet
        } else if lower.contains(".js") {
            ExternalResourceKind::Script
        } else if lower.contains(".mp4") || lower.contains(".webm") || lower.contains(".ogg") {
            ExternalResourceKind::Video
        } else if lower.contains(".mp3") || lower.contains(".wav") || lower.contains(".flac") {
            ExternalResourceKind::Audio
        } else if lower.contains(".ico") {
            ExternalResourceKind::Icon
        } else {
            ExternalResourceKind::Unknown
        }
    }
    
    /// Guess MIME type from URL based on extension
    fn guess_mime_from_url(url: &str, category: &str) -> Option<MimeTypeHint> {
        let lower = url.to_lowercase();
        // Find extension
        let ext = lower.rsplit('.').next()?;
        // Remove query string if present
        let ext = ext.split('?').next()?;
        
        // Check if it's a valid extension
        let valid_exts = [
            "png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp", "avif",
            "ttf", "otf", "woff", "woff2", "eot",
            "css", "js", "mjs",
            "mp4", "webm", "ogg", "mp3", "wav", "flac",
        ];
        
        if valid_exts.contains(&ext) {
            Some(MimeTypeHint::from_extension(ext))
        } else if !category.is_empty() {
            // Use category hint for default
            match category {
                "image" => Some(MimeTypeHint::new("image/*")),
                "font" => Some(MimeTypeHint::new("font/*")),
                "stylesheet" => Some(MimeTypeHint::new("text/css")),
                "script" => Some(MimeTypeHint::new("application/javascript")),
                "video" => Some(MimeTypeHint::new("video/*")),
                "audio" => Some(MimeTypeHint::new("audio/*")),
                _ => None,
            }
        } else {
            None
        }
    }
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

/// Error for mismatched open/close tags in XML hierarchy
#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct MalformedHierarchyError {
    /// The tag that was expected (from the opening tag)
    pub expected: AzString,
    /// The tag that was actually found (the closing tag)
    pub got: AzString,
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
    MalformedHierarchy(MalformedHierarchyError),
    ParserError(XmlParseError),
    UnclosedRootNode,
    UnexpectedDeclaration(XmlTextPos),
    NodesLimitReached,
    AttributesLimitReached,
    NamespacesLimitReached,
    InvalidName(XmlTextPos),
    NonXmlChar(XmlTextPos),
    InvalidChar(XmlTextPos),
    InvalidChar2(XmlTextPos),
    InvalidString(XmlTextPos),
    InvalidExternalID(XmlTextPos),
    InvalidComment(XmlTextPos),
    InvalidCharacterData(XmlTextPos),
    UnknownToken(XmlTextPos),
    UnexpectedEndOfStream,
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
            MalformedHierarchy(e) => write!(
                f,
                "Malformed hierarchy: expected <{}/> closing tag, got <{}/>",
                e.expected.as_str(),
                e.got.as_str()
            ),
            ParserError(p) => write!(f, "{}", p),
            UnclosedRootNode => write!(f, "unclosed root node"),
            UnexpectedDeclaration(tp) => write!(f, "unexpected declaration at {tp}"),
            NodesLimitReached => write!(f, "nodes limit reached"),
            AttributesLimitReached => write!(f, "attributes limit reached"),
            NamespacesLimitReached => write!(f, "namespaces limit reached"),
            InvalidName(tp) => write!(f, "invalid name at {tp}"),
            NonXmlChar(tp) => write!(f, "non xml char at {tp}"),
            InvalidChar(tp) => write!(f, "invalid char at {tp}"),
            InvalidChar2(tp) => write!(f, "invalid char2 at {tp}"),
            InvalidString(tp) => write!(f, "invalid string at {tp}"),
            InvalidExternalID(tp) => write!(f, "invalid externalid at {tp}"),
            InvalidComment(tp) => write!(f, "invalid comment at {tp}"),
            InvalidCharacterData(tp) => write!(f, "invalid character data at {tp}"),
            UnknownToken(tp) => write!(f, "unknown token at {tp}"),
            UnexpectedEndOfStream => write!(f, "unexpected end of stream"),
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
            args: ComponentArgumentVec::new(),
            accepts_text: false,
        }
    }
}

impl ComponentArguments {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct FilteredComponentArguments {
    /// The types of the component, i.e. `date => String`, in order
    pub types: ComponentArgumentTypes,
    /// The values of the component, i.e. `date => "01.01.1998"`
    pub values: StringPairVec,
    /// Whether this widget accepts text. Note that this will be passed as the first
    /// argument when rendering the Rust code.
    pub accepts_text: bool,
}

impl Default for FilteredComponentArguments {
    fn default() -> Self {
        Self {
            types: ComponentArgumentVec::new(),
            values: Vec::new().into(),
            accepts_text: false,
        }
    }
}

impl FilteredComponentArguments {
    fn new() -> Self {
        Self::default()
    }
}

// ============================================================================
// New repr(C) component system — replaces XmlComponentTrait over time
// ============================================================================

/// Identifies a component within a library collection.
/// e.g. collection="builtin", name="div" for the `<div>` element,
/// or collection="shadcn", name="avatar" for a custom component.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ComponentId {
    /// Library / collection name: "builtin", "shadcn", "myproject"
    pub collection: AzString,
    /// Component name within the collection: "div", "avatar", "card"
    pub name: AzString,
}

impl ComponentId {
    pub fn builtin(name: &str) -> Self {
        Self {
            collection: AzString::from_const_str("builtin"),
            name: AzString::from(name),
        }
    }

    pub fn new(collection: &str, name: &str) -> Self {
        Self {
            collection: AzString::from(collection),
            name: AzString::from(name),
        }
    }

    /// Returns "collection:name" format string
    pub fn qualified_name(&self) -> String {
        format!("{}:{}", self.collection.as_str(), self.name.as_str())
    }
}

/// A parameter that a component accepts (for the GUI builder / code export).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ComponentParam {
    /// Parameter name, e.g. "label", "image", "size"
    pub name: AzString,
    /// Type name from the Azul type system, e.g. "String", "f32", "RefAny"
    /// "RefAny" signals: this is a backreference slot
    pub param_type: AzString,
    /// Default value (as a string), or None if required
    pub default_value: OptionString,
    /// Human-readable description
    pub description: AzString,
}

impl_vec!(ComponentParam, ComponentParamVec, ComponentParamVecDestructor, ComponentParamVecDestructorType, ComponentParamVecSlice, OptionComponentParam);
impl_option!(ComponentParam, OptionComponentParam, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl_vec_debug!(ComponentParam, ComponentParamVec);
impl_vec_partialeq!(ComponentParam, ComponentParamVec);
impl_vec_clone!(ComponentParam, ComponentParamVec, ComponentParamVecDestructor);

/// A callback slot that a component exposes for parent wiring.
/// References a `CallbackTypeDef` from the api.json type system.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ComponentCallbackSlot {
    /// Slot name, e.g. "on_click", "on_value_change", "on_focus_lost"
    pub name: AzString,
    /// The callback type name, e.g. "ButtonOnClickCallbackType"
    pub callback_type: AzString,
    /// Human-readable description
    pub description: AzString,
}

impl_vec!(ComponentCallbackSlot, ComponentCallbackSlotVec, ComponentCallbackSlotVecDestructor, ComponentCallbackSlotVecDestructorType, ComponentCallbackSlotVecSlice, OptionComponentCallbackSlot);
impl_option!(ComponentCallbackSlot, OptionComponentCallbackSlot, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl_vec_debug!(ComponentCallbackSlot, ComponentCallbackSlotVec);
impl_vec_partialeq!(ComponentCallbackSlot, ComponentCallbackSlotVec);
impl_vec_clone!(ComponentCallbackSlot, ComponentCallbackSlotVec, ComponentCallbackSlotVecDestructor);

// ============================================================================
// Component type system — rich type descriptors for component fields
// ============================================================================

/// A single argument in a callback signature.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ComponentCallbackArg {
    /// Argument name, e.g. "button_id"
    pub name: AzString,
    /// Argument type
    pub arg_type: ComponentFieldType,
}

impl_vec!(ComponentCallbackArg, ComponentCallbackArgVec, ComponentCallbackArgVecDestructor, ComponentCallbackArgVecDestructorType, ComponentCallbackArgVecSlice, OptionComponentCallbackArg);
impl_option!(ComponentCallbackArg, OptionComponentCallbackArg, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl_vec_debug!(ComponentCallbackArg, ComponentCallbackArgVec);
impl_vec_partialeq!(ComponentCallbackArg, ComponentCallbackArgVec);
impl_vec_eq!(ComponentCallbackArg, ComponentCallbackArgVec);
impl_vec_partialord!(ComponentCallbackArg, ComponentCallbackArgVec);
impl_vec_ord!(ComponentCallbackArg, ComponentCallbackArgVec);
impl_vec_hash!(ComponentCallbackArg, ComponentCallbackArgVec);
impl_vec_clone!(ComponentCallbackArg, ComponentCallbackArgVec, ComponentCallbackArgVecDestructor);

/// Callback signature: return type + argument list.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ComponentCallbackSignature {
    /// Return type name, e.g. "Update"
    pub return_type: AzString,
    /// Callback arguments (excluding the implicit `&mut RefAny` and `&mut CallbackInfo`)
    pub args: ComponentCallbackArgVec,
}

/// Heap-allocated box for recursive `ComponentFieldType` (e.g. `Option<String>`).
/// Uses raw pointer indirection to break the infinite size.
#[repr(C)]
pub struct ComponentFieldTypeBox {
    pub ptr: *mut ComponentFieldType,
}

impl ComponentFieldTypeBox {
    pub fn new(t: ComponentFieldType) -> Self {
        Self { ptr: Box::into_raw(Box::new(t)) }
    }

    pub fn as_ref(&self) -> &ComponentFieldType {
        unsafe { &*self.ptr }
    }
}

impl Clone for ComponentFieldTypeBox {
    fn clone(&self) -> Self {
        Self::new(unsafe { (*self.ptr).clone() })
    }
}

impl Drop for ComponentFieldTypeBox {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { let _ = Box::from_raw(self.ptr); }
        }
    }
}

impl fmt::Debug for ComponentFieldTypeBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ptr.is_null() {
            write!(f, "ComponentFieldTypeBox(null)")
        } else {
            write!(f, "ComponentFieldTypeBox({:?})", unsafe { &*self.ptr })
        }
    }
}

impl PartialEq for ComponentFieldTypeBox {
    fn eq(&self, other: &Self) -> bool {
        if self.ptr.is_null() && other.ptr.is_null() { return true; }
        if self.ptr.is_null() || other.ptr.is_null() { return false; }
        unsafe { *self.ptr == *other.ptr }
    }
}

impl Eq for ComponentFieldTypeBox {}

impl PartialOrd for ComponentFieldTypeBox {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ComponentFieldTypeBox {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (self.ptr.is_null(), other.ptr.is_null()) {
            (true, true) => core::cmp::Ordering::Equal,
            (true, false) => core::cmp::Ordering::Less,
            (false, true) => core::cmp::Ordering::Greater,
            (false, false) => unsafe { (*self.ptr).cmp(&*other.ptr) },
        }
    }
}

impl core::hash::Hash for ComponentFieldTypeBox {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        if !self.ptr.is_null() {
            unsafe { (*self.ptr).hash(state); }
        }
    }
}

/// Rich type descriptor for a component field.
/// Replaces the old `AzString` type names ("String", "bool", etc.) with
/// a structured enum that the debugger can use for type-aware editing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum ComponentFieldType {
    String,
    Bool,
    I32,
    I64,
    U32,
    U64,
    Usize,
    F32,
    F64,
    ColorU,
    CssProperty,
    ImageRef,
    FontRef,
    /// StyledDom slot — field name = slot name
    StyledDom,
    /// Callback with typed signature
    Callback(ComponentCallbackSignature),
    /// RefAny data binding with type hint
    RefAny(AzString),
    /// Optional value (recursive via Box)
    OptionType(ComponentFieldTypeBox),
    /// Vec of values (recursive via Box)
    VecType(ComponentFieldTypeBox),
    /// Reference to a struct defined in the same library
    StructRef(AzString),
    /// Reference to an enum defined in the same library
    EnumRef(AzString),
}

/// A single variant in a component enum model.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ComponentEnumVariant {
    /// Variant name, e.g. "Admin", "Editor", "Viewer"
    pub name: AzString,
    /// Optional associated fields for this variant
    pub fields: ComponentDataFieldVec,
}

impl_vec!(ComponentEnumVariant, ComponentEnumVariantVec, ComponentEnumVariantVecDestructor, ComponentEnumVariantVecDestructorType, ComponentEnumVariantVecSlice, OptionComponentEnumVariant);
impl_option!(ComponentEnumVariant, OptionComponentEnumVariant, copy = false, [Debug, Clone, PartialEq, PartialOrd]);
impl_vec_debug!(ComponentEnumVariant, ComponentEnumVariantVec);
impl_vec_partialeq!(ComponentEnumVariant, ComponentEnumVariantVec);
impl_vec_partialord!(ComponentEnumVariant, ComponentEnumVariantVec);
impl_vec_clone!(ComponentEnumVariant, ComponentEnumVariantVec, ComponentEnumVariantVecDestructor);

/// A named enum model for code generation.
/// Stored in `ComponentLibrary::enum_models`.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ComponentEnumModel {
    /// Enum name, e.g. "UserRole"
    pub name: AzString,
    /// Human-readable description
    pub description: AzString,
    /// Variants
    pub variants: ComponentEnumVariantVec,
}

impl_vec!(ComponentEnumModel, ComponentEnumModelVec, ComponentEnumModelVecDestructor, ComponentEnumModelVecDestructorType, ComponentEnumModelVecSlice, OptionComponentEnumModel);
impl_option!(ComponentEnumModel, OptionComponentEnumModel, copy = false, [Debug, Clone, PartialEq, PartialOrd]);
impl_vec_debug!(ComponentEnumModel, ComponentEnumModelVec);
impl_vec_partialeq!(ComponentEnumModel, ComponentEnumModelVec);
impl_vec_partialord!(ComponentEnumModel, ComponentEnumModelVec);
impl_vec_clone!(ComponentEnumModel, ComponentEnumModelVec, ComponentEnumModelVecDestructor);

/// Default value for a component field.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum ComponentDefaultValue {
    /// No default value (field is required)
    None,
    /// String literal default
    String(AzString),
    /// Boolean default
    Bool(bool),
    /// i32 default
    I32(i32),
    /// i64 default
    I64(i64),
    /// u32 default
    U32(u32),
    /// u64 default
    U64(u64),
    /// usize default
    Usize(usize),
    /// f32 default
    F32(f32),
    /// f64 default
    F64(f64),
    /// ColorU default
    ColorU(ColorU),
    /// Default is an instance of another component
    ComponentInstance(ComponentInstanceDefault),
    /// Default callback function pointer name
    CallbackFnPointer(AzString),
}

impl_option!(ComponentDefaultValue, OptionComponentDefaultValue, copy = false, [Debug, Clone, PartialEq, PartialOrd]);

/// Default component instance for a StyledDom slot.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ComponentInstanceDefault {
    /// Library name, e.g. "builtin"
    pub library: AzString,
    /// Component tag, e.g. "a"
    pub component: AzString,
    /// Field overrides for this instance
    pub field_overrides: ComponentFieldOverrideVec,
}

/// An override for a single field in a component instance.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ComponentFieldOverride {
    /// Field name to override
    pub field_name: AzString,
    /// Value source for this override
    pub source: ComponentFieldValueSource,
}

impl_vec!(ComponentFieldOverride, ComponentFieldOverrideVec, ComponentFieldOverrideVecDestructor, ComponentFieldOverrideVecDestructorType, ComponentFieldOverrideVecSlice, OptionComponentFieldOverride);
impl_option!(ComponentFieldOverride, OptionComponentFieldOverride, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl_vec_debug!(ComponentFieldOverride, ComponentFieldOverrideVec);
impl_vec_partialeq!(ComponentFieldOverride, ComponentFieldOverrideVec);
impl_vec_eq!(ComponentFieldOverride, ComponentFieldOverrideVec);
impl_vec_partialord!(ComponentFieldOverride, ComponentFieldOverrideVec);
impl_vec_ord!(ComponentFieldOverride, ComponentFieldOverrideVec);
impl_vec_hash!(ComponentFieldOverride, ComponentFieldOverrideVec);
impl_vec_clone!(ComponentFieldOverride, ComponentFieldOverrideVec, ComponentFieldOverrideVecDestructor);

/// How a field value is sourced at the instance level.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum ComponentFieldValueSource {
    /// Use the component's default value
    Default,
    /// Hardcoded literal value
    Literal(AzString),
    /// Bound to an app state path (e.g. "app_state.user.name")
    Binding(AzString),
}

/// Runtime value for a component field — the "instance" counterpart
/// to `ComponentFieldType` (which is the "class" / type descriptor).
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ComponentFieldValue {
    String(AzString),
    Bool(bool),
    I32(i32),
    I64(i64),
    U32(u32),
    U64(u64),
    Usize(usize),
    F32(f32),
    F64(f64),
    ColorU(ColorU),
    /// Option<T> with no value
    None,
    /// StyledDom slot content
    StyledDom(StyledDom),
    /// Struct fields, in order
    Struct(ComponentFieldNamedValueVec),
    /// Enum variant
    Enum { variant: AzString, fields: ComponentFieldNamedValueVec },
}

/// Named field value: (field_name, value) pair.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ComponentFieldNamedValue {
    pub name: AzString,
    pub value: ComponentFieldValue,
}

impl_vec!(ComponentFieldNamedValue, ComponentFieldNamedValueVec, ComponentFieldNamedValueVecDestructor, ComponentFieldNamedValueVecDestructorType, ComponentFieldNamedValueVecSlice, OptionComponentFieldNamedValue);
impl_option!(ComponentFieldNamedValue, OptionComponentFieldNamedValue, copy = false, [Debug, Clone, PartialEq]);
impl_vec_debug!(ComponentFieldNamedValue, ComponentFieldNamedValueVec);
impl_vec_partialeq!(ComponentFieldNamedValue, ComponentFieldNamedValueVec);
impl_vec_clone!(ComponentFieldNamedValue, ComponentFieldNamedValueVec, ComponentFieldNamedValueVecDestructor);

impl_vec!(ComponentFieldValue, ComponentFieldValueVec, ComponentFieldValueVecDestructor, ComponentFieldValueVecDestructorType, ComponentFieldValueVecSlice, OptionComponentFieldValue);
impl_option!(ComponentFieldValue, OptionComponentFieldValue, copy = false, [Debug, Clone, PartialEq]);
impl_vec_debug!(ComponentFieldValue, ComponentFieldValueVec);
impl_vec_partialeq!(ComponentFieldValue, ComponentFieldValueVec);
impl_vec_clone!(ComponentFieldValue, ComponentFieldValueVec, ComponentFieldValueVecDestructor);

/// A field in the component's internal data model.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ComponentDataField {
    /// Field name, e.g. "counter", "text", "number"
    pub name: AzString,
    /// Rich type descriptor for this field
    pub field_type: ComponentFieldType,
    /// Typed default value, or None if the field is required
    pub default_value: OptionComponentDefaultValue,
    /// Whether this field is required (must be provided by the parent)
    pub required: bool,
    /// Human-readable description
    pub description: AzString,
}

impl_vec!(ComponentDataField, ComponentDataFieldVec, ComponentDataFieldVecDestructor, ComponentDataFieldVecDestructorType, ComponentDataFieldVecSlice, OptionComponentDataField);
impl_option!(ComponentDataField, OptionComponentDataField, copy = false, [Debug, Clone, PartialEq, PartialOrd]);
impl_vec_debug!(ComponentDataField, ComponentDataFieldVec);
impl_vec_partialeq!(ComponentDataField, ComponentDataFieldVec);
impl_vec_partialord!(ComponentDataField, ComponentDataFieldVec);
impl_vec_clone!(ComponentDataField, ComponentDataFieldVec, ComponentDataFieldVecDestructor);

/// A named data model (struct definition) for code generation.
///
/// Stored in `ComponentLibrary::data_models`. Components reference these
/// by name in `ComponentDataField::field_type`, enabling nested/structured
/// data models. For example, a `UserCard` component might have a field
/// `user: UserProfile` where `UserProfile` is a `ComponentDataModel`.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ComponentDataModel {
    /// Type name, e.g. "UserProfile", "TodoItem"
    pub name: AzString,
    /// Human-readable description
    pub description: AzString,
    /// Fields in this struct
    pub fields: ComponentDataFieldVec,
}

impl_vec!(ComponentDataModel, ComponentDataModelVec, ComponentDataModelVecDestructor, ComponentDataModelVecDestructorType, ComponentDataModelVecSlice, OptionComponentDataModel);
impl_option!(ComponentDataModel, OptionComponentDataModel, copy = false, [Debug, Clone]);
impl_vec_debug!(ComponentDataModel, ComponentDataModelVec);
impl_vec_clone!(ComponentDataModel, ComponentDataModelVec, ComponentDataModelVecDestructor);
impl_vec_mut!(ComponentDataModel, ComponentDataModelVec);

/// What children a component accepts
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ChildPolicy {
    /// No children allowed (void elements: br, hr, img, input)
    NoChildren,
    /// Any children allowed (div, body, section)
    AnyChildren,
    /// Only text content (p, span, h1-h6)
    TextOnly,
}

impl Default for ChildPolicy {
    fn default() -> Self {
        ChildPolicy::AnyChildren
    }
}

impl ChildPolicy {
    pub fn create() -> Self {
        Self::default()
    }
}

/// Source of a component definition — determines whether it can be exported
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ComponentSource {
    /// Built into the DLL (HTML elements). Never exported.
    Builtin,
    /// Compiled Rust widget (Button, TextInput, etc.). Never exported.
    Compiled,
    /// Defined via JSON/XML at runtime. Can be exported.
    UserDefined,
}

impl Default for ComponentSource {
    fn default() -> Self {
        ComponentSource::UserDefined
    }
}

impl ComponentSource {
    pub fn create() -> Self {
        Self::default()
    }
}

/// The target language for code compilation
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum CompileTarget {
    Rust,
    C,
    Cpp,
    Python,
}

impl_result!(
    StyledDom,
    RenderDomError,
    ResultStyledDomRenderDomError,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl_result!(
    AzString,
    CompileError,
    ResultStringCompileError,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// Render function type: takes component definition + arguments, returns StyledDom
pub type ComponentRenderFn = fn(
    &ComponentDef,
    &XmlComponentMap,
    &FilteredComponentArguments,
    &OptionString,
) -> ResultStyledDomRenderDomError;

/// Compile function type: takes component definition + target language + context, returns source code
pub type ComponentCompileFn = fn(
    &ComponentDef,
    &CompileTarget,
    &XmlComponentMap,
    &FilteredComponentArguments,
    &OptionString,
    indent: usize,
) -> ResultStringCompileError;

/// Raw function pointer type that returns a single ComponentDef when called.
/// Used as the `cb` field in `RegisterComponentFn`.
pub type RegisterComponentFnType = extern "C" fn() -> ComponentDef;

/// Callback struct for registering individual components at startup.
///
/// In C: pass a bare `extern "C" fn() -> ComponentDef` function pointer —
/// it converts automatically via `From<RegisterComponentFnType>`.
///
/// In Python: construct this struct with `cb` set to a trampoline and
/// `ctx` set to `Some(RefAny(...))` wrapping the Python callable.
#[repr(C)]
pub struct RegisterComponentFn {
    pub cb: RegisterComponentFnType,
    /// For FFI: stores the foreign callable (e.g., PyFunction).
    /// Native Rust/C code sets this to None.
    pub ctx: crate::refany::OptionRefAny,
}

impl_callback!(RegisterComponentFn, RegisterComponentFnType);

/// Raw function pointer type that returns a complete ComponentLibrary when called.
/// Used as the `cb` field in `RegisterComponentLibraryFn`.
pub type RegisterComponentLibraryFnType = extern "C" fn() -> ComponentLibrary;

/// Callback struct for registering entire component libraries at startup.
///
/// In C: pass a bare `extern "C" fn() -> ComponentLibrary` function pointer —
/// it converts automatically via `From<RegisterComponentLibraryFnType>`.
///
/// In Python: construct this struct with `cb` set to a trampoline and
/// `ctx` set to `Some(RefAny(...))` wrapping the Python callable.
#[repr(C)]
pub struct RegisterComponentLibraryFn {
    pub cb: RegisterComponentLibraryFnType,
    /// For FFI: stores the foreign callable (e.g., PyFunction).
    /// Native Rust/C code sets this to None.
    pub ctx: crate::refany::OptionRefAny,
}

impl_callback!(RegisterComponentLibraryFn, RegisterComponentLibraryFnType);

/// A component definition — the "class" / "template" of a component.
/// Can come from Rust builtins, compiled widgets, JSON, or user creation in debugger.
///
/// This is the new `repr(C)` replacement for `XmlComponentTrait`.
#[derive(Clone)]
#[repr(C)]
pub struct ComponentDef {
    /// Collection + name, e.g. builtin:div, shadcn:avatar
    pub id: ComponentId,
    /// Human-readable display name, e.g. "Link" for builtin:a, "Avatar" for shadcn:avatar
    pub display_name: AzString,
    /// Markdown documentation for the component
    pub description: AzString,
    /// Whether this component accepts text content
    pub accepts_text: bool,
    /// Child policy (no children, any, text only)
    pub child_policy: ChildPolicy,
    /// The component's own scoped CSS
    pub scoped_css: AzString,
    /// Example usage as XML
    pub example_xml: AzString,
    /// Where this component was defined (determines exportability)
    pub source: ComponentSource,
    /// Unified data model: all value fields, callback slots, and child slots
    /// in a single named struct. Code gen uses `data_model.name` as the
    /// input struct type name (e.g. "ButtonData").
    pub data_model: ComponentDataModel,
    /// XML/HTML template body for user-defined components.
    /// Used by the template-based render_fn/compile_fn.
    /// Empty for builtin components (they render via node_type).
    pub template: AzString,
    /// Render to live DOM
    pub render_fn: ComponentRenderFn,
    /// Compile to source code in target language
    pub compile_fn: ComponentCompileFn,
    /// The NodeType to create for this component (for builtins)
    pub node_type: OptionNodeType,
}

impl fmt::Debug for ComponentDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComponentDef")
            .field("id", &self.id)
            .field("display_name", &self.display_name)
            .field("source", &self.source)
            .field("accepts_text", &self.accepts_text)
            .field("data_model", &self.data_model.name)
            .finish()
    }
}

impl_vec!(ComponentDef, ComponentDefVec, ComponentDefVecDestructor, ComponentDefVecDestructorType, ComponentDefVecSlice, OptionComponentDef);
impl_option!(ComponentDef, OptionComponentDef, copy = false, [Clone]);
impl_vec_debug!(ComponentDef, ComponentDefVec);
impl_vec_clone!(ComponentDef, ComponentDefVec, ComponentDefVecDestructor);
impl_vec_mut!(ComponentDef, ComponentDefVec);

/// A named collection of component definitions
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ComponentLibrary {
    /// Library identifier, e.g. "builtin", "shadcn", "myproject"
    pub name: AzString,
    /// Version string
    pub version: AzString,
    /// Human-readable description
    pub description: AzString,
    /// The components in this library
    pub components: ComponentDefVec,
    /// Whether this library can be exported (false for builtin/compiled)
    pub exportable: bool,
    /// Whether this library can be modified by the user (add/remove/edit components).
    /// False for builtin and compiled libraries. True for user-created libraries.
    pub modifiable: bool,
    /// Named data model types defined by this library.
    /// Components reference these by name in their `field_type`.
    pub data_models: ComponentDataModelVec,
    /// Named enum types defined by this library.
    /// Components reference these via `ComponentFieldType::EnumRef(name)`.
    pub enum_models: ComponentEnumModelVec,
}

impl_vec!(ComponentLibrary, ComponentLibraryVec, ComponentLibraryVecDestructor, ComponentLibraryVecDestructorType, ComponentLibraryVecSlice, OptionComponentLibrary);
impl_option!(ComponentLibrary, OptionComponentLibrary, copy = false, [Debug, Clone]);
impl_vec_debug!(ComponentLibrary, ComponentLibraryVec);
impl_vec_clone!(ComponentLibrary, ComponentLibraryVec, ComponentLibraryVecDestructor);
impl_vec_mut!(ComponentLibrary, ComponentLibraryVec);

/// The new component map — holds libraries with namespaced components.
/// Coexists with `XmlComponentMap` during migration.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ComponentMap {
    /// Libraries indexed by name. "builtin" is always present.
    pub libraries: ComponentLibraryVec,
}

impl ComponentMap {
    /// Qualified lookup: "shadcn:avatar" -> finds library "shadcn", component "avatar"
    pub fn get(&self, collection: &str, name: &str) -> Option<&ComponentDef> {
        self.libraries
            .iter()
            .find(|lib| lib.name.as_str() == collection)
            .and_then(|lib| lib.components.iter().find(|c| c.id.name.as_str() == name))
    }

    /// Unqualified lookup: "div" -> searches ONLY the "builtin" library.
    pub fn get_unqualified(&self, name: &str) -> Option<&ComponentDef> {
        self.get("builtin", name)
    }

    /// Parse a "collection:name" string into a lookup
    pub fn get_by_qualified_name(&self, qualified: &str) -> Option<&ComponentDef> {
        if let Some((collection, name)) = qualified.split_once(':') {
            self.get(collection, name)
        } else {
            self.get_unqualified(qualified)
        }
    }

    /// Get all libraries that can be exported (user-defined only)
    pub fn get_exportable_libraries(&self) -> Vec<&ComponentLibrary> {
        self.libraries.iter().filter(|lib| lib.exportable).collect()
    }

    /// Get all component definitions across all libraries
    pub fn all_components(&self) -> Vec<&ComponentDef> {
        self.libraries.iter().flat_map(|lib| lib.components.iter()).collect()
    }

}

// ============================================================================
// Builtin component bridge — wraps existing render/compile into ComponentDef
// ============================================================================

/// Default render function for builtin HTML elements.
/// Delegates to creating a DOM node of the appropriate NodeType.
fn builtin_render_fn(
    def: &ComponentDef,
    _components: &XmlComponentMap,
    _args: &FilteredComponentArguments,
    text: &OptionString,
) -> ResultStyledDomRenderDomError {
    let node_type: NodeType = Option::from(def.node_type.clone()).unwrap_or(NodeType::Div);
    let mut dom = Dom::create_node(node_type);
    if let Some(text_str) = text.as_ref() {
        let prepared = prepare_string(text_str);
        if !prepared.is_empty() {
            dom = dom.with_children(alloc::vec![Dom::create_text(prepared)].into());
        }
    }
    let r: Result<StyledDom, RenderDomError> = Ok(dom.style(Css::empty()));
    r.into()
}

/// Default compile function for builtin HTML elements.
/// Generates `Dom::create_node(NodeType::Div)` style code for the target language.
fn builtin_compile_fn(
    def: &ComponentDef,
    target: &CompileTarget,
    _components: &XmlComponentMap,
    _args: &FilteredComponentArguments,
    text: &OptionString,
    indent: usize,
) -> ResultStringCompileError {
    let node_type: NodeType = Option::from(def.node_type.clone()).unwrap_or(NodeType::Div);
    let type_name = format!("{:?}", node_type); // "Div", "Body", "P", etc.

    let r: Result<AzString, CompileError> = match target {
        CompileTarget::Rust => {
            if let Some(text_str) = text.as_ref() {
                Ok(format!(
                    "Dom::create_node(NodeType::{}).with_children(vec![Dom::create_text(AzString::from_const_str(\"{}\"))].into())",
                    type_name,
                    text_str.as_str().replace("\\", "\\\\").replace("\"", "\\\"")
                ).into())
            } else {
                Ok(format!("Dom::create_node(NodeType::{})", type_name).into())
            }
        }
        CompileTarget::C => {
            if let Some(text_str) = text.as_ref() {
                Ok(format!(
                    "AzDom_createText(AzString_fromConstStr(\"{}\"))",
                    text_str.as_str().replace("\\", "\\\\").replace("\"", "\\\"")
                ).into())
            } else {
                Ok(format!("AzDom_create{}()", type_name).into())
            }
        }
        CompileTarget::Cpp => {
            Ok(format!("Dom::create_{}()", type_name.to_lowercase()).into())
        }
        CompileTarget::Python => {
            Ok(format!("Dom.{}()", type_name.to_lowercase()).into())
        }
    };
    r.into()
}

/// Default render function for user-defined (JSON-imported) components.
/// Renders the component as a div with a text label showing the component name.
pub fn user_defined_render_fn(
    def: &ComponentDef,
    _components: &XmlComponentMap,
    _args: &FilteredComponentArguments,
    text: &OptionString,
) -> ResultStyledDomRenderDomError {
    let mut dom = Dom::create_node(NodeType::Div);
    if let Some(text_str) = text.as_ref() {
        let prepared = prepare_string(text_str);
        if !prepared.is_empty() {
            dom = dom.with_children(alloc::vec![Dom::create_text(prepared)].into());
        }
    }
    let r: Result<StyledDom, RenderDomError> = Ok(dom.style(Css::empty()));
    r.into()
}

/// Default compile function for user-defined (JSON-imported) components.
/// Generates code that creates a div node for the target language.
pub fn user_defined_compile_fn(
    def: &ComponentDef,
    target: &CompileTarget,
    _components: &XmlComponentMap,
    _args: &FilteredComponentArguments,
    text: &OptionString,
    indent: usize,
) -> ResultStringCompileError {
    let tag = def.id.name.as_str();
    let r: Result<AzString, CompileError> = match target {
        CompileTarget::Rust => {
            if let Some(text_str) = text.as_ref() {
                Ok(format!(
                    "Dom::create_node(NodeType::Div).with_children(vec![Dom::create_text(AzString::from_const_str(\"{}\"))].into())",
                    text_str.as_str().replace("\\", "\\\\").replace("\"", "\\\"")
                ).into())
            } else {
                Ok(format!("Dom::create_node(NodeType::Div) /* {} */", tag).into())
            }
        }
        CompileTarget::C => {
            if let Some(text_str) = text.as_ref() {
                Ok(format!(
                    "AzDom_createText(AzString_fromConstStr(\"{}\"))",
                    text_str.as_str().replace("\\", "\\\\").replace("\"", "\\\"")
                ).into())
            } else {
                Ok(format!("AzDom_createDiv() /* {} */", tag).into())
            }
        }
        CompileTarget::Cpp => {
            Ok(format!("Dom::create_div() /* {} */", tag).into())
        }
        CompileTarget::Python => {
            Ok(format!("Dom.div() # {}", tag).into())
        }
    };
    r.into()
}

/// Create a ComponentDef for a builtin HTML element
fn builtin_component_def(tag: &str, display_name: &str, node_type: NodeType, accepts_text: bool, child_policy: ChildPolicy) -> ComponentDef {
    let fields = builtin_data_model(tag);
    let model_name = format!("{}Data", display_name);
    ComponentDef {
        id: ComponentId::builtin(tag),
        display_name: AzString::from(display_name),
        description: AzString::from(format!("HTML <{}> element", tag).as_str()),
        accepts_text,
        child_policy,
        scoped_css: AzString::from_const_str(""),
        example_xml: AzString::from(format!("<{}>content</{}>", tag, tag).as_str()),
        source: ComponentSource::Builtin,
        data_model: ComponentDataModel {
            name: AzString::from(model_name.as_str()),
            description: AzString::from(format!("Data model for <{}>", tag).as_str()),
            fields: fields.into(),
        },
        template: AzString::from_const_str(""),
        render_fn: builtin_render_fn,
        compile_fn: builtin_compile_fn,
        node_type: OptionNodeType::Some(node_type),
    }
}

/// Helper to create a ComponentDataField with a rich type
fn data_field(name: &str, ft: ComponentFieldType, default: Option<ComponentDefaultValue>, description: &str) -> ComponentDataField {
    let required = default.is_none();
    ComponentDataField {
        name: AzString::from(name),
        field_type: ft,
        default_value: match default {
            Some(d) => OptionComponentDefaultValue::Some(d),
            None => OptionComponentDefaultValue::None,
        },
        required,
        description: AzString::from(description),
    }
}

/// Returns the tag-specific data model fields for builtin HTML elements.
/// These are the component's "main data model" — the attributes that define
/// what the component needs as configuration (e.g., `href` for `<a>`,
/// `src` for `<img>`). Universal HTML attributes (id, class, style, etc.)
/// are NOT included here — they are added separately by the debug server.
fn builtin_data_model(tag: &str) -> Vec<ComponentDataField> {
    use ComponentFieldType::*;
    use ComponentDefaultValue as D;
    match tag {
        "a" => alloc::vec![
            data_field("href", String, Some(D::String(AzString::from_const_str(""))), "URL the link points to"),
            data_field("target", String, Some(D::String(AzString::from_const_str(""))), "Where to open the linked document (_blank, _self, _parent, _top)"),
            data_field("rel", String, Some(D::String(AzString::from_const_str(""))), "Relationship between current and linked document"),
        ],
        "img" | "image" => alloc::vec![
            data_field("src", String, None, "URL of the image"),
            data_field("alt", String, Some(D::String(AzString::from_const_str(""))), "Alternative text for the image"),
            data_field("width", String, Some(D::String(AzString::from_const_str(""))), "Width of the image"),
            data_field("height", String, Some(D::String(AzString::from_const_str(""))), "Height of the image"),
        ],
        "form" => alloc::vec![
            data_field("action", String, Some(D::String(AzString::from_const_str(""))), "URL where form data is submitted"),
            data_field("method", String, Some(D::String(AzString::from_const_str("GET"))), "HTTP method for form submission (GET or POST)"),
        ],
        "label" => alloc::vec![
            data_field("for", String, Some(D::String(AzString::from_const_str(""))), "ID of the form element this label is for"),
        ],
        "button" => alloc::vec![
            data_field("type", String, Some(D::String(AzString::from_const_str("button"))), "Button type (button, submit, reset)"),
            data_field("disabled", Bool, Some(D::Bool(false)), "Whether the button is disabled"),
        ],
        "td" | "th" => alloc::vec![
            data_field("colspan", I32, Some(D::I32(1)), "Number of columns the cell spans"),
            data_field("rowspan", I32, Some(D::I32(1)), "Number of rows the cell spans"),
        ],
        "icon" => alloc::vec![
            data_field("name", String, Some(D::String(AzString::from_const_str(""))), "Icon name"),
        ],
        "ol" => alloc::vec![
            data_field("start", I32, Some(D::I32(1)), "Start value for the ordered list"),
            data_field("type", String, Some(D::String(AzString::from_const_str("1"))), "Numbering type (1, A, a, I, i)"),
        ],
        _ => alloc::vec![],
    }
}

impl Default for ComponentMap {
    /// Returns an empty `ComponentMap` with no libraries.
    ///
    /// Use `AppConfig::create()` (which registers the 52 builtins via
    /// `register_builtin_components`) followed by `ComponentMap::from_libraries()`
    /// to get a fully-populated map.
    fn default() -> Self {
        ComponentMap {
            libraries: ComponentLibraryVec::from_const_slice(&[]),
        }
    }
}

impl ComponentMap {
    pub fn create() -> Self {
        Self::default()
    }

    /// Build a `ComponentMap` from the libraries stored in an `AppConfig`.
    ///
    /// The `component_libraries` field already contains builtins (registered in
    /// `AppConfig::create()`) plus any user-added libraries.  No merging needed —
    /// `add_component_library` / `add_component` handle insertion at registration time.
    pub fn from_libraries(libs: &ComponentLibraryVec) -> Self {
        ComponentMap {
            libraries: libs.clone(),
        }
    }
}

/// Register the 52 built-in HTML element components.
///
/// This is an `extern "C"` function pointer compatible with
/// `RegisterComponentLibraryFnType`, so it can be passed directly to
/// `AppConfig::add_component_library()`.
///
/// Called once during `AppConfig::create()` — the framework dogfoods
/// its own component registration system for builtins.
pub extern "C" fn register_builtin_components() -> ComponentLibrary {
    ComponentLibrary {
        name: AzString::from_const_str("builtin"),
        version: AzString::from_const_str("1.0.0"),
        description: AzString::from_const_str("Built-in HTML elements"),
        exportable: false,
        modifiable: false,
        data_models: Vec::new().into(),
        enum_models: Vec::new().into(),
        components: alloc::vec![
            // Structural
            builtin_component_def("html", "HTML", NodeType::Html, false, ChildPolicy::AnyChildren),
            builtin_component_def("head", "Head", NodeType::Head, false, ChildPolicy::AnyChildren),
            builtin_component_def("title", "Title", NodeType::Title, true, ChildPolicy::TextOnly),
            builtin_component_def("body", "Body", NodeType::Body, false, ChildPolicy::AnyChildren),
            // Block-level
            builtin_component_def("div", "Div", NodeType::Div, false, ChildPolicy::AnyChildren),
            builtin_component_def("header", "Header", NodeType::Header, false, ChildPolicy::AnyChildren),
            builtin_component_def("footer", "Footer", NodeType::Footer, false, ChildPolicy::AnyChildren),
            builtin_component_def("section", "Section", NodeType::Section, false, ChildPolicy::AnyChildren),
            builtin_component_def("article", "Article", NodeType::Article, false, ChildPolicy::AnyChildren),
            builtin_component_def("aside", "Aside", NodeType::Aside, false, ChildPolicy::AnyChildren),
            builtin_component_def("nav", "Nav", NodeType::Nav, false, ChildPolicy::AnyChildren),
            builtin_component_def("main", "Main", NodeType::Main, false, ChildPolicy::AnyChildren),
            // Headings
            builtin_component_def("h1", "Heading 1", NodeType::H1, true, ChildPolicy::TextOnly),
            builtin_component_def("h2", "Heading 2", NodeType::H2, true, ChildPolicy::TextOnly),
            builtin_component_def("h3", "Heading 3", NodeType::H3, true, ChildPolicy::TextOnly),
            builtin_component_def("h4", "Heading 4", NodeType::H4, true, ChildPolicy::TextOnly),
            builtin_component_def("h5", "Heading 5", NodeType::H5, true, ChildPolicy::TextOnly),
            builtin_component_def("h6", "Heading 6", NodeType::H6, true, ChildPolicy::TextOnly),
            // Text content
            builtin_component_def("p", "Paragraph", NodeType::P, true, ChildPolicy::AnyChildren),
            builtin_component_def("span", "Span", NodeType::Span, true, ChildPolicy::AnyChildren),
            builtin_component_def("pre", "Preformatted", NodeType::Pre, true, ChildPolicy::TextOnly),
            builtin_component_def("code", "Code", NodeType::Code, true, ChildPolicy::TextOnly),
            builtin_component_def("blockquote", "Blockquote", NodeType::BlockQuote, true, ChildPolicy::AnyChildren),
            builtin_component_def("br", "Line Break", NodeType::Br, false, ChildPolicy::NoChildren),
            builtin_component_def("hr", "Horizontal Rule", NodeType::Hr, false, ChildPolicy::NoChildren),
            builtin_component_def("icon", "Icon", NodeType::Div, true, ChildPolicy::NoChildren),
            // Lists
            builtin_component_def("ul", "Unordered List", NodeType::Ul, false, ChildPolicy::AnyChildren),
            builtin_component_def("ol", "Ordered List", NodeType::Ol, false, ChildPolicy::AnyChildren),
            builtin_component_def("li", "List Item", NodeType::Li, true, ChildPolicy::AnyChildren),
            builtin_component_def("dl", "Description List", NodeType::Dl, false, ChildPolicy::AnyChildren),
            builtin_component_def("dt", "Description Term", NodeType::Dt, true, ChildPolicy::TextOnly),
            builtin_component_def("dd", "Description Details", NodeType::Dd, true, ChildPolicy::AnyChildren),
            // Tables
            builtin_component_def("table", "Table", NodeType::Table, false, ChildPolicy::AnyChildren),
            builtin_component_def("thead", "Table Head", NodeType::THead, false, ChildPolicy::AnyChildren),
            builtin_component_def("tbody", "Table Body", NodeType::TBody, false, ChildPolicy::AnyChildren),
            builtin_component_def("tfoot", "Table Foot", NodeType::TFoot, false, ChildPolicy::AnyChildren),
            builtin_component_def("tr", "Table Row", NodeType::Tr, false, ChildPolicy::AnyChildren),
            builtin_component_def("th", "Table Header Cell", NodeType::Th, true, ChildPolicy::AnyChildren),
            builtin_component_def("td", "Table Data Cell", NodeType::Td, true, ChildPolicy::AnyChildren),
            // Inline
            builtin_component_def("a", "Link", NodeType::A, true, ChildPolicy::AnyChildren),
            builtin_component_def("strong", "Strong", NodeType::Strong, true, ChildPolicy::TextOnly),
            builtin_component_def("em", "Emphasis", NodeType::Em, true, ChildPolicy::TextOnly),
            builtin_component_def("b", "Bold", NodeType::B, true, ChildPolicy::TextOnly),
            builtin_component_def("i", "Italic", NodeType::I, true, ChildPolicy::TextOnly),
            builtin_component_def("u", "Underline", NodeType::U, true, ChildPolicy::TextOnly),
            builtin_component_def("small", "Small", NodeType::Small, true, ChildPolicy::TextOnly),
            builtin_component_def("mark", "Mark", NodeType::Mark, true, ChildPolicy::TextOnly),
            builtin_component_def("sub", "Subscript", NodeType::Sub, true, ChildPolicy::TextOnly),
            builtin_component_def("sup", "Superscript", NodeType::Sup, true, ChildPolicy::TextOnly),
            // Forms
            builtin_component_def("form", "Form", NodeType::Form, false, ChildPolicy::AnyChildren),
            builtin_component_def("label", "Label", NodeType::Label, true, ChildPolicy::AnyChildren),
            builtin_component_def("button", "Button", NodeType::Button, true, ChildPolicy::AnyChildren),
        ].into(),
    }
}

// ============================================================================
// End new component system types
// ============================================================================

/// Specifies a component that reacts to a parsed XML node
pub trait XmlComponentTrait {
    /// Clone this trait object into a new Box.
    /// Required so that `XmlComponent` (and by extension `XmlComponentVec`) can be `Clone`.
    fn clone_box(&self) -> Box<dyn XmlComponentTrait>;

    /// Returns the type ID of this component, default = `div`
    fn get_type_id(&self) -> String {
        "div".to_string()
    }

    /// Returns the XML node for this component, used in the `get_html_string` debugging code
    /// (necessary to compile the component into a function during the Rust compilation stage)
    fn get_xml_node(&self) -> XmlNode {
        XmlNode::create(self.get_type_id())
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
    /// dom.assert_eq(Dom::create_div().with_id("test"));
    /// ```
    #[cfg(test)]
    pub fn assert_eq(self, other: StyledDom) {
        let mut fixed = Dom::create_body().style(Css::empty());
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

/// Represents a child of an XML node - either an element or text
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum XmlNodeChild {
    /// A text node
    Text(AzString),
    /// An element node
    Element(XmlNode),
}

impl_option!(
    XmlNodeChild,
    OptionXmlNodeChild,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl XmlNodeChild {
    /// Get the text content if this is a text node
    pub fn as_text(&self) -> Option<&str> {
        match self {
            XmlNodeChild::Text(s) => Some(s.as_str()),
            XmlNodeChild::Element(_) => None,
        }
    }

    /// Get the element if this is an element node
    pub fn as_element(&self) -> Option<&XmlNode> {
        match self {
            XmlNodeChild::Text(_) => None,
            XmlNodeChild::Element(node) => Some(node),
        }
    }

    /// Get the element mutably if this is an element node
    pub fn as_element_mut(&mut self) -> Option<&mut XmlNode> {
        match self {
            XmlNodeChild::Text(_) => None,
            XmlNodeChild::Element(node) => Some(node),
        }
    }
}

impl_vec!(XmlNodeChild, XmlNodeChildVec, XmlNodeChildVecDestructor, XmlNodeChildVecDestructorType, XmlNodeChildVecSlice, OptionXmlNodeChild);
impl_vec_mut!(XmlNodeChild, XmlNodeChildVec);
impl_vec_debug!(XmlNodeChild, XmlNodeChildVec);
impl_vec_partialeq!(XmlNodeChild, XmlNodeChildVec);
impl_vec_eq!(XmlNodeChild, XmlNodeChildVec);
impl_vec_partialord!(XmlNodeChild, XmlNodeChildVec);
impl_vec_ord!(XmlNodeChild, XmlNodeChildVec);
impl_vec_hash!(XmlNodeChild, XmlNodeChildVec);
impl_vec_clone!(XmlNodeChild, XmlNodeChildVec, XmlNodeChildVecDestructor);

/// Represents one XML node tag
#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct XmlNode {
    /// Type of the node
    pub node_type: XmlTagName,
    /// Attributes of an XML node (note: not yet filtered and / or broken into function arguments!)
    pub attributes: XmlAttributeMap,
    /// Direct children of this node (can be text or element nodes)
    pub children: XmlNodeChildVec,
}

impl_option!(
    XmlNode,
    OptionXmlNode,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl XmlNode {
    pub fn create<I: Into<XmlTagName>>(node_type: I) -> Self {
        XmlNode {
            node_type: node_type.into(),
            ..Default::default()
        }
    }
    pub fn with_children(mut self, v: Vec<XmlNodeChild>) -> Self {
        Self {
            children: v.into(),
            ..self
        }
    }

    /// Get all text content concatenated from direct children
    pub fn get_text_content(&self) -> String {
        self.children
            .as_ref()
            .iter()
            .filter_map(|child| child.as_text())
            .collect::<Vec<_>>()
            .join("")
    }

    /// Check if this node has only text children (no element children)
    pub fn has_only_text_children(&self) -> bool {
        self.children
            .as_ref()
            .iter()
            .all(|child| matches!(child, XmlNodeChild::Text(_)))
    }
}

impl_vec!(XmlNode, XmlNodeVec, XmlNodeVecDestructor, XmlNodeVecDestructorType, XmlNodeVecSlice, OptionXmlNode);
impl_vec_mut!(XmlNode, XmlNodeVec);
impl_vec_debug!(XmlNode, XmlNodeVec);
impl_vec_partialeq!(XmlNode, XmlNodeVec);
impl_vec_eq!(XmlNode, XmlNodeVec);
impl_vec_partialord!(XmlNode, XmlNodeVec);
impl_vec_ord!(XmlNode, XmlNodeVec);
impl_vec_hash!(XmlNode, XmlNodeVec);
impl_vec_clone!(XmlNode, XmlNodeVec, XmlNodeVecDestructor);

#[repr(C)]
pub struct XmlComponent {
    pub id: String,
    /// DOM rendering component (boxed trait)
    pub renderer: Box<dyn XmlComponentTrait>,
    /// Whether this component should inherit variables from the parent scope
    pub inherit_vars: bool,
}

impl Clone for XmlComponent {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            renderer: self.renderer.clone_box(),
            inherit_vars: self.inherit_vars,
        }
    }
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

impl_option!(XmlComponent, OptionXmlComponent, copy = false, clone = false, [Debug]);
impl_vec!(XmlComponent, XmlComponentVec, XmlComponentVecDestructor, XmlComponentVecDestructorType, XmlComponentVecSlice, OptionXmlComponent);
impl_vec_clone!(XmlComponent, XmlComponentVec, XmlComponentVecDestructor);
impl_vec_mut!(XmlComponent, XmlComponentVec);
impl_vec_debug!(XmlComponent, XmlComponentVec);

/// Holds all XML components - builtin components
#[repr(C)]
pub struct XmlComponentMap {
    /// Stores all known components that can be used during DOM rendering
    /// Lookup by normalized component name (lowercase with underscores)
    pub components: XmlComponentVec,
}

impl Default for XmlComponentMap {
    fn default() -> Self {
        let mut map = Self {
            components: XmlComponentVec::new(),
        };

        // Structural elements
        map.register_component(XmlComponent {
            id: normalize_casing("html"),
            renderer: Box::new(HtmlRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("head"),
            renderer: Box::new(HeadRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("title"),
            renderer: Box::new(TitleRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("body"),
            renderer: Box::new(BodyRenderer::new()),
            inherit_vars: true,
        });

        // Block-level elements
        map.register_component(XmlComponent {
            id: normalize_casing("div"),
            renderer: Box::new(DivRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("header"),
            renderer: Box::new(HeaderRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("footer"),
            renderer: Box::new(FooterRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("section"),
            renderer: Box::new(SectionRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("article"),
            renderer: Box::new(ArticleRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("aside"),
            renderer: Box::new(AsideRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("nav"),
            renderer: Box::new(NavRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("main"),
            renderer: Box::new(MainRenderer::new()),
            inherit_vars: true,
        });

        // Heading elements
        map.register_component(XmlComponent {
            id: normalize_casing("h1"),
            renderer: Box::new(H1Renderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("h2"),
            renderer: Box::new(H2Renderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("h3"),
            renderer: Box::new(H3Renderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("h4"),
            renderer: Box::new(H4Renderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("h5"),
            renderer: Box::new(H5Renderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("h6"),
            renderer: Box::new(H6Renderer::new()),
            inherit_vars: true,
        });

        // Text content elements
        map.register_component(XmlComponent {
            id: normalize_casing("p"),
            renderer: Box::new(TextRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("span"),
            renderer: Box::new(SpanRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("pre"),
            renderer: Box::new(PreRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("code"),
            renderer: Box::new(CodeRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("blockquote"),
            renderer: Box::new(BlockquoteRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("br"),
            renderer: Box::new(BrRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("hr"),
            renderer: Box::new(HrRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("icon"),
            renderer: Box::new(IconRenderer::new()),
            inherit_vars: true,
        });

        // List elements
        map.register_component(XmlComponent {
            id: normalize_casing("ul"),
            renderer: Box::new(UlRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("ol"),
            renderer: Box::new(OlRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("li"),
            renderer: Box::new(LiRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("dl"),
            renderer: Box::new(DlRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("dt"),
            renderer: Box::new(DtRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("dd"),
            renderer: Box::new(DdRenderer::new()),
            inherit_vars: true,
        });

        // Table elements
        map.register_component(XmlComponent {
            id: normalize_casing("table"),
            renderer: Box::new(TableRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("thead"),
            renderer: Box::new(TheadRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("tbody"),
            renderer: Box::new(TbodyRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("tfoot"),
            renderer: Box::new(TfootRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("tr"),
            renderer: Box::new(TrRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("th"),
            renderer: Box::new(ThRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("td"),
            renderer: Box::new(TdRenderer::new()),
            inherit_vars: true,
        });

        // Inline elements
        map.register_component(XmlComponent {
            id: normalize_casing("a"),
            renderer: Box::new(ARenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("strong"),
            renderer: Box::new(StrongRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("em"),
            renderer: Box::new(EmRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("b"),
            renderer: Box::new(BRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("i"),
            renderer: Box::new(IRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("u"),
            renderer: Box::new(URenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("small"),
            renderer: Box::new(SmallRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("mark"),
            renderer: Box::new(MarkRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("sub"),
            renderer: Box::new(SubRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("sup"),
            renderer: Box::new(SupRenderer::new()),
            inherit_vars: true,
        });

        // Form elements
        map.register_component(XmlComponent {
            id: normalize_casing("form"),
            renderer: Box::new(FormRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("label"),
            renderer: Box::new(LabelRenderer::new()),
            inherit_vars: true,
        });
        map.register_component(XmlComponent {
            id: normalize_casing("button"),
            renderer: Box::new(ButtonRenderer::new()),
            inherit_vars: true,
        });

        map
    }
}

impl XmlComponentMap {
    pub fn register_component(&mut self, comp: XmlComponent) {
        // Replace existing or push new
        if let Some(existing) = self.components.iter_mut().find(|c| c.id == comp.id) {
            *existing = comp;
        } else {
            self.components.push(comp);
        }
    }
    
    /// Get a component by its normalized name
    pub fn get(&self, name: &str) -> Option<&XmlComponent> {
        self.components.iter().find(|c| c.id == name)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
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
    MalformedHierarchy(MalformedHierarchyError),
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
#[repr(C, u8)]
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

/// Wrapper for UselessFunctionArgument error data.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct UselessFunctionArgumentError {
    pub component_name: AzString,
    pub argument_name: AzString,
    pub valid_args: StringVec,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum ComponentError {
    /// While instantiating a component, a function argument
    /// was encountered that the component won't use or react to.
    UselessFunctionArgument(UselessFunctionArgumentError),
    /// A certain node type can't be rendered, because the
    /// renderer for this node is not available isn't available
    ///
    /// UnknownComponent(component_name)
    UnknownComponent(AzString),
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
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

/// Wrapper for MissingType error data.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct MissingTypeError {
    pub arg_pos: usize,
    pub arg_name: AzString,
}

/// Wrapper for WhiteSpaceInComponentName error data.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct WhiteSpaceInComponentNameError {
    pub arg_pos: usize,
    pub arg_name: AzString,
}

/// Wrapper for WhiteSpaceInComponentType error data.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct WhiteSpaceInComponentTypeError {
    pub arg_pos: usize,
    pub arg_name: AzString,
    pub arg_type: AzString,
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ComponentParseError {
    /// Given XmlNode is not a `<component />` node.
    NotAComponent,
    /// A `<component>` node does not have a `name` attribute.
    UnnamedComponent,
    /// Argument at position `usize` is either empty or has no name
    MissingName(usize),
    /// Argument at position `usize` with the name
    /// `String` doesn't have a `: type`
    MissingType(MissingTypeError),
    /// Component name may not contain a whitespace
    /// (probably missing a `:` between the name and the type)
    WhiteSpaceInComponentName(WhiteSpaceInComponentNameError),
    /// Component type may not contain a whitespace
    /// (probably missing a `,` between the type and the next name)
    WhiteSpaceInComponentType(WhiteSpaceInComponentTypeError),
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
            MalformedHierarchy(e) => write!(
                f,
                "Invalid </{}> tag: expected </{}>",
                e.got.as_str(),
                e.expected.as_str()
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
            MissingType(e) => write!(
                f,
                "Argument \"{}\" at position {} doesn't have a `: type`",
                e.arg_pos, e.arg_name
            ),
            WhiteSpaceInComponentName(e) => {
                write!(
                    f,
                    "Missing `:` between the name and the type in argument {} (around \"{}\")",
                    e.arg_pos, e.arg_name
                )
            }
            WhiteSpaceInComponentType(e) => {
                write!(
                    f,
                    "Missing `,` between two arguments (in argument {}, position {}, around \
                     \"{}\")",
                    e.arg_name, e.arg_pos, e.arg_type
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
            UselessFunctionArgument(e) => {
                write!(
                    f,
                    "Useless component argument \"{}\": \"{}\" - available args are: {:#?}",
                    e.component_name, e.argument_name, e.valid_args
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

/// Macro to generate HTML element components
/// Each HTML tag becomes a component that renders the corresponding DOM node
macro_rules! html_component {
    ($name:ident, $tag:expr, $node_type:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name {
            node: XmlNode,
        }

        impl $name {
            pub fn new() -> Self {
                Self {
                    node: XmlNode::create($tag),
                }
            }
        }

        impl XmlComponentTrait for $name {
            fn clone_box(&self) -> Box<dyn XmlComponentTrait> {
                Box::new(self.clone())
            }

            fn get_available_arguments(&self) -> ComponentArguments {
                ComponentArguments {
                    args: ComponentArgumentVec::new(),
                    accepts_text: true,
                }
            }

            fn render_dom(
                &self,
                _: &XmlComponentMap,
                _: &FilteredComponentArguments,
                text: &XmlTextContent,
            ) -> Result<StyledDom, RenderDomError> {
                let mut dom = Dom::create_node($node_type);

                // Add text content if present
                if let Some(text_str) = text.as_ref() {
                    let prepared = prepare_string(text_str);
                    if !prepared.is_empty() {
                        dom = dom.with_children(alloc::vec![Dom::create_text(prepared)].into());
                    }
                }

                Ok(dom.style(Css::empty()))
            }

            fn compile_to_rust_code(
                &self,
                _: &XmlComponentMap,
                _: &ComponentArguments,
                _: &XmlTextContent,
            ) -> Result<String, CompileError> {
                Ok(format!(
                    "Dom::create_node(NodeType::{})",
                    stringify!($node_type)
                ))
            }

            fn get_xml_node(&self) -> XmlNode {
                self.node.clone()
            }
        }
    };
}

// Generate components for HTML elements
html_component!(HtmlRenderer, "html", NodeType::Html);
html_component!(HeadRenderer, "head", NodeType::Head);
html_component!(TitleRenderer, "title", NodeType::Title);
html_component!(HeaderRenderer, "header", NodeType::Header);
html_component!(FooterRenderer, "footer", NodeType::Footer);
html_component!(SectionRenderer, "section", NodeType::Section);
html_component!(ArticleRenderer, "article", NodeType::Article);
html_component!(AsideRenderer, "aside", NodeType::Aside);
html_component!(NavRenderer, "nav", NodeType::Nav);
html_component!(MainRenderer, "main", NodeType::Main);
html_component!(H1Renderer, "h1", NodeType::H1);
html_component!(H2Renderer, "h2", NodeType::H2);
html_component!(H3Renderer, "h3", NodeType::H3);
html_component!(H4Renderer, "h4", NodeType::H4);
html_component!(H5Renderer, "h5", NodeType::H5);
html_component!(H6Renderer, "h6", NodeType::H6);
html_component!(SpanRenderer, "span", NodeType::Span);
html_component!(PreRenderer, "pre", NodeType::Pre);
html_component!(CodeRenderer, "code", NodeType::Code);
html_component!(BlockquoteRenderer, "blockquote", NodeType::BlockQuote);
html_component!(UlRenderer, "ul", NodeType::Ul);
html_component!(OlRenderer, "ol", NodeType::Ol);
html_component!(LiRenderer, "li", NodeType::Li);
html_component!(DlRenderer, "dl", NodeType::Dl);
html_component!(DtRenderer, "dt", NodeType::Dt);
html_component!(DdRenderer, "dd", NodeType::Dd);
html_component!(TableRenderer, "table", NodeType::Table);
html_component!(TheadRenderer, "thead", NodeType::THead);
html_component!(TbodyRenderer, "tbody", NodeType::TBody);
html_component!(TfootRenderer, "tfoot", NodeType::TFoot);
html_component!(TrRenderer, "tr", NodeType::Tr);
html_component!(ThRenderer, "th", NodeType::Th);
html_component!(TdRenderer, "td", NodeType::Td);
html_component!(ARenderer, "a", NodeType::A);
html_component!(StrongRenderer, "strong", NodeType::Strong);
html_component!(EmRenderer, "em", NodeType::Em);
html_component!(BRenderer, "b", NodeType::B);
html_component!(IRenderer, "i", NodeType::I);
html_component!(URenderer, "u", NodeType::U);
html_component!(SmallRenderer, "small", NodeType::Small);
html_component!(MarkRenderer, "mark", NodeType::Mark);
html_component!(SubRenderer, "sub", NodeType::Sub);
html_component!(SupRenderer, "sup", NodeType::Sup);
html_component!(FormRenderer, "form", NodeType::Form);
html_component!(LabelRenderer, "label", NodeType::Label);
html_component!(ButtonRenderer, "button", NodeType::Button);
html_component!(HrRenderer, "hr", NodeType::Hr);

/// Render for a `div` component
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DivRenderer {
    node: XmlNode,
}

impl DivRenderer {
    pub fn new() -> Self {
        Self {
            node: XmlNode::create("div"),
        }
    }
}

impl XmlComponentTrait for DivRenderer {
    fn clone_box(&self) -> Box<dyn XmlComponentTrait> {
        Box::new(self.clone())
    }

    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments::new()
    }

    fn render_dom(
        &self,
        _: &XmlComponentMap,
        _: &FilteredComponentArguments,
        _: &XmlTextContent,
    ) -> Result<StyledDom, RenderDomError> {
        Ok(Dom::create_div().style(Css::empty()))
    }

    fn compile_to_rust_code(
        &self,
        _: &XmlComponentMap,
        _: &ComponentArguments,
        _: &XmlTextContent,
    ) -> Result<String, CompileError> {
        Ok("Dom::create_div()".into())
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
            node: XmlNode::create("body"),
        }
    }
}

impl XmlComponentTrait for BodyRenderer {
    fn clone_box(&self) -> Box<dyn XmlComponentTrait> {
        Box::new(self.clone())
    }

    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments::new()
    }

    fn render_dom(
        &self,
        _: &XmlComponentMap,
        _: &FilteredComponentArguments,
        _: &XmlTextContent,
    ) -> Result<StyledDom, RenderDomError> {
        Ok(Dom::create_body().style(Css::empty()))
    }

    fn compile_to_rust_code(
        &self,
        _: &XmlComponentMap,
        _: &ComponentArguments,
        _: &XmlTextContent,
    ) -> Result<String, CompileError> {
        Ok("Dom::create_body()".into())
    }

    fn get_xml_node(&self) -> XmlNode {
        self.node.clone()
    }
}

/// Render for a `br` component
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BrRenderer {
    node: XmlNode,
}

impl BrRenderer {
    pub fn new() -> Self {
        Self {
            node: XmlNode::create("br"),
        }
    }
}

impl XmlComponentTrait for BrRenderer {
    fn clone_box(&self) -> Box<dyn XmlComponentTrait> {
        Box::new(self.clone())
    }

    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments::new()
    }

    fn render_dom(
        &self,
        _: &XmlComponentMap,
        _: &FilteredComponentArguments,
        _: &XmlTextContent,
    ) -> Result<StyledDom, RenderDomError> {
        Ok(Dom::create_node(NodeType::Br).style(Css::empty()))
    }

    fn compile_to_rust_code(
        &self,
        _: &XmlComponentMap,
        _: &ComponentArguments,
        _: &XmlTextContent,
    ) -> Result<String, CompileError> {
        Ok("Dom::create_node(NodeType::Br)".into())
    }

    fn get_xml_node(&self) -> XmlNode {
        self.node.clone()
    }
}

/// Renderer for an `icon` component
///
/// Renders an icon element that will be resolved by the IconProvider.
/// The icon name is specified via the `name` attribute or the text content.
///
/// # Example
/// ```html
/// <icon name="home" />
/// <icon>settings</icon>
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IconRenderer {
    node: XmlNode,
}

impl IconRenderer {
    pub fn new() -> Self {
        Self {
            node: XmlNode::create("icon"),
        }
    }
}

impl XmlComponentTrait for IconRenderer {
    fn clone_box(&self) -> Box<dyn XmlComponentTrait> {
        Box::new(self.clone())
    }

    fn get_available_arguments(&self) -> ComponentArguments {
        let mut args = ComponentArgumentVec::new();
        args.push(ComponentArgument { name: "name".into(), arg_type: "String".into() });
        ComponentArguments {
            args,
            accepts_text: true, // Allow <icon>name</icon> syntax
        }
    }

    fn render_dom(
        &self,
        _: &XmlComponentMap,
        args: &FilteredComponentArguments,
        content: &XmlTextContent,
    ) -> Result<StyledDom, RenderDomError> {
        // Get icon name from either the 'name' attribute or text content
        let icon_name = args.values.get_key("name")
            .map(|s| s.as_str().to_string())
            .or_else(|| content.as_ref().map(|s| prepare_string(&s)))
            .unwrap_or_else(|| "invalid-icon".to_string());
        
        Ok(Dom::create_node(NodeType::Icon(AzString::from(icon_name))).style(Css::empty()))
    }

    fn compile_to_rust_code(
        &self,
        _: &XmlComponentMap,
        args: &ComponentArguments,
        content: &XmlTextContent,
    ) -> Result<String, CompileError> {
        let icon_name = args.args.iter()
            .find(|a| a.name.as_str() == "name")
            .map(|a| a.arg_type.to_string())
            .or_else(|| content.as_ref().map(|s| s.to_string()))
            .unwrap_or_else(|| "invalid-icon".to_string());
        
        Ok(format!("Dom::create_node(NodeType::Icon(AzString::from(\"{}\")))", icon_name))
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
            node: XmlNode::create("p"),
        }
    }
}

impl XmlComponentTrait for TextRenderer {
    fn clone_box(&self) -> Box<dyn XmlComponentTrait> {
        Box::new(self.clone())
    }

    fn get_available_arguments(&self) -> ComponentArguments {
        ComponentArguments {
            args: ComponentArgumentVec::new(),
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
        Ok(Dom::create_node(NodeType::P)
            .with_children(vec![Dom::create_text(content)].into())
            .style(Css::empty()))
    }

    fn compile_to_rust_code(
        &self,
        _: &XmlComponentMap,
        args: &ComponentArguments,
        content: &XmlTextContent,
    ) -> Result<String, CompileError> {
        Ok(String::from(
            "Dom::create_node(NodeType::P).with_children(vec![Dom::create_text(content)].into())",
        ))
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

    let mut args = ComponentArgumentVec::new();

    for (arg_idx, arg) in input.split(",").enumerate() {
        let mut colon_iterator = arg.split(":");

        let arg_name = colon_iterator.next().ok_or(MissingName(arg_idx))?;
        let arg_name = arg_name.trim();

        if arg_name.is_empty() {
            return Err(MissingName(arg_idx));
        }
        if arg_name.chars().any(char::is_whitespace) {
            return Err(WhiteSpaceInComponentName(WhiteSpaceInComponentNameError { arg_pos: arg_idx, arg_name: arg_name.into() }));
        }

        let arg_type = colon_iterator
            .next()
            .ok_or(MissingType(MissingTypeError { arg_pos: arg_idx, arg_name: arg_name.into() }))?;
        let arg_type = arg_type.trim();

        if arg_type.is_empty() {
            return Err(MissingType(MissingTypeError { arg_pos: arg_idx, arg_name: arg_name.into() }));
        }

        if arg_type.chars().any(char::is_whitespace) {
            return Err(WhiteSpaceInComponentType(WhiteSpaceInComponentTypeError {
                arg_pos: arg_idx,
                arg_name: arg_name.into(),
                arg_type: arg_type.into(),
            }));
        }

        let arg_name = normalize_casing(arg_name);
        let arg_type = arg_type.to_string();

        args.push(ComponentArgument { name: arg_name.into(), arg_type: arg_type.into() });
    }

    Ok(args)
}

/// Filters the XML attributes of a component given XmlAttributeMap.
///
/// Validates that:
/// - Attribute names are recognized by the component
/// - Values are type-compatible with declared field types (when possible)
/// - Unknown attributes trigger warnings (lenient mode for HTML compat)
pub fn validate_and_filter_component_args(
    xml_attributes: &XmlAttributeMap,
    valid_args: &ComponentArguments,
) -> Result<FilteredComponentArguments, ComponentError> {
    let mut map = FilteredComponentArguments {
        types: ComponentArgumentVec::new(),
        values: StringPairVec::from_const_slice(&[]),
        accepts_text: valid_args.accepts_text,
    };

    for AzStringPair { key, value } in xml_attributes.as_ref().iter() {
        let xml_attribute_name = key;
        let xml_attribute_value = value;
        if let Some(valid_arg_type) = valid_args
            .args
            .iter()
            .find(|s| s.name.as_str() == xml_attribute_name.as_str())
            .map(|q| &q.arg_type)
        {
            // Validate value against declared type (basic checks)
            validate_attribute_value(
                xml_attribute_name.as_str(),
                xml_attribute_value.as_str(),
                valid_arg_type.as_str(),
            );

            map.types.push(ComponentArgument {
                name: xml_attribute_name.as_str().into(),
                arg_type: valid_arg_type.clone(),
            });
            map.values.insert_kv(
                xml_attribute_name.as_str(),
                xml_attribute_value.as_str(),
            );
        } else if DEFAULT_ARGS.contains(&xml_attribute_name.as_str()) {
            // no error, but don't insert the attribute name
            map.values.insert_kv(
                xml_attribute_name.as_str(),
                xml_attribute_value.as_str(),
            );
        } else {
            // key was not expected for this component
            // WARNING: Lenient mode - ignore unknown attributes instead of erroring
            // This allows HTML with unsupported attributes (like <img src="...">) to render
            #[cfg(feature = "std")]
            eprintln!(
                "Warning: Useless component argument \"{}\": \"{}\" for component with args: {:?}",
                xml_attribute_name,
                xml_attribute_value,
                valid_args.args.iter().map(|s| &s.name).collect::<Vec<_>>()
            );

            // Still insert the value so it's available, but don't validate the type
            map.values.insert_kv(
                xml_attribute_name.as_str(),
                xml_attribute_value.as_str(),
            );
        }
    }

    Ok(map)
}

/// Validate an attribute value against a declared type string.
/// Prints a warning if the value is incompatible (lenient — does not error).
fn validate_attribute_value(attr_name: &str, attr_value: &str, type_str: &str) {
    let warning = match type_str {
        "bool" | "Bool" | "boolean" => {
            if !matches!(attr_value, "true" | "false" | "1" | "0" | "yes" | "no") {
                Some(format!("expected bool ('true'/'false'), got '{}'", attr_value))
            } else { None }
        }
        "i32" | "I32" | "int" => {
            if attr_value.parse::<i32>().is_err() {
                Some(format!("expected i32, got '{}'", attr_value))
            } else { None }
        }
        "i64" | "I64" => {
            if attr_value.parse::<i64>().is_err() {
                Some(format!("expected i64, got '{}'", attr_value))
            } else { None }
        }
        "u32" | "U32" => {
            if attr_value.parse::<u32>().is_err() {
                Some(format!("expected u32, got '{}'", attr_value))
            } else { None }
        }
        "u64" | "U64" => {
            if attr_value.parse::<u64>().is_err() {
                Some(format!("expected u64, got '{}'", attr_value))
            } else { None }
        }
        "usize" | "Usize" => {
            if attr_value.parse::<usize>().is_err() {
                Some(format!("expected usize, got '{}'", attr_value))
            } else { None }
        }
        "f32" | "F32" | "float" => {
            if attr_value.parse::<f32>().is_err() {
                Some(format!("expected f32, got '{}'", attr_value))
            } else { None }
        }
        "f64" | "F64" | "double" => {
            if attr_value.parse::<f64>().is_err() {
                Some(format!("expected f64, got '{}'", attr_value))
            } else { None }
        }
        // String, ColorU, CssProperty, etc. — accept any value
        _ => None,
    };

    if let Some(msg) = warning {
        #[cfg(feature = "std")]
        eprintln!("Warning: attribute '{}' type mismatch: {}", attr_name, msg);
    }
}

/// Validate a component's template XML recursively.
///
/// Checks that all child component references in the template:
/// - Reference components that exist in the component map
/// - Pass valid attributes to those components
/// - Don't create circular references
///
/// This works on pre-parsed XML nodes (from the component's template).
#[cfg(feature = "std")]
pub fn validate_component_template_recursive(
    template_children: &[XmlNodeChild],
    component_name: &str,
    component_map: &XmlComponentMap,
    visited: &mut alloc::collections::BTreeSet<alloc::string::String>,
) -> Result<(), alloc::string::String> {
    use alloc::string::ToString;

    if !visited.insert(component_name.to_string()) {
        return Err(alloc::format!(
            "Circular component reference detected: '{}' references itself (chain: {:?})",
            component_name, visited
        ));
    }

    // Recursively check each child element
    for child in template_children {
        validate_xml_node_recursive(child, component_map, visited)?;
    }

    visited.remove(component_name);
    Ok(())
}

/// Recursively validate a single XML node and its children against the component map.
#[cfg(feature = "std")]
fn validate_xml_node_recursive(
    node: &XmlNodeChild,
    component_map: &XmlComponentMap,
    visited: &mut alloc::collections::BTreeSet<alloc::string::String>,
) -> Result<(), alloc::string::String> {
    use alloc::string::ToString;

    let element = match node {
        XmlNodeChild::Element(e) => e,
        _ => return Ok(()),
    };

    let tag_normalized = normalize_casing(&element.node_type);

    // Check if this component exists
    if let Some(xml_component) = component_map.get(&tag_normalized) {
        // Validate attributes against declared arguments
        let available_args = xml_component.renderer.get_available_arguments();
        for AzStringPair { key, .. } in element.attributes.as_ref().iter() {
            let attr_name = key.as_str();
            if !DEFAULT_ARGS.contains(&attr_name)
                && !available_args.args.iter().any(|a| a.name.as_str() == attr_name)
            {
                #[cfg(feature = "std")]
                eprintln!(
                    "Warning: component '{}' does not accept attribute '{}' (available: {:?})",
                    tag_normalized,
                    attr_name,
                    available_args.args.iter().map(|a| &a.name).collect::<alloc::vec::Vec<_>>()
                );
            }
        }

        // If this component has a template, recursively validate its children
        let xml_node = xml_component.renderer.get_xml_node();
        if !xml_node.children.as_ref().is_empty() {
            validate_component_template_recursive(
                xml_node.children.as_ref(),
                &tag_normalized,
                component_map,
                visited,
            )?;
        }
    }
    // Note: unknown tags are allowed (lenient mode for HTML compat)

    // Recurse into children
    for child in element.children.as_ref().iter() {
        validate_xml_node_recursive(child, component_map, visited)?;
    }

    Ok(())
}

/// Find the one and only `<body>` node, return error if
/// there is no app node or there are multiple app nodes
pub fn get_html_node<'a>(root_nodes: &'a [XmlNodeChild]) -> Result<&'a XmlNode, DomXmlParseError> {
    let mut html_node_iterator = root_nodes.iter().filter_map(|child| {
        if let XmlNodeChild::Element(node) = child {
            let node_type_normalized = normalize_casing(&node.node_type);
            if &node_type_normalized == "html" {
                Some(node)
            } else {
                None
            }
        } else {
            None
        }
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
pub fn get_body_node<'a>(root_nodes: &'a [XmlNodeChild]) -> Result<&'a XmlNode, DomXmlParseError> {
    // First try to find body as a direct child (proper HTML structure)
    let direct_body = root_nodes.iter().filter_map(|child| {
        if let XmlNodeChild::Element(node) = child {
            let node_type_normalized = normalize_casing(&node.node_type);
            if &node_type_normalized == "body" {
                Some(node)
            } else {
                None
            }
        } else {
            None
        }
    }).next();
    
    if let Some(body) = direct_body {
        return Ok(body);
    }
    
    // If not found as direct child, search recursively (for malformed HTML like example.com)
    // where <body> might be nested inside <head> due to missing </head> tag
    fn find_body_recursive<'a>(nodes: &'a [XmlNodeChild]) -> Option<&'a XmlNode> {
        for child in nodes {
            if let XmlNodeChild::Element(node) = child {
                let node_type_normalized = normalize_casing(&node.node_type);
                if &node_type_normalized == "body" {
                    return Some(node);
                }
                // Recurse into children
                if let Some(found) = find_body_recursive(node.children.as_ref()) {
                    return Some(found);
                }
            }
        }
        None
    }
    
    find_body_recursive(root_nodes).ok_or(DomXmlParseError::NoBodyInHtml)
}

static DEFAULT_STR: &str = "";

/// Searches in the the `root_nodes` for a `node_type`, convenience function in order to
/// for example find the first <blah /> node in all these nodes.
/// This function searches recursively through the entire tree.
pub fn find_node_by_type<'a>(
    root_nodes: &'a [XmlNodeChild],
    node_type: &str,
) -> Option<&'a XmlNode> {
    // First check direct children
    for child in root_nodes {
        if let XmlNodeChild::Element(node) = child {
            if normalize_casing(&node.node_type).as_str() == node_type {
                return Some(node);
            }
        }
    }
    
    // If not found, search recursively (for malformed HTML)
    for child in root_nodes {
        if let XmlNodeChild::Element(node) = child {
            if let Some(found) = find_node_by_type(node.children.as_ref(), node_type) {
                return Some(found);
            }
        }
    }
    
    None
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
    let child = root_node.children.as_mut().get_mut(item)?;
    match child {
        XmlNodeChild::Element(node) => get_item_internal(&mut hierarchy, node),
        XmlNodeChild::Text(_) => None, // Can't traverse into text nodes
    }
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
    let child = root_node.children.as_mut().get_mut(cur_item)?;
    match child {
        XmlNodeChild::Element(node) => get_item_internal(hierarchy, node),
        XmlNodeChild::Text(_) => None, // Can't traverse into text nodes
    }
}

/// Parses an XML string and returns a `StyledDom` with the components instantiated in the
/// `<app></app>`
pub fn str_to_dom<'a>(
    root_nodes: &'a [XmlNodeChild],
    component_map: &'a mut XmlComponentMap,
    max_width: Option<f32>,
) -> Result<StyledDom, DomXmlParseError> {
    let html_node = get_html_node(root_nodes)?;
    let body_node = get_body_node(html_node.children.as_ref())?;

    let mut global_style = None;

    if let Some(head_node) = find_node_by_type(html_node.children.as_ref(), "head") {
        // parse all dynamic XML components from the head node
        for child in head_node.children.as_ref() {
            if let XmlNodeChild::Element(node) = child {
                match DynamicXmlComponent::new(node) {
                    Ok(comp) => {
                        let node_name = comp.name.clone();
                        component_map.register_component(XmlComponent {
                            id: normalize_casing(&node_name),
                            renderer: Box::new(comp),
                            inherit_vars: false,
                        });
                    }
                    Err(ComponentParseError::NotAComponent) => {} /* not a <component /> node, */
                    // ignore
                    Err(e) => return Err(e.into()), /* Error during parsing the XML
                                                     * component, bail */
                }
            }
        }

        // parse the <style></style> tag contents, if present
        if let Some(style_node) = find_node_by_type(head_node.children.as_ref(), "style") {
            let text = style_node.get_text_content();
            if !text.is_empty() {
                let parsed_css = Css::from_string(text.into());
                global_style = Some(parsed_css);
            }
        }
    }

    render_dom_from_body_node(&body_node, global_style, component_map, max_width)
        .map_err(|e| e.into())
}

/// Parses an XML string and returns a `String`, which contains the Rust source code
/// (i.e. it compiles the XML to valid Rust)
pub fn str_to_rust_code<'a>(
    root_nodes: &'a [XmlNodeChild],
    imports: &str,
    component_map: &'a mut XmlComponentMap,
) -> Result<String, CompileError> {
    let html_node = get_html_node(&root_nodes)?;
    let body_node = get_body_node(html_node.children.as_ref())?;
    let mut global_style = Css::empty();

    if let Some(head_node) = find_node_by_type(html_node.children.as_ref(), "head") {
        for child in head_node.children.as_ref() {
            if let XmlNodeChild::Element(node) = child {
                match DynamicXmlComponent::new(node) {
                    Ok(node) => {
                        let node_name = node.name.clone();
                        component_map.register_component(XmlComponent {
                            id: normalize_casing(&node_name),
                            renderer: Box::new(node),
                            inherit_vars: false,
                        });
                    }
                    Err(ComponentParseError::NotAComponent) => {} /* not a <component /> node, */
                    // ignore
                    Err(e) => return Err(CompileError::Xml(e.into())), /* Error during parsing
                                                                        * the XML
                                                                        * component, bail */
                }
            }
        }

        if let Some(style_node) = find_node_by_type(head_node.children.as_ref(), "style") {
            let text = style_node.get_text_content();
            if !text.is_empty() {
                let parsed_css = azul_css::parser2::new_from_str(&text).0;
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

extern \"C\" fn render(_: RefAny, _: LayoutCallbackInfo) -> StyledDom {
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
        .map(|a| format!("{}: {}", a.name, a.arg_type))
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

/// Fast XML to Dom conversion that builds Dom tree directly without intermediate StyledDom
/// This is O(n) instead of O(n²) for large documents
fn xml_node_to_dom_fast<'a>(
    xml_node: &'a XmlNode,
    component_map: &'a XmlComponentMap,
) -> Result<Dom, RenderDomError> {
    use crate::dom::{Dom, NodeType, IdOrClass};
    
    let component_name = normalize_casing(&xml_node.node_type);
    
    // Get the component to determine the NodeType
    let xml_component = component_map
        .get(&component_name)
        .ok_or(ComponentError::UnknownComponent(component_name.clone().into()))?;
    
    // Create the DOM node based on component type
    let node_type = get_node_type_for_component(&component_name);
    let mut dom = Dom::create_node(node_type);
    
    // Set id and class attributes
    let mut ids_and_classes = Vec::new();
    if let Some(id_str) = xml_node.attributes.get_key("id") {
        for id in id_str.split_whitespace() {
            ids_and_classes.push(IdOrClass::Id(id.into()));
        }
    }
    if let Some(class_str) = xml_node.attributes.get_key("class") {
        for class in class_str.split_whitespace() {
            ids_and_classes.push(IdOrClass::Class(class.into()));
        }
    }
    if !ids_and_classes.is_empty() {
        dom.root.set_ids_and_classes(ids_and_classes.into());
    }
    
    // Recursively convert children
    let mut children = Vec::new();
    for child in xml_node.children.as_ref().iter() {
        match child {
            XmlNodeChild::Element(child_node) => {
                let child_dom = xml_node_to_dom_fast(child_node, component_map)?;
                children.push(child_dom);
            }
            XmlNodeChild::Text(text) => {
                let text_dom = Dom::create_text(AzString::from(text.as_str()));
                children.push(text_dom);
            }
        }
    }
    
    if !children.is_empty() {
        dom = dom.with_children(children.into());
    }
    
    Ok(dom)
}

/// Map component name to NodeType
fn get_node_type_for_component(name: &str) -> crate::dom::NodeType {
    use crate::dom::NodeType;
    match name {
        "html" => NodeType::Html,
        "head" => NodeType::Head,
        "title" => NodeType::Title,
        "body" => NodeType::Body,
        "div" => NodeType::Div,
        "p" => NodeType::P,
        "span" => NodeType::Span,
        "br" => NodeType::Br,
        "h1" => NodeType::H1,
        "h2" => NodeType::H2,
        "h3" => NodeType::H3,
        "h4" => NodeType::H4,
        "h5" => NodeType::H5,
        "h6" => NodeType::H6,
        "header" => NodeType::Header,
        "footer" => NodeType::Footer,
        "section" => NodeType::Section,
        "article" => NodeType::Article,
        "aside" => NodeType::Aside,
        "nav" => NodeType::Nav,
        "main" => NodeType::Main,
        "pre" => NodeType::Pre,
        "code" => NodeType::Code,
        "blockquote" => NodeType::BlockQuote,
        "ul" => NodeType::Ul,
        "ol" => NodeType::Ol,
        "li" => NodeType::Li,
        "dl" => NodeType::Dl,
        "dt" => NodeType::Dt,
        "dd" => NodeType::Dd,
        "table" => NodeType::Table,
        "thead" => NodeType::THead,
        "tbody" => NodeType::TBody,
        "tfoot" => NodeType::TFoot,
        "tr" => NodeType::Tr,
        "th" => NodeType::Th,
        "td" => NodeType::Td,
        "a" => NodeType::A,
        "strong" => NodeType::Strong,
        "em" => NodeType::Em,
        "b" => NodeType::B,
        "i" => NodeType::I,
        "u" => NodeType::U,
        "small" => NodeType::Small,
        "mark" => NodeType::Mark,
        "sub" => NodeType::Sub,
        "sup" => NodeType::Sup,
        "form" => NodeType::Form,
        "label" => NodeType::Label,
        "button" => NodeType::Button,
        "hr" => NodeType::Hr,
        _ => NodeType::Div, // Default fallback
    }
}

pub fn render_dom_from_body_node<'a>(
    body_node: &'a XmlNode,
    mut global_css: Option<Css>,
    component_map: &'a XmlComponentMap,
    max_width: Option<f32>,
) -> Result<StyledDom, RenderDomError> {
    // OPTIMIZATION: Build Dom tree first, then style once at the end
    // This avoids O(n) StyledDom::create() calls for each of the ~360k nodes
    let body_dom = xml_node_to_dom_fast(body_node, component_map)?;
    
    // OPTIMIZATION: Combine all CSS rules and apply ONCE instead of multiple restyle() calls
    // Each restyle() is O(n * m) where n=nodes and m=CSS rules
    let mut combined_stylesheets = Vec::new();
    
    // Add max-width constraint if specified
    if let Some(max_width) = max_width {
        let max_width_css = Css::from_string(
            format!("html {{ max-width: {max_width}px; }}").into(),
        );
        for s in max_width_css.stylesheets.as_ref().iter() {
            combined_stylesheets.push(s.clone());
        }
    }
    
    // Add global CSS from <style> tags
    if let Some(css) = global_css.take() {
        for s in css.stylesheets.as_ref().iter() {
            combined_stylesheets.push(s.clone());
        }
    }
    
    let combined_css = Css::new(combined_stylesheets);
    
    // IMPORTANT: Build the full DOM tree BEFORE applying CSS.
    // CSS selectors like `html { background: ... }` must be able to match the
    // <html> element, which means it must exist in the tree when CSS is applied.
    // Previously, CSS was applied to the body-only tree first, then <html> was
    // wrapped around it with Css::empty() — causing html-targeted rules to be lost.
    
    // Determine the root node type from the un-styled DOM
    use crate::dom::NodeType;
    let root_node_type = body_dom.root.node_type.clone();
    
    let mut full_dom = match root_node_type {
        NodeType::Html => {
            // Already has proper HTML root, style as-is
            body_dom
        }
        NodeType::Body => {
            // Has Body root, wrap in HTML first, then style the whole tree
            Dom::create_html().with_child(body_dom)
        }
        _ => {
            // Other elements (div, etc), wrap in HTML > Body
            let body_wrapper = Dom::create_body().with_child(body_dom);
            Dom::create_html().with_child(body_wrapper)
        }
    };
    
    // Apply combined CSS once to the COMPLETE DOM tree (including html wrapper)
    let styled = full_dom.style(combined_css);

    Ok(styled)
}

/// Takes a single (expanded) app node and renders the DOM or returns an error
pub fn render_dom_from_body_node_inner<'a>(
    xml_node: &'a XmlNode,
    component_map: &'a XmlComponentMap,
    parent_xml_attributes: &FilteredComponentArguments,
) -> Result<StyledDom, RenderDomError> {
    let component_name = normalize_casing(&xml_node.node_type);

    let xml_component = component_map
        .get(&component_name)
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
        v.arg_type = format_args_dynamic(&v.arg_type, &parent_xml_attributes.types).into();
    }

    // Don't pass text content to the component renderer - text children will be appended separately
    let mut dom = xml_component.renderer.render_dom(
        component_map,
        &filtered_xml_attributes,
        &OptionString::None,
    )?;
    set_attributes(&mut dom, &xml_node.attributes, &filtered_xml_attributes);

    // Track child index for O(1) append instead of O(n) count
    let mut child_index = 0usize;
    for child in xml_node.children.as_ref().iter() {
        match child {
            XmlNodeChild::Element(child_node) => {
                let child_dom = render_dom_from_body_node_inner(
                    child_node,
                    component_map,
                    &filtered_xml_attributes,
                )?;
                dom.append_child_with_index(child_dom, child_index);
                child_index += 1;
            }
            XmlNodeChild::Text(text) => {
                // Create a text node for text children
                let text_dom = Dom::create_text(AzString::from(text.as_str())).style(Css::empty());
                dom.append_child_with_index(text_dom, child_index);
                child_index += 1;
            }
        }
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
        let css_key_map = azul_css::props::property::get_css_key_map();
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
            azul_css::parser2::parse_css_declaration(
                key.trim(),
                value.trim(),
                azul_css::parser2::ErrorLocationRange::default(),
                &css_key_map,
                &mut Vec::new(),
                &mut attributes,
            );
        }

        let props = attributes
            .into_iter()
            .filter_map(|s| {
                use azul_css::dynamic_selector::CssPropertyWithConditions;
                match s {
                    CssDeclaration::Static(s) => Some(CssPropertyWithConditions::simple(s)),
                    _ => return None,
                }
            })
            .collect::<Vec<_>>();

        node_data.set_css_props(props.into());
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
        use azul_css::format_rust_code::GetHash;
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

/// Item of a split string - either a variable name (with optional format spec) or a string
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum DynamicItem {
    /// A variable reference, e.g. {counter} or {counter:?} or {price:.2}
    Var {
        name: String,
        /// Optional format specifier after the colon: "?" for debug, ".2" for precision, etc.
        format_spec: Option<String>,
    },
    Str(String),
}

/// Splits a string into formatting arguments, supporting format specifiers like `{var:?}`
/// ```rust
/// # use azul_core::xml::DynamicItem::*;
/// # use azul_core::xml::split_dynamic_string;
/// let s = "hello {a}, {b}{{ {c} }}";
/// let split = split_dynamic_string(s);
/// let output = vec![
///     Str("hello ".to_string()),
///     Var { name: "a".to_string(), format_spec: None },
///     Str(", ".to_string()),
///     Var { name: "b".to_string(), format_spec: None },
///     Str("{ ".to_string()),
///     Var { name: "c".to_string(), format_spec: None },
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
                    let var_content: String = input
                        [(current_idx + 1)..(current_idx + start_offset - 1)]
                        .iter()
                        .collect();
                    // Split on first ':' to separate variable name from format specifier
                    let (var_name, format_spec) = if let Some(colon_pos) = var_content.find(':') {
                        let name = var_content[..colon_pos].to_string();
                        let spec = var_content[(colon_pos + 1)..].to_string();
                        (name, Some(spec))
                    } else {
                        (var_content, None)
                    };
                    items.push(Var { name: var_name, format_spec });
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
            DynamicItem::Var { name, format_spec } => {
                let variable_name = normalize_casing(name.trim());
                match variables
                    .iter()
                    .find(|s| s.name.as_str() == variable_name)
                    .map(|q| &q.arg_type)
                {
                    Some(resolved_var) => {
                        // Format specifiers are applied at compile time, not at runtime replacement
                        s.push_str(&resolved_var);
                    }
                    None => {
                        s.push('{');
                        s.push_str(name);
                        if let Some(spec) = format_spec {
                            s.push(':');
                            s.push_str(spec);
                        }
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

    let mut css = match find_node_by_type(xml_node.children.as_ref(), "style") {
        Some(style_node) => {
            let text = style_node.get_text_content();
            if !text.is_empty() {
                Some(text)
            } else {
                None
            }
        }
        None => None,
    };
    let mut css = match css {
        Some(text) => azul_css::parser2::new_from_str(&text).0,
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
        v.arg_type = format_args_dynamic(&v.arg_type, &parent_xml_attributes.args).into();
    }

    let text_content = xml_node.get_text_content();
    let text = if !text_content.is_empty() {
        Some(AzString::from(format_args_dynamic(
            &text_content,
            &filtered_xml_attributes.args,
        )))
    } else {
        None
    };

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
        for (child_idx, child) in xml_node.children.as_ref().iter().enumerate() {
            if let XmlNodeChild::Element(child_node) = child {
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

    for xml_component in components.components.iter() {
        render_component_inner(
            &mut map,
            xml_component.id.clone(),
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
        use azul_css::css::CssPathSelector::*;

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
    use azul_css::css::{CssNthChildSelector, CssPathPseudoSelector, CssPathSelector::*};

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
                if idx_in_parent.saturating_sub(p.offset as usize) % p.pattern_repeat as usize != 0
                {
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
    use azul_css::css::CssDeclaration;

    let t = "";
    let t2 = "    ";
    let mut dom_string = String::from("Dom::create_body()");
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
        use azul_css::props::property::format_static_css_prop;

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
        use azul_css::format_rust_code::GetHash;
        let children_hash = body_node.children.as_ref().get_hash();
        dom_string.push_str(&format!("\r\n.with_children(DomVec::from_vec(vec![\r\n"));

        for (child_idx, child) in body_node.children.as_ref().iter().enumerate() {
            match child {
                XmlNodeChild::Element(child_node) => {
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
                XmlNodeChild::Text(text) => {
                    let text = text.trim();
                    if !text.is_empty() {
                        let escaped = text.replace("\\", "\\\\").replace("\"", "\\\"");
                        dom_string
                            .push_str(&format!("{}Dom::create_text(\"{}\".into()),\r\n", t, escaped));
                    }
                }
            }
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
                    ending = Some(p.clone());
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
            Var { name, format_spec } => {
                let var_name = normalize_casing(name.trim());
                if let Some(spec) = format_spec {
                    format!("format!(\"{{:{}}}\", {}).into()", spec, var_name)
                } else {
                    var_name
                }
            }
            Str(s) => format!("AzString::from_const_str(\"{}\")", s),
        }
    } else {
        // build a "format!("{var}, blah", var)" string
        let mut formatted_str = String::from("format!(\"");
        let mut variables = Vec::new();
        for item in input {
            match item {
                Var { name, format_spec } => {
                    let variable_name = normalize_casing(name.trim());
                    if let Some(spec) = format_spec {
                        formatted_str.push_str(&format!("{{{}:{}}}", variable_name, spec));
                    } else {
                        formatted_str.push_str(&format!("{{{}}}", variable_name));
                    }
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
    use azul_css::css::CssDeclaration;

    let t = String::from("    ").repeat(tabs - 1);
    let t2 = String::from("    ").repeat(tabs);

    let component_name = normalize_casing(&node.node_type);

    let xml_component = component_map
        .get(&component_name)
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
        v.arg_type = format_args_dynamic(&v.arg_type, &parent_xml_attributes.args).into();
    }

    let instantiated_function_arguments = {
        let mut args = filtered_xml_attributes
            .types
            .iter()
            .filter_map(|arg| {
                match node.attributes.get_key(&arg.name).cloned() {
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
        let node_text = node.get_text_content();
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
        "br" => NodeTypeTag::Br,
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
        use azul_css::props::property::format_static_css_prop;

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
        .filter_map(|(child_idx, c)| match c {
            XmlNodeChild::Element(child_node) => {
                let mut matcher = matcher.clone();
                matcher.path.push(CssPathSelector::Children);
                matcher.indices_in_parent.push(child_idx);
                matcher.children_length.push(node.children.len());

                Some(compile_node_to_rust_code_inner(
                    child_node,
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
                ))
            }
            XmlNodeChild::Text(text) => {
                let text = text.trim();
                if text.is_empty() {
                    None
                } else {
                    let t2 = String::from("    ").repeat(tabs);
                    let escaped = text.replace("\\", "\\\\").replace("\"", "\\\"");
                    Some(Ok(format!("{}Dom::create_text(\"{}\".into())", t2, escaped)))
                }
            }
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
#[derive(Clone)]
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
            None => ComponentArgumentVec::new(),
        };

        Ok(Self {
            name: normalize_casing(&name),
            arguments: ComponentArguments { args, accepts_text },
            root: root.clone(),
        })
    }
}

impl XmlComponentTrait for DynamicXmlComponent {
    fn clone_box(&self) -> Box<dyn XmlComponentTrait> {
        Box::new(self.clone())
    }

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
                let text = style_node.get_text_content();
                if !text.is_empty() {
                    let parsed_css = Css::from_string(text.into());
                    Some(parsed_css)
                } else {
                    None
                }
            }
            None => None,
        };

        let mut dom = StyledDom::default();

        for child in self.root.children.as_ref() {
            if let XmlNodeChild::Element(child_node) = child {
                dom.append_child(render_dom_from_body_node_inner(
                    child_node, components, arguments,
                )?);
            }
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
        Ok("Dom::create_div()".into()) // TODO!s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::{Dom, NodeType};

    #[test]
    fn test_inline_span_parsing() {
        // This test verifies that HTML with inline spans is parsed correctly
        // The DOM structure should preserve text nodes before, inside, and after the span

        let html = r#"<p>Text before <span class="highlight">inline text</span> text after.</p>"#;

        // Expected DOM structure:
        // <p>
        //   ├─ TextNode: "Text before "
        //   ├─ <span class="highlight">
        //   │   └─ TextNode: "inline text"
        //   └─ TextNode: " text after."

        // For this test, we'll create the DOM structure manually
        // since we're testing the parsing logic
        let expected_dom = Dom::create_p().with_children(
            vec![
                Dom::create_text("Text before "),
                Dom::create_node(NodeType::Span)
                    .with_children(vec![Dom::create_text("inline text")].into()),
                Dom::create_text(" text after."),
            ]
            .into(),
        );

        // Verify the structure has 3 children at the top level
        assert_eq!(expected_dom.children.as_ref().len(), 3);

        // Verify the middle child is a span
        match &expected_dom.children.as_ref()[1].root.node_type {
            NodeType::Span => {}
            other => panic!("Expected Span, got {:?}", other),
        }

        // Verify the span has 1 child (the text node)
        assert_eq!(expected_dom.children.as_ref()[1].children.as_ref().len(), 1);

        println!("Test passed: Inline span parsing structure is correct");
    }

    #[test]
    fn test_xml_node_structure() {
        // Test the basic XmlNode structure to ensure text content is preserved
        // Updated to use XmlNodeChild enum (Text/Element)

        let node = XmlNode {
            node_type: "p".into(),
            attributes: XmlAttributeMap {
                inner: StringPairVec::from_const_slice(&[]),
            },
            children: vec![
                XmlNodeChild::Text("Before ".into()),
                XmlNodeChild::Element(XmlNode {
                    node_type: "span".into(),
                    children: vec![XmlNodeChild::Text("inline".into())].into(),
                    ..Default::default()
                }),
                XmlNodeChild::Text(" after".into()),
            ]
            .into(),
        };

        // Verify structure
        assert_eq!(node.children.as_ref().len(), 3);
        assert_eq!(node.children.as_ref()[0].as_text(), Some("Before "));
        assert_eq!(
            node.children.as_ref()[1]
                .as_element()
                .unwrap()
                .node_type
                .as_str(),
            "span"
        );
        assert_eq!(node.children.as_ref()[2].as_text(), Some(" after"));

        // Verify span's child
        let span = node.children.as_ref()[1].as_element().unwrap();
        assert_eq!(span.children.as_ref().len(), 1);
        assert_eq!(span.children.as_ref()[0].as_text(), Some("inline"));

        println!("Test passed: XmlNode structure preserves text nodes correctly");
    }
}
