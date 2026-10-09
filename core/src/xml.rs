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
    string::{String, ToString},
    vec::Vec,
};
use core::{fmt, fmt::Write, hash::Hash};

use azul_css::{
    css::{Css, CssRuleBlock, NodeTypeTag},
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
    dom::{Dom, NodeData, NodeType, OptionNodeType},
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

/// Name of a component argument (e.g. `"text"`, `"href"`).
type ComponentArgumentName = String;
/// Type of a component argument as a string (e.g. `"String"`, `"bool"`).
type ComponentArgumentType = String;
/// Zero-based position of an argument in the component's argument list.
type ComponentArgumentOrder = usize;

/// FFI-safe replacement for `(ComponentArgumentName, ComponentArgumentType)` tuple.
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ComponentArgument {
    pub name: AzString,
    pub arg_type: AzString,
}

impl_vec!(
    ComponentArgument,
    ComponentArgumentVec,
    ComponentArgumentVecDestructor,
    ComponentArgumentVecDestructorType,
    ComponentArgumentVecSlice,
    OptionComponentArgument
);
impl_option!(
    ComponentArgument,
    OptionComponentArgument,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_vec_debug!(ComponentArgument, ComponentArgumentVec);
impl_vec_partialeq!(ComponentArgument, ComponentArgumentVec);
impl_vec_eq!(ComponentArgument, ComponentArgumentVec);
impl_vec_partialord!(ComponentArgument, ComponentArgumentVec);
impl_vec_ord!(ComponentArgument, ComponentArgumentVec);
impl_vec_hash!(ComponentArgument, ComponentArgumentVec);
impl_vec_clone!(
    ComponentArgument,
    ComponentArgumentVec,
    ComponentArgumentVecDestructor
);
impl_vec_mut!(ComponentArgument, ComponentArgumentVec);

/// Universal HTML attribute names that are handled by the framework
/// and should not be passed through to component-specific argument lists.
const DEFAULT_ARGS: [&str; 8] = [
    "id",
    "class",
    "tabindex",
    "focusable",
    "accepts_text",
    "name",
    "style",
    "args",
];

/// Opaque void type for FFI pointers. Uses a custom definition instead of
/// `core::ffi::c_void` for `#[repr(C)]` compatibility in the generated API.
#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone)]
pub enum c_void {}

/// Type of an XML node in the parsed tree.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub enum XmlNodeType {
    Root,
    Element,
    PI,
    Comment,
    Text,
}

/// A namespace-qualified XML name (e.g. `svg:rect` has namespace `"svg"` and local name `"rect"`).
#[repr(C)]
#[derive(Debug)]
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
    #[must_use]
    pub fn new(s: &str) -> Self {
        Self {
            inner: AzString::from(s),
        }
    }

    #[must_use]
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
            "js" | "mjs" => "application/javascript",
            // Data and text
            "json" => "application/json",
            "txt" => "text/plain",
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
        Self {
            inner: AzString::from(mime),
        }
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

impl_vec!(
    ExternalResource,
    ExternalResourceVec,
    ExternalResourceVecDestructor,
    ExternalResourceVecDestructorType,
    ExternalResourceVecSlice,
    OptionExternalResource
);
impl_vec_mut!(ExternalResource, ExternalResourceVec);
impl_vec_debug!(ExternalResource, ExternalResourceVec);
impl_vec_partialeq!(ExternalResource, ExternalResourceVec);
impl_vec_eq!(ExternalResource, ExternalResourceVec);
impl_vec_partialord!(ExternalResource, ExternalResourceVec);
impl_vec_ord!(ExternalResource, ExternalResourceVec);
impl_vec_hash!(ExternalResource, ExternalResourceVec);
impl_vec_clone!(
    ExternalResource,
    ExternalResourceVec,
    ExternalResourceVecDestructor
);

/// AUDIT 2026-07-08: maximum XML/HTML nesting depth handled by the recursive
/// DOM-build (`xml_node_to_dom_fast`, `xml_node_to_fast_dom`), resource-scan
/// (iterative worklist in `scan_external_resources`) and `<body>`-lookup
/// (`find_body_recursive`) passes. These bound descent per nesting level, so a pathologically deep
/// document (e.g. tens of thousands of nested `<div>`s) would overflow the native
/// stack. Beyond this depth, deeper children are ignored rather than crashing.
/// 512 is far past any realistic hand-authored markup while staying comfortably
/// inside the default thread stack.
pub(crate) const MAX_XML_NESTING_DEPTH: usize = 512;

/// AUDIT 2026-07-08: maximum recursion depth for [`ComponentFieldType::parse`],
/// which recurses through `Option<..>` / `Vec<..>` wrappers. Caps attacker
/// strings such as `"Option<".repeat(100_000)` that would otherwise overflow the
/// stack. 64 nested type wrappers is far beyond any real field type.
const MAX_TYPE_PARSE_DEPTH: usize = 64;

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
#[repr(C)]
pub struct Xml {
    pub root: XmlNodeChildVec,
}

impl Xml {
    /// Parses real-world HTML the way a browser builds its tree - never
    /// fails (unquoted attributes, `<br>` without a slash, implied end tags,
    /// stray end tags, Word / Outlook markup, the HTML named references; see
    /// [`html`]). A fragment is a document: `<html>`, `<head>` and `<body>`
    /// are implied. The strict XML loaders stay strict.
    #[must_use]
    pub fn create_from_html(html: AzString) -> Self {
        Self {
            root: html::parse_html_nodes(html.as_str()).into(),
        }
    }

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
    /// - `<style>` blocks with @import or `url()`
    #[must_use]
    pub fn scan_external_resources(&self) -> ExternalResourceVec {
        let mut resources = Vec::new();

        // AUDIT 2026-07-08: iterative DFS with an explicit worklist. The old
        // per-node recursion overflowed the stack on pathologically deep markup
        // (a single-purpose scan frame is large: string lowercasing + closure +
        // wide match). An explicit stack keeps memory on the heap; `depth` still
        // bounds how deep we descend so unbounded input can't grow the worklist
        // without limit.
        //
        // The stack pops LAST first, so children go on in REVERSE: the first
        // child comes off next and the scan is the document's pre-order (it
        // listed siblings last first). Text is never CSS by itself - a
        // `<style>`'s text is scanned by its element (`scan_node`); scanning
        // every text node made prose that says `url(...)` a resource and read
        // each stylesheet twice.
        let mut stack: Vec<(&XmlNodeChild, usize)> = Vec::new();
        for child in self.root.as_ref().iter().rev() {
            stack.push((child, 0));
        }
        while let Some((child, depth)) = stack.pop() {
            if let XmlNodeChild::Element(node) = child {
                if depth > MAX_XML_NESTING_DEPTH {
                    // Deeper subtrees are simply not scanned.
                    continue;
                }
                Self::scan_node(node, &mut resources);
                for c in node.children.as_ref().iter().rev() {
                    stack.push((c, depth + 1));
                }
            }
        }

        resources.into()
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive:
                                                                   // single-purpose
                                                                   // parser/builder/dispatch (one
                                                                   // branch per input variant)
    fn scan_node(node: &XmlNode, resources: &mut Vec<ExternalResource>) {
        let tag_name = node.node_type.inner.as_str().to_lowercase();

        // Get attribute lookup helper
        let get_attr = |name: &str| -> Option<String> {
            node.attributes
                .inner
                .as_ref()
                .iter()
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
                    } else if as_attr == "font" {
                        (ExternalResourceKind::Font, "font")
                    } else if as_attr == "script" {
                        (ExternalResourceKind::Script, "script")
                    } else if as_attr == "image" {
                        (ExternalResourceKind::Image, "image")
                    } else {
                        (ExternalResourceKind::Unknown, "")
                    };

                    let mime = type_attr
                        .map(|t| MimeTypeHint::new(&t))
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
                    let mime = type_attr
                        .map(|t| MimeTypeHint::new(&t))
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
                    let kind = if type_attr.as_ref().is_some_and(|t| t.starts_with("audio")) {
                        ExternalResourceKind::Audio
                    } else {
                        ExternalResourceKind::Video
                    };
                    let mime = type_attr.map(|t| MimeTypeHint::new(&t)).or_else(|| {
                        Self::guess_mime_from_url(
                            &src,
                            if kind == ExternalResourceKind::Audio {
                                "audio"
                            } else {
                                "video"
                            },
                        )
                    });

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
            "virtualized-view" | "embed" | "object" => {
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
                for child in node.children.as_ref() {
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

        // Children are walked by the iterative driver in `scan_external_resources`.
    }

    /// Extract URLs from CSS content (handles `url()` and @import)
    fn extract_css_urls(css: &str, resources: &mut Vec<ExternalResource>) {
        // AUDIT 2026-07-08: fold to lowercase ONCE using ASCII-only folding.
        // `to_ascii_lowercase` never changes a string's byte length (only A-Z are
        // touched, multi-byte code points are left verbatim), so every byte offset
        // into `lower` maps 1:1 onto `css`. The old code called `to_lowercase()`
        // every iteration (O(n^2)) and then sliced the ORIGINAL `css` with an
        // offset computed in the lowercased temporary -- for characters whose
        // lowercase changes byte length (e.g. 'İ' U+0130, 2 bytes -> 3 bytes) that
        // offset landed off a char boundary and panicked. Searching in `lower` and
        // slicing `css` at the same offset also makes the `url(` / `@import` scans
        // case-insensitive for free.
        let lower = css.to_ascii_lowercase();

        // url(...) scan (case-insensitive)
        let mut search_from = 0;
        while let Some(rel) = lower[search_from..].find("url(") {
            let url_start = search_from + rel;
            let after = url_start + 4;
            // Skip a `url(` that is the argument of an `@import` — the @import scan
            // below emits it, correctly tagged as a Stylesheet. Without this guard the
            // same URL is pushed twice (once here, mistagged "url()").
            if lower[..url_start].trim_end().ends_with("@import") {
                search_from = after;
                continue;
            }
            let after_url = &css[after..];
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
            search_from = after;
        }

        // Handle @import "url" or @import url(...) (case-insensitive)
        let mut search_from = 0;
        while let Some(rel) = lower[search_from..].find("@import") {
            let after = search_from + rel + 7;
            let after_import = &css[after..];
            let trimmed = after_import.trim_start();

            // Match `url(` case-insensitively without allocating. `get(..4)`
            // returns `None` if byte 4 is not a char boundary, so the slice below
            // can never panic on multi-byte input.
            let import_url = if trimmed
                .get(..4)
                .is_some_and(|p| p.eq_ignore_ascii_case("url("))
            {
                Self::extract_url_value(&trimmed[4..])
            } else {
                Self::extract_quoted_string(trimmed)
            };

            if let Some(url) = import_url {
                resources.push(ExternalResource {
                    url: AzString::from(url),
                    kind: ExternalResourceKind::Stylesheet,
                    mime_type: Some(MimeTypeHint::new("text/css")).into(),
                    source_element: AzString::from("style"),
                    source_attribute: AzString::from("@import"),
                });
            }

            search_from = after;
        }
    }

    /// Extract value from url(...) - handles quoted and unquoted URLs
    fn extract_url_value(s: &str) -> Option<String> {
        let trimmed = s.trim_start();
        if trimmed.starts_with('"') {
            Self::extract_quoted_string(trimmed)
        } else if let Some(rest) = trimmed.strip_prefix('\'') {
            let end = rest.find('\'')?;
            Some(rest[..end].to_string())
        } else {
            let end = trimmed.find(')')?;
            Some(trimmed[..end].trim().to_string())
        }
    }

    /// Extract a quoted string value
    fn extract_quoted_string(s: &str) -> Option<String> {
        if let Some(rest) = s.strip_prefix('"') {
            let end = rest.find('"')?;
            Some(rest[..end].to_string())
        } else if let Some(rest) = s.strip_prefix('\'') {
            let end = rest.find('\'')?;
            Some(rest[..end].to_string())
        } else {
            None
        }
    }

    /// Parse srcset attribute into individual URLs
    fn parse_srcset(srcset: &str) -> Vec<String> {
        srcset
            .split(',')
            .filter_map(|entry| {
                let trimmed = entry.trim();
                // srcset format: "url 1x" or "url 100w"
                trimmed
                    .split_whitespace()
                    .next()
                    .map(alloc::string::ToString::to_string)
            })
            .filter(|url| !url.is_empty())
            .collect()
    }

    /// Check if a URL looks like a downloadable resource (not a page)
    fn looks_like_resource(url: &str) -> bool {
        let lower = url.to_lowercase();
        // Check for common resource extensions
        let resource_exts = [
            ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".ico", ".bmp", ".ttf", ".otf",
            ".woff", ".woff2", ".eot", ".css", ".js", ".mp4", ".webm", ".ogg", ".mp3", ".wav",
            ".pdf", ".zip", ".tar", ".gz",
        ];
        resource_exts.iter().any(|ext| lower.ends_with(ext))
    }

    /// Guess the resource kind from URL based on file extension.
    // `url` is lowercased into `path` below, so these literal `.ext` checks are
    // already case-insensitive — the lint can't see the runtime lowercasing.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    fn guess_kind_from_url(url: &str) -> ExternalResourceKind {
        let lower = url.to_lowercase();
        // Strip query string before checking extension
        let path = lower.split('?').next().unwrap_or(&lower);
        if path.ends_with(".png")
            || path.ends_with(".jpg")
            || path.ends_with(".jpeg")
            || path.ends_with(".gif")
            || path.ends_with(".webp")
            || path.ends_with(".svg")
            || path.ends_with(".bmp")
            || path.ends_with(".avif")
        {
            ExternalResourceKind::Image
        } else if path.ends_with(".ttf")
            || path.ends_with(".otf")
            || path.ends_with(".woff")
            || path.ends_with(".woff2")
            || path.ends_with(".eot")
        {
            ExternalResourceKind::Font
        } else if path.ends_with(".css") {
            ExternalResourceKind::Stylesheet
        } else if path.ends_with(".js") || path.ends_with(".mjs") {
            ExternalResourceKind::Script
        } else if path.ends_with(".mp4") || path.ends_with(".webm") || path.ends_with(".ogg") {
            ExternalResourceKind::Video
        } else if path.ends_with(".mp3") || path.ends_with(".wav") || path.ends_with(".flac") {
            ExternalResourceKind::Audio
        } else if path.ends_with(".ico") {
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
            "png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp", "avif", "ttf", "otf", "woff",
            "woff2", "eot", "css", "js", "mjs", "mp4", "webm", "ogg", "mp3", "wav", "flac",
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
#[repr(C)]
pub struct NonXmlCharError {
    pub ch: u32, /* u32 = char, but ABI stable */
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
#[repr(C)]
pub struct InvalidCharError {
    pub expected: u8,
    pub got: u8,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
#[repr(C)]
pub struct InvalidCharMultipleError {
    pub expected: u8,
    pub got: U8Vec,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
#[repr(C)]
pub struct InvalidQuoteError {
    pub got: u8,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
#[repr(C)]
pub struct InvalidSpaceError {
    pub got: u8,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
#[repr(C)]
pub struct InvalidStringError {
    pub got: AzString,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::XmlStreamError::{
            InvalidChar, InvalidCharMultiple, InvalidCharacterData, InvalidCommentData,
            InvalidCommentEnd, InvalidExternalID, InvalidName, InvalidQuote, InvalidReference,
            InvalidSpace, InvalidString, NonXmlChar, UnexpectedEndOfStream,
        };
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

#[derive(Debug, PartialEq, PartialOrd, Clone, Copy, Ord, Hash, Eq)]
#[repr(C)]
pub struct XmlTextPos {
    pub row: u32,
    pub col: u32,
}

impl fmt::Display for XmlTextPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}:{}", self.row, self.col)
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
#[repr(C)]
pub struct XmlTextError {
    pub stream_error: XmlStreamError,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::XmlParseError::{
            InvalidAttribute, InvalidCdata, InvalidCharData, InvalidComment, InvalidDeclaration,
            InvalidDoctype, InvalidElement, InvalidEntity, InvalidPI, UnknownToken,
        };
        match self {
            InvalidDeclaration(e) => {
                write!(f, "Invalid declaration: {} at {}", e.stream_error, e.pos)
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
            UnknownToken(e) => write!(f, "Unknown token at {e}"),
        }
    }
}

impl_result!(
    Xml,
    XmlError,
    ResultXmlXmlError,
    copy = false,
    [Debug, PartialEq, Eq, PartialOrd, Clone]
);

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
#[repr(C)]
pub struct DuplicatedNamespaceError {
    pub ns: AzString,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
#[repr(C)]
pub struct UnknownNamespaceError {
    pub ns: AzString,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
#[repr(C)]
pub struct UnexpectedCloseTagError {
    pub expected: AzString,
    pub actual: AzString,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
#[repr(C)]
pub struct UnknownEntityReferenceError {
    pub entity: AzString,
    pub pos: XmlTextPos,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
#[repr(C)]
pub struct DuplicatedAttributeError {
    pub attribute: AzString,
    pub pos: XmlTextPos,
}

/// Error for mismatched open/close tags in XML hierarchy
#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
#[repr(C)]
pub struct MalformedHierarchyError {
    /// The tag that was expected (from the opening tag)
    pub expected: AzString,
    /// The tag that was actually found (the closing tag)
    pub got: AzString,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
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
    #[allow(clippy::too_many_lines)] // large but cohesive: one arm per variant
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::XmlError::{
            AttributesLimitReached, DtdDetected, DuplicatedAttribute, DuplicatedNamespace,
            EntityReferenceLoop, InvalidAttributeValue, InvalidChar, InvalidChar2,
            InvalidCharacterData, InvalidComment, InvalidElementNamePrefix, InvalidExternalID,
            InvalidName, InvalidString, InvalidXmlPrefixUri, MalformedEntityReference,
            MalformedHierarchy, NamespacesLimitReached, NoParserAvailable, NoRootNode,
            NodesLimitReached, NonXmlChar, ParserError, SizeLimit, UnclosedRootNode,
            UnexpectedCloseTag, UnexpectedDeclaration, UnexpectedEndOfStream,
            UnexpectedEntityCloseTag, UnexpectedXmlUri, UnexpectedXmlnsUri, UnknownEntityReference,
            UnknownNamespace, UnknownToken,
        };
        match self {
            NoParserAvailable => write!(
                f,
                "Library was compiled without XML parser (XML parser not available)"
            ),
            InvalidXmlPrefixUri(pos) => {
                write!(f, "Invalid XML Prefix URI at line {}:{}", pos.row, pos.col)
            }
            UnexpectedXmlUri(pos) => {
                write!(f, "Unexpected XML URI at line {}:{}", pos.row, pos.col)
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
            ParserError(p) => write!(f, "{p}"),
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

// ============================================================================
// New repr(C) component system
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
    #[must_use]
    pub fn builtin(name: &str) -> Self {
        Self {
            collection: AzString::from_const_str("builtin"),
            name: AzString::from(name),
        }
    }

    #[must_use]
    pub fn new(collection: &str, name: &str) -> Self {
        Self {
            collection: AzString::from(collection),
            name: AzString::from(name),
        }
    }

    /// Returns "collection:name" format string
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}:{}", self.collection.as_str(), self.name.as_str())
    }
}

// ============================================================================
// Component type system — rich type descriptors for component fields
// ============================================================================

/// A single argument in a callback signature.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ComponentCallbackArg {
    /// Argument name, e.g. "`button_id`"
    pub name: AzString,
    /// Argument type
    pub arg_type: ComponentFieldType,
}

impl_vec!(
    ComponentCallbackArg,
    ComponentCallbackArgVec,
    ComponentCallbackArgVecDestructor,
    ComponentCallbackArgVecDestructorType,
    ComponentCallbackArgVecSlice,
    OptionComponentCallbackArg
);
impl_option!(
    ComponentCallbackArg,
    OptionComponentCallbackArg,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_vec_debug!(ComponentCallbackArg, ComponentCallbackArgVec);
impl_vec_partialeq!(ComponentCallbackArg, ComponentCallbackArgVec);
impl_vec_eq!(ComponentCallbackArg, ComponentCallbackArgVec);
impl_vec_partialord!(ComponentCallbackArg, ComponentCallbackArgVec);
impl_vec_ord!(ComponentCallbackArg, ComponentCallbackArgVec);
impl_vec_hash!(ComponentCallbackArg, ComponentCallbackArgVec);
impl_vec_clone!(
    ComponentCallbackArg,
    ComponentCallbackArgVec,
    ComponentCallbackArgVecDestructor
);

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
    #[must_use]
    pub fn new(t: ComponentFieldType) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(t)),
        }
    }

    #[must_use]
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
        // Null the pointer as we free it, so a *second* drop is a no-op instead
        // of a double free. This type is a by-value payload of the
        // `ComponentFieldType` enum, whose codegen FFI mirror gets
        // `impl Drop { _delete }` (= drop_in_place of the real type) AND Rust
        // field drop-glue — dropping each by-value field twice. Without this
        // take-and-null the second drop would `Box::from_raw` a dangling pointer.
        let ptr = core::mem::replace(&mut self.ptr, core::ptr::null_mut());
        if !ptr.is_null() {
            unsafe {
                drop(Box::from_raw(ptr));
            }
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
        if self.ptr.is_null() && other.ptr.is_null() {
            return true;
        }
        if self.ptr.is_null() || other.ptr.is_null() {
            return false;
        }
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

impl Hash for ComponentFieldTypeBox {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        if !self.ptr.is_null() {
            unsafe {
                (*self.ptr).hash(state);
            }
        }
    }
}

/// Heap-allocated box for recursive `ComponentFieldValue` (e.g. `Some(value)`).
/// Uses raw pointer indirection to break the infinite size.
#[repr(C)]
pub struct ComponentFieldValueBox {
    pub ptr: *mut ComponentFieldValue,
}

impl ComponentFieldValueBox {
    #[must_use]
    pub fn new(v: ComponentFieldValue) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(v)),
        }
    }

    #[must_use]
    pub fn as_ref(&self) -> &ComponentFieldValue {
        unsafe { &*self.ptr }
    }
}

impl Clone for ComponentFieldValueBox {
    fn clone(&self) -> Self {
        Self::new(unsafe { (*self.ptr).clone() })
    }
}

impl Drop for ComponentFieldValueBox {
    fn drop(&mut self) {
        // Take-and-null so a second drop (codegen FFI double-drop of a by-value
        // field, see `ComponentFieldTypeBox`) is a no-op, not a double free.
        let ptr = core::mem::replace(&mut self.ptr, core::ptr::null_mut());
        if !ptr.is_null() {
            unsafe {
                drop(Box::from_raw(ptr));
            }
        }
    }
}

impl fmt::Debug for ComponentFieldValueBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ptr.is_null() {
            write!(f, "ComponentFieldValueBox(null)")
        } else {
            write!(f, "ComponentFieldValueBox({:?})", unsafe { &*self.ptr })
        }
    }
}

impl PartialEq for ComponentFieldValueBox {
    fn eq(&self, other: &Self) -> bool {
        if self.ptr.is_null() && other.ptr.is_null() {
            return true;
        }
        if self.ptr.is_null() || other.ptr.is_null() {
            return false;
        }
        unsafe { *self.ptr == *other.ptr }
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
    /// `StyledDom` slot — field name = slot name
    StyledDom,
    /// Callback with typed signature
    Callback(ComponentCallbackSignature),
    /// `RefAny` data binding with type hint
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

impl ComponentFieldType {
    /// Parse a field type string like "String", "Option<Bool>", "Vec<I32>",
    /// "Callback(fn(LayoutCallbackInfo) -> Dom)", "StructRef(MyStruct)" etc.
    /// Returns `None` if the string cannot be parsed.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::parse_depth(s, 0)
    }

    /// Depth-bounded implementation of [`parse`](Self::parse).
    ///
    /// AUDIT 2026-07-08: `Option<..>` / `Vec<..>` wrappers recurse once per level,
    /// so an attacker string like `"Option<".repeat(100_000)` (with matching `>`)
    /// overflowed the stack. Recursion is capped at [`MAX_TYPE_PARSE_DEPTH`];
    /// beyond it, parsing fails (`None`) instead of crashing.
    fn parse_depth(s: &str, depth: usize) -> Option<Self> {
        if depth > MAX_TYPE_PARSE_DEPTH {
            return None;
        }
        let s = s.trim();
        match s {
            "String" | "string" => return Some(Self::String),
            "Bool" | "bool" => return Some(Self::Bool),
            "I32" | "i32" => return Some(Self::I32),
            "I64" | "i64" => return Some(Self::I64),
            "U32" | "u32" => return Some(Self::U32),
            "U64" | "u64" => return Some(Self::U64),
            "Usize" | "usize" => return Some(Self::Usize),
            "F32" | "f32" => return Some(Self::F32),
            "F64" | "f64" => return Some(Self::F64),
            "ColorU" => return Some(Self::ColorU),
            "CssProperty" => return Some(Self::CssProperty),
            "ImageRef" => return Some(Self::ImageRef),
            "FontRef" => return Some(Self::FontRef),
            "StyledDom" => return Some(Self::StyledDom),
            "RefAny" => return Some(Self::RefAny(AzString::from(""))),
            _ => {}
        }

        // Option<T>
        if let Some(inner) = s.strip_prefix("Option<").and_then(|r| r.strip_suffix('>')) {
            let inner_type = Self::parse_depth(inner, depth + 1)?;
            return Some(Self::OptionType(ComponentFieldTypeBox::new(inner_type)));
        }

        // Vec<T>
        if let Some(inner) = s.strip_prefix("Vec<").and_then(|r| r.strip_suffix('>')) {
            let inner_type = Self::parse_depth(inner, depth + 1)?;
            return Some(Self::VecType(ComponentFieldTypeBox::new(inner_type)));
        }

        // Callback(signature)
        if let Some(sig) = s
            .strip_prefix("Callback(")
            .and_then(|r| r.strip_suffix(')'))
        {
            return Some(Self::Callback(ComponentCallbackSignature {
                return_type: AzString::from(sig),
                args: Vec::new().into(),
            }));
        }

        // RefAny(TypeHint)
        if let Some(hint) = s.strip_prefix("RefAny(").and_then(|r| r.strip_suffix(')')) {
            return Some(Self::RefAny(AzString::from(hint)));
        }

        // EnumRef(Name) — explicit
        if let Some(name) = s.strip_prefix("EnumRef(").and_then(|r| r.strip_suffix(')')) {
            return Some(Self::EnumRef(AzString::from(name)));
        }

        // StructRef(Name) — explicit
        if let Some(name) = s
            .strip_prefix("StructRef(")
            .and_then(|r| r.strip_suffix(')'))
        {
            return Some(Self::StructRef(AzString::from(name)));
        }

        // If starts with uppercase, treat as StructRef
        if s.chars().next().is_some_and(char::is_uppercase) {
            return Some(Self::StructRef(AzString::from(s)));
        }

        None
    }

    /// Format this field type to its canonical string representation.
    /// This is the inverse of `parse`.
    #[must_use]
    pub fn format(&self) -> String {
        match self {
            Self::String => "String".to_string(),
            Self::Bool => "Bool".to_string(),
            Self::I32 => "I32".to_string(),
            Self::I64 => "I64".to_string(),
            Self::U32 => "U32".to_string(),
            Self::U64 => "U64".to_string(),
            Self::Usize => "Usize".to_string(),
            Self::F32 => "F32".to_string(),
            Self::F64 => "F64".to_string(),
            Self::ColorU => "ColorU".to_string(),
            Self::CssProperty => "CssProperty".to_string(),
            Self::ImageRef => "ImageRef".to_string(),
            Self::FontRef => "FontRef".to_string(),
            Self::StyledDom => "StyledDom".to_string(),
            Self::Callback(sig) => format!("Callback({})", sig.return_type.as_str()),
            Self::RefAny(hint) => {
                if hint.as_str().is_empty() {
                    "RefAny".to_string()
                } else {
                    format!("RefAny({})", hint.as_str())
                }
            }
            Self::OptionType(inner) => format!("Option<{}>", inner.as_ref().format()),
            Self::VecType(inner) => format!("Vec<{}>", inner.as_ref().format()),
            Self::StructRef(name) | Self::EnumRef(name) => name.as_str().to_string(),
        }
    }
}

impl fmt::Display for ComponentFieldType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}

/// A single variant in a component enum model.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ComponentEnumVariant {
    /// Variant name, e.g. "Admin", "Editor", "Viewer"
    pub name: AzString,
    /// Human-readable description for this variant
    pub description: AzString,
    /// Optional associated fields for this variant
    pub fields: ComponentDataFieldVec,
}

impl_vec!(
    ComponentEnumVariant,
    ComponentEnumVariantVec,
    ComponentEnumVariantVecDestructor,
    ComponentEnumVariantVecDestructorType,
    ComponentEnumVariantVecSlice,
    OptionComponentEnumVariant
);
impl_option!(
    ComponentEnumVariant,
    OptionComponentEnumVariant,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec_debug!(ComponentEnumVariant, ComponentEnumVariantVec);
impl_vec_partialeq!(ComponentEnumVariant, ComponentEnumVariantVec);
impl_vec_clone!(
    ComponentEnumVariant,
    ComponentEnumVariantVec,
    ComponentEnumVariantVecDestructor
);

/// A named enum model for code generation.
/// Stored in `ComponentLibrary::enum_models`.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ComponentEnumModel {
    /// Enum name, e.g. "`UserRole`"
    pub name: AzString,
    /// Human-readable description
    pub description: AzString,
    /// Variants
    pub variants: ComponentEnumVariantVec,
}

impl_vec!(
    ComponentEnumModel,
    ComponentEnumModelVec,
    ComponentEnumModelVecDestructor,
    ComponentEnumModelVecDestructorType,
    ComponentEnumModelVecSlice,
    OptionComponentEnumModel
);
impl_option!(
    ComponentEnumModel,
    OptionComponentEnumModel,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec_debug!(ComponentEnumModel, ComponentEnumModelVec);
impl_vec_partialeq!(ComponentEnumModel, ComponentEnumModelVec);
impl_vec_clone!(
    ComponentEnumModel,
    ComponentEnumModelVec,
    ComponentEnumModelVecDestructor
);

/// Default value for a component field.
#[derive(Debug, Clone, PartialEq)]
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
    /// `ColorU` default
    ColorU(ColorU),
    /// Default is an instance of another component
    ComponentInstance(ComponentInstanceDefault),
    /// Default callback function pointer name
    CallbackFnPointer(AzString),
    /// JSON string representing a complex default value
    Json(AzString),
}

impl_option!(
    ComponentDefaultValue,
    OptionComponentDefaultValue,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// Default component instance for a `StyledDom` slot.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ComponentFieldOverride {
    /// Field name to override
    pub field_name: AzString,
    /// Value source for this override
    pub source: ComponentFieldValueSource,
}

impl_vec!(
    ComponentFieldOverride,
    ComponentFieldOverrideVec,
    ComponentFieldOverrideVecDestructor,
    ComponentFieldOverrideVecDestructorType,
    ComponentFieldOverrideVecSlice,
    OptionComponentFieldOverride
);
impl_option!(
    ComponentFieldOverride,
    OptionComponentFieldOverride,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);
impl_vec_debug!(ComponentFieldOverride, ComponentFieldOverrideVec);
impl_vec_partialeq!(ComponentFieldOverride, ComponentFieldOverrideVec);
impl_vec_clone!(
    ComponentFieldOverride,
    ComponentFieldOverrideVec,
    ComponentFieldOverrideVecDestructor
);

/// How a field value is sourced at the instance level.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum ComponentFieldValueSource {
    /// Use the component's default value
    Default,
    /// Hardcoded literal value (as string, parsed at runtime)
    Literal(AzString),
    /// Bound to an app state path (e.g. "`app_state.user.name`")
    Binding(AzString),
}
#[allow(variant_size_differences)]
// repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size
// disparity accepted
/// Runtime value for a component field — the "instance" counterpart
/// to `ComponentFieldType` (which is the "class" / type descriptor).
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
#[allow(clippy::large_enum_variant)] // #[repr(C,u8)] FFI enum: boxing a variant changes the C
                                     // ABI/api.json
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
    /// Option<T> with a value
    Some(ComponentFieldValueBox),
    /// Vec of values
    Vec(ComponentFieldValueVec),
    /// `StyledDom` slot content
    StyledDom(StyledDom),
    /// Struct fields, in order
    Struct(ComponentFieldNamedValueVec),
    /// Enum variant
    Enum {
        variant: AzString,
        fields: ComponentFieldNamedValueVec,
    },
    /// Callback function reference (function name as string)
    Callback(AzString),
    /// Opaque reference-counted data
    RefAny(crate::refany::RefAny),
}

/// Named field value: (`field_name`, value) pair.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ComponentFieldNamedValue {
    pub name: AzString,
    pub value: ComponentFieldValue,
}

impl_vec!(
    ComponentFieldNamedValue,
    ComponentFieldNamedValueVec,
    ComponentFieldNamedValueVecDestructor,
    ComponentFieldNamedValueVecDestructorType,
    ComponentFieldNamedValueVecSlice,
    OptionComponentFieldNamedValue
);
impl_option!(
    ComponentFieldNamedValue,
    OptionComponentFieldNamedValue,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec_debug!(ComponentFieldNamedValue, ComponentFieldNamedValueVec);
impl_vec_partialeq!(ComponentFieldNamedValue, ComponentFieldNamedValueVec);
impl_vec_clone!(
    ComponentFieldNamedValue,
    ComponentFieldNamedValueVec,
    ComponentFieldNamedValueVecDestructor
);

impl ComponentFieldNamedValueVec {
    /// Look up a field by name, return a reference to its value.
    #[must_use]
    pub fn get_field(&self, name: &str) -> Option<&ComponentFieldValue> {
        self.as_ref().iter().find_map(|v| {
            if v.name.as_str() == name {
                Some(&v.value)
            } else {
                None
            }
        })
    }

    /// Convenience: get a field as `&str` if it is `ComponentFieldValue::String`.
    #[must_use]
    pub fn get_string(&self, name: &str) -> Option<&AzString> {
        match self.get_field(name) {
            Some(ComponentFieldValue::String(s)) => Some(s),
            _ => None,
        }
    }
}

impl_vec!(
    ComponentFieldValue,
    ComponentFieldValueVec,
    ComponentFieldValueVecDestructor,
    ComponentFieldValueVecDestructorType,
    ComponentFieldValueVecSlice,
    OptionComponentFieldValue
);
impl_option!(
    ComponentFieldValue,
    OptionComponentFieldValue,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec_debug!(ComponentFieldValue, ComponentFieldValueVec);
impl_vec_partialeq!(ComponentFieldValue, ComponentFieldValueVec);
impl_vec_clone!(
    ComponentFieldValue,
    ComponentFieldValueVec,
    ComponentFieldValueVecDestructor
);

/// A field in the component's internal data model.
#[derive(Debug, Clone, PartialEq)]
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

impl_vec!(
    ComponentDataField,
    ComponentDataFieldVec,
    ComponentDataFieldVecDestructor,
    ComponentDataFieldVecDestructorType,
    ComponentDataFieldVecSlice,
    OptionComponentDataField
);
impl_option!(
    ComponentDataField,
    OptionComponentDataField,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec_debug!(ComponentDataField, ComponentDataFieldVec);
impl_vec_partialeq!(ComponentDataField, ComponentDataFieldVec);
impl_vec_clone!(
    ComponentDataField,
    ComponentDataFieldVec,
    ComponentDataFieldVecDestructor
);

/// A named data model (struct definition) for code generation.
///
/// Stored in `ComponentLibrary::data_models`. Components reference these
/// by name in `ComponentDataField::field_type`, enabling nested/structured
/// data models. For example, a `UserCard` component might have a field
/// `user: UserProfile` where `UserProfile` is a `ComponentDataModel`.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ComponentDataModel {
    /// Type name, e.g. "`UserProfile`", "`TodoItem`"
    pub name: AzString,
    /// Human-readable description
    pub description: AzString,
    /// Fields in this struct
    pub fields: ComponentDataFieldVec,
}

impl ComponentDataModel {
    /// Look up a field by name.
    #[must_use]
    pub fn get_field(&self, name: &str) -> Option<&ComponentDataField> {
        self.fields
            .as_ref()
            .iter()
            .find(|f| f.name.as_str() == name)
    }

    /// Look up a field's default value as a string, if it exists and is a String variant.
    #[must_use]
    pub fn get_default_string(&self, name: &str) -> Option<&AzString> {
        self.get_field(name).and_then(|f| match &f.default_value {
            OptionComponentDefaultValue::Some(ComponentDefaultValue::String(s)) => Some(s),
            _ => None,
        })
    }

    /// Clone this data model, overriding the default value for a field by name.
    /// If the field is not found, the data model is returned unchanged.
    #[must_use]
    pub fn with_default(mut self, name: &str, value: ComponentDefaultValue) -> Self {
        let mut fields_vec = core::mem::replace(
            &mut self.fields,
            ComponentDataFieldVec::from_const_slice(&[]),
        )
        .into_library_owned_vec();
        for f in &mut fields_vec {
            if f.name.as_str() == name {
                f.default_value = OptionComponentDefaultValue::Some(value);
                break;
            }
        }
        self.fields = ComponentDataFieldVec::from_vec(fields_vec);
        self
    }
}

impl_vec!(
    ComponentDataModel,
    ComponentDataModelVec,
    ComponentDataModelVecDestructor,
    ComponentDataModelVecDestructorType,
    ComponentDataModelVecSlice,
    OptionComponentDataModel
);
impl_option!(
    ComponentDataModel,
    OptionComponentDataModel,
    copy = false,
    [Debug, Clone]
);
impl_vec_debug!(ComponentDataModel, ComponentDataModelVec);
impl_vec_clone!(
    ComponentDataModel,
    ComponentDataModelVec,
    ComponentDataModelVecDestructor
);
impl_vec_mut!(ComponentDataModel, ComponentDataModelVec);

// ============================================================================
// Serde support for ComponentDataModel (feature-gated)
// ============================================================================

#[cfg(feature = "serde-json")]
mod serde_impl {
    use serde::{ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};

    #[allow(clippy::wildcard_imports)] // serde impl module mirrors the parent surface
    use super::*;

    // --- AzString helpers ---

    fn ser_azstring<S: Serializer>(s: &AzString, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(s.as_str())
    }

    fn de_azstring<'de, D: Deserializer<'de>>(deserializer: D) -> Result<AzString, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(AzString::from(s.as_str()))
    }

    // --- ComponentFieldType ---

    impl Serialize for ComponentFieldType {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_str(&field_type_to_string(self))
        }
    }

    impl<'de> Deserialize<'de> for ComponentFieldType {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let s = String::deserialize(deserializer)?;
            Ok(string_to_field_type(&s))
        }
    }

    fn field_type_to_string(ft: &ComponentFieldType) -> String {
        match ft {
            ComponentFieldType::String => "String".into(),
            ComponentFieldType::Bool => "bool".into(),
            ComponentFieldType::I32 => "i32".into(),
            ComponentFieldType::I64 => "i64".into(),
            ComponentFieldType::U32 => "u32".into(),
            ComponentFieldType::U64 => "u64".into(),
            ComponentFieldType::Usize => "usize".into(),
            ComponentFieldType::F32 => "f32".into(),
            ComponentFieldType::F64 => "f64".into(),
            ComponentFieldType::ColorU => "ColorU".into(),
            ComponentFieldType::CssProperty => "CssProperty".into(),
            ComponentFieldType::ImageRef => "ImageRef".into(),
            ComponentFieldType::FontRef => "FontRef".into(),
            ComponentFieldType::StyledDom => "Dom".into(),
            ComponentFieldType::Callback(sig) => {
                alloc::format!("Callback({})", sig.return_type.as_str())
            }
            ComponentFieldType::RefAny(hint) => alloc::format!("RefAny({})", hint.as_str()),
            ComponentFieldType::OptionType(inner) => {
                alloc::format!("Option<{}>", field_type_to_string(inner.as_ref()))
            }
            ComponentFieldType::VecType(inner) => {
                alloc::format!("Vec<{}>", field_type_to_string(inner.as_ref()))
            }
            ComponentFieldType::StructRef(name) => alloc::format!("struct:{}", name.as_str()),
            ComponentFieldType::EnumRef(name) => alloc::format!("enum:{}", name.as_str()),
        }
    }

    // A 5-arm strip_prefix dispatch ladder. `option_if_let_else` (nursery)
    // wants `map_or_else` here, which would nest five closures inside each
    // other's else-branch — strictly less readable than the ladder.
    #[allow(clippy::option_if_let_else)]
    fn string_to_field_type(s: &str) -> ComponentFieldType {
        match s {
            "String" | "string" => ComponentFieldType::String,
            "bool" | "Bool" => ComponentFieldType::Bool,
            "i32" | "I32" => ComponentFieldType::I32,
            "i64" | "I64" => ComponentFieldType::I64,
            "u32" | "U32" => ComponentFieldType::U32,
            "u64" | "U64" => ComponentFieldType::U64,
            "usize" | "Usize" => ComponentFieldType::Usize,
            "f32" | "F32" => ComponentFieldType::F32,
            "f64" | "F64" => ComponentFieldType::F64,
            "ColorU" | "Color" | "color" => ComponentFieldType::ColorU,
            "CssProperty" => ComponentFieldType::CssProperty,
            "ImageRef" | "Image" => ComponentFieldType::ImageRef,
            "FontRef" | "Font" => ComponentFieldType::FontRef,
            "Dom" | "StyledDom" | "Children" => ComponentFieldType::StyledDom,
            other => {
                if let Some(inner) = other
                    .strip_prefix("Option<")
                    .and_then(|s| s.strip_suffix('>'))
                {
                    ComponentFieldType::OptionType(ComponentFieldTypeBox::new(
                        string_to_field_type(inner),
                    ))
                } else if let Some(inner) =
                    other.strip_prefix("Vec<").and_then(|s| s.strip_suffix('>'))
                {
                    ComponentFieldType::VecType(ComponentFieldTypeBox::new(string_to_field_type(
                        inner,
                    )))
                } else if let Some(name) = other.strip_prefix("struct:") {
                    ComponentFieldType::StructRef(AzString::from(name))
                } else if let Some(name) = other.strip_prefix("enum:") {
                    ComponentFieldType::EnumRef(AzString::from(name))
                } else if other.starts_with("Callback") {
                    let ret = other
                        .strip_prefix("Callback(")
                        .and_then(|s| s.strip_suffix(')'))
                        .unwrap_or("()");
                    ComponentFieldType::Callback(ComponentCallbackSignature {
                        return_type: AzString::from(ret),
                        args: ComponentCallbackArgVec::from_const_slice(&[]),
                    })
                } else if other.starts_with("RefAny") {
                    let hint = other
                        .strip_prefix("RefAny(")
                        .and_then(|s| s.strip_suffix(')'))
                        .unwrap_or("");
                    ComponentFieldType::RefAny(AzString::from(hint))
                } else {
                    ComponentFieldType::String // fallback
                }
            }
        }
    }

    // --- ComponentDefaultValue ---

    impl Serialize for ComponentDefaultValue {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeMap;
            match self {
                Self::None => serializer.serialize_none(),
                Self::String(s) => serializer.serialize_str(s.as_str()),
                Self::Bool(b) => serializer.serialize_bool(*b),
                Self::I32(v) => serializer.serialize_i32(*v),
                Self::I64(v) => serializer.serialize_i64(*v),
                Self::U32(v) => serializer.serialize_u32(*v),
                Self::U64(v) => serializer.serialize_u64(*v),
                Self::Usize(v) => serializer.serialize_u64(*v as u64),
                Self::F32(v) => serializer.serialize_f32(*v),
                Self::F64(v) => serializer.serialize_f64(*v),
                Self::ColorU(c) => serializer.serialize_str(&alloc::format!(
                    "#{:02x}{:02x}{:02x}{:02x}",
                    c.r,
                    c.g,
                    c.b,
                    c.a
                )),
                Self::ComponentInstance(ci) => {
                    let mut map = serializer.serialize_map(Some(2))?;
                    map.serialize_entry("library", ci.library.as_str())?;
                    map.serialize_entry("component", ci.component.as_str())?;
                    map.end()
                }
                Self::CallbackFnPointer(name) => serializer.serialize_str(name.as_str()),
                Self::Json(json_str) => {
                    // Serialize raw JSON string as-is by parsing and re-emitting
                    match serde_json::from_str::<serde_json::Value>(json_str.as_str()) {
                        Ok(v) => v.serialize(serializer),
                        Err(_) => serializer.serialize_str(json_str.as_str()),
                    }
                }
            }
        }
    }

    impl<'de> Deserialize<'de> for ComponentDefaultValue {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let val = serde_json::Value::deserialize(deserializer)?;
            // NOTE: `Value::Null` is deliberately NOT its own arm — it maps to
            // `Self::None`, which is exactly what the catch-all below produces.
            Ok(match val {
                serde_json::Value::Bool(b) => Self::Bool(b),
                serde_json::Value::Number(n) => n.as_i64().map_or_else(
                    || n.as_f64().map_or(Self::None, Self::F64),
                    |i| i32::try_from(i).map_or(Self::I64(i), Self::I32),
                ),
                serde_json::Value::String(s) => Self::String(AzString::from(s.as_str())),
                _ => Self::None,
            })
        }
    }

    // --- OptionComponentDefaultValue ---

    impl Serialize for OptionComponentDefaultValue {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            match self {
                Self::Some(v) => v.serialize(serializer),
                Self::None => serializer.serialize_none(),
            }
        }
    }

    impl<'de> Deserialize<'de> for OptionComponentDefaultValue {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let val = Option::<ComponentDefaultValue>::deserialize(deserializer)?;
            Ok(val.map_or(Self::None, Self::Some))
        }
    }

    // --- ComponentDataField ---

    impl Serialize for ComponentDataField {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut s = serializer.serialize_struct("ComponentDataField", 5)?;
            s.serialize_field("name", self.name.as_str())?;
            s.serialize_field("type", &self.field_type)?;
            s.serialize_field("default", &self.default_value)?;
            s.serialize_field("required", &self.required)?;
            s.serialize_field("description", self.description.as_str())?;
            s.end()
        }
    }

    impl<'de> Deserialize<'de> for ComponentDataField {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            struct Helper {
                name: String,
                #[serde(rename = "type", default = "default_type")]
                field_type: ComponentFieldType,
                #[serde(default)]
                default: OptionComponentDefaultValue,
                #[serde(default)]
                required: bool,
                #[serde(default)]
                description: String,
            }
            const fn default_type() -> ComponentFieldType {
                ComponentFieldType::String
            }

            let h = Helper::deserialize(deserializer)?;
            Ok(Self {
                name: AzString::from(h.name.as_str()),
                field_type: h.field_type,
                default_value: h.default,
                required: h.required,
                description: AzString::from(h.description.as_str()),
            })
        }
    }

    // --- ComponentDataModel ---

    impl Serialize for ComponentDataModel {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut s = serializer.serialize_struct("ComponentDataModel", 3)?;
            s.serialize_field("name", self.name.as_str())?;
            s.serialize_field("description", self.description.as_str())?;
            let fields: Vec<&ComponentDataField> = self.fields.as_ref().iter().collect();
            s.serialize_field("fields", &fields)?;
            s.end()
        }
    }

    impl<'de> Deserialize<'de> for ComponentDataModel {
        /// A data model is a JSON **object**. This deliberately drives the
        /// deserializer with `deserialize_map` instead of `deserialize_struct`:
        /// the struct hint makes serde accept a *sequence* as well (the
        /// positional encoding used by compact formats), so `from_json("[]")`
        /// used to succeed and hand back a nameless, field-less model instead of
        /// reporting that the input is not a data model at all. Every key stays
        /// optional, so `{}` still deserializes to the empty model.
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            use serde::de::{IgnoredAny, MapAccess, Visitor};

            struct ModelVisitor;

            impl<'de> Visitor<'de> for ModelVisitor {
                type Value = ComponentDataModel;

                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a data model object with `name`, `description` and `fields`")
                }

                fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                    let mut name: Option<String> = None;
                    let mut description: Option<String> = None;
                    let mut fields: Option<Vec<ComponentDataField>> = None;

                    while let Some(key) = map.next_key::<String>()? {
                        match key.as_str() {
                            "name" => name = Some(map.next_value()?),
                            "description" => description = Some(map.next_value()?),
                            "fields" => fields = Some(map.next_value()?),
                            // Unknown keys are ignored (forward compatibility),
                            // but their values must still be consumed.
                            _ => {
                                map.next_value::<IgnoredAny>()?;
                            }
                        }
                    }

                    Ok(ComponentDataModel {
                        name: AzString::from(name.unwrap_or_default().as_str()),
                        description: AzString::from(description.unwrap_or_default().as_str()),
                        fields: ComponentDataFieldVec::from_vec(fields.unwrap_or_default()),
                    })
                }
            }

            deserializer.deserialize_map(ModelVisitor)
        }
    }
}

// NOTE: no `pub use serde_impl::*` — the module holds only private helpers and
// trait impls, and trait impls are in scope crate-wide (and for downstream
// users) regardless of the defining module's visibility.

#[cfg(feature = "serde-json")]
impl ComponentDataModel {
    /// Serialize this data model to a JSON string.
    ///
    /// # Errors
    ///
    /// Returns the serializer's error message if the model cannot be
    /// represented as JSON.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| alloc::format!("{e}"))
    }

    /// Deserialize a data model from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns the parser's error message if `json` is malformed or does not
    /// match the data-model shape.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| alloc::format!("{e}"))
    }
}

/// Source of a component definition — determines whether it can be exported
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum ComponentSource {
    /// Built into the DLL (HTML elements). Never exported.
    Builtin,
    /// Compiled Rust widget (Button, `TextInput`, etc.). Never exported.
    Compiled,
    /// Defined via JSON/XML at runtime. Can be exported.
    #[default]
    UserDefined,
}

impl ComponentSource {
    #[must_use]
    pub fn create() -> Self {
        Self::default()
    }
}

/// How generated code builds an instance of a component: the language-NEUTRAL
/// half of a [`ComponentDef`].
///
/// The code generator (`azul_core::codegen`, the `codegen` feature) turns it
/// into the IR every binding language's printer prints. It replaced the
/// per-language string hook `compile_fn`.
///
/// Variant 0 is [`ComponentCodegen::RenderFunction`], so a zero-initialised C
/// struct is a component that code calls through its render function.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum ComponentCodegen {
    /// A call of the component's own render function,
    /// `render_<name>(<value fields>)`: the code export defines it once,
    /// from the component's template (a component made in `AzBuilder`) or
    /// else from what it renders. User-defined and registered components.
    RenderFunction,
    /// An HTML element of the builtin library: the most specific
    /// `Dom::create_<tag>(..)` its attributes and text pick.
    Element,
    /// A constructor in api.json vocabulary (a widget):
    /// `<class>::<constructor>(<args>)`, then `.<setter>(<field>)` for each
    /// field the instance sets, then `.<finish>()`.
    Call(ComponentCallCodegen),
}

impl ComponentCodegen {
    /// [`ComponentCodegen::RenderFunction`].
    #[must_use]
    pub const fn render_function() -> Self {
        Self::RenderFunction
    }

    /// [`ComponentCodegen::Element`].
    #[must_use]
    pub const fn element() -> Self {
        Self::Element
    }

    /// [`ComponentCodegen::Call`].
    #[must_use]
    pub const fn call(call: ComponentCallCodegen) -> Self {
        Self::Call(call)
    }
}

/// A widget's constructor in api.json vocabulary, for
/// [`ComponentCodegen::Call`].
///
/// `Button::create(label).dom()` is
/// `{ class: "Button", constructor: "create", args: ["label"], setters: [], finish: "dom" }`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ComponentCallCodegen {
    /// The api.json class (`Button`).
    pub class: AzString,
    /// Its constructor (`create`).
    pub constructor: AzString,
    /// The data-model fields passed to the constructor, in order.
    pub args: StringVec,
    /// `key` = a data-model field, `value` = the builder method that sets it
    /// (`with_button_type`), applied when the instance sets that field.
    pub setters: StringPairVec,
    /// The method that turns the widget into a `Dom` (`dom`), or empty when
    /// the constructor returns one.
    pub finish: AzString,
}

impl ComponentCallCodegen {
    /// A constructor call with no setters.
    #[must_use]
    pub fn create(class: AzString, constructor: AzString, args: StringVec, finish: AzString) -> Self {
        Self {
            class,
            constructor,
            args,
            setters: StringPairVec::from_const_slice(&[]),
            finish,
        }
    }
}

impl_result!(
    StyledDom,
    RenderDomError,
    ResultStyledDomRenderDomError,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// Render function type: takes component definition + data model (with current values
/// in `default_value` fields) + component map for recursive sub-component instantiation,
/// returns `StyledDom`.
///
/// The `data` parameter is typically `def.data_model` cloned and with caller-provided
/// values substituted into the `default_value` fields.
pub type ComponentRenderFn =
    fn(&ComponentDef, &ComponentDataModel, &ComponentMap) -> ResultStyledDomRenderDomError;

/// Raw function pointer type that returns a single `ComponentDef` when called.
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
    /// For FFI: stores the foreign callable (e.g., `PyFunction`).
    /// Native Rust/C code sets this to None.
    pub ctx: crate::refany::OptionRefAny,
}

impl_callback!(RegisterComponentFn, RegisterComponentFnType);

/// Raw function pointer type that returns a complete `ComponentLibrary` when called.
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
    /// For FFI: stores the foreign callable (e.g., `PyFunction`).
    /// Native Rust/C code sets this to None.
    pub ctx: crate::refany::OptionRefAny,
}

impl_callback!(RegisterComponentLibraryFn, RegisterComponentLibraryFnType);

// Host-invoker plumbing (see crate::host_invoker): the callback takes no
// arguments at all, so the thunk reads the context from the invocation slot.
// Without a host to ask, the library is empty.
crate::impl_managed_callback! {
    wrapper:        RegisterComponentLibraryFn,
    ctx_field:      ctx,
    args:           [],
    return_ty:      ComponentLibrary,
    default_ret:    <ComponentLibrary as crate::host_invoker::HostOut>::unwritten(),
    invoker_static: REGISTER_COMPONENT_LIBRARY_FN_INVOKER,
    invoker_ty:     AzRegisterComponentLibraryFnInvoker,
    thunk_fn:       az_register_component_library_fn_thunk,
    setter_fn:      AzApp_setRegisterComponentLibraryFnInvoker,
    from_handle_fn: AzRegisterComponentLibraryFn_createFromHostHandle,
    from_handle_byref_fn: AzRegisterComponentLibraryFn_createFromHostHandleByref,
}

impl crate::host_invoker::HostOut for ComponentLibrary {
    /// An empty library: `const` empty strings and vectors own no memory.
    fn unwritten() -> Self {
        Self {
            name: AzString::from_const_str(""),
            version: AzString::from_const_str(""),
            description: AzString::from_const_str(""),
            components: ComponentDefVec::from_const_slice(&[]),
            exportable: false,
            modifiable: false,
            data_models: ComponentDataModelVec::from_const_slice(&[]),
            enum_models: ComponentEnumModelVec::from_const_slice(&[]),
        }
    }
}

/// A component definition — the "class" / "template" of a component.
/// Can come from Rust builtins, compiled widgets, JSON, or user creation in debugger.
#[derive(Clone)]
#[repr(C)]
pub struct ComponentDef {
    /// Collection + name, e.g. builtin:div, shadcn:avatar
    pub id: ComponentId,
    /// Human-readable display name, e.g. "Link" for builtin:a, "Avatar" for shadcn:avatar
    pub display_name: AzString,
    /// Markdown documentation for the component
    pub description: AzString,
    /// The component's CSS
    pub css: AzString,
    /// Where this component was defined (determines exportability)
    pub source: ComponentSource,
    /// Unified data model: all value fields, callback slots, and child slots
    /// in a single named struct. Code gen uses `data_model.name` as the
    /// input struct type name (e.g. "`ButtonData`").
    /// The `default_value` on each field doubles as the "current value" for
    /// preview rendering — callers override defaults before calling `render_fn`.
    pub data_model: ComponentDataModel,
    /// Render to live DOM
    pub render_fn: ComponentRenderFn,
    /// How generated code builds an instance (language-neutral: every code
    /// generator prints it).
    pub codegen: ComponentCodegen,
    /// Source code for `render_fn` (user-defined components only)
    pub render_fn_source: OptionString,
}

impl fmt::Debug for ComponentDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComponentDef")
            .field("id", &self.id)
            .field("display_name", &self.display_name)
            .field("source", &self.source)
            .field("data_model", &self.data_model.name)
            .finish_non_exhaustive()
    }
}

impl_vec!(
    ComponentDef,
    ComponentDefVec,
    ComponentDefVecDestructor,
    ComponentDefVecDestructorType,
    ComponentDefVecSlice,
    OptionComponentDef
);
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

impl_vec!(
    ComponentLibrary,
    ComponentLibraryVec,
    ComponentLibraryVecDestructor,
    ComponentLibraryVecDestructorType,
    ComponentLibraryVecSlice,
    OptionComponentLibrary
);
impl_option!(
    ComponentLibrary,
    OptionComponentLibrary,
    copy = false,
    [Debug, Clone]
);
impl_vec_debug!(ComponentLibrary, ComponentLibraryVec);
impl_vec_clone!(
    ComponentLibrary,
    ComponentLibraryVec,
    ComponentLibraryVecDestructor
);
impl_vec_mut!(ComponentLibrary, ComponentLibraryVec);

/// The component map — holds libraries with namespaced components.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ComponentMap {
    /// Libraries indexed by name. "builtin" is always present.
    pub libraries: ComponentLibraryVec,
}

impl ComponentMap {
    /// Qualified lookup: "shadcn:avatar" -> finds library "shadcn", component "avatar"
    #[must_use]
    pub fn get(&self, collection: &str, name: &str) -> Option<&ComponentDef> {
        self.libraries
            .iter()
            .find(|lib| lib.name.as_str() == collection)
            .and_then(|lib| lib.components.iter().find(|c| c.id.name.as_str() == name))
    }

    /// Unqualified lookup: "div" -> searches ONLY the "builtin" library.
    #[must_use]
    pub fn get_unqualified(&self, name: &str) -> Option<&ComponentDef> {
        self.get("builtin", name)
    }

    /// Parse a "collection:name" string into a lookup
    #[must_use]
    pub fn get_by_qualified_name(&self, qualified: &str) -> Option<&ComponentDef> {
        if let Some((collection, name)) = qualified.split_once(':') {
            self.get(collection, name)
        } else {
            self.get_unqualified(qualified)
        }
    }

    /// Get all libraries that can be exported (user-defined only)
    #[must_use]
    pub fn get_exportable_libraries(&self) -> Vec<&ComponentLibrary> {
        self.libraries.iter().filter(|lib| lib.exportable).collect()
    }

    /// Get all component definitions across all libraries
    #[must_use]
    pub fn all_components(&self) -> Vec<&ComponentDef> {
        self.libraries
            .iter()
            .flat_map(|lib| lib.components.iter())
            .collect()
    }
}

// ============================================================================
// Builtin component bridge — wraps existing render/compile into ComponentDef
// ============================================================================

/// Single source of truth mapping HTML/SVG tag names to node variants.
///
/// Each `"tag" => Variant` entry expands to **both** a `NodeType::Variant` arm in
/// [`tag_to_node_type`] and a `NodeTypeTag::Variant` arm in [`tag_to_node_type_tag`],
/// so the two lookups can never drift apart. Tags whose two enums diverge —
/// `img`, `image`, `icon` — are handled as explicit special cases inside each
/// generated function and are intentionally absent from this table.
macro_rules! html_tag_node_types {
    ($($tag:literal => $variant:ident),* $(,)?) => {
        /// Map a builtin tag name to its corresponding `NodeType`.
        /// Falls back to `NodeType::Div` for unknown tags.
        #[must_use] pub fn tag_to_node_type(tag: &str) -> NodeType {
            match tag {
                // `<img>` becomes a replaced `NodeType::Image`. The `src` attribute is not
                // available here, so a placeholder `NullImage` (0x0, empty tag) is created;
                // `xml_node_to_dom_fast` overrides it with a `NullImage` whose `tag` carries
                // the `src` bytes so a renderer (e.g. printpdf) can resolve the actual image.
                "img" => NodeType::Image(azul_css::css::BoxOrStatic::heap(
                    crate::resources::ImageRef::null_image(
                        0,
                        0,
                        crate::resources::RawImageFormat::RGBA8,
                        alloc::vec::Vec::new(),
                    ),
                )),
                // `<icon>content_copy</icon>` becomes an un-named `NodeType::Icon`;
                // the icon SPEC is its text content, consumed by the icon
                // resolution pass (`resolve_icons_in_styled_dom`) against the
                // registered icon packs — exactly like a ligature icon font
                // turns glyph text into an icon. The builders stay generic.
                "icon" => NodeType::Icon(azul_css::css::BoxOrStatic::heap(
                    azul_css::AzString::from_const_str(""),
                )),
                // `<transient-window>` starts CLOSED with every default; the
                // parser applies `open=` / `anchor=` / `dismiss=` / `size=` /
                // `tearoff=` onto this config afterwards (see
                // `apply_transient_window_attrs`). Carrying the config inline is
                // what lets a closed popup cost nothing.
                "transient-window" => NodeType::TransientWindow(
                    crate::transient::TransientWindowConfig::closed(),
                ),
                $($tag => NodeType::$variant,)*
                // An element of a foreign vocabulary (Word's `<o:p>`,
                // Outlook's `<st1:place>`) is HTML's unknown element: inline.
                t if is_foreign_element(t) => NodeType::Span,
                _ => NodeType::Div,
            }
        }

        /// Map a tag name to its CSS `NodeTypeTag` for CSS matching in the compile pipeline.
        /// Falls back to `NodeTypeTag::Div` for unknown tags.
        pub(crate) fn tag_to_node_type_tag(tag: &str) -> NodeTypeTag {
            match tag {
                // `img`/`image`/`icon` have no 1:1 `NodeType` equivalent (see
                // `tag_to_node_type`), so they map to dedicated `NodeTypeTag` variants.
                "img" | "image" => NodeTypeTag::Img,
                "icon" => NodeTypeTag::Icon,
                "transient-window" => NodeTypeTag::TransientWindow,
                $($tag => NodeTypeTag::$variant,)*
                t if is_foreign_element(t) => NodeTypeTag::Span,
                _ => NodeTypeTag::Div,
            }
        }
    };
}

html_tag_node_types! {
    // Document structure
    "html" => Html,
    "head" => Head,
    "title" => Title,
    "body" => Body,
    // Block-level
    "div" => Div,
    "header" => Header,
    "footer" => Footer,
    "section" => Section,
    "article" => Article,
    "aside" => Aside,
    "nav" => Nav,
    "main" => Main,
    "figure" => Figure,
    "figcaption" => FigCaption,
    "address" => Address,
    "details" => Details,
    "summary" => Summary,
    "dialog" => Dialog,
    // Headings
    "h1" => H1,
    "h2" => H2,
    "h3" => H3,
    "h4" => H4,
    "h5" => H5,
    "h6" => H6,
    // Text content
    "p" => P,
    "span" => Span,
    "pre" => Pre,
    "code" => Code,
    "blockquote" => BlockQuote,
    "br" => Br,
    "hr" => Hr,
    "pagebreak" => PageBreak,
    // Lists
    "ul" => Ul,
    "ol" => Ol,
    "li" => Li,
    "dl" => Dl,
    "dt" => Dt,
    "dd" => Dd,
    "menu" => Menu,
    "menuitem" => MenuItem,
    "dir" => Dir,
    // Tables
    "table" => Table,
    "caption" => Caption,
    "thead" => THead,
    "tbody" => TBody,
    "tfoot" => TFoot,
    "tr" => Tr,
    "th" => Th,
    "td" => Td,
    "colgroup" => ColGroup,
    "col" => Col,
    // Forms
    "form" => Form,
    "fieldset" => FieldSet,
    "legend" => Legend,
    "label" => Label,
    "input" => Input,
    "button" => Button,
    "select" => Select,
    "optgroup" => OptGroup,
    "option" => SelectOption,
    "textarea" => TextArea,
    "output" => Output,
    "progress" => Progress,
    "meter" => Meter,
    "datalist" => DataList,
    // Inline
    "a" => A,
    "strong" => Strong,
    "em" => Em,
    "b" => B,
    "i" => I,
    "u" => U,
    "s" => S,
    "small" => Small,
    "mark" => Mark,
    "del" => Del,
    "ins" => Ins,
    "samp" => Samp,
    "kbd" => Kbd,
    "var" => Var,
    "cite" => Cite,
    "dfn" => Dfn,
    "abbr" => Abbr,
    "acronym" => Acronym,
    "q" => Q,
    "time" => Time,
    "sub" => Sub,
    "sup" => Sup,
    "big" => Big,
    // Legacy presentational tags mail still writes (HTML obsolete features,
    // 16.2): read as the element whose rendering they share, so they stay
    // INLINE instead of becoming the unknown tag's block. `<strike>` is
    // `<s>`, `<tt>` is `<code>` (monospace), `<font>` and `<nobr>` are
    // spans (their attributes are not read).
    "strike" => S,
    "tt" => Code,
    "font" => Span,
    "nobr" => Span,
    "bdo" => Bdo,
    "bdi" => Bdi,
    "wbr" => Wbr,
    "ruby" => Ruby,
    "rt" => Rt,
    "rtc" => Rtc,
    "rp" => Rp,
    "data" => Data,
    // Embedded content (`img` is a special case in the generated fns)
    "canvas" => Canvas,
    "object" => Object,
    "param" => Param,
    "embed" => Embed,
    "audio" => Audio,
    "video" => Video,
    "source" => Source,
    "track" => Track,
    "map" => Map,
    "area" => Area,
    // SVG elements
    "svg" => Svg,
    "g" => SvgG,
    "defs" => SvgDefs,
    "symbol" => SvgSymbol,
    "use" => SvgUse,
    "switch" => SvgSwitch,
    "path" => SvgPath,
    "circle" => SvgCircle,
    "rect" => SvgRect,
    "ellipse" => SvgEllipse,
    "line" => SvgLine,
    "polygon" => SvgPolygon,
    "polyline" => SvgPolyline,
    "tspan" => SvgTspan,
    "textpath" => SvgTextPath,
    "lineargradient" => SvgLinearGradient,
    "radialgradient" => SvgRadialGradient,
    "stop" => SvgStop,
    "pattern" => SvgPattern,
    "clippath" => SvgClipPathElement,
    "mask" => SvgMask,
    "filter" => SvgFilter,
    "feblend" => SvgFeBlend,
    "fecolormatrix" => SvgFeColorMatrix,
    "fecomponenttransfer" => SvgFeComponentTransfer,
    "fecomposite" => SvgFeComposite,
    "feconvolvematrix" => SvgFeConvolveMatrix,
    "fediffuselighting" => SvgFeDiffuseLighting,
    "fedisplacementmap" => SvgFeDisplacementMap,
    "fedistantlight" => SvgFeDistantLight,
    "fedropshadow" => SvgFeDropShadow,
    "feflood" => SvgFeFlood,
    "fefuncr" => SvgFeFuncR,
    "fefuncg" => SvgFeFuncG,
    "fefuncb" => SvgFeFuncB,
    "fefunca" => SvgFeFuncA,
    "fegaussianblur" => SvgFeGaussianBlur,
    "feimage" => SvgFeImage,
    "femerge" => SvgFeMerge,
    "femergenode" => SvgFeMergeNode,
    "femorphology" => SvgFeMorphology,
    "feoffset" => SvgFeOffset,
    "fepointlight" => SvgFePointLight,
    "fespecularlighting" => SvgFeSpecularLighting,
    "fespotlight" => SvgFeSpotLight,
    "fetile" => SvgFeTile,
    "feturbulence" => SvgFeTurbulence,
    "foreignobject" => SvgForeignObject,
    "desc" => SvgDesc,
    "view" => SvgView,
    "animate" => SvgAnimate,
    "animatemotion" => SvgAnimateMotion,
    "animatetransform" => SvgAnimateTransform,
    "set" => SvgSet,
    "mpath" => SvgMpath,
    // Metadata
    "meta" => Meta,
    "link" => Link,
    "script" => Script,
    "style" => Style,
    "base" => Base,
}

/// Default render function for builtin HTML elements: the element with its
/// text - what a drop inserts ([`builtin_dom`]; the preview adds its example).
fn builtin_render_fn(
    def: &ComponentDef,
    data: &ComponentDataModel,
    _component_map: &ComponentMap,
) -> ResultStyledDomRenderDomError {
    let mut dom = builtin_dom(def.id.name.as_str(), data, false);
    let r: Result<StyledDom, RenderDomError> = Ok(StyledDom::create(&mut dom, Css::empty()));
    r.into()
}

/// Pushes a `<div>` containing `"field_name: value"` text into the children list.
fn push_scalar_field(children: &mut Vec<Dom>, field_name: &str, value: &dyn fmt::Display) {
    use crate::dom::{Dom, NodeType};
    let text = alloc::format!("{field_name}: {value}");
    children.push(
        Dom::create_node(NodeType::Div).with_children(
            alloc::vec![Dom::create_text_do_not_use_without_block_level_wrapper(
                text
            )]
            .into(),
        ),
    );
}

/// Default render function for user-defined (JSON-imported) components.
///
/// Interprets the `ComponentDef` structure generically:
/// 1. Creates a wrapper `<div>` with the component's CSS class
/// 2. For each data field, renders content based on type:
///    - String fields → text node with current value
///    - Bool fields → conditional display
///    - `StyledDom` fields → embeds the child DOM subtree
///    - StructRef/EnumRef → recursively renders sub-components if found in `ComponentMap`
///    - Other scalar fields → text display of the value
/// 3. Applies the component's scoped CSS
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
#[must_use]
pub fn user_defined_render_fn(
    def: &ComponentDef,
    data: &ComponentDataModel,
    component_map: &ComponentMap,
) -> ResultStyledDomRenderDomError {
    use azul_css::css::Css;

    use crate::dom::{Dom, NodeType};

    let mut children: Vec<Dom> = Vec::new();

    for field in data.fields.as_ref() {
        let field_name = field.name.as_str();

        // Get the current value from default_value
        match &field.default_value {
            OptionComponentDefaultValue::None => {
                // Required field with no value — skip in preview
            }
            OptionComponentDefaultValue::Some(default_val) => {
                match default_val {
                    ComponentDefaultValue::String(s) => {
                        let text = s.as_str().trim();
                        if !text.is_empty() {
                            let label_dom = Dom::create_node(NodeType::Div).with_children(
                                alloc::vec![
                                    Dom::create_text_do_not_use_without_block_level_wrapper(
                                        text.to_string()
                                    )
                                ]
                                .into(),
                            );
                            children.push(label_dom);
                        }
                    }
                    ComponentDefaultValue::Bool(v) => {
                        push_scalar_field(&mut children, field_name, v);
                    }
                    ComponentDefaultValue::I32(v) => {
                        push_scalar_field(&mut children, field_name, v);
                    }
                    ComponentDefaultValue::I64(v) => {
                        push_scalar_field(&mut children, field_name, v);
                    }
                    ComponentDefaultValue::U32(v) => {
                        push_scalar_field(&mut children, field_name, v);
                    }
                    ComponentDefaultValue::U64(v) => {
                        push_scalar_field(&mut children, field_name, v);
                    }
                    ComponentDefaultValue::Usize(v) => {
                        push_scalar_field(&mut children, field_name, v);
                    }
                    ComponentDefaultValue::F32(v) => {
                        push_scalar_field(&mut children, field_name, v);
                    }
                    ComponentDefaultValue::F64(v) => {
                        push_scalar_field(&mut children, field_name, v);
                    }
                    ComponentDefaultValue::ColorU(c) => {
                        let text = alloc::format!(
                            "{}: #{:02x}{:02x}{:02x}{:02x}",
                            field_name,
                            c.r,
                            c.g,
                            c.b,
                            c.a
                        );
                        children.push(
                            Dom::create_node(NodeType::Div).with_children(
                                alloc::vec![
                                    Dom::create_text_do_not_use_without_block_level_wrapper(text)
                                ]
                                .into(),
                            ),
                        );
                    }
                    ComponentDefaultValue::ComponentInstance(ci) => {
                        // Recursively instantiate sub-component from ComponentMap
                        if let Some(sub_comp) =
                            component_map.get(ci.library.as_str(), ci.component.as_str())
                        {
                            let sub_data = sub_comp.data_model.clone();
                            match (sub_comp.render_fn)(sub_comp, &sub_data, component_map) {
                                ResultStyledDomRenderDomError::Ok(_styled_dom) => {
                                    // Sub-component rendered successfully — add a placeholder
                                    // (StyledDom cannot be directly converted back to Dom)
                                    let text = alloc::format!(
                                        "[{}:{}]",
                                        ci.library.as_str(),
                                        ci.component.as_str()
                                    );
                                    children.push(
                                        Dom::create_node(NodeType::Div).with_children(
                                            alloc::vec![Dom::create_text_do_not_use_without_block_level_wrapper(text)].into(),
                                        ),
                                    );
                                }
                                ResultStyledDomRenderDomError::Err(_) => {
                                    // On error, show a placeholder
                                    let text = alloc::format!(
                                        "[Error rendering {}:{}]",
                                        ci.library.as_str(),
                                        ci.component.as_str()
                                    );
                                    children.push(
                                        Dom::create_node(NodeType::Div).with_children(
                                            alloc::vec![Dom::create_text_do_not_use_without_block_level_wrapper(text)].into(),
                                        ),
                                    );
                                }
                            }
                        } else {
                            let text = alloc::format!(
                                "[Unknown component {}:{}]",
                                ci.library.as_str(),
                                ci.component.as_str()
                            );
                            children.push(
                                Dom::create_node(NodeType::Div).with_children(
                                    alloc::vec![
                                        Dom::create_text_do_not_use_without_block_level_wrapper(
                                            text
                                        )
                                    ]
                                    .into(),
                                ),
                            );
                        }
                    }
                    ComponentDefaultValue::CallbackFnPointer(name) => {
                        // Callbacks are not rendered, just acknowledged
                        let text = alloc::format!("{}: fn({})", field_name, name.as_str());
                        children.push(
                            Dom::create_node(NodeType::Div).with_children(
                                alloc::vec![
                                    Dom::create_text_do_not_use_without_block_level_wrapper(text)
                                ]
                                .into(),
                            ),
                        );
                    }
                    ComponentDefaultValue::Json(json_str) => {
                        let text = alloc::format!("{}: {}", field_name, json_str.as_str());
                        children.push(
                            Dom::create_node(NodeType::Div).with_children(
                                alloc::vec![
                                    Dom::create_text_do_not_use_without_block_level_wrapper(text)
                                ]
                                .into(),
                            ),
                        );
                    }
                    ComponentDefaultValue::None => {
                        // No default, skip
                    }
                }
            }
        }
    }

    let mut wrapper = Dom::create_node(NodeType::Div);
    if !children.is_empty() {
        wrapper = wrapper.with_children(children.into());
    }

    // Apply component CSS
    let css = if def.css.as_str().is_empty() {
        Css::empty()
    } else {
        Css::from_string(def.css.clone())
    };

    let r: Result<StyledDom, RenderDomError> = Ok(StyledDom::create(&mut wrapper, css));
    r.into()
}

/// Create a `ComponentDef` for a builtin HTML element.
///
/// # Arguments
/// * `tag` - HTML tag name (e.g. "button", "div")
/// * `display_name` - Human-readable name (e.g. "Button", "Div")
/// * `default_text` - Default text content for the preview, or `None` if the element has no text.
///   Pass `Some("Button text")` for `<button>`, `Some("")` for text elements like `<span>` that
///   accept text but have no meaningful default.
/// * `css` - Component-level CSS string. For most builtin elements this is `""` because styling
///   comes from `ua_css.rs` and the `SystemStyle`. Components that need extra styling (e.g. a
///   future high-level button widget) can pass CSS here.
fn builtin_component_def(
    tag: &str,
    display_name: &str,
    default_text: Option<&str>,
    css: &str,
) -> ComponentDef {
    let mut fields = builtin_data_model(tag);
    // If a default_text is provided, this element accepts text content
    if let Some(text) = default_text {
        fields.push(data_field(
            "text",
            ComponentFieldType::String,
            Some(ComponentDefaultValue::String(AzString::from(text))),
            "Text content of the element",
        ));
    }
    let model_name = format!("{display_name}Data");
    ComponentDef {
        id: ComponentId::builtin(tag),
        display_name: AzString::from(display_name),
        description: AzString::from(format!("HTML <{tag}> element").as_str()),
        css: AzString::from(css),
        source: ComponentSource::Builtin,
        data_model: ComponentDataModel {
            name: AzString::from(model_name.as_str()),
            description: AzString::from(format!("Data model for <{tag}>").as_str()),
            fields: fields.into(),
        },
        render_fn: builtin_render_fn,
        codegen: ComponentCodegen::Element,
        render_fn_source: None.into(),
    }
}

/// Helper to create a `ComponentDataField` with a rich type
fn data_field(
    name: &str,
    ft: ComponentFieldType,
    default: Option<ComponentDefaultValue>,
    description: &str,
) -> ComponentDataField {
    let required = default.is_none();
    ComponentDataField {
        name: AzString::from(name),
        field_type: ft,
        default_value: default.map_or_else(
            || OptionComponentDefaultValue::None,
            OptionComponentDefaultValue::Some,
        ),
        required,
        description: AzString::from(description),
    }
}

/// Returns the tag-specific data model fields for builtin HTML elements.
/// These are the component's "main data model" — the attributes that define
/// what the component needs as configuration (e.g., `href` for `<a>`,
/// `src` for `<img>`). Universal HTML attributes (id, class, style, etc.)
/// are NOT included here — they are added separately by the debug server.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one
                                 // branch per input variant)
fn builtin_data_model(tag: &str) -> Vec<ComponentDataField> {
    use ComponentDefaultValue as D;
    use ComponentFieldType::{Bool, String, I32};
    match tag {
        "a" => alloc::vec![
            data_field(
                "href",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "URL the link points to"
            ),
            data_field(
                "target",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Where to open the linked document (_blank, _self, _parent, _top)"
            ),
            data_field(
                "rel",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Relationship between current and linked document"
            ),
        ],
        "img" | "image" => alloc::vec![
            data_field("src", String, None, "URL of the image"),
            data_field(
                "alt",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Alternative text for the image"
            ),
            data_field(
                "width",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Width of the image"
            ),
            data_field(
                "height",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Height of the image"
            ),
            data_field(
                "align",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Float (left, right) or vertical alignment (top, middle, bottom) of the image"
            ),
            data_field(
                "border",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Width of the image's border in pixels"
            ),
            data_field(
                "hspace",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Gap left and right of the image in pixels"
            ),
            data_field(
                "vspace",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Gap above and below the image in pixels"
            ),
        ],
        "form" => alloc::vec![
            data_field(
                "action",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "URL where form data is submitted"
            ),
            data_field(
                "method",
                String,
                Some(D::String(AzString::from_const_str("GET"))),
                "HTTP method for form submission (GET or POST)"
            ),
        ],
        "label" => alloc::vec![data_field(
            "for",
            String,
            Some(D::String(AzString::from_const_str(""))),
            "ID of the form element this label is for"
        ),],
        "button" => alloc::vec![
            data_field(
                "type",
                String,
                Some(D::String(AzString::from_const_str("button"))),
                "Button type (button, submit, reset)"
            ),
            data_field(
                "disabled",
                Bool,
                Some(D::Bool(false)),
                "Whether the button is disabled"
            ),
        ],
        "td" | "th" => alloc::vec![
            data_field(
                "colspan",
                I32,
                Some(D::I32(1)),
                "Number of columns the cell spans"
            ),
            data_field(
                "rowspan",
                I32,
                Some(D::I32(1)),
                "Number of rows the cell spans"
            ),
        ],
        "icon" => alloc::vec![data_field(
            "name",
            String,
            Some(D::String(AzString::from_const_str(""))),
            "Icon name"
        ),],
        "ol" => alloc::vec![
            data_field(
                "start",
                I32,
                Some(D::I32(1)),
                "Start value for the ordered list"
            ),
            data_field(
                "type",
                String,
                Some(D::String(AzString::from_const_str("1"))),
                "Numbering type (1, A, a, I, i)"
            ),
            data_field(
                "reversed",
                Bool,
                Some(D::Bool(false)),
                "Whether the list counts down"
            ),
        ],
        "ul" => alloc::vec![
            data_field(
                "type",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Bullet type (disc, circle, square)"
            ),
        ],
        "li" => alloc::vec![
            data_field(
                "value",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "The item's number in an ordered list (the next items count on from it)"
            ),
            data_field(
                "type",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Numbering or bullet type of the item (1, A, a, I, i, disc, circle, square)"
            ),
        ],
        "font" => alloc::vec![
            data_field(
                "face",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Font family names, comma separated"
            ),
            data_field(
                "size",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Legacy font size 1..7, or relative to 3 (+1, -2)"
            ),
            data_field(
                "color",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Text color"
            ),
        ],
        "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => alloc::vec![
            data_field(
                "align",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Text alignment (left, right, center, justify)"
            ),
        ],
        "body" => alloc::vec![
            data_field(
                "bgcolor",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Background color of the document"
            ),
            data_field(
                "text",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Text color of the document"
            ),
        ],
        // Form controls
        "input" => alloc::vec![
            data_field(
                "type",
                String,
                Some(D::String(AzString::from_const_str("text"))),
                "Input type (text, password, email, number, checkbox, radio, etc.)"
            ),
            data_field(
                "name",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Name of the input for form submission"
            ),
            data_field(
                "value",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Current value of the input"
            ),
            data_field(
                "placeholder",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Placeholder text"
            ),
            data_field(
                "disabled",
                Bool,
                Some(D::Bool(false)),
                "Whether the input is disabled"
            ),
            data_field(
                "required",
                Bool,
                Some(D::Bool(false)),
                "Whether the input is required"
            ),
            data_field(
                "readonly",
                Bool,
                Some(D::Bool(false)),
                "Whether the input is read-only"
            ),
            data_field(
                "checked",
                Bool,
                Some(D::Bool(false)),
                "Whether the checkbox/radio is checked"
            ),
            data_field(
                "min",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Minimum value (for number, range, date)"
            ),
            data_field(
                "max",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Maximum value (for number, range, date)"
            ),
            data_field(
                "step",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Step increment (for number, range)"
            ),
            data_field(
                "pattern",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Regex pattern for validation"
            ),
            data_field(
                "maxlength",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Maximum number of characters"
            ),
        ],
        "select" => alloc::vec![
            data_field(
                "name",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Name for form submission"
            ),
            data_field(
                "multiple",
                Bool,
                Some(D::Bool(false)),
                "Whether multiple options can be selected"
            ),
            data_field(
                "disabled",
                Bool,
                Some(D::Bool(false)),
                "Whether the select is disabled"
            ),
            data_field(
                "required",
                Bool,
                Some(D::Bool(false)),
                "Whether selection is required"
            ),
            data_field(
                "size",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Number of visible options"
            ),
        ],
        "option" => alloc::vec![
            data_field(
                "value",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Value submitted with the form"
            ),
            data_field(
                "selected",
                Bool,
                Some(D::Bool(false)),
                "Whether this option is selected"
            ),
            data_field(
                "disabled",
                Bool,
                Some(D::Bool(false)),
                "Whether this option is disabled"
            ),
        ],
        "optgroup" => alloc::vec![
            data_field(
                "label",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Label for the option group"
            ),
            data_field(
                "disabled",
                Bool,
                Some(D::Bool(false)),
                "Whether the group is disabled"
            ),
        ],
        "textarea" => alloc::vec![
            data_field(
                "name",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Name for form submission"
            ),
            data_field(
                "placeholder",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Placeholder text"
            ),
            data_field("rows", I32, Some(D::I32(2)), "Number of visible text lines"),
            data_field(
                "cols",
                I32,
                Some(D::I32(20)),
                "Visible width in average character widths"
            ),
            data_field(
                "disabled",
                Bool,
                Some(D::Bool(false)),
                "Whether the textarea is disabled"
            ),
            data_field(
                "required",
                Bool,
                Some(D::Bool(false)),
                "Whether content is required"
            ),
            data_field(
                "readonly",
                Bool,
                Some(D::Bool(false)),
                "Whether the textarea is read-only"
            ),
            data_field(
                "maxlength",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Maximum number of characters"
            ),
        ],
        "fieldset" => alloc::vec![data_field(
            "disabled",
            Bool,
            Some(D::Bool(false)),
            "Whether all controls in the fieldset are disabled"
        ),],
        "output" => alloc::vec![
            data_field(
                "for",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "IDs of elements that contributed to the output"
            ),
            data_field(
                "name",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Name for form submission"
            ),
        ],
        "progress" => alloc::vec![
            data_field(
                "value",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Current progress value"
            ),
            data_field(
                "max",
                String,
                Some(D::String(AzString::from_const_str("1"))),
                "Maximum value"
            ),
        ],
        "meter" => alloc::vec![
            data_field(
                "value",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Current value"
            ),
            data_field(
                "min",
                String,
                Some(D::String(AzString::from_const_str("0"))),
                "Minimum value"
            ),
            data_field(
                "max",
                String,
                Some(D::String(AzString::from_const_str("1"))),
                "Maximum value"
            ),
            data_field(
                "low",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Low threshold"
            ),
            data_field(
                "high",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "High threshold"
            ),
            data_field(
                "optimum",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Optimum value"
            ),
        ],
        // Interactive
        "details" => alloc::vec![data_field(
            "open",
            Bool,
            Some(D::Bool(false)),
            "Whether the details are visible"
        ),],
        "dialog" => alloc::vec![data_field(
            "open",
            Bool,
            Some(D::Bool(false)),
            "Whether the dialog is active and can be interacted with"
        ),],
        // Embedded content
        "audio" | "video" => alloc::vec![
            data_field(
                "src",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "URL of the media resource"
            ),
            data_field(
                "controls",
                Bool,
                Some(D::Bool(false)),
                "Whether to show playback controls"
            ),
            data_field(
                "autoplay",
                Bool,
                Some(D::Bool(false)),
                "Whether to start playing automatically"
            ),
            data_field(
                "loop",
                Bool,
                Some(D::Bool(false)),
                "Whether to loop playback"
            ),
            data_field(
                "muted",
                Bool,
                Some(D::Bool(false)),
                "Whether audio is muted"
            ),
            data_field(
                "preload",
                String,
                Some(D::String(AzString::from_const_str("auto"))),
                "Preload hint (none, metadata, auto)"
            ),
        ],
        "source" => alloc::vec![
            data_field("src", String, None, "URL of the media resource"),
            data_field(
                "type",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "MIME type of the resource"
            ),
        ],
        "track" => alloc::vec![
            data_field("src", String, None, "URL of the track file"),
            data_field(
                "kind",
                String,
                Some(D::String(AzString::from_const_str("subtitles"))),
                "Kind of text track (subtitles, captions, descriptions, chapters, metadata)"
            ),
            data_field(
                "srclang",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Language of the track text"
            ),
            data_field(
                "label",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "User-readable title for the track"
            ),
            data_field(
                "default",
                Bool,
                Some(D::Bool(false)),
                "Whether this is the default track"
            ),
        ],
        "canvas" => alloc::vec![
            data_field(
                "width",
                String,
                Some(D::String(AzString::from_const_str("300"))),
                "Width of the canvas in pixels"
            ),
            data_field(
                "height",
                String,
                Some(D::String(AzString::from_const_str("150"))),
                "Height of the canvas in pixels"
            ),
        ],
        "embed" => alloc::vec![
            data_field("src", String, None, "URL of the resource to embed"),
            data_field(
                "type",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "MIME type of the embedded content"
            ),
            data_field(
                "width",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Width"
            ),
            data_field(
                "height",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Height"
            ),
        ],
        "object" => alloc::vec![
            data_field(
                "data",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "URL of the resource"
            ),
            data_field(
                "type",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "MIME type of the resource"
            ),
            data_field(
                "width",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Width"
            ),
            data_field(
                "height",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Height"
            ),
        ],
        "param" => alloc::vec![
            data_field("name", String, None, "Name of the parameter"),
            data_field(
                "value",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Value of the parameter"
            ),
        ],
        "area" => alloc::vec![
            data_field(
                "shape",
                String,
                Some(D::String(AzString::from_const_str("default"))),
                "Shape of the area (default, rect, circle, poly)"
            ),
            data_field(
                "coords",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Coordinates of the area"
            ),
            data_field(
                "href",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "URL for the area link"
            ),
            data_field(
                "alt",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Alternative text"
            ),
            data_field(
                "target",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Where to open the linked document"
            ),
        ],
        "map" => alloc::vec![data_field(
            "name",
            String,
            None,
            "Name of the image map (referenced by usemap)"
        ),],
        // Inline semantics with special attributes
        "time" => alloc::vec![data_field(
            "datetime",
            String,
            Some(D::String(AzString::from_const_str(""))),
            "Machine-readable date/time value"
        ),],
        "data" => alloc::vec![data_field(
            "value",
            String,
            Some(D::String(AzString::from_const_str(""))),
            "Machine-readable value"
        ),],
        "abbr" | "acronym" | "dfn" => alloc::vec![data_field(
            "title",
            String,
            Some(D::String(AzString::from_const_str(""))),
            "Full expansion or definition"
        ),],
        "q" | "blockquote" => alloc::vec![data_field(
            "cite",
            String,
            Some(D::String(AzString::from_const_str(""))),
            "URL of the source of the quotation"
        ),],
        "del" | "ins" => alloc::vec![
            data_field(
                "cite",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "URL explaining the change"
            ),
            data_field(
                "datetime",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Date/time of the change"
            ),
        ],
        "bdo" => alloc::vec![data_field(
            "dir",
            String,
            Some(D::String(AzString::from_const_str("ltr"))),
            "Text direction (ltr, rtl)"
        ),],
        "col" | "colgroup" => alloc::vec![data_field(
            "span",
            I32,
            Some(D::I32(1)),
            "Number of columns the element spans"
        ),],
        // Metadata
        "meta" => alloc::vec![
            data_field(
                "name",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Metadata name"
            ),
            data_field(
                "content",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Metadata value"
            ),
            data_field(
                "charset",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Character encoding"
            ),
            data_field(
                "http-equiv",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "HTTP header equivalent"
            ),
        ],
        "link" => alloc::vec![
            data_field("rel", String, None, "Relationship type"),
            data_field(
                "href",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "URL of the linked resource"
            ),
            data_field(
                "type",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "MIME type of the linked resource"
            ),
        ],
        "script" => alloc::vec![
            data_field(
                "src",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "URL of external script"
            ),
            data_field(
                "type",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "MIME type or module"
            ),
            data_field(
                "async",
                Bool,
                Some(D::Bool(false)),
                "Execute asynchronously"
            ),
            data_field(
                "defer",
                Bool,
                Some(D::Bool(false)),
                "Defer execution until page load"
            ),
        ],
        "style" => alloc::vec![data_field(
            "type",
            String,
            Some(D::String(AzString::from_const_str("text/css"))),
            "MIME type of the style sheet"
        ),],
        "base" => alloc::vec![
            data_field(
                "href",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Base URL for relative URLs"
            ),
            data_field(
                "target",
                String,
                Some(D::String(AzString::from_const_str(""))),
                "Default target for hyperlinks"
            ),
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
        Self {
            libraries: ComponentLibraryVec::from_const_slice(&[]),
        }
    }
}

impl ComponentMap {
    #[must_use]
    pub fn create() -> Self {
        Self::default()
    }

    /// Create a `ComponentMap` with the 52 built-in HTML element components pre-registered.
    #[must_use]
    pub fn with_builtin() -> Self {
        Self {
            libraries: alloc::vec![register_builtin_components()].into(),
        }
    }

    /// Build a `ComponentMap` from the libraries stored in an `AppConfig`.
    ///
    /// The `component_libraries` field already contains builtins (registered in
    /// `AppConfig::create()`) plus any user-added libraries.  No merging needed —
    /// `add_component_library` / `add_component` handle insertion at registration time.
    #[must_use]
    pub fn from_libraries(libs: &ComponentLibraryVec) -> Self {
        Self {
            libraries: libs.clone(),
        }
    }
}

/// A component's ARGUMENTS from an element's attributes.
///
/// `dm` with every field it declares set from the attribute of the same name,
/// parsed to the field's type; every other field keeps its default, and an
/// attribute no field declares adds nothing.
///
/// THE one path from markup attributes to component arguments, for every
/// component - builtin and user alike: the XML loaders fill a builtin
/// element's arguments with it ([`apply_builtin_args_from_attributes`]), the
/// builder fills a user component instance's (`e2e::builder`), and the
/// render fn reads the result.
///
/// Names match case-insensitively, as HTML attribute names do. A `Bool`
/// follows HTML's boolean attributes: present means `true` (`disabled=""`),
/// unless it says `false` / `0` / `no` / `off`. A number that does not parse
/// keeps the field's default. Field types an attribute string cannot carry
/// (a `StyledDom`, a callback, a struct) keep their defaults.
#[must_use]
pub fn data_model_with_attributes<'a>(
    dm: &ComponentDataModel,
    attributes: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> ComponentDataModel {
    let attributes: Vec<(&str, &str)> = attributes.into_iter().collect();
    let mut model = dm.clone();
    let mut fields_vec = core::mem::replace(
        &mut model.fields,
        ComponentDataFieldVec::from_const_slice(&[]),
    )
    .into_library_owned_vec();
    for field in &mut fields_vec {
        let Some((_, raw)) = attributes
            .iter()
            .rev()
            .find(|(k, _)| k.trim().eq_ignore_ascii_case(field.name.as_str()))
        else {
            continue;
        };
        let t = raw.trim();
        let parsed = match field.field_type {
            ComponentFieldType::String => Some(ComponentDefaultValue::String(AzString::from(*raw))),
            ComponentFieldType::Bool => Some(ComponentDefaultValue::Bool(
                !(t.eq_ignore_ascii_case("false")
                    || t == "0"
                    || t.eq_ignore_ascii_case("no")
                    || t.eq_ignore_ascii_case("off")),
            )),
            ComponentFieldType::I32 => t.parse::<i32>().ok().map(ComponentDefaultValue::I32),
            ComponentFieldType::I64 => t.parse::<i64>().ok().map(ComponentDefaultValue::I64),
            ComponentFieldType::U32 => t.parse::<u32>().ok().map(ComponentDefaultValue::U32),
            ComponentFieldType::U64 => t.parse::<u64>().ok().map(ComponentDefaultValue::U64),
            ComponentFieldType::Usize => t.parse::<usize>().ok().map(ComponentDefaultValue::Usize),
            ComponentFieldType::F32 => t.parse::<f32>().ok().map(ComponentDefaultValue::F32),
            ComponentFieldType::F64 => t.parse::<f64>().ok().map(ComponentDefaultValue::F64),
            _ => None,
        };
        if let Some(value) = parsed {
            field.default_value = OptionComponentDefaultValue::Some(value);
        }
    }
    model.fields = ComponentDataFieldVec::from_vec(fields_vec);
    model
}

/// The builtin elements whose component arguments
/// [`apply_builtin_element_args`] lands on the node (the others' declared
/// fields are read by the element's own path - `img` `width`/`height`, the
/// form controls, `td` `colspan` - or not yet at all; see
/// `scripts/MAILVIEW_2026_09_30.md`).
const BUILTIN_ARGUMENT_ELEMENTS: &[&str] = &["a", "area", "link", "base", "img", "ol", "li"];

/// The render side of a builtin element's ARGUMENTS (its component's
/// declared fields, filled by [`data_model_with_attributes`]): what they set
/// on its node.
///
/// - `a`, `area`, `link`, `base`: `href` - where a click on the link goes; an app reads it
///   with `CallbackInfo::get_node_attribute(node, "href")` - plus `target` and `rel` (not on
///   `base`, which has no `rel`).
/// - `img`: `src` and `alt`, as attributes an app can read (the image itself stays the
///   loader's `NullImage` placeholder carrying `src`) - what a mail client needs to show "[image:
///   alt]" and to load the picture on request.
/// - `ol`: `reversed` (and its `start` when it counts down) as the attributes the layout's list
///   numbering reads (`compute_counters`); an `ol` counting up starts through its
///   presentational hint ([`builtin_presentational_hints`]: `counter-reset`).
/// - `li`: `value` (a number) as its `Value` attribute: the item's number in its list, which
///   the layout's numbering reads and the next items count on from.
///
/// An empty value sets nothing; a value already on the node is not
/// duplicated.
pub fn apply_builtin_element_args(tag: &str, args: &ComponentDataModel, node: &mut NodeData) {
    use crate::dom::AttributeType as A;

    let value = |name: &str| {
        args.get_default_string(name)
            .filter(|v| !v.as_str().trim().is_empty())
            .cloned()
    };
    let mut add: Vec<A> = Vec::new();
    match tag {
        "a" | "area" | "link" => {
            add.extend(value("href").map(A::Href));
            add.extend(value("target").map(A::Target));
            add.extend(value("rel").map(A::Rel));
        }
        "base" => {
            add.extend(value("href").map(A::Href));
            add.extend(value("target").map(A::Target));
        }
        "img" | "image" => {
            add.extend(value("src").map(A::Src));
            add.extend(value("alt").map(A::Alt));
        }
        "ol" => {
            if argument_bool(args, "reversed") {
                add.push(A::Custom(crate::dom::AttributeNameValue {
                    attr_name: AzString::from_const_str("reversed"),
                    value: AzString::from_const_str(""),
                }));
                if let Some(start) = argument_i32(args, "start").filter(|s| *s != 1) {
                    add.push(A::Custom(crate::dom::AttributeNameValue {
                        attr_name: AzString::from_const_str("start"),
                        value: AzString::from(start.to_string()),
                    }));
                }
            }
        }
        "li" => {
            add.extend(
                value("value")
                    .filter(|v| v.as_str().trim().parse::<i32>().is_ok())
                    .map(A::Value),
            );
        }
        _ => {}
    }
    if add.is_empty() {
        return;
    }
    let mut all = node.attributes().clone().into_library_owned_vec();
    for a in add {
        if !all.contains(&a) {
            all.push(a);
        }
    }
    node.set_attributes(all.into());
}

/// For the XML loaders: a builtin element's component arguments from its
/// attributes.
///
/// Its builtin data model is filled by [`data_model_with_attributes`] and
/// landed by [`apply_builtin_element_args`]. Only the elements whose arguments
/// land on the node ([`BUILTIN_ARGUMENT_ELEMENTS`]) build a model, so the
/// thousands of `div`s of a large document cost one slice lookup. `tag` is
/// lowercase.
pub fn apply_builtin_args_from_attributes<'a>(
    tag: &str,
    attributes: impl IntoIterator<Item = (&'a str, &'a str)>,
    node: &mut NodeData,
) {
    if !BUILTIN_ARGUMENT_ELEMENTS.contains(&tag) {
        return;
    }
    let model = ComponentDataModel {
        name: AzString::from_const_str(""),
        description: AzString::from_const_str(""),
        fields: builtin_data_model(tag).into(),
    };
    let args = data_model_with_attributes(&model, attributes);
    apply_builtin_element_args(tag, &args, node);
}

/// An `I32` argument's value.
fn argument_i32(args: &ComponentDataModel, name: &str) -> Option<i32> {
    match &args.get_field(name)?.default_value {
        OptionComponentDefaultValue::Some(ComponentDefaultValue::I32(n)) => Some(*n),
        _ => None,
    }
}

/// A `Bool` argument's value (`false` when the model has none).
fn argument_bool(args: &ComponentDataModel, name: &str) -> bool {
    matches!(
        args.get_field(name).map(|f| &f.default_value),
        Some(OptionComponentDefaultValue::Some(ComponentDefaultValue::Bool(true)))
    )
}

/// The elements that have presentational hints ([`builtin_presentational_hints`]).
const PRESENTATIONAL_ELEMENTS: &[&str] = &[
    "ol", "ul", "li", "img", "image", "font", "center", "div", "p", "h1", "h2", "h3", "h4", "h5",
    "h6", "body",
];

/// The CSS of a builtin element's PRESENTATIONAL arguments.
///
/// The HTML Standard's presentational hints (its rendering section), as
/// declarations (`list-style-type: lower-alpha; ...`, empty for none):
///
/// - `ol` / `ul` / `li` `type` (`1 a A i I`, `disc circle square`): `list-style-type`; `ol
///   start` (counting up): `counter-reset: list-item <start - 1>`;
/// - `font`: `face` -> `font-family`, `size` (1..7, or relative to 3) -> `font-size` (10 13 16 18
///   24 32 48 px), `color` -> `color`;
/// - `center`: `text-align: center`; `align` on `div`, `p`, `h1`..`h6`: `text-align`;
/// - `img`: `align` (`left` / `right` float, `top` / `middle` / `bottom` ... align vertically),
///   `border` (a solid border of that width), `hspace` / `vspace` (margins), a percentage `width`
///   / `height` (a number is the image's intrinsic size, which the loader sets);
/// - `body`: `bgcolor` -> `background-color`, `text` -> `color`.
///
/// The arguments are the element's builtin data model filled from its
/// `attributes` ([`data_model_with_attributes`], the one filler); a value that
/// is not what the attribute takes (a colour with a `;` in it) is left out.
/// The XML loaders put these declarations BEFORE the element's `style`
/// attribute's, which wins over them. (A browser ranks them below the
/// author's stylesheets too; azul's inline declarations outrank every
/// stylesheet, so a sheet's rule cannot override a hint yet.) The table
/// attributes (`bgcolor` / `width` / `align` on `table`, `td` ...) are not
/// here.
#[must_use]
pub fn builtin_presentational_hints<'a>(
    tag: &str,
    attributes: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> String {
    if !PRESENTATIONAL_ELEMENTS.contains(&tag) {
        return String::new();
    }
    let model = ComponentDataModel {
        name: AzString::from_const_str(""),
        description: AzString::from_const_str(""),
        fields: builtin_data_model(tag).into(),
    };
    let args = data_model_with_attributes(&model, attributes);
    let text = |name: &str| -> Option<String> {
        args.get_default_string(name)
            .map(|v| v.as_str().trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let mut css = String::new();
    let mut push = |property: &str, value: &str| {
        css.push_str(property);
        css.push_str(": ");
        css.push_str(value);
        css.push_str("; ");
    };
    match tag {
        "ol" | "ul" | "li" => {
            if let Some(kind) = text("type").filter(|t| !(tag == "ol" && t == "1")) {
                if let Some(style) = list_style_of_type(&kind) {
                    push("list-style-type", style);
                }
            }
            if tag == "ol" && !argument_bool(&args, "reversed") {
                if let Some(start) = argument_i32(&args, "start").filter(|s| *s != 1) {
                    push(
                        "counter-reset",
                        &format!("list-item {}", start.saturating_sub(1)),
                    );
                }
            }
        }
        "img" | "image" => {
            for (attribute, property) in [("width", "width"), ("height", "height")] {
                if let Some(percent) = text(attribute).and_then(|v| html_percentage(&v)) {
                    push(property, &percent);
                }
            }
            if let Some(align) = text("align") {
                match align.to_ascii_lowercase().as_str() {
                    "left" => push("float", "left"),
                    "right" => push("float", "right"),
                    "top" => push("vertical-align", "top"),
                    "texttop" => push("vertical-align", "text-top"),
                    "middle" | "absmiddle" | "abscenter" => push("vertical-align", "middle"),
                    "bottom" | "baseline" => push("vertical-align", "baseline"),
                    "absbottom" => push("vertical-align", "bottom"),
                    _ => {}
                }
            }
            if let Some(border) = text("border")
                .and_then(|v| html_pixels(&v))
                .filter(|b| *b > 0)
            {
                push("border", &format!("{border}px solid"));
            }
            if let Some(h) = text("hspace").and_then(|v| html_pixels(&v)) {
                push("margin-left", &format!("{h}px"));
                push("margin-right", &format!("{h}px"));
            }
            if let Some(v) = text("vspace").and_then(|v| html_pixels(&v)) {
                push("margin-top", &format!("{v}px"));
                push("margin-bottom", &format!("{v}px"));
            }
        }
        "font" => {
            if let Some(face) = text("face").filter(|f| is_safe_css_value(f)) {
                push("font-family", &face);
            }
            if let Some(px) = text("size").and_then(|v| legacy_font_size_px(&v)) {
                push("font-size", &format!("{px}px"));
            }
            if let Some(color) = text("color").and_then(|v| legacy_color(&v)) {
                push("color", &color);
            }
        }
        "center" => push("text-align", "center"),
        "body" => {
            if let Some(color) = text("bgcolor").and_then(|v| legacy_color(&v)) {
                push("background-color", &color);
            }
            if let Some(color) = text("text").and_then(|v| legacy_color(&v)) {
                push("color", &color);
            }
        }
        _ => {
            // div, p, h1..h6
            if let Some(align) = text("align") {
                let align = align.to_ascii_lowercase();
                if matches!(align.as_str(), "left" | "right" | "center" | "justify") {
                    push("text-align", &align);
                }
            }
        }
    }
    css
}

/// HTML's list `type`: `1 a A i I` (case matters) and `disc circle square`.
fn list_style_of_type(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "1" => "decimal",
        "a" => "lower-alpha",
        "A" => "upper-alpha",
        "i" => "lower-roman",
        "I" => "upper-roman",
        k if k.eq_ignore_ascii_case("disc") => "disc",
        k if k.eq_ignore_ascii_case("circle") => "circle",
        k if k.eq_ignore_ascii_case("square") => "square",
        k if k.eq_ignore_ascii_case("none") => "none",
        _ => return None,
    })
}

/// A non-negative number of pixels (`4`, `4px`).
fn html_pixels(value: &str) -> Option<u32> {
    value
        .trim()
        .trim_end_matches("px")
        .trim()
        .parse::<u32>()
        .ok()
}

/// A percentage (`100%`) as CSS.
fn html_percentage(value: &str) -> Option<String> {
    let number = value.trim().strip_suffix('%')?.trim();
    number
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map(|_| format!("{number}%"))
}

/// The pixel size of HTML's legacy font sizes: 1..7 (x-small .. xxx-large), a
/// `+n` / `-n` relative to 3.
fn legacy_font_size_px(value: &str) -> Option<u32> {
    // Sizes 1..7 are the `font-size` keywords from `x-small` on (one table).
    let px = &azul_css::props::basic::font::FONT_SIZE_KEYWORDS_PX[1..];
    let value = value.trim();
    let (relative, digits) = match value.as_bytes().first()? {
        b'+' => (1, &value[1..]),
        b'-' => (-1, &value[1..]),
        _ => (0, value),
    };
    let digits: String = digits.chars().take_while(char::is_ascii_digit).collect();
    let n: i32 = digits.parse().ok()?;
    let size = if relative == 0 { n } else { 3 + relative * n };
    let index = usize::try_from(size.clamp(1, 7) - 1).ok()?;
    px.get(index).map(|(_, size)| u32::from(*size))
}

/// A legacy colour attribute as a CSS colour: `#rgb` / `#rrggbb` (also
/// without the `#`, as mail writes it), a colour name, `rgb(..)`; `None` for
/// anything else.
fn legacy_color(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || !is_safe_css_value(value) {
        return None;
    }
    let hex = value.strip_prefix('#').unwrap_or(value);
    if matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(format!("#{hex}"));
    }
    if value.starts_with('#') {
        return None;
    }
    Some(value.to_string())
}

/// A value that stays one CSS value: no `;`, braces, escapes, markup or
/// control characters.
fn is_safe_css_value(value: &str) -> bool {
    !value
        .chars()
        .any(|c| matches!(c, ';' | '{' | '}' | '\\' | '<' | '>') || c.is_control())
}

// ============================================================================
// Structural builtin components: if, for, map
// ============================================================================

/// `builtin:if` — conditional rendering.
/// Takes `condition: Bool`, `then: StyledDom`, and optionally `else: StyledDom`.
fn builtin_if_component() -> ComponentDef {
    ComponentDef {
        id: ComponentId::builtin("if"),
        display_name: AzString::from_const_str("If"),
        description: AzString::from_const_str(
            "Conditional rendering: shows 'then' if condition is true, else shows 'else' (if \
             provided).",
        ),
        css: AzString::from_const_str(""),
        source: ComponentSource::Builtin,
        data_model: ComponentDataModel {
            name: AzString::from_const_str("IfData"),
            description: AzString::from_const_str("Data for conditional rendering"),
            fields: alloc::vec![data_field(
                "condition",
                ComponentFieldType::Bool,
                Some(ComponentDefaultValue::Bool(false)),
                "The boolean condition to evaluate"
            ),]
            .into(),
        },
        render_fn: builtin_if_render_fn,
        codegen: ComponentCodegen::RenderFunction,
        render_fn_source: None.into(),
    }
}

fn builtin_if_render_fn(
    _comp: &ComponentDef,
    data_model: &ComponentDataModel,
    _component_map: &ComponentMap,
) -> ResultStyledDomRenderDomError {
    // Evaluate the condition field
    let condition = data_model
        .fields
        .iter()
        .find(|f| f.name.as_str() == "condition")
        .and_then(|f| match &f.default_value {
            OptionComponentDefaultValue::Some(ComponentDefaultValue::Bool(b)) => Some(*b),
            _ => None,
        })
        .unwrap_or(false);

    let label = if condition {
        "if: true (then branch)"
    } else {
        "if: false (else branch)"
    };
    let mut dom = Dom::create_node(NodeType::Div).with_children(
        alloc::vec![Dom::create_text_do_not_use_without_block_level_wrapper(
            label
        )]
        .into(),
    );
    let css = Css::empty();
    ResultStyledDomRenderDomError::Ok(StyledDom::create(&mut dom, css))
}

/// `builtin:for` — iterative rendering.
/// Takes `count: U32` (number of iterations), renders children N times.
fn builtin_for_component() -> ComponentDef {
    ComponentDef {
        id: ComponentId::builtin("for"),
        display_name: AzString::from_const_str("For Loop"),
        description: AzString::from_const_str(
            "Iterative rendering: repeats children 'count' times.",
        ),
        css: AzString::from_const_str(""),
        source: ComponentSource::Builtin,
        data_model: ComponentDataModel {
            name: AzString::from_const_str("ForData"),
            description: AzString::from_const_str("Data for iterative rendering"),
            fields: alloc::vec![data_field(
                "count",
                ComponentFieldType::U32,
                Some(ComponentDefaultValue::U32(3)),
                "Number of iterations"
            ),]
            .into(),
        },
        render_fn: builtin_for_render_fn,
        codegen: ComponentCodegen::RenderFunction,
        render_fn_source: None.into(),
    }
}

fn builtin_for_render_fn(
    _comp: &ComponentDef,
    data_model: &ComponentDataModel,
    _component_map: &ComponentMap,
) -> ResultStyledDomRenderDomError {
    let count = data_model
        .fields
        .iter()
        .find(|f| f.name.as_str() == "count")
        .and_then(|f| match &f.default_value {
            OptionComponentDefaultValue::Some(ComponentDefaultValue::U32(n)) => Some(*n),
            _ => None,
        })
        .unwrap_or(3);

    let mut items: Vec<Dom> = Vec::new();
    for i in 0..count {
        items.push(
            Dom::create_node(NodeType::Div).with_children(
                alloc::vec![Dom::create_text_do_not_use_without_block_level_wrapper(
                    alloc::format!("Item {i}")
                )]
                .into(),
            ),
        );
    }
    let mut dom = Dom::create_node(NodeType::Div).with_children(items.into());
    let css = Css::empty();
    ResultStyledDomRenderDomError::Ok(StyledDom::create(&mut dom, css))
}

/// `builtin:map` — map data to DOM.
/// Takes `data_json: String` (JSON array) + maps each element.
fn builtin_map_component() -> ComponentDef {
    ComponentDef {
        id: ComponentId::builtin("map"),
        display_name: AzString::from_const_str("Map"),
        description: AzString::from_const_str(
            "Map data to DOM: applies a template to each item in a collection.",
        ),
        css: AzString::from_const_str(""),
        source: ComponentSource::Builtin,
        data_model: ComponentDataModel {
            name: AzString::from_const_str("MapData"),
            description: AzString::from_const_str("Data for map rendering"),
            fields: alloc::vec![data_field(
                "data_json",
                ComponentFieldType::String,
                Some(ComponentDefaultValue::String(AzString::from_const_str(
                    "[]"
                ))),
                "JSON array of items to map over"
            ),]
            .into(),
        },
        render_fn: builtin_map_render_fn,
        codegen: ComponentCodegen::RenderFunction,
        render_fn_source: None.into(),
    }
}

fn builtin_map_render_fn(
    _comp: &ComponentDef,
    data_model: &ComponentDataModel,
    _component_map: &ComponentMap,
) -> ResultStyledDomRenderDomError {
    // For now, render a placeholder — actual mapping requires callback support
    let data_str = data_model
        .fields
        .iter()
        .find(|f| f.name.as_str() == "data_json")
        .and_then(|f| match &f.default_value {
            OptionComponentDefaultValue::Some(ComponentDefaultValue::String(s)) => {
                Some(s.as_str().to_string())
            }
            _ => None,
        })
        .unwrap_or_else(|| "[]".to_string());

    let label = alloc::format!("map: data_json={data_str}");
    let mut dom = Dom::create_node(NodeType::Div).with_children(
        alloc::vec![Dom::create_text_do_not_use_without_block_level_wrapper(
            label
        )]
        .into(),
    );
    let css = Css::empty();
    ResultStyledDomRenderDomError::Ok(StyledDom::create(&mut dom, css))
}

// ============================================================================
// The builtin HTML elements: ONE table (name, text default, preview)
// ============================================================================

/// What a builtin element's PREVIEW shows - its palette card in `AzBuilder`
/// (`get_component_thumbnail`) and the Components view's preview. Configured
/// once per element in [`BUILTIN_ELEMENTS`], next to its text default, and
/// rendered by [`builtin_preview_dom`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinPreview {
    /// The element as a drop inserts it: with its default text (a `<p>`, a
    /// `<strong>`), or drawing by itself (an `<hr>`, a `<button>`).
    Itself,
    /// The element holding an EXAMPLE that a drop does not insert: a
    /// container is dropped empty, but its card shows what it is for (a
    /// `<ul>` with two items, a `<section>` as a labelled box, an `<input>`
    /// with a placeholder). `text` stands in only where the element has no
    /// text of its own.
    Example {
        attrs: &'static [(&'static str, &'static str)],
        text: &'static str,
        children: &'static [PreviewNode],
    },
    /// Nothing to show on its own (document structure, a break, what only
    /// shows inside another element or shows a source): the reason, which
    /// the card shows instead of an empty box.
    NoVisual(&'static str),
}

impl BuiltinPreview {
    /// The element holding `children`.
    const fn holding(children: &'static [PreviewNode]) -> Self {
        Self::Example {
            attrs: &[],
            text: "",
            children,
        }
    }

    /// The element with the example attributes `attrs`.
    const fn with(attrs: &'static [(&'static str, &'static str)]) -> Self {
        Self::Example {
            attrs,
            text: "",
            children: &[],
        }
    }

    /// A block container: a dashed box with `label` in it.
    const fn boxed(label: &'static str) -> Self {
        Self::Example {
            attrs: PREVIEW_BOX,
            text: label,
            children: &[],
        }
    }

    /// Its example as `(attrs, text, children)`; empty for the other kinds.
    const fn example(
        self,
    ) -> (
        &'static [(&'static str, &'static str)],
        &'static str,
        &'static [PreviewNode],
    ) {
        match self {
            Self::Example {
                attrs,
                text,
                children,
            } => (attrs, text, children),
            Self::Itself | Self::NoVisual(_) => (&[], "", &[]),
        }
    }
}

/// One element of a [`BuiltinPreview::Example`]: `<tag attrs>text children</tag>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreviewNode {
    tag: &'static str,
    attrs: &'static [(&'static str, &'static str)],
    text: &'static str,
    children: &'static [PreviewNode],
}

impl PreviewNode {
    /// `<tag>text</tag>`
    const fn text(tag: &'static str, text: &'static str) -> Self {
        Self {
            tag,
            attrs: &[],
            text,
            children: &[],
        }
    }

    /// `<tag>children</tag>`
    const fn holding(tag: &'static str, children: &'static [Self]) -> Self {
        Self {
            tag,
            attrs: &[],
            text: "",
            children,
        }
    }

    /// `<tag attrs/>`
    const fn with(tag: &'static str, attrs: &'static [(&'static str, &'static str)]) -> Self {
        Self {
            tag,
            attrs,
            text: "",
            children: &[],
        }
    }

    /// This node as parsed markup, for `xml_node_to_dom_fast`.
    fn markup(&self) -> XmlNode {
        preview_xml(self.tag, self.attrs, self.text, self.children)
    }
}

/// One builtin HTML element: the component `builtin:<tag>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BuiltinElement {
    tag: &'static str,
    display_name: &'static str,
    /// The default of its `text` field - which a drop inserts
    /// (`builder_insert`) and its preview shows; `None`: it takes no text.
    text: Option<&'static str>,
    preview: BuiltinPreview,
}

const fn el(
    tag: &'static str,
    display_name: &'static str,
    text: Option<&'static str>,
    preview: BuiltinPreview,
) -> BuiltinElement {
    BuiltinElement {
        tag,
        display_name,
        text,
        preview,
    }
}

/// The look of a container's preview: a dashed box, so an empty-looking
/// block still reads as one.
const PREVIEW_BOX: &[(&str, &str)] = &[(
    "style",
    "border: 1px dashed #9ca3af; padding: 4px 6px; color: #4b5563",
)];
const LIST_ITEMS: &[PreviewNode] = &[
    PreviewNode::text("li", "First item"),
    PreviewNode::text("li", "Second item"),
];
const TERM_AND_DESCRIPTION: &[PreviewNode] = &[
    PreviewNode::text("dt", "Term"),
    PreviewNode::text("dd", "Description"),
];
const HEAD_CELLS: &[PreviewNode] = &[
    PreviewNode::text("th", "Name"),
    PreviewNode::text("th", "Value"),
];
const BODY_CELLS: &[PreviewNode] = &[
    PreviewNode::text("td", "Width"),
    PreviewNode::text("td", "42"),
];
const HEAD_ROW: PreviewNode = PreviewNode::holding("tr", HEAD_CELLS);
const BODY_ROW: PreviewNode = PreviewNode::holding("tr", BODY_CELLS);
const HEAD_ROWS: &[PreviewNode] = &[HEAD_ROW];
const BODY_ROWS: &[PreviewNode] = &[BODY_ROW];
const TABLE_ROWS: &[PreviewNode] = &[HEAD_ROW, BODY_ROW];
const FIGURE_CAPTION: &[PreviewNode] = &[PreviewNode::text("figcaption", "Figure caption")];
const DETAILS_SUMMARY: &[PreviewNode] = &[PreviewNode::text("summary", "Details")];
const RUBY_TEXT: &[PreviewNode] = &[PreviewNode::text("rt", "annotation")];
const FORM_FIELDS: &[PreviewNode] = &[
    PreviewNode::text("label", "Name"),
    PreviewNode::with("input", &[("placeholder", "Your name")]),
];
const FIELDSET_FIELDS: &[PreviewNode] = &[
    PreviewNode::text("legend", "Legend"),
    PreviewNode::text("label", "Field"),
];
const SELECT_OPTIONS: &[PreviewNode] = &[
    PreviewNode::text("option", "Option 1"),
    PreviewNode::text("option", "Option 2"),
];
const SVG_SHAPES: &[PreviewNode] = &[
    PreviewNode::with(
        "circle",
        &[("cx", "12"), ("cy", "12"), ("r", "10"), ("fill", "#3b82f6")],
    ),
    PreviewNode::with(
        "path",
        &[
            ("d", "M 26,3 L 44,3 L 44,21 L 26,21 Z"),
            ("fill", "#f59e0b"),
        ],
    ),
];

/// Every builtin HTML element, in palette order: THE one place its name, its
/// text default and its preview are configured ([`register_builtin_components`]
/// builds the components from it, [`builtin_preview_dom`] the previews).
#[rustfmt::skip]
static BUILTIN_ELEMENTS: &[BuiltinElement] = {
    use BuiltinPreview::{Itself, NoVisual};
    &[
        // Structural
        el("html", "HTML", None, NoVisual("the document root: a builder document is its <body>")),
        el("head", "Head", None, NoVisual("the document's metadata: it has no box")),
        el("title", "Title", Some(""), NoVisual("the window title: not drawn in the page")),
        el("body", "Body", None, NoVisual("the document itself: the builder document's root")),
        // Block-level
        el("div", "Div", None, BuiltinPreview::boxed("Div")),
        el("header", "Header", None, BuiltinPreview::boxed("Header")),
        el("footer", "Footer", None, BuiltinPreview::boxed("Footer")),
        el("section", "Section", None, BuiltinPreview::boxed("Section")),
        el("article", "Article", None, BuiltinPreview::boxed("Article")),
        el("aside", "Aside", None, BuiltinPreview::boxed("Aside")),
        el("nav", "Nav", None, BuiltinPreview::boxed("Nav")),
        el("main", "Main", None, BuiltinPreview::boxed("Main")),
        el("figure", "Figure", None, BuiltinPreview::Example { attrs: PREVIEW_BOX, text: "", children: FIGURE_CAPTION }),
        el("figcaption", "Figure Caption", Some("Figure caption"), Itself),
        el("address", "Address", Some("Address"), Itself),
        el("details", "Details", None, BuiltinPreview::holding(DETAILS_SUMMARY)),
        el("summary", "Summary", Some("Details"), Itself),
        el("dialog", "Dialog", None, BuiltinPreview::boxed("Dialog")),
        // Headings: the level's name, so the preview shows its size
        el("h1", "Heading 1", Some("Heading 1"), Itself),
        el("h2", "Heading 2", Some("Heading 2"), Itself),
        el("h3", "Heading 3", Some("Heading 3"), Itself),
        el("h4", "Heading 4", Some("Heading 4"), Itself),
        el("h5", "Heading 5", Some("Heading 5"), Itself),
        el("h6", "Heading 6", Some("Heading 6"), Itself),
        // Text content
        el("p", "Paragraph", Some("Paragraph text"), Itself),
        el("span", "Span", Some("Span text"), Itself),
        el("pre", "Preformatted", Some("Preformatted text"), Itself),
        el("code", "Code", Some("code"), Itself),
        el("blockquote", "Blockquote", Some("Blockquote"), Itself),
        el("br", "Line Break", None, NoVisual("a line break inside text: no box of its own")),
        el("hr", "Horizontal Rule", None, Itself),
        el("pagebreak", "Page Break", None, NoVisual("a page break: only paged output (print, PDF) shows it")),
        // The icon's spec is its text content (`<icon>home</icon>`).
        el("icon", "Icon", Some("home"), Itself),
        // Lists
        el("ul", "Unordered List", None, BuiltinPreview::holding(LIST_ITEMS)),
        el("ol", "Ordered List", None, BuiltinPreview::holding(LIST_ITEMS)),
        el("li", "List Item", Some("List item"), Itself),
        el("dl", "Description List", None, BuiltinPreview::holding(TERM_AND_DESCRIPTION)),
        el("dt", "Description Term", Some("Term"), Itself),
        el("dd", "Description Details", Some("Description"), Itself),
        el("menu", "Menu", None, BuiltinPreview::holding(LIST_ITEMS)),
        el("menuitem", "Menu Item", Some("Menu item"), Itself),
        el("dir", "Directory List", None, BuiltinPreview::holding(LIST_ITEMS)),
        // Tables
        el("table", "Table", None, BuiltinPreview::holding(TABLE_ROWS)),
        el("caption", "Table Caption", Some("Table caption"), Itself),
        el("thead", "Table Head", None, BuiltinPreview::holding(HEAD_ROWS)),
        el("tbody", "Table Body", None, BuiltinPreview::holding(BODY_ROWS)),
        el("tfoot", "Table Foot", None, BuiltinPreview::holding(BODY_ROWS)),
        el("tr", "Table Row", None, BuiltinPreview::holding(BODY_CELLS)),
        el("th", "Table Header Cell", Some("Header"), Itself),
        el("td", "Table Data Cell", Some("Cell"), Itself),
        el("colgroup", "Column Group", None, NoVisual("styles a table's columns: no box of its own")),
        el("col", "Column", None, NoVisual("styles a table's column: no box of its own")),
        // Inline
        el("a", "Link", Some("Link text"), Itself),
        el("strong", "Strong", Some("Strong text"), Itself),
        el("em", "Emphasis", Some("Emphasized text"), Itself),
        el("b", "Bold", Some("Bold text"), Itself),
        el("i", "Italic", Some("Italic text"), Itself),
        el("u", "Underline", Some("Underlined text"), Itself),
        el("s", "Strikethrough", Some("Struck-through text"), Itself),
        el("small", "Small", Some("Small text"), Itself),
        el("mark", "Mark", Some("Marked text"), Itself),
        el("del", "Deleted Text", Some("Deleted text"), Itself),
        el("ins", "Inserted Text", Some("Inserted text"), Itself),
        el("sub", "Subscript", Some("Subscript"), Itself),
        el("sup", "Superscript", Some("Superscript"), Itself),
        el("samp", "Sample Output", Some("Sample output"), Itself),
        el("kbd", "Keyboard Input", Some("Ctrl+C"), Itself),
        el("var", "Variable", Some("x"), Itself),
        el("cite", "Citation", Some("Citation"), Itself),
        el("dfn", "Definition", Some("Definition"), Itself),
        el("abbr", "Abbreviation", Some("Abbr."), Itself),
        el("acronym", "Acronym", Some("ACRONYM"), Itself),
        el("q", "Inline Quote", Some("Quotation"), Itself),
        el("time", "Time", Some("12:00"), Itself),
        el("big", "Big", Some("Big text"), Itself),
        el("bdo", "BiDi Override", Some("BiDi override"), Itself),
        el("bdi", "BiDi Isolate", Some("BiDi isolate"), Itself),
        el("wbr", "Word Break Opportunity", None, NoVisual("a line-break opportunity inside a word: no box of its own")),
        el("ruby", "Ruby Annotation", None, BuiltinPreview::Example { attrs: &[], text: "Ruby", children: RUBY_TEXT }),
        el("rt", "Ruby Text", Some("annotation"), Itself),
        el("rtc", "Ruby Text Container", None, BuiltinPreview::holding(RUBY_TEXT)),
        el("rp", "Ruby Parenthesis", Some("("), Itself),
        el("data", "Data", Some("Data"), Itself),
        // Forms: a raw control becomes its widget in the window, and in the preview
        el("form", "Form", None, BuiltinPreview::holding(FORM_FIELDS)),
        el("fieldset", "Field Set", None, BuiltinPreview::Example { attrs: PREVIEW_BOX, text: "", children: FIELDSET_FIELDS }),
        el("legend", "Legend", Some("Legend"), Itself),
        el("label", "Label", Some("Label"), Itself),
        el("input", "Input", None, BuiltinPreview::with(&[("placeholder", "Input")])),
        el("button", "Button", Some("Button text"), Itself),
        el("select", "Select", None, BuiltinPreview::holding(SELECT_OPTIONS)),
        el("optgroup", "Option Group", None, NoVisual("a heading over options: shows inside a <select>")),
        el("option", "Option", Some("Option"), NoVisual("shows inside a <select>")),
        el("textarea", "Text Area", Some(""), BuiltinPreview::with(&[("placeholder", "Text area")])),
        el("output", "Output", Some("Output"), Itself),
        el("progress", "Progress", None, NoVisual("not drawn by azul yet (the ProgressBar widget is)")),
        el("meter", "Meter", None, NoVisual("not drawn by azul yet")),
        el("datalist", "Data List", None, NoVisual("the suggestions of an <input list>: never drawn itself")),
        // Embedded content: each shows what its source names
        el("canvas", "Canvas", None, NoVisual("shows what the app draws into it")),
        el("object", "Object", None, NoVisual("shows the resource its data attribute names")),
        el("param", "Parameter", None, NoVisual("a parameter of its <object>")),
        el("embed", "Embed", None, NoVisual("shows the resource its src names")),
        el("audio", "Audio", None, NoVisual("plays the audio its src names")),
        el("video", "Video", None, NoVisual("plays the video its src names")),
        el("source", "Source", None, NoVisual("a source of its <audio> / <video>")),
        el("track", "Track", None, NoVisual("a text track of its <video>")),
        el("map", "Image Map", None, NoVisual("the clickable regions of an image: no box of its own")),
        el("area", "Map Area", None, NoVisual("a clickable region of an image map")),
        el("svg", "SVG", None, BuiltinPreview::Example { attrs: &[("width", "48"), ("height", "24"), ("viewBox", "0 0 48 24")], text: "", children: SVG_SHAPES }),
        // Metadata
        el("meta", "Meta", None, NoVisual("document metadata: it has no box")),
        el("link", "Link (Resource)", None, NoVisual("links a resource to the document: it has no box")),
        el("script", "Script", Some(""), NoVisual("code, not content")),
        el("style", "Style", Some(""), NoVisual("a stylesheet, not content")),
        el("base", "Base URL", None, NoVisual("document metadata: it has no box")),
    ]
};

/// The builtin element `tag`, if it is one.
fn builtin_element(tag: &str) -> Option<&'static BuiltinElement> {
    BUILTIN_ELEMENTS.iter().find(|e| e.tag == tag)
}

/// Why the builtin element `tag` has no preview (its palette card says "no
/// visual" with this); `None` for one that shows something, and for a tag
/// that is not a builtin element.
#[must_use]
pub fn builtin_no_visual(tag: &str) -> Option<&'static str> {
    match builtin_element(tag)?.preview {
        BuiltinPreview::NoVisual(why) => Some(why),
        BuiltinPreview::Itself | BuiltinPreview::Example { .. } => None,
    }
}

/// `<tag attrs>text children</tag>` as parsed markup.
fn preview_xml(tag: &str, attrs: &[(&str, &str)], text: &str, children: &[PreviewNode]) -> XmlNode {
    let mut kids: Vec<XmlNodeChild> = Vec::new();
    if !text.is_empty() {
        kids.push(XmlNodeChild::Text(AzString::from(text)));
    }
    kids.extend(children.iter().map(|c| XmlNodeChild::Element(c.markup())));
    XmlNode {
        node_type: XmlTagName::from(tag),
        attributes: XmlAttributeMap::from(StringPairVec::from_vec(
            attrs
                .iter()
                .map(|(k, v)| AzStringPair {
                    key: AzString::from(*k),
                    value: AzString::from(*v),
                })
                .collect::<Vec<_>>(),
        )),
        children: kids.into(),
    }
}

/// What a builtin element's PREVIEW shows with `data` (its palette card, the
/// Components view).
///
/// The element with its `text` (the data model's, else its example's) plus the
/// example attributes and children its entry in the `BUILTIN_ELEMENTS` table
/// configures - which a drop does not insert.
///
/// Unstyled on purpose: a raw `<input>` / `<select>` becomes its widget only
/// where the widgets are, and azul-layout resolves them for the preview as it
/// does for every document it mounts.
#[must_use]
pub fn builtin_preview_dom(tag: &str, data: &ComponentDataModel) -> Dom {
    builtin_dom(tag, data, true)
}

/// A builtin element with `data`'s text and - with `example` - its preview
/// example. Built by the XML loader's own path (`xml_node_to_dom_fast`: the
/// one attribute table), so an example attribute sets exactly what markup
/// would. An unknown tag is the node `tag_to_node_type` makes, with its text.
fn builtin_dom(tag: &str, data: &ComponentDataModel, example: bool) -> Dom {
    let (attrs, example_text, children) = builtin_element(tag)
        .filter(|_| example)
        .map_or(BuiltinPreview::Itself, |e| e.preview)
        .example();
    let own = data
        .get_default_string("text")
        .map(|t| prepare_string(t.as_str()))
        .unwrap_or_default();
    let text = if own.is_empty() {
        example_text.to_string()
    } else {
        own
    };
    // The component's ARGUMENTS become the element's attributes, so the
    // loader's own path lands them exactly as it lands markup's
    // (`apply_builtin_args_from_attributes`, the attribute table, the
    // element's own reading): the example's first, the arguments over them.
    let arg_strings = argument_attributes(data, &builtin_data_model(tag));
    let mut all_attrs: Vec<(&str, &str)> = attrs.to_vec();
    for (k, v) in &arg_strings {
        all_attrs.retain(|(ek, _)| !ek.eq_ignore_ascii_case(k));
        all_attrs.push((k.as_str(), v.as_str()));
    }
    let node = preview_xml(tag, &all_attrs, &text, children);
    xml_node_to_dom_fast(&node, &ComponentMap::default(), false, None, 0).unwrap_or_else(|_| {
        let bare = Dom::create_node(tag_to_node_type(tag));
        if text.is_empty() {
            bare
        } else {
            bare.with_children(
                alloc::vec![Dom::create_text_do_not_use_without_block_level_wrapper(
                    text
                )]
                .into(),
            )
        }
    })
}

/// A component's arguments as attribute strings, for the render fn of a
/// builtin element: every field but `text` that the caller SET (its value
/// differs from the element's declared default in `defaults`) to a string, a
/// bool or a number - a bool as HTML does: present when `true`, absent when
/// `false`. Empty strings and unset fields are left out.
fn argument_attributes(
    data: &ComponentDataModel,
    defaults: &[ComponentDataField],
) -> Vec<(String, String)> {
    data.fields
        .as_ref()
        .iter()
        .filter(|f| f.name.as_str() != "text")
        .filter(|f| {
            !defaults
                .iter()
                .any(|d| d.name.as_str() == f.name.as_str() && d.default_value == f.default_value)
        })
        .filter_map(|f| {
            let OptionComponentDefaultValue::Some(v) = &f.default_value else {
                return None;
            };
            let value = match v {
                ComponentDefaultValue::String(s) if !s.as_str().is_empty() => {
                    s.as_str().to_string()
                }
                ComponentDefaultValue::Bool(true) => String::new(),
                ComponentDefaultValue::I32(n) => n.to_string(),
                ComponentDefaultValue::I64(n) => n.to_string(),
                ComponentDefaultValue::U32(n) => n.to_string(),
                ComponentDefaultValue::U64(n) => n.to_string(),
                ComponentDefaultValue::Usize(n) => n.to_string(),
                ComponentDefaultValue::F32(n) => n.to_string(),
                ComponentDefaultValue::F64(n) => n.to_string(),
                _ => return None,
            };
            Some((f.name.as_str().to_string(), value))
        })
        .collect()
}

/// Register the built-in components: one per HTML element of the
/// `BUILTIN_ELEMENTS` table, then the structural `if` / `for` / `map`.
///
/// This is an `extern "C"` function pointer compatible with
/// `RegisterComponentLibraryFnType`, so it can be passed directly to
/// `AppConfig::add_component_library()`.
///
/// Called once during `AppConfig::create()` — the framework dogfoods
/// its own component registration system for builtins.
#[must_use]
pub extern "C" fn register_builtin_components() -> ComponentLibrary {
    ComponentLibrary {
        name: AzString::from_const_str("builtin"),
        version: AzString::from_const_str("1.0.0"),
        description: AzString::from_const_str("Built-in HTML elements"),
        exportable: false,
        modifiable: false,
        data_models: Vec::new().into(),
        enum_models: Vec::new().into(),
        components: BUILTIN_ELEMENTS
            .iter()
            .map(|e| builtin_component_def(e.tag, e.display_name, e.text, ""))
            // Structural control-flow builtins (F1-F3)
            .chain([
                builtin_if_component(),
                builtin_for_component(),
                builtin_map_component(),
            ])
            .collect::<Vec<_>>()
            .into(),
    }
}

// ============================================================================
// End new component system types
// ============================================================================

/// Wrapper for the XML parser - necessary to easily create a Dom from
/// XML without putting an XML solver into `azul-core`.
#[derive(Debug, Default)]
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
    ///
    /// # Panics
    ///
    /// Panics if the rendered DOM does not equal `other` (this is a test-only
    /// assertion helper).
    #[cfg(test)]
    pub fn assert_eq(self, other: StyledDom) {
        let mut body = Dom::create_body();
        let mut fixed = StyledDom::create(&mut body, Css::empty());
        fixed.append_child(other);
        assert!(
            !(self.parsed_dom != fixed),
            "\r\nExpected DOM did not match:\r\n\r\nexpected: ----------\r\n{}\r\ngot: \
             ----------\r\n{}\r\n",
            self.parsed_dom.get_html_string("", "", true),
            fixed.get_html_string("", "", true)
        );
    }

    #[must_use]
    pub fn into_styled_dom(self) -> StyledDom {
        self.into()
    }
}

impl From<DomXml> for StyledDom {
    fn from(val: DomXml) -> Self {
        val.parsed_dom
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
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s.as_str()),
            Self::Element(_) => None,
        }
    }

    /// Get the element if this is an element node
    #[must_use]
    pub const fn as_element(&self) -> Option<&XmlNode> {
        match self {
            Self::Text(_) => None,
            Self::Element(node) => Some(node),
        }
    }

    /// Get the element mutably if this is an element node
    pub const fn as_element_mut(&mut self) -> Option<&mut XmlNode> {
        match self {
            Self::Text(_) => None,
            Self::Element(node) => Some(node),
        }
    }
}

impl_vec!(
    XmlNodeChild,
    XmlNodeChildVec,
    XmlNodeChildVecDestructor,
    XmlNodeChildVecDestructorType,
    XmlNodeChildVecSlice,
    OptionXmlNodeChild
);
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
        Self {
            node_type: node_type.into(),
            ..Default::default()
        }
    }
    #[must_use]
    pub fn with_children(mut self, v: Vec<XmlNodeChild>) -> Self {
        Self {
            children: v.into(),
            ..self
        }
    }

    /// Get all text content concatenated from direct children
    #[must_use]
    pub fn get_text_content(&self) -> String {
        self.children
            .as_ref()
            .iter()
            .filter_map(|child| child.as_text())
            .collect::<Vec<_>>()
            .join("")
    }

    /// Check if this node has only text children (no element children)
    #[must_use]
    pub fn has_only_text_children(&self) -> bool {
        self.children
            .as_ref()
            .iter()
            .all(|child| matches!(child, XmlNodeChild::Text(_)))
    }
}

impl_vec!(
    XmlNode,
    XmlNodeVec,
    XmlNodeVecDestructor,
    XmlNodeVecDestructorType,
    XmlNodeVecSlice,
    OptionXmlNode
);
impl_vec_mut!(XmlNode, XmlNodeVec);
impl_vec_debug!(XmlNode, XmlNodeVec);
impl_vec_partialeq!(XmlNode, XmlNodeVec);
impl_vec_eq!(XmlNode, XmlNodeVec);
impl_vec_partialord!(XmlNode, XmlNodeVec);
impl_vec_ord!(XmlNode, XmlNodeVec);
impl_vec_hash!(XmlNode, XmlNodeVec);
impl_vec_clone!(XmlNode, XmlNodeVec, XmlNodeVecDestructor);

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
        Self::Dom(RenderDomError::Component(e))
    }
}

impl From<CssParseErrorOwned> for CompileError {
    fn from(e: CssParseErrorOwned) -> Self {
        Self::Css(e)
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::CompileError::{Css, Dom, Xml};
        match self {
            Dom(d) => write!(f, "{d}"),
            Xml(s) => write!(f, "{s}"),
            Css(s) => write!(f, "{}", s.to_shared()),
        }
    }
}

impl From<RenderDomError> for CompileError {
    fn from(e: RenderDomError) -> Self {
        Self::Dom(e)
    }
}

impl From<DomXmlParseError> for CompileError {
    fn from(e: DomXmlParseError) -> Self {
        Self::Xml(e)
    }
}

/// Wrapper for `UselessFunctionArgument` error data.
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
    /// `UnknownComponent(component_name)`
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

/// Wrapper for `MissingType` error data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct MissingTypeError {
    pub arg_pos: usize,
    pub arg_name: AzString,
}

/// Wrapper for `WhiteSpaceInComponentName` error data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct WhiteSpaceInComponentNameError {
    pub arg_pos: usize,
    pub arg_name: AzString,
}

/// Wrapper for `WhiteSpaceInComponentType` error data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct WhiteSpaceInComponentTypeError {
    pub arg_pos: usize,
    pub arg_name: AzString,
    pub arg_type: AzString,
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ComponentParseError {
    /// Given `XmlNode` is not a `<component />` node.
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

impl fmt::Display for DomXmlParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::DomXmlParseError::{
            Component, Css, MalformedHierarchy, MultipleBodyNodes, MultipleHtmlRootNodes,
            NoBodyInHtml, NoHtmlNode, RenderDom, Xml,
        };
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
            Xml(e) => write!(f, "Error parsing XML: {e}"),
            MalformedHierarchy(e) => write!(
                f,
                "Invalid </{}> tag: expected </{}>",
                e.got.as_str(),
                e.expected.as_str()
            ),
            RenderDom(e) => write!(f, "Error rendering DOM: {e}"),
            Component(c) => write!(f, "Error parsing component in <head> node:\r\n{c}"),
            Css(c) => write!(f, "Error parsing CSS in <head> node:\r\n{}", c.to_shared()),
        }
    }
}

impl fmt::Display for ComponentParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::ComponentParseError::{
            CssError, MissingName, MissingType, NotAComponent, UnnamedComponent,
            WhiteSpaceInComponentName, WhiteSpaceInComponentType,
        };
        match self {
            NotAComponent => write!(f, "Expected <component/> node, found no such node"),
            UnnamedComponent => write!(
                f,
                "Found <component/> tag with out a \"name\" attribute, component must have a name"
            ),
            MissingName(arg_pos) => write!(
                f,
                "Argument at position {arg_pos} is either empty or has no name"
            ),
            MissingType(e) => write!(
                f,
                "Argument \"{}\" at position {} doesn't have a `: type`",
                e.arg_name, e.arg_pos
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::ComponentError::{UnknownComponent, UselessFunctionArgument};
        match self {
            UselessFunctionArgument(e) => {
                write!(
                    f,
                    "Useless component argument \"{}\": \"{}\" - available args are: {:#?}",
                    e.component_name, e.argument_name, e.valid_args
                )
            }
            UnknownComponent(name) => write!(f, "Unknown component: \"{name}\""),
        }
    }
}

impl fmt::Display for RenderDomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::RenderDomError::{Component, CssError};
        match self {
            Component(c) => write!(f, "{c}"),
            CssError(e) => write!(f, "Error parsing CSS in component: {}", e.to_shared()),
        }
    }
}

/// Find the one and only `<body>` node, return error if
/// there is no app node or there are multiple app nodes
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
/// # Errors
///
/// Returns an error if the document has no `<html>` root node.
/// Every `<head><style>` block's text, concatenated in document order.
///
/// ALL of them, not the first: a document may state its stylesheet in several
/// blocks (a reset, then the page's own rules), and browsers apply each in
/// turn. Reading only the first silently dropped everything after it - and it
/// is silent in the worst way, because the page still renders, just with some
/// rules missing and nothing to say which.
pub(crate) fn head_style_text(html_node: &XmlNode) -> String {
    let Some(head) = find_node_by_type(html_node.children.as_ref(), "head") else {
        return String::new();
    };
    let mut out = String::new();
    for child in head.children.as_ref() {
        let XmlNodeChild::Element(element) = child else {
            continue;
        };
        if !element.node_type.as_str().eq_ignore_ascii_case("style") {
            continue;
        }
        let text = element.get_text_content();
        if !text.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&text);
        }
    }
    out
}

/// The document's single root `<html>` node.
///
/// # Errors
///
/// Returns [`DomXmlParseError::NoHtmlNode`] when the document has no root
/// `<html>` node, and [`DomXmlParseError::MultipleHtmlRootNodes`] when it has
/// more than one.
// `DomXmlParseError` is the crate's public XML error type, shared with the C
// ABI; boxing it to shrink the Err variant would change that surface.
#[allow(clippy::result_large_err)]
pub fn get_html_node(
    root_nodes: &[XmlNodeChild],
) -> Result<alloc::borrow::Cow<'_, XmlNode>, DomXmlParseError> {
    use alloc::borrow::Cow;

    let mut html_node_iterator = root_nodes.iter().filter_map(|child| {
        if let XmlNodeChild::Element(node) = child {
            // HTML element names are case-insensitive (ASCII). NOT normalize_casing:
            // that inserts '_' before each uppercase letter for component-name
            // canonicalisation, so "HTML" would become "h_t_m_l" and never match.
            if node.node_type.as_str().eq_ignore_ascii_case("html") {
                Some(node)
            } else {
                None
            }
        } else {
            None
        }
    });

    if let Some(html_node) = html_node_iterator.next() {
        return if html_node_iterator.next().is_some() {
            Err(DomXmlParseError::MultipleHtmlRootNodes)
        } else {
            Ok(Cow::Borrowed(html_node))
        };
    }

    // NO <html> ROOT: synthesise one, the way a browser does.
    //
    // Requiring the wrapper made a perfectly good fragment - `<svg>…</svg>`,
    // `<div>hi</div>`, an icon file straight off the disk - parse into the
    // TEXT "No <html> node found as the root of the file", which then lays
    // out and paints like any other text. The failure was silent and looked
    // exactly like a rendering bug: a caller measuring pixels saw an error
    // message it never asked for and no sign of what happened.
    //
    // A root `<body>`/`<head>` is ADOPTED rather than nested (again like a
    // browser): wrapping `<body>` inside a fresh `<body>` would give the
    // document two, which is its own error.
    let has_structural_root = root_nodes.iter().any(|child| match child {
        XmlNodeChild::Element(node) => {
            let tag = node.node_type.as_str();
            tag.eq_ignore_ascii_case("body") || tag.eq_ignore_ascii_case("head")
        }
        XmlNodeChild::Text(_) => false,
    });
    if has_structural_root {
        return Ok(Cow::Owned(
            XmlNode::create("html").with_children(root_nodes.to_vec()),
        ));
    }

    // METADATA GOES TO THE HEAD, content to the body - the same split a
    // browser makes. A `<style>` left in the body is not looked at by
    // `str_to_dom_unstyled` (it reads `<head><style>`), so a fragment that
    // brought its own stylesheet would render unstyled and give no hint why.
    let (head_children, body_children): (Vec<_>, Vec<_>) =
        root_nodes.iter().cloned().partition(|child| match child {
            XmlNodeChild::Element(node) => {
                let tag = node.node_type.as_str();
                tag.eq_ignore_ascii_case("style")
                    || tag.eq_ignore_ascii_case("link")
                    || tag.eq_ignore_ascii_case("meta")
                    || tag.eq_ignore_ascii_case("title")
                    || tag.eq_ignore_ascii_case("base")
            }
            XmlNodeChild::Text(_) => false,
        });

    let mut html_children = Vec::new();
    if !head_children.is_empty() {
        html_children.push(XmlNodeChild::Element(
            XmlNode::create("head").with_children(head_children),
        ));
    }
    html_children.push(XmlNodeChild::Element(
        XmlNode::create("body").with_children(body_children),
    ));
    Ok(Cow::Owned(
        XmlNode::create("html").with_children(html_children),
    ))
}

/// Find the one and only `<body>` node, return error if
/// there is no app node or there are multiple app nodes
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
/// # Errors
///
/// Returns an error if the document has no `<body>` node.
pub fn get_body_node(root_nodes: &[XmlNodeChild]) -> Result<&XmlNode, DomXmlParseError> {
    fn find_body_recursive(nodes: &[XmlNodeChild], depth: usize) -> Option<&XmlNode> {
        // AUDIT 2026-07-08: bound recursion depth to avoid a stack overflow on
        // pathologically deep markup while hunting for the <body> element.
        if depth > MAX_XML_NESTING_DEPTH {
            return None;
        }
        for child in nodes {
            if let XmlNodeChild::Element(node) = child {
                // case-insensitive ASCII tag match; see get_html_node.
                if node.node_type.as_str().eq_ignore_ascii_case("body") {
                    return Some(node);
                }
                // Recurse into children
                if let Some(found) = find_body_recursive(node.children.as_ref(), depth + 1) {
                    return Some(found);
                }
            }
        }
        None
    }

    // First try to find body as a direct child (proper HTML structure)
    let direct_body = root_nodes.iter().find_map(|child| {
        if let XmlNodeChild::Element(node) = child {
            // case-insensitive ASCII tag match; see get_html_node.
            if node.node_type.as_str().eq_ignore_ascii_case("body") {
                Some(node)
            } else {
                None
            }
        } else {
            None
        }
    });

    if let Some(body) = direct_body {
        return Ok(body);
    }

    // If not found as direct child, search recursively (for malformed HTML like example.com)
    // where <body> might be nested inside <head> due to missing </head> tag
    find_body_recursive(root_nodes, 0).ok_or(DomXmlParseError::NoBodyInHtml)
}

/// Searches in the the `root_nodes` for a `node_type`, convenience function in order to
/// for example find the first <blah /> node in all these nodes.
/// This function searches recursively through the entire tree.
fn find_node_by_type<'a>(root_nodes: &'a [XmlNodeChild], node_type: &str) -> Option<&'a XmlNode> {
    // First check direct children
    for child in root_nodes {
        if let XmlNodeChild::Element(node) = child {
            // case-insensitive ASCII tag match; see get_html_node.
            if node.node_type.as_str().eq_ignore_ascii_case(node_type) {
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

#[must_use]
pub fn find_attribute<'a>(node: &'a XmlNode, attribute: &str) -> Option<&'a AzString> {
    node.attributes
        .iter()
        .find(|n| normalize_casing(n.key.as_str()).as_str() == attribute)
        .map(|s| &s.value)
}

/// Normalizes input such as `abcDef`, `AbcDef`, `abc-def` to the normalized form of `abc_def`
#[must_use]
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
    let Some(item) = hierarchy.pop() else {
        return Some(root_node);
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
    let Some(cur_item) = hierarchy.pop() else {
        return Some(root_node);
    };
    let child = root_node.children.as_mut().get_mut(cur_item)?;
    match child {
        XmlNodeChild::Element(node) => get_item_internal(hierarchy, node),
        XmlNodeChild::Text(_) => None, // Can't traverse into text nodes
    }
}

/// Parses an XML string and returns a `StyledDom` with the components instantiated in the
/// `<app></app>`
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
/// # Errors
///
/// Returns an error if the XML cannot be parsed into a DOM (malformed markup or an unknown
/// component).
pub fn str_to_dom<'a>(
    root_nodes: &'a [XmlNodeChild],
    component_map: &'a ComponentMap,
    max_width: Option<f32>,
) -> Result<StyledDom, DomXmlParseError> {
    str_to_dom_loading_fonts(root_nodes, component_map, max_width, None)
}

/// [`str_to_dom`], with the source of the fonts an `<svg>`'s `@font-face`s
/// embed ([`element::FontSourceFn`]; without one they are not loaded).
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
/// # Errors
///
/// Returns an error if the XML cannot be parsed into a DOM (malformed markup or an unknown
/// component).
pub fn str_to_dom_loading_fonts<'a>(
    root_nodes: &'a [XmlNodeChild],
    component_map: &'a ComponentMap,
    max_width: Option<f32>,
    font_source: Option<element::FontSourceFn>,
) -> Result<StyledDom, DomXmlParseError> {
    // Delegate to the fast path (Dom::Fast / CompactDom arena).
    str_to_dom_fast(root_nodes, component_map, max_width, font_source)
}

/// Parse XML to `StyledDom` via arena-based `FastDom` (no tree intermediary).
///
/// **Note**: `str_to_dom()` now delegates to this function, so you can use
/// either one. This function is kept for backward compatibility.
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would
                                   // break the C ABI/api.json
fn str_to_dom_fast<'a>(
    root_nodes: &'a [XmlNodeChild],
    component_map: &'a ComponentMap,
    max_width: Option<f32>,
    font_source: Option<element::FontSourceFn>,
) -> Result<StyledDom, DomXmlParseError> {
    let html_node = get_html_node(root_nodes)?;
    let body_node = get_body_node(html_node.children.as_ref())?;

    let style_text = head_style_text(&html_node);
    let global_style = if style_text.is_empty() {
        None
    } else {
        Some(Css::from_string(style_text.into()))
    };

    render_dom_from_body_node_fast(
        &html_node,
        body_node,
        global_style,
        component_map,
        max_width,
        font_source,
    )
    .map_err(Into::into)
}

/// The root `Html` node of a loaded document: the `<html>` element's own
/// attributes (its inline `style`, `lang`, `dir`, ids and classes) landed as
/// every other element's are ([`element::render_element`]). Both loaders
/// built a bare `Html` node and dropped them (WPT8 found (c)).
fn html_root_node_data(html_node: &XmlNode) -> NodeData {
    let pairs = attribute_pairs(html_node);
    element::render_element(
        &ComponentMap::default(),
        &element::Element {
            tag: "html",
            attributes: &pairs,
            scope: element::ElementScope::default(),
            font_faces: &[],
        },
        &mut element::Landing {
            css_key_map: None,
            intern: &mut |s: &str| AzString::from(s),
        },
    )
}

/// Parses XML nodes and returns a `Dom` with CSS stylesheets attached (but not applied).
///
/// Unlike `str_to_dom` which returns a fully styled `StyledDom`, this function
/// returns an unstyled `Dom` whose `css` field carries the parsed `<style>` rules.
/// The layout framework will apply the CSS during the cascade pass.
///
/// This is the correct function for building a `Dom` from XML in layout callbacks
/// (which must return `Dom`, not `StyledDom`).
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
/// # Errors
///
/// Returns an error if the XML cannot be parsed into a DOM (malformed markup or an unknown
/// component).
pub fn str_to_dom_unstyled<'a>(
    root_nodes: &'a [XmlNodeChild],
    component_map: &'a ComponentMap,
) -> Result<Dom, DomXmlParseError> {
    str_to_dom_unstyled_loading_fonts(root_nodes, component_map, None)
}

/// [`str_to_dom_unstyled`], with the source of the fonts an `<svg>`'s
/// `@font-face`s embed ([`element::FontSourceFn`]; without one they are not
/// loaded).
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
/// # Errors
///
/// Returns an error if the XML cannot be parsed into a DOM (malformed markup or an unknown
/// component).
pub fn str_to_dom_unstyled_loading_fonts<'a>(
    root_nodes: &'a [XmlNodeChild],
    component_map: &'a ComponentMap,
    font_source: Option<element::FontSourceFn>,
) -> Result<Dom, DomXmlParseError> {
    let html_node = get_html_node(root_nodes)?;
    let body_node = get_body_node(html_node.children.as_ref())?;

    let style_text = head_style_text(&html_node);
    let global_style = if style_text.is_empty() {
        None
    } else {
        Some(Css::from_string(style_text.into()))
    };

    // Build the DOM tree from the body node
    let body_dom = xml_node_to_dom_fast(body_node, component_map, false, font_source, 0)
        .map_err(DomXmlParseError::from)?;

    // Wrap in proper HTML structure (NodeType is imported at module top)
    let root_node_type = body_dom.root.node_type.clone();
    // The root carries the `<html>` element's own attributes.
    let html_root = || {
        let mut html = Dom::create_html();
        html.root = html_root_node_data(&html_node);
        html
    };

    let mut full_dom = match root_node_type {
        NodeType::Html => body_dom,
        NodeType::Body => html_root().with_child(body_dom),
        _ => {
            let body_wrapper = Dom::create_body().with_child(body_dom);
            html_root().with_child(body_wrapper)
        }
    };

    // Attach CSS to the Dom's css field instead of applying it immediately
    if let Some(css) = global_style {
        full_dom.css = alloc::vec![css].into();
    }

    Ok(full_dom)
}

/// Parse an SVG numeric attribute value to f32.
///
/// STRICT: a geometry attribute (`cx`, `r`, `x1`, ...) is a USER UNIT, and
/// `cx="10px"` is not valid SVG. The `<svg>` element's own `width`/`height`
/// are CSS lengths and a different thing entirely - see [`parse_svg_length`].
fn parse_svg_float(attr: Option<&str>) -> Option<f32> {
    attr?.trim().parse::<f32>().ok()
}

/// Parse the `<svg>` element's own `width`/`height`, which - unlike the
/// geometry attributes - are CSS LENGTHS.
///
/// A bare `px` is accepted because `width="16px"` is as common in the wild as
/// `width="16"`. A relative unit (`%`, `em`) is REJECTED rather than guessed
/// at: the caller then falls back to the viewBox, which is a real answer,
/// instead of resolving a percentage against nothing.
fn parse_svg_length(attr: Option<&str>) -> Option<f32> {
    let raw = attr?.trim();
    let number = raw.strip_suffix("px").unwrap_or(raw).trim();
    number.parse::<f32>().ok()
}

/// Parse an SVG `viewBox` into `(min_x, min_y, width, height)`.
///
/// Space- or comma-separated, per the spec; exactly four numbers, because
/// three or five is a malformed viewBox and silently taking the first four
/// would place the art somewhere nobody asked for.
fn parse_svg_view_box(value: &str) -> Option<(f32, f32, f32, f32)> {
    let nums: Vec<f32> = value
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|s| !s.is_empty())
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    match nums[..] {
        [min_x, min_y, width, height] if width > 0.0 && height > 0.0 => {
            Some((min_x, min_y, width, height))
        }
        _ => None,
    }
}

/// Parse an SVG `points` attribute (used by `<polygon>` and `<polyline>`).
fn parse_svg_points(pts: &str, close: bool) -> Option<crate::svg::SvgMultiPolygon> {
    let nums: Vec<f32> = pts
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f32>().ok())
        .collect();
    if nums.len() < 4 || !nums.len().is_multiple_of(2) {
        return None;
    }
    let mut elements = Vec::new();
    let points: Vec<azul_css::props::basic::SvgPoint> = nums
        .chunks_exact(2)
        .map(|c| azul_css::props::basic::SvgPoint { x: c[0], y: c[1] })
        .collect();
    for w in points.windows(2) {
        elements.push(crate::svg::SvgPathElement::Line(crate::svg::SvgLine::new(
            w[0], w[1],
        )));
    }
    if close && points.len() >= 2 {
        let first = points[0];
        let last = *points.last().unwrap();
        if (first.x - last.x).abs() > 0.001 || (first.y - last.y).abs() > 0.001 {
            elements.push(crate::svg::SvgPathElement::Line(crate::svg::SvgLine::new(
                last, first,
            )));
        }
    }
    Some(crate::svg::SvgMultiPolygon {
        rings: crate::svg::SvgPathVec::from_vec(vec![crate::svg::SvgPath {
            items: crate::svg::SvgPathElementVec::from_vec(elements),
        }]),
    })
}

/// The geometry of an SVG shape element (`path`, `circle`, `rect`,
/// `ellipse`, `line`, `polygon`, `polyline`) in its user units: what its
/// node's `SvgNodeData::Path` clips its box to. `None` for an empty or
/// degenerate shape (a circle of no radius, a path that does not parse).
fn svg_shape_geometry(element: &element::Element<'_>) -> Option<crate::svg::SvgMultiPolygon> {
    let tag = element.tag;
    match tag {
        "path" => element
            .attribute("d")
            .and_then(|d| crate::path_parser::parse_svg_path_d(d).ok()),
        "circle" => {
            let cx = parse_svg_float(element.attribute("cx")).unwrap_or(0.0);
            let cy = parse_svg_float(element.attribute("cy")).unwrap_or(0.0);
            let r = parse_svg_float(element.attribute("r")).unwrap_or(0.0);
            if r > 0.0 {
                Some(crate::svg::SvgMultiPolygon {
                    rings: crate::svg::SvgPathVec::from_vec(vec![
                        crate::path_parser::svg_circle_to_paths(cx, cy, r),
                    ]),
                })
            } else {
                None
            }
        }
        "rect" => {
            let x = parse_svg_float(element.attribute("x")).unwrap_or(0.0);
            let y = parse_svg_float(element.attribute("y")).unwrap_or(0.0);
            let w = parse_svg_float(element.attribute("width")).unwrap_or(0.0);
            let h = parse_svg_float(element.attribute("height")).unwrap_or(0.0);
            let rx = parse_svg_float(element.attribute("rx")).unwrap_or(0.0);
            let ry = parse_svg_float(element.attribute("ry")).unwrap_or(rx);
            if w > 0.0 && h > 0.0 {
                Some(crate::svg::SvgMultiPolygon {
                    rings: crate::svg::SvgPathVec::from_vec(vec![
                        crate::path_parser::svg_rect_to_path(x, y, w, h, rx, ry),
                    ]),
                })
            } else {
                None
            }
        }
        "ellipse" => {
            let cx = parse_svg_float(element.attribute("cx")).unwrap_or(0.0);
            let cy = parse_svg_float(element.attribute("cy")).unwrap_or(0.0);
            let rx = parse_svg_float(element.attribute("rx")).unwrap_or(0.0);
            let ry = parse_svg_float(element.attribute("ry")).unwrap_or(0.0);
            if rx > 0.0 && ry > 0.0 {
                // Approximate ellipse with 4 cubic beziers (using rx for x-kappa, ry for
                // y-kappa)
                use azul_css::props::basic::{SvgCubicCurve, SvgPoint};
                const KAPPA: f32 = 0.552_284_8;
                let kx = rx * KAPPA;
                let ky = ry * KAPPA;
                let elements = vec![
                    crate::svg::SvgPathElement::CubicCurve(SvgCubicCurve {
                        start: SvgPoint { x: cx, y: cy - ry },
                        ctrl_1: SvgPoint {
                            x: cx + kx,
                            y: cy - ry,
                        },
                        ctrl_2: SvgPoint {
                            x: cx + rx,
                            y: cy - ky,
                        },
                        end: SvgPoint { x: cx + rx, y: cy },
                    }),
                    crate::svg::SvgPathElement::CubicCurve(SvgCubicCurve {
                        start: SvgPoint { x: cx + rx, y: cy },
                        ctrl_1: SvgPoint {
                            x: cx + rx,
                            y: cy + ky,
                        },
                        ctrl_2: SvgPoint {
                            x: cx + kx,
                            y: cy + ry,
                        },
                        end: SvgPoint { x: cx, y: cy + ry },
                    }),
                    crate::svg::SvgPathElement::CubicCurve(SvgCubicCurve {
                        start: SvgPoint { x: cx, y: cy + ry },
                        ctrl_1: SvgPoint {
                            x: cx - kx,
                            y: cy + ry,
                        },
                        ctrl_2: SvgPoint {
                            x: cx - rx,
                            y: cy + ky,
                        },
                        end: SvgPoint { x: cx - rx, y: cy },
                    }),
                    crate::svg::SvgPathElement::CubicCurve(SvgCubicCurve {
                        start: SvgPoint { x: cx - rx, y: cy },
                        ctrl_1: SvgPoint {
                            x: cx - rx,
                            y: cy - ky,
                        },
                        ctrl_2: SvgPoint {
                            x: cx - kx,
                            y: cy - ry,
                        },
                        end: SvgPoint { x: cx, y: cy - ry },
                    }),
                ];
                Some(crate::svg::SvgMultiPolygon {
                    rings: crate::svg::SvgPathVec::from_vec(vec![crate::svg::SvgPath {
                        items: crate::svg::SvgPathElementVec::from_vec(elements),
                    }]),
                })
            } else {
                None
            }
        }
        "line" => {
            let x1 = parse_svg_float(element.attribute("x1")).unwrap_or(0.0);
            let y1 = parse_svg_float(element.attribute("y1")).unwrap_or(0.0);
            let x2 = parse_svg_float(element.attribute("x2")).unwrap_or(0.0);
            let y2 = parse_svg_float(element.attribute("y2")).unwrap_or(0.0);
            Some(crate::svg::SvgMultiPolygon {
                rings: crate::svg::SvgPathVec::from_vec(vec![crate::svg::SvgPath {
                    items: crate::svg::SvgPathElementVec::from_vec(vec![
                        crate::svg::SvgPathElement::Line(crate::svg::SvgLine::new(
                            azul_css::props::basic::SvgPoint { x: x1, y: y1 },
                            azul_css::props::basic::SvgPoint { x: x2, y: y2 },
                        )),
                    ]),
                }]),
            })
        }
        "polygon" | "polyline" => element
            .attribute("points")
            .and_then(|pts| parse_svg_points(pts, tag == "polygon")),
        _ => None,
    }
}

/// Every `<style>` element's text in this subtree, in document order.
///
/// Depth-bounded for the same reason the DOM conversion is: this reads files
/// nothing in this build produced.
pub(crate) fn collect_style_text(node: &XmlNode, out: &mut Vec<String>, depth: usize) {
    if depth >= MAX_XML_NESTING_DEPTH {
        return;
    }
    for child in node.children.as_ref() {
        let XmlNodeChild::Element(element) = child else {
            continue;
        };
        if element.node_type.as_str().eq_ignore_ascii_case("style") {
            let text = element.get_text_content();
            if !text.is_empty() {
                out.push(text);
            }
        } else {
            collect_style_text(element, out, depth + 1);
        }
    }
}

/// Is this element one that DRAWS NOTHING, subtree and all?
///
/// Not "unknown" - unknown tags are ordinary boxes and stay `<div>`s. These
/// are elements that a renderer is DEFINED not to draw, so whatever they
/// contain is about the document rather than in it, and turning their text
/// into text nodes puts prose on screen.
///
/// Two kinds, and a real icon theme hands us both in every file:
///
///   * `<metadata>` (SVG 1.1 §5.10), which is where Inkscape parks an RDF block - and inside it
///     `<dc:format>image/svg+xml</dc:format>`, whose text drew across the window controls of a
///     client-side titlebar, clipped to 16px, as the letters `im`;
///   * anything in a FOREIGN NAMESPACE (SVG 1.1 §23.2), which is the rest of what Inkscape leaves
///     behind: `<sodipodi:namedview>`, `<inkscape:grid>`, `<rdf:RDF>`, `<cc:Work>`.
///
/// A prefix alone does not make an element foreign: a document that declares
/// the SVG or XHTML namespace may well write `<svg:path>`, which is a path.
///
/// `<style>` belongs to this family too but is handled separately at the call
/// site: its text is not nothing, it is a stylesheet, and it is lifted onto
/// the element that contains it.
///
/// Both are SVG's rules: the DOM builders ask this for the elements INSIDE an
/// `<svg>`. In an HTML document a foreign element (Word's `<o:p>`, Outlook's
/// `<st1:place>`) is an unknown element, which HTML renders inline with its
/// content ([`tag_to_node_type`] makes it a `<span>`).
#[must_use]
pub fn element_draws_nothing(raw_tag: &str, normalized_tag: &str) -> bool {
    normalized_tag == "metadata" || is_foreign_element(raw_tag)
}

/// Whether `tag` is an element of a foreign namespace: a prefix other than
/// `svg`, `html` and `xhtml` (`o:p`, `sodipodi:namedview`).
#[must_use]
pub fn is_foreign_element(tag: &str) -> bool {
    match tag.split_once(':') {
        Some((prefix, _)) => {
            let prefix = prefix.trim();
            !(prefix.eq_ignore_ascii_case("svg")
                || prefix.eq_ignore_ascii_case("html")
                || prefix.eq_ignore_ascii_case("xhtml"))
        }
        None => false,
    }
}

/// Convert an XML node tree into a `Dom` tree: [`walk_element`] into a
/// [`DomTreeSink`]. `RenderDomError` is large but is the crate's public XML
/// error type, shared with the C ABI, so it is not boxed here.
#[allow(clippy::result_large_err)]
fn xml_node_to_dom_fast<'a>(
    xml_node: &'a XmlNode,
    component_map: &'a ComponentMap,
    inside_svg: bool,
    font_source: Option<element::FontSourceFn>,
    depth: usize,
) -> Result<Dom, RenderDomError> {
    let mut sink = DomTreeSink::default();
    walk_element(
        xml_node,
        component_map,
        element::ElementScope { inside_svg },
        element::FontScope {
            source: font_source,
            faces: &[],
        },
        &mut sink,
        depth,
    )?;
    Ok(sink.finish())
}

/// Where a walk of the markup ([`walk_element`]) puts what it builds: a `Dom`
/// tree ([`DomTreeSink`]) or a `FastDom` arena ([`CompactDomBuilder`]) - ONE
/// walk for both.
trait DomSink {
    /// Open an element node; its children follow until [`Self::close`].
    fn open(&mut self, node: NodeData);
    /// A node without children (a text).
    fn leaf(&mut self, node: NodeData);
    /// A stylesheet for the OPEN node's subtree.
    fn scope_css(&mut self, css: Css);
    /// Close the open node.
    fn close(&mut self);
}

/// A [`DomSink`] that builds a `Dom` tree.
#[derive(Default)]
struct DomTreeSink {
    /// The open elements, innermost last.
    open: Vec<Dom>,
    /// The closed root.
    done: Option<Dom>,
}

impl DomTreeSink {
    fn finish(self) -> Dom {
        self.done.unwrap_or_else(Dom::create_div)
    }
}

impl DomSink for DomTreeSink {
    fn open(&mut self, node: NodeData) {
        let mut dom = Dom::create_div();
        dom.root = node;
        self.open.push(dom);
    }
    fn leaf(&mut self, node: NodeData) {
        self.open(node);
        self.close();
    }
    fn scope_css(&mut self, css: Css) {
        if let Some(dom) = self.open.last_mut() {
            dom.add_component_css(css);
        }
    }
    fn close(&mut self) {
        let Some(dom) = self.open.pop() else {
            return;
        };
        match self.open.last_mut() {
            Some(parent) => parent.add_child(dom),
            None => self.done = Some(dom),
        }
    }
}

impl DomSink for CompactDomBuilder {
    fn open(&mut self, node: NodeData) {
        self.open_node(node);
    }
    fn leaf(&mut self, node: NodeData) {
        self.add_leaf(node);
    }
    fn scope_css(&mut self, css: Css) {
        if let Some(&(open, _)) = self.stack.last() {
            self.add_css(open, css);
        }
    }
    fn close(&mut self) {
        self.close_node();
    }
}

/// An element's attributes as `(name, value)` pairs, in document order.
fn attribute_pairs(xml_node: &XmlNode) -> Vec<(&str, &str)> {
    xml_node
        .attributes
        .as_slice()
        .iter()
        .map(|pair| (pair.key.as_str(), pair.value.as_str()))
        .collect()
}

/// THE walk of core's XML -> DOM builders: `xml_node` instantiated by its
/// component ([`element::render_element`]), then its children - a
/// `data-l10n` key first, as a localizable text; a `<style>` as a stylesheet
/// of this element's subtree (an `<svg>` takes every sheet inside it); an
/// element that draws nothing not at all ([`element::child_role`]).
///
/// Recursion is bounded: at [`MAX_XML_NESTING_DEPTH`] the element is emitted
/// without its children rather than overflowing the native stack.
fn walk_element(
    xml_node: &XmlNode,
    component_map: &ComponentMap,
    scope: element::ElementScope,
    fonts: element::FontScope<'_>,
    sink: &mut dyn DomSink,
    depth: usize,
) -> Result<(), RenderDomError> {
    let tag = element::element_tag(component_map, xml_node.node_type.as_str());
    let pairs = attribute_pairs(xml_node);
    let node = element::render_element(
        component_map,
        &element::Element {
            tag: &tag,
            attributes: &pairs,
            scope,
            font_faces: fonts.faces,
        },
        &mut element::Landing {
            css_key_map: None,
            intern: &mut |s: &str| AzString::from(s),
        },
    );
    sink.open(node);
    if depth < MAX_XML_NESTING_DEPTH {
        if let Some(key) = element::l10n_key(&pairs) {
            sink.leaf(NodeData::create_text_do_not_use_without_block_level_wrapper(
                AzString::tr(key),
            ));
        }
        // An `<svg>`'s stylesheet is SVG-GLOBAL: nearly always written in
        // `<defs><style>`, and `<defs>` draws nothing - hung on the `<svg>`.
        // Its `@font-face`s are ITS fonts: two pages' `F1`s are two fonts.
        let mut svg_faces = Vec::new();
        if tag == "svg" {
            let mut texts = Vec::new();
            collect_style_text(xml_node, &mut texts, 0);
            for text in texts {
                svg_faces.extend(fonts.load(&text));
                sink.scope_css(Css::from_string(text.into()));
            }
        }
        let children_faces;
        let children_fonts = if svg_faces.is_empty() {
            fonts
        } else {
            children_faces = [fonts.faces, &svg_faces].concat();
            element::FontScope {
                faces: &children_faces,
                ..fonts
            }
        };
        let children_scope = scope.for_children_of(&tag);
        for child in xml_node.children.as_ref() {
            match child {
                XmlNodeChild::Element(child_node) => {
                    match element::child_role(children_scope, child_node.node_type.as_str()) {
                        element::ChildRole::Node => walk_element(
                            child_node,
                            component_map,
                            children_scope,
                            children_fonts,
                            sink,
                            depth + 1,
                        )?,
                        // Inside an `<svg>` it was taken above.
                        element::ChildRole::Stylesheet if tag != "svg" => {
                            let text = child_node.get_text_content();
                            if !text.is_empty() {
                                sink.scope_css(Css::from_string(text.into()));
                            }
                        }
                        element::ChildRole::Stylesheet | element::ChildRole::Nothing => {}
                    }
                }
                XmlNodeChild::Text(text) => {
                    sink.leaf(NodeData::create_text_do_not_use_without_block_level_wrapper(
                        AzString::from(text.as_str()),
                    ));
                }
            }
        }
    }
    sink.close();
    Ok(())
}

/// Builder for arena-based DOM construction (`FastDom`).
/// Builds two parallel Vecs (hierarchy + `node_data`) in a single DFS pass.
#[derive(Debug)]
pub struct CompactDomBuilder {
    hierarchy: Vec<crate::styled_dom::NodeHierarchyItem>,
    node_data: Vec<NodeData>,
    css: Vec<crate::dom::CssWithNodeId>,
    /// Stack of (`node_index`, `previous_child_index`) for open elements
    stack: Vec<(usize, Option<usize>)>,
}

impl Default for CompactDomBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CompactDomBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            hierarchy: Vec::new(),
            node_data: Vec::new(),
            css: Vec::new(),
            stack: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            hierarchy: Vec::with_capacity(cap),
            node_data: Vec::with_capacity(cap),
            css: Vec::new(),
            stack: Vec::new(),
        }
    }

    /// Open a new element node. Must be paired with `close_node()`.
    pub fn open_node(&mut self, node_data: NodeData) {
        use crate::{id::NodeId, styled_dom::NodeHierarchyItem};

        let idx = self.hierarchy.len();

        // Determine parent from stack
        let parent_raw = if let Some(&(parent_idx, _)) = self.stack.last() {
            NodeId::into_raw(&Some(NodeId::new(parent_idx)))
        } else {
            0 // No parent (root)
        };

        // Determine previous sibling from parent's last child tracking
        let prev_sibling_raw = if let Some(&(_, prev_child)) = self.stack.last() {
            prev_child.map_or(0, |pi| NodeId::into_raw(&Some(NodeId::new(pi))))
        } else {
            0
        };

        // If there's a previous sibling, set its next_sibling to us
        if let Some(&(_, Some(prev_idx))) = self.stack.last() {
            self.hierarchy[prev_idx].next_sibling = NodeId::into_raw(&Some(NodeId::new(idx)));
        }

        // Update parent's "last seen child" to us
        if let Some(parent) = self.stack.last_mut() {
            parent.1 = Some(idx);
        }

        // Push the hierarchy item (last_child will be set in close_node)
        self.hierarchy.push(NodeHierarchyItem {
            parent: parent_raw,
            previous_sibling: prev_sibling_raw,
            next_sibling: 0, // Will be set by next sibling's open_node
            last_child: 0,   // Will be set in close_node
        });
        self.node_data.push(node_data);

        // Push onto stack: this node is now the "open" element, no children yet
        self.stack.push((idx, None));
    }

    /// Close the current element. Sets the `last_child` pointer.
    pub fn close_node(&mut self) {
        use crate::id::NodeId;

        if let Some((idx, last_child_idx)) = self.stack.pop() {
            // Set last_child on this node's hierarchy item
            self.hierarchy[idx].last_child =
                last_child_idx.map_or(0, |lc| NodeId::into_raw(&Some(NodeId::new(lc))));
        }
    }

    /// Add a leaf node (text, br, hr, etc.) that has no children.
    pub fn add_leaf(&mut self, node_data: NodeData) {
        self.open_node(node_data);
        self.close_node();
    }

    /// Add a CSS stylesheet scoped to a node ID.
    pub fn add_css(&mut self, node_id: usize, css: Css) {
        self.css.push(crate::dom::CssWithNodeId { node_id, css });
    }

    /// Finish building and produce a `FastDom`.
    #[must_use]
    pub fn finish(self) -> crate::dom::FastDom {
        crate::dom::FastDom {
            node_hierarchy: self.hierarchy.into(),
            node_data: self.node_data.into(),
            css: self.css.into(),
        }
    }
}

/// Convert an XML node tree into a `FastDom` (arena-based) in a single DFS
/// pass: [`walk_element`] into a [`CompactDomBuilder`].
#[allow(clippy::result_large_err)]
// returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
fn xml_node_to_fast_dom<'a>(
    xml_node: &'a XmlNode,
    component_map: &'a ComponentMap,
    inside_svg: bool,
    font_source: Option<element::FontSourceFn>,
    builder: &mut CompactDomBuilder,
    depth: usize,
) -> Result<(), RenderDomError> {
    walk_element(
        xml_node,
        component_map,
        element::ElementScope { inside_svg },
        element::FontScope {
            source: font_source,
            faces: &[],
        },
        builder,
        depth,
    )
}

/// Render a DOM from an XML body node using the fast arena-based path.
/// Builds a `FastDom` directly (no tree intermediary), then creates `StyledDom`.
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would
                                   // break the C ABI/api.json
fn render_dom_from_body_node_fast<'a>(
    html_node: &XmlNode,
    body_node: &'a XmlNode,
    mut global_css: Option<Css>,
    component_map: &'a ComponentMap,
    max_width: Option<f32>,
    font_source: Option<element::FontSourceFn>,
) -> Result<StyledDom, RenderDomError> {
    let mut builder = CompactDomBuilder::new();

    // Build the HTML > Body wrapper + body content in one pass
    // Open <html>, with the element's own attributes
    builder.open_node(html_root_node_data(html_node));
    // Open <body> (the body_node content goes inside)
    xml_node_to_fast_dom(
        body_node,
        component_map,
        false,
        font_source,
        &mut builder,
        0,
    )?;
    // Close <html>
    builder.close_node();

    // Collect CSS rules from each source.
    let mut combined_rules: Vec<CssRuleBlock> = Vec::new();
    if let Some(max_width) = max_width {
        let max_width_css =
            Css::from_string(format!("html {{ max-width: {max_width}px; }}").into());
        combined_rules.extend(max_width_css.rules.into_library_owned_vec());
    }
    let mut combined_keyframes = Vec::new();
    if let Some(css) = global_css.take() {
        combined_rules.extend(css.rules.into_library_owned_vec());
        combined_keyframes.extend(css.keyframes.into_library_owned_vec());
    }
    let mut combined_css = Css::new(combined_rules);
    combined_css.keyframes = combined_keyframes.into();

    // Add CSS to the FastDom
    let mut fast_dom = builder.finish();
    fast_dom.css = vec![crate::dom::CssWithNodeId {
        node_id: 0, // Global scope (root)
        css: combined_css,
    }]
    .into();

    // Create StyledDom via the fast path (no tree→arena conversion)
    let styled = StyledDom::create_from_fast_dom(fast_dom);
    Ok(styled)
}

// render_dom_from_body_node() removed — use render_dom_from_body_node_fast() or str_to_dom()

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
///     Var {
///         name: "a".to_string(),
///         format_spec: None,
///     },
///     Str(", ".to_string()),
///     Var {
///         name: "b".to_string(),
///         format_spec: None,
///     },
///     Str("{ ".to_string()),
///     Var {
///         name: "c".to_string(),
///         format_spec: None,
///     },
///     Str(" }".to_string()),
/// ];
/// assert_eq!(output, split);
/// ```
#[must_use]
pub fn split_dynamic_string(input: &str) -> Vec<DynamicItem> {
    use self::DynamicItem::{Str, Var};

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
                    items.push(Var {
                        name: var_name,
                        format_spec,
                    });
                    current_idx += start_offset;
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
fn combine_and_replace_dynamic_items(
    input: &[DynamicItem],
    variables: &ComponentArgumentVec,
) -> String {
    let mut s = String::new();

    for item in input {
        match item {
            DynamicItem::Var { name, format_spec } => {
                let variable_name = normalize_casing(name.trim());
                if let Some(resolved_var) = variables
                    .iter()
                    .find(|s| s.name.as_str() == variable_name)
                    .map(|q| &q.arg_type)
                {
                    // Format specifiers are applied at compile time, not at runtime replacement
                    s.push_str(resolved_var);
                } else {
                    s.push('{');
                    s.push_str(name);
                    if let Some(spec) = format_spec {
                        s.push(':');
                        s.push_str(spec);
                    }
                    s.push('}');
                }
            }
            DynamicItem::Str(dynamic_str) => {
                s.push_str(dynamic_str);
            }
        }
    }

    s
}

/// Given a string and a key => value mapping, replaces parts of the string with the value, i.e.:
///
/// ```rust
/// # use azul_core::xml::{format_args_dynamic, ComponentArgument, ComponentArgumentVec};
/// # use azul_css::AzString;
/// let variables: ComponentArgumentVec = vec![
///     ComponentArgument {
///         name: AzString::from("a"),
///         arg_type: AzString::from("value1"),
///     },
///     ComponentArgument {
///         name: AzString::from("b"),
///         arg_type: AzString::from("value2"),
///     },
/// ]
/// .into();
///
/// let initial = "hello {a}, {b}{{ {c} }}";
/// let expected = "hello value1, value2{ {c} }".to_string();
/// assert_eq!(format_args_dynamic(initial, &variables), expected);
/// ```
///
/// Note: the number (0, 1, etc.) is the order of the argument, it is irrelevant for
/// runtime formatting, only important for keeping the component / function arguments
/// in order when compiling the arguments to Rust code
#[must_use]
pub fn format_args_dynamic(input: &str, variables: &ComponentArgumentVec) -> String {
    let dynamic_str_items = split_dynamic_string(input);
    combine_and_replace_dynamic_items(&dynamic_str_items, variables)
}

// NOTE: Two sequential returns count as a single return, while single returns get ignored.
#[must_use]
pub fn prepare_string(input: &str) -> String {
    const SPACE: &str = " ";
    const RETURN: &str = "\n";

    let input = input.trim();

    if input.is_empty() {
        return String::new();
    }

    // The character references, decoded by THE decoder
    // (`html::decode_character_references`, XML's rules with HTML's names) in
    // one left-to-right pass, so `&amp;lt;` is the text `&lt;`, not `<`.
    // `&nbsp;` is the exception: it stays `&nbsp;` here (written as `&amp;nbsp;`
    // first, which the one pass turns back into `&nbsp;`) so the per-line pass
    // below, which runs AFTER the trim, turns it into a space the trim keeps -
    // decoded to U+00A0 now, `str::trim` would eat it.
    let protected = input.replace("&nbsp;", "&amp;nbsp;");
    let input = html::decode_character_references(&protected, html::CharRefMode::Xml);

    let input_len = input.len();
    let mut final_lines: Vec<String> = Vec::new();
    let mut last_line_was_empty = false;

    for line in input.lines() {
        let line = line.trim();
        let line = line.replace("&nbsp;", " ");
        let current_line_is_empty = line.is_empty();

        if !current_line_is_empty {
            if last_line_was_empty {
                final_lines.push(format!("{RETURN}{line}"));
            } else {
                final_lines.push(line);
            }
        }

        last_line_was_empty = current_line_is_empty;
    }

    let mut target = String::with_capacity(input_len);
    for (line_idx, line) in final_lines.iter().enumerate() {
        // A joining space goes before every line EXCEPT the first (idx 0) and a
        // paragraph break (RETURN-prefixed). The old code also skipped the LAST line,
        // which dropped the word boundary for a soft-wrapped final line
        // ("Hello\nworld" -> "Helloworld").
        if !(line.starts_with(RETURN) || line_idx == 0) {
            target.push_str(SPACE);
        }
        target.push_str(line);
    }
    target
}

/// Parses a string ("true" or "false")
#[must_use]
pub fn parse_bool(input: &str) -> Option<bool> {
    match input {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// ONE table: which XML attribute sets what on a node (the XML → DOM
/// builders and the code generator both read it).
#[path = "xml_attributes.rs"]
pub mod attributes;

/// One element of markup -> one DOM node: the builtin renderers every XML
/// loader instantiates its elements with.
#[path = "xml_element.rs"]
pub mod element;

/// HTML as a browser reads it (the lenient loader), and the ONE tree
/// construction every XML loader shares.
///
// The empty `///` line ends this summary: rustdoc joins it with the `//!`
// docs of `xml_html.rs` (clippy::too_long_first_doc_paragraph).
#[path = "xml_html.rs"]
pub mod html;

/// The HTML Standard's named character references (generated).
#[path = "xml_entities.rs"]
mod entities;

#[cfg(test)]
#[path = "xml_test.rs"]
mod xml_test;

/// The `<html>` element's own attributes on the root node both loaders build
/// (WPT8 found (c)).
#[cfg(test)]
mod html_root_attribute_tests {
    use super::*;

    /// `<html style="background: red"><body></body></html>`.
    fn html_with_a_style() -> Vec<XmlNodeChild> {
        let body = XmlNode {
            node_type: "body".into(),
            attributes: XmlAttributeMap::default(),
            children: Vec::<XmlNodeChild>::new().into(),
        };
        let html = XmlNode {
            node_type: "html".into(),
            attributes: XmlAttributeMap::from(StringPairVec::from_vec(alloc::vec![AzStringPair {
                key: AzString::from("style"),
                value: AzString::from("background: red;"),
            }])),
            children: alloc::vec![XmlNodeChild::Element(body)].into(),
        };
        alloc::vec![XmlNodeChild::Element(html)]
    }

    #[test]
    fn the_html_elements_style_attribute_reaches_the_root() {
        let map = ComponentMap::with_builtin();

        // The tree loader (`str_to_dom_unstyled`).
        let dom = str_to_dom_unstyled(&html_with_a_style(), &map).expect("the markup parses");
        assert!(
            matches!(dom.root.node_type, NodeType::Html),
            "the root is the html element"
        );
        assert!(
            !dom.root.style.rules.as_ref().is_empty(),
            "the html element's inline style is on the root (the tree loader built a bare Html)"
        );

        // The arena loader (`str_to_dom`).
        let styled = str_to_dom(&html_with_a_style(), &map, None).expect("the markup parses");
        let root = &styled.node_data.as_container()[crate::id::NodeId::ZERO];
        assert!(
            matches!(root.node_type, NodeType::Html),
            "the root is the html element"
        );
        assert!(
            !root.style.rules.as_ref().is_empty(),
            "the html element's inline style is on the root (the arena loader built a bare Html)"
        );
    }
}

/// `Xml::scan_external_resources` (MAIL9's engine gap, SCANORDER).
#[cfg(test)]
mod scan_external_resources_tests {
    use super::*;

    #[test]
    fn scan_external_resources_lists_resources_in_document_order_once_and_ignores_url_in_prose() {
        // The worklist was a LIFO stack fed in order, so siblings came out
        // last first; and every TEXT node was scanned as CSS, so prose that
        // says `url(...)` became a resource and a `<style>` sheet was scanned
        // twice (its text, then its element).
        let xml = Xml::create_from_html(AzString::from(
            "<html><body><img src=\"a.png\"><p>write url(prose.png) in CSS</p><img \
             src=\"b.png\"><style>.x { background: url(c.png); }</style></body></html>",
        ));
        let urls: Vec<String> = xml
            .scan_external_resources()
            .as_ref()
            .iter()
            .map(|r| r.url.as_str().to_string())
            .collect();
        assert_eq!(
            urls,
            ["a.png", "b.png", "c.png"],
            "document order, each once, nothing from prose"
        );
    }
}

/// `MimeTypeHint::from_extension`, the one extension table.
#[cfg(test)]
mod mime_type_hint_tests {
    use super::*;

    /// A JSON or text file had no row and came out as
    /// `application/octet-stream`, so crash_mail kept its own twin table for
    /// the two (FIX9-INPUT round 2 item 4).
    #[test]
    fn a_json_or_txt_extension_has_its_own_media_type() {
        for (extension, media_type) in [
            ("json", "application/json"),
            ("JSON", "application/json"),
            ("txt", "text/plain"),
            ("TXT", "text/plain"),
        ] {
            assert_eq!(
                MimeTypeHint::from_extension(extension).inner.as_str(),
                media_type,
                "the media type of .{extension}"
            );
        }
        // Unchanged: a known row, and the default.
        assert_eq!(
            MimeTypeHint::from_extension("png").inner.as_str(),
            "image/png"
        );
        assert_eq!(
            MimeTypeHint::from_extension("xyz").inner.as_str(),
            "application/octet-stream"
        );
    }
}
