//! Space and Enter on a focused button press it, as a pointer does.
//!
//! The user's report: the Submit button's click animation plays for a mouse
//! click, not for Space or Enter. A keyboard activation dispatched the
//! synthetic click and nothing else, so the button never took its `:active`
//! face, and the press animation every theme declares for that face never
//! ran. The control is pressed while the key is down and released when it
//! comes up.

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    styled_dom::NodeHierarchyItemId,
    window::VirtualKeyCode,
};

use super::*;
use crate::desktop::shell2::common::event::PlatformWindow;

/// `body > button`: one themed button, no callback of its own.
extern "C" fn one_button(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body().with_child(azul_layout::widgets::button::Button::create("Send".into()).dom())
}

/// The button's node in the root DOM.
fn button_node(window: &HeadlessWindow) -> NodeId {
    let lw = window.common.layout_window.as_ref().expect("laid out");
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = sd.node_data.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .find(|n| {
            node_data[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| c.as_class() == Some("__azul-native-button"))
        })
        .expect("the button is in the DOM")
}

/// Whether `node` shows its `:active` face.
fn is_pressed(window: &HeadlessWindow, node: NodeId) -> bool {
    let lw = window.common.layout_window.as_ref().expect("laid out");
    lw.layout_results[&DomId::ROOT_ID]
        .styled_dom
        .styled_nodes
        .as_container()[node]
        .styled_node_state
        .active
}

/// `key` goes down (and stays down), as the platform reports it.
fn key_down(window: &mut HeadlessWindow, key: VirtualKeyCode) {
    window.snapshot_window_state_baseline("test.key_down");
    {
        let keyboard = window.get_common_mut().keyboard_state_mut();
        keyboard.pressed_virtual_keycodes.insert_hm_item(key);
        keyboard.current_virtual_keycode = azul_core::window::OptionVirtualKeyCode::Some(key);
        keyboard.is_repeat = false;
        keyboard.sync_modifiers();
    }
    let _ = window.process_window_events(0);
}

/// `key` comes up again.
fn key_up(window: &mut HeadlessWindow, key: VirtualKeyCode) {
    window.snapshot_window_state_baseline("test.key_up");
    {
        let keyboard = window.get_common_mut().keyboard_state_mut();
        keyboard.pressed_virtual_keycodes.remove_hm_item(&key);
        keyboard.current_virtual_keycode = azul_core::window::OptionVirtualKeyCode::None;
        keyboard.sync_modifiers();
    }
    let _ = window.process_window_events(0);
}

#[test]
fn space_and_enter_press_a_focused_button_until_the_key_comes_up() {
    for key in [VirtualKeyCode::Space, VirtualKeyCode::Return] {
        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_with(&state, one_button);
        window.regenerate_layout().expect("initial layout");
        let _ = window.common.take_regeneration();
        let node = button_node(&window);
        window
            .common
            .layout_window
            .as_mut()
            .expect("laid out")
            .focus_manager
            .set_focused_node_with_visibility(
                Some(DomNodeId {
                    dom: DomId::ROOT_ID,
                    node: NodeHierarchyItemId::from_crate_internal(Some(node)),
                }),
                true,
            );
        assert!(!is_pressed(&window, node), "harness: {key:?}: the button rests");

        key_down(&mut window, key);
        assert!(
            is_pressed(&window, node),
            "{key:?} is down on the focused button: it shows its pressed face"
        );
        key_up(&mut window, key);
        assert!(
            !is_pressed(&window, node),
            "{key:?} came up: the button is released"
        );
    }
}
