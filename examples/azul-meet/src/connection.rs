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

/// How often a bind that failed because the platform was not ready yet is tried again (`LateBind`), and how many times.
const BIND_RETRY_MS: u64 = 1_000;
const BIND_RETRIES: u32 = 20;

/// The state of the endpoint's late bind. The endpoint is bound when the first thing that needs it starts, not when the app starts
/// ([`ensure_endpoint`]).
#[derive(Debug, Default)]
pub(crate) struct LateBind {
    /// A feature asked for the local network (the direct-call screen: a call by a bare id finds its peer there): every bind from then on
    /// looks there.
    pub(crate) wanted: bool,
    /// The bound endpoint looks for peers on the local network (`lan_for`: asked for, or no relay at all).
    pub(crate) lan: bool,
    /// The tries that failed because the platform was not ready yet.
    tries: u32,
    /// When (`wall_ms`) the next of them is due.
    retry_at: Option<u64>,
}

/// Whether the engine's `reason` for not binding says that the platform is not ready yet ("the Android context is not ready").
pub(crate) fn not_ready(reason: &str) -> bool {
    reason.to_ascii_lowercase().contains("not ready")
}

impl LateBind {
    /// A try failed with `reason` at `now`: whether another is scheduled. Only a platform that is not ready yet is waited for, and only
    /// [`BIND_RETRIES`] times; any other reason (no engine in the library, no socket) is final.
    pub(crate) fn failed(&mut self, reason: &str, now: u64) -> bool {
        if !not_ready(reason) || self.tries >= BIND_RETRIES {
            self.retry_at = None;
            return false;
        }
        self.tries += 1;
        self.retry_at = Some(now + BIND_RETRY_MS);
        true
    }

    /// Whether a scheduled try is due at `now`.
    pub(crate) fn due(&self, now: u64) -> bool {
        self.retry_at.is_some_and(|at| now >= at)
    }

    /// The endpoint is bound (looking on the local network or not: `lan`): nothing is waited for.
    pub(crate) fn bound(&mut self, lan: bool) {
        self.lan = lan;
        self.tries = 0;
        self.retry_at = None;
    }
}

/// Whether a connection of the endpoint is open (a meeting's peer, a room's chat link, a direct call): an endpoint in use is not replaced.
fn endpoint_in_use(s: &MeetState) -> bool {
    !s.remotes.is_empty()
        || !s.pending.is_empty()
        || !s.chat_conns.is_empty()
        || !s.chat_pending.is_empty()
        || s.direct.in_call
        || s.direct.outgoing.is_some()
}

/// Binds the iroh endpoint when something needs it and it is not bound: a meeting entered, a room's view opened, a direct call opened
/// or dialed. Not when the app starts (`start_rooms`): a bind starts the engine's network, which on iOS and macOS makes the system ask
/// about the local network, and which on Android fails until the activity exists ("the Android context is not ready": that is tried
/// again every second, 20 times, from the room tick, and the start screen stays quiet meanwhile). `lan`: the endpoint also looks for
/// peers on the local network (a direct call by id); an endpoint bound without it is bound again with it when nothing is connected.
/// True when an endpoint is bound afterwards.
pub(crate) fn ensure_endpoint(s: &mut MeetState, lan: bool) -> bool {
    // The wish for the local network stays: every later bind (a retry, a relay that changed) looks there too.
    if lan {
        s.late_bind.wanted = true;
    }
    if s.endpoint.is_some() && (s.late_bind.lan || !lan || endpoint_in_use(s)) {
        return true;
    }
    let Some(room) = s.room.as_ref() else {
        return false;
    };
    let relay = room.relay.clone();
    let wanted = s.late_bind.wanted;
    let endpoint = bind_endpoint(&relay, relay_only(), wanted);
    if !endpoint.is_bound() {
        let reason = bind_failure(&endpoint);
        let again = s.late_bind.failed(&reason, wall_ms());
        if again {
            eprintln!("[azmeet] {}: the network is not ready yet, trying again: {reason}", s.name);
            s.link_status = String::from("starting the network");
        } else {
            eprintln!("[azmeet] {}: no iroh endpoint: {reason}", s.name);
            s.notice = t_args("azmeet-no-network", &[("why", Arg::from(reason.as_str()))]);
            s.link_status = reason;
        }
        return s.endpoint.is_some();
    }
    let node_id = endpoint.endpoint_id().as_str().to_string();
    let looks = lan_for(&relay, wanted);
    eprintln!(
        "[azmeet] {}: endpoint {} (relays {relay:?}{}{})",
        s.name,
        short_id(&node_id),
        if relay_only() { ", relay only" } else { "" },
        if looks { ", local network" } else { "" }
    );
    s.me = routes::peer_key(&node_id);
    s.endpoint = Some(endpoint);
    s.late_bind.bound(looks);
    s.link_status = String::from("not in a meeting");
    if let Some(room) = s.room.as_mut() {
        room.node_id = node_id;
        // The new endpoint's ticket arrives with its `Ready` event.
        room.ticket.clear();
    }
    true
}

/// A bind that failed because the platform was not ready is tried again when its time comes (the room tick).
pub(crate) fn retry_bind(s: &mut MeetState) {
    if s.late_bind.due(wall_ms()) {
        ensure_endpoint(s, false);
    }
}

/// Binds the iroh endpoint anew for `relay` unless it uses that one already. Only outside a
/// meeting (the start screen, the settings opened from it), so no peer is connected. While no
/// endpoint is bound (nothing has needed one yet: [`ensure_endpoint`]) the choice is only kept:
/// the bind that comes uses it, and changing a setting binds nothing.
pub(crate) fn rebind_endpoint(s: &mut MeetState, relay: Relay) {
    let Some(room) = s.room.as_ref() else {
        return;
    };
    if relay == room.relay {
        return;
    }
    if s.endpoint.is_none() {
        println!(
            "AZMEET_TRANSPORT {}",
            rooms::transport_label(&relay, relay_only())
        );
        if let Some(room) = s.room.as_mut() {
            room.relay = relay;
        }
        return;
    }
    let endpoint = bind_endpoint(&relay, relay_only(), s.late_bind.wanted);
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
    s.late_bind.bound(lan_for(&relay, s.late_bind.wanted));
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

#[cfg(test)]
#[path = "connection_test.rs"]
mod connection_test;
