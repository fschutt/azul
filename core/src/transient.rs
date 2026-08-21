//! `<transient-window>`: a popup that is a REAL OS window, drawn from a subtree
//! of the one DOM.
//!
//! A colour picker that opens below its swatch, a tooltip with a pointer
//! arrow, a tear-off tool palette — each wants its own window surface (so it
//! can escape the parent's bounds, carry a shadow, sit above everything) but
//! NOT its own application: it needs the parent's state, callbacks and
//! styling, and it must open and close by flipping one attribute.
//!
//! That is what this node type provides. While `open == false` the element
//! contributes nothing to layout — its subtree is not laid out at all. When
//! `open == true` the engine materialises the subtree as a transient window
//! anchored to the node's PARENT, routes input on that surface back into the
//! same `LayoutWindow`, and tears it down when `open` flips back.
//!
//! The app never touches a window. It toggles `open`.
//!
//! ## Why not the existing menu path
//!
//! Context menus already become real OS windows on every backend, but each
//! one runs a SEPARATE application window with its own layout callback and a
//! copied `RefAny`. That is why the Wayland menu renders white (its private
//! `LayoutWindow` never receives a layout pass from the parent's loop) and why
//! Escape / outside-click are reimplemented per backend and broken on some. A
//! transient window owns a SURFACE but renders a subtree of the PARENT's DOM:
//! one tree, one event loop, one dismiss implementation.
//!
//! See `scripts/TRANSIENT_WINDOW_PLAN_2026_08_22.md` for the full design,
//! including tear-off and window shapes, which build on this.

use crate::geom::{LogicalSize, OptionLogicalSize};

/// Which edge of the anchor node a transient window opens from.
///
/// Expressed as an EDGE, never as coordinates. Wayland clients cannot address
/// screen positions — the compositor hides them — so the only placement that
/// works everywhere is "this edge of that rect, with this gravity", which
/// `xdg_positioner` takes natively and the other backends can compute from.
/// A design that stored `(x, y)` would work on X11 and be wrong on Wayland.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum TransientAnchor {
    /// Below the anchor, left edges aligned — what a dropdown or a colour
    /// picker does. The default, because it is what `<select>` does.
    #[default]
    Bottom,
    /// Above the anchor, left edges aligned.
    Top,
    /// To the left of the anchor, top edges aligned.
    Left,
    /// To the right of the anchor, top edges aligned — what a submenu does.
    Right,
    /// At the pointer position rather than the anchor rect — what a context
    /// menu does.
    Cursor,
}

/// What closes a transient window without the app asking.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum TransientDismiss {
    /// A press outside the window closes it, and so does Escape. The default:
    /// it is how every popup a user has ever met behaves.
    #[default]
    Outside,
    /// Only Escape closes it. For a popup the user interacts with by clicking
    /// around it — a floating toolbar.
    Escape,
    /// Nothing closes it but the app. For palettes that stay up.
    None,
}

/// The inline configuration of a `NodeType::TransientWindow`.
///
/// `Copy` and small on purpose: it rides inside `NodeType` the way
/// `GeolocationProbeConfig` does, so opening a popup needs no allocation and
/// `NodeType` (48 bytes, set by its largest payload) does not grow.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TransientWindowConfig {
    /// The ONLY thing an application toggles. `true` materialises the subtree
    /// as a window; `false` tears it down and drops it from layout entirely.
    pub open: bool,
    /// Which edge of the anchor (the node's parent) it opens from.
    pub anchor: TransientAnchor,
    /// What closes it.
    pub dismiss: TransientDismiss,
    /// Explicit size, or `None` to size the window to its content — the
    /// common case, and the reason a colour picker never has to guess how tall
    /// its own panel is.
    ///
    /// `OptionLogicalSize`, the `#[repr(C, u8)]` option, NOT `Option<_>`: this
    /// struct rides inside `NodeType`, which rides inside every `Dom` the C ABI
    /// passes by value, and a Rust `Option` has no stable layout — clippy's
    /// `improper_ctypes_definitions` flagged every `extern "C" fn -> Dom` in
    /// the tree the moment a plain `Option` went in here.
    pub size: OptionLogicalSize,
    /// `true` lets the user drag the window OUT of its anchor, at which point
    /// it becomes a free toplevel that is still the same DOM subtree. Phase 6
    /// of the plan; parsed and carried now so the attribute is stable.
    pub tearoff: bool,
}

impl Default for TransientWindowConfig {
    /// Closed, anchored below, dismissed by outside-click, content-sized.
    fn default() -> Self {
        Self {
            open: false,
            anchor: TransientAnchor::Bottom,
            dismiss: TransientDismiss::Outside,
            size: OptionLogicalSize::None,
            tearoff: false,
        }
    }
}

impl TransientWindowConfig {
    /// Closed, with every other field at its default.
    #[must_use]
    pub const fn closed() -> Self {
        Self {
            open: false,
            anchor: TransientAnchor::Bottom,
            dismiss: TransientDismiss::Outside,
            size: OptionLogicalSize::None,
            tearoff: false,
        }
    }

    /// Open, with every other field at its default.
    #[must_use]
    pub const fn opened() -> Self {
        Self { open: true, ..Self::closed() }
    }

    #[must_use]
    pub const fn with_anchor(mut self, anchor: TransientAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    #[must_use]
    pub const fn with_dismiss(mut self, dismiss: TransientDismiss) -> Self {
        self.dismiss = dismiss;
        self
    }

    #[must_use]
    pub const fn with_size(mut self, size: LogicalSize) -> Self {
        self.size = OptionLogicalSize::Some(size);
        self
    }

    #[must_use]
    pub const fn with_tearoff(mut self, tearoff: bool) -> Self {
        self.tearoff = tearoff;
        self
    }
}

impl TransientWindowConfig {
    /// Apply one XML/HTML attribute. Returns `true` if the key was one of ours.
    ///
    /// `size="WxH"` in logical px; anything else for a known key degrades to
    /// the default rather than erroring — a typo must not make a popup refuse
    /// to open. Unknown keys return `false` so the caller can treat them as
    /// ordinary attributes (id, class, …).
    pub fn apply_attr(&mut self, key: &str, value: &str) -> bool {
        match key {
            "open" => {
                self.open = matches!(value.trim(), "true" | "open" | "1" | "");
                true
            }
            "anchor" => {
                self.anchor = TransientAnchor::parse(value);
                true
            }
            "dismiss" => {
                self.dismiss = TransientDismiss::parse(value);
                true
            }
            "tearoff" => {
                self.tearoff = matches!(value.trim(), "true" | "1" | "");
                true
            }
            "size" => {
                self.size = match value.trim().split_once('x') {
                    Some((w, h)) => match (w.trim().parse::<f32>(), h.trim().parse::<f32>()) {
                        (Ok(w), Ok(h)) if w > 0.0 && h > 0.0 => {
                            OptionLogicalSize::Some(LogicalSize::new(w, h))
                        }
                        _ => OptionLogicalSize::None,
                    },
                    None => OptionLogicalSize::None, // "content" or garbage
                };
                true
            }
            _ => false,
        }
    }
}

impl TransientAnchor {
    /// The attribute value, as written in XML: `anchor="bottom"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::Left => "left",
            Self::Right => "right",
            Self::Cursor => "cursor",
        }
    }

    /// Parse the attribute value. Unknown strings fall back to the default
    /// rather than erroring: a typo in a popup's anchor should degrade to
    /// "opens below", not to "does not open".
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "top" => Self::Top,
            "left" => Self::Left,
            "right" => Self::Right,
            "cursor" => Self::Cursor,
            _ => Self::Bottom,
        }
    }
}

impl TransientDismiss {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Outside => "outside",
            Self::Escape => "escape",
            Self::None => "none",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "escape" => Self::Escape,
            "none" => Self::None,
            _ => Self::Outside,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default must be CLOSED. A popup that is open by default would
    /// materialise a window for every `<transient-window>` in the tree on
    /// first layout.
    #[test]
    fn the_default_is_closed_and_anchored_below() {
        let c = TransientWindowConfig::default();
        assert!(!c.open);
        assert_eq!(c.anchor, TransientAnchor::Bottom);
        assert_eq!(c.dismiss, TransientDismiss::Outside);
        assert!(matches!(c.size, OptionLogicalSize::None), "content-sized unless told otherwise");
    }

    /// Attribute values round-trip, and unknown ones degrade to the default
    /// rather than failing.
    #[test]
    fn anchor_and_dismiss_round_trip_through_their_attribute_strings() {
        for a in [
            TransientAnchor::Bottom,
            TransientAnchor::Top,
            TransientAnchor::Left,
            TransientAnchor::Right,
            TransientAnchor::Cursor,
        ] {
            assert_eq!(TransientAnchor::parse(a.as_str()), a);
        }
        for d in [TransientDismiss::Outside, TransientDismiss::Escape, TransientDismiss::None] {
            assert_eq!(TransientDismiss::parse(d.as_str()), d);
        }
        assert_eq!(TransientAnchor::parse("sideways"), TransientAnchor::Bottom);
        assert_eq!(TransientDismiss::parse("maybe"), TransientDismiss::Outside);
    }

    /// Attributes as written in XML produce the config they name.
    #[test]
    fn attributes_apply_onto_the_config() {
        let mut c = TransientWindowConfig::closed();
        assert!(c.apply_attr("open", "true"));
        assert!(c.apply_attr("anchor", "right"));
        assert!(c.apply_attr("dismiss", "escape"));
        assert!(c.apply_attr("size", "320x240"));
        assert!(c.apply_attr("tearoff", "true"));
        assert!(!c.apply_attr("class", "x"), "not ours — the caller keeps it");

        assert!(c.open);
        assert_eq!(c.anchor, TransientAnchor::Right);
        assert_eq!(c.dismiss, TransientDismiss::Escape);
        assert!(matches!(c.size, OptionLogicalSize::Some(s) if s.width == 320.0 && s.height == 240.0));
        assert!(c.tearoff);

        // Degrade, never refuse: a bad size means content-sized.
        c.apply_attr("size", "big");
        assert!(matches!(c.size, OptionLogicalSize::None));
        c.apply_attr("open", "false");
        assert!(!c.open);
    }

    /// The config must not GROW `NodeType`.
    ///
    /// `NodeType` is 48 bytes today (measured 2026-08-22); the largest payload
    /// sets that. This config rides inline, so it must fit under the existing
    /// largest variant rather than under some round number — a first version
    /// of this test said `<= 16` and failed at 28 bytes, which would have been
    /// a false alarm about a struct that fits comfortably.
    #[test]
    fn the_config_does_not_grow_node_type() {
        let cfg = core::mem::size_of::<TransientWindowConfig>();
        let node = core::mem::size_of::<crate::dom::NodeType>();
        assert!(
            cfg < node,
            "TransientWindowConfig is {cfg} bytes, NodeType is {node}: the config \
             has become the largest payload and is now what sets NodeType's size"
        );
    }
}
