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

/// Reserved for future use (e.g., selection handles, resize handles)
pub const TAG_TYPE_RESERVED: u16 = 0x0300;

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
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HitTestTag {
    /// A regular DOM node (button, div, text container, etc.)
    ///
    /// These are nodes that have callbacks, are focusable, or have cursor styles.
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

    /// Get the TagId if this is a DOM node tag
    pub fn as_dom_node(&self) -> Option<TagId> {
        match self {
            HitTestTag::DomNode { tag_id } => Some(*tag_id),
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
