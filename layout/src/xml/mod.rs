//! XML/HTML parsing module for the Azul toolkit.
//!
//! Provides two parsing paths:
//! - `parse_xml_string`: builds an `XmlNode` tree (used by `domxml_from_str`)
//! - `parse_xml_to_fast_dom_with_css`: builds an arena-based `FastDom` directly
//!   from XML tokens (used by `parse_xml_to_styled_dom`)
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

/// Decodes XML/HTML entities in a string.
/// Handles standard XML entities: &lt; &gt; &amp; &apos; &quot;
/// and numeric character references: &#60; &#x3C;
/// Returns `Cow::Borrowed` when no entities are found (zero-alloc fast path).
fn decode_xml_entities(s: &str) -> std::borrow::Cow<'_, str> {
    // Fast path: if no ampersand, no entities to decode
    if !s.contains('&') {
        return std::borrow::Cow::Borrowed(s);
    }
    decode_xml_entities_slow(s)
}

fn decode_xml_entities_slow(s: &str) -> std::borrow::Cow<'_, str> {
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

    std::borrow::Cow::Owned(result)
}

pub use azul_core::xml::*;
use azul_core::{dom::Dom, impl_from, styled_dom::StyledDom, window::StringPairVec};
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

/// Create a Dom (with CSS attached but not applied) from an already-parsed Xml structure.
///
/// Returns an unstyled `Dom` suitable for use in layout callbacks (which return `Dom`,
/// not `StyledDom`). The CSS from `<style>` tags is attached to the `Dom.css` field
/// and will be applied during the cascade pass.
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
/// # Errors
///
/// Returns an `XmlError` if the XML cannot be parsed.
pub fn parse_xml_to_styled_dom_resolving_icons(
    xml: &str,
    provider: &azul_core::icon::SharedIconProvider,
    system_style: &azul_css::system::SystemStyle,
) -> Result<StyledDom, XmlError> {
    let parsed = parse_xml(xml)?;
    let mut dom = dom_from_parsed_xml(parsed);
    azul_core::icon::resolve_icons_in_dom(&mut dom, provider, system_style);
    Ok(StyledDom::create_from_dom(dom))
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
    static MEM_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let mem_on = *MEM_ENABLED.get_or_init(azul_core::profile::memory_enabled);

    let rss0 = if mem_on { peak_rss_bytes() } else { 0 };
    let (mut fast_dom, css) = parse_xml_to_fast_dom_with_css(xml)?;
    if mem_on {
        let rss1 = peak_rss_bytes();
        eprintln!(
            "[XML] tokenize+fast_dom       : +{:.2} MiB",
            (rss1.saturating_sub(rss0)) as f64 / 1024.0 / 1024.0,
        );
    }

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

    Ok(styled)
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

/// Internal: parse XML into `FastDom` + collected CSS stylesheets.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded layout/render numeric cast
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn parse_xml_to_fast_dom_with_css(
    xml: &str,
) -> Result<(azul_core::dom::FastDom, Vec<Css>), XmlError> {
    use azul_core::dom::{IdOrClass, NodeData, NodeType, TabIndex};
    use azul_core::xml::CompactDomBuilder;
    use xmlparser::{
        ElementEnd::{Close, Empty, Open},
        Token::{Attribute, ElementEnd, ElementStart, Text},
        Tokenizer,
    };

    const ESTIMATED_BYTES_PER_NODE: usize = 20;

    const VOID_ELEMENTS: &[&str] = &[
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
        "source", "track", "wbr",
    ];

    // Lowercase `src` into `dst`, reusing `dst`'s existing capacity.
    // Zero-alloc when dst's capacity is already ≥ src.len() AND no uppercase
    // conversion is needed (the happy path for HTML5 where tags are lowercase).
    fn lowercase_into(dst: &mut String, src: &str) {
        dst.clear();
        if src.bytes().all(|b| !b.is_ascii_uppercase()) {
            dst.push_str(src);
        } else {
            dst.reserve(src.len());
            for b in src.bytes() {
                dst.push(b.to_ascii_lowercase() as char);
            }
        }
    }

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

    let tokenizer = Tokenizer::from_fragment(xml, 0..xml.len());

    let estimated_nodes = xml.len() / ESTIMATED_BYTES_PER_NODE;
    let mut builder = CompactDomBuilder::with_capacity(estimated_nodes);
    let mut collected_css: Vec<Css> = Vec::new();
    let mut inside_style_tag = false;
    let mut style_text = String::new();
    // Track <head> depth: skip DOM nodes inside <head> (still collect <style> CSS).
    // This ensures the FastDom contains only <html><body>... as the layout engine expects.
    let mut head_depth: usize = 0;

    // Temporary storage for current element's attributes
    let mut current_tag: String = String::new();
    let mut current_attrs: Vec<(String, String)> = Vec::new();
    let mut pending_open = false;

    // Pre-compute the CSS key map once (used for style= attribute parsing)
    let css_key_map = azul_css::props::property::get_css_key_map();

    // One bump arena for every AzString produced during this parse —
    // id/class tokens, text nodes, etc. Replaces ~1k small heap allocs
    // with a handful of 64 KiB chunks. Each AzString carries its own
    // Arc reference to the arena, so the arena survives until the last
    // string is dropped (typically when the StyledDom is dropped).
    let mut str_arena = azul_css::corety::StringArena::new();

    // Finalize the pending open element: create NodeData from tag + attrs, push to builder
    // tag is already lowercase
    let finalize_open =
        |builder: &mut CompactDomBuilder,
         str_arena: &mut azul_css::corety::StringArena,
         tag: &str,
         attrs: &[(String, String)],
         css_key_map: &azul_css::props::property::CssKeyMap| {
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

            // Apply attributes — build AttributeTypeVec directly (avoids the
            // clone + retain dance in set_ids_and_classes for fresh NodeData).
            let mut attr_vec: Vec<azul_core::dom::AttributeType> = Vec::new();
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
                match key.as_str() {
                    "id" => {
                        for id in value.split_whitespace() {
                            attr_vec.push(azul_core::dom::AttributeType::Id(str_arena.intern(id)));
                        }
                    }
                    "class" => {
                        for class in value.split_whitespace() {
                            attr_vec.push(azul_core::dom::AttributeType::Class(
                                str_arena.intern(class),
                            ));
                        }
                    }
                    "focusable" => {
                        if let Some(f) = parse_bool(value.as_str()) {
                            nd.set_tab_index(if f {
                                TabIndex::Auto
                            } else {
                                TabIndex::NoKeyboardFocus
                            });
                        }
                    }
                    "tabindex" => {
                        if let Ok(ti) = value.parse::<isize>() {
                            match ti {
                                0 => nd.set_tab_index(TabIndex::Auto),
                                i if i > 0 => {
                                    nd.set_tab_index(TabIndex::OverrideInParent(i as u32))
                                }
                                _ => nd.set_tab_index(TabIndex::NoKeyboardFocus),
                            }
                        }
                    }
                    "style" => {
                        let mut css_attrs = Vec::new();
                        for s in value.split(';') {
                            let mut s = s.split(':');
                            let Some(key) = s.next() else { continue };
                            let Some(val) = s.next() else { continue };
                            // Called for its side effect (writes parsed props into
                            // `css_attrs`); the returned value is intentionally discarded.
                            drop(azul_css::parser2::parse_css_declaration(
                                key.trim(),
                                val.trim(),
                                azul_css::parser2::ErrorLocationRange::default(),
                                css_key_map,
                                &mut Vec::new(),
                                &mut css_attrs,
                            ));
                        }
                        let props = css_attrs
                            .into_iter()
                            .filter_map(|s| {
                                use azul_css::css::CssDeclaration;
                                use azul_css::dynamic_selector::CssPropertyWithConditions;
                                match s {
                                    CssDeclaration::Static(s) => {
                                        Some(CssPropertyWithConditions::simple(s))
                                    }
                                    CssDeclaration::Dynamic(_) => None,
                                }
                            })
                            .collect::<Vec<_>>();
                        if !props.is_empty() {
                            nd.set_css_props(props.into());
                        }
                    }
                    "contenteditable" => {
                        match parse_bool(value.as_str()) {
                            Some(true) => nd.set_contenteditable(true),
                            // An explicit `false` is NOT "no attribute": inside an
                            // editable host it walls its subtree off (HTML's
                            // inheritance rule, `is_node_contenteditable_inherited`)
                            // and keeps that subtree out of the host's edit buffer
                            // and out of the block the edit is shaped into. Dropped
                            // here, a mounted `<p contenteditable="false">` island
                            // behaved like any other child — the Rust API's
                            // `with_attribute(ContentEditable(false))` and the HTML
                            // loader disagreed on the same document.
                            Some(false) => {
                                attr_vec.push(azul_core::dom::AttributeType::ContentEditable(false))
                            }
                            None => {}
                        }
                    }
                    _ => {}
                }
            }
            if !attr_vec.is_empty() {
                nd.set_attributes(attr_vec.into());
            }
            // Write the parsed popup config back into the node's payload.
            if let Some(cfg) = transient_cfg {
                nd.set_node_type(NodeType::TransientWindow(cfg));
            }

            builder.open_node(nd);
        };

    let mut last_was_void = false;
    let mut tag_stack: Vec<String> = Vec::new(); // for matching close tags

    for token in tokenizer {
        let token = token.map_err(|e| XmlError::ParserError(translate_xmlparser_error(e)))?;
        match token {
            ElementStart { local, .. } => {
                // Flush any pending open element
                if pending_open {
                    let is_void = VOID_ELEMENTS.contains(&current_tag.as_str());
                    if current_tag == "head" {
                        head_depth += 1;
                    }
                    if head_depth == 0 {
                        finalize_open(
                            &mut builder,
                            &mut str_arena,
                            &current_tag,
                            &current_attrs,
                            &css_key_map,
                        );
                        if is_void {
                            builder.close_node();
                        }
                    }
                    if !is_void {
                        tag_stack.push(core::mem::take(&mut current_tag));
                    }
                }

                // Reuse the current_tag buffer — avoids ~1023 fresh String
                // allocations per parse (one per ElementStart).
                lowercase_into(&mut current_tag, local.as_str());
                current_attrs.clear();
                pending_open = true;
                last_was_void = VOID_ELEMENTS.contains(&current_tag.as_str());
            }
            Attribute { local, value, .. } => {
                // decode_xml_entities returns Cow::Borrowed when no entities
                // are present (the common case), so `.into_owned()` is the
                // only fresh allocation here. The key is copied via
                // `to_string()` because we can't hold a borrow across token
                // iterations. TODO: when we switch current_attrs to
                // Vec<(&str, Cow<str>)> this becomes zero-alloc for the key.
                current_attrs.push((
                    local.to_string(),
                    decode_xml_entities(value.as_str()).into_owned(),
                ));
            }
            ElementEnd { end: Open, .. } => {
                if pending_open {
                    let is_void = VOID_ELEMENTS.contains(&current_tag.as_str());
                    if current_tag == "style" {
                        inside_style_tag = true;
                        style_text.clear();
                    }
                    if current_tag == "head" {
                        head_depth += 1;
                    }
                    if head_depth == 0 {
                        finalize_open(
                            &mut builder,
                            &mut str_arena,
                            &current_tag,
                            &current_attrs,
                            &css_key_map,
                        );
                        if is_void {
                            builder.close_node();
                        }
                    }
                    if !is_void {
                        // Use take() instead of clone() — after pending_open=false,
                        // current_tag is not read again until the next ElementStart
                        // reassigns it via lowercase_into.
                        tag_stack.push(core::mem::take(&mut current_tag));
                    }
                    pending_open = false;
                }
            }
            ElementEnd { end: Empty, .. } => {
                // Self-closing element: open + immediately close
                if pending_open {
                    if current_tag == "head" {
                        head_depth += 1;
                    }
                    if head_depth == 0 {
                        finalize_open(
                            &mut builder,
                            &mut str_arena,
                            &current_tag,
                            &current_attrs,
                            &css_key_map,
                        );
                        builder.close_node();
                    }
                    if current_tag == "head" && head_depth > 0 {
                        head_depth -= 1;
                    }
                    pending_open = false;
                }
            }
            ElementEnd {
                end: Close(_, close_value),
                ..
            } => {
                if pending_open {
                    let is_void = VOID_ELEMENTS.contains(&current_tag.as_str());
                    if current_tag == "head" {
                        head_depth += 1;
                    }
                    if head_depth == 0 {
                        finalize_open(
                            &mut builder,
                            &mut str_arena,
                            &current_tag,
                            &current_attrs,
                            &css_key_map,
                        );
                        if is_void {
                            builder.close_node();
                        }
                    }
                    if !is_void {
                        tag_stack.push(core::mem::take(&mut current_tag));
                    }
                    pending_open = false;
                }

                let close_lower = close_value.as_str().to_ascii_lowercase();
                let close_str = close_lower.as_str();
                if VOID_ELEMENTS.contains(&close_str) {
                    continue;
                }

                // If closing a <style> tag, parse collected CSS
                if close_str == "style" && inside_style_tag {
                    if !style_text.is_empty() {
                        let parsed_css = Css::from_string(core::mem::take(&mut style_text).into());
                        collected_css.push(parsed_css);
                    }
                    inside_style_tag = false;
                }

                // Pop until we find matching tag
                while let Some(top) = tag_stack.last() {
                    let is_match = top == close_str;
                    let was_head = top == "head";
                    // Pop this tag (unconditionally auto-close mismatched tags)
                    let popped = tag_stack.pop().unwrap();
                    if popped == "head" && head_depth > 0 {
                        head_depth -= 1;
                    }
                    if head_depth == 0 && !was_head {
                        builder.close_node();
                    }
                    if is_match {
                        break;
                    }
                }
            }
            Text { text } => {
                if pending_open {
                    let is_void = VOID_ELEMENTS.contains(&current_tag.as_str());
                    if current_tag == "style" {
                        inside_style_tag = true;
                        style_text.clear();
                    }
                    if current_tag == "head" {
                        head_depth += 1;
                    }
                    if head_depth == 0 {
                        finalize_open(
                            &mut builder,
                            &mut str_arena,
                            &current_tag,
                            &current_attrs,
                            &css_key_map,
                        );
                        if is_void {
                            builder.close_node();
                        }
                    }
                    if !is_void {
                        tag_stack.push(current_tag.clone());
                    }
                    pending_open = false;
                }

                let text_str = text.as_str();
                if !text_str.is_empty() {
                    if inside_style_tag {
                        style_text.push_str(text_str);
                    } else if head_depth == 0 {
                        // Skip whitespace-only text at <html> level (between </head> and <body>)
                        // but keep whitespace inside <body> (it's significant for inline layout)
                        let inside_body = tag_stack.iter().any(|t| t == "body");
                        if inside_body || !text_str.trim().is_empty() {
                            let decoded = decode_xml_entities(text_str);
                            builder.add_leaf(
                                NodeData::create_text_do_not_use_without_block_level_wrapper(
                                    str_arena.intern(&decoded),
                                ),
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Close any remaining open elements
    if pending_open {
        finalize_open(
            &mut builder,
            &mut str_arena,
            &current_tag,
            &current_attrs,
            &css_key_map,
        );
    }
    while tag_stack.pop().is_some() {
        builder.close_node();
    }

    // Drop the arena handle explicitly. AzStrings already embedded in
    // the FastDom keep the backing bytes alive via their cloned Arc refs.
    drop(str_arena);

    Ok((builder.finish(), collected_css))
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
#[cfg(feature = "xml")]
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Errors
///
/// Returns an `XmlError` if the XML cannot be parsed.
pub fn parse_xml_string(xml: &str) -> Result<Vec<XmlNodeChild>, XmlError> {
    use xmlparser::{
        ElementEnd::{Close, Empty},
        Token::{Attribute, ElementEnd, ElementStart, Text},
        Tokenizer,
    };

    use self::XmlParseError::*;

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

    let mut root_node = XmlNode::default();

    // Strip UTF-8 BOM if present (some W3C test files have it)
    let xml = xml.strip_prefix('\u{FEFF}').unwrap_or(xml);

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

    let tokenizer = Tokenizer::from_fragment(xml, 0..xml.len());

    // OPTIMIZED: Use a stack of raw pointers to avoid O(n*d) traversal on every token.
    // This is safe because:
    // 1. All pointers point into `root_node` which is owned and not moved
    // 2. We never hold multiple mutable references simultaneously
    // 3. The stack is only used within this function
    let mut node_stack: Vec<*mut XmlNode> = vec![&raw mut root_node];

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
                    if let Some(XmlNodeChild::Element(ref mut new_child)) =
                        current_parent.children.as_mut().get_mut(children_len - 1)
                    {
                        node_stack.push(std::ptr::from_mut::<XmlNode>(new_child));
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
                        value: AzString::from(&*decode_xml_entities(value.as_str())),
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
                            .push(XmlNodeChild::Text(AzString::from(&*decoded_text)));
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

    // A well-formed document unwinds back to just the root sentinel. If an element was
    // left open (e.g. a bare "<svg" with no closing bracket, which the fragment tokenizer
    // yields as one ElementStart then cleanly ends), node_stack still holds it — reject
    // it instead of returning a "valid" partial tree.
    if node_stack.len() != 1 {
        return Err(XmlError::UnclosedRootNode);
    }

    Ok(root_node.children.into())
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
            "&", "&&", "&lt", "&#", "&#x", "&foo;", "&LT;", // entity table is case-sensitive
            "&Amp;", "& lt;", "a & b", "&;",
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
            "parse_xml_string panicked on {INPUT:?}: the DOCTYPE sniff slices \
             xml[..9] without an is_char_boundary check"
        );
        assert!(
            b.is_ok(),
            "parse_xml_to_fast_dom panicked on {INPUT:?}: same unchecked \
             xml[..9] slice"
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
        let src = "<html><head><title>T</title>\
                   <style>div { width: 10px; }</style></head>\
                   <body>x</body></html>";
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
            "<html><body>\
             <icon> content_copy </icon>\
             <icon>missing:x, testpack:CONTENT_COPY</icon>\
             <icon>unknown_icon</icon>\
             </body></html>",
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
    fn dom_from_parsed_xml_reports_errors_instead_of_panicking() {
        // No <html>/<body>: the documented behaviour is an error Dom, not a
        // panic and not an empty tree.
        for root in [
            Vec::new(),
            vec![XmlNodeChild::Text("bare text".into())],
            vec![XmlNodeChild::Element(XmlNode::create("div"))],
            vec![XmlNodeChild::Element(XmlNode::create("html"))],
        ] {
            let dom = dom_from_parsed_xml(Xml { root: root.into() });
            assert!(
                matches!(dom.root.get_node_type(), NodeType::Body),
                "the error Dom is rendered as a <body> with a label"
            );
            assert_eq!(dom.children.as_ref().len(), 1);
        }
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

    #[test]
    fn dom_from_parsed_xml_caps_recursion_on_deeply_nested_input() {
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
