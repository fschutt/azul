//! Plain text and HTML files as books, so one reading path reads everything.
//!
//! A text file becomes a [`Container`] of XHTML chapters and a [`Book`]: its paragraphs
//! (blank-line separated; a hard-wrapped paragraph's lines joined, as Project Gutenberg writes
//! them; a file without blank lines has a paragraph per line), split into chapters at its
//! headings ("CHAPTER I", "Book Two", "PROLOGUE", ...) - each heading a table-of-contents
//! entry - and a chapter longer than [`MAX_CHAPTER_CHARS`] split again at a paragraph's end
//! (laying out one huge chapter is slow). An HTML file is a book of one chapter read by the
//! HTML5-like parser.

use azul::xml::Xml;

use crate::epub::{Book, Container, Metadata, SpineItem, TocEntry};

/// A chapter of a text book is split at a paragraph's end after this many characters.
pub const MAX_CHAPTER_CHARS: usize = 60_000;

/// The words a heading line starts with (case-insensitive), followed by a blank.
const HEADING_WORDS: &[&str] = &[
    "chapter", "book", "part", "volume", "act", "canto", "letter",
];
/// Lines that are headings on their own (case-insensitive, a trailing `.` or `:` allowed).
const HEADING_LINES: &[&str] = &[
    "prologue",
    "epilogue",
    "preface",
    "introduction",
    "afterword",
    "conclusion",
    "foreword",
];

/// The paragraphs of a text: see the module documentation.
#[must_use]
pub fn paragraphs(text: &str) -> Vec<String> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let blocks: Vec<&str> = text
        .split("\n\n")
        .filter(|b| !b.trim().is_empty())
        .collect();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if blocks.len() <= 1 && lines.len() > 1 {
        return lines.iter().map(|l| xmltree_fold(l)).collect();
    }
    blocks
        .iter()
        .map(|b| xmltree_fold(b))
        .filter(|p| !p.is_empty())
        .collect()
}

fn xmltree_fold(s: &str) -> String {
    crate::xmltree::fold_space(s)
}

/// Whether a paragraph is a heading: one short line that starts with a heading word and a
/// blank ("CHAPTER IV.", "Book Two") or is a heading on its own ("PROLOGUE").
#[must_use]
pub fn is_heading(paragraph: &str) -> bool {
    let p = paragraph.trim();
    if p.is_empty() || p.chars().count() > 60 || p.contains('\n') {
        return false;
    }
    let lower = p.to_lowercase();
    let bare = lower.trim_end_matches(['.', ':']);
    if HEADING_LINES.contains(&bare) {
        return true;
    }
    HEADING_WORDS.iter().any(|w| {
        lower
            .strip_prefix(w)
            .is_some_and(|rest| rest.starts_with(' ') && rest.trim().chars().count() <= 40)
    })
}

/// One chapter's XHTML.
fn chapter_xhtml(title: &str, heading: Option<&str>, paragraphs: &[String]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>");
    out.push_str(Xml::encode_text(title).as_str());
    out.push_str("</title></head><body>");
    if let Some(heading) = heading {
        out.push_str("<h2>");
        out.push_str(Xml::encode_text(heading).as_str());
        out.push_str("</h2>");
    }
    for p in paragraphs {
        out.push_str("<p>");
        out.push_str(Xml::encode_text(p.as_str()).as_str());
        out.push_str("</p>");
    }
    out.push_str("</body></html>");
    out
}

/// A text file as a book titled `title`.
#[must_use]
pub fn text_book(text: &str, title: &str) -> (Container, Book) {
    // The chapters: (heading, paragraphs).
    let mut chapters: Vec<(Option<String>, Vec<String>)> = vec![(None, Vec::new())];
    for p in paragraphs(text) {
        if is_heading(&p) {
            chapters.push((Some(p), Vec::new()));
        } else if let Some(last) = chapters.last_mut() {
            last.1.push(p);
        }
    }
    if chapters
        .first()
        .is_some_and(|(h, ps)| h.is_none() && ps.is_empty())
    {
        chapters.remove(0);
    }
    if chapters.is_empty() {
        chapters.push((None, Vec::new()));
    }
    let mut files = Vec::new();
    let mut spine = Vec::new();
    let mut toc = Vec::new();
    for (heading, paragraphs) in chapters {
        // A long chapter in parts, each ending at a paragraph's end.
        let mut parts: Vec<Vec<String>> = vec![Vec::new()];
        let mut size = 0;
        for p in paragraphs {
            if size > 0 && size + p.len() > MAX_CHAPTER_CHARS {
                parts.push(Vec::new());
                size = 0;
            }
            size += p.len();
            if let Some(part) = parts.last_mut() {
                part.push(p);
            }
        }
        for (i, part) in parts.iter().enumerate() {
            let index = spine.len();
            let path = format!("text/part-{:03}.xhtml", index + 1);
            let label = heading.clone().unwrap_or_else(|| title.to_string());
            let shown_heading = if i == 0 { heading.as_deref() } else { None };
            let xhtml = chapter_xhtml(&label, shown_heading, part);
            spine.push(SpineItem {
                path: path.clone(),
                media_type: "application/xhtml+xml".to_string(),
                linear: true,
                size: xhtml.len() as u64,
            });
            if i == 0 {
                toc.push(TocEntry {
                    label,
                    path: path.clone(),
                    fragment: String::new(),
                    depth: 0,
                    chapter: Some(index),
                });
            }
            files.push((path, xhtml.into_bytes()));
        }
    }
    let book = Book {
        metadata: Metadata {
            title: title.to_string(),
            ..Metadata::default()
        },
        package_path: String::new(),
        manifest: Vec::new(),
        spine,
        toc,
        cover: None,
    };
    (Container::from_files(files), book)
}

/// An HTML file as a book of one chapter titled `title` (its `<title>` when it has one).
#[must_use]
pub fn html_book(html: &str, title: &str) -> (Container, Book) {
    let (xml, _) = crate::xmltree::parse_document(html, true);
    let own_title = crate::xmltree::find_first(xml.root.as_slice(), "title")
        .map(crate::xmltree::text)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| title.to_string());
    let path = "book.html".to_string();
    let book = Book {
        metadata: Metadata {
            title: own_title.clone(),
            ..Metadata::default()
        },
        package_path: String::new(),
        manifest: Vec::new(),
        spine: vec![SpineItem {
            path: path.clone(),
            media_type: "text/html".to_string(),
            linear: true,
            size: html.len() as u64,
        }],
        toc: vec![TocEntry {
            label: own_title,
            path: path.clone(),
            fragment: String::new(),
            depth: 0,
            chapter: Some(0),
        }],
        cover: None,
    };
    (
        Container::from_files(vec![(path, html.as_bytes().to_vec())]),
        book,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraphs_are_blank_line_blocks_with_their_lines_joined() {
        assert_eq!(
            paragraphs("It was a\r\ndark night.\r\n\r\n\r\nSecond   one.\n"),
            vec![
                "It was a dark night.".to_string(),
                "Second one.".to_string()
            ]
        );
        assert_eq!(
            paragraphs("one line\nanother line\n"),
            vec!["one line".to_string(), "another line".to_string()],
            "a file without blank lines has a paragraph per line"
        );
        assert!(paragraphs("  \n\n ").is_empty());
    }

    #[test]
    fn headings_are_short_lines_that_name_a_chapter() {
        assert!(is_heading("CHAPTER I."));
        assert!(is_heading("Chapter 12"));
        assert!(is_heading("BOOK TWO"));
        assert!(is_heading("Prologue"));
        assert!(is_heading("EPILOGUE:"));
        assert!(!is_heading(
            "Chapters of the history of a very long and winding road go on."
        ));
        assert!(!is_heading("Bookkeeping"));
        assert!(!is_heading("It was the best of times."));
    }

    #[test]
    fn a_text_file_is_a_book_in_chapters_at_its_headings() {
        let text = "A Tale\n\nby Someone\n\nCHAPTER I\n\nIt was a\ndark night & <cold>.\n\n\
                    Second para.\n\nCHAPTER II\n\nMore.";
        let (container, book) = text_book(text, "A Tale");
        assert_eq!(book.metadata.title, "A Tale");
        assert_eq!(book.spine.len(), 3);
        let labels: Vec<&str> = book.toc.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, vec!["A Tale", "CHAPTER I", "CHAPTER II"]);
        assert_eq!(book.toc[2].chapter, Some(2));
        let ch1 = container.text(&book.spine[1].path).expect("chapter 1");
        assert!(ch1.contains("<h2>CHAPTER I</h2><p>It was a dark night &amp; &lt;cold&gt;.</p><p>Second para.</p>"), "{ch1}");
        let (xml, how) = crate::xmltree::parse_document(&ch1, false);
        assert_eq!(
            how,
            crate::xmltree::Parsed::Xml,
            "the chapter is well-formed XHTML"
        );
        assert_eq!(crate::xmltree::find_all(xml.root.as_slice(), "p").len(), 2);
    }

    #[test]
    fn a_long_chapter_is_split_at_a_paragraph_end() {
        let para = "word ".repeat(2_000); // 10 000 characters
        let text: String = (0..15)
            .map(|_| para.clone())
            .collect::<Vec<_>>()
            .join("\n\n");
        let (_, book) = text_book(&text, "Long");
        assert!(book.spine.len() >= 3, "{} chapters", book.spine.len());
        assert_eq!(
            book.toc.len(),
            1,
            "the parts of one chapter share its entry"
        );
        assert!(book
            .spine
            .iter()
            .all(|s| s.size < (MAX_CHAPTER_CHARS + 12_000) as u64));
    }

    #[test]
    fn an_html_file_is_a_book_of_one_chapter_named_by_its_title() {
        let (container, book) = html_book("<title>Notes</title><p>One<p>Two", "file.html");
        assert_eq!(book.metadata.title, "Notes");
        assert_eq!(book.spine.len(), 1);
        assert!(book.spine[0].is_html());
        assert!(container.get("book.html").is_some());
        let (_, untitled) = html_book("<p>x", "file.html");
        assert_eq!(untitled.metadata.title, "file.html");
    }
}
