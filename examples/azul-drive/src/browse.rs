//! What AzDrive's window shows, as plain data: the rows of a folder and their
//! order, sizes and dates as text, the way back and up, the breadcrumb, and the
//! "Add drive" form. No azul types here, so all of it is tested without a window.

use std::{cmp::Ordering, path::Path};

use azul_storage::{
    config::{DriveAuth, DriveEntry, DriveLocation},
    key, sigv4, Credentials, HttpCall, HttpReply, ListPage, S3Config, S3Drive, Transport,
};
use serde::{Deserialize, Serialize};

/// A column of the Details layout: Explorer's four, and the two "Add
/// columns" offers (the key, an S3 object's ETag).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Column {
    Name,
    Modified,
    Type,
    Size,
    Path,
    Tag,
}

impl Column {
    pub const ALL: [Column; 6] = [
        Column::Name,
        Column::Modified,
        Column::Type,
        Column::Size,
        Column::Path,
        Column::Tag,
    ];

    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Column::Name => 0,
            Column::Modified => 1,
            Column::Type => 2,
            Column::Size => 3,
            Column::Path => 4,
            Column::Tag => 5,
        }
    }

    #[must_use]
    pub fn from_index(index: usize) -> Option<Column> {
        Column::ALL.get(index).copied()
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Column::Name => "Name",
            Column::Modified => "Date modified",
            Column::Type => "Type",
            Column::Size => "Size",
            Column::Path => "Folder path",
            Column::Tag => "ETag",
        }
    }
}

/// The order of the list: a column, up or down. Folders always come first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sort {
    pub column: Column,
    pub descending: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Sort {
            column: Column::Name,
            descending: false,
        }
    }
}

impl Sort {
    /// A click on a column header: the same column turns the order around,
    /// another column sorts by it, ascending.
    #[must_use]
    pub fn clicked(self, column: Column) -> Sort {
        Sort {
            column,
            descending: column == self.column && !self.descending,
        }
    }
}

/// One row: a folder or a file of the open folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The file's key, or the folder's prefix (ending in `/`).
    pub key: String,
    /// The last segment: `0001.eml`, `inbox`.
    pub name: String,
    pub is_folder: bool,
    /// Bytes; `None` for a folder.
    pub size: Option<u64>,
    /// Seconds since 1970; `None` when unknown (and for folders).
    pub modified: Option<u64>,
    /// An S3 object's entity tag, when the listing has one.
    pub etag: Option<String>,
}

impl Entry {
    /// What the Name column shows: `inbox/` for a folder, the name for a file.
    #[must_use]
    pub fn label(&self) -> String {
        if self.is_folder {
            format!("{}/", self.name)
        } else {
            self.name.clone()
        }
    }

    /// The name as Explorer shows it: without the extension when
    /// "File name extensions" is off (a folder keeps its whole name).
    #[must_use]
    pub fn display_name(&self, show_extensions: bool) -> String {
        if show_extensions || self.is_folder {
            return self.name.clone();
        }
        match extension_of(&self.name) {
            Some(ext) => self.name[..self.name.len() - ext.len() - 1].to_string(),
            None => self.name.clone(),
        }
    }

    /// The Type column: "File folder", "Text Document", "RS File".
    #[must_use]
    pub fn kind(&self) -> String {
        kind_of(&self.name, self.is_folder)
    }

    /// A hidden item: its name starts with a dot (the trash folder too).
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.name.starts_with('.')
    }
}

/// The extension of a file name, without its dot and without case folding:
/// `photo.JPG` -> `JPG`; `.env` and `README` have none.
#[must_use]
pub fn extension_of(name: &str) -> Option<&str> {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => Some(&name[i + 1..]),
        _ => None,
    }
}

/// What Explorer's Type column says for a name.
#[must_use]
pub fn kind_of(name: &str, is_folder: bool) -> String {
    if is_folder {
        return String::from("File folder");
    }
    let Some(ext) = extension_of(name) else {
        return String::from("File");
    };
    let known = match ext.to_ascii_lowercase().as_str() {
        "txt" => "Text Document",
        "md" => "Markdown File",
        "pdf" => "PDF Document",
        "jpg" | "jpeg" => "JPEG image",
        "png" => "PNG image",
        "gif" => "GIF image",
        "bmp" => "BMP image",
        "webp" => "WEBP image",
        "svg" => "SVG Document",
        "mp4" | "m4v" | "mov" => "Video",
        "mp3" | "wav" | "flac" | "ogg" | "m4a" => "Audio",
        "zip" => "Compressed (zipped) Folder",
        "html" | "htm" => "HTML Document",
        "json" => "JSON File",
        "csv" => "CSV File",
        "eml" => "E-mail Message",
        "ics" => "iCalendar File",
        "docx" => "Word Document",
        "xlsx" => "Excel Worksheet",
        "pptx" => "PowerPoint Presentation",
        _ => "",
    };
    if known.is_empty() {
        format!("{} File", ext.to_ascii_uppercase())
    } else {
        known.to_string()
    }
}

/// The rows of one listing page of the folder `prefix`: its folders, then its
/// files, named relative to the folder.
#[must_use]
pub fn entries_of(page: &ListPage, prefix: &str) -> Vec<Entry> {
    let folders = page.folders.iter().filter_map(|folder| {
        let name = key::last_segment(folder);
        (!name.is_empty()).then(|| Entry {
            key: folder.clone(),
            name: name.to_string(),
            is_folder: true,
            size: None,
            modified: None,
            etag: None,
        })
    });
    // A key ending in `/` is a "folder marker" object some tools write: the folder
    // itself, or one already listed as a common prefix.
    let files = page
        .objects
        .iter()
        .filter(|object| object.key != prefix && !object.key.ends_with('/'))
        .map(|object| Entry {
            key: object.key.clone(),
            name: object.name().to_string(),
            is_folder: false,
            size: Some(object.size),
            modified: object.modified,
            etag: object.etag.clone(),
        });
    folders.chain(files).collect()
}

/// Names compare without case first, then with it (so the order is total).
fn compare_names(a: &Entry, b: &Entry) -> Ordering {
    a.name
        .to_lowercase()
        .cmp(&b.name.to_lowercase())
        .then_with(|| a.name.cmp(&b.name))
}

/// Puts the rows in `sort` order, folders first; ties go by name.
pub fn sort_entries(entries: &mut [Entry], sort: Sort) {
    entries.sort_by(|a, b| {
        b.is_folder
            .cmp(&a.is_folder)
            .then_with(|| {
                let by_column = match sort.column {
                    Column::Name => compare_names(a, b),
                    Column::Size => a.size.cmp(&b.size),
                    Column::Modified => a.modified.cmp(&b.modified),
                    Column::Type => a.kind().cmp(&b.kind()),
                    Column::Path => a.key.cmp(&b.key),
                    Column::Tag => a.etag.cmp(&b.etag),
                };
                if sort.descending {
                    by_column.reverse()
                } else {
                    by_column
                }
            })
            .then_with(|| compare_names(a, b))
    });
}

/// `0 B`, `999 B`, `1.5 KB`, `5.0 MB`, `324 GB` (azul's
/// `DiskSpace::format_bytes`: 1024-based, as Explorer shows); empty for a
/// folder.
#[must_use]
pub fn format_size(bytes: Option<u64>) -> String {
    bytes.map_or_else(String::new, |b| azul::file::DiskSpace::format_bytes(b).to_string())
}

/// `2009-10-12 17:50` in `zone`; empty when unknown.
#[must_use]
pub fn format_modified<Tz: chrono::TimeZone>(unix: Option<u64>, zone: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    unix.and_then(|secs| i64::try_from(secs).ok())
        .and_then(|secs| zone.timestamp_opt(secs, 0).single())
        .map(|time| time.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

/// Where the window is: Quick access (the pinned folders), the "This PC"
/// overview of the drives, or a folder of one drive (`prefix` `""` = its
/// root).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Place {
    QuickAccess,
    ThisPc,
    Folder { drive: String, prefix: String },
}

impl Place {
    #[must_use]
    pub fn folder(drive: &str, prefix: &str) -> Place {
        Place::Folder {
            drive: drive.to_string(),
            prefix: prefix.to_string(),
        }
    }
}

/// The places visited, as a browser keeps them: Back walks the trail, Forward
/// returns along it until a new place is visited.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    back: Vec<Place>,
    forward: Vec<Place>,
}

impl History {
    /// Leaving `from` for a new place: Forward has nothing to return to.
    pub fn visit(&mut self, from: Place) {
        const KEEP: usize = 100;
        self.back.push(from);
        if self.back.len() > KEEP {
            self.back.remove(0);
        }
        self.forward.clear();
    }

    /// Back from `current`: the place before it, `current` kept for Forward.
    pub fn back(&mut self, current: Place) -> Option<Place> {
        let place = self.back.pop()?;
        self.forward.push(current);
        Some(place)
    }

    /// Forward from `current`: where Back came from, `current` kept for Back.
    pub fn forward(&mut self, current: Place) -> Option<Place> {
        let place = self.forward.pop()?;
        self.back.push(current);
        Some(place)
    }

    #[must_use]
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    #[must_use]
    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Nothing to go back or forward to.
    pub fn clear(&mut self) {
        self.back.clear();
        self.forward.clear();
    }
}

/// Whether `entry` stays in a listing filtered by `search` (a case-insensitive
/// part of the name; an empty search keeps everything).
#[must_use]
pub fn matches_search(entry: &Entry, search: &str) -> bool {
    let needle = search.trim().to_lowercase();
    needle.is_empty() || entry.name.to_lowercase().contains(&needle)
}

/// The overview of the drives, as the address bar names it.
pub const THIS_PC: &str = "This PC";

/// The pinned folders, as the address bar names them.
pub const QUICK_ACCESS: &str = "Quick access";

/// The address bar's editable text for `place`: `This PC`, `Home`,
/// `Home/mail/inbox`.
#[must_use]
pub fn path_text(place: &Place, drive_name: Option<&str>) -> String {
    match place {
        Place::QuickAccess => QUICK_ACCESS.to_string(),
        Place::ThisPc => THIS_PC.to_string(),
        Place::Folder { drive, prefix } => {
            let name = drive_name.unwrap_or(drive);
            let folder = prefix.trim_end_matches('/');
            if folder.is_empty() {
                name.to_string()
            } else {
                format!("{name}/{folder}")
            }
        }
    }
}

/// The place a typed path names: `This PC`; `Home`, `Home/mail`,
/// `This PC/Home/mail/` or with backslashes - the drive by name, without case;
/// `None` for an unknown drive. `drives` are `(id, name)`.
#[must_use]
pub fn parse_path(text: &str, drives: &[(String, String)]) -> Option<Place> {
    if text.trim().eq_ignore_ascii_case(QUICK_ACCESS) {
        return Some(Place::QuickAccess);
    }
    let text = text.trim().replace('\\', "/");
    let mut parts = text
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .peekable();
    if parts.peek().is_some_and(|part| part.eq_ignore_ascii_case(THIS_PC)) {
        parts.next();
    }
    let Some(name) = parts.next() else {
        return text
            .trim()
            .eq_ignore_ascii_case(THIS_PC)
            .then_some(Place::ThisPc);
    };
    let (id, _) = drives
        .iter()
        .find(|(_, drive_name)| drive_name.eq_ignore_ascii_case(name))?;
    let mut prefix = String::new();
    for part in parts {
        prefix.push_str(part);
        prefix.push('/');
    }
    Some(Place::folder(id, &prefix))
}

/// The address bar's trail for `place`, each crumb with the place it goes
/// to: `This PC`; then the drive (its root) and every folder down to the
/// open one.
#[must_use]
pub fn crumbs_of(place: &Place, drive_name: &str) -> Vec<(String, Place)> {
    if *place == Place::QuickAccess {
        return vec![(QUICK_ACCESS.to_string(), Place::QuickAccess)];
    }
    let mut trail = vec![(THIS_PC.to_string(), Place::ThisPc)];
    if let Place::Folder { drive, prefix } = place {
        trail.push((drive_name.to_string(), Place::folder(drive, "")));
        for (label, folder) in key::folder_trail(prefix) {
            trail.push((label, Place::folder(drive, &folder)));
        }
    }
    trail
}

/// `n` things as Explorer counts them: "1 item", "3 items", "0 items".
#[must_use]
pub fn counted(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// A `file://` URL of a local path, percent-encoded, for the OS to open.
#[must_use]
pub fn file_url(path: &Path) -> String {
    #[cfg(windows)]
    let text = path.to_string_lossy().replace('\\', "/");
    #[cfg(not(windows))]
    let text = path.to_string_lossy().into_owned();
    // `C:/Users/..`: the drive letter and its colon stay as they are.
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return format!(
            "file:///{}{}",
            &text[..2],
            sigv4::uri_encode(&text[2..], false)
        );
    }
    let absolute = if text.starts_with('/') {
        text
    } else {
        format!("/{text}")
    };
    format!("file://{}", sigv4::uri_encode(&absolute, false))
}

/// The region an empty "Region" field means.
pub const DEFAULT_REGION: &str = "us-east-1";

/// The "Add drive" form. `Debug` never shows the keys.
#[derive(Clone, PartialEq, Eq)]
pub struct DriveForm {
    pub name: String,
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub path_style: bool,
}

impl Default for DriveForm {
    fn default() -> Self {
        DriveForm {
            name: String::new(),
            endpoint: String::new(),
            region: String::new(),
            bucket: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            path_style: true,
        }
    }
}

impl std::fmt::Debug for DriveForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DriveForm")
            .field("name", &self.name)
            .field("endpoint", &self.endpoint)
            .field("region", &self.region)
            .field("bucket", &self.bucket)
            .field("access_key", &"<hidden>")
            .field("secret_key", &"<hidden>")
            .field("path_style", &self.path_style)
            .finish()
    }
}

/// A transport that sends nothing: `S3Drive::new` checks the endpoint and the
/// bucket name without one.
struct NoTransport;

impl Transport for NoTransport {
    fn send(&self, _call: &HttpCall) -> Result<HttpReply, String> {
        Err(String::from("not sent"))
    }
}

impl DriveForm {
    /// The bucket settings and keys of the form, or what to fix, as a sentence.
    pub fn check(&self) -> Result<(S3Config, Credentials), String> {
        let endpoint = self.endpoint.trim();
        let lower = endpoint.to_ascii_lowercase();
        if self.name.trim().is_empty() {
            return Err(String::from("Give the drive a name."));
        }
        if endpoint.is_empty() {
            return Err(String::from(
                "Enter the endpoint, for example https://s3.eu-central-1.amazonaws.com.",
            ));
        }
        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            return Err(String::from(
                "The endpoint must start with http:// or https://.",
            ));
        }
        if self.bucket.trim().is_empty() {
            return Err(String::from("Enter the bucket name."));
        }
        if self.access_key.trim().is_empty() {
            return Err(String::from("Enter the access key."));
        }
        if self.secret_key.trim().is_empty() {
            return Err(String::from("Enter the secret key."));
        }
        let region = match self.region.trim() {
            "" => DEFAULT_REGION,
            region => region,
        };
        let config = S3Config {
            endpoint: endpoint.to_string(),
            region: region.to_string(),
            bucket: self.bucket.trim().to_string(),
            path_style: self.path_style,
        };
        let credentials = Credentials::new(self.access_key.trim(), self.secret_key.trim());
        // The same checks the drive makes when it opens: a readable endpoint and a
        // bucket name that fits in the URL.
        S3Drive::new(config.clone(), credentials.clone(), Box::new(NoTransport))
            .map_err(|e| format!("{e}."))?;
        Ok((config, credentials))
    }

    /// The drives-file entry (without the keys, which go to the keyring).
    pub fn entry(&self, id: &str) -> Result<DriveEntry, String> {
        let (config, _) = self.check()?;
        Ok(DriveEntry {
            id: id.to_string(),
            name: self.name.trim().to_string(),
            location: DriveLocation::S3 {
                endpoint: config.endpoint,
                region: config.region,
                bucket: config.bucket,
                path_style: config.path_style,
                auth: DriveAuth::Keyring,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use azul_storage::ObjectInfo;
    use chrono::{FixedOffset, Utc};

    use super::*;

    fn object(key: &str, size: u64, modified: u64) -> ObjectInfo {
        ObjectInfo {
            key: key.to_string(),
            size,
            modified: Some(modified),
            etag: None,
        }
    }

    fn file(name: &str, size: u64, modified: u64) -> Entry {
        Entry {
            key: format!("f/{name}"),
            name: name.to_string(),
            is_folder: false,
            size: Some(size),
            modified: Some(modified),
            etag: None,
        }
    }

    fn folder(name: &str) -> Entry {
        Entry {
            key: format!("f/{name}/"),
            name: name.to_string(),
            is_folder: true,
            size: None,
            modified: None,
            etag: None,
        }
    }

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn a_listing_page_becomes_folders_then_files_named_relative_to_the_folder() {
        let page = ListPage {
            folders: vec!["mail/inbox/".to_string(), "mail/sent/".to_string()],
            objects: vec![object("mail/readme.txt", 5, 100)],
            next: None,
        };
        let entries = entries_of(&page, "mail/");
        assert_eq!(names(&entries), vec!["inbox", "sent", "readme.txt"]);
        assert!(entries[0].is_folder && !entries[2].is_folder);
        assert_eq!(entries[0].key, "mail/inbox/");
        assert_eq!(entries[0].label(), "inbox/");
        assert_eq!(entries[2].key, "mail/readme.txt");
        assert_eq!(entries[2].label(), "readme.txt");
        assert_eq!(entries[2].size, Some(5));
    }

    #[test]
    fn folders_stay_first_whatever_the_sort() {
        let mut entries = vec![file("a.txt", 9, 9), folder("zeta"), file("b.txt", 1, 1)];
        for sort in [
            Sort::default(),
            Sort::default().clicked(Column::Name),
            Sort::default().clicked(Column::Size),
            Sort::default()
                .clicked(Column::Modified)
                .clicked(Column::Modified),
        ] {
            sort_entries(&mut entries, sort);
            assert_eq!(entries[0].name, "zeta", "{sort:?}");
        }
    }

    #[test]
    fn sorting_by_name_ignores_case_and_a_second_click_reverses() {
        let mut entries = vec![
            file("b.txt", 1, 1),
            file("C.txt", 1, 1),
            file("a.txt", 1, 1),
        ];
        sort_entries(&mut entries, Sort::default());
        assert_eq!(names(&entries), vec!["a.txt", "b.txt", "C.txt"]);
        let reversed = Sort::default().clicked(Column::Name);
        assert!(reversed.descending);
        sort_entries(&mut entries, reversed);
        assert_eq!(names(&entries), vec!["C.txt", "b.txt", "a.txt"]);
    }

    #[test]
    fn sorting_by_size_or_date_breaks_ties_by_name() {
        let mut entries = vec![
            file("big.bin", 300, 1),
            file("b.txt", 10, 3),
            file("a.txt", 10, 2),
        ];
        let by_size = Sort::default().clicked(Column::Size);
        assert_eq!(
            by_size,
            Sort {
                column: Column::Size,
                descending: false
            }
        );
        sort_entries(&mut entries, by_size);
        assert_eq!(names(&entries), vec!["a.txt", "b.txt", "big.bin"]);
        sort_entries(&mut entries, by_size.clicked(Column::Size));
        assert_eq!(names(&entries), vec!["big.bin", "a.txt", "b.txt"]);
        sort_entries(
            &mut entries,
            Sort {
                column: Column::Modified,
                descending: false,
            },
        );
        assert_eq!(names(&entries), vec!["big.bin", "a.txt", "b.txt"]);
    }

    #[test]
    fn sizes_read_like_a_file_manager() {
        assert_eq!(format_size(None), "");
        assert_eq!(format_size(Some(0)), "0 B");
        assert_eq!(format_size(Some(999)), "999 B");
        assert_eq!(format_size(Some(1024)), "1.0 KB");
        assert_eq!(format_size(Some(1536)), "1.5 KB");
        assert_eq!(format_size(Some(5 * 1024 * 1024)), "5.0 MB");
        assert_eq!(format_size(Some(3 * 1024 * 1024 * 1024)), "3.0 GB");
        assert_eq!(format_size(Some(324 * 1024 * 1024 * 1024)), "324 GB", "no decimal from ten up");
    }

    #[test]
    fn dates_show_in_the_given_time_zone() {
        assert_eq!(
            format_modified(Some(1_255_369_830), &Utc),
            "2009-10-12 17:50"
        );
        let berlin_summer = FixedOffset::east_opt(2 * 3600).unwrap();
        assert_eq!(
            format_modified(Some(1_255_369_830), &berlin_summer),
            "2009-10-12 19:50"
        );
        assert_eq!(format_modified(None, &Utc), "");
    }

    #[test]
    fn back_returns_to_the_places_visited_before_and_forward_returns_along_them() {
        let root = Place::folder("home", "");
        let mail = Place::folder("home", "mail/");
        let inbox = Place::folder("home", "mail/inbox/");
        let mut history = History::default();
        assert!(!history.can_go_back() && !history.can_go_forward());
        // This PC -> Home -> mail -> inbox
        history.visit(Place::ThisPc);
        history.visit(root.clone());
        history.visit(mail.clone());
        assert!(history.can_go_back());
        assert_eq!(history.back(inbox.clone()), Some(mail.clone()));
        assert!(history.can_go_forward(), "inbox waits ahead");
        assert_eq!(history.back(mail.clone()), Some(root.clone()));
        assert_eq!(history.forward(root.clone()), Some(mail.clone()));
        assert_eq!(history.forward(mail.clone()), Some(inbox.clone()));
        assert_eq!(history.forward(inbox.clone()), None, "nothing ahead");
        assert_eq!(history.back(inbox.clone()), Some(mail.clone()));
        // A new visit from mail drops what was ahead.
        history.visit(mail.clone());
        assert!(!history.can_go_forward());
        assert_eq!(history.back(Place::folder("home", "docs/")), Some(mail));
        assert_eq!(history.back(root.clone()), Some(root.clone()));
        assert_eq!(history.back(root), Some(Place::ThisPc));
        assert_eq!(history.back(Place::ThisPc), None);
        history.clear();
        assert!(!history.can_go_back() && !history.can_go_forward());
    }

    #[test]
    fn the_search_keeps_the_names_that_contain_it_without_case() {
        assert!(matches_search(&file("Report.PDF", 1, 0), ""));
        assert!(matches_search(&file("Report.PDF", 1, 0), "pdf"));
        assert!(matches_search(&folder("Mail"), "MA"));
        assert!(!matches_search(&file("notes.txt", 1, 0), "pdf"));
        assert!(matches_search(&file("a b", 1, 0), " b "), "the search is trimmed");
    }

    #[test]
    fn the_path_text_names_the_place_and_a_typed_path_finds_it_again() {
        let drives = vec![
            ("home".to_string(), "Home".to_string()),
            ("s3-1".to_string(), "S3 Drive".to_string()),
        ];
        assert_eq!(path_text(&Place::ThisPc, None), "This PC");
        assert_eq!(path_text(&Place::folder("home", ""), Some("Home")), "Home");
        assert_eq!(
            path_text(&Place::folder("home", "mail/inbox/"), Some("Home")),
            "Home/mail/inbox"
        );
        assert_eq!(parse_path("This PC", &drives), Some(Place::ThisPc));
        assert_eq!(parse_path("  this pc ", &drives), Some(Place::ThisPc));
        assert_eq!(parse_path("Home", &drives), Some(Place::folder("home", "")));
        assert_eq!(
            parse_path("home/mail/inbox", &drives),
            Some(Place::folder("home", "mail/inbox/"))
        );
        assert_eq!(
            parse_path("This PC\\S3 Drive\\mail\\", &drives),
            Some(Place::folder("s3-1", "mail/"))
        );
        assert_eq!(parse_path("Photos/2024", &drives), None, "an unknown drive");
        assert_eq!(parse_path("", &drives), None);
    }

    #[test]
    fn the_trail_starts_at_this_pc_then_the_drive_then_the_folders() {
        assert_eq!(
            crumbs_of(&Place::ThisPc, ""),
            vec![("This PC".to_string(), Place::ThisPc)]
        );
        assert_eq!(
            crumbs_of(&Place::folder("s3-1", "mail/inbox/"), "S3 Drive"),
            vec![
                ("This PC".to_string(), Place::ThisPc),
                ("S3 Drive".to_string(), Place::folder("s3-1", "")),
                ("mail".to_string(), Place::folder("s3-1", "mail/")),
                ("inbox".to_string(), Place::folder("s3-1", "mail/inbox/")),
            ]
        );
    }

    #[test]
    fn one_thing_is_counted_in_the_singular_and_every_other_number_in_the_plural() {
        assert_eq!(counted(1, "drive", "drives"), "1 drive");
        assert_eq!(counted(2, "drive", "drives"), "2 drives");
        assert_eq!(counted(0, "item", "items"), "0 items");
        assert_eq!(counted(1, "item selected", "items selected"), "1 item selected");
    }

    #[cfg(not(windows))]
    #[test]
    fn a_file_url_percent_encodes_the_path() {
        assert_eq!(
            file_url(Path::new("/tmp/a b/\u{fc}.txt")),
            "file:///tmp/a%20b/%C3%BC.txt"
        );
        assert_eq!(file_url(Path::new("/tmp/x#1.pdf")), "file:///tmp/x%231.pdf");
    }

    #[cfg(windows)]
    #[test]
    fn a_file_url_percent_encodes_the_path() {
        assert_eq!(
            file_url(Path::new("C:\\Users\\a b\\x.pdf")),
            "file:///C:/Users/a%20b/x.pdf"
        );
    }

    fn filled() -> DriveForm {
        DriveForm {
            name: "S3 Drive".to_string(),
            endpoint: "http://127.0.0.1:9000".to_string(),
            region: "us-east-1".to_string(),
            bucket: "azdrive".to_string(),
            access_key: "AKIDTEST".to_string(),
            secret_key: "test-secret".to_string(),
            path_style: true,
        }
    }

    #[test]
    fn the_drive_form_names_what_is_missing() {
        let with = |f: fn(&mut DriveForm)| {
            let mut form = filled();
            f(&mut form);
            form.check().unwrap_err()
        };
        assert!(with(|f| f.name.clear()).contains("name"));
        assert!(with(|f| f.endpoint.clear()).contains("endpoint"));
        assert!(with(|f| f.endpoint = "s3.amazonaws.com".to_string()).contains("http"));
        assert!(with(|f| f.bucket.clear()).contains("bucket"));
        assert!(with(|f| f.access_key.clear()).contains("access key"));
        assert!(with(|f| f.secret_key.clear()).contains("secret key"));
        assert!(filled().check().is_ok());
    }

    #[test]
    fn an_empty_region_means_us_east_1() {
        let mut form = filled();
        form.region = "  ".to_string();
        let (config, _) = form.check().unwrap();
        assert_eq!(config.region, DEFAULT_REGION);
    }

    #[test]
    fn the_drive_form_makes_a_keyring_drive_without_secrets() {
        let (config, credentials) = filled().check().unwrap();
        assert_eq!(config.endpoint, "http://127.0.0.1:9000");
        assert_eq!(config.bucket, "azdrive");
        assert!(config.path_style);
        assert_eq!(credentials, Credentials::new("AKIDTEST", "test-secret"));
        let entry = filled().entry("s3-drive-1").unwrap();
        assert_eq!(entry.id, "s3-drive-1");
        assert_eq!(entry.name, "S3 Drive");
        match &entry.location {
            DriveLocation::S3 { auth, bucket, .. } => {
                assert_eq!(auth, &DriveAuth::Keyring);
                assert_eq!(bucket, "azdrive");
            }
            other => panic!("not an S3 drive: {other:?}"),
        }
        let text = format!("{entry:?}");
        assert!(
            !text.contains("test-secret") && !text.contains("AKIDTEST"),
            "{text}"
        );
    }

    #[test]
    fn the_drive_form_never_shows_the_keys_in_debug_output() {
        let text = format!("{:?}", filled());
        assert!(!text.contains("test-secret"), "{text}");
        assert!(!text.contains("AKIDTEST"), "{text}");
        assert!(text.contains("azdrive"), "{text}");
    }

    #[test]
    fn quick_access_is_a_place_of_its_own_in_the_trail_and_the_typed_path() {
        let drives = vec![("home".to_string(), "Home".to_string())];
        assert_eq!(path_text(&Place::QuickAccess, None), "Quick access");
        assert_eq!(parse_path("quick access", &drives), Some(Place::QuickAccess));
        assert_eq!(
            crumbs_of(&Place::QuickAccess, ""),
            vec![("Quick access".to_string(), Place::QuickAccess)]
        );
    }

    #[test]
    fn the_type_column_names_the_kind_of_file_as_explorer_does() {
        assert_eq!(kind_of("inbox", true), "File folder");
        assert_eq!(kind_of("notes.txt", false), "Text Document");
        assert_eq!(kind_of("photo.JPG", false), "JPEG image");
        assert_eq!(kind_of("paper.pdf", false), "PDF Document");
        assert_eq!(kind_of("main.rs", false), "RS File");
        assert_eq!(kind_of("README", false), "File");
        assert_eq!(kind_of(".bashrc", false), "File", "a leading dot is not an extension");
        assert_eq!(file("a.png", 1, 0).kind(), "PNG image");
    }

    #[test]
    fn sorting_by_type_groups_the_kinds_then_names() {
        let mut entries = vec![
            file("b.txt", 1, 1),
            file("a.png", 1, 1),
            folder("z"),
            file("a.txt", 1, 1),
        ];
        sort_entries(
            &mut entries,
            Sort {
                column: Column::Type,
                descending: false,
            },
        );
        assert_eq!(names(&entries), vec!["z", "a.png", "a.txt", "b.txt"]);
    }

    #[test]
    fn extensions_hide_when_asked_and_dot_files_are_hidden_items() {
        let f = file("report.final.pdf", 1, 0);
        assert_eq!(f.display_name(true), "report.final.pdf");
        assert_eq!(f.display_name(false), "report.final");
        assert_eq!(folder("v1.2").display_name(false), "v1.2", "a folder has no extension");
        assert_eq!(file(".env", 1, 0).display_name(false), ".env");
        assert!(file(".env", 1, 0).is_hidden());
        assert!(folder(".azdrive-trash").is_hidden());
        assert!(!file("env", 1, 0).is_hidden());
    }

    #[test]
    fn every_column_has_a_header_and_reads_back_from_its_index() {
        for (i, column) in Column::ALL.iter().enumerate() {
            assert_eq!(column.index(), i);
            assert_eq!(Column::from_index(i), Some(*column));
            assert!(!column.label().is_empty());
        }
        assert_eq!(Column::Modified.label(), "Date modified");
        assert_eq!(Column::from_index(99), None);
    }
}
