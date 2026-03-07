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

/// Structured spec paragraph data — the raw "chunks" that callers combine
/// with whatever framing they need (review for Gemini, implementation for
/// claude-exec agents).
pub struct SpecParagraphContext {
    /// e.g. "Block Formatting Context"
    pub feature_name: String,
    /// e.g. "block-formatting-context"
    pub feature_id: String,
    /// e.g. "BFC establishment and block layout"
    pub feature_description: String,
    /// e.g. 5 of 50
    pub para_index: usize,
    /// e.g. 50
    pub total_paragraphs: usize,
    /// Source file paths with line counts, e.g. [("layout/src/solver3/fc.rs", 7261)]
    pub source_files: Vec<(String, usize)>,
    /// The spec paragraph text
    pub paragraph_text: String,
    /// e.g. "Appendix A: Glossary"
    pub paragraph_section: String,
    /// e.g. "css-display-3.html"
    pub paragraph_source_file: String,
    /// e.g. Some("glossary")
    pub paragraph_section_id: Option<String>,
    /// e.g. ["block formatting context", "margin collapsing"]
    pub matched_keywords: Vec<String>,
    /// e.g. ["https://www.w3.org/TR/CSS22/visuren.html#block-formatting"]
    pub spec_urls: Vec<String>,
}

/// Extract the structured chunks from a SkillNode + paragraph.
/// This is the single source of truth — no string parsing needed.
pub fn extract_paragraph_context(
    node: &SkillNode,
    paragraph: &ExtractedParagraph,
    para_index: usize,
    total_paragraphs: usize,
    workspace_root: &Path,
) -> SpecParagraphContext {
    let source_files = node.source_files.iter()
        .filter(|f| {
            (node.needs_text_engine || !f.contains("text3"))
            && (node.needs_css_source || !f.starts_with("css/"))
            && (node.needs_core_source || !f.starts_with("core/"))
        })
        .map(|file_path| {
            let full_path = workspace_root.join(file_path);
            let line_count = std::fs::read_to_string(&full_path)
                .map(|c| c.lines().count())
                .unwrap_or(0);
            (file_path.clone(), line_count)
        })
        .collect();

    SpecParagraphContext {
        feature_name: node.name.clone(),
        feature_id: node.id.clone(),
        feature_description: node.description.clone(),
        para_index,
        total_paragraphs,
        source_files,
        paragraph_text: paragraph.text.clone(),
        paragraph_section: paragraph.section.clone(),
        paragraph_source_file: paragraph.source_file.clone(),
        paragraph_section_id: paragraph.section_id.clone(),
        matched_keywords: paragraph.matched_keywords.clone(),
        spec_urls: node.spec_urls.clone(),
    }
}

impl SpecParagraphContext {
    /// Format just the spec context chunks (feature, sources, paragraph).
    /// No framing — callers wrap this with their own instructions.
    pub fn format_spec_context(&self) -> String {
        let mut out = String::new();

        out.push_str("## Feature Context\n\n");
        out.push_str(&format!(
            "**{}** (id: `{}`): {}\n\n",
            self.feature_name, self.feature_id, self.feature_description
        ));

        out.push_str("## Source Files to Read\n\n");
        for (path, lines) in &self.source_files {
            out.push_str(&format!("- `{}` ({} lines)\n", path, lines));
        }
        out.push('\n');

        out.push_str("## Spec Paragraph to Verify\n\n");
        out.push_str(&format!(
            "**Source**: {} (from `{}`)\n",
            self.paragraph_section, self.paragraph_source_file
        ));
        if let Some(ref id) = self.paragraph_section_id {
            out.push_str(&format!("**Section ID**: `{}`\n", id));
        }
        out.push_str(&format!(
            "**Matched keywords**: {}\n\n",
            self.matched_keywords.join(", ")
        ));
        out.push_str(&format!("> {}\n\n", self.paragraph_text));

        if !self.spec_urls.is_empty() {
            out.push_str("**Full spec**: ");
            out.push_str(&self.spec_urls.join(", "));
            out.push_str("\n\n");
        }

        out
    }

    /// Format as a Gemini review prompt (the old generate_paragraph_prompt output).
    pub fn format_review_prompt(&self) -> String {
        let mut prompt = String::new();

        prompt.push_str(&format!(
            "# Spec Paragraph Review: {} — paragraph {}/{}\n\n",
            self.feature_name,
            self.para_index + 1,
            self.total_paragraphs,
        ));
        prompt.push_str(
            "You are reviewing a CSS layout engine against ONE specific W3C spec paragraph.\n\
             Read the source files listed below, then check whether the code correctly\n\
             implements what this paragraph requires.\n\n",
        );

        prompt.push_str(&self.format_spec_context());

        prompt.push_str("## Instructions\n\n");
        prompt.push_str("1. Read the source files above\n");
        prompt.push_str("2. Find the code that implements (or should implement) what this paragraph describes\n");
        prompt.push_str("3. Check:\n");
        prompt.push_str("   - Is the requirement from this paragraph implemented at all?\n");
        prompt.push_str("   - If yes, does the implementation match the spec exactly?\n");
        prompt.push_str("   - Are edge cases from the paragraph handled?\n");
        prompt.push_str("   - Are there TODO/FIXME/HACK comments near the relevant code?\n\n");

        prompt.push_str("## Response Format\n\n");
        prompt.push_str("**IMPLEMENTED**: yes | partially | no\n\n");
        prompt.push_str("**LOCATION**: file:line where this is (or should be) implemented\n\n");
        prompt.push_str("**VERDICT**: `PASS` | `MINOR_ISSUES` | `NEEDS_CHANGES` | `NOT_IMPLEMENTED`\n\n");
        prompt.push_str("**DETAILS**: Explain what the code does vs what the paragraph requires.\n");
        prompt.push_str("Quote specific lines. If there are issues, describe the fix.\n\n");

        prompt
    }
}

/// Generate a self-contained agent prompt for a single spec paragraph.
///
/// This is the review-framed version used for Gemini and saved to .md files.
/// The executor's `build_full_prompt()` uses the structured data directly
/// via `format_spec_context()` instead of parsing this output.
pub fn generate_paragraph_prompt(
    node: &SkillNode,
    paragraph: &ExtractedParagraph,
    para_index: usize,
    total_paragraphs: usize,
    workspace_root: &Path,
) -> String {
    let ctx = extract_paragraph_context(node, paragraph, para_index, total_paragraphs, workspace_root);
    ctx.format_review_prompt()
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

/// Extract type/function signature hints from css/ and core/ source files.
///
/// Scans for `pub enum`, `pub struct`, and `pub fn` lines to give agents
/// a quick index of available types without reading full files.
fn extract_type_hints(source_files: &[(String, usize)], workspace_root: &Path) -> String {
    let mut hints = String::new();
    for (path, _) in source_files {
        if path.starts_with("css/") || path.starts_with("core/") {
            let content = std::fs::read_to_string(workspace_root.join(path))
                .unwrap_or_default();
            for line in content.lines() {
                let trimmed = line.trim();
                // Skip macro template lines (contain $)
                if trimmed.contains('$') {
                    continue;
                }
                // Skip parse error types — they're noise for agents
                if trimmed.contains("ParseError") {
                    continue;
                }
                if trimmed.starts_with("pub enum ")
                    || trimmed.starts_with("pub struct ")
                    || (trimmed.starts_with("pub fn ") && path.contains("getters"))
                {
                    hints.push_str(&format!("- `{}`: `{}`\n", path, trimmed));
                }
            }
        }
    }
    hints
}

/// Structured context for a group of spec paragraphs (typically 2).
pub struct GroupedSpecContext {
    pub feature_name: String,
    pub feature_id: String,
    pub feature_description: String,
    pub group_index: usize,
    pub total_groups: usize,
    pub source_files: Vec<(String, usize)>,
    pub paragraphs: Vec<GroupedParagraph>,
    pub spec_urls: Vec<String>,
    pub type_hints: String,
}

pub struct GroupedParagraph {
    pub text: String,
    pub section: String,
    pub source_file: String,
    pub section_id: Option<String>,
    pub matched_keywords: Vec<String>,
    pub content_hash: String,
}

/// Generate a grouped review prompt for multiple spec paragraphs.
///
/// Groups consecutive paragraphs (typically 2) into a single prompt to
/// halve the number of agent invocations while keeping prompts focused.
pub fn generate_grouped_prompt(
    node: &SkillNode,
    paragraphs: &[&ExtractedParagraph],
    group_index: usize,
    total_groups: usize,
    workspace_root: &Path,
) -> String {
    let source_files: Vec<(String, usize)> = node.source_files.iter()
        .filter(|f| {
            (node.needs_text_engine || !f.contains("text3"))
            && (node.needs_css_source || !f.starts_with("css/"))
            && (node.needs_core_source || !f.starts_with("core/"))
        })
        .map(|file_path| {
            let full_path = workspace_root.join(file_path);
            let line_count = std::fs::read_to_string(&full_path)
                .map(|c| c.lines().count())
                .unwrap_or(0);
            (file_path.clone(), line_count)
        })
        .collect();

    let type_hints = extract_type_hints(&source_files, workspace_root);

    let para_count = paragraphs.len();
    let mut prompt = String::new();

    prompt.push_str(&format!(
        "# Spec Paragraph Review: {} — group {}/{}\n\n",
        node.name,
        group_index + 1,
        total_groups,
    ));
    prompt.push_str(&format!(
        "You are reviewing a CSS layout engine against {} W3C spec paragraph{}.\n\
         Read the source files listed below, then check whether the code correctly\n\
         implements what these paragraphs require.\n\n",
        para_count,
        if para_count > 1 { "s" } else { "" },
    ));

    prompt.push_str("## Feature Context\n\n");
    prompt.push_str(&format!(
        "**{}** (id: `{}`): {}\n\n",
        node.name, node.id, node.description
    ));

    prompt.push_str("## Source Files to Read\n\n");
    for (path, lines) in &source_files {
        prompt.push_str(&format!("- `{}` ({} lines)\n", path, lines));
    }
    prompt.push('\n');

    if !type_hints.is_empty() {
        prompt.push_str("## Relevant Types\n\n");
        prompt.push_str(&type_hints);
        prompt.push('\n');
    }

    for (i, para) in paragraphs.iter().enumerate() {
        let hash = super::extractor::paragraph_content_hash(para);
        let spec_tag = format!("{}:{}", node.id, hash);

        prompt.push_str(&format!(
            "## Spec Paragraph {} (tag: `{}`)\n\n",
            i + 1,
            spec_tag,
        ));
        prompt.push_str(&format!(
            "**Source**: {} (from `{}`)\n",
            para.section, para.source_file
        ));
        if let Some(ref id) = para.section_id {
            prompt.push_str(&format!("**Section ID**: `{}`\n", id));
        }
        prompt.push_str(&format!(
            "**Matched keywords**: {}\n\n",
            para.matched_keywords.join(", ")
        ));
        prompt.push_str(&format!("> {}\n\n", para.text));
    }

    if !node.spec_urls.is_empty() {
        prompt.push_str("**Full spec**: ");
        prompt.push_str(&node.spec_urls.join(", "));
        prompt.push_str("\n\n");
    }

    prompt.push_str("## Instructions\n\n");
    prompt.push_str("1. Read the source files above\n");
    prompt.push_str("2. For EACH spec paragraph, find the code that implements (or should implement) what it describes\n");
    prompt.push_str("3. Check:\n");
    prompt.push_str("   - Is the requirement from each paragraph implemented at all?\n");
    prompt.push_str("   - If yes, does the implementation match the spec exactly?\n");
    prompt.push_str("   - Are edge cases from the paragraphs handled?\n");
    prompt.push_str("   - Are there TODO/FIXME/HACK comments near the relevant code?\n\n");

    prompt.push_str("## Response Format\n\n");
    prompt.push_str("For EACH paragraph, respond with:\n\n");
    prompt.push_str("**PARAGRAPH N** (tag: `<tag>`):\n\n");
    prompt.push_str("**IMPLEMENTED**: yes | partially | no\n\n");
    prompt.push_str("**LOCATION**: file:line where this is (or should be) implemented\n\n");
    prompt.push_str("**VERDICT**: `PASS` | `MINOR_ISSUES` | `NEEDS_CHANGES` | `NOT_IMPLEMENTED`\n\n");
    prompt.push_str("**DETAILS**: Explain what the code does vs what the paragraph requires.\n");
    prompt.push_str("Quote specific lines. If there are issues, describe the fix.\n\n");

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
