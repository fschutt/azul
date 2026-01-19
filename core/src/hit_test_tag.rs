//! Type-safe hit-test tag system for WebRender integration.
//!
//! This module provides a type-safe abstraction over WebRender's `ItemTag` (u64, u16) system.
//! Instead of using raw bit manipulation, we use explicit types to represent different kinds
//! of hit-test targets.
//!
//! ## Tag Types
//!
//! WebRender uses `ItemTag = (u64, u16)` for hit-testing. We encode different tag types
//! in the **upper byte of the u16 field** (tag.1) to distinguish between:
//!
//! - **DOM Node Tags**: For regular interactive DOM elements (callbacks, focus, cursor)
//! - **Scrollbar Tags**: For scrollbar components (track, thumb, buttons)
//!
//! ## Encoding Scheme
//!
//! ```text
//! WebRender ItemTag = (u64, u16)
//!
//! For DOM Node Tags:
//!   tag.0 = TagId.inner (sequential counter 1, 2, 3, ...)
//!   tag.1 = TAG_TYPE_DOM_NODE (0x0100)
//!
//! For Scrollbar Tags:
//!   tag.0 = DomId (32 bits) << 32 | NodeId (32 bits)
//!   tag.1 = TAG_TYPE_SCROLLBAR (0x0200) | ScrollbarComponent (2 bits in lower byte)
//!
//! Tag type identification via tag.1 upper byte:
//!   0x01 = DOM Node
//!   0x02 = Scrollbar
//!   0x03 = Reserved (future: selection handles, resize handles, etc.)
//! ```
//!
//! This approach ensures:
//! - **No bit-level conflicts**: Each tag type has its own marker in tag.1
//! - **Type safety**: Explicit enum variants for each tag type
//! - **Easy debugging**: Tag type is immediately visible in tag.1
//! - **Extensibility**: New tag types can be added without changing existing code

use crate::dom::{DomId, TagId};
use crate::id::NodeId;
use core::fmt;

// ============================================================================
// Tag Type Markers (stored in upper byte of ItemTag.1)
// ============================================================================

/// Marker for DOM node tags (regular UI elements with callbacks, focus, etc.)
pub const TAG_TYPE_DOM_NODE: u16 = 0x0100;

/// Marker for scrollbar component tags
pub const TAG_TYPE_SCROLLBAR: u16 = 0x0200;

/// Marker for cursor hit-test areas (determines which cursor icon to show)
/// These are separate from DOM node tags to allow efficient cursor resolution
/// without iterating over all DOM nodes.
pub const TAG_TYPE_CURSOR: u16 = 0x0300;

/// Marker for text selection hit-test areas (determines text selection regions)
/// These are pushed for text runs to enable text selection without affecting
/// other hit-test logic.
pub const TAG_TYPE_SELECTION: u16 = 0x0400;

/// Reserved for future use (e.g., resize handles, drag-drop targets)
pub const TAG_TYPE_RESERVED: u16 = 0x0500;

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
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(ScrollbarComponent::VerticalTrack),
            1 => Some(ScrollbarComponent::VerticalThumb),
            2 => Some(ScrollbarComponent::HorizontalTrack),
            3 => Some(ScrollbarComponent::HorizontalThumb),
            _ => None,
        }
    }

    /// Check if this is a vertical component
    pub fn is_vertical(&self) -> bool {
        matches!(
            self,
            ScrollbarComponent::VerticalTrack | ScrollbarComponent::VerticalThumb
        )
    }

    /// Check if this is a thumb (draggable handle)
    pub fn is_thumb(&self) -> bool {
        matches!(
            self,
            ScrollbarComponent::VerticalThumb | ScrollbarComponent::HorizontalThumb
        )
    }
}

// ============================================================================
// WebRender Hit-Test Tag (unified type-safe representation)
// ============================================================================

/// Unified, type-safe representation of a WebRender hit-test tag.
///
/// This enum represents all possible types of hit-test targets. Each variant
/// can be encoded to and decoded from WebRender's `(u64, u16)` ItemTag format.
///
/// ## Namespace Separation
///
/// Different tag types are kept in separate namespaces to:
/// - Enable efficient hit-test queries (only iterate over relevant tags)
/// - Get automatic depth sorting from WebRender per namespace
/// - Prevent accidental collisions between different hit-test purposes
///
/// | Namespace | Purpose                              |
/// |-----------|--------------------------------------|
/// | 0x0100    | DOM nodes (callbacks, focus, hover)  |
/// | 0x0200    | Scrollbar components                 |
/// | 0x0300    | Cursor areas (cursor icon display)   |
/// | 0x0400    | Selection areas (text selection)     |
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HitTestTag {
    /// A regular DOM node (button, div, text container, etc.)
    ///
    /// These are nodes that have callbacks, are focusable, or have hover styles.
    /// The TagId is a sequential counter assigned during DOM styling.
    DomNode {
        /// The unique tag ID assigned to this DOM node
        tag_id: TagId,
    },

    /// A scrollbar component (track or thumb)
    ///
    /// Each scrollable container can have up to 2 scrollbars.
    /// The scrollbar is identified by the DomId and NodeId of the scrollable container.
    Scrollbar {
        /// The DOM that contains the scrollable container
        dom_id: DomId,
        /// The NodeId of the scrollable container (not the scrollbar itself)
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
        /// The NodeId of the element with the cursor property
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
        /// The NodeId of the text container (not the Text node itself)
        container_node_id: NodeId,
        /// The index of the text run within the container (for multi-line text)
        text_run_index: u16,
    },
}

/// Cursor type encoded in cursor hit-test tags.
/// Stored in the lower byte of the ItemTag.1 field.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
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
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => CursorType::Default,
            1 => CursorType::Pointer,
            2 => CursorType::Text,
            3 => CursorType::Crosshair,
            4 => CursorType::Move,
            5 => CursorType::NotAllowed,
            6 => CursorType::Grab,
            7 => CursorType::Grabbing,
            8 => CursorType::EResize,
            9 => CursorType::WResize,
            10 => CursorType::NResize,
            11 => CursorType::SResize,
            12 => CursorType::EwResize,
            13 => CursorType::NsResize,
            14 => CursorType::NeswResize,
            15 => CursorType::NwseResize,
            16 => CursorType::ColResize,
            17 => CursorType::RowResize,
            18 => CursorType::Wait,
            19 => CursorType::Help,
            20 => CursorType::Progress,
            _ => CursorType::Default,
        }
    }
}

impl HitTestTag {
    /// Encode this tag to WebRender's ItemTag format.
    ///
    /// Returns `(u64, u16)` suitable for passing to WebRender's `push_hit_test`.
    pub fn to_item_tag(&self) -> (u64, u16) {
        match self {
            HitTestTag::DomNode { tag_id } => {
                // tag.0 = TagId.inner (the sequential counter)
                // tag.1 = TAG_TYPE_DOM_NODE marker
                (tag_id.inner, TAG_TYPE_DOM_NODE)
            }
            HitTestTag::Scrollbar {
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
            HitTestTag::Cursor {
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
            HitTestTag::Selection {
                dom_id,
                container_node_id,
                text_run_index,
            } => {
                // tag.0 = DomId (upper 16 bits) | NodeId (middle 32 bits) | text_run_index (lower 16 bits)
                let tag_value = ((dom_id.inner as u64) << 48)
                    | ((container_node_id.index() as u64) << 16)
                    | (*text_run_index as u64);
                (tag_value, TAG_TYPE_SELECTION)
            }
        }
    }

    /// Decode a WebRender ItemTag back to a typed HitTestTag.
    ///
    /// Returns `None` if the tag format is invalid or unrecognized.
    pub fn from_item_tag(tag: (u64, u16)) -> Option<Self> {
        let (tag_value, tag_type) = tag;

        // Extract tag type from upper byte
        let type_marker = tag_type & 0xFF00;

        match type_marker {
            TAG_TYPE_DOM_NODE => {
                // DOM node tag: tag.0 is the TagId
                Some(HitTestTag::DomNode {
                    tag_id: TagId { inner: tag_value },
                })
            }
            TAG_TYPE_SCROLLBAR => {
                // Scrollbar tag: decode DomId, NodeId, and component
                let dom_id = DomId {
                    inner: ((tag_value >> 32) & 0xFFFFFFFF) as usize,
                };
                let node_id = NodeId::new((tag_value & 0xFFFFFFFF) as usize);
                let component_value = (tag_type & 0x00FF) as u8;
                let component = ScrollbarComponent::from_u8(component_value)?;

                Some(HitTestTag::Scrollbar {
                    dom_id,
                    node_id,
                    component,
                })
            }
            TAG_TYPE_CURSOR => {
                // Cursor tag: decode DomId, NodeId, and cursor type
                let dom_id = DomId {
                    inner: ((tag_value >> 32) & 0xFFFFFFFF) as usize,
                };
                let node_id = NodeId::new((tag_value & 0xFFFFFFFF) as usize);
                let cursor_value = (tag_type & 0x00FF) as u8;
                let cursor_type = CursorType::from_u8(cursor_value);

                Some(HitTestTag::Cursor {
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
                let container_node_id = NodeId::new(((tag_value >> 16) & 0xFFFFFFFF) as usize);
                let text_run_index = (tag_value & 0xFFFF) as u16;

                Some(HitTestTag::Selection {
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
                    Some(HitTestTag::DomNode {
                        tag_id: TagId { inner: tag_value },
                    })
                } else {
                    None
                }
            }
        }
    }

    /// Check if this is a DOM node tag
    pub fn is_dom_node(&self) -> bool {
        matches!(self, HitTestTag::DomNode { .. })
    }

    /// Check if this is a scrollbar tag
    pub fn is_scrollbar(&self) -> bool {
        matches!(self, HitTestTag::Scrollbar { .. })
    }

    /// Check if this is a cursor tag
    pub fn is_cursor(&self) -> bool {
        matches!(self, HitTestTag::Cursor { .. })
    }

    /// Check if this is a selection tag
    pub fn is_selection(&self) -> bool {
        matches!(self, HitTestTag::Selection { .. })
    }

    /// Get the TagId if this is a DOM node tag
    pub fn as_dom_node(&self) -> Option<TagId> {
        match self {
            HitTestTag::DomNode { tag_id } => Some(*tag_id),
            _ => None,
        }
    }

    /// Get cursor info if this is a cursor tag
    pub fn as_cursor(&self) -> Option<(DomId, NodeId, CursorType)> {
        match self {
            HitTestTag::Cursor {
                dom_id,
                node_id,
                cursor_type,
            } => Some((*dom_id, *node_id, *cursor_type)),
            _ => None,
        }
    }

    /// Get selection info if this is a selection tag
    pub fn as_selection(&self) -> Option<(DomId, NodeId, u16)> {
        match self {
            HitTestTag::Selection {
                dom_id,
                container_node_id,
                text_run_index,
            } => Some((*dom_id, *container_node_id, *text_run_index)),
            _ => None,
        }
    }

    /// Get scrollbar info if this is a scrollbar tag
    pub fn as_scrollbar(&self) -> Option<(DomId, NodeId, ScrollbarComponent)> {
        match self {
            HitTestTag::Scrollbar {
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
            HitTestTag::DomNode { tag_id } => {
                write!(f, "DomNode(tag:{})", tag_id.inner)
            }
            HitTestTag::Scrollbar {
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
            HitTestTag::Cursor {
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
            HitTestTag::Selection {
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

// ============================================================================
// Hit-Test Result Types (what gets returned from hit-testing)
// ============================================================================

/// Result of processing a single WebRender hit-test item.
///
/// A single hit position can hit multiple overlapping elements, and each element
/// can have multiple "roles" (e.g., a button that is also selectable text).
#[derive(Debug, Clone, PartialEq)]
pub struct HitTestResult {
    /// The typed tag that was hit
    pub tag: HitTestTag,

    /// Point in the viewport coordinate space
    pub point_in_viewport: crate::geom::LogicalPosition,

    /// Point relative to the hit item's origin
    pub point_relative_to_item: crate::geom::LogicalPosition,
}

// ============================================================================
// Conversion from legacy ScrollbarHitId
// ============================================================================

/// Convert from legacy ScrollbarHitId to the new type-safe system
impl From<crate::hit_test::ScrollbarHitId> for HitTestTag {
    fn from(legacy: crate::hit_test::ScrollbarHitId) -> Self {
        use crate::hit_test::ScrollbarHitId;
        match legacy {
            ScrollbarHitId::VerticalTrack(dom_id, node_id) => HitTestTag::Scrollbar {
                dom_id,
                node_id,
                component: ScrollbarComponent::VerticalTrack,
            },
            ScrollbarHitId::VerticalThumb(dom_id, node_id) => HitTestTag::Scrollbar {
                dom_id,
                node_id,
                component: ScrollbarComponent::VerticalThumb,
            },
            ScrollbarHitId::HorizontalTrack(dom_id, node_id) => HitTestTag::Scrollbar {
                dom_id,
                node_id,
                component: ScrollbarComponent::HorizontalTrack,
            },
            ScrollbarHitId::HorizontalThumb(dom_id, node_id) => HitTestTag::Scrollbar {
                dom_id,
                node_id,
                component: ScrollbarComponent::HorizontalThumb,
            },
        }
    }
}

/// Convert from the new type-safe system to legacy ScrollbarHitId
impl TryFrom<HitTestTag> for crate::hit_test::ScrollbarHitId {
    type Error = ();

    fn try_from(tag: HitTestTag) -> Result<Self, Self::Error> {
        use crate::hit_test::ScrollbarHitId;
        match tag {
            HitTestTag::Scrollbar {
                dom_id,
                node_id,
                component,
            } => match component {
                ScrollbarComponent::VerticalTrack => {
                    Ok(ScrollbarHitId::VerticalTrack(dom_id, node_id))
                }
                ScrollbarComponent::VerticalThumb => {
                    Ok(ScrollbarHitId::VerticalThumb(dom_id, node_id))
                }
                ScrollbarComponent::HorizontalTrack => {
                    Ok(ScrollbarHitId::HorizontalTrack(dom_id, node_id))
                }
                ScrollbarComponent::HorizontalThumb => {
                    Ok(ScrollbarHitId::HorizontalThumb(dom_id, node_id))
                }
            },
            _ => Err(()),
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
