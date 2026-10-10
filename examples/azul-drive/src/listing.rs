//! The open folder's listing as it streams in, and the window of it the views build - as plain
//! data, so all of it is tested without a window.
//!
//! A folder opens at once whatever its size: the scan of a folder on this computer reads the
//! names and kinds only (one `read_dir`, no stat per entry) and hands them over in batches - the
//! first batch as soon as it holds a screen of rows, then one every [`BATCH_MS`] - while a
//! bucket's listing hands over each page as it comes. Each batch is merged into the rows, which
//! stay in the view's sort order ([`merge_batch`]: linear, never a re-sort of the folder). The
//! views are virtual: they build the [`Line`]s in view and a screen either side
//! ([`lines_window`]); the items of the lines in view are the only ones stat'ed for their size
//! and date ([`stats_wanted`]) and the only pictures that get thumbnails. A sort by Size or Date
//! modified needs every item's stat: then all of them are asked for ([`unknown_keys`]) and the
//! rows sorted again once they are in.

use std::{cmp::Ordering, collections::HashSet, ops::Range};

use crate::browse::{self, Column, Entry, Sort};

/// The first batch of a scan holds this many rows: a screen of the Details layout and more, so
/// the folder shows at once.
pub const FIRST_BATCH: usize = 256;

/// After the first batch, a scan gathers this long before it hands over the next one: every
/// batch rebuilds the window, so a folder of 100,000 names arrives in a handful of batches.
pub const BATCH_MS: u64 = 120;

/// The most items one stat job looks up (the rows in view, a screen either side, are far
/// fewer; a sort by size asks for the rest in jobs of this size).
pub const STAT_MAX: usize = 2048;

/// The height of a group's header line in a virtual view (px).
pub const HEADER_PX: f32 = 30.0;

/// Whether a scan lists the directory entry `name`: the storage crate's half-written temporary
/// files (`.<name>.azul-storage-<pid>-<n>.tmp`) never, and at a drive's root not its own
/// bookkeeping (`.azlin`) - what `LocalDrive::list` leaves out too.
#[must_use]
pub fn listed_name(name: &str, at_root: bool) -> bool {
    let temporary =
        name.starts_with('.') && name.contains(".azul-storage-") && name.ends_with(".tmp");
    !temporary && !(at_root && name == azul_storage::manifest::MANIFEST_DIR)
}

/// The row of a directory entry the scan read: its key under `prefix`, its name, whether it is
/// a folder - its size and date unknown until a stat of the rows in view.
#[must_use]
pub fn scanned_entry(prefix: &str, name: &str, is_folder: bool) -> Entry {
    Entry {
        key: if is_folder {
            format!("{prefix}{name}/")
        } else {
            format!("{prefix}{name}")
        },
        name: name.to_string(),
        is_folder,
        size: None,
        modified: None,
        etag: None,
        known: false,
    }
}

/// Merges `batch` into `rows`, which are in `sort` order, keeping them so: the batch is sorted,
/// then the two runs are merged - linear in the rows, so a folder streamed in batches costs what
/// one sort of it costs, not a sort per batch. A batch that sorts after every row (a bucket's
/// pages come in key order) is appended. Equal rows keep the earlier one first.
pub fn merge_batch(rows: &mut Vec<Entry>, mut batch: Vec<Entry>, sort: Sort) {
    if batch.is_empty() {
        return;
    }
    browse::sort_entries(&mut batch, sort);
    let after_every_row = match (rows.last(), batch.first()) {
        (Some(last), Some(first)) => {
            browse::compare_entries(last, first, sort) != Ordering::Greater
        }
        _ => true,
    };
    if after_every_row {
        rows.extend(batch);
        return;
    }
    let old = std::mem::take(rows);
    rows.reserve(old.len() + batch.len());
    let mut a = old.into_iter().peekable();
    let mut b = batch.into_iter().peekable();
    loop {
        let from_old = match (a.peek(), b.peek()) {
            (Some(x), Some(y)) => browse::compare_entries(x, y, sort) != Ordering::Greater,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        let next = if from_old { a.next() } else { b.next() };
        if let Some(entry) = next {
            rows.push(entry);
        }
    }
}

/// One line of a virtual folder view: a group's header, or the items at positions
/// `start..end` of the shown order (one item in Details and Content, a row of cells in the
/// grids).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    Header { group: usize },
    Items { start: usize, end: usize },
}

/// The lines of a view whose shown order is `group_sizes` groups (their item counts, one after
/// the other), `columns` items to a line. Grouped, every group starts with its header and a
/// closed group (`open[g] == false`) shows nothing but its header; ungrouped (one group) there
/// are no headers.
#[must_use]
pub fn lines_of(group_sizes: &[usize], open: &[bool], columns: usize, grouped: bool) -> Vec<Line> {
    let columns = columns.max(1);
    let mut lines = Vec::new();
    let mut at = 0;
    for (group, &size) in group_sizes.iter().enumerate() {
        if grouped {
            lines.push(Line::Header { group });
        }
        let shown = !grouped || open.get(group).copied().unwrap_or(true);
        if shown {
            let mut start = at;
            while start < at + size {
                // Saturating: `columns` may be usize::MAX (an infinite width).
                let end = start.saturating_add(columns).min(at + size);
                lines.push(Line::Items { start, end });
                start = end;
            }
        }
        at += size;
    }
    lines
}

/// Where each line starts (px from the top) and how tall all of them are, a header
/// [`HEADER_PX`] high and a line of items `line_px`.
#[must_use]
pub fn line_tops(lines: &[Line], line_px: f32) -> (Vec<f32>, f32) {
    let mut tops = Vec::with_capacity(lines.len());
    let mut y = 0.0;
    for line in lines {
        tops.push(y);
        y += match line {
            Line::Header { .. } => HEADER_PX,
            Line::Items { .. } => line_px,
        };
    }
    (tops, y)
}

/// The first line that ends below `y` (the one `y` falls in), by the lines' tops.
fn line_at(tops: &[f32], total: f32, y: f32) -> usize {
    if tops.is_empty() {
        return 0;
    }
    let y = y.clamp(0.0, total);
    // The last top at or above `y`.
    match tops.binary_search_by(|top| top.partial_cmp(&y).unwrap_or(Ordering::Less)) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    }
}

/// The lines a view `viewport` px high scrolled `scroll_y` down shows: those in view (the one
/// cut at the bottom too). Empty for no lines or no height.
#[must_use]
pub fn lines_in_view(tops: &[f32], total: f32, scroll_y: f32, viewport: f32) -> Range<usize> {
    if tops.is_empty() || !(viewport > 0.0) || !scroll_y.is_finite() {
        return 0..0;
    }
    // An offset kept from a longer listing may lie past this one's end: the last screen.
    let scroll_y = scroll_y.clamp(0.0, (total - viewport).max(0.0));
    let first = line_at(tops, total, scroll_y);
    let mut end = first;
    while end < tops.len() && tops[end] < scroll_y + viewport {
        end += 1;
    }
    first..end.max((first + 1).min(tops.len()))
}

/// The lines a virtual view builds: the ones in view and a screen either side, so a scroll of
/// up to a screen shows rows that are built already.
#[must_use]
pub fn lines_window(tops: &[f32], total: f32, scroll_y: f32, viewport: f32) -> Range<usize> {
    let seen = lines_in_view(tops, total, scroll_y, viewport);
    if seen.is_empty() {
        return seen;
    }
    let screen = seen.len();
    let first = seen.start.saturating_sub(screen);
    let end = (seen.end + screen).min(tops.len());
    first..end
}

/// The items of `items` at positions `range`, the range cut to what `items` holds. A view's
/// range is what it has room for - a grid of twelve cells over a folder of one picture - and
/// `slice::get` answers `None` for a range that runs past the end, which read as "nothing in
/// view": a folder smaller than its view never had its sizes, counts or thumbnails asked for.
#[must_use]
pub fn in_view<T>(items: &[T], range: Range<usize>) -> &[T] {
    let end = range.end.min(items.len());
    let start = range.start.min(end);
    &items[start..end]
}

/// The item positions the lines `range` of `lines` hold.
#[must_use]
pub fn positions_of(lines: &[Line], range: Range<usize>) -> Range<usize> {
    let mut start = usize::MAX;
    let mut end = 0;
    for line in in_view(lines, range) {
        if let Line::Items { start: s, end: e } = *line {
            start = start.min(s);
            end = end.max(e);
        }
    }
    if start == usize::MAX {
        0..0
    } else {
        start..end
    }
}

/// The keys of the items at positions `range` of `shown` whose size and date are unknown and not
/// asked for yet, at most `max` - what a stat job of the rows in view looks up.
#[must_use]
pub fn stats_wanted(
    shown: &[&Entry],
    range: Range<usize>,
    asked: &HashSet<String>,
    max: usize,
) -> Vec<String> {
    in_view(shown, range)
        .iter()
        .filter(|e| !e.known && !asked.contains(&e.key))
        .take(max)
        .map(|e| e.key.clone())
        .collect()
}

/// The keys of every row whose size and date are unknown and not asked for yet, at most `max`
/// (what a sort by Size or Date modified waits for).
#[must_use]
pub fn unknown_keys(rows: &[Entry], asked: &HashSet<String>, max: usize) -> Vec<String> {
    rows.iter()
        .filter(|e| !e.known && !asked.contains(&e.key))
        .take(max)
        .map(|e| e.key.clone())
        .collect()
}

/// Whether every row's size and date are known.
#[must_use]
pub fn all_known(rows: &[Entry]) -> bool {
    rows.iter().all(|e| e.known)
}

/// Whether the order `sort` needs every item's size or date: a folder's names and kinds are
/// all a scan reads, its sizes and dates only the stat of the rows in view.
#[must_use]
pub fn sort_needs_stats(sort: Sort) -> bool {
    matches!(sort.column, Column::Size | Column::Modified)
}

/// One item's size and date as a stat found them (`None`: a folder's size, or a date the file
/// system does not keep).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stat {
    pub key: String,
    pub size: Option<u64>,
    pub modified: Option<u64>,
}

/// Writes `stats` into the rows they name (a row gone since - another folder, a refresh - is not
/// there to write); the rows are known from then on. How many rows changed.
pub fn apply_stats(rows: &mut [Entry], stats: &[Stat]) -> usize {
    if stats.is_empty() {
        return 0;
    }
    let by_key: std::collections::HashMap<&str, &Stat> =
        stats.iter().map(|s| (s.key.as_str(), s)).collect();
    let mut changed = 0;
    for row in rows.iter_mut() {
        if let Some(stat) = by_key.get(row.key.as_str()) {
            if !row.is_folder {
                row.size = stat.size;
            }
            row.modified = stat.modified;
            row.known = true;
            changed += 1;
        }
    }
    changed
}

/// The rows a refresh read again (`fresh`, every one unknown) keep what the old rows knew - a
/// size, a date - until their own stats are in. They stay unknown, so the rows in view are
/// stat'ed again and a sort by Size waits for the new sizes: F5 never blanks the Size and Date
/// columns while it reads. (A row's key says its kind: a folder's ends with `/`.)
pub fn carry_stats(old: &[Entry], fresh: &mut [Entry]) {
    if old.is_empty() {
        return;
    }
    let by_key: std::collections::HashMap<&str, &Entry> = old
        .iter()
        .filter(|e| e.known)
        .map(|e| (e.key.as_str(), e))
        .collect();
    for row in fresh.iter_mut().filter(|r| !r.known) {
        if let Some(was) = by_key.get(row.key.as_str()) {
            row.size = was.size;
            row.modified = was.modified;
        }
    }
}

/// What the status line says about a listing of `count` items: "12,345 items", while the scan
/// still runs "12,345 items so far" (`$n` the count grouped in the window's language).
#[must_use]
pub fn count_text(count: usize, done: bool) -> azul_appkit::l10n::Phrase {
    let said = if done {
        "azdrive-status-items"
    } else {
        "azdrive-status-items-so-far"
    };
    azul_appkit::l10n::Phrase::new(said)
        .arg("count", count)
        .arg("n", azul_appkit::l10n::grouped(count as u64))
}

/// `12345` as `12,345` (the status line counts big folders).
#[must_use]
pub fn grouped_digits(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_view_with_room_for_more_than_its_folder_holds_still_holds_the_folders_items() {
        let items = [1, 2, 3];
        assert_eq!(in_view(&items, 0..12), &[1, 2, 3], "a grid of twelve over three items");
        assert_eq!(in_view(&items, 2..12), &[3]);
        assert!(in_view(&items, 5..12).is_empty(), "scrolled past the end");
        assert_eq!(in_view(&items, 1..2), &[2]);
        let entries = [scanned_entry("", "a.txt", false), scanned_entry("", "b.txt", false)];
        let shown: Vec<&Entry> = entries.iter().collect();
        assert_eq!(
            stats_wanted(&shown, 0..40, &HashSet::new(), 10).len(),
            2,
            "a folder smaller than its view has its sizes asked for"
        );
    }
    use super::*;

    fn file(name: &str) -> Entry {
        scanned_entry("f/", name, false)
    }

    fn folder(name: &str) -> Entry {
        scanned_entry("f/", name, true)
    }

    fn names(rows: &[Entry]) -> Vec<&str> {
        rows.iter().map(|e| e.name.as_str()).collect()
    }

    /// Batches arrive in the order the directory gives (any order); the rows stay sorted as a
    /// sort of the whole folder sorts them, folders first, after every batch.
    #[test]
    fn batches_merge_into_the_sorted_rows_as_a_whole_sort_would_order_them() {
        let sort = Sort::default();
        let mut rows = Vec::new();
        merge_batch(
            &mut rows,
            vec![file("m.txt"), folder("zeta"), file("B.txt")],
            sort,
        );
        assert_eq!(names(&rows), vec!["zeta", "B.txt", "m.txt"]);
        merge_batch(
            &mut rows,
            vec![file("a.txt"), folder("alpha"), file("z.txt")],
            sort,
        );
        assert_eq!(
            names(&rows),
            vec!["alpha", "zeta", "a.txt", "B.txt", "m.txt", "z.txt"]
        );
        let mut whole = rows.clone();
        browse::sort_entries(&mut whole, sort);
        assert_eq!(rows, whole, "the merge ends where one sort ends");
        merge_batch(&mut rows, Vec::new(), sort);
        assert_eq!(rows.len(), 6, "an empty batch changes nothing");
    }

    /// A bucket's pages come in key order: a batch that sorts after every row is appended, and
    /// the order is still the sort's.
    #[test]
    fn a_batch_after_every_row_is_appended() {
        let sort = Sort::default();
        let mut rows = vec![file("a.txt"), file("b.txt")];
        merge_batch(&mut rows, vec![file("d.txt"), file("c.txt")], sort);
        assert_eq!(names(&rows), vec!["a.txt", "b.txt", "c.txt", "d.txt"]);
    }

    /// Descending order merges descending.
    #[test]
    fn a_descending_sort_merges_descending() {
        let sort = Sort {
            column: Column::Name,
            descending: true,
        };
        let mut rows = Vec::new();
        merge_batch(&mut rows, vec![file("a"), file("c")], sort);
        merge_batch(&mut rows, vec![file("b"), file("d")], sort);
        assert_eq!(names(&rows), vec!["d", "c", "b", "a"]);
    }

    /// A scan leaves out what `LocalDrive::list` leaves out: half-written temporary files, and
    /// the drive's own bookkeeping at its root (a folder of that name deeper down is content).
    #[test]
    fn a_scan_lists_what_the_drive_lists() {
        assert!(listed_name("notes.txt", true));
        assert!(
            listed_name(".hidden", true),
            "a hidden item is listed (and filtered by the view)"
        );
        assert!(!listed_name(".notes.txt.azul-storage-77-3.tmp", false));
        assert!(!listed_name(".azlin", true));
        assert!(listed_name(".azlin", false));
        let entry = scanned_entry("docs/", "inbox", true);
        assert_eq!(entry.key, "docs/inbox/");
        assert!(entry.is_folder && !entry.known && entry.size.is_none());
        assert_eq!(scanned_entry("", "a.txt", false).key, "a.txt");
    }

    /// Ungrouped, the lines are the items `columns` at a time; grouped, every group starts with
    /// its header, and a closed group keeps only its header.
    #[test]
    fn the_lines_are_the_items_a_line_at_a_time_under_their_group_headers() {
        assert_eq!(
            lines_of(&[5], &[true], 2, false),
            vec![
                Line::Items { start: 0, end: 2 },
                Line::Items { start: 2, end: 4 },
                Line::Items { start: 4, end: 5 },
            ]
        );
        assert_eq!(
            lines_of(&[2, 3], &[true, false], 1, true),
            vec![
                Line::Header { group: 0 },
                Line::Items { start: 0, end: 1 },
                Line::Items { start: 1, end: 2 },
                Line::Header { group: 1 },
            ]
        );
        assert_eq!(lines_of(&[0], &[true], 4, false), Vec::<Line>::new());
        assert_eq!(
            lines_of(&[3], &[true], 0, false).len(),
            3,
            "no columns: one a line"
        );
    }

    /// A view that fits more columns than any folder has items (an infinite width cast to
    /// `usize` is `usize::MAX`) lays every group out on one line, ending at the group's end: the
    /// line's end never overflows (a panic in a debug build, an endless loop in a release one).
    #[test]
    fn a_line_of_more_columns_than_items_ends_at_its_group() {
        assert_eq!(
            lines_of(&[3, 2], &[true, true], usize::MAX, true),
            vec![
                Line::Header { group: 0 },
                Line::Items { start: 0, end: 3 },
                Line::Header { group: 1 },
                Line::Items { start: 3, end: 5 },
            ]
        );
    }

    /// F5 keeps the old rows' sizes and dates on the rows it read again, unknown still (their
    /// stats are asked for again): the Size column does not blank while the folder is read.
    #[test]
    fn a_refresh_shows_the_old_sizes_until_the_new_ones_are_in() {
        let mut old = vec![
            scanned_entry("", "a.txt", false),
            scanned_entry("", "b.txt", false),
            scanned_entry("", "dir", true),
        ];
        let stats = [
            Stat {
                key: String::from("a.txt"),
                size: Some(5),
                modified: Some(10),
            },
            Stat {
                key: String::from("dir/"),
                size: None,
                modified: Some(20),
            },
        ];
        assert_eq!(apply_stats(&mut old, &stats), 2);
        let mut fresh = vec![
            scanned_entry("", "a.txt", false),
            scanned_entry("", "b.txt", false),
            scanned_entry("", "c.txt", false),
            scanned_entry("", "dir", true),
        ];
        carry_stats(&old, &mut fresh);
        let seen: Vec<(Option<u64>, Option<u64>, bool)> = fresh
            .iter()
            .map(|e| (e.size, e.modified, e.known))
            .collect();
        assert_eq!(
            seen,
            vec![
                (Some(5), Some(10), false),
                (None, None, false),
                (None, None, false),
                (None, Some(20), false),
            ],
            "a.txt and dir/ keep what they showed; b.txt was never stat'ed, c.txt is new"
        );
    }

    /// 100,000 rows of 24 px in a 480 px view: 20 rows show, a screen either side is built -
    /// never the folder; scrolled into the middle, the window follows; past the end, the last
    /// screen.
    #[test]
    fn a_virtual_view_builds_the_rows_in_view_and_a_screen_either_side() {
        let lines = lines_of(&[100_000], &[true], 1, false);
        let (tops, total) = line_tops(&lines, 24.0);
        assert_eq!(total, 2_400_000.0);
        assert_eq!(lines_in_view(&tops, total, 0.0, 480.0), 0..20);
        assert_eq!(lines_window(&tops, total, 0.0, 480.0), 0..40);
        let mid = lines_window(&tops, total, 50_000.0 * 24.0, 480.0);
        assert_eq!(mid, 49_980..50_040);
        assert!(mid.len() <= 60, "three screens at most");
        let end = lines_window(&tops, total, 1e9, 480.0);
        assert_eq!(end.end, 100_000, "past the end: the last screen");
        assert_eq!(
            lines_window(&tops, total, 0.0, 0.0),
            0..0,
            "no height, no rows"
        );
        assert_eq!(lines_window(&[], 0.0, 0.0, 480.0), 0..0);
        // A row cut at the bottom shows, and is in view.
        assert_eq!(lines_in_view(&tops, total, 10.0, 480.0), 0..21);
    }

    /// Headers and item lines differ in height: the window follows the real tops.
    #[test]
    fn the_window_follows_lines_of_two_heights() {
        let lines = lines_of(&[10, 10], &[true, true], 1, true);
        let (tops, total) = line_tops(&lines, 20.0);
        assert_eq!(tops[0], 0.0);
        assert_eq!(tops[1], HEADER_PX);
        assert_eq!(tops[11], HEADER_PX + 200.0, "the second header");
        assert_eq!(total, 2.0 * HEADER_PX + 400.0);
        let seen = lines_in_view(&tops, total, HEADER_PX + 200.0, 51.0);
        assert_eq!(
            seen,
            11..14,
            "the second header on top, two of its rows under it"
        );
        assert_eq!(positions_of(&lines, seen), 10..12);
        assert_eq!(
            positions_of(&lines, 11..12),
            0..0,
            "a header holds no items"
        );
    }

    /// Only the items in view that are not known and not asked for are stat'ed, at most `max`;
    /// a sort by Size or Date modified waits for every item's.
    #[test]
    fn only_the_unknown_items_in_view_are_stated() {
        let mut rows = vec![file("a"), file("b"), file("c"), file("d")];
        rows[1].known = true;
        let shown: Vec<&Entry> = rows.iter().collect();
        let asked: HashSet<String> = [String::from("f/c")].into_iter().collect();
        assert_eq!(stats_wanted(&shown, 0..3, &asked, 10), vec!["f/a"]);
        assert_eq!(
            stats_wanted(&shown, 0..4, &HashSet::new(), 2),
            vec!["f/a", "f/c"]
        );
        assert!(
            stats_wanted(&shown, 7..9, &HashSet::new(), 2).is_empty(),
            "out of range"
        );
        assert_eq!(unknown_keys(&rows, &asked, 10), vec!["f/a", "f/d"]);
        assert!(sort_needs_stats(Sort {
            column: Column::Size,
            descending: false
        }));
        assert!(!sort_needs_stats(Sort::default()));
        assert!(!all_known(&rows));
    }

    /// A stat answer fills in the rows it names; a folder keeps no size; a key gone since
    /// changes nothing.
    #[test]
    fn stats_fill_in_the_rows_they_name() {
        let mut rows = vec![file("a"), folder("d")];
        let changed = apply_stats(
            &mut rows,
            &[
                Stat {
                    key: String::from("f/a"),
                    size: Some(12),
                    modified: Some(100),
                },
                Stat {
                    key: String::from("f/d/"),
                    size: Some(4096),
                    modified: Some(200),
                },
                Stat {
                    key: String::from("f/gone"),
                    size: Some(1),
                    modified: None,
                },
            ],
        );
        assert_eq!(changed, 2);
        assert_eq!(
            (rows[0].size, rows[0].modified, rows[0].known),
            (Some(12), Some(100), true)
        );
        assert_eq!(
            (rows[1].size, rows[1].modified, rows[1].known),
            (None, Some(200), true)
        );
        assert!(all_known(&rows));
    }

    #[test]
    fn the_status_counts_big_folders_with_grouped_digits() {
        use azul_appkit::l10n::Arg;

        let one = count_text(1, true);
        assert_eq!(one.key, "azdrive-status-items");
        assert_eq!(one.get("count"), Some(&Arg::Int(1)));
        let big = count_text(12_345, true);
        assert_eq!(big.get("count"), Some(&Arg::Int(12_345)));
        let so_far = count_text(1_000_000, false);
        assert_eq!(so_far.key, "azdrive-status-items-so-far");
        assert_eq!(grouped_digits(12_345), "12,345");
        assert_eq!(grouped_digits(999), "999");
        assert_eq!(grouped_digits(0), "0");
    }
}
