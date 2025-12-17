use comrak::options::{Extension, Parse, Plugins, Render, RenderPlugins};

use super::HTML_ROOT;

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
    let date = if file_name.len() >= 10 && file_name.chars().nth(4) == Some('-') && file_name.chars().nth(7) == Some('-') {
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

    result.join("\n")
}

/// Get a list of all blog posts
pub fn get_blog_list() -> Vec<BlogPost> {
    vec![
        blog_post_from_md(
            "2025-12-13-hello-world",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/blog/2025-12-13-hello-world.md"
            )),
        ),
    ]
}

/// Generate HTML for a specific blog post
pub fn generate_blog_post_html(post: &BlogPost) -> String {
    let header_tags = crate::docgen::get_common_head_tags(false);
    let sidebar = crate::docgen::get_sidebar();
    let prism_script = crate::docgen::get_prism_script();

    // Pre-process content
    let processed_content = preprocess_markdown_content(&post.content);

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

    let title = &post.title;
    let date = &post.date;

    let css = "
        h1 { 
            font-family: 'Instrument Serif', Georgia, serif;
            font-size: clamp(2.2em, 4.5vw, 3.5em);
            font-weight: normal;
            line-height: 1.0;
            margin-top: 0;
            margin-bottom: 20px;
            text-shadow: currentColor 0.5px 0.5px 0.5px, currentColor -0.5px -0.5px 0.5px, currentColor 0px -0.5px 0.5px, currentColor -0.5px 0px 0.5px;
            letter-spacing: 0.02em;
        }
        h2, h3, h4 { cursor: pointer; }
        h2 { 
            font-family: 'Instrument Serif', Georgia, serif;
            font-size: 2em;
            font-weight: normal;
            margin-top: 30px; 
            margin-bottom: 10px;
            text-shadow: 0.3px 0 0 currentColor, -0.3px 0 0 currentColor;
        }
        h3 { margin-top: 25px; margin-bottom: 5px; }
        h4 { margin-top: 20px; margin-bottom: 5px; }
        #blog { max-width: 700px; line-height: 1.5; font-size: 1.2em; }
        #blog img { max-width: 700px; margin-top: 10px; margin-bottom: 10px;}
        #blog ul, #blog ol {
            margin-top: 10px;
            margin-bottom: 10px;
            margin-left: 30px;
        }
        #blog li {
            font-size: 1em;
        }
        #blog p {
            margin-bottom: 15px;
            margin-top: 10px;
        }
        #blog code {
            font-family: monospace;
            font-weight: bold;
            border-radius: 3px;
            padding: 2.5px 10px;
        }
        #blog pre code {
            font-weight: bold;
            font-family: monospace;
            margin-top: 10px;
            margin-bottom: 10px;
            display: flex;
            flex-direction: column;
            padding: 10px;
            border-radius: 3px;
        }
        .blog-date {
            color: #666;
            font-style: italic;
            margin-bottom: 20px;
        }
    ";

    format!(
        "<!DOCTYPE html>
        <html lang='en'>
        <head>
        <title>{title} - Azul Blog</title>

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
            <div id='blog'>
            <style>
                {css}
            </style>
            <h1>{title}</h1>
            <p class='blog-date'>Posted on {date}</p>
            {content}
            </div>
            <p style='font-size:1.2em;margin-top:20px;'>
            <a href='{HTML_ROOT}/blog.html'>Back to blog index</a>
            </p>
        </main>

        </div>
        {prism_script}
        </body>
        </html>"
    )
}

/// Generate the blog index page listing all posts
pub fn generate_blog_index() -> String {
    let header_tags = crate::docgen::get_common_head_tags(false);
    let sidebar = crate::docgen::get_sidebar();

    // Get all blog posts, sorted by date (newest first)
    let mut posts = get_blog_list();
    posts.sort_by(|a, b| b.date.cmp(&a.date));

    let mut post_items = String::new();
    for post in &posts {
        post_items.push_str(&format!(
            "<li class='blog-item'>
                <span class='blog-date'>{}</span>
                <a href='{HTML_ROOT}/blog/{}.html'>{}</a>
            </li>\n",
            post.date, post.file_name, post.title,
        ));
    }

    let css = "
        #blog-index { max-width: 700px; line-height: 1.5; font-size: 1.2em; }
        #blog-index ul { list-style: none; margin-left: 0; }
        #blog-index .blog-item {
            padding: 15px 0;
            border-bottom: 1px solid #eee;
        }
        #blog-index .blog-date {
            color: #666;
            font-size: 0.9em;
            margin-right: 15px;
        }
        #blog-index a {
            color: #0446bf;
            text-decoration: none;
        }
        #blog-index a:hover {
            text-decoration: underline;
        }
    ";

    format!(
        "<!DOCTYPE html>
        <html lang='en'>
        <head>
        <title>Blog - Azul GUI Framework</title>

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
            <div id='blog-index'>
            <style>
                {css}
            </style>
            <h1>Blog</h1>
            <p>News, updates, and tutorials for the Azul GUI Framework.</p>
            <ul>{post_items}</ul>
            </div>
        </main>
        </div>
        </body>
        </html>"
    )
}
