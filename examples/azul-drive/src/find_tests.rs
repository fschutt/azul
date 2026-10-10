//! The search box's search as plain data (`find`): the request a search box text makes, the
//! results as rows (names first, a content match joining its row), a cloud drive's listing
//! searched by name, the status line, the settings.

use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

use azul_search::{ContentHit, LineMatch, NameHit, NameMatcher, Pattern, PatternKind};
use azul_storage::{ListPage, ObjectInfo};

use chrono::{FixedOffset, TimeZone};

use crate::{
    browse::{Column, Entry, Sort},
    find::{
        self, DateRefine, FindEnd, FindOptions, FindPhase, FindState, Found, FoundLine,
        KindRefine, Refines, SizeRefine,
    },
    model::Settings,
};

fn row(key: &str, is_folder: bool) -> Entry {
    Entry {
        key: key.to_string(),
        name: azul_storage::key::last_segment(key).to_string(),
        is_folder,
        size: None,
        modified: None,
        etag: None,
        known: false,
    }
}

fn found(key: &str, line: Option<(u64, &str)>) -> Found {
    Found {
        entry: row(key, key.ends_with('/')),
        line: line.map(|(line, text)| FoundLine {
            line,
            text: text.to_string(),
            start: 0,
            end: text.len().min(3),
        }),
    }
}

fn state(query: &str, contents: bool, remote: bool) -> FindState {
    FindState::new(
        query.to_string(),
        contents,
        remote,
        1,
        Arc::new(AtomicBool::new(false)),
    )
}

/// The search box's text searches names in the open folder and below it: a literal part of a
/// name, or (with a wildcard) a glob; the contents too when "File contents" is on - never for
/// a glob, which names files. Hidden items as the view shows them, ignored files as set; the
/// storage crate's temporary files never, and at a drive's root not its bookkeeping folder.
#[test]
fn a_search_box_text_searches_names_below_the_folder_and_contents_on_request() {
    let root = PathBuf::from("/home/me/Documents");
    let options = |contents: bool, show_hidden: bool, ignore_files: bool| FindOptions {
        contents,
        show_hidden,
        ignore_files,
        ..FindOptions::default()
    };
    let names = find::local_request(root.clone(), "report", false, &options(false, false, true));
    assert_eq!(names.root, root);
    assert_eq!(names.names.as_ref().map(|p| p.kind), Some(PatternKind::Literal));
    assert_eq!(names.names.as_ref().map(|p| p.text.as_str()), Some("report"));
    assert!(names.contents.is_none(), "names only unless asked");
    assert!(!names.filters.hidden && names.filters.ignore_files);
    assert!(names.filters.exclude.iter().any(|g| g == find::TEMP_GLOB));
    assert!(!names.filters.exclude.iter().any(|g| g.contains(".azlin")));
    assert_eq!(names.limits.max_results, find::FIND_MAX);
    assert_eq!(names.limits.max_lines_per_file, 1, "one line previews a file");

    let contents = find::local_request(root.clone(), "report", true, &options(true, true, false));
    assert_eq!(
        contents.contents.as_ref().map(|p| (p.kind, p.text.as_str())),
        Some((PatternKind::Literal, "report"))
    );
    assert!(contents.filters.hidden && !contents.filters.ignore_files);
    assert!(contents.filters.exclude.iter().any(|g| g == "/.azlin/"), "the drive's bookkeeping");

    let glob = find::local_request(root, "*.pdf", false, &options(true, false, true));
    assert_eq!(glob.names.as_ref().map(|p| p.kind), Some(PatternKind::Glob));
    assert!(glob.contents.is_none(), "a glob names files");
    assert!(!find::searches_contents("*.pdf") && find::searches_contents("Q3 report"));
}

/// The results are rows in the order they came: the names first, then the files whose contents
/// matched; a content match of a row found by its name joins that row (its line), and a key
/// never shows twice.
#[test]
fn results_merge_names_first_and_a_content_match_joins_its_row() {
    let mut find = state("pick", true, false);
    assert!(find.running());
    find.merge(vec![found("Docs/pick.txt", None), found("Docs/picks/", None)]);
    find.merge(vec![
        found("Docs/notes.md", Some((3, "a pick"))),
        found("Docs/pick.txt", Some((1, "pick me"))),
        found("Docs/notes.md", Some((9, "again"))),
    ]);
    let keys: Vec<&str> = find.rows.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, vec!["Docs/pick.txt", "Docs/picks/", "Docs/notes.md"]);
    assert_eq!(find.lines.get("Docs/pick.txt").map(|l| l.line), Some(1));
    assert_eq!(find.lines.get("Docs/notes.md").map(|l| l.line), Some(3), "the first line stays");
    assert!(find.entry("Docs/picks/").is_some_and(|e| e.is_folder));
    assert!(find.entry("Docs/other.txt").is_none());
}

/// A name hit and a content hit of azul-search become rows with the drive's keys: the searched
/// folder's prefix and the path below it; a folder's key ends in `/`; a content hit carries its
/// first line and the match in it.
#[test]
fn a_hit_becomes_a_row_under_the_drive_prefix() {
    let file = find::found_name(
        "Docs/",
        NameHit {
            path: String::from("deep/needle.txt"),
            name: String::from("needle.txt"),
            is_dir: false,
            range: (0, 6),
            size: None,
            modified: None,
        },
    );
    assert_eq!(file.entry.key, "Docs/deep/needle.txt");
    assert_eq!(file.entry.name, "needle.txt");
    assert!(!file.entry.is_folder && !file.entry.known, "sizes come with the rows in view");
    let folder = find::found_name(
        "",
        NameHit {
            path: String::from("deep/"),
            name: String::from("deep"),
            is_dir: true,
            range: (0, 4),
            size: None,
            modified: None,
        },
    );
    assert_eq!((folder.entry.key.as_str(), folder.entry.is_folder), ("deep/", true));

    let content = find::found_content(
        "Docs/",
        ContentHit {
            path: String::from("a/plan.md"),
            lines: vec![LineMatch {
                line: 12,
                column: 7,
                text: String::from("  the zebra one"),
                text_offset: 0,
                ranges: vec![(6, 11)],
                before: Vec::new(),
                after: Vec::new(),
            }],
            more: true,
            size: None,
            modified: None,
        },
    );
    assert_eq!(content.entry.key, "Docs/a/plan.md");
    assert_eq!(content.entry.name, "plan.md");
    let line = content.line.expect("its line");
    assert_eq!((line.line, line.start, line.end), (12, 6, 11));
    assert_eq!(&line.text[line.start..line.end], "zebra");
}

/// A hit that says its size and date (the search read them with the match) makes a row that
/// knows them: no stat job for it, and a sort by size or date has them at once.
#[test]
fn a_hits_size_and_date_make_a_row_that_knows_them() {
    let file = find::found_name(
        "",
        NameHit {
            path: String::from("a.txt"),
            name: String::from("a.txt"),
            is_dir: false,
            range: (0, 1),
            size: Some(5),
            modified: Some(1_700_000_000),
        },
    );
    assert_eq!((file.entry.size, file.entry.modified), (Some(5), Some(1_700_000_000)));
    assert!(file.entry.known);
    let folder = find::found_name(
        "",
        NameHit {
            path: String::from("a/"),
            name: String::from("a"),
            is_dir: true,
            range: (0, 1),
            size: None,
            modified: Some(1_700_000_000),
        },
    );
    assert!(folder.entry.known && folder.entry.size.is_none(), "a folder has a date only");
    let content = find::found_content(
        "",
        ContentHit {
            path: String::from("b.txt"),
            lines: Vec::new(),
            more: false,
            size: Some(9),
            modified: Some(1_600_000_000),
        },
    );
    assert_eq!((content.entry.size, content.entry.known), (Some(9), true));
    assert!(content.line.is_none());
}

/// The Match column: the line without its indentation, the match marked, a long lead cut to
/// its last characters.
#[test]
fn a_found_lines_preview_marks_the_match_and_cuts_a_long_lead() {
    let line = FoundLine {
        line: 4,
        text: String::from("    let picked = 7;"),
        start: 8,
        end: 14,
    };
    assert_eq!(
        line.preview(),
        (String::from("let "), String::from("picked"), String::from(" = 7;"))
    );
    let far = FoundLine {
        line: 1,
        text: format!("{}needle end", "x".repeat(100)),
        start: 100,
        end: 106,
    };
    let (before, matched, after) = far.preview();
    assert!(before.starts_with('\u{2026}'), "{before}");
    assert_eq!(before.chars().count(), find::PREVIEW_LEAD + 1);
    assert_eq!((matched.as_str(), after.as_str()), ("needle", " end"));
}

fn object(key: &str, size: u64) -> ObjectInfo {
    ObjectInfo {
        key: key.to_string(),
        size,
        modified: Some(1_700_000_000),
        etag: Some(String::from("tag")),
    }
}

/// A cloud drive is searched by name over a recursive listing: the files whose names match
/// (with their sizes and dates, which the listing has), the folders on the way to them whose
/// names match - each once, over every page -, a folder marker as its folder; hidden ones only
/// when hidden items show.
#[test]
fn a_cloud_listing_is_searched_by_name_with_each_folder_once() {
    let matcher = NameMatcher::new(&Pattern::literal("report")).expect("compiles");
    let mut seen = HashSet::new();
    let first = ListPage {
        folders: Vec::new(),
        objects: vec![
            object("Docs/a/report.txt", 5),
            object("Docs/a/b/notes.md", 7),
            object("Docs/.hidden/report2.txt", 9),
            object("Docs/reports/", 0),
            object("Docs/report-x.pdf", 11),
        ],
        next: Some(String::from("more")),
    };
    let shown = FindOptions::default();
    let rows = find::remote_names(&first, "Docs/", &matcher, &shown, &mut seen);
    let keys: Vec<&str> = rows.iter().map(|f| f.entry.key.as_str()).collect();
    assert_eq!(keys, vec!["Docs/a/report.txt", "Docs/reports/", "Docs/report-x.pdf"]);
    let report = &rows[0].entry;
    assert_eq!((report.size, report.known), (Some(5), true));
    assert!(rows[1].entry.is_folder);
    let second = ListPage {
        folders: Vec::new(),
        objects: vec![object("Docs/reports/q3.txt", 3)],
        next: None,
    };
    assert!(
        find::remote_names(&second, "Docs/", &matcher, &shown, &mut seen).is_empty(),
        "the folder came on the first page"
    );
    let mut fresh = HashSet::new();
    let with_hidden = FindOptions {
        show_hidden: true,
        ..FindOptions::default()
    };
    let hidden = find::remote_names(&first, "Docs/", &matcher, &with_hidden, &mut fresh);
    assert!(hidden.iter().any(|f| f.entry.key == "Docs/.hidden/report2.txt"));
}

/// The status line while the search runs ("Searching... 1,234 found"; the contents' walk, a
/// cloud drive's slower one) and once it is done.
#[test]
fn the_status_line_says_how_far_the_search_got() {
    let mut find = state("pick", true, false);
    let rows: Vec<Found> = (0..1234).map(|i| found(&format!("f{i}.txt"), None)).collect();
    find.merge(rows);
    assert_eq!(find.status_text(), "Searching... 1,234 found");
    find.phase = FindPhase::Contents;
    assert_eq!(find.status_text(), "Searching file contents... 1,234 found");
    find.end = Some(FindEnd::default());
    assert!(!find.running());
    assert_eq!(find.status_text(), "1,234 items found");
    find.end = Some(FindEnd {
        limited: true,
        ..FindEnd::default()
    });
    assert_eq!(find.status_text(), "1,234 items found (the first ones)");
    let cloud = state("pick", false, true);
    assert_eq!(
        cloud.status_text(),
        "Searching names in the cloud (slower)... 0 found"
    );
    let mut nothing = state("zzz", false, false);
    nothing.end = Some(FindEnd::default());
    assert_eq!(nothing.status_text(), "No items match your search.");
    nothing.end = Some(FindEnd {
        error: Some(String::from("\"(\" is not a regular expression")),
        ..FindEnd::default()
    });
    assert!(nothing.status_text().starts_with("The search stopped: "));
}

/// The Folder column names where a result is: its folder from the drive's root.
#[test]
fn the_folder_column_names_where_a_result_is() {
    assert_eq!(find::folder_of("Docs/deep/x.txt"), "Docs/deep");
    assert_eq!(find::folder_of("Docs/deep/"), "Docs");
    assert_eq!(find::folder_of("x.txt"), "");
}

/// The search's settings: names only and ignored files skipped, until the Search tab says
/// otherwise; a settings file of an older build keeps them.
#[test]
fn the_search_settings_start_with_names_only_and_ignored_files_skipped() {
    let settings = Settings::default();
    assert!(!settings.search_contents && settings.search_ignore_files);
    let old = Settings::from_json("{\"show_hidden\": true}");
    assert!(old.show_hidden && !old.search_contents && old.search_ignore_files);
}

/// 2026-10-08 (a Thursday) 15:00 in UTC+2, the clock the date refines are tested against.
fn thursday() -> chrono::DateTime<FixedOffset> {
    FixedOffset::east_opt(2 * 3600)
        .expect("a zone")
        .with_ymd_and_hms(2026, 10, 8, 15, 0, 0)
        .single()
        .expect("a time")
}

/// Midnight of a day in UTC+2, as seconds since 1970.
fn midnight(y: i32, m: u32, d: u32) -> u64 {
    FixedOffset::east_opt(2 * 3600)
        .expect("a zone")
        .with_ymd_and_hms(y, m, d, 0, 0, 0)
        .single()
        .expect("a time")
        .timestamp() as u64
}

/// Explorer's Date modified: today, yesterday, this / last week (from Monday), this / last month,
/// this / last year - from midnight to midnight in the user's zone.
#[test]
fn a_date_refine_is_a_range_of_whole_days_in_the_users_zone() {
    let now = thursday();
    let range = |date| find::date_range(date, &now);
    assert_eq!(range(DateRefine::Any), None);
    assert_eq!(range(DateRefine::Today), Some((midnight(2026, 10, 8), midnight(2026, 10, 9))));
    assert_eq!(range(DateRefine::Yesterday), Some((midnight(2026, 10, 7), midnight(2026, 10, 8))));
    assert_eq!(range(DateRefine::ThisWeek), Some((midnight(2026, 10, 5), midnight(2026, 10, 12))));
    assert_eq!(range(DateRefine::LastWeek), Some((midnight(2026, 9, 28), midnight(2026, 10, 5))));
    assert_eq!(range(DateRefine::ThisMonth), Some((midnight(2026, 10, 1), midnight(2026, 11, 1))));
    assert_eq!(range(DateRefine::LastMonth), Some((midnight(2026, 9, 1), midnight(2026, 10, 1))));
    assert_eq!(range(DateRefine::ThisYear), Some((midnight(2026, 1, 1), midnight(2027, 1, 1))));
    assert_eq!(range(DateRefine::LastYear), Some((midnight(2025, 1, 1), midnight(2026, 1, 1))));
}

/// Kind and Size become azul-search's refine: a kind is its extensions, a size Explorer's
/// buckets; with a date they all hold at once. "Any" of each lets everything through.
#[test]
fn the_refines_become_the_searchs_kinds_sizes_and_dates() {
    let now = thursday();
    assert!(Refines::default().is_any());
    assert!(Refines::default().to_refine(&now).is_empty());
    let pictures = Refines {
        kind: KindRefine::Picture,
        ..Refines::default()
    }
    .to_refine(&now);
    assert!(pictures.extensions.iter().any(|e| e == "jpg"));
    assert!(!pictures.extensions.iter().any(|e| e == "txt"));
    let small = Refines {
        size: SizeRefine::Small,
        ..Refines::default()
    }
    .to_refine(&now);
    assert_eq!((small.min_size, small.max_size), (Some(16 * 1024), Some(1024 * 1024)));
    let empty = SizeRefine::Empty.range();
    assert_eq!(empty, (Some(0), Some(0)));
    assert_eq!(SizeRefine::Gigantic.range(), (Some(4 * 1024 * 1024 * 1024 + 1), None));
    let all = Refines {
        date: DateRefine::Today,
        kind: KindRefine::Document,
        size: SizeRefine::Tiny,
    };
    let refine = all.to_refine(&now);
    assert_eq!(refine.modified_from, Some(midnight(2026, 10, 8)));
    assert!(refine.extensions.iter().any(|e| e == "docx"));
    assert_eq!(refine.max_size, Some(16 * 1024));
    assert_eq!(all.label(), "Date modified: Today, Kind: Document, Size: Tiny (0 - 16 KB)");
    for kind in KindRefine::ALL {
        assert!(!kind.label().is_empty());
    }
}

/// "Current folder" searches the folder's own items, "All subfolders" every folder below it;
/// the refine goes along.
#[test]
fn the_location_and_the_refine_go_into_the_request() {
    let here = FindOptions {
        subfolders: false,
        refine: azul_search::Refine {
            min_size: Some(5),
            ..azul_search::Refine::default()
        },
        ..FindOptions::default()
    };
    let request = find::local_request(PathBuf::from("/d"), "x", false, &here);
    assert_eq!(request.filters.max_depth, Some(1));
    assert_eq!(request.filters.refine.min_size, Some(5));
    let below = find::local_request(PathBuf::from("/d"), "x", false, &FindOptions::default());
    assert_eq!(below.filters.max_depth, None, "all subfolders by default");
}

fn sized(key: &str, size: u64, modified: u64) -> Found {
    let mut found = found(key, None);
    found.entry.size = Some(size);
    found.entry.modified = Some(modified);
    found.entry.known = true;
    found
}

/// The results come in the order they were found (names first); a click on a column's header
/// sorts them (again: the other way), the Match column puts them back in the found order -
/// and a batch that lands later takes its place in the sort.
#[test]
fn the_results_sort_by_a_column_and_go_back_to_the_found_order() {
    let mut find = state("x", false, false);
    find.merge(vec![sized("b.txt", 30, 2), sized("a.txt", 10, 3), sized("c.txt", 20, 1)]);
    let keys = |find: &FindState| -> Vec<String> {
        find.shown().iter().map(|e| e.key.clone()).collect()
    };
    assert_eq!(keys(&find), vec!["b.txt", "a.txt", "c.txt"], "as found");
    find.set_sort(Some(Sort {
        column: Column::Size,
        descending: false,
    }));
    assert_eq!(keys(&find), vec!["a.txt", "c.txt", "b.txt"]);
    find.merge(vec![sized("d.txt", 15, 4)]);
    assert_eq!(keys(&find), vec!["a.txt", "d.txt", "c.txt", "b.txt"], "a later batch sorts in");
    find.set_sort(Some(Sort {
        column: Column::Name,
        descending: true,
    }));
    assert_eq!(keys(&find), vec!["d.txt", "c.txt", "b.txt", "a.txt"]);
    find.set_sort(None);
    assert_eq!(keys(&find), vec!["b.txt", "a.txt", "c.txt", "d.txt"], "the found order again");
    assert!(find.entry("d.txt").is_some(), "a key still finds its row");
}

/// A search of This PC runs over every drive on this computer, one job each: the rows' keys
/// name their drive, and the search ends when the last job has.
#[test]
fn a_search_of_this_pc_ends_when_its_last_drive_has() {
    let key = find::pc_key("home", "Docs/a.txt");
    assert_eq!(find::split_pc_key(&key), Some(("home", "Docs/a.txt")));
    assert_eq!(find::split_pc_key("Docs/a.txt"), None, "a key of the open drive");
    let mut find = state("a", false, false);
    find.pending = 2;
    assert!(!find.job_ended(FindEnd::default()));
    assert!(find.running());
    assert!(find.job_ended(FindEnd {
        limited: true,
        ..FindEnd::default()
    }));
    assert_eq!(find.end.as_ref().map(|e| e.limited), Some(true), "one job's limit is the search's");
}

/// "Current folder" on a cloud drive: the folder's own listing (its folders are common
/// prefixes); a refine holds there too (the listing has sizes and dates).
#[test]
fn a_cloud_folders_own_listing_is_searched_with_the_refine() {
    let matcher = NameMatcher::new(&Pattern::literal("report")).expect("compiles");
    let page = ListPage {
        folders: vec![String::from("Docs/reports/")],
        objects: vec![object("Docs/report-big.pdf", 5000), object("Docs/report-small.txt", 5)],
        next: None,
    };
    let mut seen = HashSet::new();
    let all = find::remote_names(&page, "Docs/", &matcher, &FindOptions::default(), &mut seen);
    let keys: Vec<&str> = all.iter().map(|f| f.entry.key.as_str()).collect();
    assert_eq!(keys, vec!["Docs/reports/", "Docs/report-big.pdf", "Docs/report-small.txt"]);
    let big_only = FindOptions {
        refine: azul_search::Refine {
            min_size: Some(100),
            ..azul_search::Refine::default()
        },
        ..FindOptions::default()
    };
    let mut fresh = HashSet::new();
    let big = find::remote_names(&page, "Docs/", &matcher, &big_only, &mut fresh);
    let keys: Vec<&str> = big.iter().map(|f| f.entry.key.as_str()).collect();
    assert_eq!(keys, vec!["Docs/report-big.pdf"], "a size is files only");
}

/// A cloud drive's last full listing is kept in the cache folder: read back as written (names
/// with tabs and line breaks too), refused when it is of another format; its file is named
/// after the drive, safely.
#[test]
fn a_cloud_listing_is_kept_and_read_back() {
    let dir = std::env::temp_dir().join(format!(
        "azdrive-listing-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let file = find::listing_file(&dir, "s3:my/bucket");
    assert!(file.starts_with(&dir));
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    assert!(!name.contains('/') && !name.contains(':'), "{name}");
    let listing = find::CachedListing {
        prefix: String::from("Docs/"),
        at: 1_700_000_000,
        objects: vec![
            object("Docs/a.txt", 5),
            ObjectInfo {
                key: String::from("Docs/odd\tname\n.txt"),
                size: 7,
                modified: None,
                etag: None,
            },
        ],
    };
    find::write_listing(&file, &listing).expect("written");
    assert_eq!(find::read_listing(&file), Some(listing));
    std::fs::write(&file, "something else\n").expect("overwritten");
    assert_eq!(find::read_listing(&file), None);
    assert_eq!(find::read_listing(&dir.join("missing.tsv")), None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A row the cached listing showed that the fresh listing has not got goes (deleted since).
#[test]
fn stale_results_go_when_the_fresh_listing_has_not_got_them() {
    let mut find = state("x", false, true);
    find.merge(vec![found("Docs/a.txt", Some((1, "x"))), found("Docs/b.txt", None)]);
    find.set_sort(Some(Sort {
        column: Column::Name,
        descending: false,
    }));
    find.remove(&[String::from("Docs/a.txt")]);
    let keys: Vec<String> = find.shown().iter().map(|e| e.key.clone()).collect();
    assert_eq!(keys, vec!["Docs/b.txt"]);
    assert!(find.entry("Docs/a.txt").is_none() && find.lines.get("Docs/a.txt").is_none());
    assert!(find.entry("Docs/b.txt").is_some(), "the others keep their keys");
    assert!(find.status_text().contains('1'));
}

/// A document's line for its result: the first line of its text the search's text is on, the
/// match marked (without case); none when no line holds it; a long line cut around the match.
#[test]
fn a_documents_line_is_the_first_one_its_text_matches_on() {
    let matcher = azul_search::ContentMatcher::new(&Pattern::literal("needle")).expect("compiles");
    let line = find::document_line("Subject: lunch\nthe Needle is here\nneedle again\n", &matcher)
        .expect("a line");
    assert_eq!((line.line, line.text.as_str()), (2, "the Needle is here"));
    assert_eq!(&line.text[line.start..line.end], "Needle");
    assert_eq!(find::document_line("nothing here", &matcher), None);
    let long = format!("{}needle{}", "\u{e9}".repeat(3000), "y".repeat(5000));
    let cut = find::document_line(&long, &matcher).expect("a line");
    assert!(cut.text.len() < 1000, "a long line is cut around the match");
    assert_eq!(&cut.text[cut.start..cut.end], "needle");
}

/// A drive's index has a folder of its own in the cache (named safely after the drive; two
/// ids that read alike do not share it), and reads what the search box reads by default: no
/// hidden items, no ignored files, never the storage crate's temporary files or a drive's
/// bookkeeping.
#[test]
fn a_drives_index_has_a_folder_of_its_own_and_reads_what_the_search_box_does() {
    let dir = PathBuf::from("/cache/AzDrive/index");
    let folder = find::index_dir(&dir, "s3:my/bucket");
    assert!(folder.starts_with(&dir));
    let name = folder.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    assert!(!name.contains('/') && !name.contains(':'), "{name}");
    assert_ne!(find::index_dir(&dir, "a:b"), find::index_dir(&dir, "a_b"));
    let filters = find::index_filters();
    assert!(!filters.hidden && filters.ignore_files);
    assert!(filters.exclude.iter().any(|glob| glob == find::TEMP_GLOB));
    assert!(filters.exclude.iter().any(|glob| glob.contains(azul_storage::manifest::MANIFEST_DIR)));
    assert_eq!(filters.max_depth, None, "every folder");
}

/// A drive's index says on the status line how far its update got, what it holds, or why it
/// could not be brought up to date; a search asks it once it holds the drive (a former run's
/// index too, while it is brought up to date).
#[test]
fn an_index_says_how_far_it_got_and_is_asked_once_it_holds_the_drive() {
    use azul_search_index::{IndexStatus, UpdateProgress};

    let mut info = find::IndexInfo::default();
    assert!(!info.usable());
    assert_eq!(info.status_text(), "Not indexed yet");
    info.progress = Some(UpdateProgress::default());
    assert_eq!(info.status_text(), "Indexing: looking at the files...");
    info.progress = Some(UpdateProgress {
        listed: 4000,
        to_read: 4000,
        read: 120,
    });
    assert_eq!(info.status_text(), "Indexing: 120 of 4,000 files read...");
    info.status = Some(IndexStatus {
        files: 10,
        documents: 8,
        updated: Some(1_700_000_000),
    });
    assert!(info.usable(), "a former run's index is asked while it is brought up to date");
    info.progress = None;
    info.status = Some(IndexStatus {
        files: 4000,
        documents: 3500,
        updated: Some(1_700_000_100),
    });
    assert_eq!(info.status_text(), "Indexed: 4,000 files");
    info.error = Some(String::from("another window is updating this index"));
    assert_eq!(
        info.status_text(),
        "The index could not be updated: another window is updating this index"
    );
    let never = find::IndexInfo {
        status: Some(IndexStatus::default()),
        ..find::IndexInfo::default()
    };
    assert!(!never.usable(), "an index no update went over holds nothing");
}

/// A cloud drive's last listing is kept on this computer - never an encrypted drive's, whose
/// names would lie in the cache in the clear (nor an Azlin drive's not known to be plain yet); a
/// drive without encryption keeps it.
#[test]
fn a_drive_keeps_its_listing_unless_its_names_are_encrypted() {
    use azul_storage::config::{DriveEntry, DriveLocation};

    let slot = crate::Slot::new(DriveEntry {
        id: String::from("bucket"),
        name: String::from("Bucket"),
        location: DriveLocation::Local {
            root: String::from("/tmp/bucket"),
        },
    });
    assert!(crate::keeps_listing(&slot));
}
