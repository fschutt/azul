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
    unimplemented!("RED: rem_to_em {value}")
}

/// Whether a length (one token) is relative to the text or its container: `em`, `ex`, `ch`,
/// `%`, `rem` (read as `em`), `0`, or a keyword (`auto`, `none`, `normal`, the font size
/// keywords).
#[must_use]
pub fn is_relative(value: &str) -> bool {
    unimplemented!("RED: is_relative {value}")
}

/// One declaration fitted to the reader: `None` when it goes.
#[must_use]
pub fn fit_declaration(property: &str, value: &str) -> Option<(String, String)> {
    unimplemented!("RED: fit_declaration {property} {value}")
}

/// A `style` attribute fitted to the reader (`""` when nothing stays).
#[must_use]
pub fn fit_inline(style: &str) -> String {
    unimplemented!("RED: fit_inline {style}")
}

/// A book's style sheet fitted to the reader: its rules with the declarations that stay;
/// the rules of an `@media` for the screen unwrapped, those for print or for one vendor's
/// device dropped; every other at-rule dropped.
#[must_use]
pub fn fit_sheet(sheet: &str) -> String {
    unimplemented!(
        "RED: fit_sheet {sheet} {}",
        KEPT.len() + HYPHENS_ALIASES.len()
    )
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
