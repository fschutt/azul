//! W3C Spec Reviewer
//!
//! Generates prompts for Gemini to review code against W3C specifications,
//! handles the two-stage review process (architecture → implementation),
//! and saves results for later holistic analysis.

use std::path::{Path, PathBuf};
use super::skill_tree::{SkillNode, SkillTree, VerificationStatus};
use super::extractor::ExtractedParagraph;

/// Review stage
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReviewStage {
    Architecture,
    Implementation,
}

/// Result of a review
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewResult {
    pub node_id: String,
    pub stage: String,
    pub timestamp: String,
    pub prompt: String,
    pub response: String,
    pub needs_changes: bool,
    pub issues: Vec<String>,
}

/// Generate a review prompt for a skill node
pub fn generate_review_prompt(
    node: &SkillNode,
    stage: ReviewStage,
    spec_paragraphs: &[ExtractedParagraph],
    source_code: &[(String, String)], // (filename, content) pairs
) -> String {
    let mut prompt = String::new();
    
    // Header
    prompt.push_str(&format!(
        "# CSS Layout Verification: {}\n\n",
        node.name
    ));
    
    // Stage-specific instructions
    match stage {
        ReviewStage::Architecture => {
            prompt.push_str("## Review Stage: ARCHITECTURE\n\n");
            prompt.push_str("Please review the **overall architecture** of this implementation.\n\n");
            prompt.push_str("Focus on:\n");
            prompt.push_str("1. Does the code structure match the conceptual model in the W3C spec?\n");
            prompt.push_str("2. Are the right abstractions being used?\n");
            prompt.push_str("3. Is the algorithm structured correctly at a high level?\n");
            prompt.push_str("4. Are edge cases mentioned in the spec accounted for structurally?\n\n");
            prompt.push_str("Do NOT focus on:\n");
            prompt.push_str("- Exact numerical calculations\n");
            prompt.push_str("- Minor implementation details\n");
            prompt.push_str("- Code style\n\n");
        }
        ReviewStage::Implementation => {
            prompt.push_str("## Review Stage: IMPLEMENTATION\n\n");
            prompt.push_str("Please review the **detailed implementation** of this feature.\n\n");
            prompt.push_str("Focus on:\n");
            prompt.push_str("1. Do calculations exactly match the formulas in the spec?\n");
            prompt.push_str("2. Are all conditions and edge cases handled correctly?\n");
            prompt.push_str("3. Does the code handle the 'auto' keyword correctly?\n");
            prompt.push_str("4. Are there any off-by-one errors or missing steps?\n\n");
            prompt.push_str("Provide specific line-by-line analysis where issues are found.\n\n");
        }
    }
    
    // Feature description
    prompt.push_str("## Feature Description\n\n");
    prompt.push_str(&format!("**{}**: {}\n\n", node.name, node.description));
    
    // W3C Spec References
    prompt.push_str("## W3C Specification References\n\n");
    for url in &node.spec_urls {
        prompt.push_str(&format!("- {}\n", url));
    }
    prompt.push_str("\n");
    
    // Extracted spec paragraphs
    if !spec_paragraphs.is_empty() {
        prompt.push_str("## Relevant Specification Text\n\n");
        for para in spec_paragraphs.iter().take(30) {
            prompt.push_str(&format!(
                "### {} (from {})\n\n",
                para.section,
                para.source_file
            ));
            prompt.push_str(&format!("> {}\n\n", para.text));
        }
    }
    
    // Source code to review
    prompt.push_str("## Source Code to Review\n\n");
    for (filename, content) in source_code {
        prompt.push_str(&format!("### {}\n\n", filename));
        prompt.push_str("```rust\n");
        prompt.push_str(content);
        if !content.ends_with('\n') {
            prompt.push_str("\n");
        }
        prompt.push_str("```\n\n");
    }
    
    // Response format
    prompt.push_str("## Expected Response Format\n\n");
    prompt.push_str("Please respond with:\n\n");
    prompt.push_str("1. **SUMMARY**: One paragraph summarizing your findings\n\n");
    prompt.push_str("2. **VERDICT**: One of:\n");
    prompt.push_str("   - `PASS` - Implementation matches spec\n");
    prompt.push_str("   - `MINOR_ISSUES` - Small discrepancies, not blocking\n");
    prompt.push_str("   - `NEEDS_CHANGES` - Significant issues found\n");
    prompt.push_str("   - `MAJOR_REWRITE` - Fundamental architecture problems\n\n");
    prompt.push_str("3. **ISSUES** (if any): List each issue with:\n");
    prompt.push_str("   - File and line number\n");
    prompt.push_str("   - What the spec says\n");
    prompt.push_str("   - What the code does\n");
    prompt.push_str("   - Suggested fix\n\n");
    prompt.push_str("4. **HACKS_AND_TODOS**: Review any of these patterns in the code:\n");
    prompt.push_str("   - Comments containing `TODO`, `FIXME`, `HACK`, `XXX`, `WORKAROUND`\n");
    prompt.push_str("   - Uses of `::default()` that might be masking missing implementations\n");
    prompt.push_str("   - Magic numbers or unexplained constants\n");
    prompt.push_str("   - Conditional logic that seems like a workaround\n");
    prompt.push_str("   For each, explain if it's a potential source of bugs.\n\n");
    prompt.push_str("5. **MISSING_CSS_PROPERTIES**: Are there CSS properties mentioned in the spec\n");
    prompt.push_str("   sections above that are NOT implemented in this code? List them.\n\n");
    prompt.push_str("6. **SPEC_COMPLIANCE_SCORE**: 0-100\n\n");
    
    prompt
}

/// Read source files for a skill node
pub fn read_source_files(
    node: &SkillNode,
    workspace_root: &Path,
) -> Vec<(String, String)> {
    let mut sources = Vec::new();
    
    for file_path in &node.source_files {
        // Skip text3 files if not needed
        if !node.needs_text_engine && file_path.contains("text3") {
            continue;
        }
        
        let full_path = workspace_root.join(file_path);
        match std::fs::read_to_string(&full_path) {
            Ok(content) => {
                sources.push((file_path.clone(), content));
            }
            Err(e) => {
                eprintln!("Warning: Could not read {}: {}", file_path, e);
            }
        }
    }
    
    sources
}

/// Save a review result
pub fn save_review_result(
    result: &ReviewResult,
    output_dir: &Path,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(output_dir)?;
    
    let filename = format!(
        "{}_{}.json",
        result.node_id,
        result.stage.to_lowercase()
    );
    let output_path = output_dir.join(&filename);
    
    let json = serde_json::to_string_pretty(result)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    
    std::fs::write(&output_path, &json)?;
    
    // Also save markdown version for easy reading
    let md_filename = format!(
        "{}_{}.md",
        result.node_id,
        result.stage.to_lowercase()
    );
    let md_path = output_dir.join(&md_filename);
    
    let markdown = format!(
        "# Review: {} ({})\n\n**Date**: {}\n\n## Prompt\n\n{}\n\n## Response\n\n{}\n",
        result.node_id,
        result.stage,
        result.timestamp,
        result.prompt,
        result.response
    );
    
    std::fs::write(&md_path, markdown)?;
    
    Ok(output_path)
}

/// Load all review results for a node
pub fn load_review_results(
    node_id: &str,
    results_dir: &Path,
) -> Vec<ReviewResult> {
    let mut results = Vec::new();
    
    for stage in ["architecture", "implementation"] {
        let filename = format!("{}_{}.json", node_id, stage);
        let path = results_dir.join(&filename);
        
        if path.exists() {
            if let Ok(json) = std::fs::read_to_string(&path) {
                if let Ok(result) = serde_json::from_str(&json) {
                    results.push(result);
                }
            }
        }
    }
    
    results
}

/// Update skill tree node status based on review result
pub fn update_node_status(
    tree: &mut SkillTree,
    result: &ReviewResult,
) {
    if let Some(node) = tree.nodes.get_mut(&result.node_id) {
        // When we receive a response from Gemini, update to PromptSent
        node.status = VerificationStatus::PromptSent {
            needs_changes: result.needs_changes,
        };
    }
}

/// Generate a holistic analysis prompt from all review results
pub fn generate_holistic_prompt(
    tree: &SkillTree,
    results_dir: &Path,
) -> String {
    let mut prompt = String::new();
    
    prompt.push_str("# Holistic CSS Layout Implementation Analysis\n\n");
    prompt.push_str("You are reviewing all the verification results for a CSS layout engine.\n");
    prompt.push_str("Please provide a comprehensive analysis identifying:\n\n");
    prompt.push_str("1. **Systemic Issues**: Problems that appear across multiple features\n");
    prompt.push_str("2. **Priority Order**: Which issues should be fixed first\n");
    prompt.push_str("3. **Architectural Recommendations**: High-level improvements\n");
    prompt.push_str("4. **Test Coverage Gaps**: What tests should be added\n\n");
    
    prompt.push_str("## Skill Tree Status\n\n");
    prompt.push_str("```\n");
    for node in tree.get_ordered_nodes() {
        let status = match &node.status {
            VerificationStatus::NotStarted => "[ ] Not started",
            VerificationStatus::PromptBuilt => "[P] Prompt built",
            VerificationStatus::PromptSent { needs_changes: false } => "[S] Sent (OK)",
            VerificationStatus::PromptSent { needs_changes: true } => "[!] Needs changes",
            VerificationStatus::Implemented => "[I] Implemented",
            VerificationStatus::Verified => "[✓] Verified",
        };
        prompt.push_str(&format!("{}: {} - {}\n", status, node.name, node.description));
    }
    prompt.push_str("```\n\n");
    
    prompt.push_str("## Individual Review Summaries\n\n");
    
    for node in tree.get_ordered_nodes() {
        let results = load_review_results(&node.id, results_dir);
        if !results.is_empty() {
            prompt.push_str(&format!("### {}\n\n", node.name));
            
            for result in results {
                prompt.push_str(&format!("**{} Review**:\n", result.stage));
                
                // Extract just the summary from the response (first paragraph)
                let summary = result.response
                    .lines()
                    .skip_while(|l| !l.contains("SUMMARY"))
                    .skip(1)
                    .take_while(|l| !l.starts_with('#') && !l.starts_with('*'))
                    .collect::<Vec<_>>()
                    .join(" ");
                
                if !summary.is_empty() {
                    prompt.push_str(&format!("> {}\n\n", summary.trim()));
                }
                
                if !result.issues.is_empty() {
                    prompt.push_str("Issues found:\n");
                    for issue in &result.issues {
                        prompt.push_str(&format!("- {}\n", issue));
                    }
                    prompt.push_str("\n");
                }
            }
        }
    }
    
    prompt
}

/// Parse verdict from Gemini response
pub fn parse_verdict(response: &str) -> (bool, Vec<String>) {
    let needs_changes = response.contains("NEEDS_CHANGES") 
        || response.contains("MAJOR_REWRITE");
    
    // Extract issues
    let mut issues = Vec::new();
    let mut in_issues_section = false;
    
    for line in response.lines() {
        if line.contains("**ISSUES**") || line.contains("## Issues") {
            in_issues_section = true;
            continue;
        }
        if in_issues_section {
            if line.starts_with('#') || line.contains("**") {
                in_issues_section = false;
            } else if line.starts_with("- ") || line.starts_with("* ") {
                issues.push(line[2..].trim().to_string());
            }
        }
    }
    
    (needs_changes, issues)
}
