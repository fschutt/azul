//! A rich-text editor fixture for the editing tests, in the WPT `editing/`
//! data format.
//!
//! WPT's execCommand tests (`editing/data/*.js`, run by `editing/run/*.html`)
//! are rows of `[input HTML with the selection marked, [[command, value]],
//! expected HTML]`: `foo[bar]baz` selects "bar", `foo[]bar` is a caret
//! between the two words. This fixture reads the same markup - a small tag
//! set, the selection marked with `[` and `]` inside text - lays it out as
//! the children of one `contenteditable` host, opens the editing session
//! where the markers stood, and reads the result back as markup of the same
//! kind, so a test is a row of data: input, command, expected output.
//!
//! The tag set: `p div span b strong i em u s blockquote ul ol li br`.
//! Nodes are numbered as the `StyledDom` numbers them - in pre-order, the
//! `<body>` 0 and the host 1 - so a test can name a node by its index.

#![allow(dead_code)] // each test file uses its own part of the fixture

use azul_core::{
    dom::{AttributeType, Dom, DomId, DomNodeId, NodeId, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{SelectionRange, TextBlock, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    text3::cache::{FontStyle, InlineContent, StyleProperties},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The editing host: the `<body>`'s one child.
pub const HOST: usize = 1;

/// Every edge zeroed, so a caret's column is the same in every block.
const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
"#;

/// One node of the parsed markup.
enum Markup {
    Element(String, Vec<Markup>),
    Text(String),
}

/// The markup laid out in an editing host, with the selection's two ends.
pub struct Editor {
    pub lw: LayoutWindow,
    /// Every text node: its index and its text, the markers taken out.
    pub texts: Vec<(usize, String)>,
    /// Where `[` stood: the text node and the byte in its text.
    pub start: Option<(usize, u32)>,
    /// Where `]` stood.
    pub end: Option<(usize, u32)>,
}

/// The text nodes and the markers, collected while the DOM is built.
#[derive(Default)]
struct Marks {
    texts: Vec<(usize, String)>,
    start: Option<(usize, u32)>,
    end: Option<(usize, u32)>,
}

pub fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// Parse `markup` (no attributes; `<br>` needs no end tag).
fn parse(markup: &str) -> Vec<Markup> {
    let mut stack: Vec<(String, Vec<Markup>)> = vec![(String::new(), Vec::new())];
    let mut rest = markup;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("</") {
            let close = after.find('>').expect("an end tag closes");
            let (tag, children) = stack.pop().expect("an element is open");
            assert_eq!(tag.as_str(), &after[..close], "the markup nests");
            stack
                .last_mut()
                .expect("the root is open")
                .1
                .push(Markup::Element(tag, children));
            rest = &after[close + 1..];
        } else if let Some(after) = rest.strip_prefix('<') {
            let close = after.find('>').expect("a start tag closes");
            let tag = after[..close].trim_end_matches('/').trim().to_string();
            rest = &after[close + 1..];
            if tag == "br" {
                stack
                    .last_mut()
                    .expect("the root is open")
                    .1
                    .push(Markup::Element(tag, Vec::new()));
            } else {
                stack.push((tag, Vec::new()));
            }
        } else {
            let next = rest.find('<').unwrap_or(rest.len());
            stack
                .last_mut()
                .expect("the root is open")
                .1
                .push(Markup::Text(rest[..next].to_string()));
            rest = &rest[next..];
        }
    }
    assert_eq!(stack.len(), 1, "the markup closes every element");
    stack.pop().expect("the root").1
}

fn element(tag: &str) -> Dom {
    match tag {
        "p" => Dom::create_p(),
        "div" => Dom::create_div(),
        "span" => Dom::create_span(),
        "b" => Dom::create_b(),
        "strong" => Dom::create_strong(),
        "i" => Dom::create_i(),
        "em" => Dom::create_em(),
        "u" => Dom::create_u(),
        "s" => Dom::create_s(),
        "blockquote" => Dom::create_blockquote(),
        "ul" => Dom::create_ul(),
        "ol" => Dom::create_ol(),
        "li" => Dom::create_li(),
        "br" => Dom::create_br(),
        other => panic!("the editing fixture has no <{other}>"),
    }
}

/// The DOM of `nodes`, numbered in pre-order from `*next`.
fn build(nodes: &[Markup], next: &mut usize, marks: &mut Marks) -> Vec<Dom> {
    let mut out = Vec::new();
    for node in nodes {
        let index = *next;
        *next += 1;
        match node {
            Markup::Text(raw) => {
                let mut text = String::new();
                for ch in raw.chars() {
                    match ch {
                        '[' => marks.start = Some((index, text.len() as u32)),
                        ']' => marks.end = Some((index, text.len() as u32)),
                        _ => text.push(ch),
                    }
                }
                marks.texts.push((index, text.clone()));
                out.push(Dom::create_text_do_not_use_without_block_level_wrapper(
                    text,
                ));
            }
            Markup::Element(tag, children) => {
                let mut dom = element(tag);
                for child in build(children, next, marks) {
                    dom.add_child(child);
                }
                out.push(dom);
            }
        }
    }
    out
}

/// What [`host_dom`] builds: the DOM, every text node (its index and its
/// text, the markers taken out), and where `[` and `]` stood (text node, byte).
pub type HostDom = (Dom, Vec<(usize, String)>, Option<(usize, u32)>, Option<(usize, u32)>);

/// The DOM `markup` describes, inside `body > div[contenteditable]`.
pub fn host_dom(markup: &str) -> HostDom {
    let mut marks = Marks::default();
    let mut next = HOST + 1;
    let children = build(&parse(markup), &mut next, &mut marks);
    let mut host = Dom::create_div().with_contenteditable(true);
    for child in children {
        host.add_child(child);
    }
    (
        Dom::create_body().with_child(host),
        marks.texts,
        marks.start,
        marks.end,
    )
}

/// Lay out `dom` in an 800x600 window.
pub fn lay_out(mut dom: Dom) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled_dom, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    lw
}

/// Lay `dom` out again as the app's NEXT generation (what a `RefreshDom`
/// does), in the window `lw`.
pub fn lay_out_new_generation(lw: &mut LayoutWindow, mut dom: Dom) {
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let ws = lw.current_window_state.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_new_generation(styled_dom, &ws, &rr, &sc, &mut dbg)
        .unwrap();
}

impl Editor {
    /// `markup` laid out in a focused editing host, the session opened where
    /// its markers stand (a caret at `[]`, a selection from `[` to `]`).
    pub fn new(markup: &str) -> Self {
        let (dom, texts, start, end) = host_dom(markup);
        let lw = lay_out(dom);
        let mut editor = Self {
            lw,
            texts,
            start,
            end,
        };
        editor.lw.focus_manager.set_focused_node(Some(dnid(HOST)));
        editor.select_markers();
        editor
    }

    /// The text block node `n`'s text is in.
    pub fn block_of(&self, n: usize) -> TextBlock {
        self.lw
            .text_block_of(dnid(n))
            .unwrap_or_else(|| panic!("node {n} is in a text block"))
    }

    /// The caret at byte `byte` of text node `n`, in its block.
    pub fn caret_at(&self, n: usize, byte: u32) -> (TextBlock, TextCursor) {
        let block = self.block_of(n);
        let caret = self
            .lw
            .caret_at_node_byte(block, NodeId::new(n), byte)
            .unwrap_or_else(|| panic!("text node {n} is laid out in its block"));
        (block, caret)
    }

    /// Open the session at the markers: a caret, a range in one block, or a
    /// selection across blocks (the session at `[`, as a drag leaves it).
    fn select_markers(&mut self) {
        let Some((sn, sb)) = self.start else {
            return;
        };
        let (anchor_block, anchor) = self.caret_at(sn, sb);
        let (focus_block, focus) = match self.end {
            Some((en, eb)) => self.caret_at(en, eb),
            None => (anchor_block, anchor),
        };
        if anchor_block == focus_block {
            self.lw.open_session(
                anchor_block,
                SelectionRange {
                    start: anchor,
                    end: focus,
                },
            );
        } else {
            self.lw.open_session(
                anchor_block,
                SelectionRange {
                    start: anchor,
                    end: anchor,
                },
            );
            assert!(
                self.lw
                    .set_cross_block_selection(anchor_block, anchor, focus_block, focus),
                "premise: the selection spans the blocks"
            );
        }
    }

    /// Node `n`'s content as the edit model holds it (the overlay first,
    /// the DOM second), as markup.
    pub fn markup_of(&self, n: usize) -> String {
        markup_of_runs(
            &self
                .lw
                .get_text_before_textinput(DomId::ROOT_ID, NodeId::new(n)),
        )
    }

    /// Type `text` at the session's caret, as a keystroke lands.
    pub fn type_text(&mut self, text: &str) {
        let _ = self.lw.record_text_input(text);
        let _ = self.lw.apply_text_changeset();
    }
}

/// The formatting a run's style carries: bold, italic, underline, line-through.
pub fn formats_of(style: &StyleProperties) -> [bool; 4] {
    let selector = style.font_stack.first_selector();
    [
        selector.is_some_and(|s| s.weight >= rust_fontconfig::FcWeight::Bold),
        selector.is_some_and(|s| matches!(s.style, FontStyle::Italic | FontStyle::Oblique)),
        style.text_decoration.underline,
        style.text_decoration.strikethrough,
    ]
}

const FORMAT_TAGS: [&str; 4] = ["b", "i", "u", "s"];

/// Runs as markup: each run's text inside the tags of its formatting
/// (`b i u s`, in that order), runs of one formatting joined, a line break
/// as `<br>`.
pub fn markup_of_runs(items: &[InlineContent]) -> String {
    let mut out = String::new();
    let mut open = [false; 4];
    let close_all = |out: &mut String, open: &mut [bool; 4]| {
        for i in (0..4).rev() {
            if open[i] {
                out.push_str("</");
                out.push_str(FORMAT_TAGS[i]);
                out.push('>');
                open[i] = false;
            }
        }
    };
    for item in items {
        match item {
            InlineContent::Text(run) => {
                if run.text.is_empty() {
                    continue;
                }
                let want = formats_of(&run.style);
                if want != open {
                    close_all(&mut out, &mut open);
                    for i in 0..4 {
                        if want[i] {
                            out.push('<');
                            out.push_str(FORMAT_TAGS[i]);
                            out.push('>');
                            open[i] = true;
                        }
                    }
                }
                out.push_str(&run.text);
            }
            InlineContent::LineBreak(_) => {
                close_all(&mut out, &mut open);
                out.push_str("<br>");
            }
            _ => {}
        }
    }
    close_all(&mut out, &mut open);
    out
}

/// A `Dom` subtree as markup: elements by tag name, an `<a>` with its
/// `href`, text as it is.
pub fn markup_of_dom(dom: &Dom) -> String {
    let mut out = String::new();
    write_dom(dom, &mut out);
    out
}

/// The children of a fragment `Dom` (a changeset's payload, whose root is
/// ignored) as markup.
pub fn markup_of_fragment(fragment: &Dom) -> String {
    let mut out = String::new();
    for child in fragment.children.as_ref() {
        write_dom(child, &mut out);
    }
    out
}

fn write_dom(dom: &Dom, out: &mut String) {
    match dom.root.get_node_type() {
        NodeType::Text(t) => out.push_str(t.as_str()),
        NodeType::Br => out.push_str("<br>"),
        node_type => {
            let tag = node_type.get_path().to_string();
            out.push('<');
            out.push_str(&tag);
            if matches!(node_type, NodeType::A) {
                let href = dom.root.attributes().as_ref().iter().find_map(|a| match a {
                    AttributeType::Href(h) => Some(h.as_str().to_string()),
                    _ => None,
                });
                if let Some(href) = href {
                    out.push_str(" href=\"");
                    out.push_str(&href);
                    out.push('"');
                }
            }
            out.push('>');
            for child in dom.children.as_ref() {
                write_dom(child, out);
            }
            out.push_str("</");
            out.push_str(&tag);
            out.push('>');
        }
    }
}
