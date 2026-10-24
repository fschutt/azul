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
//! comment stays one run); `select` content is body content (as in Chrome's current,
//! customizable-select parser, but without "in select"'s own end tag rules); `frameset` is
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
        self, ContentModel, EndAction, Mode, Scope, Step, TableEnd, TableStart, BREAKOUT, FORMATTING,
        FOSTER_TARGET, HEAD, HEADING, IMPLIED_END, INTEGRATION_POINT, MARKER, SPECIAL, VOID,
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
    /// HTML: an SVG / `MathML` element (13.2.6.5).
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
    pub const fn open_elements(&self) -> usize {
        self.stack.len()
    }

    /// Whether the adjusted current node is an SVG / `MathML` element (HTML): a CDATA
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
        rules::element(&key).map_or(TextMode::Data, |e| match e.content {
            ContentModel::Markup => TextMode::Data,
            ContentModel::RcData => TextMode::RcData(e.name),
            ContentModel::RawText => TextMode::RawText(e.name),
            ContentModel::PlainText => TextMode::PlainText,
        })
    }

    // ---- the stack ----

    const fn html(&self) -> bool {
        self.rules.html()
    }

    fn current(&self) -> Option<&str> {
        self.stack.last().map(|e| e.key.as_str())
    }

    /// HTML: the insertion mode in the body.
    fn mode(&self) -> Mode {
        self.stack.last().map_or(Mode::Body, |e| e.mode)
    }

    /// HTML: start tags and text are processed as SVG / `MathML` (13.2.6, the tree
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
        let push =
            !(void || (foreign && tag.self_closing)) && self.stack.len() < MAX_XML_NESTING_DEPTH;
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

    // ---- HTML: where nodes go ----

    /// 13.2.6.1 "the appropriate place for inserting a node": the end of the current node
    /// (or of the override target, a stack index) - but in front of the table when foster
    /// parenting is enabled and the target is a table, a row group or a row.
    fn appropriate_place(&self, override_target: Option<usize>) -> Place {
        let Some(target) = override_target.or_else(|| self.stack.len().checked_sub(1)) else {
            return Place {
                parent: DOCUMENT,
                before: None,
            };
        };
        let e = &self.stack[target];
        if !(self.foster && !e.foreign && rules::is(&e.key, FOSTER_TARGET)) {
            return Place {
                parent: e.node,
                before: None,
            };
        }
        let last = |name: &str| self.stack.iter().rposition(|e| e.key == name && !e.foreign);
        let last_table = last("table");
        if let Some(template) = last("template") {
            if last_table.is_none_or(|t| template > t) {
                return Place {
                    parent: self.stack[template].node,
                    before: None,
                };
            }
        }
        let Some(table) = last_table else {
            return Place {
                parent: self.stack[0].node,
                before: None,
            };
        };
        let table_node = self.stack[table].node;
        self.tree.parent(table_node).map_or_else(
            || Place {
                parent: self.stack[table.saturating_sub(1)].node,
                before: None,
            },
            |parent| Place {
                parent,
                before: Some(table_node),
            },
        )
    }

    /// HTML: text where it goes (13.2.6.1 "insert a character").
    fn insert_text(&mut self, text: &str) {
        let place = self.appropriate_place(None);
        self.tree.insert_text(place, text);
    }

    /// HTML: an element of the document (`html`, `head`, `body`, an implied `tbody` ...),
    /// opened.
    fn insert_named(
        &mut self,
        sink: &mut dyn TreeSink,
        key: &str,
        attributes: Vec<(String, String)>,
    ) -> usize {
        self.insert_element(sink, key, key, attributes, true, false)
    }

    /// HTML: the attributes the open element at `index` (`html`, `body`) lacks, added.
    fn merge_into(&mut self, index: usize, attributes: Vec<(String, String)>) {
        if let Some(e) = self.stack.get(index) {
            let node = e.node;
            self.tree.merge_attributes(node, attributes);
        }
    }

    /// HTML: close the open `name` and everything above it, if it is open.
    fn pop_named(&mut self, sink: &mut dyn TreeSink, name: &str) {
        if let Some(i) = self.stack.iter().rposition(|e| e.key == name && !e.foreign) {
            self.pop_to(sink, i);
        }
    }

    /// HTML: the current node holds raw text or RCDATA (13.2.6.4.8 "text": the tokenizer
    /// reads its content as text, its end tag ends it).
    fn in_text_element(&self) -> bool {
        self.stack
            .last()
            .is_some_and(|e| !e.foreign && rules::content(&e.key) != ContentModel::Markup)
    }

    // ---- HTML: the list of active formatting elements (13.2.4.3) ----

    fn last_marker_end(&self) -> usize {
        self.active
            .iter()
            .rposition(|f| matches!(f, Formatting::Marker))
            .map_or(0, |m| m + 1)
    }

    fn is_open(&self, node: usize) -> bool {
        self.stack.iter().any(|e| e.node == node)
    }

    fn active_index(&self, node: usize) -> Option<usize> {
        self.active
            .iter()
            .position(|f| matches!(f, Formatting::Element { node: n, .. } if *n == node))
    }

    /// "Reconstruct the active formatting elements": reopen the formatting elements that
    /// were closed by a block's start (`<p><b>x<p>y`: y is bold too).
    fn reconstruct(&mut self, sink: &mut dyn TreeSink) {
        let start = self.last_marker_end();
        let mut first = self.active.len();
        while first > start {
            match &self.active[first - 1] {
                Formatting::Element { node, .. } if !self.is_open(*node) => first -= 1,
                _ => break,
            }
        }
        for index in first..self.active.len() {
            if self.stack.len() >= MAX_XML_NESTING_DEPTH {
                return;
            }
            let (key, attributes) = match &self.active[index] {
                Formatting::Element {
                    key, attributes, ..
                } => (key.clone(), attributes.clone()),
                Formatting::Marker => continue,
            };
            let node = self.insert_element(sink, &key, &key, attributes, true, false);
            if let Some(Formatting::Element { node: n, .. }) = self.active.get_mut(index) {
                *n = node;
            }
        }
    }

    /// Put a formatting element on the list; at most three equal ones after
    /// the last marker (the HTML Standard's "Noah's Ark" clause), so a
    /// thousand unclosed `<font>`s are not reopened a thousand times.
    fn remember_formatting(&mut self, node: usize, key: &str, attributes: Vec<(String, String)>) {
        let start = self.last_marker_end();
        let equal: Vec<usize> = self.active[start..]
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                matches!(f, Formatting::Element { key: k, attributes: a, .. } if k == key && *a == attributes)
            })
            .map(|(i, _)| start + i)
            .collect();
        if equal.len() >= 3 {
            drop(self.active.remove(equal[0]));
        }
        self.active.push(Formatting::Element {
            node,
            key: String::from(key),
            attributes,
        });
    }

    /// The start tag `a` while an `a` is in the list after the last marker: the adoption
    /// agency for `a`, then that `a` leaves the list and the stack (a link does not nest in
    /// a link).
    fn close_open_link(&mut self, sink: &mut dyn TreeSink) {
        let start = self.last_marker_end();
        let Some(node) = self.active[start..].iter().rev().find_map(|f| match f {
            Formatting::Element { node, key, .. } if key == "a" => Some(*node),
            _ => None,
        }) else {
            return;
        };
        let _ = self.adoption_agency(sink, "a");
        if let Some(i) = self.active_index(node) {
            drop(self.active.remove(i));
        }
        if let Some(i) = self.stack.iter().position(|e| e.node == node) {
            self.remove_entry(i);
        }
    }

    /// 13.2.6.4.7 "the adoption agency algorithm" for the end tag `subject`.
    ///
    /// A formatting element with a block open inside it ends where it stands, the block
    /// moves out of it, and a clone of it takes the block's content. `false`: "act as
    /// described in the any other end tag entry".
    fn adoption_agency(&mut self, sink: &mut dyn TreeSink, subject: &str) -> bool {
        // 2. The current node is a `subject` the list does not know: it simply closes.
        if let Some(current) = self.stack.last() {
            if current.key == subject && self.active_index(current.node).is_none() {
                self.pop_one(sink);
                return true;
            }
        }
        // 3. - 4. The outer loop.
        for _ in 0..8 {
            // 4.3 The formatting element: the last `subject` after the last marker.
            let start = self.last_marker_end();
            let Some((entry, formatting)) =
                self.active[start..]
                    .iter()
                    .enumerate()
                    .rev()
                    .find_map(|(i, f)| match f {
                        Formatting::Element { node, key, .. } if key == subject => {
                            Some((start + i, *node))
                        }
                        _ => None,
                    })
            else {
                return false;
            };
            // 4.4 Not open: it leaves the list.
            let Some(formatting_at) = self.stack.iter().rposition(|e| e.node == formatting) else {
                drop(self.active.remove(entry));
                return true;
            };
            // 4.5 Open but not in scope: the end tag is ignored.
            if !self.node_in_scope(formatting_at, Scope::Default) {
                return true;
            }
            // 4.7 The furthest block: the first special element above it.
            let Some(block_at) = (formatting_at + 1..self.stack.len())
                .find(|&i| rules::is(&self.stack[i].key, SPECIAL))
            else {
                // 4.8 None: close it (and what is open inside it).
                while self.stack.len() > formatting_at {
                    self.pop_one(sink);
                }
                drop(self.active.remove(entry));
                return true;
            };
            let furthest_block = self.stack[block_at].node;
            // 4.13 The inner loop.
            let (last_node, bookmark, entry) = self.adopt_between(formatting_at, block_at, entry);
            // 4.14 The last node goes where the common ancestor takes it.
            let place = self.appropriate_place(Some(formatting_at.saturating_sub(1)));
            self.tree.insert(place, last_node);
            // 4.15 - 4.17 A clone of the formatting element takes the furthest block's
            // content and goes into it.
            let (key, attributes) = match &self.active[entry] {
                Formatting::Element {
                    key, attributes, ..
                } => (key.clone(), attributes.clone()),
                Formatting::Marker => return true,
            };
            let clone = self.tree.create(NodeData::Element {
                name: key.clone(),
                attributes: attributes.clone(),
            });
            self.tree.move_children(furthest_block, clone);
            self.tree.append(furthest_block, clone);
            // 4.18 The clone replaces the formatting element in the list (at the
            // bookmark) ...
            drop(self.active.remove(entry));
            let bookmark = if entry < bookmark {
                bookmark - 1
            } else {
                bookmark
            };
            self.active.insert(
                bookmark.min(self.active.len()),
                Formatting::Element {
                    node: clone,
                    key: key.clone(),
                    attributes,
                },
            );
            // 4.19 ... and on the stack, right above the furthest block.
            self.remove_entry(formatting_at);
            let block_now = self
                .stack
                .iter()
                .position(|e| e.node == furthest_block)
                .unwrap_or(self.stack.len() - 1);
            let mode = self.stack[block_now].mode;
            self.stack.insert(
                block_now + 1,
                OpenElement {
                    key,
                    node: clone,
                    pending_close: false,
                    mode,
                    foreign: false,
                },
            );
        }
        true
    }

    /// The adoption agency's inner loop (4.11 - 4.13): the elements between the formatting
    /// element (open at `formatting_at`, list entry `entry`) and the furthest block (open
    /// at `block_at`) are cloned around the block (the formatting ones) or left behind (the
    /// others). The last node, the bookmark, and the formatting element's list entry (it
    /// moves when an entry before it goes).
    fn adopt_between(
        &mut self,
        formatting_at: usize,
        block_at: usize,
        entry: usize,
    ) -> (usize, usize, usize) {
        let mut entry = entry;
        let mut bookmark = entry;
        let furthest_block = self.stack[block_at].node;
        let mut last_node = furthest_block;
        let mut node_index = block_at;
        let mut inner = 0;
        loop {
            inner += 1;
            node_index -= 1;
            if node_index <= formatting_at {
                break;
            }
            let node = self.stack[node_index].node;
            let mut list_index = self.active_index(node);
            if inner > 3 {
                if let Some(li) = list_index.take() {
                    drop(self.active.remove(li));
                    if li < bookmark {
                        bookmark -= 1;
                    }
                    if li < entry {
                        entry -= 1;
                    }
                }
            }
            let Some(li) = list_index else {
                self.remove_entry(node_index);
                continue;
            };
            let (key, attributes) = match &self.active[li] {
                Formatting::Element {
                    key, attributes, ..
                } => (key.clone(), attributes.clone()),
                Formatting::Marker => break,
            };
            let clone = self.tree.create(NodeData::Element {
                name: key.clone(),
                attributes: attributes.clone(),
            });
            self.active[li] = Formatting::Element {
                node: clone,
                key,
                attributes,
            };
            self.stack[node_index].node = clone;
            if last_node == furthest_block {
                bookmark = li + 1;
            }
            self.tree.append(clone, last_node);
            last_node = clone;
        }
        (last_node, bookmark, entry)
    }

    // ---- HTML: start tags ----

    /// A start tag, by the document's phase (13.2.6.4.1 - 13.2.6.4.6) and in the body by
    /// the insertion mode.
    fn html_start_tag(&mut self, sink: &mut dyn TreeSink, tag: &mut Tag) {
        loop {
            let again = match self.phase {
                Phase::Initial => {
                    // No doctype: quirks mode.
                    self.quirks = true;
                    self.phase = Phase::BeforeHtml;
                    true
                }
                Phase::BeforeHtml => {
                    let attributes = if tag.key == "html" {
                        core::mem::take(&mut tag.attributes)
                    } else {
                        Vec::new()
                    };
                    let _ = self.insert_named(sink, "html", attributes);
                    self.phase = Phase::BeforeHead;
                    tag.key != "html"
                }
                Phase::BeforeHead => match tag.key.as_str() {
                    "html" => {
                        self.merge_into(0, core::mem::take(&mut tag.attributes));
                        false
                    }
                    "head" => {
                        let attributes = core::mem::take(&mut tag.attributes);
                        self.head = Some(self.insert_named(sink, "head", attributes));
                        self.phase = Phase::InHead;
                        false
                    }
                    _ => {
                        // The implied head.
                        self.head = Some(self.insert_named(sink, "head", Vec::new()));
                        self.phase = Phase::InHead;
                        true
                    }
                },
                // 13.2.6.4.18 "in template": a template's content (also in the head) is
                // read by the body's rules.
                Phase::InHead | Phase::AfterHead if self.template_open() => {
                    self.body_start(sink, tag)
                }
                Phase::InHead => self.in_head_start(sink, tag),
                Phase::AfterHead => self.after_head_start(sink, tag),
                Phase::InBody => self.body_start(sink, tag),
            };
            if !again {
                return;
            }
        }
    }

    /// The head's content (13.2.6.4.4), inserted where it stands by its row of
    /// [`rules::START_TAGS`] (one insertion step: void, a template's marker ...).
    fn insert_head_content(&mut self, sink: &mut dyn TreeSink, tag: &mut Tag) {
        for step in rules::start_steps(&tag.key) {
            if !self.run_step(sink, *step, tag) {
                return;
            }
        }
    }

    /// 13.2.6.4.4 "in head" and 13.2.6.4.5 "in head noscript"; `true`: reprocess.
    fn in_head_start(&mut self, sink: &mut dyn TreeSink, tag: &mut Tag) -> bool {
        let key = tag.key.as_str();
        if key == "html" {
            self.merge_into(0, core::mem::take(&mut tag.attributes));
            return false;
        }
        if self.current() == Some("noscript") {
            // Scripting off: a `<noscript>` in the head holds only these.
            return match key {
                "basefont" | "bgsound" | "link" | "meta" | "noframes" | "style" => {
                    self.insert_head_content(sink, tag);
                    false
                }
                "head" | "noscript" => false,
                _ => {
                    self.pop_one(sink);
                    true
                }
            };
        }
        match key {
            "head" => false,
            "noscript" => {
                self.insert_for(sink, Step::Insert, tag);
                false
            }
            k if rules::is(k, HEAD) => {
                self.insert_head_content(sink, tag);
                false
            }
            _ => {
                // Anything else ends the head.
                self.pop_named(sink, "head");
                self.phase = Phase::AfterHead;
                true
            }
        }
    }

    /// 13.2.6.4.6 "after head"; `true`: reprocess.
    fn after_head_start(&mut self, sink: &mut dyn TreeSink, tag: &mut Tag) -> bool {
        match tag.key.as_str() {
            "html" => {
                self.merge_into(0, core::mem::take(&mut tag.attributes));
                false
            }
            "body" => {
                let attributes = core::mem::take(&mut tag.attributes);
                let _ = self.insert_named(sink, "body", attributes);
                self.phase = Phase::InBody;
                false
            }
            "head" => false,
            k if rules::is(k, HEAD) => {
                // "Push the node pointed to by the head element pointer onto the stack of
                // open elements. Process the token using the rules for the "in head"
                // insertion mode. Remove the node pointed to by the head element pointer
                // from the stack of open elements."
                let Some(head) = self.head else {
                    return false;
                };
                self.push_entry("head", head, false);
                self.insert_head_content(sink, tag);
                if let Some(i) = self.stack.iter().rposition(|e| e.node == head) {
                    self.remove_entry(i);
                }
                false
            }
            _ => {
                // The implied body.
                let _ = self.insert_named(sink, "body", Vec::new());
                self.phase = Phase::InBody;
                true
            }
        }
    }

    /// A start tag in the body: foreign content (13.2.6.5), the table modes, "in body";
    /// `true`: reprocess.
    fn body_start(&mut self, sink: &mut dyn TreeSink, tag: &mut Tag) -> bool {
        if self.foreign_for_start() {
            let breaks_out = rules::is(&tag.key, BREAKOUT)
                || (tag.key == "font"
                    && tag
                        .attributes
                        .iter()
                        .any(|(k, _)| matches!(k.as_str(), "color" | "face" | "size")));
            if breaks_out {
                // HTML ends the SVG / MathML content.
                while self.foreign_for_start() {
                    self.pop_one(sink);
                }
                return true;
            }
            let attributes = core::mem::take(&mut tag.attributes);
            let push = !tag.self_closing && self.stack.len() < MAX_XML_NESTING_DEPTH;
            let _ = self.insert_element(sink, &tag.name, &tag.key, attributes, push, true);
            return false;
        }
        let mode = self.mode();
        match mode {
            Mode::Body => self.in_body_start(sink, tag),
            Mode::Template => match rules::template_start(&tag.key) {
                Some(action) => self.table_start(sink, action, tag),
                None => self.in_body_start(sink, tag),
            },
            Mode::Cell | Mode::Caption => match rules::table_start(mode, &tag.key) {
                Some(action) => self.table_start(sink, action, tag),
                None => self.in_body_start(sink, tag),
            },
            Mode::ColumnGroup => match rules::table_start(mode, &tag.key) {
                Some(action) => self.table_start(sink, action, tag),
                None => {
                    // Anything else ends the column group.
                    if self.current() == Some("colgroup") {
                        self.pop_one(sink);
                        true
                    } else {
                        false
                    }
                }
            },
            Mode::Table | Mode::TableBody | Mode::Row => {
                let action = rules::table_start(mode, &tag.key).or_else(|| {
                    (mode != Mode::Table)
                        .then(|| rules::table_start(Mode::Table, &tag.key))
                        .flatten()
                });
                match action {
                    Some(action) => self.table_start(sink, action, tag),
                    None => self.fostered_start(sink, tag),
                }
            }
        }
    }

    /// 13.2.6.4.9 "in table", anything else: the body's rules with foster parenting.
    fn fostered_start(&mut self, sink: &mut dyn TreeSink, tag: &mut Tag) -> bool {
        self.foster = true;
        let again = self.in_body_start(sink, tag);
        self.foster = false;
        again
    }

    /// A row of [`rules::TABLE_START_TAGS`]; `true`: reprocess.
    fn table_start(&mut self, sink: &mut dyn TreeSink, action: TableStart, tag: &mut Tag) -> bool {
        match action {
            TableStart::Insert(context, marker) => {
                self.clear_to(sink, context);
                self.insert_for(
                    sink,
                    if marker {
                        Step::InsertMarker
                    } else {
                        Step::Insert
                    },
                    tag,
                );
                false
            }
            TableStart::Imply(context, implied) => {
                self.clear_to(sink, context);
                let _ = self.insert_named(sink, implied, Vec::new());
                true
            }
            TableStart::CloseAndReprocess(names) => self.close_and_reprocess(sink, names),
            TableStart::InsertHere => {
                self.insert_head_content(sink, tag);
                false
            }
            TableStart::Input => {
                let hidden = tag
                    .attributes
                    .iter()
                    .any(|(k, v)| k == "type" && v.eq_ignore_ascii_case("hidden"));
                if hidden {
                    self.insert_for(sink, Step::InsertVoid, tag);
                    false
                } else {
                    self.fostered_start(sink, tag)
                }
            }
            TableStart::Form => {
                if self.form.is_none() && !self.template_open() {
                    let attributes = core::mem::take(&mut tag.attributes);
                    let node = self.insert_element(sink, "form", "form", attributes, false, false);
                    self.form = Some(node);
                }
                false
            }
            TableStart::InsertVoid => {
                self.insert_for(sink, Step::InsertVoid, tag);
                false
            }
        }
    }

    /// "Clear the stack back to a ... context".
    fn clear_to(&mut self, sink: &mut dyn TreeSink, context: rules::Context) {
        while self.stack.last().is_some_and(|e| !context.holds(&e.key)) {
            self.pop_one(sink);
        }
    }

    /// If one of `names` is open in table scope: close it (and what is open in it) and
    /// reprocess the token (`true`); else ignore the token.
    fn close_and_reprocess(&mut self, sink: &mut dyn TreeSink, names: &[&str]) -> bool {
        let Some(i) = self.in_scope_any(names, Scope::Table) else {
            return false;
        };
        self.pop_to(sink, i);
        true
    }

    /// 13.2.6.4.7 "in body", a start tag; `true`: reprocess.
    fn in_body_start(&mut self, sink: &mut dyn TreeSink, tag: &mut Tag) -> bool {
        match tag.key.as_str() {
            "html" => {
                // A second `<html>`: its attributes the first one lacks.
                self.merge_into(0, core::mem::take(&mut tag.attributes));
                return false;
            }
            "body" => {
                // A second `<body>`: the same.
                if self.stack.get(1).is_some_and(|e| e.key == "body") && !self.template_open() {
                    self.merge_into(1, core::mem::take(&mut tag.attributes));
                }
                return false;
            }
            "frameset" => return false,
            _ => {}
        }
        if let Some((_, alias)) = rules::ALIASES.iter().find(|(from, _)| *from == tag.key) {
            tag.key = (*alias).to_string();
            tag.name = (*alias).to_string();
        }
        for step in rules::start_steps(&tag.key) {
            if !self.run_step(sink, *step, tag) {
                break;
            }
        }
        false
    }

    // ---- HTML: end tags ----

    /// An end tag, by the document's phase and in the body by the insertion mode; `true`:
    /// reprocess.
    fn html_end_tag(&mut self, sink: &mut dyn TreeSink, key: &str) -> bool {
        if self.in_text_element() {
            // 13.2.6.4.8 "text": the only end tag the tokenizer reads in raw text is the
            // element's own.
            self.pop_one(sink);
            return false;
        }
        let ends_head = matches!(key, "head" | "body" | "html" | "br");
        match self.phase {
            Phase::Initial => {
                self.quirks = true;
                self.phase = Phase::BeforeHtml;
                true
            }
            Phase::BeforeHtml => {
                if ends_head {
                    let _ = self.insert_named(sink, "html", Vec::new());
                    self.phase = Phase::BeforeHead;
                }
                ends_head
            }
            Phase::BeforeHead => {
                if ends_head {
                    self.head = Some(self.insert_named(sink, "head", Vec::new()));
                    self.phase = Phase::InHead;
                }
                ends_head
            }
            Phase::InHead | Phase::AfterHead if self.template_open() && key != "template" => {
                self.body_end(sink, key)
            }
            Phase::InHead => {
                if self.current() == Some("noscript") {
                    return match key {
                        "noscript" => {
                            self.pop_one(sink);
                            false
                        }
                        "br" => {
                            self.pop_one(sink);
                            true
                        }
                        _ => false,
                    };
                }
                match key {
                    "template" => {
                        self.pop_named(sink, "template");
                        false
                    }
                    "head" | "body" | "html" | "br" => {
                        self.pop_named(sink, "head");
                        self.phase = Phase::AfterHead;
                        key != "head"
                    }
                    _ => false,
                }
            }
            Phase::AfterHead => match key {
                "template" => {
                    self.pop_named(sink, "template");
                    false
                }
                "body" | "html" | "br" => {
                    let _ = self.insert_named(sink, "body", Vec::new());
                    self.phase = Phase::InBody;
                    true
                }
                _ => false,
            },
            Phase::InBody => self.body_end(sink, key),
        }
    }

    /// An end tag in the body: foreign content, the table modes, "in body"; `true`:
    /// reprocess.
    fn body_end(&mut self, sink: &mut dyn TreeSink, key: &str) -> bool {
        if self.stack.last().is_some_and(|e| e.foreign) {
            // 13.2.6.5 "any other end tag" in foreign content.
            if key == "br" || key == "p" {
                while self.foreign_for_start() {
                    self.pop_one(sink);
                }
                return true;
            }
            let mut i = self.stack.len() - 1;
            loop {
                if i == 0 {
                    return false;
                }
                if self.stack[i].key == key {
                    self.pop_to(sink, i);
                    return false;
                }
                i -= 1;
                if !self.stack[i].foreign {
                    break;
                }
            }
        }
        let mode = self.mode();
        match mode {
            Mode::Body | Mode::Template => self.in_body_end(sink, key),
            Mode::Cell | Mode::Caption => match rules::table_end(mode, key) {
                Some(action) => self.table_end(sink, action, key),
                None => self.in_body_end(sink, key),
            },
            Mode::ColumnGroup => match rules::table_end(mode, key) {
                Some(action) => self.table_end(sink, action, key),
                None => {
                    if self.current() == Some("colgroup") {
                        self.pop_one(sink);
                        true
                    } else {
                        false
                    }
                }
            },
            Mode::Table | Mode::TableBody | Mode::Row => {
                let action = rules::table_end(mode, key).or_else(|| {
                    (mode != Mode::Table)
                        .then(|| rules::table_end(Mode::Table, key))
                        .flatten()
                });
                if let Some(action) = action {
                    return self.table_end(sink, action, key);
                }
                // 13.2.6.4.9 "in table", anything else: the body's rules with foster
                // parenting.
                self.foster = true;
                let again = self.in_body_end(sink, key);
                self.foster = false;
                again
            }
        }
    }

    /// A row of [`rules::TABLE_END_TAGS`]; `true`: reprocess.
    fn table_end(&mut self, sink: &mut dyn TreeSink, action: TableEnd, key: &str) -> bool {
        match action {
            TableEnd::Close => {
                if let Some(i) = self.find_in_scope(key, Scope::Table) {
                    self.pop_to(sink, i);
                }
                false
            }
            TableEnd::CloseAndReprocess(names) => self.close_and_reprocess(sink, names),
            TableEnd::CloseIfOpen(names) => {
                self.find_in_scope(key, Scope::Table).is_some()
                    && self.close_and_reprocess(sink, names)
            }
            TableEnd::Ignore => false,
        }
    }

    /// 13.2.6.4.7 "in body", an end tag ([`rules::END_TAGS`]); `true`: reprocess.
    fn in_body_end(&mut self, sink: &mut dyn TreeSink, key: &str) -> bool {
        let (scope, action) = rules::end_rule(key);
        match action {
            EndAction::Ignore => {}
            EndAction::CloseInScope => {
                if let Some(i) = self.find_in_scope(key, scope) {
                    self.pop_to(sink, i);
                }
            }
            EndAction::P => {
                if self.find_in_scope("p", Scope::Button).is_some() {
                    self.close_p(sink);
                } else {
                    // `</p>` without a `<p>` is an empty paragraph.
                    let _ = self.insert_element(sink, "p", "p", Vec::new(), false, false);
                }
            }
            EndAction::Heading => {
                for i in (0..self.stack.len()).rev() {
                    let k = self.stack[i].key.as_str();
                    if rules::is(k, HEADING) {
                        self.pop_to(sink, i);
                        break;
                    }
                    if Scope::Default.is_boundary(k) {
                        break;
                    }
                }
            }
            EndAction::Adoption => {
                if !self.adoption_agency(sink, key) {
                    self.any_other_end(sink, key);
                }
            }
            EndAction::CloseToMarker => {
                // The marker goes when the element closes ([`Self::pop_one`]).
                if let Some(i) = self.find_in_scope(key, Scope::Default) {
                    self.pop_to(sink, i);
                }
            }
            EndAction::Br => {
                // `</br>` is a line break.
                self.reconstruct(sink);
                let _ = self.insert_element(sink, "br", "br", Vec::new(), false, false);
            }
            EndAction::Form => self.close_form(sink),
            EndAction::AnyOther => self.any_other_end(sink, key),
        }
        false
    }

    /// "Any other end tag": the nearest open element of this name closes, unless a special
    /// element (a block) is open above it.
    fn any_other_end(&mut self, sink: &mut dyn TreeSink, key: &str) {
        for i in (0..self.stack.len()).rev() {
            let k = self.stack[i].key.as_str();
            if k == key {
                self.pop_to(sink, i);
                return;
            }
            if rules::is(k, SPECIAL) {
                return;
            }
        }
    }

    /// `</form>`: the form element pointer's element leaves the stack; what is open in it
    /// stays open.
    fn close_form(&mut self, sink: &mut dyn TreeSink) {
        if self.template_open() {
            if let Some(i) = self.find_in_scope("form", Scope::Default) {
                self.pop_to(sink, i);
            }
            return;
        }
        let Some(node) = self.form.take() else {
            return;
        };
        let Some(i) = self.stack.iter().rposition(|e| e.node == node) else {
            return;
        };
        if !self.node_in_scope(i, Scope::Default) {
            return;
        }
        self.generate_implied_end_tags(sink, None);
        if let Some(i) = self.stack.iter().rposition(|e| e.node == node) {
            self.remove_entry(i);
        }
    }

    // ---- HTML: text ----

    /// Text, by the document's phase and in the body by the insertion mode.
    fn html_text(&mut self, sink: &mut dyn TreeSink, text: &str) {
        let mut text = text;
        loop {
            if self.in_text_element() {
                self.insert_text(text);
                return;
            }
            match self.phase {
                Phase::Initial | Phase::BeforeHtml | Phase::BeforeHead => {
                    // White space before the document is nothing.
                    let (_, rest) = split_space(text);
                    if rest.is_empty() {
                        return;
                    }
                    text = rest;
                    match self.phase {
                        Phase::Initial => {
                            self.quirks = true;
                            self.phase = Phase::BeforeHtml;
                        }
                        Phase::BeforeHtml => {
                            let _ = self.insert_named(sink, "html", Vec::new());
                            self.phase = Phase::BeforeHead;
                        }
                        _ => {
                            self.head = Some(self.insert_named(sink, "head", Vec::new()));
                            self.phase = Phase::InHead;
                        }
                    }
                }
                Phase::InHead | Phase::AfterHead if self.template_open() => {
                    self.body_text(sink, text);
                    return;
                }
                Phase::InHead | Phase::AfterHead => {
                    // White space stays where it is; the rest is the body's.
                    let (space, rest) = split_space(text);
                    self.insert_text(space);
                    if rest.is_empty() {
                        return;
                    }
                    text = rest;
                    if self.phase == Phase::InHead {
                        self.pop_named(sink, "head");
                        self.phase = Phase::AfterHead;
                    } else {
                        let _ = self.insert_named(sink, "body", Vec::new());
                        self.phase = Phase::InBody;
                    }
                }
                Phase::InBody => {
                    self.body_text(sink, text);
                    return;
                }
            }
        }
    }

    /// Text in the body.
    fn body_text(&mut self, sink: &mut dyn TreeSink, text: &str) {
        if self.foreign_for_start() {
            self.insert_text(text);
            return;
        }
        match self.mode() {
            Mode::Body | Mode::Cell | Mode::Caption | Mode::Template => {
                self.reconstruct(sink);
                self.insert_text(text);
            }
            Mode::Table | Mode::TableBody | Mode::Row => {
                // 13.2.6.4.10 "in table text": white space stays in the table; text with
                // anything else in it goes in front of the table.
                let in_structure = self.stack.last().is_some_and(|e| {
                    !e.foreign && (rules::is(&e.key, FOSTER_TARGET) || e.key == "template")
                });
                if in_structure && text.bytes().all(is_html_space) {
                    self.insert_text(text);
                    return;
                }
                self.foster = true;
                self.reconstruct(sink);
                self.insert_text(text);
                self.foster = false;
            }
            Mode::ColumnGroup => {
                let (space, rest) = split_space(text);
                self.insert_text(space);
                if !rest.is_empty() && self.current() == Some("colgroup") {
                    self.pop_one(sink);
                    self.body_text(sink, rest);
                }
            }
        }
    }

    // ---- the tokens ----

    /// A start tag `<name ...>` (`self_closing`: `<name ... />`).
    pub fn start_tag(
        &mut self,
        sink: &mut dyn TreeSink,
        name: &str,
        attributes: Vec<(String, String)>,
        self_closing: bool,
    ) {
        self.skip_newline = false;
        if !self.html() {
            self.xml_start_tag(sink, name, attributes, self_closing);
            return;
        }
        let key = lower(name).into_owned();
        let mut tag = Tag {
            name: key.clone(),
            key,
            attributes,
            self_closing,
        };
        self.html_start_tag(sink, &mut tag);
    }

    /// An end tag `</name>`.
    pub fn end_tag(&mut self, sink: &mut dyn TreeSink, name: &str) {
        self.skip_newline = false;
        let key = lower(name);
        if !self.html() {
            self.xml_end_tag(sink, &key);
            return;
        }
        while self.html_end_tag(sink, &key) {}
    }

    /// Text (already decoded).
    pub fn text(&mut self, sink: &mut dyn TreeSink, text: &str) {
        let mut text = text;
        if core::mem::take(&mut self.skip_newline) {
            text = text.strip_prefix('\n').unwrap_or(text);
        }
        if text.is_empty() {
            return;
        }
        if self.html() {
            self.html_text(sink, text);
        } else {
            self.pending_text.push_str(text);
        }
    }

    /// A comment: dropped (HTML: also a bogus one); XML keeps one inside a `<style>`
    /// (`<style><!-- .. --></style>`, how Outlook writes every stylesheet) as part of the
    /// sheet - HTML reads a `<style>`'s content as raw text anyway.
    pub fn comment(&mut self, _sink: &mut dyn TreeSink, text: &str) {
        self.skip_newline = false;
        if !self.html() && self.current() == Some("style") {
            self.pending_text.push_str("<!--");
            self.pending_text.push_str(text);
            self.pending_text.push_str("-->");
        }
    }

    /// A CDATA section: HTML reads one only in SVG / `MathML`, as text; XML keeps its text
    /// inside a `<style>`, else drops it.
    pub fn cdata(&mut self, sink: &mut dyn TreeSink, text: &str) {
        self.skip_newline = false;
        if self.html() {
            if !text.is_empty() {
                self.html_text(sink, text);
            }
            return;
        }
        if self.current() == Some("style") {
            self.pending_text.push_str(text);
        }
    }

    /// A `<!DOCTYPE>`: at the start of an HTML document it decides quirks mode (13.2.6.4.1);
    /// anywhere else, and in XML, it is nothing.
    pub fn doctype(&mut self, doctype: &Doctype) {
        self.skip_newline = false;
        if self.html() && self.phase == Phase::Initial {
            self.quirks = rules::doctype_is_quirky(
                &doctype.name,
                doctype.public_id.as_deref(),
                doctype.system_id.as_deref(),
                doctype.force_quirks,
            );
            self.phase = Phase::BeforeHtml;
        }
    }

    /// The end of the input: every open element closes. The number of
    /// elements that were open (under [`TreeRules::Html`] the `<html>` and
    /// `<body>` a document always has count too).
    pub fn finish(mut self, sink: &mut dyn TreeSink) -> usize {
        if !self.html() {
            self.flush_text(sink);
            let open = self.stack.len();
            while self.stack.pop().is_some() {
                sink.close_element();
            }
            return open;
        }
        // An element whose content is text ends with the input.
        while self.in_text_element() {
            self.pop_one(sink);
        }
        // Every document has its html, head and body.
        loop {
            match self.phase {
                Phase::Initial => {
                    self.quirks = true;
                    self.phase = Phase::BeforeHtml;
                }
                Phase::BeforeHtml => {
                    let _ = self.insert_named(sink, "html", Vec::new());
                    self.phase = Phase::BeforeHead;
                }
                Phase::BeforeHead => {
                    self.head = Some(self.insert_named(sink, "head", Vec::new()));
                    self.phase = Phase::InHead;
                }
                Phase::InHead => {
                    self.pop_named(sink, "head");
                    self.phase = Phase::AfterHead;
                }
                Phase::AfterHead => {
                    let _ = self.insert_named(sink, "body", Vec::new());
                    self.phase = Phase::InBody;
                }
                Phase::InBody => break,
            }
        }
        let open = self.stack.len();
        self.tree.replay(sink);
        open
    }
}
