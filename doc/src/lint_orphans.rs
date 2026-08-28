//! Two lints for the same failure: code that is correct, compiles, and is
//! never reached.
//!
//! Both were written after a day in which three separate bugs had this exact
//! shape and none of them produced a warning anywhere:
//!
//! - `rust_fontconfig::save_to_disk_cache()` had ZERO callers. The font cache
//!   was therefore never written and every launch re-scanned ~370 system fonts
//!   (186 ms) instead of loading a manifest (10-20 ms).
//! - `azul_dll::unified::map::map_widget_dom` had zero callers while
//!   documenting itself as "the single entry point the FFI `MapWidget::dom()`
//!   shims to". api.json bound `MapWidget.dom` to the azul-layout PLACEHOLDER
//!   instead, so on every desktop platform the map panned and never painted a
//!   single tile.
//! - `video_widget_dom` was the same bug, one widget over: the video widget
//!   rendered its built-in test pattern forever and never decoded anything.
//!
//! A compiler cannot see any of this. `pub` silences dead-code analysis, and
//! the binding that decides which function ships is a STRING in api.json.
//!
//! ## What is checked
//!
//! 1. [`orphaned_wiring_functions`] — the specific shape. A widget whose worker
//!    cannot live in azul-layout (it drags a dependency tree the mobile builds
//!    must not carry) gets a `*_widget_dom` wiring function in azul-dll. If
//!    api.json does not route that widget's `dom` through it, the widget ships
//!    inert.
//! 2. [`unreferenced_public_fns`] — the general shape, in the spirit of Go's
//!    unused-symbol error. A `pub fn` under the dll's integration modules that
//!    nothing in the workspace mentions is either dead or unwired; both are
//!    worth a human deciding about.

use std::{collections::BTreeSet, path::Path};

/// One finding, formatted by the caller.
#[derive(Debug, Clone)]
pub struct Orphan {
    /// Repo-relative file the symbol is defined in.
    pub file: String,
    /// 1-based line of the definition.
    pub line: usize,
    /// The symbol name.
    pub symbol: String,
    /// Why this is a problem, in one sentence.
    pub why: String,
}

/// Directories whose `pub fn`s are integration glue: they exist to be called
/// from api.json or from a sibling module, never by the outside world.
///
/// Deliberately NOT the whole crate. `dll/src` is full of `pub` items that are
/// FFI surface or re-exports, and flagging those would make the lint noise that
/// everyone learns to skip — which is the failure mode this exists to fix.
const GLUE_DIRS: &[&str] = &["dll/src/unified", "dll/src/desktop/extra"];

/// Walk `dir` collecting `.rs` files.
fn rs_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Every `.rs` file in the workspace that could reference a symbol, plus
/// api.json (whose `fn_body` strings are call sites the compiler never sees).
fn haystack(root: &Path) -> String {
    let mut buf = String::new();
    for dir in [
        "dll/src",
        "layout/src",
        "core/src",
        "css/src",
        "doc/src",
        "examples",
    ] {
        let mut files = Vec::new();
        rs_files(&root.join(dir), &mut files);
        for f in files {
            if let Ok(s) = std::fs::read_to_string(&f) {
                buf.push_str(&s);
                buf.push('\n');
            }
        }
    }
    if let Ok(s) = std::fs::read_to_string(root.join("api.json")) {
        buf.push_str(&s);
    }
    buf
}

/// Count references to `name` that are not its own definition and not a doc
/// mention. A doc comment saying "call `foo`" is exactly how `map_widget_dom`
/// looked correct while being unreachable, so those do not count.
fn reference_count(hay: &str, name: &str) -> usize {
    let mut n = 0;
    for line in hay.lines() {
        let t = line.trim_start();
        if t.starts_with("//") || t.starts_with("*") || t.starts_with("#!") {
            continue;
        }
        if t.contains(&format!("pub fn {name}")) || t.contains(&format!("fn {name}")) {
            continue; // the definition itself
        }
        if line.contains(name) {
            n += 1;
        }
    }
    n
}

/// A `*_widget_dom` in azul-dll that api.json never routes a widget's `dom`
/// through. See the module docs for why these exist at all.
#[must_use]
pub fn orphaned_wiring_functions(root: &Path) -> Vec<Orphan> {
    let api = std::fs::read_to_string(root.join("api.json")).unwrap_or_default();
    let mut out = Vec::new();
    let mut files = Vec::new();
    for dir in GLUE_DIRS {
        rs_files(&root.join(dir), &mut files);
    }
    for f in files {
        let Ok(src) = std::fs::read_to_string(&f) else {
            continue;
        };
        for (i, line) in src.lines().enumerate() {
            let t = line.trim_start();
            if !t.starts_with("pub fn ") {
                continue;
            }
            let Some(rest) = t.strip_prefix("pub fn ") else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.ends_with("_widget_dom") {
                continue;
            }
            // The binding that matters is a string inside api.json.
            if !api.contains(&name) {
                out.push(Orphan {
                    file: f.strip_prefix(root).unwrap_or(&f).display().to_string(),
                    line: i + 1,
                    symbol: name.clone(),
                    why: format!(
                        "api.json never routes a widget's `dom` through `{name}`, so the \
                         widget ships wired to azul-layout's placeholder: it renders and \
                         then never receives data, silently, on every platform"
                    ),
                });
            }
        }
    }
    out
}

/// A `pub fn` in the dll's integration glue that nothing in the workspace —
/// Rust source or api.json `fn_body` — mentions.
///
/// `extern "C"` / `#[no_mangle]` items are skipped: those ARE the FFI surface
/// and their callers are in another language entirely.
///
/// Symbols listed in `doc/orphan_allowlist.txt` are known and recorded with a
/// reason; anything NOT listed is an error, so a new orphan cannot join quietly.
#[must_use]
pub fn unreferenced_public_fns(root: &Path) -> Vec<Orphan> {
    let allow: BTreeSet<String> = std::fs::read_to_string(root.join("doc/orphan_allowlist.txt"))
        .unwrap_or_default()
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let hay = haystack(root);
    let mut out = Vec::new();
    let mut files = Vec::new();
    for dir in GLUE_DIRS {
        rs_files(&root.join(dir), &mut files);
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for f in files {
        let Ok(src) = std::fs::read_to_string(&f) else {
            continue;
        };
        let lines: Vec<&str> = src.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim_start();
            if !t.starts_with("pub fn ") && !t.starts_with("pub const fn ") {
                continue;
            }
            // Skip the FFI surface: its callers are not in this repo.
            let prev = i.saturating_sub(6)..i;
            if lines[prev]
                .iter()
                .any(|l| l.contains("no_mangle") || l.contains("extern \"C\""))
                || t.contains("extern \"C\"")
            {
                continue;
            }
            let after = t.trim_start_matches("pub ").trim_start_matches("const ");
            let Some(rest) = after.strip_prefix("fn ") else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() || !seen.insert(name.clone()) {
                continue;
            }
            if reference_count(&hay, &name) == 0 && !allow.contains(&name) {
                out.push(Orphan {
                    file: f.strip_prefix(root).unwrap_or(&f).display().to_string(),
                    line: i + 1,
                    symbol: name.clone(),
                    why: format!(
                        "`{name}` is public and nothing in the workspace or api.json calls \
                         it — either wire it up or delete it (`pub` silences the compiler's \
                         dead-code check, so this is the only place it can be caught)"
                    ),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("doc/ has a parent")
            .to_path_buf()
    }

    /// The lint that would have caught the map and video bugs must pass NOW —
    /// both are wired, so the tree is clean.
    #[test]
    fn no_wiring_function_is_orphaned_today() {
        let found = orphaned_wiring_functions(&root());
        assert!(
            found.is_empty(),
            "orphaned widget wiring: {:?}",
            found.iter().map(|o| &o.symbol).collect::<Vec<_>>()
        );
    }

    /// Prints what the general lint sees. A lint that fires fifty times is a
    /// lint everyone learns to skip, so its finding count is part of its
    /// contract, not an afterthought.
    #[test]
    fn the_general_lint_stays_small_enough_to_read() {
        let found = unreferenced_public_fns(&root());
        for o in &found {
            println!("  UNREFERENCED {}:{} {}", o.file, o.line, o.symbol);
        }
        assert!(
            found.is_empty(),
            "{} unreferenced public fn(s) not in doc/orphan_allowlist.txt. Wire them \
             up, delete them, or add them to the allowlist WITH a reason.",
            found.len()
        );
    }

    /// A doc comment naming the function must NOT count as a reference. That is
    /// precisely how `map_widget_dom` looked reachable for months: its own doc
    /// said it was the entry point, and nothing called it.
    #[test]
    fn a_doc_mention_is_not_a_call_site() {
        let hay = "/// call `wire_me` for this\n// see wire_me\npub fn wire_me() {}\n";
        assert_eq!(reference_count(hay, "wire_me"), 0);
        let hay2 = "pub fn wire_me() {}\nlet x = wire_me();\n";
        assert_eq!(reference_count(hay2, "wire_me"), 1);
    }
}
