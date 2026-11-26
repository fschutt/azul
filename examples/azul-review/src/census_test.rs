//! Counting the workspace: every folder listed from the root down, the lines of every file counted, by the rules of the tree.
//! A `MemSource` stands for the disk.

use std::sync::atomic::AtomicBool;

use super::*;
use crate::symbols_source::MemSource;

fn source(files: &[(&str, &str)]) -> MemSource {
    let mut source = MemSource::new();
    for (path, text) in files {
        source.put(path, text.as_bytes().to_vec());
    }
    source
}

fn batches(source: &MemSource) -> Vec<Batch> {
    let stop = AtomicBool::new(false);
    let mut out = Vec::new();
    walk(source, &stop, &mut |b| out.push(b));
    out
}

#[test]
fn every_folder_is_listed_once_from_the_root_down_and_the_lines_of_its_files_are_counted() {
    let s = source(&[
        ("README.md", "a\nb\nc\n"),
        ("src/lib.rs", "x\ny\n"),
        ("src/deep/b.rs", "z\n"),
        ("target/x.rs", "skipped\n"),
        (".hidden/y.rs", "skipped\n"),
        ("image.png", "not text"),
    ]);
    let got = batches(&s);
    let dirs: Vec<&str> = got.iter().map(|b| b.dir.as_str()).collect();
    assert_eq!(dirs, vec!["", "src", "src/deep"], "a folder the rules leave out is not walked");
    let lines: Vec<&[(String, u32)]> = got.iter().map(|b| b.lines.as_slice()).collect();
    assert_eq!(lines[0], [("README.md".to_string(), 3)]);
    assert_eq!(lines[1], [("lib.rs".to_string(), 2)]);
    assert_eq!(lines[2], [("b.rs".to_string(), 1)]);
    assert!(got[0].entries.iter().any(|e| e.name == "image.png"), "the listing is told whole: the tree applies the rules itself");
}

#[test]
fn lines_are_counted_as_a_page_shows_them() {
    assert_eq!(count_lines(b""), 0);
    assert_eq!(count_lines(b"one"), 1);
    assert_eq!(count_lines(b"one\n"), 1);
    assert_eq!(count_lines(b"a\nb"), 2);
    assert_eq!(count_lines(b"a\r\nb\r\n"), 2);
    assert_eq!(count_lines(b"\n"), 1);
    assert_eq!(count_lines(b"\n\n\n"), 3);
}

#[test]
fn a_file_the_window_would_not_show_has_no_lines() {
    let mut s = MemSource::new();
    s.put("bin.rs", b"\x7fELF\0\0\0\0".to_vec());
    s.put("big.rs", vec![b'a'; crate::code::MAX_OPEN_BYTES + 1]);
    s.put("ok.rs", b"fn a() {}\n".to_vec());
    let got = batches(&s);
    let lines: std::collections::BTreeMap<&str, u32> = got[0].lines.iter().map(|(n, l)| (n.as_str(), *l)).collect();
    assert_eq!(lines["bin.rs"], 0, "no text");
    assert_eq!(lines["big.rs"], 0, "over what a window holds");
    assert_eq!(lines["ok.rs"], 1);
}

#[test]
fn the_walk_stops_when_it_is_told_to_and_says_nothing_after() {
    let s = source(&[("a.rs", "x\n"), ("d1/b.rs", "x\n"), ("d1/d2/c.rs", "x\n")]);
    let stop = AtomicBool::new(false);
    let mut seen = Vec::new();
    let counted = walk(&s, &stop, &mut |b| {
        seen.push(b.dir);
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    assert_eq!(seen, vec![String::new()], "one folder, then the stop was heard");
    assert_eq!(counted.dirs, 1);
}

#[test]
fn what_is_counted_is_told_at_the_end() {
    let s = source(&[("a.rs", "x\ny\n"), ("d/b.rs", "x\n")]);
    let stop = AtomicBool::new(false);
    let counted = walk(&s, &stop, &mut |_| {});
    assert_eq!((counted.dirs, counted.files, counted.lines), (2, 2, 3));
}

#[test]
fn the_walk_and_the_tree_agree_on_what_is_in_a_workspace() {
    // the rules are one function: the files the walk counts are the files the tree lists when every folder is opened
    let files = [
        ("Cargo.toml", "[package]\n"),
        ("build/out.rs", "x\n"),
        ("src/lib.rs", "x\n"),
        ("src/build/b.rs", "x\n"),
        ("docs/guide.md", "x\n"),
        ("node_modules/m.js", "x\n"),
        ("data.bin", "x\n"),
        (".git/config", "x\n"),
    ];
    let s = source(&files);
    let mut walked: Vec<String> = Vec::new();
    for b in batches(&s) {
        for (name, _) in b.lines {
            walked.push(if b.dir.is_empty() { name } else { format!("{}/{name}", b.dir) });
        }
    }
    walked.sort();
    let mut state = crate::AppState::create(std::path::PathBuf::new(), Vec::new(), azul::prelude::RefAny::new(0u8), std::path::PathBuf::new());
    state.source = std::sync::Arc::new(s);
    state.start_workspace();
    state.expand_all();
    let mut tree_files = state.tree.file_paths(1_000);
    tree_files.sort();
    assert_eq!(walked, tree_files);
}
