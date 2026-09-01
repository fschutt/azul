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
//! carries four sections: the page exactly as it ships, the machine checks
//! azul-doc can answer with no model at all, a fact-check the agent fills in,
//! and a `## Sources` appendix of real excerpts pinned to the current commit.
//! The writing model then needs nothing but the bundle.
//!
//! Two properties are deliberate:
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
//!
//! Which pages count as shipping is `docgen::guide::get_guide_list()` — the
//! same list the website builds from, so a page that does not ship does not get
//! a bundle and a page that ships cannot be forgotten.

use std::{
    collections::BTreeSet,
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

/// Everything azul-doc can settle about one page without a model.
fn machine_checks(
    project_root: &Path,
    page_path: &Path,
    fm: &Frontmatter,
    body: &str,
    api_names: &BTreeSet<String>,
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

    // 3. Names the page presents as API that api.json does not export. Either
    //    the name is wrong, or it is internal and the page should not present
    //    it as the reader's API.
    let unknown: Vec<String> = api_like_identifiers(body)
        .into_iter()
        .filter(|n| !api_names.contains(n))
        .collect();
    if unknown.is_empty() {
        checks.push(Check {
            severity: "ok",
            detail: "every backticked API-looking name in the prose exists in api.json."
                .to_string(),
        });
    } else {
        checks.push(Check {
            severity: "check",
            detail: format!(
                "{} backticked name(s) are not public API in api.json — a renamed function, or \
                 an internal type the page presents as the reader's: {}",
                unknown.len(),
                unknown.join(", "),
            ),
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

fn render_bundle(
    guide: &Guide,
    page_rel: &str,
    raw_page: &str,
    fm: &Frontmatter,
    head: &Head,
    checks: &[Check],
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
            " **(working tree DIRTY — excerpts below may not exist at this SHA)**"
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

    out.push_str(
        "## How to use this file\n\nYou are rewriting the page in section 1. Everything you need \
         is in this file — do not go looking for anything else.\n\n1. Read section 1: the page as \
         it ships today.\n2. Read sections 2 and 3: every statement in them is a fact about the \
         source at the commit above, and outranks the page.\n3. Read section 4: verbatim source \
         excerpts. They are the evidence. If the page and an excerpt disagree, the excerpt is \
         right.\n4. Emit the COMPLETE new page — YAML frontmatter first, then the prose. Keep the \
         frontmatter's `slug`, `title`, `language`, `audience` and `guide_order` exactly as they \
         are; update `tracked_files` if section 2 says a file moved, and set `last_generated_rev` \
         to the commit above.\n5. Do not append the sources to the page. They are context, not \
         content.\n\n",
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

    if agent_pass {
        out.push_str(
            "## 3. Fact-check\n\n<!-- assemble-context: the agent replaces this block. -->\n\n_Not \
             filled in yet._\n\n## 4. Sources\n\n<!-- assemble-context: the agent replaces this \
             block. -->\n\n_Not filled in yet._\n",
        );
    } else {
        out.push_str(
            "## 3. Fact-check\n\n_Skipped: this bundle was assembled with `--no-agent`, so nothing \
             read the prose against the source. Section 2 is still authoritative._\n\n## 4. \
             Sources\n\n_Skipped: `--no-agent`._\n",
        );
    }
    out
}

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

The bundle already contains the page as it ships (section 1) and the checks azul-doc could
settle without a model (section 2). You fill in sections 3 and 4, in place, with Edit.

## The tree you are working against

Commit `{sha}`. Read the source with Read and Grep at this commit; do not reason from memory
about what azul's API looks like, and do not trust the page you are checking — it is the thing
under test.

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
- Replace the two `_Not filled in yet._` placeholders. Leave sections 1 and 2 exactly as they are:
  section 1 is the input the writer diffs against, section 2 is machine output you cannot improve.
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
    let link_problems = lint_links::check_guide_links(project_root);

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

    let mut written = 0usize;
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
        // page most likely to be wrong — with an empty Frontmatter standing in.
        let (fm, body) = parse_frontmatter(&raw_page)
            .unwrap_or_else(|| (serde_yaml::from_str("{}").unwrap(), raw_page.clone()));

        let checks = machine_checks(
            project_root,
            &page_path,
            &fm,
            &body,
            &api_names,
            &link_problems,
        );
        let bundle = render_bundle(
            guide,
            &page_rel,
            &raw_page,
            &fm,
            &head,
            &checks,
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
        written += 1;
    }

    println!(
        "wrote {} bundle(s) to {} ({} filtered out)",
        written,
        bdir.display(),
        skipped
    );

    if cfg.no_agent {
        println!("--no-agent: sections 3 and 4 were left unfilled.");
        return Ok(());
    }
    if cfg.dry_run {
        println!("--dry-run: prompts written, no agents dispatched.");
        println!("  prompts: {}", pdir.display());
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

    println!("\nBundles ready: {}", bdir.display());
    println!("Hand one to the writing model; it needs no tools and no repo access.");
    Ok(())
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
