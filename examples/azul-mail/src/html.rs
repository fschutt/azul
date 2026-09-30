//! Mail HTML made safe and well-formed for azul's XML parser.
//!
//! Real mail HTML is rarely XML: unquoted and bare attributes, unclosed `<p>` and `<td>`,
//! `&nbsp;`, uppercase tags, conditional comments. azul's parser (`Xml::from_str`) takes XHTML
//! only, so this module reads the HTML leniently and writes a small, well-formed XHTML subset:
//!
//! - scripts, styles, titles, forms' option lists, frames, SVG and MathML go with their content;
//!   comments, doctypes and processing instructions go;
//! - images are NOT loaded (remote images are off): each becomes a grey `[image: alt]` text,
//!   and a tracking pixel (1x1 or hidden) not even that;
//! - only presentational tags stay (`p div span b i u a table tr td ul li h1 ...`); `font`
//!   becomes a `span`, `center` a centred `div`, `body` a `div`, and any other tag is dropped
//!   with its text kept;
//! - attributes: `href` (http, https and mailto only), `colspan`, `rowspan`, `dir`, and `style`
//!   with a short list of properties whose values name no URL (and no negative margin);
//!   `align`, `bgcolor`, `width` and `font color` become style;
//! - every open tag is closed, mis-nested ones in order; `<p>`, `<li>`, `<td>` and `<tr>` close
//!   themselves as HTML says; nesting deeper than 200 keeps the text only;
//! - character references are decoded (named, decimal, hex) and the text re-escaped.
//!
//! The result is `<html><body><div>...</div></body></html>` for `Dom::create_from_parsed_xml`.

/// The sanitized document and what was left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sanitized {
    pub xhtml: String,
    /// Images that were not loaded.
    pub blocked_images: usize,
}

/// See the module documentation.
pub fn sanitize(html: &str) -> Sanitized {
    let mut s = Sanitizer::default();
    s.run(html);
    s.finish()
}

/// Tags deeper than this keep their text only.
const MAX_DEPTH: usize = 200;
/// How a blocked image's placeholder looks.
const BLOCKED_IMAGE_STYLE: &str = "color: #6b7385";

/// Elements that go with everything inside them.
const SKIP_WITH_CONTENT: &[&str] = &[
    "script", "style", "title", "textarea", "select", "noscript", "template", "iframe", "object",
    "applet", "svg", "math", "xmp", "frameset", "noframes", "audio", "video", "canvas",
];

/// Output tags that end an open `<p>` when they start.
const BLOCK: &[&str] = &[
    "div",
    "p",
    "ul",
    "ol",
    "dl",
    "table",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "blockquote",
    "pre",
    "hr",
    "li",
    "dd",
    "dt",
];

/// Output tags that may lie inside an open `<p>`.
const INLINE: &[&str] = &[
    "span", "a", "b", "strong", "i", "em", "u", "s", "del", "ins", "small", "big", "sub", "sup",
    "code", "abbr", "cite", "q", "kbd", "samp", "var", "mark",
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
    /// The open output elements.
    stack: Vec<&'static str>,
    blocked_images: usize,
}

impl Sanitizer {
    fn run(&mut self, html: &str) {
        let b = html.as_bytes();
        let mut i = 0;
        let mut text_start = 0;
        while i < b.len() {
            if b[i] != b'<' {
                i += 1;
                continue;
            }
            let rest = &html[i..];
            // Comments (conditional ones too), doctypes, processing instructions.
            if rest.starts_with("<!--") {
                self.text(&html[text_start..i]);
                i = rest[4..].find("-->").map_or(b.len(), |e| i + 4 + e + 3);
                text_start = i;
                continue;
            }
            if rest.starts_with("<!") || rest.starts_with("<?") {
                self.text(&html[text_start..i]);
                i = rest.find('>').map_or(b.len(), |e| i + e + 1);
                text_start = i;
                continue;
            }
            let is_end = rest.starts_with("</");
            let name_start = if is_end { i + 2 } else { i + 1 };
            if name_start >= b.len() || !b[name_start].is_ascii_alphabetic() {
                // A `<` that starts no tag is text.
                i += 1;
                continue;
            }
            self.text(&html[text_start..i]);
            let mut j = name_start;
            while j < b.len()
                && (b[j].is_ascii_alphanumeric() || matches!(b[j], b':' | b'-' | b'_'))
            {
                j += 1;
            }
            let name = html[name_start..j].to_ascii_lowercase();
            let (attributes, after) = parse_attributes(html, j);
            i = after;
            text_start = i;
            if is_end {
                self.end_tag(&name);
            } else if SKIP_WITH_CONTENT.contains(&name.as_str()) {
                let close = format!("</{name}");
                i = match find_ascii_ci(&html[i..], &close) {
                    Some(at) => html[i + at..].find('>').map_or(b.len(), |e| i + at + e + 1),
                    None => b.len(),
                };
                text_start = i;
            } else {
                self.start_tag(&name, &attributes);
            }
        }
        self.text(&html[text_start..]);
    }

    fn text(&mut self, raw: &str) {
        if !raw.is_empty() {
            escape_into(&mut self.out, &decode_references(raw), false);
        }
    }

    fn start_tag(&mut self, name: &str, attributes: &[(String, Option<String>)]) {
        match classify(name) {
            Tag::Drop => {}
            Tag::Image => {
                self.blocked_images += 1;
                let attribute = |name: &str| {
                    attributes
                        .iter()
                        .find(|(n, _)| n == name)
                        .and_then(|(_, v)| v.as_deref())
                        .map(str::trim)
                };
                if is_tracking_pixel(attribute("width"), attribute("height"), attribute("style")) {
                    return;
                }
                let alt = attribute("alt").filter(|a| !a.is_empty());
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
            Tag::Void(tag) => {
                self.implied_ends(tag);
                self.out.push('<');
                self.out.push_str(tag);
                self.out.push_str("/>");
            }
            Tag::Keep(tag) => {
                self.implied_ends(tag);
                if self.stack.len() >= MAX_DEPTH {
                    return;
                }
                self.out.push('<');
                self.out.push_str(tag);
                push_attributes(&mut self.out, name, tag, attributes);
                self.out.push('>');
                self.stack.push(tag);
            }
        }
    }

    fn end_tag(&mut self, name: &str) {
        // The renamed body closes with the document.
        if name == "body" {
            return;
        }
        if let Tag::Keep(tag) = classify(name) {
            if let Some(at) = self.stack.iter().rposition(|t| *t == tag) {
                self.pop_through(at);
            }
        }
    }

    /// The ends HTML implies when `tag` starts: a block ends an open paragraph, an item ends
    /// the open item of its list, a cell the open cell of its row, a row the open row.
    fn implied_ends(&mut self, tag: &str) {
        if BLOCK.contains(&tag) {
            if let Some(p) = self.stack.iter().rposition(|t| *t == "p") {
                if self.stack[p + 1..].iter().all(|t| INLINE.contains(t)) {
                    self.pop_through(p);
                }
            }
        }
        match tag {
            "li" => self.close_item(&["li"], &["ul", "ol"]),
            "dt" | "dd" => self.close_item(&["dt", "dd"], &["dl"]),
            "td" | "th" => self.close_item(&["td", "th"], &["tr", "table"]),
            "tr" => self.close_item(&["tr"], &["table"]),
            "thead" | "tbody" | "tfoot" => {
                self.close_item(&["thead", "tbody", "tfoot"], &["table"])
            }
            _ => {}
        }
    }

    /// Closes the innermost open `items` element, unless a `scopes` element is nearer.
    fn close_item(&mut self, items: &[&str], scopes: &[&str]) {
        for at in (0..self.stack.len()).rev() {
            let open = self.stack[at];
            if scopes.contains(&open) {
                return;
            }
            if items.contains(&open) {
                self.pop_through(at);
                return;
            }
        }
    }

    /// Closes the open elements from the innermost down to (and with) `at`.
    fn pop_through(&mut self, at: usize) {
        while self.stack.len() > at {
            if let Some(tag) = self.stack.pop() {
                self.out.push_str("</");
                self.out.push_str(tag);
                self.out.push('>');
            }
        }
    }

    fn finish(mut self) -> Sanitized {
        self.pop_through(0);
        Sanitized {
            xhtml: format!("<html><body><div>{}</div></body></html>", self.out),
            blocked_images: self.blocked_images,
        }
    }
}

/// The attributes of a tag whose name ends at `at`, and where the tag ends. Values may be
/// double-, single- or unquoted; a bare name has no value.
fn parse_attributes(html: &str, mut k: usize) -> (Vec<(String, Option<String>)>, usize) {
    let b = html.as_bytes();
    let mut attributes = Vec::new();
    loop {
        while k < b.len() && (b[k].is_ascii_whitespace() || b[k] == b'/') {
            k += 1;
        }
        if k >= b.len() {
            return (attributes, b.len());
        }
        if b[k] == b'>' {
            return (attributes, k + 1);
        }
        let name_start = k;
        while k < b.len() && !b[k].is_ascii_whitespace() && !matches!(b[k], b'=' | b'>' | b'/') {
            k += 1;
        }
        let name = html[name_start..k].to_ascii_lowercase();
        while k < b.len() && b[k].is_ascii_whitespace() {
            k += 1;
        }
        let mut value = None;
        if k < b.len() && b[k] == b'=' {
            k += 1;
            while k < b.len() && b[k].is_ascii_whitespace() {
                k += 1;
            }
            if k < b.len() && (b[k] == b'"' || b[k] == b'\'') {
                let quote = b[k] as char;
                let start = k + 1;
                let end = html[start..].find(quote).map_or(b.len(), |e| start + e);
                value = Some(decode_references(&html[start..end]));
                k = (end + 1).min(b.len());
            } else {
                let start = k;
                while k < b.len() && !b[k].is_ascii_whitespace() && b[k] != b'>' {
                    k += 1;
                }
                value = Some(decode_references(&html[start..k]));
            }
        }
        if name.is_empty() && value.is_none() {
            // Nothing read (a stray character): step over it.
            k += 1;
        } else if !name.is_empty() {
            attributes.push((name, value));
        }
    }
}

/// Writes the attributes an output tag keeps: `href`, `colspan`, `rowspan`, `dir`, and one
/// `style` from the safe style declarations and the presentational attributes.
fn push_attributes(
    out: &mut String,
    source: &str,
    tag: &str,
    attributes: &[(String, Option<String>)],
) {
    let mut kept: Vec<(&str, String)> = Vec::new();
    let mut styles: Vec<(String, String)> = Vec::new();
    if source == "center" {
        styles.push((String::from("text-align"), String::from("center")));
    }
    for (name, value) in attributes {
        let value = value.as_deref().unwrap_or("").trim();
        let lower = value.to_ascii_lowercase();
        match name.as_str() {
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
            "align" => {
                if matches!(lower.as_str(), "left" | "right" | "center" | "justify") {
                    styles.push((String::from("text-align"), lower.clone()));
                }
            }
            "valign" => {
                if matches!(lower.as_str(), "top" | "middle" | "bottom" | "baseline") {
                    styles.push((String::from("vertical-align"), lower.clone()));
                }
            }
            "bgcolor" => {
                if !value.is_empty() && safe_style_value(value) {
                    styles.push((String::from("background-color"), value.to_string()));
                }
            }
            "color" if source == "font" => {
                if !value.is_empty() && safe_style_value(value) {
                    styles.push((String::from("color"), value.to_string()));
                }
            }
            "width" | "height" if matches!(tag, "table" | "td" | "th") => {
                if let Some(length) = html_length(value) {
                    styles.push((name.clone(), length));
                }
            }
            "style" => styles.extend(parse_style(value)),
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

/// An HTML length attribute (`600`, `600px`, `50%`) as CSS.
fn html_length(value: &str) -> Option<String> {
    let number = |s: &str| {
        s.trim()
            .parse::<f32>()
            .ok()
            .filter(|n| n.is_finite() && *n >= 0.0)
    };
    if let Some(n) = value.strip_suffix('%') {
        number(n).map(|_| value.to_string())
    } else if let Some(n) = value.strip_suffix("px") {
        number(n).map(|_| value.to_string())
    } else {
        number(value).map(|_| format!("{value}px"))
    }
}

/// Where `needle` (ASCII) first occurs in `haystack`, ignoring ASCII case.
fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let (h, n) = (haystack.as_bytes(), needle.as_bytes());
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    (0..=h.len() - n.len()).find(|&k| h[k..k + n.len()].eq_ignore_ascii_case(n))
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

/// `s` with its character references decoded; a `&` that starts none stays.
fn decode_references(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        match reference(after) {
            Some((decoded, used)) => {
                if let Some(c) = decoded {
                    out.push(c);
                }
                rest = &after[used..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The reference at the start of `s` (just after its `&`): the character (`None` for a numeric
/// reference to nothing, such as `&#0;`) and the bytes used, `;` included.
fn reference(s: &str) -> Option<(Option<char>, usize)> {
    let end = s.bytes().take(33).position(|b| b == b';')?;
    if end == 0 {
        return None;
    }
    let body = &s[..end];
    if let Some(number) = body.strip_prefix('#') {
        let n = match number.strip_prefix(|c: char| c == 'x' || c == 'X') {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse::<u32>().ok()?,
        };
        return Some((numeric_char(n), end + 1));
    }
    named_char(body).map(|c| (Some(c), end + 1))
}

/// A numeric reference's character: 128 - 159 are read as Windows-1252, as browsers do.
fn numeric_char(n: u32) -> Option<char> {
    if (0x80..=0x9f).contains(&n) {
        return WINDOWS_1252[(n - 0x80) as usize];
    }
    if n == 0 {
        return None;
    }
    char::from_u32(n)
}

/// Windows-1252's characters for 0x80 - 0x9F.
const WINDOWS_1252: [Option<char>; 32] = [
    Some('\u{20ac}'),
    None,
    Some('\u{201a}'),
    Some('\u{0192}'),
    Some('\u{201e}'),
    Some('\u{2026}'),
    Some('\u{2020}'),
    Some('\u{2021}'),
    Some('\u{02c6}'),
    Some('\u{2030}'),
    Some('\u{0160}'),
    Some('\u{2039}'),
    Some('\u{0152}'),
    None,
    Some('\u{017d}'),
    None,
    None,
    Some('\u{2018}'),
    Some('\u{2019}'),
    Some('\u{201c}'),
    Some('\u{201d}'),
    Some('\u{2022}'),
    Some('\u{2013}'),
    Some('\u{2014}'),
    Some('\u{02dc}'),
    Some('\u{2122}'),
    Some('\u{0161}'),
    Some('\u{203a}'),
    Some('\u{0153}'),
    None,
    Some('\u{017e}'),
    Some('\u{0178}'),
];

/// The Latin-1 entity names, for U+00A0 to U+00FF in order.
const LATIN1: [&str; 96] = [
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf",
    "laquo", "not", "shy", "reg", "macr", "deg", "plusmn", "sup2", "sup3", "acute", "micro",
    "para", "middot", "cedil", "sup1", "ordm", "raquo", "frac14", "frac12", "frac34", "iquest",
    "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig", "Ccedil", "Egrave", "Eacute",
    "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve", "Oacute",
    "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute", "Ucirc", "Uuml", "Yacute",
    "THORN", "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil",
    "egrave", "eacute", "ecirc", "euml", "igrave", "iacute", "icirc", "iuml", "eth", "ntilde",
    "ograve", "oacute", "ocirc", "otilde", "ouml", "divide", "oslash", "ugrave", "uacute", "ucirc",
    "uuml", "yacute", "thorn", "yuml",
];

/// Other entity names mail uses.
const NAMED: &[(&str, char)] = &[
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("quot", '"'),
    ("apos", '\''),
    ("OElig", '\u{152}'),
    ("oelig", '\u{153}'),
    ("Scaron", '\u{160}'),
    ("scaron", '\u{161}'),
    ("Yuml", '\u{178}'),
    ("fnof", '\u{192}'),
    ("circ", '\u{2c6}'),
    ("tilde", '\u{2dc}'),
    ("ensp", '\u{2002}'),
    ("emsp", '\u{2003}'),
    ("thinsp", '\u{2009}'),
    ("zwnj", '\u{200c}'),
    ("zwj", '\u{200d}'),
    ("lrm", '\u{200e}'),
    ("rlm", '\u{200f}'),
    ("ndash", '\u{2013}'),
    ("mdash", '\u{2014}'),
    ("lsquo", '\u{2018}'),
    ("rsquo", '\u{2019}'),
    ("sbquo", '\u{201a}'),
    ("ldquo", '\u{201c}'),
    ("rdquo", '\u{201d}'),
    ("bdquo", '\u{201e}'),
    ("dagger", '\u{2020}'),
    ("Dagger", '\u{2021}'),
    ("bull", '\u{2022}'),
    ("hellip", '\u{2026}'),
    ("permil", '\u{2030}'),
    ("prime", '\u{2032}'),
    ("lsaquo", '\u{2039}'),
    ("rsaquo", '\u{203a}'),
    ("euro", '\u{20ac}'),
    ("trade", '\u{2122}'),
    ("larr", '\u{2190}'),
    ("uarr", '\u{2191}'),
    ("rarr", '\u{2192}'),
    ("darr", '\u{2193}'),
    ("harr", '\u{2194}'),
    ("hearts", '\u{2665}'),
    ("check", '\u{2713}'),
];

fn named_char(name: &str) -> Option<char> {
    if let Some(at) = LATIN1.iter().position(|n| *n == name) {
        return char::from_u32(0xa0 + at as u32);
    }
    NAMED.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "<html><body><div>";
    const TAIL: &str = "</div></body></html>";

    /// The sanitized body, without the wrapper every result has.
    fn inner(html: &str) -> String {
        let out = sanitize(html).xhtml;
        assert!(out.starts_with(HEAD) && out.ends_with(TAIL), "{out}");
        out[HEAD.len()..out.len() - TAIL.len()].to_string()
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
        assert_eq!(
            inner("<head><title>T</title><meta charset=utf-8></head><body>b</body>"),
            "<div>b</div>"
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
        assert_eq!(inner("<b><i>x</b>y</i>"), "<b><i>x</i></b>y");
        assert_eq!(
            inner("<div><ul><li>a<li>b"),
            "<div><ul><li>a</li><li>b</li></ul></div>"
        );
        assert_eq!(
            inner("<table><tr><td>a<td>b<tr><td>c</table>"),
            "<table><tr><td>a</td><td>b</td></tr><tr><td>c</td></tr></table>"
        );
        assert_eq!(inner("<p>a<div>b</div>"), "<p>a</p><div>b</div>");
        assert_eq!(inner("x</p></div>y"), "xy");
    }

    #[test]
    fn text_is_escaped_and_references_are_decoded() {
        assert_eq!(inner("a < b and c>d"), "a &lt; b and c&gt;d");
        assert_eq!(
            inner("a&nbsp;b &amp; &lt;c&gt; &#8364; &#x263A; &copy; &uuml; &bogus; & x"),
            "a\u{a0}b &amp; &lt;c&gt; \u{20ac} \u{263a} \u{a9} \u{fc} &amp;bogus; &amp; x"
        );
        assert_eq!(inner("&#150; &#0; x"), "\u{2013}  x");
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
            inner("<td bgcolor=\"#eee\" align=center width=50% colspan=2 nowrap>x</td>"),
            "<td colspan=\"2\" style=\"background-color: #eee; text-align: center; width: 50%\">x</td>"
        );
        assert_eq!(
            inner("<table width=600><tr><td>x</td></tr></table>"),
            "<table style=\"width: 600px\"><tr><td>x</td></tr></table>"
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
            "<span style=\"color: #ff0000\">red</span><div style=\"text-align: center\">c</div>"
        );
        assert_eq!(
            inner("<body style=\"margin:0\"><h1>T</h1></body>"),
            "<div style=\"margin: 0\"><h1>T</h1></div>"
        );
        assert_eq!(inner("<o:p>x</o:p><custom-tag>y</custom-tag>"), "xy");
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

    #[test]
    fn nesting_is_bounded() {
        let deep = "<div>".repeat(1000) + "x";
        let out = inner(&deep);
        assert!(out.matches("<div>").count() <= 200, "{}", out.len());
        assert!(out.contains('x'));
        assert_eq!(out.matches("<div>").count(), out.matches("</div>").count());
    }
}
