//! The rewrite pass: `azul-doc autoreview --rewrite-source[=KINDS]`.
//!
//! It is the normal autoreview loop (one agent per file, resumable, with
//! `--status` and `--retry-failed`) given a different job. A low-cost model
//! lists, one line each, what a stronger reviewer should look at (crash risks,
//! global state, allocations, untested code, newer idioms, ...) and makes
//! only the edits the kinds allow. By default that is comments alone; the
//! opt-in kinds add blank lines, renames of too-short locals, and rustfmt with
//! the repository's rustfmt.toml before and after the agent. When the agent
//! is done its edit goes through `style_guard`; anything else puts the file
//! back as the agent found it. `autoreview merge --rewrite-source` then turns
//! the per-file reports into one todo list, without another model.
//!
//! The kinds `idioms` and `everything` may change logic, so nothing can check
//! them: they run on a handful of named files as a preview of how a file
//! looks once its findings are fixed, with a test written first for each
//! change.
//!
//! Each queued file has its prompt `<name>.md` in the run's prompt folder and
//! these files next to it: `<name>.md.source` (the file's path), and while an
//! agent runs `<name>.md.orig.rs` (the file as the agent found it). A rejected
//! edit is kept as `<name>.md.rejected.rs` for a look.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use super::style_guard::{self, LineMap};

/// The model for this pass unless `--model` names another.
pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
/// Agents at once unless `--agents` says otherwise. Each `claude` process
/// holds a few hundred MB, so twenty would not fit on an 8 GB machine.
pub const DEFAULT_AGENTS: usize = 6;
/// Larger files are listed instead of queued: one agent would have to hold
/// the whole file, which costs more per edit and loses track. Raise it with
/// `--max-bytes` for a deliberate run on the big files.
pub const DEFAULT_MAX_BYTES: u64 = 256 * 1024;

/// Top-level folders whose tracked `.rs` files the pass covers.
const SCOPE_ROOTS: &[&str] = &["core", "css", "layout", "dll", "doc", "examples"];
/// Folders skipped at any depth: build output, vendored code, and fixtures
/// and golden files whose exact bytes other tests compare against.
const SKIP_DIRS: &[&str] = &["target", "vendor", "codegen_goldens", "goldens", "fixtures", "testdata"];

/// Kinds that may change logic run on at most this many files, named with
/// `--file`: nothing checks their edits, so they are a preview, not a pass.
pub const MAX_LOGIC_FILES: usize = 5;

/// Finding categories, in the order the merged list shows them.
pub const CATEGORIES: &[&str] = &[
    "bug",
    "overflow",
    "panic",
    "unsafe",
    "global-state",
    "alloc",
    "inline-css",
    "const-static",
    "architecture",
    "duplication",
    "dead-code",
    "superseded",
    "untested",
    "empty-test",
    "idiom",
    "paths",
    "naming",
    "todo",
    "style",
    "other",
];

/// What a rewrite run may change in a file (`--rewrite-source=KINDS`).
/// Everything else the agent notices becomes a finding, never an edit.
#[derive(Clone, Debug)]
pub struct Kinds {
    /// Rewrite comments: the default when no kinds are named.
    pub comments: bool,
    /// Add blank lines between the logical steps of long functions.
    pub newlines: bool,
    /// Rename too-short local variables (checked as clean renames).
    pub variables: bool,
    /// Run rustfmt with the root rustfmt.toml before and after the agent.
    pub fmt: bool,
    /// Apply the `idiom` findings, each with a test first. Not checked.
    pub idioms: bool,
    /// Apply every finding, each with a test first: a preview of the file
    /// after the review. Not checked.
    pub everything: bool,
}

impl Default for Kinds {
    fn default() -> Self {
        Kinds::none().with_comments()
    }
}

impl Kinds {
    /// Every name `--rewrite-source=` accepts.
    pub const NAMES: &'static [&'static str] =
        &["comments", "newlines", "variables", "fmt", "idioms", "everything"];

    fn none() -> Kinds {
        Kinds {
            comments: false,
            newlines: false,
            variables: false,
            fmt: false,
            idioms: false,
            everything: false,
        }
    }

    fn with_comments(mut self) -> Kinds {
        self.comments = true;
        self
    }

    /// Parses `comments,newlines,...`; `everything` turns on all kinds.
    pub fn parse(list: &str) -> Result<Kinds, String> {
        let mut kinds = Kinds::none();
        for name in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            match name {
                "comments" => kinds.comments = true,
                "newlines" => kinds.newlines = true,
                "variables" => kinds.variables = true,
                "fmt" => kinds.fmt = true,
                "idioms" => kinds.idioms = true,
                "everything" => {
                    kinds = Kinds {
                        comments: true,
                        newlines: true,
                        variables: true,
                        fmt: true,
                        idioms: true,
                        everything: true,
                    }
                }
                other => {
                    return Err(format!(
                        "unknown rewrite kind `{other}`; known: {}",
                        Kinds::NAMES.join(", ")
                    ))
                }
            }
        }
        if kinds.enabled().is_empty() {
            return Err("--rewrite-source= needs at least one kind".into());
        }
        Ok(kinds)
    }

    fn enabled(&self) -> Vec<&'static str> {
        let flags = [
            self.comments,
            self.newlines,
            self.variables,
            self.fmt,
            self.idioms,
            self.everything,
        ];
        Kinds::NAMES
            .iter()
            .zip(flags)
            .filter(|(_, on)| *on)
            .map(|(name, _)| *name)
            .collect()
    }

    /// Kinds that may change what the code does; nothing checks them.
    pub fn logic(&self) -> bool {
        self.idioms || self.everything
    }

    /// The run folder name: each set of kinds keeps its own progress.
    pub fn dir_name(&self) -> String {
        self.enabled().join("+")
    }

    pub fn describe(&self) -> String {
        let list = self.enabled().join(", ");
        if self.logic() {
            format!("{list} (NOT CHECKED: these edits may change logic)")
        } else {
            format!("{list} (checked; everything else becomes a finding)")
        }
    }
}

/// The files one run would queue, and why the others are left out.
pub struct Scope {
    /// Queued: repo-relative path and size in bytes.
    pub files: Vec<(PathBuf, u64)>,
    /// Marked as generated in their first lines (`@generated`, "do not edit").
    pub generated: Vec<PathBuf>,
    /// Under a skipped folder (fixtures, golden files, vendored code).
    pub fixtures: usize,
    /// Larger than `--max-bytes`.
    pub too_big: Vec<(PathBuf, u64)>,
}

/// The tracked `.rs` files in scope, read from `git ls-files` so build output
/// and scratch files never count. `filter` keeps paths containing it.
pub fn discover(project_root: &Path, filter: Option<&str>, max_bytes: u64) -> Result<Scope, String> {
    let out = Command::new("git")
        .arg("ls-files")
        .arg("-z")
        .arg("--")
        .args(SCOPE_ROOTS)
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git ls-files: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git ls-files failed (the comment pass needs a git checkout): {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let mut scope = Scope {
        files: Vec::new(),
        generated: Vec::new(),
        fixtures: 0,
        too_big: Vec::new(),
    };
    let listing = String::from_utf8_lossy(&out.stdout);
    let mut paths: Vec<&str> = listing.split('\0').filter(|p| p.ends_with(".rs")).collect();
    paths.sort_unstable();
    for rel in paths {
        if filter.is_some_and(|f| !rel.contains(f)) {
            continue;
        }
        let rel_path = PathBuf::from(rel);
        if rel.split('/').rev().skip(1).any(|d| SKIP_DIRS.contains(&d)) {
            scope.fixtures += 1;
            continue;
        }
        let Ok(bytes) = fs::read(project_root.join(rel)) else {
            continue; // deleted in the working tree
        };
        if is_generated(&String::from_utf8_lossy(&bytes[..bytes.len().min(4096)])) {
            scope.generated.push(rel_path);
        } else if bytes.len() as u64 > max_bytes {
            scope.too_big.push((rel_path, bytes.len() as u64));
        } else {
            scope.files.push((rel_path, bytes.len() as u64));
        }
    }
    scope.too_big.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(scope)
}

/// A generated file says so in a comment near its top.
fn is_generated(head: &str) -> bool {
    head.lines().take(20).any(|l| {
        let t = l.trim_start();
        let low = t.to_ascii_lowercase();
        t.starts_with("//") && (low.contains("@generated") || low.contains("do not edit"))
    })
}

/// The prompt for one file. `report` is where the findings go; `context` is
/// the file's section from `comment_context` (empty if none was built);
/// `kinds` says which edits are allowed; `edition` is the crate's edition.
pub fn build_prompt(
    project_root: &Path,
    rel: &Path,
    report: &Path,
    context: &str,
    kinds: &Kinds,
    edition: &str,
) -> String {
    let rel = rel.display();
    let abs = project_root.join(rel.to_string()).display().to_string();
    let report = report.display();
    let root = project_root.display();
    let findings = findings_section(&rel.to_string(), context, edition);
    let edits = edits_section(&abs, kinds);
    format!(
        r#"# Rewrite pass: {rel}

You work on ONE Rust source file of the Azul GUI toolkit: you note what a
stronger reviewer should look at, and you make only the edits listed in step 3.
Other agents work on other files at the same time, so touch nothing else.

- The file, the only file you may edit: `{abs}`
- Your findings file, the only file you may write: `{report}`
- The repository, for Grep and Glob: `{root}`

## Step 1: read the whole file

Read `{abs}` with the Read tool from the first line to the last (use `offset`
and `limit` for long files). Your findings use the line numbers you see now.

{findings}

{edits}

## Rules

- Use only the Read, Grep, Glob, Edit and Write tools. Do not use the network.
- Write no personal data (names, e-mail addresses, accounts) into comments or findings.
- Create, edit or delete no file other than the two above.

When you are done, reply with one line: `DONE <number of findings> findings`.

{context}"#
    )
}

/// Step 2: the findings file, its categories and the checklists behind them.
fn findings_section(rel: &str, context: &str, edition: &str) -> String {
    let context_hint = if context.is_empty() {
        ""
    } else {
        "\n\nThe context section at the end already describes the types and functions this\n\
         file uses, names the items nothing else mentions and, when coverage data was given,\n\
         what the tests never run. Use it instead of searching; Grep only to confirm a\n\
         `dead-code` or `duplication` finding."
    };
    let lean_crate = ["core/", "css/", "layout/"].iter().any(|c| rel.starts_with(c));
    let crate_note = if lean_crate {
        "\n\nThis file is in core, css or layout. Those crates should build without `std` and\n\
         allocate as little as possible, so look closely for `alloc`, `inline-css` and\n\
         `const-static` findings."
    } else {
        ""
    };
    let chains = if edition >= "2024" {
        "- nested `if let` plus `if` -> an if-let chain (`if let Some(x) = a && x > 0`)."
    } else {
        "Not on this crate's edition ({edition}): if-let chains, which arrive with the move to 2024."
    };
    let chains = chains.replace("{edition}", edition);
    format!(
        r#"## Step 2: write the findings file, before you edit anything

Write the findings file with one line per finding, exactly in this form:

    - {rel}:LINE [CATEGORY] One sentence on why it deserves a closer look.

LINE is the line number from step 1. CATEGORY is one of these.

Crash safety:
- `overflow`: `+`, `-`, `*`, `<<` or a `usize` subtraction on sizes, indices or
  parsed input that can overflow, or a narrowing `as` cast. Debug builds panic,
  release builds wrap and corrupt offsets. Name the safe form (`checked_*`,
  `saturating_*`, `try_from`).
- `panic`: `unwrap` (prefer `expect` with the reason, or no panic at all),
  `[i]` indexing, `&s[a..b]` on a `str` (may split a character), division by a
  value that can be zero, or `unreachable!` on a path that input, FFI or events
  can reach. A panic across `extern "C"` aborts. Reading a poisoned `Mutex` too.
- `unsafe`: an `unsafe` block or impl with no `SAFETY:` comment saying why it is
  sound, or `unsafe` where a safe form exists.

Design:
- `global-state`: `thread_local!`, a `static` `Mutex`/`RwLock`, `static mut`,
  `OnceLock<Mutex<..>>`, a one-time lock, a global cache, or code that runs
  before `main`: state that should be passed in or owned by a struct.
- `architecture`: a responsibility in the wrong place, `Rc`/`Arc` where plain
  ownership would do, a design that will not hold up.
- `duplication`: logic that also exists elsewhere; name the other place (`path:line` or function).
- `dead-code`: something nothing uses (confirm before you flag it).
- `superseded`: code a newer path replaced that is still around.

Cost:
- `alloc`: `format!`, `to_string`, a `String` or `Vec` built per call on a hot or
  per-frame path, `.clone()` used to get around the borrow checker or of a
  large value, `collect` only to iterate, or `std::` where `core::`/`alloc::`
  would keep the crate `no_std`.
- `inline-css`: CSS written as a string and parsed at run time where a static
  CSS block would do.
- `const-static`: a large `const` table or string, copied into every use, that
  should be a `static`.

Tests:
- `empty-test`: a test that cannot fail: no assertion, `assert!(true)`, only
  `is_ok()` on setup, an expected value computed by the code under test, or a
  name that promises a branch the body never reaches.
- `untested`: a function or branch the coverage data says no test runs, or an
  error path or edge case (empty input, zero, the maximum) no test covers.

Readability (flag only: these changes may touch logic, so a later pass makes
them with a test first):
- `idiom`: an older pattern that a newer std API replaces (list below), or a
  check followed by `unwrap` where a pattern match reads better.
- `paths`: a long qualified path written out in code (`a::b::c::Foo::new(..)`)
  where a `use` at the top, grouped with braces (`use a::b::{{c, d}};`), would
  let the code say `Foo::new(..)`.
- `naming`: a too-short, abbreviated or vague name (letters dropped from the
  middle, project-only abbreviations); a getter called `get_x` instead of `x`,
  `x_mut`, `set_x`; a constructor returning `Result` not called `try_new`.
- `style`: a function over ~80 lines, nesting deeper than three levels,
  boolean parameters (use an enum), tuples whose `.0`/`.1` would read better as
  named fields, `&Vec<T>`/`&String`/`&Box<T>`/`&Option<T>` parameters (take
  `&[T]`, `&str`, `&T`, `Option<&T>`), a long iterator chain or `for_each`
  where a `for` loop is clearer, a missing type on `.into()`/`.parse()`/
  `collect()`, a `len()` without `is_empty()`, a `new()` without `Default`, a
  result that must not be ignored without `#[must_use]`, needless `pub`, a
  magic number, an `assert!` without a message, or `..Default::default()`
  hiding fields that need real values.

Other:
- `bug`: a likely bug (wrong branch, wrong unit, ignored error, off-by-one).
- `todo`: a leftover TODO, FIXME, XXX or HACK.
- `other`: anything else worth a stronger model's time.

Flag, never fix (step 3 says which edits are yours): do not change code because
of a finding. List only what is worth a stronger reviewer's time. If there is
nothing, the file holds the single line `- none`. Write no headings or summary
into it.{context_hint}{crate_note}

Newer std APIs for `idiom` (stable on the repository's Rust 1.91):
- `opt.map_or(false, f)` -> `is_some_and(f)`; `map_or(true, f)` -> `is_none_or(f)`;
  `matches!(r, Ok(x) if ..)` -> `r.is_ok_and(..)`.
- `if c {{ Some(v) }} else {{ None }}` -> `c.then_some(v)`; a `match` that only
  unwraps or returns -> `let Some(v) = o else {{ return }};`.
- `format!("{{}}", name)` -> `format!("{{name}}")`.
- `lazy_static!` / `once_cell` -> `LazyLock`, `OnceLock`, `LazyCell`, `OnceCell`.
- `(a + b - 1) / b` -> `a.div_ceil(b)`; `x % n == 0` -> `x.is_multiple_of(n)`;
  `(a - b).abs()` -> `a.abs_diff(b)`; `(a + b) / 2` -> `a.midpoint(b)`;
  sign-changing `as` -> `cast_unsigned()` / `cast_signed()`; overflow that must
  never happen -> `strict_add` and friends.
- `repeat(x).take(n)` -> `iter::repeat_n(x, n)`; a hand-written group-by over a
  slice -> `chunk_by`; `windows(2).all(..)` -> `is_sorted()`.
- `s[..4].try_into().unwrap()` -> `first_chunk::<4>()`; checked `split_at` ->
  `split_at_checked`; retain plus collecting the removed items -> `extract_if`;
  conditional `pop` -> `pop_if`; two `&mut` into one slice -> `get_disjoint_mut`.
- `map_err(|e| {{ log(&e); e }})` -> `inspect_err`; float `partial_cmp(..).unwrap()`
  -> `total_cmp`; `fn_a == fn_b` on function pointers -> `ptr::fn_addr_eq`.
- byte index that may split a char -> `floor_char_boundary`; `CStr` built from
  `b"..\0"` -> `c".."`; `memoffset` -> `core::mem::offset_of!`;
  `std::mem::size_of` -> `size_of`; `addr_of!` -> `&raw const`.
- `#[allow(lint)]` that is meant to fire -> `#[expect(lint)]`; manual enum
  `Default` -> `#[derive(Default)]` with `#[default]`; `std::error::Error` in
  `no_std` code -> `core::error::Error`; `mem::replace(&mut x, Default::default())`
  -> `mem::take`; `Duration::from_secs(60 * m)` -> `from_mins(m)`; `Cell` get then
  set -> `Cell::update`.
{chains}
Not available here: `assert_matches!`, `array_windows`, the new range types.
Do not propose new crates."#
    )
}

/// Step 3: the edits this run allows, and the checks they go through.
fn edits_section(abs: &str, kinds: &Kinds) -> String {
    let mut steps: Vec<String> = Vec::new();
    if kinds.comments {
        steps.push(
            "Comments (`//`, `///`, `//!`, `/* */`, `/** */`, `/*! */`):\n\
             - Write plain, short English a new colleague understands: what the code is for\n  \
             and why it is needed, in flowing sentences.\n\
             - Delete comments that only repeat the code or the name next to them.\n\
             - Delete history and changelog narration: \"used to\", \"now\", \"previously\",\n  \
             \"fixed\", \"was broken before\", dates, ticket or finding ids, agent or wave\n  \
             names, plan section numbers (\"plan §4.3.5\"), references to planning notes or\n  \
             `.md` files. If such a comment carries a real fact, keep the fact, said in the\n  \
             present tense.\n\
             - Shorten long comments to the sentences that carry the why. No banner blocks.\n\
             - Keep (shortened if long) comments that explain a non-obvious why, an\n  \
             invariant, a safety argument (`SAFETY:`), a failure mode, or how the piece\n  \
             fits the whole. RFC, spec and man-page references stay.\n\
             - Struct and enum fields keep a short doc comment saying what they hold.\n\
             - A short `//!` at the top may tell the story of the module.\n\
             - Test comments say which behaviour is checked, not the story of the bug.\n\
             - A good comment stays as it is. A few good edits beat many small ones.\n\
             - In `///` and `//!` text (clippy checks it with warnings as errors) put code\n  \
             names in backticks (`LayoutWindow`, `layout()`), keep the first paragraph to\n  \
             one short sentence, keep `# Safety`, `# Errors`, `# Panics` and `# Examples`\n  \
             sections, and indent the continuation lines of list items."
                .to_string(),
        );
    }
    if kinds.newlines {
        steps.push(
            "Blank lines: put one blank line between the logical steps of a long function\n\
             or block where code runs together, and keep the blank lines that already\n\
             separate steps. Never more than one in a row."
                .to_string(),
        );
    }
    if kinds.variables {
        steps.push(
            "Local variable names: rename a local variable (a `let` binding, a function or\n\
             closure parameter, a pattern binding) whose name is too short to read (`tr1`,\n\
             `nd`, `ix`, `cb`) to a descriptive one (`table_row`, `node`, `index`,\n\
             `callback`). Keep the usual short names: `i` and `j` for indices, `x`, `y`, `w`,\n\
             `h` for geometry, `f` for a formatter, `e` for an error in a closure. Rename every\n\
             use in that function the same way, including `{name}` inside format strings.\n\
             Do not rename fields, methods, functions or types, nor a local written as a\n\
             shorthand field (`Point { x }`), and pick a name that does not already appear\n\
             in the function."
                .to_string(),
        );
    }
    if kinds.idioms || kinds.everything {
        let which = if kinds.everything {
            "every finding you listed (bugs, crash risks, naming, paths, idioms, style)"
        } else {
            "every `idiom` finding you listed"
        };
        steps.push(format!(
            "Apply {which}. For each change, first add a unit test (in this file's\n\
             `#[cfg(test)] mod tests`, created if missing) that pins today's behaviour of the\n\
             code you change, so the change is proven not to alter it; for a bug, add a test\n\
             that shows the fixed behaviour. Then make the change. Keep each change small."
        ));
    }

    let mut out = String::from("## Step 3: edit the file\n\n");
    if steps.is_empty() {
        out.push_str("Make no edits to the file: the findings are the whole job this time.");
        return out;
    }
    out.push_str(&format!("Only these kinds of edits are allowed in `{abs}`.\n"));
    for (n, step) in steps.iter().enumerate() {
        out.push_str(&format!("\n{}. {step}\n", n + 1));
    }
    if !kinds.newlines && !kinds.logic() {
        out.push_str("\nDo not add or remove blank lines between code.\n");
    }
    if kinds.logic() {
        out.push_str(
            "\nNothing checks these edits mechanically: a person reads the diff and runs the\n\
             tests. Keep every change small and explain it in its findings line.\n",
        );
    } else {
        let tokens = if kinds.variables {
            "Apart from renamed locals, every token outside comments stays exactly as it\n  \
             is, including strings, attributes such as `#[doc = \"...\"]`, and the spacing\n  \
             between symbols (`->` versus `- >`). A rename covers every use of the local in\n  \
             its function and nothing else."
        } else {
            "Every token outside comments stays exactly as it is, including names,\n  \
             strings, attributes such as `#[doc = \"...\"]`, and the spacing between symbols\n  \
             (`->` versus `- >`)."
        };
        out.push_str(&format!(
            "\nThese are checked by a program when you finish. If one fails, ALL your edits to\n\
             this file are thrown away:\n\n\
             - {tokens}\n\
             - No doc comment (`///`, `//!`) where there was none, and no `//` comment\n  \
             turned into a doc comment.\n\
             - Code blocks inside doc comments (between ``` fences) are tests: leave them\n  \
             and their fence lines exactly as they are.\n\
             - Comment lines containing `+spec` stay exactly as they are.\n\
             - Every TODO, FIXME, XXX and HACK stays; you may reword the text after it.\n\
             - The file still parses as Rust.\n"
        ));
    }
    if kinds.fmt {
        out.push_str(
            "\nThe tool runs rustfmt on the file before you start and after you finish, so do\n\
             not reformat code yourself.\n",
        );
    } else {
        out.push_str("\nDo not reformat code.\n");
    }
    out
}

/// The `claude` arguments for one comment agent: file tools only, so it can
/// neither build, run a shell, reach the network nor start sub-agents, and no
/// user configuration (CLAUDE.md, memory, hooks, plugins, MCP servers), so the
/// prompt is all it sees.
pub fn agent_cli_args(model: &str, effort: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--dangerously-skip-permissions",
        "--verbose",
        "--output-format",
        "stream-json",
        "--tools",
        "Read,Grep,Glob,Edit,Write",
        "--strict-mcp-config",
        "--no-session-persistence",
        "--safe-mode",
        "--model",
        model,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if let Some(effort) = effort {
        args.push("--effort".into());
        args.push(effort.into());
    }
    args
}

/// rustfmt with the repository's rustfmt.toml, run on one file at a time
/// through stdin so it never follows `mod` declarations into other files.
/// rustfmt.toml sets unstable options that only nightly rustfmt applies; the
/// pinned stable rustfmt skips them with a warning, so nightly is used when
/// it is installed.
#[derive(Clone)]
pub struct Formatter {
    /// `rustfmt +nightly` works.
    nightly: bool,
    /// The repository's rustfmt.toml, if there is one.
    config: Option<PathBuf>,
}

impl Formatter {
    /// The formatter to use, or None when rustfmt is not installed.
    pub fn detect(project_root: &Path) -> Option<Formatter> {
        let works = |args: &[&str]| {
            Command::new("rustfmt")
                .args(args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        };
        // Ask rustup which toolchains exist before naming one: `rustfmt +nightly`
        // on a machine without nightly makes rustup download all of it.
        let nightly_installed = Command::new("rustup")
            .args(["toolchain", "list"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.starts_with("nightly")));
        let nightly = nightly_installed && works(&["+nightly", "--version"]);
        if !nightly && !works(&["--version"]) {
            return None;
        }
        let config = Some(project_root.join("rustfmt.toml")).filter(|p| p.is_file());
        Some(Formatter { nightly, config })
    }

    pub fn describe(&self) -> &'static str {
        if self.nightly {
            "nightly rustfmt, with every option in rustfmt.toml"
        } else {
            "stable rustfmt: the unstable options in rustfmt.toml (imports_granularity, \
             group_imports, wrap_comments, format_strings, ...) are skipped; \
             `rustup toolchain install nightly --profile minimal --component rustfmt` applies them"
        }
    }

    /// `src` formatted, or None when rustfmt refuses it.
    pub fn format(&self, src: &str, edition: &str) -> Option<String> {
        let mut cmd = Command::new("rustfmt");
        if self.nightly {
            cmd.arg("+nightly");
        }
        cmd.args(["--edition", edition]);
        if let Some(config) = &self.config {
            cmd.arg("--config-path").arg(config);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        // rustfmt reads all of stdin before it writes, so writing first cannot block.
        let wrote = child
            .stdin
            .take()
            .is_some_and(|mut stdin| stdin.write_all(src.as_bytes()).is_ok());
        let out = child.wait_with_output().ok()?;
        if !wrote || !out.status.success() {
            return None;
        }
        String::from_utf8(out.stdout).ok()
    }
}

/// The edition of the crate a file belongs to, from the nearest Cargo.toml
/// with a `[package]` (rustfmt parses by edition). An inherited edition comes
/// from the root's `[workspace.package]`.
pub fn edition_for(project_root: &Path, rel: &str) -> String {
    let read = |path: &Path| {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str::<toml::Table>(&text).ok())
    };
    let workspace_edition = || {
        read(&project_root.join("Cargo.toml"))
            .and_then(|t| t.get("workspace")?.get("package")?.get("edition")?.as_str().map(String::from))
            .unwrap_or_else(|| "2021".to_string())
    };
    let mut dir = project_root.join(rel);
    while dir.pop() && dir.starts_with(project_root) {
        let Some(manifest) = read(&dir.join("Cargo.toml")) else {
            continue;
        };
        let Some(package) = manifest.get("package") else {
            continue;
        };
        return match package.get("edition") {
            Some(toml::Value::String(edition)) => edition.clone(),
            Some(_) => workspace_edition(),
            // Cargo's default when a package names no edition.
            None => "2015".to_string(),
        };
    }
    "2021".to_string()
}

/// One queued file and the files kept next to its prompt.
pub struct Item {
    /// Repo-relative path of the source file, as written in findings.
    pub rel: String,
    /// The source file itself.
    pub source: PathBuf,
    /// The findings the agent writes.
    pub report: PathBuf,
    /// The crate's edition, for rustfmt.
    edition: String,
    /// The file as the agent found it (already formatted); the edit is
    /// checked against it and it is what a failed run restores.
    snapshot: PathBuf,
    /// A rejected edit, kept for a look.
    rejected: PathBuf,
}

/// The sidecar that names a prompt's source file. Written with the prompt,
/// because a flat prompt name cannot be turned back into a path reliably.
pub fn source_sidecar(prompt: &Path) -> PathBuf {
    prompt.with_extension("md.source")
}

impl Item {
    pub fn open(prompt: &Path, project_root: &Path, reports: &Path) -> Result<Item, String> {
        let rel = fs::read_to_string(source_sidecar(prompt))
            .map_err(|e| format!("cannot read {}: {e}", source_sidecar(prompt).display()))?
            .trim()
            .to_string();
        let stem = prompt.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
        Ok(Item {
            source: project_root.join(&rel),
            report: reports.join(format!("{stem}.md")),
            edition: edition_for(project_root, &rel),
            snapshot: prompt.with_extension("md.orig.rs"),
            rejected: prompt.with_extension("md.rejected.rs"),
            rel,
        })
    }
}

/// Before the agent starts: format the file, take the snapshot, and make sure
/// the guard can read it. Returns whether rustfmt changed the file. An error
/// means the agent must not run on it.
pub fn prepare(item: &Item, fmt: Option<&Formatter>) -> Result<bool, String> {
    let now = fs::read_to_string(&item.source)
        .map_err(|e| format!("cannot read {} as UTF-8: {e}", item.rel))?;
    if let Ok(left) = fs::read_to_string(&item.snapshot) {
        // A run was interrupted with this file still checked out to an agent.
        if left != now {
            if style_guard::check_style_only(&left, &now, true).is_err() {
                return Err(format!(
                    "an interrupted run left a snapshot of {} and the file's code has changed since; \
                     check `git diff {}`, then delete {} to queue the file again",
                    item.rel,
                    item.rel,
                    item.snapshot.display()
                ));
            }
            // Only unverified style edits are left over: start again from the snapshot.
            fs::write(&item.source, &left).map_err(|e| format!("cannot restore {}: {e}", item.rel))?;
        }
    }
    let original =
        fs::read_to_string(&item.source).map_err(|e| format!("cannot read {}: {e}", item.rel))?;
    // rustfmt is trusted to keep the meaning; its output is used only if it
    // still parses and the guard can read it.
    let formatted = fmt
        .and_then(|f| f.format(&original, &item.edition))
        .filter(|f| *f != original)
        .filter(|f| style_guard::still_parses(&original, f).is_ok() && style_guard::self_test(f).is_ok());
    let src = formatted.clone().unwrap_or(original);
    style_guard::self_test(&src)
        .map_err(|e| format!("left alone, the style guard cannot check this file: {e}"))?;
    if formatted.is_some() {
        fs::write(&item.source, &src).map_err(|e| format!("cannot write {}: {e}", item.rel))?;
    }
    fs::write(&item.snapshot, &src).map_err(|e| format!("cannot write the snapshot: {e}"))?;
    // A report from an earlier attempt must not pass for this run's findings.
    let _ = fs::remove_file(&item.report);
    Ok(formatted.is_some())
}

/// After a clean exit: keep the edit if it stays within `kinds` (checked by
/// `style_guard` unless the kinds may change logic, which only must still
/// parse), format the result once more if `fmt` (new names and blank lines
/// may need a new layout), and move the findings to the final line numbers.
/// An error turns the run into a failure.
pub fn accept(
    item: &Item,
    kinds: &Kinds,
    fmt: Option<&Formatter>,
    formatted_before: bool,
) -> Result<String, String> {
    let before = fs::read_to_string(&item.snapshot).map_err(|e| format!("the snapshot is gone: {e}"))?;
    let after = fs::read_to_string(&item.source).map_err(|e| format!("cannot read the edited file: {e}"))?;
    if !item.report.exists() {
        return Err("the agent wrote no findings file".into());
    }
    let edit_map = if before == after {
        LineMap::identity()
    } else if kinds.logic() {
        style_guard::still_parses(&before, &after).map_err(|e| format!("rejected: {e}"))?;
        LineMap::identity()
    } else {
        style_guard::check_style_only(&before, &after, kinds.variables)
            .map_err(|e| format!("rejected: {e}"))?
    };

    let (layout_map, formatted_after) = match fmt.and_then(|f| f.format(&after, &item.edition)) {
        Some(formatted) if formatted != after => {
            let checked = if kinds.logic() {
                style_guard::still_parses(&after, &formatted).map(|_| LineMap::identity())
            } else {
                style_guard::check_layout_only(&after, &formatted)
            };
            match checked {
                Ok(map) => {
                    fs::write(&item.source, &formatted)
                        .map_err(|e| format!("cannot write {}: {e}", item.rel))?;
                    (map, "yes")
                }
                // Keep the checked edit as it is; a later rustfmt run can lay it out.
                Err(_) => (LineMap::identity(), "skipped (rustfmt changed more than the layout)"),
            }
        }
        _ => (LineMap::identity(), "no"),
    };

    let findings = remap_report(&item.report, &item.rel, &|line| layout_map.map(edit_map.map(line)))?;
    let _ = fs::remove_file(&item.snapshot);
    Ok(format!(
        "formatted_before={formatted_before}\nedited={}\nchecked={}\nformatted_after={formatted_after}\nfindings={findings}",
        before != after,
        !kinds.logic()
    ))
}

/// After a failed run: put the file back as the agent found it. Returns
/// whether anything had to be undone; the undone version is kept next to the
/// prompt.
pub fn restore(item: &Item) -> Result<bool, String> {
    let Ok(before) = fs::read(&item.snapshot) else {
        return Ok(false);
    };
    let now = fs::read(&item.source).unwrap_or_default();
    let changed = now != before;
    if changed {
        let _ = fs::write(&item.rejected, &now);
        fs::write(&item.source, &before).map_err(|e| format!("cannot restore {}: {e}", item.rel))?;
    }
    let _ = fs::remove_file(&item.snapshot);
    Ok(changed)
}

/// One line of a findings file.
pub struct Finding {
    pub path: String,
    pub line: usize,
    pub category: String,
    pub reason: String,
}

/// Parses `- path:LINE [category] reason`. A range (`12-15`) keeps its first line.
pub fn parse_finding(line: &str) -> Option<Finding> {
    let rest = line.trim().strip_prefix("- ")?;
    let (loc, rest) = rest.split_once(' ')?;
    let (path, line_no) = loc.trim_matches('`').rsplit_once(':')?;
    let line_no: usize = line_no.split(['-', ',']).next()?.parse().ok()?;
    let (category, reason) = rest.trim_start().strip_prefix('[')?.split_once(']')?;
    Some(Finding {
        path: path.to_string(),
        line: line_no,
        category: category.trim().to_ascii_lowercase(),
        reason: reason.trim().to_string(),
    })
}

fn format_finding(f: &Finding) -> String {
    format!("- {}:{} [{}] {}", f.path, f.line, f.category, f.reason)
}

/// Moves the findings about `rel` to the edited file's line numbers; returns
/// how many findings the report holds.
fn remap_report(report: &Path, rel: &str, map: &dyn Fn(usize) -> usize) -> Result<usize, String> {
    let text = fs::read_to_string(report).map_err(|e| format!("cannot read the findings: {e}"))?;
    let mut count = 0usize;
    let lines: Vec<String> = text
        .lines()
        .map(|l| match parse_finding(l) {
            Some(mut f) => {
                count += 1;
                if f.path == rel {
                    f.line = map(f.line);
                }
                format_finding(&f)
            }
            None => l.to_string(),
        })
        .collect();
    fs::write(report, lines.join("\n") + "\n").map_err(|e| format!("cannot write the findings: {e}"))?;
    Ok(count)
}

/// What `merge` wrote.
pub struct MergeSummary {
    pub reports: usize,
    pub findings: usize,
    pub files: usize,
    pub unparsed: usize,
}

/// Turns every findings file in `reports` into one todo list at `out`,
/// grouped by category and then by file, with counts. Lines that do not
/// parse are listed at the end, never dropped.
pub fn merge(reports: &Path, out: &Path) -> Result<MergeSummary, String> {
    let mut names: Vec<PathBuf> = fs::read_dir(reports)
        .map_err(|e| format!("cannot read {}: {e}", reports.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    names.sort();

    // category -> file -> findings
    let mut grouped: BTreeMap<String, BTreeMap<String, Vec<Finding>>> = BTreeMap::new();
    let mut unparsed: Vec<String> = Vec::new();
    let mut clean = 0usize;
    for path in &names {
        let text = fs::read_to_string(path).unwrap_or_default();
        let report_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
        let mut any = false;
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            if line == "- none" {
                continue;
            }
            match parse_finding(line) {
                Some(f) => {
                    any = true;
                    grouped
                        .entry(f.category.clone())
                        .or_default()
                        .entry(f.path.clone())
                        .or_default()
                        .push(f);
                }
                None => unparsed.push(format!("- {report_name}: `{line}`")),
            }
        }
        if !any {
            clean += 1;
        }
    }

    let mut order: Vec<String> = CATEGORIES.iter().map(|c| c.to_string()).collect();
    order.extend(grouped.keys().filter(|k| !CATEGORIES.contains(&k.as_str())).cloned());

    let findings: usize = grouped.values().flat_map(|f| f.values()).map(Vec::len).sum();
    let files = grouped.values().flat_map(|f| f.keys()).collect::<BTreeSet<_>>().len();
    let mut md = format!(
        "# Comment pass findings\n\n{findings} findings in {files} files, from {} reports \
         ({clean} with none). Each item names a file and line, then why it deserves a closer \
         look. Line numbers are those of the files after the comment pass.\n\n\
         | Category | Findings | Files |\n|---|---:|---:|\n",
        names.len(),
    );
    for cat in &order {
        if let Some(by_file) = grouped.get(cat) {
            let n: usize = by_file.values().map(Vec::len).sum();
            md.push_str(&format!("| {cat} | {n} | {} |\n", by_file.len()));
        }
    }
    for cat in &order {
        let Some(by_file) = grouped.get_mut(cat) else {
            continue;
        };
        let n: usize = by_file.values().map(Vec::len).sum();
        md.push_str(&format!("\n## {cat} ({n})\n"));
        for (file, list) in by_file.iter_mut() {
            list.sort_by_key(|f| f.line);
            md.push_str(&format!("\n### {file} ({})\n\n", list.len()));
            for f in list.iter() {
                md.push_str(&format!("- [ ] {}:{} {}\n", f.path, f.line, f.reason));
            }
        }
    }
    if !unparsed.is_empty() {
        md.push_str(&format!("\n## Lines that did not parse ({})\n\n", unparsed.len()));
        for l in &unparsed {
            md.push_str(l);
            md.push('\n');
        }
    }
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    fs::write(out, md).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    Ok(MergeSummary {
        reports: names.len(),
        findings,
        files,
        unparsed: unparsed.len(),
    })
}

/// Prints what the run covers and leaves out.
pub fn print_scope(scope: &Scope, max_bytes: u64) {
    let bytes: u64 = scope.files.iter().map(|(_, b)| b).sum();
    println!(
        "Comment pass scope: tracked .rs files under {}",
        SCOPE_ROOTS.join(", ")
    );
    println!(
        "  in scope:  {} files, {:.1} MB (~{:.1}M tokens)",
        scope.files.len(),
        bytes as f64 / 1e6,
        bytes as f64 / BYTES_PER_TOKEN / 1e6
    );
    println!(
        "  left out:  {} generated, {} under fixture/golden/vendor folders, {} over --max-bytes={}",
        scope.generated.len(),
        scope.fixtures,
        scope.too_big.len(),
        max_bytes
    );
    for path in &scope.generated {
        println!("    generated: {}", path.display());
    }
    for (path, size) in scope.too_big.iter().take(10) {
        println!("    too big:   {} ({} KB)", path.display(), size / 1024);
    }
    if scope.too_big.len() > 10 {
        println!("    too big:   ... and {} more", scope.too_big.len() - 10);
    }
}

// A rough token model for one agent on one file, used only for the dry-run
// estimate: the agent reads the file once, every turn re-reads its context
// from the prompt cache, and it writes edits, findings and some thinking.
const BYTES_PER_TOKEN: f64 = 3.5;
/// Claude Code's system prompt and tool definitions; the file's own prompt
/// (instructions plus repository context) comes on top.
const SYSTEM_TOKENS: f64 = 15_000.0;
const BASE_TURNS: f64 = 8.0;
/// One more turn (a read or an edit) per this many tokens of source.
const SOURCE_TOKENS_PER_TURN: f64 = 1_500.0;
const BASE_OUTPUT_TOKENS: f64 = 2_000.0;
const OUTPUT_PER_SOURCE_TOKEN: f64 = 0.25;

/// List prices in USD per million tokens: input, output, cache read. A cache
/// write costs 1.25 times the input price.
fn prices(model: &str) -> Option<(f64, f64, f64)> {
    let m = model.to_ascii_lowercase();
    if m.contains("sonnet-4") {
        Some((3.0, 15.0, 0.30))
    } else if m.contains("sonnet") {
        Some((2.0, 10.0, 0.20))
    } else if m.contains("opus") {
        Some((4.0, 20.0, 0.20))
    } else if m.contains("haiku") {
        Some((1.0, 5.0, 0.10))
    } else {
        None
    }
}

/// The dry-run cost estimate for the queued files, given as (source bytes,
/// prompt bytes) per file.
pub fn print_estimate(files: &[(u64, u64)], model: &str) {
    let (mut cache_read, mut cache_write, mut output) = (0.0f64, 0.0f64, 0.0f64);
    for &(source, prompt) in files {
        let t = source as f64 / BYTES_PER_TOKEN;
        let p = prompt as f64 / BYTES_PER_TOKEN;
        let turns = BASE_TURNS + t / SOURCE_TOKENS_PER_TURN;
        cache_read += turns * (SYSTEM_TOKENS + p + t / 2.0);
        cache_write += t + p;
        output += BASE_OUTPUT_TOKENS + t * OUTPUT_PER_SOURCE_TOKEN;
    }
    let total_bytes: u64 = files.iter().map(|f| f.0).sum();
    let prompt_bytes: u64 = files.iter().map(|f| f.1).sum();
    println!(
        "Estimate for {} files ({:.1} MB, ~{:.1}M source tokens; prompts with context average \
         {:.1} KB) with {model}:",
        files.len(),
        total_bytes as f64 / 1e6,
        total_bytes as f64 / BYTES_PER_TOKEN / 1e6,
        prompt_bytes as f64 / files.len().max(1) as f64 / 1024.0
    );
    println!(
        "  ~{:.0}M cache-read, ~{:.1}M fresh input, ~{:.1}M output tokens",
        cache_read / 1e6,
        cache_write / 1e6,
        output / 1e6
    );
    match prices(model) {
        Some((input, out, read)) => {
            let usd = (cache_read * read + cache_write * input * 1.25 + output * out) / 1e6;
            println!(
                "  ~${:.0} at API list prices (likely ${:.0}-${:.0}; on a subscription this is usage, \
                 not dollars)",
                usd,
                usd * 0.5,
                usd * 2.0
            );
        }
        None => println!("  (no price known for this model)"),
    }
}

/// Tracked files with uncommitted changes, to spot an agent that wrote
/// outside its own file.
pub fn changed_files(project_root: &Path) -> BTreeSet<String> {
    Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .current_dir(project_root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The dry-run test of the guard on every queued file; see
/// `style_guard::self_test`. Returns the files it fails on.
pub fn self_test_all(project_root: &Path, files: &[PathBuf]) -> Vec<(PathBuf, String)> {
    let mut failures = Vec::new();
    for rel in files {
        let result = fs::read_to_string(project_root.join(rel))
            .map_err(|e| format!("not UTF-8: {e}"))
            .and_then(|src| style_guard::self_test(&src));
        if let Err(e) = result {
            failures.push((rel.clone(), e));
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_kinds_default_to_comments_and_logic_kinds_are_marked() {
        let default = Kinds::default();
        assert_eq!(default.dir_name(), "comments");
        assert!(!default.logic());
        let style = Kinds::parse("comments,newlines,variables").unwrap();
        assert_eq!(style.dir_name(), "comments+newlines+variables");
        assert!(!style.fmt && !style.logic());
        let all = Kinds::parse("everything").unwrap();
        assert!(all.logic() && all.fmt && all.variables);
        assert!(Kinds::parse("imports").is_err());
        assert!(Kinds::parse("").is_err());
    }

    #[test]
    fn the_prompt_lists_only_the_allowed_edits() {
        let root = Path::new("/repo");
        let rel = Path::new("core/src/a.rs");
        let report = Path::new("/repo/report.md");
        let comments_only = build_prompt(root, rel, report, "", &Kinds::default(), "2021");
        assert!(comments_only.contains("Comments (`//`"));
        assert!(!comments_only.contains("Local variable names"));
        assert!(comments_only.contains("Every token outside comments stays exactly as it is"));
        assert!(comments_only.contains("Not on this crate's edition (2021)"));
        let renames = build_prompt(root, rel, report, "", &Kinds::parse("variables").unwrap(), "2024");
        assert!(renames.contains("Local variable names"));
        assert!(!renames.contains("Comments (`//`"));
        assert!(renames.contains("if-let chain"));
    }

    #[test]
    fn findings_parse_with_backticks_and_ranges() {
        let f = parse_finding("- `core/src/dom.rs:12-15` [Bug] Index can run past the end.").unwrap();
        assert_eq!(f.path, "core/src/dom.rs");
        assert_eq!(f.line, 12);
        assert_eq!(f.category, "bug");
        assert_eq!(f.reason, "Index can run past the end.");
        assert!(parse_finding("- none").is_none());
        assert!(parse_finding("## Findings").is_none());
    }

    #[test]
    fn generated_files_are_recognised_by_their_header_comment() {
        assert!(is_generated("// @generated by a tool\nfn a() {}\n"));
        assert!(is_generated("//! Table. GENERATED by x.py; do not edit by hand.\n"));
        assert!(!is_generated("fn a() { builder.line(\"// DO NOT EDIT\"); }\n"));
    }

    #[test]
    fn merge_groups_by_category_then_file_and_keeps_unparsed_lines() {
        let dir = tempfile::tempdir().unwrap();
        let reports = dir.path().join("reports");
        fs::create_dir_all(&reports).unwrap();
        fs::write(
            reports.join("a.md"),
            "- a.rs:3 [bug] Off by one.\n- a.rs:1 [style] Mixed naming.\nnot a finding\n",
        )
        .unwrap();
        fs::write(reports.join("b.md"), "- none\n").unwrap();
        let out = dir.path().join("todo.md");
        let summary = merge(&reports, &out).unwrap();
        assert_eq!((summary.reports, summary.findings, summary.files, summary.unparsed), (2, 2, 1, 1));
        let md = fs::read_to_string(&out).unwrap();
        assert!(md.find("## bug (1)").unwrap() < md.find("## style (1)").unwrap());
        assert!(md.contains("- [ ] a.rs:3 Off by one."));
        assert!(md.contains("`not a finding`"));
    }
}
