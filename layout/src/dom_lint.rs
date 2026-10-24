//! Post-layout developer warnings for misused raw text nodes.
//!
//! Browsers wrap a raw text run in an ANONYMOUS BLOCK whenever it needs one;
//! azul does not. A bare `NodeType::Text` therefore has no box of its own —
//! no rect, no clip, no layout constraints — and every box-model CSS
//! property, every id or class a stylesheet could reach it by, and every
//! pointer or focus callback, `tab_index` or `dataset` attached to one is
//! silently inert. (Lifecycle and window listeners are not: they are
//! delivered by node identity, not through a box - see
//! `has_box_dependent_callback`.) That silence has shipped real bugs (text
//! escaping its widget, click targets that never fire), which is why the raw
//! constructor is named `create_text_do_not_use_without_block_level_wrapper`
//! and why this pass exists: after layout, every text node in a shape azul
//! cannot honor is reported to the developer, once per unique finding.
//!
//! The checks are structural (DOM + computed display), deliberately not
//! geometric: they fire deterministically on the first layout of a DOM,
//! before any symptom is visible on screen.
//!
//! This runs after EVERY layout pass, in release, so it is built to cost
//! nothing on a clean DOM: the scan allocates only once a finding exists,
//! message text (two allocations for the snippet alone) is rendered only for
//! a warning that is actually printed, and the per-parent questions ("is
//! there a block-level child?", "how many items?") are answered once per
//! parent instead of once per text child.

use std::{
    collections::BTreeSet,
    hash::{Hash, Hasher},
    mem::discriminant,
    sync::Mutex,
};

use azul_core::{
    dom::{AttributeType, NodeType},
    events::EventFilter,
    id::NodeId,
    styled_dom::{NodeHierarchyItem, StyledDom},
};
use azul_css::props::layout::LayoutDisplay;

use crate::solver3::getters::{get_display_property, MultiValue};

/// Distinct findings kept alive process-wide. A DOM that is wrong in a
/// hundred places has one bug, not a hundred; past this the developer is
/// told the tap was closed rather than having the log drowned.
const MAX_DISTINCT_WARNINGS: usize = 32;

/// Findings already reported, keyed by [`dedup_key`], process-wide. Layouts
/// re-run constantly (every DOM refresh); a warning that repeats 60 times a
/// second is a warning nobody reads.
struct Emitted {
    keys: BTreeSet<u64>,
    capped: bool,
}

static EMITTED: Mutex<Emitted> = Mutex::new(Emitted {
    keys: BTreeSet::new(),
    capped: false,
});

/// The suppression tag for this lint, honored from the `AZ_SUPPRESS`
/// environment variable (comma-separated list; the common misspelling
/// `AZ_SUPRESS` is accepted too). Every emitted warning names it.
pub const SUPPRESS_TAG: &str = "bare_text";

/// `AZ_SUPPRESS=bare_text` (checked once): the developer has read the
/// warnings and wants them off — e.g. a codebase that deliberately renders
/// raw text and accepts the differences from browser behavior.
fn is_suppressed() -> bool {
    static SUPPRESSED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SUPPRESSED.get_or_init(|| {
        let v = std::env::var("AZ_SUPPRESS")
            .or_else(|_| std::env::var("AZ_SUPRESS"))
            .unwrap_or_default();
        v.split(',')
            .any(|t| t.trim().eq_ignore_ascii_case(SUPPRESS_TAG))
    })
}

/// State on a text node that a text node cannot carry, as a bitset so the
/// scan can record a finding without allocating.
const INERT_CSS: u8 = 1 << 0;
const INERT_IDS_CLASSES: u8 = 1 << 1;
const INERT_CALLBACKS: u8 = 1 << 2;
const INERT_TAB_INDEX: u8 = 1 << 3;
const INERT_DATASET: u8 = 1 << 4;
const INERT_CHILDREN: u8 = 1 << 5;

/// One finding, kept as a tag until a message is known to be needed.
#[derive(Clone, Copy)]
enum Finding {
    /// W1 — box-less node carrying state (bitset of the `INERT_*` flags).
    Inert(u8),
    /// W2 — the text run is the root: nothing owns its box.
    NoParent,
    /// W2 — text nested directly inside text.
    TextParent,
    /// W2 — one of several items in a flex/grid container.
    FlexItem {
        display: LayoutDisplay,
        child_count: usize,
    },
    /// W2 — mixed inline/block content: no line box of its own.
    BlockSibling,
}

impl Finding {
    /// Orders findings on the same node the way the checks are written
    /// (state first, placement second) — the scan visits a text node and its
    /// parent in different iterations, so the sort needs the tie-break.
    const fn rank(self) -> u8 {
        match self {
            Self::Inert(_) => 0,
            Self::NoParent => 1,
            Self::TextParent => 2,
            Self::FlexItem { .. } => 3,
            Self::BlockSibling => 4,
        }
    }
}

const fn is_text(node_type: &NodeType) -> bool {
    matches!(node_type, NodeType::Text(_))
}

fn has_ids_or_classes(data: &azul_core::dom::NodeData) -> bool {
    data.attributes()
        .as_ref()
        .iter()
        .any(|a| matches!(a, AttributeType::Id(_) | AttributeType::Class(_)))
}

/// Does the node carry a callback that can only be reached THROUGH ITS BOX?
///
/// Pointer events find their target by hit-testing a rect, and focus events
/// need a node that can hold focus - a text node has neither, so a `Hover` or
/// `Focus` listener on one can never fire. `Window` listeners fire on every
/// node for a window-level event, and `Component` (lifecycle) listeners are
/// delivered by NODE IDENTITY from the reconcile diff - `AfterMount` on a text
/// node fires like on any other node (`dll/tests/headless_lifecycle` relies on
/// it). Calling those inert would be a lie, and the lie has a cost: it once
/// sent the diagnosis of a dead lifecycle pipeline chasing the test's DOM.
fn has_box_dependent_callback(data: &azul_core::dom::NodeData) -> bool {
    data.get_callbacks()
        .as_ref()
        .iter()
        .any(|cb| matches!(cb.event, EventFilter::Hover(_) | EventFilter::Focus(_)))
}

/// Can the USER trigger one of this node's callbacks, i.e. is it a control?
///
/// The same split as [`has_box_dependent_callback`]: pointer (`Hover`) and
/// keyboard (`Focus`) listeners are aimed at the node by the user. Lifecycle
/// (`Component`: `AfterMount`, `NodeResized`), `Window`, `Application` and
/// `External` listeners are delivered without anyone aiming at the node, so a
/// node that has only those is not a control and owes nobody a name - the
/// video widget's root (two lifecycle listeners around a `VirtualView`) was
/// reported as an unnamed icon-only control.
fn is_user_interactive(data: &azul_core::dom::NodeData) -> bool {
    has_box_dependent_callback(data)
}

/// Does the element type carry a role of its own?
///
/// A declared role of `Unknown` means "not specified", and both accessibility
/// trees then fall back to the element's own role
/// (`managers::a11y_snapshot::node_type_to_role` and the accesskit mapping in
/// `managers::a11y`): a named `<button>` IS a button there. Only the elements
/// whose fallback is a real control or widget role are listed.
const fn element_implies_a_role(node_type: &NodeType) -> bool {
    matches!(
        node_type,
        NodeType::Button
            | NodeType::A
            | NodeType::Input
            | NodeType::TextArea
            | NodeType::Select
            | NodeType::SelectOption
            | NodeType::MenuItem
            | NodeType::Progress
            | NodeType::Meter
    )
}

fn display_of(styled_dom: &StyledDom, node_id: NodeId) -> LayoutDisplay {
    match get_display_property(styled_dom, Some(node_id)) {
        MultiValue::Exact(d) => d,
        _ => LayoutDisplay::Block,
    }
}

/// Whether `node_id` is absolutely or fixed positioned: out of flow, it
/// takes no place in its parent's lines (CSS Position 3 §3).
fn is_out_of_flow(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    use azul_css::props::layout::LayoutPosition;
    matches!(
        crate::solver3::positioning::get_position_type(styled_dom, Some(node_id)),
        LayoutPosition::Absolute | LayoutPosition::Fixed
    )
}

const fn is_flex_or_grid(display: LayoutDisplay) -> bool {
    matches!(
        display,
        LayoutDisplay::Flex
            | LayoutDisplay::InlineFlex
            | LayoutDisplay::Grid
            | LayoutDisplay::InlineGrid
    )
}

const fn is_block_level(display: LayoutDisplay) -> bool {
    !matches!(
        display,
        LayoutDisplay::Inline
            | LayoutDisplay::InlineBlock
            | LayoutDisplay::InlineBlockListItem
            | LayoutDisplay::InlineFlex
            | LayoutDisplay::InlineGrid
            | LayoutDisplay::InlineTable
            | LayoutDisplay::None
    )
}

/// Walk `styled_dom` and tag every text node used in a shape azul cannot
/// honor. Allocation-free while nothing is wrong.
fn collect_findings(styled_dom: &StyledDom) -> Vec<(usize, Finding)> {
    let node_data = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut found: Vec<(usize, Finding)> = Vec::new();

    for idx in 0..node_data.len() {
        let node_id = NodeId::new(idx);
        let data = &node_data[node_id];
        let Some(h) = hierarchy.get(node_id) else {
            continue;
        };

        if is_text(data.get_node_type()) {
            // W1 — state on a box-less node: every one of these is inert on
            // a text node, because only the wrapping block box carries a
            // rect. An id or a class counts: it is how a stylesheet reaches
            // the node, and every rule it selects computes onto a node that
            // never gets one.
            let mut inert = 0u8;
            if !data.get_style().rules.as_ref().is_empty() {
                inert |= INERT_CSS;
            }
            if has_ids_or_classes(data) {
                inert |= INERT_IDS_CLASSES;
            }
            if has_box_dependent_callback(data) {
                inert |= INERT_CALLBACKS;
            }
            if data.get_tab_index().is_some() {
                inert |= INERT_TAB_INDEX;
            }
            if data.get_dataset().is_some() {
                inert |= INERT_DATASET;
            }
            if h.first_child_id(node_id).is_some() {
                inert |= INERT_CHILDREN;
            }
            if inert != 0 {
                found.push((idx, Finding::Inert(inert)));
            }

            // W2 — no containing block the text can live in. Browsers would
            // generate an anonymous block here; azul does not. The remaining
            // W2 shapes depend only on the PARENT, so they are decided once
            // in the parent's own iteration below.
            match h.parent_id() {
                None => found.push((idx, Finding::NoParent)),
                Some(parent_id) if is_text(node_data[parent_id].get_node_type()) => {
                    found.push((idx, Finding::TextParent));
                }
                Some(_) => {}
            }
            continue;
        }

        // Parent-major placement checks. Answering "does this parent have a
        // block-level child / how many items does it have?" once per parent
        // keeps the pass linear; asking it per text child made a wide parent
        // with several text children quadratic.
        let mut child_count = 0usize;
        let mut has_text_child = false;
        let mut child = h.first_child_id(node_id);
        while let Some(cc) = child {
            child_count += 1;
            has_text_child |= is_text(node_data[cc].get_node_type());
            child = hierarchy
                .get(cc)
                .and_then(NodeHierarchyItem::next_sibling_id);
        }
        if !has_text_child {
            // The overwhelmingly common case: no display lookup, no second
            // sweep, nothing allocated.
            continue;
        }

        let parent_display = display_of(styled_dom, node_id);
        let finding = if is_flex_or_grid(parent_display) {
            // A text leaf as the SOLE child of a flex/grid box is the
            // sanctioned wrapper pattern (the parent is the box that carries
            // the styling — badge, the converted labels). The hazard is text
            // as ONE OF SEVERAL items: it competes in item layout with no box
            // of its own.
            if child_count <= 1 {
                continue;
            }
            Finding::FlexItem {
                display: parent_display,
                child_count,
            }
        } else {
            // Mixed inline + block content under one parent: the text has no
            // dedicated line box of its own next to block siblings. An
            // absolutely / fixed positioned sibling is out of flow and
            // splits no line, so it does not count; a float still does
            // (azul places no float inside an inline formatting context).
            let mut has_block_child = false;
            let mut sibling = h.first_child_id(node_id);
            while let Some(sib) = sibling {
                if !is_text(node_data[sib].get_node_type())
                    && is_block_level(display_of(styled_dom, sib))
                    && !is_out_of_flow(styled_dom, sib)
                {
                    has_block_child = true;
                    break;
                }
                sibling = hierarchy
                    .get(sib)
                    .and_then(NodeHierarchyItem::next_sibling_id);
            }
            if !has_block_child {
                continue;
            }
            Finding::BlockSibling
        };

        let mut child = h.first_child_id(node_id);
        while let Some(cc) = child {
            if is_text(node_data[cc].get_node_type()) {
                found.push((cc.index(), finding));
            }
            child = hierarchy
                .get(cc)
                .and_then(NodeHierarchyItem::next_sibling_id);
        }
    }

    // Node order, so a finding reads in the order the DOM was written.
    found.sort_by_key(|(idx, finding)| (*idx, finding.rank()));
    found
}

/// Walk `styled_dom` and return one message per text node that is used in a
/// shape azul cannot honor. Pure — the caller decides how to report.
#[must_use]
pub fn collect_text_placement_warnings(styled_dom: &StyledDom) -> Vec<String> {
    let found = collect_findings(styled_dom);
    if found.is_empty() {
        return Vec::new();
    }
    let node_data = styled_dom.node_data.as_container();
    found
        .into_iter()
        .map(|(idx, finding)| render(&node_data[NodeId::new(idx)], idx, finding))
        .collect()
}

/// Build the developer-facing text. Only called for a finding that is about
/// to be printed — the snippet alone costs two allocations.
fn render(data: &azul_core::dom::NodeData, idx: usize, finding: Finding) -> String {
    let snippet = match data.get_node_type() {
        NodeType::Text(t) => snippet_of(t.as_str()),
        _ => String::new(),
    };
    match finding {
        Finding::Inert(flags) => {
            let mut inert = Vec::new();
            if flags & INERT_CSS != 0 {
                inert.push("css properties");
            }
            if flags & INERT_IDS_CLASSES != 0 {
                inert.push("ids/classes");
            }
            if flags & INERT_CALLBACKS != 0 {
                inert.push("pointer/focus callbacks");
            }
            if flags & INERT_TAB_INDEX != 0 {
                inert.push("a tab_index");
            }
            if flags & INERT_DATASET != 0 {
                inert.push("a dataset");
            }
            if flags & INERT_CHILDREN != 0 {
                inert.push("element children");
            }
            format!(
                "text node {idx} ({snippet}) carries {} — INERT: a text node has no box. Move \
                 them onto a block wrapper (create_p_with_text / create_div_with_text) instead of \
                 the raw text node.",
                inert.join(" + "),
            )
        }
        Finding::NoParent => format!(
            "text node {idx} ({snippet}) has no parent — a raw text run needs a block-level \
             container (p / div / ...) to own its box.",
        ),
        Finding::TextParent => format!(
            "text node {idx} ({snippet}) is the child of another text node — wrap both in a \
             block-level container (p / div / ...).",
        ),
        Finding::FlexItem {
            display,
            child_count,
        } => format!(
            "text node {idx} ({snippet}) is one of {child_count} items in a {display:?} container \
             — a raw text run competes in flex/grid layout with no box of its own (browsers \
             auto-wrap it in an anonymous block; azul does not). Wrap it: create_p_with_text / \
             create_div_with_text.",
        ),
        Finding::BlockSibling => format!(
            "text node {idx} ({snippet}) sits NEXT TO block-level siblings — browsers would wrap \
             it in an anonymous block, azul does not, so it has no line box of its own. Wrap it: \
             create_p_with_text / create_div_with_text.",
        ),
    }
}

/// Identify the PROBLEM, not the node.
///
/// Node indices shift on every structural edit — i.e. on every keystroke in
/// a live document — so an index-derived key re-hashes each frame: the same
/// finding re-prints forever AND the dedup set grows without bound. The kind
/// of finding plus the styling identity of the text node and its parent is
/// stable across edits, and it is what the developer actually fixes: one
/// construction site.
fn dedup_key(styled_dom: &StyledDom, node_id: NodeId, finding: Finding) -> u64 {
    identity_key(styled_dom, node_id, |hasher| {
        finding.rank().hash(hasher);
        match finding {
            Finding::Inert(flags) => flags.hash(hasher),
            // NOT the child_count: adding an item to the container is not a
            // new bug.
            Finding::FlexItem { display, .. } => display.hash(hasher),
            _ => {}
        }
    })
}

/// The key of a finding on `node_id`: what `kind` hashes (which finding),
/// then the styling identity of the node and of its parent - THE problem
/// identity every lint dedupes by (see [`dedup_key`]).
fn identity_key(
    styled_dom: &StyledDom,
    node_id: NodeId,
    kind: impl FnOnce(&mut std::collections::hash_map::DefaultHasher),
) -> u64 {
    let node_data = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    kind(&mut hasher);
    hash_identity(&node_data[node_id], &mut hasher);
    match hierarchy
        .get(node_id)
        .and_then(NodeHierarchyItem::parent_id)
    {
        None => 0u8.hash(&mut hasher),
        Some(parent_id) => {
            1u8.hash(&mut hasher);
            hash_identity(&node_data[parent_id], &mut hasher);
        }
    }
    hasher.finish()
}

/// Print a finding of the lint `lint` (its suppression tag) on `node_id` ONCE
/// per process per problem ([`identity_key`]: the lint, what the node is and
/// how it is selected, and its parent's), through
/// `azul_core::diagnostics::emit_once`: the lints run whenever a new DOM is
/// laid out, and an app renders the same shape again and again. The message
/// is built only when it is printed. Returns whether it was.
fn report_once(
    styled_dom: &StyledDom,
    node_id: NodeId,
    lint: &str,
    message: impl FnOnce() -> String,
) -> bool {
    let key = identity_key(styled_dom, node_id, |hasher| lint.hash(hasher));
    azul_core::diagnostics::emit_once(key, message)
}

/// Print the nodes of one lint pass that share a problem `kind` of the lint
/// `lint` as ONE line, once per window (`scope`: the window's, 0 outside
/// one) through `azul_core::diagnostics::emit_once`. Twelve navigation rows
/// built by one helper are one fix at one construction site, not twelve
/// warnings, and a window that renders new screens with the same problem has
/// nothing new to say. `message` gets the count and the first nodes
/// ([`node_list`]) and is built only when it is printed. Returns whether it
/// was.
fn report_kind_once(
    styled_dom: &StyledDom,
    scope: u64,
    lint: &str,
    kind: u8,
    nodes: &[NodeId],
    message: impl FnOnce(&str) -> String,
) -> bool {
    if nodes.is_empty() {
        return false;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    scope.hash(&mut hasher);
    lint.hash(&mut hasher);
    kind.hash(&mut hasher);
    azul_core::diagnostics::emit_once(hasher.finish(), || {
        message(&node_list(styled_dom, nodes))
    })
}

/// "3 nodes: node 12 (#nav-mail "Mail"), node 14 (...) and 1 more" - how
/// many nodes have a finding and the first of them, each with the role it
/// declares when it declares one.
fn node_list(styled_dom: &StyledDom, nodes: &[NodeId]) -> String {
    const SHOWN: usize = 4;
    let data = styled_dom.node_data.as_container();
    let shown: Vec<String> = nodes
        .iter()
        .take(SHOWN)
        .map(|&id| {
            let who = describe_node(styled_dom, id);
            match data.get(id).and_then(|n| n.accessibility.as_ref()).map(|i| i.role) {
                Some(role) if !matches!(role, azul_core::a11y::AccessibilityRole::Unknown) => {
                    format!("{who} as {role:?}")
                }
                _ => who,
            }
        })
        .collect();
    let count = if nodes.len() == 1 {
        "1 node".to_string()
    } else {
        format!("{} nodes", nodes.len())
    };
    let more = nodes.len().saturating_sub(SHOWN);
    if more > 0 {
        format!("{count}: {} and {more} more", shown.join(", "))
    } else {
        format!("{count}: {}", shown.join(", "))
    }
}

/// The part of a node that survives a DOM rebuild: what it is and how it is
/// selected. Deliberately not its text, which may be per-frame data.
fn hash_identity(data: &azul_core::dom::NodeData, hasher: &mut impl Hasher) {
    discriminant(data.get_node_type()).hash(hasher);
    for attr in data.attributes().as_ref() {
        match attr {
            AttributeType::Id(s) => {
                0u8.hash(hasher);
                s.as_str().hash(hasher);
            }
            AttributeType::Class(s) => {
                1u8.hash(hasher);
                s.as_str().hash(hasher);
            }
            _ => {}
        }
    }
}

/// Report every finding from [`collect_text_placement_warnings`] to stderr,
/// once per unique finding per process. Call after layout.
pub fn warn_text_without_block_container(styled_dom: &StyledDom) {
    if is_suppressed() {
        return;
    }
    let found = collect_findings(styled_dom);
    if found.is_empty() {
        return;
    }
    let Ok(mut emitted) = EMITTED.lock() else {
        return;
    };
    if emitted.capped {
        return;
    }
    let node_data = styled_dom.node_data.as_container();
    for (idx, finding) in found {
        let node_id = NodeId::new(idx);
        if !emitted.keys.insert(dedup_key(styled_dom, node_id, finding)) {
            continue;
        }
        if emitted.keys.len() > MAX_DISTINCT_WARNINGS {
            emitted.capped = true;
            eprintln!(
                "[azul][text-without-block] {MAX_DISTINCT_WARNINGS} distinct findings reported — \
                 further warnings suppressed (suppress the whole lint with \
                 AZ_SUPPRESS={SUPPRESS_TAG})"
            );
            return;
        }
        eprintln!(
            "[azul][text-without-block] WARNING: {} (suppress with AZ_SUPPRESS={SUPPRESS_TAG})",
            render(&node_data[node_id], idx, finding),
        );
    }
}

fn snippet_of(text: &str) -> String {
    let mut s: String = text.chars().take(24).collect();
    if text.chars().count() > 24 {
        s.push_str("...");
    }
    format!("{s:?}")
}

#[cfg(test)]
mod autotest_generated {
    use azul_core::{
        dom::{Dom, IdOrClass, IdOrClassVec, TabIndex},
        events::{ComponentEventFilter, EventFilter, HoverEventFilter},
        refany::RefAny,
        styled_dom::StyledDom,
    };
    use azul_css::css::Css;

    use super::collect_text_placement_warnings;

    fn styled(mut dom: Dom, css: &str) -> StyledDom {
        let css = if css.is_empty() {
            Css::empty()
        } else {
            Css::from_string(css.into())
        };
        StyledDom::create(&mut dom, css)
    }

    fn raw_text(s: &str) -> Dom {
        Dom::create_text_do_not_use_without_block_level_wrapper(s)
    }

    #[test]
    fn a_correctly_wrapped_text_produces_no_warning() {
        let sd = styled(
            Dom::create_body().with_child(Dom::create_p_with_text("hello")),
            "",
        );
        assert_eq!(collect_text_placement_warnings(&sd), Vec::<String>::new());
    }

    #[test]
    fn text_inside_an_inline_span_inside_a_block_is_fine() {
        let sd = styled(
            Dom::create_body()
                .with_child(Dom::create_p().with_child(Dom::create_span_with_text("hi"))),
            "",
        );
        assert_eq!(collect_text_placement_warnings(&sd), Vec::<String>::new());
    }

    #[test]
    fn state_on_a_text_node_is_reported_as_inert() {
        let text = raw_text("styled").with_tab_index(TabIndex::Auto);
        let sd = styled(
            Dom::create_body().with_child(Dom::create_p().with_child(text)),
            "",
        );
        let w = collect_text_placement_warnings(&sd);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("INERT"), "{w:?}");
        assert!(w[0].contains("tab_index"), "{w:?}");
    }

    #[test]
    fn a_pointer_callback_on_a_text_node_is_reported_as_inert() {
        let text = raw_text("clickable").with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            RefAny::new(()),
            0usize,
        );
        let sd = styled(
            Dom::create_body().with_child(Dom::create_p().with_child(text)),
            "",
        );
        let w = collect_text_placement_warnings(&sd);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("pointer/focus callbacks"), "{w:?}");
    }

    #[test]
    fn a_lifecycle_callback_on_a_text_node_is_not_inert() {
        // Delivered by node identity from the reconcile diff, not by hit-test:
        // it fires, so the lint must not call it dead.
        let text = raw_text("mounted").with_callback(
            EventFilter::Component(ComponentEventFilter::AfterMount),
            RefAny::new(()),
            0usize,
        );
        let sd = styled(
            Dom::create_body().with_child(Dom::create_p().with_child(text)),
            "",
        );
        let w = collect_text_placement_warnings(&sd);
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn ids_and_classes_on_a_text_node_are_reported_as_inert() {
        // The shipped menu_renderer defect: the checkmark's three classes
        // ended up on the text node instead of the icon <div> that boxes it.
        // The text is that div's only child — the shape every placement check
        // calls sanctioned — so the classes are the only thing left to report.
        let icon = raw_text("✓").with_ids_and_classes(IdOrClassVec::from_vec(vec![
            IdOrClass::Class("menu-item-icon".into()),
            IdOrClass::Class("menu-item-checkbox".into()),
            IdOrClass::Class("menu-item-checkbox-checked".into()),
        ]));
        let sd = styled(
            Dom::create_body().with_child(Dom::create_div().with_child(icon)),
            "",
        );
        let w = collect_text_placement_warnings(&sd);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("INERT"), "{w:?}");
        assert!(w[0].contains("ids/classes"), "{w:?}");
    }

    #[test]
    fn a_sole_text_leaf_in_a_flex_wrapper_is_the_sanctioned_pattern() {
        // badge / the converted labels: the flex box IS the wrapper.
        let sd = styled(
            Dom::create_body().with_child(Dom::create_div().with_child(raw_text("flexed"))),
            "div { display: flex; }",
        );
        assert_eq!(collect_text_placement_warnings(&sd), Vec::<String>::new());
    }

    #[test]
    fn text_competing_with_other_flex_items_is_reported() {
        // The tree_view/radio_group shape: a raw label beside element items.
        let sd = styled(
            Dom::create_body().with_child(
                Dom::create_div()
                    .with_child(Dom::create_div())
                    .with_child(raw_text("flexed")),
            ),
            "body > div { display: flex; }",
        );
        let w = collect_text_placement_warnings(&sd);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("competes in flex/grid"), "{w:?}");
    }

    #[test]
    fn text_next_to_a_block_sibling_is_reported() {
        // The audited frame.rs shape: a title wedged between two divs.
        let sd = styled(
            Dom::create_body().with_child(
                Dom::create_div()
                    .with_child(Dom::create_div())
                    .with_child(raw_text("title"))
                    .with_child(Dom::create_div()),
            ),
            "",
        );
        let w = collect_text_placement_warnings(&sd);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("block-level siblings"), "{w:?}");
    }

    #[test]
    fn text_next_to_an_absolutely_positioned_sibling_is_not_reported() {
        // An out-of-flow box splits no line: the text keeps its line box
        // beside a mark pinned over it (a check item's box, a badge).
        for position in ["absolute", "fixed"] {
            let sd = styled(
                Dom::create_body().with_child(
                    Dom::create_div()
                        .with_child(Dom::create_div())
                        .with_child(raw_text("title")),
                ),
                &format!("body > div > div {{ position: {position}; }}"),
            );
            assert_eq!(
                collect_text_placement_warnings(&sd),
                Vec::<String>::new(),
                "position: {position}"
            );
        }
        // A float still counts: azul places no float inside an inline
        // formatting context yet.
        let sd = styled(
            Dom::create_body().with_child(
                Dom::create_div()
                    .with_child(Dom::create_div())
                    .with_child(raw_text("title")),
            ),
            "body > div > div { float: left; }",
        );
        let w = collect_text_placement_warnings(&sd);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("block-level siblings"), "{w:?}");
    }

    #[test]
    fn several_text_children_of_one_parent_are_all_reported() {
        // The per-parent memo must not swallow the sibling text runs it was
        // computed for.
        let sd = styled(
            Dom::create_body().with_child(
                Dom::create_div()
                    .with_child(raw_text("a"))
                    .with_child(Dom::create_div())
                    .with_child(raw_text("b")),
            ),
            "",
        );
        let w = collect_text_placement_warnings(&sd);
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(w[0].contains("\"a\""), "{w:?}");
        assert!(w[1].contains("\"b\""), "{w:?}");
    }

    #[test]
    fn findings_are_reported_in_node_order() {
        // The two text runs are found via DIFFERENT parents (one nested, one
        // directly under body), so the scan reaches them out of order and the
        // sort has to put them back.
        let sd = styled(
            Dom::create_body()
                .with_child(
                    Dom::create_div()
                        .with_child(Dom::create_div())
                        .with_child(raw_text("deep")),
                )
                .with_child(raw_text("shallow"))
                .with_child(Dom::create_div()),
            "",
        );
        let w = collect_text_placement_warnings(&sd);
        assert_eq!(w.len(), 2, "{w:?}");
        let indices: Vec<usize> = w
            .iter()
            .map(|m| {
                m.split_whitespace()
                    .nth(2)
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_else(|| panic!("no node index in {m:?}"))
            })
            .collect();
        assert!(indices[0] < indices[1], "{indices:?} / {w:?}");
    }

    #[test]
    fn every_widget_dom_is_warning_free() {
        // The runtime twin of the widgets' label-convention test: none of the
        // shipped widgets may trip the developer warning.
        for (name, dom) in crate::widgets::all_widget_doms_for_lint() {
            let sd = styled(Dom::create_body().with_child(dom), "");
            let w = collect_text_placement_warnings(&sd);
            assert_eq!(
                w,
                Vec::<String>::new(),
                "widget {name} trips the text lint: {w:?}"
            );
        }
    }
}

// ============================================================================
// Suppression, once, for every lint
// ============================================================================

/// Is `tag` suppressed via `AZ_SUPPRESS`?
///
/// Comma-separated; the common misspelling `AZ_SUPRESS` is accepted too.
/// `AZ_SUPPRESS=all` turns every engine lint off in one move — a shipped app
/// that has read its warnings and does not want them in a customer's console
/// should not have to enumerate them, and a new lint must not suddenly start
/// talking in that app's logs.
///
/// Read once per tag and cached: these run inside the layout pass.
pub fn lint_suppressed(tag: &str) -> bool {
    use std::{
        collections::BTreeMap,
        sync::{Mutex, OnceLock},
    };
    static CACHE: OnceLock<Mutex<BTreeMap<String, bool>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    if let Ok(c) = cache.lock() {
        if let Some(hit) = c.get(tag) {
            return *hit;
        }
    }
    let v = std::env::var("AZ_SUPPRESS")
        .or_else(|_| std::env::var("AZ_SUPRESS"))
        .unwrap_or_default();
    let hit = v.split(',').any(|t| {
        let t = t.trim();
        t.eq_ignore_ascii_case(tag) || t.eq_ignore_ascii_case("all")
    });
    if let Ok(mut c) = cache.lock() {
        c.insert(tag.to_string(), hit);
    }
    hit
}

// ============================================================================
// Lint: a <div> that holds nothing but text
// ============================================================================

/// Suppression tag for [`warn_div_used_as_text_container`].
pub const DIV_TEXT_SUPPRESS_TAG: &str = "div_as_text";

fn div_text_suppressed() -> bool {
    lint_suppressed(DIV_TEXT_SUPPRESS_TAG)
}

/// A `<div>` whose only child is a text node is almost always the wrong
/// element.
///
/// `div` is a generic CONTAINER — it says "a box, meaning unspecified". Text
/// that is a label, a caption, or a control's text belongs in `<span>`
/// (inline), and text that is prose belongs in `<p>`. Reaching for `div` is
/// usually a sign that someone wanted "a box around this text" and took the
/// first constructor that provided one.
///
/// It matters beyond tidiness. Assistive technology reads the element to decide
/// what a thing IS: a `div` conveys nothing, while `p` announces a paragraph
/// and `span` stays inline inside its label. It also affects layout — a `div`
/// is block-level, so a label inside a button becomes a block box where an
/// inline one was meant.
///
/// Not an error: a `div` with text is legal and occasionally right (a sizing
/// box that happens to contain a word). Hence a warning, and a suppression tag.
pub fn warn_div_used_as_text_container(styled_dom: &StyledDom) {
    if div_text_suppressed() {
        return;
    }

    let nodes = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();

    let mut reported = 0usize;
    for (node_id, node) in nodes
        .linear_iter()
        .filter_map(|id| nodes.get(id).map(|n| (id, n)))
    {
        if !matches!(node.get_node_type(), NodeType::Div) {
            continue;
        }
        // Exactly one child, and that child is a text node.
        let Some(item) = hierarchy.get(node_id) else {
            continue;
        };
        let Some(first) = item.first_child_id(node_id) else {
            continue;
        };
        let only_child = hierarchy
            .get(first)
            .is_some_and(|c| c.next_sibling_id().is_none());
        if !only_child {
            continue;
        }
        let Some(child) = nodes.get(first) else {
            continue;
        };
        let NodeType::Text(text) = child.get_node_type() else {
            continue;
        };

        if reported >= 8 {
            break; // one screenful is enough to act on
        }
        let printed = report_once(styled_dom, node_id, DIV_TEXT_SUPPRESS_TAG, || {
            let preview: String = text.as_str().chars().take(24).collect();
            format!(
                "[azul][div-as-text] node {} is a <div> whose only child is the text {preview:?}. \
                 A div is a generic container and says nothing about what the text IS. Use \
                 create_span_with_text for a label or a control's text (inline, stays inside its \
                 line box) or create_p_with_text for prose (block, announced as a paragraph). \
                 Assistive technology reads the element to decide what a thing is, and a div \
                 tells it nothing. (suppress with AZ_SUPPRESS={DIV_TEXT_SUPPRESS_TAG})",
                node_id.index()
            )
        });
        if printed {
            reported += 1;
        }
    }
}

// ============================================================================
// Lint: an interactive node with no accessible name
// ============================================================================

/// Suppression tag for [`warn_interactive_without_accessibility`].
pub const A11Y_SUPPRESS_TAG: &str = "a11y";

fn a11y_suppressed() -> bool {
    lint_suppressed(A11Y_SUPPRESS_TAG)
}

/// A node the user can CLICK, with nothing that names it to assistive
/// technology.
///
/// A screen reader announces a control by its accessible name. Without one it
/// reads "button" — or nothing at all — and the control is unusable without
/// sight. Azul builds its accessibility tree from `NodeData::accessibility`,
/// so a node with a click callback and no `AccessibilityInfo` is invisible to
/// that tree even though it is plainly visible on screen.
///
/// The fix is one call:
///
/// ```ignore
/// dom.with_accessibility_info(AccessibilityInfo {
///     accessibility_name: Some("Mute microphone".into()).into(),
///     ..Default::default()
/// })
/// ```
///
/// A node whose text child already spells out its label is NOT reported: azul
/// derives a name from the text in that case, so the control does announce
/// itself. What this catches is the icon-only button — the one whose label is a
/// glyph, where a sighted user infers "mute" from a picture and a screen reader
/// gets a private-use codepoint.
pub fn warn_interactive_without_accessibility(styled_dom: &StyledDom) {
    warn_interactive_without_accessibility_in(styled_dom, 0);
}

/// [`warn_interactive_without_accessibility`] for the window `scope`: every
/// unnamed control of the DOM in one line, once per window.
pub fn warn_interactive_without_accessibility_in(styled_dom: &StyledDom, scope: u64) {
    if a11y_suppressed() {
        return;
    }

    let nodes = styled_dom.node_data.as_container();

    let mut unnamed = Vec::new();
    for (node_id, node) in nodes
        .linear_iter()
        .filter_map(|id| nodes.get(id).map(|n| (id, n)))
    {
        // "Interactive" = the app attached a callback the USER can trigger.
        // Lifecycle, window and application listeners do not make a control.
        if !is_user_interactive(node) {
            continue;
        }
        if node.accessibility.is_some() {
            continue; // already named
        }

        // Does a descendant text node spell out a usable label? Azul derives a
        // name from it, so those controls DO announce themselves. (Descendants,
        // not children: a well-built control puts its label in a <span>; and
        // a private-use glyph is an icon, not a name - `has_readable_text_label`,
        // the one answer both a11y lints use.)
        if has_readable_text_label(styled_dom, node_id) {
            continue;
        }
        unnamed.push(node_id);
    }
    report_kind_once(styled_dom, scope, A11Y_SUPPRESS_TAG, 0, &unnamed, |list| {
        format!(
            "[azul][a11y] controls with a callback but no accessible name, and no text child \
             that could serve as one ({list}). An icon-only control reads to a screen reader \
             as a private-use codepoint, or as nothing. Add one: \
             .with_accessibility_info(AccessibilityInfo {{ accessibility_name: \
             Some(\"...\".into()).into(), ..Default::default() }}). (suppress with \
             AZ_SUPPRESS={A11Y_SUPPRESS_TAG})"
        )
    });
}

#[cfg(all(test, feature = "std"))]
mod semantic_and_a11y_lint_tests {
    use azul_core::dom::Dom;

    use super::*;

    // The lints share the global diagnostics ring, so they must not run
    // concurrently. The lock is workspace-wide and lives beside the ring:
    // see azul_core::diagnostics::test_lock.

    fn styled(dom: Dom) -> StyledDom {
        StyledDom::create_from_dom(dom)
    }

    #[test]
    fn a_div_holding_only_text_is_reported() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();

        warn_div_used_as_text_container(&styled(
            Dom::create_body().with_child(Dom::create_div_with_text("Unmute mic")),
        ));
        assert!(
            azul_core::diagnostics::any_contains("div-as-text"),
            "a <div> whose only child is text must be reported: {:?}",
            azul_core::diagnostics::recorded()
        );

        // ...and the alternatives must NOT be.
        azul_core::diagnostics::clear();
        warn_div_used_as_text_container(&styled(
            Dom::create_body()
                .with_child(Dom::create_span_with_text("Unmute mic"))
                .with_child(Dom::create_p_with_text("A paragraph of prose.")),
        ));
        assert!(
            !azul_core::diagnostics::any_contains("div-as-text"),
            "span and p are the RECOMMENDED forms and must not be flagged: {:?}",
            azul_core::diagnostics::recorded()
        );
        azul_core::diagnostics::clear();
    }

    /// A div that contains other elements is a container doing its job.
    #[test]
    fn a_div_with_element_children_is_not_reported() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();
        warn_div_used_as_text_container(&styled(
            Dom::create_body().with_child(
                Dom::create_div()
                    .with_child(Dom::create_span_with_text("a"))
                    .with_child(Dom::create_span_with_text("b")),
            ),
        ));
        assert!(!azul_core::diagnostics::any_contains("div-as-text"));
        azul_core::diagnostics::clear();
    }

    /// An ICON-only control — its label is a private-use glyph — announces
    /// nothing to a screen reader and must be reported.
    #[test]
    fn an_icon_only_control_without_a_name_is_reported() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();

        let icon_button = Dom::create_div()
            .with_child(Dom::create_span_with_text("\u{e161}")) // a Material icon
            .with_callback(
                azul_core::dom::EventFilter::Hover(
                    azul_core::dom::HoverEventFilter::MouseUp,
                ),
                azul_core::refany::RefAny::new(()),
                crate::callbacks::Callback::from_ptr(noop_cb),
            );
        warn_interactive_without_accessibility(&styled(Dom::create_body().with_child(icon_button)));
        assert!(
            azul_core::diagnostics::any_contains("[azul][a11y]"),
            "an icon-only control with no accessible name must be reported: {:?}",
            azul_core::diagnostics::recorded()
        );
        azul_core::diagnostics::clear();
    }

    /// A control whose text spells out its label DOES announce itself — azul
    /// derives a name from it — so reporting it would be noise.
    #[test]
    fn a_control_with_a_readable_label_is_not_reported() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();

        let text_button = Dom::create_div()
            .with_child(Dom::create_span_with_text("Unmute mic"))
            .with_callback(
                azul_core::dom::EventFilter::Hover(azul_core::dom::HoverEventFilter::MouseUp),
                azul_core::refany::RefAny::new(()),
                crate::callbacks::Callback::from_ptr(noop_cb),
            );
        warn_interactive_without_accessibility(&styled(Dom::create_body().with_child(text_button)));
        assert!(
            !azul_core::diagnostics::any_contains("[azul][a11y]"),
            "a control with a readable text label already announces itself: {:?}",
            azul_core::diagnostics::recorded()
        );
        azul_core::diagnostics::clear();
    }

    /// Lifecycle listeners (`AfterMount`, `NodeResized`) are delivered by node
    /// identity, never by the user: a node that has only those is not a
    /// control, and asking for its accessible name is noise (the video
    /// widget's root was reported as an unnamed icon-only control).
    #[test]
    fn a_node_with_only_lifecycle_callbacks_is_not_an_unnamed_control() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();

        let host = Dom::create_div()
            .with_callback(
                azul_core::dom::EventFilter::Component(
                    azul_core::dom::ComponentEventFilter::AfterMount,
                ),
                azul_core::refany::RefAny::new(()),
                crate::callbacks::Callback::from_ptr(noop_cb),
            )
            .with_child(Dom::create_div());
        warn_interactive_without_accessibility(&styled(Dom::create_body().with_child(host)));
        assert!(
            !azul_core::diagnostics::any_contains("[azul][a11y]"),
            "a lifecycle listener does not make a node a control: {:?}",
            azul_core::diagnostics::recorded()
        );
        azul_core::diagnostics::clear();
    }

    /// A `<button>` element whose declared role is left `Unknown` (the "not
    /// specified" default) IS a button to both accessibility trees, which fall
    /// back to the element's own role. Naming it must not make the lint call
    /// it "interactive, role Unknown".
    #[test]
    fn a_named_button_element_is_not_reported_as_role_unknown() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();

        let button = Dom::create_node(NodeType::Button)
            .with_accessibility_name("Play video")
            .with_callback(
                azul_core::dom::EventFilter::Hover(azul_core::dom::HoverEventFilter::MouseUp),
                azul_core::refany::RefAny::new(()),
                crate::callbacks::Callback::from_ptr(noop_cb),
            );
        warn_a11y_shape(&styled(Dom::create_body().with_child(button)));
        assert!(
            !azul_core::diagnostics::any_contains("is Unknown"),
            "a <button> falls back to the button role: {:?}",
            azul_core::diagnostics::recorded()
        );
        azul_core::diagnostics::clear();
    }

    /// Window-level listeners fire for the window, wherever the node is: a
    /// drop zone listening for dropped files is not a control, so a name
    /// without a role on it is not "interactive, role Unknown".
    #[test]
    fn a_node_with_only_window_callbacks_is_not_reported_as_role_unknown() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();

        let zone = Dom::create_div()
            .with_accessibility_name("Drop zone")
            .with_callback(
                azul_core::dom::EventFilter::Window(
                    azul_core::dom::WindowEventFilter::DroppedFile,
                ),
                azul_core::refany::RefAny::new(()),
                crate::callbacks::Callback::from_ptr(noop_cb),
            )
            .with_child(Dom::create_span_with_text("Drag files here"));
        warn_a11y_shape(&styled(Dom::create_body().with_child(zone)));
        assert!(
            !azul_core::diagnostics::any_contains("is Unknown"),
            "a window listener does not make a node a control: {:?}",
            azul_core::diagnostics::recorded()
        );
        azul_core::diagnostics::clear();
    }

    /// Control for the two above: a plain div the user can CLICK, named but
    /// with no role, is still reported.
    #[test]
    fn a_clickable_div_with_a_name_and_no_role_is_still_reported() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();

        let seek = Dom::create_div()
            .with_accessibility_name("Seek")
            .with_callback(
                azul_core::dom::EventFilter::Hover(azul_core::dom::HoverEventFilter::MouseUp),
                azul_core::refany::RefAny::new(()),
                crate::callbacks::Callback::from_ptr(noop_cb),
            );
        warn_a11y_shape(&styled(Dom::create_body().with_child(seek)));
        assert!(
            azul_core::diagnostics::any_contains("is Unknown"),
            "a clickable div with no role must still be reported: {:?}",
            azul_core::diagnostics::recorded()
        );
        azul_core::diagnostics::clear();
    }

    /// How many recorded diagnostics contain `tag`.
    fn printed(tag: &str) -> usize {
        azul_core::diagnostics::recorded()
            .iter()
            .filter(|m| m.contains(tag))
            .count()
    }

    /// A clickable div with a name and no role: one `[a11y-shape]` finding.
    fn seek_control() -> Dom {
        Dom::create_body().with_child(
            Dom::create_div()
                .with_accessibility_name("Seek")
                .with_callback(
                    azul_core::dom::EventFilter::Hover(azul_core::dom::HoverEventFilter::MouseUp),
                    azul_core::refany::RefAny::new(()),
                    crate::callbacks::Callback::from_ptr(noop_cb),
                ),
        )
    }

    /// The same finding is printed ONCE per process (ANIMFRAME8 s8): an app
    /// renders the same shape again and again, the lints meet each new DOM,
    /// and `diagnostics::emit` does no dedupe - `[a11y-shape]` printed its
    /// lines on every pass, as the bare-text lint never did.
    #[test]
    fn the_same_a11y_shape_finding_is_printed_once_per_process() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();
        warn_a11y_shape(&styled(seek_control()));
        warn_a11y_shape(&styled(seek_control()));
        assert_eq!(
            printed("[azul][a11y-shape]"),
            1,
            "the same finding twice is printed once: {:?}",
            azul_core::diagnostics::recorded()
        );
        azul_core::diagnostics::clear();
    }

    /// `count` navigation rows built by one helper: clickable and named, none
    /// with a role, each with its own id - the sidebar that printed one
    /// "role is Unknown" warning per row.
    fn nav_rows(prefix: &str, count: usize) -> StyledDom {
        let mut body = Dom::create_body();
        for i in 0..count {
            body = body.with_child(
                Dom::create_div()
                    .with_id(azul_css::AzString::from(format!("{prefix}-{i}")))
                    .with_accessibility_name(format!("Row {i}"))
                    .with_callback(
                        azul_core::dom::EventFilter::Hover(
                            azul_core::dom::HoverEventFilter::MouseUp,
                        ),
                        azul_core::refany::RefAny::new(()),
                        crate::callbacks::Callback::from_ptr(noop_cb),
                    ),
            );
        }
        styled(body)
    }

    /// Every node with the same problem is ONE line that says how many there
    /// are and names the first few: they come from one construction site, and
    /// one line per node buried the console. Another screen with the same
    /// problem in the same window adds nothing.
    #[test]
    fn every_node_with_the_same_a11y_problem_is_one_line_naming_them() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();
        warn_a11y_shape(&nav_rows("nav", 6));
        let lines = a11y_lines();
        assert_eq!(lines.len(), 1, "six rows with one problem are one line: {lines:?}");
        assert!(
            lines[0].contains("role is Unknown")
                && lines[0].contains("6 nodes")
                && lines[0].contains("#nav-0")
                && lines[0].contains("and 2 more"),
            "the line says the problem, the count and the first rows: {lines:?}"
        );
        warn_a11y_shape(&nav_rows("tab", 3));
        assert_eq!(a11y_lines().len(), 1, "the same problem again: {:?}", a11y_lines());
        azul_core::diagnostics::clear();
    }

    /// The lints say each problem once per WINDOW: a window that lays out the
    /// same shape again stays quiet, a second window says it once more.
    #[test]
    fn a_window_says_each_a11y_problem_once_and_the_next_window_says_it_again() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();
        warn_a11y_shape_in(&nav_rows("nav", 3), 1);
        warn_a11y_shape_in(&nav_rows("nav", 3), 1);
        assert_eq!(printed("[azul][a11y-shape]"), 1, "{:?}", a11y_lines());
        warn_a11y_shape_in(&nav_rows("nav", 3), 2);
        assert_eq!(printed("[azul][a11y-shape]"), 2, "{:?}", a11y_lines());
        azul_core::diagnostics::clear();
    }

    /// The same for the div-as-text lint.
    #[test]
    fn the_same_div_as_text_finding_is_printed_once_per_process() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();
        let page = || styled(Dom::create_body().with_child(Dom::create_div_with_text("Mute")));
        warn_div_used_as_text_container(&page());
        warn_div_used_as_text_container(&page());
        assert_eq!(
            printed("[azul][div-as-text]"),
            1,
            "the same finding twice is printed once: {:?}",
            azul_core::diagnostics::recorded()
        );
        azul_core::diagnostics::clear();
    }

    /// The same for the unnamed-control lint.
    #[test]
    fn the_same_unnamed_control_finding_is_printed_once_per_process() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();
        let page = || {
            styled(
                Dom::create_body().with_child(
                    Dom::create_div()
                        .with_child(Dom::create_span_with_text("\u{e161}"))
                        .with_callback(
                            azul_core::dom::EventFilter::Hover(
                                azul_core::dom::HoverEventFilter::MouseUp,
                            ),
                            azul_core::refany::RefAny::new(()),
                            crate::callbacks::Callback::from_ptr(noop_cb),
                        ),
                ),
            )
        };
        warn_interactive_without_accessibility(&page());
        warn_interactive_without_accessibility(&page());
        assert_eq!(
            printed("[azul][a11y] "),
            1,
            "the same finding twice is printed once: {:?}",
            azul_core::diagnostics::recorded()
        );
        azul_core::diagnostics::clear();
    }

    /// The recorded a11y findings (every lint tag starts `[azul][a11y`).
    fn a11y_lines() -> Vec<String> {
        azul_core::diagnostics::recorded()
            .into_iter()
            .filter(|m| m.contains("[azul][a11y"))
            .collect()
    }

    /// A widget named on its built DOM (`.dom().with_accessibility_name(..)`,
    /// a row helper naming its control by the row's label) IS named. Only the
    /// finished DOM can tell, so nothing may be printed while it is built.
    #[cfg(feature = "widgets")]
    #[test]
    fn a_check_box_named_after_it_is_built_prints_no_accessibility_warning() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();
        let page = styled(
            Dom::create_body().with_child(
                crate::widgets::check_box::CheckBox::create(true)
                    .dom()
                    .with_accessibility_name("Remember me")
                    .with_id("a11y17-named-check".into()),
            ),
        );
        warn_interactive_without_accessibility(&page);
        warn_a11y_shape(&page);
        assert!(a11y_lines().is_empty(), "a named check box: {:?}", a11y_lines());
        azul_core::diagnostics::clear();
    }

    /// An unnamed widget is ONE finding however often the app builds it, and
    /// the finding says which node it is: the widget, its id, its classes.
    #[cfg(feature = "widgets")]
    #[test]
    fn an_unnamed_check_box_is_one_finding_that_names_the_node() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();
        let page = || {
            styled(
                Dom::create_body().with_child(
                    crate::widgets::check_box::CheckBox::create(false)
                        .dom()
                        .with_id("a11y17-unnamed-check".into()),
                ),
            )
        };
        warn_a11y_shape(&page());
        warn_a11y_shape(&page());
        let lines = a11y_lines();
        assert_eq!(lines.len(), 1, "one unnamed check box, built twice: {lines:?}");
        assert!(
            lines[0].contains("#a11y17-unnamed-check") && lines[0].contains("Checkbox"),
            "the finding names the widget and its id: {lines:?}"
        );
        azul_core::diagnostics::clear();
    }

    fn toolbar(buttons: Vec<Dom>) -> Dom {
        let info = azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Toolbar,
            accessibility_name: Some(azul_css::AzString::from("Formatting")).into(),
            ..Default::default()
        };
        Dom::create_div()
            .with_id("a11y17-toolbar".into())
            .with_accessibility_info(info)
            .with_children(buttons.into())
    }

    /// A composite widget - a toolbar, a menu bar, a tab list, a list box,
    /// a tree, a grid - is ONE stop in the Tab order and the arrow keys move
    /// inside it (`widgets::roving`). A toolbar that makes every button a Tab
    /// stop makes a keyboard user Tab through all of them.
    #[test]
    fn a_toolbar_with_a_tab_stop_per_button_is_reported() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();
        let every_button_a_stop = toolbar(vec![
            Dom::create_button_no_a11y("Bold".into()),
            Dom::create_button_no_a11y("Italic".into()),
            Dom::create_button_no_a11y("Underline".into()),
        ]);
        warn_composite_navigation(&styled(Dom::create_body().with_child(every_button_a_stop)));
        let lines = a11y_lines();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("[azul][a11y-nav]") && lines[0].contains("#a11y17-toolbar"),
            "{lines:?}"
        );

        azul_core::diagnostics::clear();
        let roving = toolbar(vec![
            Dom::create_button_no_a11y("Bold".into()).with_tab_index(azul_core::dom::TabIndex::Auto),
            Dom::create_button_no_a11y("Italic".into())
                .with_tab_index(azul_core::dom::TabIndex::NoKeyboardFocus),
            Dom::create_button_no_a11y("Underline".into())
                .with_tab_index(azul_core::dom::TabIndex::NoKeyboardFocus),
        ]);
        warn_composite_navigation(&styled(Dom::create_body().with_child(roving)));
        assert!(a11y_lines().is_empty(), "a roving toolbar: {:?}", a11y_lines());
        azul_core::diagnostics::clear();
    }

    fn tile_grid(css: &str) -> Dom {
        let mut grid = Dom::create_div().with_id("a11y17-grid".into()).with_css(css);
        for k in 0..6 {
            grid = grid.with_child(
                Dom::create_div()
                    .with_tab_index(azul_core::dom::TabIndex::NoKeyboardFocus)
                    .with_accessibility_name(format!("tile {k}")),
            );
        }
        grid
    }

    /// In a grid of focusable tiles the arrow keys must move along its rows
    /// and columns: `spatial-navigation-function: grid`. The default picks
    /// the nearest tile by distance, which jumps diagonally.
    #[test]
    fn a_grid_of_focusable_tiles_without_grid_navigation_is_reported() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();
        warn_composite_navigation(&styled(Dom::create_body().with_child(tile_grid(
            "display: grid; grid-template-columns: 1fr 1fr 1fr;",
        ))));
        let lines = a11y_lines();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("spatial-navigation-function: grid") && lines[0].contains("#a11y17-grid"),
            "{lines:?}"
        );

        azul_core::diagnostics::clear();
        warn_composite_navigation(&styled(Dom::create_body().with_child(tile_grid(
            "display: grid; grid-template-columns: 1fr 1fr 1fr; spatial-navigation-function: grid;",
        ))));
        assert!(a11y_lines().is_empty(), "{:?}", a11y_lines());
        azul_core::diagnostics::clear();
    }

    extern "C" fn noop_cb(
        _: azul_core::refany::RefAny,
        _: crate::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        azul_core::callbacks::Update::DoNothing
    }
}

/// The azul widget a node came from, if it says so.
///
/// Widgets tag themselves `__azul-native-<name>`, which lets the FRAMEWORK lint
/// give WIDGET-specific advice — naming the type and the exact builder call —
/// without the widget having to warn at build time.
///
/// That distinction matters: a widget cannot warn about a missing name itself,
/// because the name arrives AFTER it is built. `Slider::new(..).dom()` runs
/// first, `.with_accessibility_name("Volume")` second. A build-time warning
/// would fire on every correctly-named slider in existence. The lint runs on
/// the finished DOM, so it sees the answer.
fn azul_widget_kind(node: &azul_core::dom::NodeData) -> Option<String> {
    node.get_ids_and_classes().iter().find_map(|ic| {
        let s = match ic {
            azul_core::dom::IdOrClass::Class(c) => c.as_str(),
            azul_core::dom::IdOrClass::Id(_) => return None,
        };
        let rest = s.strip_prefix("__azul-native-")?;
        // "__azul-native-slider-thumb" -> "slider": the first segment is the
        // widget; the rest names a part inside it.
        let name = rest.split('-').next()?;
        if name.is_empty() {
            return None;
        }
        let mut c = name.chars();
        Some(c.next()?.to_uppercase().collect::<String>() + c.as_str())
    })
}

/// Does a readable text label exist at or beneath `node_id`?
///
/// "Readable" excludes private-use codepoints: an icon glyph is a picture, and
/// reads to a screen reader as a meaningless character rather than a name.
///
/// Shared by the a11y lints because both need the same question answered, and
/// answering it differently in two places is how a lint acquires false
/// positives — the shape lint reported every labelled checkbox as "anonymous"
/// until it used this.
fn has_readable_text_label(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    let nodes = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut stack = vec![node_id];
    let mut visited = 0usize;
    while let Some(cur) = stack.pop() {
        visited += 1;
        if visited > 64 {
            break; // a label is never buried this deep; bound the walk
        }
        if let Some(cn) = nodes.get(cur) {
            if let NodeType::Text(t) = cn.get_node_type() {
                if t.as_str()
                    .chars()
                    .any(|ch| ch.is_alphanumeric() && !('\u{e000}'..='\u{f8ff}').contains(&ch))
                {
                    return true;
                }
            }
        }
        if let Some(item) = hierarchy.get(cur) {
            if let Some(first) = item.first_child_id(cur) {
                let mut sib = Some(first);
                while let Some(sid) = sib {
                    stack.push(sid);
                    sib = hierarchy
                        .get(sid)
                        .and_then(NodeHierarchyItem::next_sibling_id);
                }
            }
        }
    }
    false
}

/// How a finding names its node: its index, the azul widget it belongs to,
/// its ids and classes (the selectors a developer searches the app for) and
/// the start of its text. "node 40" alone is a puzzle; "node 40 (a Slider
/// #volume .settings-row)" is a call site.
fn describe_node(styled_dom: &StyledDom, node_id: NodeId) -> String {
    let nodes = styled_dom.node_data.as_container();
    let Some(node) = nodes.get(node_id) else {
        return format!("node {}", node_id.index());
    };
    let mut parts = Vec::new();
    if let Some(kind) = azul_widget_kind(node) {
        parts.push(format!("a {kind}"));
    }
    for ic in &node.get_ids_and_classes() {
        match ic {
            azul_core::dom::IdOrClass::Id(id) => parts.push(format!("#{}", id.as_str())),
            azul_core::dom::IdOrClass::Class(c)
                if !c.as_str().starts_with("__azul-native-") && parts.len() < 6 =>
            {
                parts.push(format!(".{}", c.as_str()));
            }
            azul_core::dom::IdOrClass::Class(_) => {}
        }
    }
    if let Some(text) = first_text(styled_dom, node_id) {
        parts.push(snippet_of(&text));
    }
    if parts.is_empty() {
        format!("node {}", node_id.index())
    } else {
        format!("node {} ({})", node_id.index(), parts.join(" "))
    }
}

/// The first non-blank text at or beneath `node_id` (bounded walk).
fn first_text(styled_dom: &StyledDom, node_id: NodeId) -> Option<String> {
    let nodes = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut stack = vec![node_id];
    let mut visited = 0usize;
    while let Some(cur) = stack.pop() {
        visited += 1;
        if visited > 64 {
            return None;
        }
        if let Some(NodeType::Text(t)) =
            nodes.get(cur).map(azul_core::dom::NodeData::get_node_type)
        {
            if !t.as_str().trim().is_empty() {
                return Some(t.as_str().to_string());
            }
        }
        let mut children = Vec::new();
        let mut child = hierarchy.get(cur).and_then(|item| item.first_child_id(cur));
        while let Some(c) = child {
            children.push(c);
            child = hierarchy.get(c).and_then(NodeHierarchyItem::next_sibling_id);
        }
        // Document order: the first child is popped first.
        stack.extend(children.into_iter().rev());
    }
    None
}

// ============================================================================
// Lint: keyboard navigation inside composites and grids
// ============================================================================

/// Suppression tag for [`warn_composite_navigation`].
pub const A11Y_NAV_SUPPRESS_TAG: &str = "a11y_nav";

/// Whether the Tab key stops on `node`: a tab index other than
/// "no keyboard focus", or an element focusable by itself (a link, a button,
/// a select).
const fn is_tab_stop(node: &azul_core::dom::NodeData) -> bool {
    use azul_core::dom::TabIndex;
    match node.get_tab_index() {
        Some(TabIndex::NoKeyboardFocus) => false,
        Some(_) => true,
        None => matches!(
            node.get_node_type(),
            NodeType::A | NodeType::Button | NodeType::Select
        ),
    }
}

/// Whether `node` edits text: Tab is how a user leaves a field, so a field
/// inside a composite (a toolbar's search box) keeps its own Tab stop.
const fn edits_text(node: &azul_core::dom::NodeData) -> bool {
    node.is_contenteditable() || matches!(node.get_node_type(), NodeType::Input | NodeType::TextArea)
}

/// Tell the developer where keyboard navigation goes wrong, from the shape
/// of the DOM.
///
/// - A composite - a toolbar, a menu bar, a tab list, a list box, a list, a tree, a grid - is ONE
///   stop in the Tab order and the arrow keys move inside it (WAI-ARIA APG, `widgets::roving`). One
///   whose items are each a Tab stop makes a keyboard user Tab through all of them, and Shift+Tab
///   back.
/// - A CSS grid of focusable tiles needs `spatial-navigation-function: grid`, or the arrow keys pick
///   the nearest tile by distance and jump diagonally instead of along the rows and columns.
///
/// Reported once per problem; suppress with `AZ_SUPPRESS=a11y_nav`.
pub fn warn_composite_navigation(styled_dom: &StyledDom) {
    use azul_core::a11y::AccessibilityRole as R;
    use azul_css::props::style::spatial_nav::StyleSpatialNavigationFunction;

    if lint_suppressed(A11Y_NAV_SUPPRESS_TAG) {
        return;
    }
    let nodes = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let styled_nodes = styled_dom.styled_nodes.as_container();
    let mut reported = 0usize;

    for (node_id, node) in nodes
        .linear_iter()
        .filter_map(|id| nodes.get(id).map(|n| (id, n)))
    {
        if reported >= 8 {
            break;
        }
        let role = node.accessibility.as_ref().map(|info| info.role);
        let composite = matches!(
            role,
            Some(
                R::Toolbar
                    | R::MenuBar
                    | R::PageTabList
                    | R::DropList
                    | R::List
                    | R::Outline
                    | R::Grid
            )
        );

        if composite {
            // The Tab stops inside it, a text field's aside.
            let mut stops = 0usize;
            let mut stack: Vec<NodeId> = Vec::new();
            let mut child = hierarchy.get(node_id).and_then(|item| item.first_child_id(node_id));
            while let Some(c) = child {
                stack.push(c);
                child = hierarchy.get(c).and_then(NodeHierarchyItem::next_sibling_id);
            }
            let mut visited = 0usize;
            while let Some(cur) = stack.pop() {
                visited += 1;
                if visited > 4096 {
                    break;
                }
                let Some(data) = nodes.get(cur) else {
                    continue;
                };
                if is_tab_stop(data) && !edits_text(data) {
                    stops += 1;
                }
                let mut grandchild = hierarchy.get(cur).and_then(|item| item.first_child_id(cur));
                while let Some(g) = grandchild {
                    stack.push(g);
                    grandchild = hierarchy.get(g).and_then(NodeHierarchyItem::next_sibling_id);
                }
            }
            if stops > 1
                && report_once(styled_dom, node_id, A11Y_NAV_SUPPRESS_TAG, || {
                    let who = describe_node(styled_dom, node_id);
                    let role = role.map(|r| format!("{r:?}")).unwrap_or_default();
                    format!(
                        "[azul][a11y-nav] {who} is a {role} with {stops} Tab stops inside it. A \
                         composite is ONE stop in the Tab order and the arrow keys move between \
                         its items: build the active item with TabIndex::Auto and the others with \
                         TabIndex::NoKeyboardFocus, move the stop on the arrow keys \
                         (widgets::roving), or use the toolbar / tab / list widget that does. \
                         (suppress with AZ_SUPPRESS={A11Y_NAV_SUPPRESS_TAG})"
                    )
                })
            {
                reported += 1;
            }
            continue;
        }

        if !matches!(display_of(styled_dom, node_id), LayoutDisplay::Grid | LayoutDisplay::InlineGrid) {
            continue;
        }
        let mut tiles = 0usize;
        let mut child = hierarchy.get(node_id).and_then(|item| item.first_child_id(node_id));
        while let Some(c) = child {
            if nodes
                .get(c)
                .is_some_and(|d| d.get_tab_index().is_some() || d.is_focusable())
            {
                tiles += 1;
            }
            child = hierarchy.get(c).and_then(NodeHierarchyItem::next_sibling_id);
        }
        if tiles < 4 {
            continue;
        }
        let grid_function = styled_nodes.get(node_id).is_some_and(|sn| {
            matches!(
                crate::solver3::getters::get_spatial_navigation_function(
                    styled_dom,
                    node_id,
                    &sn.styled_node_state
                ),
                MultiValue::Exact(StyleSpatialNavigationFunction::Grid)
            )
        });
        if !grid_function
            && report_once(styled_dom, node_id, A11Y_NAV_SUPPRESS_TAG, || {
                let who = describe_node(styled_dom, node_id);
                format!(
                    "[azul][a11y-nav] {who} is a CSS grid of {tiles} focusable tiles, and the arrow \
                     keys pick the nearest tile by distance, so they jump diagonally. Add \
                     `spatial-navigation-function: grid;` to it: the arrows then move along its rows \
                     and columns. (suppress with AZ_SUPPRESS={A11Y_NAV_SUPPRESS_TAG})"
                )
            })
        {
            reported += 1;
        }
    }
}

// ============================================================================
// Lint: what this node's SHAPE says it should declare
// ============================================================================

/// Suppression tag for [`warn_a11y_shape`].
pub const A11Y_SHAPE_SUPPRESS_TAG: &str = "a11y_shape";

/// Tell a developer, from the shape of the DOM, which accessibility attributes
/// this particular node is missing — and why each one matters.
///
/// ## The distinction this exists to teach
///
/// Azul builds its accessibility tree from `NodeData::accessibility`. A node
/// with `None` there is **not in that tree**: it is on screen and absent from
/// the interface a screen-reader user actually has. This is not "less
/// accessible", it is *invisible*, and it is the default for every node you
/// build. There is no partial credit and no automatic fallback beyond a name
/// derived from text.
///
/// `AccessibilityInfo` carries far more than a name — a role out of 118, a
/// value, a description, states, `labelled_by` / `described_by` relations, a
/// default action, an accelerator, and a live-region flag. Nobody discovers
/// that by reading `core/src/a11y.rs`, so this lint reads the node's shape and
/// names the specific field it is missing:
///
/// | shape observed                      | what to declare        |
/// |-------------------------------------|------------------------|
/// | interactive, no accessibility at all | `accessibility_name`, plus a `role` |
/// | declared, but role is `Unknown`       | `role` — "what IS this" |
/// | `Slider` / `ProgressBar` / `ScrollBar` | `accessibility_value` |
/// | `CheckButton` / `RadioButton`         | a checked state       |
/// | `PushButton` with no action named     | `default_action`      |
/// | an image with no name                | `accessibility_name` or `description` |
/// | keyboard-focusable, no role          | `role`                 |
///
/// Suppress with `AZ_SUPPRESS=a11y_shape`, or `AZ_SUPPRESS=all` for every lint.
pub fn warn_a11y_shape(styled_dom: &StyledDom) {
    warn_a11y_shape_in(styled_dom, 0);
}

/// [`warn_a11y_shape`] for the window `scope`: each kind of finding is one
/// line naming its nodes, once per window.
pub fn warn_a11y_shape_in(styled_dom: &StyledDom, scope: u64) {
    if lint_suppressed(A11Y_SHAPE_SUPPRESS_TAG) {
        return;
    }

    let nodes = styled_dom.node_data.as_container();
    let mut found: std::collections::BTreeMap<ShapeFinding, Vec<NodeId>> =
        std::collections::BTreeMap::new();
    for (node_id, node) in nodes
        .linear_iter()
        .filter_map(|id| nodes.get(id).map(|n| (id, n)))
    {
        if let Some(kind) = shape_finding(styled_dom, node_id, node) {
            found.entry(kind).or_default().push(node_id);
        }
    }
    for (kind, ids) in &found {
        report_kind_once(styled_dom, scope, A11Y_SHAPE_SUPPRESS_TAG, *kind as u8, ids, |list| {
            format!(
                "[azul][a11y-shape] {} ({list}). {} (suppress with \
                 AZ_SUPPRESS={A11Y_SHAPE_SUPPRESS_TAG})",
                kind.problem(),
                kind.advice()
            )
        });
    }
}

/// What [`warn_a11y_shape`] finds on a node: each kind is printed as one
/// line naming its nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ShapeFinding {
    /// An image with no accessibility info.
    UndeclaredImage,
    /// A tab stop with no accessibility info.
    UndeclaredFocusable,
    /// A control that declares accessibility and leaves its role Unknown.
    UnknownRole,
    /// A slider, progress bar or scroll bar without its value.
    NoValue,
    /// A check box, radio button or switch without its checked state.
    NoCheckedState,
    /// A declared node with no name, no label and no text to take one from.
    Anonymous,
}

impl ShapeFinding {
    /// What the nodes of this kind are.
    const fn problem(self) -> &'static str {
        match self {
            Self::UndeclaredImage => "IMAGES with no accessibility info",
            Self::UndeclaredFocusable => {
                "KEYBOARD-FOCUSABLE nodes (they have a tab_index) that declare no accessibility \
                 info"
            }
            Self::UnknownRole => {
                "interactive nodes that declare accessibility, but their role is Unknown"
            }
            Self::NoValue => "sliders, progress bars or scroll bars with no `accessibility_value`",
            Self::NoCheckedState => {
                "check boxes, radio buttons or switches that declare no CheckedTrue / \
                 CheckedFalse / Selected state"
            }
            Self::Anonymous => {
                "nodes that declare accessibility but have neither an `accessibility_name` nor a \
                 `labelled_by`"
            }
        }
    }

    /// Why it matters and what to declare.
    const fn advice(self) -> &'static str {
        match self {
            Self::UndeclaredImage => {
                "A screen reader announces nothing where a sighted user sees a picture. Give it \
                 .with_accessibility_info(AccessibilityInfo::named(\"what it shows\", \
                 AccessibilityRole::Graphic)), or, when it only decorates something named \
                 elsewhere, mark it decorative with `role: AccessibilityRole::Nothing` and no \
                 name."
            }
            Self::UndeclaredFocusable => {
                "Tabbing lands on something the screen reader cannot describe. At minimum give \
                 it a `role` and an `accessibility_name`."
            }
            Self::UnknownRole => {
                "A screen reader can say the name and not what the control IS. Pick from \
                 AccessibilityRole (PushButton, CheckBox, ComboBox, Slider, Link, PageTab, \
                 MenuItem, ListItem, ...)."
            }
            Self::NoValue => {
                "The value IS the content of these controls: without it a screen reader \
                 announces \"slider\" and never how far along it is. Set accessibility_value on \
                 every change, not once."
            }
            Self::NoCheckedState => {
                "It always announces as unchecked no matter what it renders. Push the state into \
                 AccessibilityInfo::states when the control toggles."
            }
            Self::Anonymous => {
                "It is in the tree and anonymous, with no text of its own to derive a name from, \
                 and only the CALL SITE knows what it is called: add \
                 `.with_accessibility_name(\"...\")` there - it MERGES, so the role, value and \
                 states survive - or point at the label you already render with \
                 `.with_accessibility_labelled_by(node)`, which cannot drift out of sync the way \
                 a copied string does."
            }
        }
    }
}

/// Which [`ShapeFinding`] the node has, if any: the first that applies.
fn shape_finding(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node: &azul_core::dom::NodeData,
) -> Option<ShapeFinding> {
    use azul_core::a11y::{AccessibilityRole, AccessibilityState};

    let interactive = is_user_interactive(node);
    let focusable = node.get_tab_index().is_some();

    let Some(info) = node.accessibility.as_ref() else {
        // The interactive no-a11y case is warn_interactive_without_accessibility's.
        // Here, the shapes worth naming even when they are not clickable.
        if matches!(node.get_node_type(), NodeType::Image(_)) {
            return Some(ShapeFinding::UndeclaredImage);
        }
        return focusable.then_some(ShapeFinding::UndeclaredFocusable);
    };

    // It DECLARED accessibility - now check that the declaration matches the
    // shape. A half-filled AccessibilityInfo is the subtler failure: the node
    // is in the tree, so nothing looks broken, and it still announces wrongly.
    let role = info.role;
    let has_name = info.accessibility_name.as_ref().is_some();

    // `Unknown` means "not specified": both accessibility trees then fall
    // back to the ELEMENT's own role (a `<button>` is a button), so only a
    // generic element left at `Unknown` really says nothing about itself.
    if matches!(role, AccessibilityRole::Unknown)
        && !element_implies_a_role(node.get_node_type())
        && (interactive || focusable)
    {
        return Some(ShapeFinding::UnknownRole);
    }

    if matches!(
        role,
        AccessibilityRole::Slider | AccessibilityRole::ProgressBar | AccessibilityRole::ScrollBar
    ) && info.accessibility_value.as_ref().is_none()
    {
        return Some(ShapeFinding::NoValue);
    }

    if matches!(
        role,
        AccessibilityRole::CheckButton | AccessibilityRole::RadioButton | AccessibilityRole::Switch
    ) && !info.states.as_ref().iter().any(|s| {
        matches!(
            s,
            AccessibilityState::CheckedTrue
                | AccessibilityState::CheckedFalse
                | AccessibilityState::Selected
        )
    }) {
        return Some(ShapeFinding::NoCheckedState);
    }

    // An explicit `role: Nothing` with no name on something nobody can click
    // or tab to is the DECORATIVE mark (ARIA's role="presentation"): the
    // picture is part of a whole that is named elsewhere, like one tile of a
    // map whose container carries the name. Demanding a name here would make
    // a screen reader read out every tile.
    if matches!(role, AccessibilityRole::Nothing) && !has_name && !interactive && !focusable {
        return None;
    }

    // A control whose visible label is a text node DOES announce itself -
    // azul derives the name from that text. Reporting those was a false
    // positive on every correctly-built labelled checkbox and slider.
    let anonymous = !has_name
        && info.labelled_by.as_ref().is_none()
        && !has_readable_text_label(styled_dom, node_id);
    anonymous.then_some(ShapeFinding::Anonymous)
}
