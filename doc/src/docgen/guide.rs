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
                            p.is_whitespace() || matches!(p, '(' | '[' | '{' | '<' | '\u{00BB}')
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
/// Latest release version from api.json (same pick as
/// `ApiData::get_latest_version_str`), cached for the process lifetime.
/// Guides write `$VERSION` in commands/URLs instead of hardcoding a
/// release number; this substitutes it at load time.
fn latest_api_version() -> &'static str {
    use std::sync::OnceLock;
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let api_path = manifest_dir.join("..").join("api.json");
        fs::read_to_string(&api_path)
            .ok()
            .and_then(|s| crate::api::ApiData::from_str(&s).ok())
            .and_then(|d| d.get_latest_version_str().map(|v| v.to_string()))
            .unwrap_or_else(|| "0.0.0".to_string())
    })
}

/// Replace `$VERSION` (and `$$VERSION$$`) markers in guide markdown with
/// the latest api.json release version. `$HOSTNAME` is left alone here —
/// guides use absolute https://azul.rs URLs by convention.
fn substitute_placeholders(content: &str) -> String {
    let v = latest_api_version();
    content.replace("$$VERSION$$", v).replace("$VERSION", v)
}

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

fn walk_collect(root: &Path, dir: &Path, out: &mut Vec<(Option<i32>, String, Guide)>) {
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
            let content = substitute_placeholders(&fs::read_to_string(&p).unwrap_or_default());
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

/// Two-tree bucket for the guide index.
pub(crate) fn classify_tree(g: &Guide) -> &'static str {
    match g.audience.as_deref() {
        // `contributor` used to be its own tree; the pages that carried it
        // are gone, so any straggler lands in advanced rather than a section
        // that no longer exists.
        Some("contributor") => "advanced",
        _ => match g.guide_order {
            Some(n) if n >= 200 => "advanced",
            Some(_) => "getting-started",
            // External pages without guide_order (e.g. binding subpages).
            None => "advanced",
        },
    }
}

/// Strip the leading `# Title` line from a markdown body. The azlin shell
/// renders the chapter title in the `.docs-hero` opener, so a body-level H1
/// would duplicate it. Only the FIRST non-blank line is considered; the raw
/// `.md` twin deployed next to the page keeps its H1 untouched.
fn strip_leading_h1(content: &str) -> String {
    let mut result: Vec<&str> = Vec::new();
    let mut seen_content = false;
    for line in content.lines() {
        if !seen_content {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            seen_content = true;
            if t.starts_with("# ") {
                continue;
            }
        }
        result.push(line);
    }
    result.join("\n")
}

/// Generate HTML for a specific guide page in the azlin docs shell.
/// The rendered markdown lands in `.docs-content` (azul-docs.css styles
/// prose/code/tables/images); guide-family extras (screenshot window
/// chrome, alerts, prev/next footer, print layout) live in docs-guide.css.
pub fn generate_guide_html(guide: &Guide, _version: &str) -> String {
    let prism_script = crate::docgen::get_prism_script();
    let search_script = crate::docgen::get_search_init(crate::docgen::PageKind::GuidePage(
        &guide.default_search_keys,
    ));

    // Pre-process content: remove mermaid blocks and expand `azul-render`
    // fences into <figure>/slideshow HTML. Use an absolute URL prefix so
    // pages at any nesting depth (`guide/dom.html` vs `guide/dom/datasets.html`)
    // resolve to the same screenshots directory.
    let processed_content = preprocess_markdown_content(&strip_leading_h1(&guide.content));
    let screenshot_prefix = format!("{HTML_ROOT}/guide/screenshots/");
    let processed_content =
        crate::reftest::autodoc::expand_azul_render_blocks(&processed_content, &screenshot_prefix);
    // Rewrite cross-page markdown links: `[text](../dom.md)` becomes the
    // absolute URL of that page. Agents write relative `.md` targets per
    // markdown convention; the deployed page needs a link that means the same
    // thing from either of its two deployed locations.
    let processed_content = rewrite_md_links(&processed_content, &guide.file_name);

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

    // Prev/next chapter links follow the linear teaching order of the
    // full guide list (same ordering as the index page).
    let all = get_guide_list();
    let pos = all.iter().position(|g| g.file_name == guide.file_name);
    let (prev, next) = match pos {
        Some(i) => (if i > 0 { all.get(i - 1) } else { None }, all.get(i + 1)),
        None => (None, None),
    };

    let mut footer =
        String::from("<div class=\"docs-guide-footer\">\n<div class=\"docs-guide-prevnext\">\n");
    if let Some(p) = prev {
        footer.push_str(&format!(
            "<a class=\"docs-guide-prev\" href=\"{HTML_ROOT}/guide/{}\">&larr; {}</a>\n",
            p.file_name,
            transform_german_quotes(&p.title),
        ));
    }
    if let Some(n) = next {
        footer.push_str(&format!(
            "<a class=\"docs-guide-next\" href=\"{HTML_ROOT}/guide/{}\">{} &rarr;</a>\n",
            n.file_name,
            transform_german_quotes(&n.title),
        ));
    }
    footer.push_str(&format!(
        "</div>\n<p class=\"docs-guide-meta\"><a href=\"{HTML_ROOT}/guide\">All chapters</a> \
         &middot; <a href=\"{HTML_ROOT}/guide/{}.md\">Markdown source</a></p>\n</div>",
        guide.file_name,
    ));

    // No lede on a chapter page. The frontmatter description exists so the
    // guide INDEX can say what a chapter covers; repeating it under the
    // chapter's own title tells the reader what they just clicked.
    let _ = &guide.description;
    let lede = String::new();

    let main_html = format!(
        r#"<section class="docs-hero">
      <div class="container">
        <h1>{title}</h1>{lede}
      </div>
    </section>
    <section class="docs-body">
      <div class="container">
        <div class="docs-layout">
        <div class="docs-content">
{content}
{footer}
        </div>
        <aside class="docs-search-rail">
          <div id="azul-search-mount" data-azs-inline></div>
        </aside>
        </div>
      </div>
    </section>"#,
        title = transform_german_quotes(&guide.title),
    );

    let page = crate::docgen::AzlinPage {
        title: guide.title.clone(),
        active_nav: "guide",
        head_extra: format!("{prism_script}\n{search_script}"),
        page_css: Some(include_str!("../../templates/docs-guide.css")),
        main_html,
    };
    crate::docgen::azlin_page(&page, false)
}

/// Generate the guide index (/ui/guide.html) in the azlin docs shell:
/// airy hero + three hairline `.docs-list` sections (no cards).
pub fn generate_guide_mainpage(_version: &str) -> String {
    let pages = get_guide_list();

    // Two trees. The contributor tree is gone with the internals pages it
    // held - it had drifted far enough from the code that it was misleading
    // rather than merely stale.
    let mut tree1: Vec<&Guide> = Vec::new(); // getting-started
    let mut tree2: Vec<&Guide> = Vec::new(); // advanced
    for g in &pages {
        match classify_tree(g) {
            "advanced" => tree2.push(g),
            _ => tree1.push(g),
        }
    }

    let getting_started = render_tree(&tree1);
    let advanced = render_tree(&tree2);

    let search_script = crate::docgen::get_search_init(crate::docgen::PageKind::Guide(&[]));

    let main_html = format!(
        r#"<section class="docs-hero">
      <div class="container">
        <h1>User guide</h1>
        <div id="azul-search-mount" class="azs-mount-inline"></div>
      </div>
    </section>
    <section class="docs-body">
      <div class="container">
        <div class="docs-content">
          <h2 id="getting-started">Getting Started</h2>
{getting_started}
          <h2 id="advanced">Advanced</h2>
{advanced}
        </div>
      </div>
    </section>"#
    );

    let page = crate::docgen::AzlinPage {
        title: "User Guide".to_string(),
        active_nav: "guide",
        head_extra: search_script,
        page_css: Some(include_str!("../../templates/docs-guide.css")),
        main_html,
    };
    crate::docgen::azlin_page(&page, false)
}

/// Render the chapters of one tree as a `.guide-grid` of `.guide-card`s.
/// A page with file_name `parent/child` becomes a sub-bullet under the page
/// with file_name `parent` (when that page exists in the same bucket); else
/// it's promoted to the top level under a synthetic group label.
fn render_tree(pages: &[&Guide]) -> String {
    use std::collections::BTreeMap;

    // Index by file_name for O(1) parent lookup.
    let by_name: BTreeMap<&str, &Guide> =
        pages.iter().map(|g| (g.file_name.as_str(), *g)).collect();

    // Pages whose parent slug is also in this bucket are children; the rest
    // are top-level. Children get bucketed under their parent's file_name.
    let mut top_level: Vec<&Guide> = Vec::new();
    let mut children: BTreeMap<String, Vec<&Guide>> = BTreeMap::new();
    let mut orphan_groups: BTreeMap<String, Vec<&Guide>> = BTreeMap::new();

    for g in pages {
        if let Some(idx) = g.file_name.rfind('/') {
            let parent_slug = &g.file_name[..idx];
            if by_name.contains_key(parent_slug) {
                children.entry(parent_slug.to_string()).or_default().push(g);
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

    // Both kinds of card - a chapter with its sub-articles, and a directory
    // that has articles but no landing page (`system/`, `data/`) - go into ONE
    // ordered list. Rendering the directories afterwards would drop them at
    // the end of the grid regardless of where they belong in the reading
    // order; a group sorts by its earliest article.
    let mut cards: Vec<(i32, String, String)> = Vec::new();
    for g in &top_level {
        cards.push((
            g.guide_order.unwrap_or(i32::MAX),
            g.title.clone(),
            render_list_item(g, &children),
        ));
    }
    for (group_slug, kids) in &orphan_groups {
        let label = group_slug.rsplit('/').next().unwrap_or(group_slug);
        let label_titled = title_case(label);
        let order = kids
            .iter()
            .filter_map(|k| k.guide_order)
            .min()
            .unwrap_or(i32::MAX);
        let mut card = format!(
            "<div class=\"guide-card\">\n<h3>{label_titled}</h3>\n<div class=\"guide-links\">\n"
        );
        for k in kids {
            card.push_str(&render_sub_li(k, &children));
        }
        card.push_str("</div>\n</div>\n");
        cards.push((order, label_titled, card));
    }
    cards.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));

    let mut s = String::new();
    s.push_str("<div class=\"guide-grid\">\n");
    for (_, _, card) in &cards {
        s.push_str(card);
    }
    s.push_str("</div>\n");
    s
}

/// One top-level chapter as a `.guide-card`: heading, one-line description,
/// and a button per article (the chapter's own page first, as
/// "Introduction").
fn render_list_item(
    g: &Guide,
    children: &std::collections::BTreeMap<String, Vec<&Guide>>,
) -> String {
    let kids: &[&Guide] = children
        .get(g.file_name.as_str())
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    // The heading is the card's link ONLY when the card has nothing else to
    // link to. As soon as a chapter has sub-articles, its own page becomes
    // the first button ("Introduction") and the heading goes back to being a
    // heading - otherwise the card has two kinds of target, a quiet one in
    // the title and loud ones below it, and the reader has to work out that
    // the title was clickable at all.
    let title = transform_german_quotes(&g.title);
    let head = if kids.is_empty() {
        format!(
            "<h3><a class=\"guide-link guide-link-lead\" href=\"{HTML_ROOT}/guide/{}\">{title}</a></h3>\n",
            g.file_name,
        )
    } else {
        format!("<h3>{title}</h3>\n")
    };

    // A card with a lot of articles (Hello World carries one per language)
    // takes the full row. In a column it wraps into a tower and leaves the
    // card beside it floating in whitespace.
    let wide = if kids.len() > 8 {
        " guide-card-wide"
    } else {
        ""
    };
    let mut s = format!("<div class=\"guide-card{wide}\">\n{head}");
    if let Some(desc) = &g.description {
        s.push_str(&format!(
            "<p>{}</p>\n",
            transform_german_quotes(&html_escape(desc)),
        ));
    }
    if !kids.is_empty() {
        s.push_str("<div class=\"guide-links\">\n");
        s.push_str(&format!(
            "<a class=\"guide-link\" href=\"{HTML_ROOT}/guide/{}\">Introduction</a>\n",
            g.file_name,
        ));
        for k in kids {
            s.push_str(&render_sub_li(k, children));
        }
        s.push_str("</div>\n");
    }
    s.push_str("</div>\n");
    s
}

/// Sub-chapter bullet (nested pages like `hello-world/rust`).
fn render_sub_li(g: &Guide, _children: &std::collections::BTreeMap<String, Vec<&Guide>>) -> String {
    // One button per article. The one-line description is dropped: the card's
    // own blurb already says what the group covers, and repeating a sentence
    // under every button is the prose this index is meant to be free of.
    format!(
        "<a class=\"guide-link\" href=\"{HTML_ROOT}/guide/{}\">{}</a>\n",
        g.file_name,
        transform_german_quotes(&index_label(g)),
    )
}

/// The label a sub-article carries INSIDE its card.
///
/// One case, `hello-world/*`: 28 buttons that each repeat "Hello World" fill
/// the widest card on the page with the two words the card heading already
/// says. Under that heading the language alone is the whole label. The page's
/// own title is untouched - this is the index view only.
fn index_label(g: &Guide) -> String {
    if g.file_name.starts_with("hello-world/") {
        if let Some(lang) = g
            .title
            .strip_prefix("Hello World [")
            .and_then(|r| r.strip_suffix(']'))
        {
            return lang.to_string();
        }
    }
    g.title.clone()
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

/// Rewrite `[text](path.md)` and `[text](path.md#anchor)` to the page's
/// absolute site URL, resolving the relative target against `slug` (the
/// linking page's own slug, e.g. `events/callbacks`).
///
/// Absolute, not extensionless-relative, because every page is deployed
/// TWICE — `<stem>.html` and `<stem>/index.html`, so the clean URL resolves
/// on any static host. Those two twins sit at different depths, so a
/// relative `../dom` in the body means two different things depending on
/// which one the host served. An absolute href means one thing everywhere.
/// Only link targets are touched — `.md` inside prose / code stays as it is.
fn rewrite_md_links(content: &str, slug: &str) -> String {
    // The linking page's directory, `""` for a top-level chapter.
    let dir = slug.rsplit_once('/').map_or("", |(d, _)| d);

    /// `../dom` seen from `events/callbacks` is `dom`. Purely textual — the
    /// guide has no symlinks and no `.` segments.
    fn resolve(dir: &str, target: &str) -> String {
        let mut segs: Vec<&str> = if dir.is_empty() {
            Vec::new()
        } else {
            dir.split('/').collect()
        };
        for seg in target.split('/') {
            match seg {
                "." | "" => {}
                ".." => {
                    segs.pop();
                }
                s => segs.push(s),
            }
        }
        segs.join("/")
    }

    let mut out = String::with_capacity(content.len());
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b']' && bytes[i + 1] == b'(' {
            out.push(']');
            out.push('(');
            i += 2;
            let start = i;
            while i < bytes.len() && bytes[i] != b')' && bytes[i] != b'\n' {
                i += 1;
            }
            let target = &content[start..i];
            let (path, frag) = match target.find('#') {
                Some(h) => target.split_at(h),
                None => (target, ""),
            };
            match path.strip_suffix(".md") {
                // An external or already-absolute `.md` target keeps its
                // shape; only the extension goes.
                Some(p) if p.starts_with("http") || p.starts_with('/') => {
                    out.push_str(p);
                }
                Some(p) => {
                    out.push_str(&format!("{HTML_ROOT}/guide/{}", resolve(dir, p)));
                }
                None => out.push_str(path),
            }
            out.push_str(frag);
            continue;
        }
        out.push(content[i..].chars().next().unwrap());
        i += content[i..].chars().next().unwrap().len_utf8();
    }
    out
}

#[cfg(test)]
mod guide_contract {
    use super::*;

    /// Every card on the index owns sub-articles.
    ///
    /// A chapter with no sub-pages renders as a lone clickable heading among
    /// cards full of buttons, which is the shape this regrouping removed. A
    /// new page therefore belongs in a topic directory - `events/`, `system/`,
    /// `data/` - not at the root of `doc/guide/en/`.
    #[test]
    fn no_index_card_stands_alone() {
        use std::collections::{BTreeMap, BTreeSet};
        let pages = get_guide_list();
        let names: BTreeSet<&str> = pages.iter().map(|g| g.file_name.as_str()).collect();
        let mut kids: BTreeMap<&str, usize> = BTreeMap::new();
        for g in &pages {
            if let Some(idx) = g.file_name.rfind('/') {
                *kids.entry(&g.file_name[..idx]).or_default() += 1;
            }
        }
        let lonely: Vec<&str> = names
            .iter()
            .copied()
            .filter(|n| !n.contains('/') && !kids.contains_key(n))
            .collect();
        assert!(
            lonely.is_empty(),
            "guide pages with no sub-articles (each would be a card with one \
             link): {lonely:?}"
        );
    }
}
