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
}

#[derive(Debug, Deserialize, Clone)]
pub struct Group {
    pub id: String,
    pub audience: Vec<String>,
    pub agent_strategy: String,
    #[serde(default)]
    pub tracked_files: Vec<String>,
    #[serde(default)]
    pub tracked_globs: Vec<String>,
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

pub fn build_autodoc_prompt(project_root: &Path, group: &Group) -> String {
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

    s.push_str(&format!("## Strategy: `{}`\n\n", group.agent_strategy));
    s.push_str(&format!("Audience(s): {}\n\n", group.audience.join(", ")));
    if let Some(notes) = &group.notes {
        s.push_str(&format!("Notes: {}\n\n", notes));
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
                slug: <slug>\n\
                title: <title>\n\
                audience: <external | contributor>\n\
                maturity: <mature | wip | stub | draft>\n\
                guide_order: <int or null>\n\
                topic_only: <bool>\n\
                prerequisites: [<slug>, ...]\n\
                tracked_files:\n  - <path>\n  - ...\n\
                last_generated_rev: <git-sha-here>\n\
                generated_at: <iso8601>\n\
                ---\n```\n\n");
    s.push_str(&format!(
        "Use git rev `{}` and the current ISO timestamp. Tracked files come from the manifest \
         (listed below).\n\n",
        head_sha(project_root).unwrap_or_else(|_| "UNKNOWN".to_string())
    ));

    s.push_str("## Tracked source files\n\n");
    if tracked.is_empty() {
        s.push_str("(none — agent must read source files directly with the Read tool)\n\n");
    } else {
        for p in &tracked {
            s.push_str(&format!("- `{}`\n", p.display()));
        }
        s.push('\n');
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
     6. **Visual examples**: if you want a screenshot, embed a fenced block with \
        the language tag `azul-render`:\n\
        ```\n\
        ```azul-render screenshot=my-example\n\
        <minimal Dom-construction code>\n\
        ```\n\
        ```\n\
        The post-step renders these via HeadlessWindow and saves PNGs to \
        `doc/guide/screenshots/<screenshot>.png`. Reference them in markdown as \
        `![](./screenshots/<screenshot>.png)`.\n\
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
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub audience: Option<String>,
    pub maturity: String,
    #[serde(default)]
    pub guide_order: Option<i32>,
    #[serde(default)]
    pub topic_only: bool,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    #[serde(default)]
    pub tracked_files: Vec<String>,
    pub last_generated_rev: String,
    #[serde(default)]
    pub generated_at: String,
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
        let prompt = build_autodoc_prompt(&project_root, group);
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

    // Phase 3: regenerate SUMMARY.md
    println!("\n=== Phase 3: Regenerating SUMMARY.md ===\n");
    regenerate_summary(&project_root, &manifest)?;

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
        let mut stales = Vec::new();
        for f in &fm.tracked_files {
            let commits = commits_since(project_root, &fm.last_generated_rev, f)
                .unwrap_or_default();
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

    write_outdated_report(project_root, fresh, &stale_pages, &no_frontmatter)?;
    let report = outdated_report_path(project_root);
    println!(
        "Pages: {} fresh, {} stale, {} without frontmatter",
        fresh,
        stale_pages.len(),
        no_frontmatter.len()
    );
    println!("Report: {}", report.display());
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
    no_fm: &[PathBuf],
) -> Result<(), String> {
    let path = outdated_report_path(project_root);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut s = String::new();
    s.push_str("# Autodoc — outdated check\n\n");
    s.push_str(&format!(
        "- {} fresh pages\n- {} stale pages\n- {} pages without frontmatter\n\n",
        fresh,
        stale.len(),
        no_fm.len()
    ));
    if !stale.is_empty() {
        s.push_str("## Stale pages\n\n");
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

// ── SUMMARY.md generation ──────────────────────────────────────────────

pub fn regenerate_summary(project_root: &Path, manifest: &Manifest) -> Result<(), String> {
    let path = project_root.join("doc/guide/SUMMARY.md");
    let mut s = String::new();
    s.push_str("# Summary\n\n");
    s.push_str("> Auto-generated by `azul-doc autoreview autodoc`.  Hand-edit only the existing `architecture.md` block.\n\n");

    // External — linear guide, ordered by guide_order
    s.push_str("# Guide\n\n");
    let mut external_linear: Vec<(&Output, &Group)> = Vec::new();
    let mut external_topics: Vec<(&Output, &Group)> = Vec::new();
    let mut contributor: Vec<(&Output, &Group)> = Vec::new();

    for g in &manifest.groups {
        for o in &g.outputs {
            let aud = o.audience.as_deref().or_else(|| {
                if g.audience.len() == 1 {
                    Some(g.audience[0].as_str())
                } else {
                    None
                }
            });
            match aud {
                Some("external") => {
                    if o.topic_only {
                        external_topics.push((o, g));
                    } else {
                        external_linear.push((o, g));
                    }
                }
                Some("contributor") => contributor.push((o, g)),
                _ => {}
            }
        }
    }

    external_linear.sort_by_key(|(o, _)| o.guide_order.unwrap_or(i32::MAX));
    for (o, _g) in &external_linear {
        let rel = o.path.strip_prefix("doc/guide/").unwrap_or(&o.path);
        let badge = match o.maturity.as_str() {
            "wip" => " (WIP)",
            "stub" => " (not yet functional)",
            "draft" => " (draft)",
            _ => "",
        };
        s.push_str(&format!("- [{}{}]({})\n", o.title, badge, rel));
    }

    if !external_topics.is_empty() {
        s.push_str("\n# Topics\n\n");
        external_topics.sort_by(|(a, _), (b, _)| a.slug.cmp(&b.slug));
        for (o, _) in &external_topics {
            let rel = o.path.strip_prefix("doc/guide/").unwrap_or(&o.path);
            let badge = match o.maturity.as_str() {
                "wip" => " (WIP)",
                "stub" => " (not yet functional)",
                _ => "",
            };
            s.push_str(&format!("- [{}{}]({})\n", o.title, badge, rel));
        }
    }

    s.push_str("\n# Contributor reference\n\n");
    let mut by_prefix: BTreeMap<String, Vec<&Output>> = BTreeMap::new();
    for (o, _) in &contributor {
        let prefix = o
            .slug
            .strip_prefix("internals/")
            .and_then(|s| s.split('-').next())
            .unwrap_or("misc")
            .to_string();
        by_prefix.entry(prefix).or_default().push(o);
    }
    for (prefix, mut outs) in by_prefix {
        outs.sort_by(|a, b| a.slug.cmp(&b.slug));
        s.push_str(&format!("\n## {}\n\n", prefix));
        for o in outs {
            let rel = o.path.strip_prefix("doc/guide/").unwrap_or(&o.path);
            let badge = match o.maturity.as_str() {
                "wip" => " (WIP)",
                "stub" => " (not yet functional)",
                _ => "",
            };
            s.push_str(&format!("- [{}{}]({})\n", o.title, badge, rel));
        }
    }

    fs::write(&path, s).map_err(|e| format!("write SUMMARY.md: {}", e))?;
    println!("Wrote {}", path.display());
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

#[derive(Debug, Clone)]
pub struct RenderBlock {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub xml: String,
    pub source_page: PathBuf,
}

pub fn extract_render_blocks(content: &str, page_path: &Path) -> Vec<RenderBlock> {
    let mut out = Vec::new();
    let mut lines = content.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("```azul-render") {
            continue;
        }
        let attrs = trimmed.trim_start_matches("```azul-render").trim();
        let mut name = None;
        let mut width = 800u32;
        let mut height = 600u32;
        for kv in attrs.split_whitespace() {
            let (k, v) = match kv.split_once('=') {
                Some(x) => x,
                None => continue,
            };
            match k {
                "screenshot" => name = Some(v.to_string()),
                "width" => width = v.parse().unwrap_or(width),
                "height" => height = v.parse().unwrap_or(height),
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

pub fn run_autodoc_screenshots(config: &AutoreviewConfig) -> Result<(), String> {
    let project_root = &config.project_root;
    let guide_dir = project_root.join("doc/guide");
    let screenshots_dir = guide_dir.join("screenshots");
    fs::create_dir_all(&screenshots_dir)
        .map_err(|e| format!("mkdir screenshots: {}", e))?;

    println!("Initializing font context...");
    let font_context = init_screenshot_font_context()?;

    let mut pages = Vec::new();
    walk_md(&guide_dir, &mut pages);

    let mut all_blocks: Vec<RenderBlock> = Vec::new();
    for page in &pages {
        let content = match fs::read_to_string(page) {
            Ok(c) => c,
            Err(_) => continue,
        };
        all_blocks.extend(extract_render_blocks(&content, page));
    }

    println!(
        "Found {} render block(s) across {} page(s)",
        all_blocks.len(),
        pages.len()
    );

    let mut ok = 0usize;
    let mut failed: Vec<(String, String)> = Vec::new();
    for block in &all_blocks {
        let out = screenshots_dir.join(format!("{}.png", block.name));
        match render_xml_to_png(&font_context, &block.xml, &out, block.width, block.height) {
            Ok(()) => {
                println!(
                    "  [ok] {}.png ({}x{})",
                    block.name, block.width, block.height
                );
                ok += 1;
            }
            Err(e) => {
                eprintln!("  [fail] {}: {}", block.name, e);
                failed.push((block.name.clone(), e));
            }
        }
    }

    println!("\n{} ok, {} failed", ok, failed.len());
    if !failed.is_empty() {
        return Err(format!("{} screenshot(s) failed to render", failed.len()));
    }
    Ok(())
}
