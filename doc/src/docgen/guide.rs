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
    /// API entries to pre-populate in the search panel for this page.
    /// Each is either `Name` or `Class.member`. Empty when the
    /// frontmatter omits `default-search-keys`.
    pub default_search_keys: Vec<String>,
}

/// Pre-process markdown content:
/// - Remove mermaid code blocks (not supported in HTML output)
/// - Strip rustdoc directive suffixes from fence tags so the
///   syntax highlighter recognises them (`rust,no_run` -> `rust`,
///   `rust,ignore` -> `rust`). Prism keys highlighting off the
///   bare language name and would otherwise emit
///   `class="language-rust,no_run"` which doesn't match any rule.
/// - Transform straight `"` into German-style „…" quotes outside of code
/// (Frontmatter is stripped earlier, in `get_guide_list`, so it never
/// reaches this stage.)
fn preprocess_markdown_content(content: &str) -> String {
    // Remove mermaid code blocks. Normalise rustdoc directive suffixes on
    // any other fence opening so Prism gets the bare language name
    // (`rust,no_run` -> `rust`, `rust,ignore` -> `rust`).
    let mut result: Vec<String> = Vec::new();
    let mut in_mermaid_block = false;
    let mut in_fence = false;

    for line in content.lines() {
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
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") && !in_fence {
            in_fence = true;
            // Strip everything after the first comma in the language tag.
            // Preserves leading indent. Skips azul-render fences which
            // carry attribute syntax that the autodoc expander parses.
            if !trimmed.starts_with("```azul-render") {
                if let Some(comma_idx) = trimmed.find(',') {
                    let prefix_len = line.len() - trimmed.len();
                    let lead = &line[..prefix_len];
                    let bare = &trimmed[..comma_idx];
                    result.push(format!("{}{}", lead, bare));
                    continue;
                }
            }
        } else if trimmed.starts_with("```") && in_fence {
            in_fence = false;
        }
        result.push(line.to_string());
    }

    let joined = result.join("\n");
    transform_german_quotes(&joined)
}

/// Replace straight `"` with German-style „ (opening) / " (closing).
/// The choice is contextual, not a toggle: a `"` preceded by whitespace
/// or start-of-line opens, one preceded by a word character closes. A
/// single unmatched `"` anywhere in the document used to flip every
/// subsequent pair, which is why a stateful toggle was wrong.
///
/// Skips fenced code blocks (``` ... ```) and inline code spans (`...`).
/// HTML attribute quotes inside markdown body would also be rewritten,
/// so avoid raw `<tag attr="...">` in body prose.
pub fn transform_german_quotes(content: &str) -> String {
    const OPEN: char = '\u{201E}'; // „
    const CLOSE: char = '\u{201C}'; // "

    let mut out = String::with_capacity(content.len());
    let mut in_fence = false;
    let mut prev_ch: Option<char> = None;

    for line in content.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            out.push_str(line);
            prev_ch = line.chars().last();
            continue;
        }
        if in_fence {
            out.push_str(line);
            prev_ch = line.chars().last();
            continue;
        }
        let mut in_inline_code = false;
        for ch in line.chars() {
            match ch {
                '`' => {
                    in_inline_code = !in_inline_code;
                    out.push(ch);
                }
                '"' if !in_inline_code => {
                    // Open if there's nothing before us, or the previous
                    // character is whitespace, an opening bracket, or
                    // sentence-leading punctuation. Otherwise close.
                    let opens = match prev_ch {
                        None => true,
                        Some(p) => {
                            p.is_whitespace()
                                || matches!(p, '(' | '[' | '{' | '<' | '\u{00BB}')
                        }
                    };
                    out.push(if opens { OPEN } else { CLOSE });
                }
                _ => out.push(ch),
            }
            prev_ch = Some(ch);
        }
    }
    out
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
            let (title, body, guide_order, audience, description, default_search_keys) =
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
                    default_search_keys,
                },
            ));
        }
    }
}

fn extract_metadata(
    content: &str,
    fallback_name: &str,
) -> (
    String,
    String,
    Option<i32>,
    Option<String>,
    Option<String>,
    Vec<String>,
) {
    if let Some((fm, body)) = crate::reftest::autodoc::parse_frontmatter(content) {
        return (
            fm.title,
            body,
            fm.guide_order,
            fm.audience,
            fm.short_desc,
            fm.default_search_keys,
        );
    }
    // No frontmatter — first H1, else fallback name.
    let mut title = fallback_name.to_string();
    for line in content.lines().take(40) {
        if let Some(t) = line.trim().strip_prefix("# ") {
            title = t.to_string();
            break;
        }
    }
    (title, content.to_string(), None, None, None, Vec::new())
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
    let search_script = crate::docgen::get_search_init(
        crate::docgen::PageKind::GuidePage(&guide.default_search_keys),
    );

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
    // Rewrite cross-page markdown links: `[text](other.md)` → `[text](other)`.
    // Agents write `.md` per markdown convention; the deployed site uses clean
    // (extensionless) URLs — the static host serves `other.html` for `other`,
    // and the redirect mirror canonicalizes any stray `.html` hit.
    let processed_content = rewrite_md_links(&processed_content);

    let content = comrak::markdown_to_html_with_plugins(
        &processed_content,
        &comrak::Options {
            render: Render {
                // We control the input - the <figure>/<img> emitted by
                // expand_azul_render_blocks would otherwise be stripped to
                // <!-- raw HTML omitted -->.
                r#unsafe: true,
                ..Render::default()
            },
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
            font-size: 2em;
            font-weight: 700;
            line-height: 1.15;
            margin-top: 0;
            margin-bottom: 25px;
            letter-spacing: 0.01em;
        }
        @media screen and (max-width: 768px) {
            h1 { font-size: 1.8em; }
        }
        @media screen and (max-width: 480px) {
            h1 { font-size: 1.5em; }
        }
        h2, h3, h4 { cursor: pointer; }
        h2 {
            font-family: 'Imbue', Georgia, serif;
            font-size: 2em;
            font-weight: normal;
            margin-top: 25px;
            margin-bottom: 15px;
        }
        h3 { margin-top: 22px; margin-bottom: 10px; font-size: 1.3em; }
        h4 { margin-top: 18px; margin-bottom: 8px; font-size: 1.1em; }
        #guide { max-width: 700px; line-height: 1.7; font-size: 1.1em; }
        /* D1/R4-6: the search sits in a column beside the article when there
           is room. When the viewport is too narrow for both, it does NOT push
           the layout (and never displaces the h1) — instead it OVERLAYS the
           text mobile-style: pinned to the bottom of the viewport, with a
           gradient above it so the text appears to fade out underneath, and
           extra bottom padding so the last lines never hide under the bar. */
        .guide-layout { display: flex; flex-wrap: nowrap; align-items: flex-start; gap: 36px; }
        .guide-layout > #guide { flex: 1 1 auto; min-width: 0; padding-bottom: 80px; }
        .guide-search-col { flex: 1 1 300px; min-width: 300px; position: sticky; top: 20px; align-self: flex-start; padding: 0; display: flex; }
        .guide-search-col .page-search { max-width: 100%; display: flex; flex-grow: 1; min-width: 100%; }
        .guide-search-col .azul-search { flex: 1 1 auto; min-width: 0; width: 100%; }
        /* overlay bar/fade colour = the guide content background, so the text
           dissolves cleanly under the bar. White in light mode, dark in dark. */
        .guide-search-col { --fade-bg: #ffffff; }
        @media (prefers-color-scheme: dark) { .guide-search-col { --fade-bg: #15181f; } }
        @media (max-width: 1100px) {
            .guide-layout { display: block; }
            .guide-layout > #guide { padding-bottom: 200px; }
            .guide-search-col {
                position: fixed; left: 0; right: 0; bottom: 0; top: auto; z-index: 9000;
                margin: 0; padding: 0; pointer-events: none;
            }
            /* full-width bar, flush to the bottom edge */
            .guide-search-col .page-search {
                max-width: 100%; margin: 0; pointer-events: auto;
                background: var(--fade-bg);
                padding: 12px 10px calc(12px + env(safe-area-inset-bottom));
            }
            /* fade: text dissolves into the content background above the bar */
            .guide-search-col::before {
                content: ''; position: absolute; left: 0; right: 0; bottom: 100%; height: 96px;
                pointer-events: none;
                background: linear-gradient(to bottom, transparent, var(--fade-bg));
            }
            /* bar lives at the bottom -> results open UPWARD, full width */
            .guide-search-col .azs-panel-inline { top: auto; bottom: calc(100% + 6px); left: 10px; right: 10px; }
        }
        #guide p { margin-bottom: 1em; }
        #guide img { max-width: 700px; margin-top: 15px; margin-bottom: 15px;}
        #guide ul, #guide ol {
            margin-top: 15px;
            margin-bottom: 15px;
            margin-left: 30px;
        }
        #guide li {
            font-size: 16px;
            margin-bottom: 0.6em;
        }
        #guide li > p { margin-bottom: 0.3em; }
        #guide li:last-child { margin-bottom: 0; }
        #guide code {
            font-family: 'Red Hat Mono', ui-monospace, SFMono-Regular, Menlo, monospace;
            font-weight: bold;
            font-size: 0.75em;
            border-radius: 5px;
            padding: 2px 5px;
        }
        #guide pre code {
            font-weight: normal;
            font-family: 'Red Hat Mono', ui-monospace, SFMono-Regular, Menlo, monospace;
            font-size: 10pt;
            margin-top: 5px;
            margin-bottom: 5px;
            display: block;
            padding: 3px;
            border-radius: 3px;
            white-space: pre;
            overflow-x: auto;
        }
        /* Dark theme for fenced code blocks - guide pages only.
           The index/landing pages don't render `#guide`, so main.css's
           light Prism theme stays in effect there. */
        #guide pre[class*=\"language-\"],
        #guide pre code[class*=\"language-\"],
        #guide pre code {
            background: #1e1e1e;
            color: #d4d4d4;
            text-shadow: none;
            padding: 12px 14px;
            border-radius: 4px;
        }
        #guide pre .token.comment,
        #guide pre .token.prolog,
        #guide pre .token.doctype,
        #guide pre .token.cdata { color: #6a9955; font-style: italic; }
        #guide pre .token.punctuation { color: #d4d4d4; }
        #guide pre .token.property,
        #guide pre .token.tag,
        #guide pre .token.boolean,
        #guide pre .token.number,
        #guide pre .token.constant,
        #guide pre .token.symbol,
        #guide pre .token.deleted { color: #b5cea8; }
        #guide pre .token.selector,
        #guide pre .token.attr-name,
        #guide pre .token.string,
        #guide pre .token.char,
        #guide pre .token.builtin,
        #guide pre .token.inserted { color: #ce9178; }
        #guide pre .token.operator,
        #guide pre .token.entity,
        #guide pre .token.url { color: #d4d4d4; background: transparent; }
        #guide pre .token.atrule,
        #guide pre .token.attr-value,
        #guide pre .token.keyword { color: #c586c0; }
        #guide pre .token.function,
        #guide pre .token.class-name { color: #dcdcaa; }
        #guide pre .token.regex,
        #guide pre .token.important,
        #guide pre .token.variable { color: #9cdcfe; }
        #guide pre .token.lifetime-annotation { color: #4ec9b0; }
        /* Fake-window chrome for azul-render screenshots. The figure wraps
           a window-shaped frame so the reader sees a screenshot, not an
           embedded UI. The subtitle goes in the titlebar instead of a
           separate <figcaption> below. */
        .azul-screenshot { margin: 24px 0; text-align: center; }
        .azul-window {
            display: inline-block;
            text-align: left;
            border: 1px solid #b8b8b8;
            border-radius: 8px;
            overflow: hidden;
            box-shadow: 0 6px 18px rgba(0, 0, 0, 0.18);
            background: #fff;
            max-width: 100%;
        }
        .azul-titlebar {
            display: flex;
            align-items: center;
            padding: 7px 12px;
            background: linear-gradient(to bottom, #ececec, #d6d6d6);
            border-bottom: 1px solid #b8b8b8;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
            font-size: 12px;
            color: #444;
            user-select: none;
        }
        .azul-tb-traffic {
            display: inline-flex;
            gap: 6px;
            margin-right: 12px;
            flex-shrink: 0;
        }
        .azul-tb-traffic > span {
            display: inline-block;
            width: 11px;
            height: 11px;
            border-radius: 50%;
            border: 0.5px solid rgba(0, 0, 0, 0.18);
        }
        .azul-tb-close { background: #ff5f57; }
        .azul-tb-min { background: #febc2e; }
        .azul-tb-max { background: #28c840; }
        .azul-tb-title {
            flex: 1;
            text-align: center;
            padding-right: 60px; /* offset traffic-light width so title is visually centred */
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
        }
        .azul-window img { display: block; max-width: 100%; height: auto; }
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
        /* Print / PDF (azul-doc → Chrome print-to-pdf): drop the nav chrome,
           use the full page width, WRAP long code lines (on-screen they
           overflow-x:auto, which would clip them in a PDF), force code blocks to
           a LIGHT theme (the dark on-screen theme wastes toner and reads poorly
           on paper — web keeps dark for content/code contrast), and keep
           figures/screenshots/tables from being split across page breaks. */
        @media print {
            @page { margin: 1.4cm; }
            aside, nav, footer, #azul-search-mount { display: none !important; }
            body, .center, main { display: block !important; margin: 0 !important; padding: 0 !important; max-width: none !important; }
            #guide { max-width: 100% !important; width: 100% !important; font-size: 11pt; line-height: 1.5; }
            h1, h2, h3, h4 { text-shadow: none !important; cursor: auto !important; break-after: avoid; }
            #guide pre,
            #guide pre code,
            #guide pre[class*=\"language-\"],
            #guide pre code[class*=\"language-\"] {
                white-space: pre-wrap !important;
                word-break: break-word;
                overflow: visible !important;
                font-size: 8.5pt;
                background: #f6f8fa !important;
                color: #24292f !important;
                border: 1px solid #d0d7de !important;
                text-shadow: none !important;
                -webkit-print-color-adjust: exact;
                print-color-adjust: exact;
            }
            /* Light syntax palette for print (GitHub-light flavored), overriding
               the dark on-screen token colors above. */
            #guide pre .token.comment,
            #guide pre .token.prolog,
            #guide pre .token.doctype,
            #guide pre .token.cdata { color: #6e7781 !important; font-style: italic; }
            #guide pre .token.punctuation,
            #guide pre .token.operator,
            #guide pre .token.entity,
            #guide pre .token.url { color: #24292f !important; background: transparent !important; }
            #guide pre .token.property,
            #guide pre .token.tag,
            #guide pre .token.boolean,
            #guide pre .token.number,
            #guide pre .token.constant,
            #guide pre .token.symbol,
            #guide pre .token.deleted { color: #0550ae !important; }
            #guide pre .token.selector,
            #guide pre .token.attr-name,
            #guide pre .token.string,
            #guide pre .token.char,
            #guide pre .token.builtin,
            #guide pre .token.inserted { color: #0a3069 !important; }
            #guide pre .token.atrule,
            #guide pre .token.attr-value,
            #guide pre .token.keyword { color: #cf222e !important; }
            #guide pre .token.function,
            #guide pre .token.class-name { color: #8250df !important; }
            #guide pre .token.regex,
            #guide pre .token.important,
            #guide pre .token.variable { color: #e36209 !important; }
            #guide pre .token.lifetime-annotation { color: #1a7f64 !important; }
            #guide img, .azul-window, .azul-screenshot, table, figure, .markdown-alert-warning {
                break-inside: avoid;
                -webkit-print-color-adjust: exact;
                print-color-adjust: exact;
            }
            a { color: inherit; text-decoration: underline; }
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
            <div class='guide-layout'>
            <div id='guide'>
            <style>
                {css}
            </style>
            {content}
            <p style='font-size:1.2em;margin-top:20px;'>
            <a href='{HTML_ROOT}/guide'>Back to guide index</a>
            </p>
            </div>
            <aside class='guide-search-col'>
            <div id='azul-search-mount' class='azs-mount-inline page-search'></div>
            </aside>
            </div>
        </main>

        </div>
        {prism_script}
        {search_script}
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
    let search_script = crate::docgen::get_search_init(crate::docgen::PageKind::Guide(&[]));

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
            <div class='guide-layout'>
            <div class='guide-main'>
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
            </div>
            <aside class='guide-search-col'>
            <div id='azul-search-mount' class='azs-mount-inline page-search'></div>
            </aside>
            </div>
        </main>
        </div>
        {search_script}
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
        "<li><a href=\"{HTML_ROOT}/guide/{}\">{}</a>",
        g.file_name,
        transform_german_quotes(&g.title),
    );
    if let Some(desc) = &g.description {
        s.push_str(&format!(
            "\n<div class=\"page-desc\">{}</div>",
            transform_german_quotes(&html_escape(desc)),
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

/// Rewrite `[text](path.md)` and `[text](path.md#anchor)` to an extensionless
/// link (`path`) — the static host serves `path.html` for a request to `path`.
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
                } else {
                    out.push_str(path);
                }
                out.push_str(frag);
            } else if target.ends_with(".md") {
                out.push_str(&target[..target.len() - 3]);
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
            "<li><a href=\"{HTML_ROOT}/guide/{version}\">{version}</a></li>\n",
        ));
    }

    let header_tags = crate::docgen::get_common_head_tags(false);
    let sidebar = crate::docgen::get_sidebar();
    let search_script = crate::docgen::get_search_init(crate::docgen::PageKind::Guide(&[]));

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
            <div class='guide-layout'>
            <div class='guide-main'>
            <h1>Choose guide version</h1>
            <div>
            <ul>{version_items}</ul>
            </div>
            </div>
            <aside class='guide-search-col'>
            <div id='azul-search-mount' class='azs-mount-inline page-search'></div>
            </aside>
            </div>
        </main>
        </div>
        {search_script}
        </body>
        </html>"
    )
}
