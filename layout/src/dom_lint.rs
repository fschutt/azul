//! Post-layout developer warnings for misused raw text nodes.
//!
//! Browsers wrap a raw text run in an ANONYMOUS BLOCK whenever it needs one;
//! azul does not. A bare `NodeType::Text` therefore has no box of its own —
//! no rect, no clip, no layout constraints — and every box-model CSS
//! property, every id or class a stylesheet could reach it by, and every
//! callback, `tab_index` or `dataset` attached to one is silently inert.
//! That silence has shipped real bugs (text escaping its widget, click
//! targets that never fire), which is why the raw constructor is named
//! `create_p_with_text` and why this pass
//! exists: after layout, every text node in a shape azul cannot honor is
//! reported to the developer, once per unique finding.
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

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::mem::discriminant;
use std::sync::Mutex;

use azul_core::{
    dom::{AttributeType, NodeType},
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
        v.split(',').any(|t| t.trim().eq_ignore_ascii_case(SUPPRESS_TAG))
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

fn display_of(styled_dom: &StyledDom, node_id: NodeId) -> LayoutDisplay {
    match get_display_property(styled_dom, Some(node_id)) {
        MultiValue::Exact(d) => d,
        _ => LayoutDisplay::Block,
    }
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
            if !data.get_callbacks().as_ref().is_empty() {
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
            child = hierarchy.get(cc).and_then(NodeHierarchyItem::next_sibling_id);
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
            // dedicated line box of its own next to block siblings.
            let mut has_block_child = false;
            let mut sibling = h.first_child_id(node_id);
            while let Some(sib) = sibling {
                if !is_text(node_data[sib].get_node_type())
                    && is_block_level(display_of(styled_dom, sib))
                {
                    has_block_child = true;
                    break;
                }
                sibling = hierarchy.get(sib).and_then(NodeHierarchyItem::next_sibling_id);
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
            child = hierarchy.get(cc).and_then(NodeHierarchyItem::next_sibling_id);
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
                inert.push("callbacks");
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
                "text node {idx} ({snippet}) carries {} — INERT: a text node has no box. \
                 Move them onto a block wrapper (create_p_with_text / create_div_with_text) \
                 instead of the raw text node.",
                inert.join(" + "),
            )
        }
        Finding::NoParent => format!(
            "text node {idx} ({snippet}) has no parent — a raw text run needs a \
             block-level container (p / div / ...) to own its box.",
        ),
        Finding::TextParent => format!(
            "text node {idx} ({snippet}) is the child of another text node — \
             wrap both in a block-level container (p / div / ...).",
        ),
        Finding::FlexItem {
            display,
            child_count,
        } => format!(
            "text node {idx} ({snippet}) is one of {child_count} items in a \
             {display:?} container — a raw text run competes in flex/grid \
             layout with no box of its own (browsers auto-wrap it in an anonymous \
             block; azul does not). Wrap it: create_p_with_text / \
             create_div_with_text.",
        ),
        Finding::BlockSibling => format!(
            "text node {idx} ({snippet}) sits NEXT TO block-level siblings — browsers \
             would wrap it in an anonymous block, azul does not, so it has no line box \
             of its own. Wrap it: create_p_with_text / create_div_with_text.",
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
    let node_data = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    finding.rank().hash(&mut hasher);
    match finding {
        Finding::Inert(flags) => flags.hash(&mut hasher),
        // NOT the child_count: adding an item to the container is not a new
        // bug.
        Finding::FlexItem { display, .. } => display.hash(&mut hasher),
        _ => {}
    }

    hash_identity(&node_data[node_id], &mut hasher);
    match hierarchy.get(node_id).and_then(NodeHierarchyItem::parent_id) {
        None => 0u8.hash(&mut hasher),
        Some(parent_id) => {
            1u8.hash(&mut hasher);
            hash_identity(&node_data[parent_id], &mut hasher);
        }
    }
    hasher.finish()
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
                "[azul][text-without-block] {MAX_DISTINCT_WARNINGS} distinct findings \
                 reported — further warnings suppressed \
                 (suppress the whole lint with AZ_SUPPRESS={SUPPRESS_TAG})"
            );
            return;
        }
        eprintln!(
            "[azul][text-without-block] WARNING: {} \
             (suppress with AZ_SUPPRESS={SUPPRESS_TAG})",
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
    use azul_core::dom::{Dom, IdOrClass, IdOrClassVec, TabIndex};
    use azul_core::styled_dom::StyledDom;
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
        let sd = styled(Dom::create_body().with_child(Dom::create_p().with_child(text)), "");
        let w = collect_text_placement_warnings(&sd);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("INERT"), "{w:?}");
        assert!(w[0].contains("tab_index"), "{w:?}");
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
            assert_eq!(w, Vec::<String>::new(), "widget {name} trips the text lint: {w:?}");
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
    use std::{collections::BTreeMap, sync::{Mutex, OnceLock}};
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
    for (node_id, node) in nodes.linear_iter().filter_map(|id| nodes.get(id).map(|n| (id, n))) {
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

        reported += 1;
        if reported > 8 {
            break; // one screenful is enough to act on
        }
        let preview: String = text.as_str().chars().take(24).collect();
        azul_core::diagnostics::emit(format!(
            "[azul][div-as-text] node {} is a <div> whose only child is the text \
             {preview:?}. A div is a generic container and says nothing about \
             what the text IS. Use create_span_with_text for a label or a \
             control's text (inline, stays inside its line box) or \
             create_p_with_text for prose (block, announced as a paragraph). \
             Assistive technology reads the element to decide what a thing is, \
             and a div tells it nothing. \
             (suppress with AZ_SUPPRESS={DIV_TEXT_SUPPRESS_TAG})",
            node_id.index()
        ));
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
    if a11y_suppressed() {
        return;
    }

    let nodes = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();

    let mut reported = 0usize;
    for (node_id, node) in nodes.linear_iter().filter_map(|id| nodes.get(id).map(|n| (id, n))) {
        // "Interactive" = the app attached a callback to it.
        if node.get_callbacks().is_empty() {
            continue;
        }
        if node.accessibility.is_some() {
            continue; // already named
        }

        // Does a descendant text node spell out a usable label? Azul derives a
        // name from it, so those controls DO announce themselves.
        // Walk DESCENDANTS, not just direct children. A well-built control puts
        // its label in a <span> — the shape this crate recommends — so the text
        // is a grandchild. Scanning one level deep flagged every correctly
        // written button, which a test caught before this shipped.
        let mut has_readable_text = false;
        let mut stack = vec![node_id];
        let mut visited = 0usize;
        while let Some(cur) = stack.pop() {
            visited += 1;
            if visited > 64 {
                break; // a label is never buried this deep; bound the walk
            }
            if let Some(cn) = nodes.get(cur) {
                if let NodeType::Text(t) = cn.get_node_type() {
                    // A private-use glyph is an ICON, not a name: it reads as a
                    // meaningless codepoint.
                    if t.as_str().chars().any(|ch| {
                        ch.is_alphanumeric() && !('\u{e000}'..='\u{f8ff}').contains(&ch)
                    }) {
                        has_readable_text = true;
                        break;
                    }
                }
            }
            if let Some(item) = hierarchy.get(cur) {
                if let Some(first) = item.first_child_id(cur) {
                    let mut sib = Some(first);
                    while let Some(sid) = sib {
                        stack.push(sid);
                        sib = hierarchy.get(sid).and_then(NodeHierarchyItem::next_sibling_id);
                    }
                }
            }
        }
        if has_readable_text {
            continue;
        }

        reported += 1;
        if reported > 8 {
            break;
        }
        azul_core::diagnostics::emit(format!(
            "[azul][a11y] node {} has a callback but no accessible name, and no \
             text child that could serve as one — an icon-only control reads to \
             a screen reader as a private-use codepoint, or as nothing. Add one: \
             .with_accessibility_info(AccessibilityInfo {{ accessibility_name: \
             Some(\"...\".into()).into(), ..Default::default() }}). \
             (suppress with AZ_SUPPRESS={A11Y_SUPPRESS_TAG})",
            node_id.index()
        ));
    }
}

#[cfg(all(test, feature = "std"))]
mod semantic_and_a11y_lint_tests {
    use super::*;
    use azul_core::dom::Dom;

    /// The lints share the global diagnostics ring, so they must not run
    /// concurrently — this has already bitten twice in this codebase.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn styled(dom: Dom) -> StyledDom {
        StyledDom::create_from_dom(dom)
    }

    #[test]
    fn a_div_holding_only_text_is_reported() {
        let _g = TEST_LOCK.lock();
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
        let _g = TEST_LOCK.lock();
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
        let _g = TEST_LOCK.lock();
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
        warn_interactive_without_accessibility(&styled(
            Dom::create_body().with_child(icon_button),
        ));
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
        let _g = TEST_LOCK.lock();
        azul_core::diagnostics::clear();

        let text_button = Dom::create_div()
            .with_child(Dom::create_span_with_text("Unmute mic"))
            .with_callback(
                azul_core::dom::EventFilter::Hover(
                    azul_core::dom::HoverEventFilter::MouseUp,
                ),
                azul_core::refany::RefAny::new(()),
                crate::callbacks::Callback::from_ptr(noop_cb),
            );
        warn_interactive_without_accessibility(&styled(
            Dom::create_body().with_child(text_button),
        ));
        assert!(
            !azul_core::diagnostics::any_contains("[azul][a11y]"),
            "a control with a readable text label already announces itself: {:?}",
            azul_core::diagnostics::recorded()
        );
        azul_core::diagnostics::clear();
    }

    extern "C" fn noop_cb(
        _: azul_core::refany::RefAny,
        _: crate::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        azul_core::callbacks::Update::DoNothing
    }
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
                if t.as_str().chars().any(|ch| {
                    ch.is_alphanumeric() && !('\u{e000}'..='\u{f8ff}').contains(&ch)
                }) {
                    return true;
                }
            }
        }
        if let Some(item) = hierarchy.get(cur) {
            if let Some(first) = item.first_child_id(cur) {
                let mut sib = Some(first);
                while let Some(sid) = sib {
                    stack.push(sid);
                    sib = hierarchy.get(sid).and_then(NodeHierarchyItem::next_sibling_id);
                }
            }
        }
    }
    false
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
    if lint_suppressed(A11Y_SHAPE_SUPPRESS_TAG) {
        return;
    }

    use azul_core::a11y::{AccessibilityRole, AccessibilityState};

    let nodes = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut reported = 0usize;

    for (node_id, node) in nodes.linear_iter().filter_map(|id| nodes.get(id).map(|n| (id, n))) {
        if reported >= 8 {
            break; // one screenful is enough to act on
        }
        let idx = node_id.index();
        let interactive = !node.get_callbacks().is_empty();
        let focusable = node.get_tab_index().is_some();

        let Some(info) = node.accessibility.as_ref() else {
            // The no-a11y case is handled by warn_interactive_without_accessibility,
            // which already reports interactive nodes. Here, add the shapes that
            // are worth naming even when they are not clickable.
            if matches!(node.get_node_type(), NodeType::Image(_)) {
                reported += 1;
                azul_core::diagnostics::emit(format!(
                    "[azul][a11y-shape] node {idx} is an IMAGE with no accessibility \
                     info, so it is absent from the accessibility tree entirely — a \
                     screen reader announces nothing where a sighted user sees a \
                     picture. Give it .with_accessibility_info(AccessibilityInfo {{ \
                     accessibility_name: Some(\"what it shows\".into()).into(), \
                     role: AccessibilityRole::Graphic, ..Default::default() }}), or \
                     mark it decorative by naming it explicitly as such. \
                     (suppress with AZ_SUPPRESS={A11Y_SHAPE_SUPPRESS_TAG})"
                ));
            } else if focusable {
                reported += 1;
                azul_core::diagnostics::emit(format!(
                    "[azul][a11y-shape] node {idx} is KEYBOARD-FOCUSABLE (it has a \
                     tab_index) but declares no accessibility info, so tabbing lands \
                     on something the screen reader cannot describe. At minimum give \
                     it a `role` and an `accessibility_name`. \
                     (suppress with AZ_SUPPRESS={A11Y_SHAPE_SUPPRESS_TAG})"
                ));
            }
            continue;
        };

        // It DECLARED accessibility — now check that the declaration matches the
        // shape. A half-filled AccessibilityInfo is the subtler failure: the node
        // is in the tree, so nothing looks broken, and it still announces wrongly.
        let role = info.role;
        let has_name = info.accessibility_name.as_ref().is_some();
        let has_value = info.accessibility_value.as_ref().is_some();

        if matches!(role, AccessibilityRole::Unknown) && (interactive || focusable) {
            reported += 1;
            azul_core::diagnostics::emit(format!(
                "[azul][a11y-shape] node {idx} is interactive and declares \
                 accessibility, but its `role` is Unknown — a screen reader can say \
                 its name and not what it IS. Pick from AccessibilityRole \
                 (PushButton, CheckBox, ComboBox, Slider, Link, Tab, MenuItem, …). \
                 (suppress with AZ_SUPPRESS={A11Y_SHAPE_SUPPRESS_TAG})"
            ));
            continue;
        }

        if matches!(
            role,
            AccessibilityRole::Slider | AccessibilityRole::ProgressBar | AccessibilityRole::ScrollBar
        ) && !has_value
        {
            reported += 1;
            azul_core::diagnostics::emit(format!(
                "[azul][a11y-shape] node {idx} has role {role:?} but no \
                 `accessibility_value`. The value IS the content of these controls \
                 — without it a screen reader announces \"slider\" and never how far \
                 along it is. Set accessibility_value on every change, not once. \
                 (suppress with AZ_SUPPRESS={A11Y_SHAPE_SUPPRESS_TAG})"
            ));
            continue;
        }

        if matches!(role, AccessibilityRole::CheckButton | AccessibilityRole::RadioButton) {
            let states = info.states.as_ref();
            let declares_checked = states.iter().any(|s| {
                matches!(s, AccessibilityState::CheckedTrue | AccessibilityState::CheckedFalse | AccessibilityState::Selected)
            });
            if !declares_checked {
                reported += 1;
                azul_core::diagnostics::emit(format!(
                    "[azul][a11y-shape] node {idx} has role {role:?} but declares no \
                     CheckedTrue/CheckedFalse/Selected state, so it always announces as unchecked no \
                     matter what it renders. Push the state into \
                     AccessibilityInfo::states when the control toggles. \
                     (suppress with AZ_SUPPRESS={A11Y_SHAPE_SUPPRESS_TAG})"
                ));
                continue;
            }
        }

        // A control whose visible label is a text node DOES announce itself —
        // azul derives the name from that text. Reporting those was a false
        // positive on every correctly-built labelled checkbox and slider.
        if !has_name
            && info.labelled_by.as_ref().is_none()
            && !has_readable_text_label(styled_dom, node_id)
        {
            reported += 1;
            azul_core::diagnostics::emit(format!(
                "[azul][a11y-shape] node {idx} declares accessibility (role \
                 {role:?}) but has neither an `accessibility_name` nor a \
                 `labelled_by` pointing at the node that names it. It is in the \
                 tree and anonymous. Use labelled_by when a separate label element \
                 already carries the text — that keeps the two from drifting apart. \
                 (suppress with AZ_SUPPRESS={A11Y_SHAPE_SUPPRESS_TAG})"
            ));
        }
    }
}
