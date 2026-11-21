//! Settings > Meetings' connection: the meetings service's field with its check and Retry, and the
//! iroh relay's choice (Default, Azlin, Custom) that binds the endpoint anew.

use super::*;

/// The meeting server field as typed.
pub(crate) extern "C" fn on_server_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        if let Some(room) = s.room.as_mut() {
            room.server_text = state.get_text().as_str().to_string();
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Enter in the meeting server field takes its address.
pub(crate) extern "C" fn on_server_key(
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
            commit_server(&mut data, &mut info, &text)
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// Leaving the meeting server field takes its address.
pub(crate) extern "C" fn on_server_blur(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> Update {
    let text = state.get_text().as_str().to_string();
    commit_server(&mut data, &mut info, &text)
}

/// The meeting server field was left with `text` (Enter, or it lost focus): a new address is the
/// meeting server for every request from now on, and is checked (`GET /health`); it is saved once
/// it answers. The same address is checked again only when its last check failed.
pub(crate) fn commit_server(data: &mut RefAny, info: &mut CallbackInfo, text: &str) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        if room.stage != Stage::Start {
            return Update::DoNothing;
        }
        room.server_text = text.to_string();
        let Some(server) = rooms::normalize_server(text) else {
            room.server_ok = false;
            room.server_pending = false;
            room.server_status = t("azmeet-not-server-address");
            return Update::RefreshDom;
        };
        if server == room.worker && room.server_ok {
            return Update::DoNothing;
        }
        room.server_text = server.clone();
        room.worker = server;
        room.checks = room.checks.wrapping_add(1);
        room.server_ok = false;
        room.server_pending = true;
        room.server_status = t_args("azmeet-asking-server", &[("server", Arg::from(&room.worker))]);
        let job = HttpJob::health(room);
        rebind_for_server(s);
        job
    };
    spawn_http(info, data.clone(), job);
    Update::RefreshDom
}

// ==== The iroh relay (Settings > Meetings) ====

/// The relay in words: n0's, Azlin's, a company's address, or none.
pub(crate) fn relay_words(relay: &Relay) -> String {
    match (rooms::RelayChoice::of(relay), relay) {
        (Some(rooms::RelayChoice::Default), _) => t("azmeet-relay-default"),
        (Some(rooms::RelayChoice::Azlin), _) => t("azmeet-relay-azlin"),
        (_, Relay::Custom(url)) => url.clone(),
        _ => t("azmeet-relay-none"),
    }
}

/// The relay picked: n0's or Azlin's is used at once (the endpoint is bound anew), and saved;
/// "Custom" shows the field and waits for an address unless one was typed before.
pub(crate) extern "C" fn on_relay_choice(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(choice) = rooms::RelayChoice::ALL.get(index).copied() else {
        return Update::DoNothing;
    };
    let Some(room) = s.room.as_mut().filter(|room| room.stage == Stage::Start) else {
        return Update::DoNothing;
    };
    room.relay_status.clear();
    room.relay_custom = choice == rooms::RelayChoice::Custom;
    let typed = room.relay_text.clone();
    if let Some(value) = rooms::relay_setting(choice, &typed) {
        use_relay(s, &mut info, value);
    }
    Update::RefreshDom
}

/// The custom relay field as typed.
pub(crate) extern "C" fn on_relay_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        if let Some(room) = guard.room.as_mut() {
            room.relay_text = state.get_text().as_str().to_string();
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Enter in the custom relay field takes its address.
pub(crate) extern "C" fn on_relay_key(
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
            commit_relay(&mut data, &mut info, &text)
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// Leaving the custom relay field takes its address.
pub(crate) extern "C" fn on_relay_blur(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> Update {
    let text = state.get_text().as_str().to_string();
    commit_relay(&mut data, &mut info, &text)
}

/// The custom relay field was left with `text`: an address is the relay from now on (and saved);
/// anything else is said under the field.
pub(crate) fn commit_relay(data: &mut RefAny, info: &mut CallbackInfo, text: &str) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(room) = s.room.as_mut().filter(|room| room.stage == Stage::Start) else {
        return Update::DoNothing;
    };
    room.relay_text = text.to_string();
    let Some(value) = rooms::relay_setting(rooms::RelayChoice::Custom, text) else {
        room.relay_status = t("azmeet-relay-not-address");
        return Update::RefreshDom;
    };
    room.relay_status.clear();
    room.relay_text = value.clone();
    use_relay(s, info, value);
    Update::RefreshDom
}

/// Saves the relay setting `value` (`rooms::relay_setting`) and binds the endpoint for the relay
/// it gives (`--relay` still wins for this run).
pub(crate) fn use_relay(s: &mut MeetState, info: &mut CallbackInfo, value: String) {
    let worker = s.room.as_ref().map(|room| room.worker.clone()).unwrap_or_default();
    let relay = relay_for(&worker, Some(value.as_str()));
    remember(s, |prefs| prefs.relay = Some(value));
    save_prefs(s, info);
    if let Some(room) = s.room.as_mut() {
        // An address is used now: the choice shows as what it is.
        room.relay_custom = false;
    }
    rebind_endpoint(s, relay);
}

/// A meeting server on another host may need other relays (none for one on this machine, the
/// public ones otherwise): the endpoint is bound again when that choice changes. Only on the start
/// screen, so no peer is connected.
pub(crate) fn rebind_for_server(s: &mut MeetState) {
    let Some(room) = s.room.as_ref() else {
        return;
    };
    let saved = kit_prefs(&s.kit).relay;
    let relay = relay_for(&room.worker, saved.as_deref());
    rebind_endpoint(s, relay);
}

/// Binds the iroh endpoint anew for `relay` unless it uses that one already. Only outside a
/// meeting (the start screen, the settings opened from it), so no peer is connected.
pub(crate) fn rebind_endpoint(s: &mut MeetState, relay: Relay) {
    let Some(room) = s.room.as_ref() else {
        return;
    };
    if relay == room.relay {
        return;
    }
    let endpoint = bind_endpoint(&relay, relay_only());
    println!(
        "AZMEET_TRANSPORT {}",
        rooms::transport_label(&relay, relay_only())
    );
    if !endpoint.is_bound() {
        let reason = bind_failure(&endpoint);
        eprintln!(
            "[azmeet] {}: no iroh endpoint with relays {relay:?}: {reason}",
            s.name
        );
        return;
    }
    let node_id = endpoint.endpoint_id().as_str().to_string();
    eprintln!(
        "[azmeet] {}: endpoint {} (relays {relay:?}) for the meeting server {}",
        s.name,
        short_id(&node_id),
        room.worker
    );
    s.me = routes::peer_key(&node_id);
    s.endpoint = Some(endpoint);
    if let Some(room) = s.room.as_mut() {
        room.node_id = node_id;
        // The new endpoint's ticket arrives with its `Ready` event.
        room.ticket.clear();
        room.relay = relay;
    }
}

/// The answer to a meeting server check: the line under the field says whether it answers, and a
/// server that answers is remembered in `meet/settings.json` (`flush_files`).
pub(crate) extern "C" fn on_health(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut data, check)) = reply_parts(data) else {
        return Update::DoNothing;
    };
    let answer = http_answer(result);
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(room) = s.room.as_mut() else {
        return Update::DoNothing;
    };
    if room.checks != check {
        // The answer to an address typed over since.
        return Update::DoNothing;
    }
    room.server_pending = false;
    if !matches!(answer, Ok((200, _))) {
        room.server_ok = false;
        room.server_status = server_trouble(&room.worker, &answer);
        return Update::RefreshDom;
    }
    room.server_ok = true;
    room.server_status = t("azmeet-server-answers");
    let worker = room.worker.clone();
    remember(s, |prefs| prefs.server = Some(worker));
    save_prefs(s, &mut info);
    Update::RefreshDom
}
