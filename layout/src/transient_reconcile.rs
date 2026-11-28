//! The reconcile pass of a window's `<transient-window>`s: after a layout of the root, find the
//! open ones, measure what each shows, place it, and tell the manager.
//!
//! The pass belongs to the ENGINE, not to a shell: a desktop shell turns its result into OS
//! windows, a phone shell (one surface, [`WindowPresentation::OneSurface`]) shows the same open
//! set as overlays inside the window, and the e2e runner and the tests drive the very same pass.
//! What differs per host is only where the room comes from: a desktop popup may use the screen the
//! anchor is on (`screens_of` answers with every screen in the window's logical px), an overlay
//! the window less its safe area, the keyboard and a margin ([`LayoutWindow::overlay_bounds`]).

use alloc::{string::String, vec::Vec};

use azul_core::{
    dom::{DomId, DomNodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    id::NodeId,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    transient::TransientAnchor,
};

use crate::{
    placement::{screen_for_anchor, ScreenRects},
    transient::{collect_open_transient_windows, ReconcileEnv, WindowPresentation},
    window::LayoutWindow,
    window_state::FullWindowState,
};

/// How far a popup stays off the edges of the screen's work area (or of the window, for an
/// overlay).
pub const POPUP_SCREEN_MARGIN: f32 = 8.0;

/// Room enough for a title bar: 28 pt on macOS, about 31 px on Windows, up
/// to 46 px for a GNOME header bar.
const TITLE_BAR_ROOM: f32 = 48.0;

/// The most room a popup anchored at `anchor` may take, for a parent of
/// `parent_size` among `screens` (both in the parent's logical px): the work
/// area of the screen the ANCHOR is on less a margin on every side, so a
/// popover too big for the screen comes up at the screen's size and scrolls
/// (the placement then keeps it on the screen). Without a screen to go by -
/// Wayland, which never tells, or a layout pass off the main thread - the width
/// is the parent's (what a popup was always measured against there) and the
/// height is not limited; the compositor's positioner keeps the popup on
/// screen there.
#[must_use]
pub fn popup_room(
    parent_size: LogicalSize,
    screens: &[ScreenRects],
    anchor: LogicalRect,
) -> LogicalSize {
    match screen_for_anchor(anchor, screens) {
        Some(wa) => LogicalSize::new(
            (wa.size.width - 2.0 * POPUP_SCREEN_MARGIN).max(1.0),
            (wa.size.height - 2.0 * POPUP_SCREEN_MARGIN).max(1.0),
        ),
        None => LogicalSize::new(parent_size.width, f32::INFINITY),
    }
}

/// The room a popup's content has in `room`: less a title bar, when its
/// window has one.
#[must_use]
pub fn content_room(room: LogicalSize, titled: bool) -> LogicalSize {
    if titled {
        LogicalSize::new(room.width, (room.height - TITLE_BAR_ROOM).max(1.0))
    } else {
        room
    }
}

/// The most room an overlay may take in `bounds` (what [`LayoutWindow::overlay_bounds`] answers):
/// the bounds less a margin on every side, so a panel too big for the window comes up at the
/// window's size and scrolls, and never touches the edge. The Dom pass caps the overlay's root
/// to the same room ([`overlay_room_for`]), so what is measured is what is laid out.
#[must_use]
pub fn overlay_room(bounds: LogicalRect) -> LogicalSize {
    LogicalSize::new(
        (bounds.size.width - 2.0 * POPUP_SCREEN_MARGIN).max(1.0),
        (bounds.size.height - 2.0 * POPUP_SCREEN_MARGIN).max(1.0),
    )
}

/// [`overlay_room`] of a window of `window` size whose safe area takes `insets` (`[top, bottom,
/// left, right]`) and whose keyboard takes `keyboard` from the bottom: the room the Dom pass
/// caps an overlay to, from what it can know while it builds the DOM.
#[must_use]
pub fn overlay_room_for(window: LogicalSize, insets: [f32; 4], keyboard: f32) -> LogicalSize {
    overlay_room(overlay_bounds_of(window, insets, keyboard))
}

/// The rect inside `window` that a floating overlay may be in: the window less the safe area
/// (`insets` as `[top, bottom, left, right]`), and less the keyboard, which covers the bottom bar
/// and everything above it up to its own height.
#[must_use]
pub fn overlay_bounds_of(window: LogicalSize, insets: [f32; 4], keyboard: f32) -> LogicalRect {
    let [top, bottom, left, right] = insets;
    let bottom = bottom.max(keyboard.max(0.0));
    LogicalRect::new(
        LogicalPosition::new(left, top),
        LogicalSize::new(
            (window.width - left - right).max(0.0),
            (window.height - top - bottom).max(0.0),
        ),
    )
}

/// Whether the window of the `<transient-window>` at `node` has a title bar:
/// its `titlebar` attribute, else a dialog window (`anchor="dialog"`) has
/// one and a popover none. On a window with one surface the engine draws what
/// the title bar would have (a sheet's header).
#[must_use]
pub fn transient_has_titlebar(styled: &StyledDom, node: NodeId, anchor: TransientAnchor) -> bool {
    use azul_core::dom::AttributeType;
    styled
        .node_data
        .as_container()
        .get(node)
        .and_then(|nd| {
            nd.attributes().as_ref().iter().find_map(|a| match a {
                AttributeType::Titlebar(on) => Some(*on),
                _ => None,
            })
        })
        .unwrap_or(anchor == TransientAnchor::Dialog)
}

/// The title of a transient window - a popup, a torn-off toplevel, a dialog
/// window, a sheet: the `<transient-window>` node's `title` attribute, else its
/// accessible name (`aria-label`, what a modal names its window by), else
/// `parent_title`. A dialog the app did not name belongs to the window that
/// opened it, so it carries that window's title, as a message box does.
#[must_use]
pub fn transient_window_title(styled: &StyledDom, node: NodeId, parent_title: &str) -> String {
    use azul_core::dom::AttributeType;
    let nodes = styled.node_data.as_container();
    nodes
        .get(node)
        .and_then(|nd| {
            nd.attributes().as_ref().iter().find_map(|a| match a {
                AttributeType::Title(t) | AttributeType::AriaLabel(t)
                    if !t.as_str().trim().is_empty() =>
                {
                    Some(t.as_str().to_owned())
                }
                _ => None,
            })
        })
        .unwrap_or_else(|| parent_title.to_owned())
}

impl LayoutWindow {
    /// The rect inside the window an overlay may be in: the window less its safe area and the
    /// keyboard ([`overlay_bounds_of`]), as this window stands on them now.
    #[must_use]
    pub fn overlay_bounds(&self, window_state: &FullWindowState) -> LogicalRect {
        let insets = self.effective_safe_area_insets(window_state);
        let keyboard = insets
            .keyboard
            .into_option()
            .and_then(|k| k.to_pixels_absolute().into_option())
            .unwrap_or(0.0);
        overlay_bounds_of(
            window_state.size.dimensions,
            Self::safe_area_px(&insets),
            keyboard,
        )
    }

    /// Find the open `<transient-window>`s in the root dom, measure what each shows, place it, and
    /// reconcile the manager. The result is the diff the shell takes ([`Self::take_transient_diff`]),
    /// accumulated: a layout may run several passes before the shell looks.
    ///
    /// On a window that [presents inline](Self::effective_presentation) every popup becomes an
    /// overlay: it gets a box in the window instead of a surface. On any other window the pass is
    /// what every shell always ran, with the room each popup measures in taken from `screens_of`
    /// (every screen around the window in its logical px; empty where the shell cannot tell).
    ///
    /// Split out of the layout funnel because it needs `&mut self` twice in ways the borrow checker
    /// will not allow inline: once to read the root layout (for anchor rects) and once to lay out
    /// each popup's content.
    pub fn reconcile_transients(
        &mut self,
        window_state: &FullWindowState,
        screens_of: &mut dyn FnMut() -> Vec<ScreenRects>,
    ) {
        // Whether a popup held the keyboard when this pass's display list was built: the list
        // rings the invoker only if none did.
        let keyboard_owned_before = self.transient_keyboard_owner().is_some();
        let presentation = self.effective_presentation(window_state);

        // 1. What does the parent layout say is open, and where is each anchor?
        let wanted = {
            let Some(root) = self.layout_results.get(&DomId::ROOT_ID) else {
                return;
            };
            let styled = &root.styled_dom;
            // Anchor rects come from the ROOT dom's layout. Borrow the result immutably for the whole
            // collection, then drop it before laying out.
            let forced: Vec<_> = self.transient_windows.forced_open_nodes().to_vec();
            // A window the user docked onto a drop zone anchors to the zone.
            let overrides = self.transient_windows.anchor_overrides();
            // Anchors are VIEWPORT rects (scroll taken off): the popup must open where the anchor
            // is on screen, not where the unscrolled layout has it.
            collect_open_transient_windows(styled, &forced, &overrides, |node| {
                self.get_node_rect_in_viewport(DomNodeId {
                    dom: DomId::ROOT_ID,
                    node: NodeHierarchyItemId::from_crate_internal(Some(node)),
                })
            })
        };
        // `anchor="viewport"` (a modal dialog's top layer) covers the whole window instead of
        // hanging off its anchor node: anchored to the viewport and laid out at its size, so a
        // resize of this window resizes the cover on the next pass.
        let window = window_state.size.dimensions;
        let wanted: Vec<_> = wanted.into_iter().map(|p| p.cover_viewport(window)).collect();

        // The window an overlay is in: the pointer, for the popup that opens at it.
        let cursor = match window_state.mouse_state.cursor_position {
            azul_core::window::CursorPosition::InWindow(p) => Some(p),
            _ => None,
        };
        let bounds = self.overlay_bounds(window_state);
        let env = ReconcileEnv {
            presentation,
            window,
            bounds,
            cursor,
        };

        // 2. Reconcile, measuring each popup's content on demand (on scratch caches - a popup
        //    window lays the content out itself, an overlay is laid out by the root's next pass).
        let diff = {
            // `reconcile` wants a closure that reads this window while the manager is borrowed
            // mutably - split the borrow by taking the manager out, reconciling, and putting it back.
            let mut manager = core::mem::take(&mut self.transient_windows);
            // Where focus is as popups open: owed back on close, whichever way they opened (see
            // `remember_focus_for_opened` below).
            let focus_at_open = self
                .focus_manager
                .get_focused_node()
                .copied()
                .map(|n| (n, self.focus_manager.focus_is_visible));
            // The screens around this window in its logical px, asked once a pass and only when a
            // popup is measured; each popup gets the room of the screen its anchor is on.
            let mut screens: Option<Vec<ScreenRects>> = None;
            let diff = manager.reconcile_in(&wanted, &env, |content_dom, placement| {
                // The room: an overlay has the window; a popup the screen its anchor is on.
                let room = if presentation == WindowPresentation::OneSurface
                    && placement.dock == azul_core::transient::TransientDock::Popup
                {
                    overlay_room(bounds)
                } else {
                    let screens = screens.get_or_insert_with(|| screens_of());
                    popup_room(window, screens, placement.anchor_rect)
                };
                // A window with a title bar has that much less room for its content. (An overlay
                // draws its own header inside the room: the Dom pass caps it there.)
                let titled = presentation == WindowPresentation::Windows
                    && self
                        .layout_results
                        .get(&DomId::ROOT_ID)
                        .is_some_and(|r| {
                            transient_has_titlebar(&r.styled_dom, placement.node, placement.anchor)
                        });
                let room = content_room(room, titled);
                let measured = self.layout_transient_content(
                    placement.node,
                    content_dom,
                    placement.size,
                    window_state,
                    room,
                )?;
                // A DROP-DOWN IS AT LEAST AS WIDE AS THE CONTROL IT DROPS OUT OF. A popup on the
                // top or bottom edge is the `<select>` shape - a combo box's option list, a date
                // field's calendar - and one narrower than its own field reads as a stray context
                // menu (2026-09-01 request, alongside the same rule for native menus). A MINIMUM
                // only: wider content still wins, and an app that gave an explicit `size` has
                // already been handed it back above, so this cannot override it. Left/right anchors
                // are side panels, where the anchor's width means nothing.
                let widened = if matches!(
                    placement.anchor,
                    TransientAnchor::Bottom | TransientAnchor::Top
                ) {
                    LogicalSize::new(
                        measured
                            .width
                            .max(placement.anchor_rect.size.width)
                            .min(room.width.max(measured.width)),
                        measured.height,
                    )
                } else {
                    measured
                };
                Some(widened)
            });
            // A window opened by its `open` ATTRIBUTE reached no callback seam that could have
            // recorded the focus it owes back.
            manager.remember_focus_for_opened(&diff, focus_at_open);
            self.transient_windows = manager;
            diff
        };

        // The app's `torn` attribute tore a window off / docked it: the node hears about it like it
        // hears about a drag.
        for (node, torn, bounds) in &diff.torn_changes {
            #[cfg(feature = "std")]
            let now = std::time::Instant::now().into();
            #[cfg(not(feature = "std"))]
            let now = azul_core::task::Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 });
            self.pending_lifecycle_events
                .push(azul_core::diff::create_tearoff_event(
                    *node,
                    DomId::ROOT_ID,
                    &now,
                    *torn,
                    *bounds,
                ));
        }
        // Accumulate, never assign: a layout call may run several passes before the backend takes
        // the diff, and an exit that reaches no reconcile (the fingerprint-equal "layout unchanged"
        // path) must not replay an older pass's opened/closed list either - the continuity test
        // caught a popup about to be re-created that way. `merge` also cancels an open+close pair
        // the backend never saw, so nothing flashes.
        self.pending_transient_diff.merge(diff);

        // A popup took the keyboard (its invoker stops being ringed) or handed it back (the ring
        // returns). The list was built BEFORE this reconcile, with the old answer, so rebuild it now.
        if self.transient_keyboard_owner().is_some() != keyboard_owned_before {
            self.refresh_focus_ring();
        }
    }
}
