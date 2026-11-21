//! The text a file holds, for the index: a plain file's (UTF-8, or UTF-16 with a byte-order
//! mark), an office document's (Word, Excel, PowerPoint, OpenDocument, EPUB: the text nodes of
//! their XML parts, a line per paragraph), a mail's (subject, senders and recipients, its text
//! bodies), a PDF's through the app's extractor. Pictures, sound, video, archives and programs
//! are known by their names and never read.

use std::{
    io::{Cursor, Read},
    sync::Arc,
};

/// The text of a file kind the crate cannot read itself: a PDF, whose reader the app links
/// (azul's). `None`: no text.
pub type ExtractFn = Arc<dyn Fn(&[u8]) -> Option<String> + Send + Sync>;

/// What the app hands in for the kinds it reads.
#[derive(Clone, Default)]
pub struct Extractors {
    /// A PDF's text (`None`: PDFs are listed, not indexed).
    pub pdf: Option<ExtractFn>,
}

/// How a file is read for its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// As text (passed over when its first bytes hold a NUL: binary).
    Text,
    /// A zip of XML parts.
    Office,
    /// An RFC 5322 message (`.eml`).
    Mail,
    /// Through [`Extractors::pdf`].
    Pdf,
}

/// The most text one file puts into the index (bytes): a longer text is cut.
pub const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;

/// Files of these extensions are never read: their bytes are no text.
const BINARY: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff", "ico", "icns", "heic", "heif",
    "psd", "raw", "cr2", "nef", "dng", "mp3", "wav", "flac", "ogg", "oga", "m4a", "aac", "opus",
    "aiff", "mp4", "m4v", "mov", "avi", "mkv", "webm", "wmv", "mpg", "mpeg", "zip", "gz", "tgz",
    "bz2", "xz", "7z", "rar", "zst", "tar", "iso", "dmg", "pkg", "deb", "rpm", "exe", "dll",
    "so", "dylib", "o", "a", "lib", "obj", "bin", "class", "jar", "wasm", "pyc", "ttf", "otf",
    "woff", "woff2", "sqlite", "db", "doc", "xls", "ppt", "odg", "azkv", "rlib", "rmeta", "pdb",
];

/// How a file of this name is read, `None` for a kind that holds no text.
#[must_use]
pub fn kind_of(name: &str) -> Option<Kind> {
    let extension = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "docx" | "docm" | "dotx" | "xlsx" | "xlsm" | "pptx" | "pptm" | "odt" | "ods" | "odp"
        | "epub" => Some(Kind::Office),
        "eml" => Some(Kind::Mail),
        "pdf" => Some(Kind::Pdf),
        e if BINARY.contains(&e) => None,
        _ => Some(Kind::Text),
    }
}

/// The text of the file `name` whose bytes are `bytes` (a plain file's first
/// [`MAX_TEXT_BYTES`] are enough), cut to [`MAX_TEXT_BYTES`]; `None` when it holds none.
#[must_use]
pub fn extract(name: &str, bytes: &[u8], extractors: &Extractors) -> Option<String> {
    let text = match kind_of(name)? {
        Kind::Text => plain(bytes),
        Kind::Office => office(name, bytes),
        Kind::Mail => mail(bytes),
        Kind::Pdf => extractors.pdf.as_ref().and_then(|pdf| pdf(bytes)),
    }?;
    let text = cut(text);
    (!text.trim().is_empty()).then_some(text)
}

/// `text` cut to [`MAX_TEXT_BYTES`] at a character's boundary.
fn cut(mut text: String) -> String {
    if text.len() > MAX_TEXT_BYTES {
        let mut end = MAX_TEXT_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

/// A plain file's text: UTF-8 (invalid bytes replaced), or UTF-16 with its byte-order mark;
/// `None` for binary bytes (a NUL among the first 8 KB).
fn plain(bytes: &[u8]) -> Option<String> {
    let utf16 = |big: bool| {
        let units = bytes[2..].chunks_exact(2).map(|pair| {
            if big {
                u16::from_be_bytes([pair[0], pair[1]])
            } else {
                u16::from_le_bytes([pair[0], pair[1]])
            }
        });
        char::decode_utf16(units)
            .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect::<String>()
    };
    match bytes {
        [0xFF, 0xFE, ..] => return Some(utf16(false)),
        [0xFE, 0xFF, ..] => return Some(utf16(true)),
        _ => {}
    }
    if bytes.iter().take(8192).any(|b| *b == 0) {
        return None;
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Whether the zip entry `entry` of an office file with this extension holds its text.
fn office_part(extension: &str, entry: &str) -> bool {
    let xml = entry.ends_with(".xml");
    match extension {
        "docx" | "docm" | "dotx" => {
            entry == "word/document.xml"
                || entry == "word/footnotes.xml"
                || entry == "word/endnotes.xml"
                || ((entry.starts_with("word/header") || entry.starts_with("word/footer")) && xml)
        }
        // The shared strings hold a workbook's text (the sheets' cells hold their indices).
        "xlsx" | "xlsm" => entry == "xl/sharedStrings.xml",
        "pptx" | "pptm" => {
            (entry.starts_with("ppt/slides/slide") || entry.starts_with("ppt/notesSlides/"))
                && xml
        }
        "odt" | "ods" | "odp" => entry == "content.xml",
        "epub" => {
            entry.ends_with(".xhtml") || entry.ends_with(".html") || entry.ends_with(".htm")
        }
        _ => false,
    }
}

/// An entry name as a key that orders `slide2` before `slide10`: its digits as numbers.
fn natural(name: &str) -> Vec<(String, u64)> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut number: Option<u64> = None;
    for c in name.chars() {
        match (c.to_digit(10), number) {
            (Some(d), Some(n)) => number = Some(n.saturating_mul(10).saturating_add(u64::from(d))),
            (Some(d), None) => number = Some(u64::from(d)),
            (None, Some(n)) => {
                parts.push((std::mem::take(&mut text), n));
                number = None;
                text.push(c);
            }
            (None, None) => text.push(c),
        }
    }
    parts.push((text, number.unwrap_or(0)));
    parts
}

/// An office document's text: the text of its XML parts in order, a line per paragraph.
fn office(name: &str, bytes: &[u8]) -> Option<String> {
    let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut parts: Vec<String> = archive
        .file_names()
        .filter(|entry| office_part(&extension, entry))
        .map(str::to_string)
        .collect();
    parts.sort_by_key(|entry| natural(entry));
    let mut out = String::new();
    for entry in parts {
        if out.len() >= MAX_TEXT_BYTES {
            break;
        }
        let Ok(file) = archive.by_name(&entry) else {
            continue;
        };
        let mut xml = String::new();
        if file.take(MAX_TEXT_BYTES as u64 * 4).read_to_string(&mut xml).is_err() {
            continue;
        }
        xml_text(&xml, &mut out);
        if !out.ends_with('\n') && !out.is_empty() {
            out.push('\n');
        }
    }
    Some(out)
}

/// The elements whose start begins a line of the text: paragraphs, headings, cells, list
/// items, line breaks, a workbook's strings.
const BLOCKS: &[&str] = &[
    "p", "h", "h1", "h2", "h3", "h4", "h5", "h6", "si", "br", "cr", "tab", "li", "tr", "td",
    "th", "div", "title", "line-break",
];

/// The elements whose text is no text of the document (an EPUB page's style and scripts).
const SKIPPED: &[&str] = &["style", "script"];

/// The text nodes of `xml`, a line per block element, into `out`.
fn xml_text(xml: &str, out: &mut String) {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };
    let Ok(document) = roxmltree::Document::parse_with_options(xml, options) else {
        return;
    };
    for node in document.descendants() {
        if node.is_element() {
            if BLOCKS.contains(&node.tag_name().name()) && !out.is_empty() && !out.ends_with('\n')
            {
                out.push('\n');
            }
        } else if node.is_text() {
            let skipped = node
                .parent_element()
                .is_some_and(|parent| SKIPPED.contains(&parent.tag_name().name()));
            if let (false, Some(text)) = (skipped, node.text()) {
                out.push_str(text);
            }
        }
        if out.len() >= MAX_TEXT_BYTES {
            return;
        }
    }
}

/// A mail's text: its subject, the names and addresses of its senders and recipients, its
/// text bodies (an HTML-only one as text).
fn mail(bytes: &[u8]) -> Option<String> {
    let message = mail_parser::MessageParser::default().parse(bytes)?;
    let mut out = String::new();
    if let Some(subject) = message.subject() {
        out.push_str(subject);
        out.push('\n');
    }
    for address in [message.from(), message.to()].into_iter().flatten() {
        for one in address.as_list().unwrap_or_default() {
            for part in [one.name(), one.address()].into_iter().flatten() {
                out.push_str(part);
                out.push(' ');
            }
        }
        out.push('\n');
    }
    for body in 0..message.text_body_count() {
        if let Some(text) = message.body_text(body) {
            out.push_str(&text);
            out.push('\n');
        }
    }
    Some(out)
}
