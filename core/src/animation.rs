//! Core data structures for configuring and tracking CSS animations

/// Specifies which image layer of an element an animation should apply to.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum UpdateImageType {
    /// The animation targets the element's background.
    Background,
    /// The animation targets the element's main content.
    Content,
}
