//! Render azul DOM tree to HTML with a CSS stylesheet built from the StyledDom cascade.
//!
//! **Architecture**: Azul runs its full CSS cascade on the server (Dom → StyledDom),
//! resolving ALL conditions (OS, theme, viewport, container, language). The computed
//! styles are then emitted as `#az_N { ... }` rules. Only interactive pseudo-states
//! (`:hover`, `:focus`, `:active`) remain as CSS rules for the browser.
//!
//! Images are collected and served at `/az/img/{id}`, fonts at `/az/font/{id}`.

use std::collections::BTreeMap;
use std::sync::Arc;

use azul_core::callbacks::{LayoutCallback, LayoutCallbackInfo, LayoutCallbackInfoRefData};
use azul_core::dom::{Dom, NodeData, NodeType};
use azul_core::gl::OptionGlContextPtr;
use azul_core::id::NodeId;
use azul_core::prop_cache::{CssPropertyCache, StatefulCssProperty};
use azul_core::refany::RefAny;
use azul_core::resources::{ImageCache, ImageRef, RouteMatch};
use azul_core::styled_dom::StyledDom;
use azul_css::dynamic_selector::PseudoStateType;
use azul_css::props::property::CssPropertyType;
use azul_css::system::SystemStyle;
use azul_layout::window_state::FullWindowState;
use rust_fontconfig::FcFontCache;
use rust_fontconfig::registry::FcFontRegistry;

use super::cb_gen::CallbackWasm;

/// Collected image to serve at `/az/img/{id}`.
#[derive(Debug, Clone)]
pub struct CollectedImage {
    pub id: usize,
    pub data: Vec<u8>,
    pub content_type: &'static str,
}

/// Collected font to serve at `/az/font/{id}`.
#[derive(Debug, Clone)]
pub struct CollectedFont {
    pub id: usize,
    pub name: String,
    pub data: Vec<u8>,
    pub content_type: &'static str,
}

/// Complete output of rendering a route.
#[derive(Debug, Clone)]
pub struct RenderOutput {
    pub html: String,
    pub images: Vec<CollectedImage>,
    pub fonts: Vec<CollectedFont>,
}

/// Render the initial full HTML page for a route.
///
/// 1. Calls the layout callback → `Dom`
/// 2. Runs Azul's full cascade → `StyledDom` (all conditions resolved server-side)
/// 3. Walks the StyledDom and emits HTML + `#az_N` stylesheet rules from computed styles
/// 4. Pseudo-states (`:hover`, `:focus`, `:active`) emitted as CSS rules for the browser
pub fn render_initial_page(
    app_data: &RefAny,
    layout_callback: &LayoutCallback,
    window_state: &FullWindowState,
    fc_cache: &Arc<FcFontCache>,
    _font_registry: Option<&FcFontRegistry>,
    mini_wasm: &[u8],
    cb_wasms: &[CallbackWasm],
    active_route: Option<&RouteMatch>,
    bundled_fonts: &[azul_core::resources::NamedFont],
) -> RenderOutput {
    // 1. Run layout callback → Dom (recursive tree with CSS attached)
    let dom = call_layout(app_data, layout_callback, window_state, fc_cache, active_route);

    // Debug log
    let mut debug_counter = 0;
    debug_print_dom(&dom, 0, &mut debug_counter);

    // 2. Run Azul's full cascade: Dom → StyledDom
    //    This resolves ALL conditions (OS, theme, viewport, container, language)
    //    and produces computed styles per node.
    let styled_dom = StyledDom::create_from_dom(dom);

    let node_count = styled_dom.node_data.as_ref().len();
    eprintln!("[azul-web] StyledDom cascade complete: {} nodes", node_count);

    // 3. Generate preload hints
    let preload_hints = generate_preload_hints(mini_wasm, cb_wasms);
    let loader_js_content = super::loader_js::generate_loader_js("stub", cb_wasms);

    // 4. Walk the StyledDom: generate HTML structure + CSS rules from computed styles
    let mut ctx = RenderContext::new();

    // Collect bundled fonts as @font-face rules
    for named_font in bundled_fonts {
        let font_id = ctx.fonts.len();
        ctx.font_face_rules.push(format!(
            "@font-face {{ font-family: \"{}\"; src: url(\"/az/font/{}\"); }}",
            html_escape_attr(named_font.name.as_str()),
            font_id,
        ));
        ctx.fonts.push(CollectedFont {
            id: font_id,
            name: named_font.name.as_str().to_string(),
            data: named_font.bytes.as_ref().to_vec(),
            content_type: "font/ttf",
        });
    }

    // Render the flat StyledDom arena into HTML, reading computed styles from the cache
    let body_html = ctx.render_styled_dom(&styled_dom);

    eprintln!(
        "[azul-web] Rendered {} nodes, {} with callbacks, {} CSS rules, {} images, {} fonts",
        ctx.node_counter, ctx.callback_count, ctx.css_rules.len(),
        ctx.images.len(), ctx.fonts.len(),
    );

    // 5. Build stylesheet
    let stylesheet = build_stylesheet(&ctx);

    let html = format!(
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
        stylesheet,
        body_html,
        loader_js_content,
    );

    RenderOutput {
        html,
        images: ctx.images,
        fonts: ctx.fonts,
    }
}

/// Build the full CSS stylesheet from collected rules.
fn build_stylesheet(ctx: &RenderContext) -> String {
    let mut parts = Vec::new();
    for rule in &ctx.font_face_rules {
        parts.push(rule.clone());
    }
    for rule in &ctx.css_rules {
        parts.push(rule.clone());
    }
    parts.join("\n")
}

/// State accumulated during StyledDom → HTML rendering.
struct RenderContext {
    node_counter: usize,
    callback_count: usize,
    css_rules: Vec<String>,
    font_face_rules: Vec<String>,
    images: Vec<CollectedImage>,
    fonts: Vec<CollectedFont>,
}

impl RenderContext {
    fn new() -> Self {
        Self {
            node_counter: 0,
            callback_count: 0,
            css_rules: Vec::new(),
            font_face_rules: Vec::new(),
            images: Vec::new(),
            fonts: Vec::new(),
        }
    }

    /// Render the StyledDom (flat arena) into HTML, reading computed styles
    /// from the property cache. Uses a depth-first traversal of the node hierarchy.
    fn render_styled_dom(&mut self, styled_dom: &StyledDom) -> String {
        let node_data = styled_dom.node_data.as_ref();
        let hierarchy = styled_dom.node_hierarchy.as_container();
        let cache: &CssPropertyCache = &styled_dom.css_property_cache.ptr;

        if node_data.is_empty() {
            return String::new();
        }

        // The root is typically node 0 (or styled_dom.root)
        let root_id = styled_dom.root.into_crate_internal().unwrap_or(NodeId::ZERO);
        self.render_node_recursive(root_id, node_data, hierarchy.internal, cache)
    }

    /// Recursively render a node and its children from the flat arena.
    fn render_node_recursive(
        &mut self,
        node_id: NodeId,
        node_data: &[NodeData],
        hierarchy: &[azul_core::styled_dom::NodeHierarchyItem],
        cache: &CssPropertyCache,
    ) -> String {
        let idx = node_id.index();
        if idx >= node_data.len() {
            return String::new();
        }

        let nd = &node_data[idx];
        let az_id = self.node_counter;
        self.node_counter += 1;

        // Text nodes → escaped text (no wrapper element)
        if let NodeType::Text(ref text) = nd.node_type {
            return html_escape(text.as_str());
        }

        let tag = match &nd.node_type {
            NodeType::Body | NodeType::Html | NodeType::Head => "div",
            other => node_type_to_html_tag(other),
        };
        let is_void = is_void_element(tag);

        // ── Build attributes ──
        let mut classes = Vec::new();
        let mut html_attrs = Vec::new();

        for attr in nd.attributes().as_ref().iter() {
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

        let mut attrs = format!(" id=\"az_{}\"", az_id);
        if !classes.is_empty() {
            attrs.push_str(&format!(" class=\"{}\"", classes.join(" ")));
        }
        for a in &html_attrs {
            attrs.push(' ');
            attrs.push_str(a);
        }

        // ── Handle image nodes → /az/img/{id} ──
        if let NodeType::Image(ref img_ref) = nd.node_type {
            let image_ref: &ImageRef = img_ref.as_ref();
            if let Some(raw_image) = image_ref.get_rawimage() {
                let img_id = self.images.len();
                let (data, content_type) = match azul_layout::image::encode_png(&raw_image) {
                    Ok(encoded) => (encoded.into_library_owned_vec(), "image/png"),
                    Err(_) => {
                        let bytes = raw_image.pixels.into_library_owned_vec();
                        (bytes, "application/octet-stream")
                    }
                };
                self.images.push(CollectedImage { id: img_id, data, content_type });
                attrs.push_str(&format!(" src=\"/az/img/{}\"", img_id));
            }
        }

        // ── CSS from computed styles (Azul cascade already resolved) ──
        self.emit_css_from_cache(cache, idx, az_id);

        // ── Callback data attributes (Phase 0 server-side execution) ──
        if !nd.callbacks.as_ref().is_empty() {
            self.callback_count += 1;
            attrs.push_str(&format!(" data-az-cb=\"{}\"", az_id));
            if let Some(first_cb) = nd.callbacks.as_ref().first() {
                let ev_name = event_filter_to_js_name(&first_cb.event);
                attrs.push_str(&format!(" data-az-ev=\"{}\"", ev_name));
            }
        }

        if is_void {
            return format!("<{}{}/>", tag, attrs);
        }

        // ── Children (walk the arena hierarchy) ──
        let mut children_html = String::new();

        if let Some(text) = node_type_inline_text(&nd.node_type) {
            children_html.push_str(&html_escape(text));
        }

        // Walk children via first_child / next_sibling in the flat hierarchy
        if let Some(first_child) = hierarchy.get(idx).and_then(|h| h.first_child_id(node_id)) {
            let mut child_id = first_child;
            loop {
                children_html.push_str(
                    &self.render_node_recursive(child_id, node_data, hierarchy, cache),
                );
                match hierarchy.get(child_id.index()).and_then(|h| h.next_sibling_id()) {
                    Some(next) => child_id = next,
                    None => break,
                }
            }
        }

        format!("<{}{}>{}</{}>", tag, attrs, children_html, tag)
    }

    /// Emit CSS rules for a node from the property cache.
    ///
    /// - `computed_values[node]` → base rule `#az_N { ... }` (fully cascade-resolved)
    /// - `css_props[node]` with state=Hover → `#az_N:hover { ... }` (browser-interactive)
    /// - `css_props[node]` with state=Focus → `#az_N:focus { ... }` etc.
    fn emit_css_from_cache(&mut self, cache: &CssPropertyCache, node_idx: usize, az_id: usize) {
        // Base computed styles (all conditions already resolved by Azul)
        if let Some(computed) = cache.computed_values.get(node_idx) {
            if !computed.is_empty() {
                let decls: Vec<String> = computed.iter()
                    .map(|(_ptype, pwith)| pwith.property.format_css())
                    .collect();
                self.css_rules.push(format!("#az_{} {{ {} }}", az_id, decls.join(" ")));
            }
        }

        // Interactive pseudo-state rules from the css_props cache
        // (these are properties that differ based on :hover, :focus, :active, etc.)
        let props_slice = cache.css_props.get_slice(node_idx);
        if !props_slice.is_empty() {
            let mut pseudo_groups: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
            for sp in props_slice.iter() {
                if let Some(css_pseudo) = pseudo_state_to_css(&sp.state) {
                    pseudo_groups.entry(css_pseudo).or_default().push(sp.property.format_css());
                }
                // Normal state properties are already in computed_values, skip them here
            }
            for (pseudo, decls) in pseudo_groups {
                self.css_rules.push(format!("#az_{}{} {{ {} }}", az_id, pseudo, decls.join(" ")));
            }
        }
    }
}

/// Convert a PseudoStateType to a CSS pseudo-class string.
/// Returns None for Normal (those are in computed_values, not pseudo rules).
fn pseudo_state_to_css(state: &PseudoStateType) -> Option<&'static str> {
    match state {
        PseudoStateType::Normal => None, // base styles handled by computed_values
        PseudoStateType::Hover => Some(":hover"),
        PseudoStateType::Active => Some(":active"),
        PseudoStateType::Focus => Some(":focus"),
        PseudoStateType::FocusWithin => Some(":focus-within"),
        PseudoStateType::Disabled => Some(":disabled"),
        PseudoStateType::CheckedTrue => Some(":checked"),
        PseudoStateType::Visited => Some(":visited"),
        PseudoStateType::Dragging => Some(":active"), // closest CSS equivalent
        _ => None,
    }
}

/// Call the layout callback to produce a Dom tree.
fn call_layout(
    app_data: &RefAny,
    layout_callback: &LayoutCallback,
    window_state: &FullWindowState,
    fc_cache: &Arc<FcFontCache>,
    active_route: Option<&RouteMatch>,
) -> Dom {
    let image_cache = ImageCache::default();
    let gl_context = OptionGlContextPtr::None;
    let system_style = Arc::new(SystemStyle::default());

    let ref_data = LayoutCallbackInfoRefData {
        image_cache: &image_cache,
        gl_context: &gl_context,
        system_fonts: fc_cache.as_ref(),
        system_style,
        active_route,
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

    if let NodeType::Text(ref text) = node_data.node_type {
        let preview: String = text.as_str().chars().take(40).collect();
        eprintln!("[azul-web]   {}[{}] #text \"{}\"", indent, node_id, preview);
    } else {
        eprintln!("[azul-web]   {}[{}] <{}>{}", indent, node_id, tag, extras_str);
    }

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
    if !mini_wasm.is_empty() {
        let hash = content_hash(mini_wasm);
        hints.push_str(&format!(
            "<link rel=\"preload\" href=\"/az/mini.{}.wasm\" as=\"fetch\" crossorigin>\n",
            hash
        ));
    }
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

/// Map NodeType to HTML tag name.
fn node_type_to_html_tag(node_type: &NodeType) -> &'static str {
    match node_type {
        NodeType::Html => "html", NodeType::Head => "head", NodeType::Body => "body",
        NodeType::Div => "div", NodeType::P => "p", NodeType::Article => "article",
        NodeType::Section => "section", NodeType::Nav => "nav", NodeType::Aside => "aside",
        NodeType::Header => "header", NodeType::Footer => "footer", NodeType::Main => "main",
        NodeType::Figure => "figure", NodeType::FigCaption => "figcaption",
        NodeType::H1 => "h1", NodeType::H2 => "h2", NodeType::H3 => "h3",
        NodeType::H4 => "h4", NodeType::H5 => "h5", NodeType::H6 => "h6",
        NodeType::Br => "br", NodeType::Hr => "hr", NodeType::Pre => "pre",
        NodeType::BlockQuote => "blockquote", NodeType::Address => "address",
        NodeType::Details => "details", NodeType::Summary => "summary", NodeType::Dialog => "dialog",
        NodeType::Ul => "ul", NodeType::Ol => "ol", NodeType::Li => "li",
        NodeType::Dl => "dl", NodeType::Dt => "dt", NodeType::Dd => "dd",
        NodeType::Menu => "menu", NodeType::MenuItem => "menuitem", NodeType::Dir => "dir",
        NodeType::Table => "table", NodeType::Caption => "caption", NodeType::THead => "thead",
        NodeType::TBody => "tbody", NodeType::TFoot => "tfoot", NodeType::Tr => "tr",
        NodeType::Th => "th", NodeType::Td => "td", NodeType::ColGroup => "colgroup",
        NodeType::Col => "col", NodeType::Form => "form", NodeType::FieldSet => "fieldset",
        NodeType::Legend => "legend", NodeType::Label => "label", NodeType::Input => "input",
        NodeType::Button => "button", NodeType::Select => "select", NodeType::OptGroup => "optgroup",
        NodeType::SelectOption => "option", NodeType::TextArea => "textarea",
        NodeType::Output => "output", NodeType::Progress => "progress", NodeType::Meter => "meter",
        NodeType::DataList => "datalist", NodeType::Span => "span", NodeType::A => "a",
        NodeType::Em => "em", NodeType::Strong => "strong", NodeType::B => "b",
        NodeType::I => "i", NodeType::U => "u", NodeType::S => "s", NodeType::Mark => "mark",
        NodeType::Del => "del", NodeType::Ins => "ins", NodeType::Code => "code",
        NodeType::Samp => "samp", NodeType::Kbd => "kbd", NodeType::Var => "var",
        NodeType::Cite => "cite", NodeType::Dfn => "dfn", NodeType::Abbr => "abbr",
        NodeType::Acronym => "acronym", NodeType::Q => "q", NodeType::Time => "time",
        NodeType::Sub => "sub", NodeType::Sup => "sup", NodeType::Small => "small",
        NodeType::Big => "big", NodeType::Bdo => "bdo", NodeType::Bdi => "bdi",
        NodeType::Wbr => "wbr", NodeType::Ruby => "ruby", NodeType::Rt => "rt",
        NodeType::Rtc => "rtc", NodeType::Rp => "rp", NodeType::Data => "data",
        NodeType::Canvas => "canvas", NodeType::Object => "object", NodeType::Param => "param",
        NodeType::Embed => "embed", NodeType::Audio => "audio", NodeType::Video => "video",
        NodeType::Source => "source", NodeType::Track => "track", NodeType::Map => "map",
        NodeType::Area => "area", NodeType::Image(_) => "img",
        NodeType::Svg => "svg", NodeType::SvgG => "g", NodeType::SvgDefs => "defs",
        NodeType::SvgSymbol => "symbol", NodeType::SvgUse => "use", NodeType::SvgSwitch => "switch",
        NodeType::SvgPath => "path", NodeType::SvgCircle => "circle", NodeType::SvgRect => "rect",
        NodeType::SvgEllipse => "ellipse", NodeType::SvgLine => "line",
        NodeType::SvgPolygon => "polygon", NodeType::SvgPolyline => "polyline",
        NodeType::SvgText(_) => "text", NodeType::SvgTspan => "tspan",
        NodeType::SvgTextPath => "textPath", NodeType::SvgLinearGradient => "linearGradient",
        NodeType::SvgRadialGradient => "radialGradient", NodeType::SvgStop => "stop",
        NodeType::SvgPattern => "pattern", NodeType::SvgClipPathElement => "clipPath",
        NodeType::SvgMask => "mask", NodeType::SvgFilter => "filter",
        NodeType::SvgImage(_) => "image", NodeType::SvgForeignObject => "foreignObject",
        NodeType::SvgTitle => "title", NodeType::SvgA => "a", NodeType::SvgMarker => "marker",
        NodeType::Title => "title", NodeType::Meta => "meta", NodeType::Link => "link",
        NodeType::Script => "script", NodeType::Style => "style", NodeType::Base => "base",
        NodeType::Before | NodeType::After | NodeType::Marker | NodeType::Placeholder => "span",
        NodeType::Text(_) => "span", NodeType::VirtualView => "div", NodeType::Icon(_) => "span",
        _ => "div",
    }
}

fn node_type_inline_text(node_type: &NodeType) -> Option<&str> {
    match node_type {
        NodeType::Text(s) => Some(s.as_str()),
        NodeType::SvgText(s) => Some(s.as_str()),
        _ => None,
    }
}

fn is_void_element(tag: &str) -> bool {
    matches!(tag,
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img"
            | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr"
    )
}

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

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c { '&' => out.push_str("&amp;"), '<' => out.push_str("&lt;"), '>' => out.push_str("&gt;"), _ => out.push(c) }
    }
    out
}

fn html_escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c { '&' => out.push_str("&amp;"), '"' => out.push_str("&quot;"), '<' => out.push_str("&lt;"), '>' => out.push_str("&gt;"), _ => out.push(c) }
    }
    out
}

const RESET_CSS: &str = r#"
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
html, body { width: 100%; height: 100%; }
"#;
