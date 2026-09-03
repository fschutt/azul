//! The editable document model — AzWriter's intermediate representation.
//!
//! One tree, four mappings, no format lock-in:
//!
//! ```text
//!   markdown  ──from_markdown──▶            ┌──to_content_dom──▶ Dom (canvas / PDF)
//!   .docx     ──from_docx_bytes─▶  IrDocument
//!   (odf: a future mapper)         ◀─edits──┘──to_markdown────▶ markdown (save)
//! ```
//!
//! Why not use the `docx-parser` model as the IR directly (the obvious
//! shortcut): its `types.rs` is crate-private and Serialize-ONLY — the
//! sanctioned interface is the JSON it emits, and even that repo's own
//! native consumer (its mcp-server) works on the JSON, not the types. So
//! the IR here is OURS (editable, format-neutral), and [`from_docx_wire`]
//! deserializes the parser's JSON into it through a serde SUBSET mirror:
//! the fields we render are declared, everything else is ignored, and an
//! unknown body/run tag degrades to nothing instead of failing the load.
//!
//! Design rules the rest of the editor relies on:
//! - **Block indices are stable across the render**: `to_content_dom`
//!   emits exactly one root child per `IrBlock`, in order, so the engine's
//!   structural changesets (child-index + byte positions) and the
//!   pagination `[block]` / `[block, child]` paths address IR blocks 1:1.
//! - **One node per run**: a paragraph's children correspond 1:1 to its
//!   `runs` — a plain run is a bare text node (the exact shape the editor
//!   rendered before formatting existed), a formatted run is ONE element
//!   (`strong` / `em` / `span`) holding one text node. Formatting never
//!   nests, so child indices keep meaning "run index".

use azul::dom::{Dom, IdOrClass};
use azul::vec::IdOrClassVec;

// ============================================================================
// The model
// ============================================================================

/// A document: an ordered list of blocks. The title is derived (first
/// heading), never stored.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IrDocument {
    pub blocks: Vec<IrBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrBlock {
    Paragraph(IrParagraph),
    List(IrList),
    Table(IrTable),
    /// `<hr>` / `---`.
    Rule,
    /// A hard page break (docx `<w:br w:type="page"/>`). Markdown has no
    /// spelling for it; it survives IR round-trips and the PDF path.
    PageBreak,
}

/// The paragraph STYLE — the thing the ribbon's Styles gallery selects.
/// Quote and code are paragraph styles here (the office-suite model),
/// not container blocks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum IrParaStyle {
    #[default]
    Body,
    /// 1..=6.
    Heading(u8),
    Quote,
    /// A fenced code block; the language tag is kept for the fence.
    CodeBlock {
        language: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IrAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct IrParagraph {
    pub style: IrParaStyle,
    pub align: IrAlign,
    pub runs: Vec<IrRun>,
}

/// A maximal span of text with uniform formatting. Flags are flat — a
/// bold-italic word is ONE run with two flags, never nested runs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IrRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
}

impl IrRun {
    pub fn plain<S: Into<String>>(text: S) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// Same formatting axes (text ignored) — the run-merge predicate.
    pub fn same_format(&self, other: &Self) -> bool {
        self.bold == other.bold
            && self.italic == other.italic
            && self.underline == other.underline
            && self.strike == other.strike
            && self.code == other.code
            && self.link == other.link
    }

    pub fn is_plain(&self) -> bool {
        !self.bold && !self.italic && !self.underline && !self.strike && !self.code
            && self.link.is_none()
    }
}

/// v1 list: one paragraph of runs per item (matches the editor's existing
/// `[block, li]` serializer paths). Nested lists flatten into their parent.
#[derive(Debug, Clone, PartialEq)]
pub struct IrList {
    pub ordered: bool,
    pub items: Vec<IrListItem>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct IrListItem {
    pub runs: Vec<IrRun>,
}

/// v1 table: plain-text cells. The first row is the header when
/// `has_header` (markdown tables always have one; docx tables never do).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IrTable {
    pub has_header: bool,
    pub rows: Vec<Vec<String>>,
}

impl IrDocument {
    /// The display title: the first heading's text, if any.
    #[must_use]
    pub fn derived_title(&self) -> Option<String> {
        self.blocks.iter().find_map(|b| match b {
            IrBlock::Paragraph(p) if matches!(p.style, IrParaStyle::Heading(_)) => {
                Some(flatten_runs(&p.runs))
            }
            _ => None,
        })
    }

    /// Word count over all textual content.
    #[must_use]
    pub fn word_count(&self) -> usize {
        let mut n = 0;
        for b in &self.blocks {
            match b {
                IrBlock::Paragraph(p) => n += flatten_runs(&p.runs).split_whitespace().count(),
                IrBlock::List(l) => {
                    for item in &l.items {
                        n += flatten_runs(&item.runs).split_whitespace().count();
                    }
                }
                IrBlock::Table(t) => {
                    for row in &t.rows {
                        for cell in row {
                            n += cell.split_whitespace().count();
                        }
                    }
                }
                IrBlock::Rule | IrBlock::PageBreak => {}
            }
        }
        n
    }
}

#[must_use]
pub fn flatten_runs(runs: &[IrRun]) -> String {
    runs.iter().map(|r| r.text.as_str()).collect()
}

/// Append `run`, merging into the previous run when the formatting is
/// identical — the invariant that keeps runs MAXIMAL.
pub fn push_run(runs: &mut Vec<IrRun>, run: IrRun) {
    if run.text.is_empty() {
        return;
    }
    if let Some(last) = runs.last_mut() {
        if last.same_format(&run) {
            last.text.push_str(&run.text);
            return;
        }
    }
    runs.push(run);
}

// ============================================================================
// markdown → IR
// ============================================================================

/// Parse markdown into the IR. Supports the same dialect the editor always
/// spoke (CommonMark + strikethrough + tables), now WITHOUT the HTML/XML
/// round-trip: pulldown events build the IR directly.
#[must_use]
pub fn from_markdown(markdown: &str) -> IrDocument {
    use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);

    let mut doc = IrDocument::default();

    // Inline state: nesting DEPTHS (markdown allows **a **b** c**), plus the
    // innermost link.
    let mut bold = 0usize;
    let mut italic = 0usize;
    let mut strike = 0usize;
    let mut link: Vec<String> = Vec::new();

    // Block state.
    let mut para: Option<IrParagraph> = None;
    // (ordered, items) per open list; nested lists flatten into index 0.
    let mut lists: Vec<IrList> = Vec::new();
    let mut in_item = false;
    let mut code_block: Option<(Option<String>, String)> = None;
    let mut table: Option<IrTable> = None;
    let mut cell: Option<String> = None;

    let heading_level = |l: HeadingLevel| -> u8 {
        match l {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
            HeadingLevel::H5 => 5,
            HeadingLevel::H6 => 6,
        }
    };

    // Where does inline text go right now?
    enum Sink<'a> {
        Para(&'a mut IrParagraph),
        Item(&'a mut IrListItem),
        None,
    }

    macro_rules! sink {
        ($para:expr, $lists:expr, $in_item:expr) => {
            if let Some(p) = $para.as_mut() {
                Sink::Para(p)
            } else if $in_item {
                if let Some(item) = $lists
                    .first_mut()
                    .and_then(|l: &mut IrList| l.items.last_mut())
                {
                    Sink::Item(item)
                } else {
                    Sink::None
                }
            } else {
                Sink::None
            }
        };
    }

    let mut quote_depth = 0usize;

    for ev in Parser::new_ext(markdown, opts) {
        match ev {
            Event::Start(Tag::Paragraph) => {
                if !in_item {
                    para = Some(IrParagraph {
                        style: if quote_depth > 0 {
                            IrParaStyle::Quote
                        } else {
                            IrParaStyle::Body
                        },
                        ..IrParagraph::default()
                    });
                }
            }
            Event::End(Tag::Paragraph) => {
                if let Some(p) = para.take() {
                    doc.blocks.push(IrBlock::Paragraph(p));
                }
            }
            Event::Start(Tag::Heading(level, _, _)) => {
                para = Some(IrParagraph {
                    style: IrParaStyle::Heading(heading_level(level)),
                    ..IrParagraph::default()
                });
            }
            Event::End(Tag::Heading(..)) => {
                if let Some(p) = para.take() {
                    doc.blocks.push(IrBlock::Paragraph(p));
                }
            }
            Event::Start(Tag::BlockQuote) => quote_depth += 1,
            Event::End(Tag::BlockQuote) => quote_depth = quote_depth.saturating_sub(1),
            Event::Start(Tag::CodeBlock(kind)) => {
                let language = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.to_string()),
                    _ => None,
                };
                code_block = Some((language, String::new()));
            }
            Event::End(Tag::CodeBlock(_)) => {
                if let Some((language, mut text)) = code_block.take() {
                    while text.ends_with('\n') {
                        text.pop();
                    }
                    doc.blocks.push(IrBlock::Paragraph(IrParagraph {
                        style: IrParaStyle::CodeBlock { language },
                        align: IrAlign::Left,
                        runs: vec![IrRun::plain(text)],
                    }));
                }
            }
            Event::Start(Tag::List(start)) => {
                // Nested lists flatten into the outermost one at End (v1).
                lists.push(IrList {
                    ordered: start.is_some(),
                    items: Vec::new(),
                });
            }
            Event::End(Tag::List(_)) => {
                if let Some(inner) = lists.pop() {
                    if let Some(outer) = lists.first_mut() {
                        outer.items.extend(inner.items);
                    } else if !inner.items.is_empty() {
                        doc.blocks.push(IrBlock::List(inner));
                    }
                }
            }
            Event::Start(Tag::Item) => {
                if let Some(l) = lists.last_mut() {
                    l.items.push(IrListItem::default());
                }
                in_item = true;
            }
            Event::End(Tag::Item) => in_item = false,
            Event::Start(Tag::Emphasis) => italic += 1,
            Event::End(Tag::Emphasis) => italic = italic.saturating_sub(1),
            Event::Start(Tag::Strong) => bold += 1,
            Event::End(Tag::Strong) => bold = bold.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => strike += 1,
            Event::End(Tag::Strikethrough) => strike = strike.saturating_sub(1),
            Event::Start(Tag::Link(_, url, _)) => link.push(url.to_string()),
            Event::End(Tag::Link(..)) => {
                link.pop();
            }
            Event::Start(Tag::Table(_)) => {
                table = Some(IrTable {
                    has_header: true,
                    rows: Vec::new(),
                });
            }
            Event::End(Tag::Table(_)) => {
                if let Some(t) = table.take() {
                    doc.blocks.push(IrBlock::Table(t));
                }
            }
            Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => {
                if let Some(t) = table.as_mut() {
                    t.rows.push(Vec::new());
                }
            }
            Event::End(Tag::TableHead) | Event::End(Tag::TableRow) => {}
            Event::Start(Tag::TableCell) => cell = Some(String::new()),
            Event::End(Tag::TableCell) => {
                if let (Some(c), Some(t)) = (cell.take(), table.as_mut()) {
                    if let Some(row) = t.rows.last_mut() {
                        row.push(c);
                    }
                }
            }
            Event::Rule => doc.blocks.push(IrBlock::Rule),
            Event::Text(t) => {
                if let Some((_, buf)) = code_block.as_mut() {
                    buf.push_str(&t);
                } else if let Some(c) = cell.as_mut() {
                    c.push_str(&t);
                } else {
                    let run = IrRun {
                        text: t.to_string(),
                        bold: bold > 0,
                        italic: italic > 0,
                        strike: strike > 0,
                        underline: false,
                        code: false,
                        link: link.last().cloned(),
                    };
                    match sink!(para, lists, in_item) {
                        Sink::Para(p) => push_run(&mut p.runs, run),
                        Sink::Item(i) => push_run(&mut i.runs, run),
                        Sink::None => {}
                    }
                }
            }
            Event::Code(t) => {
                let run = IrRun {
                    text: t.to_string(),
                    code: true,
                    bold: bold > 0,
                    italic: italic > 0,
                    strike: strike > 0,
                    underline: false,
                    link: link.last().cloned(),
                };
                if let Some(c) = cell.as_mut() {
                    c.push_str(&t);
                } else {
                    match sink!(para, lists, in_item) {
                        Sink::Para(p) => push_run(&mut p.runs, run),
                        Sink::Item(i) => push_run(&mut i.runs, run),
                        Sink::None => {}
                    }
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                let run = IrRun {
                    text: " ".to_string(),
                    bold: bold > 0,
                    italic: italic > 0,
                    strike: strike > 0,
                    underline: false,
                    code: false,
                    link: link.last().cloned(),
                };
                if let Some((_, buf)) = code_block.as_mut() {
                    buf.push('\n');
                } else if let Some(c) = cell.as_mut() {
                    c.push(' ');
                } else {
                    match sink!(para, lists, in_item) {
                        Sink::Para(p) => push_run(&mut p.runs, run),
                        Sink::Item(i) => push_run(&mut i.runs, run),
                        Sink::None => {}
                    }
                }
            }
            Event::Html(_) | Event::FootnoteReference(_) | Event::TaskListMarker(_) => {}
            Event::Start(_) | Event::End(_) => {}
        }
    }

    // The classic office-suite implicit empty paragraph: an empty document
    // still has ONE paragraph, or the caret has nothing to anchor to.
    if doc.blocks.is_empty() {
        doc.blocks
            .push(IrBlock::Paragraph(IrParagraph::default()));
    }
    doc
}

// ============================================================================
// IR → markdown
// ============================================================================

/// Serialize the IR back to markdown. Inline formatting round-trips
/// (`**`/`*`/`~~`/`` ` ``/links); alignment and page breaks do not —
/// markdown has no spelling for them, they live only in the IR.
#[must_use]
pub fn to_markdown(doc: &IrDocument) -> String {
    let mut out = String::new();
    for block in &doc.blocks {
        match block {
            IrBlock::Paragraph(p) => match &p.style {
                IrParaStyle::Heading(level) => {
                    for _ in 0..(*level).clamp(1, 6) {
                        out.push('#');
                    }
                    out.push(' ');
                    out.push_str(runs_to_markdown(&p.runs).trim());
                    out.push_str("\n\n");
                }
                IrParaStyle::Quote => {
                    out.push_str("> ");
                    out.push_str(runs_to_markdown(&p.runs).trim());
                    out.push_str("\n\n");
                }
                IrParaStyle::CodeBlock { language } => {
                    out.push_str("```");
                    if let Some(lang) = language {
                        out.push_str(lang);
                    }
                    out.push('\n');
                    out.push_str(flatten_runs(&p.runs).trim_end());
                    out.push_str("\n```\n\n");
                }
                IrParaStyle::Body => {
                    let text = runs_to_markdown(&p.runs);
                    let text = text.trim();
                    if !text.is_empty() {
                        out.push_str(text);
                    }
                    out.push_str("\n\n");
                }
            },
            IrBlock::List(l) => {
                for (i, item) in l.items.iter().enumerate() {
                    if l.ordered {
                        out.push_str(&format!("{}. ", i + 1));
                    } else {
                        out.push_str("- ");
                    }
                    out.push_str(runs_to_markdown(&item.runs).trim());
                    out.push('\n');
                }
                out.push('\n');
            }
            IrBlock::Table(t) => {
                let cols = t.rows.iter().map(Vec::len).max().unwrap_or(0);
                if cols == 0 {
                    continue;
                }
                let mut rows = t.rows.iter();
                let header: Vec<String> = if t.has_header {
                    rows.next().cloned().unwrap_or_default()
                } else {
                    vec![String::new(); cols]
                };
                let line = |cells: &[String]| {
                    let mut s = String::from("|");
                    for i in 0..cols {
                        s.push(' ');
                        s.push_str(cells.get(i).map_or("", |c| c.as_str()));
                        s.push_str(" |");
                    }
                    s
                };
                out.push_str(&line(&header));
                out.push('\n');
                out.push('|');
                for _ in 0..cols {
                    out.push_str(" --- |");
                }
                out.push('\n');
                for row in rows {
                    out.push_str(&line(row));
                    out.push('\n');
                }
                out.push('\n');
            }
            IrBlock::Rule => out.push_str("---\n\n"),
            IrBlock::PageBreak => {} // no markdown spelling; IR-only
        }
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn runs_to_markdown(runs: &[IrRun]) -> String {
    let mut out = String::new();
    for run in runs {
        let mut text = run.text.clone();
        if run.code {
            text = format!("`{text}`");
        }
        if run.strike {
            text = format!("~~{text}~~");
        }
        if run.italic {
            text = format!("*{text}*");
        }
        if run.bold {
            text = format!("**{text}**");
        }
        if let Some(url) = &run.link {
            text = format!("[{text}]({url})");
        }
        out.push_str(&text);
    }
    out
}

// ============================================================================
// IR → content Dom
// ============================================================================

/// Render the IR into the editor's content tree: one root child per block,
/// one node per run (see the module docs — the changeset and pagination
/// index spaces depend on both).
///
/// The caller wraps this in the page/margin chrome; the root here is the
/// same contenteditable `div` the markdown pipeline always produced.
#[must_use]
pub fn to_content_dom(doc: &IrDocument, doc_css: &str) -> Dom {
    let mut root = Dom::create_div();
    root.set_contenteditable(true);
    // The document stylesheet is AUTHOR css (selector rules like
    // `p { margin-bottom: 11px; }`), so it rides the subtree-stylesheet
    // slot (`Dom.css`, the cascade's input) — `with_css` parses INLINE
    // declarations for the node itself and is the wrong door for a sheet.
    if !doc_css.trim().is_empty() {
        let sheet = azul::css::Css::from_string(doc_css);
        root.css = vec![sheet].into();
    }

    for block in &doc.blocks {
        root.add_child(block_to_dom(block));
    }
    root.fixup_children_estimated();
    root
}

fn block_to_dom(block: &IrBlock) -> Dom {
    match block {
        IrBlock::Paragraph(p) => {
            let mut node = match &p.style {
                IrParaStyle::Heading(1) => Dom::create_h1(),
                IrParaStyle::Heading(2) => Dom::create_h2(),
                IrParaStyle::Heading(3) => Dom::create_h3(),
                // The serializer and a11y stop at h3 for now; deeper levels
                // keep their IR identity and render as h3.
                IrParaStyle::Heading(_) => Dom::create_h3(),
                IrParaStyle::Quote => Dom::create_blockquote(),
                IrParaStyle::CodeBlock { .. } => Dom::create_pre(),
                IrParaStyle::Body => Dom::create_p(),
            };
            for run in &p.runs {
                node.add_child(run_to_dom(run));
            }
            let node = match p.align {
                IrAlign::Left => node,
                IrAlign::Center => node.with_css("text-align: center;"),
                IrAlign::Right => node.with_css("text-align: right;"),
                IrAlign::Justify => node.with_css("text-align: justify;"),
            };
            node
        }
        IrBlock::List(l) => {
            let mut list = if l.ordered {
                Dom::create_ol()
            } else {
                Dom::create_ul()
            };
            for item in &l.items {
                let mut li = Dom::create_li();
                for run in &item.runs {
                    li.add_child(run_to_dom(run));
                }
                list.add_child(li);
            }
            list
        }
        IrBlock::Table(t) => {
            let mut table = Dom::create_table_no_a11y();
            for row in &t.rows {
                let mut tr = Dom::create_tr();
                for cell in row {
                    tr.add_child(Dom::create_td_with_text(cell.as_str()));
                }
                table.add_child(tr);
            }
            table.with_css("border-collapse: collapse;")
        }
        IrBlock::Rule => Dom::create_hr(),
        // Invisible structural marker; the paged pipeline picks it up by
        // class. Renders as nothing in the flow.
        IrBlock::PageBreak => Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from(vec![IdOrClass::Class(
                "mw-pagebreak".into(),
            )]))
            .with_css("height: 0px;"),
    }
}

fn run_to_dom(run: &IrRun) -> Dom {
    if run.is_plain() {
        return Dom::create_text_do_not_use_without_block_level_wrapper(run.text.as_str());
    }

    // ONE element per run: the dominant axis picks the semantic tag, the
    // remaining axes ride on inline css. Never nested — child index == run
    // index is a load-bearing invariant.
    let (mut node, bold_done, italic_done) = if run.bold {
        (Dom::create_strong(), true, false)
    } else if run.italic {
        (
            Dom::create_em_with_text(""), // no bare create_em in the API
            false,
            true,
        )
    } else {
        (Dom::create_span(), false, false)
    };
    // create_em_with_text("") arrives with one empty text child; the run's
    // text replaces the children wholesale either way.
    node.children = Vec::<Dom>::new().into();
    node.add_child(Dom::create_text_do_not_use_without_block_level_wrapper(
        run.text.as_str(),
    ));

    let mut css = String::new();
    if run.bold && !bold_done {
        css.push_str("font-weight: bold;");
    }
    if run.italic && !italic_done {
        css.push_str("font-style: italic;");
    }
    match (run.underline, run.strike) {
        (true, true) => css.push_str("text-decoration: underline line-through;"),
        (true, false) => css.push_str("text-decoration: underline;"),
        (false, true) => css.push_str("text-decoration: line-through;"),
        (false, false) => {}
    }
    if run.code {
        css.push_str("font-family: monospace;");
    }
    if run.link.is_some() {
        // Rendered as a styled span for now; the IR keeps the href.
        css.push_str("color: #0563c1; text-decoration: underline;");
    }
    node.fixup_children_estimated();
    if css.is_empty() {
        node
    } else {
        node.with_css(css.as_str())
    }
}

// ============================================================================
// docx wire → IR (serde subset mirror of docx-parser's JSON)
// ============================================================================

mod wire {
    //! The subset of `docx-parser`'s Serialize-only wire we consume.
    //! Unknown fields are ignored (serde's default); unknown body/run tags
    //! land in the `Unknown` variants and are dropped, so a parser upgrade
    //! degrades a load instead of failing it.
    use serde::Deserialize;

    #[derive(Deserialize, Debug, Default)]
    pub struct Doc {
        #[serde(default)]
        pub body: Vec<Body>,
    }

    #[derive(Deserialize, Debug)]
    #[serde(tag = "type", rename_all = "camelCase")]
    pub enum Body {
        Paragraph(Para),
        Table(Table),
        PageBreak {},
        ColumnBreak,
        SectionBreak {},
        #[serde(other)]
        Unknown,
    }

    #[derive(Deserialize, Debug, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct Para {
        pub alignment: String,
        pub outline_level: Option<u32>,
        pub numbering: Option<Numbering>,
        pub runs: Vec<Run>,
    }

    #[derive(Deserialize, Debug, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct Numbering {
        pub format: String,
    }

    #[derive(Deserialize, Debug)]
    #[serde(tag = "type", rename_all = "camelCase")]
    pub enum Run {
        Text(TextRun),
        Break {},
        #[serde(other)]
        Unknown,
    }

    #[derive(Deserialize, Debug, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct TextRun {
        pub text: String,
        pub bold: bool,
        pub italic: bool,
        pub underline: bool,
        pub strikethrough: bool,
    }

    #[derive(Deserialize, Debug, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct Table {
        pub rows: Vec<TableRow>,
    }

    // The parser's table wire is row → cells → block elements; we only
    // harvest paragraph text out of cells (v1 plain-text tables).
    #[derive(Deserialize, Debug, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct TableRow {
        pub cells: Vec<TableCell>,
    }

    #[derive(Deserialize, Debug, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct TableCell {
        pub content: Vec<CellElement>,
    }

    #[derive(Deserialize, Debug)]
    #[serde(tag = "type", rename_all = "camelCase")]
    pub enum CellElement {
        Paragraph(Para),
        #[serde(other)]
        Unknown,
    }
}

/// Deserialize `docx-parser`'s JSON wire into the IR.
///
/// # Errors
/// Only a structurally un-parseable JSON fails; unknown content degrades.
pub fn from_docx_wire(json: &str) -> Result<IrDocument, String> {
    let doc: wire::Doc = serde_json::from_str(json).map_err(|e| e.to_string())?;

    let mut blocks: Vec<IrBlock> = Vec::new();
    for element in doc.body {
        match element {
            wire::Body::Paragraph(p) => append_wire_paragraph(&mut blocks, p),
            wire::Body::Table(t) => {
                let rows: Vec<Vec<String>> = t
                    .rows
                    .into_iter()
                    .map(|row| {
                        row.cells
                            .into_iter()
                            .map(|cell| {
                                cell.content
                                    .into_iter()
                                    .filter_map(|el| match el {
                                        wire::CellElement::Paragraph(p) => {
                                            Some(wire_runs_text(&p.runs))
                                        }
                                        wire::CellElement::Unknown => None,
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .collect()
                    })
                    .collect();
                if !rows.is_empty() {
                    blocks.push(IrBlock::Table(IrTable {
                        has_header: false,
                        rows,
                    }));
                }
            }
            wire::Body::PageBreak {} => blocks.push(IrBlock::PageBreak),
            wire::Body::ColumnBreak | wire::Body::SectionBreak {} | wire::Body::Unknown => {}
        }
    }
    if blocks.is_empty() {
        blocks.push(IrBlock::Paragraph(IrParagraph::default()));
    }
    Ok(IrDocument { blocks })
}

fn wire_runs_text(runs: &[wire::Run]) -> String {
    let mut s = String::new();
    for run in runs {
        match run {
            wire::Run::Text(t) => s.push_str(&t.text),
            wire::Run::Break {} => s.push(' '),
            wire::Run::Unknown => {}
        }
    }
    s
}

fn append_wire_paragraph(blocks: &mut Vec<IrBlock>, p: wire::Para) {
    let mut runs: Vec<IrRun> = Vec::new();
    for run in &p.runs {
        match run {
            wire::Run::Text(t) => push_run(
                &mut runs,
                IrRun {
                    text: t.text.clone(),
                    bold: t.bold,
                    italic: t.italic,
                    underline: t.underline,
                    strike: t.strikethrough,
                    code: false,
                    link: None,
                },
            ),
            wire::Run::Break {} => push_run(&mut runs, IrRun::plain(" ")),
            wire::Run::Unknown => {}
        }
    }

    let align = match p.alignment.as_str() {
        "center" => IrAlign::Center,
        "right" => IrAlign::Right,
        "both" => IrAlign::Justify,
        _ => IrAlign::Left,
    };

    // A numbered paragraph joins the trailing list (creating it if the
    // previous block is not a compatible list) — how word processors store
    // lists: per-paragraph numbering, grouped at read time.
    if let Some(numbering) = &p.numbering {
        let ordered = numbering.format != "bullet";
        let item = IrListItem { runs };
        match blocks.last_mut() {
            Some(IrBlock::List(l)) if l.ordered == ordered => l.items.push(item),
            _ => blocks.push(IrBlock::List(IrList {
                ordered,
                items: vec![item],
            })),
        }
        return;
    }

    let style = match p.outline_level {
        Some(level) => IrParaStyle::Heading((level.saturating_add(1)).clamp(1, 6) as u8),
        None => IrParaStyle::Body,
    };
    blocks.push(IrBlock::Paragraph(IrParagraph { style, align, runs }));
}

/// Load a `.docx` file into the IR through the `docx-parser` crate
/// (git ref; MIT). The rich JSON wire is the primary path; if our subset
/// mirror ever falls out of step with the parser's schema, the markdown
/// projection is the fallback so the document still opens.
///
/// # Errors
/// Both paths failing (not a docx / unreadable archive).
pub fn from_docx_bytes(data: &[u8]) -> Result<IrDocument, String> {
    match docx_parser::parse_docx_native(data) {
        Ok(json) => match from_docx_wire(&json) {
            Ok(doc) => Ok(doc),
            Err(wire_err) => docx_parser::to_markdown_native(data)
                .map(|md| from_markdown(&md))
                .map_err(|md_err| format!("wire: {wire_err}; markdown: {md_err}")),
        },
        Err(e) => Err(e),
    }
}

// ============================================================================
// Editing: the engine changeset mirror + formatting + text sync
// ============================================================================

use azul::css::DocumentOperation;
use azul::dom::{DocOpMergeNodes, DocOpSplitNode};

/// One inline formatting axis a toolbar toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatAxis {
    Bold,
    Italic,
    Underline,
    Strike,
}

fn floor_char_boundary(s: &str, mut b: usize) -> usize {
    b = b.min(s.len());
    while b > 0 && !s.is_char_boundary(b) {
        b -= 1;
    }
    b
}

/// Merge adjacent same-format runs and drop empty ones — restores the
/// "runs are maximal" invariant after splicing.
pub fn normalize_runs(runs: &mut Vec<IrRun>) {
    let old = core::mem::take(runs);
    for run in old {
        push_run(runs, run);
    }
}

/// Apply an engine [`DocumentOperation`] to the IR — the block-level mirror
/// of the engine's own `apply_document_operation` (same index and position
/// semantics: `resume_path.last()` names the block AFTER a split / the
/// SURVIVOR of a merge; `at.child_index` addresses the block's runs/items;
/// a `text_byte` cuts the addressed run). Returns the inverse operation and
/// ITS resume path (the same pair the engine's `AppliedEdit` carries), or
/// `None` when the op cannot be mirrored (unknown variant, out of range).
pub fn apply_operation(
    doc: &mut IrDocument,
    op: &DocumentOperation,
    resume_path: &[u32],
) -> Option<(DocumentOperation, Vec<u32>)> {
    let resume_last = resume_path.last().copied().unwrap_or(0);
    match op {
        DocumentOperation::SplitNode(split) => {
            let idx = (resume_last as usize).saturating_sub(1);
            if idx >= doc.blocks.len() {
                return None;
            }
            let ci = split.at.child_index as usize;
            let byte = split.at.text_byte.into_option();
            let second = match &mut doc.blocks[idx] {
                IrBlock::Paragraph(p) => {
                    let mut tail: Vec<IrRun> = Vec::new();
                    match byte {
                        Some(b) if ci < p.runs.len() => {
                            let run = &mut p.runs[ci];
                            let cut = floor_char_boundary(&run.text, b as usize);
                            let mut head_run = run.clone();
                            let tail_text = run.text[cut..].to_string();
                            head_run.text = run.text[..cut].to_string();
                            let mut tail_run = run.clone();
                            tail_run.text = tail_text;
                            *run = head_run;
                            tail.push(tail_run);
                            tail.extend(p.runs.drain(ci + 1..));
                        }
                        _ => {
                            tail.extend(p.runs.drain(ci.min(p.runs.len())..));
                        }
                    }
                    let mut tail_p = IrParagraph {
                        style: p.style.clone(),
                        align: p.align,
                        runs: tail,
                    };
                    normalize_runs(&mut p.runs);
                    normalize_runs(&mut tail_p.runs);
                    IrBlock::Paragraph(tail_p)
                }
                IrBlock::List(l) => {
                    let items = l.items.split_off(ci.min(l.items.len()));
                    IrBlock::List(IrList {
                        ordered: l.ordered,
                        items,
                    })
                }
                IrBlock::Table(t) => {
                    let rows = t.rows.split_off(ci.min(t.rows.len()));
                    IrBlock::Table(IrTable {
                        has_header: false,
                        rows,
                    })
                }
                // A childless block splits into itself + an empty clone of
                // its shape — the engine mirror of "clone NodeData, move no
                // children".
                IrBlock::Rule => IrBlock::Rule,
                IrBlock::PageBreak => IrBlock::PageBreak,
            };
            doc.blocks.insert(idx + 1, second);
            Some((
                DocumentOperation::MergeNodes(DocOpMergeNodes {
                    first: split.node,
                    second: split.node,
                    join: split.at,
                }),
                vec![idx as u32],
            ))
        }
        DocumentOperation::MergeNodes(merge) => {
            let first = resume_last as usize;
            if first + 1 >= doc.blocks.len() {
                return None;
            }
            let second = doc.blocks.remove(first + 1);
            let join_is_text = merge.join.text_byte.into_option().is_some();
            match (&mut doc.blocks[first], second) {
                (IrBlock::Paragraph(a), IrBlock::Paragraph(b)) => {
                    if join_is_text {
                        for run in b.runs {
                            push_run(&mut a.runs, run);
                        }
                    } else {
                        a.runs.extend(b.runs);
                    }
                }
                (IrBlock::List(a), IrBlock::List(b)) => a.items.extend(b.items),
                (IrBlock::Paragraph(a), IrBlock::List(b)) => {
                    for item in b.items {
                        for run in item.runs {
                            push_run(&mut a.runs, run);
                        }
                    }
                }
                (IrBlock::List(a), IrBlock::Paragraph(b)) => {
                    a.items.push(IrListItem { runs: b.runs });
                }
                // Structure-only seams (rules, tables meeting paragraphs):
                // the second block's text is best-effort appended; a rule
                // absorbs nothing.
                (IrBlock::Table(a), IrBlock::Table(b)) => a.rows.extend(b.rows),
                (_, _second) => {}
            }
            Some((
                DocumentOperation::SplitNode(DocOpSplitNode {
                    node: merge.first,
                    at: merge.join,
                }),
                vec![(first + 1) as u32],
            ))
        }
        _ => None,
    }
}

/// Which runs (by index) a `[start, end)` byte range of the block's
/// FLATTENED text covers, splitting partially-covered runs first so the
/// covered set is exact. Returns the covered index range.
fn split_runs_at_range(
    runs: &mut Vec<IrRun>,
    start: usize,
    end: usize,
) -> core::ops::Range<usize> {
    // Clamp BOTH bounds to char boundaries of the flattened text first —
    // the same clamped values drive the splits AND the coverage walk, or a
    // mid-code-point bound covers one run too many.
    let flat = flatten_runs(runs);
    let start = floor_char_boundary(&flat, start);
    let end = floor_char_boundary(&flat, end).max(start);

    // Split at `end` FIRST (so `start` offsets stay valid), then at `start`.
    let split_at = |runs: &mut Vec<IrRun>, byte: usize| {
        let mut acc = 0usize;
        for i in 0..runs.len() {
            let len = runs[i].text.len();
            if byte > acc && byte < acc + len {
                let cut = byte - acc;
                if runs[i].text.is_char_boundary(cut) {
                    let mut tail = runs[i].clone();
                    tail.text = runs[i].text[cut..].to_string();
                    runs[i].text.truncate(cut);
                    runs.insert(i + 1, tail);
                }
                return;
            }
            acc += len;
        }
    };
    split_at(runs, end);
    split_at(runs, start);

    // After the splits, `start` and `end` both lie on run boundaries: the
    // covered set is [first run starting at `start` .. first run starting
    // at `end`). An empty range (start == end) is a pure insertion point.
    let mut acc = 0usize;
    let mut first = None;
    let mut last = None;
    for (i, run) in runs.iter().enumerate() {
        if acc == start && first.is_none() {
            first = Some(i);
        }
        if acc >= end {
            last = Some(i);
            break;
        }
        acc += run.text.len();
    }
    if first.is_none() && acc == start {
        first = Some(runs.len());
    }
    let first = first.unwrap_or(runs.len());
    let last = last.unwrap_or(runs.len());
    first..last.max(first)
}

/// Toggle one formatting axis over `[start_byte, end_byte)` of block
/// `block_idx`'s flattened text (a `DocumentSelectionSpan` mapped into the
/// model). Word-processor semantics: if ANY covered part lacks the flag,
/// the whole range gains it; otherwise the whole range loses it. Returns
/// false when the block has no editable runs or the range is empty.
pub fn toggle_format_range(
    doc: &mut IrDocument,
    block_idx: usize,
    start_byte: usize,
    end_byte: usize,
    axis: FormatAxis,
) -> bool {
    if start_byte >= end_byte {
        return false;
    }
    let runs = match doc.blocks.get_mut(block_idx) {
        Some(IrBlock::Paragraph(p)) => &mut p.runs,
        _ => return false,
    };
    let covered = split_runs_at_range(runs, start_byte, end_byte);
    if covered.is_empty() {
        return false;
    }
    let get = |r: &IrRun| match axis {
        FormatAxis::Bold => r.bold,
        FormatAxis::Italic => r.italic,
        FormatAxis::Underline => r.underline,
        FormatAxis::Strike => r.strike,
    };
    let target = !runs[covered.clone()].iter().all(get);
    for run in &mut runs[covered] {
        match axis {
            FormatAxis::Bold => run.bold = target,
            FormatAxis::Italic => run.italic = target,
            FormatAxis::Underline => run.underline = target,
            FormatAxis::Strike => run.strike = target,
        }
    }
    normalize_runs(runs);
    true
}

/// Replace ONE run's text wholesale (the per-run text-sync fast path: the
/// engine's overlay is keyed on the session node, which renders as exactly
/// one run). Formatting stays; empty text drops the run.
pub fn set_run_text(doc: &mut IrDocument, block_idx: usize, run_idx: usize, text: &str) -> bool {
    let runs = match doc.blocks.get_mut(block_idx) {
        Some(IrBlock::Paragraph(p)) => &mut p.runs,
        _ => return false,
    };
    if run_idx >= runs.len() {
        // Typing into an empty paragraph: the engine's session sits on the
        // block itself and no run exists yet — materialize one.
        if runs.is_empty() && run_idx == 0 {
            if text.is_empty() {
                return false;
            }
            runs.push(IrRun::plain(text));
            return true;
        }
        return false;
    }
    if runs[run_idx].text == text {
        return false;
    }
    runs[run_idx].text = text.to_string();
    normalize_runs(runs);
    true
}

/// Set a block's paragraph style (the Styles gallery).
pub fn set_block_style(doc: &mut IrDocument, block_idx: usize, style: IrParaStyle) -> bool {
    match doc.blocks.get_mut(block_idx) {
        Some(IrBlock::Paragraph(p)) => {
            p.style = style;
            true
        }
        _ => false,
    }
}

/// Fold one engine text edit into the model: the block/item addressed by
/// `path` (`[block]` or `[block, item]`) now reads `new_text`. A prefix/
/// suffix diff confines the change to the touched runs, so formatting on
/// untouched text survives typing; the changed middle inherits the format
/// of the run it lands in.
pub fn sync_block_text(doc: &mut IrDocument, path: &[u32], new_text: &str) -> bool {
    let runs: &mut Vec<IrRun> = match (path, doc.blocks.get_mut(path.first().map_or(usize::MAX, |b| *b as usize))) {
        ([_], Some(IrBlock::Paragraph(p))) => &mut p.runs,
        ([_, item], Some(IrBlock::List(l))) => {
            match l.items.get_mut(*item as usize) {
                Some(it) => &mut it.runs,
                None => return false,
            }
        }
        _ => return false,
    };
    let old = flatten_runs(runs);
    if old == new_text {
        return false;
    }

    // Common prefix / suffix on char boundaries; the middle is the edit.
    let mut prefix = old
        .bytes()
        .zip(new_text.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    prefix = floor_char_boundary(&old, prefix.min(new_text.len()));
    let mut suffix = old
        .bytes()
        .rev()
        .zip(new_text.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    suffix = suffix.min(old.len() - prefix).min(new_text.len() - prefix);
    while suffix > 0 && !old.is_char_boundary(old.len() - suffix) {
        suffix -= 1;
    }

    let mid_new = &new_text[prefix..new_text.len() - suffix];
    let covered = split_runs_at_range(runs, prefix, old.len() - suffix);

    // The replacement inherits the format of the FIRST covered run, else of
    // the run just before the cut, else plain.
    let mut template = runs
        .get(covered.start)
        .or_else(|| covered.start.checked_sub(1).and_then(|i| runs.get(i)))
        .cloned()
        .unwrap_or_default();
    template.text = mid_new.to_string();

    runs.splice(covered, (!template.text.is_empty()).then_some(template));
    normalize_runs(runs);
    true
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use azul::dom::NodeType;

    use super::*;

    const SAMPLE: &str = "# Title\n\nHello **bold** and *it* and ~~gone~~ and `code`.\n\n- item one\n- item two\n\n1. first\n2. second\n\n> quoted words\n\n```rust\nlet x = 1;\n```\n\n---\n";

    #[test]
    fn markdown_round_trips_through_the_ir() {
        let ir = from_markdown(SAMPLE);
        let back = to_markdown(&ir);
        assert_eq!(back, SAMPLE, "md -> IR -> md must be the identity here");
    }

    #[test]
    fn formatting_becomes_flat_runs() {
        let ir = from_markdown("a **b** *c* plain\n");
        let IrBlock::Paragraph(p) = &ir.blocks[0] else {
            panic!("paragraph expected")
        };
        assert_eq!(p.runs.len(), 5, "a | b(bold) | space | c(italic) | plain: {:?}", p.runs);
        assert!(p.runs[1].bold && !p.runs[1].italic);
        assert!(p.runs[3].italic && !p.runs[3].bold);
    }

    #[test]
    fn empty_document_keeps_the_caret_anchor_paragraph() {
        let ir = from_markdown("");
        assert_eq!(ir.blocks.len(), 1);
        assert!(matches!(&ir.blocks[0], IrBlock::Paragraph(p) if p.runs.is_empty()));
    }

    #[test]
    fn content_dom_has_one_child_per_block_and_one_node_per_run() {
        let ir = from_markdown(SAMPLE);
        let dom = to_content_dom(&ir, "");
        assert_eq!(
            dom.children.as_ref().len(),
            ir.blocks.len(),
            "block index space must match the render 1:1"
        );
        // Block 1 is the bold/italic paragraph: its child count is its run count.
        let IrBlock::Paragraph(p) = &ir.blocks[1] else {
            panic!()
        };
        let rendered = &dom.children.as_ref()[1];
        assert_eq!(
            rendered.children.as_ref().len(),
            p.runs.len(),
            "child index == run index"
        );
        // Plain run = bare text node; bold run = a single element node.
        assert!(matches!(
            rendered.children.as_ref()[0].root.node_type,
            NodeType::Text(_)
        ));
        assert!(!matches!(
            rendered.children.as_ref()[1].root.node_type,
            NodeType::Text(_)
        ));
    }

    #[test]
    fn docx_wire_subset_deserializes_and_degrades() {
        let json = r#"{
            "section": {},
            "body": [
                {"type":"paragraph","alignment":"left","outlineLevel":0,
                 "runs":[{"type":"text","text":"Heading","bold":false,"italic":false,
                          "underline":false,"strikethrough":false,"fontSize":16.0}]},
                {"type":"paragraph","alignment":"both",
                 "runs":[{"type":"text","text":"Body ","bold":false,"italic":false,
                          "underline":false,"strikethrough":false,"fontSize":11.0},
                         {"type":"text","text":"bold","bold":true,"italic":false,
                          "underline":false,"strikethrough":false,"fontSize":11.0},
                         {"type":"shape","whatever":123}]},
                {"type":"paragraph","alignment":"left",
                 "numbering":{"numId":1,"level":0,"format":"bullet","text":"•"},
                 "runs":[{"type":"text","text":"a bullet","bold":false,"italic":false,
                          "underline":false,"strikethrough":false,"fontSize":11.0}]},
                {"type":"pageBreak"},
                {"type":"someFutureThing","payload":{}}
            ]
        }"#;
        let ir = from_docx_wire(json).expect("subset must deserialize");
        assert_eq!(ir.blocks.len(), 4, "unknown tag dropped: {:?}", ir.blocks);
        assert!(
            matches!(&ir.blocks[0], IrBlock::Paragraph(p) if p.style == IrParaStyle::Heading(1)),
            "outlineLevel 0 is Heading 1"
        );
        let IrBlock::Paragraph(p) = &ir.blocks[1] else {
            panic!()
        };
        assert_eq!(p.align, IrAlign::Justify);
        assert_eq!(p.runs.len(), 2);
        assert!(p.runs[1].bold);
        assert!(matches!(&ir.blocks[2], IrBlock::List(l) if !l.ordered && l.items.len() == 1));
        assert!(matches!(&ir.blocks[3], IrBlock::PageBreak));
    }

    fn split_op(block_after: u32, child: u32, byte: Option<u32>) -> (DocumentOperation, Vec<u32>) {
        use azul::css::NodePosition;
        use azul::dom::{DomId, DomNodeId, NodeHierarchyItemId};
        let at = NodePosition {
            child_index: child,
            text_byte: byte.into(),
        };
        (
            DocumentOperation::SplitNode(DocOpSplitNode {
                node: DomNodeId {
                    dom: DomId { inner: 0 },
                    node: NodeHierarchyItemId::from_raw(0),
                },
                at,
            }),
            vec![block_after],
        )
    }

    #[test]
    fn split_paragraph_at_byte_mirrors_the_engine() {
        let mut ir = from_markdown("Hello **bold** world\n");
        // Split block 0 at run 0 ("Hello "), byte 3: "Hel" | "lo **bold** world".
        let (op, resume) = split_op(1, 0, Some(3));
        let (inverse, inv_resume) =
            apply_operation(&mut ir, &op, &resume).expect("split applies");
        assert_eq!(ir.blocks.len(), 2);
        let IrBlock::Paragraph(a) = &ir.blocks[0] else { panic!() };
        let IrBlock::Paragraph(b) = &ir.blocks[1] else { panic!() };
        assert_eq!(flatten_runs(&a.runs), "Hel");
        assert_eq!(flatten_runs(&b.runs), "lo bold world");
        assert!(b.runs.iter().any(|r| r.bold), "bold survives the split");
        // The inverse merge restores one block with runs intact.
        let (_, _) = apply_operation(&mut ir, &inverse, &inv_resume).expect("merge applies");
        assert_eq!(ir.blocks.len(), 1);
        let IrBlock::Paragraph(p) = &ir.blocks[0] else { panic!() };
        assert_eq!(flatten_runs(&p.runs), "Hello bold world");
        assert!(p.runs.iter().any(|r| r.bold));
    }

    #[test]
    fn toggle_bold_over_a_range_splits_runs_and_merges_back() {
        let mut ir = from_markdown("hello world\n");
        // Bold "world" (bytes 6..11).
        assert!(toggle_format_range(&mut ir, 0, 6, 11, FormatAxis::Bold));
        let IrBlock::Paragraph(p) = &ir.blocks[0] else { panic!() };
        assert_eq!(p.runs.len(), 2, "{:?}", p.runs);
        assert!(!p.runs[0].bold && p.runs[1].bold);
        assert_eq!(p.runs[1].text, "world");
        assert_eq!(to_markdown(&ir), "hello **world**\n");
        // Toggling the same range again clears it and the runs merge back.
        assert!(toggle_format_range(&mut ir, 0, 6, 11, FormatAxis::Bold));
        let IrBlock::Paragraph(p) = &ir.blocks[0] else { panic!() };
        assert_eq!(p.runs.len(), 1);
        assert_eq!(to_markdown(&ir), "hello world\n");
    }

    #[test]
    fn toggle_bold_respects_multibyte_boundaries() {
        let mut ir = from_markdown("gr\u{00fc}\u{00df}e here\n");
        // A range end landing INSIDE the 2-byte '\u{00df}' clamps to a char boundary
        // instead of splitting the code point.
        assert!(toggle_format_range(&mut ir, 0, 0, 5, FormatAxis::Bold));
        let IrBlock::Paragraph(p) = &ir.blocks[0] else { panic!() };
        let flat = flatten_runs(&p.runs);
        assert_eq!(flat, "gr\u{00fc}\u{00df}e here", "no bytes lost: {flat:?}");
        assert!(p.runs[0].bold);
        assert_eq!(
            p.runs[0].text, "gr\u{00fc}",
            "the clamped range covers exactly the runs it touches"
        );
        assert!(!p.runs[1].bold, "{:?}", p.runs);
    }

    #[test]
    fn sync_block_text_preserves_untouched_formatting() {
        let mut ir = from_markdown("one **two** three\n");
        // The user typed "XY" inside "three": new flat text.
        assert!(sync_block_text(&mut ir, &[0], "one two thrXYee"));
        let IrBlock::Paragraph(p) = &ir.blocks[0] else { panic!() };
        assert_eq!(flatten_runs(&p.runs), "one two thrXYee");
        assert!(
            p.runs.iter().any(|r| r.bold && r.text == "two"),
            "bold run untouched by an edit elsewhere: {:?}",
            p.runs
        );
    }

    #[test]
    fn derived_title_and_word_count() {
        let ir = from_markdown(SAMPLE);
        assert_eq!(ir.derived_title().as_deref(), Some("Title"));
        assert!(ir.word_count() >= 14, "count = {}", ir.word_count());
    }
}

#[cfg(test)]
mod docx_end_to_end {
    use super::*;

    /// The oracle is docx-parser's REAL output, not a hand-written wire
    /// sample: a minimal genuine .docx (heading via outlineLvl, bold/italic
    /// runs, a page break) goes through `parse_docx_native` and the subset
    /// mirror must land it in the IR. This is what catches the mirror
    /// drifting from the parser's actual serialization.
    #[test]
    fn a_real_docx_lands_in_the_ir() {
        let bytes = include_bytes!("../testdata/sample.docx");
        let ir = from_docx_bytes(bytes).expect("docx must load");
        assert!(
            matches!(&ir.blocks[0], IrBlock::Paragraph(p)
                if p.style == IrParaStyle::Heading(1)
                && flatten_runs(&p.runs) == "A Real Heading"),
            "block 0: {:?}",
            ir.blocks.first()
        );
        let IrBlock::Paragraph(p) = &ir.blocks[1] else {
            panic!("block 1: {:?}", ir.blocks.get(1))
        };
        assert_eq!(flatten_runs(&p.runs), "Plain then bold italic");
        assert!(p.runs.iter().any(|r| r.bold), "a bold run survives: {:?}", p.runs);
        assert!(p.runs.iter().any(|r| r.italic), "an italic run survives");
        assert!(
            ir.blocks.iter().any(|b| matches!(b, IrBlock::PageBreak)),
            "the page break survives: {:?}",
            ir.blocks
        );
        assert_eq!(ir.derived_title().as_deref(), Some("A Real Heading"));
    }
}


