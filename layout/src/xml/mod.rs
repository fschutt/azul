//! XML/HTML parsing module for the Azul toolkit.
//!
//! Provides two parsing paths:
//! - `parse_xml_string`: builds an `XmlNode` tree (used by `domxml_from_str`)
//! - `parse_xml_to_fast_dom_with_css`: builds an arena-based `FastDom` directly from XML tokens
//!   (used by `parse_xml_to_styled_dom`)
//!
//! Both paths handle HTML5-lite features: void elements, auto-closing tags,
//! XML entity decoding, `<style>` CSS extraction, and BOM/DOCTYPE stripping.
//!
//! Data types (`XmlNode`, `XmlError`, etc.) live in `azul_core::xml`; this
//! module provides the parsing implementations.

#![allow(unused_variables)]

use alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec};
use core::fmt;
#[cfg(feature = "std")]
use std::path::Path;

#[cfg(feature = "svg")]
pub mod svg;

/// Decodes the character references in a string by XML's rules with HTML's
/// names (the strict loaders' text and attribute values):
/// `azul_core::xml::html::decode_character_references` in
/// `CharRefMode::Xml` - `&lt;` `&amp;` ..., the numeric references, and the
/// HTML Standard's 2231 names (`&copy;`, `&NotEqualTilde;`), each ended by
/// its `;`. Returns `Cow::Borrowed` when there is nothing to decode.
fn decode_xml_entities(s: &str) -> std::borrow::Cow<'_, str> {
    azul_core::xml::html::decode_character_references(s, azul_core::xml::html::CharRefMode::Xml)
}

/// [`decode_xml_entities`], always allocating (the twin the tests compare
/// the borrowing path with).
#[cfg(test)]
fn decode_xml_entities_slow(s: &str) -> std::borrow::Cow<'_, str> {
    std::borrow::Cow::Owned(decode_xml_entities(s).into_owned())
}

pub use azul_core::xml::*;
use azul_core::{dom::Dom, impl_from, styled_dom::StyledDom};
#[cfg(feature = "parser")]
use azul_css::parser2::CssParseError;
use azul_css::{css::Css, AzString, OptionString, U8Vec};
use xmlparser::Tokenizer;

#[cfg(feature = "xml")]
#[must_use]
pub fn domxml_from_str(xml: &str, component_map: &ComponentMap) -> DomXml {
    let error_css = Css::empty();

    let parsed = match parse_xml_string(xml) {
        Ok(parsed) => parsed,
        Err(e) => {
            return DomXml {
                parsed_dom: {
                    let mut dom = Dom::create_body()
                        .with_children(vec![Dom::create_p_with_text(format!("{e}"))].into());
                    StyledDom::create(&mut dom, error_css)
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
                        .with_children(vec![Dom::create_p_with_text(format!("{e}"))].into());
                    StyledDom::create(&mut dom, error_css)
                },
            };
        }
    };

    DomXml { parsed_dom }
}

/// Creates a `Dom` from an already-parsed `Xml` structure, for use in layout
/// callbacks. CSS from `<style>` tags is attached to `Dom.css` and applied
/// during the cascade pass.
// FFI-exported (api.json fn_body azul_layout::xml::dom_from_parsed_xml(xml)): owned Xml by value.
#[allow(clippy::needless_pass_by_value)]
#[must_use]
pub fn dom_from_parsed_xml(xml: Xml) -> Dom {
    let component_map = ComponentMap::with_builtin();
    match str_to_dom_unstyled(xml.root.as_ref(), &component_map) {
        Ok(dom) => dom,
        Err(e) => {
            Dom::create_body().with_children(vec![Dom::create_p_with_text(format!("{e}"))].into())
        }
    }
}

/// Fastest path: parse XML string directly into `FastDom` without intermediate `XmlNode` tree.
///
/// Feeds XML tokenizer events directly into `CompactDomBuilder`, skipping both the
/// `XmlNode` tree construction AND the Dom tree construction.
/// Parse XML string directly into a `FastDom` (arena-based DOM) in a single pass.
///
/// Also extracts `<style>` tag content as CSS. Returns both the `FastDom` and
/// collected CSS stylesheets. No intermediate `XmlNode` tree is built.
///
/// This is the fastest XML→DOM path: XML tokens feed directly into
/// `CompactDomBuilder`, and `<style>` text is collected inline.
/// # Errors
///
/// Returns an `XmlError` if the XML cannot be parsed.
pub fn parse_xml_to_fast_dom(xml: &str) -> Result<azul_core::dom::FastDom, XmlError> {
    let (fast_dom, _css) = parse_xml_to_fast_dom_with_css(xml)?;
    Ok(fast_dom)
}

/// `parse_xml_to_styled_dom`, but resolving `<icon>` nodes on the way.
///
/// Routes through `Dom` (a real tree) rather than `FastDom`, and that is not an
/// oversight: an icon resolves to a SUBTREE, and `FastDom` - like `StyledDom` -
/// is a flat arena in DFS order, so a subtree cannot be spliced into it without
/// inserting mid-arena and shifting every index after it. Resolving on the tree
/// and cascading once is what makes an arbitrary `Dom` usable as an icon.
///
/// Use this whenever an icon provider is available. `parse_xml_to_styled_dom`
/// exists for callers that have none, and differs only in that.
///
/// The raw form controls are replaced with a memory of their own, which
/// nothing else reads: a caller that has a WINDOW styles the document with
/// `LayoutWindow::style_xml_document` instead, so the form it lands in can
/// read the controls' values (and a reset forget them) in the window's
/// memory.
///
/// # Errors
///
/// Returns an `XmlError` if the XML cannot be parsed.
pub fn parse_xml_to_styled_dom_resolving_icons(
    xml: &str,
    provider: &azul_core::icon::SharedIconProvider,
    system_style: &azul_css::system::SystemStyle,
) -> Result<StyledDom, XmlError> {
    styled_xml_document(xml, provider, system_style, resolve_form_controls_detached)
}

/// A `Dom` built outside any window - `AzBuilder`'s component previews - styled
/// the way [`parse_xml_to_styled_dom_resolving_icons`] styles a parsed
/// document: raw form controls become widgets (with a memory of their own),
/// `<icon>`s resolve against `provider` (without one they stay as they are),
/// then the cascade, with no window context (the UA's light table).
#[must_use]
pub fn style_detached_dom(
    mut dom: Dom,
    provider: Option<&azul_core::icon::SharedIconProvider>,
    system_style: &azul_css::system::SystemStyle,
) -> StyledDom {
    resolve_form_controls_detached(&mut dom);
    match provider {
        Some(provider) => azul_core::icon::styled_dom_resolving_icons(dom, provider, system_style),
        None => StyledDom::create_from_dom(dom),
    }
}

/// Raw `<input>` / `<select>` / `<textarea>` / `<form>` nodes → their widgets,
/// with a form-control memory nothing else reads (no window owns the DOM).
fn resolve_form_controls_detached(dom: &mut Dom) {
    #[cfg(feature = "widgets")]
    let _ = crate::form_controls::resolve_form_controls_in_dom(
        dom,
        &crate::form_controls::FormControlMemory::default(),
        crate::form_controls::FORM_SCOPE_ROOT,
    );
}

/// THE path from an XML document to a `StyledDom` with its icons resolved,
/// behind [`parse_xml_to_styled_dom_resolving_icons`] and
/// `LayoutWindow::style_xml_document`: parse, let `resolve_form_controls`
/// replace the raw `<input>` / `<select>` / `<textarea>` / `<form>` nodes by
/// widgets (with whose memory is the caller's to say) - first, like in every
/// other app DOM (`crate::form_controls`), and before the icons: the widgets
/// contain icons - then resolve the icons and cascade.
pub(crate) fn styled_xml_document(
    xml: &str,
    provider: &azul_core::icon::SharedIconProvider,
    system_style: &azul_css::system::SystemStyle,
    resolve_form_controls: impl FnOnce(&mut Dom),
) -> Result<StyledDom, XmlError> {
    let parsed = parse_xml(xml)?;
    let mut dom = dom_from_parsed_xml(parsed);
    resolve_form_controls(&mut dom);
    Ok(azul_core::icon::styled_dom_resolving_icons(
        dom,
        provider,
        system_style,
    ))
}

/// Parse XML directly into `FastDom` + extracted CSS, ready for `StyledDom`.
#[allow(clippy::cast_precision_loss)] // bounded layout/render numeric cast
/// # Errors
///
/// Returns an `XmlError` if the XML cannot be parsed.
pub fn parse_xml_to_styled_dom(xml: &str) -> Result<StyledDom, XmlError> {
    // Optional per-phase RSS/timing breakdown.
    // Gated on AZ_PROFILE=memory — prints
    //   [XML] tokenize+fast_dom       : +XX MiB in YY ms
    //   [XML] css attach              : +XX MiB in YY ms
    //   [XML] create_from_fast_dom    : +XX MiB in YY ms
    // to locate which sub-phase of the parse-cascade dominates the
    // RSS jump seen between `page start` and `xml parsed`.
    let mem_on = memory_profile_enabled();

    let rss0 = if mem_on { peak_rss_bytes() } else { 0 };
    let (fast_dom, css) = parse_xml_to_fast_dom_with_css(xml)?;
    if mem_on {
        let rss1 = peak_rss_bytes();
        eprintln!(
            "[XML] tokenize+fast_dom       : +{:.2} MiB",
            (rss1.saturating_sub(rss0)) as f64 / 1024.0 / 1024.0,
        );
    }
    Ok(styled_document(fast_dom, css, mem_on))
}

/// HTML as a browser reads it, straight into a `StyledDom`: the LENIENT twin
/// of [`parse_xml_to_styled_dom`] (the same arena, the same `<head>` and
/// `<style>` handling; the tokenizer and the tree construction of
/// [`parse_html_string`]). Never fails.
#[must_use]
pub fn parse_html_to_styled_dom(source: &str) -> StyledDom {
    let (fast_dom, css) = parse_html_to_fast_dom_with_css(source);
    styled_document(fast_dom, css, memory_profile_enabled())
}

/// Whether `AZ_PROFILE=memory` asks for the per-phase RSS breakdown.
fn memory_profile_enabled() -> bool {
    static MEM_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *MEM_ENABLED.get_or_init(azul_core::profile::memory_enabled)
}

/// A document loader's arena and stylesheets as a `StyledDom`: the
/// stylesheets merged into one global sheet, then the cascade.
#[allow(clippy::cast_precision_loss)] // bounded layout/render numeric cast
fn styled_document(
    mut fast_dom: azul_core::dom::FastDom,
    css: Vec<Css>,
    mem_on: bool,
) -> StyledDom {
    let rss1 = if mem_on { peak_rss_bytes() } else { 0 };
    // Attach CSS to the FastDom
    if !css.is_empty() {
        // Rules AND keyframes: merging by rules alone silently dropped every
        // `@keyframes` block a `<style>` element declared, so
        // `-azul-animation-out: shrinkOut 1s` fell back to the default slide
        // at runtime while the unit parser tests stayed green.
        let mut combined_rules = Vec::new();
        let mut combined_keyframes = Vec::new();
        for c in css {
            combined_rules.extend(c.rules.into_library_owned_vec());
            combined_keyframes.extend(c.keyframes.into_library_owned_vec());
        }
        let mut combined_css = Css::new(combined_rules);
        combined_css.keyframes = combined_keyframes.into();
        fast_dom.css = vec![azul_core::dom::CssWithNodeId {
            node_id: 0, // global scope
            css: combined_css,
        }]
        .into();
    }
    if mem_on {
        let rss2 = peak_rss_bytes();
        eprintln!(
            "[XML] css attach              : +{:.2} MiB",
            (rss2.saturating_sub(rss1)) as f64 / 1024.0 / 1024.0,
        );
    }

    // Hint the allocator to return pages freed by the CSS parser.
    // The tokenizer+parser created many small allocations (selectors,
    // declarations, strings) that are now packed into FastDom. Purging
    // here returns those pages before the cascade allocates more.
    crate::probe::hint_purge_allocator();

    let rss2 = if mem_on { peak_rss_bytes() } else { 0 };
    let styled = StyledDom::create_from_fast_dom(fast_dom);

    // Major purge point: the cascade just freed ~3 MiB of intermediate
    // allocations (build-phase Vecs, CSS selector matching state, pruned
    // properties). Tell the allocator to return those pages NOW before
    // the layout pass allocates more on top of them.
    crate::probe::hint_purge_allocator();

    if mem_on {
        let rss3 = peak_rss_bytes();
        eprintln!(
            "[XML] create_from_fast_dom    : +{:.2} MiB",
            (rss3.saturating_sub(rss2)) as f64 / 1024.0 / 1024.0,
        );
    }

    styled
}

/// Resident-set bytes for RSS checkpoints — mirrors servo-shot's
/// `peak_rss_bytes()`. Uses `getrusage(RUSAGE_SELF)` via the
/// `probe` feature's `libc` dep; returns 0 without it so the
/// caller just doesn't emit meaningful deltas.
#[cfg(all(unix, feature = "probe"))]
fn peak_rss_bytes() -> u64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &raw mut usage) } != 0 {
        return 0;
    }
    let ru = usage.ru_maxrss as u64;
    // macOS reports bytes, Linux reports KiB.
    #[cfg(target_os = "macos")]
    {
        ru
    }
    #[cfg(not(target_os = "macos"))]
    {
        ru.saturating_mul(1024)
    }
}

#[cfg(not(all(unix, feature = "probe")))]
const fn peak_rss_bytes() -> u64 {
    0
}

/// One open element of the document loader's arena ([`FastDomSink`]).
#[derive(Debug, Clone, Copy)]
struct FastOpen {
    /// In the arena (not inside the `<head>`, not an element that draws
    /// nothing).
    emitted: bool,
    /// Keeps its subtree out of the arena.
    hides: bool,
    body: bool,
    svg: bool,
    style: bool,
}

/// The document loader's [`html::TreeSink`]: the elements straight into a
/// `FastDom` arena (`CompactDomBuilder`, no `XmlNode` tree in between).
///
/// The `<head>` stays out of the arena but its `<style>`s are collected, as
/// every `<style>` is; text that is only white space is dropped outside the
/// `<body>`; inside an `<svg>`, an element that draws nothing (`<metadata>`,
/// a foreign namespace's editor state) is left out with its subtree - as the
/// tree loader's DOM builder leaves it out.
struct FastDomSink<'k> {
    builder: CompactDomBuilder,
    /// One bump arena for every AzString produced during this parse —
    /// id/class tokens, text nodes, etc. Replaces ~1k small heap allocs
    /// with a handful of 64 KiB chunks. Each AzString carries its own
    /// Arc reference to the arena, so the arena survives until the last
    /// string is dropped (typically when the StyledDom is dropped).
    str_arena: azul_css::corety::StringArena,
    /// The parser's key map, computed once (the `style` attributes).
    css_key_map: &'k azul_css::props::property::CssKeyMap,
    css: Vec<Css>,
    open: Vec<FastOpen>,
    /// Open elements that keep their subtree out of the arena.
    hidden: usize,
    /// Open `<body>`s: inside one, white space is text.
    bodies: usize,
    /// Open `<svg>`s in the arena.
    svgs: usize,
    /// The text of the open `<style>`.
    style: Option<String>,
}

impl<'k> FastDomSink<'k> {
    fn new(source_len: usize, css_key_map: &'k azul_css::props::property::CssKeyMap) -> Self {
        const ESTIMATED_BYTES_PER_NODE: usize = 20;
        Self {
            builder: CompactDomBuilder::with_capacity(source_len / ESTIMATED_BYTES_PER_NODE),
            str_arena: azul_css::corety::StringArena::new(),
            css_key_map,
            css: Vec::new(),
            open: Vec::new(),
            hidden: 0,
            bodies: 0,
            svgs: 0,
            style: None,
        }
    }

    fn finish(self) -> (azul_core::dom::FastDom, Vec<Css>) {
        let Self {
            builder,
            str_arena,
            css,
            ..
        } = self;
        // Drop the arena handle explicitly. AzStrings already embedded in
        // the FastDom keep the backing bytes alive via their cloned Arc refs.
        drop(str_arena);
        (builder.finish(), css)
    }
}

impl html::TreeSink for FastDomSink<'_> {
    fn open_element(&mut self, name: &str, attributes: &[(String, String)]) {
        // The `<head>` and a `<style>` are not nodes of the document (a
        // stylesheet is collected; the tree loader's DOM builder lifts it onto
        // its parent element), nor, inside an `<svg>`, an element that draws
        // nothing.
        let hides = name == "head"
            || name == "style"
            || (self.svgs > 0 && element_draws_nothing(name, name));
        let emitted = self.hidden == 0 && !hides;
        if emitted {
            open_fast_node(
                &mut self.builder,
                &mut self.str_arena,
                name,
                attributes,
                self.css_key_map,
            );
        }
        let style = name == "style";
        if style {
            self.style = Some(String::new());
        }
        let body = name == "body";
        let svg = emitted && name == "svg";
        self.hidden += usize::from(hides);
        self.bodies += usize::from(body);
        self.svgs += usize::from(svg);
        self.open.push(FastOpen {
            emitted,
            hides,
            body,
            svg,
            style,
        });
    }

    fn close_element(&mut self) {
        let Some(open) = self.open.pop() else {
            return;
        };
        if open.emitted {
            self.builder.close_node();
        }
        self.hidden -= usize::from(open.hides);
        self.bodies -= usize::from(open.body);
        self.svgs -= usize::from(open.svg);
        if open.style {
            if let Some(text) = self.style.take() {
                if !text.is_empty() {
                    self.css.push(Css::from_string(text.into()));
                }
            }
        }
    }

    fn text(&mut self, text: &str) {
        if let Some(style) = self.style.as_mut() {
            style.push_str(text);
            return;
        }
        if self.hidden > 0 {
            return;
        }
        // Skip whitespace-only text at <html> level (between </head> and <body>)
        // but keep whitespace inside <body> (it's significant for inline layout)
        if self.bodies > 0 || !text.trim().is_empty() {
            let text = self.str_arena.intern(text);
            self.builder.add_leaf(
                azul_core::dom::NodeData::create_text_do_not_use_without_block_level_wrapper(text),
            );
        }
    }
}

/// Open the arena node of a `tag` element (lower-case) with its attributes.
#[allow(clippy::too_many_lines)] // large but cohesive: one element, every attribute kind
fn open_fast_node(
    builder: &mut CompactDomBuilder,
    str_arena: &mut azul_css::corety::StringArena,
    tag: &str,
    attrs: &[(String, String)],
    css_key_map: &azul_css::props::property::CssKeyMap,
) {
    use azul_core::dom::{NodeData, NodeType};

    let node_type = tag_to_node_type(tag);
    let mut nd = NodeData::create_node(node_type);

    // `<transient-window open="true" anchor="bottom" …>`: the config rides
    // INSIDE the NodeType, so its attributes are applied onto that payload
    // rather than stored as generic attributes. Done before the generic
    // loop so the keys it consumes never reach `attr_vec`.
    let mut transient_cfg = match nd.get_node_type() {
        NodeType::TransientWindow(c) => Some(*c),
        _ => None,
    };

    // `attr_vec`: what the popup config leaves on the node
    // (`tearoff-zone`); `settings`: every other attribute.
    let mut attr_vec: Vec<azul_core::dom::AttributeType> = Vec::new();
    let mut settings: Vec<(u8, azul_core::xml::attributes::NodeSetting)> = Vec::new();
    for (key, value) in attrs {
        if let Some(cfg) = transient_cfg.as_mut() {
            if cfg.apply_attr(key.as_str(), value.as_str()) {
                // `tearoff="zone:<selector>"`: the MODE rides in the
                // config (it is `Copy`), the selector - a string - stays
                // on the node as its `tearoff-zone` attribute, where the
                // engine's drop handling reads it.
                if key == "tearoff" {
                    if let Some(selector) = value.trim().strip_prefix("zone:") {
                        attr_vec.push(azul_core::dom::AttributeType::Custom(
                            azul_core::dom::AttributeNameValue {
                                attr_name: str_arena.intern("tearoff-zone"),
                                value: str_arena.intern(selector.trim()),
                            },
                        ));
                    }
                }
                continue;
            }
        }
        // Every other attribute through the ONE table core's loader and
        // the code generator read too (`azul_core::xml::attributes`).
        if let Some(setting) =
            azul_core::xml::attributes::setting_of(tag, key.as_str(), value.as_str())
        {
            settings.push(setting);
        }
    }
    // HTML's presentational hints (`<font color>`, `<ol type>`, `<center>`,
    // `<img align>` ...): the CSS of the element's builtin arguments, before
    // its `style` attribute - the same function core's DOM builder asks.
    let hints = azul_core::xml::builtin_presentational_hints(
        tag,
        attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())),
    );
    let hint_props = if hints.is_empty() {
        Vec::new()
    } else {
        azul_core::xml::attributes::style_declarations(&hints, css_key_map)
    };
    azul_core::xml::attributes::apply_settings(
        &mut nd,
        azul_core::xml::attributes::ordered(settings.into_iter()),
        hint_props,
        Some(css_key_map),
        &mut |s: &str| str_arena.intern(s),
    );

    // The element's COMPONENT arguments (`<a href target rel>`,
    // `<img src alt>`): the fields its builtin component declares,
    // filled from the attributes and landed on the node by the same
    // functions core's loader and the builtin render fn use.
    azul_core::xml::apply_builtin_args_from_attributes(
        tag,
        attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())),
        &mut nd,
    );

    // ---- Fluent / l10n handling ----
    // `<p data-l10n="greeting_key" data-l10n-name="Alice">` stays a
    // `<p>`: its `data-l10n-*` arguments go on the element, and the key
    // becomes its first child (below, once the element is open) - the
    // shape core's XML builders produce. We do a second pass over the
    // same attrs slice rather than keeping state inside the match
    // because we need all of them to be visible at once.
    let l10n_key = attrs
        .iter()
        .find(|(k, _)| k.as_str() == "data-l10n")
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.is_empty());
    if l10n_key.is_some() {
        // Collect data-l10n-* arguments.
        let fluent_args = azul_core::dom::FluentArgKVVec::from_l10n_attributes(
            attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())),
        );
        if !fluent_args.is_empty() {
            nd.fluent_args = Some(Box::new(fluent_args));
        }
    }

    if !attr_vec.is_empty() {
        let mut all = nd.attributes().clone().into_library_owned_vec();
        all.extend(attr_vec);
        nd.set_attributes(all.into());
    }
    // Write the parsed popup config back into the node's payload.
    if let Some(cfg) = transient_cfg {
        nd.set_node_type(NodeType::TransientWindow(cfg));
    }

    builder.open_node(nd);

    // The key, marked localizable, as the element's first child.
    if let Some(key) = l10n_key {
        builder.add_leaf(
            NodeData::create_text_do_not_use_without_block_level_wrapper(
                azul_css::corety::AzString::tr(key),
            ),
        );
    }
}

/// The strict loaders' tokenizer (`xmlparser`: an XML syntax error is an
/// error) feeding the one tree construction, [`html::TreeBuilder`], which
/// both strict loaders and the lenient ones share - so the two loaders build
/// one tree from one document. Text and attribute values are decoded by
/// XML's rules (with HTML's names). Returns the start tag the input ended in
/// when it was cut off (`<svg` and the end): the tree loader rejects it, the
/// document loader keeps it.
fn feed_xml_tokens(
    tokenizer: Tokenizer<'_>,
    builder: &mut html::TreeBuilder,
    sink: &mut dyn html::TreeSink,
) -> Result<Option<(String, Vec<(String, String)>)>, XmlError> {
    use xmlparser::{ElementEnd, Token};

    // A namespace prefix is part of the name: `<user:card/>` is the `card`
    // component of library `user` (`ComponentMap::get_by_qualified_name`),
    // `<svg:rect/>` a rect, and `</o:p>` closes an `<o:p>`, not a `<p>`.
    fn qualified(prefix: &str, local: &str) -> String {
        if prefix.is_empty() {
            String::from(local)
        } else {
            format!("{prefix}:{local}")
        }
    }

    let mut start: Option<(String, Vec<(String, String)>)> = None;
    for token in tokenizer {
        let token = token.map_err(|e| XmlError::ParserError(translate_xmlparser_error(e)))?;
        match token {
            Token::ElementStart { prefix, local, .. } => {
                start = Some((qualified(prefix.as_str(), local.as_str()), Vec::new()));
            }
            Token::Attribute { local, value, .. } => {
                if let Some((_, attributes)) = start.as_mut() {
                    attributes.push((
                        String::from(local.as_str()),
                        decode_xml_entities(value.as_str()).into_owned(),
                    ));
                }
            }
            Token::ElementEnd {
                end: ElementEnd::Open,
                ..
            } => {
                if let Some((name, attributes)) = start.take() {
                    builder.start_tag(sink, &name, attributes, false);
                }
            }
            Token::ElementEnd {
                end: ElementEnd::Empty,
                ..
            } => {
                if let Some((name, attributes)) = start.take() {
                    builder.start_tag(sink, &name, attributes, true);
                }
            }
            Token::ElementEnd {
                end: ElementEnd::Close(prefix, local),
                ..
            } => builder.end_tag(sink, &qualified(prefix.as_str(), local.as_str())),
            Token::Text { text } => builder.text(sink, &decode_xml_entities(text.as_str())),
            Token::Comment { text, .. } => builder.comment(sink, text.as_str()),
            Token::Cdata { text, .. } => builder.cdata(sink, text.as_str()),
            _ => {}
        }
    }
    Ok(start)
}

/// Internal: parse XML into `FastDom` + collected CSS stylesheets.
fn parse_xml_to_fast_dom_with_css(
    xml: &str,
) -> Result<(azul_core::dom::FastDom, Vec<Css>), XmlError> {
    // Strip BOM
    let xml = xml.strip_prefix('\u{FEFF}').unwrap_or(xml);
    let mut xml = xml.trim();

    // Skip <?xml ... ?>
    if xml.starts_with("<?") {
        if let Some(pos) = xml.find("?>") {
            xml = &xml[(pos + 2)..];
        }
    }

    // Skip <!DOCTYPE ...>
    let mut xml = xml.trim();
    if xml.len() > 9
        && xml.is_char_boundary(9)
        && xml[..9].to_ascii_lowercase().starts_with("<!doctype")
    {
        if let Some(pos) = xml.find('>') {
            xml = &xml[(pos + 1)..];
        }
    } else if xml.starts_with("<!--") {
        if let Some(end) = xml.find("-->") {
            xml = &xml[(end + 3)..];
            xml = xml.trim();
        }
    }

    // Pre-compute the CSS key map once (used for style= attribute parsing)
    let css_key_map = azul_css::props::property::get_css_key_map();
    let mut sink = FastDomSink::new(xml.len(), &css_key_map);
    // The one tree construction, the names lower-cased (an HTML document
    // written as XML: `<DIV>` is a div).
    let mut builder = html::TreeBuilder::new(html::TreeRules::XmlFolded);
    let cut_off = feed_xml_tokens(
        Tokenizer::from_fragment(xml, 0..xml.len()),
        &mut builder,
        &mut sink,
    )?;
    // A start tag cut off by the end of the input still opens its element
    // (the document loader always kept it), and every open element closes.
    if let Some((name, attributes)) = cut_off {
        builder.start_tag(&mut sink, &name, attributes, false);
    }
    let _ = builder.finish(&mut sink);
    Ok(sink.finish())
}

/// The lenient twin of [`parse_xml_to_fast_dom_with_css`]: HTML as a browser
/// reads it ([`html::parse_html_into`]), into the same arena.
fn parse_html_to_fast_dom_with_css(source: &str) -> (azul_core::dom::FastDom, Vec<Css>) {
    let css_key_map = azul_css::props::property::get_css_key_map();
    let mut sink = FastDomSink::new(source.len(), &css_key_map);
    html::parse_html_into(source, &mut sink);
    sink.finish()
}

/// Loads, parses and builds a DOM from an XML file
///
/// **Warning**: The file is reloaded from disk on every function call - do not
/// use this in release builds! This function deliberately never fails: In an error case,
/// the error gets rendered as a `NodeType::Label`.
#[cfg(all(feature = "std", feature = "xml"))]
pub fn domxml_from_file<I: AsRef<Path>>(file_path: I, component_map: &ComponentMap) -> DomXml {
    use std::fs;

    let error_css = Css::empty();

    let xml = match fs::read_to_string(file_path.as_ref()) {
        Ok(xml) => xml,
        Err(e) => {
            return DomXml {
                parsed_dom: {
                    let mut dom = Dom::create_body().with_children(
                        vec![Dom::create_p_with_text(format!(
                            "Error reading: \"{}\": {}",
                            file_path.as_ref().to_string_lossy(),
                            e
                        ))]
                        .into(),
                    );
                    StyledDom::create(&mut dom, error_css)
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
///
/// STRICT: `xmlparser` tokenizes (an XML syntax error is an error, and so is
/// an element left open at the end); the tree construction is the one every
/// loader shares ([`html::TreeBuilder`], [`html::TreeRules::Xml`]: the names
/// as written, void elements, implied end tags, end tags matched within
/// their scope). HTML as a browser reads it: [`parse_html_string`].
#[cfg(feature = "xml")]
/// # Errors
///
/// Returns an `XmlError` if the XML cannot be parsed.
pub fn parse_xml_string(xml: &str) -> Result<Vec<XmlNodeChild>, XmlError> {
    // Strip UTF-8 BOM if present (some W3C test files have it)
    let xml = xml.strip_prefix('\u{FEFF}').unwrap_or(xml);
    // The text as given: a parse error's line / column are counted in it, not
    // in what the prefix stripping below leaves (a pasted document with blank
    // lines and a doctype before a broken tag must point at that tag's line).
    let full = xml;

    // Search for "<?xml" and "?>" tags and delete them from the XML
    let mut xml = xml.trim();
    if xml.starts_with("<?") {
        let pos = xml
            .find("?>")
            .ok_or(XmlError::MalformedHierarchy(MalformedHierarchyError {
                expected: "<?xml".into(),
                got: "?>".into(),
            }))?;
        xml = &xml[(pos + 2)..];
    }

    // Delete <!DOCTYPE ...> if necessary (case-insensitive)
    let mut xml = xml.trim();
    if xml.len() > 9
        && xml.is_char_boundary(9)
        && xml[..9].to_ascii_lowercase().starts_with("<!doctype")
    {
        let pos = xml
            .find('>')
            .ok_or(XmlError::MalformedHierarchy(MalformedHierarchyError {
                expected: "<!DOCTYPE".into(),
                got: ">".into(),
            }))?;
        xml = &xml[(pos + 1)..];
    } else if xml.starts_with("<!--") {
        // Skip HTML comments at the start
        if let Some(end) = xml.find("-->") {
            xml = &xml[(end + 3)..];
            xml = xml.trim();
        }
    }

    // `xml` is a slice of `full`: tokenize that range of the full text, so the
    // positions are the full text's.
    let start = (xml.as_ptr() as usize).saturating_sub(full.as_ptr() as usize);
    let tokenizer = Tokenizer::from_fragment(full, start..start + xml.len());

    let mut builder = html::TreeBuilder::new(html::TreeRules::Xml);
    let mut sink = html::XmlTreeSink::new();
    let cut_off = feed_xml_tokens(tokenizer, &mut builder, &mut sink)?;
    // A well-formed document closes every element it opens. A bare "<svg"
    // with no closing bracket (the fragment tokenizer yields one
    // ElementStart, then cleanly ends) is open too: rejected instead of
    // returning a "valid" partial tree.
    if cut_off.is_some() || builder.finish(&mut sink) != 0 {
        return Err(XmlError::UnclosedRootNode);
    }
    Ok(sink.finish())
}

#[cfg(feature = "xml")]
/// # Errors
///
/// Returns an `XmlError` if the XML cannot be parsed.
pub fn parse_xml(s: &str) -> Result<Xml, XmlError> {
    Ok(Xml {
        root: parse_xml_string(s)?.into(),
    })
}

#[cfg(not(feature = "xml"))]
pub fn parse_xml(s: &str) -> Result<Xml, XmlError> {
    Err(XmlError::NoParserAvailable)
}

/// HTML as a browser reads it - a mail, a paste, a page: the LENIENT loader.
///
/// Never fails: unquoted and bare attributes, upper-case names, `<br>`
/// without its slash, `p` / `li` / `td` ... without their end tags, a stray
/// end tag, `<` and `&` in text, Word's and Outlook's markup, the HTML named
/// character references - from what a browser builds a tree from, this
/// builds the same tree (`azul_core::xml::html`, which lists what its tree
/// construction simplifies). A fragment is a document: its `<html>`,
/// `<head>` and `<body>` are implied. The strict loaders
/// ([`parse_xml_string`], [`parse_xml_to_styled_dom`]) stay strict: an XML
/// syntax error is reported there.
#[must_use]
pub fn parse_html_string(source: &str) -> Vec<XmlNodeChild> {
    html::parse_html_nodes(source)
}

/// [`parse_html_string`] as an [`Xml`] document (for [`dom_from_parsed_xml`]).
#[must_use]
pub fn parse_html(source: &str) -> Xml {
    Xml::create_from_html(AzString::from(source))
}

// to_string(&self) -> String

#[cfg(feature = "xml")]
#[must_use]
pub fn translate_roxmltree_expandedname(e: roxmltree::ExpandedName<'_, '_>) -> XmlQualifiedName {
    let ns: Option<AzString> = e.namespace().map(|e| e.to_string().into());
    XmlQualifiedName {
        local_name: e.name().to_string().into(),
        namespace: ns.into(),
    }
}

#[cfg(feature = "xml")]
fn translate_roxmltree_attribute(e: roxmltree::Attribute<'_, '_>) -> XmlQualifiedName {
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
                got: a,
                pos: translate_xmlparser_textpos(tp),
            })
        }
        xmlparser::StreamError::InvalidSpace(a, tp) => {
            XmlStreamError::InvalidSpace(InvalidSpaceError {
                got: a,
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
#[must_use]
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
#[inline]
const fn translate_xmlparser_textpos(o: xmlparser::TextPos) -> XmlTextPos {
    XmlTextPos {
        row: o.row,
        col: o.col,
    }
}

#[cfg(feature = "xml")]
#[inline]
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
    /// This method parses the XML string and converts it to an Azul `StyledDom`.
    /// On error, it returns a `StyledDom` displaying the error message.
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

// ============================================================================
// Adversarial unit tests (autotest). Inline so the private helpers
// (`decode_xml_entities*`, `parse_xml_to_fast_dom_with_css`, `peak_rss_bytes`,
// `translate_*`) are reachable.
// ============================================================================

#[cfg(test)]
mod autotest_generated {
    use azul_core::dom::{FastDom, NodeData, NodeType, TabIndex};

    use super::*;

    // ------------------------------------------------------------------
    // helpers
    // ------------------------------------------------------------------

    /// Element children of an `XmlNodeChild` slice (skips text nodes).
    #[cfg(feature = "xml")]
    fn elements(children: &[XmlNodeChild]) -> Vec<&XmlNode> {
        children
            .iter()
            .filter_map(XmlNodeChild::as_element)
            .collect()
    }

    /// Text children of an `XmlNodeChild` slice (skips element nodes).
    #[cfg(feature = "xml")]
    fn texts(children: &[XmlNodeChild]) -> Vec<&str> {
        children.iter().filter_map(XmlNodeChild::as_text).collect()
    }

    /// `<html><body>…</body></html>` around `body`.
    ///
    /// Every fixture goes through this so the document's first 9 bytes are
    /// ASCII: `parse_xml*` slices `xml[..9]` for the DOCTYPE sniff without a
    /// char-boundary check (see
    /// `parse_entrypoints_do_not_panic_on_short_multibyte_input`).
    fn doc(body: &str) -> String {
        format!("<html><body>{body}</body></html>")
    }

    /// Flat node arena of a `FastDom`.
    fn nodes(dom: &FastDom) -> &[NodeData] {
        dom.node_data.as_ref()
    }

    /// Text content of a `NodeType::Text` node (`None` for every other kind).
    fn text_of(nd: &NodeData) -> Option<String> {
        match nd.get_node_type() {
            NodeType::Text(_) => nd.get_node_type().format(),
            _ => None,
        }
    }

    /// Minimal XML escaper — the inverse of `decode_xml_entities`.
    #[cfg(feature = "xml")]
    fn escape(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&apos;"),
                _ => out.push(c),
            }
        }
        out
    }

    /// Non-grammar / hostile fragments. All ASCII on purpose so they exercise
    /// the tokenizer rather than the `xml[..9]` slice.
    const GARBAGE: &[&str] = &[
        "<<<<>>>>",
        "!!!not xml at all!!!",
        "<a b=c>",
        "</>",
        "</div>",
        "<a></a",
        "&&&&&&&&&&&&",
        "]]>",
        "<!--",
        "<![CDATA[",
        "<?",
        "<!DOCTYPE",
        "\u{0}\u{1}\u{2}",
        "<a><<a><<<a>",
        "= = = = = = = = = =",
    ];

    // ------------------------------------------------------------------
    // decode_xml_entities / decode_xml_entities_slow
    // ------------------------------------------------------------------

    #[test]
    fn decode_xml_entities_borrows_when_there_is_no_ampersand() {
        for s in ["", "hello", "  ", "日本語 🙂", "<tag/>", "a;b;c;"] {
            assert!(
                matches!(decode_xml_entities(s), std::borrow::Cow::Borrowed(_)),
                "{s:?} has no '&' and must take the zero-alloc path"
            );
            assert_eq!(&*decode_xml_entities(s), s);
        }
    }

    #[test]
    fn decode_xml_entities_decodes_the_five_named_entities_and_nbsp() {
        assert_eq!(&*decode_xml_entities("&lt;"), "<");
        assert_eq!(&*decode_xml_entities("&gt;"), ">");
        assert_eq!(&*decode_xml_entities("&amp;"), "&");
        assert_eq!(&*decode_xml_entities("&apos;"), "'");
        assert_eq!(&*decode_xml_entities("&quot;"), "\"");
        assert_eq!(&*decode_xml_entities("&nbsp;"), "\u{00A0}");
        assert_eq!(
            &*decode_xml_entities("a&lt;b&gt;c&amp;d&quot;e&apos;f"),
            "a<b>c&d\"e'f"
        );
    }

    #[test]
    fn decode_xml_entities_decodes_numeric_references() {
        // decimal, lowercase hex, uppercase hex marker
        assert_eq!(&*decode_xml_entities("&#60;"), "<");
        assert_eq!(&*decode_xml_entities("&#x3C;"), "<");
        assert_eq!(&*decode_xml_entities("&#X3c;"), "<");
        assert_eq!(&*decode_xml_entities("&#65;"), "A");
        // boundary code points: NUL, BMP max, astral, and the last legal scalar
        assert_eq!(&*decode_xml_entities("&#0;"), "\u{0}");
        assert_eq!(&*decode_xml_entities("&#xFFFF;"), "\u{FFFF}");
        assert_eq!(&*decode_xml_entities("&#65536;"), "\u{10000}");
        assert_eq!(&*decode_xml_entities("&#1114111;"), "\u{10FFFF}");
        assert_eq!(&*decode_xml_entities("&#x10FFFF;"), "\u{10FFFF}");
        // combining marks survive
        assert_eq!(&*decode_xml_entities("e&#x301;"), "e\u{301}");
    }

    #[test]
    fn decode_xml_entities_keeps_out_of_range_and_surrogate_code_points_verbatim() {
        // Every one of these must round-trip to itself: no panic, no
        // replacement char, no silent truncation to a wrong scalar.
        for s in [
            "&#xD800;",      // lone high surrogate
            "&#xDFFF;",      // lone low surrogate
            "&#55296;",      // decimal surrogate
            "&#x110000;",    // one past the last scalar
            "&#1114112;",    // decimal, one past the last scalar
            "&#123456789;",  // entity name is exactly 10 bytes (the length cap)
            "&#4294967296;", // u32::MAX + 1
            "&#99999999999999;",
            "&#x;",
            "&#;",
            "&#xZZ;",
            "&#-1;",
        ] {
            assert_eq!(
                &*decode_xml_entities(s),
                s,
                "{s:?} is not a decodable reference and must be preserved byte-for-byte"
            );
        }
    }

    #[test]
    fn decode_xml_entities_keeps_unterminated_and_unknown_entities_verbatim() {
        for s in [
            // The entity table is case-sensitive (`&LT;` is HTML's own
            // upper-case name for `<`, `&lT;` is nothing).
            "&", "&&", "&lt", "&#", "&#x", "&foo;", "&lT;", "&Amp;", "& lt;", "a & b", "&;",
        ] {
            assert_eq!(&*decode_xml_entities(s), s, "{s:?} must be preserved");
        }
        // Trailing garbage after a valid entity is still emitted.
        assert_eq!(&*decode_xml_entities("&lt;&"), "<&");
    }

    #[test]
    fn decode_xml_entities_does_not_double_decode() {
        // A single pass only. `&amp;lt;` is the escaped form of the literal
        // text `&lt;` and must NOT collapse to `<` — that would be an
        // injection vector for anything that escapes user text once.
        assert_eq!(&*decode_xml_entities("&amp;lt;"), "&lt;");
        assert_eq!(&*decode_xml_entities("&amp;amp;"), "&amp;");
        assert_eq!(&*decode_xml_entities("&amp;#60;"), "&#60;");
    }

    #[test]
    fn decode_xml_entities_handles_pathological_input_without_panicking() {
        // Entity name far past the 10-byte cap: bails out and preserves input.
        let long_name = format!("&{};", "a".repeat(10_000));
        assert_eq!(&*decode_xml_entities(&long_name), long_name);

        // Unterminated '&' followed by a megabyte of text.
        let long_tail = format!("&{}", "x".repeat(1_000_000));
        assert_eq!(decode_xml_entities(&long_tail).len(), long_tail.len());

        // Multibyte / astral / combining input mixed with entities. The entity
        // scanner uses `char::is_alphanumeric`, so multibyte chars can land in
        // the accumulator — slicing must stay on char boundaries.
        for s in [
            "&\u{1F600}\u{1F600};",
            "&½;",
            "&日本;",
            "&#\u{1F600};",
            "&e\u{301};",
            "🙂&amp;🙂",
        ] {
            let out = decode_xml_entities(s);
            assert!(
                !out.is_empty(),
                "{s:?} decoded to nothing (input was non-empty)"
            );
        }

        // Alternating entities at scale must not go quadratic-and-panic.
        let many = "&lt;".repeat(50_000);
        assert_eq!(decode_xml_entities(&many).chars().count(), 50_000);
    }

    #[test]
    fn decode_xml_entities_slow_matches_the_fast_path() {
        // The fast path is only a `contains('&')` short-circuit: for '&'-free
        // input the slow path must be the identity, and for everything else
        // the two must agree exactly.
        for s in [
            "",
            "plain",
            "日本語 🙂",
            "&lt;",
            "&amp;lt;",
            "&#x1F600;",
            "&unknown;",
            "&",
        ] {
            assert_eq!(
                &*decode_xml_entities(s),
                &*decode_xml_entities_slow(s),
                "fast/slow path disagree on {s:?}"
            );
        }
        for s in ["", "plain", "日本語 🙂", "a;b", "<>"] {
            assert_eq!(&*decode_xml_entities_slow(s), s);
        }
    }

    // ------------------------------------------------------------------
    // KNOWN BUG: unchecked `xml[..9]` slice in the DOCTYPE sniff
    // ------------------------------------------------------------------

    /// `parse_xml_string` (line ~737) and `parse_xml_to_fast_dom_with_css`
    /// (line ~328) both do
    ///
    /// ```ignore
    /// if xml.len() > 9 && xml[..9].to_ascii_lowercase().starts_with("<!doctype")
    /// ```
    ///
    /// `&str[..9]` panics when byte 9 is not a UTF-8 char boundary, so any
    /// input longer than 9 bytes whose third-or-so character is multibyte
    /// aborts the parse with `byte index 9 is not a char boundary` instead of
    /// returning `Err`. `domxml_from_str`, which documents that it "deliberately
    /// never fails", inherits the panic.
    ///
    /// The fix belongs in the source (`xml.is_char_boundary(9)` guard, or
    /// `xml.get(..9)`), so this test asserts the correct invariant and is
    /// expected to be RED until that lands.
    #[cfg(feature = "xml")]
    #[test]
    fn parse_entrypoints_do_not_panic_on_short_multibyte_input() {
        // 3 x 4-byte emoji = 12 bytes; boundaries are 0/4/8/12, so 9 is inside
        // the third character.
        const INPUT: &str = "😀😀😀";
        assert!(INPUT.len() > 9 && !INPUT.is_char_boundary(9));

        let a = std::panic::catch_unwind(|| parse_xml_string(INPUT).is_ok());
        let b = std::panic::catch_unwind(|| parse_xml_to_fast_dom(INPUT).is_ok());

        assert!(
            a.is_ok(),
            "parse_xml_string panicked on {INPUT:?}: the DOCTYPE sniff slices xml[..9] without an \
             is_char_boundary check"
        );
        assert!(
            b.is_ok(),
            "parse_xml_to_fast_dom panicked on {INPUT:?}: same unchecked xml[..9] slice"
        );
    }

    // ------------------------------------------------------------------
    // parse_xml_string
    // ------------------------------------------------------------------

    #[cfg(feature = "xml")]
    #[test]
    fn parse_xml_string_accepts_empty_and_whitespace_only_input() {
        for s in [
            "",
            " ",
            "   ",
            "\t\n",
            "\r\n\r\n",
            "\u{FEFF}",
            "\u{FEFF}   ",
        ] {
            let parsed = parse_xml_string(s)
                .unwrap_or_else(|e| panic!("{s:?} should parse to an empty tree, got {e}"));
            assert!(parsed.is_empty(), "{s:?} produced {} roots", parsed.len());
        }
    }

    #[cfg(feature = "xml")]
    #[test]
    fn parse_xml_string_parses_a_minimal_document() {
        let parsed = parse_xml_string(&doc("<div>hi</div>")).expect("valid document");
        let roots = elements(&parsed);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].node_type.as_str(), "html");

        let body = elements(roots[0].children.as_ref());
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].node_type.as_str(), "body");

        let div = elements(body[0].children.as_ref());
        assert_eq!(div.len(), 1);
        assert_eq!(div[0].node_type.as_str(), "div");
        assert_eq!(texts(div[0].children.as_ref()), vec!["hi"]);
    }

    #[cfg(feature = "xml")]
    #[test]
    fn parse_xml_string_rejects_unclosed_elements() {
        // A well-formed document unwinds to the root sentinel; anything left
        // open must be an error rather than a silently-truncated tree.
        assert!(matches!(
            parse_xml_string("<div>"),
            Err(XmlError::UnclosedRootNode)
        ));
        assert!(matches!(
            parse_xml_string("<html><body><div>"),
            Err(XmlError::UnclosedRootNode)
        ));
        assert!(
            parse_xml_string("<html><body><div>text").is_err(),
            "an unclosed element must not yield a partial 'valid' tree"
        );
    }

    #[cfg(feature = "xml")]
    #[test]
    fn parse_xml_string_rejects_truncated_declaration_and_doctype() {
        assert!(matches!(
            parse_xml_string("<?xml version=\"1.0\""),
            Err(XmlError::MalformedHierarchy(_))
        ));
        assert!(matches!(
            parse_xml_string("<!DOCTYPE html PUBLIC \"x\""),
            Err(XmlError::MalformedHierarchy(_))
        ));
        // ...but the complete forms are stripped and the rest parses.
        for prefix in [
            "<?xml version=\"1.0\"?>",
            "<!DOCTYPE html>",
            "<!doctype HTML>",
            "<!-- leading comment -->",
            "\u{FEFF}",
        ] {
            let src = format!("{prefix}{}", doc("<div/>"));
            let parsed = parse_xml_string(&src)
                .unwrap_or_else(|e| panic!("{prefix:?} prefix should be stripped, got {e}"));
            let roots = elements(&parsed);
            assert_eq!(roots.len(), 1, "{prefix:?} -> {roots:?}");
            assert_eq!(roots[0].node_type.as_str(), "html");
        }
    }

    #[cfg(feature = "xml")]
    #[test]
    fn parse_xml_string_is_deterministic_on_garbage() {
        for g in GARBAGE {
            // The contract is "Err or a tree", never a panic and never a
            // different answer for the same bytes.
            let a = parse_xml_string(g);
            let b = parse_xml_string(g);
            assert_eq!(a.is_ok(), b.is_ok(), "{g:?} parsed non-deterministically");
            assert_eq!(a.ok(), b.ok(), "{g:?} produced two different trees");
        }
    }

    #[cfg(feature = "xml")]
    #[test]
    fn parse_xml_string_trims_leading_and_trailing_whitespace() {
        let padded = format!("  \t\n{}\n\t  ", doc("<div/>"));
        let a = parse_xml_string(&padded).expect("padded document");
        let b = parse_xml_string(&doc("<div/>")).expect("bare document");
        assert_eq!(a, b, "surrounding whitespace must not change the tree");
    }

    #[cfg(feature = "xml")]
    #[test]
    fn parse_xml_string_keeps_trailing_junk_as_text() {
        // Lenient HTML-ish parsing: trailing junk becomes a text node at the
        // root rather than an error or a dropped document.
        let parsed =
            parse_xml_string(&format!("{};garbage", doc("<div/>"))).expect("lenient parse");
        assert_eq!(elements(&parsed).len(), 1);
        assert_eq!(texts(&parsed), vec![";garbage"]);
    }

    #[cfg(feature = "xml")]
    #[test]
    fn parse_xml_string_round_trips_escaped_text() {
        for raw in [
            "a",
            "<b>bold</b> & \"quotes\" 'apos'",
            "&&&&",
            "  spaced  ",
            "日本語 🙂 combining e\u{301}",
            "1 < 2 > 0 && true",
        ] {
            let src = doc(&escape(raw));
            let parsed =
                parse_xml_string(&src).unwrap_or_else(|e| panic!("{src:?} should parse, got {e}"));
            let html = elements(&parsed);
            let body = elements(html[0].children.as_ref());
            assert_eq!(
                texts(body[0].children.as_ref()),
                vec![raw],
                "escape -> parse must be the identity for {raw:?}"
            );
        }
    }

    #[cfg(feature = "xml")]
    #[test]
    fn parse_xml_string_decodes_attribute_entities() {
        let parsed = parse_xml_string(&doc(
            r#"<div t="&amp;&lt;&gt;&quot;&apos;x" u="&nosuch;" v="&#x1F600;"></div>"#,
        ))
        .expect("valid document");
        let html = elements(&parsed);
        let body = elements(html[0].children.as_ref());
        let div = elements(body[0].children.as_ref());
        let attrs = &div[0].attributes;

        assert_eq!(attrs.get_key("t").map(AzString::as_str), Some("&<>\"'x"));
        assert_eq!(attrs.get_key("u").map(AzString::as_str), Some("&nosuch;"));
        assert_eq!(attrs.get_key("v").map(AzString::as_str), Some("😀"));
    }

    #[cfg(feature = "xml")]
    #[test]
    fn parse_xml_string_tolerates_extra_and_mismatched_close_tags() {
        let nested = doc("<div></span></div>");
        let paragraphs = doc("<p>one<p>two");
        for src in [
            "<a></a></a>",
            "<a></a></b>",
            nested.as_str(),
            paragraphs.as_str(),
            "<br></br>",
            "<br>",
            "<br><br><br>",
        ] {
            let a = parse_xml_string(src);
            let b = parse_xml_string(src);
            assert_eq!(a.is_ok(), b.is_ok(), "{src:?} is non-deterministic");
            assert_eq!(a.ok(), b.ok(), "{src:?} produced two different trees");
        }
        // A bare void element is a complete document (auto-closed at EOF).
        let parsed = parse_xml_string("<br>").expect("bare void element");
        assert_eq!(elements(&parsed).len(), 1);
        assert_eq!(elements(&parsed)[0].node_type.as_str(), "br");
    }

    #[cfg(feature = "xml")]
    #[test]
    fn parse_xml_string_handles_deep_nesting_without_stack_overflow() {
        // Building is iterative, but dropping the resulting `XmlNode` tree is
        // recursive, so this pins the depth the *whole* lifecycle survives.
        // (The arena path is exercised at 10k in
        // `parse_xml_to_fast_dom_handles_ten_thousand_nested_elements`.)
        const DEPTH: usize = 1_000;
        let mut src = String::with_capacity(DEPTH * 12);
        for _ in 0..DEPTH {
            src.push_str("<a>");
        }
        for _ in 0..DEPTH {
            src.push_str("</a>");
        }

        let parsed = parse_xml_string(&src).expect("balanced nesting is valid");
        let mut depth = 0_usize;
        {
            let mut cursor: Vec<&XmlNode> = elements(&parsed);
            while !cursor.is_empty() {
                depth += 1;
                let node: &XmlNode = cursor[0];
                cursor = elements(node.children.as_ref());
            }
        }
        assert_eq!(depth, DEPTH, "every nesting level must be preserved");
        // Dropping the tree is the recursive half of the lifecycle — a deeper
        // tree would blow the stack here, not during the (iterative) parse.
        drop(parsed);
    }

    #[cfg(feature = "xml")]
    #[test]
    fn parse_xml_string_handles_a_one_million_char_text_node() {
        let payload = "x".repeat(1_000_000);
        let parsed = parse_xml_string(&doc(&payload)).expect("long text is valid");
        let html = elements(&parsed);
        let body = elements(html[0].children.as_ref());
        let t = texts(body[0].children.as_ref());
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].len(), 1_000_000);
    }

    #[cfg(feature = "xml")]
    #[test]
    fn parse_xml_string_handles_many_sibling_elements() {
        const N: usize = 2_000;
        let parsed = parse_xml_string(&doc(&"<i>x</i>".repeat(N))).expect("wide tree is valid");
        let html = elements(&parsed);
        let body = elements(html[0].children.as_ref());
        assert_eq!(elements(body[0].children.as_ref()).len(), N);
    }

    // ------------------------------------------------------------------
    // parse_xml
    // ------------------------------------------------------------------

    #[cfg(feature = "xml")]
    #[test]
    fn parse_xml_agrees_with_parse_xml_string() {
        let one = doc("<div>hi</div>");
        let two = doc("<i/><i/>");
        for src in ["", "   ", one.as_str(), two.as_str()] {
            let via_xml = parse_xml(src).expect("valid");
            let via_string = parse_xml_string(src).expect("valid");
            assert_eq!(
                via_xml.root.as_ref(),
                via_string.as_slice(),
                "parse_xml must be a thin wrapper over parse_xml_string for {src:?}"
            );
        }
        assert!(parse_xml("<div>").is_err());
    }

    #[cfg(not(feature = "xml"))]
    #[test]
    fn parse_xml_without_the_xml_feature_reports_no_parser() {
        for s in ["", "   ", "<div/>", "garbage"] {
            assert!(matches!(parse_xml(s), Err(XmlError::NoParserAvailable)));
        }
    }

    // ------------------------------------------------------------------
    // parse_xml_to_fast_dom / parse_xml_to_fast_dom_with_css
    // ------------------------------------------------------------------

    /// `<transient-window>` parses to its NodeType with the attributes applied
    /// onto the inline config — and those attributes do NOT leak into the
    /// generic attribute list, where they would be meaningless.
    #[test]
    fn transient_window_tag_parses_its_attributes_into_the_config() {
        use azul_core::transient::{TransientAnchor, TransientDismiss};
        let dom = parse_xml_to_fast_dom(
            r#"<div><transient-window open="true" anchor="right" dismiss="escape" size="300x200" class="picker"><p>hi</p></transient-window></div>"#,
        )
        .expect("parses");
        let n = nodes(&dom);
        let tw = n
            .iter()
            .find_map(|nd| match nd.get_node_type() {
                NodeType::TransientWindow(c) => Some((*c, nd)),
                _ => None,
            })
            .expect("a TransientWindow node");
        let (cfg, nd) = tw;
        assert!(cfg.open, "open=\"true\" must open it");
        assert_eq!(cfg.anchor, TransientAnchor::Right);
        assert_eq!(cfg.dismiss, TransientDismiss::Escape);
        assert!(
            matches!(cfg.size, azul_core::geom::OptionLogicalSize::Some(s) if s.width == 300.0)
        );
        // `class` is an ordinary attribute and must survive; the popup keys
        // must NOT have been stored as attributes.
        let classes: Vec<String> = nd
            .get_ids_and_classes()
            .iter()
            .filter_map(|ic| match ic {
                azul_core::dom::IdOrClass::Class(c) => Some(c.as_str().to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(classes, vec!["picker".to_string()]);
    }

    /// With no attributes at all the tag is a CLOSED popup — the default must
    /// never open a window by accident.
    #[test]
    fn a_bare_transient_window_tag_is_closed() {
        let dom = parse_xml_to_fast_dom("<div><transient-window/></div>").expect("parses");
        let closed = nodes(&dom)
            .iter()
            .any(|nd| matches!(nd.get_node_type(), NodeType::TransientWindow(c) if !c.open));
        assert!(closed, "a bare <transient-window/> must parse as closed");
    }

    #[test]
    fn parse_xml_to_fast_dom_accepts_empty_and_whitespace_only_input() {
        for s in ["", " ", "   ", "\t\n", "\u{FEFF}", "\u{FEFF}  \n "] {
            let dom = parse_xml_to_fast_dom(s)
                .unwrap_or_else(|e| panic!("{s:?} should yield an empty arena, got {e}"));
            assert!(
                nodes(&dom).is_empty(),
                "{s:?} produced {} nodes",
                nodes(&dom).len()
            );
            assert_eq!(dom.node_hierarchy.as_ref().len(), nodes(&dom).len());
        }
    }

    #[test]
    fn parse_xml_to_fast_dom_builds_the_expected_arena() {
        let dom = parse_xml_to_fast_dom(&doc("<div>hi</div>")).expect("valid document");
        let n = nodes(&dom);
        assert_eq!(n.len(), 4, "html + body + div + text");
        assert_eq!(
            dom.node_hierarchy.as_ref().len(),
            n.len(),
            "hierarchy and node_data arenas must stay parallel"
        );
        assert!(matches!(n[0].get_node_type(), NodeType::Html));
        assert!(matches!(n[1].get_node_type(), NodeType::Body));
        assert!(matches!(n[2].get_node_type(), NodeType::Div));
        assert_eq!(text_of(&n[3]).as_deref(), Some("hi"));
    }

    #[test]
    fn parse_xml_to_fast_dom_lowercases_tag_names() {
        let dom = parse_xml_to_fast_dom("<HTML><BODY><DiV/></BODY></HTML>").expect("valid");
        let n = nodes(&dom);
        assert_eq!(n.len(), 3);
        assert!(matches!(n[0].get_node_type(), NodeType::Html));
        assert!(matches!(n[1].get_node_type(), NodeType::Body));
        assert!(matches!(n[2].get_node_type(), NodeType::Div));
    }

    #[test]
    fn parse_xml_to_fast_dom_skips_head_but_collects_style_css() {
        let src = "<html><head><title>T</title><style>div { width: 10px; \
                   }</style></head><body>x</body></html>";
        let (dom, css) = parse_xml_to_fast_dom_with_css(src).expect("valid document");
        let n = nodes(&dom);

        assert_eq!(n.len(), 3, "html + body + text; <head> subtree is dropped");
        assert!(
            !n.iter()
                .any(|nd| matches!(nd.get_node_type(), NodeType::Head | NodeType::Title)),
            "no <head>/<title> node may reach the arena"
        );
        assert_eq!(text_of(&n[2]).as_deref(), Some("x"));
        assert_eq!(css.len(), 1, "the <style> body must still be collected");
        assert!(
            !css[0].rules.as_ref().is_empty(),
            "the CSS must have parsed"
        );
    }

    #[test]
    fn parse_xml_to_fast_dom_splits_ids_and_classes_on_whitespace() {
        let dom = parse_xml_to_fast_dom(&doc(r#"<div id="a b" class="c  d
        e"></div>"#))
        .expect("valid document");
        let div = &nodes(&dom)[2];

        assert!(div.has_id("a") && div.has_id("b"));
        assert!(div.has_class("c") && div.has_class("d") && div.has_class("e"));
        assert!(!div.has_id("a b"), "the raw joined value must not survive");
        assert_eq!(div.get_ids_and_classes().as_ref().len(), 5);
    }

    /// Reads the tab index of the `<div>` in `doc("<div {attrs}></div>")`.
    fn tab_index_with(attrs: &str) -> Option<TabIndex> {
        let dom = parse_xml_to_fast_dom(&doc(&format!("<div {attrs}></div>")))
            .unwrap_or_else(|e| panic!("{attrs:?} should parse, got {e}"));
        nodes(&dom)[2].get_tab_index()
    }

    #[test]
    fn parse_xml_to_fast_dom_maps_tabindex_boundaries() {
        assert_eq!(tab_index_with(r#"tabindex="0""#), Some(TabIndex::Auto));
        assert_eq!(tab_index_with(r#"tabindex="-0""#), Some(TabIndex::Auto));
        assert_eq!(
            tab_index_with(r#"tabindex="1""#),
            Some(TabIndex::OverrideInParent(1))
        );
        assert_eq!(
            tab_index_with(r#"tabindex="+3""#),
            Some(TabIndex::OverrideInParent(3)),
            "isize::from_str accepts a leading '+'"
        );
        assert_eq!(
            tab_index_with(r#"tabindex="-1""#),
            Some(TabIndex::NoKeyboardFocus)
        );
        assert_eq!(
            tab_index_with(r#"tabindex="-9223372036854775808""#),
            Some(TabIndex::NoKeyboardFocus),
            "i64::MIN is still just 'negative'"
        );

        // NodeFlags packs the override value into bits [27:0].
        const MAX_EXACT: u32 = (1 << 28) - 1;
        assert_eq!(
            tab_index_with(&format!(r#"tabindex="{MAX_EXACT}""#)),
            Some(TabIndex::OverrideInParent(MAX_EXACT))
        );
        // Past that it truncates rather than saturating or panicking. Two
        // lossy steps stack up here: `isize as u32` in the XML parser, then
        // the 28-bit mask in `NodeFlags::set_tab_index`. Pinned as-is because
        // the safety property is "bounded and deterministic", not "exact".
        assert_eq!(
            tab_index_with(r#"tabindex="268435456""#),
            Some(TabIndex::OverrideInParent(0)),
            "1 << 28 truncates to 0"
        );
        assert_eq!(
            tab_index_with(r#"tabindex="9223372036854775807""#),
            Some(TabIndex::OverrideInParent(MAX_EXACT)),
            "i64::MAX -> u32::MAX -> 28-bit mask"
        );
    }

    #[test]
    fn parse_xml_to_fast_dom_ignores_unparseable_tabindex() {
        let baseline = tab_index_with("");
        for junk in [
            r#"tabindex="""#,
            r#"tabindex="NaN""#,
            r#"tabindex="inf""#,
            r#"tabindex="-inf""#,
            r#"tabindex="1.0""#,
            r#"tabindex="1e5""#,
            r#"tabindex=" 3 ""#,
            r#"tabindex="0x10""#,
            r#"tabindex="99999999999999999999999999""#,
            r#"tabindex="-99999999999999999999999999""#,
            r#"tabindex="🙂""#,
        ] {
            assert_eq!(
                tab_index_with(junk),
                baseline,
                "{junk} must leave the tab index untouched"
            );
        }
    }

    #[test]
    fn parse_xml_to_fast_dom_parses_bool_attributes_case_sensitively() {
        assert_eq!(tab_index_with(r#"focusable="true""#), Some(TabIndex::Auto));
        assert_eq!(
            tab_index_with(r#"focusable="false""#),
            Some(TabIndex::NoKeyboardFocus)
        );
        for junk in [
            r#"focusable="TRUE""#,
            r#"focusable="1""#,
            r#"focusable="yes""#,
        ] {
            assert_eq!(
                tab_index_with(junk),
                tab_index_with(""),
                "{junk} is not a bool literal and must be ignored"
            );
        }

        let editable = |v: &str| {
            let dom = parse_xml_to_fast_dom(&doc(&format!(r#"<div contenteditable="{v}"></div>"#)))
                .expect("valid");
            nodes(&dom)[2].is_contenteditable()
        };
        assert!(editable("true"));
        assert!(!editable("false"));
        assert!(!editable("TRUE"));
        assert!(!editable(""));
        assert!(!editable("1"));

        // `contenteditable="false"` is kept as the attribute the editable
        // inheritance walk and the edit-buffer collector wall a subtree off
        // by; anything that is not the literal `false` is not.
        let walled = |v: &str| {
            let dom = parse_xml_to_fast_dom(&doc(&format!(r#"<div contenteditable="{v}"></div>"#)))
                .expect("valid");
            nodes(&dom)[2]
                .attributes()
                .as_ref()
                .iter()
                .any(|a| matches!(a, azul_core::dom::AttributeType::ContentEditable(false)))
        };
        assert!(walled("false"));
        assert!(!walled("true"));
        assert!(!walled("FALSE"));
        assert!(!walled(""));
    }

    #[test]
    fn parse_xml_to_fast_dom_survives_malformed_style_attributes() {
        let big = "a:b;".repeat(2_000);
        for style in [
            "",
            ";;;;",
            "::::",
            ":",
            "width",
            "width:",
            ":10px",
            "a:b:c",
            ";:;:;:",
            "width:10px",
            "width:10px;;;height:;;",
            "width:not-a-length",
            "🙂:🙂",
            big.as_str(),
        ] {
            let dom = parse_xml_to_fast_dom(&doc(&format!(r#"<div style="{style}"></div>"#)))
                .unwrap_or_else(|e| panic!("style={style:?} should parse, got {e}"));
            assert_eq!(
                nodes(&dom).len(),
                3,
                "style={style:?} must not change the node count"
            );
        }
    }

    #[test]
    fn parse_xml_to_fast_dom_survives_unbalanced_tags() {
        // The interesting case: elements opened inside <head> are pushed onto
        // the tag stack but never opened in the builder, so the EOF unwind
        // calls close_node() more often than open_node() ran. That must be a
        // no-op, not an underflow.
        let dom = parse_xml_to_fast_dom("<html><head><title>").expect("lenient parse");
        assert_eq!(nodes(&dom).len(), 1, "only <html> survives");
        assert!(matches!(nodes(&dom)[0].get_node_type(), NodeType::Html));

        for src in [
            "</div>",
            "</div></div></div>",
            "<a></a></a>",
            "<html><body></body></body></html>",
            "<html><head><head><head>",
            "<html><body><div></span></div></body></html>",
        ] {
            let a = parse_xml_to_fast_dom(src);
            let b = parse_xml_to_fast_dom(src);
            assert_eq!(a.is_ok(), b.is_ok(), "{src:?} is non-deterministic");
            if let (Ok(a), Ok(b)) = (&a, &b) {
                assert_eq!(nodes(a).len(), nodes(b).len(), "{src:?} node count drifted");
            }
        }
    }

    #[test]
    fn parse_xml_to_fast_dom_is_deterministic_on_garbage() {
        for g in GARBAGE {
            let a = parse_xml_to_fast_dom(g);
            let b = parse_xml_to_fast_dom(g);
            assert_eq!(a.is_ok(), b.is_ok(), "{g:?} parsed non-deterministically");
            if let (Ok(a), Ok(b)) = (&a, &b) {
                assert_eq!(nodes(a).len(), nodes(b).len(), "{g:?} node count drifted");
            }
        }
    }

    #[test]
    fn parse_xml_to_fast_dom_handles_ten_thousand_nested_elements() {
        // The arena path is iterative on the way in and flat on the way out,
        // so it should hold a depth the recursive XmlNode tree cannot.
        const DEPTH: usize = 10_000;
        let mut src = String::with_capacity(DEPTH * 12 + 32);
        src.push_str("<html><body>");
        for _ in 0..DEPTH {
            src.push_str("<div>");
        }
        for _ in 0..DEPTH {
            src.push_str("</div>");
        }
        src.push_str("</body></html>");

        let dom = parse_xml_to_fast_dom(&src).expect("balanced nesting is valid");
        assert_eq!(nodes(&dom).len(), DEPTH + 2);
    }

    #[test]
    fn parse_xml_to_fast_dom_handles_a_one_million_char_document() {
        let payload = "x".repeat(1_000_000);
        let dom = parse_xml_to_fast_dom(&doc(&payload)).expect("long text is valid");
        let n = nodes(&dom);
        assert_eq!(n.len(), 3, "html + body + one text node");
        assert_eq!(text_of(&n[2]).map(|s| s.len()), Some(1_000_000));
    }

    #[test]
    fn parse_xml_to_fast_dom_strips_bom_declaration_doctype_and_comments() {
        let expected = nodes(&parse_xml_to_fast_dom(&doc("<div/>")).expect("baseline")).len();
        for prefix in [
            "\u{FEFF}",
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>",
            "<!DOCTYPE html>",
            "<!doctype HTML>",
            "<!DoCtYpE html SYSTEM \"about:legacy-compat\">",
            "<!-- leading comment -->",
        ] {
            let src = format!("{prefix}{}", doc("<div/>"));
            let dom = parse_xml_to_fast_dom(&src)
                .unwrap_or_else(|e| panic!("{prefix:?} should be stripped, got {e}"));
            assert_eq!(nodes(&dom).len(), expected, "{prefix:?} changed the arena");
        }
    }

    #[test]
    fn parse_xml_to_fast_dom_preserves_unicode_text() {
        for payload in [
            "日本語",
            "🙂🙂🙂🙂",
            "e\u{301}\u{302}\u{303}",
            "\u{200B}\u{FEFF}mid-string BOM",
            "ﷺ",
        ] {
            let dom = parse_xml_to_fast_dom(&doc(payload))
                .unwrap_or_else(|e| panic!("{payload:?} should parse, got {e}"));
            let n = nodes(&dom);
            assert_eq!(n.len(), 3, "{payload:?}");
            assert_eq!(text_of(&n[2]).as_deref(), Some(payload));
        }
    }

    #[test]
    fn parse_xml_to_fast_dom_treats_numeric_looking_documents_as_text() {
        // Boundary numeric strings are markup content here, not numbers: they
        // must survive verbatim rather than being coerced or rejected.
        for payload in [
            "0",
            "-0",
            "9223372036854775807",
            "-9223372036854775808",
            "18446744073709551616",
            "1e309",
            "-1e-309",
            "NaN",
            "inf",
            "-inf",
        ] {
            let dom = parse_xml_to_fast_dom(&doc(payload))
                .unwrap_or_else(|e| panic!("{payload:?} should parse, got {e}"));
            assert_eq!(text_of(&nodes(&dom)[2]).as_deref(), Some(payload));
        }
    }

    // ------------------------------------------------------------------
    // parse_xml_to_styled_dom
    // ------------------------------------------------------------------

    #[test]
    fn icon_tag_yields_unnamed_icon_nodes_with_spec_text_children() {
        use azul_core::dom::NodeType;

        // The tokenizer stays fully generic: `<icon>spec</icon>` is an
        // un-named Icon node with its spec preserved as a text child.
        // The RESOLVER consumes the spec (see the resolution test below).
        let fast =
            parse_xml_to_fast_dom("<html><body><icon> content_copy </icon><p>x</p></body></html>")
                .expect("icon markup must parse");

        let icon_names: Vec<&str> = nodes(&fast)
            .iter()
            .filter_map(|nd| match nd.get_node_type() {
                NodeType::Icon(name) => Some(name.as_ref().as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            icon_names,
            vec![""],
            "the builder must not interpret the spec"
        );

        let spec_preserved = nodes(&fast).iter().any(|nd| {
            matches!(nd.get_node_type(), NodeType::Text(t) if t.as_ref().as_str().trim() == "content_copy")
        });
        assert!(
            spec_preserved,
            "the spec text child must be preserved for the resolver"
        );
    }

    #[test]
    fn icon_resolution_consumes_the_spec_text_like_a_ligature_font() {
        use azul_core::{
            dom::NodeType,
            icon::{IconProviderHandle, SharedIconProvider},
            refany::{OptionRefAny, RefAny},
            styled_dom::StyledDom,
        };
        use azul_css::system::SystemStyle;

        // Marker resolver: registered icons become a Text("RESOLVED") node,
        // unregistered ones become Text("MISSING") — enough to observe both
        // the spec-derived LOOKUP and the replacement without a real font.
        extern "C" fn marker_resolver(
            data: OptionRefAny,
            original: &azul_core::dom::NodeData,
            _: &SystemStyle,
        ) -> Dom {
            let marker = if data.is_some() {
                "RESOLVED"
            } else {
                "MISSING"
            };
            let mut replacement = Dom::create_div();
            replacement.root = original.clone();
            replacement
                .root
                .set_node_type(NodeType::Text(azul_css::css::BoxOrStatic::heap(
                    marker.into(),
                )));
            replacement
        }

        let mut provider = IconProviderHandle::with_resolver(marker_resolver);
        provider.register_icon("testpack", "content_copy", RefAny::new(1u8));
        let provider = SharedIconProvider::from_handle(provider);

        // Both the bare-name spec and the pack-qualified fallback-list spec
        // (`missing:x` first — must fall through to `testpack:content_copy`).
        let styled = parse_xml_to_styled_dom_resolving_icons(
            "<html><body><icon> content_copy </icon><icon>missing:x, \
             testpack:CONTENT_COPY</icon><icon>unknown_icon</icon></body></html>",
            &provider,
            &SystemStyle::default(),
        )
        .expect("icon markup must cascade");

        let texts: Vec<String> = styled
            .node_data
            .as_ref()
            .iter()
            .filter_map(|nd| match nd.get_node_type() {
                NodeType::Text(t) => Some(t.as_ref().as_str().to_string()),
                _ => None,
            })
            .collect();

        let resolved = texts.iter().filter(|t| t.as_str() == "RESOLVED").count();
        let missing = texts.iter().filter(|t| t.as_str() == "MISSING").count();
        assert_eq!(
            resolved, 2,
            "bare + pack-qualified specs must both resolve: {texts:?}"
        );
        assert_eq!(
            missing, 1,
            "the unknown spec resolves to no data: {texts:?}"
        );

        // The spec text was consumed — it must not survive as renderable text.
        assert!(
            !texts
                .iter()
                .any(|t| t.contains("content_copy") || t.contains("unknown_icon")),
            "spec text children must be cleared after resolution: {texts:?}"
        );
    }

    #[test]
    fn parse_xml_to_styled_dom_accepts_empty_and_whitespace_only_input() {
        for s in ["", "   ", "\t\n", "\u{FEFF}"] {
            let styled = parse_xml_to_styled_dom(s)
                .unwrap_or_else(|e| panic!("{s:?} should cascade cleanly, got {e}"));
            assert!(styled.node_data.as_ref().is_empty(), "{s:?}");
        }
    }

    #[test]
    fn parse_xml_to_styled_dom_keeps_the_fast_dom_node_count() {
        for src in [
            doc("<div>hi</div>"),
            doc("<div><span>a</span><span>b</span></div>"),
            "<html><head><style>div { width: 10px; }</style></head><body><div/></body></html>"
                .to_string(),
        ] {
            let fast = parse_xml_to_fast_dom(&src).expect("fast path");
            let styled = parse_xml_to_styled_dom(&src).expect("styled path");
            assert_eq!(
                styled.node_data.as_ref().len(),
                nodes(&fast).len(),
                "the cascade must not add or drop nodes for {src:?}"
            );
            assert_eq!(
                styled.node_hierarchy.as_ref().len(),
                styled.node_data.as_ref().len()
            );
        }
    }

    #[test]
    fn parse_xml_to_styled_dom_is_deterministic_on_garbage() {
        for g in GARBAGE {
            let a = parse_xml_to_styled_dom(g);
            let b = parse_xml_to_styled_dom(g);
            assert_eq!(a.is_ok(), b.is_ok(), "{g:?} cascaded non-deterministically");
        }
    }

    // ------------------------------------------------------------------
    // dom_from_parsed_xml
    // ------------------------------------------------------------------

    #[test]
    fn a_fragment_becomes_a_document_and_a_broken_one_still_reports() {
        // BROWSER-LIKE: a fragment gets a synthesised `<html><body>` root,
        // so it renders as ITSELF rather than as the text "No <html> node
        // found as the root of the file" - which used to lay out and paint
        // like any other text, so a caller measuring pixels saw an error
        // message it never asked for.
        for root in [
            Vec::new(),
            vec![XmlNodeChild::Text("bare text".into())],
            vec![XmlNodeChild::Element(XmlNode::create("div"))],
            vec![XmlNodeChild::Element(XmlNode::create("svg"))],
        ] {
            let dom = dom_from_parsed_xml(Xml { root: root.into() });
            assert!(
                matches!(dom.root.get_node_type(), NodeType::Html),
                "a real document is rooted at <html>, got {:?} - a <body> root here would mean \
                 the ERROR Dom came back instead",
                dom.root.get_node_type()
            );
        }

        // An EXPLICIT `<html>` with no `<body>` is still an error Dom: the
        // author stated the structure, so a missing body is their mistake and
        // reporting it is more useful than guessing.
        let broken = dom_from_parsed_xml(Xml {
            root: vec![XmlNodeChild::Element(XmlNode::create("html"))].into(),
        });
        assert!(matches!(broken.root.get_node_type(), NodeType::Body));
        assert_eq!(broken.children.as_ref().len(), 1, "one label child");
    }

    #[test]
    fn dom_from_parsed_xml_builds_a_dom_for_a_minimal_document() {
        let body = XmlNode::create("body")
            .with_children(vec![XmlNodeChild::Element(XmlNode::create("div"))]);
        let html = XmlNode::create("html").with_children(vec![XmlNodeChild::Element(body)]);
        let dom = dom_from_parsed_xml(Xml {
            root: vec![XmlNodeChild::Element(html)].into(),
        });

        assert!(matches!(dom.root.get_node_type(), NodeType::Html));
        assert_eq!(dom.children.as_ref().len(), 1, "the <body> subtree");
    }

    /// Runs on a thread with an 8 MiB stack ON PURPOSE - see the twin
    /// `xml_node_to_dom_fast_deep_nesting_ok` in azul-core. libtest hands each
    /// test the platform default (2 MiB on Linux), which is smaller than any
    /// context this code really runs in; a debug-profile frame of the builder
    /// is big enough that the 512 the cap allows clear 2 MiB, so the
    /// dev-profile CI job aborted here with "has overflowed its stack" while
    /// the product itself was fine on its 8 MiB main thread.
    #[test]
    fn dom_from_parsed_xml_caps_recursion_on_deeply_nested_input() {
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                // MAX_XML_NESTING_DEPTH is 512; past it the builder drops children
                // instead of blowing the native stack.
                const DEPTH: usize = 550;
                let mut node = XmlNode::create("div");
                for _ in 0..DEPTH {
                    node = XmlNode::create("div").with_children(vec![XmlNodeChild::Element(node)]);
                }
                let body = XmlNode::create("body").with_children(vec![XmlNodeChild::Element(node)]);
                let html = XmlNode::create("html").with_children(vec![XmlNodeChild::Element(body)]);

                let dom = dom_from_parsed_xml(Xml {
                    root: vec![XmlNodeChild::Element(html)].into(),
                });
                assert!(matches!(dom.root.get_node_type(), NodeType::Html));
            })
            .expect("spawn deep-nesting probe")
            .join()
            .expect("deep DOM build must not overflow the stack");
    }

    // ------------------------------------------------------------------
    // domxml_from_str / domxml_from_file / DomXmlExt
    // ------------------------------------------------------------------

    #[cfg(feature = "xml")]
    #[test]
    fn domxml_from_str_never_fails() {
        let map = ComponentMap::with_builtin();
        let mut cases: Vec<String> = GARBAGE.iter().map(|s| (*s).to_string()).collect();
        cases.push(String::new());
        cases.push("   ".to_string());
        cases.push("<svg".to_string());
        cases.push("<?xml".to_string());
        cases.push(doc("<div>hi</div>"));

        for src in cases {
            let dom_xml = domxml_from_str(&src, &map);
            assert!(
                !dom_xml.parsed_dom.node_data.as_ref().is_empty(),
                "{src:?} produced an empty StyledDom; errors must render as a label"
            );
        }
    }

    #[cfg(all(feature = "std", feature = "xml"))]
    #[test]
    fn domxml_from_file_renders_io_errors_as_a_dom() {
        let map = ComponentMap::with_builtin();
        for path in [
            "/nonexistent-azul-autotest-dir/definitely-not-here.xml",
            "",
            "/",
            "/proc/self/nonexistent-🙂",
        ] {
            let dom_xml = domxml_from_file(path, &map);
            assert!(
                !dom_xml.parsed_dom.node_data.as_ref().is_empty(),
                "{path:?} must render the io::Error as a label, not fail"
            );
        }
    }

    #[cfg(feature = "xml")]
    #[test]
    fn dom_xml_ext_matches_domxml_from_str() {
        let map = ComponentMap::with_builtin();
        let valid = doc("<div>hi</div>");
        for src in ["", "<svg", valid.as_str()] {
            let via_ext = <Dom as DomXmlExt>::from_xml_string(src);
            let via_fn = domxml_from_str(src, &map).parsed_dom;
            assert_eq!(
                via_ext.node_data.as_ref().len(),
                via_fn.node_data.as_ref().len(),
                "the extension trait must be a pure delegation for {src:?}"
            );
        }
    }

    // ------------------------------------------------------------------
    // peak_rss_bytes
    // ------------------------------------------------------------------

    #[test]
    fn peak_rss_bytes_never_panics_and_never_goes_backwards() {
        let a = peak_rss_bytes();
        let _ballast = "x".repeat(4 * 1024 * 1024);
        let b = peak_rss_bytes();

        #[cfg(all(unix, feature = "probe"))]
        assert!(
            b >= a,
            "ru_maxrss is a high-water mark and must never decrease ({a} -> {b})"
        );
        #[cfg(not(all(unix, feature = "probe")))]
        assert_eq!(
            (a, b),
            (0, 0),
            "without the probe feature the stub must be a constant 0"
        );
    }

    // ------------------------------------------------------------------
    // translate_* (xmlparser / roxmltree -> FFI-stable azul types)
    // ------------------------------------------------------------------

    #[cfg(feature = "xml")]
    #[test]
    fn translate_textpos_round_trips_boundary_values() {
        for (row, col) in [
            (0, 0),
            (1, 1),
            (0, u32::MAX),
            (u32::MAX, 0),
            (u32::MAX, u32::MAX),
        ] {
            let expected = XmlTextPos { row, col };
            assert_eq!(
                translate_xmlparser_textpos(xmlparser::TextPos::new(row, col)),
                expected
            );
            assert_eq!(
                translate_roxml_textpos(roxmltree::TextPos::new(row, col)),
                expected
            );
        }
    }

    #[cfg(feature = "xml")]
    #[test]
    fn translate_roxmltree_expandedname_preserves_name_and_namespace() {
        let plain: roxmltree::ExpandedName<'_, '_> = "rect".into();
        let out = translate_roxmltree_expandedname(plain);
        assert_eq!(out.local_name.as_str(), "rect");
        assert!(out.namespace.as_ref().is_none());

        let ns: roxmltree::ExpandedName<'_, '_> = ("http://www.w3.org/2000/svg", "rect").into();
        let out = translate_roxmltree_expandedname(ns);
        assert_eq!(out.local_name.as_str(), "rect");
        assert_eq!(
            out.namespace.as_ref().map(AzString::as_str),
            Some("http://www.w3.org/2000/svg")
        );

        // Degenerate names must survive untouched, not be normalised away.
        for name in ["", " ", "日本語-🙂", "a:b"] {
            let e: roxmltree::ExpandedName<'_, '_> = name.into();
            assert_eq!(
                translate_roxmltree_expandedname(e).local_name.as_str(),
                name
            );
        }
        let empty_ns: roxmltree::ExpandedName<'_, '_> = ("", "x").into();
        assert_eq!(
            translate_roxmltree_expandedname(empty_ns)
                .namespace
                .as_ref()
                .map(AzString::as_str),
            Some(""),
            "an empty namespace URI is Some(\"\"), not None"
        );
    }

    #[cfg(feature = "xml")]
    #[test]
    fn translate_roxmltree_attribute_preserves_name_and_namespace() {
        let rdoc =
            roxmltree::Document::parse(r#"<e xmlns:x="urn:x" x:a="1" b="2"/>"#).expect("valid XML");
        let attrs: Vec<XmlQualifiedName> = rdoc
            .root_element()
            .attributes()
            .map(translate_roxmltree_attribute)
            .collect();

        assert_eq!(attrs.len(), 2, "xmlns declarations are not attributes");
        let a = attrs
            .iter()
            .find(|q| q.local_name.as_str() == "a")
            .expect("x:a");
        assert_eq!(a.namespace.as_ref().map(AzString::as_str), Some("urn:x"));
        let b = attrs
            .iter()
            .find(|q| q.local_name.as_str() == "b")
            .expect("b");
        assert!(
            b.namespace.as_ref().is_none(),
            "an unprefixed attribute has no namespace"
        );
    }

    #[cfg(feature = "xml")]
    #[test]
    fn translate_xmlparser_streamerror_maps_every_variant() {
        use xmlparser::StreamError as Se;

        let p = xmlparser::TextPos::new(3, 7);
        let x = XmlTextPos { row: 3, col: 7 };

        assert_eq!(
            translate_xmlparser_streamerror(Se::UnexpectedEndOfStream),
            XmlStreamError::UnexpectedEndOfStream
        );
        assert_eq!(
            translate_xmlparser_streamerror(Se::InvalidName),
            XmlStreamError::InvalidName
        );
        assert_eq!(
            translate_xmlparser_streamerror(Se::InvalidReference),
            XmlStreamError::InvalidReference
        );
        assert_eq!(
            translate_xmlparser_streamerror(Se::InvalidExternalID),
            XmlStreamError::InvalidExternalID
        );
        assert_eq!(
            translate_xmlparser_streamerror(Se::InvalidCommentData),
            XmlStreamError::InvalidCommentData
        );
        assert_eq!(
            translate_xmlparser_streamerror(Se::InvalidCommentEnd),
            XmlStreamError::InvalidCommentEnd
        );
        assert_eq!(
            translate_xmlparser_streamerror(Se::InvalidCharacterData),
            XmlStreamError::InvalidCharacterData
        );
        // Astral char -> u32 (the FFI-stable representation) without loss.
        assert_eq!(
            translate_xmlparser_streamerror(Se::NonXmlChar('\u{1F600}', p)),
            XmlStreamError::NonXmlChar(NonXmlCharError {
                ch: 0x1F600,
                pos: x
            })
        );
        assert_eq!(
            translate_xmlparser_streamerror(Se::InvalidQuote(b'`', p)),
            XmlStreamError::InvalidQuote(InvalidQuoteError { got: b'`', pos: x })
        );
        assert_eq!(
            translate_xmlparser_streamerror(Se::InvalidSpace(b'\t', p)),
            XmlStreamError::InvalidSpace(InvalidSpaceError { got: b'\t', pos: x })
        );
        assert_eq!(
            translate_xmlparser_streamerror(Se::InvalidString("?>", p)),
            XmlStreamError::InvalidString(InvalidStringError {
                got: "?>".into(),
                pos: x
            })
        );
        // NOTE: xmlparser documents InvalidChar/InvalidCharMultiple as
        // (actual, expected, pos), but the translation stores the first field
        // as `expected` and the second as `got` — i.e. the two are swapped.
        // Characterised here rather than "fixed" in the test: it only affects
        // error-message wording, and pinning it makes the swap visible if the
        // mapping is ever corrected.
        assert_eq!(
            translate_xmlparser_streamerror(Se::InvalidChar(b'a', b'b', p)),
            XmlStreamError::InvalidChar(InvalidCharError {
                expected: b'a',
                got: b'b',
                pos: x
            })
        );
        assert_eq!(
            translate_xmlparser_streamerror(Se::InvalidCharMultiple(b'a', &b"xy"[..], p)),
            XmlStreamError::InvalidCharMultiple(InvalidCharMultipleError {
                expected: b'a',
                got: vec![b'x', b'y'].into(),
                pos: x
            })
        );
    }

    #[cfg(feature = "xml")]
    #[test]
    fn translate_xmlparser_error_maps_every_variant() {
        use xmlparser::{Error as Xe, StreamError as Se};

        let p = xmlparser::TextPos::new(9, 4);
        let x = XmlTextPos { row: 9, col: 4 };
        let te = XmlTextError {
            stream_error: XmlStreamError::InvalidName,
            pos: x,
        };

        assert_eq!(
            translate_xmlparser_error(Xe::InvalidDeclaration(Se::InvalidName, p)),
            XmlParseError::InvalidDeclaration(te.clone())
        );
        assert_eq!(
            translate_xmlparser_error(Xe::InvalidComment(Se::InvalidName, p)),
            XmlParseError::InvalidComment(te.clone())
        );
        assert_eq!(
            translate_xmlparser_error(Xe::InvalidPI(Se::InvalidName, p)),
            XmlParseError::InvalidPI(te.clone())
        );
        assert_eq!(
            translate_xmlparser_error(Xe::InvalidDoctype(Se::InvalidName, p)),
            XmlParseError::InvalidDoctype(te.clone())
        );
        assert_eq!(
            translate_xmlparser_error(Xe::InvalidEntity(Se::InvalidName, p)),
            XmlParseError::InvalidEntity(te.clone())
        );
        assert_eq!(
            translate_xmlparser_error(Xe::InvalidElement(Se::InvalidName, p)),
            XmlParseError::InvalidElement(te.clone())
        );
        assert_eq!(
            translate_xmlparser_error(Xe::InvalidAttribute(Se::InvalidName, p)),
            XmlParseError::InvalidAttribute(te.clone())
        );
        assert_eq!(
            translate_xmlparser_error(Xe::InvalidCdata(Se::InvalidName, p)),
            XmlParseError::InvalidCdata(te.clone())
        );
        assert_eq!(
            translate_xmlparser_error(Xe::InvalidCharData(Se::InvalidName, p)),
            XmlParseError::InvalidCharData(te)
        );
        assert_eq!(
            translate_xmlparser_error(Xe::UnknownToken(p)),
            XmlParseError::UnknownToken(x)
        );
    }

    #[cfg(feature = "xml")]
    #[test]
    fn translate_roxmltree_error_maps_every_variant() {
        use roxmltree::Error as Re;

        let p = roxmltree::TextPos::new(2, 5);
        let x = XmlTextPos { row: 2, col: 5 };

        assert_eq!(
            translate_roxmltree_error(Re::InvalidXmlPrefixUri(p)),
            XmlError::InvalidXmlPrefixUri(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::UnexpectedXmlUri(p)),
            XmlError::UnexpectedXmlUri(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::UnexpectedXmlnsUri(p)),
            XmlError::UnexpectedXmlnsUri(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::InvalidElementNamePrefix(p)),
            XmlError::InvalidElementNamePrefix(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::DuplicatedNamespace(String::from("ns"), p)),
            XmlError::DuplicatedNamespace(DuplicatedNamespaceError {
                ns: "ns".into(),
                pos: x
            })
        );
        assert_eq!(
            translate_roxmltree_error(Re::UnknownNamespace(String::from("ns"), p)),
            XmlError::UnknownNamespace(UnknownNamespaceError {
                ns: "ns".into(),
                pos: x
            })
        );
        assert_eq!(
            translate_roxmltree_error(Re::UnexpectedCloseTag(
                String::from("a"),
                String::from("b"),
                p
            )),
            XmlError::UnexpectedCloseTag(UnexpectedCloseTagError {
                expected: "a".into(),
                actual: "b".into(),
                pos: x
            })
        );
        assert_eq!(
            translate_roxmltree_error(Re::UnexpectedEntityCloseTag(p)),
            XmlError::UnexpectedEntityCloseTag(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::UnknownEntityReference(String::from("e"), p)),
            XmlError::UnknownEntityReference(UnknownEntityReferenceError {
                entity: "e".into(),
                pos: x
            })
        );
        assert_eq!(
            translate_roxmltree_error(Re::MalformedEntityReference(p)),
            XmlError::MalformedEntityReference(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::EntityReferenceLoop(p)),
            XmlError::EntityReferenceLoop(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::InvalidAttributeValue(p)),
            XmlError::InvalidAttributeValue(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::DuplicatedAttribute(String::from("a"), p)),
            XmlError::DuplicatedAttribute(DuplicatedAttributeError {
                attribute: "a".into(),
                pos: x
            })
        );
        assert_eq!(
            translate_roxmltree_error(Re::NoRootNode),
            XmlError::NoRootNode
        );
        assert_eq!(
            translate_roxmltree_error(Re::DtdDetected),
            XmlError::DtdDetected
        );
        assert_eq!(
            translate_roxmltree_error(Re::UnclosedRootNode),
            XmlError::UnclosedRootNode
        );
        assert_eq!(
            translate_roxmltree_error(Re::UnexpectedDeclaration(p)),
            XmlError::UnexpectedDeclaration(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::NodesLimitReached),
            XmlError::NodesLimitReached
        );
        assert_eq!(
            translate_roxmltree_error(Re::AttributesLimitReached),
            XmlError::AttributesLimitReached
        );
        assert_eq!(
            translate_roxmltree_error(Re::NamespacesLimitReached),
            XmlError::NamespacesLimitReached
        );
        assert_eq!(
            translate_roxmltree_error(Re::InvalidName(p)),
            XmlError::InvalidName(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::NonXmlChar('\u{0}', p)),
            XmlError::NonXmlChar(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::InvalidChar(b'a', b'b', p)),
            XmlError::InvalidChar(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::InvalidChar2("ab", b'c', p)),
            XmlError::InvalidChar2(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::InvalidString("s", p)),
            XmlError::InvalidString(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::InvalidExternalID(p)),
            XmlError::InvalidExternalID(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::InvalidComment(p)),
            XmlError::InvalidComment(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::InvalidCharacterData(p)),
            XmlError::InvalidCharacterData(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::UnknownToken(p)),
            XmlError::UnknownToken(x)
        );
        assert_eq!(
            translate_roxmltree_error(Re::UnexpectedEndOfStream),
            XmlError::UnexpectedEndOfStream
        );
        // roxmltree 0.21's EntityResolver is folded into UnknownEntityReference.
        assert_eq!(
            translate_roxmltree_error(Re::EntityResolver(p, String::from("e"))),
            XmlError::UnknownEntityReference(UnknownEntityReferenceError {
                entity: "e".into(),
                pos: x
            })
        );
    }
}
