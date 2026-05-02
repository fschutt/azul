//! Autodoc — parallel system-level documentation generation.
//!
//! Reads `doc/autodoc-groups.toml`, dispatches one agent per group, each
//! agent writes one or more guide pages with YAML frontmatter declaring
//! the source files it covers and the git revision at generation time.
//!
//! The companion `autodoc-check` subcommand walks generated pages and
//! reports which ones have stale source files (using `git log`).

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::reftest::autoreview::AutoreviewConfig;
use crate::spec::executor::{self, AgentResult, SHUTDOWN_REQUESTED};

// ── Manifest types ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub meta: Meta,
    #[serde(rename = "group", default)]
    pub groups: Vec<Group>,
}

#[derive(Debug, Deserialize)]
pub struct Meta {
    #[serde(default)]
    pub generated_from: String,
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub existing_guides: Vec<String>,
    #[serde(default)]
    pub shared_context_files: Vec<String>,
    #[serde(default, rename = "trees")]
    pub trees: Vec<TreeDef>,
    #[serde(default)]
    pub writing_style: Option<WritingStyle>,
    #[serde(default)]
    pub agent_thinking: Option<AgentThinking>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TreeDef {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub audience: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WritingStyle {
    #[serde(default)]
    pub reference: String,
    #[serde(default)]
    pub reader_model: String,
    #[serde(default)]
    pub voice: String,
    #[serde(default)]
    pub tone: String,
    #[serde(default)]
    pub length_target: String,
    #[serde(default)]
    pub opening_pattern: String,
    #[serde(default)]
    pub code_examples: String,
    #[serde(default)]
    pub visuals: String,
    #[serde(default)]
    pub cross_links: String,
    #[serde(default)]
    pub avoid: Vec<String>,
    #[serde(default)]
    pub prefer: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentThinking {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub instructions: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Group {
    pub id: String,
    #[serde(default)]
    pub tree: String,
    pub audience: Vec<String>,
    pub agent_strategy: String,
    #[serde(default)]
    pub tracked_files: Vec<String>,
    #[serde(default)]
    pub tracked_globs: Vec<String>,
    /// Design docs in `scripts/` (e.g. `TEXT_INPUT_ARCHITECTURE_V4.md`).
    /// These are *intent*, not authoritative source. The agent must
    /// verify against `tracked_files` (the real code) and document
    /// divergences explicitly.
    #[serde(default)]
    pub design_docs: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub outputs: Vec<Output>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Output {
    pub slug: String,
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub audience: Option<String>,
    #[serde(default = "default_maturity")]
    pub maturity: String,
    #[serde(default)]
    pub guide_order: Option<i32>,
    #[serde(default)]
    pub topic_only: bool,
    #[serde(default)]
    pub prerequisites: Vec<String>,
}

fn default_maturity() -> String {
    "mature".to_string()
}

// ── Loader ─────────────────────────────────────────────────────────────

pub fn manifest_path(project_root: &Path) -> PathBuf {
    project_root.join("doc/autodoc-groups.toml")
}

pub fn load_manifest(project_root: &Path) -> Result<Manifest, String> {
    let p = manifest_path(project_root);
    let text = fs::read_to_string(&p)
        .map_err(|e| format!("read {}: {}", p.display(), e))?;
    let m: Manifest = toml::from_str(&text)
        .map_err(|e| format!("parse {}: {}", p.display(), e))?;
    validate_manifest(&m)?;
    Ok(m)
}

fn validate_manifest(m: &Manifest) -> Result<(), String> {
    use std::collections::HashSet;
    let mut seen_paths = HashSet::new();
    let mut seen_slugs = HashSet::new();
    for g in &m.groups {
        if g.outputs.is_empty() {
            return Err(format!("group `{}` has no outputs", g.id));
        }
        for o in &g.outputs {
            if !seen_paths.insert(o.path.clone()) {
                return Err(format!("duplicate output path: {}", o.path));
            }
            if !seen_slugs.insert(o.slug.clone()) {
                return Err(format!("duplicate slug: {}", o.slug));
            }
        }
    }
    Ok(())
}

// ── Output directory layout ────────────────────────────────────────────

pub fn autodoc_dir(project_root: &Path) -> PathBuf {
    project_root.join("doc/target/autoreview/autodoc")
}

pub fn prompts_dir(project_root: &Path) -> PathBuf {
    autodoc_dir(project_root).join("prompts")
}

pub fn outdated_report_path(project_root: &Path) -> PathBuf {
    autodoc_dir(project_root).join("outdated.md")
}

// ── Tracked-file resolution ────────────────────────────────────────────

/// Resolve a group's tracked_files + tracked_globs to a sorted, deduped
/// list of paths relative to `project_root`.
pub fn resolve_tracked(project_root: &Path, group: &Group) -> Vec<PathBuf> {
    use std::collections::BTreeSet;
    let mut out: BTreeSet<PathBuf> = BTreeSet::new();
    for f in &group.tracked_files {
        let p = PathBuf::from(f);
        if project_root.join(&p).exists() {
            out.insert(p);
        }
    }
    for glob in &group.tracked_globs {
        for p in expand_glob(project_root, glob) {
            out.insert(p);
        }
    }
    out.into_iter().collect()
}

/// Minimal glob matcher: supports prefix/**/*.ext and prefix/*.ext.
/// Walks the tree under the longest static prefix, then matches each
/// found file against the suffix pattern.
pub fn expand_glob(project_root: &Path, glob: &str) -> Vec<PathBuf> {
    let (static_prefix, pattern) = split_glob(glob);
    let walk_root = project_root.join(&static_prefix);
    if !walk_root.exists() {
        return Vec::new();
    }
    let recursive = pattern.contains("**");
    let mut out = Vec::new();
    walk_collect(&walk_root, &walk_root, recursive, &pattern, &mut out, project_root);
    out
}

fn split_glob(glob: &str) -> (String, String) {
    // Find the first segment that contains a wildcard.
    let mut prefix = Vec::new();
    let mut rest = Vec::new();
    let mut found = false;
    for seg in glob.split('/') {
        if !found && (seg.contains('*') || seg.contains('?')) {
            found = true;
        }
        if found {
            rest.push(seg);
        } else {
            prefix.push(seg);
        }
    }
    (prefix.join("/"), rest.join("/"))
}

fn walk_collect(
    root: &Path,
    cur: &Path,
    recursive: bool,
    pattern: &str,
    out: &mut Vec<PathBuf>,
    project_root: &Path,
) {
    let entries = match fs::read_dir(cur) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name.starts_with('.') || name == "target" {
                continue;
            }
            if recursive {
                walk_collect(root, &path, recursive, pattern, out, project_root);
            }
        } else if let Ok(rel_to_root) = path.strip_prefix(root) {
            if simple_match(pattern, &rel_to_root.to_string_lossy()) {
                if let Ok(rel_to_proj) = path.strip_prefix(project_root) {
                    out.push(rel_to_proj.to_path_buf());
                }
            }
        }
    }
}

/// Match a path against a glob pattern of the form `**/*.ext` or `*.ext`.
fn simple_match(pattern: &str, path: &str) -> bool {
    if pattern == "**" || pattern == "**/*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("**/") {
        return simple_match(suffix, path)
            || path
                .split('/')
                .any(|_| simple_match(suffix, path.split_once('/').map(|x| x.1).unwrap_or(path)))
            || path.contains('/')
                && simple_match(pattern, path.split_once('/').map(|x| x.1).unwrap_or(path));
    }
    if let Some(ext) = pattern.strip_prefix("*.") {
        return path.rsplit_once('.').map(|x| x.1) == Some(ext)
            && !path.contains('/');
    }
    if !pattern.contains('*') {
        return pattern == path;
    }
    // Single * wildcard.
    if let Some(idx) = pattern.find('*') {
        let (pre, post) = pattern.split_at(idx);
        let post = &post[1..];
        return path.starts_with(pre) && path.ends_with(post);
    }
    pattern == path
}

// ── Relevant-report selection ──────────────────────────────────────────

/// For a group, find report files in `doc/target/autoreview/reports/` that
/// correspond to the group's tracked source files.
pub fn relevant_reports(project_root: &Path, group: &Group) -> Vec<PathBuf> {
    let reports_dir = project_root.join("doc/target/autoreview/reports");
    if !reports_dir.is_dir() {
        return Vec::new();
    }
    let tracked = resolve_tracked(project_root, group);
    let mut out = Vec::new();
    for src in &tracked {
        // core/src/dom.rs → core__src__dom.md
        let s = src.to_string_lossy();
        let stem = s.strip_suffix(".rs").unwrap_or(&s);
        let safe = stem.replace('/', "__");
        let report = reports_dir.join(format!("{}.md", safe));
        if report.exists() {
            out.push(report);
        }
    }
    out
}

// ── Prompt builder ─────────────────────────────────────────────────────

const SOURCE_INCLUSION_BUDGET: usize = 40_000; // chars; agent reads larger files itself
const REPORT_INCLUSION_BUDGET: usize = 30_000;

pub fn build_autodoc_prompt(
    project_root: &Path,
    manifest: &Manifest,
    group: &Group,
) -> String {
    let tracked = resolve_tracked(project_root, group);
    let reports = relevant_reports(project_root, group);

    let mut s = String::new();
    s.push_str(&format!("# Autodoc: `{}`\n\n", group.id));
    s.push_str(&format!(
        "You are writing system-level documentation for the `{}` system.\n\n",
        group.id
    ));
    s.push_str("**You are running in parallel with other agents writing other systems. \
                You must ONLY write the output files listed below. Do NOT modify any other file.**\n\n");

    // ── Tree context ──────────────────────────────────────────────
    if !group.tree.is_empty() {
        if let Some(td) = manifest.meta.trees.iter().find(|t| t.id == group.tree) {
            s.push_str(&format!(
                "## Book tree: `{}` — {}\n\n{}\n\n",
                td.id, td.title, td.description.trim()
            ));
        } else {
            s.push_str(&format!("## Book tree: `{}`\n\n", group.tree));
        }
    }

    s.push_str(&format!("## Strategy: `{}`\n\n", group.agent_strategy));
    s.push_str(&format!("Audience(s): {}\n\n", group.audience.join(", ")));
    if let Some(notes) = &group.notes {
        s.push_str(&format!("Notes: {}\n\n", notes));
    }

    // ── Shared context (read first) ───────────────────────────────
    if !manifest.meta.shared_context_files.is_empty() {
        s.push_str("## Shared context — READ THESE FIRST\n\n");
        s.push_str("Before writing any output, load these files. They anchor your work \
                    in the project's existing structure and conventions:\n\n");
        for f in &manifest.meta.shared_context_files {
            s.push_str(&format!("- `{f}`\n"));
        }
        s.push('\n');
    }

    // ── Writing style ─────────────────────────────────────────────
    if let Some(ws) = &manifest.meta.writing_style {
        s.push_str("## Writing style\n\n");
        if !ws.reference.is_empty() {
            s.push_str(&format!("**Reference style**: {}\n\n", ws.reference));
        }
        if !ws.reader_model.is_empty() {
            s.push_str(&format!("**Reader**: {}\n\n", ws.reader_model));
        }
        if !ws.voice.is_empty() {
            s.push_str(&format!("- **Voice**: {}\n", ws.voice));
        }
        if !ws.tone.is_empty() {
            s.push_str(&format!("- **Tone**: {}\n", ws.tone));
        }
        if !ws.length_target.is_empty() {
            s.push_str(&format!("- **Length**: {}\n", ws.length_target));
        }
        if !ws.opening_pattern.is_empty() {
            s.push_str(&format!("- **Opening**: {}\n", ws.opening_pattern));
        }
        if !ws.code_examples.is_empty() {
            s.push_str(&format!("- **Code**: {}\n", ws.code_examples));
        }
        if !ws.visuals.is_empty() {
            s.push_str(&format!("- **Visuals**: {}\n", ws.visuals));
        }
        if !ws.cross_links.is_empty() {
            s.push_str(&format!("- **Links**: {}\n", ws.cross_links));
        }
        if !ws.avoid.is_empty() {
            s.push_str("\n**Avoid**:\n");
            for a in &ws.avoid {
                s.push_str(&format!("- {a}\n"));
            }
        }
        if !ws.prefer.is_empty() {
            s.push_str("\n**Prefer**:\n");
            for p in &ws.prefer {
                s.push_str(&format!("- {p}\n"));
            }
        }
        s.push('\n');
    }

    // ── Max-effort thinking ──────────────────────────────────────
    if let Some(at) = &manifest.meta.agent_thinking {
        let mode = if at.mode.is_empty() { "max-effort" } else { &at.mode };
        s.push_str(&format!("## Thinking mode: `{mode}`\n\n"));
        s.push_str("Use extended thinking. Do not skim — actually load and reason about \
                    the source. Quality over speed.\n\n");
        if !at.instructions.is_empty() {
            for (i, inst) in at.instructions.iter().enumerate() {
                s.push_str(&format!("{}. {inst}\n", i + 1));
            }
            s.push('\n');
        }
    }

    s.push_str("## Output files (write each one)\n\n");
    for o in &group.outputs {
        s.push_str(&format!("- `{}` — *{}*", o.path, o.title));
        let mut tags = Vec::new();
        if let Some(a) = &o.audience { tags.push(format!("audience={}", a)); }
        tags.push(format!("maturity={}", o.maturity));
        if let Some(n) = o.guide_order { tags.push(format!("guide_order={}", n)); }
        if o.topic_only { tags.push("topic_only".to_string()); }
        if !tags.is_empty() {
            s.push_str(&format!(" ({})", tags.join(", ")));
        }
        s.push('\n');
        if !o.prerequisites.is_empty() {
            s.push_str(&format!(
                "  - Reader has read: {}\n",
                o.prerequisites.join(", ")
            ));
        }
    }
    s.push('\n');

    s.push_str("## Required frontmatter\n\n");
    s.push_str("Each output file MUST start with YAML frontmatter:\n\n");
    s.push_str("```yaml\n---\n\
                slug: <slug>            # URL slug; for English == canonical_slug\n\
                title: <title>\n\
                language: en             # canonical pages are always English\n\
                canonical_slug: <slug>   # same as `slug` for English pages\n\
                audience: <external | contributor>\n\
                maturity: <mature | wip | stub | draft>\n\
                guide_order: <int or null>\n\
                topic_only: <bool>\n\
                prerequisites: [<canonical-slug>, ...]\n\
                tracked_files:\n  - <path>\n  - ...\n\
                last_generated_rev: <git-sha-here>\n\
                generated_at: <iso8601>\n\
                ---\n```\n\n");
    s.push_str(&format!(
        "Use git rev `{}` and the current ISO timestamp. Tracked files come from the manifest \
         (listed below). `language: en` and `canonical_slug == slug` because this run \
         generates canonical English pages. Do NOT emit `source_rev` or `source_hash` — \
         those fields belong to translations only and are written by the future `translate` \
         subcommand.\n\n",
        head_sha(project_root).unwrap_or_else(|_| "UNKNOWN".to_string())
    ));

    s.push_str("## Tracked source files (TRUTH)\n\n");
    if tracked.is_empty() {
        s.push_str("(none — agent must read source files directly with the Read tool)\n\n");
    } else {
        for p in &tracked {
            s.push_str(&format!("- `{}`\n", p.display()));
        }
        s.push('\n');
    }

    // ── Design docs (INTENT, may be outdated) ────────────────────
    if !group.design_docs.is_empty() {
        s.push_str("## Design docs (INTENT — read after the source)\n\n");
        s.push_str("These files in `scripts/` are *design intent*. They were written \
                    when the system was being planned and may now disagree with the \
                    code. Read them to understand the **why** and the original mental \
                    model — then verify everything against the tracked source files \
                    above. **The code is truth, the design docs are context.**\n\n");
        s.push_str("If the doc and the code disagree, document what the code does. \
                    If a divergence is significant (e.g. a planned approach was \
                    abandoned), add a one-line note like *\"The original design \
                    proposed X; the implementation took approach Y because [reason \
                    visible in commit history or comments].\"*\n\n");
        for d in &group.design_docs {
            s.push_str(&format!("- `scripts/{d}`\n"));
        }
        s.push('\n');

        // Embed the design docs (subject to the same source budget — they
        // count against `SOURCE_INCLUSION_BUDGET` so we don't double-count.)
        s.push_str("### Embedded design docs\n\n");
        let mut design_budget = SOURCE_INCLUSION_BUDGET / 2;
        let mut embedded = 0usize;
        for d in &group.design_docs {
            let abs = project_root.join("scripts").join(d);
            let content = match fs::read_to_string(&abs) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if content.len() > design_budget {
                continue;
            }
            design_budget -= content.len();
            embedded += 1;
            s.push_str(&format!("\n#### `scripts/{d}`\n\n```markdown\n{content}\n```\n"));
        }
        if embedded == 0 {
            s.push_str("(All design docs exceeded the inline budget. Read with the Read tool.)\n\n");
        }
    }

    s.push_str("### Embedded source (small files only)\n\n");
    let mut budget = SOURCE_INCLUSION_BUDGET;
    let mut included = 0usize;
    for p in &tracked {
        let abs = project_root.join(p);
        let content = match fs::read_to_string(&abs) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if content.len() > budget {
            continue;
        }
        budget -= content.len();
        included += 1;
        s.push_str(&format!("\n#### `{}`\n\n```rust\n{}\n```\n", p.display(), content));
    }
    if included == 0 {
        s.push_str("(All tracked files exceed the inline budget. Use the Read tool.)\n\n");
    }

    s.push_str("## Existing review notes\n\n");
    let mut rep_budget = REPORT_INCLUSION_BUDGET;
    let mut rep_included = 0usize;
    for r in &reports {
        let content = match fs::read_to_string(r) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if content.len() > rep_budget {
            continue;
        }
        rep_budget -= content.len();
        rep_included += 1;
        s.push_str(&format!(
            "\n#### `{}`\n\n```markdown\n{}\n```\n",
            r.display(),
            content
        ));
    }
    if rep_included == 0 && !reports.is_empty() {
        s.push_str("(Reports exceeded budget. Read them yourself if useful.)\n\n");
    }

    s.push_str("## Authoring rules\n\n");
    s.push_str(authoring_rules());

    s
}

fn authoring_rules() -> &'static str {
    "1. Write each output file using the Write tool. Include the YAML frontmatter \
     described above.\n\
     2. **External audience pages** must NOT use any concept that isn't in the \
        `prerequisites` list. If you must reference a later concept, link forward \
        with `(covered later in <slug>)` and keep the discussion brief.\n\
     3. **Contributor pages** may freely cross-reference other internals pages \
        and link to source code paths.\n\
     4. **Maturity tag**:\n\
        - `mature`: examples must compile and run; use ` ```rust ` blocks.\n\
        - `wip`: examples should compile; use ` ```rust ` blocks but it's OK if \
          some need `,no_run`. Add a one-line WIP notice at the top of the page.\n\
        - `stub`: the runtime isn't wired up. Document the *intent*. Use \
          ` ```rust,ignore ` for all code blocks. Add a loud notice that the \
          system is not yet functional.\n\
        - `draft`: only set this if you bail out partway through. Mark explicitly.\n\
     5. **Code samples**: prefer concrete, copy-pasteable examples over prose. \
        Hidden setup lines (`# use azul::*;`) are encouraged.\n\
     6. **Visual examples**: embed a fenced block with the `azul-render` \
        language tag. The body is XHTML (NOT a Rust snippet) — the same dialect \
        the reftest harness consumes. The release pipeline renders it via \
        HeadlessWindow and saves a PNG. **Do not** add a separate markdown \
        image link — the HTML preprocessor expands the fence into a \
        `<figure>` automatically.\n\
        \n\
        Single screenshot:\n\
        ```\n\
        ```azul-render screenshot=hello-world width=400 height=200 subtitle=\"The classic output\"\n\
        <body><p style=\"font-size: 24px; padding: 20px;\">Hello, world!</p></body>\n\
        ```\n\
        ```\n\
        \n\
        Sequence (slideshow): give multiple consecutive blocks the same \
        `slideshow=ID`. They are grouped into one slideshow widget in source \
        order, each with its own `subtitle`. Use this to show \
        before/after/animation steps:\n\
        ```\n\
        ```azul-render screenshot=scroll-1 slideshow=scroll-demo subtitle=\"Initial state — scroll position 0\"\n\
        <body>...</body>\n\
        ```\n\
        \n\
        ```azul-render screenshot=scroll-2 slideshow=scroll-demo subtitle=\"After scrolling 100px\"\n\
        <body>...</body>\n\
        ```\n\
        \n\
        ```azul-render screenshot=scroll-3 slideshow=scroll-demo subtitle=\"At the bottom\"\n\
        <body>...</body>\n\
        ```\n\
        ```\n\
        \n\
        Attribute reference: `screenshot=` (required, unique PNG name), \
        `width=`/`height=` (optional, default 800x600), `subtitle=\"...\"` \
        (optional caption — quote it if it contains spaces), \
        `slideshow=ID` (optional, groups frames).\n\
     7. **Length**: aim for 200–800 lines of markdown per page. Internals pages \
        may be longer if the system is complex.\n\
     8. **Do not** create new directories you weren't asked to. Output paths in \
        the manifest are absolute (relative to project root).\n\
     9. **No external link assumptions**: don't reference azul.rs URLs that may \
        not exist. Cross-link to other guide files using relative paths.\n\
    10. After writing all output files, output the literal token \
        `AUTODOC_DONE` and stop.\n"
}

// ── Frontmatter ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Frontmatter {
    /// URL slug — *localized*. For English: same as `canonical_slug`. For
    /// translations: localized form (e.g. `architektur`), used in the URL
    /// `/<lang>/<slug>`.
    pub slug: String,
    pub title: String,
    /// Language code (e.g. "en", "de"). Required.
    #[serde(default = "default_language")]
    pub language: String,
    /// English-side identity. Same as `slug` for English pages; for
    /// translations, the canonical English slug this page mirrors.
    /// Used by the website to find sibling translations and by the
    /// staleness checker to look up the canonical source file.
    #[serde(default)]
    pub canonical_slug: Option<String>,

    #[serde(default)]
    pub audience: Option<String>,
    pub maturity: String,
    #[serde(default)]
    pub guide_order: Option<i32>,
    #[serde(default)]
    pub topic_only: bool,
    /// One-line summary of *this* page (not its first sentence). Localised
    /// per language. Rendered indented under the page title in the guide
    /// index. Aim for "section summary" granularity, not topic.
    #[serde(default)]
    pub short_desc: Option<String>,
    /// Prerequisite *canonical* slugs (English). The website resolves
    /// these to localized URLs at render time so a German page reading
    /// `prerequisites: [hello-world]` links to the German "hallo-welt"
    /// page.
    #[serde(default)]
    pub prerequisites: Vec<String>,

    /// Source files this guide documents. Only meaningful on canonical
    /// English pages — translations leave this empty and inherit
    /// staleness from their canonical source.
    #[serde(default)]
    pub tracked_files: Vec<String>,
    /// Git SHA at which the page was last regenerated. Required for
    /// canonical English pages.
    #[serde(default)]
    pub last_generated_rev: Option<String>,

    /// For translations only: the git SHA of the canonical English file
    /// at the time this translation was produced. The staleness checker
    /// reports this translation as stale if the canonical file has
    /// commits after `source_rev`.
    #[serde(default)]
    pub source_rev: Option<String>,

    /// For translations only: SHA-256 of the canonical English file's
    /// content (after the frontmatter, body bytes only) at the time of
    /// translation. The release pipeline hashes the current canonical
    /// body and compares — mismatch = translation is out of date and
    /// the build fails.
    #[serde(default)]
    pub source_hash: Option<String>,

    #[serde(default)]
    pub generated_at: String,
}

fn default_language() -> String {
    "en".to_string()
}

impl Frontmatter {
    pub fn is_canonical_english(&self) -> bool {
        self.language == "en"
    }
    pub fn effective_canonical_slug(&self) -> &str {
        self.canonical_slug.as_deref().unwrap_or(&self.slug)
    }
}

/// SHA-256 of a canonical page's body (post-frontmatter content). Used
/// to detect translation drift: each translation records the canonical
/// hash at translation time; the release pipeline recomputes the hash
/// from the live English file and refuses to ship if it has changed.
pub fn hash_body(body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.as_bytes());
    let result = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in result {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", b);
    }
    hex
}

/// Read a canonical English file and return the SHA-256 of its body.
/// Returns None if the file is missing or has no frontmatter.
pub fn read_canonical_hash(
    project_root: &Path,
    canonical_slug: &str,
) -> Option<String> {
    let path = project_root.join(format!("doc/guide/en/{}.md", canonical_slug));
    let content = fs::read_to_string(&path).ok()?;
    let (_fm, body) = parse_frontmatter(&content)?;
    Some(hash_body(&body))
}

/// Extract YAML frontmatter and return (parsed, body).
pub fn parse_frontmatter(content: &str) -> Option<(Frontmatter, String)> {
    let stripped = content.strip_prefix("---\n")?;
    let end = stripped.find("\n---\n")?;
    let yaml = &stripped[..end];
    let body = stripped[end + 5..].to_string();
    let fm: Frontmatter = serde_yaml::from_str(yaml).ok()?;
    Some((fm, body))
}

// ── Git helpers ────────────────────────────────────────────────────────

pub fn head_sha(project_root: &Path) -> Result<String, String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git rev-parse: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// List commits that touched `file` since `since_sha`. Empty if file is
/// fresh enough.
pub fn commits_since(
    project_root: &Path,
    since_sha: &str,
    file: &str,
) -> Result<Vec<String>, String> {
    let range = format!("{}..HEAD", since_sha);
    let out = Command::new("git")
        .args(["log", &range, "--format=%H", "--", file])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git log: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git log failed for {}: {}",
            file,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

// ── Subcommand: autodoc ───────────────────────────────────────────────

pub fn run_autodoc(config: &AutoreviewConfig) -> Result<(), String> {
    let project_root = config.project_root.clone();
    let manifest = load_manifest(&project_root)?;

    println!(
        "Loaded manifest: {} groups, {} outputs",
        manifest.groups.len(),
        manifest.groups.iter().map(|g| g.outputs.len()).sum::<usize>()
    );

    let pdir = prompts_dir(&project_root);
    fs::create_dir_all(&pdir)
        .map_err(|e| format!("create {}: {}", pdir.display(), e))?;

    // Phase 1: generate prompts
    println!("\n=== Phase 1: Generating autodoc prompts ===\n");
    let mut prompt_count = 0;
    let mut filtered = 0;
    for group in &manifest.groups {
        if let Some(filter) = &config.file_filter {
            if !group.id.contains(filter.as_str()) {
                filtered += 1;
                continue;
            }
        }
        let prompt_path = pdir.join(format!("{}.md", group.id));
        let status = executor::classify_prompt(&prompt_path, config.retry_failed);
        if matches!(
            status,
            executor::PromptStatus::Done | executor::PromptStatus::Taken { .. }
        ) {
            continue;
        }
        let prompt = build_autodoc_prompt(&project_root, &manifest, group);
        fs::write(&prompt_path, &prompt)
            .map_err(|e| format!("write {}: {}", prompt_path.display(), e))?;
        prompt_count += 1;
    }
    println!(
        "Generated {} prompts ({} filtered out)",
        prompt_count, filtered
    );

    if config.dry_run {
        println!("\n--dry-run: stopping after prompt generation.");
        return Ok(());
    }

    // Phase 2: dispatch agents
    println!("\n=== Phase 2: Dispatching autodoc agents ===\n");
    dispatch_autodoc_agents(config)?;

    Ok(())
}

fn dispatch_autodoc_agents(config: &AutoreviewConfig) -> Result<(), String> {
    let project_root = &config.project_root;
    let prompts = prompts_dir(project_root);

    let (pending, done, failed, taken) =
        executor::scan_prompts_dir(&prompts, config.retry_failed);

    println!(
        "Prompt status: {} pending, {} done, {} failed, {} in-progress",
        pending.len(),
        done,
        failed,
        taken
    );

    if pending.is_empty() {
        println!("No pending prompts.");
        return Ok(());
    }

    let agent_count = config.agents.min(pending.len());
    println!("Launching {} concurrent autodoc agents...\n", agent_count);

    executor::install_sigint_handler();

    let work_queue: Arc<Mutex<VecDeque<PathBuf>>> =
        Arc::new(Mutex::new(pending.into_iter().collect()));

    let spinner = nanospinner::MultiSpinner::new().start();
    let slot_spinners: Vec<_> = (0..agent_count)
        .map(|i| spinner.add(format!("[DOC {:02}] idle", i)))
        .collect();

    let mut handles = Vec::with_capacity(agent_count);

    for (slot_idx, line) in slot_spinners.into_iter().enumerate() {
        let work_queue = Arc::clone(&work_queue);
        let timeout = config.timeout;
        let model = config.model.clone();
        let project_root = project_root.clone();

        let handle = std::thread::spawn(move || {
            loop {
                if SHUTDOWN_REQUESTED.load(Ordering::Relaxed) {
                    break;
                }
                let prompt_path = {
                    let mut q = work_queue.lock().unwrap();
                    q.pop_front()
                };
                let prompt_path = match prompt_path {
                    Some(p) => p,
                    None => break,
                };
                let name = prompt_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                line.update(format!("[DOC {:02}] {}", slot_idx, name));
                let result = run_autodoc_agent(
                    &project_root,
                    slot_idx,
                    &prompt_path,
                    timeout,
                    model.as_deref(),
                    &|status| {
                        line.update(format!("[DOC {:02}] {} | {}", slot_idx, name, status));
                    },
                );
                let msg = if result.success {
                    format!("{}: done", name)
                } else {
                    format!(
                        "{}: FAILED ({})",
                        name,
                        result.error.as_deref().unwrap_or("unknown")
                    )
                };
                line.update(format!("[DOC {:02}] {}", slot_idx, msg));
            }
            line.update(format!("[DOC {:02}] finished", slot_idx));
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.join();
    }

    Ok(())
}

fn run_autodoc_agent(
    project_root: &Path,
    slot_index: usize,
    prompt_path: &Path,
    timeout: Duration,
    model: Option<&str>,
    on_progress: &dyn Fn(&str),
) -> AgentResult {
    let taken_path = prompt_path.with_extension("md.taken");
    let result_path = prompt_path.with_extension("md.result");
    let done_path = prompt_path.with_extension("md.done");
    let failed_path = prompt_path.with_extension("md.failed");

    let prompt_text = match fs::read_to_string(prompt_path) {
        Ok(c) => c,
        Err(e) => {
            return AgentResult {
                success: false,
                patches: 0,
                error: Some(format!("read prompt: {}", e)),
            }
        }
    };

    let result_file = match fs::File::create(&result_path) {
        Ok(f) => f,
        Err(e) => {
            return AgentResult {
                success: false,
                patches: 0,
                error: Some(format!("create result: {}", e)),
            }
        }
    };
    let result_file_err = match result_file.try_clone() {
        Ok(f) => f,
        Err(e) => {
            return AgentResult {
                success: false,
                patches: 0,
                error: Some(format!("clone fd: {}", e)),
            }
        }
    };

    let mut cmd_args: Vec<&str> = vec![
        "-p",
        "--dangerously-skip-permissions",
        "--verbose",
        "--output-format",
        "stream-json",
        "--disallowedTools",
        "mcp__*",
        "--disallowedTools",
        "rust-analyzer-lsp",
    ];
    if let Some(m) = model {
        cmd_args.push("--model");
        cmd_args.push(m);
    }

    let mut child = match Command::new("claude")
        .args(&cmd_args)
        .env_remove("CLAUDECODE")
        .current_dir(project_root)
        .stdin(Stdio::piped())
        .stdout(result_file)
        .stderr(result_file_err)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return AgentResult {
                success: false,
                patches: 0,
                error: Some(format!("spawn claude: {}", e)),
            }
        }
    };

    let pid = child.id();
    if let Err(e) = executor::write_taken_file(&taken_path, slot_index, pid) {
        let _ = child.kill();
        return AgentResult {
            success: false,
            patches: 0,
            error: Some(e),
        };
    }
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt_text.as_bytes());
    }

    let progress_path = prompt_path.with_extension("md.progress");
    let start = Instant::now();
    let exit_status = loop {
        if SHUTDOWN_REQUESTED.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(&taken_path);
            let _ = fs::remove_file(&progress_path);
            return AgentResult {
                success: false,
                patches: 0,
                error: Some("shutdown".into()),
            };
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = fs::remove_file(&taken_path);
                    let _ = fs::remove_file(&progress_path);
                    let partial = fs::read_to_string(&result_path).unwrap_or_default();
                    let _ = fs::write(
                        &failed_path,
                        format!(
                            "Agent timed out after {}s\nslot={}\n\n--- PARTIAL OUTPUT ---\n{}",
                            timeout.as_secs(),
                            slot_index,
                            partial,
                        ),
                    );
                    return AgentResult {
                        success: false,
                        patches: 0,
                        error: Some("timeout".into()),
                    };
                }
                let elapsed = start.elapsed().as_secs();
                let activity = executor::read_stream_json_activity(&result_path);
                let status_line = format!(
                    "{}:{:02} | {}",
                    elapsed / 60,
                    elapsed % 60,
                    if activity.is_empty() {
                        "working..."
                    } else {
                        &activity
                    },
                );
                on_progress(&status_line);
                let _ = fs::write(&progress_path, &status_line);
                std::thread::sleep(Duration::from_secs(2));
            }
            Err(e) => {
                let _ = fs::remove_file(&taken_path);
                let _ = fs::remove_file(&progress_path);
                let _ = fs::write(&failed_path, format!("wait: {}\n", e));
                return AgentResult {
                    success: false,
                    patches: 0,
                    error: Some(format!("wait: {}", e)),
                };
            }
        }
    };

    let _ = fs::remove_file(&progress_path);
    let _ = fs::remove_file(&taken_path);
    let result_content = executor::extract_result_text(&result_path);
    let elapsed = start.elapsed();

    if !exit_status.success() && result_content.trim().is_empty() {
        let code = exit_status.code().unwrap_or(-1);
        let _ = fs::write(
            &failed_path,
            format!(
                "Agent exited with code {}\nelapsed_secs={}\nslot={}\n\n--- AGENT OUTPUT ---\n{}",
                code,
                elapsed.as_secs(),
                slot_index,
                result_content,
            ),
        );
        return AgentResult {
            success: false,
            patches: 0,
            error: Some(format!("exit {}", code)),
        };
    }

    let _ = fs::write(
        &done_path,
        format!(
            "action=AUTODOC\nslot={}\nelapsed_secs={}\n\n--- AGENT OUTPUT ---\n{}",
            slot_index,
            elapsed.as_secs(),
            result_content,
        ),
    );

    AgentResult {
        success: true,
        patches: 0,
        error: None,
    }
}

// ── Subcommand: autodoc-check (outdated detection) ────────────────────

pub fn run_autodoc_check(config: &AutoreviewConfig) -> Result<(), String> {
    let project_root = &config.project_root;
    let guide_dir = project_root.join("doc/guide");
    if !guide_dir.is_dir() {
        return Err(format!("missing {}", guide_dir.display()));
    }

    let mut pages: Vec<PathBuf> = Vec::new();
    walk_md(&guide_dir, &mut pages);
    pages.sort();

    let mut stale_pages: Vec<(PathBuf, Vec<StaleFile>)> = Vec::new();
    let mut no_frontmatter: Vec<PathBuf> = Vec::new();
    let mut fresh = 0usize;

    let mut translation_stale: Vec<(PathBuf, String, Vec<String>)> = Vec::new();

    for page in &pages {
        let content = match fs::read_to_string(page) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let (fm, _body) = match parse_frontmatter(&content) {
            Some(x) => x,
            None => {
                no_frontmatter.push(page.clone());
                continue;
            }
        };

        // Translation page: check against canonical English source.
        if fm.language != "en" {
            let canonical_slug = fm.effective_canonical_slug().to_string();
            let canonical_path = format!("doc/guide/en/{}.md", canonical_slug);
            let mut reasons: Vec<String> = Vec::new();

            // 1) Content hash check — the authoritative "did the body change" signal.
            match (&fm.source_hash, read_canonical_hash(project_root, &canonical_slug)) {
                (Some(recorded), Some(current)) if recorded == &current => {
                    // body matches — no drift
                }
                (Some(recorded), Some(current)) => {
                    reasons.push(format!(
                        "body hash mismatch (recorded `{}`, current `{}`)",
                        &recorded[..12.min(recorded.len())],
                        &current[..12.min(current.len())],
                    ));
                }
                (None, _) => {
                    reasons.push("(no source_hash recorded — re-translate)".to_string());
                }
                (Some(_), None) => {
                    reasons.push(format!(
                        "canonical file `{}` is missing or unparsable",
                        canonical_path
                    ));
                }
            }

            // 2) Git rev check — informational; surfaces *which* commits touched
            //    the canonical file even when the hash already matches.
            if let Some(source_rev) = &fm.source_rev {
                let commits =
                    commits_since(project_root, source_rev, &canonical_path).unwrap_or_default();
                if !commits.is_empty() && reasons.is_empty() {
                    // Hash matched but git rev shows commits — either no-op
                    // commits (whitespace, frontmatter-only updates) or the
                    // body was reverted. Still report so the user can ack.
                    reasons.push(format!(
                        "{} commit(s) touched canonical since source_rev (body unchanged — \
                         consider bumping source_rev)",
                        commits.len()
                    ));
                }
            } else if reasons.is_empty() {
                reasons.push("(no source_rev recorded)".to_string());
            }

            if reasons.is_empty() {
                fresh += 1;
            } else {
                translation_stale.push((page.clone(), canonical_path, reasons));
            }
            continue;
        }

        // Canonical English page: check tracked_files against last_generated_rev.
        let last_gen = match &fm.last_generated_rev {
            Some(s) => s.clone(),
            None => {
                // Canonical page that wasn't generated by autodoc — treat
                // as fresh for now (existing hand-written pages like
                // architecture.md).
                fresh += 1;
                continue;
            }
        };
        let mut stales = Vec::new();
        for f in &fm.tracked_files {
            let commits = commits_since(project_root, &last_gen, f).unwrap_or_default();
            if !commits.is_empty() {
                stales.push(StaleFile {
                    path: f.clone(),
                    commits,
                });
            }
        }
        if stales.is_empty() {
            fresh += 1;
        } else {
            stale_pages.push((page.clone(), stales));
        }
    }

    write_outdated_report(
        project_root,
        fresh,
        &stale_pages,
        &translation_stale,
        &no_frontmatter,
    )?;
    let report = outdated_report_path(project_root);
    println!(
        "Pages: {} fresh, {} stale, {} without frontmatter, {} translation-stale",
        fresh,
        stale_pages.len(),
        no_frontmatter.len(),
        translation_stale.len(),
    );
    println!("Report: {}", report.display());
    if config.strict && (!stale_pages.is_empty() || !translation_stale.is_empty()) {
        return Err(format!(
            "autodoc-check --strict: {} stale, {} translation-stale (see {})",
            stale_pages.len(),
            translation_stale.len(),
            report.display(),
        ));
    }
    Ok(())
}

#[derive(Debug)]
pub struct StaleFile {
    pub path: String,
    pub commits: Vec<String>,
}

fn walk_md(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_md(&p, out);
        } else if p.extension().map(|e| e == "md").unwrap_or(false) {
            // Skip the working backlog; that's source material, not a page.
            if p.file_name().map(|n| n == "reference.md").unwrap_or(false) {
                continue;
            }
            out.push(p);
        }
    }
}

fn write_outdated_report(
    project_root: &Path,
    fresh: usize,
    stale: &[(PathBuf, Vec<StaleFile>)],
    translation_stale: &[(PathBuf, String, Vec<String>)],
    no_fm: &[PathBuf],
) -> Result<(), String> {
    let path = outdated_report_path(project_root);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut s = String::new();
    s.push_str("# Autodoc — outdated check\n\n");
    s.push_str(&format!(
        "- {} fresh pages\n\
         - {} stale canonical pages (tracked source files changed)\n\
         - {} stale translations (canonical English changed since translation)\n\
         - {} pages without frontmatter\n\n",
        fresh,
        stale.len(),
        translation_stale.len(),
        no_fm.len()
    ));
    if !stale.is_empty() {
        s.push_str("## Stale canonical pages\n\n");
        for (page, stales) in stale {
            let rel = page.strip_prefix(project_root).unwrap_or(page);
            s.push_str(&format!("### `{}`\n\n", rel.display()));
            for st in stales {
                s.push_str(&format!(
                    "- `{}` — {} commit(s) since generation\n",
                    st.path,
                    st.commits.len()
                ));
                for c in &st.commits {
                    s.push_str(&format!("  - `{}`\n", c));
                }
            }
            s.push('\n');
        }
    }
    if !translation_stale.is_empty() {
        s.push_str("## Stale translations\n\n");
        for (page, canonical, reasons) in translation_stale {
            let rel = page.strip_prefix(project_root).unwrap_or(page);
            s.push_str(&format!(
                "### `{}`\n- canonical: `{}`\n",
                rel.display(),
                canonical
            ));
            for r in reasons {
                s.push_str(&format!("- {}\n", r));
            }
            s.push('\n');
        }
    }
    if !no_fm.is_empty() {
        s.push_str("## Pages without frontmatter\n\n");
        for p in no_fm {
            let rel = p.strip_prefix(project_root).unwrap_or(p);
            s.push_str(&format!("- `{}`\n", rel.display()));
        }
        s.push('\n');
    }
    fs::write(&path, s).map_err(|e| format!("write report: {}", e))?;
    Ok(())
}

// ── Screenshot harness ────────────────────────────────────────────────
//
// Agents may emit ```azul-render fenced blocks like:
//
//   ```azul-render screenshot=hello-world width=400 height=200
//   <body><p style="font-size: 24px;">Hello, world!</p></body>
//   ```
//
// `autodoc-screenshots` walks every guide page, extracts these blocks,
// renders them via the headless cpurender pipeline (same one the reftest
// harness uses), and saves them as PNGs under doc/guide/screenshots/.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderBlock {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Caption text shown beneath the image. Empty if no `subtitle=` attr.
    #[serde(default)]
    pub subtitle: String,
    /// Slideshow identifier. Multiple blocks sharing the same slideshow id
    /// are grouped into one slideshow widget at HTML render time, in source
    /// order of appearance within the page.
    #[serde(default)]
    pub slideshow: Option<String>,
    /// XML/XHTML body of the block — fed to the renderer.
    #[serde(skip)]
    pub xml: String,
    /// Source markdown page (relative to project root in the manifest).
    pub source_page: PathBuf,
}

/// Quote-aware attribute parser. Accepts:
///   key=value
///   key="value with spaces"
/// Whitespace separates pairs. Bare keys (no `=`) are ignored.
pub fn parse_attrs(s: &str) -> Vec<(String, String)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        // Read key
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let key = std::str::from_utf8(&bytes[key_start..i]).unwrap_or("").to_string();
        if key.is_empty() {
            break;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            // bare key, skip
            continue;
        }
        i += 1; // consume '='
        // Read value
        let value = if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            let v_start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            let v = std::str::from_utf8(&bytes[v_start..i]).unwrap_or("").to_string();
            if i < bytes.len() {
                i += 1; // consume closing quote
            }
            v
        } else {
            let v_start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            std::str::from_utf8(&bytes[v_start..i]).unwrap_or("").to_string()
        };
        out.push((key, value));
    }
    out
}

pub fn extract_render_blocks(content: &str, page_path: &Path) -> Vec<RenderBlock> {
    let mut out = Vec::new();
    let mut lines = content.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("```azul-render") {
            continue;
        }
        let attrs_str = trimmed.trim_start_matches("```azul-render").trim();
        let attrs = parse_attrs(attrs_str);

        let mut name = None;
        let mut width = 800u32;
        let mut height = 600u32;
        let mut subtitle = String::new();
        let mut slideshow: Option<String> = None;

        for (k, v) in &attrs {
            match k.as_str() {
                "screenshot" => name = Some(v.clone()),
                "width" => width = v.parse().unwrap_or(width),
                "height" => height = v.parse().unwrap_or(height),
                "subtitle" => subtitle = v.clone(),
                "slideshow" => slideshow = Some(v.clone()),
                _ => {}
            }
        }

        let mut body = String::new();
        for inner in lines.by_ref() {
            if inner.trim_start().starts_with("```") {
                break;
            }
            body.push_str(inner);
            body.push('\n');
        }
        if let Some(n) = name {
            out.push(RenderBlock {
                name: n,
                width,
                height,
                subtitle,
                slideshow,
                xml: body,
                source_page: page_path.to_path_buf(),
            });
        }
    }
    out
}

/// Initialize a FontContext suitable for rendering arbitrary XML snippets.
/// Heavy — takes ~100–500ms depending on system font count. Call once per
/// screenshot run, not per block.
pub fn init_screenshot_font_context() -> Result<azul_layout::FontContext, String> {
    let registry = azul_layout::FcFontRegistry::new();
    let _ = registry.load_from_disk_cache();
    registry.spawn_scout_and_builders();
    let os = rust_fontconfig::OperatingSystem::current();
    let common = rust_fontconfig::config::tokenize_common_families(os);
    registry.request_fonts(&common);
    let fc_cache = registry.shared_cache();
    let mut font_context = azul_layout::FontContext::from_fc_cache(fc_cache);
    let warmup = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>x</p></body></html>";
    if let Ok(dom) = azul_layout::xml::parse_xml_to_styled_dom(warmup) {
        font_context.pre_resolve_chains_for_dom(&dom, &azul_css::system::Platform::current());
    }
    font_context.load_fonts_for_chains();
    Ok(font_context)
}

/// Render an XML/XHTML snippet to a PNG file. Wraps the snippet in a
/// minimal HTML envelope if the agent didn't supply one.
pub fn render_xml_to_png(
    font_context: &azul_layout::FontContext,
    xml: &str,
    output_path: &Path,
    width: u32,
    height: u32,
) -> Result<(), String> {
    use azul_core::dom::DomId;
    use azul_core::geom::LogicalSize;
    use azul_layout::callbacks::ExternalSystemCallbacks;
    use azul_layout::window_state::FullWindowState;

    let envelope = wrap_xml_envelope(xml);
    let styled_dom = azul_layout::xml::parse_xml_to_styled_dom(&envelope)
        .map_err(|e| format!("xml parse: {}", e))?;

    let mut layout_window = azul_layout::LayoutWindow::from_font_context(font_context)
        .map_err(|e| format!("LayoutWindow: {:?}", e))?;

    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize {
        width: width as f32,
        height: height as f32,
    };
    ws.size.dpi = 96;
    let mut rr = azul_core::resources::RendererResources::default();
    let ext = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = None;

    layout_window
        .layout_and_generate_display_list(styled_dom, &ws, &mut rr, &ext, &mut debug_messages)
        .map_err(|e| format!("layout: {}", e))?;

    let dl = layout_window
        .layout_results
        .remove(&DomId::ROOT_ID)
        .ok_or("no layout result")?
        .display_list;

    let mut gc = azul_layout::glyph_cache::GlyphCache::new();
    let pixmap = azul_layout::cpurender::render_with_font_manager(
        &dl,
        &rr,
        &layout_window.font_manager,
        azul_layout::cpurender::RenderOptions {
            width: width as f32,
            height: height as f32,
            dpi_factor: 1.0,
        },
        &mut gc,
    )
    .map_err(|e| format!("render: {}", e))?;

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {}", parent.display(), e))?;
    }

    let img = image::RgbaImage::from_raw(width, height, pixmap.data().to_vec())
        .ok_or_else(|| "image conversion failed".to_string())?;
    img.save(output_path)
        .map_err(|e| format!("save {}: {}", output_path.display(), e))?;
    Ok(())
}

fn wrap_xml_envelope(snippet: &str) -> String {
    let trimmed = snippet.trim();
    if trimmed.starts_with("<html") || trimmed.starts_with("<?xml") {
        return snippet.to_string();
    }
    if trimmed.starts_with("<body") {
        return format!(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\">{}</html>",
            snippet
        );
    }
    format!(
        "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body>{}</body></html>",
        snippet
    )
}

/// Manifest describing all rendered screenshots and slideshow groupings.
/// Written to `doc/guide/screenshots/manifest.json` after a successful
/// run. Consumed by the HTML rendering pipeline to expand `azul-render`
/// fences into figures and slideshow widgets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotManifest {
    pub version: u32,
    pub generated_at: String,
    pub screenshots: Vec<RenderBlock>,
    /// slideshow_id → ordered list of screenshot names that belong to it
    pub slideshows: BTreeMap<String, Vec<String>>,
}

pub fn screenshot_manifest_path(project_root: &Path, lang: &str) -> PathBuf {
    project_root.join(format!("doc/guide/{}/screenshots/manifest.json", lang))
}

fn page_language_from_path(page: &Path, project_root: &Path) -> Option<String> {
    let rel = page.strip_prefix(project_root.join("doc/guide")).ok()?;
    rel.iter()
        .next()
        .and_then(|s| s.to_str().map(|s| s.to_string()))
}

fn write_screenshot_manifest(
    project_root: &Path,
    lang: &str,
    blocks: &[RenderBlock],
) -> Result<(), String> {
    let mut slideshows: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for b in blocks {
        if let Some(sid) = &b.slideshow {
            slideshows
                .entry(sid.clone())
                .or_default()
                .push(b.name.clone());
        }
    }

    // Make source_page relative to project_root for portability.
    let mut entries = Vec::with_capacity(blocks.len());
    for b in blocks {
        let mut clone = b.clone();
        clone.source_page = b
            .source_page
            .strip_prefix(project_root)
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|_| b.source_page.clone());
        entries.push(clone);
    }

    let manifest = ScreenshotManifest {
        version: 1,
        generated_at: chrono::Utc::now().to_rfc3339(),
        screenshots: entries,
        slideshows,
    };

    let path = screenshot_manifest_path(project_root, lang);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("manifest json: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("write {}: {}", path.display(), e))?;
    Ok(())
}

pub fn run_autodoc_screenshots(config: &AutoreviewConfig) -> Result<(), String> {
    let project_root = &config.project_root;
    let guide_dir = project_root.join("doc/guide");

    println!("Initializing font context...");
    let font_context = init_screenshot_font_context()?;

    let mut pages = Vec::new();
    walk_md(&guide_dir, &mut pages);

    // Bucket pages by language: doc/guide/<lang>/...
    let mut by_lang: BTreeMap<String, Vec<RenderBlock>> = BTreeMap::new();
    for page in &pages {
        let lang = match page_language_from_path(page, project_root) {
            Some(l) => l,
            None => continue,
        };
        let content = match fs::read_to_string(page) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for block in extract_render_blocks(&content, page) {
            by_lang.entry(lang.clone()).or_default().push(block);
        }
    }

    let total: usize = by_lang.values().map(|v| v.len()).sum();
    let slideshow_count: usize = by_lang
        .values()
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|b| b.slideshow.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        })
        .sum();
    println!(
        "Found {} render block(s) across {} page(s), {} language(s), {} slideshow(s)",
        total,
        pages.len(),
        by_lang.len(),
        slideshow_count,
    );

    let mut ok = 0usize;
    let mut failed: Vec<(String, String)> = Vec::new();

    for (lang, blocks) in &by_lang {
        let screenshots_dir = guide_dir.join(lang).join("screenshots");
        fs::create_dir_all(&screenshots_dir)
            .map_err(|e| format!("mkdir {}: {}", screenshots_dir.display(), e))?;

        let mut succeeded: Vec<RenderBlock> = Vec::new();
        for block in blocks {
            let out = screenshots_dir.join(format!("{}.png", block.name));
            match render_xml_to_png(&font_context, &block.xml, &out, block.width, block.height)
            {
                Ok(()) => {
                    let badge = match (&block.slideshow, block.subtitle.is_empty()) {
                        (Some(sid), false) => {
                            format!(" [slide:{} | \"{}\"]", sid, block.subtitle)
                        }
                        (Some(sid), true) => format!(" [slide:{}]", sid),
                        (None, false) => format!(" [\"{}\"]", block.subtitle),
                        (None, true) => String::new(),
                    };
                    println!(
                        "  [{}/ok] {}.png ({}x{}){}",
                        lang, block.name, block.width, block.height, badge
                    );
                    ok += 1;
                    succeeded.push(block.clone());
                }
                Err(e) => {
                    eprintln!("  [{}/fail] {}: {}", lang, block.name, e);
                    failed.push((format!("{}/{}", lang, block.name), e));
                }
            }
        }

        if !succeeded.is_empty() {
            write_screenshot_manifest(project_root, lang, &succeeded)?;
            println!(
                "Wrote manifest: {}",
                screenshot_manifest_path(project_root, lang).display()
            );
        }
    }

    println!("\n{} ok, {} failed", ok, failed.len());
    if !failed.is_empty() {
        return Err(format!("{} screenshot(s) failed to render", failed.len()));
    }
    Ok(())
}

// ── Markdown → HTML expansion for azul-render fences ─────────────────
//
// Called by the docgen pipeline before comrak runs. Converts each
// `azul-render` fenced block into either a `<figure>` (single screenshot)
// or, when the block belongs to a slideshow, into a slideshow opener/closer
// that wraps consecutive frames with the same slideshow id.

/// Replace `azul-render` fenced blocks with figure/slideshow HTML.
/// `screenshot_url_prefix` is prepended to PNG filenames (e.g. "./screenshots/"
/// for relative-path use within a single guide page, or "/guide/screenshots/"
/// for absolute URLs at site root).
pub fn expand_azul_render_blocks(content: &str, screenshot_url_prefix: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut lines = content.lines().peekable();
    let mut current_slideshow: Option<String> = None;
    let mut frames_in_current: Vec<(String, String, u32, u32)> = Vec::new(); // name, subtitle, w, h

    fn flush_slideshow(
        out: &mut String,
        slideshow_id: &str,
        frames: &[(String, String, u32, u32)],
        prefix: &str,
    ) {
        out.push_str(&format!(
            "<div class=\"azul-slideshow\" data-name=\"{}\">\n",
            html_escape(slideshow_id)
        ));
        for (i, (name, subtitle, w, h)) in frames.iter().enumerate() {
            out.push_str(&format!(
                "  <figure class=\"azul-slide\" data-frame=\"{}\">\n    <img src=\"{}{}.png\" \
                 width=\"{}\" height=\"{}\" loading=\"lazy\"/>\n    <figcaption>{}</figcaption>\n  </figure>\n",
                i,
                prefix,
                name,
                w,
                h,
                html_escape(subtitle),
            ));
        }
        out.push_str("</div>\n");
    }

    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("```azul-render") {
            // Not an azul-render fence. If we were inside a slideshow, close it
            // when we hit a non-empty unrelated line (preserve blank-line gaps).
            if !line.trim().is_empty() && current_slideshow.is_some() {
                if let Some(id) = current_slideshow.take() {
                    flush_slideshow(&mut out, &id, &frames_in_current, screenshot_url_prefix);
                    frames_in_current.clear();
                }
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }

        // Parse attrs
        let attrs_str = trimmed.trim_start_matches("```azul-render").trim();
        let attrs = parse_attrs(attrs_str);
        let mut name = None;
        let mut width = 800u32;
        let mut height = 600u32;
        let mut subtitle = String::new();
        let mut slideshow: Option<String> = None;
        for (k, v) in &attrs {
            match k.as_str() {
                "screenshot" => name = Some(v.clone()),
                "width" => width = v.parse().unwrap_or(width),
                "height" => height = v.parse().unwrap_or(height),
                "subtitle" => subtitle = v.clone(),
                "slideshow" => slideshow = Some(v.clone()),
                _ => {}
            }
        }
        // Consume body
        for inner in lines.by_ref() {
            if inner.trim_start().starts_with("```") {
                break;
            }
        }
        let name = match name {
            Some(n) => n,
            None => continue, // malformed, drop silently
        };

        match (slideshow.clone(), &current_slideshow) {
            (Some(sid), Some(current)) if &sid == current => {
                // continuing slideshow
                frames_in_current.push((name, subtitle, width, height));
            }
            (Some(sid), _) => {
                // starting new slideshow (or different one); flush previous
                if let Some(id) = current_slideshow.take() {
                    flush_slideshow(&mut out, &id, &frames_in_current, screenshot_url_prefix);
                    frames_in_current.clear();
                }
                current_slideshow = Some(sid);
                frames_in_current.push((name, subtitle, width, height));
            }
            (None, _) => {
                // single figure; flush any open slideshow first
                if let Some(id) = current_slideshow.take() {
                    flush_slideshow(&mut out, &id, &frames_in_current, screenshot_url_prefix);
                    frames_in_current.clear();
                }
                out.push_str(&format!(
                    "<figure class=\"azul-screenshot\">\n  <img src=\"{}{}.png\" width=\"{}\" \
                     height=\"{}\" loading=\"lazy\"/>\n",
                    screenshot_url_prefix, name, width, height,
                ));
                if !subtitle.is_empty() {
                    out.push_str(&format!(
                        "  <figcaption>{}</figcaption>\n",
                        html_escape(&subtitle)
                    ));
                }
                out.push_str("</figure>\n");
            }
        }
    }
    if let Some(id) = current_slideshow.take() {
        flush_slideshow(&mut out, &id, &frames_in_current, screenshot_url_prefix);
    }
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
