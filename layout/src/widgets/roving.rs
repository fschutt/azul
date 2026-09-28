//! WAI-ARIA APG "roving tabindex" for composite widgets.
//!
//! A radio group, a segmented control, a tab list, a listbox, a tree and a
//! date grid are each ONE stop in the Tab order. Tab lands on the group's
//! active item (the checked radio, the selected segment, the active tab, the
//! selected row, the chosen day) - or on its first item when nothing is
//! active - and the next Tab leaves the group. Inside the group the ARROW keys
//! move between items, and the Tab stop moves with them:
//!
//! * the active item is built with `TabIndex::Auto`, every other item with
//!   `TabIndex::NoKeyboardFocus` (focusable by click and from code, never by
//!   Tab) - [`item_tab_index`] / [`stop_index`];
//! * an arrow handler (`EventFilter::Focus(VirtualKeyDown)` on every item)
//!   works out the target item, rewrites the tab indices through
//!   `CallbackInfo::set_tab_index` and focuses the target - [`move_stop`];
//!   a click only rewrites them, because the click already focused the row -
//!   [`set_stop`];
//! * a handled key calls `prevent_default`, so the engine's spatial
//!   navigation does not ALSO move focus. A key held with Alt, Ctrl, Cmd or
//!   Shift is left alone - [`plain_key`] - so OS and app shortcuts survive.
//!
//! The mechanics live here so every composite widget behaves the same; which
//! keys do what (wrap or not, move-and-select or move-only, Home/End, the
//! grid's week/month steps) stays with each widget.

use alloc::vec::Vec;

use azul_core::{
    callbacks::FocusTarget,
    dom::{DomNodeId, TabIndex},
    window::{KeyboardState, VirtualKeyCode},
};

use crate::callbacks::CallbackInfo;

/// The tab index item `index` of a roving group is built with when item
/// `stop` holds the group's one Tab stop.
#[must_use]
pub(crate) const fn item_tab_index(index: usize, stop: usize) -> TabIndex {
    if index == stop {
        TabIndex::Auto
    } else {
        TabIndex::NoKeyboardFocus
    }
}

/// Which item of a `count`-item group holds the Tab stop: `active` when it
/// names an item, the first item otherwise (APG: "if no item is selected,
/// focus lands on the first").
#[must_use]
pub(crate) const fn stop_index(active: Option<usize>, count: usize) -> usize {
    match active {
        Some(i) if i < count => i,
        _ => 0,
    }
}

/// The key a composite widget may claim: the key being pressed, or `None`
/// when Alt, Ctrl, Cmd (super) or Shift is held. Those chords belong to the
/// OS and the app (Alt+arrow is "back" in many shells, Cmd+arrow jumps
/// lines), so the widget must neither act on them nor `prevent_default`.
#[must_use]
pub(crate) fn plain_key(ks: &KeyboardState) -> Option<VirtualKeyCode> {
    if ks.alt_down() || ks.ctrl_down() || ks.super_down() || ks.shift_down() {
        return None;
    }
    ks.current_virtual_keycode.into_option()
}

/// A move along a one-dimensional group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Previous,
    Next,
    First,
    Last,
}

/// The item a `step` from `current` lands on, in a group of `count` items.
///
/// `wrap` makes Previous on the first item land on the last and Next on the
/// last land on the first, as a radio group and a tab list do; without it the
/// ends hold, as in a listbox or a tree. `None` for an empty group. A
/// `current` past the end is treated as the last item.
#[must_use]
pub(crate) const fn step_target(
    current: usize,
    count: usize,
    step: Step,
    wrap: bool,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let last = count - 1;
    let current = if current > last { last } else { current };
    Some(match step {
        Step::First => 0,
        Step::Last => last,
        Step::Previous => {
            if current > 0 {
                current - 1
            } else if wrap {
                last
            } else {
                0
            }
        }
        Step::Next => {
            if current < last {
                current + 1
            } else if wrap {
                0
            } else {
                last
            }
        }
    })
}

/// The children of `parent`, in document order.
#[must_use]
pub(crate) fn children_of(info: &CallbackInfo, parent: DomNodeId) -> Vec<DomNodeId> {
    let mut out = Vec::new();
    let mut cur = info.get_first_child(parent);
    while let Some(node) = cur {
        out.push(node);
        cur = info.get_next_sibling(node);
    }
    out
}

/// Whether `node` carries the class `class`.
#[must_use]
pub(crate) fn has_class(info: &CallbackInfo, node: DomNodeId, class: &str) -> bool {
    info.get_node_classes(node)
        .as_ref()
        .iter()
        .any(|c| c.as_str() == class)
}

/// The children of `parent` that carry `class`, in document order: a group's
/// items, told apart by their class from anything else among the children
/// (a tab list's spacers) - and from the inner nodes of an item, should a
/// hit ever resolve to one of those.
#[must_use]
pub(crate) fn items_of(info: &CallbackInfo, parent: DomNodeId, class: &str) -> Vec<DomNodeId> {
    children_of(info, parent)
        .into_iter()
        .filter(|n| has_class(info, *n, class))
        .collect()
}

/// Makes `items[stop]` the group's one Tab stop WITHOUT moving focus - for a
/// click, which has already focused the clicked item.
///
/// Every item is written, not just the old and the new stop: the group then
/// heals whatever state its stop was in (a stale build, an out-of-range
/// selection) instead of trusting it.
pub(crate) fn set_stop(info: &mut CallbackInfo, items: &[DomNodeId], stop: usize) {
    if stop >= items.len() {
        return;
    }
    for (i, item) in items.iter().enumerate() {
        info.set_tab_index(*item, item_tab_index(i, stop));
    }
}

/// Makes `items[stop]` the group's one Tab stop and focuses it - what an
/// arrow key does.
pub(crate) fn move_stop(info: &mut CallbackInfo, items: &[DomNodeId], stop: usize) {
    let Some(target) = items.get(stop).copied() else {
        return;
    };
    set_stop(info, items, stop);
    info.set_focus(FocusTarget::Id(target));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_stop_is_built_as_a_tab_stop() {
        assert_eq!(item_tab_index(2, 2), TabIndex::Auto);
        assert_eq!(item_tab_index(0, 2), TabIndex::NoKeyboardFocus);
        assert_eq!(item_tab_index(3, 2), TabIndex::NoKeyboardFocus);
    }

    #[test]
    fn the_stop_is_the_active_item_or_else_the_first() {
        assert_eq!(stop_index(Some(2), 4), 2);
        assert_eq!(stop_index(Some(4), 4), 0, "an out-of-range active item");
        assert_eq!(stop_index(None, 4), 0);
        assert_eq!(stop_index(None, 0), 0);
    }

    #[test]
    fn steps_wrap_only_when_asked_to() {
        assert_eq!(step_target(0, 3, Step::Previous, true), Some(2));
        assert_eq!(step_target(2, 3, Step::Next, true), Some(0));
        assert_eq!(step_target(0, 3, Step::Previous, false), Some(0));
        assert_eq!(step_target(2, 3, Step::Next, false), Some(2));
        assert_eq!(step_target(1, 3, Step::Next, false), Some(2));
        assert_eq!(step_target(1, 3, Step::Previous, false), Some(0));
        assert_eq!(step_target(1, 3, Step::First, true), Some(0));
        assert_eq!(step_target(1, 3, Step::Last, true), Some(2));
        assert_eq!(step_target(9, 3, Step::Previous, false), Some(1));
        assert_eq!(step_target(0, 0, Step::Next, true), None);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)] // keyboard state built key by key
    fn a_held_modifier_leaves_the_key_to_the_os_and_the_app() {
        let mut ks = KeyboardState::default();
        ks.current_virtual_keycode = Some(VirtualKeyCode::Down).into();
        ks.pressed_virtual_keycodes = vec![VirtualKeyCode::Down].into();
        assert_eq!(plain_key(&ks), Some(VirtualKeyCode::Down));
        for m in [
            VirtualKeyCode::LAlt,
            VirtualKeyCode::RControl,
            VirtualKeyCode::LWin,
            VirtualKeyCode::LShift,
        ] {
            ks.pressed_virtual_keycodes = vec![m, VirtualKeyCode::Down].into();
            assert_eq!(plain_key(&ks), None, "{m:?}+Down must not be claimed");
        }
    }
}

/// Test harness shared by the composite widgets' tests: it drives a widget's
/// REAL key handler (the one registered on the focused node) and the REAL Tab
/// order (`resolve_focus_target`) over a laid-out-free `StyledDom`.
#[cfg(test)]
pub(crate) mod test_support {
    use std::{
        collections::{BTreeMap, BTreeSet, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        callbacks::{FocusTarget, Update},
        dom::{DomId, DomNodeId, EventFilter},
        events::FocusEventFilter,
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle, VirtualKeyCode},
    };
    use azul_css::system::SystemStyle;
    use rust_fontconfig::FcFontCache;

    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{
            Callback, CallbackChange, CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks,
        },
        managers::focus_cursor::{resolve_focus_target, FocusResolution},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    /// A `DomLayoutResult` with an EMPTY layout tree: the Tab order and the
    /// hierarchy walks a key handler does read only the styled DOM, so no
    /// real layout (and no font) is needed.
    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(DisplayList::default()),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// The nodes `presses` Tab presses land on, one after the other, starting
    /// with focus on `from` (`None` = nothing focused). Shift+Tab when
    /// `forward` is false. This is the engine's own Tab order.
    pub(crate) fn tab_walk(
        styled: &StyledDom,
        from: Option<DomNodeId>,
        forward: bool,
        presses: usize,
    ) -> Vec<DomNodeId> {
        let mut results = BTreeMap::new();
        results.insert(DomId::ROOT_ID, layout_result(styled.clone()));
        let target = if forward {
            FocusTarget::Next
        } else {
            FocusTarget::Previous
        };
        let mut at = from;
        let mut out = Vec::with_capacity(presses);
        for _ in 0..presses {
            match resolve_focus_target(&target, &results, at, &BTreeSet::new()) {
                Ok(FocusResolution::Resolved(n)) => {
                    out.push(n);
                    at = Some(n);
                }
                _ => break,
            }
        }
        out
    }

    /// Applies the `SetNodeTabIndex` writes in `changes` to `styled`, as the
    /// shell does after the callback returns.
    pub(crate) fn apply_tab_index_writes(styled: &mut StyledDom, changes: &[CallbackChange]) {
        for change in changes {
            if let CallbackChange::SetNodeTabIndex {
                node_id, tab_index, ..
            } = change
            {
                if node_id.index() < styled.node_data.as_ref().len() {
                    styled.node_data.as_container_mut()[*node_id].set_tab_index(*tab_index);
                }
            }
        }
    }

    /// Presses `key` - holding the `held` modifier keys - while `focused` has
    /// keyboard focus: runs the `Focus(VirtualKeyDown)` handler the widget
    /// registered ON THAT NODE, with the payload it registered, the way the
    /// engine dispatches a key to the focused node.
    ///
    /// `None` when the node carries no key handler at all.
    pub(crate) fn press(
        styled: &StyledDom,
        focused: DomNodeId,
        key: VirtualKeyCode,
        held: &[VirtualKeyCode],
    ) -> Option<(Update, Vec<CallbackChange>)> {
        let node_id = focused.node.into_crate_internal()?;
        let key_down = EventFilter::Focus(FocusEventFilter::VirtualKeyDown);
        let (core_cb, data) = styled
            .node_data
            .as_container()
            .get(node_id)?
            .get_callbacks()
            .as_ref()
            .iter()
            .find(|cb| cb.event == key_down)
            .map(|cb| (cb.callback.clone(), cb.refany.clone()))?;

        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window
            .layout_results
            .insert(DomId::ROOT_ID, layout_result(styled.clone()));

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let mut current_window_state = FullWindowState::default();
        current_window_state.keyboard_state.current_virtual_keycode = Some(key).into();
        let mut pressed: Vec<VirtualKeyCode> = held.to_vec();
        pressed.push(key);
        current_window_state.keyboard_state.pressed_virtual_keycodes = pressed.into();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            focused,
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );
        let update = Callback::from_core(core_cb).invoke(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        Some((update, recorded))
    }

    /// The node the callback asked to focus (the last `FocusTarget::Id`).
    pub(crate) fn focus_request(changes: &[CallbackChange]) -> Option<DomNodeId> {
        changes.iter().rev().find_map(|c| match c {
            CallbackChange::SetFocusTarget {
                target: FocusTarget::Id(n),
            } => Some(*n),
            _ => None,
        })
    }

    /// Whether the callback cancelled the key's default action (spatial
    /// navigation / scrolling).
    pub(crate) fn prevented(changes: &[CallbackChange]) -> bool {
        changes
            .iter()
            .any(|c| matches!(c, CallbackChange::PreventDefault))
    }
}
