//! W3C Specification Verification System
//!
//! Semi-automated pipeline for verifying CSS layout compliance against W3C specs.
//! See `doc/README.md` for the full pipeline documentation.
//!
//! ## Pipeline
//!
//! 1. `spec download` + `spec build-all` — fetch specs, build per-paragraph prompts
//! 2. `spec claude-exec` — run parallel agents to generate patches
//! 3. `spec review-md` — generate Gemini review prompt from patches
//! 4. `spec review-arch` — generate Gemini merge-group prompt
//! 5. `spec agent-apply` — apply patches via Claude agents (4-phase workflow)

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
    
    // Handle --help for any subcommand
    if args.len() >= 2 && (args[1] == "--help" || args[1] == "-h") {
        return print_subcommand_help(&args[0]);
    }

    match args[0].as_str() {
        "--help" | "-h" => { print_spec_help(); Ok(()) }
        "download" => cmd_download(&config),
        "tree" => cmd_tree(&config),
        "extract" => {
            if args.len() < 2 {
                return print_subcommand_help("extract");
            }
            cmd_extract(&config, &args[1], workspace_root)
        }
        "build-all" => cmd_build_all(&config, workspace_root),
        "claude-exec" => {
            let rest: Vec<String> = args[1..].to_vec();
            executor::run_executor(&config, workspace_root, &rest)
        }
        "status" => cmd_status(&config, workspace_root),
        "paragraphs" => cmd_paragraphs(),
        "annotations" => cmd_annotations(workspace_root),
        "review-md" => {
            if args.len() < 2 {
                return print_subcommand_help("review-md");
            }
            let no_src = args[1..].iter().any(|a| a == "--no-src");
            let no_spec = args[1..].iter().any(|a| a == "--no-spec");
            let target = args[1..].iter().find(|a| !a.starts_with("--"))
                .ok_or("Missing <dir|hash> argument. Run: spec review-md --help".to_string())?;
            executor::cmd_review_md(target, workspace_root, no_src, no_spec)
        }
        "review-arch" => {
            let no_src = args[1..].iter().any(|a| a == "--no-src");
            let positional: Vec<&String> = args[1..].iter()
                .filter(|a| !a.starts_with("--"))
                .collect();
            if positional.len() < 2 {
                return print_subcommand_help("review-arch");
            }
            executor::cmd_review_arch(positional[0], positional[1], workspace_root, no_src)
        }
        "agent-apply" => {
            let sub_args = &args[1..];
            let parse_flag = |flag: &str| -> Option<String> {
                // Try --flag value
                if let Some(pos) = sub_args.iter().position(|a| a == flag) {
                    return sub_args.get(pos + 1).cloned();
                }
                // Try --flag=value
                let prefix = format!("{}=", flag);
                for a in sub_args {
                    if a.starts_with(&prefix) {
                        return Some(a[prefix.len()..].to_string());
                    }
                }
                None
            };
            let refactor_md = parse_flag("--refactor-md");
            let review_md = parse_flag("--review-md");
            let arch_md = parse_flag("--arch-md");
            let groups_json = parse_flag("--groups-json");
            // The only positional arg is the patch dir
            let flag_values: std::collections::HashSet<&str> = {
                let mut set = std::collections::HashSet::new();
                for flag in &["--refactor-md", "--review-md", "--arch-md", "--groups-json"] {
                    if let Some(pos) = sub_args.iter().position(|a| a == flag) {
                        if let Some(val) = sub_args.get(pos + 1) {
                            set.insert(val.as_str());
                        }
                    }
                }
                set
            };
            let positional: Vec<&str> = sub_args.iter()
                .filter(|a| !a.starts_with("--") && !flag_values.contains(a.as_str()))
                .map(|s| s.as_str())
                .collect();
            let groups_json = groups_json.or_else(|| positional.get(1).map(|s| s.to_string()));
            let patch_dir = positional.first().map(|s| s.to_string());
            if patch_dir.is_none() || groups_json.is_none() {
                return print_subcommand_help("agent-apply");
            }
            let apply_args = executor::AgentApplyArgs {
                patch_dir: patch_dir.unwrap(),
                groups_json: groups_json.unwrap(),
                refactor_md,
                review_md,
                arch_md,
            };
            executor::cmd_agent_apply(&apply_args, workspace_root)
        }
        _ => {
            print_spec_help();
            Err(format!("Unknown spec command: '{}'. Run: azul-doc spec --help", args[0]))
        }
    }
}

fn print_subcommand_help(cmd: &str) -> Result<(), String> {
    match cmd {
        "download" => {
            println!("azul-doc spec download");
            println!();
            println!("Download all registered W3C spec HTML files to doc/target/w3c_specs/.");
            println!("Sources: css-display-3, css22 (visuren, visudet, box, tables), css-text-3.");
        }
        "tree" => {
            println!("azul-doc spec tree");
            println!();
            println!("Display the 16-feature CSS skill tree with dependency tiers and status.");
        }
        "extract" => {
            println!("azul-doc spec extract <feature-id>");
            println!();
            println!("Extract and display spec paragraphs matched by a feature's keywords.");
            println!("Useful for inspecting which W3C text maps to a feature before building prompts.");
            println!();
            println!("Example: azul-doc spec extract block-formatting-context");
        }
        "build-all" => {
            println!("azul-doc spec build-all");
            println!();
            println!("Build one prompt file per spec paragraph per feature.");
            println!("Output: doc/target/skill_tree/prompts/<feature>_<NNN>.md");
            println!("Each prompt is self-contained: feature context + spec paragraph + instructions.");
        }
        "claude-exec" => {
            println!("azul-doc spec claude-exec [options]");
            println!();
            println!("Run parallel Claude agents. Each agent picks an unprocessed prompt,");
            println!("reads the layout source code, and generates a patch.");
            println!();
            println!("Options:");
            println!("  --agents=N        Number of parallel agents (default: 12)");
            println!("  --timeout=S       Per-agent timeout in seconds (default: 480)");
            println!("  --retry-failed    Re-queue previously failed/timed-out prompts");
            println!("  --status          Show done/failed/pending counts and exit");
            println!("  --cleanup         Remove all agent worktrees");
            println!("  --force-api       Allow running with ANTHROPIC_API_KEY set");
        }
        "status" => {
            println!("azul-doc spec status");
            println!();
            println!("Scan source code for +spec: annotation markers and show per-feature");
            println!("verification coverage as a progress bar.");
        }
        "paragraphs" => {
            println!("azul-doc spec paragraphs");
            println!();
            println!("List all known spec paragraph IDs from the paragraph registry.");
        }
        "annotations" => {
            println!("azul-doc spec annotations");
            println!();
            println!("Scan layout source files for // +spec: comments and report coverage.");
        }
        "review-md" => {
            println!("azul-doc spec review-md [options] <dir|commit-hash>");
            println!();
            println!("Generate a Gemini review prompt from patches or commits.");
            println!("Categorizes each patch as CODE (functional) or ANNOT (comment-only).");
            println!();
            println!("Arguments:");
            println!("  <dir>             Directory of .patch files");
            println!("  <commit-hash>     Generate review from <hash>..HEAD");
            println!();
            println!("Options:");
            println!("  --no-src          Omit source file appendix (saves tokens)");
            println!("  --no-spec         Omit W3C spec paragraph context");
            println!();
            println!("Output: /tmp/agent-run-review-prompt.md (feed to Gemini)");
        }
        "review-arch" => {
            println!("azul-doc spec review-arch [options] <patch-dir> <review.md>");
            println!();
            println!("Generate an architecture review prompt for Gemini.");
            println!("Takes the patch directory + the review-md output, produces a prompt");
            println!("that asks Gemini to sort patches into ordered merge groups (JSON).");
            println!();
            println!("Arguments:");
            println!("  <patch-dir>       Directory containing .patch files");
            println!("  <review.md>       Path to the review-md output (e.g. scripts/RUN2.md)");
            println!();
            println!("Options:");
            println!("  --no-src          Omit source file appendix");
            println!();
            println!("Output: /tmp/agent-arch-review-prompt.md (feed to Gemini)");
        }
        "agent-apply" => {
            println!("azul-doc spec agent-apply [flags] <patch-dir>");
            println!();
            println!("Apply patches sequentially using Claude agents, guided by merge groups.");
            println!("Each agent processes one merge group through 4 phases:");
            println!("  1. Refactoring   — implement groundwork relevant to this group");
            println!("  2. LLM-apply     — apply patch semantic intent to current code");
            println!("  3. Compile       — cargo check -p azul-dll --features build-dll");
            println!("  4. Review        — verify against W3C spec, fix, compile again");
            println!();
            println!("Arguments:");
            println!("  <patch-dir>               Directory containing .patch files");
            println!();
            println!("Required flags:");
            println!("  --groups-json <path>      Merge groups JSON (from Gemini via review-arch)");
            println!();
            println!("Optional flags:");
            println!("  --refactor-md <path>      Groundwork/refactoring plan (e.g. GROUNDWORK.md)");
            println!("  --review-md <path>        Patch quality review (output of review-md / Gemini)");
            println!("  --arch-md <path>          Architecture review (Gemini's analysis before grouping)");
            println!();
            println!("Example:");
            println!("  azul-doc spec agent-apply \\");
            println!("    --groups-json scripts/run2.json \\");
            println!("    --refactor-md scripts/GROUNDWORK.md \\");
            println!("    --review-md scripts/RUN2.md \\");
            println!("    --arch-md scripts/ARCH_REVIEW.md \\");
            println!("    doc/target/skill_tree/all_patches/run2_patches");
            println!();
            println!("Patches are moved to applied/, skipped/, or failed/ as they're processed.");
        }
        _ => {
            println!("No help available for '{}'.", cmd);
        }
    }
    Ok(())
}

fn print_spec_help() {
    println!("azul-doc spec — W3C Spec Verification Pipeline");
    println!();
    println!("Stage 1: Generate patches from spec paragraphs");
    println!("  download              Download all W3C specs locally");
    println!("  tree                  Display the CSS feature skill tree");
    println!("  build-all             Build per-paragraph agent prompts for all features");
    println!("  claude-exec           Run parallel Claude agents on prompts");
    println!();
    println!("Stage 2: Review patches (feed output to Gemini)");
    println!("  review-md <dir>       Generate review prompt from patch directory");
    println!();
    println!("Stage 3: Architecture plan (feed output to Gemini)");
    println!("  review-arch <dir> <review.md>");
    println!("                        Generate merge-group prompt from patches + review");
    println!();
    println!("Stage 4: Apply patches via agents");
    println!("  agent-apply [flags] <patch-dir>");
    println!("                        Apply patches via Claude agents (see --help)");
    println!();
    println!("Utilities:");
    println!("  status                Verification progress (scans +spec: markers)");
    println!("  extract <feature>     Show spec paragraphs matched by a feature");
    println!("  paragraphs            List all known spec paragraph IDs");
    println!("  annotations           Scan source for +spec: annotation comments");
    println!();
    println!("Run 'azul-doc spec <command> --help' for detailed usage of any command.");
    println!();
    println!("Full pipeline:");
    println!("  1. azul-doc spec claude-exec --agents=8              # generates patches");
    println!("  2. azul-doc spec review-md --no-src <patch-dir>      # → feed to Gemini");
    println!("  3. azul-doc spec review-arch <patch-dir> <review>    # → feed to Gemini");
    println!("  4. azul-doc spec agent-apply --groups-json plan.json <patch-dir>");
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
