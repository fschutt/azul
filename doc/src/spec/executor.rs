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

// ── review-md command ──────────────────────────────────────────────────

/// Generate a Gemini review prompt covering all changes from `base_commit..HEAD`.
///
/// Categorizes each commit as CODE (has non-comment code changes) or ANNOT
/// (annotation-only), includes full diffs for CODE commits, flags misleading
/// commits, and appends the full solver3/text3 source for refactoring context.
pub fn cmd_review_md(target: &str, workspace_root: &Path, no_src: bool, no_spec: bool) -> Result<(), String> {
    use std::io::Write as _;

    // Resolve relative paths: try as-is first, then relative to workspace_root
    let target_path = {
        let p = PathBuf::from(target);
        if p.is_dir() {
            p
        } else {
            let resolved = workspace_root.join(target);
            if resolved.is_dir() { resolved } else { p }
        }
    };
    if target_path.is_dir() {
        cmd_review_md_from_dir(&target_path, workspace_root, no_src, no_spec)
    } else {
        cmd_review_md_from_commits(target, workspace_root, no_src)
    }
}

fn categorize_diff_text(diff_text: &str) -> (usize, usize, usize, usize) {
    let mut real_adds = 0usize;
    let mut real_dels = 0usize;
    let mut total_adds = 0usize;
    let mut total_dels = 0usize;
    let mut in_diff = false;

    for diff_line in diff_text.lines() {
        if diff_line.starts_with("diff --git") {
            in_diff = true;
            continue;
        }
        if !in_diff {
            continue;
        }
        if diff_line.starts_with("+++") || diff_line.starts_with("---") {
            continue;
        }
        if diff_line.starts_with('+') {
            total_adds += 1;
            let trimmed = diff_line[1..].trim();
            if !trimmed.is_empty() && !trimmed.starts_with("//") {
                real_adds += 1;
            }
        } else if diff_line.starts_with('-') && diff_line != "--" && diff_line != "-- " {
            total_dels += 1;
            let trimmed = diff_line[1..].trim();
            if !trimmed.is_empty() && !trimmed.starts_with("//") {
                real_dels += 1;
            }
        }
    }

    (total_adds, total_dels, real_adds, real_dels)
}

fn write_review_header(f: &mut fs::File, count: usize) {
    use std::io::Write as _;

    writeln!(f, "# Agent Run Code Review — Refactoring & Lazy Commit Analysis\n").unwrap();
    writeln!(f, "You are reviewing {} patches made by AI agents to a CSS layout engine (Rust).", count).unwrap();
    writeln!(f, "The agents were tasked with reading W3C CSS spec paragraphs and either:").unwrap();
    writeln!(f, "1. Annotating the source code with `// +spec:feature-HASH` markers where behavior is already implemented").unwrap();
    writeln!(f, "2. Implementing missing behavior described by the spec paragraph\n").unwrap();

    writeln!(f, "## Your Tasks\n").unwrap();
    writeln!(f, "### Task A: Identify patches that need refactoring").unwrap();
    writeln!(f, "Look for:").unwrap();
    writeln!(f, "- **Code duplication**: same logic repeated in multiple places (should be extracted to a helper)").unwrap();
    writeln!(f, "- **Comment concatenation bugs**: two comments merged on one line").unwrap();
    writeln!(f, "- **Spaghetti if/else**: unnecessary branching that makes code harder to follow").unwrap();
    writeln!(f, "- **Wrong abstractions**: code added in the wrong place architecturally").unwrap();
    writeln!(f, "- **Conflicting patches**: multiple patches that modify the same code region independently\n").unwrap();
    writeln!(f, "For each issue, specify:").unwrap();
    writeln!(f, "- Which patch(es) introduced it").unwrap();
    writeln!(f, "- What the problem is").unwrap();
    writeln!(f, "- What refactoring is needed (be specific)\n").unwrap();

    writeln!(f, "### Task B: Identify lazy/misleading patches").unwrap();
    writeln!(f, "Some patches claim to \"implement\" or \"fix\" behavior but only add annotation comments.").unwrap();
    writeln!(f, "For each, specify: patch name, what it claims, what it actually does,").unwrap();
    writeln!(f, "and whether the claimed implementation is genuinely needed.\n").unwrap();

    writeln!(f, "### Task C: Identify genuinely good implementation patches").unwrap();
    writeln!(f, "List patches that made real, correct code changes. Note any conflicts.\n").unwrap();
}

fn write_review_response_format(f: &mut fs::File) {
    use std::io::Write as _;

    writeln!(f, "## Response Format\n").unwrap();
    writeln!(f, "### A. Refactoring Needed").unwrap();
    writeln!(f, "| Patch(es) | Issue | Refactoring Required |").unwrap();
    writeln!(f, "|-----------|-------|---------------------|").unwrap();
    writeln!(f, "| ... | ... | ... |\n").unwrap();
    writeln!(f, "### B. Lazy/Misleading Patches to Redo").unwrap();
    writeln!(f, "| Patch | Claims | Actually Does | Implementation Needed? |").unwrap();
    writeln!(f, "|-------|--------|---------------|----------------------|").unwrap();
    writeln!(f, "| ... | ... | ... | Yes/No (explain) |\n").unwrap();
    writeln!(f, "### C. Good Implementation Patches").unwrap();
    writeln!(f, "| Patch | What it does | Quality | Notes |").unwrap();
    writeln!(f, "|-------|-------------|---------|-------|").unwrap();
    writeln!(f, "| ... | ... | Good/OK/Needs review | ... |\n").unwrap();
}

fn write_source_appendix(f: &mut fs::File, workspace_root: &Path) {
    use std::io::Write as _;

    let source_files = [
        "layout/src/solver3/fc.rs",
        "layout/src/solver3/layout_tree.rs",
        "layout/src/solver3/positioning.rs",
        "layout/src/solver3/sizing.rs",
        "layout/src/solver3/geometry.rs",
        "layout/src/solver3/cache.rs",
        "layout/src/solver3/getters.rs",
        "layout/src/solver3/taffy_bridge.rs",
        "layout/src/solver3/mod.rs",
        "layout/src/text3/cache.rs",
        "layout/src/text3/knuth_plass.rs",
    ];

    writeln!(f, "---\n").unwrap();
    writeln!(f, "## APPENDIX: Current Source Files\n").unwrap();
    writeln!(f, "Full source for refactoring context.\n").unwrap();

    for src in &source_files {
        let full_path = workspace_root.join(src);
        match fs::read_to_string(&full_path) {
            Ok(content) => {
                writeln!(f, "### `{}`\n", src).unwrap();
                writeln!(f, "```rust").unwrap();
                write!(f, "{}", content).unwrap();
                if !content.ends_with('\n') {
                    writeln!(f).unwrap();
                }
                writeln!(f, "```\n").unwrap();
            }
            Err(e) => {
                writeln!(f, "### `{}` — MISSING: {}\n", src, e).unwrap();
            }
        }
    }
}

/// Extract the "## Spec Paragraph to Verify" section from a prompt .md file.
fn extract_spec_paragraph(prompt_content: &str) -> Option<String> {
    let mut result = String::new();
    let mut in_section = false;

    for line in prompt_content.lines() {
        if line.starts_with("## Spec Paragraph") {
            in_section = true;
            continue; // skip the heading itself
        }
        if in_section && line.starts_with("## ") {
            break; // next section
        }
        if in_section {
            result.push_str(line);
            result.push('\n');
        }
    }

    let trimmed = result.trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

fn cmd_review_md_from_dir(dir: &Path, workspace_root: &Path, no_src: bool, no_spec: bool) -> Result<(), String> {
    use std::io::Write as _;

    let mut patches: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read dir {}: {}", dir.display(), e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "patch").unwrap_or(false))
        .collect();
    patches.sort();

    if patches.is_empty() {
        return Err(format!("No .patch files found in {}", dir.display()));
    }

    // Categorize patches
    let mut code_patches = Vec::new();
    let mut annot_patches = Vec::new();
    let mut misleading = Vec::new();

    for patch_path in &patches {
        let content = fs::read_to_string(patch_path)
            .map_err(|e| format!("Failed to read {}: {}", patch_path.display(), e))?;
        let name = patch_path.file_name().unwrap().to_string_lossy().to_string();

        let (total_adds, total_dels, real_adds, real_dels) = categorize_diff_text(&content);
        let is_code = real_adds > 0 || real_dels > 0;

        let entry = format!(
            "{} {}  [+{}/-{}, code:+{}/-{}]",
            if is_code { "CODE " } else { "ANNOT" },
            name, total_adds, total_dels, real_adds, real_dels,
        );

        if is_code {
            code_patches.push((name, content));
        } else {
            let lower = name.to_lowercase();
            // Extract subject from patch
            let subject = content.lines()
                .find(|l| l.starts_with("Subject:"))
                .unwrap_or("")
                .to_lowercase();
            if subject.contains("implement") || subject.contains("fix") {
                misleading.push(entry.clone());
            }
            annot_patches.push(entry);
        }
    }

    // Build the output file
    let out_path = PathBuf::from("/tmp/agent-run-review-prompt.md");
    let mut f = fs::File::create(&out_path)
        .map_err(|e| format!("Failed to create {}: {}", out_path.display(), e))?;

    write_review_header(&mut f, patches.len());

    // Stats
    writeln!(f, "## Patch Summary\n").unwrap();
    writeln!(f, "- Source directory: `{}`", dir.display()).unwrap();
    writeln!(f, "- Total patches: {}", patches.len()).unwrap();
    writeln!(f, "- CODE patches (contain real code changes): {}", code_patches.len()).unwrap();
    writeln!(f, "- ANNOT patches (annotation-only): {}\n", annot_patches.len()).unwrap();

    // All patches categorized
    writeln!(f, "## All Patches (categorized)\n").unwrap();
    writeln!(f, "```").unwrap();
    for (name, _) in &code_patches {
        writeln!(f, "CODE  {}", name).unwrap();
    }
    for entry in &annot_patches {
        writeln!(f, "{}", entry).unwrap();
    }
    writeln!(f, "```\n").unwrap();

    // CODE patch diffs
    writeln!(f, "## CODE Patches — Full Diffs\n").unwrap();
    writeln!(f, "Focus your review on these {} patches:\n", code_patches.len()).unwrap();

    // Find prompts directory containing the original .md prompt files.
    // Patches are named <feature>_<num>.md.done.001.patch,
    // prompts are <feature>_<num>.md. Search: same dir, parent/prompts,
    // grandparent/prompts, or anywhere up with a "prompts" subdir.
    let sample_prompt_name = patches.first()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(|n| n.split(".done.").next().unwrap_or(""))
        .unwrap_or("");
    let prompts_dir = {
        let mut found = None;
        // Check same dir
        if !sample_prompt_name.is_empty() && dir.join(sample_prompt_name).exists() {
            found = Some(dir.to_path_buf());
        }
        // Walk up parents looking for a "prompts" dir containing the file
        if found.is_none() {
            let mut cursor = Some(dir.as_ref() as &Path);
            while let Some(d) = cursor {
                let candidate = d.join("prompts");
                if candidate.is_dir() && candidate.join(sample_prompt_name).exists() {
                    found = Some(candidate);
                    break;
                }
                cursor = d.parent();
            }
        }
        found
    };

    for (name, content) in &code_patches {
        writeln!(f, "---\n").unwrap();
        writeln!(f, "### {}\n", name).unwrap();

        // Try to include the spec paragraph from the prompt file
        if !no_spec {
            let prompt_name = name.split(".done.").next().unwrap_or(name);
            let spec_para = prompts_dir.as_ref().and_then(|pd| {
                let prompt_path = pd.join(prompt_name);
                fs::read_to_string(&prompt_path).ok()
            }).and_then(|content| extract_spec_paragraph(&content));

            if let Some(para) = spec_para {
                writeln!(f, "**W3C Spec Paragraph:**\n").unwrap();
                writeln!(f, "{}\n", para).unwrap();
            }
        }

        writeln!(f, "```diff").unwrap();
        let mut in_diff = false;
        for dl in content.lines() {
            if dl.starts_with("diff --git") {
                in_diff = true;
            }
            if in_diff {
                writeln!(f, "{}", dl).unwrap();
            }
        }
        writeln!(f, "```\n").unwrap();
    }

    // Misleading patches
    if !misleading.is_empty() {
        writeln!(f, "## Misleading Patches (claim implement/fix but annotation-only)\n").unwrap();
        writeln!(f, "```").unwrap();
        for entry in &misleading {
            writeln!(f, "{}", entry).unwrap();
        }
        writeln!(f, "```\n").unwrap();
    }

    write_review_response_format(&mut f);

    if !no_src {
        write_source_appendix(&mut f, workspace_root);
    }

    drop(f);

    let file_size = fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
    let est_tokens = file_size / 4;

    println!("Review prompt generated: {}", out_path.display());
    println!("  {} patches analyzed ({} CODE, {} ANNOT)",
        patches.len(), code_patches.len(), annot_patches.len());
    if !misleading.is_empty() {
        println!("  {} misleading patches flagged", misleading.len());
    }
    println!("  File size: {:.1} MB (~{:.0}K tokens)",
        file_size as f64 / 1_048_576.0, est_tokens as f64 / 1000.0);

    Ok(())
}

fn cmd_review_md_from_commits(base_commit: &str, workspace_root: &Path, no_src: bool) -> Result<(), String> {
    use std::io::Write as _;

    // Verify the base commit exists
    let output = Command::new("git")
        .args(["rev-parse", "--verify", base_commit])
        .current_dir(workspace_root)
        .output()
        .map_err(|e| format!("git rev-parse failed: {}", e))?;
    if !output.status.success() {
        return Err(format!("Invalid commit: {}", base_commit));
    }
    let base_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Get all commits in range
    let output = Command::new("git")
        .args(["log", "--oneline", "--reverse", &format!("{}..HEAD", base_sha)])
        .current_dir(workspace_root)
        .output()
        .map_err(|e| format!("git log failed: {}", e))?;
    let all_commits: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect();

    if all_commits.is_empty() {
        return Err("No commits in range".to_string());
    }

    // Get overall diff stats
    let output = Command::new("git")
        .args(["diff", "--stat", &format!("{}..HEAD", base_sha)])
        .current_dir(workspace_root)
        .output()
        .map_err(|e| format!("git diff --stat failed: {}", e))?;
    let diff_stat = String::from_utf8_lossy(&output.stdout).to_string();

    // Categorize each commit as CODE or ANNOT
    let mut code_commits = Vec::new();
    let mut annot_commits = Vec::new();
    let mut misleading = Vec::new();

    for line in &all_commits {
        let hash = line.split_whitespace().next().unwrap_or("");
        if hash.is_empty() {
            continue;
        }

        let output = Command::new("git")
            .args(["show", hash, "--", "*.rs"])
            .current_dir(workspace_root)
            .output()
            .map_err(|e| format!("git show failed: {}", e))?;
        let diff_text = String::from_utf8_lossy(&output.stdout).to_string();

        let (total_adds, total_dels, real_adds, real_dels) = categorize_diff_text(&diff_text);
        let is_code = real_adds > 0 || real_dels > 0;

        let entry = format!(
            "{} {}  [+{}/-{}, code:+{}/-{}]",
            if is_code { "CODE " } else { "ANNOT" },
            line, total_adds, total_dels, real_adds, real_dels,
        );

        if is_code {
            code_commits.push((hash.to_string(), line.clone(), diff_text));
        } else {
            let lower = line.to_lowercase();
            if lower.contains("implement") || lower.contains("fix") {
                misleading.push(entry.clone());
            }
            annot_commits.push(entry);
        }
    }

    // Build the output file
    let out_path = PathBuf::from("/tmp/agent-run-review-prompt.md");
    let mut f = fs::File::create(&out_path)
        .map_err(|e| format!("Failed to create {}: {}", out_path.display(), e))?;

    write_review_header(&mut f, all_commits.len());

    // Stats
    writeln!(f, "## Commit Summary\n").unwrap();
    writeln!(f, "- Base: `{}`", base_sha).unwrap();
    writeln!(f, "- Total commits: {}", all_commits.len()).unwrap();
    writeln!(f, "- CODE commits (contain real code changes): {}", code_commits.len()).unwrap();
    writeln!(f, "- ANNOT commits (annotation-only): {}", annot_commits.len()).unwrap();
    writeln!(f, "\n### Diff stats\n```\n{}```\n", diff_stat).unwrap();

    // All commits categorized
    writeln!(f, "## All Commits (categorized)\n").unwrap();
    writeln!(f, "```").unwrap();
    for (_hash, line, _) in &code_commits {
        writeln!(f, "CODE  {}", line).unwrap();
    }
    for entry in &annot_commits {
        writeln!(f, "{}", entry).unwrap();
    }
    writeln!(f, "```\n").unwrap();

    // CODE commit diffs
    writeln!(f, "## CODE Commits — Full Diffs\n").unwrap();
    writeln!(f, "Focus your review on these {} commits:\n", code_commits.len()).unwrap();

    for (_hash, line, diff_text) in &code_commits {
        writeln!(f, "---\n").unwrap();
        writeln!(f, "### {}\n", line).unwrap();
        writeln!(f, "```diff").unwrap();
        let mut in_diff = false;
        for dl in diff_text.lines() {
            if dl.starts_with("diff --git") {
                in_diff = true;
            }
            if in_diff {
                writeln!(f, "{}", dl).unwrap();
            }
        }
        writeln!(f, "```\n").unwrap();
    }

    // Misleading commits
    if !misleading.is_empty() {
        writeln!(f, "## Misleading Commits (claim implement/fix but annotation-only)\n").unwrap();
        writeln!(f, "```").unwrap();
        for entry in &misleading {
            writeln!(f, "{}", entry).unwrap();
        }
        writeln!(f, "```\n").unwrap();
    }

    write_review_response_format(&mut f);

    if !no_src {
        write_source_appendix(&mut f, workspace_root);
    }

    drop(f);

    let file_size = fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
    let est_tokens = file_size / 4;

    println!("Review prompt generated: {}", out_path.display());
    println!("  {} commits analyzed ({} CODE, {} ANNOT)",
        all_commits.len(), code_commits.len(), annot_commits.len());
    if !misleading.is_empty() {
        println!("  {} misleading commits flagged", misleading.len());
    }
    println!("  File size: {:.1} MB (~{:.0}K tokens)",
        file_size as f64 / 1_048_576.0, est_tokens as f64 / 1000.0);

    Ok(())
}

// ── review-arch command ───────────────────────────────────────────────

/// Generate a Gemini architecture-review prompt.
///
/// Solves the "tunnel vision" problem: each `claude-exec` agent only sees one spec
/// paragraph in isolation. This prompt gives Gemini ALL patches together with the
/// original spec paragraphs, so it can identify cross-cutting concerns, contradictions,
/// and architectural changes needed to support multiple patches cleanly.
pub fn cmd_review_arch(
    patch_dir: &str,
    review_path: &str,
    workspace_root: &Path,
    no_src: bool,
) -> Result<(), String> {
    use std::io::Write as _;

    // Resolve patch dir
    let patch_dir = {
        let p = PathBuf::from(patch_dir);
        if p.is_dir() { p } else {
            let resolved = workspace_root.join(patch_dir);
            if resolved.is_dir() { resolved } else {
                return Err(format!("Patch directory not found: {}", patch_dir));
            }
        }
    };

    // Read review file
    let review_path = {
        let p = PathBuf::from(review_path);
        if p.is_file() { p } else { workspace_root.join(review_path) }
    };
    let review_content = fs::read_to_string(&review_path)
        .map_err(|e| format!("Failed to read review file {}: {}", review_path.display(), e))?;

    // Scan patches
    let mut patches: Vec<PathBuf> = fs::read_dir(&patch_dir)
        .map_err(|e| format!("Failed to read dir {}: {}", patch_dir.display(), e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "patch").unwrap_or(false))
        .collect();
    patches.sort();

    if patches.is_empty() {
        return Err(format!("No .patch files found in {}", patch_dir.display()));
    }

    // Load original spec paragraphs from prompt files
    // Prompt files live in the same dir or a sibling "prompts" dir
    // Format: {feature}_{NNN}.md → contains "## Spec Paragraph to Verify" section
    let prompts_dir = workspace_root.join("doc/target/skill_tree/prompts");
    let mut para_map: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new(); // feature_NNN → paragraph text

    if prompts_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&prompts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "md").unwrap_or(false) {
                    let stem = path.file_stem().and_then(|s| s.to_str())
                        .unwrap_or("").to_string();
                    if let Ok(content) = fs::read_to_string(&path) {
                        // Extract the spec paragraph: text between "> " lines after
                        // "## Spec Paragraph to Verify"
                        if let Some(start) = content.find("## Spec Paragraph to Verify") {
                            let after = &content[start..];
                            let para_text: String = after.lines()
                                .skip(1) // skip the heading
                                .skip_while(|l| !l.starts_with("> "))
                                .take_while(|l| l.starts_with("> ") || l.starts_with(">"))
                                .map(|l| l.trim_start_matches("> ").trim_start_matches('>'))
                                .collect::<Vec<&str>>()
                                .join(" ");
                            if !para_text.is_empty() {
                                // Truncate very long paragraphs
                                let truncated = if para_text.len() > 600 {
                                    let mut end = 600;
                                    while !para_text.is_char_boundary(end) { end -= 1; }
                                    format!("{}...", &para_text[..end])
                                } else {
                                    para_text
                                };
                                para_map.insert(stem, truncated);
                            }
                        }
                    }
                }
            }
        }
    }
    let para_count = para_map.len();

    let out_path = PathBuf::from("/tmp/agent-review-arch-prompt.md");
    let mut f = fs::File::create(&out_path)
        .map_err(|e| format!("Failed to create {}: {}", out_path.display(), e))?;

    writeln!(f, "# Architecture Review — Cross-Patch Analysis\n").unwrap();
    writeln!(f, "## Background\n").unwrap();
    writeln!(f, "You are reviewing {} patches generated by parallel AI agents against a CSS layout engine (Rust).", patches.len()).unwrap();
    writeln!(f, "Each agent worked on ONE spec paragraph in isolation ('tunnel vision'). This means:").unwrap();
    writeln!(f, "- Agents didn't see each other's patches or spec paragraphs").unwrap();
    writeln!(f, "- Multiple agents may have solved the same problem differently").unwrap();
    writeln!(f, "- Patches may contradict each other or make incompatible assumptions").unwrap();
    writeln!(f, "- No agent considered how their changes interact with changes from other paragraphs\n").unwrap();
    writeln!(f, "Your job is to review the patches WITH the original spec paragraphs and identify").unwrap();
    writeln!(f, "what needs to change in the architecture of the patches before they can be applied.\n").unwrap();

    writeln!(f, "## Your Tasks\n").unwrap();
    writeln!(f, "### 1. Cross-patch contradictions").unwrap();
    writeln!(f, "Find patches that make incompatible changes to the same code. For each conflict:").unwrap();
    writeln!(f, "- Which spec paragraphs are involved? (check the original text below)").unwrap();
    writeln!(f, "- Which patch is more correct per the spec?").unwrap();
    writeln!(f, "- How should the conflict be resolved?\n").unwrap();

    writeln!(f, "### 2. Tunnel vision gaps").unwrap();
    writeln!(f, "Identify cases where an agent implemented a narrow fix for their paragraph but").unwrap();
    writeln!(f, "missed the broader context visible only when reading multiple paragraphs together:").unwrap();
    writeln!(f, "- A patch adds a special case that another paragraph's rule already covers generally").unwrap();
    writeln!(f, "- A patch hardcodes assumptions that break under conditions described in other paragraphs").unwrap();
    writeln!(f, "- Related spec requirements split across paragraphs that need a unified implementation\n").unwrap();

    writeln!(f, "### 3. Architectural changes needed").unwrap();
    writeln!(f, "What structural changes to the PATCHES (not the codebase) are needed?").unwrap();
    writeln!(f, "- Patches that should be merged into one coherent implementation").unwrap();
    writeln!(f, "- Patches that need to be rewritten to use a shared abstraction").unwrap();
    writeln!(f, "- Ordering constraints: which patches must be applied before others\n").unwrap();

    writeln!(f, "### 4. ABI and regression concerns").unwrap();
    writeln!(f, "- Patches that modify `#[repr(C)]` structs or public FFI types").unwrap();
    writeln!(f, "- Patches that replace better code with worse code (regressions)").unwrap();
    writeln!(f, "- Patches with hallucinated APIs or fundamentally wrong logic\n").unwrap();

    writeln!(f, "## Response Format\n").unwrap();
    writeln!(f, "Produce a structured markdown document. Be specific: reference patch names,").unwrap();
    writeln!(f, "spec paragraph IDs, file names, and function names.\n").unwrap();

    // Include original spec paragraphs
    if !para_map.is_empty() {
        writeln!(f, "---\n").unwrap();
        writeln!(f, "## APPENDIX A: Original Spec Paragraphs ({} total)\n", para_count).unwrap();
        writeln!(f, "These are the W3C spec paragraphs that the agents were tasked with implementing.\n").unwrap();

        // Group by feature
        let mut by_feature: std::collections::BTreeMap<&str, Vec<(&str, &str)>> =
            std::collections::BTreeMap::new();
        for (key, text) in &para_map {
            let feature = key.rfind('_').map(|i| &key[..i]).unwrap_or(key);
            by_feature.entry(feature).or_default().push((key, text));
        }

        for (feature, paras) in &by_feature {
            writeln!(f, "### {}\n", feature).unwrap();
            for (key, text) in paras {
                writeln!(f, "**{}**: {}\n", key, text).unwrap();
            }
        }
    }

    // Include review
    writeln!(f, "---\n").unwrap();
    writeln!(f, "## APPENDIX B: Patch Review (from review-md)\n").unwrap();
    writeln!(f, "{}\n", review_content).unwrap();

    // Include source if requested
    if !no_src {
        write_source_appendix(&mut f, workspace_root);
    }

    drop(f);

    let file_size = fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
    let est_tokens = file_size / 4;

    println!("Architecture review prompt generated: {}", out_path.display());
    println!("  {} patches referenced, {} spec paragraphs included", patches.len(), para_count);
    println!("  File size: {:.1} MB (~{:.0}K tokens)",
        file_size as f64 / 1_048_576.0, est_tokens as f64 / 1000.0);

    Ok(())
}

// ── refactor-md command ──────────────────────────────────────────────

/// Generate a Gemini refactoring-plan prompt.
///
/// Takes the patch directory, the review-md output, and optionally the
/// review-arch output. Asks Gemini to produce a refactoring plan.
pub fn cmd_refactor_md(
    patch_dir: &str,
    review_path: &str,
    arch_path: Option<&str>,
    workspace_root: &Path,
    no_src: bool,
) -> Result<(), String> {
    use std::io::Write as _;

    // Resolve patch dir
    let patch_dir = {
        let p = PathBuf::from(patch_dir);
        if p.is_dir() { p } else {
            let resolved = workspace_root.join(patch_dir);
            if resolved.is_dir() { resolved } else {
                return Err(format!("Patch directory not found: {}", patch_dir));
            }
        }
    };

    // Read review file
    let review_path = {
        let p = PathBuf::from(review_path);
        if p.is_file() { p } else { workspace_root.join(review_path) }
    };
    let review_content = fs::read_to_string(&review_path)
        .map_err(|e| format!("Failed to read review file {}: {}", review_path.display(), e))?;

    // Read optional arch file
    let arch_content = if let Some(ap) = arch_path {
        let p = PathBuf::from(ap);
        let resolved = if p.is_file() { p } else { workspace_root.join(ap) };
        Some(fs::read_to_string(&resolved)
            .map_err(|e| format!("Failed to read arch file {}: {}", resolved.display(), e))?)
    } else {
        None
    };

    let out_path = PathBuf::from("/tmp/agent-refactor-prompt.md");
    let mut f = fs::File::create(&out_path)
        .map_err(|e| format!("Failed to create {}: {}", out_path.display(), e))?;

    writeln!(f, "# Refactoring Groundwork Plan\n").unwrap();
    writeln!(f, "You are planning refactoring work needed before applying patches to a CSS layout engine (Rust).\n").unwrap();
    writeln!(f, "A review of the patches identified conflict clusters and architectural issues.").unwrap();
    writeln!(f, "Your job is to produce a **refactoring plan** (GROUNDWORK.md): a list of abstractions,").unwrap();
    writeln!(f, "helpers, and structural changes that should be made BEFORE applying patches.\n").unwrap();

    writeln!(f, "## Your Tasks\n").unwrap();
    writeln!(f, "For each refactoring item, specify:").unwrap();
    writeln!(f, "1. **What**: The abstraction/helper/refactor to create").unwrap();
    writeln!(f, "2. **Why**: Why it's needed (which conflict clusters or patches benefit)").unwrap();
    writeln!(f, "3. **Where**: Which files and functions to modify").unwrap();
    writeln!(f, "4. **Needed for patches**: List specific patch names that depend on this groundwork\n").unwrap();

    writeln!(f, "## Guidelines\n").unwrap();
    writeln!(f, "- Focus on abstractions that prevent multiple patches from scattering ad-hoc logic").unwrap();
    writeln!(f, "- Prioritize helpers that reduce merge conflicts between patches").unwrap();
    writeln!(f, "- Keep it concrete: name specific functions, types, and files").unwrap();
    writeln!(f, "- Number the items (## 1, ## 2, ...) for easy reference\n").unwrap();

    // Include review
    writeln!(f, "---\n").unwrap();
    writeln!(f, "## APPENDIX A: Patch Review\n").unwrap();
    writeln!(f, "{}\n", review_content).unwrap();

    // Include arch review if available
    if let Some(arch) = &arch_content {
        writeln!(f, "---\n").unwrap();
        writeln!(f, "## APPENDIX B: Architecture Review\n").unwrap();
        writeln!(f, "{}\n", arch).unwrap();
    }

    // Include source if requested
    if !no_src {
        write_source_appendix(&mut f, workspace_root);
    }

    drop(f);

    let file_size = fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
    let est_tokens = file_size / 4;

    println!("Refactoring plan prompt generated: {}", out_path.display());
    println!("  File size: {:.1} MB (~{:.0}K tokens)",
        file_size as f64 / 1_048_576.0, est_tokens as f64 / 1000.0);

    Ok(())
}

// ── groups-json command ──────────────────────────────────────────────

/// Generate a Gemini merge-group prompt.
///
/// Takes the patch directory plus outputs from previous pipeline steps
/// (review-md, review-arch, refactor-md) and asks Gemini to produce
/// merge groups with application ordering (JSON).
pub fn cmd_groups_json(
    patch_dir: &str,
    review_path: &str,
    arch_path: Option<&str>,
    refactor_path: Option<&str>,
    workspace_root: &Path,
    no_src: bool,
) -> Result<(), String> {
    use std::io::Write as _;

    // Resolve patch dir
    let patch_dir = {
        let p = PathBuf::from(patch_dir);
        if p.is_dir() { p } else {
            let resolved = workspace_root.join(patch_dir);
            if resolved.is_dir() { resolved } else {
                return Err(format!("Patch directory not found: {}", patch_dir));
            }
        }
    };

    // Read review file (resolve relative to workspace root if needed)
    let review_path = {
        let p = PathBuf::from(review_path);
        if p.is_file() { p } else { workspace_root.join(review_path) }
    };
    let review_content = fs::read_to_string(&review_path)
        .map_err(|e| format!("Failed to read review file {}: {}", review_path.display(), e))?;

    // Read optional context files from previous pipeline steps
    let resolve = |path: &str| -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_file() { p } else { workspace_root.join(path) }
    };
    let arch_content = if let Some(ap) = arch_path {
        Some(fs::read_to_string(&resolve(ap))
            .map_err(|e| format!("Failed to read --review-arch {}: {}", ap, e))?)
    } else {
        None
    };
    let refactor_content = if let Some(rp) = refactor_path {
        Some(fs::read_to_string(&resolve(rp))
            .map_err(|e| format!("Failed to read --refactor-md {}: {}", rp, e))?)
    } else {
        None
    };

    // Scan all patches, extract metadata
    let mut patches: Vec<PathBuf> = fs::read_dir(&patch_dir)
        .map_err(|e| format!("Failed to read dir {}: {}", patch_dir.display(), e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "patch").unwrap_or(false))
        .collect();
    patches.sort();

    if patches.is_empty() {
        return Err(format!("No .patch files found in {}", patch_dir.display()));
    }

    // Build per-patch summary: name, files touched, CODE/ANNOT, size
    struct PatchInfo {
        name: String,
        feature: String,
        para_num: String,
        files_touched: Vec<String>,
        is_code: bool,
        added: usize,
        removed: usize,
    }

    let mut patch_infos = Vec::new();
    for patch_path in &patches {
        let content = fs::read_to_string(patch_path)
            .map_err(|e| format!("Failed to read {}: {}", patch_path.display(), e))?;
        let name = patch_path.file_name().unwrap().to_string_lossy().to_string();

        // Extract feature and paragraph number from name
        // e.g. "block-formatting-context_023.md.done.001.patch"
        let stem = name.split(".md").next().unwrap_or(&name);
        let (feature, para_num) = if let Some(idx) = stem.rfind('_') {
            (stem[..idx].to_string(), stem[idx + 1..].to_string())
        } else {
            (stem.to_string(), String::new())
        };

        // Files touched
        let files_touched: Vec<String> = content
            .lines()
            .filter(|l| l.starts_with("diff --git"))
            .filter_map(|l| l.split(" b/").nth(1))
            .map(|s| s.to_string())
            .collect();

        let (total_adds, total_dels, real_adds, real_dels) = categorize_diff_text(&content);
        let is_code = real_adds > 0 || real_dels > 0;

        patch_infos.push(PatchInfo {
            name,
            feature,
            para_num,
            files_touched,
            is_code,
            added: total_adds,
            removed: total_dels,
        });
    }

    let code_count = patch_infos.iter().filter(|p| p.is_code).count();
    let annot_count = patch_infos.len() - code_count;

    // Build file-to-patches index (which patches touch which files)
    let mut file_to_patches: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for pi in &patch_infos {
        for f in &pi.files_touched {
            file_to_patches
                .entry(f.clone())
                .or_default()
                .push(pi.name.clone());
        }
    }

    // Identify conflict clusters: patches that touch the same file
    let conflict_files: Vec<(&String, &Vec<String>)> = file_to_patches
        .iter()
        .filter(|(_, patches)| patches.len() > 1)
        .collect();

    // Build output
    let out_path = PathBuf::from("/tmp/agent-arch-review-prompt.md");
    let mut f = fs::File::create(&out_path)
        .map_err(|e| format!("Failed to create {}: {}", out_path.display(), e))?;

    writeln!(f, "# Architecture Review — Patch Merge Planning\n").unwrap();
    writeln!(f, "You are planning how to apply {} patches to a CSS layout engine (Rust).", patches.len()).unwrap();
    writeln!(f, "A previous review identified code quality issues and conflicts.").unwrap();
    writeln!(f, "Your job is to produce a **merge plan**: group patches into ordered merge groups.\n").unwrap();

    writeln!(f, "## Your Tasks\n").unwrap();
    writeln!(f, "### Task 1: Produce merge groups").unwrap();
    writeln!(f, "Group patches that touch the same code regions or implement the same spec feature.").unwrap();
    writeln!(f, "For each group, specify:").unwrap();
    writeln!(f, "- **Group ID** (sequential number)").unwrap();
    writeln!(f, "- **Patches** in this group (by filename)").unwrap();
    writeln!(f, "- **Action**: `APPLY` (apply as-is), `MERGE` (agent must merge conflicting patches),").unwrap();
    writeln!(f, "  `PICK_ONE` (choose best, skip others), `SKIP` (don't apply)").unwrap();
    writeln!(f, "- **Preferred patch** (for PICK_ONE groups)").unwrap();
    writeln!(f, "- **`agent_context`**: A DETAILED instruction block for the applying agent. THIS IS CRITICAL.\n").unwrap();

    writeln!(f, "### Task 2: Write thorough `agent_context` for each group").unwrap();
    writeln!(f, "The `agent_context` field is passed VERBATIM to the agent that will apply this group.").unwrap();
    writeln!(f, "The agent will NOT see any other groups, the review, or the architecture plan —").unwrap();
    writeln!(f, "it ONLY sees: the patch diff(s), the agent_context, and the current source code.").unwrap();
    writeln!(f, "Therefore, `agent_context` MUST include everything the agent needs:\n").unwrap();
    writeln!(f, "- **What the patch does**: 1-2 sentence summary of the semantic intent").unwrap();
    writeln!(f, "- **Which W3C spec section** it implements (e.g., \"CSS 2.2 §10.3.7\")").unwrap();
    writeln!(f, "- **Known bugs in the patch** from the review (e.g., \"uses width > 0.0 as auto proxy — fix this\")").unwrap();
    writeln!(f, "- **Refactoring needed**: if the patch duplicates existing helpers, name the helper to reuse").unwrap();
    writeln!(f, "- **Functions/types to reuse**: specific existing functions the agent should call instead of adding new ones").unwrap();
    writeln!(f, "- **ABI concerns**: if the patch modifies `#[repr(C)]` structs, warn about FFI breakage").unwrap();
    writeln!(f, "- **For MERGE groups**: which parts of each patch to take, where they conflict, how to combine them").unwrap();
    writeln!(f, "- **For PICK_ONE groups**: why the preferred patch is better, what the others got wrong").unwrap();
    writeln!(f, "- **Compilation notes**: if missing imports or signature changes are needed, mention them\n").unwrap();

    writeln!(f, "### Task 3: Order the groups").unwrap();
    writeln!(f, "Order groups so that:").unwrap();
    writeln!(f, "- Independent patches come first (fewer conflicts, establish base)").unwrap();
    writeln!(f, "- ANNOT-only patches come last (they just add comments, easy to adapt)").unwrap();
    writeln!(f, "- Complex MERGE groups come after their dependencies are applied").unwrap();
    writeln!(f, "- Patches that add new types/enums come before patches that use them\n").unwrap();

    writeln!(f, "### Task 4: Flag patches to skip").unwrap();
    writeln!(f, "Based on the patch review below, flag patches that:").unwrap();
    writeln!(f, "- Are regressions (replace better code with worse code)").unwrap();
    writeln!(f, "- Are completely superseded by another patch in the same group").unwrap();
    writeln!(f, "- Have hallucinated APIs or fundamentally wrong logic\n").unwrap();

    writeln!(f, "### Task 5: Size limits").unwrap();
    writeln!(f, "- No group should contain more than 15 CODE patches.").unwrap();
    writeln!(f, "- If >15 independent CODE patches remain, split them by feature prefix").unwrap();
    writeln!(f, "  (e.g. all `width-calculation_*` in one group, all `line-breaking_*` in another).").unwrap();
    writeln!(f, "- ANNOT-only groups may be arbitrarily large (they use a fast-path).\n").unwrap();

    // Stats
    writeln!(f, "## Patch Inventory\n").unwrap();
    writeln!(f, "- Total patches: {}", patches.len()).unwrap();
    writeln!(f, "- CODE patches: {}", code_count).unwrap();
    writeln!(f, "- ANNOT patches: {}", annot_count).unwrap();

    // Features breakdown
    let mut feature_counts: std::collections::BTreeMap<&str, (usize, usize)> =
        std::collections::BTreeMap::new();
    for pi in &patch_infos {
        let entry = feature_counts.entry(&pi.feature).or_insert((0, 0));
        if pi.is_code {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }
    writeln!(f, "\n### By feature\n").unwrap();
    writeln!(f, "| Feature | CODE | ANNOT | Total |").unwrap();
    writeln!(f, "|---------|------|-------|-------|").unwrap();
    for (feature, (code, annot)) in &feature_counts {
        writeln!(f, "| {} | {} | {} | {} |", feature, code, annot, code + annot).unwrap();
    }

    // Conflict map
    writeln!(f, "\n## File Conflict Map\n").unwrap();
    writeln!(f, "These files are touched by multiple patches (potential merge conflicts):\n").unwrap();
    for (file, patches) in &conflict_files {
        writeln!(f, "### `{}`  ({} patches)\n", file, patches.len()).unwrap();
        for p in *patches {
            let info = patch_infos.iter().find(|pi| &pi.name == p).unwrap();
            writeln!(f, "- `{}` [{}] +{}/-{}",
                p,
                if info.is_code { "CODE" } else { "ANNOT" },
                info.added,
                info.removed,
            ).unwrap();
        }
        writeln!(f).unwrap();
    }

    // All patches list
    writeln!(f, "## All Patches\n").unwrap();
    writeln!(f, "```").unwrap();
    for pi in &patch_infos {
        writeln!(f, "{} {} {} [+{}/-{}] files: {}",
            if pi.is_code { "CODE " } else { "ANNOT" },
            pi.name,
            pi.feature,
            pi.added,
            pi.removed,
            pi.files_touched.join(", "),
        ).unwrap();
    }
    writeln!(f, "```\n").unwrap();

    // Include prior analysis from previous pipeline steps
    let mut appendix = b'A';
    writeln!(f, "---\n").unwrap();
    writeln!(f, "## APPENDIX {}: Patch Review (from review-md)\n", appendix as char).unwrap();
    writeln!(f, "This review was produced by a prior analysis pass. Use it to inform your grouping.\n").unwrap();
    writeln!(f, "{}\n", review_content).unwrap();
    appendix += 1;

    if let Some(arch) = &arch_content {
        writeln!(f, "---\n").unwrap();
        writeln!(f, "## APPENDIX {}: Architecture Review (from review-arch)\n", appendix as char).unwrap();
        writeln!(f, "Cross-patch analysis identifying tunnel-vision issues and architectural concerns.\n").unwrap();
        writeln!(f, "{}\n", arch).unwrap();
        appendix += 1;
    }

    if let Some(refactor) = &refactor_content {
        writeln!(f, "---\n").unwrap();
        writeln!(f, "## APPENDIX {}: Refactoring Plan (from refactor-md)\n", appendix as char).unwrap();
        writeln!(f, "Groundwork abstractions to implement before applying patches.\n").unwrap();
        writeln!(f, "{}\n", refactor).unwrap();
        appendix += 1;
    }
    let _ = appendix; // suppress unused warning

    // Include source if requested
    if !no_src {
        write_source_appendix(&mut f, workspace_root);
    }

    // Response format
    writeln!(f, "---\n").unwrap();
    writeln!(f, "## Required Response Format\n").unwrap();
    writeln!(f, "Respond with a JSON array of merge groups. The `agent_context` field is the most important —").unwrap();
    writeln!(f, "it will be passed verbatim to the applying agent as its sole instruction context.\n").unwrap();
    writeln!(f, "```json").unwrap();
    writeln!(f, "[").unwrap();
    writeln!(f, "  {{").unwrap();
    writeln!(f, "    \"group_id\": 1,").unwrap();
    writeln!(f, "    \"action\": \"APPLY\",").unwrap();
    writeln!(f, "    \"patches\": [\"floats_004.md.done.001.patch\"],").unwrap();
    writeln!(f, "    \"preferred\": null,").unwrap();
    writeln!(f, "    \"agent_context\": \"## Intent\\nFix float overlap: use float MARGIN BOX instead of content box for overlap checks (CSS 2.2 §9.5).\\n\\n## Spec\\nCSS 2.2 §9.5: 'The border box of a table, a block-level replaced element, or an element in the normal flow that establishes a new BFC MUST NOT overlap the margin box of any floats in the same BFC.'\\n\\n## Known Issues\\n- `is_block_level_replaced()` only checks Image, not video/canvas/input. This is OK for now, just apply as-is.\\n- The margin-box math in `available_line_box_space` is correct: verified origin - margin_start + size + margin_start + margin_end = correct margin box end.\\n\\n## Apply Instructions\\nApply directly. No conflicts expected. Verify `cargo check -p azul-layout` passes.\"").unwrap();
    writeln!(f, "  }},").unwrap();
    writeln!(f, "  {{").unwrap();
    writeln!(f, "    \"group_id\": 2,").unwrap();
    writeln!(f, "    \"action\": \"PICK_ONE\",").unwrap();
    writeln!(f, "    \"patches\": [\"line-breaking_008.md.done.001.patch\", \"line-breaking_015.md.done.001.patch\", \"line-breaking_040.md.done.001.patch\"],").unwrap();
    writeln!(f, "    \"preferred\": \"line-breaking_008.md.done.001.patch\",").unwrap();
    writeln!(f, "    \"agent_context\": \"## Intent\\nAdd `word-break` CSS property (CSS Text 3 §5.2): break-all, keep-all, normal.\\n\\n## Why _008 is preferred\\n- Correctly suppresses hyphenation for break-all (spec: 'Hyphenation is not applied')\\n- Has proper Hash/Eq derives needed for caching\\n- Clean 3-way match in peek_next_unit_with_word_break\\n- _015 misses hyphenation suppression; _040 also misses it and has less explicit CJK detection\\n\\n## Known Issues to Fix\\n- No CSS property wiring: `word_break` field in UnifiedConstraints defaults to Normal but is never read from CSS. You must add wiring in fc.rs where UnifiedConstraints is built — read word-break from the style and map to the WordBreak enum.\\n- The `break_one_line` signature changes — update ALL callers.\\n\\n## Existing Code to Reuse\\n- `is_word_separator()` already exists in cache.rs — the patch correctly uses it.\\n- `UnifiedConstraints` is built in `translate_to_text3_constraints()` in fc.rs — add word_break there.\"").unwrap();
    writeln!(f, "  }},").unwrap();
    writeln!(f, "  {{").unwrap();
    writeln!(f, "    \"group_id\": 3,").unwrap();
    writeln!(f, "    \"action\": \"SKIP\",").unwrap();
    writeln!(f, "    \"patches\": [\"display-property_001.md.done.001.patch\"],").unwrap();
    writeln!(f, "    \"preferred\": null,").unwrap();
    writeln!(f, "    \"agent_context\": \"SKIP — regression: replaces comprehensive get_display_type() blockification (handles TableRowGroup, RunIn, etc.) with a simpler version that loses these mappings.\"").unwrap();
    writeln!(f, "  }}").unwrap();
    writeln!(f, "]").unwrap();
    writeln!(f, "```\n").unwrap();
    writeln!(f, "IMPORTANT:").unwrap();
    writeln!(f, "- Every patch file must appear in exactly one group. Do not omit any patches.").unwrap();
    writeln!(f, "- The `agent_context` must be SELF-CONTAINED. The applying agent sees ONLY this field + the patch diffs.").unwrap();
    writeln!(f, "  It does NOT see the review, the conflict map, or any other groups. Include everything it needs.").unwrap();
    writeln!(f, "- Use markdown formatting in `agent_context` (## headings, bullet points) for clarity.").unwrap();
    writeln!(f, "- For ANNOT-only groups, `agent_context` can be brief: just say what spec paragraph to annotate.").unwrap();

    drop(f);

    let file_size = fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
    let est_tokens = file_size / 4;

    println!("Architecture review prompt generated: {}", out_path.display());
    println!("  {} patches inventoried ({} CODE, {} ANNOT)", patches.len(), code_count, annot_count);
    println!("  {} files with multi-patch conflicts", conflict_files.len());
    println!("  File size: {:.1} MB (~{:.0}K tokens)",
        file_size as f64 / 1_048_576.0, est_tokens as f64 / 1000.0);

    Ok(())
}

// ── agent-apply command ──────────────────────────────────────────────

/// Apply patches sequentially using an AI agent, guided by an architecture plan.
///
/// Reads the merge-group JSON (from Gemini's review-arch output), then processes
/// each group in order: SKIP groups are ignored, APPLY/PICK_ONE/MERGE groups
/// get dispatched to a `claude` agent that applies the patch(es) to the codebase.
/// Arguments for agent-apply, parsed from CLI flags.
pub struct AgentApplyArgs {
    pub patch_dir: String,
    pub groups_json: String,
    pub refactor_md: Option<String>,
    pub review_md: Option<String>,
    pub review_arch: Option<String>,
}

pub fn cmd_agent_apply(
    args: &AgentApplyArgs,
    workspace_root: &Path,
) -> Result<(), String> {

    // Helper: resolve a path relative to workspace_root if not absolute/found
    let resolve = |path: &str| -> PathBuf {
        let p = PathBuf::from(path);
        if p.exists() { p } else { workspace_root.join(path) }
    };

    // Resolve patch dir
    let patch_dir = {
        let resolved = resolve(&args.patch_dir);
        if resolved.is_dir() { resolved } else {
            return Err(format!("Patch directory not found: {}", args.patch_dir));
        }
    };

    // Read optional markdown context files
    let read_optional = |path: &Option<String>, label: &str| -> Result<Option<String>, String> {
        match path {
            Some(p) => {
                let resolved = resolve(p);
                let content = fs::read_to_string(&resolved)
                    .map_err(|e| format!("Failed to read {} {}: {}", label, resolved.display(), e))?;
                println!("Loaded {}: {} ({} bytes)", label, resolved.display(), content.len());
                Ok(Some(content))
            }
            None => Ok(None),
        }
    };

    let refactor_content = read_optional(&args.refactor_md, "--refactor-md")?;
    let review_content = read_optional(&args.review_md, "--review-md")?;
    let arch_content = read_optional(&args.review_arch, "--review-arch")?;

    // Read and parse the groups JSON
    let groups_path = resolve(&args.groups_json);
    let plan_content = fs::read_to_string(&groups_path)
        .map_err(|e| format!("Failed to read groups JSON {}: {}", groups_path.display(), e))?;

    let json_str = extract_json_from_plan(&plan_content)?;

    let groups: Vec<MergeGroup> = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse merge groups JSON: {}", e))?;

    println!("Loaded {} merge groups from {}", groups.len(), groups_path.display());

    // Progress tracking: record group outcomes in a status file.
    // Original .patch files are NEVER moved or deleted — we use
    // groups.json + source code +spec: markers to track progress.
    let status_path = patch_dir.join("agent-apply-status.json");
    let mut group_results: Vec<AgentApplyGroupResult> = Vec::new();

    // Load existing status to support resuming
    if status_path.exists() {
        if let Ok(content) = fs::read_to_string(&status_path) {
            if let Ok(existing) = serde_json::from_str::<Vec<AgentApplyGroupResult>>(&content) {
                group_results = existing;
            }
        }
    }

    let mut applied = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    // Get HEAD before we start so we can count commits per group
    let head_before_all = get_git_head(workspace_root);

    for group in &groups {
        // Skip groups we already processed in a previous run
        if let Some(prev) = group_results.iter().find(|r| r.group_id == group.group_id) {
            match prev.outcome.as_str() {
                "applied" | "skipped" | "failed" => {
                    match prev.outcome.as_str() {
                        "applied" => applied += prev.patch_count,
                        "skipped" => skipped += prev.patch_count,
                        "failed" => failed += prev.patch_count,
                        _ => {}
                    }
                    println!("  [group {}] already {} (previous run)", group.group_id, prev.outcome);
                    continue;
                }
                // "partial" or "in_progress" — remove stale entry and re-process
                // (sub-group tags handle fine-grained resume)
                other => {
                    println!("  [group {}] previous run was '{}', re-processing...", group.group_id, other);
                    group_results.retain(|r| r.group_id != group.group_id);
                }
            }
        }

        match group.action.as_str() {
            "SKIP" => {
                println!("  [group {}] SKIP: {}", group.group_id,
                    group.patches.join(", "));
                skipped += group.patches.len();
                group_results.push(AgentApplyGroupResult {
                    group_id: group.group_id,
                    action: "SKIP".to_string(),
                    patches: group.patches.clone(),
                    patch_count: group.patches.len(),
                    outcome: "skipped".to_string(),
                    commits: 0,
                    reason: Some("SKIP action in groups.json".to_string()),
                    sub_results: None,
                });
                save_status(&status_path, &group_results);
                continue;
            }
            "APPLY" | "PICK_ONE" | "MERGE" => {}
            other => {
                eprintln!("  [group {}] Unknown action '{}', skipping", group.group_id, other);
                skipped += group.patches.len();
                group_results.push(AgentApplyGroupResult {
                    group_id: group.group_id,
                    action: other.to_string(),
                    patches: group.patches.clone(),
                    patch_count: group.patches.len(),
                    outcome: "skipped".to_string(),
                    commits: 0,
                    reason: Some(format!("Unknown action '{}'", other)),
                    sub_results: None,
                });
                save_status(&status_path, &group_results);
                continue;
            }
        }

        // Determine which patches to give the agent
        let active_patches: Vec<String> = match group.action.as_str() {
            "PICK_ONE" => {
                // Use preferred if specified, otherwise first
                let preferred = group.preferred.as_deref()
                    .unwrap_or_else(|| &group.patches[0]);
                vec![preferred.to_string()]
            }
            _ => group.patches.clone(), // APPLY and MERGE: give all patches
        };

        // Read patch contents
        let mut patch_contents = Vec::new();
        let mut missing = false;
        for p in &active_patches {
            let path = patch_dir.join(p);
            match fs::read_to_string(&path) {
                Ok(content) => patch_contents.push((p.clone(), content)),
                Err(e) => {
                    eprintln!("  [group {}] Missing patch {}: {}", group.group_id, p, e);
                    missing = true;
                }
            }
        }
        if missing {
            failed += active_patches.len();
            group_results.push(AgentApplyGroupResult {
                group_id: group.group_id,
                action: group.action.clone(),
                patches: group.patches.clone(),
                patch_count: active_patches.len(),
                outcome: "failed".to_string(),
                commits: 0,
                reason: Some("Missing patch file(s)".to_string()),
                sub_results: None,
            });
            save_status(&status_path, &group_results);
            continue;
        }

        // Detect annotation-only group (same logic as build_apply_prompt)
        let is_annot_only = group.agent_context.as_deref()
            .map(|ctx| ctx.contains("annotation comment") || ctx.contains("// +spec:"))
            .unwrap_or(false)
            && group.action == "APPLY"
            && patch_contents.len() > 10;

        // ── Large-group splitting by feature prefix ──────────────────
        if group.action == "APPLY" && patch_contents.len() > 15 && !is_annot_only {
            use std::collections::BTreeMap;

            println!("  [group {}] APPLY {} patches (splitting by feature prefix)",
                group.group_id, patch_contents.len());

            // Group patches by feature prefix
            let mut by_feature: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
            for (name, content) in &patch_contents {
                let feature = name.find('_')
                    .map(|i| name[..i].to_string())
                    .unwrap_or_else(|| "misc".to_string());
                by_feature.entry(feature).or_default().push((name.clone(), content.clone()));
            }

            let feature_names: Vec<String> = by_feature.keys().cloned().collect();
            println!("    -> {} sub-groups: {}", feature_names.len(),
                feature_names.iter().map(|f| format!("{}({})", f, by_feature[f].len()))
                    .collect::<Vec<_>>().join(", "));

            // Run refactoring prep agent first (if refactor content is available)
            let refactor_tag = format!("agent-apply/group-{}-refactor", group.group_id);
            let refactor_done = {
                let output = Command::new("git")
                    .args(["tag", "-l", &refactor_tag])
                    .current_dir(workspace_root)
                    .output();
                matches!(output, Ok(o) if !o.stdout.is_empty())
            };

            if refactor_content.is_some() && !refactor_done {
                println!("    -> running refactoring prep sub-agent...");
                let refactor_prompt = build_refactor_prep_prompt(
                    &feature_names,
                    refactor_content.as_deref(),
                    workspace_root,
                );
                let head_before_refactor = get_git_head(workspace_root);
                let refactor_ok = run_apply_agent(
                    &refactor_prompt, workspace_root, group.group_id,
                )?;
                let refactor_commits = count_commits_since(workspace_root, &head_before_refactor);
                if refactor_ok && refactor_commits > 0 {
                    let _ = Command::new("git")
                        .args(["tag", &refactor_tag])
                        .current_dir(workspace_root)
                        .output();
                    println!("    -> refactor prep: {} commit(s), tagged as {}", refactor_commits, refactor_tag);
                } else {
                    println!("    -> refactor prep: no commits (ok, continuing with feature sub-groups)");
                }
            } else if refactor_done {
                println!("    -> refactor prep already done (tag exists)");
            }

            // Process each feature sub-group
            let mut sub_results: Vec<SubGroupResult> = Vec::new();
            let mut total_sub_commits = 0usize;
            let mut total_sub_applied = 0usize;
            let mut total_sub_failed = 0usize;

            for (feature, sub_patches) in &by_feature {
                // Check if already done (resume support)
                if is_sub_group_done(workspace_root, group.group_id, feature) {
                    println!("    [sub:{}] already done (tag exists), skipping", feature);
                    total_sub_applied += sub_patches.len();
                    sub_results.push(SubGroupResult {
                        feature: feature.clone(),
                        patches: sub_patches.iter().map(|(n, _)| n.clone()).collect(),
                        outcome: "applied".to_string(),
                        commits: 0, // unknown from previous run
                        tag: Some(format!("agent-apply/group-{}-{}", group.group_id, feature)),
                        reason: Some("resumed: tag already existed".to_string()),
                    });
                    continue;
                }

                println!("    [sub:{}] applying {} patches...", feature, sub_patches.len());

                let head_before_sub = get_git_head(workspace_root);

                // Build a synthetic MergeGroup for this sub-group
                let sub_group = MergeGroup {
                    group_id: group.group_id,
                    action: "APPLY".to_string(),
                    patches: sub_patches.iter().map(|(n, _)| n.clone()).collect(),
                    preferred: None,
                    agent_context: group.agent_context.clone(),
                    notes: group.notes.clone(),
                };

                let sub_prompt = build_apply_prompt(
                    &sub_group,
                    sub_patches,
                    refactor_content.as_deref(),
                    review_content.as_deref(),
                    arch_content.as_deref(),
                    workspace_root,
                );

                let sub_ok = run_apply_agent(&sub_prompt, workspace_root, group.group_id)?;
                let sub_commit_count = count_commits_since(workspace_root, &head_before_sub);

                let sub_tag_name = format!("agent-apply/group-{}-{}", group.group_id, feature);

                if sub_ok && sub_commit_count > 0 {
                    total_sub_applied += sub_patches.len();
                    total_sub_commits += sub_commit_count;

                    let _ = Command::new("git")
                        .args(["tag", &sub_tag_name])
                        .current_dir(workspace_root)
                        .output();
                    println!("    [sub:{}] -> {} commit(s), tagged {}", feature, sub_commit_count, sub_tag_name);

                    sub_results.push(SubGroupResult {
                        feature: feature.clone(),
                        patches: sub_patches.iter().map(|(n, _)| n.clone()).collect(),
                        outcome: "applied".to_string(),
                        commits: sub_commit_count,
                        tag: Some(sub_tag_name),
                        reason: None,
                    });
                } else {
                    // Check for partial success or token exhaustion
                    let reason = if sub_commit_count > 0 {
                        total_sub_applied += sub_patches.len(); // partial counts as progress
                        total_sub_commits += sub_commit_count;
                        let _ = Command::new("git")
                            .args(["tag", &sub_tag_name])
                            .current_dir(workspace_root)
                            .output();
                        format!("partial: agent failed but made {} commit(s)", sub_commit_count)
                    } else {
                        total_sub_failed += sub_patches.len();
                        "Agent exited without making any commits".to_string()
                    };
                    println!("    [sub:{}] -> FAILED: {}", feature, reason);

                    sub_results.push(SubGroupResult {
                        feature: feature.clone(),
                        patches: sub_patches.iter().map(|(n, _)| n.clone()).collect(),
                        outcome: if sub_commit_count > 0 { "partial".to_string() } else { "failed".to_string() },
                        commits: sub_commit_count,
                        tag: if sub_commit_count > 0 { Some(sub_tag_name) } else { None },
                        reason: Some(reason),
                    });
                }

                // Save status after each sub-group for resume support
                // We save a temporary group result that gets replaced at the end
                let temp_result = AgentApplyGroupResult {
                    group_id: group.group_id,
                    action: group.action.clone(),
                    patches: group.patches.clone(),
                    patch_count: active_patches.len(),
                    outcome: "in_progress".to_string(),
                    commits: total_sub_commits,
                    reason: None,
                    sub_results: Some(sub_results.clone()),
                };
                let mut temp_results = group_results.clone();
                temp_results.push(temp_result);
                save_status(&status_path, &temp_results);
            }

            // Final group result for the split group
            let overall_outcome = if total_sub_failed == 0 {
                "applied"
            } else if total_sub_applied > 0 {
                "partial"
            } else {
                "failed"
            };

            applied += total_sub_applied;
            failed += total_sub_failed;

            println!("    [group {}] split complete: {} applied, {} failed, {} total commits",
                group.group_id, total_sub_applied, total_sub_failed, total_sub_commits);

            group_results.push(AgentApplyGroupResult {
                group_id: group.group_id,
                action: group.action.clone(),
                patches: group.patches.clone(),
                patch_count: active_patches.len(),
                outcome: overall_outcome.to_string(),
                commits: total_sub_commits,
                reason: None,
                sub_results: Some(sub_results),
            });
            save_status(&status_path, &group_results);
            continue;
        }

        // ── Normal (non-split) path ──────────────────────────────────
        println!("  [group {}] {} {} patches: {}",
            group.group_id,
            group.action,
            active_patches.len(),
            active_patches.join(", "),
        );

        // Record HEAD before this group so we can count new commits
        let head_before = get_git_head(workspace_root);

        // Build agent prompt
        let prompt = build_apply_prompt(
            &group,
            &patch_contents,
            refactor_content.as_deref(),
            review_content.as_deref(),
            arch_content.as_deref(),
            workspace_root,
        );

        // Run agent
        let agent_ok = run_apply_agent(&prompt, workspace_root, group.group_id)?;

        // Count commits made by the agent
        let commit_count = count_commits_since(workspace_root, &head_before);

        if agent_ok && commit_count > 0 {
            applied += active_patches.len();
            println!("    -> {} new commit(s)", commit_count);

            // Tag HEAD so we can trace which commits belong to this group
            let tag_name = format!("agent-apply/group-{}", group.group_id);
            let tag_result = Command::new("git")
                .args(["tag", &tag_name])
                .current_dir(workspace_root)
                .output();
            match tag_result {
                Ok(o) if o.status.success() => {
                    println!("    -> tagged as {}", tag_name);
                }
                _ => {
                    eprintln!("    -> warning: failed to create tag {}", tag_name);
                }
            }

            group_results.push(AgentApplyGroupResult {
                group_id: group.group_id,
                action: group.action.clone(),
                patches: group.patches.clone(),
                patch_count: active_patches.len(),
                outcome: "applied".to_string(),
                commits: commit_count,
                reason: None,
                sub_results: None,
            });
        } else {
            failed += active_patches.len();
            let reason = if commit_count == 0 {
                "Agent exited without making any commits".to_string()
            } else {
                format!("Agent failed (exit code) but made {} commit(s)", commit_count)
            };
            println!("    -> FAILED: {}", reason);
            group_results.push(AgentApplyGroupResult {
                group_id: group.group_id,
                action: group.action.clone(),
                patches: group.patches.clone(),
                patch_count: active_patches.len(),
                outcome: if commit_count > 0 { "partial".to_string() } else { "failed".to_string() },
                commits: commit_count,
                reason: Some(reason),
                sub_results: None,
            });
        }
        save_status(&status_path, &group_results);
    }

    println!("\nAgent apply complete:");
    println!("  Applied: {}", applied);
    println!("  Skipped: {}", skipped);
    println!("  Failed:  {}", failed);
    println!("  Status:  {}", status_path.display());

    Ok(())
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct MergeGroup {
    group_id: usize,
    action: String,
    patches: Vec<String>,
    preferred: Option<String>,
    /// Rich context passed verbatim to the applying agent.
    /// Includes: spec reference, known bugs, refactoring notes, reusable functions.
    agent_context: Option<String>,
    /// Legacy field — kept for backward compat with older plans
    notes: Option<String>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct AgentApplyGroupResult {
    group_id: usize,
    action: String,
    patches: Vec<String>,
    patch_count: usize,
    outcome: String, // "applied", "skipped", "failed", "partial"
    commits: usize,
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sub_results: Option<Vec<SubGroupResult>>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct SubGroupResult {
    feature: String,
    patches: Vec<String>,
    outcome: String, // "applied", "skipped", "failed"
    commits: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

fn save_status(path: &Path, results: &[AgentApplyGroupResult]) {
    if let Ok(json) = serde_json::to_string_pretty(results) {
        let _ = fs::write(path, json);
    }
}

fn get_git_head(workspace_root: &Path) -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(workspace_root)
        .output()
        .ok()
        .and_then(|o| if o.status.success() {
            String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
        } else {
            None
        })
        .unwrap_or_default()
}

fn count_commits_since(workspace_root: &Path, since_ref: &str) -> usize {
    if since_ref.is_empty() {
        return 0;
    }
    Command::new("git")
        .args(["rev-list", "--count", &format!("{}..HEAD", since_ref)])
        .current_dir(workspace_root)
        .output()
        .ok()
        .and_then(|o| if o.status.success() {
            String::from_utf8(o.stdout).ok()
                .and_then(|s| s.trim().parse::<usize>().ok())
        } else {
            None
        })
        .unwrap_or(0)
}

fn is_sub_group_done(workspace_root: &Path, group_id: usize, feature: &str) -> bool {
    let tag = format!("agent-apply/group-{}-{}", group_id, feature);
    let output = Command::new("git")
        .args(["tag", "-l", &tag])
        .current_dir(workspace_root)
        .output();
    matches!(output, Ok(o) if !o.stdout.is_empty())
}

fn detect_failure_reason(stdout: &str) -> Option<String> {
    let token_patterns = [
        "token limit",
        "context window",
        "rate limit",
        "maximum output length",
    ];
    for pat in &token_patterns {
        if stdout.to_lowercase().contains(pat) {
            return Some(format!("token_exhaustion (matched: '{}')", pat));
        }
    }
    None
}

fn build_refactor_prep_prompt(
    features: &[String],
    refactor_content: Option<&str>,
    workspace_root: &Path,
) -> String {
    let mut prompt = String::new();
    prompt.push_str("You are preparing a CSS layout engine (Rust) for a batch of spec patches.\n");
    prompt.push_str(&format!("Working directory: {}\n\n", workspace_root.display()));

    prompt.push_str("## CRITICAL RULES\n\n");
    prompt.push_str("1. DO NOT cd TO ANY OTHER DIRECTORY. Stay in the working directory.\n");
    prompt.push_str("2. Compile with: `cargo check -p azul-dll --features build-dll`\n");
    prompt.push_str("3. Commit messages: `refactor(<area>): <what it does>`\n\n");

    prompt.push_str("## Task\n\n");
    prompt.push_str("Perform structural refactoring to prepare the codebase for the following features:\n\n");
    for f in features {
        prompt.push_str(&format!("- `{}`\n", f));
    }
    prompt.push_str("\n");

    prompt.push_str("This is a **prep-only** step. Do NOT implement any spec features.\n");
    prompt.push_str("Focus on:\n");
    prompt.push_str("- Extracting helpers that multiple features will need\n");
    prompt.push_str("- Adding enums or types that patches will reference\n");
    prompt.push_str("- Cleaning up code regions that will be modified\n\n");

    if let Some(refactor) = refactor_content {
        prompt.push_str("## Refactoring Plan\n\n");
        prompt.push_str(refactor);
        prompt.push_str("\n\n");
    }

    prompt.push_str("## Workflow\n\n");
    prompt.push_str("1. Read the refactoring plan above\n");
    prompt.push_str("2. Read the current source files in `layout/src/solver3/`\n");
    prompt.push_str("3. Make targeted structural changes\n");
    prompt.push_str("4. Compile: `cargo check -p azul-dll --features build-dll`\n");
    prompt.push_str("5. Commit: `refactor: prep for <features>`\n\n");

    prompt
}

fn extract_json_from_plan(content: &str) -> Result<String, String> {
    // Try to find JSON array in the content (may be in a code fence)
    let content = content.trim();

    // If it starts with '[', it's raw JSON
    if content.starts_with('[') {
        return Ok(content.to_string());
    }

    // Look for ```json ... ``` block
    if let Some(start) = content.find("```json") {
        let after = &content[start + 7..];
        if let Some(end) = after.find("```") {
            return Ok(after[..end].trim().to_string());
        }
    }

    // Look for just ``` ... ``` with JSON inside
    if let Some(start) = content.find("```") {
        let after = &content[start + 3..];
        // Skip optional language tag on same line
        let after = if let Some(nl) = after.find('\n') {
            &after[nl + 1..]
        } else {
            after
        };
        if let Some(end) = after.find("```") {
            let candidate = after[..end].trim();
            if candidate.starts_with('[') {
                return Ok(candidate.to_string());
            }
        }
    }

    // Look for first '[' to last ']'
    if let Some(start) = content.find('[') {
        if let Some(end) = content.rfind(']') {
            return Ok(content[start..=end].to_string());
        }
    }

    Err("Could not find JSON merge-group array in arch plan file".to_string())
}

fn build_apply_prompt(
    group: &MergeGroup,
    patch_contents: &[(String, String)],
    refactor_content: Option<&str>,
    review_content: Option<&str>,
    arch_content: Option<&str>,
    workspace_root: &Path,
) -> String {
    let mut prompt = String::new();

    prompt.push_str("You are applying patches to a CSS layout engine written in Rust.\n");
    prompt.push_str(&format!("Working directory: {}\n\n", workspace_root.display()));

    prompt.push_str("## CRITICAL RULES\n\n");
    prompt.push_str("1. DO NOT cd TO ANY OTHER DIRECTORY. Stay in the working directory.\n");
    prompt.push_str("2. Apply the SEMANTIC INTENT of the patches, not the literal diff.\n");
    prompt.push_str("   The patches were generated against a different version of the code.\n");
    prompt.push_str("   Read the current source files, understand what the patches want to change,\n");
    prompt.push_str("   and make the equivalent change to the current code.\n");
    prompt.push_str("3. Compile with: `cargo check -p azul-dll --features build-dll`\n");
    prompt.push_str("   This is the ONLY valid compilation check. Do NOT use `cargo check -p azul-layout`.\n");
    prompt.push_str("4. If compilation fails, fix the errors. Do NOT leave broken code.\n");
    prompt.push_str("5. Commit messages should follow: `spec(<feature>): <what it does>`\n");
    prompt.push_str("   Example: `spec(line-breaking): implement word-break and line-break CSS properties`\n\n");

    // ── 5-Phase Workflow ───────────────────────────────────────────────
    prompt.push_str("## Workflow (5 phases, in order)\n\n");

    prompt.push_str("### Phase 1: Refactoring Groundwork\n\n");
    prompt.push_str("Before touching the patches, perform any refactoring needed to prepare the codebase.\n");
    prompt.push_str("This ensures patches plug into clean abstractions rather than scattering ad-hoc logic.\n");
    prompt.push_str("Do NOT commit yet — all committing happens in Phase 5.\n\n");

    prompt.push_str("### Phase 2: Apply Patches (LLM-apply)\n\n");
    prompt.push_str("Read each patch diff below. Understand the semantic intent. Apply the changes to the\n");
    prompt.push_str("current codebase, adapting line numbers, function signatures, and context as needed.\n");
    prompt.push_str("For MERGE groups: combine the best parts of all patches into one coherent implementation.\n");
    prompt.push_str("For PICK_ONE groups: apply the preferred patch, verify nothing unique is lost from others.\n");
    prompt.push_str("Do NOT commit yet.\n\n");

    prompt.push_str("### Phase 3: Compile Check\n\n");
    prompt.push_str("Run: `cargo check -p azul-dll --features build-dll`\n");
    prompt.push_str("Fix ALL compilation errors. Do NOT commit yet.\n\n");

    prompt.push_str("### Phase 4: Review Against Spec\n\n");
    prompt.push_str("Re-read the patch diffs and the agent_context. Verify:\n");
    prompt.push_str("- The implementation matches the referenced W3C spec section\n");
    prompt.push_str("- No logic was lost or incorrectly adapted\n");
    prompt.push_str("- Edge cases mentioned in agent_context are handled\n");
    prompt.push_str("If you find issues, fix them. Then compile again:\n");
    prompt.push_str("  `cargo check -p azul-dll --features build-dll`\n\n");

    prompt.push_str("### Phase 5: Commit (semantic, by hunk)\n\n");
    prompt.push_str("Now that all changes are made and compiling, create CLEAN SEMANTIC commits.\n");
    prompt.push_str("Use `git add -p` (patch mode) to stage hunks selectively, creating multiple\n");
    prompt.push_str("smaller commits that each represent one logical change. The goal is ~2-5\n");
    prompt.push_str("well-structured commits per group, NOT one giant commit.\n\n");
    prompt.push_str("For example:\n");
    prompt.push_str("  1. `refactor(fc): extract float overlap check into helper` (if Phase 1 refactored)\n");
    prompt.push_str("  2. `spec(block-formatting-context): implement BFC float clearance per §9.5.2`\n");
    prompt.push_str("  3. `spec(block-formatting-context): annotate float positioning rules`\n\n");
    prompt.push_str("Each commit must compile on its own. Use `git stash` to verify if needed.\n\n");

    // ── Detect annotation-only group ───────────────────────────────────
    let is_annot_only = group.agent_context.as_deref()
        .map(|ctx| ctx.contains("annotation comment") || ctx.contains("// +spec:"))
        .unwrap_or(false)
        && group.action == "APPLY"
        && patch_contents.len() > 10;

    // ── Task type ──────────────────────────────────────────────────────
    match group.action.as_str() {
        "APPLY" if is_annot_only => {
            prompt.push_str("## Task Type: APPLY (annotation-only fast path)\n\n");
            prompt.push_str("These patches ONLY add `// +spec:` annotation comments. They do NOT change\n");
            prompt.push_str("any runtime behaviour.\n\n");
            prompt.push_str("### Fast-path instructions\n\n");
            prompt.push_str("Use `git am` to apply these patches ONE BY ONE. For each patch:\n");
            prompt.push_str("1. Run `git am --3way <patch-file>`\n");
            prompt.push_str("2. If it fails, run `git am --abort`, skip that patch, and continue with the next\n");
            prompt.push_str("3. Do NOT spend time manually resolving conflicts — just skip failed patches\n\n");
            prompt.push_str("After applying all patches, SQUASH all the resulting commits into ONE commit:\n");
            prompt.push_str("```\n");
            prompt.push_str("git reset --soft <HEAD-before-first-am>\n");
            prompt.push_str("git commit -m \"spec: add +spec annotation comments (batch)\"\n");
            prompt.push_str("```\n\n");
            prompt.push_str("This is the ONLY group where `git am` is allowed. All other groups must use\n");
            prompt.push_str("the semantic LLM-apply workflow (Phase 1-5 above).\n\n");
            prompt.push_str("Report how many patches applied vs skipped at the end.\n\n");
        }
        "APPLY" => {
            prompt.push_str("## Task Type: APPLY\n\n");
            prompt.push_str("Apply the following patch(es) to the codebase. These are independent patches\n");
            prompt.push_str("that don't conflict with each other.\n\n");
        }
        "PICK_ONE" => {
            prompt.push_str("## Task Type: PICK_ONE\n\n");
            prompt.push_str("Multiple patches implement the same feature. The preferred patch has been selected.\n");
            prompt.push_str("Apply it, but review the others to ensure no unique logic is lost.\n\n");
        }
        "MERGE" => {
            prompt.push_str("## Task Type: MERGE\n\n");
            prompt.push_str("Multiple patches modify the same code region with overlapping changes.\n");
            prompt.push_str("Read ALL patches, understand the combined intent, and produce a single\n");
            prompt.push_str("coherent implementation incorporating the best parts of each.\n\n");
        }
        _ => {}
    }

    // ── Extra instructions for CODE groups ───────────────────────────
    if !is_annot_only {
        prompt.push_str("## Additional Guidelines\n\n");
        prompt.push_str("### Sanity-check larger code sections\n");
        prompt.push_str("For any code change spanning more than 5 lines, pause and verify:\n");
        prompt.push_str("- Does the logic actually match the referenced W3C spec section?\n");
        prompt.push_str("- Are there off-by-one errors in box model math?\n");
        prompt.push_str("- Does the patch use APIs/functions that actually exist in the current code?\n");
        prompt.push_str("  (Remember: patches were generated by agents that may have hallucinated APIs.)\n\n");
        prompt.push_str("### Cross-crate changes are allowed\n");
        prompt.push_str("If a patch requires changes outside `layout/`, you may modify other crates:\n");
        prompt.push_str("- `css/src/` — CSS property types, parsing, enums\n");
        prompt.push_str("- `core/src/` — core types, callbacks, DOM structures\n");
        prompt.push_str("- `dll/src/` — FFI layer (if new types need to be exposed)\n");
        prompt.push_str("The compilation check (`cargo check -p azul-dll --features build-dll`) covers all of these.\n\n");
        prompt.push_str("### Learn from recent git history\n");
        prompt.push_str("Before making changes, run `git log --oneline -20` to understand what refactoring\n");
        prompt.push_str("has already been done on this branch. This helps you understand the current state\n");
        prompt.push_str("of the code and avoid reverting recent improvements.\n\n");
    }

    // ── Collect feature names for this group (for filtering context) ──
    let group_features: Vec<String> = group.patches.iter()
        .filter_map(|p| {
            // "line-breaking_040.md.done.001.patch" → "line-breaking"
            p.find('_').map(|i| p[..i].to_string())
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    // ── Filter helper: keep only lines/sections mentioning group features ──
    let filter_by_relevance = |full_text: &str| -> String {
        // For small groups (≤10 patches), filter context to relevant sections.
        // For large groups (like group 17 with 117 patches), include everything
        // since filtering would remove too much.
        if group_features.len() > 8 {
            return full_text.to_string();
        }

        let mut result = String::new();
        let mut include_section = false;
        for line in full_text.lines() {
            // Section headers (## or ###) reset the inclusion decision
            if line.starts_with("## ") || line.starts_with("### ") {
                include_section = group_features.iter().any(|f| {
                    line.to_lowercase().contains(&f.replace('-', " "))
                        || line.to_lowercase().contains(&f.replace('-', "-"))
                }) || line.contains("Cluster") && group_features.iter().any(|f| {
                    // Also match if the section body mentions our patches
                    true // will be refined by patch name matching below
                });
                // Always include top-level headings for structure
                if line.starts_with("## ") && !line.starts_with("### ") {
                    result.push_str(line);
                    result.push('\n');
                    continue;
                }
            }
            // Also include lines that directly mention any of our patch names
            if !include_section {
                let dominated_by_patch = group.patches.iter().any(|p| {
                    let stem = p.split('.').next().unwrap_or(p);
                    line.contains(stem)
                });
                if dominated_by_patch {
                    include_section = true;
                }
            }
            if include_section {
                result.push_str(line);
                result.push('\n');
            }
        }
        if result.trim().is_empty() {
            // Fallback: include everything if filtering removed all content
            full_text.to_string()
        } else {
            result
        }
    };

    // ── Refactoring groundwork (from --refactor-md) ──────────────────
    if let Some(refactor) = refactor_content {
        prompt.push_str("## Refactoring Plan (Phase 1 reference)\n\n");
        prompt.push_str("Only sections relevant to this group's features are shown.\n\n");
        let filtered = filter_by_relevance(refactor);
        prompt.push_str(&filtered);
        prompt.push_str("\n\n");
    }

    // ── Patch review summary (from --review-md) ────────────────────────
    if let Some(review) = review_content {
        prompt.push_str("## Patch Review Summary (filtered to relevant features)\n\n");
        let filtered = filter_by_relevance(review);
        prompt.push_str(&filtered);
        prompt.push_str("\n\n");
    }

    // ── Architecture review (from --review-arch) ────────────────────────
    if let Some(arch) = arch_content {
        prompt.push_str("## Architecture Review (filtered to relevant sections)\n\n");
        let filtered = filter_by_relevance(arch);
        prompt.push_str(&filtered);
        prompt.push_str("\n\n");
    }

    // ── Per-group agent context (from groups JSON agent_context field) ──
    if let Some(ctx) = &group.agent_context {
        prompt.push_str("## Group-Specific Instructions\n\n");
        prompt.push_str("The following instructions are specific to THIS merge group.\n");
        prompt.push_str("This is your primary source of truth for what to do.\n\n");
        prompt.push_str(ctx);
        prompt.push_str("\n\n");
    } else if let Some(notes) = &group.notes {
        prompt.push_str(&format!("## Reviewer Notes\n\n{}\n\n", notes));
    }

    // ── Load spec paragraphs from original prompt files ────────────────
    let prompts_dir = workspace_root.join("doc/target/skill_tree/prompts");
    let extract_spec_paragraph = |patch_name: &str| -> Option<String> {
        // "line-breaking_040.md.done.001.patch" → "line-breaking_040.md"
        let prompt_stem = patch_name
            .strip_suffix(".patch")
            .and_then(|s| s.find(".md.done").map(|i| &s[..i + 3]))?;
        let prompt_path = prompts_dir.join(prompt_stem);
        let content = fs::read_to_string(&prompt_path).ok()?;
        // Extract text between "## Spec Paragraph to Verify" and "## Instructions"
        let start = content.find("## Spec Paragraph to Verify")?;
        let after = &content[start..];
        let end = after.find("## Instructions").unwrap_or(after.len());
        let section = &after[..end];
        // Skip the heading line itself
        let body: String = section.lines()
            .skip(1)
            .collect::<Vec<&str>>()
            .join("\n");
        let trimmed = body.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    };

    // ── Patch diffs ────────────────────────────────────────────────────
    if is_annot_only {
        // For annotation-only groups, just list the patch file paths
        // (the agent will use `git am` directly on the files)
        let patch_dir_display = workspace_root.join("doc/target/skill_tree/all_patches/run2_patches");
        prompt.push_str(&format!("## Patch Directory\n\n`{}`\n\n", patch_dir_display.display()));
        prompt.push_str("## Patch Files (apply in this order)\n\n");
        for (name, _) in patch_contents {
            prompt.push_str(&format!("- `{}`\n", name));
        }
        prompt.push_str("\n");
    } else {
        for (name, content) in patch_contents {
            prompt.push_str(&format!("## Patch: {}\n\n", name));

            // Include the original W3C spec paragraph for sanity-checking
            if let Some(spec_para) = extract_spec_paragraph(name) {
                prompt.push_str("### W3C Spec Paragraph (verify implementation against this)\n\n");
                prompt.push_str(&spec_para);
                prompt.push_str("\n\n");
            }

            prompt.push_str("### Diff\n\n");
            prompt.push_str("```diff\n");
            let mut in_diff = false;
            for line in content.lines() {
                if line.starts_with("diff --git") {
                    in_diff = true;
                }
                if in_diff {
                    prompt.push_str(line);
                    prompt.push('\n');
                }
            }
            prompt.push_str("```\n\n");
        }
    }

    prompt
}

fn run_apply_agent(
    prompt: &str,
    workspace_root: &Path,
    group_id: usize,
) -> Result<bool, String> {

    // Write prompt to temp file
    let prompt_path = workspace_root.join(format!(".claude-agents/apply-group-{}.md", group_id));
    let _ = fs::create_dir_all(prompt_path.parent().unwrap());
    fs::write(&prompt_path, prompt)
        .map_err(|e| format!("Failed to write prompt: {}", e))?;

    let result_path = workspace_root.join(format!(".claude-agents/apply-group-{}.result", group_id));

    let start = Instant::now();

    let mut child = Command::new("claude")
        .args([
            "-p",
            "--dangerously-skip-permissions",
            "--verbose",
            "--output-format", "stream-json",
            "--disallowedTools",
            "Bash(cargo build*)", "Bash(cargo run*)", "Bash(cargo test*)",
            "mcp__*", "rust-analyzer-lsp",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(workspace_root)
        .env("GIT_DIR", workspace_root.join(".git"))
        .env("GIT_WORK_TREE", workspace_root)
        .spawn()
        .map_err(|e| format!("Failed to spawn claude: {}", e))?;

    // Write prompt to stdin
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes());
    }

    // Wait with timeout (60 minutes)
    let timeout = Duration::from_secs(3600);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let elapsed = start.elapsed().as_secs();
                let stdout = child.stdout.take()
                    .map(|mut s| {
                        let mut buf = String::new();
                        std::io::Read::read_to_string(&mut s, &mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();

                let _ = fs::write(&result_path, &stdout);

                // Detect token exhaustion or rate limits in output
                if let Some(failure_reason) = detect_failure_reason(&stdout) {
                    println!("    -> DETECTED: {} ({}s)", failure_reason, elapsed);
                }

                if status.success() {
                    println!("    -> OK ({}s)", elapsed);
                    return Ok(true);
                } else {
                    println!("    -> FAILED exit={} ({}s)", status.code().unwrap_or(-1), elapsed);
                    return Ok(false);
                }
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    println!("    -> TIMEOUT ({}s)", timeout.as_secs());
                    return Ok(false);
                }
                std::thread::sleep(Duration::from_secs(2));
            }
            Err(e) => {
                return Err(format!("Failed to wait for agent: {}", e));
            }
        }
    }
}

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

### CSS type definitions (css/src/props/):
- `layout/display.rs` — LayoutDisplay enum (None, Block, Inline, InlineBlock, ...)
- `layout/spacing.rs` — Margin, Padding types
- `layout/dimensions.rs` — Width, Height, box-sizing
- `layout/overflow.rs` — Overflow enum
- `layout/position.rs` — Position enum (static, relative, absolute, fixed, sticky)
- `style/text.rs` — StyleLineHeight, StyleWhiteSpace, StyleHyphens, StyleLineBreak, StyleTextIndent

### Core types (core/src/):
- `dom.rs` — FormattingContext enum, NodeType
- `styled_dom.rs` — StyledDom, styled node access
"#;

fn build_agent_instructions(feature_id: &str, spec_tags: &[String]) -> String {
    let grep_pattern = spec_tags.iter()
        .map(|t| format!("+spec:{}", t))
        .collect::<Vec<_>>()
        .join("\\|");
    let grep_cmd = format!("grep -rn \"{}\" layout/src/", grep_pattern);

    let marker_instructions = spec_tags.iter()
        .map(|t| format!("  `// +spec:{} - <brief description>`", t))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"## Your Task

### Step 1: Check if the feature is already implemented

First, check whether the spec paragraphs' requirements are ALREADY correctly
implemented in the codebase. Many features are already working — your job is
to verify, not to blindly add code.

```
{grep_cmd}
```

Read the relevant source files listed in the codebase orientation above.
Find the EXACT code path that handles each spec paragraph's requirements.

### Step 2: Evaluate the implementation

Your job is to be a SKEPTICAL REVIEWER. Do not assume the code is correct.
For each requirement in each spec paragraph:

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

### Step 3: Act based on what you found

**If the code ALREADY correctly implements a paragraph:**
- Add the appropriate marker comment at EVERY implementation site
  (there may be multiple). Use the tag for the specific paragraph:
{marker_instructions}
- Find each function/match-arm/code-block that handles this behavior
  and add the marker on the line above.
- Commit with the marker comments only.
- This is the EXPECTED outcome for most paragraphs.

**If the code is MISSING functionality required by a paragraph:**
- Implement it. Use the Edit tool to modify source files.
- Make minimal, focused changes — only what the paragraph requires.
- You MAY add new functions, new match arms, new fields, or new files.
- Do NOT refactor surrounding code or add unrelated improvements.
- Add `// +spec:<tag> - <what this implements>` at each implementation site.

**If the code has a BUG relative to a paragraph's requirements:**
- Fix the bug. Show the before/after logic clearly in the commit message.
- Add `// +spec:<tag> - <what this fixes>` at the fix site.

**If a spec paragraph does NOT apply to this codebase** (e.g., it only
applies to flex/grid which is handled by Taffy, or it describes user agent
behavior we don't implement):
- Do NOT commit for that paragraph. Output `NOT_APPLICABLE` and move on.

**Before implementing anything, search for existing functions and types.**
Many CSS properties, enums, getters, and helper functions already exist in the
codebase. Before creating a new type or function:
- `grep -rn "StylePropertyName\|PropertyName" css/src/` to check if the CSS
  enum/parser already exists.
- `grep -rn "get_property_name" layout/src/solver3/getters.rs` to check if a
  getter already exists.
- `grep -rn "PropertyName" core/src/prop_cache.rs` to check if the prop_cache
  getter exists.
- Reuse existing types and functions rather than creating duplicates.

**Flex and Grid layout is handled entirely by the Taffy library**, not by our
layout solver. Do NOT implement flex/grid layout logic. Do NOT modify
`taffy_bridge.rs`. If a spec paragraph only applies to flex/grid contexts,
output `NOT_APPLICABLE` and exit.

**Feature completeness is important.** If a spec paragraph references a CSS
property that doesn't exist yet in the codebase:
- Add the new CSS property variant to `css/src/css_properties.rs` (enum + parsing).
- Add the getter function in `layout/src/solver3/getters.rs`.
- Wire it into the layout algorithm where it's needed.
- If the spec paragraph affects text/inline layout, you may add new fields or
  logic in `layout/src/text3/cache.rs` or `layout/src/text3/knuth_plass.rs`.
- Do NOT leave a TODO — implement a working first version, even if approximate.
  We can refine the implementation later, but a stub that does nothing is useless.

### Step 4: Commit (if you made changes)

If you added marker comments, implemented features, or fixed bugs:
```
git add -A
git commit -m "spec({feature_id}): <short description>"
```

If the paragraphs are not applicable or you have nothing to add, it is OK to
exit with zero commits. This is NOT a failure.

### Output format

Output a brief summary for EACH paragraph:
- **TAG**: the spec tag
- **ACTION**: `ANNOTATED` / `IMPLEMENTED` / `FIXED` / `NOT_APPLICABLE`
- **FILES**: list of files modified (or "none")
- **DESCRIPTION**: 1-2 sentences explaining what you did

## CRITICAL RULES — VIOLATION = IMMEDIATE FAILURE

- DO NOT `cd` TO ANY OTHER DIRECTORY. You are in a git worktree. Stay in
  the current working directory for ALL commands (read, edit, grep, git).
  Running `cd /some/other/path` will cause your commits to go to the wrong
  branch and your work will be LOST. Use relative paths like `layout/src/...`.
- DO NOT RUN `cargo build`, `cargo test`, `cargo check`, `rustc`, `clang`,
  OR ANY COMPILATION/BUILD COMMAND. Due to CPU limitations, compilation is
  not possible in this environment. It does not matter if your change is not
  100% correct — we will fix compilation errors later.
- DO NOT USE `rust-analyzer`, LSP tools, OR ANY MCP TOOLS.
- Make ONLY the changes needed for the spec paragraphs in this prompt.
- Zero commits is OK if the paragraphs are not applicable or already covered.
- If unsure whether a change is correct, make your best effort.
"#,
        feature_id = feature_id,
        grep_cmd = grep_cmd,
        marker_instructions = marker_instructions,
    )
}

/// Extract the spec context from a review prompt .md file.
///
/// The .md files are written with review framing by `generate_paragraph_prompt()`.
/// We extract only the structured sections (Feature Context, Source Files,
/// Spec Paragraph) and discard review-specific framing (Instructions,
/// Response Format, header).
///
/// This is the fallback path — it parses the markdown sections by header.
/// If the prompt format changes, only this function needs updating.
fn extract_spec_context_from_md(prompt_content: &str) -> String {
    // Sections we want to keep (in order they appear)
    const KEEP_SECTIONS: &[&str] = &[
        "## Feature Context",
        "## Source Files to Read",
        "## Relevant Types",
        "## Spec Paragraph",  // matches "## Spec Paragraph to Verify" and "## Spec Paragraph N"
    ];

    let mut result = String::new();
    let mut keeping = false;

    for line in prompt_content.lines() {
        if line.starts_with("## ") {
            keeping = KEEP_SECTIONS.iter().any(|s| line.starts_with(s));
        }
        if keeping {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

fn build_full_prompt(prompt_path: &Path, working_dir: &Path) -> Result<String, String> {
    let paragraph_content = fs::read_to_string(prompt_path)
        .map_err(|e| format!("Failed to read prompt {}: {}", prompt_path.display(), e))?;

    // Extract feature_id and hash(es) from filename:
    //   "box-model_a3f2c1.md" → feature_id="box-model", hash_part="a3f2c1"
    //   "box-model_a3f2c1+b4e7d2.md" → feature_id="box-model", hash_part="a3f2c1+b4e7d2"
    let stem = prompt_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let (feature_id, hash_part) = match stem.rfind('_') {
        Some(i) => (&stem[..i], &stem[i + 1..]),
        None => (stem, "000000"),
    };
    // hash_part may be "a3f2c1" or "a3f2c1+b4e7d2" (grouped)
    // Tag format: "feature:hash" — colon separator so humans can grep by feature
    let spec_tags: Vec<String> = hash_part.split('+')
        .map(|h| format!("{}:{}", feature_id, h))
        .collect();

    // Extract just the spec context (feature, sources, paragraph) from the
    // review-framed .md file, discarding review instructions/response format.
    let spec_context = extract_spec_context_from_md(&paragraph_content);

    let mut full_prompt =
        String::with_capacity(CODEBASE_CONTEXT.len() + spec_context.len() + 4096);

    full_prompt.push_str(CODEBASE_CONTEXT);
    full_prompt.push('\n');
    full_prompt.push_str(&format!(
        "## Working Directory\n\nYour current working directory is: `{}`\n\
         You are in a git worktree. ALL file paths are relative to this directory.\n\
         Do NOT `cd` anywhere else — your commits will be lost if you do.\n\n",
        working_dir.display()
    ));
    full_prompt.push_str(&spec_context);
    full_prompt.push('\n');
    full_prompt.push_str(&build_agent_instructions(feature_id, &spec_tags));

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
    let full_prompt = match build_full_prompt(prompt_path, &slot.path) {
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
            // Block compilation tools
            "--disallowedTools",
            "Bash(cargo *)",
            "--disallowedTools",
            "Bash(rustc *)",
            "--disallowedTools",
            "Bash(clang *)",
            "--disallowedTools",
            "Bash(gcc *)",
            "--disallowedTools",
            "Bash(make *)",
            "--disallowedTools",
            "Bash(cmake *)",
            // Block MCP tools that leak from user config
            "--disallowedTools",
            "mcp__*",
            // Block rust-analyzer LSP tool
            "--disallowedTools",
            "rust-analyzer-lsp",
        ])
        .env_remove("CLAUDECODE")
        // Pin git operations to the worktree so agents can't accidentally
        // commit to the main repo branch by cd-ing to the main repo path.
        .env("GIT_DIR", slot.path.join(".git"))
        .env("GIT_WORK_TREE", &slot.path)
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

    // Zero commits is OK — paragraph may be not applicable or already covered.
    // Only mark as failed if the agent itself errored (non-zero exit).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_merge_groups_raw_json() {
        let json = r###"[
  {
    "group_id": 1,
    "action": "APPLY",
    "patches": ["floats_004.md.done.001.patch"],
    "preferred": null,
    "agent_context": "## Intent\nFix float margin-box overlap per CSS 2.2 §9.5.\n\n## Known Issues\n- is_block_level_replaced() only checks Image."
  },
  {
    "group_id": 2,
    "action": "PICK_ONE",
    "patches": ["line-breaking_008.md.done.001.patch", "line-breaking_015.md.done.001.patch"],
    "preferred": "line-breaking_008.md.done.001.patch",
    "agent_context": "## Intent\nAdd word-break property.\n\n## Why _008\nCorrectly suppresses hyphenation."
  },
  {
    "group_id": 3,
    "action": "SKIP",
    "patches": ["display-property_001.md.done.001.patch"],
    "preferred": null,
    "agent_context": "SKIP — regression."
  }
]"###;

        let extracted = extract_json_from_plan(json).unwrap();
        let groups: Vec<MergeGroup> = serde_json::from_str(&extracted).unwrap();
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].action, "APPLY");
        assert_eq!(groups[0].patches, vec!["floats_004.md.done.001.patch"]);
        assert!(groups[0].agent_context.as_ref().unwrap().contains("margin-box"));
        assert_eq!(groups[1].action, "PICK_ONE");
        assert_eq!(groups[1].preferred.as_deref(), Some("line-breaking_008.md.done.001.patch"));
        assert_eq!(groups[2].action, "SKIP");
    }

    #[test]
    fn test_parse_merge_groups_markdown_fenced() {
        let md = r###"Here is my analysis of the patches:

Based on the review, I recommend the following merge groups:

```json
[
  {
    "group_id": 1,
    "action": "MERGE",
    "patches": ["box-model_024.md.done.001.patch", "box-model_039.md.done.001.patch"],
    "preferred": null,
    "agent_context": "## Intent\nBoth add is_first_fragment/is_last_fragment to InlineBorderInfo.\n\n## Merge Strategy\nTake _039's RTL awareness + _024's split detection logic.\n\n## Files\n- cache.rs: merge InlineBorderInfo field additions\n- display_list.rs: combine border suppression logic"
  },
  {
    "group_id": 2,
    "action": "APPLY",
    "patches": ["intrinsic-sizing_019.md.done.001.patch"],
    "preferred": null,
    "agent_context": "Add fit-content() CSS keyword. Formula: min(max-content, max(min-content, arg))."
  }
]
```

Note: I've grouped the box-model patches because they conflict on InlineBorderInfo.
"###;

        let extracted = extract_json_from_plan(md).unwrap();
        let groups: Vec<MergeGroup> = serde_json::from_str(&extracted).unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].action, "MERGE");
        assert_eq!(groups[0].patches.len(), 2);
        assert!(groups[0].agent_context.as_ref().unwrap().contains("RTL awareness"));
        assert_eq!(groups[1].action, "APPLY");
    }

    #[test]
    fn test_parse_merge_groups_with_legacy_notes() {
        let json = r###"[
  {
    "group_id": 1,
    "action": "APPLY",
    "patches": ["test.patch"],
    "preferred": null,
    "notes": "Legacy notes field"
  }
]"###;

        let extracted = extract_json_from_plan(json).unwrap();
        let groups: Vec<MergeGroup> = serde_json::from_str(&extracted).unwrap();
        assert_eq!(groups.len(), 1);
        assert!(groups[0].agent_context.is_none());
        assert_eq!(groups[0].notes.as_deref(), Some("Legacy notes field"));
    }

    #[test]
    fn test_parse_merge_groups_bare_code_fence() {
        // Gemini sometimes uses ``` without json language tag
        let md = r###"
```
[{"group_id":1,"action":"SKIP","patches":["bad.patch"],"preferred":null,"agent_context":"Regression."}]
```
"###;
        let extracted = extract_json_from_plan(md).unwrap();
        let groups: Vec<MergeGroup> = serde_json::from_str(&extracted).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].action, "SKIP");
    }

    #[test]
    fn test_extract_json_no_json_found() {
        let bad = "This is just text with no JSON at all.";
        assert!(extract_json_from_plan(bad).is_err());
    }

    #[test]
    fn test_categorize_diff_annot_only() {
        let patch = r###"From abc123 Mon Sep 17 00:00:00 2001
Subject: [PATCH] annotate something

---
 layout/src/solver3/fc.rs | 2 ++
 1 file changed, 2 insertions(+)

diff --git a/layout/src/solver3/fc.rs b/layout/src/solver3/fc.rs
--- a/layout/src/solver3/fc.rs
+++ b/layout/src/solver3/fc.rs
@@ -100,6 +100,8 @@ fn foo() {
     let x = 1;
+    // +spec:block-formatting-context-p001: BFC establishment
+    // This is already correctly implemented
     let y = 2;
--
2.43.0
"###;
        let (total_adds, total_dels, real_adds, real_dels) = categorize_diff_text(patch);
        assert_eq!(real_adds, 0, "annotation-only patch should have 0 real adds");
        assert_eq!(real_dels, 0, "annotation-only patch should have 0 real dels");
        assert!(total_adds > 0, "should have comment additions");
    }

    #[test]
    fn test_categorize_diff_code_change() {
        let patch = r###"diff --git a/layout/src/solver3/fc.rs b/layout/src/solver3/fc.rs
--- a/layout/src/solver3/fc.rs
+++ b/layout/src/solver3/fc.rs
@@ -100,6 +100,8 @@ fn foo() {
-    let x = calculate_old();
+    let x = calculate_new();
+    // +spec:bfc-p005: updated calculation
--
2.43.0
"###;
        let (total_adds, total_dels, real_adds, real_dels) = categorize_diff_text(patch);
        assert_eq!(real_adds, 1, "should detect code addition");
        assert_eq!(real_dels, 1, "should detect code deletion");
    }

    #[test]
    fn test_extract_spec_paragraph() {
        let prompt = r###"# Spec Paragraph Review

## Feature Context

**Block Formatting Context**

## Spec Paragraph to Verify

**Source**: CSS 2.2 Section 9.4
**Section ID**: `block-formatting`

> Floats, absolutely positioned elements, block containers that are not block boxes...

**Full spec**: https://www.w3.org/TR/CSS22/visuren.html

## Instructions

1. Read the source files
"###;
        let para = extract_spec_paragraph(prompt).unwrap();
        assert!(para.contains("CSS 2.2 Section 9.4"));
        assert!(para.contains("Floats, absolutely positioned"));
        assert!(!para.contains("Instructions"));
    }
}
