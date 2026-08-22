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
    refany::OptionRefAny,
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
    forced_open: &[NodeId],
    mut rect_of: impl FnMut(NodeId) -> Option<LogicalRect>,
) -> Vec<TransientPlacement> {
    let nodes = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut out = Vec::new();

    for node in nodes.linear_iter() {
        let Some(nd) = nodes.get(node) else { continue };
        let NodeType::TransientWindow(cfg) = nd.get_node_type() else { continue };
        // The attribute, or a callback's `set_transient_window_open(true)`
        // held by the manager — a widget can open its own popup without the
        // app carrying a flag for it.
        if !cfg.open && !forced_open.contains(&node) {
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
        let rect_of = |n: NodeId| Some(rect(n.index() as f32 * 100.0, 0.0, 50.0, 20.0));
        let found = collect_open_transient_windows(&styled, &[], rect_of);

        assert_eq!(found.len(), 1, "the closed one must not be returned");
        let p = &found[0];
        // body=0, div=1, transient=2 — the anchor must be the DIV's rect (x=100),
        // not the transient node's own (x=200).
        assert_eq!(p.anchor_rect.origin.x, 100.0, "anchored to the parent, not itself");
        assert_eq!(p.anchor, TransientAnchor::Bottom);
        assert_eq!(p.dismiss, TransientDismiss::Outside);

        // A callback's `set_transient_window_open(node, true)` opens the
        // CLOSED one too (body=0, div=1, tw=2, div=3, tw=4) — anchored to
        // its own parent (x=300), never to the other popup's.
        let forced = collect_open_transient_windows(&styled, &[NodeId::new(4)], rect_of);
        assert_eq!(forced.len(), 2, "attribute-open plus forced-open");
        assert_eq!(forced[1].node, NodeId::new(4));
        assert_eq!(forced[1].anchor_rect.origin.x, 300.0);
        // Forcing a node that is not a transient window is ignored.
        let bogus = collect_open_transient_windows(&styled, &[NodeId::new(1)], rect_of);
        assert_eq!(bogus.len(), 1);
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

// ============================================================================
// The manager: which transient windows are open, and their laid-out content
// ============================================================================

/// One open transient window as the ENGINE sees it: where it is anchored,
/// what it contains, and how big its content came out.
///
/// The backend owns the SURFACE (an `xdg_popup`, an `NSPanel`, a `WS_POPUP` hwnd,
/// or nothing at all headless); this owns everything the surface displays.
/// The split is deliberate — it is what lets one dismiss implementation and
/// one layout path serve every platform.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenTransientWindow {
    /// The `<transient-window>` node in the parent DOM that this window IS.
    /// Identity: if the parent rebuilds and the same node is still open, the
    /// window persists rather than closing and re-opening.
    pub source_node: NodeId,
    /// The id under which the popup's content is laid out. Stable for the
    /// window's lifetime, so `layout_results[content_dom]` is its content.
    pub content_dom: DomId,
    /// Anchor, edge, dismiss, requested size — from the last parent layout.
    pub placement: TransientPlacement,
    /// The size the content came out at after its own layout, or the
    /// requested size if one was given. What the backend sizes the surface to.
    pub content_size: LogicalSize,
    /// The backend's handle on the surface — whatever it chooses to keep
    /// (the shell keeps the popup window's shared mailbox here). `None` until
    /// the backend has acted on `opened`, and always `None` headless. Opaque
    /// to this crate on purpose: the engine does not know what a surface is.
    pub surface: OptionRefAny,
}

/// Tracks the open transient windows across parent layouts.
///
/// The hard part is CONTINUITY. The parent may rebuild its DOM sixty times a
/// second (a resize drag), and each rebuild produces a fresh `StyledDom` with
/// fresh node ids. A popup must not close and re-open on every one of those —
/// that is the flicker the screenshare fix (d386614cd) chased out of image
/// nodes, and it would be far worse on a window. So a window is matched to its
/// source node across rebuilds, and only a node that is genuinely gone (or
/// `open=false`) tears its window down.
#[derive(Debug, Default)]
pub struct TransientWindowManager {
    open: Vec<OpenTransientWindow>,
    /// Monotonic: a content `DomId` is never reused within a window's lifetime,
    /// so a stale reference to a closed popup cannot alias a new one.
    next_index: usize,
    /// Source nodes the USER dismissed (outside click, Escape) whose node may
    /// still say `open=true` — the app has not caught up yet, or does not
    /// care. Edge-triggered: such a node stays closed until its `open` goes
    /// false and true again. Without this a dismissed popup would reopen on
    /// the very next parent layout, which is the bug that makes "click
    /// outside to close" feel broken.
    dismissed: Vec<NodeId>,
    /// Surface handles of windows this manager closed on its own (the app
    /// set `open=false`, or the node unmounted), waiting for the backend to
    /// collect them with [`Self::take_closed_surfaces`] and tear the surfaces
    /// down. A dismissal hands its window straight back to the caller instead.
    closed_surfaces: Vec<OptionRefAny>,
    /// Nodes a callback opened with `set_transient_window_open(node, true)`.
    /// Held here, node-keyed, so a self-contained widget (the colour picker's
    /// swatch) can open its popup without the app threading an `open` flag
    /// through its state and layout callback. Cleared by
    /// `set_transient_window_open(node, false)` or by a user dismissal.
    forced_open: Vec<NodeId>,
}

/// What changed between two parent layouts, so the backend knows which
/// surfaces to create, move, or destroy. Returned by
/// [`TransientWindowManager::reconcile`] and accumulated on the window until
/// the backend takes it — a layout call may run several passes (lifecycle
/// callbacks re-layout) before the backend looks.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TransientDiff {
    /// Content dom ids whose window must be CREATED.
    pub opened: Vec<DomId>,
    /// Content dom ids whose window still exists but moved or resized.
    pub moved: Vec<DomId>,
    /// Content dom ids whose window must be DESTROYED.
    pub closed: Vec<DomId>,
}

impl TransientDiff {
    /// Nothing to do.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.opened.is_empty() && self.moved.is_empty() && self.closed.is_empty()
    }

    /// Fold a later pass's diff into this one.
    ///
    /// A window opened and then closed before the backend ever saw it cancels
    /// out entirely — creating a surface only to destroy it would flash. A
    /// move on a window that is about to be created or destroyed is noise.
    pub fn merge(&mut self, later: Self) {
        for dom in later.closed {
            if let Some(i) = self.opened.iter().position(|d| *d == dom) {
                self.opened.remove(i); // never existed as far as the backend knows
                self.moved.retain(|d| *d != dom);
                continue;
            }
            self.moved.retain(|d| *d != dom);
            if !self.closed.contains(&dom) {
                self.closed.push(dom);
            }
        }
        for dom in later.opened {
            // Content ids are never reused, so an id in `closed` cannot reopen.
            if !self.opened.contains(&dom) {
                self.opened.push(dom);
            }
        }
        for dom in later.moved {
            if !self.opened.contains(&dom) && !self.moved.contains(&dom) {
                self.moved.push(dom);
            }
        }
    }
}

impl TransientWindowManager {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            open: Vec::new(),
            next_index: 0,
            dismissed: Vec::new(),
            closed_surfaces: Vec::new(),
            forced_open: Vec::new(),
        }
    }

    /// Every currently open window.
    #[must_use]
    pub fn open_windows(&self) -> &[OpenTransientWindow] {
        &self.open
    }

    /// The open window whose content is laid out under `dom`, if any.
    #[must_use]
    pub fn get(&self, dom: DomId) -> Option<&OpenTransientWindow> {
        self.open.iter().find(|w| w.content_dom == dom)
    }

    /// Mutable access, for the backend to attach its surface handle.
    #[must_use]
    pub fn get_mut(&mut self, dom: DomId) -> Option<&mut OpenTransientWindow> {
        self.open.iter_mut().find(|w| w.content_dom == dom)
    }

    /// The USER closed the popup hanging off `source_node` (outside click,
    /// Escape). Closes it now and remembers the node as dismissed, so the
    /// node's still-`open` attribute does not reopen it on the next pass —
    /// only an `open` that goes false and true again can. Returns the window
    /// that was closed, so the caller can drop its surface and its layout
    /// result and tell the app.
    pub fn dismiss(&mut self, source_node: NodeId) -> Option<OpenTransientWindow> {
        // The user closed it: a callback's "keep open" is over too.
        self.forced_open.retain(|n| *n != source_node);
        if !self.dismissed.contains(&source_node) {
            self.dismissed.push(source_node);
        }
        let i = self.open.iter().position(|w| w.source_node == source_node)?;
        Some(self.open.remove(i))
    }

    /// Source nodes currently held closed by a user dismissal.
    #[must_use]
    pub fn dismissed_nodes(&self) -> &[NodeId] {
        &self.dismissed
    }

    /// Surfaces of windows closed since the backend last asked. Each comes
    /// out exactly once.
    pub fn take_closed_surfaces(&mut self) -> Vec<OptionRefAny> {
        core::mem::take(&mut self.closed_surfaces)
    }

    /// A callback asked for `node`'s popup to be open (or closed) regardless
    /// of its `open` attribute. Opening also lifts an earlier user dismissal —
    /// the user clicked the swatch again, which is the re-arm. Takes effect
    /// on the next reconcile. Returns whether anything changed.
    pub fn set_forced_open(&mut self, node: NodeId, open: bool) -> bool {
        let was = self.forced_open.contains(&node);
        if open {
            self.dismissed.retain(|n| *n != node);
            if !was {
                self.forced_open.push(node);
            }
            !was
        } else {
            self.forced_open.retain(|n| *n != node);
            was
        }
    }

    /// Nodes currently held open by a callback.
    #[must_use]
    pub fn forced_open_nodes(&self) -> &[NodeId] {
        &self.forced_open
    }

    /// Is `dom` one of ours? Lets a dispatcher route a click on a popup surface
    /// to the right content without the backend knowing about content ids.
    #[must_use]
    pub fn is_transient_dom(&self, dom: DomId) -> bool {
        self.get(dom).is_some()
    }

    /// Bring the open set in line with what the parent's latest layout says.
    ///
    /// `wanted` is what [`collect_open_transient_windows`] found this pass.
    /// `content_size_of` lays out a window's content and reports its extent;
    /// it is a closure because layout needs the whole `LayoutWindow` and this
    /// struct must not borrow it.
    ///
    /// Matching is by `source_node`. That is exactly right for a rebuild that
    /// preserved structure and exactly wrong for one that reordered siblings —
    /// which is the same trade the reconciler already makes for every other
    /// per-node state (datasets, images), so a transient window is no more and
    /// no less stable than the node it hangs off.
    pub fn reconcile(
        &mut self,
        wanted: &[TransientPlacement],
        mut content_size_of: impl FnMut(DomId, &TransientPlacement) -> Option<LogicalSize>,
    ) -> TransientDiff {
        let mut diff = TransientDiff::default();

        // 0. A dismissed node is re-armed the moment the app stops asking for
        //    it (`open=false`); while it still asks, the dismissal wins.
        self.dismissed.retain(|n| wanted.iter().any(|p| p.node == *n));
        let wanted: Vec<TransientPlacement> =
            wanted.iter().filter(|p| !self.dismissed.contains(&p.node)).copied().collect();

        // 1. Close anything no longer wanted.
        let still_wanted = |w: &OpenTransientWindow| wanted.iter().any(|p| p.node == w.source_node);
        let (keep, close): (Vec<_>, Vec<_>) = core::mem::take(&mut self.open)
            .into_iter()
            .partition(still_wanted);
        for w in close {
            diff.closed.push(w.content_dom);
            self.closed_surfaces.push(w.surface);
        }
        self.open = keep;

        // 2. Update or open each wanted window.
        for p in &wanted {
            if let Some(existing) = self.open.iter_mut().find(|w| w.source_node == p.node) {
                let moved = existing.placement != *p;
                existing.placement = *p;
                if let Some(sz) = content_size_of(existing.content_dom, p) {
                    if moved || sz != existing.content_size {
                        existing.content_size = sz;
                        diff.moved.push(existing.content_dom);
                    }
                } else if moved {
                    diff.moved.push(existing.content_dom);
                }
                continue;
            }

            let content_dom = transient_dom_id(self.next_index);
            self.next_index += 1;
            let Some(content_size) = content_size_of(content_dom, p) else {
                continue; // content could not be laid out; do not open onto nothing
            };
            self.open.push(OpenTransientWindow {
                source_node: p.node,
                content_dom,
                placement: *p,
                content_size,
                surface: OptionRefAny::None,
            });
            diff.opened.push(content_dom);
        }

        diff
    }

    /// Close everything — the parent window is going away.
    pub fn close_all(&mut self) -> Vec<DomId> {
        core::mem::take(&mut self.open)
            .into_iter()
            .map(|w| {
                self.closed_surfaces.push(w.surface);
                w.content_dom
            })
            .collect()
    }
}

/// A popup is keyed by the node it hangs off, and node ids are arena indices
/// that shift when the parent rebuilds. Without this the popup would point at
/// a live but WRONG node after any rebuild that inserted or removed a sibling
/// above it — and `LayoutWindow`'s remap destructure refuses to compile until
/// every node-keyed manager is listed, which is how this impl came to exist.
///
/// A source node that is UNMOUNTED by the rebuild drops its window: the thing
/// it was anchored to is gone, so there is nothing for the popup to belong to.
impl crate::managers::NodeIdRemap for TransientWindowManager {
    fn remap_node_ids(&mut self, dom: DomId, map: &crate::managers::NodeIdMap) {
        if dom != DomId::ROOT_ID {
            return; // transient windows hang off the parent (root) dom only
        }
        let mut unmounted = Vec::new();
        self.open.retain_mut(|w| {
            if let Some(new_id) = map.resolve(w.source_node) {
                w.source_node = new_id;
                w.placement.node = new_id;
                true
            } else {
                unmounted.push(core::mem::replace(&mut w.surface, OptionRefAny::None));
                false
            }
        });
        self.closed_surfaces.extend(unmounted);
        // A dismissal follows its node too; an unmounted node's is moot.
        self.dismissed = self.dismissed.iter().filter_map(|n| map.resolve(*n)).collect();
        self.forced_open = self.forced_open.iter().filter_map(|n| map.resolve(*n)).collect();
    }
}

#[cfg(test)]
mod manager_tests {
    use super::*;
    use azul_core::geom::LogicalPosition;

    fn placement(node: usize, x: f32) -> TransientPlacement {
        TransientPlacement {
            node: NodeId::new(node),
            anchor_rect: LogicalRect::new(LogicalPosition::new(x, 0.0), LogicalSize::new(10.0, 10.0)),
            anchor: TransientAnchor::Bottom,
            dismiss: TransientDismiss::Outside,
            size: OptionLogicalSize::None,
            tearoff: false,
        }
    }
    fn sized(_: DomId, _: &TransientPlacement) -> Option<LogicalSize> {
        Some(LogicalSize::new(100.0, 50.0))
    }

    /// THE property this exists for: a window that is still wanted after a
    /// parent rebuild keeps its content dom — it is not closed and re-opened.
    #[test]
    fn a_still_open_window_survives_a_rebuild_without_reopening() {
        let mut m = TransientWindowManager::new();
        let d1 = m.reconcile(&[placement(4, 0.0)], sized);
        assert_eq!(d1.opened.len(), 1);
        let id = d1.opened[0];

        // Same node, same place: nothing happens.
        let d2 = m.reconcile(&[placement(4, 0.0)], sized);
        assert_eq!(d2, TransientDiff::default(), "nothing changed, nothing to do");
        assert_eq!(m.get(id).map(|w| w.source_node), Some(NodeId::new(4)));

        // Same node, anchor moved (the parent was resized): MOVED, not re-opened.
        let d3 = m.reconcile(&[placement(4, 50.0)], sized);
        assert_eq!(d3.moved, vec![id]);
        assert!(d3.opened.is_empty() && d3.closed.is_empty());
    }

    /// A node that stops being open closes its window, and a new open node
    /// gets a FRESH id — never a recycled one.
    #[test]
    fn closing_and_reopening_never_reuses_an_id() {
        let mut m = TransientWindowManager::new();
        let first = m.reconcile(&[placement(4, 0.0)], sized).opened[0];
        let d = m.reconcile(&[], sized);
        assert_eq!(d.closed, vec![first]);
        assert!(m.open_windows().is_empty());

        let second = m.reconcile(&[placement(4, 0.0)], sized).opened[0];
        assert_ne!(first, second, "a stale handle to the old popup must not alias the new one");
    }

    /// Content that cannot be laid out does not open a window.
    #[test]
    fn unlayoutable_content_does_not_open() {
        let mut m = TransientWindowManager::new();
        let d = m.reconcile(&[placement(4, 0.0)], |_, _| None);
        assert!(d.opened.is_empty());
        assert!(m.open_windows().is_empty());
    }
}
