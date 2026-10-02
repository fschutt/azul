//! Mail HTML made safe and well-formed for azul's XML parser.
//!
//! Real mail HTML is rarely XML: unquoted and bare attributes, unclosed `<p>` and `<td>`,
//! `&nbsp;`, uppercase tags, conditional comments. The mail is read with the engine's own HTML
//! parser, `Xml::create_from_html` (the lenient tokenizer and the browser-like tree construction
//! every azul loader shares: implied end tags, an end tag closing only within its element's
//! scope, the formatting elements reopened in the next block, the implied `<tbody>`, the HTML
//! Standard's named references), and the tree it builds is written out as a small, well-formed
//! XHTML subset by the POLICY below:
//!
//! - scripts, titles, form controls (`select`, `textarea`, `button` with its label; `form` and
//!   `input` are dropped too, since azul would make them live widgets), frames, SVG and MathML
//!   go with their content; comments, doctypes and processing instructions go;
//! - a `<style>` keeps only safe rules (the declarations `style` attributes keep, plain or in
//!   `@media`), each scoped to the paper (below); `@import`, `@font-face`, any URL, positioning
//!   and the `<!--` / `-->` wrapping go;
//! - images are NOT loaded (remote images are off): each becomes a grey `[image: alt]` text,
//!   and a tracking pixel (1x1 or hidden) not even that;
//! - only presentational tags stay (`p div span b i u a table tr td ul li h1 ...`); `font`
//!   becomes a `span`, `center` a centred `div`, a `body` with something to say (a `style`, a
//!   `bgcolor` ...) a `div`, and any other tag is dropped with its text kept;
//! - attributes: `href` (http, https and mailto only), `colspan`, `rowspan`, `dir`, and `style`
//!   with a short list of properties whose values name no URL (and no negative margin); the
//!   presentational ones of tables, cells and blocks (`align`, `valign`, `width`, `height`,
//!   `bgcolor`, `border`, `bordercolor`, `cellpadding`, `cellspacing`, `nowrap`) as written, for
//!   azul's own HTML hints; on an element renamed here (`body`, `font`) and on a kept picture,
//!   what they mean becomes style (`bgcolor`, `text`, `color`, `face`, `size`, `align`,
//!   `hspace`, `vspace`, `border`);
//! - nesting deeper than 200 keeps the text only;
//! - the text (its character references decoded by the parser) is re-escaped.
//!
//! The mail is read on PAPER, a `<div class="azmail-paper">`: a mail that says nothing about
//! the dark mode was designed on white, so its paper is white with dark text in either mode (and
//! its links a blue readable on white). A mail with its own dark rules
//! ([`Sanitized::has_dark_rules`]) keeps them, and its paper follows the app's mode through a
//! `prefers-color-scheme` rule of its own, so the mail's dark rules fire on a dark sheet.
//!
//! The result is `<html><head><style>...</style></head><body><div class="azmail-paper">...
//! </div></body></html>` for `Dom::create_from_parsed_xml`.

use azul::{
    dom::{XmlNode, XmlNodeChild},
    xml::Xml,
};

/// The sanitized document and what was left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sanitized {
    pub xhtml: String,
    /// Images that were not loaded.
    pub blocked_images: usize,
    /// Whether the mail has its own rules for the dark mode: a `prefers-color-scheme: dark`
    /// media rule, or a `color-scheme` / `supported-color-schemes` meta or declaration that names
    /// `dark`. Then its paper follows the app's mode; otherwise it is always white.
    pub has_dark_rules: bool,
    /// The web pictures kept as `<img src>` ([`sanitize_with`] with pictures on), each once,
    /// in order: what the app downloads and registers under its address.
    pub remote_images: Vec<String>,
}

/// See the module documentation.
pub fn sanitize(html: &str) -> Sanitized {
    sanitize_with(html, false)
}

/// [`sanitize`], or - with `pictures` (the reader clicked "download pictures") - the same with
/// every web picture (`http:` / `https:`) kept as `<img src alt width height/>` and listed in
/// [`Sanitized::remote_images`]. A tracking pixel stays out either way, and a picture that is
/// not on the web (`cid:` - an attached one -, `data:`, anything else) stays a placeholder.
pub fn sanitize_with(html: &str, pictures: bool) -> Sanitized {
    let mut s = Sanitizer {
        pictures,
        ..Sanitizer::default()
    };
    let document = Xml::create_from_html(html);
    s.children(&document.root);
    s.finish()
}

/// Tags deeper than this keep their text only.
const MAX_DEPTH: usize = 200;
/// How a blocked image's placeholder looks.
const BLOCKED_IMAGE_STYLE: &str = "color: #6b7385";

/// The class of the paper the mail is read on (see the module documentation).
const PAPER_CLASS: &str = "azmail-paper";
/// The paper in the light mode - and always, for a mail without dark rules.
const PAPER_LIGHT: &str =
    ".azmail-paper { background-color: #ffffff; color: #1a1a1a; padding: 12px; } ";
/// The paper in the dark mode, for a mail with dark rules.
const PAPER_DARK: &str = "@media (prefers-color-scheme: dark) { .azmail-paper { \
                          background-color: #1e1e1e; color: #e8e8e8; } } ";
/// A link on white paper: a blue readable on white (the UA's dark-mode link colour is not).
/// Before the mail's own rules, which win over it.
const PAPER_LINK: &str = ".azmail-paper a { color: #0b57d0; } ";

/// Elements that go with everything inside them.
const SKIP_WITH_CONTENT: &[&str] = &[
    "script", "style", "title", "textarea", "select", "button", "noscript", "template", "iframe",
    "object", "applet", "svg", "math", "xmp", "frameset", "noframes", "audio", "video", "canvas",
];

/// Style properties that stay (with a value that names no URL).
const STYLE_PROPERTIES: &[&str] = &[
    "color",
    "background-color",
    "background",
    "font-weight",
    "font-style",
    "font-size",
    "font-family",
    "text-align",
    "text-decoration",
    "text-transform",
    "line-height",
    "letter-spacing",
    "vertical-align",
    "white-space",
    "width",
    "min-width",
    "max-width",
    "height",
    "max-height",
    "display",
    "overflow",
    "opacity",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "border",
    "border-top",
    "border-right",
    "border-bottom",
    "border-left",
    "border-color",
    "border-style",
    "border-width",
    "border-collapse",
    "border-spacing",
    "border-radius",
    "list-style-type",
];

/// What a source tag becomes.
enum Tag {
    /// An element with content, under this output name.
    Keep(&'static str),
    /// An empty element (`<br/>`, `<hr/>`).
    Void(&'static str),
    /// An image: a placeholder text.
    Image,
    /// Not in the output; its content is.
    Drop,
}

fn classify(name: &str) -> Tag {
    const SAME: &[&str] = &[
        "p",
        "div",
        "span",
        "a",
        "b",
        "strong",
        "i",
        "em",
        "u",
        "s",
        "del",
        "ins",
        "small",
        "big",
        "sub",
        "sup",
        "code",
        "pre",
        "blockquote",
        "ul",
        "ol",
        "li",
        "dl",
        "dt",
        "dd",
        "table",
        "caption",
        "thead",
        "tbody",
        "tfoot",
        "tr",
        "td",
        "th",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "abbr",
        "cite",
        "q",
        "kbd",
        "samp",
        "var",
        "mark",
    ];
    if let Some(same) = SAME.iter().find(|t| **t == name) {
        return Tag::Keep(*same);
    }
    match name {
        "font" | "label" => Tag::Keep("span"),
        "center" | "body" | "main" | "section" | "article" | "header" | "footer" | "nav"
        | "aside" | "figure" | "figcaption" | "address" | "details" | "summary" => Tag::Keep("div"),
        "strike" => Tag::Keep("s"),
        "tt" => Tag::Keep("code"),
        "br" => Tag::Void("br"),
        "hr" => Tag::Void("hr"),
        "img" => Tag::Image,
        _ => Tag::Drop,
    }
}

#[derive(Default)]
struct Sanitizer {
    out: String,
    /// How many output elements are open around the one being written.
    depth: usize,
    blocked_images: usize,
    /// The mail's own style rules, made safe and scoped to the paper.
    styles: String,
    has_dark_rules: bool,
    /// Web pictures are kept ("download pictures").
    pictures: bool,
    /// The web pictures kept, each once.
    remote_images: Vec<String>,
}

impl Sanitizer {
    /// The nodes of the parsed mail, in order.
    fn children(&mut self, nodes: &[XmlNodeChild]) {
        for node in nodes {
            match node {
                XmlNodeChild::Text(text) => escape_into(&mut self.out, text.as_str(), false),
                XmlNodeChild::Element(element) => self.element(element),
            }
        }
    }

    /// One element of the parsed mail (its name and its attribute names are lower case, its
    /// attribute values and its text decoded) and everything in it, by the policy.
    fn element(&mut self, element: &XmlNode) {
        let name = element.node_type.inner.as_str();
        let attributes: Vec<(&str, &str)> = element
            .attributes
            .inner
            .iter()
            .map(|pair| (pair.key.as_str(), pair.value.as_str()))
            .collect();
        if name == "style" {
            // The sheet (the element's raw text): kept where it is safe (`style_sheet`).
            let mut css = String::new();
            for node in element.children.iter() {
                if let XmlNodeChild::Text(text) = node {
                    css.push_str(text.as_str());
                }
            }
            self.style_sheet(&css);
            return;
        }
        if SKIP_WITH_CONTENT.contains(&name) {
            return;
        }
        if name == "meta" {
            self.meta(&attributes);
        }
        match classify(name) {
            Tag::Drop => self.children(&element.children),
            Tag::Image => self.image(&attributes),
            Tag::Void(tag) => {
                self.out.push('<');
                self.out.push_str(tag);
                self.out.push_str("/>");
            }
            Tag::Keep(tag) => {
                let mut kept = String::new();
                push_attributes(&mut kept, name, tag, &attributes);
                // Every document has a `<body>` (the parser implies one around a fragment): only
                // one with something to say - a style, a background - becomes a `div`.
                if (name == "body" && kept.is_empty()) || self.depth >= MAX_DEPTH {
                    self.children(&element.children);
                    return;
                }
                self.out.push('<');
                self.out.push_str(tag);
                self.out.push_str(&kept);
                self.out.push('>');
                self.depth += 1;
                self.children(&element.children);
                self.depth -= 1;
                self.out.push_str("</");
                self.out.push_str(tag);
                self.out.push('>');
            }
        }
    }

    /// A `<meta name="color-scheme">` (or the older `supported-color-schemes`) that names
    /// `dark`: the mail supports the dark mode.
    fn meta(&mut self, attributes: &[(&str, &str)]) {
        let value = |name: &str| {
            attributes
                .iter()
                .find(|&&(n, _)| n == name)
                .map(|&(_, v)| v.to_ascii_lowercase())
        };
        let names_scheme = value("name")
            .is_some_and(|n| matches!(n.trim(), "color-scheme" | "supported-color-schemes"));
        if names_scheme && value("content").is_some_and(|c| names_dark(&c)) {
            self.has_dark_rules = true;
        }
    }

    /// A `<style>`'s sheet: its safe rules, scoped to the paper, join the document's.
    fn style_sheet(&mut self, css: &str) {
        let (safe, dark) = sanitize_style_sheet(css);
        self.styles.push_str(&safe);
        self.has_dark_rules |= dark;
    }

    /// An `<img>`: not loaded (a placeholder text, or nothing for a tracking pixel), or - with
    /// pictures on - kept for the app to fetch.
    fn image(&mut self, attributes: &[(&str, &str)]) {
        self.blocked_images += 1;
        let attribute = |name: &str| {
            attributes
                .iter()
                .find(|&&(n, _)| n == name)
                .map(|&(_, v)| v.trim())
        };
        if is_tracking_pixel(attribute("width"), attribute("height"), attribute("style")) {
            return;
        }
        let alt = attribute("alt").filter(|a| !a.is_empty());
        let src = attribute("src").unwrap_or("");
        let scheme = src.to_ascii_lowercase();
        if self.pictures && (scheme.starts_with("https://") || scheme.starts_with("http://")) {
            // Loaded after all: an image the app fetches and caches under its src.
            self.blocked_images -= 1;
            self.out.push_str("<img src=\"");
            escape_into(&mut self.out, src, true);
            self.out.push('"');
            if let Some(alt) = alt {
                self.out.push_str(" alt=\"");
                escape_into(&mut self.out, alt, true);
                self.out.push('"');
            }
            let pixels = |name: &str| {
                attribute(name)
                    .map(|v| v.trim_end_matches("px").trim())
                    .and_then(|v| v.parse::<u32>().ok())
            };
            for name in ["width", "height"] {
                if let Some(n) = pixels(name) {
                    self.out.push_str(&format!(" {name}=\"{n}\""));
                }
            }
            // What a picture's legacy attributes mean in a browser (HTML's rendering section,
            // "images"): `align` floats it or aligns it on the line, `hspace` / `vspace` are
            // its margins, `border` a solid border.
            let mut styles: Vec<String> = Vec::new();
            let align = attribute("align").map(str::to_ascii_lowercase);
            match align.as_deref() {
                Some("left") => styles.push(String::from("float: left")),
                Some("right") => styles.push(String::from("float: right")),
                Some("top") => styles.push(String::from("vertical-align: top")),
                Some("texttop") => styles.push(String::from("vertical-align: text-top")),
                Some("middle" | "absmiddle" | "center") => {
                    styles.push(String::from("vertical-align: middle"));
                }
                Some("bottom" | "baseline") => styles.push(String::from("vertical-align: baseline")),
                Some("absbottom") => styles.push(String::from("vertical-align: bottom")),
                _ => {}
            }
            if let Some(n) = pixels("hspace") {
                styles.push(format!("margin-left: {n}px"));
                styles.push(format!("margin-right: {n}px"));
            }
            if let Some(n) = pixels("vspace") {
                styles.push(format!("margin-top: {n}px"));
                styles.push(format!("margin-bottom: {n}px"));
            }
            if let Some(n) = pixels("border") {
                styles.push(format!("border-width: {n}px"));
                styles.push(String::from("border-style: solid"));
            }
            if !styles.is_empty() {
                self.out.push_str(" style=\"");
                escape_into(&mut self.out, &styles.join("; "), true);
                self.out.push('"');
            }
            self.out.push_str("/>");
            if !self.remote_images.iter().any(|u| u == src) {
                self.remote_images.push(src.to_string());
            }
            return;
        }
        let label = match alt {
            Some(alt) => format!("[image: {alt}]"),
            None => String::from("[image]"),
        };
        self.out.push_str("<span style=\"");
        self.out.push_str(BLOCKED_IMAGE_STYLE);
        self.out.push_str("\">");
        escape_into(&mut self.out, &label, false);
        self.out.push_str("</span>");
    }

    fn finish(self) -> Sanitized {
        // The paper first, the mail's rules after it (they win where they say something).
        let mut sheet = String::from(PAPER_LIGHT);
        if self.has_dark_rules {
            sheet.push_str(PAPER_DARK);
        } else {
            sheet.push_str(PAPER_LINK);
        }
        sheet.push_str(&self.styles);
        let mut xhtml = String::from("<html><head><style>");
        escape_into(&mut xhtml, sheet.trim_end(), false);
        xhtml.push_str("</style></head><body><div class=\"");
        xhtml.push_str(PAPER_CLASS);
        xhtml.push_str("\">");
        xhtml.push_str(&self.out);
        xhtml.push_str("</div></body></html>");
        Sanitized {
            xhtml,
            blocked_images: self.blocked_images,
            has_dark_rules: self.has_dark_rules,
            remote_images: self.remote_images,
        }
    }
}

/// The presentational attributes azul's own HTML hints map to CSS where a browser does (core's
/// `presentational_css`, the one generator: on tables, their parts and cells, `div`, `p` and the
/// headings - on any other element they mean nothing there either). Kept as written.
const PRESENTATIONAL: &[&str] = &[
    "align",
    "valign",
    "width",
    "height",
    "bgcolor",
    "border",
    "bordercolor",
    "cellpadding",
    "cellspacing",
    "nowrap",
];

/// Writes the attributes an output tag keeps: `href`, `colspan`, `rowspan`, `dir`, the
/// presentational attributes ([`PRESENTATIONAL`]) for azul's HTML hints, and one `style`: what a
/// renamed element said with its attributes, then the safe style declarations.
fn push_attributes(out: &mut String, source: &str, tag: &str, attributes: &[(&str, &str)]) {
    let mut kept: Vec<(&str, String)> = Vec::new();
    let mut styles: Vec<(String, String)> = Vec::new();
    if source == "center" {
        styles.push((String::from("text-align"), String::from("center")));
    }
    // An element renamed in the output (`<body>` a `div`, `<font>` a `span`) takes what its
    // attributes mean along as style: azul's hints know them only on the element they were
    // written on.
    let renamed = source != tag;
    for &(name, value) in attributes {
        let value = value.trim();
        let lower = value.to_ascii_lowercase();
        match name {
            "href" if tag == "a" => {
                if ["http://", "https://", "mailto:"]
                    .iter()
                    .any(|scheme| lower.starts_with(scheme))
                {
                    kept.push(("href", value.to_string()));
                }
            }
            "colspan" | "rowspan" if tag == "td" || tag == "th" => {
                if let Ok(n) = value.parse::<u32>() {
                    if (1..=1000).contains(&n) {
                        let name = if name == "colspan" {
                            "colspan"
                        } else {
                            "rowspan"
                        };
                        kept.push((name, n.to_string()));
                    }
                }
            }
            "dir" => {
                if matches!(lower.as_str(), "ltr" | "rtl" | "auto") {
                    kept.push(("dir", lower.clone()));
                }
            }
            "bgcolor" | "text" if source == "body" => {
                if !value.is_empty() && safe_style_value(value) {
                    let property = if name == "bgcolor" {
                        "background-color"
                    } else {
                        "color"
                    };
                    styles.push((String::from(property), value.to_string()));
                }
            }
            "color" | "face" if source == "font" => {
                if !value.is_empty() && safe_style_value(value) {
                    let property = if name == "color" { "color" } else { "font-family" };
                    styles.push((String::from(property), value.to_string()));
                }
            }
            "size" if source == "font" => {
                if let Some(px) = html_font_size(value) {
                    styles.push((String::from("font-size"), format!("{px}px")));
                }
            }
            "style" => styles.extend(parse_style(value)),
            _ if !renamed && PRESENTATIONAL.contains(&name) => {
                kept.push((name, value.to_string()));
            }
            _ => {}
        }
    }
    for (name, value) in kept {
        out.push(' ');
        out.push_str(name);
        out.push_str("=\"");
        escape_into(out, &value, true);
        out.push('"');
    }
    if !styles.is_empty() {
        let joined = styles
            .iter()
            .map(|(p, v)| format!("{p}: {v}"))
            .collect::<Vec<_>>()
            .join("; ");
        out.push_str(" style=\"");
        escape_into(out, &joined, true);
        out.push('"');
    }
}

/// A `<font size>` in px: HTML's rules for a legacy font size (`1` to `7`, or `+n` / `-n` from
/// `3`, clamped to `1..=7`) and the CSS absolute sizes they stand for at a 16px `medium`.
fn html_font_size(value: &str) -> Option<u32> {
    const PX: [u32; 7] = [10, 13, 16, 18, 24, 32, 48];
    let value = value.trim();
    let (sign, rest) = match value.as_bytes().first() {
        Some(b'+') => (Some(1_i64), &value[1..]),
        Some(b'-') => (Some(-1_i64), &value[1..]),
        _ => (None, value),
    };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let n: i64 = rest[..digits].parse().ok()?;
    let size = sign.map_or(n, |sign| 3 + sign * n).clamp(1, 7);
    usize::try_from(size - 1).ok().and_then(|i| PX.get(i).copied())
}

/// Whether a `color-scheme` value names the dark mode (`dark`, `light dark`, `only dark`).
fn names_dark(value: &str) -> bool {
    value
        .split(|c: char| c.is_whitespace() || c == ',')
        .any(|word| word.eq_ignore_ascii_case("dark"))
}

/// A mail's style sheet made safe, and whether it has rules for the dark mode.
///
/// Kept: rules (`selectors { declarations }`) with the declarations [`parse_style`] keeps, and
/// `@media` blocks of such rules, every selector scoped to the paper
/// ([`scope_selectors`]). Gone: every other at-rule (`@import`, `@font-face`, `@page`,
/// `@keyframes`, `@supports`, `@charset`), comments, the `<!--` / `-->` wrapping, a rule
/// left with no declaration. Dark rules: a kept `@media` whose condition says
/// `prefers-color-scheme: dark`, or a `color-scheme` declaration that names `dark`.
fn sanitize_style_sheet(css: &str) -> (String, bool) {
    let mut plain = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(at) = rest.find("/*") {
        plain.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        rest = match after.find("*/") {
            Some(end) => &after[end + 2..],
            None => "",
        };
    }
    plain.push_str(rest);
    let plain = plain.replace("<!--", " ").replace("-->", " ");
    let mut out = String::new();
    let mut dark = false;
    sanitize_rules(&plain, &mut out, &mut dark, false);
    (out, dark)
}

/// The rules of `css` (a sheet, or the inside of an `@media` block when `nested`), made safe
/// into `out`.
fn sanitize_rules(css: &str, out: &mut String, dark: &mut bool, nested: bool) {
    let mut rest = css.trim_start();
    while !rest.is_empty() {
        let brace = rest.find('{');
        if let Some(at_rule) = rest.strip_prefix('@') {
            let name_len = at_rule
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
                .unwrap_or(at_rule.len());
            let name = at_rule[..name_len].to_ascii_lowercase();
            // A statement (`@import ...;`) ends at its `;` - when no block follows at all, or
            // before the first one - a block at-rule at its block.
            match (rest.find(';'), brace) {
                (Some(semi), None) => {
                    rest = rest[semi + 1..].trim_start();
                }
                (Some(semi), Some(b)) if semi < b => {
                    rest = rest[semi + 1..].trim_start();
                }
                (_, Some(b)) => {
                    let condition = rest[1 + name_len..b].trim();
                    let (body, after) = block_body(&rest[b..]);
                    if name == "media" && !nested && safe_style_value(condition) {
                        let mut inner = String::new();
                        sanitize_rules(body, &mut inner, dark, true);
                        if !inner.is_empty() {
                            let lower = condition.to_ascii_lowercase();
                            if lower.contains("prefers-color-scheme") && lower.contains("dark") {
                                *dark = true;
                            }
                            out.push_str("@media ");
                            out.push_str(condition);
                            out.push_str(" { ");
                            out.push_str(&inner);
                            out.push_str("} ");
                        }
                    }
                    rest = after.trim_start();
                }
                (None, None) => break,
            }
            continue;
        }
        let Some(b) = brace else {
            break;
        };
        let selectors = rest[..b].trim();
        let (body, after) = block_body(&rest[b..]);
        let lower = body.to_ascii_lowercase();
        if let Some(at) = lower.find("color-scheme") {
            let value = lower[at..].split(';').next().unwrap_or("");
            if names_dark(value.split_once(':').map_or("", |(_, v)| v)) {
                *dark = true;
            }
        }
        let declarations = parse_style(body);
        if let (false, Some(scoped)) = (declarations.is_empty(), scope_selectors(selectors)) {
            out.push_str(&scoped);
            out.push_str(" { ");
            for (property, value) in &declarations {
                out.push_str(property);
                out.push_str(": ");
                out.push_str(value);
                out.push_str("; ");
            }
            out.push_str("} ");
        }
        rest = after.trim_start();
    }
}

/// The inside of the block that starts at `s` (its `{`) and what follows its matching `}`; an
/// unclosed block runs to the end.
fn block_body(s: &str) -> (&str, &str) {
    let mut depth = 0_usize;
    for (at, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return (&s[1..at], &s[at + 1..]);
                }
            }
            _ => {}
        }
    }
    (s.get(1..).unwrap_or(""), "")
}

/// A rule's selectors scoped to the paper, so a mail's sheet cannot restyle the app around
/// it: `body`, `html` and `:root` ARE the paper, every other selector is a descendant of it
/// (`.x` becomes `.azmail-paper .x`). `None` for a selector list that is empty or holds what
/// no selector needs (`<`, `\`, `{`, `@`, a URL or a control character).
fn scope_selectors(selectors: &str) -> Option<String> {
    let bad = |s: &str| {
        s.is_empty()
            || s.contains(['<', '\\', '{', '}', '@'])
            || s.to_ascii_lowercase().contains("url(")
            || s.chars().any(char::is_control)
    };
    let scoped: Vec<String> = selectors
        .split(',')
        .map(str::trim)
        .map(|sel| {
            if bad(sel) {
                return None;
            }
            let first_len = sel.find(char::is_whitespace).unwrap_or(sel.len());
            let (first, tail) = sel.split_at(first_len);
            Some(
                if matches!(
                    first.to_ascii_lowercase().as_str(),
                    "body" | "html" | ":root"
                ) {
                    format!(".{PAPER_CLASS}{tail}")
                } else {
                    format!(".{PAPER_CLASS} {sel}")
                },
            )
        })
        .collect::<Option<Vec<String>>>()?;
    Some(scoped.join(", "))
}

/// The safe declarations of a `style` attribute, property names in lower case, `!important`
/// dropped.
fn parse_style(style: &str) -> Vec<(String, String)> {
    style
        .split(';')
        .filter_map(|declaration| {
            let (property, value) = declaration.split_once(':')?;
            let property = property.trim().to_ascii_lowercase();
            let mut value = value.trim().to_string();
            if let Some(at) = value.to_ascii_lowercase().find("!important") {
                value.truncate(at);
                value = value.trim().to_string();
            }
            // A negative margin pulls the mail over other content.
            let negative = property.starts_with("margin") && value.contains('-');
            (STYLE_PROPERTIES.contains(&property.as_str())
                && !value.is_empty()
                && !negative
                && safe_style_value(&value))
            .then_some((property, value))
        })
        .collect()
}

/// Whether an image is a tracking pixel: at most 1x1, or hidden.
fn is_tracking_pixel(width: Option<&str>, height: Option<&str>, style: Option<&str>) -> bool {
    let tiny = |length: Option<&str>| {
        length
            .and_then(|l| l.trim_end_matches("px").trim().parse::<f32>().ok())
            .is_some_and(|n| n <= 1.0)
    };
    let hidden = style.is_some_and(|s| {
        let s: String = s
            .to_ascii_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        s.contains("display:none") || s.contains("visibility:hidden")
    });
    (tiny(width) && tiny(height)) || hidden
}

/// Whether a style value names nothing to fetch or run.
fn safe_style_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ![
        "url(",
        "url (",
        "expression",
        "javascript:",
        "@import",
        "\\",
        "<",
        ">",
        "/*",
        "behavior",
        "-moz-binding",
        "image-set",
    ]
    .iter()
    .any(|bad| lower.contains(bad))
        && !value.chars().any(char::is_control)
}

/// Appends `s` escaped for XML text (or, with `attribute`, a double-quoted attribute value),
/// leaving out characters XML does not allow and control characters.
fn escape_into(out: &mut String, s: &str, attribute: bool) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(c),
            '\u{fffe}' | '\u{ffff}' => {}
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every result's body is the mail on its PAPER (the sheet it is read on).
    const PAPER: &str = "<body><div class=\"azmail-paper\">";
    const TAIL: &str = "</div></body></html>";

    /// The sanitized body, without the wrapper every result has.
    fn inner(html: &str) -> String {
        let out = sanitize(html).xhtml;
        let start = out.find(PAPER).map(|at| at + PAPER.len());
        assert!(start.is_some() && out.ends_with(TAIL), "{out}");
        out[start.unwrap_or(0)..out.len() - TAIL.len()].to_string()
    }

    /// The sanitized document's style sheet (`<head><style>`), as written.
    fn style_sheet(s: &Sanitized) -> String {
        let out = &s.xhtml;
        assert!(out.starts_with("<html><head><style>"), "{out}");
        let start = out.find("<style>").map(|at| at + "<style>".len());
        let end = out.find("</style>");
        match (start, end) {
            (Some(a), Some(b)) if a <= b => out[a..b].to_string(),
            _ => panic!("no style sheet in {out}"),
        }
    }

    /// A mail that says nothing about the dark mode was designed on white: it is shown on
    /// white paper with dark text in EITHER mode. The sheet holds no `prefers-color-scheme`
    /// condition, so the cascade, which matches those against the window's mode, finds nothing
    /// to change in a dark app: the paper stays white. Its links get a blue readable on white
    /// (the UA's dark-mode link colour is a pale one meant for a dark background).
    #[test]
    fn a_mail_without_dark_rules_is_white_paper_even_in_a_dark_app() {
        let s = sanitize("<p>Hi <a href=\"https://example.org\">link</a></p>");
        assert!(!s.has_dark_rules);
        let css = style_sheet(&s);
        assert!(
            css.contains(".azmail-paper { background-color: #ffffff; color: #1a1a1a;"),
            "{css}"
        );
        assert!(
            !css.contains("prefers-color-scheme"),
            "nothing follows the mode: {css}"
        );
        assert!(css.contains(".azmail-paper a { color: #0b57d0; }"), "{css}");
        assert_eq!(
            inner("<p>Hi</p>"),
            "<p>Hi</p>",
            "the paper wraps the mail's own markup"
        );
    }

    /// A mail that ships its own dark design keeps it: its `prefers-color-scheme: dark` rules
    /// stay (scoped to the paper), and the paper follows the app's mode - white with dark text
    /// in the light mode, dark with light text in the dark one - so the mail's dark rules fire
    /// on a dark sheet, as its author designed them.
    #[test]
    fn a_mail_with_dark_rules_keeps_them_and_its_paper_follows_the_mode() {
        let s = sanitize(
            "<style>.x { color: #111111 } @media (prefers-color-scheme: dark) { .x { color: \
             #eeeeee } }</style><p class=x>t</p>",
        );
        assert!(s.has_dark_rules);
        let css = style_sheet(&s);
        assert!(
            css.contains(".azmail-paper { background-color: #ffffff; color: #1a1a1a;"),
            "the light mode's paper: {css}"
        );
        assert!(
            css.contains(
                "@media (prefers-color-scheme: dark) { .azmail-paper { background-color: \
                 #1e1e1e; color: #e8e8e8; } }"
            ),
            "the dark mode's paper: {css}"
        );
        assert!(
            css.contains(
                "@media (prefers-color-scheme: dark) { .azmail-paper .x { color: #eeeeee; } }"
            ),
            "the mail's own dark rule: {css}"
        );
        assert!(
            !css.contains(".azmail-paper a { color"),
            "links take the UA's colour of the mode the paper is in: {css}"
        );
    }

    /// Dark support is also declared without a media query: a `color-scheme` (or the older
    /// `supported-color-schemes`) meta, or a `color-scheme` declaration, that names `dark`.
    #[test]
    fn a_color_scheme_meta_or_declaration_naming_dark_counts_as_dark_rules() {
        assert!(
            sanitize("<head><meta name=\"color-scheme\" content=\"light dark\"></head><p>t</p>")
                .has_dark_rules
        );
        assert!(
            sanitize("<meta name=supported-color-schemes content=\"light dark\"><p>t</p>")
                .has_dark_rules
        );
        assert!(
            sanitize("<style>:root { color-scheme: light dark; }</style><p>t</p>").has_dark_rules
        );
        assert!(!sanitize("<meta name=\"color-scheme\" content=\"light\"><p>t</p>").has_dark_rules);
        assert!(!sanitize("<style>.x { color: red }</style><p>t</p>").has_dark_rules);
    }

    /// A mail's style sheet keeps only what is safe, scoped to the paper: no imports, fonts,
    /// fetched URLs or positioning, no comment wrapping; the mail's `body` is the paper.
    #[test]
    fn a_mails_style_sheet_keeps_only_safe_rules_scoped_to_the_paper() {
        let s = sanitize(
            "<style><!-- @import url(x.css); @font-face { src: url(f.woff) } body { margin: 0; \
             background-color: #f4f4f4 } * { position: fixed } .b { color: red; background: \
             url(t.png) } --></style>t",
        );
        let css = style_sheet(&s);
        for gone in [
            "import",
            "url(",
            "font-face",
            "position",
            "<!--",
            "-->",
            "&lt;!--",
        ] {
            assert!(!css.contains(gone), "{gone} is gone: {css}");
        }
        assert!(
            css.contains(".azmail-paper { margin: 0; background-color: #f4f4f4; }"),
            "the mail's body is the paper: {css}"
        );
        assert!(css.contains(".azmail-paper .b { color: red; }"), "{css}");
        assert_eq!(inner("<style>p { color: red }</style>ok"), "ok");
    }

    #[test]
    fn well_formed_markup_passes_through() {
        assert_eq!(
            inner("<p>Hello <b>world</b></p>"),
            "<p>Hello <b>world</b></p>"
        );
        assert_eq!(inner(""), "");
    }

    #[test]
    fn scripts_styles_and_comments_are_gone_with_their_content() {
        assert_eq!(
            inner("<script>alert('<b>x</b>')</script><style>p { color: red }</style>ok"),
            "ok"
        );
        assert_eq!(
            inner("<!DOCTYPE html><!-- a comment --><!--[if mso]><table><![endif]-->t<?xml x?>"),
            "t"
        );
        // A `<body>` with nothing to say is no element of its own: the parser implies one
        // around every fragment.
        assert_eq!(
            inner("<head><title>T</title><meta charset=utf-8></head><body>b</body>"),
            "b"
        );
        assert_eq!(
            inner(
                "<svg><image href=\"https://x.example/a.png\"/></svg><iframe src=x>f</iframe>after"
            ),
            "after"
        );
    }

    #[test]
    fn html_that_is_not_xml_becomes_xml() {
        assert_eq!(inner("<P>a<P>b"), "<p>a</p><p>b</p>");
        assert_eq!(
            inner("line<br>next<BR/>end<hr>"),
            "line<br/>next<br/>end<hr/>"
        );
        // `</b>` closes the `<i>` in it, and the `<i>` goes on after it, as in a browser.
        assert_eq!(inner("<b><i>x</b>y</i>"), "<b><i>x</i></b><i>y</i>");
        assert_eq!(
            inner("<div><ul><li>a<li>b"),
            "<div><ul><li>a</li><li>b</li></ul></div>"
        );
        assert_eq!(
            inner("<table><tr><td>a<td>b<tr><td>c</table>"),
            "<table><tbody><tr><td>a</td><td>b</td></tr><tr><td>c</td></tr></tbody></table>"
        );
        assert_eq!(inner("<p>a<div>b</div>"), "<p>a</p><div>b</div>");
        // A stray `</div>` closes nothing; `</p>` alone is an empty paragraph.
        assert_eq!(inner("x</p></div>y"), "x<p></p>y");
    }

    #[test]
    fn text_is_escaped_and_references_are_decoded() {
        assert_eq!(inner("a < b and c>d"), "a &lt; b and c&gt;d");
        assert_eq!(
            inner("a&nbsp;b &amp; &lt;c&gt; &#8364; &#x263A; &copy; &uuml; &bogus; & x"),
            "a\u{a0}b &amp; &lt;c&gt; \u{20ac} \u{263a} \u{a9} \u{fc} &amp;bogus; &amp; x"
        );
        // `&#0;` is U+FFFD, as a browser reads it.
        assert_eq!(inner("&#150; &#0; x"), "\u{2013} \u{fffd} x");
        assert_eq!(inner("a\u{1}b\u{7f}c"), "abc");
    }

    #[test]
    fn only_safe_attributes_stay() {
        assert_eq!(
            inner("<div onclick=\"steal()\" class=big id=a title='t'>x</div>"),
            "<div>x</div>"
        );
        assert_eq!(
            inner("<a href=\"https://example.org/?a=1&b=2\">l</a>"),
            "<a href=\"https://example.org/?a=1&amp;b=2\">l</a>"
        );
        assert_eq!(inner("<a href=\"javascript:alert(1)\">x</a>"), "<a>x</a>");
        assert_eq!(inner("<a href=' JavaScript:x'>x</a>"), "<a>x</a>");
        assert_eq!(
            inner("<a href=mailto:ada@example.org>m</a>"),
            "<a href=\"mailto:ada@example.org\">m</a>"
        );
        assert_eq!(
            inner(
                "<table><tr><td bgcolor=\"#eee\" align=center width=50% colspan=2 nowrap>x</td>\
                 </tr></table>"
            ),
            "<table><tbody><tr><td bgcolor=\"#eee\" align=\"center\" width=\"50%\" colspan=\"2\" \
             nowrap=\"\">x</td></tr></tbody></table>"
        );
        assert_eq!(
            inner("<table width=600><tr><td>x</td></tr></table>"),
            "<table width=\"600\"><tbody><tr><td>x</td></tr></tbody></table>"
        );
        assert_eq!(inner("<p dir=rtl>x</p>"), "<p dir=\"rtl\">x</p>");
    }

    #[test]
    fn styles_keep_only_safe_properties() {
        assert_eq!(
            inner(
                "<span style=\"color: red; background: url(https://x.example/y.png); \
                 position: fixed; FONT-WEIGHT: bold !important\">t</span>"
            ),
            "<span style=\"color: red; font-weight: bold\">t</span>"
        );
        assert_eq!(
            inner("<p style=\"background-image:url( 'x' ); color:expression(alert(1)); margin:0\">t</p>"),
            "<p style=\"margin: 0\">t</p>"
        );
        assert_eq!(inner("<p style=\"\">t</p>"), "<p>t</p>");
    }

    #[test]
    fn legacy_tags_become_their_modern_twins() {
        assert_eq!(
            inner("<font color=\"#ff0000\" face=Arial>red</font><center>c</center>"),
            "<span style=\"color: #ff0000; font-family: Arial\">red</span><div style=\"text-align: \
             center\">c</div>"
        );
        assert_eq!(
            inner("<body style=\"margin:0\"><h1>T</h1></body>"),
            "<div style=\"margin: 0\"><h1>T</h1></div>"
        );
        assert_eq!(inner("<o:p>x</o:p><custom-tag>y</custom-tag>"), "xy");
    }

    /// The presentational attributes of tables, cells and blocks reach azul as they are: its
    /// own HTML hints map them to CSS as Chrome's do (core's `presentational_css`, the one
    /// generator), `<table align=center>` a centred table, `cellpadding` / `cellspacing` /
    /// `border` included - where the sanitizer kept a subset as `text-align` and dropped the
    /// rest. What becomes another element keeps its meaning as style: the body's `bgcolor` /
    /// `text`, a `<font>`'s `color` / `face` / `size`, a picture's `align` / `hspace` /
    /// `vspace` / `border`.
    #[test]
    fn presentational_attributes_mean_what_they_mean_in_a_browser() {
        assert_eq!(
            inner(
                "<table align=center width=600 cellpadding=8 cellspacing=0 border=1 \
                 bordercolor=#ccc bgcolor=ffffff><tr valign=top><td align=right width=50% \
                 height=40 nowrap>x</td></tr></table>"
            ),
            "<table align=\"center\" width=\"600\" cellpadding=\"8\" cellspacing=\"0\" \
             border=\"1\" bordercolor=\"#ccc\" bgcolor=\"ffffff\"><tbody><tr valign=\"top\"><td \
             align=\"right\" width=\"50%\" height=\"40\" nowrap=\"\">x</td></tr></tbody></table>"
        );
        assert_eq!(inner("<p align=center>c</p>"), "<p align=\"center\">c</p>");
        assert_eq!(
            inner("<body bgcolor=\"#f4f4f4\" text=\"#333333\"><p>b</p></body>"),
            "<div style=\"background-color: #f4f4f4; color: #333333\"><p>b</p></div>"
        );
        assert_eq!(
            inner("<font face=\"Arial, Helvetica\" size=\"2\" color=red>a</font><font size=+2>b</font>"),
            "<span style=\"font-family: Arial, Helvetica; font-size: 13px; color: red\">a</span>\
             <span style=\"font-size: 24px\">b</span>"
        );
        let pictures = sanitize_with(
            "<img src=\"https://cdn.example/a.png\" width=\"80\" height=\"60\" align=\"left\" \
             hspace=\"10\" vspace=\"4\" border=\"2\">",
            true,
        );
        assert!(
            pictures.xhtml.contains(
                "<img src=\"https://cdn.example/a.png\" width=\"80\" height=\"60\" style=\"float: \
                 left; margin-left: 10px; margin-right: 10px; margin-top: 4px; margin-bottom: \
                 4px; border-width: 2px; border-style: solid\"/>"
            ),
            "{}",
            pictures.xhtml
        );
    }

    #[test]
    fn remote_images_are_not_loaded() {
        let s = sanitize(
            "<img src=\"https://tracker.example/p.gif\" alt=\"Logo\"><img src=x width=1 height=1>",
        );
        assert_eq!(s.blocked_images, 2);
        assert!(!s.xhtml.contains("<img"), "{}", s.xhtml);
        assert!(!s.xhtml.contains("tracker"), "{}", s.xhtml);
        assert_eq!(
            inner("<img src=\"https://tracker.example/p.gif\" alt=\"Logo\">"),
            "<span style=\"color: #6b7385\">[image: Logo]</span>"
        );
        assert_eq!(
            inner("<img src=x>"),
            "<span style=\"color: #6b7385\">[image]</span>"
        );
    }

    /// azul turns parsed form controls into live widgets, so a phishing mail would show a working
    /// sign-in form: forms, fields and buttons go (a button with its label), their text stays.
    #[test]
    fn form_controls_go_and_a_buttons_label_with_it() {
        assert_eq!(
            inner(
                "<form action=\"https://x.example/login\">Name <input name=email \
                 value=\"ada@example.org\"><button>Sign in</button><select><option>A\
                 </select><textarea>t</textarea></form>after"
            ),
            "Name after"
        );
    }

    /// "Download pictures": web pictures come back as images the app fetches; a tracking
    /// pixel, an attached picture (`cid:`) and anything not on the web do not.
    #[test]
    fn downloaded_pictures_keep_their_web_images_and_list_them() {
        let html = "<p>a<img src=\"https://cdn.example/logo.png\" alt=\"Logo\" width=\"120\" \
                    height=\"40px\">b<img src=\"cid:part1@example\" alt=\"Inline\">\
                    <img src=\"https://t.example/o.gif\" width=1 height=1>\
                    <img src=\"javascript:alert(1)\"><img src=https://cdn.example/logo.png></p>";
        let off = sanitize(html);
        assert_eq!(off, sanitize_with(html, false), "off is the plain sanitizer");
        assert_eq!(off.blocked_images, 5);
        assert!(off.remote_images.is_empty());
        assert!(!off.xhtml.contains("<img"), "{}", off.xhtml);
        let on = sanitize_with(html, true);
        assert_eq!(on.remote_images, vec![String::from("https://cdn.example/logo.png")]);
        assert!(
            on.xhtml.contains(
                "<img src=\"https://cdn.example/logo.png\" alt=\"Logo\" width=\"120\" height=\"40\"/>"
            ),
            "{}",
            on.xhtml
        );
        assert!(!on.xhtml.contains("t.example"), "a tracking pixel stays out: {}", on.xhtml);
        assert!(!on.xhtml.contains("javascript"), "{}", on.xhtml);
        assert!(on.xhtml.contains("[image: Inline]"), "an attached picture: {}", on.xhtml);
        assert_eq!(on.blocked_images, 3, "the pixel, the attached one and the script one");
    }

    /// A 1x1 or hidden image is a tracking pixel: blocked, and not even a placeholder shows.
    #[test]
    fn a_tracking_pixel_leaves_no_placeholder() {
        let pixels = "a<img src=\"https://t.example/o.gif\" width=\"1\" height=\"1px\">b\
                      <img src=x style=\"display: none\">c<img src=y width=0 height=0>d";
        assert_eq!(sanitize(pixels).blocked_images, 3);
        assert_eq!(inner(pixels), "abcd");
        // A divider line is an image, not a pixel.
        assert_eq!(
            inner("<img src=x width=600 height=1>"),
            "<span style=\"color: #6b7385\">[image]</span>"
        );
    }

    /// Negative margins pull content over other content (and over the reading pane's own
    /// header): they go.
    #[test]
    fn negative_margins_are_dropped() {
        assert_eq!(
            inner("<div style=\"margin-top: -40px; margin: 0 -10px; padding: 4px; margin-left: 2px\">x</div>"),
            "<div style=\"padding: 4px; margin-left: 2px\">x</div>"
        );
    }

    /// The mail is read with the engine's HTML parser (`Xml::create_from_html`,
    /// DEDUP_EDITORS B20), so the sanitized tree is the one a browser builds - where AzMail's
    /// own tokenizer built a simpler one: a stray end tag in a table cell does not close the
    /// table around it, a formatting element left open goes on in the next paragraph, `</p>`
    /// alone is an empty paragraph, a row gets the `<tbody>` it implies, and every one of the
    /// HTML Standard's 2231 named references decodes (with the legacy ones also without `;`).
    #[test]
    fn the_sanitizer_builds_the_tree_a_browser_builds() {
        assert_eq!(
            inner("<div><table><tr><td>a</div>b</td></tr></table></div>"),
            "<div><table><tbody><tr><td>ab</td></tr></tbody></table></div>",
            "a stray </div> inside a cell closes nothing"
        );
        assert_eq!(
            inner("<p><b>x<p>y"),
            "<p><b>x</b></p><p><b>y</b></p>",
            "the bold left open goes on in the next paragraph"
        );
        assert_eq!(inner("x</p>y"), "x<p></p>y", "</p> alone is an empty paragraph");
        assert_eq!(
            inner("&star; &ThickSpace; &lrarr; &copy 2026"),
            "\u{2606} \u{205f}\u{200a} \u{21c6} \u{a9} 2026",
            "the named references a browser knows"
        );
        assert_eq!(
            inner("<td>loose</td><tr>row"),
            "looserow",
            "a cell or a row outside any table is ignored, its content stays"
        );
    }

    #[test]
    fn nesting_is_bounded() {
        let deep = "<div>".repeat(1000) + "x";
        let out = inner(&deep);
        assert!(out.matches("<div>").count() <= 200, "{}", out.len());
        assert!(out.contains('x'));
        assert_eq!(out.matches("<div>").count(), out.matches("</div>").count());
    }
}
