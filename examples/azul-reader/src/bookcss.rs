//! A book's style sheets fitted to the reader.
//!
//! A book brings its LAYOUT (margins, indents, alignment, emphasis, lists, tables, breaks
//! between chapters) and the reader brings its PAPER and its TYPE: the colours and
//! backgrounds go (the paper - white, sepia, night - decides them, and a dark text a book
//! hard-codes would vanish on the night paper), the font families go (the reader's font
//! decides; a monospace one stays monospace), absolute font sizes and line heights go (the
//! reader's size decides; a relative size scales with it), `rem` becomes `em` (the root is
//! the reader's column, not the window), positioning goes. Fonts and pictures the sheet would
//! fetch (`@font-face`, `@import`, any `url(`) go too. What stays is read and written by the
//! one sheet reader, `azul_appkit::css`.

use azul_appkit::css::{self, CssItem};

/// The properties a book keeps (anything else is dropped).
const KEPT: &[&str] = &[
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "text-indent",
    "text-align",
    "text-align-last",
    "text-transform",
    "text-decoration",
    "text-decoration-line",
    "font-style",
    "font-weight",
    "font-variant",
    "letter-spacing",
    "word-spacing",
    "vertical-align",
    "white-space",
    "display",
    "visibility",
    "list-style",
    "list-style-type",
    "list-style-position",
    "border",
    "border-top",
    "border-right",
    "border-bottom",
    "border-left",
    "border-width",
    "border-style",
    "border-color",
    "border-collapse",
    "border-spacing",
    "float",
    "clear",
    "page-break-before",
    "page-break-after",
    "page-break-inside",
    "break-before",
    "break-after",
    "break-inside",
    "orphans",
    "widows",
    "hyphens",
    // Sized only in units relative to the text or the page (below).
    "width",
    "min-width",
    "max-width",
    "height",
    "min-height",
    "max-height",
    "font-size",
    "line-height",
    // Kept as `monospace` only (below).
    "font-family",
];

/// Vendor names of `hyphens`.
const HYPHENS_ALIASES: &[&str] = &[
    "-webkit-hyphens",
    "-epub-hyphens",
    "-moz-hyphens",
    "-ms-hyphens",
    "adobe-hyphenate",
];

/// `value` with every `<number>rem` written `<number>em`.
#[must_use]
pub fn rem_to_em(value: &str) -> String {
    // ASCII lower-casing keeps every byte where it was: the indices hold for `value`.
    let lower = value.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut from = 0;
    let mut written = 0;
    while let Some(found) = lower[from..].find("rem") {
        let at = from + found;
        let after = at + 3;
        let number_before = at > 0 && (bytes[at - 1].is_ascii_digit() || bytes[at - 1] == b'.');
        let word_ends = after >= bytes.len() || !bytes[after].is_ascii_alphanumeric();
        if number_before && word_ends {
            out.push_str(&value[written..at]);
            out.push_str("em");
            written = after;
        }
        from = after;
    }
    out.push_str(&value[written..]);
    out
}

/// Whether a length (one token) is relative to the text or its container: `em`, `ex`, `ch`,
/// `%`, `rem` (read as `em`), `0`, or a keyword (`auto`, `none`, `normal`, the font size
/// keywords).
#[must_use]
pub fn is_relative(value: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "auto",
        "none",
        "normal",
        "inherit",
        "initial",
        "unset",
        "xx-small",
        "x-small",
        "small",
        "medium",
        "large",
        "x-large",
        "xx-large",
        "xxx-large",
        "smaller",
        "larger",
        "fit-content",
        "min-content",
        "max-content",
    ];
    let v = value.trim().to_ascii_lowercase();
    if KEYWORDS.contains(&v.as_str()) {
        return true;
    }
    let number = |s: &str| s.trim().parse::<f32>().is_ok_and(f32::is_finite);
    if let Ok(n) = v.parse::<f32>() {
        return n == 0.0;
    }
    for unit in ["rem", "em", "ex", "ch", "%"] {
        if let Some(n) = v.strip_suffix(unit) {
            return number(n);
        }
    }
    false
}

/// One declaration fitted to the reader: `None` when it goes.
#[must_use]
pub fn fit_declaration(property: &str, value: &str) -> Option<(String, String)> {
    let property = property.trim().to_ascii_lowercase();
    let value = value.trim();
    if value.is_empty() || !css::safe_value(value) {
        return None;
    }
    let property = if HYPHENS_ALIASES.contains(&property.as_str()) {
        "hyphens".to_string()
    } else {
        property
    };
    if !KEPT.contains(&property.as_str()) {
        return None;
    }
    let value = rem_to_em(value);
    match property.as_str() {
        "font-family" => {
            let l = value.to_ascii_lowercase();
            (l.contains("mono") || l.contains("courier") || l.contains("consol"))
                .then(|| (property, "monospace".to_string()))
        }
        "font-size" | "width" | "min-width" | "max-width" | "height" | "min-height"
        | "max-height" => is_relative(&value).then_some((property, value)),
        "line-height" => {
            let unitless = value
                .trim()
                .parse::<f32>()
                .is_ok_and(|n| n.is_finite() && n > 0.0);
            (unitless || is_relative(&value)).then_some((property, value))
        }
        _ => Some((property, value)),
    }
}

/// A `style` attribute fitted to the reader (`""` when nothing stays).
#[must_use]
pub fn fit_inline(style: &str) -> String {
    css::parse_declarations(style)
        .iter()
        .filter_map(|(p, v)| fit_declaration(p, v))
        .map(|(p, v)| format!("{p}: {v}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// A book's style sheet fitted to the reader: its rules with the declarations that stay;
/// the rules of an `@media` for the screen unwrapped, those for print or for one vendor's
/// device dropped; every other at-rule dropped.
#[must_use]
pub fn fit_sheet(sheet: &str) -> String {
    let plain = css::strip_comments(sheet);
    let mut out = String::new();
    fit_items(&plain, &mut out, 0);
    out
}

/// The rules of `sheet` (or of an `@media` block's inside, `depth` deep) fitted into `out`.
fn fit_items(sheet: &str, out: &mut String, depth: usize) {
    css::for_each_item(sheet, &mut |item| match item {
        CssItem::Rule {
            selectors,
            declarations,
        } => {
            if !safe_selectors(selectors) {
                return;
            }
            let kept: Vec<String> = declarations
                .iter()
                .filter_map(|(p, v)| fit_declaration(p, v))
                .map(|(p, v)| format!("{p}: {v};"))
                .collect();
            if kept.is_empty() {
                return;
            }
            out.push_str(selectors);
            out.push_str(" { ");
            out.push_str(&kept.join(" "));
            out.push_str(" }\n");
        }
        CssItem::AtBlock {
            name,
            condition,
            body,
        } => {
            if name == "media" && depth < 4 && screen_media(condition) {
                fit_items(body, out, depth + 1);
            }
        }
        CssItem::AtStatement { .. } => {}
    });
}

/// Whether an `@media` condition is for a screen (not for print, speech or one vendor's
/// device).
fn screen_media(condition: &str) -> bool {
    let c = condition.to_ascii_lowercase();
    css::safe_value(condition)
        && !["print", "amzn", "kindle", "speech", "aural"]
            .iter()
            .any(|other| c.contains(other))
}

/// Whether a selector list holds only what selectors need (no markup, braces, at-signs,
/// escapes, URLs or control characters).
fn safe_selectors(selectors: &str) -> bool {
    !selectors.is_empty()
        && !selectors.contains(['<', '\\', '{', '}', '@', ';'])
        && !selectors.to_ascii_lowercase().contains("url(")
        && !selectors.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit(property: &str, value: &str) -> Option<String> {
        fit_declaration(property, value).map(|(p, v)| format!("{p}: {v}"))
    }

    #[test]
    fn the_book_keeps_its_layout_and_the_reader_its_paper_and_type() {
        assert_eq!(
            fit("text-indent", "1.5em").as_deref(),
            Some("text-indent: 1.5em")
        );
        assert_eq!(
            fit("Margin", "0 0 1em 0").as_deref(),
            Some("margin: 0 0 1em 0")
        );
        assert_eq!(fit("color", "#333"), None, "the paper decides the colours");
        assert_eq!(fit("background", "white"), None);
        assert_eq!(fit("background-color", "#fff"), None);
        assert_eq!(
            fit("font-family", "Georgia, serif"),
            None,
            "the reader's font"
        );
        assert_eq!(
            fit("font-family", "\"Courier New\", monospace").as_deref(),
            Some("font-family: monospace")
        );
        assert_eq!(fit("position", "absolute"), None);
        assert_eq!(
            fit("break-before", "page").as_deref(),
            Some("break-before: page")
        );
        assert_eq!(
            fit("-webkit-hyphens", "auto").as_deref(),
            Some("hyphens: auto")
        );
    }

    #[test]
    fn sizes_stay_only_relative_to_the_readers_text() {
        assert_eq!(
            fit("font-size", "1.2em").as_deref(),
            Some("font-size: 1.2em")
        );
        assert_eq!(fit("font-size", "120%").as_deref(), Some("font-size: 120%"));
        assert_eq!(
            fit("font-size", "large").as_deref(),
            Some("font-size: large")
        );
        assert_eq!(
            fit("font-size", "1.5rem").as_deref(),
            Some("font-size: 1.5em"),
            "rem is em"
        );
        assert_eq!(fit("font-size", "12pt"), None);
        assert_eq!(fit("font-size", "16px"), None);
        assert_eq!(
            fit("line-height", "1.4").as_deref(),
            Some("line-height: 1.4")
        );
        assert_eq!(fit("line-height", "18px"), None);
        assert_eq!(
            fit("width", "600px"),
            None,
            "a fixed width would overflow a small page"
        );
        assert_eq!(fit("max-width", "100%").as_deref(), Some("max-width: 100%"));
        assert_eq!(fit("height", "auto").as_deref(), Some("height: auto"));
        assert_eq!(
            fit("margin-left", "2rem").as_deref(),
            Some("margin-left: 2em")
        );
        assert_eq!(
            rem_to_em("1rem 0.5REM 2em remains"),
            "1em 0.5em 2em remains"
        );
        assert!(is_relative("0"));
        assert!(!is_relative("3cm"));
    }

    #[test]
    fn what_would_fetch_or_run_goes() {
        assert_eq!(fit("margin", "expression(1)"), None);
        assert_eq!(fit("list-style", "url(dot.png) disc"), None);
        assert_eq!(fit("display", "none").as_deref(), Some("display: none"));
    }

    #[test]
    fn an_inline_style_keeps_what_fits() {
        assert_eq!(
            fit_inline("color: red; text-align: center; font-size: 12pt; font-weight: bold"),
            "text-align: center; font-weight: bold"
        );
        assert_eq!(fit_inline("color: red"), "");
    }

    #[test]
    fn a_sheet_keeps_its_rules_and_the_screens_media_and_drops_the_rest() {
        let sheet = "@charset \"utf-8\";\n@import url(more.css);\n\
                     @font-face { font-family: Fancy; src: url(f.otf) }\n\
                     /* body */ body { font-family: Fancy; color: #111; margin: 0 5% }\n\
                     p { text-indent: 1em; font-size: 11pt }\n\
                     .gone { color: blue }\n\
                     @media screen { h1 { text-align: center } }\n\
                     @media amzn-kf8 { h1 { text-align: left } }\n\
                     @media print { h1 { page-break-before: always } }\n\
                     @page { margin: 2cm }\n\
                     a<b { margin: 0 }";
        let fitted = fit_sheet(sheet);
        assert_eq!(
            fitted, "body { margin: 0 5%; }\np { text-indent: 1em; }\nh1 { text-align: center; }\n",
            "the font, the colours, the absolute size, the vendor and print media, @page and a \
             selector with markup in it went"
        );
    }
}
