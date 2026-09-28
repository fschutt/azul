//! Which context menu a secondary click opens.
//!
//! ONE answer for every shell. The macOS, Windows, X11 and Wayland shells
//! each used to pick the node a right-click opens the menu of on their own:
//! macOS walked up from the front-most hit, X11 and Wayland from the highest
//! `NodeId` of the LOWEST dom, and Windows took the first menu in `NodeId`
//! order - the OUTERMOST one - without walking at all. The same click opened
//! a different menu, or none, depending on the platform. The shells keep only
//! the platform work: presenting the menu (an `NSMenu`, a Win32 popup, a
//! menu window) at the cursor.
//!
//! What asks for a menu is the SECONDARY click: the right button on every
//! platform, and a Control + primary press on macOS as well
//! ([`is_secondary_press`]).

use azul_core::{
    dom::{DomId, DomNodeId, NodeData, NodeId},
    events::MouseButton,
    hit_test::FullHitTest,
    menu::Menu,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::system::Platform;

use crate::{managers::hover::InputPointId, window::LayoutWindow};

/// Is a press of `button` the SECONDARY click - the one that asks for a
/// context menu - on `platform`?
///
/// The right button everywhere. On macOS a primary press with Control held
/// is one too: AppKit hands it to a view as a LEFT `mouseDown:` with the
/// Control flag, and every Mac user without a second button opens menus that
/// way. Command + click is not a secondary click anywhere (on macOS it
/// extends a selection).
#[must_use]
pub fn is_secondary_press(platform: &Platform, button: MouseButton, control_held: bool) -> bool {
    match button {
        MouseButton::Right => true,
        MouseButton::Left => control_held && matches!(platform, Platform::MacOs),
        _ => false,
    }
}

/// How many dom boundaries (`VirtualView` pages in pages) the walk of
/// [`nearest_context_menu`] crosses before it gives up: a host chain that
/// loops must not spin.
const MAX_DOM_HOPS: usize = 16;

/// The node whose context menu a secondary click on `start` opens, and that
/// menu: `start` itself or its nearest ancestor that carries one.
///
/// A child dom (a `VirtualView` page: a video, a progress bar, a
/// virtualized list) is content of the node that hosts it, so the walk
/// continues past the page's root at its host - a right press on a video
/// inside a box with a menu opens the box's menu, as a press on any other
/// content of the box does. It used to stop at the page's root and open
/// nothing.
///
/// `styled_dom_of` looks a dom up (`LayoutWindow::layout_results`).
/// `host_of` names the node that hosts a child dom
/// (`VirtualViewManager::host_of_nested_dom`).
// `'a` is named once: a `dyn Fn` that returns a reference has no input
// lifetime to elide it from, so it cannot be `'_`.
#[allow(single_use_lifetimes)]
#[must_use]
pub fn nearest_context_menu<'a>(
    styled_dom_of: &dyn Fn(DomId) -> Option<&'a StyledDom>,
    start: DomNodeId,
    host_of: &dyn Fn(DomId) -> Option<(DomId, NodeId)>,
) -> Option<(DomNodeId, Menu)> {
    let mut dom = start.dom;
    let mut current = start.node.into_crate_internal();
    for _ in 0..MAX_DOM_HOPS {
        let styled_dom = styled_dom_of(dom)?;
        let node_data = styled_dom.node_data.as_container();
        let hierarchy = styled_dom.node_hierarchy.as_container();
        // A malformed parent chain must not spin.
        let mut budget = node_data.len();
        while let Some(node) = current {
            if let Some(menu) = node_data.get(node).and_then(NodeData::get_context_menu) {
                return Some((dom_node(dom, node), menu.clone()));
            }
            if budget == 0 {
                break;
            }
            budget -= 1;
            current = hierarchy.get(node).and_then(|h| h.parent_id());
        }
        // Past the dom's root: a child dom goes on at the node hosting it.
        let (host_dom, host_node) = host_of(dom)?;
        dom = host_dom;
        current = Some(host_node);
    }
    None
}

/// The context menu a secondary click opens where `hit` was taken, and the
/// node that carries it.
///
/// Doms FRONT-MOST FIRST (a child dom is composited over its host, see
/// `hover::deepest_node_across_doms`), each from its front-most hit node,
/// walking up to the nearest menu ([`nearest_context_menu`], through the
/// dom's hosts too). A dom with no menu on that way does not hide the menus
/// of the doms behind it.
// `'a`: see `nearest_context_menu`.
#[allow(single_use_lifetimes)]
#[must_use]
pub fn context_menu_under_hit<'a>(
    hit: &FullHitTest,
    styled_dom_of: &dyn Fn(DomId) -> Option<&'a StyledDom>,
    host_of: &dyn Fn(DomId) -> Option<(DomId, NodeId)>,
) -> Option<(DomNodeId, Menu)> {
    hit.hovered_nodes.iter().rev().find_map(|(dom, ht)| {
        // The front-most hit of this dom: lowest depth, then the deeper node.
        let front = ht
            .regular_hit_test_nodes
            .iter()
            .min_by(|(a_id, a), (b_id, b)| a.hit_depth.cmp(&b.hit_depth).then(b_id.cmp(a_id)))
            .map(|(node, _)| *node)?;
        nearest_context_menu(styled_dom_of, dom_node(*dom, front), host_of)
    })
}

const fn dom_node(dom: DomId, node: NodeId) -> DomNodeId {
    DomNodeId {
        dom,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    }
}

impl LayoutWindow {
    /// The context menu a secondary click opens where the primary pointer
    /// is now - the hover manager's latest hit test - and the node that
    /// carries it. `None` when nothing under the pointer has one.
    ///
    /// The shells call this on the secondary press or release (their
    /// platform's convention) after the hit test for that event, and present
    /// the menu at the cursor.
    #[must_use]
    pub fn context_menu_under_pointer(&self) -> Option<(DomNodeId, Menu)> {
        self.context_menu_under_seat(azul_core::window::PRIMARY_POINTER_SEAT)
    }

    /// [`Self::context_menu_under_pointer`] for any pointer seat (a second
    /// cursor, a pen's barrel button on Wayland).
    #[must_use]
    pub fn context_menu_under_seat(&self, seat_id: u64) -> Option<(DomNodeId, Menu)> {
        let hit = self
            .hover_manager
            .get_current(&InputPointId::for_seat(seat_id))?;
        let layout_results = &self.layout_results;
        let virtual_views = &self.virtual_view_manager;
        context_menu_under_hit(
            hit,
            &|dom| layout_results.get(&dom).map(|lr| &lr.styled_dom),
            &|dom| virtual_views.host_of_nested_dom(dom),
        )
    }
}
