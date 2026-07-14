//! Default Action Processing for Keyboard Events
//!
//! This module implements W3C-compliant default actions for keyboard events.
//! Default actions are built-in behaviors that occur after event dispatch,
//! unless `event.prevent_default()` was called.
//!
//! ## W3C Event Model
//!
//! Per DOM Level 2/3 and W3C UI Events:
//!
//! 1. Event is dispatched through capture → target → bubble phases
//! 2. Callbacks can call `event.prevent_default()` to cancel default action
//! 3. After dispatch, if not prevented, the default action is performed
//!
//! ## Keyboard Default Actions
//!
//! | Key | Modifiers | Default Action |
//! |-----|-----------|----------------|
//! | Tab | None | Focus next element |
//! | Tab | Shift | Focus previous element |
//! | Enter | None | Activate focused element (if activatable) |
//! | Space | None | Activate focused element (if activatable) |
//! | Escape | None | Clear focus |
//!
//! ## Activation Behavior (HTML5)
//!
//! Per HTML5 spec, elements with "activation behavior" can be activated via
//! Enter or Space. This generates a synthetic click event:
//!
//! - Button elements
//! - Anchor elements with href
//! - Input elements (submit, button, checkbox, radio)
//! - Any element with a click callback
//!
//! See: https://html.spec.whatwg.org/multipage/interaction.html#activation-behavior

use alloc::vec::Vec;
use azul_core::{
    callbacks::FocusTarget,
    dom::{DomId, DomNodeId, NodeId},
    events::{DefaultAction, DefaultActionResult, ScrollAmount, ScrollDirection},
    window::{KeyboardState, VirtualKeyCode},
};
use crate::window::DomLayoutResult;
use std::collections::BTreeMap;

/// Determine the default action for a keyboard event based on the
/// current key, focused element, and whether `prevent_default()` was called.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
#[must_use] pub fn determine_keyboard_default_action(
    keyboard_state: &KeyboardState,
    focused_node: Option<DomNodeId>,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    prevented: bool,
) -> DefaultActionResult {
    // If prevented, return early with no action
    if prevented {
        return DefaultActionResult::prevented();
    }

    // Get the current key (if any)
    let Some(current_key) = keyboard_state.current_virtual_keycode.into_option() else {
        return DefaultActionResult::default();
    };

    // Check modifier state
    let shift_down = keyboard_state.shift_down();
    let ctrl_down = keyboard_state.ctrl_down();
    let alt_down = keyboard_state.alt_down();

    // Determine action based on key
    let action = match current_key {
        // Tab navigation
        VirtualKeyCode::Tab => {
            if ctrl_down || alt_down {
                // Ctrl+Tab / Alt+Tab are typically handled by OS
                DefaultAction::None
            } else if shift_down {
                DefaultAction::FocusPrevious
            } else {
                DefaultAction::FocusNext
            }
        }

        // Activation (Enter key)
        VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => {
            focused_node.as_ref().map_or(DefaultAction::None, |focus| if is_element_activatable(focus, layout_results) {
                    DefaultAction::ActivateFocusedElement {
                        target: *focus,
                    }
                } else {
                    // Enter on non-activatable element - might submit form
                    // For now, no action (form handling could be added later)
                    DefaultAction::None
                })
        }

        // Activation (Space key) — or page-scroll when nothing activatable
        // has focus (MWA-C-scroll: the browser default; Shift+Space pages up).
        VirtualKeyCode::Space => {
            match focused_node.as_ref() {
                Some(focus)
                    if is_element_activatable(focus, layout_results)
                        && !is_text_input(focus, layout_results) =>
                {
                    DefaultAction::ActivateFocusedElement { target: *focus }
                }
                // Space in text input should insert space (handled by text input system)
                Some(focus) if is_text_input(focus, layout_results) => DefaultAction::None,
                _ => DefaultAction::ScrollFocusedContainer {
                    direction: if shift_down {
                        ScrollDirection::Up
                    } else {
                        ScrollDirection::Down
                    },
                    amount: ScrollAmount::Page,
                },
            }
        }

        // Escape - clear focus
        VirtualKeyCode::Escape => {
            if focused_node.is_some() {
                DefaultAction::ClearFocus
            } else {
                // Could close modal/dialog here if any is open
                DefaultAction::None
            }
        }

        // Arrow keys - scroll or navigate
        VirtualKeyCode::Up | VirtualKeyCode::Down | VirtualKeyCode::Left | VirtualKeyCode::Right => {
            let direction = match current_key {
                VirtualKeyCode::Up => ScrollDirection::Up,
                VirtualKeyCode::Down => ScrollDirection::Down,
                VirtualKeyCode::Left => ScrollDirection::Left,
                _ => ScrollDirection::Right,
            };
            // MWA-C-scroll: arrows scroll with NO focused node too (the
            // consumer anchors on the hovered container then) — only a
            // focused text input claims the arrows for caret movement.
            focused_node.as_ref().map_or(
                DefaultAction::ScrollFocusedContainer {
                    direction,
                    amount: ScrollAmount::Line,
                },
                |focus| {
                    if is_text_input(focus, layout_results) {
                        DefaultAction::None
                    } else {
                        DefaultAction::ScrollFocusedContainer {
                            direction,
                            amount: ScrollAmount::Line,
                        }
                    }
                },
            )
        }

        // Page Up/Down
        VirtualKeyCode::PageUp => {
            DefaultAction::ScrollFocusedContainer {
                direction: ScrollDirection::Up,
                amount: ScrollAmount::Page,
            }
        }
        VirtualKeyCode::PageDown => {
            DefaultAction::ScrollFocusedContainer {
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Page,
            }
        }

        // Home/End
        VirtualKeyCode::Home => {
            if ctrl_down {
                // Ctrl+Home - go to start of document
                DefaultAction::FocusFirst
            } else {
                DefaultAction::ScrollFocusedContainer {
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::Document,
                }
            }
        }
        VirtualKeyCode::End => {
            if ctrl_down {
                // Ctrl+End - go to end of document
                DefaultAction::FocusLast
            } else {
                DefaultAction::ScrollFocusedContainer {
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::Document,
                }
            }
        }

        // All other keys - no default action
        _ => DefaultAction::None,
    };

    DefaultActionResult::new(action)
}

/// Check if an element is activatable (can receive synthetic click from Enter/Space).
fn is_element_activatable(node_id: &DomNodeId, layout_results: &BTreeMap<DomId, DomLayoutResult>) -> bool {
    let Some(layout) = layout_results.get(&node_id.dom) else {
        return false;
    };
    let Some(internal_id) = node_id.node.into_crate_internal() else {
        return false;
    };
    layout.styled_dom.node_data.as_container()
        .get(internal_id)
        .is_some_and(azul_core::dom::NodeData::is_activatable)
}

/// Check if an element is a text input (where Space should insert text, not activate).
fn is_text_input(node_id: &DomNodeId, layout_results: &BTreeMap<DomId, DomLayoutResult>) -> bool {
    use azul_core::events::{EventFilter, FocusEventFilter};
    let Some(layout) = layout_results.get(&node_id.dom) else {
        return false;
    };
    let Some(internal_id) = node_id.node.into_crate_internal() else {
        return false;
    };
    let node_data = layout.styled_dom.node_data.as_container();
    let Some(node) = node_data.get(internal_id) else {
        return false;
    };

    // Check if this node has a TextInput callback (FocusEventFilter::TextInput)
    // which indicates it's a text input field
    node.get_callbacks()
        .iter()
        .any(|cb| matches!(cb.event, EventFilter::Focus(FocusEventFilter::TextInput)))
}

/// Convert a `DefaultAction` to a `FocusTarget` for the focus manager.
///
/// This bridges the gap between the abstract `DefaultAction` and the
/// concrete `FocusTarget` that the `FocusManager` understands.
#[must_use] pub const fn default_action_to_focus_target(action: &DefaultAction) -> Option<FocusTarget> {
    match action {
        DefaultAction::FocusNext => Some(FocusTarget::Next),
        DefaultAction::FocusPrevious => Some(FocusTarget::Previous),
        DefaultAction::FocusFirst => Some(FocusTarget::First),
        DefaultAction::FocusLast => Some(FocusTarget::Last),
        DefaultAction::ClearFocus => Some(FocusTarget::NoFocus),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_core::styled_dom::NodeHierarchyItemId;

    #[test]
    #[allow(clippy::field_reassign_with_default)] // struct built incrementally / test setup; a struct literal is not clearer here
    fn test_tab_focus_next() {
        let mut keyboard_state = KeyboardState::default();
        keyboard_state.current_virtual_keycode = Some(VirtualKeyCode::Tab).into();
        
        let result = determine_keyboard_default_action(
            &keyboard_state,
            None,
            &BTreeMap::new(),
            false,
        );
        
        assert!(matches!(result.action, DefaultAction::FocusNext));
        assert!(!result.prevented);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)] // struct built incrementally / test setup; a struct literal is not clearer here
    fn test_shift_tab_focus_previous() {
        let mut keyboard_state = KeyboardState::default();
        keyboard_state.current_virtual_keycode = Some(VirtualKeyCode::Tab).into();
        // Add LShift to pressed keys to simulate Shift being held
        keyboard_state.pressed_virtual_keycodes = vec![VirtualKeyCode::LShift, VirtualKeyCode::Tab].into();
        
        let result = determine_keyboard_default_action(
            &keyboard_state,
            None,
            &BTreeMap::new(),
            false,
        );
        
        assert!(matches!(result.action, DefaultAction::FocusPrevious));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)] // struct built incrementally / test setup; a struct literal is not clearer here
    fn test_escape_clears_focus() {
        let mut keyboard_state = KeyboardState::default();
        keyboard_state.current_virtual_keycode = Some(VirtualKeyCode::Escape).into();
        
        let focused = Some(DomNodeId {
            dom: DomId { inner: 0 },
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        });
        
        let result = determine_keyboard_default_action(
            &keyboard_state,
            focused,
            &BTreeMap::new(),
            false,
        );
        
        assert!(matches!(result.action, DefaultAction::ClearFocus));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)] // struct built incrementally / test setup; a struct literal is not clearer here
    fn test_prevented_returns_no_action() {
        let mut keyboard_state = KeyboardState::default();
        keyboard_state.current_virtual_keycode = Some(VirtualKeyCode::Tab).into();
        
        let result = determine_keyboard_default_action(
            &keyboard_state,
            None,
            &BTreeMap::new(),
            true, // prevented!
        );

        assert!(result.prevented);
        assert!(matches!(result.action, DefaultAction::None));
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)] // KeyboardState is built incrementally in the helpers
mod autotest_generated {
    use std::collections::HashMap;

    use azul_core::{
        a11y::{AccessibilityRole, AccessibilityState, SmallAriaInfo},
        dom::{Dom, NodeData, NodeType},
        events::{EventFilter, FocusEventFilter, HoverEventFilter},
        geom::LogicalRect,
        refany::RefAny,
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::solver3::{display_list::DisplayList, layout_tree::LayoutTree};

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    /// A `NodeData` with a single callback attached. The callback pointer is a
    /// dummy `usize` — nothing in this module ever invokes it, both functions
    /// under test only look at `CoreCallbackData::event`.
    fn node_with_callback(node_type: NodeType, event: EventFilter) -> NodeData {
        let mut nd = NodeData::create_node(node_type);
        nd.add_callback(event, RefAny::new(0u32), 0usize);
        nd
    }

    fn node_with_role(node_type: NodeType, role: AccessibilityRole) -> NodeData {
        let mut nd = NodeData::create_node(node_type);
        nd.set_accessibility_info(SmallAriaInfo::label("label").with_role(role).to_full_info());
        nd
    }

    /// A control that *has* activation behaviour (role `PushButton`) but is
    /// explicitly disabled — `is_activatable` must reject it.
    fn disabled_control() -> NodeData {
        let mut nd = NodeData::create_node(NodeType::Input);
        let mut info = SmallAriaInfo::label("Save")
            .with_role(AccessibilityRole::PushButton)
            .to_full_info();
        info.states = vec![AccessibilityState::Unavailable].into();
        nd.set_accessibility_info(info);
        nd
    }

    /// One DOM whose children each have a *distinct* `NodeType`, so tests can
    /// look a node up by type without hardcoding flatten indices:
    ///
    /// - `Button`   — activatable (inherent), not a text input
    /// - `Div`      — neither
    /// - `TextArea` — text input (has a `Focus(TextInput)` callback), not activatable
    /// - `A`        — activatable (inherent) *and* a text input (pathological overlap)
    /// - `P`        — activatable via a `Hover(LeftMouseUp)` click callback
    /// - `Input`    — role `PushButton` but `Unavailable` → disabled
    /// - `Select`   — activatable purely via the `CheckButton` a11y role
    fn fixture() -> BTreeMap<DomId, DomLayoutResult> {
        let dom = Dom::create_body()
            .with_child(Dom::create_from_data(NodeData::create_button_no_a11y()))
            .with_child(Dom::create_from_data(NodeData::create_div()))
            .with_child(Dom::create_from_data(node_with_callback(
                NodeType::TextArea,
                EventFilter::Focus(FocusEventFilter::TextInput),
            )))
            .with_child(Dom::create_from_data(node_with_callback(
                NodeType::A,
                EventFilter::Focus(FocusEventFilter::TextInput),
            )))
            .with_child(Dom::create_from_data(node_with_callback(
                NodeType::P,
                EventFilter::Hover(HoverEventFilter::LeftMouseUp),
            )))
            .with_child(Dom::create_from_data(disabled_control()))
            .with_child(Dom::create_from_data(node_with_role(
                NodeType::Select,
                AccessibilityRole::CheckButton,
            )));

        let styled_dom = StyledDom::create_from_dom(dom);

        let mut map = BTreeMap::new();
        map.insert(
            DomId::ROOT_ID,
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
                display_list: DisplayList::default(),
                scroll_ids: HashMap::new(),
                scroll_id_to_node_id: HashMap::new(),
            },
        );
        map
    }

    fn dom_node(index: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }
    }

    /// Locate the fixture node with the given `NodeType`. Scanning (instead of
    /// assuming `child i == NodeId(i + 1)`) keeps the tests honest even if the
    /// flatten order or anonymous-box insertion ever changes.
    fn node_of(layouts: &BTreeMap<DomId, DomLayoutResult>, matcher: fn(&NodeType) -> bool) -> DomNodeId {
        let layout = layouts.get(&DomId::ROOT_ID).expect("fixture dom missing");
        let container = layout.styled_dom.node_data.as_container();
        for i in 0..container.len() {
            if container
                .get(NodeId::new(i))
                .is_some_and(|nd| matcher(&nd.node_type))
            {
                return dom_node(i);
            }
        }
        panic!("fixture node not found");
    }

    fn button(l: &BTreeMap<DomId, DomLayoutResult>) -> DomNodeId {
        node_of(l, |t| matches!(t, NodeType::Button))
    }
    fn div(l: &BTreeMap<DomId, DomLayoutResult>) -> DomNodeId {
        node_of(l, |t| matches!(t, NodeType::Div))
    }
    fn textarea(l: &BTreeMap<DomId, DomLayoutResult>) -> DomNodeId {
        node_of(l, |t| matches!(t, NodeType::TextArea))
    }
    fn anchor(l: &BTreeMap<DomId, DomLayoutResult>) -> DomNodeId {
        node_of(l, |t| matches!(t, NodeType::A))
    }
    fn clickable_p(l: &BTreeMap<DomId, DomLayoutResult>) -> DomNodeId {
        node_of(l, |t| matches!(t, NodeType::P))
    }
    fn disabled(l: &BTreeMap<DomId, DomLayoutResult>) -> DomNodeId {
        node_of(l, |t| matches!(t, NodeType::Input))
    }
    fn role_only(l: &BTreeMap<DomId, DomLayoutResult>) -> DomNodeId {
        node_of(l, |t| matches!(t, NodeType::Select))
    }
    fn body(l: &BTreeMap<DomId, DomLayoutResult>) -> DomNodeId {
        node_of(l, |t| matches!(t, NodeType::Body))
    }

    // --- Deliberately broken node ids ---------------------------------

    /// References a `DomId` that is not in the map at all.
    fn missing_dom() -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: 9999 },
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
        }
    }

    /// In-range DOM, node index far past the end of the container.
    fn out_of_bounds_node() -> DomNodeId {
        dom_node(9999)
    }

    /// The "no node" sentinel (`inner == 0` decodes to `None`).
    fn null_node() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::NONE,
        }
    }

    /// Maximal raw encoding. Decodes to `NodeId(usize::MAX - 1)`; the container
    /// lookup must reject it rather than index out of bounds.
    fn max_node() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_raw(usize::MAX),
        }
    }

    fn kbd(key: VirtualKeyCode, mods: &[VirtualKeyCode]) -> KeyboardState {
        let mut ks = KeyboardState::default();
        ks.current_virtual_keycode = Some(key).into();
        let mut pressed = mods.to_vec();
        pressed.push(key);
        ks.pressed_virtual_keycodes = pressed.into();
        ks
    }

    const ALL_KEYS: &[VirtualKeyCode] = &[
        VirtualKeyCode::Tab,
        VirtualKeyCode::Return,
        VirtualKeyCode::NumpadEnter,
        VirtualKeyCode::Space,
        VirtualKeyCode::Escape,
        VirtualKeyCode::Up,
        VirtualKeyCode::Down,
        VirtualKeyCode::Left,
        VirtualKeyCode::Right,
        VirtualKeyCode::PageUp,
        VirtualKeyCode::PageDown,
        VirtualKeyCode::Home,
        VirtualKeyCode::End,
        VirtualKeyCode::F1,
        VirtualKeyCode::Key1,
        VirtualKeyCode::LShift,
        VirtualKeyCode::LControl,
        VirtualKeyCode::LAlt,
    ];

    const MOD_SETS: &[&[VirtualKeyCode]] = &[
        &[],
        &[VirtualKeyCode::LShift],
        &[VirtualKeyCode::RShift],
        &[VirtualKeyCode::LControl],
        &[VirtualKeyCode::RControl],
        &[VirtualKeyCode::LAlt],
        &[VirtualKeyCode::RAlt],
        &[VirtualKeyCode::LControl, VirtualKeyCode::LShift],
        &[VirtualKeyCode::LAlt, VirtualKeyCode::LShift],
        &[
            VirtualKeyCode::LControl,
            VirtualKeyCode::LAlt,
            VirtualKeyCode::LShift,
        ],
    ];

    fn scroll(direction: ScrollDirection, amount: ScrollAmount) -> DefaultAction {
        DefaultAction::ScrollFocusedContainer { direction, amount }
    }

    // ==================================================================
    // determine_keyboard_default_action — prevention & missing key
    // ==================================================================

    #[test]
    fn prevented_beats_every_key_modifier_and_focus_combination() {
        let layouts = fixture();
        let focus_states = [
            None,
            Some(button(&layouts)),
            Some(textarea(&layouts)),
            Some(missing_dom()),
            Some(null_node()),
        ];

        for key in ALL_KEYS {
            for mods in MOD_SETS {
                for focus in focus_states {
                    let result =
                        determine_keyboard_default_action(&kbd(*key, mods), focus, &layouts, true);
                    assert!(result.prevented, "prevent_default() must be reported for {key:?}");
                    assert_eq!(
                        result.action,
                        DefaultAction::None,
                        "a prevented event must never carry an action ({key:?})"
                    );
                }
            }
        }
    }

    #[test]
    fn no_current_key_yields_the_default_result() {
        let layouts = fixture();
        // Modifiers held, keys "pressed", but no `current_virtual_keycode`.
        let mut ks = KeyboardState::default();
        ks.pressed_virtual_keycodes =
            vec![VirtualKeyCode::LShift, VirtualKeyCode::LControl].into();

        let result =
            determine_keyboard_default_action(&ks, Some(button(&layouts)), &layouts, false);
        assert_eq!(result.action, DefaultAction::None);
        assert!(!result.prevented);
    }

    #[test]
    fn never_reports_prevented_when_not_prevented() {
        let layouts = fixture();
        for key in ALL_KEYS {
            for mods in MOD_SETS {
                let result = determine_keyboard_default_action(
                    &kbd(*key, mods),
                    Some(button(&layouts)),
                    &layouts,
                    false,
                );
                assert!(!result.prevented, "{key:?} must not set `prevented`");
            }
        }
    }

    // ==================================================================
    // Tab
    // ==================================================================

    #[test]
    fn tab_with_ctrl_or_alt_yields_no_action() {
        let layouts = fixture();
        for mods in [
            &[VirtualKeyCode::LControl][..],
            &[VirtualKeyCode::RControl][..],
            &[VirtualKeyCode::LAlt][..],
            &[VirtualKeyCode::RAlt][..],
            // Ctrl/Alt must win even when Shift is also down.
            &[VirtualKeyCode::LControl, VirtualKeyCode::LShift][..],
            &[VirtualKeyCode::LAlt, VirtualKeyCode::RShift][..],
        ] {
            let result = determine_keyboard_default_action(
                &kbd(VirtualKeyCode::Tab, mods),
                None,
                &layouts,
                false,
            );
            assert_eq!(
                result.action,
                DefaultAction::None,
                "Ctrl/Alt+Tab belongs to the OS, not the app ({mods:?})"
            );
        }
    }

    #[test]
    fn tab_uses_either_shift_key_and_ignores_focus() {
        let layouts = fixture();
        for shift in [VirtualKeyCode::LShift, VirtualKeyCode::RShift] {
            let result = determine_keyboard_default_action(
                &kbd(VirtualKeyCode::Tab, &[shift]),
                Some(textarea(&layouts)),
                &layouts,
                false,
            );
            assert_eq!(result.action, DefaultAction::FocusPrevious);
        }
        let result = determine_keyboard_default_action(
            &kbd(VirtualKeyCode::Tab, &[]),
            Some(textarea(&layouts)),
            &layouts,
            false,
        );
        assert_eq!(result.action, DefaultAction::FocusNext);
    }

    // ==================================================================
    // Enter / NumpadEnter
    // ==================================================================

    #[test]
    fn enter_activates_every_kind_of_activatable_element() {
        let layouts = fixture();
        for target in [
            button(&layouts),
            anchor(&layouts),
            clickable_p(&layouts),
            role_only(&layouts),
        ] {
            for key in [VirtualKeyCode::Return, VirtualKeyCode::NumpadEnter] {
                let result = determine_keyboard_default_action(
                    &kbd(key, &[]),
                    Some(target),
                    &layouts,
                    false,
                );
                assert_eq!(
                    result.action,
                    DefaultAction::ActivateFocusedElement { target },
                    "{key:?} on an activatable element must activate exactly that element"
                );
            }
        }
    }

    #[test]
    fn enter_on_a_disabled_control_does_not_activate() {
        let layouts = fixture();
        let result = determine_keyboard_default_action(
            &kbd(VirtualKeyCode::Return, &[]),
            Some(disabled(&layouts)),
            &layouts,
            false,
        );
        assert_eq!(
            result.action,
            DefaultAction::None,
            "an Unavailable (disabled) control must never be activated"
        );
    }

    #[test]
    fn enter_on_non_activatable_or_unfocused_yields_no_action() {
        let layouts = fixture();
        for focus in [None, Some(div(&layouts)), Some(body(&layouts))] {
            let result = determine_keyboard_default_action(
                &kbd(VirtualKeyCode::Return, &[]),
                focus,
                &layouts,
                false,
            );
            assert_eq!(result.action, DefaultAction::None);
        }
    }

    #[test]
    fn enter_on_a_dangling_focus_target_does_not_panic() {
        let layouts = fixture();
        let empty: BTreeMap<DomId, DomLayoutResult> = BTreeMap::new();

        // Bogus node ids against a populated map.
        for focus in [
            missing_dom(),
            out_of_bounds_node(),
            null_node(),
            max_node(),
        ] {
            let result = determine_keyboard_default_action(
                &kbd(VirtualKeyCode::Return, &[]),
                Some(focus),
                &layouts,
                false,
            );
            assert_eq!(
                result.action,
                DefaultAction::None,
                "a focus target that cannot be resolved must not be activated"
            );
        }

        // A perfectly valid node id against an empty map.
        let result = determine_keyboard_default_action(
            &kbd(VirtualKeyCode::Return, &[]),
            Some(dom_node(1)),
            &empty,
            false,
        );
        assert_eq!(result.action, DefaultAction::None);
    }

    // ==================================================================
    // Space
    // ==================================================================

    #[test]
    fn space_activates_a_focused_button() {
        let layouts = fixture();
        let target = button(&layouts);
        let result = determine_keyboard_default_action(
            &kbd(VirtualKeyCode::Space, &[]),
            Some(target),
            &layouts,
            false,
        );
        assert_eq!(
            result.action,
            DefaultAction::ActivateFocusedElement { target }
        );
    }

    #[test]
    fn space_in_a_text_input_is_swallowed_even_when_the_element_is_activatable() {
        let layouts = fixture();
        // Plain text input: not activatable at all.
        let result = determine_keyboard_default_action(
            &kbd(VirtualKeyCode::Space, &[]),
            Some(textarea(&layouts)),
            &layouts,
            false,
        );
        assert_eq!(
            result.action,
            DefaultAction::None,
            "Space in a text input must insert text, not scroll or activate"
        );

        // Pathological overlap: an <a> (inherently activatable) that also has a
        // TextInput callback. Text-input behaviour must win over activation,
        // otherwise typing a space in it would fire a synthetic click.
        let result = determine_keyboard_default_action(
            &kbd(VirtualKeyCode::Space, &[]),
            Some(anchor(&layouts)),
            &layouts,
            false,
        );
        assert_eq!(
            result.action,
            DefaultAction::None,
            "text-input behaviour must take precedence over activation for Space"
        );
    }

    #[test]
    fn space_pages_the_scroll_container_when_nothing_activatable_has_focus() {
        let layouts = fixture();
        for focus in [
            None,
            Some(div(&layouts)),
            Some(body(&layouts)),
            Some(disabled(&layouts)),
            Some(missing_dom()),
            Some(null_node()),
            Some(out_of_bounds_node()),
        ] {
            let down = determine_keyboard_default_action(
                &kbd(VirtualKeyCode::Space, &[]),
                focus,
                &layouts,
                false,
            );
            assert_eq!(down.action, scroll(ScrollDirection::Down, ScrollAmount::Page));

            let up = determine_keyboard_default_action(
                &kbd(VirtualKeyCode::Space, &[VirtualKeyCode::RShift]),
                focus,
                &layouts,
                false,
            );
            assert_eq!(
                up.action,
                scroll(ScrollDirection::Up, ScrollAmount::Page),
                "Shift+Space pages up"
            );
        }
    }

    // ==================================================================
    // Escape
    // ==================================================================

    #[test]
    fn escape_clears_focus_only_when_something_is_focused() {
        let layouts = fixture();
        // Even an unresolvable focus target counts as "focused" — Escape only
        // checks `is_some()`, and clearing a dangling focus is still correct.
        for focus in [
            button(&layouts),
            div(&layouts),
            null_node(),
            missing_dom(),
            max_node(),
        ] {
            let result = determine_keyboard_default_action(
                &kbd(VirtualKeyCode::Escape, &[]),
                Some(focus),
                &layouts,
                false,
            );
            assert_eq!(result.action, DefaultAction::ClearFocus);
        }

        let result = determine_keyboard_default_action(
            &kbd(VirtualKeyCode::Escape, &[]),
            None,
            &layouts,
            false,
        );
        assert_eq!(result.action, DefaultAction::None);
    }

    // ==================================================================
    // Arrows / PageUp / PageDown / Home / End
    // ==================================================================

    #[test]
    fn arrow_keys_map_to_their_own_direction_and_scroll_by_line() {
        let layouts = fixture();
        for (key, direction) in [
            (VirtualKeyCode::Up, ScrollDirection::Up),
            (VirtualKeyCode::Down, ScrollDirection::Down),
            (VirtualKeyCode::Left, ScrollDirection::Left),
            (VirtualKeyCode::Right, ScrollDirection::Right),
        ] {
            // No focus, non-text focus and unresolvable focus all scroll.
            for focus in [
                None,
                Some(button(&layouts)),
                Some(div(&layouts)),
                Some(missing_dom()),
                Some(null_node()),
            ] {
                let result = determine_keyboard_default_action(
                    &kbd(key, &[]),
                    focus,
                    &layouts,
                    false,
                );
                assert_eq!(
                    result.action,
                    scroll(direction, ScrollAmount::Line),
                    "{key:?} must scroll one line towards {direction:?}"
                );
            }

            // A focused text input claims the arrows for caret movement.
            let result = determine_keyboard_default_action(
                &kbd(key, &[]),
                Some(textarea(&layouts)),
                &layouts,
                false,
            );
            assert_eq!(
                result.action,
                DefaultAction::None,
                "{key:?} in a text input must move the caret, not scroll"
            );
        }
    }

    #[test]
    fn page_keys_scroll_a_page_regardless_of_modifiers_and_focus() {
        let layouts = fixture();
        for (key, direction) in [
            (VirtualKeyCode::PageUp, ScrollDirection::Up),
            (VirtualKeyCode::PageDown, ScrollDirection::Down),
        ] {
            for mods in MOD_SETS {
                for focus in [None, Some(textarea(&layouts)), Some(button(&layouts))] {
                    let result = determine_keyboard_default_action(
                        &kbd(key, mods),
                        focus,
                        &layouts,
                        false,
                    );
                    assert_eq!(result.action, scroll(direction, ScrollAmount::Page));
                }
            }
        }
    }

    #[test]
    fn home_and_end_switch_between_scrolling_and_focus_on_ctrl() {
        let layouts = fixture();

        for ctrl in [VirtualKeyCode::LControl, VirtualKeyCode::RControl] {
            let home =
                determine_keyboard_default_action(&kbd(VirtualKeyCode::Home, &[ctrl]), None, &layouts, false);
            assert_eq!(home.action, DefaultAction::FocusFirst);

            let end =
                determine_keyboard_default_action(&kbd(VirtualKeyCode::End, &[ctrl]), None, &layouts, false);
            assert_eq!(end.action, DefaultAction::FocusLast);

            // Ctrl still wins when Shift/Alt are also held.
            let home_shift = determine_keyboard_default_action(
                &kbd(VirtualKeyCode::Home, &[ctrl, VirtualKeyCode::LShift, VirtualKeyCode::LAlt]),
                Some(textarea(&layouts)),
                &layouts,
                false,
            );
            assert_eq!(home_shift.action, DefaultAction::FocusFirst);
        }

        // Without Ctrl: scroll to the document start / end. Note this happens
        // even inside a focused text input (no Home/End caret handling here).
        for focus in [None, Some(textarea(&layouts))] {
            let home = determine_keyboard_default_action(
                &kbd(VirtualKeyCode::Home, &[VirtualKeyCode::LShift]),
                focus,
                &layouts,
                false,
            );
            assert_eq!(home.action, scroll(ScrollDirection::Up, ScrollAmount::Document));

            let end = determine_keyboard_default_action(
                &kbd(VirtualKeyCode::End, &[]),
                focus,
                &layouts,
                false,
            );
            assert_eq!(end.action, scroll(ScrollDirection::Down, ScrollAmount::Document));
        }
    }

    #[test]
    fn keys_without_a_default_action_yield_none() {
        let layouts = fixture();
        for key in [
            VirtualKeyCode::F1,
            VirtualKeyCode::Key1,
            VirtualKeyCode::LShift,
            VirtualKeyCode::LControl,
            VirtualKeyCode::LAlt,
        ] {
            for focus in [None, Some(button(&layouts)), Some(textarea(&layouts))] {
                let result =
                    determine_keyboard_default_action(&kbd(key, &[]), focus, &layouts, false);
                assert_eq!(result.action, DefaultAction::None, "{key:?} has no default action");
            }
        }
    }

    // ==================================================================
    // Whole-surface smoke: no panic, deterministic, self-consistent
    // ==================================================================

    #[test]
    fn every_key_modifier_and_focus_combination_is_panic_free_and_deterministic() {
        let layouts = fixture();
        let empty: BTreeMap<DomId, DomLayoutResult> = BTreeMap::new();
        let focus_states = [
            None,
            Some(button(&layouts)),
            Some(div(&layouts)),
            Some(textarea(&layouts)),
            Some(anchor(&layouts)),
            Some(disabled(&layouts)),
            Some(body(&layouts)),
            Some(missing_dom()),
            Some(out_of_bounds_node()),
            Some(null_node()),
            Some(max_node()),
        ];

        for key in ALL_KEYS {
            for mods in MOD_SETS {
                for focus in focus_states {
                    for maps in [&layouts, &empty] {
                        let ks = kbd(*key, mods);
                        let a = determine_keyboard_default_action(&ks, focus, maps, false);
                        let b = determine_keyboard_default_action(&ks, focus, maps, false);
                        assert_eq!(
                            a.action, b.action,
                            "{key:?} must be a pure function of its inputs"
                        );
                        assert_eq!(a.prevented, b.prevented);

                        // An activation can only ever target the focused node.
                        if let DefaultAction::ActivateFocusedElement { target } = a.action {
                            assert_eq!(
                                Some(target),
                                focus,
                                "activation must target the focused node, nothing else"
                            );
                        }
                    }
                }
            }
        }
    }

    // ==================================================================
    // is_element_activatable
    // ==================================================================

    #[test]
    fn is_element_activatable_true_and_false_cases() {
        let layouts = fixture();
        for (node, expected, why) in [
            (button(&layouts), true, "a <button> is inherently activatable"),
            (anchor(&layouts), true, "an <a> is inherently activatable"),
            (clickable_p(&layouts), true, "a click callback grants activation behaviour"),
            (role_only(&layouts), true, "the CheckButton a11y role grants activation behaviour"),
            (div(&layouts), false, "a plain <div> has no activation behaviour"),
            (body(&layouts), false, "the root <body> is not activatable"),
            (textarea(&layouts), false, "a text input is not activatable"),
            (disabled(&layouts), false, "an Unavailable control is not activatable"),
        ] {
            assert_eq!(is_element_activatable(&node, &layouts), expected, "{why}");
        }
    }

    #[test]
    fn is_element_activatable_rejects_every_unresolvable_node_id() {
        let layouts = fixture();
        let empty: BTreeMap<DomId, DomLayoutResult> = BTreeMap::new();

        for node in [
            missing_dom(),
            out_of_bounds_node(),
            null_node(),
            max_node(),
        ] {
            assert!(!is_element_activatable(&node, &layouts));
        }

        // A valid id, but no layout results at all.
        assert!(!is_element_activatable(&button(&layouts), &empty));
        assert!(!is_element_activatable(&dom_node(0), &empty));
    }

    // ==================================================================
    // is_text_input
    // ==================================================================

    #[test]
    fn is_text_input_true_and_false_cases() {
        let layouts = fixture();
        for (node, expected, why) in [
            (textarea(&layouts), true, "a Focus(TextInput) callback marks a text input"),
            (anchor(&layouts), true, "even an <a> counts if it has a TextInput callback"),
            (button(&layouts), false, "a <button> is not a text input"),
            (div(&layouts), false, "a plain <div> is not a text input"),
            (body(&layouts), false, "the root <body> is not a text input"),
            (
                clickable_p(&layouts),
                false,
                "a non-TextInput callback must not be mistaken for a text input",
            ),
            (disabled(&layouts), false, "a disabled control is not a text input"),
        ] {
            assert_eq!(is_text_input(&node, &layouts), expected, "{why}");
        }
    }

    #[test]
    fn is_text_input_rejects_every_unresolvable_node_id() {
        let layouts = fixture();
        let empty: BTreeMap<DomId, DomLayoutResult> = BTreeMap::new();

        for node in [
            missing_dom(),
            out_of_bounds_node(),
            null_node(),
            max_node(),
        ] {
            assert!(!is_text_input(&node, &layouts));
        }

        assert!(!is_text_input(&textarea(&layouts), &empty));
        assert!(!is_text_input(&dom_node(0), &empty));
    }

    #[test]
    fn predicates_are_pure_and_never_both_wrong_for_the_body_root() {
        let layouts = fixture();
        let root = body(&layouts);
        assert_eq!(
            is_element_activatable(&root, &layouts),
            is_element_activatable(&root, &layouts)
        );
        assert_eq!(is_text_input(&root, &layouts), is_text_input(&root, &layouts));
        assert!(!is_element_activatable(&root, &layouts));
        assert!(!is_text_input(&root, &layouts));
    }

    // ==================================================================
    // default_action_to_focus_target
    // ==================================================================

    /// Every `DefaultAction` variant, so the mapping test cannot silently miss
    /// a newly added one.
    fn all_default_actions() -> Vec<DefaultAction> {
        let node = dom_node(1);
        let mut v = vec![
            DefaultAction::FocusNext,
            DefaultAction::FocusPrevious,
            DefaultAction::FocusFirst,
            DefaultAction::FocusLast,
            DefaultAction::ClearFocus,
            DefaultAction::ActivateFocusedElement { target: node },
            DefaultAction::SubmitForm { form_node: node },
            DefaultAction::CloseModal { modal_node: node },
            DefaultAction::SelectAllText,
            DefaultAction::None,
        ];
        for direction in [
            ScrollDirection::Up,
            ScrollDirection::Down,
            ScrollDirection::Left,
            ScrollDirection::Right,
        ] {
            for amount in [ScrollAmount::Line, ScrollAmount::Page, ScrollAmount::Document] {
                v.push(scroll(direction, amount));
            }
        }
        v
    }

    #[test]
    fn focus_actions_map_to_their_focus_target() {
        for (action, target) in [
            (DefaultAction::FocusNext, FocusTarget::Next),
            (DefaultAction::FocusPrevious, FocusTarget::Previous),
            (DefaultAction::FocusFirst, FocusTarget::First),
            (DefaultAction::FocusLast, FocusTarget::Last),
            (DefaultAction::ClearFocus, FocusTarget::NoFocus),
        ] {
            assert_eq!(default_action_to_focus_target(&action), Some(target));
        }
    }

    #[test]
    fn mapping_is_some_exactly_for_focus_actions() {
        for action in all_default_actions() {
            let is_focus_action = matches!(
                action,
                DefaultAction::FocusNext
                    | DefaultAction::FocusPrevious
                    | DefaultAction::FocusFirst
                    | DefaultAction::FocusLast
                    | DefaultAction::ClearFocus
            );
            let mapped = default_action_to_focus_target(&action);
            assert_eq!(
                mapped.is_some(),
                is_focus_action,
                "{action:?} must map to a FocusTarget iff it is a focus action"
            );
            // Non-focus actions (activation, scrolling, ...) must never be
            // turned into a focus move.
            if !is_focus_action {
                assert_eq!(mapped, None);
            }
        }
    }

    #[test]
    fn mapping_is_injective_over_the_focus_actions() {
        let mapped: Vec<FocusTarget> = all_default_actions()
            .iter()
            .filter_map(default_action_to_focus_target)
            .collect();
        assert_eq!(mapped.len(), 5, "exactly five actions move focus");
        let mut deduped = mapped.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(
            deduped.len(),
            mapped.len(),
            "two different focus actions must not collapse onto the same FocusTarget"
        );
    }

    #[test]
    fn mapping_is_usable_in_a_const_context() {
        const NEXT: Option<FocusTarget> = default_action_to_focus_target(&DefaultAction::FocusNext);
        const NOTHING: Option<FocusTarget> =
            default_action_to_focus_target(&DefaultAction::SelectAllText);
        assert_eq!(NEXT, Some(FocusTarget::Next));
        assert_eq!(NOTHING, None);
    }

    // ==================================================================
    // Round trip: key press -> DefaultAction -> FocusTarget
    // ==================================================================

    #[test]
    fn key_presses_round_trip_through_to_the_focus_manager() {
        let layouts = fixture();
        let focus = Some(button(&layouts));

        for (key, mods, expected) in [
            (VirtualKeyCode::Tab, &[][..], Some(FocusTarget::Next)),
            (
                VirtualKeyCode::Tab,
                &[VirtualKeyCode::LShift][..],
                Some(FocusTarget::Previous),
            ),
            (
                VirtualKeyCode::Home,
                &[VirtualKeyCode::LControl][..],
                Some(FocusTarget::First),
            ),
            (
                VirtualKeyCode::End,
                &[VirtualKeyCode::LControl][..],
                Some(FocusTarget::Last),
            ),
            (VirtualKeyCode::Escape, &[][..], Some(FocusTarget::NoFocus)),
            // Not a focus action: activation must not reach the focus manager.
            (VirtualKeyCode::Return, &[][..], None),
            (VirtualKeyCode::PageDown, &[][..], None),
        ] {
            let action =
                determine_keyboard_default_action(&kbd(key, mods), focus, &layouts, false).action;
            assert_eq!(
                default_action_to_focus_target(&action),
                expected,
                "{key:?} + {mods:?} round-trips to the wrong FocusTarget"
            );
        }
    }

    #[test]
    fn a_prevented_key_press_never_reaches_the_focus_manager() {
        let layouts = fixture();
        for key in ALL_KEYS {
            for mods in MOD_SETS {
                let action = determine_keyboard_default_action(
                    &kbd(*key, mods),
                    Some(button(&layouts)),
                    &layouts,
                    true,
                )
                .action;
                assert_eq!(
                    default_action_to_focus_target(&action),
                    None,
                    "{key:?} was prevented, so focus must not move"
                );
            }
        }
    }
}
