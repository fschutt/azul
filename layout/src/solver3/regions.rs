//! CSS Regions 1: named flows laid out through chains of regions.
//!
//! An element with `flow-into: <name>` (or the content of one with
//! `flow-into: <name> content`) is taken out of its parent's flow and goes
//! into the named flow `<name>`, in document order. Every block container
//! with `flow-from: <name>` is a region of that flow: the regions, in
//! document order, form the flow's region chain, and the flow's content is
//! laid out through it as through a chain of fragmentainers (CSS
//! Fragmentation 3). Each region has its own width, its own `shape-inside`
//! and its own floats, and a paragraph that does not fit one region goes on
//! in the next one with the same line breaker, hyphenation, justification
//! and inline state.
//!
//! The box tree holds the flow's content as the children of the flow's
//! first region ([`flow_children`]): the main pass lays it out there, at that
//! region's width, as one block formatting context, and every cache, measure
//! and box the content needs exists afterwards. [`lay_out_named_flows`] then
//! runs once the boxes of the document are placed, the regions' own
//! positions and sizes known, and lays the content out again through the
//! whole chain. It leaves on every box of the flow where its parts went
//! ([`BoxFragment`]) and on every region what it shows ([`RegionContent`]);
//! the display list paints each region's part of the flow in that region's
//! place in the painting order (a region is a stacking context). What does
//! not fit the last region runs on below it and is reported, never hidden
//! ([`FlowReport::overflow`]).

use alloc::{collections::BTreeMap, collections::BTreeSet, string::String, sync::Arc, vec::Vec};
use core::cell::RefCell;
use std::collections::HashMap;

use azul_core::{
    dom::{FormattingContext, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    styled_dom::StyledDom,
};
use azul_css::props::layout::{
    fragmentation::{BreakInside, PageBreak},
    FlowFrom, FlowInto, LayoutDisplay, LayoutFloat, LayoutPosition, LayoutWritingMode,
    ShapeInside,
};

use crate::{
    font_traits::{ParsedFontTrait, TextLayoutCache},
    solver3::{
        fc::{self, FloatingContext, LayoutConstraints, TextAlign},
        geometry::{ContainingBlock, EdgeSizes, WritingModeContext},
        getters,
        layout_tree::{
            get_display_type, is_replaced_element, CachedInlineLayout, LayoutNodeId, LayoutTree,
        },
        positioning::get_position_type,
        LayoutContext, PositionVec,
    },
    text3::cache::{
        AvailableSpace, Exclusion, ExclusionWrap, FragmentLimits, ParagraphFlow, Point,
        PositionedItem, Rect, ShapeBoundary, ShapedItem, UnifiedLayout,
    },
};

// ---------------------------------------------------------------------------
// The named flows of a document
// ---------------------------------------------------------------------------

/// Whether any element of the document declares `flow-into` or `flow-from`:
/// without one no box is in a named flow and no box is a region, and none of
/// this module's work is done.
#[must_use]
pub fn document_has_named_flows(styled_dom: &StyledDom) -> bool {
    use azul_css::compact_cache::{DOM_HAS_FLOW_FROM, DOM_HAS_FLOW_INTO};
    styled_dom
        .css_property_cache
        .ptr
        .compact_cache
        .as_ref()
        .is_none_or(|cc| cc.dom_declared_flags & (DOM_HAS_FLOW_INTO | DOM_HAS_FLOW_FROM) != 0)
}

/// The `flow-into` of `id`.
fn flow_into(styled_dom: &StyledDom, id: NodeId) -> Option<FlowInto> {
    let nodes = styled_dom.node_data.as_container();
    let node_data = nodes.get(id)?;
    let styled_nodes = styled_dom.styled_nodes.as_container();
    let state = &styled_nodes.get(id)?.styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_flow_into(node_data, &id, state)?
        .get_property()
        .cloned()
}

/// The name of the flow `id` would be a region of (`flow-from`).
fn flow_from(styled_dom: &StyledDom, id: NodeId) -> Option<String> {
    let nodes = styled_dom.node_data.as_container();
    let node_data = nodes.get(id)?;
    let styled_nodes = styled_dom.styled_nodes.as_container();
    let state = &styled_nodes.get(id)?.styled_node_state;
    match styled_dom
        .css_property_cache
        .ptr
        .get_flow_from(node_data, &id, state)?
        .get_property()?
    {
        FlowFrom::Named(name) => Some(name.as_str().into()),
        FlowFrom::None => None,
    }
}

/// Whether `id` can be a region: a non-replaced block container (CSS
/// Regions 1, 3.2).
fn is_block_container(styled_dom: &StyledDom, id: NodeId) -> bool {
    let nodes = styled_dom.node_data.as_container();
    let Some(node_data) = nodes.get(id) else {
        return false;
    };
    !is_replaced_element(node_data)
        && matches!(
            get_display_type(styled_dom, id),
            LayoutDisplay::Block
                | LayoutDisplay::FlowRoot
                | LayoutDisplay::InlineBlock
                | LayoutDisplay::ListItem
                | LayoutDisplay::InlineBlockListItem
                | LayoutDisplay::TableCell
                | LayoutDisplay::TableCaption
        )
}

/// One named flow of a document: the elements in it and its region chain.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DomNamedFlow {
    /// The flow's name.
    pub name: String,
    /// The nodes in the flow, in document order: the elements with
    /// `flow-into: <name>` and the children of those with `flow-into: <name>
    /// content`.
    pub content: Vec<NodeId>,
    /// Its regions, in document order.
    pub regions: Vec<NodeId>,
}

/// The named flows of a document, in the order their names first appear.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NamedFlowIndex {
    /// The flows.
    pub flows: Vec<DomNamedFlow>,
    /// Which flow each region is a region of.
    region_of: BTreeMap<NodeId, usize>,
    /// Every node that went into a flow, out of its parent's flow.
    moved: BTreeSet<NodeId>,
}

impl NamedFlowIndex {
    /// The named flows of `styled_dom`.
    ///
    /// The walk goes in document order. A `display: none` subtree takes part
    /// in nothing. The children of a region are not formatted unless they go
    /// into a flow, so a `flow-from` among them makes no region. A box in
    /// the content of a flow is no region of that same flow (the flow would
    /// flow into itself), and a region whose flow would, through the regions
    /// in its content, flow back into the flow that holds it is dropped too.
    #[must_use]
    pub fn of(styled_dom: &StyledDom) -> Self {
        struct Visit {
            node: NodeId,
            /// The flow the node's subtree is in, if any.
            within: Option<usize>,
            /// Whether it is the content of a region, not formatted.
            hidden: bool,
            /// The flow the node goes into as a child of a `content` element.
            content_of: Option<usize>,
        }
        let mut index = Self::default();
        if !document_has_named_flows(styled_dom) {
            return index;
        }
        let hierarchy = styled_dom.node_hierarchy.as_container();
        let root = styled_dom
            .root
            .into_crate_internal()
            .unwrap_or(NodeId::ZERO);
        // The flow each region's nearest content node is in, for the cycle
        // check below: (region, its flow, the flow it sits in).
        let mut nested_regions: Vec<(NodeId, usize, usize)> = Vec::new();
        let mut stack = alloc::vec![Visit {
            node: root,
            within: None,
            hidden: false,
            content_of: None,
        }];
        while let Some(visit) = stack.pop() {
            if hierarchy.get(visit.node).is_none()
                || get_display_type(styled_dom, visit.node) == LayoutDisplay::None
            {
                continue;
            }
            let mut within = visit.within;
            let mut hidden = visit.hidden;
            let mut children_content_of = None;
            match flow_into(styled_dom, visit.node) {
                Some(FlowInto::Named(name)) => {
                    let flow = index.flow_named(name.as_str());
                    index.flows[flow].content.push(visit.node);
                    index.moved.insert(visit.node);
                    within = Some(flow);
                    hidden = false;
                }
                Some(FlowInto::NamedContent(name)) => {
                    children_content_of = Some(index.flow_named(name.as_str()));
                }
                _ => {
                    if let Some(flow) = visit.content_of {
                        index.flows[flow].content.push(visit.node);
                        index.moved.insert(visit.node);
                        within = Some(flow);
                        hidden = false;
                    }
                }
            }
            if !hidden {
                if let Some(name) = flow_from(styled_dom, visit.node) {
                    let flow = index.flow_named(&name);
                    if within != Some(flow) && is_block_container(styled_dom, visit.node) {
                        index.flows[flow].regions.push(visit.node);
                        index.region_of.insert(visit.node, flow);
                        if let Some(outer) = within {
                            nested_regions.push((visit.node, flow, outer));
                        }
                        hidden = true;
                    }
                }
            }
            let children: Vec<NodeId> = visit.node.az_children(&hierarchy).collect();
            for &child in children.iter().rev() {
                stack.push(Visit {
                    node: child,
                    within,
                    hidden,
                    content_of: children_content_of,
                });
            }
        }
        index.drop_cyclic_regions(&nested_regions);
        index
    }

    /// The index of the flow called `name`, a new one if there is none yet.
    fn flow_named(&mut self, name: &str) -> usize {
        if let Some(flow) = self.flows.iter().position(|f| f.name == name) {
            flow
        } else {
            self.flows.push(DomNamedFlow {
                name: name.into(),
                content: Vec::new(),
                regions: Vec::new(),
            });
            self.flows.len() - 1
        }
    }

    /// Drops the regions that make a flow hold itself: a region of flow `b`
    /// in the content of flow `a` lays `b` out inside `a`, and when `b`'s
    /// content holds a region of `a` the boxes would hold each other.
    fn drop_cyclic_regions(&mut self, nested_regions: &[(NodeId, usize, usize)]) {
        // `a -> b`: a region of `b` lies in the content of `a`.
        let mut edges: Vec<(usize, usize, NodeId)> = nested_regions
            .iter()
            .map(|&(region, flow, outer)| (outer, flow, region))
            .collect();
        loop {
            let reaches = |from: usize, to: usize, edges: &[(usize, usize, NodeId)]| {
                let mut seen = BTreeSet::new();
                let mut todo = alloc::vec![from];
                while let Some(flow) = todo.pop() {
                    if flow == to {
                        return true;
                    }
                    if seen.insert(flow) {
                        todo.extend(edges.iter().filter(|e| e.0 == flow).map(|e| e.1));
                    }
                }
                false
            };
            let Some(cyclic) = edges
                .iter()
                .position(|&(outer, flow, _)| reaches(flow, outer, &edges))
            else {
                break;
            };
            let (_, flow, region) = edges.remove(cyclic);
            self.flows[flow].regions.retain(|&r| r != region);
            self.region_of.remove(&region);
        }
    }

    /// The flow `id` is a region of.
    #[must_use]
    pub fn region_flow(&self, id: NodeId) -> Option<&DomNamedFlow> {
        self.region_of.get(&id).map(|&flow| &self.flows[flow])
    }

    /// Whether `id` went into a named flow, out of its parent's flow.
    #[must_use]
    pub fn is_in_a_flow(&self, id: NodeId) -> bool {
        self.moved.contains(&id)
    }
}

thread_local! {
    /// The named flows of the document whose box tree is being built, while
    /// a [`NamedFlowScope`] for it is alive: the address of its `StyledDom`
    /// and its index.
    static FLOW_SCOPE: RefCell<Option<(usize, Arc<NamedFlowIndex>)>> = const { RefCell::new(None) };
}

/// Keeps the named flows of one document at hand while its box tree is
/// built, so the tree builder's many questions about children cost no walk
/// of the document each.
///
/// The scope borrows the `StyledDom` for as long as it lives: only while that
/// document cannot go away (and no other can take its address) is its
/// address a key for its flows. A document asked about outside its scope has
/// its flows found again ([`flow_index`]).
pub(crate) struct NamedFlowScope<'a> {
    previous: Option<(usize, Arc<NamedFlowIndex>)>,
    _document: core::marker::PhantomData<&'a StyledDom>,
}

impl<'a> NamedFlowScope<'a> {
    /// Keeps the flows of `styled_dom` at hand until the scope is dropped.
    pub(crate) fn enter(styled_dom: &'a StyledDom) -> Self {
        let entry = document_has_named_flows(styled_dom).then(|| {
            (
                core::ptr::from_ref(styled_dom) as usize,
                Arc::new(NamedFlowIndex::of(styled_dom)),
            )
        });
        let previous = FLOW_SCOPE.with(|scope| scope.replace(entry));
        Self {
            previous,
            _document: core::marker::PhantomData,
        }
    }
}

impl Drop for NamedFlowScope<'_> {
    fn drop(&mut self) {
        let previous = self.previous.take();
        FLOW_SCOPE.with(|scope| *scope.borrow_mut() = previous);
    }
}

/// The named flows of `styled_dom`, `None` when it has none.
#[must_use]
pub(crate) fn flow_index(styled_dom: &StyledDom) -> Option<Arc<NamedFlowIndex>> {
    if !document_has_named_flows(styled_dom) {
        return None;
    }
    let address = core::ptr::from_ref(styled_dom) as usize;
    let scoped = FLOW_SCOPE.with(|scope| {
        scope
            .borrow()
            .as_ref()
            .filter(|(key, _)| *key == address)
            .map(|(_, index)| Arc::clone(index))
    });
    Some(scoped.unwrap_or_else(|| Arc::new(NamedFlowIndex::of(styled_dom))))
}

/// The children `parent` lays out, given its own (`children`), in a document
/// with named flows: a region lays out the content of its flow (the first
/// region of the chain holds it for the box tree, the others hold none),
/// never its own children; a box lays out none of its children that went
/// into a flow.
#[must_use]
pub(crate) fn flow_children(
    styled_dom: &StyledDom,
    parent: NodeId,
    mut children: Vec<NodeId>,
) -> Vec<NodeId> {
    let Some(index) = flow_index(styled_dom) else {
        return children;
    };
    if let Some(flow) = index.region_flow(parent) {
        return if flow.regions.first() == Some(&parent) {
            flow.content.clone()
        } else {
            Vec::new()
        };
    }
    children.retain(|child| !index.is_in_a_flow(*child));
    children
}

/// Whether `id` is a region: it establishes a block formatting context of
/// block-level boxes, its inline content wrapped in anonymous blocks.
#[must_use]
pub(crate) fn is_region(styled_dom: &StyledDom, id: NodeId) -> bool {
    flow_index(styled_dom).is_some_and(|index| index.region_flow(id).is_some())
}

// ---------------------------------------------------------------------------
// What the pass leaves on the boxes
// ---------------------------------------------------------------------------

/// Where the boxes of named flows went, kept on their layout nodes: the
/// boxes of a flow and the regions hold one, and the root box holds the
/// document's reports ([`LayoutTree::named_flows`]).
#[derive(Debug, Clone, Default)]
pub struct FlowData {
    /// The parts of this box of a named flow, one per region it reaches, in
    /// the order of the chain.
    pub fragments: Vec<BoxFragment>,
    /// What this box shows as a region.
    pub region: Option<RegionContent>,
    /// On the root box: what each named flow of the document came to.
    pub reports: Vec<FlowReport>,
}

impl LayoutTree {
    /// What each named flow of the laid-out document came to, in the order
    /// the flows' names first appear: where its content went, region by
    /// region, and what did not fit.
    #[must_use]
    pub fn named_flows(&self) -> &[FlowReport] {
        self.warm(LayoutNodeId::new(self.root))
            .and_then(|w| w.flow.as_deref())
            .map_or(&[], |flow| flow.reports.as_slice())
    }
}

/// The part of a box of a named flow that one region holds.
#[derive(Debug, Clone)]
pub struct BoxFragment {
    /// The region's layout node.
    pub region: usize,
    /// The part's border box, in the document's coordinates. A box that goes
    /// on in the next region fills this one to its end and has no bottom
    /// edge here, and the part that goes on has no top edge (CSS
    /// Fragmentation 3, `box-decoration-break: slice`).
    pub rect: LogicalRect,
    /// Whether the box starts in this region.
    pub starts: bool,
    /// Whether the box ends in this region.
    pub ends: bool,
    /// Whether the box is in this region whole, its subtree with it: it is
    /// painted from its own place, nothing of it is anywhere else.
    pub whole: bool,
    /// The lines of the box's inline formatting context in this region,
    /// positioned from the content box of `rect`.
    pub lines: Option<Arc<CachedInlineLayout>>,
}

/// What a region shows of its flow.
#[derive(Debug, Clone, Default)]
pub struct RegionContent {
    /// The index of the flow's report in [`LayoutTree::named_flows`].
    pub flow: usize,
    /// The layout nodes of the flow's content, in order: the children of
    /// the flow's first region.
    pub content: Vec<usize>,
}

/// What a named flow came to: where its content went and what was left.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlowReport {
    /// The flow's name.
    pub name: String,
    /// Its regions, in the order of the chain, with what each holds.
    pub regions: Vec<RegionReport>,
    /// What did not fit the last region, if anything.
    pub overflow: Option<FlowOverflow>,
}

/// What one region of a flow holds.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionReport {
    /// The region's layout node.
    pub region: usize,
    /// The region's DOM node.
    pub dom_node: Option<NodeId>,
    /// The region's border box.
    pub border_box: LogicalRect,
    /// The paragraphs whose lines it holds, in order.
    pub paragraphs: Vec<ParagraphReport>,
    /// The boxes placed whole in it (pictures, tables, boxes that do not
    /// break), in order.
    pub boxes: Vec<usize>,
}

/// The lines of one paragraph in one region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParagraphReport {
    /// The paragraph's layout node.
    pub node: usize,
    /// Its DOM node (an anonymous paragraph has none).
    pub dom_node: Option<NodeId>,
    /// The index of its first line here, counted through the whole paragraph.
    pub first_line: usize,
    /// How many of its lines are here.
    pub lines: usize,
}

/// The content of a flow left over after its last region.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowOverflow {
    /// The layout node of the first box with content left (`None` for a
    /// flow without regions, whose content has no boxes).
    pub node: Option<usize>,
    /// Its DOM node.
    pub dom_node: Option<NodeId>,
    /// The text node the first text left is in, if that box is a paragraph.
    pub text_node: Option<NodeId>,
    /// The byte offset of that text in its text node, as laid out (after
    /// white space processing).
    pub byte_offset: Option<usize>,
    /// The last region of the chain, `None` for a flow without regions.
    pub last_region: Option<usize>,
    /// How much taller the last region would have to be to hold the rest, in
    /// px.
    pub needed_height: f32,
}

// ---------------------------------------------------------------------------
// The pass
// ---------------------------------------------------------------------------

/// One region of a chain as the pass sees it.
#[derive(Debug, Clone)]
struct Fragmentainer {
    /// The region's layout node.
    node: usize,
    /// Its content box.
    content: LogicalRect,
    /// Its `shape-inside`, in its content box's coordinates.
    shape: Option<ShapeBoundary>,
    /// The floats placed in it, in its content box's coordinates.
    floats: Vec<Exclusion>,
}

/// The inline room the boxes around a box take in a region: the margins,
/// borders and paddings of the split boxes it is in.
#[derive(Debug, Clone, Copy, Default)]
struct Inset {
    start: f32,
    end: f32,
}

/// Collapsed margins, positive and negative apart (CSS 2.2 8.3.1).
#[derive(Debug, Clone, Copy, Default)]
struct Margins {
    positive: f32,
    negative: f32,
}

impl Margins {
    fn add(&mut self, margin: f32) {
        if margin >= 0.0 {
            self.positive = self.positive.max(margin);
        } else {
            self.negative = self.negative.min(margin);
        }
    }

    fn take(&mut self) -> f32 {
        let collapsed = self.positive + self.negative;
        *self = Self::default();
        collapsed
    }
}

/// How a box takes part in the flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// It is laid out elsewhere (an absolutely positioned box) or not at all.
    Skip,
    /// Its lines break across regions.
    Paragraph,
    /// It breaks between its children.
    Container,
    /// It is placed whole beside the content, which wraps around it.
    Float,
    /// It is placed whole.
    Monolith,
}

/// The state of one flow's walk through its chain.
struct FlowWalk<'r> {
    chain: Vec<Fragmentainer>,
    /// The region the walk is in.
    region: usize,
    /// The block offset in it.
    pen: f32,
    /// The margins not yet placed.
    margins: Margins,
    /// Whether the region holds content (a break before more content then
    /// makes progress).
    holds_content: bool,
    /// Whether the region was entered by an unforced break and holds nothing
    /// yet: the margins adjoining that break truncate to zero, where those
    /// after a forced break stay (CSS Fragmentation 3, 5.2).
    truncates_margins: bool,
    /// A forced break the last box asked for after it.
    break_pending: bool,
    /// The first box whose content runs past the end of the chain's last
    /// region.
    first_overflowing: Option<usize>,
    /// The region reports, one per region of the chain.
    reports: &'r mut Vec<RegionReport>,
    /// What did not fit the last region.
    overflow: Option<FlowOverflow>,
    /// The parts of every box placed so far.
    fragments: BTreeMap<usize, Vec<BoxFragment>>,
}

/// Lays the named flows of the document out through their region chains,
/// once every box of it is placed (`positions` absolute), and records where
/// their content went: [`FlowData`] on the boxes, the reports on the root
/// box ([`LayoutTree::named_flows`]).
pub fn lay_out_named_flows<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    positions: &mut PositionVec,
) {
    for warm in &mut tree.warm {
        warm.flow = None;
    }
    let Some(index) = flow_index(ctx.styled_dom) else {
        return;
    };
    let mut reports = Vec::new();
    for dom_flow in &index.flows {
        let report_index = reports.len();
        reports.push(lay_out_named_flow(
            ctx,
            tree,
            text_cache,
            positions,
            dom_flow,
            report_index,
        ));
    }
    if let Some(warm) = tree.warm_mut(LayoutNodeId::new(tree.root)) {
        warm.flow.get_or_insert_with(Box::default).reports = reports;
    }
}

/// Lays one named flow out through its region chain and records where its
/// content went; its report is the `report_index`th of the document.
fn lay_out_named_flow<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    positions: &mut PositionVec,
    dom_flow: &DomNamedFlow,
    report_index: usize,
) -> FlowReport {
    let mut report = FlowReport {
        name: dom_flow.name.clone(),
        ..FlowReport::default()
    };
    let regions: Vec<usize> = dom_flow
        .regions
        .iter()
        .filter_map(|dom| tree.dom_to_layout.get(dom)?.first().map(|id| id.index()))
        .collect();
    let Some(&first) = regions.first() else {
        // Without a region the content has no box at all: all of it is left.
        report.overflow = dom_flow.content.first().map(|&dom| FlowOverflow {
            node: None,
            dom_node: Some(dom),
            text_node: None,
            byte_offset: None,
            last_region: None,
            needed_height: 0.0,
        });
        return report;
    };
    let content: Vec<usize> = tree.children(first).to_vec();
    let chain: Vec<Fragmentainer> = regions
        .iter()
        .map(|&region| fragmentainer(ctx, tree, positions, region))
        .collect();
    report.regions = chain
        .iter()
        .map(|f| RegionReport {
            region: f.node,
            dom_node: tree.get(LayoutNodeId::new(f.node)).and_then(|n| n.dom_node_id),
            border_box: border_box_of(tree, positions, f.node),
            paragraphs: Vec::new(),
            boxes: Vec::new(),
        })
        .collect();
    let mut walk = FlowWalk {
        chain,
        region: 0,
        pen: 0.0,
        margins: Margins::default(),
        holds_content: false,
        truncates_margins: false,
        break_pending: false,
        first_overflowing: None,
        reports: &mut report.regions,
        overflow: None,
        fragments: BTreeMap::new(),
    };
    for &child in &content {
        walk.place(ctx, tree, text_cache, positions, child, Inset::default());
    }
    walk.finish(tree);
    let fragments = core::mem::take(&mut walk.fragments);
    let overflow = walk.overflow.take();
    let chain = core::mem::take(&mut walk.chain);
    drop(walk);
    report.overflow = overflow;
    for (node, parts) in fragments {
        if let Some(warm) = tree.warm_mut(LayoutNodeId::new(node)) {
            warm.flow.get_or_insert_with(Box::default).fragments = parts;
        }
    }
    for region in &chain {
        if let Some(warm) = tree.warm_mut(LayoutNodeId::new(region.node)) {
            warm.flow.get_or_insert_with(Box::default).region = Some(RegionContent {
                flow: report_index,
                content: content.clone(),
            });
        }
    }
    report
}

/// The border box of `node` as the main pass placed it.
fn border_box_of(tree: &LayoutTree, positions: &PositionVec, node: usize) -> LogicalRect {
    LogicalRect::new(
        super::pos_get(positions, node).unwrap_or_default(),
        tree.get(LayoutNodeId::new(node))
            .and_then(|n| n.used_size)
            .unwrap_or_default(),
    )
}

/// The region `node` as a fragmentainer: its content box and its
/// `shape-inside`, resolved against that box.
fn fragmentainer<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    tree: &LayoutTree,
    positions: &PositionVec,
    node: usize,
) -> Fragmentainer {
    let border_box = border_box_of(tree, positions, node);
    let bp = tree
        .get(LayoutNodeId::new(node))
        .map(|n| n.box_props.unpack())
        .unwrap_or_default();
    let edges = Edges::of(&bp);
    let content = LogicalRect::new(
        LogicalPosition::new(
            border_box.origin.x + edges.left,
            border_box.origin.y + edges.top,
        ),
        LogicalSize::new(
            (border_box.size.width - edges.inline()).max(0.0),
            (border_box.size.height - edges.top - edges.bottom).max(0.0),
        ),
    );
    let shape = tree
        .get(LayoutNodeId::new(node))
        .and_then(|n| n.dom_node_id)
        .and_then(|dom| region_shape(ctx, dom, content.size));
    Fragmentainer {
        node,
        content,
        shape,
        floats: Vec::new(),
    }
}

/// The `shape-inside` of the region `dom`, resolved against its content box
/// of `size` (its reference box), in that box's coordinates.
fn region_shape<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    dom: NodeId,
    size: LogicalSize,
) -> Option<ShapeBoundary> {
    let styled_dom = ctx.styled_dom;
    let nodes = styled_dom.node_data.as_container();
    let node_data = nodes.get(dom)?;
    let styled_nodes = styled_dom.styled_nodes.as_container();
    let state = &styled_nodes.get(dom)?.styled_node_state;
    let ShapeInside::Shape(shape) = styled_dom
        .css_property_cache
        .ptr
        .get_shape_inside(node_data, &dom, state)?
        .get_property()?
    else {
        return None;
    };
    let reference = azul_css::shape::ShapeReference {
        width: size.width,
        height: size.height,
        font_size: getters::get_element_font_size(styled_dom, dom, state),
        root_font_size: crate::solver3::layout_tree::get_root_font_size(styled_dom),
        viewport_width: ctx.viewport_size.width,
        viewport_height: ctx.viewport_size.height,
    };
    Some(ShapeBoundary::from_css_shape(
        shape,
        Point::default(),
        &reference,
        &mut None,
    ))
}

/// How the box `node` takes part in its flow.
fn kind_of(styled_dom: &StyledDom, tree: &LayoutTree, node: usize) -> Kind {
    let Some(hot) = tree.get(LayoutNodeId::new(node)) else {
        return Kind::Skip;
    };
    if matches!(
        get_position_type(styled_dom, hot.dom_node_id),
        LayoutPosition::Absolute | LayoutPosition::Fixed
    ) || matches!(hot.formatting_context, FormattingContext::None)
    {
        return Kind::Skip;
    }
    let style = tree.warm(LayoutNodeId::new(node)).map(|w| &w.computed_style);
    if style.is_some_and(|s| s.float != LayoutFloat::None) {
        return Kind::Float;
    }
    // Only a box as tall as its content breaks: a box of a fixed height
    // holds what it holds.
    let auto_height = style.is_none_or(|s| s.height.is_none());
    let replaced = hot.dom_node_id.is_some_and(|dom| {
        let nodes = styled_dom.node_data.as_container();
        nodes.get(dom).is_some_and(is_replaced_element)
    });
    match hot.formatting_context {
        FormattingContext::Inline if auto_height && !replaced => Kind::Paragraph,
        FormattingContext::Block { .. } if auto_height && !replaced => Kind::Container,
        _ => Kind::Monolith,
    }
}

/// Whether `value` forces a break before or after a box in a flow: a page
/// break is a region break too.
const fn forces_a_break(value: PageBreak) -> bool {
    getters::is_forced_page_break(value) || matches!(value, PageBreak::Region)
}

impl FlowWalk<'_> {
    /// The region the walk is in.
    fn here(&self) -> &Fragmentainer {
        &self.chain[self.region]
    }

    /// Whether the walk is in the chain's last region.
    const fn in_last_region(&self) -> bool {
        self.region + 1 >= self.chain.len()
    }

    /// The block size the region the walk is in gives its content: the last
    /// region of the chain is endless, what runs past its end is the flow's
    /// overflow.
    fn extent(&self) -> f32 {
        if self.in_last_region() {
            f32::INFINITY
        } else {
            self.here().content.size.height
        }
    }

    /// Goes on in the next region, `false` in the last one.
    fn next_region(&mut self, forced: bool) -> bool {
        if self.in_last_region() {
            return false;
        }
        self.region += 1;
        self.pen = 0.0;
        self.holds_content = false;
        self.truncates_margins = !forced;
        if !forced {
            self.margins = Margins::default();
        }
        true
    }

    /// Places the margins before the next content: right after an unforced
    /// break they are truncated.
    fn flush_margins(&mut self) {
        let collapsed = self.margins.take();
        if !(self.truncates_margins && !self.holds_content && self.pen == 0.0) {
            self.pen += collapsed;
        }
    }

    /// Notes that `node` put content into the region down to `bottom`.
    fn note_content(&mut self, node: usize, bottom: f32) {
        self.holds_content = true;
        self.truncates_margins = false;
        if self.in_last_region()
            && bottom > self.here().content.size.height + 0.01
            && self.first_overflowing.is_none()
        {
            self.first_overflowing = Some(node);
        }
    }

    /// Ends the walk: the content that ran past the end of the last region
    /// is the flow's overflow.
    fn finish(&mut self, tree: &LayoutTree) {
        let height = self.here().content.size.height;
        let past = self.pen - height;
        if !self.in_last_region() || past <= 0.01 {
            return;
        }
        if let Some(overflow) = &mut self.overflow {
            overflow.needed_height = past;
            return;
        }
        let node = self.first_overflowing;
        self.overflow = Some(FlowOverflow {
            node,
            dom_node: node
                .and_then(|n| tree.get(LayoutNodeId::new(n)))
                .and_then(|n| n.dom_node_id),
            text_node: None,
            byte_offset: None,
            last_region: Some(self.here().node),
            needed_height: past,
        });
    }

    /// Applies the break its previous sibling asked for after it and
    /// `break-before` of the box `dom`: a forced break moves the walk to the
    /// next region when this one holds content.
    fn break_before(&mut self, styled_dom: &StyledDom, dom: Option<NodeId>) {
        let forced =
            self.break_pending || forces_a_break(getters::get_break_before(styled_dom, dom));
        self.break_pending = false;
        if forced && self.holds_content {
            self.next_region(true);
        }
    }

    /// Lays the box `node` out at the walk's pen, `inset` from the region's
    /// edges.
    fn place<T: ParsedFontTrait>(
        &mut self,
        ctx: &mut LayoutContext<'_, T>,
        tree: &mut LayoutTree,
        text_cache: &mut TextLayoutCache,
        positions: &mut PositionVec,
        node: usize,
        inset: Inset,
    ) {
        let kind = kind_of(ctx.styled_dom, tree, node);
        if kind == Kind::Skip {
            return;
        }
        let dom = tree.get(LayoutNodeId::new(node)).and_then(|n| n.dom_node_id);
        self.break_before(ctx.styled_dom, dom);
        match kind {
            Kind::Paragraph => {
                if !self.place_paragraph(ctx, tree, text_cache, positions, node, inset) {
                    self.place_whole(ctx, tree, text_cache, positions, node, inset);
                }
            }
            Kind::Container => self.place_container(ctx, tree, text_cache, positions, node, inset),
            Kind::Float => self.place_float(ctx, tree, text_cache, positions, node, inset),
            Kind::Monolith | Kind::Skip => {
                self.place_whole(ctx, tree, text_cache, positions, node, inset);
            }
        }
        if forces_a_break(getters::get_break_after(ctx.styled_dom, dom)) {
            self.break_pending = true;
        }
    }

    /// The border-box width of `node` in a region `width` wide with
    /// `inset`, and its offset from the inset's start: a box of `width:
    /// auto` fills what its margins leave, any other keeps its width.
    fn box_width(tree: &LayoutTree, node: usize, width: f32, inset: Inset) -> (f32, f32) {
        let hot = tree.get(LayoutNodeId::new(node));
        let bp = hot.map(|n| n.box_props.unpack()).unwrap_or_default();
        let auto = tree
            .warm(LayoutNodeId::new(node))
            .is_none_or(|w| w.computed_style.width.is_none());
        let room = (width - inset.start - inset.end - bp.margin.left - bp.margin.right).max(0.0);
        let own = if auto {
            room
        } else {
            hot.and_then(|n| n.used_size).map_or(room, |s| s.width)
        };
        (own, bp.margin.left)
    }

    /// Lays a block container of block-level boxes out child by child,
    /// breaking between them, and records its part in every region it
    /// reaches.
    fn place_container<T: ParsedFontTrait>(
        &mut self,
        ctx: &mut LayoutContext<'_, T>,
        tree: &mut LayoutTree,
        text_cache: &mut TextLayoutCache,
        positions: &mut PositionVec,
        node: usize,
        inset: Inset,
    ) {
        let bp = tree
            .get(LayoutNodeId::new(node))
            .map(|n| n.box_props.unpack())
            .unwrap_or_default();
        let dom = tree.get(LayoutNodeId::new(node)).and_then(|n| n.dom_node_id);
        // A box that asks not to break goes to the next region whole when it
        // fits a fresh one and not what is left of this one; its height is
        // the one the main pass gave it.
        if matches!(
            getters::get_break_inside(ctx.styled_dom, dom),
            BreakInside::Avoid | BreakInside::AvoidRegion
        ) && self.holds_content
            && !self.in_last_region()
        {
            let height = tree
                .get(LayoutNodeId::new(node))
                .and_then(|n| n.used_size)
                .map_or(0.0, |s| s.height);
            let next = self.chain[self.region + 1].content.size.height;
            if self.pen + bp.margin.top + height > self.extent() + 0.01 && height <= next + 0.01 {
                self.next_region(false);
            }
        }
        self.margins.add(bp.margin.top);
        let edges = Edges::of(&bp);
        if edges.top > 0.0 {
            self.flush_margins();
        }
        let start = (self.region, self.pen);
        if edges.top > 0.0 {
            self.pen += edges.top;
            self.note_content(node, self.pen);
        }
        let (_, margin_start) = Self::box_width(tree, node, self.here().content.size.width, inset);
        let inner = Inset {
            start: inset.start + margin_start + edges.left,
            end: inset.end + bp.margin.right + edges.right,
        };
        let children = tree.children(node).to_vec();
        for child in children {
            self.place(ctx, tree, text_cache, positions, child, inner);
        }
        if edges.bottom > 0.0 {
            self.flush_margins();
            self.pen += edges.bottom;
            self.note_content(node, self.pen);
        }
        self.record_slices(tree, positions, node, inset, start);
        self.margins.add(bp.margin.bottom);
    }

    /// The part of the box `node` in region `region`: from where the box
    /// starts (`start`, its region and block offset) or the region's top,
    /// to the pen (in the walk's region) or the region's end (where it goes
    /// on).
    fn part_rect(
        &self,
        tree: &LayoutTree,
        node: usize,
        inset: Inset,
        region: usize,
        start: (usize, f32),
    ) -> LogicalRect {
        let content = self.chain[region].content;
        let (width, margin_start) = Self::box_width(tree, node, content.size.width, inset);
        let top = if region == start.0 { start.1 } else { 0.0 };
        let bottom = if region == self.region {
            self.pen
        } else {
            content.size.height
        };
        LogicalRect::new(
            LogicalPosition::new(
                content.origin.x + inset.start + margin_start,
                content.origin.y + top,
            ),
            LogicalSize::new(width, (bottom - top).max(0.0)),
        )
    }

    /// Records the parts of the block `node`, which started at `start` and
    /// ends at the walk's pen, and places the box at its first part.
    fn record_slices(
        &mut self,
        tree: &mut LayoutTree,
        positions: &mut PositionVec,
        node: usize,
        inset: Inset,
        start: (usize, f32),
    ) {
        let parts: Vec<BoxFragment> = (start.0..=self.region)
            .map(|region| BoxFragment {
                region: self.chain[region].node,
                rect: self.part_rect(tree, node, inset, region, start),
                starts: region == start.0,
                ends: region == self.region,
                whole: false,
                lines: None,
            })
            .collect();
        if let Some(first) = parts.first() {
            super::pos_set(positions, node, first.rect.origin);
            if let Some(hot) = tree.get_mut(LayoutNodeId::new(node)) {
                hot.used_size = Some(first.rect.size);
            }
        }
        self.fragments.insert(node, parts);
    }

    /// Lays a box out whole at the pen, in the first region from here on
    /// where it fits (or here, when no region would hold it), at that
    /// region's width.
    fn place_whole<T: ParsedFontTrait>(
        &mut self,
        ctx: &mut LayoutContext<'_, T>,
        tree: &mut LayoutTree,
        text_cache: &mut TextLayoutCache,
        positions: &mut PositionVec,
        node: usize,
        inset: Inset,
    ) {
        let bp = tree
            .get(LayoutNodeId::new(node))
            .map(|n| n.box_props.unpack())
            .unwrap_or_default();
        self.margins.add(bp.margin.top);
        self.flush_margins();
        let mut relaid_out = false;
        let (width, margin_start) = loop {
            let (width, margin_start) =
                Self::box_width(tree, node, self.here().content.size.width, inset);
            relaid_out |= lay_out_at_width(ctx, tree, text_cache, node, width);
            let height = tree
                .get(LayoutNodeId::new(node))
                .and_then(|n| n.used_size)
                .map_or(0.0, |s| s.height);
            let next = self
                .chain
                .get(self.region + 1)
                .map_or(0.0, |f| f.content.size.height);
            match crate::solver3::break_token::fragment_fit(
                self.pen,
                height,
                self.extent(),
                next,
                self.holds_content,
            ) {
                crate::solver3::break_token::FitDecision::BreakBeforeHere
                    if self.next_region(false) => {}
                _ => break (width, margin_start),
            }
        };
        let content = self.here().content;
        let to = LogicalPosition::new(
            content.origin.x + inset.start + margin_start,
            content.origin.y + self.pen,
        );
        place_box(tree, positions, node, to, relaid_out);
        let size = tree
            .get(LayoutNodeId::new(node))
            .and_then(|n| n.used_size)
            .unwrap_or_else(|| LogicalSize::new(width, 0.0));
        self.pen += size.height;
        self.note_content(node, self.pen);
        self.reports[self.region].boxes.push(node);
        self.fragments.insert(
            node,
            alloc::vec![BoxFragment {
                region: self.here().node,
                rect: LogicalRect::new(to, size),
                starts: true,
                ends: true,
                whole: true,
                lines: None,
            }],
        );
        self.margins.add(bp.margin.bottom);
    }

    /// Places a float at the pen, on its side of the region past the floats
    /// already there at that height, and makes it an exclusion the lines of
    /// this region wrap around. It takes no room in the block direction.
    fn place_float<T: ParsedFontTrait>(
        &mut self,
        ctx: &mut LayoutContext<'_, T>,
        tree: &mut LayoutTree,
        text_cache: &mut TextLayoutCache,
        positions: &mut PositionVec,
        node: usize,
        inset: Inset,
    ) {
        let Some(hot) = tree.get(LayoutNodeId::new(node)) else {
            return;
        };
        let bp = hot.box_props.unpack();
        let dom = hot.dom_node_id;
        let size = hot.used_size.unwrap_or_default();
        let side = tree
            .warm(LayoutNodeId::new(node))
            .map_or(LayoutFloat::Left, |w| w.computed_style.float);
        let relaid_out = lay_out_at_width(ctx, tree, text_cache, node, size.width);
        let size = tree
            .get(LayoutNodeId::new(node))
            .and_then(|n| n.used_size)
            .unwrap_or(size);
        // The float's top is the pen with the margins placed so far; they
        // stay pending for the content after it, which collapses with them.
        let top = self.pen + (self.margins.positive + self.margins.negative);
        let region = self.region;
        let content = self.here().content;
        let margin_box = LogicalSize::new(
            size.width + bp.margin.left + bp.margin.right,
            size.height + bp.margin.top + bp.margin.bottom,
        );
        // The floats already on this side at this height push it inwards.
        let beside: f32 = self
            .here()
            .floats
            .iter()
            .filter(|f| f.wrap == float_wrap(side))
            .filter_map(|f| f.clip)
            .filter(|clip| clip.y < top + margin_box.height && clip.y + clip.height > top)
            .map(|clip| clip.width)
            .sum();
        let x = if side == LayoutFloat::Right {
            content.size.width - inset.end - beside - margin_box.width
        } else {
            inset.start + beside
        };
        let border_box = LogicalRect::new(
            LogicalPosition::new(x + bp.margin.left, top + bp.margin.top),
            size,
        );
        let to = LogicalPosition::new(
            content.origin.x + border_box.origin.x,
            content.origin.y + border_box.origin.y,
        );
        place_box(tree, positions, node, to, relaid_out);
        let clip = Rect {
            x,
            y: top,
            width: margin_box.width,
            height: margin_box.height,
        };
        let (shape, margin) = dom
            .and_then(|dom| {
                fc::float_shape_outside(ctx, dom, border_box, &bp.margin, content.size.width)
            })
            .unwrap_or((ShapeBoundary::Rectangle(clip), 0.0));
        self.chain[region].floats.push(Exclusion {
            shape,
            margin,
            clip: Some(clip),
            wrap: float_wrap(side),
        });
        self.note_content(node, top);
        self.reports[region].boxes.push(node);
        self.fragments.insert(
            node,
            alloc::vec![BoxFragment {
                region: self.chain[region].node,
                rect: LogicalRect::new(to, size),
                starts: true,
                ends: true,
                whole: true,
                lines: None,
            }],
        );
    }

    /// Lays a paragraph out region by region. `false` when it has no inline
    /// content the pass can break (it is then placed whole).
    #[allow(clippy::too_many_lines)] // one walk: fit, orphans and widows, overflow
    fn place_paragraph<T: ParsedFontTrait>(
        &mut self,
        ctx: &mut LayoutContext<'_, T>,
        tree: &mut LayoutTree,
        text_cache: &mut TextLayoutCache,
        positions: &mut PositionVec,
        node: usize,
        inset: Inset,
    ) -> bool {
        let bp = tree
            .get(LayoutNodeId::new(node))
            .map(|n| n.box_props.unpack())
            .unwrap_or_default();
        let edges = Edges::of(&bp);
        let first_width = self.text_width(tree, node, inset, &edges, self.region);
        let Some(paragraph) = fc::flow_paragraph(ctx, text_cache, tree, node, first_width) else {
            return false;
        };
        let dom = tree.get(LayoutNodeId::new(node)).and_then(|n| n.dom_node_id);
        let orphans =
            usize::try_from(getters::get_orphans(ctx.styled_dom, dom).max(1)).unwrap_or(1);
        let widows = usize::try_from(getters::get_widows(ctx.styled_dom, dom).max(1)).unwrap_or(1);
        let avoid_inside = matches!(
            getters::get_break_inside(ctx.styled_dom, dom),
            BreakInside::Avoid | BreakInside::AvoidRegion
        );
        let fonts = ctx.font_manager.get_loaded_fonts();
        let mut flow = ParagraphFlow::new(
            Arc::clone(&paragraph.logical_items),
            Arc::clone(&paragraph.shaped_items),
            &paragraph.constraints,
        );

        self.margins.add(bp.margin.top);
        self.flush_margins();
        let mut start = (self.region, self.pen);
        self.pen += edges.top;

        let mut parts: Vec<Part> = Vec::new();
        let mut line_count = 0usize;
        loop {
            let origin = self.text_origin(tree, node, inset, &edges, self.region, self.pen);
            let constraints = self.fragment_constraints(
                self.region,
                &paragraph.constraints,
                self.text_width(tree, node, inset, &edges, self.region),
                origin,
                self.pen,
                edges.bottom,
            );
            let can_move_on = !self.in_last_region() && self.holds_content;
            let limits = FragmentLimits {
                keep_first_line: !can_move_on,
                max_lines: None,
            };
            let mut trial = flow.clone();
            let Ok(fragment) = trial.layout_fragment(&constraints, limits, &mut None, &fonts)
            else {
                return false;
            };
            // A paragraph that would break here moves to the next region
            // whole when it asks not to break, or when fewer than `orphans`
            // of its lines would stay here.
            if !trial.is_done()
                && flow.is_at_start()
                && can_move_on
                && (avoid_inside || fragment.lines < orphans)
            {
                self.next_region(false);
                self.flush_margins();
                start = (self.region, self.pen);
                self.pen += edges.top;
                continue;
            }
            if fragment.lines == 0 {
                if self.next_region(false) {
                    continue;
                }
                break;
            }
            // Widows: when the next region would end the paragraph with
            // fewer than `widows` lines, this region gives some of its own;
            // when it cannot without keeping fewer than `orphans`, the
            // paragraph goes to the next region whole, where it can.
            let mut lines_here = fragment.lines;
            let mut move_whole = false;
            if !trial.is_done() && !self.in_last_region() {
                let next = self.region + 1;
                let next_constraints = self.fragment_constraints(
                    next,
                    &paragraph.constraints,
                    self.text_width(tree, node, inset, &edges, next),
                    self.text_origin(tree, node, inset, &edges, next, 0.0),
                    0.0,
                    edges.bottom,
                );
                let mut probe = trial.clone();
                if let Ok(rest) = probe.layout_fragment(
                    &next_constraints,
                    FragmentLimits::default(),
                    &mut None,
                    &fonts,
                ) {
                    if probe.is_done() && rest.lines < widows {
                        let give = widows - rest.lines;
                        if lines_here > give && lines_here - give >= orphans {
                            lines_here -= give;
                        } else {
                            move_whole = flow.is_at_start() && can_move_on;
                        }
                    }
                }
            }
            if move_whole {
                self.next_region(false);
                self.flush_margins();
                start = (self.region, self.pen);
                self.pen += edges.top;
                continue;
            }
            // In the last region the lines past its end are the flow's
            // overflow: the first of them is where the overflow starts.
            if self.in_last_region() && self.overflow.is_none() {
                let mut room = constraints.clone();
                room.available_height =
                    Some((self.here().content.size.height - self.pen - edges.bottom).max(0.0));
                let mut probe = flow.clone();
                let limits = FragmentLimits {
                    keep_first_line: false,
                    max_lines: None,
                };
                if probe.layout_fragment(&room, limits, &mut None, &fonts).is_ok()
                    && !probe.is_done()
                {
                    self.paragraph_overflow(tree, node, &probe);
                }
            }
            let placed = if lines_here < fragment.lines {
                let limits = FragmentLimits {
                    keep_first_line: limits.keep_first_line,
                    max_lines: Some(lines_here),
                };
                match flow.layout_fragment(&constraints, limits, ctx.debug_messages, &fonts) {
                    Ok(placed) => placed,
                    Err(_) => return false,
                }
            } else {
                flow = trial;
                fragment
            };
            self.note_content(node, self.pen + placed.block_end);
            self.reports[self.region].paragraphs.push(ParagraphReport {
                node,
                dom_node: dom,
                first_line: line_count,
                lines: placed.lines,
            });
            parts.push(Part {
                region: self.region,
                origin,
                layout: placed.layout,
            });
            line_count += placed.lines;
            self.pen += placed.block_end;
            if flow.is_done() || !self.next_region(false) {
                break;
            }
        }
        self.pen += edges.bottom;
        self.record_paragraph(
            ctx.styled_dom,
            tree,
            positions,
            node,
            inset,
            &edges,
            start,
            &parts,
            &paragraph,
        );
        self.margins.add(bp.margin.bottom);
        true
    }

    /// The width a paragraph's lines have in region `region`.
    fn text_width(
        &self,
        tree: &LayoutTree,
        node: usize,
        inset: Inset,
        edges: &Edges,
        region: usize,
    ) -> f32 {
        let (width, _) = Self::box_width(tree, node, self.chain[region].content.size.width, inset);
        (width - edges.inline()).max(0.0)
    }

    /// Where a paragraph's lines start in region `region` at block offset
    /// `pen`: the top left corner of its content there.
    fn text_origin(
        &self,
        tree: &LayoutTree,
        node: usize,
        inset: Inset,
        edges: &Edges,
        region: usize,
        pen: f32,
    ) -> LogicalPosition {
        let content = self.chain[region].content;
        let (_, margin_start) = Self::box_width(tree, node, content.size.width, inset);
        LogicalPosition::new(
            content.origin.x + inset.start + margin_start + edges.left,
            content.origin.y + pen,
        )
    }

    /// The constraints of a paragraph's fragment in region `region`, its
    /// lines `text_width` wide from `origin`, `pen` down the region: the
    /// room left there (unbounded in the last region, which is endless) and
    /// the region's shape and floats, seen from the lines' origin.
    fn fragment_constraints(
        &self,
        region: usize,
        base: &crate::text3::cache::UnifiedConstraints,
        text_width: f32,
        origin: LogicalPosition,
        pen: f32,
        bottom_edge: f32,
    ) -> crate::text3::cache::UnifiedConstraints {
        let fragmentainer = &self.chain[region];
        let content = fragmentainer.content;
        let mut constraints = base.clone();
        constraints.available_width = AvailableSpace::Definite(text_width);
        constraints.available_height = (region + 1 < self.chain.len())
            .then(|| (content.size.height - pen - bottom_edge).max(0.0));
        let dx = content.origin.x - origin.x;
        let dy = content.origin.y - origin.y;
        if let Some(shape) = &fragmentainer.shape {
            constraints.shape_boundaries.push(shape.translated(dx, dy));
            let area = (content.size.height - pen).max(0.0);
            constraints.shape_area_height = Some(
                constraints
                    .shape_area_height
                    .map_or(area, |own| own.min(area)),
            );
        }
        constraints
            .shape_exclusions
            .extend(fragmentainer.floats.iter().map(|f| f.translated(dx, dy)));
        constraints
    }

    /// Notes the text `flow` has left as where the flow's overflow starts.
    fn paragraph_overflow(&mut self, tree: &LayoutTree, node: usize, flow: &ParagraphFlow) {
        let (text_node, byte_offset) = flow
            .remaining()
            .find_map(ShapedItem::as_cluster)
            .map_or((None, None), |cluster| {
                (
                    cluster.source_node_id,
                    usize::try_from(cluster.source_cluster_id.start_byte_in_run).ok(),
                )
            });
        self.overflow = Some(FlowOverflow {
            node: Some(node),
            dom_node: tree.get(LayoutNodeId::new(node)).and_then(|n| n.dom_node_id),
            text_node,
            byte_offset,
            last_region: Some(self.here().node),
            needed_height: 0.0,
        });
    }

    /// Records a paragraph's parts: each region's lines on their own, the
    /// whole paragraph's lines from its first part's content box (the
    /// selection, the caret and hit-testing read the paragraph as one
    /// layout), and its atomic inlines where their lines put them.
    #[allow(clippy::too_many_arguments)]
    fn record_paragraph(
        &mut self,
        styled_dom: &StyledDom,
        tree: &mut LayoutTree,
        positions: &mut PositionVec,
        node: usize,
        inset: Inset,
        edges: &Edges,
        start: (usize, f32),
        parts: &[Part],
        paragraph: &fc::FlowParagraph,
    ) {
        let first_rect = self.part_rect(tree, node, inset, start.0, start);
        let first_content =
            LogicalPosition::new(first_rect.origin.x + edges.left, first_rect.origin.y + edges.top);
        let mut fragments = Vec::new();
        let mut whole_items: Vec<PositionedItem> = Vec::new();
        for region in start.0..=self.region {
            let rect = self.part_rect(tree, node, inset, region, start);
            // A part's lines stand from the content box of its rect.
            let content_origin =
                LogicalPosition::new(rect.origin.x + edges.left, rect.origin.y + edges.top);
            let text_width = (rect.size.width - edges.inline()).max(0.0);
            let mut lines = None;
            for part in parts.iter().filter(|p| p.region == region) {
                lines = Some(Arc::new(CachedInlineLayout::new(
                    Arc::new(shift_layout(
                        &part.layout,
                        part.origin.x - content_origin.x,
                        part.origin.y - content_origin.y,
                    )),
                    AvailableSpace::Definite(text_width),
                    false,
                )));
                whole_items.extend(
                    shift_layout(
                        &part.layout,
                        part.origin.x - first_content.x,
                        part.origin.y - first_content.y,
                    )
                    .items,
                );
            }
            fragments.push(BoxFragment {
                region: self.chain[region].node,
                rect,
                starts: region == start.0,
                ends: region == self.region,
                whole: false,
                lines,
            });
        }
        super::pos_set(positions, node, first_rect.origin);
        if let Some(hot) = tree.get_mut(LayoutNodeId::new(node)) {
            hot.used_size = Some(first_rect.size);
        }
        let whole = Arc::new(UnifiedLayout {
            items: whole_items,
            overflow: crate::text3::cache::OverflowInfo::default(),
        });
        if let Some(warm) = tree.warm_mut(LayoutNodeId::new(node)) {
            warm.inline_layout_result = Some(Box::new(CachedInlineLayout::new(
                Arc::clone(&whole),
                AvailableSpace::Definite((first_rect.size.width - edges.inline()).max(0.0)),
                false,
            )));
        }
        // The atomic inlines (inline-blocks, images) stand where their lines
        // put them, in the region of their line.
        let output = fc::finish_inline_layout(
            &whole,
            &paragraph.constraints,
            styled_dom,
            paragraph.style_dom_id,
            &paragraph.child_map,
        );
        for (child, at) in output.positions {
            let to = LogicalPosition::new(first_content.x + at.x, first_content.y + at.y);
            move_subtree(tree, positions, child, to);
            let size = tree
                .get(LayoutNodeId::new(child))
                .and_then(|n| n.used_size)
                .unwrap_or_default();
            let region = fragments
                .iter()
                .find(|f| f.rect.contains(to))
                .map_or(fragments[0].region, |f| f.region);
            self.fragments.insert(
                child,
                alloc::vec![BoxFragment {
                    region,
                    rect: LogicalRect::new(to, size),
                    starts: true,
                    ends: true,
                    whole: true,
                    lines: None,
                }],
            );
        }
        self.fragments.insert(node, fragments);
    }
}

/// The lines of one region's part of a paragraph, laid out from `origin`.
struct Part {
    region: usize,
    origin: LogicalPosition,
    layout: UnifiedLayout,
}

/// A box's borders and paddings: the edges around its content.
#[derive(Debug, Clone, Copy)]
struct Edges {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

impl Edges {
    fn of(bp: &crate::solver3::geometry::ResolvedBoxProps) -> Self {
        Self {
            left: bp.border.left + bp.padding.left,
            right: bp.border.right + bp.padding.right,
            top: bp.border.top + bp.padding.top,
            bottom: bp.border.bottom + bp.padding.bottom,
        }
    }

    fn inline(&self) -> f32 {
        self.left + self.right
    }
}

/// The side lines flow on beside a float on `side`.
const fn float_wrap(side: LayoutFloat) -> ExclusionWrap {
    match side {
        LayoutFloat::Right => ExclusionWrap::Left,
        LayoutFloat::Left | LayoutFloat::None => ExclusionWrap::Right,
    }
}

/// `layout` with every item moved by `dx`, `dy`.
fn shift_layout(layout: &UnifiedLayout, dx: f32, dy: f32) -> UnifiedLayout {
    let mut moved = layout.clone();
    for item in &mut moved.items {
        item.position.x += dx;
        item.position.y += dy;
    }
    moved.overflow.unclipped_bounds.x += dx;
    moved.overflow.unclipped_bounds.y += dy;
    moved
}

/// Places the box `node` at `to` (its border box's top left corner): its
/// subtree moves with it, or, when it was just laid out again
/// (`relaid_out`), is placed from the relative positions that layout gave.
fn place_box(
    tree: &LayoutTree,
    positions: &mut PositionVec,
    node: usize,
    to: LogicalPosition,
    relaid_out: bool,
) {
    if !relaid_out {
        move_subtree(tree, positions, node, to);
        return;
    }
    super::pos_set(positions, node, to);
    let bp = tree
        .get(LayoutNodeId::new(node))
        .map(|n| n.box_props.unpack())
        .unwrap_or_default();
    let content = LogicalPosition::new(
        to.x + bp.border.left + bp.padding.left,
        to.y + bp.border.top + bp.padding.top,
    );
    super::cache::position_bfc_child_descendants(tree, node, content, positions);
}

/// Moves the box `node` to `to` (its border box's top left corner), its
/// subtree with it.
fn move_subtree(tree: &LayoutTree, positions: &mut PositionVec, node: usize, to: LogicalPosition) {
    let from = super::pos_get(positions, node).unwrap_or(to);
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let mut stack = alloc::vec![node];
    while let Some(index) = stack.pop() {
        if let Some(at) = super::pos_get(positions, index) {
            super::pos_set(positions, index, LogicalPosition::new(at.x + dx, at.y + dy));
        }
        stack.extend_from_slice(tree.children(index));
    }
    super::pos_set(positions, node, to);
}

/// Lays the box `node` out again at the border-box width `width` when the
/// main pass gave it another: its content re-wraps, and its height follows
/// when it is auto. A replaced box keeps its size. `true` when it was laid
/// out again.
fn lay_out_at_width<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    node: usize,
    width: f32,
) -> bool {
    let Some(hot) = tree.get(LayoutNodeId::new(node)) else {
        return false;
    };
    let size = hot.used_size.unwrap_or_default();
    if (size.width - width).abs() < 0.5 || tree.children(node).is_empty() {
        return false;
    }
    let replaced = hot.dom_node_id.is_some_and(|dom| {
        let nodes = ctx.styled_dom.node_data.as_container();
        nodes.get(dom).is_some_and(is_replaced_element)
    });
    if replaced {
        return false;
    }
    let bp = hot.box_props.unpack();
    let formatting_context = hot.formatting_context;
    let auto_height = tree
        .warm(LayoutNodeId::new(node))
        .is_none_or(|w| w.computed_style.height.is_none());
    let edges = Edges::of(&bp);
    let inner = LogicalSize::new(
        (width - edges.inline()).max(0.0),
        if auto_height {
            f32::INFINITY
        } else {
            (size.height - edges.top - edges.bottom).max(0.0)
        },
    );
    let constraints = LayoutConstraints {
        available_size: inner,
        writing_mode: LayoutWritingMode::HorizontalTb,
        writing_mode_ctx: WritingModeContext::default(),
        bfc_state: None,
        text_align: TextAlign::Start,
        containing_block_size: inner,
        available_width_type: AvailableSpace::Definite(inner.width),
        fragmentainer: None,
        column_flow: None,
    };
    let mut float_cache: HashMap<usize, FloatingContext> = HashMap::new();
    let Ok(result) =
        fc::layout_formatting_context(ctx, tree, text_cache, node, &constraints, &mut float_cache)
    else {
        return false;
    };
    for (child, at) in &result.output.positions {
        if let Some(warm) = tree.warm_mut(LayoutNodeId::new(*child)) {
            warm.relative_position = Some(*at);
        }
    }
    // A flex or grid box measures its padding in its overflow size, a block
    // does not.
    let padding_in_overflow = matches!(
        formatting_context,
        FormattingContext::Flex | FormattingContext::Grid
    );
    let height = if auto_height {
        result.output.overflow_size.height
            + bp.border.top
            + bp.border.bottom
            + if padding_in_overflow {
                0.0
            } else {
                bp.padding.top + bp.padding.bottom
            }
    } else {
        size.height
    };
    if let Some(hot) = tree.get_mut(LayoutNodeId::new(node)) {
        hot.used_size = Some(LogicalSize::new(width, height));
    }
    true
}
