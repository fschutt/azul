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
    /// `external` or `contributor`. Used by the index to bucket into trees.
    pub audience: Option<String>,
    /// Linear teaching order (Tree 1: 10–199, Tree 2: 200+).
    pub guide_order: Option<i32>,
    /// One-liner shown beneath the link in the guide index. Comes from
    /// the page's `short_desc` frontmatter field — localised per page and
    /// authored by hand (not extracted from prose).
    pub description: Option<String>,
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
            let rel = match p.strip_prefix(root) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let stem = rel.with_extension("");
            let file_name = stem.to_string_lossy().replace('\\', "/");
            let content = fs::read_to_string(&p).unwrap_or_default();
            let (title, body, guide_order, audience, description) =
                extract_metadata(&content, &file_name);
            out.push((
                guide_order,
                file_name.clone(),
                Guide {
                    title,
                    file_name,
                    content: body,
                    audience,
                    guide_order,
                    description,
                },
            ));
        }
    }
}

fn extract_metadata(
    content: &str,
    fallback_name: &str,
) -> (String, String, Option<i32>, Option<String>, Option<String>) {
    if let Some((fm, body)) = crate::reftest::autodoc::parse_frontmatter(content) {
        return (fm.title, body, fm.guide_order, fm.audience, fm.short_desc);
    }
    // No frontmatter — first H1, else fallback name.
    let mut title = fallback_name.to_string();
    for line in content.lines().take(40) {
        if let Some(t) = line.trim().strip_prefix("# ") {
            title = t.to_string();
            break;
        }
    }
    (title, content.to_string(), None, None, None)
}

/// Three-tree bucket for the guide index.
fn classify_tree(g: &Guide) -> &'static str {
    match g.audience.as_deref() {
        Some("contributor") => "contributor",
        _ => match g.guide_order {
            Some(n) if n >= 200 => "advanced",
            Some(_) => "getting-started",
            // External pages without guide_order (e.g. binding subpages)
            // — group with advanced unless slug starts with internals/.
            None if g.file_name.starts_with("internals/") => "contributor",
            None => "advanced",
        },
    }
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
    // Rewrite cross-page markdown links: `[text](other.md)` → `[text](other.html)`.
    // Agents write `.md` per markdown convention; deploy serves `.html`.
    let processed_content = rewrite_md_links(&processed_content);

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
                header_ids: Some(String::new()),
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
    let pages = get_guide_list();

    // Bucket pages by tree.
    let mut tree1: Vec<&Guide> = Vec::new(); // getting-started
    let mut tree2: Vec<&Guide> = Vec::new(); // advanced
    let mut tree3: Vec<&Guide> = Vec::new(); // contributor
    for g in &pages {
        match classify_tree(g) {
            "contributor" => tree3.push(g),
            "advanced" => tree2.push(g),
            _ => tree1.push(g),
        }
    }

    let getting_started = render_tree(&tree1);
    let advanced = render_tree(&tree2);
    let contributor = render_tree(&tree3);

    let header_tags = crate::docgen::get_common_head_tags(false);
    let sidebar = crate::docgen::get_sidebar();

    let css = "
        #guide-index { max-width: 760px; }
        #guide-index h2 {
            font-family: 'Instrument Serif', Georgia, serif;
            font-size: 1.6em;
            font-weight: normal;
            margin-top: 32px;
            margin-bottom: 12px;
            text-shadow: 0.3px 0 0 currentColor, -0.3px 0 0 currentColor;
        }
        #guide-index h2:first-child { margin-top: 8px; }
        #guide-index ul {
            list-style: none;
            padding-left: 0;
            margin: 0;
        }
        #guide-index ul ul {
            padding-left: 18px;
            margin: 4px 0 8px 0;
            border-left: 1px dashed #ccc;
        }
        #guide-index li {
            line-height: 1.35;
            margin: 0;
            padding: 4px 0;
            font-size: 15px;
        }
        #guide-index ul ul li { font-size: 14px; padding: 3px 0; }
        #guide-index a { text-decoration: none; font-weight: 500; }
        #guide-index a:hover { text-decoration: underline; }
        #guide-index .page-desc {
            color: #555;
            font-size: 13px;
            margin: 1px 0 0 16px;
            line-height: 1.4;
        }
        #guide-index ul ul .page-desc { margin-left: 14px; }
    ";

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
            <style>{css}</style>
            <div id='guide-index'>
                <h2 id='getting-started'><a href='#getting-started' style='text-decoration:none;color:inherit;'>Getting Started</a></h2>
                {getting_started}

                <h2 id='advanced'><a href='#advanced' style='text-decoration:none;color:inherit;'>Advanced</a></h2>
                {advanced}

                <h2 id='contributors'><a href='#contributors' style='text-decoration:none;color:inherit;'>Contributors</a></h2>
                {contributor}
            </div>
        </main>
        </div>
        </body>
        </html>"
    )
}

/// Render a flat list of pages as a nested tree based on slug `/` hierarchy.
/// A page with file_name `parent/child` becomes a sub-bullet under the page
/// with file_name `parent` (when that page exists in the same bucket); else
/// it's promoted to the top level under a synthetic group label.
fn render_tree(pages: &[&Guide]) -> String {
    use std::collections::BTreeMap;

    // Index by file_name for O(1) parent lookup.
    let by_name: BTreeMap<&str, &Guide> = pages
        .iter()
        .map(|g| (g.file_name.as_str(), *g))
        .collect();

    // Pages whose parent slug is also in this bucket are children; the rest
    // are top-level. Children get bucketed under their parent's file_name.
    let mut top_level: Vec<&Guide> = Vec::new();
    let mut children: BTreeMap<String, Vec<&Guide>> = BTreeMap::new();
    let mut orphan_groups: BTreeMap<String, Vec<&Guide>> = BTreeMap::new();

    for g in pages {
        if let Some(idx) = g.file_name.rfind('/') {
            let parent_slug = &g.file_name[..idx];
            if by_name.contains_key(parent_slug) {
                children
                    .entry(parent_slug.to_string())
                    .or_default()
                    .push(g);
                continue;
            }
            // No parent page exists — promote, but group under the prefix.
            orphan_groups
                .entry(parent_slug.to_string())
                .or_default()
                .push(g);
            continue;
        }
        top_level.push(g);
    }

    // Sort each level by guide_order, then by title.
    let sort_key = |g: &&Guide| (g.guide_order.unwrap_or(i32::MAX), g.title.clone());
    top_level.sort_by_key(sort_key);
    for v in children.values_mut() {
        v.sort_by_key(sort_key);
    }
    for v in orphan_groups.values_mut() {
        v.sort_by_key(sort_key);
    }

    let mut s = String::new();
    s.push_str("<ul>\n");
    for g in &top_level {
        s.push_str(&render_li(g, &children));
    }
    // Render orphan groups (e.g. `bindings/` without a `bindings.md` parent).
    for (group_slug, kids) in &orphan_groups {
        let label = group_slug.rsplit('/').next().unwrap_or(group_slug);
        let label_titled = title_case(label);
        s.push_str(&format!(
            "<li><span style='font-weight:600;'>{label_titled}</span>\n<ul>\n"
        ));
        for k in kids {
            s.push_str(&render_li(k, &children));
        }
        s.push_str("</ul>\n</li>\n");
    }
    s.push_str("</ul>\n");
    s
}

fn render_li(g: &Guide, children: &std::collections::BTreeMap<String, Vec<&Guide>>) -> String {
    let mut s = format!(
        "<li><a href=\"{HTML_ROOT}/guide/{}.html\">{}</a>",
        g.file_name, g.title,
    );
    if let Some(desc) = &g.description {
        s.push_str(&format!(
            "\n<div class=\"page-desc\">{}</div>",
            html_escape(desc),
        ));
    }
    if let Some(kids) = children.get(g.file_name.as_str()) {
        s.push_str("\n<ul>\n");
        for k in kids {
            s.push_str(&render_li(k, children));
        }
        s.push_str("</ul>\n");
    }
    s.push_str("</li>\n");
    s
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn title_case(s: &str) -> String {
    s.split('-')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().chain(c).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Rewrite `[text](path.md)` and `[text](path.md#anchor)` to `.html`.
/// Only touches link targets — `.md` inside prose / code stays untouched.
fn rewrite_md_links(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Look for a markdown link target: `](...)`
        if i + 1 < bytes.len() && bytes[i] == b']' && bytes[i + 1] == b'(' {
            out.push(']');
            out.push('(');
            i += 2;
            // Capture target up to the matching `)`. Markdown allows balanced
            // parens but autodoc-written links don't use them — bail at first `)`.
            let start = i;
            while i < bytes.len() && bytes[i] != b')' && bytes[i] != b'\n' {
                i += 1;
            }
            let target = &content[start..i];
            // Rewrite `.md` immediately before `#fragment` or end.
            if let Some(hash) = target.find('#') {
                let (path, frag) = target.split_at(hash);
                if path.ends_with(".md") {
                    out.push_str(&path[..path.len() - 3]);
                    out.push_str(".html");
                } else {
                    out.push_str(path);
                }
                out.push_str(frag);
            } else if target.ends_with(".md") {
                out.push_str(&target[..target.len() - 3]);
                out.push_str(".html");
            } else {
                out.push_str(target);
            }
            continue;
        }
        out.push(content[i..].chars().next().unwrap());
        i += content[i..].chars().next().unwrap().len_utf8();
    }
    out
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
