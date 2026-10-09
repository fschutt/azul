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

use crate::{
    browse::Entry,
    find::{self, FindEnd, FindPhase, FindState, Found, FoundLine},
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
    let names = find::local_request(root.clone(), "report", false, false, true, false);
    assert_eq!(names.root, root);
    assert_eq!(names.names.as_ref().map(|p| p.kind), Some(PatternKind::Literal));
    assert_eq!(names.names.as_ref().map(|p| p.text.as_str()), Some("report"));
    assert!(names.contents.is_none(), "names only unless asked");
    assert!(!names.filters.hidden && names.filters.ignore_files);
    assert!(names.filters.exclude.iter().any(|g| g == find::TEMP_GLOB));
    assert!(!names.filters.exclude.iter().any(|g| g.contains(".azlin")));
    assert_eq!(names.limits.max_results, find::FIND_MAX);
    assert_eq!(names.limits.max_lines_per_file, 1, "one line previews a file");

    let contents = find::local_request(root.clone(), "report", true, true, false, true);
    assert_eq!(
        contents.contents.as_ref().map(|p| (p.kind, p.text.as_str())),
        Some((PatternKind::Literal, "report"))
    );
    assert!(contents.filters.hidden && !contents.filters.ignore_files);
    assert!(contents.filters.exclude.iter().any(|g| g == "/.azlin/"), "the drive's bookkeeping");

    let glob = find::local_request(root, "*.pdf", true, false, true, false);
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
        },
    );
    assert_eq!(content.entry.key, "Docs/a/plan.md");
    assert_eq!(content.entry.name, "plan.md");
    let line = content.line.expect("its line");
    assert_eq!((line.line, line.start, line.end), (12, 6, 11));
    assert_eq!(&line.text[line.start..line.end], "zebra");
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
    let rows = find::remote_names(&first, "Docs/", &matcher, false, &mut seen);
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
        find::remote_names(&second, "Docs/", &matcher, false, &mut seen).is_empty(),
        "the folder came on the first page"
    );
    let mut fresh = HashSet::new();
    let hidden = find::remote_names(&first, "Docs/", &matcher, true, &mut fresh);
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
        error: None,
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
        limited: false,
        error: Some(String::from("\"(\" is not a regular expression")),
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
