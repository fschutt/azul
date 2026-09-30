//! What AzDrive's window shows, as plain data: the rows of a folder and their
//! order, sizes and dates as text, the way back and up, the breadcrumb, and the
//! "Add drive" form. No azul types here, so all of it is tested without a window.

use std::{cmp::Ordering, path::Path};

use azul_storage::{
    config::{DriveAuth, DriveEntry, DriveLocation},
    key, sigv4, Credentials, HttpCall, HttpReply, ListPage, S3Config, S3Drive, Transport,
};

/// A column of the file list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Name,
    Size,
    Modified,
}

impl Column {
    pub const ALL: [Column; 3] = [Column::Name, Column::Size, Column::Modified];

    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Column::Name => 0,
            Column::Size => 1,
            Column::Modified => 2,
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
            Column::Size => "Size",
            Column::Modified => "Modified",
        }
    }
}

/// The order of the list: a column, up or down. Folders always come first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// `0 B`, `999 B`, `1.5 KB`, `5.0 MB` (1024-based, as file managers show);
/// empty for a folder.
#[must_use]
pub fn format_size(bytes: Option<u64>) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    match bytes {
        None => String::new(),
        Some(bytes) if bytes < 1024 => format!("{bytes} B"),
        Some(bytes) => {
            let mut value = bytes as f64 / 1024.0;
            let mut unit = 0;
            while value >= 1024.0 && unit + 1 < UNITS.len() {
                value /= 1024.0;
                unit += 1;
            }
            format!("{value:.1} {}", UNITS[unit])
        }
    }
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

/// The folders visited before, for "Back".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    visited: Vec<String>,
}

impl History {
    /// Leaving the folder `from` for another one.
    pub fn visit(&mut self, from: &str) {
        const KEEP: usize = 100;
        self.visited.push(from.to_string());
        if self.visited.len() > KEEP {
            self.visited.remove(0);
        }
    }

    /// The folder to go back to, forgotten here.
    pub fn back(&mut self) -> Option<String> {
        self.visited.pop()
    }

    #[must_use]
    pub fn can_go_back(&self) -> bool {
        !self.visited.is_empty()
    }

    /// Another drive: nothing to go back to.
    pub fn clear(&mut self) {
        self.visited.clear();
    }
}

/// The folder above `prefix`; `None` at the root.
#[must_use]
pub fn up(prefix: &str) -> Option<String> {
    if prefix.is_empty() {
        None
    } else {
        Some(key::parent_prefix(prefix))
    }
}

/// The breadcrumb: the drive (its root), then every folder down to `prefix`,
/// as `(label, prefix)`.
#[must_use]
pub fn crumbs(drive_name: &str, prefix: &str) -> Vec<(String, String)> {
    let mut trail = vec![(drive_name.to_string(), String::new())];
    trail.extend(key::folder_trail(prefix));
    trail
}

/// The file an upload of `file_name` becomes in the folder `prefix`.
#[must_use]
pub fn upload_key(prefix: &str, file_name: &str) -> Option<String> {
    if file_name.contains('/') || key::check_path_key(file_name).is_err() {
        return None;
    }
    Some(format!("{prefix}{file_name}"))
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
        }
    }

    fn folder(name: &str) -> Entry {
        Entry {
            key: format!("f/{name}/"),
            name: name.to_string(),
            is_folder: true,
            size: None,
            modified: None,
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
    fn back_returns_to_the_folders_visited_before() {
        let mut history = History::default();
        assert!(!history.can_go_back());
        history.visit("");
        history.visit("mail/");
        assert!(history.can_go_back());
        assert_eq!(history.back().as_deref(), Some("mail/"));
        assert_eq!(history.back().as_deref(), Some(""));
        assert_eq!(history.back(), None);
    }

    #[test]
    fn up_goes_to_the_parent_and_stops_at_the_root() {
        assert_eq!(up("mail/inbox/").as_deref(), Some("mail/"));
        assert_eq!(up("mail/").as_deref(), Some(""));
        assert_eq!(up(""), None);
    }

    #[test]
    fn the_breadcrumb_starts_at_the_drive_and_ends_at_the_folder() {
        assert_eq!(
            crumbs("S3 Drive", ""),
            vec![("S3 Drive".to_string(), String::new())]
        );
        assert_eq!(
            crumbs("S3 Drive", "mail/inbox/"),
            vec![
                ("S3 Drive".to_string(), String::new()),
                ("mail".to_string(), "mail/".to_string()),
                ("inbox".to_string(), "mail/inbox/".to_string()),
            ]
        );
    }

    #[test]
    fn an_upload_goes_into_the_open_folder() {
        assert_eq!(upload_key("mail/", "a.txt").as_deref(), Some("mail/a.txt"));
        assert_eq!(upload_key("", "a.txt").as_deref(), Some("a.txt"));
        assert_eq!(upload_key("mail/", ""), None);
        assert_eq!(upload_key("", ".."), None);
        assert_eq!(upload_key("", "a/b"), None);
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
}
