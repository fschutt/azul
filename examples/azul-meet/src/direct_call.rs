//! Direct calls wired to the iroh endpoint and the window; the messages, the safety words and the
//! recent calls are `direct.rs`.
//!
//! A connection comes up three ways. The one this side dialed for its call says hello at once.
//! One from an id the call expects (the roster) joins the call at once. Any other, while this
//! side is in no meetings-service meeting, waits for its hello and then prompts. While this side
//! is in such a meeting, a hello is answered "busy". Once a call is taken the connection is a
//! peer like a meeting's (`accept_peer`), and the call screen, the routing and the media run as
//! they do in a meeting; nothing is announced to a meetings service.
//!
//! Lines for scripts: `AZMEET_DIRECT_ADDRESS <ticket>` when the endpoint is up,
//! `AZMEET_DIRECT_DIALING`, `AZMEET_DIRECT_RINGING <words>`, `AZMEET_DIRECT_INCOMING <words>
//! <name>`, `AZMEET_DIRECT_CALL <words> <name>`, `AZMEET_DIRECT_DECLINED <busy|no>`,
//! `AZMEET_DIRECT_BUSY <name>`, `AZMEET_DIRECT_JOINED <name>`, `AZMEET_DIRECT_LEFT <name>`.

use super::*;
use crate::direct::{self, DirectMsg, Incoming, Knock, Outgoing, Recent, RosterPeer};

/// This endpoint's id, empty while it is not bound.
fn own_id(s: &MeetState) -> String {
    s.endpoint
        .as_ref()
        .map(|endpoint| endpoint.endpoint_id().as_str().to_string())
        .unwrap_or_default()
}

/// This endpoint's full ticket: the one announced last, else the endpoint's now.
fn own_ticket(s: &MeetState) -> String {
    let announced = s
        .room
        .as_ref()
        .map(|room| room.ticket.clone())
        .filter(|ticket| !ticket.is_empty());
    announced.unwrap_or_else(|| {
        s.endpoint
            .as_ref()
            .map(|endpoint| endpoint.ticket().as_str().to_string())
            .unwrap_or_default()
    })
}

/// This computer's address in the form the direct-call screen shows.
pub(crate) fn own_address(s: &MeetState) -> String {
    direct::address(s.direct.form, &own_ticket(s), &own_id(s))
}

/// Sends `msg` on connection `handle`.
fn send(s: &MeetState, handle: u64, msg: &DirectMsg) {
    if let Some(endpoint) = s.endpoint.as_ref() {
        endpoint.send_message(handle, direct::encode(msg));
    }
}

fn disconnect(s: &MeetState, handle: u64) {
    if let Some(endpoint) = s.endpoint.as_ref() {
        endpoint.disconnect(handle);
    }
}

/// Seconds since 1970, for a recent call.
fn now_secs() -> i64 {
    i64::try_from(wall_ms() / 1000).unwrap_or(i64::MAX)
}

/// Keeps `call` among the recent calls, in the settings.
fn remember_call(s: &mut MeetState, call: Recent) {
    direct::remember(&mut s.direct.recents, call);
    let text = direct::recents_to_text(&s.direct.recents);
    remember(s, |prefs| prefs.direct_calls = Some(text));
}

/// A connection came up (`node_id` on connection `handle`). `Some(changed)` when it belongs to
/// a direct call (made, expected or perhaps coming in); `None` for a meeting's peers.
pub(crate) fn on_connected(s: &mut MeetState, handle: u64, node_id: &str) -> Option<bool> {
    let now = now_ms(s);
    // A newcomer the call expects (its roster): in at once, no prompt.
    if s.direct.in_call {
        if let Some(pos) = s.direct.expected.iter().position(|p| p.id.eq_ignore_ascii_case(node_id)) {
            let person = s.direct.expected.remove(pos);
            println!("AZMEET_DIRECT_JOINED {}", person.name);
            eprintln!("[azmeet] {}: {} joined the direct call", s.name, person.name);
            s.direct.people.push((handle, person));
            return Some(accept_peer(s, handle, node_id.to_string()));
        }
    }
    // The call this side dials came up: say hello, and the safety words while it rings.
    let me = own_id(s);
    let (name, ticket) = (s.name.clone(), own_ticket(s));
    if let Some(out) = s.direct.outgoing.as_mut() {
        let ours = out.handle.is_none()
            && out
                .id
                .as_deref()
                .map_or(true, |want| want.eq_ignore_ascii_case(node_id));
        if ours {
            out.handle = Some(handle);
            out.id = Some(node_id.to_string());
            out.words = direct::words_line(direct::safety_words(&me, node_id));
            println!("AZMEET_DIRECT_RINGING {}", out.words);
            send(s, handle, &DirectMsg::Hello { name, ticket });
            return Some(true);
        }
    }
    // In a meetings-service meeting: the meeting's peers (a caller hears "busy" on its hello).
    if accepts_peers(s) && !s.direct.in_call {
        return None;
    }
    // Someone calling, perhaps: a caller once it says hello.
    s.direct.knocks.push(Knock {
        handle,
        id: node_id.to_string(),
        since_ms: now,
    });
    Some(false)
}

/// Connection `handle` is gone: a knock, the prompt it made, the call being made on it or its
/// person in the call. True when the window changes.
pub(crate) fn on_disconnected(s: &mut MeetState, handle: u64) -> bool {
    let mut changed = false;
    s.direct.knocks.retain(|k| k.handle != handle);
    if s.direct.incoming.as_ref().is_some_and(|c| c.handle == handle) {
        s.direct.incoming = None;
        s.direct.note = t("azmeet-direct-caller-gone");
        changed = true;
    }
    if s.direct.outgoing.as_ref().is_some_and(|o| o.handle == Some(handle)) {
        s.direct.outgoing = None;
        s.direct.note = t("azmeet-direct-no-answer");
        changed = true;
    }
    if let Some(pos) = s.direct.people.iter().position(|(h, _)| *h == handle) {
        let (_, person) = s.direct.people.remove(pos);
        println!("AZMEET_DIRECT_LEFT {}", person.name);
        changed = true;
    }
    changed
}

/// After a connection went: a direct call nobody else is in any more ends.
pub(crate) fn end_if_alone(s: &mut MeetState) -> bool {
    if !s.direct.in_call || !s.direct.people.is_empty() || !s.remotes.is_empty() {
        return false;
    }
    let _ = leave_meeting(s);
    s.notice = t("azmeet-direct-ended");
    true
}

/// A direct-call message on connection `handle`. True when the window changes.
pub(crate) fn on_message(s: &mut MeetState, handle: u64, msg: DirectMsg) -> bool {
    match msg {
        DirectMsg::Hello { name, ticket } => on_hello(s, handle, &name, ticket),
        DirectMsg::Accept { name, ticket } => on_accepted(s, handle, &name, ticket),
        DirectMsg::Decline { busy } => on_declined(s, handle, busy),
        DirectMsg::Roster { peers } => on_roster(s, handle, peers),
        DirectMsg::Expect { peer } => on_expect(s, handle, peer),
    }
}

/// A caller said hello: the prompt, unless this side is busy.
fn on_hello(s: &mut MeetState, handle: u64, name: &str, ticket: String) -> bool {
    let name = chatroom::clean_name(name);
    let id = match s.direct.knocks.iter().position(|k| k.handle == handle) {
        Some(pos) => s.direct.knocks.remove(pos).id,
        None => s
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.peer_endpoint_id(handle).as_str().to_string())
            .unwrap_or_default(),
    };
    let in_meeting = accepts_peers(s) && !s.direct.in_call;
    if in_meeting || s.direct.incoming.is_some() || id.is_empty() {
        println!("AZMEET_DIRECT_BUSY {name}");
        send(s, handle, &DirectMsg::Decline { busy: true });
        // A meeting took it as a peer waiting for its announcement: it never comes.
        s.pending.retain(|p| p.handle != handle);
        disconnect(s, handle);
        return false;
    }
    let words = direct::words_line(direct::safety_words(&own_id(s), &id));
    println!("AZMEET_DIRECT_INCOMING {words} {name}");
    eprintln!("[azmeet] {}: {name} calls directly ({})", s.name, short_id(&id));
    s.direct.incoming = Some(Incoming {
        handle,
        id,
        name,
        ticket,
        words,
    });
    true
}

/// The call this side made was taken.
fn on_accepted(s: &mut MeetState, handle: u64, name: &str, ticket: String) -> bool {
    if s.direct.outgoing.as_ref().map(|o| o.handle) != Some(Some(handle)) {
        return false;
    }
    let Some(out) = s.direct.outgoing.take() else {
        return false;
    };
    let name = chatroom::clean_name(name);
    let address = if ticket.is_empty() { out.address.clone() } else { ticket.clone() };
    remember_call(
        s,
        Recent {
            name: name.clone(),
            address,
            at: now_secs(),
            outgoing: true,
        },
    );
    let person = RosterPeer {
        id: out.id.unwrap_or_default(),
        name,
        ticket,
    };
    start_call(s, handle, person, out.words)
}

/// The call this side made was declined.
fn on_declined(s: &mut MeetState, handle: u64, busy: bool) -> bool {
    if s.direct.outgoing.as_ref().map(|o| o.handle) != Some(Some(handle)) {
        return false;
    }
    s.direct.outgoing = None;
    s.direct.note = t(if busy { "azmeet-direct-busy" } else { "azmeet-direct-declined" });
    println!("AZMEET_DIRECT_DECLINED {}", if busy { "busy" } else { "no" });
    disconnect(s, handle);
    true
}

/// From someone in the call: the others in it, which this side, the newcomer, dials.
fn on_roster(s: &mut MeetState, handle: u64, peers: Vec<RosterPeer>) -> bool {
    if !s.direct.in_call || !s.direct.in_call_with(handle) {
        return false;
    }
    let me = own_id(s);
    for peer in peers {
        let known = peer.id.eq_ignore_ascii_case(&me)
            || s.direct.name_of(&peer.id).is_some()
            || peer.ticket.is_empty();
        if known {
            continue;
        }
        if let Some(endpoint) = s.endpoint.as_ref() {
            let _dialing = endpoint.connect(peer.ticket.as_str());
        }
        s.direct.expected.push(peer);
    }
    false
}

/// From someone in the call: a newcomer to expect, whose connection joins without a prompt.
fn on_expect(s: &mut MeetState, handle: u64, peer: RosterPeer) -> bool {
    if s.direct.in_call && s.direct.in_call_with(handle) && s.direct.name_of(&peer.id).is_none() {
        s.direct.expected.push(peer);
    }
    false
}

/// Into the call with `person` on connection `handle` (`words`: their safety words): the call
/// screen, without a meetings service.
fn start_call(s: &mut MeetState, handle: u64, person: RosterPeer, words: String) -> bool {
    if !s.direct.in_call {
        s.direct.in_call = true;
        s.direct.open = false;
        s.direct.words = words.clone();
        kit::close_settings(&s.kit);
        if let Some(room) = s.room.as_mut() {
            room.enter_direct();
        }
        s.notice = t_args("azmeet-direct-in-call", &[("words", Arg::from(words.as_str()))]);
        s.link_status = String::from("in a direct call");
    }
    println!("AZMEET_DIRECT_CALL {words} {}", person.name);
    eprintln!("[azmeet] {}: in a direct call with {}", s.name, person.name);
    let id = person.id.clone();
    s.direct.people.push((handle, person));
    accept_peer(s, handle, id)
}

/// Every room tick: a connection that said no hello in time goes, and so does a call nobody
/// took. True when the window changes.
pub(crate) fn tick(s: &mut MeetState) -> bool {
    let now = now_ms(s);
    let stale: Vec<u64> = s
        .direct
        .knocks
        .iter()
        .filter(|k| now.saturating_sub(k.since_ms) >= direct::HELLO_MS)
        .map(|k| k.handle)
        .collect();
    for handle in stale {
        s.direct.knocks.retain(|k| k.handle != handle);
        disconnect(s, handle);
    }
    let rang_out = s
        .direct
        .outgoing
        .as_ref()
        .is_some_and(|o| now.saturating_sub(o.since_ms) >= direct::RING_MS);
    if rang_out {
        if let Some(handle) = s.direct.outgoing.take().and_then(|o| o.handle) {
            disconnect(s, handle);
        }
        s.direct.note = t("azmeet-direct-no-answer");
        return true;
    }
    false
}

// ==== What the window shows ====

/// The direct-call screen's facts.
pub(crate) fn view(s: &MeetState) -> ui::DirectView {
    let address = own_address(s);
    let status = match (&s.direct.outgoing, s.direct.note.is_empty()) {
        (Some(out), _) if out.handle.is_some() => t_args("azmeet-direct-ringing", &[("words", Arg::from(out.words.as_str()))]),
        (Some(_), _) => t("azmeet-direct-dialing"),
        (None, false) => s.direct.note.clone(),
        (None, true) => String::new(),
    };
    let recents = s
        .direct
        .recents
        .iter()
        .map(|r| ui::RecentRow {
            name: ui::person_name(&r.name),
            address: r.address.clone(),
            when: times::at_text(r.at, local_offset_at(u64::try_from(r.at).unwrap_or(0))),
            outgoing: r.outgoing,
        })
        .collect();
    ui::DirectView {
        address,
        id_only: s.direct.form == direct::AddressForm::IdOnly,
        copied: s.direct.copied,
        call_text: s.direct.call_text.clone(),
        calling: s.direct.outgoing.is_some(),
        status,
        recents,
    }
}

/// The prompt of a call coming in.
pub(crate) fn incoming_view(s: &MeetState) -> Option<ui::IncomingView> {
    s.direct.incoming.as_ref().map(|call| ui::IncomingView {
        name: ui::person_name(&call.name),
        words: call.words.clone(),
    })
}

// ==== The buttons ====

/// Settings > Meetings' "Direct call...": the direct-call screen.
pub(crate) extern "C" fn on_open(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    kit::close_settings(&s.kit);
    s.direct.open = true;
    s.direct.copied = false;
    Update::RefreshDom
}

/// The direct-call screen's Back: the start screen.
pub(crate) extern "C" fn on_back(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    guard.direct.open = false;
    Update::RefreshDom
}

/// "With direct addresses" / "Id only".
pub(crate) extern "C" fn on_form(mut data: RefAny, _info: CallbackInfo, state: SegmentedState) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    guard.direct.form = if state.selected_index == 1 {
        direct::AddressForm::IdOnly
    } else {
        direct::AddressForm::WithAddresses
    };
    guard.direct.copied = false;
    Update::RefreshDom
}

/// "Copy": this computer's address to the clipboard.
pub(crate) extern "C" fn on_copy(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let address = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        guard.direct.copied = true;
        own_address(&guard)
    };
    if address.is_empty() {
        return Update::DoNothing;
    }
    copy_text(&mut info, &address);
    Update::RefreshDom
}

/// The Call field as typed.
pub(crate) extern "C" fn on_text(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        guard.direct.call_text = state.get_text().as_str().to_string();
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// "Call": dials the address typed.
pub(crate) extern "C" fn on_call(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    dial(&mut guard)
}

/// A recent call's "Call": its address in the field, dialed.
pub(crate) extern "C" fn on_recent(data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, address)) = row_click(data) else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    guard.direct.call_text = address;
    dial(&mut guard)
}

/// Dials the Call field's address.
fn dial(s: &mut MeetState) -> Update {
    if s.direct.outgoing.is_some() || s.direct.in_call {
        return Update::DoNothing;
    }
    let address = s.direct.call_text.trim().to_string();
    if address.is_empty() {
        s.direct.note = t("azmeet-direct-no-address");
        return Update::RefreshDom;
    }
    if address == own_ticket(s) || address.eq_ignore_ascii_case(&own_id(s)) {
        s.direct.note = t("azmeet-direct-yourself");
        return Update::RefreshDom;
    }
    let Some(endpoint) = s.endpoint.clone() else {
        s.direct.note = t("azmeet-direct-no-network");
        return Update::RefreshDom;
    };
    if !endpoint.connect(address.as_str()) {
        s.direct.note = t("azmeet-direct-cannot-dial");
        return Update::RefreshDom;
    }
    println!("AZMEET_DIRECT_DIALING");
    let id = direct::is_bare_id(&address).then(|| address.to_ascii_lowercase());
    s.direct.note.clear();
    s.direct.outgoing = Some(Outgoing {
        address,
        handle: None,
        id,
        words: String::new(),
        since_ms: now_ms(s),
    });
    Update::RefreshDom
}

/// "Cancel" while it rings.
pub(crate) extern "C" fn on_cancel(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(out) = s.direct.outgoing.take() else {
        return Update::DoNothing;
    };
    if let Some(handle) = out.handle {
        disconnect(s, handle);
    }
    s.direct.note = t("azmeet-direct-cancelled");
    Update::RefreshDom
}

/// The prompt's "Accept": into the call. Someone else in it already: the newcomer gets the
/// roster, the others expect the newcomer.
pub(crate) extern "C" fn on_accept(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(call) = s.direct.incoming.take() else {
        return Update::DoNothing;
    };
    send(
        s,
        call.handle,
        &DirectMsg::Accept {
            name: s.name.clone(),
            ticket: own_ticket(s),
        },
    );
    let newcomer = RosterPeer {
        id: call.id.clone(),
        name: call.name.clone(),
        ticket: call.ticket.clone(),
    };
    let others: Vec<RosterPeer> = s.direct.people.iter().map(|(_, p)| p.clone()).collect();
    if !others.is_empty() {
        send(s, call.handle, &DirectMsg::Roster { peers: others });
        let handles: Vec<u64> = s.direct.people.iter().map(|(h, _)| *h).collect();
        for handle in handles {
            send(s, handle, &DirectMsg::Expect { peer: newcomer.clone() });
        }
    }
    remember_call(
        s,
        Recent {
            name: call.name.clone(),
            address: call.ticket.clone(),
            at: now_secs(),
            outgoing: false,
        },
    );
    start_call(s, call.handle, newcomer, call.words);
    save_prefs(s, &mut info);
    Update::RefreshDom
}

/// The prompt's "Decline".
pub(crate) extern "C" fn on_decline(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(call) = s.direct.incoming.take() else {
        return Update::DoNothing;
    };
    send(s, call.handle, &DirectMsg::Decline { busy: false });
    disconnect(s, call.handle);
    println!("AZMEET_DIRECT_DECLINED_HERE {}", call.name);
    Update::RefreshDom
}
