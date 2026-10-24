//! Clearing a text field tells its app: Backspace over the field's last
//! character (or over a select-all) leaves the engine's buffer EMPTY, and the
//! `TextInput`'s (and the `TextArea`'s) `on_text_input` hook must see the
//! empty value.
//!
//! The widget mirrors the engine's buffer on every `Input` notification
//! (`adopt_engine_text`), but refused to adopt an EMPTY read over a non-empty
//! mirror ("an empty answer is ambiguous" - it was, before the engine's read
//! descended into block wrappers and answered `None` for a missing node). So
//! the field was cleared on screen and the app never heard it: AzMonitor's
//! filter kept filtering by the old text, and the next word typed was
//! appended to it (E2E, 2026-10-06).

use std::{
    cell::RefCell,
    sync::{Arc, Mutex},
};

use azul::desktop::shell2::{
    common::{event::SharedUndoManager, PlatformWindow},
    headless::HeadlessWindow,
};
use azul_core::{
    callbacks::{
        FocusTarget, FocusTargetPath, LayoutCallback, LayoutCallbackInfo, LayoutCallbackType,
        Update,
    },
    dom::{Dom, DomId},
    events::ProcessEventResult,
    geom::LogicalSize,
    icon::{IconProviderHandle, SharedIconProvider},
    id::NodeId,
    refany::{OptionRefAny, RefAny},
    resources::AppConfig,
    window::{OptionVirtualKeyCode, VirtualKeyCode},
};
use azul_css::css::{CssPath, CssPathSelector};
use azul_layout::{
    callbacks::{CallbackChange, CallbackInfo},
    widgets::{
        text_area::{TextArea, TextAreaOnTextInputCallbackType, TextAreaState},
        text_input::{
            OnTextInputReturn, TextInput, TextInputOnTextInputCallbackType, TextInputState,
            TextInputValid,
        },
    },
    window_state::WindowCreateOptions,
};
use rust_fontconfig::FcFontCache;

/// Every value the field's `on_text_input` hook was handed, in order.
#[derive(Clone, Default)]
struct Seen(Arc<Mutex<Vec<String>>>);

extern "C" fn on_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(seen) = data.downcast_ref::<Seen>() {
        seen.0.lock().unwrap().push(state.get_text());
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// The `TextArea` twin of [`on_text`].
extern "C" fn on_area_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextAreaState,
) -> OnTextInputReturn {
    if let Some(seen) = data.downcast_ref::<Seen>() {
        seen.0.lock().unwrap().push(state.get_text());
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// body > TextArea#field holding "a". The model never changes.
extern "C" fn area_layout(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let hook: TextAreaOnTextInputCallbackType = on_area_text;
    Dom::create_body().with_child(
        TextArea::create()
            .with_text("a".into())
            .with_on_text_input(data, hook)
            .dom()
            .with_id("field".into()),
    )
}

/// body > TextInput#field holding "a". The model never changes.
extern "C" fn layout_cb(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let hook: TextInputOnTextInputCallbackType = on_text;
    Dom::create_body().with_child(
        TextInput::create()
            .with_text("a".into())
            .with_on_text_input(data, hook)
            .dom()
            .with_id("field".into()),
    )
}

fn make_window(seen: Seen, layout: LayoutCallbackType) -> HeadlessWindow {
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize::new(400.0, 200.0);
    options.window_state.layout_callback = LayoutCallback {
        cb: layout,
        ctx: OptionRefAny::None,
    };
    HeadlessWindow::new(
        options,
        Arc::new(RefCell::new(RefAny::new(seen))),
        SharedUndoManager::new(),
        AppConfig::default(),
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::build()),
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

/// `service_frame`'s routing, without the render.
fn honor(window: &mut HeadlessWindow, tier: ProcessEventResult) {
    use azul_core::events::ProcessEventResult as R;
    if tier >= R::ShouldRegenerateDomCurrentWindow {
        window
            .regenerate_layout()
            .expect("regenerate_layout after RefreshDom");
    } else if tier == R::ShouldIncrementalRelayout {
        let _ = window.relayout_only();
    }
    window.common.drain_virtual_view_updates();
}

/// The KeyDown / KeyUp arms of `HeadlessWindow::run`: snapshot, key state,
/// event pass, honor the result.
fn press_key(window: &mut HeadlessWindow, vk: VirtualKeyCode) {
    window.snapshot_window_state_baseline("test.key_down");
    window.common.keyboard_state_mut().current_virtual_keycode = OptionVirtualKeyCode::Some(vk);
    window
        .common
        .keyboard_state_mut()
        .pressed_virtual_keycodes
        .insert_hm_item(vk);
    let down = window.process_window_events(0);
    honor(window, down);

    window.snapshot_window_state_baseline("test.key_up");
    window.common.keyboard_state_mut().current_virtual_keycode = OptionVirtualKeyCode::None;
    window
        .common
        .keyboard_state_mut()
        .pressed_virtual_keycodes
        .remove_hm_item(&vk);
    let up = window.process_window_events(0);
    honor(window, up);
}

/// The field's text as the engine reads it for the next edit.
fn field_text(window: &HeadlessWindow) -> String {
    let lw = window.common.layout_window.as_ref().expect("layout_window");
    let lr = lw.layout_results.get(&DomId::ROOT_ID).expect("root dom");
    let nodes = lr.styled_dom.node_data.as_container();
    let field = (0..nodes.len())
        .map(NodeId::new)
        .find(|n| nodes.get(*n).is_some_and(|nd| nd.has_id("field")))
        .expect("the field carries its id");
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, field);
    lw.extract_text_from_inline_content(&content)
}

/// Focus the field holding "a", press Backspace: the app's hook must see "".
fn backspace_clears_the_field_and_the_app_hears_it(layout: LayoutCallbackType) {
    let seen = Seen::default();
    let mut window = make_window(seen.clone(), layout);
    window.regenerate_layout().expect("initial layout");
    let _ = window.common.take_regeneration();

    // Focus the field: a contenteditable host seeds its caret at the END.
    window.snapshot_window_state_baseline("test.focus");
    let r = window.apply_user_change(&CallbackChange::SetFocusTarget {
        target: FocusTarget::Path(FocusTargetPath {
            dom: DomId::ROOT_ID,
            css_path: CssPath {
                selectors: vec![CssPathSelector::Id("field".into())].into(),
            },
        }),
    });
    honor(&mut window, r);
    assert_eq!(field_text(&window), "a", "premise: the field holds \"a\"");

    press_key(&mut window, VirtualKeyCode::Back);
    assert_eq!(
        field_text(&window),
        "",
        "premise: Backspace deleted the field's one character"
    );
    let values = seen.0.lock().unwrap().clone();
    assert_eq!(
        values.last().map(String::as_str),
        Some(""),
        "the app's on_text_input saw the empty field (it saw {values:?})"
    );
}

#[test]
fn backspace_over_the_last_character_tells_the_app_the_field_is_empty() {
    backspace_clears_the_field_and_the_app_hears_it(layout_cb);
}

#[test]
fn backspace_over_the_last_character_tells_the_app_the_text_area_is_empty() {
    backspace_clears_the_field_and_the_app_hears_it(area_layout);
}
