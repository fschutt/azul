use comrak::options::{Extension, Parse, Plugins, Render, RenderPlugins};

use super::HTML_ROOT;

/// Guide information structure
pub struct Guide {
    /// Title for navigation (from api.json or hardcoded)
    pub title: String,
    /// File name derived from the .md filename (for URL)
    pub file_name: String,
    /// Raw markdown content
    pub content: String,
}

/// Create a Guide from a markdown file path, content, and explicit title
fn guide_from_md(md_filename: &str, title: &str, content: &'static str) -> Guide {
    // Remove .md extension for URL
    let file_name = md_filename.trim_end_matches(".md").to_string();

    Guide {
        title: title.to_string(),
        file_name,
        content: content.to_string(),
    }
}

/// Pre-process markdown content:
/// - Remove mermaid code blocks (not supported in HTML output)
/// Note: We keep the first H1 header now since that's the real title
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

/// Get a list of all guides
pub fn get_guide_list() -> Vec<Guide> {
    vec![
        guide_from_md(
            "installation",
            "Installation",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/guide/installation.md"
            )),
        ),
        guide_from_md(
            "getting-started-rust",
            "Getting Started (Rust)",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/guide/getting-started-rust.md"
            )),
        ),
        guide_from_md(
            "getting-started-c",
            "Getting Started (C)",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/guide/getting-started-c.md"
            )),
        ),
        guide_from_md(
            "getting-started-cpp",
            "Getting Started (C++)",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/guide/getting-started-cpp.md"
            )),
        ),
        guide_from_md(
            "getting-started-python",
            "Getting Started (Python)",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/guide/getting-started-python.md"
            )),
        ),
        guide_from_md(
            "css-styling",
            "CSS Styling",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/css-styling.md")),
        ),
        guide_from_md(
            "widgets",
            "Widgets",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/widgets.md")),
        ),
        guide_from_md(
            "architecture",
            "Architecture",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/guide/architecture.md"
            )),
        ),
        guide_from_md(
            "comparison",
            "Comparison with Other Frameworks",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/comparison.md")),
        ),
    ]
}

/// Generate HTML for a specific guide
pub fn generate_guide_html(guide: &Guide, version: &str) -> String {
    let header_tags = crate::docgen::get_common_head_tags(false);
    let sidebar = crate::docgen::get_sidebar();
    let prism_script = crate::docgen::get_prism_script();

    // Pre-process content: remove first H1 header (we add our own) and mermaid diagrams
    let processed_content = preprocess_markdown_content(&guide.content);

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
            font-size: 1.2em;
            margin-bottom: 1em;
            margin-top: 1em;
        }
        #guide p {
            margin-bottom: 15px;
            margin-top: 10px;
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
            font-size: 1.2em;
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
            <ul>{version_items}</ul>
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
