use super::*;
use crate::search::Found;

#[test]
fn a_workspace_folder_is_listed_one_level_and_files_read_and_write_through_the_drive() {
    let dir = temp_dir("drive");
    let root = Root {
        drive_root: dir.clone(),
        prefix: String::new(),
        data_tree: false,
        name: "ws".to_string(),
    };
    let drive = drive_of(&root);
    let written = run_jobs(
        &drive,
        &root.prefix,
        vec![
            DriveJob::Write {
                key: "src/main.rs".to_string(),
                bytes: b"fn main() {}\n".to_vec(),
            },
            DriveJob::Write {
                key: "Cargo.toml".to_string(),
                bytes: b"[package]\n".to_vec(),
            },
        ],
    );
    assert!(written
        .iter()
        .all(|o| matches!(o, DriveOutcome::Written { result: Ok(()), .. })));
    let out = run_jobs(
        &drive,
        &root.prefix,
        vec![
            DriveJob::List { folder: String::new() },
            DriveJob::Read {
                key: "src/main.rs".to_string(),
            },
        ],
    );
    match &out[0] {
        DriveOutcome::Listed {
            folders, files, error, ..
        } => {
            assert_eq!(error, &None);
            assert_eq!(folders, &vec!["src".to_string()]);
            assert_eq!(files, &vec!["Cargo.toml".to_string()]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        out[1],
        DriveOutcome::Read {
            key: "src/main.rs".to_string(),
            result: Ok(b"fn main() {}\n".to_vec())
        }
    );
    assert!(
        !dir.join(azul_storage::manifest::MANIFEST_DIR).exists(),
        "a folder of the user's gets no manifest"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_quick_open_index_walks_the_folders_but_not_build_output_or_version_control() {
    let dir = temp_dir("index");
    let root = Root {
        drive_root: dir.clone(),
        prefix: String::new(),
        data_tree: false,
        name: "ws".to_string(),
    };
    let drive = drive_of(&root);
    let keys = [
        "Cargo.toml",
        "src/main.rs",
        "src/deep/mod.rs",
        "target/debug/out.rs",
        ".git/config",
        "node_modules/x/index.js",
    ];
    let writes = keys
        .iter()
        .map(|k| DriveJob::Write {
            key: (*k).to_string(),
            bytes: b"x".to_vec(),
        })
        .collect();
    let written = run_jobs(&drive, &root.prefix, writes);
    assert!(written
        .iter()
        .all(|o| matches!(o, DriveOutcome::Written { result: Ok(()), .. })));
    let out = run_jobs(&drive, &root.prefix, vec![DriveJob::Index { limit: 100 }]);
    match &out[0] {
        DriveOutcome::Indexed {
            files,
            complete,
            error,
        } => {
            let mut files = files.clone();
            files.sort();
            assert_eq!(files, vec!["Cargo.toml", "src/deep/mod.rs", "src/main.rs"]);
            assert!(*complete);
            assert_eq!(error, &None);
        }
        other => panic!("{other:?}"),
    }
    let capped = run_jobs(&drive, &root.prefix, vec![DriveJob::Index { limit: 2 }]);
    assert!(
        matches!(&capped[0], DriveOutcome::Indexed { files, complete: false, .. } if files.len() == 2),
        "{capped:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_search_of_the_folder_reads_its_text_files_but_not_binaries_or_build_output() {
    let dir = temp_dir("search");
    let root = Root {
        drive_root: dir.clone(),
        prefix: String::new(),
        data_tree: false,
        name: "ws".to_string(),
    };
    let drive = drive_of(&root);
    let files: [(&str, &[u8]); 5] = [
        ("Cargo.toml", b"[package]\nname = \"picked\"\n"),
        ("src/lib.rs", b"pub fn picked() -> u32 {\n    7\n}\n// picked twice\n"),
        ("notes.md", b"nothing here\n"),
        ("logo.bin", b"picked\0\0\0"),
        ("target/out.rs", b"picked\n"),
    ];
    let writes = files
        .iter()
        .map(|(k, b)| DriveJob::Write {
            key: (*k).to_string(),
            bytes: b.to_vec(),
        })
        .collect();
    let _ = run_jobs(&drive, &root.prefix, writes);
    let job = SearchJob {
        query: "picked".to_string(),
        how: TextMatch::default(),
        generation: 1,
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let found = search_files(&root.folder(), &job);
    let mut keys: Vec<(&str, usize)> = found.files.iter().map(|f| (f.key.as_str(), f.hits.len())).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec![("Cargo.toml", 1), ("src/lib.rs", 2)]);
    assert!(found.complete && found.error.is_none());
    let lib = found.files.iter().find(|f| f.key == "src/lib.rs").expect("lib.rs");
    assert_eq!((lib.hits[1].line, lib.hits[1].start), (3, 3));
    job.cancel.store(true, Ordering::Relaxed);
    let stopped = search_files(&root.folder(), &job);
    assert!(stopped.files.is_empty() && !stopped.complete, "a raised cancel stops the search");
    let branch = run_jobs(&drive, &root.prefix, vec![DriveJob::Branch { folder: dir.clone() }]);
    assert_eq!(
        branch,
        vec![DriveOutcome::Branch {
            folder: dir.clone(),
            branch: None
        }],
        "no repository"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A user's folder as the workspace, with `files` written into it.
fn workspace_with(name: &str, files: &[(&str, &[u8])]) -> (PathBuf, Root) {
    let dir = temp_dir(name);
    for (key, bytes) in files {
        let path = dir.join(key);
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("the folder");
        std::fs::write(path, bytes).expect("the file");
    }
    let root = Root {
        drive_root: dir.clone(),
        prefix: String::new(),
        data_tree: false,
        name: "ws".to_string(),
    };
    (dir, root)
}

fn picked() -> SearchJob {
    SearchJob {
        query: "picked".to_string(),
        how: TextMatch::default(),
        generation: 1,
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

/// The editor opens a file as UTF-8 (invalid bytes replaced): a UTF-16 file's match could
/// not be shown where it is, so the search passes the file over as binary (as before the
/// shared engine, which could read it).
#[test]
fn a_search_of_the_folder_passes_over_a_utf16_file_the_editor_cannot_show() {
    let mut utf16 = vec![0xFF, 0xFE];
    for unit in "first line\nthe picked one\n".encode_utf16() {
        utf16.extend_from_slice(&unit.to_le_bytes());
    }
    let (dir, root) = workspace_with(
        "search-utf16",
        &[("notes.txt", &utf16[..]), ("plain.txt", &b"the picked one\n"[..])],
    );
    let found = search_files(&root.folder(), &picked());
    let keys: Vec<&str> = found.files.iter().map(|f| f.key.as_str()).collect();
    assert_eq!(keys, vec!["plain.txt"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A CRLF file's lines and a UTF-8 byte-order mark read as in the editor the match opens in:
/// no `\r` in a preview, no column for the mark; every match of a line is a result, case
/// as asked.
#[test]
fn a_search_of_the_folder_reads_crlf_lines_and_a_bom_as_the_editor_does() {
    let text = "\u{feff}fn main() {\r\n    let picked = 7;\r\n    picked + picked\r\n}\r\n";
    let (dir, root) = workspace_with("search-crlf", &[("main.rs", text.as_bytes())]);
    let found = search_files(&root.folder(), &picked());
    let hits = &found.files[0].hits;
    assert_eq!(hits.len(), 3, "{hits:?}");
    assert_eq!((hits[0].line, hits[0].start, hits[0].end), (1, 8, 14));
    assert_eq!(hits[0].preview, "let picked = 7;", "the indentation and the CR left out");
    assert_eq!(&hits[0].preview[hits[0].preview_start..hits[0].preview_end], "picked");
    assert_eq!(hits[2].found(), Found { line: 2, start: 13, end: 19 });
    let case = SearchJob {
        query: "PICKED".to_string(),
        how: TextMatch {
            match_case: true,
            ..TextMatch::default()
        },
        ..picked()
    };
    assert!(search_files(&root.folder(), &case).files.is_empty(), "case as asked");
    let any = SearchJob {
        query: "PICKED".to_string(),
        ..picked()
    };
    assert_eq!(search_files(&root.folder(), &any).files[0].hits.len(), 3, "any case");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_results_list_the_files_nearest_the_folder_first_in_name_order() {
    let (dir, root) = workspace_with(
        "search-order",
        &[
            ("c.txt", &b"picked\n"[..]),
            ("a/b/c.txt", &b"picked\n"[..]),
            ("b.txt", &b"picked\n"[..]),
            ("a/z.txt", &b"picked\n"[..]),
        ],
    );
    let found = search_files(&root.folder(), &picked());
    let keys: Vec<&str> = found.files.iter().map(|f| f.key.as_str()).collect();
    assert_eq!(keys, vec!["b.txt", "c.txt", "a/z.txt", "a/b/c.txt"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_tasks_are_read_from_the_roots_manifests_and_a_huge_one_is_left_unread() {
    let big = vec![b'#'; TASK_FILE_MAX + 1];
    let (dir, root) = workspace_with(
        "tasks",
        &[
            ("Cargo.toml", b"[package]\nname = \"x\"\n"),
            ("package.json", &big),
            ("src/Makefile", b"deep:\n\ttrue\n"),
        ],
    );
    let drive = drive_of(&root);
    let outcomes = run_jobs(&drive, &root.prefix, vec![DriveJob::TaskFiles]);
    let files = match outcomes.as_slice() {
        [DriveOutcome::TaskFiles { files }] => files.clone(),
        other => panic!("{other:?}"),
    };
    let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["Cargo.toml"], "the manifest over the cap and the one in a subfolder are not read");
    assert!(files[0].1.contains("[package]"));
    let _ = std::fs::remove_dir_all(&dir);
}
