//! Hit-test result types for determining which DOM nodes are under the cursor,
//! scroll state tracking, and pipeline/document identification. These types
//! feed into the event dispatch system.

use alloc::collections::BTreeMap;
use core::{
    fmt,
    sync::atomic::{AtomicU32, Ordering as AtomicOrdering},
};

use crate::{
    dom::{DomId, DomNodeHash, DomNodeId, OptionDomNodeId, ScrollTagId, ScrollbarOrientation, TagId},
    geom::{LogicalPosition, LogicalRect},
    id::NodeId,
    resources::IdNamespace,
    window::MouseCursorType,
    OrderedMap,
};

/// Result of a hit test against a single DOM, containing all nodes hit
/// by the cursor along with scroll, scrollbar, and cursor-type information.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
pub struct HitTest {
    pub regular_hit_test_nodes: BTreeMap<NodeId, HitTestItem>,
    pub scroll_hit_test_nodes: BTreeMap<NodeId, ScrollHitTestItem>,
    /// Hit test results for scrollbar components.
    pub scrollbar_hit_test_nodes: BTreeMap<ScrollbarHitId, ScrollbarHitTestItem>,
    /// Hit test results for cursor areas (text runs with cursor property).
    /// Maps `NodeId` to (`CursorType`, `hit_depth`) - the cursor type and z-depth of the hit.
    pub cursor_hit_test_nodes: BTreeMap<NodeId, CursorHitTestItem>,
}

/// Hit test item for cursor areas (determines which cursor icon to show).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct CursorHitTestItem {
    pub cursor_type: CursorType,
    pub hit_depth: u32,
    pub point_in_viewport: LogicalPosition,
}

impl HitTest {
    #[must_use] pub const fn empty() -> Self {
        Self {
            regular_hit_test_nodes: BTreeMap::new(),
            scroll_hit_test_nodes: BTreeMap::new(),
            scrollbar_hit_test_nodes: BTreeMap::new(),
            cursor_hit_test_nodes: BTreeMap::new(),
        }
    }
    #[must_use] pub fn is_empty(&self) -> bool {
        self.regular_hit_test_nodes.is_empty()
            && self.scroll_hit_test_nodes.is_empty()
            && self.scrollbar_hit_test_nodes.is_empty()
            && self.cursor_hit_test_nodes.is_empty()
    }
}

/// Unique identifier for a specific component of a scrollbar.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum ScrollbarHitId {
    VerticalTrack(DomId, NodeId),
    VerticalThumb(DomId, NodeId),
    HorizontalTrack(DomId, NodeId),
    HorizontalThumb(DomId, NodeId),
}

/// Hit test item specifically for scrollbar components.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct ScrollbarHitTestItem {
    pub point_in_viewport: LogicalPosition,
    pub point_relative_to_item: LogicalPosition,
    pub orientation: ScrollbarOrientation,
}

/// Scroll frame identifier combining a unique `u64` tag with its owning `PipelineId`.
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
        write!(f, "{self}")
    }
}

/// A node whose content overflows its parent, requiring scroll handling.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd)]
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

/// Extra source identifier within a pipeline, allowing multiple independent
/// subsystems to generate `PipelineId` values without collision.
///
/// All pipelines still share the same `IdNamespace` and `DocumentId`.
pub type PipelineSourceId = u32;

/// Information about a scroll frame, given to the user by the framework
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
pub struct ScrollPosition {
    /// How big is the parent container
    /// (so that things like "scroll to left edge" can be implemented)?
    pub parent_rect: LogicalRect,
    /// How big is the scroll rect (i.e. the union of all children)?
    pub children_rect: LogicalRect,
}

/// Identifies a document within a namespace, used for multi-document rendering.
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
        write!(f, "{self}")
    }
}

/// Identifies a rendering pipeline by source and sequence number.
#[derive(Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct PipelineId(pub PipelineSourceId, pub u32);

impl ::core::fmt::Display for PipelineId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PipelineId({}, {})", self.0, self.1)
    }
}

impl ::core::fmt::Debug for PipelineId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self}")
    }
}

static LAST_PIPELINE_ID: AtomicU32 = AtomicU32::new(0);

impl Default for PipelineId {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineId {
    pub const DUMMY: Self = Self(0, 0);

    pub fn new() -> Self {
        Self(
            LAST_PIPELINE_ID.fetch_add(1, AtomicOrdering::SeqCst),
            0,
        )
    }
}

/// A single hit-test result for a regular (non-scroll) DOM node.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd)]
pub struct HitTestItem {
    /// The hit point in the coordinate space of the "viewport" of the display item.
    /// The viewport is the scroll node formed by the root reference frame of the display item's
    /// pipeline.
    pub point_in_viewport: LogicalPosition,
    /// The coordinates of the original hit test point relative to the origin of this item.
    /// This is useful for calculating things like text offsets in the client.
    pub point_relative_to_item: LogicalPosition,
    /// Necessary to easily get the nearest `VirtualView` node
    pub is_focusable: bool,
    /// If this hit is a `VirtualView` node, stores the `VirtualViews` `DomId` + the origin of the `VirtualView`
    pub is_virtual_view_hit: Option<(DomId, LogicalPosition)>,
    /// Z-order depth from `WebRender` hit test (0 = frontmost/topmost in z-order).
    /// Lower values are closer to the user. This preserves the ordering from
    /// `WebRender`'s hit test results which returns items front-to-back.
    pub hit_depth: u32,
}

/// A hit-test result for a scrollable DOM node.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd)]
pub struct ScrollHitTestItem {
    /// The hit point in the coordinate space of the "viewport" of the display item.
    /// The viewport is the scroll node formed by the root reference frame of the display item's
    /// pipeline.
    pub point_in_viewport: LogicalPosition,
    /// The coordinates of the original hit test point relative to the origin of this item.
    /// This is useful for calculating things like text offsets in the client.
    pub point_relative_to_item: LogicalPosition,
    /// If this hit is a `VirtualView` node, stores the `VirtualViews` `DomId` + the origin of the `VirtualView`
    pub scroll_node: OverflowingScrollNode,
}

/// Map of active scroll states, keyed by their external scroll ID.
#[derive(Debug, Default)]
pub struct ScrollStates(pub OrderedMap<ExternalScrollId, ScrollState>);

impl ScrollStates {
    #[must_use] pub fn new() -> Self {
        Self::default()
    }

    #[must_use] pub fn get_scroll_position(&self, scroll_id: &ExternalScrollId) -> Option<LogicalPosition> {
        self.0.get(scroll_id).map(ScrollState::get)
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
            .or_default()
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
            .or_default()
            .add(scroll_by_x, scroll_by_y, &node.child_rect);
    }
}

/// Current scroll position for a single scroll frame.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct ScrollState {
    /// Amount in pixel that the current node is scrolled
    pub scroll_position: LogicalPosition,
}

impl_option!(
    ScrollState,
    OptionScrollState,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd]
);

impl ScrollState {
    /// Return the current position of the scroll state
    #[must_use] pub const fn get(&self) -> LogicalPosition {
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
    pub const fn set(&mut self, x: f32, y: f32, child_rect: &LogicalRect) {
        self.scroll_position.x = x.max(0.0).min(child_rect.size.width);
        self.scroll_position.y = y.max(0.0).min(child_rect.size.height);
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            scroll_position: LogicalPosition::zero(),
        }
    }
}

/// Complete hit-test result across all DOMs, including the currently focused node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullHitTest {
    pub hovered_nodes: BTreeMap<DomId, HitTest>,
    pub focused_node: OptionDomNodeId,
}

impl FullHitTest {
    /// Create an empty hit-test result
    #[must_use] pub fn empty(focused_node: Option<DomNodeId>) -> Self {
        Self {
            hovered_nodes: BTreeMap::new(),
            focused_node: focused_node.into(),
        }
    }

    /// Returns `true` if no nodes were hovered (ignores `focused_node`).
    #[must_use] pub fn is_empty(&self) -> bool {
        self.hovered_nodes.is_empty()
    }
}

/// Result of determining which mouse cursor icon to display based on hit-test results.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CursorTypeHitTest {
    /// closest-node is used for determining the cursor: property
    /// The node is guaranteed to have a non-default cursor: property,
    /// so that the cursor icon can be set accordingly
    pub cursor_node: Option<(DomId, NodeId)>,
    /// Mouse cursor type to set (if `cursor_node` is None, this is set to
    /// `MouseCursorType::Default`)
    pub cursor_icon: MouseCursorType,
}

// ============================================================================
// Type-safe hit-test tag system (merged from the former `hit_test_tag` module).
//
// Encodes WebRender's ItemTag = (u64, u16): the tag *type* lives in the upper
// byte of tag.1 (DOM node / scrollbar / selection / cursor / scroll-container),
// keeping tag types free of bit-level conflicts. See the TAG_TYPE_* constants.
// ============================================================================
// ============================================================================
// Tag Type Markers (stored in upper byte of ItemTag.1)
// ============================================================================

/// Marker for DOM node tags (regular UI elements with callbacks, focus, etc.)
pub const TAG_TYPE_DOM_NODE: u16 = 0x0100;

/// Marker for scrollbar component tags
pub const TAG_TYPE_SCROLLBAR: u16 = 0x0200;

/// Marker for text selection hit-test areas (determines text selection regions)
///
/// These are pushed for text runs to enable text selection without affecting
/// other hit-test logic. Selection may trigger re-rendering.
///
/// NOTE: Text selection hit-testing currently uses `TAG_TYPE_CURSOR` (0x0400).
/// This constant is used by the `HitTestTag::Selection` variant for encoding
/// selection-specific tags (e.g., text run selection areas).
pub const TAG_TYPE_SELECTION: u16 = 0x0300;

/// Marker for cursor hit-test areas (determines which cursor icon to show)
///
/// These are separate from DOM node tags to allow efficient cursor resolution
/// without iterating over all DOM nodes. Cursor changes never require re-rendering.
pub const TAG_TYPE_CURSOR: u16 = 0x0400;

/// Marker for scroll container hit-test areas (for trackpad/wheel scrolling)
///
/// These identify scrollable containers even when no DOM node callbacks are registered.
/// Scroll containers push this tag so the scroll manager can find them during wheel events.
pub const TAG_TYPE_SCROLL_CONTAINER: u16 = 0x0500;


// ============================================================================
// Scrollbar Component Types (stored in lower byte of ItemTag.1 for scrollbar tags)
// ============================================================================

/// Scrollbar component type identifier.
///
/// Each scrollable container can have up to 2 scrollbars (vertical + horizontal),
/// and each scrollbar has 2 main hit regions (track + thumb).
///
/// Future extensions could add:
/// - `UpButton`, `DownButton`, `LeftButton`, `RightButton` for scroll arrows
/// - `PageUp`, `PageDown` for page-scroll regions
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ScrollbarComponent {
    /// The vertical scrollbar track (background area)
    VerticalTrack = 0,
    /// The vertical scrollbar thumb (draggable handle)
    VerticalThumb = 1,
    /// The horizontal scrollbar track (background area)
    HorizontalTrack = 2,
    /// The horizontal scrollbar thumb (draggable handle)
    HorizontalThumb = 3,
    // Future: scroll arrow buttons
    // VerticalUpButton = 4,
    // VerticalDownButton = 5,
    // HorizontalLeftButton = 6,
    // HorizontalRightButton = 7,
}

impl ScrollbarComponent {
    /// Convert from raw u8 value
    #[must_use] pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::VerticalTrack),
            1 => Some(Self::VerticalThumb),
            2 => Some(Self::HorizontalTrack),
            3 => Some(Self::HorizontalThumb),
            _ => None,
        }
    }

}

// ============================================================================
// WebRender Hit-Test Tag (unified type-safe representation)
// ============================================================================

/// Unified, type-safe representation of a `WebRender` hit-test tag.
///
/// This enum represents all possible types of hit-test targets. Each variant
/// can be encoded to and decoded from `WebRender`'s `(u64, u16)` `ItemTag` format.
///
/// ## Namespace Separation
///
/// Different tag types are kept in separate namespaces to:
/// - Enable efficient hit-test queries (only iterate over relevant tags)
/// - Get automatic depth sorting from `WebRender` per namespace
/// - Prevent accidental collisions between different hit-test purposes
///
/// | Namespace | Purpose                              |
/// |-----------|--------------------------------------|
/// | 0x0100    | DOM nodes (callbacks, focus, hover)  |
/// | 0x0200    | Scrollbar components                 |
/// | 0x0300    | Selection areas (text selection)     |
/// | 0x0400    | Cursor areas (cursor icon display)     |
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HitTestTag {
    /// A regular DOM node (button, div, text container, etc.)
    ///
    /// These are nodes that have callbacks, are focusable, or have hover styles.
    /// The `TagId` is a sequential counter assigned during DOM styling.
    DomNode {
        /// The unique tag ID assigned to this DOM node
        tag_id: TagId,
    },

    /// A scrollbar component (track or thumb)
    ///
    /// Each scrollable container can have up to 2 scrollbars.
    /// The scrollbar is identified by the `DomId` and `NodeId` of the scrollable container.
    Scrollbar {
        /// The DOM that contains the scrollable container
        dom_id: DomId,
        /// The `NodeId` of the scrollable container (not the scrollbar itself)
        node_id: NodeId,
        /// Which component of the scrollbar was hit
        component: ScrollbarComponent,
    },

    /// A cursor hit-test area (determines which cursor icon to display)
    ///
    /// These are pushed separately from DOM nodes to allow efficient cursor
    /// resolution. The cursor type is encoded in the lower byte of tag.1.
    Cursor {
        /// The DOM node this cursor area belongs to
        dom_id: DomId,
        /// The `NodeId` of the element with the cursor property
        node_id: NodeId,
        /// The cursor type to display when hovering over this area
        cursor_type: CursorType,
    },

    /// A text selection hit-test area
    ///
    /// These are pushed for text runs to enable text selection.
    /// Separate from DOM nodes to prevent interference with other hit-testing.
    Selection {
        /// The DOM containing the text
        dom_id: DomId,
        /// The `NodeId` of the text container (not the Text node itself)
        container_node_id: NodeId,
        /// The index of the text run within the container (for multi-line text)
        text_run_index: u16,
    },
}

/// Cursor type encoded in cursor hit-test tags.
/// Stored in the lower byte of the ItemTag.1 field.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(u8)]
pub enum CursorType {
    #[default]
    Default = 0,
    Pointer = 1,
    Text = 2,
    Crosshair = 3,
    Move = 4,
    NotAllowed = 5,
    Grab = 6,
    Grabbing = 7,
    EResize = 8,
    WResize = 9,
    NResize = 10,
    SResize = 11,
    EwResize = 12,
    NsResize = 13,
    NeswResize = 14,
    NwseResize = 15,
    ColResize = 16,
    RowResize = 17,
    Wait = 18,
    Help = 19,
    Progress = 20,
    // Add more as needed, up to 255
}

impl CursorType {
    /// Convert from raw u8 value
    #[must_use] pub const fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Default,
            1 => Self::Pointer,
            2 => Self::Text,
            3 => Self::Crosshair,
            4 => Self::Move,
            5 => Self::NotAllowed,
            6 => Self::Grab,
            7 => Self::Grabbing,
            8 => Self::EResize,
            9 => Self::WResize,
            10 => Self::NResize,
            11 => Self::SResize,
            12 => Self::EwResize,
            13 => Self::NsResize,
            14 => Self::NeswResize,
            15 => Self::NwseResize,
            16 => Self::ColResize,
            17 => Self::RowResize,
            18 => Self::Wait,
            19 => Self::Help,
            20 => Self::Progress,
            _ => Self::Default,
        }
    }
}

impl HitTestTag {
    /// Encode this tag to `WebRender`'s `ItemTag` format.
    ///
    /// Returns `(u64, u16)` suitable for passing to `WebRender`'s `push_hit_test`.
    #[must_use] pub fn to_item_tag(&self) -> (u64, u16) {
        match self {
            Self::DomNode { tag_id } => {
                // tag.0 = TagId.inner (the sequential counter)
                // tag.1 = TAG_TYPE_DOM_NODE marker
                (tag_id.inner, TAG_TYPE_DOM_NODE)
            }
            Self::Scrollbar {
                dom_id,
                node_id,
                component,
            } => {
                // tag.0 = DomId (upper 32 bits) | NodeId (lower 32 bits)
                let tag_value = ((dom_id.inner as u64) << 32) | (node_id.index() as u64);
                // tag.1 = TAG_TYPE_SCROLLBAR | component type in lower byte
                let tag_type = TAG_TYPE_SCROLLBAR | (*component as u16);
                (tag_value, tag_type)
            }
            Self::Cursor {
                dom_id,
                node_id,
                cursor_type,
            } => {
                // tag.0 = DomId (upper 32 bits) | NodeId (lower 32 bits)
                let tag_value = ((dom_id.inner as u64) << 32) | (node_id.index() as u64);
                // tag.1 = TAG_TYPE_CURSOR | cursor type in lower byte
                let tag_type = TAG_TYPE_CURSOR | (*cursor_type as u16);
                (tag_value, tag_type)
            }
            Self::Selection {
                dom_id,
                container_node_id,
                text_run_index,
            } => {
                // tag.0 = DomId (upper 16 bits) | NodeId (middle 32 bits) | text_run_index (lower 16 bits)
                debug_assert!(dom_id.inner <= 0xFFFF, "Selection tag: DomId {} exceeds 16-bit range", dom_id.inner);
                debug_assert!(container_node_id.index() <= 0xFFFF_FFFF, "Selection tag: NodeId {} exceeds 32-bit range", container_node_id.index());
                let tag_value = ((dom_id.inner as u64) << 48)
                    | ((container_node_id.index() as u64) << 16)
                    | u64::from(*text_run_index);
                (tag_value, TAG_TYPE_SELECTION)
            }
        }
    }

    /// Decode a `WebRender` `ItemTag` back to a typed `HitTestTag`.
    ///
    /// Returns `None` if the tag format is invalid or unrecognized.
    #[must_use] pub fn from_item_tag(tag: (u64, u16)) -> Option<Self> {
        let (tag_value, tag_type) = tag;

        // Extract tag type from upper byte
        let type_marker = tag_type & 0xFF00;

        match type_marker {
            TAG_TYPE_DOM_NODE => {
                // DOM node tag: tag.0 is the TagId
                Some(Self::DomNode {
                    tag_id: TagId { inner: tag_value },
                })
            }
            TAG_TYPE_SCROLLBAR => {
                // Scrollbar tag: decode DomId, NodeId, and component
                let dom_id = DomId {
                    inner: ((tag_value >> 32) & 0xFFFF_FFFF) as usize,
                };
                let node_id = NodeId::new((tag_value & 0xFFFF_FFFF) as usize);
                let component_value = (tag_type & 0x00FF) as u8;
                let component = ScrollbarComponent::from_u8(component_value)?;

                Some(Self::Scrollbar {
                    dom_id,
                    node_id,
                    component,
                })
            }
            TAG_TYPE_CURSOR => {
                // Cursor tag: decode DomId, NodeId, and cursor type
                let dom_id = DomId {
                    inner: ((tag_value >> 32) & 0xFFFF_FFFF) as usize,
                };
                let node_id = NodeId::new((tag_value & 0xFFFF_FFFF) as usize);
                let cursor_value = (tag_type & 0x00FF) as u8;
                let cursor_type = CursorType::from_u8(cursor_value);

                Some(Self::Cursor {
                    dom_id,
                    node_id,
                    cursor_type,
                })
            }
            TAG_TYPE_SELECTION => {
                // Selection tag: decode DomId, NodeId, and text run index
                let dom_id = DomId {
                    inner: ((tag_value >> 48) & 0xFFFF) as usize,
                };
                let container_node_id = NodeId::new(((tag_value >> 16) & 0xFFFF_FFFF) as usize);
                let text_run_index = (tag_value & 0xFFFF) as u16;

                Some(Self::Selection {
                    dom_id,
                    container_node_id,
                    text_run_index,
                })
            }
            _ => {
                // Unknown tag type - could be a legacy tag or corruption
                // For backwards compatibility, treat tags with tag_type == 0
                // as legacy DOM node tags (old format before type markers)
                if tag_type == 0 {
                    Some(Self::DomNode {
                        tag_id: TagId { inner: tag_value },
                    })
                } else {
                    None
                }
            }
        }
    }

    /// Check if this is a DOM node tag
    #[must_use] pub const fn is_dom_node(&self) -> bool {
        matches!(self, Self::DomNode { .. })
    }

    /// Check if this is a scrollbar tag
    #[must_use] pub const fn is_scrollbar(&self) -> bool {
        matches!(self, Self::Scrollbar { .. })
    }

    /// Check if this is a cursor tag
    #[must_use] pub const fn is_cursor(&self) -> bool {
        matches!(self, Self::Cursor { .. })
    }

    /// Check if this is a selection tag
    #[must_use] pub const fn is_selection(&self) -> bool {
        matches!(self, Self::Selection { .. })
    }

    /// Get the `TagId` if this is a DOM node tag
    #[must_use] pub const fn as_dom_node(&self) -> Option<TagId> {
        match self {
            Self::DomNode { tag_id } => Some(*tag_id),
            _ => None,
        }
    }

    /// Get cursor info if this is a cursor tag
    #[must_use] pub const fn as_cursor(&self) -> Option<(DomId, NodeId, CursorType)> {
        match self {
            Self::Cursor {
                dom_id,
                node_id,
                cursor_type,
            } => Some((*dom_id, *node_id, *cursor_type)),
            _ => None,
        }
    }

    /// Get selection info if this is a selection tag
    #[must_use] pub const fn as_selection(&self) -> Option<(DomId, NodeId, u16)> {
        match self {
            Self::Selection {
                dom_id,
                container_node_id,
                text_run_index,
            } => Some((*dom_id, *container_node_id, *text_run_index)),
            _ => None,
        }
    }

    /// Get scrollbar info if this is a scrollbar tag
    #[must_use] pub const fn as_scrollbar(&self) -> Option<(DomId, NodeId, ScrollbarComponent)> {
        match self {
            Self::Scrollbar {
                dom_id,
                node_id,
                component,
            } => Some((*dom_id, *node_id, *component)),
            _ => None,
        }
    }
}

impl fmt::Display for HitTestTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DomNode { tag_id } => {
                write!(f, "DomNode(tag:{})", tag_id.inner)
            }
            Self::Scrollbar {
                dom_id,
                node_id,
                component,
            } => {
                write!(
                    f,
                    "Scrollbar(dom:{}, node:{}, {:?})",
                    dom_id.inner,
                    node_id.index(),
                    component
                )
            }
            Self::Cursor {
                dom_id,
                node_id,
                cursor_type,
            } => {
                write!(
                    f,
                    "Cursor(dom:{}, node:{}, {:?})",
                    dom_id.inner,
                    node_id.index(),
                    cursor_type
                )
            }
            Self::Selection {
                dom_id,
                container_node_id,
                text_run_index,
            } => {
                write!(
                    f,
                    "Selection(dom:{}, container:{}, run:{})",
                    dom_id.inner,
                    container_node_id.index(),
                    text_run_index
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dom_node_tag_roundtrip() {
        let tag = HitTestTag::DomNode {
            tag_id: TagId { inner: 42 },
        };
        let item_tag = tag.to_item_tag();
        let decoded = HitTestTag::from_item_tag(item_tag).unwrap();
        assert_eq!(tag, decoded);
    }

    #[test]
    fn test_scrollbar_tag_roundtrip() {
        let tag = HitTestTag::Scrollbar {
            dom_id: DomId { inner: 1 },
            node_id: NodeId::new(123),
            component: ScrollbarComponent::VerticalThumb,
        };
        let item_tag = tag.to_item_tag();
        let decoded = HitTestTag::from_item_tag(item_tag).unwrap();
        assert_eq!(tag, decoded);
    }

    #[test]
    fn test_dom_node_tag_not_confused_with_scrollbar() {
        // A DOM node tag with value 673 should NOT be decoded as a scrollbar
        let dom_tag = HitTestTag::DomNode {
            tag_id: TagId { inner: 673 },
        };
        let item_tag = dom_tag.to_item_tag();

        // Verify it has the correct type marker
        assert_eq!(item_tag.1, TAG_TYPE_DOM_NODE);

        // Verify it decodes correctly
        let decoded = HitTestTag::from_item_tag(item_tag).unwrap();
        assert!(decoded.is_dom_node());
        assert!(!decoded.is_scrollbar());
    }

    #[test]
    fn test_legacy_tag_compatibility() {
        // Old format tags had tag.1 == 0
        // They should be treated as DOM node tags for backwards compatibility
        let legacy_tag = (42u64, 0u16);
        let decoded = HitTestTag::from_item_tag(legacy_tag).unwrap();
        assert!(decoded.is_dom_node());
        assert_eq!(decoded.as_dom_node().unwrap().inner, 42);
    }
}
