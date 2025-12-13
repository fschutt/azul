//! LLM-assisted debug analysis for layout test failures
//! Uses Gemini API with tiktoken for token budget management

use std::{
    fs,
    path::{Path, PathBuf},
};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tiktoken_rs::cl100k_base;

/// Maximum tokens for Gemini API (leaving headroom for response)
const MAX_TOKENS: usize = 900_000;

/// Token cost per image at MEDIA_RESOLUTION_ULTRA_HIGH (2240 tokens)
const IMAGE_TOKENS_ULTRA_HIGH: usize = 2240;

/// Gemini API URL (using gemini-3-pro-preview with thinking)
const GEMINI_API_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-3-pro-preview:generateContent";

// Gemini API request/response structures
#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "generationConfig")]
    generation_config: GenerationConfig,
}

#[derive(Serialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum GeminiPart {
    Text { text: String },
    InlineData { inline_data: InlineData },
}

#[derive(Serialize)]
struct InlineData {
    mime_type: String,
    data: String,
}

#[derive(Serialize)]
struct GenerationConfig {
    #[serde(rename = "thinkingConfig")]
    thinking_config: ThinkingConfig,
}

#[derive(Serialize)]
struct ThinkingConfig {
    #[serde(rename = "thinkingLevel")]
    thinking_level: String,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    error: Option<GeminiError>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: GeminiResponseContent,
}

#[derive(Deserialize)]
struct GeminiResponseContent {
    parts: Vec<GeminiResponsePart>,
}

#[derive(Deserialize)]
struct GeminiResponsePart {
    text: Option<String>,
}

#[derive(Deserialize)]
struct GeminiError {
    message: String,
}

/// Configuration for debug analysis
pub struct DebugConfig {
    pub test_name: String,
    pub question: Option<String>,
    pub azul_root: PathBuf,
    pub output_dir: PathBuf,
}

/// Collected debug data for a test
#[derive(Debug)]
pub struct TestDebugData {
    pub test_name: String,
    pub xhtml_source: String,
    pub css_warnings: Vec<String>,
    pub layout_debug_messages: Vec<String>,
    pub chrome_layout_data: Option<String>,
    pub azul_display_list: Option<String>,
    pub diff_count: Option<usize>,
    /// Chrome screenshot as base64 PNG
    pub chrome_image_base64: Option<String>,
    /// Azul screenshot as base64 WebP  
    pub azul_image_base64: Option<String>,
}

/// Priority levels for source files
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FilePriority {
    Critical, // Core solver files
    High,     // text3/cache.rs, cpurender/mod.rs
    Medium,   // Taffy reference
}

/// Run the debug analysis for a specific test
pub fn run_debug_analysis(config: DebugConfig) -> anyhow::Result<()> {
    println!(
        "[DEBUG] Starting debug analysis for test: {}",
        config.test_name
    );

    // Load API key
    let api_key_path = config.azul_root.join("GEMINI_API_KEY.txt");
    let api_key = fs::read_to_string(&api_key_path)
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to load Gemini API key from {:?}: {}",
                api_key_path,
                e
            )
        })?
        .trim()
        .to_string();
    println!("[DEBUG] API key loaded successfully");

    // Check if test has already passed
    if is_test_passed(&config)? {
        println!(
            "[DEBUG] Test '{}' has already passed, skipping",
            config.test_name
        );
        return Ok(());
    }

    // Find the test file
    let test_file = find_test_file(&config)?;
    println!("[DEBUG] Found test file: {:?}", test_file);

    // Run the test and collect debug data
    let debug_data = run_test_and_collect_data(&config, &test_file)?;

    // Check if test already passes (pixel difference <= 0.5% of total pixels)
    // Total pixels = WIDTH * HEIGHT = 1920 * 1080 = 2,073,600
    // 0.5% = 10,368 pixels
    const PASS_THRESHOLD: usize = (1920 * 1080) / 200; // 0.5% tolerance
    let diff_count = debug_data.diff_count.unwrap_or(0);

    if diff_count <= PASS_THRESHOLD {
        let percentage = (diff_count as f64 / (1920.0 * 1080.0)) * 100.0;
        println!(
            "\n✅ Test '{}' passes ({} pixels different = {:.3}%, threshold 0.5%). No debug \
             needed.",
            config.test_name, diff_count, percentage
        );
        return Ok(());
    }

    let percentage = (diff_count as f64 / (1920.0 * 1080.0)) * 100.0;
    println!(
        "[DEBUG] Pixel difference: {} ({:.3}% of screen)",
        diff_count, percentage
    );

    // Collect source code with token budget
    let source_files = collect_source_code_with_budget(&config)?;

    // Build the prompt
    let prompt = build_prompt(&debug_data, &source_files, &config)?;

    // Initialize tokenizer and count final tokens
    let bpe = cl100k_base().map_err(|e| anyhow::anyhow!("Failed to init tokenizer: {}", e))?;
    let total_tokens = bpe.encode_with_special_tokens(&prompt).len();
    println!(
        "[DEBUG] Final prompt: {} tokens ({} chars)",
        total_tokens,
        prompt.len()
    );

    // Save prompt to file
    fs::create_dir_all(&config.output_dir)?;

    let prompt_path = config
        .output_dir
        .join(format!("{}_prompt.md", config.test_name));
    fs::write(&prompt_path, &prompt)?;
    println!("[DEBUG] Prompt saved to: {:?}", prompt_path);

    // Save debug data as JSON
    let json_path = config
        .output_dir
        .join(format!("{}_debug.json", config.test_name));
    let json_data = format!(
        r#"{{
    "test_name": "{}",
    "xhtml_chars": {},
    "css_warnings": {},
    "debug_messages": {},
    "chrome_layout_chars": {},
    "azul_display_list_chars": {},
    "diff_count": {},
    "total_tokens": {},
    "source_files": {}
}}"#,
        debug_data.test_name,
        debug_data.xhtml_source.len(),
        debug_data.css_warnings.len(),
        debug_data.layout_debug_messages.len(),
        debug_data
            .chrome_layout_data
            .as_ref()
            .map(|s| s.len())
            .unwrap_or(0),
        debug_data
            .azul_display_list
            .as_ref()
            .map(|s| s.len())
            .unwrap_or(0),
        debug_data.diff_count.unwrap_or(0),
        total_tokens,
        source_files.len()
    );
    fs::write(&json_path, &json_data)?;
    println!("[DEBUG] Debug data saved to: {:?}", json_path);

    println!("\n[DEBUG] Analysis complete!");
    println!("  - Prompt file: {:?}", prompt_path);
    println!("  - Total tokens: {} / {}", total_tokens, MAX_TOKENS);
    println!("  - Source files included: {}", source_files.len());
    println!(
        "\n🤖 Sending to Gemini API... This will take a while ({} tokens sent)",
        total_tokens
    );

    // Call Gemini API
    let response = call_gemini_api(&api_key, &debug_data, &prompt)?;

    // Save response
    let response_path = config
        .output_dir
        .join(format!("{}_response.md", config.test_name));
    fs::write(&response_path, &response)?;

    println!("\n📝 Response saved to: {:?}", response_path);
    println!("\n{}", "=".repeat(80));
    println!("{}", response);
    println!("{}", "=".repeat(80));

    Ok(())
}

/// Call the Gemini API with multimodal content (text + images)
fn call_gemini_api(
    api_key: &str,
    debug_data: &TestDebugData,
    text_prompt: &str,
) -> anyhow::Result<String> {
    // Build multimodal parts: images first, then text
    let mut parts: Vec<GeminiPart> = Vec::new();

    // Add Chrome screenshot if available
    if let Some(ref chrome_base64) = debug_data.chrome_image_base64 {
        parts.push(GeminiPart::InlineData {
            inline_data: InlineData {
                mime_type: "image/png".to_string(),
                data: chrome_base64.clone(),
            },
        });
    }

    // Add Azul screenshot if available
    if let Some(ref azul_base64) = debug_data.azul_image_base64 {
        parts.push(GeminiPart::InlineData {
            inline_data: InlineData {
                mime_type: "image/webp".to_string(),
                data: azul_base64.clone(),
            },
        });
    }

    // Add text prompt
    parts.push(GeminiPart::Text {
        text: text_prompt.to_string(),
    });

    let request = GeminiRequest {
        contents: vec![GeminiContent {
            role: "user".to_string(),
            parts,
        }],
        generation_config: GenerationConfig {
            thinking_config: ThinkingConfig {
                thinking_level: "HIGH".to_string(),
            },
        },
    };

    let url = format!("{}?key={}", GEMINI_API_URL, api_key);

    let response: GeminiResponse = ureq::post(&url)
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
        .and_then(|c| c.content.parts.into_iter().next())
        .and_then(|p| p.text)
        .ok_or_else(|| anyhow::anyhow!("No text in Gemini response"))?;

    Ok(text)
}

/// Check if a test has already passed based on existing results
fn is_test_passed(config: &DebugConfig) -> anyhow::Result<bool> {
    let results_path = config.azul_root.join("doc/target/reftest_results.json");
    if !results_path.exists() {
        return Ok(false);
    }

    let results_content = fs::read_to_string(&results_path)?;

    // Simple check: look for test name with "passed" status
    // Format: "test_name": { "status": "passed" }
    let search_pattern = format!(r#""{}""#, config.test_name);
    if results_content.contains(&search_pattern) {
        // Check if it's marked as passed
        if let Some(pos) = results_content.find(&search_pattern) {
            let after = &results_content[pos..];
            if let Some(status_pos) = after.find("\"status\"") {
                let status_section = &after[status_pos..];
                if status_section.contains("\"passed\"")
                    && !status_section[..100.min(status_section.len())].contains("\"failed\"")
                {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

/// Find the test file by name (tries .xht, .xhtml, .html extensions)
fn find_test_file(config: &DebugConfig) -> anyhow::Result<PathBuf> {
    let reftest_dir = config.azul_root.join("doc/working");

    // Try different extensions
    for ext in &["xht", "xhtml", "html"] {
        // Search in reftest subdirectories
        for entry in walkdir::WalkDir::new(&reftest_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let file_ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

                if file_stem == config.test_name && file_ext == *ext {
                    return Ok(path.to_path_buf());
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "Test file not found for: {}",
        config.test_name
    ))
}

/// Run the test and collect all debug data
fn run_test_and_collect_data(
    config: &DebugConfig,
    test_file: &Path,
) -> anyhow::Result<TestDebugData> {
    // Read XHTML source
    let xhtml_source = fs::read_to_string(test_file)?;

    // Get Chrome path
    let chrome_path = super::get_chrome_path();

    // Generate Chrome screenshot and layout data
    let debug_dir = config.output_dir.join("debug_output");
    fs::create_dir_all(&debug_dir)?;

    let chrome_screenshot_path = debug_dir.join(format!("{}_chrome.png", config.test_name));
    let chrome_layout_path = debug_dir.join(format!("{}_chrome_layout.json", config.test_name));

    println!("[DEBUG] Generating Chrome reference...");
    super::generate_chrome_screenshot_with_debug(
        &chrome_path,
        test_file,
        &chrome_screenshot_path,
        &chrome_layout_path,
        super::WIDTH,
        super::HEIGHT,
    )?;

    let chrome_layout_data = if chrome_layout_path.exists() {
        Some(fs::read_to_string(&chrome_layout_path)?)
    } else {
        None
    };

    // Generate Azul rendering with debug output
    println!("[DEBUG] Generating Azul rendering...");
    let azul_screenshot_path = debug_dir.join(format!("{}_azul.webp", config.test_name));

    let (css_warnings, layout_debug_messages, azul_display_list) =
        generate_azul_with_debug(&test_file, &azul_screenshot_path)?;

    // Compare images if both exist
    let diff_count = if chrome_screenshot_path.exists() && azul_screenshot_path.exists() {
        match super::compare_images(&chrome_screenshot_path, &azul_screenshot_path) {
            Ok(count) => Some(count),
            Err(_) => None,
        }
    } else {
        None
    };

    // Read images as base64 for Gemini API
    let chrome_image_base64 = if chrome_screenshot_path.exists() {
        let bytes = fs::read(&chrome_screenshot_path)?;
        Some(base64::engine::general_purpose::STANDARD.encode(&bytes))
    } else {
        None
    };

    let azul_image_base64 = if azul_screenshot_path.exists() {
        let bytes = fs::read(&azul_screenshot_path)?;
        Some(base64::engine::general_purpose::STANDARD.encode(&bytes))
    } else {
        None
    };

    Ok(TestDebugData {
        test_name: config.test_name.clone(),
        xhtml_source,
        css_warnings,
        layout_debug_messages,
        chrome_layout_data,
        azul_display_list,
        diff_count,
        chrome_image_base64,
        azul_image_base64,
    })
}

/// Generate Azul rendering and capture debug output
fn generate_azul_with_debug(
    test_file: &Path,
    output_path: &Path,
) -> anyhow::Result<(Vec<String>, Vec<String>, Option<String>)> {
    // Use the existing generate_azul_rendering function
    let debug_data = super::generate_azul_rendering(test_file, output_path, 1.0)?;

    // Extract CSS warnings (DebugData has css_warnings as String)
    let css_warnings = if debug_data.css_warnings.is_empty() {
        Vec::new()
    } else {
        debug_data
            .css_warnings
            .lines()
            .map(|s| s.to_string())
            .collect()
    };

    // Extract layout messages (DebugData has render_warnings as Vec<String>)
    let layout_debug_messages = debug_data.render_warnings.clone();

    // Format display list (DebugData has display_list as String)
    let display_list = if !debug_data.display_list.is_empty() {
        Some(debug_data.display_list.clone())
    } else {
        None
    };

    Ok((css_warnings, layout_debug_messages, display_list))
}

/// Collect source code files within token budget
/// Note: Images use fixed token costs (2240 per image at ULTRA_HIGH resolution)
fn collect_source_code_with_budget(config: &DebugConfig) -> anyhow::Result<Vec<(String, String)>> {
    let bpe = cl100k_base().map_err(|e| anyhow::anyhow!("Failed to init tokenizer: {}", e))?;

    // Define source files with priorities
    let mut source_files: Vec<(FilePriority, PathBuf)> = Vec::new();

    // Critical: solver3 core files (excluding large/less relevant ones)
    let solver3_dir = config.azul_root.join("layout/src/solver3");
    if solver3_dir.exists() {
        for entry in fs::read_dir(&solver3_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "rs").unwrap_or(false) {
                let filename = path.file_name().unwrap().to_str().unwrap();
                // Skip large or less relevant files
                if !["pagination.rs", "scrollbar.rs", "paged_layout.rs"].contains(&filename) {
                    source_files.push((FilePriority::Critical, path));
                }
            }
        }
    }

    // High: text3/cache.rs and cpurender.rs (in that order)
    let text3_cache_rs = config.azul_root.join("layout/src/text3/cache.rs");
    if text3_cache_rs.exists() {
        source_files.push((FilePriority::High, text3_cache_rs));
    }

    let cpurender_rs = config.azul_root.join("layout/src/cpurender.rs");
    if cpurender_rs.exists() {
        source_files.push((FilePriority::High, cpurender_rs));
    }

    // Medium: taffy reference implementation
    let taffy_compute_dir = config.azul_root.join("layout/taffy/src/compute");
    if taffy_compute_dir.exists() {
        for entry in walkdir::WalkDir::new(&taffy_compute_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "rs").unwrap_or(false) {
                source_files.push((FilePriority::Medium, path.to_path_buf()));
            }
        }
    }

    // Sort by priority
    source_files.sort_by_key(|(priority, _)| *priority);

    // Reserve tokens for images (2 images at ULTRA_HIGH = 2 * 2240 = 4480 tokens)
    let image_tokens = 2 * IMAGE_TOKENS_ULTRA_HIGH;
    println!(
        "[DEBUG] Reserved {} tokens for images (2 x ULTRA_HIGH)",
        image_tokens
    );

    // Load files within token budget (after reserving for images)
    let mut result: Vec<(String, String)> = Vec::new();
    let mut total_tokens = 0;
    let token_budget = MAX_TOKENS - 100_000 - image_tokens; // Reserve 100k for test data + prompt structure + images

    for (priority, path) in source_files {
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let tokens = bpe.encode_with_special_tokens(&content).len();

        if total_tokens + tokens > token_budget {
            println!(
                "[DEBUG] Token budget reached, skipping {:?} ({} tokens)",
                path, tokens
            );
            break;
        }

        let relative_path = path
            .strip_prefix(&config.azul_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        println!(
            "[DEBUG] Including {:?}: {} tokens (priority: {:?})",
            relative_path, tokens, priority
        );

        result.push((relative_path, content));
        total_tokens += tokens;
    }

    println!(
        "[DEBUG] Total source code tokens: {} / {}",
        total_tokens, token_budget
    );

    Ok(result)
}

/// Build the final prompt for the LLM
/// Note: Images are embedded as base64 data URLs for Gemini API multimodal input
fn build_prompt(
    debug_data: &TestDebugData,
    source_files: &[(String, String)],
    config: &DebugConfig,
) -> anyhow::Result<String> {
    let mut prompt = String::new();

    // Header
    prompt.push_str("# Layout Engine Debug Analysis\n\n");
    prompt.push_str("You are debugging a CSS layout engine implementation. ");
    prompt.push_str(
        "The test below is failing - the Azul layout engine produces different output than \
         Chrome.\n\n",
    );

    // User question if provided
    if let Some(ref question) = config.question {
        prompt.push_str(&format!("## Specific Question\n\n{}\n\n", question));
    }

    // Test information
    prompt.push_str(&format!("## Test: {}\n\n", debug_data.test_name));

    if let Some(diff) = debug_data.diff_count {
        prompt.push_str(&format!("**Pixel difference count:** {}\n\n", diff));
    }

    // Screenshots (embedded as base64 for Gemini multimodal)
    prompt.push_str("## Visual Comparison\n\n");
    prompt
        .push_str("Below are the screenshots from Chrome (reference) and Azul (implementation).\n");
    prompt.push_str("Compare them visually to identify layout differences.\n\n");

    if let Some(ref chrome_base64) = debug_data.chrome_image_base64 {
        prompt.push_str("### Chrome Reference Screenshot\n\n");
        prompt.push_str(&format!(
            "![Chrome Reference](data:image/png;base64,{})\n\n",
            chrome_base64
        ));
    }

    if let Some(ref azul_base64) = debug_data.azul_image_base64 {
        prompt.push_str("### Azul Implementation Screenshot\n\n");
        prompt.push_str(&format!(
            "![Azul Implementation](data:image/webp;base64,{})\n\n",
            azul_base64
        ));
    }

    // XHTML Source
    prompt.push_str("## XHTML Source\n\n```xml\n");
    prompt.push_str(&debug_data.xhtml_source);
    prompt.push_str("\n```\n\n");

    // CSS Warnings
    if !debug_data.css_warnings.is_empty() {
        prompt.push_str("## CSS Warnings\n\n```\n");
        for warning in &debug_data.css_warnings {
            prompt.push_str(warning);
            prompt.push('\n');
        }
        prompt.push_str("```\n\n");
    }

    // Layout Debug Messages
    if !debug_data.layout_debug_messages.is_empty() {
        prompt.push_str("## Layout Debug Messages\n\n```\n");
        for msg in &debug_data.layout_debug_messages {
            prompt.push_str(msg);
            prompt.push('\n');
        }
        prompt.push_str("```\n\n");
    }

    // Chrome Layout Data
    if let Some(ref chrome_data) = debug_data.chrome_layout_data {
        prompt.push_str("## Chrome Reference Layout (JSON)\n\n```json\n");
        prompt.push_str(chrome_data);
        prompt.push_str("\n```\n\n");
    }

    // Azul Display List
    if let Some(ref display_list) = debug_data.azul_display_list {
        prompt.push_str("## Azul Display List\n\n```\n");
        // Truncate if too long
        if display_list.len() > 50000 {
            prompt.push_str(&display_list[..50000]);
            prompt.push_str("\n... (truncated)\n");
        } else {
            prompt.push_str(display_list);
        }
        prompt.push_str("\n```\n\n");
    }

    // Source Code
    prompt.push_str("## Source Code Reference\n\n");
    prompt.push_str("Below is the relevant source code from the layout engine implementation.\n\n");

    for (path, content) in source_files {
        prompt.push_str(&format!("### {}\n\n```rust\n", path));
        prompt.push_str(content);
        prompt.push_str("\n```\n\n");
    }

    // Instructions
    prompt.push_str("## Task\n\n");
    prompt.push_str(
        "**IMPORTANT: If the Chrome and Azul screenshots look identical (test passes), respond \
         with just: SUCCESS**\n\n",
    );
    prompt.push_str("Otherwise, if there are differences:\n");
    prompt
        .push_str("1. Compare the Chrome and Azul screenshots visually to identify differences.\n");
    prompt.push_str(
        "2. Analyze the Chrome reference layout JSON and compare it to Azul's display list.\n",
    );
    prompt.push_str("3. Identify specific differences in positioning, sizing, or styling.\n");
    prompt.push_str(
        "4. Trace through the source code to find the likely cause of the discrepancy.\n",
    );
    prompt.push_str("5. Suggest specific code changes to fix the issue.\n\n");
    prompt.push_str("Please provide:\n");
    prompt.push_str("- A clear explanation of what's wrong\n");
    prompt.push_str("- The specific file and function where the bug likely exists\n");
    prompt.push_str("- A code fix or approach to resolve the issue\n");

    Ok(prompt)
}
