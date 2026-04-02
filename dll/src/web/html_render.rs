//! Render azul DOM tree to HTML.
//!
//! Converts the `Dom` tree (returned by layout callbacks) into HTML strings
//! with stable `id="az_{N}"` attributes per node. CSS properties from the
//! node's `css_props` are emitted as `style=""` inline styles.
//!
//! Phase 0: The HTML is served as a complete page. Nodes with callbacks
//! get `data-az-cb` attributes so the Phase 0 loader JS can intercept
//! clicks and POST them to the server.

use std::sync::Arc;

use azul_core::callbacks::{LayoutCallback, LayoutCallbackInfo, LayoutCallbackInfoRefData};
use azul_core::dom::{Dom, NodeData, NodeType};
use azul_core::gl::OptionGlContextPtr;
use azul_core::refany::RefAny;
use azul_core::resources::ImageCache;
use azul_css::system::SystemStyle;
use azul_layout::window_state::FullWindowState;
use rust_fontconfig::FcFontCache;
use rust_fontconfig::registry::FcFontRegistry;

use super::cb_gen::CallbackWasm;
use super::loader_js;

/// Render the initial full HTML page for a route.
///
/// Calls the layout callback natively to produce a `Dom`, then converts
/// it to an HTML string wrapped in a complete `<!DOCTYPE html>` page.
pub fn render_initial_page(
    app_data: &RefAny,
    layout_callback: &LayoutCallback,
    window_state: &FullWindowState,
    fc_cache: &Arc<FcFontCache>,
    _font_registry: Option<&FcFontRegistry>,
    mini_wasm: &[u8],
    cb_wasms: &[CallbackWasm],
) -> String {
    // Run the layout callback to get the DOM tree
    let dom = call_layout(app_data, layout_callback, window_state, fc_cache);

    // Debug: print DOM tree structure
    let mut debug_counter = 0;
    debug_print_dom(&dom, 0, &mut debug_counter);

    // Generate preload hints for WASM assets
    let preload_hints = generate_preload_hints(mini_wasm, cb_wasms);

    // Generate the loader JS
    let loader_js_content = loader_js::generate_loader_js("stub", cb_wasms);

    // Render the DOM to HTML, collecting pseudo-state CSS rules
    let mut node_counter = 0;
    let mut callback_count = 0;
    let mut pseudo_css_rules = Vec::new();
    let body_html = render_dom_node(&dom, &mut node_counter, &mut callback_count, &mut pseudo_css_rules);
    eprintln!(
        "[azul-web] Rendered {} nodes, {} with callbacks, {} pseudo-state CSS rules",
        node_counter, callback_count, pseudo_css_rules.len(),
    );

    // Build pseudo-state stylesheet
    let pseudo_css = if pseudo_css_rules.is_empty() {
        String::new()
    } else {
        pseudo_css_rules.join("\n")
    };

    // Assemble the full page
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Azul Web App</title>
{}
<style>{}
{}
</style>
</head>
<body>
<div id="az-body">
{}
</div>
<script>
{}
</script>
</body>
</html>"#,
        preload_hints,
        RESET_CSS,
        pseudo_css,
        body_html,
        loader_js_content,
    )
}

/// Call the layout callback to produce a Dom tree.
fn call_layout(
    app_data: &RefAny,
    layout_callback: &LayoutCallback,
    window_state: &FullWindowState,
    fc_cache: &Arc<FcFontCache>,
) -> Dom {
    let image_cache = ImageCache::default();
    let gl_context = OptionGlContextPtr::None;
    let system_style = Arc::new(SystemStyle::default());

    let ref_data = LayoutCallbackInfoRefData {
        image_cache: &image_cache,
        gl_context: &gl_context,
        system_fonts: fc_cache.as_ref(),
        system_style,
    };

    let info = LayoutCallbackInfo::new(
        &ref_data,
        window_state.size.clone(),
        window_state.theme,
    );

    (layout_callback.cb)(app_data.clone(), info)
}

/// Debug-print the DOM tree structure at startup.
fn debug_print_dom(dom: &Dom, depth: usize, counter: &mut usize) {
    let indent = "  ".repeat(depth);
    let node_id = *counter;
    *counter += 1;
    let node_data = &dom.root;

    let tag = node_type_to_html_tag(&node_data.node_type);
    let css_count = node_data.css_props.as_ref().len();
    let cb_count = node_data.callbacks.as_ref().len();
    let attr_count = node_data.attributes().as_ref().len();

    let mut extras = Vec::new();
    if css_count > 0 { extras.push(format!("{} css_props", css_count)); }
    if cb_count > 0 { extras.push(format!("{} callbacks", cb_count)); }
    if attr_count > 0 { extras.push(format!("{} attrs", attr_count)); }

    let extras_str = if extras.is_empty() {
        String::new()
    } else {
        format!(" ({})", extras.join(", "))
    };

    // For text nodes, show a snippet of the text
    if let NodeType::Text(ref text) = node_data.node_type {
        let preview: String = text.as_str().chars().take(40).collect();
        eprintln!("[azul-web]   {}[{}] #text \"{}\"", indent, node_id, preview);
    } else {
        eprintln!("[azul-web]   {}[{}] <{}>{}", indent, node_id, tag, extras_str);
    }

    // Print CSS properties for debugging
    for prop in node_data.css_props.as_ref().iter() {
        eprintln!("[azul-web]   {}  style: {}", indent, prop.property.format_css());
    }

    for child in dom.children.as_ref().iter() {
        debug_print_dom(child, depth + 1, counter);
    }
}

/// Generate `<link rel="preload">` hints for WASM assets.
fn generate_preload_hints(mini_wasm: &[u8], cb_wasms: &[CallbackWasm]) -> String {
    let mut hints = String::new();

    // Preload azul-mini.wasm (even if stub, for future compatibility)
    if !mini_wasm.is_empty() {
        let hash = content_hash(mini_wasm);
        hints.push_str(&format!(
            "<link rel=\"preload\" href=\"/az/mini.{}.wasm\" as=\"fetch\" crossorigin>\n",
            hash
        ));
    }

    // Preload each callback WASM
    for cb in cb_wasms {
        if cb.is_client_side && !cb.wasm_bytes.is_empty() {
            hints.push_str(&format!(
                "<link rel=\"preload\" href=\"/az/cb/{}.{}.wasm\" as=\"fetch\" crossorigin>\n",
                cb.name, cb.content_hash
            ));
        }
    }

    hints
}

/// Simple content hash for cache-busting URLs.
fn content_hash(data: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

/// Recursively render a Dom node and its children to HTML.
fn render_dom_node(
    dom: &Dom,
    counter: &mut usize,
    callback_count: &mut usize,
    pseudo_css_rules: &mut Vec<String>,
) -> String {
    let node_id = *counter;
    *counter += 1;

    let node_data = &dom.root;

    // Text nodes render as escaped text content (no wrapping element)
    if let NodeType::Text(ref text) = node_data.node_type {
        return html_escape(text.as_str());
    }

    // Map node type to HTML tag — Body/Html/Head are rendered as div
    // since we already have the outer document structure
    let tag = match &node_data.node_type {
        NodeType::Body | NodeType::Html | NodeType::Head => "div",
        other => node_type_to_html_tag(other),
    };

    let is_void = is_void_element(tag);

    // ── Build attributes ──

    let mut classes = Vec::new();
    let mut html_attrs = Vec::new();

    // Emit all HTML attributes using name()/value() methods
    for attr in node_data.attributes().as_ref().iter() {
        let name = attr.name();
        if name == "id" {
            if let Some(id) = attr.as_id() {
                html_attrs.push(format!("data-az-id=\"{}\"", html_escape_attr(id)));
            }
        } else if name == "class" {
            if let Some(class) = attr.as_class() {
                classes.push(html_escape_attr(class));
            }
        } else if attr.is_boolean() {
            html_attrs.push(name.to_string());
        } else {
            let value = attr.value();
            if !value.as_str().is_empty() {
                html_attrs.push(format!("{}=\"{}\"", name, html_escape_attr(value.as_str())));
            }
        }
    }

    // Assemble attributes string
    let mut attrs = format!(" id=\"az_{}\"", node_id);
    if !classes.is_empty() {
        attrs.push_str(&format!(" class=\"{}\"", classes.join(" ")));
    }
    for a in &html_attrs {
        attrs.push(' ');
        attrs.push_str(a);
    }

    // ── Inline styles from css_props ──

    let (inline_style, pseudo_rules) = render_styles(node_data, node_id);
    if !inline_style.is_empty() {
        attrs.push_str(&format!(" style=\"{}\"", html_escape_attr(&inline_style)));
    }
    pseudo_css_rules.extend(pseudo_rules);

    // ── Callback data attributes (Phase 0 server-side execution) ──

    if !node_data.callbacks.as_ref().is_empty() {
        *callback_count += 1;
        attrs.push_str(&format!(" data-az-cb=\"{}\"", node_id));
        if let Some(first_cb) = node_data.callbacks.as_ref().first() {
            let ev_name = event_filter_to_js_name(&first_cb.event);
            attrs.push_str(&format!(" data-az-ev=\"{}\"", ev_name));
        }
    }

    if is_void {
        return format!("<{}{}/>", tag, attrs);
    }

    // ── Children ──

    let mut children_html = String::new();

    if let Some(text) = node_type_inline_text(&node_data.node_type) {
        children_html.push_str(&html_escape(text));
    }

    for child in dom.children.as_ref().iter() {
        children_html.push_str(&render_dom_node(child, counter, callback_count, pseudo_css_rules));
    }

    format!("<{}{}>{}</{}>", tag, attrs, children_html, tag)
}

/// Extract CSS styles from a node's css_props.
///
/// Returns (inline_style_string, vec_of_pseudo_state_css_rules).
///
/// - Unconditional properties → inline `style=""` (deduped: last value wins per property type)
/// - PseudoState properties → CSS rules like `#az_3:hover { color: red; }`
/// - Other dynamic selectors (OS, media, etc.) → skipped for now
fn render_styles(node_data: &NodeData, node_id: usize) -> (String, Vec<String>) {
    use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
    use std::collections::BTreeMap;

    // Dedup inline styles: last value per property type wins
    let mut inline_map: BTreeMap<azul_css::props::property::CssPropertyType, String> = BTreeMap::new();

    // Group pseudo-state properties: (pseudo_state) → Vec<css_declaration>
    let mut pseudo_map: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();

    for prop_with_cond in node_data.css_props.as_ref().iter() {
        let conditions = prop_with_cond.apply_if.as_ref();

        if conditions.is_empty() {
            // Unconditional → inline style (dedup by property type)
            let prop_type = prop_with_cond.property.get_type();
            inline_map.insert(prop_type, prop_with_cond.property.format_css());
        } else {
            // Check if ALL conditions are pseudo-state (common case: single hover/active/focus)
            let pseudo_state = extract_single_pseudo_state(conditions);
            if let Some(css_pseudo) = pseudo_state {
                pseudo_map
                    .entry(css_pseudo)
                    .or_default()
                    .push(prop_with_cond.property.format_css());
            }
            // Other dynamic selectors (OS, viewport, etc.) are skipped for now
        }
    }

    let inline_style = inline_map.values().cloned().collect::<Vec<_>>().join(" ");

    let pseudo_rules: Vec<String> = pseudo_map
        .into_iter()
        .map(|(pseudo, props)| {
            format!("#az_{}{} {{ {} }}", node_id, pseudo, props.join(" "))
        })
        .collect();

    (inline_style, pseudo_rules)
}

/// If all conditions in the list are a single PseudoState, return the CSS pseudo-class string.
fn extract_single_pseudo_state(conditions: &[azul_css::dynamic_selector::DynamicSelector]) -> Option<&'static str> {
    use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};

    if conditions.len() != 1 {
        return None; // Multiple conditions — not a simple pseudo-state
    }

    match &conditions[0] {
        DynamicSelector::PseudoState(state) => match state {
            PseudoStateType::Hover => Some(":hover"),
            PseudoStateType::Active => Some(":active"),
            PseudoStateType::Focus => Some(":focus"),
            PseudoStateType::FocusWithin => Some(":focus-within"),
            PseudoStateType::Disabled => Some(":disabled"),
            PseudoStateType::CheckedTrue => Some(":checked"),
            PseudoStateType::Visited => Some(":visited"),
            PseudoStateType::Dragging => Some(":active"), // closest CSS equivalent
            _ => None,
        },
        _ => None,
    }
}

/// Map NodeType to HTML tag name.
fn node_type_to_html_tag(node_type: &NodeType) -> &'static str {
    match node_type {
        NodeType::Html => "html",
        NodeType::Head => "head",
        NodeType::Body => "body",
        NodeType::Div => "div",
        NodeType::P => "p",
        NodeType::Article => "article",
        NodeType::Section => "section",
        NodeType::Nav => "nav",
        NodeType::Aside => "aside",
        NodeType::Header => "header",
        NodeType::Footer => "footer",
        NodeType::Main => "main",
        NodeType::Figure => "figure",
        NodeType::FigCaption => "figcaption",
        NodeType::H1 => "h1",
        NodeType::H2 => "h2",
        NodeType::H3 => "h3",
        NodeType::H4 => "h4",
        NodeType::H5 => "h5",
        NodeType::H6 => "h6",
        NodeType::Br => "br",
        NodeType::Hr => "hr",
        NodeType::Pre => "pre",
        NodeType::BlockQuote => "blockquote",
        NodeType::Address => "address",
        NodeType::Details => "details",
        NodeType::Summary => "summary",
        NodeType::Dialog => "dialog",
        NodeType::Ul => "ul",
        NodeType::Ol => "ol",
        NodeType::Li => "li",
        NodeType::Dl => "dl",
        NodeType::Dt => "dt",
        NodeType::Dd => "dd",
        NodeType::Menu => "menu",
        NodeType::MenuItem => "menuitem",
        NodeType::Dir => "dir",
        NodeType::Table => "table",
        NodeType::Caption => "caption",
        NodeType::THead => "thead",
        NodeType::TBody => "tbody",
        NodeType::TFoot => "tfoot",
        NodeType::Tr => "tr",
        NodeType::Th => "th",
        NodeType::Td => "td",
        NodeType::ColGroup => "colgroup",
        NodeType::Col => "col",
        NodeType::Form => "form",
        NodeType::FieldSet => "fieldset",
        NodeType::Legend => "legend",
        NodeType::Label => "label",
        NodeType::Input => "input",
        NodeType::Button => "button",
        NodeType::Select => "select",
        NodeType::OptGroup => "optgroup",
        NodeType::SelectOption => "option",
        NodeType::TextArea => "textarea",
        NodeType::Output => "output",
        NodeType::Progress => "progress",
        NodeType::Meter => "meter",
        NodeType::DataList => "datalist",
        NodeType::Span => "span",
        NodeType::A => "a",
        NodeType::Em => "em",
        NodeType::Strong => "strong",
        NodeType::B => "b",
        NodeType::I => "i",
        NodeType::U => "u",
        NodeType::S => "s",
        NodeType::Mark => "mark",
        NodeType::Del => "del",
        NodeType::Ins => "ins",
        NodeType::Code => "code",
        NodeType::Samp => "samp",
        NodeType::Kbd => "kbd",
        NodeType::Var => "var",
        NodeType::Cite => "cite",
        NodeType::Dfn => "dfn",
        NodeType::Abbr => "abbr",
        NodeType::Acronym => "acronym",
        NodeType::Q => "q",
        NodeType::Time => "time",
        NodeType::Sub => "sub",
        NodeType::Sup => "sup",
        NodeType::Small => "small",
        NodeType::Big => "big",
        NodeType::Bdo => "bdo",
        NodeType::Bdi => "bdi",
        NodeType::Wbr => "wbr",
        NodeType::Ruby => "ruby",
        NodeType::Rt => "rt",
        NodeType::Rtc => "rtc",
        NodeType::Rp => "rp",
        NodeType::Data => "data",
        NodeType::Canvas => "canvas",
        NodeType::Object => "object",
        NodeType::Param => "param",
        NodeType::Embed => "embed",
        NodeType::Audio => "audio",
        NodeType::Video => "video",
        NodeType::Source => "source",
        NodeType::Track => "track",
        NodeType::Map => "map",
        NodeType::Area => "area",
        NodeType::Image(_) => "img",
        NodeType::Svg => "svg",
        NodeType::SvgG => "g",
        NodeType::SvgDefs => "defs",
        NodeType::SvgSymbol => "symbol",
        NodeType::SvgUse => "use",
        NodeType::SvgSwitch => "switch",
        NodeType::SvgPath => "path",
        NodeType::SvgCircle => "circle",
        NodeType::SvgRect => "rect",
        NodeType::SvgEllipse => "ellipse",
        NodeType::SvgLine => "line",
        NodeType::SvgPolygon => "polygon",
        NodeType::SvgPolyline => "polyline",
        NodeType::SvgText(_) => "text",
        NodeType::SvgTspan => "tspan",
        NodeType::SvgTextPath => "textPath",
        NodeType::SvgLinearGradient => "linearGradient",
        NodeType::SvgRadialGradient => "radialGradient",
        NodeType::SvgStop => "stop",
        NodeType::SvgPattern => "pattern",
        NodeType::SvgClipPathElement => "clipPath",
        NodeType::SvgMask => "mask",
        NodeType::SvgFilter => "filter",
        NodeType::SvgImage(_) => "image",
        NodeType::SvgForeignObject => "foreignObject",
        NodeType::SvgTitle => "title",
        NodeType::SvgA => "a",
        NodeType::SvgMarker => "marker",
        NodeType::Title => "title",
        NodeType::Meta => "meta",
        NodeType::Link => "link",
        NodeType::Script => "script",
        NodeType::Style => "style",
        NodeType::Base => "base",
        NodeType::Before | NodeType::After | NodeType::Marker | NodeType::Placeholder => "span",
        NodeType::Text(_) => "span",
        NodeType::VirtualView => "div",
        NodeType::Icon(_) => "span",
        _ => "div",
    }
}

/// Extract inline text content from a NodeType, if it has any.
fn node_type_inline_text(node_type: &NodeType) -> Option<&str> {
    match node_type {
        NodeType::Text(s) => Some(s.as_str()),
        NodeType::SvgText(s) => Some(s.as_str()),
        _ => None,
    }
}

/// Whether an HTML tag is a void element (self-closing, no end tag).
fn is_void_element(tag: &str) -> bool {
    matches!(
        tag,
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img"
            | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr"
    )
}

/// Map an azul EventFilter to a JS event name.
fn event_filter_to_js_name(event: &azul_core::events::EventFilter) -> &'static str {
    use azul_core::events::{EventFilter, HoverEventFilter, FocusEventFilter};
    match event {
        EventFilter::Hover(h) => match h {
            HoverEventFilter::MouseUp => "click",
            HoverEventFilter::MouseDown => "mousedown",
            HoverEventFilter::MouseOver => "mouseover",
            HoverEventFilter::MouseLeave => "mouseleave",
            HoverEventFilter::MouseEnter => "mouseenter",
            HoverEventFilter::Scroll => "scroll",
            HoverEventFilter::TextInput => "input",
            HoverEventFilter::VirtualKeyDown => "keydown",
            HoverEventFilter::VirtualKeyUp => "keyup",
            _ => "click",
        },
        EventFilter::Focus(f) => match f {
            FocusEventFilter::FocusReceived => "focus",
            FocusEventFilter::FocusLost => "blur",
            FocusEventFilter::TextInput => "input",
            FocusEventFilter::VirtualKeyDown => "keydown",
            FocusEventFilter::VirtualKeyUp => "keyup",
            _ => "click",
        },
        _ => "click",
    }
}

/// Escape HTML special characters in text content.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape HTML attribute values.
fn html_escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Minimal reset CSS for the web backend.
const RESET_CSS: &str = r#"
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
html, body { width: 100%; height: 100%; }
"#;
