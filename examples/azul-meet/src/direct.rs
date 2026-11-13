//! Direct calls: one AzMeet reaches another by its iroh address, with no meetings service at any
//! point (ARCHITECTURE.md, "Direct calls").
//!
//! The caller dials the other computer's address (its full ticket, or its bare endpoint id, found
//! through the relay) and, once connected, says hello: its name and its own ticket. The callee
//! never takes the call by itself: it shows a prompt with the caller's name and four safety words
//! made from both endpoint ids, which the two people read to each other, and answers Accept or
//! Decline. iroh's connection is end-to-end encrypted and authenticated by both endpoint ids, so
//! the call needs no room key and nothing from a server.
//!
//! More than two: whoever takes a newcomer into a call hands it the roster (everyone else's id,
//! name and ticket) and tells the others to expect it; the newcomer dials each, and a connection
//! from an expected id joins without a prompt.
//!
//! On the wire every direct-call message is a reliable message of kind [`KIND_DIRECT`]: the kind
//! byte, a version byte, then JSON. This module is the pure part: the messages, the safety words,
//! the address forms and the recent calls. `lib.rs` wires them to the endpoint and the window.

use serde::{Deserialize, Serialize};

/// Kind byte of a direct-call message (1 is the audio state, 2 to 5 video, 6 the sync, 7 the
/// relay envelope, 8 is kept for chat records).
pub const KIND_DIRECT: u8 = 9;
/// Version byte of a direct-call message.
pub const DIRECT_VERSION: u8 = 1;
/// How many recent direct calls are kept.
pub const RECENTS: usize = 10;
/// How long an outgoing call rings before it is given up, ms.
pub const RING_MS: u64 = 45_000;

/// One person of a direct call as the roster names them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterPeer {
    /// The endpoint id: how a connection is recognised.
    pub id: String,
    /// The name they gave.
    pub name: String,
    /// Their ticket: how a newcomer dials them.
    pub ticket: String,
}

/// A direct-call message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum DirectMsg {
    /// The caller's first message on the connection it dialed: its name and its ticket.
    Hello { name: String, ticket: String },
    /// The callee took the call: its name and its ticket.
    Accept { name: String, ticket: String },
    /// The callee said no; `busy`: it is in a meeting.
    Decline { busy: bool },
    /// To a newcomer: everyone else in the call. It dials each of them.
    Roster { peers: Vec<RosterPeer> },
    /// To everyone else: a newcomer to expect. Its connection joins without a prompt.
    Expect { peer: RosterPeer },
}

/// The bytes of `msg` on the wire.
#[must_use]
pub fn encode(msg: &DirectMsg) -> Vec<u8> {
    let mut bytes = vec![KIND_DIRECT, DIRECT_VERSION];
    bytes.extend(serde_json::to_vec(msg).unwrap_or_default());
    bytes
}

/// A direct-call message read back; `None` for any other kind, another version, or bad JSON.
#[must_use]
pub fn decode(bytes: &[u8]) -> Option<DirectMsg> {
    match bytes {
        [KIND_DIRECT, DIRECT_VERSION, json @ ..] => serde_json::from_slice(json).ok(),
        _ => None,
    }
}

/// The words the safety words are taken from: 256, one per byte, short and easy to say.
pub const WORDS: [&str; 256] = [
    "acorn", "actor", "adobe", "agent", "alarm", "album", "alder", "alpha", "amber", "anchor",
    "angle", "apple", "apron", "arch", "arena", "arrow", "aspen", "atlas", "attic", "autumn",
    "badge", "bagel", "baker", "bamboo", "banjo", "barley", "barn", "basil", "basket", "beach",
    "beacon", "bean", "bear", "beaver", "bell", "berry", "birch", "bison", "blade", "blanket",
    "bloom", "blue", "board", "boat", "bonnet", "bottle", "boulder", "bow", "bread", "brick",
    "bridge", "brook", "broom", "bubble", "bucket", "buffalo", "bugle", "butter", "button",
    "cabin", "cable", "cactus", "cake", "camel", "camera", "canal", "candle", "canoe", "canyon",
    "cape", "carbon", "cargo", "carpet", "carrot", "castle", "cedar", "cello", "chalk", "chapel",
    "cherry", "chess", "chimney", "cider", "cinema", "circus", "citrus", "clay", "cliff", "clock",
    "cloud", "clover", "coast", "cobalt", "cocoa", "comet", "compass", "copper", "coral", "cotton",
    "cougar", "crane", "crater", "crayon", "cricket", "crown", "crystal", "cup", "daisy", "delta",
    "desert", "diamond", "dolphin", "domino", "donkey", "dove", "dragon", "drum", "eagle", "easel",
    "echo", "eclipse", "elbow", "ember", "emerald", "engine", "falcon", "feather", "fern",
    "fiddle", "field", "fig", "flag", "flame", "flint", "flute", "forest", "fossil", "fountain",
    "fox", "galaxy", "garden", "garlic", "gazelle", "geyser", "ginger", "glacier", "globe",
    "goose", "granite", "grape", "gravel", "guitar", "hammer", "harbor", "harp", "hazel", "heron",
    "hill", "honey", "horizon", "hornet", "iceberg", "igloo", "island", "ivory", "jacket", "jade",
    "jaguar", "jasmine", "jelly", "jungle", "kayak", "kettle", "kite", "koala", "ladder", "lagoon",
    "lake", "lantern", "lemon", "lily", "linen", "lion", "lizard", "llama", "lotus", "magnet",
    "mango", "maple", "marble", "meadow", "melon", "meteor", "mint", "mirror", "moose", "mosaic",
    "moss", "motor", "mountain", "mule", "nectar", "needle", "nest", "nickel", "nutmeg", "oak",
    "oasis", "ocean", "olive", "onion", "opal", "orange", "orbit", "orchid", "otter", "owl",
    "paddle", "palm", "panda", "paper", "parrot", "peach", "pearl", "pebble", "pelican", "pepper",
    "piano", "pigeon", "pillow", "pine", "planet", "plum", "pony", "poppy", "prism", "pumpkin",
    "puzzle", "quail", "quartz", "quill", "rabbit", "radar", "radish", "raft", "rain", "raven",
    "reef", "ribbon", "ridge", "river", "robin", "rocket", "rose", "ruby", "saddle",
];

/// The safety words of a direct call between endpoints `a` and `b`: four words from a hash of
/// both ids, the same on both sides whoever called.
#[must_use]
pub fn safety_words(a: &str, b: &str) -> [&'static str; 4] {
    let (low, high) = if a <= b { (a, b) } else { (b, a) };
    let digest = crate::crypto::sha256(format!("azmeet/v1/direct-call\n{low}\n{high}").as_bytes());
    [
        WORDS[usize::from(digest[0])],
        WORDS[usize::from(digest[1])],
        WORDS[usize::from(digest[2])],
        WORDS[usize::from(digest[3])],
    ]
}

/// The safety words as one line: "maple orbit quill salmon".
#[must_use]
pub fn words_line(words: [&str; 4]) -> String {
    words.join(" ")
}

/// Which form of this computer's address the dialog shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AddressForm {
    /// The full ticket: the endpoint id, the relay and the direct addresses. Faster to reach,
    /// but it shows this computer's IP addresses to whoever has it.
    #[default]
    WithAddresses,
    /// The bare endpoint id: found through the relay, it shows no IP address.
    IdOnly,
}

/// This computer's address in `form`, from its `ticket` and its endpoint `id`.
#[must_use]
pub fn address(form: AddressForm, ticket: &str, id: &str) -> String {
    match form {
        AddressForm::WithAddresses if !ticket.is_empty() => ticket.to_string(),
        _ => id.to_string(),
    }
}

/// Whether `text` is a bare endpoint id (64 hex digits), which names the endpoint a connection
/// will come from.
#[must_use]
pub fn is_bare_id(text: &str) -> bool {
    let text = text.trim();
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A direct call kept in the list of recent calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recent {
    /// The other person's name.
    pub name: String,
    /// The address to call them again: their ticket.
    pub address: String,
    /// When, seconds since 1970.
    pub at: i64,
    /// This side called (else it was called).
    pub outgoing: bool,
}

/// Puts `call` first in `recents`: an earlier call to the same address goes, at most
/// [`RECENTS`] stay.
pub fn remember(recents: &mut Vec<Recent>, call: Recent) {
    recents.retain(|r| r.address != call.address);
    recents.insert(0, call);
    recents.truncate(RECENTS);
}

/// The recent calls as the settings keep them (JSON).
#[must_use]
pub fn recents_to_text(recents: &[Recent]) -> String {
    serde_json::to_string(recents).unwrap_or_default()
}

/// The recent calls read back; nothing for text that does not read.
#[must_use]
pub fn recents_from_text(text: &str) -> Vec<Recent> {
    serde_json::from_str(text).unwrap_or_default()
}

/// How long a connection that came in may wait for its hello before it is dropped, ms.
pub const HELLO_MS: u64 = 10_000;

/// A call this side is making: dialed, not taken yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    /// What was dialed: a ticket, or a bare endpoint id.
    pub address: String,
    /// The connection it came up on, once it did.
    pub handle: Option<u64>,
    /// The callee's endpoint id: known at once from a bare id, else from the connection.
    pub id: Option<String>,
    /// The safety words, once connected; empty before.
    pub words: String,
    /// When it was dialed (the window's clock, ms): it rings [`RING_MS`] at most.
    pub since_ms: u64,
}

/// A call coming in, shown as a prompt until Accept or Decline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incoming {
    /// The connection it came on.
    pub handle: u64,
    /// The caller's endpoint id.
    pub id: String,
    /// The name the caller gave.
    pub name: String,
    /// The caller's ticket: how a newcomer to the call dials it, and the recent call's address.
    pub ticket: String,
    /// The safety words of both ids.
    pub words: String,
}

/// A connection that came in while this side was in no meeting: a caller once it says hello,
/// dropped after [`HELLO_MS`] without one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Knock {
    pub handle: u64,
    pub id: String,
    pub since_ms: u64,
}

/// This device's direct calls.
#[derive(Debug, Clone, Default)]
pub struct DirectState {
    /// The direct-call screen shows (Settings > Meetings > "Direct call...").
    pub open: bool,
    /// Which form of this computer's address the screen shows.
    pub form: AddressForm,
    /// The Call field as typed.
    pub call_text: String,
    /// "Copy" was clicked for the address shown.
    pub copied: bool,
    /// The call this side is making.
    pub outgoing: Option<Outgoing>,
    /// The call coming in, prompting.
    pub incoming: Option<Incoming>,
    /// Connections waiting for their hello.
    pub knocks: Vec<Knock>,
    /// In a direct call (the call screen shows it).
    pub in_call: bool,
    /// The others in the call: their connection and who they are.
    pub people: Vec<(u64, RosterPeer)>,
    /// Who the call expects (the roster): a connection from one of these ids joins at once.
    pub expected: Vec<RosterPeer>,
    /// The safety words of the call with the first person in it, for the header.
    pub words: String,
    /// What the last call came to, under the Call field; empty for nothing.
    pub note: String,
    /// The recent calls, newest first.
    pub recents: Vec<Recent>,
}

impl DirectState {
    /// No call, the recent calls `recents`.
    #[must_use]
    pub fn new(recents: Vec<Recent>) -> Self {
        DirectState {
            recents,
            ..DirectState::default()
        }
    }

    /// The name of the person with endpoint id `id` in (or expected by) the call.
    #[must_use]
    pub fn name_of(&self, id: &str) -> Option<String> {
        self.people
            .iter()
            .map(|(_, p)| p)
            .chain(self.expected.iter())
            .find(|p| p.id.eq_ignore_ascii_case(id))
            .map(|p| p.name.clone())
    }

    /// Whether connection `handle` belongs to someone in the call.
    #[must_use]
    pub fn in_call_with(&self, handle: u64) -> bool {
        self.people.iter().any(|(h, _)| *h == handle)
    }

    /// The call is over: nobody in it, nobody expected.
    pub fn end_call(&mut self) {
        self.in_call = false;
        self.people.clear();
        self.expected.clear();
        self.words.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADA: &str = "1f2e3d4c5b6a79881f2e3d4c5b6a79881f2e3d4c5b6a79881f2e3d4c5b6a7988";
    const BEN: &str = "a0b1c2d3e4f5a6b7a0b1c2d3e4f5a6b7a0b1c2d3e4f5a6b7a0b1c2d3e4f5a6b7";

    #[test]
    fn a_direct_call_message_reads_back_as_sent() {
        let peer = RosterPeer { id: BEN.into(), name: "Ben".into(), ticket: "endpointabc".into() };
        for msg in [
            DirectMsg::Hello { name: "Ada".into(), ticket: "endpointxyz".into() },
            DirectMsg::Accept { name: "Ben".into(), ticket: "endpointabc".into() },
            DirectMsg::Decline { busy: true },
            DirectMsg::Roster { peers: vec![peer.clone()] },
            DirectMsg::Expect { peer },
        ] {
            let bytes = encode(&msg);
            assert_eq!(&bytes[..2], &[KIND_DIRECT, DIRECT_VERSION]);
            assert_eq!(decode(&bytes), Some(msg));
        }
    }

    #[test]
    fn another_kind_or_version_is_no_direct_call_message() {
        let mut bytes = encode(&DirectMsg::Decline { busy: false });
        assert!(decode(&bytes).is_some());
        bytes[1] = DIRECT_VERSION + 1;
        assert_eq!(decode(&bytes), None);
        assert_eq!(decode(&[1, 0]), None, "the audio state");
        assert_eq!(decode(&[KIND_DIRECT, DIRECT_VERSION, b'{']), None, "bad JSON");
    }

    #[test]
    fn both_sides_see_the_same_four_safety_words_whoever_called() {
        let ada_sees = safety_words(ADA, BEN);
        let ben_sees = safety_words(BEN, ADA);
        assert_eq!(ada_sees, ben_sees);
        assert!(ada_sees.iter().all(|w| WORDS.contains(w)), "{ada_sees:?}");
        assert_ne!(safety_words(ADA, BEN), safety_words(ADA, &BEN.replace('a', "b")), "another id, other words");
    }

    #[test]
    fn the_safety_words_are_256_different_plain_words() {
        let mut seen = std::collections::BTreeSet::new();
        for w in WORDS {
            assert!(!w.is_empty() && w.bytes().all(|b| b.is_ascii_lowercase()), "{w}");
            assert!(seen.insert(w), "{w} twice");
        }
        assert_eq!(words_line(["maple", "orbit", "quill", "salmon"]), "maple orbit quill salmon");
    }

    #[test]
    fn the_id_only_address_shows_no_ip_address() {
        assert_eq!(address(AddressForm::WithAddresses, "endpointxyz", ADA), "endpointxyz");
        assert_eq!(address(AddressForm::IdOnly, "endpointxyz", ADA), ADA);
        assert_eq!(address(AddressForm::WithAddresses, "", ADA), ADA, "no ticket yet: the id");
        assert!(is_bare_id(ADA) && !is_bare_id("endpointxyz"));
    }

    #[test]
    fn a_direct_call_knows_its_people_by_id_and_forgets_them_when_it_ends() {
        let mut state = DirectState::new(Vec::new());
        state.in_call = true;
        state.people.push((7, RosterPeer { id: BEN.into(), name: "Ben".into(), ticket: "b".into() }));
        state.expected.push(RosterPeer { id: ADA.into(), name: "Ada".into(), ticket: "a".into() });
        assert_eq!(state.name_of(&BEN.to_uppercase()).as_deref(), Some("Ben"));
        assert_eq!(state.name_of(ADA).as_deref(), Some("Ada"), "an expected newcomer's name is known");
        assert!(state.in_call_with(7) && !state.in_call_with(8));
        state.end_call();
        assert!(!state.in_call && state.people.is_empty() && state.expected.is_empty());
        assert_eq!(state.name_of(BEN), None);
    }

    #[test]
    fn a_recent_call_goes_first_once_and_the_list_stays_short() {
        let call = |name: &str, address: &str, at: i64| Recent { name: name.into(), address: address.into(), at, outgoing: true };
        let mut recents = Vec::new();
        remember(&mut recents, call("Ben", "b", 1));
        remember(&mut recents, call("Cleo", "c", 2));
        remember(&mut recents, call("Ben", "b", 3));
        assert_eq!(recents.iter().map(|r| (r.name.as_str(), r.at)).collect::<Vec<_>>(), vec![("Ben", 3), ("Cleo", 2)]);
        for i in 0..20 {
            remember(&mut recents, call("X", &format!("x{i}"), 10 + i));
        }
        assert_eq!(recents.len(), RECENTS);
        assert_eq!(recents_from_text(&recents_to_text(&recents)), recents);
        assert!(recents_from_text("not json").is_empty());
    }
}
