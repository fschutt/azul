//! Render azul DOM tree to HTML.
//!
//! Converts the `Dom` tree (returned by layout callbacks) into HTML strings
//! with stable `id="az_{N}"` attributes per node. In the full pipeline,
//! these IDs are used by the WASM-side diff algorithm to patch the DOM.
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
    _mini_wasm: &[u8],
    cb_wasms: &[CallbackWasm],
) -> String {
    // Run the layout callback to get the DOM tree
    let dom = call_layout(app_data, layout_callback, window_state, fc_cache);

    // Generate the loader JS
    let loader_js_content = loader_js::generate_loader_js("stub", cb_wasms);

    // Render the DOM to HTML
    let mut node_counter = 0;
    let body_html = render_dom_node(&dom, &mut node_counter);

    // Assemble the full page
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Azul Web App</title>
<style>{}</style>
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
        RESET_CSS,
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
    // Create the minimal ref data the layout callback needs.
    // For web, we have no GL context and an empty image cache.
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

/// Recursively render a Dom node and its children to HTML.
fn render_dom_node(dom: &Dom, counter: &mut usize) -> String {
    let node_id = *counter;
    *counter += 1;

    let node_data = &dom.root;
    let tag = node_type_to_html_tag(&node_data.node_type);

    // Check if this is a text node
    if let NodeType::Text(ref text) = node_data.node_type {
        return html_escape(text.as_str());
    }

    // Check if this is a void element (no closing tag)
    let is_void = is_void_element(tag);

    // Build attributes
    let mut attrs = format!(" id=\"az_{}\"", node_id);

    // Add id/class attributes from the node
    for attr in node_data.attributes().as_ref().iter() {
        if let Some(id) = attr.as_id() {
            attrs.push_str(&format!(" data-az-id=\"{}\"", html_escape_attr(id)));
        }
        if let Some(class) = attr.as_class() {
            attrs.push_str(&format!(" class=\"{}\"", html_escape_attr(class)));
        }
    }

    // Add callback data attributes for Phase 0 server-side execution
    if !node_data.callbacks.as_ref().is_empty() {
        attrs.push_str(&format!(" data-az-cb=\"{}\"", node_id));
        // Use the first callback's event type
        if let Some(first_cb) = node_data.callbacks.as_ref().first() {
            let ev_name = event_filter_to_js_name(&first_cb.event);
            attrs.push_str(&format!(" data-az-ev=\"{}\"", ev_name));
        }
    }

    if is_void {
        return format!("<{}{}/>", tag, attrs);
    }

    // Render children
    let mut children_html = String::new();

    // Handle inline text content for nodes that have it
    if let Some(text) = node_type_inline_text(&node_data.node_type) {
        children_html.push_str(&html_escape(text));
    }

    // Render child Dom nodes
    for child in dom.children.as_ref().iter() {
        children_html.push_str(&render_dom_node(child, counter));
    }

    format!("<{}{}>{}</{}>", tag, attrs, children_html, tag)
}

/// Map NodeType to HTML tag name.
fn node_type_to_html_tag(node_type: &NodeType) -> &'static str {
    match node_type {
        // Document structure
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
        // Lists
        NodeType::Ul => "ul",
        NodeType::Ol => "ol",
        NodeType::Li => "li",
        NodeType::Dl => "dl",
        NodeType::Dt => "dt",
        NodeType::Dd => "dd",
        NodeType::Menu => "menu",
        NodeType::MenuItem => "menuitem",
        NodeType::Dir => "dir",
        // Tables
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
        // Forms
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
        // Inline elements
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
        // Embedded content
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
        // SVG elements
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
        // Metadata
        NodeType::Title => "title",
        NodeType::Meta => "meta",
        NodeType::Link => "link",
        NodeType::Script => "script",
        NodeType::Style => "style",
        NodeType::Base => "base",
        // Pseudo-elements → rendered as spans
        NodeType::Before | NodeType::After | NodeType::Marker | NodeType::Placeholder => "span",
        // Special content types
        NodeType::Text(_) => "span", // text nodes handled separately above
        NodeType::VirtualView => "div",
        NodeType::Icon(_) => "span",
        // Catch-all for any remaining SVG filter elements, etc.
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
