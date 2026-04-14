//! Interactive, commit-by-commit replay of a reference branch.
//!
//! Walks every commit in a reference branch (e.g. `midlevel-fixes-reference`)
//! and prompts the user to decide what to do with each one:
//!
//!   [y] apply as-is (cherry-pick; LLM-graft if it doesn't apply cleanly)
//!   [n] reject (don't apply)
//!   [e] edit — give a custom instruction to a Claude agent
//!   [s] show diff again
//!   [q] quit (progress is saved)
//!
//! Pure-`.md` commits are auto-skipped. When a code commit is paired with a
//! following `docs: update autoreview report …` commit, the report's diff is
//! shown as context but the docs commit itself is also auto-skipped (we drop
//! all `.md` changes from the new history).
//!
//! After every applied commit we run `codegen all` + `autofix` so that each
//! commit is a buildable DLL.
//!
//! Progress is saved to `doc/target/autoreview/apply-midlevel/progress.json`
//! after every decision. Re-running the command resumes where it left off.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

// ── Public API ────────────────────────────────────────────────────────────

pub struct Config {
    pub reference: String,
    pub base: Option<String>,
    pub project_root: PathBuf,
    pub model: Option<String>,
}

pub fn parse_args(args: &[&str], project_root: &Path) -> Result<Config, String> {
    let mut reference = None;
    let mut base = None;
    let mut model = None;

    for arg in args {
        if let Some(v) = arg.strip_prefix("--reference=") {
            reference = Some(v.to_string());
        } else if let Some(v) = arg.strip_prefix("--base=") {
            base = Some(v.to_string());
        } else if let Some(v) = arg.strip_prefix("--model=") {
            model = Some(v.to_string());
        } else if arg.starts_with('-') {
            return Err(format!("Unknown option: {}", arg));
        }
    }

    let reference = reference
        .ok_or_else(|| "--reference=<branch-or-tag> is required".to_string())?;

    Ok(Config {
        reference,
        base,
        project_root: project_root.to_path_buf(),
        model,
    })
}

pub fn run(config: Config) -> Result<(), String> {
    let project_root = config.project_root.clone();

    // Resolve reference and base SHAs up-front
    let reference_sha = git_rev_parse(&project_root, &config.reference)?;
    let base_ref = config.base.clone()
        .unwrap_or_else(|| "origin/layout-debug-clean".to_string());
    let base_sha = git_rev_parse(&project_root, &base_ref)?;

    // Load existing progress or bootstrap a fresh one
    let progress_path = progress_path(&project_root);
    fs::create_dir_all(progress_path.parent().unwrap())
        .map_err(|e| format!("Failed to create progress dir: {}", e))?;

    let mut progress = load_progress(&progress_path)
        .unwrap_or_else(|_| Progress::new(&config.reference, &reference_sha, &base_sha));

    // Safety: don't allow reusing progress from a different reference
    if progress.reference != config.reference || progress.reference_sha != reference_sha {
        return Err(format!(
            "progress.json is for reference {} (sha {}), but you asked for {} (sha {}). \
             Delete {} to start fresh.",
            progress.reference, progress.reference_sha,
            config.reference, reference_sha,
            progress_path.display()
        ));
    }

    // Build the ordered commit list (oldest → newest)
    let commits = git_commit_list(&project_root, &base_sha, &reference_sha)?;
    let total = commits.len();
    println!("Reference {} → {} commits", config.reference, total);
    println!("Base: {}", base_ref);
    println!("Progress: {}/{} processed\n", progress.processed.len(), total);

    // Main loop
    loop {
        let next = match find_next(&commits, &progress) {
            Some(sha) => sha.clone(),
            None => {
                println!("All commits processed.");
                break;
            }
        };

        let info = load_commit_info(&project_root, &next)?;
        let paired_docs = find_paired_docs(&project_root, &commits, &next)?;

        // Auto-skip pure-md commits (docs-only)
        if info.is_pure_md {
            println!("[skip] {}  {}  (pure .md)", short(&next), info.subject);
            progress.current = None;
            progress.processed.push(Decision {
                sha: next.clone(),
                subject: info.subject.clone(),
                decision: DecisionKind::SkippedMd,
                new_sha: None,
                notes: None,
            });
            save_progress(&progress_path, &progress)?;
            continue;
        }

        // Show the commit
        let processed_so_far = progress.processed.len();
        print_commit_summary(processed_so_far + 1, total, &info, paired_docs.as_ref());

        // Mark current and save — so Ctrl+C leaves a known-good pointer
        progress.current = Some(next.clone());
        save_progress(&progress_path, &progress)?;

        let action = prompt_user()?;

        match action {
            UserAction::Yes | UserAction::Edit(_) => {
                let user_instruction = match &action {
                    UserAction::Edit(s) => Some(s.as_str()),
                    _ => None,
                };

                let outcome = run_apply_agent(
                    &project_root,
                    &next,
                    &info,
                    paired_docs.as_ref(),
                    user_instruction,
                    &config,
                );

                match outcome {
                    Ok(Applied { new_sha }) => {
                        progress.current = None;
                        progress.processed.push(Decision {
                            sha: next.clone(),
                            subject: info.subject.clone(),
                            decision: if user_instruction.is_some() {
                                DecisionKind::AppliedEdited
                            } else {
                                DecisionKind::AppliedByAgent
                            },
                            new_sha: Some(new_sha),
                            notes: user_instruction.map(|s| s.to_string()),
                        });
                        save_progress(&progress_path, &progress)?;
                        println!();
                    }
                    Err(e) => {
                        println!("\n[ERROR] agent apply failed: {}\n", e);
                        println!("Repository state left as-is. Resolve manually or quit.");
                        return Err(e);
                    }
                }
            }
            UserAction::No(notes) => {
                progress.current = None;
                progress.processed.push(Decision {
                    sha: next.clone(),
                    subject: info.subject.clone(),
                    decision: DecisionKind::Rejected,
                    new_sha: None,
                    notes,
                });
                save_progress(&progress_path, &progress)?;
                println!();
            }
            UserAction::Show => {
                // Re-print and loop back
                continue;
            }
            UserAction::Quit => {
                println!("Saving progress and exiting.");
                save_progress(&progress_path, &progress)?;
                break;
            }
        }
    }

    Ok(())
}

// ── Progress types ────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Progress {
    reference: String,
    reference_sha: String,
    base_sha: String,
    /// SHA of commit we were about to process when we last saved. `None`
    /// means we're between commits (cleanly saved).
    current: Option<String>,
    processed: Vec<Decision>,
}

impl Progress {
    fn new(reference: &str, reference_sha: &str, base_sha: &str) -> Self {
        Self {
            reference: reference.to_string(),
            reference_sha: reference_sha.to_string(),
            base_sha: base_sha.to_string(),
            current: None,
            processed: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Decision {
    sha: String,
    subject: String,
    decision: DecisionKind,
    new_sha: Option<String>,
    notes: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum DecisionKind {
    /// Agent applied the commit (cherry-pick or graft, it decides) and
    /// the full CI pipeline + cross-compile succeeded.
    AppliedByAgent,
    /// User provided a custom instruction; agent applied and verified.
    AppliedEdited,
    /// User rejected the commit; nothing applied.
    Rejected,
    /// Pure-`.md` commit, auto-skipped.
    SkippedMd,
}

fn progress_path(project_root: &Path) -> PathBuf {
    project_root.join("doc/target/autoreview/apply-midlevel/progress.json")
}

fn load_progress(path: &Path) -> Result<Progress, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn save_progress(path: &Path, progress: &Progress) -> Result<(), String> {
    let json = serde_json::to_string_pretty(progress)
        .map_err(|e| format!("Serialize progress: {}", e))?;
    // Write via tmp + rename for crash-safety
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &json)
        .map_err(|e| format!("Write progress tmp: {}", e))?;
    fs::rename(&tmp, path)
        .map_err(|e| format!("Rename progress tmp: {}", e))?;
    Ok(())
}

// ── Commit inspection ─────────────────────────────────────────────────────

struct CommitInfo {
    sha: String,
    subject: String,
    body: String,
    files: Vec<FileChange>,
    is_pure_md: bool,
    /// Path to a Rust source file referenced in the subject, derived as
    /// `layout/src/widgets/label.rs` from a subject like
    /// "refactor: foo in layout/src/widgets/label.rs".
    subject_source_path: Option<String>,
}

struct FileChange {
    path: String,
    additions: u32,
    deletions: u32,
}

fn load_commit_info(project_root: &Path, sha: &str) -> Result<CommitInfo, String> {
    let (subject, body) = git_commit_message(project_root, sha)?;
    let files = git_commit_numstat(project_root, sha)?;
    let is_pure_md = !files.is_empty()
        && files.iter().all(|f| f.path.ends_with(".md"));

    // Heuristic: any token in the subject ending in `.rs` that points at a real file
    let subject_source_path = subject
        .split(|c: char| c.is_whitespace() || c == ',' || c == ':')
        .find(|tok| tok.ends_with(".rs"))
        .map(|s| s.to_string());

    Ok(CommitInfo {
        sha: sha.to_string(),
        subject,
        body,
        files,
        is_pure_md,
        subject_source_path,
    })
}

/// If the *next* commit after `sha` is a paired `docs: update autoreview report …`,
/// return its info. We use this to display the agent's justification alongside the
/// code change, even though we drop the docs commit itself.
fn find_paired_docs(
    project_root: &Path,
    commits: &[String],
    sha: &str,
) -> Result<Option<CommitInfo>, String> {
    let idx = match commits.iter().position(|c| c == sha) {
        Some(i) => i,
        None => return Ok(None),
    };
    let Some(next) = commits.get(idx + 1) else { return Ok(None); };

    let info = load_commit_info(project_root, next)?;
    let is_docs = info.subject.starts_with("docs: update autoreview report")
        || info.subject.starts_with("docs: autoreview report");
    if is_docs {
        Ok(Some(info))
    } else {
        Ok(None)
    }
}

fn find_next<'a>(commits: &'a [String], progress: &Progress) -> Option<&'a String> {
    // Collect set of already-processed SHAs (including auto-skipped docs that
    // were implicitly skipped — those appear as SkippedMd entries).
    let done: std::collections::HashSet<&str> =
        progress.processed.iter().map(|d| d.sha.as_str()).collect();
    commits.iter().find(|c| !done.contains(c.as_str()))
}

// ── UI ────────────────────────────────────────────────────────────────────

fn print_commit_summary(
    n: usize,
    total: usize,
    info: &CommitInfo,
    paired: Option<&CommitInfo>,
) {
    println!("════════════════════════════════════════════════════════════════════════");
    println!("Commit {}/{}  {}", n, total, &info.sha[..12]);
    println!("Subject: {}", info.subject);
    if !info.body.is_empty() {
        println!("Body:");
        for line in info.body.lines() {
            println!("  {}", line);
        }
    }
    println!("\nFiles:");
    for f in &info.files {
        println!("  {:>5} +{:<4} -{:<4}  {}", "", f.additions, f.deletions, f.path);
    }
    if let Some(p) = paired {
        println!("\nPaired docs commit: {}  {}", &p.sha[..12], p.subject);
    }
    println!("────────────────────────────────────────────────────────────────────────");
}

enum UserAction {
    Yes,
    No(Option<String>),
    Edit(String),
    Show,
    Quit,
}

fn prompt_user() -> Result<UserAction, String> {
    print!("Decision? [y]es / [n]o / [e]dit / [s]how-diff / [q]uit: ");
    io::stdout().flush().ok();

    let line = read_line()?;
    let c = line.trim().chars().next().unwrap_or(' ');

    match c {
        'y' | 'Y' => Ok(UserAction::Yes),
        'n' | 'N' => {
            println!("  Reason (one line, empty to skip):");
            let notes = read_line()?;
            let notes = notes.trim().to_string();
            Ok(UserAction::No(if notes.is_empty() { None } else { Some(notes) }))
        }
        'e' | 'E' => {
            println!("  Instructions for the agent (end with a single '.' on its own line):");
            let instr = read_multiline_until_dot()?;
            Ok(UserAction::Edit(instr))
        }
        's' | 'S' => Ok(UserAction::Show),
        'q' | 'Q' => Ok(UserAction::Quit),
        _ => {
            println!("  (unrecognised — showing again)");
            Ok(UserAction::Show)
        }
    }
}

fn read_line() -> Result<String, String> {
    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line)
        .map_err(|e| format!("stdin read: {}", e))?;
    Ok(line)
}

fn read_multiline_until_dot() -> Result<String, String> {
    let stdin = io::stdin();
    let mut buf = String::new();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| format!("stdin read: {}", e))?;
        if line.trim() == "." { break; }
        buf.push_str(&line);
        buf.push('\n');
    }
    Ok(buf)
}

// ── Git helpers ──────────────────────────────────────────────────────────

fn git_rev_parse(project_root: &Path, reference: &str) -> Result<String, String> {
    let out = Command::new("git")
        .args(["rev-parse", reference])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git rev-parse {}: {}", reference, e))?;
    if !out.status.success() {
        return Err(format!(
            "git rev-parse {} failed: {}",
            reference,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn git_commit_list(
    project_root: &Path,
    base: &str,
    reference: &str,
) -> Result<Vec<String>, String> {
    let out = Command::new("git")
        .args(["log", "--reverse", "--format=%H", &format!("{}..{}", base, reference)])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git log: {}", e))?;
    if !out.status.success() {
        return Err(format!("git log: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

fn git_commit_message(project_root: &Path, sha: &str) -> Result<(String, String), String> {
    let out = Command::new("git")
        .args(["log", "-1", "--format=%s%n%b", sha])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git log -1 {}: {}", sha, e))?;
    if !out.status.success() {
        return Err(format!("git log -1 {}: {}", sha, String::from_utf8_lossy(&out.stderr)));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut iter = text.splitn(2, '\n');
    let subject = iter.next().unwrap_or("").trim().to_string();
    let body = iter.next().unwrap_or("").trim().to_string();
    Ok((subject, body))
}

fn git_commit_numstat(project_root: &Path, sha: &str) -> Result<Vec<FileChange>, String> {
    let out = Command::new("git")
        .args(["show", "--numstat", "--format=", sha])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git show --numstat: {}", e))?;
    if !out.status.success() {
        return Err(format!("git show --numstat: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let mut changes = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let mut cols = line.split('\t');
        let a = cols.next().unwrap_or("0");
        let d = cols.next().unwrap_or("0");
        let path = cols.next().unwrap_or("").to_string();
        if path.is_empty() { continue; }
        changes.push(FileChange {
            path,
            additions: a.parse().unwrap_or(0),
            deletions: d.parse().unwrap_or(0),
        });
    }
    Ok(changes)
}

fn git_head(project_root: &Path) -> Result<String, String> {
    git_rev_parse(project_root, "HEAD")
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

// ── Apply (single agent-driven path) ─────────────────────────────────────

struct Applied {
    new_sha: String,
}

/// Spawn one Claude agent that owns the entire apply + verification pipeline:
///   1. cherry-pick (or graft manually if that fails)
///   2. drop .md files
///   3. commit
///   4. run the CI pipeline (autofix → patch → normalize → codegen all)
///   5. cross-compile check on linux + windows + darwin
///   6. amend any pipeline output into the commit
///
/// The CLI verifies success by checking that HEAD advanced, the tree is clean,
/// and the new commit's diff touches no `.md` files.
fn run_apply_agent(
    project_root: &Path,
    sha: &str,
    info: &CommitInfo,
    paired: Option<&CommitInfo>,
    user_instruction: Option<&str>,
    config: &Config,
) -> Result<Applied, String> {
    let pre_head = git_head(project_root)?;

    // Refuse to start if the tree is dirty — the agent needs a clean slate.
    let dirty = !index_is_empty(project_root)? || has_worktree_changes(project_root)?;
    if dirty {
        return Err(
            "working tree / index not clean. Commit or stash before continuing.".into(),
        );
    }

    let prompt = build_agent_prompt(project_root, sha, info, paired, user_instruction)?;

    // Persist the prompt for auditing. Agent output is streamed to the user's
    // terminal directly (so they can watch progress and see the session ID the
    // claude CLI prints). If they want a persistent log they can pipe the
    // whole invocation through `tee`.
    let agent_dir = project_root.join("doc/target/autoreview/apply-midlevel/agent-prompts");
    fs::create_dir_all(&agent_dir).ok();
    let prompt_path = agent_dir.join(format!("{}.md", short(sha)));
    fs::write(&prompt_path, &prompt).ok();

    println!("  → spawning claude agent (prompt saved to {})", prompt_path.display());
    println!("  → agent output streams below — the session ID will appear in claude's banner");
    println!("────────────────────────────────────────────────────────────────────────");

    let mut cmd_args: Vec<&str> = vec![
        "-p",
        "--dangerously-skip-permissions",
        "--verbose",
        "--disallowedTools", "mcp__*",
        "--disallowedTools", "rust-analyzer-lsp",
    ];
    if let Some(m) = config.model.as_ref() {
        cmd_args.push("--model");
        cmd_args.push(m);
    }

    // Ensure the agent uses the rustup toolchain (which has all cross-compile
    // targets installed) rather than Homebrew's cargo, which doesn't.
    let path_with_rustup = rustup_prefixed_path();

    let mut child = Command::new("claude")
        .args(&cmd_args)
        .env_remove("CLAUDECODE")
        .env("PATH", &path_with_rustup)
        .current_dir(project_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("spawn claude: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes())
            .map_err(|e| format!("write prompt stdin: {}", e))?;
        // Close stdin so claude -p knows input is complete
        drop(stdin);
    }

    let status = child.wait().map_err(|e| format!("wait claude: {}", e))?;
    println!("────────────────────────────────────────────────────────────────────────");
    if !status.success() {
        return Err(format!("claude exited with status {}", status));
    }

    // Verify: HEAD advanced, tree clean, and the new commit doesn't touch .md.
    let post_head = git_head(project_root)?;
    if post_head == pre_head {
        return Err("agent made no commits".into());
    }

    if has_worktree_changes(project_root)? || !index_is_empty(project_root)? {
        return Err("agent left working tree dirty after committing".into());
    }

    let md_hits = git_diff_touches_md(project_root, &pre_head, &post_head)?;
    if !md_hits.is_empty() {
        return Err(format!("agent committed .md files: {}", md_hits.join(", ")));
    }

    Ok(Applied { new_sha: post_head })
}

fn build_agent_prompt(
    project_root: &Path,
    sha: &str,
    info: &CommitInfo,
    paired: Option<&CommitInfo>,
    user_instruction: Option<&str>,
) -> Result<String, String> {
    let code_diff = git_show_diff(project_root, sha)?;
    let docs_diff = match paired {
        Some(p) => git_show_diff(project_root, &p.sha)?,
        None => String::new(),
    };

    let mut p = String::new();
    p.push_str("You are replaying ONE commit from a reference branch onto the current\n");
    p.push_str("working tree. The original commit below was produced by an automated\n");
    p.push_str("code-review agent during a mid-level cleanup pass. Your job is to apply\n");
    p.push_str("the same intent to the current tree and PROVE via the CI pipeline that\n");
    p.push_str("the commit doesn't break anything on any supported platform.\n\n");

    p.push_str("═══ STEP 1 — Apply the commit's intent ════════════════════════════════\n\n");
    if user_instruction.is_none() {
        p.push_str("First try a clean cherry-pick:\n\n");
        p.push_str(&format!("    git cherry-pick --no-commit {}\n\n", sha));
        p.push_str("If it applies cleanly: continue to step 2.\n");
        p.push_str("If it conflicts:\n");
        p.push_str("    git cherry-pick --abort\n");
        p.push_str("and manually apply the INTENT of the diff below, using the paired\n");
        p.push_str("review report for context about WHY the change was made.\n");
    } else {
        p.push_str("IGNORE the original commit's diff as a literal instruction.\n");
        p.push_str("Follow the USER INSTRUCTION below instead. Use the original diff\n");
        p.push_str("and paired report only as reference context for what the original\n");
        p.push_str("change was trying to accomplish.\n");
    }
    p.push_str("\n");

    p.push_str("═══ STEP 2 — Drop all .md files ═══════════════════════════════════════\n\n");
    p.push_str("Run:\n");
    p.push_str("    git reset HEAD -- '*.md'\n");
    p.push_str("    git checkout -- '*.md'\n");
    p.push_str("    # also remove any NEW untracked .md files:\n");
    p.push_str("    git ls-files --others --exclude-standard '*.md' | xargs -r rm -f\n");
    p.push_str("If after dropping .md the staged diff is empty, abort (nothing to commit).\n\n");

    p.push_str("═══ STEP 3 — Commit the source changes ════════════════════════════════\n\n");
    p.push_str("Use this subject line verbatim:\n\n");
    p.push_str(&format!("    {}\n\n", info.subject));
    if !info.body.is_empty() {
        p.push_str("Include the original body in the commit message. Append nothing else —\n");
        p.push_str("NO `Co-Authored-By`, NO `Generated with Claude Code` footer.\n\n");
    } else {
        p.push_str("NO body, NO `Co-Authored-By`, NO `Generated with Claude Code` footer.\n\n");
    }

    p.push_str("═══ STEP 4 — Run the FULL CI pipeline ═════════════════════════════════\n\n");
    p.push_str("Run these in order. If any step fails, investigate and fix the root cause\n");
    p.push_str("before continuing. Do NOT skip any step. ALWAYS use `--release` (`-r`) for\n");
    p.push_str("every cargo invocation — debug artifacts can easily fill the disk.\n\n");
    p.push_str("    cargo run -r -p azul-doc -- autofix\n");
    p.push_str("    cargo run -r -p azul-doc -- patch safe target/autofix/patches\n");
    p.push_str("    cargo run -r -p azul-doc -- patch      target/autofix/patches\n");
    p.push_str("    cargo run -r -p azul-doc -- normalize\n");
    p.push_str("    cargo run -r -p azul-doc -- codegen all\n\n");
    p.push_str("After these, if `git status --porcelain` is non-empty, drop any .md changes\n");
    p.push_str("(same commands as step 2), stage everything else, and amend the commit:\n\n");
    p.push_str("    git add -A\n");
    p.push_str("    git reset HEAD -- '*.md'\n");
    p.push_str("    git checkout -- '*.md'\n");
    p.push_str("    git commit --amend --no-edit\n\n");

    p.push_str("═══ STEP 5 — Cross-compile verification ═══════════════════════════════\n\n");
    p.push_str("Verify the DLL still compiles on every supported target. ALWAYS use\n");
    p.push_str("`--release` — disk space is limited and debug artifacts are much larger\n");
    p.push_str("(sometimes 5-10× larger) than release artifacts.\n\n");
    p.push_str("    cargo check --release -p azul-dll --features build-dll                                    # host (darwin)\n");
    p.push_str("    cargo check --release --target x86_64-unknown-linux-gnu -p azul-dll --features build-dll   # linux\n");
    p.push_str("    cargo check --release --target x86_64-pc-windows-gnu    -p azul-dll --features build-dll   # windows\n\n");
    p.push_str("If any of these targets isn't installed, install it with `rustup target add <t>`.\n");
    p.push_str("If any target FAILS TO COMPILE, investigate the error and fix it — the whole\n");
    p.push_str("point of this step is catching platform-specific regressions. Do NOT finish\n");
    p.push_str("with a broken cross-compile.\n\n");
    p.push_str("If cross-compile introduces new changes (rare, but possible with generated code),\n");
    p.push_str("amend them into the commit the same way as step 4.\n\n");
    p.push_str("If the build fails with ENOSPC (disk full), you may need to run\n");
    p.push_str("`cargo clean --target <that-target>` between target checks to reclaim space.\n\n");

    p.push_str("═══ STEP 6 — Final invariants (you MUST satisfy all of these) ════════\n\n");
    p.push_str("  • HEAD points at exactly ONE new commit (your new SHA).\n");
    p.push_str("  • `git status --porcelain` is EMPTY.\n");
    p.push_str("  • The new commit's diff contains NO .md files.\n");
    p.push_str("  • All 3 cross-compile targets pass `cargo check`.\n\n");

    // ── Reference material ──────────────────────────────────────────────
    p.push_str(&format!("Original commit SHA: {}\n", sha));
    p.push_str(&format!("Original subject: {}\n", info.subject));
    if !info.body.is_empty() {
        p.push_str(&format!("Original body:\n{}\n", info.body));
    }

    if let Some(instr) = user_instruction {
        p.push_str("\n=== USER INSTRUCTION (overrides original intent) ===\n");
        p.push_str(instr);
        p.push_str("\n=== END USER INSTRUCTION ===\n");
    }

    p.push_str("\n=== ORIGINAL COMMIT DIFF ===\n");
    p.push_str(&code_diff);
    p.push_str("\n=== END ORIGINAL COMMIT DIFF ===\n");

    if !docs_diff.is_empty() {
        p.push_str("\n=== PAIRED REVIEW REPORT DIFF (context only — DO NOT COMMIT) ===\n");
        p.push_str(&docs_diff);
        p.push_str("\n=== END REPORT DIFF ===\n");
    }

    p.push_str("\nProceed through all six steps. Do not stop until the final invariants hold.\n");
    Ok(p)
}

// ── Git plumbing ─────────────────────────────────────────────────────────

fn git_show_diff(project_root: &Path, sha: &str) -> Result<String, String> {
    let out = Command::new("git")
        .args(["show", sha])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git show: {}", e))?;
    if !out.status.success() {
        return Err(format!("git show: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn drop_md_changes(project_root: &Path) -> Result<(), String> {
    // Unstage any .md files
    let _ = Command::new("git")
        .args(["reset", "HEAD", "--", "*.md"])
        .current_dir(project_root)
        .output();
    // Revert working-tree .md changes
    let _ = Command::new("git")
        .args(["checkout", "--", "*.md"])
        .current_dir(project_root)
        .output();
    // Remove any new untracked .md files the agent may have added
    let out = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard", "*.md"])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git ls-files: {}", e))?;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let p = project_root.join(line.trim());
        if p.is_file() { let _ = fs::remove_file(&p); }
    }
    Ok(())
}

fn index_is_empty(project_root: &Path) -> Result<bool, String> {
    let out = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git diff --cached: {}", e))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

/// Build a PATH with `~/.cargo/bin` prepended so rustup-managed `cargo`/`rustc`
/// (with installed cross-compile targets) wins over any system-wide install
/// (e.g. Homebrew rustc on macOS, which ships only the host sysroot).
fn rustup_prefixed_path() -> String {
    let existing = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let rustup_bin = format!("{}/.cargo/bin", home);
    if existing.split(':').any(|p| p == rustup_bin) {
        // Already present — just reorder to put it first
        let filtered: Vec<&str> = existing
            .split(':')
            .filter(|p| *p != rustup_bin)
            .collect();
        format!("{}:{}", rustup_bin, filtered.join(":"))
    } else {
        format!("{}:{}", rustup_bin, existing)
    }
}

fn has_worktree_changes(project_root: &Path) -> Result<bool, String> {
    let out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git status: {}", e))?;
    Ok(!String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

fn git_diff_touches_md(
    project_root: &Path,
    from: &str,
    to: &str,
) -> Result<Vec<String>, String> {
    let out = Command::new("git")
        .args(["diff", "--name-only", &format!("{}..{}", from, to)])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git diff names: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git diff names: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.ends_with(".md"))
        .map(|l| l.to_string())
        .collect())
}

fn reset_working_tree_to(project_root: &Path, sha: &str) -> Result<(), String> {
    run_git(project_root, &["reset", "--hard", sha])
}

fn commit_with_message(project_root: &Path, subject: &str, body: &str) -> Result<(), String> {
    let full = if body.is_empty() {
        subject.to_string()
    } else {
        format!("{}\n\n{}", subject, body)
    };
    let out = Command::new("git")
        .args(["commit", "-m", &full])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git commit: {}", e))?;
    if !out.status.success() {
        return Err(format!("git commit: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(())
}

fn run_git(project_root: &Path, args: &[&str]) -> Result<(), String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git {:?}: {}", args, e))?;
    if !out.status.success() {
        return Err(format!("git {:?}: {}", args, String::from_utf8_lossy(&out.stderr)));
    }
    Ok(())
}

// The CLI no longer runs cargo commands. Everything below the commit-level
// (autofix pipeline, codegen, cross-compile verification) is delegated to the
// agent and enforced via post-hoc checks (tree clean, HEAD advanced, no .md).
