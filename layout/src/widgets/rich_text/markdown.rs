//! A [`RichTextDoc`] as Markdown, both ways.
//!
//! The writer emits ONE canonical form (AzNotes' writer, promoted): a body
//! written here reads back to the same document, and text that merely looks
//! like Markdown (`*x*`, `# not a heading`) is escaped so it stays text -
//! AzWriter's naive writer turned `*x*` into italics on the next load
//! (scripts/DEDUP_EDITORS A1). Underline is `<u>..</u>`, a check item
//! `- [ ]` / `- [x]`, a quoted block `> ` per quote level, a table a GFM
//! pipe table, a page break the comment `<!-- pagebreak -->`. Alignment has
//! no Markdown form and is not written.
//!
//! The reader (feature `rich_text_markdown`, pulldown-cmark with
//! strikethrough, task lists and tables) reads CommonMark from any editor
//! into the flat blocks: nested lists become indent levels, a loose list's
//! paragraphs land in their items, a soft break is a line break.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use azul_css::AzString;

use super::doc::{RichBlock, RichBlockKind, RichRun, RichTableRow, RichTextDoc};
#[cfg(feature = "rich_text_markdown")]
use super::doc::{push_run, RichCheck, RichFormats, RichTableRowVec};

/// The comment a page break is written as.
pub const PAGE_BREAK_COMMENT: &str = "<!-- pagebreak -->";

impl RichTextDoc {
    /// The document as Markdown (the canonical form, see the module docs).
    #[must_use]
    pub fn to_markdown(&self) -> AzString {
        AzString::from(doc_to_markdown(self))
    }

    /// A Markdown text read into a document (any CommonMark editor's
    /// output; see the module docs).
    #[cfg(feature = "rich_text_markdown")]
    #[must_use]
    pub fn create_from_markdown(markdown: AzString) -> Self {
        markdown_to_doc(markdown.as_str())
    }
}

// ==== Markdown -> RichTextDoc ====

/// What receives inline content while parsing.
#[cfg(feature = "rich_text_markdown")]
struct Builder {
    blocks: Vec<RichBlock>,
    /// The block inline content goes into.
    sink: Option<usize>,
    /// The sink is a list item that has no text yet (its paragraph, in a
    /// loose list, lands in it).
    item_fresh: bool,
    /// One entry per open list: ordered or not.
    lists: Vec<bool>,
    quote: u8,
    bold: usize,
    italic: usize,
    strike: usize,
    underline: usize,
    links: Vec<String>,
    /// An open fenced / indented code block: language and text.
    code: Option<(String, String)>,
    /// An open image: its src and the alt text collected so far.
    image: Option<(String, String)>,
    /// An open table: its rows so far (the head row first).
    table: Option<Vec<Vec<String>>>,
    /// The cell being read.
    cell: Option<String>,
}

#[cfg(feature = "rich_text_markdown")]
impl Builder {
    fn new() -> Self {
        Self {
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
            table: None,
            cell: None,
        }
    }

    /// A new block of `kind` at the current quote depth.
    fn push_block(&mut self, kind: RichBlockKind, runs: Vec<RichRun>) -> usize {
        self.blocks
            .push(RichBlock::new(kind, runs).with_quote_depth(self.quote));
        self.blocks.len() - 1
    }

    fn open(&mut self, kind: RichBlockKind) -> usize {
        let at = self.push_block(kind, Vec::new());
        self.sink = Some(at);
        self.item_fresh = false;
        at
    }

    /// The block inline content goes into, opened when there is none (text
    /// after an image, inline HTML outside a paragraph).
    fn sink(&mut self) -> usize {
        match self.sink {
            Some(i) => i,
            None => self.open(RichBlockKind::Paragraph),
        }
    }

    fn run(&self, text: &str, code: bool) -> RichRun {
        let formats = RichFormats {
            bold: self.bold > 0,
            italic: self.italic > 0,
            underline: self.underline > 0,
            strike: self.strike > 0,
            code,
        };
        let run = RichRun::plain(text).with_formats(formats);
        match self.links.last() {
            Some(url) => run.with_link(AzString::from(url.as_str())),
            None => run,
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
        if let Some(cell) = self.cell.as_mut() {
            cell.push_str(text);
            return;
        }
        let run = self.run(text, code);
        let i = self.sink();
        self.item_fresh = false;
        let mut runs = self.blocks[i].runs_vec();
        push_run(&mut runs, run);
        self.blocks[i].set_runs(runs);
    }

    fn depth(&self) -> u8 {
        u8::try_from(self.lists.len().saturating_sub(1)).unwrap_or(u8::MAX)
    }
}

/// Parses a Markdown body into the flat document.
#[cfg(feature = "rich_text_markdown")]
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn markdown_to_doc(markdown: &str) -> RichTextDoc {
    use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_TABLES);

    let mut b = Builder::new();
    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(Tag::Paragraph) => {
                let into_item = b.item_fresh && b.sink.is_some();
                if !into_item {
                    b.open(RichBlockKind::Paragraph);
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
                b.open(RichBlockKind::Heading(level));
            }
            Event::End(Tag::Heading(..)) => b.sink = None,
            Event::Start(Tag::BlockQuote) => {
                b.quote = b.quote.saturating_add(1);
                b.sink = None;
            }
            Event::End(Tag::BlockQuote) => {
                b.quote = b.quote.saturating_sub(1);
                b.sink = None;
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => {
                        lang.split_whitespace().next().unwrap_or("").to_string()
                    }
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
                    let runs = if text.is_empty() {
                        Vec::new()
                    } else {
                        vec![RichRun::plain(&text)]
                    };
                    b.push_block(RichBlockKind::Code(AzString::from(lang)), runs);
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
                    RichBlockKind::Numbered(depth)
                } else {
                    RichBlockKind::Bullet(depth)
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
                    b.blocks[i].kind = RichBlockKind::Check(RichCheck { indent, checked });
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
            Event::Start(Tag::Image(_, url, _)) => {
                b.image = Some((url.to_string(), String::new()));
            }
            Event::End(Tag::Image(..)) => {
                if let Some((src, alt)) = b.image.take() {
                    // An image is a block of its own: the text before it
                    // stays in its paragraph (a paragraph that holds nothing
                    // else goes), the text after it opens a new one.
                    if let Some(i) = b.sink {
                        if i + 1 == b.blocks.len()
                            && b.blocks[i].is_empty()
                            && matches!(b.blocks[i].kind, RichBlockKind::Paragraph)
                        {
                            b.blocks.pop();
                        }
                    }
                    let image = super::doc::RichImage {
                        src: AzString::from(src),
                        alt: AzString::from(alt),
                    };
                    b.push_block(RichBlockKind::Image(image), Vec::new());
                    b.sink = None;
                }
            }
            Event::Start(Tag::Table(_)) => {
                b.table = Some(Vec::new());
                b.sink = None;
            }
            Event::End(Tag::Table(_)) => {
                if let Some(mut rows) = b.table.take() {
                    // A head row of empty cells is the header GFM demands of
                    // a table that has none.
                    let has_header = rows
                        .first()
                        .is_some_and(|head| head.iter().any(|c| !c.trim().is_empty()));
                    if !has_header && !rows.is_empty() {
                        rows.remove(0);
                    }
                    let rows: Vec<RichTableRow> = rows
                        .into_iter()
                        .map(|cells| RichTableRow::create(azul_css::StringVec::from(cells)))
                        .collect();
                    let table = super::doc::RichTable {
                        rows: RichTableRowVec::from_vec(rows),
                        has_header,
                    };
                    b.push_block(RichBlockKind::Table(table), Vec::new());
                }
                b.sink = None;
            }
            Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => {
                if let Some(rows) = b.table.as_mut() {
                    rows.push(Vec::new());
                }
            }
            Event::End(Tag::TableHead) | Event::End(Tag::TableRow) => {}
            Event::Start(Tag::TableCell) => b.cell = Some(String::new()),
            Event::End(Tag::TableCell) => {
                if let (Some(cell), Some(rows)) = (b.cell.take(), b.table.as_mut()) {
                    if let Some(row) = rows.last_mut() {
                        row.push(cell);
                    }
                }
            }
            Event::Text(text) => b.push_text(&text, false),
            Event::Code(text) => b.push_text(&text, true),
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, buf)) = b.code.as_mut() {
                    buf.push('\n');
                } else if let Some((_, alt)) = b.image.as_mut() {
                    alt.push(' ');
                } else if let Some(cell) = b.cell.as_mut() {
                    cell.push(' ');
                } else if b.sink.is_some() {
                    b.push_text("\n", false);
                }
            }
            Event::Rule => {
                b.push_block(RichBlockKind::Rule, Vec::new());
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
                    t if t == PAGE_BREAK_COMMENT => {
                        b.push_block(RichBlockKind::PageBreak, Vec::new());
                        b.sink = None;
                    }
                    _ => {
                        // Anything else stays as its text: nothing is lost,
                        // and the writer writes it back escaped. A line of an
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
        let mut runs = block.runs_vec();
        while let Some(last) = runs.last_mut() {
            let text = last.text.as_str();
            let trimmed = text.trim_end_matches('\n').len();
            if trimmed == text.len() {
                break;
            }
            let kept = text[..trimmed].to_string();
            last.set_text(kept);
            if last.text.as_str().is_empty() {
                runs.pop();
            } else {
                break;
            }
        }
        block.set_runs(runs);
    }
    RichTextDoc::from_blocks(b.blocks)
}

// ==== RichTextDoc -> Markdown ====

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
    const ORDER: [Marker; 4] = [
        Marker::Bold,
        Marker::Italic,
        Marker::Strike,
        Marker::Underline,
    ];

    const fn open(self) -> &'static str {
        match self {
            Marker::Bold => "**",
            Marker::Italic => "*",
            Marker::Strike => "~~",
            Marker::Underline => "<u>",
        }
    }

    const fn close(self) -> &'static str {
        match self {
            Marker::Bold => "**",
            Marker::Italic => "*",
            Marker::Strike => "~~",
            Marker::Underline => "</u>",
        }
    }

    const fn on(self, run: &RichRun) -> bool {
        match self {
            Marker::Bold => run.formats.bold,
            Marker::Italic => run.formats.italic,
            Marker::Strike => run.formats.strike,
            Marker::Underline => run.formats.underline,
        }
    }
}

/// Runs of ONE link (or none) with their emphasis: markers open in a fixed
/// order and close as late as the runs allow, whitespace moved outside a
/// marker so the delimiters stay flanking.
fn emphasis_markdown(runs: &[RichRun]) -> String {
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
        if run.formats.code {
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
pub fn inline_markdown(runs: &[RichRun]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < runs.len() {
        let link = runs[i].link_str().map(ToString::to_string);
        let mut j = i;
        while j < runs.len() && runs[j].link_str() == link.as_deref() {
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

/// A table as a GFM pipe table (a table without a header row gets an empty
/// one: GFM has no headless table).
fn table_markdown(table: &super::doc::RichTable) -> String {
    let columns = table.columns().max(1);
    let line = |row: Option<&RichTableRow>| {
        let mut s = String::from("|");
        for i in 0..columns {
            s.push(' ');
            let cell = row.map_or("", |r| r.cell(i));
            // A pipe would end the cell: GFM's `\|`.
            let escaped = escape_inline(&cell.replace('\n', " ")).replace('|', "\\|");
            s.push_str(&escaped);
            s.push_str(" |");
        }
        s
    };
    let rows = table.rows.as_ref();
    let (head, body) = if table.has_header && !rows.is_empty() {
        (Some(&rows[0]), &rows[1..])
    } else {
        (None, rows)
    };
    let mut out = line(head);
    out.push_str("\n|");
    for _ in 0..columns {
        out.push_str(" --- |");
    }
    for row in body {
        out.push('\n');
        out.push_str(&line(Some(row)));
    }
    out
}

/// `depth` quote markers: `> ` per level (`>` alone on an empty line).
fn quote_marks(depth: u8) -> String {
    "> ".repeat(usize::from(depth))
}

/// `text` with every line inside `depth` quote levels.
fn quoted(text: &str, depth: u8) -> String {
    if depth == 0 {
        return text.to_string();
    }
    let marks = quote_marks(depth);
    text.split('\n')
        .map(|l| {
            if l.is_empty() {
                marks.trim_end().to_string()
            } else {
                format!("{marks}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The document as Markdown: blocks separated by a blank line, list items
/// by a single newline (one tight list), quoted blocks of one quote joined
/// by a quote line (one quote).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn doc_to_markdown(doc: &RichTextDoc) -> String {
    let mut out = String::new();
    // The content column of the last list item at each depth.
    let mut columns: Vec<usize> = Vec::new();
    let mut top_family: Option<char> = None;
    let mut prev: Option<&RichBlock> = None;
    for (index, block) in doc.blocks().iter().enumerate() {
        if prev.is_some_and(|p| p.quote_depth != block.quote_depth) {
            columns.clear();
            top_family = None;
        }
        let text = match &block.kind {
            RichBlockKind::Paragraph => {
                let inline = escape_line_starts(&inline_markdown(block.runs.as_ref()));
                if inline.trim().is_empty() {
                    continue;
                }
                inline
            }
            RichBlockKind::Heading(level) => {
                let inline = inline_markdown(block.runs.as_ref()).replace("\\\n", " ");
                let hashes = "#".repeat(usize::from((*level).clamp(1, 6)));
                if inline.trim().is_empty() {
                    hashes
                } else {
                    format!("{hashes} {}", inline.trim())
                }
            }
            RichBlockKind::Bullet(_) | RichBlockKind::Numbered(_) | RichBlockKind::Check(_) => {
                let depth = usize::from(block.kind.indent());
                columns.truncate(depth);
                let lead = if depth == 0 {
                    0
                } else {
                    columns.last().copied().unwrap_or(0)
                };
                let marker = match &block.kind {
                    RichBlockKind::Numbered(_) => format!("{}. ", doc.number_of(index)),
                    _ => "- ".to_string(),
                };
                columns.push(lead + marker.len());
                let task = match &block.kind {
                    RichBlockKind::Check(c) if c.checked => "[x] ",
                    RichBlockKind::Check(_) => "[ ] ",
                    _ => "",
                };
                let inline = inline_markdown(block.runs.as_ref());
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
            RichBlockKind::Code(lang) => {
                let code = block.flat();
                let fence = code_fence(&code);
                if code.is_empty() {
                    format!("{fence}{lang}\n{fence}")
                } else {
                    format!("{fence}{lang}\n{code}\n{fence}")
                }
            }
            RichBlockKind::Rule => "---".to_string(),
            RichBlockKind::Image(image) => {
                format!(
                    "![{}]({})",
                    escape_inline(image.alt.as_str()),
                    destination(image.src.as_str())
                )
            }
            RichBlockKind::PageBreak => PAGE_BREAK_COMMENT.to_string(),
            RichBlockKind::Table(table) => {
                if table.rows.as_ref().is_empty() {
                    continue;
                }
                table_markdown(table)
            }
        };
        if !block.kind.is_list() {
            columns.clear();
        }
        // The list a top-level item starts or continues: `-` (bullets and
        // check items, one list) or a number. Another one at the top level
        // starts a new list, after a blank line.
        let family = match &block.kind {
            RichBlockKind::Numbered(0) => Some('n'),
            RichBlockKind::Bullet(0) => Some('b'),
            RichBlockKind::Check(c) if c.indent == 0 => Some('b'),
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
            let same_quote = prev.quote_depth == block.quote_depth;
            if prev.kind.is_list() && block.kind.is_list() && !new_list && same_quote {
                out.push('\n');
            } else if prev.quote_depth > 0 && block.quote_depth > 0 {
                let shared = prev.quote_depth.min(block.quote_depth);
                out.push('\n');
                out.push_str(quote_marks(shared).trim_end());
                out.push('\n');
            } else {
                out.push_str("\n\n");
            }
        }
        out.push_str(&quoted(&text, block.quote_depth));
        prev = Some(block);
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

#[cfg(all(test, feature = "rich_text_markdown"))]
mod tests {
    use super::*;
    use crate::widgets::rich_text::doc::{RichFormat, RichImage, RichTable};

    fn round_trip(markdown: &str) {
        let doc = markdown_to_doc(markdown);
        let back = doc_to_markdown(&doc);
        assert_eq!(
            back, markdown,
            "Markdown -> doc -> Markdown is the identity\n{doc:#?}"
        );
        assert_eq!(
            markdown_to_doc(&back),
            doc,
            "and the document reads back the same"
        );
    }

    fn plain(text: &str) -> RichRun {
        RichRun::plain(text)
    }

    fn kinds(doc: &RichTextDoc) -> Vec<RichBlockKind> {
        doc.blocks().iter().map(|b| b.kind.clone()).collect()
    }

    fn check(indent: u8, checked: bool) -> RichBlockKind {
        RichBlockKind::Check(RichCheck { indent, checked })
    }

    const SAMPLE: &str = "# Offsite agenda\n\nBring **laptops**, *chargers* and ~~snacks~~ and `adapters`.\n\n- Agree on Q4 priorities\n- Pick the release date\n  - before the holidays\n\n1. Book the room\n2. Print agendas\n\n- [x] Book the room\n- [ ] Send the dial-in link\n\n> Quoted words\n>\n> over two paragraphs\n\n```rust\nlet x = 1;\nlet y = `x`;\n```\n\n---\n\nSee [the wiki](https://example.org/wiki) and <u>underlined</u> text.\n\n![A diagram](abc/assets/diagram.png)\n";

    #[test]
    fn the_sample_note_round_trips() {
        round_trip(SAMPLE);
    }

    #[test]
    fn the_sample_note_parses_into_the_flat_blocks() {
        let doc = markdown_to_doc(SAMPLE);
        assert_eq!(
            kinds(&doc),
            vec![
                RichBlockKind::Heading(1),
                RichBlockKind::Paragraph,
                RichBlockKind::Bullet(0),
                RichBlockKind::Bullet(0),
                RichBlockKind::Bullet(1),
                RichBlockKind::Numbered(0),
                RichBlockKind::Numbered(0),
                check(0, true),
                check(0, false),
                RichBlockKind::Paragraph,
                RichBlockKind::Paragraph,
                RichBlockKind::Code(AzString::from("rust")),
                RichBlockKind::Rule,
                RichBlockKind::Paragraph,
                RichBlockKind::Image(RichImage {
                    src: AzString::from("abc/assets/diagram.png"),
                    alt: AzString::from("A diagram"),
                }),
            ]
        );
        let depths: Vec<u8> = doc.blocks().iter().map(|b| b.quote_depth).collect();
        assert_eq!(depths[9], 1, "the quote's paragraphs are quoted");
        assert_eq!(depths[10], 1);
        assert_eq!(depths[8], 0);
        assert_eq!(
            doc.blocks()[1].runs_vec(),
            vec![
                plain("Bring "),
                plain("laptops").with_format(RichFormat::Bold),
                plain(", "),
                plain("chargers").with_format(RichFormat::Italic),
                plain(" and "),
                plain("snacks").with_format(RichFormat::Strike),
                plain(" and "),
                plain("adapters").with_format(RichFormat::Code),
                plain("."),
            ]
        );
        assert_eq!(doc.blocks()[11].flat(), "let x = 1;\nlet y = `x`;");
        assert_eq!(
            doc.blocks()[13].runs_vec()[1],
            plain("the wiki").with_link(AzString::from("https://example.org/wiki"))
        );
        assert_eq!(
            doc.blocks()[13].runs_vec()[3],
            plain("underlined").with_format(RichFormat::Underline)
        );
    }

    #[test]
    fn nested_and_overlapping_formats_round_trip() {
        round_trip("**bold *both* bold** plain *it* **b** *i*\n");
        round_trip("***both*** and [**a bold link**](https://a.example)\n");
        let doc = RichTextDoc::from_blocks(vec![RichBlock::new(
            RichBlockKind::Paragraph,
            vec![
                plain("x"),
                plain("y").with_format(RichFormat::Bold),
                plain("z")
                    .with_format(RichFormat::Bold)
                    .with_format(RichFormat::Italic),
            ],
        )]);
        let md = doc_to_markdown(&doc);
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
            let doc = RichTextDoc::from_blocks(vec![RichBlock::paragraph(text)]);
            let md = doc_to_markdown(&doc);
            let back = markdown_to_doc(&md);
            assert_eq!(back.block_count(), 1, "{text:?} -> {md:?}");
            assert_eq!(
                back.blocks()[0].kind,
                RichBlockKind::Paragraph,
                "{text:?} -> {md:?}"
            );
            assert_eq!(back.blocks()[0].quote_depth, 0, "{text:?} -> {md:?}");
            assert_eq!(
                back.blocks()[0].flat(),
                text.trim_start(),
                "{text:?} -> {md:?}"
            );
        }
    }

    #[test]
    fn a_line_break_inside_a_paragraph_and_a_list_item_round_trips() {
        let doc = RichTextDoc::from_blocks(vec![
            RichBlock::paragraph("line one\nline two"),
            RichBlock::text(RichBlockKind::Bullet(0), "item\ncontinued"),
        ]);
        let md = doc_to_markdown(&doc);
        assert_eq!(md, "line one\\\nline two\n\n- item\\\n  continued\n");
        assert_eq!(markdown_to_doc(&md), doc);
    }

    #[test]
    fn a_soft_break_from_another_editor_is_a_line_break() {
        let doc = markdown_to_doc("first\nsecond\n");
        assert_eq!(doc.blocks()[0].flat(), "first\nsecond");
    }

    #[test]
    fn code_spans_and_fences_grow_around_backticks() {
        assert_eq!(code_span("a`b"), "``a`b``");
        assert_eq!(code_span("`x`"), "`` `x` ``");
        let doc = RichTextDoc::from_blocks(vec![RichBlock::text(
            RichBlockKind::Code(AzString::from("")),
            "```\ninner\n```",
        )]);
        let md = doc_to_markdown(&doc);
        assert!(md.starts_with("````\n"), "{md}");
        assert_eq!(markdown_to_doc(&md), doc);
    }

    #[test]
    fn numbered_lists_renumber_and_nest_under_their_marker_width() {
        let doc = RichTextDoc::from_blocks(vec![
            RichBlock::text(RichBlockKind::Numbered(0), "a"),
            RichBlock::text(RichBlockKind::Bullet(1), "nested"),
            RichBlock::text(RichBlockKind::Numbered(0), "b"),
        ]);
        let md = doc_to_markdown(&doc);
        assert_eq!(md, "1. a\n   - nested\n2. b\n");
        assert_eq!(markdown_to_doc(&md), doc);
        assert_eq!(markdown_to_doc("5. five\n6. six\n").block_count(), 2);
    }

    #[test]
    fn empty_paragraphs_are_not_written_and_an_empty_body_is_empty() {
        let doc = RichTextDoc::from_blocks(vec![
            RichBlock::paragraph(""),
            RichBlock::paragraph("x"),
            RichBlock::paragraph(""),
        ]);
        assert_eq!(doc_to_markdown(&doc), "x\n");
        assert_eq!(doc_to_markdown(&RichTextDoc::create()), "");
        assert_eq!(markdown_to_doc(""), RichTextDoc::create());
    }

    #[test]
    fn a_loose_list_from_another_editor_reads_as_items() {
        let doc = markdown_to_doc("- a\n\n- [x] b\n\n1. c\n\n   more about c\n");
        assert_eq!(
            kinds(&doc),
            vec![
                RichBlockKind::Bullet(0),
                check(0, true),
                RichBlockKind::Numbered(0),
                RichBlockKind::Paragraph,
            ]
        );
        assert_eq!(doc.blocks()[3].flat(), "more about c");
        assert_eq!(
            doc_to_markdown(&doc),
            "- a\n- [x] b\n\n1. c\n\nmore about c\n"
        );
    }

    #[test]
    fn nested_quotes_and_a_list_in_a_quote_round_trip_with_their_depths() {
        round_trip("> Hi Ada,\n>\n> > Last year it came early.\n>\n> Bring gloves.\n");
        round_trip("> - bulbs\n> - gloves\n\nAfter the quote.\n");
        let doc = markdown_to_doc("> > deep\n");
        assert_eq!(doc.blocks()[0].quote_depth, 2);
        assert_eq!(doc.blocks()[0].flat(), "deep");
    }

    #[test]
    fn a_table_round_trips_with_and_without_its_header_row() {
        let md = "| Quarter | Goal |\n| --- | --- |\n| Q4 | Ship \\| launch |\n";
        let doc = markdown_to_doc(md);
        let table = doc.blocks()[0].table().expect("a table").clone();
        assert!(table.has_header);
        assert_eq!(table.rows.as_ref().len(), 2);
        assert_eq!(table.rows.as_ref()[1].cell(1), "Ship | launch");
        assert_eq!(doc_to_markdown(&doc), md);
        let mut headless = RichTable::create_empty(1, 2);
        headless.rows = RichTableRowVec::from_vec(vec![RichTableRow::create(
            azul_css::StringVec::from(vec![String::from("a"), String::from("b")]),
        )]);
        let doc = RichTextDoc::from_blocks(vec![RichBlock::new(
            RichBlockKind::Table(headless.clone()),
            vec![],
        )]);
        let md = doc_to_markdown(&doc);
        assert_eq!(md, "|  |  |\n| --- | --- |\n| a | b |\n");
        assert_eq!(markdown_to_doc(&md).blocks()[0].table(), Some(&headless));
    }

    #[test]
    fn a_page_break_round_trips_as_a_comment() {
        let doc = RichTextDoc::from_blocks(vec![
            RichBlock::paragraph("page one"),
            RichBlock::new(RichBlockKind::PageBreak, vec![]),
            RichBlock::paragraph("page two"),
        ]);
        let md = doc_to_markdown(&doc);
        assert_eq!(md, "page one\n\n<!-- pagebreak -->\n\npage two\n");
        assert_eq!(markdown_to_doc(&md), doc);
    }

    #[test]
    fn the_api_forms_wrap_the_same_conversions() {
        let doc = RichTextDoc::create_from_markdown(AzString::from("# T\n\nbody\n"));
        assert_eq!(doc.to_markdown().as_str(), "# T\n\nbody\n");
    }
}
