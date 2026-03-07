//! W3C Specification Verification System
//!
//! Semi-automated pipeline for verifying CSS layout compliance against W3C specs.
//! See `doc/README.md` for the full pipeline documentation.
//!
//! ## Pipeline
//!
//! 1. `spec claude-exec` — run parallel agents to generate patches
//! 2. `spec review-md` — generate Gemini prompt for patch quality review
//! 3. `spec review-arch` — generate Gemini prompt for architecture review
//! 4. `spec refactor-md` — generate Gemini prompt for refactoring plan
//! 5. `spec groups-json` — generate Gemini prompt for merge groups (JSON)
//! 6. `spec agent-apply` — apply patches via Claude agents (4-phase workflow)

use std::path::PathBuf;
use std::collections::HashSet;

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
    generate_review_prompt, generate_paragraph_prompt, generate_grouped_prompt,
    read_source_files, save_review_result, load_review_results, update_node_status,
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
            // Legacy alias: redirect to "paragraphs <feature>"
            if args.len() < 2 {
                return print_subcommand_help("paragraphs");
            }
            cmd_paragraphs(&config, Some(&args[1]), workspace_root)
        }
        "build-all" => cmd_build_all(&config, workspace_root),
        "claude-exec" => {
            let rest: Vec<String> = args[1..].to_vec();
            executor::run_executor(&config, workspace_root, &rest)
        }
        "status" => cmd_status(&config, workspace_root),
        "paragraphs" => {
            if args.len() >= 2 {
                cmd_paragraphs(&config, Some(&args[1]), workspace_root)
            } else {
                cmd_paragraphs(&config, None, workspace_root)
            }
        }
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
            let sub_args = &args[1..];
            let no_src = sub_args.iter().any(|a| a == "--no-src");
            let review_md = parse_named_flag(sub_args, "--review-md");
            let positional: Vec<&str> = sub_args.iter()
                .filter(|a| !a.starts_with("--") && !is_flag_value(sub_args, a, &["--review-md"]))
                .map(|s| s.as_str())
                .collect();
            let patch_dir = positional.first().map(|s| *s);
            if patch_dir.is_none() || review_md.is_none() {
                return print_subcommand_help("review-arch");
            }
            executor::cmd_review_arch(patch_dir.unwrap(), &review_md.unwrap(), workspace_root, no_src)
        }
        "refactor-md" => {
            let sub_args = &args[1..];
            let no_src = sub_args.iter().any(|a| a == "--no-src");
            let review_md = parse_named_flag(sub_args, "--review-md");
            let review_arch = parse_named_flag(sub_args, "--review-arch");
            let positional: Vec<&str> = sub_args.iter()
                .filter(|a| !a.starts_with("--") && !is_flag_value(sub_args, a, &["--review-md", "--review-arch"]))
                .map(|s| s.as_str())
                .collect();
            let patch_dir = positional.first().map(|s| *s);
            if patch_dir.is_none() || review_md.is_none() {
                return print_subcommand_help("refactor-md");
            }
            executor::cmd_refactor_md(
                patch_dir.unwrap(),
                &review_md.unwrap(),
                review_arch.as_deref(),
                workspace_root,
                no_src,
            )
        }
        "groups-json" => {
            let sub_args = &args[1..];
            let no_src = sub_args.iter().any(|a| a == "--no-src");
            let flags = &["--review-md", "--review-arch", "--refactor-md"];
            let review_md = parse_named_flag(sub_args, "--review-md");
            let review_arch = parse_named_flag(sub_args, "--review-arch");
            let refactor_md = parse_named_flag(sub_args, "--refactor-md");
            let positional: Vec<&str> = sub_args.iter()
                .filter(|a| !a.starts_with("--") && !is_flag_value(sub_args, a, flags))
                .map(|s| s.as_str())
                .collect();
            let patch_dir = positional.first().map(|s| *s);
            if patch_dir.is_none() || review_md.is_none() {
                return print_subcommand_help("groups-json");
            }
            executor::cmd_groups_json(
                patch_dir.unwrap(),
                &review_md.unwrap(),
                review_arch.as_deref(),
                refactor_md.as_deref(),
                workspace_root,
                no_src,
            )
        }
        "agent-apply" => {
            let sub_args = &args[1..];
            let flags = &["--refactor-md", "--review-md", "--review-arch", "--groups-json"];
            let refactor_md = parse_named_flag(sub_args, "--refactor-md");
            let review_md = parse_named_flag(sub_args, "--review-md");
            let review_arch = parse_named_flag(sub_args, "--review-arch");
            let groups_json = parse_named_flag(sub_args, "--groups-json");
            let positional: Vec<&str> = sub_args.iter()
                .filter(|a| !a.starts_with("--") && !is_flag_value(sub_args, a, flags))
                .map(|s| s.as_str())
                .collect();
            let patch_dir = positional.first().map(|s| s.to_string());
            if patch_dir.is_none() || groups_json.is_none() {
                return print_subcommand_help("agent-apply");
            }
            let apply_args = executor::AgentApplyArgs {
                patch_dir: patch_dir.unwrap(),
                groups_json: groups_json.unwrap(),
                refactor_md,
                review_md,
                review_arch,
            };
            executor::cmd_agent_apply(&apply_args, workspace_root)
        }
        _ => {
            print_spec_help();
            Err(format!("Unknown spec command: '{}'. Run: azul-doc spec --help", args[0]))
        }
    }
}

/// Parse a named flag like `--review-md <value>` or `--review-md=<value>` from args.
fn parse_named_flag(args: &[String], flag: &str) -> Option<String> {
    // Try --flag value
    if let Some(pos) = args.iter().position(|a| a == flag) {
        return args.get(pos + 1).cloned();
    }
    // Try --flag=value
    let prefix = format!("{}=", flag);
    for a in args {
        if a.starts_with(&prefix) {
            return Some(a[prefix.len()..].to_string());
        }
    }
    None
}

/// Check if `candidate` is the value of one of the named flags (i.e. follows it).
fn is_flag_value(args: &[String], candidate: &str, flags: &[&str]) -> bool {
    for flag in flags {
        if let Some(pos) = args.iter().position(|a| a == flag) {
            if let Some(val) = args.get(pos + 1) {
                if val == candidate {
                    return true;
                }
            }
        }
    }
    false
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
            // Legacy alias
            return print_subcommand_help("paragraphs");
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
            println!("azul-doc spec paragraphs              # All paragraphs grouped by feature");
            println!("azul-doc spec paragraphs <feature>    # Paragraphs for one feature (with text)");
            println!();
            println!("List extracted W3C spec paragraphs. Without arguments, shows all paragraphs");
            println!("grouped by feature. With a feature ID, shows detailed paragraph text.");
            println!();
            println!("Example: azul-doc spec paragraphs block-formatting-context");
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
            println!("azul-doc spec review-arch --review-md <REVIEW.md> [options] <patch-dir>");
            println!();
            println!("Generate an architecture review prompt for Gemini.");
            println!();
            println!("Solves the 'tunnel vision' problem: each claude-exec agent only sees");
            println!("one spec paragraph. This prompt gives Gemini all patches + original");
            println!("spec paragraphs together, so it can identify cross-cutting concerns:");
            println!("  - Patches that contradict each other or duplicate work");
            println!("  - Architectural changes needed to support multiple patches cleanly");
            println!("  - ABI concerns (#[repr(C)] struct changes)");
            println!("  - Ordering constraints between patches");
            println!();
            println!("Arguments:");
            println!("  <patch-dir>              Directory containing .patch files");
            println!();
            println!("Required flags:");
            println!("  --review-md <REVIEW.md>  Gemini output from the review-md step");
            println!();
            println!("Options:");
            println!("  --no-src                 Omit source file appendix");
            println!();
            println!("Output: /tmp/agent-review-arch-prompt.md");
            println!("Feed to Gemini, save output, pass to agent-apply via --review-arch");
        }
        "refactor-md" => {
            println!("azul-doc spec refactor-md --review-md <REVIEW.md> [options] <patch-dir>");
            println!();
            println!("Generate a refactoring plan prompt for Gemini.");
            println!("Asks Gemini to identify abstractions and helpers needed before");
            println!("applying patches (groundwork that prevents ad-hoc code scattering).");
            println!();
            println!("Arguments:");
            println!("  <patch-dir>                Directory containing .patch files");
            println!();
            println!("Required flags:");
            println!("  --review-md <REVIEW.md>    Gemini output from the review-md step");
            println!();
            println!("Optional flags:");
            println!("  --review-arch <ARCH.md>    Gemini output from the review-arch step");
            println!();
            println!("Options:");
            println!("  --no-src                   Omit source file appendix");
            println!();
            println!("Output: /tmp/agent-refactor-prompt.md");
            println!("Feed to Gemini, save output, pass to agent-apply via --refactor-md");
        }
        "groups-json" => {
            println!("azul-doc spec groups-json --review-md <REVIEW.md> [options] <patch-dir>");
            println!();
            println!("Generate a merge-group prompt for Gemini.");
            println!("Asks Gemini to sort patches into ordered merge groups (JSON output)");
            println!("with actions (APPLY/MERGE/PICK_ONE/SKIP) and per-group agent_context.");
            println!();
            println!("This is the final analysis step. It receives all prior Gemini outputs");
            println!("so it can produce well-informed merge groups and agent_context.");
            println!();
            println!("Arguments:");
            println!("  <patch-dir>                Directory containing .patch files");
            println!();
            println!("Required flags:");
            println!("  --review-md <REVIEW.md>    Gemini output from the review-md step");
            println!();
            println!("Optional flags:");
            println!("  --review-arch <ARCH.md>    Gemini output from the review-arch step");
            println!("  --refactor-md <REFACTOR.md>  Gemini output from the refactor-md step");
            println!();
            println!("Options:");
            println!("  --no-src                   Omit source file appendix");
            println!();
            println!("Output: /tmp/agent-groups-json-prompt.md");
            println!("Feed to Gemini, save JSON output, pass to agent-apply via --groups-json");
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
            println!("  --groups-json <path>      Merge groups JSON (from groups-json / Gemini)");
            println!();
            println!("Optional flags:");
            println!("  --refactor-md <path>      Refactoring plan (from refactor-md / Gemini)");
            println!("  --review-md <path>        Patch quality review (from review-md / Gemini)");
            println!("  --review-arch <path>      Architecture review (from review-arch / Gemini)");
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
    println!("Stage 2: Analyze patches with Gemini (each generates a prompt → feed to Gemini)");
    println!("  review-md             Patch quality review (CODE/ANNOT, conflicts, skip list)");
    println!("  review-arch           Architecture review (cross-patch tunnel vision fix)");
    println!("  refactor-md           Refactoring plan (groundwork before applying patches)");
    println!("  groups-json           Merge groups (ordered JSON: APPLY/MERGE/PICK_ONE/SKIP)");
    println!();
    println!("Stage 3: Apply patches via agents");
    println!("  agent-apply           Apply patches via Claude agents (4-phase workflow)");
    println!();
    println!("Utilities:");
    println!("  status                Verification progress (scans +spec: markers)");
    println!("  paragraphs            All paragraphs grouped by feature");
    println!("  paragraphs <feature>  Show paragraphs for one feature (with text)");
    println!("  annotations           Scan source for +spec: annotation comments");
    println!();
    println!("Run 'azul-doc spec <command> --help' for detailed usage of any command.");
    println!();
    println!("Full pipeline (each step feeds its output into subsequent steps):");
    println!();
    println!("  1. claude-exec --agents=8                                    # patches");
    println!("  2. review-md --no-src <dir>                                  # → Gemini");
    println!("  3. review-arch --review-md REVIEW.md <dir>                   # → Gemini");
    println!("  4. refactor-md --review-md REVIEW.md --review-arch ARCH.md <dir>");
    println!("                                                               # → Gemini");
    println!("  5. groups-json --review-md REVIEW.md --review-arch ARCH.md \\");
    println!("       --refactor-md REFACTOR.md <dir>                         # → Gemini");
    println!("  6. agent-apply --groups-json GROUPS.json --review-md REVIEW.md \\");
    println!("       --review-arch ARCH.md --refactor-md REFACTOR.md <dir>");
}

/// Build one prompt file per deduplicated spec paragraph, per feature.
///
/// Each file is self-contained: feature context + single paragraph + instructions.
/// Agents pick up individual files and read the source code themselves.
pub(crate) fn cmd_build_all(config: &SpecConfig, workspace_root: &std::path::Path) -> Result<(), String> {
    use std::collections::{HashMap, HashSet};

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

    // Phase 1: Extract all paragraphs per feature, then deduplicate across features.
    // Each unique paragraph (by text) is assigned to the feature with the most
    // keyword matches. This eliminates ~73% of redundant prompts.

    struct FeatureParas {
        node: skill_tree::SkillNode,
        paragraphs: Vec<extractor::ExtractedParagraph>,
    }

    let mut all_features: Vec<(String, FeatureParas)> = Vec::new();

    for node_id in &node_ids {
        let node = tree.nodes.get(node_id).unwrap().clone();
        let paragraphs = match extract_for_skill_node(&node, &config.spec_dir) {
            Ok(p) => p,
            Err(e) => {
                println!("  [SKIP] {}: {}", node_id, e);
                continue;
            }
        };
        all_features.push((node_id.clone(), FeatureParas { node, paragraphs }));
    }

    // Build a map: paragraph_text → best (feature_id, keyword_count)
    // Paragraphs seen by multiple features are assigned to the one with more matches.
    let mut para_owner: HashMap<String, (String, usize)> = HashMap::new();

    for (node_id, fp) in &all_features {
        for para in &fp.paragraphs {
            let key = para.text.clone();
            let kw_count = para.matched_keywords.len();
            let entry = para_owner.entry(key).or_insert_with(|| (node_id.clone(), 0));
            if kw_count > entry.1 || (kw_count == entry.1 && node_id < &entry.0) {
                *entry = (node_id.clone(), kw_count);
            }
        }
    }

    let total_before_dedup: usize = all_features.iter().map(|(_, fp)| fp.paragraphs.len()).sum();

    // Phase 2: Generate prompts, skipping paragraphs owned by other features.
    let mut total_files = 0usize;
    let mut total_tokens = 0usize;
    let mut total_deduped = 0usize;

    let max_group_size = 3usize;

    for (node_id, fp) in &all_features {
        // Keep only paragraphs owned by this feature
        let owned_paras: Vec<&extractor::ExtractedParagraph> = fp.paragraphs.iter()
            .filter(|p| para_owner.get(&p.text).map(|(owner, _)| owner == node_id).unwrap_or(false))
            .collect();

        let skipped = fp.paragraphs.len() - owned_paras.len();
        total_deduped += skipped;

        let para_count = owned_paras.len();
        let mut feature_tokens = 0usize;
        let mut seen_filenames = HashSet::new();

        // Semantic grouping: group consecutive paragraphs from the same
        // source file that share ≥ 2/3 of their keywords, up to 3 per group.
        let groups = group_paragraphs_by_keyword_overlap(&owned_paras, max_group_size);
        let total_groups = groups.len();

        for (group_idx, group) in groups.iter().enumerate() {
            let group_refs: Vec<&extractor::ExtractedParagraph> = group.iter().copied().collect();
            let prompt = reviewer::generate_grouped_prompt(
                &fp.node, &group_refs, group_idx, total_groups, workspace_root,
            );

            let tokens = prompt.len() / 4;
            feature_tokens += tokens;

            // Filename = concatenated hashes with + separator
            let hashes: Vec<String> = group.iter()
                .map(|p| extractor::paragraph_content_hash(p))
                .collect();
            let hash_part = hashes.join("+");
            let mut filename = format!("{}_{}.md", node_id, hash_part);
            // Handle hash collisions by appending a counter suffix
            if seen_filenames.contains(&filename) {
                let mut suffix = 2;
                loop {
                    filename = format!("{}_{}{}.md", node_id, hash_part, suffix);
                    if !seen_filenames.contains(&filename) {
                        break;
                    }
                    suffix += 1;
                }
            }
            seen_filenames.insert(filename.clone());
            let prompt_path = prompts_dir.join(&filename);
            std::fs::write(&prompt_path, &prompt)
                .map_err(|e| format!("Failed to write {}: {}", prompt_path.display(), e))?;
        }

        total_files += total_groups;
        total_tokens += feature_tokens;

        let text_indicator = if fp.node.needs_text_engine { " +text3" } else { "" };
        let css_indicator = if fp.node.needs_css_source { " +css" } else { "" };
        let dedup_note = if skipped > 0 {
            format!("  (-{} deduped)", skipped)
        } else {
            String::new()
        };
        let avg_group = if total_groups > 0 { para_count as f64 / total_groups as f64 } else { 0.0 };
        println!("  [OK] {:30} {:>4} files ({} paras, {:.1}/group avg)  (~{} tokens avg){}{}{}",
            node_id,
            total_groups,
            para_count,
            avg_group,
            if total_groups > 0 { feature_tokens / total_groups } else { 0 },
            text_indicator,
            css_indicator,
            dedup_note,
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
    if total_deduped > 0 {
        println!("Deduplication: {} → {} prompts ({} duplicates removed)",
            total_before_dedup, total_files, total_deduped);
    }
    println!("Prompts saved to: {}", prompts_dir.display());

    Ok(())
}

/// Group consecutive paragraphs from the same source file that share
/// at least 2/3 of their keywords, up to `max_size` per group.
///
/// Walks through paragraphs in order. For each paragraph, tries to extend
/// the current group if:
///   1. Same `source_file` as previous paragraph
///   2. Keyword overlap with the *first* paragraph in the group is ≥ 2/3
///      (using Jaccard-like: |intersection| / |smaller set| ≥ 2/3)
///   3. Group hasn't reached `max_size`
///
/// Otherwise, starts a new group. Singletons are fine.
fn group_paragraphs_by_keyword_overlap<'a>(
    paras: &[&'a extractor::ExtractedParagraph],
    max_size: usize,
) -> Vec<Vec<&'a extractor::ExtractedParagraph>> {
    if paras.is_empty() {
        return Vec::new();
    }

    let keywords_overlap_sufficient = |a: &extractor::ExtractedParagraph, b: &extractor::ExtractedParagraph| -> bool {
        let set_a: HashSet<&str> = a.matched_keywords.iter().map(|s| s.as_str()).collect();
        let set_b: HashSet<&str> = b.matched_keywords.iter().map(|s| s.as_str()).collect();
        let intersection = set_a.intersection(&set_b).count();
        let smaller = set_a.len().min(set_b.len());
        // Need ≥ 2/3 overlap relative to the smaller set.
        // Use integer math: intersection * 3 >= smaller * 2
        if smaller == 0 {
            return false;
        }
        intersection * 3 >= smaller * 2
    };

    let mut groups: Vec<Vec<&'a extractor::ExtractedParagraph>> = Vec::new();
    let mut current_group: Vec<&'a extractor::ExtractedParagraph> = vec![paras[0]];

    for para in &paras[1..] {
        let anchor = current_group[0];
        let can_extend = current_group.len() < max_size
            && para.source_file == anchor.source_file
            && keywords_overlap_sufficient(anchor, para);

        if can_extend {
            current_group.push(para);
        } else {
            groups.push(std::mem::take(&mut current_group));
            current_group.push(para);
        }
    }

    if !current_group.is_empty() {
        groups.push(current_group);
    }

    groups
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

fn cmd_paragraphs_for_feature(config: &SpecConfig, feature_id: &str) -> Result<(), String> {
    let tree = config.load_skill_tree();

    let node = tree.nodes.get(feature_id)
        .ok_or_else(|| format!("Unknown feature: {}. Use 'spec tree' to see available features.", feature_id))?;

    println!("Spec paragraphs for: {}\n", node.name);
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
                let (feature_id, hash_part) = match stem.rfind('_') {
                    Some(i) => (&stem[..i], &stem[i + 1..]),
                    None => continue,
                };
                // hash_part may be "a3f2c1" or "a3f2c1+b4e7d2" (grouped)
                let hashes: Vec<&str> = hash_part.split('+').collect();

                let entry = features
                    .entry(feature_id.to_string())
                    .or_insert((0, 0));
                entry.0 += hashes.len();
                for h in &hashes {
                    // Tag format: "feature:hash" (colon separator)
                    let spec_tag = format!("{}:{}", feature_id, h);
                    if found_tags.contains(&spec_tag) {
                        entry.1 += 1;
                    }
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


fn cmd_paragraphs(config: &SpecConfig, feature: Option<&str>, workspace_root: &std::path::Path) -> Result<(), String> {
    if let Some(feature_id) = feature {
        return cmd_paragraphs_for_feature(config, feature_id);
    }

    // No feature specified: show all paragraphs grouped by feature
    let tree = config.load_skill_tree();
    let mut total = 0usize;

    for (feature_id, node) in &tree.nodes {
        match extract_for_skill_node(node, &config.spec_dir) {
            Ok(paragraphs) if !paragraphs.is_empty() => {
                println!("## {} ({}) — {} paragraphs\n",
                    node.name, feature_id, paragraphs.len());
                for (i, para) in paragraphs.iter().enumerate() {
                    println!("  {}. [{}] {} (from {})",
                        i + 1,
                        para.matched_keywords.join(", "),
                        para.section,
                        para.source_file,
                    );
                }
                println!();
                total += paragraphs.len();
            }
            Ok(_) => {
                println!("## {} ({}) — 0 paragraphs\n", node.name, feature_id);
            }
            Err(e) => {
                eprintln!("## {} ({}) — error: {}\n", node.name, feature_id, e);
            }
        }
    }

    println!("Total: {} paragraphs across {} features", total, tree.nodes.len());
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
