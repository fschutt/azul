use core::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};
use std::collections::BTreeMap;

use crate::{
    dom::{DomId, DomNodeHash, ScrollTagId},
    id::NodeId,
    resources::IdNamespace,
    styled_dom::NodeHierarchyItemId,
    window::{LogicalPosition, LogicalRect, LogicalSize},
};

#[derive(Clone, Copy, Eq, Hash, PartialEq, Ord, PartialOrd)]
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
    /// Nodes that need to clip their direct children (i.e. nodes with overflow-x and overflow-y
    /// set to "Hidden")
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
            parent_dom_hash: DomNodeHash(0),
            scroll_tag_id: ScrollTagId(TagId(0)),
        }
    }
}

/// This type carries no valuable semantics for WR. However, it reflects the fact that
/// clients (Servo) may generate pipelines by different semi-independent sources.
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
    /// Necessary to easily get the nearest IFrame node
    pub is_focusable: bool,
    /// If this hit is an IFrame node, stores the IFrames DomId + the origin of the IFrame
    pub is_iframe_hit: Option<(DomId, LogicalPosition)>,
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
    /// If this hit is an IFrame node, stores the IFrames DomId + the origin of the IFrame
    pub scroll_node: OverflowingScrollNode,
}
