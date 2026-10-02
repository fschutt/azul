//! A note on disk: a small front matter, then the body in Markdown.
//!
//! ```text
//! ---
//! title: Offsite agenda
//! tags: [work, planning]
//! pinned: true
//! created: 2026-09-29T08:10:00Z
//! modified: 2026-09-30T08:10:00Z
//! ---
//!
//! # Goals
//! - Agree on Q4 priorities
//! - [x] Book the room
//! ```
//!
//! The body is the shared rich-text editor's document (azul's
//! `RichTextDoc`), read with `RichTextDoc::from_markdown` and written with
//! `to_markdown` - one canonical form, so a body written by AzNotes reads
//! back to the same document (the round-trip tests live with the editor, in
//! azul-layout's `widgets::rich_text::markdown`). Front matter keys AzNotes
//! does not know (an Obsidian `aliases:`) are kept verbatim.

use azul::widgets::{RichBlockKind, RichTextDoc};

/// The front matter of a note.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Meta {
    pub title: String,
    /// Without the leading `#`, in the order the user added them.
    pub tags: Vec<String>,
    pub pinned: bool,
    /// Seconds since 1970 (UTC).
    pub created: u64,
    pub modified: u64,
    /// Lines of keys AzNotes does not know, written back unchanged.
    pub extra: Vec<String>,
}

// ==== Front matter ====

/// Splits `text` into its front matter lines and the body. A file without
/// front matter is all body.
fn split_front_matter(text: &str) -> (Option<Vec<&str>>, &str) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return (None, text);
    };
    let mut lines = Vec::new();
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        offset += line.len();
        let bare = line.trim_end_matches(['\n', '\r']);
        if bare == "---" || bare == "..." {
            let body = &rest[offset..];
            let body = body
                .strip_prefix("\r\n")
                .or_else(|| body.strip_prefix('\n'))
                .unwrap_or(body);
            return (Some(lines), body);
        }
        lines.push(bare);
    }
    // No closing fence: not front matter after all.
    (None, text)
}

/// A YAML scalar as written by hand or by [`yaml_string`]: double-quoted
/// (with `\"`, `\\`, `\n`, `\t`), single-quoted (with `''`), or bare.
fn yaml_scalar(value: &str) -> String {
    let value = value.trim();
    if let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(other) => out.push(other),
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        return out;
    }
    if let Some(inner) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        return inner.replace("''", "'");
    }
    // A bare scalar ends at a comment.
    match value.find(" #") {
        Some(i) => value[..i].trim_end().to_string(),
        None => value.to_string(),
    }
}

/// The items of a flow list `[a, "b c"]` (or a bare `a, b`).
fn yaml_flow_list(value: &str) -> Vec<String> {
    let value = value.trim();
    let inner = value
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .unwrap_or(value);
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in inner.chars() {
        match quote {
            Some(q) => {
                current.push(c);
                if escaped {
                    escaped = false;
                } else if c == '\\' && q == '"' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                current.push(c);
            }
            None if c == ',' => {
                items.push(yaml_scalar(&current));
                current.clear();
            }
            None => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        items.push(yaml_scalar(&current));
    }
    items.into_iter().filter(|s| !s.is_empty()).collect()
}

/// Whether `s` can be written as a bare YAML scalar and read back as the
/// same string.
fn yaml_bare_ok(s: &str, in_flow: bool) -> bool {
    if s.is_empty() || s.trim() != s {
        return false;
    }
    if matches!(
        s.to_ascii_lowercase().as_str(),
        "true" | "false" | "yes" | "no" | "null" | "~" | "on" | "off"
    ) || s.parse::<f64>().is_ok()
    {
        return false;
    }
    let first = s.chars().next().unwrap_or(' ');
    if "-?:,[]{}#&*!|>'\"%@`".contains(first) {
        return false;
    }
    if s.contains(": ") || s.contains(" #") || s.ends_with(':') || s.contains('\n') || s.contains('\t') {
        return false;
    }
    if in_flow && s.contains([',', '[', ']', '{', '}']) {
        return false;
    }
    true
}

/// `s` as a YAML scalar: bare when that reads back the same, else
/// double-quoted.
fn yaml_string(s: &str, in_flow: bool) -> String {
    if yaml_bare_ok(s, in_flow) {
        return s.to_string();
    }
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A tag as AzNotes keeps it: trimmed, without a leading `#`.
#[must_use]
pub fn clean_tag(tag: &str) -> String {
    tag.trim().trim_start_matches('#').trim().to_string()
}

/// `tags` cleaned, empty ones dropped, duplicates (ignoring case) once.
#[must_use]
pub fn clean_tags<'a>(tags: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in tags {
        let tag = clean_tag(tag);
        if !tag.is_empty() && !out.iter().any(|t| t.eq_ignore_ascii_case(&tag)) {
            out.push(tag);
        }
    }
    out
}

/// The front matter of `lines`.
fn parse_meta(lines: &[&str]) -> Meta {
    let mut meta = Meta::default();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        let Some((key, value)) = line.split_once(':').filter(|_| {
            !line.starts_with(' ') && !line.starts_with('\t') && !line.starts_with('-')
        }) else {
            meta.extra.push(line.to_string());
            continue;
        };
        // A block list under the key: `tags:` then `  - a` lines.
        let mut block_items: Vec<String> = Vec::new();
        let mut block_lines: Vec<String> = Vec::new();
        if value.trim().is_empty() {
            while i < lines.len() {
                let next = lines[i];
                let trimmed = next.trim_start();
                if trimmed.starts_with("- ") || trimmed == "-" {
                    block_items.push(yaml_scalar(trimmed.trim_start_matches('-')));
                    block_lines.push(next.to_string());
                    i += 1;
                } else if next.starts_with(' ') || next.starts_with('\t') {
                    block_lines.push(next.to_string());
                    i += 1;
                } else {
                    break;
                }
            }
        }
        match key.trim() {
            "title" => meta.title = yaml_scalar(value),
            "tags" | "tag" => {
                let items = if value.trim().is_empty() {
                    block_items
                } else {
                    yaml_flow_list(value)
                };
                meta.tags = clean_tags(items.iter().map(String::as_str));
            }
            "pinned" => {
                meta.pinned = matches!(
                    yaml_scalar(value).to_ascii_lowercase().as_str(),
                    "true" | "yes" | "on" | "1"
                );
            }
            "created" => {
                meta.created = azul_storage::time::parse_iso8601(&yaml_scalar(value)).unwrap_or(0);
            }
            "modified" | "updated" => {
                meta.modified = azul_storage::time::parse_iso8601(&yaml_scalar(value)).unwrap_or(0);
            }
            _ => {
                meta.extra.push(line.to_string());
                meta.extra.extend(block_lines);
            }
        }
    }
    meta
}

/// The front matter block of `meta`, fences included.
#[must_use]
pub fn meta_to_front_matter(meta: &Meta) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("title: {}\n", yaml_string(&meta.title, false)));
    let tags: Vec<String> = meta.tags.iter().map(|t| yaml_string(t, true)).collect();
    out.push_str(&format!("tags: [{}]\n", tags.join(", ")));
    out.push_str(&format!("pinned: {}\n", meta.pinned));
    out.push_str(&format!("created: {}\n", azul_storage::time::iso8601(meta.created)));
    out.push_str(&format!("modified: {}\n", azul_storage::time::iso8601(meta.modified)));
    for line in &meta.extra {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("---\n");
    out
}

/// A note file: its front matter and its body. A file without front
/// matter is all body; its title is its first heading (or first line), its
/// dates `file_modified`.
#[must_use]
pub fn parse_note(text: &str, file_modified: u64) -> (Meta, RichTextDoc) {
    let (front, body) = split_front_matter(text);
    let doc = RichTextDoc::from_markdown(body);
    let has_front = front.is_some();
    let mut meta = match front {
        Some(lines) => parse_meta(&lines),
        None => Meta::default(),
    };
    if meta.title.trim().is_empty() && !has_front {
        meta.title = derived_title(&doc);
    }
    if meta.created == 0 {
        meta.created = file_modified;
    }
    if meta.modified == 0 {
        meta.modified = file_modified.max(meta.created);
    }
    (meta, doc)
}

/// The note file of `meta` and `doc`.
#[must_use]
pub fn note_to_file(meta: &Meta, doc: &RichTextDoc) -> String {
    let mut out = meta_to_front_matter(meta);
    let body = doc.to_markdown();
    if !body.as_str().is_empty() {
        out.push('\n');
        out.push_str(body.as_str());
    }
    out
}

/// A title for a note that has none: its first heading, else its first
/// line of text.
#[must_use]
pub fn derived_title(doc: &RichTextDoc) -> String {
    doc.blocks
        .iter()
        .find(|b| matches!(b.kind, RichBlockKind::Heading(_)) && !b.is_empty())
        .or_else(|| doc.blocks.iter().find(|b| b.kind.has_text() && !b.is_empty()))
        .map(|b| {
            let text = b.get_text();
            crate::model::truncate_chars(text.as_str().lines().next().unwrap_or("").trim(), 80)
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_round_trips_and_keeps_unknown_keys() {
        let meta = Meta {
            title: "Offsite: agenda #1".to_string(),
            tags: vec!["work".to_string(), "q4 plans".to_string(), "a,b".to_string()],
            pinned: true,
            created: 1_790_000_000,
            modified: 1_790_086_400,
            extra: vec!["aliases:".to_string(), "  - offsite".to_string()],
        };
        let doc = RichTextDoc::from_markdown("# Goals\n\n- one\n");
        let file = note_to_file(&meta, &doc);
        assert!(file.starts_with("---\ntitle: \"Offsite: agenda #1\"\n"), "{file}");
        assert!(file.contains("tags: [work, q4 plans, \"a,b\"]\n"), "{file}");
        assert!(file.ends_with("---\n\n# Goals\n\n- one\n"), "the body after the fence: {file}");
        let (meta2, doc2) = parse_note(&file, 0);
        assert_eq!(meta2, meta);
        assert_eq!(doc2, doc);
    }

    #[test]
    fn hand_written_front_matter_is_read() {
        let file = "---\ntitle: 'It''s here'\ntags:\n  - '#Ideas'\n  - reading\npinned: yes\ncreated: 2026-09-30T08:10:00Z\n---\nBody text\n";
        let (meta, doc) = parse_note(file, 5);
        assert_eq!(meta.title, "It's here");
        assert_eq!(meta.tags, vec!["Ideas".to_string(), "reading".to_string()]);
        assert!(meta.pinned);
        assert_eq!(meta.created, azul_storage::time::parse_iso8601("2026-09-30T08:10:00Z").unwrap());
        assert_eq!(meta.modified, meta.created, "no modified: the created date");
        assert_eq!(doc.blocks[0].get_text().as_str(), "Body text");
    }

    #[test]
    fn a_plain_markdown_file_takes_its_title_from_its_first_heading() {
        let (meta, doc) = parse_note("Intro line\n\n## Real title\n", 42);
        assert_eq!(meta.title, "Real title");
        assert_eq!((meta.created, meta.modified), (42, 42));
        assert_eq!(doc.blocks.len(), 2);
        let (meta, _) = parse_note("just text\n", 1);
        assert_eq!(meta.title, "just text");
    }

    #[test]
    fn tags_are_cleaned_and_deduplicated_ignoring_case() {
        assert_eq!(
            clean_tags(["#Work", "work", " ideas ", "", "#"]),
            vec!["Work".to_string(), "ideas".to_string()]
        );
    }
}
