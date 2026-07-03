use comrak::options::{Extension, Parse, Plugins, Render, RenderPlugins};

use super::{azlin_page, AzlinPage, HTML_ROOT};

/// Blog post information structure
pub struct BlogPost {
    /// Title for the post (extracted from first H1 or filename)
    pub title: String,
    /// File name derived from the .md filename (for URL)
    pub file_name: String,
    /// Date of the post (extracted from filename: YYYY-MM-DD-title.md)
    pub date: String,
    /// Raw markdown content
    pub content: String,
}

/// Create a BlogPost from a markdown file path, content
fn blog_post_from_md(md_filename: &str, content: &'static str) -> BlogPost {
    // Expected format: YYYY-MM-DD-title.md
    let file_name = md_filename.trim_end_matches(".md").to_string();

    // Extract date from filename (first 10 chars: YYYY-MM-DD)
    let date = if file_name.len() >= 10
        && file_name.chars().nth(4) == Some('-')
        && file_name.chars().nth(7) == Some('-')
    {
        file_name[0..10].to_string()
    } else {
        "Unknown".to_string()
    };

    // Extract title from first H1 header in content
    let title = content
        .lines()
        .find(|line| line.starts_with("# "))
        .map(|line| line.trim_start_matches("# ").to_string())
        .unwrap_or_else(|| {
            // Fallback: derive title from filename after date
            if file_name.len() > 11 {
                file_name[11..].replace('-', " ")
            } else {
                file_name.clone()
            }
        });

    BlogPost {
        title,
        file_name,
        date,
        content: content.to_string(),
    }
}

/// Pre-process markdown content:
/// - Remove first H1 header (we display it separately)
/// - Remove mermaid code blocks (not supported in HTML output)
fn preprocess_markdown_content(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut in_mermaid_block = false;
    let mut removed_first_h1 = false;

    for line in lines {
        // Remove first H1 header
        if !removed_first_h1 && line.starts_with("# ") {
            removed_first_h1 = true;
            continue;
        }

        // Remove mermaid code blocks
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

    crate::docgen::guide::transform_german_quotes(&result.join("\n"))
}

/// Get a list of all blog posts
pub fn get_blog_list() -> Vec<BlogPost> {
    vec![blog_post_from_md(
        "2025-12-13-hello-world",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/blog/2025-12-13-hello-world.md"
        )),
    )]
}

/// Minimal HTML escape for text that came out of markdown source.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Strip markdown inline links: `[text](url)` becomes `text`.
fn strip_md_links(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('[') {
        let after = &rest[start..];
        if let Some(mid_rel) = after.find("](") {
            if let Some(end_rel) = after[mid_rel..].find(')') {
                out.push_str(&rest[..start]);
                out.push_str(&after[1..mid_rel]);
                rest = &after[mid_rel + end_rel + 1..];
                continue;
            }
        }
        break;
    }
    out.push_str(rest);
    out
}

/// First paragraph of the post (after the H1), de-markdowned, for the
/// blog index excerpt.
fn post_excerpt(post: &BlogPost) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut started = false;
    for line in post.content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.starts_with("```") {
            if started {
                break;
            }
            continue;
        }
        if trimmed.is_empty() {
            if started {
                break;
            }
            continue;
        }
        started = true;
        lines.push(trimmed);
    }
    let text = strip_md_links(&lines.join(" ")).replace("**", "").replace('`', "");
    escape_html(&text)
}

/// Render the post's markdown body to HTML (shared prose/code/img styles in
/// azul-docs.css cover the output; Prism handles code fences client-side).
fn render_markdown(content: &str) -> String {
    comrak::markdown_to_html_with_plugins(
        content,
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
    )
}

/// Generate HTML for a specific blog post (azlin docs shell).
pub fn generate_blog_post_html(post: &BlogPost) -> String {
    let processed_content = preprocess_markdown_content(&post.content);
    let content = render_markdown(&processed_content);

    let title = escape_html(&post.title);
    let date = &post.date;

    let main_html = format!(
        r#"    <section class="docs-hero">
      <div class="container">
        <h1>{title}</h1>
        <p class="docs-lede docs-meta">Posted on {date}</p>
      </div>
    </section>
    <section class="docs-body">
      <div class="container">
        <div class="docs-layout">
        <div class="docs-content">
{content}
          <hr/>
          <p><a href="{HTML_ROOT}/blog">&larr; Back to blog</a></p>
        </div>
        <aside class="docs-search-rail">
          <div id="azul-search-mount" data-azs-inline></div>
        </aside>
        </div>
      </div>
    </section>"#
    );

    azlin_page(
        &AzlinPage {
            title: format!("{} - Azul Blog", post.title),
            active_nav: "blog",
            // Sticky API-search rail next to the post body (the post's
            // reading measure leaves the right gutter free for results).
            head_extra: format!(
                "{}\n{}",
                crate::docgen::get_prism_script(),
                crate::docgen::get_search_init(crate::docgen::PageKind::Other)
            ),
            page_css: None,
            main_html,
        },
        true,
    )
}

/// Generate the blog index page listing all posts (azlin docs shell).
pub fn generate_blog_index() -> String {
    // Get all blog posts, sorted by date (newest first)
    let mut posts = get_blog_list();
    posts.sort_by(|a, b| b.date.cmp(&a.date));

    let mut post_items = String::new();
    for post in &posts {
        let href = format!("{HTML_ROOT}/blog/{}.html", post.file_name);
        let title = escape_html(&post.title);
        let excerpt = post_excerpt(post);
        post_items.push_str(&format!(
            r#"        <article class="docs-list-item">
          <h3><a href="{href}">{title}</a></h3>
          <p class="docs-meta">Posted on {date}</p>
          <p>{excerpt}</p>
          <a class="docs-read-more" href="{href}">Read more &rarr;</a>
        </article>
"#,
            date = post.date,
        ));
    }

    let main_html = format!(
        r#"    <section class="docs-hero">
      <div class="container">
        <p class="docs-eyebrow">Blog</p>
        <h1>Blog</h1>
        <p class="docs-lede">News, updates, and tutorials for the Azul GUI framework.</p>
      </div>
    </section>
    <section class="docs-body">
      <div class="container">
        <div class="docs-list">
{post_items}        </div>
      </div>
    </section>"#
    );

    azlin_page(
        &AzlinPage {
            title: "Blog - Azul GUI framework".to_string(),
            active_nav: "blog",
            head_extra: String::new(),
            page_css: None,
            main_html,
        },
        true,
    )
}
