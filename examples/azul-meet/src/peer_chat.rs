//! Chat over the peers (ARCHITECTURE.md, "Chat over the peers"): each message this side writes
//! goes to every peer connected for its room as a record (`chatroom::ChatRecord`, kind 8) - the
//! call's participants when it is the call's room, and the room's chat connections (a room view
//! between calls, a viewer of the call's room: `chat_link`) - and a peer that takes a record new
//! to it passes it on to every other peer of the room, so a member reached only through a
//! backbone peer gets it too. With nobody online for a room its messages wait. While the
//! meetings service still carries and numbers each message as well, a record from a peer is
//! listed at once and numbered when the service's copy arrives.
//!
//! Two peers that meet compare what they hold: each tells the other the newest records of the
//! room it has (`ChatHave`) and gets the ones it lacks back, so a device catches up from whoever
//! is online.
//!
//! For scripts: `AZMEET_PEER_CHAT <name>: <text>` for a message that arrived from a peer before
//! the service brought it.

use super::*;
use crate::chatroom::{ChatHave, ChatRecord, Changes};

/// The connections a record of `room` goes to: the call's participants (`remotes`) when `room`
/// is the call's, and the chat connections of `room` (`conns`: handle, room), but `except`.
pub(crate) fn targets_of(
    call_room: Option<&str>,
    remotes: &[u64],
    conns: &[(u64, &str)],
    room: &str,
    except: Option<u64>,
) -> Vec<u64> {
    let call: &[u64] = if call_room == Some(room) { remotes } else { &[] };
    call.iter()
        .copied()
        .chain(conns.iter().filter(|(_, r)| *r == room).map(|(h, _)| *h))
        .filter(|h| Some(*h) != except)
        .collect()
}

/// The room whose chat connection `conn` carries: the call's for one of its participants, the
/// room it was made for for a chat connection, `None` for any other.
pub(crate) fn room_of_conn<'a>(
    call_room: Option<&'a str>,
    remotes: &[u64],
    conns: &[(u64, &'a str)],
    conn: u64,
) -> Option<&'a str> {
    if remotes.contains(&conn) {
        return call_room;
    }
    conns.iter().find(|(h, _)| *h == conn).map(|(_, r)| *r)
}

/// The handles of `s`'s call participants and chat connections, for [`targets_of`].
fn handles(s: &MeetState) -> (Option<String>, Vec<u64>, Vec<(u64, String)>) {
    (
        call_room_id(s),
        s.remotes.iter().map(|r| r.handle).collect(),
        s.chat_conns.iter().map(|c| (c.handle, c.room.clone())).collect(),
    )
}

fn as_refs(conns: &[(u64, String)]) -> Vec<(u64, &str)> {
    conns.iter().map(|(h, r)| (*h, r.as_str())).collect()
}

/// For every room with a peer online: which of its records this side holds, to a peer new to
/// it; and this side's messages of the room, sealed for the peers, to every one. A room with
/// nobody online keeps its messages until someone is.
pub(crate) fn flush(s: &mut MeetState, endpoint: &IrohEndpoint) {
    let (call_room, remotes, conns) = handles(s);
    let conn_refs = as_refs(&conns);
    let mut rooms: Vec<&str> = call_room.iter().map(String::as_str).collect();
    for (_, room) in &conn_refs {
        if !rooms.contains(room) {
            rooms.push(room);
        }
    }
    for room in rooms {
        let targets = targets_of(call_room.as_deref(), &remotes, &conn_refs, room, None);
        if targets.is_empty() {
            continue;
        }
        if let Some(have) = s.chats.get(room).map(|chat| chat.have()) {
            let bytes = have.encode();
            if call_room.as_deref() == Some(room) {
                for remote in s.remotes.iter_mut().filter(|r| !r.chat_have_sent) {
                    endpoint.send_message(remote.handle, bytes.clone());
                    remote.chat_have_sent = true;
                }
            }
            for c in s.chat_conns.iter_mut().filter(|c| c.room == room && !c.have_sent) {
                endpoint.send_message(c.handle, bytes.clone());
                c.have_sent = true;
            }
        }
        let records = {
            let (Some(me), Some(chat)) = (s.identity.as_ref(), s.chats.get_mut(room)) else {
                continue;
            };
            chat.peer_records(me)
        };
        for record in records {
            let bytes = record.encode();
            for handle in &targets {
                endpoint.send_message(*handle, bytes.clone());
            }
        }
    }
}

/// A record from the peer behind connection `conn` (a participant of the call or a chat
/// connection, never a connection still waiting for its announcement), for the room it carries:
/// taken into the room's chat, passed on to every other peer of the room when it was new here.
/// What it changed waits for the pump ([`take_changes`]). True when the window changes.
pub(crate) fn on_record(s: &mut MeetState, endpoint: &IrohEndpoint, conn: u64, record: ChatRecord) -> bool {
    let (call_room, remotes, conns) = handles(s);
    let conn_refs = as_refs(&conns);
    if room_of_conn(call_room.as_deref(), &remotes, &conn_refs, conn) != Some(record.room.as_str()) {
        return false;
    }
    let room = record.room.clone();
    let (changes, new) = {
        let (Some(me), Some(chat)) = (s.identity.as_ref(), s.chats.get_mut(&room)) else {
            return false;
        };
        chat.take_record(me, record.clone())
    };
    for m in &changes.arrived {
        println!("AZMEET_PEER_CHAT {}: {}", member_name(&m.name), m.text);
    }
    if new {
        let bytes = record.encode();
        for handle in targets_of(call_room.as_deref(), &remotes, &conn_refs, &room, Some(conn)) {
            endpoint.send_message(handle, bytes.clone());
        }
    }
    let shown = changes.shown;
    if changes != Changes::default() {
        s.peer_chat.push((room, changes));
    }
    shown
}

/// A peer said which records of the room it carries it holds: it gets the ones this side holds
/// and it lacks. Nothing in the window changes.
pub(crate) fn on_have(s: &MeetState, endpoint: &IrohEndpoint, conn: u64, have: &ChatHave) -> bool {
    let (call_room, remotes, conns) = handles(s);
    if room_of_conn(call_room.as_deref(), &remotes, &as_refs(&conns), conn) != Some(have.room.as_str()) {
        return false;
    }
    let lacking = s
        .chats
        .get(&have.room)
        .map(|chat| chat.missing_for(have))
        .unwrap_or_default();
    for record in lacking {
        endpoint.send_message(conn, record.encode());
    }
    false
}

/// What the peers' records changed since the last pump, by room, for `apply_chat_changes`.
pub(crate) fn take_changes(s: &mut MeetState) -> Vec<(String, Changes)> {
    std::mem::take(&mut s.peer_chat)
}

#[cfg(test)]
mod tests {
    use super::{room_of_conn, targets_of};

    /// A record goes to every connection of its room - the call's participants when it is the
    /// call's room, and the room's chat connections - but the one it came from.
    #[test]
    fn a_record_goes_to_every_connection_of_its_room_but_the_one_it_came_from() {
        let conns = [(7, "room-a"), (8, "room-b"), (9, "room-a")];
        assert_eq!(targets_of(Some("room-a"), &[1, 2], &conns, "room-a", Some(2)), vec![1, 7, 9]);
        assert_eq!(targets_of(Some("room-a"), &[1, 2], &conns, "room-b", None), vec![8]);
        // Nobody online for a room: nothing to send to, its messages wait.
        assert!(targets_of(None, &[], &conns, "room-c", None).is_empty());
    }

    /// A connection carries the chat of the room it was made for, nothing else: a record or a
    /// list from it for another room is not taken.
    #[test]
    fn a_connection_carries_only_the_chat_of_the_room_it_was_made_for() {
        let conns = [(7, "room-a"), (8, "room-b")];
        assert_eq!(room_of_conn(Some("room-a"), &[1], &conns, 1), Some("room-a"));
        assert_eq!(room_of_conn(Some("room-a"), &[1], &conns, 8), Some("room-b"));
        assert_eq!(room_of_conn(None, &[1], &conns, 1), None, "a call's peer outside a call");
        assert_eq!(room_of_conn(Some("room-a"), &[1], &conns, 5), None, "a stranger");
    }
}
