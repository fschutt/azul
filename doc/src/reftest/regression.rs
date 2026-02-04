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
    eprintln!("╔═══════════════════════════════════════════════════════════════╗");
    eprintln!("║           Azul Reftest Regression Statistics                  ║");
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    eprintln!();
    
    let regression_dir = config.output_dir.join("regression");
    
    if !regression_dir.exists() {
        eprintln!("No regression data found. Run 'debug-regression' first.");
        return Ok(());
    }
    
    // Collect all processed commits
    eprintln!("[1/2] Collecting commit data...");
    let commits = collect_processed_commits(&regression_dir)?;
    eprintln!("  Found {} processed commits", commits.len());
    
    // Generate TXT report to stdout
    eprintln!("[2/2] Generating diff report...");
    generate_diff_report(&commits, &config.azul_root)?;
    
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

/// Calculate the worst regressions by total diff delta
fn calculate_worst_regressions(
    commit_data: &[(CommitInfo, CommitStatus)],
    azul_root: &Path,
    top_n: usize,
) -> Vec<(String, String, i64, String)> {
    // (prev_hash, curr_hash, total_delta, message)
    let mut regressions: Vec<(String, String, i64, String)> = Vec::new();
    
    let mut sorted_commits: Vec<_> = commit_data.iter().collect();
    sorted_commits.sort_by(|a, b| b.0.date.cmp(&a.0.date));
    
    let mut prev_results: Option<serde_json::Value> = None;
    let mut prev_commit: Option<&CommitInfo> = None;
    
    for (commit, status) in sorted_commits.iter().rev() {
        if !matches!(status, CommitStatus::Success { .. }) {
            continue;
        }
        
        let results_path = azul_root
            .join("doc/target/reftest/regression")
            .join(&commit.short_hash)
            .join("results.json");
        
        if !results_path.exists() {
            continue;
        }
        
        let content = match fs::read_to_string(&results_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        
        let current: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };
        
        if let (Some(prev), Some(prev_c)) = (&prev_results, prev_commit) {
            let detailed = diff_results_detailed(prev, &current);
            
            // Sum up all positive deltas (regressions)
            let total_delta: i64 = detailed.iter()
                .filter(|c| c.status == "REGRESSED" || c.status == "BROKE")
                .map(|c| (c.curr_diff - c.prev_diff) as i64)
                .sum();
            
            if total_delta > 0 {
                regressions.push((
                    prev_c.short_hash.clone(),
                    commit.short_hash.clone(),
                    total_delta,
                    commit.message.clone(),
                ));
            }
        }
        
        prev_results = Some(current);
        prev_commit = Some(commit);
    }
    
    // Sort by total delta descending
    regressions.sort_by(|a, b| b.2.cmp(&a.2));
    regressions.truncate(top_n);
    regressions
}

/// Generate a diff report comparing render_warnings between commits
fn generate_diff_report(commit_data: &[(CommitInfo, CommitStatus)], azul_root: &Path) -> anyhow::Result<()> {
    // Sort commits by date (newest first)
    let mut sorted_commits: Vec<_> = commit_data.iter().collect();
    sorted_commits.sort_by(|a, b| b.0.date.cmp(&a.0.date));
    
    println!("# Azul Layout Regression Analysis");
    println!("# Generated: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    println!("# Commits analyzed: {}", sorted_commits.len());
    println!();
    
    // First pass: calculate worst regressions
    let worst_regressions = calculate_worst_regressions(commit_data, azul_root, 5);
    
    if !worst_regressions.is_empty() {
        println!("## WORST REGRESSIONS (Top {})", worst_regressions.len());
        println!();
        println!("These commits caused the largest total regression (sum of all diff deltas):");
        println!();
        for (i, (prev_hash, curr_hash, total_delta, message)) in worst_regressions.iter().enumerate() {
            println!("{}. {} -> {} (total delta: +{})", i + 1, prev_hash, curr_hash, total_delta);
            println!("   Message: {}", message);
        }
        println!();
    }
    
    println!("## PART 1: Summary of Changes (sorted by date, oldest first)");
    println!();
    
    // Collect all detailed comparisons for Part 2
    let mut all_detailed: Vec<(CommitInfo, CommitInfo, Vec<TestComparison>)> = Vec::new();
    
    // Load results.json for each commit and compare
    let mut prev_results: Option<serde_json::Value> = None;
    let mut prev_commit: Option<&CommitInfo> = None;
    
    // Process from oldest to newest for proper diffing
    for (commit, status) in sorted_commits.iter().rev() {
        if !matches!(status, CommitStatus::Success { .. }) {
            continue;
        }
        
        // Try to load results.json
        let results_path = azul_root
            .join("doc/target/reftest/regression")
            .join(&commit.short_hash)
            .join("results.json");
        
        if !results_path.exists() {
            continue;
        }
        
        let content = match fs::read_to_string(&results_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        
        let current: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };
        
        if let (Some(prev), Some(prev_c)) = (&prev_results, prev_commit) {
            // Compare with previous commit
            let diffs = diff_results(prev, &current);
            let detailed = diff_results_detailed(prev, &current);
            
            if !diffs.is_empty() {
                println!("═══════════════════════════════════════════════════════════════");
                println!("COMMIT: {} -> {}", prev_c.short_hash, commit.short_hash);
                println!("MESSAGE: {}", commit.message);
                println!("DATE: {}", commit.date);
                println!("AUTHOR: {}", commit.author);
                println!("───────────────────────────────────────────────────────────────");
                
                for diff in &diffs {
                    println!("{}", diff);
                }
                println!();
                
                if !detailed.is_empty() {
                    all_detailed.push((prev_c.clone(), (*commit).clone(), detailed));
                }
            }
        }
        
        prev_results = Some(current);
        prev_commit = Some(commit);
    }
    
    // Part 2: Detailed analysis with log diffs
    if !all_detailed.is_empty() {
        println!();
        println!("## PART 2: Detailed Regression Analysis");
        println!();
        println!("This section shows what changed in the layout engine output between commits.");
        println!();
        
        for (prev_c, curr_c, comparisons) in &all_detailed {
            // Only show regressions in detail
            let regressions: Vec<_> = comparisons.iter()
                .filter(|c| c.status == "REGRESSED" || c.status == "BROKE")
                .collect();
            
            if regressions.is_empty() {
                continue;
            }
            
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("REGRESSION: {} -> {}", prev_c.short_hash, curr_c.short_hash);
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!();
            println!("BEFORE ({}, {}):", prev_c.short_hash, prev_c.date.split_whitespace().take(2).collect::<Vec<_>>().join(" "));
            println!("  Message: {}", prev_c.message);
            println!();
            println!("AFTER ({}, {}):", curr_c.short_hash, curr_c.date.split_whitespace().take(2).collect::<Vec<_>>().join(" "));
            println!("  Message: {}", curr_c.message);
            println!();
            
            for comp in regressions {
                println!("┌─── TEST: {} [{}] ───────────────────────────────", comp.test_name, comp.status);
                println!("│");
                println!("│ DIFF COUNT: {} -> {} (delta: {:+})", comp.prev_diff, comp.curr_diff, comp.curr_diff - comp.prev_diff);
                println!("│");
                
                // Show solved_layout diff if different
                if comp.prev_solved_layout != comp.curr_solved_layout {
                    println!("│ SOLVED LAYOUT:");
                    println!("│   BEFORE: {}", comp.prev_solved_layout.lines().next().unwrap_or(""));
                    println!("│   AFTER:  {}", comp.curr_solved_layout.lines().next().unwrap_or(""));
                    println!("│");
                }
                
                // Show display list item count diff
                let prev_items = comp.prev_display_list.lines()
                    .find(|l| l.contains("Items:"))
                    .unwrap_or("");
                let curr_items = comp.curr_display_list.lines()
                    .find(|l| l.contains("Items:"))
                    .unwrap_or("");
                if prev_items != curr_items {
                    println!("│ DISPLAY LIST:");
                    println!("│   BEFORE: {}", prev_items.trim());
                    println!("│   AFTER:  {}", curr_items.trim());
                    println!("│");
                }
                
                // Show log changes
                if !comp.log_changes.is_empty() {
                    println!("│ LOG CHANGES (by source location):");
                    for change in &comp.log_changes {
                        println!("│   {}", change);
                    }
                    println!("│");
                }
                
                println!("└────────────────────────────────────────────────────────────────");
                println!();
            }
            
            // Show git diff for layout directory
            println!("GIT DIFF (layout/):");
            println!("```diff");
            let git_diff = std::process::Command::new("git")
                .args(["diff", "--stat", &format!("{}..{}", prev_c.short_hash, curr_c.short_hash), "--", "layout/"])
                .current_dir(azul_root)
                .output();
            
            if let Ok(output) = git_diff {
                let diff_stat = String::from_utf8_lossy(&output.stdout);
                if !diff_stat.trim().is_empty() {
                    println!("{}", diff_stat.trim());
                }
            }
            
            // Get full git diff
            let git_diff_full = std::process::Command::new("git")
                .args(["diff", &format!("{}..{}", prev_c.short_hash, curr_c.short_hash), "--", "layout/"])
                .current_dir(azul_root)
                .output();
            
            if let Ok(output) = git_diff_full {
                let diff_content = String::from_utf8_lossy(&output.stdout);
                print!("{}", diff_content);
            }
            println!("```");
            println!();
        }
    }
    
    Ok(())
}

/// Diff two results.json files and return a list of changes
fn diff_results(prev: &serde_json::Value, curr: &serde_json::Value) -> Vec<String> {
    let mut diffs = Vec::new();
    
    let prev_tests = prev.get("tests").and_then(|t| t.as_array());
    let curr_tests = curr.get("tests").and_then(|t| t.as_array());
    
    let (Some(prev_tests), Some(curr_tests)) = (prev_tests, curr_tests) else {
        return diffs;
    };
    
    // Build maps by test_name
    let prev_map: std::collections::HashMap<&str, &serde_json::Value> = prev_tests
        .iter()
        .filter_map(|t| t.get("test_name").and_then(|n| n.as_str()).map(|n| (n, t)))
        .collect();
    
    for curr_test in curr_tests {
        let Some(test_name) = curr_test.get("test_name").and_then(|n| n.as_str()) else {
            continue;
        };
        
        let curr_diff = curr_test.get("diff_count").and_then(|d| d.as_i64()).unwrap_or(0);
        let curr_passed = curr_test.get("passed").and_then(|p| p.as_bool()).unwrap_or(false);
        
        if let Some(prev_test) = prev_map.get(test_name) {
            let prev_diff = prev_test.get("diff_count").and_then(|d| d.as_i64()).unwrap_or(0);
            let prev_passed = prev_test.get("passed").and_then(|p| p.as_bool()).unwrap_or(false);
            
            // Check for pass/fail change or significant diff
            let has_change = prev_passed != curr_passed || (curr_diff - prev_diff).abs() > 1000;
            
            if has_change {
                let status = if prev_passed != curr_passed {
                    if curr_passed { "[FIXED]" } else { "[BROKE]" }
                } else if curr_diff < prev_diff {
                    "[IMPROVED]"
                } else {
                    "[REGRESSED]"
                };
                
                diffs.push(format!("  {} {} (diff: {} -> {}, delta: {:+})", 
                    status, test_name, prev_diff, curr_diff, curr_diff - prev_diff));
            }
        } else {
            // New test
            diffs.push(format!("  [NEW] {} (diff: {}, passed: {})", test_name, curr_diff, curr_passed));
        }
    }
    
    diffs
}

/// Detailed test comparison for prompt output
#[derive(Debug)]
struct TestComparison {
    test_name: String,
    status: String,
    prev_diff: i64,
    curr_diff: i64,
    prev_solved_layout: String,
    curr_solved_layout: String,
    prev_display_list: String,
    curr_display_list: String,
    log_changes: Vec<String>,
}

/// Generate detailed diff comparing render_warnings and other fields
fn diff_results_detailed(
    prev: &serde_json::Value,
    curr: &serde_json::Value,
) -> Vec<TestComparison> {
    let mut comparisons = Vec::new();
    
    let prev_tests = prev.get("tests").and_then(|t| t.as_array());
    let curr_tests = curr.get("tests").and_then(|t| t.as_array());
    
    let (Some(prev_tests), Some(curr_tests)) = (prev_tests, curr_tests) else {
        return comparisons;
    };
    
    // Build maps by test_name
    let prev_map: std::collections::HashMap<&str, &serde_json::Value> = prev_tests
        .iter()
        .filter_map(|t| t.get("test_name").and_then(|n| n.as_str()).map(|n| (n, t)))
        .collect();
    
    for curr_test in curr_tests {
        let Some(test_name) = curr_test.get("test_name").and_then(|n| n.as_str()) else {
            continue;
        };
        
        let curr_diff = curr_test.get("diff_count").and_then(|d| d.as_i64()).unwrap_or(0);
        let curr_passed = curr_test.get("passed").and_then(|p| p.as_bool()).unwrap_or(false);
        
        if let Some(prev_test) = prev_map.get(test_name) {
            let prev_diff = prev_test.get("diff_count").and_then(|d| d.as_i64()).unwrap_or(0);
            let prev_passed = prev_test.get("passed").and_then(|p| p.as_bool()).unwrap_or(false);
            
            // Only include significant changes
            let has_change = prev_passed != curr_passed || (curr_diff - prev_diff).abs() > 1000;
            if !has_change {
                continue;
            }
            
            let status = if prev_passed != curr_passed {
                if curr_passed { "FIXED".to_string() } else { "BROKE".to_string() }
            } else if curr_diff < prev_diff {
                "IMPROVED".to_string()
            } else {
                "REGRESSED".to_string()
            };
            
            // Extract fields
            let prev_solved = prev_test.get("solved_layout")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let curr_solved = curr_test.get("solved_layout")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            
            let prev_display = prev_test.get("display_list")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let curr_display = curr_test.get("display_list")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            
            // Compare render_warnings by source location
            let log_changes = diff_render_warnings(prev_test, curr_test);
            
            comparisons.push(TestComparison {
                test_name: test_name.to_string(),
                status,
                prev_diff,
                curr_diff,
                prev_solved_layout: prev_solved,
                curr_solved_layout: curr_solved,
                prev_display_list: prev_display,
                curr_display_list: curr_display,
                log_changes,
            });
        }
    }
    
    comparisons
}

/// Diff render_warnings by grouping them by source location
fn diff_render_warnings(prev_test: &serde_json::Value, curr_test: &serde_json::Value) -> Vec<String> {
    let mut changes = Vec::new();
    
    let prev_warnings = prev_test.get("render_warnings")
        .and_then(|w| w.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();
    
    let curr_warnings = curr_test.get("render_warnings")
        .and_then(|w| w.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();
    
    // Extract source locations and group messages
    fn extract_location(msg: &str) -> Option<&str> {
        // Pattern: (layout/src/solver3/mod.rs:123:45)
        if let Some(start) = msg.rfind('(') {
            if let Some(end) = msg.rfind(')') {
                if start < end {
                    return Some(&msg[start+1..end]);
                }
            }
        }
        None
    }
    
    // Group by location
    let mut prev_by_loc: std::collections::HashMap<String, Vec<&str>> = std::collections::HashMap::new();
    let mut curr_by_loc: std::collections::HashMap<String, Vec<&str>> = std::collections::HashMap::new();
    
    for msg in &prev_warnings {
        let loc = extract_location(msg).unwrap_or("unknown").to_string();
        prev_by_loc.entry(loc).or_default().push(msg);
    }
    
    for msg in &curr_warnings {
        let loc = extract_location(msg).unwrap_or("unknown").to_string();
        curr_by_loc.entry(loc).or_default().push(msg);
    }
    
    // Find differences
    let all_locs: std::collections::HashSet<&String> = prev_by_loc.keys().chain(curr_by_loc.keys()).collect();
    
    for loc in all_locs {
        let prev_count = prev_by_loc.get(loc).map(|v| v.len()).unwrap_or(0);
        let curr_count = curr_by_loc.get(loc).map(|v| v.len()).unwrap_or(0);
        
        if prev_count != curr_count {
            if prev_count == 0 {
                changes.push(format!("  + [NEW] {} ({}x)", loc, curr_count));
            } else if curr_count == 0 {
                changes.push(format!("  - [REMOVED] {} (was {}x)", loc, prev_count));
            } else {
                changes.push(format!("  ~ [CHANGED] {} ({}x -> {}x)", loc, prev_count, curr_count));
            }
        }
    }
    
    // Also check for specific value changes in key messages
    let key_patterns = ["font chains", "DOM has", "Display list with", "main_pen=", "content_box_height="];
    
    for pattern in key_patterns {
        let prev_match: Option<&str> = prev_warnings.iter().find(|m| m.contains(pattern)).copied();
        let curr_match: Option<&str> = curr_warnings.iter().find(|m| m.contains(pattern)).copied();
        
        if prev_match != curr_match {
            if let (Some(p), Some(c)) = (prev_match, curr_match) {
                // Extract the numeric value if possible
                let extract_num = |s: &str, pat: &str| -> Option<String> {
                    if let Some(idx) = s.find(pat) {
                        let rest = &s[idx + pat.len()..];
                        let num: String = rest.chars().take_while(|c| c.is_numeric() || *c == '.').collect();
                        if !num.is_empty() {
                            return Some(num);
                        }
                    }
                    None
                };
                
                if let (Some(pv), Some(cv)) = (extract_num(p, pattern), extract_num(c, pattern)) {
                    if pv != cv {
                        changes.push(format!("  VALUE: {} {} -> {}", pattern, pv, cv));
                    }
                }
            }
        }
    }
    
    changes
}

// Gemini API structures (same as debug.rs)
#[derive(serde::Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "generationConfig")]
    generation_config: GenerationConfig,
}

#[derive(serde::Serialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(serde::Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(serde::Serialize)]
struct GenerationConfig {
    #[serde(rename = "thinkingConfig")]
    thinking_config: ThinkingConfig,
}

#[derive(serde::Serialize)]
struct ThinkingConfig {
    #[serde(rename = "thinkingLevel")]
    thinking_level: String,
}

#[derive(serde::Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    error: Option<GeminiError>,
}

#[derive(serde::Deserialize)]
struct GeminiCandidate {
    content: GeminiResponseContent,
}

#[derive(serde::Deserialize)]
struct GeminiResponseContent {
    parts: Vec<GeminiResponsePart>,
}

#[derive(serde::Deserialize)]
struct GeminiResponsePart {
    text: Option<String>,
}

#[derive(serde::Deserialize)]
struct GeminiError {
    message: String,
}

/// Send regression analysis to Gemini API
pub fn run_statistics_send(config: RegressionConfig, output_path: Option<PathBuf>) -> anyhow::Result<()> {

    /// Gemini API URL (using gemini-3-pro-preview with thinking)
    const GEMINI_API_URL: &str =
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-3-pro-preview:generateContent";
        
    let regression_dir = config.output_dir.join("regression");
    
    if !regression_dir.exists() {
        eprintln!("No regression data found. Run 'debug-regression' first.");
        return Ok(());
    }
    
    // Load API key
    let api_key_path = config.azul_root.join("GEMINI_API_KEY.txt");
    let api_key = fs::read_to_string(&api_key_path)
        .map_err(|e| anyhow::anyhow!("Failed to load Gemini API key from {:?}: {}", api_key_path, e))?
        .trim()
        .to_string();
    eprintln!("[INFO] API key loaded");
    
    // Generate prompt on-the-fly
    eprintln!("[INFO] Generating prompt...");
    let prompt = generate_full_prompt(&config)?;
    eprintln!("[INFO] Prompt generated: {} chars", prompt.len());
    
    // Build request
    let request = GeminiRequest {
        contents: vec![GeminiContent {
            role: "user".to_string(),
            parts: vec![GeminiPart {
                text: prompt,
            }],
        }],
        generation_config: GenerationConfig {
            thinking_config: ThinkingConfig {
                thinking_level: "HIGH".to_string(),
            },
        },
    };
    
    let url = format!("{}?key={}", GEMINI_API_URL, api_key);
    
    eprintln!("[INFO] Sending to Gemini API (this may take a while)...");
    
    let response: GeminiResponse = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(600)) // 10 minute timeout
        .set("Content-Type", "application/json")
        .send_json(&request)
        .map_err(|e| anyhow::anyhow!("Gemini API request failed: {}", e))?
        .into_json()
        .map_err(|e| anyhow::anyhow!("Failed to parse Gemini response: {}", e))?;
    
    // Check for errors
    if let Some(error) = response.error {
        return Err(anyhow::anyhow!("Gemini API error: {}", error.message));
    }
    
    // Extract text from response
    let text = response
        .candidates
        .and_then(|c| c.into_iter().next())
        .and_then(|c| c.content.parts.into_iter().filter_map(|p| p.text).collect::<Vec<_>>().join("\n\n").into())
        .unwrap_or_default();
    
    // Save response
    let response_path = output_path.unwrap_or_else(|| config.azul_root.join("gemini_response.md"));
    fs::write(&response_path, &text)?;
    
    eprintln!("[INFO] Response saved to: {:?}", response_path);
    println!("{}", text);
    
    Ok(())
}

/// Generate the full prompt as a String (used by both run_statistics_prompt and run_statistics_send)
fn generate_full_prompt(config: &RegressionConfig) -> anyhow::Result<String> {
    let regression_dir = config.output_dir.join("regression");
    let commits = collect_processed_commits(&regression_dir)?;
    
    let mut prompt = String::new();
    
    // Header
    prompt.push_str("# Azul Layout Regression Analysis - Gemini Prompt\n\n");
    prompt.push_str("## Task\n\n");
    prompt.push_str("Analyze the following layout regression data and source code.\n");
    prompt.push_str("Identify which code changes caused regressions and suggest fixes.\n\n");
    
    // Regression diffs - capture output
    prompt.push_str("## Regression History\n\n");
    prompt.push_str("The following shows which commits changed test results:\n\n");
    prompt.push_str("```\n");
    prompt.push_str(&generate_diff_report_string(&commits, &config.azul_root)?);
    prompt.push_str("```\n\n");
    
    // Current source code
    prompt.push_str("## Current Source Code\n\n");
    
    // solver3 files
    let solver3_dir = config.azul_root.join("layout/src/solver3");
    if solver3_dir.exists() {
        prompt.push_str("### layout/src/solver3/\n\n");
        for entry in fs::read_dir(&solver3_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "rs").unwrap_or(false) {
                let filename = path.file_name().unwrap().to_string_lossy();
                prompt.push_str(&format!("#### {}\n\n", filename));
                prompt.push_str("```rust\n");
                if let Ok(content) = fs::read_to_string(&path) {
                    prompt.push_str(&content);
                }
                prompt.push_str("\n```\n\n");
            }
        }
    }
    
    // text3 files
    let text3_dir = config.azul_root.join("layout/src/text3");
    if text3_dir.exists() {
        prompt.push_str("### layout/src/text3/\n\n");
        for entry in fs::read_dir(&text3_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "rs").unwrap_or(false) {
                let filename = path.file_name().unwrap().to_string_lossy();
                prompt.push_str(&format!("#### {}\n\n", filename));
                prompt.push_str("```rust\n");
                if let Ok(content) = fs::read_to_string(&path) {
                    prompt.push_str(&content);
                }
                prompt.push_str("\n```\n\n");
            }
        }
    }
    
    // Instructions
    prompt.push_str("## Instructions\n\n");
    prompt.push_str("Based on the regression history and source code above:\n");
    prompt.push_str("1. Identify which commits caused the most significant regressions\n");
    prompt.push_str("2. Analyze the code changes that likely caused these issues\n");
    prompt.push_str("3. Suggest specific code fixes to restore correct behavior\n");
    prompt.push_str("4. Focus on the most impactful regressions first\n");
    
    Ok(prompt)
}

/// Generate diff report as a String (instead of printing to stdout)
fn generate_diff_report_string(commit_data: &[(CommitInfo, CommitStatus)], azul_root: &Path) -> anyhow::Result<String> {
    let mut output = String::new();
    
    // Sort commits by date (newest first)
    let mut sorted_commits: Vec<_> = commit_data.iter().collect();
    sorted_commits.sort_by(|a, b| b.0.date.cmp(&a.0.date));
    
    output.push_str(&format!("# Azul Layout Regression Analysis\n"));
    output.push_str(&format!("# Generated: {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));
    output.push_str(&format!("# Commits analyzed: {}\n\n", sorted_commits.len()));
    
    // First pass: calculate worst regressions
    let worst_regressions = calculate_worst_regressions(commit_data, azul_root, 5);
    
    if !worst_regressions.is_empty() {
        output.push_str(&format!("## WORST REGRESSIONS (Top {})\n\n", worst_regressions.len()));
        output.push_str("These commits caused the largest total regression (sum of all diff deltas):\n\n");
        for (i, (prev_hash, curr_hash, total_delta, message)) in worst_regressions.iter().enumerate() {
            output.push_str(&format!("{}. {} -> {} (total delta: +{})\n", i + 1, prev_hash, curr_hash, total_delta));
            output.push_str(&format!("   Message: {}\n", message));
        }
        output.push_str("\n");
    }
    
    output.push_str("## PART 1: Summary of Changes (sorted by date, oldest first)\n\n");
    
    // Collect all detailed comparisons for Part 2
    let mut all_detailed: Vec<(CommitInfo, CommitInfo, Vec<TestComparison>)> = Vec::new();
    
    // Load results.json for each commit and compare
    let mut prev_results: Option<serde_json::Value> = None;
    let mut prev_commit: Option<&CommitInfo> = None;
    
    // Process from oldest to newest for proper diffing
    for (commit, status) in sorted_commits.iter().rev() {
        if !matches!(status, CommitStatus::Success { .. }) {
            continue;
        }
        
        // Try to load results.json
        let results_path = azul_root
            .join("doc/target/reftest/regression")
            .join(&commit.short_hash)
            .join("results.json");
        
        if !results_path.exists() {
            continue;
        }
        
        let content = match fs::read_to_string(&results_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        
        let current: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };
        
        if let (Some(prev), Some(prev_c)) = (&prev_results, prev_commit) {
            // Compare with previous commit
            let diffs = diff_results(prev, &current);
            let detailed = diff_results_detailed(prev, &current);
            
            if !diffs.is_empty() {
                output.push_str("═══════════════════════════════════════════════════════════════\n");
                output.push_str(&format!("COMMIT: {} -> {}\n", prev_c.short_hash, commit.short_hash));
                output.push_str(&format!("MESSAGE: {}\n", commit.message));
                output.push_str(&format!("DATE: {}\n", commit.date));
                output.push_str(&format!("AUTHOR: {}\n", commit.author));
                output.push_str("───────────────────────────────────────────────────────────────\n");
                
                for diff in &diffs {
                    output.push_str(&format!("{}\n", diff));
                }
                output.push_str("\n");
                
                if !detailed.is_empty() {
                    all_detailed.push((prev_c.clone(), (*commit).clone(), detailed));
                }
            }
        }
        
        prev_results = Some(current);
        prev_commit = Some(commit);
    }
    
    // Part 2: Detailed analysis with log diffs
    if !all_detailed.is_empty() {
        output.push_str("\n## PART 2: Detailed Regression Analysis\n\n");
        output.push_str("This section shows what changed in the layout engine output between commits.\n\n");
        
        for (prev_c, curr_c, comparisons) in &all_detailed {
            // Only show regressions in detail
            let regressions: Vec<_> = comparisons.iter()
                .filter(|c| c.status == "REGRESSED" || c.status == "BROKE")
                .collect();
            
            if regressions.is_empty() {
                continue;
            }
            
            output.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
            output.push_str(&format!("REGRESSION: {} -> {}\n", prev_c.short_hash, curr_c.short_hash));
            output.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\n");
            output.push_str(&format!("BEFORE ({}, {}):\n", prev_c.short_hash, prev_c.date.split_whitespace().take(2).collect::<Vec<_>>().join(" ")));
            output.push_str(&format!("  Message: {}\n\n", prev_c.message));
            output.push_str(&format!("AFTER ({}, {}):\n", curr_c.short_hash, curr_c.date.split_whitespace().take(2).collect::<Vec<_>>().join(" ")));
            output.push_str(&format!("  Message: {}\n\n", curr_c.message));
            
            for comp in regressions {
                output.push_str(&format!("┌─── TEST: {} [{}] ───────────────────────────────\n", comp.test_name, comp.status));
                output.push_str("│\n");
                output.push_str(&format!("│ DIFF COUNT: {} -> {} (delta: {:+})\n", comp.prev_diff, comp.curr_diff, comp.curr_diff - comp.prev_diff));
                output.push_str("│\n");
                
                // Show solved_layout diff if different
                if comp.prev_solved_layout != comp.curr_solved_layout {
                    output.push_str("│ SOLVED LAYOUT:\n");
                    output.push_str(&format!("│   BEFORE: {}\n", comp.prev_solved_layout.lines().next().unwrap_or("")));
                    output.push_str(&format!("│   AFTER:  {}\n", comp.curr_solved_layout.lines().next().unwrap_or("")));
                    output.push_str("│\n");
                }
                
                // Show display list item count diff
                let prev_items = comp.prev_display_list.lines()
                    .find(|l| l.contains("Items:"))
                    .unwrap_or("");
                let curr_items = comp.curr_display_list.lines()
                    .find(|l| l.contains("Items:"))
                    .unwrap_or("");
                if prev_items != curr_items {
                    output.push_str("│ DISPLAY LIST:\n");
                    output.push_str(&format!("│   BEFORE: {}\n", prev_items.trim()));
                    output.push_str(&format!("│   AFTER:  {}\n", curr_items.trim()));
                    output.push_str("│\n");
                }
                
                // Show log changes
                if !comp.log_changes.is_empty() {
                    output.push_str("│ LOG CHANGES (by source location):\n");
                    for change in &comp.log_changes {
                        output.push_str(&format!("│   {}\n", change));
                    }
                    output.push_str("│\n");
                }
                
                output.push_str("└────────────────────────────────────────────────────────────────\n\n");
            }
            
            // Show git diff for layout directory
            output.push_str("GIT DIFF (layout/):\n");
            output.push_str("```diff\n");
            let git_diff = std::process::Command::new("git")
                .args(["diff", "--stat", &format!("{}..{}", prev_c.short_hash, curr_c.short_hash), "--", "layout/"])
                .current_dir(azul_root)
                .output();
            
            if let Ok(git_output) = git_diff {
                let diff_stat = String::from_utf8_lossy(&git_output.stdout);
                if !diff_stat.trim().is_empty() {
                    output.push_str(&format!("{}\n", diff_stat.trim()));
                }
            }
            
            // Get full git diff
            let git_diff_full = std::process::Command::new("git")
                .args(["diff", &format!("{}..{}", prev_c.short_hash, curr_c.short_hash), "--", "layout/"])
                .current_dir(azul_root)
                .output();
            
            if let Ok(git_output) = git_diff_full {
                let diff_content = String::from_utf8_lossy(&git_output.stdout);
                output.push_str(&diff_content);
            }
            output.push_str("```\n\n");
        }
    }
    
    Ok(output)
}

/// Generate a full prompt for Gemini with regression analysis and source code
pub fn run_statistics_prompt(config: RegressionConfig) -> anyhow::Result<()> {
    let regression_dir = config.output_dir.join("regression");
    
    if !regression_dir.exists() {
        eprintln!("No regression data found. Run 'debug-regression' first.");
        return Ok(());
    }
    
    // Generate and print the full prompt
    let prompt = generate_full_prompt(&config)?;
    println!("{}", prompt);
    
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
