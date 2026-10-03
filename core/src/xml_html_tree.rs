//! The tree construction (`azul_core::xml::html`): the HTML Living Standard's 13.2.6, run
//! from the rule tables of `xml_html_rules.rs`.
//!
//! Two ways to build:
//!
//! - [`TreeRules::Html`] (the lenient loaders) builds into a tree of its own, where nodes can
//!   still move - foster parenting puts content misplaced in a table in front of it, the
//!   adoption agency moves a block out of a misnested formatting element - and hands the
//!   finished tree to the [`TreeSink`] at [`TreeBuilder::finish`]. The document's phases
//!   (initial ... after head) are a [`Phase`]; in the body, the insertion mode is the one the
//!   open elements decide (13.2.4.1, kept per open element).
//! - [`TreeRules::Xml`] / [`TreeRules::XmlFolded`] (the strict loaders) stream into the sink
//!   as they go, with the XML conveniences of the body's rules (void elements, implied end
//!   tags, end tags matched within their scope) - the same rows, the HTML-only steps off -
//!   and the simplified misnesting of formatting elements (`<b><p>x</b>`: the `<b>` closes
//!   when the block inside it does).
//!
//! What is simplified (see also the rule tables): comments are not nodes (text across a
//! comment stays one run); `select` and `template` content is body content; `frameset` is
//! not modelled; SVG names are not case-adjusted (`lineargradient`); quirks mode only
//! changes what the standard's tree construction changes (`<table>` in an open `<p>`).

use alloc::{
    borrow::Cow,
    string::{String, ToString},
    vec::Vec,
};

use super::{
    is_html_space,
    rules::{
        self, Content, EndAction, Mode, Scope, Step, TableEnd, TableStart, BREAKOUT,
        FORMATTING, FOSTER_TARGET, HEAD, HEADING, IMPLIED_END, INTEGRATION_POINT, MARKER,
        SPECIAL, VOID,
    },
    tokenizer::{Doctype, TextMode},
    TreeSink, MAX_XML_NESTING_DEPTH,
};

/// Which repairs a [`TreeBuilder`] makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreeRules {
    /// XML as written (the tree loader, `parse_xml_string`): names keep
    /// their case; the HTML conveniences every loader has - void elements,
    /// implied end tags, an end tag closing only within its element's
    /// scope; `<x/>` is an empty element; an element left open is reported
    /// by [`TreeBuilder::finish`].
    Xml,
    /// The same with the element names lower-cased (the document loader,
    /// `parse_xml_to_fast_dom`).
    XmlFolded,
    /// HTML as a browser reads it (the lenient loaders): the HTML Standard's
    /// tree construction (see the module docs). `<x/>` is an empty element
    /// only for the void elements and inside `<svg>` / `<math>`.
    Html,
}

impl TreeRules {
    const fn folds_case(self) -> bool {
        !matches!(self, Self::Xml)
    }

    const fn html(self) -> bool {
        matches!(self, Self::Html)
    }
}

/// `name` lower-cased, borrowed when it already is.
fn lower(name: &str) -> Cow<'_, str> {
    if name.bytes().any(|b| b.is_ascii_uppercase()) {
        Cow::Owned(name.to_ascii_lowercase())
    } else {
        Cow::Borrowed(name)
    }
}

/// `s` without its leading HTML white space, and the white space.
fn split_space(s: &str) -> (&str, &str) {
    let rest = s.trim_start_matches(['\t', '\n', '\u{C}', '\r', ' ']);
    (&s[..s.len() - rest.len()], rest)
}

// ============================================================================
// The tree HTML builds
// ============================================================================

/// What a node of the [`Tree`] is.
#[derive(Debug)]
enum NodeData {
    Document,
    Element {
        name: String,
        attributes: Vec<(String, String)>,
    },
    Text(String),
}

/// A node of the [`Tree`], linked to its parent and siblings.
#[derive(Debug)]
struct Node {
    data: NodeData,
    parent: Option<usize>,
    first_child: Option<usize>,
    last_child: Option<usize>,
    previous: Option<usize>,
    next: Option<usize>,
}

/// The document node of a [`Tree`].
const DOCUMENT: usize = 0;

/// The tree HTML builds before it hands it to the sink: nodes can still move.
#[derive(Debug)]
struct Tree {
    nodes: Vec<Node>,
}

/// Where a node goes: the end of `parent`, or before `before` in it.
#[derive(Debug, Clone, Copy)]
struct Place {
    parent: usize,
    before: Option<usize>,
}

impl Tree {
    fn new() -> Self {
        let mut tree = Self { nodes: Vec::new() };
        let _ = tree.create(NodeData::Document);
        tree
    }

    fn create(&mut self, data: NodeData) -> usize {
        self.nodes.push(Node {
            data,
            parent: None,
            first_child: None,
            last_child: None,
            previous: None,
            next: None,
        });
        self.nodes.len() - 1
    }

    fn parent(&self, node: usize) -> Option<usize> {
        self.nodes[node].parent
    }

    /// Take `node` out of its parent.
    fn detach(&mut self, node: usize) {
        let Some(parent) = self.nodes[node].parent else {
            return;
        };
        let (previous, next) = (self.nodes[node].previous, self.nodes[node].next);
        match previous {
            Some(p) => self.nodes[p].next = next,
            None => self.nodes[parent].first_child = next,
        }
        match next {
            Some(n) => self.nodes[n].previous = previous,
            None => self.nodes[parent].last_child = previous,
        }
        let n = &mut self.nodes[node];
        n.parent = None;
        n.previous = None;
        n.next = None;
    }

    /// `child` (taken out of where it was) as the last child of `parent`.
    fn append(&mut self, parent: usize, child: usize) {
        self.detach(child);
        let last = self.nodes[parent].last_child;
        {
            let c = &mut self.nodes[child];
            c.parent = Some(parent);
            c.previous = last;
            c.next = None;
        }
        match last {
            Some(l) => self.nodes[l].next = Some(child),
            None => self.nodes[parent].first_child = Some(child),
        }
        self.nodes[parent].last_child = Some(child);
    }

    /// `child` (taken out of where it was) before `before`, a child of `parent`.
    fn insert_before(&mut self, parent: usize, child: usize, before: usize) {
        self.detach(child);
        let previous = self.nodes[before].previous;
        {
            let c = &mut self.nodes[child];
            c.parent = Some(parent);
            c.previous = previous;
            c.next = Some(before);
        }
        self.nodes[before].previous = Some(child);
        match previous {
            Some(p) => self.nodes[p].next = Some(child),
            None => self.nodes[parent].first_child = Some(child),
        }
    }

    fn insert(&mut self, place: Place, child: usize) {
        match place.before {
            Some(before) => self.insert_before(place.parent, child, before),
            None => self.append(place.parent, child),
        }
    }

    /// Text at `place`: appended to the text node it follows, if it follows one (13.2.6.1
    /// "insert a character": a text run is one node).
    fn insert_text(&mut self, place: Place, text: &str) {
        if text.is_empty() {
            return;
        }
        let previous = match place.before {
            Some(before) => self.nodes[before].previous,
            None => self.nodes[place.parent].last_child,
        };
        if let Some(p) = previous {
            if let NodeData::Text(existing) = &mut self.nodes[p].data {
                existing.push_str(text);
                return;
            }
        }
        let node = self.create(NodeData::Text(text.to_string()));
        self.insert(place, node);
    }

    /// Every child of `from` to the end of `to`.
    fn move_children(&mut self, from: usize, to: usize) {
        while let Some(child) = self.nodes[from].first_child {
            self.append(to, child);
        }
    }

    /// The attributes of `attributes` that `node` does not have yet, added (13.2.6.4.7: a
    /// second `<html>` / `<body>`).
    fn merge_attributes(&mut self, node: usize, attributes: Vec<(String, String)>) {
        if let NodeData::Element {
            attributes: own, ..
        } = &mut self.nodes[node].data
        {
            for (key, value) in attributes {
                if !own.iter().any(|(k, _)| *k == key) {
                    own.push((key, value));
                }
            }
        }
    }

    /// The tree, depth first, into `sink` (without recursion: a tree is up to
    /// `MAX_XML_NESTING_DEPTH` deep).
    fn replay(&self, sink: &mut dyn TreeSink) {
        let mut current = self.nodes[DOCUMENT].first_child;
        while let Some(node) = current {
            match &self.nodes[node].data {
                NodeData::Element { name, attributes } => {
                    sink.open_element(name, attributes);
                    if let Some(first) = self.nodes[node].first_child {
                        current = Some(first);
                        continue;
                    }
                    sink.close_element();
                }
                NodeData::Text(text) => sink.text(text),
                NodeData::Document => {}
            }
            // The next sibling, else the next sibling of the nearest ancestor that has one
            // (closing the ancestors on the way).
            let mut at = node;
            current = loop {
                if let Some(next) = self.nodes[at].next {
                    break Some(next);
                }
                match self.nodes[at].parent {
                    Some(parent) if parent != DOCUMENT => {
                        sink.close_element();
                        at = parent;
                    }
                    _ => break None,
                }
            };
        }
    }
}

// ============================================================================
// The builder's state
// ============================================================================

/// An element the builder has open (13.2.4.2 "the stack of open elements").
#[derive(Debug, Clone)]
struct OpenElement {
    /// Its name, lower-cased (the rules compare these).
    key: String,
    /// Its node in the [`Tree`] (HTML); a serial number (XML).
    node: usize,
    /// XML: its end tag came while a block inside it was open (`<b><p>x</b>`): it closes
    /// as soon as it is the current node again.
    pending_close: bool,
    /// HTML: the insertion mode inside it (13.2.4.1: the nearest element of
    /// [`rules::MODES`] at or below it).
    mode: Mode,
    /// HTML: an SVG / MathML element (13.2.6.5).
    foreign: bool,
}

/// An entry of the list of active formatting elements (13.2.4.3; HTML only).
#[derive(Debug, Clone)]
enum Formatting {
    /// A cell (or caption ...) opened: nothing before it is reopened in it.
    Marker,
    /// A formatting element: reopened, with these attributes, wherever text
    /// or inline content comes while it is not open.
    Element {
        node: usize,
        key: String,
        attributes: Vec<(String, String)>,
    },
}

/// The document's phases before its body (13.2.6.4.1 - 13.2.6.4.6); in the body the open
/// elements decide the insertion mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Initial,
    BeforeHtml,
    BeforeHead,
    InHead,
    AfterHead,
    InBody,
}

/// A start tag being processed (it may be reprocessed in another phase or mode).
#[derive(Debug)]
struct Tag {
    /// The name as the element gets it (XML: as written, or lower-cased).
    name: String,
    /// The name lower-cased.
    key: String,
    attributes: Vec<(String, String)>,
    self_closing: bool,
}

/// The tree construction (see the module docs): fed the tokens of a
/// document in order (`start_tag`, `end_tag`, `text` ...), it opens and
/// closes the elements of the tree on a [`TreeSink`], balanced.
#[derive(Debug)]
pub struct TreeBuilder {
    rules: TreeRules,
    stack: Vec<OpenElement>,
    active: Vec<Formatting>,
    next_id: usize,
    /// The next text's first line feed is dropped (after `<pre>`).
    skip_newline: bool,
    /// XML: the text since the last element opened or closed: a browser's text
    /// node is one run, also across a comment or a tag it ignores
    /// (`a<!-- x -->b`, `a</font>b` with no `<font>` open).
    pending_text: String,
    /// How many `<p>` / `<li>` `<dd>` `<dt>` are open: a block start looks
    /// for one only when there is one (ten thousand nested `<div>`s are not
    /// searched ten thousand times).
    open_paragraphs: usize,
    open_list_items: usize,
    /// HTML: the tree being built.
    tree: Tree,
    /// HTML: the document's phase.
    phase: Phase,
    /// HTML: the document is in quirks mode (no doctype, or a legacy one).
    quirks: bool,
    /// HTML: foster parenting is enabled (13.2.6.4.9, anything else in a table).
    foster: bool,
    /// HTML: the head element pointer.
    head: Option<usize>,
    /// HTML: the form element pointer.
    form: Option<usize>,
}

impl TreeBuilder {
    /// A builder that applies `rules`.
    #[must_use]
    pub fn new(rules: TreeRules) -> Self {
        Self {
            rules,
            stack: Vec::new(),
            active: Vec::new(),
            next_id: 0,
            skip_newline: false,
            pending_text: String::new(),
            open_paragraphs: 0,
            open_list_items: 0,
            tree: Tree::new(),
            phase: Phase::Initial,
            quirks: false,
            foster: false,
            head: None,
            form: None,
        }
    }

    /// How many elements are open.
    #[must_use]
    pub fn open_elements(&self) -> usize {
        self.stack.len()
    }

    /// Whether the adjusted current node is an SVG / MathML element (HTML): a CDATA
    /// section is one there ([`super::HtmlTokenizer::set_cdata_allowed`]).
    #[must_use]
    pub fn in_foreign_content(&self) -> bool {
        self.rules.html() && self.stack.last().is_some_and(|e| e.foreign)
    }

    /// How the tokenizer reads the content of the start tag `name` that comes next (13.2.6.2;
    /// [`super::HtmlTokenizer::set_text_mode`]): an HTML `<style>` is raw text, an SVG one
    /// markup.
    #[must_use]
    pub fn text_mode_for(&self, name: &str) -> TextMode {
        if !self.rules.html() || self.foreign_for_start() {
            return TextMode::Data;
        }
        let key = lower(name);
        match rules::element(&key) {
            Some(e) => match e.content {
                Content::Markup => TextMode::Data,
                Content::RcData => TextMode::RcData(e.name),
                Content::RawText => TextMode::RawText(e.name),
                Content::PlainText => TextMode::PlainText,
            },
            None => TextMode::Data,
        }
    }

    // ---- the stack ----

    fn html(&self) -> bool {
        self.rules.html()
    }

    fn current(&self) -> Option<&str> {
        self.stack.last().map(|e| e.key.as_str())
    }

    /// HTML: the insertion mode in the body.
    fn mode(&self) -> Mode {
        self.stack.last().map_or(Mode::Body, |e| e.mode)
    }

    /// HTML: start tags and text are processed as SVG / MathML (13.2.6, the tree
    /// construction dispatcher): the current node is foreign and no integration point.
    fn foreign_for_start(&self) -> bool {
        self.stack
            .last()
            .is_some_and(|e| e.foreign && !rules::is(&e.key, INTEGRATION_POINT))
    }

    fn template_open(&self) -> bool {
        self.stack.iter().any(|e| e.key == "template" && !e.foreign)
    }

    fn find_in_scope(&self, key: &str, scope: Scope) -> Option<usize> {
        self.in_scope_any(&[key], scope)
    }

    /// The nearest open element named one of `keys`, if no boundary of `scope` comes first.
    fn in_scope_any(&self, keys: &[&str], scope: Scope) -> Option<usize> {
        for (i, e) in self.stack.iter().enumerate().rev() {
            if keys.contains(&e.key.as_str()) {
                return Some(i);
            }
            if scope.is_boundary(&e.key) {
                return None;
            }
        }
        None
    }

    /// Whether the open element at `index` is in `scope` (no boundary above it).
    fn node_in_scope(&self, index: usize, scope: Scope) -> bool {
        !self.stack[index + 1..]
            .iter()
            .any(|e| scope.is_boundary(&e.key))
    }

    fn count_open(&mut self, key: &str, opened: bool) {
        let counter = match key {
            "p" => &mut self.open_paragraphs,
            "li" | "dd" | "dt" => &mut self.open_list_items,
            _ => return,
        };
        if opened {
            *counter += 1;
        } else {
            *counter = counter.saturating_sub(1);
        }
    }

    fn push_entry(&mut self, key: &str, node: usize, foreign: bool) {
        let below = self.stack.last().map_or(Mode::Body, |e| e.mode);
        let mode = if foreign {
            below
        } else {
            rules::mode_of(key).unwrap_or(below)
        };
        self.count_open(key, true);
        self.stack.push(OpenElement {
            key: key.to_string(),
            node,
            pending_close: false,
            mode,
            foreign,
        });
    }

    /// Take the open element at `index` off the stack (wherever it is).
    fn remove_entry(&mut self, index: usize) {
        let e = self.stack.remove(index);
        self.count_open(&e.key, false);
    }

    /// XML: hand the pending text to the sink.
    fn flush_text(&mut self, sink: &mut dyn TreeSink) {
        if !self.pending_text.is_empty() {
            sink.text(&self.pending_text);
            self.pending_text.clear();
        }
    }

    /// Insert an element `name` (`key` lower-cased) with `attributes` where it goes and,
    /// with `push`, open it (else it is closed at once).
    fn insert_element(
        &mut self,
        sink: &mut dyn TreeSink,
        name: &str,
        key: &str,
        attributes: Vec<(String, String)>,
        push: bool,
        foreign: bool,
    ) -> usize {
        if self.html() {
            let place = self.appropriate_place(None);
            let node = self.tree.create(NodeData::Element {
                name: name.to_string(),
                attributes,
            });
            self.tree.insert(place, node);
            if push {
                self.push_entry(key, node, foreign);
            }
            return node;
        }
        self.flush_text(sink);
        sink.open_element(name, &attributes);
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        if push {
            self.push_entry(key, id, false);
        } else {
            sink.close_element();
        }
        id
    }

    /// Close the current node.
    fn pop_one(&mut self, sink: &mut dyn TreeSink) {
        let Some(e) = self.stack.pop() else {
            return;
        };
        self.count_open(&e.key, false);
        if !self.html() {
            self.flush_text(sink);
            sink.close_element();
            return;
        }
        if !e.foreign && rules::is(&e.key, MARKER) {
            // A cell closed: what was opened in it is not reopened outside.
            while let Some(f) = self.active.pop() {
                if matches!(f, Formatting::Marker) {
                    break;
                }
            }
        }
    }

    /// Close the element at `index` and every element above it; then (XML) every
    /// element whose end tag is pending and that is now the current node.
    fn pop_to(&mut self, sink: &mut dyn TreeSink, index: usize) {
        while self.stack.len() > index {
            self.pop_one(sink);
        }
        while self.stack.last().is_some_and(|e| e.pending_close) {
            self.pop_one(sink);
        }
    }

    // ---- the steps both rule sets share (13.2.6.4.7) ----

    /// "Close a p element" if one is open in button scope.
    fn close_p(&mut self, sink: &mut dyn TreeSink) {
        if self.open_paragraphs == 0 {
            return;
        }
        if let Some(i) = self.find_in_scope("p", Scope::Button) {
            self.pop_to(sink, i);
        }
    }

    /// An `<li>` closes the open `<li>` (a `<dd>` / `<dt>` the open `<dd>`
    /// or `<dt>`) - not one of an outer list, nor one beyond a block.
    fn close_list_item(&mut self, sink: &mut dyn TreeSink, names: &[&str]) {
        if self.open_list_items == 0 {
            return;
        }
        for i in (0..self.stack.len()).rev() {
            let key = self.stack[i].key.as_str();
            if names.contains(&key) {
                self.pop_to(sink, i);
                return;
            }
            if rules::is(key, SPECIAL) && !matches!(key, "address" | "div" | "p") {
                return;
            }
        }
    }

    /// XML: close the open elements named `names` of the current table (and what
    /// is open inside them): the cell before a new cell, the cell and the
    /// row before a new row.
    fn close_in_table(&mut self, sink: &mut dyn TreeSink, names: &[&str]) {
        let mut lowest = None;
        for i in (0..self.stack.len()).rev() {
            let key = self.stack[i].key.as_str();
            if Scope::Table.is_boundary(key) {
                break;
            }
            if names.contains(&key) {
                lowest = Some(i);
            }
        }
        if let Some(i) = lowest {
            self.pop_to(sink, i);
        }
    }

    /// 13.2.6.3 "generate implied end tags" (but for `except`).
    fn generate_implied_end_tags(&mut self, sink: &mut dyn TreeSink, except: Option<&str>) {
        while let Some(e) = self.stack.last() {
            if e.foreign || !rules::is(&e.key, IMPLIED_END) || Some(e.key.as_str()) == except {
                return;
            }
            self.pop_one(sink);
        }
    }

    /// Run one step of a start tag's rule; `false`: the token is done (inserted or
    /// ignored).
    fn run_step(&mut self, sink: &mut dyn TreeSink, step: Step, tag: &mut Tag) -> bool {
        let html = self.html();
        match step {
            Step::CloseP => self.close_p(sink),
            Step::ClosePUnlessQuirks => {
                if !self.quirks {
                    self.close_p(sink);
                }
            }
            Step::PopHeading => {
                if self.current().is_some_and(|c| rules::is(c, HEADING)) {
                    self.pop_to(sink, self.stack.len() - 1);
                }
            }
            Step::PopCurrent(name) => {
                if self.current() == Some(name) {
                    self.pop_to(sink, self.stack.len() - 1);
                }
            }
            Step::CloseListItem(names) => self.close_list_item(sink, names),
            Step::CloseInTable(names) => {
                if !html {
                    self.close_in_table(sink, names);
                }
            }
            Step::CloseInScope(name) => {
                if html {
                    if let Some(i) = self.find_in_scope(name, Scope::Default) {
                        self.pop_to(sink, i);
                    }
                }
            }
            Step::CloseRubyParts { except_rtc } => {
                if html && self.find_in_scope("ruby", Scope::Default).is_some() {
                    self.generate_implied_end_tags(sink, except_rtc.then_some("rtc"));
                }
            }
            Step::CloseOpenLink => {
                if html {
                    self.close_open_link(sink);
                }
            }
            Step::CloseOpenNobr => {
                if html && self.find_in_scope("nobr", Scope::Default).is_some() {
                    let _ = self.adoption_agency(sink, "nobr");
                    self.reconstruct(sink);
                }
            }
            Step::IgnoreInForm => {
                if html && self.form.is_some() && !self.template_open() {
                    return false;
                }
            }
            Step::Ignore => {
                if html {
                    return false;
                }
            }
            Step::Reconstruct => {
                if html {
                    self.reconstruct(sink);
                }
            }
            Step::Insert
            | Step::InsertVoid
            | Step::InsertFormatting
            | Step::InsertMarker
            | Step::InsertForm
            | Step::InsertSkippingNewline
            | Step::InsertForeign => {
                self.insert_for(sink, step, tag);
                return false;
            }
        }
        true
    }

    /// The insertion step of a start tag's rule.
    fn insert_for(&mut self, sink: &mut dyn TreeSink, step: Step, tag: &mut Tag) {
        let attributes = core::mem::take(&mut tag.attributes);
        let void = step == Step::InsertVoid || rules::is(&tag.key, VOID);
        if !self.html() {
            // XML: an empty element `<x/>` or a void one opens and closes at once.
            let push = !(void || tag.self_closing);
            let _ = self.insert_element(sink, &tag.name, &tag.key, attributes, push, false);
            return;
        }
        let foreign = step == Step::InsertForeign;
        // Past the depth limit, an element's content goes to its parent (as in Blink).
        let push = !void
            && !(foreign && tag.self_closing)
            && self.stack.len() < MAX_XML_NESTING_DEPTH;
        let remembered = (step == Step::InsertFormatting && push).then(|| attributes.clone());
        let node = self.insert_element(sink, &tag.name, &tag.key, attributes, push, foreign);
        match step {
            Step::InsertFormatting => {
                if let Some(attributes) = remembered {
                    self.remember_formatting(node, &tag.key, attributes);
                }
            }
            Step::InsertMarker => {
                if push {
                    self.active.push(Formatting::Marker);
                }
            }
            Step::InsertForm => {
                if !self.template_open() {
                    self.form = Some(node);
                }
            }
            Step::InsertSkippingNewline => self.skip_newline = true,
            _ => {}
        }
    }

    // ---- XML ----

    fn xml_start_tag(
        &mut self,
        sink: &mut dyn TreeSink,
        name: &str,
        attributes: Vec<(String, String)>,
        self_closing: bool,
    ) {
        let name: Cow<'_, str> = if self.rules.folds_case() {
            lower(name)
        } else {
            Cow::Borrowed(name)
        };
        let key = lower(&name).into_owned();
        let mut tag = Tag {
            name: name.into_owned(),
            key,
            attributes,
            self_closing,
        };
        for step in rules::start_steps(&tag.key) {
            if !self.run_step(sink, *step, &mut tag) {
                return;
            }
        }
    }

    fn xml_end_tag(&mut self, sink: &mut dyn TreeSink, key: &str) {
        if rules::is(key, VOID) {
            return;
        }
        let (scope, _) = rules::end_rule(key);
        let Some(i) = self.find_in_scope(key, scope) else {
            return;
        };
        if rules::is(key, FORMATTING)
            && self.stack[i + 1..]
                .iter()
                .any(|e| rules::is(&e.key, SPECIAL))
        {
            // `<b><p>x</b>y</p>`: the paragraph stays open; the `<b>`
            // closes when the paragraph does.
            self.stack[i].pending_close = true;
            return;
        }
        self.pop_to(sink, i);
    }

    // ==== HTML (part 2) ====
}
