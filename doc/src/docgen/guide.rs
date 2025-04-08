use std::collections::HashMap;

use comrak::{ExtensionOptions, ParseOptions, Plugins, RenderOptions, RenderPlugins};

use super::HTML_ROOT;

/// Guide information structure
pub struct Guide {
    pub title: String,
    pub file_name: String,
    pub content: String,
}

/// Get a list of all guides
pub fn get_guide_list() -> Vec<Guide> {
    // In a real implementation, this would scan the guide directory
    // and read the markdown files, converting them to HTML

    vec![
        Guide {
            title: "Installation".to_string(),
            file_name: "Installation".to_string(),
            content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/00_Installation.md")).to_string(),
        },
        Guide {
            title: "Getting Started".to_string(),
            file_name: "GettingStarted".to_string(),
            content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/01_Getting_Started.md")).to_string(),
        },
        Guide {
            title: "Application Architecture".to_string(),
            file_name: "ApplicationArchitecture".to_string(),
            content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/02_Application_Architecture.md")).to_string(),
        },
        Guide {
            title: "CSS Styling".to_string(),
            file_name: "CssStyling".to_string(),
            content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/03_CSS_Styling.md")).to_string(),
        },
        Guide {
            title: "Images, SVG and Charts".to_string(),
            file_name: "ImagesSvgAndCharts".to_string(),
            content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/04_Images_SVG.md")).to_string(),
        },
        Guide {
            title: "Timers, Threads and Animations".to_string(),
            file_name: "TimersThreadsAndAnimations".to_string(),
            content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/05_Timers_Threads_Animations.md")).to_string(),
        },
        Guide {
            title: "OpenGL".to_string(),
            file_name: "OpenGL".to_string(),
            content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/06_OpenGL.md")).to_string(),
        },
        Guide {
            title: "Unit Testing".to_string(),
            file_name: "UnitTesting".to_string(),
            content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/07_Unit_Testing.md")).to_string(),
        },
        Guide {
            title: "XHTML and azul-workbench".to_string(),
            file_name: "XhtmlAndAzulWorkbench".to_string(),
            content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/08_XHTML_And_Workbench.md")).to_string(),
        },
        Guide {
            title: "Notes for C".to_string(),
            file_name: "NotesForC".to_string(),
            content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/09_NotesForC.md")).to_string(),
        },
        Guide {
            title: "Notes for C++".to_string(),
            file_name: "NotesForCpp".to_string(),
            content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/10_NotesForCpp.md")).to_string(),
        },
        Guide {
            title: "Notes for Python".to_string(),
            file_name: "NotesForPython".to_string(),
            content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/guide/11_NotesForPython.md")).to_string(),
        },
    ]
}

/// Generate HTML for a specific guide
pub fn generate_guide_html(guide: &Guide, version: &str) -> String {
    let header_tags = crate::docgen::get_common_head_tags();
    let sidebar = crate::docgen::get_sidebar();
    let content = comrak::markdown_to_html_with_plugins(
        &guide.content,
        &comrak::Options {
            render: RenderOptions {
                unsafe_: true,
                ..Default::default()
            },
            parse: ParseOptions::default(),
            extension: ExtensionOptions {
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
        h2 { margin-top: 10px;margin-bottom: 10px;}
        h3 { margin-top: 10px;margin-bottom: 5px; }
        #guide { max-width: 700px; line-height: 1.5; }
        #guide img { max-width: 700px; margin-top: 10px; margin-bottom: 10px;}
        #guide ul {
            margin-top: 10px;
            margin-bottom: 10px;
            margin-left: 30px;
        }
        #guide p {
            margin-bottom: 5px;
            margin-top: 5px;
            font-size: 1.2em;
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
            <nav>
            {sidebar}
            </nav>
        </aside>

        <main>
            <h1>{title}</h1>
            <div id='guide'>
            <style>
                {css}
            </style>
            {content}
            </div>
            <p style='font-size:1.2em;margin-top:20px;'>
            <a href='{HTML_ROOT}/guide/{version}'>Back to guide index</a>
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
            "<li><a href=\"{HTML_ROOT}/guide/{version}/{}\">{}</a></li>\n",
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
            <nav>
            {sidebar}
            </nav>
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
            <nav>
            {sidebar}
            </nav>
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
