//! Git history regression analysis for reftests
//!
//! Commands:
//! - `debug-regression <file.txt>` - Process commits listed in file (one hash per line)
//! - `debug-regression statistics` - Generate HTML report from existing data

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    process::Command,
};

use image::GenericImageView;

/// Persistent temp directory for git worktree
const TEMP_DIR_NAME: &str = "azul-regression-worktree";

/// Configuration for regression analysis
pub struct RegressionConfig {
    /// Root of the azul repository
    pub azul_root: PathBuf,
    /// Path to file containing git refs (one per line), or direct refs
    pub refs_file: Option<PathBuf>,
    /// Direct refs (if not using file)
    pub refs: Vec<String>,
    /// Test directory containing .xht files
    pub test_dir: PathBuf,
    /// Output directory for cached screenshots (doc/target/reftest)
    pub output_dir: PathBuf,
}

/// Information about a commit
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub hash: String,
    pub short_hash: String,
    pub message: String,
    pub date: String,
    pub author: String,
}

/// Status of a commit's reftest run
#[derive(Debug, Clone)]
pub enum CommitStatus {
    /// Not yet processed
    Pending,
    /// Build failed with error message
    BuildFailed(String),
    /// Reftest ran successfully, screenshots available
    Success {
        screenshots: HashMap<String, PathBuf>,
    },
}

/// Load refs from a file (one hash per line)
fn load_refs_from_file(path: &Path) -> anyhow::Result<Vec<String>> {
    let file = fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let refs: Vec<String> = reader
        .lines()
        .filter_map(|line| {
            let line = line.ok()?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect();
    Ok(refs)
}

/// Run regression analysis for specific commits
pub fn run_regression_analysis(config: RegressionConfig) -> anyhow::Result<()> {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║           Azul Reftest Regression Analysis                    ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
    println!();
    
    // Create regression output directory
    let regression_dir = config.output_dir.join("regression");
    fs::create_dir_all(&regression_dir)?;
    
    // Step 1: Get persistent temp directory
    let temp_dir = get_or_create_temp_dir(&config)?;
    println!("[1/4] Temp directory: {}", temp_dir.display());
    
    // Step 2: Load refs from file or use direct refs
    let refs = if let Some(ref file_path) = config.refs_file {
        println!("[2/4] Loading refs from file: {}", file_path.display());
        load_refs_from_file(file_path)?
    } else {
        config.refs.clone()
    };
    
    // Create a temporary config with loaded refs for resolve_refs
    let config_with_refs = RegressionConfig {
        azul_root: config.azul_root.clone(),
        refs_file: None,
        refs,
        test_dir: config.test_dir.clone(),
        output_dir: config.output_dir.clone(),
    };
    
    let commits = resolve_refs(&config_with_refs)?;
    println!("       Resolved {} commit(s) to process", commits.len());
    
    if commits.is_empty() {
        println!("No commits to analyze.");
        return Ok(());
    }
    
    // Step 3: Find which commits still need processing
    let pending = find_pending_commits(&commits, &regression_dir);
    println!("[3/4] {} commits need processing ({} already done)", 
             pending.len(), commits.len() - pending.len());
    
    if pending.is_empty() {
        println!("All specified commits already processed.");
        println!("Run 'debug-regression statistics' to generate the report.");
        return Ok(());
    }
    
    // Step 4: Process each pending commit
    println!("[4/4] Processing pending commits...\n");
    
    for (idx, commit) in pending.iter().enumerate() {
        process_commit(
            commit,
            &temp_dir,
            &regression_dir,
            idx + 1,
            pending.len(),
        )?;
    }
    
    println!("\n╔═══════════════════════════════════════════════════════════════╗");
    println!("║                    Processing Complete!                        ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
    println!("Run 'debug-regression statistics' to generate the comparison report.");
    
    Ok(())
}

/// Generate statistics report from existing data
pub fn run_statistics(config: RegressionConfig) -> anyhow::Result<()> {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║           Azul Reftest Regression Statistics                  ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
    println!();
    
    let regression_dir = config.output_dir.join("regression");
    
    if !regression_dir.exists() {
        println!("No regression data found. Run 'debug-regression' first.");
        return Ok(());
    }
    
    // Ensure Chrome references exist
    println!("[1/3] Checking Chrome references...");
    let test_files = find_test_files(&config.test_dir)?;
    let chrome_dir = regression_dir.join("chrome");
    ensure_chrome_references(&test_files, &chrome_dir)?;
    
    // Collect all processed commits
    println!("[2/3] Collecting commit data...");
    let commits = collect_processed_commits(&regression_dir)?;
    println!("  Found {} processed commits", commits.len());
    
    // Generate HTML report
    println!("[3/3] Generating comparison report...");
    generate_html_report(&commits, &regression_dir, &chrome_dir)?;
    
    println!("\n╔═══════════════════════════════════════════════════════════════╗");
    println!("║                    Report Generated!                           ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
    println!("Report: {}", regression_dir.join("index.html").display());
    
    Ok(())
}

/// Get or create the persistent temp directory
fn get_or_create_temp_dir(config: &RegressionConfig) -> anyhow::Result<PathBuf> {
    let temp_base = std::env::temp_dir().join(TEMP_DIR_NAME);
    
    if temp_base.exists() {
        // Check if it's a valid git repo
        let git_dir = temp_base.join(".git");
        if git_dir.exists() || temp_base.join("HEAD").exists() {
            println!("  Using existing worktree at {}", temp_base.display());
            return Ok(temp_base);
        }
        // Invalid, remove and recreate
        println!("  Removing invalid temp directory...");
        let _ = fs::remove_dir_all(&temp_base);
    }
    
    // Clone the repository
    println!("  Cloning repository to temp directory (this may take a while)...");
    
    let output = Command::new("git")
        .args([
            "clone",
            "--no-checkout",
            "--shared",
            config.azul_root.to_str().unwrap(),
            temp_base.to_str().unwrap(),
        ])
        .output()?;
    
    if !output.status.success() {
        // Try without --shared for compatibility
        let output = Command::new("git")
            .args([
                "clone",
                "--no-checkout",
                config.azul_root.to_str().unwrap(),
                temp_base.to_str().unwrap(),
            ])
            .output()?;
        
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Failed to clone repository: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    
    println!("  Clone complete.");
    Ok(temp_base)
}

/// Resolve git refs to commit info
fn resolve_refs(config: &RegressionConfig) -> anyhow::Result<Vec<CommitInfo>> {
    let mut commits = Vec::new();
    
    for git_ref in &config.refs {
        let output = Command::new("git")
            .current_dir(&config.azul_root)
            .args(["log", "-1", "--format=%H|%h|%s|%ci|%an", git_ref])
            .output()?;
        
        if !output.status.success() {
            println!("  Warning: Could not resolve ref '{}', skipping", git_ref);
            continue;
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.trim();
        
        if let Some(commit) = parse_commit_line(line) {
            commits.push(commit);
        }
    }
    
    Ok(commits)
}

/// Parse a git log line into CommitInfo
fn parse_commit_line(line: &str) -> Option<CommitInfo> {
    let parts: Vec<&str> = line.splitn(5, '|').collect();
    if parts.len() == 5 {
        Some(CommitInfo {
            hash: parts[0].to_string(),
            short_hash: parts[1].to_string(),
            message: parts[2].to_string(),
            date: parts[3].to_string(),
            author: parts[4].to_string(),
        })
    } else {
        None
    }
}

/// Find commits that haven't been processed yet
fn find_pending_commits(commits: &[CommitInfo], regression_dir: &Path) -> Vec<CommitInfo> {
    commits
        .iter()
        .filter(|c| {
            let commit_dir = regression_dir.join(&c.short_hash);
            if !commit_dir.exists() {
                return true;
            }
            // Check if there's a completion marker
            if commit_dir.join("COMPLETE").exists() || commit_dir.join("BUILD_ERROR.txt").exists() {
                return false;
            }
            true
        })
        .cloned()
        .collect()
}

/// Collect all processed commits from the regression directory
fn collect_processed_commits(regression_dir: &Path) -> anyhow::Result<Vec<(CommitInfo, CommitStatus)>> {
    let mut results = Vec::new();
    
    for entry in fs::read_dir(regression_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if !path.is_dir() {
            continue;
        }
        
        let dir_name = path.file_name().unwrap().to_string_lossy().to_string();
        
        // Skip chrome directory
        if dir_name == "chrome" {
            continue;
        }
        
        // Try to read commit info from the directory
        let commit_info_path = path.join("commit_info.txt");
        let commit = if commit_info_path.exists() {
            let content = fs::read_to_string(&commit_info_path)?;
            parse_commit_line(&content).unwrap_or_else(|| CommitInfo {
                hash: dir_name.clone(),
                short_hash: dir_name.clone(),
                message: "Unknown".to_string(),
                date: "Unknown".to_string(),
                author: "Unknown".to_string(),
            })
        } else {
            CommitInfo {
                hash: dir_name.clone(),
                short_hash: dir_name.clone(),
                message: "Unknown".to_string(),
                date: "Unknown".to_string(),
                author: "Unknown".to_string(),
            }
        };
        
        let status = if path.join("BUILD_ERROR.txt").exists() {
            let err = fs::read_to_string(path.join("BUILD_ERROR.txt"))
                .unwrap_or_else(|_| "Unknown error".to_string());
            CommitStatus::BuildFailed(err)
        } else if path.join("COMPLETE").exists() {
            // Collect screenshots
            let mut screenshots = HashMap::new();
            for file_entry in fs::read_dir(&path)? {
                let file_entry = file_entry?;
                let filename = file_entry.file_name().to_string_lossy().to_string();
                if filename.ends_with("_azul.webp") || filename.ends_with("_azul.png") {
                    let test_name = filename
                        .trim_end_matches("_azul.webp")
                        .trim_end_matches("_azul.png")
                        .to_string();
                    screenshots.insert(test_name, file_entry.path());
                }
            }
            CommitStatus::Success { screenshots }
        } else {
            CommitStatus::Pending
        };
        
        results.push((commit, status));
    }
    
    // Sort by short_hash for consistent ordering
    results.sort_by(|a, b| a.0.short_hash.cmp(&b.0.short_hash));
    
    Ok(results)
}

/// Process a single commit
fn process_commit(
    commit: &CommitInfo,
    temp_dir: &Path,
    regression_dir: &Path,
    current: usize,
    total: usize,
) -> anyhow::Result<()> {
    let commit_dir = regression_dir.join(&commit.short_hash);
    fs::create_dir_all(&commit_dir)?;
    
    // Save commit info for later
    fs::write(
        commit_dir.join("commit_info.txt"),
        format!("{}|{}|{}|{}|{}", commit.hash, commit.short_hash, commit.message, commit.date, commit.author)
    )?;
    
    println!("┌─ [{}/{}] {} ─────────────────────────────────────", current, total, commit.short_hash);
    println!("│  {}", commit.message.chars().take(60).collect::<String>());
    println!("│  by {} on {}", commit.author, commit.date.split_whitespace().next().unwrap_or(""));
    
    // Checkout this commit in the temp directory
    print!("│  [CHECKOUT] ");
    std::io::stdout().flush()?;
    
    let output = Command::new("git")
        .current_dir(temp_dir)
        .args(["checkout", "--force", &commit.hash])
        .output()?;
    
    if !output.status.success() {
        let err = format!("Checkout failed: {}", String::from_utf8_lossy(&output.stderr));
        println!("FAILED");
        fs::write(commit_dir.join("BUILD_ERROR.txt"), &err)?;
        println!("└─ Skipped (checkout error)\n");
        return Ok(());
    }
    println!("OK");
    
    // Run reftest (this will build azul-doc and run all tests in /doc/working)
    print!("│  [REFTEST] cargo run --release -p azul-doc -- reftest ");
    std::io::stdout().flush()?;
    
    let reftest_output_dir = temp_dir.join("doc").join("target").join("reftest");
    
    // Run: cargo run --release -p azul-doc -- reftest
    // This builds azul-doc and runs all reftests in /doc/working
    let output = Command::new("cargo")
        .current_dir(temp_dir)
        .args(["run", "--release", "-p", "azul-doc", "--", "reftest"])
        .env("CARGO_TERM_COLOR", "never")
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Combine stderr and stdout for error info
        let combined = format!("{}\n{}", stderr, stdout);
        let error_summary: String = combined
            .lines()
            .filter(|l| l.contains("error[") || l.contains("error:") || l.contains("FAILED"))
            .take(15)
            .collect::<Vec<_>>()
            .join("\n");
        
        let err = if error_summary.is_empty() {
            combined.lines().rev().take(30).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")
        } else {
            error_summary
        };
        
        println!("FAILED");
        fs::write(commit_dir.join("BUILD_ERROR.txt"), &err)?;
        println!("└─ Skipped (build/reftest error)\n");
        return Ok(());
    }
    println!("OK");
    
    // Copy screenshots to regression folder
    print!("│  [COPY] ");
    std::io::stdout().flush()?;
    
    let mut count = 0;
    let reftest_img_dir = reftest_output_dir.join("reftest_img");
    
    if reftest_img_dir.exists() {
        for entry in fs::read_dir(&reftest_img_dir)? {
            let entry = entry?;
            let path = entry.path();
            let filename = path.file_name().unwrap().to_string_lossy();
            
            // Copy only azul screenshots
            if filename.ends_with("_azul.webp") || filename.ends_with("_azul.png") {
                let dest = commit_dir.join(&*filename);
                fs::copy(&path, &dest)?;
                count += 1;
            }
        }
    }
    
    // Also copy results.json if it exists
    let results_json = reftest_output_dir.join("results.json");
    if results_json.exists() {
        fs::copy(&results_json, commit_dir.join("results.json"))?;
    }
    
    println!("{} files", count);
    
    // Mark as complete
    fs::write(commit_dir.join("COMPLETE"), "")?;
    
    println!("└─ Done\n");
    Ok(())
}

/// Find test files in directory
fn find_test_files(test_dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    
    if !test_dir.exists() {
        return Ok(files);
    }
    
    for entry in fs::read_dir(test_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy().to_lowercase();
                if ext == "xht" || ext == "xhtml" || ext == "html" {
                    files.push(path);
                }
            }
        }
    }
    
    files.sort();
    Ok(files)
}

/// Ensure Chrome reference screenshots exist
fn ensure_chrome_references(
    test_files: &[PathBuf],
    chrome_dir: &Path,
) -> anyhow::Result<()> {
    fs::create_dir_all(chrome_dir)?;
    let chrome_path = super::get_chrome_path();
    
    let mut generated = 0;
    for test_file in test_files {
        let test_name = test_file.file_stem().unwrap().to_string_lossy().to_string();
        let chrome_img = chrome_dir.join(format!("{}.png", test_name));
        
        if chrome_img.exists() {
            continue;
        }
        
        print!("  Generating Chrome reference for {}... ", test_name);
        std::io::stdout().flush()?;
        
        let layout_json = chrome_dir.join(format!("{}_layout.json", test_name));
        
        match super::generate_chrome_screenshot_with_debug(
            &chrome_path,
            test_file,
            &chrome_img,
            &layout_json,
            super::WIDTH,
            super::HEIGHT,
        ) {
            Ok(_) => {
                println!("OK");
                generated += 1;
            }
            Err(e) => {
                println!("FAILED: {}", e);
            }
        }
    }
    
    if generated == 0 {
        println!("  All Chrome references already exist.");
    } else {
        println!("  Generated {} new Chrome references.", generated);
    }
    
    Ok(())
}

/// Generate HTML comparison report
fn generate_html_report(
    commit_data: &[(CommitInfo, CommitStatus)],
    regression_dir: &Path,
    chrome_dir: &Path,
) -> anyhow::Result<()> {
    // Collect all test names from Chrome dir
    let mut test_names: Vec<String> = Vec::new();
    if chrome_dir.exists() {
        for entry in fs::read_dir(chrome_dir)? {
            let entry = entry?;
            let filename = entry.file_name().to_string_lossy().to_string();
            if filename.ends_with(".png") && !filename.contains("_layout") {
                let name = filename.trim_end_matches(".png").to_string();
                test_names.push(name);
            }
        }
    }
    test_names.sort();
    
    // Calculate diffs between commits and Chrome reference
    let mut test_diffs: BTreeMap<String, Vec<(String, String, i64, i64)>> = BTreeMap::new();
    for test in &test_names {
        test_diffs.insert(test.clone(), Vec::new());
    }
    
    let chrome_ref_diffs: HashMap<String, i64> = HashMap::new();
    
    for (commit, status) in commit_data {
        if let CommitStatus::Success { screenshots } = status {
            for test in &test_names {
                if let Some(azul_path) = screenshots.get(test) {
                    let chrome_path = chrome_dir.join(format!("{}.png", test));
                    
                    if chrome_path.exists() {
                        if let Ok(diff_to_chrome) = compare_images(&chrome_path, azul_path) {
                            if let Some(diffs) = test_diffs.get_mut(test) {
                                diffs.push((
                                    commit.short_hash.clone(),
                                    commit.message.clone(),
                                    diff_to_chrome,
                                    0, // We'll calculate diff from previous later
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Generate HTML
    let mut html = String::new();
    html.push_str(r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Azul Reftest Regression Analysis</title>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, sans-serif; margin: 20px; background: #f5f5f5; }
        h1, h2 { color: #333; }
        .summary { background: white; padding: 20px; border-radius: 8px; margin-bottom: 20px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
        .test-section { background: white; padding: 20px; border-radius: 8px; margin-bottom: 20px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
        .test-name { font-size: 1.2em; font-weight: bold; margin-bottom: 10px; color: #2196F3; }
        .commit-hash { font-family: monospace; background: #eee; padding: 2px 6px; border-radius: 3px; }
        .pixel-diff { font-weight: bold; }
        .diff-good { color: #4CAF50; }
        .diff-bad { color: #f44336; }
        .diff-neutral { color: #ff9800; }
        table { width: 100%; border-collapse: collapse; margin-top: 10px; }
        th, td { padding: 8px; text-align: left; border-bottom: 1px solid #ddd; }
        th { background: #f0f0f0; }
        .status-ok { color: #4CAF50; }
        .status-failed { color: #f44336; }
        .status-pending { color: #ff9800; }
        .comparison { display: flex; gap: 20px; margin-top: 15px; flex-wrap: wrap; }
        .comparison-item { text-align: center; }
        .comparison-item img { max-width: 400px; max-height: 300px; border: 1px solid #ddd; }
        .comparison-label { font-size: 0.9em; color: #666; margin-bottom: 5px; }
        details { margin-top: 10px; }
        summary { cursor: pointer; color: #2196F3; font-weight: bold; }
        .build-error { background: #ffebee; padding: 10px; border-radius: 4px; font-family: monospace; font-size: 0.85em; white-space: pre-wrap; max-height: 200px; overflow: auto; }
    </style>
</head>
<body>
    <h1>🔍 Azul Reftest Regression Analysis</h1>
"#);

    // Summary section
    let total = commit_data.len();
    let success = commit_data.iter().filter(|(_, s)| matches!(s, CommitStatus::Success { .. })).count();
    let failed = commit_data.iter().filter(|(_, s)| matches!(s, CommitStatus::BuildFailed(_))).count();
    let pending = commit_data.iter().filter(|(_, s)| matches!(s, CommitStatus::Pending)).count();
    
    html.push_str(&format!(r#"
    <div class="summary">
        <h2>Summary</h2>
        <p><strong>Commits processed:</strong> {} total, {} successful, {} failed, {} pending</p>
        <p><strong>Tests tracked:</strong> {}</p>
    </div>
"#, total, success, failed, pending, test_names.len()));

    // Test results by test
    html.push_str("<h2>📊 Results by Test</h2>\n");

    for test in &test_names {
        let diffs = test_diffs.get(test).unwrap();
        let chrome_img = format!("chrome/{}.png", test);
        
        html.push_str(&format!(r#"
    <div class="test-section">
        <div class="test-name">{}</div>
"#, test));

        if diffs.is_empty() {
            html.push_str("        <p>No data available for this test.</p>\n");
        } else {
            html.push_str("        <table>\n");
            html.push_str("            <tr><th>Commit</th><th>Message</th><th>Diff from Chrome</th></tr>\n");
            
            for (hash, msg, diff_chrome, _) in diffs {
                let diff_class = if *diff_chrome == 0 {
                    "diff-good"
                } else if *diff_chrome < 10000 {
                    "diff-neutral"
                } else {
                    "diff-bad"
                };
                
                let percentage = (*diff_chrome as f64 / (1920.0 * 1080.0)) * 100.0;
                
                html.push_str(&format!(
                    "            <tr><td><span class=\"commit-hash\">{}</span></td><td>{}</td><td class=\"{}\">{}px ({:.2}%)</td></tr>\n",
                    hash,
                    msg.chars().take(50).collect::<String>(),
                    diff_class,
                    diff_chrome,
                    percentage
                ));
            }
            html.push_str("        </table>\n");
        }
        
        // Comparison images
        html.push_str(&format!(r#"
        <details>
            <summary>View Screenshots</summary>
            <div class="comparison">
                <div class="comparison-item">
                    <div class="comparison-label">Chrome Reference</div>
                    <img src="{}" alt="Chrome">
                </div>
"#, chrome_img));

        // Show first and last Azul screenshots
        if let Some((first_hash, _, _, _)) = diffs.first() {
            let first_img = format!("{}/{}_azul.webp", first_hash, test);
            html.push_str(&format!(r#"
                <div class="comparison-item">
                    <div class="comparison-label">Azul @ {}</div>
                    <img src="{}" alt="Azul">
                </div>
"#, first_hash, first_img));
        }

        if diffs.len() > 1 {
            if let Some((last_hash, _, _, _)) = diffs.last() {
                let last_img = format!("{}/{}_azul.webp", last_hash, test);
                html.push_str(&format!(r#"
                <div class="comparison-item">
                    <div class="comparison-label">Azul @ {}</div>
                    <img src="{}" alt="Azul">
                </div>
"#, last_hash, last_img));
            }
        }

        html.push_str("            </div>\n        </details>\n    </div>\n");
    }

    // Commit details
    html.push_str("<h2>📋 Commit Details</h2>\n");
    
    for (commit, status) in commit_data {
        let (status_class, status_text) = match status {
            CommitStatus::Success { screenshots } => ("status-ok", format!("✓ {} screenshots", screenshots.len())),
            CommitStatus::BuildFailed(_) => ("status-failed", "✗ Build failed".to_string()),
            CommitStatus::Pending => ("status-pending", "⏳ Pending".to_string()),
        };
        
        html.push_str(&format!(r#"
    <div class="test-section">
        <div class="test-name"><span class="commit-hash">{}</span> - {}</div>
        <p><strong>Date:</strong> {} | <strong>Author:</strong> {} | <strong>Status:</strong> <span class="{}">{}</span></p>
"#,
            commit.short_hash,
            commit.message.chars().take(60).collect::<String>(),
            commit.date.split_whitespace().next().unwrap_or(""),
            commit.author,
            status_class,
            status_text
        ));
        
        if let CommitStatus::BuildFailed(err) = status {
            html.push_str(&format!(
                "        <details><summary>View Error</summary><div class=\"build-error\">{}</div></details>\n",
                err.replace('<', "&lt;").replace('>', "&gt;")
            ));
        }
        
        html.push_str("    </div>\n");
    }

    html.push_str("</body>\n</html>\n");

    let report_path = regression_dir.join("index.html");
    fs::write(&report_path, html)?;
    
    println!("  Report generated: {}", report_path.display());
    
    Ok(())
}

/// Compare two images and return pixel difference
fn compare_images(path1: &Path, path2: &Path) -> anyhow::Result<i64> {
    let img1 = image::open(path1)?;
    let img2 = image::open(path2)?;
    
    let (w1, h1) = img1.dimensions();
    let (w2, h2) = img2.dimensions();
    
    if w1 != w2 || h1 != h2 {
        return Ok((w1 * h1) as i64);
    }
    
    let rgba1 = img1.to_rgba8();
    let rgba2 = img2.to_rgba8();
    
    let mut diff: i64 = 0;
    for (p1, p2) in rgba1.pixels().zip(rgba2.pixels()) {
        if p1 != p2 {
            diff += 1;
        }
    }
    
    Ok(diff)
}
