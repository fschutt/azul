/// Convert snake_case to lowerCamelCase
pub fn snake_case_to_lower_camel(snake_str: &str) -> String {
    let mut parts = snake_str.split('_');
    let first = parts.next().unwrap_or("");
    let rest: String = parts.map(|s| {
        let mut c = s.chars();
        match c.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        }
    }).collect();
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
    newdoc = newdoc.replace("```rust", "<code>").replace("```", "</code>");
    
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

/// Render an example description for HTML
pub fn render_example_description(descr: &str, replace: bool) -> String {
    let descr = descr.trim();
    if replace {
        descr.replace("\"", "&quot;")
            .replace("\n", "")
            .replace("\r\n", "")
            .replace("#", "&pound;")
    } else {
        descr.to_string()
    }
}

/// Render example code for HTML
pub fn render_example_code(code: &str, replace: bool) -> String {
    let code = code.replace(">", "&gt;").replace("<", "&lt;");
    if replace {
        code.replace("\"", "&quot;")
            .replace("\n", "<br/>")
            .replace("\r\n", "<br/>")
            .replace(" ", "&nbsp;")
            .trim()
            .to_string()
    } else {
        code.trim().to_string()
    }
}
