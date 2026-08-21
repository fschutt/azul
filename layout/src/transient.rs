//! Engine-side support for `<transient-window>`: find the open ones after a
//! layout, and describe each as something a backend can turn into a surface.
//!
//! The node type and its config live in `azul_core::transient`. This module is
//! the bridge between "a node in the parent's DOM says `open=true`" and "the
//! backend has a window to create" — it does the part that is the same on
//! every platform, so the per-backend code only has to answer "given this
//! anchor rect and this gravity, make a surface".
//!
//! A transient window is NOT laid out in its parent (see
//! `layout_tree::get_display_type`, which returns `display: none` for the node
//! in its parent's flow). Its subtree is laid out separately, as a root, with
//! its own `DomId` — the same treatment a `VirtualView` child gets — and that
//! is what [`collect_open_transient_windows`] sets up.

use alloc::vec::Vec;

use azul_core::{
    dom::{DomId, NodeType},
    geom::{LogicalRect, LogicalSize, OptionLogicalSize},
    id::NodeId,
    styled_dom::StyledDom,
    transient::{TransientAnchor, TransientDismiss, TransientWindowConfig},
};

/// Everything a backend needs to materialise one open transient window.
///
/// Placement is an ANCHOR RECT plus an EDGE, never a screen position. Wayland
/// cannot be given coordinates (the compositor hides them) and `xdg_positioner`
/// takes exactly this shape; the other backends compute a position from it in
/// one shared place ([`TransientPlacement::resolve`]) so they cannot disagree.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct TransientPlacement {
    /// The `<transient-window>` node in the PARENT dom.
    pub node: NodeId,
    /// The anchor: the transient node's parent element, in parent-window
    /// logical coordinates. What the popup opens FROM.
    pub anchor_rect: LogicalRect,
    /// Which edge of the anchor it opens from.
    pub anchor: TransientAnchor,
    /// What closes it.
    pub dismiss: TransientDismiss,
    /// The requested size, or `None` for content-sized. The backend lays the
    /// subtree out with `available = size` when given, and with an unbounded
    /// available size otherwise, then uses the content's extent.
    pub size: OptionLogicalSize,
    /// Whether the user may tear it off into a free toplevel (phase 6).
    pub tearoff: bool,
}

impl TransientPlacement {
    /// Where the popup's top-left goes, in the PARENT's logical coordinates,
    /// for a popup of `size` opening from this anchor.
    ///
    /// Shared by every backend that needs a coordinate (X11, Windows, macOS,
    /// headless) so the arithmetic lives once. Wayland does NOT call this — it
    /// hands `anchor_rect` + `anchor` to `xdg_positioner` and lets the
    /// compositor place it, which is the only way to get it right there.
    ///
    /// `cursor` is used only for [`TransientAnchor::Cursor`]; pass the current
    /// pointer position, or `None` to fall back to the anchor's corner.
    #[must_use]
    pub fn resolve(&self, size: LogicalSize, cursor: Option<azul_core::geom::LogicalPosition>) -> azul_core::geom::LogicalPosition {
        use azul_core::geom::LogicalPosition;
        let a = self.anchor_rect;
        match self.anchor {
            TransientAnchor::Bottom => LogicalPosition::new(a.origin.x, a.origin.y + a.size.height),
            TransientAnchor::Top => LogicalPosition::new(a.origin.x, a.origin.y - size.height),
            TransientAnchor::Left => LogicalPosition::new(a.origin.x - size.width, a.origin.y),
            TransientAnchor::Right => LogicalPosition::new(a.origin.x + a.size.width, a.origin.y),
            TransientAnchor::Cursor => cursor.unwrap_or(a.origin),
        }
    }
}

/// Every `<transient-window>` in `styled_dom` whose config says `open`, with
/// the anchor rect of its parent element taken from `positions` (the
/// parent-window layout result, indexed by `NodeId`).
///
/// Closed transient windows are not returned — there is nothing to do for
/// them, and the caller must not have to filter. A transient window with no
/// parent (the root itself) is skipped: there is nothing to anchor to.
///
/// `rect_of` is how the caller supplies "where did node N land in the parent's
/// layout"; it is a closure rather than a map so a `DomLayoutResult` and a test
/// fixture can both drive it without converting.
#[must_use]
pub fn collect_open_transient_windows(
    styled_dom: &StyledDom,
    mut rect_of: impl FnMut(NodeId) -> Option<LogicalRect>,
) -> Vec<TransientPlacement> {
    let nodes = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut out = Vec::new();

    for node in nodes.linear_iter() {
        let Some(nd) = nodes.get(node) else { continue };
        let NodeType::TransientWindow(cfg) = nd.get_node_type() else { continue };
        if !cfg.open {
            continue;
        }
        let Some(parent) = hierarchy.get(node).and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id) else {
            continue; // a root transient window has nothing to anchor to
        };
        let Some(anchor_rect) = rect_of(parent) else {
            continue; // the parent was not laid out (display:none ancestor)
        };
        out.push(placement_for(node, anchor_rect, cfg));
    }
    out
}

/// Build the placement for one open node. Split out so the pure mapping from
/// config to placement is testable without a `StyledDom`.
#[must_use]
pub const fn placement_for(node: NodeId, anchor_rect: LogicalRect, cfg: &TransientWindowConfig) -> TransientPlacement {
    TransientPlacement {
        node,
        anchor_rect,
        anchor: cfg.anchor,
        dismiss: cfg.dismiss,
        size: cfg.size,
        tearoff: cfg.tearoff,
    }
}

/// The `DomId` under which an open transient window's subtree is laid out.
///
/// Transient windows are children of the PARENT dom but laid out as roots of
/// their own, and the child-dom machinery (`VirtualView`, iframes) keys
/// everything by `DomId`. Allocating from a high base keeps these ids clear of
/// the ones `VirtualView` hands out from 1 upward, so the two never collide in
/// `layout_results`.
#[must_use]
pub const fn transient_dom_id(index: usize) -> DomId {
    DomId { inner: 0x1000_0000 + index }
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_core::{
        dom::Dom,
        geom::LogicalPosition,
    };

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
    }

    /// Only OPEN transient windows are collected, and each one's anchor is its
    /// PARENT's rect — not its own, which has no layout.
    #[test]
    fn only_open_windows_are_collected_and_anchored_to_their_parent() {
        let dom = Dom::create_body()
            .with_child(
                Dom::create_div().with_child(Dom::create_from_data(
                    azul_core::dom::NodeData::create_node(NodeType::TransientWindow(
                        TransientWindowConfig::opened(),
                    )),
                )),
            )
            .with_child(
                Dom::create_div().with_child(Dom::create_from_data(
                    azul_core::dom::NodeData::create_node(NodeType::TransientWindow(
                        TransientWindowConfig::closed(),
                    )),
                )),
            );
        let styled = StyledDom::create_from_dom(dom);

        // Give every node a distinct rect keyed by its index so we can tell
        // "anchored to parent" from "anchored to self".
        let found = collect_open_transient_windows(&styled, |n| {
            Some(rect(n.index() as f32 * 100.0, 0.0, 50.0, 20.0))
        });

        assert_eq!(found.len(), 1, "the closed one must not be returned");
        let p = &found[0];
        // body=0, div=1, transient=2 — the anchor must be the DIV's rect (x=100),
        // not the transient node's own (x=200).
        assert_eq!(p.anchor_rect.origin.x, 100.0, "anchored to the parent, not itself");
        assert_eq!(p.anchor, TransientAnchor::Bottom);
        assert_eq!(p.dismiss, TransientDismiss::Outside);
    }

    /// Placement arithmetic: the popup's top-left for each edge.
    #[test]
    fn resolve_places_the_popup_on_the_requested_edge() {
        let mk = |anchor| TransientPlacement {
            node: NodeId::new(0),
            anchor_rect: rect(100.0, 200.0, 40.0, 20.0),
            anchor,
            dismiss: TransientDismiss::Outside,
            size: OptionLogicalSize::None,
            tearoff: false,
        };
        let popup = LogicalSize::new(300.0, 150.0);

        assert_eq!(mk(TransientAnchor::Bottom).resolve(popup, None), LogicalPosition::new(100.0, 220.0));
        assert_eq!(mk(TransientAnchor::Top).resolve(popup, None), LogicalPosition::new(100.0, 50.0));
        assert_eq!(mk(TransientAnchor::Right).resolve(popup, None), LogicalPosition::new(140.0, 200.0));
        assert_eq!(mk(TransientAnchor::Left).resolve(popup, None), LogicalPosition::new(-200.0, 200.0));
        assert_eq!(
            mk(TransientAnchor::Cursor).resolve(popup, Some(LogicalPosition::new(7.0, 9.0))),
            LogicalPosition::new(7.0, 9.0)
        );
        assert_eq!(
            mk(TransientAnchor::Cursor).resolve(popup, None),
            LogicalPosition::new(100.0, 200.0),
            "no cursor: fall back to the anchor corner rather than (0,0)"
        );
    }

    /// Transient dom ids must never collide with the VirtualView ids that
    /// count up from 1.
    #[test]
    fn transient_dom_ids_do_not_collide_with_virtual_view_ids() {
        for i in 0..64 {
            assert!(transient_dom_id(i).inner >= 0x1000_0000);
        }
        assert_ne!(transient_dom_id(0), transient_dom_id(1));
    }
}
