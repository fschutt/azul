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
        if parts.len() > 0 {
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
    // These are handled at the line level in format_doc_lines
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

/// Format multi-line documentation for HTML output
/// Each line becomes a separate paragraph with proper HTML escaping
/// - Removes ``` code block markers
/// - Converts empty lines to <br/> for proper spacing
pub fn format_doc_lines(doc_lines: &[String]) -> String {
    if doc_lines.is_empty() {
        return String::new();
    }

    let mut in_code_block = false;
    let mut result = Vec::new();

    for line in doc_lines {
        let trimmed = line.trim();

        // Skip code block markers (``` or ```rust, ```python, etc.)
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        // Skip lines inside code blocks
        if in_code_block {
            continue;
        }

        // Convert empty lines to <br/> for proper spacing
        if trimmed.is_empty() {
            result.push("<br/>".to_string());
        } else {
            result.push(format_doc(line));
        }
    }

    result.join("\n")
}

/// Join documentation lines into a single string for display
pub fn join_doc_lines(doc_lines: &[String]) -> String {
    doc_lines.join(" ")
}
