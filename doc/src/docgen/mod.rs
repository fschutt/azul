mod agentic;
mod apidocs;
pub mod blog;
pub mod donate;
pub mod guide;
mod search;
use std::{collections::BTreeMap, path::Path};

use serde_derive::{Deserialize, Serialize};

use crate::api::{ApiData, Language, LoadedExample};

// Site path configuration. To rename azul.rs -> azlin.io, or relocate the docs
// under a different sub-path, change SITE_ROOT / UI_PATH here: every asset URL,
// sidebar link, search source and font path derives from them, so a move never
// silently breaks the CSS/asset links again. (HTML_ROOT is kept as the
// fully-qualified docs root; the test below guards it against drift.)
pub const SITE_ROOT: &str = "https://azul.rs";
/// Sub-path the docs/UI site is served under. The marketing landing lives at the
/// domain root; the whole generated docs site lives under SITE_ROOT + UI_PATH.
pub const UI_PATH: &str = "/ui";
/// Fully-qualified root of the generated docs/UI site (`SITE_ROOT` + `UI_PATH`).
/// Every docs page, release-asset link and search source must build its URLs
/// from this so the /ui move (and any future relocation) can't silently
/// re-break cross-page links — see deploy.rs / regression.rs which import it.
pub const HTML_ROOT: &str = "https://azul.rs/ui";

#[test]
fn html_root_matches_site_and_ui() {
    assert_eq!(HTML_ROOT, format!("{SITE_ROOT}{UI_PATH}"));
}

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

    // The guide's own search index. Same shape as the api one, so the
    // existing `api-index` adapter reads it unchanged - and the box that says
    // "Search guide" searches the guide.
    docs.insert(
        "guide/search.json".to_string(),
        search::generate_guide_index(&guide::get_guide_list()),
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

/// Languages always shown inline (above the fold). The original 11 solid
/// bindings stay flat; the languages promoted on 2026-07-04 (zig, go,
/// pascal, scala, fortran, haskell) land in the "more languages…" overflow
/// (any whitelisted language NOT in this list renders there).
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
    // Promoted 2026-07-04: hello-world counter e2e green on the matrix
    // (scripts/e2e_language_matrix.sh), install steps verified truthful,
    // guide pages present. See scripts/BINDINGS_REVIEW_2026_07_04.md.
    "zig", "go", "pascal", "scala", "fortran", "haskell",
    // Promoted 2026-07-06: counter e2e green on the merged dll (perl/lisp
    // host-invoker; php via the ext-php-rs native extension). Truthful
    // install steps + guides. Kept OUT of the CI gate (SHIPPED_LANGS) until
    // their fragile toolchains — FFI::Platypus, quicklisp+cffi-libffi, the
    // ext-php-rs build — are confirmed on the CI runners.
    "perl", "lisp", "php",
    // Promoted 2026-07-06 (genericity thesis): candidate bindings emitted by
    // codegen v2 + examples + guides. Two archetypes proven — C-ABI-direct
    // (odin, nim: real C fn-ptrs) and the host-invoker path (racket, red).
    // "Impl blindly, validate via CI" — kept OUT of the CI gate (SHIPPED_LANGS)
    // until their matrix rows go green cross-OS; guides marked experimental.
    "odin", "nim", "racket", "red",
    // More archetype-A candidates (2026-07-06): d/crystal/julia redeclare the C
    // ABI; swift/v consume the generated azul.h. Same CI-validated, non-gating
    // status as the row above.
    "d", "crystal", "v", "swift", "julia",
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
        // The toggle is a button IN the row, not a block under it: collapsed,
        // it costs no vertical space at all. Checkbox + label rather than
        // <details> because a <details> box cannot put its summary in the flex
        // row and its contents on the next line. `$$EXAMPLE_ID$$` is
        // substituted per example, so the id stays unique down the page.
        out.push_str(&format!(
            "\n        <input type=\"checkbox\" class=\"lang-more-toggle\" \
             id=\"lang-more-$$EXAMPLE_ID$$\">\n        \
             <label class=\"lang-more-btn\" for=\"lang-more-$$EXAMPLE_ID$$\"></label>\n        \
             <div class=\"lang-more-grid\">\n        {}\n        </div>",
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
        .replace("<!-- HEAD -->", &get_landing_head_tags(inline_css))
        .replace("<!-- NAV -->", &azlin_nav("overview"))
        .replace("<!-- FOOTER -->", &azlin_footer())
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
    format!(
        r#"<script src="{HTML_ROOT}/prism/prism.min.js" defer></script>"#
    )
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
.center main h1, .center main h2, .center main h3, .center main h4,
.docs-content h1, .docs-content h2, .docs-content h3, .docs-content h4 {
  position: relative;
}
.center main h1 .anchor, .center main h2 .anchor,
.center main h3 .anchor, .center main h4 .anchor,
.docs-content h1 .anchor, .docs-content h2 .anchor,
.docs-content h3 .anchor, .docs-content h4 .anchor {
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
.center main h3:hover .anchor, .center main h4:hover .anchor,
.docs-content h1:hover .anchor, .docs-content h2:hover .anchor,
.docs-content h3:hover .anchor, .docs-content h4:hover .anchor {
  opacity: 1;
}
.center main h1 .anchor::before, .center main h2 .anchor::before,
.center main h3 .anchor::before, .center main h4 .anchor::before,
.docs-content h1 .anchor::before, .docs-content h2 .anchor::before,
.docs-content h3 .anchor::before, .docs-content h4 .anchor::before {
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
  font-family: "Red Hat Display", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
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
  .center main h3 .anchor, .center main h4 .anchor,
  .docs-content h1 .anchor, .docs-content h2 .anchor,
  .docs-content h3 .anchor, .docs-content h4 .anchor { color: #888; }
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
    '.center main h3 > a.anchor[id], .center main h4 > a.anchor[id], ' +
    '.docs-content h1 > a.anchor[id], .docs-content h2 > a.anchor[id], ' +
    '.docs-content h3 > a.anchor[id], .docs-content h4 > a.anchor[id]'
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

/// Head tags for the /ui landing page ONLY. The landing uses the azlin.io
/// Flora Design system (flora.css: tokens, type stack, nav strip, stone
/// buttons and the CSS depth rig) plus ui-landing.css for the docs-specific
/// parts (release card, example sections, code panels) — NOT main.css.
///
/// Production (`inline_css == true`) inlines both stylesheets to prevent FOUC;
/// debug links them externally (flora.css + ui-landing.css are copied to the
/// deploy root by main.rs) so CSS edits don't need a docgen re-run.
pub fn get_landing_head_tags(inline_css: bool) -> String {
    // Search assets live under the docs sub-path, same rule as the docs head.
    let base_url: &str = if inline_css { HTML_ROOT } else { UI_PATH };

    let css_tag = if inline_css {
        let flora_css = include_str!("../../templates/flora.css");
        let landing_css = include_str!("../../templates/ui-landing.css");
        format!("<style>\n{}\n{}\n</style>", flora_css, landing_css)
    } else {
        // Both files are copied to the deploy root (next to /foam.svg).
        "<link rel='stylesheet' type='text/css' href='/flora.css'>\n      \
         <link rel='stylesheet' type='text/css' href='/ui-landing.css'>"
            .to_string()
    };

    format!("
      <meta charset='utf-8'/>
      <meta name='viewport' content='width=device-width, initial-scale=1'>
      <meta http-equiv='Content-Type' content='text/html; charset=utf-8'/>
      <meta name='description' content='Cross-platform MIT-licensed desktop GUI framework for C and Rust using the Mozilla WebRender rendering engine'>
      <meta name='keywords' content='gui, rust, user interface'>

      {theme_boot}
      <link rel='preload' as='font' href='{base_url}/fonts/EBGaramond-Variable.woff2' type='font/woff2' crossorigin='anonymous'>
      <link rel='preload' as='font' href='{base_url}/fonts/GrenzeGotisch-Variable.woff2' type='font/woff2' crossorigin='anonymous'>
      <link rel='shortcut icon' type='image/x-icon' href='{base_url}/favicon.ico'>
      <link rel='stylesheet' href='{base_url}/prism/prism.min.css'>
      <link rel='stylesheet' href='{base_url}/azul-search.css'>
      {css_tag}
      <!-- TEMPORARY doc-review tool (remove this line + azul-review.js in a later release) -->
      <script defer src='{base_url}/azul-review.js'></script>
    ", base_url=base_url, css_tag=css_tag, theme_boot=get_theme_boot_script())
}

/// RETIRED. Every page now goes through `get_docs_head_tags` (Flora Design:
/// flora.css + azul-docs.css) or `get_landing_head_tags`; nothing calls this
/// and `main.css` is NOT part of the Flora theme. Kept only so an older
/// generator path can be revived if needed - do not wire new pages to it.
///
/// # Arguments
/// * `inline_css` - If true, the CSS from main.css is inlined in a <style> tag
///                  to prevent flash of unstyled content (FOUC).
///                  If false, only a <link> to main.css is used (faster for development).
pub fn get_common_head_tags(inline_css: bool) -> String {
    // Base URL - use absolute paths for both production and development
    // This ensures subpages like /blog/foo.html correctly reference /fonts, /main.css etc.
    // The whole docs site lives under /ui, so static assets resolve there too.
    // Prod uses the fully-qualified HTML_ROOT (= https://azul.rs/ui); debug uses
    // the root-relative /ui prefix (works against the local http.server).
    let base_url: &str = if inline_css {
        HTML_ROOT
    } else {
        UI_PATH // Root-relative paths like /ui/fonts/..., /ui/main.css
    };

    let css_tag = if inline_css {
        // Read and inline the CSS file to prevent FOUC
        let css_content = include_str!("../../templates/main.css");
        format!("<style>\n{}\n</style>", css_content)
    } else {
        // Link to local stylesheet for development (main.css is copied to deploy folder)
        format!("<link rel='stylesheet' type='text/css' href='{UI_PATH}/main.css'>")
    };

    format!("
      <meta charset='utf-8'/>
      <meta name='viewport' content='width=device-width, initial-scale=1'>
      <meta http-equiv='Content-Type' content='text/html; charset=utf-8'/>
      <meta name='description' content='Cross-platform MIT-licensed desktop GUI framework for C and Rust using the Mozilla WebRender rendering engine'>
      <meta name='keywords' content='gui, rust, user interface'>

      <link rel='preload' as='font' href='{base_url}/fonts/RedHatDisplay-VariableFont_wght.ttf' type='font/ttf' crossorigin='anonymous'>
      <link rel='preload' as='font' href='{base_url}/fonts/InstrumentSerif-Regular.ttf' type='font/ttf' crossorigin='anonymous'>
      <link rel='shortcut icon' type='image/x-icon' href='{base_url}/favicon.ico'>
      <link rel='stylesheet' href='{base_url}/prism/prism.min.css'>
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
/// The panel renders ONLY into an element with id `azul-search-mount`. A page
/// without one gets no search box - the box is something a page opts into,
/// not something the shell sprinkles on.
pub enum PageKind<'a> {
    Api,
    Guide(&'a [String]),
    /// Individual guide page. Uses the API search index (not pagefind) so the
    /// page's frontmatter `default_search_keys` resolve to real API entries,
    /// which the JS auto-expands on load as direct links to the API docs for
    /// the items mentioned on that page.
    GuidePage(&'a [String]),
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
            // The guide's OWN index, generated by search::generate_guide_index.
            // This box says "Search guide"; pointing it at pagefind meant it
            // found nothing wherever the CLI was absent, and pairing it with
            // the api index meant it found the wrong corpus. The guide text
            // is in the generator, so it indexes itself.
            format!("{{ type: 'api-index', url: '{UI_PATH}/guide/search.json' }}"),
            "Search guide",
        ),
        // Individual guide page: search the API index and open new tabs so a
        // click doesn't yank the reader off the tutorial. The frontmatter keys
        // ride in as `defaults`; the JS auto-expands them into API-doc links.
        PageKind::GuidePage(defaults) => (
            false,
            "_blank",
            serde_json::to_string(defaults).unwrap_or_else(|_| "[]".to_string()),
            r#"{ type: 'api-default' }"#.to_string(),
            "Search API",
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
        r#"<script src="{UI_PATH}/azul-search.js" defer></script>
<script>
// Set before the deferred search script runs so its api-index/search.json
// fetches resolve under the docs sub-path (/ui) instead of the domain root.
window.AZS_DOC_BASE = "{UI_PATH}";
document.addEventListener('DOMContentLoaded', function () {{
  if (!window.AzulSearch) return;
  // No mount, no search. The old fallback attached a floating pill to any
  // page that merely LOADED this script, which is how the API version picker
  // (where you have not picked a version yet), the blog post and the release
  // page each grew a search box nobody put there.
  var mount = document.getElementById('azul-search-mount');
  if (!mount) return;
  window.AzulSearch.mount({{
    source: {source_json},
    onApiPage: {on_api},
    linkTarget: '{link_target}',
    defaults: {defaults_json},
    placeholder: '{placeholder}',
    mount: mount,
    inline: true,
  }});
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
          <li><a href='https://azul.rs/ui'>overview</a></li>
          <li><a href='https://azul.rs/ui/releases'>releases</a></li>
          <li><a href='https://github.com/fschutt/azul'>code</a></li>
          <li><a href='https://azul.rs/ui/guide'>guide</a></li>
          <li><a href='https://azul.rs/ui/api'>api</a></li>
          <li><a href='https://azul.rs/ui/reftest'>reftests</a></li>
          <li><a href='https://azul.rs/ui/blog'>blog</a></li>
          <li><a href='https://azul.rs/ui/donate'>donate</a></li>
        </ul>
      </nav>
    "
    )
}

// ===========================================================================
// Azlin docs shell (2026-07-04 CSS rearchitecture)
//
// ONE shell for every docs page: floating nav strip + airy opener +
// footer, styled by flora.css (tokens/nav/buttons) + azul-docs.css (docs
// content). Page families may add ONE family stylesheet (docs-*.css,
// passed as `page_css`); ad-hoc inline <style> blocks in Rust strings are
// forbidden - that patchwork is what produced the unreadable-blue-links /
// two-designs situation this replaces.
// ===========================================================================

/// Everything `azlin_page` needs to assemble a full docs page.
pub struct AzlinPage {
    /// `<title>` content (" - Azul GUI framework" is NOT appended).
    pub title: String,
    /// Which navbar entry to mark active: "overview" | "releases" | "guide"
    /// | "api" | "reftests" | "blog" | "donate" (anything else: none).
    pub active_nav: &'static str,
    /// Extra tags appended to `<head>` (search init, prism script, family
    /// stylesheet links...). May be empty.
    pub head_extra: String,
    /// Optional page-family stylesheet CONTENT (e.g.
    /// `include_str!("../../templates/docs-api.css")`). Inlined in prod,
    /// and ALSO inlined in debug (family css is not copied to the deploy
    /// root; only azul-docs.css is).
    pub page_css: Option<&'static str>,
    /// Contents of `<main>` (typically `.docs-hero` + `.docs-body`).
    pub main_html: String,
}

/// Inline boot script for the light/dark switch. This MUST run before first
/// paint (it is emitted in `<head>`, not deferred): reading the stored choice
/// afterwards would flash the wrong ground for a frame.
///
/// Three states, matching the CSS: no attribute = follow the system,
/// `data-theme="light"` / `"dark"` = the reader has chosen.
pub fn get_theme_boot_script() -> &'static str {
    r#"<script>
      (function () {
        try {
          var t = localStorage.getItem('flora-theme');
          if (t === 'light' || t === 'dark') {
            document.documentElement.setAttribute('data-theme', t);
          }
        } catch (e) { /* private mode: fall back to the system preference */ }
      })();
    </script>"#
}

/// The lamp: a raised stone in the navbar that cycles light -> dark -> system.
/// Rendered inside `.nav-right` so it sits next to the links on desktop and
/// next to the hamburger on mobile.
pub fn azlin_theme_toggle() -> &'static str {
    r##"<button class="fl-lamp" type="button" aria-label="Switch between light and dark" title="Light / dark"
          onclick="(function(b){var r=document.documentElement;var c=r.getAttribute('data-theme');var sysDark=window.matchMedia('(prefers-color-scheme: dark)').matches;var next=(c==='dark'||(!c&&sysDark))?'light':'dark';if((next==='dark')===sysDark){r.removeAttribute('data-theme');try{localStorage.removeItem('flora-theme');}catch(e){}}else{r.setAttribute('data-theme',next);try{localStorage.setItem('flora-theme',next);}catch(e){}}})(this)">
          <svg class="fl-lamp-moon" viewBox="0 0 24 24" aria-hidden="true"><path d="M20 14.5A8.2 8.2 0 0 1 9.5 4 8.5 8.5 0 1 0 20 14.5z"/></svg>
          <svg class="fl-lamp-sun" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="4.2"/><path d="M12 2.4v2.2M12 19.4v2.2M2.4 12h2.2M19.4 12h2.2M5.2 5.2l1.6 1.6M17.2 17.2l1.6 1.6M18.8 5.2l-1.6 1.6M6.8 17.2l-1.6 1.6"/></svg>
        </button>"##
}

/// Head tags for docs pages: fonts (Grenze Gotisch / EB Garamond / Fira Sans
/// + Red Hat Mono), favicon, prism theme, search css, flora.css +
/// azul-docs.css (linked in debug, inlined in prod - same rule as the /ui
/// landing).
pub fn get_docs_head_tags(inline_css: bool, page_css: Option<&'static str>) -> String {
    let base_url: &str = if inline_css { HTML_ROOT } else { UI_PATH };

    let mut css_tag = if inline_css {
        let flora_css = include_str!("../../templates/flora.css");
        let docs_css = include_str!("../../templates/azul-docs.css");
        format!("<style>\n{}\n{}\n</style>", flora_css, docs_css)
    } else {
        "<link rel='stylesheet' type='text/css' href='/flora.css'>\n      \
         <link rel='stylesheet' type='text/css' href='/azul-docs.css'>"
            .to_string()
    };
    if let Some(family) = page_css {
        css_tag.push_str(&format!("\n      <style>\n{}\n</style>", family));
    }

    format!("
      <meta charset='utf-8'/>
      <meta name='viewport' content='width=device-width, initial-scale=1'>
      <meta http-equiv='Content-Type' content='text/html; charset=utf-8'/>
      <meta name='description' content='Cross-platform MIT-licensed desktop GUI framework for C and Rust using the Mozilla WebRender rendering engine'>
      <meta name='keywords' content='gui, rust, user interface'>

      {theme_boot}
      <link rel='preload' as='font' href='{base_url}/fonts/EBGaramond-Variable.woff2' type='font/woff2' crossorigin='anonymous'>
      <link rel='preload' as='font' href='{base_url}/fonts/GrenzeGotisch-Variable.woff2' type='font/woff2' crossorigin='anonymous'>
      <link rel='shortcut icon' type='image/x-icon' href='{base_url}/favicon.ico'>
      <link rel='stylesheet' href='{base_url}/prism/prism.min.css'>
      <link rel='stylesheet' href='{base_url}/azul-search.css'>
      {css_tag}
      {anchor_link}
      <!-- TEMPORARY doc-review tool (remove this line + azul-review.js in a later release) -->
      <script defer src='{base_url}/azul-review.js'></script>
    ", base_url=base_url, css_tag=css_tag, anchor_link=get_anchor_link_script(),
       theme_boot=get_theme_boot_script())
}

/// The floating nav strip + mobile menu, identical to the /ui landing's
/// (single source of truth for docs pages; the landing templates keep their
/// static copies - keep the link list in sync with index.template.html and
/// azlin-index.template.html).
/// The corner assembly on the selected tab: the flare, the cove, the run-out
/// and the foot, on each side. Order matters - the cove's outer antialias has
/// to land over the flare, not under it. See `.fl-tab-flare` in flora.css.
pub const AZLIN_TAB_SHOULDERS: &str =
    "<span class=\"fl-tab-flare fl-tab-flare-l\" aria-hidden=\"true\"></span><span class=\"fl-tab-flare fl-tab-flare-r\" aria-hidden=\"true\"></span><span class=\"fl-tab-cove fl-tab-cove-l\" aria-hidden=\"true\"></span><span class=\"fl-tab-cove fl-tab-cove-r\" aria-hidden=\"true\"></span><span class=\"fl-tab-runout fl-tab-runout-l\" aria-hidden=\"true\"></span><span class=\"fl-tab-runout fl-tab-runout-r\" aria-hidden=\"true\"></span><span class=\"fl-tab-foot\" aria-hidden=\"true\"></span>";

/// The docs strip: everything under /ui.
pub fn azlin_nav(active: &str) -> String {
    // `home` leads the strip and is never the active tab: from inside /ui
    // there was no way back to the front page at all, and "overview" reads
    // as the top of the DOCS, which is where the reader already is.
    // `code` points off-site, so it sits last: between them the strip reads
    // left to right as a path through the site, with the one exit at the end.
    const LINKS: [(&str, &str); 9] = [
        ("home", "https://azul.rs/"),
        ("overview", "https://azul.rs/ui"),
        ("releases", "https://azul.rs/ui/releases"),
        ("guide", "https://azul.rs/ui/guide"),
        ("api", "https://azul.rs/ui/api"),
        ("reftests", "https://azul.rs/ui/reftest"),
        ("blog", "https://azul.rs/ui/blog"),
        ("donate", "https://azul.rs/ui/donate"),
        ("code", "https://github.com/fschutt/azul"),
    ];
    render_nav(&LINKS, active)
}

/// The product strip: the marketing pages at the domain root.
///
/// There is no separate home page - the workspace IS the front page, so its
/// tab points at `/`. The three root templates used to carry a hand-written
/// copy of this each, which is how `/os` ended up still saying "/OS" after
/// the labels were rewritten. One list, one renderer, no drift.
pub fn azlin_root_nav(active: &str) -> String {
    const LINKS: [(&str, &str); 3] = [
        ("workspace", "/"),
        ("ui toolkit", "/ui/"),
        ("operating system", "/os"),
    ];
    render_nav(&LINKS, active)
}

/// Renders a nav strip and its drop panel from ONE ordered list, so a label
/// or an order can never differ between the two.
fn render_nav(links: &[(&str, &str)], active: &str) -> String {
    let nav_links = links
        .iter()
        .map(|(name, href)| {
            // The selected tab carries the shoulder elements: the cove
            // fillets that flare it out to meet the strip's rule. They are
            // decorative geometry, hence aria-hidden.
            if *name == active {
                format!(
                    "<a href=\"{href}\" role=\"menuitem\" class=\"active\">{shoulders}{name}</a>",
                    shoulders = AZLIN_TAB_SHOULDERS
                )
            } else {
                format!("<a href=\"{href}\" role=\"menuitem\">{name}</a>")
            }
        })
        .collect::<Vec<_>>()
        .join("\n          ");
    let panel = links
        .iter()
        .map(|(name, href)| {
            let class = if *name == active { " class=\"active\"" } else { "" };
            format!("<a href=\"{href}\"{class}>{name}</a>")
        })
        .collect::<Vec<_>>()
        .join("\n    ");
    let theme_toggle = azlin_theme_toggle();
    let orb = azlin_orb();
    format!(
        r##"<a href="#main-content" class="skip-to-content">Skip to main content</a>
  <nav class="navbar" role="navigation" aria-label="Main navigation">
    <div class="container">
      <div class="nav-links" role="menu">
        {nav_links}
      </div>
      <div class="nav-right">
        {theme_toggle}
        {orb}
      </div>
    </div>
  </nav>
  <div class="nav-overlay" aria-hidden="true" onclick="document.body.classList.remove('nav-open')"></div>
  <aside class="mobile-menu" id="site-nav-panel" aria-label="Site navigation">
    {panel}
  </aside>"##
    )
}

/// The socket: the brand mark sunk into the nav strip at the top right,
/// overhanging its bottom edge.
///
/// TWO of them are emitted and CSS shows one. On desktop the strip already
/// carries the whole navigation as tabs, so a second menu behind the mark
/// would be a duplicate - there it is a plain link home. Below 900px the
/// tabs are gone, so there it is the menu trigger and replaces the old
/// hamburger. The click handler only flips a class; the open/close motion
/// is a CSS transition on `.mobile-menu`, so the widget port inherits the
/// timing.
/// It is built out of real layers rather than one box with a shadow list -
/// the well cut into the strip, the shadow its top lip casts down into it,
/// the light bouncing back off the far wall, a metal collar, the domed mark,
/// its gloss, and the reflection the well throws onto the mark's lower rim.
/// Each is its own element so the widget port can map one to one.
pub fn azlin_orb() -> &'static str {
    r##"<a href="/" class="fl-orb fl-orb-home" aria-label="Go to the homepage">
          <span class="fl-orb-well" aria-hidden="true">
            <span class="fl-orb-well-shadow"></span>
            <span class="fl-orb-well-bounce"></span>
          </span>
          <span class="fl-orb-collar" aria-hidden="true"></span>
          <span class="fl-orb-stone">
            <img src="/logo.svg" alt="Azul" class="nav-brand-logo">
            <span class="fl-orb-gloss" aria-hidden="true"></span>
            <span class="fl-orb-edge" aria-hidden="true"></span>
          </span>
        </a>
        <button class="fl-orb fl-orb-menu" type="button" aria-label="Open site navigation" aria-expanded="false" aria-controls="site-nav-panel"
          onclick="var o=document.body.classList.toggle('nav-open');this.setAttribute('aria-expanded',o);">
          <span class="fl-orb-well" aria-hidden="true">
            <span class="fl-orb-well-shadow"></span>
            <span class="fl-orb-well-bounce"></span>
          </span>
          <span class="fl-orb-collar" aria-hidden="true"></span>
          <span class="fl-orb-stone">
            <img src="/logo.svg" alt="Azul" class="nav-brand-logo">
            <span class="fl-orb-gloss" aria-hidden="true"></span>
            <span class="fl-orb-edge" aria-hidden="true"></span>
          </span>
        </button>"##
}

/// THE footer. Every page on the site gets this one - docs pages call it
/// directly, the landing templates carry a `<!-- FOOTER -->` marker that the
/// build fills in. It used to be copied by hand into five places, and a
/// change to the wording reached three of them.
pub fn azlin_footer() -> String {
    r#"<footer role="contentinfo" class="docs-footer">
    <div class="container">
      <p><a href="https://en.wikipedia.org/wiki/Ad_maiorem_Dei_gloriam" target="_blank" rel="noopener noreferrer">A.M.D.G.</a> &mdash; Azlin Project 2026</p>
    </div>
  </footer>
  <script>document.querySelectorAll('.mobile-menu a, .nav-links a').forEach(function(a){a.addEventListener('click',function(){document.body.classList.remove('nav-open');});});</script>
  <script>
  // Explicit "Copy" button on every code block (same affordance as the
  // /ui landing examples).
  document.addEventListener('DOMContentLoaded', function () {
    document.querySelectorAll('.docs-content pre').forEach(function (pre) {
      var btn = document.createElement('button');
      btn.className = 'docs-copy-btn';
      btn.type = 'button';
      btn.textContent = 'Copy';
      btn.addEventListener('click', function () {
        var code = pre.querySelector('code');
        navigator.clipboard.writeText((code || pre).innerText).then(function () {
          btn.textContent = 'Copied!';
          setTimeout(function () { btn.textContent = 'Copy'; }, 1500);
        });
      });
      pre.appendChild(btn);
    });
  });
  </script>"#
        .to_string()
}

/// Assemble a complete docs page in the azlin shell.
pub fn azlin_page(page: &AzlinPage, inline_css: bool) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <title>{title}</title>
  {head}
  {head_extra}
</head>
<body class="docs">
  {nav}
  <main id="main-content" role="main">
{main}
  </main>
  {footer}
</body>
</html>"#,
        title = page.title,
        head = get_docs_head_tags(inline_css, page.page_css),
        head_extra = page.head_extra,
        nav = azlin_nav(page.active_nav),
        main = page.main_html,
        footer = azlin_footer(),
    )
}

/// Write a docs page under BOTH `<dir>/<name>.html` (legacy inbound links)
/// and `<dir>/<name>/index.html` (serves the extensionless clean URL on
/// GitHub Pages and python -m http.server alike). Rendered pages link the
/// clean URL only; the .html twin is for old bookmarks/backlinks.
pub fn write_page_clean_url(
    dir: &std::path::Path,
    name: &str,
    html: &str,
) -> anyhow::Result<()> {
    use std::fs;
    fs::create_dir_all(dir)?;
    fs::write(dir.join(format!("{name}.html")), html)?;
    let clean_dir = dir.join(name);
    fs::create_dir_all(&clean_dir)?;
    fs::write(clean_dir.join("index.html"), html)?;
    Ok(())
}

/// Guard against a stylesheet losing a section.
///
/// flora.css is ~3400 lines in one file, and it has been truncated three
/// times by an edit that replaced a marker-to-marker range whose end marker
/// had drifted past the middle of the file. Each time the site kept BUILDING
/// - CSS has no compile step, a missing rule is simply a rule that never
/// matches - and the damage only showed up as unstyled buttons and
/// UA-purple links in a screenshot.
///
/// So the stylesheets get a compile-time-ish contract instead: every section
/// that must exist is named here, and `cargo test -p azul-doc` fails if one
/// goes missing. Braces are counted too, since a half-eaten rule silently
/// swallows everything after it.
///
/// Add to these lists when you add a section. That is the point: the list is
/// the inventory, and losing an entry is now a test failure rather than a
/// visual regression someone has to notice.
#[cfg(test)]
mod stylesheet_contract {
    const FLORA: &str = include_str!("../../templates/flora.css");
    const DOCS: &str = include_str!("../../templates/azul-docs.css");
    const GUIDE: &str = include_str!("../../templates/docs-guide.css");
    const LANDING: &str = include_str!("../../templates/ui-landing.css");
    const SEARCH: &str = include_str!("../../templates/azul-search.css");

    /// Sections of flora.css, in the order they appear. Every one of these has
    /// been lost at least once to an over-broad edit.
    const FLORA_REQUIRED: &[&str] = &[
        // tokens + the two dark blocks
        "--fl-pg:", "--fl-acc:", "--fl-metal-turn:", "--fl-text:",
        ":root[data-theme=\"dark\"]", "prefers-color-scheme: dark",
        // chrome
        ".navbar {", ".navbar::after", ".nav-links a {", ".nav-links a.active {",
        ".fl-orb {", ".fl-orb-well", ".fl-lamp {",
        // the tab corner assembly
        ".fl-tab-flare", ".fl-tab-cove", ".fl-tab-runout", ".fl-tab-foot",
        // buttons - the section that keeps disappearing
        ".btn {", ".btn-primary {", ".btn-secondary {", ".btn-hero-primary {",
        ".btn-quiet {",
        // page structure
        ".hero {", ".hero::before", ".ui-hero::before", ".feature-card {", ".feature-media",
        ".faq-section {", ".docs-footer {", "footer {",
        // link colours - without these every link is UA purple
        "a:visited", ".btn-primary, .btn-primary:visited",
        // furniture, code, motion, the depth rig
        "::-webkit-scrollbar", ".token.comment", "THE DROP PANEL",
        ".copy-btn,", ".docs-copy-btn {",
        "STONE RIG", "FLORA DESIGN - MOTION", "@keyframes fl-sheen",
        "prefers-reduced-motion",
    ];

    const DOCS_REQUIRED: &[&str] = &[
        "body.docs", ".docs-hero", ".docs-content p,", ".docs-content pre",
        ".docs-card {",
        ".docs-card {", ".docs-list-item {", ".docs-layout",
    ];

    /// docs-guide.css - the guide index's cards live here.
    const GUIDE_REQUIRED: &[&str] = &[
        ".guide-grid", ".guide-card", ".guide-links", "a.guide-link",
        "a.guide-card-btn",
        ".guide-link-lead", ".azul-window", ".markdown-alert-warning", "@media print",
    ];

    const LANDING_REQUIRED: &[&str] = &[
        ".ui-hero {", ".feature-section {", ".lang-grid button,",
        ".lang-more-btn", ".lang-more-toggle:checked ~ .lang-more-grid",
        ".code-panel {", ".example-code {", "#latestrelease {",
    ];

    const SEARCH_REQUIRED: &[&str] = &[
        ".azul-search {", ".azs-inline-row", ".azs-panel", ".azs-result a",
        ".azs-kind", "--azs-bg:",
    ];

    fn check(name: &str, css: &str, required: &[&str]) {
        let missing: Vec<_> = required.iter().filter(|s| !css.contains(**s)).collect();
        assert!(
            missing.is_empty(),
            "{name} lost {} section(s): {missing:?}\n\
             A rule that is gone still compiles - it just never matches. If you \
             removed one on purpose, drop it from the list in the same commit.",
            missing.len()
        );
        let open = css.matches('{').count();
        let close = css.matches('}').count();
        assert_eq!(open, close, "{name}: unbalanced braces ({open} open, {close} close)");
    }

    /// The strip and the footer have exactly ONE source each. Both used to be
    /// copied by hand into the landing templates and the reftest report, and
    /// both drifted: /os kept an old tab label after the others were renamed,
    /// and a footer rewrite reached three of five copies. A template may carry
    /// the `<!-- NAV -->` / `<!-- FOOTER -->` marker; it may not carry the
    /// markup.
    #[test]
    fn nav_and_footer_have_one_source() {
        const TEMPLATES: &[(&str, &str)] = &[
            ("index.template.html", include_str!("../../templates/index.template.html")),
            ("azlin-index.template.html", include_str!("../../templates/azlin-index.template.html")),
            ("azlin-os.html", include_str!("../../templates/azlin-os.html")),
            ("azlin-ws.html", include_str!("../../templates/azlin-ws.html")),
            ("report_template.html", include_str!("../reftest/report_template.html")),
        ];
        for (name, html) in TEMPLATES {
            assert!(
                !html.contains("<nav class=\"navbar\""),
                "{name} carries its own nav strip - use <!-- NAV --> and let                  docgen::azlin_nav / azlin_root_nav render it"
            );
            assert!(
                !html.contains("class=\"docs-footer\""),
                "{name} carries its own footer - use <!-- FOOTER --> and let                  docgen::azlin_footer render it"
            );
            assert!(
                html.contains("<!-- NAV -->") && html.contains("<!-- FOOTER -->"),
                "{name} is missing a <!-- NAV --> or <!-- FOOTER --> marker"
            );
        }
    }

    #[test]
    fn stylesheets_keep_their_sections() {
        check("flora.css", FLORA, FLORA_REQUIRED);
        check("azul-docs.css", DOCS, DOCS_REQUIRED);
        check("docs-guide.css", GUIDE, GUIDE_REQUIRED);
        check("ui-landing.css", LANDING, LANDING_REQUIRED);
        check("azul-search.css", SEARCH, SEARCH_REQUIRED);
    }

    /// The dark palette is written twice - once for the system preference,
    /// once for the explicit toggle - and they have to agree, or choosing a
    /// theme silently gives you a different one.
    #[test]
    fn dark_blocks_agree() {
        fn tokens(block: &str) -> Vec<&str> {
            block
                .lines()
                .filter_map(|l| l.trim().strip_prefix("--fl-"))
                .filter_map(|l| l.split(':').next())
                .collect()
        }
        let media = FLORA
            .split("@media (prefers-color-scheme: dark) {\n    :root:not([data-theme=\"light\"]) {")
            .nth(1)
            .expect("media dark block");
        let attr = FLORA
            .split(":root[data-theme=\"dark\"] {\n    color-scheme: dark;")
            .nth(1)
            .expect("attribute dark block");
        let a = tokens(media.split("\n    }\n}").next().unwrap());
        let b = tokens(attr.split("\n}").next().unwrap());
        assert_eq!(
            a, b,
            "the two dark blocks define different tokens - an explicit theme \
             choice would not match the system one"
        );
    }
}
