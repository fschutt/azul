//! Guide link lint: every pointer in `doc/guide/` must land on something that
//! exists, checked against the worktree rather than against memory.
//!
//! This is the local half of the documentation-drift gate. CI also runs
//! `docproof`, which asks git what happened to a documented path and can
//! therefore say *"moved to `core/src/compact.rs` in a0d295796"* — a claim
//! that needs full history and a network checkout. This lint needs neither,
//! so it runs in `azul-doc check` and in `cargo test -p azul-doc`, which is
//! where a contributor meets it before pushing. The two overlap deliberately
//! on stale source paths and diverge either side of that:
//!
//!   - docproof proves DRIFT from history: a path that once existed and moved.
//!     A path that never existed is not a finding there, because there is no
//!     deletion to point at.
//!   - this lint proves RESOLUTION against the tree: a `[text](../dom.md)`
//!     whose page is not there, an image that is not on disk, a `#anchor` with
//!     no heading behind it, a typo'd source path. No history involved, so a
//!     path invented yesterday is caught just as well as one that rotted.
//!
//! What it deliberately does NOT check: external `http(s)` links. That would
//! put the network in the build, and a rate-limited host would fail a run for
//! a reason no contributor could act on.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

/// One dangling pointer, with enough to jump straight to it.
pub struct Problem {
    /// Repo-relative, so the printed line is clickable from the repo root.
    pub file: String,
    pub line: usize,
    pub detail: String,
}

/// Directories that are build output or vendored, never a documented path.
const NOT_SOURCE: &[&str] = &["target", "node_modules"];

/// Guide subdirectories holding generated assets rather than pages.
const NOT_PAGES: &[&str] = &["screenshots", "target"];

/// The whole check. `project_root` is the repository root (the parent of
/// `doc/`), because a guide sentence naming `core/src/compact.rs` means that
/// path from the root, not from the page.
pub fn check_guide_links(project_root: &Path) -> Vec<Problem> {
    let guide_root = project_root.join("doc").join("guide").join("en");
    let mut problems = Vec::new();
    if !guide_root.is_dir() {
        return problems;
    }

    let pages = collect_pages(&guide_root);
    let page_slugs: BTreeSet<String> = pages.iter().map(|(slug, _)| slug.clone()).collect();
    let top_level = top_level_dirs(project_root);

    // Heading anchors are computed lazily and once per page: a page linked
    // from twenty others still renders once.
    let mut anchors: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (slug, path) in &pages {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let rel = format!("doc/guide/en/{slug}.md");
        let dir = slug.rsplit_once('/').map_or("", |(d, _)| d);

        for (lineno, line) in prose_lines(&text) {
            for target in link_targets(line) {
                check_link(
                    &target,
                    dir,
                    &rel,
                    slug,
                    lineno,
                    &page_slugs,
                    &pages,
                    &guide_root,
                    path,
                    &mut anchors,
                    &mut problems,
                );
            }
            for claim in path_claims(line, &top_level) {
                if !project_root.join(&claim).exists() {
                    problems.push(Problem {
                        file: rel.clone(),
                        line: lineno,
                        detail: format!(
                            "`{claim}` is not in the worktree (a repo path that moved or was \
                             deleted, or a typo)"
                        ),
                    });
                }
            }
        }
    }

    problems.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    problems
}

#[allow(clippy::too_many_arguments)]
fn check_link(
    target: &str,
    dir: &str,
    rel: &str,
    slug: &str,
    lineno: usize,
    page_slugs: &BTreeSet<String>,
    pages: &[(String, PathBuf)],
    guide_root: &Path,
    page_path: &Path,
    anchors: &mut BTreeMap<String, BTreeSet<String>>,
    problems: &mut Vec<Problem>,
) {
    // The network stays out of the build; `mailto:` has nothing to resolve.
    if target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
    {
        return;
    }

    let (path, frag) = match target.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (target, None),
    };

    // A bare `#anchor` points inside this same page.
    if path.is_empty() {
        if let Some(frag) = frag {
            let set = page_anchors(slug, page_path, anchors);
            if !set.contains(frag) {
                problems.push(Problem {
                    file: rel.to_string(),
                    line: lineno,
                    detail: format!("`#{frag}` matches no heading on this page"),
                });
            }
        }
        return;
    }

    // `/guide/<slug>` — the shape `rewrite_md_links` emits, and the shape a
    // hand-written cross-link to the deployed site takes.
    if let Some(rest) = path.strip_prefix('/') {
        if let Some(slug_part) = rest.strip_prefix("guide/") {
            let wanted = slug_part.trim_end_matches('/');
            if !page_slugs.contains(wanted) {
                problems.push(Problem {
                    file: rel.to_string(),
                    line: lineno,
                    detail: format!("`/guide/{wanted}` is not a guide page"),
                });
            }
        }
        // Any other site-absolute link (`/api`, `/releases`) is a route this
        // lint cannot see, and guessing would produce noise.
        return;
    }

    if let Some(stem) = path.strip_suffix(".md") {
        let resolved = resolve(dir, stem);
        if !page_slugs.contains(&resolved) {
            problems.push(Problem {
                file: rel.to_string(),
                line: lineno,
                detail: format!("`{path}` resolves to `{resolved}`, which is not a guide page"),
            });
            return;
        }
        if let Some(frag) = frag {
            if let Some((_, target_path)) = pages.iter().find(|(s, _)| *s == resolved) {
                let set = page_anchors(&resolved, target_path, anchors);
                if !set.contains(frag) {
                    problems.push(Problem {
                        file: rel.to_string(),
                        line: lineno,
                        detail: format!("`{path}#{frag}` matches no heading on that page"),
                    });
                }
            }
        }
        return;
    }

    // Everything else relative is a file on disk next to the page: an image, a
    // downloadable, a screenshot — or a source file in the repository, which a
    // hello-world page reaches with enough `../` to climb out of the guide
    // entirely. Resolved against the page's own directory rather than as a
    // slug, so climbing past `doc/guide/en/` lands where the reader's browser
    // and editor would put it.
    let mut on_disk = page_path.parent().unwrap_or(guide_root).to_path_buf();
    for seg in path.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                on_disk.pop();
            }
            s => on_disk.push(s),
        }
    }
    if !on_disk.exists() {
        problems.push(Problem {
            file: rel.to_string(),
            line: lineno,
            detail: format!("`{path}` resolves to no file on disk"),
        });
    }
}

/// Heading anchors as the deployed page will actually carry them.
///
/// Rendered through comrak with the same `header_ids` setting
/// `generate_guide_html` uses, and the ids read back out of the emitted
/// anchor tags — so this cannot drift from what the site serves the way a
/// reimplemented slug function would.
fn page_anchors<'a>(
    slug: &str,
    path: &Path,
    cache: &'a mut BTreeMap<String, BTreeSet<String>>,
) -> &'a BTreeSet<String> {
    if !cache.contains_key(slug) {
        let text = fs::read_to_string(path).unwrap_or_default();
        let body = match crate::reftest::autodoc::parse_frontmatter(&text) {
            Some((_, body)) => body,
            None => text,
        };
        let html = comrak::markdown_to_html(
            &body,
            &comrak::Options {
                extension: comrak::ExtensionOptions {
                    header_ids: Some(String::new()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let mut found = BTreeSet::new();
        // comrak emits exactly `class="anchor" id="<prefix><id>"`; the prefix
        // is empty here, as it is in the generator.
        for chunk in html.split("class=\"anchor\" id=\"").skip(1) {
            if let Some(end) = chunk.find('"') {
                found.insert(chunk[..end].to_string());
            }
        }
        cache.insert(slug.to_string(), found);
    }
    &cache[slug]
}

/// `../dom` seen from `events/callbacks` is `dom`. Textual, like the
/// generator's own resolver — the guide has no symlinks.
fn resolve(dir: &str, target: &str) -> String {
    let mut segs: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for seg in target.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                segs.pop();
            }
            s => segs.push(s),
        }
    }
    segs.join("/")
}

/// Lines outside fenced code blocks, numbered from 1.
///
/// A fence is where the examples live, and an example is allowed to name a
/// file that does not exist here — `cbits/azul_shims.c` in the Haskell page
/// belongs to the READER's project, not to this repository.
fn prose_lines(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut fence: Option<char> = None;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let opener = if trimmed.starts_with("```") {
            Some('`')
        } else if trimmed.starts_with("~~~") {
            Some('~')
        } else {
            None
        };
        match (fence, opener) {
            (None, Some(c)) => fence = Some(c),
            // Only the same fence character closes: a ``` inside a ~~~ block
            // is content.
            (Some(open), Some(c)) if open == c => fence = None,
            _ => {}
        }
        if fence.is_none() && opener.is_none() {
            out.push((i + 1, line));
        }
    }
    out
}

/// Every `](target)` on the line, link and image alike.
///
/// Hand-rolled rather than a regex because the label may itself contain
/// brackets, and only the `](` seam matters.
fn link_targets(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            let start = i + 2;
            if let Some(len) = line[start..].find(')') {
                let inner = &line[start..start + len];
                // `[text](url "title")` — the title is not a target.
                let target = inner.split_whitespace().next().unwrap_or("");
                if !target.is_empty() && !inside_code_span(line, i) {
                    out.push(target.to_string());
                }
                i = start + len;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Is byte `pos` inside a backtick span? An odd number of backticks before it
/// on the line means yes — which is how `` `s.cstring[](p)` `` in the Nim page
/// stays out of the link set.
fn inside_code_span(line: &str, pos: usize) -> bool {
    line[..pos].matches('`').count() % 2 == 1
}

/// Backticked tokens on the line that CLAIM to be a path in this repository.
///
/// The test is the first segment: a token starting with a real top-level
/// directory (`core/`, `doc/`, `examples/`, `scripts/`) is a claim about this
/// tree and must resolve. A token starting with anything else — `cbits/…`,
/// `.azul/…`, `src/main.rs` — names the reader's project, or a file inside
/// another crate's layout, and this lint has no business judging it.
fn path_claims(line: &str, top_level: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let close = match after.find('`') {
            Some(c) => c,
            None => break,
        };
        let token = after[..close].trim();
        rest = &after[close + 1..];

        // `path/to/thing.rs:83` — a line number is a pointer at the file, not
        // part of its name. Ranges too.
        let token = token
            .trim_end_matches(|c: char| c == '.' || c == ',' || c == ';' || c == ')')
            .split(':')
            .next()
            .unwrap_or("");
        if token.is_empty() || !token.contains('/') || token.contains(char::is_whitespace) {
            continue;
        }
        // Placeholders (`examples/cpp/cpp<NN>/`, `guide/<lang>/`) and globs
        // describe a shape, not a file.
        if token.contains(['<', '>', '*', '{', '}', '$', '?']) {
            continue;
        }
        let first = token.split('/').next().unwrap_or("");
        if top_level.contains(first) {
            out.push(token.trim_end_matches('/').to_string());
        }
    }
    out
}

fn top_level_dirs(project_root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if let Ok(entries) = fs::read_dir(project_root) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || NOT_SOURCE.contains(&name.as_str()) {
                continue;
            }
            out.insert(name);
        }
    }
    out
}

/// Every guide page as `(slug, path)`, slug being the deployed name
/// (`events/callbacks`).
fn collect_pages(guide_root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    walk(guide_root, guide_root, &mut out);
    out.sort();
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if NOT_PAGES.contains(&name) {
                continue;
            }
            walk(root, &p, out);
        } else if p.extension().map(|e| e == "md").unwrap_or(false) {
            if let Ok(rel) = p.strip_prefix(root) {
                let slug = rel.with_extension("").to_string_lossy().replace('\\', "/");
                out.push((slug, p));
            }
        }
    }
}

#[cfg(test)]
mod guide_link_contract {
    use super::*;

    fn project_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    /// The gate itself. A page that points at a file, a page, or a heading
    /// that is not there is a broken link on the deployed site, and the
    /// contributor who moved the thing is the one who can still fix it
    /// cheaply.
    #[test]
    fn every_guide_pointer_resolves() {
        let problems = check_guide_links(&project_root());
        let rendered: Vec<String> = problems
            .iter()
            .map(|p| format!("{}:{}: {}", p.file, p.line, p.detail))
            .collect();
        assert!(
            rendered.is_empty(),
            "dangling pointers in doc/guide/en:\n  {}",
            rendered.join("\n  ")
        );
    }

    #[test]
    fn fenced_examples_are_not_claims() {
        let md = "prose `core/src/nope.rs`\n```\nfenced `core/src/also_nope.rs`\n```\n";
        let lines = prose_lines(md);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].0, 1);
    }

    #[test]
    fn only_repo_rooted_tokens_are_claims() {
        let top: BTreeSet<String> = ["core", "doc"].iter().map(|s| s.to_string()).collect();
        let claims = path_claims(
            "see `core/src/compact.rs:83` and `cbits/shim.c` and `doc/guide/` and `x<N>/y.rs`",
            &top,
        );
        assert_eq!(claims, vec!["core/src/compact.rs", "doc/guide"]);
    }

    #[test]
    fn a_link_inside_a_code_span_is_not_a_link() {
        assert_eq!(link_targets("text [a](../dom.md) more"), vec!["../dom.md"]);
        assert!(link_targets("`s.cstring[](p)`").is_empty());
        assert_eq!(link_targets("[a](x.png \"title\")"), vec!["x.png"]);
    }

    #[test]
    fn relative_targets_resolve_like_the_generator() {
        assert_eq!(resolve("events", "../dom"), "dom");
        assert_eq!(resolve("events", "callbacks"), "events/callbacks");
        assert_eq!(resolve("", "layout/flex"), "layout/flex");
    }
}
