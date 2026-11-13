//! The search end to end, on temporary folders: names, contents, regexes, binary files, ignore
//! files, hidden files, filters, limits, cancellation, the order of the phases.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    list_files, search, search_listed, Case, ContentHit, Event, Filters, Limits, NameHit, Pattern,
    Phase, Refine, Request, Summary,
};

/// A fresh folder under the system's temporary folder, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        // Tests run in parallel: the clock alone could give two of them one folder.
        static MADE: AtomicUsize = AtomicUsize::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!(
            "azul-search-{tag}-{}-{nanos}-{}",
            std::process::id(),
            MADE.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&dir).expect("a temporary folder");
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// Writes `bytes` to the file at `path` (`/`-separated), its folders made first.
    fn write(&self, path: &str, bytes: &[u8]) {
        let file = self.0.join(path);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).expect("the file's folder");
        }
        fs::write(file, bytes).expect("the file");
    }

    fn folder(&self, path: &str) {
        fs::create_dir_all(self.0.join(path)).expect("a folder");
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Every event of a search, and its summary.
fn run(request: &Request) -> (Vec<Event>, Summary) {
    let cancel = AtomicBool::new(false);
    let mut events = Vec::new();
    let summary =
        search(request, &cancel, &mut |event| events.push(event)).expect("the search starts");
    (events, summary)
}

fn names(events: &[Event]) -> Vec<String> {
    let mut found: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::Name(NameHit { path, .. }) => Some(path.clone()),
            _ => None,
        })
        .collect();
    found.sort();
    found
}

fn contents(events: &[Event]) -> Vec<ContentHit> {
    let mut found: Vec<ContentHit> = events
        .iter()
        .filter_map(|e| match e {
            Event::Content(hit) => Some(hit.clone()),
            _ => None,
        })
        .collect();
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

fn content_paths(events: &[Event]) -> Vec<String> {
    contents(events).into_iter().map(|hit| hit.path).collect()
}

/// A small tree: names and contents to find, at several depths.
fn sample() -> TempDir {
    let dir = TempDir::new("sample");
    dir.write("a.txt", b"nothing here\n");
    dir.write("docs/Report.md", b"# Report\n\nThe picked numbers.\n");
    dir.write("docs/deep/report-2.txt", b"line one\npicked twice: picked\n");
    dir.write("src/lib.rs", b"pub fn picked() -> u32 {\n    7\n}\n");
    dir.folder("reports");
    dir
}

#[test]
fn names_are_found_in_the_folder_and_every_folder_below_it() {
    let dir = sample();
    let (events, summary) = run(&Request::new(dir.path()).with_names(Pattern::literal("report")));
    assert_eq!(
        names(&events),
        vec!["docs/Report.md", "docs/deep/report-2.txt", "reports/"],
        "without case, at every depth, a folder with its /"
    );
    assert_eq!(summary.names, 3);
    assert!(summary.walked >= 8, "every file and folder walked: {summary:?}");
    let folder = events.iter().find_map(|e| match e {
        Event::Name(hit) if hit.path == "reports/" => Some(hit.clone()),
        _ => None,
    });
    let folder = folder.expect("the folder");
    assert!(folder.is_dir);
    assert_eq!((folder.name.as_str(), folder.range), ("reports", (0, 6)));
}

#[test]
fn smart_case_minds_an_upper_case_letter() {
    let dir = sample();
    let smart = Pattern::literal("Report").with_case(Case::Smart);
    let (events, _) = run(&Request::new(dir.path()).with_names(smart));
    assert_eq!(names(&events), vec!["docs/Report.md"]);
}

#[test]
fn a_glob_matches_whole_names_and_a_glob_with_a_slash_matches_paths() {
    let dir = sample();
    let (md, _) = run(&Request::new(dir.path()).with_names(Pattern::guess("*.md")));
    assert_eq!(names(&md), vec!["docs/Report.md"]);
    let (deep, _) = run(&Request::new(dir.path()).with_names(Pattern::glob("docs/**/*.txt")));
    assert_eq!(names(&deep), vec!["docs/deep/report-2.txt"]);
}

#[test]
fn a_regex_finds_names_and_lines() {
    let dir = sample();
    let (by_name, _) =
        run(&Request::new(dir.path()).with_names(Pattern::regex(r"^report-\d+\.txt$")));
    assert_eq!(names(&by_name), vec!["docs/deep/report-2.txt"]);
    let (by_line, _) = run(&Request::new(dir.path()).with_contents(Pattern::regex(r"fn \w+\(")));
    let hits = contents(&by_line);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "src/lib.rs");
    assert_eq!(hits[0].lines[0].ranges, vec![(4, 14)]);
}

#[test]
fn a_content_search_reports_each_files_lines_with_their_columns() {
    let dir = sample();
    let (events, summary) =
        run(&Request::new(dir.path()).with_contents(Pattern::literal("picked")));
    let hits = contents(&events);
    assert_eq!(
        hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(),
        vec!["docs/Report.md", "docs/deep/report-2.txt", "src/lib.rs"]
    );
    let twice = &hits[1].lines[0];
    assert_eq!((twice.line, twice.column), (2, 1));
    assert_eq!(twice.text, "picked twice: picked");
    assert_eq!(twice.ranges, vec![(0, 6), (14, 20)]);
    assert_eq!((summary.files, summary.matches), (3, 3));
    assert_eq!(summary.searched, 4, "every file read once: {summary:?}");
}

#[test]
fn context_lines_come_with_a_match() {
    let dir = TempDir::new("context");
    dir.write("notes.txt", b"one\ntwo\nneedle\nfour\nfive\n");
    let request = Request::new(dir.path())
        .with_contents(Pattern::literal("needle"))
        .with_context(1);
    let (events, _) = run(&request);
    let line = &contents(&events)[0].lines[0];
    assert_eq!((line.before.as_slice(), line.after.as_slice()), (
        &[String::from("two")][..],
        &[String::from("four")][..]
    ));
}

#[test]
fn binary_files_are_passed_over_but_their_names_are_found() {
    let dir = TempDir::new("binary");
    dir.write("logo.bin", b"picked\0\0\0");
    dir.write("picked.txt", b"picked\n");
    let request = Request::new(dir.path())
        .with_names(Pattern::literal("logo"))
        .with_contents(Pattern::literal("picked"));
    let (events, summary) = run(&request);
    assert_eq!(names(&events), vec!["logo.bin"]);
    assert_eq!(content_paths(&events), vec!["picked.txt"]);
    assert_eq!(summary.binary, 1);
}

#[test]
fn a_file_over_the_size_limit_is_not_read_but_its_name_is_found() {
    let dir = TempDir::new("size");
    dir.write("big.txt", "picked\n".repeat(100).as_bytes());
    dir.write("small.txt", b"picked\n");
    let request = Request::new(dir.path())
        .with_names(Pattern::literal("big"))
        .with_contents(Pattern::literal("picked"))
        .with_limits(Limits {
            max_file_size: 100,
            ..Limits::default()
        });
    let (events, summary) = run(&request);
    assert_eq!(names(&events), vec!["big.txt"]);
    assert_eq!(content_paths(&events), vec!["small.txt"]);
    assert_eq!(summary.too_large, 1);
}

#[test]
fn a_utf16_file_is_text_unless_the_app_reads_utf8_only() {
    let dir = TempDir::new("utf16");
    let mut bytes = vec![0xFF, 0xFE];
    for unit in "the picked one\n".encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    dir.write("notes.txt", &bytes);
    dir.write("plain.txt", b"picked\n");
    let request = Request::new(dir.path()).with_contents(Pattern::literal("picked"));
    let (text, _) = run(&request);
    assert_eq!(content_paths(&text), vec!["notes.txt", "plain.txt"]);
    let (utf8_only, summary) = run(&request.clone().with_utf16(false));
    assert_eq!(content_paths(&utf8_only), vec!["plain.txt"]);
    assert_eq!(summary.binary, 1, "passed over as binary");
}

#[test]
fn ignore_files_and_hidden_files_are_honoured_unless_turned_off() {
    let dir = TempDir::new("ignore");
    dir.write(".gitignore", b"target/\n*.log\n");
    dir.write("target/out.txt", b"picked\n");
    dir.write("app.log", b"picked\n");
    dir.write(".hidden.txt", b"picked\n");
    dir.write(".config/settings.txt", b"picked\n");
    dir.write("src/main.rs", b"picked\n");
    let picked = || Request::new(dir.path()).with_contents(Pattern::literal("picked"));
    let (default, _) = run(&picked());
    assert_eq!(content_paths(&default), vec!["src/main.rs"]);
    let (no_ignore, _) = run(&picked().with_filters(Filters {
        ignore_files: false,
        ..Filters::default()
    }));
    assert_eq!(
        content_paths(&no_ignore),
        vec!["app.log", "src/main.rs", "target/out.txt"]
    );
    let (hidden, _) = run(&picked().with_filters(Filters {
        hidden: true,
        ..Filters::default()
    }));
    assert_eq!(
        content_paths(&hidden),
        vec![".config/settings.txt", ".hidden.txt", "src/main.rs"]
    );
}

#[test]
fn include_and_exclude_globs_narrow_the_search() {
    let dir = sample();
    let only_rust = Filters {
        include: vec![String::from("*.rs")],
        ..Filters::default()
    };
    let (rust, _) = run(&Request::new(dir.path())
        .with_contents(Pattern::literal("picked"))
        .with_filters(only_rust));
    assert_eq!(content_paths(&rust), vec!["src/lib.rs"]);
    let without_docs = Filters {
        exclude: vec![String::from("docs/")],
        ..Filters::default()
    };
    let (rest, _) = run(&Request::new(dir.path())
        .with_names(Pattern::literal("re"))
        .with_filters(without_docs));
    assert_eq!(names(&rest), vec!["reports/"]);
}

#[test]
fn names_come_first_then_contents() {
    let dir = sample();
    let request = Request::new(dir.path())
        .with_names(Pattern::literal("picked"))
        .with_contents(Pattern::literal("picked"));
    dir.write("picked-list.txt", b"picked\n");
    let (events, summary) = run(&request);
    let order: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            Event::Phase(Phase::Names) => Some("names"),
            Event::Phase(Phase::Contents) => Some("contents"),
            Event::Name(_) => Some("name"),
            Event::Content(_) => Some("content"),
            Event::Progress(_) => None,
        })
        .collect();
    assert_eq!(order[..3], ["names", "name", "contents"]);
    assert!(order[3..].iter().all(|e| *e == "content"), "{order:?}");
    assert_eq!((summary.names, summary.files), (1, 4));
}

#[test]
fn the_result_limit_stops_the_search_and_says_so() {
    let dir = TempDir::new("limit");
    for i in 0..50 {
        dir.write(&format!("match-{i:02}.txt"), b"x\n");
    }
    let request = Request::new(dir.path())
        .with_names(Pattern::literal("match"))
        .with_limits(Limits {
            max_results: 10,
            ..Limits::default()
        });
    let (events, summary) = run(&request);
    assert_eq!(names(&events).len(), 10);
    assert!(summary.limited && !summary.cancelled);

    let lines = TempDir::new("limit-lines");
    lines.write("many.txt", "x\n".repeat(30).as_bytes());
    let total = Request::new(lines.path())
        .with_contents(Pattern::literal("x"))
        .with_limits(Limits {
            max_matches: 5,
            ..Limits::default()
        });
    let (events, summary) = run(&total);
    let hits = contents(&events);
    assert_eq!((hits[0].lines.len(), hits[0].more), (5, true));
    assert!(summary.limited);
    let per_file = Request::new(lines.path())
        .with_contents(Pattern::literal("x"))
        .with_limits(Limits {
            max_lines_per_file: 2,
            ..Limits::default()
        });
    let (events, summary) = run(&per_file);
    let hits = contents(&events);
    assert_eq!((hits[0].lines.len(), hits[0].more), (2, true));
    assert!(!summary.limited, "a file's own limit does not end the search");
}

#[test]
fn a_cancelled_search_stops_at_once_and_hands_over_nothing_more() {
    let dir = TempDir::new("cancel");
    for i in 0..400 {
        dir.write(&format!("d{}/file-{i:03}.txt", i % 20), b"picked\n");
    }
    let request = Request::new(dir.path()).with_contents(Pattern::literal("picked"));
    let cancel = AtomicBool::new(false);
    let mut found = 0;
    let mut cancelled_at = None;
    let summary = search(&request, &cancel, &mut |event| {
        if let Event::Content(_) = event {
            found += 1;
            cancel.store(true, Ordering::SeqCst);
            cancelled_at.get_or_insert_with(Instant::now);
        }
    })
    .expect("the search starts");
    let took = cancelled_at.map_or(Duration::ZERO, |at| at.elapsed());
    assert_eq!(found, 1, "nothing is handed over after the cancel");
    assert!(summary.cancelled && !summary.limited);
    assert!(took < Duration::from_secs(2), "it stopped in {took:?}");

    let before = AtomicBool::new(true);
    let mut events = 0;
    let summary = search(&request, &before, &mut |event| {
        if !matches!(event, Event::Phase(_)) {
            events += 1;
        }
    })
    .expect("the search starts");
    assert_eq!((events, summary.searched), (0, 0), "cancelled before it began");
    assert!(summary.cancelled);
}

#[test]
fn a_bad_pattern_or_filter_glob_is_an_error_before_anything_is_walked() {
    let dir = sample();
    let cancel = AtomicBool::new(false);
    let mut events = 0;
    let bad_regex = Request::new(dir.path()).with_contents(Pattern::regex("("));
    assert!(search(&bad_regex, &cancel, &mut |_| events += 1).is_err());
    let bad_glob = Request::new(dir.path())
        .with_names(Pattern::literal("a"))
        .with_filters(Filters {
            include: vec![String::from("{a,b")],
            ..Filters::default()
        });
    assert!(search(&bad_glob, &cancel, &mut |_| events += 1).is_err());
    assert_eq!(events, 0);
}

/// Seconds since 1970 of a file's date, as the hits carry it.
fn year(y: u64) -> u64 {
    (y - 1970) * 365 * 24 * 3600
}

/// Sets the date a file was last modified.
fn set_modified(dir: &TempDir, path: &str, secs: u64) {
    let file = fs::File::options()
        .write(true)
        .open(dir.path().join(path))
        .expect("the file");
    file.set_modified(UNIX_EPOCH + Duration::from_secs(secs))
        .expect("its date");
}

#[test]
fn the_current_folder_alone_is_searched_when_asked() {
    let dir = sample();
    let here = Filters {
        max_depth: Some(1),
        ..Filters::default()
    };
    let (events, _) = run(&Request::new(dir.path())
        .with_names(Pattern::literal("report"))
        .with_filters(here));
    assert_eq!(names(&events), vec!["reports/"], "the folder's own items only");
}

#[test]
fn a_name_hit_says_its_size_and_date() {
    let dir = sample();
    let (events, _) = run(&Request::new(dir.path()).with_names(Pattern::literal("report-2")));
    let hit = events
        .iter()
        .find_map(|e| match e {
            Event::Name(hit) => Some(hit.clone()),
            _ => None,
        })
        .expect("the file");
    assert_eq!(hit.size, Some(30), "\"line one\\npicked twice: picked\\n\"");
    assert!(hit.modified.is_some());
    let (folders, _) = run(&Request::new(dir.path()).with_names(Pattern::literal("reports")));
    let folder = folders
        .iter()
        .find_map(|e| match e {
            Event::Name(hit) => Some(hit.clone()),
            _ => None,
        })
        .expect("the folder");
    assert_eq!(folder.size, None, "a folder has no size");
}

#[test]
fn refine_keeps_the_kinds_the_sizes_and_the_dates_asked_for() {
    let dir = TempDir::new("refine");
    dir.write("a.md", &[b'x'; 10]);
    dir.write("b.txt", &[b'x'; 1000]);
    dir.write("c.MD", &[b'x'; 5000]);
    dir.folder("md-notes");
    set_modified(&dir, "a.md", year(2001));
    let every = || Request::new(dir.path()).with_names(Pattern::glob("*"));
    let refined = |refine: Refine| {
        run(&every().with_filters(Filters {
            refine,
            ..Filters::default()
        }))
        .0
    };
    let kinds = refined(Refine {
        extensions: vec![String::from("md")],
        ..Refine::default()
    });
    assert_eq!(names(&kinds), vec!["a.md", "c.MD"], "a kind is files of its extensions");
    let big = refined(Refine {
        min_size: Some(100),
        ..Refine::default()
    });
    assert_eq!(names(&big), vec!["b.txt", "c.MD"], "a size is files only");
    let old = refined(Refine {
        modified_until: Some(year(2010)),
        ..Refine::default()
    });
    assert_eq!(names(&old), vec!["a.md"]);
    let recent = refined(Refine {
        modified_from: Some(year(2010)),
        ..Refine::default()
    });
    assert_eq!(
        names(&recent),
        vec!["b.txt", "c.MD", "md-notes/"],
        "a date is files' and folders'"
    );
    let small = run(&Request::new(dir.path())
        .with_contents(Pattern::literal("x"))
        .with_filters(Filters {
            refine: Refine {
                max_size: Some(100),
                ..Refine::default()
            },
            ..Filters::default()
        }))
    .0;
    assert_eq!(content_paths(&small), vec!["a.md"], "the contents' walk refines too");
    assert_eq!(contents(&small)[0].size, Some(10));
}

#[test]
fn every_file_is_listed_with_its_size_and_date() {
    let dir = sample();
    dir.write(".hidden.txt", b"x");
    let cancel = AtomicBool::new(false);
    let mut files = Vec::new();
    let summary = list_files(dir.path(), &Filters::default(), &cancel, &mut |file| {
        files.push(file)
    })
    .expect("the walk starts");
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let listed: Vec<(&str, u64)> = files.iter().map(|f| (f.path.as_str(), f.size)).collect();
    assert_eq!(
        listed,
        vec![
            ("a.txt", 13),
            ("docs/Report.md", 30),
            ("docs/deep/report-2.txt", 30),
            ("src/lib.rs", 33)
        ],
        "files only, the hidden one left out"
    );
    assert!(files.iter().all(|f| f.modified.is_some()));
    assert!(summary.walked >= 8 && !summary.cancelled);
    let stopped = AtomicBool::new(true);
    let mut none = 0;
    list_files(dir.path(), &Filters::default(), &stopped, &mut |_| none += 1)
        .expect("the walk starts");
    assert_eq!(none, 0);
}

#[test]
fn the_files_an_index_names_are_searched_without_a_walk() {
    let dir = sample();
    let request = Request::new(dir.path()).with_contents(Pattern::literal("picked"));
    let cancel = AtomicBool::new(false);
    let mut events = Vec::new();
    let named = [
        String::from("docs/Report.md"),
        String::from("a.txt"),
        String::from("gone.txt"),
    ];
    let summary = search_listed(&request, &named, &cancel, &mut |event| events.push(event))
        .expect("the search starts");
    assert_eq!(content_paths(&events), vec!["docs/Report.md"]);
    assert_eq!((summary.searched, summary.errors), (2, 1), "the gone file is an error");
    let lines = &contents(&events)[0].lines;
    assert_eq!((lines[0].line, lines[0].text.as_str()), (3, "The picked numbers."));
}
