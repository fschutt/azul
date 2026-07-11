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
    geom::{LogicalPosition, LogicalRect, LogicalSize},
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ExternalScrollId({})", self.0)
    }
}

impl ::core::fmt::Debug for ExternalScrollId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DocumentId {{ ns: {}, id: {} }}",
            self.namespace_id, self.id
        )
    }
}

impl ::core::fmt::Debug for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

/// Identifies a rendering pipeline by source and sequence number.
#[derive(Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct PipelineId(pub PipelineSourceId, pub u32);

impl ::core::fmt::Display for PipelineId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PipelineId({}, {})", self.0, self.1)
    }
}

impl ::core::fmt::Debug for PipelineId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
        let max_scroll = max_scroll_rect(node);
        self.0
            .entry(node.parent_external_scroll_id)
            .or_default()
            .set(scroll_position.x, scroll_position.y, &max_scroll);
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
        let max_scroll = max_scroll_rect(node);
        self.0
            .entry(node.parent_external_scroll_id)
            .or_default()
            .add(scroll_by_x, scroll_by_y, &max_scroll);
    }
}

/// Compute the maximum scrollable range for a scroll node.
///
/// The maximum scroll offset is `content − viewport` (`child_rect − parent_rect`),
/// clamped to `>= 0`. Previously the scroll position was clamped to the full
/// content size, which let the content scroll entirely out of view. The returned
/// rect keeps `child_rect.origin` and stores the max offset in `size`.
fn max_scroll_rect(node: &OverflowingScrollNode) -> LogicalRect {
    LogicalRect::new(
        node.child_rect.origin,
        LogicalSize::new(
            (node.child_rect.size.width - node.parent_rect.size.width).max(0.0),
            (node.child_rect.size.height - node.parent_rect.size.height).max(0.0),
        ),
    )
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

    /// Add a scroll X / Y onto the existing scroll state.
    ///
    /// `max_scroll_rect` is the *scroll range* rect: its size is the maximum
    /// scrollable offset (`content − viewport`, clamped to `>= 0`), NOT the full
    /// content size. See [`ScrollStates::scroll_node`]. Clamping via `.max(0.0)`
    /// first also collapses any NaN input to `0.0` (`f32::max` returns the
    /// non-NaN operand), so a NaN delta can never poison the scroll position.
    pub fn add(&mut self, x: f32, y: f32, max_scroll_rect: &LogicalRect) {
        self.scroll_position.x = (self.scroll_position.x + x)
            .max(0.0)
            .min(max_scroll_rect.size.width.max(0.0));
        self.scroll_position.y = (self.scroll_position.y + y)
            .max(0.0)
            .min(max_scroll_rect.size.height.max(0.0));
    }

    /// Set the scroll state to a new position.
    ///
    /// `max_scroll_rect` is the *scroll range* rect (see [`ScrollState::add`]).
    pub const fn set(&mut self, x: f32, y: f32, max_scroll_rect: &LogicalRect) {
        self.scroll_position.x = x.max(0.0).min(max_scroll_rect.size.width.max(0.0));
        self.scroll_position.y = y.max(0.0).min(max_scroll_rect.size.height.max(0.0));
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
    // Explicit u8 -> variant table documenting every discriminant; `0 => Default`
    // intentionally mirrors the `_ => Default` fallback (the `#[default]` is 0).
    #[allow(clippy::match_same_arms)]
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
                // AUDIT: mask each field to its bit width so an out-of-range DomId /
                // NodeId can never bleed into an adjacent field (silent cross-field
                // corruption). Masking clamps consistently in debug and release —
                // a >16-bit DomId is absurd but must degrade gracefully, not panic.
                let dom_bits = (dom_id.inner as u64) & 0xFFFF;
                let node_bits = (container_node_id.index() as u64) & 0xFFFF_FFFF;
                let tag_value = (dom_bits << 48)
                    | (node_bits << 16)
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
#[allow(clippy::float_cmp)] // exact-value assertions on computed layout floats
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

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
    }

    #[test]
    fn scroll_state_clamps_to_content_minus_viewport() {
        // content 300 tall, viewport 100 tall -> max scroll offset = 200
        let max = max_scroll_rect(&OverflowingScrollNode {
            parent_rect: rect(0.0, 0.0, 100.0, 100.0),
            child_rect: rect(0.0, 0.0, 100.0, 300.0),
            ..Default::default()
        });
        assert_eq!(max.size.height, 200.0);

        let mut st = ScrollState::default();
        st.set(0.0, 999.0, &max);
        assert_eq!(st.scroll_position.y, 200.0); // not 300 (content would fully scroll away)
    }

    #[test]
    fn scroll_state_no_scroll_when_content_fits() {
        // content == viewport -> zero scroll range
        let max = max_scroll_rect(&OverflowingScrollNode {
            parent_rect: rect(0.0, 0.0, 100.0, 100.0),
            child_rect: rect(0.0, 0.0, 100.0, 100.0),
            ..Default::default()
        });
        let mut st = ScrollState::default();
        st.add(50.0, 50.0, &max);
        assert_eq!(st.scroll_position, LogicalPosition::zero());
    }

    #[test]
    fn scroll_state_nan_delta_does_not_poison() {
        let max = rect(0.0, 0.0, 100.0, 200.0);
        let mut st = ScrollState::default();
        st.add(f32::NAN, f32::NAN, &max);
        assert_eq!(st.scroll_position, LogicalPosition::zero());
    }

    #[test]
    fn selection_tag_out_of_range_domid_is_clamped_not_corrupting() {
        // A DomId > 0xFFFF must not bleed into the NodeId field. The masked
        // encode/decode round-trips within the 16-bit DomId window.
        let tag = HitTestTag::Selection {
            dom_id: DomId { inner: 0x1_0007 }, // exceeds 16 bits
            container_node_id: NodeId::new(5),
            text_run_index: 9,
        };
        let (value, ty) = tag.to_item_tag();
        assert_eq!(ty, TAG_TYPE_SELECTION);
        // NodeId field (bits 16..48) is exactly 5, uncorrupted by the overflow.
        assert_eq!((value >> 16) & 0xFFFF_FFFF, 5);
        // DomId field is the low 16 bits of the input (0x0007).
        assert_eq!(value >> 48, 0x0007);
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::unreadable_literal
)]
mod autotest_generated {
    use super::*;

    // ------------------------------------------------------------------
    // helpers
    // ------------------------------------------------------------------

    fn r(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
    }

    /// An `OverflowingScrollNode` with only the two rects that `max_scroll_rect`
    /// actually reads set; everything else stays at its `Default`.
    fn scroll_node_of(parent: LogicalRect, child: LogicalRect) -> OverflowingScrollNode {
        OverflowingScrollNode {
            parent_rect: parent,
            child_rect: child,
            ..Default::default()
        }
    }

    fn ext_id(raw: u64) -> ExternalScrollId {
        ExternalScrollId(raw, PipelineId::DUMMY)
    }

    const ALL_CURSORS: [CursorType; 21] = [
        CursorType::Default,
        CursorType::Pointer,
        CursorType::Text,
        CursorType::Crosshair,
        CursorType::Move,
        CursorType::NotAllowed,
        CursorType::Grab,
        CursorType::Grabbing,
        CursorType::EResize,
        CursorType::WResize,
        CursorType::NResize,
        CursorType::SResize,
        CursorType::EwResize,
        CursorType::NsResize,
        CursorType::NeswResize,
        CursorType::NwseResize,
        CursorType::ColResize,
        CursorType::RowResize,
        CursorType::Wait,
        CursorType::Help,
        CursorType::Progress,
    ];

    const ALL_SCROLLBAR_COMPONENTS: [ScrollbarComponent; 4] = [
        ScrollbarComponent::VerticalTrack,
        ScrollbarComponent::VerticalThumb,
        ScrollbarComponent::HorizontalTrack,
        ScrollbarComponent::HorizontalThumb,
    ];

    // ------------------------------------------------------------------
    // HitTest / FullHitTest — constructors + predicates
    // ------------------------------------------------------------------

    #[test]
    fn hit_test_empty_is_neutral_element() {
        let h = HitTest::empty();
        assert!(h.is_empty());
        assert_eq!(h.regular_hit_test_nodes.len(), 0);
        assert_eq!(h.scroll_hit_test_nodes.len(), 0);
        assert_eq!(h.scrollbar_hit_test_nodes.len(), 0);
        assert_eq!(h.cursor_hit_test_nodes.len(), 0);
        // repeated construction is stable / equal
        assert_eq!(HitTest::empty(), HitTest::empty());
    }

    #[test]
    fn hit_test_is_empty_false_if_any_single_map_is_populated() {
        // Each of the four maps must independently flip `is_empty()` to false,
        // otherwise a hit in (say) only the cursor map would be silently dropped.
        let mut a = HitTest::empty();
        a.regular_hit_test_nodes.insert(
            NodeId::ZERO,
            HitTestItem {
                point_in_viewport: LogicalPosition::zero(),
                point_relative_to_item: LogicalPosition::zero(),
                is_focusable: false,
                is_virtual_view_hit: None,
                hit_depth: 0,
            },
        );
        assert!(!a.is_empty());

        let mut b = HitTest::empty();
        b.scroll_hit_test_nodes.insert(
            NodeId::new(usize::MAX),
            ScrollHitTestItem {
                point_in_viewport: LogicalPosition::zero(),
                point_relative_to_item: LogicalPosition::zero(),
                scroll_node: OverflowingScrollNode::default(),
            },
        );
        assert!(!b.is_empty());

        let mut c = HitTest::empty();
        c.scrollbar_hit_test_nodes.insert(
            ScrollbarHitId::VerticalTrack(DomId::ROOT_ID, NodeId::ZERO),
            ScrollbarHitTestItem {
                point_in_viewport: LogicalPosition::zero(),
                point_relative_to_item: LogicalPosition::zero(),
                orientation: ScrollbarOrientation::Vertical,
            },
        );
        assert!(!c.is_empty());

        let mut d = HitTest::empty();
        d.cursor_hit_test_nodes.insert(
            NodeId::ZERO,
            CursorHitTestItem {
                cursor_type: CursorType::Default,
                hit_depth: u32::MAX,
                point_in_viewport: LogicalPosition::new(f32::NAN, f32::INFINITY),
            },
        );
        assert!(!d.is_empty());
    }

    #[test]
    fn full_hit_test_empty_is_empty_regardless_of_focused_node() {
        let none = FullHitTest::empty(None);
        assert!(none.is_empty());
        assert!(none.focused_node.is_none());

        // `is_empty()` is documented to ignore `focused_node` — a focused node
        // must NOT make an unhovered hit test look non-empty.
        let focused = FullHitTest::empty(Some(DomNodeId::ROOT));
        assert!(focused.is_empty());
        assert!(focused.focused_node.is_some());
        assert_eq!(focused.hovered_nodes.len(), 0);
    }

    #[test]
    fn full_hit_test_is_empty_false_once_a_dom_is_hovered() {
        let mut f = FullHitTest::empty(None);
        f.hovered_nodes.insert(DomId::ROOT_ID, HitTest::empty());
        // NOTE: an *empty* HitTest inserted under a DomId still counts as
        // "hovered" — `is_empty()` only looks at the outer map's length.
        assert!(!f.is_empty());
    }

    // ------------------------------------------------------------------
    // PipelineId / DocumentId / ExternalScrollId — constructors + serializers
    // ------------------------------------------------------------------

    #[test]
    fn pipeline_id_new_is_monotonic_and_second_field_is_zero() {
        let a = PipelineId::new();
        let b = PipelineId::new();
        let c = PipelineId::default();
        // The counter only ever moves forward (other tests in this binary may
        // bump it concurrently, so assert ordering, not adjacency).
        assert!(b.0 > a.0);
        assert!(c.0 > b.0);
        assert_eq!(a.1, 0);
        assert_eq!(b.1, 0);
        assert_eq!(PipelineId::DUMMY, PipelineId(0, 0));
    }

    #[test]
    fn pipeline_id_display_and_debug_agree_on_edge_values() {
        assert_eq!(alloc::format!("{}", PipelineId::DUMMY), "PipelineId(0, 0)");
        let maxed = PipelineId(u32::MAX, u32::MAX);
        let shown = alloc::format!("{maxed}");
        assert_eq!(shown, "PipelineId(4294967295, 4294967295)");
        assert_eq!(alloc::format!("{maxed:?}"), shown);
    }

    #[test]
    fn document_id_display_handles_min_and_max() {
        let zero = DocumentId {
            namespace_id: IdNamespace(0),
            id: 0,
        };
        let maxed = DocumentId {
            namespace_id: IdNamespace(u32::MAX),
            id: u32::MAX,
        };
        for d in [zero, maxed] {
            let shown = alloc::format!("{d}");
            assert!(!shown.is_empty());
            assert!(shown.starts_with("DocumentId {"));
            // Debug delegates to Display, so the two must be byte-identical.
            assert_eq!(alloc::format!("{d:?}"), shown);
        }
        assert!(alloc::format!("{maxed}").contains("4294967295"));
    }

    #[test]
    fn external_scroll_id_display_omits_the_pipeline_but_keys_stay_distinct() {
        let a = ExternalScrollId(7, PipelineId(1, 0));
        let b = ExternalScrollId(7, PipelineId(2, 0));

        // Display/Debug only render `.0` — they are deliberately lossy, so two
        // *different* IDs can format identically. Guard that this laxness never
        // leaks into equality/ordering (which is what BTreeMap keys rely on).
        assert_eq!(alloc::format!("{a}"), "ExternalScrollId(7)");
        assert_eq!(alloc::format!("{a:?}"), alloc::format!("{b:?}"));
        assert_ne!(a, b);

        let mut states = ScrollStates::new();
        states.0.insert(a, ScrollState::default());
        states.0.insert(b, ScrollState::default());
        assert_eq!(states.0.len(), 2);
    }

    #[test]
    fn external_scroll_id_display_no_panic_at_u64_max() {
        let shown = alloc::format!("{}", ExternalScrollId(u64::MAX, PipelineId::DUMMY));
        assert_eq!(shown, "ExternalScrollId(18446744073709551615)");
    }

    // ------------------------------------------------------------------
    // max_scroll_rect — numeric edge cases
    // ------------------------------------------------------------------

    #[test]
    fn max_scroll_rect_is_content_minus_viewport_and_keeps_origin() {
        let max = max_scroll_rect(&scroll_node_of(
            r(0.0, 0.0, 100.0, 100.0),
            r(-12.5, 7.25, 400.0, 300.0),
        ));
        assert_eq!(max.size.width, 300.0);
        assert_eq!(max.size.height, 200.0);
        // documented: the returned rect keeps `child_rect.origin`
        assert_eq!(max.origin.x, -12.5);
        assert_eq!(max.origin.y, 7.25);
    }

    #[test]
    fn max_scroll_rect_clamps_negative_range_to_zero() {
        // viewport larger than content -> nothing to scroll (must not go negative)
        let max = max_scroll_rect(&scroll_node_of(
            r(0.0, 0.0, 500.0, 500.0),
            r(0.0, 0.0, 100.0, 100.0),
        ));
        assert_eq!(max.size.width, 0.0);
        assert_eq!(max.size.height, 0.0);
    }

    #[test]
    fn max_scroll_rect_nan_and_infinite_sizes_do_not_produce_nan() {
        // NaN content size: `NaN - x` is NaN, and `.max(0.0)` collapses it to 0.0.
        let nan = max_scroll_rect(&scroll_node_of(
            r(0.0, 0.0, 10.0, 10.0),
            r(0.0, 0.0, f32::NAN, f32::NAN),
        ));
        assert!(!nan.size.width.is_nan());
        assert!(!nan.size.height.is_nan());
        assert_eq!(nan.size.width, 0.0);
        assert_eq!(nan.size.height, 0.0);

        // inf - inf == NaN -> also collapses to 0.0 rather than poisoning scroll.
        let both_inf = max_scroll_rect(&scroll_node_of(
            r(0.0, 0.0, f32::INFINITY, f32::INFINITY),
            r(0.0, 0.0, f32::INFINITY, f32::INFINITY),
        ));
        assert_eq!(both_inf.size.width, 0.0);
        assert_eq!(both_inf.size.height, 0.0);

        // Infinite content over a finite viewport stays infinite (unbounded scroll).
        let inf = max_scroll_rect(&scroll_node_of(
            r(0.0, 0.0, 10.0, 10.0),
            r(0.0, 0.0, f32::INFINITY, f32::INFINITY),
        ));
        assert!(inf.size.width.is_infinite() && inf.size.width.is_sign_positive());
        assert!(inf.size.height.is_infinite() && inf.size.height.is_sign_positive());
    }

    #[test]
    fn max_scroll_rect_at_float_extremes_does_not_panic() {
        let max = max_scroll_rect(&scroll_node_of(
            r(f32::MIN, f32::MIN, f32::MIN_POSITIVE, f32::MAX),
            r(f32::MAX, f32::MAX, f32::MAX, f32::MIN_POSITIVE),
        ));
        assert!(max.size.width >= 0.0);
        assert!(max.size.height >= 0.0);
        assert!(!max.size.width.is_nan());
        assert!(!max.size.height.is_nan());
    }

    // ------------------------------------------------------------------
    // ScrollState — numeric (zero / negative / min-max / overflow / NaN)
    // ------------------------------------------------------------------

    #[test]
    fn scroll_state_default_and_get_round_trip() {
        let st = ScrollState::default();
        assert_eq!(st.get(), LogicalPosition::zero());

        let st = ScrollState {
            scroll_position: LogicalPosition::new(3.5, -4.25),
        };
        // `get` is a pure accessor: it must not clamp or normalize.
        assert_eq!(st.get().x, 3.5);
        assert_eq!(st.get().y, -4.25);
    }

    #[test]
    fn scroll_state_set_zero_is_identity_within_range() {
        let max = r(0.0, 0.0, 100.0, 200.0);
        let mut st = ScrollState::default();
        st.set(0.0, 0.0, &max);
        assert_eq!(st.get(), LogicalPosition::zero());

        st.set(50.0, 150.0, &max);
        assert_eq!(st.get().x, 50.0);
        assert_eq!(st.get().y, 150.0);
    }

    #[test]
    fn scroll_state_set_clamps_negative_to_zero_and_overshoot_to_max() {
        let max = r(0.0, 0.0, 100.0, 200.0);
        let mut st = ScrollState::default();

        st.set(-1.0, -f32::MAX, &max);
        assert_eq!(st.get().x, 0.0);
        assert_eq!(st.get().y, 0.0);

        st.set(f32::MAX, f32::MAX, &max);
        assert_eq!(st.get().x, 100.0);
        assert_eq!(st.get().y, 200.0);

        st.set(f32::INFINITY, f32::INFINITY, &max);
        assert_eq!(st.get().x, 100.0);
        assert_eq!(st.get().y, 200.0);

        st.set(f32::NEG_INFINITY, f32::NEG_INFINITY, &max);
        assert_eq!(st.get().x, 0.0);
        assert_eq!(st.get().y, 0.0);
    }

    #[test]
    fn scroll_state_set_nan_position_collapses_to_zero() {
        let max = r(0.0, 0.0, 100.0, 200.0);
        let mut st = ScrollState {
            scroll_position: LogicalPosition::new(40.0, 40.0),
        };
        st.set(f32::NAN, f32::NAN, &max);
        assert!(!st.get().x.is_nan());
        assert!(!st.get().y.is_nan());
        assert_eq!(st.get(), LogicalPosition::zero());
    }

    #[test]
    fn scroll_state_set_nan_max_range_collapses_to_zero() {
        // A NaN *range* must not leak into the position either: `NaN.max(0.0)`
        // is 0.0, so the position clamps to 0 rather than becoming NaN.
        let max = r(0.0, 0.0, f32::NAN, f32::NAN);
        let mut st = ScrollState::default();
        st.set(75.0, 75.0, &max);
        assert_eq!(st.get(), LogicalPosition::zero());
    }

    #[test]
    fn scroll_state_set_negative_max_range_collapses_to_zero() {
        let max = r(0.0, 0.0, -50.0, -50.0);
        let mut st = ScrollState::default();
        st.set(10.0, 10.0, &max);
        assert_eq!(st.get(), LogicalPosition::zero());
    }

    #[test]
    fn scroll_state_add_accumulates_then_saturates_at_the_range() {
        let max = r(0.0, 0.0, 100.0, 200.0);
        let mut st = ScrollState::default();
        st.add(30.0, 30.0, &max);
        st.add(30.0, 30.0, &max);
        assert_eq!(st.get().x, 60.0);
        assert_eq!(st.get().y, 60.0);

        // Deltas far beyond the f32 range must clamp to the max offset, and
        // re-applying them must not push the position past it (or to +inf).
        st.add(f32::MAX, f32::MAX, &max);
        st.add(f32::MAX, f32::MAX, &max);
        assert_eq!(st.get().x, 100.0);
        assert_eq!(st.get().y, 200.0);
        assert!(st.get().x.is_finite() && st.get().y.is_finite());
    }

    #[test]
    fn scroll_state_add_negative_underflow_clamps_to_zero() {
        let max = r(0.0, 0.0, 100.0, 200.0);
        let mut st = ScrollState {
            scroll_position: LogicalPosition::new(10.0, 10.0),
        };
        st.add(f32::MIN, f32::MIN, &max);
        assert_eq!(st.get(), LogicalPosition::zero());

        st.add(f32::NEG_INFINITY, f32::NEG_INFINITY, &max);
        assert_eq!(st.get(), LogicalPosition::zero());
        assert!(st.get().x.is_finite() && st.get().y.is_finite());
    }

    #[test]
    fn scroll_state_add_inf_minus_inf_does_not_poison_position() {
        // Worst case: an unbounded scroll range lets the position itself become
        // +inf, and the *next* delta is -inf -> `inf + -inf == NaN`. The `.max(0.0)`
        // clamp must still collapse that NaN back to a defined 0.0.
        let unbounded = r(0.0, 0.0, f32::INFINITY, f32::INFINITY);
        let mut st = ScrollState::default();
        st.add(f32::INFINITY, f32::INFINITY, &unbounded);
        assert!(st.get().x.is_infinite());

        st.add(f32::NEG_INFINITY, f32::NEG_INFINITY, &unbounded);
        assert!(!st.get().x.is_nan());
        assert!(!st.get().y.is_nan());
        assert_eq!(st.get(), LogicalPosition::zero());
    }

    #[test]
    fn scroll_state_add_nan_delta_from_nonzero_position_resets_to_zero() {
        let max = r(0.0, 0.0, 100.0, 200.0);
        let mut st = ScrollState {
            scroll_position: LogicalPosition::new(50.0, 50.0),
        };
        st.add(f32::NAN, 0.0, &max);
        // `50 + NaN == NaN`, `NaN.max(0.0) == 0.0`: the delta is discarded *and*
        // the previously-good X position is lost. Defined, but lossy — pinned here.
        assert_eq!(st.get().x, 0.0);
        assert_eq!(st.get().y, 50.0);
    }

    // ------------------------------------------------------------------
    // ScrollStates — map behaviour
    // ------------------------------------------------------------------

    #[test]
    fn scroll_states_new_is_empty_and_lookup_misses_return_none() {
        let states = ScrollStates::new();
        assert_eq!(states.0.len(), 0);
        assert!(states.get_scroll_position(&ext_id(0)).is_none());
        assert!(states.get_scroll_position(&ext_id(u64::MAX)).is_none());
    }

    #[test]
    fn scroll_states_set_scroll_position_creates_entry_and_clamps() {
        let node = scroll_node_of(r(0.0, 0.0, 100.0, 100.0), r(0.0, 0.0, 100.0, 300.0));
        let mut states = ScrollStates::new();

        states.set_scroll_position(&node, LogicalPosition::new(999.0, 999.0));
        let pos = states
            .get_scroll_position(&node.parent_external_scroll_id)
            .expect("entry must exist after set_scroll_position");
        assert_eq!(states.0.len(), 1);
        assert_eq!(pos.x, 0.0); // no horizontal overflow -> zero range
        assert_eq!(pos.y, 200.0); // 300 content - 100 viewport

        // Re-setting the same node updates in place rather than adding a key.
        states.set_scroll_position(&node, LogicalPosition::new(-5.0, -5.0));
        assert_eq!(states.0.len(), 1);
        let pos = states
            .get_scroll_position(&node.parent_external_scroll_id)
            .unwrap();
        assert_eq!(pos, LogicalPosition::zero());
    }

    #[test]
    fn scroll_states_set_scroll_position_with_nan_stays_defined() {
        let node = scroll_node_of(r(0.0, 0.0, 100.0, 100.0), r(0.0, 0.0, 100.0, 300.0));
        let mut states = ScrollStates::new();
        states.set_scroll_position(&node, LogicalPosition::new(f32::NAN, f32::NAN));
        let pos = states
            .get_scroll_position(&node.parent_external_scroll_id)
            .unwrap();
        assert!(!pos.x.is_nan() && !pos.y.is_nan());
        assert_eq!(pos, LogicalPosition::zero());
    }

    #[test]
    fn scroll_states_scroll_node_accumulates_and_saturates() {
        let node = scroll_node_of(r(0.0, 0.0, 100.0, 100.0), r(0.0, 0.0, 400.0, 300.0));
        let mut states = ScrollStates::new();

        states.scroll_node(&node, 0.0, 0.0);
        assert_eq!(
            states
                .get_scroll_position(&node.parent_external_scroll_id)
                .unwrap(),
            LogicalPosition::zero()
        );

        states.scroll_node(&node, 10.0, 10.0);
        states.scroll_node(&node, 10.0, 10.0);
        let pos = states
            .get_scroll_position(&node.parent_external_scroll_id)
            .unwrap();
        assert_eq!(pos.x, 20.0);
        assert_eq!(pos.y, 20.0);

        // Saturate: max range is (400-100, 300-100) = (300, 200).
        states.scroll_node(&node, f32::MAX, f32::INFINITY);
        let pos = states
            .get_scroll_position(&node.parent_external_scroll_id)
            .unwrap();
        assert_eq!(pos.x, 300.0);
        assert_eq!(pos.y, 200.0);

        // ...and NaN deltas never corrupt the stored position.
        states.scroll_node(&node, f32::NAN, f32::NAN);
        let pos = states
            .get_scroll_position(&node.parent_external_scroll_id)
            .unwrap();
        assert!(!pos.x.is_nan() && !pos.y.is_nan());
        assert_eq!(states.0.len(), 1);
    }

    #[test]
    fn scroll_states_keys_are_pipeline_qualified() {
        // Same raw scroll tag, different pipeline => two independent scroll states.
        let a = OverflowingScrollNode {
            parent_rect: r(0.0, 0.0, 10.0, 10.0),
            child_rect: r(0.0, 0.0, 10.0, 100.0),
            parent_external_scroll_id: ExternalScrollId(1, PipelineId(1, 0)),
            ..Default::default()
        };
        let b = OverflowingScrollNode {
            parent_external_scroll_id: ExternalScrollId(1, PipelineId(2, 0)),
            ..a
        };

        let mut states = ScrollStates::new();
        states.scroll_node(&a, 0.0, 25.0);
        states.scroll_node(&b, 0.0, 50.0);
        assert_eq!(states.0.len(), 2);
        assert_eq!(
            states
                .get_scroll_position(&a.parent_external_scroll_id)
                .unwrap()
                .y,
            25.0
        );
        assert_eq!(
            states
                .get_scroll_position(&b.parent_external_scroll_id)
                .unwrap()
                .y,
            50.0
        );
    }

    // ------------------------------------------------------------------
    // ScrollbarComponent / CursorType — from_u8 over the whole domain
    // ------------------------------------------------------------------

    #[test]
    fn scrollbar_component_from_u8_covers_all_256_values() {
        for v in 0u8..=255 {
            match ScrollbarComponent::from_u8(v) {
                Some(c) => {
                    assert!(v < 4, "value {v} unexpectedly decoded to {c:?}");
                    // discriminant round-trips
                    assert_eq!(c as u8, v);
                }
                None => assert!(v >= 4, "value {v} should have decoded"),
            }
        }
        for c in ALL_SCROLLBAR_COMPONENTS {
            assert_eq!(ScrollbarComponent::from_u8(c as u8), Some(c));
        }
    }

    #[test]
    fn cursor_type_from_u8_is_total_and_unknown_falls_back_to_default() {
        for v in 0u8..=255 {
            let c = CursorType::from_u8(v);
            if v <= 20 {
                assert_eq!(c as u8, v, "known discriminant {v} must round-trip");
            } else {
                // Out-of-range bytes must degrade to Default, never panic.
                assert_eq!(c, CursorType::Default, "unknown byte {v} must be Default");
            }
        }
        assert_eq!(CursorType::default(), CursorType::Default);
        for c in ALL_CURSORS {
            assert_eq!(CursorType::from_u8(c as u8), c);
        }
    }

    // ------------------------------------------------------------------
    // HitTestTag — encode/decode round-trips
    // ------------------------------------------------------------------

    #[test]
    fn dom_node_tag_round_trips_at_u64_boundaries() {
        for inner in [0u64, 1, 673, u64::from(u32::MAX), u64::MAX] {
            let tag = HitTestTag::DomNode {
                tag_id: TagId { inner },
            };
            let item = tag.to_item_tag();
            assert_eq!(item, (inner, TAG_TYPE_DOM_NODE));
            assert_eq!(HitTestTag::from_item_tag(item), Some(tag));
            assert_eq!(tag.as_dom_node().unwrap().inner, inner);
        }
    }

    #[test]
    fn scrollbar_tag_round_trips_for_every_component_at_field_boundaries() {
        let ids = [
            (0usize, 0usize),
            (0, u32::MAX as usize),
            (u32::MAX as usize, 0),
            (u32::MAX as usize, u32::MAX as usize),
        ];
        for component in ALL_SCROLLBAR_COMPONENTS {
            for (dom, node) in ids {
                let tag = HitTestTag::Scrollbar {
                    dom_id: DomId { inner: dom },
                    node_id: NodeId::new(node),
                    component,
                };
                let item = tag.to_item_tag();
                assert_eq!(item.1 & 0xFF00, TAG_TYPE_SCROLLBAR);
                assert_eq!(
                    HitTestTag::from_item_tag(item),
                    Some(tag),
                    "scrollbar round-trip failed for dom={dom} node={node} {component:?}"
                );
            }
        }
    }

    #[test]
    fn cursor_tag_round_trips_for_every_cursor_type() {
        for cursor_type in ALL_CURSORS {
            let tag = HitTestTag::Cursor {
                dom_id: DomId { inner: u32::MAX as usize },
                node_id: NodeId::new(u32::MAX as usize),
                cursor_type,
            };
            let item = tag.to_item_tag();
            assert_eq!(item.1 & 0xFF00, TAG_TYPE_CURSOR);
            assert_eq!(
                HitTestTag::from_item_tag(item),
                Some(tag),
                "cursor round-trip failed for {cursor_type:?}"
            );
        }
    }

    #[test]
    fn selection_tag_round_trips_at_every_field_boundary() {
        let cases = [
            (0usize, 0usize, 0u16),
            (0xFFFF, u32::MAX as usize, u16::MAX),
            (1, 1, 1),
            (0xFFFF, 0, u16::MAX),
        ];
        for (dom, node, run) in cases {
            let tag = HitTestTag::Selection {
                dom_id: DomId { inner: dom },
                container_node_id: NodeId::new(node),
                text_run_index: run,
            };
            let item = tag.to_item_tag();
            assert_eq!(item.1, TAG_TYPE_SELECTION);
            assert_eq!(
                HitTestTag::from_item_tag(item),
                Some(tag),
                "selection round-trip failed for dom={dom} node={node} run={run}"
            );
        }
    }

    #[test]
    fn selection_tag_oversized_node_id_is_masked_not_bled_into_dom_id() {
        // Mirror of the DomId-overflow guard, from the other side: a
        // container_node_id wider than 32 bits must be masked, leaving the
        // DomId field (bits 48..64) intact.
        let tag = HitTestTag::Selection {
            dom_id: DomId { inner: 0x00AB },
            container_node_id: NodeId::new(u32::MAX as usize),
            text_run_index: 0xBEEF,
        };
        let (value, _) = tag.to_item_tag();
        assert_eq!(value >> 48, 0x00AB);
        assert_eq!((value >> 16) & 0xFFFF_FFFF, u64::from(u32::MAX));
        assert_eq!(value & 0xFFFF, 0xBEEF);
    }

    #[test]
    fn tag_namespaces_do_not_collide_on_identical_payloads() {
        // The same numeric payload under four different type markers must decode
        // to four different variants — that is the whole point of the namespaces.
        let payload = 0x0000_0001_0000_0002u64;
        let decoded: [HitTestTag; 4] = [
            HitTestTag::from_item_tag((payload, TAG_TYPE_DOM_NODE)).unwrap(),
            HitTestTag::from_item_tag((payload, TAG_TYPE_SCROLLBAR)).unwrap(),
            HitTestTag::from_item_tag((payload, TAG_TYPE_SELECTION)).unwrap(),
            HitTestTag::from_item_tag((payload, TAG_TYPE_CURSOR)).unwrap(),
        ];
        assert!(decoded[0].is_dom_node());
        assert!(decoded[1].is_scrollbar());
        assert!(decoded[2].is_selection());
        assert!(decoded[3].is_cursor());
        for (i, a) in decoded.iter().enumerate() {
            for b in decoded.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
    }

    // ------------------------------------------------------------------
    // HitTestTag::from_item_tag — malformed / unknown input
    // ------------------------------------------------------------------

    #[test]
    fn from_item_tag_sweeps_every_type_marker_without_panicking() {
        // Exhaustive sweep of the upper byte x a few lower bytes: every
        // combination must either decode or return None — never panic.
        for hi in 0u16..=0xFF {
            for lo in [0u16, 1, 3, 4, 20, 21, 0xFF] {
                let tag_type = (hi << 8) | lo;
                let decoded = HitTestTag::from_item_tag((0xDEAD_BEEF_CAFE_BABE, tag_type));
                let expected_some = match hi {
                    0x00 => tag_type == 0,      // legacy DOM tags only
                    0x01 | 0x03 | 0x04 => true, // DomNode / Selection / Cursor
                    0x02 => lo < 4,             // Scrollbar: component must be valid
                    _ => false,                 // 0x05.. (incl. SCROLL_CONTAINER) is unknown
                };
                assert_eq!(
                    decoded.is_some(),
                    expected_some,
                    "tag_type {tag_type:#06x} decoded to {decoded:?}"
                );
            }
        }
    }

    #[test]
    fn from_item_tag_rejects_invalid_scrollbar_components() {
        for lo in 4u16..=0xFF {
            let item = (0u64, TAG_TYPE_SCROLLBAR | lo);
            assert!(
                HitTestTag::from_item_tag(item).is_none(),
                "scrollbar component byte {lo} should be rejected"
            );
        }
    }

    #[test]
    fn from_item_tag_unknown_cursor_byte_degrades_to_default() {
        // Unlike scrollbars, an unknown cursor byte is *not* an error: it maps
        // to CursorType::Default so a corrupt tag still yields a usable cursor.
        for lo in [21u16, 100, 0xFF] {
            let decoded = HitTestTag::from_item_tag((0, TAG_TYPE_CURSOR | lo)).unwrap();
            assert_eq!(
                decoded.as_cursor().unwrap().2,
                CursorType::Default,
                "cursor byte {lo} should fall back to Default"
            );
        }
    }

    #[test]
    fn from_item_tag_dom_node_ignores_the_lower_byte() {
        // Only the upper byte selects the namespace; junk in the lower byte of a
        // DOM-node tag must not flip it to another variant or drop it.
        for lo in [0u16, 1, 0x7F, 0xFF] {
            let decoded = HitTestTag::from_item_tag((99, TAG_TYPE_DOM_NODE | lo)).unwrap();
            assert!(decoded.is_dom_node());
            assert_eq!(decoded.as_dom_node().unwrap().inner, 99);
        }
    }

    #[test]
    fn from_item_tag_scroll_container_marker_is_not_decodable() {
        // TAG_TYPE_SCROLL_CONTAINER has no HitTestTag variant — it must be
        // rejected rather than silently aliased onto another namespace.
        assert!(HitTestTag::from_item_tag((0, TAG_TYPE_SCROLL_CONTAINER)).is_none());
        assert!(HitTestTag::from_item_tag((u64::MAX, TAG_TYPE_SCROLL_CONTAINER)).is_none());
    }

    #[test]
    fn from_item_tag_legacy_zero_type_only_matches_exact_zero() {
        // tag_type == 0 is the legacy DOM-node escape hatch...
        let legacy = HitTestTag::from_item_tag((u64::MAX, 0)).unwrap();
        assert_eq!(legacy.as_dom_node().unwrap().inner, u64::MAX);
        // ...but a *nonzero* lower byte with a zero upper byte is not a known
        // namespace and must be rejected, not treated as legacy.
        for lo in 1u16..=0xFF {
            assert!(
                HitTestTag::from_item_tag((0, lo)).is_none(),
                "tag_type {lo:#06x} must not be treated as a legacy DOM tag"
            );
        }
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn scrollbar_encode_keeps_decoded_fields_inside_their_bit_windows() {
        // A NodeId wider than 32 bits is absurd, but must degrade without
        // panicking (no shift/`as` overflow) and must not produce out-of-window
        // decoded values. NOTE: unlike `Selection`, the `Scrollbar`/`Cursor`
        // encoders do NOT mask their fields, so such an id is lossy — see report.
        let tag = HitTestTag::Scrollbar {
            dom_id: DomId { inner: 0 },
            node_id: NodeId::new(1usize << 32),
            component: ScrollbarComponent::VerticalTrack,
        };
        let (value, ty) = tag.to_item_tag();
        assert_eq!(ty & 0xFF00, TAG_TYPE_SCROLLBAR);

        let decoded = HitTestTag::from_item_tag((value, ty)).expect("must still decode");
        let (dom, node, component) = decoded.as_scrollbar().unwrap();
        assert!(dom.inner <= u32::MAX as usize);
        assert!(node.index() <= u32::MAX as usize);
        assert_eq!(component, ScrollbarComponent::VerticalTrack);
    }

    // ------------------------------------------------------------------
    // HitTestTag — predicates + accessors
    // ------------------------------------------------------------------

    fn sample_tags() -> [HitTestTag; 4] {
        [
            HitTestTag::DomNode {
                tag_id: TagId { inner: 7 },
            },
            HitTestTag::Scrollbar {
                dom_id: DomId { inner: 1 },
                node_id: NodeId::new(2),
                component: ScrollbarComponent::HorizontalThumb,
            },
            HitTestTag::Cursor {
                dom_id: DomId { inner: 3 },
                node_id: NodeId::new(4),
                cursor_type: CursorType::Grabbing,
            },
            HitTestTag::Selection {
                dom_id: DomId { inner: 5 },
                container_node_id: NodeId::new(6),
                text_run_index: 8,
            },
        ]
    }

    #[test]
    fn exactly_one_predicate_is_true_per_variant() {
        for tag in sample_tags() {
            let flags = [
                tag.is_dom_node(),
                tag.is_scrollbar(),
                tag.is_cursor(),
                tag.is_selection(),
            ];
            assert_eq!(
                flags.iter().filter(|b| **b).count(),
                1,
                "predicates not mutually exclusive for {tag:?}"
            );
        }
    }

    #[test]
    fn accessors_agree_with_predicates_and_return_none_otherwise() {
        for tag in sample_tags() {
            assert_eq!(tag.as_dom_node().is_some(), tag.is_dom_node());
            assert_eq!(tag.as_scrollbar().is_some(), tag.is_scrollbar());
            assert_eq!(tag.as_cursor().is_some(), tag.is_cursor());
            assert_eq!(tag.as_selection().is_some(), tag.is_selection());

            // exactly one accessor yields a value
            let some_count = usize::from(tag.as_dom_node().is_some())
                + usize::from(tag.as_scrollbar().is_some())
                + usize::from(tag.as_cursor().is_some())
                + usize::from(tag.as_selection().is_some());
            assert_eq!(some_count, 1, "accessor overlap for {tag:?}");
        }
    }

    #[test]
    fn accessors_return_the_constructed_payload() {
        let [dom, scrollbar, cursor, selection] = sample_tags();

        assert_eq!(dom.as_dom_node().unwrap().inner, 7);

        let (d, n, c) = scrollbar.as_scrollbar().unwrap();
        assert_eq!((d.inner, n.index()), (1, 2));
        assert_eq!(c, ScrollbarComponent::HorizontalThumb);

        let (d, n, c) = cursor.as_cursor().unwrap();
        assert_eq!((d.inner, n.index()), (3, 4));
        assert_eq!(c, CursorType::Grabbing);

        let (d, n, run) = selection.as_selection().unwrap();
        assert_eq!((d.inner, n.index()), (5, 6));
        assert_eq!(run, 8);
    }

    // ------------------------------------------------------------------
    // HitTestTag — Display
    // ------------------------------------------------------------------

    #[test]
    fn hit_test_tag_display_is_non_empty_and_variant_tagged() {
        let [dom, scrollbar, cursor, selection] = sample_tags();
        for (tag, prefix) in [
            (dom, "DomNode("),
            (scrollbar, "Scrollbar("),
            (cursor, "Cursor("),
            (selection, "Selection("),
        ] {
            let shown = alloc::format!("{tag}");
            assert!(!shown.is_empty());
            assert!(
                shown.starts_with(prefix),
                "expected {shown:?} to start with {prefix:?}"
            );
        }
    }

    #[test]
    fn hit_test_tag_display_handles_extreme_ids() {
        let tags = [
            HitTestTag::DomNode {
                tag_id: TagId { inner: u64::MAX },
            },
            HitTestTag::Scrollbar {
                dom_id: DomId { inner: usize::MAX },
                node_id: NodeId::new(usize::MAX),
                component: ScrollbarComponent::VerticalThumb,
            },
            HitTestTag::Cursor {
                dom_id: DomId { inner: usize::MAX },
                node_id: NodeId::new(usize::MAX),
                cursor_type: CursorType::Progress,
            },
            HitTestTag::Selection {
                dom_id: DomId { inner: usize::MAX },
                container_node_id: NodeId::new(usize::MAX),
                text_run_index: u16::MAX,
            },
        ];
        for tag in tags {
            assert!(!alloc::format!("{tag}").is_empty());
            assert!(!alloc::format!("{tag:?}").is_empty());
            // encoding an extreme tag must not panic either
            let _ = tag.to_item_tag();
        }
    }
}
