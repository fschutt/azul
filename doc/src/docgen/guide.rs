use comrak::options::{Extension, Parse, Plugins, Render, RenderPlugins};

use super::HTML_ROOT;

/// Guide information structure
pub struct Guide {
    /// Title extracted from first H1 header in markdown (for navigation)
    pub title: String,
    /// File name derived from the .md filename (for URL)
    pub file_name: String,
    /// Raw markdown content
    pub content: String,
}

/// Extract the first H1 header from markdown content as the title
fn extract_title_from_markdown(content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            return trimmed[2..].trim().to_string();
        }
    }
    "Untitled".to_string()
}

/// Create a Guide from a markdown file path and content
fn guide_from_md(md_filename: &str, content: &'static str) -> Guide {
    // Remove .md extension and any leading numbers/underscores for URL
    let file_name = md_filename
        .trim_end_matches(".md")
        .to_string();
    
    let title = extract_title_from_markdown(content);
    
    Guide {
        title,
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
        guide_from_md("installation", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/installation.md"
        ))),
        guide_from_md("01_Getting_Started", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/01_Getting_Started.md"
        ))),
        guide_from_md("architecture", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/architecture.md"
        ))),
        guide_from_md("03_CSS_Styling", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/03_CSS_Styling.md"
        ))),
        guide_from_md("04_Images_SVG", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/04_Images_SVG.md"
        ))),
        guide_from_md("05_Timers_Threads_Animations", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/05_Timers_Threads_Animations.md"
        ))),
        guide_from_md("06_OpenGL", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/06_OpenGL.md"
        ))),
        guide_from_md("07_Unit_Testing", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/07_Unit_Testing.md"
        ))),
        guide_from_md("08_XHTML_And_Workbench", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/08_XHTML_And_Workbench.md"
        ))),
        guide_from_md("09_NotesForC", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/09_NotesForC.md"
        ))),
        guide_from_md("10_NotesForCpp", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/10_NotesForCpp.md"
        ))),
        guide_from_md("11_NotesForPython", include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/guide/11_NotesForPython.md"
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
            "<li><a href=\"{HTML_ROOT}/guide/{version}\">{version}</a></li>\n",
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
