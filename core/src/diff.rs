//! DOM Reconciliation Module
//!
//! This module provides the reconciliation algorithm that compares two DOM trees
//! and generates lifecycle events. It uses stable keys and content hashing to
//! identify moves vs. mounts/unmounts.
//!
//! The reconciliation strategy is:
//! 1. **Stable Key Match:** If `.with_key()` is used, it's an absolute match (O(1)).
//! 2. **CSS ID Match:** If no key, the CSS id, where the app's [`NodeIdentityConfig`] makes it a key.
//! 3. **Structural Key Match:** type + place among the siblings + parent's key (recursive).
//! 4. **Hash Match (Content Match):** Check for identical `DomNodeHash`.
//! 5. **Structural Hash Match:** For text nodes, match by structural hash (ignoring content).
//! 6. **Fallback:** Anything not matched is a `Mount` (new) or `Unmount` (old leftovers).

use alloc::{
    collections::{BTreeMap, VecDeque},
    string::{String, ToString},
    vec::Vec,
};
use core::hash::Hash;

use azul_css::props::property::{CssPropertyType, RelayoutScope};

use crate::{
    dom::{AttributeType, DomId, DomNodeHash, DomNodeId, IdOrClass, NodeData, NodeType},
    events::{
        ComponentEventFilter, EventData, EventFilter, EventPhase, EventSource, EventType,
        LifecycleEventData, LifecycleReason, SyntheticEvent,
    },
    geom::LogicalRect,
    id::NodeId,
    refany::RefAny,
    styled_dom::{
        ChangedCssProperty, NodeHierarchyItem, NodeHierarchyItemId, RestyleResult, StyledNodeState,
    },
    task::Instant,
    OrderedMap,
};

// ============================================================================
// NodeChangeSet — granular per-node change flags
// ============================================================================

/// Bit flags describing what changed about a node between old and new DOM.
/// Multiple flags can be set simultaneously. Uses manual bit manipulation
/// instead of bitflags crate to avoid adding a dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NodeChangeSet {
    pub bits: u32,
}

impl NodeChangeSet {
    // --- Changes that affect LAYOUT (need relayout + repaint) ---

    /// Node type changed entirely (e.g., Text → Image).
    pub const NODE_TYPE_CHANGED: u32 = 0b0000_0000_0000_0001;
    /// Text content changed (for Text nodes).
    pub const TEXT_CONTENT: u32 = 0b0000_0000_0000_0010;
    /// CSS IDs or classes changed (may cause restyle → relayout).
    pub const IDS_AND_CLASSES: u32 = 0b0000_0000_0000_0100;
    /// Inline CSS properties changed that affect layout.
    pub const INLINE_STYLE_LAYOUT: u32 = 0b0000_0000_0000_1000;
    /// Children added, removed, or reordered.
    pub const CHILDREN_CHANGED: u32 = 0b0000_0000_0001_0000;
    /// Image source changed (may affect intrinsic size).
    pub const IMAGE_CHANGED: u32 = 0b0000_0000_0010_0000;
    /// Contenteditable flag changed.
    pub const CONTENTEDITABLE: u32 = 0b0000_0000_0100_0000;
    /// Tab index changed.
    pub const TAB_INDEX: u32 = 0b0000_0000_1000_0000;

    // --- Changes that affect PAINT only (no relayout needed) ---

    /// Inline CSS properties changed that affect paint only.
    pub const INLINE_STYLE_PAINT: u32 = 0b0000_0001_0000_0000;
    /// The only inline CSS that changed are the values of the node's insets (`top`, `right`,
    /// `bottom`, `left`: [`NodeDataFingerprint::inline_inset_hash`]); which insets it declares
    /// did not change. The box may have to be placed again, and - where its size comes from
    /// its insets (`left` and `right` with an `auto` width) - laid out again: a layout change
    /// ([`Self::AFFECTS_LAYOUT`]), told apart from [`Self::INLINE_STYLE_LAYOUT`] so a layout
    /// that can place a box again without laying out its interior may do so.
    pub const INLINE_STYLE_INSETS: u32 = 0b0100_0000_0000_0000_0000;
    /// Styled node state changed (hover, active, focus, etc.).
    pub const STYLED_STATE: u32 = 0b0000_0010_0000_0000;

    // --- Changes that affect NEITHER layout nor paint ---

    /// Callbacks changed (new `RefAny`, different event handlers).
    pub const CALLBACKS: u32 = 0b0000_0100_0000_0000;
    /// Dataset changed.
    pub const DATASET: u32 = 0b0000_1000_0000_0000;
    /// Accessibility info changed.
    pub const ACCESSIBILITY: u32 = 0b0001_0000_0000_0000;
    /// Whether the node itself makes it a hit-test target changed: a handler
    /// of an event that is not the window's, a tab stop or a context menu
    /// came or went ([`crate::layout_inputs::HitTestInputs`]). Nothing the
    /// node is laid out or looks like moved, but the display list paints it
    /// anew, with its hit-test area or without one.
    pub const HIT_TEST_TARGET: u32 = 0b0010_0000_0000_0000_0000;

    // --- Changes that affect the node's whole SUBTREE ---

    /// A custom-property (`--name`) definition in the node's own style
    /// changed. Every descendant may read it through `var()` - including
    /// layout properties - so the change is not the node's alone: without
    /// this flag a definition-only change reported nothing, and a display
    /// list patch spliced the readers' stale items (design §9.1 pitfall 11,
    /// "a rice edit repaints half the window").
    pub const CUSTOM_PROPERTIES: u32 = 0b0010_0000_0000_0000;
    /// The node's `lang` attribute changed: the content language of the
    /// node and of every descendant that states none of its own. Lines
    /// hyphenate in it (`hyphens: auto`), so the node's own lines and those
    /// of the inline formatting contexts below it may break anew.
    pub const CONTENT_LANGUAGE: u32 = 0b0100_0000_0000_0000;
    /// Something the boxes below the node read from it changed
    /// ([`crate::layout_inputs::LayoutReach::Descendants`]): the legacy
    /// `align`, an SVG group's `transform` or stroke, an `<svg>`'s `viewBox`.
    /// Every box below the node is laid out again.
    pub const DESCENDANT_INPUTS: u32 = 0b0001_0000_0000_0000_0000;

    // --- Changes that affect the node's own box ---

    /// Something the node's own box is laid out or painted from changed,
    /// besides its type, style and flags
    /// ([`crate::layout_inputs::LayoutReach::OwnBox`]): a cell's span, an
    /// SVG text's position, a shape's geometry.
    pub const OWN_BOX_INPUTS: u32 = 0b1000_0000_0000_0000;

    // --- Composite masks ---

    /// Any change that requires a layout pass.
    pub const AFFECTS_LAYOUT: u32 = Self::NODE_TYPE_CHANGED
        | Self::TEXT_CONTENT
        | Self::IDS_AND_CLASSES
        | Self::INLINE_STYLE_LAYOUT
        | Self::INLINE_STYLE_INSETS
        | Self::CHILDREN_CHANGED
        | Self::IMAGE_CHANGED
        | Self::CONTENTEDITABLE
        | Self::CUSTOM_PROPERTIES
        | Self::CONTENT_LANGUAGE
        | Self::OWN_BOX_INPUTS
        | Self::DESCENDANT_INPUTS;

    /// Any change that requires a paint/display-list update (but not layout).
    /// (`CUSTOM_PROPERTIES` is a layout change: a relayout repaints anyway,
    /// and the two masks stay disjoint.)
    pub const AFFECTS_PAINT: u32 = Self::INLINE_STYLE_PAINT | Self::STYLED_STATE;

    #[must_use]
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bits == 0
    }

    #[must_use]
    pub const fn contains(&self, flag: u32) -> bool {
        (self.bits & flag) == flag
    }

    #[must_use]
    pub const fn intersects(&self, mask: u32) -> bool {
        (self.bits & mask) != 0
    }

    pub const fn insert(&mut self, flag: u32) {
        self.bits |= flag;
    }

    /// Returns true if no visual change occurred (only callbacks/dataset/a11y).
    #[must_use]
    pub const fn is_visually_unchanged(&self) -> bool {
        !self.intersects(Self::AFFECTS_LAYOUT) && !self.intersects(Self::AFFECTS_PAINT)
    }

    /// Returns true if layout is needed.
    #[must_use]
    pub const fn needs_layout(&self) -> bool {
        self.intersects(Self::AFFECTS_LAYOUT)
    }

    /// Returns true if paint is needed (but not necessarily layout).
    #[must_use]
    pub const fn needs_paint(&self) -> bool {
        self.intersects(Self::AFFECTS_PAINT)
    }
}

impl core::ops::BitOrAssign for NodeChangeSet {
    fn bitor_assign(&mut self, rhs: Self) {
        self.bits |= rhs.bits;
    }
}

impl core::ops::BitOr for NodeChangeSet {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self {
            bits: self.bits | rhs.bits,
        }
    }
}

/// Extended diff result that includes per-node change information.
#[derive(Debug, Clone, Default)]
pub struct ExtendedDiffResult {
    /// Original diff result (lifecycle events + node moves).
    pub diff: DiffResult,
    /// Per-node change report for matched (moved) nodes.
    /// Each entry: (`old_node_id`, `new_node_id`, `what_changed`).
    /// Only contains entries for nodes that were matched.
    pub node_changes: Vec<(NodeId, NodeId, NodeChangeSet)>,
}

/// The declarations of a node's own style that are not static properties:
/// its `var()` / `env()` references and its custom-property definitions,
/// each with its rule's conditions, in declaration order.
#[allow(clippy::type_complexity)] // two parallel (declaration, conditions) lists
fn non_static_declarations(
    node: &NodeData,
) -> (
    Vec<(
        &azul_css::css::DynamicCssProperty,
        &azul_css::dynamic_selector::DynamicSelectorVec,
    )>,
    Vec<(
        &azul_css::css::CssCustomProperty,
        &azul_css::dynamic_selector::DynamicSelectorVec,
    )>,
) {
    use azul_css::css::CssDeclaration;
    let mut refs = Vec::new();
    let mut defs = Vec::new();
    for rule in node.style.rules.as_ref() {
        for d in rule.declarations.as_ref() {
            match d {
                CssDeclaration::Static(_) => {}
                CssDeclaration::Dynamic(r) => refs.push((r, &rule.conditions)),
                CssDeclaration::CustomProperty(c) => defs.push((c, &rule.conditions)),
            }
        }
    }
    (refs, defs)
}

/// Hash a node's own style for the change fingerprints: every declaration
/// with its rule's condition count. A static property hashes exactly as the
/// old static-only walk did; `var()` references and custom-property
/// definitions count too, or a definition-only change fingerprinted as "no
/// change" and its readers kept their stale values.
pub(crate) fn hash_inline_style<H: core::hash::Hasher>(style: &azul_css::css::Css, h: &mut H) {
    use azul_css::css::CssDeclaration;
    for rule in style.rules.as_ref() {
        // The conditions are part of the style: the same declaration for
        // `:hover` and for `:focus` is another style.
        rule.conditions.as_slice().hash(h);
        for d in rule.declarations.as_ref() {
            match d {
                CssDeclaration::Static(p) => p.hash(h),
                other => other.hash(h),
            }
        }
    }
}

/// How far a change of ONE static declaration of a node's own style reaches, for the node's
/// change fingerprint ([`NodeDataFingerprint`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineReach {
    /// The node's layout may depend on it: everything that is neither of the two below.
    Layout,
    /// An inset (`top`, `right`, `bottom`, `left`): where a positioned box is placed.
    Inset,
    /// A `transform` of at least one function, or an `opacity`, as an exact value: what the
    /// display list binds by key and both renderers read live. No box depends on the VALUE;
    /// that the node HAS a transform does (a transformed box is the containing block of its
    /// positioned descendants), and that stays with [`InlineReach::Layout`].
    GpuValue,
}

fn inline_reach(property: &azul_css::props::property::CssProperty) -> InlineReach {
    use azul_css::props::property::{CssProperty, CssPropertyType};
    match property {
        CssProperty::Transform(value)
            if value
                .get_property()
                .is_some_and(|functions| !functions.is_empty()) =>
        {
            InlineReach::GpuValue
        }
        CssProperty::Opacity(value) if value.get_property().is_some() => InlineReach::GpuValue,
        other => match other.get_type() {
            CssPropertyType::Top
            | CssPropertyType::Right
            | CssPropertyType::Bottom
            | CssPropertyType::Left => InlineReach::Inset,
            _ => InlineReach::Layout,
        },
    }
}

/// A node's own style as three hashes, by how far a change reaches ([`InlineReach`]):
/// `(everything else, the values of its insets, the values of its transform and opacity)`.
///
/// A declaration of the second or third kind still counts in the FIRST hash with its
/// property type and its rule's conditions (in declaration order): that the node declares
/// it, and where among its declarations, is part of the style a layout was made with. Only
/// its value is kept out. So two styles that differ in the first hash differ as before
/// (a layout change), and two that agree in it declare the same insets and the same
/// transform / opacity, with other values at most.
fn hash_inline_style_by_reach(style: &azul_css::css::Css) -> (u64, u64, u64) {
    use azul_css::css::CssDeclaration;
    use core::hash::Hasher;

    let mut rest = crate::hash::DefaultHasher::new();
    let mut insets = crate::hash::DefaultHasher::new();
    let mut gpu_values = crate::hash::DefaultHasher::new();
    for rule in style.rules.as_ref() {
        // The conditions are part of the style: the same declaration for
        // `:hover` and for `:focus` is another style.
        rule.conditions.as_slice().hash(&mut rest);
        for d in rule.declarations.as_ref() {
            match d {
                CssDeclaration::Static(p) => match inline_reach(p) {
                    InlineReach::Layout => p.hash(&mut rest),
                    InlineReach::Inset => {
                        p.get_type().hash(&mut rest);
                        rule.conditions.as_slice().hash(&mut insets);
                        p.hash(&mut insets);
                    }
                    InlineReach::GpuValue => {
                        p.get_type().hash(&mut rest);
                        rule.conditions.as_slice().hash(&mut gpu_values);
                        p.hash(&mut gpu_values);
                    }
                },
                other => other.hash(&mut rest),
            }
        }
    }
    (rest.finish(), insets.finish(), gpu_values.finish())
}

/// Whether the own (inline) styles of two matched nodes differ ONLY in what
/// [`NodeDataFingerprint::diff`] does not call a layout of the node's interior.
///
/// The values of its insets ([`NodeChangeSet::INLINE_STYLE_INSETS`]) and / or of an exact
/// `transform` / `opacity` ([`NodeChangeSet::INLINE_STYLE_PAINT`]). `false` for equal styles and
/// for any other difference (a declaration that comes or goes, another property, a condition).
///
/// The three hashes the fingerprint holds, compared the way it compares them: a caller that
/// leaves such a node to the fingerprint - the CSS diff of a rebuild, which would otherwise
/// name it as restyled and have its box built fresh - agrees with it by construction.
#[must_use]
pub fn inline_style_moved_in_place(old: &NodeData, new: &NodeData) -> bool {
    let (old_rest, old_insets, old_gpu_values) = hash_inline_style_by_reach(&old.style);
    let (new_rest, new_insets, new_gpu_values) = hash_inline_style_by_reach(&new.style);
    old_rest == new_rest && (old_insets != new_insets || old_gpu_values != new_gpu_values)
}

/// Compare two matched `NodeData` instances field-by-field and return
/// a `NodeChangeSet` describing what changed.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
#[must_use]
pub fn compute_node_changes(
    old_node: &NodeData,
    new_node: &NodeData,
    old_styled_state: Option<&StyledNodeState>,
    new_styled_state: Option<&StyledNodeState>,
) -> NodeChangeSet {
    let mut changes = NodeChangeSet::empty();

    // 1. Node type discriminant
    if core::mem::discriminant(old_node.get_node_type())
        != core::mem::discriminant(new_node.get_node_type())
    {
        changes.insert(NodeChangeSet::NODE_TYPE_CHANGED);
        return changes; // everything else is irrelevant
    }

    // 2. Content-specific comparison (same discriminant)
    match (old_node.get_node_type(), new_node.get_node_type()) {
        (NodeType::Text(old_text), NodeType::Text(new_text)) => {
            if old_text.as_str() != new_text.as_str() {
                changes.insert(NodeChangeSet::TEXT_CONTENT);
            }
        }
        (NodeType::Image(old_img), NodeType::Image(new_img)) => {
            // Use Hash-based comparison (pointer identity for decoded images,
            // callback identity for callback images)
            use core::hash::Hasher;
            let hash_img = |img: &crate::resources::ImageRef| -> u64 {
                let mut h = crate::hash::DefaultHasher::new();
                img.hash(&mut h);
                h.finish()
            };
            if hash_img(old_img) != hash_img(new_img) {
                changes.insert(NodeChangeSet::IMAGE_CHANGED);
            }
        }
        _ => {} // Same non-content type → no content change
    }

    // 3. What the layout reads of the attributes and the SVG data, by what
    // a change of it moves: the ids and classes, the content language, the
    // node's own box, the boxes below it. The fingerprint hashes the same
    // inputs (`LayoutInputHashes`), so the two tiers report alike.
    {
        use crate::layout_inputs::{reads_alike, LayoutReach};
        for (reach, flag) in [
            (LayoutReach::Selectors, NodeChangeSet::IDS_AND_CLASSES),
            (LayoutReach::Lines, NodeChangeSet::CONTENT_LANGUAGE),
            (LayoutReach::OwnBox, NodeChangeSet::OWN_BOX_INPUTS),
            (LayoutReach::Descendants, NodeChangeSet::DESCENDANT_INPUTS),
        ] {
            if !reads_alike(old_node, new_node, reach) {
                changes.insert(flag);
            }
        }
    }

    // 4. Inline CSS properties — classify into layout-affecting vs paint-only.
    // After the inline-vs-component unification, inline CSS is stored as a `Css`
    // with rule blocks; iterate it via the `(property, conditions)` flat view to
    // keep the per-property compare semantics this code was written for.
    if old_node.style != new_node.style {
        let mut has_layout = false;
        let mut has_paint = false;

        // Classify a changed/added/removed property into the layout vs paint bucket.
        #[allow(clippy::items_after_statements)]
        fn mark(prop_type: CssPropertyType, has_layout: &mut bool, has_paint: &mut bool) {
            if prop_type.relayout_scope(true) == RelayoutScope::None {
                *has_paint = true;
            } else {
                *has_layout = true;
            }
        }

        // AUDIT: key the diff by (prop_type, conditions), NOT prop_type alone.
        // A node can carry the same property under different conditions (e.g.
        // `color: red` and `color: blue` scoped to `:hover`); keying by
        // prop_type collapsed them into one map slot, so a change to one
        // conditional variant could be silently dropped. Match each new
        // property against an old entry with the SAME prop_type AND the same
        // conditions, and mark any old entry left unmatched as removed.
        let old_props: Vec<(CssPropertyType, _, _)> = old_node
            .style
            .iter_inline_properties()
            .map(|(prop, conds)| (prop.get_type(), prop, conds))
            .collect();
        let mut old_matched = vec![false; old_props.len()];

        for (prop, conds) in new_node.style.iter_inline_properties() {
            let prop_type = prop.get_type();
            // Find an as-yet-unmatched old entry with the same (type, conditions).
            let mut found_unchanged = false;
            for (i, (old_type, old_prop, old_conds)) in old_props.iter().enumerate() {
                if old_matched[i]
                    || *old_type != prop_type
                    || old_conds.as_slice() != conds.as_slice()
                {
                    continue;
                }
                old_matched[i] = true;
                if *old_prop == prop {
                    found_unchanged = true;
                }
                break;
            }
            // Unchanged only when we matched an old (type, conditions) slot whose
            // value is identical; otherwise the property was added or changed.
            if !found_unchanged {
                mark(prop_type, &mut has_layout, &mut has_paint);
            }
        }

        // Check for removed properties (old (type, conditions) slots never matched)
        for (i, (old_type, _, _)) in old_props.iter().enumerate() {
            if !old_matched[i] {
                mark(*old_type, &mut has_layout, &mut has_paint);
            }
        }

        // The declarations the static view above skips. A `var()` / `env()`
        // reference is a property of THIS node: added, removed or changed,
        // it counts as its property type. A custom-property definition feeds
        // every descendant that reads it: its own flag, whole-subtree scope.
        let (old_references, old_definitions) = non_static_declarations(old_node);
        let (new_references, new_definitions) = non_static_declarations(new_node);
        for (d, _) in new_references.iter().filter(|r| !old_references.contains(r)) {
            mark(d.default_value.get_type(), &mut has_layout, &mut has_paint);
        }
        for (d, _) in old_references.iter().filter(|r| !new_references.contains(r)) {
            mark(d.default_value.get_type(), &mut has_layout, &mut has_paint);
        }
        if old_definitions != new_definitions {
            changes.insert(NodeChangeSet::CUSTOM_PROPERTIES);
        }

        if has_layout {
            changes.insert(NodeChangeSet::INLINE_STYLE_LAYOUT);
        }
        if has_paint {
            changes.insert(NodeChangeSet::INLINE_STYLE_PAINT);
        }
    }

    // 5. Callbacks
    {
        let old_cbs = old_node.callbacks.as_ref();
        let new_cbs = new_node.callbacks.as_ref();
        if old_cbs.len() == new_cbs.len() {
            for (o, n) in old_cbs.iter().zip(new_cbs.iter()) {
                if o.event != n.event || o.callback != n.callback {
                    changes.insert(NodeChangeSet::CALLBACKS);
                    break;
                }
            }
        } else {
            changes.insert(NodeChangeSet::CALLBACKS);
        }
    }

    // 6. Dataset
    if old_node.get_dataset() != new_node.get_dataset() {
        changes.insert(NodeChangeSet::DATASET);
    }

    // 7. Contenteditable
    if old_node.is_contenteditable() != new_node.is_contenteditable() {
        changes.insert(NodeChangeSet::CONTENTEDITABLE);
    }

    // 8. Tab index
    if old_node.get_tab_index() != new_node.get_tab_index() {
        changes.insert(NodeChangeSet::TAB_INDEX);
    }

    // 8b. Whether the node itself is a hit-test target, read as the
    // fingerprint and the tag generator read it.
    {
        use crate::layout_inputs::HitTestInputs;
        let target = |node: &NodeData| HitTestInputs::of(node).make_a_target();
        if target(old_node) != target(new_node) {
            changes.insert(NodeChangeSet::HIT_TEST_TARGET);
        }
    }

    // 9. Styled node state (hover, active, focused, etc.)
    if old_styled_state != new_styled_state {
        changes.insert(NodeChangeSet::STYLED_STATE);
    }

    changes
}

/// What makes a node of a rebuilt DOM another node than the one at its place
/// before.
///
/// A node that is the same node keeps its dataset, focus, scroll, hover and
/// text-edit state; a node that is not unmounts and the new one mounts
/// (`BeforeUnmount` / `AfterMount`).
///
/// An explicit key (`with_key`) always decides, and a node of another type
/// is always another node: a text's caret or an image's frame means
/// nothing on a node of another kind. A keyless node is otherwise known by
/// its place among its siblings and its parent's identity, and this says
/// what else counts.
///
/// The default is React's rules: the place is the index among all
/// siblings, and neither the classes nor the CSS id change a node's
/// identity. Set it once in `AppConfig::node_identity`; every window and
/// every view of the app reconciles by it.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub struct NodeIdentityConfig {
    /// Which siblings count for a keyless node's place. Default: `All`.
    pub siblings_counted: SiblingsCounted,
    /// Whether a node whose class list changed is a new node, so toggling a
    /// class such as `active` resets its state. Default: `false`, as in
    /// React, where a changed `className` keeps the element.
    pub class_change_remounts: bool,
    /// Whether the CSS id acts as a key: a node with an id is the node with
    /// the same id wherever it stands, and a changed id makes a new node.
    /// Default: `false`, as in React, where `id` is an attribute like any
    /// other.
    pub id_is_key: bool,
}

/// Which siblings count for a keyless node's place, see
/// [`NodeIdentityConfig::siblings_counted`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum SiblingsCounted {
    /// Every sibling, as React's child index: a node inserted in front
    /// moves every later sibling one place, so each of them is the node
    /// that stood at its new place before, if that one had the same type.
    #[default]
    All,
    /// Only the siblings of the node's own type, as CSS `:nth-of-type`: a
    /// node of another type inserted in front moves nothing.
    SameType,
}

impl NodeIdentityConfig {
    /// The key a node carries wherever it stands: its explicit key, else its
    /// CSS id when the id acts as a key. `None` for a node known by its place.
    pub(crate) fn terminal_key(self, node: &NodeData) -> Option<u64> {
        use core::hash::Hasher;
        if let Some(key) = node.get_key() {
            return Some(key);
        }
        if !self.id_is_key {
            return None;
        }
        node.attributes().as_ref().iter().find_map(|attr| {
            attr.as_id().map(|id| {
                let mut hasher = crate::hash::DefaultHasher::new();
                id.hash(&mut hasher);
                hasher.finish()
            })
        })
    }

    /// Whether `old` and `new` differ in what this config makes part of a
    /// node's identity beyond its type, place and key: its class list.
    fn classes_differ(self, old: &NodeData, new: &NodeData) -> bool {
        fn classes(node: &NodeData) -> Vec<&str> {
            node.attributes()
                .as_ref()
                .iter()
                .filter_map(AttributeType::as_class)
                .collect::<Vec<_>>()
        }
        self.class_change_remounts && classes(old) != classes(new)
    }
}

/// How many steps the passes of the reconciliation took: what the size sweeps of the tests read
/// to say the passes are linear in the nodes. The metered entry points ([`reconcile_dom_metered`],
/// [`precompute_reconciliation_keys_metered`], ...) add to the one they are handed; the public ones
/// hand them a throwaway.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReconcileWork {
    /// The reconciliation keys: a sibling visited, a node placed.
    pub(crate) keys: usize,
    /// The shallow tiers of `reconcile_dom` (B2, B3): an old node indexed, a queue entry popped.
    pub(crate) shallow: usize,
    /// The subtree pass of `reconcile_dom` (A2): an old node indexed, a queue entry popped.
    pub(crate) subtree: usize,
}

/// What the shallow tiers of [`reconcile_dom`] (B2, B3) ask of an old node's PARENT before a new
/// node may take it: the gate that keeps a keyless match from carrying focus, scroll or dataset
/// state across parents.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ParentGate {
    /// The new node's parent was matched to this old node: the old node is its child.
    Node(NodeId),
    /// The new node's parent is unmatched (or it has none): the old node's parent has this
    /// reconciliation key (`None`: it has no parent).
    Key(Option<u64>),
}

/// The old nodes the shallow tiers of [`reconcile_dom`] (B2: content hash, B3: structural hash)
/// may still claim, one queue per (hash of the node, gate of its parent), each in document order.
///
/// The tiers used to search ONE queue per hash, over every old node of the DOM, for the first
/// entry that was unconsumed and whose parent agreed - and an entry another pass had consumed
/// never left its queue. A structural hash ignores a text node's text and every node's inline
/// style, so all the text nodes of a DOM shared one queue and all the spans of one class
/// another: each new node no earlier pass had matched walked thousands of consumed entries
/// (a map view growing from 6,000 to 12,500 nodes: about 20 million steps a settle). Here an
/// old node is entered only if it is still unconsumed, under the gates it can pass, and a claim
/// pops the front of one queue: every entry is looked at once.
#[derive(Default)]
struct ShallowTiers {
    content: OrderedMap<(DomNodeHash, ParentGate), VecDeque<NodeId>>,
    structural: OrderedMap<(DomNodeHash, ParentGate), VecDeque<NodeId>>,
}

impl ShallowTiers {
    /// The queues of the old nodes `consumed` does not hold yet.
    fn of(
        old_node_data: &[NodeData],
        old_hierarchy: &[NodeHierarchyItem],
        old_rec_keys: &[u64],
        consumed: &[bool],
        work: &mut ReconcileWork,
    ) -> Self {
        let mut tiers = Self::default();
        for (idx, node) in old_node_data.iter().enumerate() {
            if consumed.get(idx).copied().unwrap_or(false) {
                continue;
            }
            work.shallow += 1;
            let id = NodeId::new(idx);
            let parent = old_hierarchy
                .get(idx)
                .and_then(NodeHierarchyItem::parent_id);
            let parent_key = parent.and_then(|p| old_rec_keys.get(p.index()).copied());
            let content = node.calculate_node_data_hash();
            let structural = node.calculate_structural_hash();
            // Under its parent's reconciliation key, and - when it has a parent - under the
            // parent itself: the two things a new node's gate can name.
            let by_key = ParentGate::Key(parent_key);
            tiers.content.entry((content, by_key)).or_default().push_back(id);
            tiers
                .structural
                .entry((structural, by_key))
                .or_default()
                .push_back(id);
            if let Some(parent) = parent {
                let by_node = ParentGate::Node(parent);
                tiers.content.entry((content, by_node)).or_default().push_back(id);
                tiers
                    .structural
                    .entry((structural, by_node))
                    .or_default()
                    .push_back(id);
            }
        }
        tiers
    }

    /// The first old node of the queue of `(hash, gate)`, in document order, that no pass has
    /// consumed. The entries before it are consumed for good and leave the queue with it.
    fn claim(
        queues: &mut OrderedMap<(DomNodeHash, ParentGate), VecDeque<NodeId>>,
        hash: DomNodeHash,
        gate: ParentGate,
        consumed: &[bool],
        work: &mut ReconcileWork,
    ) -> Option<NodeId> {
        let queue = queues.get_mut(&(hash, gate))?;
        while let Some(old_id) = queue.pop_front() {
            work.shallow += 1;
            if !consumed.get(old_id.index()).copied().unwrap_or(true) {
                return Some(old_id);
            }
        }
        None
    }
}

/// Calculate the reconciliation key for a node using the priority hierarchy:
/// 1. Explicit key (set via `.with_key()`)
/// 2. CSS ID (set via `.with_id("my-id")`), when `identity.id_is_key`
/// 3. Structural key: the node's type and its place among its siblings
///    (counted as `identity.siblings_counted` says), its classes when
///    `identity.class_change_remounts`, and its parent's reconciliation key
///
/// The structural key allows keyless nodes to be matched across frames when
/// their place and type are stable (even if content changed — which then
/// fires an `Update` lifecycle event, see `reconcile_dom`).
///
/// When `hierarchy` is empty (or this node has no entry), the structural
/// key degrades to the node's type (and classes) — the parent and the place
/// simply drop out. This lets callers that don't track hierarchy (tests,
/// flat-DOM scenarios) still benefit from explicit-key and CSS-ID matching
/// without divergent behavior.
#[must_use]
pub fn calculate_reconciliation_key(
    node_data: &[NodeData],
    hierarchy: &[NodeHierarchyItem],
    node_id: NodeId,
    identity: NodeIdentityConfig,
) -> u64 {
    calculate_reconciliation_key_metered(node_data, hierarchy, node_id, identity, &mut 0)
}

/// [`calculate_reconciliation_key`], adding the steps it took to `key_steps`
/// ([`ReconcileWork::keys`]).
#[must_use]
pub(crate) fn calculate_reconciliation_key_metered(
    node_data: &[NodeData],
    hierarchy: &[NodeHierarchyItem],
    node_id: NodeId,
    identity: NodeIdentityConfig,
    key_steps: &mut usize,
) -> u64 {
    use core::hash::Hasher;

    let n = node_data.len();

    // Terminal (parent-independent) key for a node: Priority 1 explicit key,
    // else Priority 2 CSS ID, else `None` (structural — needs the parent chain).
    let terminal_key = |nid: NodeId| identity.terminal_key(&node_data[nid.index()]);

    // Fast path: the node itself has an explicit key or CSS ID.
    if let Some(key) = terminal_key(node_id) {
        return key;
    }

    // Priority 3: structural key, computed ITERATIVELY up the parent chain.
    //
    // AUDIT: the previous implementation recursed once per ancestor with no
    // depth cap and no cycle guard, so a deep DOM overflowed the stack and a
    // corrupt (cyclic) hierarchy recursed forever — and `precompute_*` calls
    // this once per node. Walk upward instead, bounded by the node count.
    //
    // Collect the structural chain from `node_id` upward. The walk stops at:
    //   - the root (a node with no parent) — structural base is just `discriminant + classes`,
    //   - a terminal (explicit-key / CSS-ID) ancestor, whose key seeds the fold, or
    //   - `n` iterations (a valid parent chain is at most `n` long, so exceeding that means the
    //     hierarchy is cyclic/corrupt — stop).
    let mut chain: Vec<NodeId> = Vec::new();
    let mut seed_parent_key: Option<u64> = None;
    let mut cur = node_id;
    for _ in 0..n {
        if cur.index() >= n {
            break;
        }
        chain.push(cur);
        match hierarchy
            .get(cur.index())
            .and_then(NodeHierarchyItem::parent_id)
        {
            None => break,
            Some(parent) => {
                if let Some(k) = terminal_key(parent) {
                    seed_parent_key = Some(k);
                    break;
                }
                cur = parent;
            }
        }
    }

    // Fold from the topmost ancestor down to `node_id`. `parent_key` threads the
    // accumulated key of the level above (identical to the old recursion, just
    // unrolled bottom-up).
    let mut parent_key: Option<u64> = seed_parent_key;
    for &nid in chain.iter().rev() {
        let node = &node_data[nid.index()];
        let mut hasher = crate::hash::DefaultHasher::new();

        core::mem::discriminant(node.get_node_type()).hash(&mut hasher);
        if identity.class_change_remounts {
            for attr in node.attributes().as_ref() {
                if let Some(class) = attr.as_class() {
                    class.hash(&mut hasher);
                }
            }
        }

        if let Some(parent_id) = hierarchy
            .get(nid.index())
            .and_then(NodeHierarchyItem::parent_id)
        {
            // The node's place: the siblings before `nid` that count.
            let mut sibling_index: usize = 0;
            let mut current = hierarchy
                .get(parent_id.index())
                .and_then(|h| h.first_child_id(parent_id));
            while let Some(sibling_id) = current {
                *key_steps += 1;
                if sibling_id == nid {
                    break;
                }
                let sibling = &node_data[sibling_id.index()];
                let counts = match identity.siblings_counted {
                    SiblingsCounted::All => true,
                    SiblingsCounted::SameType => {
                        core::mem::discriminant(sibling.get_node_type())
                            == core::mem::discriminant(node.get_node_type())
                    }
                };
                if counts {
                    sibling_index += 1;
                }
                current = hierarchy
                    .get(sibling_id.index())
                    .and_then(NodeHierarchyItem::next_sibling_id);
            }

            sibling_index.hash(&mut hasher);
            parent_key.unwrap_or(0).hash(&mut hasher);
        }

        parent_key = Some(hasher.finish());
    }

    parent_key.unwrap_or(0)
}

/// Precompute reconciliation keys for every node in a DOM tree.
///
/// Called once per side (old/new) at the start of `reconcile_dom`. Returns a
/// vector indexed by node index (`keys[node_id.index()]`) so lookup during
/// reconciliation is O(1).
///
/// ONE top-down pass: a node's key is its terminal key, or its type (and classes), its place
/// among its siblings and its parent's key, so each parent hands its children their places as it
/// walks them - O(nodes). [`calculate_reconciliation_key`] answers one node from the root's
/// chain by walking every earlier sibling at each level, which over a whole DOM is quadratic in
/// the children of a parent (a map view's 12,470 labels under one root: 78 million steps a side).
#[must_use]
pub fn precompute_reconciliation_keys(
    node_data: &[NodeData],
    hierarchy: &[NodeHierarchyItem],
    identity: NodeIdentityConfig,
) -> Vec<u64> {
    precompute_reconciliation_keys_metered(node_data, hierarchy, identity, &mut 0)
}

/// [`precompute_reconciliation_keys`], adding the steps it took to `key_steps`
/// ([`ReconcileWork::keys`]).
#[must_use]
pub(crate) fn precompute_reconciliation_keys_metered(
    node_data: &[NodeData],
    hierarchy: &[NodeHierarchyItem],
    identity: NodeIdentityConfig,
    key_steps: &mut usize,
) -> Vec<u64> {
    use core::hash::Hasher;

    let n = node_data.len();
    // A hierarchy that does not cover the nodes (a caller that tracks none): the keys degrade to
    // the node's type, which the per-node function answers.
    if hierarchy.len() < n {
        return (0..n)
            .map(|idx| {
                calculate_reconciliation_key_metered(
                    node_data,
                    hierarchy,
                    NodeId::new(idx),
                    identity,
                    key_steps,
                )
            })
            .collect();
    }
    let mut keys = vec![0_u64; n];
    let mut placed = vec![false; n];
    // (node, its parent's key, its place among its siblings, whether it has a parent)
    let mut pending: Vec<(usize, Option<u64>, usize, bool)> = (0..n)
        .filter(|idx| hierarchy[*idx].parent_id().is_none())
        .map(|idx| (idx, None, 0, false))
        .collect();
    while let Some((idx, parent_key, place, has_parent)) = pending.pop() {
        if placed[idx] {
            continue;
        }
        placed[idx] = true;
        *key_steps += 1;
        let node = &node_data[idx];
        let key = identity.terminal_key(node).unwrap_or_else(|| {
            let mut hasher = crate::hash::DefaultHasher::new();
            core::mem::discriminant(node.get_node_type()).hash(&mut hasher);
            if identity.class_change_remounts {
                for attr in node.attributes().as_ref() {
                    if let Some(class) = attr.as_class() {
                        class.hash(&mut hasher);
                    }
                }
            }
            if has_parent {
                place.hash(&mut hasher);
                parent_key.unwrap_or(0).hash(&mut hasher);
            }
            hasher.finish()
        });
        keys[idx] = key;
        // The children, each with its place: counted as the identity says, the siblings before it.
        let mut all_before = 0_usize;
        let mut of_type: Vec<(core::mem::Discriminant<NodeType>, usize)> = Vec::new();
        let mut child = hierarchy[idx].first_child_id(NodeId::new(idx));
        let mut walked = 0_usize;
        while let Some(child_id) = child {
            // A corrupt sibling chain: stop at the node count, as the per-node walk does.
            if child_id.index() >= n || walked >= n {
                break;
            }
            walked += 1;
            *key_steps += 1;
            let kind = core::mem::discriminant(node_data[child_id.index()].get_node_type());
            let child_place = match identity.siblings_counted {
                SiblingsCounted::All => all_before,
                SiblingsCounted::SameType => {
                    of_type.iter().find(|(d, _)| *d == kind).map_or(0, |(_, count)| *count)
                }
            };
            all_before += 1;
            if matches!(identity.siblings_counted, SiblingsCounted::SameType) {
                match of_type.iter_mut().find(|(d, _)| *d == kind) {
                    Some((_, count)) => *count += 1,
                    None => of_type.push((kind, 1)),
                }
            }
            pending.push((child_id.index(), Some(key), child_place, true));
            child = hierarchy[child_id.index()].next_sibling_id();
        }
    }
    // Whatever no root reaches (a cyclic or corrupt hierarchy): the per-node walk, which stops
    // at the node count.
    for idx in 0..n {
        if !placed[idx] {
            keys[idx] = calculate_reconciliation_key_metered(
                node_data,
                hierarchy,
                NodeId::new(idx),
                identity,
                key_steps,
            );
        }
    }
    keys
}

/// Represents a mapping between a node in the old DOM and the new DOM.
#[derive(Debug, Clone, Copy)]
pub struct NodeMove {
    /// The `NodeId` in the old DOM array
    pub old_node_id: NodeId,
    /// The `NodeId` in the new DOM array
    pub new_node_id: NodeId,
}

/// The result of a DOM diff, containing lifecycle events and node mappings.
#[derive(Debug, Clone, Default)]
pub struct DiffResult {
    /// Lifecycle events generated by the diff (Mount, Unmount, Resize, Update)
    pub events: Vec<SyntheticEvent>,
    /// Maps Old `NodeId` -> New `NodeId` for state migration (focus, scroll, etc.)
    pub node_moves: Vec<NodeMove>,
}

/// Per-node hash of the node's own content PLUS its entire subtree, children
/// folded in document order. Two equal values mean the subtrees are
/// content-identical for reconciliation purposes — the strong-identity tier
/// `reconcile_dom` uses before falling back to positional matching.
///
/// The arena is depth-first pre-order (a parent's index is always lower than
/// its children's), so one REVERSE walk sees every child before its parent.
fn compute_subtree_hashes(node_data: &[NodeData], hierarchy: &[NodeHierarchyItem]) -> Vec<u64> {
    use core::hash::{Hash, Hasher};
    let mut hashes = vec![0u64; node_data.len()];
    for idx in (0..node_data.len()).rev() {
        let mut h = crate::hash::DefaultHasher::new();
        node_data[idx].calculate_node_data_hash().hash(&mut h);
        let mut child = hierarchy
            .get(idx)
            .and_then(|item| item.first_child_id(NodeId::new(idx)));
        while let Some(c) = child {
            if c.index() >= hashes.len() {
                break;
            }
            hashes[c.index()].hash(&mut h);
            child = hierarchy
                .get(c.index())
                .and_then(NodeHierarchyItem::next_sibling_id);
        }
        hashes[idx] = h.finish();
    }
    hashes
}

/// Calculates the difference between two DOM frames and generates lifecycle events.
///
/// This is the main entry point for DOM reconciliation. It compares the old and new
/// DOM trees and produces:
/// - Mount events for new nodes
/// - Unmount events for removed nodes
/// - Resize events for nodes whose bounds changed
/// - Update events for nodes whose logical position is stable but content changed
///
/// # Matching priority
/// For every node, the reconciliation key (`calculate_reconciliation_key`) encodes
/// Priority 1 (`.with_key()`), Priority 2 (CSS ID, when `identity.id_is_key`), and
/// Priority 3 (structural key: type + place among the siblings + parent key). The
/// tiers are then tried in order:
///
/// 1. **Reconciliation key** — matches logical identity, may fire Update on content change.
/// 2. **Content hash** — exact match including content; catches pure reorders of anonymous nodes.
/// 3. **Structural hash** — matches node type + attrs ignoring text content; for text-edit cases.
///
/// Between the two runs the pass that makes a keyless tree survive being
/// re-parented: a node is also identified by the old parent its already-matched
/// children descend from, which no change of shape ABOVE it can disturb.
///
/// # Arguments
/// * `old_node_data` / `new_node_data` - Per-node data for each frame
/// * `old_hierarchy` / `new_hierarchy` - Parent/sibling pointers. Pass `&[]` if unavailable; the
///   structural-key branch of the reconciliation key degrades gracefully.
/// * `old_layout` / `new_layout` - Layout bounds used to detect Resize events
/// * `dom_id` - The DOM identifier
/// * `timestamp` - Current timestamp for events
/// * `identity` - What makes a node another node: the app's `AppConfig::node_identity`
#[allow(clippy::needless_pass_by_value)] // owned azul value taken by value (public API / ownership-transfer convention)
#[must_use]
pub fn reconcile_dom(
    old_node_data: &[NodeData],
    new_node_data: &[NodeData],
    old_hierarchy: &[NodeHierarchyItem],
    new_hierarchy: &[NodeHierarchyItem],
    old_layout: &OrderedMap<NodeId, LogicalRect>,
    new_layout: &OrderedMap<NodeId, LogicalRect>,
    dom_id: DomId,
    timestamp: Instant,
    identity: NodeIdentityConfig,
) -> DiffResult {
    reconcile_dom_metered(
        old_node_data,
        new_node_data,
        old_hierarchy,
        new_hierarchy,
        old_layout,
        new_layout,
        dom_id,
        timestamp,
        identity,
        &mut ReconcileWork::default(),
    )
}

/// [`reconcile_dom`], adding the steps its passes took to `work`.
#[allow(clippy::needless_pass_by_value)] // owned azul value taken by value (public API / ownership-transfer convention)
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
#[allow(clippy::too_many_arguments)] // `reconcile_dom`'s inputs plus the meter
#[must_use]
pub(crate) fn reconcile_dom_metered(
    old_node_data: &[NodeData],
    new_node_data: &[NodeData],
    old_hierarchy: &[NodeHierarchyItem],
    new_hierarchy: &[NodeHierarchyItem],
    old_layout: &OrderedMap<NodeId, LogicalRect>,
    new_layout: &OrderedMap<NodeId, LogicalRect>,
    dom_id: DomId,
    timestamp: Instant,
    identity: NodeIdentityConfig,
    work: &mut ReconcileWork,
) -> DiffResult {
    // Helper: pop the first non-consumed NodeId from a queue.
    fn pop_first_unconsumed(queue: &mut VecDeque<NodeId>, consumed: &[bool]) -> Option<NodeId> {
        while let Some(&old_id) = queue.front() {
            queue.pop_front();
            if !consumed[old_id.index()] {
                return Some(old_id);
            }
        }
        None
    }

    let mut result = DiffResult::default();

    // --- STEP 1: INDEX THE OLD DOM ---
    //
    // Three tiers, in priority order:
    //   Tier 1: reconciliation key (.with_key() / CSS ID / structural key, as `identity` says)
    //   Tier 2: content hash (exact node_data hash — matches pure reorders)
    //   Tier 3: structural hash (discriminant + attrs, ignores text — matches text edits)
    //
    // Each tier is keyed with a `VecDeque<NodeId>` because all three can legitimately
    // collide (two sibling divs produce the same structural key, two identical nodes
    // produce the same content hash, etc.); we consume in document order on match.

    let old_rec_keys =
        precompute_reconciliation_keys_metered(old_node_data, old_hierarchy, identity, &mut work.keys);
    // AUDIT: precompute NEW keys too so the Tier-2/Tier-3 keyless tiers can be
    // gated on parent-key agreement (see STEP 2). Also lets Tier 1 look the key
    // up instead of recomputing it per node.
    let new_rec_keys =
        precompute_reconciliation_keys_metered(new_node_data, new_hierarchy, identity, &mut work.keys);

    let mut old_by_rec_key: OrderedMap<u64, VecDeque<NodeId>> = OrderedMap::default();
    let mut old_nodes_consumed = vec![false; old_node_data.len()];

    // Tier 1's queues. The queues of Tiers 2 and 3 are built where those tiers run (passes
    // B2 / B3), over the old nodes still unconsumed there: see `ShallowTiers`.
    for (idx, rec_key) in old_rec_keys.iter().enumerate().take(old_node_data.len()) {
        old_by_rec_key
            .entry(*rec_key)
            .or_default()
            .push_back(NodeId::new(idx));
    }

    // --- STEP 2: CLAIM MATCHES, STRONG IDENTITY BEFORE POSITIONAL ---
    //
    // The old single loop ran every tier per new node, in new-document order.
    // That let a PREPENDED subtree steal identity: the auto "structural"
    // reconciliation key hashes a sibling index, so prepending a
    // banner shifts every following sibling's key by one and Tier 1 pairs
    // banner↔page0, page0↔page1, … — and (DomId, NodeId)-keyed state (text
    // overlay, focus, selections) follows the WRONG element. Shallow
    // content-hash tiers cannot save it either: a wrapper div's own hash
    // ignores the child text that actually distinguishes it.
    //
    // So matching now runs in PASSES over all new nodes, strongest evidence
    // first, positional evidence last:
    //   A1: terminal identity (explicit `.with_key()` / a CSS id that is a key)
    //   A2: exact SUBTREE hash (node + entire subtree content-identical) —
    //       deliberately NOT parent-gated: re-pagination moves a paragraph's
    //       whole subtree under a different page container, and following it
    //       there is the point.
    //   A3: bottom-up identity — a node IS the old parent its already-matched
    //       children all descend from. Nothing above either node is consulted,
    //       which is what survives chrome being injected over the root.
    //   B1: positional structural key (the old Tier 1 fallback)
    //   B2: shallow content hash   (parent-gated, as before)
    //   B3: shallow structural hash (parent-gated, as before — text edits)
    //
    // B2/B3's parent gate prefers the parent's MATCH over the parent's key, so
    // the descent picks up where A3's climb stopped: the one child whose text
    // changed is found under the parent A3 just identified.
    // A2's queues are built where A2 runs: they are keyed by the container too (see there).
    let old_subtree_hashes = compute_subtree_hashes(old_node_data, old_hierarchy);
    let new_subtree_hashes = compute_subtree_hashes(new_node_data, new_hierarchy);

    let has_terminal_identity = |node: &NodeData| identity.terminal_key(node).is_some();

    let n_new = new_node_data.len();
    let mut matched: Vec<Option<NodeId>> = vec![None; n_new];
    let mut matched_by_rec_key: Vec<bool> = vec![false; n_new];

    // Pass A1: terminal identity (explicit key / CSS ID). Their reconciliation
    // key IS the terminal key, so the existing queue serves them.
    for (new_idx, new_node) in new_node_data.iter().enumerate() {
        if !has_terminal_identity(new_node) {
            continue;
        }
        if let Some(queue) = old_by_rec_key.get_mut(&new_rec_keys[new_idx]) {
            if let Some(old_id) = pop_first_unconsumed(queue, &old_nodes_consumed) {
                old_nodes_consumed[old_id.index()] = true;
                matched[new_idx] = Some(old_id);
                matched_by_rec_key[new_idx] = true;
            }
        }
    }

    // Pass A2: exact subtree identity. An explicit `.with_key()` that missed
    // A1 stays unmatched (Mount) — a key is an intentional identity marker.
    // Identical twins (equal subtrees in one container) are left to B1's
    // positional key: matched by place, not by document order.
    //
    // Cross-parent moves are ALLOWED between anonymous parents (that is the
    // point: re-pagination shifts a paragraph's whole subtree under a
    // different page container and state must follow it) — but NOT across a
    // parent whose TERMINAL identity (explicit key / CSS ID) changed:
    // `#left → #right` is the author saying "a different container", and the
    // parent-key-gate test pins that a leaf must not migrate across it.
    let terminal_key_of = |node: &NodeData| identity.terminal_key(node);
    // The gate is the NEAREST ancestor with a terminal identity, not just the
    // parent: in AzMail's account wizard page 1's `#acct-name > p > "x"`
    // gave way to page 2's `#acct-imap-host > p > "x"`, and the text "x"
    // (its parent the anonymous `p` on both sides) matched across - the
    // user's typing in the name field moved into the host field (MAIL6).
    // Anonymous containers (re-pagination's pages) are still crossed freely.
    // One forward pass: the arena is pre-order, a parent precedes its child.
    let container_identities =
        |data: &[NodeData], hierarchy: &[NodeHierarchyItem]| -> Vec<Option<u64>> {
            let mut out: Vec<Option<u64>> = vec![None; data.len()];
            for idx in 0..data.len() {
                let container = hierarchy
                    .get(idx)
                    .and_then(NodeHierarchyItem::parent_id)
                    .filter(|p| p.index() < idx)
                    .and_then(|p| terminal_key_of(&data[p.index()]).or_else(|| out[p.index()]));
                out[idx] = container;
            }
            out
        };
    let old_containers = container_identities(old_node_data, old_hierarchy);
    let new_containers = container_identities(new_node_data, new_hierarchy);
    // TWINS - equal subtrees in one container: a sheet's empty cells, a
    // map's pending tiles, a page's repeated words - are matched by PLACE
    // (B1's positional key), not here: pairing them in document order made
    // every twin after a changed one take its neighbour's old node, so
    // selecting one cell "moved" every cell after it one place (and their
    // state went with them). The content pass is for a DISTINCT subtree that
    // moved, which is unique on both sides.
    let twin_counts = |hashes: &[u64], containers: &[Option<u64>]| {
        let mut counts: BTreeMap<(u64, Option<u64>), usize> = BTreeMap::new();
        for (idx, h) in hashes.iter().enumerate() {
            let container = containers.get(idx).copied().flatten();
            *counts.entry((*h, container)).or_default() += 1;
        }
        counts
    };
    let old_twins = twin_counts(&old_subtree_hashes, &old_containers);
    let new_twins = twin_counts(&new_subtree_hashes, &new_containers);
    // The old nodes this pass may claim, one queue per (subtree hash, container), each in
    // document order. The container is the part of the claim that depends on the NEW node, so
    // it is part of the key. There used to be one queue per subtree hash, searched with
    // `position` for the first entry that was unconsumed and stood in the new node's
    // container - and an entry A1 had consumed never left its queue. Equal subtrees in k
    // containers (the same cell under k keyed rows) that come in another order than before,
    // or behind consumed entries, took k steps each: k^2 / 2. A claim now pops the front of
    // the one queue it may take from. The node it gets is the same: the first entry in
    // document order that is unconsumed and whose container is the new node's.
    let mut old_by_subtree: OrderedMap<(u64, Option<u64>), VecDeque<NodeId>> =
        OrderedMap::default();
    for (idx, h) in old_subtree_hashes.iter().enumerate() {
        work.subtree += 1;
        let container = old_containers.get(idx).copied().flatten();
        old_by_subtree
            .entry((*h, container))
            .or_default()
            .push_back(NodeId::new(idx));
    }
    for new_idx in 0..n_new {
        if matched[new_idx].is_some() || new_node_data[new_idx].get_key().is_some() {
            continue;
        }
        let new_container = new_containers.get(new_idx).copied().flatten();
        let identity = (new_subtree_hashes[new_idx], new_container);
        if old_twins.get(&identity) != Some(&1) || new_twins.get(&identity) != Some(&1) {
            continue;
        }
        if let Some(queue) = old_by_subtree.get_mut(&identity) {
            // The entries in front of the first unconsumed one are consumed for good (A1 took
            // them) and leave the queue with it: every entry is looked at once.
            while let Some(old_id) = queue.pop_front() {
                work.subtree += 1;
                if !old_nodes_consumed[old_id.index()] {
                    old_nodes_consumed[old_id.index()] = true;
                    matched[new_idx] = Some(old_id);
                    break;
                }
            }
        }
    }

    // Pass A3: bottom-up identity — a parent is known by its children.
    //
    // A2 leaves the ancestors of ANY changed content unmatched, because their
    // subtree hash moved with it, and B1 cannot rescue them once the tree
    // SHAPE above them changed: the positional key folds every ancestor's
    // sibling index, so injecting window chrome above the document —
    // `html > [menubar, body]` becoming
    // `html > [titlebar, html > [menubar, body]]` — shifts every key in the
    // document at once. The user's whole spine (root, body, the container of
    // the one label whose counter ticked over) then mass-unmounts and takes
    // focus, scroll and dataset state with it. Worse, the injected `<html>`
    // carries the SAME root-level positional key the user's old `<html>` had,
    // so B1 hands the wrapper the old root and the real root never finds it.
    //
    // So identify a node the way a re-parent cannot disturb: by WHICH OLD
    // NODES its children turned out to be. If every already-matched child of a
    // new node descends from one and the same unconsumed old node of the same
    // kind, that old node IS this node — nothing above either of them was
    // consulted, so a wrapper above the root is invisible to the question.
    //
    // One REVERSE pass suffices: the arena is depth-first pre-order, so a
    // parent's index is always lower than its children's and a single walk
    // backwards sees every child before its parent. Identity therefore climbs
    // the entire spine in one sweep, from the leaves A2 anchored up to the
    // user's root — and stops there, because the old root has no parent for
    // the wrapper to claim.
    for new_idx in (0..n_new).rev() {
        if matched[new_idx].is_some() || new_node_data[new_idx].get_key().is_some() {
            continue;
        }
        let new_id = NodeId::new(new_idx);
        let mut candidate: Option<NodeId> = None;
        let mut agreed = true;
        let mut child = new_hierarchy
            .get(new_idx)
            .and_then(|item| item.first_child_id(new_id));
        // Bounded like the key walk above: a sibling chain is at most `n_new`
        // long, so exceeding that means the hierarchy is cyclic — stop.
        let mut guard = n_new;
        while let Some(c) = child {
            if c.index() >= n_new || guard == 0 {
                break;
            }
            guard -= 1;
            if let Some(old_child) = matched[c.index()] {
                // The old parent of a matched child. A matched child that WAS
                // the old root has none, and nothing is the parent of a root:
                // that is a disagreement, not a candidate. (Which is exactly
                // the injected wrapper — its only matched child is the user's
                // old root — so the wrapper correctly mounts.)
                let old_parent = old_hierarchy
                    .get(old_child.index())
                    .and_then(NodeHierarchyItem::parent_id);
                match (old_parent, candidate) {
                    (Some(p), None) => candidate = Some(p),
                    (Some(p), Some(existing)) if existing == p => {}
                    _ => {
                        agreed = false;
                        break;
                    }
                }
            }
            child = new_hierarchy
                .get(c.index())
                .and_then(NodeHierarchyItem::next_sibling_id);
        }
        if !agreed {
            continue;
        }
        let Some(old_id) = candidate else {
            continue;
        };
        if old_id.index() >= old_node_data.len() || old_nodes_consumed[old_id.index()] {
            continue;
        }
        let old_node = &old_node_data[old_id.index()];
        let new_node = &new_node_data[new_idx];
        // Same kind of element, and the same terminal identity: an author who
        // moved the children out of `#left` and into `#right` said "a
        // different container", and A1 already had its chance at both. A
        // changed class list makes another container where the app's rules
        // say so, as it makes another node in every other pass.
        if core::mem::discriminant(old_node.get_node_type())
            != core::mem::discriminant(new_node.get_node_type())
            || terminal_key_of(old_node) != terminal_key_of(new_node)
            || identity.classes_differ(old_node, new_node)
        {
            continue;
        }
        old_nodes_consumed[old_id.index()] = true;
        matched[new_idx] = Some(old_id);
        // Identity-by-children is LOGICAL identity, not a content coincidence,
        // so a container whose own content changed still fires `Updated` —
        // the same event it got back when B1 was the one matching it.
        matched_by_rec_key[new_idx] = true;
    }

    // Pass B1: positional structural key — the old Tier 1 for keyless nodes,
    // now running only for what strong evidence left over.
    for (new_idx, new_node) in new_node_data.iter().enumerate() {
        if matched[new_idx].is_some() || new_node.get_key().is_some() {
            continue;
        }
        if let Some(queue) = old_by_rec_key.get_mut(&new_rec_keys[new_idx]) {
            if let Some(old_id) = pop_first_unconsumed(queue, &old_nodes_consumed) {
                old_nodes_consumed[old_id.index()] = true;
                matched[new_idx] = Some(old_id);
                matched_by_rec_key[new_idx] = true;
            }
        }
    }

    // Passes B2/B3: shallow content / structural hash. Their queues are built when the first
    // new node gets here, over the old nodes no pass above consumed (`ShallowTiers`).
    let mut shallow: Option<ShallowTiers> = None;
    for (new_idx, new_node) in new_node_data.iter().enumerate() {
        if matched[new_idx].is_some() || new_node.get_key().is_some() {
            continue;
        }
        // AUDIT: parent-key of the new node. The keyless shallow tiers are
        // only allowed to claim an old node whose parent's reconciliation key
        // agrees — otherwise two structurally-identical nodes under DIFFERENT
        // parents would match and migrate focus/scroll/dataset state to an
        // unrelated subtree. When either hierarchy is unavailable this is
        // `None` on both sides, so the gate is a no-op.
        let new_parent_key: Option<u64> = new_hierarchy
            .get(new_idx)
            .and_then(NodeHierarchyItem::parent_id)
            .map(|p| new_rec_keys[p.index()]);

        // The old node this new node's PARENT was matched to, if it matched at
        // all. Two nodes whose parents turned out to be the SAME node are
        // children of one parent in both frames — "match the children of
        // matched parents", and strictly stronger evidence than comparing the
        // parents' positional keys, which a re-parent ABOVE them invalidates
        // although neither node moved. Falls back to the key comparison when
        // the parent is itself unmatched, so nothing that matches today stops.
        let new_parent_match: Option<NodeId> = new_hierarchy
            .get(new_idx)
            .and_then(NodeHierarchyItem::parent_id)
            .and_then(|p| matched.get(p.index()).copied().flatten());
        // The gate as the queues are keyed: an old node passes it when its parent IS the
        // matched one, or - the new node's parent unmatched - when its parent's key is the
        // new parent's.
        let gate = new_parent_match.map_or(ParentGate::Key(new_parent_key), ParentGate::Node);
        let tiers = shallow.get_or_insert_with(|| {
            ShallowTiers::of(
                old_node_data,
                old_hierarchy,
                &old_rec_keys,
                &old_nodes_consumed,
                work,
            )
        });

        // B2: Content hash (exact match — catches pure reorders)
        let hash = new_node.calculate_node_data_hash();
        if let Some(old_id) =
            ShallowTiers::claim(&mut tiers.content, hash, gate, &old_nodes_consumed, work)
        {
            old_nodes_consumed[old_id.index()] = true;
            matched[new_idx] = Some(old_id);
            continue;
        }

        // B3: Structural hash (text-node fallback — ignores text content)
        let structural_hash = new_node.calculate_structural_hash();
        if let Some(old_id) = ShallowTiers::claim(
            &mut tiers.structural,
            structural_hash,
            gate,
            &old_nodes_consumed,
            work,
        ) {
            old_nodes_consumed[old_id.index()] = true;
            matched[new_idx] = Some(old_id);
        }
    }

    // --- STEP 3: PROCESS MATCHES / MOUNTS, in new-document order ---

    for (new_idx, new_node) in new_node_data.iter().enumerate() {
        let new_id = NodeId::new(new_idx);
        let matched_old_id = matched[new_idx];
        let matched_by_rec_key = matched_by_rec_key[new_idx];

        if let Some(old_id) = matched_old_id {
            // FOUND A MATCH (It might be at a different index, but it's the "same" node)

            result.node_moves.push(NodeMove {
                old_node_id: old_id,
                new_node_id: new_id,
            });

            // Check for Resize
            let old_rect = old_layout
                .get(&old_id)
                .copied()
                .unwrap_or(LogicalRect::zero());
            let new_rect = new_layout
                .get(&new_id)
                .copied()
                .unwrap_or(LogicalRect::zero());

            if old_rect.size != new_rect.size {
                // Fire Resize Event
                if has_resize_callback(new_node) {
                    result.events.push(create_lifecycle_event(
                        EventType::Resize,
                        new_id,
                        dom_id,
                        &timestamp,
                        LifecycleEventData {
                            reason: LifecycleReason::Resize,
                            previous_bounds: Some(old_rect),
                            current_bounds: new_rect,
                        },
                    ));
                }
            }

            // Fire Update when the node was matched by logical identity (reconciliation
            // key: explicit .with_key(), CSS ID, or structural key) but its content hash
            // differs. Tier-2/Tier-3 matches by definition don't carry an Update — a
            // content-hash match is content-identical, and a structural-hash match is
            // a text edit handled by cursor/text reconciliation elsewhere.
            // The handler first: almost no node has one, and the two hashes walk the whole
            // node (they were computed for every matched node of every rebuild).
            if matched_by_rec_key && has_update_callback(new_node) {
                let old_hash = old_node_data[old_id.index()].calculate_node_data_hash();
                let new_hash = new_node.calculate_node_data_hash();

                if old_hash != new_hash {
                    result.events.push(create_lifecycle_event(
                        EventType::Update,
                        new_id,
                        dom_id,
                        &timestamp,
                        LifecycleEventData {
                            reason: LifecycleReason::Update,
                            previous_bounds: Some(old_rect),
                            current_bounds: new_rect,
                        },
                    ));
                }
            }
        } else {
            // NO MATCH FOUND -> MOUNT (New Node)
            if has_mount_callback(new_node) {
                let bounds = new_layout
                    .get(&new_id)
                    .copied()
                    .unwrap_or(LogicalRect::zero());
                result.events.push(create_lifecycle_event(
                    EventType::Mount,
                    new_id,
                    dom_id,
                    &timestamp,
                    LifecycleEventData {
                        reason: LifecycleReason::InitialMount,
                        previous_bounds: None,
                        current_bounds: bounds,
                    },
                ));
            }
        }
    }

    // --- STEP 4: CLEANUP (UNMOUNTS) ---
    // Any old node that wasn't claimed is effectively destroyed.

    for (old_idx, consumed) in old_nodes_consumed.iter().enumerate() {
        if !consumed {
            let old_id = NodeId::new(old_idx);
            let old_node = &old_node_data[old_idx];

            if has_unmount_callback(old_node) {
                let bounds = old_layout
                    .get(&old_id)
                    .copied()
                    .unwrap_or(LogicalRect::zero());
                result.events.push(create_lifecycle_event(
                    EventType::Unmount,
                    old_id,
                    dom_id,
                    &timestamp,
                    LifecycleEventData {
                        reason: LifecycleReason::Unmount,
                        previous_bounds: Some(bounds),
                        current_bounds: LogicalRect::zero(),
                    },
                ));
            }
        }
    }

    result
}

/// Creates a lifecycle event with all necessary fields.
fn create_lifecycle_event(
    event_type: EventType,
    node_id: NodeId,
    dom_id: DomId,
    timestamp: &Instant,
    data: LifecycleEventData,
) -> SyntheticEvent {
    let dom_node_id = DomNodeId {
        dom: dom_id,
        node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
    };
    SyntheticEvent {
        event_type,
        source: EventSource::Lifecycle,
        phase: EventPhase::Target,
        target: dom_node_id,
        current_target: dom_node_id,
        timestamp: timestamp.clone(),
        data: EventData::Lifecycle(data),
        stopped: false,
        stopped_immediate: false,
        prevented_default: false,
        at_target_only: false,
    }
}

/// The event a `<transient-window>` receives when the USER closed it — an
/// outside click, or Escape — as opposed to the app flipping `open`.
///
/// Built here, next to the other lifecycle events, so it carries the same
/// `EventSource::Lifecycle` / `EventPhase::Target` shape the dispatcher
/// expects for a `ComponentEventFilter`. `bounds` is the popup's anchor
/// rect in the parent, the closest thing to "where it was".
#[must_use]
pub fn create_dismiss_event(
    node_id: NodeId,
    dom_id: DomId,
    timestamp: &Instant,
    bounds: LogicalRect,
) -> SyntheticEvent {
    create_lifecycle_event(
        EventType::Dismiss,
        node_id,
        dom_id,
        timestamp,
        LifecycleEventData {
            reason: LifecycleReason::Dismiss,
            previous_bounds: None,
            current_bounds: bounds,
        },
    )
}

/// The lifecycle event a `<transient-window>` gets on a tear-off or a dock.
///
/// `torn == true`: torn off its anchor (`bounds` = the toplevel's rect in the
/// parent). `torn == false`: docked back (`bounds` = the anchor it docked onto).
#[must_use]
pub fn create_tearoff_event(
    node_id: NodeId,
    dom_id: DomId,
    timestamp: &Instant,
    torn: bool,
    bounds: LogicalRect,
) -> SyntheticEvent {
    let (ty, reason) = if torn {
        (EventType::TearOff, LifecycleReason::TearOff)
    } else {
        (EventType::Dock, LifecycleReason::Dock)
    };
    create_lifecycle_event(
        ty,
        node_id,
        dom_id,
        timestamp,
        LifecycleEventData {
            reason,
            previous_bounds: None,
            current_bounds: bounds,
        },
    )
}

/// Check if the node has an `AfterMount` callback registered.
fn has_mount_callback(node: &NodeData) -> bool {
    node.get_callbacks().iter().any(|cb| {
        matches!(
            cb.event,
            EventFilter::Component(ComponentEventFilter::AfterMount)
        )
    })
}

/// Check if the node has a `BeforeUnmount` callback registered.
fn has_unmount_callback(node: &NodeData) -> bool {
    node.get_callbacks().iter().any(|cb| {
        matches!(
            cb.event,
            EventFilter::Component(ComponentEventFilter::BeforeUnmount)
        )
    })
}

/// Check if the node has a `NodeResized` callback registered.
fn has_resize_callback(node: &NodeData) -> bool {
    node.get_callbacks().iter().any(|cb| {
        matches!(
            cb.event,
            EventFilter::Component(ComponentEventFilter::NodeResized)
        )
    })
}

/// Check if the node has any lifecycle callback that would respond to updates.
fn has_update_callback(node: &NodeData) -> bool {
    node.get_callbacks().iter().any(|cb| {
        matches!(
            cb.event,
            EventFilter::Component(ComponentEventFilter::Updated)
        )
    })
}

/// Migrate state (focus, scroll, etc.) from old node IDs to new node IDs.
///
/// This function should be called after reconciliation to update any state
/// that references old `NodeIds` to use the new `NodeIds`.
///
/// # Example
/// ```rust,ignore
/// let diff = reconcile_dom(...);
/// let migration_map = create_migration_map(&diff.node_moves);
///
/// // Migrate focus
/// if let Some(current_focus) = focus_manager.focused_node {
///     if let Some(&new_id) = migration_map.get(&current_focus) {
///         focus_manager.focused_node = Some(new_id);
///     } else {
///         // Focused node was unmounted, clear focus
///         focus_manager.focused_node = None;
///     }
/// }
/// ```
#[must_use]
pub fn create_migration_map(node_moves: &[NodeMove]) -> OrderedMap<NodeId, NodeId> {
    let mut map = OrderedMap::default();
    for m in node_moves {
        map.insert(m.old_node_id, m.new_node_id);
    }
    map
}

/// Suppression tag for the image-churn lint, honored from `AZ_SUPPRESS`.
pub const IMAGE_CHURN_SUPPRESS_TAG: &str = "image_churn";

/// Re-initialisations per second above which an image node is churning.
///
/// A widget legitimately rebuilds its image node with a placeholder now and
/// then — the first build after mount has no frame yet. Doing it dozens of
/// times a second means a LIVE image is being discarded and re-awaited on every
/// frame, which is a bug in how the node is built, not in the content.
const IMAGE_CHURN_PER_SEC: u32 = 10;

/// The framework notices, by itself, when an image node re-initialises at frame
/// rate — and says what is almost always wrong.
///
/// The symptom is a video or capture node that flickers: it holds a real frame,
/// the DOM rebuilds, the fresh node carries only a placeholder, and the live
/// image is thrown away until the next frame arrives 16-33ms later. On a
/// resizing window, which rebuilds continuously, that is a continuous flash.
///
/// The cause is almost always a missing DATASET + merge callback. Without one
/// the reconciler cannot tell that the rebuilt node is the same widget, so it
/// has nothing to carry forward — see `transfer_states`, which does carry the
/// previous frame when a merge callback exists.
///
/// Detection lives HERE, in the reconciler, because neither DOM shows it alone:
/// the old build has a frame, the new build has a placeholder, and only the
/// pair reveals the churn. No user code has to opt in.
///
/// What it remembers is per node: how many times it re-initialised, when that
/// window started and whether it was said. A DOM that is rebuilt keeps one
/// (the window that rebuilds it: [`transfer_states_tracking`]), so the nodes
/// of one window are never mistaken for those of another, and a test asserts
/// on the detector it made instead of on a message on stderr that nothing can
/// observe.
#[derive(Debug, Default, Clone)]
pub struct ImageChurn {
    /// By node index: `(count, window_start, warned)`. The clock is `std::time::Instant`, which
    /// panics where there is no system clock (wasm32), so the detector is absent there.
    #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
    nodes: BTreeMap<usize, (u32, std::time::Instant, bool)>,
}

impl ImageChurn {
    /// How many times this node has re-initialised inside the current window.
    #[cfg(all(feature = "std", not(target_arch = "wasm32"), test))]
    pub(crate) fn count(&self, node_index: usize) -> u32 {
        self.nodes.get(&node_index).map_or(0, |entry| entry.0)
    }

    /// Notes that the image of node `node_index` re-initialised, and says so once it does at
    /// frame rate. `carried`: the previous frame was carried forward.
    #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
    fn note(&mut self, node_index: usize, carried: bool) {
        use std::time::Instant;

        // The environment's `AZ_SUPPRESS` (installed once at the start): it is the configuration
        // of the process, as it is for every lint.
        if crate::diagnostics::settings()
            .suppress
            .contains(IMAGE_CHURN_SUPPRESS_TAG)
        {
            return;
        }

        // Per node: how many times it re-initialised, when that window started, and
        // whether we have already said so. "The time it was last updated and how
        // much" is the whole state — no history, no allocation per event.
        let now = Instant::now();
        let entry = self.nodes.entry(node_index).or_insert((0, now, false));
        if now.duration_since(entry.1).as_secs_f32() >= 1.0 {
            *entry = (1, now, entry.2);
            return;
        }
        entry.0 += 1;

        // Warn once per node. This runs on every rebuild of a resizing window; a
        // warning per frame would bury the message it is trying to deliver.
        if entry.0 < IMAGE_CHURN_PER_SEC || entry.2 {
            return;
        }
        entry.2 = true;
        let rate = entry.0;

        if carried {
            crate::diagnostics::emit(format!(
                "[azul][image-churn] node {node_index} rebuilt its image as a PLACEHOLDER {rate}x in \
                 one second. The previous frame was carried forward each time, so nothing flickers — \
                 but a live image node is being reconstructed every frame. If this is not a capture \
                 widget, build the node once and update it through the image cache. (suppress with \
                 AZ_SUPPRESS={IMAGE_CHURN_SUPPRESS_TAG})"
            ));
        } else {
            crate::diagnostics::emit(format!(
                "[azul][image-churn] node {node_index} rebuilt its image as a PLACEHOLDER {rate}x in \
                 one second and the previous frame could NOT be carried forward: this node has NO \
                 DATASET + merge callback, so the reconciler cannot tell the rebuilt node is the same \
                 widget. The live image is discarded every frame and the node falls back to its \
                 placeholder until the next one arrives — a continuous flicker. If this is a video or \
                 camera node, it is almost certainly missing its dataset: attach one with a \
                 DatasetMergeCallback (see MapWidget / ScreenCaptureWidget). (suppress with \
                 AZ_SUPPRESS={IMAGE_CHURN_SUPPRESS_TAG})"
            ));
        }
    }

    #[cfg(not(all(feature = "std", not(target_arch = "wasm32"))))]
    fn note(&mut self, _node_index: usize, _carried: bool) {}
}

/// Executes state migration between the old DOM and the new DOM based on diff results.
///
/// This iterates through matched nodes. If a match has BOTH a merge callback AND a dataset,
/// it executes the callback to transfer state from the old node to the new node.
///
/// This must be called **before** the old DOM is dropped, because we need to access its data.
///
/// # Arguments
/// * `old_node_data` - Mutable reference to the old DOM's node data (source of heavy state)
/// * `new_node_data` - Mutable reference to the new DOM's node data (target for heavy state)
/// * `node_moves` - The matched nodes from the reconciliation diff
///
/// # Example
/// ```rust,ignore
/// let diff_result = reconcile_dom(&old_data, &new_data, ...);
///
/// // Execute state migration BEFORE old_dom is dropped
/// transfer_states(&mut old_data, &mut new_data, &diff_result.node_moves);
///
/// // Now safe to drop old_dom - heavy resources have been transferred
/// drop(old_dom);
/// ```
pub fn transfer_states(
    old_node_data: &mut [NodeData],
    new_node_data: &mut [NodeData],
    node_moves: &[NodeMove],
) -> crate::refany::Repointing {
    transfer_states_tracking(
        old_node_data,
        new_node_data,
        node_moves,
        &mut ImageChurn::default(),
    )
}

/// [`transfer_states`], noting the image nodes that went back to a placeholder in `churn`.
///
/// `churn` is the [`ImageChurn`] of the DOM being rebuilt, which the window that rebuilds it keeps
/// from one rebuild to the next (a throwaway one, as [`transfer_states`] makes, never sees a node
/// twice, so it never says anything).
pub fn transfer_states_tracking(
    old_node_data: &mut [NodeData],
    new_node_data: &mut [NodeData],
    node_moves: &[NodeMove],
    churn: &mut ImageChurn,
) -> crate::refany::Repointing {
    use crate::refany::OptionRefAny;

    let mut replaced = crate::refany::Repointing::default();
    // Where the clones of a dataset a merge dropped are looked for (step 5). The first merge
    // that drops one walks the whole new arena, as every merge used to: m merging nodes times
    // n nodes, a list of 1,000 stateful rows a million node visits per rebuild. The second
    // one builds the index of which node holds a `RefAny` of which allocation
    // (`RefAnyHolders`), and from then on a merge visits the nodes listed for what it
    // dropped. A DOM with one stateful widget pays the one walk it always paid and no index;
    // a DOM with many pays one walk and one index, not a walk per widget.
    let mut walked_the_arena = false;
    let mut holders: Option<RefAnyHolders> = None;
    for movement in node_moves {
        let old_idx = movement.old_node_id.index();
        let new_idx = movement.new_node_id.index();

        // Bounds check
        if old_idx >= old_node_data.len() || new_idx >= new_node_data.len() {
            continue;
        }

        // 1. Check if the NEW node has requested a merge callback
        let Some(merge_callback) = new_node_data[new_idx].get_merge_callback() else {
            // No merge callback — nothing can be carried forward. If this node
            // is an image that just reverted to a placeholder while the old
            // build held a real frame, that live frame is being DISCARDED, and
            // at frame rate it is a visible flicker. This is the "forgot the
            // dataset on a video node" case, and the framework can see it
            // without anyone asking.
            if new_node_data[new_idx].image_is_placeholder()
                && !old_node_data[old_idx].image_is_placeholder()
            {
                churn.note(new_idx, false);
            }
            continue; // No merge callback, skip
        };

        // 2. Check if BOTH nodes have datasets
        // We need to temporarily take the datasets to satisfy borrow checker
        let old_dataset = old_node_data[old_idx].take_dataset();
        let new_dataset = new_node_data[new_idx].take_dataset();

        match (new_dataset, old_dataset) {
            (Some(new_data), Some(old_data)) => {
                // The fresh DOM's dataset allocation. A widget builds its dataset,
                // its VirtualView content `refany`, AND its event-callback
                // `refany`s from clones of ONE `RefAny` — so every one shares THIS
                // allocation (`RefAny::clone` shares `sharing_info`; only the
                // per-clone `instance_id` differs). The merge below keeps the
                // PERSISTENT (old) allocation (e.g. MapWidget shares its tile cache
                // so background fetch threads keep writing into it), so every clone
                // of the fresh one is now orphaned and must be re-pointed — or the
                // widget fragments across two caches: the VirtualView rendered an
                // empty clone (blank/grey tiles) while the live data sat in the
                // dataset, and pan/zoom mutated yet a third copy. Identity = the
                // shared `RefCountInner` pointer (`sharing_info.ptr`).
                let orphan = new_data.clone();
                // The one before goes too when the merge keeps the fresh one:
                // what still holds it (a timer, a thread started from the
                // node) has to follow.
                let held = old_data.clone();

                // 3. EXECUTE THE MERGE CALLBACK
                // The callback receives both datasets and returns the merged result
                let merged = merge_callback.invoke(new_data, old_data);

                // 3b. CARRY THE LIVE IMAGE FORWARD.
                //
                // A merge callback ran, so this is the SAME logical widget as
                // before — the reconciler matched them and the widget asked for
                // its state to persist. A capture widget rebuilds its node with
                // a PLACEHOLDER every time (`Dom::create_image(null_image)`),
                // because the fresh widget struct has no frame yet; the live
                // frame arrives later by writeback. So on every DOM rebuild the
                // node reverted to the placeholder and stayed there until the
                // next frame landed ~16-33ms later.
                //
                // That is the flash reported when resizing a window while
                // screensharing: "the screen flickers, like it is
                // re-initializing". Nothing was re-initialising — the last frame
                // was simply thrown away and re-awaited.
                //
                // NARROW ON PURPOSE: only when the NEW image is a null/
                // placeholder image and the OLD one is not. An app that
                // deliberately swaps in a real image still wins, and one that
                // deliberately clears to a placeholder is the only case this
                // changes — which is indistinguishable from "has not produced a
                // frame yet" and is what the widget itself does every rebuild.
                if new_node_data[new_idx].image_is_placeholder()
                    && !old_node_data[old_idx].image_is_placeholder()
                {
                    if let Some(prev) = old_node_data[old_idx].get_image_ref_cloned() {
                        new_node_data[new_idx].set_image_ref(prev);
                    }
                    // Handled — but still worth saying if it happens every
                    // frame, because rebuilding a live image node at 60 Hz is
                    // work nobody asked for.
                    churn.note(new_idx, true);
                }

                // 4. Store the merged result back in the new node
                new_node_data[new_idx].set_dataset(OptionRefAny::Some(merged.clone()));

                // 5. UNIFY: re-point every refany across the NEW DOM that was a
                // clone of the now-discarded fresh dataset onto the merged result,
                // so the whole widget reads ONE cache. Covers VirtualView content
                // refanys + event-callback refanys + any node's dataset cloned
                // from the same source. (Generalises the old special-case that
                // only re-pointed a VirtualView ON the merge node itself — the
                // MapWidget puts its VirtualView in a CHILD and its pan/zoom
                // callbacks on the parent, which that case missed.)
                let mut merge = crate::refany::Repointing::default();
                merge.replace(&orphan, &merged);
                merge.replace(&held, &merged);
                // The merge changed its own node: the merged dataset went in, and maybe the
                // old node's image with the data of its callback (3b). The index lists the
                // node under what it holds now, before the lists are read.
                if let Some(index) = holders.as_mut() {
                    index.enter(new_node_data, new_idx);
                }
                // `merge` is empty when the fresh dataset, the old one and the merged one
                // share one allocation: nothing was dropped, there is nothing to look for
                // (the walk used to visit every node to find that out).
                if !merge.is_empty() {
                    if walked_the_arena {
                        if holders.is_none() {
                            holders = Some(RefAnyHolders::of(new_node_data));
                        }
                        if let Some(index) = holders.as_mut() {
                            index.repoint(
                                new_node_data,
                                &merge,
                                [
                                    orphan.sharing_info.ptr as usize,
                                    held.sharing_info.ptr as usize,
                                ],
                                merged.sharing_info.ptr as usize,
                            );
                        }
                    } else {
                        walked_the_arena = true;
                        repoint_orphaned_refanys(new_node_data, &merge);
                    }
                }
                replaced.extend(merge);
            }
            (new_ds, old_ds) => {
                // One or both datasets missing - restore what we had
                if let Some(ds) = new_ds {
                    new_node_data[new_idx].set_dataset(OptionRefAny::Some(ds));
                }
                if let Some(ds) = old_ds {
                    old_node_data[old_idx].set_dataset(OptionRefAny::Some(ds));
                }
            }
        }
    }
    if let Some(index) = &holders {
        replaced.visits.add(index.visits as u64);
    }
    replaced
}

/// Points every `RefAny` across `node_data` that is a clone of an allocation
/// a merge dropped at the dataset that took its place (`replaced`).
///
/// The whole widget then reads ONE state: whatever holds a clone, the walk
/// reaches it through its node (`RefAnyHolder for NodeData`, the rows of the
/// table of a node's fields): the handlers' data, an image's and a view's,
/// the datasets, the menus' items, the presence animations. The `MapWidget`
/// puts its `VirtualView` in a CHILD and its pan/zoom callbacks on the
/// parent, which is why this scans the whole arena and not just the merge
/// node.
fn repoint_orphaned_refanys(node_data: &mut [NodeData], replaced: &crate::refany::Repointing) {
    replaced.visits.add(node_data.len() as u64);
    for node in node_data.iter_mut() {
        replaced.repoint(node);
    }
}

/// Which nodes of an arena hold a `RefAny` of which allocation: what lets a merge of
/// [`transfer_states`] visit the nodes that hold a clone of the dataset it dropped, and no
/// other node, where [`repoint_orphaned_refanys`] walks the whole arena.
///
/// The unit is the node, not the single `RefAny` in it: a holder hands out its `RefAny`s one
/// walk at a time (`RefAnyHolder::for_each_refany`) and has no address for one of them. A
/// listed node is walked as before (`Repointing::repoint`), so the lists only say where to
/// LOOK: a node may be listed twice, or under an allocation it holds no `RefAny` of any more
/// (its dataset was taken for a merge, its image replaced), and nothing happens to it.
///
/// What has to hold is the other direction - a node that holds a `RefAny` of an allocation is
/// in the list of that allocation - and it holds as long as every change of the arena is
/// entered. [`transfer_states`] changes the arena in two ways. A merge puts the merged dataset
/// (and maybe an image) into its own node: [`Self::enter`] lists that node again. And it
/// repoints the clones of what it dropped: [`Self::repoint`] moves the nodes it repointed to
/// the list of the kept allocation, so a later merge that drops THAT allocation - a chain, the
/// merges of several nodes that share one dataset - finds them there.
///
/// An allocation is known by its address (`sharing_info.ptr`). A freed allocation whose
/// address comes back as another one leaves listed nodes that hold no `RefAny` of it: looked
/// at for nothing, as above. (The ones a merge dropped are not freed while this lives: the
/// `Repointing` that is returned keeps a clone of each.)
#[derive(Default)]
struct RefAnyHolders {
    by_allocation: OrderedMap<usize, Vec<usize>>,
    /// How many nodes were entered and repointed: what [`transfer_states`] adds to the
    /// `visits` of the `Repointing` it returns.
    visits: usize,
}

impl RefAnyHolders {
    /// The index of the whole arena as it is now.
    fn of(node_data: &mut [NodeData]) -> Self {
        let mut holders = Self::default();
        for idx in 0..node_data.len() {
            holders.enter(node_data, idx);
        }
        holders
    }

    /// Lists node `idx` under every allocation it holds a `RefAny` of now.
    fn enter(&mut self, node_data: &mut [NodeData], idx: usize) {
        use crate::refany::RefAnyHolder;

        let Some(node) = node_data.get_mut(idx) else {
            return;
        };
        self.visits += 1;
        let by_allocation = &mut self.by_allocation;
        node.for_each_refany(&mut |refany: &mut RefAny| {
            let nodes = by_allocation
                .entry(refany.sharing_info.ptr as usize)
                .or_default();
            // A node's dataset and its handlers' data are mostly clones of one `RefAny` and
            // come one after the other: listed once.
            if nodes.last() != Some(&idx) {
                nodes.push(idx);
            }
        });
    }

    /// Points every clone of the allocations `merge` dropped (at the addresses `dropped`) at
    /// the dataset it kept (at `kept`), in the nodes listed for them, and lists those nodes
    /// under the kept allocation.
    ///
    /// The nodes are visited in the order of the arena, each once: the order the walk over
    /// the whole arena repoints in, so the clones of the kept dataset are made in the same
    /// sequence as there (a clone's `instance_id` counts them).
    fn repoint(
        &mut self,
        node_data: &mut [NodeData],
        merge: &crate::refany::Repointing,
        dropped: [usize; 2],
        kept: usize,
    ) {
        let mut nodes: Vec<usize> = Vec::new();
        for at in dropped {
            // `merge` has no entry for an allocation that is the kept one
            // (`Repointing::replace`): its holders stay as they are, and listed where they are.
            if at == kept {
                continue;
            }
            if let Some(listed) = self.by_allocation.remove(&at) {
                nodes.extend(listed);
            }
        }
        nodes.sort_unstable();
        nodes.dedup();
        for idx in &nodes {
            if let Some(node) = node_data.get_mut(*idx) {
                self.visits += 1;
                merge.repoint(node);
            }
        }
        if !nodes.is_empty() {
            self.by_allocation.entry(kept).or_default().extend(nodes);
        }
    }
}

/// The pre-cascade fast path's half of [`transfer_states`].
///
/// When the fresh build's fingerprints equal the retained DOM's, the cascade
/// is skipped and the retained `StyledDom` is kept; the fresh build's event
/// callbacks are installed on it (they may reference new app state). That
/// left the DATASETS behind: the fresh callbacks' `RefAny`s were clones of
/// the fresh build's dataset, the retained node kept last frame's, and no
/// merge callback ever ran — so a `RefreshDom` that rebuilt an identical DOM
/// reset every stateful widget's callback state (a slider's drag died on its
/// second move) and split the widget across two allocations, the exact
/// fragmentation [`repoint_orphaned_refanys`] exists to prevent.
///
/// Same rules as `transfer_states`, with the retained node as "old" and the
/// fresh dataset as "new": merge through the node's merge callback when it
/// has one, otherwise the fresh dataset wins; then re-point everything on the
/// retained DOM that was a clone of the fresh dataset at the result. Call it
/// AFTER the fresh callbacks have been installed on `node_data`, once per
/// fresh dataset, with `idx` the node's flattened index.
pub fn merge_fresh_dataset(
    node_data: &mut [NodeData],
    idx: usize,
    fresh: RefAny,
) -> crate::refany::Repointing {
    merge_fresh_datasets(node_data, alloc::vec![(idx, fresh)])
}

/// [`merge_fresh_dataset`] for a whole build at once - what the pre-cascade
/// fast path must call, because ONE widget's fresh datasets are clones of
/// one allocation spread over several nodes.
///
/// A dialog keeps its state on the wrapper (with the merge callback) AND on
/// its panel (no merge callback). Merged node by node, the wrapper's merge
/// re-pointed every clone of the fresh allocation - but the panel still held
/// last frame's dataset then, so there was nothing to re-point; the panel's
/// turn came next and, having no merge callback, installed the fresh
/// allocation: an orphan. A control inside the dialog then wrote its return
/// value into a copy nobody read. So, like [`transfer_states`]: install
/// EVERY fresh dataset first, then run each merge callback and re-point the
/// whole arena at its result.
pub fn merge_fresh_datasets(
    node_data: &mut [NodeData],
    fresh: Vec<(usize, RefAny)>,
) -> crate::refany::Repointing {
    use crate::refany::OptionRefAny;

    let mut replaced = crate::refany::Repointing::default();
    // 1. Every fresh dataset goes in; what each node held is kept for its
    //    merge callback.
    let mut merges: Vec<(usize, usize, RefAny)> = Vec::new();
    for (idx, fresh) in fresh {
        let Some(nd) = node_data.get_mut(idx) else {
            continue;
        };
        let retained = nd.take_dataset();
        let fresh_alloc = fresh.sharing_info.ptr as usize;
        nd.set_dataset(OptionRefAny::Some(fresh));
        if let (Some(_), Some(old)) = (nd.get_merge_callback(), retained) {
            merges.push((idx, fresh_alloc, old));
        }
    }
    // 2. Merge where the widget asked for it, and re-point every clone of
    //    the fresh allocation - on any node - at the result.
    for (idx, fresh_alloc, old) in merges {
        let Some(nd) = node_data.get_mut(idx) else {
            continue;
        };
        let Some(merge_callback) = nd.get_merge_callback() else {
            continue;
        };
        let Some(current) = nd.take_dataset() else {
            continue;
        };
        if current.sharing_info.ptr as usize != fresh_alloc {
            // An earlier merge of the same allocation already re-pointed
            // this node: it is unified, and merging again would fork it.
            nd.set_dataset(OptionRefAny::Some(current));
            continue;
        }
        let (fresh, held) = (current.clone(), old.clone());
        let merged_state = merge_callback.invoke(current, old);
        nd.set_dataset(OptionRefAny::Some(merged_state.clone()));
        let mut merge = crate::refany::Repointing::default();
        merge.replace(&fresh, &merged_state);
        merge.replace(&held, &merged_state);
        repoint_orphaned_refanys(node_data, &merge);
        replaced.extend(merge);
    }
    replaced
}

/// Puts a fresh build's payloads onto the retained arena of a DOM whose
/// fingerprints did not move (the same tree, the same styles).
///
/// Every value the table of a node's fields says is carried
/// ([`crate::layout_inputs::NodeField`]): the handlers, an image's callback,
/// a view's callback and data, the menus, the marker... - and then the
/// datasets, merged as the full path's `transfer_states` merges them. The
/// handlers and the views go in before the datasets, so a widget whose
/// handlers and view share one `RefAny` stays one allocation.
pub fn adopt_fresh_payloads(
    node_data: &mut [NodeData],
    transfers: &PreCascadeTransfers,
) -> crate::refany::Repointing {
    use crate::layout_inputs::NodeField;

    for (idx, values) in &transfers.nodes {
        if let Some(nd) = node_data.get_mut(*idx) {
            for field in NodeField::ALL {
                if field.retained().carries() && field != NodeField::Dataset {
                    field.carry(values, nd);
                }
            }
        }
    }
    let datasets = transfers
        .nodes
        .iter()
        .filter_map(|(idx, values)| Some((*idx, values.get_dataset()?.clone())))
        .collect();
    merge_fresh_datasets(node_data, datasets)
}

/// Carries the fresh values of a rebuild onto the DOM the window keeps.
///
/// The values are the ones the table of a node's fields says are carried
/// ([`crate::layout_inputs::NodeField`]), from `fresh`, a styled DOM that
/// compared alike (`crate::styled_dom::is_layout_equivalent`), onto
/// `retained`: node by node, the datasets as they are (the fresh DOM's are
/// merged already).
pub fn carry_fresh_values(retained: &mut [NodeData], fresh: &[NodeData]) {
    use crate::layout_inputs::NodeField;

    for (held, new) in retained.iter_mut().zip(fresh) {
        for field in NodeField::ALL {
            if field.retained().carries() {
                field.carry(new, held);
            }
        }
    }
}

/// Calculate a stable key for a contenteditable node using the hierarchy:
///
/// 1. **Explicit Key** - If `.with_key()` was called, use that
/// 2. **CSS ID** - If the node has a CSS ID (e.g., `#my-editor`), hash that
/// 3. **Structural Key** - Hash of `(nth-of-type, parent_key)` recursively
///
/// The structural key prevents shifting when elements are inserted before siblings.
/// For example, in `<div><p>A</p><p contenteditable>B</p></div>`, if we insert
/// a new `<p>` at the start, the contenteditable `<p>` becomes nth-child(3) but
/// its nth-of-type stays stable (it's still the 2nd `<p>`).
///
/// # Arguments
/// * `node_data` - All nodes in the DOM
/// * `hierarchy` - Parent-child relationships
/// * `node_id` - The node to calculate the key for
///
/// # Returns
/// A stable u64 key for the node
#[must_use]
pub fn calculate_contenteditable_key(
    node_data: &[NodeData],
    hierarchy: &[NodeHierarchyItem],
    node_id: NodeId,
) -> u64 {
    use core::hash::Hasher;

    let n = node_data.len();

    // Terminal (parent-independent) key: Priority 1 explicit key, else
    // Priority 2 CSS ID, else `None` (structural — needs the parent chain).
    let terminal_key = |nid: NodeId| -> Option<u64> {
        let node = &node_data[nid.index()];
        // Priority 1: Explicit key (from .with_key())
        if let Some(explicit_key) = node.get_key() {
            return Some(explicit_key);
        }
        // Priority 2: CSS ID
        for attr in node.attributes().as_ref() {
            if let Some(id) = attr.as_id() {
                let mut hasher = crate::hash::DefaultHasher::new(); // Different seed for ID keys
                hasher.write(id.as_bytes());
                return Some(hasher.finish());
            }
        }
        None
    };

    // Fast path: the node itself has an explicit key or CSS ID.
    if let Some(key) = terminal_key(node_id) {
        return key;
    }

    // Priority 3: structural key, computed ITERATIVELY up the parent chain.
    //
    // AUDIT: replaces unbounded parent-chain recursion (stack overflow on deep
    // DOMs, infinite recursion on a cyclic hierarchy). Same fold as the old
    // recursion, unrolled bottom-up and bounded by the node count.
    let mut chain: Vec<NodeId> = Vec::new();
    let mut seed_parent_key: Option<u64> = None;
    let mut cur = node_id;
    for _ in 0..n {
        if cur.index() >= n {
            break;
        }
        chain.push(cur);
        match hierarchy
            .get(cur.index())
            .and_then(NodeHierarchyItem::parent_id)
        {
            None => break,
            Some(parent) => {
                if let Some(k) = terminal_key(parent) {
                    seed_parent_key = Some(k);
                    break;
                }
                cur = parent;
            }
        }
    }

    // Fold from the topmost ancestor down to `node_id`. Unlike the
    // reconciliation key, the contenteditable structural key ALWAYS writes a
    // `parent_key` (0 at the root) and an `nth_of_type` (0 at the root), so the
    // per-level hashing is unconditional — preserve that exactly.
    let mut parent_key: u64 = seed_parent_key.unwrap_or(0);
    for &nid in chain.iter().rev() {
        let node = &node_data[nid.index()];
        let mut hasher = crate::hash::DefaultHasher::new(); // Different seed for structural keys

        let node_parent = hierarchy
            .get(nid.index())
            .and_then(NodeHierarchyItem::parent_id);

        // parent_key: 0 at the root, else the accumulated key of the level above.
        let level_parent_key = if node_parent.is_some() { parent_key } else { 0 };
        hasher.write(&level_parent_key.to_le_bytes());

        // nth-of-type: count same-discriminant siblings before `nid`.
        let node_discriminant = core::mem::discriminant(node.get_node_type());
        let nth_of_type = node_parent.map_or(0u32, |parent_id| {
            let mut count = 0u32;
            let mut sibling_id = hierarchy
                .get(parent_id.index())
                .and_then(|h| h.first_child_id(parent_id));
            while let Some(sib_id) = sibling_id {
                if sib_id == nid {
                    break;
                }
                let sibling_discriminant =
                    core::mem::discriminant(node_data[sib_id.index()].get_node_type());
                if sibling_discriminant == node_discriminant {
                    count += 1;
                }
                sibling_id = hierarchy
                    .get(sib_id.index())
                    .and_then(NodeHierarchyItem::next_sibling_id);
            }
            count
        });
        hasher.write(&nth_of_type.to_le_bytes());

        // Hash the node type discriminant (Discriminant<T> implements Hash)
        node_discriminant.hash(&mut hasher);

        // Also hash the classes for additional stability
        for attr in node.attributes().as_ref() {
            if let Some(class) = attr.as_class() {
                hasher.write(class.as_bytes());
            }
        }

        parent_key = hasher.finish();
    }

    parent_key
}

/// Reconcile cursor byte position when text content changes.
///
/// This function maps a cursor position from old text to new text, preserving
/// the cursor's logical position as much as possible:
///
/// 1. If cursor is in unchanged prefix → stays at same byte offset
/// 2. If cursor is in unchanged suffix → adjusts by length difference
/// 3. If cursor is in changed region → places at end of new content
///
/// # Arguments
/// * `old_text` - The previous text content
/// * `new_text` - The new text content
/// * `old_cursor_byte` - Cursor byte offset in old text
///
/// # Returns
/// The reconciled cursor byte offset in new text
///
/// # Example
/// ```rust,ignore
/// let old_text = "Hello";
/// let new_text = "Hello World";
/// let old_cursor = 5; // cursor at end of "Hello"
/// let new_cursor = reconcile_cursor_position(old_text, new_text, old_cursor);
/// assert_eq!(new_cursor, 5); // cursor stays at same position (prefix unchanged)
/// ```
#[must_use]
pub fn reconcile_cursor_position(old_text: &str, new_text: &str, old_cursor_byte: usize) -> usize {
    // AUDIT: every returned offset is snapped DOWN to the nearest UTF-8 char
    // boundary in `new_text` (and clamped to its length). The prefix/suffix
    // scans below compare byte-by-byte and can land mid-codepoint, so a raw
    // return value could later panic when used to slice `new_text` as a `str`.
    let snap = |offset: usize| -> usize {
        let mut o = offset.min(new_text.len());
        while o > 0 && !new_text.is_char_boundary(o) {
            o -= 1;
        }
        o
    };

    // If texts are equal, cursor is unchanged
    if old_text == new_text {
        return snap(old_cursor_byte);
    }

    // Empty old text - place cursor at end of new text
    if old_text.is_empty() {
        return new_text.len();
    }

    // Empty new text - place cursor at 0
    if new_text.is_empty() {
        return 0;
    }

    // Find common prefix (how many bytes from the start are identical)
    let common_prefix_bytes = old_text
        .bytes()
        .zip(new_text.bytes())
        .take_while(|(a, b)| a == b)
        .count();

    // If cursor was in the unchanged prefix, it stays at the same byte offset
    if old_cursor_byte <= common_prefix_bytes {
        return snap(old_cursor_byte);
    }

    // Find common suffix (how many bytes from the end are identical)
    let common_suffix_bytes = old_text
        .bytes()
        .rev()
        .zip(new_text.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();

    // Calculate where the suffix starts in old and new text
    let old_suffix_start = old_text.len().saturating_sub(common_suffix_bytes);
    let new_suffix_start = new_text.len().saturating_sub(common_suffix_bytes);

    // If cursor was in the unchanged suffix, adjust by length difference
    if old_cursor_byte >= old_suffix_start {
        // saturating: an out-of-range cursor (> old_text.len()) must clamp to the
        // end of the new text like every other path here, not underflow-panic.
        let offset_from_end = old_text.len().saturating_sub(old_cursor_byte);
        return snap(new_text.len().saturating_sub(offset_from_end));
    }

    // Cursor was in the changed region - place at end of inserted content
    // This handles insertions (cursor moves with new text) and deletions (cursor at edit point)
    snap(new_suffix_start)
}

/// Get the text content from a `NodeData` if it's a Text node.
///
/// Returns the text string if the node is `NodeType::Text`, otherwise `None`.
#[must_use]
pub fn get_node_text_content(node: &NodeData) -> Option<&str> {
    if let NodeType::Text(ref text) = node.get_node_type() {
        Some(text.as_str())
    } else {
        None
    }
}

// ============================================================================
// ChangeAccumulator — unifies all change input paths
// ============================================================================

/// Text change info for cursor/selection reconciliation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChange {
    /// The text content before the change.
    pub old_text: String,
    /// The text content after the change.
    pub new_text: String,
}

/// Per-node change report combining multiple information sources.
#[derive(Debug, Clone, Default)]
pub struct NodeChangeReport {
    /// Bitflags from DOM-level field comparison.
    pub change_set: NodeChangeSet,

    /// Highest `RelayoutScope` from any CSS property that changed on this node.
    /// This is more granular than `NodeChangeSet`'s binary LAYOUT/PAINT split.
    ///
    /// - `None` → repaint only (color, opacity, transform)
    /// - `IfcOnly` → reshape text in the containing IFC
    /// - `SizingOnly` → recompute this node's intrinsic size
    /// - `Full` → full subtree relayout (display, position, float, etc.)
    pub relayout_scope: RelayoutScope,

    /// Individual CSS properties that changed (for fine-grained cache invalidation).
    /// Empty if the change was structural (text content, node type, etc.)
    pub changed_css_properties: Vec<CssPropertyType>,

    /// If text content changed, the old and new text for cursor reconciliation.
    pub text_change: Option<TextChange>,
}

impl NodeChangeReport {
    /// Returns the `DirtyFlag` level needed for this change report.
    /// Maps `RelayoutScope` + `NodeChangeSet` → a simple tri-state.
    #[must_use]
    pub fn needs_layout(&self) -> bool {
        self.change_set.needs_layout() || self.relayout_scope > RelayoutScope::None
    }

    #[must_use]
    pub const fn needs_paint(&self) -> bool {
        self.change_set.needs_paint()
    }

    #[must_use]
    pub fn is_visually_unchanged(&self) -> bool {
        self.change_set.is_visually_unchanged() && self.relayout_scope == RelayoutScope::None
    }
}

/// Unified change report that merges information from all three change paths:
///
/// 1. **DOM reconciliation** (`compute_node_changes` after `reconcile_dom`)
/// 2. **CSS restyle** (`restyle_on_state_change` for hover/focus/active)
/// 3. **Runtime edits** (`words_changed`, `css_properties_changed`, `images_changed`)
///
/// This is the single source of truth for "what work needs to happen this frame".
#[derive(Debug, Clone, Default)]
pub struct ChangeAccumulator {
    /// Per-node change info. Key is the new-DOM `NodeId`.
    pub per_node: BTreeMap<NodeId, NodeChangeReport>,

    /// Maximum `RelayoutScope` across all changed nodes.
    /// Quick check: if this is `None`, we can skip layout entirely.
    pub max_scope: RelayoutScope,

    /// Nodes that are newly mounted (no old counterpart).
    /// These always need full layout.
    pub mounted_nodes: Vec<NodeId>,

    /// Nodes that were unmounted (no new counterpart).
    /// Used for cleanup (remove from scroll/focus/cursor managers).
    pub unmounted_nodes: Vec<NodeId>,
}

/// Whether no pixel of a window is drawn from a property of this type: its value is read by
/// the OS (the pointer shape over the node), never by the display list.
///
/// A restyle that changed
/// nothing else asks for no repaint and no layout.
#[must_use]
pub const fn paints_nothing(ty: CssPropertyType) -> bool {
    matches!(ty, CssPropertyType::Cursor)
}

impl ChangeAccumulator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if no changes were detected at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.per_node.is_empty() && self.mounted_nodes.is_empty() && self.unmounted_nodes.is_empty()
    }

    /// Returns true if layout work is needed (any node has scope > None).
    #[must_use]
    pub fn needs_layout(&self) -> bool {
        self.max_scope > RelayoutScope::None
            || !self.mounted_nodes.is_empty()
            || self.per_node.values().any(NodeChangeReport::needs_layout)
    }

    /// Returns true if only paint work is needed (no layout).
    #[must_use]
    pub fn needs_paint_only(&self) -> bool {
        !self.needs_layout() && self.per_node.values().any(NodeChangeReport::needs_paint)
    }

    /// Returns true if only non-visual changes occurred (callbacks, dataset, a11y).
    #[must_use]
    pub fn is_visually_unchanged(&self) -> bool {
        self.mounted_nodes.is_empty()
            && self.unmounted_nodes.is_empty()
            && self.max_scope == RelayoutScope::None
            && self
                .per_node
                .values()
                .all(NodeChangeReport::is_visually_unchanged)
    }

    /// Add a node change from DOM reconciliation (Path A).
    pub fn add_dom_change(
        &mut self,
        new_node_id: NodeId,
        change_set: NodeChangeSet,
        relayout_scope: RelayoutScope,
        text_change: Option<TextChange>,
        changed_css_properties: Vec<CssPropertyType>,
    ) {
        if relayout_scope > self.max_scope {
            self.max_scope = relayout_scope;
        }

        let report = self.per_node.entry(new_node_id).or_default();
        report.change_set |= change_set;
        if relayout_scope > report.relayout_scope {
            report.relayout_scope = relayout_scope;
        }
        if text_change.is_some() {
            report.text_change = text_change;
        }
        report.changed_css_properties.extend(changed_css_properties);
    }

    /// Add a text change (from runtime edit or DOM reconciliation).
    pub fn add_text_change(&mut self, node_id: NodeId, old_text: String, new_text: String) {
        let scope = RelayoutScope::IfcOnly;
        if scope > self.max_scope {
            self.max_scope = scope;
        }

        let report = self.per_node.entry(node_id).or_default();
        report.change_set.insert(NodeChangeSet::TEXT_CONTENT);
        if scope > report.relayout_scope {
            report.relayout_scope = scope;
        }
        report.text_change = Some(TextChange { old_text, new_text });
    }

    /// Add a CSS property change (from runtime edit or restyle).
    pub fn add_css_change(
        &mut self,
        node_id: NodeId,
        prop_type: CssPropertyType,
        scope: RelayoutScope,
    ) {
        if scope > self.max_scope {
            self.max_scope = scope;
        }

        let report = self.per_node.entry(node_id).or_default();
        if scope > RelayoutScope::None {
            report.change_set.insert(NodeChangeSet::INLINE_STYLE_LAYOUT);
        } else {
            report.change_set.insert(NodeChangeSet::INLINE_STYLE_PAINT);
        }
        if scope > report.relayout_scope {
            report.relayout_scope = scope;
        }
        report.changed_css_properties.push(prop_type);
    }

    /// Add an image change (from runtime edit or DOM reconciliation).
    pub fn add_image_change(&mut self, node_id: NodeId, scope: RelayoutScope) {
        if scope > self.max_scope {
            self.max_scope = scope;
        }

        let report = self.per_node.entry(node_id).or_default();
        report.change_set.insert(NodeChangeSet::IMAGE_CHANGED);
        if scope > report.relayout_scope {
            report.relayout_scope = scope;
        }
    }

    /// Add a mounted (new) node.
    pub fn add_mount(&mut self, node_id: NodeId) {
        self.mounted_nodes.push(node_id);
    }

    /// Add an unmounted (removed) node.
    pub fn add_unmount(&mut self, node_id: NodeId) {
        self.unmounted_nodes.push(node_id);
    }

    /// Merge a `RestyleResult` (from `restyle_on_state_change()`) into this accumulator.
    ///
    /// This is the bridge between Path B (restyle) and the unified change pipeline.
    /// Each `ChangedCssProperty` is classified via `relayout_scope()` to determine
    /// whether it affects layout or only paint.
    pub fn merge_restyle_result(&mut self, restyle: &crate::styled_dom::RestyleResult) {
        for (node_id, changed_props) in &restyle.changed_nodes {
            for changed in changed_props {
                let prop_type = changed.current_prop.get_type();
                // A property no pixel of the window is drawn from - the pointer shape a
                // `:hover { cursor: pointer }` asks the OS for - changes nothing the frame
                // shows: it is no paint work, and a hover that only moved it rebuilds no list.
                if paints_nothing(prop_type) {
                    continue;
                }
                let scope = prop_type.relayout_scope(true); // conservative
                self.add_css_change(*node_id, prop_type, scope);
            }
        }
    }

    /// Populate this accumulator from an `ExtendedDiffResult` + the old/new DOM data.
    ///
    /// This converts per-node `NodeChangeSet` flags into full `NodeChangeReport`s
    /// with `RelayoutScope` classification.
    pub fn merge_extended_diff(
        &mut self,
        extended: &ExtendedDiffResult,
        old_node_data: &[NodeData],
        new_node_data: &[NodeData],
    ) {
        for &(old_id, new_id, ref change_set) in &extended.node_changes {
            if change_set.is_empty() {
                continue;
            }

            // Determine RelayoutScope from the change flags
            let scope = Self::classify_change_scope(*change_set, new_node_data, new_id);

            // Extract text change info if TEXT_CONTENT flag is set
            let text_change = if change_set.contains(NodeChangeSet::TEXT_CONTENT) {
                let old_text = get_node_text_content(&old_node_data[old_id.index()])
                    .unwrap_or("")
                    .to_string();
                let new_text = get_node_text_content(&new_node_data[new_id.index()])
                    .unwrap_or("")
                    .to_string();
                Some(TextChange { old_text, new_text })
            } else {
                None
            };

            self.add_dom_change(new_id, *change_set, scope, text_change, Vec::new());
        }

        // Track mounts: new nodes that didn't match anything in old
        let matched_new: alloc::collections::BTreeSet<usize> = extended
            .diff
            .node_moves
            .iter()
            .map(|m| m.new_node_id.index())
            .collect();

        for idx in 0..new_node_data.len() {
            if !matched_new.contains(&idx) {
                self.add_mount(NodeId::new(idx));
            }
        }

        // Track unmounts: old nodes that didn't match anything in new
        let matched_old: alloc::collections::BTreeSet<usize> = extended
            .diff
            .node_moves
            .iter()
            .map(|m| m.old_node_id.index())
            .collect();

        for idx in 0..old_node_data.len() {
            if !matched_old.contains(&idx) {
                self.add_unmount(NodeId::new(idx));
            }
        }
    }

    /// Classify a `NodeChangeSet` into the appropriate `RelayoutScope`.
    fn classify_change_scope(
        change_set: NodeChangeSet,
        new_node_data: &[NodeData],
        new_node_id: NodeId,
    ) -> RelayoutScope {
        // NODE_TYPE_CHANGED or CHILDREN_CHANGED → Full
        if change_set.contains(NodeChangeSet::NODE_TYPE_CHANGED)
            || change_set.contains(NodeChangeSet::CHILDREN_CHANGED)
        {
            return RelayoutScope::Full;
        }

        // IDS_AND_CLASSES → Full (conservative: class change may add layout-affecting CSS)
        if change_set.contains(NodeChangeSet::IDS_AND_CLASSES) {
            return RelayoutScope::Full;
        }

        // CUSTOM_PROPERTIES → Full: any descendant may read the variable,
        // layout properties included.
        if change_set.contains(NodeChangeSet::CUSTOM_PROPERTIES) {
            return RelayoutScope::Full;
        }

        // CONTENT_LANGUAGE → Full: every inline formatting context below may
        // hyphenate its lines anew, and the boxes holding them resize.
        if change_set.contains(NodeChangeSet::CONTENT_LANGUAGE) {
            return RelayoutScope::Full;
        }

        // DESCENDANT_INPUTS → Full: every box below the node reads it.
        // OWN_BOX_INPUTS → Full as well: a cell's span re-grids its whole
        // table, which no scope narrower than the subtree describes.
        if change_set.intersects(NodeChangeSet::DESCENDANT_INPUTS | NodeChangeSet::OWN_BOX_INPUTS) {
            return RelayoutScope::Full;
        }

        // INLINE_STYLE_LAYOUT → could be IfcOnly, SizingOnly, or Full
        // We need to check individual properties for the exact scope.
        // For now, we use SizingOnly as a conservative default since
        // the individual property scopes were already checked in compute_node_changes.
        if change_set.contains(NodeChangeSet::INLINE_STYLE_LAYOUT) {
            // Walk the inline CSS properties to find the max scope
            let new_node = &new_node_data[new_node_id.index()];
            let mut max_scope = RelayoutScope::None;
            for (prop, _conds) in new_node.style.iter_inline_properties() {
                let scope = prop.get_type().relayout_scope(true);
                if scope > max_scope {
                    max_scope = scope;
                }
            }
            // The node's `var()` / `env()` references count as their
            // property (a changed `display: var(..)` is as Full as a static
            // one); the static view above skips them.
            for (reference, _conds) in non_static_declarations(new_node).0 {
                let scope = reference.default_value.get_type().relayout_scope(true);
                if scope > max_scope {
                    max_scope = scope;
                }
            }
            return if max_scope == RelayoutScope::None {
                RelayoutScope::SizingOnly // conservative fallback
            } else {
                max_scope
            };
        }

        // TEXT_CONTENT → IfcOnly (reshape text, may cascade)
        if change_set.contains(NodeChangeSet::TEXT_CONTENT) {
            return RelayoutScope::IfcOnly;
        }

        // IMAGE_CHANGED → SizingOnly (intrinsic size may change)
        if change_set.contains(NodeChangeSet::IMAGE_CHANGED) {
            return RelayoutScope::SizingOnly;
        }

        // CONTENTEDITABLE → SizingOnly
        if change_set.contains(NodeChangeSet::CONTENTEDITABLE) {
            return RelayoutScope::SizingOnly;
        }

        // Paint-only or no-visual changes
        if change_set.intersects(NodeChangeSet::AFFECTS_PAINT) {
            return RelayoutScope::None;
        }

        RelayoutScope::None
    }
}

/// Perform a full reconciliation with change detection.
///
/// This combines `reconcile_dom()` + `compute_node_changes()` into a single
/// pass that produces an `ExtendedDiffResult` with per-node change flags.
///
/// The `ChangeAccumulator` can then be populated from the result via
/// `accumulator.merge_extended_diff()`.
#[must_use]
pub fn reconcile_dom_with_changes(
    old_node_data: &[NodeData],
    new_node_data: &[NodeData],
    old_hierarchy: &[NodeHierarchyItem],
    new_hierarchy: &[NodeHierarchyItem],
    old_styled_nodes: Option<&[StyledNodeState]>,
    new_styled_nodes: Option<&[StyledNodeState]>,
    old_layout: &OrderedMap<NodeId, LogicalRect>,
    new_layout: &OrderedMap<NodeId, LogicalRect>,
    dom_id: DomId,
    timestamp: Instant,
    identity: NodeIdentityConfig,
) -> ExtendedDiffResult {
    // Step 1: Run standard reconciliation
    let diff = reconcile_dom(
        old_node_data,
        new_node_data,
        old_hierarchy,
        new_hierarchy,
        old_layout,
        new_layout,
        dom_id,
        timestamp,
        identity,
    );

    // Step 2: For each matched pair, compute what changed
    let mut node_changes = Vec::new();
    for node_move in &diff.node_moves {
        let old_nd = &old_node_data[node_move.old_node_id.index()];
        let new_nd = &new_node_data[node_move.new_node_id.index()];

        let old_state = old_styled_nodes.and_then(|s| s.get(node_move.old_node_id.index()));
        let new_state = new_styled_nodes.and_then(|s| s.get(node_move.new_node_id.index()));

        let changes = compute_node_changes(old_nd, new_nd, old_state, new_state);
        node_changes.push((node_move.old_node_id, node_move.new_node_id, changes));
    }

    ExtendedDiffResult { diff, node_changes }
}

// ============================================================================
// NodeDataFingerprint — multi-field hash for fast change detection
// ============================================================================

/// Per-node hash broken into independent fields for fast change detection.
///
/// Instead of a single u64 hash (which loses all granularity), this stores
/// separate hashes per field category. Comparing two fingerprints is O(1)
/// (6 integer comparisons) and immediately tells us WHICH category changed,
/// avoiding the more expensive `compute_node_changes()` for unchanged nodes.
///
/// Two-tier strategy:
/// - **Tier 1** (this struct): O(1) per node, identifies which categories changed.
/// - **Tier 2** (`compute_node_changes`): O(n) per changed field, does field-by-field comparison
///   only for nodes that Tier 1 identified as changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NodeDataFingerprint {
    /// Hash of `node_type` (Text content, Image ref, Div, etc.)
    pub content_hash: u64,
    /// Hash of `styled_node_state` (hover, focus, active bits)
    pub state_hash: u64,
    /// Hash of inline CSS properties: every declaration, except that an inset and an exact
    /// `transform` / `opacity` count with their property type only (their values are the
    /// two hashes below; `hash_inline_style_by_reach`)
    pub inline_css_hash: u64,
    /// Hash of the values of the inline insets (`top`, `right`, `bottom`, `left`)
    pub inline_inset_hash: u64,
    /// Hash of the values of an inline `transform` (of at least one function) and `opacity`:
    /// what the display list binds by key
    pub inline_gpu_hash: u64,
    /// Hash of `ids_and_classes`, the attributes that select style rules
    /// ([`crate::layout_inputs::LayoutReach::Selectors`])
    pub ids_classes_hash: u64,
    /// Hash of callbacks (event types + function pointers)
    pub callbacks_hash: u64,
    /// Hash of the layout-relevant attributes (contenteditable and the
    /// anonymous-box flag - never the tab index, see [`Self::compute`])
    pub attrs_hash: u64,
    /// Hash of the dataset's PRESENCE and TYPE — never its allocation.
    ///
    /// A widget's dataset is state, not layout. Most widgets allocate a fresh
    /// `RefAny` on every build, and `RefAny` hashes by pointer, so hashing the
    /// dataset into `attrs_hash` (which maps to `CONTENTEDITABLE`, a layout
    /// change) made every `with_dataset` node LAYOUT-DIRTY on every
    /// `RefreshDom`: the `TextArea` became a standalone layout root on each
    /// callback, was re-laid-out with its `min-height` while its flex
    /// container kept the old slot, and painted 64 px into a 36 px slot — over
    /// the slider beneath it. A dataset change is [`NodeChangeSet::DATASET`],
    /// which affects neither layout nor paint.
    pub dataset_hash: u64,
    /// Hash of the `lang` attributes: the content language the node states.
    /// Its descendants inherit it, and lines hyphenate in it, so a change
    /// is [`NodeChangeSet::CONTENT_LANGUAGE`] and makes the layout compare
    /// the line styles of the subtree, as an inherited style change does
    /// ([`Self::may_restyle_descendants`]).
    pub lang_hash: u64,
    /// Hash of what the node's own box is laid out and painted from besides
    /// its type, style and flags: a cell's span, an SVG text's position, a
    /// shape's geometry ([`crate::layout_inputs::LayoutReach::OwnBox`]). A
    /// change is [`NodeChangeSet::OWN_BOX_INPUTS`].
    pub own_box_hash: u64,
    /// Hash of what the boxes below the node read from it: the legacy
    /// `align`, an SVG group's `transform` and stroke, an `<svg>`'s `viewBox`
    /// ([`crate::layout_inputs::LayoutReach::Descendants`]). A change is
    /// [`NodeChangeSet::DESCENDANT_INPUTS`], and every box below the node is
    /// laid out again ([`Self::rebuilds_descendants`]).
    pub descendants_hash: u64,
    /// Whether the node itself makes it a hit-test target: a handler of an
    /// event that is not the window's, a tab stop, a context menu
    /// ([`crate::layout_inputs::HitTestInputs`]). A change is
    /// [`NodeChangeSet::HIT_TEST_TARGET`]: the display list paints the node
    /// anew, with its hit-test area or without one.
    pub hit_test_target: bool,
}

impl NodeDataFingerprint {
    /// Compute a fingerprint from a node's data and styled state.
    #[must_use]
    pub fn compute(node: &NodeData, styled_state: Option<&StyledNodeState>) -> Self {
        use core::hash::{Hash, Hasher};

        // Content hash
        let content_hash = {
            let mut h = crate::hash::DefaultHasher::new();
            node.get_node_type().hash(&mut h);
            h.finish()
        };

        // State hash
        let state_hash = {
            let mut h = crate::hash::DefaultHasher::new();
            if let Some(state) = styled_state {
                state.hash(&mut h);
            }
            h.finish()
        };

        // Inline CSS hash — full CssProperty value (matches the legacy
        // CssPropertyWithConditions::hash that hashed both property and the
        // condition vec length).
        let (inline_css_hash, inline_inset_hash, inline_gpu_hash) =
            hash_inline_style_by_reach(&node.style);

        // What the layout reads of the attributes and the SVG data, one hash
        // per reach, from one walk over them (the layout's attribute table).
        let inputs = crate::layout_inputs::LayoutInputHashes::of(node);

        // Callbacks hash
        let callbacks_hash = {
            let mut h = crate::hash::DefaultHasher::new();
            for cb in node.callbacks.as_ref() {
                cb.event.hash(&mut h);
                cb.callback.hash(&mut h);
            }
            h.finish()
        };

        // Attributes hash — the layout-relevant ones only: contenteditable
        // and the anonymous-box bit. NOT the tab index, although it lives in
        // the same `flags` word: which node Tab lands on changes neither
        // layout nor paint, and hashed here it made every row of a
        // roving-tabindex group (radio group, segmented control, tab list)
        // LAYOUT-dirty whenever its stop moved - rebuilt as a fresh relayout
        // root on each selection change. A tab-index-only change now
        // fingerprints identical; the node data it lives in is read live by
        // the focus manager, so nothing cached goes stale.
        let attrs_hash = {
            let mut h = crate::hash::DefaultHasher::new();
            node.is_contenteditable().hash(&mut h);
            node.flags.is_anonymous().hash(&mut h);
            h.finish()
        };

        // Dataset hash: presence + type, NOT the allocation (see the field doc).
        let dataset_hash = {
            let mut h = crate::hash::DefaultHasher::new();
            match node.get_dataset() {
                Some(ds) => {
                    true.hash(&mut h);
                    ds.get_type_id().hash(&mut h);
                }
                None => false.hash(&mut h),
            }
            h.finish()
        };

        Self {
            content_hash,
            state_hash,
            inline_css_hash,
            inline_inset_hash,
            inline_gpu_hash,
            ids_classes_hash: inputs.selectors,
            callbacks_hash,
            attrs_hash,
            dataset_hash,
            lang_hash: inputs.lines,
            own_box_hash: inputs.own_box,
            descendants_hash: inputs.descendants,
            hit_test_target: crate::layout_inputs::HitTestInputs::of(node).make_a_target(),
        }
    }

    /// Returns a quick `NodeChangeSet` by comparing two fingerprints.
    /// This is O(1) — just comparing their u64s.
    ///
    /// The result is *conservative*: if a field hash differs, we set the
    /// broadest applicable flag. For precise classification (e.g., which
    /// CSS properties changed and their `relayout_scope()`), the caller
    /// should fall back to `compute_node_changes()` for changed nodes.
    #[must_use]
    pub const fn diff(&self, other: &Self) -> NodeChangeSet {
        let mut changes = NodeChangeSet::empty();

        if self.content_hash != other.content_hash {
            // Could be TEXT_CONTENT, IMAGE_CHANGED, or NODE_TYPE_CHANGED
            // We set both TEXT_CONTENT and IMAGE_CHANGED conservatively;
            // compute_node_changes() will refine this.
            changes.insert(NodeChangeSet::TEXT_CONTENT);
            changes.insert(NodeChangeSet::IMAGE_CHANGED);
        }

        if self.state_hash != other.state_hash {
            changes.insert(NodeChangeSet::STYLED_STATE);
        }

        if self.inline_css_hash != other.inline_css_hash {
            // Conservative: inline CSS could affect layout or paint.
            // compute_node_changes() checks relayout_scope() per property.
            changes.insert(NodeChangeSet::INLINE_STYLE_LAYOUT);
        } else if self.inline_inset_hash != other.inline_inset_hash {
            // The same declarations, other inset values: the box is placed again (still a
            // layout change, see the flag).
            changes.insert(NodeChangeSet::INLINE_STYLE_INSETS);
        } else if self.inline_gpu_hash != other.inline_gpu_hash {
            // The same declarations and insets, another transform matrix or opacity: no
            // box moves, the display list takes the value.
            changes.insert(NodeChangeSet::INLINE_STYLE_PAINT);
        }

        if self.ids_classes_hash != other.ids_classes_hash {
            changes.insert(NodeChangeSet::IDS_AND_CLASSES);
        }

        if self.callbacks_hash != other.callbacks_hash {
            changes.insert(NodeChangeSet::CALLBACKS);
        }

        if self.attrs_hash != other.attrs_hash {
            changes.insert(NodeChangeSet::TAB_INDEX);
            changes.insert(NodeChangeSet::CONTENTEDITABLE);
        }

        if self.dataset_hash != other.dataset_hash {
            changes.insert(NodeChangeSet::DATASET);
        }

        if self.lang_hash != other.lang_hash {
            changes.insert(NodeChangeSet::CONTENT_LANGUAGE);
        }

        if self.own_box_hash != other.own_box_hash {
            changes.insert(NodeChangeSet::OWN_BOX_INPUTS);
        }

        if self.descendants_hash != other.descendants_hash {
            changes.insert(NodeChangeSet::DESCENDANT_INPUTS);
        }

        if self.hit_test_target != other.hit_test_target {
            changes.insert(NodeChangeSet::HIT_TEST_TARGET);
        }

        changes
    }

    /// Returns true if the fingerprint is identical (no changes at all).
    #[must_use]
    pub fn is_identical(&self, other: &Self) -> bool {
        self == other
    }

    /// Quick check: could this change affect layout?
    #[must_use]
    pub const fn might_affect_layout(&self, other: &Self) -> bool {
        self.content_hash != other.content_hash
            || self.inline_css_hash != other.inline_css_hash
            || self.inline_inset_hash != other.inline_inset_hash
            || self.ids_classes_hash != other.ids_classes_hash
            || self.attrs_hash != other.attrs_hash
            || self.lang_hash != other.lang_hash
            || self.own_box_hash != other.own_box_hash
            || self.descendants_hash != other.descendants_hash
    }

    /// Quick check: could this change affect visuals at all?
    #[must_use]
    pub const fn might_affect_visuals(&self, other: &Self) -> bool {
        self.content_hash != other.content_hash
            || self.state_hash != other.state_hash
            || self.inline_css_hash != other.inline_css_hash
            || self.inline_inset_hash != other.inline_inset_hash
            || self.inline_gpu_hash != other.inline_gpu_hash
            || self.ids_classes_hash != other.ids_classes_hash
            || self.lang_hash != other.lang_hash
            || self.own_box_hash != other.own_box_hash
            || self.descendants_hash != other.descendants_hash
    }

    /// Whether a change from `self` to `other` may move what the node's
    /// descendants inherit: a style its inline CSS declares, its ids and
    /// classes select or its state (`:hover`, `:focus`) switches on, or the
    /// content language its `lang` attribute states. The node's content,
    /// callbacks and other attributes are its own.
    ///
    /// The layout's reconcile asks this of every node: below one that says
    /// yes, it compares what each text and each inline formatting context
    /// was laid out with against what they inherit now.
    #[must_use]
    pub const fn may_restyle_descendants(&self, other: &Self) -> bool {
        self.inline_css_hash != other.inline_css_hash
            || self.inline_inset_hash != other.inline_inset_hash
            || self.inline_gpu_hash != other.inline_gpu_hash
            || self.ids_classes_hash != other.ids_classes_hash
            || self.state_hash != other.state_hash
            || self.lang_hash != other.lang_hash
    }

    /// Whether a change from `self` to `other` moves what the boxes below
    /// the node read from it ([`Self::descendants_hash`]): the layout's
    /// reconcile builds every node below such a node fresh.
    #[must_use]
    pub const fn rebuilds_descendants(&self, other: &Self) -> bool {
        self.descendants_hash != other.descendants_hash
    }

    /// Whether the node states anything the boxes below it read from it: a
    /// node new to the tree that does is laid out with every box below it.
    #[must_use]
    pub const fn reaches_descendants(&self) -> bool {
        self.descendants_hash != 0
    }
}

// ============================================================================
// Pre-cascade DOM fingerprints (two tiers: STRUCTURE vs STYLE)
// ============================================================================

/// Two-tier fingerprints of a recursive [`crate::dom::Dom`].
///
/// Computed BEFORE the cascade, in the same pre-order the flattener
/// (`convert_dom_into_compact_dom`) assigns `NodeId`s — index `i` in each Vec
/// is flattened `NodeId(i)`.
///
/// WHY TWO TIERS (user directive 2026-08-08): "the start should just scan
/// over the `NodeHierarchy` to discover anything that changed, which is
/// iterating over a minimal array" — and css must be EXCLUDED from that
/// first equivalence, because a stylesheet can only affect the subtree it
/// is attached to:
///
/// - **structure**: hierarchy shape + node content (`node_type`, ids/classes, attributes, callback
///   EVENT types). NO css of any kind. If this tier is equal, the old tree, its shaped text and its
///   intrinsic caches are all reusable — and if the style tier is ALSO equal, the previous CASCADE
///   is reusable wholesale (skip `create_from_dom` entirely).
/// - **style**: per-node inline css + (at subtree roots that carry `.with_css()` sheets) the sheet
///   content. A difference here with an equal structure tier means: keep the tree, re-cascade the
///   affected subtree(s) only.
///
/// The per-node arrays exist so a mismatch NAMES the changed nodes (the
/// eventual dirty-set for scoped re-cascade / word-granular text relayout);
/// the root folds make the equal case one u64 compare per tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomFingerprints {
    /// Per-node structural hash, pre-order. Folds: `node_type` content
    /// (image-callback nodes hash (fn ptr, `RefAny` `type_id`) — the `RefAny`
    /// INSTANCE is rebuilt every frame by design and is transferred, not
    /// compared; mirrors `is_layout_equivalent`), ids+classes, callback
    /// event types, contenteditable/flags/dataset, and child COUNT (pre-order
    /// alone cannot distinguish `[a [b] c]` from `[a [b c]]`).
    pub structure: Vec<u64>,
    /// Per-node style hash, pre-order: inline css properties + conditions,
    /// plus the node's attached `.with_css()` sheets (path, declarations,
    /// @-conditions, priority per rule).
    pub style: Vec<u64>,
    /// Order-sensitive fold of `structure`.
    pub structure_root: u64,
    /// Order-sensitive fold of `style`.
    pub style_root: u64,
    /// For every TEXT node, in pre-order: its index and its structure hash with the CONTENT
    /// of the text left out (that it is a text, whether it is empty, whether it is
    /// localizable, and every other field and the child count as in `structure`). Two builds
    /// whose `style` and `text_shapes` are equal and whose `structure` differs only at text
    /// nodes differ in the characters of those texts and in nothing else
    /// ([`texts_alone_changed`]): the same tree, the same nodes, the same styles.
    pub text_shapes: Vec<(usize, u64)>,
    /// Per node, pre-order: how many nodes its subtree holds, itself included. A pre-order
    /// with every node's subtree size has ONE shape, so two builds with equal `subtree_len`
    /// are the same tree whatever their nodes are: index `i` names the node at the same
    /// place in both ([`same_tree_changes`]).
    pub subtree_len: Vec<u32>,
    /// `(index, hash)` of every node that has sheets attached (`Dom::with_css` with rules,
    /// `Dom::with_component_css`), in pre-order: the sheet half of `style`, alone. Equal
    /// lists mean every sheet is what it was and hangs where it hung, so a `style` hash that
    /// still differs is a node's own inline style.
    pub sheets: Vec<(usize, u64)>,
}

/// The payloads of a fresh build the pre-cascade skip carries onto the DOM it
/// retains: what the table of a node's fields says is carried
/// ([`crate::layout_inputs::NodeField::values_to_carry`]).
///
/// The skip keeps last build's `StyledDom`, but the values every build makes
/// anew have to be the fresh build's: the handlers, an image's callback and a
/// view's callback and data may reference new app state, a menu's handlers
/// too, a marker is minted anew. Indices are flattened `NodeIds`.
#[derive(Debug, Default, Clone)]
pub struct PreCascadeTransfers {
    /// `(flattened NodeId index, a node holding the fresh build's values to
    /// carry and nothing else)` for every node that holds one, in pre-order.
    pub nodes: Vec<(usize, NodeData)>,
}

impl PreCascadeTransfers {
    /// Whether the fresh build holds a virtual view: a DOM of its own, which
    /// a retained arena would not bring back.
    #[must_use]
    pub fn holds_views(&self) -> bool {
        self.nodes
            .iter()
            .any(|(_, values)| values.get_virtual_view_node_ref().is_some())
    }
}

/// Walk a recursive [`crate::dom::Dom`] once, pre-order.
///
/// Produces both fingerprint tiers and the `RefAny` transfer list. Cost: one hash pass over
/// node data — no cascade, no allocation proportional to anything but node
/// count.
#[allow(clippy::too_many_lines)] // cohesive single-pass walker; splitting adds state-threading
#[must_use]
pub fn fingerprint_dom(dom: &crate::dom::Dom) -> (DomFingerprints, PreCascadeTransfers) {
    use core::hash::{Hash, Hasher};

    fn node_structure_hash(node: &NodeData, child_count: usize) -> u64 {
        use core::hash::{Hash, Hasher};

        let mut h = crate::hash::DefaultHasher::new();

        // Every field of the node the table says is compared, by its shape
        // (`NodeField::hash_shape`): its type, its handlers' events, its
        // flags, attributes, SVG data, menus, key... The style is the style
        // tier's, and what is carried travels with the transfers.
        for field in crate::layout_inputs::NodeField::ALL {
            field.hash_shape(node, &mut h);
        }

        // hierarchy shape
        child_count.hash(&mut h);

        h.finish()
    }

    fn node_style_hash(dom: &crate::dom::Dom) -> u64 {
        use core::hash::{Hash, Hasher};
        let mut h = crate::hash::DefaultHasher::new();

        hash_inline_style(&dom.root.style, &mut h);

        // Attached .with_css() sheets — subtree-scoped by construction, so
        // they belong to THIS node's style identity.
        dom.css.as_ref().len().hash(&mut h);
        for css in dom.css.as_ref() {
            for rule in css.rules.as_ref() {
                rule.path.hash(&mut h);
                for decl in rule.declarations.as_ref() {
                    decl.hash(&mut h);
                }
                // `DynamicSelector` hashes its f32 thresholds by their bits;
                // formatting each condition as text cost an allocation per
                // condition, per rule, per node, per pass.
                rule.conditions.as_slice().hash(&mut h);
                rule.priority.hash(&mut h);
            }
        }

        h.finish()
    }

    /// [`node_structure_hash`] of a text node without the characters of its text.
    fn text_shape_hash(node: &NodeData, text: &str, localizable: bool, child_count: usize) -> u64 {
        use core::hash::{Hash, Hasher};

        let mut h = crate::hash::DefaultHasher::new();
        for field in crate::layout_inputs::NodeField::ALL {
            if field == crate::layout_inputs::NodeField::NodeType {
                // A text: a rule can ask whether a node is empty (`:empty`) and the
                // translation pass replaces a localizable text, so both stay in the shape.
                0x7E87_5A9E_u32.hash(&mut h);
                text.is_empty().hash(&mut h);
                localizable.hash(&mut h);
            } else {
                field.hash_shape(node, &mut h);
            }
        }
        child_count.hash(&mut h);
        h.finish()
    }

    /// The sheets attached to `dom`'s own node, as [`node_style_hash`] folds them.
    fn node_sheets_hash(dom: &crate::dom::Dom) -> u64 {
        use core::hash::{Hash, Hasher};
        let mut h = crate::hash::DefaultHasher::new();
        dom.css.as_ref().len().hash(&mut h);
        for css in dom.css.as_ref() {
            for rule in css.rules.as_ref() {
                rule.path.hash(&mut h);
                for decl in rule.declarations.as_ref() {
                    decl.hash(&mut h);
                }
                rule.conditions.as_slice().hash(&mut h);
                rule.priority.hash(&mut h);
            }
        }
        h.finish()
    }

    fn walk(dom: &crate::dom::Dom, fp: &mut DomFingerprints, transfers: &mut PreCascadeTransfers) {
        let idx = fp.structure.len();
        fp.structure
            .push(node_structure_hash(&dom.root, dom.children.as_ref().len()));
        fp.style.push(node_style_hash(dom));
        // Filled in once the children are walked: the subtree ends where the walk stands then.
        fp.subtree_len.push(0);
        if !dom.css.as_ref().is_empty() {
            fp.sheets.push((idx, node_sheets_hash(dom)));
        }
        if let NodeType::Text(text) = dom.root.get_node_type() {
            fp.text_shapes.push((
                idx,
                text_shape_hash(
                    &dom.root,
                    text.as_ref().as_str(),
                    text.as_ref().is_localizable(),
                    dom.children.as_ref().len(),
                ),
            ));
        }

        // What the table says is carried: the values every build makes anew.
        if let Some(values) = crate::layout_inputs::NodeField::values_to_carry(&dom.root) {
            transfers.nodes.push((idx, values));
        }

        for child in dom.children.as_ref() {
            walk(child, fp, transfers);
        }
        let len = u32::try_from(fp.structure.len() - idx).unwrap_or(u32::MAX);
        if let Some(slot) = fp.subtree_len.get_mut(idx) {
            *slot = len;
        }
    }

    let mut fp = DomFingerprints {
        structure: Vec::new(),
        style: Vec::new(),
        structure_root: 0,
        style_root: 0,
        text_shapes: Vec::new(),
        subtree_len: Vec::new(),
        sheets: Vec::new(),
    };
    let mut transfers = PreCascadeTransfers::default();
    walk(dom, &mut fp, &mut transfers);

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

    (fp, transfers)
}

/// The nodes whose TEXT a rebuild changed, when texts are all it changed: the pre-order
/// indices at which `next` differs from `prev`, ascending.
///
/// `None` when the two builds are
/// alike, and when anything else differs - a node added or removed, a class, a handler's
/// event, any style, a text that came or went or became empty.
///
/// The proof is in the three tiers. Equal `style`: every node's own style and attached
/// sheets are what they were. Equal `text_shapes`: the same nodes are texts, and each is what
/// it was but for its characters. A `structure` hash that differs at a node that is no text
/// is a change this function does not answer for (`None`). What is left - a differing
/// `structure` at a text whose shape did not move - is a change of its characters. Every
/// node's child count is in its `structure` hash and in its text shape, and the nodes are in
/// pre-order, so the tree is the same tree and index `i` names the same node in both builds.
#[must_use]
pub fn texts_alone_changed(prev: &DomFingerprints, next: &DomFingerprints) -> Option<Vec<usize>> {
    if prev.structure.len() != next.structure.len()
        || prev.style != next.style
        || prev.text_shapes != next.text_shapes
    {
        return None;
    }
    let mut changed = Vec::new();
    // `text_shapes` is in pre-order, as the walk that fills it: one cursor follows it.
    let mut texts = next.text_shapes.iter().map(|(index, _)| *index).peekable();
    for (index, (old, new)) in prev.structure.iter().zip(next.structure.iter()).enumerate() {
        while texts.next_if(|text| *text < index).is_some() {}
        if old == new {
            continue;
        }
        if texts.peek() != Some(&index) {
            return None;
        }
        changed.push(index);
    }
    (!changed.is_empty()).then_some(changed)
}

/// The nodes of `dom` at the pre-order indices `indices` (ascending), numbered as
/// [`fingerprint_dom`] numbers them. An index past the last node yields nothing.
#[must_use]
pub fn nodes_at<'a>(dom: &'a crate::dom::Dom, indices: &[usize]) -> Vec<&'a NodeData> {
    fn walk<'a>(
        dom: &'a crate::dom::Dom,
        next_index: &mut usize,
        wanted: &mut core::iter::Peekable<core::slice::Iter<'_, usize>>,
        found: &mut Vec<&'a NodeData>,
    ) {
        if wanted.peek().is_none() {
            return;
        }
        let index = *next_index;
        *next_index += 1;
        if wanted.next_if(|wanted_index| **wanted_index == index).is_some() {
            found.push(&dom.root);
        }
        for child in dom.children.as_ref() {
            walk(child, next_index, wanted, found);
        }
    }

    let mut found = Vec::with_capacity(indices.len());
    let mut next_index = 0;
    let mut wanted = indices.iter().peekable();
    walk(dom, &mut next_index, &mut wanted, &mut found);
    found
}

/// Whether [`adopt_fresh_texts`] can write the texts of `fresh` at `changed` onto `retained` so
/// that the arena reads as a full rebuild would leave it.
///
/// Each of the nodes is a text in both, none of them is localizable (the translation pass, which
/// the retained arena went through and this write does not, replaces such a text), and none has a
/// handler (the reconciliation of a full rebuild tells a node's `Update` handler that its content
/// moved).
#[must_use]
pub fn fresh_texts_are_adoptable(
    retained: &[NodeData],
    fresh: &crate::dom::Dom,
    changed: &[usize],
) -> bool {
    let nodes = nodes_at(fresh, changed);
    nodes.len() == changed.len()
        && changed.iter().zip(nodes.iter()).all(|(index, new)| {
            let Some(old) = retained.get(*index) else {
                return false;
            };
            let (NodeType::Text(new_text), NodeType::Text(old_text)) =
                (new.get_node_type(), old.get_node_type())
            else {
                return false;
            };
            !new_text.as_ref().is_localizable()
                && !old_text.as_ref().is_localizable()
                && old.get_localization_key().is_none()
                && new.get_callbacks().as_ref().is_empty()
                && old.get_callbacks().as_ref().is_empty()
        })
}

/// Writes the texts a rebuild changed onto the arena the window keeps: the node at each
/// index of `changed` gets the text of the node `fresh` has there.
///
/// The caller has asked
/// [`fresh_texts_are_adoptable`]; a node that is no text in either is left as it is.
pub fn adopt_fresh_texts(retained: &mut [NodeData], fresh: &crate::dom::Dom, changed: &[usize]) {
    for (index, new) in changed.iter().zip(nodes_at(fresh, changed)) {
        let Some(old) = retained.get_mut(*index) else {
            continue;
        };
        if matches!(
            (new.get_node_type(), old.get_node_type()),
            (NodeType::Text(_), NodeType::Text(_))
        ) {
            old.set_node_type(new.get_node_type().clone());
        }
    }
}

/// The nodes a rebuild changed when it kept the TREE: the pre-order indices at which `next`
/// differs from `prev` in what the node is (`structure`) or in its own style (`style`),
/// ascending.
///
/// `None` when the two builds are alike, and when the rebuild did more than change
/// nodes where they stand: a node came, went or moved (`subtree_len` differs), or a sheet
/// attached to a node changed, came or went (`sheets` differs).
///
/// Equal `subtree_len`: the same tree, so index `i` names the node at the same place in both
/// builds. Equal `sheets`: every attached sheet is what it was and hangs where it hung, and
/// with the same tree its scope (the subtree of its node) covers the same nodes. What is left
/// is per node: its type and text, its handlers' events, its flags, attributes, ids and
/// classes, its key (`structure`), and its inline style (`style`). A node this does not name
/// is, field for field of the table the fingerprints compare
/// ([`crate::layout_inputs::NodeField`]), the node it was.
///
/// Which of the named nodes can be written onto the retained arena is
/// [`changed_nodes_are_adoptable`]'s to say.
#[must_use]
pub fn same_tree_changes(prev: &DomFingerprints, next: &DomFingerprints) -> Option<Vec<usize>> {
    let n = next.structure.len();
    if prev.structure.len() != n
        || prev.style.len() != n
        || next.style.len() != n
        || prev.subtree_len.len() != n
        || next.subtree_len.len() != n
        || prev.subtree_len != next.subtree_len
        || prev.sheets != next.sheets
    {
        return None;
    }
    let changed: Vec<usize> = (0..n)
        .filter(|i| prev.structure[*i] != next.structure[*i] || prev.style[*i] != next.style[*i])
        .collect();
    (!changed.is_empty()).then_some(changed)
}

/// Whether [`adopt_changed_nodes`] can write the nodes of `fresh` at `changed` onto `retained` so
/// that the arena reads as a full rebuild would leave it.
///
/// The node at each index being THE SAME NODE as before (its focus, scroll, hover, text-edit state
/// and dataset stay with it).
///
/// A full rebuild pairs the nodes with `reconcile_dom`. In the same tree, a node whose type,
/// terminal key (`with_key`, or the id where `identity.id_is_key`) and - where
/// `identity.class_change_remounts` - classes are what they were has the reconciliation key
/// it had (the key is those three, the node's place and its parent's key), so the positional
/// pass pairs it with the node at its place. What a full rebuild does for a pair beyond
/// styling it and laying it out keeps this off, node by node:
/// - an `Updated` handler on the node: the reconciliation tells it that its content moved;
/// - an image: a rebuilt image node takes the live frame of the node before it
///   (`transfer_states`), and counts as re-initialised;
/// - an icon, a virtual view, a localizable text or a text that was translated: the passes
///   between the app's callback and the cascade resolve, host or translate them;
/// - a node whose Fluent arguments changed: the translated text below it (a key whose
///   arguments hang on its paragraph) was formatted with the old ones, and only the translation
///   pass of a full rebuild formats it again - the text is no changed node itself (its key is
///   what it was), so nothing else would redo it;
/// - a presentational attribute (a `Custom` attribute on the fresh node, a hint already in
///   the retained node's style): `StyledDom`'s creation turns those into declarations;
/// - a style that makes generated content (an inline list item, a footnote): the creation
///   adds nodes for it;
/// - other handlers' events, a dataset that came or went, a geolocation probe: the tail of
///   the shell's rebuild reads them off every node of the new DOM (the lists of nodes with
///   window handlers and datasets, the sensor and gamepad listeners, the permission diff).
#[must_use]
pub fn changed_nodes_are_adoptable(
    retained: &[NodeData],
    fresh: &crate::dom::Dom,
    changed: &[usize],
    identity: NodeIdentityConfig,
) -> bool {
    /// A node the passes around the cascade treat specially (see above).
    fn special(node: &NodeData) -> bool {
        match node.get_node_type() {
            NodeType::Image(_)
            | NodeType::Icon(_)
            | NodeType::VirtualView
            | NodeType::GeolocationProbe(_) => true,
            NodeType::Text(text) => text.as_ref().is_localizable(),
            _ => false,
        }
    }
    /// A presentational hint among the node's own rules: any rule below the inline priority.
    fn has_hint(node: &NodeData) -> bool {
        node.style
            .rules
            .as_ref()
            .iter()
            .any(|rule| rule.priority < azul_css::css::rule_priority::INLINE)
    }

    let nodes = nodes_at(fresh, changed);
    nodes.len() == changed.len()
        && changed.iter().zip(nodes.iter()).all(|(index, new)| {
            let Some(old) = retained.get(*index) else {
                return false;
            };
            core::mem::discriminant(old.get_node_type())
                == core::mem::discriminant(new.get_node_type())
                && identity.terminal_key(old) == identity.terminal_key(new)
                && !identity.classes_differ(old, new)
                && !has_update_callback(new)
                && !special(old)
                && !special(new)
                && old.get_localization_key().is_none()
                && !has_hint(old)
                && !has_hint(new)
                && !crate::generated_content::node_generates_nodes(new)
                && crate::layout_inputs::NodeField::Callbacks.reads_alike(old, new)
                && crate::layout_inputs::NodeField::FluentArgs.reads_alike(old, new)
                && old.get_dataset().is_some() == new.get_dataset().is_some()
                && !new
                    .attributes()
                    .as_ref()
                    .iter()
                    .any(|attr| matches!(attr, AttributeType::Custom(_)))
        })
}

/// Writes the nodes a rebuild changed onto the arena the window keeps, and answers the nodes
/// that stood there: `(index, the node before)`, ascending.
///
/// The caller has asked
/// [`changed_nodes_are_adoptable`], and has carried the fresh build's payloads onto the arena
/// ([`adopt_fresh_payloads`]) BEFORE this.
///
/// The node written at an index is the fresh build's node - every field the table compares
/// ([`crate::layout_inputs::NodeField`]): its type and text, flags, attributes, style, key -
/// with, for every field the table carries and the fresh node holds a value of (its handlers,
/// its dataset, a menu, its marker...), the value the retained node holds after the payloads
/// were carried: the fresh one, merged with the state of the build before where the node has
/// a merge callback. A carried field the fresh node holds nothing of stays empty, as it is on
/// the node a full rebuild makes.
pub fn adopt_changed_nodes(
    retained: &mut [NodeData],
    fresh: &crate::dom::Dom,
    changed: &[usize],
) -> Vec<(usize, NodeData)> {
    use crate::layout_inputs::NodeField;

    let mut replaced = Vec::with_capacity(changed.len());
    for (index, new) in changed.iter().zip(nodes_at(fresh, changed)) {
        let Some(held) = retained.get_mut(*index) else {
            continue;
        };
        let mut node = new.clone();
        for field in NodeField::ALL {
            if field.retained().carries() && field.holds_value(new) {
                field.carry(held, &mut node);
            }
        }
        replaced.push((*index, core::mem::replace(held, node)));
    }
    replaced
}

#[cfg(test)]
#[path = "diff_test.rs"]
mod diff_test;
