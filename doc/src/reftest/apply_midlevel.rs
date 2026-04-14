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
    /// Pre-analyzer model; defaults to the main model or Haiku if unset.
    pub analyzer_model: Option<String>,
    /// If true, skip the pre-analysis agent (faster, but no recommendation).
    pub skip_analyze: bool,
}

pub fn parse_args(args: &[&str], project_root: &Path) -> Result<Config, String> {
    let mut reference = None;
    let mut base = None;
    let mut model = None;
    let mut analyzer_model = None;
    let mut skip_analyze = false;

    for arg in args {
        if let Some(v) = arg.strip_prefix("--reference=") {
            reference = Some(v.to_string());
        } else if let Some(v) = arg.strip_prefix("--base=") {
            base = Some(v.to_string());
        } else if let Some(v) = arg.strip_prefix("--model=") {
            model = Some(v.to_string());
        } else if let Some(v) = arg.strip_prefix("--analyzer-model=") {
            analyzer_model = Some(v.to_string());
        } else if *arg == "--no-analyze" {
            skip_analyze = true;
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
        analyzer_model,
        skip_analyze,
    })
}

pub fn run(config: Config) -> Result<(), String> {
    let project_root = config.project_root.clone();

    // Stage the current azul-doc binary in a location that survives `cargo
    // clean`. The agent will invoke this via $AZUL_DOC_BIN instead of
    // `cargo run -p azul-doc -- ...`, which would trigger a ~45s rebuild
    // every time it runs `cargo clean` (and then has to re-run azul-doc).
    let azul_doc_bin = stage_binary(&project_root)?;
    println!("Staged azul-doc binary at: {}", azul_doc_bin.display());

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

        // Show the commit + overall progress
        let processed_so_far = progress.processed.len();
        print_commit_summary(
            &project_root,
            processed_so_far + 1,
            total,
            &progress,
            &info,
            paired_docs.as_ref(),
            &config.reference,
        );

        // Mark current and save — so Ctrl+C leaves a known-good pointer
        progress.current = Some(next.clone());
        save_progress(&progress_path, &progress)?;

        // ── Plan session (analyzer iterations) ──────────────────────────
        let mut plan = PlanSession { iterations: Vec::new() };

        // Initial analysis (no user feedback yet)
        if !config.skip_analyze {
            match run_analysis_agent(
                &project_root, &next, &info, paired_docs.as_ref(),
                &plan, None, &config,
            ) {
                Ok(output) => plan.iterations.push(PlanIteration {
                    user_feedback: None,
                    analyzer_output: output,
                }),
                Err(e) => {
                    println!("[warn] analysis agent failed (continuing without): {}", e);
                }
            }
        }

        // ── Decision / plan-refinement loop ─────────────────────────────
        // Stays in this loop until the user picks y/s/r/q. [p] just refines
        // and loops; [d] checks out + restores and loops.
        let decision_taken: UserAction = loop {
            let action = prompt_user()?;
            match action {
                UserAction::Refine(feedback) => {
                    match run_analysis_agent(
                        &project_root, &next, &info, paired_docs.as_ref(),
                        &plan, Some(&feedback), &config,
                    ) {
                        Ok(output) => plan.iterations.push(PlanIteration {
                            user_feedback: Some(feedback),
                            analyzer_output: output,
                        }),
                        Err(e) => println!("[warn] refinement failed: {}", e),
                    }
                    continue;
                }
                UserAction::Show => {
                    open_commit_in_editor(&project_root, &next, &info)?;
                    continue;
                }
                other => break other,
            }
        };

        match decision_taken {
            UserAction::Yes => {
                let pre_head = git_head(&project_root)?;
                let user_refinements: Vec<String> = plan.iterations.iter()
                    .filter_map(|it| it.user_feedback.clone())
                    .collect();
                let was_refined = !user_refinements.is_empty();

                // Apply + post-apply refine loop
                let final_outcome = loop {
                    let outcome = run_apply_agent(
                        &project_root, &next, &info,
                        paired_docs.as_ref(),
                        &plan,
                        &pre_head,
                        &config,
                    );
                    match outcome {
                        Ok(applied) => {
                            print_applied_summary(&project_root, &pre_head, &applied.new_sha)?;
                            match prompt_post_apply()? {
                                PostApply::Accept => break Ok(applied),
                                PostApply::Refine(instr) => {
                                    plan.iterations.push(PlanIteration {
                                        user_feedback: Some(instr),
                                        analyzer_output: String::new(),
                                    });
                                    continue;
                                }
                                PostApply::Revert => {
                                    run_git(&project_root, &["reset", "--hard", &pre_head])?;
                                    break Err("user reverted the commit".to_string());
                                }
                                PostApply::Quit => {
                                    println!("Quitting without advancing. Re-run to resume.");
                                    save_progress(&progress_path, &progress)?;
                                    return Ok(());
                                }
                            }
                        }
                        Err(e) => {
                            println!("\n[ERROR] agent apply failed: {}\n", e);
                            println!("Repository state left as-is. Resolve manually or quit.");
                            return Err(e);
                        }
                    }
                };

                match final_outcome {
                    Ok(Applied { new_sha }) => {
                        progress.current = None;
                        let notes = if user_refinements.is_empty() {
                            None
                        } else {
                            Some(user_refinements.join("\n---\n"))
                        };
                        progress.processed.push(Decision {
                            sha: next.clone(),
                            subject: info.subject.clone(),
                            decision: if was_refined {
                                DecisionKind::AppliedEdited
                            } else {
                                DecisionKind::AppliedByAgent
                            },
                            new_sha: Some(new_sha),
                            notes,
                        });
                        save_progress(&progress_path, &progress)?;
                        println!();
                    }
                    Err(reason) => {
                        progress.current = None;
                        progress.processed.push(Decision {
                            sha: next.clone(),
                            subject: info.subject.clone(),
                            decision: DecisionKind::Rejected,
                            new_sha: None,
                            notes: Some(reason),
                        });
                        save_progress(&progress_path, &progress)?;
                        println!();
                    }
                }
            }
            UserAction::Skip(notes) => {
                progress.current = None;
                progress.processed.push(Decision {
                    sha: next.clone(),
                    subject: info.subject.clone(),
                    decision: DecisionKind::SkippedByUser,
                    new_sha: None,
                    notes,
                });
                save_progress(&progress_path, &progress)?;
                println!();
            }
            UserAction::Reject(notes) => {
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
            UserAction::Quit => {
                println!("Saving progress and exiting.");
                save_progress(&progress_path, &progress)?;
                break;
            }
            UserAction::Refine(_) | UserAction::Show => unreachable!(),
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
    /// User chose to skip — we'll revisit later.
    SkippedByUser,
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

    // Heuristic: any token in the subject that looks like a source-file path
    let source_exts = [".rs", ".toml", ".json", ".yaml", ".yml", ".h", ".hpp", ".c", ".cpp", ".py"];
    let subject_source_path = subject
        .split(|c: char| c.is_whitespace() || c == ',' || c == ':')
        .find(|tok| source_exts.iter().any(|ext| tok.ends_with(ext)))
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
    project_root: &Path,
    n: usize,
    total: usize,
    progress: &Progress,
    info: &CommitInfo,
    paired: Option<&CommitInfo>,
    reference: &str,
) {
    let applied = progress.processed.iter().filter(|d| matches!(
        d.decision,
        DecisionKind::AppliedByAgent | DecisionKind::AppliedEdited
    )).count();
    let rejected = progress.processed.iter().filter(|d| d.decision == DecisionKind::Rejected).count();
    let skipped  = progress.processed.iter().filter(|d| matches!(
        d.decision,
        DecisionKind::SkippedMd | DecisionKind::SkippedByUser
    )).count();
    let remaining = total.saturating_sub(progress.processed.len()).saturating_sub(1);

    let branch = git_current_branch(project_root).unwrap_or_else(|_| "?".into());
    let head   = git_head(project_root).unwrap_or_else(|_| "?".into());

    println!("════════════════════════════════════════════════════════════════════════");
    println!("  Reference: {}  →  commit {}/{}", reference, n, total);
    println!("  Replaying onto: branch {} @ {}", branch, short(&head));
    println!("  Progress: applied={}  rejected={}  skipped={}  remaining={}", applied, rejected, skipped, remaining);
    println!("────────────────────────────────────────────────────────────────────────");
    println!("  Next SHA: {}", &info.sha[..12]);
    println!("  Subject:  {}", info.subject);
    if !info.body.is_empty() {
        for line in info.body.lines() {
            println!("    > {}", line);
        }
    }
    println!();
    println!("  Files ({}):", info.files.len());
    for f in &info.files {
        println!("    +{:<4} -{:<4}  {}", f.additions, f.deletions, f.path);
    }
    if let Some(p) = paired {
        println!("\n  Paired docs commit: {}  {}", &p.sha[..12], p.subject);
    }
    println!("════════════════════════════════════════════════════════════════════════");
}

/// Checkout the reference commit so an already-open editor (VSCode etc.)
/// auto-refreshes its git view to that commit's state. Blocks on Enter, then
/// restores the original branch.
fn open_commit_in_editor(
    project_root: &Path,
    sha: &str,
    info: &CommitInfo,
) -> Result<(), String> {
    let branch = git_current_branch(project_root)?;
    if branch == "HEAD" {
        return Err("already on a detached HEAD; resolve that first".into());
    }

    if has_worktree_changes(project_root)? || !index_is_empty(project_root)? {
        return Err("working tree not clean; cannot checkout for inspection".into());
    }

    println!();
    println!("  Checking out {} (detached HEAD) …", &sha[..12]);
    run_git(project_root, &["checkout", "--detach", sha])?;

    println!();
    println!("  ┌─ inspecting commit {} ({}) ─────────────────────", &sha[..12], info.subject);
    println!("  │ Switch to your editor (VSCode etc.) — its git view should have");
    println!("  │ auto-refreshed to this commit's state. Poke around.");
    println!("  │ Press Enter to restore branch `{}` and return to the prompt.", branch);
    println!("  └──────────────────────────────────────────────────────────────────");
    let _ = read_line();

    println!("  Restoring branch {} …", branch);
    run_git(project_root, &["checkout", &branch])?;
    println!();
    Ok(())
}

fn git_current_branch(project_root: &Path) -> Result<String, String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git branch: {}", e))?;
    if !out.status.success() {
        return Err(format!("git branch: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

enum UserAction {
    /// Apply the commit using the accumulated plan (analyzer output + user
    /// refinements). Spawns the apply-agent.
    Yes,
    /// Don't apply — "I'll revisit later", record as skipped.
    Skip(Option<String>),
    /// Definitely don't apply — record as rejected with a reason.
    Reject(Option<String>),
    /// Refine the plan: feed this extra feedback to the analyzer (the apply
    /// agent is NOT invoked yet). The analyzer incorporates your note into
    /// its existing plan and re-prints it.
    Refine(String),
    /// Checkout the reference commit so the user's editor shows its state.
    Show,
    /// Save progress and exit.
    Quit,
}

enum PostApply {
    /// The commit looks good — advance to the next one.
    Accept,
    /// Give the agent another round of instructions. The previous commit is
    /// collapsed and the refinement is squashed into a single commit.
    Refine(String),
    /// Revert this commit entirely (hard reset to pre-apply HEAD).
    Revert,
    /// Save progress and exit without advancing.
    Quit,
}

fn prompt_post_apply() -> Result<PostApply, String> {
    println!();
    print!("Accept this commit? [y]es / [e]dit-further / [r]evert / [q]uit: ");
    io::stdout().flush().ok();
    let line = read_line()?;
    let c = line.trim().chars().next().unwrap_or(' ');
    match c {
        'y' | 'Y' => Ok(PostApply::Accept),
        'e' | 'E' => {
            println!("  Additional instructions for the agent (end with '.' on its own line):");
            let instr = read_multiline_until_dot()?;
            Ok(PostApply::Refine(instr))
        }
        'r' | 'R' => Ok(PostApply::Revert),
        'q' | 'Q' => Ok(PostApply::Quit),
        _ => {
            println!("  (unrecognised — treating as edit)");
            println!("  Additional instructions for the agent (end with '.' on its own line):");
            let instr = read_multiline_until_dot()?;
            Ok(PostApply::Refine(instr))
        }
    }
}

fn print_applied_summary(
    project_root: &Path,
    pre_head: &str,
    new_head: &str,
) -> Result<(), String> {
    println!("\n═══ applied ═══════════════════════════════════════════════════════════");
    // Show the diffstat of the new commit-range
    let out = Command::new("git")
        .args(["diff", "--stat", &format!("{}..{}", pre_head, new_head)])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git diff --stat: {}", e))?;
    print!("{}", String::from_utf8_lossy(&out.stdout));

    // Show the new commit's subject
    let out = Command::new("git")
        .args(["log", "--format=%h %s", &format!("{}..{}", pre_head, new_head)])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git log: {}", e))?;
    println!("\nCommit(s):");
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        println!("  {}", line);
    }
    Ok(())
}

fn prompt_user() -> Result<UserAction, String> {
    println!("Decision?");
    println!("  [y] yes    — apply using the current plan");
    println!("  [p] plan   — refine the plan: add feedback, analyzer revises");
    println!("  [s] skip   — don't apply now, come back later");
    println!("  [r] reject — don't apply, record as rejected with reason");
    println!("  [d] diff   — checkout commit so your editor shows its state");
    println!("  [q] quit");
    print!("> ");
    io::stdout().flush().ok();

    let line = read_line()?;
    let c = line.trim().chars().next().unwrap_or(' ');

    match c {
        'y' | 'Y' => Ok(UserAction::Yes),
        'p' | 'P' => {
            println!("  Feedback for the analyzer (end with a single '.' on its own line):");
            let fb = read_multiline_until_dot()?;
            Ok(UserAction::Refine(fb))
        }
        's' | 'S' => {
            println!("  Reason (one line, empty to skip prompt):");
            let notes = read_line()?.trim().to_string();
            Ok(UserAction::Skip(if notes.is_empty() { None } else { Some(notes) }))
        }
        'r' | 'R' => {
            println!("  Reason for rejecting (one line, empty to skip prompt):");
            let notes = read_line()?.trim().to_string();
            Ok(UserAction::Reject(if notes.is_empty() { None } else { Some(notes) }))
        }
        'd' | 'D' => Ok(UserAction::Show),
        'q' | 'Q' => Ok(UserAction::Quit),
        _ => {
            println!("  (unrecognised — please pick one of y/p/s/r/d/q)");
            prompt_user()
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

// ── Plan session (accumulated analyzer output + user feedback) ────────────

struct PlanSession {
    iterations: Vec<PlanIteration>,
}

struct PlanIteration {
    /// None for the initial pass; Some(text) when the user supplied feedback
    /// that the analyzer should incorporate.
    user_feedback: Option<String>,
    /// Analyzer's output for this iteration. Empty string when this entry
    /// represents post-apply user feedback that the apply-agent should heed
    /// (no analyzer run happens in that case).
    analyzer_output: String,
}

impl PlanSession {
    fn latest_plan(&self) -> Option<&str> {
        self.iterations.iter()
            .rev()
            .find(|it| !it.analyzer_output.is_empty())
            .map(|it| it.analyzer_output.as_str())
    }

    fn all_feedback(&self) -> Vec<&str> {
        self.iterations.iter()
            .filter_map(|it| it.user_feedback.as_deref())
            .collect()
    }
}

// ── Pre-analysis agent ────────────────────────────────────────────────────

/// Spawn a read-only Claude agent that looks at the commit diff + paired
/// review report + current codebase and recommends an action. If `plan` has
/// previous iterations, they are passed as context so the analyzer REFINES
/// its prior plan rather than starting over.
///
/// Output is tee'd to the terminal AND captured to a string, which is
/// returned for storage in the plan session.
fn run_analysis_agent(
    project_root: &Path,
    sha: &str,
    info: &CommitInfo,
    paired: Option<&CommitInfo>,
    plan: &PlanSession,
    new_feedback: Option<&str>,
    config: &Config,
) -> Result<String, String> {
    let is_refinement = !plan.iterations.is_empty();
    let prompt = build_analysis_prompt(project_root, sha, info, paired, plan, new_feedback)?;

    let agent_dir = project_root.join("doc/target/autoreview/apply-midlevel/agent-prompts");
    fs::create_dir_all(&agent_dir).ok();
    let iter_idx = plan.iterations.len();
    let prompt_path = agent_dir.join(format!("{}.analysis.{}.md", short(sha), iter_idx));
    fs::write(&prompt_path, &prompt).ok();

    println!();
    if is_refinement {
        println!("┌── analyzer refining plan (iter #{}) ───────────────────────────────", iter_idx + 1);
    } else {
        println!("┌── analyzer: initial plan ─────────────────────────────────────────");
    }

    let model = config
        .analyzer_model
        .as_deref()
        .or(config.model.as_deref())
        .unwrap_or("opus");

    let path_with_rustup = rustup_prefixed_path();
    let session_uuid = make_session_uuid(sha, &format!("a{}", iter_idx));
    let session_name = format!("analyzer {}.{}", short(sha), iter_idx);

    let cmd_args: Vec<&str> = vec![
        "-p",
        "--dangerously-skip-permissions",
        "--verbose",
        "--output-format", "stream-json",
        "--include-partial-messages",
        "--session-id", &session_uuid,
        "-n", &session_name,
        "--disallowedTools", "mcp__*",
        "--disallowedTools", "Edit",
        "--disallowedTools", "Write",
        "--disallowedTools", "NotebookEdit",
        "--disallowedTools", "Bash(git commit*)",
        "--disallowedTools", "Bash(git cherry-pick*)",
        "--disallowedTools", "Bash(git reset*)",
        "--disallowedTools", "Bash(cargo*)",
        "--model", model,
    ];

    println!("  session-id: {}", session_uuid);
    println!("  attach with: claude --resume {}", session_uuid);
    println!();

    let out = spawn_claude_streaming(
        &cmd_args, prompt.as_bytes(), project_root, &path_with_rustup, None,
    )?;
    println!("└───────────────────────────────────────────────────────────────────────");
    Ok(out.text)
}

// ── Streaming helpers ─────────────────────────────────────────────────────

struct StreamOutput {
    /// Accumulated assistant text content (captured from `assistant` events
    /// after being printed live as `stream_event` deltas).
    text: String,
    #[allow(dead_code)]
    session_id: Option<String>,
}

/// Spawn `claude` with the given args in stream-json mode, pipe the prompt
/// into stdin, and stream the output live to stdout while capturing text.
///
/// `extra_env` allows callers to pass additional env vars (like `AZUL_DOC_BIN`).
fn spawn_claude_streaming(
    args: &[&str],
    stdin_bytes: &[u8],
    cwd: &Path,
    path_env: &str,
    extra_env: Option<&[(&str, &Path)]>,
) -> Result<StreamOutput, String> {
    let mut cmd = Command::new("claude");
    cmd.args(args)
        .env_remove("CLAUDECODE")
        .env("PATH", path_env)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    if let Some(env_pairs) = extra_env {
        for (k, v) in env_pairs {
            cmd.env(k, v);
        }
    }

    let mut child = cmd.spawn().map_err(|e| format!("spawn claude: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(stdin_bytes)
            .map_err(|e| format!("write prompt stdin: {}", e))?;
        drop(stdin);
    }

    let stdout = child.stdout.take()
        .ok_or_else(|| "failed to grab child stdout".to_string())?;
    let handle = std::thread::spawn(move || process_stream_events(stdout));

    let status = child.wait().map_err(|e| format!("wait claude: {}", e))?;
    let out = handle.join().map_err(|_| "stream thread panicked".to_string())?;
    if !status.success() {
        return Err(format!("claude exited with status {}", status));
    }
    Ok(out)
}

fn process_stream_events<R: std::io::Read + Send + 'static>(stdout: R) -> StreamOutput {
    use std::io::BufRead;
    let reader = std::io::BufReader::new(stdout);
    let mut text = String::new();
    let mut session_id: Option<String> = None;

    for line in reader.lines() {
        let line = match line { Ok(l) => l, Err(_) => break };
        if line.trim().is_empty() { continue; }

        let v: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                // Non-JSON — print as-is
                println!("{}", line);
                continue;
            }
        };

        if session_id.is_none() {
            if let Some(sid) = v.get("session_id").and_then(|s| s.as_str()) {
                session_id = Some(sid.to_string());
            }
        }

        match v.get("type").and_then(|t| t.as_str()) {
            // Live token-by-token text deltas + tool-call announcements.
            Some("stream_event") => {
                let event = match v.get("event") {
                    Some(e) => e, None => continue,
                };
                match event.get("type").and_then(|t| t.as_str()) {
                    Some("content_block_delta") => {
                        if let Some(t) = event.pointer("/delta/text").and_then(|s| s.as_str()) {
                            print!("{}", t);
                            io::stdout().flush().ok();
                        }
                    }
                    Some("content_block_start") => {
                        if let Some(block) = event.get("content_block") {
                            if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                                let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                                println!("\n  ⚙ [{}]", name);
                            }
                        }
                    }
                    _ => {}
                }
            }
            // Final assistant message — capture text (already printed as deltas)
            // + surface tool-call input summaries (these arrive intact here).
            Some("assistant") => {
                if let Some(content) = v.pointer("/message/content").and_then(|c| c.as_array()) {
                    for block in content {
                        match block.get("type").and_then(|t| t.as_str()) {
                            Some("text") => {
                                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                                    text.push_str(t);
                                }
                            }
                            Some("tool_use") => {
                                let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                                let summary = summarize_tool_input(name, block.get("input"));
                                if !summary.is_empty() {
                                    println!("    {}", summary);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Some("result") => {
                println!();
            }
            _ => {}
        }
    }
    StreamOutput { text, session_id }
}

fn summarize_tool_input(name: &str, input: Option<&serde_json::Value>) -> String {
    let Some(i) = input else { return String::new() };
    match name {
        "Read" | "Edit" | "Write" | "NotebookEdit" => i.get("file_path")
            .and_then(|v| v.as_str()).unwrap_or("").into(),
        "Bash" => {
            let cmd = i.get("command").and_then(|v| v.as_str()).unwrap_or("");
            cmd.chars().take(120).collect()
        }
        "Grep" => {
            let pat = i.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            let path = i.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            format!("{:?} in {}", pat, path)
        }
        "Glob" => i.get("pattern").and_then(|v| v.as_str()).unwrap_or("").into(),
        _ => String::new(),
    }
}

/// Build a deterministic UUID from a git SHA + a per-call suffix so re-runs
/// (analyzer iterations, apply retries) get predictable session IDs the user
/// can copy-paste into `claude --resume`.
fn make_session_uuid(sha: &str, suffix: &str) -> String {
    let base: String = sha.chars().filter(|c| c.is_ascii_hexdigit()).take(32).collect();
    let padded: String = if base.len() < 32 {
        let mut b = base.clone();
        while b.len() < 32 { b.push('0'); }
        b
    } else {
        base
    };
    let mut hash: u32 = 0;
    for c in suffix.bytes() { hash = hash.wrapping_mul(31).wrapping_add(c as u32); }
    let suffix_hex = format!("{:02x}", (hash & 0xff) as u8);
    let mut out: String = padded.chars().take(30).collect();
    out.push_str(&suffix_hex);
    format!("{}-{}-{}-{}-{}",
        &out[0..8], &out[8..12], &out[12..16], &out[16..20], &out[20..32])
}

fn build_analysis_prompt(
    project_root: &Path,
    sha: &str,
    info: &CommitInfo,
    paired: Option<&CommitInfo>,
    plan: &PlanSession,
    new_feedback: Option<&str>,
) -> Result<String, String> {
    let code_diff = git_show_diff(project_root, sha)?;
    let docs_diff = match paired {
        Some(p) => git_show_diff(project_root, &p.sha)?,
        None => String::new(),
    };

    let mut p = String::new();
    let is_refinement = !plan.iterations.is_empty();

    if is_refinement {
        p.push_str("You are REFINING an existing plan for a single commit from an\n");
        p.push_str("automated mid-level code review. The user has reviewed your previous\n");
        p.push_str("recommendation and given feedback. Your job is to incorporate that\n");
        p.push_str("feedback and output a REVISED plan. DO NOT redo the entire analysis\n");
        p.push_str("from scratch — build on the previous iteration.\n\n");
    } else {
        p.push_str("You are analyzing ONE commit from an automated mid-level code review ");
        p.push_str("before the user decides what to do with it. DO NOT modify any files. ");
        p.push_str("Your job is to give the user a short recommendation.\n\n");

        p.push_str("The previous review-agent pass was told to find and fix mid-level\n");
        p.push_str("issues (dead code, duplication, unwired APIs, small refactors). It\n");
        p.push_str("sometimes DELETED code that looked unused but was actually meant to\n");
        p.push_str("be part of the public API — it just wasn't wired in `api.json` yet.\n");
        p.push_str("Your analysis should catch those cases.\n\n");
    }

    p.push_str("Classify the commit into one of these categories:\n\n");
    p.push_str("  [KEEP]     Clean, correct fix — apply as-is.\n");
    p.push_str("  [WIRE]     Deletes a type/field/fn that should be WIRED into the\n");
    p.push_str("             public API (api.json) instead of removed.\n");
    p.push_str("  [REFACTOR] Intent right, execution off — needs a custom instruction.\n");
    p.push_str("  [REJECT]   Incorrect or harmful — do not apply.\n");
    p.push_str("  [UNCLEAR]  Can't tell — user should inspect.\n\n");

    if !is_refinement {
        p.push_str("How to analyze (first pass):\n");
        p.push_str("  1. Read the commit diff below.\n");
        p.push_str("  2. Read the paired review report (context for WHY).\n");
        p.push_str("  3. For each deletion, use Grep/Read to check whether the deleted\n");
        p.push_str("     name is referenced in the current tree. Check api.json too.\n");
        p.push_str("  4. Output a SHORT recommendation (≤ 10 lines):\n\n");
        p.push_str("       [CATEGORY] <one sentence>\n");
        p.push_str("       Why: <1-2 sentences>\n");
        p.push_str("       Plan: <bulleted steps the apply-agent should follow>\n");
        p.push_str("       Suggested user action: <y / p with feedback / s / r>\n\n");
    } else {
        p.push_str("How to refine:\n");
        p.push_str("  1. Re-read the user's feedback (below).\n");
        p.push_str("  2. Adjust ONLY the parts of the previous plan that the feedback\n");
        p.push_str("     touches. Leave the rest intact.\n");
        p.push_str("  3. Output the FULL revised plan in the same format as before:\n\n");
        p.push_str("       [CATEGORY] <one sentence>\n");
        p.push_str("       Why: <1-2 sentences, updated if needed>\n");
        p.push_str("       Plan: <bulleted steps, refined>\n");
        p.push_str("       Change since last iteration: <one sentence summarising what's new>\n");
        p.push_str("       Suggested user action: <y / p with feedback / s / r>\n\n");
    }
    p.push_str("Be terse. The user is running through 335 more commits.\n\n");

    p.push_str(&format!("Original commit SHA: {}\n", sha));
    p.push_str(&format!("Original subject: {}\n", info.subject));
    if !info.body.is_empty() {
        p.push_str(&format!("Original body:\n{}\n", info.body));
    }

    // Previous iterations (if any)
    for (i, it) in plan.iterations.iter().enumerate() {
        p.push_str(&format!("\n=== PREVIOUS ITERATION {} ===\n", i + 1));
        if let Some(fb) = &it.user_feedback {
            p.push_str("User feedback that led to this iteration:\n");
            p.push_str(fb);
            p.push_str("\n\n");
        }
        if !it.analyzer_output.is_empty() {
            p.push_str("Your plan at that point:\n");
            p.push_str(&it.analyzer_output);
            p.push_str("\n");
        }
        p.push_str(&format!("=== END ITERATION {} ===\n", i + 1));
    }

    if let Some(fb) = new_feedback {
        p.push_str("\n=== NEW USER FEEDBACK (this iteration) ===\n");
        p.push_str(fb);
        p.push_str("\n=== END NEW FEEDBACK ===\n");
    }

    p.push_str("\n=== COMMIT DIFF ===\n");
    p.push_str(&code_diff);
    p.push_str("\n=== END COMMIT DIFF ===\n");

    if !docs_diff.is_empty() {
        p.push_str("\n=== PAIRED REVIEW REPORT ===\n");
        p.push_str(&docs_diff);
        p.push_str("\n=== END REPORT ===\n");
    }

    Ok(p)
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
///
/// `pre_head` is the SHA BEFORE the very first apply attempt for this commit.
/// If the agent is refining a previously-applied commit (HEAD != pre_head when
/// called), we soft-reset HEAD to pre_head after the agent finishes and
/// re-commit the combined staged changes as a single commit. This keeps one
/// commit per reference-commit in the replayed history.
fn run_apply_agent(
    project_root: &Path,
    sha: &str,
    info: &CommitInfo,
    paired: Option<&CommitInfo>,
    plan: &PlanSession,
    pre_head: &str,
    config: &Config,
) -> Result<Applied, String> {
    let is_refinement = git_head(project_root)? != pre_head;
    let user_instruction = if plan.all_feedback().is_empty() {
        None
    } else {
        Some(plan.all_feedback().join("\n---\n"))
    };
    let user_instruction = user_instruction.as_deref();

    // Refuse to start if the tree is dirty — the agent needs a clean slate.
    let dirty = !index_is_empty(project_root)? || has_worktree_changes(project_root)?;
    if dirty {
        return Err(
            "working tree / index not clean. Commit or stash before continuing.".into(),
        );
    }

    let prompt = build_agent_prompt(project_root, sha, info, paired, user_instruction, plan)?;

    // Persist the prompt for auditing. Agent output is streamed to the user's
    // terminal directly (so they can watch progress and see the session ID the
    // claude CLI prints). If they want a persistent log they can pipe the
    // whole invocation through `tee`.
    let agent_dir = project_root.join("doc/target/autoreview/apply-midlevel/agent-prompts");
    fs::create_dir_all(&agent_dir).ok();
    let prompt_path = agent_dir.join(format!("{}.md", short(sha)));
    fs::write(&prompt_path, &prompt).ok();

    println!();
    println!("╔══ spawning claude agent ══════════════════════════════════════════════");
    println!("║  prompt       : {}", prompt_path.display());
    println!("║  target SHA   : {}  {}", short(sha), info.subject);
    println!("║  to attach    : look for 'session_id' in the output below, then");
    println!("║                 claude --resume <id>");
    println!("║  to abort     : Ctrl+C — progress is saved, this commit re-runs");
    println!("╚═══════════════════════════════════════════════════════════════════════");
    println!();

    // rust-analyzer-lsp is ALLOWED here — this agent runs sequentially and
    // benefits from the type info. We only block MCP tools leaking from user
    // config.
    // Default to opus. User can override via --model=<x>.
    let model = config.model.as_deref().unwrap_or("opus");
    let session_uuid = make_session_uuid(sha, &format!("b{}", plan.iterations.len()));
    let session_name = format!("apply {}", short(sha));
    let cmd_args: Vec<&str> = vec![
        "-p",
        "--dangerously-skip-permissions",
        "--verbose",
        "--output-format", "stream-json",
        "--include-partial-messages",
        "--session-id", &session_uuid,
        "-n", &session_name,
        "--disallowedTools", "mcp__*",
        "--model", model,
    ];

    // Ensure the agent uses the rustup toolchain (which has all cross-compile
    // targets installed) rather than Homebrew's cargo, which doesn't.
    let path_with_rustup = rustup_prefixed_path();

    // Use the staged azul-doc binary (copied outside target/ at startup, so
    // `cargo clean` in the agent doesn't wipe it).
    let azul_doc_bin = project_root.join(
        if cfg!(windows) { ".apply-midlevel/azul-doc.exe" } else { ".apply-midlevel/azul-doc" }
    );

    println!("  session-id: {}", session_uuid);
    println!("  attach with: claude --resume {}", session_uuid);
    println!();

    let _out = spawn_claude_streaming(
        &cmd_args, prompt.as_bytes(), project_root, &path_with_rustup,
        Some(&[("AZUL_DOC_BIN", azul_doc_bin.as_path())]),
    )?;
    println!("────────────────────────────────────────────────────────────────────────");

    // Verify tree is clean.
    let post_head = git_head(project_root)?;
    if has_worktree_changes(project_root)? || !index_is_empty(project_root)? {
        return Err("agent left working tree dirty after committing".into());
    }

    if post_head == pre_head {
        return Err("agent made no commits".into());
    }

    // If the agent made more than one commit, or we're refining (pre_head is
    // the original baseline and HEAD already had a commit from a prior round),
    // collapse everything on top of pre_head into a single squashed commit.
    let commit_count = count_commits(project_root, pre_head, &post_head)?;
    let final_head = if commit_count > 1 || is_refinement {
        squash_to_one_commit(project_root, pre_head, info, user_instruction)?
    } else {
        post_head.clone()
    };

    // Check the squashed (or single) commit doesn't touch .md
    let md_hits = git_diff_touches_md(project_root, pre_head, &final_head)?;
    if !md_hits.is_empty() {
        return Err(format!("agent committed .md files: {}", md_hits.join(", ")));
    }

    Ok(Applied { new_sha: final_head })
}

fn count_commits(project_root: &Path, from: &str, to: &str) -> Result<usize, String> {
    let out = Command::new("git")
        .args(["rev-list", "--count", &format!("{}..{}", from, to)])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git rev-list: {}", e))?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<usize>()
        .map_err(|e| format!("parse commit count: {}", e))
}

/// Soft-reset HEAD to `base` so all changes since `base` are staged, drop
/// `.md` files, and re-commit as a single commit with a clean message derived
/// from the reference commit's subject/body plus an optional note about the
/// user's instructions.
fn squash_to_one_commit(
    project_root: &Path,
    base: &str,
    info: &CommitInfo,
    user_instruction: Option<&str>,
) -> Result<String, String> {
    run_git(project_root, &["reset", "--soft", base])?;
    drop_md_changes(project_root)?;

    if index_is_empty(project_root)? {
        // Everything was .md — undo
        reset_working_tree_to(project_root, base)?;
        return Err("after collapsing and dropping .md, nothing remains to commit".into());
    }

    let subject = info.subject.clone();
    let mut body = info.body.clone();
    if !body.is_empty() { body.push_str("\n\n"); }
    body.push_str(&format!("Replayed from {}", &info.sha[..12]));
    if user_instruction.is_some() {
        body.push_str(" (with user refinements).");
    } else {
        body.push_str(".");
    }

    commit_with_message(project_root, &subject, &body)?;
    git_head(project_root)
}

fn build_agent_prompt(
    project_root: &Path,
    sha: &str,
    info: &CommitInfo,
    paired: Option<&CommitInfo>,
    user_instruction: Option<&str>,
    plan: &PlanSession,
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
        p.push_str("This is either a user-directed override or a REFINEMENT pass.\n");
        p.push_str("The user has reviewed a previous attempt at this commit and wants\n");
        p.push_str("something different. Check `git log -1 --format=%s%n%b HEAD` — if HEAD\n");
        p.push_str("already looks like the commit you're replaying, this is a refinement:\n");
        p.push_str("apply the USER INSTRUCTION as an ADDITIONAL edit on top of HEAD, then\n");
        p.push_str("make a new commit (the caller will squash it into one).\n\n");
        p.push_str("If HEAD has not yet been replayed, IGNORE the original commit's diff\n");
        p.push_str("as a literal instruction and follow the USER INSTRUCTION below. Use\n");
        p.push_str("the original diff and paired report only as reference context.\n");
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
    p.push_str("IMPORTANT: the azul-doc binary you need for these steps is already built\n");
    p.push_str("and available as `$AZUL_DOC_BIN` in your environment. Invoke it directly:\n\n");
    p.push_str("    \"$AZUL_DOC_BIN\" autofix\n\n");
    p.push_str("NOT `cargo run -r -p azul-doc -- autofix` — the cargo form would rebuild\n");
    p.push_str("azul-doc after any `cargo clean` you do. `$AZUL_DOC_BIN` is the exact\n");
    p.push_str("release binary of the CLI that spawned you, so it's guaranteed current.\n\n");
    p.push_str("  (a) Run autofix + apply in a LOOP until autofix reports\n");
    p.push_str("      `Generated 0 patches`. One pass is not enough — applying patches\n");
    p.push_str("      can surface new inconsistencies that autofix then needs to fix too.\n\n");
    p.push_str("      Loop:\n");
    p.push_str("          \"$AZUL_DOC_BIN\" autofix\n");
    p.push_str("          # if the above printed `Generated 0 patches` → break out of loop\n");
    p.push_str("          \"$AZUL_DOC_BIN\" patch safe target/autofix/patches\n");
    p.push_str("          \"$AZUL_DOC_BIN\" patch      target/autofix/patches\n");
    p.push_str("          # loop back to `autofix`\n\n");
    p.push_str("      Only proceed once autofix produces zero patches.\n\n");
    p.push_str("  (b) Then run the normalize + codegen pass ONCE:\n\n");
    p.push_str("          \"$AZUL_DOC_BIN\" normalize\n");
    p.push_str("          \"$AZUL_DOC_BIN\" codegen all\n\n");
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

    // Agreed plan — if the analyzer ran and the user approved (possibly after
    // refinements), this is the source of truth for what to do. The commit
    // diff below is now REFERENCE CONTEXT, not a literal recipe.
    if let Some(latest) = plan.latest_plan() {
        p.push_str("\n=== AGREED PLAN (source of truth — follow this) ===\n");
        p.push_str(latest);
        p.push_str("\n=== END AGREED PLAN ===\n");
    }

    if let Some(instr) = user_instruction {
        p.push_str("\n=== USER FEEDBACK HISTORY (already folded into the plan above) ===\n");
        p.push_str(instr);
        p.push_str("\n=== END FEEDBACK HISTORY ===\n");
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

/// Copy the currently-running `azul-doc` binary to a location that survives
/// `cargo clean`, and return the stable path. The copy happens once at
/// startup — repeat runs will overwrite it with whatever version of azul-doc
/// is currently launching the tool.
fn stage_binary(project_root: &Path) -> Result<PathBuf, String> {
    let src = std::env::current_exe()
        .map_err(|e| format!("current_exe: {}", e))?;
    // Use project_root/.apply-midlevel/ which is outside any cargo target dir.
    let dest_dir = project_root.join(".apply-midlevel");
    fs::create_dir_all(&dest_dir)
        .map_err(|e| format!("mkdir .apply-midlevel: {}", e))?;
    let dest = dest_dir.join(if cfg!(windows) { "azul-doc.exe" } else { "azul-doc" });

    // Don't re-copy if it's already identical (avoid invalidating ETag, etc.)
    let src_meta = fs::metadata(&src).map_err(|e| format!("stat src: {}", e))?;
    let should_copy = match fs::metadata(&dest) {
        Ok(dst_meta) => dst_meta.len() != src_meta.len(),
        Err(_) => true,
    };
    if should_copy {
        fs::copy(&src, &dest).map_err(|e| format!("copy azul-doc: {}", e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dest)
                .map_err(|e| format!("stat dest: {}", e))?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&dest, perms)
                .map_err(|e| format!("chmod: {}", e))?;
        }
    }
    Ok(dest)
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
