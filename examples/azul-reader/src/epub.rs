//! An EPUB book: the container (a zip), the package (the OPF: metadata, manifest, spine), the
//! table of contents (the EPUB 3 navigation document, else the EPUB 2 NCX) and the paths
//! between them.
//!
//! Everything here is plain data read with azul's parsers - the zip through `azul::zip::Zip`,
//! every XML file through [`crate::xmltree::parse_document`] (azul's strict XML loader, the
//! HTML5-like parser as the fallback) - and tested without a window.
//!
//! Paths: every path is the file's path INSIDE the container (`OEBPS/text/ch1.xhtml`); an
//! `href` is resolved against the directory of the file that names it, percent-decoded, its
//! `#fragment` split off ([`resolve`]).

use std::collections::BTreeMap;

use azul::dom::XmlNode;

use crate::xmltree::{self, attr, child, children_named, elements, find_all, find_first, text};

/// Why a file is not a book AzReader can open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EpubError {
    /// The bytes are not a zip archive (or an empty one).
    NotAZip,
    /// No package document: no `META-INF/container.xml` naming one, and no `.opf` file.
    NoPackage,
    /// The package document names no chapter that is in the archive.
    NoChapters,
}

impl std::fmt::Display for EpubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            EpubError::NotAZip => "the file is not an EPUB book (not a zip archive)",
            EpubError::NoPackage => "the book has no package document (content.opf)",
            EpubError::NoChapters => "the book names no chapter that is in it",
        })
    }
}

/// The files of a book by their path in the container.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Container {
    files: BTreeMap<String, Vec<u8>>,
}

impl Container {
    /// A container of these `(path, bytes)` files (a test's book, a plain file).
    #[must_use]
    pub fn from_files<I: IntoIterator<Item = (String, Vec<u8>)>>(files: I) -> Container {
        Container {
            files: files.into_iter().collect(),
        }
    }

    /// The files of an EPUB (a zip archive), read with azul's `Zip`. Directories are left out.
    pub fn from_zip_bytes(bytes: &[u8]) -> Result<Container, EpubError> {
        let zip = azul::zip::Zip::from_bytes(bytes.to_vec());
        let mut files = BTreeMap::new();
        for i in 0..zip.file_count() {
            if zip.file_is_directory(i) {
                continue;
            }
            let path = zip.file_path(i).as_str().to_string();
            if path.is_empty() || path.ends_with('/') {
                continue;
            }
            files.insert(path, zip.file_data(i).as_slice().to_vec());
        }
        if files.is_empty() {
            return Err(EpubError::NotAZip);
        }
        Ok(Container { files })
    }

    /// The bytes of the file at `path`: the exact path, else the one path that differs only
    /// in case (a package that writes `Text/Ch1.xhtml` for `text/ch1.xhtml`).
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        if let Some(bytes) = self.files.get(path) {
            return Some(bytes.as_slice());
        }
        self.files
            .iter()
            .find(|(p, _)| p.eq_ignore_ascii_case(path))
            .map(|(_, bytes)| bytes.as_slice())
    }

    /// The file at `path` as text: UTF-8 (a byte-order mark dropped), UTF-16 with its mark,
    /// a byte that is not UTF-8 as U+FFFD.
    #[must_use]
    pub fn text(&self, path: &str) -> Option<String> {
        self.get(path).map(decode_text)
    }

    /// Every path, in order.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    /// The number of files.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// No file at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// `bytes` as text: see [`Container::text`].
#[must_use]
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF_u8, 0xBB, 0xBF][..]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    let utf16 = |rest: &[u8], little: bool| -> String {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| {
                if little {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF_u8, 0xFE][..]) {
        return utf16(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE_u8, 0xFF][..]) {
        return utf16(rest, false);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// What the package says about the book.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    pub title: String,
    /// The creators, in order (`dc:creator`).
    pub authors: Vec<String>,
    pub language: String,
    pub identifier: String,
    pub publisher: String,
    pub description: String,
    pub date: String,
}

/// One file the package lists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManifestItem {
    pub id: String,
    /// The file's path in the container.
    pub path: String,
    pub media_type: String,
    /// `properties` (EPUB 3): `nav`, `cover-image`, `svg`, ...
    pub properties: Vec<String>,
}

/// One chapter in reading order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpineItem {
    /// The chapter's path in the container.
    pub path: String,
    pub media_type: String,
    /// `linear="no"`: outside the main flow (notes, a cover page) - still read in order.
    pub linear: bool,
    /// The file's size in bytes: the chapter's weight in the book's progress.
    pub size: u64,
}

impl SpineItem {
    /// The chapter is HTML (not XHTML): read with the HTML parser straight away.
    #[must_use]
    pub fn is_html(&self) -> bool {
        self.media_type.eq_ignore_ascii_case("text/html")
            || self.path.to_ascii_lowercase().ends_with(".html")
            || self.path.to_ascii_lowercase().ends_with(".htm")
    }
}

/// One line of the table of contents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TocEntry {
    pub label: String,
    /// The file it points to (its path in the container).
    pub path: String,
    /// The `#fragment` (an element id in that file), "" for the file's start.
    pub fragment: String,
    /// 0 for a top-level entry, 1 for one nested in it, ...
    pub depth: usize,
    /// The spine index of [`TocEntry::path`] (`None`: the file is not a chapter).
    pub chapter: Option<usize>,
}

/// A book: what the package and the table of contents say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Book {
    pub metadata: Metadata,
    /// The package document's path (`OEBPS/content.opf`).
    pub package_path: String,
    pub manifest: Vec<ManifestItem>,
    /// The chapters in reading order (only files that are in the container).
    pub spine: Vec<SpineItem>,
    pub toc: Vec<TocEntry>,
    /// The cover image's path, if the book names one.
    pub cover: Option<String>,
}

impl Book {
    /// The spine index of the chapter at `path`.
    #[must_use]
    pub fn chapter_of_path(&self, path: &str) -> Option<usize> {
        self.spine.iter().position(|s| s.path == path).or_else(|| {
            self.spine
                .iter()
                .position(|s| s.path.eq_ignore_ascii_case(path))
        })
    }

    /// Every chapter's weight in the book's progress (its size, at least 1).
    #[must_use]
    pub fn chapter_weights(&self) -> Vec<u64> {
        self.spine.iter().map(|s| s.size.max(1)).collect()
    }

    /// The title of chapter `chapter`: the first entry of the table of contents that points
    /// into it, else the last entry that points before it (the part it belongs to), else "".
    #[must_use]
    pub fn chapter_title(&self, chapter: usize) -> String {
        if let Some(entry) = self.toc.iter().find(|e| e.chapter == Some(chapter)) {
            return entry.label.clone();
        }
        self.toc
            .iter()
            .filter(|e| e.chapter.is_some_and(|c| c < chapter))
            .last()
            .map(|e| e.label.clone())
            .unwrap_or_default()
    }

    /// The media type the manifest gives the file at `path`.
    #[must_use]
    pub fn media_type_of(&self, path: &str) -> Option<&str> {
        self.manifest
            .iter()
            .find(|m| m.path == path)
            .map(|m| m.media_type.as_str())
    }
}

/// The directory part of a container path, with its trailing `/` (`""` at the top).
#[must_use]
pub fn dir_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..=i],
        None => "",
    }
}

/// `%xx` escapes decoded (an invalid one stays as written).
#[must_use]
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let hex = |b: u8| -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(hi * 16 + lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// An `href` in the file at `base` (a container path) as `(path, fragment)`: relative to
/// `base`'s directory (`..` and `.` folded, a leading `/` = the container's top),
/// percent-decoded; `#x` alone is `base` itself with fragment `x`.
#[must_use]
pub fn resolve(base: &str, href: &str) -> (String, String) {
    let href = href.trim();
    let (path_part, fragment) = match href.split_once('#') {
        Some((p, f)) => (p, f),
        None => (href, ""),
    };
    // A query is no part of a file's path.
    let path_part = path_part.split('?').next().unwrap_or("");
    let fragment = percent_decode(fragment);
    if path_part.is_empty() {
        return (base.to_string(), fragment);
    }
    let path_part = percent_decode(path_part);
    let joined = match path_part.strip_prefix('/') {
        Some(absolute) => absolute.to_string(),
        None => format!("{}{}", dir_of(base), path_part),
    };
    let mut segments: Vec<&str> = Vec::new();
    for segment in joined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            s => segments.push(s),
        }
    }
    (segments.join("/"), fragment)
}

/// The package document's path: what `META-INF/container.xml` names (its first rootfile of
/// the OPF type), else the first `.opf` file in the container.
#[must_use]
pub fn package_path(container: &Container) -> Option<String> {
    if let Some(text) = container.text("META-INF/container.xml") {
        let (xml, _) = xmltree::parse_document(&text, false);
        for rootfile in find_all(xml.root.as_slice(), "rootfile") {
            let media = attr(rootfile, "media-type").unwrap_or("").trim();
            let Some(path) = attr(rootfile, "full-path") else {
                continue;
            };
            let path = path.trim().trim_start_matches('/').to_string();
            let opf =
                media.is_empty() || media.eq_ignore_ascii_case("application/oebps-package+xml");
            if opf && container.get(&path).is_some() {
                return Some(path);
            }
        }
    }
    container
        .paths()
        .find(|p| p.to_ascii_lowercase().ends_with(".opf"))
        .map(str::to_string)
}

/// Reads the book in `container`.
pub fn parse_book(container: &Container) -> Result<Book, EpubError> {
    let package_path = package_path(container).ok_or(EpubError::NoPackage)?;
    let package_text = container.text(&package_path).ok_or(EpubError::NoPackage)?;
    let (xml, _) = xmltree::parse_document(&package_text, false);
    let root = xml.root.as_slice();
    let metadata_node = find_first(root, "metadata");
    let metadata = metadata_node.map(read_metadata).unwrap_or_default();

    let manifest: Vec<ManifestItem> = find_first(root, "manifest")
        .map(|m| find_all(m.children.as_slice(), "item"))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| {
            let href = attr(item, "href")?;
            let (path, _) = resolve(&package_path, href);
            Some(ManifestItem {
                id: attr(item, "id").unwrap_or("").trim().to_string(),
                path,
                media_type: attr(item, "media-type").unwrap_or("").trim().to_string(),
                properties: attr(item, "properties")
                    .unwrap_or("")
                    .split_whitespace()
                    .map(str::to_string)
                    .collect(),
            })
        })
        .collect();

    let spine_node = find_first(root, "spine");
    let mut spine = Vec::new();
    if let Some(spine_node) = spine_node {
        for itemref in children_named(spine_node, "itemref") {
            let Some(idref) = attr(itemref, "idref").map(str::trim) else {
                continue;
            };
            let Some(item) = manifest.iter().find(|m| m.id == idref) else {
                continue;
            };
            let Some(bytes) = container.get(&item.path) else {
                continue;
            };
            let linear =
                !attr(itemref, "linear").is_some_and(|l| l.trim().eq_ignore_ascii_case("no"));
            spine.push(SpineItem {
                path: item.path.clone(),
                media_type: item.media_type.clone(),
                linear,
                size: bytes.len() as u64,
            });
        }
    }
    if spine.is_empty() {
        return Err(EpubError::NoChapters);
    }

    let has = |m: &ManifestItem, property: &str| m.properties.iter().any(|p| p == property);
    let cover = manifest
        .iter()
        .find(|m| has(m, "cover-image"))
        .map(|m| m.path.clone())
        .or_else(|| {
            let meta = metadata_node.and_then(|md| {
                find_all(md.children.as_slice(), "meta")
                    .into_iter()
                    .find(|m| {
                        attr(m, "name").is_some_and(|n| n.trim().eq_ignore_ascii_case("cover"))
                    })
            })?;
            let id = attr(meta, "content")?.trim();
            manifest.iter().find(|m| m.id == id).map(|m| m.path.clone())
        })
        .or_else(|| {
            manifest
                .iter()
                .find(|m| {
                    m.media_type.to_ascii_lowercase().starts_with("image/")
                        && (m.id.to_ascii_lowercase().contains("cover")
                            || m.path.to_ascii_lowercase().contains("cover"))
                })
                .map(|m| m.path.clone())
        })
        .filter(|p| container.get(p).is_some());

    let mut toc = Vec::new();
    if let Some(nav) = manifest.iter().find(|m| has(m, "nav")) {
        if let Some(nav_text) = container.text(&nav.path) {
            toc = nav_toc(&nav_text, &nav.path);
        }
    }
    if toc.is_empty() {
        let ncx_id = spine_node.and_then(|s| attr(s, "toc")).map(str::trim);
        let ncx = ncx_id
            .and_then(|id| manifest.iter().find(|m| m.id == id))
            .or_else(|| {
                manifest.iter().find(|m| {
                    m.media_type
                        .eq_ignore_ascii_case("application/x-dtbncx+xml")
                })
            });
        if let Some(ncx) = ncx {
            if let Some(ncx_text) = container.text(&ncx.path) {
                toc = ncx_toc(&ncx_text, &ncx.path);
            }
        }
    }
    if toc.is_empty() {
        toc = spine_toc(container, &spine);
    }
    let mut book = Book {
        metadata,
        package_path,
        manifest,
        spine,
        toc: Vec::new(),
        cover,
    };
    for entry in &mut toc {
        if !entry.path.is_empty() {
            entry.chapter = book.chapter_of_path(&entry.path);
        }
    }
    book.toc = toc;
    Ok(book)
}

/// The package's `<metadata>`: the Dublin Core elements by their local names, at any depth
/// (an OEB 1.2 package nests them in `<dc-metadata>`).
fn read_metadata(md: &XmlNode) -> Metadata {
    let all = |name: &str| -> Vec<String> {
        find_all(md.children.as_slice(), name)
            .into_iter()
            .map(text)
            .filter(|t| !t.is_empty())
            .collect()
    };
    let first = |name: &str| all(name).into_iter().next().unwrap_or_default();
    Metadata {
        title: first("title"),
        authors: all("creator"),
        language: first("language"),
        identifier: first("identifier"),
        publisher: first("publisher"),
        description: first("description"),
        date: first("date"),
    }
}

/// The EPUB 3 navigation document's `toc` nav (else its first nav) as entries.
fn nav_toc(nav_text: &str, nav_path: &str) -> Vec<TocEntry> {
    let (xml, _) = xmltree::parse_document(nav_text, false);
    let navs = find_all(xml.root.as_slice(), "nav");
    let is_toc = |n: &XmlNode| {
        attr(n, "type").is_some_and(|t| t.split_whitespace().any(|w| w.eq_ignore_ascii_case("toc")))
    };
    let Some(nav) = navs
        .iter()
        .copied()
        .find(|n| is_toc(*n))
        .or_else(|| navs.first().copied())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(list) = find_first(nav.children.as_slice(), "ol") {
        nav_list(list, 0, nav_path, &mut out);
    }
    out
}

/// One `<ol>` of the navigation document: each `<li>`'s link (or heading `<span>`), then its
/// nested list one level deeper.
fn nav_list(list: &XmlNode, depth: usize, base: &str, out: &mut Vec<TocEntry>) {
    for item in children_named(list, "li") {
        let link = elements(item).find(|e| matches!(xmltree::name(e).as_str(), "a" | "span"));
        if let Some(link) = link {
            let mut label = text(link);
            if label.is_empty() {
                label = attr(link, "title")
                    .map(xmltree::fold_space)
                    .unwrap_or_default();
            }
            let (path, fragment) = match attr(link, "href") {
                Some(href) if xmltree::name(link) == "a" => resolve(base, href),
                _ => (String::new(), String::new()),
            };
            if !label.is_empty() || !path.is_empty() {
                out.push(TocEntry {
                    label,
                    path,
                    fragment,
                    depth,
                    chapter: None,
                });
            }
        }
        if let Some(sub) = child(item, "ol") {
            nav_list(sub, depth + 1, base, out);
        }
    }
}

/// The EPUB 2 NCX's `navMap` as entries (nested `navPoint`s one level deeper).
fn ncx_toc(ncx_text: &str, ncx_path: &str) -> Vec<TocEntry> {
    let (xml, _) = xmltree::parse_document(ncx_text, false);
    let mut out = Vec::new();
    if let Some(map) = find_first(xml.root.as_slice(), "navmap") {
        ncx_points(map, 0, ncx_path, &mut out);
    }
    out
}

fn ncx_points(parent: &XmlNode, depth: usize, base: &str, out: &mut Vec<TocEntry>) {
    for point in children_named(parent, "navpoint") {
        let label = child(point, "navlabel")
            .and_then(|l| child(l, "text"))
            .map(text)
            .unwrap_or_default();
        let (path, fragment) = child(point, "content")
            .and_then(|c| attr(c, "src"))
            .map(|src| resolve(base, src))
            .unwrap_or_default();
        out.push(TocEntry {
            label,
            path,
            fragment,
            depth,
            chapter: None,
        });
        ncx_points(point, depth + 1, base, out);
    }
}

/// A book without a table of contents: each chapter by its title (its `<title>`, else its
/// first heading, else "Chapter N").
fn spine_toc(container: &Container, spine: &[SpineItem]) -> Vec<TocEntry> {
    spine
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let label = container
                .text(&item.path)
                .map(|t| chapter_label(&t, item.is_html()))
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| format!("Chapter {}", i + 1));
            TocEntry {
                label,
                path: item.path.clone(),
                fragment: String::new(),
                depth: 0,
                chapter: Some(i),
            }
        })
        .collect()
}

/// A chapter's own title: its `<title>`, else its first `h1` / `h2` / `h3`.
fn chapter_label(source: &str, html: bool) -> String {
    let (xml, _) = xmltree::parse_document(source, html);
    let root = xml.root.as_slice();
    for name in ["title", "h1", "h2", "h3"] {
        if let Some(node) = find_first(root, name) {
            let label = text(node);
            if !label.is_empty() {
                return label;
            }
        }
    }
    String::new()
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! Small books for the tests (here and in the other modules).

    use super::Container;

    pub const CONTAINER_XML: &str = "<?xml version=\"1.0\"?>\n\
        <container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\
        <rootfiles><rootfile full-path=\"OEBPS/content.opf\" \
        media-type=\"application/oebps-package+xml\"/></rootfiles></container>";

    pub const OPF3: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
        <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\">\
        <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
        <dc:identifier id=\"uid\">urn:uuid:1234</dc:identifier>\
        <dc:title>The Tale of Two Files</dc:title>\
        <dc:creator>Ada Writer</dc:creator><dc:creator>Bo Coauthor</dc:creator>\
        <dc:language>en</dc:language><dc:publisher>Azul Press</dc:publisher>\
        <dc:date>2026-10-03</dc:date>\
        </metadata>\
        <manifest>\
        <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\
        <item id=\"c2\" href=\"text/ch2.xhtml\" media-type=\"application/xhtml+xml\"/>\
        <item id=\"c1\" href=\"text/ch1.xhtml\" media-type=\"application/xhtml+xml\"/>\
        <item id=\"css\" href=\"css/book.css\" media-type=\"text/css\"/>\
        <item id=\"img\" href=\"images/cover%20art.jpg\" media-type=\"image/jpeg\" properties=\"cover-image\"/>\
        <item id=\"gone\" href=\"text/missing.xhtml\" media-type=\"application/xhtml+xml\"/>\
        </manifest>\
        <spine><itemref idref=\"c1\"/><itemref idref=\"gone\"/><itemref idref=\"c2\" linear=\"no\"/></spine>\
        </package>";

    pub const NAV: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
        <!DOCTYPE html>\n\
        <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\
        <head><title>Contents</title></head><body>\
        <nav epub:type=\"landmarks\"><ol><li><a href=\"text/ch1.xhtml\">Start</a></li></ol></nav>\
        <nav epub:type=\"toc\"><h1>Contents</h1><ol>\
        <li><a href=\"text/ch1.xhtml\">Chapter  One</a>\
        <ol><li><a href=\"text/ch1.xhtml#part-2\">A <em>second</em> part</a></li></ol></li>\
        <li><span>Appendix</span><ol><li><a href=\"text/ch2.xhtml\">Chapter Two</a></li></ol></li>\
        </ol></nav></body></html>";

    pub const CH1: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
        <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>One</title>\
        <link rel=\"stylesheet\" type=\"text/css\" href=\"../css/book.css\"/></head>\
        <body><h1>Chapter One</h1><p>It was the best of times.</p>\
        <p id=\"part-2\"><a id=\"x\"/>It was the worst of times.</p>\
        <p><img src=\"../images/cover%20art.jpg\" alt=\"Cover\"/></p></body></html>";

    pub const CH2: &str =
        "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>Two</title></head>\
        <body><h1>Chapter Two</h1><p>The end.</p></body></html>";

    pub const CSS: &str = "p { margin: 0 0 1em 0; text-indent: 1.5em; color: #333; }";

    /// The EPUB 3 book: a nav document, two chapters (one missing from the zip), a cover.
    pub fn book3() -> Container {
        Container::from_files(vec![
            ("mimetype".to_string(), b"application/epub+zip".to_vec()),
            (
                "META-INF/container.xml".to_string(),
                CONTAINER_XML.as_bytes().to_vec(),
            ),
            ("OEBPS/content.opf".to_string(), OPF3.as_bytes().to_vec()),
            ("OEBPS/nav.xhtml".to_string(), NAV.as_bytes().to_vec()),
            ("OEBPS/text/ch1.xhtml".to_string(), CH1.as_bytes().to_vec()),
            ("OEBPS/text/ch2.xhtml".to_string(), CH2.as_bytes().to_vec()),
            ("OEBPS/css/book.css".to_string(), CSS.as_bytes().to_vec()),
            (
                "OEBPS/images/cover art.jpg".to_string(),
                vec![0xFF, 0xD8, 0xFF],
            ),
        ])
    }

    pub const OPF2: &str = "<?xml version=\"1.0\"?>\
        <opf:package xmlns:opf=\"http://www.idpf.org/2007/opf\" version=\"2.0\">\
        <opf:metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
        <dc:title>  Old   Style  </dc:title><dc:creator opf:role=\"aut\">C. Writer</dc:creator>\
        <opf:meta name=\"cover\" content=\"cov\"/></opf:metadata>\
        <opf:manifest><opf:item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>\
        <opf:item id=\"a\" href=\"Chapter%201.html\" media-type=\"application/xhtml+xml\"/>\
        <opf:item id=\"b\" href=\"chapter2.html\" media-type=\"application/xhtml+xml\"/>\
        <opf:item id=\"cov\" href=\"cover.png\" media-type=\"image/png\"/></opf:manifest>\
        <opf:spine toc=\"ncx\"><opf:itemref idref=\"a\"/><opf:itemref idref=\"b\"/></opf:spine>\
        </opf:package>";

    pub const NCX: &str = "<?xml version=\"1.0\"?>\
        <ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\"><navMap>\
        <navPoint id=\"n1\" playOrder=\"1\"><navLabel><text>One</text></navLabel>\
        <content src=\"Chapter%201.html\"/>\
        <navPoint id=\"n2\" playOrder=\"2\"><navLabel><text>One, again</text></navLabel>\
        <content src=\"Chapter%201.html#again\"/></navPoint></navPoint>\
        <navPoint id=\"n3\" playOrder=\"3\"><navLabel><text>Two</text></navLabel>\
        <content src=\"chapter2.html\"/></navPoint></navMap></ncx>";

    /// The EPUB 2 book: no container.xml (the package is found by its suffix), an NCX.
    pub fn book2() -> Container {
        Container::from_files(vec![
            ("book.opf".to_string(), OPF2.as_bytes().to_vec()),
            ("toc.ncx".to_string(), NCX.as_bytes().to_vec()),
            ("Chapter 1.html".to_string(), b"<p>One</p>".to_vec()),
            ("chapter2.html".to_string(), b"<p>Two</p>".to_vec()),
            ("cover.png".to_string(), vec![0x89, b'P', b'N', b'G']),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::{fixtures::*, *};

    #[test]
    fn an_href_is_resolved_against_the_directory_of_the_file_that_names_it() {
        assert_eq!(dir_of("OEBPS/text/ch1.xhtml"), "OEBPS/text/");
        assert_eq!(dir_of("ch1.xhtml"), "");
        let r = |base: &str, href: &str| resolve(base, href);
        assert_eq!(
            r("OEBPS/content.opf", "text/ch1.xhtml"),
            ("OEBPS/text/ch1.xhtml".into(), "".into())
        );
        assert_eq!(
            r("OEBPS/text/ch1.xhtml", "../images/cover%20art.jpg"),
            ("OEBPS/images/cover art.jpg".into(), "".into())
        );
        assert_eq!(
            r("OEBPS/text/ch1.xhtml", "#note-3"),
            ("OEBPS/text/ch1.xhtml".into(), "note-3".into())
        );
        assert_eq!(
            r("OEBPS/nav.xhtml", "./text/ch2.xhtml#s1"),
            ("OEBPS/text/ch2.xhtml".into(), "s1".into())
        );
        assert_eq!(
            r("OEBPS/text/ch1.xhtml", "/OEBPS/x.css"),
            ("OEBPS/x.css".into(), "".into())
        );
        assert_eq!(
            r("a.opf", "../../escape.xhtml"),
            ("escape.xhtml".into(), "".into()),
            "never above the top"
        );
        assert_eq!(percent_decode("a%20b%2Fc%zz%"), "a b/c%zz%");
        assert_eq!(percent_decode("caf%C3%A9"), "caf\u{e9}");
    }

    #[test]
    fn text_files_are_decoded_from_utf8_and_utf16_with_their_marks() {
        assert_eq!(decode_text(b"\xEF\xBB\xBFhello"), "hello");
        assert_eq!(decode_text(&[0xFF, 0xFE, b'h', 0, b'i', 0]), "hi");
        assert_eq!(decode_text(&[0xFE, 0xFF, 0, b'h', 0, b'i']), "hi");
        assert_eq!(decode_text(b"a\xFFb"), "a\u{FFFD}b");
    }

    #[test]
    fn a_container_finds_a_file_by_its_path_and_then_without_case() {
        let c = Container::from_files(vec![("OEBPS/Text/Ch1.xhtml".to_string(), b"x".to_vec())]);
        assert_eq!(c.get("OEBPS/Text/Ch1.xhtml"), Some(&b"x"[..]));
        assert_eq!(c.get("oebps/text/ch1.xhtml"), Some(&b"x"[..]));
        assert_eq!(c.get("OEBPS/Text/Ch2.xhtml"), None);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn the_package_is_the_one_container_xml_names_else_the_first_opf() {
        assert_eq!(package_path(&book3()).as_deref(), Some("OEBPS/content.opf"));
        assert_eq!(package_path(&book2()).as_deref(), Some("book.opf"));
        assert_eq!(
            package_path(&Container::from_files(vec![("a.txt".to_string(), vec![])])),
            None
        );
    }

    #[test]
    fn the_package_gives_the_metadata_and_the_chapters_in_spine_order() {
        let book = parse_book(&book3()).expect("a book");
        assert_eq!(book.package_path, "OEBPS/content.opf");
        assert_eq!(book.metadata.title, "The Tale of Two Files");
        assert_eq!(
            book.metadata.authors,
            vec!["Ada Writer".to_string(), "Bo Coauthor".to_string()]
        );
        assert_eq!(book.metadata.language, "en");
        assert_eq!(book.metadata.identifier, "urn:uuid:1234");
        assert_eq!(book.metadata.publisher, "Azul Press");
        let paths: Vec<&str> = book.spine.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["OEBPS/text/ch1.xhtml", "OEBPS/text/ch2.xhtml"],
            "spine order (not manifest order); a chapter missing from the zip is left out"
        );
        assert!(book.spine[0].linear);
        assert!(
            !book.spine[1].linear,
            "linear=\"no\" is kept, read in order"
        );
        assert_eq!(book.spine[0].size, CH1.len() as u64);
        assert_eq!(
            book.chapter_weights(),
            vec![CH1.len() as u64, CH2.len() as u64]
        );
        assert_eq!(
            book.cover.as_deref(),
            Some("OEBPS/images/cover art.jpg"),
            "properties=cover-image"
        );
        assert_eq!(book.media_type_of("OEBPS/css/book.css"), Some("text/css"));
        assert_eq!(book.chapter_of_path("OEBPS/text/ch2.xhtml"), Some(1));
    }

    #[test]
    fn the_epub3_navigation_document_is_the_table_of_contents_with_its_nesting() {
        let book = parse_book(&book3()).expect("a book");
        let toc: Vec<(&str, &str, &str, usize, Option<usize>)> = book
            .toc
            .iter()
            .map(|e| {
                (
                    e.label.as_str(),
                    e.path.as_str(),
                    e.fragment.as_str(),
                    e.depth,
                    e.chapter,
                )
            })
            .collect();
        assert_eq!(
            toc,
            vec![
                ("Chapter One", "OEBPS/text/ch1.xhtml", "", 0, Some(0)),
                (
                    "A second part",
                    "OEBPS/text/ch1.xhtml",
                    "part-2",
                    1,
                    Some(0)
                ),
                ("Appendix", "", "", 0, None),
                ("Chapter Two", "OEBPS/text/ch2.xhtml", "", 1, Some(1)),
            ],
            "the toc nav (not the landmarks), labels with their white space folded, a heading \
             without a link kept as a label"
        );
        assert_eq!(
            book.chapter_title(0),
            "Chapter One",
            "the first entry into the chapter"
        );
        assert_eq!(book.chapter_title(1), "Chapter Two");
        let mut later = book.clone();
        later.spine.push(SpineItem {
            path: "OEBPS/text/ch3.xhtml".into(),
            ..SpineItem::default()
        });
        assert_eq!(
            later.chapter_title(2),
            "Chapter Two",
            "a chapter without an entry: the part before"
        );
    }

    #[test]
    fn an_epub2_book_reads_its_ncx_and_its_cover_meta() {
        let book = parse_book(&book2()).expect("a book");
        assert_eq!(book.metadata.title, "Old Style", "white space folded");
        assert_eq!(book.metadata.authors, vec!["C. Writer".to_string()]);
        assert_eq!(book.spine.len(), 2);
        assert_eq!(book.spine[0].path, "Chapter 1.html", "percent-decoded");
        assert!(book.spine[0].is_html(), "an .html chapter is read as HTML");
        assert_eq!(
            book.cover.as_deref(),
            Some("cover.png"),
            "<meta name=cover content=id>"
        );
        let toc: Vec<(&str, &str, usize, Option<usize>)> = book
            .toc
            .iter()
            .map(|e| (e.label.as_str(), e.fragment.as_str(), e.depth, e.chapter))
            .collect();
        assert_eq!(
            toc,
            vec![
                ("One", "", 0, Some(0)),
                ("One, again", "again", 1, Some(0)),
                ("Two", "", 0, Some(1))
            ]
        );
    }

    #[test]
    fn a_book_without_a_table_of_contents_lists_its_chapters() {
        let mut c = book3();
        let opf = OPF3.replace(" properties=\"nav\"", "");
        c = Container::from_files(
            c.paths()
                .map(|p| {
                    let bytes = if p == "OEBPS/content.opf" {
                        opf.as_bytes().to_vec()
                    } else {
                        c.get(p).unwrap_or_default().to_vec()
                    };
                    (p.to_string(), bytes)
                })
                .collect::<Vec<_>>(),
        );
        let book = parse_book(&c).expect("a book");
        let labels: Vec<&str> = book.toc.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, vec!["One", "Two"], "each chapter by its <title>");
        assert_eq!(book.toc[1].chapter, Some(1));
    }

    #[test]
    fn what_is_not_a_book_says_why() {
        assert_eq!(
            Container::from_zip_bytes(b"not a zip at all"),
            Err(EpubError::NotAZip)
        );
        assert_eq!(
            parse_book(&Container::from_files(vec![(
                "a.txt".to_string(),
                b"x".to_vec()
            )])),
            Err(EpubError::NoPackage)
        );
        let empty_spine = OPF3.replace(
            "<itemref idref=\"c1\"/><itemref idref=\"gone\"/><itemref idref=\"c2\" linear=\"no\"/>",
            "",
        );
        let c = Container::from_files(vec![
            (
                "META-INF/container.xml".to_string(),
                CONTAINER_XML.as_bytes().to_vec(),
            ),
            ("OEBPS/content.opf".to_string(), empty_spine.into_bytes()),
        ]);
        assert_eq!(parse_book(&c), Err(EpubError::NoChapters));
        assert!(EpubError::NoPackage.to_string().contains("package"));
    }

    #[test]
    fn an_epub_zip_made_with_azuls_zip_reads_back_as_the_same_book() {
        let mut zip = azul::zip::Zip::create();
        for path in book3().paths().map(str::to_string).collect::<Vec<_>>() {
            let bytes = book3().get(&path).unwrap_or_default().to_vec();
            zip.add_file(path, bytes);
        }
        zip.add_directory("OEBPS/");
        let bytes: Vec<u8> = zip.to_bytes().as_slice().to_vec();
        let c = Container::from_zip_bytes(&bytes).expect("a zip");
        assert_eq!(c, book3(), "every file, no directory entry");
        assert_eq!(
            parse_book(&c).expect("a book").metadata.title,
            "The Tale of Two Files"
        );
    }
}
