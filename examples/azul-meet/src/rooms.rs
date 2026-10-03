//! The pure logic of AzMeet rooms, free of azul types so `cargo test -p AzMeet` checks it without
//! a window: reading a meeting link, choosing which side of a pair dials, diffing the peers list
//! between two polls, planning the dials, and choosing the iroh relays.
//!
//! The meeting server is the `meet` Worker (azul-apps `cf-workers/meet`). It mints a room id
//! (the credential) and a short code, and holds each participant's iroh ticket.

use std::collections::{BTreeMap, BTreeSet};

/// Prefix of the link a meeting is shared by.
pub const APP_LINK_PREFIX: &str = "azlin://meet/";

/// Polls without a connection after which an unanswered dial is tried again.
pub const REDIAL_AFTER_POLLS: u32 = 5;

/// The alphabet of room ids (lower-case Crockford base32), as the meet Worker mints them.
const ID_ALPHABET: &str = "0123456789abcdefghjkmnpqrstvwxyz";
const ID_LEN: usize = 26;
/// The alphabet of room codes: no `0`, `1`, `i`, `l` or `o`.
const CODE_ALPHABET: &str = "23456789abcdefghjkmnpqrstuvwxyz";
const CODE_LEN: usize = 9;

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
pub fn parse_room_link(input: &str) -> Option<RoomKey> {
    let s = input.trim();
    let s = s.split(|c: char| c == '?' || c == '#').next().unwrap_or("");
    let s = s.trim_end_matches('/');
    if let Some(key) = strip_prefix_ignore_case(s, APP_LINK_PREFIX) {
        return parse_room_key(key);
    }
    let Some((scheme, rest)) = s.split_once("://") else {
        return parse_room_key(s);
    };
    if !scheme.eq_ignore_ascii_case("https") && !scheme.eq_ignore_ascii_case("http") {
        return None;
    }
    // Everything after the host: the landing page is `/rooms/<key>`, under any base path.
    let segments: Vec<&str> = rest.split('/').skip(1).filter(|p| !p.is_empty()).collect();
    match segments.as_slice() {
        [.., rooms, key] if rooms.eq_ignore_ascii_case("rooms") => parse_room_key(key),
        _ => None,
    }
}

fn strip_prefix_ignore_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &s[prefix.len()..])
}

/// Reads a room id or a room code in any case; a code may drop its dashes.
pub fn parse_room_key(key: &str) -> Option<RoomKey> {
    let key = key.trim().to_ascii_lowercase();
    if key.len() == ID_LEN && key.chars().all(|c| ID_ALPHABET.contains(c)) {
        return Some(RoomKey::Id(key));
    }
    let code: String = key
        .chars()
        .filter(|c| *c != '-' && !c.is_whitespace())
        .collect();
    if code.len() == CODE_LEN && code.chars().all(|c| CODE_ALPHABET.contains(c)) {
        return Some(RoomKey::Code(format!(
            "{}-{}-{}",
            &code[..3],
            &code[3..6],
            &code[6..]
        )));
    }
    None
}

/// Whether this endpoint dials `other`. The lower endpoint id dials and the higher one waits, so
/// two peers that find each other in the same poll open one connection, not two.
pub fn dials(me: &str, other: &str) -> bool {
    !me.is_empty() && me < other
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
pub fn diff_peers(before: &[PeerRecord], after: &[PeerRecord], me: &str) -> PeerDiff {
    let earlier: BTreeMap<&str, &PeerRecord> = before
        .iter()
        .filter(|p| p.node_id != me)
        .map(|p| (p.node_id.as_str(), p))
        .collect();
    let mut diff = PeerDiff::default();
    let mut listed: BTreeSet<&str> = BTreeSet::new();
    for p in after.iter().filter(|p| p.node_id != me) {
        if !listed.insert(p.node_id.as_str()) {
            continue;
        }
        match earlier.get(p.node_id.as_str()) {
            None => diff.joined.push(p.clone()),
            Some(prev) if prev.ticket != p.ticket => diff.moved.push(p.clone()),
            Some(prev) if prev.name != p.name => diff.renamed.push(p.clone()),
            Some(_) => {}
        }
    }
    let mut gone: BTreeSet<&str> = BTreeSet::new();
    for p in before.iter().filter(|p| p.node_id != me) {
        if !listed.contains(p.node_id.as_str()) && gone.insert(p.node_id.as_str()) {
            diff.left.push(p.clone());
        }
    }
    diff
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
    me: &str,
    peers: &[PeerRecord],
    connected: &BTreeSet<String>,
    dialed: &BTreeMap<String, Dialed>,
    poll: u32,
) -> Vec<PeerRecord> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut plan = Vec::new();
    for p in peers {
        if !seen.insert(p.node_id.as_str())
            || !dials(me, &p.node_id)
            || connected.contains(&p.node_id)
        {
            continue;
        }
        let due = match dialed.get(&p.node_id) {
            None => true,
            Some(d) => d.ticket != p.ticket || poll.wrapping_sub(d.at_poll) >= REDIAL_AFTER_POLLS,
        };
        if due {
            plan.push(p.clone());
        }
    }
    plan
}

/// Whether `host` (as a URL spells it, IPv6 in brackets or not) is this machine.
pub fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
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
pub fn relay_choice(setting: Option<&str>, worker_host: &str) -> Relay {
    match setting.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) if s.eq_ignore_ascii_case("off") || s == "0" => Relay::Off,
        Some(s) if s.eq_ignore_ascii_case("default") || s == "1" => Relay::Default,
        Some(url) => Relay::Custom(url.to_string()),
        None if is_loopback_host(worker_host) => Relay::Off,
        None => Relay::Default,
    }
}

/// The meeting server when none was saved, `AZMEET_WORKER` is not set and none is built in: the
/// local mock (`cf-workers/meet/dev-server.mjs`).
pub const LOCAL_WORKER: &str = "http://127.0.0.1:8787";
/// A settings file longer than this is not read.
pub const MAX_SETTINGS_BYTES: usize = 4096;
/// The longest meeting server address taken.
const MAX_SERVER_CHARS: usize = 2048;
/// The settings file's line naming the meeting server.
const SETTINGS_KEY: &str = "meeting_server=";

/// Where the meeting server's address at start came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerSource {
    /// Saved the last time it answered (the start screen's field).
    Saved,
    /// `AZMEET_WORKER`.
    Environment,
    /// Built in: `PRODUCTION_WORKER`, else [`LOCAL_WORKER`].
    BuiltIn,
}

/// A meeting server address as typed: trimmed, without trailing slashes. `None` unless it is an
/// http or https address with a host, printable ASCII without spaces, a query or a fragment, and at
/// most 2048 characters.
pub fn normalize_server(input: &str) -> Option<String> {
    let s = input.trim().trim_end_matches('/');
    if s.is_empty() || s.len() > MAX_SERVER_CHARS || !s.bytes().all(|b| b.is_ascii_graphic()) {
        return None;
    }
    if s.contains(|c: char| c == '?' || c == '#') {
        return None;
    }
    let rest = strip_prefix_ignore_case(s, "https://")
        .or_else(|| strip_prefix_ignore_case(s, "http://"))?;
    let host = rest.split('/').next().unwrap_or("");
    if host.is_empty() || host.starts_with(':') {
        return None;
    }
    Some(s.to_string())
}

/// The meeting server the start screen's field shows at start, and where it came from: the one
/// saved last time, else `AZMEET_WORKER`, else `built_in` (else [`LOCAL_WORKER`]). A candidate that
/// is no meeting server address is passed over.
pub fn server_prefill(
    saved: Option<&str>,
    env: Option<&str>,
    built_in: &str,
) -> (String, ServerSource) {
    if let Some(server) = saved.and_then(normalize_server) {
        return (server, ServerSource::Saved);
    }
    if let Some(server) = env.and_then(normalize_server) {
        return (server, ServerSource::Environment);
    }
    let server = normalize_server(built_in).unwrap_or_else(|| LOCAL_WORKER.to_string());
    (server, ServerSource::BuiltIn)
}

/// Whether AzMeet opens its in-process demo instead of the start screen: only when nothing was
/// configured (the built-in default) and nothing answers there. A saved or configured server that
/// does not answer gets the start screen, which says so next to the field.
pub fn opens_demo(source: ServerSource, answers: bool) -> bool {
    source == ServerSource::BuiltIn && !answers
}

/// The settings file's text remembering `server`. AzMeet keeps its own settings in
/// `meet/settings.json`; AzCalendar (which compiles this file in as `meet_rooms`) still stores its
/// meeting server this way - keep these while it does.
pub fn encode_settings(server: &str) -> String {
    format!("{SETTINGS_KEY}{server}\n")
}

/// The meeting server a settings file's text remembers; `None` for a file that is too long, has no
/// such line, or names no meeting server address. Other lines are ignored.
pub fn decode_settings(text: &str) -> Option<String> {
    if text.len() > MAX_SETTINGS_BYTES {
        return None;
    }
    text.lines()
        .filter_map(|line| line.trim().strip_prefix(SETTINGS_KEY))
        .last()
        .and_then(normalize_server)
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
    fn a_local_meeting_server_means_no_relays_unless_asked() {
        assert_eq!(relay_choice(None, "127.0.0.1"), Relay::Off);
        assert_eq!(relay_choice(None, "localhost"), Relay::Off);
        assert_eq!(relay_choice(Some(" "), "::1"), Relay::Off);
        assert_eq!(relay_choice(None, "[::1]"), Relay::Off);
        assert_eq!(relay_choice(None, "meet.example.com"), Relay::Default);
        assert_eq!(relay_choice(Some("off"), "meet.example.com"), Relay::Off);
        assert_eq!(relay_choice(Some("default"), "127.0.0.1"), Relay::Default);
        assert_eq!(
            relay_choice(Some("https://relay.example.com"), "127.0.0.1"),
            Relay::Custom("https://relay.example.com".to_string())
        );
    }

    #[test]
    fn the_meeting_server_is_the_saved_one_else_azmeet_worker_else_the_built_in_default() {
        let saved = Some("https://meet.example.com/");
        let env = Some("http://127.0.0.1:9999");
        assert_eq!(
            server_prefill(saved, env, "https://built.in"),
            (
                String::from("https://meet.example.com"),
                ServerSource::Saved
            )
        );
        assert_eq!(
            server_prefill(None, env, "https://built.in"),
            (
                String::from("http://127.0.0.1:9999"),
                ServerSource::Environment
            )
        );
        assert_eq!(
            server_prefill(None, None, "https://built.in"),
            (String::from("https://built.in"), ServerSource::BuiltIn)
        );
        assert_eq!(
            server_prefill(None, None, ""),
            (String::from(LOCAL_WORKER), ServerSource::BuiltIn)
        );
    }

    #[test]
    fn a_saved_or_configured_server_that_is_no_address_is_passed_over() {
        assert_eq!(
            server_prefill(
                Some("  "),
                Some("ftp://meet.example.com"),
                "https://built.in"
            ),
            (String::from("https://built.in"), ServerSource::BuiltIn)
        );
        assert_eq!(
            server_prefill(Some("not an address"), Some(" http://localhost:8787/ "), ""),
            (
                String::from("http://localhost:8787"),
                ServerSource::Environment
            )
        );
    }

    #[test]
    fn a_meeting_server_address_is_trimmed_and_needs_http_or_https_and_a_host() {
        assert_eq!(
            normalize_server(" https://meet.example.com// "),
            Some(String::from("https://meet.example.com"))
        );
        assert_eq!(
            normalize_server("HTTP://127.0.0.1:8787/base/"),
            Some(String::from("HTTP://127.0.0.1:8787/base"))
        );
        let long = format!("https://{}.com", "a".repeat(3000));
        for bad in [
            "",
            "meet.example.com",
            "ftp://meet.example.com",
            "https://",
            "http:///rooms",
            "https://:8787",
            "https://meet example.com",
            "https://meet.example.com/?room=1",
            "https://meet.example.com/#x",
            long.as_str(),
        ] {
            assert_eq!(normalize_server(bad), None, "{bad}");
        }
    }

    #[test]
    fn only_a_built_in_server_that_does_not_answer_opens_the_demo() {
        assert!(opens_demo(ServerSource::BuiltIn, false));
        assert!(!opens_demo(ServerSource::BuiltIn, true));
        assert!(!opens_demo(ServerSource::Saved, false));
        assert!(!opens_demo(ServerSource::Environment, false));
    }

    #[test]
    fn the_settings_file_keeps_the_meeting_server_and_refuses_what_it_cannot_trust() {
        let text = encode_settings("https://meet.example.com");
        assert_eq!(text, "meeting_server=https://meet.example.com\n");
        assert_eq!(
            decode_settings(&text),
            Some(String::from("https://meet.example.com"))
        );
        assert_eq!(
            decode_settings("# AzMeet\nother=1\n  meeting_server=http://a.b  \n"),
            Some(String::from("http://a.b"))
        );
        assert_eq!(decode_settings(""), None);
        assert_eq!(decode_settings("meeting_server=javascript:alert(1)"), None);
        let padded = format!(
            "meeting_server=https://a.b\n{}",
            "x".repeat(MAX_SETTINGS_BYTES)
        );
        assert_eq!(decode_settings(&padded), None, "a file that is too long");
    }
}
