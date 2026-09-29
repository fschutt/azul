//! The pure logic of AzMeet rooms, free of azul types so `cargo test -p AzMeet` checks it without
//! a window: reading a meeting link, choosing which side of a pair dials, diffing the peers list
//! between two polls, planning the dials, and reading the meeting server's address.
//!
//! The meeting server is the `meet` Worker (azul-apps `cf-workers/meet`). It mints a room id
//! (the credential) and a short code, and holds each participant's iroh ticket.

use std::collections::{BTreeMap, BTreeSet};

/// Prefix of the link a meeting is shared by.
pub const APP_LINK_PREFIX: &str = "azlin://meet/";

/// Polls without a connection after which an unanswered dial is tried again.
pub const REDIAL_AFTER_POLLS: u32 = 5;

/// What a meeting link names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomKey {
    /// The room id: 26 characters, the credential the peers API takes.
    Id(String),
    /// The short code ("xq4-8kd-2nm"), which the meeting server resolves to the id.
    Code(String),
}

impl RoomKey {
    /// The path segment the meeting server looks the room up by.
    pub fn as_str(&self) -> &str {
        match self {
            RoomKey::Id(s) | RoomKey::Code(s) => s,
        }
    }
}

/// Reads `azlin://meet/<key>`, the landing page `http(s)://<host>/rooms/<key>`, or a bare key,
/// where the key is a room id or a room code.
pub fn parse_room_link(_input: &str) -> Option<RoomKey> {
    None
}

/// Reads a room id or a room code in any case; a code may drop its dashes.
pub fn parse_room_key(_key: &str) -> Option<RoomKey> {
    None
}

/// Whether this endpoint dials `other`. The lower endpoint id dials and the higher one waits, so
/// two peers that find each other in the same poll open one connection, not two.
pub fn dials(_me: &str, _other: &str) -> bool {
    false
}

/// One participant as the meeting server lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRecord {
    pub node_id: String,
    pub ticket: String,
    pub name: String,
}

/// How the peers list changed between two polls.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerDiff {
    /// Listed now and not before.
    pub joined: Vec<PeerRecord>,
    /// Listed both times with a different ticket: the peer restarted or moved.
    pub moved: Vec<PeerRecord>,
    /// Listed both times with the same ticket and a different name.
    pub renamed: Vec<PeerRecord>,
    /// Listed before and not now: left, or its record expired.
    pub left: Vec<PeerRecord>,
}

impl PeerDiff {
    pub fn is_empty(&self) -> bool {
        self.joined.is_empty()
            && self.moved.is_empty()
            && self.renamed.is_empty()
            && self.left.is_empty()
    }
}

/// The change from `before` to `after`, leaving out this endpoint (`me`) and repeated entries.
pub fn diff_peers(_before: &[PeerRecord], _after: &[PeerRecord], _me: &str) -> PeerDiff {
    PeerDiff::default()
}

/// A dial this endpoint made: to which ticket, and in which poll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialed {
    pub ticket: String,
    pub at_poll: u32,
}

/// The peers to dial after poll number `poll`: those this endpoint dials (see [`dials`]) that are
/// not `connected`, and that were never dialed, moved to a new ticket since, or have not answered
/// for [`REDIAL_AFTER_POLLS`] polls.
pub fn plan_dials(
    _me: &str,
    _peers: &[PeerRecord],
    _connected: &BTreeSet<String>,
    _dialed: &BTreeMap<String, Dialed>,
    _poll: u32,
) -> Vec<PeerRecord> {
    Vec::new()
}

/// Host and port of an `http://` or `https://` address (80 and 443 by default).
pub fn host_port(_url: &str) -> Option<(String, u16)> {
    None
}

/// Whether `host` is this machine.
pub fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host == "::1" || host.starts_with("127.")
}

/// Where the iroh endpoint may relay when no direct path exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Relay {
    Off,
    Default,
    Custom(String),
}

/// The relay setting from `AZMEET_RELAY` ("off", "default" or a relay URL). Unset, a meeting
/// server on this machine means a local test, so no relays; any other means the public relays,
/// since the participants are on different networks.
pub fn relay_choice(_setting: Option<&str>, _worker_host: &str) -> Relay {
    Relay::Off
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "a2h859hyqkfaa11nhzxfh3gd7f";
    const CODE: &str = "xq4-8kd-2nm";

    fn id() -> Option<RoomKey> {
        Some(RoomKey::Id(ID.to_string()))
    }

    fn code() -> Option<RoomKey> {
        Some(RoomKey::Code(CODE.to_string()))
    }

    fn peer(node_id: &str, ticket: &str, name: &str) -> PeerRecord {
        PeerRecord {
            node_id: node_id.to_string(),
            ticket: ticket.to_string(),
            name: name.to_string(),
        }
    }

    #[test]
    fn an_app_link_names_the_room() {
        assert_eq!(parse_room_link(&format!("azlin://meet/{ID}")), id());
        assert_eq!(parse_room_link(&format!("AZLIN://meet/{ID}/")), id());
        assert_eq!(parse_room_link(&format!("azlin://meet/{CODE}")), code());
    }

    #[test]
    fn the_landing_page_address_names_the_room() {
        assert_eq!(
            parse_room_link(&format!("https://meet.example.com/rooms/{ID}")),
            id()
        );
        assert_eq!(
            parse_room_link(&format!("https://meet.example.com/rooms/{ID}/")),
            id()
        );
        assert_eq!(
            parse_room_link(&format!("https://meet.example.com/rooms/{ID}?x=1#top")),
            id()
        );
        assert_eq!(
            parse_room_link(&format!("http://127.0.0.1:8787/rooms/{ID}")),
            id()
        );
        assert_eq!(
            parse_room_link(&format!("https://example.com/meet/rooms/{CODE}")),
            code()
        );
    }

    #[test]
    fn a_bare_id_or_code_is_read_in_any_case_with_space_around_it() {
        assert_eq!(parse_room_link(&format!("  {ID}\n")), id());
        assert_eq!(parse_room_link(&ID.to_uppercase()), id());
        assert_eq!(parse_room_link("XQ4-8KD-2NM"), code());
        assert_eq!(parse_room_link("xq48kd2nm"), code());
        assert_eq!(parse_room_link("xq4 8kd 2nm"), code());
        assert_eq!(parse_room_key(ID), id());
        assert_eq!(parse_room_key(CODE), code());
    }

    #[test]
    fn anything_else_is_not_a_meeting_link() {
        for input in [
            "",
            "   ",
            "hello",
            "azlin://meet/",
            "azlin://chat/a2h859hyqkfaa11nhzxfh3gd7f",
            "https://meet.example.com/",
            "https://meet.example.com/rooms/",
            "https://meet.example.com/other/a2h859hyqkfaa11nhzxfh3gd7f",
            "ftp://meet.example.com/rooms/a2h859hyqkfaa11nhzxfh3gd7f",
            // `u` is not in the id alphabet, and 25 or 27 characters are no id.
            "a2h859hyqkfaa11nhzxfh3gd7u",
            "a2h859hyqkfaa11nhzxfh3gd7",
            "a2h859hyqkfaa11nhzxfh3gd7ff",
            // 8 or 10 code characters, and `1` / `0` are not in the code alphabet.
            "xq4-8kd-2n",
            "xq4-8kd-2nmm",
            "xq4-8kd-2n1",
            "xq4-8kd-2n0",
        ] {
            assert_eq!(parse_room_link(input), None, "{input:?}");
        }
    }

    #[test]
    fn the_lower_endpoint_id_dials_and_the_higher_one_waits() {
        assert!(dials("0a", "0b"));
        assert!(!dials("0b", "0a"));
        assert!(!dials("0a", "0a"));
        assert!(!dials("", "0a"));
    }

    #[test]
    fn exactly_one_of_two_different_peers_dials() {
        let ids = ["00ff", "0100", "a0", "ab", "abc", "ff"];
        for a in ids {
            for b in ids {
                if a != b {
                    assert!(dials(a, b) != dials(b, a), "{a} {b}");
                }
            }
        }
    }

    #[test]
    fn a_diff_reports_who_joined_moved_was_renamed_and_left() {
        let before = [
            peer("a", "ta", "Ada"),
            peer("b", "tb", "Ben"),
            peer("c", "tc", "Cy"),
        ];
        let after = [
            peer("b", "tb2", "Ben"),
            peer("c", "tc", "Cyrus"),
            peer("d", "td", "Dee"),
        ];
        let diff = diff_peers(&before, &after, "me");
        assert_eq!(diff.joined, vec![peer("d", "td", "Dee")]);
        assert_eq!(diff.moved, vec![peer("b", "tb2", "Ben")]);
        assert_eq!(diff.renamed, vec![peer("c", "tc", "Cyrus")]);
        assert_eq!(diff.left, vec![peer("a", "ta", "Ada")]);
        assert!(!diff.is_empty());
    }

    #[test]
    fn the_same_list_twice_is_no_change() {
        let list = [peer("a", "ta", "Ada"), peer("b", "tb", "Ben")];
        assert!(diff_peers(&list, &list, "me").is_empty());
        assert!(diff_peers(&[], &[], "me").is_empty());
    }

    #[test]
    fn a_diff_leaves_out_this_endpoint_and_repeated_entries() {
        let before = [peer("me", "t0", "Me")];
        let after = [
            peer("me", "t1", "Me"),
            peer("a", "ta", "Ada"),
            peer("a", "ta", "Ada"),
        ];
        let diff = diff_peers(&before, &after, "me");
        assert_eq!(diff.joined, vec![peer("a", "ta", "Ada")]);
        assert!(diff.moved.is_empty());
        assert!(diff.left.is_empty());
        let gone = diff_peers(&after, &before, "me");
        assert_eq!(gone.left, vec![peer("a", "ta", "Ada")]);
        assert!(gone.joined.is_empty());
    }

    #[test]
    fn a_new_peer_with_a_higher_id_is_dialed_once() {
        let peers = [peer("b", "tb", "Ben")];
        let connected = BTreeSet::new();
        let mut dialed = BTreeMap::new();
        assert_eq!(
            plan_dials("a", &peers, &connected, &dialed, 1),
            vec![peer("b", "tb", "Ben")]
        );
        dialed.insert(
            "b".to_string(),
            Dialed {
                ticket: "tb".to_string(),
                at_poll: 1,
            },
        );
        assert!(plan_dials("a", &peers, &connected, &dialed, 2).is_empty());
    }

    #[test]
    fn a_peer_with_a_lower_id_is_left_to_dial_this_one() {
        let peers = [peer("a", "ta", "Ada")];
        assert!(plan_dials("b", &peers, &BTreeSet::new(), &BTreeMap::new(), 1).is_empty());
    }

    #[test]
    fn a_connected_peer_is_not_dialed_again() {
        let peers = [peer("b", "tb", "Ben"), peer("c", "tc", "Cy")];
        let connected: BTreeSet<String> = ["b".to_string()].into_iter().collect();
        assert_eq!(
            plan_dials("a", &peers, &connected, &BTreeMap::new(), 1),
            vec![peer("c", "tc", "Cy")]
        );
    }

    #[test]
    fn a_peer_that_moved_to_a_new_ticket_is_dialed_again_at_once() {
        let mut dialed = BTreeMap::new();
        dialed.insert(
            "b".to_string(),
            Dialed {
                ticket: "old".to_string(),
                at_poll: 3,
            },
        );
        let peers = [peer("b", "new", "Ben")];
        assert_eq!(
            plan_dials("a", &peers, &BTreeSet::new(), &dialed, 4),
            vec![peer("b", "new", "Ben")]
        );
    }

    #[test]
    fn an_unanswered_dial_is_tried_again_after_a_few_polls() {
        let mut dialed = BTreeMap::new();
        dialed.insert(
            "b".to_string(),
            Dialed {
                ticket: "tb".to_string(),
                at_poll: 1,
            },
        );
        let peers = [peer("b", "tb", "Ben")];
        let none = BTreeSet::new();
        assert!(plan_dials("a", &peers, &none, &dialed, REDIAL_AFTER_POLLS).is_empty());
        assert_eq!(
            plan_dials("a", &peers, &none, &dialed, 1 + REDIAL_AFTER_POLLS),
            vec![peer("b", "tb", "Ben")]
        );
    }

    #[test]
    fn a_meeting_server_address_gives_its_host_and_port() {
        let hp = |h: &str, p: u16| Some((h.to_string(), p));
        assert_eq!(host_port("http://127.0.0.1:8787"), hp("127.0.0.1", 8787));
        assert_eq!(host_port("http://127.0.0.1:8787/"), hp("127.0.0.1", 8787));
        assert_eq!(
            host_port("https://meet.example.com"),
            hp("meet.example.com", 443)
        );
        assert_eq!(host_port("HTTP://localhost/rooms"), hp("localhost", 80));
        assert_eq!(host_port("https://[::1]:9000/x"), hp("::1", 9000));
        assert_eq!(host_port("https://[::1]"), hp("::1", 443));
        assert_eq!(host_port("meet.example.com"), None);
        assert_eq!(host_port("ftp://meet.example.com"), None);
        assert_eq!(host_port("http://:80"), None);
        assert_eq!(host_port("http://host:notaport"), None);
    }

    #[test]
    fn a_local_meeting_server_means_no_relays_unless_asked() {
        assert_eq!(relay_choice(None, "127.0.0.1"), Relay::Off);
        assert_eq!(relay_choice(None, "localhost"), Relay::Off);
        assert_eq!(relay_choice(Some(" "), "::1"), Relay::Off);
        assert_eq!(relay_choice(None, "meet.example.com"), Relay::Default);
        assert_eq!(relay_choice(Some("off"), "meet.example.com"), Relay::Off);
        assert_eq!(relay_choice(Some("default"), "127.0.0.1"), Relay::Default);
        assert_eq!(
            relay_choice(Some("https://relay.example.com"), "127.0.0.1"),
            Relay::Custom("https://relay.example.com".to_string())
        );
    }
}
