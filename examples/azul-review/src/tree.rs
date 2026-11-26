//! The file tree as a model: what the folders hold, which are open, and the rows that are on the screen.
//!
//! The tree of a workspace is not read whole. The root is listed when the window opens, a folder when it is opened (a
//! listing is one `FileSource::list` on a thread of its own), and what the review does not read - hidden names, build
//! output, links, files that are no text of code - is not in it. A tree of the Azul checkout is 5,000 files in 1,400
//! folders: the model holds them (a node is a name and a few numbers), the window builds only the rows of the folders that
//! are open and, of those, only the ones on the screen ([`row_window`]).
//!
//! Pure: the listings go in as `DirEntry` lists, no disk, no window, no azul (`tree_test.rs`). The rules of what is listed
//! are the symbol index's ([`Exclusions`]: the walk of the two must not differ), and one that is the review's own: a file is
//! shown when it is code, a manifest or a text the reviewer reads (the names the earlier eager walk read).

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use crate::symbols_source::{self, DirEntry, EntryKind, Exclusions, FileClass};

/// A node of the tree.
pub type NodeId = usize;

/// How many nodes a tree keeps: far more than a checkout of a very large project.
pub const MAX_NODES: usize = 250_000;
/// How long a name is, bytes: longer is no name a file system gives.
pub const MAX_NAME_BYTES: usize = 255;
/// The height of a row, px: the window's rows and the window of rows are of this height.
pub const ROW_H: f32 = 22.0;
/// How many rows are built above and below the ones on the screen, so a scroll does not show a gap before the next frame.
pub const OVERSCAN: usize = 6;

/// The extensions of the texts a review reads besides the languages the symbol index knows.
static TEXTS: [&str; 12] = ["rs", "toml", "md", "markdown", "c", "h", "cpp", "hpp", "py", "js", "ts", "sh"];
/// The configuration the reviewer reads.
static CONFIGS: [&str; 3] = ["yml", "yaml", "json"];

/// Whether a review shows the file `name`.
#[must_use]
pub fn is_reviewable(name: &str) -> bool {
    if !matches!(symbols_source::classify(name), FileClass::Other) {
        return true;
    }
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    if stem.is_empty() {
        return false;
    }
    let ext = ext.to_ascii_lowercase();
    TEXTS.contains(&ext.as_str()) || CONFIGS.contains(&ext.as_str())
}

/// What the walk leaves out.
#[derive(Clone, Debug, Default)]
pub struct Rules {
    pub exclusions: Exclusions,
}

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
}

/// A name, and what is known of it.
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub parent: Option<NodeId>,
    pub kind: Kind,
    /// Bytes (a file).
    pub size: u64,
    /// Milliseconds since the Unix epoch (a file; 0: not known).
    pub mtime_ms: u64,
    /// How many lines a file has, once they are counted.
    pub lines: Option<u32>,
    children: Vec<NodeId>,
    listed: bool,
    expanded: bool,
    listing: bool,
    /// Gone from the tree (a listing that no longer has it): the place in the list is kept so no id moves.
    gone: bool,
}

impl Node {
    fn new(name: String, parent: Option<NodeId>, kind: Kind) -> Self {
        Self {
            name,
            parent,
            kind,
            size: 0,
            mtime_ms: 0,
            lines: None,
            children: Vec::new(),
            listed: false,
            expanded: false,
            listing: false,
            gone: false,
        }
    }
}

/// What a listing did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Listed {
    /// How many entries became nodes (new ones; a name that was there already is not counted).
    pub added: usize,
    /// How many entries the rules leave out.
    pub left_out: usize,
    /// The cap on the number of nodes was reached: the rest of the listing was not taken.
    pub capped: bool,
}

/// What a click on a row of the tree did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    /// A file: it is opened, not toggled.
    File,
    /// The folder is open now; `needs_listing`: nobody has listed it yet.
    Opened { needs_listing: bool },
    Closed,
}

/// One row of the tree as the window draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: NodeId,
    pub depth: usize,
    pub name: String,
    pub kind: Kind,
    /// A folder that is open.
    pub expanded: bool,
    /// A folder whose entries are known.
    pub listed: bool,
    /// A folder that is open and being listed.
    pub loading: bool,
}

/// The tree.
#[derive(Debug, Clone)]
pub struct Tree {
    nodes: Vec<Node>,
    cap: usize,
    files: usize,
    dirs: usize,
}

impl Default for Tree {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `name` can be the name of an entry of a folder.
fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_BYTES
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
}

/// Folders before files, each by name without regard to the case, then by the name as it is.
fn display_order(a: &DirEntry, b: &DirEntry) -> std::cmp::Ordering {
    let rank = |e: &DirEntry| u8::from(e.kind != EntryKind::Dir);
    rank(a)
        .cmp(&rank(b))
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| a.name.cmp(&b.name))
}

impl Tree {
    /// A tree of the root alone: open, and not listed.
    #[must_use]
    pub fn new() -> Self {
        Self::with_cap(MAX_NODES)
    }

    /// A tree that keeps at most `cap` nodes (the root is one).
    #[must_use]
    pub fn with_cap(cap: usize) -> Self {
        let mut root = Node::new(String::new(), None, Kind::Dir);
        root.expanded = true;
        Self { nodes: vec![root], cap: cap.max(1), files: 0, dirs: 0 }
    }

    /// A tree of these workspace-relative paths, every folder listed and open (the tests' trees, and a workspace given
    /// whole).
    #[must_use]
    pub fn from_paths(paths: &[&str]) -> Self {
        let mut tree = Self::new();
        for path in paths {
            let mut at = tree.root();
            let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
            for (i, part) in parts.iter().enumerate() {
                let kind = if i + 1 == parts.len() { Kind::File } else { Kind::Dir };
                let existing = tree.child_named(at, part);
                at = match existing {
                    Some(id) => id,
                    None => tree.push(at, (*part).to_string(), kind),
                };
                if kind == Kind::Dir {
                    tree.nodes[at].listed = true;
                    tree.nodes[at].expanded = true;
                }
            }
        }
        tree.nodes[0].listed = true;
        tree.sort_all();
        tree
    }

    /// The root.
    #[must_use]
    pub const fn root(&self) -> NodeId {
        0
    }

    /// The node `id`.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id).filter(|n| !n.gone)
    }

    #[must_use]
    pub fn kind(&self, id: NodeId) -> Option<Kind> {
        self.node(id).map(|n| n.kind)
    }

    /// How many files and how many folders the tree holds (the root is none of them).
    #[must_use]
    pub const fn file_count(&self) -> usize {
        self.files
    }

    #[must_use]
    pub const fn dir_count(&self) -> usize {
        self.dirs
    }

    /// The children of a folder, in the order they are shown.
    #[must_use]
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        self.node(id).map_or(&[], |n| n.children.as_slice())
    }

    /// Whether the folder's entries are known.
    #[must_use]
    pub fn is_listed(&self, id: NodeId) -> bool {
        self.node(id).is_some_and(|n| n.listed)
    }

    #[must_use]
    pub fn is_expanded(&self, id: NodeId) -> bool {
        self.node(id).is_some_and(|n| n.expanded)
    }

    /// Opens or closes a folder (a file is none).
    pub fn set_expanded(&mut self, id: NodeId, expanded: bool) {
        if let Some(n) = self.nodes.get_mut(id).filter(|n| n.kind == Kind::Dir && !n.gone) {
            n.expanded = expanded;
        }
    }

    /// A listing of the folder is under way (or is over).
    pub fn set_listing(&mut self, id: NodeId, listing: bool) {
        if let Some(n) = self.nodes.get_mut(id).filter(|n| n.kind == Kind::Dir && !n.gone) {
            n.listing = listing;
        }
    }

    /// How many lines the file has.
    pub fn set_lines(&mut self, id: NodeId, lines: u32) {
        if let Some(n) = self.nodes.get_mut(id).filter(|n| n.kind == Kind::File && !n.gone) {
            n.lines = Some(lines);
        }
    }

    /// The workspace-relative path of a node (`""` for the root), `/` between the names.
    #[must_use]
    pub fn path(&self, id: NodeId) -> String {
        let mut names: Vec<&str> = Vec::new();
        let mut at = Some(id);
        while let Some(i) = at {
            let Some(n) = self.nodes.get(i) else {
                break;
            };
            if n.parent.is_some() {
                names.push(n.name.as_str());
            }
            at = n.parent;
        }
        names.reverse();
        names.join("/")
    }

    /// The node at `path`, where the tree knows it (`""` is the root; an empty part, `.` or `..` is no path).
    #[must_use]
    pub fn find(&self, path: &str) -> Option<NodeId> {
        let mut at = self.root();
        if path.is_empty() {
            return Some(at);
        }
        for part in path.split('/') {
            if part.is_empty() || part == "." || part == ".." {
                return None;
            }
            at = self.child_named(at, part)?;
        }
        Some(at)
    }

    fn child_named(&self, dir: NodeId, name: &str) -> Option<NodeId> {
        self.children(dir).iter().copied().find(|&c| self.nodes[c].name == name)
    }

    fn push(&mut self, parent: NodeId, name: String, kind: Kind) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node::new(name, Some(parent), kind));
        self.nodes[parent].children.push(id);
        match kind {
            Kind::Dir => self.dirs += 1,
            Kind::File => self.files += 1,
        }
        id
    }

    /// Orders the children of every folder as they are shown.
    fn sort_all(&mut self) {
        for id in 0..self.nodes.len() {
            let mut kids = std::mem::take(&mut self.nodes[id].children);
            kids.sort_by(|&a, &b| {
                let (x, y) = (&self.nodes[a], &self.nodes[b]);
                u8::from(x.kind == Kind::File)
                    .cmp(&u8::from(y.kind == Kind::File))
                    .then_with(|| x.name.to_lowercase().cmp(&y.name.to_lowercase()))
                    .then_with(|| x.name.cmp(&y.name))
            });
            self.nodes[id].children = kids;
        }
    }

    /// Takes a node and everything under it out of the tree (the counts follow).
    fn drop_subtree(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(i) = stack.pop() {
            if self.nodes[i].gone {
                continue;
            }
            self.nodes[i].gone = true;
            match self.nodes[i].kind {
                Kind::Dir => self.dirs = self.dirs.saturating_sub(1),
                Kind::File => self.files = self.files.saturating_sub(1),
            }
            stack.extend(std::mem::take(&mut self.nodes[i].children));
        }
    }

    /// Takes a listing of the folder `dir`: what `rules` leave in becomes its children, folders before files; what was
    /// listed before and is not any more goes (with all under it), what is still there keeps what it was told (its listing,
    /// whether it is open, its lines). The folder is listed from now on.
    pub fn list(&mut self, dir: NodeId, entries: &[DirEntry], rules: &Rules) -> Listed {
        let mut out = Listed::default();
        if self.kind(dir) != Some(Kind::Dir) {
            return out;
        }
        let beside_manifest = entries
            .iter()
            .any(|e| e.kind == EntryKind::File && symbols_source::is_manifest_name(&e.name));
        let depth = self.depth(dir);
        let mut kept: Vec<&DirEntry> = Vec::new();
        for e in entries {
            let shown = is_name(&e.name)
                && match e.kind {
                    EntryKind::Dir => {
                        depth < rules.exclusions.max_depth && !rules.exclusions.skips_dir(&e.name, beside_manifest)
                    }
                    EntryKind::File => !rules.exclusions.skips_file(&e.name) && is_reviewable(&e.name),
                    EntryKind::Other => false,
                };
            if shown {
                kept.push(e);
            } else {
                out.left_out += 1;
            }
        }
        kept.sort_by(|a, b| display_order(a, b));
        let old: HashMap<(String, bool), NodeId> = self
            .children(dir)
            .iter()
            .map(|&c| ((self.nodes[c].name.clone(), self.nodes[c].kind == Kind::Dir), c))
            .collect();
        let room = self.cap.saturating_sub(self.nodes.len());
        let mut children = Vec::with_capacity(kept.len());
        let mut reused: HashSet<NodeId> = HashSet::new();
        for e in kept {
            let is_dir = e.kind == EntryKind::Dir;
            if let Some(&id) = old.get(&(e.name.clone(), is_dir)) {
                self.nodes[id].size = e.size;
                self.nodes[id].mtime_ms = e.mtime_ms;
                children.push(id);
                reused.insert(id);
            } else if out.added < room {
                let id = self.nodes.len();
                let mut n = Node::new(e.name.clone(), Some(dir), if is_dir { Kind::Dir } else { Kind::File });
                n.size = e.size;
                n.mtime_ms = e.mtime_ms;
                self.nodes.push(n);
                if is_dir {
                    self.dirs += 1;
                } else {
                    self.files += 1;
                }
                children.push(id);
                out.added += 1;
            } else {
                out.capped = true;
            }
        }
        let vanished: Vec<NodeId> = old.values().copied().filter(|id| !reused.contains(id)).collect();
        self.nodes[dir].children = Vec::new();
        for id in vanished {
            self.drop_subtree(id);
        }
        self.nodes[dir].children = children;
        self.nodes[dir].listed = true;
        self.nodes[dir].listing = false;
        out
    }

    /// How deep a node is (the root's children are 1).
    fn depth(&self, id: NodeId) -> usize {
        let mut depth = 0;
        let mut at = self.nodes.get(id).and_then(|n| n.parent);
        while let Some(i) = at {
            depth += 1;
            at = self.nodes[i].parent;
        }
        depth
    }

    /// A click on a row: a folder opens or closes, a file is left to the caller.
    pub fn toggle(&mut self, id: NodeId) -> Toggle {
        match self.kind(id) {
            Some(Kind::Dir) => {
                let was = self.nodes[id].expanded;
                self.nodes[id].expanded = !was;
                if was {
                    Toggle::Closed
                } else {
                    Toggle::Opened { needs_listing: !self.nodes[id].listed }
                }
            }
            _ => Toggle::File,
        }
    }

    /// Opens the folders on the way to `path`; the folders that must still be listed before the way can go on - at most
    /// one, the first from the root, because the next is not known until it is listed (empty: the way is open, or `path` is
    /// not in the tree).
    pub fn reveal(&mut self, path: &str) -> Vec<String> {
        let mut at = self.root();
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        for (i, part) in parts.iter().enumerate() {
            if self.kind(at) != Some(Kind::Dir) {
                break;
            }
            self.nodes[at].expanded = true;
            if !self.nodes[at].listed {
                return vec![self.path(at)];
            }
            let Some(next) = self.child_named(at, part) else {
                return Vec::new();
            };
            at = next;
            if i + 1 == parts.len() {
                break;
            }
        }
        Vec::new()
    }

    /// The first file in the order it is shown: the first file of the root, else the first file of the first listed
    /// folder that holds one (none: no file is known yet).
    #[must_use]
    pub fn first_file(&self) -> Option<NodeId> {
        let root_file = self.children(self.root()).iter().copied().find(|&c| self.nodes[c].kind == Kind::File);
        if root_file.is_some() {
            return root_file;
        }
        let mut stack: Vec<NodeId> = self.dirs_of(self.root());
        while let Some(dir) = stack.pop() {
            if !self.nodes[dir].listed {
                continue;
            }
            if let Some(f) = self.children(dir).iter().copied().find(|&c| self.nodes[c].kind == Kind::File) {
                return Some(f);
            }
            stack.extend(self.dirs_of(dir));
        }
        None
    }

    /// The folders under `dir`, last first (what a stack pops in the order they are shown).
    fn dirs_of(&self, dir: NodeId) -> Vec<NodeId> {
        self.children(dir).iter().rev().copied().filter(|&c| self.nodes[c].kind == Kind::Dir).collect()
    }

    /// The paths of the files of the folders listed so far, in the order they are shown whether the folders are open or
    /// not, at most `limit`.
    #[must_use]
    pub fn file_paths(&self, limit: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.children(self.root()).iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            if out.len() >= limit {
                break;
            }
            match self.nodes[id].kind {
                Kind::File => out.push(self.path(id)),
                Kind::Dir => stack.extend(self.children(id).iter().rev().copied()),
            }
        }
        out
    }

    /// The folders whose entries are not known yet.
    #[must_use]
    pub fn unlisted_dirs(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == Kind::Dir && !n.listed && !n.gone)
            .map(|(id, _)| id)
            .collect()
    }

    /// Opens every folder.
    pub fn expand_all(&mut self) {
        for n in self.nodes.iter_mut().filter(|n| n.kind == Kind::Dir && !n.gone) {
            n.expanded = true;
        }
    }

    /// The rows of the folders that are open, in the order they are shown.
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        // the nodes still to show, last first: (id, depth)
        let mut stack: Vec<(NodeId, usize)> = self.children(self.root()).iter().rev().map(|&c| (c, 0)).collect();
        while let Some((id, depth)) = stack.pop() {
            let n = &self.nodes[id];
            rows.push(Row {
                id,
                depth,
                name: n.name.clone(),
                kind: n.kind,
                expanded: n.kind == Kind::Dir && n.expanded,
                listed: n.kind == Kind::Dir && n.listed,
                loading: n.kind == Kind::Dir && n.expanded && n.listing && !n.listed,
            });
            if n.kind == Kind::Dir && n.expanded {
                stack.extend(n.children.iter().rev().map(|&c| (c, depth + 1)));
            }
        }
        rows
    }
}

/// The rows to build for a viewport of `viewport_h` px scrolled `scroll_y` px down, of `rows` rows `row_h` px high, with
/// `overscan` rows above and below: never more rows than there are, whatever the numbers are.
#[must_use]
pub fn row_window(rows: usize, scroll_y: f32, viewport_h: f32, row_h: f32, overscan: usize) -> Range<usize> {
    if rows == 0 {
        return 0..0;
    }
    let row_h = if row_h.is_finite() && row_h > 0.0 { row_h } else { ROW_H };
    let viewport_h = if viewport_h.is_finite() && viewport_h > 0.0 { viewport_h } else { 0.0 };
    let scroll_y = if scroll_y.is_finite() && scroll_y > 0.0 { scroll_y } else { 0.0 };
    let shown = (viewport_h / row_h).ceil() as usize;
    let first = ((scroll_y / row_h).floor() as usize).min(rows.saturating_sub(shown));
    let end = (((scroll_y + viewport_h) / row_h).ceil() as usize).max(first + shown).min(rows);
    first.saturating_sub(overscan)..end.saturating_add(overscan).min(rows)
}

#[cfg(test)]
#[path = "tree_test.rs"]
mod tree_test;
