//! Parallel agent executor for W3C spec verification.
//!
//! Dispatches per-paragraph prompts to `claude` CLI processes running in
//! git worktrees. Each agent gets an isolated working copy so they can
//! edit + commit without conflicting with each other.

use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use nanospinner::MultiSpinner;

use super::SpecConfig;

// ── CLI argument parsing ───────────────────────────────────────────────

struct ExecArgs {
    agents: usize,
    timeout_secs: u64,
    retry_failed: bool,
    status_only: bool,
    collect: bool,
    cleanup: bool,
    force_api: bool,
}

fn parse_exec_args(args: &[String]) -> ExecArgs {
    let mut ea = ExecArgs {
        agents: 8,
        timeout_secs: 480,
        retry_failed: false,
        status_only: false,
        collect: false,
        cleanup: false,
        force_api: false,
    };

    for arg in args {
        if let Some(val) = arg.strip_prefix("--agents=") {
            ea.agents = val.parse().unwrap_or(8);
        } else if let Some(val) = arg.strip_prefix("--timeout=") {
            ea.timeout_secs = val.parse().unwrap_or(480);
        } else if arg == "--retry-failed" {
            ea.retry_failed = true;
        } else if arg == "--status" {
            ea.status_only = true;
        } else if arg == "--collect" {
            ea.collect = true;
        } else if arg == "--cleanup" {
            ea.cleanup = true;
        } else if arg == "--force-api" {
            ea.force_api = true;
        }
    }

    ea
}

// ── Worktree management ────────────────────────────────────────────────

struct WorktreeSlot {
    path: PathBuf,
    branch: String,
}

fn worktrees_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".claude-agents")
}

/// Get the current HEAD commit sha of the main workspace.
fn get_head_sha(workspace_root: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(workspace_root)
        .output()
        .map_err(|e| format!("git rev-parse HEAD failed: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "git rev-parse HEAD failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn create_worktree_pool(
    workspace_root: &Path,
    count: usize,
) -> Result<Vec<WorktreeSlot>, String> {
    let base = worktrees_dir(workspace_root);
    fs::create_dir_all(&base)
        .map_err(|e| format!("Failed to create .claude-agents dir: {}", e))?;

    let head_sha = get_head_sha(workspace_root)?;
    let mut slots = Vec::with_capacity(count);

    for i in 0..count {
        let slot_name = format!("slot-{:03}", i);
        let branch = format!("spec-agent-{:03}", i);
        let slot_path = base.join(&slot_name);

        if slot_path.exists() {
            // Already exists — reset branch to main HEAD
            reset_worktree(&slot_path, &head_sha)?;
        } else {
            // Create new worktree with branch at HEAD
            let output = Command::new("git")
                .args(["worktree", "add", "-B", &branch])
                .arg(&slot_path)
                .arg("HEAD")
                .current_dir(workspace_root)
                .output()
                .map_err(|e| format!("git worktree add failed: {}", e))?;

            if !output.status.success() {
                return Err(format!(
                    "git worktree add slot-{:03} failed: {}",
                    i,
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
        }

        slots.push(WorktreeSlot {
            path: slot_path,
            branch,
        });

        println!("  [SLOT {:03}] ready", i);
    }

    Ok(slots)
}

/// Reset a worktree to a specific commit, discarding all local changes.
fn reset_worktree(slot_path: &Path, target_sha: &str) -> Result<(), String> {
    // Reset branch HEAD to target
    let output = Command::new("git")
        .args(["reset", "--hard", target_sha])
        .current_dir(slot_path)
        .output()
        .map_err(|e| format!("git reset --hard failed: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "git reset --hard failed in {}: {}",
            slot_path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Remove untracked files
    let output = Command::new("git")
        .args(["clean", "-fd"])
        .current_dir(slot_path)
        .output()
        .map_err(|e| format!("git clean -fd failed: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "git clean -fd failed in {}: {}",
            slot_path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

fn cleanup_worktrees(workspace_root: &Path) -> Result<(), String> {
    let base = worktrees_dir(workspace_root);
    if !base.exists() {
        println!("No .claude-agents directory found.");
        return Ok(());
    }

    let entries: Vec<_> = fs::read_dir(&base)
        .map_err(|e| format!("Failed to read .claude-agents: {}", e))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();

    for entry in &entries {
        let slot_name = entry.file_name();
        let slot_name_str = slot_name.to_string_lossy();
        println!("  Removing worktree {}...", slot_name_str);

        let output = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(entry.path())
            .current_dir(workspace_root)
            .output()
            .map_err(|e| format!("git worktree remove failed: {}", e))?;

        if !output.status.success() {
            eprintln!(
                "  Warning: could not remove {}: {}",
                slot_name_str,
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    // Prune stale worktree references
    let _ = Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(workspace_root)
        .output();

    // Remove the directory itself if empty
    let _ = fs::remove_dir(&base);

    // Delete spec-agent-* branches left behind by worktrees
    let branch_output = Command::new("git")
        .args(["branch", "--list", "spec-agent-*"])
        .current_dir(workspace_root)
        .output()
        .ok();
    if let Some(output) = branch_output {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let branch = line.trim();
            if branch.is_empty() {
                continue;
            }
            let result = Command::new("git")
                .args(["branch", "-D", branch])
                .current_dir(workspace_root)
                .output();
            match result {
                Ok(o) if o.status.success() => {
                    println!("  Deleted branch {}", branch);
                }
                _ => {
                    eprintln!("  Warning: could not delete branch {}", branch);
                }
            }
        }
    }

    println!("Cleanup complete.");
    Ok(())
}

// ── Prompt status scanning ─────────────────────────────────────────────

#[derive(Debug, Clone)]
enum PromptStatus {
    Done,
    Failed,
    Taken { pid: u32 },
    Pending,
}

fn classify_prompt(prompt_path: &Path, retry_failed: bool) -> PromptStatus {
    let done_path = prompt_path.with_extension("md.done");
    let failed_path = prompt_path.with_extension("md.failed");
    let taken_path = prompt_path.with_extension("md.taken");

    if done_path.exists() {
        return PromptStatus::Done;
    }

    if taken_path.exists() {
        // Check if PID is still alive
        if let Ok(content) = fs::read_to_string(&taken_path) {
            if let Some(pid) = parse_taken_pid(&content) {
                if is_pid_alive(pid) {
                    return PromptStatus::Taken { pid };
                }
            }
        }
        // Stale .taken — clean up
        let _ = fs::remove_file(&taken_path);
        let _ = fs::write(
            &failed_path,
            "Agent crashed or executor was killed (stale .taken file)\n",
        );
    }

    if failed_path.exists() && !retry_failed {
        return PromptStatus::Failed;
    }

    if failed_path.exists() && retry_failed {
        let _ = fs::remove_file(&failed_path);
    }

    PromptStatus::Pending
}

fn parse_taken_pid(content: &str) -> Option<u32> {
    for part in content.split_whitespace() {
        if let Some(pid_str) = part.strip_prefix("pid=") {
            return pid_str.parse().ok();
        }
    }
    None
}

fn is_pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

fn scan_prompts_dir(
    prompts_dir: &Path,
    retry_failed: bool,
) -> (Vec<PathBuf>, usize, usize, usize) {
    let mut pending = Vec::new();
    let mut done_count = 0usize;
    let mut failed_count = 0usize;
    let mut taken_count = 0usize;

    let mut entries: Vec<_> = fs::read_dir(prompts_dir)
        .into_iter()
        .flat_map(|rd| rd.filter_map(|e| e.ok()))
        .filter(|e| {
            let path = e.path();
            path.extension().map(|ext| ext == "md").unwrap_or(false)
                && !path.to_string_lossy().contains(".md.")
        })
        .map(|e| e.path())
        .collect();

    entries.sort();

    for path in entries {
        match classify_prompt(&path, retry_failed) {
            PromptStatus::Done => done_count += 1,
            PromptStatus::Failed => failed_count += 1,
            PromptStatus::Taken { .. } => taken_count += 1,
            PromptStatus::Pending => pending.push(path),
        }
    }

    (pending, done_count, failed_count, taken_count)
}

// ── Prompt building ────────────────────────────────────────────────────

const CODEBASE_CONTEXT: &str = r#"## Codebase Orientation

You are working in the `azul` CSS layout engine repository.
The layout solver is in `layout/src/solver3/`.

### Module structure:
- `mod.rs` — Entry point: `layout_document()`, `LayoutContext`, containing block logic
- `layout_tree.rs` — `LayoutTree`, `LayoutNode` (parent/children/box_props/used_size)
- `geometry.rs` — `BoxProps`, `EdgeSizes`, `IntrinsicSizes`, `UnresolvedBoxProps`
- `sizing.rs` — Width/height calculation, intrinsic size computation
- `getters.rs` — CSS property accessors (resolved values from styled DOM)
- `fc.rs` — Formatting context layout (BFC, IFC, table dispatch)
- `positioning.rs` — Relative/absolute positioning
- `cache.rs` — Incremental layout cache (9+1 slot, Taffy-inspired)
- `taffy_bridge.rs` — Flex/Grid delegation to Taffy library

### Key types:
- Nodes indexed by `usize` into `LayoutTree.nodes: Vec<LayoutNode>`
- `LogicalPosition`, `LogicalSize`, `LogicalRect` — CSS logical units
- `pos_get(positions, idx)` / `pos_set(positions, idx, pos)` — position helpers
- `EdgeSizes { top, right, bottom, left }` — used for margin/padding/border
- `FormattingContext` enum: `Bfc`, `Ifc`, `Table`, `Flex`, `Grid`

### Important patterns:
- `calculated_positions[idx]` stores the **margin-box** position of node idx
- Containing block = **content-box** of parent (margin-box + border + padding)
- Relative positioning applied AFTER layout, absolute positioning AFTER that
- Flex/Grid is handled by Taffy — do NOT modify taffy_bridge.rs
"#;

fn build_agent_instructions(feature_id: &str, spec_tag: &str) -> String {
    format!(
        r#"## Your Task

You MUST leave at least one `// +spec:{spec_tag}` marker comment in the source
code and MUST commit it. This is how we track progress.

### Step 1: Search for existing implementation

```
grep -rn "+spec:{spec_tag}" layout/src/
```

### Step 2: Read the relevant source files

Read the files listed in the codebase orientation above. Understand the
current implementation before making changes.

### Step 3: Scrutinize and implement

Your job is to be a SKEPTICAL REVIEWER. Do not assume the code is correct.
For each requirement in the spec paragraph:

1. Find the EXACT code path that handles this requirement
2. Read the actual logic — does it match what the spec says word-for-word?
3. Check edge cases mentioned in the spec paragraph
4. Look for missing conditions, wrong comparisons, or incomplete handling

Common bugs to look for:
- Spec says "X unless Y" but code only handles X, missing the Y exception
- Spec says "computed value" but code uses "specified value"
- Spec says "content edge" but code uses "border edge" or "margin edge"
- Spec lists multiple conditions but code only checks some of them
- Spec says "all" but code has incomplete match arms

**If the code is MISSING functionality required by this paragraph:**
- Implement it. Use the Edit tool to modify source files.
- Make minimal, focused changes — only what this paragraph requires.
- You MAY add new functions, new match arms, new fields, or new files.
- Do NOT refactor surrounding code or add unrelated improvements.
- Add `// +spec:{spec_tag} - <what this implements>` at each implementation site.

**If the code has a BUG relative to this paragraph's requirements:**
- Fix the bug. Show the before/after logic clearly in the commit message.
- Add `// +spec:{spec_tag} - <what this fixes>` at the fix site.

**If the code correctly implements this paragraph (after careful scrutiny):**
- You MUST still add `// +spec:{spec_tag} - <brief description>` marker
  comments at EVERY implementation site (there may be multiple).
- Find each function/match-arm/code-block that handles this behavior
  and add the marker on the line above.
- This is NOT optional — markers are how we track spec coverage.

### Step 4: ALWAYS commit

You MUST always commit, even if you only added marker comments:
```
git add -A
git commit -m "spec({feature_id}): <short description>"
```

A run with zero commits is considered a FAILURE.

### Output format

After committing, output a brief summary:
- **ACTION**: `IMPLEMENTED` / `FIXED` / `ANNOTATED` (chose one)
- **FILES**: list of files modified
- **DESCRIPTION**: 1-2 sentences explaining what you did

## CRITICAL RULES

- Do NOT run `cargo build`, `cargo test`, `cargo check`, or any compilation
  command. Another process will handle compilation later.
- Make ONLY the changes needed for this one spec paragraph.
- You MUST commit at least once. Zero commits = failure.
- If unsure whether a change is correct, make your best effort.
"#,
        feature_id = feature_id,
        spec_tag = spec_tag,
    )
}

fn build_full_prompt(prompt_path: &Path) -> Result<String, String> {
    let paragraph_content = fs::read_to_string(prompt_path)
        .map_err(|e| format!("Failed to read prompt {}: {}", prompt_path.display(), e))?;

    // Extract feature_id and paragraph number from filename:
    //   "box-model_001.md" → feature_id="box-model", para_num="001"
    //   spec_tag = "box-model-p001"
    let stem = prompt_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let (feature_id, para_num) = match stem.rfind('_') {
        Some(i) => (&stem[..i], &stem[i + 1..]),
        None => (stem, "000"),
    };
    let spec_tag = format!("{}-p{}", feature_id, para_num);

    let mut full_prompt =
        String::with_capacity(CODEBASE_CONTEXT.len() + paragraph_content.len() + 4096);

    full_prompt.push_str(CODEBASE_CONTEXT);
    full_prompt.push('\n');
    full_prompt.push_str(&paragraph_content);
    full_prompt.push('\n');
    full_prompt.push_str(&build_agent_instructions(feature_id, &spec_tag));

    Ok(full_prompt)
}

// ── Agent execution ────────────────────────────────────────────────────

/// Parse stream-json .result file for the most recent activity.
/// Returns e.g. "Read fc.rs", "Edit mod.rs", "Bash", "thinking...", or "".
fn read_stream_json_activity(result_path: &Path) -> String {
    let content = match fs::read_to_string(result_path) {
        Ok(c) if !c.is_empty() => c,
        _ => return String::new(),
    };

    for line in content.lines().rev().take(10) {
        let event: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match event_type {
            "tool_use" => {
                let name = event
                    .pointer("/tool/name")
                    .or_else(|| event.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("tool");
                let file = event
                    .pointer("/tool/input/file_path")
                    .or_else(|| event.pointer("/input/file_path"))
                    .and_then(|f| f.as_str())
                    .map(|f| f.rsplit('/').next().unwrap_or(f));
                return match file {
                    Some(f) => format!("{} {}", name, f),
                    None => name.to_string(),
                };
            }
            "tool_result" => continue, // skip, look further back for the tool_use
            "assistant" => return "thinking...".to_string(),
            _ => continue,
        }
    }

    // Fallback: report file size
    if let Ok(meta) = fs::metadata(result_path) {
        let kb = meta.len() / 1024;
        if kb > 0 {
            return format!("{}KB output", kb);
        }
    }
    String::new()
}

/// Extract the final plain-text result from a stream-json .result file.
/// Looks for the `{"type":"result","result":"..."}` event.
fn extract_result_text(result_path: &Path) -> String {
    let content = match fs::read_to_string(result_path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    // The result event is usually the last line
    for line in content.lines().rev() {
        let event: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if event.get("type").and_then(|t| t.as_str()) == Some("result") {
            if let Some(text) = event.get("result").and_then(|r| r.as_str()) {
                return text.to_string();
            }
        }
    }

    // Fallback: return raw content (maybe not stream-json)
    content
}

/// Check which files the agent has modified in its worktree.
/// Returns e.g. "editing fc.rs (+2 files)" or empty string.
fn check_worktree_activity(slot_path: &Path) -> String {
    let output = Command::new("git")
        .args(["diff", "--name-only"])
        .current_dir(slot_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok();
    let output = match output {
        Some(o) if o.status.success() => o,
        _ => return String::new(),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let files: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    if files.is_empty() {
        return String::new();
    }
    let last = files.last().unwrap();
    let fname = last.rsplit('/').next().unwrap_or(last);
    if files.len() == 1 {
        format!("editing {}", fname)
    } else {
        format!("editing {} (+{} files)", fname, files.len() - 1)
    }
}

struct AgentResult {
    success: bool,
    patches: usize,
    error: Option<String>,
}

fn write_taken_file(taken_path: &Path, slot: usize, pid: u32) -> Result<(), String> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    fs::write(
        taken_path,
        format!("slot={} pid={} started={}\n", slot, pid, now),
    )
    .map_err(|e| format!("Failed to write .taken: {}", e))
}

/// Extract commits the agent made (ahead of `base_sha`) as patch files.
fn extract_patches(
    slot_path: &Path,
    base_sha: &str,
    prompt_path: &Path,
) -> Result<usize, String> {
    // Count commits ahead of base
    let output = Command::new("git")
        .args(["rev-list", "--count", &format!("{}..HEAD", base_sha)])
        .current_dir(slot_path)
        .output()
        .map_err(|e| format!("git rev-list failed: {}", e))?;

    let count_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let commit_count: usize = count_str.parse().unwrap_or(0);

    if commit_count == 0 {
        return Ok(0);
    }

    // Use git format-patch to extract each commit
    let tmp_dir = tempfile::tempdir()
        .map_err(|e| format!("Failed to create temp dir: {}", e))?;

    let range = format!("{}..HEAD", base_sha);
    let output = Command::new("git")
        .args(["format-patch", &range, "--output-directory"])
        .arg(tmp_dir.path())
        .current_dir(slot_path)
        .output()
        .map_err(|e| format!("git format-patch failed: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "git format-patch failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Copy patch files with proper naming
    let mut patch_files: Vec<_> = fs::read_dir(tmp_dir.path())
        .map_err(|e| format!("Failed to read temp dir: {}", e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "patch").unwrap_or(false))
        .collect();

    patch_files.sort();

    let done_path = prompt_path.with_extension("md.done");
    for (i, patch_file) in patch_files.iter().enumerate() {
        let dest = PathBuf::from(format!("{}.{:03}.patch", done_path.display(), i + 1));
        fs::copy(patch_file, &dest)
            .map_err(|e| format!("Failed to copy patch: {}", e))?;
    }

    Ok(patch_files.len())
}

fn run_agent_in_slot(
    slot: &WorktreeSlot,
    slot_index: usize,
    prompt_path: &Path,
    timeout: Duration,
    base_sha: &str,
    on_progress: &dyn Fn(&str),
) -> AgentResult {
    let taken_path = prompt_path.with_extension("md.taken");
    let result_path = prompt_path.with_extension("md.result");
    let done_path = prompt_path.with_extension("md.done");
    let failed_path = prompt_path.with_extension("md.failed");

    // Reset worktree to base commit
    if let Err(e) = reset_worktree(&slot.path, base_sha) {
        return AgentResult {
            success: false,
            patches: 0,
            error: Some(format!("Failed to reset worktree: {}", e)),
        };
    }

    // Build prompt
    let full_prompt = match build_full_prompt(prompt_path) {
        Ok(p) => p,
        Err(e) => {
            return AgentResult {
                success: false,
                patches: 0,
                error: Some(e),
            };
        }
    };

    // Open result file for stdout/stderr capture
    let result_file = match fs::File::create(&result_path) {
        Ok(f) => f,
        Err(e) => {
            return AgentResult {
                success: false,
                patches: 0,
                error: Some(format!("Failed to create .result file: {}", e)),
            };
        }
    };
    let result_file_err = match result_file.try_clone() {
        Ok(f) => f,
        Err(e) => {
            return AgentResult {
                success: false,
                patches: 0,
                error: Some(format!("Failed to clone file handle: {}", e)),
            };
        }
    };

    // Remove CLAUDECODE env var so nested sessions are allowed.
    // --output-format stream-json --verbose: writes one JSON event per line,
    // enabling real-time progress parsing from the .result file.
    let mut child = match Command::new("claude")
        .args([
            "-p",
            "--dangerously-skip-permissions",
            "--verbose",
            "--output-format",
            "stream-json",
        ])
        .env_remove("CLAUDECODE")
        .current_dir(&slot.path)
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
                error: Some(format!("Failed to spawn claude: {}", e)),
            };
        }
    };

    let pid = child.id();

    // Write .taken file
    if let Err(e) = write_taken_file(&taken_path, slot_index, pid) {
        let _ = child.kill();
        return AgentResult {
            success: false,
            patches: 0,
            error: Some(e),
        };
    }

    // Send prompt via stdin, then close pipe
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(full_prompt.as_bytes());
    }

    let progress_path = prompt_path.with_extension("md.progress");

    // Poll for completion with timeout.
    // Every 2s: read tail of .result file (stream-json flushes per event),
    // extract activity summary, write .progress file, update spinner.
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
                error: Some("Shutdown requested".to_string()),
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
                        success: false,
                        patches: 0,
                        error: Some("Timeout".to_string()),
                    };
                }
                let elapsed = start.elapsed().as_secs();
                // Try stream-json activity first, fall back to git diff
                let mut activity = read_stream_json_activity(&result_path);
                if activity.is_empty() {
                    activity = check_worktree_activity(&slot.path);
                }
                let status_line = format!(
                    "{}:{:02} | {}",
                    elapsed / 60,
                    elapsed % 60,
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
                    success: false,
                    patches: 0,
                    error: Some(format!("Wait error: {}", e)),
                };
            }
        }
    };

    let _ = fs::remove_file(&progress_path);

    // Remove .taken
    let _ = fs::remove_file(&taken_path);

    if !exit_status.success() {
        let code = exit_status.code().unwrap_or(-1);
        let result_content = extract_result_text(&result_path);
        let elapsed = start.elapsed();
        let _ = fs::write(
            &failed_path,
            format!(
                "Agent exited with code {}\nelapsed_secs={}\nslot={}\n\n--- AGENT OUTPUT ---\n{}",
                code, elapsed.as_secs(), slot_index, result_content
            ),
        );
        return AgentResult {
            success: false,
            patches: 0,
            error: Some(format!("Exit code {}", code)),
        };
    }

    // Success — extract patches
    let patches = match extract_patches(&slot.path, base_sha, prompt_path) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("  Warning: patch extraction failed: {}", e);
            0
        }
    };

    // Read agent output for the .done file (extract text from stream-json)
    let result_content = extract_result_text(&result_path);
    let elapsed = start.elapsed();

    // Determine action type from output
    let action = if result_content.contains("IMPLEMENTED") {
        "IMPLEMENTED"
    } else if result_content.contains("FIXED") {
        "FIXED"
    } else if result_content.contains("ANNOTATED") {
        "ANNOTATED"
    } else {
        "COMPLETED"
    };

    // Write .done with full details: summary header + full agent output
    let done_content = format!(
        "action={}\npatches={}\nslot={}\nelapsed_secs={}\n\n--- AGENT OUTPUT ---\n{}",
        action, patches, slot_index, elapsed.as_secs(), result_content,
    );
    let _ = fs::write(&done_path, done_content);

    // If 0 patches (agent didn't commit), mark as failed
    if patches == 0 {
        let _ = fs::rename(&done_path, &failed_path);
        let fail_msg = format!(
            "Agent completed but made 0 commits (expected at least 1 annotation commit).\n\
             elapsed={}s\n\n--- AGENT OUTPUT ---\n{}",
            elapsed.as_secs(), result_content,
        );
        let _ = fs::write(&failed_path, fail_msg);
        return AgentResult {
            success: false,
            patches: 0,
            error: Some("Zero commits".to_string()),
        };
    }

    AgentResult {
        success: true,
        patches,
        error: None,
    }
}

// ── Collect (cherry-pick) ──────────────────────────────────────────────

fn collect_patches(workspace_root: &Path) -> Result<(), String> {
    let base = worktrees_dir(workspace_root);
    if !base.exists() {
        return Err("No .claude-agents directory found. Run claude-exec first.".to_string());
    }

    let mut applied = 0usize;
    let mut skipped = 0usize;
    let mut no_changes = 0usize;

    let main_head = get_head_sha(workspace_root)?;

    let mut entries: Vec<_> = fs::read_dir(&base)
        .map_err(|e| format!("Failed to read .claude-agents: {}", e))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let slot_path = entry.path();
        let slot_name = entry.file_name();
        let slot_name_str = slot_name.to_string_lossy().to_string();

        // Get commit hashes on this worktree ahead of main HEAD
        let hash_output = Command::new("git")
            .args([
                "log",
                "--format=%H",
                "--reverse",
                &format!("{}..HEAD", main_head),
            ])
            .current_dir(&slot_path)
            .output()
            .map_err(|e| format!("git log failed for {}: {}", slot_name_str, e))?;

        let hash_text = String::from_utf8_lossy(&hash_output.stdout).to_string();
        let hashes: Vec<&str> = hash_text.trim().lines().collect();

        if hashes.is_empty() || (hashes.len() == 1 && hashes[0].is_empty()) {
            no_changes += 1;
            continue;
        }

        println!(
            "  [{}] {} commits to cherry-pick",
            slot_name_str,
            hashes.len()
        );

        for hash in &hashes {
            let cp_output = Command::new("git")
                .args(["cherry-pick", hash])
                .current_dir(workspace_root)
                .output()
                .map_err(|e| format!("cherry-pick failed: {}", e))?;

            if cp_output.status.success() {
                applied += 1;
            } else {
                let _ = Command::new("git")
                    .args(["cherry-pick", "--abort"])
                    .current_dir(workspace_root)
                    .output();
                let short = if hash.len() >= 8 { &hash[..8] } else { hash };
                eprintln!(
                    "  Conflict cherry-picking {} from {}, skipping",
                    short, slot_name_str
                );
                skipped += 1;
            }
        }
    }

    println!("\nCollect summary:");
    println!("  Applied:    {}", applied);
    println!("  Conflicts:  {}", skipped);
    println!("  No changes: {}", no_changes);

    Ok(())
}

// ── Preflight checks ───────────────────────────────────────────────────

fn preflight_checks(config: &SpecConfig, workspace_root: &Path) -> Result<(), String> {
    println!("Preflight checks");
    println!("================\n");

    // 1. Refuse to run inside an existing Claude Code session
    if std::env::var("CLAUDECODE").is_ok() {
        return Err(
            "Cannot run inside a Claude Code session.\n\
             The executor spawns claude CLI subprocesses which would conflict.\n\
             Run this command from a regular terminal:\n\
             \n\
             ./target/release/azul-doc spec claude-exec"
                .to_string(),
        );
    }
    println!("  [OK] Not running inside Claude Code");

    // 2. Check that ANTHROPIC_API_KEY is NOT set (avoid accidental API billing)
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        return Err(
            "ANTHROPIC_API_KEY is set in environment.\n\
             This would route claude CLI through the paid API instead of your \
             Pro/Max subscription.\n\
             Unset it first:  unset ANTHROPIC_API_KEY\n\
             Or pass --force-api to override this check."
                .to_string(),
        );
    }
    println!("  [OK] No ANTHROPIC_API_KEY set (using subscription plan)");

    // 2. Verify working directory looks like the azul repo
    let solver_dir = workspace_root.join("layout/src/solver3");
    if !solver_dir.is_dir() {
        return Err(format!(
            "Working directory does not look like the azul repo.\n\
             Expected layout/src/solver3/ in: {}",
            workspace_root.display()
        ));
    }
    let git_dir = workspace_root.join(".git");
    if !git_dir.exists() {
        return Err(format!(
            "Not a git repository: {}",
            workspace_root.display()
        ));
    }
    println!("  [OK] Working directory: {}", workspace_root.display());

    // 3. Check that W3C spec files are downloaded
    let spec_dir = &config.spec_dir;
    if !spec_dir.is_dir() {
        return Err(format!(
            "W3C specs not downloaded.\n\
             Run:  azul-doc spec download\n\
             Expected directory: {}",
            spec_dir.display()
        ));
    }
    let spec_count = fs::read_dir(spec_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "html")
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0);
    if spec_count == 0 {
        return Err(format!(
            "No HTML spec files found in {}.\nRun:  azul-doc spec download",
            spec_dir.display()
        ));
    }
    println!("  [OK] {} W3C spec files downloaded", spec_count);

    // 4. Check claude CLI is available
    let claude_check = Command::new("claude")
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    match claude_check {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("  [OK] claude CLI: {}", version);
        }
        _ => {
            return Err(
                "claude CLI not found or not working.\n\
                 Install: https://docs.anthropic.com/en/docs/claude-code"
                    .to_string(),
            );
        }
    }

    // 5. Smoke test: spawn a single claude process with a trivial prompt
    println!("  Smoke test: spawning claude -p ...");
    {
        let mut child = Command::new("claude")
            .args(["-p", "--dangerously-skip-permissions"])
            .env_remove("CLAUDECODE")
            .env_remove("ANTHROPIC_API_KEY")
            .current_dir(workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn claude for smoke test: {}", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(b"Respond with exactly: HELLO");
        }

        let output = child
            .wait_with_output()
            .map_err(|e| format!("Smoke test wait failed: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(format!(
                "Smoke test failed (exit code {}).\n\
                 stdout: {}\n\
                 stderr: {}\n\
                 \n\
                 Make sure `claude` CLI is logged in and working.",
                output.status.code().unwrap_or(-1),
                stdout.chars().take(200).collect::<String>(),
                stderr.chars().take(200).collect::<String>(),
            ));
        }

        let response = String::from_utf8_lossy(&output.stdout);
        println!("  [OK] claude responded: {}", response.trim().chars().take(40).collect::<String>());
    }

    // 6. Rebuild prompts (ensures they're fresh, not stale)
    println!("\n  Rebuilding prompts...\n");
    super::cmd_build_all(config, workspace_root)?;
    println!();

    // 7. Verify prompt count
    let prompts_dir = config.skill_tree_dir.join("prompts");
    let prompt_count = fs::read_dir(&prompts_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| {
                    let p = e.path();
                    p.extension().map(|ext| ext == "md").unwrap_or(false)
                        && !p.to_string_lossy().contains(".md.")
                })
                .count()
        })
        .unwrap_or(0);
    if prompt_count == 0 {
        return Err("No prompt files generated. Check spec download and extraction.".to_string());
    }
    println!("  [OK] {} prompt files ready\n", prompt_count);

    Ok(())
}

// ── Main entry point ───────────────────────────────────────────────────

pub fn run_executor(
    config: &SpecConfig,
    workspace_root: &Path,
    args: &[String],
) -> Result<(), String> {
    let ea = parse_exec_args(args);
    let prompts_dir = config.skill_tree_dir.join("prompts");

    // --cleanup and --collect don't need preflight
    if ea.cleanup {
        return cleanup_worktrees(workspace_root);
    }
    if ea.collect {
        return collect_patches(workspace_root);
    }

    // --status doesn't need preflight either
    if ea.status_only {
        if !prompts_dir.exists() {
            return Err(
                "No prompts directory found. Run `azul-doc spec build-all` first.".to_string(),
            );
        }
        let (pending, done_count, failed_count, taken_count) =
            scan_prompts_dir(&prompts_dir, ea.retry_failed);
        let total = done_count + failed_count + taken_count + pending.len();

        // Scan source for +spec: markers (the real source of truth)
        let found_tags = super::scan_spec_tags(workspace_root);

        println!("Prompt execution status");
        println!("=======================\n");
        println!("  .done files:   {:>4} / {}", done_count, total);
        println!("  .failed files: {:>4} / {}", failed_count, total);
        println!("  .taken files:  {:>4} / {}", taken_count, total);
        println!("  Pending:       {:>4} / {}", pending.len(), total);
        println!("\n  +spec: markers in source: {}", found_tags.len());
        println!(
            "  Execution progress: {:.1}%",
            if total > 0 {
                done_count as f64 / total as f64 * 100.0
            } else {
                0.0
            }
        );
        return Ok(());
    }

    // Full execution — run all preflight checks (including prompt rebuild)
    if !ea.force_api {
        preflight_checks(config, workspace_root)?;
    } else {
        // --force-api: skip API key check but still rebuild prompts
        println!("  [WARN] --force-api: skipping ANTHROPIC_API_KEY check\n");
        super::cmd_build_all(config, workspace_root)?;
    }

    // Scan prompt status (after rebuild)
    let (pending, done_count, failed_count, taken_count) =
        scan_prompts_dir(&prompts_dir, ea.retry_failed);

    let total = done_count + failed_count + taken_count + pending.len();

    if pending.is_empty() {
        println!(
            "No pending prompts. {} done, {} failed.",
            done_count, failed_count
        );
        if failed_count > 0 {
            println!("Use --retry-failed to re-queue failed prompts.");
        }
        return Ok(());
    }

    let agent_count = ea.agents.min(pending.len());
    let timeout = Duration::from_secs(ea.timeout_secs);

    println!("Agent Executor");
    println!("==============\n");
    println!("  Pending prompts: {}", pending.len());
    println!("  Done:            {}", done_count);
    println!("  Failed:          {}", failed_count);
    println!("  Agent slots:     {}", agent_count);
    println!("  Timeout:         {}s", ea.timeout_secs);
    println!();

    // Record base SHA before creating worktrees
    let base_sha = get_head_sha(workspace_root)?;

    // Create worktree pool
    println!("Creating worktree pool ({} slots)...", agent_count);
    let slots = create_worktree_pool(workspace_root, agent_count)?;
    println!();

    // Shared state
    let queue: Arc<Mutex<VecDeque<PathBuf>>> =
        Arc::new(Mutex::new(VecDeque::from(pending)));
    let completed = Arc::new(Mutex::new(0usize));
    let failed = Arc::new(Mutex::new(0usize));

    // Install SIGINT handler
    install_sigint_handler();

    // Create multi-spinner on main thread (MultiSpinnerHandle is !Send)
    let ms = MultiSpinner::new().start();
    let lines: Vec<_> = (0..agent_count)
        .map(|i| ms.add(format!("[{}] waiting...", i)))
        .collect();

    // Spawn worker threads — each gets one SpinnerLineHandle (which is Send)
    let mut handles = Vec::with_capacity(agent_count);

    for (i, (slot, line)) in slots.into_iter().zip(lines).enumerate() {
        let queue = Arc::clone(&queue);
        let completed = Arc::clone(&completed);
        let failed = Arc::clone(&failed);
        let base_sha = base_sha.clone();

        let handle = std::thread::spawn(move || {
            let mut done_count = 0usize;
            let mut fail_count = 0usize;
            let mut prev_summary = String::new();

            loop {
                if SHUTDOWN_REQUESTED.load(Ordering::Relaxed) {
                    break;
                }

                let prompt_path = {
                    let mut q = queue.lock().unwrap();
                    q.pop_front()
                };

                let prompt_path = match prompt_path {
                    Some(p) => p,
                    None => break,
                };

                let prompt_name = prompt_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?")
                    .to_string();

                if prev_summary.is_empty() {
                    line.update(format!("[{}] {} | starting...", i, prompt_name));
                } else {
                    line.update(format!(
                        "[{}] {} | starting... ({})",
                        i, prompt_name, prev_summary
                    ));
                }

                let result = run_agent_in_slot(
                    &slot, i, &prompt_path, timeout, &base_sha,
                    &|msg| {
                        line.update(format!("[{}] {} | {}", i, prompt_name, msg));
                    },
                );

                if result.success {
                    let mut c = completed.lock().unwrap();
                    *c += 1;
                    done_count += 1;
                    prev_summary = format!(
                        "prev: {} OK {}p",
                        prompt_name, result.patches
                    );
                } else {
                    let mut f = failed.lock().unwrap();
                    *f += 1;
                    fail_count += 1;
                    prev_summary = format!("prev: {} FAIL", prompt_name);
                }
            }

            line.success_with(format!(
                "[{}] finished — {} done, {} failed",
                i, done_count, fail_count
            ));
        });

        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        let _ = handle.join();
    }

    ms.stop();

    // Print summary
    let final_completed = *completed.lock().unwrap();
    let final_failed = *failed.lock().unwrap();

    println!("\n{}", "=".repeat(40));
    println!("Execution complete");
    println!("  Completed: {}", final_completed);
    println!("  Failed:    {}", final_failed);
    println!("  Previously done: {}", done_count);
    println!(
        "  Total progress:  {}/{}",
        done_count + final_completed,
        total
    );

    // Clean up worktrees and branches
    println!("\nCleaning up worktrees...");
    if let Err(e) = cleanup_worktrees(workspace_root) {
        eprintln!("  Warning: cleanup failed: {}", e);
    }

    Ok(())
}

// ── Signal handling ────────────────────────────────────────────────────

/// Global shutdown flag set by SIGINT handler.
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Install a SIGINT handler that sets the SHUTDOWN_REQUESTED flag.
fn install_sigint_handler() {
    SHUTDOWN_REQUESTED.store(false, Ordering::Relaxed);

    #[cfg(unix)]
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = sigint_handler as usize;
        sa.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
    }
}

#[cfg(unix)]
extern "C" fn sigint_handler(_sig: libc::c_int) {
    SHUTDOWN_REQUESTED.store(true, Ordering::Relaxed);
    // Write directly to stderr (async-signal-safe)
    let msg = b"\nShutdown requested, finishing current agents...\n";
    unsafe {
        libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
    }
}
