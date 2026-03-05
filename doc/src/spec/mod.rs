//! W3C Specification Verification System
//!
//! This module provides tools for systematically verifying the CSS layout
//! implementation against W3C specifications.
//!
//! ## Workflow
//!
//! 1. `spec download` - Download W3C specs locally
//! 2. `spec tree` - View the skill tree of CSS features
//! 3. `spec extract <feature>` - Extract relevant spec paragraphs
//! 4. `spec review <feature>` - Generate review prompt for Gemini
//! 5. `spec send <feature>` - Send to Gemini API
//! 6. `spec status` - View verification progress
//! 7. `spec holistic` - Generate holistic analysis from all results

use std::path::PathBuf;

pub mod skill_tree;
pub mod downloader;
pub mod executor;
pub mod extractor;
pub mod reviewer;
pub mod paragraphs;

pub use skill_tree::{SkillTree, SkillNode, VerificationStatus};
pub use downloader::{SpecRegistry, download_all_specs, download_specs_for_node};
pub use extractor::{extract_paragraphs, extract_for_skill_node, format_paragraphs_for_prompt};
pub use reviewer::{
    generate_review_prompt, generate_paragraph_prompt, read_source_files,
    save_review_result, load_review_results, update_node_status,
    generate_holistic_prompt, ReviewStage, ReviewResult, parse_verdict,
};
pub use paragraphs::{ParagraphRegistry, SpecParagraph, scan_source_for_annotations, scan_spec_tags};

/// Configuration for the spec verification system
pub struct SpecConfig {
    /// Directory where downloaded specs are stored
    pub spec_dir: PathBuf,
    /// Directory where skill tree state and results are stored
    pub skill_tree_dir: PathBuf,
    /// Gemini API key
    pub api_key: String,
}

impl SpecConfig {
    pub fn new(workspace_root: &std::path::Path, api_key: String) -> Self {
        Self {
            spec_dir: workspace_root.join("doc/w3c-specs"),
            skill_tree_dir: workspace_root.join("doc/skill-tree"),
            api_key,
        }
    }
    
    pub fn from_azul_root(azul_root: &std::path::Path) -> Self {
        let spec_dir = azul_root.join("doc").join("target").join("w3c_specs");
        let skill_tree_dir = azul_root.join("doc").join("target").join("skill_tree");
        
        let api_key = std::fs::read_to_string(azul_root.join("GEMINI_API_KEY.txt"))
            .unwrap_or_default()
            .trim()
            .to_string();
        
        Self {
            spec_dir,
            skill_tree_dir,
            api_key,
        }
    }
    
    pub fn skill_tree_path(&self) -> PathBuf {
        self.skill_tree_dir.join("tree.json")
    }
    
    pub fn results_dir(&self) -> PathBuf {
        self.skill_tree_dir.join("results")
    }
    
    /// Load or create skill tree
    pub fn load_skill_tree(&self) -> SkillTree {
        let path = self.skill_tree_path();
        if path.exists() {
            SkillTree::load(&path).unwrap_or_default()
        } else {
            SkillTree::default()
        }
    }
    
    /// Save skill tree
    pub fn save_skill_tree(&self, tree: &SkillTree) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.skill_tree_dir)?;
        tree.save(&self.skill_tree_path())
    }
}

/// Main entry point for spec commands
pub fn run_spec_command(args: &[String], workspace_root: &std::path::Path) -> Result<(), String> {
    let config = SpecConfig::from_azul_root(workspace_root);
    
    if args.is_empty() {
        print_spec_help();
        return Ok(());
    }
    
    match args[0].as_str() {
        "download" => cmd_download(&config),
        "tree" => cmd_tree(&config),
        "extract" => {
            if args.len() < 2 {
                return Err("Usage: spec extract <feature-id>".to_string());
            }
            cmd_extract(&config, &args[1], workspace_root)
        }
        "review" => {
            if args.len() < 2 {
                return Err("Usage: spec review <feature-id> [--stage=arch|impl]".to_string());
            }
            let stage = if args.iter().any(|a| a.contains("impl")) {
                ReviewStage::Implementation
            } else {
                ReviewStage::Architecture
            };
            cmd_review(&config, &args[1], stage, workspace_root)
        }
        "send" => {
            if args.len() < 2 {
                return Err("Usage: spec send <feature-id> [--stage=arch|impl]".to_string());
            }
            let stage = if args.iter().any(|a| a.contains("impl")) {
                ReviewStage::Implementation
            } else {
                ReviewStage::Architecture
            };
            cmd_send(&config, &args[1], stage, workspace_root)
        }
        "build-all" => cmd_build_all(&config, workspace_root),
        "claude-exec" => {
            let rest: Vec<String> = args[1..].to_vec();
            executor::run_executor(&config, workspace_root, &rest)
        }
        "status" => cmd_status(&config, workspace_root),
        "holistic" => cmd_holistic(&config),
        "next" => cmd_next(&config),
        "paragraphs" => cmd_paragraphs(),
        "annotations" => cmd_annotations(workspace_root),
        "review-md" => {
            if args.len() < 2 {
                return Err("Usage: spec review-md [--no-src] <commit-hash|dir>\n\
                    If argument is a directory, concatenates all .patch files in it.\n\
                    If argument is a commit hash, generates review from <hash>..HEAD.\n\
                    --no-src: omit source file appendix (only patches + review prompt).".to_string());
            }
            let no_src = args[1..].iter().any(|a| a == "--no-src");
            let target = args[1..].iter().find(|a| !a.starts_with("--"))
                .ok_or("Missing <commit-hash|dir> argument".to_string())?;
            executor::cmd_review_md(target, workspace_root, no_src)
        }
        "review-arch" => {
            let no_src = args[1..].iter().any(|a| a == "--no-src");
            let positional: Vec<&String> = args[1..].iter()
                .filter(|a| !a.starts_with("--"))
                .collect();
            if positional.len() < 2 {
                return Err("Usage: spec review-arch [--no-src] <patch-dir> <review-md-output>\n\
                    Generates an architecture review prompt for Gemini.\n\
                    <patch-dir>: directory containing .patch files\n\
                    <review-md-output>: path to the review-md output file".to_string());
            }
            executor::cmd_review_arch(positional[0], positional[1], workspace_root, no_src)
        }
        "agent-apply" => {
            let positional: Vec<&String> = args[1..].iter()
                .filter(|a| !a.starts_with("--"))
                .collect();
            if positional.len() < 2 {
                return Err("Usage: spec agent-apply <patch-dir> <arch-plan-json>\n\
                    Applies patches sequentially using AI agents, guided by the arch plan.\n\
                    <patch-dir>: directory containing .patch files\n\
                    <arch-plan-json>: JSON file with merge groups from review-arch".to_string());
            }
            executor::cmd_agent_apply(positional[0], positional[1], workspace_root)
        }
        _ => {
            print_spec_help();
            Err(format!("Unknown spec command: {}", args[0]))
        }
    }
}

fn print_spec_help() {
    println!("W3C Spec Verification System");
    println!("============================");
    println!();
    println!("Commands:");
    println!("  download            Download all W3C specs locally");
    println!("  tree                Display the CSS feature skill tree");
    println!("  extract <feature>   Extract relevant spec paragraphs for a feature");
    println!("  review <feature>    Generate a review prompt (saves to file)");
    println!("  send <feature>      Send review prompt to Gemini API");
    println!("  build-all           Build all prompts for all features");
    println!("  claude-exec         Run parallel Claude agents on prompts");
    println!("    --agents=N          Number of parallel agents (default: 12)");
    println!("    --timeout=S         Per-agent timeout in seconds (default: 480)");
    println!("    --retry-failed      Re-queue previously failed prompts");
    println!("    --status            Show done/taken/failed/pending counts");
    println!("    --collect           Cherry-pick agent commits to current branch");
    println!("    --cleanup           Remove all agent worktrees");
    println!("    --force-api         Allow running with ANTHROPIC_API_KEY set");
    println!("  status              Show verification status for all features");
    println!("  holistic            Generate holistic analysis from all results");
    println!("  next                Show the next feature to verify");
    println!("  paragraphs          List all known spec paragraph IDs for annotations");
    println!("  annotations         Scan source for +spec: annotations");
    println!("  review-md [--no-src] <hash|dir>  Generate Gemini review prompt from commits or patch dir");
    println!("  review-arch [--no-src] <dir> <review>  Generate architecture review prompt");
    println!("  agent-apply <dir> <arch-plan>   Apply patches sequentially using AI agents");
    println!();
    println!("Options:");
    println!("  --stage=arch        Architecture review (default)");
    println!("  --stage=impl        Implementation review");
    println!();
    println!("Annotation format in source code:");
    println!("  // +spec:css22-box-8.3.1-p1 - margin collapsing between siblings");
    println!();
    println!("Example workflow:");
    println!("  1. azul-doc spec download");
    println!("  2. azul-doc spec tree");
    println!("  3. azul-doc spec next");
    println!("  4. azul-doc spec review box-model");
    println!("  5. azul-doc spec send box-model");
    println!("  6. azul-doc spec status");
}

/// Build one prompt file per deduplicated spec paragraph, per feature.
///
/// Each file is self-contained: feature context + single paragraph + instructions.
/// Agents pick up individual files and read the source code themselves.
pub(crate) fn cmd_build_all(config: &SpecConfig, workspace_root: &std::path::Path) -> Result<(), String> {
    let mut tree = config.load_skill_tree();
    let node_ids: Vec<String> = tree.nodes.keys().cloned().collect();

    let prompts_dir = config.skill_tree_dir.join("prompts");
    std::fs::create_dir_all(&prompts_dir)
        .map_err(|e| format!("Failed to create directory: {}", e))?;

    // Clean old prompts
    if let Ok(entries) = std::fs::read_dir(&prompts_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().map(|e| e == "md").unwrap_or(false) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    println!("Building per-paragraph agent prompts for {} features...\n", node_ids.len());

    let mut total_files = 0usize;
    let mut total_tokens = 0usize;

    for node_id in &node_ids {
        let node = tree.nodes.get(node_id).unwrap().clone();

        // Extract + deduplicate spec paragraphs
        let paragraphs = match extract_for_skill_node(&node, &config.spec_dir) {
            Ok(p) => p,
            Err(e) => {
                println!("  [SKIP] {}: {}", node_id, e);
                continue;
            }
        };

        let para_count = paragraphs.len();
        let mut feature_tokens = 0usize;

        for (i, para) in paragraphs.iter().enumerate() {
            let prompt = reviewer::generate_paragraph_prompt(
                &node, para, i, para_count, workspace_root,
            );

            let tokens = prompt.len() / 4;
            feature_tokens += tokens;

            let filename = format!("{}_{:03}.md", node_id, i + 1);
            let prompt_path = prompts_dir.join(&filename);
            std::fs::write(&prompt_path, &prompt)
                .map_err(|e| format!("Failed to write {}: {}", prompt_path.display(), e))?;
        }

        total_files += para_count;
        total_tokens += feature_tokens;

        let text_indicator = if node.needs_text_engine { " +text3" } else { "" };
        println!("  [OK] {:30} {:>3} files  (~{} tokens avg){}",
            node_id,
            para_count,
            if para_count > 0 { feature_tokens / para_count } else { 0 },
            text_indicator
        );

        // Update status
        if let Some(n) = tree.nodes.get_mut(node_id) {
            if n.status == VerificationStatus::NotStarted {
                n.status = VerificationStatus::PromptBuilt;
            }
        }
    }

    config.save_skill_tree(&tree)
        .map_err(|e| format!("Failed to save tree: {}", e))?;

    println!("\n{}", "=".repeat(60));
    println!("Total: {} prompt files, ~{} tokens combined", total_files, total_tokens);
    println!("Prompts saved to: {}", prompts_dir.display());

    Ok(())
}

fn cmd_download(config: &SpecConfig) -> Result<(), String> {
    println!("Downloading W3C specifications...\n");
    download_all_specs(&config.spec_dir)?;
    println!("\nSpecs saved to: {}", config.spec_dir.display());
    Ok(())
}

fn cmd_tree(config: &SpecConfig) -> Result<(), String> {
    let tree = config.load_skill_tree();
    tree.print_tree();
    Ok(())
}

fn cmd_extract(config: &SpecConfig, feature_id: &str, _workspace_root: &std::path::Path) -> Result<(), String> {
    let tree = config.load_skill_tree();
    
    let node = tree.nodes.get(feature_id)
        .ok_or_else(|| format!("Unknown feature: {}. Use 'spec tree' to see available features.", feature_id))?;
    
    println!("Extracting spec paragraphs for: {}\n", node.name);
    println!("Keywords: {}\n", node.keywords.join(", "));
    
    let paragraphs = extract_for_skill_node(node, &config.spec_dir)?;
    
    println!("Found {} relevant paragraphs:\n", paragraphs.len());
    
    for (i, para) in paragraphs.iter().enumerate().take(20) {
        println!("{}. [{}] {} (from {})", 
            i + 1,
            para.matched_keywords.join(", "),
            para.section,
            para.source_file
        );
        println!("   {}\n", 
            para.text.chars().take(200).collect::<String>()
        );
    }
    
    if paragraphs.len() > 20 {
        println!("... and {} more paragraphs", paragraphs.len() - 20);
    }
    
    Ok(())
}

fn cmd_review(
    config: &SpecConfig, 
    feature_id: &str, 
    stage: ReviewStage,
    workspace_root: &std::path::Path
) -> Result<(), String> {
    let mut tree = config.load_skill_tree();
    
    let node = tree.nodes.get(feature_id)
        .ok_or_else(|| format!("Unknown feature: {}", feature_id))?
        .clone();
    
    let stage_name = match stage {
        ReviewStage::Architecture => "architecture",
        ReviewStage::Implementation => "implementation",
    };
    
    println!("Generating {} review prompt for: {}\n", stage_name, node.name);
    
    // Extract spec paragraphs
    let paragraphs = extract_for_skill_node(&node, &config.spec_dir)?;
    
    // Read source files
    let sources = read_source_files(&node, workspace_root);
    
    // Generate prompt
    let prompt = generate_review_prompt(&node, stage, &paragraphs, &sources);
    
    // Save prompt to file
    std::fs::create_dir_all(&config.skill_tree_dir)
        .map_err(|e| format!("Failed to create directory: {}", e))?;
    
    let prompt_path = config.skill_tree_dir.join(format!("{}_{}_prompt.md", feature_id, stage_name));
    std::fs::write(&prompt_path, &prompt)
        .map_err(|e| format!("Failed to write prompt: {}", e))?;
    
    // Update status to PromptBuilt
    if let Some(n) = tree.nodes.get_mut(feature_id) {
        if n.status == VerificationStatus::NotStarted {
            n.status = VerificationStatus::PromptBuilt;
            config.save_skill_tree(&tree)
                .map_err(|e| format!("Failed to save tree: {}", e))?;
            println!("[STATUS] Updated to: PromptBuilt");
        }
    }
    
    println!("Prompt saved to: {}", prompt_path.display());
    println!("\nPrompt length: {} chars ({} tokens approx)", 
        prompt.len(), 
        prompt.len() / 4
    );
    
    Ok(())
}

fn cmd_send(
    config: &SpecConfig,
    feature_id: &str,
    stage: ReviewStage,
    workspace_root: &std::path::Path,
) -> Result<(), String> {
    if config.api_key.is_empty() {
        return Err("No API key configured. Place API key in GEMINI_API_KEY.txt in azul root".to_string());
    }
    
    let mut tree = config.load_skill_tree();
    
    let node = tree.nodes.get(feature_id)
        .ok_or_else(|| format!("Unknown feature: {}", feature_id))?
        .clone();
    
    let stage_name = match stage {
        ReviewStage::Architecture => "architecture",
        ReviewStage::Implementation => "implementation",
    };
    
    println!("Sending {} review to Gemini for: {}\n", stage_name, node.name);
    
    // Extract spec paragraphs
    let paragraphs = extract_for_skill_node(&node, &config.spec_dir)?;
    
    // Read source files
    let sources = read_source_files(&node, workspace_root);
    
    // Generate prompt
    let prompt = generate_review_prompt(&node, stage, &paragraphs, &sources);
    
    println!("Sending {} chars to Gemini...", prompt.len());
    
    // Call Gemini API
    let response = call_gemini_api(&config.api_key, &prompt)?;
    
    // Parse verdict
    let (needs_changes, issues) = parse_verdict(&response);
    
    // Create result
    let result = ReviewResult {
        node_id: feature_id.to_string(),
        stage: stage_name.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        prompt,
        response: response.clone(),
        needs_changes,
        issues: issues.clone(),
    };
    
    // Save result
    let results_dir = config.results_dir();
    save_review_result(&result, &results_dir)
        .map_err(|e| format!("Failed to save result: {}", e))?;
    
    // Update tree status
    update_node_status(&mut tree, &result);
    config.save_skill_tree(&tree)
        .map_err(|e| format!("Failed to save tree: {}", e))?;
    
    // Print summary
    println!("\n{}", "=".repeat(60));
    println!("REVIEW COMPLETE: {}", node.name);
    println!("{}", "=".repeat(60));
    
    if needs_changes {
        println!("\n⚠️  NEEDS CHANGES\n");
        for issue in &issues {
            println!("  - {}", issue);
        }
    } else {
        println!("\n✅ PASS\n");
    }
    
    println!("\nFull response saved to: {}/{}_{}.md", 
        results_dir.display(), feature_id, stage_name);
    
    Ok(())
}

fn cmd_status(config: &SpecConfig, workspace_root: &std::path::Path) -> Result<(), String> {
    let tree = config.load_skill_tree();
    let prompts_dir = config.skill_tree_dir.join("prompts");

    // Scan source code for +spec: marker comments — this is the source of truth
    println!("Scanning source code for +spec: markers...\n");
    let found_tags = scan_spec_tags(workspace_root);

    // Count prompt files per feature and check which have markers
    let mut features: std::collections::BTreeMap<String, (usize, usize)> =
        std::collections::BTreeMap::new(); // feature_id → (total_paragraphs, marked_count)

    if prompts_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&prompts_dir) {
            let mut paths: Vec<_> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().map(|e| e == "md").unwrap_or(false)
                        && !p.to_string_lossy().contains(".md.")
                })
                .collect();
            paths.sort();

            for path in &paths {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let (feature_id, para_num) = match stem.rfind('_') {
                    Some(i) => (&stem[..i], &stem[i + 1..]),
                    None => continue,
                };
                let spec_tag = format!("{}-p{}", feature_id, para_num);

                let entry = features
                    .entry(feature_id.to_string())
                    .or_insert((0, 0));
                entry.0 += 1;
                if found_tags.contains(&spec_tag) {
                    entry.1 += 1;
                }
            }
        }
    }

    // Print per-feature status
    println!("Verification Status (source: +spec: markers in code)");
    println!("=====================================================\n");

    let mut total_paragraphs = 0usize;
    let mut total_marked = 0usize;

    for (feature_id, (para_count, marked)) in &features {
        total_paragraphs += para_count;
        total_marked += marked;

        let pct = if *para_count > 0 {
            *marked as f64 / *para_count as f64 * 100.0
        } else {
            0.0
        };

        let bar_width = 20;
        let filled = if *para_count > 0 {
            (bar_width * marked) / para_count
        } else {
            0
        };
        let bar: String = std::iter::repeat('#')
            .take(filled)
            .chain(std::iter::repeat('.').take(bar_width - filled))
            .collect();

        let status = if marked == para_count {
            "DONE"
        } else if *marked > 0 {
            "    "
        } else {
            "    "
        };

        // Look up display name from tree
        let name = tree
            .nodes
            .get(feature_id.as_str())
            .map(|n| n.name.as_str())
            .unwrap_or(feature_id.as_str());

        println!(
            "  {:30} [{bar}] {:>3}/{:<3} ({:>5.1}%) {status}",
            name,
            marked,
            para_count,
            pct,
        );
    }

    let total_pct = if total_paragraphs > 0 {
        total_marked as f64 / total_paragraphs as f64 * 100.0
    } else {
        0.0
    };

    println!("\n{}", "=".repeat(72));
    println!(
        "Total: {}/{} paragraphs marked ({:.1}%)",
        total_marked, total_paragraphs, total_pct
    );
    println!("Unique +spec: tags in source: {}", found_tags.len());

    Ok(())
}

fn cmd_holistic(config: &SpecConfig) -> Result<(), String> {
    let tree = config.load_skill_tree();
    
    let prompt = generate_holistic_prompt(&tree, &config.results_dir());
    
    let output_path = config.skill_tree_dir.join("holistic_prompt.md");
    std::fs::write(&output_path, &prompt)
        .map_err(|e| format!("Failed to write: {}", e))?;
    
    println!("Holistic analysis prompt saved to: {}", output_path.display());
    println!("\nLength: {} chars ({} tokens approx)", prompt.len(), prompt.len() / 4);
    
    Ok(())
}

fn cmd_next(config: &SpecConfig) -> Result<(), String> {
    let tree = config.load_skill_tree();
    
    if let Some(node) = tree.get_next_unverified() {
        println!("Next feature to verify:");
        println!();
        println!("  ID:          {}", node.id);
        println!("  Name:        {}", node.name);
        println!("  Description: {}", node.description);
        println!("  Difficulty:  {}/5", node.difficulty);
        println!();
        println!("Dependencies (verified):");
        for dep in &node.depends_on {
            println!("  ✓ {}", dep);
        }
        println!();
        println!("To review, run:");
        println!("  azul-doc spec review {}", node.id);
    } else {
        // Check if all are verified or if there are blockers
        let unverified: Vec<_> = tree.nodes.values()
            .filter(|n| n.status != VerificationStatus::Verified)
            .collect();
        
        if unverified.is_empty() {
            println!("🎉 All features verified!");
        } else {
            println!("No features are ready for verification.");
            println!("The following have unmet dependencies:\n");
            for node in unverified {
                let missing_deps: Vec<_> = node.depends_on.iter()
                    .filter(|d| {
                        tree.nodes.get(*d)
                            .map(|n| n.status != VerificationStatus::Verified)
                            .unwrap_or(false)
                    })
                    .collect();
                if !missing_deps.is_empty() {
                    println!("  {} - needs: {}", node.id, missing_deps.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
                }
            }
        }
    }
    
    Ok(())
}

/// Call Gemini API with the prompt
fn call_gemini_api(api_key: &str, prompt: &str) -> Result<String, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro-preview-05-06:generateContent?key={}",
        api_key
    );
    
    let request_body = serde_json::json!({
        "contents": [{
            "parts": [{
                "text": prompt
            }]
        }],
        "generationConfig": {
            "thinkingConfig": {
                "thinkingBudget": 32768
            }
        }
    });
    
    // Write request to temp file
    let temp_request = std::env::temp_dir().join("spec_review_request.json");
    std::fs::write(&temp_request, serde_json::to_string(&request_body).unwrap())
        .map_err(|e| format!("Failed to write request: {}", e))?;
    
    // Call curl
    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "-X", "POST",
            "-H", "Content-Type: application/json",
            "-d", &format!("@{}", temp_request.display()),
            "--max-time", "600",
            &url,
        ])
        .output()
        .map_err(|e| format!("Failed to call API: {}", e))?;
    
    if !output.status.success() {
        return Err(format!(
            "API call failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    
    let response_text = String::from_utf8_lossy(&output.stdout);
    
    // Parse response
    let response: serde_json::Value = serde_json::from_str(&response_text)
        .map_err(|e| format!("Failed to parse response: {}\n\nRaw: {}", e, response_text))?;
    
    // Extract text from response
    let text = response["candidates"][0]["content"]["parts"]
        .as_array()
        .ok_or("No parts in response")?
        .iter()
        .filter_map(|p| p["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    
    if text.is_empty() {
        Err(format!("Empty response from API: {}", response_text))
    } else {
        Ok(text)
    }
}

fn cmd_paragraphs() -> Result<(), String> {
    let registry = ParagraphRegistry::new();
    registry.print_all();
    Ok(())
}

fn cmd_annotations(workspace_root: &std::path::Path) -> Result<(), String> {
    println!("Scanning source files for +spec: annotations...\n");
    
    let annotations = scan_source_for_annotations(workspace_root);
    
    if annotations.is_empty() {
        println!("No annotations found.");
        println!("\nTo add annotations, use comments like:");
        println!("  // +spec:css22-box-8.3.1-p1 - margin collapsing");
        return Ok(());
    }
    
    let registry = ParagraphRegistry::new();
    let mut known_count = 0;
    let mut unknown_ids = Vec::new();
    
    for (file, annots) in &annotations {
        println!("## {}\n", file);
        for (spec_id, line, context) in annots {
            let known = if registry.get(spec_id).is_some() {
                known_count += 1;
                "✓"
            } else {
                unknown_ids.push(spec_id.clone());
                "?"
            };
            println!("  L{:4} [{}] {} ", line, known, spec_id);
            println!("         {}\n", context.chars().take(80).collect::<String>());
        }
    }
    
    let total: usize = annotations.values().map(|v| v.len()).sum();
    println!("\nSummary: {} annotations found, {} known, {} unknown", 
        total, known_count, unknown_ids.len());
    
    if !unknown_ids.is_empty() {
        println!("\nUnknown spec IDs (add to paragraphs.rs):");
        for id in unknown_ids.iter().take(10) {
            println!("  - {}", id);
        }
        if unknown_ids.len() > 10 {
            println!("  ... and {} more", unknown_ids.len() - 10);
        }
    }
    
    Ok(())
}
