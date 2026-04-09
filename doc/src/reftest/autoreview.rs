//! Automated code-review pipeline.
//!
//! Discovers source files, dispatches Claude agents (read-only, no worktrees)
//! to review each file, produces quality reports, merges them into a checklist,
//! and optionally processes the checklist to implement improvements.

use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::spec::executor::{
    self, AgentResult, SHUTDOWN_REQUESTED,
};

// ── Configuration ──────────────────────────────────────────────────────

pub struct AutoreviewConfig {
    pub project_root: PathBuf,
    pub agents: usize,
    pub timeout: Duration,
    pub model: Option<String>,
    pub file_filter: Option<String>,
    pub retry_failed: bool,
    pub dry_run: bool,
    pub status_only: bool,
    pub collect_only: bool,
    pub cleanup: bool,
    pub subcommand: AutoreviewSubcommand,
}

pub enum AutoreviewSubcommand {
    Run,
    Merge,
    Process,
}

// ── Output directory layout ────────────────────────────────────────────

fn output_dir(project_root: &Path) -> PathBuf {
    project_root.join("doc/target/autoreview")
}

fn prompts_dir(project_root: &Path) -> PathBuf {
    output_dir(project_root).join("prompts")
}

fn reports_dir(project_root: &Path) -> PathBuf {
    output_dir(project_root).join("reports")
}

fn merge_dir(project_root: &Path) -> PathBuf {
    output_dir(project_root).join("merge")
}

fn process_prompts_dir(project_root: &Path) -> PathBuf {
    output_dir(project_root).join("process/prompts")
}

// ── File discovery ─────────────────────────────────────────────────────

/// Walk a directory tree collecting `.rs` files (relative to `root`).
fn walk_rs_files(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('.') || name == "target" {
                continue;
            }
            walk_rs_files(&path, root, out);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_path_buf());
            }
        }
    }
}

/// Discover source files eligible for review.
fn discover_source_files(config: &AutoreviewConfig) -> Vec<PathBuf> {
    let root = &config.project_root;
    let mut files = Vec::new();

    for dir in &["core/src", "css/src", "layout/src", "dll/src", "shell/src"] {
        let full = root.join(dir);
        if full.is_dir() {
            walk_rs_files(&full, root, &mut files);
        }
    }

    if let Some(ref filter) = config.file_filter {
        files.retain(|p| p.to_string_lossy().contains(filter.as_str()));
    }

    files.sort();
    files
}

/// Convert a relative path like `core/src/dom.rs` to a safe flat name
/// like `core__src__dom`.
fn path_to_safe_name(rel_path: &Path) -> String {
    let s = rel_path.to_string_lossy();
    let s = s.strip_suffix(".rs").unwrap_or(&s);
    s.replace('/', "__")
}

// ── Prompt builders ────────────────────────────────────────────────────

fn build_review_prompt(
    file_path: &Path,      // relative, e.g. "core/src/dom.rs"
    project_root: &Path,
    report_abs_path: &Path, // absolute path for the output report
) -> String {
    // List existing doc/guide files so agent knows what system docs exist.
    let guide_dir = project_root.join("doc/guide");
    let mut guide_files: Vec<String> = Vec::new();
    if guide_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&guide_dir) {
            for e in entries.flatten() {
                if e.path().extension().map(|x| x == "md").unwrap_or(false) {
                    guide_files.push(e.file_name().to_string_lossy().to_string());
                }
            }
        }
    }
    guide_files.sort();
    let guide_listing = if guide_files.is_empty() {
        "(none)".to_string()
    } else {
        guide_files.join(", ")
    };

    let file_display = file_path.display();
    let report_display = report_abs_path.display();

    format!(
r#"# Code Review: {file_display}

You are reviewing the source file `{file_display}` in the Azul GUI toolkit codebase.
Your job is to read it, use tools to verify every claim, and write a structured
review report.  Do NOT modify any source files — only produce the report.

Write the report to: `{report_display}`

---

## Review Checklist

Work through every item below.  For each one, **use Grep / Glob / Read to
verify** before writing your finding.

### 1. Duplicated Functionality
For each public function, search the rest of the codebase for functions with
similar names or logic.  Note any that should be consolidated.

### 2. Missing or Outdated Documentation
Check every public struct, enum, function, constant, and type alias.
Types exposed in the public API especially need doc comments.
Flag items with no docs, and docs that no longer match the code.

### 3. Module-Level Documentation (`//!`)
Check whether the file has a `//!` module doc comment at the top.
Every module should have a brief `//!` block explaining:
- What this module is responsible for
- Key types / entry points it exports
- How it fits into the larger crate (what depends on it, what it depends on)
Flag files with no module doc, or with a module doc that doesn't match
what the file actually contains.

### 4. File Size
- Under ~50 non-blank lines of real code → suggest merging with a related module.
- Over ~3000-4000 lines → *may* benefit from splitting, but only if the file
  mixes unrelated concerns.  A large file is fine if its functions are small and
  cohesive (e.g. `fc.rs`).  Do NOT suggest splitting just because a file is long.

### 5. Outdated Comments
For each comment that references a specific item, verify the reference still
exists and is accurate.

### 6. Obvious Bugs
Logic errors, bad unwraps, off-by-one, incorrect type conversions, race
conditions, unsafe misuse.

Also specifically check for:
- **Stub code**: `todo!()`, `unimplemented!()`, `"in a real implementation"`,
  `"placeholder"`, `"dummy"`, `"stub"` — anything that was never finished.
- **Hard-coded magic numbers**: numeric literals that should be named constants
  or ideally configurable via a parameter / config struct.
- **`..Default::default()`** in struct construction — this is a common source of
  bugs because it silently zero-initialises fields that may need real values.
  For every `..Default::default()` usage, check the `Default` impl and verify
  the defaulted fields are genuinely safe to leave at their default.

### 7. Dead Code / Unwired Systems
For each public function and type, grep for call sites outside its own module.
If zero results, flag it.  Check if the feature the file describes is actually
wired into the rest of the codebase.

### 8. Documentation Verbosity
Docs should be short and to-the-point.  Flag overly verbose doc comments.
**Exception**: `refany.rs` is allowed to be verbose.

### 9. Refactoring Opportunities
Functions over ~100 LOC that should be split; repeated patterns that could be
extracted; missing or unnecessary abstractions.

### 10. Code Style
- Prefer chaining (`iter().filter().map().collect()`)
- Prefer early-return / guard clauses over deep nesting
- Target 60–100 LOC per function; extract sub-functions for longer ones
- **No** `if let Some {{ if {{ if {{ … }} }} }}` towers — flatten with
  early returns, `?`, or `let … else`
- Minimal indentation

### 11. Vibe-Coding Hints & Stub Code
Search for: `FIX:`, `PHASE`, `TODO`, `FIXME`, `HACK`, `STEP X:`,
`in a real implementation`, `placeholder`, `dummy`, `stub`,
`todo!()`, `unimplemented!()`, `0.0 /* */`, `// temporary`.
These often come from AI agents working off phased plans.
Also flag non-functional code samples in comments and placeholder stubs.

Check the `scripts/` directory for any planning documents related to the
file's feature area — these can reveal the original design intent and
whether the current implementation diverged from the plan.

### 12. Compiler Warnings / Unclean Patterns
Look for code that would trigger compiler warnings or clippy lints:
- Direct casts of function pointers to integers (use `as *const ()` first)
- Unused imports, variables, or `#[allow(unused)]` that could be cleaned up
- Unnecessary `unsafe` blocks, or `unsafe` that can be replaced with safe code
- `as` casts that could silently truncate or lose sign — prefer `try_into()`
- Deprecated API usage
- Missing `#[must_use]` on functions that return important values
- Raw pointer arithmetic that could use safer abstractions

### 13. System Documentation Needs
If this file is *part of* a system (event loop, rendering pipeline, layout
solver, text shaping, accessibility, windowing, etc.):
- Name the system this file belongs to.
- Check whether `doc/guide/` already has a document for that system.
  Existing guide files: {guide_listing}
- If no guide exists, add it to the report's System Documentation section.
  Many files will belong to the same undocumented system — that is expected;
  the merge step will consolidate these into a single list.

---

## Rules

- **Read-only**: do NOT use the Edit tool.  Only produce the report via Write.
- If you see `// +spec` comments, note them but NEVER suggest removing them.
- Be specific: include `file.rs:LINE` references and function names.
- For every "unused function" claim, include the Grep command you used and
  confirm zero results.
- Rate each finding: **HIGH** (bugs, dead code, major duplication),
  **MEDIUM** (missing docs, style, refactoring), or **LOW** (minor).

## Report Format

```
# Review: <file path>

## Summary
- Lines: N
- Public functions: N
- Public structs/enums: N
- Findings: X high, Y medium, Z low

## Findings

### [HIGH] Category — brief description
- **Location**: `file.rs:123`
- **Details**: …
- **Evidence**: (grep results, cross-references)
- **Recommendation**: …

### [MEDIUM] …
…

## System Documentation
- System identified: yes / no
- Existing doc: (path or "none")
- Doc needed: (description, or "n/a")
```
"#
    )
}

fn build_merge_prompt(reports_dir: &Path, checklist_path: &Path) -> String {
    let rd = reports_dir.display();
    let cp = checklist_path.display();

    format!(
r#"# Merge Autoreview Reports

Read every `.md` report in `{rd}` and produce a single merged checklist.

**CRITICAL: Do NOT drop any issues.**  Every finding from every report must
appear in the output — either as its own checklist item or merged into a
group that references the source report.  When in doubt, keep it separate.

## Steps
1. Glob for `{rd}/*.md` — read each one.
2. Group *identical* findings across files (e.g. 15 files all missing module
   docs → one entry listing all 15 files).
3. Sort by severity: HIGH → MEDIUM → LOW.
4. For each group, list all affected files and reference the source report(s).
5. Do NOT summarise or paraphrase — keep the concrete details (line numbers,
   function names, evidence) from the original reports.
6. Collect all "System Documentation Needed" entries into a single deduplicated
   section at the end.
7. Write the result to `{cp}`.

## Output format

```
# Autoreview Checklist

Reports analysed: N
Total findings: N (X high, Y medium, Z low)

## HIGH Priority

### 1. [Category] Brief description
- **Files**: file1.rs:123, file2.rs:456, …
- **Details**: what needs to be done
- **Source reports**: report1.md, report2.md
- [ ] Action item (concrete, actionable sentence)

### 2. …

## MEDIUM Priority
…

## LOW Priority
…

## System Documentation Needed
Deduplicated list of systems that need guide documentation:
- [ ] `doc/guide/xxx.md` — system name — which files implement it
…

## Architecture Notes
Cross-cutting observations, suggested module reorganisations, etc.
```

One line per `- [ ]` action item.  The checklist may be long — that is fine.
"#
    )
}

fn build_process_prompt(checklist_path: &Path) -> String {
    let cp = checklist_path.display();

    format!(
r#"# Autoreview: Process Checklist

Read the checklist at `{cp}` and implement improvements one task at a time.

## Workflow

1. Read the checklist.
2. For each unchecked `- [ ]` item, starting from the top (highest priority):
   a. Read the relevant source files.
   b. **Bug fix?** → Write a minimal test that demonstrates the bug first
      (red), then fix (green).  If a proper test isn't feasible, at least
      describe the reproduction in the commit message.
   c. **Refactoring?** → Use Grep to find every call site before changing
      any signature.  Update all callers.
   d. **Documentation?** → Add or update doc comments / guide files.
   e. Create exactly **one commit** for this item.
3. Move to the next item.  Skip items that need more context than you have.

## Commit message format

```
category: brief description

Details.
Autoreview item: #N
```

Categories: `fix`, `refactor`, `docs`, `cleanup`, `style`

## Rules

- One commit per checklist item — do NOT batch.
- Never edit or remove `// +spec` comments.
- Do NOT run `cargo build` or `cargo test`.
- Keep changes minimal and focused.
"#
    )
}

// ── Review-agent runner (no worktree) ──────────────────────────────────

/// Spawn a single Claude CLI process against the project root (read-only)
/// to review one file.  Returns when the agent finishes or times out.
fn run_review_agent(
    project_root: &Path,
    slot_index: usize,
    prompt_path: &Path,
    timeout: Duration,
    model: Option<&str>,
    on_progress: &dyn Fn(&str),
) -> AgentResult {
    let taken_path  = prompt_path.with_extension("md.taken");
    let result_path = prompt_path.with_extension("md.result");
    let done_path   = prompt_path.with_extension("md.done");
    let failed_path = prompt_path.with_extension("md.failed");

    // Read prompt text
    let prompt_text = match fs::read_to_string(prompt_path) {
        Ok(c) => c,
        Err(e) => return AgentResult {
            success: false, patches: 0,
            error: Some(format!("Failed to read prompt: {}", e)),
        },
    };

    // Open result file
    let result_file = match fs::File::create(&result_path) {
        Ok(f) => f,
        Err(e) => return AgentResult {
            success: false, patches: 0,
            error: Some(format!("Failed to create .result file: {}", e)),
        },
    };
    let result_file_err = match result_file.try_clone() {
        Ok(f) => f,
        Err(e) => return AgentResult {
            success: false, patches: 0,
            error: Some(format!("Failed to clone file handle: {}", e)),
        },
    };

    // Build CLI args — block editing, compilation, and MCP tools.
    let mut cmd_args: Vec<&str> = vec![
        "-p",
        "--dangerously-skip-permissions",
        "--verbose",
        "--output-format", "stream-json",
        // Review agents must not edit source
        "--disallowedTools", "Edit",
        "--disallowedTools", "NotebookEdit",
        // Block compilation
        "--disallowedTools", "Bash(cargo *)",
        "--disallowedTools", "Bash(rustc *)",
        "--disallowedTools", "Bash(clang *)",
        "--disallowedTools", "Bash(gcc *)",
        "--disallowedTools", "Bash(make *)",
        "--disallowedTools", "Bash(cmake *)",
        // Block MCP / LSP leaks
        "--disallowedTools", "mcp__*",
        "--disallowedTools", "rust-analyzer-lsp",
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
        Err(e) => return AgentResult {
            success: false, patches: 0,
            error: Some(format!("Failed to spawn claude: {}", e)),
        },
    };

    let pid = child.id();
    if let Err(e) = executor::write_taken_file(&taken_path, slot_index, pid) {
        let _ = child.kill();
        return AgentResult { success: false, patches: 0, error: Some(e) };
    }

    // Send prompt via stdin then close
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt_text.as_bytes());
    }

    let progress_path = prompt_path.with_extension("md.progress");

    // Poll loop
    let start = Instant::now();
    let exit_status = loop {
        if SHUTDOWN_REQUESTED.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(&taken_path);
            let _ = fs::remove_file(&progress_path);
            return AgentResult {
                success: false, patches: 0,
                error: Some("Shutdown requested".into()),
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
                            timeout.as_secs(), slot_index, partial,
                        ),
                    );
                    return AgentResult {
                        success: false, patches: 0,
                        error: Some("Timeout".into()),
                    };
                }
                let elapsed = start.elapsed().as_secs();
                let activity = executor::read_stream_json_activity(&result_path);
                let status_line = format!(
                    "{}:{:02} | {}",
                    elapsed / 60, elapsed % 60,
                    if activity.is_empty() { "working..." } else { &activity },
                );
                on_progress(&status_line);
                let _ = fs::write(&progress_path, &status_line);
                std::thread::sleep(Duration::from_secs(2));
            }
            Err(e) => {
                let _ = fs::remove_file(&taken_path);
                let _ = fs::remove_file(&progress_path);
                let _ = fs::write(&failed_path, format!("Wait error: {}\n", e));
                return AgentResult {
                    success: false, patches: 0,
                    error: Some(format!("Wait error: {}", e)),
                };
            }
        }
    };

    let _ = fs::remove_file(&progress_path);
    let _ = fs::remove_file(&taken_path);

    if !exit_status.success() {
        let code = exit_status.code().unwrap_or(-1);
        let result_content = executor::extract_result_text(&result_path);
        let elapsed = start.elapsed();
        let _ = fs::write(
            &failed_path,
            format!(
                "Agent exited with code {}\nelapsed_secs={}\nslot={}\n\n--- AGENT OUTPUT ---\n{}",
                code, elapsed.as_secs(), slot_index, result_content,
            ),
        );
        return AgentResult {
            success: false, patches: 0,
            error: Some(format!("Exit code {}", code)),
        };
    }

    // Success
    let result_content = executor::extract_result_text(&result_path);
    let elapsed = start.elapsed();
    let _ = fs::write(
        &done_path,
        format!(
            "action=REVIEWED\nslot={}\nelapsed_secs={}\n\n--- AGENT OUTPUT ---\n{}",
            slot_index, elapsed.as_secs(), result_content,
        ),
    );

    AgentResult { success: true, patches: 0, error: None }
}

// ── Dispatch ───────────────────────────────────────────────────────────

fn dispatch_review_agents(config: &AutoreviewConfig) -> Result<(), String> {
    let project_root = &config.project_root;
    let prompts = prompts_dir(project_root);

    let (pending, done, failed, taken) =
        executor::scan_prompts_dir(&prompts, config.retry_failed);

    println!(
        "Prompt status: {} pending, {} done, {} failed, {} in-progress",
        pending.len(), done, failed, taken,
    );

    if pending.is_empty() {
        println!("No pending prompts to process.");
        return Ok(());
    }

    let agent_count = config.agents.min(pending.len());
    println!("Launching {} concurrent review agents...\n", agent_count);

    executor::install_sigint_handler();

    // Background thread: kill cargo / rustc / rust-analyzer processes every 5s.
    // Even with --disallowedTools, rust-analyzer (from the user's IDE) or
    // agent sub-processes can trigger builds that lock up the machine when
    // dozens of agents run in parallel.
    let kill_loop = std::thread::spawn(|| {
        while !SHUTDOWN_REQUESTED.load(Ordering::Relaxed) {
            for proc in &["cargo", "rustc", "rust-analyzer", "ra-multiplex", "ra_multiplex"] {
                let _ = Command::new("pkill").arg("-9").arg("-f").arg(proc).output();
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });

    let work_queue: Arc<Mutex<VecDeque<PathBuf>>> =
        Arc::new(Mutex::new(pending.into_iter().collect()));

    let spinner = nanospinner::MultiSpinner::new().start();
    let slot_spinners: Vec<_> = (0..agent_count)
        .map(|i| spinner.add(format!("[AGENT {:03}] idle", i)))
        .collect();

    let results: Arc<Mutex<Vec<(String, AgentResult)>>> =
        Arc::new(Mutex::new(Vec::new()));

    let mut handles = Vec::with_capacity(agent_count);

    for (slot_idx, line) in slot_spinners.into_iter().enumerate() {
        let work_queue = Arc::clone(&work_queue);
        let results = Arc::clone(&results);
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

                line.update(format!("[AGENT {:03}] {}", slot_idx, name));

                let result = run_review_agent(
                    &project_root,
                    slot_idx,
                    &prompt_path,
                    timeout,
                    model.as_deref(),
                    &|status| {
                        line.update(format!(
                            "[AGENT {:03}] {} | {}", slot_idx, name, status,
                        ));
                    },
                );

                let msg = if result.success {
                    format!("{}: done", name)
                } else {
                    format!("{}: FAILED ({})",
                        name, result.error.as_deref().unwrap_or("unknown"))
                };
                line.update(format!("[AGENT {:03}] {}", slot_idx, msg));

                results.lock().unwrap().push((name, result));
            }
            line.update(format!("[AGENT {:03}] finished", slot_idx));
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.join();
    }

    // Stop the kill loop
    SHUTDOWN_REQUESTED.store(true, Ordering::Relaxed);
    let _ = kill_loop.join();
    SHUTDOWN_REQUESTED.store(false, Ordering::Relaxed);

    let results = results.lock().unwrap();
    let total = results.len();
    let success = results.iter().filter(|(_, r)| r.success).count();

    println!("\n\nReview agent execution complete:");
    println!("  Total:   {}", total);
    println!("  Success: {}", success);
    println!("  Failed:  {}", total - success);

    Ok(())
}

// ── Subcommand: run (review) ───────────────────────────────────────────

fn run_review(config: AutoreviewConfig) -> Result<(), String> {
    let project_root = config.project_root.clone();

    if config.cleanup {
        println!("Cleaning up autoreview worktrees...");
        executor::cleanup_worktrees_autoreview(&project_root)?;
        return Ok(());
    }

    if config.status_only {
        return show_status(&project_root, config.retry_failed);
    }

    preflight_checks(&project_root, config.dry_run)?;

    // Ensure directories
    for d in &[
        output_dir(&project_root),
        prompts_dir(&project_root),
        reports_dir(&project_root),
        merge_dir(&project_root),
    ] {
        fs::create_dir_all(d)
            .map_err(|e| format!("Failed to create {}: {}", d.display(), e))?;
    }

    // Phase 1: discover files
    println!("\n=== Phase 1: Discovering source files ===\n");
    let source_files = discover_source_files(&config);

    if source_files.is_empty() {
        println!("No source files found matching the filter.");
        return Ok(());
    }

    println!("Found {} source files to review", source_files.len());
    if let Some(ref f) = config.file_filter {
        println!("  Filter: {}", f);
    }

    // Phase 2: generate prompts
    println!("\n=== Phase 2: Generating review prompts ===\n");
    let prompts = prompts_dir(&project_root);
    let reports = reports_dir(&project_root);
    let mut prompt_count = 0;

    for file_path in &source_files {
        let safe = path_to_safe_name(file_path);
        let prompt_path = prompts.join(format!("{}.md", safe));
        let report_path = reports.join(format!("{}.md", safe));

        let status = executor::classify_prompt(&prompt_path, config.retry_failed);
        match status {
            executor::PromptStatus::Done | executor::PromptStatus::Taken { .. } => continue,
            _ => {}
        }

        let text = build_review_prompt(file_path, &project_root, &report_path);
        fs::write(&prompt_path, &text)
            .map_err(|e| format!("Failed to write prompt: {}", e))?;
        prompt_count += 1;
    }

    println!("Generated {} prompts in {}", prompt_count, prompts.display());

    if config.dry_run {
        println!("\n--dry-run: stopping after prompt generation.");
        return Ok(());
    }

    // Phase 3: dispatch agents
    println!("\n=== Phase 3: Dispatching review agents ===\n");
    dispatch_review_agents(&config)?;

    // Phase 4: summary
    println!("\n=== Phase 4: Summary ===\n");
    show_status(&project_root, false)?;

    let report_count = count_md_files(&reports);
    println!("\nReview reports written: {}", report_count);
    println!("Reports directory: {}", reports.display());
    println!("\nNext step: run `azul-doc autoreview merge` to create the merged checklist.");

    Ok(())
}

// ── Subcommand: merge ──────────────────────────────────────────────────

fn run_merge(config: AutoreviewConfig) -> Result<(), String> {
    let project_root = &config.project_root;
    let reports = reports_dir(project_root);
    let merge = merge_dir(project_root);

    fs::create_dir_all(&merge)
        .map_err(|e| format!("Failed to create merge dir: {}", e))?;

    if !reports.is_dir() {
        return Err("No reports directory. Run `autoreview` first.".into());
    }

    let n = count_md_files(&reports);
    if n == 0 {
        return Err("No report files found. Run `autoreview` first.".into());
    }
    println!("Found {} report files to merge", n);

    preflight_checks(project_root, config.dry_run)?;

    let checklist_path = merge.join("checklist.md");
    let prompt_text = build_merge_prompt(&reports, &checklist_path);

    let merge_prompt_path = merge.join("merge_prompt.md");
    fs::write(&merge_prompt_path, &prompt_text)
        .map_err(|e| format!("Failed to write merge prompt: {}", e))?;

    if config.dry_run {
        println!("--dry-run: merge prompt written to {}", merge_prompt_path.display());
        return Ok(());
    }

    println!("\nDispatching merge agent...\n");

    let spinner = nanospinner::MultiSpinner::new().start();
    let line = spinner.add("[MERGE] working...".to_string());

    // Re-use run_review_agent — the merge agent is also read-only
    // (it reads reports and writes one checklist file).
    let result = run_review_agent(
        project_root,
        0,
        &merge_prompt_path,
        config.timeout,
        config.model.as_deref(),
        &|status| { line.update(format!("[MERGE] {}", status)); },
    );

    if result.success {
        line.update("[MERGE] done".to_string());
        if checklist_path.exists() {
            println!("\n\nMerge complete!");
            println!("Checklist: {}", checklist_path.display());
            println!("\nNext step: run `azul-doc autoreview process` to implement improvements.");
        } else {
            println!("\n\nMerge agent finished but checklist was not created at expected path.");
            println!("Check agent output: {}", merge_prompt_path.with_extension("md.result").display());
        }
    } else {
        line.update(format!(
            "[MERGE] FAILED: {}", result.error.as_deref().unwrap_or("unknown"),
        ));
        return Err("Merge agent failed".into());
    }

    Ok(())
}

// ── Subcommand: process ────────────────────────────────────────────────

fn run_process(config: AutoreviewConfig) -> Result<(), String> {
    let project_root = &config.project_root;
    let checklist_path = merge_dir(project_root).join("checklist.md");

    if !checklist_path.exists() {
        return Err("No checklist found. Run `autoreview merge` first.".into());
    }

    preflight_checks(project_root, config.dry_run)?;

    let prompt_text = build_process_prompt(&checklist_path);

    let pdir = process_prompts_dir(project_root);
    fs::create_dir_all(&pdir)
        .map_err(|e| format!("Failed to create process dir: {}", e))?;

    let process_prompt_path = pdir.join("process.md");
    fs::write(&process_prompt_path, &prompt_text)
        .map_err(|e| format!("Failed to write process prompt: {}", e))?;

    if config.dry_run {
        println!("--dry-run: process prompt written to {}", process_prompt_path.display());
        return Ok(());
    }

    println!("\nDispatching process agent (this will create commits)...\n");

    // The process agent needs to edit code, so we use a worktree so that
    // commits don't land directly on the user's branch until reviewed.
    let slots = executor::create_worktree_pool_autoreview(project_root, 1)?;
    let base_sha = executor::get_head_sha(project_root)?;
    println!("Base SHA: {}", &base_sha[..12]);

    executor::install_sigint_handler();

    let spinner = nanospinner::MultiSpinner::new().start();
    let line = spinner.add("[PROCESS] working...".to_string());

    let process_timeout = Duration::from_secs(3600).max(config.timeout);

    let result = executor::run_agent_in_slot_autoreview(
        &slots[0],
        0,
        &process_prompt_path,
        process_timeout,
        &base_sha,
        config.model.as_deref(),
        &|status| { line.update(format!("[PROCESS] {}", status)); },
    );

    if result.success {
        line.update("[PROCESS] done".to_string());
        println!("\n\nProcess complete!");
        println!("Patches created: {}", result.patches);
        if result.patches > 0 {
            println!(
                "Commits are in worktree: {}",
                slots[0].path.display(),
            );
            println!("Review with: git -C {} log --oneline {}..HEAD",
                slots[0].path.display(), &base_sha[..12]);
            println!("Collect with: azul-doc autoreview --collect");
        }
    } else {
        line.update(format!(
            "[PROCESS] FAILED: {}", result.error.as_deref().unwrap_or("unknown"),
        ));
        return Err("Process agent failed".into());
    }

    Ok(())
}

// ── Collect patches from process worktree ──────────────────────────────

fn collect_process_patches(project_root: &Path) -> Result<(), String> {
    let pdir = process_prompts_dir(project_root);
    if !pdir.exists() {
        return Err("No process directory. Run `autoreview process` first.".into());
    }

    let patches_dir = output_dir(project_root).join("patches");
    fs::create_dir_all(&patches_dir)
        .map_err(|e| format!("Failed to create patches dir: {}", e))?;

    let mut collected = 0usize;
    if let Ok(entries) = fs::read_dir(&pdir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.to_string_lossy().contains(".md.done.")
                && p.extension().map(|e| e == "patch").unwrap_or(false)
            {
                let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                let dest = patches_dir.join(&name);
                if !dest.exists() {
                    fs::copy(&p, &dest)
                        .map_err(|e| format!("Failed to copy patch: {}", e))?;
                    collected += 1;
                }
            }
        }
    }

    println!("Collected {} patches to {}", collected, patches_dir.display());
    Ok(())
}

// ── Status ─────────────────────────────────────────────────────────────

fn show_status(project_root: &Path, retry_failed: bool) -> Result<(), String> {
    let prompts = prompts_dir(project_root);
    if !prompts.exists() {
        println!("No autoreview prompts directory. Run `autoreview` first.");
        return Ok(());
    }

    let (pending, done, failed, taken) =
        executor::scan_prompts_dir(&prompts, retry_failed);

    let total = pending.len() + done + failed + taken;
    println!("Autoreview Status");
    println!("=================\n");
    println!("  Total prompts:  {}", total);
    println!("  Done:           {} ({:.0}%)", done,
        if total > 0 { done as f64 / total as f64 * 100.0 } else { 0.0 });
    println!("  Failed:         {}", failed);
    println!("  In-progress:    {}", taken);
    println!("  Pending:        {}", pending.len());

    let reports = reports_dir(project_root);
    if reports.is_dir() {
        println!("\n  Reports: {}", count_md_files(&reports));
    }

    let checklist = merge_dir(project_root).join("checklist.md");
    if checklist.exists() {
        println!("  Checklist: {}", checklist.display());
    }

    // Detail failed / in-progress
    if failed > 0 || taken > 0 {
        println!();
        if let Ok(entries) = fs::read_dir(&prompts) {
            for entry in entries.flatten() {
                let p = entry.path();
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
                if ext == "taken" {
                    let c = fs::read_to_string(&p).unwrap_or_default();
                    println!("  IN-PROGRESS: {} ({})", name, c.trim());
                } else if ext == "failed" {
                    let c = fs::read_to_string(&p).unwrap_or_default();
                    let first = c.lines().next().unwrap_or("unknown");
                    println!("  FAILED: {} — {}", name, first);
                }
            }
        }
    }

    Ok(())
}

// ── Preflight ──────────────────────────────────────────────────────────

fn preflight_checks(project_root: &Path, dry_run: bool) -> Result<(), String> {
    println!("Preflight checks");
    println!("================\n");

    if !dry_run {
        if std::env::var("CLAUDECODE").is_ok() {
            return Err(
                "Cannot run inside a Claude Code session.\n\
                 Run from a regular terminal:\n\n\
                 ./target/release/azul-doc autoreview"
                    .into(),
            );
        }
        println!("  [OK] Not running inside Claude Code");

        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            return Err(
                "ANTHROPIC_API_KEY is set.\n\
                 Unset it first: unset ANTHROPIC_API_KEY"
                    .into(),
            );
        }
        println!("  [OK] No ANTHROPIC_API_KEY set");

        match Command::new("claude").arg("--version").output() {
            Ok(o) if o.status.success() => {
                let v = String::from_utf8_lossy(&o.stdout);
                println!("  [OK] claude CLI: {}", v.trim());
            }
            _ => return Err("claude CLI not found.".into()),
        }
    } else {
        println!("  [SKIP] Agent checks skipped (--dry-run)");
    }

    println!("  [OK] Working directory: {}", project_root.display());
    println!();
    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────────

fn count_md_files(dir: &Path) -> usize {
    fs::read_dir(dir)
        .into_iter()
        .flat_map(|rd| rd.filter_map(|e| e.ok()))
        .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
        .count()
}

// ── CLI ────────────────────────────────────────────────────────────────

pub fn parse_autoreview_args(
    args: &[&str],
    project_root: &Path,
) -> Result<AutoreviewConfig, String> {
    let mut config = AutoreviewConfig {
        project_root: project_root.to_path_buf(),
        agents: 20,
        timeout: Duration::from_secs(1200),
        model: None,
        file_filter: None,
        retry_failed: false,
        dry_run: false,
        status_only: false,
        collect_only: false,
        cleanup: false,
        subcommand: AutoreviewSubcommand::Run,
    };

    for arg in args {
        match *arg {
            "merge"          => config.subcommand = AutoreviewSubcommand::Merge,
            "process"        => config.subcommand = AutoreviewSubcommand::Process,
            "--retry-failed" => config.retry_failed = true,
            "--dry-run"      => config.dry_run = true,
            "--status"       => config.status_only = true,
            "--collect"      => config.collect_only = true,
            "--cleanup"      => config.cleanup = true,
            _ if arg.starts_with("--agents=") => {
                let n = arg.strip_prefix("--agents=").unwrap();
                config.agents = n.parse()
                    .map_err(|_| format!("Invalid --agents: {}", arg))?;
            }
            _ if arg.starts_with("--timeout=") => {
                let s = arg.strip_prefix("--timeout=").unwrap();
                let secs: u64 = s.parse()
                    .map_err(|_| format!("Invalid --timeout: {}", arg))?;
                config.timeout = Duration::from_secs(secs);
            }
            _ if arg.starts_with("--model=") => {
                config.model = Some(
                    arg.strip_prefix("--model=").unwrap().to_string(),
                );
            }
            _ if arg.starts_with("--file=") => {
                config.file_filter = Some(
                    arg.strip_prefix("--file=").unwrap().to_string(),
                );
            }
            _ if arg.starts_with('-') => {
                return Err(format!("Unknown option: {}", arg));
            }
            _ => { /* ignore positional (e.g. "claude-exec") */ }
        }
    }

    Ok(config)
}

pub fn run_autoreview(config: AutoreviewConfig) -> Result<(), String> {
    // Handle --collect regardless of subcommand
    if config.collect_only {
        return collect_process_patches(&config.project_root);
    }

    match config.subcommand {
        AutoreviewSubcommand::Run     => run_review(config),
        AutoreviewSubcommand::Merge   => run_merge(config),
        AutoreviewSubcommand::Process => run_process(config),
    }
}
