//! A chapter read into the tree the reader lays out.
//!
//! The chapter (XHTML, read by azul's strict XML loader, the HTML5-like parser as the
//! fallback - [`crate::xmltree::parse_document`]) is written out as a new `Xml` tree that
//! `Dom::create_from_parsed_xml` makes the chapter's DOM:
//!
//! ```text
//! html
//!   head
//!     style   the reader's base sheet, then the book's sheets (linked and inline),
//!             fitted to the reader (crate::bookcss)
//!   body      the chapter's body: its id and classes, its style fitted
//!     ...
//! ```
//!
//! - kept: the document's elements with `id`, `class`, a fitted `style`, `colspan`, `rowspan`,
//!   `dir`, `lang`; every other attribute (event handlers, `href`, `data-*`) goes;
//! - gone with their content: scripts, styles (read above), titles, forms and their controls
//!   (azul would make them live widgets), frames, media, canvases, objects, drawings;
//! - `<img>` (and an `<svg>` that only frames an `<image>`, the usual cover page): its `src`
//!   becomes the picture's image-cache name ([`image_src`]) and its size the one that fits the
//!   page ([`fit_image`]) - the SAME box in the pagination (which has no pictures) and on
//!   screen. A picture the book does not have goes (its `alt` text stays);
//! - `center` is a centred `div`, `font` a `span`, `math` its text in a `span`;
//! - white space between the rows of a table and the items of a list goes.
//!
//! On the way the chapter's plain text is collected: its length places an `id` in the chapter
//! (a table-of-contents entry with a `#fragment`) and gives a bookmark its excerpt.

use azul::{
    dom::{XmlAttributeMap, XmlNode, XmlNodeChild},
    str::{String as AzString, StringPair},
    vec::{StringPairVec, XmlNodeChildVec},
    xml::{Xml, XmlTagName},
};

use crate::{
    bookcss,
    epub::{resolve, Container},
    xmltree::{self, attr, find_all, find_first},
};

/// The prefix of a picture's name in the image cache: `azreader:<book>/<path>`.
pub const IMAGE_SCHEME: &str = "azreader:";

/// The reader's own sheet, before the book's (which wins where it says something).
pub const BASE_SHEET: &str = "body { margin: 0; padding: 0; }\n\
a { color: inherit; text-decoration: none; }\n\
img { break-inside: avoid; }\n\
pre { white-space: pre-wrap; }\n\
h1, h2, h3, h4, h5, h6 { break-after: avoid; }\n\
table { border-collapse: collapse; max-width: 100%; }\n";

/// Elements that go with everything inside them.
const SKIP_WITH_CONTENT: &[&str] = &[
    "script",
    "style",
    "title",
    "head",
    "noscript",
    "template",
    "iframe",
    "frame",
    "frameset",
    "object",
    "embed",
    "applet",
    "audio",
    "video",
    "canvas",
    "form",
    "input",
    "select",
    "textarea",
    "button",
    "datalist",
    "output",
    "progress",
    "meter",
    "icon",
    "transient-window",
    "link",
    "meta",
    "base",
];

/// Containers whose white-space-only text is no content (it only indents the markup).
const NO_TEXT_CONTAINERS: &[&str] = &[
    "html", "table", "thead", "tbody", "tfoot", "tr", "colgroup", "ul", "ol", "dl", "menu",
];

/// Attributes that stay (with `style`, which is fitted).
const KEPT_ATTRIBUTES: &[&str] = &["id", "class", "colspan", "rowspan", "dir", "lang"];

/// Deeper than this, an element keeps only its text.
const MAX_DEPTH: usize = 160;

/// The image-cache name of the picture at `path` of book `book`.
#[must_use]
pub fn image_src(book: &str, path: &str) -> String {
    format!("{IMAGE_SCHEME}{book}/{path}")
}

/// A picture a chapter shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentImage {
    /// Its path in the book.
    pub path: String,
    /// Its image-cache name ([`image_src`]).
    pub src: String,
}

/// What the chapter is read for.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadOptions {
    /// The book's id (the pictures' names).
    pub book: String,
    /// The page's text area (a picture is never bigger).
    pub page_width: f32,
    pub page_height: f32,
}

/// A chapter, read.
#[derive(Debug, Clone)]
pub struct Chapter {
    /// `html > (head > style) + body`, for `Dom::create_from_parsed_xml`.
    pub xml: Xml,
    /// Its pictures, each once, in order.
    pub images: Vec<ContentImage>,
    /// Every element `id` and how many characters of the plain text come before it.
    pub anchors: Vec<(String, usize)>,
    /// The plain text, its white space folded.
    pub text: String,
}

impl Chapter {
    /// The share of the chapter's text before the element with `id` (0 when the chapter has
    /// no text), `None` when no element has it.
    #[must_use]
    pub fn anchor_fraction(&self, id: &str) -> Option<f32> {
        unimplemented!("RED: anchor_fraction {id} {}", self.anchors.len())
    }

    /// About `max_chars` characters of the text from the share `fraction` of it on, from the
    /// start of a word, ending at a word with an ellipsis when cut.
    #[must_use]
    pub fn excerpt(&self, fraction: f32, max_chars: usize) -> String {
        unimplemented!("RED: excerpt {fraction} {max_chars} {}", self.text.len())
    }
}

/// The size a picture of `natural` pixels is shown at on a page of `page`: its own size when
/// it fits, else scaled down (keeping its shape) to the page's width and to 95 % of its height
/// (whole pixels, rounded down).
#[must_use]
pub fn fit_image(natural: (u32, u32), page: (f32, f32)) -> (f32, f32) {
    unimplemented!("RED: fit_image {natural:?} {page:?}")
}

/// Reads the chapter at `path` of `container` (`html`: read it as HTML straight away).
/// `size_of(path)` answers a picture's natural size in pixels (`None`: not a picture azul can
/// decode - it goes).
pub fn read_chapter(
    container: &Container,
    path: &str,
    html: bool,
    options: &ReadOptions,
    size_of: &mut dyn FnMut(&str) -> Option<(u32, u32)>,
) -> Chapter {
    unimplemented!(
        "RED: read_chapter {path} {html} {options:?} {} {}",
        container.len(),
        size_of("").is_some()
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::epub::fixtures;

    /// A tree as one line: `name[attr=value ...]{children}`, text as a quoted string with its
    /// white space folded (white-space-only text left out).
    pub(crate) fn outline(nodes: &[XmlNodeChild]) -> String {
        let mut out = Vec::new();
        for node in nodes {
            match node {
                XmlNodeChild::Text(t) => {
                    let folded = xmltree::fold_space(t.as_str());
                    if !folded.is_empty() {
                        out.push(format!("{folded:?}"));
                    }
                }
                XmlNodeChild::Element(e) => {
                    let mut s = e.node_type.inner.as_str().to_string();
                    let attrs = xmltree::attrs(e);
                    if !attrs.is_empty() {
                        let list: Vec<String> =
                            attrs.iter().map(|(k, v)| format!("{k}={v}")).collect();
                        s.push_str(&format!("[{}]", list.join(" ")));
                    }
                    let inner = outline(e.children.as_slice());
                    if !inner.is_empty() {
                        s.push_str(&format!("{{{inner}}}"));
                    }
                    out.push(s);
                }
            }
        }
        out.join(" ")
    }

    fn options() -> ReadOptions {
        ReadOptions {
            book: "b1".to_string(),
            page_width: 400.0,
            page_height: 600.0,
        }
    }

    fn sizes(path: &str) -> Option<(u32, u32)> {
        match path {
            "OEBPS/images/cover art.jpg" => Some((800, 1200)),
            "OEBPS/images/small.png" => Some((40, 20)),
            _ => None,
        }
    }

    fn read(container: &Container, path: &str) -> Chapter {
        read_chapter(container, path, false, &options(), &mut |p| sizes(p))
    }

    fn body_of(chapter: &Chapter) -> String {
        let body = find_first(chapter.xml.root.as_slice(), "body").expect("a body");
        outline(std::slice::from_ref(&XmlNodeChild::Element(body.clone())))
    }

    fn sheet_of(chapter: &Chapter) -> String {
        let style = find_first(chapter.xml.root.as_slice(), "style").expect("a style");
        xmltree::raw_text(style)
    }

    #[test]
    fn a_chapter_is_html_with_the_readers_sheet_then_the_books_fitted_sheet_and_its_body() {
        let chapter = read(&fixtures::book3(), "OEBPS/text/ch1.xhtml");
        let root = outline(chapter.xml.root.as_slice());
        assert!(root.starts_with("html{head{style{"), "{root}");
        let sheet = sheet_of(&chapter);
        assert!(
            sheet.starts_with(BASE_SHEET),
            "the reader's sheet first: {sheet}"
        );
        assert!(
            sheet.ends_with("p { margin: 0 0 1em 0; text-indent: 1.5em; }\n"),
            "the linked book.css, its colour gone: {sheet}"
        );
        assert_eq!(
            body_of(&chapter),
            "body{h1{\"Chapter One\"} p{\"It was the best of times.\"} \
             p[id=part-2]{a[id=x] \"It was the worst of times.\"} \
             p{img[src=azreader:b1/OEBPS/images/cover art.jpg alt=Cover style=width: 380px; height: 570px; max-width: none; max-height: none;]}}"
        );
        assert_eq!(
            chapter.text,
            "Chapter One It was the best of times. It was the worst of times."
        );
        assert_eq!(
            chapter.images,
            vec![ContentImage {
                path: "OEBPS/images/cover art.jpg".to_string(),
                src: "azreader:b1/OEBPS/images/cover art.jpg".to_string()
            }]
        );
    }

    #[test]
    fn scripts_forms_and_unknown_attributes_go_and_ids_classes_and_fitted_styles_stay() {
        let page = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>T</title>\
            <style>.big { font-size: 2em; color: red }</style><script>alert(1)</script></head>\
            <body class=\"chapter\" onload=\"x()\"><div id=\"d\" class=\"big\" data-x=\"1\" \
            style=\"color: blue; text-align: center\" onclick=\"y()\"><form><input value=\"q\"/>\
            </form><center>Mid</center><font color=\"red\">Red</font><p>Text<br/>more</p>\
            <ul>\n  <li>One</li>\n  <li>Two</li>\n</ul></div></body></html>";
        let c = Container::from_files(vec![("c.xhtml".to_string(), page.as_bytes().to_vec())]);
        let chapter = read(&c, "c.xhtml");
        assert_eq!(
            body_of(&chapter),
            "body[class=chapter]{div[id=d class=big style=text-align: center]{\
             div[style=text-align: center]{\"Mid\"} span{\"Red\"} p{\"Text\" br \"more\"} \
             ul{li{\"One\"} li{\"Two\"}}}}"
        );
        assert!(
            sheet_of(&chapter).ends_with(".big { font-size: 2em; }\n"),
            "{}",
            sheet_of(&chapter)
        );
        let ul = find_first(chapter.xml.root.as_slice(), "ul").expect("ul");
        assert_eq!(
            ul.children.as_slice().len(),
            2,
            "the indentation between the items went"
        );
    }

    #[test]
    fn a_picture_fits_the_page_and_one_the_book_does_not_have_leaves_its_alt_text() {
        assert_eq!(
            fit_image((40, 20), (400.0, 600.0)),
            (40.0, 20.0),
            "a small picture keeps its size"
        );
        assert_eq!(
            fit_image((800, 400), (400.0, 600.0)),
            (400.0, 200.0),
            "scaled to the width"
        );
        assert_eq!(
            fit_image((400, 1200), (400.0, 600.0)),
            (190.0, 570.0),
            "scaled to 95 % of the height"
        );
        assert_eq!(fit_image((0, 0), (400.0, 600.0)), (0.0, 0.0));
        let page = "<html><body><p><img src=\"missing.png\" alt=\"A map\"/></p>\
            <svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
            viewBox=\"0 0 40 20\"><image width=\"40\" height=\"20\" xlink:href=\"images/small.png\"/></svg>\
            <svg><circle r=\"3\"/></svg></body></html>";
        let c = Container::from_files(vec![
            ("OEBPS/p.xhtml".to_string(), page.as_bytes().to_vec()),
            ("OEBPS/images/small.png".to_string(), vec![1]),
        ]);
        let chapter = read(&c, "OEBPS/p.xhtml");
        assert_eq!(
            body_of(&chapter),
            "body{p{span{\"[A map]\"}} img[src=azreader:b1/OEBPS/images/small.png style=width: 40px; height: 20px; max-width: none; max-height: none;]}",
            "the missing picture's alt text; the svg frame of a picture is the picture; a drawing goes"
        );
        assert_eq!(chapter.images.len(), 1);
    }

    #[test]
    fn an_id_is_placed_by_the_text_before_it_and_a_bookmark_gets_an_excerpt() {
        let chapter = read(&fixtures::book3(), "OEBPS/text/ch1.xhtml");
        let at = chapter.anchor_fraction("part-2").expect("the id");
        let before = "Chapter One It was the best of times. ".chars().count() as f32;
        let all = chapter.text.chars().count() as f32;
        assert!((at - before / all).abs() < 0.02, "{at} vs {}", before / all);
        assert_eq!(chapter.anchor_fraction("nope"), None);
        assert_eq!(
            chapter.anchor_fraction("x"),
            Some(at),
            "the empty anchor in it"
        );
        assert_eq!(chapter.excerpt(0.0, 20), "Chapter One It was\u{2026}");
        assert_eq!(chapter.excerpt(at, 200), "It was the worst of times.");
        assert_eq!(chapter.excerpt(1.0, 20), "");
    }

    #[test]
    fn a_chapter_that_is_no_xml_is_read_as_html() {
        let c = Container::from_files(vec![(
            "c.html".to_string(),
            b"<p>One<p class=x>Two &amp; <b>three".to_vec(),
        )]);
        let chapter = read(&c, "c.html");
        assert_eq!(
            body_of(&chapter),
            "body{p{\"One\"} p[class=x]{\"Two &\" b{\"three\"}}}"
        );
        assert_eq!(chapter.text, "One Two & three");
    }
}
