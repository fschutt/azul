mod apidocs;
pub mod blog;
pub mod donate;
mod guide;
use std::{collections::BTreeMap, path::Path};

use serde_derive::{Deserialize, Serialize};

use crate::api::{ApiData, Language, LoadedExample};

const HTML_ROOT: &str = "https://azul.rs";

/// Generate all documentation files
///
/// # Arguments
/// * `inline_css` - If true, CSS will be inlined into index.html to prevent FOUC.
///                  If false, only a link to main.css is used (faster for development).
pub fn generate_docs(
    api_data: &ApiData,
    imageoutput_path: &Path,
    imageoutput_url: &str,
    inline_css: bool,
) -> anyhow::Result<BTreeMap<String, String>> {
    let mut docs = BTreeMap::new();

    // Generate main index.html
    docs.insert(
        "index.html".to_string(),
        generate_index_html(&api_data, imageoutput_path, imageoutput_url, inline_css)?,
    );

    // Generate API documentation for each version
    for version in api_data.get_sorted_versions() {
        let api_html = apidocs::generate_api_html(api_data, &version);
        docs.insert(format!("api/{}.html", version), api_html);
    }

    // Generate combined API page
    docs.insert(
        "api.html".to_string(),
        apidocs::generate_api_index(api_data),
    );

    let latest_version = api_data.get_latest_version_str().unwrap();

    // Generate guide pages (version-agnostic, only one master version)
    for guide in guide::get_guide_list() {
        let guide_html = guide::generate_guide_html(&guide, latest_version);
        docs.insert(format!("guide/{}.html", guide.file_name), guide_html);
    }

    // Generate combined guide page
    docs.insert(
        "guide.html".to_string(),
        guide::generate_guide_mainpage(latest_version),
    );

    // Generate blog posts
    for post in blog::get_blog_list() {
        let post_html = blog::generate_blog_post_html(&post);
        docs.insert(format!("blog/{}.html", post.file_name), post_html);
    }

    // Generate blog index page
    docs.insert("blog.html".to_string(), blog::generate_blog_index());

    Ok(docs)
}

/// Generate the HTML for language tabs based on tabOrder configuration
fn generate_language_tabs_html(installation: &crate::api::Installation) -> String {
    let mut tabs = Vec::new();

    // Use tabOrder if specified, otherwise use default order
    let tab_order: Vec<String> = if installation.tab_order.is_empty() {
        // Default order: rust, python, cpp, c
        vec![
            "rust".to_string(),
            "python".to_string(),
            "cpp".to_string(),
            "c".to_string(),
        ]
    } else {
        installation.tab_order.clone()
    };

    for lang in &tab_order {
        // Check if this is a dialect group (has variants)
        if let Some(dialect) = installation.dialects.get(lang) {
            // Generate dropdown for dialect - user must select a variant
            let default_variant = &dialect.default;

            // Build options HTML for the dropdown - sorted by version (newest first)
            let mut variants: Vec<_> = dialect.variants.iter().collect();
            variants.sort_by(|a, b| b.0.cmp(a.0)); // Reverse sort: cpp23 > cpp20 > cpp17...

            let mut options_html = String::new();
            for (var_key, var_config) in variants {
                options_html.push_str(&format!(
                    "<option value=\"{}\"{}>{}</option>",
                    var_key,
                    if var_key == default_variant {
                        " selected"
                    } else {
                        ""
                    },
                    var_config.display_name
                ));
            }

            // Dropdown-only tab for dialects (no separate button - select IS the tab)
            tabs.push(format!(
                r#"<div class="lang-tab-dropdown" data-lang="{}">
                    <select class="dialect-select" onchange="selectLanguage(this.value)">{}</select>
                </div>"#,
                lang, options_html
            ));
        } else if let Some(lang_config) = installation.languages.get(lang) {
            // Skip dialect variants - they're handled by the parent dialect group
            if lang_config.dialect_of.is_some() {
                continue;
            }
            // Simple button for non-dialect languages
            tabs.push(format!(
                r#"<button data-lang="{}" onclick="selectLanguage('{}')">{}</button>"#,
                lang, lang, lang_config.display_name
            ));
        }
    }

    tabs.join("\n        ")
}

/// Rendered example with all code variants for JavaScript
/// Note: code fields are stored RAW (not HTML-escaped) for JSON serialization
/// Use escape_code() when inserting into HTML templates
#[derive(Debug, Serialize, Deserialize, Clone)]
struct ExampleRendered {
    id: String,
    #[serde(skip)]
    title: String, // Joined with <br> for multiline display
    #[serde(skip)]
    description: String,
    alt: String,
    #[serde(rename = "showOnIndex")]
    show_on_index: bool,
    #[serde(skip)]
    screenshot_windows: String,
    #[serde(skip)]
    screenshot_linux: String,
    #[serde(skip)]
    screenshot_mac: String,
    code_c: String,
    code_cpp: String,
    code_cpp03: String,
    code_cpp11: String,
    code_cpp14: String,
    code_cpp17: String,
    code_cpp20: String,
    code_cpp23: String,
    code_python: String,
    code_rust: String,
}

impl ExampleRendered {
    fn from_loaded(e: LoadedExample, imageoutput_path: &Path, imageoutput_url: &str) -> Self {
        let name = &e.name;

        // Write screenshot files
        let _ = std::fs::write(
            imageoutput_path.join(&format!("{name}.windows.png")),
            &e.screenshot.windows,
        );
        let _ = std::fs::write(
            imageoutput_path.join(&format!("{name}.linux.png")),
            &e.screenshot.linux,
        );
        let _ = std::fs::write(
            imageoutput_path.join(&format!("{name}.mac.png")),
            &e.screenshot.mac,
        );

        // Get C++ code for each version (fall back to legacy cpp if not available)
        // Note: store RAW code, not HTML-escaped - escape when inserting into HTML
        let get_cpp_code = |lang: Language| -> String {
            e.code
                .get(lang)
                .map(|b| String::from_utf8_lossy(b).to_string())
                .unwrap_or_else(|| String::from_utf8_lossy(&e.code.cpp).to_string())
        };

        ExampleRendered {
            id: name.clone(),
            title: e.title.join("<br>"), // Join multiline titles with <br>
            description: comrak::markdown_to_html(
                &e.description.join("\r\n"),
                &comrak::Options::default(),
            ),
            alt: e.alt.clone(),
            show_on_index: e.show_on_index,
            screenshot_windows: format!("{imageoutput_url}/{name}.windows.png"),
            screenshot_linux: format!("{imageoutput_url}/{name}.linux.png"),
            screenshot_mac: format!("{imageoutput_url}/{name}.mac.png"),
            code_c: String::from_utf8_lossy(&e.code.c).to_string(),
            code_cpp: String::from_utf8_lossy(e.code.get_cpp()).to_string(),
            code_cpp03: get_cpp_code(Language::Cpp03),
            code_cpp11: get_cpp_code(Language::Cpp11),
            code_cpp14: get_cpp_code(Language::Cpp14),
            code_cpp17: get_cpp_code(Language::Cpp17),
            code_cpp20: get_cpp_code(Language::Cpp20),
            code_cpp23: get_cpp_code(Language::Cpp23),
            code_python: String::from_utf8_lossy(&e.code.python).to_string(),
            code_rust: String::from_utf8_lossy(&e.code.rust).to_string(),
        }
    }
}

/// Generate the main index.html page - imageoutput_path is the folder where all the screenshots go
///
/// # Arguments
/// * `inline_css` - If true, CSS from main.css will be inlined into a <style> tag.
///                  If false, only a <link> to main.css is used.
fn generate_index_html(
    api_data: &ApiData,
    imageoutput_path: &Path,
    imageoutput_url: &str,
    inline_css: bool,
) -> anyhow::Result<String> {
    let latest_version_str = api_data.get_latest_version_str().unwrap();
    let latest_version = api_data.get_version(latest_version_str).unwrap();
    let latest_version_date = &latest_version.date;

    let imagepath = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/assets/screenshots"
    );
    let examples_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples");
    let imageoutput_path = Path::new(imageoutput_path);

    assert!(Path::new(imagepath).exists());
    assert!(Path::new(examples_path).exists());
    assert!(imageoutput_path.exists());

    let examples = latest_version
        .examples
        .iter()
        .map(|s| s.load(examples_path, &imagepath))
        .collect::<anyhow::Result<Vec<LoadedExample>>>()?;

    let ex: Vec<ExampleRendered> = examples
        .into_iter()
        .map(|e| ExampleRendered::from_loaded(e, imageoutput_path, imageoutput_url))
        .collect();

    // Filter examples for index display
    let index_examples: Vec<&ExampleRendered> = ex.iter().filter(|e| e.show_on_index).collect();

    let index_html_template = include_str!("../../templates/index.template.html")
        .replace("$$ROOT_RELATIVE$$", "https://azul.rs")
        .replace("<!-- HEAD -->", &get_common_head_tags(inline_css))
        .replace("<!-- SIDEBAR -->", &get_sidebar())
        .replace("<!-- PRISM_SCRIPT -->", &get_prism_script());

    // Generate language tabs HTML from configuration
    let language_tabs_html = generate_language_tabs_html(&latest_version.installation);

    let index_example_html_template = include_str!("../../templates/index.section.template.html")
        .replace("$$ROOT_RELATIVE$$", "https://azul.rs")
        .replace("$$LANGUAGE_TABS$$", &language_tabs_html);

    let examples_html = index_examples
        .iter()
        .enumerate()
        .map(|(idx, ex)| {
            let is_first = idx == 0;
            index_example_html_template
                .replace("$$EXAMPLE_TITLE$$", &ex.title)
                .replace("$$EXAMPLE_DESCRIPTION$$", &ex.description)
                .replace("$$EXAMPLE_ID$$", &ex.id)
                .replace("$$EXAMPLE_CODE$$", &escape_code(&ex.code_python))
                .replace("$$EXAMPLE_IMAGE_ALT$$", &ex.alt)
                .replace("$$EXAMPLE_IMAGE_SOURCE_LINUX$$", &ex.screenshot_linux)
                .replace("$$EXAMPLE_IMAGE_SOURCE_MAC$$", &ex.screenshot_mac)
                .replace("$$EXAMPLE_IMAGE_SOURCE_WINDOWS$$", &ex.screenshot_windows)
                .replace("$$IS_FIRST$$", if is_first { "true" } else { "false" })
                .replace(
                    "$$INSTALL_DISPLAY$$",
                    if is_first { "" } else { "display:none;" },
                )
        })
        .collect::<Vec<_>>()
        .join("\r\n");

    // Generate JSON with all examples (including C++ versions)
    let ex_json = serde_json::to_string(
        &ex.iter()
            .map(|s| (s.id.clone(), s))
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap_or_default();

    // Generate installation instructions JSON
    let installation_json =
        generate_installation_json(&latest_version.installation, latest_version_str);

    Ok(index_html_template
        .replace("$$INDEX_SECTION_EXAMPLES$$", &examples_html)
        .replace("$$JAVASCRIPT_EXAMPLES$$", &ex_json)
        .replace("$$JAVASCRIPT_INSTALLATION$$", &installation_json)
        .replace("$$LATEST_VERSION$$", latest_version_str)
        .replace("$$LATEST_DATE$$", &latest_version_date))
}

/// Generate JavaScript-compatible installation instructions
fn generate_installation_json(installation: &crate::api::Installation, version: &str) -> String {
    use crate::api::InstallationStep;

    #[derive(Serialize)]
    struct InstallationConfig {
        version: String,
        hostname: String,
        /// Order of language tabs
        #[serde(rename = "tabOrder")]
        tab_order: Vec<String>,
        /// Dialect groups (e.g., cpp -> { displayName, default, variants })
        dialects: BTreeMap<String, DialectJson>,
        /// Language configurations
        languages: BTreeMap<String, LanguageInstall>,
    }

    #[derive(Serialize)]
    struct DialectJson {
        #[serde(rename = "displayName")]
        display_name: String,
        default: String,
        variants: BTreeMap<String, VariantJson>,
    }

    #[derive(Serialize)]
    struct VariantJson {
        #[serde(rename = "displayName")]
        display_name: String,
        #[serde(rename = "altText")]
        alt_text: String,
    }

    #[derive(Serialize)]
    struct LanguageInstall {
        #[serde(rename = "displayName")]
        display_name: String,
        /// If this is a dialect of another language group
        #[serde(rename = "dialectOf", skip_serializing_if = "Option::is_none")]
        dialect_of: Option<String>,
        /// Available methods for this language (e.g., ["pip", "uv"] for Python)
        #[serde(skip_serializing_if = "Vec::is_empty")]
        methods: Vec<String>,
        /// Steps per method (if methods are available)
        #[serde(rename = "methodSteps", skip_serializing_if = "BTreeMap::is_empty")]
        method_steps: BTreeMap<String, Vec<StepJson>>,
        /// Platform-specific steps
        #[serde(skip_serializing_if = "Option::is_none")]
        windows: Option<Vec<StepJson>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        linux: Option<Vec<StepJson>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        macos: Option<Vec<StepJson>>,
    }

    #[derive(Serialize, Clone)]
    struct StepJson {
        #[serde(rename = "type")]
        step_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        content: String,
    }

    fn convert_steps(steps: &[InstallationStep], hostname: &str, version: &str) -> Vec<StepJson> {
        steps
            .iter()
            .map(|step| {
                let interpolated = step.interpolate(hostname, version);
                match interpolated {
                    InstallationStep::Code { language, content } => StepJson {
                        step_type: "code".to_string(),
                        language: Some(language),
                        content,
                    },
                    InstallationStep::Command { content } => StepJson {
                        step_type: "command".to_string(),
                        language: None,
                        content,
                    },
                    InstallationStep::Text { content } => StepJson {
                        step_type: "text".to_string(),
                        language: None,
                        content,
                    },
                }
            })
            .collect()
    }

    let hostname = HTML_ROOT;

    // Convert dialects
    let mut dialects = BTreeMap::new();
    for (key, dialect) in &installation.dialects {
        let mut variants = BTreeMap::new();
        for (var_key, var) in &dialect.variants {
            variants.insert(
                var_key.clone(),
                VariantJson {
                    display_name: var.display_name.clone(),
                    alt_text: var.alt_text.clone(),
                },
            );
        }
        dialects.insert(
            key.clone(),
            DialectJson {
                display_name: dialect.display_name.clone(),
                default: dialect.default.clone(),
                variants,
            },
        );
    }

    // Convert languages
    let mut languages = BTreeMap::new();
    for (lang_key, lang_config) in &installation.languages {
        let methods: Vec<String> = lang_config
            .methods
            .as_ref()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();

        let mut method_steps = BTreeMap::new();
        if let Some(methods_map) = &lang_config.methods {
            for (method_key, method_config) in methods_map {
                method_steps.insert(
                    method_key.clone(),
                    convert_steps(&method_config.steps, hostname, version),
                );
            }
        }

        let (windows, linux, macos) = if let Some(platforms) = &lang_config.platforms {
            (
                platforms
                    .get("windows")
                    .map(|s| convert_steps(&s.steps, hostname, version)),
                platforms
                    .get("linux")
                    .map(|s| convert_steps(&s.steps, hostname, version)),
                platforms
                    .get("macos")
                    .map(|s| convert_steps(&s.steps, hostname, version)),
            )
        } else {
            (None, None, None)
        };

        languages.insert(
            lang_key.clone(),
            LanguageInstall {
                display_name: lang_config.display_name.clone(),
                dialect_of: lang_config.dialect_of.clone(),
                methods,
                method_steps,
                windows,
                linux,
                macos,
            },
        );
    }

    let config = InstallationConfig {
        version: version.to_string(),
        hostname: hostname.to_string(),
        tab_order: installation.tab_order.clone(),
        dialects,
        languages,
    };

    serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_string())
}

fn escape_code(s: &str) -> String {
    s.replace("<", "&lt;").replace(">", "&gt;")
}

/// Get the Prism.js syntax highlighting script tag.
/// Uses CDN-hosted Prism with autoloader for automatic language loading.
/// Should be included at the end of the body for code highlighting.
pub fn get_prism_script() -> String {
    r#"<script src="https://cdn.jsdelivr.net/npm/prismjs@1.29.0/components/prism-core.min.js"></script>
<script src="https://cdn.jsdelivr.net/npm/prismjs@1.29.0/plugins/autoloader/prism-autoloader.min.js"></script>"#.to_string()
}

/// Generate common head tags for HTML pages.
///
/// # Arguments
/// * `inline_css` - If true, the CSS from main.css is inlined in a <style> tag
///                  to prevent flash of unstyled content (FOUC).
///                  If false, only a <link> to main.css is used (faster for development).
pub fn get_common_head_tags(inline_css: bool) -> String {
    // Base URL - use absolute paths for both production and development
    // This ensures subpages like /blog/foo.html correctly reference /fonts, /main.css etc.
    let base_url = if inline_css {
        "https://azul.rs"
    } else {
        "" // Root-relative paths like /fonts/..., /main.css
    };

    let css_tag = if inline_css {
        // Read and inline the CSS file to prevent FOUC
        let css_content = include_str!("../../templates/main.css");
        format!("<style>\n{}\n</style>", css_content)
    } else {
        // Link to local stylesheet for development (main.css is copied to deploy folder)
        "<link rel='stylesheet' type='text/css' href='/main.css'>".to_string()
    };

    format!("
      <meta charset='utf-8'/>
      <meta name='viewport' content='width=device-width, initial-scale=1'>
      <meta http-equiv='Content-Type' content='text/html; charset=utf-8'/>
      <meta name='description' content='Cross-platform MIT-licensed desktop GUI framework for C and Rust using the Mozilla WebRender rendering engine'>
      <meta name='keywords' content='gui, rust, user interface'>
  
      <link rel='preload' as='font' href='{base_url}/fonts/SourceSerifPro-Regular.ttf' type='font/ttf' crossorigin='anonymous'>
      <link rel='preload' as='font' href='{base_url}/fonts/InstrumentSerif-Regular.ttf' type='font/ttf' crossorigin='anonymous'>
      <link rel='shortcut icon' type='image/x-icon' href='{base_url}/favicon.ico'>
      <link rel='stylesheet' href='https://cdn.jsdelivr.net/npm/prismjs@1.29.0/themes/prism.min.css'>
      {css_tag}
    ", base_url=base_url, css_tag=css_tag)
}

pub fn get_sidebar() -> String {
    format!(
        "
        <nav>
        <ul class='nav-grid'>
          <li><a href='https://azul.rs'>overview</a></li>
          <li><a href='https://azul.rs/releases.html'>releases</a></li>
          <li><a href='https://github.com/fschutt/azul'>code</a></li>
          <li><a href='https://discord.gg/V96ZGKqQvn'>discord</a></li>
          <li><a href='https://azul.rs/guide.html'>guide</a></li>
          <li><a href='https://azul.rs/api.html'>api</a></li>
          <li><a href='https://azul.rs/reftest.html'>reftests</a></li>
          <li><a href='https://azul.rs/blog.html'>blog</a></li>
          <li><a href='https://azul.rs/donate.html'>donate</a></li>
        </ul>
      </nav>
    "
    )
}
