/// Convert snake_case to lowerCamelCase
pub fn snake_case_to_lower_camel(snake_str: &str) -> String {
    let mut parts = snake_str.split('_');
    let first = parts.next().unwrap_or("");
    let rest: String = parts
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect();
    format!("{}{}", first, rest)
}

/// Strip function argument types for mem_transmute
///
/// Transforms "mut dom: AzDom, event: AzEventFilter" to "transmute(dom), transmute(event)"
pub fn strip_fn_arg_types_mem_transmute(arg_list: &str) -> String {
    if arg_list.is_empty() {
        return String::new();
    }

    let mut result = String::new();

    for item in arg_list.split(',') {
        let parts: Vec<&str> = item.split(':').collect();
        if !parts.is_empty() {
            let part_a = parts[0].trim();
            // If the part starts with mut, strip it
            let part_a = part_a.strip_prefix("mut ").unwrap_or(part_a);
            result.push_str(&format!("transmute({}), ", part_a));
        }
    }

    // Remove trailing ", " if it exists
    if !result.is_empty() {
        result.truncate(result.len() - 2);
    }

    result
}

/// Strip function argument types
///
/// Transforms "mut dom: AzDom, event: AzEventFilter" to "_: AzDom, _: AzEventFilter"
pub fn strip_fn_arg_types(arg_list: &str) -> String {
    if arg_list.is_empty() {
        return String::new();
    }

    let mut result = String::new();

    for item in arg_list.split(',') {
        let parts: Vec<&str> = item.split(':').collect();
        if parts.len() > 1 {
            let part_b = parts[1].trim();
            result.push_str(&format!("_: {}, ", part_b));
        }
    }

    // Remove trailing ", " if it exists
    if !result.is_empty() {
        result.truncate(result.len() - 2);
    }

    result
}

/// Format a docstring for HTML
pub fn format_doc(docstring: &str) -> String {
    let mut newdoc = docstring.replace('<', "&lt;").replace('>', "&gt;");

    // Remove code block markers entirely (```rust, ```python, ```, etc.)
    // These are handled at the line level in split_doc_paragraphs
    newdoc = newdoc
        .replace("```rust", "")
        .replace("```python", "")
        .replace("```c", "")
        .replace("```cpp", "")
        .replace("```json", "")
        .replace("```", "");

    // Replace inline code marks
    let mut processed = String::new();
    let parts: Vec<&str> = newdoc.split('`').collect();

    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 0 {
            processed.push_str(part);
        } else {
            processed.push_str(&format!("<code>{}</code>", part));
        }
    }

    // Replace bold marks
    let mut final_doc = String::new();
    let parts: Vec<&str> = processed.split("**").collect();

    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 0 {
            final_doc.push_str(part);
        } else {
            final_doc.push_str(&format!("<strong>{}</strong>", part));
        }
    }

    final_doc.replace("\r\n", "<br/>")
}

/// Join documentation lines into a single string for display
pub fn join_doc_lines(doc_lines: &[String]) -> String {
    doc_lines.join(" ")
}

/// Split doc lines into paragraphs on blank lines, dropping ``` fences and
/// the code lines between them.
fn split_doc_paragraphs(doc_lines: &[String]) -> Vec<Vec<String>> {
    let mut in_code_block = false;
    let mut paragraphs = Vec::new();
    let mut current = Vec::new();

    for line in doc_lines {
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }

        if trimmed.is_empty() {
            if !current.is_empty() {
                paragraphs.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line.clone());
        }
    }
    if !current.is_empty() {
        paragraphs.push(current);
    }

    paragraphs
}

fn format_doc_paragraph(lines: &[String]) -> String {
    lines
        .iter()
        .map(|l| format_doc(l))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A one-line `# Heading` / `## Heading` markdown paragraph, split into its
/// text (with the leading `#`s and space stripped).
///
/// Rustdoc headings always sit on their own line surrounded by blank lines,
/// so a paragraph produced by `split_doc_paragraphs` is a heading exactly
/// when it has one line starting with `#`.
fn as_markdown_heading(paragraph: &[String]) -> Option<&str> {
    let [line] = paragraph else { return None };
    let trimmed = line.trim();
    let text = trimmed.trim_start_matches('#');
    if text.len() == trimmed.len() {
        return None; // no leading '#' at all
    }
    Some(text.trim_start())
}

/// Render one paragraph: a `# Heading` line becomes a small heading instead
/// of running its literal `#` into the prose.
fn render_doc_paragraph(css_class: &str, paragraph: &[String]) -> String {
    if let Some(heading) = as_markdown_heading(paragraph) {
        return format!(
            "<p class=\"{css_class} doc doc-heading\"><strong>{}</strong></p>",
            format_doc(heading)
        );
    }
    format!(
        "<p class=\"{css_class} doc\">{}</p>",
        format_doc_paragraph(paragraph)
    )
}

/// Render a doc comment as a self-contained block: a `<div>` that separates
/// it visually from the surrounding page, with only the first paragraph
/// shown - the rest, if any, sits collapsed behind a `<details>` toggle so a
/// long warning doesn't push the whole API page down.
pub fn render_doc_block(css_class: &str, doc_lines: &[String]) -> String {
    let paragraphs = split_doc_paragraphs(doc_lines);
    let Some((first, rest)) = paragraphs.split_first() else {
        return String::new();
    };

    let mut out = format!("<div class=\"{css_class} doc-block\">");
    out.push_str(&render_doc_paragraph(css_class, first));
    if !rest.is_empty() {
        out.push_str("<details class=\"doc-more\"><summary>More</summary>");
        for paragraph in rest {
            out.push_str(&render_doc_paragraph(css_class, paragraph));
        }
        out.push_str("</details>");
    }
    out.push_str("</div>");
    out
}
