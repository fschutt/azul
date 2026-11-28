//! Menus of a window with ONE surface (a phone): the drop-down of a `DropDown`, the menu of a
//! toolbar button, the context menu of a long press - every `Menu` an app opens.
//!
//! A desktop shows a menu as a native menu or as a window of its own. A window with one surface can
//! open neither, so the menu is shown INSIDE the window, over the page: the engine records it here
//! ([`MenuOverlays`]), the Dom pass ([`crate::menu_overlay_dom`]) puts a `<transient-window>` at the
//! end of the page's DOM that holds the menu's rows, and from there the menu is an overlay like
//! every other popup of such a window (`crate::transient_overlay`): it is placed against the control
//! it dropped from, flips and slides to stay on the screen, closes on a press outside it, on Escape
//! and on the back button, and takes the keyboard from nobody.
//!
//! What the user picks runs in THIS window, as it did for a native menu: the item's callback is
//! called by the row that was pressed, in the pass of that press, with the window's own state.
//!
//! This module holds what is recorded; it has no widget in it, so a build without the widgets
//! still has the window's menus to ask about.

use alloc::vec::Vec;

use azul_core::{
    geom::{LogicalRect, LogicalSize},
    id::NodeId,
    menu::Menu,
    refany::RefAny,
    styled_dom::StyledDom,
};

use crate::window::LayoutWindow;

/// How many menus a window shows at once. A menu opened from an item of a menu is an expansion in
/// place, never another menu, so more than a few at once is an app that opens one on every frame:
/// the oldest is closed (and a diagnostic says so) rather than the window filling up with them.
pub const MAX_OPEN_MENUS: usize = 4;

/// The prefix of the id of the `<transient-window>` that shows a menu: what ties the node to the
/// menu it shows (`__azul-menu-overlay-<id>`), and what a scenario selects it by.
pub const MENU_ID_PREFIX: &str = "__azul-menu-overlay-";

/// Which submenus of a menu are expanded: the paths of their items (the index of the item in its
/// menu, then of the item in its submenu, and so on). Shared (a `RefAny`) between the record of
/// the menu and the rows that toggle it: a tap on an item with a submenu flips its path here and
/// asks for the page again, which builds the rows from this.
#[derive(Debug, Default)]
pub(crate) struct ExpandedSubmenus {
    open: Vec<Vec<usize>>,
}

impl ExpandedSubmenus {
    /// Is the submenu of the item at `path` expanded?
    pub(crate) fn is_open(&self, path: &[usize]) -> bool {
        self.open.iter().any(|p| p.as_slice() == path)
    }

    /// Expands the submenu of the item at `path`, or folds it again.
    pub(crate) fn toggle(&mut self, path: &[usize]) {
        match self.open.iter().position(|p| p.as_slice() == path) {
            Some(at) => {
                self.open.remove(at);
            }
            None => self.open.push(path.to_vec()),
        }
    }
}

/// One menu a window shows over its page.
#[derive(Debug, Clone)]
pub struct MenuOverlay {
    /// Names this menu for as long as it is open (never reused in a window).
    pub id: u64,
    /// What the app asked to have shown.
    pub menu: Menu,
    /// What the menu hangs from, in window coordinates: the rect of the control it dropped out of,
    /// or - a context menu - the point it was opened at as a rect with no size.
    pub anchor: LogicalRect,
    /// The size of the window when the menu opened. A menu belongs to the shape of the window it
    /// was opened in: when that changes (a turn of the phone) it closes, as the platforms' own do.
    pub window: LogicalSize,
    /// The submenus expanded in place ([`ExpandedSubmenus`]).
    pub(crate) expanded: RefAny,
}

/// The menus a window shows, oldest first (the last is the top).
#[derive(Debug, Clone, Default)]
pub struct MenuOverlays {
    open: Vec<MenuOverlay>,
    last_id: u64,
}

impl MenuOverlays {
    /// No menu open.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            open: Vec::new(),
            last_id: 0,
        }
    }

    /// Records a menu opened for `anchor` in a window of `window` size. Returns its id - or `None`
    /// when the request closed a menu instead: the control that a menu drops out of opens it and
    /// closes it again, so a second request for the same anchor is the control being tapped
    /// twice.
    pub fn open(&mut self, menu: &Menu, anchor: LogicalRect, window: LogicalSize) -> Option<u64> {
        // A point (a context menu) is no control: another request there is another menu.
        let is_a_control = anchor.size.width > 0.0 || anchor.size.height > 0.0;
        if is_a_control {
            if let Some(at) = self.open.iter().position(|m| m.anchor == anchor) {
                self.open.remove(at);
                return None;
            }
        }
        if self.open.len() >= MAX_OPEN_MENUS {
            let dropped = self.open.remove(0);
            azul_core::diagnostics::emit(alloc::format!(
                "[azul][warn] a window shows at most {MAX_OPEN_MENUS} menus at once: the oldest \
                 (menu {}) was closed to open another",
                dropped.id
            ));
        }
        self.last_id += 1;
        let id = self.last_id;
        self.open.push(MenuOverlay {
            id,
            menu: menu.clone(),
            anchor,
            window,
            expanded: RefAny::new(ExpandedSubmenus::default()),
        });
        Some(id)
    }

    /// Forgets the menu `id`. Returns whether it was open.
    pub fn close(&mut self, id: u64) -> bool {
        let before = self.open.len();
        self.open.retain(|m| m.id != id);
        self.open.len() != before
    }

    /// The menu `id`, if it is open.
    #[must_use]
    pub fn get(&self, id: u64) -> Option<&MenuOverlay> {
        self.open.iter().find(|m| m.id == id)
    }

    /// Is no menu open?
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    /// How many menus are open.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.open.len()
    }

    /// The open menus, oldest first.
    #[must_use]
    pub fn menus(&self) -> &[MenuOverlay] {
        &self.open
    }

    /// Forgets every menu that was opened in a window of another size than `window`; returns
    /// their ids.
    pub fn close_stale(&mut self, window: LogicalSize) -> Vec<u64> {
        let stale: Vec<u64> = self
            .open
            .iter()
            .filter(|m| m.window != window)
            .map(|m| m.id)
            .collect();
        self.open.retain(|m| m.window == window);
        stale
    }
}

/// The id of the menu the `<transient-window>` at `node` shows, if it is the node of one.
#[must_use]
pub(crate) fn menu_overlay_id_of(styled: &StyledDom, node: NodeId) -> Option<u64> {
    let data = styled.node_data.as_container().get(node)?;
    data.get_ids_and_classes().iter().find_map(|c| {
        c.as_id()?
            .strip_prefix(MENU_ID_PREFIX)?
            .parse::<u64>()
            .ok()
    })
}

/// The `<transient-window>` that shows the menu `id`, if the page has it.
fn node_of_menu(styled: &StyledDom, id: u64) -> Option<NodeId> {
    let wanted = alloc::format!("{MENU_ID_PREFIX}{id}");
    let nodes = styled.node_data.as_container();
    (0..nodes.len()).map(NodeId::new).find(|node| {
        nodes
            .get(*node)
            .is_some_and(|data| data.get_ids_and_classes().iter().any(|c| c.as_id() == Some(wanted.as_str())))
    })
}

impl LayoutWindow {
    /// A callback opened `menu` on a window with one surface: it hangs from `anchor` (the control's
    /// rect in window coordinates, or the point of a context menu as a rect with no size) in a
    /// window of `window` size. The menu is shown by the next page the Dom pass makes, so the
    /// caller asks for one. Returns whether a menu is open now: `false` when the request was the
    /// control that opened it being tapped again.
    pub fn open_menu_overlay(
        &mut self,
        menu: &Menu,
        anchor: LogicalRect,
        window: LogicalSize,
    ) -> bool {
        self.menu_overlays.open(menu, anchor, window).is_some()
    }

    /// What the `<transient-window>` of a menu hangs from: the rect the menu was opened for, in
    /// place of the rect of the node's parent. `None` for a node that is no menu's.
    pub(crate) fn menu_overlay_anchor(&self, styled: &StyledDom, node: NodeId) -> Option<LogicalRect> {
        let id = menu_overlay_id_of(styled, node)?;
        self.menu_overlays.get(id).map(|m| m.anchor)
    }

    /// The popup at `node` was closed (a press outside it, Escape, the back button, a pick): when
    /// it showed a menu, the menu is over.
    pub(crate) fn forget_menu_overlay_at(&mut self, node: NodeId) {
        if self.menu_overlays.is_empty() {
            return;
        }
        let id = self
            .layout_results
            .get(&azul_core::dom::DomId::ROOT_ID)
            .and_then(|root| menu_overlay_id_of(&root.styled_dom, node));
        if let Some(id) = id {
            self.menu_overlays.close(id);
        }
    }

    /// Closes the menus that were opened in a window of another size than `window`: the window
    /// was turned or resized under them. The popups they were shown in are closed with them.
    pub(crate) fn close_stale_menu_overlays(&mut self, window: LogicalSize) {
        if self.menu_overlays.is_empty() {
            return;
        }
        let stale = self.menu_overlays.close_stale(window);
        for id in stale {
            let node = self
                .layout_results
                .get(&azul_core::dom::DomId::ROOT_ID)
                .and_then(|root| node_of_menu(&root.styled_dom, id));
            if let Some(node) = node {
                drop(self.dismiss_transient_window(node));
            }
        }
    }
}

#[cfg(test)]
#[path = "menu_overlay_test.rs"]
mod menu_overlay_test;
