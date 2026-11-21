//! The window's other controls: the side panel, the chat field, the settings' devices, quality and
//! switches, the keyboard shortcuts.

use super::*;

/// The side panel's tabs (`ui::PanelView` order): people, chat, statistics. Opening the chat reads
/// it; opening the statistics shows them as they are now.
pub(crate) extern "C" fn on_panel(mut data: RefAny, _info: CallbackInfo, state: SegmentedState) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    s.panel = match state.selected_index {
        1 => SidePanel::Chat,
        2 => SidePanel::Statistics,
        _ => SidePanel::People,
    };
    if s.panel == SidePanel::Chat {
        if let Some(id) = call_room_id(s) {
            if s.chats.get_mut(&id).is_some_and(chatroom::ChatRoom::mark_read) {
                remember_room(s, &id);
            }
        }
    }
    if s.panel == SidePanel::Statistics {
        s.stats_shown = stats_lines(s);
    }
    Update::RefreshDom
}

/// The chat field as typed.
pub(crate) extern "C" fn on_chat_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.chat_draft = state.get_text().as_str().to_string();
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Enter in the chat field sends the message.
pub(crate) extern "C" fn on_chat_key(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let update = match key {
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
            let text = state.get_text().as_str().to_string();
            let field = info.get_hit_node();
            send_draft(&mut data, &mut info, Some(field), &text)
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// The chat's Send button (the field beside it, `ui::chat`, lost the focus first and handed
/// over its text: `on_chat_blur`).
pub(crate) extern "C" fn on_chat_send(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let text = data
        .downcast_ref::<MeetState>()
        .map(|s| s.chat_draft.clone())
        .unwrap_or_default();
    let field = info.get_previous_sibling(info.get_hit_node()).into_option();
    send_draft(&mut data, &mut info, field, &text)
}

/// The chat field lost the focus: the draft as the field holds it (deletions included).
pub(crate) extern "C" fn on_chat_blur(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.chat_draft = state.get_text().as_str().to_string();
    }
    Update::DoNothing
}

/// Sends `text` to the chat and empties the field (`field`: the chat field, when known);
/// nothing for an empty message. The meeting's `chat.jsonl` is written again.
pub(crate) fn send_draft(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    field: Option<DomNodeId>,
    text: &str,
) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    if !send_chat(s, text) {
        return Update::DoNothing;
    }
    s.chat_draft.clear();
    flush_files(s, info);
    // Re-rendering the field with an empty draft is not enough: what the user typed outranks
    // the DOM until the app SETS the text.
    if let Some(field) = field {
        TextInput::set_text_in(*info, field, AzString::from(""));
    }
    Update::RefreshDom
}

/// The name field (the waiting room, the settings' Meetings): the name others see (each room's
/// member record carries it, sealed, once the field is left: `on_name_blur`), remembered - and
/// written when the field is left or the meeting is entered, not with every key.
pub(crate) extern "C" fn on_name_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().trim().to_string();
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        if !text.is_empty() {
            s.name = text.clone();
            remember(s, |prefs| prefs.name = Some(text));
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// The name field was left: the name typed is written into `meet/settings.json`, and every room
/// gets this device's record again with it (sealed with each room's link).
pub(crate) extern "C" fn on_name_blur(
    mut data: RefAny,
    mut info: CallbackInfo,
    _state: TextInputState,
) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        for chat in s.chats.values_mut() {
            chat.set_name(&s.name);
        }
        save_prefs(s, &mut info);
    }
    Update::DoNothing
}

/// The gear (and Mod+, through the kit's keys): azul-appkit's settings page.
pub(crate) extern "C" fn on_settings_open(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(kit_ref) = data.downcast_ref::<MeetState>().map(|s| s.kit.clone()) else {
        return Update::DoNothing;
    };
    kit::open_settings(&kit_ref, None);
    Update::RefreshDom
}

/// The start screen's "Connection settings": the settings page at Meetings, where the
/// meetings service's field is.
pub(crate) extern "C" fn on_connection_settings(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(kit_ref) = data.downcast_ref::<MeetState>().map(|s| s.kit.clone()) else {
        return Update::DoNothing;
    };
    kit::open_settings(&kit_ref, Some(ui::APP_CATEGORIES[1]));
    Update::RefreshDom
}

/// The microphone picked (the waiting room, the settings), remembered by its name.
/// (`MicrophoneWidget` and `AudioSink` open the system's default device on every platform
/// today; the pick is kept for when they take one.)
pub(crate) extern "C" fn on_mic_choice(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.mic_choice = index;
        s.mic_name = device_name(&s.mics, index);
        let name = s.mic_name.clone();
        remember(s, |prefs| prefs.microphone = name);
        save_prefs(s, &mut info);
    }
    Update::RefreshDom
}

/// The speaker picked (see `on_mic_choice`).
pub(crate) extern "C" fn on_speaker_choice(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.speaker_choice = index;
        s.speaker_name = device_name(&s.speakers, index);
        let name = s.speaker_name.clone();
        remember(s, |prefs| prefs.speaker = name);
        save_prefs(s, &mut info);
    }
    Update::RefreshDom
}

/// The camera picked: the camera widget opens the one facing that way; remembered.
pub(crate) extern "C" fn on_camera_choice(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.camera_choice = index.min(CAMERAS.len() - 1);
        let camera = s.camera_choice;
        remember(s, |prefs| prefs.camera = camera);
        save_prefs(s, &mut info);
    }
    Update::RefreshDom
}

/// The highest rendition this side asks for, by the quality picked in the settings.
pub(crate) const QUALITY_CAPS: [u32; 3] = [720, 360, 180];

/// The video quality picked in the settings: what this side asks for changes at once, and it is
/// remembered.
pub(crate) extern "C" fn on_quality(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.quality = index.min(QUALITY_CAPS.len() - 1);
        let quality = s.quality;
        remember(s, |prefs| prefs.quality = quality);
        save_prefs(s, &mut info);
        network_changed(s, false);
    }
    Update::RefreshDom
}

/// "Mirror my video": this side's own picture turned as a mirror shows it, or not; remembered.
pub(crate) extern "C" fn on_mirror(mut data: RefAny, mut info: CallbackInfo, state: CheckBoxState) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.mirror = state.checked;
        remember(s, |prefs| prefs.mirror = state.checked);
        save_prefs(s, &mut info);
    }
    Update::RefreshDom
}

/// "Join with the microphone off": how the waiting room's switch starts; remembered.
pub(crate) extern "C" fn on_join_muted(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: CheckBoxState,
) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.join_muted = state.checked;
        remember(s, |prefs| prefs.join_muted = state.checked);
        save_prefs(s, &mut info);
    }
    Update::DoNothing
}

/// "Join with the camera off": how the waiting room's switch starts; remembered.
pub(crate) extern "C" fn on_join_camera_off(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: CheckBoxState,
) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.join_camera_off = state.checked;
        remember(s, |prefs| prefs.join_camera_off = state.checked);
        save_prefs(s, &mut info);
    }
    Update::DoNothing
}

/// The mode at `index` (`azul_appkit::ModePref` order: system, light, dark) as azul takes it.
pub(crate) fn mode_option(index: usize) -> OptionDarkLightMode {
    match index {
        1 => OptionDarkLightMode::Some(DarkLightMode::Light),
        2 => OptionDarkLightMode::Some(DarkLightMode::Dark),
        _ => OptionDarkLightMode::None,
    }
}

/// The window's keys: the kit's first (Mod+, the settings, F1 the shortcuts, Escape closes
/// them), then AzMeet's (`keys::SHORTCUTS`, the rule in `keys::command_for`): Ctrl / Cmd + D the
/// microphone, Ctrl / Cmd + E the camera.
pub(crate) extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let kit_ref = data.downcast_ref::<MeetState>().map(|s| s.kit.clone());
    if let Some(update) = kit_ref.and_then(|k| kit::handle_key(&k, &mut info)) {
        return update;
    }
    let key = match info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()
    {
        Some(VirtualKeyCode::D) => keys::Key::D,
        Some(VirtualKeyCode::E) => keys::Key::E,
        _ => keys::Key::Other,
    };
    match keys::command_for(key, info.get_key_modifiers().primary_down()) {
        Some(keys::Command::ToggleMic) => mic_toggle(data, info),
        Some(keys::Command::ToggleCamera) => cam_toggle(data, info),
        None => Update::DoNothing,
    }
}
