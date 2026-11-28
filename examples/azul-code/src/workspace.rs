//! The workspace: the folder open in the explorer, its tree (listed folder
//! by folder as the user opens them, through the Drive), and the open files
//! (tabs). Plain Rust: the explorer (a virtualized list) shows the slice of
//! [`Workspace::rows`] in view - the rows are kept, made again only when a
//! listing arrives or a folder opens or closes - and a row is named by its
//! key.
//!
//! Also plain: where a path on disk is in a workspace ([`key_of_path`]),
//! what tells two open files apart ([`doc_ident`]), the recent folders
//! ([`remember`]) and quick open's ranking ([`quick_matches`]).

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

/// The names the explorer never shows (VSCode's `files.exclude` defaults).
pub static HIDDEN_ENTRIES: [&str; 6] = [".git", ".svn", ".hg", "CVS", ".DS_Store", "Thumbs.db"];

/// What the explorer never shows ([`HIDDEN_ENTRIES`]).
#[must_use]
pub fn hidden_entry(name: &str) -> bool {
    HIDDEN_ENTRIES.contains(&name)
}

/// Build output and dependencies: folders quick open and the search do not walk into.
pub static SKIPPED_FOLDERS: [&str; 3] = ["target", "node_modules", "bower_components"];

/// The folders quick open's index does not walk into: version control and
/// other dot folders, build output, dependencies (VSCode's `search.exclude`,
/// and Cargo's `target/`; [`SKIPPED_FOLDERS`]).
#[must_use]
pub fn skipped_folder(name: &str) -> bool {
    name.starts_with('.') || SKIPPED_FOLDERS.contains(&name)
}

/// Where the workspace's files are: a drive's folder and a key prefix in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Root {
    /// The folder the `LocalDrive` is opened at.
    pub drive_root: PathBuf,
    /// The workspace's keys in that drive start with this ("" for a folder
    /// of the user's, `code/sample/` for the sample in the data tree).
    pub prefix: String,
    /// The drive is the data tree's (it keeps the `.azlin/cache` manifest);
    /// `false` for a folder of the user's.
    pub data_tree: bool,
    /// What the explorer calls the workspace.
    pub name: String,
}

impl Root {
    /// The folder on disk the workspace is: the drive's folder, or the folder its prefix names
    /// in it (the sample's `code/sample/` in the data tree).
    #[must_use]
    pub fn folder(&self) -> PathBuf {
        let mut folder = self.drive_root.clone();
        for segment in self.prefix.split('/').filter(|s| !s.is_empty()) {
            folder.push(segment);
        }
        folder
    }
}

/// One entry of a listed folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub folder: bool,
}

/// A row of the explorer, in the tree's depth-first order (row `i` is the
/// tree's node `i + 1`; node 0 is the workspace itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The key relative to the workspace: `src/main.rs`, `src/` for a folder.
    pub key: String,
    pub name: String,
    pub depth: usize,
    pub folder: bool,
    pub expanded: bool,
}

/// The workspace. See the module documentation.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: Root,
    /// Listed folders (`""` is the workspace's own) and their entries,
    /// folders first, then by name.
    listings: BTreeMap<String, Vec<Entry>>,
    /// Open folders.
    expanded: BTreeSet<String>,
    /// The file or folder last clicked.
    pub selected: Option<String>,
    /// The explorer's rows, kept from the listings and the open folders (the
    /// virtualized explorer slices them on every frame of a scroll).
    rows: Vec<Row>,
}

impl Workspace {
    /// A workspace at `root`, nothing listed yet.
    #[must_use]
    pub fn new(root: Root) -> Workspace {
        Workspace {
            root,
            listings: BTreeMap::new(),
            expanded: BTreeSet::from([String::new()]),
            selected: None,
            rows: Vec::new(),
        }
    }

    /// The entries of `folder` (a key ending in `/`, or `""`) arrived (the
    /// [`hidden_entry`] names left out).
    pub fn set_listing(&mut self, folder: &str, mut folders: Vec<String>, mut files: Vec<String>) {
        folders.retain(|n| !hidden_entry(n));
        files.retain(|n| !hidden_entry(n));
        folders.sort_by_key(|n| n.to_lowercase());
        files.sort_by_key(|n| n.to_lowercase());
        let entries = folders
            .into_iter()
            .map(|name| Entry { name, folder: true })
            .chain(files.into_iter().map(|name| Entry { name, folder: false }))
            .collect();
        self.listings.insert(folder.to_string(), entries);
        self.rebuild_rows();
    }

    /// The rows made again from the listings and the open folders.
    fn rebuild_rows(&mut self) {
        let mut out = Vec::new();
        self.walk("", 0, &mut out);
        self.rows = out;
    }

    /// `folder`'s entries as rows at `depth`, open folders' entries under
    /// them.
    fn walk(&self, folder: &str, depth: usize, out: &mut Vec<Row>) {
        let Some(entries) = self.listings.get(folder) else {
            return;
        };
        for e in entries {
            let key = if e.folder {
                format!("{folder}{}/", e.name)
            } else {
                format!("{folder}{}", e.name)
            };
            let expanded = e.folder && self.expanded.contains(&key);
            out.push(Row {
                key: key.clone(),
                name: e.name.clone(),
                depth,
                folder: e.folder,
                expanded,
            });
            if expanded {
                self.walk(&key, depth + 1, out);
            }
        }
    }

    /// Whether `folder` was listed.
    #[must_use]
    pub fn is_listed(&self, folder: &str) -> bool {
        self.listings.contains_key(folder)
    }

    /// Opens or closes `folder`; `true` when it must be listed first.
    pub fn toggle(&mut self, folder: &str, open: bool) -> bool {
        let list = if open {
            self.expanded.insert(folder.to_string());
            !self.listings.contains_key(folder)
        } else {
            self.expanded.remove(folder);
            false
        };
        self.rebuild_rows();
        list
    }

    /// Whether `folder` is open.
    #[must_use]
    pub fn is_expanded(&self, folder: &str) -> bool {
        self.expanded.contains(folder)
    }

    /// Closes every folder (the explorer's "Collapse Folders").
    pub fn collapse_all(&mut self) {
        self.expanded = BTreeSet::from([String::new()]);
        self.rebuild_rows();
    }

    /// The explorer's rows: the workspace's entries, an open folder's
    /// entries under it, depth first.
    #[must_use]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The row of `key`, if it is shown.
    #[must_use]
    pub fn index_of(&self, key: &str) -> Option<usize> {
        self.rows.iter().position(|r| r.key == key)
    }

    /// Forgets every listing (the explorer's Refresh); the open folders, the
    /// workspace's own (`""`) first, are what to list again.
    pub fn refresh(&mut self) -> Vec<String> {
        self.listings.clear();
        self.rebuild_rows();
        self.expanded.iter().cloned().collect()
    }

    /// The drive key of workspace key `key`.
    #[must_use]
    pub fn drive_key(&self, key: &str) -> String {
        format!("{}{}", self.root.prefix, key)
    }

    /// The workspace key of drive key `drive_key` (`None` outside it).
    #[must_use]
    pub fn relative_key(&self, drive_key: &str) -> Option<String> {
        drive_key.strip_prefix(self.root.prefix.as_str()).map(str::to_string)
    }
}

/// The last segment of a key: `src/main.rs` -> `main.rs`, `src/` -> `src`.
#[must_use]
pub fn file_name(key: &str) -> &str {
    let key = key.strip_suffix('/').unwrap_or(key);
    key.rsplit_once('/').map_or(key, |(_, name)| name)
}

/// The folder a key is in: `src/main.rs` and `src/deep/` are in `src/`;
/// `None` for an entry of the workspace's own folder.
#[must_use]
pub fn parent_folder(key: &str) -> Option<&str> {
    let key = key.strip_suffix('/').unwrap_or(key);
    key.rfind('/').map(|i| &key[..=i])
}

/// A tab's label: the file's name, `*` when it has unsaved changes.
#[must_use]
pub fn tab_label(name: &str, dirty: bool) -> String {
    if dirty {
        format!("{name} *")
    } else {
        name.to_string()
    }
}

/// The workspace key of the file at `path` when it lies in `root`'s folder
/// (and under its prefix); `None` for a path outside it, or the folder
/// itself.
#[must_use]
pub fn key_of_path(root: &Root, path: &Path) -> Option<String> {
    let inside = path.strip_prefix(&root.drive_root).ok()?;
    let mut parts = Vec::new();
    for part in inside.components() {
        match part {
            Component::Normal(name) => parts.push(name.to_str()?.to_string()),
            _ => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    let drive_key = parts.join("/");
    drive_key
        .strip_prefix(root.prefix.as_str())
        .filter(|key| !key.is_empty())
        .map(str::to_string)
}

/// What tells two open files apart: the file's place on disk (`root` and
/// `key`). A file opened alone has its own folder as its root, so its key
/// alone ("main.rs") could be a workspace file's too.
#[must_use]
pub fn doc_ident(root: &Root, key: &str) -> String {
    root.drive_root
        .join(format!("{}{}", root.prefix, key))
        .display()
        .to_string()
}

/// The most folders the recent list keeps.
pub const RECENT_MAX: usize = 10;

/// The recent folders with `folder` first, each once, [`RECENT_MAX`] at
/// most.
#[must_use]
pub fn remember(recent: &[String], folder: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(RECENT_MAX);
    out.push(folder.to_string());
    out.extend(
        recent
            .iter()
            .filter(|f| f.as_str() != folder)
            .take(RECENT_MAX - 1)
            .cloned(),
    );
    out
}

/// The recent folders as the settings keep them (a JSON array).
#[must_use]
pub fn recent_to_json(recent: &[String]) -> String {
    serde_json::to_string(recent).unwrap_or_else(|_| "[]".to_string())
}

/// The recent folders from the settings (none from anything but a JSON
/// array of strings).
#[must_use]
pub fn recent_from_json(text: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(text)
        .unwrap_or_default()
        .into_iter()
        .filter(|f| !f.trim().is_empty())
        .take(RECENT_MAX)
        .collect()
}

/// The most files quick open lists.
pub const QUICK_MAX: usize = 50;

/// Whether every letter of `query` (spaces left out) occurs in `text` in
/// order, in any case - the command palette's own rule, so the palette
/// keeps every row quick open hands it.
#[must_use]
pub fn letters_in_order(query: &str, text: &str) -> bool {
    let mut haystack = text.chars().flat_map(char::to_lowercase);
    for wanted in query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
    {
        if !haystack.any(|c| c == wanted) {
            return false;
        }
    }
    true
}

/// The files of `files` (workspace keys) quick open lists for `query`, as
/// indices, best first: a file name that starts with the query, then one
/// that holds it, then any path with its letters in order; shorter paths
/// first among equals. At most `max`.
#[must_use]
pub fn quick_matches(files: &[String], query: &str, max: usize) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    let mut hits: Vec<(u8, usize, usize)> = files
        .iter()
        .enumerate()
        .filter(|(_, f)| letters_in_order(&query, f))
        .map(|(i, f)| {
            let name = file_name(f).to_lowercase();
            let rank = if query.is_empty() {
                2
            } else if name.starts_with(&query) {
                0
            } else if name.contains(&query) {
                1
            } else {
                2
            };
            (rank, f.len(), i)
        })
        .collect();
    hits.sort_unstable();
    hits.into_iter().take(max).map(|(_, _, i)| i).collect()
}

/// What a tab holds, for [`Tabs`].
pub trait TabDoc {
    /// The workspace key of the file.
    fn key(&self) -> &str;
}

/// The open files in tab order and the one in front.
#[derive(Debug, Clone)]
pub struct Tabs<D> {
    pub docs: Vec<D>,
    pub active: usize,
}

impl<D> Default for Tabs<D> {
    fn default() -> Self {
        Tabs {
            docs: Vec::new(),
            active: 0,
        }
    }
}

impl<D: TabDoc> Tabs<D> {
    /// The tab of `key`, if it is open.
    #[must_use]
    pub fn find(&self, key: &str) -> Option<usize> {
        self.docs.iter().position(|d| d.key() == key)
    }

    /// `doc` in front: its own tab when the file is open already (the new
    /// copy is dropped), else a new tab after the active one.
    pub fn open(&mut self, doc: D) -> usize {
        if let Some(i) = self.find(doc.key()) {
            self.active = i;
            return i;
        }
        let at = if self.docs.is_empty() {
            0
        } else {
            (self.active + 1).min(self.docs.len())
        };
        self.docs.insert(at, doc);
        self.active = at;
        at
    }

    /// Closes tab `index`; the one right of it (else left) comes to front.
    pub fn close(&mut self, index: usize) -> Option<D> {
        if index >= self.docs.len() {
            return None;
        }
        let doc = self.docs.remove(index);
        if self.docs.is_empty() {
            self.active = 0;
        } else if self.active > index {
            self.active -= 1;
        } else if self.active == index {
            self.active = index.min(self.docs.len() - 1);
        }
        Some(doc)
    }

    #[must_use]
    pub fn active(&self) -> Option<&D> {
        self.docs.get(self.active)
    }

    pub fn active_mut(&mut self) -> Option<&mut D> {
        self.docs.get_mut(self.active)
    }
}

#[cfg(test)]
#[path = "workspace_test.rs"]
mod tests;
