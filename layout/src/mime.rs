//! MIME types and file-name extensions, both ways.
//!
//! What another app is told a shared file IS (`ShareRequest`, the type the chooser filters its targets by), and what an
//! app's file dialog asks the system for (`*.md` -> `text/markdown`), come from this one table: a file the app gives no MIME
//! type for is described by its name's extension, and an extension the table does not know is `application/octet-stream`
//! (what the platform says of a file it cannot name), never a guess.
//!
//! The table is `static` data and the functions are pure: no platform is asked (a phone's `MimeTypeMap` knows less than this
//! table about the types the apps of this tree write: `.ics`, `.vcf`, `.gpx`, `.epub`, `.md`).

/// The type of a file the platform cannot name: a receiver offers every app that takes "any file".
pub const OCTET_STREAM: &str = "application/octet-stream";

/// `(extension without its dot, lower case; MIME type)`: the first row of a MIME type is the extension [`extension_for`]
/// answers for it.
static TABLE: &[(&str, &str)] = &[
    // text and documents
    ("txt", "text/plain"),
    ("text", "text/plain"),
    ("log", "text/plain"),
    ("md", "text/markdown"),
    ("markdown", "text/markdown"),
    ("html", "text/html"),
    ("htm", "text/html"),
    ("css", "text/css"),
    ("csv", "text/csv"),
    ("tsv", "text/tab-separated-values"),
    ("rtf", "application/rtf"),
    ("xml", "text/xml"),
    ("json", "application/json"),
    ("yaml", "application/yaml"),
    ("yml", "application/yaml"),
    ("toml", "application/toml"),
    ("pdf", "application/pdf"),
    ("epub", "application/epub+zip"),
    ("doc", "application/msword"),
    ("docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
    ("xls", "application/vnd.ms-excel"),
    ("xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
    ("ppt", "application/vnd.ms-powerpoint"),
    ("pptx", "application/vnd.openxmlformats-officedocument.presentationml.presentation"),
    ("odt", "application/vnd.oasis.opendocument.text"),
    ("ods", "application/vnd.oasis.opendocument.spreadsheet"),
    ("odp", "application/vnd.oasis.opendocument.presentation"),
    // calendar, contacts, mail, places
    ("ics", "text/calendar"),
    ("vcf", "text/vcard"),
    ("eml", "message/rfc822"),
    ("gpx", "application/gpx+xml"),
    ("kml", "application/vnd.google-earth.kml+xml"),
    ("geojson", "application/geo+json"),
    // archives
    ("zip", "application/zip"),
    ("gz", "application/gzip"),
    ("tar", "application/x-tar"),
    ("7z", "application/x-7z-compressed"),
    ("rar", "application/vnd.rar"),
    // images
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("jpe", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("avif", "image/avif"),
    ("heic", "image/heic"),
    ("heif", "image/heif"),
    ("bmp", "image/bmp"),
    ("tif", "image/tiff"),
    ("tiff", "image/tiff"),
    ("ico", "image/x-icon"),
    ("svg", "image/svg+xml"),
    // audio
    ("mp3", "audio/mpeg"),
    ("m4a", "audio/mp4"),
    ("aac", "audio/aac"),
    ("wav", "audio/wav"),
    ("ogg", "audio/ogg"),
    ("oga", "audio/ogg"),
    ("opus", "audio/opus"),
    ("flac", "audio/flac"),
    ("mid", "audio/midi"),
    ("midi", "audio/midi"),
    // video
    ("mp4", "video/mp4"),
    ("m4v", "video/mp4"),
    ("mov", "video/quicktime"),
    ("webm", "video/webm"),
    ("mkv", "video/x-matroska"),
    ("avi", "video/x-msvideo"),
    ("3gp", "video/3gpp"),
    ("mpeg", "video/mpeg"),
    ("mpg", "video/mpeg"),
    // source code (a receiver treats these as text)
    ("rs", "text/plain"),
    ("c", "text/plain"),
    ("h", "text/plain"),
    ("cpp", "text/plain"),
    ("py", "text/plain"),
    ("js", "text/javascript"),
    ("ts", "text/plain"),
    ("java", "text/plain"),
    ("kt", "text/plain"),
    ("go", "text/plain"),
    ("sh", "text/plain"),
    ("sql", "text/plain"),
    // fonts
    ("ttf", "font/ttf"),
    ("otf", "font/otf"),
    ("woff", "font/woff"),
    ("woff2", "font/woff2"),
    // the formats of the apps of this tree
    ("show", "application/vnd.azul.show"),
    ("azt", "application/vnd.azul.selftest"),
    ("db", "application/vnd.sqlite3"),
    ("sqlite", "application/vnd.sqlite3"),
    ("bin", "application/octet-stream"),
    ("dat", "application/octet-stream"),
];

/// The extension of `name`, lower case and without its dot, or `None` for a name that has none (`Makefile`, `.profile`,
/// `trailing.`). Only the last dot counts: `archive.tar.gz` is `gz`.
#[must_use]
pub fn extension_of(name: &str) -> Option<String> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let (stem, extension) = base.rsplit_once('.')?;
    if stem.is_empty() || extension.is_empty() || extension.len() > 16 {
        return None;
    }
    if !extension.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(extension.to_ascii_lowercase())
}

/// The MIME type of files with the extension `extension` (with or without its dot, any case), or `None` when the table
/// does not know it.
#[must_use]
pub fn for_extension(extension: &str) -> Option<&'static str> {
    let wanted = extension.trim_start_matches(['*', '.']).to_ascii_lowercase();
    TABLE
        .iter()
        .find(|(known, _)| *known == wanted)
        .map(|(_, mime)| *mime)
}

/// The MIME type of a file called `name`: by its extension, else [`OCTET_STREAM`].
#[must_use]
pub fn for_file_name(name: &str) -> &'static str {
    extension_of(name)
        .as_deref()
        .and_then(for_extension)
        .unwrap_or(OCTET_STREAM)
}

/// The extension (without its dot) a file of the MIME type `mime` is usually given, or `None` for a type the table does not
/// know. Parameters (`; charset=utf-8`) and case are ignored.
#[must_use]
pub fn extension_for(mime: &str) -> Option<&'static str> {
    let wanted = base_type(mime);
    TABLE
        .iter()
        .find(|(_, known)| known.eq_ignore_ascii_case(&wanted))
        .map(|(extension, _)| *extension)
}

/// `mime` without its parameters and in lower case: `Text/Plain; charset=UTF-8` is `text/plain`.
#[must_use]
pub fn base_type(mime: &str) -> String {
    mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase()
}

/// Whether `text` is a MIME type of the form `type/subtype` that a platform accepts in a share intent or a manifest: two
/// tokens of letters, digits and `!#$&-^_.+`, one slash, nothing else (a wildcard `*/*` and `image/*` are accepted: the
/// filters of an intent use them).
#[must_use]
pub fn is_valid(text: &str) -> bool {
    let Some((kind, subtype)) = text.split_once('/') else {
        return false;
    };
    let token = |t: &str| {
        !t.is_empty()
            && t.len() <= 127
            && (t == "*"
                || t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "!#$&-^_.+".contains(c)))
    };
    token(kind) && token(subtype) && !(kind == "*" && subtype != "*")
}

/// The one type that describes all of `types`, the way a share intent of several files needs it: the same type when they
/// agree (`image/png`), the family when only the subtype differs (`image/*`), `*/*` when the families differ or the list is
/// empty. An invalid entry counts as [`OCTET_STREAM`].
#[must_use]
pub fn common_type(types: &[&str]) -> String {
    let mut common: Option<String> = None;
    for raw in types {
        let base = base_type(raw);
        let base = if is_valid(&base) { base } else { OCTET_STREAM.to_string() };
        common = Some(match common {
            None => base,
            Some(previous) if previous == base => previous,
            Some(previous) => {
                let (previous_kind, _) = previous.split_once('/').unwrap_or(("*", "*"));
                let (kind, _) = base.split_once('/').unwrap_or(("*", "*"));
                if previous_kind == kind && kind != "*" {
                    format!("{kind}/*")
                } else {
                    "*/*".to_string()
                }
            }
        });
    }
    common.unwrap_or_else(|| "*/*".to_string())
}

#[cfg(test)]
#[path = "mime_test.rs"]
mod mime_test;
