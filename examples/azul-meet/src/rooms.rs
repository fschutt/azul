//! The pure logic of AzMeet rooms, free of azul types so `cargo test -p AzMeet` checks it without
//! a window: reading a meeting link (and the invite secret in its fragment), choosing which side
//! of a pair dials, diffing the peers list between two polls, planning the dials, choosing the
//! iroh relays and the meeting server, and saying when a meeting is.
//!
//! The meeting server is the `meet` Worker (azul-apps `cf-workers/meet`). AzMeet mints a room id
//! (the credential) and an invite secret itself and registers the room with the Worker, which
//! gives it a short code and holds each participant's signed iroh ticket (CRYPTO.md).

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

/// A meeting link as read: the room it names, and the invite secret of its fragment
/// (`azlin://meet/<room>#<secret>`, CRYPTO.md section 4) when it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomLink {
    pub key: RoomKey,
    /// 26 characters of the id alphabet; `None` for a code, or a link without its fragment (a
    /// knock).
    pub secret: Option<String>,
}

/// The invite secret of a link's fragment: `#<secret>` or `#k=<secret>`, in any case.
fn fragment_secret(fragment: &str) -> Option<String> {
    let f = fragment.trim();
    let f = f.strip_prefix("k=").unwrap_or(f).to_ascii_lowercase();
    (f.len() == ID_LEN && f.chars().all(|c| ID_ALPHABET.contains(c))).then_some(f)
}

/// Reads `azlin://meet/<key>`, the landing page `http(s)://<host>/rooms/<key>`, or a bare key,
/// where the key is a room id or a room code; a `#` fragment is passed over ([`read_room_link`]
/// reads its invite secret). AzCalendar reads its meeting links with it too.
pub fn parse_room_link(input: &str) -> Option<RoomKey> {
    read_room_link(input).map(|link| link.key)
}

/// [`parse_room_link`], with the invite secret of a `#` fragment.
pub fn read_room_link(input: &str) -> Option<RoomLink> {
    let s = input.trim();
    let (s, fragment) = s.split_once('#').unwrap_or((s, ""));
    let key = parse_link_key(s)?;
    let secret = match &key {
        RoomKey::Id(_) => fragment_secret(fragment),
        RoomKey::Code(_) => None,
    };
    Some(RoomLink { key, secret })
}

/// The room a link names, without its fragment.
fn parse_link_key(input: &str) -> Option<RoomKey> {
    let s = input.split('?').next().unwrap_or("");
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
    /// The name: the member's (from its sealed record) once the announcement is verified.
    pub name: String,
    /// The device that signed the announcement, and its signature (CRYPTO.md section 10).
    pub device: Option<String>,
    pub sig: Option<String>,
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

/// How this side's packets may travel, for stdout (`AZMEET_TRANSPORT <label>`) and the
/// statistics: `direct` (no relay), `direct+relay <relays>` (a direct path where one forms, the
/// relay otherwise), `relay-only <relays>` (`--relay-only`: never a direct path), or `none`
/// (relay-only without a relay: nothing can carry a packet).
pub fn transport_label(relay: &Relay, relay_only: bool) -> String {
    let relays = match relay {
        Relay::Off => None,
        Relay::Default => Some("default"),
        Relay::Custom(url) => Some(url.as_str()),
    };
    match (relays, relay_only) {
        (None, false) => String::from("direct"),
        (None, true) => String::from("none"),
        (Some(relays), false) => format!("direct+relay {relays}"),
        (Some(relays), true) => format!("relay-only {relays}"),
    }
}

/// What the meeting server field shows while it is empty: an example, never an address AzMeet
/// would use by itself.
pub const SERVER_PLACEHOLDER: &str = "https://meet.example.com";
/// The longest meeting server address taken.
const MAX_SERVER_CHARS: usize = 2048;

/// Where the meeting server's address at start came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerSource {
    /// The command line's `--worker`: this run only, over the saved one.
    CommandLine,
    /// Saved the last time it answered (the start screen's field).
    Saved,
    /// `AZMEET_WORKER`, or what the shared Azlin config names (AzMeet passes its resolved
    /// address as the `env` layer: the variable, the config file, the profile).
    Environment,
    /// Built in: `PRODUCTION_WORKER` (`AZMEET_DEFAULT_WORKER` at build time).
    BuiltIn,
    /// None anywhere: the start screen asks for one. AzMeet never guesses an address.
    Unset,
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
/// saved last time, else `AZMEET_WORKER`, else `built_in`, else none ([`ServerSource::Unset`],
/// an empty address). A candidate that is no meeting server address is passed over.
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
    match normalize_server(built_in) {
        Some(server) => (server, ServerSource::BuiltIn),
        None => (String::new(), ServerSource::Unset),
    }
}

/// The meeting server at start: the command line's `--worker` (`flag`) for this run, over
/// everything [`server_prefill`] weighs; a `flag` that is no meeting server address is passed
/// over too.
pub fn server_choice(
    flag: Option<&str>,
    saved: Option<&str>,
    env: Option<&str>,
    built_in: &str,
) -> (String, ServerSource) {
    match flag.and_then(normalize_server) {
        Some(server) => (server, ServerSource::CommandLine),
        None => server_prefill(saved, env, built_in),
    }
}

/// The link others join with: the Worker's app link of the room (`azlin://meet/<room>`) with the
/// invite secret as its fragment (CRYPTO.md section 4); the bare link without a secret.
#[must_use]
pub fn link_with_secret(link: &str, secret: Option<&str>) -> String {
    let base = link.split('#').next().unwrap_or(link);
    match secret {
        Some(secret) => format!("{base}#{secret}"),
        None => base.to_string(),
    }
}

/// The polls of `poll_ms` between two announcements when the meeting server keeps a ticket
/// `ttl_secs` (its `peer_ttl_seconds`): a sixth of it, so two lost announcements in a row still
/// keep the ticket listed - 10 polls of 2 s for the Worker's 120 s; at least 1.
#[must_use]
pub fn reannounce_polls(ttl_secs: f64, poll_ms: u64) -> u32 {
    let polls = (ttl_secs * 1000.0 / 6.0 / poll_ms.max(1) as f64).floor();
    if polls.is_finite() && polls >= 1.0 {
        polls.min(f64::from(u32::MAX)) as u32
    } else {
        1
    }
}

/// `starts` to `ends` (seconds since 1970) in the time zone `offset_secs` east of UTC: "Thu 9 Oct
/// 2026, 14:00-15:00", or both dates when it ends on another day.
#[must_use]
pub fn when_text(starts: u64, ends: u64, offset_secs: i32) -> String {
    use chrono::{FixedOffset, TimeZone};
    let Some(zone) = FixedOffset::east_opt(offset_secs) else {
        return String::new();
    };
    let at = |secs: u64| {
        i64::try_from(secs)
            .ok()
            .and_then(|secs| zone.timestamp_opt(secs, 0).single())
    };
    let (Some(a), Some(b)) = (at(starts), at(ends)) else {
        return String::new();
    };
    const DAY: &str = "%a %-d %b %Y, %H:%M";
    if a.date_naive() == b.date_naive() {
        format!("{}-{}", a.format(DAY), b.format("%H:%M"))
    } else {
        format!("{} - {}", a.format(DAY), b.format(DAY))
    }
}

/// A start typed as `YYYY-MM-DD HH:MM` in the time zone `offset_secs` east of UTC, as seconds
/// since 1970; `None` for anything else.
#[must_use]
pub fn parse_local_start(text: &str, offset_secs: i32) -> Option<u64> {
    use chrono::{FixedOffset, NaiveDateTime, TimeZone};
    let naive = NaiveDateTime::parse_from_str(text.trim(), "%Y-%m-%d %H:%M").ok()?;
    let zone = FixedOffset::east_opt(offset_secs)?;
    let at = zone.from_local_datetime(&naive).single()?;
    u64::try_from(at.timestamp()).ok()
}

/// `n` minutes as people say it: "5 min", "2 h", "2 h 5 min", "3 days".
fn span(minutes: u64) -> String {
    match minutes {
        0..=59 => format!("{} min", minutes.max(1)),
        60..=1439 if minutes % 60 == 0 => format!("{} h", minutes / 60),
        60..=1439 => format!("{} h {} min", minutes / 60, minutes % 60),
        _ if minutes / 1440 == 1 => String::from("1 day"),
        _ => format!("{} days", minutes / 1440),
    }
}

/// Where a meeting with these times (seconds since 1970) stands at `now`: "starts in 25 min",
/// "started 5 min ago, ends in 55 min", "ended 2 h ago".
#[must_use]
pub fn meeting_status(starts: u64, ends: u64, now: u64) -> String {
    let minutes = |a: u64, b: u64| a.saturating_sub(b).div_ceil(60);
    if now < starts {
        format!("starts in {}", span(minutes(starts, now)))
    } else if now < ends {
        format!(
            "started {} ago, ends in {}",
            span(minutes(now, starts)),
            span(minutes(ends, now))
        )
    } else {
        format!("ended {} ago", span(minutes(now, ends)))
    }
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
            device: None,
            sig: None,
        }
    }

    /// The room a link names.
    fn key_of(input: &str) -> Option<RoomKey> {
        let key = parse_room_link(input);
        assert_eq!(key, read_room_link(input).map(|l| l.key), "both read the same room");
        key
    }

    const SECRET: &str = "k7qz2m9x4c8v1b6n3r5t0w2y8p";

    #[test]
    fn a_links_fragment_carries_the_invite_secret_and_a_code_carries_none() {
        let link = read_room_link(&format!("azlin://meet/{ID}#{SECRET}")).unwrap();
        assert_eq!(link.key, RoomKey::Id(ID.to_string()));
        assert_eq!(link.secret.as_deref(), Some(SECRET));
        let web = read_room_link(&format!(
            "https://meet.example.com/rooms/{ID}#k={}",
            SECRET.to_uppercase()
        ));
        assert_eq!(web.and_then(|l| l.secret).as_deref(), Some(SECRET), "k= and any case");
        let bare = read_room_link(&format!("azlin://meet/{ID}")).unwrap();
        assert_eq!(bare.secret, None, "a knock");
        let short = read_room_link(&format!("azlin://meet/{ID}#short")).unwrap();
        assert_eq!(short.secret, None);
        let code = read_room_link(&format!("{CODE}#{SECRET}")).unwrap();
        assert_eq!(code.secret, None, "a code has no secret");
        assert_eq!(
            parse_room_link(&format!("azlin://meet/{ID}#{SECRET}")),
            id(),
            "the room alone (AzCalendar's reading)"
        );
        assert_eq!(
            link_with_secret(&format!("azlin://meet/{ID}"), Some(SECRET)),
            format!("azlin://meet/{ID}#{SECRET}")
        );
        assert_eq!(
            link_with_secret(&format!("azlin://meet/{ID}#old"), None),
            format!("azlin://meet/{ID}"),
            "the bare link"
        );
    }

    #[test]
    fn a_meetings_times_read_as_people_say_them() {
        let starts = 1_760_000_000;
        let ends = starts + 3600;
        assert_eq!(meeting_status(starts, ends, starts - 25 * 60), "starts in 25 min");
        assert_eq!(meeting_status(starts, ends, starts - 125 * 60), "starts in 2 h 5 min");
        assert_eq!(meeting_status(starts, ends, starts - 120 * 60), "starts in 2 h");
        assert_eq!(meeting_status(starts, ends, starts - 3 * 86_400), "starts in 3 days");
        assert_eq!(meeting_status(starts, ends, starts - 30), "starts in 1 min", "never 0");
        assert_eq!(meeting_status(starts, ends, starts + 300), "started 5 min ago, ends in 55 min");
        assert_eq!(meeting_status(starts, ends, ends + 7200), "ended 2 h ago");
    }

    /// 2026-10-09T14:00:00Z, a Friday.
    const FRIDAY_14_UTC: u64 = 1_791_554_400;

    #[test]
    fn a_meetings_times_show_in_the_time_zone_and_a_typed_start_reads_back() {
        let t = FRIDAY_14_UTC;
        assert_eq!(when_text(t, t + 3600, 0), "Fri 9 Oct 2026, 14:00-15:00");
        assert_eq!(when_text(t, t + 3600, 7200), "Fri 9 Oct 2026, 16:00-17:00");
        assert_eq!(
            when_text(t, t + 11 * 3600, 7200),
            "Fri 9 Oct 2026, 16:00 - Sat 10 Oct 2026, 03:00",
            "both dates when it ends on another day"
        );
        assert_eq!(parse_local_start("2026-10-09 16:00", 7200), Some(t));
        assert_eq!(parse_local_start(" 2026-10-09 14:00 ", 0), Some(t));
        assert_eq!(parse_local_start("next friday", 0), None);
        assert_eq!(parse_local_start("2026-02-30 10:00", 0), None);
    }

    #[test]
    fn a_ticket_is_announced_again_after_a_sixth_of_the_servers_ttl() {
        assert_eq!(reannounce_polls(120.0, 2000), 10, "the Worker's default: every 20 s");
        assert_eq!(reannounce_polls(30.0, 2000), 2);
        assert_eq!(reannounce_polls(5.0, 2000), 1, "at least every poll");
        assert_eq!(reannounce_polls(f64::NAN, 2000), 1);
    }

    #[test]
    fn an_app_link_names_the_room() {
        assert_eq!(key_of(&format!("azlin://meet/{ID}")), id());
        assert_eq!(key_of(&format!("AZLIN://meet/{ID}/")), id());
        assert_eq!(key_of(&format!("azlin://meet/{CODE}")), code());
    }

    #[test]
    fn the_landing_page_address_names_the_room() {
        assert_eq!(
            key_of(&format!("https://meet.example.com/rooms/{ID}")),
            id()
        );
        assert_eq!(
            key_of(&format!("https://meet.example.com/rooms/{ID}/")),
            id()
        );
        assert_eq!(
            key_of(&format!("https://meet.example.com/rooms/{ID}?x=1#top")),
            id()
        );
        assert_eq!(
            key_of(&format!("http://127.0.0.1:8787/rooms/{ID}")),
            id()
        );
        assert_eq!(
            key_of(&format!("https://example.com/meet/rooms/{CODE}")),
            code()
        );
    }

    #[test]
    fn a_bare_id_or_code_is_read_in_any_case_with_space_around_it() {
        assert_eq!(key_of(&format!("  {ID}\n")), id());
        assert_eq!(key_of(&ID.to_uppercase()), id());
        assert_eq!(key_of("XQ4-8KD-2NM"), code());
        assert_eq!(key_of("xq48kd2nm"), code());
        assert_eq!(key_of("xq4 8kd 2nm"), code());
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

    /// The E2E's relay phase reads `AZMEET_TRANSPORT relay-only http://127.0.0.1:<port>`.
    #[test]
    fn the_transport_label_says_whether_a_direct_path_may_form_and_through_which_relays() {
        let local = Relay::Custom(String::from("http://127.0.0.1:3340"));
        assert_eq!(transport_label(&Relay::Off, false), "direct");
        assert_eq!(transport_label(&Relay::Default, false), "direct+relay default");
        assert_eq!(transport_label(&local, false), "direct+relay http://127.0.0.1:3340");
        assert_eq!(transport_label(&local, true), "relay-only http://127.0.0.1:3340");
        assert_eq!(transport_label(&Relay::Default, true), "relay-only default");
        assert_eq!(transport_label(&Relay::Off, true), "none", "relay-only without a relay");
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
            (String::new(), ServerSource::Unset),
            "no address anywhere: none is guessed"
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

    /// `--worker` is this run's meeting server even over the saved one (a script's worker is
    /// not lost to what an earlier run saved); one that is no address is passed over.
    #[test]
    fn the_worker_switch_wins_over_the_saved_server() {
        let saved = Some("https://meet.example.com");
        let env = Some("http://127.0.0.1:9999");
        assert_eq!(
            server_choice(Some(" http://127.0.0.1:8790/ "), saved, env, "https://built.in"),
            (
                String::from("http://127.0.0.1:8790"),
                ServerSource::CommandLine
            )
        );
        assert_eq!(
            server_choice(Some("not an address"), saved, env, "https://built.in"),
            (
                String::from("https://meet.example.com"),
                ServerSource::Saved
            )
        );
        assert_eq!(
            server_choice(None, None, env, "https://built.in"),
            server_prefill(None, env, "https://built.in")
        );
    }
}
