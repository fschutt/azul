use std::path::{Path, PathBuf};

pub const LINES_PER_PAGE: usize = 46;

const REVIEWABLE: &[&str] = &[
    "rs", "toml", "md", "c", "h", "cpp", "hpp", "py", "js", "ts", "sh", "yml", "yaml", "json",
];

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    pub display: String,
    pub lines: Vec<String>,
}

impl SourceFile {
    pub fn page_count(&self) -> usize {
        self.lines.len().div_ceil(LINES_PER_PAGE).max(1)
    }

    pub fn page(&self, page: usize) -> (usize, &[String]) {
        let start = page * LINES_PER_PAGE;
        if start >= self.lines.len() {
            return (start + 1, &[]);
        }
        let end = (start + LINES_PER_PAGE).min(self.lines.len());
        (start + 1, &self.lines[start..end])
    }
}

pub fn load_tree(root: &Path, limit: usize) -> Vec<SourceFile> {
    let mut out = Vec::new();
    walk(root, root, &mut out, limit, 0);
    out.sort_by(|a, b| a.display.cmp(&b.display));
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<SourceFile>, limit: usize, depth: usize) {
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
