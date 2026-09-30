//! AzBuilder's document model: the tree the visual builder edits.
//!
//! The builder UI (`dll/src/desktop/shell2/common/debugger/debugger-dnd.js`)
//! used to edit the window's LIVE `StyledDom` in place (`insert_node` /
//! `delete_node`). That tree is a flat DFS array whose first child is derived
//! as `id + 1`, so it cannot take an insert at an arbitrary position, a move or
//! an undo, and the next `Update::RefreshDom` rebuilt it from the app's
//! `layout()` and threw every edit away.
//!
//! This module gives the builder a real document instead:
//!
//! * a tree of [`BuilderNode`]s — elements, text, and instances of library
//!   components — each with a `uid` that stays stable across moves and undo,
//! * edits that validate first and then snapshot (undo / redo),
//! * a serialisation to one XML document that is mounted over the window
//!   through the existing `mount` pipeline (`CallbackChange::RemountDom` →
//!   `LayoutWindow::e2e_mount` → `regenerate_layout`), so the native window
//!   shows exactly the document and no DOM refresh can lose it.
//!
//! Component instances are EXPANDED into plain XML before mounting: the XML
//! parser drops any `prefix:name` tag it does not know as "draws nothing"
//! (`element_draws_nothing`, core/src/xml.rs), so a `<user:card/>` would
//! otherwise vanish.
//!
//! **Template components.** "Convert subtree to component" stores the subtree
//! as an XML template in [`ComponentDef::render_fn_source`], behind
//! [`TEMPLATE_MARKER`], and points `render_fn` at
//! [`builder_template_render_fn`]. `ComponentDef` is `repr(C)` and part of the
//! C API, so this needs no new field. Parameters are written as `{name}`
//! placeholders (`{{` / `}}` for literal braces) and are ordinary data-model
//! fields, so the preview, the render-tree view and the thumbnails work for a
//! converted component through the same `render_fn` call every component
//! goes through.
//!
//! Every element the document mounts carries a marker class `azb-<uid>` so
//! the UI can map a live node back to its document node.

use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    hash::{Hash, Hasher},
};

use azul_core::{
    dom::{Dom, NodeType},
    id::NodeId,
    styled_dom::StyledDom,
    xml::{
        ComponentCodegen, ComponentDataField, ComponentDataFieldVec, ComponentDataModel,
        ComponentDef, ComponentDefVec, ComponentDefaultValue, ComponentFieldType, ComponentId,
        ComponentLibrary, ComponentLibraryVec, ComponentMap, ComponentSource,
        OptionComponentDefaultValue, ResultStyledDomRenderDomError, XmlNodeChild,
    },
};
use azul_css::{css::Css, AzString};

/// First line of a builder template in `ComponentDef::render_fn_source`.
pub const TEMPLATE_MARKER: &str = "<!-- azul-builder-template -->";

/// The document root (`<body>`) always has this uid.
pub const ROOT_UID: u64 = 0;

/// Every element the document mounts carries the marker class
/// `azb-<uid>`: how a live node (a hit test, the Inspector) finds its
/// document node. The builder's plumbing, never the user's markup.
pub const MARKER_PREFIX: &str = "azb-";

/// The document uid a marker class `azb-<uid>` names; `None` for any other
/// class (`azb-card` is an ordinary class).
#[must_use]
pub fn marker_uid(class: &str) -> Option<u64> {
    let digits = class.strip_prefix(MARKER_PREFIX)?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The document uid the live node `id` carries in its marker class, if it
/// is a mounted document element.
#[must_use]
pub fn node_marker(sd: &StyledDom, id: NodeId) -> Option<u64> {
    sd.node_data
        .as_ref()
        .get(id.index())?
        .attributes()
        .as_ref()
        .iter()
        .filter_map(|a| a.as_class())
        .find_map(marker_uid)
}

/// Undo snapshots kept per document.
const MAX_UNDO: usize = 100;

/// How deep component instances may nest inside each other while expanding
/// (a template that contains an instance of itself stops here).
const MAX_EXPANSION_DEPTH: usize = 16;

/// How deep an imported / parsed tree may nest.
const MAX_TREE_DEPTH: usize = 128;

/// Thumbnails kept per window before the cache starts over.
const MAX_THUMBNAILS: usize = 512;

/// The XML parser's void elements (`layout/src/xml/mod.rs`): written
/// self-closing, and they take no children.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// The XML parser's HTML5 auto-close rules (`layout/src/xml/mod.rs`): a `<p>`
/// ends where a block element starts, so `<p><div/></p>` parses as two
/// SIBLINGS. A document that nested them would show one tree and mount
/// another, so the builder refuses the nesting instead.
const AUTO_CLOSE: &[(&str, &[&str])] = &[
    (
        "p",
        &[
            "address",
            "article",
            "aside",
            "blockquote",
            "div",
            "dl",
            "fieldset",
            "footer",
            "form",
            "h1",
            "h2",
            "h3",
            "h4",
            "h5",
            "h6",
            "header",
            "hr",
            "main",
            "nav",
            "ol",
            "p",
            "pre",
            "section",
            "table",
            "ul",
        ],
    ),
    ("li", &["li"]),
    ("td", &["td", "th", "tr"]),
    ("th", &["td", "th", "tr"]),
    ("tr", &["tr"]),
    ("option", &["option", "optgroup"]),
    ("optgroup", &["optgroup"]),
    ("dd", &["dd", "dt"]),
    ("dt", &["dd", "dt"]),
];

thread_local! {
    /// Re-entrancy depth of [`builder_template_render_fn`]: a data-model
    /// component can render a template component that renders the first one
    /// again, and that cycle goes through `render_fn`, not through the
    /// expansion depth counter.
    static TEMPLATE_RENDER_DEPTH: Cell<usize> = const { Cell::new(0) };
}

// ===========================================================================
// Nodes
// ===========================================================================

/// What a document node is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuilderNodeKind {
    /// A plain element (`div`, `p`, … — every builtin component). The only
    /// kind that takes children.
    Element { tag: String },
    /// A text node.
    Text { text: String },
    /// An instance of a (non-builtin) library component. A leaf: its DOM is
    /// the component's, and its attributes are the component's arguments.
    Component { library: String, name: String },
}

/// One node of the builder document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuilderNode {
    /// Stable identity: survives moves, undo and redo. `0` is the root.
    pub uid: u64,
    pub kind: BuilderNodeKind,
    /// Attributes. On an element, `text` is its text content (written before
    /// the children); on a component instance these are its arguments.
    pub attrs: BTreeMap<String, String>,
    pub children: Vec<BuilderNode>,
}

impl BuilderNode {
    fn element(uid: u64, tag: &str) -> Self {
        Self {
            uid,
            kind: BuilderNodeKind::Element {
                tag: tag.to_string(),
            },
            attrs: BTreeMap::new(),
            children: Vec::new(),
        }
    }

    fn text(uid: u64, text: &str) -> Self {
        Self {
            uid,
            kind: BuilderNodeKind::Text {
                text: text.to_string(),
            },
            attrs: BTreeMap::new(),
            children: Vec::new(),
        }
    }

    /// Why this node cannot take children, or `None` if it can.
    fn why_not_a_container(&self) -> Option<String> {
        match &self.kind {
            BuilderNodeKind::Element { tag } if is_void(tag) => Some(format!(
                "node {} is a <{tag}>, a void element, which cannot have children",
                self.uid
            )),
            BuilderNodeKind::Element { .. } => None,
            BuilderNodeKind::Text { .. } => Some(format!(
                "node {} is a text node, which cannot have children",
                self.uid
            )),
            BuilderNodeKind::Component { library, name } => Some(format!(
                "node {} is an instance of the component {library}:{name}, whose content comes \
                 from the component; edit the component instead",
                self.uid
            )),
        }
    }

    /// The node as the builder UI reads it.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        m.insert("uid".into(), serde_json::json!(self.uid));
        match &self.kind {
            BuilderNodeKind::Element { tag } => {
                m.insert("kind".into(), serde_json::json!("element"));
                m.insert("tag".into(), serde_json::json!(tag));
            }
            BuilderNodeKind::Text { text } => {
                m.insert("kind".into(), serde_json::json!("text"));
                m.insert("tag".into(), serde_json::json!("#text"));
                m.insert("text".into(), serde_json::json!(text));
            }
            BuilderNodeKind::Component { library, name } => {
                m.insert("kind".into(), serde_json::json!("component"));
                m.insert("library".into(), serde_json::json!(library));
                m.insert("tag".into(), serde_json::json!(name));
            }
        }
        m.insert("attrs".into(), serde_json::json!(self.attrs));
        m.insert(
            "children".into(),
            serde_json::Value::Array(self.children.iter().map(Self::to_json).collect()),
        );
        serde_json::Value::Object(m)
    }

    /// The inverse of [`BuilderNode::to_json`], for a document read back from
    /// a project file (B4). A `uid` in the JSON is ignored: the document
    /// numbers its nodes itself ([`BuilderDocument::from_root`]).
    ///
    /// # Errors
    /// Not an object, an unknown `kind`, an invalid tag / attribute /
    /// component name, children on a node that takes none, a nesting the
    /// HTML parser would undo, or a tree nested deeper than the cap.
    pub fn from_json(v: &serde_json::Value) -> Result<Self, String> {
        Self::from_json_at(v, 0)
    }

    fn from_json_at(v: &serde_json::Value, depth: usize) -> Result<Self, String> {
        use serde_json::Value;
        if depth > MAX_TREE_DEPTH {
            return Err(format!("the document is nested deeper than {MAX_TREE_DEPTH}"));
        }
        let obj = v
            .as_object()
            .ok_or("every document node must be a JSON object")?;
        let kind = obj.get("kind").and_then(Value::as_str).unwrap_or("element");
        let tag = obj.get("tag").and_then(Value::as_str).unwrap_or("");

        if kind == "text" {
            let text = obj.get("text").and_then(Value::as_str).unwrap_or("");
            return Ok(Self::text(0, text));
        }

        let mut attrs = BTreeMap::new();
        if let Some(a) = obj.get("attrs") {
            let a = a
                .as_object()
                .ok_or("`attrs` must be an object of strings")?;
            for (k, val) in a {
                if !is_valid_attr_name(k) {
                    return Err(format!("{k:?} is not a valid attribute name"));
                }
                let s = match val {
                    Value::String(s) => s.clone(),
                    Value::Null => continue,
                    other => other.to_string(),
                };
                attrs.insert(k.clone(), s);
            }
        }

        let kind = match kind {
            "element" => {
                let tag = tag.to_ascii_lowercase();
                if !is_valid_tag(&tag) {
                    return Err(format!("{tag:?} is not a valid element name"));
                }
                BuilderNodeKind::Element { tag }
            }
            "component" => {
                let library = obj.get("library").and_then(Value::as_str).unwrap_or("");
                if !is_valid_library_name(library) || library == "builtin" {
                    return Err(format!(
                        "{library:?} is not a component library (an instance names a \
                         non-builtin library)"
                    ));
                }
                if tag.is_empty()
                    || !tag
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    return Err(format!("{tag:?} is not a valid component name"));
                }
                BuilderNodeKind::Component {
                    library: library.to_string(),
                    name: tag.to_string(),
                }
            }
            other => {
                return Err(format!(
                    "unknown node kind {other:?} (element, text or component)"
                ))
            }
        };

        let mut node = Self {
            uid: 0,
            kind,
            attrs,
            children: Vec::new(),
        };
        let children: &[Value] = match obj.get("children") {
            None | Some(Value::Null) => &[],
            Some(Value::Array(cs)) => cs.as_slice(),
            Some(_) => return Err("`children` must be an array".to_string()),
        };
        if !children.is_empty() {
            let leaf = match &node.kind {
                BuilderNodeKind::Element { tag } if is_void(tag) => {
                    Some(format!("a <{tag}> is a void element and cannot have children"))
                }
                BuilderNodeKind::Element { .. } => None,
                BuilderNodeKind::Text { .. } => {
                    Some("a text node cannot have children".to_string())
                }
                BuilderNodeKind::Component { library, name } => Some(format!(
                    "an instance of {library}:{name} cannot have children (its content comes \
                     from the component)"
                )),
            };
            if let Some(why) = leaf {
                return Err(why);
            }
            for c in children {
                let child = Self::from_json_at(c, depth + 1)?;
                if let Some(why) = why_not_inside(&node.kind, &child.kind) {
                    return Err(why);
                }
                node.children.push(child);
            }
        }
        Ok(node)
    }
}

// ===========================================================================
// The document
// ===========================================================================

/// The tree the builder edits, with its undo history.
#[derive(Debug, Clone)]
pub struct BuilderDocument {
    /// Always an element `body` with uid [`ROOT_UID`].
    pub root: BuilderNode,
    /// The document's own stylesheet (B5): mounted after the component CSS
    /// and the project's stylesheets, saved with the document, exported as
    /// the app's stylesheet. Part of every undo step.
    pub stylesheet: String,
    /// Never reused, not even by undo: a uid names one node forever.
    next_uid: u64,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

/// One undo step: everything an edit can change.
#[derive(Debug, Clone)]
struct Snapshot {
    root: BuilderNode,
    stylesheet: String,
}

/// The `format` of a saved builder document (`document.json`,
/// `builder_save_document`).
pub const DOCUMENT_FORMAT: &str = "azul-builder-document";

/// Largest document stylesheet `builder_set_stylesheet` takes.
const MAX_STYLESHEET: usize = 4 * 1024 * 1024;

impl Default for BuilderDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl BuilderDocument {
    /// An empty `<body>`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            root: BuilderNode::element(ROOT_UID, "body"),
            stylesheet: String::new(),
            next_uid: ROOT_UID + 1,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// A document holding what the window shows right now, so the first edit
    /// does not wipe an app's UI. uids are assigned in DFS order from the
    /// `<body>` (uid 0), so importing an unchanged DOM twice yields the same
    /// uids — the UI may address a node it saw in an earlier, not-yet-stored
    /// import. Engine-internal subtrees (`__azul-*` classes: widget internals,
    /// the client-side titlebar) are left out.
    #[must_use]
    pub fn from_styled_dom(live: Option<&StyledDom>) -> Self {
        let mut doc = Self::new();
        let Some(sd) = live else {
            return doc;
        };
        let node_data = sd.node_data.as_ref();
        if node_data.is_empty() {
            return doc;
        }
        let body = node_data
            .iter()
            .position(|nd| matches!(nd.get_node_type(), NodeType::Body));
        let mut uids = ROOT_UID;
        match body {
            Some(b) => {
                if let Some(mut root) = node_from_styled(sd, NodeId::new(b), &mut uids, true, 0) {
                    root.uid = ROOT_UID;
                    root.kind = BuilderNodeKind::Element {
                        tag: "body".to_string(),
                    };
                    doc.root = root;
                }
            }
            None => {
                uids = ROOT_UID + 1;
                if let Some(n) = node_from_styled(sd, NodeId::new(0), &mut uids, true, 0) {
                    doc.root.children.push(n);
                }
            }
        }
        doc.next_uid = uids.max(ROOT_UID + 1);
        doc
    }

    /// A document holding `root` — a tree read back from a project file (B4).
    /// A root that is not a `<body>` is wrapped in one. uids are assigned in
    /// DFS order from the root (uid 0), like [`BuilderDocument::from_styled_dom`];
    /// the history starts empty (opening a file is not an edit).
    #[must_use]
    pub fn from_root(root: BuilderNode) -> Self {
        let is_body = matches!(&root.kind, BuilderNodeKind::Element { tag } if tag == "body");
        let mut root = if is_body {
            root
        } else {
            let mut body = BuilderNode::element(ROOT_UID, "body");
            body.children.push(root);
            body
        };
        fn number(node: &mut BuilderNode, next: &mut u64) {
            node.uid = *next;
            *next += 1;
            for c in &mut node.children {
                number(c, next);
            }
        }
        let mut next = ROOT_UID;
        number(&mut root, &mut next);
        Self {
            root,
            stylesheet: String::new(),
            next_uid: next.max(ROOT_UID + 1),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// The document as a file (`document.json`, `builder_save_document`):
    /// `{format, version, root, stylesheet}`, the tree without the session's
    /// uids.
    #[must_use]
    pub fn to_file_json(&self) -> serde_json::Value {
        fn strip(v: &mut serde_json::Value) {
            if let serde_json::Value::Object(m) = v {
                m.remove("uid");
                if let Some(serde_json::Value::Array(cs)) = m.get_mut("children") {
                    for c in cs {
                        strip(c);
                    }
                }
            }
        }
        let mut tree = self.root.to_json();
        strip(&mut tree);
        serde_json::json!({
            "format": DOCUMENT_FORMAT,
            "version": 1,
            "root": tree,
            "stylesheet": self.stylesheet,
        })
    }

    /// The inverse of [`BuilderDocument::to_file_json`]: a fresh document
    /// (DFS uids, an empty history). Also accepted: a file without
    /// `stylesheet` (written before B5) and a bare node tree.
    ///
    /// # Errors
    /// Another `format`, a `stylesheet` that is not a string, or a tree
    /// [`BuilderNode::from_json`] refuses.
    pub fn from_file_json(v: &serde_json::Value) -> Result<Self, String> {
        if let Some(f) = v.get("format").and_then(serde_json::Value::as_str) {
            if f != DOCUMENT_FORMAT {
                return Err(format!("the format is {f:?}, not {DOCUMENT_FORMAT:?}"));
            }
        }
        let stylesheet = match v.get("root").and(v.get("stylesheet")) {
            None | Some(serde_json::Value::Null) => String::new(),
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(_) => return Err("`stylesheet` must be a string".to_string()),
        };
        let root = v.get("root").unwrap_or(v);
        let mut doc = Self::from_root(BuilderNode::from_json(root)?);
        doc.stylesheet = stylesheet;
        Ok(doc)
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Record the state before an edit. Called only AFTER an edit validated,
    /// so a refused edit leaves no empty undo step behind.
    fn checkpoint(&mut self) {
        let snap = self.snapshot();
        self.undo.push(snap);
        if self.undo.len() > MAX_UNDO {
            let excess = self.undo.len() - MAX_UNDO;
            self.undo.drain(..excess);
        }
        self.redo.clear();
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            root: self.root.clone(),
            stylesheet: self.stylesheet.clone(),
        }
    }

    /// Put `snap` in place; answers what it replaced.
    fn restore(&mut self, snap: Snapshot) -> Snapshot {
        Snapshot {
            root: core::mem::replace(&mut self.root, snap.root),
            stylesheet: core::mem::replace(&mut self.stylesheet, snap.stylesheet),
        }
    }

    fn alloc_uid(&mut self) -> u64 {
        let uid = self.next_uid;
        self.next_uid += 1;
        uid
    }

    /// Step back one edit.
    ///
    /// # Errors
    /// There is nothing to undo.
    pub fn undo(&mut self) -> Result<(), String> {
        let prev = self.undo.pop().ok_or("nothing to undo")?;
        let current = self.restore(prev);
        self.redo.push(current);
        Ok(())
    }

    /// Re-apply the edit the last undo took back.
    ///
    /// # Errors
    /// There is nothing to redo.
    pub fn redo(&mut self) -> Result<(), String> {
        let next = self.redo.pop().ok_or("nothing to redo")?;
        let current = self.restore(next);
        self.undo.push(current);
        Ok(())
    }

    /// Replace the document's own stylesheet: one undo step; the same text
    /// again is no step at all.
    ///
    /// # Errors
    /// A stylesheet larger than 4 MiB.
    pub fn set_stylesheet(&mut self, css: &str) -> Result<(), String> {
        if css.len() > MAX_STYLESHEET {
            return Err(format!(
                "the stylesheet is {} bytes; the limit is {MAX_STYLESHEET}",
                css.len()
            ));
        }
        if self.stylesheet != css {
            self.checkpoint();
            self.stylesheet = css.to_string();
        }
        Ok(())
    }

    /// The child-index path from the root to `uid`.
    fn path(&self, uid: u64) -> Result<Vec<usize>, String> {
        fn walk(node: &BuilderNode, uid: u64, path: &mut Vec<usize>) -> bool {
            if node.uid == uid {
                return true;
            }
            for (i, c) in node.children.iter().enumerate() {
                path.push(i);
                if walk(c, uid, path) {
                    return true;
                }
                path.pop();
            }
            false
        }
        let mut path = Vec::new();
        if walk(&self.root, uid, &mut path) {
            Ok(path)
        } else {
            Err(format!("no node with uid {uid} in the builder document"))
        }
    }

    fn at(&self, path: &[usize]) -> &BuilderNode {
        let mut n = &self.root;
        for &i in path {
            n = &n.children[i];
        }
        n
    }

    fn at_mut(&mut self, path: &[usize]) -> &mut BuilderNode {
        let mut n = &mut self.root;
        for &i in path {
            n = &mut n.children[i];
        }
        n
    }

    /// The node with this uid.
    ///
    /// # Errors
    /// No such node.
    pub fn node(&self, uid: u64) -> Result<&BuilderNode, String> {
        let path = self.path(uid)?;
        Ok(self.at(&path))
    }

    /// Insert a new node as child `index` of `parent` (`None` = append).
    /// Returns the new node's uid.
    ///
    /// # Errors
    /// Unknown parent, a parent that takes no children, or an index past the end.
    pub fn insert(
        &mut self,
        parent: u64,
        index: Option<usize>,
        kind: BuilderNodeKind,
        attrs: BTreeMap<String, String>,
    ) -> Result<u64, String> {
        let ppath = self.path(parent)?;
        let len = {
            let p = self.at(&ppath);
            if let Some(why) = p.why_not_a_container() {
                return Err(why);
            }
            if let Some(why) = why_not_inside(&p.kind, &kind) {
                return Err(why);
            }
            p.children.len()
        };
        let at = match index {
            None => len,
            Some(i) if i <= len => i,
            Some(i) => {
                return Err(format!(
                    "index {i} is past the end of node {parent}'s {len} children"
                ))
            }
        };
        self.checkpoint();
        let uid = self.alloc_uid();
        self.at_mut(&ppath).children.insert(
            at,
            BuilderNode {
                uid,
                kind,
                attrs,
                children: Vec::new(),
            },
        );
        Ok(uid)
    }

    /// Move `uid` to child slot `index` of `parent` (`None` = append).
    ///
    /// `index` is the slot among the parent's children AS THEY ARE BEFORE
    /// THE MOVE — what a drop indicator between two rows points at. Moving a
    /// node to a later slot of its own parent therefore lands it one lower
    /// than `index` once it has left its old slot.
    ///
    /// # Errors
    /// Moving the root, moving a node into itself or its own subtree, an
    /// unknown node, a parent that takes no children, an index past the end.
    pub fn move_node(&mut self, uid: u64, parent: u64, index: Option<usize>) -> Result<(), String> {
        if uid == ROOT_UID {
            return Err("the document root cannot be moved".to_string());
        }
        let npath = self.path(uid)?;
        let ppath = self.path(parent)?;
        if ppath.starts_with(&npath) {
            return Err(format!(
                "cannot move node {uid} into node {parent}: that is the node itself or one of its \
                 own descendants"
            ));
        }
        let plen = {
            let p = self.at(&ppath);
            if let Some(why) = p.why_not_a_container() {
                return Err(why);
            }
            if let Some(why) = why_not_inside(&p.kind, &self.at(&npath).kind) {
                return Err(why);
            }
            p.children.len()
        };
        if let Some(i) = index {
            if i > plen {
                return Err(format!(
                    "index {i} is past the end of node {parent}'s {plen} children"
                ));
            }
        }
        let Some((&old_index, old_parent_path)) = npath.split_last() else {
            return Err("the document root cannot be moved".to_string());
        };
        let same_parent = self.at(old_parent_path).uid == parent;

        self.checkpoint();
        let node = self.at_mut(old_parent_path).children.remove(old_index);

        // Removing the node shifts its later siblings up by one; if the new
        // parent's path runs through one of them, shift the path with it.
        let mut target_path = ppath;
        let depth = old_parent_path.len();
        if target_path.len() > depth
            && target_path[..depth] == old_parent_path[..]
            && target_path[depth] > old_index
        {
            target_path[depth] -= 1;
        }

        let mut at = index.unwrap_or(usize::MAX);
        if same_parent && index.is_some() && old_index < at {
            at -= 1;
        }
        let target = self.at_mut(&target_path);
        let at = at.min(target.children.len());
        target.children.insert(at, node);
        Ok(())
    }

    /// Remove `uid` and its subtree.
    ///
    /// # Errors
    /// The root, or an unknown node.
    pub fn delete(&mut self, uid: u64) -> Result<(), String> {
        if uid == ROOT_UID {
            return Err("the document root cannot be deleted".to_string());
        }
        let path = self.path(uid)?;
        let Some((&last, parent)) = path.split_last() else {
            return Err("the document root cannot be deleted".to_string());
        };
        self.checkpoint();
        self.at_mut(parent).children.remove(last);
        Ok(())
    }

    /// Set (`Some`) or remove (`None`) an attribute. On a text node the only
    /// attribute is `text`, its content.
    ///
    /// # Errors
    /// Unknown node, an invalid attribute name, or a non-`text` attribute on
    /// a text node.
    pub fn set_attribute(
        &mut self,
        uid: u64,
        name: &str,
        value: Option<String>,
    ) -> Result<(), String> {
        if !is_valid_attr_name(name) {
            return Err(format!("{name:?} is not a valid attribute name"));
        }
        let path = self.path(uid)?;
        if matches!(self.at(&path).kind, BuilderNodeKind::Text { .. }) && name != "text" {
            return Err(format!(
                "node {uid} is a text node; its only attribute is `text`, not `{name}`"
            ));
        }
        self.checkpoint();
        let node = self.at_mut(&path);
        if let BuilderNodeKind::Text { text } = &mut node.kind {
            *text = value.unwrap_or_default();
        } else {
            match value {
                Some(v) => {
                    node.attrs.insert(name.to_string(), v);
                }
                None => {
                    node.attrs.remove(name);
                }
            }
        }
        Ok(())
    }

    /// Copy `uid` and its subtree right after it (B5). The copy's nodes take
    /// fresh uids, in DFS order. Returns the copy's uid.
    ///
    /// # Errors
    /// The root, or an unknown node.
    pub fn duplicate(&mut self, uid: u64) -> Result<u64, String> {
        if uid == ROOT_UID {
            return Err("the document root cannot be duplicated".to_string());
        }
        let path = self.path(uid)?;
        let Some((&last, parent)) = path.split_last() else {
            return Err("the document root cannot be duplicated".to_string());
        };
        let mut copy = self.at(&path).clone();
        self.checkpoint();
        self.renumber(&mut copy);
        let new_uid = copy.uid;
        self.at_mut(parent).children.insert(last + 1, copy);
        Ok(new_uid)
    }

    /// Give `node` and its subtree fresh uids, in DFS order.
    fn renumber(&mut self, node: &mut BuilderNode) {
        node.uid = self.alloc_uid();
        for c in &mut node.children {
            self.renumber(c);
        }
    }

    /// Replace the whole document - tree and stylesheet - by `loaded` (a
    /// file read back, `builder_load_document`): ONE undo step, unlike a
    /// project load, which starts a history. The new nodes take fresh uids.
    ///
    /// # Errors
    /// None today; `Result` for [`BuilderSession::edit`].
    pub fn replace_with(&mut self, loaded: BuilderDocument) -> Result<(), String> {
        let BuilderDocument {
            mut root,
            stylesheet,
            ..
        } = loaded;
        self.checkpoint();
        for c in &mut root.children {
            self.renumber(c);
        }
        root.uid = ROOT_UID;
        self.root = root;
        self.stylesheet = stylesheet;
        Ok(())
    }

    /// Replace the subtree at `uid` by an instance of `library:name`, keeping
    /// the uid (the UI's selection stays on it).
    ///
    /// # Errors
    /// The root, or an unknown node.
    pub fn replace_with_instance(
        &mut self,
        uid: u64,
        library: &str,
        name: &str,
        attrs: BTreeMap<String, String>,
    ) -> Result<(), String> {
        if uid == ROOT_UID {
            return Err("the document root cannot become a component".to_string());
        }
        let path = self.path(uid)?;
        self.checkpoint();
        let node = self.at_mut(&path);
        node.kind = BuilderNodeKind::Component {
            library: library.to_string(),
            name: name.to_string(),
        };
        node.attrs = attrs;
        node.children.clear();
        Ok(())
    }

    /// The document as the builder UI reads it.
    #[must_use]
    pub fn to_json(&self, active: bool) -> serde_json::Value {
        serde_json::json!({
            "active": active,
            "can_undo": self.can_undo(),
            "can_redo": self.can_redo(),
            "root": self.root.to_json(),
            "stylesheet": self.stylesheet,
        })
    }

    /// The whole document as the XML the window mounts, with every component
    /// instance expanded and every component's CSS in the `<style>`.
    #[must_use]
    pub fn to_mount_xml(&self, map: &ComponentMap) -> String {
        self.to_mount_xml_with(map, "")
    }

    /// [`BuilderDocument::to_mount_xml`] plus the project's stylesheet (B4),
    /// written AFTER the component CSS so the project's rules win on equal
    /// specificity, as an app stylesheet does.
    ///
    /// The document's own stylesheet (B5) comes last, in a `<style>` of its
    /// own. Every `<head><style>` becomes the stylesheet the parser hangs on
    /// the document ROOT (`str_to_dom_unstyled`: `Dom.css` of `<html>`, what
    /// `with_component_css` does), so the sheet is part of what is mounted
    /// and survives every remount - unlike the Inspector's
    /// `set_node_css_override`, which edits the live node. It is not a
    /// `<style>` inside `<body>`: that would be an INNER sheet, which the
    /// cascade ranks below the outer one (`collect_css_from_dom`), and a
    /// component's CSS would beat the document's on equal specificity.
    #[must_use]
    pub fn to_mount_xml_with(&self, map: &ComponentMap, stylesheet: &str) -> String {
        let mut w = XmlWriter::new(map);
        let mut body = String::new();
        w.write_node(&mut body, &self.root, true, 0);
        let mut css = w.css;
        if !stylesheet.trim().is_empty() {
            css.push_str(stylesheet);
            css.push('\n');
        }
        let own = if self.stylesheet.trim().is_empty() {
            String::new()
        } else {
            format!("<style>{}</style>", escape_xml(&self.stylesheet))
        };
        format!(
            "<html><head><style>{}</style>{own}</head>{body}</html>",
            escape_xml(&css)
        )
    }
}

// ===========================================================================
// The per-window session (lives in `E2eScratch`)
// ===========================================================================

/// What a builder op asks the window to do after it answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remount {
    /// Nothing changed on screen.
    Keep,
    /// Mount this XML document.
    Mount(String),
    /// Give the window back to the app's own `layout()`.
    Unmount,
}

/// A builder op's answer: the JSON for the UI and the remount for the window.
#[derive(Debug, Clone)]
pub struct BuilderReply {
    pub json: serde_json::Value,
    pub remount: Remount,
}

#[derive(Debug, Clone)]
struct Thumbnail {
    /// `None`: the component renders nothing visible.
    data: Option<String>,
    width: f32,
    height: f32,
    /// A builtin element with nothing to show on its own: why
    /// (`azul_core::xml::builtin_no_visual`); the card says "no visual".
    no_visual: Option<&'static str>,
}

/// This window's builder state: the document once the builder has taken the
/// window over, and the palette thumbnail cache.
#[derive(Debug, Default)]
pub struct BuilderSession {
    doc: Option<BuilderDocument>,
    thumbnails: BTreeMap<u64, Thumbnail>,
    /// The open project's stylesheets (`styles/**/*.css`, concatenated in
    /// path order), mounted after the component CSS (B4, project.rs).
    stylesheet: String,
}

impl BuilderSession {
    /// Whether the builder has taken over the window.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.doc.is_some()
    }

    /// The project stylesheet (`styles/**.css`, B4) the document is mounted
    /// with — not the document's own ([`BuilderDocument::stylesheet`]).
    #[must_use]
    pub fn project_stylesheet(&self) -> &str {
        &self.stylesheet
    }

    /// Replace the project stylesheet. Answers the remount that shows it:
    /// the document again if the builder has the window, else nothing (the
    /// next edit mounts it).
    pub fn set_project_stylesheet(&mut self, map: &ComponentMap, css: String) -> Remount {
        if self.stylesheet == css {
            return Remount::Keep;
        }
        self.stylesheet = css;
        match &self.doc {
            Some(d) => Remount::Mount(d.to_mount_xml_with(map, &self.stylesheet)),
            None => Remount::Keep,
        }
    }

    /// Take the window over with a document read back from a file (a
    /// project's `document.json`): its history starts empty, the window
    /// shows it at once.
    pub fn load_document(&mut self, map: &ComponentMap, doc: BuilderDocument) -> BuilderReply {
        let reply = BuilderReply {
            json: doc.to_json(true),
            remount: Remount::Mount(doc.to_mount_xml_with(map, &self.stylesheet)),
        };
        self.doc = Some(doc);
        reply
    }

    /// The document — or, before the first edit, what the first edit would
    /// start from (`active: false`, not stored).
    #[must_use]
    pub fn document_json(&self, live: Option<&StyledDom>) -> serde_json::Value {
        match &self.doc {
            Some(d) => d.to_json(true),
            None => BuilderDocument::from_styled_dom(live).to_json(false),
        }
    }

    /// The mount XML for the current document, if the builder is active —
    /// for re-mounting after a component the document uses changed.
    #[must_use]
    pub fn remount_xml(&self, map: &ComponentMap) -> Option<String> {
        self.doc
            .as_ref()
            .map(|d| d.to_mount_xml_with(map, &self.stylesheet))
    }

    /// The document the code export reads: the builder's, or — before the
    /// first edit — what the window shows (imported, not stored).
    #[must_use]
    pub fn export_document(&self, live: Option<&StyledDom>) -> std::borrow::Cow<'_, BuilderDocument> {
        match &self.doc {
            Some(d) => std::borrow::Cow::Borrowed(d),
            None => std::borrow::Cow::Owned(BuilderDocument::from_styled_dom(live)),
        }
    }

    /// Run one edit. The first edit imports the live DOM; an edit that fails
    /// leaves the session exactly as it was (a failed FIRST edit does not
    /// take the window over).
    fn edit<R>(
        &mut self,
        live: Option<&StyledDom>,
        map: &ComponentMap,
        f: impl FnOnce(&mut BuilderDocument) -> Result<R, String>,
    ) -> Result<(R, BuilderReply), String> {
        let was_active = self.doc.is_some();
        let mut doc = match self.doc.take() {
            Some(d) => d,
            None => BuilderDocument::from_styled_dom(live),
        };
        match f(&mut doc) {
            Ok(r) => {
                let reply = BuilderReply {
                    json: doc.to_json(true),
                    remount: Remount::Mount(doc.to_mount_xml_with(map, &self.stylesheet)),
                };
                self.doc = Some(doc);
                Ok((r, reply))
            }
            Err(e) => {
                if was_active {
                    self.doc = Some(doc);
                }
                Err(e)
            }
        }
    }

    /// `builder_insert`: a palette drop.
    ///
    /// # Errors
    /// See [`BuilderDocument::insert`]; also an unknown component, an invalid
    /// tag or attribute name.
    #[allow(clippy::too_many_arguments)]
    pub fn insert(
        &mut self,
        live: Option<&StyledDom>,
        map: &ComponentMap,
        parent: u64,
        index: Option<usize>,
        library: Option<&str>,
        component: &str,
        attrs: BTreeMap<String, String>,
    ) -> Result<BuilderReply, String> {
        let (kind, attrs) = resolve_insert(map, library, component, attrs)?;
        let (uid, mut reply) =
            self.edit(live, map, |doc| doc.insert(parent, index, kind, attrs))?;
        if let Some(obj) = reply.json.as_object_mut() {
            obj.insert("inserted".into(), serde_json::json!(uid));
        }
        Ok(reply)
    }

    /// `builder_move`: a tree row dragged onto another position.
    ///
    /// # Errors
    /// See [`BuilderDocument::move_node`].
    pub fn move_node(
        &mut self,
        live: Option<&StyledDom>,
        map: &ComponentMap,
        node: u64,
        parent: u64,
        index: Option<usize>,
    ) -> Result<BuilderReply, String> {
        Ok(self
            .edit(live, map, |doc| doc.move_node(node, parent, index))?
            .1)
    }

    /// `builder_delete`.
    ///
    /// # Errors
    /// See [`BuilderDocument::delete`].
    pub fn delete(
        &mut self,
        live: Option<&StyledDom>,
        map: &ComponentMap,
        node: u64,
    ) -> Result<BuilderReply, String> {
        Ok(self.edit(live, map, |doc| doc.delete(node))?.1)
    }

    /// `builder_set_attribute`.
    ///
    /// # Errors
    /// See [`BuilderDocument::set_attribute`].
    pub fn set_attribute(
        &mut self,
        live: Option<&StyledDom>,
        map: &ComponentMap,
        node: u64,
        name: &str,
        value: Option<String>,
    ) -> Result<BuilderReply, String> {
        Ok(self
            .edit(live, map, |doc| doc.set_attribute(node, name, value))?
            .1)
    }

    /// `builder_set_stylesheet`: the document's own stylesheet (one undo
    /// step; like every edit, the first one takes the window over).
    ///
    /// # Errors
    /// See [`BuilderDocument::set_stylesheet`].
    pub fn set_document_stylesheet(
        &mut self,
        live: Option<&StyledDom>,
        map: &ComponentMap,
        css: &str,
    ) -> Result<BuilderReply, String> {
        Ok(self.edit(live, map, |doc| doc.set_stylesheet(css))?.1)
    }

    /// `builder_duplicate`: answers the document plus `inserted` (the
    /// copy's uid, which the UI selects).
    ///
    /// # Errors
    /// See [`BuilderDocument::duplicate`].
    pub fn duplicate(
        &mut self,
        live: Option<&StyledDom>,
        map: &ComponentMap,
        node: u64,
    ) -> Result<BuilderReply, String> {
        let (uid, mut reply) = self.edit(live, map, |doc| doc.duplicate(node))?;
        if let Some(obj) = reply.json.as_object_mut() {
            obj.insert("inserted".into(), serde_json::json!(uid));
        }
        Ok(reply)
    }

    /// `builder_load_document`: `loaded` replaces the document as one edit.
    ///
    /// # Errors
    /// See [`BuilderDocument::replace_with`].
    pub fn replace_document(
        &mut self,
        live: Option<&StyledDom>,
        map: &ComponentMap,
        loaded: BuilderDocument,
    ) -> Result<BuilderReply, String> {
        Ok(self.edit(live, map, |doc| doc.replace_with(loaded))?.1)
    }

    /// `builder_undo`.
    ///
    /// # Errors
    /// Nothing to undo.
    pub fn undo(&mut self, map: &ComponentMap) -> Result<BuilderReply, String> {
        Ok(self.edit(None, map, BuilderDocument::undo)?.1)
    }

    /// `builder_redo`.
    ///
    /// # Errors
    /// Nothing to redo.
    pub fn redo(&mut self, map: &ComponentMap) -> Result<BuilderReply, String> {
        Ok(self.edit(None, map, BuilderDocument::redo)?.1)
    }

    /// `builder_reset`: drop the document and give the window back to the app.
    pub fn reset(&mut self) -> BuilderReply {
        let was_active = self.doc.take().is_some();
        BuilderReply {
            json: serde_json::json!({ "active": false, "was_active": was_active }),
            remount: if was_active {
                Remount::Unmount
            } else {
                Remount::Keep
            },
        }
    }

    /// `builder_convert_to_component`: register the subtree at `node` as a new
    /// template component `library:name` (the library is created if it does
    /// not exist) and, if `replace`, put an instance of it where the subtree
    /// was.
    ///
    /// # Errors
    /// Invalid names, the root / a text node / an instance as the subtree, a
    /// library that is not modifiable, a component that already exists.
    #[allow(clippy::too_many_arguments)]
    pub fn convert_to_component(
        &mut self,
        live: Option<&StyledDom>,
        map: &mut ComponentMap,
        node: u64,
        library: &str,
        name: &str,
        display_name: Option<&str>,
        replace: bool,
    ) -> Result<BuilderReply, String> {
        if !is_valid_library_name(library) {
            return Err(format!(
                "{library:?} is not a valid library name (letters, digits, '-' and '_', starting \
                 with a letter)"
            ));
        }
        if library == "builtin" {
            return Err("the builtin library is not modifiable".to_string());
        }
        if !is_valid_component_name(name) {
            return Err(format!(
                "{name:?} is not a valid component name (lowercase letters, digits, '-' and '_', \
                 starting with a letter)"
            ));
        }
        if node == ROOT_UID {
            return Err("the document root cannot become a component".to_string());
        }

        let subtree = {
            let imported;
            let doc = match &self.doc {
                Some(d) => d,
                None => {
                    imported = BuilderDocument::from_styled_dom(live);
                    &imported
                }
            };
            doc.node(node)?.clone()
        };
        let tag = match &subtree.kind {
            BuilderNodeKind::Element { tag } => tag.clone(),
            BuilderNodeKind::Text { .. } => {
                return Err(format!(
                    "node {node} is a text node; convert the element that contains it"
                ))
            }
            BuilderNodeKind::Component { library, name } => {
                return Err(format!(
                    "node {node} already is an instance of {library}:{name}"
                ))
            }
        };

        let (template, params) = infer_template(&subtree);
        let display = display_name
            .map(str::to_string)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| title_case(name));
        let def = template_component_def(
            library,
            name,
            &display,
            &format!("Converted from a <{tag}> subtree in AzBuilder"),
            "",
            &template,
            params.iter().map(InferredParam::to_field).collect(),
        );
        add_component(map, library, def)?;

        let component = serde_json::json!({
            "library": library,
            "name": name,
            "display_name": display,
            "template": template,
            "fields": params.iter().map(|p| serde_json::json!({
                "name": p.name, "default": p.default, "description": p.description,
            })).collect::<Vec<_>>(),
        });

        let mut reply = if replace {
            // No arguments: the instance takes the component's defaults, which
            // ARE the subtree's values — it renders exactly as the subtree did.
            self.edit(live, map, |doc| {
                doc.replace_with_instance(node, library, name, BTreeMap::new())
            })?
            .1
        } else {
            BuilderReply {
                json: self.document_json(live),
                remount: Remount::Keep,
            }
        };
        if let Some(obj) = reply.json.as_object_mut() {
            obj.insert("component".into(), component);
        }
        Ok(reply)
    }

    /// `get_component_thumbnail`: the component's default instance rendered by
    /// the CPU renderer ([`preview_styled_dom`]), cached until the component
    /// changes (the cache key is a fingerprint of its CSS, template, data
    /// model and render fn, and the mode). A builtin element with nothing to
    /// show on its own is not rendered: it answers `empty` and its
    /// `no_visual` reason.
    ///
    /// `dark` renders it in dark mode ([`preview_in_dark_mode`]), for a page
    /// that shows its palette in dark mode; otherwise it renders light, on
    /// white.
    ///
    /// # Errors
    /// Unknown component, a failing `render_fn`, a failing render.
    #[allow(clippy::too_many_arguments)] // one per field of the op
    pub fn thumbnail(
        &mut self,
        callback_info: &crate::callbacks::CallbackInfo,
        map: &ComponentMap,
        library: &str,
        name: &str,
        width: Option<f32>,
        dpi: Option<f32>,
        dark: bool,
    ) -> Result<serde_json::Value, String> {
        let def = map
            .get(library, name)
            .ok_or_else(|| format!("Component '{name}' not found in library '{library}'"))?;
        let width = width.unwrap_or(160.0).clamp(16.0, 1024.0);
        let dpi = dpi.unwrap_or(2.0).clamp(0.5, 4.0);
        let key = thumbnail_key(def, library, width, dpi, dark);

        if let Some(why) = (library == "builtin")
            .then(|| azul_core::xml::builtin_no_visual(name))
            .flatten()
        {
            let none = Thumbnail {
                data: None,
                width: 0.0,
                height: 0.0,
                no_visual: Some(why),
            };
            return Ok(thumbnail_json(library, name, key, &none, false));
        }
        if let Some(t) = self.thumbnails.get(&key) {
            return Ok(thumbnail_json(library, name, key, t, true));
        }

        let mut styled = preview_styled_dom(callback_info, library, def, &def.data_model, map)?;
        let background_color = if dark {
            preview_in_dark_mode(&mut styled, callback_info)
        } else {
            azul_css::props::basic::color::ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            }
        };
        // Same as `get_component_preview`: a builtin's render_fn styles with
        // no CSS, so the component CSS goes on afterwards (cascaded under the
        // mode's context set above).
        if !def.css.as_str().trim().is_empty() {
            styled.restyle(Css::from_string(def.css.clone()));
        }
        let opts = crate::cpurender::ComponentPreviewOptions {
            width: Some(width),
            height: None,
            dpi_factor: dpi,
            background_color,
        };
        let result = crate::cpurender::render_component_preview(
            &styled,
            &callback_info.get_layout_window().font_manager,
            opts,
            Some(callback_info.get_system_style()),
        )?;
        let thumb = if result.png_data.is_empty() {
            Thumbnail {
                data: None,
                width: 0.0,
                height: 0.0,
                no_visual: None,
            }
        } else {
            Thumbnail {
                data: Some(format!(
                    "data:image/png;base64,{}",
                    crate::callbacks::base64_encode(&result.png_data)
                )),
                width: result.content_width,
                height: result.content_height,
                no_visual: None,
            }
        };
        if self.thumbnails.len() >= MAX_THUMBNAILS {
            self.thumbnails.clear();
        }
        self.thumbnails.insert(key, thumb.clone());
        Ok(thumbnail_json(library, name, key, &thumb, false))
    }
}

fn thumbnail_json(
    library: &str,
    name: &str,
    key: u64,
    t: &Thumbnail,
    cached: bool,
) -> serde_json::Value {
    serde_json::json!({
        "library": library,
        "name": name,
        "key": format!("{key:016x}"),
        "data": t.data,
        "empty": t.data.is_none(),
        "width": t.width,
        "height": t.height,
        "cached": cached,
        "no_visual": t.no_visual,
    })
}

/// A component's default look as every preview shows it - the palette
/// thumbnail and `get_component_preview`: a builtin HTML element through its
/// configured preview (`azul_core::xml::builtin_preview_dom`: its text and
/// example), styled like a document the builder mounts - raw form controls
/// as their widgets, icons resolved (`crate::xml::style_detached_dom`); any
/// other component through its `render_fn`.
///
/// # Errors
/// A failing `render_fn`.
pub fn preview_styled_dom(
    callback_info: &crate::callbacks::CallbackInfo,
    library: &str,
    def: &ComponentDef,
    data_model: &ComponentDataModel,
    map: &ComponentMap,
) -> Result<StyledDom, String> {
    if library == "builtin" && def.codegen == ComponentCodegen::Element {
        let dom = azul_core::xml::builtin_preview_dom(def.id.name.as_str(), data_model);
        let system_style = callback_info.get_system_style();
        return Ok(crate::xml::style_detached_dom(
            dom,
            callback_info.get_layout_window().icon_provider.as_ref(),
            &system_style,
        ));
    }
    match (def.render_fn)(def, data_model, map) {
        ResultStyledDomRenderDomError::Ok(sd) => Ok(sd),
        ResultStyledDomRenderDomError::Err(e) => Err(format!(
            "render_fn failed for '{library}:{}': {e:?}",
            def.id.name.as_str()
        )),
    }
}

/// Cascade a detached preview in dark mode, the way the window cascades its
/// own DOM when it is dark: the window's context (OS, language, app theme)
/// with the mode dark and the `system:` palette of dark mode
/// (`SystemStyle::colors_for_theme`). Without it a preview has no context at
/// all: light, and no conditional rule (`prefers-color-scheme: dark`,
/// a widget's dark twin) applies.
///
/// Returns the background to render it on: the dark content background
/// (`system:background`), as white is the light one.
fn preview_in_dark_mode(
    styled: &mut StyledDom,
    callback_info: &crate::callbacks::CallbackInfo,
) -> azul_css::props::basic::color::ColorU {
    use azul_css::{props::basic::color::SystemColorRef, system::DarkLightMode};

    let window = callback_info.get_layout_window();
    let mut context = window.dynamic_selector_context(&window.current_window_state);
    context.mode = DarkLightMode::Dark;
    context.system_colors = callback_info
        .get_system_style()
        .colors_for_theme(DarkLightMode::Dark);
    let background = SystemColorRef::Background.resolve_for_theme(&context.system_colors, true);
    styled.set_dynamic_selector_context(context);
    background
}

/// Everything that changes what a component looks like.
fn thumbnail_key(def: &ComponentDef, library: &str, width: f32, dpi: f32, dark: bool) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    library.hash(&mut h);
    def.id.name.as_str().hash(&mut h);
    def.css.as_str().hash(&mut h);
    def.render_fn_source
        .as_ref()
        .map(|s| s.as_str())
        .hash(&mut h);
    format!("{:?}", def.data_model).hash(&mut h);
    (def.render_fn as usize).hash(&mut h);
    width.to_bits().hash(&mut h);
    dpi.to_bits().hash(&mut h);
    dark.hash(&mut h);
    h.finish()
}

/// A palette drop's payload → the node to insert.
fn resolve_insert(
    map: &ComponentMap,
    library: Option<&str>,
    component: &str,
    mut attrs: BTreeMap<String, String>,
) -> Result<(BuilderNodeKind, BTreeMap<String, String>), String> {
    if let Some(bad) = attrs.keys().find(|k| !is_valid_attr_name(k)) {
        return Err(format!("{bad:?} is not a valid attribute name"));
    }
    let library = library.filter(|l| !l.is_empty()).unwrap_or("builtin");
    if component == "#text" {
        let text = attrs.remove("text").unwrap_or_default();
        return Ok((BuilderNodeKind::Text { text }, BTreeMap::new()));
    }
    if library == "builtin" {
        let tag = component.to_ascii_lowercase();
        if !is_valid_tag(&tag) {
            return Err(format!("{component:?} is not a valid element name"));
        }
        // A dropped <p> / <h1> / <button> should SHOW something: take the
        // builtin's default text, as its own preview does.
        if !attrs.contains_key("text") {
            if let Some(t) = map
                .get("builtin", &tag)
                .and_then(|def| def.data_model.get_default_string("text"))
            {
                if !t.as_str().is_empty() {
                    attrs.insert("text".to_string(), t.as_str().to_string());
                }
            }
        }
        return Ok((BuilderNodeKind::Element { tag }, attrs));
    }
    if map.get(library, component).is_none() {
        return Err(format!(
            "unknown component '{library}:{component}' (no such library, or no such component in it)"
        ));
    }
    Ok((
        BuilderNodeKind::Component {
            library: library.to_string(),
            name: component.to_string(),
        },
        attrs,
    ))
}

// ===========================================================================
// XML output (mounting + template expansion)
// ===========================================================================

struct XmlWriter<'m> {
    map: &'m ComponentMap,
    /// The CSS of every component the output uses, once each.
    css: String,
    css_seen: BTreeSet<String>,
}

impl<'m> XmlWriter<'m> {
    fn new(map: &'m ComponentMap) -> Self {
        Self {
            map,
            css: String::new(),
            css_seen: BTreeSet::new(),
        }
    }

    /// `mark`: this is a DOCUMENT node, give it its `azb-<uid>` class.
    fn write_node(&mut self, out: &mut String, node: &BuilderNode, mark: bool, depth: usize) {
        match &node.kind {
            BuilderNodeKind::Text { text } => out.push_str(&escape_xml(text)),
            BuilderNodeKind::Element { tag } => {
                write_open_tag(out, tag, &node.attrs, mark.then_some(node.uid));
                if is_void(tag) {
                    out.push_str("/>");
                    return;
                }
                out.push('>');
                if let Some(t) = node.attrs.get("text") {
                    out.push_str(&escape_xml(t));
                }
                for c in &node.children {
                    self.write_node(out, c, mark, depth);
                }
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
            }
            BuilderNodeKind::Component { library, name } => {
                self.write_instance(out, node, library, name, mark.then_some(node.uid), depth);
            }
        }
    }

    fn write_instance(
        &mut self,
        out: &mut String,
        node: &BuilderNode,
        library: &str,
        name: &str,
        marker: Option<u64>,
        depth: usize,
    ) {
        if depth >= MAX_EXPANSION_DEPTH {
            write_placeholder(
                out,
                marker,
                &format!("[{library}:{name}: components nested deeper than {MAX_EXPANSION_DEPTH}]"),
            );
            return;
        }
        // A copy of the `&'m` reference: `def` borrows the map, not `self`.
        let map: &'m ComponentMap = self.map;
        let Some(def) = map.get(library, name) else {
            write_placeholder(
                out,
                marker,
                &format!("[missing component {library}:{name}]"),
            );
            return;
        };
        if self.css_seen.insert(format!("{library}:{name}")) && !def.css.as_str().trim().is_empty()
        {
            self.css.push_str(def.css.as_str());
            self.css.push('\n');
        }

        let roots = if let Some(template) = template_of(def) {
            let args = instance_args(&def.data_model, &node.attrs);
            match parse_fragment(&substitute(template, &args)) {
                Ok(r) => r,
                Err(e) => {
                    write_placeholder(out, marker, &format!("[{library}:{name}: {e}]"));
                    return;
                }
            }
        } else {
            let dm = data_model_with_args(&def.data_model, &node.attrs);
            match (def.render_fn)(def, &dm, map) {
                ResultStyledDomRenderDomError::Ok(sd) => rendered_roots(&sd),
                ResultStyledDomRenderDomError::Err(e) => {
                    write_placeholder(
                        out,
                        marker,
                        &format!("[{library}:{name} failed to render: {e:?}]"),
                    );
                    return;
                }
            }
        };

        // `class` / `id` / `style` that are not arguments land on the root.
        let passthrough: BTreeMap<String, String> = node
            .attrs
            .iter()
            .filter(|(k, _)| {
                matches!(k.as_str(), "class" | "id" | "style")
                    && def.data_model.get_field(k).is_none()
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let root = single_root(roots, marker, &passthrough);
        self.write_node(out, &root, false, depth + 1);
    }
}

fn write_open_tag(
    out: &mut String,
    tag: &str,
    attrs: &BTreeMap<String, String>,
    mark: Option<u64>,
) {
    out.push('<');
    out.push_str(tag);
    let mut class = attrs.get("class").cloned().unwrap_or_default();
    if let Some(uid) = mark {
        if !class.is_empty() {
            class.push(' ');
        }
        class.push_str(&format!("{MARKER_PREFIX}{uid}"));
    }
    for (k, v) in attrs {
        if k == "text" || k == "class" {
            continue;
        }
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        out.push_str(&escape_xml(v));
        out.push('"');
    }
    if !class.is_empty() {
        out.push_str(" class=\"");
        out.push_str(&escape_xml(&class));
        out.push('"');
    }
}

fn write_placeholder(out: &mut String, marker: Option<u64>, message: &str) {
    let mut attrs = BTreeMap::new();
    attrs.insert("class".to_string(), "az-builder-placeholder".to_string());
    write_open_tag(out, "div", &attrs, marker);
    out.push('>');
    out.push_str(&escape_xml(message));
    out.push_str("</div>");
}

/// A component's expansion as ONE element (wrapping several roots in a
/// `<div>`), carrying the instance's marker class and pass-through attributes.
fn single_root(
    roots: Vec<BuilderNode>,
    marker: Option<u64>,
    passthrough: &BTreeMap<String, String>,
) -> BuilderNode {
    let single_element =
        roots.len() == 1 && matches!(roots[0].kind, BuilderNodeKind::Element { .. });
    let mut root = if single_element {
        roots
            .into_iter()
            .next()
            .unwrap_or_else(|| BuilderNode::element(0, "div"))
    } else {
        let mut d = BuilderNode::element(0, "div");
        d.children = roots;
        d
    };
    let mut classes: Vec<String> = Vec::new();
    if let Some(c) = root.attrs.get("class") {
        classes.push(c.clone());
    }
    if let Some(c) = passthrough.get("class") {
        classes.push(c.clone());
    }
    if let Some(uid) = marker {
        classes.push(format!("{MARKER_PREFIX}{uid}"));
    }
    if !classes.is_empty() {
        root.attrs.insert("class".to_string(), classes.join(" "));
    }
    for key in ["id", "style"] {
        if let Some(v) = passthrough.get(key) {
            root.attrs.insert(key.to_string(), v.clone());
        }
    }
    root
}

/// What a `render_fn` produced, as document nodes (for inlining).
fn rendered_roots(sd: &StyledDom) -> Vec<BuilderNode> {
    let mut uids = 0;
    node_from_styled(sd, NodeId::new(0), &mut uids, false, 0)
        .into_iter()
        .collect()
}

// ── for the code export (layout/src/e2e/export.rs) ──

/// A node and its subtree as plain XML: component instances expanded (like
/// the mount), NO `azb-<uid>` markers; plus the CSS of every component it
/// uses (the code export takes that CSS; its markup is
/// [`export_node_markup`]).
#[must_use]
pub fn export_node_xml(node: &BuilderNode, map: &ComponentMap) -> (String, String) {
    let mut w = XmlWriter::new(map);
    let mut out = String::new();
    w.write_node(&mut out, node, false, 0);
    (out, w.css)
}

/// A node and its subtree as markup for the COMPONENT-AWARE code export
/// (`azul_core::codegen::dom`): component instances stay tags
/// `<library:name field="value" ..>` (the export calls each component's
/// function instead of inlining it), no `azb-<uid>` markers.
#[must_use]
pub fn export_node_markup(node: &BuilderNode) -> String {
    let mut out = String::new();
    write_template(&mut out, node, None, "div");
    out
}

/// One live `StyledDom` node (and its subtree) as a document node. `import`:
/// leave out engine-internal subtrees and the builder's own marker classes.
fn node_from_styled(
    sd: &StyledDom,
    id: NodeId,
    uids: &mut u64,
    import: bool,
    depth: usize,
) -> Option<BuilderNode> {
    let nd = sd.node_data.as_ref().get(id.index())?;
    let mut take_uid = || {
        let u = *uids;
        *uids += 1;
        u
    };
    if let NodeType::Text(t) = nd.get_node_type() {
        let s = t.as_str();
        if s.trim().is_empty() {
            return None;
        }
        return Some(BuilderNode::text(take_uid(), s));
    }
    let mut classes: Vec<String> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    for attr in nd.attributes().as_ref() {
        if let Some(c) = attr.as_class() {
            if import && c.starts_with("__azul-") {
                return None;
            }
            if marker_uid(c).is_none() {
                classes.push(c.to_string());
            }
        } else if let Some(i) = attr.as_id() {
            ids.push(i.to_string());
        }
    }
    let tag = nd.get_node_type().get_path().to_string();
    let mut node = BuilderNode::element(take_uid(), &tag);
    if !classes.is_empty() {
        node.attrs.insert("class".to_string(), classes.join(" "));
    }
    if !ids.is_empty() {
        node.attrs.insert("id".to_string(), ids.join(" "));
    }
    if depth < MAX_TREE_DEPTH {
        let hierarchy = sd.node_hierarchy.as_ref();
        // Bounded by the node count: a corrupted sibling chain must not hang
        // the op.
        let mut budget = sd.node_data.as_ref().len();
        let mut next = hierarchy.get(id.index()).and_then(|h| h.first_child_id(id));
        while let Some(c) = next {
            if budget == 0 {
                break;
            }
            budget -= 1;
            if let Some(n) = node_from_styled(sd, c, uids, import, depth + 1) {
                node.children.push(n);
            }
            next = hierarchy.get(c.index()).and_then(|h| h.next_sibling_id());
        }
    }
    // `<p>Hello</p>` reads better as one row with text than as two rows.
    if !is_void(&tag)
        && node.children.len() == 1
        && matches!(node.children[0].kind, BuilderNodeKind::Text { .. })
    {
        if let BuilderNodeKind::Text { text } = node.children.remove(0).kind {
            node.attrs.insert("text".to_string(), text);
        }
    }
    Some(node)
}

/// Parse an XML fragment (a template, after substitution) into nodes. A
/// `prefix:name` tag (other than `svg:` / `html:` / `xhtml:`) is a component
/// instance.
fn parse_fragment(xml: &str) -> Result<Vec<BuilderNode>, String> {
    let children =
        crate::xml::parse_xml_string(xml).map_err(|e| format!("template does not parse: {e:?}"))?;
    Ok(children
        .iter()
        .filter_map(|c| node_from_xml(c, 0))
        .collect())
}

fn node_from_xml(child: &XmlNodeChild, depth: usize) -> Option<BuilderNode> {
    match child {
        XmlNodeChild::Text(t) => {
            let s = t.as_str();
            if s.trim().is_empty() {
                None
            } else {
                Some(BuilderNode::text(0, s))
            }
        }
        XmlNodeChild::Element(n) => {
            if depth > MAX_TREE_DEPTH {
                return None;
            }
            let raw = n.node_type.inner.as_str();
            let kind = match raw.split_once(':') {
                Some((lib, name))
                    if !matches!(lib.to_ascii_lowercase().as_str(), "svg" | "html" | "xhtml") =>
                {
                    BuilderNodeKind::Component {
                        library: lib.to_string(),
                        name: name.to_string(),
                    }
                }
                _ => BuilderNodeKind::Element {
                    tag: raw.to_ascii_lowercase(),
                },
            };
            let mut attrs = BTreeMap::new();
            for pair in n.attributes.inner.as_ref() {
                attrs.insert(
                    pair.key.as_str().to_string(),
                    pair.value.as_str().to_string(),
                );
            }
            let children = n
                .children
                .as_ref()
                .iter()
                .filter_map(|c| node_from_xml(c, depth + 1))
                .collect();
            Some(BuilderNode {
                uid: 0,
                kind,
                attrs,
                children,
            })
        }
    }
}

// ===========================================================================
// Template components
// ===========================================================================

/// The template of a component made by the builder, if it is one.
#[must_use]
pub fn template_of(def: &ComponentDef) -> Option<&str> {
    let src = def.render_fn_source.as_ref()?;
    src.as_str()
        .trim_start()
        .strip_prefix(TEMPLATE_MARKER)
        .map(str::trim)
}

/// `render_fn` of a template component: substitute the data model's current
/// values into the template, expand nested instances, parse, style with the
/// component CSS. Falls back to `user_defined_render_fn` for a component
/// without a template, and renders a visible error instead of failing.
#[must_use]
pub fn builder_template_render_fn(
    def: &ComponentDef,
    data: &ComponentDataModel,
    map: &ComponentMap,
) -> ResultStyledDomRenderDomError {
    let Some(template) = template_of(def) else {
        return azul_core::xml::user_defined_render_fn(def, data, map);
    };
    let depth = TEMPLATE_RENDER_DEPTH.with(|d| {
        let v = d.get();
        d.set(v + 1);
        v
    });
    let rendered = if depth >= MAX_EXPANSION_DEPTH {
        Err(format!(
            "components nested deeper than {MAX_EXPANSION_DEPTH} (a template that contains itself?)"
        ))
    } else {
        template_to_dom(template, data, map)
    };
    TEMPLATE_RENDER_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));

    let (mut dom, nested_css) = match rendered {
        Ok(v) => v,
        Err(msg) => (
            Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                format!(
                    "[{}:{}: {msg}]",
                    def.id.collection.as_str(),
                    def.id.name.as_str()
                ),
            )),
            String::new(),
        ),
    };
    let mut css_text = def.css.as_str().to_string();
    if !nested_css.is_empty() {
        css_text.push('\n');
        css_text.push_str(&nested_css);
    }
    let css = if css_text.trim().is_empty() {
        Css::empty()
    } else {
        Css::from_string(AzString::from(css_text))
    };
    ResultStyledDomRenderDomError::Ok(StyledDom::create(&mut dom, css))
}

fn template_to_dom(
    template: &str,
    data: &ComponentDataModel,
    map: &ComponentMap,
) -> Result<(Dom, String), String> {
    let args = instance_args(data, &BTreeMap::new());
    let roots = parse_fragment(&substitute(template, &args))?;
    let mut w = XmlWriter::new(map);
    let mut body = String::new();
    for r in &roots {
        w.write_node(&mut body, r, false, 0);
    }
    let doc = format!("<html><body>{body}</body></html>");
    let xml = crate::xml::parse_xml(&doc).map_err(|e| format!("{e:?}"))?;
    let html = azul_core::xml::str_to_dom_unstyled(xml.root.as_ref(), map)
        .map_err(|e| format!("{e:?}"))?;
    Ok((body_content(html), w.css))
}

/// The `<body>`'s content of a parsed `<html><body>…</body></html>`: its only
/// child, or all of them wrapped in a `<div>`.
fn body_content(html: Dom) -> Dom {
    let is_body = matches!(html.root.get_node_type(), NodeType::Body);
    let body = if is_body {
        Some(html)
    } else {
        html.children
            .as_ref()
            .iter()
            .find(|c| matches!(c.root.get_node_type(), NodeType::Body))
            .cloned()
    };
    let Some(body) = body else {
        return Dom::create_div();
    };
    let kids = body.children.as_ref();
    let mut out = if kids.len() == 1 {
        kids[0].clone()
    } else {
        Dom::create_div().with_children(body.children.clone())
    };
    let _ = out.fixup_children_estimated();
    out
}

/// Replace `{name}` by `args[name]` (XML-escaped); `{{` / `}}` are literal
/// braces; an unknown or malformed placeholder stays as written.
#[must_use]
pub fn substitute(template: &str, args: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(pos) = rest.find(|c: char| c == '{' || c == '}') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        if tail.starts_with("{{") {
            out.push('{');
            rest = &tail[2..];
            continue;
        }
        if tail.starts_with("}}") {
            out.push('}');
            rest = &tail[2..];
            continue;
        }
        if tail.starts_with('{') {
            if let Some(end) = tail.find('}') {
                let name = &tail[1..end];
                if is_param_name(name) {
                    if let Some(v) = args.get(name) {
                        out.push_str(&escape_xml(v));
                        rest = &tail[end + 1..];
                        continue;
                    }
                }
            }
        }
        out.push_str(&tail[..1]);
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

/// A parameter inferred from a subtree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferredParam {
    pub name: String,
    pub default: String,
    pub description: String,
}

impl InferredParam {
    fn to_field(&self) -> ComponentDataField {
        ComponentDataField {
            name: AzString::from(self.name.as_str()),
            field_type: ComponentFieldType::String,
            default_value: OptionComponentDefaultValue::Some(ComponentDefaultValue::String(
                AzString::from(self.default.as_str()),
            )),
            required: false,
            description: AzString::from(self.description.as_str()),
        }
    }
}

#[derive(Default)]
struct Inference {
    params: Vec<InferredParam>,
    texts: usize,
}

impl Inference {
    fn unique(&self, base: &str) -> String {
        if !self.params.iter().any(|p| p.name == base) {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base}_{n}"))
            .find(|c| !self.params.iter().any(|p| &p.name == c))
            .unwrap_or_else(|| base.to_string())
    }

    fn text_param(&mut self, text: &str, tag: &str) -> String {
        self.texts += 1;
        let name = if self.texts == 1 {
            self.unique("text")
        } else {
            self.unique(&format!("text_{}", self.texts))
        };
        self.params.push(InferredParam {
            name: name.clone(),
            default: text.to_string(),
            description: format!("Text of the <{tag}>"),
        });
        name
    }

    fn attr_param(&mut self, attr: &str, value: &str, tag: &str) -> String {
        let base: String = attr
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        let base = if base.starts_with(|c: char| c.is_ascii_digit()) || base.is_empty() {
            format!("attr_{base}")
        } else {
            base
        };
        let name = self.unique(&base);
        self.params.push(InferredParam {
            name: name.clone(),
            default: value.to_string(),
            description: format!("`{attr}` of the <{tag}>"),
        });
        name
    }
}

/// The template for a subtree, with its text and its non-structural
/// attributes (everything but `class` / `id` / `style`) turned into
/// parameters. Text params are `text`, `text_2`, …; attribute params are
/// named after the attribute.
#[must_use]
pub fn infer_template(root: &BuilderNode) -> (String, Vec<InferredParam>) {
    let mut inf = Inference::default();
    let mut out = String::new();
    write_template(&mut out, root, Some(&mut inf), "div");
    (out, inf.params)
}

/// `inf: Some` = infer parameters (literal braces get escaped); `None` = the
/// tree already IS a template, write it as it is (its `{placeholders}` stay).
fn write_template(
    out: &mut String,
    node: &BuilderNode,
    mut inf: Option<&mut Inference>,
    parent_tag: &str,
) {
    let lit = |s: &str, inferring: bool| {
        let e = escape_xml(s);
        if inferring {
            e.replace('{', "{{").replace('}', "}}")
        } else {
            e
        }
    };
    let inferring = inf.is_some();
    match &node.kind {
        BuilderNodeKind::Text { text } => match inf.as_deref_mut() {
            Some(i) if !text.trim().is_empty() => {
                let name = i.text_param(text, parent_tag);
                out.push('{');
                out.push_str(&name);
                out.push('}');
            }
            _ => out.push_str(&lit(text, inferring)),
        },
        BuilderNodeKind::Element { tag } => {
            out.push('<');
            out.push_str(tag);
            for (k, v) in &node.attrs {
                if k == "text" {
                    continue;
                }
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                match inf.as_deref_mut() {
                    Some(i) if !matches!(k.as_str(), "class" | "id" | "style") => {
                        let name = i.attr_param(k, v, tag);
                        out.push('{');
                        out.push_str(&name);
                        out.push('}');
                    }
                    _ => out.push_str(&lit(v, inferring)),
                }
                out.push('"');
            }
            if is_void(tag) {
                out.push_str("/>");
                return;
            }
            out.push('>');
            if let Some(t) = node.attrs.get("text") {
                match inf.as_deref_mut() {
                    Some(i) if !t.trim().is_empty() => {
                        let name = i.text_param(t, tag);
                        out.push('{');
                        out.push_str(&name);
                        out.push('}');
                    }
                    _ => out.push_str(&lit(t, inferring)),
                }
            }
            for c in &node.children {
                write_template(out, c, inf.as_deref_mut(), tag);
            }
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
        BuilderNodeKind::Component { library, name } => {
            out.push('<');
            out.push_str(library);
            out.push(':');
            out.push_str(name);
            for (k, v) in &node.attrs {
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                out.push_str(&lit(v, inferring));
                out.push('"');
            }
            out.push_str("/>");
        }
    }
}

/// A new user-defined component whose `render_fn` is the template.
#[must_use]
pub fn template_component_def(
    library: &str,
    name: &str,
    display_name: &str,
    description: &str,
    css: &str,
    template: &str,
    fields: Vec<ComponentDataField>,
) -> ComponentDef {
    let mut def = ComponentDef {
        id: ComponentId::new(library, name),
        display_name: AzString::from(display_name),
        description: AzString::from(description),
        css: AzString::from(css),
        source: ComponentSource::UserDefined,
        data_model: ComponentDataModel {
            name: AzString::from(format!("{}Data", pascal_case(display_name)).as_str()),
            description: AzString::from(description),
            fields: ComponentDataFieldVec::from_vec(fields),
        },
        render_fn: azul_core::xml::user_defined_render_fn,
        codegen: ComponentCodegen::RenderFunction,
        render_fn_source: None.into(),
    };
    set_template(&mut def, template);
    def
}

/// Make `def` a template component: `template` (placeholders and all) is its
/// markup and it renders through [`builder_template_render_fn`]. The code
/// export calls it through its render function
/// (`ComponentCodegen::RenderFunction`), defined from this template (see
/// `export::template_markup`).
pub fn set_template(def: &mut ComponentDef, template: &str) {
    def.render_fn = builder_template_render_fn;
    def.codegen = ComponentCodegen::RenderFunction;
    def.render_fn_source = Some(AzString::from(
        format!("{TEMPLATE_MARKER}\n{template}").as_str(),
    ))
    .into();
}

/// `create_component {render_tree}` — the UI's live-DOM "create component
/// from subtree": the tree becomes the template, its text and attributes
/// become the data model.
///
/// # Errors
/// A malformed or empty `render_tree`.
pub fn install_inferred_template(
    def: &mut ComponentDef,
    tree: &serde_json::Value,
) -> Result<(), String> {
    let nodes = nodes_from_render_tree(tree)?;
    if nodes.is_empty() {
        return Err("render_tree is empty".to_string());
    }
    let mut inf = Inference::default();
    let mut template = String::new();
    for n in &nodes {
        write_template(&mut template, n, Some(&mut inf), "div");
    }
    def.data_model.fields =
        ComponentDataFieldVec::from_vec(inf.params.iter().map(InferredParam::to_field).collect());
    set_template(def, &template);
    Ok(())
}

/// `update_component {render_tree}` — the component detail's tree editor:
/// the tree IS the template (its `{placeholders}` stay); the data model is
/// kept.
///
/// # Errors
/// A malformed `render_tree`.
pub fn install_template(def: &mut ComponentDef, tree: &serde_json::Value) -> Result<(), String> {
    let nodes = nodes_from_render_tree(tree)?;
    let mut template = String::new();
    for n in &nodes {
        write_template(&mut template, n, None, "div");
    }
    set_template(def, &template);
    Ok(())
}

/// A template component's tree in the shape the UI's mini tree reads
/// (`{nodes: [{tag, text?, classes, attrs, children, _component?}]}`),
/// placeholders and all — so editing it and sending it back through
/// `update_component {render_tree}` round-trips.
#[must_use]
pub fn template_render_tree_json(def: &ComponentDef) -> Option<serde_json::Value> {
    let template = template_of(def)?;
    let nodes = parse_fragment(template).ok()?;
    Some(serde_json::json!({
        "nodes": nodes.iter().map(render_tree_node_json).collect::<Vec<_>>(),
        "template": template,
    }))
}

fn render_tree_node_json(node: &BuilderNode) -> serde_json::Value {
    match &node.kind {
        BuilderNodeKind::Text { text } => serde_json::json!({
            "tag": "__text__", "text": text, "classes": [], "children": [],
        }),
        BuilderNodeKind::Element { tag } => {
            let classes: Vec<&str> = node
                .attrs
                .get("class")
                .map(|c| c.split_whitespace().collect())
                .unwrap_or_default();
            let attrs: BTreeMap<&String, &String> = node
                .attrs
                .iter()
                .filter(|(k, _)| k.as_str() != "class" && k.as_str() != "text")
                .collect();
            let mut v = serde_json::json!({
                "tag": tag,
                "classes": classes,
                "attrs": attrs,
                "children": node.children.iter().map(render_tree_node_json).collect::<Vec<_>>(),
            });
            if let (Some(t), Some(obj)) = (node.attrs.get("text"), v.as_object_mut()) {
                obj.insert("text".into(), serde_json::json!(t));
            }
            v
        }
        BuilderNodeKind::Component { library, name } => serde_json::json!({
            "tag": name,
            "_component": { "library": library, "component": name },
            "classes": [],
            "attrs": node.attrs,
            "children": [],
        }),
    }
}

/// The UI's render-tree JSON (a node object, or an array of them) → nodes.
///
/// # Errors
/// Not an object / array, a node that is not an object, invalid names.
pub fn nodes_from_render_tree(tree: &serde_json::Value) -> Result<Vec<BuilderNode>, String> {
    match tree {
        serde_json::Value::Array(items) => {
            items.iter().map(|v| node_from_render_json(v, 0)).collect()
        }
        serde_json::Value::Object(_) => Ok(vec![node_from_render_json(tree, 0)?]),
        serde_json::Value::Null => Ok(Vec::new()),
        _ => Err("render_tree must be a node object or an array of nodes".to_string()),
    }
}

fn node_from_render_json(v: &serde_json::Value, depth: usize) -> Result<BuilderNode, String> {
    if depth > MAX_TREE_DEPTH {
        return Err(format!(
            "render_tree is nested deeper than {MAX_TREE_DEPTH}"
        ));
    }
    let obj = v
        .as_object()
        .ok_or("every render_tree node must be a JSON object")?;
    let tag = obj
        .get("tag")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("div");
    let text = obj
        .get("text")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if tag == "__text__" || tag == "#text" {
        return Ok(BuilderNode::text(0, text));
    }

    let mut attrs: BTreeMap<String, String> = BTreeMap::new();
    if let Some(serde_json::Value::Object(a)) = obj.get("attrs") {
        for (k, val) in a {
            if let Some(s) = val.as_str() {
                if !is_valid_attr_name(k) {
                    return Err(format!("{k:?} is not a valid attribute name"));
                }
                attrs.insert(k.clone(), s.to_string());
            }
        }
    }

    if let Some(c) = obj.get("_component").and_then(serde_json::Value::as_object) {
        let library = c
            .get("library")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("builtin");
        let component = c
            .get("component")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(tag);
        if !(library.is_empty() || library == "builtin") {
            if !is_valid_library_name(library) || component.is_empty() {
                return Err(format!(
                    "{library}:{component} is not a valid component reference"
                ));
            }
            return Ok(BuilderNode {
                uid: 0,
                kind: BuilderNodeKind::Component {
                    library: library.to_string(),
                    name: component.to_string(),
                },
                attrs,
                children: Vec::new(),
            });
        }
        return element_from_render_json(obj, component, text, attrs, depth);
    }
    element_from_render_json(obj, tag, text, attrs, depth)
}

fn element_from_render_json(
    obj: &serde_json::Map<String, serde_json::Value>,
    tag: &str,
    text: &str,
    mut attrs: BTreeMap<String, String>,
    depth: usize,
) -> Result<BuilderNode, String> {
    let tag = tag.to_ascii_lowercase();
    if !is_valid_tag(&tag) {
        return Err(format!("{tag:?} is not a valid element name"));
    }
    if let Some(serde_json::Value::Array(cs)) = obj.get("classes") {
        let mut classes: Vec<&str> = attrs
            .get("class")
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default();
        for c in cs.iter().filter_map(serde_json::Value::as_str) {
            // UI state, not markup: the mini tree tags its own drops, and a
            // subtree taken from a mounted document carries the builder's
            // `azb-<uid>` markers, which must not be baked into a template.
            if c != "component-instance"
                && marker_uid(c).is_none()
                && !c.is_empty()
                && !classes.contains(&c)
            {
                classes.push(c);
            }
        }
        let joined = classes.join(" ");
        if !joined.is_empty() {
            attrs.insert("class".to_string(), joined);
        }
    }
    if let Some(id) = obj.get("id").and_then(serde_json::Value::as_str) {
        if !id.is_empty() {
            attrs.insert("id".to_string(), id.to_string());
        }
    }
    if !text.is_empty() {
        attrs.insert("text".to_string(), text.to_string());
    }
    let mut children = Vec::new();
    if let Some(serde_json::Value::Array(cs)) = obj.get("children") {
        for c in cs {
            children.push(node_from_render_json(c, depth + 1)?);
        }
    }
    Ok(BuilderNode {
        uid: 0,
        kind: BuilderNodeKind::Element { tag },
        attrs,
        children,
    })
}

/// Add `def` to `library`, creating the library (modifiable, exportable) if it
/// does not exist.
///
/// # Errors
/// The library is not modifiable, or already has a component of that name.
pub fn add_component(
    map: &mut ComponentMap,
    library: &str,
    def: ComponentDef,
) -> Result<(), String> {
    let empty = ComponentLibraryVec::from_const_slice(&[]);
    let mut libs = core::mem::replace(&mut map.libraries, empty).into_library_owned_vec();
    let result = push_component(&mut libs, library, def);
    map.libraries = ComponentLibraryVec::from_vec(libs);
    result
}

fn push_component(
    libs: &mut Vec<ComponentLibrary>,
    library: &str,
    def: ComponentDef,
) -> Result<(), String> {
    let name = def.id.name.as_str().to_string();
    if let Some(lib) = libs.iter_mut().find(|l| l.name.as_str() == library) {
        if !lib.modifiable {
            return Err(format!("library '{library}' is not modifiable"));
        }
        if lib.components.iter().any(|c| c.id.name.as_str() == name) {
            return Err(format!(
                "component '{library}:{name}' already exists; pick another name"
            ));
        }
        let mut comps =
            core::mem::replace(&mut lib.components, Vec::new().into()).into_library_owned_vec();
        comps.push(def);
        lib.components = ComponentDefVec::from_vec(comps);
    } else {
        libs.push(ComponentLibrary {
            name: AzString::from(library),
            version: AzString::from_const_str("0.1.0"),
            description: AzString::from_const_str("Created in AzBuilder"),
            components: ComponentDefVec::from_vec(vec![def]),
            exportable: true,
            modifiable: true,
            data_models: azul_core::xml::ComponentDataModelVec::from_const_slice(&[]),
            enum_models: azul_core::xml::ComponentEnumModelVec::from_const_slice(&[]),
        });
    }
    Ok(())
}

// ===========================================================================
// Data-model helpers
// ===========================================================================

/// Every field's value for one instance: the instance's attribute, else the
/// field's default, else "".
fn instance_args(
    dm: &ComponentDataModel,
    attrs: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    dm.fields
        .as_ref()
        .iter()
        .map(|f| {
            let name = f.name.as_str();
            let v = attrs
                .get(name)
                .cloned()
                .or_else(|| default_to_string(&f.default_value))
                .unwrap_or_default();
            (name.to_string(), v)
        })
        .collect()
}

fn default_to_string(v: &OptionComponentDefaultValue) -> Option<String> {
    let OptionComponentDefaultValue::Some(d) = v else {
        return None;
    };
    match d {
        ComponentDefaultValue::None | ComponentDefaultValue::ComponentInstance(_) => None,
        ComponentDefaultValue::String(s)
        | ComponentDefaultValue::CallbackFnPointer(s)
        | ComponentDefaultValue::Json(s) => Some(s.as_str().to_string()),
        ComponentDefaultValue::Bool(b) => Some(b.to_string()),
        ComponentDefaultValue::I32(n) => Some(n.to_string()),
        ComponentDefaultValue::I64(n) => Some(n.to_string()),
        ComponentDefaultValue::U32(n) => Some(n.to_string()),
        ComponentDefaultValue::U64(n) => Some(n.to_string()),
        ComponentDefaultValue::Usize(n) => Some(n.to_string()),
        ComponentDefaultValue::F32(n) => Some(n.to_string()),
        ComponentDefaultValue::F64(n) => Some(n.to_string()),
        ComponentDefaultValue::ColorU(c) => {
            Some(format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a))
        }
    }
}

/// The data model with an instance's attributes as the current values (for a
/// component rendered through its own `render_fn`).
fn data_model_with_args(
    dm: &ComponentDataModel,
    attrs: &BTreeMap<String, String>,
) -> ComponentDataModel {
    let mut out = dm.clone();
    let mut fields: Vec<ComponentDataField> = dm.fields.as_ref().to_vec();
    for f in &mut fields {
        let Some(v) = attrs.get(f.name.as_str()) else {
            continue;
        };
        let t = v.trim();
        let parsed = match f.field_type {
            ComponentFieldType::String => {
                Some(ComponentDefaultValue::String(AzString::from(v.as_str())))
            }
            ComponentFieldType::Bool => Some(ComponentDefaultValue::Bool(matches!(
                t,
                "true" | "1" | "yes" | "on"
            ))),
            ComponentFieldType::I32 => t.parse::<i32>().ok().map(ComponentDefaultValue::I32),
            ComponentFieldType::I64 => t.parse::<i64>().ok().map(ComponentDefaultValue::I64),
            ComponentFieldType::U32 => t.parse::<u32>().ok().map(ComponentDefaultValue::U32),
            ComponentFieldType::U64 => t.parse::<u64>().ok().map(ComponentDefaultValue::U64),
            ComponentFieldType::Usize => t.parse::<usize>().ok().map(ComponentDefaultValue::Usize),
            ComponentFieldType::F32 => t.parse::<f32>().ok().map(ComponentDefaultValue::F32),
            ComponentFieldType::F64 => t.parse::<f64>().ok().map(ComponentDefaultValue::F64),
            _ => None,
        };
        if let Some(p) = parsed {
            f.default_value = OptionComponentDefaultValue::Some(p);
        }
    }
    out.fields = ComponentDataFieldVec::from_vec(fields);
    out
}

// ===========================================================================
// Names and escaping
// ===========================================================================

fn is_void(tag: &str) -> bool {
    VOID_ELEMENTS.contains(&tag)
}

/// Why `child` cannot be a child of `parent` under [`AUTO_CLOSE`], if it cannot.
fn why_not_inside(parent: &BuilderNodeKind, child: &BuilderNodeKind) -> Option<String> {
    let (BuilderNodeKind::Element { tag: parent_tag }, BuilderNodeKind::Element { tag }) =
        (parent, child)
    else {
        return None;
    };
    let closers = AUTO_CLOSE
        .iter()
        .find(|(p, _)| *p == parent_tag.as_str())?
        .1;
    closers.contains(&tag.as_str()).then(|| {
        format!(
            "a <{tag}> cannot go inside a <{parent_tag}>: HTML ends the <{parent_tag}> where the \
             <{tag}> starts, so the window would show them as siblings"
        )
    })
}

fn is_valid_tag(tag: &str) -> bool {
    let mut chars = tag.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_valid_attr_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
}

fn is_valid_component_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

fn is_valid_library_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn is_param_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Escape for XML text and attribute values (the parser decodes all five).
fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// `my-card` → `My Card`.
fn title_case(name: &str) -> String {
    name.split(|c: char| c == '-' || c == '_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut cs = w.chars();
            cs.next()
                .map(|f| f.to_ascii_uppercase().to_string() + cs.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `My Card` → `MyCard` (a type name for code export).
fn pascal_case(display: &str) -> String {
    let s: String = display
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut cs = w.chars();
            cs.next()
                .map(|f| f.to_ascii_uppercase().to_string() + cs.as_str())
                .unwrap_or_default()
        })
        .collect();
    if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) {
        format!("Component{s}")
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn el(tag: &str) -> BuilderNodeKind {
        BuilderNodeKind::Element {
            tag: tag.to_string(),
        }
    }

    /// The child uids of `uid`, in order.
    fn kids(doc: &BuilderDocument, uid: u64) -> Vec<u64> {
        doc.node(uid)
            .expect("node exists")
            .children
            .iter()
            .map(|c| c.uid)
            .collect()
    }

    fn three_paragraphs() -> BuilderDocument {
        let mut doc = BuilderDocument::new();
        for id in ["a", "b", "c"] {
            doc.insert(ROOT_UID, None, el("p"), attrs(&[("id", id)]))
                .expect("insert");
        }
        doc
    }

    #[test]
    fn every_insert_takes_exactly_one_new_uid_and_lands_at_its_index() {
        let mut doc = three_paragraphs();
        assert_eq!(kids(&doc, 0), vec![1, 2, 3]);
        let d = doc
            .insert(0, Some(1), el("div"), BTreeMap::new())
            .expect("insert");
        assert_eq!(d, 4);
        assert_eq!(kids(&doc, 0), vec![1, 4, 2, 3]);
        let s = doc
            .insert(4, None, el("span"), BTreeMap::new())
            .expect("insert");
        assert_eq!(kids(&doc, 4), vec![s]);
    }

    #[test]
    fn an_index_past_the_end_is_refused_and_leaves_no_undo_step() {
        let mut doc = three_paragraphs();
        let undo_before = doc.undo.len();
        let err = doc
            .insert(0, Some(4), el("p"), BTreeMap::new())
            .unwrap_err();
        assert!(err.contains("past the end"), "{err}");
        assert_eq!(doc.undo.len(), undo_before);
    }

    #[test]
    fn moving_to_a_later_slot_of_the_same_parent_uses_the_before_move_slot() {
        let mut doc = three_paragraphs();
        // a to the slot after c (slot 3 of [a, b, c]).
        doc.move_node(1, 0, Some(3)).expect("move");
        assert_eq!(kids(&doc, 0), vec![2, 3, 1]);
        // c to the slot before b.
        doc.move_node(3, 0, Some(0)).expect("move");
        assert_eq!(kids(&doc, 0), vec![3, 2, 1]);
        // Dropping a node right before or after itself changes nothing.
        doc.move_node(2, 0, Some(1)).expect("move");
        assert_eq!(kids(&doc, 0), vec![3, 2, 1]);
        doc.move_node(2, 0, Some(2)).expect("move");
        assert_eq!(kids(&doc, 0), vec![3, 2, 1]);
    }

    #[test]
    fn moving_into_a_later_sibling_follows_the_sibling_up_one_slot() {
        let mut doc = three_paragraphs();
        let div = doc
            .insert(0, None, el("div"), BTreeMap::new())
            .expect("insert");
        // a leaves slot 0, which shifts the div from slot 3 to slot 2.
        doc.move_node(1, div, None).expect("move");
        assert_eq!(kids(&doc, 0), vec![2, 3, div]);
        assert_eq!(kids(&doc, div), vec![1]);
    }

    #[test]
    fn a_node_cannot_move_into_itself_or_its_subtree() {
        let mut doc = BuilderDocument::new();
        let outer = doc
            .insert(0, None, el("div"), BTreeMap::new())
            .expect("insert");
        let inner = doc
            .insert(outer, None, el("div"), BTreeMap::new())
            .expect("insert");
        assert!(doc
            .move_node(outer, inner, None)
            .unwrap_err()
            .contains("descendant"));
        assert!(doc
            .move_node(outer, outer, None)
            .unwrap_err()
            .contains("descendant"));
        assert!(doc
            .move_node(ROOT_UID, outer, None)
            .unwrap_err()
            .contains("root"));
    }

    #[test]
    fn text_nodes_void_elements_and_instances_take_no_children() {
        let mut doc = BuilderDocument::new();
        let t = doc
            .insert(
                0,
                None,
                BuilderNodeKind::Text { text: "x".into() },
                BTreeMap::new(),
            )
            .expect("insert");
        let br = doc
            .insert(0, None, el("br"), BTreeMap::new())
            .expect("insert");
        assert!(doc
            .insert(t, None, el("p"), BTreeMap::new())
            .unwrap_err()
            .contains("text node"));
        assert!(doc
            .insert(br, None, el("p"), BTreeMap::new())
            .unwrap_err()
            .contains("void"));
        let card = doc
            .insert(
                0,
                None,
                BuilderNodeKind::Component {
                    library: "user".into(),
                    name: "card".into(),
                },
                BTreeMap::new(),
            )
            .expect("insert");
        assert!(doc
            .insert(card, None, el("p"), BTreeMap::new())
            .unwrap_err()
            .contains("instance of the component user:card"));
        // A refused edit leaves the tree as it was.
        assert_eq!(kids(&doc, 0), vec![t, br, card]);
    }

    #[test]
    fn a_block_element_cannot_go_inside_a_paragraph_but_inline_content_can() {
        let mut doc = BuilderDocument::new();
        let p = doc
            .insert(0, None, el("p"), BTreeMap::new())
            .expect("insert");
        let err = doc.insert(p, None, el("div"), BTreeMap::new()).unwrap_err();
        assert!(err.contains("cannot go inside a <p>"), "{err}");
        doc.insert(p, None, el("span"), BTreeMap::new())
            .expect("inline content is fine");
        let div = doc
            .insert(0, None, el("div"), BTreeMap::new())
            .expect("insert");
        assert!(doc
            .move_node(div, p, None)
            .unwrap_err()
            .contains("cannot go inside"));
        let li = doc
            .insert(0, None, el("li"), BTreeMap::new())
            .expect("insert");
        assert!(doc.insert(li, None, el("li"), BTreeMap::new()).is_err());
    }

    #[test]
    fn undo_and_redo_replay_edits_and_a_new_edit_clears_redo() {
        let mut doc = three_paragraphs();
        doc.delete(2).expect("delete");
        assert_eq!(kids(&doc, 0), vec![1, 3]);
        doc.undo().expect("undo");
        assert_eq!(kids(&doc, 0), vec![1, 2, 3]);
        doc.redo().expect("redo");
        assert_eq!(kids(&doc, 0), vec![1, 3]);
        doc.undo().expect("undo");
        assert!(doc.can_redo());
        let hr = doc
            .insert(0, None, el("hr"), BTreeMap::new())
            .expect("insert");
        assert!(!doc.can_redo(), "a new edit forks the history");
        assert_eq!(hr, 4);
        // uids are never reused, not even after the edit that took one was undone.
        doc.undo().expect("undo");
        let br = doc
            .insert(0, None, el("br"), BTreeMap::new())
            .expect("insert");
        assert_eq!(br, 5);
        assert_eq!(kids(&doc, 0), vec![1, 2, 3, 5]);
        doc.undo().expect("undo");
        assert_eq!(kids(&doc, 0), vec![1, 2, 3]);
        // ...and the step before that is the third paragraph's insert.
        doc.undo().expect("undo");
        assert_eq!(kids(&doc, 0), vec![1, 2]);
    }

    #[test]
    fn the_undo_history_is_capped() {
        let mut doc = BuilderDocument::new();
        for _ in 0..(MAX_UNDO + 20) {
            doc.insert(0, None, el("p"), BTreeMap::new())
                .expect("insert");
        }
        assert_eq!(doc.undo.len(), MAX_UNDO);
    }

    #[test]
    fn the_mount_xml_marks_every_document_node_and_escapes_text() {
        let mut doc = BuilderDocument::new();
        let p = doc
            .insert(
                0,
                None,
                el("p"),
                attrs(&[("class", "x"), ("text", "a < b & \"c\"")]),
            )
            .expect("insert");
        doc.insert(0, None, el("br"), BTreeMap::new())
            .expect("insert");
        let xml = doc.to_mount_xml(&ComponentMap::default());
        assert!(xml.contains("<body class=\"azb-0\">"), "{xml}");
        assert!(
            xml.contains(&format!(
                "<p class=\"x azb-{p}\">a &lt; b &amp; &quot;c&quot;</p>"
            )),
            "{xml}"
        );
        assert!(xml.contains("<br class=\"azb-2\"/>"), "{xml}");
    }

    #[test]
    fn substitution_fills_placeholders_escapes_values_and_keeps_literal_braces() {
        let args = attrs(&[("text", "<b>&"), ("n", "1")]);
        assert_eq!(substitute("<p>{text}</p>", &args), "<p>&lt;b&gt;&amp;</p>");
        assert_eq!(substitute("{{text}} {n}", &args), "{text} 1");
        assert_eq!(
            substitute("{unknown} {1bad} {", &args),
            "{unknown} {1bad} {"
        );
    }

    #[test]
    fn inference_turns_text_and_attributes_into_parameters() {
        let mut card = BuilderNode::element(1, "div");
        card.attrs = attrs(&[("class", "card {x}")]);
        let mut h1 = BuilderNode::element(2, "h1");
        h1.attrs = attrs(&[("text", "Title")]);
        let mut a = BuilderNode::element(3, "a");
        a.attrs = attrs(&[("href", "https://e.com"), ("text", "More")]);
        card.children = vec![h1, a];

        let (template, params) = infer_template(&card);
        assert_eq!(
            template,
            "<div class=\"card {{x}}\"><h1>{text}</h1><a href=\"{href}\">{text_2}</a></div>"
        );
        let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["text", "href", "text_2"]);
        assert_eq!(params[1].default, "https://e.com");

        // Substituting the defaults gives back the subtree's markup.
        let defaults: BTreeMap<String, String> = params
            .iter()
            .map(|p| (p.name.clone(), p.default.clone()))
            .collect();
        assert_eq!(
            substitute(&template, &defaults),
            "<div class=\"card {x}\"><h1>Title</h1><a href=\"https://e.com\">More</a></div>"
        );
    }

    #[test]
    fn a_render_tree_becomes_nodes_and_a_raw_template_keeps_its_placeholders() {
        let tree = serde_json::json!([{
            "tag": "div", "classes": ["panel", "component-instance", "azb-3"], "_idx": 0,
            "children": [
                { "tag": "p", "text": "{text}", "children": [] },
                { "tag": "__text__", "text": "tail" },
                { "tag": "card", "_component": { "library": "user", "component": "card" } }
            ]
        }]);
        let nodes = nodes_from_render_tree(&tree).expect("parses");
        let mut t = String::new();
        write_template(&mut t, &nodes[0], None, "div");
        assert_eq!(
            t,
            "<div class=\"panel\"><p>{text}</p>tail<user:card/></div>"
        );
        assert!(nodes_from_render_tree(&serde_json::json!(3)).is_err());
        assert!(nodes_from_render_tree(&serde_json::json!({"tag": "Not A Tag"})).is_err());
    }

    #[test]
    fn a_template_parses_back_into_component_instances_and_elements() {
        let nodes = parse_fragment("<div class=\"a\"><user:card title=\"x\"/><svg:g/></div>")
            .expect("parses");
        assert_eq!(nodes.len(), 1);
        let kinds: Vec<&BuilderNodeKind> = nodes[0].children.iter().map(|c| &c.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &BuilderNodeKind::Component {
                    library: "user".into(),
                    name: "card".into()
                },
                &el("svg:g"),
            ]
        );
    }

    #[test]
    fn a_converted_component_expands_inline_with_the_instance_marker_and_arguments() {
        let mut map = ComponentMap::default();
        let def = template_component_def(
            "user",
            "badge",
            "Badge",
            "",
            ".badge { color: red; }",
            "<span class=\"badge\">{text}</span>",
            vec![InferredParam {
                name: "text".into(),
                default: "New".into(),
                description: String::new(),
            }
            .to_field()],
        );
        add_component(&mut map, "user", def).expect("added");
        let mut doc = BuilderDocument::new();
        let a = doc
            .insert(
                0,
                None,
                BuilderNodeKind::Component {
                    library: "user".into(),
                    name: "badge".into(),
                },
                BTreeMap::new(),
            )
            .expect("insert");
        let b = doc
            .insert(
                0,
                None,
                BuilderNodeKind::Component {
                    library: "user".into(),
                    name: "badge".into(),
                },
                attrs(&[("text", "Hot"), ("class", "big")]),
            )
            .expect("insert");
        let xml = doc.to_mount_xml(&map);
        assert!(
            xml.contains(&format!("<span class=\"badge azb-{a}\">New</span>")),
            "{xml}"
        );
        assert!(
            xml.contains(&format!("<span class=\"badge big azb-{b}\">Hot</span>")),
            "{xml}"
        );
        // The component CSS goes into the <style>, once.
        assert_eq!(xml.matches(".badge { color: red; }").count(), 1, "{xml}");
        // A second component of the same name is refused.
        let dup = template_component_def("user", "badge", "Badge", "", "", "<i/>", Vec::new());
        assert!(add_component(&mut map, "user", dup)
            .unwrap_err()
            .contains("already exists"));
    }

    #[test]
    fn a_missing_component_renders_a_visible_placeholder_instead_of_vanishing() {
        let mut doc = BuilderDocument::new();
        doc.root.children.push(BuilderNode {
            uid: 7,
            kind: BuilderNodeKind::Component {
                library: "gone".into(),
                name: "x".into(),
            },
            attrs: BTreeMap::new(),
            children: Vec::new(),
        });
        let xml = doc.to_mount_xml(&ComponentMap::default());
        assert!(xml.contains("azb-7"), "{xml}");
        assert!(xml.contains("[missing component gone:x]"), "{xml}");
    }

    #[test]
    fn a_self_containing_template_stops_at_the_depth_limit() {
        let mut map = ComponentMap::default();
        let def = template_component_def(
            "user",
            "loop",
            "Loop",
            "",
            "",
            "<div><user:loop/></div>",
            Vec::new(),
        );
        add_component(&mut map, "user", def).expect("added");
        let mut doc = BuilderDocument::new();
        doc.insert(
            0,
            None,
            BuilderNodeKind::Component {
                library: "user".into(),
                name: "loop".into(),
            },
            BTreeMap::new(),
        )
        .expect("insert");
        let xml = doc.to_mount_xml(&map);
        assert!(xml.contains("nested deeper than"), "{xml}");
    }

    #[test]
    fn names_are_validated() {
        assert!(is_valid_tag("h1") && is_valid_tag("my-tag"));
        assert!(
            !is_valid_tag("H1") && !is_valid_tag("1p") && !is_valid_tag("a b") && !is_valid_tag("")
        );
        assert!(is_valid_attr_name("data-x") && is_valid_attr_name("xml:lang"));
        assert!(!is_valid_attr_name("a b") && !is_valid_attr_name("\"x"));
        assert!(is_valid_component_name("my-card_2") && !is_valid_component_name("My"));
        assert_eq!(title_case("my-card"), "My Card");
        assert_eq!(pascal_case("My Card"), "MyCard");
        assert_eq!(pascal_case("1 up"), "Component1Up");
    }

    // ── B4: a document read back from a project file ──

    #[test]
    fn a_document_written_as_json_reads_back_as_the_same_tree_with_fresh_uids() {
        let mut doc = three_paragraphs();
        doc.insert(2, None, el("span"), attrs(&[("text", "inner")]))
            .expect("span in b");
        doc.insert(
            0,
            Some(0),
            BuilderNodeKind::Component {
                library: "user".into(),
                name: "card".into(),
            },
            attrs(&[("text", "Hi")]),
        )
        .expect("instance");
        let json = doc.root.to_json();
        let back = BuilderNode::from_json(&json).expect("reads back");
        let loaded = BuilderDocument::from_root(back);
        // Same shape, same attributes; uids renumbered in DFS order.
        assert_eq!(loaded.root.children.len(), 4);
        assert_eq!(
            loaded.root.children[0].kind,
            BuilderNodeKind::Component {
                library: "user".into(),
                name: "card".into()
            }
        );
        assert_eq!(loaded.root.children[2].children[0].attrs["text"], "inner");
        assert_eq!(kids(&loaded, 0), vec![1, 2, 3, 5]);
        assert_eq!(kids(&loaded, 3), vec![4]);
        assert!(!loaded.can_undo(), "opening a file is not an edit");
        // The next insert takes a uid nobody has.
        let mut loaded = loaded;
        let uid = loaded
            .insert(0, None, el("p"), BTreeMap::new())
            .expect("insert after load");
        assert_eq!(uid, 6);
    }

    #[test]
    fn a_document_file_that_the_parser_would_undo_is_refused_with_a_reason() {
        let cases: [(serde_json::Value, &str); 5] = [
            (
                serde_json::json!({ "kind": "element", "tag": "br",
                    "children": [ { "kind": "text", "text": "x" } ] }),
                "void",
            ),
            (
                serde_json::json!({ "kind": "element", "tag": "p",
                    "children": [ { "kind": "element", "tag": "div" } ] }),
                "cannot go inside",
            ),
            (serde_json::json!({ "kind": "widget", "tag": "p" }), "unknown node kind"),
            (serde_json::json!({ "kind": "element", "tag": "P P" }), "not a valid element"),
            (
                serde_json::json!({ "kind": "component", "library": "builtin", "tag": "div" }),
                "not a component library",
            ),
        ];
        for (json, needle) in cases {
            let err = BuilderNode::from_json(&json).expect_err("refused");
            assert!(err.contains(needle), "{json}: expected '{needle}' in {err}");
        }
    }

    #[test]
    fn the_project_stylesheet_is_mounted_after_the_component_css() {
        let map = ComponentMap::default();
        let mut session = BuilderSession::default();
        assert_eq!(
            session.set_project_stylesheet(&map, "#a { width: 1px; }".into()),
            Remount::Keep,
            "no document on screen: nothing to remount"
        );
        let reply = session.load_document(
            &map,
            BuilderDocument::from_root(BuilderNode::element(0, "body")),
        );
        match reply.remount {
            Remount::Mount(xml) => assert!(xml.contains("#a { width: 1px; }"), "{xml}"),
            other => panic!("load mounts the document, got {other:?}"),
        }
        match session.set_project_stylesheet(&map, "#a { width: 2px; }".into()) {
            Remount::Mount(xml) => {
                assert!(xml.contains("#a { width: 2px; }") && !xml.contains("1px"), "{xml}");
            }
            other => panic!("a new stylesheet remounts, got {other:?}"),
        }
        assert_eq!(
            session.set_project_stylesheet(&map, "#a { width: 2px; }".into()),
            Remount::Keep,
            "the same text again changes nothing"
        );
    }

    // ── B5: the marker, the document's own stylesheet ──

    #[test]
    fn only_azb_followed_by_digits_is_a_marker() {
        assert_eq!(marker_uid("azb-0"), Some(0));
        assert_eq!(marker_uid("azb-42"), Some(42));
        assert_eq!(marker_uid("azb-card"), None, "an ordinary class");
        assert_eq!(marker_uid("azb-"), None);
        assert_eq!(marker_uid("azb-1x"), None);
        assert_eq!(marker_uid("xazb-1"), None);
        // A live node's marker; an import keeps an ordinary `azb-` class and
        // drops the marker.
        let sd = crate::xml::parse_xml_to_styled_dom(
            "<html><body><div class=\"azb-card azb-7\"></div></body></html>",
        )
        .expect("parses");
        let marked: Vec<u64> = (0..sd.node_data.as_ref().len())
            .filter_map(|i| node_marker(&sd, NodeId::new(i)))
            .collect();
        assert_eq!(marked, vec![7]);
        let imported = BuilderDocument::from_styled_dom(Some(&sd));
        assert_eq!(imported.root.children[0].attrs["class"], "azb-card");
        let mut doc = BuilderDocument::new();
        doc.insert(0, None, el("p"), attrs(&[("class", "azb-card")]))
            .expect("insert");
        let xml = doc.to_mount_xml(&ComponentMap::default());
        assert!(xml.contains("class=\"azb-card azb-1\""), "{xml}");
    }

    #[test]
    fn the_documents_stylesheet_is_one_undo_step_and_the_same_text_is_none() {
        let mut doc = three_paragraphs();
        let steps = doc.undo.len();
        doc.set_stylesheet(".a { color: red; }").expect("set");
        assert_eq!(doc.undo.len(), steps + 1);
        doc.set_stylesheet(".a { color: red; }").expect("same");
        assert_eq!(doc.undo.len(), steps + 1, "the same text is no step");
        doc.delete(2).expect("delete");
        // Undo walks back through tree edits and stylesheet edits alike.
        doc.undo().expect("undo the delete");
        assert_eq!(kids(&doc, 0), vec![1, 2, 3]);
        assert_eq!(doc.stylesheet, ".a { color: red; }");
        doc.undo().expect("undo the stylesheet");
        assert_eq!(doc.stylesheet, "");
        doc.redo().expect("redo the stylesheet");
        assert_eq!(doc.stylesheet, ".a { color: red; }");
        assert_eq!(kids(&doc, 0), vec![1, 2, 3], "the tree is untouched by it");
        assert!(doc
            .set_stylesheet(&"x".repeat(MAX_STYLESHEET + 1))
            .unwrap_err()
            .contains("limit"));
    }

    #[test]
    fn the_documents_stylesheet_is_mounted_last_in_a_style_of_its_own() {
        let mut map = ComponentMap::default();
        let def = template_component_def(
            "user",
            "badge",
            "Badge",
            "",
            ".badge { color: red; }",
            "<span class=\"badge\">x</span>",
            Vec::new(),
        );
        add_component(&mut map, "user", def).expect("added");
        let mut doc = BuilderDocument::new();
        doc.insert(
            0,
            None,
            BuilderNodeKind::Component {
                library: "user".into(),
                name: "badge".into(),
            },
            BTreeMap::new(),
        )
        .expect("instance");
        assert!(
            !doc.to_mount_xml(&map).contains("</style><style>"),
            "no sheet: no second <style>"
        );
        doc.set_stylesheet(".badge > b { color: blue; }").expect("set");
        let xml = doc.to_mount_xml_with(&map, "#p { width: 1px; }");
        let component = xml.find(".badge { color: red; }").expect("component css");
        let project = xml.find("#p { width: 1px; }").expect("project css");
        let own = xml
            .find("<style>.badge &gt; b { color: blue; }</style></head>")
            .expect("the document's own sheet, escaped, last in the <head>");
        assert!(component < project && project < own, "{xml}");
    }

    #[test]
    fn a_duplicate_lands_right_after_the_original_with_fresh_dfs_uids() {
        let mut doc = three_paragraphs();
        let span = doc
            .insert(2, None, el("span"), attrs(&[("text", "x")]))
            .expect("span in b");
        let steps = doc.undo.len();
        let copy = doc.duplicate(2).expect("duplicate b");
        assert_eq!(copy, 5);
        assert_eq!(kids(&doc, 0), vec![1, 2, 5, 3]);
        assert_eq!(kids(&doc, 2), vec![span], "the original keeps its children");
        assert_eq!(kids(&doc, 5), vec![6], "the copy has copies of them");
        assert_eq!(doc.node(6).expect("copy").attrs["text"], "x");
        assert_eq!(doc.undo.len(), steps + 1, "one undo step");
        doc.undo().expect("undo");
        assert_eq!(kids(&doc, 0), vec![1, 2, 3]);
        assert!(doc.duplicate(ROOT_UID).unwrap_err().contains("root"));
        assert!(doc.duplicate(99).unwrap_err().contains("99"));
        // uids are never reused: the next copy starts after the undone one.
        assert_eq!(doc.duplicate(1).expect("again"), 7);
    }

    #[test]
    fn replacing_the_document_is_one_undo_step_with_fresh_uids() {
        let mut doc = three_paragraphs();
        doc.set_stylesheet("#a { width: 1px; }").expect("set");
        let mut other = BuilderDocument::new();
        let div = other
            .insert(0, None, el("div"), BTreeMap::new())
            .expect("div");
        other
            .insert(div, None, el("p"), BTreeMap::new())
            .expect("p");
        other.set_stylesheet("div { }").expect("sheet");
        let loaded = BuilderDocument::from_file_json(&other.to_file_json()).expect("reads");
        doc.replace_with(loaded).expect("replace");
        assert_eq!(kids(&doc, 0), vec![4], "fresh uids after 1..3");
        assert_eq!(kids(&doc, 4), vec![5]);
        assert_eq!(doc.stylesheet, "div { }");
        doc.undo().expect("undo");
        assert_eq!(kids(&doc, 0), vec![1, 2, 3]);
        assert_eq!(doc.stylesheet, "#a { width: 1px; }");
    }

    #[test]
    fn a_document_file_carries_the_stylesheet_and_older_files_and_bare_trees_still_read() {
        let mut doc = three_paragraphs();
        doc.set_stylesheet("#a { width: 1px; }").expect("set");
        let file = doc.to_file_json();
        assert_eq!(file["format"], DOCUMENT_FORMAT);
        assert_eq!(file["stylesheet"], "#a { width: 1px; }");
        assert!(!file.to_string().contains("\"uid\""), "{file}");
        let back = BuilderDocument::from_file_json(&file).expect("reads back");
        assert_eq!(back.stylesheet, "#a { width: 1px; }");
        assert_eq!(kids(&back, 0), vec![1, 2, 3]);
        assert!(!back.can_undo(), "reading a file is not an edit");

        // Written before B5: no stylesheet.
        let old = serde_json::json!({ "format": DOCUMENT_FORMAT, "version": 1, "root": file["root"] });
        assert_eq!(BuilderDocument::from_file_json(&old).expect("old").stylesheet, "");
        // A bare tree.
        let bare = BuilderDocument::from_file_json(&file["root"]).expect("bare");
        assert_eq!(kids(&bare, 0), vec![1, 2, 3]);
        // Refusals.
        let wrong = serde_json::json!({ "format": "azul-project", "root": file["root"] });
        assert!(BuilderDocument::from_file_json(&wrong)
            .unwrap_err()
            .contains("format"));
        let bad = serde_json::json!({ "root": file["root"], "stylesheet": 3 });
        assert!(BuilderDocument::from_file_json(&bad)
            .unwrap_err()
            .contains("stylesheet"));
    }
}
