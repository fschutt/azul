use std::{
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::Instant,
};

use azul_core::{
    dom::DomId,
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
    window::StringPairVec,
    xml::{get_html_node, DomXml, ComponentMap, XmlNode, XmlNodeChild},
};
use azul_css::{
    css::{Css, CssDeclaration},
    parser2::CssParseWarnMsgOwned,
    props::property::CssProperty,
    LayoutDebugMessageType,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    window_state::FullWindowState,
    xml::{domxml_from_str, parse_xml_string},
};
use base64::Engine;
use image::{self, GenericImageView};
use serde_derive::{Deserialize, Serialize};

pub mod debug;
pub mod regression;

pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;

#[derive(Debug, Serialize, Deserialize)]
pub struct TestResults {
    pub tests: Vec<EnhancedTestResult>,
    pub total_tests: usize,
    pub passed_tests: usize,
}

// let test_dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/working"));
// let output_dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/output"));

pub struct RunRefTestsConfig {
    pub test_dir: PathBuf,
    pub output_dir: PathBuf,
    pub output_filename: &'static str,
}

pub fn run_reftests(config: RunRefTestsConfig) -> anyhow::Result<()> {
    let RunRefTestsConfig {
        test_dir,
        output_dir,
        output_filename,
    } = config;

    // Create output directory if it doesn't exist
    fs::create_dir_all(&output_dir)?;

    println!("Looking for test files in {}", test_dir.display());

    // Find all XHTML files in the test directory (returns empty vec if dir doesn't exist)
    let test_files = find_test_files(&test_dir).unwrap_or_else(|e| {
        eprintln!("Warning: Could not read test directory: {}", e);
        Vec::new()
    });
    println!("Found {} test files", test_files.len());

    // Results to be collected for JSON
    let enhanced_results = Arc::new(Mutex::new(Vec::new()));

    // Get Chrome path
    let chrome_path = get_chrome_path();

    // Get Chrome version
    let chrome_version = get_chrome_version(&chrome_path);
    let is_chrome_installed = !chrome_version.contains("Unknown");

    // Current time
    let current_time = chrono::Local::now().format("%Y-%m-%d").to_string();

    // Git hash
    let git_hash = get_git_hash();

    if !is_chrome_installed {
        eprintln!("ERROR: Chrome not found. Tests will not be run.");
        eprintln!(
            "Please ensure Chrome is installed or set the CHROME environment variable to the \
             correct path."
        );

        // Generate empty report with just header information
        generate_enhanced_html_report(
            &output_dir.join(output_filename),
            &Vec::new(),
            &chrome_version,
            &current_time,
            &git_hash,
            is_chrome_installed,
        )?;

        return Ok(());
    }

    let _ = std::fs::create_dir_all(output_dir.join("reftest_img"));

    // Process tests
    test_files.iter().for_each(|test_file| {
        let test_name = test_file.file_stem().unwrap().to_string_lossy().to_string();
        println!("Processing test: {}", test_name);

        let chrome_img = output_dir
            .join("reftest_img")
            .join(format!("{}_chrome.webp", test_name));
        let chrome_layout_json = output_dir
            .join("reftest_img")
            .join(format!("{}_chrome_layout.json", test_name));
        let azul_img = output_dir
            .join("reftest_img")
            .join(format!("{}_azul.webp", test_name));

        // Generate Chrome reference and layout data if they don't exist
        let mut chrome_layout_data = String::new();
        if !chrome_img.exists() {
            println!("  Generating Chrome reference for {}", test_name);
            match generate_chrome_screenshot_with_debug(
                &chrome_path,
                test_file,
                &chrome_img,
                &chrome_layout_json,
                WIDTH,
                HEIGHT,
            ) {
                Ok(layout_data) => {
                    println!("  Chrome screenshot and layout data generated successfully");
                    chrome_layout_data = layout_data;
                }
                Err(e) => {
                    println!("  Failed to generate Chrome screenshot: {}", e);
                    return;
                }
            }
        } else {
            println!("  Using existing Chrome reference for {}", test_name);
            // Try to read existing layout data
            if let Ok(data) = fs::read_to_string(&chrome_layout_json) {
                chrome_layout_data = data;
            }
        }

        let (chrome_w, chrome_h) = match image::open(&chrome_img) {
            Ok(img) => img.dimensions(),
            Err(e) => {
                println!("  Failed to open Chrome image: {}", e);
                return;
            }
        };

        let dpi_factor = (chrome_w as f32 / WIDTH as f32).max(chrome_h as f32 / HEIGHT as f32);

        // Generate Azul rendering
        let mut debug_data = None;
        match generate_azul_rendering(test_file, &azul_img, dpi_factor) {
            Ok(mut data) => {
                println!("  Azul rendering generated successfully");
                // Add Chrome layout data to debug_data
                data.chrome_layout = chrome_layout_data.clone();
                debug_data = Some(data);
            }
            Err(e) => {
                println!("  Failed to generate Azul rendering: {}", e);
                return;
            }
        }

        // Compare images and generate diff
        match compare_images(&chrome_img, &azul_img) {
            Ok(diff_count) => {
                let passed = diff_count <= PASS_THRESHOLD_PIXELS; // 0.5% tolerance
                println!(
                    "  Comparison complete: {} differing pixels, test {}",
                    diff_count,
                    if passed { "PASSED" } else { "FAILED" }
                );

                // Read the original XHTML source
                let xhtml_source = match fs::read_to_string(test_file) {
                    Ok(content) => Some(content),
                    Err(_) => None,
                };

                // Store enhanced result with debug data
                let mut enhanced_results_vec = enhanced_results.lock().unwrap();
                enhanced_results_vec.push(EnhancedTestResult::from_debug_data(
                    test_name.to_string(),
                    diff_count,
                    passed,
                    debug_data.unwrap_or_default(),
                ));
            }
            Err(e) => {
                println!("  Failed to compare images: {}", e);
            }
        }
    });

    // Get enhanced results
    let final_enhanced_results = enhanced_results.lock().unwrap();
    let passed_tests = final_enhanced_results.iter().filter(|r| r.passed).count();

    // Generate enhanced HTML report with header information
    println!("Generating HTML report");
    generate_enhanced_html_report(
        &output_dir.join(output_filename),
        &final_enhanced_results,
        &chrome_version,
        &current_time,
        &git_hash,
        is_chrome_installed,
    )?;

    // Generate JSON results
    println!("Generating JSON results");
    generate_json_results(&output_dir, &*final_enhanced_results, passed_tests)?;

    println!(
        "Testing complete. Results saved to {}",
        output_dir.display()
    );
    println!("Passed: {}/{}", passed_tests, final_enhanced_results.len());

    Ok(())
}

/// Generate a reftest HTML page without running any tests.
/// If test_dir exists and contains tests, it will show "X tests found (not run)".
/// If test_dir doesn't exist or is empty, it will show "0 tests found".
///
/// This function also copies existing reftest results and images from the standard
/// reftest output directory (doc/target/reftest) if they exist.
pub fn generate_reftest_page(output_dir: &Path, test_dir: Option<&Path>) -> anyhow::Result<()> {
    fs::create_dir_all(output_dir)?;

    // Check if we have existing reftest results to copy
    // The reftest command outputs to doc/target/reftest, so look for results there
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let existing_reftest_dir = manifest_dir.join("target").join("reftest");
    let existing_results_json = existing_reftest_dir.join("results.json");
    let existing_index_html = existing_reftest_dir.join("index.html");
    let existing_reftest_img = existing_reftest_dir.join("reftest_img");

    // If we have existing results, copy them instead of generating empty page
    if existing_results_json.exists() && existing_index_html.exists() {
        println!("  Found existing reftest results, copying...");

        // Copy index.html
        fs::copy(&existing_index_html, output_dir.join("index.html"))?;
        println!("    Copied index.html");

        // Copy results.json
        fs::copy(&existing_results_json, output_dir.join("results.json"))?;
        println!("    Copied results.json");

        // Copy reftest_img directory if it exists
        if existing_reftest_img.exists() && existing_reftest_img.is_dir() {
            let dest_img_dir = output_dir.join("reftest_img");
            fs::create_dir_all(&dest_img_dir)?;

            let mut img_count = 0;
            for entry in fs::read_dir(&existing_reftest_img)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let filename = path.file_name().unwrap();
                    fs::copy(&path, dest_img_dir.join(filename))?;
                    img_count += 1;
                }
            }
            println!("    Copied {} reftest images", img_count);
        }

        // Also copy to reftest.html in parent directory
        if output_dir
            .file_name()
            .map(|n| n == "reftest")
            .unwrap_or(false)
        {
            if let Some(parent) = output_dir.parent() {
                fs::copy(output_dir.join("index.html"), parent.join("reftest.html"))?;
            }
        }

        println!("  [OK] Reftest results copied from existing run");
        return Ok(());
    }

    // No existing results, generate placeholder page
    // Count tests if directory provided
    let test_count = test_dir
        .and_then(|dir| find_test_files(dir).ok())
        .map(|files| files.len())
        .unwrap_or(0);

    // Get Chrome version
    let chrome_path = get_chrome_path();
    let chrome_version = get_chrome_version(&chrome_path);
    let is_chrome_installed = !chrome_version.contains("Unknown");

    // Current time
    let current_time = chrono::Local::now().format("%Y-%m-%d").to_string();

    // Git hash
    let git_hash = get_git_hash();

    // Generate report with empty results
    generate_enhanced_html_report(
        &output_dir.join("index.html"),
        &Vec::new(),
        &chrome_version,
        &current_time,
        &git_hash,
        is_chrome_installed,
    )?;

    // Also copy to reftest.html in parent directory if output_dir is named "reftest"
    if output_dir
        .file_name()
        .map(|n| n == "reftest")
        .unwrap_or(false)
    {
        if let Some(parent) = output_dir.parent() {
            fs::copy(output_dir.join("index.html"), parent.join("reftest.html"))?;
        }
    }

    println!(
        "Reftest page generated at {} ({} tests found, not run)",
        output_dir.join("index.html").display(),
        test_count
    );

    Ok(())
}

/// Threshold for passing: 0.5% of total pixels (1920 * 1080 = 2,073,600)
pub const PASS_THRESHOLD_PIXELS: usize = (1920 * 1080) / 200;

/// Run a single reftest and generate HTML report
pub fn run_single_reftest(test_name: &str, config: RunRefTestsConfig) -> anyhow::Result<()> {
    let RunRefTestsConfig {
        test_dir,
        output_dir,
        output_filename,
    } = config;

    // Create output directory
    fs::create_dir_all(&output_dir)?;
    let _ = fs::create_dir_all(output_dir.join("reftest_img"));

    // Find the test file
    let test_file = find_test_file_by_name(test_name, &test_dir)?;
    println!("Found test file: {}", test_file.display());

    // Get Chrome info
    let chrome_path = get_chrome_path();
    let chrome_version = get_chrome_version(&chrome_path);
    let is_chrome_installed = !chrome_version.contains("Unknown");
    let current_time = chrono::Local::now().format("%Y-%m-%d").to_string();
    let git_hash = get_git_hash();

    if !is_chrome_installed {
        return Err(anyhow::anyhow!("Chrome not found"));
    }

    // Generate screenshots
    let chrome_img = output_dir
        .join("reftest_img")
        .join(format!("{}_chrome.webp", test_name));
    let chrome_layout_json = output_dir
        .join("reftest_img")
        .join(format!("{}_chrome_layout.json", test_name));
    let azul_img = output_dir
        .join("reftest_img")
        .join(format!("{}_azul.webp", test_name));

    // Generate Chrome reference
    println!("Generating Chrome reference...");
    let chrome_layout_data = generate_chrome_screenshot_with_debug(
        &chrome_path,
        &test_file,
        &chrome_img,
        &chrome_layout_json,
        WIDTH,
        HEIGHT,
    )?;

    let (chrome_w, chrome_h) = image::open(&chrome_img)?.dimensions();
    let dpi_factor = (chrome_w as f32 / WIDTH as f32).max(chrome_h as f32 / HEIGHT as f32);

    // Generate Azul rendering
    println!("Generating Azul rendering...");
    let mut debug_data = generate_azul_rendering(&test_file, &azul_img, dpi_factor)?;
    debug_data.chrome_layout = chrome_layout_data;

    // Compare images
    let diff_count = compare_images(&chrome_img, &azul_img)?;
    let percentage = (diff_count as f64 / (1920.0 * 1080.0)) * 100.0;
    let passed = diff_count <= PASS_THRESHOLD_PIXELS;

    println!(
        "Comparison: {} pixels different ({:.3}%), test {}",
        diff_count,
        percentage,
        if passed {
            "PASSED [ OK ]"
        } else {
            "FAILED [ ERROR ]"
        }
    );

    // Read XHTML source
    let xhtml_source = fs::read_to_string(&test_file).ok();

    // Create result
    let result =
        EnhancedTestResult::from_debug_data(test_name.to_string(), diff_count, passed, debug_data);

    // Generate HTML report
    generate_enhanced_html_report(
        &output_dir.join(output_filename),
        &vec![result],
        &chrome_version,
        &current_time,
        &git_hash,
        is_chrome_installed,
    )?;

    Ok(())
}

/// Find a test file by name (tries .xht, .xhtml, .html extensions)
fn find_test_file_by_name(test_name: &str, test_dir: &Path) -> anyhow::Result<PathBuf> {
    for ext in &["xht", "xhtml", "html"] {
        for entry in walkdir::WalkDir::new(test_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let file_ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if file_stem == test_name && file_ext == *ext {
                    return Ok(path.to_path_buf());
                }
            }
        }
    }
    Err(anyhow::anyhow!("Test file not found for: {}", test_name))
}

/// Runs a single reftest and prints all debug information to stdout.
/// Designed for headless inspection by an LLM.
pub fn run_single_reftest_headless(
    test_name: &str,
    test_dir: &Path,
    output_dir: &Path,
) -> anyhow::Result<()> {
    // Create output directory if it doesn't exist
    fs::create_dir_all(output_dir)?;

    let test_file = test_dir.join(format!("{}.xht", test_name));
    if !test_file.exists() {
        return Err(anyhow::anyhow!(
            "Test file not found: {}",
            test_file.display()
        ));
    }

    println!("Processing test: {}", test_name);

    // Get Chrome path and check if installed
    let chrome_path = get_chrome_path();
    if get_chrome_version(&chrome_path).contains("Unknown") {
        return Err(anyhow::anyhow!(
            "Google Chrome not found. Please install it or set the CHROME env var."
        ));
    }

    let chrome_img_path = output_dir.join(format!("{}_chrome.webp", test_name));
    let azul_img_path = output_dir.join(format!("{}_azul.webp", test_name));
    let chrome_layout_json_path = output_dir.join(format!("{}_chrome_layout.json", test_name));

    // 1. Generate Chrome screenshot and layout data
    println!("  Generating Chrome reference and layout data...");
    let chrome_layout_data = generate_chrome_screenshot_with_debug(
        &chrome_path,
        &test_file,
        &chrome_img_path,
        &chrome_layout_json_path,
        WIDTH,
        HEIGHT,
    )?;
    println!(
        "  - Chrome screenshot saved to {}",
        chrome_img_path.display()
    );
    println!(
        "  - Chrome layout data saved to {}",
        chrome_layout_json_path.display()
    );

    let (chrome_w, chrome_h) = image::open(&chrome_img_path)?.dimensions();
    let dpi_factor = (chrome_w as f32 / WIDTH as f32).max(chrome_h as f32 / HEIGHT as f32);

    // 2. Generate Azul rendering and collect debug data
    println!("  Generating Azul rendering...");
    let mut debug_data = generate_azul_rendering(&test_file, &azul_img_path, dpi_factor)?;
    debug_data.chrome_layout = chrome_layout_data;
    println!("  - Azul rendering saved to {}", azul_img_path.display());

    // 3. Compare images
    println!("  Comparing images...");
    match compare_images(&chrome_img_path, &azul_img_path) {
        Ok(diff_count) => {
            let passed = diff_count <= PASS_THRESHOLD_PIXELS;
            println!("\n--- COMPARISON RESULT ---");
            println!("Test Name: {}", test_name);
            println!("Result: {}", if passed { "PASSED" } else { "FAILED" });
            println!("Differing Pixels: {}", diff_count);
            println!("-------------------------\n");
        }
        Err(e) => {
            eprintln!("\n--- COMPARISON FAILED ---");
            eprintln!("Error during image comparison: {}", e);
            println!("-------------------------\n");
        }
    }

    // 4. Print all collected debug data to stdout (including Chrome layout)
    println!("\n--- DEBUG INFORMATION ---");
    println!("{}", debug_data.format());

    // Print Chrome layout data separately for easier parsing
    if !debug_data.chrome_layout.is_empty() {
        println!("\n--- CHROME LAYOUT DATA ---");
        println!("{}", debug_data.chrome_layout);
        println!("--- END CHROME LAYOUT DATA ---");
    }

    println!("--- END DEBUG INFORMATION ---\n");

    Ok(())
}

fn find_test_files(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut test_files = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().map_or(false, |ext| ext == "xht") {
            test_files.push(path);
        }
    }

    Ok(test_files)
}

pub fn get_chrome_path() -> String {
    // Check for environment variable override first
    if let Ok(chrome_path) = env::var("CHROME") {
        if !chrome_path.is_empty() {
            return chrome_path;
        }
    }

    // Check platform-specific default locations
    #[cfg(target_os = "macos")]
    {
        let default_path = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
        if Path::new(default_path).exists() {
            return default_path.to_string();
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Common Linux Chrome paths
        for path in &[
            "/usr/bin/google-chrome",
            "/usr/bin/chromium-browser",
            "/usr/bin/chromium",
        ] {
            if Path::new(path).exists() {
                return path.to_string();
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Check Program Files locations
        let program_files =
            env::var("PROGRAMFILES").unwrap_or_else(|_| "C:\\Program Files".to_string());
        let x86_program_files =
            env::var("PROGRAMFILES(X86)").unwrap_or_else(|_| "C:\\Program Files (x86)".to_string());

        let chrome_paths = [
            format!("{}\\Google\\Chrome\\Application\\chrome.exe", program_files),
            format!(
                "{}\\Google\\Chrome\\Application\\chrome.exe",
                x86_program_files
            ),
        ];

        for path in &chrome_paths {
            if Path::new(path).exists() {
                return path.to_string();
            }
        }
    }

    // Default to just "chrome" and let the system resolve it
    "chrome".to_string()
}

fn get_chrome_version(chrome_path: &str) -> String {
    match Command::new(chrome_path).arg("--version").output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "Unknown".to_string(),
    }
}

fn get_git_hash() -> String {
    // Try using git command first
    let git_result = Command::new("git").args(["rev-parse", "HEAD"]).output();
    if let Ok(output) = git_result {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string()
                .chars()
                .take(8)
                .collect();
        }
    }

    // Fall back to reading .git/HEAD
    if let Ok(head) = std::fs::read_to_string(".git/HEAD") {
        if head.starts_with("ref: ") {
            let ref_path = head.trim_start_matches("ref: ").trim();
            if let Ok(hash) = std::fs::read_to_string(format!(".git/{}", ref_path)) {
                return hash.trim().to_string().chars().take(8).collect();
            }
        }
    }

    "Unknown".to_string()
}

/// JavaScript to extract layout information from Chrome
const CHROME_LAYOUT_EXTRACTOR_JS: &str = include_str!("./chrome_layout_extractor.js");

fn generate_chrome_screenshot(
    chrome_path: &str,
    test_file: &Path,
    output_file: &Path,
    width: u32,
    height: u32,
) -> anyhow::Result<()> {
    let canonical_path = test_file.canonicalize()?;

    let status = Command::new(chrome_path)
        .arg("--headless")
        .arg(format!("--screenshot={}", output_file.display()))
        .arg(format!("--window-size={},{}", width, height))
        .arg(format!("file://{}", canonical_path.display()))
        .status()?;

    if !status.success() {
        return Err(anyhow::anyhow!("Chrome exited with status {}", status));
    }

    Ok(())
}

/// Generate Chrome screenshot and extract layout debug information
pub fn generate_chrome_screenshot_with_debug(
    chrome_path: &str,
    test_file: &Path,
    output_file: &Path,
    layout_output_file: &Path,
    width: u32,
    height: u32,
) -> anyhow::Result<String> {
    let canonical_path = test_file.canonicalize()?;

    // First, take the screenshot
    let status = Command::new(chrome_path)
        .arg("--headless")
        .arg(format!("--screenshot={}", output_file.display()))
        .arg(format!("--window-size={},{}", width, height))
        .arg(format!("file://{}", canonical_path.display()))
        .status()?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "Chrome screenshot exited with status {}",
            status
        ));
    }

    // Extract layout information using dump-dom with inline script
    // The script must execute synchronously during HTML parsing
    let layout_json = generate_simple_layout_info(chrome_path, test_file, width, height)
        .unwrap_or_else(|e| format!("{{\"error\": \"Layout extraction failed: {}\"}}", e));

    // Save layout JSON to file
    fs::write(layout_output_file, &layout_json)?;

    Ok(layout_json)
}

/// Layout extraction using Chrome's dump-dom with inline script
fn generate_simple_layout_info(
    chrome_path: &str,
    test_file: &Path,
    width: u32,
    height: u32,
) -> anyhow::Result<String> {
    // Use a script that runs synchronously and stores result in DOM
    // The key is to NOT use any async operations
    let simple_script = r#"(function() {
    var result = { 
        timestamp: new Date().toISOString(),
        viewport: { width: window.innerWidth, height: window.innerHeight },
        elements: [] 
    };
    var els = document.querySelectorAll('body, body *');
    for (var i = 0; i < els.length; i++) {
        var el = els[i];
        if (el.tagName === 'SCRIPT' || el.tagName === 'STYLE' || el.tagName === 'PRE' && el.id === 'azul-layout-data') continue;
        var rect = el.getBoundingClientRect();
        var cs = window.getComputedStyle(el);
        result.elements.push({
            i: i,
            tag: el.tagName.toLowerCase(),
            id: el.id || null,
            cls: el.className || null,
            bounds: { x: Math.round(rect.x), y: Math.round(rect.y), w: Math.round(rect.width), h: Math.round(rect.height) },
            margin: { t: parseFloat(cs.marginTop)||0, r: parseFloat(cs.marginRight)||0, b: parseFloat(cs.marginBottom)||0, l: parseFloat(cs.marginLeft)||0 },
            padding: { t: parseFloat(cs.paddingTop)||0, r: parseFloat(cs.paddingRight)||0, b: parseFloat(cs.paddingBottom)||0, l: parseFloat(cs.paddingLeft)||0 },
            border: { t: parseFloat(cs.borderTopWidth)||0, r: parseFloat(cs.borderRightWidth)||0, b: parseFloat(cs.borderBottomWidth)||0, l: parseFloat(cs.borderLeftWidth)||0 },
            display: cs.display,
            position: cs.position,
            float: cs.cssFloat || cs.float || 'none',
            clear: cs.clear
        });
    }
    return JSON.stringify(result, null, 2);
})()"#;

    // Create temp HTML with the script embedded
    let temp_dir = std::env::temp_dir();
    let temp_html = temp_dir.join("chrome_simple_layout.html");
    let original_content = fs::read_to_string(test_file)?;

    // Inject script that writes to a pre element SYNCHRONOUSLY
    let extraction_html = if original_content.contains("</body>") {
        original_content.replace(
            "</body>",
            &format!(
                r#"<pre id="azul-layout-data" style="display:none"></pre>
<script>document.getElementById('azul-layout-data').textContent = {};</script>
</body>"#,
                simple_script
            ),
        )
    } else {
        format!(
            r#"<!DOCTYPE html><html><head></head><body>
{}
<pre id="azul-layout-data" style="display:none"></pre>
<script>document.getElementById('azul-layout-data').textContent = {};</script>
</body></html>"#,
            original_content, simple_script
        )
    };

    fs::write(&temp_html, &extraction_html)?;

    // Use dump-dom - the script runs synchronously during parsing
    let output = Command::new(chrome_path)
        .arg("--headless")
        .arg("--disable-gpu")
        .arg("--dump-dom")
        .arg(format!("--window-size={},{}", width, height))
        .arg(format!("file://{}", temp_html.display()))
        .output()?;

    let _ = fs::remove_file(&temp_html);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Chrome dump-dom failed: {}", stderr));
    }

    let dom_output = String::from_utf8_lossy(&output.stdout);

    // Extract from pre element
    if let Some(start) = dom_output.find("<pre id=\"azul-layout-data\"") {
        if let Some(content_start) = dom_output[start..].find('>') {
            let after_tag = &dom_output[start + content_start + 1..];
            if let Some(end) = after_tag.find("</pre>") {
                let json_content = &after_tag[..end];
                let decoded = html_decode(json_content);
                if decoded.starts_with('{') && decoded.ends_with('}') {
                    return Ok(decoded);
                } else {
                    return Ok(format!(
                        "{{\"error\": \"Invalid JSON\", \"content\": {:?}}}",
                        decoded.chars().take(200).collect::<String>()
                    ));
                }
            }
        }
    }

    Ok(format!(
        "{{\"error\": \"Could not find layout data element\", \"dom_len\": {}}}",
        dom_output.len()
    ))
}

/// Decode basic HTML entities
fn html_decode(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

pub fn generate_azul_rendering(
    test_file: &Path,
    output_file: &Path,
    dpi_factor: f32,
) -> anyhow::Result<DebugData> {
    let start = Instant::now();

    // Read XML content
    let xml_content = fs::read_to_string(test_file)?;

    // Initialize debug data collector
    let mut debug_collector = DebugDataCollector::new(xml_content.clone());

    // Parse XML to DomXml
    let (dom_xml, metadata, xml) =
        EnhancedXmlParser::parse_test_file(test_file).map_err(|e| anyhow::anyhow!("{e}"))?;

    // Extract styling and metadata
    debug_collector.set_metadata(
        metadata.title,
        metadata.assert_content,
        metadata.help_link,
        metadata.flags,
        metadata.author,
    );

    // Format XML structure for debugging
    let mut xml_formatted = String::new();
    for node in xml {
        if let XmlNodeChild::Element(elem) = node {
            xml_formatted.push_str(&EnhancedXmlParser::format_xml_for_display(&elem, 0));
        }
    }

    // Extract and analyze CSS
    let mut css_collector = CssWarningCollector::new();
    if let Ok(css_text) = extract_css_from_xml(&xml_content) {
        let parsed_css = css_collector.parse_css(&css_text);
        let css_stats = CssStats::analyze(&parsed_css);
        debug_collector.set_css_debug_info(css_collector.format_warnings(), css_stats.format());
    }

    // Store DOM information
    debug_collector.set_dom_debug_info(
        xml_formatted,
        dom_xml.parsed_dom.get_html_string("", "", true),
    );

    let xml_formatting_time = start.elapsed().as_millis() as u64;

    // Generate and save PNG
    let (warnings, layout_time_ms, render_time_ms) = styled_dom_to_png_with_debug(
        &dom_xml.parsed_dom,
        output_file,
        WIDTH,
        HEIGHT,
        dpi_factor,
        &mut debug_collector,
    )?;

    // Record rendering time
    debug_collector.set_render_info(
        xml_formatting_time,
        layout_time_ms,
        render_time_ms,
        warnings,
    );

    // Save debug data to JSON
    let debug_data = debug_collector.get_data();

    Ok(debug_data)
}

fn styled_dom_to_png_with_debug(
    styled_dom: &StyledDom,
    output_file: &Path,
    width: u32,
    height: u32,
    dpi_factor: f32,
    debug_collector: &mut DebugDataCollector,
) -> anyhow::Result<(Vec<String>, u64, u64)> {
    let start_time_layout = std::time::Instant::now();

    // Create window state for layout
    let mut fake_window_state = FullWindowState::default();
    fake_window_state.size.dimensions = LogicalSize {
        width: width as f32,
        height: height as f32,
    };
    fake_window_state.size.dpi = (96.0 * dpi_factor) as u32;

    // Create resources for layout
    let mut renderer_resources = azul_core::resources::RendererResources::default();

    // Solve layout with debug information (solver3)
    let (display_list, debug_msg, layout_window) = solve_layout_with_debug(
        styled_dom.clone(),
        &fake_window_state,
        &mut renderer_resources,
        debug_collector,
    )?;

    // Capture display list for debugging
    let display_list_debug = format_display_list_for_debug_solver3(&display_list);
    debug_collector.set_display_list_debug_info(display_list_debug);

    let layout_time_ms = start_time_layout.elapsed().as_millis() as u64;

    let start_time_render = std::time::Instant::now();

    // Render the display list using the new cpurender module with FontManager
    let pixmap = azul_layout::cpurender::render_with_font_manager(
        &display_list,
        &renderer_resources,
        &layout_window.font_manager,
        azul_layout::cpurender::RenderOptions {
            width: width as f32,
            height: height as f32,
            dpi_factor,
        },
    )
    .map_err(|e| anyhow::anyhow!("Rendering failed: {}", e))?;

    let rendering_time_ms = start_time_render.elapsed().as_millis() as u64;

    // Convert pixmap to image bytes
    let pixmap_data = pixmap.data();

    // Use image crate to save webp image
    let rgba = image::RgbaImage::from_raw(pixmap.width(), pixmap.height(), pixmap_data.to_vec())
        .ok_or(anyhow::anyhow!("Failed to create image from pixmap data"))?;

    // Save as WebP with lossless quality
    let mut webp_data = Vec::new();
    let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut webp_data);
    encoder.encode(
        &rgba.into_raw(),
        pixmap.width(),
        pixmap.height(),
        image::ColorType::Rgba8.into(),
    )?;

    // Write the WebP data to file
    std::fs::write(output_file, webp_data)?;

    Ok((debug_msg, layout_time_ms, rendering_time_ms))
}

#[derive(Debug, Copy, Clone)]
pub struct Options {
    /// matching threshold (0 to 1); smaller is more sensitive
    pub threshold: f64,
    /// whether to skip anti-aliasing detection
    pub include_aa: bool,
    /// opacity of original image in diff output
    pub alpha: f64,
    /// color of anti-aliased pixels in diff output
    pub aa_color: [u8; 4],
    /// color of different pixels in diff output
    pub diff_color: [u8; 4],
    /// whether to detect dark on light differences between img1 and img2 and set an alternative
    /// color to differentiate between the two
    pub diff_color_alt: Option<[u8; 4]>,
    /// draw the diff over a transparent background (a mask)
    pub diff_mask: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            threshold: 0.1,
            include_aa: false,
            alpha: 0.1,
            aa_color: [255, 255, 0, 255],
            diff_color: [255, 0, 0, 255],
            diff_color_alt: None,
            diff_mask: false,
        }
    }
}

/// Helper function to determine if two pixels are similar enough (for anti-aliasing)
fn pixels_similar(p1: &image::Rgba<u8>, p2: &image::Rgba<u8>, threshold: f64) -> bool {
    // Skip fully transparent pixels
    if p1[3] == 0 && p2[3] == 0 {
        return true;
    }

    // Calculate color distance, accounting for alpha
    let delta_squared = (0..3)
        .map(|i| {
            let d = (p1[i] as f64 / 255.0) - (p2[i] as f64 / 255.0);
            d * d
        })
        .sum::<f64>();

    // Calculate alpha distance
    let alpha_delta = ((p1[3] as f64 / 255.0) - (p2[3] as f64 / 255.0)).abs();

    // Return true if both color and alpha differences are within threshold
    delta_squared < threshold * threshold && alpha_delta < threshold
}

pub fn compare_images(chrome_img_path: &Path, azul_img_path: &Path) -> anyhow::Result<usize> {
    println!(
        "  Comparing images: {} vs {}",
        chrome_img_path.display(),
        azul_img_path.display()
    );

    // Load images
    let chrome_img = image::open(chrome_img_path)?;
    let azul_img = image::open(azul_img_path)?;

    // Convert images to RGBA8 for pixel-by-pixel comparison
    let chrome_rgba = chrome_img.to_rgba8();
    let azul_rgba = azul_img.to_rgba8();

    // Check dimensions
    if chrome_rgba.dimensions() != azul_rgba.dimensions() {
        return Err(anyhow::anyhow!(
            "Image dimensions don't match: {:?} vs {:?}",
            chrome_rgba.dimensions(),
            azul_rgba.dimensions()
        ));
    }

    let (width, height) = chrome_rgba.dimensions();
    let mut diff_count = 0;

    // Perform direct byte comparison with anti-aliasing allowance
    for y in 0..height {
        for x in 0..width {
            let chrome_pixel = chrome_rgba.get_pixel(x, y);
            let azul_pixel = azul_rgba.get_pixel(x, y);

            // Compare pixels with some tolerance for anti-aliasing
            if !pixels_similar(chrome_pixel, azul_pixel, 0.1) {
                diff_count += 1;
            }
        }
    }

    Ok(diff_count)
}

fn generate_json_results(
    output_dir: &Path,
    results: &[EnhancedTestResult],
    passed_tests: usize,
) -> anyhow::Result<()> {
    let json_path = output_dir.join("results.json");
    let mut file = File::create(&json_path)?;

    let test_results = TestResults {
        tests: results.to_vec(),
        total_tests: results.len(),
        passed_tests,
    };

    let json = serde_json::to_string_pretty(&test_results)?;
    file.write_all(json.as_bytes())?;

    println!("JSON results saved to {}", json_path.display());

    Ok(())
}

/// Metadata extracted from a test file
#[derive(Debug, Clone, Default)]
pub struct TestMetadata {
    pub title: String,
    pub assert_content: String,
    pub help_link: String,
    pub flags: String,
    pub author: String,
}

/// Enhanced XML parser that extracts metadata from test files
pub struct EnhancedXmlParser;

impl EnhancedXmlParser {
    /// Parse an XHTML file and extract both the DOM and metadata
    pub fn parse_test_file(
        file_path: &Path,
    ) -> Result<(DomXml, TestMetadata, Vec<XmlNodeChild>), String> {
        // Read file content
        let xml_content = match fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(e) => return Err(format!("Error reading file: {}", e)),
        };

        // Parse XML
        let parsed_xml = match parse_xml_string(&xml_content) {
            Ok(nodes) => nodes,
            Err(e) => return Err(format!("XML parse error: {}", e)),
        };

        // Extract metadata
        let metadata = Self::extract_metadata(&parsed_xml);

        // Parse to DOM
        let dom = domxml_from_str(&xml_content, &ComponentMap::with_builtin());

        Ok((dom, metadata, parsed_xml))
    }

    /// Extract metadata from parsed XML nodes
    pub fn extract_metadata(nodes: &[XmlNodeChild]) -> TestMetadata {
        let mut metadata = TestMetadata::default();

        // Find the <html> node
        if let Ok(html_node) = get_html_node(nodes) {
            // Look for <head> node
            for child in html_node.children.as_ref() {
                let elem = match child {
                    XmlNodeChild::Element(e) => e,
                    XmlNodeChild::Text(_) => continue,
                };
                if elem.node_type.as_str().to_lowercase() == "head" {
                    Self::extract_head_metadata(elem, &mut metadata);
                }
            }
        }

        metadata
    }

    /// Extract metadata from the <head> node
    fn extract_head_metadata(head_node: &XmlNode, metadata: &mut TestMetadata) {
        for child in head_node.children.as_ref() {
            let elem = match child {
                XmlNodeChild::Element(e) => e,
                XmlNodeChild::Text(_) => continue, // Skip text nodes
            };

            match elem.node_type.as_str().to_lowercase().as_str() {
                "title" => {
                    // Get text from children
                    for title_child in elem.children.as_ref() {
                        if let XmlNodeChild::Text(text) = title_child {
                            metadata.title = text.as_str().to_string();
                            break;
                        }
                    }
                }
                "meta" => {
                    // Handle meta tags
                    let name = Self::get_attribute_value(&elem.attributes, "name");
                    let content = Self::get_attribute_value(&elem.attributes, "content");

                    if let (Some(name), Some(content)) = (name, content) {
                        match name.as_str() {
                            "assert" => metadata.assert_content = content,
                            "flags" => metadata.flags = content,
                            _ => {} // Ignore other meta tags
                        }
                    }
                }
                "link" => {
                    // Handle link tags
                    let rel = Self::get_attribute_value(&elem.attributes, "rel");

                    if let Some(rel) = rel {
                        match rel.as_str() {
                            "help" => {
                                if let Some(href) =
                                    Self::get_attribute_value(&elem.attributes, "href")
                                {
                                    metadata.help_link = href;
                                }
                            }
                            "author" => {
                                if let Some(title) =
                                    Self::get_attribute_value(&elem.attributes, "title")
                                {
                                    metadata.author = title;
                                }
                            }
                            _ => {} // Ignore other link types
                        }
                    }
                }
                _ => {} // Ignore other head elements
            }
        }
    }

    /// Get attribute value by name from attributes list
    fn get_attribute_value(attributes: &StringPairVec, name: &str) -> Option<String> {
        for attr in attributes.as_ref() {
            if attr.key.as_str() == name {
                return Some(attr.value.as_str().to_string());
            }
        }
        None
    }

    /// Format XML node for debugging display
    pub fn format_xml_for_display(node: &XmlNode, indent: usize) -> String {
        let indent_str = " ".repeat(indent);
        let mut output = format!("{}{}:\n", indent_str, node.node_type.as_str());

        // Add attributes
        if !node.attributes.is_empty() {
            output.push_str(&format!("{}  Attributes:\n", indent_str));
            for attr in node.attributes.as_ref() {
                output.push_str(&format!(
                    "{}    {} = \"{}\"\n",
                    indent_str,
                    attr.key.as_str(),
                    attr.value.as_str()
                ));
            }
        }

        // Add children (can be text or element nodes)
        if !node.children.is_empty() {
            output.push_str(&format!("{}  Children:\n", indent_str));
            for child in node.children.as_ref() {
                match child {
                    XmlNodeChild::Text(text) => {
                        let trimmed = text.as_str().trim();
                        if !trimmed.is_empty() {
                            output.push_str(&format!("{}    Text: \"{}\"\n", indent_str, trimmed));
                        }
                    }
                    XmlNodeChild::Element(elem) => {
                        output.push_str(&Self::format_xml_for_display(elem, indent + 4));
                    }
                }
            }
        }

        output
    }
}

/// CSS Warning types
#[derive(Debug, Clone)]
pub enum CssWarningType {
    /// Parse error
    ParseError(CssParseWarnMsgOwned),
    /// Property not supported
    UnsupportedProperty(String),
    /// Value out of range
    ValueOutOfRange(String, String),
    /// Unknown selector
    UnknownSelector(String),
    /// Potentially invalid rule
    InvalidRule(String),
}

impl std::fmt::Display for CssWarningType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CssWarningType::ParseError(err) => write!(
                f,
                "Parse error at {:?}..{:?}: {}",
                err.location.start,
                err.location.end,
                err.warning.to_shared()
            ),
            CssWarningType::UnsupportedProperty(prop) => {
                write!(f, "Unsupported property: {}", prop)
            }
            CssWarningType::ValueOutOfRange(prop, val) => {
                write!(f, "Value out of range for {}: {}", prop, val)
            }
            CssWarningType::UnknownSelector(sel) => write!(f, "Unknown selector: {}", sel),
            CssWarningType::InvalidRule(rule) => write!(f, "Potentially invalid rule: {}", rule),
        }
    }
}

/// Collects CSS warnings during parsing and validation
pub struct CssWarningCollector {
    pub warnings: Vec<CssWarningType>,
}

impl CssWarningCollector {
    /// Create a new CSS warning collector
    pub fn new() -> Self {
        Self {
            warnings: Vec::new(),
        }
    }

    /// Parse CSS and collect warnings
    pub fn parse_css(&mut self, css_text: &str) -> Css {
        // Parse CSS and get warnings
        let (css, warnings) = Css::from_string_with_warnings(css_text.to_string().into());

        // Check for parse errors
        for w in warnings {
            self.warnings.push(CssWarningType::ParseError(w));
        }

        // Validate the CSS properties
        self.validate_css(&css);

        // Get the parsed CSS
        css
    }

    /// Validate CSS properties and collect warnings
    fn validate_css(&mut self, css: &Css) {
        for stylesheet in css.stylesheets.as_ref() {
            for rule in stylesheet.rules.as_ref() {
                // Check selector validity
                self.validate_selector(&rule.path.to_string());

                // Check property validity
                for decl in rule.declarations.as_ref() {
                    match decl {
                        CssDeclaration::Static(prop) => {
                            self.validate_property(prop);
                        }
                        CssDeclaration::Dynamic(dynamic) => {
                            self.validate_property(&dynamic.default_value);
                        }
                    }
                }
            }
        }
    }

    /// Validate a CSS selector
    fn validate_selector(&mut self, selector: &str) {
        // Check for potential selector issues
        if selector.contains(">>") {
            self.warnings.push(CssWarningType::UnknownSelector(format!(
                "Non-standard selector syntax: {}",
                selector
            )));
        }

        // Check for potentially unsupported pseudo-selectors
        let problematic_pseudos = [":has(", ":is(", ":where(", "::part(", "::slotted("];
        for pseudo in problematic_pseudos {
            if selector.contains(pseudo) {
                self.warnings.push(CssWarningType::UnknownSelector(format!(
                    "Potentially unsupported pseudo-selector: {}",
                    pseudo
                )));
            }
        }
    }

    /// Validate a CSS property
    fn validate_property(&mut self, property: &CssProperty) {
        // Example validations - add more as needed
        match property {
            CssProperty::Display(val) => {
                // Check for display values
                if val.is_none() {
                    self.warnings.push(CssWarningType::ValueOutOfRange(
                        "display".to_string(),
                        format!("{:?}", val),
                    ));
                }
            }
            CssProperty::MarginLeft(val) => {
                // Check for negative margins that might cause issues
                if let Some(margin) = val.get_property() {
                    if margin.inner.number.get().is_sign_negative() {
                        self.warnings.push(CssWarningType::ValueOutOfRange(
                            format!("{:?}", property),
                            format!("{:?}", margin),
                        ));
                    }
                }
            }
            CssProperty::MarginRight(val) => {
                // Check for negative margins that might cause issues
                if let Some(margin) = val.get_property() {
                    if margin.inner.number.get().is_sign_negative() {
                        self.warnings.push(CssWarningType::ValueOutOfRange(
                            format!("{:?}", property),
                            format!("{:?}", margin),
                        ));
                    }
                }
            }
            CssProperty::MarginTop(val) => {
                // Check for negative margins that might cause issues
                if let Some(margin) = val.get_property() {
                    if margin.inner.number.get().is_sign_negative() {
                        self.warnings.push(CssWarningType::ValueOutOfRange(
                            format!("{:?}", property),
                            format!("{:?}", margin),
                        ));
                    }
                }
            }
            CssProperty::MarginBottom(val) => {
                // Check for negative margins that might cause issues
                if let Some(margin) = val.get_property() {
                    if margin.inner.number.get().is_sign_negative() {
                        self.warnings.push(CssWarningType::ValueOutOfRange(
                            format!("{:?}", property),
                            format!("{:?}", margin),
                        ));
                    }
                }
            }
            // Add more property validations as needed
            _ => {}
        }
    }

    /// Format the warnings as a string for display
    pub fn format_warnings(&self) -> String {
        use std::fmt::Write;
        if self.warnings.is_empty() {
            return "No CSS warnings detected.".to_string();
        }

        let mut output = String::new();
        writeln!(output, "CSS Warnings ({})", self.warnings.len()).unwrap();
        writeln!(output, "===================").unwrap();

        for (i, warning) in self.warnings.iter().enumerate() {
            writeln!(output, "{}. {}", i + 1, warning).unwrap();
        }

        output
    }

    /// Check if there are any warnings
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Return the number of warnings
    pub fn warning_count(&self) -> usize {
        self.warnings.len()
    }
}

/// Simple struct for capturing CSS statistics
pub struct CssStats {
    pub rule_count: usize,
    pub declaration_count: usize,
    pub selectors: Vec<String>,
    pub properties: Vec<String>,
}

impl CssStats {
    /// Analyze CSS and return statistics
    pub fn analyze(css: &Css) -> Self {
        let mut stats = Self {
            rule_count: 0,
            declaration_count: 0,
            selectors: Vec::new(),
            properties: Vec::new(),
        };

        for stylesheet in css.stylesheets.as_ref() {
            for rule in stylesheet.rules.as_ref() {
                stats.rule_count += 1;
                stats.selectors.push(rule.path.to_string());

                for decl in rule.declarations.as_ref() {
                    stats.declaration_count += 1;
                    match decl {
                        CssDeclaration::Static(prop) => {
                            stats.properties.push(format!("{:?}", prop));
                        }
                        CssDeclaration::Dynamic(dynamic) => {
                            stats
                                .properties
                                .push(format!("{:?}", dynamic.default_value));
                        }
                    }
                }
            }
        }

        stats
    }

    /// Format CSS statistics as a string
    pub fn format(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();

        writeln!(output, "CSS Statistics").unwrap();
        writeln!(output, "==============").unwrap();
        writeln!(output, "Rules: {}", self.rule_count).unwrap();
        writeln!(output, "Declarations: {}", self.declaration_count).unwrap();

        if !self.selectors.is_empty() {
            writeln!(output, "\nSelectors:").unwrap();
            for (i, sel) in self.selectors.iter().enumerate() {
                if i < 10 {
                    // Limit number of selectors shown
                    writeln!(output, "- {}", sel).unwrap();
                }
            }

            if self.selectors.len() > 10 {
                writeln!(output, "... ({} more)", self.selectors.len() - 10).unwrap();
            }
        }

        // Count property types
        let mut property_types = std::collections::BTreeMap::new();
        for prop in &self.properties {
            let property_type = if let Some(idx) = prop.find('(') {
                &prop[0..idx]
            } else {
                prop
            };

            *property_types.entry(property_type.to_string()).or_insert(0) += 1;
        }

        writeln!(output, "\nProperty Types:").unwrap();
        let mut sorted_types: Vec<_> = property_types.iter().collect();
        sorted_types.sort_by(|a, b| b.1.cmp(a.1));

        for (prop_type, count) in sorted_types {
            writeln!(output, "- {}: {}", prop_type, count).unwrap();
        }

        output
    }
}

/// Contains all debug information for a test
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DebugData {
    // Metadata extracted from the test file
    pub title: String,
    pub assert_content: String,
    pub help_link: String,
    pub flags: String,
    pub author: String,

    pub xhtml_source: String,

    // CSS parsing
    pub css_warnings: String,
    pub css_stats: String,

    // XML and DOM structure
    pub parsed_xml: String,
    pub styled_dom: String,

    // Layout information
    pub solved_layout: String,
    pub layout_stats: String,

    // Display list information
    pub display_list: String,

    // Chrome computed layout (via CDP, if available)
    pub chrome_layout: String,

    // Rendering information
    pub xml_formatting_time_ms: u64,
    pub layout_time_ms: u64,
    pub render_time_ms: u64,
    pub render_warnings: Vec<String>,
}

impl DebugData {
    /// Create a new debug data collector
    pub fn new(xhtml_source: String) -> DebugData {
        let mut m = DebugData::default();
        m.xhtml_source = xhtml_source;
        m
    }

    /// Format the entire debug data as a string
    pub fn format(&self) -> String {
        use std::fmt::Write;

        let mut output = String::new();

        // Metadata section
        writeln!(output, "# Test Metadata").unwrap();
        writeln!(output, "Title: {}", self.title).unwrap();
        if !self.assert_content.is_empty() {
            writeln!(output, "Assert: {}", self.assert_content).unwrap();
        }
        if !self.help_link.is_empty() {
            writeln!(output, "Spec Link: {}", self.help_link).unwrap();
        }
        if !self.author.is_empty() {
            writeln!(output, "Author: {}", self.author).unwrap();
        }
        if !self.flags.is_empty() {
            writeln!(output, "Flags: {}", self.flags).unwrap();
        }

        // Add all other sections with appropriate headers
        self.add_section(&mut output, "CSS Warnings", &self.css_warnings);
        self.add_section(&mut output, "CSS Statistics", &self.css_stats);
        self.add_section(&mut output, "Parsed XML", &self.parsed_xml);
        self.add_section(&mut output, "Styled DOM", &self.styled_dom);
        self.add_section(&mut output, "Solved Layout (Azul)", &self.solved_layout);
        self.add_section(&mut output, "Layout Statistics", &self.layout_stats);
        self.add_section(&mut output, "Display List (Azul)", &self.display_list);

        // Rendering information
        writeln!(output, "\n# Rendering Information").unwrap();
        writeln!(
            output,
            "XML formatting time: {} ms",
            self.xml_formatting_time_ms
        )
        .unwrap();
        writeln!(output, "Layout time: {} ms", self.layout_time_ms).unwrap();
        writeln!(output, "Render time: {} ms", self.render_time_ms).unwrap();

        if !self.render_warnings.is_empty() {
            writeln!(output, "\n## Layout Debug Trace").unwrap();
            writeln!(
                output,
                "The following trace shows the code path taken during layout calculation:"
            )
            .unwrap();
            for (i, warning) in self.render_warnings.iter().enumerate() {
                writeln!(output, "{}. {}", i + 1, warning).unwrap();
            }
        }

        // Chrome layout data (if available)
        if !self.chrome_layout.is_empty() && self.chrome_layout != "{}" {
            writeln!(output, "\n# Chrome Layout Data").unwrap();
            writeln!(
                output,
                "The following JSON contains Chrome's computed layout for comparison:"
            )
            .unwrap();
            // Try to pretty-print if it's valid JSON
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&self.chrome_layout) {
                if let Ok(pretty) = serde_json::to_string_pretty(&parsed) {
                    writeln!(output, "{}", pretty).unwrap();
                } else {
                    writeln!(output, "{}", self.chrome_layout).unwrap();
                }
            } else {
                writeln!(output, "{}", self.chrome_layout).unwrap();
            }
        }

        output
    }

    /// Add a section to the output if it's not empty
    fn add_section(&self, output: &mut String, title: &str, content: &str) {
        use std::fmt::Write;
        if !content.is_empty() {
            writeln!(output, "\n# {}", title).unwrap();
            writeln!(output, "{}", content).unwrap();
        }
    }
}

/// Debug data collector for the reftest runner
pub struct DebugDataCollector {
    pub data: DebugData,
}

impl DebugDataCollector {
    /// Create a new debug data collector
    pub fn new(source: String) -> Self {
        Self {
            data: DebugData::new(source),
        }
    }

    /// Set metadata for the test
    pub fn set_metadata(
        &mut self,
        title: String,
        assert_content: String,
        help_link: String,
        flags: String,
        author: String,
    ) {
        self.data.title = title;
        self.data.assert_content = assert_content;
        self.data.help_link = help_link;
        self.data.flags = flags;
        self.data.author = author;
    }

    /// Set CSS warnings and stats
    pub fn set_css_debug_info(&mut self, warnings: String, stats: String) {
        self.data.css_warnings = warnings;
        self.data.css_stats = stats;
    }

    /// Set XML and DOM structure
    pub fn set_dom_debug_info(&mut self, parsed_xml: String, styled_dom: String) {
        self.data.parsed_xml = parsed_xml;
        self.data.styled_dom = styled_dom;
    }

    /// Set layout information
    pub fn set_layout_debug_info(&mut self, solved_layout: String, layout_stats: String) {
        self.data.solved_layout = solved_layout;
        self.data.layout_stats = layout_stats;
    }

    /// Set display list information
    pub fn set_display_list_debug_info(&mut self, display_list: String) {
        self.data.display_list = display_list;
    }

    /// Add rendering information
    pub fn set_render_info(
        &mut self,
        xml_formatting_time_ms: u64,
        layout_time_ms: u64,
        render_time_ms: u64,
        warnings: Vec<String>,
    ) {
        self.data.xml_formatting_time_ms = xml_formatting_time_ms;
        self.data.layout_time_ms = layout_time_ms;
        self.data.render_time_ms = render_time_ms;
        self.data.render_warnings = warnings;
    }

    /// Get the formatted debug data
    pub fn get_formatted_data(&self) -> String {
        self.data.format()
    }

    /// Get the debug data
    pub fn get_data(&self) -> DebugData {
        self.data.clone()
    }
}

/// Wrapper around layout solving that captures debug information
/// Uses the new solver3 + text3 layout engine
pub fn solve_layout_with_debug(
    styled_dom: StyledDom,
    fake_window_state: &FullWindowState,
    renderer_resources: &mut RendererResources,
    debug_collector: &mut DebugDataCollector,
) -> anyhow::Result<(
    azul_layout::solver3::display_list::DisplayList,
    Vec<String>,
    azul_layout::LayoutWindow,
)> {
    use std::fmt::Write;

    // Create LayoutWindow for layout computation
    let fc_cache = azul_layout::font::loading::build_font_cache();
    let mut layout_window = azul_layout::LayoutWindow::new(fc_cache)?;

    // Prepare debug messages
    let mut debug_messages = Some(Vec::new());

    // Start timer
    let start = std::time::Instant::now();

    let external = ExternalSystemCallbacks::rust_internal();

    // Call layout_and_generate_display_list with all required parameters
    layout_window.layout_and_generate_display_list(
        styled_dom,
        fake_window_state,
        renderer_resources,
        &external,
        &mut debug_messages,
    )?;

    // Extract the display list from the root DOM's layout result
    // We need to take ownership, so we remove the layout result and extract the display list
    let display_list = layout_window
        .layout_results
        .remove(&DomId::ROOT_ID)
        .ok_or_else(|| anyhow::anyhow!("No layout result for root DOM"))?
        .display_list;

    // End timer
    let elapsed = start.elapsed();

    // Collect layout warnings with message type
    let warnings: Vec<String> = debug_messages
        .unwrap_or_default()
        .into_iter()
        .map(|s| {
            let msg_type = match s.message_type {
                LayoutDebugMessageType::Info => "INFO",
                LayoutDebugMessageType::Warning => "WARN",
                LayoutDebugMessageType::Error => "ERROR",
                LayoutDebugMessageType::BoxProps => "BOX",
                LayoutDebugMessageType::CssGetter => "CSS",
                LayoutDebugMessageType::BfcLayout => "BFC",
                LayoutDebugMessageType::IfcLayout => "IFC",
                LayoutDebugMessageType::TableLayout => "TABLE",
                LayoutDebugMessageType::DisplayType => "DISPLAY",
                LayoutDebugMessageType::PositionCalculation => "POS",
            };
            format!("[{}] {} ({})", msg_type, s.message, s.location)
        })
        .collect();

    // Capture layout statistics
    let mut layout_stats = String::new();
    writeln!(layout_stats, "Layout Statistics").unwrap();
    writeln!(layout_stats, "=================").unwrap();
    writeln!(layout_stats, "Layout time: {:?}", elapsed).unwrap();
    writeln!(
        layout_stats,
        "Display list items: {}",
        display_list.items.len()
    )
    .unwrap();

    debug_collector.set_layout_debug_info(
        format!("Display list with {} items", display_list.items.len()),
        layout_stats,
    );

    Ok((display_list, warnings, layout_window))
}

// Old solve_layout_with_debug that returned LayoutResult - removed
// The new version returns DisplayList directly from solver3

/// Format the display list for debugging
pub fn format_display_list_for_debug_solver3(display_list: &azul_layout::DisplayList3) -> String {
    use std::fmt::Write;
    let mut output = String::new();

    writeln!(output, "Display List (solver3)").unwrap();
    writeln!(output, "=============").unwrap();
    writeln!(output, "Items: {}", display_list.items.len()).unwrap();

    for (idx, item) in display_list.items.iter().enumerate() {
        writeln!(output, "  [{}] {:?}", idx, item).unwrap();
    }

    output
}

// Old solver2 display list formatting functions removed
// (CachedDisplayList, DisplayListMsg, DisplayListFrame no longer exist)

/// Enhanced test result with debug data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedTestResult {
    // Basic test info
    pub test_name: String,
    pub diff_count: usize,
    pub passed: bool,

    // Metadata from test file
    pub title: String,
    pub assert_content: String,
    pub help_link: String,
    pub flags: String,
    pub author: String,

    // Debug info
    pub xhtml_source: String,
    pub css_warnings: String,
    pub parsed_xml: String,
    pub styled_dom: String,
    pub solved_layout: String,
    pub display_list: String,

    // Chrome computed layout (via CDP, if available)
    pub chrome_layout: String,

    // Additional stats
    pub xml_formatting_time_ms: u64,
    pub layout_time_ms: u64,
    pub render_time_ms: u64,
    pub render_warnings: Vec<String>,
}

impl EnhancedTestResult {
    /// Create a new enhanced test result from test name
    pub fn new(test_name: String, xhtml_source: String) -> Self {
        Self {
            test_name,
            xhtml_source,
            diff_count: 0,
            passed: false,
            title: String::new(),
            assert_content: String::new(),
            help_link: String::new(),
            flags: String::new(),
            author: String::new(),
            css_warnings: String::new(),
            parsed_xml: String::new(),
            styled_dom: String::new(),
            solved_layout: String::new(),
            display_list: String::new(),
            chrome_layout: String::new(),
            render_warnings: Vec::new(),
            xml_formatting_time_ms: 0,
            layout_time_ms: 0,
            render_time_ms: 0,
        }
    }

    /// Create an enhanced test result from debug data
    pub fn from_debug_data(
        test_name: String,
        diff_count: usize,
        passed: bool,
        debug_data: DebugData,
    ) -> Self {
        Self {
            test_name,
            diff_count,
            passed,
            title: debug_data.title,
            assert_content: debug_data.assert_content,
            help_link: debug_data.help_link,
            flags: debug_data.flags,
            author: debug_data.author,
            render_warnings: debug_data.render_warnings,
            css_warnings: debug_data.css_warnings,
            xhtml_source: debug_data.xhtml_source,
            parsed_xml: debug_data.parsed_xml,
            styled_dom: debug_data.styled_dom,
            solved_layout: debug_data.solved_layout,
            display_list: debug_data.display_list,
            chrome_layout: debug_data.chrome_layout,
            xml_formatting_time_ms: debug_data.xml_formatting_time_ms,
            layout_time_ms: debug_data.layout_time_ms,
            render_time_ms: debug_data.render_time_ms,
        }
    }
}

// Generate enhanced HTML report
fn generate_enhanced_html_report(
    report_path: &Path,
    results: &[EnhancedTestResult],
    chrome_version: &str,
    current_time: &str,
    git_hash: &str,
    is_chrome_installed: bool,
) -> anyhow::Result<()> {
    let mut file = File::create(&report_path)?;

    // Read the HTML template
    let html_template = include_str!("./report_template.html");

    // Add header information
    let chrome_class = if is_chrome_installed {
        "success"
    } else {
        "error"
    };

    // Replace placeholders with actual data
    let html_content = html_template
        .replace("CURRENT_TIME", current_time)
        .replace("CHROME_CLASS", chrome_class)
        .replace("CHROME_VERSION", &chrome_version.replace("Google ", ""))
        .replace("GIT_HASH", git_hash)
        .replace(
            "{TEST_DATA_BASE64}",
            &base64::prelude::BASE64_STANDARD
                .encode(&serde_json::to_string(&results).unwrap_or_default()),
        );

    // Write HTML to file
    file.write_all(html_content.as_bytes())?;

    println!(
        "Enhanced HTML report generated at {}",
        report_path.display()
    );

    Ok(())
}

/// Helper function to extract CSS from an XHTML file
fn extract_css_from_xml(xml_content: &str) -> anyhow::Result<String> {
    let mut css = String::new();

    // Simple string-based extraction for efficiency
    if let Some(style_start) = xml_content.find("<style type=\"text/css\">") {
        if let Some(style_end) = xml_content[style_start..].find("</style>") {
            css = xml_content[style_start + 23..style_start + style_end]
                .trim()
                .to_string();
        }
    }

    Ok(css)
}
