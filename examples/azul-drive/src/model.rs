//! What AzDrive's folder view is, as plain data: Explorer's eight layouts,
//! the groups ("Group by"), the columns of the Details layout and their
//! widths, the multi-selection with its anchor and focus (click, Ctrl+click,
//! Shift+click, the arrow keys), type-ahead, and the settings that persist.
//! No azul types here, so all of it is tested without a window.

use chrono::{DateTime, Datelike, Local, TimeDelta, TimeZone};
use serde::{Deserialize, Serialize};

use crate::browse::{self, Column, Entry, Sort};

// ==== Layouts ====

/// Explorer's eight ways to draw a folder (View > Layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewLayout {
    ExtraLargeIcons,
    LargeIcons,
    MediumIcons,
    SmallIcons,
    List,
    Details,
    Tiles,
    Content,
}

impl ViewLayout {
    pub const ALL: [ViewLayout; 8] = [
        ViewLayout::ExtraLargeIcons,
        ViewLayout::LargeIcons,
        ViewLayout::MediumIcons,
        ViewLayout::SmallIcons,
        ViewLayout::List,
        ViewLayout::Details,
        ViewLayout::Tiles,
        ViewLayout::Content,
    ];

    /// The ribbon's label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ViewLayout::ExtraLargeIcons => "Extra large icons",
            ViewLayout::LargeIcons => "Large icons",
            ViewLayout::MediumIcons => "Medium icons",
            ViewLayout::SmallIcons => "Small icons",
            ViewLayout::List => "List",
            ViewLayout::Details => "Details",
            ViewLayout::Tiles => "Tiles",
            ViewLayout::Content => "Content",
        }
    }

    /// The name in the settings file and on the command line.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ViewLayout::ExtraLargeIcons => "extra_large_icons",
            ViewLayout::LargeIcons => "large_icons",
            ViewLayout::MediumIcons => "medium_icons",
            ViewLayout::SmallIcons => "small_icons",
            ViewLayout::List => "list",
            ViewLayout::Details => "details",
            ViewLayout::Tiles => "tiles",
            ViewLayout::Content => "content",
        }
    }

    /// The layout a name means (`large-icons` works too).
    #[must_use]
    pub fn from_name(name: &str) -> Option<ViewLayout> {
        let name = name.trim().to_ascii_lowercase().replace('-', "_");
        ViewLayout::ALL.into_iter().find(|l| l.name() == name)
    }

    /// The ribbon's icon (Material Icons).
    #[must_use]
    pub fn icon(self) -> &'static str {
        match self {
            ViewLayout::ExtraLargeIcons => "photo_size_select_actual",
            ViewLayout::LargeIcons => "photo_size_select_large",
            ViewLayout::MediumIcons => "photo_size_select_small",
            ViewLayout::SmallIcons => "apps",
            ViewLayout::List => "view_list",
            ViewLayout::Details => "view_headline",
            ViewLayout::Tiles => "view_module",
            ViewLayout::Content => "view_agenda",
        }
    }

    /// The size of an item's icon, in px.
    #[must_use]
    pub fn icon_px(self) -> f32 {
        match self {
            ViewLayout::ExtraLargeIcons => 112.0,
            ViewLayout::LargeIcons => 72.0,
            ViewLayout::MediumIcons => 44.0,
            ViewLayout::SmallIcons | ViewLayout::List | ViewLayout::Details => 18.0,
            ViewLayout::Tiles => 40.0,
            ViewLayout::Content => 32.0,
        }
    }

    /// Whether the items wrap in rows (the arrow keys move by a row up and
    /// down); Details and Content are one item per line.
    #[must_use]
    pub fn is_grid(self) -> bool {
        !matches!(self, ViewLayout::Details | ViewLayout::Content)
    }

    /// The width of one item of a grid, in px.
    #[must_use]
    pub fn cell_width(self) -> f32 {
        match self {
            ViewLayout::ExtraLargeIcons => 168.0,
            ViewLayout::LargeIcons => 120.0,
            ViewLayout::MediumIcons => 96.0,
            ViewLayout::SmallIcons => 200.0,
            ViewLayout::List => 240.0,
            ViewLayout::Tiles => 250.0,
            ViewLayout::Details | ViewLayout::Content => 0.0,
        }
    }

    /// Items per row of a grid `width` px wide (at least one).
    #[must_use]
    pub fn columns_in(self, width: f32) -> usize {
        let cell = self.cell_width();
        if cell <= 0.0 || width <= cell {
            1
        } else {
            (width / cell).floor() as usize
        }
    }
}

// ==== Selection ====

/// The selected items (their keys) with Explorer's anchor (where a
/// Shift+click range starts) and focus (the item the keyboard is on).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    keys: Vec<String>,
    anchor: Option<String>,
    focus: Option<String>,
}

/// The keys from `a` to `b` (either order) in `order`; `[b]` when `a` is
/// not there.
fn range(order: &[&str], a: &str, b: &str) -> Vec<String> {
    let Some(j) = order.iter().position(|k| *k == b) else {
        return Vec::new();
    };
    let i = order.iter().position(|k| *k == a).unwrap_or(j);
    let (from, to) = if i <= j { (i, j) } else { (j, i) };
    order[from..=to].iter().map(|k| (*k).to_string()).collect()
}

impl Selection {
    #[must_use]
    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.keys.iter().any(|k| k == key)
    }

    /// The item the keyboard is on.
    #[must_use]
    pub fn focus(&self) -> Option<&str> {
        self.focus.as_deref()
    }

    /// The one selected item, when exactly one is.
    #[must_use]
    pub fn single(&self) -> Option<&str> {
        match self.keys.as_slice() {
            [only] => Some(only.as_str()),
            _ => None,
        }
    }

    /// A plain click: only `key`, which is the new anchor and focus.
    pub fn click(&mut self, key: &str) {
        self.keys = vec![key.to_string()];
        self.anchor = Some(key.to_string());
        self.focus = Some(key.to_string());
    }

    /// Ctrl+click: `key` in or out; it is the new anchor and focus.
    pub fn toggle(&mut self, key: &str) {
        if self.contains(key) {
            self.keys.retain(|k| k != key);
        } else {
            self.keys.push(key.to_string());
        }
        self.anchor = Some(key.to_string());
        self.focus = Some(key.to_string());
    }

    /// Shift+click: the items from the anchor to `key`, nothing else; the
    /// anchor stays.
    pub fn extend(&mut self, key: &str, order: &[&str]) {
        let anchor = self.anchor.clone().unwrap_or_else(|| key.to_string());
        self.keys = range(order, &anchor, key);
        self.anchor = Some(anchor);
        self.focus = Some(key.to_string());
    }

    /// Ctrl+Shift+click: the items from the anchor to `key` added; the
    /// selection then follows the visible order.
    pub fn add_range(&mut self, key: &str, order: &[&str]) {
        let anchor = self.anchor.clone().unwrap_or_else(|| key.to_string());
        let added = range(order, &anchor, key);
        let mut keys: Vec<String> = order
            .iter()
            .filter(|k| self.contains(k) || added.iter().any(|a| a == *k))
            .map(|k| (*k).to_string())
            .collect();
        // Keys not in the order (none, normally) stay.
        for k in &self.keys {
            if !keys.contains(k) {
                keys.push(k.clone());
            }
        }
        self.keys = keys;
        self.anchor = Some(anchor);
        self.focus = Some(key.to_string());
    }

    /// Select all.
    pub fn select_all(&mut self, order: &[&str]) {
        self.keys = order.iter().map(|k| (*k).to_string()).collect();
        if self.focus.is_none() {
            self.focus = order.first().map(|k| (*k).to_string());
        }
    }

    /// Select none (the focus stays).
    pub fn clear(&mut self) {
        self.keys.clear();
    }

    /// Invert selection.
    pub fn invert(&mut self, order: &[&str]) {
        self.keys = order
            .iter()
            .filter(|k| !self.contains(k))
            .map(|k| (*k).to_string())
            .collect();
    }

    /// Keeps only the items still listed (after a listing or a search).
    pub fn retain(&mut self, order: &[&str]) {
        self.keys.retain(|k| order.contains(&k.as_str()));
        if self.focus.as_deref().is_some_and(|f| !order.contains(&f)) {
            self.focus = None;
        }
        if self.anchor.as_deref().is_some_and(|a| !order.contains(&a)) {
            self.anchor = None;
        }
    }

    /// Selects exactly `keys` (what a paste or a new folder made), the last
    /// one focused.
    pub fn set(&mut self, keys: Vec<String>) {
        self.anchor = keys.first().cloned();
        self.focus = keys.last().cloned();
        self.keys = keys;
    }

    /// An arrow key: the focus moves `delta` items in `order` (clamped);
    /// `extend` (Shift) selects from the anchor to it, `keep` (Ctrl) moves
    /// only the focus, otherwise the focused item is the selection. With
    /// nothing focused the first item (the last, going back) is the target.
    /// Returns the focused key.
    pub fn step(
        &mut self,
        order: &[&str],
        delta: isize,
        extend: bool,
        keep: bool,
    ) -> Option<String> {
        if order.is_empty() {
            return None;
        }
        let last = order.len() - 1;
        let current = self
            .focus
            .as_deref()
            .and_then(|f| order.iter().position(|k| *k == f));
        let target = match current {
            None if delta < 0 => last,
            None => 0,
            Some(i) => (i as isize).saturating_add(delta).clamp(0, last as isize) as usize,
        };
        let key = order[target].to_string();
        self.focus = Some(key.clone());
        if keep {
            return Some(key);
        }
        if extend {
            let anchor = self.anchor.clone().unwrap_or_else(|| key.clone());
            self.keys = range(order, &anchor, &key);
            self.anchor = Some(anchor);
        } else {
            self.keys = vec![key.clone()];
            self.anchor = Some(key.clone());
        }
        Some(key)
    }

    /// Ctrl+Space: the focused item in or out of the selection.
    pub fn toggle_focused(&mut self) {
        if let Some(key) = self.focus.clone() {
            self.toggle(&key);
        }
    }
}

// ==== Type-ahead ====

/// The letters typed in a row: a pause starts a new search.
#[derive(Debug, Clone, Default)]
pub struct TypeAhead {
    buffer: String,
    last_ms: u64,
}

impl TypeAhead {
    /// A pause longer than this starts a new search.
    pub const RESET_MS: u64 = 1000;

    /// Adds `c` typed at `now_ms`; returns what to search for.
    pub fn push(&mut self, c: char, now_ms: u64) -> String {
        if self.buffer.is_empty() || now_ms.saturating_sub(self.last_ms) > Self::RESET_MS {
            self.buffer.clear();
        }
        self.buffer.extend(c.to_lowercase());
        self.last_ms = now_ms;
        self.buffer.clone()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

/// The item type-ahead lands on: the first name starting with `query` from
/// the current item on (wrapping); one letter typed again and again moves
/// to the NEXT name starting with it. Case does not matter.
#[must_use]
pub fn type_ahead_match(names: &[&str], query: &str, current: Option<usize>) -> Option<usize> {
    let query = query.to_lowercase();
    let first = query.chars().next()?;
    let n = names.len();
    if n == 0 {
        return None;
    }
    let repeated = query.chars().all(|c| c == first);
    let (needle, start) = if repeated {
        (first.to_string(), current.map_or(0, |c| c + 1))
    } else {
        (query, current.unwrap_or(0))
    };
    (0..n)
        .map(|i| (start + i) % n)
        .find(|&i| names[i].to_lowercase().starts_with(&needle))
}

// ==== Groups ====

/// View > Group by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GroupBy {
    #[default]
    None,
    Name,
    Type,
    Size,
    Modified,
}

impl GroupBy {
    pub const ALL: [GroupBy; 5] = [
        GroupBy::None,
        GroupBy::Name,
        GroupBy::Type,
        GroupBy::Size,
        GroupBy::Modified,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            GroupBy::None => "(None)",
            GroupBy::Name => "Name",
            GroupBy::Type => "Type",
            GroupBy::Size => "Size",
            GroupBy::Modified => "Date modified",
        }
    }

    /// The group of `entry` - its place among the groups and its header -
    /// `now` (seconds since 1970) in the local zone for the date groups.
    #[must_use]
    pub fn group(self, entry: &Entry, now: i64) -> (u32, String) {
        match self {
            GroupBy::None => (0, String::new()),
            GroupBy::Name => {
                let first = entry
                    .name
                    .chars()
                    .next()
                    .map(|c| c.to_ascii_uppercase())
                    .unwrap_or(' ');
                let (order, label) = match first {
                    '0'..='9' => (0, "0 - 9"),
                    'A'..='H' => (1, "A - H"),
                    'I'..='P' => (2, "I - P"),
                    'Q'..='Z' => (3, "Q - Z"),
                    _ => (4, "Other"),
                };
                (order, label.to_string())
            }
            GroupBy::Type if entry.is_folder => (0, entry.kind()),
            GroupBy::Type => (1, entry.kind()),
            GroupBy::Size if entry.is_folder => (0, String::from("Folders")),
            GroupBy::Size => {
                const KB: u64 = 1024;
                const MB: u64 = 1024 * KB;
                const GB: u64 = 1024 * MB;
                let (order, label) = match entry.size {
                    None => (8, "Unspecified"),
                    Some(0) => (1, "Empty (0 KB)"),
                    Some(s) if s < 16 * KB => (2, "Tiny (0 - 16 KB)"),
                    Some(s) if s < MB => (3, "Small (16 KB - 1 MB)"),
                    Some(s) if s < 128 * MB => (4, "Medium (1 - 128 MB)"),
                    Some(s) if s < GB => (5, "Large (128 MB - 1 GB)"),
                    Some(s) if s < 4 * GB => (6, "Huge (1 - 4 GB)"),
                    Some(_) => (7, "Gigantic (> 4 GB)"),
                };
                (order, label.to_string())
            }
            GroupBy::Modified => {
                let now = Local
                    .timestamp_opt(now, 0)
                    .single()
                    .unwrap_or_else(Local::now);
                let (order, label) = date_group(entry.modified, &now);
                (order, label.to_string())
            }
        }
    }
}

/// Explorer's date groups, relative to `now`'s day in `now`'s zone: Today,
/// Yesterday, Earlier this week (weeks start on Monday), Last week, Earlier
/// this month, Last month, Earlier this year, A long time ago.
#[must_use]
pub fn date_group<Tz: TimeZone>(unix: Option<u64>, now: &DateTime<Tz>) -> (u32, &'static str) {
    const UNKNOWN: (u32, &str) = (8, "Unknown date");
    let Some(secs) = unix.and_then(|s| i64::try_from(s).ok()) else {
        return UNKNOWN;
    };
    let Some(then) = now.timezone().timestamp_opt(secs, 0).single() else {
        return UNKNOWN;
    };
    let today = now.date_naive();
    let day = then.date_naive();
    if day >= today {
        return (0, "Today");
    }
    if (today - day).num_days() == 1 {
        return (1, "Yesterday");
    }
    let week_start = today - TimeDelta::days(i64::from(today.weekday().num_days_from_monday()));
    if day >= week_start {
        return (2, "Earlier this week");
    }
    if day >= week_start - TimeDelta::days(7) {
        return (3, "Last week");
    }
    if day.year() == today.year() && day.month() == today.month() {
        return (4, "Earlier this month");
    }
    let (last_year, last_month) = if today.month() == 1 {
        (today.year() - 1, 12)
    } else {
        (today.year(), today.month() - 1)
    };
    if day.year() == last_year && day.month() == last_month {
        return (5, "Last month");
    }
    if day.year() == today.year() {
        return (6, "Earlier this year");
    }
    (7, "A long time ago")
}

/// One group of the view: its header and its items in the sorted order.
#[derive(Debug, Clone)]
pub struct Group<'a> {
    pub label: String,
    pub entries: Vec<&'a Entry>,
}

/// The items in groups, in the groups' order; inside a group the items keep
/// the order they came in (the sort). `GroupBy::None` is one group without
/// a header; no items, no groups.
#[must_use]
pub fn group_entries<'a>(entries: &[&'a Entry], by: GroupBy, now: i64) -> Vec<Group<'a>> {
    if entries.is_empty() {
        return Vec::new();
    }
    if by == GroupBy::None {
        return vec![Group {
            label: String::new(),
            entries: entries.to_vec(),
        }];
    }
    let mut groups: Vec<(u32, String, Vec<&'a Entry>)> = Vec::new();
    for &entry in entries {
        let (order, label) = by.group(entry, now);
        match groups.iter_mut().find(|g| g.1 == label) {
            Some(group) => group.2.push(entry),
            None => groups.push((order, label, vec![entry])),
        }
    }
    groups.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    groups
        .into_iter()
        .map(|(_, label, entries)| Group { label, entries })
        .collect()
}

// ==== Columns ====

/// A visible column of the Details layout and its width.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColumnWidth {
    pub column: Column,
    pub width: f32,
}

/// The Details layout's columns, in order, with their widths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnLayout {
    pub columns: Vec<ColumnWidth>,
}

/// A column's width before anyone resized it.
#[must_use]
pub fn default_width(column: Column) -> f32 {
    match column {
        Column::Name => 280.0,
        Column::Modified => 150.0,
        Column::Type => 150.0,
        Column::Size => 90.0,
        Column::Path => 220.0,
        Column::Tag => 220.0,
    }
}

impl Default for ColumnLayout {
    fn default() -> Self {
        ColumnLayout {
            columns: [Column::Name, Column::Modified, Column::Type, Column::Size]
                .into_iter()
                .map(|column| ColumnWidth {
                    column,
                    width: default_width(column),
                })
                .collect(),
        }
    }
}

/// What a cell of the Details layout says.
#[must_use]
pub fn cell_text(entry: &Entry, column: Column, show_extensions: bool) -> String {
    match column {
        Column::Name => entry.display_name(show_extensions),
        Column::Modified => browse::format_modified(entry.modified, &Local),
        Column::Type => entry.kind(),
        Column::Size => browse::format_size(entry.size),
        Column::Path => entry.key.clone(),
        Column::Tag => entry.etag.clone().unwrap_or_default(),
    }
}

impl ColumnLayout {
    /// No column gets narrower than this.
    pub const MIN_WIDTH: f32 = 40.0;
    /// "Size all columns to fit" never makes one wider than this.
    pub const MAX_FIT_WIDTH: f32 = 640.0;

    #[must_use]
    pub fn visible(&self) -> Vec<Column> {
        self.columns.iter().map(|c| c.column).collect()
    }

    #[must_use]
    pub fn is_visible(&self, column: Column) -> bool {
        self.columns.iter().any(|c| c.column == column)
    }

    #[must_use]
    pub fn width(&self, column: Column) -> f32 {
        self.columns
            .iter()
            .find(|c| c.column == column)
            .map_or_else(|| default_width(column), |c| c.width)
    }

    /// The sum of the visible columns' widths.
    #[must_use]
    pub fn total_width(&self) -> f32 {
        self.columns.iter().map(|c| c.width).sum()
    }

    /// "Add columns": shows a hidden column (at the end) or hides a shown
    /// one; Name always stays.
    pub fn toggle(&mut self, column: Column) {
        if column == Column::Name {
            return;
        }
        if self.is_visible(column) {
            self.columns.retain(|c| c.column != column);
        } else {
            self.columns.push(ColumnWidth {
                column,
                width: default_width(column),
            });
        }
    }

    /// A header's edge was dragged: the column's new width.
    pub fn resize(&mut self, column: Column, width: f32) {
        if let Some(c) = self.columns.iter_mut().find(|c| c.column == column) {
            c.width = width.max(Self::MIN_WIDTH);
        }
    }

    /// "Size all columns to fit": every column as wide as its widest text
    /// (the header's too).
    pub fn fit(&mut self, entries: &[&Entry], show_extensions: bool) {
        const CHAR_PX: f32 = 7.0;
        const PADDING_PX: f32 = 24.0;
        const ICON_PX: f32 = 24.0;
        for c in &mut self.columns {
            let longest = entries
                .iter()
                .map(|e| cell_text(e, c.column, show_extensions).chars().count())
                .max()
                .unwrap_or(0)
                .max(c.column.label().chars().count());
            let icon = if c.column == Column::Name {
                ICON_PX
            } else {
                0.0
            };
            c.width = (longest as f32 * CHAR_PX + PADDING_PX + icon)
                .clamp(Self::MIN_WIDTH, Self::MAX_FIT_WIDTH);
        }
    }
}

// ==== Settings ====

/// A pinned folder of Quick access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pinned {
    pub drive: String,
    pub prefix: String,
    /// What Quick access calls it (the folder's name, or the drive's).
    pub name: String,
}

/// Where a new window opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartPlace {
    QuickAccess,
    ThisPc,
}

/// What AzDrive remembers between runs (`<config dir>/azul-drive/settings.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub layout: ViewLayout,
    pub group_by: GroupBy,
    pub sort: Sort,
    pub show_hidden: bool,
    pub show_extensions: bool,
    pub item_checkboxes: bool,
    pub navigation_pane: bool,
    pub preview_pane: bool,
    pub details_pane: bool,
    pub columns: ColumnLayout,
    pub pinned: Vec<Pinned>,
    /// Ask before a delete that cannot be undone (a cloud drive, Shift+Delete).
    pub confirm_delete: bool,
    pub start: StartPlace,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            layout: ViewLayout::Details,
            group_by: GroupBy::None,
            sort: Sort::default(),
            show_hidden: false,
            show_extensions: true,
            item_checkboxes: false,
            navigation_pane: true,
            preview_pane: false,
            details_pane: true,
            columns: ColumnLayout::default(),
            pinned: Vec::new(),
            confirm_delete: true,
            start: StartPlace::ThisPc,
        }
    }
}

impl Settings {
    /// The settings file's text.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// The settings a file says; what it leaves out (or a broken file) keeps
    /// the defaults.
    #[must_use]
    pub fn from_json(text: &str) -> Settings {
        serde_json::from_str(text).unwrap_or_default()
    }

    /// Whether `drive` / `prefix` is pinned to Quick access.
    #[must_use]
    pub fn is_pinned(&self, drive: &str, prefix: &str) -> bool {
        self.pinned
            .iter()
            .any(|p| p.drive == drive && p.prefix == prefix)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, TimeZone};

    use super::*;
    use crate::browse::{Column, Entry, Sort};

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

    fn keys(order: &[&str]) -> Vec<String> {
        order.iter().map(|s| s.to_string()).collect()
    }

    const ORDER: [&str; 5] = ["a", "b", "c", "d", "e"];

    #[test]
    fn explorer_has_eight_layouts_and_each_reads_back_from_its_name() {
        assert_eq!(ViewLayout::ALL.len(), 8);
        let labels: Vec<&str> = ViewLayout::ALL.iter().map(|l| l.label()).collect();
        assert_eq!(
            labels,
            vec![
                "Extra large icons",
                "Large icons",
                "Medium icons",
                "Small icons",
                "List",
                "Details",
                "Tiles",
                "Content"
            ]
        );
        for layout in ViewLayout::ALL {
            assert_eq!(ViewLayout::from_name(layout.name()), Some(layout));
        }
        assert_eq!(ViewLayout::from_name("nonsense"), None);
        assert!(ViewLayout::ExtraLargeIcons.icon_px() > ViewLayout::LargeIcons.icon_px());
        assert!(ViewLayout::LargeIcons.icon_px() > ViewLayout::MediumIcons.icon_px());
        assert!(ViewLayout::MediumIcons.icon_px() > ViewLayout::SmallIcons.icon_px());
        assert!(ViewLayout::LargeIcons.is_grid() && ViewLayout::Tiles.is_grid());
        assert!(!ViewLayout::Details.is_grid() && !ViewLayout::Content.is_grid());
    }

    #[test]
    fn a_plain_click_selects_one_item_and_moves_the_anchor() {
        let mut sel = Selection::default();
        sel.click("b");
        assert_eq!(sel.keys(), keys(&["b"]).as_slice());
        assert_eq!(sel.focus(), Some("b"));
        sel.click("d");
        assert_eq!(sel.keys(), keys(&["d"]).as_slice());
        assert_eq!(sel.single(), Some("d"));
    }

    #[test]
    fn ctrl_click_toggles_and_shift_click_selects_the_range_from_the_anchor() {
        let mut sel = Selection::default();
        sel.click("b");
        sel.toggle("d");
        assert_eq!(sel.keys(), keys(&["b", "d"]).as_slice());
        assert_eq!(sel.single(), None);
        sel.toggle("b");
        assert_eq!(sel.keys(), keys(&["d"]).as_slice());
        // The anchor is the last Ctrl+clicked item, b - also when the click
        // took it out (Explorer). Shift+click on a selects a..b.
        sel.extend("a", &ORDER);
        assert_eq!(sel.keys(), keys(&["a", "b"]).as_slice());
        // A second Shift+click re-ranges from the same anchor.
        sel.extend("e", &ORDER);
        assert_eq!(sel.keys(), keys(&["b", "c", "d", "e"]).as_slice());
        // Ctrl+Shift+click adds the range to what is selected.
        sel.click("a");
        sel.add_range("b", &ORDER);
        sel.toggle("e");
        // The Ctrl+click on e made e the anchor: the range is d..e.
        sel.add_range("d", &ORDER);
        assert_eq!(sel.keys(), keys(&["a", "b", "d", "e"]).as_slice());
    }

    #[test]
    fn select_all_none_and_invert_work_over_the_visible_order() {
        let mut sel = Selection::default();
        sel.select_all(&ORDER);
        assert_eq!(sel.len(), 5);
        sel.clear();
        assert!(sel.is_empty());
        sel.click("b");
        sel.toggle("c");
        sel.invert(&ORDER);
        assert_eq!(sel.keys(), keys(&["a", "d", "e"]).as_slice());
        // Items that left the listing leave the selection.
        sel.retain(&["a", "e"]);
        assert_eq!(sel.keys(), keys(&["a", "e"]).as_slice());
    }

    #[test]
    fn the_arrow_keys_move_the_focus_and_shift_extends_ctrl_only_moves() {
        let mut sel = Selection::default();
        // Nothing focused: the first step lands on the first item.
        assert_eq!(sel.step(&ORDER, 1, false, false), Some("a".to_string()));
        assert_eq!(sel.keys(), keys(&["a"]).as_slice());
        sel.step(&ORDER, 1, false, false);
        assert_eq!(sel.keys(), keys(&["b"]).as_slice());
        sel.step(&ORDER, 2, true, false);
        assert_eq!(sel.keys(), keys(&["b", "c", "d"]).as_slice());
        // Ctrl moves the focus without touching the selection.
        sel.step(&ORDER, 1, false, true);
        assert_eq!(sel.focus(), Some("e"));
        assert_eq!(sel.keys(), keys(&["b", "c", "d"]).as_slice());
        // Clamped at the ends (Home / End are big steps).
        sel.step(&ORDER, isize::MIN / 2, false, false);
        assert_eq!(sel.keys(), keys(&["a"]).as_slice());
        sel.step(&ORDER, isize::MAX / 2, false, false);
        assert_eq!(sel.keys(), keys(&["e"]).as_slice());
        assert_eq!(Selection::default().step(&[], 1, false, false), None);
    }

    #[test]
    fn type_ahead_finds_the_first_name_starting_with_what_was_typed() {
        let names = ["Apple", "apricot", "Banana", "blueberry", "cherry"];
        let mut ta = TypeAhead::default();
        assert_eq!(ta.push('a', 1_000), "a");
        assert_eq!(type_ahead_match(&names, "a", None), Some(0));
        // A quick second key extends the prefix.
        assert_eq!(ta.push('p', 1_300), "ap");
        assert_eq!(type_ahead_match(&names, "apr", Some(0)), Some(1));
        // After a pause the buffer starts again.
        assert_eq!(ta.push('b', 3_000), "b");
        // Repeating one letter cycles through the names starting with it.
        assert_eq!(type_ahead_match(&names, "b", Some(2)), Some(3));
        assert_eq!(type_ahead_match(&names, "bb", Some(3)), Some(2));
        assert_eq!(type_ahead_match(&names, "zz", Some(0)), None);
    }

    #[test]
    fn group_by_name_size_type_and_date_puts_items_into_explorers_groups() {
        assert_eq!(GroupBy::Name.group(&file("apple.txt", 1, 0), 0).1, "A - H");
        assert_eq!(GroupBy::Name.group(&file("Quince", 1, 0), 0).1, "Q - Z");
        assert_eq!(GroupBy::Name.group(&file("7up", 1, 0), 0).1, "0 - 9");
        assert_eq!(GroupBy::Name.group(&file("_x", 1, 0), 0).1, "Other");
        assert_eq!(GroupBy::Size.group(&file("e", 0, 0), 0).1, "Empty (0 KB)");
        assert_eq!(
            GroupBy::Size.group(&file("t", 10_000, 0), 0).1,
            "Tiny (0 - 16 KB)"
        );
        assert_eq!(
            GroupBy::Size.group(&file("s", 500_000, 0), 0).1,
            "Small (16 KB - 1 MB)"
        );
        assert_eq!(
            GroupBy::Size.group(&file("m", 50 * 1024 * 1024, 0), 0).1,
            "Medium (1 - 128 MB)"
        );
        assert_eq!(GroupBy::Size.group(&folder("x"), 0).1, "Folders");
        assert_eq!(GroupBy::Type.group(&folder("x"), 0).1, "File folder");
        assert_eq!(
            GroupBy::Type.group(&file("a.txt", 1, 0), 0).1,
            "Text Document"
        );
        // Folders come first in every grouping by size or type.
        assert!(
            GroupBy::Size.group(&folder("x"), 0).0 < GroupBy::Size.group(&file("e", 0, 0), 0).0
        );
    }

    #[test]
    fn dates_group_relative_to_today_in_the_local_zone() {
        let zone = FixedOffset::east_opt(0).unwrap();
        // Wednesday 2026-09-30 12:00 UTC.
        let now = zone.with_ymd_and_hms(2026, 9, 30, 12, 0, 0).unwrap();
        let at = |y, m, d| zone.with_ymd_and_hms(y, m, d, 9, 0, 0).unwrap().timestamp() as u64;
        let label = |unix: Option<u64>| date_group(unix, &now).1;
        assert_eq!(label(Some(at(2026, 9, 30))), "Today");
        assert_eq!(label(Some(at(2026, 9, 29))), "Yesterday");
        assert_eq!(label(Some(at(2026, 9, 28))), "Earlier this week");
        assert_eq!(label(Some(at(2026, 9, 22))), "Last week");
        assert_eq!(label(Some(at(2026, 9, 2))), "Earlier this month");
        assert_eq!(label(Some(at(2026, 8, 15))), "Last month");
        assert_eq!(label(Some(at(2026, 2, 1))), "Earlier this year");
        assert_eq!(label(Some(at(2019, 2, 1))), "A long time ago");
        assert_eq!(label(None), "Unknown date");
        assert!(
            date_group(Some(at(2026, 9, 30)), &now).0 < date_group(Some(at(2019, 2, 1)), &now).0
        );
    }

    #[test]
    fn grouping_keeps_the_sorted_order_inside_each_group() {
        let entries = vec![
            folder("zeta"),
            file("apple", 1, 0),
            file("Zoo", 1, 0),
            file("avocado", 1, 0),
        ];
        let refs: Vec<&Entry> = entries.iter().collect();
        let groups = group_entries(&refs, GroupBy::Name, 0);
        let shown: Vec<(String, Vec<&str>)> = groups
            .iter()
            .map(|g| {
                (
                    g.label.clone(),
                    g.entries.iter().map(|e| e.name.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            vec![
                ("A - H".to_string(), vec!["apple", "avocado"]),
                ("Q - Z".to_string(), vec!["zeta", "Zoo"]),
            ]
        );
        assert_eq!(group_entries(&refs, GroupBy::None, 0).len(), 1);
        assert!(group_entries(&[], GroupBy::Name, 0).is_empty());
    }

    #[test]
    fn columns_can_be_added_resized_and_sized_to_fit() {
        let mut columns = ColumnLayout::default();
        assert_eq!(
            columns.visible(),
            vec![Column::Name, Column::Modified, Column::Type, Column::Size]
        );
        columns.toggle(Column::Path);
        assert!(columns.visible().contains(&Column::Path));
        columns.toggle(Column::Path);
        assert!(!columns.visible().contains(&Column::Path));
        columns.toggle(Column::Name);
        assert!(columns.visible().contains(&Column::Name), "Name never goes");
        columns.resize(Column::Size, 10.0);
        assert_eq!(columns.width(Column::Size), ColumnLayout::MIN_WIDTH);
        columns.resize(Column::Size, 150.0);
        assert_eq!(columns.width(Column::Size), 150.0);
        let long = file("a-very-long-file-name-that-needs-room.txt", 1, 0);
        columns.fit(&[&long], true);
        assert!(
            columns.width(Column::Name) > 250.0,
            "{}",
            columns.width(Column::Name)
        );
        assert!(columns.width(Column::Size) < 150.0, "fits the short sizes");
    }

    #[test]
    fn settings_round_trip_through_json_and_unknown_fields_fall_back_to_defaults() {
        let mut settings = Settings::default();
        assert_eq!(settings.layout, ViewLayout::Details);
        assert!(settings.navigation_pane && !settings.preview_pane);
        settings.layout = ViewLayout::LargeIcons;
        settings.group_by = GroupBy::Type;
        settings.sort = Sort {
            column: Column::Size,
            descending: true,
        };
        settings.show_hidden = true;
        settings.pinned.push(Pinned {
            drive: "home".to_string(),
            prefix: "docs/".to_string(),
            name: "docs".to_string(),
        });
        let text = settings.to_json();
        assert_eq!(Settings::from_json(&text), settings);
        let partial = Settings::from_json(r#"{"layout":"tiles","show_extensions":false}"#);
        assert_eq!(partial.layout, ViewLayout::Tiles);
        assert!(!partial.show_extensions);
        assert_eq!(partial.group_by, GroupBy::None);
        assert_eq!(Settings::from_json("not json"), Settings::default());
    }
}
