//! A native command that stands for a keystroke runs AS that keystroke
//! (EVENTS7, wave 7): `PlatformWindow::press_shortcut_keys`.
//!
//! WRITER6: on native macOS the Edit menu's Undo applied the engine's text
//! undo directly (`edit_command` -> `apply_system_change(UndoTextEdit)`)
//! before any key handler ran, so the rich-text editor - which owns its undo
//! history and takes Cmd+Z in its key handler - lost Undo to the engine. A
//! menu command now runs its keystroke through the ordinary key passes: the
//! app's key handlers first, then the key's default action unless a handler
//! vetoed it.

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{DomId, DomNodeId, NodeId},
    events::{EventFilter, FocusEventFilter},
    styled_dom::NodeHierarchyItemId,
    window::VirtualKeyCode,
};

use super::*;
use crate::desktop::shell2::common::event::PlatformWindow;

/// One KeyDown as the editor's key handler saw it: the key, whether the
/// primary modifier was held, whether Shift was.
type SeenKey = (Option<VirtualKeyCode>, bool, bool);

type KeyLog = Arc<std::sync::Mutex<Vec<SeenKey>>>;

/// The editor's key handler: it logs the key and, like an editor that owns
/// its undo history, vetoes the key's default action.
extern "C" fn log_key_down(
    mut data: RefAny,
    mut info: azul_layout::callbacks::CallbackInfo,
) -> Update {
    let keyboard = info.get_current_keyboard_state();
    if let Some(log) = data.downcast_ref::<KeyLog>() {
        log.lock().unwrap().push((
            keyboard.current_virtual_keycode.into_option(),
            keyboard.primary_down(),
            keyboard.shift_down(),
        ));
    }
    info.prevent_default();
    Update::DoNothing
}

/// `body > div[contenteditable, key handler] > "abc"`; the app state is the
/// handler's log.
extern "C" fn editor_with_key_log(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let log = data
        .downcast_ref::<KeyLog>()
        .map(|l| l.clone())
        .expect("the key log");
    Dom::create_body().with_child(
        Dom::create_div()
            .with_contenteditable(true)
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    callback: CoreCallback {
                        cb: log_key_down as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(log),
                }]
                .into(),
            )
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "abc",
            )),
    )
}

/// The key the platform's shortcuts are held with: Cmd on a Mac, Ctrl
/// elsewhere (`KeyModifiers::primary_down`).
fn primary_key() -> VirtualKeyCode {
    if azul_core::window::mac_shortcut_conventions() {
        VirtualKeyCode::LWin
    } else {
        VirtualKeyCode::LControl
    }
}

#[test]
fn a_menu_command_runs_as_its_keystroke_through_the_key_handlers_first() {
    let log: KeyLog = Arc::default();
    let state = Arc::new(RefCell::new(RefAny::new(log.clone())));
    let mut window = make_window_with(&state, editor_with_key_log);
    window.regenerate_layout().expect("initial layout");
    let _ = window.common.take_regeneration();
    window
        .common
        .layout_window
        .as_mut()
        .expect("the window has a layout")
        .focus_manager
        .set_focused_node(Some(DomNodeId {
            dom: DomId { inner: 0 },
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        }));

    // Edit > Undo.
    let _ = window.press_shortcut_keys(&[primary_key(), VirtualKeyCode::Z], "test.menu.undo");
    assert_eq!(
        log.lock().unwrap().clone(),
        vec![(Some(VirtualKeyCode::Z), true, false)],
        "the editor's key handler sees Undo as its keystroke, primary + Z, once"
    );
    let keyboard = window.common.current_window_state().keyboard_state.clone();
    assert!(
        keyboard.pressed_virtual_keycodes.as_slice().is_empty(),
        "the keystroke is released again: {:?}",
        keyboard.pressed_virtual_keycodes
    );
    assert!(!keyboard.primary_down());

    // Edit > Redo: primary + Shift + Z.
    let _ = window.press_shortcut_keys(
        &[primary_key(), VirtualKeyCode::LShift, VirtualKeyCode::Z],
        "test.menu.redo",
    );
    assert_eq!(
        log.lock().unwrap().last().copied(),
        Some((Some(VirtualKeyCode::Z), true, true)),
        "Redo is primary + Shift + Z"
    );
}

#[test]
fn a_menu_command_leaves_a_key_the_user_holds_held() {
    let log: KeyLog = Arc::default();
    let state = Arc::new(RefCell::new(RefAny::new(log.clone())));
    let mut window = make_window_with(&state, editor_with_key_log);
    window.regenerate_layout().expect("initial layout");
    let _ = window.common.take_regeneration();

    // The user holds Shift while picking the item with the pointer.
    window
        .common
        .keyboard_state_mut()
        .pressed_virtual_keycodes
        .insert_hm_item(VirtualKeyCode::LShift);
    window.common.keyboard_state_mut().sync_modifiers();
    window.discard_input_delta("test.shift.held");

    let _ = window.press_shortcut_keys(
        &[primary_key(), VirtualKeyCode::LShift, VirtualKeyCode::Z],
        "test.menu.redo",
    );
    let keyboard = window.common.current_window_state().keyboard_state.clone();
    assert_eq!(
        keyboard.pressed_virtual_keycodes.as_slice(),
        &[VirtualKeyCode::LShift][..],
        "only what the command pressed comes up again"
    );
}
