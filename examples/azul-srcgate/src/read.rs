//! The files a gate reads: every file a build compiles under the roots it names, each as the
//! source and as [`crate::code`].
//!
//! A root is a crate's `src` directory (`examples/azul-storage/src`), one package (its
//! `Cargo.toml` beside `src/`), or a directory of packages (`examples`: each package in it). A
//! package's files are the module trees of its targets (`lib.rs`, `main.rs`, `bin/*.rs`,
//! `bin/*/main.rs`, `build.rs`, `examples/*.rs`, and the `path` of a `[lib]` or a `[[bin]]` of its
//! `Cargo.toml` that stands elsewhere: `src/tray_main.rs`); a file only a test build compiles is a
//! test file and is not read. A root that holds too few files was read in the wrong place and
//! fails.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use crate::{find, lex, tree::module_files};

/// A tree a gate reads, from the base directory the gate runs against (the workspace's root),
/// and the least number of compiled files it holds (a scan that finds fewer read the wrong
/// place).
#[derive(Debug, Clone, Copy)]
pub struct Root {
    pub dir: &'static str,
    pub min_files: usize,
}

/// One compiled file as a gate reads it.
#[derive(Debug, Clone)]
pub struct Code {
    /// From the base directory, `/`-separated.
    pub rel: String,
    pub path: PathBuf,
    /// The file as it is.
    pub source: String,
    /// [`lex::code`] of it: test items and comments blanked, line breaks kept.
    pub text: String,
}

impl Code {
    /// The tokens of the code with their lines.
    #[must_use]
    pub fn tokens(&self) -> Vec<find::Located> {
        find::locate(&self.text)
    }

    /// Line `n` (from 1) of the source, trimmed: what a finding quotes.
    #[must_use]
    pub fn source_line(&self, n: usize) -> &str {
        self.source
            .lines()
            .nth(n.wrapping_sub(1))
            .unwrap_or("")
            .trim()
    }
}

/// How a gate takes the files no build compiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orphans {
    /// A `.rs` file under a root that no build compiles (not even a test build) fails the gate:
    /// no gate can vouch for code that is not in the module tree.
    Fail,
    /// They are left alone (a root that holds generated or vendored trees).
    Ignore,
}

fn canonical(p: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(p).map_err(|e| format!("{}: {e}", p.display()))
}

/// `dir`'s `*.rs` and `*/main.rs` (a target directory's entry files).
fn entry_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for p in rd.flatten().map(|e| e.path()) {
        if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        } else if p.join("main.rs").is_file() {
            out.push(p.join("main.rs"));
        }
    }
    out.sort();
    out
}

/// A crate's targets in `src`: `lib.rs`, `main.rs`, `bin/*.rs`, `bin/*/main.rs`.
fn targets(src: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = ["lib.rs", "main.rs"]
        .iter()
        .map(|t| src.join(t))
        .filter(|p| p.is_file())
        .collect();
    out.extend(entry_files(&src.join("bin")));
    out
}

/// The `path = ".."` of the `[lib]` and the `[[bin]]` tables of a package's `Cargo.toml`, as files
/// below the package: the targets that do not stand where cargo looks for them
/// (`[[bin]] name = "azul-bridge-tray" path = "src/tray_main.rs"`). Read by lines - the manifests
/// of this workspace write a table's keys under its header - so a target the reading cannot
/// follow is left out and not guessed. (`[[test]]`, `[[bench]]` and `[[example]]` tables are not
/// shipped code.)
fn declared_targets(package: &Path) -> Vec<PathBuf> {
    let Ok(manifest) = std::fs::read_to_string(package.join("Cargo.toml")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut in_target = false;
    for line in manifest.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_target = line == "[lib]" || line == "[[bin]]";
            continue;
        }
        if !in_target {
            continue;
        }
        let Some(value) = line
            .strip_prefix("path")
            .and_then(|rest| rest.trim_start().strip_prefix('='))
        else {
            continue;
        };
        let Some(quoted) = value.trim().strip_prefix('"') else {
            continue;
        };
        let Some(end) = quoted.find('"') else {
            continue;
        };
        let target = package.join(&quoted[..end]);
        if target.is_file() {
            out.push(target);
        }
    }
    out
}

/// What a root holds: each crate as (the directory whose `.rs` files must all be compiled, the
/// targets its module trees start from).
fn crates_of(dir: &Path) -> Result<Vec<(PathBuf, Vec<PathBuf>)>, String> {
    let is_src = |d: &Path| d.join("lib.rs").is_file() || d.join("main.rs").is_file();
    let is_package = |p: &Path| p.join("Cargo.toml").is_file() && is_src(&p.join("src"));
    if is_src(dir) {
        // a `src` folder named as the root: the manifest beside it is read for its declared targets
        let mut t = targets(dir);
        for declared in dir.parent().map(declared_targets).unwrap_or_default() {
            if !t.contains(&declared) {
                t.push(declared);
            }
        }
        return Ok(vec![(dir.to_path_buf(), t)]);
    }
    let mut packages: Vec<PathBuf> = if is_package(dir) {
        vec![dir.to_path_buf()]
    } else {
        std::fs::read_dir(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| is_package(p))
            .collect()
    };
    packages.sort();
    if packages.is_empty() {
        return Err(format!(
            "{}: no crate (Cargo.toml, src/lib.rs or src/main.rs) under it",
            dir.display()
        ));
    }
    Ok(packages
        .into_iter()
        .map(|p| {
            let mut t = targets(&p.join("src"));
            if p.join("build.rs").is_file() {
                t.push(p.join("build.rs"));
            }
            t.extend(entry_files(&p.join("examples")));
            for declared in declared_targets(&p) {
                if !t.contains(&declared) {
                    t.push(declared);
                }
            }
            (p.join("src"), t)
        })
        .collect())
}

/// Every `.rs` file under `dir` (a crate's `src`).
fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for p in rd.flatten().map(|e| e.path()) {
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Every file a build compiles under `roots` (outside tests), read, as [`Code`] - or what is
/// wrong: a root that cannot be read, a file that cannot be read, a `mod name;` without a file,
/// (with [`Orphans::Fail`]) a `.rs` file under a root that no build compiles, and a root with
/// fewer than its `min_files`.
///
/// # Errors
///
/// Every finding, as a sentence.
pub fn files(base: &Path, roots: &[Root], orphans: Orphans) -> Result<Vec<Code>, Vec<String>> {
    let mut wrong = Vec::new();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let base = canonical(base).map_err(|e| vec![e])?;
    for root in roots {
        let dir = match canonical(&base.join(root.dir)) {
            Ok(dir) => dir,
            Err(e) => {
                wrong.push(e);
                continue;
            }
        };
        let crates = match crates_of(&dir) {
            Ok(crates) => crates,
            Err(e) => {
                wrong.push(e);
                continue;
            }
        };
        let mut n = 0;
        for (src, crate_targets) in crates {
            let (mut shipped, mut built) = (BTreeSet::new(), BTreeSet::new());
            for t in crate_targets {
                let ship = module_files(&t, false);
                let test = module_files(&t, true);
                wrong.extend(ship.problems);
                wrong.extend(test.problems);
                shipped.extend(ship.files.iter().filter_map(|p| canonical(p).ok()));
                built.extend(test.files.iter().filter_map(|p| canonical(p).ok()));
            }
            if orphans == Orphans::Fail {
                let mut listed = Vec::new();
                rs_files(&src, &mut listed);
                let orphaned: Vec<String> = listed
                    .iter()
                    .filter_map(|p| canonical(p).ok())
                    .filter(|p| !built.contains(p))
                    .map(|p| p.display().to_string())
                    .collect();
                if !orphaned.is_empty() {
                    wrong.push(format!(
                        "{}: files no build compiles, so no gate can vouch for them (declare or \
                         delete them): {orphaned:#?}",
                        src.display()
                    ));
                }
            }
            for path in shipped {
                if !path.starts_with(&dir) {
                    continue;
                }
                n += 1;
                if !seen.insert(path.clone()) {
                    continue;
                }
                let source = match std::fs::read_to_string(&path) {
                    Ok(source) => source,
                    Err(e) => {
                        wrong.push(format!("{}: {e}", path.display()));
                        continue;
                    }
                };
                let rel = path
                    .strip_prefix(&base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = lex::code(&source);
                out.push(Code {
                    rel,
                    path,
                    source,
                    text,
                });
            }
        }
        if n < root.min_files {
            wrong.push(format!(
                "{}: {n} compiled files, fewer than the {} the gate counts on",
                root.dir, root.min_files
            ));
        }
    }
    if wrong.is_empty() {
        Ok(out)
    } else {
        Err(wrong)
    }
}
