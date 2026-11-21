//! What kind of file this is, from its first bytes (and its extension only as a tie-break).
//!
//! The kind decides the pipeline:
//! - **dense** kinds are skipped early, without a trial: video, audio, HEIC/AVIF/WebP/GIF, JPEG,
//!   PNG, gzip, and archives that are compressed already (zstd, xz, 7z, RAR, ...). zstd would
//!   save under 1 % on them, so trying costs time for nothing;
//! - **JPEG images** (and TIFF and camera RAW files, which hold JPEG previews) go to the Lepton
//!   transform;
//! - **deflate containers** (PDF, ZIP and Office files, EPUB, PNG, gzip) go to the containers
//!   transform: their deflate streams become plain text plus the bytes that rebuild them, and
//!   the JPEG images inside go through Lepton. The plain text is then compressed, even for PNG
//!   and gzip;
//! - **PDF, ZIP and Office files are not dense**: their streams and members compress by 4 to
//!   24 % with zstd alone, so they get the first-segment trial even when the transform does not
//!   run;
//! - **text** (JSON, CSV, markup, source code, prose) also tries brotli, and small text files
//!   try the shipped dictionary of their kind.
//!
//! Everything else gets the trial on its first segment (with early abort) and the 5 % rule.

/// How many bytes of a file's start detection looks at.
pub const SNIFF_LEN: usize = 4096;

/// A file's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Jpeg,
    Png,
    Gif,
    WebP,
    /// HEIC and AVIF (HEVC / AV1 stills in ISO media boxes).
    Heif,
    JpegXl,
    /// TIFF, and the camera RAW files built on it (DNG, CR2, NEF, ARW, ...) or on ISO media (CR3).
    Tiff,
    /// MP4, MOV, 3GP, Matroska, WebM, AVI.
    Video,
    /// AAC / M4A, MP3, Ogg (Vorbis, Opus), FLAC.
    Audio,
    /// Uncompressed audio: WAV, AIFF.
    Wav,
    Pdf,
    /// A ZIP archive that is not one of the document kinds below (also JAR, APK).
    Zip,
    /// Office documents in ZIP: DOCX / XLSX / PPTX and OpenDocument (ODT, ODS, ODP).
    Office,
    Epub,
    Gzip,
    /// Compressed already: zstd, xz, bzip2, 7z, RAR, LZ4, CAB.
    Archive,
    /// WOFF and WOFF2 web fonts (compressed inside).
    Font,
    Json,
    Csv,
    /// XML, HTML, SVG.
    Markup,
    /// Source code.
    Code,
    /// Any other text: prose, Markdown, logs, configuration.
    Text,
    Sqlite,
    /// Nothing recognised.
    Binary,
}

impl Kind {
    /// Whether the codec stage skips this kind without a trial: compressed already, and zstd
    /// saves under 1 %.
    #[must_use]
    pub fn is_dense(self) -> bool {
        matches!(
            self,
            Kind::Jpeg
                | Kind::Png
                | Kind::Gif
                | Kind::WebP
                | Kind::Heif
                | Kind::JpegXl
                | Kind::Video
                | Kind::Audio
                | Kind::Gzip
                | Kind::Archive
                | Kind::Font
        )
    }

    /// Whether the file may hold JPEG images worth giving to Lepton (the file itself, or
    /// pictures stored in it without further compression).
    #[must_use]
    pub fn may_hold_jpegs(self) -> bool {
        matches!(
            self,
            Kind::Jpeg | Kind::Pdf | Kind::Zip | Kind::Office | Kind::Epub | Kind::Tiff
        )
    }

    /// Whether the file keeps deflate streams that the containers transform rebuilds: PDF
    /// (FlateDecode), ZIP and its document kinds (deflated members), PNG (IDAT) and gzip.
    #[must_use]
    pub fn holds_deflate_streams(self) -> bool {
        matches!(
            self,
            Kind::Pdf | Kind::Zip | Kind::Office | Kind::Epub | Kind::Png | Kind::Gzip
        )
    }

    /// Whether the file is text, where brotli is tried next to zstd.
    #[must_use]
    pub fn is_text(self) -> bool {
        matches!(
            self,
            Kind::Json | Kind::Csv | Kind::Markup | Kind::Code | Kind::Text
        )
    }

    /// A short lower-case name, for reports and logs.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Kind::Jpeg => "jpeg",
            Kind::Png => "png",
            Kind::Gif => "gif",
            Kind::WebP => "webp",
            Kind::Heif => "heif",
            Kind::JpegXl => "jpegxl",
            Kind::Tiff => "tiff",
            Kind::Video => "video",
            Kind::Audio => "audio",
            Kind::Wav => "wav",
            Kind::Pdf => "pdf",
            Kind::Zip => "zip",
            Kind::Office => "office",
            Kind::Epub => "epub",
            Kind::Gzip => "gzip",
            Kind::Archive => "archive",
            Kind::Font => "font",
            Kind::Json => "json",
            Kind::Csv => "csv",
            Kind::Markup => "markup",
            Kind::Code => "code",
            Kind::Text => "text",
            Kind::Sqlite => "sqlite",
            Kind::Binary => "binary",
        }
    }
}

/// Magic numbers at offset 0 and the kind each one names.
const MAGIC: &[(&[u8], Kind)] = &[
    (b"\xFF\xD8\xFF", Kind::Jpeg),
    (b"\x89PNG\r\n\x1A\n", Kind::Png),
    (b"GIF87a", Kind::Gif),
    (b"GIF89a", Kind::Gif),
    (b"%PDF-", Kind::Pdf),
    (b"\x1F\x8B", Kind::Gzip),
    (b"\x28\xB5\x2F\xFD", Kind::Archive), // zstd
    (b"\xFD7zXZ\x00", Kind::Archive),     // xz
    (b"7z\xBC\xAF\x27\x1C", Kind::Archive),
    (b"Rar!\x1A\x07", Kind::Archive),
    (b"\x04\x22\x4D\x18", Kind::Archive), // LZ4 frame
    (b"MSCF\x00\x00\x00\x00", Kind::Archive), // CAB
    (b"\x1A\x45\xDF\xA3", Kind::Video),   // Matroska, WebM
    (b"OggS", Kind::Audio),
    (b"fLaC", Kind::Audio),
    (b"ID3", Kind::Audio), // MP3 with an ID3v2 tag
    (b"\xFF\x0A", Kind::JpegXl),
    (b"\x00\x00\x00\x0CJXL \x0D\x0A\x87\x0A", Kind::JpegXl),
    (b"wOFF", Kind::Font),
    (b"wOF2", Kind::Font),
    (b"II*\x00", Kind::Tiff),
    (b"MM\x00*", Kind::Tiff),
    (b"IIRO", Kind::Tiff), // Olympus ORF
    (b"IIU\x00", Kind::Tiff), // Panasonic RW2
    (b"SQLite format 3\x00", Kind::Sqlite),
];

/// The kind of a file from `start` (its first bytes; [`SNIFF_LEN`] are enough) and, as a
/// tie-break, its name's `extension`.
#[must_use]
pub fn detect(start: &[u8], extension: Option<&str>) -> Kind {
    if let Some(kind) = by_magic(start) {
        return kind;
    }
    let by_extension = extension.and_then(kind_of_extension);
    match text_kind(start) {
        Some(kind) => match (kind, by_extension) {
            // Content says text; the extension may name which text.
            (Kind::Text, Some(ext)) if ext.is_text() => ext,
            (Kind::Text, None) if looks_like_code(start) => Kind::Code,
            _ => kind,
        },
        // Binary content. A text extension does not make it text, but a dense extension
        // (a raw AAC stream, a headerless format) saves the trial.
        None => match by_extension {
            Some(ext) if ext.is_dense() && ext != Kind::Jpeg => ext,
            _ => Kind::Binary,
        },
    }
}

fn by_magic(start: &[u8]) -> Option<Kind> {
    if let Some((_, kind)) = MAGIC.iter().find(|(magic, _)| start.starts_with(magic)) {
        return Some(*kind);
    }
    if start.starts_with(b"BZh") && start.get(3).is_some_and(|b| (b'1'..=b'9').contains(b)) {
        return Some(Kind::Archive);
    }
    if start.starts_with(b"PK\x03\x04") {
        return Some(zip_kind(start));
    }
    if start.starts_with(b"PK\x05\x06") || start.starts_with(b"PK\x07\x08") {
        return Some(Kind::Zip);
    }
    if start.len() >= 12 && &start[4..8] == b"ftyp" {
        return Some(iso_media_kind(&start[8..12]));
    }
    if start.len() >= 12 && start.starts_with(b"RIFF") {
        return match &start[8..12] {
            b"WEBP" => Some(Kind::WebP),
            b"AVI " => Some(Kind::Video),
            b"WAVE" => Some(Kind::Wav),
            _ => None,
        };
    }
    if start.len() >= 12 && start.starts_with(b"FORM") && matches!(&start[8..12], b"AIFF" | b"AIFC")
    {
        return Some(Kind::Wav);
    }
    // MPEG audio frames (MP3 without a tag) and AAC in ADTS: a frame sync and a layer / profile
    // that real files use (a looser test would call random binary data audio).
    if start.len() >= 4
        && start[0] == 0xFF
        && matches!(start[1], 0xFB | 0xFA | 0xF3 | 0xF2 | 0xE3 | 0xE2 | 0xF1 | 0xF9)
    {
        return Some(Kind::Audio);
    }
    // A PDF may have a few bytes of junk before its header (the format allows 1024).
    let head = &start[..start.len().min(1024)];
    if head.windows(5).any(|w| w == b"%PDF-") {
        return Some(Kind::Pdf);
    }
    None
}

/// The kind of an ISO media file from its major brand.
fn iso_media_kind(brand: &[u8]) -> Kind {
    match brand {
        b"heic" | b"heix" | b"hevc" | b"hevx" | b"heim" | b"heis" | b"mif1" | b"msf1"
        | b"avif" | b"avis" => Kind::Heif,
        b"M4A " | b"M4B " | b"M4P " | b"F4A " | b"F4B " => Kind::Audio,
        b"crx " => Kind::Tiff,
        _ => Kind::Video,
    }
}

/// The kind of a ZIP archive from its first member: Office files write `[Content_Types].xml`
/// (OOXML) or a stored `mimetype` member (OpenDocument, EPUB) first.
fn zip_kind(start: &[u8]) -> Kind {
    let Some(name_len) = start.get(26..28).map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
    else {
        return Kind::Zip;
    };
    let Some(extra_len) = start.get(28..30).map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
    else {
        return Kind::Zip;
    };
    let Some(name) = start.get(30..30 + name_len) else {
        return Kind::Zip;
    };
    if name == b"[Content_Types].xml" || name.starts_with(b"_rels/") || name.starts_with(b"docProps/") {
        return Kind::Office;
    }
    if name == b"mimetype" {
        let data_start = 30 + name_len + extra_len;
        let data = start.get(data_start..).unwrap_or(&[]);
        if data.starts_with(b"application/epub+zip") {
            return Kind::Epub;
        }
        if data.starts_with(b"application/vnd.oasis.opendocument") {
            return Kind::Office;
        }
    }
    Kind::Zip
}

/// The kind a file name's extension names, if it names one.
#[must_use]
pub fn kind_of_extension(extension: &str) -> Option<Kind> {
    let ext = extension.trim_start_matches('.').to_ascii_lowercase();
    let kind = match ext.as_str() {
        "jpg" | "jpeg" | "jpe" | "jfif" => Kind::Jpeg,
        "png" | "apng" => Kind::Png,
        "gif" => Kind::Gif,
        "webp" => Kind::WebP,
        "heic" | "heif" | "avif" => Kind::Heif,
        "jxl" => Kind::JpegXl,
        "tif" | "tiff" | "dng" | "cr2" | "cr3" | "nef" | "nrw" | "arw" | "orf" | "rw2" | "pef"
        | "srw" | "raf" => Kind::Tiff,
        "mp4" | "m4v" | "mov" | "3gp" | "mkv" | "webm" | "avi" | "wmv" | "flv" | "mts"
        | "m2ts" => Kind::Video,
        "mp3" | "m4a" | "aac" | "ogg" | "oga" | "opus" | "flac" | "wma" => Kind::Audio,
        "wav" | "aif" | "aiff" => Kind::Wav,
        "pdf" => Kind::Pdf,
        "zip" | "jar" | "apk" | "aar" | "whl" | "nupkg" | "xpi" | "ipa" => Kind::Zip,
        "docx" | "docm" | "dotx" | "xlsx" | "xlsm" | "xltx" | "pptx" | "pptm" | "potx"
        | "odt" | "ods" | "odp" | "odg" => Kind::Office,
        "epub" => Kind::Epub,
        "gz" | "tgz" => Kind::Gzip,
        "zst" | "xz" | "txz" | "bz2" | "tbz2" | "7z" | "rar" | "lz4" | "br" | "cab" | "lzma" => {
            Kind::Archive
        }
        "woff" | "woff2" => Kind::Font,
        "json" | "jsonl" | "ndjson" | "geojson" | "har" => Kind::Json,
        "csv" | "tsv" => Kind::Csv,
        "xml" | "html" | "htm" | "xhtml" | "svg" | "plist" | "xsd" | "xsl" | "xslt" | "rss"
        | "atom" | "kml" | "gpx" => Kind::Markup,
        "rs" | "py" | "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "c" | "h" | "cc" | "cpp"
        | "cxx" | "hpp" | "hh" | "m" | "mm" | "java" | "kt" | "kts" | "go" | "rb" | "php"
        | "swift" | "cs" | "fs" | "scala" | "lua" | "pl" | "pm" | "r" | "sh" | "bash" | "zsh"
        | "fish" | "ps1" | "sql" | "dart" | "zig" | "nim" | "hs" | "ml" | "ex" | "exs"
        | "erl" | "clj" | "el" | "vue" | "svelte" | "css" | "scss" | "less" | "gradle"
        | "cmake" => Kind::Code,
        "txt" | "md" | "markdown" | "rst" | "log" | "tex" | "bib" | "ini" | "toml" | "yaml"
        | "yml" | "cfg" | "conf" | "properties" | "env" | "srt" | "vtt" | "eml" | "mbox"
        | "ics" | "vcf" => Kind::Text,
        "sqlite" | "sqlite3" | "db" => Kind::Sqlite,
        _ => return None,
    };
    Some(kind)
}

/// The text kind of `start`, or `None` for binary content.
fn text_kind(start: &[u8]) -> Option<Kind> {
    if start.is_empty() {
        return None;
    }
    // UTF-16 with a byte order mark: text, though full of zero bytes.
    if start.starts_with(b"\xFF\xFE") || start.starts_with(b"\xFE\xFF") {
        return Some(Kind::Text);
    }
    let body = start.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(start);
    if body.contains(&0) {
        return None;
    }
    let utf8 = match std::str::from_utf8(body) {
        Ok(_) => true,
        // Only a character cut off by the end of the sniffed bytes.
        Err(e) => e.error_len().is_none(),
    };
    if !utf8 {
        // Latin-1 and similar 8-bit text: almost every byte printable.
        let printable = body
            .iter()
            .filter(|&&b| matches!(b, 0x09..=0x0D | 0x20..=0x7E | 0xA0..=0xFF))
            .count();
        return (printable * 100 >= body.len() * 95).then_some(Kind::Text);
    }
    let trimmed = body.trim_ascii_start();
    if trimmed.starts_with(b"{") || trimmed.starts_with(b"[") {
        return Some(Kind::Json);
    }
    if trimmed.starts_with(b"<") {
        let next = trimmed.get(1).copied().unwrap_or(0);
        if next == b'?' || next == b'!' || next.is_ascii_alphabetic() {
            return Some(Kind::Markup);
        }
    }
    if looks_like_csv(body) {
        return Some(Kind::Csv);
    }
    Some(Kind::Text)
}

/// Whether the lines of `text` look like a table: the first line has a delimiter, and at least
/// four of five complete lines after it have as many of it.
fn looks_like_csv(text: &[u8]) -> bool {
    // The last line may be cut by the end of the sniffed bytes: only complete lines count.
    let complete = match text.iter().rposition(|&b| b == b'\n') {
        Some(end) => &text[..end],
        None => return false,
    };
    let lines: Vec<&[u8]> = complete
        .split(|&b| b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() < 3 {
        return false;
    }
    [b',', b';', b'\t', b'|'].iter().any(|&delimiter| {
        let count = |line: &[u8]| line.iter().filter(|&&b| b == delimiter).count();
        let first = count(lines[0]);
        if first == 0 {
            return false;
        }
        let same = lines[1..].iter().filter(|line| count(line) == first).count();
        same * 5 >= (lines.len() - 1) * 4
    })
}

/// Whether plain text reads like source code: a shebang, or several lines that open the way
/// code does.
fn looks_like_code(text: &[u8]) -> bool {
    if text.starts_with(b"#!") {
        return true;
    }
    const OPENERS: &[&[u8]] = &[
        b"fn ", b"pub ", b"use ", b"impl ", b"struct ", b"enum ", b"mod ", b"def ", b"class ",
        b"import ", b"from ", b"#include", b"#define", b"function ", b"const ", b"let ", b"var ",
        b"package ", b"func ", b"return ", b"if (", b"for (", b"} else", b"#[", b"@",
    ];
    let lines = text
        .split(|&b| b == b'\n')
        .filter(|line| {
            let line = line.trim_ascii_start();
            OPENERS.iter().any(|opener| line.starts_with(opener))
        })
        .count();
    lines >= 3
}
