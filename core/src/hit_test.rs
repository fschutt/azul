use alloc::collections::BTreeMap;
use core::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};

use crate::{
    dom::{DomId, DomNodeHash, DomNodeId, OptionDomNodeId, ScrollTagId, ScrollbarOrientation},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test_tag::CursorType,
    id::NodeId,
    resources::IdNamespace,
    styled_dom::NodeHierarchyItemId,
    window::MouseCursorType,
    FastHashMap,
};

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct HitTest {
    pub regular_hit_test_nodes: BTreeMap<NodeId, HitTestItem>,
    pub scroll_hit_test_nodes: BTreeMap<NodeId, ScrollHitTestItem>,
    /// Hit test results for scrollbar components.
    pub scrollbar_hit_test_nodes: BTreeMap<ScrollbarHitId, ScrollbarHitTestItem>,
    /// Hit test results for cursor areas (text runs with cursor property).
    /// Maps NodeId to (CursorType, hit_depth) - the cursor type and z-depth of the hit.
    pub cursor_hit_test_nodes: BTreeMap<NodeId, CursorHitTestItem>,
}

/// Hit test item for cursor areas (determines which cursor icon to show).
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct CursorHitTestItem {
    pub cursor_type: CursorType,
    pub hit_depth: u32,
    pub point_in_viewport: LogicalPosition,
}

impl HitTest {
    pub fn empty() -> Self {
        Self {
            regular_hit_test_nodes: BTreeMap::new(),
            scroll_hit_test_nodes: BTreeMap::new(),
            scrollbar_hit_test_nodes: BTreeMap::new(),
            cursor_hit_test_nodes: BTreeMap::new(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.regular_hit_test_nodes.is_empty()
            && self.scroll_hit_test_nodes.is_empty()
            && self.scrollbar_hit_test_nodes.is_empty()
            && self.cursor_hit_test_nodes.is_empty()
    }
}

/// NEW: Unique identifier for a specific component of a scrollbar.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum ScrollbarHitId {
    VerticalTrack(DomId, NodeId),
    VerticalThumb(DomId, NodeId),
    HorizontalTrack(DomId, NodeId),
    HorizontalThumb(DomId, NodeId),
}

/// Hit test item specifically for scrollbar components.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ScrollbarHitTestItem {
    pub point_in_viewport: LogicalPosition,
    pub point_relative_to_item: LogicalPosition,
    pub orientation: ScrollbarOrientation,
}

#[derive(Copy, Clone, Eq, Hash, PartialEq, Ord, PartialOrd)]
#[repr(C)]
pub struct ExternalScrollId(pub u64, pub PipelineId);

impl ::core::fmt::Display for ExternalScrollId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ExternalScrollId({})", self.0)
    }
}

impl ::core::fmt::Debug for ExternalScrollId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct ScrolledNodes {
    pub overflowing_nodes: BTreeMap<NodeHierarchyItemId, OverflowingScrollNode>,
    /// Nodes that need to clip their direct children (i.e. nodes
    /// with overflow-x and overflow-y set to "Hidden")
    pub clip_nodes: BTreeMap<NodeId, LogicalSize>,
    pub tags_to_node_ids: BTreeMap<ScrollTagId, NodeHierarchyItemId>,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct OverflowingScrollNode {
    pub parent_rect: LogicalRect,
    pub child_rect: LogicalRect,
    pub virtual_child_rect: LogicalRect,
    pub parent_external_scroll_id: ExternalScrollId,
    pub parent_dom_hash: DomNodeHash,
    pub scroll_tag_id: ScrollTagId,
}

impl Default for OverflowingScrollNode {
    fn default() -> Self {
        use crate::dom::TagId;
        Self {
            parent_rect: LogicalRect::zero(),
            child_rect: LogicalRect::zero(),
            virtual_child_rect: LogicalRect::zero(),
            parent_external_scroll_id: ExternalScrollId(0, PipelineId::DUMMY),
            parent_dom_hash: DomNodeHash { inner: 0 },
            scroll_tag_id: ScrollTagId {
                inner: TagId { inner: 0 },
            },
        }
    }
}

/// This type carries no valuable semantics for WR. However, it reflects the fact that
/// clients (Servo) may generate pipelines by different semi-independent sources.
///
/// These pipelines still belong to the same `IdNamespace` and the same `DocumentId`.
/// Having this extra Id field enables them to generate `PipelineId` without collision.
pub type PipelineSourceId = u32;

/// Information about a scroll frame, given to the user by the framework
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct ScrollPosition {
    /// How big is the parent container
    /// (so that things like "scroll to left edge" can be implemented)?
    pub parent_rect: LogicalRect,
    /// How big is the scroll rect (i.e. the union of all children)?
    pub children_rect: LogicalRect,
}

#[derive(Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct DocumentId {
    pub namespace_id: IdNamespace,
    pub id: u32,
}

impl ::core::fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "DocumentId {{ ns: {}, id: {} }}",
            self.namespace_id, self.id
        )
    }
}

impl ::core::fmt::Debug for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

#[derive(Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct PipelineId(pub PipelineSourceId, pub u32);

impl ::core::fmt::Display for PipelineId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PipelineId({}, {})", self.0, self.1)
    }
}

impl ::core::fmt::Debug for PipelineId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

static LAST_PIPELINE_ID: AtomicUsize = AtomicUsize::new(0);

impl PipelineId {
    pub const DUMMY: PipelineId = PipelineId(0, 0);

    pub fn new() -> Self {
        PipelineId(
            LAST_PIPELINE_ID.fetch_add(1, AtomicOrdering::SeqCst) as u32,
            0,
        )
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct HitTestItem {
    /// The hit point in the coordinate space of the "viewport" of the display item.
    /// The viewport is the scroll node formed by the root reference frame of the display item's
    /// pipeline.
    pub point_in_viewport: LogicalPosition,
    /// The coordinates of the original hit test point relative to the origin of this item.
    /// This is useful for calculating things like text offsets in the client.
    pub point_relative_to_item: LogicalPosition,
    /// Necessary to easily get the nearest VirtualizedView node
    pub is_focusable: bool,
    /// If this hit is a VirtualizedView node, stores the VirtualizedViews DomId + the origin of the VirtualizedView
    pub is_virtualized_view_hit: Option<(DomId, LogicalPosition)>,
    /// Z-order depth from WebRender hit test (0 = frontmost/topmost in z-order).
    /// Lower values are closer to the user. This preserves the ordering from
    /// WebRender's hit test results which returns items front-to-back.
    pub hit_depth: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct ScrollHitTestItem {
    /// The hit point in the coordinate space of the "viewport" of the display item.
    /// The viewport is the scroll node formed by the root reference frame of the display item's
    /// pipeline.
    pub point_in_viewport: LogicalPosition,
    /// The coordinates of the original hit test point relative to the origin of this item.
    /// This is useful for calculating things like text offsets in the client.
    pub point_relative_to_item: LogicalPosition,
    /// If this hit is a VirtualizedView node, stores the VirtualizedViews DomId + the origin of the VirtualizedView
    pub scroll_node: OverflowingScrollNode,
}

#[derive(Debug, Default)]
pub struct ScrollStates(pub FastHashMap<ExternalScrollId, ScrollState>);

impl ScrollStates {
    /// Special rendering function that skips building a layout and only does
    /// hit-testing and rendering - called on pure scroll events, since it's
    /// significantly less CPU-intensive to just render the last display list instead of
    /// re-layouting on every single scroll event.
    #[must_use]
    pub fn should_scroll_render(
        &mut self,
        (scroll_x, scroll_y): &(f32, f32),
        hit_test: &FullHitTest,
    ) -> bool {
        let mut should_scroll_render = false;

        for hit_test in hit_test.hovered_nodes.values() {
            for scroll_hit_test_item in hit_test.scroll_hit_test_nodes.values() {
                self.scroll_node(&scroll_hit_test_item.scroll_node, *scroll_x, *scroll_y);
                should_scroll_render = true;
                break; // only scroll first node that was hit
            }
        }

        should_scroll_render
    }

    pub fn new() -> ScrollStates {
        ScrollStates::default()
    }

    pub fn get_scroll_position(&self, scroll_id: &ExternalScrollId) -> Option<LogicalPosition> {
        self.0.get(&scroll_id).map(|entry| entry.get())
    }

    /// Set the scroll amount - does not update the `entry.used_this_frame`,
    /// since that is only relevant when we are actually querying the renderer.
    pub fn set_scroll_position(
        &mut self,
        node: &OverflowingScrollNode,
        scroll_position: LogicalPosition,
    ) {
        self.0
            .entry(node.parent_external_scroll_id)
            .or_insert_with(|| ScrollState::default())
            .set(scroll_position.x, scroll_position.y, &node.child_rect);
    }

    /// Updating (add to) the existing scroll amount does not update the
    /// `entry.used_this_frame`, since that is only relevant when we are
    /// actually querying the renderer.
    pub fn scroll_node(
        &mut self,
        node: &OverflowingScrollNode,
        scroll_by_x: f32,
        scroll_by_y: f32,
    ) {
        self.0
            .entry(node.parent_external_scroll_id)
            .or_insert_with(|| ScrollState::default())
            .add(scroll_by_x, scroll_by_y, &node.child_rect);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ScrollState {
    /// Amount in pixel that the current node is scrolled
    pub scroll_position: LogicalPosition,
}

impl_option!(
    ScrollState,
    OptionScrollState,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

impl ScrollState {
    /// Return the current position of the scroll state
    pub fn get(&self) -> LogicalPosition {
        self.scroll_position
    }

    /// Add a scroll X / Y onto the existing scroll state
    pub fn add(&mut self, x: f32, y: f32, child_rect: &LogicalRect) {
        self.scroll_position.x = (self.scroll_position.x + x)
            .max(0.0)
            .min(child_rect.size.width);
        self.scroll_position.y = (self.scroll_position.y + y)
            .max(0.0)
            .min(child_rect.size.height);
    }

    /// Set the scroll state to a new position
    pub fn set(&mut self, x: f32, y: f32, child_rect: &LogicalRect) {
        self.scroll_position.x = x.max(0.0).min(child_rect.size.width);
        self.scroll_position.y = y.max(0.0).min(child_rect.size.height);
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        ScrollState {
            scroll_position: LogicalPosition::zero(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FullHitTest {
    pub hovered_nodes: BTreeMap<DomId, HitTest>,
    pub focused_node: OptionDomNodeId,
}

pub struct FullHitTestHoveredNode {
    pub dom_id: DomId,
    pub hit_test: HitTest,
}

impl_option!(
    FullHitTest,
    OptionFullHitTest,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl FullHitTest {
    /// Create an empty hit-test result
    pub fn empty(focused_node: Option<DomNodeId>) -> Self {
        Self {
            hovered_nodes: BTreeMap::new(),
            focused_node: focused_node.into(),
        }
    }

    /// Check if no nodes were hit
    pub fn is_empty(&self) -> bool {
        self.hovered_nodes.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CursorTypeHitTest {
    /// closest-node is used for determining the cursor: property
    /// The node is guaranteed to have a non-default cursor: property,
    /// so that the cursor icon can be set accordingly
    pub cursor_node: Option<(DomId, NodeId)>,
    /// Mouse cursor type to set (if cursor_node is None, this is set to
    /// `MouseCursorType::Default`)
    pub cursor_icon: MouseCursorType,
}
