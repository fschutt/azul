//! `azul-doc assemble-context` — turn every shipping guide page into a
//! SELF-CONTAINED context bundle a non-agentic writing model can rewrite from.
//!
//! The split this exists to make:
//!
//!   - **Finding out what is true** is agentic work. It means reading the
//!     source at HEAD, following a call, checking that a type still has the
//!     field a paragraph claims, noticing that a function was renamed. Tools,
//!     many turns, a lot of tokens spent on things that never reach the page.
//!   - **Writing the page** is not. Given the current page, a list of its
//!     factual errors, and verbatim source excerpts that settle them, a strong
//!     writing model produces a better page in ONE turn with no tools at all.
//!
//! So this command does the first half and hands over a file. Each bundle
//! carries five sections: the page exactly as it ships, the machine checks
//! azul-doc can answer with no model at all (including the unfinished-work
//! markers in the page's own source), a fact-check the agent fills in, a
//! `## Sources` appendix of real excerpts pinned to the current commit, and
//! the writing instructions. The writing model then needs nothing but the
//! bundle.
//!
//! Three properties are deliberate:
//!
//!   - **The bundle names its commit.** An excerpt is only evidence if you know
//!     which tree it came from. Every bundle records `git rev-parse HEAD`, and
//!     a bundle whose SHA is not HEAD is stale by inspection rather than by
//!     belief.
//!   - **The deterministic checks run whether or not an agent does.** File
//!     existence, per-file staleness since `last_generated_rev`, unknown public
//!     API names, dangling links — azul-doc knows all of these for certain, and
//!     `--no-agent` produces a bundle carrying exactly them. The agent is asked
//!     only for what needs judgement.
//!   - **The bundle cannot hide unfinished work.** Section 2b lists every
//!     `TODO` / `FIXME` / `todo!(` / `unimplemented!(` in the files the page
//!     tracks. A model that never sees them cannot tell a shipped feature from
//!     an aspirational one, and writes confident prose about both; a model that
//!     sees an EMPTY list learns something equally useful — the hedging in the
//!     page is obsolete and should go.
//!
//! Which pages count as shipping is `docgen::guide::get_guide_list()` — the
//! same list the website builds from, so a page that does not ship does not get
//! a bundle and a page that ships cannot be forgotten.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use crate::{
    api::ApiData,
    docgen::guide::{get_guide_list, Guide},
    lint_links,
    reftest::{
        autodoc::{commits_since, parse_frontmatter, Frontmatter},
        autoreview::{AutoreviewConfig, AutoreviewSubcommand},
    },
};

/// How the command was invoked.
pub struct Config {
    pub project_root: PathBuf,
    /// Only pages whose slug contains this substring.
    pub filter: Option<String>,
    /// Concurrent `claude -p` agents.
    pub agents: usize,
    pub model: Option<String>,
    pub timeout: Duration,
    /// Write the bundles and prompts, dispatch nothing.
    pub dry_run: bool,
    /// Skip the agent pass entirely: the bundle carries the page and the
    /// machine checks, and says so.
    pub no_agent: bool,
    /// Re-run pages whose previous agent run failed.
    pub retry_failed: bool,
}

impl Config {
    #[must_use]
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            filter: None,
            agents: 4,
            model: None,
            timeout: Duration::from_secs(900),
            dry_run: false,
            no_agent: false,
            retry_failed: false,
        }
    }
}

/// Where bundles land. Under `doc/target/` because they are build output: they
/// are regenerated from the tree, never hand-edited, and never committed.
fn bundles_dir(project_root: &Path) -> PathBuf {
    project_root.join("doc/target/assemble-context")
}

fn prompts_dir(project_root: &Path) -> PathBuf {
    bundles_dir(project_root).join("prompts")
}

/// The bundle for one page. `slug` is the guide's `file_name`, so a nested page
/// (`internals/dom`) keeps its directory in the bundle name (`internals__dom`)
/// and cannot collide with a top-level page of the same stem.
fn bundle_path(project_root: &Path, slug: &str) -> PathBuf {
    bundles_dir(project_root).join(format!("{}.md", slug.replace('/', "__")))
}

fn prompt_path(project_root: &Path, slug: &str) -> PathBuf {
    prompts_dir(project_root).join(format!("{}.md", slug.replace('/', "__")))
}

// ── The tree this bundle is pinned to ──────────────────────────────────

struct Head {
    sha: String,
    date: String,
    dirty: bool,
}

fn read_head(project_root: &Path) -> Head {
    let git = |args: &[&str]| -> String {
        Command::new("git")
            .args(args)
            .current_dir(project_root)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    Head {
        sha: git(&["rev-parse", "HEAD"]),
        date: git(&["log", "-1", "--format=%cI"]),
        // A dirty tree is not a failure — it is the normal state while writing
        // docs — but an excerpt taken from it is not reproducible from the SHA
        // alone, so the bundle says so out loud.
        dirty: !git(&["status", "--porcelain"]).is_empty(),
    }
}

// ── Machine checks: what azul-doc knows for certain ────────────────────

/// One deterministic finding. `severity` is what the writing model should do
/// about it, not how bad it is: `Fix` means the page is wrong today.
struct Check {
    severity: &'static str,
    detail: String,
}

/// Every public API name in api.json: class names plus `Class::member` for
/// every constructor and function. Built once and shared across pages.
fn public_api_names(api: &ApiData) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for version in api.0.values() {
        for module in version.api.values() {
            for (class_name, class) in &module.classes {
                names.insert(class_name.clone());
                // Members are known under both spellings prose uses.
                let mut qualified: Vec<&String> = Vec::new();
                // Enum variants and struct fields are also written BARE ("a
                // `Text` node", "the `Alt` attribute"), so the bare spelling
                // counts as known too - without it the check drowned its real
                // findings in variant names.
                let mut plain: Vec<&String> = Vec::new();
                if let Some(ctors) = &class.constructors {
                    qualified.extend(ctors.keys());
                }
                if let Some(funcs) = &class.functions {
                    qualified.extend(funcs.keys());
                }
                if let Some(variants) = &class.enum_fields {
                    for map in variants {
                        qualified.extend(map.keys());
                        plain.extend(map.keys());
                    }
                }
                if let Some(fields) = &class.struct_fields {
                    for map in fields {
                        qualified.extend(map.keys());
                        plain.extend(map.keys());
                    }
                }
                for m in qualified {
                    names.insert(format!("{}::{}", class_name, m));
                    names.insert(format!("{}.{}", class_name, m));
                }
                for m in plain {
                    names.insert(m.clone());
                }
            }
        }
    }
    names
}

/// Fold a name to the shape every binding agrees on: ASCII letters and digits,
/// lowercased, with whatever separator the language chose thrown away.
///
/// `Dom.CreateBody` (C#), `Dom::create_body` (Rust) and `dom.create_body`
/// (Python) all fold to `domcreatebody`, so ONE api.json entry answers for
/// every binding's spelling of it. Without this the check reports a page for
/// writing its own language's naming convention.
fn fold_name(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The api.json spellings a C-ABI symbol could be. `AzDom_createBody` is
/// `Dom::createBody`; `AzUpdate_RefreshDom` is `Update::RefreshDom`; `AzUpdate`
/// is `Update`.
///
/// Deliberately does NOT yield the bare type or the bare member: accepting
/// `AzDom_thisWasNeverAFunction` because `Dom` exists would turn the check off
/// rather than fix it. The generated header is what settles a real member whose
/// api.json name is spelled differently.
///
/// Empty for anything that is not `Az` + an uppercase letter, so a page's own
/// `AzFoo` placeholder stays visible.
fn demangle_binding_name(token: &str) -> Vec<String> {
    let Some(rest) = token.strip_prefix("Az") else {
        return Vec::new();
    };
    if !rest.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return Vec::new();
    }
    match rest.split_once('_') {
        Some((ty, member)) if !ty.is_empty() && !member.is_empty() => {
            vec![format!("{ty}::{member}"), format!("{ty}.{member}")]
        }
        _ => vec![rest.to_string()],
    }
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Every `Az…` identifier in the generated C header.
///
/// The header is the one artifact that spells the C-ABI names exactly as a
/// binding page spells them, so a name it exports IS public API whatever
/// api.json calls it — this is what settles `AzString_fromConstStr`, whose
/// api.json counterpart is `String::from_c_str`, a different name entirely.
///
/// Returns an empty set (and no source) when codegen has never run; the check
/// then says so instead of blaming the page.
fn binding_symbols(project_root: &Path) -> (BTreeSet<String>, Option<String>) {
    const REL: &str = "target/codegen/azul.h";
    let Ok(src) = fs::read_to_string(project_root.join(REL)) else {
        return (BTreeSet::new(), None);
    };
    let mut out = BTreeSet::new();
    let bytes = src.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let starts_here = bytes[i] == b'A'
            && bytes.get(i + 1) == Some(&b'z')
            && (i == 0 || !is_ident_byte(bytes[i - 1]));
        if !starts_here {
            i += 1;
            continue;
        }
        let mut j = i + 2;
        while j < bytes.len() && is_ident_byte(bytes[j]) {
            j += 1;
        }
        // `Az` on its own is a namespace prefix in prose, not a symbol.
        if j > i + 2 {
            out.insert(src[i..j].to_string());
        }
        i = j;
    }
    (out, Some(REL.to_string()))
}

/// Everything a backticked name can legitimately be checked against.
///
/// This exists because a BINDING page does not spell api.json's names. The C
/// guide writes `AzDom_createBody`, C# writes `Dom.CreateBody`, api.json says
/// `Dom::create_body` — three spellings of one function. Comparing strings
/// reported all of them as "not public API": 17 phantom findings on the C page
/// alone, every one of which would have sent the fact-check agent hunting a
/// function that is right there in `azul.h`.
struct ApiVocabulary {
    /// api.json's own spellings, verbatim.
    names: BTreeSet<String>,
    /// The same names under [`fold_name`], so a naming convention cannot make a
    /// real name look missing.
    folded: BTreeSet<String>,
    /// `Az…` symbols exported by the generated C header.
    binding: BTreeSet<String>,
    /// Which file `binding` came from, or `None` if codegen has not run.
    binding_source: Option<String>,
}

impl ApiVocabulary {
    fn new(names: BTreeSet<String>, project_root: &Path) -> Self {
        let folded = names.iter().map(|n| fold_name(n)).collect();
        let (binding, binding_source) = binding_symbols(project_root);
        Self {
            names,
            folded,
            binding,
            binding_source,
        }
    }

    /// Does this project export the thing the page calls `token`, under ANY of
    /// the spellings a binding could use?
    fn knows(&self, token: &str) -> bool {
        if self.names.contains(token) || self.binding.contains(token) {
            return true;
        }
        if self.folded.contains(&fold_name(token)) {
            return true;
        }
        demangle_binding_name(token)
            .iter()
            .any(|n| self.names.contains(n) || self.folded.contains(&fold_name(n)))
    }

    /// One clause naming what a name was compared against, so the reader of a
    /// finding knows how hard azul-doc looked before reporting it.
    fn reference_clause(&self) -> String {
        match &self.binding_source {
            Some(src) => format!("api.json (any binding's spelling) or `{src}`"),
            None => "api.json (any binding's spelling)".to_string(),
        }
    }
}

/// Backticked identifiers in `body` that LOOK like public API names — a
/// CamelCase word, or a `Class::member` / `Class.member` pair.
///
/// Deliberately narrow. A guide legitimately names internal types the C API
/// never exposes, so a miss here is reported as "not public API", which is a
/// question for the writer, not a defect.
fn api_like_identifiers(body: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = body;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else { break };
        let token = &after[..close];
        rest = &after[close + 1..];
        if token.is_empty() || token.len() > 80 || token.contains(char::is_whitespace) {
            continue;
        }
        // Strip a call's parentheses and any generic tail: `Dom::create_div()`
        // and `Vec<NodeId>` both name something checkable.
        let token = token.split('(').next().unwrap_or(token);
        let token = token.split('<').next().unwrap_or(token);
        // SCREAMING_CASE is an env var or a constant (`AZ_RICING`), not a type,
        // and Rust's prelude is not azul's API. Both were pure noise.
        const PRELUDE: &[&str] = &[
            "Option", "Some", "None", "Vec", "String", "Result", "Ok", "Err", "Box", "Arc", "Rc",
            "HashMap", "BTreeMap", "Self", "Send", "Sync", "Copy", "Clone", "Debug", "Default",
            "Drop", "Iterator", "Into", "From",
        ];
        let screaming = token
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
        let looks_like_api = !screaming
            && !PRELUDE.contains(&token)
            && token
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase())
            && token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '.');
        if looks_like_api {
            out.insert(token.to_string());
        }
    }
    out
}

// ── Honesty: what the tracked source admits is unfinished ──────────────

/// One unfinished-work marker in a file the page documents.
#[derive(Clone)]
struct Marker {
    file: String,
    line: usize,
    text: String,
}

/// The markers a maintainer left in one file: `TODO` and `FIXME` in any
/// comment, plus the two macros that make a claim false at RUNTIME rather than
/// merely in prose — `todo!(` and `unimplemented!(`.
///
/// A missing or unreadable file yields nothing; check 1 already reports a
/// tracked file that does not exist, and reporting it twice teaches nothing.
fn scan_markers(project_root: &Path, rel: &str) -> Vec<Marker> {
    const NEEDLES: [&str; 4] = ["TODO", "FIXME", "todo!(", "unimplemented!("];
    let Ok(src) = fs::read_to_string(project_root.join(rel)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        if !NEEDLES.iter().any(|n| line.contains(n)) {
            continue;
        }
        let trimmed = line.trim();
        // A tracked file can be generated JSON with enormous lines. The marker
        // is the signal; the line it sits in is not worth a screenful.
        let text = if trimmed.chars().count() > 160 {
            trimmed.chars().take(157).collect::<String>() + "…"
        } else {
            trimmed.to_string()
        };
        out.push(Marker {
            file: rel.to_string(),
            line: i + 1,
            text,
        });
    }
    out
}

/// What the page and its source admit about their own maturity.
struct MaturityReport {
    /// The page's own `maturity:` frontmatter key, when it carries one.
    page_maturity: Option<String>,
    /// The guide's `*WIP.*` hedging paragraph, quoted, if the body has one.
    wip_paragraph: Option<String>,
    /// Tracked files that were actually read.
    files_scanned: usize,
    markers: Vec<Marker>,
}

/// Read every tracked file once per RUN, not once per page: `api.json` is
/// tracked by dozens of guides and is megabytes long.
fn maturity_report(
    project_root: &Path,
    fm: &Frontmatter,
    body: &str,
    cache: &mut BTreeMap<String, Vec<Marker>>,
) -> MaturityReport {
    let mut markers = Vec::new();
    let mut files_scanned = 0usize;
    for f in &fm.tracked_files {
        if !project_root.join(f).exists() {
            continue;
        }
        files_scanned += 1;
        let found = cache
            .entry(f.clone())
            .or_insert_with(|| scan_markers(project_root, f));
        markers.extend(found.iter().cloned());
    }
    MaturityReport {
        page_maturity: {
            let m = fm.maturity.trim();
            (!m.is_empty()).then(|| m.to_string())
        },
        // The guide's house style for "do not trust this yet" is a paragraph
        // that starts `*WIP.*`. It is prose, so only the body can be asked.
        wip_paragraph: body
            .lines()
            .find(|l| l.trim_start().starts_with("*WIP"))
            .map(|l| {
                let t = l.trim();
                if t.chars().count() > 200 {
                    t.chars().take(197).collect::<String>() + "…"
                } else {
                    t.to_string()
                }
            }),
        files_scanned,
        markers,
    }
}

/// Everything azul-doc can settle about one page without a model.
fn machine_checks(
    project_root: &Path,
    page_path: &Path,
    fm: &Frontmatter,
    body: &str,
    vocab: &ApiVocabulary,
    link_problems: &[lint_links::Problem],
) -> Vec<Check> {
    let mut checks = Vec::new();

    // 1. Do the tracked files still exist? A page whose subject moved is not
    //    merely stale, it documents a path that is not there.
    if fm.tracked_files.is_empty() {
        checks.push(Check {
            severity: "note",
            detail: "the page declares no `tracked_files`, so there is nothing to pin its claims \
                     to. Consider adding the files it documents."
                .to_string(),
        });
    }
    for f in &fm.tracked_files {
        if !project_root.join(f).exists() {
            checks.push(Check {
                severity: "FIX",
                detail: format!(
                    "`{f}` is listed in `tracked_files` and DOES NOT EXIST at this commit — the \
                     page documents a file that has moved or been deleted."
                ),
            });
        }
    }

    // 2. What has changed under the page since it was last generated? This is
    //    the list of commits the prose has never seen.
    match &fm.last_generated_rev {
        None => checks.push(Check {
            severity: "note",
            detail: "no `last_generated_rev`: this page was written by hand, so there is no \
                     baseline to diff against. Treat every claim as unverified."
                .to_string(),
        }),
        Some(rev) => {
            let mut any = false;
            for f in &fm.tracked_files {
                let commits = commits_since(project_root, rev, f).unwrap_or_default();
                if !commits.is_empty() {
                    any = true;
                    checks.push(Check {
                        severity: "FIX",
                        detail: format!(
                            "`{f}`: {} commit(s) since `last_generated_rev` ({}). The prose \
                             predates them.",
                            commits.len(),
                            &rev[..12.min(rev.len())],
                        ),
                    });
                }
            }
            if !any {
                checks.push(Check {
                    severity: "ok",
                    detail: format!(
                        "no tracked file has changed since `last_generated_rev` ({}).",
                        &rev[..12.min(rev.len())],
                    ),
                });
            }
        }
    }

    // 3. Names the page presents as API that this project does not export
    //    under ANY binding's spelling. Either the name is wrong, or it is
    //    internal and the page should not present it as the reader's API, or
    //    it is a placeholder the example invented.
    //
    //    "Under any binding's spelling" is the whole point: a C page writes
    //    `AzDom_createBody` for what api.json calls `Dom::create_body`, and
    //    reporting that as missing is noise the reader has to disprove.
    let unknown: Vec<String> = api_like_identifiers(body)
        .into_iter()
        .filter(|n| !vocab.knows(n))
        .collect();
    if unknown.is_empty() {
        checks.push(Check {
            severity: "ok",
            detail: format!(
                "every backticked API-looking name in the prose resolves to {}.",
                vocab.reference_clause(),
            ),
        });
    } else {
        checks.push(Check {
            severity: "check",
            detail: format!(
                "{} backticked name(s) resolve to nothing in {} — a renamed function, an internal \
                 type the page presents as the reader's, or a name the example invented: {}",
                unknown.len(),
                vocab.reference_clause(),
                unknown.join(", "),
            ),
        });
    }
    if vocab.binding_source.is_none() {
        checks.push(Check {
            severity: "note",
            detail: "`target/codegen/azul.h` is not present, so C-ABI spellings \
                     (`AzDom_createBody`) could only be resolved by demangling. Any `Az…` name \
                     listed above may be a false alarm; run `azul-doc codegen all` and re-run to \
                     settle it."
                .to_string(),
        });
    }

    // 4. Pointers that do not resolve. `check_guide_links` already proved
    //    these against the worktree; carry this page's share into the bundle so
    //    the writer sees them without running anything.
    let rel = page_path
        .strip_prefix(project_root)
        .unwrap_or(page_path)
        .to_string_lossy()
        .replace('\\', "/");
    for p in link_problems.iter().filter(|p| p.file == rel) {
        checks.push(Check {
            severity: "FIX",
            detail: format!("line {}: {}", p.line, p.detail),
        });
    }

    checks
}

// ── The bundle ─────────────────────────────────────────────────────────

/// A fence long enough to quote a markdown page that itself contains fences.
const OUTER_FENCE: &str = "`````";

/// How many markers a bundle prints before it starts eliding. A page that
/// tracks `dll/src/lib.rs` would otherwise bury its own prose under a work
/// list that belongs in an issue tracker.
const MARKER_CAP: usize = 40;

fn render_bundle(
    guide: &Guide,
    page_rel: &str,
    raw_page: &str,
    fm: &Frontmatter,
    head: &Head,
    checks: &[Check],
    maturity: &MaturityReport,
    agent_pass: bool,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "<!-- Generated by `azul-doc assemble-context`. Build output: regenerate, never \
         hand-edit. -->\n\n# Context bundle — {}\n\n",
        guide.title
    ));
    out.push_str(&format!(
        "- page: `{page_rel}`\n- commit: `{}`{}\n- commit date: {}\n- audience: {}\n- tracked \
         files: {}\n\n",
        if head.sha.is_empty() {
            "unknown"
        } else {
            &head.sha
        },
        if head.dirty {
            " **(NOT what this bundle was built from — see the warning below)**"
        } else {
            ""
        },
        head.date,
        guide.audience.as_deref().unwrap_or("unspecified"),
        if fm.tracked_files.is_empty() {
            "(none declared)".to_string()
        } else {
            fm.tracked_files
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(", ")
        },
    ));

    // A dirty tree is the normal state while writing docs, so this is a
    // warning and not a refusal — but the bundle must not claim a commit it
    // cannot vouch for. Excerpts were read from disk, and disk is not the SHA.
    if head.dirty {
        out.push_str(
            "> ⚠ **The working tree has UNCOMMITTED CHANGES.** Everything below was read from \
             the working tree, not from the commit named above. Excerpts and line numbers \
             describe the files as they are on disk; they may not match that commit, and the \
             commit alone will not reproduce them.\n\n",
        );
    }

    out.push_str(
        "## How to use this file\n\nYou are rewriting the page in section 1. Everything you need \
         is in this file — do not go looking for anything else.\n\n1. Read section 1: the page as \
         it ships today.\n2. Read sections 2, 2b and 3: every statement in them is a fact about \
         the source, produced by reading it, and outranks the page.\n3. Read section 4: verbatim \
         source excerpts. They are the evidence. If the page and an excerpt disagree, the excerpt \
         is right.\n4. Read section 5: how to write the page. It is the last word on style, \
         honesty and what your output may contain.\n5. Emit the COMPLETE new page — YAML \
         frontmatter first, then the prose. Keep the frontmatter's `slug`, `title`, `language`, \
         `audience` and `guide_order` exactly as they are; update `tracked_files` if section 2 \
         says a file moved, and set `last_generated_rev` to the commit above.\n6. Do not append \
         the sources to the page. They are context, not content.\n\n",
    );

    out.push_str("## 1. The page as it ships today\n\n");
    out.push_str(OUTER_FENCE);
    out.push_str("markdown\n");
    out.push_str(raw_page);
    if !raw_page.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(OUTER_FENCE);
    out.push_str("\n\n");

    out.push_str(
        "## 2. Machine checks\n\nProduced by `azul-doc` from the tree itself — no model was \
         involved, so these are not opinions. `FIX` means the page is wrong today; `check` means \
         a human or the fact-check below must decide; `ok` is recorded so its absence is \
         meaningful.\n\n",
    );
    if checks.is_empty() {
        out.push_str("_(no checks produced output)_\n\n");
    } else {
        for c in checks {
            out.push_str(&format!("- **{}** — {}\n", c.severity, c.detail));
        }
        out.push('\n');
    }

    out.push_str(&render_maturity_section(fm, maturity));

    if agent_pass {
        out.push_str(
            "## 3. Fact-check\n\n<!-- assemble-context: the agent replaces this block. -->\n\n_Not \
             filled in yet._\n\n## 4. Sources\n\n<!-- assemble-context: the agent replaces this \
             block. -->\n\n_Not filled in yet._\n\n",
        );
    } else {
        out.push_str(
            "## 3. Fact-check\n\n_Skipped: this bundle was assembled with `--no-agent`, so nothing \
             read the prose against the source. Sections 2 and 2b are still authoritative._\n\n## \
             4. Sources\n\n_Skipped: `--no-agent`. Every code example in the page is therefore \
             unverified: do not add new ones, and delete any you cannot justify from the page \
             itself._\n\n",
        );
    }

    out.push_str(WRITING_INSTRUCTIONS);
    out
}

/// Section 2b: what the page's own source admits is unfinished.
///
/// The absence of markers is as informative as their presence — it is the only
/// evidence the writing model gets that a hedge in the prose is obsolete — so
/// an empty list is printed loudly instead of being skipped.
fn render_maturity_section(fm: &Frontmatter, m: &MaturityReport) -> String {
    let mut out = String::from(
        "## 2b. Unfinished work behind this page\n\nEvery `TODO`, `FIXME`, `todo!(` and \
         `unimplemented!(` in the files this page tracks, so the page cannot describe an \
         unfinished thing as finished. Machine-produced from the tree; no model was involved.\n\n",
    );

    match &m.page_maturity {
        Some(v) => out.push_str(&format!(
            "- the page declares `maturity: {v}` in its own frontmatter.\n"
        )),
        None => out.push_str("- the page declares no `maturity` key.\n"),
    }
    match &m.wip_paragraph {
        Some(p) => out.push_str(&format!("- the page carries a WIP paragraph: {p}\n")),
        None => out.push_str("- the page carries no `*WIP.*` paragraph.\n"),
    }

    if fm.tracked_files.is_empty() {
        out.push_str(
            "- the page tracks no files, so nothing could be scanned. THIS IS NOT EVIDENCE THAT \
             THE FEATURE IS FINISHED — it is evidence that nobody said which source to look at.\n\n",
        );
        return out;
    }

    if m.markers.is_empty() {
        out.push_str(&format!(
            "- **{} tracked file(s) scanned; NO markers found.** The code behind this page carries \
             no admission of unfinished work. If the page hedges — a `maturity: wip` key, a \
             `*WIP.*` paragraph, \"not yet\", \"planned\" — the hedge is unsupported and should \
             go.\n\n",
            m.files_scanned,
        ));
        return out;
    }

    out.push_str(&format!(
        "- **{} tracked file(s) scanned; {} marker(s) found.** A page that describes any of these \
         as working is wrong, not merely optimistic.\n",
        m.files_scanned,
        m.markers.len(),
    ));
    for mk in m.markers.iter().take(MARKER_CAP) {
        out.push_str(&format!("- `{}:{}` — {}\n", mk.file, mk.line, mk.text));
    }
    if m.markers.len() > MARKER_CAP {
        out.push_str(&format!(
            "\n_({} further marker(s) not listed; the cap is {}. Grep the tracked files if you \
             need them all.)_\n",
            m.markers.len() - MARKER_CAP,
            MARKER_CAP,
        ));
    }
    out.push('\n');
    out
}

// ── The writing model's instructions ───────────────────────────────────

/// Section 5. The bundle's last word, and the only part addressed to the model
/// that actually emits the page.
///
/// It is a constant because it must be IDENTICAL in every bundle: the writing
/// model is a fresh context each time, and a rule that varies per page is a
/// rule that gets applied inconsistently across the guide.
const WRITING_INSTRUCTIONS: &str = r#"## 5. How to write this page

You are the writing model. Everything above is your input; this section is your brief.

### What you output

- Output the finished Markdown page and NOTHING else. No preamble, no sign-off, no "here is the
  rewritten page", no explanation of what you changed.
- The reader has never seen this codebase and will never see this bundle. Never mention the
  bundle, the fact-check, the sources, a model, or the fact that the page was rewritten.

### How it reads

- Short sentences, one idea each. Plain words over clever ones. Prefer the concrete noun to the
  abstraction.
- Say what a thing does, and why a reader would want it, BEFORE how to call it.
- Do not write: we, let's, simply, just, easy, obviously, note that, it's worth mentioning, in
  this section we will. Do not hedge when the source settles the answer.
- Match the shipping page's length unless the findings require more.

### What you may claim

- Every code example must be copied from section 4 verbatim, or assembled only from signatures
  shown there. Never invent an API, a field, an argument or a default. If section 4 does not
  settle it, leave it out.
- Fix every finding in section 3. A finding marked WRONG must not survive in any form.
- Keep the page's front matter, its title, and the headings a reader navigates by. Reorganise
  only where section 3 shows the current order misleads.

### Honesty about maturity

- If the marker list in section 2b is empty for this page's tracked files, the feature ships:
  delete any `*WIP.*` paragraph, and drop the `wip` claim from the front matter by setting
  `maturity: mature`. (The `maturity` key itself is required — change its value, never remove
  the key.)
- If markers remain, keep ONE plain sentence naming exactly what is not finished, and delete the
  vague hedging around it.
- Never describe something as working when the source has a `todo!` or `unimplemented!` for it.
"#;

// ── The agent's instructions ───────────────────────────────────────────

fn render_prompt(project_root: &Path, guide: &Guide, page_rel: &str, head: &Head) -> String {
    let bundle = bundle_path(project_root, &guide.file_name);
    let bundle_rel = bundle
        .strip_prefix(project_root)
        .unwrap_or(&bundle)
        .to_string_lossy()
        .replace('\\', "/");

    format!(
        r#"You are assembling CONTEXT for a documentation page. You are not writing the page.

A different model — one with no tools — will rewrite `{page_rel}` from the bundle at
`{bundle_rel}` and nothing else. Your job is to put everything that model needs into that
bundle, and to make sure none of it is wrong.

The bundle already contains the page as it ships (section 1), the checks azul-doc could settle
without a model (sections 2 and 2b), and the writing model's own brief (section 5). You fill in
sections 3 and 4, in place, with Edit.

## The tree you are working against

Commit `{sha}`. Read the source with Read and Grep at this commit; do not reason from memory
about what azul's API looks like, and do not trust the page you are checking — it is the thing
under test.
{dirty_note}

## Section 3 — Fact-check

Read the page's prose claim by claim. A claim is anything a reader could act on: a type has a
field, a function takes an argument, a default is 16px, an event fires before another, a
feature flag gates something, a file lives at a path. For each claim you can settle, decide:
WRONG, STALE (true once, not now), UNVERIFIABLE (nothing in the source decides it), or leave it
out if it is right — a fact-check listing everything it agreed with is unreadable.

Write one entry per finding:

```
### <short claim, quoted from the page>
- **Verdict:** WRONG | STALE | UNVERIFIABLE
- **The page says:** <quote>
- **The source says:** <what is actually true>
- **Evidence:** `path/to/file.rs:120-134` (excerpt in section 4)
```

Order them most-misleading first. A wrong function signature a reader would type outranks a
stale sentence in an overview paragraph. If you find nothing wrong, say so in one line — that
is a real and useful result.

A page that describes something as working while its tracked source has a `todo!(` or
`unimplemented!(` for that thing is a WRONG finding, not a stylistic one — the reader's program
panics. Section 2b lists those markers; check the ones that touch what the page promises, and
record the verdict with the marker's `path:line` as evidence. The reverse matters too: a page
hedging about a feature whose source carries no marker at all is claiming a limitation that does
not exist, which is also WRONG.

Also flag what the page is MISSING that its tracked files clearly support: a public entry point
with no mention, a footgun the source guards against and the prose never warns about. One line
each under a `### Missing` heading. Do not turn this into a wish list; only name things the
source proves matter.

## Section 4 — Sources

Verbatim excerpts, at this commit, that settle the findings above and support the page's main
claims. This is the ONLY view of the source the writing model will get, so it has to stand
alone.

For each excerpt:

```
### `path/to/file.rs:120-134` — <what this excerpt settles, one line>

```rust
<the lines, copied EXACTLY, no elisions, no edits, no "...">
```
```

Rules that matter:

- **Verbatim or not at all.** If you paraphrase inside a fence, the writing model will copy your
  paraphrase into the docs as if it were code. Copy the bytes. If a block is too long to include,
  pick a smaller block — do not summarize it inside the fence.
- **Line numbers must be real** and must match the excerpt at this commit.
- **Signatures over bodies.** A reader needs `pub fn foo(&mut self, x: Bar) -> Baz`, its doc
  comment, and the two lines that show the invariant — rarely the whole function.
- **Include the doc comments.** They are usually the best prose in the repo and the writing model
  should be able to draw on them.
- **Cover the page, not just the errors.** Every major section of the page should have at least
  one excerpt behind it, so the writer can rewrite that section without guessing.
- Aim for 8–20 excerpts. Under 5 means you did not look; over 30 means you are pasting files.

## Rules

- Edit ONLY `{bundle_rel}`. Do not touch the guide page, the source, or anything else.
- Replace the two `_Not filled in yet._` placeholders and nothing else. Sections 1, 2, 2b and 5
  stay byte-for-byte as they are: section 1 is the input the writer diffs against, sections 2 and
  2b are machine output you cannot improve, and section 5 is the writing model's brief — it is not
  addressed to you and must reach it intact, at the END of the file.
- Do not rewrite the prose, do not suggest wording, do not add a "suggested rewrite" section. The
  writing model does that, and your suggestions would anchor it to your phrasing.
- If the page is fine and well-covered, a short section 3 and a solid section 4 is the correct
  result. Do not invent findings to look thorough.

When you are done, reply with one line: the number of findings and the number of excerpts.
"#,
        page_rel = page_rel,
        bundle_rel = bundle_rel,
        sha = if head.sha.is_empty() {
            "HEAD".to_string()
        } else {
            head.sha.clone()
        },
        // The excerpts you are about to paste come off disk. If disk is not the
        // commit, saying "at commit X" in section 4 is a claim the bundle
        // cannot back, so the agent is told to describe the tree it can see.
        dirty_note = if head.dirty {
            "\nThe working tree has UNCOMMITTED CHANGES. Read the files as they are on disk, and\nwherever these instructions say \"at this commit\", read \"in the working tree\": your excerpts\nand line numbers must match what is on disk, not what that commit holds. Say so in section 4 if\nan excerpt comes from a modified file."
        } else {
            ""
        },
    )
}

// ── Entry point ────────────────────────────────────────────────────────

/// Assemble a context bundle for every shipping guide page.
///
/// # Errors
/// Fails if the bundle directory cannot be created or an agent dispatch fails.
pub fn run(cfg: &Config) -> Result<(), String> {
    let project_root = &cfg.project_root;
    let head = read_head(project_root);
    let bdir = bundles_dir(project_root);
    let pdir = prompts_dir(project_root);
    fs::create_dir_all(&pdir).map_err(|e| format!("create {}: {}", pdir.display(), e))?;

    // api.json and the link lint are per-tree, not per-page: pay for them once.
    let api_path = project_root.join("api.json");
    let api_names = fs::read_to_string(&api_path)
        .ok()
        .and_then(|s| serde_json::from_str::<ApiData>(&s).ok())
        .map(|api| public_api_names(&api))
        .unwrap_or_default();
    if api_names.is_empty() {
        eprintln!(
            "[warn] api.json produced no names ({}); the public-API check will be silent.",
            api_path.display()
        );
    }
    let vocab = ApiVocabulary::new(api_names, project_root);
    if vocab.binding_source.is_none() {
        eprintln!(
            "[warn] target/codegen/azul.h not found; C-ABI spellings (`AzDom_createBody`) can \
             only be demangled, not confirmed. Run `azul-doc codegen all` for a cleaner \
             public-API check."
        );
    }
    let link_problems = lint_links::check_guide_links(project_root);
    // Tracked files repeat across pages — `api.json` alone is tracked by dozens
    // and is megabytes long — so each file is read at most once per run.
    let mut marker_cache: BTreeMap<String, Vec<Marker>> = BTreeMap::new();

    let guides = get_guide_list();
    println!(
        "assemble-context: {} shipping guide page(s) at {}{}",
        guides.len(),
        if head.sha.is_empty() {
            "an unknown commit"
        } else {
            &head.sha[..12.min(head.sha.len())]
        },
        if head.dirty { " (dirty tree)" } else { "" },
    );

    // (bundle path relative to the project root, page title) — the summary's
    // whole job is to be pasteable, so it must not print absolute paths the
    // owner would have to edit.
    let mut written: Vec<(String, String)> = Vec::new();
    let mut skipped = 0usize;
    for guide in &guides {
        if let Some(f) = &cfg.filter {
            if !guide.file_name.contains(f.as_str()) {
                skipped += 1;
                continue;
            }
        }
        let page_rel = format!("doc/guide/en/{}.md", guide.file_name);
        let page_path = project_root.join(&page_rel);
        let raw_page = match fs::read_to_string(&page_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[warn] {}: {}", page_rel, e);
                continue;
            }
        };
        // A page with no frontmatter still gets a bundle — it is exactly the
        // page most likely to be wrong — with a stub Frontmatter standing in.
        // The stub carries the three keys serde has no default for, so an
        // unparsable page produces a bundle instead of a panic.
        let (fm, body) = parse_frontmatter(&raw_page).unwrap_or_else(|| {
            let stub = serde_yaml::from_str("slug: ''\ntitle: ''\nmaturity: ''")
                .expect("stub frontmatter carries every field without a serde default");
            (stub, raw_page.clone())
        });

        let checks = machine_checks(project_root, &page_path, &fm, &body, &vocab, &link_problems);
        let maturity = maturity_report(project_root, &fm, &body, &mut marker_cache);
        let bundle = render_bundle(
            guide,
            &page_rel,
            &raw_page,
            &fm,
            &head,
            &checks,
            &maturity,
            !cfg.no_agent,
        );
        let bpath = bundle_path(project_root, &guide.file_name);
        fs::write(&bpath, bundle).map_err(|e| format!("write {}: {}", bpath.display(), e))?;

        if !cfg.no_agent {
            let ppath = prompt_path(project_root, &guide.file_name);
            let status = crate::spec::executor::classify_prompt(&ppath, cfg.retry_failed);
            if !matches!(
                status,
                crate::spec::executor::PromptStatus::Done
                    | crate::spec::executor::PromptStatus::Taken { .. }
            ) {
                fs::write(&ppath, render_prompt(project_root, guide, &page_rel, &head))
                    .map_err(|e| format!("write {}: {}", ppath.display(), e))?;
            }
        }
        written.push((
            bpath
                .strip_prefix(project_root)
                .unwrap_or(&bpath)
                .to_string_lossy()
                .replace('\\', "/"),
            guide.title.clone(),
        ));
    }

    println!(
        "wrote {} bundle(s) to {} ({} filtered out)",
        written.len(),
        bdir.display(),
        skipped
    );

    if cfg.no_agent {
        println!("--no-agent: sections 3 and 4 were left unfilled.");
        print_summary(project_root, &written);
        return Ok(());
    }
    if cfg.dry_run {
        println!("--dry-run: prompts written, no agents dispatched.");
        println!("  prompts: {}", pdir.display());
        print_summary(project_root, &written);
        return Ok(());
    }

    println!("\n=== Dispatching fact-check agents ===\n");
    let ar = AutoreviewConfig {
        project_root: project_root.clone(),
        agents: cfg.agents,
        timeout: cfg.timeout,
        model: cfg.model.clone(),
        file_filter: None,
        retry_failed: cfg.retry_failed,
        dry_run: false,
        status_only: false,
        strict: false,
        subcommand: AutoreviewSubcommand::Autodoc,
        limit: None,
        reference: None,
    };
    crate::reftest::autodoc::dispatch_prompt_agents(&ar, &pdir, "CTX")?;

    print_summary(project_root, &written);
    Ok(())
}

/// Above this many bundles, one line each is a wall the owner scrolls past.
const SUMMARY_LIST_CAP: usize = 12;

/// The last thing the command prints: where the bundles are and how to get one
/// into a writing model.
///
/// The bundle is only useful once it is in a chat window, and on macOS that is
/// `pbcopy`. Printing the exact command removes the step where the owner has to
/// go find the file and remember what the tool called it.
fn print_summary(project_root: &Path, written: &[(String, String)]) {
    let bdir = bundles_dir(project_root);
    let bdir_rel = bdir
        .strip_prefix(project_root)
        .unwrap_or(&bdir)
        .to_string_lossy()
        .replace('\\', "/");

    if written.is_empty() {
        println!("\nNo bundles were assembled (every page was filtered out).");
        return;
    }

    println!(
        "\n=== {} bundle(s) assembled in {} ===\n",
        written.len(),
        bdir_rel,
    );

    if written.len() <= SUMMARY_LIST_CAP {
        // Widest path first so the `#` comments line up in a terminal.
        let width = written.iter().map(|(p, _)| p.len()).max().unwrap_or(0);
        for (path, title) in written {
            println!("pbcopy < {path:<width$}   # {title}");
        }
    } else {
        println!("Too many to list. To copy one:\n");
        println!("  pbcopy < {bdir_rel}/<slug>.md          # slugs: `ls {bdir_rel}`\n");
        println!("To work through all of them, one paste at a time:\n");
        println!("  for f in {bdir_rel}/*.md; do");
        println!("      pbcopy < \"$f\"; echo \"copied $f — paste it, then press enter\"; read _");
        println!("  done");
    }

    println!(
        "\nEach bundle is self-contained — the page, the checks, the fact-check, verbatim source \
         excerpts and the writing brief — so it can be pasted straight into a writing model with \
         no tools and no repo access.",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifier scanner has to be narrow enough that a page full of
    /// prose in backticks does not drown the real finding.
    #[test]
    fn only_api_shaped_backticks_are_collected() {
        let found = api_like_identifiers(
            "Use `Dom::create_div()` inside `NodeData`, not `foo`, `--flag`, `a b`, or \
             `Vec<NodeId>`.",
        );
        assert!(found.contains("Dom::create_div"), "{found:?}");
        assert!(found.contains("NodeData"), "{found:?}");
        // Lowercase words, flags and phrases are prose, not API.
        assert!(!found.contains("foo"), "{found:?}");
        assert!(!found.contains("--flag"), "{found:?}");
        assert!(!found.contains("a b"), "{found:?}");
        // Rust's prelude is not azul's API: `Vec<NodeId>` contributes nothing.
        assert!(!found.contains("Vec"), "{found:?}");

        // An env var is not a type name.
        assert!(
            api_like_identifiers("set `AZ_RICING=1`").is_empty(),
            "SCREAMING_CASE is an env var, not API",
        );
    }

    /// A page quoting fenced code must not be able to close the bundle's own
    /// fence — the writing model would then read the rest of the bundle as
    /// prose it is supposed to emit.
    #[test]
    fn the_embedded_page_cannot_break_out_of_its_fence() {
        assert!(
            OUTER_FENCE.len() > 3,
            "a 3-backtick fence would be closed by the page's own code blocks",
        );
    }

    fn vocab_of(names: &[&str], binding: &[&str]) -> ApiVocabulary {
        let names: BTreeSet<String> = names.iter().map(|s| (*s).to_string()).collect();
        ApiVocabulary {
            folded: names.iter().map(|n| fold_name(n)).collect(),
            names,
            binding: binding.iter().map(|s| (*s).to_string()).collect(),
            binding_source: (!binding.is_empty()).then(|| "target/codegen/azul.h".to_string()),
        }
    }

    /// The bug this check existed to cause: a binding page spells the same
    /// function its own way, and every one of those spellings was reported as
    /// "not public API".
    #[test]
    fn a_bindings_spelling_of_a_real_function_is_not_a_finding() {
        let v = vocab_of(&["Dom", "Dom::create_body", "Dom.create_body"], &[]);
        // C, C#, Python, Rust — one function, four spellings.
        for name in [
            "AzDom_createBody",
            "Dom.CreateBody",
            "Dom.create_body",
            "AzDom",
        ] {
            assert!(v.knows(name), "{name} should resolve to Dom::create_body");
        }
    }

    /// Resolving manglings must not degrade into accepting everything: a
    /// member the project does not have is still a finding, even when its type
    /// exists.
    #[test]
    fn demangling_does_not_accept_an_invented_member() {
        let v = vocab_of(&["Dom", "Dom::create_body"], &[]);
        assert!(!v.knows("AzDom_thisWasNeverAFunction"));
        assert!(!v.knows("AzFoo"), "a page's placeholder type is a finding");
        assert!(!v.knows("Az"), "a bare namespace prefix is not a symbol");
    }

    /// `AzString_fromConstStr` is exported by the C header and has NO api.json
    /// counterpart under that name (api.json calls it `String::from_c_str`), so
    /// only the generated bindings can clear it.
    #[test]
    fn a_header_only_symbol_counts_as_public_api() {
        let v = vocab_of(
            &["String", "String::from_c_str"],
            &["AzString_fromConstStr"],
        );
        assert!(v.knows("AzString_fromConstStr"));
        assert!(!v.knows("AzString_fromWishfulThinking"));
    }

    /// Without the header, a C name that only the header could settle stays a
    /// finding — and the bundle has to say the header was missing rather than
    /// let the reader assume the name is wrong.
    #[test]
    fn a_missing_header_is_reported_not_papered_over() {
        let v = vocab_of(&["String", "String::from_c_str"], &[]);
        assert!(!v.knows("AzString_fromConstStr"));
        assert!(
            !v.reference_clause().contains("azul.h"),
            "the clause must not name a file that was not read",
        );
    }

    /// The scanner has to find both the comment markers and the two macros
    /// that make a page's claim false at runtime.
    #[test]
    fn markers_are_found_with_their_line_numbers() {
        let dir = std::env::temp_dir().join("az-assemble-context-markers");
        fs::create_dir_all(&dir).unwrap();
        let rel = "sample.rs";
        fs::write(
            dir.join(rel),
            "fn a() {}\n// TODO: wire this up\nfn b() { todo!(\"later\") }\nfn c() {}\n",
        )
        .unwrap();
        let found = scan_markers(&dir, rel);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].line, 2);
        assert_eq!(found[0].text, "// TODO: wire this up");
        assert_eq!(found[1].line, 3);
        assert!(found[1].text.contains("todo!("));
    }

    /// The absence of markers is the signal the owner asked for, so it must be
    /// stated out loud rather than left as an empty list.
    #[test]
    fn an_empty_marker_list_is_stated_explicitly() {
        let fm: Frontmatter = serde_yaml::from_str(
            "slug: x\ntitle: X\nmaturity: wip\ntracked_files: [core/src/a.rs]",
        )
        .unwrap();
        let clean = MaturityReport {
            page_maturity: Some("wip".to_string()),
            wip_paragraph: None,
            files_scanned: 1,
            markers: Vec::new(),
        };
        let rendered = render_maturity_section(&fm, &clean);
        assert!(rendered.contains("NO markers found"), "{rendered}");
        assert!(rendered.contains("maturity: wip"), "{rendered}");
    }

    /// No page hits the cap today (the worst tracks 22 markers), which is
    /// exactly why the branch needs a test: the first page that tracks a big
    /// file would otherwise be the first to run it.
    #[test]
    fn a_long_marker_list_is_capped_and_says_what_it_elided() {
        let fm: Frontmatter =
            serde_yaml::from_str("slug: x\ntitle: X\nmaturity: wip\ntracked_files: [a.rs]")
                .unwrap();
        let rendered = render_maturity_section(
            &fm,
            &MaturityReport {
                page_maturity: None,
                wip_paragraph: Some("*WIP.* not done".to_string()),
                files_scanned: 1,
                markers: (1..=MARKER_CAP + 3)
                    .map(|line| Marker {
                        file: "a.rs".to_string(),
                        line,
                        text: "// TODO: x".to_string(),
                    })
                    .collect(),
            },
        );
        assert_eq!(rendered.matches("// TODO: x").count(), MARKER_CAP);
        assert!(
            rendered.contains("3 further marker(s) not listed"),
            "{rendered}"
        );
        assert!(rendered.contains("*WIP.* not done"), "{rendered}");
    }

    /// Section 5 is the writing model's only brief, and a model reads the end
    /// of a long document best — so the bundle must END with it.
    #[test]
    fn the_bundle_ends_with_the_writing_brief() {
        let guide = Guide {
            title: "Hello World [C]".to_string(),
            file_name: "hello-world/c".to_string(),
            content: String::new(),
            audience: Some("external".to_string()),
            guide_order: Some(12),
            description: None,
            default_search_keys: Vec::new(),
        };
        let fm: Frontmatter = serde_yaml::from_str("slug: x\ntitle: X\nmaturity: wip").unwrap();
        let head = Head {
            sha: "abc123".to_string(),
            date: "2026-01-01T00:00:00Z".to_string(),
            dirty: true,
        };
        let m = MaturityReport {
            page_maturity: Some("wip".to_string()),
            wip_paragraph: None,
            files_scanned: 0,
            markers: Vec::new(),
        };
        let out = render_bundle(
            &guide,
            "doc/guide/en/x.md",
            "# X\n",
            &fm,
            &head,
            &[],
            &m,
            true,
        );
        assert!(out.trim_end().ends_with(WRITING_INSTRUCTIONS.trim_end()));
        assert!(out.contains("## 5. How to write this page"));
        assert!(out.contains("## 2b. Unfinished work behind this page"));
        // A dirty tree cannot be a footnote: the bundle names a commit its
        // excerpts did not come from.
        assert!(out.contains("UNCOMMITTED CHANGES"), "{out}");
        // Section 4 must still come before section 5, or the agent's excerpts
        // would land after the brief.
        let (s4, s5) = (out.find("## 4. Sources"), out.find("## 5. How to write"));
        assert!(s4 < s5 && s4.is_some());
    }

    /// The prompt is the agent's only instruction sheet; if the tree is dirty
    /// and the prompt does not say so, the agent pins line numbers to a commit
    /// that does not have them.
    #[test]
    fn the_prompt_admits_a_dirty_tree() {
        let guide = Guide {
            title: "X".to_string(),
            file_name: "x".to_string(),
            content: String::new(),
            audience: None,
            guide_order: None,
            description: None,
            default_search_keys: Vec::new(),
        };
        let root = Path::new("/tmp/x");
        let dirty = Head {
            sha: "abc".to_string(),
            date: String::new(),
            dirty: true,
        };
        let clean = Head {
            sha: "abc".to_string(),
            date: String::new(),
            dirty: false,
        };
        assert!(render_prompt(root, &guide, "p.md", &dirty).contains("UNCOMMITTED CHANGES"));
        assert!(!render_prompt(root, &guide, "p.md", &clean).contains("UNCOMMITTED CHANGES"));
    }

    /// A nested page keeps its directory in the bundle name, so two pages
    /// with the same stem cannot overwrite each other.
    #[test]
    fn nested_pages_get_distinct_bundle_names() {
        let root = Path::new("/tmp/x");
        assert_ne!(
            bundle_path(root, "dom"),
            bundle_path(root, "internals/dom"),
            "a nested page must not collide with a top-level one",
        );
        assert!(bundle_path(root, "internals/dom")
            .to_string_lossy()
            .ends_with("internals__dom.md"));
    }
}
