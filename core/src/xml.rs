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
use core::{fmt, fmt::Write, hash::Hash};

use azul_css::{
    css::{
        Css, CssDeclaration, CssPath, CssPathPseudoSelector, CssPathSelector, CssRuleBlock,
        NodeTypeTag,
    },
    codegen::format::VecContents,
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

/// Holds the list of arguments and whether the component accepts text content.
/// Used by the compile pipeline to generate Rust function signatures.
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentArguments {
    pub args: ComponentArgumentVec,
    pub accepts_text: bool,
}

/// Name of an XML/HTML component (e.g. `"button"`, `"my-widget"`).
type ComponentName = String;
/// Compiled source code string for a component.
type CompiledComponent = String;

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
    #[must_use] pub fn new(s: &str) -> Self {
        Self {
            inner: AzString::from(s),
        }
    }

    #[must_use] pub fn from_extension(ext: &str) -> Self {
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
const MAX_XML_NESTING_DEPTH: usize = 512;

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
    #[must_use] pub fn scan_external_resources(&self) -> ExternalResourceVec {
        let mut resources = Vec::new();

        // AUDIT 2026-07-08: iterative DFS with an explicit worklist. The old
        // per-node recursion overflowed the stack on pathologically deep markup
        // (a single-purpose scan frame is large: string lowercasing + closure +
        // wide match). An explicit stack keeps memory on the heap; `depth` still
        // bounds how deep we descend so unbounded input can't grow the worklist
        // without limit.
        let mut stack: Vec<(&XmlNodeChild, usize)> = Vec::new();
        for child in self.root.as_ref() {
            stack.push((child, 0));
        }
        while let Some((child, depth)) = stack.pop() {
            match child {
                XmlNodeChild::Text(text) => {
                    // CSS @import / url() in text content (inside <style> tags).
                    Self::extract_css_urls(text.as_str(), &mut resources);
                }
                XmlNodeChild::Element(node) => {
                    if depth > MAX_XML_NESTING_DEPTH {
                        // Deeper subtrees are simply not scanned.
                        continue;
                    }
                    Self::scan_node(node, &mut resources);
                    for c in node.children.as_ref() {
                        stack.push((c, depth + 1));
                    }
                }
            }
        }

        resources.into()
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
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
                    let kind = if type_attr
                        .as_ref()
                        .is_some_and(|t| t.starts_with("audio"))
                    {
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
            let import_url = if trimmed.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("url(")) {
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
                trimmed.split_whitespace().next().map(alloc::string::ToString::to_string)
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
        use self::XmlStreamError::{UnexpectedEndOfStream, InvalidName, NonXmlChar, InvalidChar, InvalidCharMultiple, InvalidQuote, InvalidSpace, InvalidString, InvalidReference, InvalidExternalID, InvalidCommentData, InvalidCommentEnd, InvalidCharacterData};
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
        use self::XmlParseError::{InvalidDeclaration, InvalidComment, InvalidPI, InvalidDoctype, InvalidEntity, InvalidElement, InvalidAttribute, InvalidCdata, InvalidCharData, UnknownToken};
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::XmlError::{NoParserAvailable, InvalidXmlPrefixUri, UnexpectedXmlUri, UnexpectedXmlnsUri, InvalidElementNamePrefix, DuplicatedNamespace, UnknownNamespace, UnexpectedCloseTag, UnexpectedEntityCloseTag, UnknownEntityReference, MalformedEntityReference, EntityReferenceLoop, InvalidAttributeValue, DuplicatedAttribute, NoRootNode, SizeLimit, DtdDetected, MalformedHierarchy, ParserError, UnclosedRootNode, UnexpectedDeclaration, NodesLimitReached, AttributesLimitReached, NamespacesLimitReached, InvalidName, NonXmlChar, InvalidChar, InvalidChar2, InvalidString, InvalidExternalID, InvalidComment, InvalidCharacterData, UnknownToken, UnexpectedEndOfStream};
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
    #[must_use] pub fn builtin(name: &str) -> Self {
        Self {
            collection: AzString::from_const_str("builtin"),
            name: AzString::from(name),
        }
    }

    #[must_use] pub fn new(collection: &str, name: &str) -> Self {
        Self {
            collection: AzString::from(collection),
            name: AzString::from(name),
        }
    }

    /// Returns "collection:name" format string
    #[must_use] pub fn qualified_name(&self) -> String {
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
    #[must_use] pub fn new(t: ComponentFieldType) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(t)),
        }
    }

    #[must_use] pub fn as_ref(&self) -> &ComponentFieldType {
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
    #[must_use] pub fn new(v: ComponentFieldValue) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(v)),
        }
    }

    #[must_use] pub fn as_ref(&self) -> &ComponentFieldValue {
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
    #[must_use] pub fn parse(s: &str) -> Option<Self> {
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
            return Some(Self::OptionType(ComponentFieldTypeBox::new(
                inner_type,
            )));
        }

        // Vec<T>
        if let Some(inner) = s.strip_prefix("Vec<").and_then(|r| r.strip_suffix('>')) {
            let inner_type = Self::parse_depth(inner, depth + 1)?;
            return Some(Self::VecType(ComponentFieldTypeBox::new(
                inner_type,
            )));
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
    #[must_use] pub fn format(&self) -> String {
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
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// Runtime value for a component field — the "instance" counterpart
/// to `ComponentFieldType` (which is the "class" / type descriptor).
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
#[allow(clippy::large_enum_variant)] // #[repr(C,u8)] FFI enum: boxing a variant changes the C ABI/api.json
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
    #[must_use] pub fn get_field(&self, name: &str) -> Option<&ComponentFieldValue> {
        self.as_ref().iter().find_map(|v| {
            if v.name.as_str() == name {
                Some(&v.value)
            } else {
                None
            }
        })
    }

    /// Convenience: get a field as `&str` if it is `ComponentFieldValue::String`.
    #[must_use] pub fn get_string(&self, name: &str) -> Option<&AzString> {
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
    #[must_use] pub fn get_field(&self, name: &str) -> Option<&ComponentDataField> {
        self.fields
            .as_ref()
            .iter()
            .find(|f| f.name.as_str() == name)
    }

    /// Look up a field's default value as a string, if it exists and is a String variant.
    #[must_use] pub fn get_default_string(&self, name: &str) -> Option<&AzString> {
        self.get_field(name).and_then(|f| match &f.default_value {
            OptionComponentDefaultValue::Some(ComponentDefaultValue::String(s)) => Some(s),
            _ => None,
        })
    }

    /// Clone this data model, overriding the default value for a field by name.
    /// If the field is not found, the data model is returned unchanged.
    #[must_use] pub fn with_default(mut self, name: &str, value: ComponentDefaultValue) -> Self {
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
    use super::*;
    use serde::ser::SerializeStruct;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    // --- AzString helpers ---

    fn ser_azstring<S: Serializer>(s: &AzString, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(s.as_str())
    }

    fn de_azstring<'de, D: Deserializer<'de>>(deserializer: D) -> Result<AzString, D::Error> {
        let s = alloc::string::String::deserialize(deserializer)?;
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
            let s = alloc::string::String::deserialize(deserializer)?;
            Ok(string_to_field_type(&s))
        }
    }

    fn field_type_to_string(ft: &ComponentFieldType) -> alloc::string::String {
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
                ComponentDefaultValue::None => serializer.serialize_none(),
                ComponentDefaultValue::String(s) => serializer.serialize_str(s.as_str()),
                ComponentDefaultValue::Bool(b) => serializer.serialize_bool(*b),
                ComponentDefaultValue::I32(v) => serializer.serialize_i32(*v),
                ComponentDefaultValue::I64(v) => serializer.serialize_i64(*v),
                ComponentDefaultValue::U32(v) => serializer.serialize_u32(*v),
                ComponentDefaultValue::U64(v) => serializer.serialize_u64(*v),
                ComponentDefaultValue::Usize(v) => serializer.serialize_u64(*v as u64),
                ComponentDefaultValue::F32(v) => serializer.serialize_f32(*v),
                ComponentDefaultValue::F64(v) => serializer.serialize_f64(*v),
                ComponentDefaultValue::ColorU(c) => serializer.serialize_str(&alloc::format!(
                    "#{:02x}{:02x}{:02x}{:02x}",
                    c.r,
                    c.g,
                    c.b,
                    c.a
                )),
                ComponentDefaultValue::ComponentInstance(ci) => {
                    let mut map = serializer.serialize_map(Some(2))?;
                    map.serialize_entry("library", ci.library.as_str())?;
                    map.serialize_entry("component", ci.component.as_str())?;
                    map.end()
                }
                ComponentDefaultValue::CallbackFnPointer(name) => {
                    serializer.serialize_str(name.as_str())
                }
                ComponentDefaultValue::Json(json_str) => {
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
            Ok(match val {
                serde_json::Value::Null => ComponentDefaultValue::None,
                serde_json::Value::Bool(b) => ComponentDefaultValue::Bool(b),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        if let Ok(v) = i32::try_from(i) {
                            ComponentDefaultValue::I32(v)
                        } else {
                            ComponentDefaultValue::I64(i)
                        }
                    } else if let Some(f) = n.as_f64() {
                        ComponentDefaultValue::F64(f)
                    } else {
                        ComponentDefaultValue::None
                    }
                }
                serde_json::Value::String(s) => {
                    ComponentDefaultValue::String(AzString::from(s.as_str()))
                }
                _ => ComponentDefaultValue::None,
            })
        }
    }

    // --- OptionComponentDefaultValue ---

    impl Serialize for OptionComponentDefaultValue {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            match self {
                OptionComponentDefaultValue::Some(v) => v.serialize(serializer),
                OptionComponentDefaultValue::None => serializer.serialize_none(),
            }
        }
    }

    impl<'de> Deserialize<'de> for OptionComponentDefaultValue {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let val = Option::<ComponentDefaultValue>::deserialize(deserializer)?;
            Ok(match val {
                Some(v) => OptionComponentDefaultValue::Some(v),
                None => OptionComponentDefaultValue::None,
            })
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
                name: alloc::string::String,
                #[serde(rename = "type", default = "default_type")]
                field_type: ComponentFieldType,
                #[serde(default)]
                default: OptionComponentDefaultValue,
                #[serde(default)]
                required: bool,
                #[serde(default)]
                description: alloc::string::String,
            }
            fn default_type() -> ComponentFieldType {
                ComponentFieldType::String
            }

            let h = Helper::deserialize(deserializer)?;
            Ok(ComponentDataField {
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
            let fields: alloc::vec::Vec<&ComponentDataField> =
                self.fields.as_ref().iter().collect();
            s.serialize_field("fields", &fields)?;
            s.end()
        }
    }

    impl<'de> Deserialize<'de> for ComponentDataModel {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            struct Helper {
                #[serde(default)]
                name: alloc::string::String,
                #[serde(default)]
                description: alloc::string::String,
                #[serde(default)]
                fields: alloc::vec::Vec<ComponentDataField>,
            }

            let h = Helper::deserialize(deserializer)?;
            Ok(ComponentDataModel {
                name: AzString::from(h.name.as_str()),
                description: AzString::from(h.description.as_str()),
                fields: ComponentDataFieldVec::from_vec(h.fields),
            })
        }
    }
}

// Re-export serde impls so they're visible when the feature is enabled
#[cfg(feature = "serde-json")]
pub use serde_impl::*;

#[cfg(feature = "serde-json")]
impl ComponentDataModel {
    /// Serialize this data model to a JSON string.
    pub fn to_json(&self) -> Result<alloc::string::String, alloc::string::String> {
        serde_json::to_string_pretty(self).map_err(|e| alloc::format!("{}", e))
    }

    /// Deserialize a data model from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, alloc::string::String> {
        serde_json::from_str(json).map_err(|e| alloc::format!("{}", e))
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
    #[must_use] pub fn create() -> Self {
        Self::default()
    }
}

/// The target language for code compilation
// Threaded by reference through the codegen call graph; kept non-Copy so
// deriving Copy doesn't force trivially_copy_pass_by_ref churn across the many
// &CompileTarget codegen callers for a perf-neutral change.
#[allow(missing_copy_implementations)]
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

/// Render function type: takes component definition + data model (with current values
/// in `default_value` fields) + component map for recursive sub-component instantiation,
/// returns `StyledDom`.
///
/// The `data` parameter is typically `def.data_model` cloned and with caller-provided
/// values substituted into the `default_value` fields.
pub type ComponentRenderFn =
    fn(&ComponentDef, &ComponentDataModel, &ComponentMap) -> ResultStyledDomRenderDomError;

/// Compile function type: takes component definition + target language + data model, returns source code.
pub type ComponentCompileFn = fn(
    &ComponentDef,
    &CompileTarget,
    &ComponentDataModel,
    indent: usize,
) -> ResultStringCompileError;

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

/// A component definition — the "class" / "template" of a component.
/// Can come from Rust builtins, compiled widgets, JSON, or user creation in debugger.
///
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
    /// Compile to source code in target language
    pub compile_fn: ComponentCompileFn,
    /// Source code for `render_fn` (user-defined components only)
    pub render_fn_source: OptionString,
    /// Source code for `compile_fn` (user-defined components only)
    pub compile_fn_source: OptionString,
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
    #[must_use] pub fn get(&self, collection: &str, name: &str) -> Option<&ComponentDef> {
        self.libraries
            .iter()
            .find(|lib| lib.name.as_str() == collection)
            .and_then(|lib| lib.components.iter().find(|c| c.id.name.as_str() == name))
    }

    /// Unqualified lookup: "div" -> searches ONLY the "builtin" library.
    #[must_use] pub fn get_unqualified(&self, name: &str) -> Option<&ComponentDef> {
        self.get("builtin", name)
    }

    /// Parse a "collection:name" string into a lookup
    #[must_use] pub fn get_by_qualified_name(&self, qualified: &str) -> Option<&ComponentDef> {
        if let Some((collection, name)) = qualified.split_once(':') {
            self.get(collection, name)
        } else {
            self.get_unqualified(qualified)
        }
    }

    /// Get all libraries that can be exported (user-defined only)
    #[must_use] pub fn get_exportable_libraries(&self) -> Vec<&ComponentLibrary> {
        self.libraries.iter().filter(|lib| lib.exportable).collect()
    }

    /// Get all component definitions across all libraries
    #[must_use] pub fn all_components(&self) -> Vec<&ComponentDef> {
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
                $($tag => NodeType::$variant,)*
                _ => NodeType::Div,
            }
        }

        /// Map a tag name to its CSS `NodeTypeTag` for CSS matching in the compile pipeline.
        /// Falls back to `NodeTypeTag::Div` for unknown tags.
        fn tag_to_node_type_tag(tag: &str) -> NodeTypeTag {
            match tag {
                // `img`/`image`/`icon` have no 1:1 `NodeType` equivalent (see
                // `tag_to_node_type`), so they map to dedicated `NodeTypeTag` variants.
                "img" | "image" => NodeTypeTag::Img,
                "icon" => NodeTypeTag::Icon,
                $($tag => NodeTypeTag::$variant,)*
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

/// Default render function for builtin HTML elements.
/// Delegates to creating a DOM node of the appropriate `NodeType`.
fn builtin_render_fn(
    def: &ComponentDef,
    data: &ComponentDataModel,
    _component_map: &ComponentMap,
) -> ResultStyledDomRenderDomError {
    let node_type = tag_to_node_type(def.id.name.as_str());
    let mut dom = Dom::create_node(node_type);
    if let Some(text_str) = data.get_default_string("text") {
        let prepared = prepare_string(text_str);
        if !prepared.is_empty() {
            dom = dom.with_children(alloc::vec![Dom::create_text(prepared)].into());
        }
    }
    let r: Result<StyledDom, RenderDomError> = Ok(StyledDom::create(&mut dom, Css::empty()));
    r.into()
}

/// Default compile function for builtin HTML elements.
/// Generates `Dom::create_node(NodeType::Div)` style code for the target language.
fn builtin_compile_fn(
    def: &ComponentDef,
    target: &CompileTarget,
    data: &ComponentDataModel,
    indent: usize,
) -> ResultStringCompileError {
    let node_type = tag_to_node_type(def.id.name.as_str());
    let type_name = format!("{node_type:?}"); // "Div", "Body", "P", etc.
    let text = data.get_default_string("text");

    let r: Result<AzString, CompileError> = match target {
        CompileTarget::Rust => {
            text.map_or_else(|| Ok(format!("Dom::create_node(NodeType::{type_name})").into()), |text_str| Ok(format!(
                    "Dom::create_node(NodeType::{}).with_children(vec![Dom::create_text(\"{}\")])",
                    type_name,
                    text_str.as_str().replace('\\', "\\\\").replace('"', "\\\"")
                ).into()))
        }
        CompileTarget::C => {
            text.map_or_else(|| Ok(format!("AzDom_create{type_name}()").into()), |text_str| Ok(format!(
                    "AzDom_createText(AZ_STR(\"{}\"))",
                    text_str
                        .as_str()
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"")
                )
                .into()))
        }
        CompileTarget::Cpp => Ok(format!("Dom::create_{}()", type_name.to_lowercase()).into()),
        CompileTarget::Python => Ok(format!("Dom.create_{}()", type_name.to_lowercase()).into()),
    };
    r.into()
}

/// Pushes a `<div>` containing `"field_name: value"` text into the children list.
fn push_scalar_field(children: &mut Vec<Dom>, field_name: &str, value: &dyn fmt::Display) {
    use crate::dom::{Dom, NodeType};
    let text = alloc::format!("{field_name}: {value}");
    children.push(
        Dom::create_node(NodeType::Div).with_children(alloc::vec![Dom::create_text(text)].into()),
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
#[must_use] pub fn user_defined_render_fn(
    def: &ComponentDef,
    data: &ComponentDataModel,
    component_map: &ComponentMap,
) -> ResultStyledDomRenderDomError {
    use crate::dom::{Dom, NodeType};
    use azul_css::css::Css;

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
                                alloc::vec![Dom::create_text(text.to_string())].into(),
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
                            Dom::create_node(NodeType::Div)
                                .with_children(alloc::vec![Dom::create_text(text)].into()),
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
                                            alloc::vec![Dom::create_text(text)].into(),
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
                                            alloc::vec![Dom::create_text(text)].into(),
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
                                Dom::create_node(NodeType::Div)
                                    .with_children(alloc::vec![Dom::create_text(text)].into()),
                            );
                        }
                    }
                    ComponentDefaultValue::CallbackFnPointer(name) => {
                        // Callbacks are not rendered, just acknowledged
                        let text = alloc::format!("{}: fn({})", field_name, name.as_str());
                        children.push(
                            Dom::create_node(NodeType::Div)
                                .with_children(alloc::vec![Dom::create_text(text)].into()),
                        );
                    }
                    ComponentDefaultValue::Json(json_str) => {
                        let text = alloc::format!("{}: {}", field_name, json_str.as_str());
                        children.push(
                            Dom::create_node(NodeType::Div)
                                .with_children(alloc::vec![Dom::create_text(text)].into()),
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

/// Default compile function for user-defined (JSON-imported) components.
///
/// Generates source code that creates the component's DOM structure for the
/// target language. For each data field, emits the appropriate code:
/// - String fields → text node creation
/// - Scalar fields → formatted display
/// - `ComponentInstance` → function call to sub-component's render function
/// - `StyledDom` slots → child parameter pass-through
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
#[must_use] pub fn user_defined_compile_fn(
    def: &ComponentDef,
    target: &CompileTarget,
    data: &ComponentDataModel,
    indent: usize,
) -> ResultStringCompileError {
    let tag = def.id.name.as_str();
    let indent_str = " ".repeat(indent * 4);
    let inner_indent = " ".repeat((indent + 1) * 4);

    let r: Result<AzString, CompileError> = match target {
        CompileTarget::Rust => {
            let mut lines = Vec::new();
            lines.push(alloc::format!("{indent_str}// Component: {tag}"));
            lines.push(alloc::format!(
                "{indent_str}let mut children: Vec<Dom> = Vec::new();"
            ));

            for field in data.fields.as_ref() {
                let fname = field.name.as_str();
                match &field.default_value {
                    OptionComponentDefaultValue::Some(ComponentDefaultValue::String(s)) => {
                        let escaped = s.as_str().replace('\\', "\\\\").replace('"', "\\\"");
                        lines.push(alloc::format!(
                            "{inner_indent}children.push(Dom::create_text(\"{escaped}\"));"
                        ));
                    }
                    OptionComponentDefaultValue::Some(ComponentDefaultValue::Bool(b)) => {
                        lines.push(alloc::format!(
                            "{inner_indent}children.push(Dom::create_text(format!(\"{{}}: {{}}\", \"{fname}\", {b}).as_str()));"
                        ));
                    }
                    OptionComponentDefaultValue::Some(
                        ComponentDefaultValue::ComponentInstance(ci),
                    ) => {
                        let fn_name =
                            alloc::format!("render_{}", ci.component.as_str().replace('-', "_"));
                        lines.push(alloc::format!(
                            "{}children.push({}()); // sub-component {}:{}",
                            inner_indent,
                            fn_name,
                            ci.library.as_str(),
                            ci.component.as_str()
                        ));
                    }
                    _ => {
                        // For other types, generate a placeholder comment
                        lines.push(alloc::format!(
                            "{}// field '{}': {:?}",
                            inner_indent,
                            fname,
                            field.field_type
                        ));
                    }
                }
            }

            lines.push(alloc::format!(
                "{indent_str}Dom::create_node(NodeType::Div).with_children(children.into())"
            ));
            Ok(lines.join("\n").into())
        }
        CompileTarget::C => {
            let mut lines = Vec::new();
            lines.push(alloc::format!("{indent_str}/* Component: {tag} */"));
            lines.push(alloc::format!(
                "{indent_str}AzDom root = AzDom_createDiv();"
            ));

            for field in data.fields.as_ref() {
                let fname = field.name.as_str();
                match &field.default_value {
                    OptionComponentDefaultValue::Some(ComponentDefaultValue::String(s)) => {
                        let escaped = s.as_str().replace('\\', "\\\\").replace('"', "\\\"");
                        lines.push(alloc::format!(
                            "{inner_indent}AzDom_addChild(&root, AzDom_createText(AZ_STR(\"{escaped}\")));"
                        ));
                    }
                    OptionComponentDefaultValue::Some(
                        ComponentDefaultValue::ComponentInstance(ci),
                    ) => {
                        let fn_name =
                            alloc::format!("render_{}", ci.component.as_str().replace('-', "_"));
                        lines.push(alloc::format!(
                            "{inner_indent}AzDom_addChild(&root, {fn_name}());"
                        ));
                    }
                    _ => {
                        lines.push(alloc::format!("{inner_indent}/* field '{fname}' */"));
                    }
                }
            }

            lines.push(alloc::format!("{indent_str}return root;"));
            Ok(lines.join("\n").into())
        }
        CompileTarget::Cpp => {
            let mut lines = Vec::new();
            lines.push(alloc::format!("{indent_str}// Component: {tag}"));
            lines.push(alloc::format!(
                "{indent_str}auto root = Dom::create_div();"
            ));

            for field in data.fields.as_ref() {
                let fname = field.name.as_str();
                match &field.default_value {
                    OptionComponentDefaultValue::Some(ComponentDefaultValue::String(s)) => {
                        let escaped = s.as_str().replace('\\', "\\\\").replace('"', "\\\"");
                        lines.push(alloc::format!(
                            "{inner_indent}root.add_child(Dom::create_text(String(\"{escaped}\")));"
                        ));
                    }
                    OptionComponentDefaultValue::Some(
                        ComponentDefaultValue::ComponentInstance(ci),
                    ) => {
                        let fn_name =
                            alloc::format!("render_{}", ci.component.as_str().replace('-', "_"));
                        lines.push(alloc::format!(
                            "{inner_indent}root.add_child({fn_name}());"
                        ));
                    }
                    _ => {
                        lines.push(alloc::format!("{inner_indent}// field '{fname}'"));
                    }
                }
            }

            lines.push(alloc::format!("{indent_str}return root;"));
            Ok(lines.join("\n").into())
        }
        CompileTarget::Python => {
            let mut lines = Vec::new();
            lines.push(alloc::format!("{indent_str}# Component: {tag}"));
            lines.push(alloc::format!("{indent_str}root = Dom.create_div()"));

            for field in data.fields.as_ref() {
                let fname = field.name.as_str();
                match &field.default_value {
                    OptionComponentDefaultValue::Some(ComponentDefaultValue::String(s)) => {
                        let escaped = s
                            .as_str()
                            .replace('\\', "\\\\")
                            .replace('"', "\\\"")
                            .replace('\'', "\\'");
                        lines.push(alloc::format!(
                            "{inner_indent}root = root.with_child(Dom.create_text(\"{escaped}\"))"
                        ));
                    }
                    OptionComponentDefaultValue::Some(
                        ComponentDefaultValue::ComponentInstance(ci),
                    ) => {
                        let fn_name =
                            alloc::format!("render_{}", ci.component.as_str().replace('-', "_"));
                        lines.push(alloc::format!(
                            "{inner_indent}root = root.with_child({fn_name}())"
                        ));
                    }
                    _ => {
                        lines.push(alloc::format!("{inner_indent}# field '{fname}'"));
                    }
                }
            }

            lines.push(alloc::format!("{indent_str}return root"));
            Ok(lines.join("\n").into())
        }
    };
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
/// * `css` - Component-level CSS string. For most builtin elements this is `""` because
///   styling comes from `ua_css.rs` and the `SystemStyle`. Components that need extra
///   styling (e.g. a future high-level button widget) can pass CSS here.
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
        compile_fn: builtin_compile_fn,
        render_fn_source: None.into(),
        compile_fn_source: None.into(),
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
        default_value: default.map_or_else(|| OptionComponentDefaultValue::None, OptionComponentDefaultValue::Some),
        required,
        description: AzString::from(description),
    }
}

/// Returns the tag-specific data model fields for builtin HTML elements.
/// These are the component's "main data model" — the attributes that define
/// what the component needs as configuration (e.g., `href` for `<a>`,
/// `src` for `<img>`). Universal HTML attributes (id, class, style, etc.)
/// are NOT included here — they are added separately by the debug server.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
fn builtin_data_model(tag: &str) -> Vec<ComponentDataField> {
    use ComponentDefaultValue as D;
    use ComponentFieldType::{String, Bool, I32};
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
    #[must_use] pub fn create() -> Self {
        Self::default()
    }

    /// Create a `ComponentMap` with the 52 built-in HTML element components pre-registered.
    #[must_use] pub fn with_builtin() -> Self {
        Self {
            libraries: alloc::vec![register_builtin_components()].into(),
        }
    }

    /// Build a `ComponentMap` from the libraries stored in an `AppConfig`.
    ///
    /// The `component_libraries` field already contains builtins (registered in
    /// `AppConfig::create()`) plus any user-added libraries.  No merging needed —
    /// `add_component_library` / `add_component` handle insertion at registration time.
    #[must_use] pub fn from_libraries(libs: &ComponentLibraryVec) -> Self {
        Self {
            libraries: libs.clone(),
        }
    }
}

/// Convert XML attributes to a `ComponentDataModel` by cloning the component's
/// base data model and overriding field defaults with values from the XML attributes.
///
/// This is the bridge between the XML parsing layer (key-value string pairs)
/// and the typed component data model. For each field in the base model,
/// if a matching XML attribute exists, its string value is set as the new default.
///
/// # Arguments
/// * `base_model` - The component's data model template (from `ComponentDef::data_model`)
/// * `xml_attributes` - The XML node's attribute map
/// * `text_content` - Optional text content from child text nodes
///
/// # Returns
/// A cloned `ComponentDataModel` with overridden defaults
fn xml_attrs_to_data_model(
    base_model: &ComponentDataModel,
    xml_attributes: &XmlAttributeMap,
    text_content: Option<&str>,
) -> ComponentDataModel {
    let mut model = base_model.clone();

    // Override defaults from XML attributes
    let mut fields_vec = core::mem::replace(
        &mut model.fields,
        ComponentDataFieldVec::from_const_slice(&[]),
    )
    .into_library_owned_vec();

    for field in &mut fields_vec {
        if let Some(attr_value) = xml_attributes.get_key(field.name.as_str()) {
            // Override the default_value with the XML attribute's string value
            field.default_value = OptionComponentDefaultValue::Some(ComponentDefaultValue::String(
                attr_value.clone(),
            ));
        }
    }

    model.fields = ComponentDataFieldVec::from_vec(fields_vec);

    // Handle text content — set the "text" field if present
    if let Some(text) = text_content {
        let prepared = prepare_string(text);
        if !prepared.is_empty() {
            model = model.with_default(
                "text",
                ComponentDefaultValue::String(AzString::from(prepared.as_str())),
            );
        }
    }

    model
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
        description: AzString::from_const_str("Conditional rendering: shows 'then' if condition is true, else shows 'else' (if provided)."),
        css: AzString::from_const_str(""),
        source: ComponentSource::Builtin,
        data_model: ComponentDataModel {
            name: AzString::from_const_str("IfData"),
            description: AzString::from_const_str("Data for conditional rendering"),
            fields: alloc::vec![
                data_field("condition", ComponentFieldType::Bool, Some(ComponentDefaultValue::Bool(false)), "The boolean condition to evaluate"),
            ].into(),
        },
        render_fn: builtin_if_render_fn,
        compile_fn: builtin_if_compile_fn,
        render_fn_source: None.into(),
        compile_fn_source: None.into(),
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
    let mut dom =
        Dom::create_node(NodeType::Div).with_children(alloc::vec![Dom::create_text(label)].into());
    let css = Css::empty();
    ResultStyledDomRenderDomError::Ok(StyledDom::create(&mut dom, css))
}

fn builtin_if_compile_fn(
    _comp: &ComponentDef,
    target: &CompileTarget,
    _data: &ComponentDataModel,
    _indent: usize,
) -> ResultStringCompileError {
    match target {
        CompileTarget::Rust => ResultStringCompileError::Ok(AzString::from(
            "if data.condition {\n    // then branch\n    Dom::create_div()\n} else {\n    // else branch\n    Dom::create_div()\n}"
        )),
        CompileTarget::C => ResultStringCompileError::Ok(AzString::from(
            "if (data.condition) {\n    // then branch\n    AzDom_createDiv();\n} else {\n    // else branch\n    AzDom_createDiv();\n}"
        )),
        CompileTarget::Cpp => ResultStringCompileError::Ok(AzString::from(
            "if (data.condition) {\n    // then branch\n    Dom::create_div();\n} else {\n    // else branch\n    Dom::create_div();\n}"
        )),
        CompileTarget::Python => ResultStringCompileError::Ok(AzString::from(
            "if data.condition:\n    # then branch\n    Dom.create_div()\nelse:\n    # else branch\n    Dom.create_div()"
        )),
    }
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
        compile_fn: builtin_for_compile_fn,
        render_fn_source: None.into(),
        compile_fn_source: None.into(),
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
            Dom::create_node(NodeType::Div)
                .with_children(alloc::vec![Dom::create_text(alloc::format!("Item {i}"))].into()),
        );
    }
    let mut dom = Dom::create_node(NodeType::Div).with_children(items.into());
    let css = Css::empty();
    ResultStyledDomRenderDomError::Ok(StyledDom::create(&mut dom, css))
}

fn builtin_for_compile_fn(
    _comp: &ComponentDef,
    target: &CompileTarget,
    _data: &ComponentDataModel,
    _indent: usize,
) -> ResultStringCompileError {
    match target {
        CompileTarget::Rust => ResultStringCompileError::Ok(AzString::from(
            "let mut children = Vec::new();\nfor i in 0..data.count {\n    children.push(Dom::create_div());\n}\nDom::create_div().with_children(children)"
        )),
        CompileTarget::C => ResultStringCompileError::Ok(AzString::from(
            "AzDom container = AzDom_createDiv();\nfor (uint32_t i = 0; i < data.count; i++) {\n    AzDom_addChild(&container, AzDom_createDiv());\n}"
        )),
        CompileTarget::Cpp => ResultStringCompileError::Ok(AzString::from(
            "auto container = Dom::create_div();\nfor (uint32_t i = 0; i < data.count; i++) {\n    container.add_child(Dom::create_div());\n}"
        )),
        CompileTarget::Python => ResultStringCompileError::Ok(AzString::from(
            "container = Dom.create_div()\nfor i in range(data.count):\n    container = container.with_child(Dom.create_div())"
        )),
    }
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
        compile_fn: builtin_map_compile_fn,
        render_fn_source: None.into(),
        compile_fn_source: None.into(),
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
    let mut dom =
        Dom::create_node(NodeType::Div).with_children(alloc::vec![Dom::create_text(label)].into());
    let css = Css::empty();
    ResultStyledDomRenderDomError::Ok(StyledDom::create(&mut dom, css))
}

fn builtin_map_compile_fn(
    _comp: &ComponentDef,
    target: &CompileTarget,
    _data: &ComponentDataModel,
    _indent: usize,
) -> ResultStringCompileError {
    match target {
        CompileTarget::Rust => ResultStringCompileError::Ok(AzString::from(
            "let items: Vec<serde_json::Value> = serde_json::from_str(&data.data_json).unwrap_or_default();\nlet children: Vec<Dom> = items.iter().map(|item| {\n    Dom::create_div() // map template\n}).collect();\nDom::create_div().with_children(children)"
        )),
        CompileTarget::C => ResultStringCompileError::Ok(AzString::from(
            "// Parse data.data_json and map each item\nAzDom container = AzDom_createDiv();\n// TODO: iterate parsed JSON array"
        )),
        CompileTarget::Cpp => ResultStringCompileError::Ok(AzString::from(
            "// Parse data.data_json and map each item\nauto container = Dom::create_div();\n// TODO: iterate parsed JSON array"
        )),
        CompileTarget::Python => ResultStringCompileError::Ok(AzString::from(
            "import json\nitems = json.loads(data.data_json)\ncontainer = Dom.create_div()\nfor item in items:\n    container = container.with_child(Dom.create_div())"
        )),
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
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
#[must_use] pub extern "C" fn register_builtin_components() -> ComponentLibrary {
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
            builtin_component_def("html", "HTML", None, ""),
            builtin_component_def("head", "Head", None, ""),
            builtin_component_def("title", "Title", Some(""), ""),
            builtin_component_def("body", "Body", None, ""),
            // Block-level
            builtin_component_def("div", "Div", None, ""),
            builtin_component_def("header", "Header", None, ""),
            builtin_component_def("footer", "Footer", None, ""),
            builtin_component_def("section", "Section", None, ""),
            builtin_component_def("article", "Article", None, ""),
            builtin_component_def("aside", "Aside", None, ""),
            builtin_component_def("nav", "Nav", None, ""),
            builtin_component_def("main", "Main", None, ""),
            builtin_component_def("figure", "Figure", None, ""),
            builtin_component_def("figcaption", "Figure Caption", Some(""), ""),
            builtin_component_def("address", "Address", Some(""), ""),
            builtin_component_def("details", "Details", None, ""),
            builtin_component_def("summary", "Summary", Some("Details"), ""),
            builtin_component_def("dialog", "Dialog", None, ""),
            // Headings — default text is the heading level name so preview is visible
            builtin_component_def("h1", "Heading 1", Some("Heading 1"), ""),
            builtin_component_def("h2", "Heading 2", Some("Heading 2"), ""),
            builtin_component_def("h3", "Heading 3", Some("Heading 3"), ""),
            builtin_component_def("h4", "Heading 4", Some("Heading 4"), ""),
            builtin_component_def("h5", "Heading 5", Some("Heading 5"), ""),
            builtin_component_def("h6", "Heading 6", Some("Heading 6"), ""),
            // Text content
            builtin_component_def("p", "Paragraph", Some("Paragraph text"), ""),
            builtin_component_def("span", "Span", Some(""), ""),
            builtin_component_def("pre", "Preformatted", Some(""), ""),
            builtin_component_def("code", "Code", Some(""), ""),
            builtin_component_def("blockquote", "Blockquote", Some(""), ""),
            builtin_component_def("br", "Line Break", None, ""),
            builtin_component_def("hr", "Horizontal Rule", None, ""),
            builtin_component_def("icon", "Icon", Some(""), ""),
            // Lists
            builtin_component_def("ul", "Unordered List", None, ""),
            builtin_component_def("ol", "Ordered List", None, ""),
            builtin_component_def("li", "List Item", Some("List item"), ""),
            builtin_component_def("dl", "Description List", None, ""),
            builtin_component_def("dt", "Description Term", Some(""), ""),
            builtin_component_def("dd", "Description Details", Some(""), ""),
            builtin_component_def("menu", "Menu", None, ""),
            builtin_component_def("menuitem", "Menu Item", Some(""), ""),
            builtin_component_def("dir", "Directory List", None, ""),
            // Tables
            builtin_component_def("table", "Table", None, ""),
            builtin_component_def("caption", "Table Caption", Some(""), ""),
            builtin_component_def("thead", "Table Head", None, ""),
            builtin_component_def("tbody", "Table Body", None, ""),
            builtin_component_def("tfoot", "Table Foot", None, ""),
            builtin_component_def("tr", "Table Row", None, ""),
            builtin_component_def("th", "Table Header Cell", Some("Header"), ""),
            builtin_component_def("td", "Table Data Cell", Some(""), ""),
            builtin_component_def("colgroup", "Column Group", None, ""),
            builtin_component_def("col", "Column", None, ""),
            // Inline
            builtin_component_def("a", "Link", Some("Link text"), ""),
            builtin_component_def("strong", "Strong", Some(""), ""),
            builtin_component_def("em", "Emphasis", Some(""), ""),
            builtin_component_def("b", "Bold", Some(""), ""),
            builtin_component_def("i", "Italic", Some(""), ""),
            builtin_component_def("u", "Underline", Some(""), ""),
            builtin_component_def("s", "Strikethrough", Some(""), ""),
            builtin_component_def("small", "Small", Some(""), ""),
            builtin_component_def("mark", "Mark", Some(""), ""),
            builtin_component_def("del", "Deleted Text", Some(""), ""),
            builtin_component_def("ins", "Inserted Text", Some(""), ""),
            builtin_component_def("sub", "Subscript", Some(""), ""),
            builtin_component_def("sup", "Superscript", Some(""), ""),
            builtin_component_def("samp", "Sample Output", Some(""), ""),
            builtin_component_def("kbd", "Keyboard Input", Some(""), ""),
            builtin_component_def("var", "Variable", Some(""), ""),
            builtin_component_def("cite", "Citation", Some(""), ""),
            builtin_component_def("dfn", "Definition", Some(""), ""),
            builtin_component_def("abbr", "Abbreviation", Some(""), ""),
            builtin_component_def("acronym", "Acronym", Some(""), ""),
            builtin_component_def("q", "Inline Quote", Some(""), ""),
            builtin_component_def("time", "Time", Some(""), ""),
            builtin_component_def("big", "Big", Some(""), ""),
            builtin_component_def("bdo", "BiDi Override", Some(""), ""),
            builtin_component_def("bdi", "BiDi Isolate", Some(""), ""),
            builtin_component_def("wbr", "Word Break Opportunity", None, ""),
            builtin_component_def("ruby", "Ruby Annotation", None, ""),
            builtin_component_def("rt", "Ruby Text", Some(""), ""),
            builtin_component_def("rtc", "Ruby Text Container", None, ""),
            builtin_component_def("rp", "Ruby Parenthesis", Some(""), ""),
            builtin_component_def("data", "Data", Some(""), ""),
            // Forms
            builtin_component_def("form", "Form", None, ""),
            builtin_component_def("fieldset", "Field Set", None, ""),
            builtin_component_def("legend", "Legend", Some("Legend"), ""),
            builtin_component_def("label", "Label", Some("Label"), ""),
            builtin_component_def("input", "Input", None, ""),
            builtin_component_def("button", "Button", Some("Button text"), ""),
            builtin_component_def("select", "Select", None, ""),
            builtin_component_def("optgroup", "Option Group", None, ""),
            builtin_component_def("option", "Option", Some(""), ""),
            builtin_component_def("textarea", "Text Area", Some(""), ""),
            builtin_component_def("output", "Output", Some(""), ""),
            builtin_component_def("progress", "Progress", None, ""),
            builtin_component_def("meter", "Meter", None, ""),
            builtin_component_def("datalist", "Data List", None, ""),
            // Embedded content
            builtin_component_def("canvas", "Canvas", None, ""),
            builtin_component_def("object", "Object", None, ""),
            builtin_component_def("param", "Parameter", None, ""),
            builtin_component_def("embed", "Embed", None, ""),
            builtin_component_def("audio", "Audio", None, ""),
            builtin_component_def("video", "Video", None, ""),
            builtin_component_def("source", "Source", None, ""),
            builtin_component_def("track", "Track", None, ""),
            builtin_component_def("map", "Image Map", None, ""),
            builtin_component_def("area", "Map Area", None, ""),
            builtin_component_def("svg", "SVG", None, ""),
            // Metadata
            builtin_component_def("meta", "Meta", None, ""),
            builtin_component_def("link", "Link (Resource)", None, ""),
            builtin_component_def("script", "Script", Some(""), ""),
            builtin_component_def("style", "Style", Some(""), ""),
            builtin_component_def("base", "Base URL", None, ""),
            // Structural control-flow builtins (F1-F3)
            builtin_if_component(),
            builtin_for_component(),
            builtin_map_component(),
        ]
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
        assert!(!(self.parsed_dom != fixed), 
                "\r\nExpected DOM did not match:\r\n\r\nexpected: ----------\r\n{}\r\ngot: \
                 ----------\r\n{}\r\n",
                self.parsed_dom.get_html_string("", "", true),
                fixed.get_html_string("", "", true)
            );
    }

    #[must_use] pub fn into_styled_dom(self) -> StyledDom {
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
    #[must_use] pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s.as_str()),
            Self::Element(_) => None,
        }
    }

    /// Get the element if this is an element node
    #[must_use] pub const fn as_element(&self) -> Option<&XmlNode> {
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
    #[must_use] pub fn with_children(mut self, v: Vec<XmlNodeChild>) -> Self {
        Self {
            children: v.into(),
            ..self
        }
    }

    /// Get all text content concatenated from direct children
    #[must_use] pub fn get_text_content(&self) -> String {
        self.children
            .as_ref()
            .iter()
            .filter_map(|child| child.as_text())
            .collect::<Vec<_>>()
            .join("")
    }

    /// Check if this node has only text children (no element children)
    #[must_use] pub fn has_only_text_children(&self) -> bool {
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
        use self::CompileError::{Dom, Xml, Css};
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
        use self::DomXmlParseError::{NoHtmlNode, MultipleHtmlRootNodes, NoBodyInHtml, MultipleBodyNodes, Xml, MalformedHierarchy, RenderDom, Component, Css};
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
        use self::ComponentParseError::{NotAComponent, UnnamedComponent, MissingName, MissingType, WhiteSpaceInComponentName, WhiteSpaceInComponentType, CssError};
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
        use self::ComponentError::{UselessFunctionArgument, UnknownComponent};
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
pub fn get_html_node(root_nodes: &[XmlNodeChild]) -> Result<&XmlNode, DomXmlParseError> {
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

#[must_use] pub fn find_attribute<'a>(node: &'a XmlNode, attribute: &str) -> Option<&'a AzString> {
    node.attributes
        .iter()
        .find(|n| normalize_casing(n.key.as_str()).as_str() == attribute)
        .map(|s| &s.value)
}

/// Normalizes input such as `abcDef`, `AbcDef`, `abc-def` to the normalized form of `abc_def`
#[must_use] pub fn normalize_casing(input: &str) -> String {
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
/// Returns an error if the XML cannot be parsed into a DOM (malformed markup or an unknown component).
pub fn str_to_dom<'a>(
    root_nodes: &'a [XmlNodeChild],
    component_map: &'a ComponentMap,
    max_width: Option<f32>,
) -> Result<StyledDom, DomXmlParseError> {
    // Delegate to the fast path (Dom::Fast / CompactDom arena).
    str_to_dom_fast(root_nodes, component_map, max_width)
}

/// Parse XML to `StyledDom` via arena-based `FastDom` (no tree intermediary).
///
/// **Note**: `str_to_dom()` now delegates to this function, so you can use
/// either one. This function is kept for backward compatibility.
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
fn str_to_dom_fast<'a>(
    root_nodes: &'a [XmlNodeChild],
    component_map: &'a ComponentMap,
    max_width: Option<f32>,
) -> Result<StyledDom, DomXmlParseError> {
    let html_node = get_html_node(root_nodes)?;
    let body_node = get_body_node(html_node.children.as_ref())?;

    let mut global_style = None;

    if let Some(head_node) = find_node_by_type(html_node.children.as_ref(), "head") {
        if let Some(style_node) = find_node_by_type(head_node.children.as_ref(), "style") {
            let text = style_node.get_text_content();
            if !text.is_empty() {
                let parsed_css = Css::from_string(text.into());
                global_style = Some(parsed_css);
            }
        }
    }

    render_dom_from_body_node_fast(body_node, global_style, component_map, max_width)
        .map_err(Into::into)
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
/// Returns an error if the XML cannot be parsed into a DOM (malformed markup or an unknown component).
pub fn str_to_dom_unstyled<'a>(
    root_nodes: &'a [XmlNodeChild],
    component_map: &'a ComponentMap,
) -> Result<Dom, DomXmlParseError> {
    let html_node = get_html_node(root_nodes)?;
    let body_node = get_body_node(html_node.children.as_ref())?;

    let mut global_style = None;

    if let Some(head_node) = find_node_by_type(html_node.children.as_ref(), "head") {
        if let Some(style_node) = find_node_by_type(head_node.children.as_ref(), "style") {
            let text = style_node.get_text_content();
            if !text.is_empty() {
                let parsed_css = Css::from_string(text.into());
                global_style = Some(parsed_css);
            }
        }
    }

    // Build the DOM tree from the body node
    let body_dom = xml_node_to_dom_fast(body_node, component_map, false, 0)
        .map_err(DomXmlParseError::from)?;

    // Wrap in proper HTML structure (NodeType is imported at module top)
    let root_node_type = body_dom.root.node_type.clone();

    let mut full_dom = match root_node_type {
        NodeType::Html => body_dom,
        NodeType::Body => Dom::create_html().with_child(body_dom),
        _ => {
            let body_wrapper = Dom::create_body().with_child(body_dom);
            Dom::create_html().with_child(body_wrapper)
        }
    };

    // Attach CSS to the Dom's css field instead of applying it immediately
    if let Some(css) = global_style {
        full_dom.css = alloc::vec![css].into();
    }

    Ok(full_dom)
}

/// Parses an XML string and returns a `String`, which contains the Rust source code
/// (i.e. it compiles the XML to valid Rust)
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
/// # Errors
///
/// Returns an error if the XML cannot be parsed or compiled to Rust code.
pub fn str_to_rust_code<'a>(
    root_nodes: &'a [XmlNodeChild],
    imports: &str,
    component_map: &'a ComponentMap,
) -> Result<String, CompileError> {
    let html_node = get_html_node(root_nodes)?;
    let body_node = get_body_node(html_node.children.as_ref())?;
    let mut global_style = Css::empty();

    if let Some(head_node) = find_node_by_type(html_node.children.as_ref(), "head") {
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
        body_node,
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
        .map(|l| format!("        {l}"))
        .collect::<Vec<String>>()
        .join("\r\n");

    // NOTE: `css_blocks` / `extra_blocks` are no longer emitted — per-node styles
    // are now inlined as `.with_css("..")` strings (public API) rather than as
    // `const CSS_MATCH_*: NodeDataInlineCssPropertyVec` blocks (that API was
    // removed in 32d44ed8a). The maps stay in the signatures for compatibility.
    let _ = (&css_blocks, &extra_blocks);

    let main_func = "

use azul::{
    app::{App, AppConfig},
    dom::Dom,
    callbacks::{RefAny, LayoutCallbackInfo},
    window::WindowCreateOptions,
};

struct Data { }

extern \"C\" fn render(_: RefAny, _: LayoutCallbackInfo) -> Dom {
    crate::ui::render()
}

fn main() {
    let config = AppConfig::create();
    let app = App::create(RefAny::new(Data { }), config);
    let window = WindowCreateOptions::create(render);
    app.run(window);
}";

    let ui_module = format!(
        "#[allow(unused_imports)]\r\npub mod ui {{

    use azul::prelude::*;
    use azul::dom::{{NodeType, TabIndex, SmallAriaInfo}};
    use azul::str::String as AzString;

    pub fn render() -> Dom {{\r\n{app_source}\r\n    }}\r\n}}"
    );
    let source_code = format!(
        "#![windows_subsystem = \"windows\"]\r\n//! Auto-generated UI source \
         code\r\n{}\r\n{}\r\n\r\n{}{}",
        imports,
        compile_components(Vec::new()), // no user-defined components to compile
        ui_module,
        main_func,
    );

    Ok(source_code)
}

// Compile all components to source code
#[allow(clippy::needless_pass_by_value)] // owned azul value taken by value (public API / ownership-transfer convention)
fn compile_components(
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
            let name = &normalize_casing(name);
            let f = compile_component(name, function_args, function_body)
                .lines()
                .map(|l| format!("    {l}"))
                .collect::<Vec<String>>()
                .join("\r\n");

            // let css_blocks = ...

            format!(
                "#[allow(unused_imports)]\r\npub mod {name} {{\r\n    use azul::dom::Dom;\r\n    use \
                 azul::str::String as AzString;\r\n{f}\r\n}}"
            )
        })
        .collect::<Vec<String>>()
        .join("\r\n\r\n");

    let cs = cs
        .lines()
        .map(|l| format!("    {l}"))
        .collect::<Vec<String>>()
        .join("\r\n");

    if cs.is_empty() {
        cs
    } else {
        format!("pub mod components {{\r\n{cs}\r\n}}")
    }
}

fn format_component_args(component_args: &ComponentArgumentVec) -> String {
    let mut args = component_args
        .iter()
        .map(|a| format!("{}: {}", a.name, a.arg_type))
        .collect::<Vec<String>>();

    args.sort_by(|a, b| b.cmp(a));

    args.join(", ")
}

#[must_use] pub fn compile_component(
    component_name: &str,
    component_args: &ComponentArguments,
    component_function_body: &str,
) -> String {
    let component_name = &normalize_casing(component_name);
    let function_args = format_component_args(&component_args.args);
    let component_function_body = component_function_body
        .lines()
        .map(|l| format!("    {l}"))
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

/// Parse an SVG numeric attribute value to f32.
fn parse_svg_float(attr: Option<&AzString>) -> Option<f32> {
    attr?.as_str().trim().parse::<f32>().ok()
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

/// Fast XML to Dom conversion that builds Dom tree directly without intermediate `StyledDom`
/// This is O(n) instead of O(n²) for large documents
/// Apply the shared set of XML attributes onto a single [`NodeData`] node.
///
/// Handles `<img src>` rebuild, `id`/`class`, `focusable`, `tabindex`, inline
/// `style`, and SVG-shape geometry — the block that was previously duplicated
/// verbatim between [`xml_node_to_dom_fast`] (operating on `dom.root`) and
/// [`xml_node_to_fast_dom`] (operating on the arena `NodeData`). `component_name`
/// must already be normalized (lowercased); the caller computes `child_inside_svg`.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
fn apply_xml_node_attributes(
    node: &mut crate::dom::NodeData,
    xml_node: &XmlNode,
    component_name: &str,
    inside_svg: bool,
) {
    use crate::dom::{IdOrClass, NodeType, TabIndex};

    // `<img src="...">`: rebuild the placeholder Image node so its `NullImage`
    // carries the `src` string (as UTF-8 bytes in `tag`). The bytes are NOT
    // resolved here — a downstream renderer (printpdf, the compositor, ...) uses
    // the tag to look up and embed the actual image. Optional `width`/`height`
    // attributes set the intrinsic size used for layout (CSS still overrides).
    if component_name == "img" {
        if let Some(src) = xml_node.attributes.get_key("src") {
            let width = xml_node
                .attributes
                .get_key("width")
                .and_then(|w| {
                    w.as_str()
                        .trim()
                        .trim_end_matches("px")
                        .trim()
                        .parse::<usize>()
                        .ok()
                })
                .unwrap_or(0);
            let height = xml_node
                .attributes
                .get_key("height")
                .and_then(|h| {
                    h.as_str()
                        .trim()
                        .trim_end_matches("px")
                        .trim()
                        .parse::<usize>()
                        .ok()
                })
                .unwrap_or(0);
            let image_ref = crate::resources::ImageRef::null_image(
                width,
                height,
                crate::resources::RawImageFormat::RGBA8,
                src.as_str().as_bytes().to_vec(),
            );
            node
                .set_node_type(NodeType::Image(azul_css::css::BoxOrStatic::heap(image_ref)));
        }
    }

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
        node.set_ids_and_classes(ids_and_classes.into());
    }

    // Handle focusable attribute
    if let Some(focusable) = xml_node
        .attributes
        .get_key("focusable")
        .and_then(|f| parse_bool(f.as_str()))
    {
        if focusable { node.set_tab_index(TabIndex::Auto) } else { node.set_tab_index(TabIndex::NoKeyboardFocus) }
    }

    // Handle tabindex attribute
    if let Some(tab_index) = xml_node
        .attributes
        .get_key("tabindex")
        .and_then(|val| val.parse::<isize>().ok())
    {
        match tab_index {
            0 => node.set_tab_index(TabIndex::Auto),
            i if i > 0 => node.set_tab_index(TabIndex::OverrideInParent(u32::try_from(i).unwrap_or(u32::MAX))),
            _ => node.set_tab_index(TabIndex::NoKeyboardFocus),
        }
    }

    // Table cell span attributes (`colspan` / `rowspan`).
    apply_cell_span_attributes(node, xml_node);

    // HTML `dir` attribute → the `direction` CSS property (dir="rtl"/"ltr"). Without
    // this, dir="rtl" (the common way to set RTL in HTML) had no effect. Appended
    // BEFORE the inline `style` below so author style still wins on equal specificity.
    let dir_prop = xml_node.attributes.get_key("dir").and_then(|d| {
        let v = d.as_str().trim();
        if v.eq_ignore_ascii_case("rtl") {
            Some(azul_css::props::style::StyleDirection::Rtl)
        } else if v.eq_ignore_ascii_case("ltr") {
            Some(azul_css::props::style::StyleDirection::Ltr)
        } else {
            None
        }
    });

    // Handle inline style attribute (and the mapped `dir` attribute above)
    let style_attr = xml_node.attributes.get_key("style");
    if style_attr.is_some() || dir_prop.is_some() {
        use azul_css::dynamic_selector::CssPropertyWithConditions;
        let css_key_map = azul_css::props::property::get_css_key_map();
        let mut props: Vec<CssPropertyWithConditions> = Vec::new();
        if let Some(dir) = dir_prop {
            props.push(CssPropertyWithConditions::simple(
                azul_css::props::property::CssProperty::Direction(
                    azul_css::css::CssPropertyValue::Exact(dir),
                ),
            ));
        }
        if let Some(style) = style_attr {
            let mut attributes = Vec::new();
            for s in style.as_str().split(';') {
                let mut s = s.split(':');
                let Some(key) = s.next() else {
                    continue;
                };
                let Some(value) = s.next() else {
                    continue;
                };
                // Called for its side effect (writes parsed props into `attributes`);
                // the returned value is intentionally discarded.
                drop(azul_css::parser2::parse_css_declaration(
                    key.trim(),
                    value.trim(),
                    azul_css::parser2::ErrorLocationRange::default(),
                    &css_key_map,
                    &mut Vec::new(),
                    &mut attributes,
                ));
            }
            props.extend(attributes.into_iter().filter_map(|s| match s {
                CssDeclaration::Static(s) => Some(CssPropertyWithConditions::simple(s)),
                CssDeclaration::Dynamic(_) => None,
            }));
        }
        if !props.is_empty() {
            node.set_css_props(props.into());
        }
    }

    // Handle SVG shape elements when inside an <svg> context
    let tag = component_name;
    let is_svg_shape = inside_svg
        && matches!(
            tag,
            "path" | "circle" | "rect" | "ellipse" | "line" | "polygon" | "polyline"
        );

    if is_svg_shape {
        let clip = match tag {
            "path" => xml_node
                .attributes
                .get_key("d")
                .and_then(|d| crate::path_parser::parse_svg_path_d(d.as_str()).ok()),
            "circle" => {
                let cx = parse_svg_float(xml_node.attributes.get_key("cx")).unwrap_or(0.0);
                let cy = parse_svg_float(xml_node.attributes.get_key("cy")).unwrap_or(0.0);
                let r = parse_svg_float(xml_node.attributes.get_key("r")).unwrap_or(0.0);
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
                let x = parse_svg_float(xml_node.attributes.get_key("x")).unwrap_or(0.0);
                let y = parse_svg_float(xml_node.attributes.get_key("y")).unwrap_or(0.0);
                let w = parse_svg_float(xml_node.attributes.get_key("width")).unwrap_or(0.0);
                let h = parse_svg_float(xml_node.attributes.get_key("height")).unwrap_or(0.0);
                let rx = parse_svg_float(xml_node.attributes.get_key("rx")).unwrap_or(0.0);
                let ry = parse_svg_float(xml_node.attributes.get_key("ry")).unwrap_or(rx);
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
                let cx = parse_svg_float(xml_node.attributes.get_key("cx")).unwrap_or(0.0);
                let cy = parse_svg_float(xml_node.attributes.get_key("cy")).unwrap_or(0.0);
                let rx = parse_svg_float(xml_node.attributes.get_key("rx")).unwrap_or(0.0);
                let ry = parse_svg_float(xml_node.attributes.get_key("ry")).unwrap_or(0.0);
                if rx > 0.0 && ry > 0.0 {
                    // Approximate ellipse with 4 cubic beziers (using rx for x-kappa, ry for y-kappa)
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
                let x1 = parse_svg_float(xml_node.attributes.get_key("x1")).unwrap_or(0.0);
                let y1 = parse_svg_float(xml_node.attributes.get_key("y1")).unwrap_or(0.0);
                let x2 = parse_svg_float(xml_node.attributes.get_key("x2")).unwrap_or(0.0);
                let y2 = parse_svg_float(xml_node.attributes.get_key("y2")).unwrap_or(0.0);
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
            "polygon" | "polyline" => xml_node
                .attributes
                .get_key("points")
                .and_then(|pts| parse_svg_points(pts.as_str(), tag == "polygon")),
            _ => None,
        };

        if let Some(mp) = clip {
            node.set_svg_data(crate::dom::SvgNodeData::Path(mp));
        }
    }
}

/// Parse the HTML `colspan` / `rowspan` presentational attributes into
/// `AttributeType`s on the node. The table layout reads them back via
/// `get_cell_spans`. Without this the XML→DOM conversion dropped them and every
/// cell defaulted to span 1, so `<th colspan="2">` only covered one column.
/// Parsed unconditionally — non-cell elements simply don't carry these attributes.
fn apply_cell_span_attributes(node: &mut crate::dom::NodeData, xml_node: &XmlNode) {
    let mut spans = Vec::new();
    if let Some(n) = xml_node
        .attributes
        .get_key("colspan")
        .and_then(|v| v.as_str().trim().parse::<i32>().ok())
    {
        spans.push(crate::dom::AttributeType::ColSpan(n));
    }
    if let Some(n) = xml_node
        .attributes
        .get_key("rowspan")
        .and_then(|v| v.as_str().trim().parse::<i32>().ok())
    {
        spans.push(crate::dom::AttributeType::RowSpan(n));
    }
    if !spans.is_empty() {
        let mut v = node.attributes().clone().into_library_owned_vec();
        v.extend(spans);
        node.set_attributes(v.into());
    }
}

#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
// component_map is threaded through the whole fast-DOM pipeline for parity with the
// component-expanding interpreter path (see ~xml.rs:2845); this fast path never expands
// components, so it only forwards the map into recursive calls. Removing it here would
// cascade unused-param removals up the entire pipeline.
#[allow(clippy::only_used_in_recursion)]
fn xml_node_to_dom_fast<'a>(
    xml_node: &'a XmlNode,
    component_map: &'a ComponentMap,
    inside_svg: bool,
    depth: usize,
) -> Result<Dom, RenderDomError> {
    use crate::dom::Dom;

    let component_name = normalize_casing(&xml_node.node_type);

    // Look up the component definition
    let node_type = tag_to_node_type(&component_name);
    let mut dom = Dom::create_node(node_type);

    apply_xml_node_attributes(&mut dom.root, xml_node, &component_name, inside_svg);

    let child_inside_svg = inside_svg || component_name == "svg";

    // AUDIT 2026-07-08: bound recursion depth to avoid a native stack overflow on
    // pathologically deep markup. At the cap, this node is emitted without its
    // children (truncation) rather than crashing the process.
    // AUDIT-TODO: a worklist-based iterative builder would preserve deep subtrees.
    if depth >= MAX_XML_NESTING_DEPTH {
        return Ok(dom);
    }

    // Recursively convert children
    let mut children = Vec::new();
    for child in xml_node.children.as_ref() {
        match child {
            XmlNodeChild::Element(child_node) => {
                let child_dom =
                    xml_node_to_dom_fast(child_node, component_map, child_inside_svg, depth + 1)?;
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

/// Builder for arena-based DOM construction (`FastDom`).
/// Builds two parallel Vecs (hierarchy + `node_data`) in a single DFS pass.
#[derive(Debug)]
pub struct CompactDomBuilder {
    hierarchy: Vec<crate::styled_dom::NodeHierarchyItem>,
    node_data: Vec<crate::dom::NodeData>,
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
    #[must_use] pub const fn new() -> Self {
        Self {
            hierarchy: Vec::new(),
            node_data: Vec::new(),
            css: Vec::new(),
            stack: Vec::new(),
        }
    }

    #[must_use] pub fn with_capacity(cap: usize) -> Self {
        Self {
            hierarchy: Vec::with_capacity(cap),
            node_data: Vec::with_capacity(cap),
            css: Vec::new(),
            stack: Vec::new(),
        }
    }

    /// Open a new element node. Must be paired with `close_node()`.
    pub fn open_node(&mut self, node_data: crate::dom::NodeData) {
        use crate::id::NodeId;
        use crate::styled_dom::NodeHierarchyItem;

        let idx = self.hierarchy.len();

        // Determine parent from stack
        let parent_raw = if let Some(&(parent_idx, _)) = self.stack.last() {
            NodeId::into_raw(&Some(NodeId::new(parent_idx)))
        } else {
            0 // No parent (root)
        };

        // Determine previous sibling from parent's last child tracking
        let prev_sibling_raw = if let Some(&(_, prev_child)) = self.stack.last() {
            prev_child
                .map_or(0, |pi| NodeId::into_raw(&Some(NodeId::new(pi))))
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
            self.hierarchy[idx].last_child = last_child_idx
                .map_or(0, |lc| NodeId::into_raw(&Some(NodeId::new(lc))));
        }
    }

    /// Add a leaf node (text, br, hr, etc.) that has no children.
    pub fn add_leaf(&mut self, node_data: crate::dom::NodeData) {
        self.open_node(node_data);
        self.close_node();
    }

    /// Add a CSS stylesheet scoped to a node ID.
    pub fn add_css(&mut self, node_id: usize, css: Css) {
        self.css.push(crate::dom::CssWithNodeId { node_id, css });
    }

    /// Finish building and produce a `FastDom`.
    #[must_use] pub fn finish(self) -> crate::dom::FastDom {
        crate::dom::FastDom {
            node_hierarchy: self.hierarchy.into(),
            node_data: self.node_data.into(),
            css: self.css.into(),
        }
    }
}

/// Convert an XML node tree into a `FastDom` (arena-based) in a single DFS pass.
/// This is the fast path equivalent of `xml_node_to_dom_fast`.
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
// See xml_node_to_dom_fast: component_map is forwarded for pipeline parity, not read here.
#[allow(clippy::only_used_in_recursion)]
fn xml_node_to_fast_dom<'a>(
    xml_node: &'a XmlNode,
    component_map: &'a ComponentMap,
    inside_svg: bool,
    builder: &mut CompactDomBuilder,
    depth: usize,
) -> Result<(), RenderDomError> {
    use crate::dom::NodeData;

    let component_name = normalize_casing(&xml_node.node_type);
    let node_type = tag_to_node_type(&component_name);
    let mut node_data = NodeData::create_node(node_type);

    apply_xml_node_attributes(&mut node_data, xml_node, &component_name, inside_svg);

    let child_inside_svg = inside_svg || component_name == "svg";

    // Open this node in the builder
    builder.open_node(node_data);

    // AUDIT 2026-07-08: bound recursion depth to avoid a native stack overflow on
    // pathologically deep markup. At the cap, children are dropped (the node is
    // still opened+closed) rather than crashing the process.
    // AUDIT-TODO: a worklist-based iterative builder would preserve deep subtrees.
    if depth < MAX_XML_NESTING_DEPTH {
        // Recursively convert children
        for child in xml_node.children.as_ref() {
            match child {
                XmlNodeChild::Element(child_node) => {
                    xml_node_to_fast_dom(
                        child_node,
                        component_map,
                        child_inside_svg,
                        builder,
                        depth + 1,
                    )?;
                }
                XmlNodeChild::Text(text) => {
                    builder.add_leaf(NodeData::create_text(AzString::from(text.as_str())));
                }
            }
        }
    }

    // Close this node
    builder.close_node();

    Ok(())
}

/// Render a DOM from an XML body node using the fast arena-based path.
/// Builds a `FastDom` directly (no tree intermediary), then creates `StyledDom`.
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
fn render_dom_from_body_node_fast<'a>(
    body_node: &'a XmlNode,
    mut global_css: Option<Css>,
    component_map: &'a ComponentMap,
    max_width: Option<f32>,
) -> Result<StyledDom, RenderDomError> {
    use crate::dom::{NodeData, NodeType};

    let mut builder = CompactDomBuilder::new();

    // Build the HTML > Body wrapper + body content in one pass
    // Open <html>
    builder.open_node(NodeData::create_node(NodeType::Html));
    // Open <body> (the body_node content goes inside)
    xml_node_to_fast_dom(body_node, component_map, false, &mut builder, 0)?;
    // Close <html>
    builder.close_node();

    // Collect CSS rules from each source.
    let mut combined_rules: Vec<CssRuleBlock> = Vec::new();
    if let Some(max_width) = max_width {
        let max_width_css =
            Css::from_string(format!("html {{ max-width: {max_width}px; }}").into());
        combined_rules.extend(max_width_css.rules.into_library_owned_vec());
    }
    if let Some(css) = global_css.take() {
        combined_rules.extend(css.rules.into_library_owned_vec());
    }
    let combined_css = Css::new(combined_rules);

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

fn set_stringified_attributes(
    dom_string: &mut String,
    xml_attributes: &XmlAttributeMap,
    filtered_xml_attributes: &ComponentArgumentVec,
    tabs: usize,
) {
    let t0 = String::from("    ").repeat(tabs);
    let t = String::from("    ").repeat(tabs + 1);

    // push ids and classes as chained `.with_id("..")` / `.with_class("..")`
    // calls (public builder API; both take `Into<AzString>`, so bare &str works).
    let _ = &t;
    for id in xml_attributes
        .get_key("id")
        .map(|s| s.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default()
    {
        let _ = write!(
            dom_string,
            "\r\n{}.with_id(\"{}\")",
            t0,
            format_args_dynamic(id, filtered_xml_attributes)
        );
    }

    for class in xml_attributes
        .get_key("class")
        .map(|s| s.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default()
    {
        let _ = write!(
            dom_string,
            "\r\n{}.with_class(\"{}\")",
            t0,
            format_args_dynamic(class, filtered_xml_attributes)
        );
    }

    if let Some(focusable) = xml_attributes
        .get_key("focusable")
        .map(|f| format_args_dynamic(f, filtered_xml_attributes))
        .and_then(|f| parse_bool(&f))
    {
        if focusable { let _ = write!(dom_string, "\r\n{t}.with_tab_index(TabIndex::Auto)"); } else { let _ = write!(dom_string,
            "\r\n{t}.with_tab_index(TabIndex::NoKeyboardFocus)"
        ); }
    }

    if let Some(tab_index) = xml_attributes
        .get_key("tabindex")
        .map(|val| format_args_dynamic(val, filtered_xml_attributes))
        .and_then(|val| val.parse::<isize>().ok())
    {
        match tab_index {
            0 => { let _ = write!(dom_string, "\r\n{t}.with_tab_index(TabIndex::Auto)"); },
            i if i > 0 => { let _ = write!(dom_string,
                "\r\n{}.with_tab_index(TabIndex::OverrideInParent({}))",
                t, usize::try_from(i).unwrap_or(0)
            ); },
            _ => { let _ = write!(dom_string,
                "\r\n{t}.with_tab_index(TabIndex::NoKeyboardFocus)"
            ); },
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
#[must_use] pub fn split_dynamic_string(input: &str) -> Vec<DynamicItem> {
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
                    .map(|q| &q.arg_type) {
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
///     ComponentArgument { name: AzString::from("a"), arg_type: AzString::from("value1") },
///     ComponentArgument { name: AzString::from("b"), arg_type: AzString::from("value2") },
/// ].into();
///
/// let initial = "hello {a}, {b}{{ {c} }}";
/// let expected = "hello value1, value2{ {c} }".to_string();
/// assert_eq!(format_args_dynamic(initial, &variables), expected);
/// ```
///
/// Note: the number (0, 1, etc.) is the order of the argument, it is irrelevant for
/// runtime formatting, only important for keeping the component / function arguments
/// in order when compiling the arguments to Rust code
#[must_use] pub fn format_args_dynamic(input: &str, variables: &ComponentArgumentVec) -> String {
    let dynamic_str_items = split_dynamic_string(input);
    combine_and_replace_dynamic_items(&dynamic_str_items, variables)
}

/// Decode a numeric character reference body (the part between `&` and `;`),
/// e.g. `"#65"` -> `'A'`, `"#x41"` -> `'A'`. Returns `None` if it is not a valid
/// numeric reference.
fn decode_numeric_entity(entity: &str) -> Option<char> {
    let num = entity.strip_prefix('#')?;
    let code = if let Some(hex) = num.strip_prefix(['x', 'X']) {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        num.parse::<u32>().ok()?
    };
    char::from_u32(code)
}

/// Decode the common HTML/XML entities in a single left-to-right pass.
///
/// Handles `&lt;` `&gt;` `&amp;` `&quot;` `&apos;` and numeric references
/// (`&#NN;` / `&#xHH;`). `&nbsp;` and any unrecognized `&...;` sequence are left
/// verbatim. The single pass guarantees `&amp;` never double-decodes a following
/// entity. See [`prepare_string`] for why `&nbsp;` is deliberately preserved.
fn decode_entities(input: &str) -> String {
    // Longest handled entity body is a hex numeric ref like `#x10FFFF` (8 bytes);
    // cap the `;` search window so a stray `&` far from a `;` stays cheap.
    const MAX_ENTITY_BODY: usize = 12;

    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < input.len() {
        if bytes[i] == b'&' {
            if let Some(semi_rel) = input[i + 1..].find(';') {
                if semi_rel <= MAX_ENTITY_BODY {
                    let body = &input[i + 1..i + 1 + semi_rel];
                    let end = i + 1 + semi_rel; // index of ';'
                    // Leave &nbsp; for the per-line pass in prepare_string.
                    if body.eq_ignore_ascii_case("nbsp") {
                        out.push_str(&input[i..=end]);
                        i = end + 1;
                        continue;
                    }
                    let decoded = match body {
                        "lt" => Some('<'),
                        "gt" => Some('>'),
                        "amp" => Some('&'),
                        "quot" => Some('"'),
                        "apos" => Some('\''),
                        _ => decode_numeric_entity(body),
                    };
                    if let Some(c) = decoded {
                        out.push(c);
                        i = end + 1;
                        continue;
                    }
                }
            }
            // Not a recognized entity: emit the '&' literally.
            out.push('&');
            i += 1;
        } else {
            // Copy one whole UTF-8 char (i is always on a char boundary here).
            let ch = input[i..].chars().next().unwrap_or('\u{FFFD}');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

// NOTE: Two sequential returns count as a single return, while single returns get ignored.
#[must_use] pub fn prepare_string(input: &str) -> String {
    const SPACE: &str = " ";
    const RETURN: &str = "\n";

    let input = input.trim();

    if input.is_empty() {
        return String::new();
    }

    // AUDIT 2026-07-08: previously only `&lt;`/`&gt;` were decoded. Decode the full
    // common named-entity set (`&lt;` `&gt;` `&amp;` `&quot;` `&apos;`) plus numeric
    // references (`&#NN;` decimal and `&#xHH;` hex) in a single left-to-right pass.
    // A single pass is used deliberately so `&amp;` cannot double-decode a following
    // entity (e.g. "&amp;lt;" -> literal "&lt;", not "<"). `&nbsp;` is intentionally
    // left untouched here so the per-line pass below (which runs AFTER trimming) can
    // still turn it into a space that survives leading/trailing trim.
    let input = decode_entities(input);

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
                final_lines.push(line.to_string());
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
#[must_use] pub fn parse_bool(input: &str) -> Option<bool> {
    match input {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct CssMatcher {
    path: Vec<CssPathSelector>,
    indices_in_parent: Vec<usize>,
    children_length: Vec<usize>,
}

impl CssMatcher {
    fn get_hash(&self) -> u64 {
        use core::hash::Hash;

        use core::hash::Hasher;

        let mut hasher = crate::hash::DefaultHasher::new();
        for p in &self.path {
            p.hash(&mut hasher);
        }
        hasher.finish()
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
                    }
                    cur_pathgroup_scan += 1;
                    cur_selfgroup_scan += n;
                    path_group = path_groups[cur_pathgroup_scan].clone();
                }
                None => return false, // group was not found in remaining items
            }
        }

        // only return true if all path_groups matched
        cur_pathgroup_scan == path_groups.len() - 1
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
    use azul_css::css::{CssNthChildSelector, CssPathPseudoSelector, CssPathSelector::{Global, PseudoSelector, Type, Class, Id}};

    for selector in a {
        match selector {
            // always matches
            Global |
PseudoSelector(CssPathPseudoSelector::Hover | CssPathPseudoSelector::Active |
CssPathPseudoSelector::Focus) => {}

            Type(tag) => {
                if !b.iter().any(|t| **t == Type(*tag)) {
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
                if !idx_in_parent.is_multiple_of(2) {
                    return false;
                }
            }
            PseudoSelector(CssPathPseudoSelector::NthChild(CssNthChildSelector::Odd)) => {
                if idx_in_parent.is_multiple_of(2) {
                    return false;
                }
            }
            PseudoSelector(CssPathPseudoSelector::NthChild(CssNthChildSelector::Pattern(p))) => {
                if !idx_in_parent.saturating_sub(p.offset as usize).is_multiple_of(p.pattern_repeat as usize)
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

#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
/// # Errors
///
/// Returns an error if the body node cannot be compiled to Rust code.
pub fn compile_body_node_to_rust_code<'a>(
    body_node: &'a XmlNode,
    component_map: &'a ComponentMap,
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
        // Track property types for the helper-const machinery, then emit the
        // matched declarations as an inline CSS string. (The old path emitted a
        // `const CSS_MATCH_*: NodeDataInlineCssPropertyVec` + `.with_inline_css_props`,
        // but that API was removed in 32d44ed8a; `.with_css(<str>)` is the
        // current equivalent and parses pseudo blocks too.)
        for css_block in &css_blocks_for_this_node {
            for declaration in css_block.block.declarations.as_ref() {
                let prop = match declaration {
                    CssDeclaration::Static(s) => s,
                    CssDeclaration::Dynamic(d) => &d.default_value,
                };
                extra_blocks.insert_from_css_property(prop);
            }
        }

        let inline_css = css_blocks_to_inline_string(&css_blocks_for_this_node);
        if !inline_css.is_empty() {
            let escaped = inline_css.replace('\\', "\\\\").replace('"', "\\\"");
            let _ = write!(dom_string, "\r\n{t2}.with_css(\"{escaped}\")");
        }
        let _ = (&mut *css_blocks, matcher_hash); // retained for signature compat
    }

    if !body_node.children.as_ref().is_empty() {
        use azul_css::codegen::format::GetHash;
        let children_hash = body_node.children.as_ref().get_hash();
        dom_string.push_str("\r\n.with_children(vec![\r\n");

        for (child_idx, child) in body_node.children.as_ref().iter().enumerate() {
            match child {
                XmlNodeChild::Element(child_node) => {
                    let mut matcher = matcher.clone();
                    matcher.path.push(CssPathSelector::Children);
                    matcher.indices_in_parent.push(child_idx);
                    matcher.children_length.push(body_node.children.len());

                    let _ = write!(dom_string,
                        "{}{},\r\n",
                        t,
                        compile_node_to_rust_code_inner(
                            child_node,
                            component_map,
                            1,
                            extra_blocks,
                            css_blocks,
                            css,
                            matcher,
                        )?
                    );
                }
                XmlNodeChild::Text(text) => {
                    let text = text.trim();
                    if !text.is_empty() {
                        let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
                        let _ = write!(dom_string,
                            "{t}Dom::create_text(\"{escaped}\"),\r\n"
                        );
                    }
                }
            }
        }
        let _ = write!(dom_string, "\r\n{t}])");
    }

    let dom_string = dom_string.trim();
    Ok(dom_string.to_string())
}

/// Serialize the CSS blocks matched for a node into one inline CSS string for
/// `Dom::with_css(...)`. `with_css` parses via `Css::parse_inline`, which runs
/// the full selector+nesting machinery, so `:hover`/`:active`/`:focus` are
/// emitted as nested pseudo blocks and round-trip faithfully; plain rules are
/// emitted flat as `key: value;` (via `CssProperty::key()` / `value()`).
fn css_blocks_to_inline_string(blocks: &[CssBlock]) -> String {
    fn decls_of(block: &CssBlock) -> Vec<String> {
        block
            .block
            .declarations
            .as_ref()
            .iter()
            .map(|d| {
                let prop = match d {
                    CssDeclaration::Static(s) => s,
                    CssDeclaration::Dynamic(dy) => &dy.default_value,
                };
                format!("{}: {};", prop.key(), prop.value())
            })
            .collect()
    }

    let mut normal: Vec<String> = Vec::new();
    let mut pseudo: Vec<String> = Vec::new();
    for block in blocks {
        let pseudo_sel = match block.ending {
            Some(CssPathPseudoSelector::Hover) => Some(":hover"),
            Some(CssPathPseudoSelector::Active) => Some(":active"),
            Some(CssPathPseudoSelector::Focus) => Some(":focus"),
            _ => None,
        };
        match pseudo_sel {
            None => normal.extend(decls_of(block)),
            Some(sel) => pseudo.push(format!("{} {{ {} }}", sel, decls_of(block).join(" "))),
        }
    }

    let mut parts = normal;
    parts.extend(pseudo);
    parts.join(" ")
}

fn get_css_blocks(css: &Css, matcher: &CssMatcher) -> Vec<CssBlock> {
    let mut blocks = Vec::new();

    for css_block in css.rules.as_ref() {
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

    blocks
}

fn compile_and_format_dynamic_items(input: &[DynamicItem]) -> String {
    use self::DynamicItem::{Var, Str};
    if input.is_empty() {
        String::from("AzString::from_const_str(\"\")")
    } else if input.len() == 1 {
        // common: there is only one "dynamic item" - skip the "format!()" macro
        match &input[0] {
            Var { name, format_spec } => {
                let var_name = normalize_casing(name.trim());
                if let Some(spec) = format_spec {
                    format!("format!(\"{{:{spec}}}\", {var_name}).into()")
                } else {
                    var_name
                }
            }
            Str(s) => format!("AzString::from_const_str(\"{s}\")"),
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
                        let _ = write!(formatted_str, "{{{variable_name}:{spec}}}");
                    } else {
                        let _ = write!(formatted_str, "{{{variable_name}}}");
                    }
                    variables.push(variable_name.clone());
                }
                Str(s) => {
                    let s = s.replace('"', "\\\"");
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

#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
// component_map is forwarded through the codegen recursion for parity with the
// component-expanding path; this Rust-codegen path only threads it into recursive calls.
#[allow(clippy::only_used_in_recursion)]
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
fn compile_node_to_rust_code_inner(
    node: &XmlNode,
    component_map: &ComponentMap,
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

    // Look up the CSS NodeTypeTag
    let node_type_tag = tag_to_node_type_tag(&component_name);
    let node_type = CssPathSelector::Type(node_type_tag);

    // Emit a plain `create_node(<Tag>)` for the base node. Do NOT route through
    // the component `compile_fn`: its Rust arm bakes inline text into a
    // `.with_children(..)`, which the child-walk below would then OVERWRITE with
    // a second `.with_children(..)` — silently dropping the text on any node
    // that has BOTH text and element children. The child-walk handles ALL
    // children (text + elements) in order, so the base node must stay childless.
    // Interactive/data tags (Button/Input/…) whose NodeType carries data fall
    // back to `div`, matching the C/C++/Python walkers (`safe_container_tag`).
    let ctor = analyze_node_ctor(&component_name, node);
    let mut dom_string = ctor.render_rust().map_or_else(|| {
        let tag = safe_container_tag(&format!("{:?}", tag_to_node_type(&component_name)));
        format!("{t2}Dom::create_node(NodeType::{tag})")
    }, |expr| format!("{t2}{expr}"));

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
        // Track property types for the helper-const machinery, then emit the
        // matched declarations as an inline CSS string. (The old path emitted a
        // `const CSS_MATCH_*: NodeDataInlineCssPropertyVec` + `.with_inline_css_props`,
        // but that API was removed in 32d44ed8a; `.with_css(<str>)` is the
        // current equivalent and parses pseudo blocks too.)
        for css_block in &css_blocks_for_this_node {
            for declaration in css_block.block.declarations.as_ref() {
                let prop = match declaration {
                    CssDeclaration::Static(s) => s,
                    CssDeclaration::Dynamic(d) => &d.default_value,
                };
                extra_blocks.insert_from_css_property(prop);
            }
        }

        let inline_css = css_blocks_to_inline_string(&css_blocks_for_this_node);
        if !inline_css.is_empty() {
            let escaped = inline_css.replace('\\', "\\\\").replace('"', "\\\"");
            let _ = write!(dom_string, "\r\n{t2}.with_css(\"{escaped}\")");
        }
        let _ = (&mut *css_blocks, matcher_hash); // retained for signature compat
    }

    set_stringified_attributes(
        &mut dom_string,
        &node.attributes,
        &ComponentArgumentVec::new(),
        tabs,
    );

    // Text folded into the ctor (Tier A/C) is skipped, as is a `<caption>`
    // already injected by `create_table`.
    let mut caption_skipped = false;
    let mut children_string = node
        .children
        .as_ref()
        .iter()
        .enumerate()
        .filter_map(|(child_idx, c)| match c {
            XmlNodeChild::Element(child_node) => {
                if ctor.skip_caption()
                    && !caption_skipped
                    && child_node.node_type.as_str().eq_ignore_ascii_case("caption")
                {
                    caption_skipped = true;
                    return None;
                }
                let mut matcher = matcher.clone();
                matcher.path.push(CssPathSelector::Children);
                matcher.indices_in_parent.push(child_idx);
                matcher.children_length.push(node.children.len());

                Some(compile_node_to_rust_code_inner(
                    child_node,
                    component_map,
                    tabs + 1,
                    extra_blocks,
                    css_blocks,
                    css,
                    matcher,
                ))
            }
            XmlNodeChild::Text(text) => {
                if ctor.consumes_text() {
                    return None;
                }
                let text = text.trim();
                if text.is_empty() {
                    None
                } else {
                    let t2 = String::from("    ").repeat(tabs);
                    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
                    Some(Ok(format!(
                        "{t2}Dom::create_text(\"{escaped}\")"
                    )))
                }
            }
        })
        .collect::<Result<Vec<_>, _>>()?
        .join(",\r\n");

    if !children_string.is_empty() {
        let _ = write!(dom_string,
            "\r\n{t2}.with_children(vec![\r\n{children_string}\r\n{t2}])"
        );
    }

    Ok(dom_string)
}

// ───────────────────────────────────────────────────────────────────────────
// Generic FLUENT DOM-builder emitter (C++ / Python).
//
// Rust has its own dedicated walker above (`compile_*_to_rust_code`). C++ and
// Python share this generic walker because their builder APIs are also fluent
// (`Dom::create_*().with_css(..).with_child(..)`); only the surface tokens
// differ, captured in `FluentSyntax`. Plain C is imperative and has its own
// walker (`compile_*_to_c_code`).
// ───────────────────────────────────────────────────────────────────────────

/// Tags with a zero-arg per-tag creator (`create_<tag>()` / `AzDom_create<Tag>()`
/// / `create_node(NodeType::<Tag>)`). Interactive / data elements (Button, Input,
/// Img, Select, Textarea, Label, A, Table, …) take constructor arguments, so an
/// exported page maps them to a plain `div` container (structure preserved; the
/// user re-wires behavior). Keep these CamelCase to match `NodeTypeTag` debug names.
const SAFE_CONTAINER_TAGS: &[&str] = &[
    // These must match the real `NodeType` Debug names exactly (the lookup below is a
    // string compare against `{:?}`). Six used to be mis-cased — "Blockquote",
    // "Colgroup", "Figcaption", "Tbody", "Tfoot", "Thead" — so those tags silently
    // degraded to "Div".
    "Abbr", "Acronym", "Address", "Article", "Aside", "B", "Bdi", "Bdo", "Big",
    "BlockQuote", "Body", "Br", "Caption", "Cite", "Code", "ColGroup", "Dd",
    "Del", "Dfn", "Dir", "Div", "Dl", "Dt", "Em", "Embed", "FigCaption",
    "Figure", "Footer", "H1", "H2", "H3", "H4", "H5", "H6", "Head", "Header",
    "Hr", "Html", "I", "Ins", "Kbd", "Li", "Link", "Main", "Map", "Mark",
    "Meta", "Nav", "Object", "Ol", "P", "Pre", "Q", "Rp", "Rt", "Rtc", "Ruby",
    "S", "Samp", "Script", "Section", "Small", "Span", "Strong", "Style", "Sub",
    "Sup", "Svg", "TBody", "Td", "TFoot", "Th", "THead", "Title", "Tr", "U",
    "Ul", "Var", "Wbr",
];

/// The CamelCase tag to actually emit a creator for: the tag itself if it has a
/// zero-arg creator, else `"Div"`.
fn safe_container_tag(tag_dbg: &str) -> &'static str {
    SAFE_CONTAINER_TAGS.iter().copied().find(|t| *t == tag_dbg).unwrap_or("Div")
}

// ───────────────────────────────────────────────────────────────────────────
// Semantic / accessibility-aware constructor selection.
//
// Instead of mapping every element to a plain `div`, an exported live page
// picks the *most specific* Azul constructor so the generated app keeps the
// page's semantics + accessibility tree:
//
//   • Tier A  `create_<tag>_with_text(text)` — a tag with a single text child
//             and no element children (P, Span, H1-H6, Li, Td, Code, …).
//   • Tier B  aria-only / void widgets (Details, Summary, Form, Canvas, Area,
//             …) — `create_<tag>(SmallAriaInfo::label(..))` when `aria-label`
//             is present, else `create_<tag>_no_a11y()`.
//   • Tier C  multi-arg widgets (Button, A, Label, Input, Select, Option,
//             Optgroup, Textarea, Table) — args pulled from HTML attributes.
//   • Tier D  scalar-driven widgets (Progress, Meter, Dialog) — the `*_no_a11y`
//             form with extracted numeric args (the full aria structs are
//             complex; the NoA11y form is simplest + correct).
//
// Every symbol emitted here is verified to exist in `target/codegen/azul.h` (C)
// and `azul20.hpp` (C++); anything else falls back to `safe_container_tag`
// (`div`). The four walkers share `analyze_node_ctor` and each renders the
// result with its own surface tokens.
// ───────────────────────────────────────────────────────────────────────────

/// A single positional argument of a semantic constructor. String payloads are
/// RAW — escaping happens at render time (matching the walkers).
#[derive(Debug, Clone)]
enum CtorArg {
    /// Plain string literal (`AzString` / `String` / `"…"`).
    Str(String),
    /// `SmallAriaInfo` built from an accessible label.
    Aria(String),
    /// `f32` numeric literal.
    Float(f32),
    /// `OptionString::Some(text)`.
    OptSome(String),
    /// `OptionString::None`.
    OptNone,
}

/// The constructor chosen for an element node.
enum NodeCtor {
    /// Plain container — keep each walker's existing `create_<tag>()` path.
    Plain,
    /// A specific semantic constructor.
    Semantic {
        /// Canonical CamelCase suffix after `create` / `AzDom_create`
        /// (e.g. `Button`, `ButtonNoA11y`, `PWithText`, `A`, `ANoA11y`).
        suffix: String,
        args: Vec<CtorArg>,
        /// The node's direct text is folded into the ctor — skip text children
        /// in the walk so it isn't emitted twice.
        consumes_text: bool,
        /// The table aria form injects its own `<caption>` child — drop the
        /// first literal `<caption>` element so it isn't duplicated.
        skip_caption: bool,
    },
}

/// Uppercase the first character (`button` → `Button`, `h1` → `H1`). HTML tags
/// are single lowercase tokens, so this yields the exact `AzDom_create<Suffix>`
/// spelling.
fn cap_first(tag: &str) -> String {
    let mut c = tag.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
}

/// CamelCase → `snake_case` for the C++/Python/Rust method names
/// (`ButtonNoA11y` → `button_no_a11y`, `PWithText` → `p_with_text`,
/// `ANoA11y` → `a_no_a11y`, `H1WithText` → `h1_with_text`).
fn camel_to_snake(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(char::is_ascii_lowercase);
            if prev.is_ascii_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_ascii_uppercase() && next_lower)
            {
                out.push('_');
            }
        }
        out.extend(ch.to_lowercase());
    }
    out
}

/// Escape `\` and `"` for a double-quoted string literal.
fn esc_lit(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Format an `f32` as a valid float literal with a decimal point (`1` → `1.0`).
fn fmt_f32_lit(f: f32) -> String {
    let s = format!("{f}");
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        format!("{s}.0")
    }
}

/// Joined, trimmed text of a node's *direct* text children (`"  Go  "` → `"Go"`).
fn node_direct_text(node: &XmlNode) -> String {
    node.children
        .as_ref()
        .iter()
        .filter_map(|c| match c {
            XmlNodeChild::Text(t) => {
                let t = t.trim();
                if t.is_empty() { None } else { Some(t.to_string()) }
            }
            XmlNodeChild::Element(_) => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Non-empty `aria-label` attribute value, if present.
fn node_aria_label(node: &XmlNode) -> Option<String> {
    node.attributes.get_key("aria-label").and_then(|v| {
        let v = v.as_str().trim();
        if v.is_empty() { None } else { Some(v.to_string()) }
    })
}

/// Attribute value, or `default` when absent.
fn node_attr_or(node: &XmlNode, key: &str, default: &str) -> String {
    node.attributes
        .get_key(key).map_or_else(|| default.to_string(), |v| v.as_str().to_string())
}

/// Attribute parsed as `f32`, or `default` when absent / unparsable.
fn node_attr_f32(node: &XmlNode, key: &str, default: f32) -> f32 {
    node.attributes
        .get_key(key)
        .and_then(|v| v.as_str().trim().parse::<f32>().ok())
        .unwrap_or(default)
}

/// Text of the node's first `<caption>` element child, if any (non-empty).
fn first_caption_text(node: &XmlNode) -> Option<String> {
    node.children.as_ref().iter().find_map(|c| match c {
        XmlNodeChild::Element(e) if e.node_type.as_str().eq_ignore_ascii_case("caption") => {
            let t = e.get_text_content();
            let t = t.trim();
            if t.is_empty() { None } else { Some(t.to_string()) }
        }
        _ => None,
    })
}

/// Tags with a single-arg `create_<tag>_with_text(text)` constructor (Tier A).
const WITH_TEXT_TAGS: &[&str] = &[
    "acronym", "b", "bdi", "bdo", "big", "blockquote", "cite", "code", "del",
    "dfn", "em", "h1", "h2", "h3", "h4", "h5", "h6", "i", "ins", "kbd", "li",
    "mark", "p", "pre", "rp", "rt", "s", "samp", "small", "span", "strong",
    "style", "sub", "sup", "td", "th", "title", "u", "var",
];

/// Pick the semantic constructor for `tag` (lowercase HTML tag) + `node`.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
fn analyze_node_ctor(tag: &str, node: &XmlNode) -> NodeCtor {
    // Helper for the common "no caption skip" case.
    fn sem(suffix: impl Into<String>, args: Vec<CtorArg>, consumes_text: bool) -> NodeCtor {
        NodeCtor::Semantic {
            suffix: suffix.into(),
            args,
            consumes_text,
            skip_caption: false,
        }
    }

    let aria = node_aria_label(node);
    let has_aria = aria.is_some();
    let label = aria.unwrap_or_default();
    // `has_only_text_children()` is also true for childless nodes; pair it with
    // `has_text` so empty elements stay plain containers.
    let pure_text = node.has_only_text_children();
    let text = node_direct_text(node);
    let has_text = !text.is_empty();
    let cap = cap_first(tag);

    // Tier A — *_with_text (single text child, no element children).
    if WITH_TEXT_TAGS.contains(&tag) {
        if pure_text && has_text {
            return sem(format!("{cap}WithText"), vec![CtorArg::Str(text)], true);
        }
        return NodeCtor::Plain;
    }

    match tag {
        // Tier B — aria-only / void widgets.
        "details" | "form" | "fieldset" | "legend" | "menu" | "output"
        | "datalist" | "canvas" | "audio" | "video" | "area" => {
            if has_aria {
                sem(cap, vec![CtorArg::Aria(label)], false)
            } else {
                sem(format!("{cap}NoA11y"), vec![], false)
            }
        }
        // Summary is Tier B but also has a WithText form for a single text child.
        "summary" => {
            if pure_text && has_text {
                if has_aria {
                    sem("SummaryWithText", vec![CtorArg::Str(text), CtorArg::Aria(label)], true)
                } else {
                    sem("SummaryWithTextNoA11y", vec![CtorArg::Str(text)], true)
                }
            } else if has_aria {
                sem("Summary", vec![CtorArg::Aria(label)], false)
            } else {
                sem("SummaryNoA11y", vec![], false)
            }
        }

        // Tier C — multi-arg widgets (args from HTML attributes).
        "button" => {
            if has_aria {
                sem("Button", vec![CtorArg::Str(text), CtorArg::Aria(label)], true)
            } else {
                sem("ButtonNoA11y", vec![CtorArg::Str(text)], true)
            }
        }
        "a" => {
            let href = node_attr_or(node, "href", "");
            if has_aria {
                sem("A", vec![CtorArg::Str(href), CtorArg::Str(text), CtorArg::Aria(label)], true)
            } else {
                let lbl = if has_text { CtorArg::OptSome(text) } else { CtorArg::OptNone };
                sem("ANoA11y", vec![CtorArg::Str(href), lbl], true)
            }
        }
        "label" => {
            let for_id = node_attr_or(node, "for", "");
            if has_aria {
                sem("Label", vec![CtorArg::Str(for_id), CtorArg::Str(text), CtorArg::Aria(label)], true)
            } else {
                sem("LabelNoA11y", vec![CtorArg::Str(for_id), CtorArg::Str(text)], true)
            }
        }
        "input" => {
            let ty = node_attr_or(node, "type", "text");
            let name = node_attr_or(node, "name", "");
            if has_aria {
                sem("Input", vec![CtorArg::Str(ty), CtorArg::Str(name), CtorArg::Str(label.clone()), CtorArg::Aria(label)], false)
            } else {
                sem("InputNoA11y", vec![CtorArg::Str(ty), CtorArg::Str(name), CtorArg::Str(label)], false)
            }
        }
        "textarea" => {
            let name = node_attr_or(node, "name", "");
            if has_aria {
                sem("Textarea", vec![CtorArg::Str(name), CtorArg::Str(label.clone()), CtorArg::Aria(label)], false)
            } else {
                sem("TextareaNoA11y", vec![CtorArg::Str(name), CtorArg::Str(label)], false)
            }
        }
        "select" => {
            let name = node_attr_or(node, "name", "");
            if has_aria {
                sem("Select", vec![CtorArg::Str(name), CtorArg::Str(label.clone()), CtorArg::Aria(label)], false)
            } else {
                sem("SelectNoA11y", vec![CtorArg::Str(name), CtorArg::Str(label)], false)
            }
        }
        "option" => {
            let value = node_attr_or(node, "value", "");
            if has_aria {
                sem("Option", vec![CtorArg::Str(value), CtorArg::Str(text), CtorArg::Aria(label)], true)
            } else {
                sem("OptionNoA11y", vec![CtorArg::Str(value), CtorArg::Str(text)], true)
            }
        }
        "optgroup" => {
            let lbl = node_attr_or(node, "label", "");
            if has_aria {
                sem("Optgroup", vec![CtorArg::Str(lbl), CtorArg::Aria(label)], false)
            } else {
                sem("OptgroupNoA11y", vec![CtorArg::Str(lbl)], false)
            }
        }
        "table" => {
            if has_aria {
                // The aria form injects a caption child, so take the caption from
                // the literal <caption> (or the aria label) and drop the literal.
                let caption = first_caption_text(node).unwrap_or_else(|| label.clone());
                NodeCtor::Semantic {
                    suffix: "Table".to_string(),
                    args: vec![CtorArg::Str(caption), CtorArg::Aria(label)],
                    consumes_text: false,
                    skip_caption: true,
                }
            } else {
                sem("TableNoA11y", vec![], false)
            }
        }

        // Tier D — scalar-driven widgets (NoA11y form with extracted numbers).
        "progress" => sem(
            "ProgressNoA11y",
            vec![
                CtorArg::Float(node_attr_f32(node, "value", 0.0)),
                CtorArg::Float(node_attr_f32(node, "max", 1.0)),
            ],
            false,
        ),
        "meter" => sem(
            "MeterNoA11y",
            vec![
                CtorArg::Float(node_attr_f32(node, "value", 0.0)),
                CtorArg::Float(node_attr_f32(node, "min", 0.0)),
                CtorArg::Float(node_attr_f32(node, "max", 1.0)),
            ],
            false,
        ),
        "dialog" => sem("DialogNoA11y", vec![], false),

        _ => NodeCtor::Plain,
    }
}

impl CtorArg {
    /// Rust expression for this argument (`AzString::from(..)` works for both the
    /// `Into<AzString>` and the concrete `AzString` parameter forms).
    fn render_rust(&self) -> String {
        match self {
            Self::Str(s) => format!("AzString::from(\"{}\")", esc_lit(s)),
            Self::Aria(s) => format!("SmallAriaInfo::label(AzString::from(\"{}\"))", esc_lit(s)),
            Self::Float(f) => fmt_f32_lit(*f),
            Self::OptSome(s) => format!("OptionString::Some(AzString::from(\"{}\"))", esc_lit(s)),
            Self::OptNone => "OptionString::None".to_string(),
        }
    }
    fn render_c(&self) -> String {
        match self {
            Self::Str(s) => format!("AZ_STR(\"{}\")", esc_lit(s)),
            Self::Aria(s) => format!("AzSmallAriaInfo_label(AZ_STR(\"{}\"))", esc_lit(s)),
            Self::Float(f) => format!("{}f", fmt_f32_lit(*f)),
            Self::OptSome(s) => format!("AzOptionString_some(AZ_STR(\"{}\"))", esc_lit(s)),
            Self::OptNone => "AzOptionString_none()".to_string(),
        }
    }
    fn render_cpp(&self) -> String {
        match self {
            Self::Str(s) => format!("String(\"{}\")", esc_lit(s)),
            Self::Aria(s) => format!("SmallAriaInfo::label(String(\"{}\"))", esc_lit(s)),
            Self::Float(f) => format!("{}f", fmt_f32_lit(*f)),
            Self::OptSome(s) => format!("OptionString::some(String(\"{}\"))", esc_lit(s)),
            Self::OptNone => "OptionString::none()".to_string(),
        }
    }
    fn render_python(&self) -> String {
        match self {
            Self::Str(s) => format!("\"{}\"", esc_lit(s)),
            Self::Aria(s) => format!("azul.SmallAriaInfo.label(\"{}\")", esc_lit(s)),
            Self::Float(f) => fmt_f32_lit(*f),
            Self::OptSome(s) => format!("azul.OptionString.some(\"{}\")", esc_lit(s)),
            Self::OptNone => "azul.OptionString.none()".to_string(),
        }
    }
}

impl NodeCtor {
    const fn consumes_text(&self) -> bool {
        matches!(self, Self::Semantic { consumes_text: true, .. })
    }
    const fn skip_caption(&self) -> bool {
        matches!(self, Self::Semantic { skip_caption: true, .. })
    }
    /// `Dom::create_…(args)` for Rust, or `None` for a plain container.
    fn render_rust(&self) -> Option<String> {
        match self {
            Self::Plain => None,
            Self::Semantic { suffix, args, .. } => Some(format!(
                "Dom::create_{}({})",
                camel_to_snake(suffix),
                args.iter().map(CtorArg::render_rust).collect::<Vec<_>>().join(", ")
            )),
        }
    }
    /// `AzDom_create…(args)` for C, or `None` for a plain container.
    fn render_c(&self) -> Option<String> {
        match self {
            Self::Plain => None,
            Self::Semantic { suffix, args, .. } => Some(format!(
                "AzDom_create{}({})",
                suffix,
                args.iter().map(CtorArg::render_c).collect::<Vec<_>>().join(", ")
            )),
        }
    }
    /// Fluent `Dom::create_…` (C++) / `azul.Dom.create_…` (Python), or `None`.
    fn render_fluent(&self, target: &CompileTarget) -> Option<String> {
        match self {
            Self::Plain => None,
            Self::Semantic { suffix, args, .. } => {
                let snake = camel_to_snake(suffix);
                let (prefix, rendered) = match target {
                    CompileTarget::Cpp => (
                        format!("Dom::create_{snake}"),
                        args.iter().map(CtorArg::render_cpp).collect::<Vec<_>>(),
                    ),
                    CompileTarget::Python => (
                        format!("azul.Dom.create_{snake}"),
                        args.iter().map(CtorArg::render_python).collect::<Vec<_>>(),
                    ),
                    _ => return None,
                };
                Some(format!("{}({})", prefix, rendered.join(", ")))
            }
        }
    }
}

/// Per-language token hooks for the fluent walker. The `&str` args are already
/// escaped for a double-quoted string literal.
struct FluentSyntax {
    target: CompileTarget,
    /// tag debug-name (e.g. "Div") -> full create expression
    create_node: fn(&str) -> String,
    /// escaped text -> create-text expression
    create_text: fn(&str) -> String,
    /// escaped css -> `.with_css(..)` call
    with_css: fn(&str) -> String,
    /// escaped class -> `.with_class(..)` call
    with_class: fn(&str) -> String,
    /// escaped id -> `.with_id(..)` call
    with_id: fn(&str) -> String,
    /// escaped child expression -> `.with_child(..)` call (children are chained)
    with_child: fn(&str) -> String,
}

const CPP_SYNTAX: FluentSyntax = FluentSyntax {
    target: CompileTarget::Cpp,
    // Use per-tag creators (Dom::create_div(), create_p(), create_body(), …)
    // — `NodeType` is a tagged union, so `create_node` would need union
    // construction; the per-tag creators exist for every common HTML element.
    create_node: |tag| alloc::format!("Dom::create_{}()", tag.to_lowercase()),
    create_text: |s| alloc::format!("Dom::create_text(String(\"{s}\"))"),
    with_css: |s| alloc::format!(".with_css(String(\"{s}\"))"),
    with_class: |s| alloc::format!(".with_class(String(\"{s}\"))"),
    with_id: |s| alloc::format!(".with_id(String(\"{s}\"))"),
    with_child: |c| alloc::format!(".with_child({c})"),
};

const PYTHON_SYNTAX: FluentSyntax = FluentSyntax {
    target: CompileTarget::Python,
    // Per-tag creators (azul.Dom.create_div(), …) — see CPP_SYNTAX note.
    create_node: |tag| alloc::format!("azul.Dom.create_{}()", tag.to_lowercase()),
    create_text: |s| alloc::format!("azul.Dom.create_text(\"{s}\")"),
    with_css: |s| alloc::format!(".with_css(\"{s}\")"),
    with_class: |s| alloc::format!(".with_class(\"{s}\")"),
    with_id: |s| alloc::format!(".with_id(\"{s}\")"),
    with_child: |c| alloc::format!(".with_child({c})"),
};

/// Walk one element node, emitting a fluent create-expression for `syntax`'s
/// language. Mirrors `compile_node_to_rust_code_inner` but token-parameterized.
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
// See compile_node_to_rust_code_inner: component_map is forwarded for codegen-path parity.
#[allow(clippy::only_used_in_recursion)]
fn compile_node_fluent(
    node: &XmlNode,
    syntax: &FluentSyntax,
    component_map: &ComponentMap,
    css: &Css,
    mut matcher: CssMatcher,
) -> Result<String, CompileError> {
    use azul_css::css::CssDeclaration;

    let component_name = normalize_casing(&node.node_type);
    let node_type_tag = tag_to_node_type_tag(&component_name);
    let tag_dbg = alloc::format!("{:?}", tag_to_node_type(&component_name));

    // Base create-expression. For an exported live page every node is a plain
    // HTML element, so emit a per-tag creator directly via the language hooks
    // (universal + verified) rather than the per-component `compile_fn`, whose
    // C++/Python arms emit stale placeholder syntax (`Dom.div()` etc.).
    // Interactive/data tags (whose creators need args) fall back to `div`. Any
    // element text shows up as a Text child below and is handled there.
    let ctor = analyze_node_ctor(&component_name, node);
    let mut s = ctor.render_fluent(&syntax.target).map_or_else(|| (syntax.create_node)(safe_container_tag(&tag_dbg)), |expr| expr);

    matcher.path.push(CssPathSelector::Type(node_type_tag));
    let ids: Vec<String> = node.attributes.get_key("id")
        .map(|v| v.split_whitespace().map(alloc::string::ToString::to_string).collect())
        .unwrap_or_default();
    matcher.path.extend(ids.iter().map(|id| CssPathSelector::Id(id.clone().into())));
    let classes: Vec<String> = node.attributes.get_key("class")
        .map(|v| v.split_whitespace().map(alloc::string::ToString::to_string).collect())
        .unwrap_or_default();
    matcher.path.extend(classes.iter().map(|c| CssPathSelector::Class(c.clone().into())));

    // Inline CSS (matched rules -> `.with_css("..")`, pseudo blocks included).
    let blocks = get_css_blocks(css, &matcher);
    if !blocks.is_empty() {
        let inline_css = css_blocks_to_inline_string(&blocks);
        if !inline_css.is_empty() {
            let esc = inline_css.replace('\\', "\\\\").replace('"', "\\\"");
            s.push_str(&(syntax.with_css)(&esc));
        }
    }
    for id in &ids {
        s.push_str(&(syntax.with_id)(&id.replace('\\', "\\\\").replace('"', "\\\"")));
    }
    for class in &classes {
        s.push_str(&(syntax.with_class)(&class.replace('\\', "\\\\").replace('"', "\\\"")));
    }

    // Children (chained `.with_child(..)`). Text folded into the ctor (Tier A/C)
    // is skipped here, as is a `<caption>` already injected by `create_table`.
    let mut caption_skipped = false;
    for (child_idx, child) in node.children.as_ref().iter().enumerate() {
        match child {
            XmlNodeChild::Element(child_node) => {
                if ctor.skip_caption()
                    && !caption_skipped
                    && child_node.node_type.as_str().eq_ignore_ascii_case("caption")
                {
                    caption_skipped = true;
                    continue;
                }
                let mut m = matcher.clone();
                m.path.push(CssPathSelector::Children);
                m.indices_in_parent.push(child_idx);
                m.children_length.push(node.children.len());
                let child_src = compile_node_fluent(child_node, syntax, component_map, css, m)?;
                s.push_str(&(syntax.with_child)(&child_src));
            }
            XmlNodeChild::Text(text) => {
                if ctor.consumes_text() {
                    continue;
                }
                let text = text.trim();
                if !text.is_empty() {
                    let esc = text.replace('\\', "\\\\").replace('"', "\\\"");
                    s.push_str(&(syntax.with_child)(&(syntax.create_text)(&esc)));
                }
            }
        }
    }

    Ok(s)
}

/// Build the `<body>` render-expression for `syntax`'s language.
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
fn compile_body_fluent<'a>(
    body_node: &'a XmlNode,
    syntax: &FluentSyntax,
    component_map: &'a ComponentMap,
    css: &Css,
    mut matcher: CssMatcher,
) -> Result<String, CompileError> {
    let mut s = (syntax.create_node)("Body");
    matcher.path.push(CssPathSelector::Type(NodeTypeTag::Body));
    let classes: Vec<String> = body_node.attributes.get_key("class")
        .map(|v| v.split_whitespace().map(alloc::string::ToString::to_string).collect())
        .unwrap_or_default();
    matcher.path.extend(classes.iter().map(|c| CssPathSelector::Class(c.clone().into())));

    let blocks = get_css_blocks(css, &matcher);
    if !blocks.is_empty() {
        let inline_css = css_blocks_to_inline_string(&blocks);
        if !inline_css.is_empty() {
            let esc = inline_css.replace('\\', "\\\\").replace('"', "\\\"");
            s.push_str(&(syntax.with_css)(&esc));
        }
    }
    for class in &classes {
        s.push_str(&(syntax.with_class)(&class.replace('\\', "\\\\").replace('"', "\\\"")));
    }

    for (child_idx, child) in body_node.children.as_ref().iter().enumerate() {
        match child {
            XmlNodeChild::Element(child_node) => {
                let mut m = matcher.clone();
                m.path.push(CssPathSelector::Children);
                m.indices_in_parent.push(child_idx);
                m.children_length.push(body_node.children.len());
                let child_src = compile_node_fluent(child_node, syntax, component_map, css, m)?;
                s.push_str(&(syntax.with_child)(&child_src));
            }
            XmlNodeChild::Text(text) => {
                let text = text.trim();
                if !text.is_empty() {
                    let esc = text.replace('\\', "\\\\").replace('"', "\\\"");
                    s.push_str(&(syntax.with_child)(&(syntax.create_text)(&esc)));
                }
            }
        }
    }
    Ok(s)
}

/// Parse the page's `<style>` and seed a matcher rooted at `<body>`. Shared by
/// the C++/Python/C entry points (mirrors the head of `str_to_rust_code`).
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
fn parse_page_style_and_body(
    root_nodes: &[XmlNodeChild],
) -> Result<(Css, &XmlNode), CompileError> {
    let html_node = get_html_node(root_nodes)?;
    let body_node = get_body_node(html_node.children.as_ref())?;
    let mut global_style = Css::empty();
    if let Some(head_node) = find_node_by_type(html_node.children.as_ref(), "head") {
        if let Some(style_node) = find_node_by_type(head_node.children.as_ref(), "style") {
            let text = style_node.get_text_content();
            if !text.is_empty() {
                global_style = azul_css::parser2::new_from_str(&text).0;
            }
        }
    }
    global_style.sort_by_specificity();
    Ok((global_style, body_node))
}

fn body_matcher(body_node: &XmlNode) -> CssMatcher {
    CssMatcher {
        path: Vec::new(),
        indices_in_parent: vec![0],
        children_length: vec![body_node.children.as_ref().len()],
    }
}

/// Compile a full HTML page to a compilable **C++** Azul app.
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
/// # Errors
///
/// Returns an error if the XML cannot be parsed or compiled to C++ code.
pub fn str_to_cpp_code<'a>(
    root_nodes: &'a [XmlNodeChild],
    component_map: &'a ComponentMap,
) -> Result<String, CompileError> {
    let (global_style, body_node) = parse_page_style_and_body(root_nodes)?;
    let render = compile_body_fluent(body_node, &CPP_SYNTAX, component_map, &global_style, body_matcher(body_node))?;
    Ok(alloc::format!(
        "// Auto-generated UI source code (C++). Build:\n\
         //   clang++ -std=c++20 -I <azul>/target/codegen main.cpp -lazul\n\
         #include \"azul20.hpp\"\n\
         using namespace azul;\n\n\
         struct Data {{}};\n\n\
         AzDom render(AzRefAny data, AzLayoutCallbackInfo info) {{\n    \
         return {render};\n}}\n\n\
         int main() {{\n    \
         RefAny data = RefAny::create(Data{{}});\n    \
         WindowCreateOptions window = WindowCreateOptions::create(render);\n    \
         App app = App::create(std::move(data), AppConfig::default_());\n    \
         app.run(std::move(window));\n    \
         return 0;\n}}\n"
    ))
}

/// Compile a full HTML page to a compilable **Python** Azul app.
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
/// # Errors
///
/// Returns an error if the XML cannot be parsed or compiled to Python code.
pub fn str_to_python_code<'a>(
    root_nodes: &'a [XmlNodeChild],
    component_map: &'a ComponentMap,
) -> Result<String, CompileError> {
    let (global_style, body_node) = parse_page_style_and_body(root_nodes)?;
    let render = compile_body_fluent(body_node, &PYTHON_SYNTAX, component_map, &global_style, body_matcher(body_node))?;
    Ok(alloc::format!(
        "# Auto-generated UI source code (Python). Run: python3 main.py\n\
         import azul\n\n\
         class Data:\n    pass\n\n\
         def render(data, info):\n    return (\n        {}\n    )\n\n\
         def main():\n    \
         app = azul.App.create(Data(), azul.AppConfig.create())\n    \
         window = azul.WindowCreateOptions.create(render)\n    \
         app.run(window)\n\n\
         if __name__ == \"__main__\":\n    main()\n",
        render.replace("\r\n", "\n        ")
    ))
}

// ───────────────────────────────────────────────────────────────────────────
// Imperative C emitter. C has no fluent builder: each node is a statement that
// creates an `AzDom` local, applies css/class (by-value, returns), and pushes
// children via `AzDom_addChild(&parent, child)`. A recursive walk emits the
// statements bottom-up and returns the variable name holding each node.
// ───────────────────────────────────────────────────────────────────────────

/// C per-tag creator suffix: `NodeTypeTag` debug name with first char kept and
/// the rest lowercased (`Div`->`Div`, `BlockQuote`->`Blockquote`, `H1`->`H1`),
/// matching `AzDom_create<Suffix>` in azul.h.
fn c_creator_suffix(tag_dbg: &str) -> String {
    let mut chars = tag_dbg.chars();
    chars.next().map_or_else(|| "Div".to_string(), |first| {
            let rest: String = chars.as_str().to_lowercase();
            alloc::format!("{first}{rest}")
        })
}

#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
fn compile_node_c(
    node: &XmlNode,
    component_map: &ComponentMap,
    css: &Css,
    mut matcher: CssMatcher,
    counter: &mut usize,
    out: &mut String,
) -> Result<String, CompileError> {
    let _ = component_map;
    let component_name = normalize_casing(&node.node_type);
    let node_type_tag = tag_to_node_type_tag(&component_name);
    let tag_dbg = alloc::format!("{:?}", tag_to_node_type(&component_name));

    let var = alloc::format!("n{}", *counter);
    *counter += 1;
    let ctor = analyze_node_ctor(&component_name, node);
    match ctor.render_c() {
        Some(expr) => { let _ = writeln!(out, "    AzDom {var} = {expr};"); },
        None => { let _ = writeln!(out,
            "    AzDom {} = AzDom_create{}();",
            var,
            c_creator_suffix(safe_container_tag(&tag_dbg))
        ); },
    }

    matcher.path.push(CssPathSelector::Type(node_type_tag));
    let ids: Vec<String> = node.attributes.get_key("id")
        .map(|v| v.split_whitespace().map(alloc::string::ToString::to_string).collect())
        .unwrap_or_default();
    matcher.path.extend(ids.iter().map(|id| CssPathSelector::Id(id.clone().into())));
    let classes: Vec<String> = node.attributes.get_key("class")
        .map(|v| v.split_whitespace().map(alloc::string::ToString::to_string).collect())
        .unwrap_or_default();
    matcher.path.extend(classes.iter().map(|c| CssPathSelector::Class(c.clone().into())));

    let blocks = get_css_blocks(css, &matcher);
    if !blocks.is_empty() {
        let inline_css = css_blocks_to_inline_string(&blocks);
        if !inline_css.is_empty() {
            let esc = inline_css.replace('\\', "\\\\").replace('"', "\\\"");
            let _ = writeln!(out, "    {var} = AzDom_withCss({var}, AZ_STR(\"{esc}\"));");
        }
    }
    for id in &ids {
        let esc = id.replace('\\', "\\\\").replace('"', "\\\"");
        let _ = writeln!(out, "    {var} = AzDom_withId({var}, AZ_STR(\"{esc}\"));");
    }
    for class in &classes {
        let esc = class.replace('\\', "\\\\").replace('"', "\\\"");
        let _ = writeln!(out, "    {var} = AzDom_withClass({var}, AZ_STR(\"{esc}\"));");
    }

    let mut caption_skipped = false;
    for (child_idx, child) in node.children.as_ref().iter().enumerate() {
        match child {
            XmlNodeChild::Element(child_node) => {
                if ctor.skip_caption()
                    && !caption_skipped
                    && child_node.node_type.as_str().eq_ignore_ascii_case("caption")
                {
                    caption_skipped = true;
                    continue;
                }
                let mut m = matcher.clone();
                m.path.push(CssPathSelector::Children);
                m.indices_in_parent.push(child_idx);
                m.children_length.push(node.children.len());
                let child_var = compile_node_c(child_node, component_map, css, m, counter, out)?;
                let _ = writeln!(out, "    AzDom_addChild(&{var}, {child_var});");
            }
            XmlNodeChild::Text(text) => {
                if ctor.consumes_text() {
                    continue;
                }
                let text = text.trim();
                if !text.is_empty() {
                    let esc = text.replace('\\', "\\\\").replace('"', "\\\"");
                    let _ = writeln!(out,
                        "    AzDom_addChild(&{var}, AzDom_createText(AZ_STR(\"{esc}\")));"
                    );
                }
            }
        }
    }
    Ok(var)
}

/// Compile a full HTML page to a compilable **C** Azul app.
#[allow(clippy::result_large_err)] // returns a #[repr(C,u8)] FFI error enum; boxing a variant would break the C ABI/api.json
/// # Errors
///
/// Returns an error if the XML cannot be parsed or compiled to C code.
pub fn str_to_c_code<'a>(
    root_nodes: &'a [XmlNodeChild],
    component_map: &'a ComponentMap,
) -> Result<String, CompileError> {
    let (global_style, body_node) = parse_page_style_and_body(root_nodes)?;
    let mut body = String::new();
    let mut counter = 0usize;

    // Emit the body as the root node, then its children.
    let root = alloc::format!("n{counter}");
    counter += 1;
    let _ = writeln!(body, "    AzDom {root} = AzDom_createBody();");

    let mut matcher = body_matcher(body_node);
    matcher.path.push(CssPathSelector::Type(NodeTypeTag::Body));
    let classes: Vec<String> = body_node.attributes.get_key("class")
        .map(|v| v.split_whitespace().map(alloc::string::ToString::to_string).collect())
        .unwrap_or_default();
    matcher.path.extend(classes.iter().map(|c| CssPathSelector::Class(c.clone().into())));
    let blocks = get_css_blocks(&global_style, &matcher);
    if !blocks.is_empty() {
        let inline_css = css_blocks_to_inline_string(&blocks);
        if !inline_css.is_empty() {
            let esc = inline_css.replace('\\', "\\\\").replace('"', "\\\"");
            let _ = writeln!(body, "    {root} = AzDom_withCss({root}, AZ_STR(\"{esc}\"));");
        }
    }
    for (child_idx, child) in body_node.children.as_ref().iter().enumerate() {
        match child {
            XmlNodeChild::Element(child_node) => {
                let mut m = matcher.clone();
                m.path.push(CssPathSelector::Children);
                m.indices_in_parent.push(child_idx);
                m.children_length.push(body_node.children.len());
                let child_var = compile_node_c(child_node, component_map, &global_style, m, &mut counter, &mut body)?;
                let _ = writeln!(body, "    AzDom_addChild(&{root}, {child_var});");
            }
            XmlNodeChild::Text(text) => {
                let text = text.trim();
                if !text.is_empty() {
                    let esc = text.replace('\\', "\\\\").replace('"', "\\\"");
                    let _ = writeln!(body,
                        "    AzDom_addChild(&{root}, AzDom_createText(AZ_STR(\"{esc}\")));"
                    );
                }
            }
        }
    }

    Ok(alloc::format!(
        "/* Auto-generated UI source code (C). Build:\n\
         *   clang -I <azul>/target/codegen main.c -lazul\n */\n\
         #include \"azul.h\"\n\
         #include <string.h>\n\
         #define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))\n\n\
         AzDom render(AzRefAny data, AzLayoutCallbackInfo info) {{\n\
         {body}    return {root};\n}}\n\n\
         int main(void) {{\n    \
         AzString data_type = AZ_STR(\"Data\");\n    \
         AzRefAny data = AzRefAny_newC((AzGlVoidPtrConst){{ .ptr = NULL }}, 0, 1, 0, data_type, NULL, 0, 0);\n    \
         AzApp app = AzApp_create(data, AzAppConfig_create());\n    \
         AzWindowCreateOptions window = AzWindowCreateOptions_create(render);\n    \
         AzApp_run(&app, window);\n    \
         AzApp_delete(&app);\n    \
         return 0;\n}}\n"
    ))
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

    #[test]
    fn test_img_tag_becomes_image_node_with_src_tag() {
        // `<img src="cat.jpg" width="300" height="169">` must become a
        // `NodeType::Image` whose `NullImage` carries the `src` string as its
        // `tag` (so a renderer can resolve the bytes later), plus the declared
        // intrinsic size.
        use crate::resources::DecodedImage;
        use crate::window::{AzStringPair, StringPairVec};

        let img_node = XmlNode {
            node_type: "img".into(),
            attributes: XmlAttributeMap::from(StringPairVec::from_vec(alloc::vec![
                AzStringPair {
                    key: "src".into(),
                    value: "cat.jpg".into()
                },
                AzStringPair {
                    key: "width".into(),
                    value: "300".into()
                },
                AzStringPair {
                    key: "height".into(),
                    value: "169".into()
                },
            ])),
            children: Vec::new().into(),
        };

        let component_map = ComponentMap::default();
        let dom = xml_node_to_dom_fast(&img_node, &component_map, false, 0)
            .expect("xml_node_to_dom_fast for <img> should succeed");

        match dom.root.get_node_type() {
            NodeType::Image(image_ref) => match image_ref.as_ref().get_data() {
                DecodedImage::NullImage {
                    tag, width, height, ..
                } => {
                    assert_eq!(
                        core::str::from_utf8(tag).unwrap(),
                        "cat.jpg",
                        "image tag must carry the src string"
                    );
                    assert_eq!(*width, 300, "width attribute should set intrinsic width");
                    assert_eq!(*height, 169, "height attribute should set intrinsic height");
                }
                other => panic!("expected NullImage carrying the src tag, got {:?}", other),
            },
            other => panic!("expected NodeType::Image for <img>, got {:?}", other),
        }

        println!("Test passed: <img src=\"cat.jpg\"> -> NodeType::Image tagged \"cat.jpg\"");
    }

    #[test]
    fn test_tag_to_node_type_img_is_image() {
        // The bare tag mapping should also yield an Image (placeholder, empty tag).
        match tag_to_node_type("img") {
            NodeType::Image(_) => {}
            other => panic!("tag_to_node_type(\"img\") should be Image, got {:?}", other),
        }
    }

    /// Build a `<div>` nested `depth` levels deep, innermost first.
    fn nested_divs(depth: usize) -> XmlNode {
        let mut node = XmlNode {
            node_type: "div".into(),
            ..Default::default()
        };
        for _ in 0..depth {
            node = XmlNode {
                node_type: "div".into(),
                children: vec![XmlNodeChild::Element(node)].into(),
                ..Default::default()
            };
        }
        node
    }

    /// AUDIT 2026-07-08: `extract_css_urls` used to slice the original string with
    /// a byte offset computed in a `to_lowercase()` temporary. On `'İ'` (whose
    /// lowercase is longer in bytes) that offset was misaligned. This must no
    /// longer panic and must still find the `@import` target.
    #[test]
    fn extract_css_urls_unicode_import_no_panic() {
        let mut res = Vec::new();
        Xml::extract_css_urls("İ@import 'x'", &mut res);
        assert_eq!(res.len(), 1, "should find the one @import target");
        assert_eq!(res[0].url.as_str(), "x");
    }

    /// The `url(` and `@import` scans are case-insensitive after the audit fix.
    #[test]
    fn extract_css_urls_is_case_insensitive() {
        let mut res = Vec::new();
        Xml::extract_css_urls("body { background: URL(http://e.com/a.png); }", &mut res);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].url.as_str(), "http://e.com/a.png");

        let mut res2 = Vec::new();
        Xml::extract_css_urls("@IMPORT \"theme.css\";", &mut res2);
        assert_eq!(res2.len(), 1);
        assert_eq!(res2[0].url.as_str(), "theme.css");
    }

    /// AUDIT 2026-07-08: the resource scan recurses per nesting level; deep markup
    /// must not overflow the stack (deeper-than-cap subtrees are just not scanned).
    #[test]
    fn scan_external_resources_deep_nesting_ok() {
        let xml = Xml {
            root: vec![XmlNodeChild::Element(nested_divs(2000))].into(),
        };
        // Must simply return (no stack overflow); no resources in a plain tree.
        drop(xml.scan_external_resources());
    }

    /// AUDIT 2026-07-08: the fast + tree DOM builders recurse per nesting level;
    /// deep markup must not overflow the stack (children beyond the cap are
    /// dropped, but the call returns `Ok`).
    #[test]
    fn xml_node_to_dom_fast_deep_nesting_ok() {
        let deep = nested_divs(2000);
        let component_map = ComponentMap::default();
        let dom = xml_node_to_dom_fast(&deep, &component_map, false, 0);
        assert!(dom.is_ok(), "deep DOM build must not overflow the stack");

        let mut builder = CompactDomBuilder::new();
        let fast = xml_node_to_fast_dom(&deep, &component_map, false, &mut builder, 0);
        assert!(fast.is_ok(), "deep FastDom build must not overflow the stack");
    }

    /// AUDIT 2026-07-08: `ComponentFieldType::parse` recurses through `Option<..>`
    /// / `Vec<..>` wrappers; an over-deep type string is rejected rather than
    /// overflowing the stack, while ordinary nesting still parses.
    #[test]
    fn component_field_type_parse_depth_capped() {
        let deep = format!("{}Bool{}", "Option<".repeat(4000), ">".repeat(4000));
        assert!(
            ComponentFieldType::parse(&deep).is_none(),
            "over-deep type string must be rejected, not overflow"
        );

        let shallow = format!("{}Bool{}", "Option<".repeat(8), ">".repeat(8));
        assert!(
            ComponentFieldType::parse(&shallow).is_some(),
            "ordinary nesting must still parse"
        );
    }

    /// AUDIT 2026-07-08: `prepare_string` now decodes the full common entity set
    /// plus numeric references, in a single pass so `&amp;` cannot double-decode.
    #[test]
    fn prepare_string_entity_decoding() {
        assert_eq!(prepare_string("a &amp; b"), "a & b");
        // `&amp;lt;` must yield the literal text "&lt;", not "<".
        assert_eq!(prepare_string("&amp;lt;"), "&lt;");
        assert_eq!(prepare_string("&quot;hi&quot;"), "\"hi\"");
        assert_eq!(prepare_string("&#65;&#66;"), "AB");
        assert_eq!(prepare_string("&#x41;"), "A");
        // Existing behavior preserved.
        assert_eq!(prepare_string("&lt;tag&gt;"), "<tag>");
    }
}

#[cfg(test)]
#[allow(clippy::all, clippy::pedantic, clippy::nursery)]
mod autotest_generated {
    use super::*;
    use crate::dom::{NodeData, NodeType};
    use azul_css::css::{CssNthChildPattern, CssNthChildSelector};

    // ----------------------------------------------------------------- helpers

    fn attrs(kv: &[(&str, &str)]) -> XmlAttributeMap {
        XmlAttributeMap::from(StringPairVec::from_vec(
            kv.iter()
                .map(|(k, v)| AzStringPair {
                    key: AzString::from(*k),
                    value: AzString::from(*v),
                })
                .collect::<Vec<_>>(),
        ))
    }

    fn node(tag: &str, kv: &[(&str, &str)], children: Vec<XmlNodeChild>) -> XmlNode {
        XmlNode {
            node_type: tag.into(),
            attributes: attrs(kv),
            children: children.into(),
        }
    }

    fn txt(s: &str) -> XmlNodeChild {
        XmlNodeChild::Text(AzString::from(s))
    }

    fn elem(n: XmlNode) -> XmlNodeChild {
        XmlNodeChild::Element(n)
    }

    /// `<html><head><style>{css}</style></head><body>{children}</body></html>`
    fn doc(css: &str, body_children: Vec<XmlNodeChild>) -> Vec<XmlNodeChild> {
        let style = node("style", &[], vec![txt(css)]);
        let head = node("head", &[], vec![elem(style)]);
        let body = node("body", &[], body_children);
        vec![elem(node("html", &[], vec![elem(head), elem(body)]))]
    }

    fn no_args() -> ComponentArgumentVec {
        ComponentArgumentVec::from_const_slice(&[])
    }

    fn dm(name: &str, fields: Vec<ComponentDataField>) -> ComponentDataModel {
        ComponentDataModel {
            name: AzString::from(name),
            description: AzString::from_const_str(""),
            fields: fields.into(),
        }
    }

    fn user_def(css: &str, fields: Vec<ComponentDataField>) -> ComponentDef {
        ComponentDef {
            id: ComponentId::new("mylib", "widget"),
            display_name: AzString::from_const_str("Widget"),
            description: AzString::from_const_str(""),
            css: AzString::from(css),
            source: ComponentSource::UserDefined,
            data_model: dm("WidgetData", fields),
            render_fn: user_defined_render_fn,
            compile_fn: user_defined_compile_fn,
            render_fn_source: None.into(),
            compile_fn_source: None.into(),
        }
    }

    /// A string that is long enough to smoke out O(n^2) / allocation blowups but
    /// still finishes fast in a debug-profile test run.
    const LONG: usize = 200_000;

    // ================================================================
    // Xml::extract_url_value  (parser)
    // ================================================================

    #[test]
    fn extract_url_value_empty_and_whitespace() {
        assert_eq!(Xml::extract_url_value(""), None);
        assert_eq!(Xml::extract_url_value("   "), None);
        assert_eq!(Xml::extract_url_value("\t\n\r "), None);
    }

    #[test]
    fn extract_url_value_valid_minimal() {
        assert_eq!(
            Xml::extract_url_value("a.png)"),
            Some("a.png".to_string()),
            "unquoted url terminated by ')'"
        );
        assert_eq!(
            Xml::extract_url_value("\"a.png\")"),
            Some("a.png".to_string()),
            "double-quoted url"
        );
        assert_eq!(
            Xml::extract_url_value("'a.png')"),
            Some("a.png".to_string()),
            "single-quoted url"
        );
    }

    #[test]
    fn extract_url_value_leading_trailing_junk_is_trimmed() {
        // Leading whitespace is trimmed by `trim_start`, inner padding by `trim`.
        assert_eq!(
            Xml::extract_url_value("   a.png   )tail"),
            Some("a.png".to_string())
        );
    }

    #[test]
    fn extract_url_value_garbage_returns_none() {
        // Unterminated quote / no closing paren => None, never a panic.
        assert_eq!(Xml::extract_url_value("\"unterminated"), None);
        assert_eq!(Xml::extract_url_value("'unterminated"), None);
        assert_eq!(Xml::extract_url_value("no-closing-paren"), None);
        assert_eq!(Xml::extract_url_value("\u{0}\u{1}\u{7f}"), None);
    }

    #[test]
    fn extract_url_value_boundary_numbers() {
        for s in [
            "0)",
            "-0)",
            "9223372036854775807)",
            "-9223372036854775808)",
            "NaN)",
            "inf)",
            "1e400)",
        ] {
            let got = Xml::extract_url_value(s);
            assert!(got.is_some(), "numeric-looking url {s:?} is still a url");
        }
        assert_eq!(Xml::extract_url_value("0)"), Some("0".to_string()));
    }

    #[test]
    fn extract_url_value_unicode_no_panic() {
        // The ')' scan must land on a char boundary of the ORIGINAL string.
        assert_eq!(
            Xml::extract_url_value("\u{1F600}\u{0301})"),
            Some("\u{1F600}\u{0301}".to_string())
        );
        assert_eq!(Xml::extract_url_value("\"\u{130}\")"), Some("\u{130}".to_string()));
        assert_eq!(Xml::extract_url_value("\u{1F600}"), None);
    }

    #[test]
    fn extract_url_value_extremely_long_terminates() {
        let s = "a".repeat(LONG);
        assert_eq!(Xml::extract_url_value(&s), None, "no ')' anywhere => None");
        let s2 = format!("{})", "b".repeat(LONG));
        assert_eq!(Xml::extract_url_value(&s2).map(|v| v.len()), Some(LONG));
    }

    #[test]
    fn extract_url_value_nested_brackets_no_stack_overflow() {
        // Not recursive, but confirm deeply "nested" input is handled iteratively.
        let s = "(".repeat(10_000);
        assert_eq!(Xml::extract_url_value(&s), None);
        let s2 = format!("{}{}", "(".repeat(10_000), ")");
        assert_eq!(Xml::extract_url_value(&s2), Some("(".repeat(10_000)));
    }

    // ================================================================
    // Xml::extract_quoted_string  (parser)
    // ================================================================

    #[test]
    fn extract_quoted_string_empty_whitespace_garbage() {
        assert_eq!(Xml::extract_quoted_string(""), None);
        assert_eq!(Xml::extract_quoted_string("   "), None);
        assert_eq!(Xml::extract_quoted_string("\t\n"), None);
        assert_eq!(Xml::extract_quoted_string("bare"), None);
        // Leading whitespace is NOT trimmed here (unlike extract_url_value).
        assert_eq!(Xml::extract_quoted_string("  \"x\""), None);
    }

    #[test]
    fn extract_quoted_string_valid_minimal_and_empty_quotes() {
        assert_eq!(Xml::extract_quoted_string("\"x\""), Some("x".to_string()));
        assert_eq!(Xml::extract_quoted_string("'x'"), Some("x".to_string()));
        // An empty quoted string is Some(""), not None.
        assert_eq!(Xml::extract_quoted_string("\"\""), Some(String::new()));
        assert_eq!(Xml::extract_quoted_string("''"), Some(String::new()));
    }

    #[test]
    fn extract_quoted_string_unterminated_is_none() {
        assert_eq!(Xml::extract_quoted_string("\"abc"), None);
        assert_eq!(Xml::extract_quoted_string("'abc"), None);
        // Mismatched quotes do not pair up.
        assert_eq!(Xml::extract_quoted_string("\"abc'"), None);
    }

    #[test]
    fn extract_quoted_string_boundary_numbers_and_unicode() {
        assert_eq!(Xml::extract_quoted_string("\"0\""), Some("0".to_string()));
        assert_eq!(Xml::extract_quoted_string("\"-0\""), Some("-0".to_string()));
        assert_eq!(Xml::extract_quoted_string("\"NaN\""), Some("NaN".to_string()));
        assert_eq!(
            Xml::extract_quoted_string("\"\u{1F600}\u{0301}\""),
            Some("\u{1F600}\u{0301}".to_string())
        );
    }

    #[test]
    fn extract_quoted_string_extremely_long_terminates() {
        let unterminated = format!("\"{}", "x".repeat(LONG));
        assert_eq!(Xml::extract_quoted_string(&unterminated), None);
        let terminated = format!("\"{}\"", "x".repeat(LONG));
        assert_eq!(
            Xml::extract_quoted_string(&terminated).map(|s| s.len()),
            Some(LONG)
        );
    }

    // ================================================================
    // Xml::parse_srcset  (parser)
    // ================================================================

    #[test]
    fn parse_srcset_empty_and_whitespace_yield_no_urls() {
        assert!(Xml::parse_srcset("").is_empty());
        assert!(Xml::parse_srcset("   ").is_empty());
        assert!(Xml::parse_srcset("\t\n").is_empty());
        assert!(Xml::parse_srcset(",,,").is_empty(), "all-empty entries dropped");
    }

    #[test]
    fn parse_srcset_valid_minimal() {
        assert_eq!(
            Xml::parse_srcset("a.png 1x, b.png 2x"),
            vec!["a.png".to_string(), "b.png".to_string()]
        );
        // No descriptor at all is still a valid single entry.
        assert_eq!(Xml::parse_srcset("a.png"), vec!["a.png".to_string()]);
    }

    #[test]
    fn parse_srcset_garbage_and_boundary_numbers() {
        assert_eq!(Xml::parse_srcset("0, -0, NaN"), vec!["0", "-0", "NaN"]);
        // Garbage bytes still round out to "first whitespace-delimited token".
        assert_eq!(Xml::parse_srcset("\u{0}\u{7f} 1x"), vec!["\u{0}\u{7f}".to_string()]);
    }

    #[test]
    fn parse_srcset_unicode_no_panic() {
        assert_eq!(
            Xml::parse_srcset("\u{1F600}.png 1x, \u{130}.png 2x"),
            vec!["\u{1F600}.png".to_string(), "\u{130}.png".to_string()]
        );
    }

    #[test]
    fn parse_srcset_extremely_long_terminates() {
        let s = "a.png 1x,".repeat(20_000);
        assert_eq!(Xml::parse_srcset(&s).len(), 20_000);
        let one_huge = "a".repeat(LONG);
        assert_eq!(Xml::parse_srcset(&one_huge).len(), 1);
    }

    // ================================================================
    // Xml::looks_like_resource / guess_kind_from_url / guess_mime_from_url
    // ================================================================

    #[test]
    fn looks_like_resource_edges() {
        assert!(!Xml::looks_like_resource(""));
        assert!(!Xml::looks_like_resource("   "));
        assert!(!Xml::looks_like_resource("/about"));
        assert!(Xml::looks_like_resource("/a.PNG"), "case-insensitive");
        assert!(Xml::looks_like_resource("x.pdf"));
        // A query string defeats the extension check (documented consequence of
        // matching on `ends_with`).
        assert!(!Xml::looks_like_resource("x.png?v=1"));
        assert!(!Xml::looks_like_resource(&"a".repeat(LONG)));
    }

    #[test]
    fn guess_kind_from_url_covers_every_bucket() {
        use ExternalResourceKind::*;
        assert_eq!(Xml::guess_kind_from_url(""), Unknown);
        assert_eq!(Xml::guess_kind_from_url("a.PNG"), Image);
        assert_eq!(Xml::guess_kind_from_url("a.woff2"), Font);
        assert_eq!(Xml::guess_kind_from_url("a.css"), Stylesheet);
        assert_eq!(Xml::guess_kind_from_url("a.mjs"), Script);
        assert_eq!(Xml::guess_kind_from_url("a.webm"), Video);
        assert_eq!(Xml::guess_kind_from_url("a.flac"), Audio);
        assert_eq!(Xml::guess_kind_from_url("a.ico"), Icon);
        // Query strings ARE stripped here (unlike looks_like_resource).
        assert_eq!(Xml::guess_kind_from_url("a.png?v=1"), Image);
        assert_eq!(Xml::guess_kind_from_url("\u{1F600}"), Unknown);
    }

    #[test]
    fn guess_mime_from_url_empty_and_garbage() {
        assert_eq!(Xml::guess_mime_from_url("", ""), None);
        assert_eq!(Xml::guess_mime_from_url("   ", ""), None);
        assert_eq!(Xml::guess_mime_from_url("\u{0}\u{7f}", ""), None);
        assert_eq!(Xml::guess_mime_from_url("\u{1F600}", ""), None);
    }

    #[test]
    fn guess_mime_from_url_valid_minimal_and_category_fallback() {
        let m = Xml::guess_mime_from_url("a.PNG", "").expect("png is a known extension");
        assert_eq!(m.inner.as_str(), "image/png");
        let m = Xml::guess_mime_from_url("a.png?v=1", "").expect("query string stripped");
        assert_eq!(m.inner.as_str(), "image/png");
        // Unknown extension + a category hint => the category wildcard.
        let m = Xml::guess_mime_from_url("/no-ext", "image").expect("category fallback");
        assert_eq!(m.inner.as_str(), "image/*");
        // Unknown category => None.
        assert_eq!(Xml::guess_mime_from_url("/no-ext", "bogus"), None);
    }

    #[test]
    fn guess_mime_from_url_boundary_numbers_and_long() {
        assert_eq!(Xml::guess_mime_from_url("0", ""), None);
        assert_eq!(Xml::guess_mime_from_url("-0", ""), None);
        assert_eq!(Xml::guess_mime_from_url("NaN", ""), None);
        assert_eq!(Xml::guess_mime_from_url("inf", ""), None);
        let long = format!("{}.png", "a".repeat(LONG));
        assert_eq!(
            Xml::guess_mime_from_url(&long, "").map(|m| m.inner.as_str().to_string()),
            Some("image/png".to_string())
        );
    }

    // ================================================================
    // Xml::extract_css_urls / scan_node / scan_external_resources
    // ================================================================

    #[test]
    fn extract_css_urls_empty_and_garbage_no_panic() {
        let mut v = Vec::new();
        Xml::extract_css_urls("", &mut v);
        Xml::extract_css_urls("   ", &mut v);
        Xml::extract_css_urls("\u{0}\u{7f}\u{1F600}", &mut v);
        Xml::extract_css_urls("url(", &mut v);
        Xml::extract_css_urls("@import", &mut v);
        Xml::extract_css_urls("@import url(", &mut v);
        assert!(v.is_empty(), "no well-formed url in any of those inputs");
    }

    #[test]
    fn extract_css_urls_valid_minimal() {
        let mut v = Vec::new();
        Xml::extract_css_urls("a { background: url('x.png'); }", &mut v);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].url.as_str(), "x.png");
        assert_eq!(v[0].kind, ExternalResourceKind::Image);
        assert_eq!(v[0].source_attribute.as_str(), "url()");
    }

    #[test]
    fn extract_css_urls_import_is_tagged_as_stylesheet() {
        let mut v = Vec::new();
        Xml::extract_css_urls("@import url(theme.css);", &mut v);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].url.as_str(), "theme.css");
        assert_eq!(v[0].kind, ExternalResourceKind::Stylesheet);
        assert_eq!(v[0].source_attribute.as_str(), "@import");
    }

    #[test]
    fn extract_css_urls_multibyte_before_url_no_panic() {
        // ASCII-only lowercasing keeps byte offsets 1:1 with the original.
        let mut v = Vec::new();
        Xml::extract_css_urls("\u{130}\u{1F600} URL(\"a.css\") \u{0301}", &mut v);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].url.as_str(), "a.css");
    }

    #[test]
    fn extract_css_urls_extremely_long_terminates() {
        // Each iteration advances search_from past the "url(" it just matched, so
        // this must terminate (and not spin).
        let mut v = Vec::new();
        Xml::extract_css_urls(&"url(".repeat(2_000), &mut v);
        // No ')' anywhere => nothing extractable, but the scan still terminates.
        assert!(v.is_empty());

        let mut v2 = Vec::new();
        Xml::extract_css_urls(&"url(a.png)".repeat(2_000), &mut v2);
        assert_eq!(v2.len(), 2_000);
    }

    #[test]
    fn scan_node_on_empty_and_extreme_nodes_no_panic() {
        let mut v = Vec::new();
        Xml::scan_node(&XmlNode::default(), &mut v);
        Xml::scan_node(&node("", &[], vec![]), &mut v);
        Xml::scan_node(&node(&"a".repeat(10_000), &[("style", "url(x.png)")], vec![]), &mut v);
        assert_eq!(v.len(), 1, "only the inline style url()");
        assert_eq!(v[0].url.as_str(), "x.png");
    }

    #[test]
    fn scan_node_img_srcset_and_background() {
        let mut v = Vec::new();
        Xml::scan_node(
            &node(
                "IMG",
                &[("src", "a.png"), ("srcset", "b.png 1x, c.png 2x"), ("background", "d.gif")],
                vec![],
            ),
            &mut v,
        );
        let urls: Vec<&str> = v.iter().map(|r| r.url.as_str()).collect();
        assert_eq!(urls, vec!["a.png", "b.png", "c.png", "d.gif"]);
        assert!(v.iter().all(|r| r.kind == ExternalResourceKind::Image));
    }

    #[test]
    fn scan_external_resources_on_empty_document() {
        let xml = Xml {
            root: Vec::new().into(),
        };
        assert_eq!(xml.scan_external_resources().as_ref().len(), 0);
    }

    #[test]
    fn scan_external_resources_finds_every_element_kind() {
        let xml = Xml {
            root: vec![
                elem(node("img", &[("src", "i.png")], vec![])),
                elem(node("link", &[("href", "s.css"), ("rel", "stylesheet")], vec![])),
                elem(node("script", &[("src", "s.js")], vec![])),
                elem(node("video", &[("src", "v.mp4"), ("poster", "p.jpg")], vec![])),
                elem(node("audio", &[("src", "a.mp3")], vec![])),
                elem(node("a", &[("href", "f.pdf")], vec![])),
                elem(node("a", &[("href", "/page")], vec![])),
            ]
            .into(),
        };
        let res = xml.scan_external_resources();
        let mut urls: Vec<&str> = res.as_ref().iter().map(|r| r.url.as_str()).collect();
        urls.sort_unstable();
        assert_eq!(
            urls,
            vec!["a.mp3", "f.pdf", "i.png", "p.jpg", "s.css", "s.js", "v.mp4"],
            "`/page` is not a resource and must be skipped"
        );
    }

    // ================================================================
    // MimeTypeHint  (constructor)
    // ================================================================

    #[test]
    fn mime_type_hint_new_no_panic_and_fields_match_args() {
        for s in ["", "   ", "text/css", "\u{1F600}", "\u{0}"] {
            assert_eq!(MimeTypeHint::new(s).inner.as_str(), s, "new() stores verbatim");
        }
        let long = "x".repeat(LONG);
        assert_eq!(MimeTypeHint::new(&long).inner.as_str().len(), LONG);
    }

    #[test]
    fn mime_type_hint_from_extension_edges() {
        assert_eq!(
            MimeTypeHint::from_extension("").inner.as_str(),
            "application/octet-stream"
        );
        assert_eq!(
            MimeTypeHint::from_extension("PnG").inner.as_str(),
            "image/png",
            "extension match is case-insensitive"
        );
        assert_eq!(MimeTypeHint::from_extension("jpeg").inner.as_str(), "image/jpeg");
        assert_eq!(MimeTypeHint::from_extension("woff2").inner.as_str(), "font/woff2");
        assert_eq!(
            MimeTypeHint::from_extension("\u{1F600}").inner.as_str(),
            "application/octet-stream"
        );
        assert_eq!(
            MimeTypeHint::from_extension(&"z".repeat(LONG)).inner.as_str(),
            "application/octet-stream"
        );
    }

    // ================================================================
    // ComponentId  (constructor / getter)
    // ================================================================

    #[test]
    fn component_id_builtin_and_new_invariants() {
        let b = ComponentId::builtin("div");
        assert_eq!(b.collection.as_str(), "builtin");
        assert_eq!(b.name.as_str(), "div");

        let c = ComponentId::new("", "");
        assert_eq!(c.collection.as_str(), "");
        assert_eq!(c.name.as_str(), "");

        let u = ComponentId::new("\u{1F600}", "\u{130}");
        assert_eq!(u.collection.as_str(), "\u{1F600}");
        assert_eq!(u.name.as_str(), "\u{130}");
    }

    #[test]
    fn component_id_qualified_name_roundtrips_through_the_map_lookup() {
        assert_eq!(ComponentId::builtin("div").qualified_name(), "builtin:div");
        assert_eq!(ComponentId::new("", "").qualified_name(), ":");
        // A name that itself contains ':' makes the qualified name ambiguous —
        // pin the (lossy) behavior so a change is noticed.
        assert_eq!(ComponentId::new("a", "b:c").qualified_name(), "a:b:c");
    }

    // ================================================================
    // ComponentFieldTypeBox / ComponentFieldValueBox  (constructor / getter)
    // ================================================================

    #[test]
    fn component_field_type_box_new_as_ref_and_clone() {
        let b = ComponentFieldTypeBox::new(ComponentFieldType::Bool);
        assert!(!b.ptr.is_null());
        assert_eq!(*b.as_ref(), ComponentFieldType::Bool);

        let c = b.clone();
        assert_eq!(*c.as_ref(), ComponentFieldType::Bool);
        assert_ne!(b.ptr, c.ptr, "clone must deep-copy, not alias");
        assert_eq!(b, c, "PartialEq compares pointees");
        drop(c);
        assert_eq!(*b.as_ref(), ComponentFieldType::Bool, "original survives");
    }

    #[test]
    fn component_field_type_box_nested_deeply_drops_cleanly() {
        let mut t = ComponentFieldType::Bool;
        for _ in 0..64 {
            t = ComponentFieldType::OptionType(ComponentFieldTypeBox::new(t));
        }
        assert_eq!(t.format(), format!("{}Bool{}", "Option<".repeat(64), ">".repeat(64)));
        drop(t);
    }

    #[test]
    fn component_field_value_box_new_as_ref_and_clone() {
        let v = ComponentFieldValueBox::new(ComponentFieldValue::I32(i32::MIN));
        assert!(!v.ptr.is_null());
        assert_eq!(*v.as_ref(), ComponentFieldValue::I32(i32::MIN));
        let c = v.clone();
        assert_ne!(v.ptr, c.ptr);
        assert_eq!(v, c);
    }

    // ================================================================
    // ComponentFieldType::parse / parse_depth / format  (round-trip)
    // ================================================================

    #[test]
    fn component_field_type_parse_empty_whitespace_garbage() {
        assert_eq!(ComponentFieldType::parse(""), None);
        assert_eq!(ComponentFieldType::parse("   "), None);
        assert_eq!(ComponentFieldType::parse("\t\n"), None);
        assert_eq!(ComponentFieldType::parse("lowercase"), None);
        assert_eq!(ComponentFieldType::parse("\u{0}\u{7f}"), None);
        assert_eq!(ComponentFieldType::parse("Option<>"), None, "empty inner rejected");
        assert_eq!(ComponentFieldType::parse("Vec<>"), None);
        assert_eq!(ComponentFieldType::parse("Option<lowercase>"), None);
    }

    #[test]
    fn component_field_type_parse_valid_minimal_and_trimming() {
        assert_eq!(ComponentFieldType::parse("String"), Some(ComponentFieldType::String));
        assert_eq!(
            ComponentFieldType::parse("  String  "),
            Some(ComponentFieldType::String),
            "leading/trailing whitespace is trimmed"
        );
        assert_eq!(ComponentFieldType::parse("bool"), Some(ComponentFieldType::Bool));
        assert_eq!(ComponentFieldType::parse("usize"), Some(ComponentFieldType::Usize));
        assert_eq!(
            ComponentFieldType::parse("StructRef(Foo)"),
            Some(ComponentFieldType::StructRef(AzString::from("Foo")))
        );
        assert_eq!(
            ComponentFieldType::parse("EnumRef(Foo)"),
            Some(ComponentFieldType::EnumRef(AzString::from("Foo")))
        );
        assert_eq!(
            ComponentFieldType::parse("RefAny"),
            Some(ComponentFieldType::RefAny(AzString::from("")))
        );
    }

    #[test]
    fn component_field_type_parse_leading_trailing_junk_is_rejected_or_absorbed() {
        // Trailing junk after a known keyword falls through to the
        // "starts uppercase => StructRef" catch-all rather than being rejected.
        assert_eq!(
            ComponentFieldType::parse("String;garbage"),
            Some(ComponentFieldType::StructRef(AzString::from("String;garbage")))
        );
        // Lowercase junk has no uppercase first char => rejected.
        assert_eq!(ComponentFieldType::parse("string;garbage"), None);
    }

    #[test]
    fn component_field_type_parse_boundary_numbers() {
        for s in ["0", "-0", "9223372036854775807", "-9223372036854775808", "1e400", "inf"] {
            assert_eq!(
                ComponentFieldType::parse(s),
                None,
                "numeric literal {s:?} is not a type name"
            );
        }
        // ...but anything starting with an uppercase letter hits the StructRef
        // catch-all, so "NaN" parses as a struct reference rather than failing.
        assert_eq!(
            ComponentFieldType::parse("NaN"),
            Some(ComponentFieldType::StructRef(AzString::from("NaN")))
        );
    }

    #[test]
    fn component_field_type_parse_unicode_no_panic() {
        assert_eq!(ComponentFieldType::parse("\u{1F600}"), None, "emoji is not uppercase");
        // A real uppercase non-ASCII letter hits the StructRef catch-all.
        assert_eq!(
            ComponentFieldType::parse("\u{0391}bc"),
            Some(ComponentFieldType::StructRef(AzString::from("\u{0391}bc")))
        );
    }

    #[test]
    fn component_field_type_parse_depth_boundary_is_exact() {
        // MAX_TYPE_PARSE_DEPTH wrappers parse; one more is rejected.
        let ok = format!(
            "{}Bool{}",
            "Option<".repeat(MAX_TYPE_PARSE_DEPTH),
            ">".repeat(MAX_TYPE_PARSE_DEPTH)
        );
        assert!(
            ComponentFieldType::parse(&ok).is_some(),
            "exactly MAX_TYPE_PARSE_DEPTH wrappers must still parse"
        );

        let too_deep = format!(
            "{}Bool{}",
            "Option<".repeat(MAX_TYPE_PARSE_DEPTH + 1),
            ">".repeat(MAX_TYPE_PARSE_DEPTH + 1)
        );
        assert_eq!(
            ComponentFieldType::parse(&too_deep),
            None,
            "one wrapper past the cap must be rejected, not overflow"
        );
    }

    #[test]
    fn component_field_type_parse_depth_direct_call_honors_start_depth() {
        assert_eq!(
            ComponentFieldType::parse_depth("Bool", MAX_TYPE_PARSE_DEPTH),
            Some(ComponentFieldType::Bool),
            "depth == cap is still allowed"
        );
        assert_eq!(
            ComponentFieldType::parse_depth("Bool", MAX_TYPE_PARSE_DEPTH + 1),
            None
        );
        assert_eq!(
            ComponentFieldType::parse_depth("Bool", usize::MAX),
            None,
            "usize::MAX start depth must not overflow, just refuse"
        );
    }

    #[test]
    fn component_field_type_parse_nested_recursion_does_not_stack_overflow() {
        let bomb = format!("{}Bool{}", "Vec<".repeat(50_000), ">".repeat(50_000));
        assert_eq!(ComponentFieldType::parse(&bomb), None);
    }

    #[test]
    fn component_field_type_parse_extremely_long_terminates() {
        // A single 200k-char uppercase token becomes a StructRef of that name.
        let long = format!("A{}", "b".repeat(LONG));
        assert_eq!(
            ComponentFieldType::parse(&long),
            Some(ComponentFieldType::StructRef(AzString::from(long.as_str())))
        );
    }

    #[test]
    fn component_field_type_round_trip_representative() {
        let representative = vec![
            ComponentFieldType::String,
            ComponentFieldType::Bool,
            ComponentFieldType::I32,
            ComponentFieldType::I64,
            ComponentFieldType::U32,
            ComponentFieldType::U64,
            ComponentFieldType::Usize,
            ComponentFieldType::F32,
            ComponentFieldType::F64,
            ComponentFieldType::ColorU,
            ComponentFieldType::CssProperty,
            ComponentFieldType::ImageRef,
            ComponentFieldType::FontRef,
            ComponentFieldType::StyledDom,
            ComponentFieldType::StructRef(AzString::from("Foo")),
            ComponentFieldType::OptionType(ComponentFieldTypeBox::new(ComponentFieldType::Bool)),
            ComponentFieldType::VecType(ComponentFieldTypeBox::new(ComponentFieldType::I32)),
            ComponentFieldType::RefAny(AzString::from("")),
            ComponentFieldType::RefAny(AzString::from("Hint")),
            ComponentFieldType::Callback(ComponentCallbackSignature {
                return_type: AzString::from("Update"),
                args: Vec::new().into(),
            }),
        ];
        for x in representative {
            let s = x.format();
            assert_eq!(
                ComponentFieldType::parse(&s),
                Some(x.clone()),
                "parse(format({x:?})) must round-trip"
            );
        }
    }

    #[test]
    fn component_field_type_round_trip_edge_values() {
        // Empty / unicode-bearing payloads.
        for x in [
            ComponentFieldType::StructRef(AzString::from("\u{0391}\u{1F600}")),
            ComponentFieldType::OptionType(ComponentFieldTypeBox::new(
                ComponentFieldType::VecType(ComponentFieldTypeBox::new(
                    ComponentFieldType::StructRef(AzString::from("Foo")),
                )),
            )),
        ] {
            assert_eq!(ComponentFieldType::parse(&x.format()), Some(x.clone()));
        }
    }

    #[test]
    fn component_field_type_format_is_an_idempotent_normalization() {
        // `EnumRef` and `StructRef` share the same canonical spelling, so parse()
        // collapses EnumRef -> StructRef. The normalization is still STABLE:
        // format(parse(format(x))) == format(x).
        let e = ComponentFieldType::EnumRef(AzString::from("Role"));
        let once = e.format();
        assert_eq!(once, "Role");
        let reparsed = ComponentFieldType::parse(&once).expect("parses");
        assert_eq!(
            reparsed,
            ComponentFieldType::StructRef(AzString::from("Role")),
            "EnumRef is lossy through format() — it comes back as StructRef"
        );
        assert_eq!(reparsed.format(), once, "but the normalization is idempotent");
    }

    #[test]
    fn component_field_type_display_matches_format() {
        let t = ComponentFieldType::OptionType(ComponentFieldTypeBox::new(ComponentFieldType::F64));
        assert_eq!(format!("{t}"), t.format());
        assert_eq!(format!("{t}"), "Option<F64>");
    }

    #[test]
    fn component_field_type_format_no_panic_on_empty_payloads() {
        let t = ComponentFieldType::Callback(ComponentCallbackSignature {
            return_type: AzString::from(""),
            args: Vec::new().into(),
        });
        assert_eq!(t.format(), "Callback()");
        // Callback() with an empty signature round-trips.
        assert_eq!(ComponentFieldType::parse("Callback()"), Some(t));
    }

    // ================================================================
    // ComponentFieldNamedValueVec::get_field / get_string  (parser-ish lookup)
    // ================================================================

    fn named(name: &str, v: ComponentFieldValue) -> ComponentFieldNamedValue {
        ComponentFieldNamedValue {
            name: AzString::from(name),
            value: v,
        }
    }

    fn named_vec() -> ComponentFieldNamedValueVec {
        vec![
            named("a", ComponentFieldValue::String(AzString::from("x"))),
            named("b", ComponentFieldValue::Bool(true)),
            named("", ComponentFieldValue::U64(u64::MAX)),
            named("\u{1F600}", ComponentFieldValue::String(AzString::from("emoji"))),
        ]
        .into()
    }

    #[test]
    fn named_value_vec_get_field_valid_minimal() {
        let v = named_vec();
        assert_eq!(
            v.get_field("a"),
            Some(&ComponentFieldValue::String(AzString::from("x")))
        );
        assert_eq!(v.get_field("b"), Some(&ComponentFieldValue::Bool(true)));
    }

    #[test]
    fn named_value_vec_get_field_empty_whitespace_garbage_unicode() {
        let v = named_vec();
        // An empty NAME is a legal key here — it matches the field literally named "".
        assert_eq!(v.get_field(""), Some(&ComponentFieldValue::U64(u64::MAX)));
        assert_eq!(v.get_field("   "), None);
        assert_eq!(v.get_field("\t\n"), None);
        assert_eq!(v.get_field("\u{0}\u{7f}"), None);
        assert!(v.get_field("\u{1F600}").is_some());
        assert_eq!(v.get_field(" a "), None, "no trimming: lookup is exact");
        assert_eq!(v.get_field("a;garbage"), None);
    }

    #[test]
    fn named_value_vec_get_field_on_empty_vec_and_long_key() {
        let empty = ComponentFieldNamedValueVec::from_const_slice(&[]);
        assert_eq!(empty.get_field("a"), None);
        assert_eq!(empty.get_string("a"), None);
        assert_eq!(named_vec().get_field(&"z".repeat(LONG)), None);
    }

    #[test]
    fn named_value_vec_get_string_only_matches_string_variant() {
        let v = named_vec();
        assert_eq!(v.get_string("a").map(AzString::as_str), Some("x"));
        assert_eq!(v.get_string("b"), None, "Bool is not a String");
        assert_eq!(v.get_string(""), None, "U64 is not a String");
        assert_eq!(v.get_string("missing"), None);
    }

    #[test]
    fn named_value_vec_boundary_numeric_keys() {
        let v: ComponentFieldNamedValueVec = vec![
            named("0", ComponentFieldValue::I32(0)),
            named("-0", ComponentFieldValue::I32(i32::MIN)),
            named("9223372036854775807", ComponentFieldValue::I64(i64::MAX)),
            named("NaN", ComponentFieldValue::F32(f32::NAN)),
        ]
        .into();
        assert_eq!(v.get_field("0"), Some(&ComponentFieldValue::I32(0)));
        assert_eq!(v.get_field("-0"), Some(&ComponentFieldValue::I32(i32::MIN)));
        assert_eq!(
            v.get_field("9223372036854775807"),
            Some(&ComponentFieldValue::I64(i64::MAX))
        );
        // NaN != NaN, so only check the variant, not equality.
        assert!(matches!(v.get_field("NaN"), Some(ComponentFieldValue::F32(f)) if f.is_nan()));
    }

    // ================================================================
    // ComponentDataModel::get_field / get_default_string / with_default
    // ================================================================

    fn model_with_text() -> ComponentDataModel {
        dm(
            "M",
            vec![
                data_field(
                    "text",
                    ComponentFieldType::String,
                    Some(ComponentDefaultValue::String(AzString::from("hi"))),
                    "",
                ),
                data_field("count", ComponentFieldType::U32, Some(ComponentDefaultValue::U32(3)), ""),
                data_field("required_one", ComponentFieldType::String, None, ""),
            ],
        )
    }

    #[test]
    fn data_model_get_field_valid_minimal_and_missing() {
        let m = model_with_text();
        assert!(m.get_field("text").is_some());
        assert!(m.get_field("count").is_some());
        assert!(m.get_field("missing").is_none());
        assert!(m.get_field("").is_none());
        assert!(m.get_field("   ").is_none());
        assert!(m.get_field(" text ").is_none(), "exact match, no trimming");
        assert!(m.get_field("\u{1F600}").is_none());
        assert!(m.get_field(&"z".repeat(LONG)).is_none());
    }

    #[test]
    fn data_model_get_field_on_empty_model() {
        let m = dm("Empty", Vec::new());
        assert!(m.get_field("anything").is_none());
        assert!(m.get_default_string("anything").is_none());
    }

    #[test]
    fn data_model_get_default_string_only_for_string_defaults() {
        let m = model_with_text();
        assert_eq!(m.get_default_string("text").map(AzString::as_str), Some("hi"));
        assert_eq!(m.get_default_string("count"), None, "U32 default is not a String");
        assert_eq!(m.get_default_string("required_one"), None, "no default at all");
        assert_eq!(m.get_default_string("missing"), None);
    }

    #[test]
    fn data_model_required_flag_follows_default_presence() {
        let m = model_with_text();
        assert!(!m.get_field("text").unwrap().required);
        assert!(
            m.get_field("required_one").unwrap().required,
            "a field with no default must be marked required"
        );
    }

    #[test]
    fn data_model_with_default_overrides_and_preserves_len() {
        let m = model_with_text();
        let before = m.fields.as_ref().len();
        let m = m.with_default("text", ComponentDefaultValue::String(AzString::from("bye")));
        assert_eq!(m.fields.as_ref().len(), before, "len is preserved");
        assert_eq!(m.get_default_string("text").map(AzString::as_str), Some("bye"));
    }

    #[test]
    fn data_model_with_default_on_missing_field_is_a_no_op() {
        let m = model_with_text();
        let m = m.with_default("nope", ComponentDefaultValue::Bool(true));
        assert_eq!(m.fields.as_ref().len(), 3);
        assert_eq!(m.get_default_string("text").map(AzString::as_str), Some("hi"));
        assert!(m.get_field("nope").is_none(), "no field is inserted");
    }

    #[test]
    fn data_model_with_default_extreme_names_and_values_no_panic() {
        let m = model_with_text()
            .with_default("", ComponentDefaultValue::None)
            .with_default(&"z".repeat(10_000), ComponentDefaultValue::F64(f64::NAN))
            .with_default("count", ComponentDefaultValue::Usize(usize::MAX))
            .with_default("text", ComponentDefaultValue::I64(i64::MIN));
        assert_eq!(m.fields.as_ref().len(), 3);
        assert!(matches!(
            m.get_field("count").unwrap().default_value,
            OptionComponentDefaultValue::Some(ComponentDefaultValue::Usize(usize::MAX))
        ));
        assert_eq!(
            m.get_default_string("text"),
            None,
            "text is now an I64 default, no longer a String"
        );
    }

    #[test]
    fn data_model_with_default_fills_only_the_first_match() {
        // Duplicate field names: `with_default` breaks after the first hit.
        let m = dm(
            "Dup",
            vec![
                data_field("x", ComponentFieldType::String, Some(ComponentDefaultValue::String(AzString::from("1"))), ""),
                data_field("x", ComponentFieldType::String, Some(ComponentDefaultValue::String(AzString::from("2"))), ""),
            ],
        )
        .with_default("x", ComponentDefaultValue::String(AzString::from("3")));
        let vals: Vec<&str> = m
            .fields
            .as_ref()
            .iter()
            .filter_map(|f| match &f.default_value {
                OptionComponentDefaultValue::Some(ComponentDefaultValue::String(s)) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(vals, vec!["3", "2"], "only the first duplicate is overridden");
    }

    // ================================================================
    // ComponentSource / ComponentMap  (constructor / getter / lookup)
    // ================================================================

    #[test]
    fn component_source_create_is_user_defined() {
        assert_eq!(ComponentSource::create(), ComponentSource::UserDefined);
        assert_eq!(ComponentSource::default(), ComponentSource::UserDefined);
    }

    #[test]
    fn component_map_create_is_empty_and_all_lookups_return_none() {
        let m = ComponentMap::create();
        assert_eq!(m.libraries.as_ref().len(), 0);
        assert!(m.get("builtin", "div").is_none());
        assert!(m.get_unqualified("div").is_none());
        assert!(m.get_by_qualified_name("builtin:div").is_none());
        assert!(m.all_components().is_empty());
        assert!(m.get_exportable_libraries().is_empty());
    }

    #[test]
    fn component_map_with_builtin_invariants() {
        let m = ComponentMap::with_builtin();
        assert_eq!(m.libraries.as_ref().len(), 1);
        let lib = &m.libraries.as_ref()[0];
        assert_eq!(lib.name.as_str(), "builtin");
        assert!(!lib.exportable, "builtins must never be exportable");
        assert!(!lib.modifiable, "builtins must never be modifiable");
        assert_eq!(
            m.all_components().len(),
            lib.components.as_ref().len(),
            "all_components must see every registered component"
        );
        assert!(
            m.get_exportable_libraries().is_empty(),
            "the builtin library is not exportable"
        );
    }

    #[test]
    fn component_map_get_valid_minimal() {
        let m = ComponentMap::with_builtin();
        assert!(m.get("builtin", "div").is_some());
        assert!(m.get("builtin", "if").is_some());
        assert!(m.get("builtin", "for").is_some());
        assert!(m.get("builtin", "map").is_some());
        assert_eq!(
            m.get("builtin", "div").unwrap().id.qualified_name(),
            "builtin:div"
        );
    }

    #[test]
    fn component_map_get_empty_whitespace_garbage_unicode() {
        let m = ComponentMap::with_builtin();
        assert!(m.get("", "").is_none());
        assert!(m.get("builtin", "").is_none());
        assert!(m.get("", "div").is_none());
        assert!(m.get("builtin", "   ").is_none());
        assert!(m.get("builtin", " div ").is_none(), "no trimming");
        assert!(m.get("builtin", "DIV").is_none(), "lookup is case-sensitive");
        assert!(m.get("builtin", "\u{1F600}").is_none());
        assert!(m.get("builtin", "\u{0}\u{7f}").is_none());
        assert!(m.get("builtin", "div;garbage").is_none());
    }

    #[test]
    fn component_map_get_extremely_long_name_terminates() {
        let m = ComponentMap::with_builtin();
        assert!(m.get(&"a".repeat(LONG), &"b".repeat(LONG)).is_none());
        assert!(m.get_unqualified(&"b".repeat(LONG)).is_none());
        assert!(m.get_by_qualified_name(&"c".repeat(LONG)).is_none());
    }

    #[test]
    fn component_map_get_unqualified_only_searches_builtin() {
        let lib = ComponentLibrary {
            name: AzString::from("mylib"),
            version: AzString::from("1.0.0"),
            description: AzString::from(""),
            components: vec![user_def("", Vec::new())].into(),
            exportable: true,
            modifiable: true,
            data_models: Vec::new().into(),
            enum_models: Vec::new().into(),
        };
        let libs: ComponentLibraryVec = vec![lib].into();
        let m = ComponentMap::from_libraries(&libs);

        assert!(m.get("mylib", "widget").is_some());
        assert!(
            m.get_unqualified("widget").is_none(),
            "unqualified lookup must NOT reach non-builtin libraries"
        );
        assert!(m.get_by_qualified_name("mylib:widget").is_some());
        assert_eq!(m.get_exportable_libraries().len(), 1);
        assert_eq!(m.all_components().len(), 1);
    }

    #[test]
    fn component_map_get_by_qualified_name_boundary_forms() {
        let m = ComponentMap::with_builtin();
        // No colon => falls back to the builtin library.
        assert!(m.get_by_qualified_name("div").is_some());
        // Exactly one colon.
        assert!(m.get_by_qualified_name("builtin:div").is_some());
        // Splits on the FIRST colon, so the remainder (incl. colons) is the name.
        assert!(m.get_by_qualified_name("builtin:div:extra").is_none());
        assert!(m.get_by_qualified_name(":").is_none());
        assert!(m.get_by_qualified_name(":div").is_none());
        assert!(m.get_by_qualified_name("builtin:").is_none());
        assert!(m.get_by_qualified_name("").is_none());
        assert!(m.get_by_qualified_name("   ").is_none());
    }

    #[test]
    fn component_map_from_libraries_clones_without_losing_entries() {
        let src = ComponentMap::with_builtin();
        let copy = ComponentMap::from_libraries(&src.libraries);
        assert_eq!(copy.all_components().len(), src.all_components().len());
        assert!(copy.get_unqualified("div").is_some());
    }

    #[test]
    fn register_builtin_components_is_stable_across_calls() {
        let a = register_builtin_components();
        let b = register_builtin_components();
        assert_eq!(a.components.as_ref().len(), b.components.as_ref().len());
        assert!(a.components.as_ref().len() > 50);
        assert_eq!(a.name.as_str(), "builtin");
        assert_eq!(a.version.as_str(), "1.0.0");
        // Every component must be namespaced into "builtin".
        assert!(a
            .components
            .as_ref()
            .iter()
            .all(|c| c.id.collection.as_str() == "builtin"));
        assert!(a
            .components
            .as_ref()
            .iter()
            .all(|c| c.source == ComponentSource::Builtin));
    }

    // ================================================================
    // XmlNodeChild / XmlNode  (getter / predicate / constructor)
    // ================================================================

    #[test]
    fn xml_node_child_as_text_and_as_element_are_mutually_exclusive() {
        let t = txt("hello");
        assert_eq!(t.as_text(), Some("hello"));
        assert!(t.as_element().is_none());

        let e = elem(node("div", &[], vec![]));
        assert!(e.as_text().is_none());
        assert_eq!(e.as_element().map(|n| n.node_type.as_str()), Some("div"));
    }

    #[test]
    fn xml_node_child_as_text_edge_values() {
        assert_eq!(txt("").as_text(), Some(""), "an empty text node is still text");
        assert_eq!(txt("   ").as_text(), Some("   "), "no trimming in the getter");
        assert_eq!(txt("\u{1F600}\u{0301}").as_text(), Some("\u{1F600}\u{0301}"));
        assert_eq!(txt("\u{0}").as_text(), Some("\u{0}"));
    }

    #[test]
    fn xml_node_child_as_element_mut_allows_mutation_and_rejects_text() {
        let mut e = elem(node("div", &[], vec![]));
        e.as_element_mut().expect("is an element").node_type = "span".into();
        assert_eq!(e.as_element().map(|n| n.node_type.as_str()), Some("span"));

        let mut t = txt("x");
        assert!(t.as_element_mut().is_none(), "text nodes have no element");
    }

    #[test]
    fn xml_node_create_and_with_children_invariants() {
        let n = XmlNode::create("div");
        assert_eq!(n.node_type.as_str(), "div");
        assert_eq!(n.children.as_ref().len(), 0);
        assert_eq!(n.attributes.as_ref().len(), 0);

        let n = n.with_children(vec![txt("a"), elem(XmlNode::create("b"))]);
        assert_eq!(n.children.as_ref().len(), 2);
        assert_eq!(n.node_type.as_str(), "div", "tag survives with_children");

        // with_children REPLACES, it does not append.
        let n = n.with_children(Vec::new());
        assert_eq!(n.children.as_ref().len(), 0);
    }

    #[test]
    fn xml_node_create_extreme_tag_names_no_panic() {
        assert_eq!(XmlNode::create("").node_type.as_str(), "");
        assert_eq!(XmlNode::create("\u{1F600}").node_type.as_str(), "\u{1F600}");
        assert_eq!(
            XmlNode::create(&*"a".repeat(10_000)).node_type.as_str().len(),
            10_000
        );
    }

    #[test]
    fn xml_node_get_text_content_concatenates_only_direct_text() {
        let n = node(
            "p",
            &[],
            vec![
                txt("a "),
                elem(node("span", &[], vec![txt("IGNORED")])),
                txt("b"),
            ],
        );
        assert_eq!(
            n.get_text_content(),
            "a b",
            "only DIRECT text children, nested element text is not included"
        );
        assert_eq!(XmlNode::default().get_text_content(), "");
        assert_eq!(node("p", &[], vec![txt(""), txt("")]).get_text_content(), "");
    }

    #[test]
    fn xml_node_has_only_text_children_true_false_and_empty() {
        assert!(
            XmlNode::default().has_only_text_children(),
            "vacuously true for a childless node — callers must pair this with a text check"
        );
        assert!(node("p", &[], vec![txt("a"), txt("b")]).has_only_text_children());
        assert!(!node("p", &[], vec![txt("a"), elem(XmlNode::create("b"))]).has_only_text_children());
        assert!(!node("p", &[], vec![elem(XmlNode::create("b"))]).has_only_text_children());
    }

    // ================================================================
    // get_html_node / get_body_node / find_node_by_type / find_attribute
    // ================================================================

    #[test]
    fn get_html_node_empty_input_is_no_html_node() {
        assert_eq!(get_html_node(&[]), Err(DomXmlParseError::NoHtmlNode));
        assert_eq!(get_html_node(&[txt("just text")]), Err(DomXmlParseError::NoHtmlNode));
        assert_eq!(
            get_html_node(&[elem(XmlNode::create("div"))]),
            Err(DomXmlParseError::NoHtmlNode)
        );
    }

    #[test]
    fn get_html_node_valid_minimal_and_case_insensitive() {
        let roots = vec![elem(XmlNode::create("HTML"))];
        assert!(get_html_node(&roots).is_ok(), "tag casing is normalized");
    }

    #[test]
    fn get_html_node_rejects_multiple_roots() {
        let roots = vec![elem(XmlNode::create("html")), elem(XmlNode::create("html"))];
        assert_eq!(
            get_html_node(&roots),
            Err(DomXmlParseError::MultipleHtmlRootNodes)
        );
    }

    #[test]
    fn get_body_node_empty_and_missing() {
        assert_eq!(get_body_node(&[]), Err(DomXmlParseError::NoBodyInHtml));
        assert_eq!(
            get_body_node(&[elem(XmlNode::create("head"))]),
            Err(DomXmlParseError::NoBodyInHtml)
        );
    }

    #[test]
    fn get_body_node_direct_and_nested() {
        let direct = vec![elem(XmlNode::create("body"))];
        assert!(get_body_node(&direct).is_ok());

        // Malformed markup: <body> buried inside <head>.
        let nested = vec![elem(node(
            "head",
            &[],
            vec![elem(node("div", &[], vec![elem(XmlNode::create("BODY"))]))],
        ))];
        assert!(
            get_body_node(&nested).is_ok(),
            "the recursive fallback finds a nested body"
        );
    }

    /// Build a `<div>` chain `depth` levels deep with `inner` at the bottom.
    fn wrap_divs(depth: usize, inner: XmlNode) -> XmlNode {
        let mut n = inner;
        for _ in 0..depth {
            n = node("div", &[], vec![elem(n)]);
        }
        n
    }

    #[test]
    fn get_body_node_deep_nesting_is_depth_capped_not_stack_overflowing() {
        // Body sits just inside the cap => found.
        let ok = vec![elem(wrap_divs(
            MAX_XML_NESTING_DEPTH - 2,
            XmlNode::create("body"),
        ))];
        assert!(get_body_node(&ok).is_ok(), "body within the depth cap is found");

        // Body far below the cap => reported missing, but MUST NOT overflow.
        let too_deep = vec![elem(wrap_divs(2_000, XmlNode::create("body")))];
        assert_eq!(
            get_body_node(&too_deep),
            Err(DomXmlParseError::NoBodyInHtml),
            "past MAX_XML_NESTING_DEPTH the search gives up instead of crashing"
        );
    }

    #[test]
    fn find_node_by_type_empty_garbage_unicode() {
        assert!(find_node_by_type(&[], "div").is_none());
        assert!(find_node_by_type(&[txt("x")], "div").is_none());

        let roots = vec![elem(XmlNode::create("div"))];
        assert!(find_node_by_type(&roots, "").is_none());
        assert!(find_node_by_type(&roots, "   ").is_none());
        assert!(find_node_by_type(&roots, "\u{1F600}").is_none());
        assert!(find_node_by_type(&roots, &"z".repeat(LONG)).is_none());
        // Tag matching is ASCII case-insensitive (HTML tags are), so "DIV" matches a
        // <div> node. (It used to compare against the normalize_casing'd tag, which was
        // case-sensitive on the needle AND mangled uppercase tags to "d_i_v".)
        assert!(find_node_by_type(&roots, "DIV").is_some());
    }

    #[test]
    fn find_node_by_type_valid_minimal_and_recursive() {
        let roots = vec![elem(node(
            "html",
            &[],
            vec![elem(node("head", &[], vec![elem(XmlNode::create("STYLE"))]))],
        ))];
        assert!(find_node_by_type(&roots, "html").is_some());
        assert!(
            find_node_by_type(&roots, "style").is_some(),
            "search recurses into the whole tree"
        );
        assert!(find_node_by_type(&roots, "body").is_none());
    }

    #[test]
    fn find_node_by_type_prefers_the_shallowest_match() {
        let roots = vec![
            elem(node("div", &[], vec![elem(XmlNode::create("span"))])),
            elem(node("span", &[("id", "shallow")], vec![])),
        ];
        let found = find_node_by_type(&roots, "span").expect("found");
        assert_eq!(
            found.attributes.get_key("id").map(AzString::as_str),
            Some("shallow"),
            "direct children are scanned before recursing"
        );
    }

    #[test]
    fn find_attribute_valid_minimal_and_missing() {
        let n = node("a", &[("href", "x"), ("id", "y")], vec![]);
        assert_eq!(find_attribute(&n, "href").map(AzString::as_str), Some("x"));
        assert_eq!(find_attribute(&n, "id").map(AzString::as_str), Some("y"));
        assert!(find_attribute(&n, "missing").is_none());
        assert!(find_attribute(&n, "").is_none());
        assert!(find_attribute(&n, "   ").is_none());
        assert!(find_attribute(&XmlNode::default(), "href").is_none());
        assert!(find_attribute(&n, &"z".repeat(LONG)).is_none());
    }

    #[test]
    fn find_attribute_compares_against_the_normalized_key() {
        // `normalize_casing` turns `aria-label` into `aria_label`, so callers must
        // pass the NORMALIZED spelling — the raw HTML spelling does not match.
        let n = node("button", &[("aria-label", "Save")], vec![]);
        assert_eq!(
            find_attribute(&n, "aria_label").map(AzString::as_str),
            Some("Save")
        );
        assert!(
            find_attribute(&n, "aria-label").is_none(),
            "the hyphenated spelling never matches (keys are normalized first)"
        );
    }

    #[test]
    fn find_attribute_unicode_keys_no_panic() {
        let n = node("div", &[("\u{130}", "v"), ("\u{1F600}", "w")], vec![]);
        assert!(find_attribute(&n, "\u{1F600}").is_some(), "emoji keys pass through");
        // 'İ' lowercases to 2 chars, so the normalized key is not 'İ'.
        assert!(find_attribute(&n, "\u{130}").is_none());
    }

    // ================================================================
    // normalize_casing
    // ================================================================

    #[test]
    fn normalize_casing_documented_forms() {
        assert_eq!(normalize_casing("abcDef"), "abc_def");
        assert_eq!(normalize_casing("AbcDef"), "abc_def");
        assert_eq!(normalize_casing("abc-def"), "abc_def");
        assert_eq!(normalize_casing("abc_def"), "abc_def");
    }

    #[test]
    fn normalize_casing_edge_inputs() {
        assert_eq!(normalize_casing(""), "");
        assert_eq!(normalize_casing("---"), "", "separators alone produce no words");
        assert_eq!(normalize_casing("___"), "");
        assert_eq!(normalize_casing("A"), "a");
        assert_eq!(
            normalize_casing("ABC"),
            "a_b_c",
            "every uppercase char starts a new word"
        );
        assert_eq!(normalize_casing("h1"), "h1");
        assert_eq!(normalize_casing("   "), "   ", "whitespace is not a separator");
    }

    #[test]
    fn normalize_casing_unicode_and_long_no_panic() {
        // 'İ' (U+0130) lowercases to TWO chars — the fn must not slice bytes.
        assert_eq!(normalize_casing("\u{130}"), "i\u{307}");
        assert_eq!(normalize_casing("\u{1F600}"), "\u{1F600}");
        assert_eq!(normalize_casing(&"a".repeat(50_000)).len(), 50_000);
        // 50k uppercase chars => 50k single-char words joined by '_'.
        assert_eq!(normalize_casing(&"A".repeat(50_000)).len(), 50_000 * 2 - 1);
    }

    // ================================================================
    // get_item / get_item_internal
    // ================================================================

    #[test]
    fn get_item_empty_hierarchy_returns_the_root() {
        let mut root = node("div", &[("id", "root")], vec![]);
        let got = get_item(&[], &mut root).expect("empty hierarchy => root");
        assert_eq!(got.attributes.get_key("id").map(AzString::as_str), Some("root"));
    }

    #[test]
    fn get_item_walks_nested_elements() {
        let mut root = node(
            "a",
            &[],
            vec![elem(node("b", &[], vec![elem(node("c", &[("id", "deep")], vec![]))]))],
        );
        let got = get_item(&[0, 0], &mut root).expect("a > b > c");
        assert_eq!(got.node_type.as_str(), "c");
        assert_eq!(got.attributes.get_key("id").map(AzString::as_str), Some("deep"));
    }

    #[test]
    fn get_item_out_of_bounds_and_text_nodes_return_none() {
        let mut root = node("a", &[], vec![txt("hello"), elem(XmlNode::create("b"))]);
        assert!(get_item(&[5], &mut root).is_none(), "out of bounds => None");
        assert!(
            get_item(&[usize::MAX], &mut root).is_none(),
            "usize::MAX index must not panic"
        );
        assert!(
            get_item(&[0], &mut root).is_none(),
            "index 0 is a TEXT node — not traversable"
        );
        assert!(get_item(&[1], &mut root).is_some());
        assert!(
            get_item(&[1, 0], &mut root).is_none(),
            "descending past a leaf => None"
        );
    }

    #[test]
    fn get_item_deep_hierarchy_terminates() {
        // 400 levels: deep, but bounded by the (short) hierarchy vec, not by markup.
        let mut root = wrap_divs(400, node("div", &[("id", "bottom")], vec![]));
        let path = vec![0usize; 400];
        let got = get_item(&path, &mut root).expect("reaches the bottom");
        assert_eq!(got.attributes.get_key("id").map(AzString::as_str), Some("bottom"));
    }

    // ================================================================
    // decode_numeric_entity  (parser)
    // ================================================================

    #[test]
    fn decode_numeric_entity_valid_minimal() {
        assert_eq!(decode_numeric_entity("#65"), Some('A'));
        assert_eq!(decode_numeric_entity("#x41"), Some('A'));
        assert_eq!(decode_numeric_entity("#X41"), Some('A'), "uppercase X accepted");
        assert_eq!(decode_numeric_entity("#x1F600"), Some('\u{1F600}'));
    }

    #[test]
    fn decode_numeric_entity_empty_whitespace_garbage() {
        assert_eq!(decode_numeric_entity(""), None);
        assert_eq!(decode_numeric_entity("   "), None);
        assert_eq!(decode_numeric_entity("\t\n"), None);
        assert_eq!(decode_numeric_entity("amp"), None, "named entity is not numeric");
        assert_eq!(decode_numeric_entity("#"), None);
        assert_eq!(decode_numeric_entity("#x"), None);
        assert_eq!(decode_numeric_entity("#zz"), None);
        assert_eq!(decode_numeric_entity("# 65"), None);
        assert_eq!(decode_numeric_entity("#65junk"), None);
        assert_eq!(decode_numeric_entity("\u{1F600}"), None);
    }

    #[test]
    fn decode_numeric_entity_boundary_code_points() {
        assert_eq!(decode_numeric_entity("#0"), Some('\u{0}'), "NUL is a valid char");
        assert_eq!(decode_numeric_entity("#x10FFFF"), Some('\u{10FFFF}'), "max scalar");
        assert_eq!(
            decode_numeric_entity("#x110000"),
            None,
            "one past the max scalar value"
        );
        assert_eq!(
            decode_numeric_entity("#xD800"),
            None,
            "a lone surrogate is not a char"
        );
        assert_eq!(
            decode_numeric_entity("#4294967295"),
            None,
            "u32::MAX is not a scalar value"
        );
        assert_eq!(
            decode_numeric_entity("#4294967296"),
            None,
            "one past u32::MAX must not wrap — it must fail to parse"
        );
        assert_eq!(decode_numeric_entity("#-1"), None, "negative is rejected by u32");
        assert_eq!(
            decode_numeric_entity("#xFFFFFFFFFFFF"),
            None,
            "hex overflow is rejected, not truncated"
        );
    }

    #[test]
    fn decode_numeric_entity_extremely_long_terminates() {
        let s = format!("#{}", "9".repeat(LONG));
        assert_eq!(s.len(), LONG + 1);
        assert_eq!(decode_numeric_entity(&s), None, "overflows u32 => None");
    }

    // ================================================================
    // decode_entities / prepare_string
    // ================================================================

    #[test]
    fn decode_entities_leaves_unrecognized_sequences_verbatim() {
        assert_eq!(decode_entities(""), "");
        assert_eq!(decode_entities("&"), "&", "a bare '&' at EOF must not panic");
        assert_eq!(decode_entities("&;"), "&;", "empty entity body is not decoded");
        assert_eq!(decode_entities("&bogus;"), "&bogus;");
        assert_eq!(decode_entities("&nbsp;"), "&nbsp;", "&nbsp; is deliberately kept");
        // A ';' further away than MAX_ENTITY_BODY is not treated as an entity end.
        assert_eq!(
            decode_entities("&averyveryverylongbody;"),
            "&averyveryverylongbody;"
        );
    }

    #[test]
    fn decode_entities_single_pass_prevents_double_decoding() {
        assert_eq!(decode_entities("&amp;lt;"), "&lt;", "&amp; must not re-open an entity");
        assert_eq!(decode_entities("&lt;&gt;&amp;&quot;&apos;"), "<>&\"'");
    }

    #[test]
    fn decode_entities_unicode_and_long_input_terminate() {
        assert_eq!(decode_entities("\u{1F600}\u{0301}\u{130}"), "\u{1F600}\u{0301}\u{130}");
        // NOTE: each '&' triggers a `find(';')` over the whole remaining suffix, so
        // this is quadratic in the number of '&'. Keep the input modest.
        let amps = "&".repeat(20_000);
        assert_eq!(decode_entities(&amps).len(), 20_000);
    }

    #[test]
    fn prepare_string_empty_and_whitespace() {
        assert_eq!(prepare_string(""), "");
        assert_eq!(prepare_string("   "), "");
        assert_eq!(prepare_string("\t\n\r  \n"), "");
    }

    #[test]
    fn prepare_string_nbsp_becomes_a_space_that_survives_trim() {
        assert_eq!(prepare_string("&nbsp;"), " ");
        assert_eq!(prepare_string("a&nbsp;b"), "a b");
    }

    #[test]
    fn prepare_string_collapses_a_blank_line_into_a_single_return() {
        assert_eq!(prepare_string("a\n\nb"), "a\nb");
        assert_eq!(prepare_string("a\n\n\n\nb"), "a\nb", "runs of blanks collapse");
    }

    #[test]
    fn prepare_string_unicode_and_long_input_no_panic() {
        assert_eq!(prepare_string("\u{1F600}"), "\u{1F600}");
        assert_eq!(prepare_string("  \u{130}\u{0301}  "), "\u{130}\u{0301}");
        assert_eq!(prepare_string(&"x".repeat(LONG)).len(), LONG);
    }

    /// BUG (reported): a soft-wrapped multi-line text node loses the word break
    /// on the FINAL line, because `prepare_string` skips the joining space when
    /// `line_idx == line_len - 1`. HTML must collapse the newline into a space.
    #[test]
    fn prepare_string_joins_wrapped_lines_with_a_space() {
        assert_eq!(
            prepare_string("Hello\nworld"),
            "Hello world",
            "a single newline between words must collapse to a space, not vanish"
        );
        assert_eq!(prepare_string("a\nb\nc"), "a b c");
    }

    // ================================================================
    // parse_bool  (parser)
    // ================================================================

    #[test]
    fn parse_bool_valid_minimal_and_everything_else_is_none() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("false"), Some(false));

        for s in [
            "", "   ", "\t\n", " true", "true ", "TRUE", "True", "FALSE", "1", "0", "-0", "yes",
            "no", "NaN", "inf", "9223372036854775807", "\u{1F600}", "true;garbage",
        ] {
            assert_eq!(parse_bool(s), None, "{s:?} must not parse as a bool");
        }
        assert_eq!(parse_bool(&"a".repeat(LONG)), None);
    }

    // ================================================================
    // split_dynamic_string / format_args_dynamic
    // ================================================================

    fn args(kv: &[(&str, &str)]) -> ComponentArgumentVec {
        kv.iter()
            .map(|(n, t)| ComponentArgument {
                name: AzString::from(*n),
                arg_type: AzString::from(*t),
            })
            .collect::<Vec<_>>()
            .into()
    }

    #[test]
    fn split_dynamic_string_empty_and_plain() {
        assert!(split_dynamic_string("").is_empty());
        assert_eq!(
            split_dynamic_string("abc"),
            vec![DynamicItem::Str("abc".to_string())]
        );
    }

    #[test]
    fn split_dynamic_string_format_spec_is_split_on_the_first_colon() {
        assert_eq!(
            split_dynamic_string("{a:?}"),
            vec![DynamicItem::Var {
                name: "a".to_string(),
                format_spec: Some("?".to_string()),
            }]
        );
        assert_eq!(
            split_dynamic_string("{a:x:y}"),
            vec![DynamicItem::Var {
                name: "a".to_string(),
                format_spec: Some("x:y".to_string()),
            }],
            "only the FIRST colon separates name from spec"
        );
    }

    #[test]
    fn split_dynamic_string_unterminated_var_stays_literal() {
        // No closing brace => the scan runs to EOF and the text stays a literal.
        assert_eq!(
            split_dynamic_string("{unterminated"),
            vec![DynamicItem::Str("{unterminated".to_string())]
        );
        // A whitespace inside the braces aborts the variable scan.
        assert_eq!(
            split_dynamic_string("{a b}"),
            vec![DynamicItem::Str("{a b}".to_string())]
        );
    }

    #[test]
    fn split_dynamic_string_unicode_and_long_input_terminate() {
        assert_eq!(
            split_dynamic_string("\u{1F600}{v}\u{130}"),
            vec![
                DynamicItem::Str("\u{1F600}".to_string()),
                DynamicItem::Var {
                    name: "v".to_string(),
                    format_spec: None
                },
                DynamicItem::Str("\u{130}".to_string()),
            ]
        );
        // The failed-variable scan advances the cursor by the amount it scanned,
        // so this stays linear rather than quadratic.
        let bomb = "{a".repeat(50_000);
        let out = split_dynamic_string(&bomb);
        assert!(out.len() <= 2, "a never-closed variable collapses, got {}", out.len());

        let braces = "{".repeat(100_000);
        assert!(split_dynamic_string(&braces).len() <= 1);
    }

    #[test]
    fn format_args_dynamic_documented_example() {
        let vars = args(&[("a", "value1"), ("b", "value2")]);
        assert_eq!(
            format_args_dynamic("hello {a}, {b}{{ {c} }}", &vars),
            "hello value1, value2{ {c} }"
        );
    }

    #[test]
    fn format_args_dynamic_unknown_var_is_preserved_verbatim() {
        let empty = no_args();
        assert_eq!(format_args_dynamic("{c}", &empty), "{c}");
        assert_eq!(
            format_args_dynamic("{c:?}", &empty),
            "{c:?}",
            "the format spec is re-attached when the var is unresolved"
        );
        // Escaped braces round-trip to themselves.
        assert_eq!(format_args_dynamic("{{}}", &empty), "{{}}");
    }

    #[test]
    fn format_args_dynamic_variable_names_are_normalized() {
        let vars = args(&[("my_var", "V")]);
        assert_eq!(format_args_dynamic("{myVar}", &vars), "V", "camelCase is normalized");
        assert_eq!(format_args_dynamic("{ my-var }", &vars), "{ my-var }",
            "whitespace inside the braces aborts the variable scan entirely");
        assert_eq!(format_args_dynamic("{my-var}", &vars), "V");
    }

    #[test]
    fn format_args_dynamic_edge_values_no_panic() {
        let empty = no_args();
        assert_eq!(format_args_dynamic("", &empty), "");
        assert_eq!(format_args_dynamic("   ", &empty), "   ");
        assert_eq!(format_args_dynamic("\u{1F600}", &empty), "\u{1F600}");
        assert_eq!(format_args_dynamic(&"x".repeat(50_000), &empty).len(), 50_000);
    }

    #[test]
    fn combine_and_replace_dynamic_items_on_empty_input() {
        assert_eq!(combine_and_replace_dynamic_items(&[], &no_args()), "");
    }

    // ================================================================
    // compile_and_format_dynamic_items / format_args_for_rust_code
    // ================================================================

    #[test]
    fn format_args_for_rust_code_empty_and_single_items() {
        assert_eq!(format_args_for_rust_code(""), "AzString::from_const_str(\"\")");
        assert_eq!(format_args_for_rust_code("hi"), "AzString::from_const_str(\"hi\")");
        assert_eq!(format_args_for_rust_code("{a}"), "a", "a lone var becomes a bare ident");
        assert_eq!(
            format_args_for_rust_code("{a:?}"),
            "format!(\"{:?}\", a).into()"
        );
    }

    #[test]
    fn format_args_for_rust_code_multi_item_builds_a_format_call() {
        assert_eq!(
            format_args_for_rust_code("x={a} y={b}"),
            "format!(\"x={a} y={b}\", a, b).into()"
        );
    }

    #[test]
    fn format_args_for_rust_code_escapes_quotes_in_literals() {
        let out = format_args_for_rust_code("say \"hi\" {a}");
        assert!(
            out.contains("say \\\"hi\\\""),
            "double quotes must be escaped for the emitted literal, got {out}"
        );
    }

    #[test]
    fn compile_and_format_dynamic_items_edge_values() {
        assert_eq!(
            compile_and_format_dynamic_items(&[]),
            "AzString::from_const_str(\"\")"
        );
        assert_eq!(
            compile_and_format_dynamic_items(&[DynamicItem::Str(String::new())]),
            "AzString::from_const_str(\"\")"
        );
        assert_eq!(
            compile_and_format_dynamic_items(&[DynamicItem::Var {
                name: "  spaced  ".to_string(),
                format_spec: None,
            }]),
            "spaced",
            "the var name is trimmed + normalized"
        );
    }

    // ================================================================
    // cap_first / camel_to_snake / esc_lit / c_creator_suffix
    // ================================================================

    #[test]
    fn cap_first_edge_inputs() {
        assert_eq!(cap_first(""), "", "empty input must not panic");
        assert_eq!(cap_first("h1"), "H1");
        assert_eq!(cap_first("button"), "Button");
        assert_eq!(cap_first("A"), "A", "already-uppercase is idempotent");
        assert_eq!(cap_first("\u{1F600}x"), "\u{1F600}x", "emoji has no uppercase form");
        // 'ß' uppercases to TWO chars — the fn must not assume 1:1.
        assert_eq!(cap_first("\u{df}x"), "SSx");
        assert_eq!(cap_first(&"a".repeat(10_000)).len(), 10_000);
    }

    #[test]
    fn camel_to_snake_documented_forms() {
        assert_eq!(camel_to_snake("ButtonNoA11y"), "button_no_a11y");
        assert_eq!(camel_to_snake("PWithText"), "p_with_text");
        assert_eq!(camel_to_snake("ANoA11y"), "a_no_a11y");
        assert_eq!(camel_to_snake("H1WithText"), "h1_with_text");
        assert_eq!(camel_to_snake("Div"), "div");
    }

    #[test]
    fn camel_to_snake_edge_inputs() {
        assert_eq!(camel_to_snake(""), "");
        assert_eq!(camel_to_snake("A"), "a");
        assert_eq!(camel_to_snake("AB"), "ab", "an all-caps run is not split");
        assert_eq!(camel_to_snake("ABc"), "a_bc", "a caps run splits before the last cap");
        assert_eq!(camel_to_snake("\u{1F600}"), "\u{1F600}");
        assert_eq!(camel_to_snake(&"a".repeat(10_000)).len(), 10_000);
    }

    #[test]
    fn esc_lit_escapes_backslash_before_quote() {
        assert_eq!(esc_lit(""), "");
        assert_eq!(esc_lit("plain"), "plain");
        assert_eq!(esc_lit("a\"b"), "a\\\"b");
        assert_eq!(esc_lit("a\\b"), "a\\\\b");
        // The backslash pass must run FIRST so an escaped quote is not double-escaped.
        assert_eq!(esc_lit("\\\""), "\\\\\\\"");
        assert_eq!(esc_lit("\u{1F600}"), "\u{1F600}");
    }

    #[test]
    fn c_creator_suffix_edge_inputs() {
        assert_eq!(c_creator_suffix(""), "Div", "empty debug name falls back to Div");
        assert_eq!(c_creator_suffix("Div"), "Div");
        assert_eq!(c_creator_suffix("H1"), "H1");
        assert_eq!(c_creator_suffix("BlockQuote"), "Blockquote");
        assert_eq!(c_creator_suffix("FigCaption"), "Figcaption");
        assert_eq!(c_creator_suffix("\u{1F600}"), "\u{1F600}");
    }

    // ================================================================
    // safe_container_tag
    // ================================================================

    #[test]
    fn safe_container_tag_falls_back_to_div_for_arg_taking_widgets() {
        assert_eq!(safe_container_tag(""), "Div");
        assert_eq!(safe_container_tag("Div"), "Div");
        assert_eq!(safe_container_tag("Span"), "Span");
        assert_eq!(safe_container_tag("H1"), "H1");
        // Interactive / arg-taking elements deliberately degrade to a container.
        assert_eq!(safe_container_tag("Button"), "Div");
        assert_eq!(safe_container_tag("Input"), "Div");
        assert_eq!(safe_container_tag("A"), "Div");
        assert_eq!(safe_container_tag("\u{1F600}"), "Div");
        assert_eq!(safe_container_tag(&"z".repeat(10_000)), "Div");
    }

    /// BUG (reported): `SAFE_CONTAINER_TAGS` is documented as holding the
    /// `NodeType`/`NodeTypeTag` **debug names**, and `safe_container_tag` compares
    /// against `format!("{:?}", tag_to_node_type(tag))`. But six entries are spelled
    /// with a different inner capitalization than the actual variant, so the
    /// comparison never matches and `<blockquote>`/`<figcaption>`/`<thead>`/`<tbody>`
    /// /`<tfoot>`/`<colgroup>` silently compile down to a plain `div`.
    #[test]
    fn safe_container_tag_matches_the_real_nodetype_debug_names() {
        for tag in [
            "blockquote",
            "figcaption",
            "thead",
            "tbody",
            "tfoot",
            "colgroup",
        ] {
            let dbg = format!("{:?}", tag_to_node_type(tag));
            assert_ne!(
                safe_container_tag(&dbg),
                "Div",
                "<{tag}> (NodeType debug name {dbg:?}) is a pure container and must keep \
                 its own creator instead of degrading to a div"
            );
        }
    }

    // ================================================================
    // fmt_f32_lit  (numeric)
    // ================================================================

    #[test]
    fn fmt_f32_lit_zero_and_negative() {
        assert_eq!(fmt_f32_lit(0.0), "0.0", "an integral value gains a decimal point");
        assert_eq!(fmt_f32_lit(-0.0), "-0.0");
        assert_eq!(fmt_f32_lit(-1.0), "-1.0");
        assert_eq!(fmt_f32_lit(1.5), "1.5");
        assert_eq!(fmt_f32_lit(-1.5), "-1.5");
    }

    #[test]
    fn fmt_f32_lit_min_max_stay_parseable_float_literals() {
        for f in [f32::MAX, f32::MIN, f32::MIN_POSITIVE, f32::EPSILON] {
            let s = fmt_f32_lit(f);
            assert_eq!(
                s.parse::<f32>(),
                Ok(f),
                "{f:e} must round-trip through its emitted literal ({s})"
            );
        }
    }

    #[test]
    fn fmt_f32_lit_nan_inf_produce_a_defined_result_and_do_not_panic() {
        // NOTE: these are NOT valid Rust/C float literals — a page with
        // `<progress value="NaN">` emits `create_progress_no_a11y(NaN, 1.0)`.
        // Pinned so a fix (e.g. clamping to 0.0) is a visible change.
        assert_eq!(fmt_f32_lit(f32::NAN), "NaN");
        assert_eq!(fmt_f32_lit(f32::INFINITY), "inf");
        assert_eq!(fmt_f32_lit(f32::NEG_INFINITY), "-inf");
    }

    // ================================================================
    // node_direct_text / node_aria_label / node_attr_or / node_attr_f32
    // first_caption_text
    // ================================================================

    #[test]
    fn node_direct_text_trims_and_skips_elements() {
        assert_eq!(node_direct_text(&XmlNode::default()), "");
        assert_eq!(node_direct_text(&node("p", &[], vec![txt("  Go  ")])), "Go");
        assert_eq!(
            node_direct_text(&node(
                "p",
                &[],
                vec![txt("a"), elem(node("b", &[], vec![txt("IGNORED")])), txt("b")]
            )),
            "a b",
            "direct text children are joined with a single space"
        );
        assert_eq!(
            node_direct_text(&node("p", &[], vec![txt("   "), txt("\t\n")])),
            "",
            "whitespace-only children are dropped"
        );
    }

    #[test]
    fn node_aria_label_ignores_empty_and_whitespace() {
        assert_eq!(node_aria_label(&XmlNode::default()), None);
        assert_eq!(node_aria_label(&node("b", &[("aria-label", "")], vec![])), None);
        assert_eq!(node_aria_label(&node("b", &[("aria-label", "   ")], vec![])), None);
        assert_eq!(
            node_aria_label(&node("b", &[("aria-label", "  Save  ")], vec![])),
            Some("Save".to_string())
        );
    }

    #[test]
    fn node_attr_or_returns_the_default_when_absent() {
        let n = node("a", &[("href", "/x"), ("empty", "")], vec![]);
        assert_eq!(node_attr_or(&n, "href", "FALLBACK"), "/x");
        assert_eq!(node_attr_or(&n, "missing", "FALLBACK"), "FALLBACK");
        assert_eq!(
            node_attr_or(&n, "empty", "FALLBACK"),
            "",
            "a present-but-empty attribute wins over the default"
        );
        assert_eq!(node_attr_or(&XmlNode::default(), "x", ""), "");
    }

    #[test]
    fn node_attr_f32_zero_negative_and_defaults() {
        let n = node(
            "meter",
            &[("zero", "0"), ("negzero", "-0"), ("neg", "-2.5"), ("pad", "  1.5  ")],
            vec![],
        );
        assert_eq!(node_attr_f32(&n, "zero", 9.0), 0.0);
        assert!(node_attr_f32(&n, "negzero", 9.0).is_sign_negative());
        assert_eq!(node_attr_f32(&n, "neg", 9.0), -2.5);
        assert_eq!(node_attr_f32(&n, "pad", 9.0), 1.5, "the value is trimmed first");
        assert_eq!(node_attr_f32(&n, "missing", 9.0), 9.0);
    }

    #[test]
    fn node_attr_f32_unparsable_falls_back_and_min_max_saturate() {
        let n = node(
            "meter",
            &[
                ("junk", "abc"),
                ("empty", ""),
                ("huge", "1e400"),
                ("tiny", "-1e400"),
                ("big", "340282350000000000000000000000000000000"),
            ],
            vec![],
        );
        assert_eq!(node_attr_f32(&n, "junk", 7.0), 7.0);
        assert_eq!(node_attr_f32(&n, "empty", 7.0), 7.0);
        assert!(
            node_attr_f32(&n, "huge", 7.0).is_infinite(),
            "an out-of-range literal saturates to inf, it does not panic"
        );
        assert_eq!(node_attr_f32(&n, "tiny", 7.0), f32::NEG_INFINITY);
        assert_eq!(node_attr_f32(&n, "big", 7.0), f32::MAX);
    }

    #[test]
    fn node_attr_f32_accepts_nan_and_inf_spellings() {
        // Rust's f32 FromStr accepts "NaN"/"inf", so hostile markup can inject a
        // non-finite value straight into codegen (see fmt_f32_lit above).
        let n = node("progress", &[("value", "NaN"), ("max", "inf")], vec![]);
        assert!(node_attr_f32(&n, "value", 0.0).is_nan());
        assert_eq!(node_attr_f32(&n, "max", 1.0), f32::INFINITY);
    }

    #[test]
    fn node_attr_f32_nan_default_is_returned_verbatim() {
        assert!(node_attr_f32(&XmlNode::default(), "x", f32::NAN).is_nan());
        assert_eq!(node_attr_f32(&XmlNode::default(), "x", f32::INFINITY), f32::INFINITY);
    }

    #[test]
    fn first_caption_text_edges() {
        assert_eq!(first_caption_text(&XmlNode::default()), None);
        assert_eq!(
            first_caption_text(&node("table", &[], vec![elem(node("caption", &[], vec![]))])),
            None,
            "an empty caption yields None"
        );
        assert_eq!(
            first_caption_text(&node(
                "table",
                &[],
                vec![elem(node("CAPTION", &[], vec![txt("  Hi  ")]))]
            )),
            Some("Hi".to_string()),
            "the tag match is ASCII-case-insensitive and the text is trimmed"
        );
    }

    // ================================================================
    // analyze_node_ctor / CtorArg / NodeCtor
    // ================================================================

    #[test]
    fn analyze_node_ctor_plain_for_unknown_and_empty_tags() {
        assert!(matches!(analyze_node_ctor("div", &XmlNode::default()), NodeCtor::Plain));
        assert!(matches!(analyze_node_ctor("", &XmlNode::default()), NodeCtor::Plain));
        assert!(matches!(
            analyze_node_ctor("\u{1F600}", &XmlNode::default()),
            NodeCtor::Plain
        ));
        let plain = analyze_node_ctor("div", &XmlNode::default());
        assert_eq!(plain.render_rust(), None);
        assert_eq!(plain.render_c(), None);
        assert_eq!(plain.render_fluent(&CompileTarget::Cpp), None);
        assert!(!plain.consumes_text());
        assert!(!plain.skip_caption());
    }

    #[test]
    fn analyze_node_ctor_with_text_tier_requires_actual_text() {
        // Empty <p> stays a plain container (has_only_text_children() is vacuously
        // true for a childless node, so `has_text` is the real gate).
        assert!(matches!(analyze_node_ctor("p", &XmlNode::default()), NodeCtor::Plain));
        // <p> with an element child is not "pure text" either.
        let mixed = node("p", &[], vec![txt("a"), elem(XmlNode::create("span"))]);
        assert!(matches!(analyze_node_ctor("p", &mixed), NodeCtor::Plain));

        let pure = node("p", &[], vec![txt("  Hello  ")]);
        let ctor = analyze_node_ctor("p", &pure);
        assert!(ctor.consumes_text(), "the text is folded into the constructor");
        assert_eq!(
            ctor.render_rust().as_deref(),
            Some("Dom::create_p_with_text(AzString::from(\"Hello\"))")
        );
        assert_eq!(
            ctor.render_c().as_deref(),
            Some("AzDom_createPWithText(AZ_STR(\"Hello\"))")
        );
        assert_eq!(
            ctor.render_fluent(&CompileTarget::Python).as_deref(),
            Some("azul.Dom.create_p_with_text(\"Hello\")")
        );
    }

    #[test]
    fn analyze_node_ctor_button_with_and_without_aria() {
        let plain_btn = node("button", &[], vec![txt("Go")]);
        assert_eq!(
            analyze_node_ctor("button", &plain_btn).render_rust().as_deref(),
            Some("Dom::create_button_no_a11y(AzString::from(\"Go\"))")
        );

        let aria_btn = node("button", &[("aria-label", "Save")], vec![txt("Go")]);
        assert_eq!(
            analyze_node_ctor("button", &aria_btn).render_rust().as_deref(),
            Some(
                "Dom::create_button(AzString::from(\"Go\"), \
                 SmallAriaInfo::label(AzString::from(\"Save\")))"
            )
        );
    }

    #[test]
    fn analyze_node_ctor_escapes_quotes_and_backslashes_in_text() {
        let btn = node("button", &[], vec![txt("say \"hi\"\\now")]);
        let rust = analyze_node_ctor("button", &btn).render_rust().expect("semantic");
        assert!(
            rust.contains("say \\\"hi\\\"\\\\now"),
            "quotes and backslashes must be escaped for the literal, got {rust}"
        );
    }

    #[test]
    fn analyze_node_ctor_anchor_uses_option_string_when_it_has_no_text() {
        let bare = node("a", &[], vec![]);
        assert_eq!(
            analyze_node_ctor("a", &bare).render_rust().as_deref(),
            Some("Dom::create_a_no_a11y(AzString::from(\"\"), OptionString::None)"),
            "a missing href defaults to an empty string, missing text to OptionString::None"
        );

        let full = node("a", &[("href", "/x")], vec![txt("Home")]);
        assert_eq!(
            analyze_node_ctor("a", &full).render_rust().as_deref(),
            Some(
                "Dom::create_a_no_a11y(AzString::from(\"/x\"), \
                 OptionString::Some(AzString::from(\"Home\")))"
            )
        );
        assert_eq!(
            analyze_node_ctor("a", &full).render_c().as_deref(),
            Some("AzDom_createANoA11y(AZ_STR(\"/x\"), AzOptionString_some(AZ_STR(\"Home\")))")
        );
    }

    #[test]
    fn analyze_node_ctor_table_aria_form_skips_the_literal_caption() {
        let t = node(
            "table",
            &[("aria-label", "Prices")],
            vec![elem(node("caption", &[], vec![txt("Q1")]))],
        );
        let ctor = analyze_node_ctor("table", &t);
        assert!(ctor.skip_caption(), "the aria form injects its own caption child");
        assert_eq!(
            ctor.render_rust().as_deref(),
            Some(
                "Dom::create_table(AzString::from(\"Q1\"), \
                 SmallAriaInfo::label(AzString::from(\"Prices\")))"
            )
        );

        let plain = analyze_node_ctor("table", &node("table", &[], vec![]));
        assert!(!plain.skip_caption());
        assert_eq!(plain.render_rust().as_deref(), Some("Dom::create_table_no_a11y()"));
    }

    #[test]
    fn analyze_node_ctor_scalar_widgets_use_defaults_and_emit_float_literals() {
        let p = node("progress", &[], vec![]);
        assert_eq!(
            analyze_node_ctor("progress", &p).render_rust().as_deref(),
            Some("Dom::create_progress_no_a11y(0.0, 1.0)"),
            "missing value/max fall back to 0.0 / 1.0 as float literals"
        );

        let m = node("meter", &[("value", "5"), ("min", "-1"), ("max", "10")], vec![]);
        assert_eq!(
            analyze_node_ctor("meter", &m).render_c().as_deref(),
            Some("AzDom_createMeterNoA11y(5.0f, -1.0f, 10.0f)")
        );
    }

    /// Non-finite attribute values flow straight into the emitted literal. Pinned
    /// so that a fix (clamping / rejecting them) shows up as a change.
    #[test]
    fn analyze_node_ctor_non_finite_attributes_emit_non_finite_literals() {
        let p = node("progress", &[("value", "NaN"), ("max", "inf")], vec![]);
        let rust = analyze_node_ctor("progress", &p).render_rust().expect("semantic");
        assert_eq!(rust, "Dom::create_progress_no_a11y(NaN, inf)");
    }

    #[test]
    fn ctor_arg_render_targets_are_distinct() {
        let s = CtorArg::Str("a\"b".to_string());
        assert_eq!(s.render_rust(), "AzString::from(\"a\\\"b\")");
        assert_eq!(s.render_c(), "AZ_STR(\"a\\\"b\")");
        assert_eq!(s.render_cpp(), "String(\"a\\\"b\")");
        assert_eq!(s.render_python(), "\"a\\\"b\"");

        assert_eq!(CtorArg::OptNone.render_rust(), "OptionString::None");
        assert_eq!(CtorArg::OptNone.render_c(), "AzOptionString_none()");
        assert_eq!(CtorArg::OptNone.render_cpp(), "OptionString::none()");
        assert_eq!(CtorArg::OptNone.render_python(), "azul.OptionString.none()");

        assert_eq!(CtorArg::Float(0.0).render_rust(), "0.0");
        assert_eq!(CtorArg::Float(0.0).render_c(), "0.0f");
        assert_eq!(CtorArg::Float(f32::NAN).render_c(), "NaNf");
    }

    #[test]
    fn node_ctor_render_fluent_returns_none_for_non_fluent_targets() {
        let ctor = analyze_node_ctor("p", &node("p", &[], vec![txt("x")]));
        assert!(ctor.render_fluent(&CompileTarget::Rust).is_none());
        assert!(ctor.render_fluent(&CompileTarget::C).is_none());
        assert!(ctor.render_fluent(&CompileTarget::Cpp).is_some());
        assert!(ctor.render_fluent(&CompileTarget::Python).is_some());
    }

    // ================================================================
    // format_component_args / compile_component / compile_components
    // ================================================================

    #[test]
    fn format_component_args_empty_and_ordering() {
        assert_eq!(format_component_args(&no_args()), "");
        // Args are sorted DESCENDING by their rendered "name: type" string.
        assert_eq!(
            format_component_args(&args(&[("a", "u32"), ("b", "String")])),
            "b: String, a: u32"
        );
    }

    #[test]
    fn format_component_args_edge_values_no_panic() {
        let a = args(&[("", ""), ("\u{1F600}", "\u{130}")]);
        let out = format_component_args(&a);
        assert!(out.contains(": "), "still emits `name: type` pairs, got {out:?}");
    }

    #[test]
    fn compile_component_emits_a_render_fn() {
        let ca = ComponentArguments {
            args: args(&[("count", "u32")]),
            accepts_text: false,
        };
        let out = compile_component("MyWidget", &ca, "Dom::create_div()");
        assert!(out.contains("pub fn render(count: u32) -> Dom {"), "got:\n{out}");
        assert!(out.contains("#[inline]"), "a one-line body is inlined");
    }

    #[test]
    fn compile_component_accepts_text_prepends_the_text_param() {
        let ca = ComponentArguments {
            args: args(&[("count", "u32")]),
            accepts_text: true,
        };
        let out = compile_component("my-widget", &ca, "Dom::create_div()");
        assert!(
            out.contains("pub fn render(text: AzString, count: u32) -> Dom {"),
            "got:\n{out}"
        );

        let ca_no_args = ComponentArguments {
            args: no_args(),
            accepts_text: true,
        };
        let out = compile_component("w", &ca_no_args, "Dom::create_div()");
        assert!(
            out.contains("pub fn render(text: AzString) -> Dom {"),
            "no trailing comma when there are no extra args, got:\n{out}"
        );
    }

    #[test]
    fn compile_component_empty_name_and_body_no_panic() {
        let ca = ComponentArguments::default();
        let out = compile_component("", &ca, "");
        assert!(out.contains("pub fn render() -> Dom {"), "got:\n{out}");
    }

    #[test]
    fn compile_components_of_an_empty_list_is_empty() {
        assert_eq!(compile_components(Vec::new()), "");
    }

    // ================================================================
    // parse_svg_float / parse_svg_points  (parser / numeric)
    // ================================================================

    #[test]
    fn parse_svg_float_none_empty_whitespace_garbage() {
        assert_eq!(parse_svg_float(None), None);
        let empty = AzString::from("");
        assert_eq!(parse_svg_float(Some(&empty)), None);
        let ws = AzString::from("   \t\n");
        assert_eq!(parse_svg_float(Some(&ws)), None);
        let junk = AzString::from("10px");
        assert_eq!(parse_svg_float(Some(&junk)), None, "units are not stripped");
        let uni = AzString::from("\u{1F600}");
        assert_eq!(parse_svg_float(Some(&uni)), None);
    }

    #[test]
    fn parse_svg_float_valid_and_boundary_numbers() {
        let padded = AzString::from("  1.5  ");
        assert_eq!(parse_svg_float(Some(&padded)), Some(1.5), "value is trimmed");
        let zero = AzString::from("0");
        assert_eq!(parse_svg_float(Some(&zero)), Some(0.0));
        let negzero = AzString::from("-0");
        assert!(parse_svg_float(Some(&negzero)).unwrap().is_sign_negative());
        let huge = AzString::from("1e400");
        assert!(
            parse_svg_float(Some(&huge)).unwrap().is_infinite(),
            "overflow saturates to inf rather than erroring"
        );
        let nan = AzString::from("NaN");
        assert!(parse_svg_float(Some(&nan)).unwrap().is_nan());
        let inf = AzString::from("-inf");
        assert_eq!(parse_svg_float(Some(&inf)), Some(f32::NEG_INFINITY));
    }

    #[test]
    fn parse_svg_points_rejects_degenerate_input() {
        assert!(parse_svg_points("", false).is_none());
        assert!(parse_svg_points("   ", false).is_none());
        assert!(parse_svg_points("garbage", false).is_none());
        assert!(parse_svg_points("1 2", false).is_none(), "a single point is not a line");
        assert!(
            parse_svg_points("1 2 3", false).is_none(),
            "an odd coordinate count is rejected"
        );
        assert!(parse_svg_points("\u{1F600}", false).is_none());
    }

    #[test]
    fn parse_svg_points_valid_minimal_and_close() {
        let open = parse_svg_points("0,0 10,0", false).expect("two points => one line");
        assert_eq!(open.rings.as_ref().len(), 1);
        assert_eq!(open.rings.as_ref()[0].items.as_ref().len(), 1);

        // `close` adds a segment back to the first point when it differs.
        let closed = parse_svg_points("0,0 10,0 10,10", true).expect("triangle");
        assert_eq!(
            closed.rings.as_ref()[0].items.as_ref().len(),
            3,
            "2 segments + 1 closing segment"
        );

        // Already-closed rings do not get a duplicate closing segment.
        let already = parse_svg_points("0,0 10,0 0,0", true).expect("closed ring");
        assert_eq!(already.rings.as_ref()[0].items.as_ref().len(), 2);
    }

    #[test]
    fn parse_svg_points_skips_unparsable_tokens_and_handles_boundaries() {
        // Unparsable coordinates are silently dropped, which can shift the pairing.
        let p = parse_svg_points("0,0 junk 10,0", false).expect("junk token dropped");
        assert_eq!(p.rings.as_ref()[0].items.as_ref().len(), 1);

        let nan = parse_svg_points("NaN,0 1,1", false).expect("NaN is a parseable f32");
        assert_eq!(nan.rings.as_ref()[0].items.as_ref().len(), 1);
    }

    #[test]
    fn parse_svg_points_extremely_long_terminates() {
        let pts = "1,2 ".repeat(20_000);
        let p = parse_svg_points(&pts, false).expect("20k points");
        assert_eq!(p.rings.as_ref()[0].items.as_ref().len(), 19_999);
    }

    // ================================================================
    // CompactDomBuilder  (constructor / numeric)
    // ================================================================

    #[test]
    fn compact_dom_builder_new_and_with_capacity_start_empty() {
        for b in [
            CompactDomBuilder::new(),
            CompactDomBuilder::with_capacity(0),
            CompactDomBuilder::with_capacity(1),
            CompactDomBuilder::with_capacity(4096),
        ] {
            let fd = b.finish();
            assert_eq!(fd.node_hierarchy.as_ref().len(), 0);
            assert_eq!(fd.node_data.as_ref().len(), 0);
            assert_eq!(fd.css.as_ref().len(), 0);
        }
        assert_eq!(
            CompactDomBuilder::default().finish().node_data.as_ref().len(),
            0
        );
    }

    #[test]
    fn compact_dom_builder_close_node_on_an_empty_stack_is_a_no_op() {
        let mut b = CompactDomBuilder::new();
        b.close_node();
        b.close_node();
        assert_eq!(b.finish().node_hierarchy.as_ref().len(), 0, "no panic, no nodes");
    }

    #[test]
    fn compact_dom_builder_keeps_hierarchy_and_data_parallel() {
        let mut b = CompactDomBuilder::new();
        b.open_node(NodeData::create_node(NodeType::Html));
        b.add_leaf(NodeData::create_text("a"));
        b.add_leaf(NodeData::create_text("b"));
        b.close_node();
        let fd = b.finish();
        assert_eq!(fd.node_data.as_ref().len(), 3);
        assert_eq!(
            fd.node_hierarchy.as_ref().len(),
            fd.node_data.as_ref().len(),
            "the two arenas must stay the same length"
        );
    }

    #[test]
    fn compact_dom_builder_unclosed_node_still_finishes() {
        let mut b = CompactDomBuilder::new();
        b.open_node(NodeData::create_node(NodeType::Div));
        // Deliberately NOT closed.
        let fd = b.finish();
        assert_eq!(fd.node_hierarchy.as_ref().len(), 1);
        assert_eq!(
            fd.node_hierarchy.as_ref()[0].last_child, 0,
            "last_child stays unset when close_node() is never called"
        );
    }

    #[test]
    fn compact_dom_builder_add_css_accepts_zero_and_usize_max_node_ids() {
        let mut b = CompactDomBuilder::new();
        b.add_css(0, Css::empty());
        b.add_css(usize::MAX, Css::empty());
        let fd = b.finish();
        assert_eq!(fd.css.as_ref().len(), 2);
        assert_eq!(fd.css.as_ref()[0].node_id, 0);
        assert_eq!(
            fd.css.as_ref()[1].node_id,
            usize::MAX,
            "an out-of-range node id is stored verbatim (no bounds check, no panic)"
        );
    }

    // ================================================================
    // xml_node_to_dom_fast / xml_node_to_fast_dom  (numeric: depth)
    // ================================================================

    #[test]
    fn xml_node_to_dom_fast_depth_zero_builds_children() {
        let map = ComponentMap::default();
        let n = node("div", &[], vec![txt("hi"), elem(XmlNode::create("span"))]);
        let dom = xml_node_to_dom_fast(&n, &map, false, 0).expect("ok");
        assert_eq!(dom.children.as_ref().len(), 2);
    }

    #[test]
    fn xml_node_to_dom_fast_at_and_past_the_depth_cap_truncates_instead_of_panicking() {
        let map = ComponentMap::default();
        let n = node("div", &[], vec![txt("hi")]);

        let at_cap = xml_node_to_dom_fast(&n, &map, false, MAX_XML_NESTING_DEPTH).expect("ok");
        assert!(
            at_cap.children.as_ref().is_empty(),
            "at the cap the node is emitted without children"
        );

        let saturated = xml_node_to_dom_fast(&n, &map, false, usize::MAX)
            .expect("usize::MAX depth must not overflow when computing depth + 1");
        assert!(saturated.children.as_ref().is_empty());

        let below = xml_node_to_dom_fast(&n, &map, false, MAX_XML_NESTING_DEPTH - 1).expect("ok");
        assert_eq!(below.children.as_ref().len(), 1, "one below the cap still recurses");
    }

    #[test]
    fn xml_node_to_fast_dom_at_the_depth_cap_still_emits_the_node() {
        let map = ComponentMap::default();
        let n = node("div", &[], vec![txt("hi")]);

        let mut b = CompactDomBuilder::new();
        xml_node_to_fast_dom(&n, &map, false, &mut b, usize::MAX).expect("no overflow");
        let fd = b.finish();
        assert_eq!(
            fd.node_data.as_ref().len(),
            1,
            "the node itself is still opened+closed, only its children are dropped"
        );

        let mut b2 = CompactDomBuilder::new();
        xml_node_to_fast_dom(&n, &map, false, &mut b2, 0).expect("ok");
        assert_eq!(b2.finish().node_data.as_ref().len(), 2, "node + text child");
    }

    #[test]
    fn apply_xml_node_attributes_extreme_tabindex_does_not_panic() {
        let map = ComponentMap::default();
        for v in [
            "0",
            "-1",
            "2147483647",
            "9223372036854775807",
            "-9223372036854775808",
            "99999999999999999999999999999999",
            "abc",
            "",
            "\u{1F600}",
        ] {
            let n = node("div", &[("tabindex", v), ("focusable", "true")], vec![]);
            assert!(
                xml_node_to_dom_fast(&n, &map, false, 0).is_ok(),
                "tabindex={v:?} must not panic"
            );
        }
    }

    #[test]
    fn apply_xml_node_attributes_img_width_height_garbage_falls_back_to_zero() {
        let map = ComponentMap::default();
        let n = node(
            "img",
            &[("src", "a.png"), ("width", "-5"), ("height", "not-a-number")],
            vec![],
        );
        let dom = xml_node_to_dom_fast(&n, &map, false, 0).expect("ok");
        match dom.root.get_node_type() {
            NodeType::Image(_) => {}
            other => panic!("expected an Image node, got {other:?}"),
        }
    }

    // ================================================================
    // set_stringified_attributes  (numeric: tabs / tabindex)
    // ================================================================

    #[test]
    fn set_stringified_attributes_zero_tabs_and_empty_attrs() {
        let mut s = String::new();
        set_stringified_attributes(&mut s, &attrs(&[]), &no_args(), 0);
        assert_eq!(s, "", "nothing to emit for an attribute-less node");
    }

    #[test]
    fn set_stringified_attributes_splits_ids_and_classes_on_whitespace() {
        let mut s = String::new();
        set_stringified_attributes(
            &mut s,
            &attrs(&[("id", "a  b"), ("class", "c\td")]),
            &no_args(),
            0,
        );
        assert!(s.contains(".with_id(\"a\")"), "got {s:?}");
        assert!(s.contains(".with_id(\"b\")"));
        assert!(s.contains(".with_class(\"c\")"));
        assert!(s.contains(".with_class(\"d\")"));
    }

    #[test]
    fn set_stringified_attributes_tabindex_boundaries() {
        let cases: &[(&str, &str)] = &[
            ("0", "TabIndex::Auto"),
            ("5", "TabIndex::OverrideInParent(5)"),
            ("-1", "TabIndex::NoKeyboardFocus"),
        ];
        for (val, expected) in cases {
            let mut s = String::new();
            set_stringified_attributes(&mut s, &attrs(&[("tabindex", val)]), &no_args(), 0);
            assert!(s.contains(expected), "tabindex={val:?} => {s:?}");
        }

        // Unparsable / overflowing values emit nothing rather than panicking.
        for val in ["abc", "", "99999999999999999999999999999999", "1.5"] {
            let mut s = String::new();
            set_stringified_attributes(&mut s, &attrs(&[("tabindex", val)]), &no_args(), 0);
            assert!(
                !s.contains("TabIndex"),
                "tabindex={val:?} must be ignored, got {s:?}"
            );
        }
    }

    #[test]
    fn set_stringified_attributes_focusable_only_accepts_exact_true_false() {
        let mut s = String::new();
        set_stringified_attributes(&mut s, &attrs(&[("focusable", "true")]), &no_args(), 0);
        assert!(s.contains("TabIndex::Auto"));

        let mut s = String::new();
        set_stringified_attributes(&mut s, &attrs(&[("focusable", "false")]), &no_args(), 0);
        assert!(s.contains("TabIndex::NoKeyboardFocus"));

        let mut s = String::new();
        set_stringified_attributes(&mut s, &attrs(&[("focusable", "TRUE")]), &no_args(), 0);
        assert!(s.is_empty(), "casing other than `true`/`false` is ignored, got {s:?}");
    }

    #[test]
    fn set_stringified_attributes_large_tab_depth_does_not_overflow() {
        // `tabs` becomes `"    ".repeat(tabs)`; a large-but-sane nesting depth must
        // stay linear and allocate without panicking.
        let mut s = String::new();
        set_stringified_attributes(&mut s, &attrs(&[("id", "x")]), &no_args(), 1_000);
        assert!(s.contains(".with_id(\"x\")"));
        assert!(s.len() > 4_000, "the 1000-level indent is actually emitted");
    }

    // ================================================================
    // group_matches / CssMatcher  (numeric: indices)
    // ================================================================

    fn refs(v: &[CssPathSelector]) -> Vec<&CssPathSelector> {
        v.iter().collect()
    }

    #[test]
    fn group_matches_global_matches_at_any_index() {
        let a = vec![CssPathSelector::Global];
        assert!(group_matches(&refs(&a), &[], 0, 0));
        assert!(
            group_matches(&refs(&a), &[], usize::MAX, usize::MAX),
            "usize::MAX indices must not overflow"
        );
    }

    #[test]
    fn group_matches_type_class_id() {
        let div = vec![CssPathSelector::Type(NodeTypeTag::Div)];
        let p = vec![CssPathSelector::Type(NodeTypeTag::P)];
        assert!(group_matches(&refs(&div), &refs(&div), 0, 1));
        assert!(!group_matches(&refs(&div), &refs(&p), 0, 1));
        assert!(!group_matches(&refs(&div), &[], 0, 1), "an empty haystack never matches");

        let cls = vec![CssPathSelector::Class(AzString::from("x"))];
        assert!(group_matches(&refs(&cls), &refs(&cls), 0, 1));
        let id = vec![CssPathSelector::Id(AzString::from("x"))];
        assert!(!group_matches(&refs(&id), &refs(&cls), 0, 1), "an id is not a class");
    }

    #[test]
    fn group_matches_first_and_last_pseudo_at_boundaries() {
        let first = vec![CssPathSelector::PseudoSelector(CssPathPseudoSelector::First)];
        assert!(group_matches(&refs(&first), &[], 0, 10));
        assert!(!group_matches(&refs(&first), &[], 1, 10));

        let last = vec![CssPathSelector::PseudoSelector(CssPathPseudoSelector::Last)];
        assert!(group_matches(&refs(&last), &[], 9, 10));
        assert!(!group_matches(&refs(&last), &[], 8, 10));
        assert!(
            group_matches(&refs(&last), &[], 0, 0),
            "parent_children == 0 saturates to 0, so index 0 counts as last"
        );
    }

    #[test]
    fn group_matches_nth_child_even_odd_and_number() {
        let even = vec![CssPathSelector::PseudoSelector(CssPathPseudoSelector::NthChild(
            CssNthChildSelector::Even,
        ))];
        assert!(group_matches(&refs(&even), &[], 0, 0));
        assert!(!group_matches(&refs(&even), &[], 1, 0));
        assert!(
            !group_matches(&refs(&even), &[], usize::MAX, 0),
            "usize::MAX is odd"
        );

        let odd = vec![CssPathSelector::PseudoSelector(CssPathPseudoSelector::NthChild(
            CssNthChildSelector::Odd,
        ))];
        assert!(group_matches(&refs(&odd), &[], 1, 0));
        assert!(!group_matches(&refs(&odd), &[], 2, 0));

        let n = vec![CssPathSelector::PseudoSelector(CssPathPseudoSelector::NthChild(
            CssNthChildSelector::Number(u32::MAX),
        ))];
        assert!(group_matches(&refs(&n), &[], u32::MAX as usize, 0));
        assert!(!group_matches(&refs(&n), &[], 0, 0));
    }

    #[test]
    fn group_matches_nth_child_pattern_zero_repeat_does_not_divide_by_zero() {
        let zero = vec![CssPathSelector::PseudoSelector(CssPathPseudoSelector::NthChild(
            CssNthChildSelector::Pattern(CssNthChildPattern {
                pattern_repeat: 0,
                offset: 0,
            }),
        ))];
        // `is_multiple_of(0)` is `self == 0` — no division by zero.
        assert!(group_matches(&refs(&zero), &[], 0, 0));
        assert!(!group_matches(&refs(&zero), &[], 5, 0));

        let offset_past = vec![CssPathSelector::PseudoSelector(CssPathPseudoSelector::NthChild(
            CssNthChildSelector::Pattern(CssNthChildPattern {
                pattern_repeat: 2,
                offset: u32::MAX,
            }),
        ))];
        assert!(
            group_matches(&refs(&offset_past), &[], 0, 0),
            "index - offset saturates to 0 rather than underflowing"
        );
    }

    #[test]
    fn group_matches_structural_combinators_never_match() {
        for sel in [
            CssPathSelector::Children,
            CssPathSelector::DirectChildren,
            CssPathSelector::AdjacentSibling,
            CssPathSelector::GeneralSibling,
        ] {
            let a = vec![sel.clone()];
            assert!(
                !group_matches(&refs(&a), &refs(&a), 0, 1),
                "{sel:?} is a combinator, not a matchable group member"
            );
        }
    }

    #[test]
    fn css_matcher_empty_path_never_matches() {
        let m = CssMatcher {
            path: Vec::new(),
            indices_in_parent: vec![0],
            children_length: vec![0],
        };
        let path = CssPath {
            selectors: vec![CssPathSelector::Type(NodeTypeTag::Body)].into(),
        };
        assert!(!m.matches(&path), "an empty matcher path can never match");

        let m2 = CssMatcher {
            path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
            indices_in_parent: vec![0],
            children_length: vec![0],
        };
        let empty_path = CssPath {
            selectors: Vec::new().into(),
        };
        assert!(!m2.matches(&empty_path), "an empty CSS path can never match");
    }

    #[test]
    fn css_matcher_get_hash_is_deterministic_and_path_sensitive() {
        let a = CssMatcher {
            path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
            indices_in_parent: vec![0],
            children_length: vec![0],
        };
        let b = CssMatcher {
            path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
            indices_in_parent: vec![9],
            children_length: vec![9],
        };
        let c = CssMatcher {
            path: vec![CssPathSelector::Type(NodeTypeTag::Div)],
            indices_in_parent: vec![0],
            children_length: vec![0],
        };
        assert_eq!(a.get_hash(), a.get_hash(), "stable across calls");
        assert_eq!(
            a.get_hash(),
            b.get_hash(),
            "the hash covers only `path`, not the sibling indices"
        );
        assert_ne!(a.get_hash(), c.get_hash());

        let empty = CssMatcher {
            path: Vec::new(),
            indices_in_parent: Vec::new(),
            children_length: Vec::new(),
        };
        let _ = empty.get_hash(); // must not panic
    }

    #[test]
    fn css_matcher_mismatched_bookkeeping_vec_lengths_bail_out() {
        // `indices_in_parent` / `children_length` must be as long as the group list.
        let m = CssMatcher {
            path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
            indices_in_parent: Vec::new(),
            children_length: Vec::new(),
        };
        let path = CssPath {
            selectors: vec![CssPathSelector::Type(NodeTypeTag::Body)].into(),
        };
        assert!(
            !m.matches(&path),
            "a desynced matcher must return false, not index out of bounds"
        );
    }

    #[test]
    fn get_css_blocks_and_inline_string_on_empty_css() {
        let m = CssMatcher {
            path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
            indices_in_parent: vec![0],
            children_length: vec![0],
        };
        assert!(get_css_blocks(&Css::empty(), &m).is_empty());
        assert_eq!(css_blocks_to_inline_string(&[]), "");
    }

    // ================================================================
    // str_to_dom / str_to_dom_unstyled / parse_page_style_and_body / body_matcher
    // ================================================================

    #[test]
    fn str_to_dom_rejects_documents_without_html_or_body() {
        let map = ComponentMap::with_builtin();
        assert_eq!(
            str_to_dom(&[], &map, None).unwrap_err(),
            DomXmlParseError::NoHtmlNode
        );
        let html_only = vec![elem(XmlNode::create("html"))];
        assert_eq!(
            str_to_dom(&html_only, &map, None).unwrap_err(),
            DomXmlParseError::NoBodyInHtml
        );
        assert!(str_to_dom_unstyled(&[], &map).is_err());
    }

    #[test]
    fn str_to_dom_valid_minimal() {
        let map = ComponentMap::with_builtin();
        let d = doc("body { color: red; }", vec![elem(node("div", &[("id", "x")], vec![]))]);
        assert!(str_to_dom(&d, &map, None).is_ok());
        assert!(str_to_dom_unstyled(&d, &map).is_ok());
    }

    #[test]
    fn str_to_dom_max_width_edge_values_do_not_panic() {
        let map = ComponentMap::with_builtin();
        let d = doc("", vec![elem(XmlNode::create("div"))]);
        for w in [
            Some(0.0f32),
            Some(-0.0),
            Some(-1.0),
            Some(f32::MAX),
            Some(f32::MIN),
            Some(f32::NAN),
            Some(f32::INFINITY),
            Some(f32::NEG_INFINITY),
            None,
        ] {
            assert!(
                str_to_dom(&d, &map, w).is_ok(),
                "max_width={w:?} is formatted straight into a CSS string and must not panic"
            );
        }
    }

    #[test]
    fn str_to_dom_deeply_nested_body_is_depth_capped_not_stack_overflowing() {
        let map = ComponentMap::with_builtin();
        let deep = wrap_divs(2_000, node("div", &[("id", "bottom")], vec![]));
        let d = doc("", vec![elem(deep)]);
        assert!(
            str_to_dom(&d, &map, None).is_ok(),
            "children past MAX_XML_NESTING_DEPTH are dropped, not crashed on"
        );
    }

    #[test]
    fn parse_page_style_and_body_and_body_matcher() {
        let d = doc("body { color: red; }", vec![elem(XmlNode::create("div"))]);
        let (css, body) = parse_page_style_and_body(&d).expect("well-formed page");
        assert_eq!(body.node_type.as_str(), "body");
        assert!(!css.rules.as_ref().is_empty(), "the <style> block is parsed");

        let m = body_matcher(body);
        assert!(m.path.is_empty(), "the matcher starts with an empty path");
        assert_eq!(m.indices_in_parent, vec![0]);
        assert_eq!(m.children_length, vec![body.children.as_ref().len()]);
    }

    #[test]
    fn parse_page_style_and_body_with_no_style_block() {
        let head = node("head", &[], vec![]);
        let body = node("body", &[], vec![]);
        let d = vec![elem(node("html", &[], vec![elem(head), elem(body)]))];
        let (css, body) = parse_page_style_and_body(&d).expect("ok");
        assert!(css.rules.as_ref().is_empty());
        assert_eq!(body.children.as_ref().len(), 0);
    }

    // ================================================================
    // str_to_rust_code / str_to_c_code / str_to_cpp_code / str_to_python_code
    // ================================================================

    #[test]
    fn str_to_rust_code_empty_input_is_an_error_not_a_panic() {
        let map = ComponentMap::with_builtin();
        assert!(matches!(
            str_to_rust_code(&[], "", &map),
            Err(CompileError::Xml(DomXmlParseError::NoHtmlNode))
        ));
        assert!(str_to_c_code(&[], &map).is_err());
        assert!(str_to_cpp_code(&[], &map).is_err());
        assert!(str_to_python_code(&[], &map).is_err());
    }

    #[test]
    fn str_to_rust_code_whitespace_and_text_only_roots_are_errors() {
        let map = ComponentMap::with_builtin();
        for roots in [vec![txt("   ")], vec![txt("\t\n")], vec![txt("garbage")]] {
            assert!(
                str_to_rust_code(&roots, "", &map).is_err(),
                "a document with no <html> element must be rejected"
            );
        }
    }

    #[test]
    fn str_to_rust_code_valid_minimal() {
        let map = ComponentMap::with_builtin();
        let d = doc("", vec![elem(node("p", &[], vec![txt("Hi")]))]);
        let src = str_to_rust_code(&d, "// imports", &map).expect("compiles");
        assert!(src.contains("Dom::create_body()"), "got:\n{src}");
        assert!(src.contains("Dom::create_p_with_text(AzString::from(\"Hi\"))"));
        assert!(src.contains("// imports"), "the imports blob is spliced in");
        assert!(src.contains("fn main()"));
    }

    #[test]
    fn str_to_c_cpp_python_code_valid_minimal() {
        let map = ComponentMap::with_builtin();
        let d = doc("", vec![elem(node("p", &[], vec![txt("Hi")]))]);

        let c = str_to_c_code(&d, &map).expect("compiles");
        assert!(c.contains("#include \"azul.h\""), "got:\n{c}");
        assert!(c.contains("AzDom n0 = AzDom_createBody();"));
        assert!(c.contains("AzDom_createPWithText(AZ_STR(\"Hi\"))"));

        let cpp = str_to_cpp_code(&d, &map).expect("compiles");
        assert!(cpp.contains("#include \"azul20.hpp\""), "got:\n{cpp}");
        assert!(cpp.contains("Dom::create_p_with_text(String(\"Hi\"))"));

        let py = str_to_python_code(&d, &map).expect("compiles");
        assert!(py.contains("import azul"), "got:\n{py}");
        assert!(py.contains("azul.Dom.create_p_with_text(\"Hi\")"));
    }

    #[test]
    fn compile_targets_escape_quotes_in_text_content() {
        let map = ComponentMap::with_builtin();
        let d = doc("", vec![elem(node("div", &[], vec![txt("say \"hi\"")]))]);

        let rust = str_to_rust_code(&d, "", &map).expect("compiles");
        assert!(rust.contains("say \\\"hi\\\""), "got:\n{rust}");
        let c = str_to_c_code(&d, &map).expect("compiles");
        assert!(c.contains("say \\\"hi\\\""), "got:\n{c}");
    }

    #[test]
    fn compile_body_node_to_rust_code_on_an_empty_body() {
        let map = ComponentMap::with_builtin();
        let body = node("body", &[], vec![]);
        let mut extra = VecContents::default();
        let mut blocks = BTreeMap::new();
        let out = compile_body_node_to_rust_code(
            &body,
            &map,
            &mut extra,
            &mut blocks,
            &Css::empty(),
            body_matcher(&body),
        )
        .expect("ok");
        assert_eq!(out, "Dom::create_body()", "no children => no .with_children()");
    }

    #[test]
    fn compile_body_node_to_rust_code_skips_whitespace_only_text_children() {
        let map = ComponentMap::with_builtin();
        let body = node("body", &[], vec![txt("   \n\t ")]);
        let mut extra = VecContents::default();
        let mut blocks = BTreeMap::new();
        let out = compile_body_node_to_rust_code(
            &body,
            &map,
            &mut extra,
            &mut blocks,
            &Css::empty(),
            body_matcher(&body),
        )
        .expect("ok");
        assert!(
            !out.contains("create_text"),
            "a whitespace-only text child emits nothing, got:\n{out}"
        );
    }

    // ================================================================
    // builtin_render_fn / builtin_compile_fn  (numeric: indent)
    // ================================================================

    #[test]
    fn builtin_render_fn_for_a_text_and_a_textless_element() {
        let map = ComponentMap::with_builtin();
        let div = map.get_unqualified("div").expect("builtin div");
        assert!(matches!(
            builtin_render_fn(div, &div.data_model, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));

        let p = map.get_unqualified("p").expect("builtin p");
        assert!(matches!(
            builtin_render_fn(p, &p.data_model, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));
    }

    #[test]
    fn builtin_compile_fn_ignores_indent_so_usize_max_is_safe() {
        let map = ComponentMap::with_builtin();
        let div = map.get_unqualified("div").expect("builtin div");
        for indent in [0usize, 1, 1024, usize::MAX] {
            match builtin_compile_fn(div, &CompileTarget::Rust, &div.data_model, indent) {
                ResultStringCompileError::Ok(s) => assert_eq!(
                    s.as_str(),
                    "Dom::create_node(NodeType::Div)",
                    "indent is unused by builtin_compile_fn (indent={indent})"
                ),
                ResultStringCompileError::Err(e) => panic!("unexpected error: {e:?}"),
            }
        }
    }

    #[test]
    fn builtin_compile_fn_emits_text_and_escapes_it() {
        let map = ComponentMap::with_builtin();
        let p = map.get_unqualified("p").expect("builtin p");
        let data = p
            .data_model
            .clone()
            .with_default("text", ComponentDefaultValue::String(AzString::from("a\"b\\c")));

        match builtin_compile_fn(p, &CompileTarget::Rust, &data, 0) {
            ResultStringCompileError::Ok(s) => {
                assert!(s.as_str().contains("a\\\"b\\\\c"), "got {}", s.as_str());
            }
            ResultStringCompileError::Err(e) => panic!("unexpected error: {e:?}"),
        }
    }

    #[test]
    fn builtin_compile_fn_covers_every_target() {
        let map = ComponentMap::with_builtin();
        let div = map.get_unqualified("div").expect("builtin div");
        for target in [
            CompileTarget::Rust,
            CompileTarget::C,
            CompileTarget::Cpp,
            CompileTarget::Python,
        ] {
            match builtin_compile_fn(div, &target, &div.data_model, 0) {
                ResultStringCompileError::Ok(s) => {
                    assert!(!s.as_str().is_empty(), "{target:?} emitted nothing");
                }
                ResultStringCompileError::Err(e) => panic!("{target:?}: {e:?}"),
            }
        }
    }

    // ================================================================
    // user_defined_render_fn / user_defined_compile_fn
    // ================================================================

    fn every_default_kind() -> Vec<ComponentDataField> {
        use ComponentDefaultValue as D;
        vec![
            data_field("s", ComponentFieldType::String, Some(D::String(AzString::from("txt"))), ""),
            data_field("b", ComponentFieldType::Bool, Some(D::Bool(true)), ""),
            data_field("i32", ComponentFieldType::I32, Some(D::I32(i32::MIN)), ""),
            data_field("i64", ComponentFieldType::I64, Some(D::I64(i64::MIN)), ""),
            data_field("u32", ComponentFieldType::U32, Some(D::U32(u32::MAX)), ""),
            data_field("u64", ComponentFieldType::U64, Some(D::U64(u64::MAX)), ""),
            data_field("us", ComponentFieldType::Usize, Some(D::Usize(usize::MAX)), ""),
            data_field("f32", ComponentFieldType::F32, Some(D::F32(f32::NAN)), ""),
            data_field("f64", ComponentFieldType::F64, Some(D::F64(f64::INFINITY)), ""),
            data_field(
                "c",
                ComponentFieldType::ColorU,
                Some(D::ColorU(ColorU { r: 0, g: 0, b: 0, a: 0 })),
                "",
            ),
            data_field("cb", ComponentFieldType::StyledDom, Some(D::CallbackFnPointer(AzString::from("on_click"))), ""),
            data_field("j", ComponentFieldType::String, Some(D::Json(AzString::from("{}"))), ""),
            data_field("none", ComponentFieldType::String, Some(D::None), ""),
            data_field("missing", ComponentFieldType::String, None, ""),
        ]
    }

    #[test]
    fn user_defined_render_fn_handles_every_default_value_kind() {
        let map = ComponentMap::with_builtin();
        let def = user_def("", every_default_kind());
        assert!(matches!(
            user_defined_render_fn(&def, &def.data_model, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));
    }

    #[test]
    fn user_defined_render_fn_on_an_empty_model_and_with_css() {
        let map = ComponentMap::with_builtin();
        let empty = user_def("", Vec::new());
        assert!(matches!(
            user_defined_render_fn(&empty, &empty.data_model, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));

        let styled = user_def(".widget { color: red; }", Vec::new());
        assert!(matches!(
            user_defined_render_fn(&styled, &styled.data_model, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));
    }

    #[test]
    fn user_defined_render_fn_unknown_sub_component_renders_a_placeholder() {
        let map = ComponentMap::create(); // empty: no library can resolve the instance
        let def = user_def(
            "",
            vec![data_field(
                "child",
                ComponentFieldType::StyledDom,
                Some(ComponentDefaultValue::ComponentInstance(ComponentInstanceDefault {
                    library: AzString::from("nope"),
                    component: AzString::from("missing"),
                    field_overrides: Vec::new().into(),
                })),
                "",
            )],
        );
        assert!(
            matches!(
                user_defined_render_fn(&def, &def.data_model, &map),
                ResultStyledDomRenderDomError::Ok(_)
            ),
            "an unresolvable sub-component must render a placeholder, not error out"
        );
    }

    #[test]
    fn user_defined_compile_fn_indent_zero_and_every_target() {
        let def = user_def("", every_default_kind());
        for target in [
            CompileTarget::Rust,
            CompileTarget::C,
            CompileTarget::Cpp,
            CompileTarget::Python,
        ] {
            match user_defined_compile_fn(&def, &target, &def.data_model, 0) {
                ResultStringCompileError::Ok(s) => {
                    assert!(!s.as_str().is_empty(), "{target:?} emitted nothing");
                }
                ResultStringCompileError::Err(e) => panic!("{target:?}: {e:?}"),
            }
        }
    }

    #[test]
    fn user_defined_compile_fn_indent_scales_the_leading_whitespace() {
        // NOTE: `indent` is used as `" ".repeat(indent * 4)`, so it is NOT safe at
        // usize::MAX (the multiply overflows). Exercise the realistic range.
        let def = user_def("", Vec::new());
        let mut prev = 0usize;
        for indent in [0usize, 1, 2, 8] {
            match user_defined_compile_fn(&def, &CompileTarget::Rust, &def.data_model, indent) {
                ResultStringCompileError::Ok(s) => {
                    let len = s.as_str().len();
                    assert!(len > prev, "indent={indent} must widen the output");
                    prev = len;
                }
                ResultStringCompileError::Err(e) => panic!("indent={indent}: {e:?}"),
            }
        }
    }

    #[test]
    fn user_defined_compile_fn_escapes_string_defaults() {
        let def = user_def(
            "",
            vec![data_field(
                "s",
                ComponentFieldType::String,
                Some(ComponentDefaultValue::String(AzString::from("a\"b\\c"))),
                "",
            )],
        );
        match user_defined_compile_fn(&def, &CompileTarget::Rust, &def.data_model, 0) {
            ResultStringCompileError::Ok(s) => {
                assert!(s.as_str().contains("a\\\"b\\\\c"), "got:\n{}", s.as_str());
            }
            ResultStringCompileError::Err(e) => panic!("{e:?}"),
        }
    }

    #[test]
    fn push_scalar_field_appends_one_div_per_call() {
        let mut children: Vec<Dom> = Vec::new();
        push_scalar_field(&mut children, "n", &i64::MIN);
        push_scalar_field(&mut children, "", &f32::NAN);
        push_scalar_field(&mut children, "\u{1F600}", &usize::MAX);
        assert_eq!(children.len(), 3);
    }

    // ================================================================
    // Structural builtins: if / for / map
    // ================================================================

    #[test]
    fn builtin_if_for_map_component_defs_are_well_formed() {
        for (def, model, field) in [
            (builtin_if_component(), "IfData", "condition"),
            (builtin_for_component(), "ForData", "count"),
            (builtin_map_component(), "MapData", "data_json"),
        ] {
            assert_eq!(def.id.collection.as_str(), "builtin");
            assert_eq!(def.data_model.name.as_str(), model);
            assert!(
                def.data_model.get_field(field).is_some(),
                "{model} must expose `{field}`"
            );
        }
    }

    #[test]
    fn builtin_if_render_fn_defaults_to_the_else_branch() {
        let map = ComponentMap::create();
        let def = builtin_if_component();
        // Missing / wrongly-typed condition => false, no panic.
        let empty = dm("IfData", Vec::new());
        assert!(matches!(
            builtin_if_render_fn(&def, &empty, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));

        let truthy = def
            .data_model
            .clone()
            .with_default("condition", ComponentDefaultValue::Bool(true));
        assert!(matches!(
            builtin_if_render_fn(&def, &truthy, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));
    }

    #[test]
    fn builtin_for_render_fn_handles_zero_and_a_wrongly_typed_count() {
        let map = ComponentMap::create();
        let def = builtin_for_component();

        let zero = def
            .data_model
            .clone()
            .with_default("count", ComponentDefaultValue::U32(0));
        assert!(matches!(
            builtin_for_render_fn(&def, &zero, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));

        // A non-U32 default falls back to the documented default of 3.
        let wrong_type = def
            .data_model
            .clone()
            .with_default("count", ComponentDefaultValue::String(AzString::from("9")));
        assert!(matches!(
            builtin_for_render_fn(&def, &wrong_type, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));
    }

    #[test]
    fn builtin_map_render_fn_defaults_to_an_empty_json_array() {
        let map = ComponentMap::create();
        let def = builtin_map_component();
        assert!(matches!(
            builtin_map_render_fn(&def, &dm("MapData", Vec::new()), &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));
        let garbage = def
            .data_model
            .clone()
            .with_default("data_json", ComponentDefaultValue::String(AzString::from("{{{")));
        assert!(
            matches!(
                builtin_map_render_fn(&def, &garbage, &map),
                ResultStyledDomRenderDomError::Ok(_)
            ),
            "malformed JSON must not panic — it is only echoed into a label"
        );
    }

    #[test]
    fn structural_builtin_compile_fns_ignore_indent_entirely() {
        let cases: [(ComponentDef, ComponentCompileFn); 3] = [
            (builtin_if_component(), builtin_if_compile_fn),
            (builtin_for_component(), builtin_for_compile_fn),
            (builtin_map_component(), builtin_map_compile_fn),
        ];
        for (def, f) in cases {
            for target in [
                CompileTarget::Rust,
                CompileTarget::C,
                CompileTarget::Cpp,
                CompileTarget::Python,
            ] {
                for indent in [0usize, usize::MAX] {
                    match f(&def, &target, &def.data_model, indent) {
                        ResultStringCompileError::Ok(s) => {
                            assert!(!s.as_str().is_empty(), "{target:?}/{indent} emitted nothing");
                        }
                        ResultStringCompileError::Err(e) => panic!("{target:?}: {e:?}"),
                    }
                }
            }
        }
    }

    // ================================================================
    // data_field / builtin_data_model / builtin_component_def
    // ================================================================

    #[test]
    fn data_field_required_is_the_inverse_of_having_a_default() {
        let with = data_field(
            "x",
            ComponentFieldType::String,
            Some(ComponentDefaultValue::String(AzString::from("v"))),
            "d",
        );
        assert!(!with.required);
        assert_eq!(with.description.as_str(), "d");

        let without = data_field("x", ComponentFieldType::String, None, "");
        assert!(without.required);
        assert!(matches!(
            without.default_value,
            OptionComponentDefaultValue::None
        ));
    }

    #[test]
    fn builtin_data_model_unknown_tag_is_empty() {
        assert!(builtin_data_model("").is_empty());
        assert!(builtin_data_model("div").is_empty());
        assert!(builtin_data_model("\u{1F600}").is_empty());
        assert!(builtin_data_model(&"z".repeat(10_000)).is_empty());
    }

    #[test]
    fn builtin_data_model_known_tags_expose_their_attributes() {
        let a = builtin_data_model("a");
        assert!(
            a.iter().any(|f| f.name.as_str() == "href"),
            "<a> must expose href"
        );
        // `src` on <img> is required (it has no default value).
        let img = builtin_data_model("img");
        let src = img
            .iter()
            .find(|f| f.name.as_str() == "src")
            .expect("img has src");
        assert!(src.required, "<img src> must be a required field");
        // `img` and `image` share the same model.
        assert_eq!(builtin_data_model("image").len(), img.len());
    }

    #[test]
    fn builtin_component_def_default_text_controls_the_text_field() {
        let with_text = builtin_component_def("p", "Paragraph", Some("Hi"), "");
        assert_eq!(
            with_text.data_model.get_default_string("text").map(AzString::as_str),
            Some("Hi")
        );
        assert_eq!(with_text.data_model.name.as_str(), "ParagraphData");
        assert_eq!(with_text.id.qualified_name(), "builtin:p");

        let no_text = builtin_component_def("div", "Div", None, "");
        assert!(
            no_text.data_model.get_field("text").is_none(),
            "a `None` default_text means the element has no text field at all"
        );

        // An empty-string default still creates the field.
        let empty_text = builtin_component_def("span", "Span", Some(""), "");
        assert!(empty_text.data_model.get_field("text").is_some());
        assert_eq!(
            empty_text.data_model.get_default_string("text").map(AzString::as_str),
            Some("")
        );
    }

    // ================================================================
    // xml_attrs_to_data_model
    // ================================================================

    #[test]
    fn xml_attrs_to_data_model_overrides_defaults_from_attributes() {
        let base = builtin_component_def("a", "Link", Some("Link text"), "").data_model;
        let model = xml_attrs_to_data_model(&base, &attrs(&[("href", "/x")]), None);
        assert_eq!(
            model.get_default_string("href").map(AzString::as_str),
            Some("/x")
        );
        assert_eq!(
            model.get_default_string("text").map(AzString::as_str),
            Some("Link text"),
            "un-supplied fields keep their defaults"
        );
        assert_eq!(
            model.fields.as_ref().len(),
            base.fields.as_ref().len(),
            "no field is added or dropped"
        );
    }

    #[test]
    fn xml_attrs_to_data_model_text_content_is_prepared_and_empty_text_is_ignored() {
        let base = builtin_component_def("a", "Link", Some("Link text"), "").data_model;

        let with_text = xml_attrs_to_data_model(&base, &attrs(&[]), Some("  Hello &amp; bye  "));
        assert_eq!(
            with_text.get_default_string("text").map(AzString::as_str),
            Some("Hello & bye"),
            "text content is trimmed and entity-decoded"
        );

        let blank = xml_attrs_to_data_model(&base, &attrs(&[]), Some("   \n\t "));
        assert_eq!(
            blank.get_default_string("text").map(AzString::as_str),
            Some("Link text"),
            "whitespace-only text content leaves the default intact"
        );
    }

    #[test]
    fn xml_attrs_to_data_model_ignores_unknown_attributes() {
        let base = builtin_component_def("a", "Link", Some(""), "").data_model;
        let before = base.fields.as_ref().len();
        let model = xml_attrs_to_data_model(
            &base,
            &attrs(&[("data-nonsense", "1"), ("", ""), ("\u{1F600}", "x")]),
            None,
        );
        assert_eq!(
            model.fields.as_ref().len(),
            before,
            "unknown attributes must not create fields"
        );
    }

    // ================================================================
    // DomXml
    // ================================================================

    #[test]
    fn dom_xml_into_styled_dom_matches_the_from_impl() {
        let via_method: StyledDom = DomXml::default().into_styled_dom();
        let via_from: StyledDom = DomXml::default().into();
        assert_eq!(
            via_method, via_from,
            "into_styled_dom() must be exactly the From<DomXml> impl"
        );
    }

    // ================================================================
    // Display impls  (serializer)
    // ================================================================

    fn pos() -> XmlTextPos {
        XmlTextPos {
            row: u32::MAX,
            col: 0,
        }
    }

    #[test]
    fn xml_text_pos_display_is_non_empty_for_edge_values() {
        assert_eq!(
            format!("{}", XmlTextPos { row: 0, col: 0 }),
            "line 0:0",
            "a zero position is still rendered"
        );
        assert_eq!(
            format!(
                "{}",
                XmlTextPos {
                    row: u32::MAX,
                    col: u32::MAX
                }
            ),
            "line 4294967295:4294967295"
        );
    }

    #[test]
    fn xml_stream_error_display_covers_every_variant() {
        let variants = vec![
            XmlStreamError::UnexpectedEndOfStream,
            XmlStreamError::InvalidName,
            XmlStreamError::NonXmlChar(NonXmlCharError {
                ch: u32::MAX,
                pos: pos(),
            }),
            XmlStreamError::InvalidChar(InvalidCharError {
                expected: u8::MAX,
                got: 0,
                pos: pos(),
            }),
            XmlStreamError::InvalidCharMultiple(InvalidCharMultipleError {
                expected: 0,
                got: Vec::<u8>::new().into(),
                pos: pos(),
            }),
            XmlStreamError::InvalidQuote(InvalidQuoteError { got: 0, pos: pos() }),
            XmlStreamError::InvalidSpace(InvalidSpaceError { got: 0, pos: pos() }),
            XmlStreamError::InvalidString(InvalidStringError {
                got: AzString::from(""),
                pos: pos(),
            }),
            XmlStreamError::InvalidReference,
            XmlStreamError::InvalidExternalID,
            XmlStreamError::InvalidCommentData,
            XmlStreamError::InvalidCommentEnd,
            XmlStreamError::InvalidCharacterData,
        ];
        for v in &variants {
            let s = format!("{v}");
            assert!(!s.is_empty(), "{v:?} must render a non-empty message");
        }
        // `char::from_u32(u32::MAX)` is None — the formatter must not unwrap it.
        assert!(format!("{}", variants[2]).contains("None"));
    }

    #[test]
    fn xml_parse_error_display_covers_every_variant() {
        let te = XmlTextError {
            stream_error: XmlStreamError::InvalidName,
            pos: pos(),
        };
        let variants = vec![
            XmlParseError::InvalidDeclaration(te.clone()),
            XmlParseError::InvalidComment(te.clone()),
            XmlParseError::InvalidPI(te.clone()),
            XmlParseError::InvalidDoctype(te.clone()),
            XmlParseError::InvalidEntity(te.clone()),
            XmlParseError::InvalidElement(te.clone()),
            XmlParseError::InvalidAttribute(te.clone()),
            XmlParseError::InvalidCdata(te.clone()),
            XmlParseError::InvalidCharData(te),
            XmlParseError::UnknownToken(pos()),
        ];
        for v in &variants {
            assert!(!format!("{v}").is_empty(), "{v:?} must render");
        }
    }

    #[test]
    fn xml_error_display_covers_the_non_css_variants() {
        let variants = vec![
            XmlError::NoParserAvailable,
            XmlError::InvalidXmlPrefixUri(pos()),
            XmlError::UnexpectedXmlUri(pos()),
            XmlError::UnexpectedXmlnsUri(pos()),
            XmlError::InvalidElementNamePrefix(pos()),
            XmlError::DuplicatedNamespace(DuplicatedNamespaceError {
                ns: AzString::from(""),
                pos: pos(),
            }),
            XmlError::UnknownNamespace(UnknownNamespaceError {
                ns: AzString::from("\u{1F600}"),
                pos: pos(),
            }),
            XmlError::UnexpectedCloseTag(UnexpectedCloseTagError {
                expected: AzString::from("a"),
                actual: AzString::from("b"),
                pos: pos(),
            }),
            XmlError::UnexpectedEntityCloseTag(pos()),
            XmlError::UnknownEntityReference(UnknownEntityReferenceError {
                entity: AzString::from("x"),
                pos: pos(),
            }),
            XmlError::MalformedEntityReference(pos()),
            XmlError::EntityReferenceLoop(pos()),
            XmlError::InvalidAttributeValue(pos()),
            XmlError::DuplicatedAttribute(DuplicatedAttributeError {
                attribute: AzString::from("id"),
                pos: pos(),
            }),
            XmlError::NoRootNode,
            XmlError::SizeLimit,
            XmlError::DtdDetected,
            XmlError::MalformedHierarchy(MalformedHierarchyError {
                expected: AzString::from("app"),
                got: AzString::from("p"),
            }),
            XmlError::ParserError(XmlParseError::UnknownToken(pos())),
            XmlError::UnclosedRootNode,
            XmlError::UnexpectedDeclaration(pos()),
            XmlError::NodesLimitReached,
            XmlError::AttributesLimitReached,
            XmlError::NamespacesLimitReached,
            XmlError::InvalidName(pos()),
            XmlError::NonXmlChar(pos()),
            XmlError::InvalidChar(pos()),
            XmlError::InvalidChar2(pos()),
            XmlError::InvalidString(pos()),
            XmlError::InvalidExternalID(pos()),
            XmlError::InvalidComment(pos()),
            XmlError::InvalidCharacterData(pos()),
            XmlError::UnknownToken(pos()),
            XmlError::UnexpectedEndOfStream,
        ];
        for v in &variants {
            assert!(!format!("{v}").is_empty(), "{v:?} must render");
        }
    }

    #[test]
    fn component_and_render_and_compile_error_display() {
        let unknown = ComponentError::UnknownComponent(AzString::from("\u{1F600}"));
        assert!(format!("{unknown}").contains("Unknown component"));

        let useless = ComponentError::UselessFunctionArgument(UselessFunctionArgumentError {
            component_name: AzString::from("c"),
            argument_name: AzString::from("a"),
            valid_args: Vec::<AzString>::new().into(),
        });
        assert!(!format!("{useless}").is_empty());

        let render: RenderDomError = unknown.clone().into();
        assert!(!format!("{render}").is_empty());

        let compile: CompileError = render.clone().into();
        assert!(!format!("{compile}").is_empty());

        let dom_xml: DomXmlParseError = render.into();
        assert!(!format!("{dom_xml}").is_empty());
        let compile2: CompileError = dom_xml.into();
        assert!(!format!("{compile2}").is_empty());
    }

    #[test]
    fn dom_xml_parse_error_display_covers_the_non_css_variants() {
        let variants = vec![
            DomXmlParseError::NoHtmlNode,
            DomXmlParseError::MultipleHtmlRootNodes,
            DomXmlParseError::NoBodyInHtml,
            DomXmlParseError::MultipleBodyNodes,
            DomXmlParseError::Xml(XmlError::NoRootNode),
            DomXmlParseError::MalformedHierarchy(MalformedHierarchyError {
                expected: AzString::from("app"),
                got: AzString::from("p"),
            }),
            DomXmlParseError::RenderDom(RenderDomError::Component(
                ComponentError::UnknownComponent(AzString::from("x")),
            )),
            DomXmlParseError::Component(ComponentParseError::NotAComponent),
        ];
        for v in &variants {
            assert!(!format!("{v}").is_empty(), "{v:?} must render");
        }
    }

    #[test]
    fn component_parse_error_display_covers_the_non_css_variants() {
        let variants = vec![
            ComponentParseError::NotAComponent,
            ComponentParseError::UnnamedComponent,
            ComponentParseError::MissingName(usize::MAX),
            ComponentParseError::MissingType(MissingTypeError {
                arg_pos: 0,
                arg_name: AzString::from(""),
            }),
            ComponentParseError::WhiteSpaceInComponentName(WhiteSpaceInComponentNameError {
                arg_pos: usize::MAX,
                arg_name: AzString::from("a b"),
            }),
            ComponentParseError::WhiteSpaceInComponentType(WhiteSpaceInComponentTypeError {
                arg_pos: 0,
                arg_name: AzString::from("a"),
                arg_type: AzString::from("b c"),
            }),
        ];
        for v in &variants {
            assert!(!format!("{v}").is_empty(), "{v:?} must render");
        }
    }

    // ================================================================
    // serde-json gated: ComponentDataModel::to_json / from_json
    // ================================================================

    #[cfg(feature = "serde-json")]
    #[test]
    fn data_model_to_json_round_trips() {
        let m = model_with_text();
        let json = m.to_json().expect("serializes");
        let back = ComponentDataModel::from_json(&json).expect("deserializes");
        assert_eq!(back.name.as_str(), m.name.as_str());
        assert_eq!(back.fields.as_ref().len(), m.fields.as_ref().len());
        assert_eq!(back.get_default_string("text").map(AzString::as_str), Some("hi"));
    }

    #[cfg(feature = "serde-json")]
    #[test]
    fn data_model_from_json_rejects_garbage_without_panicking() {
        for s in [
            "",
            "   ",
            "\t\n",
            "not json",
            "{",
            "[]",
            "null",
            "0",
            "-0",
            "9223372036854775807",
            "NaN",
            "\u{1F600}",
        ] {
            assert!(
                ComponentDataModel::from_json(s).is_err(),
                "{s:?} is not a data model"
            );
        }
    }

    #[cfg(feature = "serde-json")]
    #[test]
    fn data_model_from_json_deeply_nested_input_does_not_stack_overflow() {
        let bomb = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
        assert!(
            ComponentDataModel::from_json(&bomb).is_err(),
            "serde_json must reject the nesting bomb, not crash"
        );
    }

    #[cfg(feature = "serde-json")]
    #[test]
    fn data_model_to_json_on_an_empty_model() {
        let m = dm("Empty", Vec::new());
        let json = m.to_json().expect("serializes");
        assert!(json.contains("\"fields\""), "got {json}");
        assert!(ComponentDataModel::from_json(&json).is_ok());
    }
}
