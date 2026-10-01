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
//! The body is parsed with pulldown-cmark (strikethrough and task lists on)
//! into the flat [`Doc`], and written back by [`body_to_markdown`]. The
//! writer emits one canonical form, so a body written by AzNotes reads back
//! to the same document (the round-trip tests below). Front matter keys
//! AzNotes does not know (an Obsidian `aliases:`) are kept verbatim.

use crate::doc::{push_run, Block, BlockKind, Doc, Run};

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
pub fn parse_note(text: &str, file_modified: u64) -> (Meta, Doc) {
    let (front, body) = split_front_matter(text);
    let doc = markdown_to_doc(body);
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
pub fn note_to_file(meta: &Meta, doc: &Doc) -> String {
    let mut out = meta_to_front_matter(meta);
    let body = body_to_markdown(doc);
    if !body.is_empty() {
        out.push('\n');
        out.push_str(&body);
    }
    out
}

/// A title for a note that has none: its first heading, else its first
/// line of text.
#[must_use]
pub fn derived_title(doc: &Doc) -> String {
    doc.blocks
        .iter()
        .find(|b| matches!(b.kind, BlockKind::Heading(_)) && !b.is_empty())
        .or_else(|| doc.blocks.iter().find(|b| b.kind.has_text() && !b.is_empty()))
        .map(|b| crate::doc::truncate_chars(b.flat().lines().next().unwrap_or("").trim(), 80))
        .unwrap_or_default()
}

// ==== Markdown -> Doc ====

/// What receives inline content while parsing.
struct Builder {
    blocks: Vec<Block>,
    /// The block inline content goes into.
    sink: Option<usize>,
    /// The sink is a list item that has no text yet (its paragraph, in a
    /// loose list, lands in it).
    item_fresh: bool,
    /// One entry per open list: ordered or not.
    lists: Vec<bool>,
    quote: usize,
    bold: usize,
    italic: usize,
    strike: usize,
    underline: usize,
    links: Vec<String>,
    /// An open fenced / indented code block: language and text.
    code: Option<(String, String)>,
    /// An open image: its src and the alt text collected so far.
    image: Option<(String, String)>,
}

impl Builder {
    fn new() -> Self {
        Builder {
            blocks: Vec::new(),
            sink: None,
            item_fresh: false,
            lists: Vec::new(),
            quote: 0,
            bold: 0,
            italic: 0,
            strike: 0,
            underline: 0,
            links: Vec::new(),
            code: None,
            image: None,
        }
    }

    fn open(&mut self, kind: BlockKind) -> usize {
        self.blocks.push(Block::new(kind, Vec::new()));
        self.sink = Some(self.blocks.len() - 1);
        self.item_fresh = false;
        self.blocks.len() - 1
    }

    /// The block inline content goes into, opened when there is none (text
    /// after an image, inline HTML outside a paragraph).
    fn sink(&mut self) -> usize {
        match self.sink {
            Some(i) => i,
            None => {
                let kind = if self.quote > 0 {
                    BlockKind::Quote
                } else {
                    BlockKind::Paragraph
                };
                self.open(kind)
            }
        }
    }

    fn run(&self, text: &str, code: bool) -> Run {
        Run {
            text: text.to_string(),
            bold: self.bold > 0,
            italic: self.italic > 0,
            underline: self.underline > 0,
            strike: self.strike > 0,
            code,
            link: self.links.last().cloned(),
        }
    }

    fn push_text(&mut self, text: &str, code: bool) {
        if let Some((_, alt)) = self.image.as_mut() {
            alt.push_str(text);
            return;
        }
        if let Some((_, buf)) = self.code.as_mut() {
            buf.push_str(text);
            return;
        }
        let run = self.run(text, code);
        let i = self.sink();
        self.item_fresh = false;
        push_run(&mut self.blocks[i].runs, run);
    }

    fn depth(&self) -> u8 {
        u8::try_from(self.lists.len().saturating_sub(1)).unwrap_or(u8::MAX)
    }
}

/// Parses a Markdown body into the flat document.
#[must_use]
pub fn markdown_to_doc(markdown: &str) -> Doc {
    use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut b = Builder::new();
    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(Tag::Paragraph) => {
                let into_item = b.item_fresh && b.sink.is_some();
                if !into_item {
                    let kind = if b.quote > 0 {
                        BlockKind::Quote
                    } else {
                        BlockKind::Paragraph
                    };
                    b.open(kind);
                }
            }
            Event::End(Tag::Paragraph) => b.sink = None,
            Event::Start(Tag::Heading(level, _, _)) => {
                let level = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                b.open(BlockKind::Heading(level));
            }
            Event::End(Tag::Heading(..)) => b.sink = None,
            Event::Start(Tag::BlockQuote) => {
                b.quote += 1;
                b.sink = None;
            }
            Event::End(Tag::BlockQuote) => {
                b.quote = b.quote.saturating_sub(1);
                b.sink = None;
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.split_whitespace().next().unwrap_or("").to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                b.code = Some((lang, String::new()));
                b.sink = None;
            }
            Event::End(Tag::CodeBlock(_)) => {
                if let Some((lang, mut text)) = b.code.take() {
                    if text.ends_with('\n') {
                        text.pop();
                        if text.ends_with('\r') {
                            text.pop();
                        }
                    }
                    b.blocks.push(Block::text(BlockKind::Code { lang }, &text));
                }
                b.sink = None;
            }
            Event::Start(Tag::List(start)) => {
                b.lists.push(start.is_some());
                b.sink = None;
            }
            Event::End(Tag::List(_)) => {
                b.lists.pop();
                b.sink = None;
            }
            Event::Start(Tag::Item) => {
                let depth = b.depth();
                let kind = if b.lists.last().copied().unwrap_or(false) {
                    BlockKind::Numbered(depth)
                } else {
                    BlockKind::Bullet(depth)
                };
                b.open(kind);
                b.item_fresh = true;
            }
            Event::End(Tag::Item) => {
                b.sink = None;
                b.item_fresh = false;
            }
            Event::TaskListMarker(checked) => {
                if let Some(i) = b.sink {
                    let indent = b.blocks[i].kind.indent();
                    b.blocks[i].kind = BlockKind::Check { indent, checked };
                }
            }
            Event::Start(Tag::Emphasis) => b.italic += 1,
            Event::End(Tag::Emphasis) => b.italic = b.italic.saturating_sub(1),
            Event::Start(Tag::Strong) => b.bold += 1,
            Event::End(Tag::Strong) => b.bold = b.bold.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => b.strike += 1,
            Event::End(Tag::Strikethrough) => b.strike = b.strike.saturating_sub(1),
            Event::Start(Tag::Link(_, url, _)) => b.links.push(url.to_string()),
            Event::End(Tag::Link(..)) => {
                b.links.pop();
            }
            Event::Start(Tag::Image(_, url, _)) => b.image = Some((url.to_string(), String::new())),
            Event::End(Tag::Image(..)) => {
                if let Some((src, alt)) = b.image.take() {
                    // An image is a block of its own: the text before it
                    // stays in its paragraph (a paragraph that holds nothing
                    // else goes), the text after it opens a new one.
                    if let Some(i) = b.sink {
                        if i + 1 == b.blocks.len()
                            && b.blocks[i].is_empty()
                            && matches!(b.blocks[i].kind, BlockKind::Paragraph | BlockKind::Quote)
                        {
                            b.blocks.pop();
                        }
                    }
                    b.blocks.push(Block::new(BlockKind::Image { src, alt }, Vec::new()));
                    b.sink = None;
                }
            }
            Event::Text(text) => b.push_text(&text, false),
            Event::Code(text) => b.push_text(&text, true),
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, buf)) = b.code.as_mut() {
                    buf.push('\n');
                } else if let Some((_, alt)) = b.image.as_mut() {
                    alt.push(' ');
                } else if b.sink.is_some() {
                    b.push_text("\n", false);
                }
            }
            Event::Rule => {
                b.blocks.push(Block::new(BlockKind::Rule, Vec::new()));
                b.sink = None;
            }
            Event::Html(html) => {
                let tag = html.trim().to_ascii_lowercase();
                match tag.as_str() {
                    "<u>" | "<ins>" => b.underline += 1,
                    "</u>" | "</ins>" => b.underline = b.underline.saturating_sub(1),
                    "<br>" | "<br/>" | "<br />" => {
                        if b.sink.is_some() {
                            b.push_text("\n", false);
                        }
                    }
                    _ => {
                        // Anything else stays as its text: nothing is lost,
                        // and AzNotes writes it back escaped. A line of an
                        // HTML block keeps its line break.
                        let text = html.trim_end_matches(['\n', '\r']).to_string();
                        if !text.is_empty() {
                            b.push_text(&text, false);
                        }
                        if html.ends_with('\n') && b.sink.is_some() {
                            b.push_text("\n", false);
                        }
                    }
                }
            }
            Event::FootnoteReference(label) => b.push_text(&format!("[^{label}]"), false),
            Event::Start(_) | Event::End(_) => {}
        }
    }
    // Trailing "\n" runs (a hard break at a block's end) are not text.
    for block in &mut b.blocks {
        while let Some(last) = block.runs.last_mut() {
            let trimmed = last.text.trim_end_matches('\n').len();
            if trimmed == last.text.len() {
                break;
            }
            last.text.truncate(trimmed);
            if last.text.is_empty() {
                block.runs.pop();
            } else {
                break;
            }
        }
    }
    Doc::from_blocks(b.blocks)
}

// ==== Doc -> Markdown ====

/// `text` with every character Markdown would read as syntax escaped
/// (inline: emphasis, code, links, HTML, entities; a hard break for `\n`).
fn escape_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '\\' | '*' | '_' | '`' | '[' | ']' | '~' | '<' => {
                out.push('\\');
                out.push(c);
            }
            '&' if chars
                .get(i + 1)
                .is_some_and(|n| n.is_ascii_alphanumeric() || *n == '#') =>
            {
                out.push_str("\\&");
            }
            '\n' => out.push_str("\\\n"),
            c => out.push(c),
        }
    }
    out
}

/// `text` as a code span, fenced with one backtick more than its longest
/// run of backticks.
fn code_span(text: &str) -> String {
    let text = text.replace('\n', " ");
    let mut longest = 0usize;
    let mut current = 0usize;
    for c in text.chars() {
        if c == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    let fence = "`".repeat(longest + 1);
    let pad = text.starts_with('`')
        || text.ends_with('`')
        || (text.starts_with(' ') && text.ends_with(' ') && !text.trim().is_empty());
    if pad {
        format!("{fence} {text} {fence}")
    } else {
        format!("{fence}{text}{fence}")
    }
}

/// A link or image destination: bare, or in `<...>` when it holds spaces
/// or parentheses.
fn destination(url: &str) -> String {
    if url.is_empty() || url.contains([' ', '(', ')', '<', '>']) {
        format!("<{}>", url.replace('<', "%3C").replace('>', "%3E"))
    } else {
        url.to_string()
    }
}

/// The emphasis markers, outermost first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marker {
    Bold,
    Italic,
    Strike,
    Underline,
}

impl Marker {
    const ORDER: [Marker; 4] = [Marker::Bold, Marker::Italic, Marker::Strike, Marker::Underline];

    fn open(self) -> &'static str {
        match self {
            Marker::Bold => "**",
            Marker::Italic => "*",
            Marker::Strike => "~~",
            Marker::Underline => "<u>",
        }
    }

    fn close(self) -> &'static str {
        match self {
            Marker::Bold => "**",
            Marker::Italic => "*",
            Marker::Strike => "~~",
            Marker::Underline => "</u>",
        }
    }

    fn on(self, run: &Run) -> bool {
        match self {
            Marker::Bold => run.bold,
            Marker::Italic => run.italic,
            Marker::Strike => run.strike,
            Marker::Underline => run.underline,
        }
    }
}

/// Runs of ONE link (or none) with their emphasis: markers open in a fixed
/// order and close as late as the runs allow, whitespace moved outside a
/// marker so the delimiters stay flanking.
fn emphasis_markdown(runs: &[Run]) -> String {
    let mut out = String::new();
    let mut open: Vec<Marker> = Vec::new();
    let mut pending = String::new();
    for run in runs {
        let text = run.text.as_str();
        let trimmed_start = text.trim_start();
        let lead = &text[..text.len() - trimmed_start.len()];
        let core = trimmed_start.trim_end();
        let trail = &trimmed_start[core.len()..];
        // Close every marker from the first one this run lacks.
        if let Some(at) = open.iter().position(|m| !m.on(run)) {
            while open.len() > at {
                if let Some(m) = open.pop() {
                    out.push_str(m.close());
                }
            }
        }
        out.push_str(&escape_inline(&pending));
        pending.clear();
        out.push_str(&escape_inline(lead));
        if core.is_empty() {
            continue;
        }
        for m in Marker::ORDER {
            if m.on(run) && !open.contains(&m) {
                out.push_str(m.open());
                open.push(m);
            }
        }
        if run.code {
            out.push_str(&code_span(core));
        } else {
            out.push_str(&escape_inline(core));
        }
        pending.push_str(trail);
    }
    while let Some(m) = open.pop() {
        out.push_str(m.close());
    }
    out.push_str(&escape_inline(&pending));
    out
}

/// A block's runs as inline Markdown.
#[must_use]
pub fn inline_markdown(runs: &[Run]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < runs.len() {
        let link = runs[i].link.clone();
        let mut j = i;
        while j < runs.len() && runs[j].link == link {
            j += 1;
        }
        let inner = emphasis_markdown(&runs[i..j]);
        match link {
            Some(url) if !inner.is_empty() => {
                out.push('[');
                out.push_str(&inner);
                out.push_str("](");
                out.push_str(&destination(&url));
                out.push(')');
            }
            _ => out.push_str(&inner),
        }
        i = j;
    }
    out
}

/// Escapes what the START of a line would read as block syntax: `#`, `>`,
/// `-`, `+`, `=`, `|`, a list number (`1.` / `1)`), leading spaces (an
/// indented code block) - at the block's start and after every hard break.
fn escape_line_starts(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 4);
    for (n, line) in text.split("\\\n").enumerate() {
        if n > 0 {
            out.push_str("\\\n");
        }
        let line = line.trim_start_matches([' ', '\t']);
        let first = line.chars().next();
        match first {
            Some('#' | '>' | '-' | '+' | '=' | '|') => {
                out.push('\\');
                out.push_str(line);
            }
            Some(c) if c.is_ascii_digit() => {
                let digits = line.chars().take_while(char::is_ascii_digit).count();
                let rest = &line[digits..];
                if rest.starts_with(". ") || rest.starts_with(") ") || rest == "." || rest == ")" {
                    out.push_str(&line[..digits]);
                    out.push('\\');
                    out.push_str(rest);
                } else {
                    out.push_str(line);
                }
            }
            _ => out.push_str(line),
        }
    }
    out
}

/// A code block's fence: three backticks, or one more than the longest
/// run of backticks at a line start of its text.
fn code_fence(text: &str) -> String {
    let longest = text
        .lines()
        .map(|l| l.trim_start().chars().take_while(|c| *c == '`').count())
        .max()
        .unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}

/// The document as Markdown: blocks separated by a blank line, list items
/// by a single newline (one tight list), quotes joined by a `>` line (one
/// quote).
#[must_use]
pub fn body_to_markdown(doc: &Doc) -> String {
    let mut out = String::new();
    // The content column of the last list item at each depth.
    let mut columns: Vec<usize> = Vec::new();
    let mut top_family: Option<char> = None;
    let mut prev: Option<&BlockKind> = None;
    for (index, block) in doc.blocks.iter().enumerate() {
        let text = match &block.kind {
            BlockKind::Paragraph => {
                let inline = escape_line_starts(&inline_markdown(&block.runs));
                if inline.trim().is_empty() {
                    continue;
                }
                inline
            }
            BlockKind::Heading(level) => {
                let inline = inline_markdown(&block.runs).replace("\\\n", " ");
                let hashes = "#".repeat(usize::from((*level).clamp(1, 6)));
                if inline.trim().is_empty() {
                    hashes
                } else {
                    format!("{hashes} {}", inline.trim())
                }
            }
            BlockKind::Quote => {
                let inline = escape_line_starts(&inline_markdown(&block.runs));
                inline
                    .split('\n')
                    .map(|l| if l.is_empty() { ">".to_string() } else { format!("> {l}") })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            BlockKind::Bullet(_) | BlockKind::Numbered(_) | BlockKind::Check { .. } => {
                let depth = usize::from(block.kind.indent());
                columns.truncate(depth);
                let lead = if depth == 0 {
                    0
                } else {
                    columns.last().copied().unwrap_or(0)
                };
                let marker = match &block.kind {
                    BlockKind::Numbered(_) => format!("{}. ", doc.number_of(index)),
                    _ => "- ".to_string(),
                };
                columns.push(lead + marker.len());
                let task = match block.kind {
                    BlockKind::Check { checked: true, .. } => "[x] ",
                    BlockKind::Check { checked: false, .. } => "[ ] ",
                    _ => "",
                };
                let inline = inline_markdown(&block.runs);
                let inline = if task.is_empty() {
                    escape_line_starts(&inline)
                } else {
                    inline
                };
                // A continuation line of the item sits at its content column.
                let pad = " ".repeat(lead + marker.len());
                let inline = inline.replace("\\\n", &format!("\\\n{pad}"));
                let line = format!("{}{marker}{task}{inline}", " ".repeat(lead));
                if task.is_empty() {
                    line.trim_end().to_string()
                } else {
                    // `- [ ] ` keeps its space: without it no task marker.
                    line
                }
            }
            BlockKind::Code { lang } => {
                let code = block.flat();
                let fence = code_fence(&code);
                if code.is_empty() {
                    format!("{fence}{lang}\n{fence}")
                } else {
                    format!("{fence}{lang}\n{code}\n{fence}")
                }
            }
            BlockKind::Rule => "---".to_string(),
            BlockKind::Image { src, alt } => {
                format!("![{}]({})", escape_inline(alt), destination(src))
            }
        };
        if !block.kind.is_list() {
            columns.clear();
        }
        // The list a top-level item starts or continues: `-` (bullets and
        // check items, one list) or a number. Another one at the top level
        // starts a new list, after a blank line.
        let family = match block.kind {
            BlockKind::Numbered(0) => Some('n'),
            BlockKind::Bullet(0) | BlockKind::Check { indent: 0, .. } => Some('b'),
            _ => None,
        };
        let new_list = family.is_some() && family != top_family;
        if block.kind.is_list() {
            if family.is_some() {
                top_family = family;
            }
        } else {
            top_family = None;
        }
        if let Some(prev) = prev {
            let joiner = if prev.is_list() && block.kind.is_list() && !new_list {
                "\n"
            } else if *prev == BlockKind::Quote && block.kind == BlockKind::Quote {
                "\n>\n"
            } else {
                "\n\n"
            };
            out.push_str(joiner);
        }
        out.push_str(&text);
        prev = Some(&block.kind);
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Format;

    fn round_trip(markdown: &str) {
        let doc = markdown_to_doc(markdown);
        let back = body_to_markdown(&doc);
        assert_eq!(back, markdown, "Markdown -> Doc -> Markdown is the identity\n{doc:#?}");
        assert_eq!(markdown_to_doc(&back), doc, "and the document reads back the same");
    }

    const SAMPLE: &str = "# Offsite agenda\n\nBring **laptops**, *chargers* and ~~snacks~~ and `adapters`.\n\n- Agree on Q4 priorities\n- Pick the release date\n  - before the holidays\n\n1. Book the room\n2. Print agendas\n\n- [x] Book the room\n- [ ] Send the dial-in link\n\n> Quoted words\n>\n> over two paragraphs\n\n```rust\nlet x = 1;\nlet y = `x`;\n```\n\n---\n\nSee [the wiki](https://example.org/wiki) and <u>underlined</u> text.\n\n![A diagram](abc/assets/diagram.png)\n";

    #[test]
    fn the_sample_note_round_trips() {
        round_trip(SAMPLE);
    }

    #[test]
    fn the_sample_note_parses_into_the_flat_blocks() {
        let doc = markdown_to_doc(SAMPLE);
        let kinds: Vec<BlockKind> = doc.blocks.iter().map(|b| b.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                BlockKind::Heading(1),
                BlockKind::Paragraph,
                BlockKind::Bullet(0),
                BlockKind::Bullet(0),
                BlockKind::Bullet(1),
                BlockKind::Numbered(0),
                BlockKind::Numbered(0),
                BlockKind::Check {
                    indent: 0,
                    checked: true
                },
                BlockKind::Check {
                    indent: 0,
                    checked: false
                },
                BlockKind::Quote,
                BlockKind::Quote,
                BlockKind::Code {
                    lang: "rust".to_string()
                },
                BlockKind::Rule,
                BlockKind::Paragraph,
                BlockKind::Image {
                    src: "abc/assets/diagram.png".to_string(),
                    alt: "A diagram".to_string()
                },
            ]
        );
        assert_eq!(
            doc.blocks[1].runs,
            vec![
                Run::plain("Bring "),
                Run::plain("laptops").with(Format::Bold),
                Run::plain(", "),
                Run::plain("chargers").with(Format::Italic),
                Run::plain(" and "),
                Run::plain("snacks").with(Format::Strike),
                Run::plain(" and "),
                Run::plain("adapters").with(Format::Code),
                Run::plain("."),
            ]
        );
        assert_eq!(doc.blocks[11].flat(), "let x = 1;\nlet y = `x`;");
        assert_eq!(
            doc.blocks[13].runs[1],
            Run::plain("the wiki").linked("https://example.org/wiki")
        );
        assert_eq!(doc.blocks[13].runs[3], Run::plain("underlined").with(Format::Underline));
    }

    #[test]
    fn nested_and_overlapping_formats_round_trip() {
        round_trip("**bold *both* bold** plain *it* **b** *i*\n");
        round_trip("***both*** and [**a bold link**](https://a.example)\n");
        let doc = Doc::from_blocks(vec![Block::new(
            BlockKind::Paragraph,
            vec![
                Run::plain("x"),
                Run::plain("y").with(Format::Bold),
                Run::plain("z").with(Format::Bold).with(Format::Italic),
            ],
        )]);
        let md = body_to_markdown(&doc);
        assert_eq!(markdown_to_doc(&md), doc, "{md}");
    }

    #[test]
    fn text_that_looks_like_markdown_stays_text() {
        let tricky = [
            "# not a heading",
            "- not a list",
            "1. not a number",
            "> not a quote",
            "a *star* and _underscore_ and `tick` and [bracket] and <tag> and \\ and ~tilde~",
            "&amp; stays &amp;, a & b too",
            "=== and | pipes",
            "    four spaces",
        ];
        for text in tricky {
            let doc = Doc::from_blocks(vec![Block::paragraph(text)]);
            let md = body_to_markdown(&doc);
            let back = markdown_to_doc(&md);
            assert_eq!(back.blocks.len(), 1, "{text:?} -> {md:?}");
            assert_eq!(back.blocks[0].kind, BlockKind::Paragraph, "{text:?} -> {md:?}");
            assert_eq!(back.blocks[0].flat(), text.trim_start(), "{text:?} -> {md:?}");
        }
    }

    #[test]
    fn a_line_break_inside_a_paragraph_and_a_list_item_round_trips() {
        let doc = Doc::from_blocks(vec![
            Block::paragraph("line one\nline two"),
            Block::text(BlockKind::Bullet(0), "item\ncontinued"),
        ]);
        let md = body_to_markdown(&doc);
        assert_eq!(md, "line one\\\nline two\n\n- item\\\n  continued\n");
        assert_eq!(markdown_to_doc(&md), doc);
    }

    #[test]
    fn a_soft_break_from_another_editor_is_a_line_break() {
        let doc = markdown_to_doc("first\nsecond\n");
        assert_eq!(doc.blocks[0].flat(), "first\nsecond");
    }

    #[test]
    fn code_spans_and_fences_grow_around_backticks() {
        assert_eq!(code_span("a`b"), "``a`b``");
        assert_eq!(code_span("`x`"), "`` `x` ``");
        let doc = Doc::from_blocks(vec![Block::text(
            BlockKind::Code {
                lang: String::new(),
            },
            "```\ninner\n```",
        )]);
        let md = body_to_markdown(&doc);
        assert!(md.starts_with("````\n"), "{md}");
        assert_eq!(markdown_to_doc(&md), doc);
    }

    #[test]
    fn numbered_lists_renumber_and_nest_under_their_marker_width() {
        let doc = Doc::from_blocks(vec![
            Block::text(BlockKind::Numbered(0), "a"),
            Block::text(BlockKind::Bullet(1), "nested"),
            Block::text(BlockKind::Numbered(0), "b"),
        ]);
        let md = body_to_markdown(&doc);
        assert_eq!(md, "1. a\n   - nested\n2. b\n");
        assert_eq!(markdown_to_doc(&md), doc);
        assert_eq!(markdown_to_doc("5. five\n6. six\n").blocks.len(), 2);
    }

    #[test]
    fn empty_paragraphs_are_not_written_and_an_empty_body_is_empty() {
        let doc = Doc::from_blocks(vec![Block::paragraph(""), Block::paragraph("x"), Block::paragraph("")]);
        assert_eq!(body_to_markdown(&doc), "x\n");
        assert_eq!(body_to_markdown(&Doc::new()), "");
        assert_eq!(markdown_to_doc(""), Doc::new());
    }

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
        let doc = markdown_to_doc("# Goals\n\n- one\n");
        let file = note_to_file(&meta, &doc);
        assert!(file.starts_with("---\ntitle: \"Offsite: agenda #1\"\n"), "{file}");
        assert!(file.contains("tags: [work, q4 plans, \"a,b\"]\n"), "{file}");
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
        assert_eq!(doc.blocks[0].flat(), "Body text");
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
    fn a_loose_list_from_another_editor_reads_as_items() {
        let doc = markdown_to_doc("- a\n\n- [x] b\n\n1. c\n\n   more about c\n");
        let kinds: Vec<BlockKind> = doc.blocks.iter().map(|b| b.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                BlockKind::Bullet(0),
                BlockKind::Check {
                    indent: 0,
                    checked: true
                },
                BlockKind::Numbered(0),
                BlockKind::Paragraph,
            ]
        );
        assert_eq!(doc.blocks[3].flat(), "more about c");
        assert_eq!(body_to_markdown(&doc), "- a\n- [x] b\n\n1. c\n\nmore about c\n");
    }

    #[test]
    fn tags_are_cleaned_and_deduplicated_ignoring_case() {
        assert_eq!(
            clean_tags(["#Work", "work", " ideas ", "", "#"]),
            vec!["Work".to_string(), "ideas".to_string()]
        );
    }
}
