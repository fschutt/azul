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
    events::{DefaultAction, DefaultActionResult, EventType, ScrollAmount, ScrollDirection, SyntheticEvent},
    styled_dom::NodeHierarchyItemId,
    window::{KeyboardState, VirtualKeyCode},
};
use crate::window::DomLayoutResult;
use std::collections::BTreeMap;

/// Determine the default action for a keyboard event.
///
/// This function examines the keyboard state and focused element to determine
/// what default action (if any) should be performed.
///
/// # Arguments
///
/// * `keyboard_state` - Current keyboard state with pressed key
/// * `focused_node` - Currently focused node (if any)
/// * `layout_results` - DOM layout information for querying node properties
/// * `prevented` - Whether `prevent_default()` was called during event dispatch
///
/// # Returns
///
/// A `DefaultActionResult` indicating what action to perform, or `None` if prevented.
pub fn determine_keyboard_default_action(
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
    let current_key = match keyboard_state.current_virtual_keycode.into_option() {
        Some(key) => key,
        None => return DefaultActionResult::default(),
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
            if let Some(ref focus) = focused_node {
                if is_element_activatable(focus, layout_results) {
                    DefaultAction::ActivateFocusedElement {
                        target: focus.clone(),
                    }
                } else {
                    // Enter on non-activatable element - might submit form
                    // For now, no action (form handling could be added later)
                    DefaultAction::None
                }
            } else {
                DefaultAction::None
            }
        }

        // Activation (Space key)
        VirtualKeyCode::Space => {
            if let Some(ref focus) = focused_node {
                // Space only activates if the focused element is activatable
                // and we're not in a text input
                if is_element_activatable(focus, layout_results)
                    && !is_text_input(focus, layout_results)
                {
                    DefaultAction::ActivateFocusedElement {
                        target: focus.clone(),
                    }
                } else {
                    // Space in text input should insert space (handled by text input system)
                    DefaultAction::None
                }
            } else {
                DefaultAction::None
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
        VirtualKeyCode::Up => {
            if focused_node.is_some() && !is_text_input(&focused_node.as_ref().unwrap(), layout_results) {
                DefaultAction::ScrollFocusedContainer {
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::Line,
                }
            } else {
                DefaultAction::None
            }
        }
        VirtualKeyCode::Down => {
            if focused_node.is_some() && !is_text_input(&focused_node.as_ref().unwrap(), layout_results) {
                DefaultAction::ScrollFocusedContainer {
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::Line,
                }
            } else {
                DefaultAction::None
            }
        }
        VirtualKeyCode::Left => {
            if focused_node.is_some() && !is_text_input(&focused_node.as_ref().unwrap(), layout_results) {
                DefaultAction::ScrollFocusedContainer {
                    direction: ScrollDirection::Left,
                    amount: ScrollAmount::Line,
                }
            } else {
                DefaultAction::None
            }
        }
        VirtualKeyCode::Right => {
            if focused_node.is_some() && !is_text_input(&focused_node.as_ref().unwrap(), layout_results) {
                DefaultAction::ScrollFocusedContainer {
                    direction: ScrollDirection::Right,
                    amount: ScrollAmount::Line,
                }
            } else {
                DefaultAction::None
            }
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
        .map(|node| node.is_activatable())
        .unwrap_or(false)
}

/// Check if an element is a text input (where Space should insert text, not activate).
fn is_text_input(node_id: &DomNodeId, layout_results: &BTreeMap<DomId, DomLayoutResult>) -> bool {
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
    use azul_core::events::{EventFilter, FocusEventFilter};
    node.get_callbacks()
        .iter()
        .any(|cb| matches!(cb.event, EventFilter::Focus(FocusEventFilter::TextInput)))
}

/// Convert a DefaultAction to a FocusTarget for the focus manager.
///
/// This bridges the gap between the abstract DefaultAction and the
/// concrete FocusTarget that the FocusManager understands.
pub fn default_action_to_focus_target(action: &DefaultAction) -> Option<FocusTarget> {
    match action {
        DefaultAction::FocusNext => Some(FocusTarget::Next),
        DefaultAction::FocusPrevious => Some(FocusTarget::Previous),
        DefaultAction::FocusFirst => Some(FocusTarget::First),
        DefaultAction::FocusLast => Some(FocusTarget::Last),
        DefaultAction::ClearFocus => Some(FocusTarget::NoFocus),
        _ => None,
    }
}

/// Create a synthetic click event for element activation.
///
/// When Enter or Space is pressed on an activatable element, we generate
/// a synthetic click event that will be dispatched through the normal
/// event system.
pub fn create_activation_click_event(
    target: &DomNodeId,
    timestamp: azul_core::task::Instant,
) -> SyntheticEvent {
    use azul_core::events::{EventData, EventSource};
    
    SyntheticEvent::new(
        EventType::Click,
        EventSource::Synthetic,
        target.clone(),
        timestamp,
        EventData::None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
