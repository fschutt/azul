//! Minimal Markdown → [`Dom`] renderer for the `UpdateVersion` dialog's
//! changelog view.
//!
//! Deliberately small, line-based, and lossless where it does not
//! understand something (unknown syntax renders as plain text — a changelog
//! must never DISAPPEAR because it used a construct this renderer lacks):
//!
//! * `#` / `##` / `###` (and deeper → h3) headings
//! * `-` / `*` bullet lists
//! * fenced code blocks (verbatim, monospace)
//! * paragraphs separated by blank lines
//! * inline `**bold**` / `*em*` / `` `code` `` markers are STRIPPED (the
//!   text stays); links `[text](url)` render as `text (url)`.

use azul_core::dom::Dom;

/// Renders `md` into a column of heading / paragraph / list / code nodes.
#[must_use]
pub fn render_markdown(md: &str) -> Dom {
    let mut children: Vec<Dom> = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();
    let mut bullets: Vec<String> = Vec::new();
    let mut code: Option<Vec<String>> = None;

    let flush_paragraph = |children: &mut Vec<Dom>, paragraph: &mut Vec<String>| {
        if !paragraph.is_empty() {
            children.push(Dom::create_p_with_text(strip_inline(&paragraph.join(" "))));
            paragraph.clear();
        }
    };
    let flush_bullets = |children: &mut Vec<Dom>, bullets: &mut Vec<String>| {
        if !bullets.is_empty() {
            let items: Vec<Dom> = bullets
                .drain(..)
                .map(|b| Dom::create_li_with_text(strip_inline(&b)))
                .collect();
            children.push(Dom::create_ul().with_children(items.into()));
        }
    };

    for line in md.lines() {
        // Fenced code blocks swallow EVERYTHING until the closing fence.
        if let Some(block) = code.as_mut() {
            if line.trim_start().starts_with("```") {
                let joined = block.join("\n");
                children.push(Dom::create_pre_with_text(joined));
                code = None;
            } else {
                block.push(line.to_owned());
            }
            continue;
        }
        let trimmed = line.trim_end();
        if trimmed.trim_start().starts_with("```") {
            flush_paragraph(&mut children, &mut paragraph);
            flush_bullets(&mut children, &mut bullets);
            code = Some(Vec::new());
            continue;
        }
        if trimmed.is_empty() {
            flush_paragraph(&mut children, &mut paragraph);
            flush_bullets(&mut children, &mut bullets);
            continue;
        }
        if let Some(rest) = heading(trimmed) {
            flush_paragraph(&mut children, &mut paragraph);
            flush_bullets(&mut children, &mut bullets);
            let (level, text) = rest;
            let text = strip_inline(text);
            children.push(match level {
                1 => Dom::create_h1_with_text(text),
                2 => Dom::create_h2_with_text(text),
                _ => Dom::create_h3_with_text(text),
            });
            continue;
        }
        if let Some(item) = bullet(trimmed) {
            flush_paragraph(&mut children, &mut paragraph);
            bullets.push(item.to_owned());
            continue;
        }
        flush_bullets(&mut children, &mut bullets);
        paragraph.push(trimmed.trim_start().to_owned());
    }
    // An unclosed fence still renders its content (lossless rule).
    if let Some(block) = code {
        children.push(Dom::create_pre_with_text(block.join("\n")));
    }
    flush_paragraph(&mut children, &mut paragraph);
    flush_bullets(&mut children, &mut bullets);

    Dom::create_div().with_children(children.into())
}

/// `### Title` → `(3, "Title")`; None for non-headings.
fn heading(line: &str) -> Option<(usize, &str)> {
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &line[hashes..];
    rest.strip_prefix(' ').map(|text| (hashes, text.trim()))
}

/// `- item` / `* item` → `item`; None otherwise.
fn bullet(line: &str) -> Option<&str> {
    let t = line.trim_start();
    t.strip_prefix("- ")
        .or_else(|| t.strip_prefix("* "))
        .map(str::trim)
}

/// Strips `**`, `*`, `` ` `` markers and rewrites `[text](url)` as
/// `text (url)`. Text is never dropped, only markers.
fn strip_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '*' | '`' => {}
            '[' => {
                // [text](url) → text (url); a bare '[' stays.
                let rest: String = chars.clone().collect();
                if let Some((label, after)) = rest.split_once(']') {
                    if let Some(url_rest) = after.strip_prefix('(') {
                        if let Some((url, _)) = url_rest.split_once(')') {
                            out.push_str(label);
                            out.push_str(" (");
                            out.push_str(url);
                            out.push(')');
                            let consumed = label.len() + 1 + 1 + url.len() + 1;
                            for _ in 0..consumed {
                                let _ = chars.next();
                            }
                            continue;
                        }
                    }
                }
                out.push('[');
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use azul_core::dom::NodeType;

    use super::*;

    fn node_types(dom: &Dom) -> Vec<NodeType> {
        dom.children
            .as_ref()
            .iter()
            .map(|c| c.root.get_node_type().clone())
            .collect()
    }

    #[test]
    fn headings_map_to_their_levels_and_deeper_clamps_to_h3() {
        let dom = render_markdown("# One\n## Two\n### Three\n#### Four");
        let types = node_types(&dom);
        assert_eq!(types.len(), 4, "{types:?}");
        assert!(matches!(types[0], NodeType::H1), "{types:?}");
        assert!(matches!(types[1], NodeType::H2), "{types:?}");
        assert!(matches!(types[2], NodeType::H3), "{types:?}");
        assert!(matches!(types[3], NodeType::H3), "clamp: {types:?}");
    }

    #[test]
    fn bullets_group_into_one_list_and_paragraphs_split_on_blank_lines() {
        let dom = render_markdown("intro line\n\n- a\n- b\n- c\n\noutro");
        let types = node_types(&dom);
        // P, UL, P
        assert_eq!(types.len(), 3, "{types:?}");
        assert!(matches!(types[1], NodeType::Ul), "{types:?}");
        let ul = &dom.children.as_ref()[1];
        assert_eq!(ul.children.as_ref().len(), 3, "three <li>");
    }

    #[test]
    fn fenced_code_survives_verbatim_including_hash_lines() {
        // A '#' INSIDE a fence must NOT become a heading — the fence wins.
        let dom = render_markdown("```\n# not a heading\ncode line\n```");
        let types = node_types(&dom);
        assert_eq!(types.len(), 1, "{types:?}");
        assert!(matches!(types[0], NodeType::Pre), "{types:?}");
    }

    #[test]
    fn inline_markers_strip_but_text_and_links_survive() {
        assert_eq!(
            strip_inline("**bold** and *em* and `code`"),
            "bold and em and code"
        );
        assert_eq!(
            strip_inline("see [the docs](https://x.test/d)"),
            "see the docs (https://x.test/d)"
        );
        // Unclosed constructs stay as literal text — nothing is dropped.
        assert_eq!(strip_inline("a [bare bracket"), "a [bare bracket");
    }
}
