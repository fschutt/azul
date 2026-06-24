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
#[must_use] pub fn domxml_from_str(xml: &str, component_map: &ComponentMap) -> DomXml {
    let error_css = Css::empty();

    let parsed = match parse_xml_string(xml) {
        Ok(parsed) => parsed,
        Err(e) => {
            return DomXml {
                parsed_dom: {
                    let mut dom = Dom::create_body()
                        .with_children(vec![Dom::create_text(format!("{e}"))].into());
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
                        .with_children(vec![Dom::create_text(format!("{e}"))].into());
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
#[must_use] pub fn dom_from_parsed_xml(xml: Xml) -> Dom {
    let component_map = ComponentMap::with_builtin();
    match str_to_dom_unstyled(xml.root.as_ref(), &component_map) {
        Ok(dom) => dom,
        Err(e) => Dom::create_body().with_children(vec![Dom::create_text(format!("{e}"))].into()),
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

/// Parse XML directly into `FastDom` + extracted CSS, ready for `StyledDom`.
#[allow(clippy::cast_precision_loss)] // bounded layout/render numeric cast
/// # Errors
///
/// Returns an `XmlError` if the XML cannot be parsed.
pub fn parse_xml_to_styled_dom(xml: &str) -> Result<StyledDom, XmlError> {
    // Optional per-phase RSS/timing breakdown.
    // Gated on AZ_MEM_BREAKDOWN=1 — prints
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
        let combined_css = Css::new(css.into_iter()
            .flat_map(|c| c.rules.into_library_owned_vec())
            .collect());
        fast_dom.css = vec![azul_core::dom::CssWithNodeId {
            node_id: 0, // global scope
            css: combined_css,
        }].into();
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
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return 0;
    }
    let ru = usage.ru_maxrss as u64;
    // macOS reports bytes, Linux reports KiB.
    #[cfg(target_os = "macos")]
    { ru }
    #[cfg(not(target_os = "macos"))]
    { ru.saturating_mul(1024) }
}

#[cfg(not(all(unix, feature = "probe")))]
const fn peak_rss_bytes() -> u64 {
    0
}

/// Internal: parse XML into `FastDom` + collected CSS stylesheets.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded layout/render numeric cast
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn parse_xml_to_fast_dom_with_css(xml: &str) -> Result<(azul_core::dom::FastDom, Vec<Css>), XmlError> {
    use xmlparser::{ElementEnd::{Open, Empty, Close}, Token::{ElementStart, Attribute, ElementEnd, Text}, Tokenizer};
    use azul_core::dom::{NodeData, NodeType, IdOrClass, TabIndex};
    use azul_core::xml::CompactDomBuilder;

    const ESTIMATED_BYTES_PER_NODE: usize = 20;

    const VOID_ELEMENTS: &[&str] = &[
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta",
        "param", "source", "track", "wbr",
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
    if xml.len() > 9 && xml[..9].to_ascii_lowercase().starts_with("<!doctype") {
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
    let finalize_open = |
        builder: &mut CompactDomBuilder,
        str_arena: &mut azul_css::corety::StringArena,
        tag: &str,
        attrs: &[(String, String)],
        css_key_map: &azul_css::props::property::CssKeyMap,
    | {
        let node_type = tag_to_node_type(tag);
        let mut nd = NodeData::create_node(node_type);

        // Apply attributes — build AttributeTypeVec directly (avoids the
        // clone + retain dance in set_ids_and_classes for fresh NodeData).
        let mut attr_vec: Vec<azul_core::dom::AttributeType> = Vec::new();
        for (key, value) in attrs {
            match key.as_str() {
                "id" => {
                    for id in value.split_whitespace() {
                        attr_vec.push(azul_core::dom::AttributeType::Id(str_arena.intern(id)));
                    }
                }
                "class" => {
                    for class in value.split_whitespace() {
                        attr_vec.push(azul_core::dom::AttributeType::Class(str_arena.intern(class)));
                    }
                }
                "focusable" => {
                    if let Some(f) = parse_bool(value.as_str()) {
                        nd.set_tab_index(if f { TabIndex::Auto } else { TabIndex::NoKeyboardFocus });
                    }
                }
                "tabindex" => {
                    if let Ok(ti) = value.parse::<isize>() {
                        match ti {
                            0 => nd.set_tab_index(TabIndex::Auto),
                            i if i > 0 => nd.set_tab_index(TabIndex::OverrideInParent(i as u32)),
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
                        let _ = azul_css::parser2::parse_css_declaration(
                            key.trim(), val.trim(),
                            azul_css::parser2::ErrorLocationRange::default(),
                            css_key_map, &mut Vec::new(), &mut css_attrs,
                        );
                    }
                    let props = css_attrs.into_iter().filter_map(|s| {
                        use azul_css::css::CssDeclaration;
                        use azul_css::dynamic_selector::CssPropertyWithConditions;
                        match s {
                            CssDeclaration::Static(s) => Some(CssPropertyWithConditions::simple(s)),
                            CssDeclaration::Dynamic(_) => None,
                        }
                    }).collect::<Vec<_>>();
                    if !props.is_empty() {
                        nd.set_css_props(props.into());
                    }
                }
                "contenteditable" => {
                    if parse_bool(value.as_str()).unwrap_or(false) {
                        nd.set_contenteditable(true);
                    }
                }
                _ => {}
            }
        }
        if !attr_vec.is_empty() {
            nd.set_attributes(attr_vec.into());
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
                    if current_tag == "head" { head_depth += 1; }
                    if head_depth == 0 {
                        finalize_open(&mut builder, &mut str_arena, &current_tag, &current_attrs, &css_key_map);
                        if is_void { builder.close_node(); }
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
                current_attrs.push((local.to_string(), decode_xml_entities(value.as_str()).into_owned()));
            }
            ElementEnd { end: Open, .. } => {
                if pending_open {
                    let is_void = VOID_ELEMENTS.contains(&current_tag.as_str());
                    if current_tag == "style" {
                        inside_style_tag = true;
                        style_text.clear();
                    }
                    if current_tag == "head" { head_depth += 1; }
                    if head_depth == 0 {
                        finalize_open(&mut builder, &mut str_arena, &current_tag, &current_attrs, &css_key_map);
                        if is_void { builder.close_node(); }
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
                    if current_tag == "head" { head_depth += 1; }
                    if head_depth == 0 {
                        finalize_open(&mut builder, &mut str_arena, &current_tag, &current_attrs, &css_key_map);
                        builder.close_node();
                    }
                    if current_tag == "head" && head_depth > 0 { head_depth -= 1; }
                    pending_open = false;
                }
            }
            ElementEnd { end: Close(_, close_value), .. } => {
                if pending_open {
                    let is_void = VOID_ELEMENTS.contains(&current_tag.as_str());
                    if current_tag == "head" { head_depth += 1; }
                    if head_depth == 0 {
                        finalize_open(&mut builder, &mut str_arena, &current_tag, &current_attrs, &css_key_map);
                        if is_void { builder.close_node(); }
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
                    if popped == "head" && head_depth > 0 { head_depth -= 1; }
                    if head_depth == 0 && !was_head {
                        builder.close_node();
                    }
                    if is_match { break; }
                }
            }
            Text { text } => {
                if pending_open {
                    let is_void = VOID_ELEMENTS.contains(&current_tag.as_str());
                    if current_tag == "style" {
                        inside_style_tag = true;
                        style_text.clear();
                    }
                    if current_tag == "head" { head_depth += 1; }
                    if head_depth == 0 {
                        finalize_open(&mut builder, &mut str_arena, &current_tag, &current_attrs, &css_key_map);
                        if is_void { builder.close_node(); }
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
                            builder.add_leaf(NodeData::create_text(str_arena.intern(&decoded)));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Close any remaining open elements
    if pending_open {
        finalize_open(&mut builder, &mut str_arena, &current_tag, &current_attrs, &css_key_map);
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
    use xmlparser::{ElementEnd::{Empty, Close}, Token::{ElementStart, ElementEnd, Attribute, Text}, Tokenizer};

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
        let pos = xml.find("?>").ok_or(XmlError::MalformedHierarchy(
            MalformedHierarchyError {
                expected: "<?xml".into(),
                got: "?>".into(),
            },
        ))?;
        xml = &xml[(pos + 2)..];
    }

    // Delete <!DOCTYPE ...> if necessary (case-insensitive)
    let mut xml = xml.trim();
    if xml.len() > 9 && xml[..9].to_ascii_lowercase().starts_with("<!doctype") {
        let pos = xml.find('>').ok_or(XmlError::MalformedHierarchy(
            MalformedHierarchyError {
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
                    if let Some(XmlNodeChild::Element(ref mut new_child)) = current_parent.children.as_mut().get_mut(children_len - 1) {
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
#[must_use] pub fn translate_roxmltree_expandedname(
    e: roxmltree::ExpandedName<'_, '_>,
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
#[must_use] pub fn translate_roxmltree_error(e: roxmltree::Error) -> XmlError {
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
