//! Loading a tree of files and cutting each into PAGES.
//!
//! Pagination, not scrolling, is deliberate. Paper gives spatial memory — "the
//! Mutex question was top-right of the third sheet" — and continuous scrolling
//! destroys it. Fixed page boundaries per session mean ink stays where you
//! remember putting it.

use std::path::{Path, PathBuf};

/// Code lines per page. Fixed for the session so page boundaries are stable.
pub const LINES_PER_PAGE: usize = 46;

/// Extensions worth reviewing. Everything else is noise in the file browser.
const REVIEWABLE: &[&str] = &[
    "rs", "toml", "md", "c", "h", "cpp", "hpp", "py", "js", "ts", "sh", "yml", "yaml", "json",
];

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    /// Path shown in the UI, relative to the review root.
    pub display: String,
    pub lines: Vec<String>,
}

impl SourceFile {
    pub fn page_count(&self) -> usize {
        self.lines.len().div_ceil(LINES_PER_PAGE).max(1)
    }

    /// Lines of one page, plus the 1-based number of the first of them.
    pub fn page(&self, page: usize) -> (usize, &[String]) {
        let start = page * LINES_PER_PAGE;
        if start >= self.lines.len() {
            return (start + 1, &[]);
        }
        let end = (start + LINES_PER_PAGE).min(self.lines.len());
        (start + 1, &self.lines[start..end])
    }
}

/// Load a review target: either a whole repo or a single directory.
///
/// One entry point for both because the difference is only which paths get
/// skipped — a git repo has `target/` and `.git/` worth ignoring, a plain
/// directory (say `doc/guide/`) usually has neither.
pub fn load_tree(root: &Path, limit: usize) -> Vec<SourceFile> {
    let mut out = Vec::new();
    walk(root, root, &mut out, limit, 0);
    out.sort_by(|a, b| a.display.cmp(&b.display));
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<SourceFile>, limit: usize, depth: usize) {
    // A repo can hold hundreds of thousands of files; a review session opens a
    // handful. Bound both so a mistyped root cannot hang the UI.
    if out.len() >= limit || depth > 12 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for e in entries {
        if out.len() >= limit {
            return;
        }
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if p.is_dir() {
            walk(root, &p, out, limit, depth + 1);
        } else if is_reviewable(&p) {
            if let Some(f) = load_file(root, &p) {
                out.push(f);
            }
        }
    }
}

fn is_reviewable(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| REVIEWABLE.contains(&e))
}

fn load_file(root: &Path, p: &Path) -> Option<SourceFile> {
    let text = std::fs::read_to_string(p).ok()?;
    // A generated or minified file is one enormous line; rendering it as a
    // page of code is useless and slow.
    if text.lines().count() > 20_000 {
        return None;
    }
    let display = p
        .strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .to_string();
    Some(SourceFile {
        path: p.to_path_buf(),
        display,
        lines: text.lines().map(|l| l.replace('\t', "    ")).collect(),
    })
}
