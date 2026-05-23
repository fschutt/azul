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
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::telegram::{InputChannel, TelegramBridge, UserInput};

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
    /// If true, do not mirror prompts to Telegram even if a config exists.
    pub no_telegram: bool,
    /// Triage mode: run the analyzer + collect a decision per commit, but DO
    /// NOT spawn the apply agent. `apply` / `apply-with-edits` decisions are
    /// queued to `progress.pending`; the next normal `apply-midlevel` run
    /// will consume them unattended.
    pub triage_only: bool,
    /// Apply only commits that already have a pre-decided pending entry,
    /// and exit when none remain. Skips un-triaged commits entirely instead
    /// of prompting — lets a batch of pre-decisions run unattended to
    /// completion without falling through to interactive review.
    pub pending_only: bool,
    /// Stop the run after this many commits have been decided/applied in
    /// the current session. Lets you triage 5–10 commits, break, come back.
    /// `None` = no limit (run until all commits processed or user quits).
    /// Pure-`.md` auto-skips do not count toward the limit.
    pub limit: Option<usize>,
    /// Refresh-pending mode: walk `progress.pending` entries created before
    /// the `iterations` field existed (legacy entries that only have a
    /// `comment`), replay the analyzer with the saved feedback, and write
    /// the full iteration trace back into the entry. Entries that already
    /// have iterations are skipped. No apply agent runs in this mode.
    pub refresh_pending: bool,
    /// Max attempts per pending commit when running unattended. The first
    /// attempt counts as #1; default is 3 (so up to 2 retries). Transient
    /// errors (concurrent build, file-lock contention with other azul-doc
    /// processes) sleep 60s before retry; real errors sleep 5s and inject
    /// the failure into the agent's feedback for the next attempt.
    pub retries: u32,
}

pub fn parse_args(args: &[&str], project_root: &Path) -> Result<Config, String> {
    let mut reference = None;
    let mut base = None;
    let mut model = None;
    let mut analyzer_model = None;
    let mut skip_analyze = false;
    let mut no_telegram = false;
    let mut triage_only = false;
    let mut pending_only = false;
    let mut refresh_pending = false;
    let mut limit: Option<usize> = None;
    let mut retries: u32 = 3;

    for arg in args {
        if let Some(v) = arg.strip_prefix("--reference=") {
            reference = Some(v.to_string());
        } else if let Some(v) = arg.strip_prefix("--base=") {
            base = Some(v.to_string());
        } else if let Some(v) = arg.strip_prefix("--model=") {
            model = Some(v.to_string());
        } else if let Some(v) = arg.strip_prefix("--analyzer-model=") {
            analyzer_model = Some(v.to_string());
        } else if let Some(v) = arg.strip_prefix("--limit=") {
            limit = Some(v.parse::<usize>()
                .map_err(|_| format!("--limit must be a non-negative integer, got: {}", v))?);
        } else if let Some(v) = arg.strip_prefix("--retries=") {
            retries = v.parse::<u32>()
                .map_err(|_| format!("--retries must be a non-negative integer, got: {}", v))?
                .max(1);
        } else if *arg == "--no-analyze" {
            skip_analyze = true;
        } else if *arg == "--no-telegram" {
            no_telegram = true;
        } else if *arg == "--triage" || *arg == "--triage-only" {
            triage_only = true;
        } else if *arg == "--pending-only" || *arg == "--auto" {
            pending_only = true;
        } else if *arg == "--refresh-pending" {
            refresh_pending = true;
        } else if arg.starts_with('-') {
            return Err(format!("Unknown option: {}", arg));
        }
    }

    let mode_count =
        (triage_only as u8) + (pending_only as u8) + (refresh_pending as u8);
    if mode_count > 1 {
        return Err("--triage, --pending-only, and --refresh-pending are mutually \
                    exclusive: they walk different subsets of the commit list."
            .to_string());
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
        no_telegram,
        triage_only,
        pending_only,
        limit,
        refresh_pending,
        retries,
    })
}

pub fn run(config: Config) -> Result<(), String> {
    let project_root = config.project_root.clone();

    // Stage the current azul-doc binary in a location that survives `cargo
    // clean`. The agent will invoke this via $AZ_DOC_BIN instead of
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

    if config.refresh_pending {
        return run_refresh_pending(&project_root, &mut progress, &progress_path, &commits, &config);
    }

    let mode_label = if config.triage_only {
        "TRIAGE"
    } else if config.pending_only {
        "PENDING-ONLY"
    } else {
        "APPLY"
    };
    println!("Reference {} → {} commits  [mode: {}]", config.reference, total, mode_label);
    println!("Base: {}", base_ref);
    println!(
        "Progress: {}/{} processed, {} pending\n",
        progress.processed.len(),
        total,
        progress.pending.len()
    );
    if let Some(n) = config.limit {
        println!("Session limit: will stop after {} decision(s) this run.\n", n);
    }
    if config.triage_only {
        println!("Triage mode: analyzer + decision only — no apply agent will run.");
        println!("  `apply` / `apply-with-edits` decisions queue into progress.pending.");
        println!("  A later `apply-midlevel` (without --triage) consumes them unattended.\n");
    } else if config.pending_only {
        println!(
            "Pending-only mode: only the {} pending entr{} will be applied; un-triaged",
            progress.pending.len(),
            if progress.pending.len() == 1 { "y" } else { "ies" }
        );
        println!("  commits are left untouched. Runs to completion without prompting.\n");
    } else if !progress.pending.is_empty() {
        println!(
            "Will auto-consume {} pending decision(s) without prompting.\n",
            progress.pending.len()
        );
    }

    // ── Telegram bridge (optional) ──────────────────────────────────────
    let bridge: Option<Arc<TelegramBridge>> = if config.no_telegram {
        None
    } else {
        match TelegramBridge::from_env_or_config() {
            Some(Ok(b)) => {
                println!(
                    "[telegram] active — chat_id={}, prompts will mirror to your bot",
                    b.chat_id
                );
                let _ = b.send_message(
                    &format!(
                        "azul-doc apply-midlevel started [mode: {}]\n\
                         reference: {}\n\
                         {} commits, {} processed, {} pending",
                        mode_label,
                        config.reference,
                        total,
                        progress.processed.len(),
                        progress.pending.len()
                    ),
                    None,
                );
                Some(Arc::new(b))
            }
            Some(Err(e)) => {
                eprintln!("[telegram] config error, disabling: {}", e);
                None
            }
            None => None,
        }
    };
    let input_channel = InputChannel::start(bridge.clone());

    // Per-session counter for --limit. Pure-`.md` auto-skips don't count
    // (they happen before we reach the decision section).
    let mut decisions_this_run: usize = 0;

    // Main loop
    loop {
        let next = match find_next(&commits, &progress, config.triage_only, config.pending_only) {
            Some(sha) => sha.clone(),
            None => {
                if config.pending_only {
                    println!(
                        "No more pending entries. {} processed this run.",
                        decisions_this_run
                    );
                } else if config.triage_only {
                    println!(
                        "All commits triaged. {} entries queued for apply.",
                        progress.pending.len()
                    );
                } else {
                    println!("All commits processed.");
                }
                if let Some(b) = bridge.as_ref() {
                    let applied = progress.processed.iter().filter(|d| matches!(
                        d.decision,
                        DecisionKind::AppliedByAgent | DecisionKind::AppliedEdited
                    )).count();
                    let rejected = progress.processed.iter()
                        .filter(|d| d.decision == DecisionKind::Rejected).count();
                    let skipped = progress.processed.iter().filter(|d| matches!(
                        d.decision,
                        DecisionKind::SkippedMd | DecisionKind::SkippedByUser
                    )).count();
                    let _ = b.send_message(
                        &format!(
                            "apply-midlevel finished [mode: {}]\nreference: {}\napplied={} rejected={} skipped={} pending={}",
                            mode_label, config.reference, applied, rejected, skipped, progress.pending.len()
                        ),
                        None,
                    );
                }
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

        // Consume any pre-decided "pending" entry for this commit. In normal
        // (non-triage) mode this is the unattended path: the user already
        // decided via a previous `triage` run, so we skip the analyzer + prompt
        // entirely and go straight to the apply agent.
        //
        // In triage mode `find_next` already filters pending entries out, so
        // this only fires in normal mode.
        let pending: Option<PendingDecision> = {
            let idx = progress.pending.iter().position(|p| p.sha == next);
            idx.map(|i| progress.pending.remove(i))
        };
        let consuming_pending = pending.is_some();
        if consuming_pending {
            save_progress(&progress_path, &progress)?;
        }

        // Mark current and save — so Ctrl+C leaves a known-good pointer
        progress.current = Some(next.clone());
        save_progress(&progress_path, &progress)?;

        // ── Plan session (analyzer iterations) ──────────────────────────
        let mut plan = PlanSession { iterations: Vec::new() };

        // Restore the full iteration trace from a pending entry so the apply
        // agent's prompt gets the AGREED PLAN section (latest_plan) plus the
        // full feedback history (all_feedback) — i.e. exactly what the user
        // saw and approved during triage. Falls back to the legacy single
        // `comment` blob for entries from before `iterations` existed.
        if let Some(ref pd) = pending {
            if !pd.iterations.is_empty() {
                plan.iterations = pd.iterations.clone();
            } else if let Some(c) = &pd.comment {
                plan.iterations.push(PlanIteration {
                    user_feedback: Some(c.clone()),
                    analyzer_output: String::new(),
                });
            }
        }

        let decision_taken: UserAction = if let Some(ref pd) = pending {
            println!();
            println!(
                "[pending] auto-applying pre-decided action: {:?}",
                pd.action
            );
            let feedback_blob: Option<String> = if !pd.iterations.is_empty() {
                let v: Vec<String> = pd.iterations.iter()
                    .filter_map(|it| it.user_feedback.clone())
                    .collect();
                if v.is_empty() { None } else { Some(v.join("\n---\n")) }
            } else {
                pd.comment.clone()
            };
            if plan.latest_plan().is_some() {
                println!("[pending] restored AGREED PLAN from triage:");
                if let Some(p) = plan.latest_plan() {
                    for line in p.lines() {
                        println!("  | {}", line);
                    }
                }
            } else {
                println!("[pending] (no analyzer plan saved — legacy entry, apply agent re-derives)");
            }
            if let Some(c) = &feedback_blob {
                println!("[pending] saved instructions:");
                for line in c.lines() {
                    println!("  > {}", line);
                }
            }
            if let Some(b) = bridge.as_ref() {
                let _ = b.send_message(
                    &format!(
                        "[pending {}/{}] {}\n→ {:?}\n{}",
                        processed_so_far + 1,
                        total,
                        info.subject,
                        pd.action,
                        feedback_blob.as_deref().unwrap_or("(no extra instructions)")
                    ),
                    None,
                );
            }
            UserAction::Yes
        } else {
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
            loop {
                let phone_summary = build_phone_summary(
                    &info,
                    paired_docs.as_ref(),
                    &plan,
                    processed_so_far + 1,
                    total,
                    &progress,
                );
                let action = prompt_user(&input_channel, Some(&phone_summary))?;
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
                        open_commit_in_editor(&project_root, &next, &info, &input_channel)?;
                        continue;
                    }
                    UserAction::ShowRemote => {
                        if let Some(b) = bridge.as_ref() {
                            if let Err(e) = send_commit_diff_to_phone(b, &project_root, &next, &info) {
                                eprintln!("[telegram] sendDocument failed: {}", e);
                            }
                        }
                        continue;
                    }
                    other => break other,
                }
            }
        };

        match decision_taken {
            UserAction::Yes => {
                // Triage mode + no pre-decision: queue, do NOT spawn the apply
                // agent. This is the whole point of triage — get a decision per
                // commit without paying the 20-min CI cost for each.
                if config.triage_only && !consuming_pending {
                    let any_feedback = plan.iterations.iter()
                        .any(|it| it.user_feedback.is_some());
                    let action = if any_feedback {
                        PendingAction::Edit
                    } else {
                        PendingAction::Apply
                    };
                    // Lock in the full track record: every analyzer round
                    // and every user-refinement that led to the approved
                    // plan, in order. Restored verbatim when the apply
                    // agent runs unattended later — the prompt's AGREED
                    // PLAN section gets the exact plan from triage.
                    let iterations = plan.iterations.clone();
                    progress.current = None;
                    progress.pending.push(PendingDecision {
                        sha: next.clone(),
                        subject: info.subject.clone(),
                        action: action.clone(),
                        iterations,
                        comment: None,
                    });
                    save_progress(&progress_path, &progress)?;
                    let pending_count = progress.pending.len();
                    println!(
                        "[triage] queued ({:?}); {} pending entr{} now waiting for apply.",
                        action,
                        pending_count,
                        if pending_count == 1 { "y" } else { "ies" }
                    );
                    if let Some(b) = bridge.as_ref() {
                        let _ = b.send_message(
                            &format!(
                                "[triage {}/{}] {}\nqueued: {:?} ({} pending)",
                                processed_so_far + 1, total, info.subject, action, pending_count
                            ),
                            None,
                        );
                    }
                    println!();
                } else {

                let pre_head = git_head(&project_root)?;
                let user_refinements: Vec<String> = plan.iterations.iter()
                    .filter_map(|it| it.user_feedback.clone())
                    .collect();
                let was_refined = !user_refinements.is_empty();

                // Counts consecutive failures of run_apply_agent. Resets to 0
                // on success (so post-apply refinement gets a fresh budget).
                let mut attempts: u32 = 0;

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
                            attempts = 0;
                            print_applied_summary(&project_root, &pre_head, &applied.new_sha)?;
                            if let Some(b) = bridge.as_ref() {
                                if let Err(e) = send_applied_diff_to_phone(
                                    b, &project_root, &pre_head, &applied.new_sha,
                                ) {
                                    eprintln!("[telegram] applied-diff sendDocument failed: {}", e);
                                }
                            }
                            // When consuming a pre-decided pending entry, the
                            // user wanted unattended execution — auto-accept
                            // the result. Diff was already mirrored to phone
                            // above for later review.
                            if consuming_pending {
                                break Ok(applied);
                            }
                            match prompt_post_apply(&input_channel)? {
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
                            attempts += 1;

                            // In unattended (pending-consumer) mode, retry on
                            // failure. Concurrent-build / lock-contention
                            // errors get a 60s wait (another azul-doc process
                            // is likely using the same target/ dir) and no
                            // injected feedback — the patch is innocent, the
                            // env is wrong. Real errors get a 5s wait + a
                            // hint to the agent describing what failed.
                            if consuming_pending && attempts < config.retries {
                                let lc = e.to_ascii_lowercase();
                                let looks_transient = lc.contains("blocking waiting")
                                    || lc.contains("file lock")
                                    || lc.contains("could not acquire")
                                    || lc.contains("enospc")
                                    || lc.contains("no space left on device")
                                    || lc.contains("resource temporarily unavailable")
                                    || lc.contains("text file busy")
                                    || lc.contains("is being used by another process");
                                let sleep_secs: u64 = if looks_transient { 60 } else { 5 };
                                let kind = if looks_transient { "transient/concurrent-build" } else { "real" };

                                println!(
                                    "\n[pending] attempt {}/{} failed ({} error): {}",
                                    attempts, config.retries, kind, e
                                );
                                println!("[pending] waiting {}s, then retrying...", sleep_secs);
                                if let Some(b) = bridge.as_ref() {
                                    let _ = b.send_message(
                                        &format!(
                                            "[pending] {} attempt {}/{} ({} err) — waiting {}s before retry\n{}",
                                            short(&next), attempts, config.retries,
                                            kind, sleep_secs, e
                                        ),
                                        None,
                                    );
                                }

                                // Reset OUR scope for retry. Concurrent
                                // codegen agents' uncommitted work in
                                // lang_* / examples/ is preserved — a
                                // global `reset --hard` would wipe it and
                                // confuse the other agent. See
                                // `cleanup_our_scope_to`.
                                let _ = cleanup_our_scope_to(&project_root, &pre_head);

                                std::thread::sleep(std::time::Duration::from_secs(sleep_secs));

                                if !looks_transient {
                                    plan.iterations.push(PlanIteration {
                                        user_feedback: Some(format!(
                                            "RETRY {}/{}: the previous attempt failed with:\n{}\n\
                                             Try a different approach.",
                                            attempts + 1, config.retries, e
                                        )),
                                        analyzer_output: String::new(),
                                    });
                                }
                                continue;
                            }

                            if consuming_pending {
                                println!("\n[pending] apply failed after {} attempt(s): {}", attempts, e);
                                println!("[pending] recording as rejected; moving on to next commit.");
                                if let Some(b) = bridge.as_ref() {
                                    let _ = b.send_message(
                                        &format!(
                                            "[pending] {} APPLY FAILED after {} attempts — recorded as rejected: {}",
                                            short(&next), attempts, e
                                        ),
                                        None,
                                    );
                                }
                                break Err(format!("pending-apply failed after {} attempts: {}", attempts, e));
                            }
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
            UserAction::Refine(_) | UserAction::Show | UserAction::ShowRemote => unreachable!(),
        }

        // Session limit (--limit=N): stop after N decisions in this run so
        // the user can take a break mid-triage / cap an unattended apply batch.
        decisions_this_run += 1;
        if let Some(n) = config.limit {
            if decisions_this_run >= n {
                println!(
                    "\n--limit={} reached ({} decision{} this run). Saving and exiting.",
                    n,
                    decisions_this_run,
                    if decisions_this_run == 1 { "" } else { "s" }
                );
                save_progress(&progress_path, &progress)?;
                if let Some(b) = bridge.as_ref() {
                    let _ = b.send_message(
                        &format!(
                            "Hit --limit={} ({} this run). Re-run to continue.",
                            n, decisions_this_run
                        ),
                        None,
                    );
                }
                break;
            }
        }
    }

    Ok(())
}

/// Replay the analyzer for every pending entry that doesn't yet have its
/// `iterations` trace saved (i.e. legacy entries from before that field
/// existed). For each, we re-run the initial analyzer pass + every
/// user-feedback round in order so the final restored `PlanSession` matches
/// what the user saw and approved during triage. No apply agent is spawned;
/// the result is written back into `progress.pending` so a later
/// `apply-midlevel pending` run gets the AGREED PLAN section in its prompt.
fn run_refresh_pending(
    project_root: &Path,
    progress: &mut Progress,
    progress_path: &Path,
    commits: &[String],
    config: &Config,
) -> Result<(), String> {
    let to_refresh: Vec<usize> = progress.pending.iter().enumerate()
        .filter(|(_, pd)| pd.iterations.is_empty())
        .map(|(i, _)| i)
        .collect();

    println!(
        "Refresh-pending mode: {} legacy pending entr{} need an analyzer plan (out of {} total).",
        to_refresh.len(),
        if to_refresh.len() == 1 { "y" } else { "ies" },
        progress.pending.len()
    );
    if to_refresh.is_empty() {
        println!("Nothing to refresh.");
        return Ok(());
    }
    if let Some(n) = config.limit {
        println!("Session limit: will stop after {} refresh(es) this run.", n);
    }
    println!();

    let mut refreshed = 0usize;
    for (i, &idx) in to_refresh.iter().enumerate() {
        let pd = progress.pending[idx].clone();
        let info = load_commit_info(project_root, &pd.sha)?;
        let paired_docs = find_paired_docs(project_root, commits, &pd.sha)?;

        println!("════════════════════════════════════════════════════════════════════════");
        println!(
            "Refreshing {}  ({} of {})",
            short(&pd.sha), i + 1, to_refresh.len()
        );
        println!("  Subject: {}", info.subject);
        println!("  Action:  {:?}", pd.action);
        let legacy_comment = pd.comment.clone();
        if let Some(c) = &legacy_comment {
            println!("  Saved feedback:");
            for line in c.lines() {
                println!("    > {}", line);
            }
        } else {
            println!("  Saved feedback: (none — pure `apply`)");
        }
        println!("────────────────────────────────────────────────────────────────────────");

        // Split the legacy `\n---\n`-joined feedback back into individual
        // refinement chunks so we re-walk the same analyzer iterations the
        // user originally saw.
        let chunks: Vec<String> = legacy_comment.as_deref()
            .map(|c| c.split("\n---\n").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();

        let mut plan = PlanSession { iterations: Vec::new() };
        let mut failed = false;

        // Initial analyzer pass (no feedback yet).
        match run_analysis_agent(
            project_root, &pd.sha, &info, paired_docs.as_ref(),
            &plan, None, config,
        ) {
            Ok(output) => plan.iterations.push(PlanIteration {
                user_feedback: None,
                analyzer_output: output,
            }),
            Err(e) => {
                eprintln!("[refresh-pending] initial analyzer failed for {}: {}", short(&pd.sha), e);
                failed = true;
            }
        }

        if !failed {
            for chunk in &chunks {
                match run_analysis_agent(
                    project_root, &pd.sha, &info, paired_docs.as_ref(),
                    &plan, Some(chunk), config,
                ) {
                    Ok(output) => plan.iterations.push(PlanIteration {
                        user_feedback: Some(chunk.clone()),
                        analyzer_output: output,
                    }),
                    Err(e) => {
                        eprintln!("[refresh-pending] refinement analyzer failed for {}: {}", short(&pd.sha), e);
                        failed = true;
                        break;
                    }
                }
            }
        }

        if failed {
            println!("[refresh-pending] {} left as legacy entry; continuing.", short(&pd.sha));
        } else {
            progress.pending[idx].iterations = plan.iterations;
            // Drop the legacy `comment` blob — the iteration trace is now the
            // canonical source. (Old field stays absent in JSON via `skip_serializing_if`.)
            progress.pending[idx].comment = None;
            save_progress(progress_path, progress)?;
            refreshed += 1;
            println!("[refresh-pending] saved analyzer trace for {} (rounds={}).", short(&pd.sha), progress.pending[idx].iterations.len());
        }
        println!();

        if let Some(n) = config.limit {
            if refreshed >= n {
                println!("--limit={} reached. Stopping refresh.", n);
                break;
            }
        }
    }

    println!(
        "Refresh complete: {} entr{} updated.",
        refreshed,
        if refreshed == 1 { "y" } else { "ies" }
    );
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
    /// Pre-decided actions queued by the `triage` subcommand. The main loop
    /// auto-consumes these instead of prompting the user — so the user can do
    /// fast analyze-only triage in one pass and let the slow CI/cross-compile
    /// pipeline run unattended later.
    #[serde(default)]
    pending: Vec<PendingDecision>,
}

impl Progress {
    fn new(reference: &str, reference_sha: &str, base_sha: &str) -> Self {
        Self {
            reference: reference.to_string(),
            reference_sha: reference_sha.to_string(),
            base_sha: base_sha.to_string(),
            current: None,
            processed: Vec::new(),
            pending: Vec::new(),
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

/// A pre-decided "yes, apply this" produced during a `triage` run. The main
/// `apply-midlevel` loop consumes these without prompting, runs the apply
/// agent, and on success/failure converts the entry to a `Decision` in
/// `processed`.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct PendingDecision {
    sha: String,
    subject: String,
    action: PendingAction,
    /// Full chronological trace of every analyzer round + user-refinement
    /// the user iterated through before approving. The last entry's
    /// `analyzer_output` is the plan they signed off on. Restored verbatim
    /// into the apply agent's `PlanSession` so the prompt has the exact
    /// AGREED PLAN they saw and the entire feedback history.
    #[serde(default)]
    iterations: Vec<PlanIteration>,
    /// Legacy single-blob comment from entries created before `iterations`
    /// existed. Used as a fallback by the consumer when `iterations` is
    /// empty; new code does not write this field. Backfill with
    /// `apply-midlevel refresh-pending`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum PendingAction {
    /// Apply the commit as-is — no extra instructions.
    Apply,
    /// Apply with the saved `comment` fed to the agent as user feedback.
    Edit,
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

fn find_next<'a>(
    commits: &'a [String],
    progress: &Progress,
    skip_pending: bool,
    pending_only: bool,
) -> Option<&'a String> {
    // Collect set of already-processed SHAs (including auto-skipped docs that
    // were implicitly skipped — those appear as SkippedMd entries).
    let done: std::collections::HashSet<&str> =
        progress.processed.iter().map(|d| d.sha.as_str()).collect();
    let pending: std::collections::HashSet<&str> =
        progress.pending.iter().map(|p| p.sha.as_str()).collect();
    if pending_only {
        // --pending-only: only consider commits that already have a queued
        // pre-decision. Iterating `commits` (not `pending`) keeps the apply
        // order matching commit order, which the cherry-pick logic expects.
        commits
            .iter()
            .find(|c| pending.contains(c.as_str()) && !done.contains(c.as_str()))
    } else if skip_pending {
        // Triage mode: also skip commits whose pre-decision is already queued,
        // so a re-run resumes at the first un-triaged commit.
        commits
            .iter()
            .find(|c| !done.contains(c.as_str()) && !pending.contains(c.as_str()))
    } else {
        commits.iter().find(|c| !done.contains(c.as_str()))
    }
}

// ── UI ────────────────────────────────────────────────────────────────────

/// Phone-friendly summary for the Telegram message: progress banner + commit
/// info + the analyzer's recommendation tail (the `[CATEGORY] / Why: / Plan:
/// / Suggested user action:` block). Capped well below Telegram's 4096-char
/// limit; further truncation happens during send if needed.
fn build_phone_summary(
    info: &CommitInfo,
    paired: Option<&CommitInfo>,
    plan: &PlanSession,
    n: usize,
    total: usize,
    progress: &Progress,
) -> String {
    let applied = progress.processed.iter().filter(|d| matches!(
        d.decision,
        DecisionKind::AppliedByAgent | DecisionKind::AppliedEdited
    )).count();
    let rejected = progress.processed.iter().filter(|d| d.decision == DecisionKind::Rejected).count();
    let skipped = progress.processed.iter().filter(|d| matches!(
        d.decision,
        DecisionKind::SkippedMd | DecisionKind::SkippedByUser
    )).count();

    let mut s = String::new();
    s.push_str(&format!(
        "Commit {}/{} | applied={} rejected={} skipped={}\n",
        n, total, applied, rejected, skipped
    ));
    s.push_str(&format!("{}\n", &info.sha[..info.sha.len().min(12)]));
    s.push_str(&format!("{}\n", info.subject));

    if !info.body.is_empty() {
        let body_trim: String = info.body.lines().take(4).collect::<Vec<_>>().join("\n");
        s.push_str(&format!("\n{}\n", body_trim));
    }

    s.push_str("\nFiles:\n");
    for f in info.files.iter().take(8) {
        s.push_str(&format!("  +{} -{}  {}\n", f.additions, f.deletions, f.path));
    }
    if info.files.len() > 8 {
        s.push_str(&format!("  …(+{} more)\n", info.files.len() - 8));
    }
    if let Some(p) = paired {
        s.push_str(&format!(
            "\nPaired docs: {}\n",
            &p.sha[..p.sha.len().min(12)]
        ));
    }

    if let Some(latest) = plan.latest_plan() {
        s.push_str("\n— analyzer —\n");
        s.push_str(&extract_analyzer_summary(latest));
    }

    s
}

/// Pull just the recommendation block out of the analyzer's full output.
/// The analyzer is told to produce a structured tail starting with
/// `[KEEP|DONE|WIRE|REFACTOR|REJECT|UNCLEAR]`; we find the LAST such tag and
/// return everything from that line onward, capped at 1500 chars.
fn extract_analyzer_summary(text: &str) -> String {
    let tags = ["[KEEP]", "[DONE]", "[WIRE]", "[REFACTOR]", "[REJECT]", "[UNCLEAR]"];
    let trimmed = text.trim_end();

    let mut last_byte: Option<usize> = None;
    for tag in &tags {
        if let Some(pos) = trimmed.rfind(tag) {
            let line_start = trimmed[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
            last_byte = Some(match last_byte {
                None => line_start,
                Some(prev) => prev.max(line_start),
            });
        }
    }

    let start = last_byte.unwrap_or_else(|| {
        // No structured tag found — return the last 1500 chars.
        let total = trimmed.chars().count();
        if total > 1500 {
            trimmed
                .char_indices()
                .nth(total - 1500)
                .map(|(i, _)| i)
                .unwrap_or(0)
        } else {
            0
        }
    });

    let body = trimmed[start..].trim();
    if body.chars().count() > 1500 {
        let cut = body
            .char_indices()
            .nth(1500)
            .map(|(i, _)| i)
            .unwrap_or(body.len());
        format!("{}\n…(truncated)", &body[..cut])
    } else {
        body.to_string()
    }
}

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
    input: &InputChannel,
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

    if let Some(bridge) = input.bridge.as_ref() {
        let _ = bridge.send_message(
            &format!(
                "Local checkout of {} for editor inspection. Send any message here \
                 (or press Enter on the terminal) to restore branch {}.",
                &sha[..12],
                branch
            ),
            None,
        );
    }
    input.drain_stale();
    let _ = input.recv()?;

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
    /// (Local) Checkout the reference commit so the user's editor shows
    /// its state. Inherently local — needs a working tree to flip.
    Show,
    /// (Remote) Send the reference commit's `git show` patch to the phone.
    /// No working-tree changes; just an FYI document for the user to
    /// preview before deciding.
    ShowRemote,
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

fn prompt_post_apply(input: &InputChannel) -> Result<PostApply, String> {
    println!();
    print!("Accept this commit? accept/y / edit/e / revert/r / quit/q: ");
    io::stdout().flush().ok();

    if let Some(bridge) = input.bridge.as_ref() {
        let kb_rows: &[&[&str]] = &[&["accept", "edit"], &["revert", "quit"]];
        // The applied diff was sent right before this prompt, so the
        // message itself can be terse — the reply keyboard names the actions.
        let _ = bridge.send_message(
            "Apply succeeded. Diff is attached above.",
            Some(kb_rows),
        );
    }

    input.drain_stale();
    let line = input.recv()?.into_text();
    let trimmed = line.trim();

    // `apply`/`a` are accepted in addition to `accept` because muscle
    // memory carries over from the pre-apply prompt where the button is
    // labelled "apply". Same intent — advance to the next commit.
    let token: Option<&str> = match trimmed.to_ascii_lowercase().as_str() {
        "y" | "yes" | "a" | "accept" | "apply" | "ok" => Some("accept"),
        "e" | "edit" => Some("edit"),
        "r" | "revert" | "undo" => Some("revert"),
        "q" | "quit" | "exit" => Some("quit"),
        _ => None,
    };

    let token = match token {
        Some(t) => t,
        None => {
            if trimmed.is_empty() {
                println!("  (empty input — please pick an action, or type instructions)");
                return prompt_post_apply(input);
            }
            // Free-form text → treat as edit-further feedback directly.
            return Ok(PostApply::Refine(trimmed.to_string()));
        }
    };

    match token {
        "accept" => Ok(PostApply::Accept),
        "edit" => {
            let instr = read_followup(
                input,
                "Additional instructions for the agent:",
                "Additional instructions for the agent (end with '.' on its own line):",
            )?;
            Ok(PostApply::Refine(instr))
        }
        "revert" => Ok(PostApply::Revert),
        "quit" => Ok(PostApply::Quit),
        _ => unreachable!(),
    }
}

/// Ship the reference commit's full patch (`git show <sha>`) to Telegram
/// as `<short-sha>.patch`. Lets the user preview the diff on their phone
/// without needing local editor access. Caption carries the subject line
/// so they can recognise it at a glance.
fn send_commit_diff_to_phone(
    bridge: &TelegramBridge,
    project_root: &Path,
    sha: &str,
    info: &CommitInfo,
) -> Result<(), String> {
    let out = Command::new("git")
        .args(["show", "--patch", "--stat", sha])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git show: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git show: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let filename = format!("{}.patch", short(sha));
    let caption = format!("{}  {}", short(sha), info.subject);
    bridge.send_document(&filename, &out.stdout, Some(&caption))
}

/// Ship the *applied* diff (between `pre_head` and `new_head`) to Telegram.
/// Auto-fired right after a successful apply so the user can review what
/// the agent actually did before deciding accept/edit/revert.
fn send_applied_diff_to_phone(
    bridge: &TelegramBridge,
    project_root: &Path,
    pre_head: &str,
    new_head: &str,
) -> Result<(), String> {
    let range = format!("{}..{}", pre_head, new_head);
    let out = Command::new("git")
        .args(["log", "--patch", "--stat", "--reverse", &range])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git log: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git log: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let filename = format!("applied-{}.patch", short(new_head));
    let caption = format!("Applied diff {}..{}", short(pre_head), short(new_head));
    bridge.send_document(&filename, &out.stdout, Some(&caption))
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

fn prompt_user(
    input: &InputChannel,
    phone_summary: Option<&str>,
) -> Result<UserAction, String> {
    println!("Decision?");
    println!("  apply  / y    — apply using the current plan");
    println!("  refine / p    — refine the plan: add feedback, analyzer revises");
    println!("  skip   / s    — don't apply now, come back later");
    println!("  reject / r    — don't apply, record as rejected with reason");
    println!("  diff   / d    — checkout commit so your editor shows its state");
    println!("  quit   / q    — save progress and exit");
    println!("  (any other multi-character text is taken as analyzer feedback)");
    print!("> ");
    io::stdout().flush().ok();

    if let Some(bridge) = input.bridge.as_ref() {
        // Phone message: just the summary. The reply keyboard names the
        // actions, so no inline legend is needed.
        let msg = phone_summary.unwrap_or("Decision?").to_string();
        let kb_rows: &[&[&str]] = &[
            &["apply", "refine"],
            &["diff", "skip"],
            &["reject", "quit"],
        ];
        if let Err(e) = bridge.send_message(&msg, Some(kb_rows)) {
            eprintln!("[telegram] send failed: {}", e);
        }
    }

    input.drain_stale();
    let user_input = input.recv()?;
    let was_remote = matches!(user_input, UserInput::Remote(_));
    let line = user_input.into_text();
    let trimmed = line.trim();

    // Map both the new word-buttons and the legacy single-letter shortcuts
    // to a canonical token. Anything that doesn't match either is treated
    // as free-form analyzer feedback.
    let token: Option<&str> = match trimmed.to_ascii_lowercase().as_str() {
        "y" | "yes" | "apply" => Some("apply"),
        "p" | "plan" | "refine" => Some("refine"),
        "s" | "skip" => Some("skip"),
        "r" | "reject" => Some("reject"),
        "d" | "diff" | "show" => Some("diff"),
        "q" | "quit" | "exit" => Some("quit"),
        _ => None,
    };

    let token = match token {
        Some(t) => t,
        None => {
            // Free-form text → analyzer feedback. Empty input falls through
            // to a re-prompt rather than feeding "" to the analyzer.
            if trimmed.is_empty() {
                println!("  (empty input — please pick an action, or type feedback)");
                return prompt_user(input, phone_summary);
            }
            return Ok(UserAction::Refine(trimmed.to_string()));
        }
    };

    match token {
        "apply" => Ok(UserAction::Yes),
        "refine" => {
            let fb = read_followup(
                input,
                "Feedback for the analyzer:",
                "Feedback for the analyzer (end with a single '.' on its own line):",
            )?;
            Ok(UserAction::Refine(fb))
        }
        "skip" => {
            let notes = read_oneline(input, "Reason (one line, empty to skip prompt):")?;
            Ok(UserAction::Skip(if notes.is_empty() { None } else { Some(notes) }))
        }
        "reject" => {
            let notes = read_oneline(
                input,
                "Reason for rejecting (one line, empty to skip prompt):",
            )?;
            Ok(UserAction::Reject(if notes.is_empty() { None } else { Some(notes) }))
        }
        "diff" => {
            if was_remote {
                Ok(UserAction::ShowRemote)
            } else {
                Ok(UserAction::Show)
            }
        }
        "quit" => Ok(UserAction::Quit),
        _ => unreachable!(),
    }
}

/// Read one line of follow-up. From local stdin, accepts a single line
/// (trimmed); from Telegram, takes the entire next message verbatim.
fn read_oneline(input: &InputChannel, prompt: &str) -> Result<String, String> {
    println!("  {}", prompt);
    if let Some(bridge) = input.bridge.as_ref() {
        let _ = bridge.send_message(prompt, None);
    }
    input.drain_stale();
    Ok(input.recv()?.into_text().trim().to_string())
}

/// Read a multi-line follow-up. Local: terminate with a single `.` on its
/// own line (legacy behaviour). Remote: the entire message is taken as-is.
fn read_followup(
    input: &InputChannel,
    remote_prompt: &str,
    local_prompt: &str,
) -> Result<String, String> {
    if let Some(bridge) = input.bridge.as_ref() {
        let _ = bridge.send_message(remote_prompt, None);
    }
    println!("  {}", local_prompt);
    input.drain_stale();

    let first = input.recv()?;
    match first {
        UserInput::Remote(text) => Ok(text),
        UserInput::Local(line) => {
            let mut buf = String::new();
            if line.trim() != "." {
                buf.push_str(&line);
                if !buf.ends_with('\n') {
                    buf.push('\n');
                }
            } else {
                return Ok(buf);
            }
            // Remaining lines: keep reading until '.' on its own line, but
            // a Remote message arriving here is treated as the whole thing.
            loop {
                match input.recv()? {
                    UserInput::Remote(text) => {
                        buf.push_str(&text);
                        if !buf.ends_with('\n') {
                            buf.push('\n');
                        }
                        return Ok(buf);
                    }
                    UserInput::Local(l) => {
                        if l.trim() == "." {
                            return Ok(buf);
                        }
                        buf.push_str(&l);
                        if !buf.ends_with('\n') {
                            buf.push('\n');
                        }
                    }
                }
            }
        }
    }
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

#[derive(Serialize, Deserialize, Clone, Debug)]
struct PlanIteration {
    /// None for the initial pass; Some(text) when the user supplied feedback
    /// that the analyzer should incorporate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user_feedback: Option<String>,
    /// Analyzer's output for this iteration. Empty string when this entry
    /// represents post-apply user feedback that the apply-agent should heed
    /// (no analyzer run happens in that case).
    #[serde(default, skip_serializing_if = "String::is_empty")]
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
    let session_name = format!("analyzer {}.{}", short(sha), iter_idx);

    // Retry loop: on session-UUID collision (left over from a crashed/
    // restarted run, or a race), regenerate a fresh UUID and try again.
    let max_retries: u32 = 4;
    let mut attempt: u32 = 0;
    loop {
        let session_uuid = pick_free_session_uuid(sha, "a", iter_idx, attempt, project_root);
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

        match spawn_claude_streaming(
            &cmd_args, prompt.as_bytes(), project_root, &path_with_rustup, None,
        ) {
            Ok(out) => {
                println!("└───────────────────────────────────────────────────────────────────────");
                return Ok(out.text);
            }
            Err(ClaudeError::SessionInUse) if attempt < max_retries => {
                attempt += 1;
                println!(
                    "  [retry {}/{}] session UUID was already in use — regenerating...",
                    attempt, max_retries
                );
                continue;
            }
            Err(e) => {
                println!("└───────────────────────────────────────────────────────────────────────");
                return Err(e.to_string());
            }
        }
    }
}

// ── Streaming helpers ─────────────────────────────────────────────────────

struct StreamOutput {
    /// Accumulated assistant text content (captured from `assistant` events
    /// after being printed live as `stream_event` deltas).
    text: String,
    #[allow(dead_code)]
    session_id: Option<String>,
}

/// Distinguishes a "session UUID collision" failure (recoverable by retrying
/// with a different UUID) from any other error.
#[derive(Debug)]
enum ClaudeError {
    /// Claude refused to start because `--session-id <uuid>` was already in
    /// use (i.e. a `~/.claude/projects/<...>/<uuid>.jsonl` file exists from a
    /// prior run). Callers should retry with a fresh UUID.
    SessionInUse,
    Other(String),
}

impl std::fmt::Display for ClaudeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClaudeError::SessionInUse => write!(f, "claude session UUID already in use"),
            ClaudeError::Other(s) => write!(f, "{}", s),
        }
    }
}

/// Spawn `claude` with the given args in stream-json mode, pipe the prompt
/// into stdin, and stream the output live to stdout while capturing text.
/// Stderr is tee'd to the parent's stderr AND scanned for the specific
/// "Session ID X is already in use" message so the caller can retry.
///
/// `extra_env` allows callers to pass additional env vars (like `AZ_DOC_BIN`).
fn spawn_claude_streaming(
    args: &[&str],
    stdin_bytes: &[u8],
    cwd: &Path,
    path_env: &str,
    extra_env: Option<&[(&str, &Path)]>,
) -> Result<StreamOutput, ClaudeError> {
    let mut cmd = Command::new("claude");
    cmd.args(args)
        .env_remove("CLAUDECODE")
        .env("PATH", path_env)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(env_pairs) = extra_env {
        for (k, v) in env_pairs {
            cmd.env(k, v);
        }
    }

    let mut child = cmd.spawn()
        .map_err(|e| ClaudeError::Other(format!("spawn claude: {}", e)))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(stdin_bytes)
            .map_err(|e| ClaudeError::Other(format!("write prompt stdin: {}", e)))?;
        drop(stdin);
    }

    let stdout = child.stdout.take()
        .ok_or_else(|| ClaudeError::Other("failed to grab child stdout".to_string()))?;
    let stderr = child.stderr.take()
        .ok_or_else(|| ClaudeError::Other("failed to grab child stderr".to_string()))?;

    let stdout_handle = std::thread::spawn(move || process_stream_events(stdout));
    let stderr_handle = std::thread::spawn(move || tee_stderr(stderr));

    let status = child.wait()
        .map_err(|e| ClaudeError::Other(format!("wait claude: {}", e)))?;
    let out = stdout_handle.join()
        .map_err(|_| ClaudeError::Other("stream thread panicked".to_string()))?;
    let stderr_scan = stderr_handle.join()
        .unwrap_or(StderrScan { session_in_use: false });

    if !status.success() {
        if stderr_scan.session_in_use {
            return Err(ClaudeError::SessionInUse);
        }
        return Err(ClaudeError::Other(format!("claude exited with status {}", status)));
    }
    Ok(out)
}

struct StderrScan {
    /// True if stderr contained "Session ID … is already in use".
    session_in_use: bool,
}

/// Tee Claude's stderr to our own stderr while scanning for the specific
/// session-collision message.
fn tee_stderr<R: std::io::Read + Send + 'static>(stderr: R) -> StderrScan {
    use std::io::BufRead;
    let reader = std::io::BufReader::new(stderr);
    let mut session_in_use = false;
    for line in reader.lines() {
        let line = match line { Ok(l) => l, Err(_) => break };
        if line.contains("is already in use")
            && line.to_ascii_lowercase().contains("session id")
        {
            session_in_use = true;
        }
        eprintln!("{}", line);
    }
    StderrScan { session_in_use }
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

/// Path to the on-disk transcript file that `claude` would use for a given
/// session UUID when invoked from `cwd`. Claude stores transcripts at
/// `~/.claude/projects/<sanitized-cwd>/<uuid>.jsonl`, where the sanitization
/// replaces `/` with `-` in the absolute path.
fn claude_session_file(cwd: &Path, uuid: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let abs = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let sanitized: String = abs
        .to_string_lossy()
        .chars()
        .map(|c| if c == '/' { '-' } else { c })
        .collect();
    Some(
        PathBuf::from(home)
            .join(".claude/projects")
            .join(sanitized)
            .join(format!("{}.jsonl", uuid)),
    )
}

/// Check if a session UUID is already taken by a prior `claude` run.
fn claude_session_in_use(cwd: &Path, uuid: &str) -> bool {
    claude_session_file(cwd, uuid).map(|p| p.exists()).unwrap_or(false)
}

/// Pick a session UUID that is not already in use on disk. On the first
/// attempt we try the deterministic UUID (so re-runs stay predictable in the
/// happy path); on retries we fall back to a random UUID because
/// `make_session_uuid` only varies the last 2 hex chars (256 values per SHA)
/// which is too narrow for exhaustive retrying.
///
/// The pre-check is only advisory — `spawn_claude_streaming` also detects the
/// race where another `claude` process grabs the UUID after we checked. On
/// that error the caller should pass `retry_attempt > 0`.
fn pick_free_session_uuid(
    sha: &str,
    suffix_prefix: &str,
    iter_idx: usize,
    retry_attempt: u32,
    cwd: &Path,
) -> String {
    if retry_attempt == 0 {
        let initial = make_session_uuid(sha, &format!("{}{}", suffix_prefix, iter_idx));
        if !claude_session_in_use(cwd, &initial) {
            return initial;
        }
    }
    for _ in 0..64 {
        let uuid = make_random_session_uuid();
        if !claude_session_in_use(cwd, &uuid) {
            return uuid;
        }
    }
    // Degenerate fallback: return whatever we get and let claude's own check
    // reject it if there's still a collision.
    make_random_session_uuid()
}

/// Generate a pseudo-random UUID from process id + nanosecond clock. Not
/// cryptographically random (we have no `rand` dep), but more than enough to
/// avoid collisions in this single-user, sequentially-invoked tool.
fn make_random_session_uuid() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    // Two independent 64-bit mixes so both halves of the UUID vary per call.
    let lo = nanos
        ^ pid.wrapping_mul(0x9E37_79B9_7F4A_7C15_u128)
        ^ (pid << 64);
    let hi = nanos.rotate_left(37)
        ^ pid.wrapping_mul(0xBF58_476D_1CE4_E5B9_u128)
        ^ (pid << 32);
    let mixed = lo ^ hi.rotate_left(17);
    let hex = format!("{:032x}", mixed);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32]
    )
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
    p.push_str("  [DONE]     Bug was real but is ALREADY FIXED in the current tree\n");
    p.push_str("             (by a later commit or a larger refactor). Do not apply.\n");
    p.push_str("  [WIRE]     Deletes a type/field/fn that should be WIRED into the\n");
    p.push_str("             public API (api.json) instead of removed.\n");
    p.push_str("  [REFACTOR] Intent right, execution off — needs a custom instruction.\n");
    p.push_str("             Also use this when the fix is correct but the style is\n");
    p.push_str("             stale vs. current code (deep `if let` ladders, huge\n");
    p.push_str("             functions, imperative loops that should be iterator chains).\n");
    p.push_str("  [REJECT]   Incorrect or harmful — do not apply. Includes cases where\n");
    p.push_str("             the mid-level fix ITSELF is buggy (wrong loop bound, backwards\n");
    p.push_str("             condition, deleted code that's actually reachable).\n");
    p.push_str("  [UNCLEAR]  Can't tell — user should inspect.\n\n");

    if !is_refinement {
        p.push_str("How to analyze (first pass). These reviews are MONTHS old — many of the\n");
        p.push_str("performance-fix commits have already been applied in some form, and the\n");
        p.push_str("surrounding code has moved. Treat the report as a FIRST GUIDE, not a\n");
        p.push_str("literal recipe. Do all of the following before classifying:\n\n");
        p.push_str("  1. Read the commit diff below.\n");
        p.push_str("  2. Read the paired review report (context for WHY).\n");
        p.push_str("  3. Is the bug still present? Grep/Read the current tree for the\n");
        p.push_str("     pattern the commit fixed. If the buggy code no longer exists or\n");
        p.push_str("     a later commit already patched it, classify as [DONE].\n");
        p.push_str("  4. Look at `git log -p -- <file>` for the affected lines: why was\n");
        p.push_str("     the original code written that way? Some \"bugs\" are load-bearing\n");
        p.push_str("     workarounds. Document any context the original diff missed.\n");
        p.push_str("  5. Sanity-check the mid-level fix itself — does it actually fix the\n");
        p.push_str("     bug, or does it introduce a new one? If the fix is wrong, [REJECT].\n");
        p.push_str("  6. Check code style (small functions, early-return, no deep `if let`\n");
        p.push_str("     ladders, iterator chains where idiomatic). If the intent is right\n");
        p.push_str("     but the style clashes with current code, [REFACTOR] with\n");
        p.push_str("     specific guidance.\n");
        p.push_str("  7. For each deletion, Grep/Read to check whether the deleted name\n");
        p.push_str("     is referenced elsewhere. Check api.json for public-API wiring.\n");
        p.push_str("  8. Output a SHORT recommendation (≤ 10 lines):\n\n");
        p.push_str("       [CATEGORY] <one sentence>\n");
        p.push_str("       Why: <1-2 sentences, citing current-tree evidence>\n");
        p.push_str("       Plan: <bulleted steps the apply-agent should follow, or\n");
        p.push_str("              \"skip — already fixed by <sha>\" for [DONE]>\n");
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
    let iter_idx = plan.iterations.len();
    let session_name = format!("apply {}", short(sha));

    // Ensure the agent uses the rustup toolchain (which has all cross-compile
    // targets installed) rather than Homebrew's cargo, which doesn't.
    let path_with_rustup = rustup_prefixed_path();

    // Use the staged azul-doc binary (copied outside target/ at startup, so
    // `cargo clean` in the agent doesn't wipe it).
    let azul_doc_bin = project_root.join(
        if cfg!(windows) { ".apply-midlevel/azul-doc.exe" } else { ".apply-midlevel/azul-doc" }
    );

    // Retry loop: on session-UUID collision (left over from a crashed/
    // restarted run, or a race), regenerate a fresh UUID and try again.
    let max_retries: u32 = 4;
    let mut attempt: u32 = 0;
    loop {
        let session_uuid = pick_free_session_uuid(sha, "b", iter_idx, attempt, project_root);
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

        println!("  session-id: {}", session_uuid);
        println!("  attach with: claude --resume {}", session_uuid);
        println!();

        match spawn_claude_streaming(
            &cmd_args, prompt.as_bytes(), project_root, &path_with_rustup,
            Some(&[("AZ_DOC_BIN", azul_doc_bin.as_path())]),
        ) {
            Ok(_out) => break,
            Err(ClaudeError::SessionInUse) if attempt < max_retries => {
                attempt += 1;
                println!(
                    "  [retry {}/{}] session UUID was already in use — regenerating...",
                    attempt, max_retries
                );
                continue;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
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
    // collapse everything on top of pre_head into a single squashed commit —
    // EXCEPT when every extra commit is a `follow-up: ` commit, in which case
    // the agent intentionally split the work per STEP 6 of the prompt and we
    // preserve all commits.
    let commit_count = count_commits(project_root, pre_head, &post_head)?;
    let extras_are_followups = extra_commits_are_followups(project_root, pre_head, &post_head)
        .unwrap_or(false);
    let final_head = if (commit_count > 1 && !extras_are_followups) || is_refinement {
        if extras_are_followups && is_refinement {
            // Edge case: post-apply refinement on a commit that had follow-ups.
            // The refinement squash would collapse the follow-ups into the
            // main commit. That's destructive — the user's previous round
            // produced two distinct commits intentionally. Refuse and let the
            // user resolve manually.
            return Err(
                "refinement requested on a commit chain containing follow-up: commits; \
                 collapsing would lose intentional separation. Reset manually and re-run."
                    .into(),
            );
        }
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

/// True iff the range `from..to` has at least 2 commits AND every commit after
/// the first one (the "main" replayed commit) has a subject starting with
/// `follow-up: `. Used to decide whether the agent's multi-commit output was
/// intentional (per STEP 6 of the prompt) and should be preserved, vs. a
/// stray multi-commit that should be squashed.
fn extra_commits_are_followups(
    project_root: &Path,
    from: &str,
    to: &str,
) -> Result<bool, String> {
    let out = Command::new("git")
        .args(["log", "--reverse", "--format=%s", &format!("{}..{}", from, to)])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git log subjects: {}", e))?;
    if !out.status.success() {
        return Ok(false);
    }
    let subjects: Vec<&str> = std::str::from_utf8(&out.stdout)
        .unwrap_or("")
        .lines()
        .collect();
    if subjects.len() < 2 {
        return Ok(false);
    }
    Ok(subjects.iter().skip(1).all(|s| s.starts_with("follow-up: ")))
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
    p.push_str("code-review agent during a mid-level cleanup pass. The repo has moved\n");
    p.push_str("since those reviews were generated — many of the bugs are already fixed\n");
    p.push_str("or the surrounding code has changed shape. Your job is to apply the\n");
    p.push_str("commit's INTENT where it still makes sense, and to PROVE via the CI\n");
    p.push_str("pipeline that nothing breaks on any supported platform.\n\n");
    p.push_str("The paired review report is a FIRST GUIDE, not a literal recipe. Treat\n");
    p.push_str("it the way a senior engineer treats a stale bug report: read it for\n");
    p.push_str("context, then verify against the current code before changing anything.\n\n");

    p.push_str("═══ Concurrent agents — stay in your lane ════════════════════════════\n\n");
    p.push_str("Other azul-doc agents are running on this same checkout in parallel.\n");
    p.push_str("They edit the language-binding codegen scaffolding:\n\n");
    p.push_str("    doc/src/codegen/ir/lang_*/   (per-language IR hand-edits)\n");
    p.push_str("    doc/src/codegen/v2/lang_*/   (per-language generator code)\n");
    p.push_str("    examples/<language>/          (per-language example projects)\n\n");
    p.push_str("YOU work exclusively on the azul-dll core: `core/`, `css/`, `layout/`,\n");
    p.push_str("`dll/`, `api/`, plus `scripts/` and `doc/` paths OUTSIDE the codegen\n");
    p.push_str("subtrees listed above. There is no scope overlap — the codegen agents\n");
    p.push_str("touch language-binding files; you touch the framework they bind to.\n\n");
    p.push_str("Hard rules (violations confuse the other agent and lose its work):\n\n");
    p.push_str("  • NEVER stage, commit, revert, or `git checkout` files in the\n");
    p.push_str("    `lang_*/` or `examples/` subtrees. They aren't yours.\n");
    p.push_str("  • NEVER use `git add -A` or `git add .` — use targeted paths:\n");
    p.push_str("        git add core/ css/ layout/ dll/ api/ scripts/\n");
    p.push_str("        git add doc/src/  # but NOT doc/src/codegen/ir/lang_* or v2/lang_*\n");
    p.push_str("    Or use pathspec exclusions:\n");
    p.push_str("        git add -A ':!doc/src/codegen/ir/lang_*' \\\n");
    p.push_str("                   ':!doc/src/codegen/v2/lang_*' \\\n");
    p.push_str("                   ':!examples'\n");
    p.push_str("  • When `git status --porcelain` shows changes ONLY in those subtrees,\n");
    p.push_str("    treat that as clean for YOUR purposes — that's another agent\n");
    p.push_str("    working. Your STEP 7 invariant \"working tree empty\" applies to\n");
    p.push_str("    YOUR scope only.\n");
    p.push_str("  • If the build fails with a lock-contention error (see STEP 4\n");
    p.push_str("    warning) wait 60s and retry. Don't blame your patch.\n\n");

    p.push_str("═══ STEP 0 — Is the bug still there? ══════════════════════════════════\n\n");
    p.push_str("Before applying anything, verify the bug described in the commit/report\n");
    p.push_str("still exists in the current tree. Mid-level pass commits were often\n");
    p.push_str("performance fixes or style nits, many of which have since been fixed\n");
    p.push_str("in a different form (renamed, rewritten, superseded by a bigger refactor).\n\n");
    p.push_str("  (a) Run `git log --oneline --all -S '<symbol>' -- <file>` on the\n");
    p.push_str("      symbols the commit touches. If a later commit already resolved it,\n");
    p.push_str("      stop — no action to replay. Report \"ALREADY FIXED by <sha>\" and\n");
    p.push_str("      commit nothing.\n");
    p.push_str("  (b) For every chunk of the diff, run `git log -p -- <file>` (or\n");
    p.push_str("      `git blame`) on the surrounding lines to understand WHY the original\n");
    p.push_str("      code looked the way it did. Some \"bugs\" were load-bearing\n");
    p.push_str("      workarounds. The original diff may have missed that context.\n");
    p.push_str("  (c) Sanity-check the mid-level fix itself. The review agent that\n");
    p.push_str("      generated it sometimes proposed changes that were wrong — the\n");
    p.push_str("      wrong loop bound, a backwards condition, deleted code that was\n");
    p.push_str("      actually reachable. If the diff looks off, STOP and write a\n");
    p.push_str("      REFINEMENT note instead of applying it blindly.\n\n");

    p.push_str("═══ STEP 1 — Apply the commit's intent ════════════════════════════════\n\n");
    if user_instruction.is_none() {
        p.push_str("First try a clean cherry-pick:\n\n");
        p.push_str(&format!("    git cherry-pick --no-commit {}\n\n", sha));
        p.push_str("If it applies cleanly: sanity-check the result anyway (per Step 0)\n");
        p.push_str("before continuing. `git cherry-pick` blindly reapplies bytes; it can\n");
        p.push_str("silently revert a later fix or reintroduce a bug in the current tree.\n");
        p.push_str("If the result is wrong, `git cherry-pick --abort` (or `git reset`)\n");
        p.push_str("and skip or refine instead.\n\n");
        p.push_str("If it conflicts:\n");
        p.push_str("    git cherry-pick --abort\n");
        p.push_str("and manually apply the INTENT of the diff below, using the paired\n");
        p.push_str("review report for context about WHY the change was made. While you\n");
        p.push_str("edit, prefer current code style (see STEP 1b) over the literal diff.\n\n");
        p.push_str("STEP 1b — Code style when you hand-edit:\n");
        p.push_str("  • Prefer SMALL functions (< ~60 LoC) with one clear job.\n");
        p.push_str("  • Prefer EARLY-RETURN / guard clauses to nested `if` or match arms.\n");
        p.push_str("  • AVOID deep `if let Some(x) = ... { if let Some(y) = ... { ... } }`\n");
        p.push_str("    ladders — use `let Some(x) = ... else { return ... };` or `?` on\n");
        p.push_str("    `Option`/`Result`, or chain with `.and_then`/`.map`.\n");
        p.push_str("  • Prefer iterator chains (`iter().filter().map().collect()`) over\n");
        p.push_str("    imperative for-loops with accumulator vecs, where equivalent.\n");
        p.push_str("  • Don't reintroduce a style the surrounding code has moved past.\n");
        p.push_str("    Read 20 lines above and below your edit first and match its\n");
        p.push_str("    current idioms.\n");
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
    p.push_str("IMPORTANT — concurrent builds: other azul-doc agents (language-binding\n");
    p.push_str("codegen, etc.) may be using the same `target/` directory in parallel. If\n");
    p.push_str("a cargo command fails with `Blocking waiting for file lock`, `could not\n");
    p.push_str("acquire`, `Text file busy`, or appears to hang on a lock-related message\n");
    p.push_str("(NOT a real compile error pointing at a specific file/line in YOUR diff),\n");
    p.push_str("wait 60 seconds and retry the same cargo command, up to 3 times. DO NOT\n");
    p.push_str("interpret lock contention as a real compile error — your patch is\n");
    p.push_str("innocent. Real errors will name a file from your diff and a specific\n");
    p.push_str("line number; lock errors mention `target/` directory contention only.\n\n");
    p.push_str("CONCURRENT BUILD WARNING: other azul-doc processes (language-binding\n");
    p.push_str("codegen, parallel autoreview, etc.) may be using the same `target/`\n");
    p.push_str("directory. If `cargo` reports any of:\n\n");
    p.push_str("    Blocking waiting for file lock on build directory\n");
    p.push_str("    could not acquire build directory lock\n");
    p.push_str("    text file busy\n");
    p.push_str("    resource temporarily unavailable\n\n");
    p.push_str("…the other process is holding the lock. WAIT 60 seconds and rerun the\n");
    p.push_str("SAME command (up to 3 times). Do NOT interpret these as compile errors\n");
    p.push_str("and do NOT 'fix' imaginary problems in the code. Real compile errors name\n");
    p.push_str("specific files / line numbers in YOUR diff.\n\n");
    p.push_str("IMPORTANT: the azul-doc binary you need for these steps is already built\n");
    p.push_str("and available as `$AZ_DOC_BIN` in your environment. Invoke it directly:\n\n");
    p.push_str("    \"$AZ_DOC_BIN\" autofix\n\n");
    p.push_str("NOT `cargo run -r -p azul-doc -- autofix` — the cargo form would rebuild\n");
    p.push_str("azul-doc after any `cargo clean` you do. `$AZ_DOC_BIN` is the exact\n");
    p.push_str("release binary of the CLI that spawned you, so it's guaranteed current.\n\n");
    p.push_str("  (a) Run autofix + apply in a LOOP until autofix reports\n");
    p.push_str("      `Generated 0 patches`. One pass is not enough — applying patches\n");
    p.push_str("      can surface new inconsistencies that autofix then needs to fix too.\n\n");
    p.push_str("      Loop:\n");
    p.push_str("          \"$AZ_DOC_BIN\" autofix\n");
    p.push_str("          # if the above printed `Generated 0 patches` → break out of loop\n");
    p.push_str("          \"$AZ_DOC_BIN\" patch safe target/autofix/patches\n");
    p.push_str("          \"$AZ_DOC_BIN\" patch      target/autofix/patches\n");
    p.push_str("          # loop back to `autofix`\n\n");
    p.push_str("      Only proceed once autofix produces zero patches.\n\n");
    p.push_str("  (b) Then run the normalize + codegen pass ONCE:\n\n");
    p.push_str("          \"$AZ_DOC_BIN\" normalize\n");
    p.push_str("          \"$AZ_DOC_BIN\" codegen all\n\n");
    p.push_str("After these, if `git status --porcelain` shows changes in YOUR scope\n");
    p.push_str("(see the Concurrent-agents note up top — ignore changes in lang_* and\n");
    p.push_str("examples/), drop any .md, stage your scope, and amend the commit:\n\n");
    p.push_str("    # Scoped add — keeps the other agents' uncommitted work untouched\n");
    p.push_str("    git add -A ':!doc/src/codegen/ir/lang_*' \\\n");
    p.push_str("               ':!doc/src/codegen/v2/lang_*' \\\n");
    p.push_str("               ':!examples'\n");
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

    p.push_str("═══ STEP 6 — Follow-up tasks (if the plan calls for any) ══════════════\n\n");
    p.push_str("Re-read the AGREED PLAN and the USER FEEDBACK HISTORY (both at the end of\n");
    p.push_str("this prompt). If they mention work BEYOND the main fix — phrases like\n");
    p.push_str("\"also do X\", \"then refactor Y\", \"as a follow-up, add Z\", \"after that\n");
    p.push_str("hook up W\", \"save this as the plan\" referring to a wider goal — do that\n");
    p.push_str("work as a SEPARATE second commit. Don't bundle follow-ups into the main\n");
    p.push_str("commit; reviewers should be able to read them independently.\n\n");
    p.push_str("For each follow-up commit:\n");
    p.push_str("  (a) Implement the follow-up work on top of the main commit.\n");
    p.push_str("  (b) Repeat STEP 4 (autofix loop → normalize → codegen) and STEP 5\n");
    p.push_str("      (3-target cross-compile). Same concurrent-build warning applies.\n");
    p.push_str("  (c) Commit with subject prefix `follow-up: ` (literal, with the colon\n");
    p.push_str("      and trailing space). Example:\n\n");
    p.push_str("        follow-up: hook XIMPreeditCallbacks for IME composition\n\n");
    p.push_str("      The body should briefly say WHAT and reference the main commit's\n");
    p.push_str("      subject for context. No `Co-Authored-By`, no `Generated with…` footer.\n");
    p.push_str("  (d) The CLI looks for the literal `follow-up: ` prefix to decide it's\n");
    p.push_str("      intentional (and won't squash it into the main commit). If you\n");
    p.push_str("      forget the prefix it gets squashed away.\n\n");
    p.push_str("If there are NO follow-up tasks in the plan, skip this step. Don't invent\n");
    p.push_str("follow-ups the user didn't ask for.\n\n");

    p.push_str("═══ STEP 7 — Final invariants (you MUST satisfy all of these) ════════\n\n");
    p.push_str("  • HEAD points at the LAST of your new commit(s) (main, plus any\n");
    p.push_str("    `follow-up: ` commits from STEP 6).\n");
    p.push_str("  • `git status --porcelain` shows NO changes IN YOUR SCOPE. Changes\n");
    p.push_str("    in `doc/src/codegen/ir/lang_*`, `doc/src/codegen/v2/lang_*`, or\n");
    p.push_str("    `examples/` are other agents' work — leave them alone, they don't\n");
    p.push_str("    block you. Verify with a scoped check, e.g.:\n");
    p.push_str("        git status --porcelain -- core/ css/ layout/ dll/ api/ scripts/\n");
    p.push_str("    should be empty.\n");
    p.push_str("  • None of YOUR commits touch a .md file.\n");
    p.push_str("  • All 3 cross-compile targets pass `cargo check` at HEAD.\n\n");

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

    p.push_str("\nProceed through all seven steps. Do not stop until the final invariants hold.\n");
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
    // Same scoping as `has_worktree_changes`: ignore staged paths owned by a
    // concurrent agent. They have no business in our commit and aren't ours
    // to unstage either.
    let any_in_scope = String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|p| !is_concurrent_agent_path(p.trim()));
    Ok(!any_in_scope)
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

/// Restore the working tree + branch pointer to `target_sha`, but only for
/// paths in OUR scope. Concurrent codegen-agent uncommitted work
/// (`lang_*`, `examples/`) is left untouched — a global `reset --hard`
/// would silently wipe it and confuse the other agent.
///
/// Implementation: soft-reset the branch pointer (preserves working tree),
/// then for each currently-dirty path that's in our scope, either checkout
/// from `target_sha` (tracked) or delete (untracked). Best-effort; errors
/// on individual paths are logged and skipped so the retry can still proceed.
fn cleanup_our_scope_to(project_root: &Path, target_sha: &str) -> Result<(), String> {
    // 1. Move branch pointer back, preserving worktree contents.
    if let Err(e) = run_git(project_root, &["reset", "--soft", target_sha]) {
        eprintln!("[cleanup] reset --soft failed: {} (continuing)", e);
    }
    // 2. Unstage everything (mixed reset moves index to HEAD = target_sha).
    if let Err(e) = run_git(project_root, &["reset", "HEAD"]) {
        eprintln!("[cleanup] reset HEAD failed: {} (continuing)", e);
    }

    // 3. Walk porcelain status; for in-scope dirty paths, restore or delete.
    let out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git status: {}", e))?;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let (status, path) = match parse_porcelain_line(line) {
            Some(p) => p,
            None => continue,
        };
        if is_concurrent_agent_path(path) {
            continue;
        }
        if status.contains('?') {
            // Untracked — delete (file or empty dir).
            let abs = project_root.join(path);
            let _ = if abs.is_dir() {
                fs::remove_dir_all(&abs)
            } else {
                fs::remove_file(&abs)
            };
        } else {
            // Tracked dirty — checkout from target_sha (using `git checkout
            // <sha> -- <path>` form which both unstages and restores).
            let _ = Command::new("git")
                .args(["checkout", target_sha, "--", path])
                .current_dir(project_root)
                .status();
        }
    }
    Ok(())
}

/// True if a path is owned by a concurrent codegen / language-binding agent.
/// Files in these subtrees are out-of-scope for apply-midlevel — we never
/// stage, revert, or treat them as a dirtiness signal.
fn is_concurrent_agent_path(path: &str) -> bool {
    let p = path.trim_start_matches('"').trim_end_matches('"');
    p.starts_with("doc/src/codegen/ir/lang_")
        || p.starts_with("doc/src/codegen/v2/lang_")
        || p.starts_with("examples/")
}

/// Parse a `git status --porcelain` line into (status_codes, path), normalising
/// rename-style "old -> new" entries to just the new path. Returns None on
/// malformed input.
fn parse_porcelain_line(line: &str) -> Option<(&str, &str)> {
    if line.len() < 4 {
        return None;
    }
    let status = &line[..2];
    let rest = &line[3..];
    let path = rest.split(" -> ").last().unwrap_or(rest);
    Some((status, path.trim_start_matches('"').trim_end_matches('"')))
}

fn has_worktree_changes(project_root: &Path) -> Result<bool, String> {
    let out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("git status: {}", e))?;
    // Only flag dirtiness for paths in OUR scope — concurrent codegen agents'
    // uncommitted work in lang_* / examples/ should not block us.
    let dirty = String::from_utf8_lossy(&out.stdout).lines().any(|line| {
        match parse_porcelain_line(line) {
            Some((_, path)) => !is_concurrent_agent_path(path),
            None => false,
        }
    });
    Ok(dirty)
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
