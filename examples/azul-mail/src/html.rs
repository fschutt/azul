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
//!   `@media`), each scoped to the paper (below), its class selectors renamed to the message's
//!   class prefix; `@import`, `@font-face`, any URL, positioning and the `<!--` / `-->`
//!   wrapping go;
//! - images are NOT loaded (remote images are off): each becomes a grey `[image: alt]` text,
//!   and a tracking pixel (1x1 or hidden) not even that - except the mail's own pictures (a
//!   `cid:` naming one of its parts, [`PictureOptions::inline`]), shown at once under a key of
//!   this message's own ([`Sanitized::inline_key`]), and, after "download pictures", its web
//!   pictures;
//! - before anything is shown, the engine's pre-pass over the parsed mail
//!   (`Xml::scan_external_resources`, after the parse and before layout) lists what it would
//!   fetch from the web - pictures, fonts, style sheets ([`Sanitized::remote`]);
//! - only presentational tags stay (`p div span b i u a table tr td ul li h1 ...`); `font`
//!   becomes a `span`, `center` a centred `div`, the `body` is the MAIL BODY (below), and any
//!   other tag is dropped with its text kept;
//! - attributes: `class` (each name behind the message's own prefix,
//!   [`Sanitized::class_prefix`], so the mail's sheet applies as written and can never reach
//!   the app's `__azmail_` classes), `href` (http, https and mailto only), `colspan`,
//!   `rowspan`, `dir`, and `style`
//!   with a short list of properties whose values name no URL (and no negative margin); the
//!   presentational ones of tables, cells and blocks (`align`, `valign`, `width`, `height`,
//!   `bgcolor`, `border`, `bordercolor`, `cellpadding`, `cellspacing`, `nowrap`) as written, for
//!   azul's own HTML hints; on an element renamed here (`font`) and on a kept picture, what
//!   they mean becomes style (`color`, `face`, `size`, `align`, `hspace`, `vspace`, `border`);
//!   a colour is one value (no `;`, `{`, `}`);
//! - nesting deeper than 200 keeps the text only;
//! - the text (its character references decoded by the parser) and every attribute value are
//!   written by azul's one encoder (`Xml::encode_text` / `Xml::encode_attribute`).
//!
//! The mail is read on PAPER, a `<div class="__azmail_paper">` ([`ids::PAPER`]): a mail that
//! says nothing about the dark mode was designed on white, so its paper is white with dark text
//! in either mode (and its links a blue readable on white). A mail with its own dark rules
//! ([`Sanitized::has_dark_rules`]) keeps them, and its paper follows the app's mode through a
//! `prefers-color-scheme` rule of its own, so the mail's dark rules fire on a dark sheet. The
//! paper is an inline-block at least the pane wide - wider when the mail is (a newsletter's
//! 600 px table), as a browser's canvas grows with its content; the app scrolls it sideways.
//!
//! On the paper, the MAIL BODY `<div class="__azmail_mail_body">` ([`ids::MAIL_BODY`]) is the
//! mail's `<body>`: its 12 px padding is the margin around the mail, the body's classes, `dir`
//! and `style` are on it, the sheet's `body` rules apply to it (`html` / `:root` rules to the
//! paper), and what the body's `bgcolor` / `text` say is a rule before the mail's own rules,
//! which win over it, as an author rule wins over a presentational hint in a browser.
//!
//! The result is `<html><head><style>...</style></head><body><div class="__azmail_paper"><div
//! class="__azmail_mail_body">...</div></div></body></html>` for `Dom::create_from_parsed_xml`.

use azul::{
    dom::{XmlNode, XmlNodeChild},
    xml::{ExternalResourceKind, Xml},
};

use azul_appkit::css::{self as sheet, CssItem};

use crate::ids;

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
    /// The prefix this mail's own classes carry (`class="big"` becomes `class="<prefix>big"`,
    /// and its sheet's `.big` the same): one per message, the same with pictures on or off,
    /// never starting with the app's `__azmail_`.
    pub class_prefix: String,
    /// What the mail would fetch from the web (the engine's pre-pass over the parsed mail):
    /// the same with pictures on or off.
    pub remote: RemoteContent,
    /// The mail's own pictures shown (the Content-IDs of [`PictureOptions::inline`] its `cid:`
    /// pictures name), each once, in order: the app decodes those parts and registers each
    /// under [`Sanitized::inline_key`].
    pub inline_images: Vec<String>,
}

impl Sanitized {
    /// The image-cache key of the mail's own picture `content_id`: `cid:<class prefix><id>`,
    /// so two mails' `image001.png` never meet in the cache.
    pub fn inline_key(&self, content_id: &str) -> String {
        inline_key(&self.class_prefix, content_id)
    }
}

fn inline_key(prefix: &str, content_id: &str) -> String {
    format!("cid:{prefix}{content_id}")
}

/// What a mail would fetch from the web: found by the engine's pre-pass over the parsed mail
/// (`Xml::scan_external_resources`, after the parse and before layout); http and https
/// addresses only, each once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteContent {
    /// Pictures: `<img src>` and `srcset`, `background`, a CSS `url()` of a picture, icons.
    pub images: Vec<String>,
    /// Fonts: an `@font-face` `url()`, `<link as="font">`.
    pub fonts: Vec<String>,
    /// Style sheets: `<link rel="stylesheet">`, `@import`.
    pub stylesheets: Vec<String>,
}

impl RemoteContent {
    /// "4 pictures, 1 font and 2 style sheets" (what is there); empty when there is nothing.
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (count, one, many) in [
            (self.images.len(), "picture", "pictures"),
            (self.fonts.len(), "font", "fonts"),
            (self.stylesheets.len(), "style sheet", "style sheets"),
        ] {
            match count {
                0 => {}
                1 => parts.push(format!("1 {one}")),
                n => parts.push(format!("{n} {many}")),
            }
        }
        match parts.pop() {
            None => String::new(),
            Some(last) if parts.is_empty() => last,
            Some(last) => format!("{} and {last}", parts.join(", ")),
        }
    }
}

/// The pre-pass: what the parsed mail references on the web, by kind.
pub fn remote_content(document: &Xml) -> RemoteContent {
    let mut remote = RemoteContent::default();
    for resource in document.scan_external_resources().iter() {
        let url = resource.url.as_str().trim();
        let lower = url.to_ascii_lowercase();
        if !(lower.starts_with("https://") || lower.starts_with("http://")) {
            continue;
        }
        let list = match resource.kind {
            ExternalResourceKind::Image | ExternalResourceKind::Icon => &mut remote.images,
            ExternalResourceKind::Font => &mut remote.fonts,
            ExternalResourceKind::Stylesheet => &mut remote.stylesheets,
            _ => continue,
        };
        if !list.iter().any(|known| known == url) {
            list.push(url.to_string());
        }
    }
    remote
}

/// What the sanitizer does with a mail's pictures. `Default`: none shown (the plain
/// [`sanitize`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PictureOptions {
    /// The reader clicked "download pictures": the web pictures are kept.
    pub web: bool,
    /// The Content-IDs of the mail's own picture parts (`crate::message::inline_pictures`):
    /// a `cid:` picture naming one of them is always shown.
    pub inline: Vec<String>,
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
    sanitize_mail(
        html,
        &PictureOptions {
            web: pictures,
            inline: Vec::new(),
        },
    )
}

/// [`sanitize`] with the pictures `options` say: the web ones after "download pictures", the
/// mail's own (`cid:`) always. The mail is parsed once; the pre-pass lists its web content
/// right after the parse ([`Sanitized::remote`]).
pub fn sanitize_mail(html: &str, options: &PictureOptions) -> Sanitized {
    let mut s = Sanitizer {
        pictures: options.web,
        inline: options.inline.clone(),
        prefix: class_prefix(html),
        ..Sanitizer::default()
    };
    let document = Xml::create_from_html(html);
    let remote = remote_content(&document);
    s.children(&document.root);
    Sanitized {
        remote,
        ..s.finish()
    }
}

/// Tags deeper than this keep their text only.
const MAX_DEPTH: usize = 200;
/// How a blocked image's placeholder looks.
const BLOCKED_IMAGE_STYLE: &str = "color: #6b7385";

/// The paper in the light mode - and always, for a mail without dark rules: the colours of the
/// paper's class ([`ids::PAPER`]), after its box ([`PAPER_BOX`]).
const PAPER_LIGHT: &str = "background-color: #ffffff; color: #1a1a1a;";
/// The paper in the dark mode, for a mail with dark rules (inside a `prefers-color-scheme:
/// dark` rule).
const PAPER_DARK: &str = "background-color: #1e1e1e; color: #e8e8e8;";
/// [`PAPER_LIGHT`] under flora (inside an `@theme(flora)` rule): flora's field paper
/// (`--fl-fld`) under its ink - flora never lays a pure white field - kept light at night for
/// a mail without dark rules, whose own colours assume a light page. Only the paper itself:
/// a theme block outranks the mail's own rules, so nothing the mail styles is touched.
const PAPER_LIGHT_FLORA: &str = "background-color: #FBFAF6; color: #262521;";
/// [`PAPER_DARK`] under flora: the night field paper and the night ink.
const PAPER_DARK_FLORA: &str = "background-color: #1D1D1D; color: #E7E7E7;";
/// The paper's box: at least the pane wide, wider when the mail is (a browser's canvas grows
/// with its content the same way).
const PAPER_BOX: &str = "display: inline-block; min-width: 100%; box-sizing: border-box;";
/// The mail's body ([`ids::MAIL_BODY`]) inside the paper: the margin around the mail (a
/// browser's body has one too), inside the paper so a short mail's paper is the pane wide.
const MAIL_BODY_BOX: &str = "padding: 12px;";
/// A link on white paper: a blue readable on white (the UA's dark-mode link colour is not).
/// Before the mail's own rules, which win over it.
const PAPER_LINK: &str = "color: #0b57d0;";

/// The prefix of one message's classes ([`Sanitized::class_prefix`]): `m`, eight hex digits of
/// a hash of the mail's HTML, `_` - the same for the same mail (pictures on or off), another
/// for another one, and never the app's `__azmail_`.
fn class_prefix(html: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    html.hash(&mut hasher);
    format!("m{:08x}_", hasher.finish() & 0xffff_ffff)
}

/// Whether a mail's class name stays: one a selector names without an escape (letters,
/// digits, `-`, `_`, any non-ASCII letter).
fn is_class_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '-' || c == '_' || (!c.is_ascii() && !c.is_control())
        })
}

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
    /// The Content-IDs of the mail's own pictures that may be shown.
    inline: Vec<String>,
    /// The mail's own pictures shown, each once.
    inline_images: Vec<String>,
    /// The prefix of this message's classes ([`class_prefix`]).
    prefix: String,
    /// What the mail's `<body>` said ([`body_of`]); it is written by `finish`.
    body: Body,
}

impl Sanitizer {
    /// The nodes of the parsed mail, in order.
    fn children(&mut self, nodes: &[XmlNodeChild]) {
        for node in nodes {
            match node {
                XmlNodeChild::Text(text) => push_text(&mut self.out, text.as_str()),
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
        if name == "body" {
            // The mail's body is the mail-body element `finish` writes around the content (the
            // parser implies one `<body>` around every document): its attributes go there.
            self.body = body_of(&attributes, &self.prefix);
            self.children(&element.children);
            return;
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
                push_attributes(&mut kept, name, tag, &attributes, &self.prefix);
                if self.depth >= MAX_DEPTH {
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
        let (safe, dark) = sanitize_style_sheet(css, &self.prefix);
        self.styles.push_str(&safe);
        self.has_dark_rules |= dark;
    }

    /// An `<img>`: not loaded (a placeholder text, or nothing for a tracking pixel), or - with
    /// pictures on - kept for the app to fetch.
    fn image(&mut self, attributes: &[(&str, &str)]) {
        self.blocked_images += 1;
        let attribute = |name: &str| attribute_of(attributes, name);
        if is_tracking_pixel(attribute("width"), attribute("height"), attribute("style")) {
            return;
        }
        let alt = attribute("alt").filter(|a| !a.is_empty());
        let src = attribute("src").unwrap_or("");
        let scheme = src.to_ascii_lowercase();
        // The mail's own picture (`cid:` naming one of its parts): shown at once, under this
        // message's own key.
        let own = scheme
            .strip_prefix("cid:")
            .and(src.get(4..))
            .map(str::trim)
            .filter(|cid| self.inline.iter().any(|c| c == cid));
        if let Some(cid) = own {
            self.blocked_images -= 1;
            let key = inline_key(&self.prefix, cid);
            self.picture(&key, alt, attributes);
            if !self.inline_images.iter().any(|c| c == cid) {
                self.inline_images.push(cid.to_string());
            }
            return;
        }
        if self.pictures && (scheme.starts_with("https://") || scheme.starts_with("http://")) {
            // Loaded after all: an image the app fetches and caches under its src.
            self.blocked_images -= 1;
            self.picture(src, alt, attributes);
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
        push_text(&mut self.out, &label);
        self.out.push_str("</span>");
    }

    /// A shown picture: `<img src alt width height/>` with what its legacy attributes mean as
    /// style; `src` is where the image cache has it (the web address, or a `cid:` key).
    fn picture(&mut self, src: &str, alt: Option<&str>, attributes: &[(&str, &str)]) {
        let attribute = |name: &str| attribute_of(attributes, name);
        self.out.push_str("<img src=\"");
        push_attribute(&mut self.out, src);
        self.out.push('"');
        if let Some(alt) = alt {
            self.out.push_str(" alt=\"");
            push_attribute(&mut self.out, alt);
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
            push_attribute(&mut self.out, &styles.join("; "));
            self.out.push('"');
        }
        self.out.push_str("/>");
    }

    fn finish(self) -> Sanitized {
        // The paper first, the mail's rules after it (they win where they say something).
        let paper = ids::PAPER;
        let paper = paper.as_str();
        let mail_body = ids::MAIL_BODY;
        let mail_body = mail_body.as_str();
        let mut sheet = format!(".{paper} {{ {PAPER_BOX} {PAPER_LIGHT} }} ");
        // Under flora, flora's paper (the paper element only - see `PAPER_LIGHT_FLORA`).
        sheet.push_str(&format!("@theme(flora) {{ .{paper} {{ {PAPER_LIGHT_FLORA} }} }} "));
        if self.has_dark_rules {
            sheet.push_str(&format!(
                "@media (prefers-color-scheme: dark) {{ .{paper} {{ {PAPER_DARK} }} }} "
            ));
            sheet.push_str(&format!(
                "@theme(flora) {{ @media (prefers-color-scheme: dark) {{ .{paper} {{ \
                 {PAPER_DARK_FLORA} }} }} }} "
            ));
        } else {
            sheet.push_str(&format!(".{paper} a {{ {PAPER_LINK} }} "));
        }
        sheet.push_str(&format!(".{mail_body} {{ {MAIL_BODY_BOX} }} "));
        // The body's `bgcolor` / `text` before the mail's rules: those win over them.
        if !self.body.hints.is_empty() {
            sheet.push_str(&format!(".{mail_body} {{ {}}} ", self.body.hints));
        }
        sheet.push_str(&self.styles);
        let mut xhtml = String::from("<html><head><style>");
        push_text(&mut xhtml, sheet.trim_end());
        xhtml.push_str("</style></head><body><div class=\"");
        xhtml.push_str(paper);
        xhtml.push_str("\"><div class=\"");
        xhtml.push_str(mail_body);
        xhtml.push_str(&self.body.classes);
        xhtml.push('"');
        xhtml.push_str(&self.body.attributes);
        xhtml.push('>');
        xhtml.push_str(&self.out);
        xhtml.push_str("</div></div></body></html>");
        Sanitized {
            xhtml,
            blocked_images: self.blocked_images,
            has_dark_rules: self.has_dark_rules,
            remote_images: self.remote_images,
            class_prefix: self.prefix,
            remote: RemoteContent::default(),
            inline_images: self.inline_images,
        }
    }
}

/// The value of the attribute `name` (lower case, as the parser gives it), trimmed.
fn attribute_of<'a>(attributes: &[(&'a str, &'a str)], name: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|&&(n, _)| n == name)
        .map(|&(_, v)| v.trim())
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

/// What the mail's `<body>` says, for the element that IS its body in the output
/// ([`ids::MAIL_BODY`], written by `finish`).
#[derive(Debug, Default)]
struct Body {
    /// ` <prefix><name>` for each of its classes that stays.
    classes: String,
    /// Its `dir` and `style` as written attributes (` dir="rtl" style="..."`).
    attributes: String,
    /// What its `bgcolor` / `text` mean (`background-color: ...; color: ...; `): a rule BEFORE
    /// the mail's own rules, which win over it as an author rule wins over a presentational
    /// hint in a browser.
    hints: String,
}

/// The mail's `<body>` (see [`Body`]).
fn body_of(attributes: &[(&str, &str)], prefix: &str) -> Body {
    let mut body = Body::default();
    let mut rest: Vec<(&str, &str)> = Vec::new();
    for &(name, value) in attributes {
        let value = value.trim();
        match name {
            "class" => {
                for class in value.split_ascii_whitespace().filter(|c| is_class_name(c)) {
                    body.classes.push(' ');
                    body.classes.push_str(prefix);
                    body.classes.push_str(class);
                }
            }
            "bgcolor" | "text" => {
                if !value.is_empty() && sheet::safe_value(value) {
                    let property = if name == "bgcolor" {
                        "background-color"
                    } else {
                        "color"
                    };
                    body.hints.push_str(&format!("{property}: {value}; "));
                }
            }
            _ => rest.push((name, value)),
        }
    }
    // As a renamed element (`div`): `dir` and `style` stay, nothing presentational.
    push_attributes(&mut body.attributes, "body", "div", &rest, prefix);
    body
}

/// Writes the attributes an output tag keeps: `class` (each name that stays behind the
/// message's `prefix`), `href`, `colspan`, `rowspan`, `dir`, the presentational attributes
/// ([`PRESENTATIONAL`]) for azul's HTML hints, and one `style`: what a renamed element said
/// with its attributes, then the safe style declarations.
fn push_attributes(
    out: &mut String,
    source: &str,
    tag: &str,
    attributes: &[(&str, &str)],
    prefix: &str,
) {
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
            "class" => {
                let classes: Vec<String> = value
                    .split_ascii_whitespace()
                    .filter(|class| is_class_name(class))
                    .map(|class| format!("{prefix}{class}"))
                    .collect();
                if !classes.is_empty() {
                    kept.push(("class", classes.join(" ")));
                }
            }
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
            "color" | "face" if source == "font" => {
                if !value.is_empty() && sheet::safe_value(value) {
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
        push_attribute(out, &value);
        out.push('"');
    }
    if !styles.is_empty() {
        let joined = styles
            .iter()
            .map(|(p, v)| format!("{p}: {v}"))
            .collect::<Vec<_>>()
            .join("; ");
        out.push_str(" style=\"");
        push_attribute(out, &joined);
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
/// `@media` blocks of such rules, every selector scoped to the paper and its classes renamed to
/// the message's `prefix` ([`scope_selectors`]). Gone: every other at-rule (`@import`, `@font-face`, `@page`,
/// `@keyframes`, `@supports`, `@charset`), comments, the `<!--` / `-->` wrapping, a rule
/// left with no declaration. Dark rules: a kept `@media` whose condition says
/// `prefers-color-scheme: dark`, or a `color-scheme` declaration that names `dark`.
fn sanitize_style_sheet(css: &str, prefix: &str) -> (String, bool) {
    let plain = sheet::strip_comments(css);
    let mut out = String::new();
    let mut dark = false;
    sanitize_rules(&plain, &mut out, &mut dark, false, prefix);
    (out, dark)
}

/// The rules of `css` (a sheet, or the inside of an `@media` block when `nested`), made safe
/// into `out`: the policy over azul-appkit's style-sheet reader (`azul_appkit::css`).
fn sanitize_rules(css: &str, out: &mut String, dark: &mut bool, nested: bool, prefix: &str) {
    sheet::for_each_item(css, &mut |item| match item {
        // `@import`, `@charset`, `@namespace`: gone.
        CssItem::AtStatement { .. } => {}
        CssItem::AtBlock {
            name,
            condition,
            body,
        } => {
            if name == "media" && !nested && sheet::safe_value(condition) {
                let mut inner = String::new();
                sanitize_rules(body, &mut inner, dark, true, prefix);
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
        }
        CssItem::Rule {
            selectors,
            declarations,
        } => {
            // `supported-color-schemes`: Apple Mail's older name of `color-scheme`.
            if declarations.iter().any(|(property, value)| {
                matches!(property.as_str(), "color-scheme" | "supported-color-schemes")
                    && names_dark(value)
            }) {
                *dark = true;
            }
            let declarations = safe_declarations(declarations);
            if let (false, Some(scoped)) = (
                declarations.is_empty(),
                scope_selectors(selectors, prefix),
            ) {
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
        }
    });
}

/// A rule's selectors scoped to the paper, so a mail's sheet cannot restyle the app around
/// it: `body` IS the mail-body element ([`ids::MAIL_BODY`]), `html` and `:root` the paper,
/// every other selector is a descendant of the paper,
/// and its classes are the message's ([`rename_classes`]): `.x` becomes
/// `.__azmail_paper .<prefix>x`. `None` for a selector list that is empty or holds what no
/// selector needs (`<`, `\`, `{`, `@`, a URL or a control character).
fn scope_selectors(selectors: &str, prefix: &str) -> Option<String> {
    let paper = ids::PAPER;
    let paper = paper.as_str();
    let mail_body = ids::MAIL_BODY;
    let mail_body = mail_body.as_str();
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
            let sel = rename_classes(sel, prefix);
            let first_len = sel.find(char::is_whitespace).unwrap_or(sel.len());
            let (first, tail) = sel.split_at(first_len);
            Some(match first.to_ascii_lowercase().as_str() {
                "body" => format!(".{mail_body}{tail}"),
                "html" | ":root" => format!(".{paper}{tail}"),
                _ => format!(".{paper} {sel}"),
            })
        })
        .collect::<Option<Vec<String>>>()?;
    Some(scoped.join(", "))
}

/// One selector with each class it names (a `.` that starts a name, outside an attribute
/// selector and its quotes) renamed to the message's: `p.x:not(.y)` becomes
/// `p.<prefix>x:not(.<prefix>y)`, `a[href$=".pdf"]` stays. A `.` before a digit starts no
/// class (the selector is invalid, as in a browser), so it stays as written.
fn rename_classes(selector: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(selector.len() + prefix.len());
    let mut chars = selector.chars().peekable();
    let mut brackets = 0_usize;
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        out.push(c);
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '[') => brackets += 1,
            (None, ']') => brackets = brackets.saturating_sub(1),
            (None, '.') if brackets == 0 => {
                let starts_name = chars.peek().is_some_and(|&n| {
                    n.is_ascii_alphabetic() || n == '_' || n == '-' || !n.is_ascii()
                });
                if starts_name {
                    out.push_str(prefix);
                }
            }
            _ => {}
        }
    }
    out
}

/// The safe declarations of a `style` attribute, property names in lower case, `!important`
/// dropped (read by azul-appkit's `parse_declarations`: a `;` in quotes or parentheses ends
/// nothing).
fn parse_style(style: &str) -> Vec<(String, String)> {
    safe_declarations(sheet::parse_declarations(style))
}

/// The declarations AzMail keeps: a property of [`STYLE_PROPERTIES`] with a value that names
/// nothing to fetch or run (`azul_appkit::css::safe_value`), and no negative margin.
fn safe_declarations(declarations: Vec<(String, String)>) -> Vec<(String, String)> {
    declarations
        .into_iter()
        .filter(|(property, value)| {
            // A negative margin pulls the mail over other content.
            let negative = property.starts_with("margin") && value.contains('-');
            STYLE_PROPERTIES.contains(&property.as_str()) && !negative && sheet::safe_value(value)
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

/// Appends `s` as XML text, written by azul's one encoder (`Xml::encode_text`: `& < >` as
/// references, what XML 1.0 cannot carry left out).
fn push_text(out: &mut String, s: &str) {
    out.push_str(Xml::encode_text(s).as_str());
}

/// Appends `s` as a quoted attribute value, written by azul's one encoder
/// (`Xml::encode_attribute`: [`push_text`]'s references and both quotes).
fn push_attribute(out: &mut String, s: &str) {
    out.push_str(Xml::encode_attribute(s).as_str());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every result's body is the mail's body on its PAPER (the sheet it is read on).
    const PAPER: &str =
        "<body><div class=\"__azmail_paper\"><div class=\"__azmail_mail_body\">";
    const TAIL: &str = "</div></div></body></html>";

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
            css.contains(&format!(
                ".__azmail_paper {{ {PAPER_BOX} background-color: #ffffff; color: #1a1a1a; }}"
            )),
            "{css}"
        );
        assert!(
            !css.contains("prefers-color-scheme"),
            "nothing follows the mode: {css}"
        );
        assert!(css.contains(".__azmail_paper a { color: #0b57d0; }"), "{css}");
        assert_eq!(
            inner("<p>Hi</p>"),
            "<p>Hi</p>",
            "the paper wraps the mail's own markup"
        );
    }

    /// Under flora a mail lies on flora's field paper in flora's ink - flora never lays a pure
    /// white field - and a mail with dark rules on flora's night paper at night. Only the
    /// paper element itself: a theme block outranks the mail's own rules, so it names nothing
    /// the mail styles.
    #[test]
    fn under_flora_a_mail_lies_on_floras_field_paper() {
        let css = style_sheet(&sanitize("<p>Hi</p>"));
        assert!(
            css.contains(&format!(
                "@theme(flora) {{ .__azmail_paper {{ {PAPER_LIGHT_FLORA} }} }}"
            )),
            "{css}"
        );
        assert!(!css.contains("prefers-color-scheme"), "still light at night: {css}");
        let dark = style_sheet(&sanitize(
            "<style>@media (prefers-color-scheme: dark) { .x { color: #eeeeee } }</style>\
             <p class=x>t</p>",
        ));
        assert!(
            dark.contains(&format!(
                "@theme(flora) {{ @media (prefers-color-scheme: dark) {{ .__azmail_paper {{ \
                 {PAPER_DARK_FLORA} }} }} }}"
            )),
            "{dark}"
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
            css.contains(&format!(
                ".__azmail_paper {{ {PAPER_BOX} background-color: #ffffff; color: #1a1a1a; }}"
            )),
            "the light mode's paper: {css}"
        );
        assert!(
            css.contains(
                "@media (prefers-color-scheme: dark) { .__azmail_paper { background-color: \
                 #1e1e1e; color: #e8e8e8; } }"
            ),
            "the dark mode's paper: {css}"
        );
        let x = format!("{}x", s.class_prefix);
        assert!(
            css.contains(&format!(
                "@media (prefers-color-scheme: dark) {{ .__azmail_paper .{x} {{ color: #eeeeee; }} }}"
            )),
            "the mail's own dark rule: {css}"
        );
        assert!(
            !css.contains(".__azmail_paper a { color"),
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
            css.contains(".__azmail_mail_body { margin: 0; background-color: #f4f4f4; }"),
            "the mail's body is the mail-body element: {css}"
        );
        assert!(
            css.contains(&format!(".__azmail_paper .{}b {{ color: red; }}", s.class_prefix)),
            "{css}"
        );
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
        // What XML 1.0 cannot carry is left out (a C0 control); DEL is XML and stays, as in
        // the browser's tree - the one encoder decides (HYGIENE F5).
        assert_eq!(inner("a\u{1}b\u{7f}c"), "ab\u{7f}c");
    }

    /// The markup is written by azul's one encoder (`Xml::encode_text` /
    /// `Xml::encode_attribute`, HYGIENE F5) - no escaper of AzMail's own: in an attribute both
    /// quotes are references, in text neither is.
    #[test]
    fn markup_is_written_by_azuls_one_encoder() {
        assert_eq!(
            inner("<a href=\"https://x.example/?q='a'&r=&quot;b&quot;\">'x' \"y\"</a>"),
            "<a href=\"https://x.example/?q=&apos;a&apos;&amp;r=&quot;b&quot;\">'x' \"y\"</a>"
        );
    }

    #[test]
    fn only_safe_attributes_stay() {
        let html = "<div onclick=\"steal()\" class=big id=a title='t'>x</div>";
        assert_eq!(
            inner(html),
            format!("<div class=\"{}big\">x</div>", sanitize(html).class_prefix),
            "the class stays behind the mail's prefix, the handler, id and title go"
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

    /// A `;` inside quotes (`content: ";"`, a font name) is part of its value: the declaration
    /// is read whole (and dropped, as a value holding a `;` could end its declaration where
    /// it is written out), never split into a half value with an open quote.
    #[test]
    fn a_semicolon_inside_quotes_never_splits_a_declaration_into_a_half_value() {
        let s = sanitize(
            "<style>.x { content: \";\"; font-family: \"Open;Sans\", serif; color: red }</style>\
             <p class=x>t</p>",
        );
        let css = style_sheet(&s);
        assert!(
            css.contains(&format!(".__azmail_paper .{}x {{ color: red; }}", s.class_prefix)),
            "{css}"
        );
        assert!(!css.contains("Open"), "no half font name: {css}");
        assert_eq!(
            inner("<span style=\"content: ';'; font-family: 'Open;Sans', serif; color: red\">t</span>"),
            "<span style=\"color: red\">t</span>"
        );
    }

    #[test]
    fn legacy_tags_become_their_modern_twins() {
        assert_eq!(
            inner("<font color=\"#ff0000\" face=Arial>red</font><center>c</center>"),
            "<span style=\"color: #ff0000; font-family: Arial\">red</span><div style=\"text-align: \
             center\">c</div>"
        );
        assert!(
            sanitize("<body style=\"margin:0\"><h1>T</h1></body>")
                .xhtml
                .contains("<div class=\"__azmail_mail_body\" style=\"margin: 0\"><h1>T</h1></div>"),
            "the body's style is on the mail-body element"
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
        let body = sanitize("<body bgcolor=\"#f4f4f4\" text=\"#333333\"><p>b</p></body>");
        assert!(
            style_sheet(&body)
                .contains(".__azmail_mail_body { background-color: #f4f4f4; color: #333333; }"),
            "{}",
            body.xhtml
        );
        assert_eq!(inner("<body bgcolor=\"#f4f4f4\" text=\"#333333\"><p>b</p></body>"), "<p>b</p>");
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

    /// A mail's own classes stay, behind a prefix of the message's own, and its sheet's class
    /// selectors are renamed to match (user ruling 2026-10-02): the mail's CSS applies as its
    /// author wrote it - where the classes went and their rules matched nothing - and can
    /// never reach the app's `__azmail_` classes, not even one the mail names itself. A class
    /// a selector could not name without an escape goes; attribute selectors stay as written.
    #[test]
    fn a_mails_classes_stay_behind_its_own_prefix_and_its_rules_follow_them() {
        let s = sanitize(
            "<style>.big { color: red } p.MsoNormal, .a .b:not(.c) { margin: 0 } \
             a[href$=\".pdf\"] { color: blue } .__azmail_paper { color: #ff0000 }</style>\
             <p class=\"MsoNormal big\">t</p><div class=\" a  __azmail_paper md:flex \">x</div>\
             <font class=f color=red>y</font><span class=\"\">z</span>",
        );
        let p = s.class_prefix.as_str();
        assert!(!p.is_empty(), "{}", s.xhtml);
        assert!(!p.starts_with("__azmail_"), "{p}");
        assert!(
            s.xhtml.contains(&format!("<p class=\"{p}MsoNormal {p}big\">t</p>")),
            "{}",
            s.xhtml
        );
        assert!(
            s.xhtml.contains(&format!("<div class=\"{p}a {p}__azmail_paper\">x</div>")),
            "the mail's own `__azmail_paper` is renamed too, `md:flex` goes: {}",
            s.xhtml
        );
        assert!(
            s.xhtml.contains(&format!("<span class=\"{p}f\" style=\"color: red\">y</span>")),
            "a renamed element keeps its classes: {}",
            s.xhtml
        );
        assert!(s.xhtml.contains("<span>z</span>"), "no empty class: {}", s.xhtml);
        let css = style_sheet(&s);
        assert!(css.contains(&format!(".__azmail_paper .{p}big {{ color: red; }}")), "{css}");
        assert!(
            css.contains(&format!(
                ".__azmail_paper p.{p}MsoNormal, .__azmail_paper .{p}a .{p}b:not(.{p}c) {{ \
                 margin: 0; }}"
            )),
            "{css}"
        );
        assert!(
            css.contains(".__azmail_paper a[href$=\".pdf\"] { color: blue; }"),
            "a dot in an attribute selector is no class: {css}"
        );
        assert!(
            css.contains(&format!(".__azmail_paper .{p}__azmail_paper {{ color: #ff0000; }}")),
            "the mail's rule for the app's class names the mail's own: {css}"
        );
        assert!(
            !css.contains(".__azmail_paper { color: #ff0000"),
            "nothing of the mail reaches the paper's class: {css}"
        );
    }

    /// The prefix is the message's: two mails differ, and the same mail keeps its prefix when
    /// it is sanitized again with its pictures on (the sheet and the markup are rebuilt
    /// together, so they always agree).
    #[test]
    fn every_mail_has_its_own_class_prefix() {
        let a = "<p class=x>a</p>";
        let b = "<p class=x>b</p>";
        assert_ne!(sanitize(a).class_prefix, sanitize(b).class_prefix);
        assert_eq!(sanitize(a).class_prefix, sanitize_with(a, true).class_prefix);
        assert_eq!(sanitize(a).class_prefix, sanitize(a).class_prefix);
    }

    /// The paper is as wide as the reading pane, or as the mail when the mail is wider (a
    /// newsletter's 600 px table in a narrow pane): an inline-block at least the pane wide, as
    /// a browser's canvas grows with its content - the table no longer runs out of its paper.
    /// The 12 px margin around the mail is the mail body's padding, inside the paper, so a
    /// short mail's paper is exactly the pane wide. (Measured on the engine with the debug
    /// server's `mount`: 624 px paper for the 600 px table in a 500 px pane, 500 px for a short
    /// mail, long lines wrapping at 476 px - Chrome's numbers.)
    #[test]
    fn a_mail_wider_than_the_pane_widens_its_paper() {
        let s = sanitize("<table width=600><tr><td>x</td></tr></table>");
        let css = style_sheet(&s);
        assert!(
            css.contains(
                ".__azmail_paper { display: inline-block; min-width: 100%; box-sizing: \
                 border-box; background-color: #ffffff; color: #1a1a1a; }"
            ),
            "{css}"
        );
        assert!(css.contains(".__azmail_mail_body { padding: 12px; }"), "{css}");
        assert!(
            s.xhtml.contains(
                "<body><div class=\"__azmail_paper\"><div class=\"__azmail_mail_body\"><table \
                 width=\"600\">"
            ),
            "{}",
            s.xhtml
        );
    }

    /// The mail's `<body>` is the mail-body element: its classes, `dir` and `style` are there,
    /// and its `body` rules apply to it. What its `bgcolor` / `text` say comes BEFORE the
    /// mail's own rules, which win over it - as an author rule wins over a presentational hint
    /// in a browser, so a newsletter's dark-mode `body { background: #000 }` is the background
    /// in the dark mode (the "black frame around a white table" was the hint winning inside a
    /// paper the rule had painted).
    #[test]
    fn the_mails_body_is_the_mail_body_element_and_its_rules_win_over_its_attributes() {
        let s = sanitize(
            "<style>@media (prefers-color-scheme: dark) { body { background: #000 } } body \
             { color: #222 } html { color: #333 }</style><body bgcolor=#f6f7f2 class=body \
             style=\"margin: 0\" dir=ltr><p>x</p></body>",
        );
        let p = s.class_prefix.as_str();
        assert!(
            s.xhtml.contains(&format!(
                "<div class=\"__azmail_mail_body {p}body\" dir=\"ltr\" style=\"margin: \
                 0\"><p>x</p></div>"
            )),
            "{}",
            s.xhtml
        );
        let css = style_sheet(&s);
        let hint = css.find(".__azmail_mail_body { background-color: #f6f7f2; }");
        let rule = css.find(".__azmail_mail_body { color: #222; }");
        let dark = css.find(
            "@media (prefers-color-scheme: dark) { .__azmail_mail_body { background: #000; } }",
        );
        assert!(
            matches!((hint, rule, dark), (Some(h), Some(r), Some(d)) if h < r && h < d),
            "the hint before the mail's rules: {css}"
        );
        assert!(css.contains(".__azmail_paper { color: #333; }"), "html is the paper: {css}");
    }

    /// A colour attribute is one value: a `;` or a brace in it cannot add a declaration (or,
    /// in the sheet, a rule) - `bgcolor="red; position: fixed"` put `position: fixed` on the
    /// mail.
    #[test]
    fn a_colour_attribute_cannot_smuggle_in_a_declaration() {
        let s = sanitize(
            "<body bgcolor=\"red; position: fixed\" text=\"#000 } .__azmail_paper { display: \
             none\"><font color=\"red;position:fixed\" face=\"a;position:fixed\">x</font></body>",
        );
        assert!(!s.xhtml.contains("position"), "{}", s.xhtml);
        assert!(!s.xhtml.contains("display: none"), "{}", s.xhtml);
    }

    #[test]
    fn nesting_is_bounded() {
        let deep = "<div>".repeat(1000) + "x";
        let out = inner(&deep);
        assert!(out.matches("<div>").count() <= 200, "{}", out.len());
        assert!(out.contains('x'));
        assert_eq!(out.matches("<div>").count(), out.matches("</div>").count());
    }

    // ---- the remote-content pre-pass and the mail's own pictures ----

    /// The engine's scan of the parsed mail (`Xml::scan_external_resources`, after the parse,
    /// before layout) lists what the mail would fetch from the web - pictures, fonts, style
    /// sheets -, http and https only, each once; what the sanitizer shows decides what is
    /// fetched ("download pictures"): no tracking pixel, no background, nothing not on the web.
    #[test]
    fn the_pre_pass_lists_the_web_pictures_fonts_and_style_sheets_and_the_shown_ones_are_fetched() {
        let html = "<html><head><link rel=\"stylesheet\" href=\"https://cdn.example/mail.css\">\
                    <style>@font-face { font-family: Brand; src: url(https://cdn.example/brand.woff2) }\
                    @import url(\"https://cdn.example/more.css\");\
                    .hero { background-image: url(https://cdn.example/hero.jpg) }</style></head>\
                    <body background=\"http://cdn.example/paper.gif\">\
                    <img src=\"https://cdn.example/logo.png\" alt=\"Logo\">\
                    <img src=\"https://cdn.example/logo.png\">\
                    <img src=\"cid:part1@example\"><img src=\"data:image/png;base64,AAAA\">\
                    <img src=\"file:///etc/passwd.png\">\
                    <img src=\"https://t.example/open.gif\" width=\"1\" height=\"1\"></body></html>";
        let off = sanitize(html);
        let mut images = off.remote.images.clone();
        images.sort();
        assert_eq!(
            images,
            vec![
                String::from("http://cdn.example/paper.gif"),
                String::from("https://cdn.example/hero.jpg"),
                String::from("https://cdn.example/logo.png"),
                String::from("https://t.example/open.gif"),
            ]
        );
        assert_eq!(off.remote.fonts, vec![String::from("https://cdn.example/brand.woff2")]);
        let mut sheets = off.remote.stylesheets.clone();
        sheets.sort();
        assert_eq!(
            sheets,
            vec![
                String::from("https://cdn.example/mail.css"),
                String::from("https://cdn.example/more.css"),
            ]
        );
        assert!(off.remote_images.is_empty(), "nothing is fetched before the reader asks");
        assert_eq!(off.remote.summary(), "4 pictures, 1 font and 2 style sheets");
        // "Download pictures": the same list, and only the shown picture is fetched.
        let on = sanitize_with(html, true);
        assert_eq!(on.remote, off.remote);
        assert_eq!(on.remote_images, vec![String::from("https://cdn.example/logo.png")]);
        assert_eq!(RemoteContent::default().summary(), "");
        let one = RemoteContent {
            images: vec![String::from("https://cdn.example/a.png")],
            ..RemoteContent::default()
        };
        assert_eq!(one.summary(), "1 picture");
    }

    /// A picture the mail carries itself (`cid:` naming one of its parts) is no download: it
    /// shows at once, under a key of this message's own (two mails' `image001.png` never mix in
    /// the image cache); a `cid:` the mail does not have stays a placeholder.
    #[test]
    fn the_mails_own_cid_pictures_show_without_asking_under_a_key_of_their_own() {
        let html = "<p><img src=\"cid:logo@example\" alt=\"Logo\" width=\"120\">\
                    <img src=\"CID:missing@example\" alt=\"Gone\">\
                    <img src=\"https://cdn.example/a.png\" alt=\"Web\"></p>";
        let options = PictureOptions {
            web: false,
            inline: vec![String::from("logo@example")],
        };
        let s = sanitize_mail(html, &options);
        assert_eq!(s.inline_images, vec![String::from("logo@example")]);
        let key = s.inline_key("logo@example");
        assert_eq!(key, format!("cid:{}logo@example", s.class_prefix));
        assert!(
            s.xhtml.contains(&format!("<img src=\"{key}\" alt=\"Logo\" width=\"120\"/>")),
            "{}",
            s.xhtml
        );
        assert!(s.xhtml.contains("[image: Gone]"), "{}", s.xhtml);
        assert!(s.xhtml.contains("[image: Web]"), "{}", s.xhtml);
        assert_eq!(s.blocked_images, 2, "the missing one and the web one");
        assert!(s.remote_images.is_empty());
        // The plain sanitizer knows no parts: every cid: picture is a placeholder.
        assert!(sanitize(html).inline_images.is_empty());
        assert_eq!(sanitize_mail(html, &PictureOptions::default()), sanitize(html));
        // Another mail with the same part name gets another key.
        let other = sanitize_mail("<img src=\"cid:logo@example\"><p>other</p>", &options);
        assert_ne!(other.inline_key("logo@example"), key);
    }
}
