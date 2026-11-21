//! A builder that makes a UI tree directly in its FLAT form.
//!
//! A [`Dom`] is a tree of heap nodes: every parent owns a `Vec<Dom>`, every node with a
//! class owns a box for its rarely used fields and a vector of attributes, every inline
//! property a rule with a vector of one declaration. A rebuild that changes nothing still
//! builds, fingerprints and frees all of it.
//!
//! A [`DomBuilder`] takes the nodes in document order (`open`, the node's classes,
//! properties and handlers, its children, `close`) and appends them to a handful of
//! arenas that all nodes share:
//!
//! - one array of nodes and one of the hierarchy (parent, siblings, last child),
//! - one string for every text, class and id,
//! - one array each for the attributes, the inline properties, the handlers and the
//!   datasets.
//!
//! Nothing is allocated per node: the arenas grow by doubling. As a node is closed its two
//! fingerprint tiers are made ([`crate::diff::DomFingerprints`]: `structure`, `style`, and
//! `subtree_len`), the same numbers [`crate::diff::fingerprint_dom`] computes for the same
//! node of a `Dom`, so the early diff of a rebuild reads them instead of hashing the tree.
//!
//! What the arenas do not express (an accessibility declaration, a menu, SVG data, a
//! virtual view, a localizable text, ...) is given as a whole [`NodeData`]
//! ([`DomBuilder::open_node`], [`DomBuilder::node_data_mut`]): such a node costs what it
//! costs in a `Dom`, the nodes around it stay flat.
//!
//! [`DomBuilder::finish`] hands the result back as a [`Dom`]: its `root` is the root node
//! (a real `NodeData`), its `css` the root's sheets, and everything below the root rides
//! in the root's flat payload ([`FlatDom`]). Such a `Dom` is returned from a layout
//! callback or a `VirtualView` callback like any other. Every place that needs the tree
//! expands it ([`Dom::expand_flat`]: `Dom::add_child`, `Dom::set_children`,
//! `Dom::fixup_children_estimated`, and with it every way to a `StyledDom`); the places
//! that know the flat form read it as it is ([`fingerprint_built_dom`],
//! [`Dom::into_fast_dom`]).

use alloc::{boxed::Box, string::String, vec::Vec};
use core::{
    hash::{Hash, Hasher},
    mem::{self, ManuallyDrop},
};

use azul_css::{
    css::{BoxOrStatic, Css},
    dynamic_selector::CssPropertyWithConditions,
    AzString,
};

use super::{
    AttributeType, CssWithNodeId, Dom, EventFilter, FastDom, NodeData, NodeDataExt, NodeFlags,
    NodeType, TabIndex,
};
use crate::{
    callbacks::{CoreCallback, CoreCallbackData},
    diff::{DomFingerprints, PreCascadeTransfers},
    layout_inputs::NodeField,
    refany::{OptionRefAny, RefAny},
    styled_dom::NodeHierarchyItem,
};

#[cfg(all(test, feature = "std"))]
std::thread_local! {
    /// How many nodes [`FlatDom::fingerprints_with_root`] hashed on this thread: one per
    /// call (the root, which the `Dom` holds), whatever the size of the tree.
    pub(crate) static NODES_HASHED_AT_FINGERPRINT: core::cell::Cell<u64> =
        const { core::cell::Cell::new(0) };
    /// How many nodes a builder hashed on this thread as they were closed.
    pub(crate) static NODES_HASHED_AT_CLOSE: core::cell::Cell<u64> =
        const { core::cell::Cell::new(0) };
}

/// The text a shared text node shows to a question about node TYPES alone.
static EMPTY_TEXT: AzString = AzString::from_const_str("");

/// An index into an arena, as the flat form stores it.
fn idx(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// A run of consecutive entries of an arena.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FlatRange {
    start: u32,
    len: u32,
}

impl FlatRange {
    /// The entries of `items` the range names (none when it reaches past the end).
    fn of<T>(self, items: &[T]) -> &[T] {
        let start = self.start as usize;
        items.get(start..start + self.len as usize).unwrap_or(&[])
    }

    /// One more entry, which stands at `at`: the entries of a node are consecutive
    /// because only the node pushed last takes any.
    fn grow(&mut self, at: usize) {
        if self.len == 0 {
            self.start = idx(at);
        }
        self.len += 1;
    }
}

/// What a node of the flat form is.
#[derive(Debug, Clone)]
enum FlatKind {
    /// Its type, by value (a `div`, an image, a text given as a whole `NodeType`).
    Element(NodeType),
    /// A text whose characters are a run of the shared string. Never localizable.
    Text(FlatRange),
    /// A node given whole: `full[index]`.
    Full(u32),
}

/// One node of the flat form.
#[derive(Debug, Clone)]
struct FlatNode {
    kind: FlatKind,
    attrs: FlatRange,
    props: FlatRange,
    callbacks: FlatRange,
    flags: NodeFlags,
    key: Option<u64>,
    /// Index + 1 into the datasets, 0 = none.
    dataset: u32,
}

impl FlatNode {
    fn new(kind: FlatKind) -> Self {
        Self {
            kind,
            attrs: FlatRange { start: 0, len: 0 },
            props: FlatRange { start: 0, len: 0 },
            callbacks: FlatRange { start: 0, len: 0 },
            flags: NodeFlags::new(),
            key: None,
            dataset: 0,
        }
    }
}

/// An attribute of a flat node: an id or a class as a run of the shared string, anything
/// else by value.
#[derive(Debug, Clone)]
enum FlatAttr {
    Id(FlatRange),
    Class(FlatRange),
    Other(AttributeType),
}

/// The two fingerprint tiers of one node, and the two lists some nodes are in.
#[derive(Debug, Clone, Copy, Default)]
struct NodeHashes {
    structure: u64,
    style: u64,
    text_shape: Option<u64>,
    sheets: Option<u64>,
}

/// `len` sheets as the style tier folds them (`fingerprint_dom`: `node_style_hash`,
/// `node_sheets_hash`).
fn hash_sheets<'a, H: Hasher, I: Iterator<Item = &'a Css>>(len: usize, sheets: I, h: &mut H) {
    len.hash(h);
    for css in sheets {
        for rule in css.rules.as_ref() {
            rule.path.hash(h);
            for decl in rule.declarations.as_ref() {
                decl.hash(h);
            }
            rule.conditions.as_slice().hash(h);
            rule.priority.hash(h);
        }
    }
}

/// The structure tier of a node held as a `NodeData`: `fingerprint_dom`'s
/// `node_structure_hash`.
fn full_structure_hash(node: &NodeData, child_count: usize) -> u64 {
    let mut h = crate::hash::DefaultHasher::new();
    for field in NodeField::ALL {
        field.hash_shape(node, &mut h);
    }
    child_count.hash(&mut h);
    h.finish()
}

/// The text shape of a text node held as a `NodeData`: `fingerprint_dom`'s
/// `text_shape_hash`.
fn full_text_shape_hash(node: &NodeData, child_count: usize) -> Option<u64> {
    let NodeType::Text(text) = node.get_node_type() else {
        return None;
    };
    let mut h = crate::hash::DefaultHasher::new();
    for field in NodeField::ALL {
        if field == NodeField::NodeType {
            0x7E87_5A9E_u32.hash(&mut h);
            text.as_ref().as_str().is_empty().hash(&mut h);
            text.as_ref().is_localizable().hash(&mut h);
        } else {
            field.hash_shape(node, &mut h);
        }
    }
    child_count.hash(&mut h);
    Some(h.finish())
}

/// The style tier of a node held as a `NodeData` with `len` sheets attached:
/// `fingerprint_dom`'s `node_style_hash`.
fn full_style_hash<'a, I: Iterator<Item = &'a Css>>(node: &NodeData, len: usize, sheets: I) -> u64 {
    let mut h = crate::hash::DefaultHasher::new();
    crate::diff::hash_inline_style(&node.style, &mut h);
    hash_sheets(len, sheets, &mut h);
    h.finish()
}

/// The sheet half of the style tier: `fingerprint_dom`'s `node_sheets_hash`.
fn sheets_hash<'a, I: Iterator<Item = &'a Css>>(len: usize, sheets: I) -> u64 {
    let mut h = crate::hash::DefaultHasher::new();
    hash_sheets(len, sheets, &mut h);
    h.finish()
}

/// A `NodeData` made of the parts a flat node holds, by the calls a `Dom` is built with:
/// the attributes as `NodeData::add_class` / `add_id` / `Dom::with_attribute` leave them,
/// one inline rule per property (`NodeData::add_css_property`), the handlers in order,
/// the key and the dataset.
fn node_of_parts(
    node_type: NodeType,
    flags: NodeFlags,
    attrs: Vec<AttributeType>,
    props: impl Iterator<Item = CssPropertyWithConditions>,
    callbacks: Vec<CoreCallbackData>,
    key: Option<u64>,
    dataset: Option<RefAny>,
) -> NodeData {
    let mut node = NodeData::create_node(node_type);
    node.flags = flags;
    if !attrs.is_empty() {
        node.extra_fields_mut().attributes = attrs.into();
    }
    for prop in props {
        node.add_css_property(prop);
    }
    if !callbacks.is_empty() {
        node.callbacks = callbacks.into();
    }
    if let Some(key) = key {
        node.extra_fields_mut().key = Some(key);
    }
    if let Some(dataset) = dataset {
        node.set_dataset(OptionRefAny::Some(dataset));
    }
    node
}

/// A copy of `node` without the flat tree it carries: the root of a carrying `Dom` as a
/// node of its own. (`NodeData::clone` copies the carried tree with it.)
fn node_without_flat(node: &NodeData) -> NodeData {
    let extra = node.extra_fields().map(|extra| {
        // Field by field: a field added to `NodeDataExt` has to be named here.
        Box::new(NodeDataExt {
            attributes: extra.attributes.clone(),
            virtual_view: extra.virtual_view.clone(),
            dataset: extra.dataset.clone(),
            svg_data: extra.svg_data.clone(),
            menu_bar: extra.menu_bar.clone(),
            context_menu: extra.context_menu.clone(),
            key: extra.key,
            marker: extra.marker.clone(),
            dataset_merge_callback: extra.dataset_merge_callback.clone(),
            component_origin: extra.component_origin.clone(),
            animation_callbacks: extra.animation_callbacks.clone(),
            l10n_key: extra.l10n_key.clone(),
            part_of_parent: extra.part_of_parent,
            flat: None,
        })
    });
    NodeData {
        node_type: node.node_type.to_library_owned_nodetype(),
        callbacks: node.callbacks.clone(),
        style: node.style.clone(),
        flags: node.flags,
        accessibility: node.accessibility.clone(),
        extra,
        fluent_args: node.fluent_args.clone(),
    }
}

/// A text node of `text`, whose characters are held as the input rule has them already.
fn text_node_type(text: &str) -> NodeType {
    NodeType::Text(BoxOrStatic::heap(AzString::from_string(String::from(text))))
}

/// A UI tree in its flat form: what a [`DomBuilder`] makes and a [`Dom`] carries below its
/// root (see the module's text).
///
/// Node 0 is the root. Once the tree is carried by a `Dom` the root's data is the `Dom`'s
/// `root` and its sheets are the `Dom`'s `css`: the entry of node 0 in here is a
/// placeholder, and the fingerprints of node 0 are made from the `Dom` when they are asked
/// for ([`Self::fingerprints_with_root`]).
#[derive(Debug, Clone)]
pub struct FlatDom {
    nodes: Vec<FlatNode>,
    hierarchy: Vec<NodeHierarchyItem>,
    /// Every text, class and id, one after the other.
    text: String,
    attrs: Vec<FlatAttr>,
    props: Vec<CssPropertyWithConditions>,
    callbacks: Vec<CoreCallbackData>,
    datasets: Vec<RefAny>,
    /// The nodes given whole, in the order of their nodes.
    full: Vec<NodeData>,
    /// `(node, sheet)` for every attached sheet, ascending by node, a node's in the order
    /// they were attached.
    sheets: Vec<(u32, Css)>,
    /// Per node: `structure`, `style`, `subtree_len`; `text_shapes` and `sheets` for the
    /// nodes below the root. The two folds are made when the fingerprints are asked for.
    fingerprints: DomFingerprints,
    /// How many children the root has.
    root_children: u32,
}

impl Default for FlatDom {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            hierarchy: Vec::new(),
            text: String::new(),
            attrs: Vec::new(),
            props: Vec::new(),
            callbacks: Vec::new(),
            datasets: Vec::new(),
            full: Vec::new(),
            sheets: Vec::new(),
            fingerprints: DomFingerprints {
                structure: Vec::new(),
                style: Vec::new(),
                structure_root: 0,
                style_root: 0,
                text_shapes: Vec::new(),
                subtree_len: Vec::new(),
                sheets: Vec::new(),
            },
            root_children: 0,
        }
    }
}

// Two flat trees compare by what their fingerprints compare of the nodes below the root
// (the root is the `Dom`'s): enough for `NodeDataExt`'s comparisons, which only have to
// tell two carried trees apart.
impl FlatDom {
    fn cmp_key(&self) -> (usize, &[u64], &[u64]) {
        (
            self.nodes.len(),
            self.fingerprints.structure.get(1..).unwrap_or(&[]),
            self.fingerprints.style.get(1..).unwrap_or(&[]),
        )
    }
}
impl PartialEq for FlatDom {
    fn eq(&self, other: &Self) -> bool {
        self.cmp_key() == other.cmp_key()
    }
}
impl Eq for FlatDom {}
impl PartialOrd for FlatDom {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for FlatDom {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.cmp_key().cmp(&other.cmp_key())
    }
}
impl Hash for FlatDom {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.cmp_key().hash(state);
    }
}

impl FlatDom {
    /// How many nodes the tree holds, the root included.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The run `range` of the shared string.
    fn str_of(&self, range: FlatRange) -> &str {
        let start = range.start as usize;
        self.text.get(start..start + range.len as usize).unwrap_or("")
    }

    /// The dataset of `node`, if it has one.
    fn dataset_of(&self, node: &FlatNode) -> Option<&RefAny> {
        (node.dataset as usize)
            .checked_sub(1)
            .and_then(|i| self.datasets.get(i))
    }

    /// The sheets attached to node `i`.
    fn sheets_of(&self, i: usize) -> &[(u32, Css)] {
        if self.sheets.is_empty() {
            return &[];
        }
        let from = self.sheets.partition_point(|(owner, _)| (*owner as usize) < i);
        let to = self.sheets.partition_point(|(owner, _)| (*owner as usize) <= i);
        self.sheets.get(from..to).unwrap_or(&[])
    }

    /// The attributes of `node` as a `NodeData` holds them.
    fn attributes_of(&self, node: &FlatNode) -> Vec<AttributeType> {
        node.attrs
            .of(&self.attrs)
            .iter()
            .map(|attr| match attr {
                FlatAttr::Id(range) => {
                    AttributeType::Id(AzString::from_string(String::from(self.str_of(*range))))
                }
                FlatAttr::Class(range) => {
                    AttributeType::Class(AzString::from_string(String::from(self.str_of(*range))))
                }
                FlatAttr::Other(other) => other.clone(),
            })
            .collect()
    }

    /// Node `i` as a `NodeData`, copied out of the arenas: the node a `Dom` built by the
    /// same calls holds. `None` past the last node. (Node 0 of a tree a `Dom` carries is
    /// the `Dom`'s `root`: [`Dom::node_data_at`].)
    #[must_use]
    pub fn node_data(&self, i: usize) -> Option<NodeData> {
        let node = self.nodes.get(i)?;
        let node_type = match &node.kind {
            FlatKind::Full(index) => return self.full.get(*index as usize).cloned(),
            FlatKind::Element(node_type) => node_type.clone(),
            FlatKind::Text(range) => text_node_type(self.str_of(*range)),
        };
        Some(node_of_parts(
            node_type,
            node.flags,
            self.attributes_of(node),
            node.props.of(&self.props).iter().cloned(),
            node.callbacks.of(&self.callbacks).to_vec(),
            node.key,
            self.dataset_of(node).cloned(),
        ))
    }

    /// The structure tier of the compact node `node`, whose type and flags `typed` holds:
    /// `fingerprint_dom`'s `node_structure_hash` (and, with `text_shape`, its
    /// `text_shape_hash`) of the `NodeData` the node stands for. The rows of the node's
    /// table whose values the arenas hold are hashed from the arenas, as
    /// `NodeField::hash_shape` hashes them from a `NodeData`; every other row is
    /// `hash_shape`'s over `typed`, which holds none of them.
    fn compact_shape_hash(
        &self,
        node: &FlatNode,
        typed: &NodeData,
        child_count: usize,
        text_shape: Option<(bool, bool)>,
    ) -> u64 {
        let mut h = crate::hash::DefaultHasher::new();
        for field in NodeField::ALL {
            match (field, text_shape) {
                (NodeField::NodeType, Some((is_empty, localizable))) => {
                    0x7E87_5A9E_u32.hash(&mut h);
                    is_empty.hash(&mut h);
                    localizable.hash(&mut h);
                }
                (NodeField::Callbacks, _) => {
                    let callbacks = node.callbacks.of(&self.callbacks);
                    callbacks.len().hash(&mut h);
                    for callback in callbacks {
                        callback.event.hash(&mut h);
                    }
                }
                (NodeField::Attributes, _) => {
                    for attr in node.attrs.of(&self.attrs) {
                        match attr {
                            FlatAttr::Id(range) => {
                                1u8.hash(&mut h);
                                self.str_of(*range).hash(&mut h);
                            }
                            FlatAttr::Class(range) => {
                                2u8.hash(&mut h);
                                self.str_of(*range).hash(&mut h);
                            }
                            FlatAttr::Other(other) => {
                                3u8.hash(&mut h);
                                other.hash(&mut h);
                            }
                        }
                    }
                }
                (NodeField::Key, _) => node.key.hash(&mut h),
                (other, _) => other.hash_shape(typed, &mut h),
            }
        }
        child_count.hash(&mut h);
        h.finish()
    }

    /// The style tier of the compact node `node` with `sheets` attached:
    /// `fingerprint_dom`'s `node_style_hash` of a node whose style is one inline rule per
    /// property (the rule's conditions, then its one static declaration).
    fn compact_style_hash(&self, node: &FlatNode, sheets: &[(u32, Css)]) -> u64 {
        let mut h = crate::hash::DefaultHasher::new();
        for prop in node.props.of(&self.props) {
            prop.apply_if.as_slice().hash(&mut h);
            prop.property.hash(&mut h);
        }
        hash_sheets(sheets.len(), sheets.iter().map(|(_, css)| css), &mut h);
        h.finish()
    }

    /// The hashes of the compact node `i` (an element or a shared text), whose type and
    /// flags `typed` holds.
    fn compact_hashes(&self, i: usize, typed: &NodeData, child_count: usize) -> NodeHashes {
        let Some(node) = self.nodes.get(i) else {
            return NodeHashes::default();
        };
        let sheets = self.sheets_of(i);
        let text_shape = match typed.get_node_type() {
            NodeType::Text(text) => Some((
                text.as_ref().as_str().is_empty(),
                text.as_ref().is_localizable(),
            )),
            _ => None,
        };
        NodeHashes {
            structure: self.compact_shape_hash(node, typed, child_count, None),
            style: self.compact_style_hash(node, sheets),
            text_shape: text_shape
                .map(|shape| self.compact_shape_hash(node, typed, child_count, Some(shape))),
            sheets: (!sheets.is_empty())
                .then(|| sheets_hash(sheets.len(), sheets.iter().map(|(_, css)| css))),
        }
    }

    /// The hashes of node `i`, which has `child_count` children: what `fingerprint_dom`
    /// makes of the same node of a `Dom`.
    fn hash_node(&mut self, i: usize, child_count: usize) -> NodeHashes {
        #[cfg(all(test, feature = "std"))]
        NODES_HASHED_AT_CLOSE.with(|c| c.set(c.get() + 1));
        let Some(node) = self.nodes.get_mut(i) else {
            return NodeHashes::default();
        };
        let flags = node.flags;
        match &mut node.kind {
            FlatKind::Full(index) => {
                let index = *index as usize;
                let sheets = self.sheets_of(i);
                let Some(full) = self.full.get(index) else {
                    return NodeHashes::default();
                };
                NodeHashes {
                    structure: full_structure_hash(full, child_count),
                    style: full_style_hash(full, sheets.len(), sheets.iter().map(|(_, css)| css)),
                    text_shape: full_text_shape_hash(full, child_count),
                    sheets: (!sheets.is_empty())
                        .then(|| sheets_hash(sheets.len(), sheets.iter().map(|(_, css)| css))),
                }
            }
            FlatKind::Element(node_type) => {
                // The type moves into a node the table can read and back: no copy of an
                // image or of a text given by value.
                let mut typed = NodeData::create_node(mem::replace(node_type, NodeType::Div));
                typed.flags = flags;
                let hashes = self.compact_hashes(i, &typed, child_count);
                let node_type = mem::replace(&mut typed.node_type, NodeType::Div);
                if let Some(FlatNode {
                    kind: FlatKind::Element(slot),
                    ..
                }) = self.nodes.get_mut(i)
                {
                    *slot = node_type;
                }
                hashes
            }
            FlatKind::Text(range) => {
                let range = *range;
                let text: &str = self.str_of(range);
                // SAFETY: `view` and `typed` borrow the characters of `self.text` under a
                // lifetime they do not have. Both are locals of this block and are read
                // only by the hash functions below, which keep nothing of what they read;
                // `self` is borrowed for the whole block, so the string neither moves nor
                // changes while they live. Neither frees the bytes when dropped: `view`
                // is a `from_const_str` string (no destructor) and `typed` holds it as
                // `BoxOrStatic::Static`.
                let view =
                    AzString::from_const_str(unsafe { mem::transmute::<&str, &'static str>(text) });
                let mut typed =
                    NodeData::create_node(NodeType::Text(BoxOrStatic::Static(&raw const view)));
                typed.flags = flags;
                let hashes = self.compact_hashes(i, &typed, child_count);
                drop(typed);
                drop(view);
                hashes
            }
        }
    }

    /// The values of node `i` (below the root) that every build makes anew and a kept DOM
    /// takes over: `NodeField::values_to_carry` of the `NodeData` the node stands for.
    fn values_to_carry(&self, i: usize) -> Option<NodeData> {
        let node = self.nodes.get(i)?;
        let node_type = match &node.kind {
            FlatKind::Full(index) => {
                return NodeField::values_to_carry(self.full.get(*index as usize)?);
            }
            FlatKind::Element(node_type) => Some(node_type),
            FlatKind::Text(_) => None,
        };
        let mut values: Option<NodeData> = None;
        // The rows that carry, in the table's order: the type (an image's callback), the
        // handlers, the dataset. A compact node holds no other carried value.
        if let Some(NodeType::Image(_)) = node_type {
            let typed = NodeData::create_node(node_type.cloned().unwrap_or(NodeType::Div));
            if NodeField::NodeType.holds_value(&typed) {
                NodeField::NodeType.carry(&typed, values.get_or_insert_with(NodeData::create_div));
            }
        }
        let callbacks = node.callbacks.of(&self.callbacks);
        if !callbacks.is_empty() {
            values.get_or_insert_with(NodeData::create_div).callbacks = callbacks.to_vec().into();
        }
        if let Some(dataset) = self.dataset_of(node) {
            values
                .get_or_insert_with(NodeData::create_div)
                .set_dataset(OptionRefAny::Some(dataset.clone()));
        }
        values
    }

    /// The fingerprints of the tree whose root is `root` with `root_sheets` attached, and
    /// the values a kept DOM takes over from it: what `fingerprint_dom` answers for the
    /// `Dom` the same calls build.
    ///
    /// One node is hashed, the root; the tiers of every other node were made as the node
    /// was closed and are copied (28 bytes per node). The carried values are made here:
    /// one node per node that has a handler, a dataset or an image callback.
    #[must_use]
    pub fn fingerprints_with_root(
        &self,
        root: &NodeData,
        root_sheets: &[Css],
    ) -> (DomFingerprints, PreCascadeTransfers) {
        #[cfg(all(test, feature = "std"))]
        NODES_HASHED_AT_FINGERPRINT.with(|c| c.set(c.get() + 1));
        let mut fp = self.fingerprints.clone();
        let child_count = self.root_children as usize;
        if let Some(slot) = fp.structure.first_mut() {
            *slot = full_structure_hash(root, child_count);
        }
        if let Some(slot) = fp.style.first_mut() {
            *slot = full_style_hash(root, root_sheets.len(), root_sheets.iter());
        }
        if let Some(shape) = full_text_shape_hash(root, child_count) {
            fp.text_shapes.insert(0, (0, shape));
        }
        if !root_sheets.is_empty() {
            fp.sheets
                .insert(0, (0, sheets_hash(root_sheets.len(), root_sheets.iter())));
        }

        let mut hs = crate::hash::DefaultHasher::new();
        for v in &fp.structure {
            v.hash(&mut hs);
        }
        fp.structure_root = hs.finish();
        let mut hy = crate::hash::DefaultHasher::new();
        for v in &fp.style {
            v.hash(&mut hy);
        }
        fp.style_root = hy.finish();

        let mut transfers = PreCascadeTransfers::default();
        if let Some(values) = NodeField::values_to_carry(root) {
            transfers.nodes.push((0, values));
        }
        for i in 1..self.nodes.len() {
            if let Some(values) = self.values_to_carry(i) {
                transfers.nodes.push((i, values));
            }
        }
        (fp, transfers)
    }

    /// The nodes of the tree, one after the other, each moved out of the arenas.
    fn drain(self) -> (FlatDrain, Vec<NodeHierarchyItem>, Vec<u32>) {
        let subtree_len = self.fingerprints.subtree_len;
        (
            FlatDrain {
                nodes: self.nodes.into_iter(),
                text: self.text,
                attrs: self.attrs.into_iter(),
                props: self.props.into_iter(),
                callbacks: self.callbacks.into_iter(),
                datasets: self.datasets.into_iter(),
                full: self.full.into_iter(),
                sheets: self.sheets.into_iter().peekable(),
                next: 0,
            },
            self.hierarchy,
            subtree_len,
        )
    }

    /// The children of the root as `Dom`s: the tree below the root, as the same calls
    /// build it with `Dom::with_child`.
    fn into_children(self) -> Vec<Dom> {
        fn children_of(drain: &mut FlatDrain, subtree_len: &[u32], parent: usize) -> Vec<Dom> {
            let len = subtree_len.get(parent).copied().unwrap_or(1).max(1) as usize;
            let end = parent + len;
            let mut out = Vec::new();
            let mut i = parent + 1;
            while i < end {
                let Some((root, css)) = drain.next_node() else {
                    break;
                };
                let len = subtree_len.get(i).copied().unwrap_or(1).max(1) as usize;
                let children = children_of(drain, subtree_len, i);
                out.push(Dom {
                    root,
                    children: children.into(),
                    css: css.into(),
                    estimated_total_children: len - 1,
                });
                i += len;
            }
            out
        }

        let (mut drain, _, subtree_len) = self.drain();
        // Node 0 is the placeholder of the root, which the `Dom` holds.
        drop(drain.next_node());
        children_of(&mut drain, &subtree_len, 0)
    }

    /// The tree as the two parallel arenas `StyledDom::create_from_fast_dom` takes, under
    /// `root` with `root_css` attached. The sheets come in the order the tree path takes
    /// them out of a `Dom` (`take_css_from_dom`: a node's descendants' first, then its
    /// own), so the merged sheet is the same rule for rule.
    fn into_fast_dom(self, root: NodeData, root_css: Vec<Css>) -> FastDom {
        let count = self.nodes.len().max(1);
        let (mut drain, hierarchy, subtree_len) = self.drain();
        let mut node_data = Vec::with_capacity(count);
        let mut css: Vec<(usize, Css)> = Vec::new();
        drop(drain.next_node());
        node_data.push(root);
        let mut i = 1;
        while let Some((node, sheets)) = drain.next_node() {
            node_data.push(node);
            css.extend(sheets.into_iter().map(|sheet| (i, sheet)));
            i += 1;
        }
        css.extend(root_css.into_iter().map(|sheet| (0, sheet)));
        // Post-order of the owners: the node whose subtree ends first comes first, and of
        // two that end at the same node the inner one. Stable: a node's own stay in order.
        let last_of = |owner: usize| {
            owner + (subtree_len.get(owner).copied().unwrap_or(1).max(1) as usize) - 1
        };
        css.sort_by(|(a, _), (b, _)| last_of(*a).cmp(&last_of(*b)).then(b.cmp(a)));
        let hierarchy = if hierarchy.is_empty() {
            alloc::vec![NodeHierarchyItem::zeroed()]
        } else {
            hierarchy
        };
        FastDom {
            node_hierarchy: hierarchy.into(),
            node_data: node_data.into(),
            css: css
                .into_iter()
                .map(|(node_id, css)| CssWithNodeId { node_id, css })
                .collect::<Vec<CssWithNodeId>>()
                .into(),
        }
    }

    /// The tree as a `Dom` that carries it: the root as the `Dom`'s `root`, the root's
    /// sheets as its `css`, the rest in the root's flat payload. A tree of one node is a
    /// plain `Dom`.
    fn into_dom(mut self) -> Dom {
        let Some(mut root) = self.node_data(0) else {
            return Dom::default();
        };
        let own = self.sheets.partition_point(|(owner, _)| *owner == 0);
        let css: Vec<Css> = self.sheets.drain(..own).map(|(_, css)| css).collect();
        let below = self.nodes.len() - 1;
        if below > 0 {
            root.extra_fields_mut().flat = Some(Box::new(self));
        }
        Dom {
            root,
            children: Vec::new().into(),
            css: css.into(),
            estimated_total_children: below,
        }
    }
}

/// The nodes of a [`FlatDom`] in document order, each moved out of the arenas: the
/// arenas' entries are consecutive per node and ascending with the nodes, so one cursor
/// per arena follows the nodes.
struct FlatDrain {
    nodes: alloc::vec::IntoIter<FlatNode>,
    text: String,
    attrs: alloc::vec::IntoIter<FlatAttr>,
    props: alloc::vec::IntoIter<CssPropertyWithConditions>,
    callbacks: alloc::vec::IntoIter<CoreCallbackData>,
    datasets: alloc::vec::IntoIter<RefAny>,
    full: alloc::vec::IntoIter<NodeData>,
    sheets: core::iter::Peekable<alloc::vec::IntoIter<(u32, Css)>>,
    /// The index of the node `next_node` answers next.
    next: usize,
}

impl FlatDrain {
    fn str_of(&self, range: FlatRange) -> &str {
        let start = range.start as usize;
        self.text.get(start..start + range.len as usize).unwrap_or("")
    }

    /// The next node and the sheets attached to it.
    fn next_node(&mut self) -> Option<(NodeData, Vec<Css>)> {
        let node = self.nodes.next()?;
        let index = self.next;
        self.next += 1;
        let mut sheets = Vec::new();
        while let Some((_, css)) = self
            .sheets
            .next_if(|(owner, _)| (*owner as usize) == index)
        {
            sheets.push(css);
        }
        let attrs: Vec<AttributeType> = self
            .attrs
            .by_ref()
            .take(node.attrs.len as usize)
            .collect::<Vec<FlatAttr>>()
            .into_iter()
            .map(|attr| match attr {
                FlatAttr::Id(range) => {
                    AttributeType::Id(AzString::from_string(String::from(self.str_of(range))))
                }
                FlatAttr::Class(range) => {
                    AttributeType::Class(AzString::from_string(String::from(self.str_of(range))))
                }
                FlatAttr::Other(other) => other,
            })
            .collect();
        let props: Vec<CssPropertyWithConditions> =
            self.props.by_ref().take(node.props.len as usize).collect();
        let callbacks: Vec<CoreCallbackData> = self
            .callbacks
            .by_ref()
            .take(node.callbacks.len as usize)
            .collect();
        let dataset = if node.dataset == 0 {
            None
        } else {
            self.datasets.next()
        };
        let node_type = match node.kind {
            FlatKind::Full(_) => {
                return Some((self.full.next().unwrap_or_else(NodeData::create_div), sheets));
            }
            FlatKind::Element(node_type) => node_type,
            FlatKind::Text(range) => text_node_type(self.str_of(range)),
        };
        Some((
            node_of_parts(
                node_type,
                node.flags,
                attrs,
                props.into_iter(),
                callbacks,
                node.key,
                dataset,
            ),
            sheets,
        ))
    }
}

/// A node that is open: its children are the nodes pushed until it is closed.
#[derive(Debug, Clone)]
struct OpenNode {
    index: u32,
    children: u32,
    /// The child pushed last, as the hierarchy stores it (index + 1, 0 = none).
    last_child: u32,
}

/// Which node the next setter writes.
enum Target {
    /// None: no node is open, or the open node has children already.
    None,
    /// The compact node at this index.
    Compact(usize),
    /// The node given whole at this index of `full`.
    Full(usize),
}

/// The state of a [`DomBuilder`].
#[derive(Debug, Clone, Default)]
struct BuilderState {
    flat: FlatDom,
    open: Vec<OpenNode>,
}

impl BuilderState {
    fn push_node(&mut self, node: FlatNode) {
        let index = self.flat.nodes.len();
        let mut item = NodeHierarchyItem::zeroed();
        if let Some(parent) = self.open.last_mut() {
            item.parent = parent.index as usize + 1;
            item.previous_sibling = parent.last_child as usize;
            if let Some(previous) = (parent.last_child as usize)
                .checked_sub(1)
                .and_then(|previous| self.flat.hierarchy.get_mut(previous))
            {
                previous.next_sibling = index + 1;
            }
            if let Some(parent_item) = self.flat.hierarchy.get_mut(parent.index as usize) {
                parent_item.last_child = index + 1;
            }
            parent.last_child = idx(index) + 1;
            parent.children += 1;
        }
        self.flat.nodes.push(node);
        self.flat.hierarchy.push(item);
        self.flat.fingerprints.structure.push(0);
        self.flat.fingerprints.style.push(0);
        self.flat.fingerprints.subtree_len.push(0);
        self.open.push(OpenNode {
            index: idx(index),
            children: 0,
            last_child: 0,
        });
    }

    /// Closes `node`: its subtree ends where the builder stands, and its tiers are made.
    fn finish_node(&mut self, node: &OpenNode) {
        let i = node.index as usize;
        let len = idx(self.flat.nodes.len().saturating_sub(i));
        if let Some(slot) = self.flat.fingerprints.subtree_len.get_mut(i) {
            *slot = len;
        }
        if i == 0 {
            // The root's tiers are made from the `Dom` that carries the tree.
            self.flat.root_children = node.children;
            return;
        }
        let hashes = self.flat.hash_node(i, node.children as usize);
        let fp = &mut self.flat.fingerprints;
        if let Some(slot) = fp.structure.get_mut(i) {
            *slot = hashes.structure;
        }
        if let Some(slot) = fp.style.get_mut(i) {
            *slot = hashes.style;
        }
        if let Some(shape) = hashes.text_shape {
            fp.text_shapes.push((i, shape));
        }
        if let Some(sheets) = hashes.sheets {
            fp.sheets.push((i, sheets));
        }
    }

    fn close(&mut self) {
        // The root stays open until `finish`: a node opened after it was "closed" is one
        // more child of it.
        if self.open.len() <= 1 {
            return;
        }
        if let Some(node) = self.open.pop() {
            self.finish_node(&node);
        }
    }

    fn finish(mut self) -> FlatDom {
        while let Some(node) = self.open.pop() {
            self.finish_node(&node);
        }
        // Nodes close children first: the two lists are in document order only when no
        // listed node has a listed node below it.
        let fp = &mut self.flat.fingerprints;
        if !fp.text_shapes.windows(2).all(|pair| pair[0].0 < pair[1].0) {
            fp.text_shapes.sort_by_key(|(index, _)| *index);
        }
        if !fp.sheets.windows(2).all(|pair| pair[0].0 < pair[1].0) {
            fp.sheets.sort_by_key(|(index, _)| *index);
        }
        self.flat
    }

    /// The node the next setter writes: the open node, while no child of it was pushed.
    fn target(&self) -> Target {
        let Some(top) = self.open.last() else {
            return Target::None;
        };
        let index = top.index as usize;
        if index + 1 != self.flat.nodes.len() {
            debug_assert!(
                false,
                "DomBuilder: a node's classes, properties and handlers are given before its \
                 children"
            );
            return Target::None;
        }
        match self.flat.nodes.get(index).map(|node| &node.kind) {
            Some(FlatKind::Full(full)) => Target::Full(*full as usize),
            Some(_) => Target::Compact(index),
            None => Target::None,
        }
    }

    /// `text` appended to the shared string.
    fn push_str(&mut self, text: &str) -> FlatRange {
        let start = self.flat.text.len();
        self.flat.text.push_str(text);
        FlatRange {
            start: idx(start),
            len: idx(text.len()),
        }
    }

    fn add_attribute(&mut self, attr: AttributeType) {
        match self.target() {
            Target::None => {}
            Target::Full(full) => {
                if let Some(node) = self.flat.full.get_mut(full) {
                    // `Dom::with_attribute`.
                    let mut attrs = node.attributes().clone().into_library_owned_vec();
                    attrs.push(attr);
                    node.set_attributes(attrs.into());
                }
            }
            Target::Compact(index) => {
                let attr = match attr.with_ruled_text() {
                    AttributeType::Id(id) => FlatAttr::Id(self.push_str(id.as_str())),
                    AttributeType::Class(class) => FlatAttr::Class(self.push_str(class.as_str())),
                    other => FlatAttr::Other(other),
                };
                let at = self.flat.attrs.len();
                self.flat.attrs.push(attr);
                if let Some(node) = self.flat.nodes.get_mut(index) {
                    node.attrs.grow(at);
                }
            }
        }
    }

    fn add_class_str(&mut self, class: &str) {
        match self.target() {
            Target::None => {}
            Target::Full(full) => {
                if let Some(node) = self.flat.full.get_mut(full) {
                    node.add_class(AzString::from_string(String::from(class)));
                }
            }
            Target::Compact(index) => {
                let range = self.push_str(class);
                let at = self.flat.attrs.len();
                self.flat.attrs.push(FlatAttr::Class(range));
                if let Some(node) = self.flat.nodes.get_mut(index) {
                    node.attrs.grow(at);
                }
            }
        }
    }

    fn add_id_str(&mut self, id: &str) {
        match self.target() {
            Target::None => {}
            Target::Full(full) => {
                if let Some(node) = self.flat.full.get_mut(full) {
                    node.add_id(AzString::from_string(String::from(id)));
                }
            }
            Target::Compact(index) => {
                let range = self.push_str(id);
                let at = self.flat.attrs.len();
                self.flat.attrs.push(FlatAttr::Id(range));
                if let Some(node) = self.flat.nodes.get_mut(index) {
                    node.attrs.grow(at);
                }
            }
        }
    }

    fn add_css_property(&mut self, prop: CssPropertyWithConditions) {
        match self.target() {
            Target::None => {}
            Target::Full(full) => {
                if let Some(node) = self.flat.full.get_mut(full) {
                    node.add_css_property(prop);
                }
            }
            Target::Compact(index) => {
                let at = self.flat.props.len();
                self.flat.props.push(prop);
                if let Some(node) = self.flat.nodes.get_mut(index) {
                    node.props.grow(at);
                }
            }
        }
    }

    fn add_callback(&mut self, callback: CoreCallbackData) {
        match self.target() {
            Target::None => {}
            Target::Full(full) => {
                if let Some(node) = self.flat.full.get_mut(full) {
                    node.add_callback(callback.event, callback.refany, callback.callback);
                }
            }
            Target::Compact(index) => {
                let at = self.flat.callbacks.len();
                self.flat.callbacks.push(callback);
                if let Some(node) = self.flat.nodes.get_mut(index) {
                    node.callbacks.grow(at);
                }
            }
        }
    }

    fn set_key_hash(&mut self, key: u64) {
        match self.target() {
            Target::None => {}
            Target::Full(full) => {
                if let Some(node) = self.flat.full.get_mut(full) {
                    node.extra_fields_mut().key = Some(key);
                }
            }
            Target::Compact(index) => {
                if let Some(node) = self.flat.nodes.get_mut(index) {
                    node.key = Some(key);
                }
            }
        }
    }

    fn set_tab_index(&mut self, tab_index: TabIndex) {
        match self.target() {
            Target::None => {}
            Target::Full(full) => {
                if let Some(node) = self.flat.full.get_mut(full) {
                    node.set_tab_index(tab_index);
                }
            }
            Target::Compact(index) => {
                if let Some(node) = self.flat.nodes.get_mut(index) {
                    node.flags.set_tab_index(Some(tab_index));
                }
            }
        }
    }

    fn set_contenteditable(&mut self, contenteditable: bool) {
        match self.target() {
            Target::None => {}
            Target::Full(full) => {
                if let Some(node) = self.flat.full.get_mut(full) {
                    node.set_contenteditable(contenteditable);
                }
            }
            Target::Compact(index) => {
                if let Some(node) = self.flat.nodes.get_mut(index) {
                    node.flags.set_contenteditable_mut(contenteditable);
                }
            }
        }
    }

    fn set_dataset(&mut self, data: OptionRefAny) {
        match self.target() {
            Target::None => {}
            Target::Full(full) => {
                if let Some(node) = self.flat.full.get_mut(full) {
                    node.set_dataset(data);
                }
            }
            Target::Compact(index) => {
                let Some(node) = self.flat.nodes.get_mut(index) else {
                    return;
                };
                match data {
                    OptionRefAny::Some(data) => {
                        if let Some(slot) = (node.dataset as usize)
                            .checked_sub(1)
                            .and_then(|at| self.flat.datasets.get_mut(at))
                        {
                            *slot = data;
                        } else {
                            self.flat.datasets.push(data);
                            node.dataset = idx(self.flat.datasets.len());
                        }
                    }
                    OptionRefAny::None => {
                        // The node is the one pushed last: so is its dataset.
                        if node.dataset != 0 {
                            node.dataset = 0;
                            drop(self.flat.datasets.pop());
                        }
                    }
                }
            }
        }
    }

    fn add_component_css(&mut self, css: Css) {
        let index = match self.target() {
            Target::None => return,
            Target::Compact(index) => index,
            Target::Full(_) => self.flat.nodes.len() - 1,
        };
        self.flat.sheets.push((idx(index), css));
    }

    /// The open node as a `NodeData` to write: a compact node is taken out of the arenas
    /// and held whole from here on.
    fn node_data_mut(&mut self) -> Option<&mut NodeData> {
        let full = match self.target() {
            Target::None => return None,
            Target::Full(full) => full,
            Target::Compact(index) => {
                let whole = self.flat.node_data(index)?;
                let node = self.flat.nodes.get_mut(index)?;
                // The node is the one pushed last: its entries are the arenas' tails.
                if node.attrs.len != 0 {
                    self.flat.attrs.truncate(node.attrs.start as usize);
                }
                if node.props.len != 0 {
                    self.flat.props.truncate(node.props.start as usize);
                }
                if node.callbacks.len != 0 {
                    self.flat.callbacks.truncate(node.callbacks.start as usize);
                }
                if node.dataset != 0 {
                    drop(self.flat.datasets.pop());
                }
                let full = self.flat.full.len();
                *node = FlatNode::new(FlatKind::Full(idx(full)));
                self.flat.full.push(whole);
                full
            }
        };
        self.flat.full.get_mut(full)
    }

    fn open_node(&mut self, node: NodeData) {
        let full = idx(self.flat.full.len());
        self.flat.full.push(node);
        self.push_node(FlatNode::new(FlatKind::Full(full)));
    }

    fn open_text(&mut self, text: AzString) {
        if text.is_localizable() {
            // A translation key: the translation pass replaces it, node by node.
            self.open_node(NodeData::create_text_do_not_use_without_block_level_wrapper(text));
        } else {
            self.open_text_str(text.as_str());
        }
    }

    fn open_text_str(&mut self, text: &str) {
        let ruled = azul_css::input_chars::normalize_input_text(text);
        let range = self.push_str(&ruled);
        self.push_node(FlatNode::new(FlatKind::Text(range)));
    }

    fn add_dom(&mut self, dom: Dom) {
        let mut dom = dom;
        dom.expand_flat();
        let Dom {
            root,
            children,
            css,
            ..
        } = dom;
        self.open_node(root);
        for sheet in css.into_library_owned_vec() {
            self.add_component_css(sheet);
        }
        for child in children.into_library_owned_vec() {
            self.add_dom(child);
        }
        self.close();
    }
}

/// Builds a UI tree in its flat form: see the module's text.
///
/// ```rust
/// # use azul_core::dom::{DomBuilder, NodeType};
/// let mut b = DomBuilder::new();
/// b.open(NodeType::Div); // the root
/// b.add_class_str("list");
/// for row in ["a", "b", "c"] {
///     b.open(NodeType::Span);
///     b.add_class_str("row");
///     b.set_key(row);
///     b.add_text_str(row);
///     b.close();
/// }
/// let dom = b.finish(); // closes what is still open
/// assert_eq!(dom.node_count(), 7);
/// ```
///
/// A node's classes, ids, attributes, inline properties, handlers, key, flags, dataset and
/// sheets are given after the node is opened and before its first child. The first node
/// opened is the root; a node opened after the root was closed is one more child of it.
///
/// The handle is one pointer: every method takes it by reference and writes the builder
/// in place (nothing is moved by value per call, through the C API either).
#[repr(C)]
#[derive(Debug)]
pub struct DomBuilder {
    // `ManuallyDrop`: the box is freed by `Drop` alone, gated on `run_destructor`, the
    // way `CssPropertyCachePtr` is (a generated wrapper may run the destructor twice).
    ptr: ManuallyDrop<Box<BuilderState>>,
    run_destructor: bool,
}

impl Drop for DomBuilder {
    fn drop(&mut self) {
        if self.run_destructor {
            self.run_destructor = false;
            // SAFETY: the box is dropped once: the flag is cleared first, and nothing
            // reads `ptr` after it is.
            unsafe {
                ManuallyDrop::drop(&mut self.ptr);
            }
        }
    }
}

impl Clone for DomBuilder {
    fn clone(&self) -> Self {
        Self {
            ptr: ManuallyDrop::new(Box::new(self.state().clone())),
            run_destructor: true,
        }
    }
}

impl Default for DomBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DomBuilder {
    /// An empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ptr: ManuallyDrop::new(Box::new(BuilderState::default())),
            run_destructor: true,
        }
    }

    /// An empty builder with room for `nodes` nodes.
    #[must_use]
    pub fn with_capacity(nodes: usize) -> Self {
        let mut builder = Self::new();
        let flat = &mut builder.state_mut().flat;
        flat.nodes.reserve(nodes);
        flat.hierarchy.reserve(nodes);
        flat.fingerprints.structure.reserve(nodes);
        flat.fingerprints.style.reserve(nodes);
        flat.fingerprints.subtree_len.reserve(nodes);
        builder
    }

    fn state(&self) -> &BuilderState {
        &self.ptr
    }

    fn state_mut(&mut self) -> &mut BuilderState {
        &mut self.ptr
    }

    /// How many nodes were pushed so far.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.state().flat.nodes.len()
    }

    /// Opens a node of `node_type` (`Dom::create_node`) as the next child of the open
    /// node: the nodes pushed until the matching [`Self::close`] are its children.
    pub fn open(&mut self, node_type: NodeType) {
        self.state_mut()
            .push_node(FlatNode::new(FlatKind::Element(node_type)));
    }

    /// Opens `node`, given whole (`Dom::create_from_data`): for what the builder's own
    /// setters do not say (accessibility, a menu, SVG data, a virtual view with its
    /// callback and version, a marker, a merge callback, ...).
    pub fn open_node(&mut self, node: NodeData) {
        self.state_mut().open_node(node);
    }

    /// Closes the open node. Closing the root is left to [`Self::finish`].
    pub fn close(&mut self) {
        self.state_mut().close();
    }

    /// A node of `node_type` without children.
    pub fn add_node(&mut self, node_type: NodeType) {
        self.open(node_type);
        self.close();
    }

    /// Opens a text node (`Dom::create_text`): its characters as the input rule has
    /// them, in the shared string. A translation key (`AzString::tr`) is kept as a node
    /// of its own.
    pub fn open_text(&mut self, text: AzString) {
        self.state_mut().open_text(text);
    }

    /// [`Self::open_text`] of a string slice: nothing is allocated for it.
    pub fn open_text_str(&mut self, text: &str) {
        self.state_mut().open_text_str(text);
    }

    /// A text node without anything of its own: [`Self::open_text`], closed.
    pub fn add_text(&mut self, text: AzString) {
        self.open_text(text);
        self.close();
    }

    /// [`Self::add_text`] of a string slice: nothing is allocated for it.
    pub fn add_text_str(&mut self, text: &str) {
        self.open_text_str(text);
        self.close();
    }

    /// `dom`, a tree built the other way (a widget's), as the next child of the open
    /// node: each of its nodes is held whole.
    pub fn add_dom(&mut self, dom: Dom) {
        self.state_mut().add_dom(dom);
    }

    /// `Dom::add_class` for the open node.
    pub fn add_class(&mut self, class: AzString) {
        self.state_mut().add_class_str(class.as_str());
    }

    /// [`Self::add_class`] of a string slice: nothing is allocated for it.
    pub fn add_class_str(&mut self, class: &str) {
        self.state_mut().add_class_str(class);
    }

    /// `Dom::with_id` for the open node.
    pub fn add_id(&mut self, id: AzString) {
        self.state_mut().add_id_str(id.as_str());
    }

    /// [`Self::add_id`] of a string slice: nothing is allocated for it.
    pub fn add_id_str(&mut self, id: &str) {
        self.state_mut().add_id_str(id);
    }

    /// `Dom::with_attribute` for the open node.
    pub fn add_attribute(&mut self, attr: AttributeType) {
        self.state_mut().add_attribute(attr);
    }

    /// `Dom::add_css_property` for the open node: one inline rule.
    pub fn add_css_property(&mut self, prop: CssPropertyWithConditions) {
        self.state_mut().add_css_property(prop);
    }

    /// `Dom::add_callback` for the open node.
    pub fn add_callback<C: Into<CoreCallback>>(
        &mut self,
        event: EventFilter,
        data: RefAny,
        callback: C,
    ) {
        self.state_mut()
            .add_callback(CoreCallbackData::create(event, data, callback));
    }

    /// `Dom::set_key` for the open node.
    pub fn set_key<K: Hash>(&mut self, key: K) {
        let mut hasher = crate::hash::DefaultHasher::new();
        key.hash(&mut hasher);
        self.state_mut().set_key_hash(hasher.finish());
    }

    /// `Dom::set_tab_index` for the open node.
    pub fn set_tab_index(&mut self, tab_index: TabIndex) {
        self.state_mut().set_tab_index(tab_index);
    }

    /// `Dom::set_contenteditable` for the open node.
    pub fn set_contenteditable(&mut self, contenteditable: bool) {
        self.state_mut().set_contenteditable(contenteditable);
    }

    /// `Dom::set_dataset` for the open node.
    pub fn set_dataset(&mut self, data: OptionRefAny) {
        self.state_mut().set_dataset(data);
    }

    /// `Dom::add_component_css` for the open node: a sheet for its subtree.
    pub fn add_component_css(&mut self, css: Css) {
        self.state_mut().add_component_css(css);
    }

    /// `Dom::set_css` for the open node: `style` parsed as a sheet for its subtree.
    #[cfg(feature = "css_parser")]
    pub fn set_css(&mut self, style: &str) {
        self.add_component_css(Css::parse_scoped(style));
    }

    /// The open node as a `NodeData`, for everything `NodeData` can say and the setters
    /// above do not: the node is held whole from here on. `None` when no node is open or
    /// the open node has children already.
    pub fn node_data_mut(&mut self) -> Option<&mut NodeData> {
        self.state_mut().node_data_mut()
    }

    fn into_state(mut self) -> BuilderState {
        self.run_destructor = false;
        // SAFETY: the flag is cleared, so `Drop` does not touch `ptr` again: the box is
        // taken out exactly once.
        let state = unsafe { ManuallyDrop::take(&mut self.ptr) };
        *state
    }

    /// Closes every node still open and answers the tree as a `Dom` that carries it flat
    /// (see the module's text). An empty builder answers `Dom::default()`.
    #[must_use]
    pub fn finish(self) -> Dom {
        self.into_state().finish().into_dom()
    }
}

impl Dom {
    /// Whether this `Dom` carries the tree below its root in the flat form
    /// ([`DomBuilder::finish`]).
    #[must_use]
    pub fn is_flat(&self) -> bool {
        self.root
            .extra_fields()
            .is_some_and(|extra| extra.flat.is_some())
    }

    /// The flat form this `Dom` carries below its root, if it carries one.
    #[must_use]
    pub fn flat(&self) -> Option<&FlatDom> {
        self.root.extra_fields()?.flat.as_deref()
    }

    /// Turns a `Dom` that carries its tree flat into the tree of `Dom`s the same calls
    /// build with `Dom::with_child`; does nothing to any other `Dom`.
    ///
    /// Everything that walks a `Dom` as a tree needs this first. The builder methods
    /// that take children (`add_child`, `set_children`, `collect`) and
    /// `fixup_children_estimated`, which every way to a `StyledDom` begins with, call it.
    pub fn expand_flat(&mut self) {
        let Some(flat) = self
            .root
            .existing_extra_fields_mut()
            .and_then(|extra| extra.flat.take())
        else {
            return;
        };
        let mut children = flat.into_children();
        // Children written onto the carrying `Dom` itself come after the carried ones.
        let own = mem::take(&mut self.children).into_library_owned_vec();
        children.extend(own);
        self.estimated_total_children = children
            .iter()
            .map(|child| child.estimated_total_children + 1)
            .sum();
        self.children = children.into();
    }

    /// The node at pre-order index `index`, copied: the root for 0, a node of the flat
    /// form below it. `None` for a `Dom` that is a tree (walk it) and past the last node.
    #[must_use]
    pub fn flat_node_data_at(&self, index: usize) -> Option<NodeData> {
        let flat = self.flat()?;
        if index == 0 {
            Some(node_without_flat(&self.root))
        } else {
            flat.node_data(index)
        }
    }

    /// Whether a `Dom` that carries its tree flat holds a node whose type `wanted` says
    /// yes to, its root included. `false` for a `Dom` that is a tree (walk it).
    #[must_use]
    pub fn flat_holds_node_type(&self, wanted: impl Fn(&NodeType) -> bool) -> bool {
        let Some(flat) = self.flat() else {
            return false;
        };
        wanted(self.root.get_node_type())
            || flat.nodes.iter().skip(1).any(|node| match &node.kind {
                FlatKind::Element(node_type) => wanted(node_type),
                // A shared text is a text of no characters to this question.
                FlatKind::Text(_) => wanted(&NodeType::Text(BoxOrStatic::Static(&raw const EMPTY_TEXT))),
                FlatKind::Full(index) => flat
                    .full
                    .get(*index as usize)
                    .is_some_and(|full| wanted(full.get_node_type())),
            })
    }

    /// The two parallel arenas of a `Dom` that carries its tree flat, for
    /// `StyledDom::create_from_fast_dom`: no tree is built in between. `Err` hands back a
    /// `Dom` that is a tree (or was given children of its own beside the carried ones).
    ///
    /// # Errors
    ///
    /// The `Dom` itself, unchanged, when it does not carry a flat form alone.
    pub fn into_fast_dom(mut self) -> Result<FastDom, Self> {
        if !self.children.as_ref().is_empty() {
            return Err(self);
        }
        let Some(flat) = self
            .root
            .existing_extra_fields_mut()
            .and_then(|extra| extra.flat.take())
        else {
            return Err(self);
        };
        let Self { root, css, .. } = self;
        Ok(flat.into_fast_dom(root, css.into_library_owned_vec()))
    }
}

/// [`crate::diff::fingerprint_dom`] for a `Dom` of either form: the fingerprints and the
/// carried values of the tree `dom` stands for.
///
/// A `Dom` that carries its tree flat answers from what its builder made: its root is
/// hashed (the `Dom` holds it and may have changed it), no other node is. A tree is
/// walked as before.
#[must_use]
pub fn fingerprint_built_dom(dom: &Dom) -> (DomFingerprints, PreCascadeTransfers) {
    match dom.flat() {
        None => crate::diff::fingerprint_dom(dom),
        Some(flat) if dom.children.as_ref().is_empty() => {
            flat.fingerprints_with_root(&dom.root, dom.css.as_ref())
        }
        Some(_) => {
            // Children were written onto a carrying `Dom`: the tree it stands for is the
            // expanded one.
            let mut tree = dom.clone();
            tree.expand_flat();
            crate::diff::fingerprint_dom(&tree)
        }
    }
}

/// [`fingerprint_built_dom`] for a rebuild that is compared with the build before, whose
/// fingerprints are `prev`: the fingerprints and the carried values of `dom`, and `dom` as
/// the rest of the rebuild needs it.
///
/// A flat `Dom` STAYS flat when it reads as the build before did (the same tree, every
/// node's two tiers, the same sheets): such a rebuild keeps what it has, takes the
/// carried values and drops the fresh DOM without reading a node of it. When anything
/// differs the `Dom` becomes its tree here, because what follows walks one
/// (`crate::diff::nodes_at` and the functions on it, the passes before styling).
pub fn fingerprint_built_dom_against(
    dom: &mut Dom,
    prev: Option<&DomFingerprints>,
) -> (DomFingerprints, PreCascadeTransfers) {
    let out = fingerprint_built_dom(dom);
    if dom.is_flat() && prev != Some(&out.0) {
        dom.expand_flat();
    }
    out
}

#[cfg(test)]
#[path = "dom_builder_test.rs"]
mod dom_builder_test;
