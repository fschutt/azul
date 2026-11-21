//! A room's chat between calls (ARCHITECTURE.md, "Chat over the peers", step 6). While a room's
//! view is open and no call of it runs here, this device announces a chat-only ticket on the
//! room's rendezvous as a call does, reads who is there, and connects to the members online - a
//! call's participants too. Over these connections only the chat travels (records and history
//! lists, `peer_chat`), never media; with nobody online, what is written waits until someone is.
//!
//! In a call the same connections come from the other side: a viewer of the call's room is
//! listed chat-only, and the call connects to it for the chat alone (`on_call_peers`).
//!
//! For scripts: `AZMEET_CHAT_LINK <room> <peers>` after each read of the room's rendezvous.

use super::*;

/// A connection that carries one room's chat and nothing else.
pub(crate) struct ChatConn {
    pub(crate) handle: u64,
    pub(crate) node_id: String,
    pub(crate) room: String,
    /// Whether the peer has heard which records of the room this side holds (`ChatHave`).
    pub(crate) have_sent: bool,
}

/// The open room view's presence on its room's rendezvous.
pub(crate) struct ChatLink {
    pub(crate) room: String,
    /// Counts the links opened: an answer for an earlier one is ignored.
    session: u32,
    /// The room ticks since the link opened.
    ticks: u32,
    /// The tick of the last announcement the service took, and the ticket it named.
    announced_at: Option<u32>,
    announced_ticket: String,
    /// A request is on its way.
    busy: bool,
    /// The members' devices the rendezvous lists for the room (call participants too), each
    /// announcement signed by a member.
    peers: Vec<PeerRecord>,
    dialed: BTreeMap<String, Dialed>,
}

/// The view of `room` opened: its link starts (unless this device is in its call, which carries
/// the chat already, or the link is open).
pub(crate) fn open(s: &mut MeetState, room: &str) {
    if call_room_id(s).as_deref() == Some(room)
        || s.chat_link.as_ref().is_some_and(|l| l.room == room)
    {
        return;
    }
    close(s);
    s.chat_links_opened = s.chat_links_opened.wrapping_add(1);
    s.chat_link = Some(ChatLink {
        room: room.to_string(),
        session: s.chat_links_opened,
        ticks: 0,
        announced_at: None,
        announced_ticket: String::new(),
        busy: false,
        peers: Vec::new(),
        dialed: BTreeMap::new(),
    });
}

/// The view closed (Back, Leave room, its call entered): the link's connections end, and its
/// announcement leaves the rendezvous with the next tick.
pub(crate) fn close(s: &mut MeetState) {
    let Some(link) = s.chat_link.take() else {
        return;
    };
    // Signed now: "Leave room" ends the membership before the next tick.
    let signed_at = s
        .chats
        .get(&link.room)
        .filter(|c| c.state == chatroom::Membership::Member)
        .map(|c| c.server_now(wall_ms()));
    if let (Some(ts), true) = (signed_at, link.announced_at.is_some()) {
        s.chat_leaving.push((link.room.clone(), ts));
    }
    if call_room_id(s).as_deref() != Some(link.room.as_str()) {
        forget_room(s, &link.room);
    }
    for (handle, _) in s.chat_pending.drain(..) {
        if let Some(endpoint) = s.endpoint.as_ref() {
            endpoint.disconnect(handle);
        }
    }
}

/// Ends the chat connections of `room` and forgets who may connect for it (the link closed, or
/// the call of the room was left).
pub(crate) fn forget_room(s: &mut MeetState, room: &str) {
    let (gone, kept): (Vec<ChatConn>, Vec<ChatConn>) =
        std::mem::take(&mut s.chat_conns).into_iter().partition(|c| c.room == room);
    s.chat_conns = kept;
    if let Some(endpoint) = s.endpoint.as_ref() {
        for c in &gone {
            endpoint.disconnect(c.handle);
        }
    }
    s.chat_known.retain(|_, r| r != room);
}

/// The link's requests this tick: the leaves of links closed, then the open one's announcement
/// (again after a sixth of the service's ticket lifetime, or a new ticket) or its read of who is
/// there.
pub(crate) fn next_jobs(s: &mut MeetState) -> Vec<HttpJob> {
    let mut jobs = Vec::new();
    let Some(session) = s.room.as_ref() else {
        return jobs;
    };
    let (worker, node, ticket, every) = (
        session.worker.clone(),
        session.node_id.clone(),
        session.ticket.clone(),
        session.reannounce_polls.max(1),
    );
    if worker.is_empty() || node.is_empty() {
        return jobs;
    }
    let now = wall_ms();
    let signed_at = |s: &MeetState, room: &str| {
        s.chats
            .get(room)
            .filter(|c| c.state == chatroom::Membership::Member)
            .map(|c| c.server_now(now))
    };
    for (room, ts) in std::mem::take(&mut s.chat_leaving) {
        if let Some(me) = s.identity.as_ref() {
            jobs.push(leave_job(&worker, &room, &node, me, ts));
        }
    }
    if s
        .chat_link
        .as_ref()
        .is_some_and(|l| call_room_id(s).as_deref() == Some(l.room.as_str()))
    {
        // The call of the room carries its chat now.
        close(s);
        return jobs;
    }
    let ts = s.chat_link.as_ref().and_then(|l| signed_at(s, &l.room));
    let (Some(me), Some(link)) = (s.identity.as_ref(), s.chat_link.as_mut()) else {
        return jobs;
    };
    link.ticks = link.ticks.wrapping_add(1);
    if link.busy || ticket.is_empty() {
        return jobs;
    }
    let due = link
        .announced_at
        .map_or(true, |at| link.ticks.wrapping_sub(at) >= every)
        || link.announced_ticket != ticket;
    if due {
        // Only a member's device signs an announcement the others accept.
        if let Some(ts) = ts {
            jobs.push(announce_job(&worker, &link.room, &node, &ticket, me, ts, link.session));
            link.busy = true;
        }
    } else {
        jobs.push(poll_job(&worker, &link.room, &node, link.session));
        link.busy = true;
    }
    jobs
}

/// This endpoint's chat-only ticket in `room`, signed by this device as a call's is (CRYPTO.md
/// section 10).
pub(crate) fn announce_job(
    worker: &str,
    room: &str,
    node: &str,
    ticket: &str,
    me: &crypto::Identity,
    ts: u64,
    session: u32,
) -> HttpJob {
    let sig = me.sign(&crypto::peer_input(room, node, ticket));
    let body = serde_json::json!({
        "node_id": node,
        "ticket": ticket,
        "name": "",
        "device": me.device(),
        "sig": sig,
        "chat_only": true,
    })
    .to_string()
    .into_bytes();
    let path = format!("/rooms/{room}/peers");
    let headers = Vec::from(me.request_headers("POST", &path, ts, &body));
    HttpJob {
        verb: Verb::Post,
        url: format!("{worker}{path}"),
        body,
        headers,
        on_result: on_chat_announced,
        session,
        chat: None,
    }
}

/// Who is on `room`'s rendezvous besides this endpoint.
pub(crate) fn poll_job(worker: &str, room: &str, node: &str, session: u32) -> HttpJob {
    HttpJob::get(
        format!("{worker}/rooms/{room}/peers?except={node}"),
        on_chat_peers,
        session,
    )
}

/// Takes this endpoint's chat-only ticket off `room`'s rendezvous.
pub(crate) fn leave_job(worker: &str, room: &str, node: &str, me: &crypto::Identity, ts: u64) -> HttpJob {
    let path = format!("/rooms/{room}/peers/{node}");
    HttpJob {
        verb: Verb::Delete,
        url: format!("{worker}{path}"),
        body: Vec::new(),
        headers: Vec::from(me.request_headers("DELETE", &path, ts, b"")),
        on_result: on_chat_left,
        session: 0,
        chat: None,
    }
}

/// The open link of request `session`, if the link the request was sent for is still open.
fn link_of(s: &mut MeetState, session: u32) -> Option<&mut ChatLink> {
    s.chat_link.as_mut().filter(|l| l.session == session)
}

pub(crate) extern "C" fn on_chat_announced(data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut data, session)) = reply_parts(data) else {
        return Update::DoNothing;
    };
    let answer = http_answer(result);
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let ticket = guard.room.as_ref().map(|r| r.ticket.clone()).unwrap_or_default();
    let Some(link) = link_of(&mut guard, session) else {
        return Update::DoNothing;
    };
    link.busy = false;
    if matches!(answer, Ok((200 | 201, _))) {
        link.announced_at = Some(link.ticks);
        link.announced_ticket = ticket;
    }
    Update::DoNothing
}

/// Who is on the room's rendezvous: the members' devices whose announcement a member signed may
/// connect for the room's chat, the ones this endpoint dials are dialed, and a connection that
/// came before its announcement was read is taken now.
pub(crate) extern "C" fn on_chat_peers(data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut data, session)) = reply_parts(data) else {
        return Update::DoNothing;
    };
    let answer = http_answer(result);
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let node = s.room.as_ref().map(|r| r.node_id.clone()).unwrap_or_default();
    let Some(room) = link_of(s, session).map(|l| {
        l.busy = false;
        l.room.clone()
    }) else {
        return Update::DoNothing;
    };
    let Ok((200, Some(json))) = answer else {
        return Update::DoNothing;
    };
    let listed = verified_peers(s.chats.get(&room), peers_from(&json), &node);
    println!("AZMEET_CHAT_LINK {room} {}", listed.len());
    know(&mut s.chat_known, &room, &listed);
    take_pending(&mut s.chat_pending, &mut s.chat_conns, &s.chat_known);
    let connected: BTreeSet<String> = s
        .chat_conns
        .iter()
        .map(|c| c.node_id.clone())
        .chain(s.remotes.iter().map(|r| r.node_id.clone()))
        .collect();
    let Some(link) = s.chat_link.as_mut() else {
        return Update::DoNothing;
    };
    let plan = rooms::plan_dials(&node, &listed, &connected, &link.dialed, link.ticks);
    if let Some(endpoint) = s.endpoint.as_ref() {
        for p in &plan {
            eprintln!("[azmeet] {}: dialing {} for the chat ({})", s.name, p.name, short_id(&p.node_id));
            let _dialing = endpoint.connect(p.ticket.as_str());
            link.dialed.insert(
                p.node_id.clone(),
                Dialed {
                    ticket: p.ticket.clone(),
                    at_poll: link.ticks,
                },
            );
        }
    }
    link.peers = listed;
    Update::DoNothing
}

pub(crate) extern "C" fn on_chat_left(_data: RefAny, _info: CallbackInfo, _result: RefAny) -> Update {
    Update::DoNothing
}

/// The call's read of its room's rendezvous listed `chat_only` viewers: they may connect for the
/// chat, and a connection of one's that waited for its announcement is a chat connection now.
pub(crate) fn on_call_peers(s: &mut MeetState, room: &str, chat_only: &[PeerRecord]) {
    know(&mut s.chat_known, room, chat_only);
    let waiting: Vec<(u64, String)> = s
        .pending
        .iter()
        .filter(|p| s.chat_known.contains_key(&p.node_id))
        .map(|p| (p.handle, p.node_id.clone()))
        .collect();
    s.pending.retain(|p| !s.chat_known.contains_key(&p.node_id));
    let mut waiting = waiting;
    take_pending(&mut waiting, &mut s.chat_conns, &s.chat_known);
}

/// The devices of `peers` may connect for `room`'s chat.
fn know(known: &mut BTreeMap<String, String>, room: &str, peers: &[PeerRecord]) {
    for p in peers {
        known.insert(p.node_id.clone(), room.to_string());
    }
}

/// The connections of `pending` whose device may connect for a room's chat become that room's
/// chat connections; the others keep waiting.
fn take_pending(
    pending: &mut Vec<(u64, String)>,
    conns: &mut Vec<ChatConn>,
    known: &BTreeMap<String, String>,
) {
    pending.retain(|(handle, node_id)| match known.get(node_id) {
        Some(room) => {
            if !conns.iter().any(|c| c.handle == *handle) {
                conns.push(ChatConn {
                    handle: *handle,
                    node_id: node_id.clone(),
                    room: room.clone(),
                    have_sent: false,
                });
            }
            false
        }
        None => true,
    });
}

/// A connection came up: a device that may connect for a room's chat is a chat connection of
/// that room; while a link is open and no call runs, one not read on the rendezvous yet waits for
/// the next read. `None`: not the chat's (the call's, or refused there).
pub(crate) fn on_connected(s: &mut MeetState, handle: u64, node_id: &str) -> Option<bool> {
    if let Some(room) = s.chat_known.get(node_id).cloned() {
        if !s.chat_conns.iter().any(|c| c.handle == handle) {
            eprintln!("[azmeet] {}: {} connected for the chat of {room}", s.name, short_id(node_id));
            s.chat_conns.push(ChatConn {
                handle,
                node_id: node_id.to_string(),
                room,
                have_sent: false,
            });
        }
        return Some(false);
    }
    if s.chat_link.is_some() && !accepts_peers(s) {
        if !s.chat_pending.iter().any(|(h, _)| *h == handle) {
            s.chat_pending.push((handle, node_id.to_string()));
        }
        return Some(false);
    }
    None
}

/// A connection went: true when it was the chat's.
pub(crate) fn on_disconnected(s: &mut MeetState, handle: u64) -> bool {
    let before = s.chat_conns.len() + s.chat_pending.len();
    s.chat_conns.retain(|c| c.handle != handle);
    s.chat_pending.retain(|(h, _)| *h != handle);
    s.chat_conns.len() + s.chat_pending.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A room view's announcement is chat-only and signed by this device as a call's is: the
    /// call's participants connect to it for the chat and never count it as one of the call.
    #[test]
    fn a_room_views_announcement_is_chat_only_and_signed_by_this_device() {
        let me = crypto::Identity::from_seed([7u8; 32]);
        let job = announce_job("http://meet.test", "room-a", "node-a", "ticket-a", &me, 1000, 3);
        let body: serde_json::Value = serde_json::from_slice(&job.body).unwrap();
        assert_eq!(body["chat_only"], serde_json::json!(true));
        assert_eq!(body["device"], serde_json::json!(me.device()));
        let sig = body["sig"].as_str().unwrap();
        assert!(crypto::verify(me.device(), sig, &crypto::peer_input("room-a", "node-a", "ticket-a")));
        assert_eq!(job.url, "http://meet.test/rooms/room-a/peers");
        assert_eq!(job.session, 3);
        assert_eq!(job.verb, Verb::Post);
    }

    /// A connection that came before its device's announcement was read waits; once the device
    /// is known for a room it is that room's chat connection, and only once.
    #[test]
    fn a_connection_waiting_for_its_announcement_joins_the_rooms_chat_once_it_is_read() {
        let mut pending = vec![(4, String::from("node-b")), (5, String::from("node-x"))];
        let mut conns = Vec::new();
        let mut known = BTreeMap::new();
        know(
            &mut known,
            "room-a",
            &[PeerRecord {
                node_id: String::from("node-b"),
                ticket: String::from("t"),
                name: String::from("Ben"),
                device: None,
                sig: None,
                chat_only: true,
            }],
        );
        take_pending(&mut pending, &mut conns, &known);
        assert_eq!(pending, vec![(5, String::from("node-x"))], "a stranger keeps waiting");
        assert_eq!(conns.iter().map(|c| (c.handle, c.room.as_str())).collect::<Vec<_>>(), vec![(4, "room-a")]);
        let mut again = vec![(4, String::from("node-b"))];
        take_pending(&mut again, &mut conns, &known);
        assert_eq!(conns.len(), 1, "not twice");
    }
}
