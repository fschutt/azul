//! The library: every note with its notebook and tags, and what the note
//! list shows for a scope, a search and a sort.
//!
//! The files (the S3 split: durable data are files, one per note):
//!
//! ```text
//! notes/<notebook>/<uuid>.md            one note (front matter + Markdown)
//! notes/<notebook>/<uuid>/assets/<file> its pasted / dropped images
//! notes/<notebook>/.notebook            an empty notebook's marker
//! notes/.trash/<notebook>/<uuid>.md     a deleted note, restorable
//! notes/.history/<uuid>/<stamp>.md      a version, one per save
//! ```
//!
//! A notebook is a `/` path (`Work/Offsite`): nested notebooks are nested
//! folders. The note's body is azul's `RichTextDoc` (the shared rich-text
//! editor's document); nothing else here knows azul.

use std::collections::BTreeSet;

use azul::widgets::RichTextDoc;

use crate::markdown::{self, Meta};

/// `text` cut to `max` characters, with an ellipsis when cut.
#[must_use]
pub fn truncate_chars(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        out.push('\u{2026}');
    }
    out
}

/// Every key of the app starts here.
pub const NOTES_ROOT: &str = "notes/";
/// The trash folder under the root; a trashed note keeps its notebook path
/// below it.
pub const TRASH: &str = ".trash";
/// The versions folder under the root.
pub const HISTORY: &str = ".history";
/// The marker object of a notebook without notes.
pub const NOTEBOOK_MARKER: &str = ".notebook";
/// Where a note goes when no notebook is chosen.
pub const DEFAULT_NOTEBOOK: &str = "Notes";
/// The title shown for a note without one.
pub const UNTITLED: &str = "Untitled";

/// One note in memory.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    /// The file name without `.md` (a UUID for notes AzNotes created).
    pub id: String,
    /// `Work/Offsite`; a trashed note's starts with `.trash/`.
    pub notebook: String,
    pub meta: Meta,
    pub doc: RichTextDoc,
    /// The file as last read or written: an identical save is skipped, a
    /// different file on disk is an edit made elsewhere.
    pub saved: String,
    /// The key the file was last written under, when the note has moved
    /// since (the save deletes it).
    pub moved_from: Option<String>,
    /// Edited since the last save.
    pub dirty: bool,
    /// Bumped by every edit: a save that took generation `g` leaves the
    /// note dirty when an edit came in while it ran.
    pub generation: u64,
    /// The file's modified time (seconds) as last read or written: a
    /// rescan reads only files whose time differs.
    pub file_modified: u64,
    /// Lowercased title, text, tags and notebook (the search's haystack).
    pub haystack: String,
    /// The first line of text that is not the title.
    pub preview: String,
}

impl Note {
    /// A new, empty note.
    #[must_use]
    pub fn new(id: &str, notebook: &str, now: u64) -> Note {
        let mut note = Note {
            id: id.to_string(),
            notebook: notebook.to_string(),
            meta: Meta {
                created: now,
                modified: now,
                ..Meta::default()
            },
            doc: RichTextDoc::create(),
            saved: String::new(),
            moved_from: None,
            dirty: true,
            generation: 1,
            file_modified: 0,
            haystack: String::new(),
            preview: String::new(),
        };
        note.refresh();
        note
    }

    /// The note of a file read from `key`; `None` when the key is not a
    /// note's.
    #[must_use]
    pub fn from_file(key: &str, text: &str, file_modified: u64) -> Option<Note> {
        let (notebook, id) = parse_note_key(key)?;
        let (meta, doc) = markdown::parse_note(text, file_modified);
        let mut note = Note {
            id,
            notebook,
            meta,
            doc,
            saved: text.to_string(),
            moved_from: None,
            dirty: false,
            generation: 0,
            file_modified,
            haystack: String::new(),
            preview: String::new(),
        };
        note.refresh();
        Some(note)
    }

    /// The file this note is written as.
    #[must_use]
    pub fn to_file(&self) -> String {
        markdown::note_to_file(&self.meta, &self.doc)
    }

    #[must_use]
    pub fn key(&self) -> String {
        note_key(&self.notebook, &self.id)
    }

    /// Where its images go.
    #[must_use]
    pub fn assets_prefix(&self) -> String {
        assets_prefix(&self.notebook, &self.id)
    }

    /// The title, or "Untitled".
    #[must_use]
    pub fn display_title(&self) -> &str {
        let title = self.meta.title.trim();
        if title.is_empty() {
            UNTITLED
        } else {
            title
        }
    }

    #[must_use]
    pub fn is_trashed(&self) -> bool {
        is_trash_path(&self.notebook)
    }

    /// The notebook without the trash prefix.
    #[must_use]
    pub fn home_notebook(&self) -> &str {
        self.notebook
            .strip_prefix(TRASH)
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or(&self.notebook)
    }

    /// Recomputes the search haystack and the preview (after any edit).
    pub fn refresh(&mut self) {
        let mut hay = String::new();
        hay.push_str(&self.meta.title);
        hay.push('\n');
        hay.push_str(self.doc.plain_text().as_str());
        hay.push('\n');
        for tag in &self.meta.tags {
            hay.push('#');
            hay.push_str(tag);
            hay.push(' ');
        }
        hay.push('\n');
        hay.push_str(self.home_notebook());
        self.haystack = hay.to_lowercase();
        self.preview = self.doc.preview(self.meta.title.as_str()).as_str().to_string();
    }

    /// Marks the note as needing a save (without a new modified date: a
    /// pin, a move).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.generation += 1;
    }

    /// Marks the note edited at `now`.
    pub fn touch(&mut self, now: u64) {
        self.meta.modified = now.max(self.meta.created);
        self.mark_dirty();
    }

    /// Moves the note to `notebook` (the next save deletes the old file).
    pub fn move_to(&mut self, notebook: &str) {
        if self.notebook == notebook {
            return;
        }
        if self.moved_from.is_none() && !self.saved.is_empty() {
            self.moved_from = Some(self.key());
        }
        self.notebook = notebook.to_string();
        self.mark_dirty();
        self.refresh();
    }

    /// Whether the note has `tag` (ignoring case).
    #[must_use]
    pub fn has_tag(&self, tag: &str) -> bool {
        self.meta.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
    }

    /// Adds `tag` (cleaned; once). Returns whether it was added.
    pub fn add_tag(&mut self, tag: &str) -> bool {
        let tag = markdown::clean_tag(tag);
        if tag.is_empty() || self.has_tag(&tag) {
            return false;
        }
        self.meta.tags.push(tag);
        self.refresh();
        true
    }

    /// Removes `tag`. Returns whether it was there.
    pub fn remove_tag(&mut self, tag: &str) -> bool {
        let before = self.meta.tags.len();
        self.meta.tags.retain(|t| !t.eq_ignore_ascii_case(tag));
        let removed = self.meta.tags.len() != before;
        if removed {
            self.refresh();
        }
        removed
    }
}

// ==== Keys ====

/// `notes/<notebook>/<id>.md` (`notes/<id>.md` without a notebook).
#[must_use]
pub fn note_key(notebook: &str, id: &str) -> String {
    if notebook.is_empty() {
        format!("{NOTES_ROOT}{id}.md")
    } else {
        format!("{NOTES_ROOT}{notebook}/{id}.md")
    }
}

/// `notes/<notebook>/<id>/assets/`.
#[must_use]
pub fn assets_prefix(notebook: &str, id: &str) -> String {
    if notebook.is_empty() {
        format!("{NOTES_ROOT}{id}/assets/")
    } else {
        format!("{NOTES_ROOT}{notebook}/{id}/assets/")
    }
}

/// `notes/<notebook>/.notebook`.
#[must_use]
pub fn marker_key(notebook: &str) -> String {
    format!("{NOTES_ROOT}{notebook}/{NOTEBOOK_MARKER}")
}

/// `notes/.history/<id>/`.
#[must_use]
pub fn history_prefix(id: &str) -> String {
    format!("{NOTES_ROOT}{HISTORY}/{id}/")
}

/// `notes/.history/<id>/<stamp>.md`; the stamp (`20260930T081000Z`) sorts
/// by time.
#[must_use]
pub fn history_key(id: &str, unix_secs: u64) -> String {
    format!("{}{}.md", history_prefix(id), azul_storage::time::amz_date(unix_secs))
}

/// The time of a version key, from its stamp.
#[must_use]
pub fn history_time(key: &str) -> Option<u64> {
    let stamp = key.rsplit('/').next()?.strip_suffix(".md")?;
    // 20260930T081000Z -> 2026-09-30T08:10:00Z
    if stamp.len() != 16 {
        return None;
    }
    let iso = format!(
        "{}-{}-{}T{}:{}:{}Z",
        stamp.get(0..4)?,
        stamp.get(4..6)?,
        stamp.get(6..8)?,
        stamp.get(9..11)?,
        stamp.get(11..13)?,
        stamp.get(13..15)?
    );
    azul_storage::time::parse_iso8601(&iso)
}

/// `(notebook, id)` of a note's key; `None` for anything else (an asset, a
/// version, a marker, a file that is not Markdown).
#[must_use]
pub fn parse_note_key(key: &str) -> Option<(String, String)> {
    let rest = key.strip_prefix(NOTES_ROOT)?;
    let stem = rest.strip_suffix(".md")?;
    if rest.starts_with(&format!("{HISTORY}/")) {
        return None;
    }
    let (notebook, id) = match stem.rfind('/') {
        Some(i) => (&stem[..i], &stem[i + 1..]),
        None => ("", stem),
    };
    if id.is_empty() || id.starts_with('.') {
        return None;
    }
    // A Markdown file inside a note's assets folder is an asset.
    if notebook.contains('/') && notebook.rsplit('/').next() == Some("assets") {
        return None;
    }
    Some((notebook.to_string(), id.to_string()))
}

/// The notebook of a marker key (`notes/Work/.notebook` -> `Work`).
#[must_use]
pub fn parse_marker_key(key: &str) -> Option<String> {
    let rest = key.strip_prefix(NOTES_ROOT)?;
    let notebook = rest.strip_suffix(&format!("/{NOTEBOOK_MARKER}"))?;
    (!notebook.is_empty() && !is_trash_path(notebook)).then(|| notebook.to_string())
}

/// A notebook path under the trash.
#[must_use]
pub fn is_trash_path(notebook: &str) -> bool {
    notebook == TRASH || notebook.starts_with(&format!("{TRASH}/"))
}

/// The trash path of `notebook`.
#[must_use]
pub fn trash_path(notebook: &str) -> String {
    if notebook.is_empty() {
        TRASH.to_string()
    } else {
        format!("{TRASH}/{notebook}")
    }
}

/// A notebook name the user typed, as a path: segments trimmed, no empty
/// segment, no `.` / `..`, nothing starting with `.` (the app's own
/// folders), no backslash or control character.
pub fn clean_notebook_path(name: &str) -> Result<String, &'static str> {
    let mut segments = Vec::new();
    for segment in name.split('/') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        if segment.starts_with('.') {
            return Err("a notebook name cannot start with a dot");
        }
        if segment.contains('\\') || segment.chars().any(char::is_control) {
            return Err("a notebook name cannot hold a backslash or a control character");
        }
        if segment == "assets" {
            return Err("\"assets\" is the name of a note's image folder");
        }
        segments.push(segment.to_string());
    }
    if segments.is_empty() {
        return Err("the notebook needs a name");
    }
    Ok(segments.join("/"))
}

/// The last segment of a notebook path (its name in the tree).
#[must_use]
pub fn notebook_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

// ==== Queries ====

/// Which notes the list shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    All,
    Pinned,
    /// A notebook and the notebooks inside it.
    Notebook(String),
    Tag(String),
    Trash,
}

impl Scope {
    /// The list's heading for the scope.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Scope::All => "All notes".to_string(),
            Scope::Pinned => "Pinned".to_string(),
            Scope::Notebook(path) => notebook_name(path).to_string(),
            Scope::Tag(tag) => format!("#{tag}"),
            Scope::Trash => "Trash".to_string(),
        }
    }
}

/// What the list is sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Modified,
    Created,
    Title,
}

impl SortKey {
    pub const ALL: [SortKey; 3] = [SortKey::Modified, SortKey::Created, SortKey::Title];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Modified => "Date modified",
            SortKey::Created => "Date created",
            SortKey::Title => "Title",
        }
    }

    /// The next key ("Arrange by" cycles).
    #[must_use]
    pub fn next(self) -> SortKey {
        match self {
            SortKey::Modified => SortKey::Created,
            SortKey::Created => SortKey::Title,
            SortKey::Title => SortKey::Modified,
        }
    }

    /// What the direction toggle says in this direction.
    #[must_use]
    pub fn direction_label(self, descending: bool) -> &'static str {
        match (self, descending) {
            (SortKey::Title, false) => "A on top",
            (SortKey::Title, true) => "Z on top",
            (_, true) => "Newest on top",
            (_, false) => "Oldest on top",
        }
    }
}

/// The list's query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub scope: Scope,
    pub search: String,
    pub sort: SortKey,
    pub descending: bool,
}

impl Default for Query {
    fn default() -> Self {
        Query {
            scope: Scope::All,
            search: String::new(),
            sort: SortKey::Modified,
            descending: true,
        }
    }
}

/// One row of the note list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListRow {
    /// A section header ("Pinned", "Today").
    Section(String),
    /// A note: its index in [`Library::notes`].
    Note(usize),
}

/// The counts the navigation shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub all: usize,
    pub pinned: usize,
    pub trash: usize,
}

/// A notebook in the navigation tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotebookNode {
    pub name: String,
    pub path: String,
    /// Notes in it and in the notebooks inside it.
    pub count: usize,
    pub children: Vec<NotebookNode>,
}

/// Every note, and the notebooks that exist without notes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Library {
    pub notes: Vec<Note>,
    /// Notebooks with a marker (they show even when empty).
    pub notebooks: BTreeSet<String>,
}

/// Whether `note` is in `scope`.
#[must_use]
pub fn in_scope(note: &Note, scope: &Scope) -> bool {
    match scope {
        Scope::Trash => note.is_trashed(),
        _ if note.is_trashed() => false,
        Scope::All => true,
        Scope::Pinned => note.meta.pinned,
        Scope::Notebook(path) => {
            note.notebook == *path || note.notebook.starts_with(&format!("{path}/"))
        }
        Scope::Tag(tag) => note.has_tag(tag),
    }
}

/// Whether `note` matches every word of `search`: a word anywhere in the
/// title, text, tags or notebook (ignoring case); `#word` a tag that starts
/// with it.
#[must_use]
pub fn matches_search(note: &Note, search: &str) -> bool {
    search.split_whitespace().all(|term| {
        let term = term.to_lowercase();
        match term.strip_prefix('#') {
            Some(tag) if !tag.is_empty() => note
                .meta
                .tags
                .iter()
                .any(|t| t.to_lowercase().starts_with(tag)),
            _ => note.haystack.contains(&term),
        }
    })
}

impl Library {
    #[must_use]
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.notes.iter().position(|n| n.id == id)
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Note> {
        self.notes.iter_mut().find(|n| n.id == id)
    }

    /// Adds `note`, or replaces the note with its id. Returns its index.
    pub fn upsert(&mut self, note: Note) -> usize {
        match self.index_of(&note.id) {
            Some(i) => {
                self.notes[i] = note;
                i
            }
            None => {
                self.notes.push(note);
                self.notes.len() - 1
            }
        }
    }

    /// Every notebook (outside the trash), parents included, sorted.
    #[must_use]
    pub fn notebook_paths(&self) -> Vec<String> {
        let mut all: BTreeSet<String> = BTreeSet::new();
        let mut add = |path: &str| {
            let mut so_far = String::new();
            for segment in path.split('/').filter(|s| !s.is_empty()) {
                if !so_far.is_empty() {
                    so_far.push('/');
                }
                so_far.push_str(segment);
                all.insert(so_far.clone());
            }
        };
        for note in self.notes.iter().filter(|n| !n.is_trashed()) {
            add(&note.notebook);
        }
        for notebook in &self.notebooks {
            add(notebook);
        }
        let mut paths: Vec<String> = all.into_iter().collect();
        paths.sort_by_key(|p| p.to_lowercase());
        paths
    }

    /// Notes in `path` and the notebooks inside it.
    #[must_use]
    pub fn count_in(&self, path: &str) -> usize {
        let scope = Scope::Notebook(path.to_string());
        self.notes.iter().filter(|n| in_scope(n, &scope)).count()
    }

    /// The notebooks as a tree, each with its count.
    #[must_use]
    pub fn notebook_tree(&self) -> Vec<NotebookNode> {
        fn build(lib: &Library, paths: &[String], parent: &str) -> Vec<NotebookNode> {
            paths
                .iter()
                .filter(|p| match p.rfind('/') {
                    Some(i) => &p[..i] == parent,
                    None => parent.is_empty(),
                })
                .map(|p| NotebookNode {
                    name: notebook_name(p).to_string(),
                    path: p.clone(),
                    count: lib.count_in(p),
                    children: build(lib, paths, p),
                })
                .collect()
        }
        let paths = self.notebook_paths();
        build(self, &paths, "")
    }

    /// Every tag with its count (outside the trash), by name; tags that
    /// differ only in case are one.
    #[must_use]
    pub fn tags(&self) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = Vec::new();
        for note in self.notes.iter().filter(|n| !n.is_trashed()) {
            for tag in &note.meta.tags {
                match out.iter_mut().find(|(t, _)| t.eq_ignore_ascii_case(tag)) {
                    Some((_, n)) => *n += 1,
                    None => out.push((tag.clone(), 1)),
                }
            }
        }
        out.sort_by_key(|(t, _)| t.to_lowercase());
        out
    }

    #[must_use]
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for note in &self.notes {
            if note.is_trashed() {
                counts.trash += 1;
            } else {
                counts.all += 1;
                if note.meta.pinned {
                    counts.pinned += 1;
                }
            }
        }
        counts
    }

    /// The indices of the notes `query` shows, in list order: pinned notes
    /// first (outside the trash), then by the sort key.
    #[must_use]
    pub fn matching(&self, query: &Query) -> Vec<usize> {
        let mut hits: Vec<usize> = self
            .notes
            .iter()
            .enumerate()
            .filter(|(_, n)| in_scope(n, &query.scope) && matches_search(n, &query.search))
            .map(|(i, _)| i)
            .collect();
        let pin_first = !matches!(query.scope, Scope::Trash);
        hits.sort_by(|&a, &b| {
            let (na, nb) = (&self.notes[a], &self.notes[b]);
            let pinned = if pin_first {
                nb.meta.pinned.cmp(&na.meta.pinned)
            } else {
                core::cmp::Ordering::Equal
            };
            let key = match query.sort {
                SortKey::Modified => na.meta.modified.cmp(&nb.meta.modified),
                SortKey::Created => na.meta.created.cmp(&nb.meta.created),
                SortKey::Title => na
                    .display_title()
                    .to_lowercase()
                    .cmp(&nb.display_title().to_lowercase()),
            };
            let key = if query.descending { key.reverse() } else { key };
            pinned
                .then(key)
                .then_with(|| na.display_title().cmp(nb.display_title()))
                .then_with(|| na.id.cmp(&nb.id))
        });
        hits
    }

    /// The list's rows: [`Self::matching`] under section headers - "Pinned",
    /// then the date buckets of the sort key ("Today", "Yesterday",
    /// "Previous 7 days", "Previous 30 days", then months) when sorted by a
    /// date; by title, "Pinned" and "Notes" when there are pinned notes.
    /// `now` and `offset` (local time minus UTC, seconds) place the days.
    #[must_use]
    pub fn rows(&self, query: &Query, now: u64, offset: i64) -> Vec<ListRow> {
        let hits = self.matching(query);
        let pin_first = !matches!(query.scope, Scope::Trash | Scope::Pinned);
        let any_pinned = pin_first && hits.iter().any(|&i| self.notes[i].meta.pinned);
        let mut rows = Vec::with_capacity(hits.len() + 6);
        let mut current: Option<String> = None;
        for i in hits {
            let note = &self.notes[i];
            let section = if any_pinned && note.meta.pinned {
                Some("Pinned".to_string())
            } else {
                match query.sort {
                    SortKey::Title if any_pinned => Some("Notes".to_string()),
                    SortKey::Title => None,
                    SortKey::Modified => Some(date_bucket(note.meta.modified, now, offset)),
                    SortKey::Created => Some(date_bucket(note.meta.created, now, offset)),
                }
            };
            if section.is_some() && section != current {
                if let Some(s) = &section {
                    rows.push(ListRow::Section(s.clone()));
                }
                current = section;
            }
            rows.push(ListRow::Note(i));
        }
        rows
    }
}

// ==== Dates ====

/// The local date-time of `unix` (UTC plus `offset` seconds).
fn local(unix: u64, offset: i64) -> Option<chrono::NaiveDateTime> {
    let secs = i64::try_from(unix).ok()?.saturating_add(offset);
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0).map(|d| d.naive_utc())
}

/// Days between `then` and `now` on the local calendar (0 = the same day).
fn days_ago(then: u64, now: u64, offset: i64) -> i64 {
    use chrono::Datelike;
    match (local(then, offset), local(now, offset)) {
        (Some(t), Some(n)) => i64::from(n.date().num_days_from_ce()) - i64::from(t.date().num_days_from_ce()),
        _ => 0,
    }
}

/// The section a note dated `then` sits in.
#[must_use]
pub fn date_bucket(then: u64, now: u64, offset: i64) -> String {
    match days_ago(then, now, offset) {
        i64::MIN..=0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        2..=6 => "Previous 7 days".to_string(),
        7..=29 => "Previous 30 days".to_string(),
        _ => local(then, offset)
            .map(|t| t.format("%B %Y").to_string())
            .unwrap_or_default(),
    }
}

/// A row's date: `09:14` today, `Yesterday`, a weekday this week, `Sep 30`
/// this year, `2025-09-30` before.
#[must_use]
pub fn short_date(then: u64, now: u64, offset: i64) -> String {
    use chrono::Datelike;
    let (Some(t), Some(n)) = (local(then, offset), local(now, offset)) else {
        return String::new();
    };
    match days_ago(then, now, offset) {
        i64::MIN..=0 => t.format("%H:%M").to_string(),
        1 => "Yesterday".to_string(),
        2..=6 => t.format("%a").to_string(),
        _ if t.year() == n.year() => t.format("%b %-d").to_string(),
        _ => t.format("%Y-%m-%d").to_string(),
    }
}

/// `Edited Tue 08:10` style: the full local date and time.
#[must_use]
pub fn long_date(then: u64, offset: i64) -> String {
    local(then, offset)
        .map(|t| t.format("%a %Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

/// The note's text for the status bar: words and, for a checklist, how
/// much is done.
#[must_use]
pub fn status_text(note: &Note) -> String {
    let words = note.doc.word_count();
    let (done, total) = (note.doc.checklist_done(), note.doc.checklist_total());
    let mut out = format!("{words} word{}", if words == 1 { "" } else { "s" });
    if total > 0 {
        out.push_str(&format!(" | {done} of {total} done"));
    }
    out
}

/// One line of a comparison: kept, removed (only in the old text) or added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Kept,
    Removed,
    Added,
}

/// The lines of `old` and `new` as a comparison (the longest common
/// subsequence of lines); `None` for texts too long to compare here.
#[must_use]
pub fn line_diff(old: &str, new: &str) -> Option<Vec<(Change, String)>> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    if a.len().saturating_mul(b.len()) > 4_000_000 {
        return None;
    }
    // lcs[i][j]: the common lines of a[i..] and b[j..].
    let mut lcs = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            out.push((Change::Kept, a[i].to_string()));
            i += 1;
            j += 1;
        } else if j < b.len() && (i == a.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
            out.push((Change::Added, b[j].to_string()));
            j += 1;
        } else {
            out.push((Change::Removed, a[i].to_string()));
            i += 1;
        }
    }
    Some(out)
}

/// The preview line of a list row: the first line, else the tags.
#[must_use]
pub fn row_preview(note: &Note) -> String {
    if !note.preview.is_empty() {
        return note.preview.clone();
    }
    if !note.meta.tags.is_empty() {
        return truncate_chars(
            &note
                .meta
                .tags
                .iter()
                .map(|t| format!("#{t}"))
                .collect::<Vec<_>>()
                .join(" "),
            80,
        );
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400;
    /// 2026-09-30 12:00 UTC.
    const NOW: u64 = 1_790_769_600;

    fn note(id: &str, notebook: &str, title: &str, modified: u64) -> Note {
        let mut n = Note::new(id, notebook, modified);
        n.meta.title = title.to_string();
        n.dirty = false;
        n.refresh();
        n
    }

    fn library() -> Library {
        let mut offsite = note("a", "Work/Offsite", "Offsite agenda", NOW - 3600);
        offsite.meta.pinned = true;
        offsite.meta.tags = vec!["planning".to_string()];
        offsite.doc = RichTextDoc::create_from_markdown("# Offsite agenda\n\n- Bring laptops\n");
        offsite.refresh();
        let mut standup = note("b", "Work/Meetings", "Standup notes", NOW - 600);
        standup.meta.tags = vec!["Planning".to_string(), "daily".to_string()];
        standup.refresh();
        let recipe = note("c", "Personal/Recipes", "Pancakes", NOW - DAY);
        let old = note("d", "Personal", "Old idea", NOW - 40 * DAY);
        let mut gone = note("e", ".trash/Work", "Deleted", NOW - 2 * DAY);
        gone.refresh();
        let mut lib = Library {
            notes: vec![offsite, standup, recipe, old, gone],
            notebooks: BTreeSet::new(),
        };
        lib.notebooks.insert("Archive".to_string());
        lib
    }

    fn ids(lib: &Library, rows: &[ListRow]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                ListRow::Section(s) => format!("[{s}]"),
                ListRow::Note(i) => lib.notes[*i].id.clone(),
            })
            .collect()
    }

    #[test]
    fn keys_name_the_notebook_folder_and_parse_back() {
        assert_eq!(note_key("Work/Offsite", "abc"), "notes/Work/Offsite/abc.md");
        assert_eq!(
            parse_note_key("notes/Work/Offsite/abc.md"),
            Some(("Work/Offsite".to_string(), "abc".to_string()))
        );
        assert_eq!(parse_note_key("notes/abc.md"), Some((String::new(), "abc".to_string())));
        assert_eq!(parse_note_key("notes/.history/abc/20260930T081000Z.md"), None);
        assert_eq!(parse_note_key("notes/Work/abc/assets/readme.md"), None);
        assert_eq!(parse_note_key("notes/Work/abc/assets/pic.png"), None);
        assert_eq!(parse_note_key("notes/Work/.notebook"), None);
        assert_eq!(assets_prefix("Work", "abc"), "notes/Work/abc/assets/");
        assert_eq!(parse_marker_key("notes/Archive/.notebook"), Some("Archive".to_string()));
        assert_eq!(parse_marker_key("notes/.trash/Archive/.notebook"), None);
    }

    #[test]
    fn a_version_key_sorts_by_time_and_reads_its_time_back() {
        let key = history_key("abc", NOW);
        assert_eq!(key, "notes/.history/abc/20260930T120000Z.md");
        assert_eq!(history_time(&key), Some(NOW));
        assert!(history_key("abc", NOW - 1) < key);
    }

    #[test]
    fn notebook_names_are_cleaned_and_the_apps_folders_refused() {
        assert_eq!(clean_notebook_path(" Work / Offsite "), Ok("Work/Offsite".to_string()));
        assert!(clean_notebook_path(".trash").is_err());
        assert!(clean_notebook_path("a/../b").is_err());
        assert!(clean_notebook_path("  /  ").is_err());
        assert!(clean_notebook_path("x/assets").is_err());
    }

    #[test]
    fn search_matches_every_word_in_title_text_tags_or_notebook() {
        let lib = library();
        let hit = |q: &str| -> Vec<String> {
            let query = Query {
                search: q.to_string(),
                ..Query::default()
            };
            let mut ids: Vec<String> = lib.matching(&query).iter().map(|&i| lib.notes[i].id.clone()).collect();
            ids.sort();
            ids
        };
        assert_eq!(hit("laptops"), vec!["a"]);
        assert_eq!(hit("LAPTOPS offsite"), vec!["a"], "every word, any case");
        assert_eq!(hit("#plan"), vec!["a", "b"], "a tag prefix, any case");
        assert_eq!(hit("recipes"), vec!["c"], "the notebook");
        assert_eq!(hit("deleted"), Vec::<String>::new(), "the trash is not searched outside it");
        assert_eq!(hit("").len(), 4);
    }

    #[test]
    fn scopes_pick_pinned_notebooks_with_their_children_tags_and_the_trash() {
        let lib = library();
        let scope = |scope: Scope| -> Vec<String> {
            let query = Query {
                scope,
                ..Query::default()
            };
            lib.matching(&query).iter().map(|&i| lib.notes[i].id.clone()).collect()
        };
        assert_eq!(scope(Scope::Pinned), vec!["a"]);
        assert_eq!(scope(Scope::Notebook("Work".to_string())), vec!["a", "b"], "pinned first");
        assert_eq!(scope(Scope::Notebook("Personal".to_string())), vec!["c", "d"]);
        assert_eq!(scope(Scope::Tag("planning".to_string())), vec!["a", "b"]);
        assert_eq!(scope(Scope::Trash), vec!["e"]);
    }

    #[test]
    fn the_list_puts_pinned_notes_first_then_date_sections() {
        let lib = library();
        let rows = lib.rows(&Query::default(), NOW, 0);
        assert_eq!(
            ids(&lib, &rows),
            vec!["[Pinned]", "a", "[Today]", "b", "[Yesterday]", "c", "[August 2026]", "d"]
        );
        let by_title = Query {
            sort: SortKey::Title,
            descending: false,
            ..Query::default()
        };
        assert_eq!(
            ids(&lib, &lib.rows(&by_title, NOW, 0)),
            vec!["[Pinned]", "a", "[Notes]", "d", "c", "b"]
        );
        let oldest_first = Query {
            descending: false,
            ..Query::default()
        };
        assert_eq!(
            ids(&lib, &lib.rows(&oldest_first, NOW, 0)),
            vec!["[Pinned]", "a", "[August 2026]", "d", "[Yesterday]", "c", "[Today]", "b"]
        );
    }

    #[test]
    fn the_navigation_counts_notebooks_with_their_children_and_tags_ignoring_case() {
        let lib = library();
        assert_eq!(
            lib.notebook_paths(),
            vec!["Archive", "Personal", "Personal/Recipes", "Work", "Work/Meetings", "Work/Offsite"]
        );
        let tree = lib.notebook_tree();
        let names: Vec<(&str, usize)> = tree.iter().map(|n| (n.name.as_str(), n.count)).collect();
        assert_eq!(names, vec![("Archive", 0), ("Personal", 2), ("Work", 2)]);
        assert_eq!(tree[2].children.len(), 2);
        assert_eq!(lib.tags(), vec![("daily".to_string(), 1), ("planning".to_string(), 2)]);
        assert_eq!(
            lib.counts(),
            Counts {
                all: 4,
                pinned: 1,
                trash: 1
            }
        );
    }

    #[test]
    fn dates_read_as_a_person_would_say_them() {
        assert_eq!(short_date(NOW - 600, NOW, 0), "11:50");
        assert_eq!(short_date(NOW - DAY, NOW, 0), "Yesterday");
        assert_eq!(short_date(NOW - 3 * DAY, NOW, 0), "Sun");
        assert_eq!(short_date(NOW - 40 * DAY, NOW, 0), "Aug 21");
        assert_eq!(short_date(NOW - 400 * DAY, NOW, 0), "2025-08-26");
        assert_eq!(date_bucket(NOW - 10 * DAY, NOW, 0), "Previous 30 days");
        // 23:00 UTC yesterday is 01:00 today two hours east.
        assert_eq!(days_ago(NOW - 13 * 3600, NOW, 0), 1);
        assert_eq!(days_ago(NOW - 13 * 3600, NOW, 2 * 3600), 0);
    }

    #[test]
    fn a_note_moves_trashes_and_keeps_its_old_key_for_the_save() {
        let mut n = note("a", "Work", "x", NOW);
        n.saved = "---\n".to_string();
        n.move_to(&trash_path("Work"));
        assert!(n.is_trashed());
        assert_eq!(n.home_notebook(), "Work");
        assert_eq!(n.moved_from.as_deref(), Some("notes/Work/a.md"));
        assert_eq!(n.key(), "notes/.trash/Work/a.md");
        assert!(n.add_tag("#Ideas"));
        assert!(!n.add_tag("ideas"), "once, ignoring case");
        assert!(n.remove_tag("IDEAS"));
    }

    #[test]
    fn a_line_diff_keeps_common_lines_and_marks_the_rest() {
        let diff = line_diff("a\nb\nc\n", "a\nc\nd\n").expect("short texts compare");
        assert_eq!(
            diff,
            vec![
                (Change::Kept, "a".to_string()),
                (Change::Removed, "b".to_string()),
                (Change::Kept, "c".to_string()),
                (Change::Added, "d".to_string()),
            ]
        );
        assert_eq!(line_diff("", "x").expect("compares"), vec![(Change::Added, "x".to_string())]);
    }

    #[test]
    fn a_note_file_reads_back_into_the_same_note() {
        let mut n = note("a", "Work", "Offsite", NOW);
        n.meta.tags = vec!["work".to_string()];
        n.doc = RichTextDoc::create_from_markdown("- [ ] book the room\n");
        let file = n.to_file();
        let back = Note::from_file("notes/Work/a.md", &file, 0).expect("a note key");
        assert_eq!(back.meta, n.meta);
        assert_eq!(back.doc, n.doc);
        assert_eq!(status_text(&back), "3 words | 0 of 1 done");
    }
}
