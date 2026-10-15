//! Chat over the peers (ARCHITECTURE.md, "Chat over the peers"): in a call, each message this
//! side writes goes to every connected peer as a record (`chatroom::ChatRecord`, kind 8), and a
//! peer that takes a record new to it passes it on to every other peer it is connected to, so a
//! member reached only through a backbone peer gets it too. The meetings service still carries
//! and numbers each message as well; a record from a peer is listed at once and numbered when
//! the service's copy arrives.
//!
//! For scripts: `AZMEET_PEER_CHAT <name>: <text>` for a message that arrived from a peer before
//! the service brought it.

use super::*;
use crate::chatroom::{ChatRecord, Changes};

/// This side's messages of the call's room, sealed for the peers, to every connected peer.
pub(crate) fn flush(s: &mut MeetState, endpoint: &IrohEndpoint) {
    let Some(id) = call_room_id(s) else {
        return;
    };
    let records = {
        let (Some(me), Some(chat)) = (s.identity.as_ref(), s.chats.get_mut(&id)) else {
            return;
        };
        chat.peer_records(me)
    };
    for record in records {
        let bytes = record.encode();
        for remote in &s.remotes {
            endpoint.send_message(remote.handle, bytes.clone());
        }
    }
}

/// A record from the peer behind connection `conn` (a peer of the call, never a connection
/// still waiting for its announcement): taken into its room's chat, passed on to every other
/// peer when it was new here. What it changed waits for the pump ([`take_changes`]). True when
/// the window changes.
pub(crate) fn on_record(s: &mut MeetState, endpoint: &IrohEndpoint, conn: u64, record: ChatRecord) -> bool {
    if !s.remotes.iter().any(|r| r.handle == conn) {
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
        for remote in s.remotes.iter().filter(|r| r.handle != conn) {
            endpoint.send_message(remote.handle, bytes.clone());
        }
    }
    let shown = changes.shown;
    if changes != Changes::default() {
        s.peer_chat.push((room, changes));
    }
    shown
}

/// What the peers' records changed since the last pump, by room, for `apply_chat_changes`.
pub(crate) fn take_changes(s: &mut MeetState) -> Vec<(String, Changes)> {
    std::mem::take(&mut s.peer_chat)
}
