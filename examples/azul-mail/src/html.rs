//! Mail HTML made safe and well-formed for azul's XML parser.
//!
//! Real mail HTML is rarely XML: unquoted and bare attributes, unclosed `<p>` and `<td>`,
//! `&nbsp;`, uppercase tags, conditional comments. azul's parser (`Xml::from_str`) takes XHTML
//! only, so this module reads the HTML leniently and writes a small, well-formed XHTML subset:
//!
//! - scripts, styles, titles, forms' option lists, frames, SVG and MathML go with their content;
//!   comments, doctypes and processing instructions go;
//! - images are NOT loaded (remote images are off): each becomes a grey `[image: alt]` text;
//! - only presentational tags stay (`p div span b i u a table tr td ul li h1 ...`); `font`
//!   becomes a `span`, `center` a centred `div`, `body` a `div`, and any other tag is dropped
//!   with its text kept;
//! - attributes: `href` (http, https and mailto only), `colspan`, `rowspan`, `dir`, and `style`
//!   with a short list of properties whose values name no URL; `align`, `bgcolor`, `width` and
//!   `font color` become style;
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
    todo!()
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

    #[test]
    fn nesting_is_bounded() {
        let deep = "<div>".repeat(1000) + "x";
        let out = inner(&deep);
        assert!(out.matches("<div>").count() <= 200, "{}", out.len());
        assert!(out.contains('x'));
        assert_eq!(out.matches("<div>").count(), out.matches("</div>").count());
    }
}
