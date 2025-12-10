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
    let file_name = md_filename
        .trim_end_matches(".md")
        .to_string();
    
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
        guide_from_md("installation", "Installation", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/installation.md"
        ))),
        guide_from_md("getting-started-rust", "Getting Started (Rust)", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/getting-started-rust.md"
        ))),
        guide_from_md("getting-started-c", "Getting Started (C)", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/getting-started-c.md"
        ))),
        guide_from_md("getting-started-cpp", "Getting Started (C++)", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/getting-started-cpp.md"
        ))),
        guide_from_md("getting-started-python", "Getting Started (Python)", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/getting-started-python.md"
        ))),
        guide_from_md("css-styling", "CSS Styling", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/css-styling.md"
        ))),
        guide_from_md("widgets", "Widgets", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/widgets.md"
        ))),
        guide_from_md("architecture", "Architecture", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/architecture.md"
        ))),
        guide_from_md("comparison", "Comparison with Other Frameworks", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/comparison.md"
        ))),
    ]
}

/// Generate HTML for a specific guide
pub fn generate_guide_html(guide: &Guide, version: &str) -> String {
    let header_tags = crate::docgen::get_common_head_tags();
    let sidebar = crate::docgen::get_sidebar();
    
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
                codefence_syntax_highlighter: Some(&comrak::plugins::syntect::SyntectAdapter::new(
                    None,
                )),
                heading_adapter: None,
            },
        },
    );
    let title = &guide.title;

    let css = "
        h1, h2, h3, h4 { cursor: pointer; }
        h1 { margin-top: 30px; margin-bottom: 10px; }
        h2 { margin-top: 30px; margin-bottom: 10px; }
        h3 { margin-top: 25px; margin-bottom: 5px; }
        h4 { margin-top: 20px; margin-bottom: 5px; }
        #guide { max-width: 700px; line-height: 1.5; font-size: 1.2em; }
        #guide img { max-width: 700px; margin-top: 10px; margin-bottom: 10px;}
        #guide ul, #guide ol {
            margin-top: 10px;
            margin-bottom: 10px;
            margin-left: 30px;
        }
        #guide li {
            font-size: 1em;
        }
        #guide p {
            margin-bottom: 15px;
            margin-top: 10px;
        }
        #guide code {
            font-family: monospace;
            font-weight: bold;
            border-radius: 3px;
            padding: 2.5px 10px;
            border-radius: 3px;
        }
        #guide pre code {
            font-weight: bold;
            font-family: monospace;
            margin-top: 10px;
            margin-bottom: 10px;
            display: flex;
            flex-direction: column;
            padding: 10px;
            border-radius: 3px;
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

    let header_tags = crate::docgen::get_common_head_tags();
    let sidebar = crate::docgen::get_sidebar();

    format!(
        "<!DOCTYPE html>
        <html lang='en'>
        <head>
        <title>User guide for azul v{version}</title>

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
            <h1>User guide for azul v{version}</h1>
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

    let header_tags = crate::docgen::get_common_head_tags();
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
