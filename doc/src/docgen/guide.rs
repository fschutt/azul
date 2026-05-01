use std::fs;
use std::path::{Path, PathBuf};

use comrak::options::{Extension, Parse, Plugins, Render, RenderPlugins};

use super::HTML_ROOT;

/// Guide information structure
pub struct Guide {
    /// Title for navigation (from frontmatter or first H1)
    pub title: String,
    /// Path-like file name (no extension) used to compute the output URL.
    /// For nested pages this includes subdirectories, e.g. `internals/dom`.
    pub file_name: String,
    /// Markdown content with the YAML frontmatter already stripped.
    pub content: String,
}

/// Pre-process markdown content:
/// - Remove mermaid code blocks (not supported in HTML output)
/// (Frontmatter is stripped earlier, in `get_guide_list`, so it never
/// reaches this stage.)
fn preprocess_markdown_content(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();

    // Remove mermaid code blocks
    let mut result = Vec::new();
    let mut in_mermaid_block = false;

    for line in lines {
        if line.trim().starts_with("```mermaid") {
            in_mermaid_block = true;
            continue;
        }
        if in_mermaid_block {
            if line.trim() == "```" {
                in_mermaid_block = false;
            }
            continue;
        }
        result.push(line);
    }

    result.join("\n")
}

/// Walk `doc/guide/en/` at runtime and return one Guide per .md file.
/// Frontmatter is parsed for title/ordering; pages without frontmatter
/// fall back to their first H1 line as title.
///
/// Output ordering: pages with `guide_order` come first (ascending),
/// then everything else alphabetically.
pub fn get_guide_list() -> Vec<Guide> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lang_dir = manifest_dir.join("guide").join("en");
    let mut collected: Vec<(Option<i32>, String, Guide)> = Vec::new();
    walk_collect(&lang_dir, &lang_dir, &mut collected);

    collected.sort_by(|a, b| match (a.0, b.0) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.1.cmp(&b.1),
    });

    collected.into_iter().map(|(_, _, g)| g).collect()
}

fn walk_collect(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(Option<i32>, String, Guide)>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            // Skip generated assets
            if matches!(n, "screenshots" | "target") {
                continue;
            }
            walk_collect(root, &p, out);
        } else if p.extension().map(|e| e == "md").unwrap_or(false) {
            let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if n == "SUMMARY.md" {
                continue;
            }
            let rel = match p.strip_prefix(root) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let stem = rel.with_extension("");
            let file_name = stem.to_string_lossy().replace('\\', "/");
            let content = fs::read_to_string(&p).unwrap_or_default();
            let (title, body, guide_order) = extract_metadata(&content, &file_name);
            out.push((
                guide_order,
                file_name.clone(),
                Guide {
                    title,
                    file_name,
                    content: body,
                },
            ));
        }
    }
}

fn extract_metadata(content: &str, fallback_name: &str) -> (String, String, Option<i32>) {
    if let Some((fm, body)) = crate::reftest::autodoc::parse_frontmatter(content) {
        return (fm.title, body, fm.guide_order);
    }
    // No frontmatter — first H1, else fallback name.
    let mut title = fallback_name.to_string();
    for line in content.lines().take(40) {
        if let Some(t) = line.trim().strip_prefix("# ") {
            title = t.to_string();
            break;
        }
    }
    (title, content.to_string(), None)
}

/// Generate HTML for a specific guide
pub fn generate_guide_html(guide: &Guide, version: &str) -> String {
    let header_tags = crate::docgen::get_common_head_tags(false);
    let sidebar = crate::docgen::get_sidebar();
    let prism_script = crate::docgen::get_prism_script();

    // Pre-process content: remove mermaid blocks and expand `azul-render`
    // fences into <figure>/slideshow HTML. Use an absolute URL prefix so
    // pages at any nesting depth (`guide/dom.html` vs `guide/internals/dom.html`)
    // resolve to the same screenshots directory.
    let processed_content = preprocess_markdown_content(&guide.content);
    let screenshot_prefix = format!("{HTML_ROOT}/guide/screenshots/");
    let processed_content = crate::reftest::autodoc::expand_azul_render_blocks(
        &processed_content,
        &screenshot_prefix,
    );

    let content = comrak::markdown_to_html_with_plugins(
        &processed_content,
        &comrak::Options {
            render: Render::default(),
            parse: Parse::default(),
            extension: Extension {
                strikethrough: true,
                tagfilter: true,
                table: true,
                autolink: true,
                tasklist: true,
                superscript: true,
                footnotes: true,
                description_lists: true,
                multiline_block_quotes: true,
                alerts: true,
                math_dollars: true,
                math_code: true,
                wikilinks_title_after_pipe: true,
                wikilinks_title_before_pipe: true,
                underline: true,
                subscript: true,
                spoiler: true,
                greentext: true,
                ..Default::default()
            },
        },
        &Plugins {
            render: RenderPlugins {
                codefence_syntax_highlighter: None, // Syntax highlighting handled by Prism.js
                heading_adapter: None,
            },
        },
    );
    let title = &guide.title;

    let css = "
        h1 { 
            font-family: 'Instrument Serif', Georgia, serif;
            font-size: 2.5em;
            font-weight: normal;
            line-height: 1.2;
            margin-top: 0;
            margin-bottom: 25px;
            text-shadow: currentColor 0.5px 0.5px 0.5px, currentColor -0.5px -0.5px 0.5px, currentColor 0px -0.5px 0.5px, currentColor -0.5px 0px 0.5px;
            letter-spacing: 0.02em;
        }
        @media screen and (max-width: 768px) {
            h1 { font-size: 2.2em; }
        }
        @media screen and (max-width: 480px) {
            h1 { font-size: 1.8em; }
        }
        h2, h3, h4 { cursor: pointer; }
        h2 { 
            font-family: 'Instrument Serif', Georgia, serif;
            font-size: 2em;
            font-weight: normal;
            margin-top: 40px; 
            margin-bottom: 15px;
            text-shadow: 0.3px 0 0 currentColor, -0.3px 0 0 currentColor;
        }
        h3 { margin-top: 35px; margin-bottom: 10px; font-size: 1.3em; }
        h4 { margin-top: 25px; margin-bottom: 8px; font-size: 1.1em; }
        #guide { max-width: 700px; line-height: 1.7; font-size: 1.1em; }
        #guide img { max-width: 700px; margin-top: 15px; margin-bottom: 15px;}
        #guide ul, #guide ol {
            margin-top: 15px;
            margin-bottom: 15px;
            margin-left: 30px;
        }
        #guide li {
            font-size: 16px;
        }
        #guide code {
            font-family: monospace;
            font-weight: bold;
            font-size: 0.75em;
            border-radius: 5px;
            padding: 2px 5px;
        }
        #guide pre code {
            font-weight: normal;
            font-family: monospace;
            font-size: 10pt;
            margin-top: 5px;
            margin-bottom: 5px;
            display: block;
            padding: 3px;
            border-radius: 3px;
            white-space: pre;
            overflow-x: auto;
        }
        .markdown-alert-warning {
            padding: 10px;
            border-radius: 5px;
            border: 1px dashed #facb26;
            margin-top: 20px;
            margin-bottom: 20px;
            background: #fff8be;
            color: #222;
            box-shadow: 0px 0px 20px #facb2655;
        }
        .markdown-alert-warning .markdown-alert-title {
            font-weight: bold;
            font-size: 1.1em;
        }
    ";

    format!(
        "<!DOCTYPE html>
        <html lang='en'>
        <head>
        <title>{title}</title>

        {header_tags}
        </head>

        <body>
        <div class='center'>

        <aside>
            <header>
            <h1 style='display:none;'>Azul GUI Framework</h1>
            <a href='{HTML_ROOT}'>
                <img src='{HTML_ROOT}/logo.svg'>
            </a>
            </header>
            {sidebar}
        </aside>

        <main>
            <div id='guide'>
            <style>
                {css}
            </style>
            {content}
            </div>
            <p style='font-size:1.2em;margin-top:20px;'>
            <a href='{HTML_ROOT}/guide'>Back to guide index</a>
            </p>
        </main>

        </div>
        {prism_script}
        </body>
        </html>"
    )
}

pub fn generate_guide_mainpage(version: &str) -> String {
    let mut version_items = String::new();
    for guide_page in get_guide_list() {
        version_items.push_str(&format!(
            "<li><a href=\"{HTML_ROOT}/guide/{}.html\">{}</a></li>\n",
            guide_page.file_name, guide_page.title,
        ));
    }

    let header_tags = crate::docgen::get_common_head_tags(false);
    let sidebar = crate::docgen::get_sidebar();

    format!(
        "<!DOCTYPE html>
        <html lang='en'>
        <head>
        <title>User Guide</title>

        {header_tags}
        </head>

        <body>
        <div class='center'>
        <aside>
            <header>
            <h1 style='display:none;'>Azul GUI Framework</h1>
            <a href='{HTML_ROOT}'>
                <img src='{HTML_ROOT}/logo.svg'>
            </a>
            </header>
            {sidebar}
        </aside>
        <main>
            <h1>User Guide</h1>
            <div>
            <ul style='font-size: 18px;'>{version_items}</ul>
            </div>
        </main>
        </div>
        </body>
        </html>"
    )
}

/// Generate a combined guide index page
pub fn generate_guide_index(versions: &[String]) -> String {
    let mut version_items = String::new();
    for version in versions {
        version_items.push_str(&format!(
            "<li><a href=\"{HTML_ROOT}/guide/{version}.html\">{version}</a></li>\n",
        ));
    }

    let header_tags = crate::docgen::get_common_head_tags(false);
    let sidebar = crate::docgen::get_sidebar();

    format!(
        "<!DOCTYPE html>
        <html lang='en'>
        <head>
        <title>Choose guide version</title>

        {header_tags}
        </head>

        <body>
        <div class='center'>
        <aside>
            <header>
            <h1 style='display:none;'>Azul GUI Framework</h1>
            <a href='{HTML_ROOT}'>
                <img src='{HTML_ROOT}/logo.svg'>
            </a>
            </header>
            {sidebar}
        </aside>
        <main>
            <h1>Choose guide version</h1>
            <div>
            <ul>{version_items}</ul>
            </div>
        </main>
        </div>
        </body>
        </html>"
    )
}
