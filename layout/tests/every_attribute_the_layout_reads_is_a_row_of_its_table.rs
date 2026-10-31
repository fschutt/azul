//! Every attribute the layout reads is a row of its table.
//!
//! What the layout reads of a node must be what the change detection of a
//! rebuilt DOM looks at: the reconcile, the per-node caches and the cached
//! display list all key on the node fingerprints, so an attribute they do
//! not hash is one whose change serves the old layout. The table of
//! attributes the layout reads (`azul_core::layout_inputs::LayoutAttribute`)
//! is what the fingerprints hash, each row by what a change of it moves, and
//! the layout reads attributes only through it
//! (`NodeData::layout_attributes`). This test reads the sources that lay out
//! and paint, and fails on a read that goes around the table.

use std::path::{Path, PathBuf};

/// Reads of a node's attributes that name no row of the table.
const RAW_READS: &[&str] = &[".attributes()", ".get_attributes()", ".get_attribute("];

/// The source directories that lay out and paint: what they read is cached.
const LAYOUT_DIRS: &[&str] = &["src/solver3", "src/text3", "src/cpurender"];

/// Every `.rs` file under `dir`.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Whether `path` holds tests only.
fn is_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with("_tests.rs") || name.ends_with("_test.rs") || name == "autotest_generated.rs"
        })
}

/// How many lines of `lines` come before the file's test module: a
/// `#[cfg(test)]` on a `mod` with a body ends what the layout runs.
fn production_len(lines: &[&str]) -> usize {
    (0..lines.len())
        .find(|&i| {
            lines[i].trim() == "#[cfg(test)]"
                && lines[i + 1..]
                    .iter()
                    .map(|line| line.trim())
                    .find(|line| !line.starts_with("#["))
                    .is_some_and(|item| {
                        let item = item
                            .strip_prefix("pub(crate) ")
                            .or_else(|| item.strip_prefix("pub "))
                            .unwrap_or(item);
                        item.starts_with("mod ") && item.ends_with('{')
                    })
        })
        .unwrap_or(lines.len())
}

#[test]
fn the_layout_reads_attributes_only_through_their_table_rows() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for dir in LAYOUT_DIRS {
        rust_files(&root.join(dir), &mut files);
    }
    files.retain(|path| !is_test_file(path));
    files.sort();
    assert!(
        files.len() > 20,
        "harness: the layout's sources are found ({} files)",
        files.len()
    );

    let mut raw = Vec::new();
    let mut through_the_table = 0usize;
    for path in &files {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let lines: Vec<&str> = source.lines().collect();
        for (number, line) in lines[..production_len(&lines)].iter().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            if RAW_READS.iter().any(|read| code.contains(read)) {
                raw.push(format!(
                    "{}:{}: {}",
                    path.strip_prefix(root).unwrap_or(path).display(),
                    number + 1,
                    line.trim()
                ));
            }
            if code.contains("layout_attribute") {
                through_the_table += 1;
            }
        }
    }
    assert!(
        raw.is_empty(),
        "the layout reads attributes that are no row of its table \
         (read them through `NodeData::layout_attributes` / `layout_attribute`, \
         and give a new one its row in `LayoutAttribute`):\n{}",
        raw.join("\n")
    );
    assert!(
        through_the_table > 0,
        "harness: the layout reads its attributes through the table"
    );
}
