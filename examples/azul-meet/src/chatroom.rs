//! A room's end-to-end encrypted chat as this device sees it (`CRYPTO.md`, sections 5 to 8): who
//! is in it (each member's signed record checked: the link's proof, or the admission of a member
//! who has one), the room keys this device holds (its sealed copies opened, their senders and
//! records checked), the messages (signed, sealed, opened in the order the meet Worker numbered
//! them), and what this device sends: its record, a new room key whenever the members changed (the
//! rotation rule), its messages, an admission, its leave.
//!
//! It never touches the network: [`ChatRoom::next_call`] says the next request (one at a time per
//! room) and [`ChatRoom::on_answer`] takes the Worker's answer; the app signs and sends them on an
//! azul Thread. Pure: no azul types; the tests run three devices through an in-memory meet Worker
//! that keeps the rules of the real one (azul-apps `cf-workers/meet/src/handler.js`) and can play a
//! dishonest one.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::Deserialize;
use serde_json::json;

use crate::crypto::{self, Identity, Invite, Plain, RoomKey};

/// How long a request may go unanswered before it is given up (ms).
pub const STUCK_MS: u64 = 20_000;
/// How many messages one sync asks for.
pub const SYNC_LIMIT: u32 = 100;
/// The members a new key is sealed to must have been read this recently (ms).
const FRESH_MEMBERS_MS: u64 = 10_000;
/// The longest name, in characters (the Worker's rule for a knock's name).
pub const MAX_NAME_CHARS: usize = 64;
/// The messages kept listed, the newest.
pub const MAX_LISTED: usize = 2000;
/// Messages and keys that wait (for their key, for their sender's record), at most.
const MAX_WAITING: usize = 500;
/// Up-to-date reads a message waits for its key before it counts as written before this device
/// could read it.
const WAIT_READS: u32 = 3;

/// What a room is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomKind {
    /// A meeting: a call (maybe with times), and its chat.
    Meeting,
    /// A chat room: kept between calls (the Worker keeps it longer).
    Chat,
}

impl RoomKind {
    /// The kind the Worker names: `chat`, else a meeting.
    #[must_use]
    pub fn parse(text: Option<&str>) -> RoomKind {
        match text {
            Some("chat") => RoomKind::Chat,
            _ => RoomKind::Meeting,
        }
    }

    /// The Worker's name of the kind.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            RoomKind::Meeting => "meeting",
            RoomKind::Chat => "chat",
        }
    }
}

/// Where this device stands in the room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Membership {
    /// Not in it: only reading who is (a waiting room before "Join now").
    Outside,
    /// Its record goes out next: as a member with the link, or as a knock without it.
    Joining,
    /// Knocking: waiting for a member to let this device in.
    Knocking,
    /// A member.
    Member,
    /// Its leave goes out next.
    Leaving,
    /// Left: nothing more is sent.
    Left,
    /// The room is gone, or refused (it does not match the link, it is not encrypted).
    Closed,
}

/// A member (or a knock) as its signed record says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub device: String,
    pub dh: String,
    /// Its name (opened with the link's key; a knock's plain name); empty when it does not open.
    pub name: String,
    pub safety_code: String,
    /// When it made its record (ms, as it signed it).
    pub ts: u64,
    /// The member who let it in, when it knocked.
    pub admitted_by: Option<String>,
}

/// One message of the room, opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomMessage {
    pub id: String,
    /// The Worker's number; `None` while this device's own message is on its way.
    pub seq: Option<u64>,
    pub sender: String,
    /// The sender's name when it wrote.
    pub name: String,
    pub text: String,
    /// When the sender wrote it (ms, its clock).
    pub ts: u64,
    pub mine: bool,
}

/// A room key this device holds.
#[derive(Debug, Clone)]
struct HeldKey {
    key: RoomKey,
    epoch: u64,
    sender: String,
    members: BTreeSet<String>,
}

/// A message written here, until the Worker took it.
#[derive(Debug, Clone)]
struct Outgoing {
    id: String,
    text: String,
    ts: u64,
}

/// One request to the meet Worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub method: &'static str,
    /// From `/rooms` on: what a signed request signs.
    pub path: String,
    /// After the `?`, or empty.
    pub query: String,
    pub body: Vec<u8>,
    /// Sent with the signed-request headers (`Identity::request_headers`).
    pub signed: bool,
    pub what: CallKind,
}

impl Call {
    fn signed(
        method: &'static str,
        path: String,
        body: &serde_json::Value,
        what: CallKind,
    ) -> Call {
        Call {
            method,
            path,
            query: String::new(),
            body: body.to_string().into_bytes(),
            signed: true,
            what,
        }
    }

    /// The request's address on the meeting server `server`.
    #[must_use]
    pub fn url(&self, server: &str) -> String {
        if self.query.is_empty() {
            format!("{server}{}", self.path)
        } else {
            format!("{server}{}?{}", self.path, self.query)
        }
    }
}

/// What a request is for; its answer goes back with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallKind {
    Join,
    Sync,
    /// A new room key, by its id.
    PostKey(String),
    /// A message, by its id.
    PostMessage(String),
    /// Letting this device in.
    Admit(String),
    Leave,
}

/// A room key made here or taken in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub epoch: u64,
    pub key_id: String,
    /// How many devices hold it.
    pub members: usize,
    pub made_here: bool,
    pub sender: String,
}

/// What an answer changed: for the window, stdout and the files.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Changes {
    /// Something the window shows changed.
    pub shown: bool,
    /// Messages of others that arrived, oldest first.
    pub arrived: Vec<RoomMessage>,
    pub joined: Vec<Member>,
    pub left: Vec<Member>,
    pub knocking: Vec<Member>,
    pub keys: Vec<KeyEvent>,
    /// The first read of the room is complete: how many messages it lists.
    pub history: Option<usize>,
    /// This device was let in (it knocked): it holds the link now.
    pub admitted: bool,
    /// Why the room stopped, or why a request failed (a notice).
    pub problem: Option<String>,
}

impl Changes {
    /// Adds `other` (a later answer's changes) to these (the tests' driver sums a run's).
    #[cfg(test)]
    pub fn absorb(&mut self, other: Changes) {
        self.shown |= other.shown;
        self.arrived.extend(other.arrived);
        self.joined.extend(other.joined);
        self.left.extend(other.left);
        self.knocking.extend(other.knocking);
        self.keys.extend(other.keys);
        self.history = other.history.or(self.history);
        self.admitted |= other.admitted;
        if other.problem.is_some() {
            self.problem = other.problem;
        }
    }
}

// ==== What the Worker answers (`cf-workers/meet/src/handler.js`) ====

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct WireMember {
    pub device: String,
    pub dh: String,
    #[serde(default)]
    pub sealed_name: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    pub ts: u64,
    pub sig: String,
    #[serde(default)]
    pub proof: Option<String>,
    #[serde(default)]
    pub admitted_by: Option<String>,
    #[serde(default)]
    pub admission: Option<String>,
    #[serde(default)]
    pub state: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct WireKey {
    pub seq: u64,
    pub key_id: String,
    pub epoch: u64,
    pub sender: String,
    pub members: Vec<String>,
    pub sig: String,
    pub sealed: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct WireMessage {
    pub seq: u64,
    pub id: String,
    pub sender: String,
    pub key_id: String,
    pub body: String,
    pub sig: String,
}

/// A room as `POST /rooms`, `GET /rooms/<key>` and `/sync` answer it.
#[derive(Debug, Clone, Deserialize)]
pub struct WireRoom {
    pub room: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub invite_key: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub starts_at: Option<String>,
    #[serde(default)]
    pub ends_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SyncAnswer {
    room: WireRoom,
    now: u64,
    members_rev: u64,
    #[serde(default)]
    members: Option<Vec<WireMember>>,
    #[serde(default)]
    left: Option<Vec<WireMember>>,
    #[serde(default)]
    keys: Vec<WireKey>,
    #[serde(default)]
    keys_next: u64,
    #[serde(default)]
    epoch: u64,
    #[serde(default)]
    messages: Vec<WireMessage>,
    #[serde(default)]
    next: u64,
    #[serde(default)]
    more: bool,
}

#[derive(Debug, Default, Deserialize)]
struct WireError {
    #[serde(default)]
    error: String,
    #[serde(default)]
    now: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct JoinAnswer {
    #[serde(default)]
    state: String,
}

#[derive(Debug, Deserialize)]
struct Posted {
    seq: u64,
}

/// A name as the Worker keeps a knock's: control, zero-width and bidi-override characters out,
/// whitespace runs one space, at most [`MAX_NAME_CHARS`] characters, "Guest" when nothing is left
/// (the Worker's `cleanName`, so its `validName` takes it unchanged).
#[must_use]
pub fn clean_name(raw: &str) -> String {
    let forbidden = |c: char| {
        matches!(c, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{2069}' | '\u{feff}')
    };
    let replaced: String = raw
        .chars()
        .map(|c| if forbidden(c) { ' ' } else { c })
        .collect();
    let collapsed = replaced.split_whitespace().collect::<Vec<&str>>().join(" ");
    let clipped: String = collapsed.chars().take(MAX_NAME_CHARS).collect();
    let clipped = clipped.trim();
    if clipped.is_empty() {
        String::from("Guest")
    } else {
        clipped.to_string()
    }
}

/// One room's chat on this device.
#[derive(Debug)]
pub struct ChatRoom {
    pub room: String,
    pub code: String,
    pub kind: RoomKind,
    /// The meet Worker this room lives on.
    pub server: String,
    /// The meeting's times (seconds since 1970), when it has them.
    pub starts_at: Option<u64>,
    pub ends_at: Option<u64>,
    pub state: Membership,
    /// The newest message read here (the unread count is what came after it).
    pub read_seq: u64,
    invite: Option<Invite>,
    invite_key: Option<String>,
    name: String,
    members: BTreeMap<String, Member>,
    knocks: BTreeMap<String, Member>,
    /// Every device accepted as a member here at some time, current or departed: what a departed
    /// member signed still verifies.
    known: BTreeMap<String, Member>,
    /// Devices accepted with the link's proof at some time (who may have admitted someone).
    linked: BTreeSet<String>,
    /// Devices seen leaving, with the time of the last record of theirs accepted here: an older
    /// record of theirs is refused (CRYPTO.md section 7).
    departed: BTreeMap<String, u64>,
    members_rev: Option<u64>,
    members_read_at: Option<u64>,
    keys: BTreeMap<String, HeldKey>,
    waiting_keys: Vec<WireKey>,
    max_epoch: u64,
    key_cursor: u64,
    cursor: u64,
    more: bool,
    messages: Vec<RoomMessage>,
    /// Messages waiting for their key, with how many up-to-date reads they waited.
    waiting: Vec<(WireMessage, u32)>,
    seen: BTreeSet<String>,
    /// Messages and keys that failed a check (a signature, a sender, a seal).
    unreadable: u32,
    /// Messages written before this device could read them (no copy of their key for it).
    before_join: u32,
    outbox: VecDeque<Outgoing>,
    admit_queue: VecDeque<String>,
    /// Make a new key at the next chance (after letting someone in).
    rotate: bool,
    /// Register again: with the link's proof (a knock that got the link), or a new name.
    rejoin: bool,
    pending_key: Option<HeldKey>,
    in_flight: Option<(CallKind, u64)>,
    last_sync: Option<u64>,
    synced_once: bool,
    retry_at: u64,
    failures: u32,
    clock_offset: i64,
    last_record_ts: u64,
}

impl ChatRoom {
    /// Room `room` (its `code`) on the meet Worker `server`, with the link's `invite` (`None`:
    /// joined with the code, a knock), this device showing `name`. Nothing is sent until the
    /// first [`ChatRoom::next_call`].
    #[must_use]
    pub fn new(
        server: &str,
        room: &str,
        code: &str,
        kind: RoomKind,
        invite: Option<Invite>,
        name: &str,
    ) -> ChatRoom {
        ChatRoom {
            room: room.to_string(),
            code: code.to_string(),
            kind,
            server: server.to_string(),
            starts_at: None,
            ends_at: None,
            state: Membership::Outside,
            read_seq: 0,
            invite,
            invite_key: None,
            name: clean_name(name),
            members: BTreeMap::new(),
            knocks: BTreeMap::new(),
            known: BTreeMap::new(),
            linked: BTreeSet::new(),
            departed: BTreeMap::new(),
            members_rev: None,
            members_read_at: None,
            keys: BTreeMap::new(),
            waiting_keys: Vec::new(),
            max_epoch: 0,
            key_cursor: 0,
            cursor: 0,
            more: false,
            messages: Vec::new(),
            waiting: Vec::new(),
            seen: BTreeSet::new(),
            unreadable: 0,
            before_join: 0,
            outbox: VecDeque::new(),
            admit_queue: VecDeque::new(),
            rotate: false,
            rejoin: false,
            pending_key: None,
            in_flight: None,
            last_sync: None,
            synced_once: false,
            retry_at: 0,
            failures: 0,
            clock_offset: 0,
            last_record_ts: 0,
        }
    }

    // ---- what the window reads ----

    /// The room's link: with its invite secret in the fragment when this device holds it.
    #[must_use]
    pub fn link(&self, prefix: &str) -> String {
        match &self.invite {
            Some(invite) => format!("{prefix}{}#{}", self.room, invite.secret()),
            None => format!("{prefix}{}", self.room),
        }
    }

    #[must_use]
    pub fn invite(&self) -> Option<&Invite> {
        self.invite.as_ref()
    }

    /// The members, by device id.
    pub fn members(&self) -> impl Iterator<Item = &Member> {
        self.members.values()
    }

    /// The devices knocking, by device id.
    pub fn knocks(&self) -> impl Iterator<Item = &Member> {
        self.knocks.values()
    }

    #[must_use]
    pub fn member(&self, device: &str) -> Option<&Member> {
        self.members.get(device)
    }

    /// The messages, oldest first (this device's own on their way last).
    #[must_use]
    pub fn messages(&self) -> &[RoomMessage] {
        &self.messages
    }

    /// Messages of others after the last one read.
    #[must_use]
    pub fn unread(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| !m.mine && m.seq.is_some_and(|s| s > self.read_seq))
            .count()
    }

    /// Everything listed is read. True when that changed the count.
    pub fn mark_read(&mut self) -> bool {
        let top = self
            .messages
            .iter()
            .filter_map(|m| m.seq)
            .max()
            .unwrap_or(0);
        if top > self.read_seq {
            self.read_seq = top;
            return true;
        }
        false
    }

    /// Messages and keys that failed a check.
    #[must_use]
    pub fn unreadable(&self) -> u32 {
        self.unreadable
    }

    /// Messages written before this device could read them.
    #[must_use]
    pub fn before_join(&self) -> u32 {
        self.before_join
    }

    /// The key this device sends with now: (epoch, id, how many hold it).
    #[must_use]
    pub fn current_key(&self, me: &str) -> Option<(u64, &str, usize)> {
        self.usable_key(me)
            .map(|k| (k.epoch, k.key.id(), k.members.len()))
    }

    /// The departures this device saw (kept in its files: a replayed record stays refused).
    #[must_use]
    pub fn departed(&self) -> &BTreeMap<String, u64> {
        &self.departed
    }

    /// Departures seen in an earlier run.
    pub fn restore_departed(&mut self, departed: BTreeMap<String, u64>) {
        for (device, ts) in departed {
            let seen = self.departed.entry(device).or_insert(0);
            *seen = (*seen).max(ts);
        }
    }

    /// This device's estimate of the Worker's clock (ms), from its last answer.
    #[must_use]
    pub fn server_now(&self, now_ms: u64) -> u64 {
        (now_ms as i64).saturating_add(self.clock_offset).max(0) as u64
    }

    /// The member whose device signed this iroh announcement, when one did.
    #[must_use]
    pub fn verify_peer(
        &self,
        node_id: &str,
        ticket: &str,
        device: Option<&str>,
        sig: Option<&str>,
    ) -> Option<&Member> {
        let (device, sig) = (device?, sig?);
        let member = self.members.get(device)?;
        crypto::verify(
            device,
            sig,
            &crypto::peer_input(&self.room, node_id, ticket),
        )
        .then_some(member)
    }

    /// This device's signature of its iroh announcement (`lib.rs` signs `crypto::peer_input`
    /// itself when it announces; the tests sign with this).
    #[cfg(test)]
    #[must_use]
    pub fn peer_sig(&self, me: &Identity, node_id: &str, ticket: &str) -> String {
        me.sign(&crypto::peer_input(&self.room, node_id, ticket))
    }

    // ---- what the user does ----

    /// The messages an earlier visit kept in this device's files (`chat.jsonl`): listed in their
    /// places (one from before the numbers first), and a copy the Worker still has is not listed
    /// a second time. How many were new here.
    pub fn restore(&mut self, earlier: Vec<RoomMessage>) -> usize {
        let mut added = 0;
        for mut message in earlier {
            if !self.seen.insert(message.id.clone()) {
                continue;
            }
            if message.seq.is_none() {
                message.seq = Some(0);
            }
            self.insert_listed(message);
            added += 1;
        }
        added
    }

    /// Join: as a member with the link, or a knock without it.
    pub fn join(&mut self) {
        if matches!(self.state, Membership::Outside | Membership::Left) {
            self.state = Membership::Joining;
            self.retry_at = 0;
        }
    }

    /// Leave the room: the next request is the leave; what waited to be sent is dropped.
    pub fn leave(&mut self) {
        if matches!(
            self.state,
            Membership::Member | Membership::Knocking | Membership::Joining
        ) {
            self.state = Membership::Leaving;
            self.outbox.clear();
            self.retry_at = 0;
            self.failures = 0;
        }
    }

    /// Let the knocking device `device` in.
    pub fn admit(&mut self, device: &str) {
        if self.knocks.contains_key(device) && !self.admit_queue.iter().any(|d| d == device) {
            self.admit_queue.push_back(device.to_string());
        }
    }

    /// This device shows `name` from now on (its record goes out again).
    pub fn set_name(&mut self, name: &str) {
        let name = clean_name(name);
        if name != self.name {
            self.name = name;
            if self.state == Membership::Member {
                self.rejoin = true;
            }
        }
    }

    /// Writes `text`: listed at once (on its way), sent when a key for the current members is held.
    /// `None` for an empty message, or outside the room.
    pub fn send(&mut self, me: &Identity, text: &str, now_ms: u64) -> Option<RoomMessage> {
        let text = crate::chat::clean_text(text, crate::chat::MAX_CHAT_BYTES)?;
        if !matches!(self.state, Membership::Member | Membership::Joining) {
            return None;
        }
        let id = crypto::new_message_id().ok()?;
        let ts = self.server_now(now_ms);
        let message = RoomMessage {
            id: id.clone(),
            seq: None,
            sender: me.device().to_string(),
            name: self.name.clone(),
            text: text.clone(),
            ts,
            mine: true,
        };
        self.seen.insert(id.clone());
        self.messages.push(message.clone());
        self.trim_listed();
        self.outbox.push_back(Outgoing { id, text, ts });
        Some(message)
    }

    // ---- the requests ----

    /// The next request to send, if any: one at a time; a sync when the last one is
    /// `sync_every_ms` old. `now_ms` is this device's clock.
    pub fn next_call(&mut self, me: &Identity, now_ms: u64, sync_every_ms: u64) -> Option<Call> {
        if let Some(at) = self.in_flight.as_ref().map(|(_, at)| *at) {
            if now_ms.saturating_sub(at) < STUCK_MS {
                return None;
            }
            // Never answered: given up.
            self.in_flight = None;
            self.pending_key = None;
        }
        if now_ms < self.retry_at {
            return None;
        }
        let call = match self.state {
            Membership::Closed | Membership::Left => None,
            Membership::Leaving => Some(self.leave_call(me)),
            Membership::Joining => self.join_call(me, now_ms),
            Membership::Outside | Membership::Knocking => {
                self.sync_if_due(me, now_ms, sync_every_ms)
            }
            Membership::Member => self.member_call(me, now_ms, sync_every_ms),
        }?;
        self.in_flight = Some((call.what.clone(), now_ms));
        Some(call)
    }

    fn path(&self, tail: &str) -> String {
        format!("/rooms/{}/{tail}", self.room)
    }

    fn member_call(&mut self, me: &Identity, now_ms: u64, every: u64) -> Option<Call> {
        if self.rejoin {
            return self.join_call(me, now_ms);
        }
        while let Some(device) = self.admit_queue.front().cloned() {
            if let Some(call) = self.admit_call(me, &device) {
                return Some(call);
            }
            self.admit_queue.pop_front();
        }
        let wants_key =
            self.rotate || (!self.outbox.is_empty() && self.usable_key(me.device()).is_none());
        if wants_key {
            let fresh = self
                .members_read_at
                .is_some_and(|at| now_ms.saturating_sub(at) < FRESH_MEMBERS_MS);
            if fresh && self.members.contains_key(me.device()) {
                if let Some(call) = self.key_call(me) {
                    return Some(call);
                }
            } else if self
                .last_sync
                .map_or(true, |at| now_ms.saturating_sub(at) >= 1000)
            {
                // A key is sealed to the members as the Worker lists them now: read them first.
                return Some(self.sync_call(me));
            }
        }
        if !self.outbox.is_empty() {
            if let Some(call) = self.message_call(me) {
                return Some(call);
            }
        }
        self.sync_if_due(me, now_ms, every)
    }

    fn sync_if_due(&self, me: &Identity, now_ms: u64, every: u64) -> Option<Call> {
        let due = self.more
            || self
                .last_sync
                .map_or(true, |at| now_ms.saturating_sub(at) >= every);
        due.then(|| self.sync_call(me))
    }

    fn sync_call(&self, me: &Identity) -> Call {
        let mut query = format!(
            "for={}&after={}&keys_after={}&limit={SYNC_LIMIT}",
            me.device(),
            self.cursor,
            self.key_cursor
        );
        if let Some(rev) = self.members_rev {
            query.push_str(&format!("&members_rev={rev}"));
        }
        Call {
            method: "GET",
            path: self.path("sync"),
            query,
            body: Vec::new(),
            signed: false,
            what: CallKind::Sync,
        }
    }

    fn join_call(&mut self, me: &Identity, now_ms: u64) -> Option<Call> {
        let ts = self.server_now(now_ms).max(self.last_record_ts + 1);
        let (sealed, plain, proof) = match &self.invite {
            Some(invite) => (
                Some(invite.seal_name(me.device(), &self.name).ok()?),
                None,
                Some(invite.proof(me.device(), me.dh())),
            ),
            None => (None, Some(clean_name(&self.name)), None),
        };
        let sig = me.sign(&crypto::member_input(
            &self.room,
            me.device(),
            me.dh(),
            sealed.as_deref(),
            plain.as_deref(),
            ts,
        ));
        let body = json!({
            "dh": me.dh(),
            "sealed_name": sealed,
            "name": plain,
            "ts": ts,
            "sig": sig,
            "proof": proof,
        });
        self.last_record_ts = ts;
        Some(Call::signed(
            "PUT",
            self.path(&format!("members/{}", me.device())),
            &body,
            CallKind::Join,
        ))
    }

    fn admit_call(&self, me: &Identity, device: &str) -> Option<Call> {
        let knock = self.knocks.get(device)?;
        let admission = me.sign(&crypto::admit_input(&self.room, device, &knock.dh));
        Some(Call::signed(
            "POST",
            self.path(&format!("members/{device}/admit")),
            &json!({ "admission": admission }),
            CallKind::Admit(device.to_string()),
        ))
    }

    /// A new room key sealed to every member as listed now (the rotation rule), with the invite
    /// secret in each copy. Only a member that holds the link makes one.
    fn key_call(&mut self, me: &Identity) -> Option<Call> {
        let invite = self.invite.as_ref()?;
        let key = RoomKey::generate().ok()?;
        let epoch = self.max_epoch + 1;
        let members: Vec<String> = self.members.keys().cloned().collect();
        let mut envelopes = Vec::with_capacity(members.len());
        for device in &members {
            let dh = &self.members[device].dh;
            let sealed = crypto::seal_key(
                &self.room,
                &key,
                epoch,
                me.device(),
                device,
                dh,
                invite.secret(),
            )
            .ok()?;
            envelopes.push(json!({ "recipient": device, "sealed": sealed }));
        }
        let sig = me.sign(&crypto::key_input(&self.room, key.id(), epoch, &members));
        let body = json!({
            "key_id": key.id(),
            "epoch": epoch,
            "members": members,
            "sig": sig,
            "envelopes": envelopes,
        });
        let what = CallKind::PostKey(key.id().to_string());
        self.pending_key = Some(HeldKey {
            key,
            epoch,
            sender: me.device().to_string(),
            members: members.iter().cloned().collect(),
        });
        Some(Call::signed("POST", self.path("keys"), &body, what))
    }

    fn message_call(&self, me: &Identity) -> Option<Call> {
        let out = self.outbox.front()?;
        let key = self.usable_key(me.device())?;
        let plain = Plain {
            text: out.text.clone(),
            name: self.name.clone(),
            ts: out.ts,
        };
        let body = crypto::seal_message(&key.key, &self.room, &out.id, me.device(), &plain).ok()?;
        let sig = me.sign(&crypto::message_input(
            &self.room,
            &out.id,
            key.key.id(),
            &body,
        ));
        Some(Call::signed(
            "POST",
            self.path("messages"),
            &json!({ "id": out.id, "key_id": key.key.id(), "body": body, "sig": sig }),
            CallKind::PostMessage(out.id.clone()),
        ))
    }

    fn leave_call(&self, me: &Identity) -> Call {
        Call {
            method: "DELETE",
            path: self.path(&format!("members/{}", me.device())),
            query: String::new(),
            body: Vec::new(),
            signed: true,
            what: CallKind::Leave,
        }
    }

    /// The key to send with: the newest held key sealed to exactly the members as listed now; none
    /// while this device is not listed itself.
    fn usable_key(&self, me: &str) -> Option<&HeldKey> {
        if !self.members.contains_key(me) {
            return None;
        }
        let current: BTreeSet<String> = self.members.keys().cloned().collect();
        self.keys
            .values()
            .filter(|k| k.members == current)
            .max_by(|a, b| (a.epoch, a.key.id()).cmp(&(b.epoch, b.key.id())))
    }

    // ---- the answers ----

    /// The Worker's answer to the request `what` (`status` `None`: no answer at all), at this
    /// device's time `now_ms`. An answer to a request given up on is ignored.
    pub fn on_answer(
        &mut self,
        me: &Identity,
        what: &CallKind,
        status: Option<u16>,
        body: &str,
        now_ms: u64,
    ) -> Changes {
        let mut changes = Changes::default();
        if self.in_flight.as_ref().map(|(kind, _)| kind) != Some(what) {
            return changes;
        }
        self.in_flight = None;
        let error: WireError = serde_json::from_str(body).unwrap_or_default();
        if let Some(server_now) = error.now {
            self.set_clock(server_now, now_ms);
        }
        let Some(status) = status else {
            self.pending_key = None;
            self.failed(now_ms, &mut changes, "The meeting server does not answer.");
            return changes;
        };
        match what {
            CallKind::Sync => match (status, serde_json::from_str::<SyncAnswer>(body)) {
                (200, Ok(answer)) => self.apply_sync(me, answer, now_ms, &mut changes),
                (200, Err(_)) => self.failed(
                    now_ms,
                    &mut changes,
                    "The meeting server sent an answer AzMeet cannot read.",
                ),
                (404, _) => self.close(&mut changes, "This room has ended, or the link is wrong."),
                _ => self.failed(now_ms, &mut changes, &refused(status, &error.error)),
            },
            CallKind::Join => self.joined(status, body, &error, now_ms, &mut changes),
            CallKind::PostKey(id) => {
                let pending = self.pending_key.take();
                if matches!(status, 200 | 201) {
                    if let Some(held) = pending.filter(|k| k.key.id() == id) {
                        self.max_epoch = self.max_epoch.max(held.epoch);
                        changes.keys.push(KeyEvent {
                            epoch: held.epoch,
                            key_id: id.clone(),
                            members: held.members.len(),
                            made_here: true,
                            sender: held.sender.clone(),
                        });
                        self.keys.insert(id.clone(), held);
                    }
                    self.rotate = false;
                    self.failures = 0;
                    changes.shown = true;
                } else {
                    if matches!(status, 400 | 403) {
                        // Someone's record moved, or this device's: read the members again.
                        self.members_rev = None;
                        self.members_read_at = None;
                    }
                    if status == 403 {
                        self.state = Membership::Joining;
                    }
                    self.failed(now_ms, &mut changes, &refused(status, &error.error));
                }
            }
            CallKind::PostMessage(id) => match status {
                200 | 201 => {
                    let seq = serde_json::from_str::<Posted>(body).ok().map(|p| p.seq);
                    if self.outbox.front().is_some_and(|o| &o.id == id) {
                        self.outbox.pop_front();
                    }
                    if let (Some(seq), Some(own)) =
                        (seq, self.messages.iter_mut().find(|m| &m.id == id))
                    {
                        own.seq = Some(seq);
                    }
                    self.reorder();
                    self.failures = 0;
                    changes.shown = true;
                }
                413 => {
                    self.outbox.retain(|o| &o.id != id);
                    self.messages.retain(|m| &m.id != id);
                    changes.problem = Some(String::from(
                        "That message is too long for the meeting server.",
                    ));
                    changes.shown = true;
                }
                403 => {
                    self.state = Membership::Joining;
                    self.failed(now_ms, &mut changes, &refused(status, &error.error));
                }
                409 => {
                    // The key is not there: make another.
                    self.rotate = true;
                    self.failed(now_ms, &mut changes, &refused(status, &error.error));
                }
                _ => self.failed(now_ms, &mut changes, &refused(status, &error.error)),
            },
            CallKind::Admit(device) => {
                self.admit_queue.retain(|d| d != device);
                if status == 200 {
                    // The newcomer gets a key at once, and with it the link.
                    self.rotate = true;
                    self.members_rev = None;
                    self.members_read_at = None;
                    self.last_sync = None;
                    changes.shown = true;
                } else if status != 409 {
                    changes.problem = Some(refused(status, &error.error));
                    changes.shown = true;
                }
            }
            CallKind::Leave => {
                if matches!(status, 200 | 404) {
                    self.state = Membership::Left;
                    changes.shown = true;
                } else {
                    self.failures += 1;
                    if self.failures >= 3 {
                        // The record ends with the room.
                        self.state = Membership::Left;
                        changes.shown = true;
                    } else {
                        self.failed(now_ms, &mut changes, &refused(status, &error.error));
                    }
                }
            }
        }
        changes
    }

    fn joined(
        &mut self,
        status: u16,
        body: &str,
        error: &WireError,
        now_ms: u64,
        changes: &mut Changes,
    ) {
        match (status, error.error.as_str()) {
            (200, _) => {
                let state = serde_json::from_str::<JoinAnswer>(body)
                    .map(|a| a.state)
                    .unwrap_or_default();
                self.failures = 0;
                self.rejoin = false;
                self.state = if state == "member" {
                    Membership::Member
                } else {
                    Membership::Knocking
                };
                // Read the members again at once: this device's record is among them now.
                self.members_rev = None;
                self.members_read_at = None;
                self.last_sync = None;
                changes.shown = true;
            }
            (404, _) => self.close(changes, "This room has ended, or the link is wrong."),
            (403, "bad_proof") => self.close(
                changes,
                "This link does not open this room: ask for the link again.",
            ),
            (409, "not_encrypted") => self.close(
                changes,
                "This room was made by an older AzMeet and is not end-to-end encrypted, so this \
                 AzMeet does not join it.",
            ),
            (409, "room_full") => self.close(changes, "This room is full."),
            (401, "stale_request") | (409, "stale_record") => {
                // The clock was corrected from the answer's time: again, with a newer record.
                self.last_record_ts = self.last_record_ts.max(self.server_now(now_ms));
                self.failures += 1;
                self.retry_at = now_ms + 200 * u64::from(self.failures.min(10));
                if self.failures > 5 {
                    self.failed(now_ms, changes, &refused(status, &error.error));
                }
            }
            _ => self.failed(now_ms, changes, &refused(status, &error.error)),
        }
    }

    fn set_clock(&mut self, server_now: u64, now_ms: u64) {
        self.clock_offset = server_now as i64 - now_ms as i64;
    }

    fn failed(&mut self, now_ms: u64, changes: &mut Changes, why: &str) {
        self.failures = self.failures.saturating_add(1);
        let backoff = (500u64 << self.failures.min(6)).min(30_000);
        self.retry_at = now_ms + backoff;
        changes.problem = Some(why.to_string());
        changes.shown = true;
    }

    fn close(&mut self, changes: &mut Changes, why: &str) {
        self.state = Membership::Closed;
        self.outbox.clear();
        changes.problem = Some(why.to_string());
        changes.shown = true;
    }

    fn apply_sync(
        &mut self,
        me: &Identity,
        answer: SyncAnswer,
        now_ms: u64,
        changes: &mut Changes,
    ) {
        self.failures = 0;
        self.set_clock(answer.now, now_ms);
        self.last_sync = Some(now_ms);
        if !self.take_room(&answer.room, changes) {
            return;
        }
        if let Some(records) = answer.members {
            self.take_members(me, records, answer.left.unwrap_or_default(), changes);
            self.members_rev = Some(answer.members_rev);
        }
        self.members_read_at = Some(now_ms);
        self.max_epoch = self.max_epoch.max(answer.epoch);
        let waiting_keys = std::mem::take(&mut self.waiting_keys);
        for key in waiting_keys.into_iter().chain(answer.keys) {
            self.take_key(me, key, changes);
        }
        self.key_cursor = self.key_cursor.max(answer.keys_next);
        let up_to_date = !answer.more;
        let waiting = std::mem::take(&mut self.waiting);
        for (m, waited) in waiting {
            self.take_message(me, m, changes, waited + u32::from(up_to_date));
        }
        for m in answer.messages {
            self.cursor = self.cursor.max(m.seq);
            self.take_message(me, m, changes, 0);
        }
        self.cursor = self.cursor.max(answer.next);
        self.more = answer.more;
        if up_to_date {
            // What still has no key after a few reads was written before this device could read
            // it: no copy of its key was ever sealed to this device.
            let before = self.waiting.len();
            self.waiting.retain(|(_, waited)| *waited < WAIT_READS);
            self.before_join += (before - self.waiting.len()) as u32;
            if !self.synced_once {
                self.synced_once = true;
                changes.history = Some(self.messages.len());
                changes.shown = true;
            }
        }
    }

    /// The room's record; false when this device refuses the room (no invite key, or not the one
    /// of the link).
    fn take_room(&mut self, room: &WireRoom, changes: &mut Changes) -> bool {
        if room.room != self.room {
            self.close(
                changes,
                "The meeting server sent another room than this one.",
            );
            return false;
        }
        if !room.code.is_empty() {
            self.code = room.code.clone();
        }
        self.kind = RoomKind::parse(room.kind.as_deref());
        self.starts_at = room
            .starts_at
            .as_deref()
            .and_then(azul_storage::time::parse_iso8601);
        self.ends_at = room
            .ends_at
            .as_deref()
            .and_then(azul_storage::time::parse_iso8601);
        let Some(key) = room.invite_key.as_deref().filter(|k| crypto::is_device(k)) else {
            self.close(
                changes,
                "This room was made by an older AzMeet and is not end-to-end encrypted, so this \
                 AzMeet does not use it.",
            );
            return false;
        };
        let mismatch = self
            .invite
            .as_ref()
            .is_some_and(|invite| invite.invite_key() != key);
        if mismatch {
            self.close(
                changes,
                "The meeting server sent a room that does not match this link.",
            );
            return false;
        }
        self.invite_key = Some(key.to_string());
        true
    }

    /// Whether record `r` is signed by its device (and its keys are keys).
    fn signed_record(&self, r: &WireMember) -> bool {
        crypto::is_device(&r.device)
            && crypto::is_device(&r.dh)
            && crypto::verify(
                &r.device,
                &r.sig,
                &crypto::member_input(
                    &self.room,
                    &r.device,
                    &r.dh,
                    r.sealed_name.as_deref(),
                    r.name.as_deref(),
                    r.ts,
                ),
            )
    }

    /// Whether record `r` carries the link's proof.
    fn has_proof(&self, invite_key: &str, r: &WireMember) -> bool {
        r.proof.as_deref().is_some_and(|proof| {
            crypto::verify(
                invite_key,
                proof,
                &crypto::proof_input(&self.room, &r.device, &r.dh),
            )
        })
    }

    /// Whether record `r` was let in by a device in `linked` (one with the link's proof).
    fn admitted_by(&self, r: &WireMember, linked: &BTreeSet<String>) -> bool {
        match (r.admitted_by.as_deref(), r.admission.as_deref()) {
            (Some(by), Some(admission)) => {
                linked.contains(by)
                    && crypto::verify(
                        by,
                        admission,
                        &crypto::admit_input(&self.room, &r.device, &r.dh),
                    )
            }
            _ => false,
        }
    }

    fn member_of(&self, r: &WireMember) -> Member {
        let name = match (r.sealed_name.as_deref(), &self.invite) {
            (Some(sealed), Some(invite)) => invite
                .open_name(&r.device, sealed)
                .map(|n| clean_name(&n))
                .unwrap_or_default(),
            _ => r.name.as_deref().map(clean_name).unwrap_or_default(),
        };
        Member {
            device: r.device.clone(),
            dh: r.dh.clone(),
            name,
            safety_code: crypto::safety_code(&r.device, &r.dh).unwrap_or_default(),
            ts: r.ts,
            admitted_by: r.admitted_by.clone(),
        }
    }

    /// The members and knocks as the Worker lists them now, each record checked (CRYPTO.md
    /// sections 5 to 7), and who left.
    fn take_members(
        &mut self,
        me: &Identity,
        records: Vec<WireMember>,
        left: Vec<WireMember>,
        changes: &mut Changes,
    ) {
        let Some(invite_key) = self.invite_key.clone() else {
            return;
        };
        let valid: Vec<WireMember> = records
            .into_iter()
            .filter(|r| {
                self.departed
                    .get(&r.device)
                    .map_or(true, |seen| r.ts > *seen)
                    && self.signed_record(r)
            })
            .collect();
        for r in &valid {
            if r.state == "member" && self.has_proof(&invite_key, r) {
                self.linked.insert(r.device.clone());
            }
        }
        let mut members = BTreeMap::new();
        let mut knocks = BTreeMap::new();
        for r in &valid {
            let with_link = self.linked.contains(&r.device) && self.has_proof(&invite_key, r);
            if r.state == "member" && (with_link || self.admitted_by(r, &self.linked)) {
                members.insert(r.device.clone(), self.member_of(r));
            } else if r.state == "knocking" {
                knocks.insert(r.device.clone(), self.member_of(r));
            }
        }
        // Who left, by its own record: what it signed as a member still verifies.
        for r in &left {
            if !self.signed_record(r) {
                continue;
            }
            if self.has_proof(&invite_key, r) || self.admitted_by(r, &self.linked) {
                let member = self.member_of(r);
                self.known.entry(r.device.clone()).or_insert(member);
            }
            let seen = self.departed.entry(r.device.clone()).or_insert(0);
            *seen = (*seen).max(r.ts);
        }
        for (device, before) in &self.members {
            if !members.contains_key(device) {
                changes.left.push(before.clone());
                let seen = self.departed.entry(device.clone()).or_insert(0);
                *seen = (*seen).max(before.ts);
            }
        }
        for (device, member) in &members {
            if !self.members.contains_key(device) && device != me.device() {
                changes.joined.push(member.clone());
            }
            self.known.insert(device.clone(), member.clone());
        }
        for (device, knock) in &knocks {
            if !self.knocks.contains_key(device) && device != me.device() {
                changes.knocking.push(knock.clone());
            }
        }
        // This device's own record: a new one must be newer.
        if let Some(mine) = valid
            .iter()
            .chain(left.iter())
            .find(|r| r.device == me.device())
        {
            self.last_record_ts = self.last_record_ts.max(mine.ts);
        }
        if self.state == Membership::Knocking && members.contains_key(me.device()) {
            // Let in: the key with the link comes next.
            self.state = Membership::Member;
            changes.shown = true;
        }
        if members != self.members || knocks != self.knocks {
            changes.shown = true;
        }
        self.members = members;
        self.knocks = knocks;
        self.admit_queue.retain(|d| self.knocks.contains_key(d));
    }

    /// A copy of a room key sealed to this device, checked: its sender a member here at some time,
    /// its record signed by it and listing this device, its copy opening to the key it names.
    fn take_key(&mut self, me: &Identity, k: WireKey, changes: &mut Changes) {
        self.key_cursor = self.key_cursor.max(k.seq);
        if self.keys.contains_key(&k.key_id) {
            return;
        }
        if !self.known.contains_key(&k.sender) {
            // Its sender's record may come with the next read.
            if self.waiting_keys.len() < MAX_WAITING {
                self.waiting_keys.push(k);
            }
            return;
        }
        let listed = k.members.iter().any(|m| m == me.device());
        let signed = crypto::verify(
            &k.sender,
            &k.sig,
            &crypto::key_input(&self.room, &k.key_id, k.epoch, &k.members),
        );
        if !listed || !signed {
            self.unreadable += 1;
            return;
        }
        match crypto::open_key(me, &self.room, &k.key_id, k.epoch, &k.sender, &k.sealed) {
            Ok((key, secret)) => {
                if self.invite.is_none() {
                    self.adopt_invite(&secret, changes);
                }
                self.max_epoch = self.max_epoch.max(k.epoch);
                let members: BTreeSet<String> = k.members.iter().cloned().collect();
                changes.keys.push(KeyEvent {
                    epoch: k.epoch,
                    key_id: k.key_id.clone(),
                    members: members.len(),
                    made_here: false,
                    sender: k.sender.clone(),
                });
                self.keys.insert(
                    k.key_id.clone(),
                    HeldKey {
                        key,
                        epoch: k.epoch,
                        sender: k.sender,
                        members,
                    },
                );
                changes.shown = true;
            }
            Err(_) => self.unreadable += 1,
        }
    }

    /// The invite secret a knock got with its first key: it holds the link now, when the secret is
    /// the room's (its key the one the room was registered with).
    fn adopt_invite(&mut self, secret: &str, changes: &mut Changes) {
        let Some(invite) = Invite::new(&self.room, secret) else {
            return;
        };
        if self.invite_key.as_deref() != Some(invite.invite_key().as_str()) {
            return;
        }
        self.invite = Some(invite);
        // Its record goes out again with the link's proof and its name sealed, and the others'
        // names open now.
        self.rejoin = true;
        self.members_rev = None;
        changes.admitted = true;
        changes.shown = true;
    }

    /// A message: checked (its signature, its sender a member that held its key), opened and
    /// listed in order; one whose key this device does not hold yet waits.
    fn take_message(&mut self, me: &Identity, m: WireMessage, changes: &mut Changes, waited: u32) {
        if self.seen.contains(&m.id) {
            // This device's own message, back from the Worker: its number.
            let mut numbered = false;
            if let Some(own) = self.messages.iter_mut().find(|x| x.id == m.id) {
                if own.seq.is_none() {
                    own.seq = Some(m.seq);
                    numbered = true;
                }
            }
            if numbered {
                self.reorder();
                changes.shown = true;
            }
            return;
        }
        let signed = crypto::verify(
            &m.sender,
            &m.sig,
            &crypto::message_input(&self.room, &m.id, &m.key_id, &m.body),
        );
        if !signed {
            self.unreadable += 1;
            return;
        }
        let Some(key) = self
            .keys
            .get(&m.key_id)
            .filter(|_| self.known.contains_key(&m.sender))
        else {
            if self.waiting.len() < MAX_WAITING {
                self.waiting.push((m, waited));
            }
            return;
        };
        if !key.members.contains(&m.sender) {
            self.unreadable += 1;
            return;
        }
        let Ok(plain) = crypto::open_message(&key.key, &self.room, &m.id, &m.sender, &m.body)
        else {
            self.unreadable += 1;
            return;
        };
        let mine = m.sender == me.device();
        let message = RoomMessage {
            id: m.id.clone(),
            seq: Some(m.seq),
            sender: m.sender,
            name: clean_name(&plain.name),
            text: plain.text,
            ts: plain.ts,
            mine,
        };
        self.seen.insert(m.id);
        self.insert_listed(message.clone());
        if !mine {
            changes.arrived.push(message);
        }
        changes.shown = true;
    }

    fn insert_listed(&mut self, message: RoomMessage) {
        let seq = message.seq.unwrap_or(u64::MAX);
        let at = self
            .messages
            .iter()
            .position(|m| m.seq.map_or(true, |s| s > seq))
            .unwrap_or(self.messages.len());
        self.messages.insert(at, message);
        self.trim_listed();
    }

    /// In the Worker's order, this device's own on their way last.
    fn reorder(&mut self) {
        self.messages.sort_by_key(|m| m.seq.unwrap_or(u64::MAX));
    }

    fn trim_listed(&mut self) {
        if self.messages.len() > MAX_LISTED {
            let extra = self.messages.len() - MAX_LISTED;
            self.messages.drain(..extra);
        }
    }
}

/// What to tell the user about a refused request.
fn refused(status: u16, code: &str) -> String {
    match (status, code) {
        (429, _) => {
            String::from("Too many requests from this network; AzMeet tries again in a moment.")
        }
        (_, "") => format!("The meeting server answered {status}."),
        (_, code) => format!("The meeting server answered {status} ({code})."),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::crypto::{random_id, Invite};

    /// An in-memory meet Worker with the rules of the real one (`cf-workers/meet/src/handler.js`)
    /// for the routes a room uses: it checks every signature the real one checks, and it can be
    /// made to lie (`forge_*`, `relist`).
    #[derive(Debug, Default)]
    struct FakeWorker {
        now: u64,
        rooms: BTreeMap<String, FakeRoom>,
        seq: u64,
        /// Answer the next request with 401 stale_request and the server's time.
        stale_once: bool,
    }

    #[derive(Debug, Default)]
    struct FakeRoom {
        code: String,
        invite_key: String,
        kind: String,
        members_rev: u64,
        members: BTreeMap<String, Row>,
        keys: Vec<KeyRow>,
        envelopes: Vec<(u64, String, String, String)>,
        messages: Vec<MsgRow>,
    }

    #[derive(Debug, Clone)]
    struct Row {
        dh: String,
        sealed_name: Option<String>,
        name: Option<String>,
        ts: u64,
        sig: String,
        proof: Option<String>,
        admitted_by: Option<String>,
        admission: Option<String>,
        state: String,
    }

    #[derive(Debug, Clone)]
    struct KeyRow {
        key_id: String,
        epoch: u64,
        sender: String,
        members: Vec<String>,
        sig: String,
    }

    #[derive(Debug, Clone)]
    struct MsgRow {
        seq: u64,
        id: String,
        sender: String,
        key_id: String,
        body: String,
        sig: String,
    }

    fn answer(status: u16, body: Value) -> (u16, String) {
        (status, body.to_string())
    }

    fn text(v: &Value, key: &str) -> Option<String> {
        v.get(key).and_then(Value::as_str).map(str::to_string)
    }

    impl FakeWorker {
        fn new() -> FakeWorker {
            FakeWorker {
                now: 1_760_000_000_000,
                ..FakeWorker::default()
            }
        }

        fn create(&mut self, room: &str, invite_key: &str, kind: &str) {
            self.rooms.insert(
                room.to_string(),
                FakeRoom {
                    code: String::from("xq4-8kd-2nm"),
                    invite_key: invite_key.to_string(),
                    kind: kind.to_string(),
                    ..FakeRoom::default()
                },
            );
        }

        fn next_seq(&mut self) -> u64 {
            self.seq += 1;
            self.seq
        }

        /// Everything the Worker keeps, as text: what an attacker with the database reads.
        fn dump(&self) -> String {
            format!("{:?}", self.rooms)
        }

        /// `call` as `me` sends it: (status, body).
        fn handle(&mut self, me: &Identity, call: &Call) -> (u16, String) {
            if std::mem::take(&mut self.stale_once) {
                return answer(401, json!({ "error": "stale_request", "now": self.now }));
            }
            let mut caller = None;
            if call.signed {
                let headers = me.request_headers(call.method, &call.path, self.now, &call.body);
                let ts: u64 = headers[1].1.parse().unwrap();
                let input = crypto::request_input(call.method, &call.path, ts, &call.body);
                if !crypto::verify(&headers[0].1, &headers[2].1, &input) {
                    return answer(401, json!({ "error": "bad_signature" }));
                }
                caller = Some(headers[0].1.clone());
            }
            let body: Value = if call.body.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&call.body).unwrap()
            };
            let parts: Vec<String> = call
                .path
                .split('/')
                .filter(|p| !p.is_empty())
                .map(str::to_string)
                .collect();
            let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
            let caller = caller.unwrap_or_default();
            match (call.method, parts.as_slice()) {
                ("GET", ["rooms", room, "sync"]) => self.sync(room, &call.query),
                ("PUT", ["rooms", room, "members", device]) if *device == caller => {
                    self.put_member(room, device, &body)
                }
                ("DELETE", ["rooms", room, "members", device]) if *device == caller => {
                    self.leave(room, device)
                }
                ("POST", ["rooms", room, "members", device, "admit"]) => {
                    self.admit(room, &caller, device, &body)
                }
                ("POST", ["rooms", room, "keys"]) => self.post_key(room, &caller, &body),
                ("POST", ["rooms", room, "messages"]) => self.post_message(room, &caller, &body),
                _ => answer(403, json!({ "error": "not_your_record" })),
            }
        }

        fn room_mut(&mut self, room: &str) -> Option<&mut FakeRoom> {
            self.rooms.get_mut(room)
        }

        fn put_member(&mut self, room: &str, device: &str, body: &Value) -> (u16, String) {
            let Some(r) = self.rooms.get_mut(room) else {
                return answer(404, json!({ "error": "not_found" }));
            };
            let row = Row {
                dh: text(body, "dh").unwrap(),
                sealed_name: text(body, "sealed_name"),
                name: text(body, "name"),
                ts: body["ts"].as_u64().unwrap(),
                sig: text(body, "sig").unwrap(),
                proof: text(body, "proof"),
                admitted_by: None,
                admission: None,
                state: String::new(),
            };
            let input = crypto::member_input(
                room,
                device,
                &row.dh,
                row.sealed_name.as_deref(),
                row.name.as_deref(),
                row.ts,
            );
            if !crypto::verify(device, &row.sig, &input) {
                return answer(401, json!({ "error": "bad_record_signature" }));
            }
            let existing = r.members.get(device).cloned();
            if existing.as_ref().is_some_and(|e| row.ts <= e.ts) {
                return answer(409, json!({ "error": "stale_record" }));
            }
            let mut row = row;
            if let Some(proof) = &row.proof {
                if !crypto::verify(
                    &r.invite_key,
                    proof,
                    &crypto::proof_input(room, device, &row.dh),
                ) {
                    return answer(403, json!({ "error": "bad_proof" }));
                }
                row.state = String::from("member");
            } else {
                row.state = if existing.as_ref().is_some_and(|e| e.state == "member") {
                    String::from("member")
                } else {
                    String::from("knocking")
                };
            }
            if let Some(e) = existing.filter(|e| e.state == "member" && row.state == "member") {
                row.admitted_by = e.admitted_by;
                row.admission = e.admission;
            }
            let state = row.state.clone();
            r.members.insert(device.to_string(), row);
            r.members_rev += 1;
            answer(
                200,
                json!({ "ok": true, "state": state, "members_rev": r.members_rev }),
            )
        }

        fn leave(&mut self, room: &str, device: &str) -> (u16, String) {
            let Some(r) = self.rooms.get_mut(room) else {
                return answer(404, json!({ "error": "not_found" }));
            };
            match r.members.get_mut(device) {
                Some(row) if row.state != "left" => {
                    row.state = String::from("left");
                    r.members_rev += 1;
                    answer(200, json!({ "ok": true, "removed": true }))
                }
                _ => answer(200, json!({ "ok": true, "removed": false })),
            }
        }

        fn admit(&mut self, room: &str, caller: &str, device: &str, body: &Value) -> (u16, String) {
            let Some(r) = self.rooms.get_mut(room) else {
                return answer(404, json!({ "error": "not_found" }));
            };
            let admitter_ok = r
                .members
                .get(caller)
                .is_some_and(|m| m.state == "member" && m.proof.is_some());
            if !admitter_ok {
                return answer(403, json!({ "error": "not_a_member" }));
            }
            let Some(knock) = r.members.get_mut(device).filter(|m| m.state == "knocking") else {
                return answer(409, json!({ "error": "not_knocking" }));
            };
            let admission = text(body, "admission").unwrap();
            if !crypto::verify(
                caller,
                &admission,
                &crypto::admit_input(room, device, &knock.dh),
            ) {
                return answer(401, json!({ "error": "bad_admission" }));
            }
            knock.state = String::from("member");
            knock.admitted_by = Some(caller.to_string());
            knock.admission = Some(admission);
            r.members_rev += 1;
            answer(200, json!({ "ok": true, "state": "member" }))
        }

        fn post_key(&mut self, room: &str, caller: &str, body: &Value) -> (u16, String) {
            let seq_base = self.seq;
            let Some(r) = self.rooms.get(room) else {
                return answer(404, json!({ "error": "not_found" }));
            };
            if !r.members.get(caller).is_some_and(|m| m.state == "member") {
                return answer(403, json!({ "error": "not_a_member" }));
            }
            let key_id = text(body, "key_id").unwrap();
            let epoch = body["epoch"].as_u64().unwrap();
            let members: Vec<String> = body["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m.as_str().unwrap().to_string())
                .collect();
            let sig = text(body, "sig").unwrap();
            if !crypto::verify(
                caller,
                &sig,
                &crypto::key_input(room, &key_id, epoch, &members),
            ) {
                return answer(401, json!({ "error": "bad_key_signature" }));
            }
            if members.iter().any(|m| !r.members.contains_key(m)) {
                return answer(400, json!({ "error": "unknown_recipient" }));
            }
            let envelopes: Vec<(String, String)> = body["envelopes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| (text(e, "recipient").unwrap(), text(e, "sealed").unwrap()))
                .collect();
            let mut seq = seq_base;
            let r = self.rooms.get_mut(room).unwrap();
            for (recipient, sealed) in envelopes {
                seq += 1;
                r.envelopes.push((seq, key_id.clone(), recipient, sealed));
            }
            let mut sorted = members;
            sorted.sort();
            r.keys.push(KeyRow {
                key_id: key_id.clone(),
                epoch,
                sender: caller.to_string(),
                members: sorted,
                sig,
            });
            self.seq = seq;
            answer(201, json!({ "ok": true, "key_id": key_id, "epoch": epoch }))
        }

        fn post_message(&mut self, room: &str, caller: &str, body: &Value) -> (u16, String) {
            let seq = self.next_seq();
            let Some(r) = self.room_mut(room) else {
                return answer(404, json!({ "error": "not_found" }));
            };
            if !r.members.get(caller).is_some_and(|m| m.state == "member") {
                return answer(403, json!({ "error": "not_a_member" }));
            }
            let id = text(body, "id").unwrap();
            let key_id = text(body, "key_id").unwrap();
            let text_body = text(body, "body").unwrap();
            let sig = text(body, "sig").unwrap();
            if !crypto::verify(
                caller,
                &sig,
                &crypto::message_input(room, &id, &key_id, &text_body),
            ) {
                return answer(401, json!({ "error": "bad_message_signature" }));
            }
            if !r.keys.iter().any(|k| k.key_id == key_id) {
                return answer(409, json!({ "error": "unknown_key" }));
            }
            if let Some(existing) = r.messages.iter().find(|m| m.id == id) {
                return answer(
                    200,
                    json!({ "ok": true, "seq": existing.seq, "duplicate": true }),
                );
            }
            r.messages.push(MsgRow {
                seq,
                id: id.clone(),
                sender: caller.to_string(),
                key_id,
                body: text_body,
                sig,
            });
            answer(201, json!({ "ok": true, "id": id, "seq": seq }))
        }

        fn sync(&mut self, room: &str, query: &str) -> (u16, String) {
            let q: BTreeMap<&str, &str> = query
                .split('&')
                .filter_map(|kv| kv.split_once('='))
                .collect();
            let Some(r) = self.rooms.get(room) else {
                return answer(404, json!({ "error": "not_found" }));
            };
            let device = q["for"];
            let after: u64 = q.get("after").map_or(0, |v| v.parse().unwrap());
            let keys_after: u64 = q.get("keys_after").map_or(0, |v| v.parse().unwrap());
            let known: Option<u64> = q.get("members_rev").map(|v| v.parse().unwrap());
            let member_json = |device: &str, row: &Row| {
                json!({
                    "device": device, "dh": row.dh, "sealed_name": row.sealed_name, "name": row.name,
                    "ts": row.ts, "sig": row.sig, "proof": row.proof, "admitted_by": row.admitted_by,
                    "admission": row.admission, "state": row.state, "joined": "2026-10-08T00:00:00.000Z",
                })
            };
            let mut out = json!({
                "room": {
                    "room": room, "code": r.code, "link": format!("azlin://meet/{room}"),
                    "invite_key": r.invite_key, "kind": r.kind, "starts_at": null, "ends_at": null,
                },
                "now": self.now,
                "members_rev": r.members_rev,
            });
            if known != Some(r.members_rev) {
                out["members"] = r
                    .members
                    .iter()
                    .filter(|(_, row)| row.state != "left")
                    .map(|(d, row)| member_json(d, row))
                    .collect();
                out["left"] = r
                    .members
                    .iter()
                    .filter(|(_, row)| row.state == "left")
                    .map(|(d, row)| member_json(d, row))
                    .collect();
            }
            let keys: Vec<Value> = r
                .envelopes
                .iter()
                .filter(|(seq, _, recipient, _)| recipient == device && *seq > keys_after)
                .filter_map(|(seq, key_id, _, sealed)| {
                    let k = r.keys.iter().find(|k| &k.key_id == key_id)?;
                    Some(json!({
                        "seq": seq, "key_id": key_id, "epoch": k.epoch, "sender": k.sender,
                        "members": k.members, "sig": k.sig, "sealed": sealed,
                    }))
                })
                .collect();
            let keys_next = keys
                .last()
                .and_then(|k| k["seq"].as_u64())
                .unwrap_or(keys_after);
            out["keys"] = Value::Array(keys);
            out["keys_next"] = json!(keys_next);
            out["epoch"] = json!(r.keys.iter().map(|k| k.epoch).max().unwrap_or(0));
            let messages: Vec<Value> = r
                .messages
                .iter()
                .filter(|m| m.seq > after)
                .map(|m| {
                    json!({
                        "seq": m.seq, "id": m.id, "sender": m.sender, "key_id": m.key_id,
                        "body": m.body, "sig": m.sig,
                    })
                })
                .collect();
            let next = messages
                .last()
                .and_then(|m| m["seq"].as_u64())
                .unwrap_or(after);
            out["messages"] = Value::Array(messages);
            out["next"] = json!(next);
            out["more"] = json!(false);
            answer(200, out)
        }
    }

    /// A device in one room.
    struct Device {
        me: Identity,
        room: ChatRoom,
    }

    fn device(first: u8, name: &str, room: &str, invite: Option<Invite>) -> Device {
        let mut seed = [0u8; 32];
        seed[0] = first;
        seed[31] = 0x5a;
        Device {
            me: Identity::from_seed(seed),
            room: ChatRoom::new("http://meet.test", room, "", RoomKind::Chat, invite, name),
        }
    }

    /// Runs the device's requests against `worker` until it has nothing more to say (two quiet
    /// syncs in a row, or nothing to send for a while): all that changed. The clock moves 50 ms a
    /// step, so a retry after a refusal comes within the run.
    fn settle(d: &mut Device, worker: &mut FakeWorker) -> Changes {
        let mut all = Changes::default();
        let mut quiet = 0;
        let mut idle = 0;
        for _ in 0..120 {
            worker.now += 50;
            let Some(call) = d.room.next_call(&d.me, worker.now, 0) else {
                idle += 1;
                if idle > 10 {
                    break;
                }
                continue;
            };
            idle = 0;
            let (status, body) = worker.handle(&d.me, &call);
            let changes = d
                .room
                .on_answer(&d.me, &call.what, Some(status), &body, worker.now);
            let calm = call.what == CallKind::Sync && changes == Changes::default();
            all.absorb(changes);
            quiet = if calm { quiet + 1 } else { 0 };
            if quiet >= 2 {
                break;
            }
        }
        all
    }

    /// A chat room on `worker` with its invite.
    fn room(worker: &mut FakeWorker) -> (String, Invite) {
        let room = random_id().unwrap();
        let invite = Invite::generate(&room).unwrap();
        worker.create(&room, &invite.invite_key(), "chat");
        (room, invite)
    }

    fn texts(changes: &Changes) -> Vec<(String, String)> {
        changes
            .arrived
            .iter()
            .map(|m| (m.name.clone(), m.text.clone()))
            .collect()
    }

    fn pair(a: &str, b: &str) -> (String, String) {
        (a.to_string(), b.to_string())
    }

    /// Ada and Ben with the link, both members, each knowing the other.
    fn ada_and_ben(worker: &mut FakeWorker) -> (Device, Device, String, Invite) {
        let (id, invite) = room(worker);
        let mut ada = device(1, "Ada", &id, Some(invite.clone()));
        let mut ben = device(2, "Ben", &id, Some(invite.clone()));
        ada.room.join();
        settle(&mut ada, worker);
        ben.room.join();
        settle(&mut ben, worker);
        settle(&mut ada, worker);
        assert_eq!(ada.room.state, Membership::Member);
        assert_eq!(ben.room.state, Membership::Member);
        (ada, ben, id, invite)
    }

    #[test]
    fn two_devices_with_the_link_chat_and_the_worker_holds_no_text_and_no_name() {
        let mut w = FakeWorker::new();
        let (mut ada, mut ben, _, _) = ada_and_ben(&mut w);
        assert_eq!(ada.room.members().count(), 2);
        assert_eq!(ada.room.member(ben.me.device()).unwrap().name, "Ben");
        assert_eq!(
            ben.room.member(ada.me.device()).unwrap().safety_code,
            ada.me.safety_code()
        );
        let listed = ada
            .room
            .send(&ada.me, "Hello Ben, can you see me?", w.now)
            .unwrap();
        assert_eq!(listed.seq, None, "on its way");
        let sent = settle(&mut ada, &mut w);
        assert!(
            sent.keys
                .iter()
                .any(|k| k.made_here && k.epoch == 1 && k.members == 2),
            "{sent:?}"
        );
        assert!(
            ada.room.messages()[0].seq.is_some(),
            "numbered once the Worker took it"
        );
        let got = settle(&mut ben, &mut w);
        assert_eq!(texts(&got), vec![pair("Ada", "Hello Ben, can you see me?")]);
        assert_eq!(got.keys.len(), 1, "Ben took Ada's key");
        assert_eq!(ben.room.unread(), 1);
        assert!(ben.room.mark_read());
        assert_eq!(ben.room.unread(), 0);
        ben.room
            .send(&ben.me, "Loud and clear, Ada", w.now)
            .unwrap();
        settle(&mut ben, &mut w);
        let back = settle(&mut ada, &mut w);
        assert_eq!(texts(&back), vec![pair("Ben", "Loud and clear, Ada")]);
        assert_eq!(
            ben.room.current_key(ben.me.device()).map(|k| k.0),
            Some(1),
            "no new key: the same members"
        );
        let db = w.dump();
        for plain in ["Hello Ben", "Loud and clear", "\"Ada\"", "\"Ben\""] {
            assert!(!db.contains(plain), "the Worker holds {plain:?}");
        }
    }

    #[test]
    fn after_a_member_leaves_the_next_message_is_under_a_key_it_has_no_copy_of() {
        let mut w = FakeWorker::new();
        let (mut ada, mut ben, id, invite) = ada_and_ben(&mut w);
        let mut cleo = device(3, "Cleo", &id, Some(invite));
        cleo.room.join();
        settle(&mut cleo, &mut w);
        settle(&mut ada, &mut w);
        settle(&mut ben, &mut w);
        ada.room.send(&ada.me, "All three of us", w.now).unwrap();
        let first = settle(&mut ada, &mut w);
        assert!(first.keys.iter().any(|k| k.made_here && k.members == 3));
        assert_eq!(
            texts(&settle(&mut cleo, &mut w)),
            vec![pair("Ada", "All three of us")]
        );
        settle(&mut ben, &mut w);
        cleo.room.leave();
        settle(&mut cleo, &mut w);
        assert_eq!(cleo.room.state, Membership::Left);
        let seen = settle(&mut ada, &mut w);
        assert_eq!(
            seen.left
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Cleo"]
        );
        ada.room.send(&ada.me, "Just us two now", w.now).unwrap();
        let rotated = settle(&mut ada, &mut w);
        let key = rotated
            .keys
            .iter()
            .find(|k| k.made_here)
            .expect("a new key");
        assert_eq!((key.epoch, key.members), (2, 2), "{rotated:?}");
        assert_eq!(
            texts(&settle(&mut ben, &mut w)),
            vec![pair("Ada", "Just us two now")]
        );
        // Cleo, reading everything the Worker has, cannot open it: no copy of the new key is hers.
        let mut nosy = device(3, "Cleo", &id, cleo.room.invite().cloned());
        nosy.room.state = Membership::Outside;
        let read = settle(&mut nosy, &mut w);
        assert!(
            !read.arrived.iter().any(|m| m.text == "Just us two now"),
            "{read:?}"
        );
        assert!(
            read.arrived.iter().any(|m| m.text == "All three of us"),
            "what she was sent, she reads"
        );
    }

    #[test]
    fn a_newcomer_reads_what_is_written_after_it_joined_and_nothing_before() {
        let mut w = FakeWorker::new();
        let (mut ada, mut ben, id, invite) = ada_and_ben(&mut w);
        ada.room.send(&ada.me, "Before Cleo", w.now).unwrap();
        settle(&mut ada, &mut w);
        settle(&mut ben, &mut w);
        let mut cleo = device(3, "Cleo", &id, Some(invite));
        cleo.room.join();
        settle(&mut cleo, &mut w);
        settle(&mut ada, &mut w);
        ada.room.send(&ada.me, "Welcome, Cleo", w.now).unwrap();
        settle(&mut ada, &mut w);
        let got = settle(&mut cleo, &mut w);
        assert_eq!(texts(&got), vec![pair("Ada", "Welcome, Cleo")]);
        assert_eq!(
            cleo.room.before_join(),
            1,
            "one message was written before she could read it"
        );
        assert_eq!(cleo.room.unreadable(), 0, "and nothing failed a check");
    }

    #[test]
    fn a_device_with_only_the_code_knocks_is_let_in_and_then_holds_the_link() {
        let mut w = FakeWorker::new();
        let (id, invite) = room(&mut w);
        let mut ada = device(1, "Ada", &id, Some(invite.clone()));
        ada.room.join();
        settle(&mut ada, &mut w);
        let mut cleo = device(3, "Cleo", &id, None);
        cleo.room.join();
        settle(&mut cleo, &mut w);
        assert_eq!(cleo.room.state, Membership::Knocking);
        assert_eq!(
            cleo.room.link("azlin://meet/"),
            format!("azlin://meet/{id}"),
            "no secret yet"
        );
        let seen = settle(&mut ada, &mut w);
        assert_eq!(
            seen.knocking
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Cleo"]
        );
        let knock = ada.room.knocks().next().unwrap().clone();
        assert_eq!(
            knock.safety_code,
            cleo.me.safety_code(),
            "Ada can compare Cleo's code before letting her in"
        );
        ada.room.admit(&knock.device);
        let admitted = settle(&mut ada, &mut w);
        assert!(
            admitted.keys.iter().any(|k| k.made_here && k.members == 2),
            "a key for Cleo at once: {admitted:?}"
        );
        let let_in = settle(&mut cleo, &mut w);
        assert!(let_in.admitted, "{let_in:?}");
        assert_eq!(cleo.room.state, Membership::Member);
        assert_eq!(
            cleo.room.link("azlin://meet/"),
            invite_link(&id, &invite),
            "she holds the link now"
        );
        assert_eq!(
            cleo.room.member(ada.me.device()).map(|m| m.name.as_str()),
            Some("Ada"),
            "names open with the link"
        );
        settle(&mut ada, &mut w);
        ada.room.send(&ada.me, "Hi Cleo", w.now).unwrap();
        settle(&mut ada, &mut w);
        assert_eq!(
            texts(&settle(&mut cleo, &mut w)),
            vec![pair("Ada", "Hi Cleo")]
        );
        let mine = w.rooms[&id].members[cleo.me.device()].clone();
        assert!(
            mine.proof.is_some() && mine.name.is_none(),
            "registered again with the proof, the name sealed"
        );
    }

    fn invite_link(room: &str, invite: &Invite) -> String {
        format!("azlin://meet/{room}#{}", invite.secret())
    }

    #[test]
    fn a_record_without_the_links_proof_or_a_key_or_message_from_a_stranger_is_refused() {
        let mut w = FakeWorker::new();
        let (mut ada, mut ben, id, _) = ada_and_ben(&mut w);
        // The Worker adds a device of its own as a member, without a proof (it has no link).
        let mallory = Identity::from_seed([9; 32]);
        let ts = w.now;
        let name = Some("Mallory");
        let sig = mallory.sign(&crypto::member_input(
            &id,
            mallory.device(),
            mallory.dh(),
            None,
            name,
            ts,
        ));
        let r = w.rooms.get_mut(&id).unwrap();
        r.members.insert(
            mallory.device().to_string(),
            Row {
                dh: mallory.dh().to_string(),
                sealed_name: None,
                name: name.map(str::to_string),
                ts,
                sig,
                proof: None,
                admitted_by: None,
                admission: None,
                state: String::from("member"),
            },
        );
        r.members_rev += 1;
        settle(&mut ada, &mut w);
        assert!(
            ada.room.member(mallory.device()).is_none(),
            "no proof, no admission: no member"
        );
        // Ada's next key is sealed to Ada and Ben only: Mallory gets no copy.
        ada.room.send(&ada.me, "Only for Ben", w.now).unwrap();
        let sent = settle(&mut ada, &mut w);
        assert!(
            sent.keys.iter().any(|k| k.made_here && k.members == 2),
            "{sent:?}"
        );
        assert!(!w.rooms[&id]
            .envelopes
            .iter()
            .any(|e| e.2 == mallory.device()));
        // A key record and a message of Mallory's sealed to Ben are not taken.
        let fake = RoomKey::generate().unwrap();
        let members = vec![ben.me.device().to_string(), mallory.device().to_string()];
        let key_sig = mallory.sign(&crypto::key_input(&id, fake.id(), 9, &members));
        let sealed = crypto::seal_key(
            &id,
            &fake,
            9,
            mallory.device(),
            ben.me.device(),
            ben.me.dh(),
            &random_id().unwrap(),
        )
        .unwrap();
        let r = w.rooms.get_mut(&id).unwrap();
        r.keys.push(KeyRow {
            key_id: fake.id().to_string(),
            epoch: 9,
            sender: mallory.device().to_string(),
            members,
            sig: key_sig,
        });
        w.seq += 1;
        let seq = w.seq;
        w.rooms.get_mut(&id).unwrap().envelopes.push((
            seq,
            fake.id().to_string(),
            ben.me.device().to_string(),
            sealed,
        ));
        let msg_id = crypto::new_message_id().unwrap();
        let plain = Plain {
            text: String::from("Send me your secrets"),
            name: String::from("Ada"),
            ts: w.now,
        };
        let body = crypto::seal_message(&fake, &id, &msg_id, mallory.device(), &plain).unwrap();
        let msg_sig = mallory.sign(&crypto::message_input(&id, &msg_id, fake.id(), &body));
        w.seq += 1;
        let seq = w.seq;
        w.rooms.get_mut(&id).unwrap().messages.push(MsgRow {
            seq,
            id: msg_id,
            sender: mallory.device().to_string(),
            key_id: fake.id().to_string(),
            body,
            sig: msg_sig,
        });
        let got = settle(&mut ben, &mut w);
        assert_eq!(
            texts(&got),
            vec![pair("Ada", "Only for Ben")],
            "not Mallory's"
        );
        assert_eq!(
            ben.room.current_key(ben.me.device()).map(|k| k.0),
            Some(1),
            "Ben never sends with Mallory's key"
        );
    }

    #[test]
    fn a_departed_members_old_record_brought_back_by_the_worker_stays_refused() {
        let mut w = FakeWorker::new();
        let (mut ada, mut ben, id, _) = ada_and_ben(&mut w);
        let old = w.rooms[&id].members[ben.me.device()].clone();
        ben.room.leave();
        settle(&mut ben, &mut w);
        let seen = settle(&mut ada, &mut w);
        assert_eq!(seen.left.len(), 1);
        // The Worker lists Ben's old record as a member again.
        let r = w.rooms.get_mut(&id).unwrap();
        r.members.insert(ben.me.device().to_string(), old);
        r.members_rev += 1;
        settle(&mut ada, &mut w);
        assert!(
            ada.room.member(ben.me.device()).is_none(),
            "a record older than the leave Ada saw"
        );
        assert!(ada.room.departed().contains_key(ben.me.device()));
    }

    #[test]
    fn two_members_rotating_at_once_converge_on_the_higher_key() {
        let mut w = FakeWorker::new();
        let (mut ada, mut ben, id, invite) = ada_and_ben(&mut w);
        let mut cleo = device(3, "Cleo", &id, Some(invite));
        cleo.room.join();
        settle(&mut cleo, &mut w);
        settle(&mut ada, &mut w);
        settle(&mut ben, &mut w);
        // Both write before either has seen the other's key.
        ada.room.send(&ada.me, "From Ada", w.now).unwrap();
        ben.room.send(&ben.me, "From Ben", w.now).unwrap();
        settle(&mut ada, &mut w);
        settle(&mut ben, &mut w);
        let at_ada = settle(&mut ada, &mut w);
        assert_eq!(texts(&at_ada), vec![pair("Ben", "From Ben")]);
        let at_cleo = settle(&mut cleo, &mut w);
        assert_eq!(at_cleo.arrived.len(), 2, "{at_cleo:?}");
        let a = ada
            .room
            .current_key(ada.me.device())
            .map(|k| k.1.to_string());
        let b = ben
            .room
            .current_key(ben.me.device())
            .map(|k| k.1.to_string());
        let c = cleo
            .room
            .current_key(cleo.me.device())
            .map(|k| k.1.to_string());
        assert_eq!(a, b);
        assert_eq!(b, c, "everyone sends with the same key now");
    }

    #[test]
    fn a_device_that_comes_back_reads_the_history_it_was_a_member_for() {
        let mut w = FakeWorker::new();
        let (mut ada, mut ben, id, invite) = ada_and_ben(&mut w);
        ada.room.send(&ada.me, "one", w.now).unwrap();
        settle(&mut ada, &mut w);
        settle(&mut ben, &mut w);
        ben.room.send(&ben.me, "two", w.now).unwrap();
        settle(&mut ben, &mut w);
        // Ada starts again: the same device, the same link, nothing in memory.
        let mut again = Device {
            me: Identity::from_seed({
                let mut seed = [0u8; 32];
                seed[0] = 1;
                seed[31] = 0x5a;
                seed
            }),
            room: ChatRoom::new(
                "http://meet.test",
                &id,
                "",
                RoomKind::Chat,
                Some(invite),
                "Ada",
            ),
        };
        again.room.state = Membership::Member;
        let read = settle(&mut again, &mut w);
        assert_eq!(read.history, Some(2), "{read:?}");
        let listed: Vec<(&str, bool)> = again
            .room
            .messages()
            .iter()
            .map(|m| (m.text.as_str(), m.mine))
            .collect();
        assert_eq!(listed, vec![("one", true), ("two", false)]);
    }

    #[test]
    fn a_clock_off_by_more_than_the_workers_skew_is_corrected_from_its_answer() {
        let mut w = FakeWorker::new();
        let (id, invite) = room(&mut w);
        let mut ada = device(1, "Ada", &id, Some(invite));
        ada.room.join();
        w.stale_once = true;
        settle(&mut ada, &mut w);
        assert_eq!(
            ada.room.state,
            Membership::Member,
            "joined on the second try"
        );
    }

    #[test]
    fn an_announcement_counts_only_when_a_member_signed_it() {
        let mut w = FakeWorker::new();
        let (ada, ben, _, _) = ada_and_ben(&mut w);
        let node = "ab".repeat(32);
        let sig = ben.room.peer_sig(&ben.me, &node, "endpointben");
        let found = ada
            .room
            .verify_peer(&node, "endpointben", Some(ben.me.device()), Some(&sig));
        assert_eq!(found.map(|m| m.name.as_str()), Some("Ben"));
        assert!(ada
            .room
            .verify_peer(&node, "endpointmallory", Some(ben.me.device()), Some(&sig))
            .is_none());
        assert!(ada
            .room
            .verify_peer(&node, "endpointben", None, None)
            .is_none());
        let stranger = Identity::from_seed([7; 32]);
        let theirs = stranger.sign(&crypto::peer_input(&ada.room.room, &node, "endpointx"));
        assert!(ada
            .room
            .verify_peer(&node, "endpointx", Some(stranger.device()), Some(&theirs))
            .is_none());
    }

    #[test]
    fn a_room_that_does_not_match_its_link_or_is_not_encrypted_is_refused() {
        let mut w = FakeWorker::new();
        let (id, _) = room(&mut w);
        let other = Invite::generate(&id).unwrap();
        let mut ada = device(1, "Ada", &id, Some(other));
        let read = settle(&mut ada, &mut w);
        assert_eq!(ada.room.state, Membership::Closed);
        assert!(read.problem.unwrap().contains("does not match this link"));
        w.rooms.get_mut(&id).unwrap().invite_key = String::from("not a key");
        let mut ben = device(2, "Ben", &id, None);
        let read = settle(&mut ben, &mut w);
        assert_eq!(ben.room.state, Membership::Closed);
        assert!(read.problem.unwrap().contains("not end-to-end encrypted"));
        // An answer about another room than the one asked for is refused too.
        let mut cleo = device(3, "Cleo", &id, None);
        let mut changes = Changes::default();
        let elsewhere = WireRoom {
            room: "0".repeat(26),
            code: String::new(),
            invite_key: None,
            kind: None,
            starts_at: None,
            ends_at: None,
        };
        assert!(!cleo.room.take_room(&elsewhere, &mut changes));
        assert_eq!(cleo.room.state, Membership::Closed);
        assert!(changes.problem.unwrap().contains("another room"));
    }

    #[test]
    fn a_name_is_cleaned_as_the_worker_keeps_it() {
        assert_eq!(
            clean_name("  Ada\u{0}\n  Lovelace\u{202e} "),
            "Ada Lovelace"
        );
        assert_eq!(clean_name(""), "Guest");
        assert_eq!(clean_name("\u{200b}\u{feff}"), "Guest");
        assert_eq!(clean_name(&"x".repeat(200)).chars().count(), MAX_NAME_CHARS);
        assert_eq!(clean_name("Zoë"), "Zoë");
        assert_eq!(RoomKind::parse(Some("chat")), RoomKind::Chat);
        assert_eq!(RoomKind::parse(None).as_str(), "meeting");
    }

    #[test]
    fn a_request_is_one_at_a_time_and_a_stuck_one_is_given_up() {
        let mut w = FakeWorker::new();
        let (id, invite) = room(&mut w);
        let mut ada = device(1, "Ada", &id, Some(invite));
        ada.room.join();
        let first = ada.room.next_call(&ada.me, 1000, 0).unwrap();
        assert_eq!(first.what, CallKind::Join);
        assert!(first.signed);
        assert_eq!(
            first.url("http://meet.test"),
            format!("http://meet.test/rooms/{id}/members/{}", ada.me.device())
        );
        assert!(
            ada.room.next_call(&ada.me, 2000, 0).is_none(),
            "one at a time"
        );
        let again = ada.room.next_call(&ada.me, 1000 + STUCK_MS, 0).unwrap();
        assert_eq!(
            again.what,
            CallKind::Join,
            "the stuck one given up, sent again"
        );
        // An answer to the request given up on is not taken.
        let late = ada
            .room
            .on_answer(&ada.me, &CallKind::Sync, Some(200), "{}", 1000 + STUCK_MS);
        assert_eq!(late, Changes::default());
    }
}
