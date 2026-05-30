mod agentic;
mod apidocs;
pub mod blog;
pub mod donate;
mod guide;
mod search;
use std::{collections::BTreeMap, path::Path};

use serde_derive::{Deserialize, Serialize};

use crate::api::{ApiData, Language, LoadedExample};

const HTML_ROOT: &str = "https://azul.rs";

/// Generate all documentation files
///
/// # Arguments
/// * `inline_css` - If true, CSS will be inlined into index.html to prevent FOUC.
///                  If false, only a link to main.css is used (faster for development).
/// * `hostname` - Base URL used to interpolate `$HOSTNAME` markers inside
///                installation commands. Production: `https://azul.rs`;
///                debug deploy: `http://localhost:8000`.
pub fn generate_docs(
    api_data: &ApiData,
    imageoutput_path: &Path,
    imageoutput_url: &str,
    inline_css: bool,
    hostname: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    let mut docs = BTreeMap::new();

    // Generate main index.html
    docs.insert(
        "index.html".to_string(),
        generate_index_html(&api_data, imageoutput_path, imageoutput_url, inline_css, hostname)?,
    );

    // Generate API documentation for each version
    for version in api_data.get_sorted_versions() {
        let api_html = apidocs::generate_api_html(api_data, &version);
        docs.insert(format!("api/{}.html", version), api_html);

        // Per-version client-side search index. Consumed by azul-search.js
        // and small enough (~tens of KB gzipped) that we inline-load it on
        // first focus rather than streaming over range requests.
        if let Some(version_data) = api_data.get_version(&version) {
            let json = search::generate_search_index(&version, version_data);
            docs.insert(format!("api/{}.search.json", version), json);
        }
    }

    // Manifest so the search panel can auto-discover the latest version
    // without every page having to know the current version string.
    if let Some(latest) = api_data.get_latest_version_str() {
        let versions = api_data.get_sorted_versions();
        let manifest =
            serde_json::json!({ "latest": latest, "versions": versions }).to_string();
        docs.insert("api/index.json".to_string(), manifest);
    }

    // Generate combined API page
    docs.insert(
        "api.html".to_string(),
        apidocs::generate_api_index(api_data),
    );

    let latest_version = api_data.get_latest_version_str().unwrap();

    // Generate guide pages (version-agnostic, only one master version).
    // Each page ships as both `.html` and `.md` at the same URL stem so
    // readers (and tooling) can fetch the raw markdown directly.
    for guide in guide::get_guide_list() {
        let guide_html = guide::generate_guide_html(&guide, latest_version);
        docs.insert(format!("guide/{}.html", guide.file_name), guide_html);
        docs.insert(format!("guide/{}.md", guide.file_name), guide.content.clone());
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

    // Agentic release bundle: artefacts that let a coding agent write
    // high-quality azul apps. Built from the same guide list + api_data so
    // they stay in sync with the rest of the site.
    docs.insert("llms.txt".to_string(), agentic::generate_llms_txt(api_data));
    docs.insert(
        "llms-full.txt".to_string(),
        agentic::generate_llms_full_txt(),
    );
    let skill = agentic::generate_skill_md(api_data);
    docs.insert("skill.md".to_string(), skill.clone());
    docs.insert(".well-known/azul-skill.md".to_string(), skill);

    Ok(docs)
}

/// Languages always shown inline (above the fold). We ship 11 solid bindings,
/// so ALL of them are primary tabs now — there is no "more languages" overflow.
const PRIMARY_LANGUAGES: &[&str] = &[
    "rust", "python", "c", "cpp", "csharp", "java", "kotlin", "lua", "ruby", "node", "ocaml",
];

/// Whitelist of languages that have a SOLID, working hello-world and may
/// appear on the azul.rs frontpage install tabs. Every other binding still
/// lives in `examples/` and in api.json's `languages` data (so the data is
/// preserved and codegen still runs for them) — they are just NOT surfaced
/// on the frontpage so a visitor isn't confused by a half-working binding.
///
/// `cpp` is the dialect *group*; its per-standard variants (cpp03 … cpp23)
/// are listed too because the C++ dropdown needs them in the installation
/// JSON to populate the version selector. The variants are never rendered as
/// their own tab (they carry `dialectOf: "cpp"`), only as dropdown options.
///
/// This is the single source of truth: both the server-rendered tab HTML
/// (`generate_language_tabs_html`) and the client-side installation JSON
/// (`generate_installation_json`) filter against it, so even if api.json's
/// `tabOrder` drifts to include a non-whitelisted language, the frontpage
/// stays restricted to this set.
const FRONTPAGE_LANGUAGES: &[&str] = &[
    "python", "c", "cpp", "rust", "csharp", "java", "kotlin", "lua", "ruby", "node", "ocaml",
    // C++ dialect variants — dropdown options only, never standalone tabs.
    "cpp03", "cpp11", "cpp14", "cpp17", "cpp20", "cpp23",
];

/// True if `lang` is allowed on the frontpage (see [`FRONTPAGE_LANGUAGES`]).
fn is_frontpage_language(lang: &str) -> bool {
    FRONTPAGE_LANGUAGES.contains(&lang)
}

/// Generate the HTML for language tabs based on tabOrder configuration.
///
/// Renders the four primary languages as flat buttons; the rest go into a
/// `<details>` wrapper that the user can expand. Dialect groups (e.g. C++)
/// are always rendered as a single dropdown regardless of which row they
/// land in. The `<details>` is part of the same `.lang-grid` so clicking
/// inside it doesn't change the language unless the user chooses one.
///
/// Only languages in [`FRONTPAGE_LANGUAGES`] are ever rendered, even if
/// `tabOrder` lists more — non-whitelisted entries are skipped (NOT appended),
/// so a half-working binding can't leak onto the frontpage.
fn generate_language_tabs_html(installation: &crate::api::Installation) -> String {
    // Use tabOrder if specified, otherwise use default order. Either way,
    // restrict to the frontpage whitelist so broken bindings stay hidden.
    let tab_order: Vec<String> = if installation.tab_order.is_empty() {
        PRIMARY_LANGUAGES.iter().map(|s| s.to_string()).collect()
    } else {
        installation.tab_order.clone()
    };
    let tab_order: Vec<String> = tab_order
        .into_iter()
        .filter(|lang| is_frontpage_language(lang))
        .collect();

    let render_lang_button = |lang: &str| -> Option<String> {
        if let Some(dialect) = installation.dialects.get(lang) {
            let default_variant = &dialect.default;
            let mut variants: Vec<_> = dialect.variants.iter().collect();
            // Reverse sort so newest dialect (e.g. cpp23) is first.
            variants.sort_by(|a, b| b.0.cmp(a.0));
            let mut options_html = String::new();
            for (var_key, var_config) in variants {
                options_html.push_str(&format!(
                    "<option value=\"{}\"{}>{}</option>",
                    var_key,
                    if var_key == default_variant { " selected" } else { "" },
                    var_config.display_name
                ));
            }
            Some(format!(
                r#"<div class="lang-tab-dropdown" data-lang="{}">
                    <select class="dialect-select" onchange="selectLanguage(this.value)">{}</select>
                </div>"#,
                lang, options_html
            ))
        } else if let Some(lang_config) = installation.languages.get(lang) {
            if lang_config.dialect_of.is_some() {
                return None; // handled by the parent dialect group
            }
            Some(format!(
                r#"<button data-lang="{}" onclick="selectLanguage('{}')">{}</button>"#,
                lang, lang, lang_config.display_name
            ))
        } else {
            None
        }
    };

    let mut primary_tabs = Vec::new();
    let mut overflow_tabs = Vec::new();
    for lang in &tab_order {
        let html = match render_lang_button(lang) {
            Some(s) => s,
            None => continue,
        };
        if PRIMARY_LANGUAGES.iter().any(|p| p == lang) {
            primary_tabs.push(html);
        } else {
            overflow_tabs.push(html);
        }
    }

    let mut out = primary_tabs.join("\n        ");
    if !overflow_tabs.is_empty() {
        out.push_str(&format!(
            "\n        <details class=\"lang-more\"><summary>more languages…</summary>\n        \
             <div class=\"lang-more-grid\">\n        {}\n        </div></details>",
            overflow_tabs.join("\n        ")
        ));
    }
    out
}

/// Rendered example with all code variants for JavaScript.
///
/// Code fields are stored RAW (not HTML-escaped) for JSON serialization;
/// use `escape_code()` when inserting into HTML templates.
///
/// The named `code_*` fields cover the languages baked into the renderer
/// (`c`, `rust`, `python`, the C++ standards). Every other language declared
/// in api.json's `code` block (ada, csharp, lua, ruby, ...) is surfaced via
/// the flattened `code_extra` map — each entry serializes as `code_<lang>`,
/// matching the `examples[id]['code_' + currentLang]` lookup the index JS
/// already does.
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
    /// All other languages (`code_<lang>`) — flattened so the JS lookup
    /// `examples[id]['code_' + currentLang]` works without renaming.
    #[serde(flatten)]
    code_extra: BTreeMap<String, String>,
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

        // Promote every extra language (ada, csharp, lua, ...) to a
        // `code_<lang>` key so the JS picks it up without any extra plumbing.
        let code_extra: BTreeMap<String, String> = e
            .code
            .extra
            .iter()
            .map(|(lang, bytes)| {
                (
                    format!("code_{lang}"),
                    String::from_utf8_lossy(bytes).to_string(),
                )
            })
            .collect();

        ExampleRendered {
            id: name.clone(),
            title: e.title.join("<br>"), // Join multiline titles with <br>
            description: comrak::markdown_to_html(
                &guide::transform_german_quotes(&e.description.join("\r\n")),
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
            code_extra,
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
    hostname: &str,
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
        .replace(
            "<!-- PRISM_SCRIPT -->",
            &format!("{}\n{}", get_prism_script(), get_search_init(PageKind::Other)),
        );

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
        generate_installation_json(&latest_version.installation, latest_version_str, hostname);

    Ok(index_html_template
        .replace("$$INDEX_SECTION_EXAMPLES$$", &examples_html)
        .replace("$$JAVASCRIPT_EXAMPLES$$", &ex_json)
        .replace("$$JAVASCRIPT_INSTALLATION$$", &installation_json)
        .replace("$$LATEST_VERSION$$", latest_version_str)
        .replace("$$LATEST_DATE$$", &latest_version_date))
}

/// Generate JavaScript-compatible installation instructions
fn generate_installation_json(
    installation: &crate::api::Installation,
    version: &str,
    hostname: &str,
) -> String {
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

    // Convert dialects. Only whitelisted dialect groups (e.g. `cpp`) are
    // emitted so the frontpage install panel matches the rendered tabs.
    let mut dialects = BTreeMap::new();
    for (key, dialect) in &installation.dialects {
        if !is_frontpage_language(key) {
            continue;
        }
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

    // Convert languages. Restrict to the frontpage whitelist (including the
    // cpp dialect variants the dropdown needs) so no broken-binding install
    // steps ship to the frontpage. The full `languages` data still lives in
    // api.json and still drives codegen — this only trims the frontpage JSON.
    let mut languages = BTreeMap::new();
    for (lang_key, lang_config) in &installation.languages {
        if !is_frontpage_language(lang_key) {
            continue;
        }
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
        // Mirror the server-rendered tab filter: frontpage whitelist only.
        tab_order: installation
            .tab_order
            .iter()
            .filter(|lang| is_frontpage_language(lang))
            .cloned()
            .collect(),
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

/// CSS + JS that turns every `<h1>` … `<h4>` with an inner
/// `<a class="anchor" id="...">` (the comrak-emitted slug) into a
/// click target. Clicking the heading scrolls to it, updates
/// `location.hash`, and copies the absolute URL to the clipboard so
/// readers can paste a deep link straight into chat. A "#" glyph
/// fades in on hover to signal the affordance, plus a small toast
/// confirms the copy.
pub fn get_anchor_link_script() -> String {
    r##"<style>
.center main h1, .center main h2, .center main h3, .center main h4 {
  position: relative;
}
.center main h1 .anchor, .center main h2 .anchor,
.center main h3 .anchor, .center main h4 .anchor {
  position: absolute;
  left: -1em;
  top: 0;
  bottom: 0;
  width: 1em;
  display: flex;
  align-items: center;
  justify-content: flex-start;
  color: #aaa;
  text-decoration: none;
  opacity: 0;
  transition: opacity 0.12s ease;
  font-weight: normal;
}
.center main h1:hover .anchor, .center main h2:hover .anchor,
.center main h3:hover .anchor, .center main h4:hover .anchor {
  opacity: 1;
}
.center main h1 .anchor::before, .center main h2 .anchor::before,
.center main h3 .anchor::before, .center main h4 .anchor::before {
  content: "#";
  font-size: 0.7em;
}
.azs-deeplink-toast {
  position: fixed;
  bottom: 24px;
  left: 50%;
  transform: translateX(-50%);
  background: rgba(0, 0, 0, 0.82);
  color: #fff;
  font-family: "Rubik", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  font-size: 13px;
  padding: 8px 14px;
  border-radius: 6px;
  z-index: 10000;
  pointer-events: none;
  opacity: 0;
  transition: opacity 0.12s ease;
}
.azs-deeplink-toast[data-visible="true"] { opacity: 1; }
@media (prefers-color-scheme: dark) {
  .center main h1 .anchor, .center main h2 .anchor,
  .center main h3 .anchor, .center main h4 .anchor { color: #888; }
}
</style>
<script>
document.addEventListener('DOMContentLoaded', function () {
  // Comrak emits each heading as
  //   <h2><a class="anchor" id="slug" aria-hidden="true"></a>Title</h2>
  // The empty anchor is positioned to the left of the heading by the
  // CSS above and acts as the visible click target. We also make the
  // *whole heading* clickable for big-fingers users — clicking either
  // updates location.hash, scrolls smoothly, and copies the deep link
  // to the clipboard.
  var headings = document.querySelectorAll(
    '.center main h1 > a.anchor[id], .center main h2 > a.anchor[id], ' +
    '.center main h3 > a.anchor[id], .center main h4 > a.anchor[id]'
  );
  if (headings.length === 0) return;

  // Make sure the empty anchor element is still keyboard-focusable.
  // Comrak marks it aria-hidden="true" which screen readers honor;
  // sighted keyboard users still need tab access for deep links.
  headings.forEach(function (a) {
    a.setAttribute('href', '#' + a.id);
    a.setAttribute('aria-label', 'Link to this section');
    a.removeAttribute('aria-hidden');
  });

  var toast;
  function showToast(msg) {
    if (!toast) {
      toast = document.createElement('div');
      toast.className = 'azs-deeplink-toast';
      toast.setAttribute('role', 'status');
      document.body.appendChild(toast);
    }
    toast.textContent = msg;
    toast.dataset.visible = 'true';
    clearTimeout(toast._timer);
    toast._timer = setTimeout(function () {
      toast.dataset.visible = 'false';
    }, 1400);
  }

  function copyDeepLink(id) {
    var url = window.location.origin + window.location.pathname + '#' + id;
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(url).then(
        function () { showToast('Link copied'); },
        function () { showToast('Link: ' + url); }
      );
    } else {
      showToast('Link: ' + url);
    }
  }

  // Click on the heading body (not on any link inside it) deep-links.
  document.querySelectorAll('.center main h1, .center main h2, .center main h3, .center main h4')
    .forEach(function (h) {
      var anchor = h.querySelector(':scope > a.anchor[id]');
      if (!anchor) return;
      h.style.cursor = 'pointer';
      h.addEventListener('click', function (ev) {
        // Don't intercept clicks on links *inside* the heading text.
        if (ev.target !== h && ev.target !== anchor) {
          var t = ev.target;
          while (t && t !== h) {
            if (t.tagName === 'A') return;
            t = t.parentNode;
          }
        }
        ev.preventDefault();
        history.replaceState(null, '', '#' + anchor.id);
        copyDeepLink(anchor.id);
        // Smooth-scroll the heading into view; the empty anchor has
        // no height so scrollIntoView wouldn't land on it.
        h.scrollIntoView({ behavior: 'smooth', block: 'start' });
      });
    });
});
</script>"##.to_string()
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

      <link rel='preload' as='font' href='{base_url}/fonts/Rubik-VariableFont_wght.ttf' type='font/ttf' crossorigin='anonymous'>
      <link rel='preload' as='font' href='{base_url}/fonts/PlayfairDisplay-VariableFont_wght.ttf' type='font/ttf' crossorigin='anonymous'>
      <link rel='shortcut icon' type='image/x-icon' href='{base_url}/favicon.ico'>
      <link rel='stylesheet' href='https://cdn.jsdelivr.net/npm/prismjs@1.29.0/themes/prism.min.css'>
      <link rel='stylesheet' href='{base_url}/azul-search.css'>
      {css_tag}
      {anchor_link}
      <!-- TEMPORARY doc-review tool (remove this line + azul-review.js in a later release) -->
      <script defer src='{base_url}/azul-review.js'></script>
    ", base_url=base_url, css_tag=css_tag, anchor_link=get_anchor_link_script())
}

/// Script tag + init for the search panel.
///
/// `page_kind` controls behavior the JS layer can't infer:
///   - `Api`     — clicking a result stays on the same page (anchor jump).
///                 Searches the API index only.
///   - `Guide`   — pagefind-only search over guide content. Defaults are
///                 frontmatter-driven entries shown when the input is empty.
///                 Clicking opens the api page in a new tab.
///   - `Other`   — clicking navigates the same tab. Searches the API index.
///
/// If a page contains an element with id `azul-search-mount`, the JS will
/// render an inline search bar at that location; otherwise it falls back
/// to a floating pill in the corner.
pub enum PageKind<'a> {
    Api,
    Guide(&'a [String]),
    Other,
}

pub fn get_search_init(kind: PageKind<'_>) -> String {
    // Guide pages search guide content via pagefind only. API search lives
    // on the api page itself; guide readers don't need symbol search to
    // intrude on tutorial reading.
    let (on_api, link_target, defaults_json, source_json, placeholder) = match kind {
        PageKind::Api => (
            true,
            "_self",
            String::from("[]"),
            r#"{ type: 'api-default' }"#.to_string(),
            "Search API",
        ),
        PageKind::Guide(defaults) => (
            false,
            "_blank",
            serde_json::to_string(defaults).unwrap_or_else(|_| "[]".to_string()),
            r#"{ type: 'pagefind', url: '/pagefind/' }"#.to_string(),
            "Search guide",
        ),
        PageKind::Other => (
            false,
            "_self",
            String::from("[]"),
            r#"{ type: 'api-default' }"#.to_string(),
            "Search API",
        ),
    };

    format!(
        r#"<script src="/azul-search.js" defer></script>
<script>
document.addEventListener('DOMContentLoaded', function () {{
  if (!window.AzulSearch) return;
  var mount = document.getElementById('azul-search-mount');
  var opts = {{
    source: {source_json},
    onApiPage: {on_api},
    linkTarget: '{link_target}',
    defaults: {defaults_json},
    placeholder: '{placeholder}',
  }};
  if (mount) {{
    opts.mount = mount;
    opts.inline = true;
    window.AzulSearch.mount(opts);
  }} else {{
    window.AzulSearch.attach(opts);
  }}
}});
</script>"#,
        source_json = source_json,
        on_api = on_api,
        link_target = link_target,
        defaults_json = defaults_json,
        placeholder = placeholder,
    )
}

pub fn get_sidebar() -> String {
    format!(
        "
        <nav>
        <ul class='nav-grid'>
          <li><a href='https://azul.rs'>overview</a></li>
          <li><a href='https://azul.rs/releases'>releases</a></li>
          <li><a href='https://github.com/fschutt/azul'>code</a></li>
          <li><a href='https://discord.gg/V96ZGKqQvn'>discord</a></li>
          <li><a href='https://azul.rs/guide'>guide</a></li>
          <li><a href='https://azul.rs/api'>api</a></li>
          <li><a href='https://azul.rs/reftest'>reftests</a></li>
          <li><a href='https://azul.rs/blog'>blog</a></li>
          <li><a href='https://azul.rs/donate'>donate</a></li>
        </ul>
      </nav>
    "
    )
}
