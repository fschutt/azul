//! AzCalendar's side of AzMeet: which meeting server registers the links (the saved setting, else
//! AzMeet's), the links AzCalendar makes itself (so making one works offline), what it sends to
//! register a room (`POST /rooms {room, starts_at, ends_at}`, the event's times in UTC), what the
//! server's answer means, what to tell the user when registering fails and whether to try again,
//! and which AzMeet program "Join meeting" opens. Link formats and the settings format are
//! AzMeet's own (`meet_rooms`, AzMeet's `rooms.rs`), not repeated here.

use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, NaiveTime, SecondsFormat, TimeDelta, TimeZone, Utc};
use serde::Deserialize;

use crate::{
    event::Meeting,
    meet_rooms::{self, RoomKey},
    settings,
};

/// The meeting server: set by AzMeet's variable.
pub const WORKER_VAR: &str = "AZMEET_WORKER";
/// The meeting AzMeet joins at start: AzMeet's variable.
pub const JOIN_VAR: &str = "AZMEET_JOIN";
/// The AzMeet program "Join meeting" opens, when it is not the one next to AzCalendar.
pub const AZMEET_BIN_VAR: &str = "AZMEET_BIN";
/// The name of the AzMeet program (without the platform's suffix).
pub const AZMEET_PROGRAM: &str = "AzMeet";

/// The meeting server built in at build time (`AZMEET_DEFAULT_WORKER`), the same default as
/// AzMeet's; empty means none.
pub const BUILT_IN_WORKER: &str = match option_env!("AZMEET_DEFAULT_WORKER") {
    Some(url) => url,
    None => "",
};

/// The meeting server: the one in the settings file's text `saved` (AzMeet's settings format),
/// else `env` (`AZMEET_WORKER`), else `built_in`, else AzMeet's local development server - the
/// order AzMeet itself uses (`meet_rooms::server_prefill`). There always is one: links are made
/// here and registered with it once it answers.
pub fn server_setting(saved: Option<&str>, env: Option<&str>, built_in: &str) -> String {
    let saved = saved.and_then(meet_rooms::decode_settings);
    meet_rooms::server_prefill(saved.as_deref(), env, built_in).0
}

/// The meeting server saved in the settings file at `path` (`settings::path`); `None` without a
/// file, or with one that names none.
pub fn read_saved_server(path: &Path) -> Option<String> {
    meet_rooms::decode_settings(&settings::read_text(path)?)
}

/// Saves `server` in the settings file at `path` (AzMeet's `meeting_server=` line), keeping the
/// file's other settings.
pub fn save_server(path: &Path, server: &str) -> std::io::Result<()> {
    settings::write_line(path, &meet_rooms::encode_settings(server))
}

/// The alphabet of room ids: lower-case Crockford base32, as the meeting server mints them and
/// AzMeet reads them (`meet_rooms::parse_room_key`, whose tests pin the two together).
const ROOM_ID_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// A room id made here, from 130 of `entropy`'s bits (all of the first two words, the two lowest
/// of the third): 26 characters of the room-id alphabet, the same shape and strength as the ids
/// the meeting server mints, so the server can register it as it is.
pub fn new_room_id(entropy: [u64; 3]) -> String {
    let mut bits = (u128::from(entropy[0]) << 64) | u128::from(entropy[1]);
    (0..26)
        .map(|i| {
            let value = if i < 25 {
                let v = (bits & 31) as usize;
                bits >>= 5;
                v
            } else {
                // 125 bits used: the last three of the pair and two of the third word.
                (bits & 7) as usize | (((entropy[2] & 3) as usize) << 3)
            };
            char::from(ROOM_ID_ALPHABET[value])
        })
        .collect()
}

/// A meeting AzCalendar made for `room_id`, to be registered with `server`: pending until it is.
pub fn pending_meeting(server: &str, room_id: &str) -> Meeting {
    Meeting {
        link: format!("{}{room_id}", meet_rooms::APP_LINK_PREFIX),
        server: server.to_string(),
        code: String::new(),
        expires: String::new(),
        starts_at: String::new(),
        ends_at: String::new(),
        pending: true,
    }
}

/// The room id a meeting's link names, if it names one.
pub fn room_id_of(meeting: &Meeting) -> Option<String> {
    match meet_rooms::parse_room_link(&meeting.link)? {
        RoomKey::Id(id) => Some(id),
        RoomKey::Code(_) => None,
    }
}

/// When an event on `date` from `start` to `end` (wall-clock times in `zone`, the user's zone in
/// the app) starts and ends, in UTC: the times the meeting server keeps its room for. Of a time
/// the clocks pass twice, the first; a time they skip (DST starts) is read an hour later, which is
/// the same instant in the zone's offset from before the change.
pub fn utc_window<Tz: TimeZone>(
    zone: &Tz,
    date: NaiveDate,
    start: NaiveTime,
    end: NaiveTime,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let utc = |time: NaiveTime| {
        let local = date.and_time(time);
        zone.from_local_datetime(&local)
            .earliest()
            .or_else(|| {
                zone.from_local_datetime(&(local + TimeDelta::hours(1)))
                    .earliest()
            })
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_else(|| local.and_utc())
    };
    (utc(start), utc(end))
}

/// The body of `POST /rooms` registering the room `room_id` for a meeting from `starts` to
/// `ends`: `{"room": "<id>", "starts_at": "2026-10-06T07:00:00Z", "ends_at": "..."}` (RFC 3339,
/// UTC). The server keeps the room until a while after `ends`, not only a day after it was made.
pub fn register_body(room_id: &str, starts: DateTime<Utc>, ends: DateTime<Utc>) -> String {
    let rfc3339 = |t: DateTime<Utc>| t.to_rfc3339_opts(SecondsFormat::Secs, true);
    serde_json::json!({
        "room": room_id,
        "starts_at": rfc3339(starts),
        "ends_at": rfc3339(ends),
    })
    .to_string()
}

/// What `POST /rooms` answers: `{room, code, link, url, expires, starts_at, ends_at}`
/// (`starts_at` / `ends_at` null or missing for a room kept without times).
#[derive(Deserialize)]
struct RoomAnswer {
    room: String,
    code: Option<String>,
    link: Option<String>,
    expires: Option<String>,
    starts_at: Option<String>,
    ends_at: Option<String>,
}

/// The meeting `server` minted, read from its answer to `POST /rooms`. The link must name the
/// room the server minted, by its id; it is kept as `azlin://meet/<room id>`.
pub fn minted_meeting(server: &str, body: &str) -> Result<Meeting, String> {
    let answer: RoomAnswer = serde_json::from_str(body)
        .map_err(|e| format!("The meeting server sent an answer AzCalendar cannot read ({e})."))?;
    let link = answer
        .link
        .filter(|link| !link.trim().is_empty())
        .unwrap_or_else(|| format!("{}{}", meet_rooms::APP_LINK_PREFIX, answer.room));
    let minted = meet_rooms::parse_room_key(&answer.room);
    match meet_rooms::parse_room_link(&link) {
        Some(RoomKey::Id(id)) if minted == Some(RoomKey::Id(id.clone())) => Ok(Meeting {
            link: format!("{}{id}", meet_rooms::APP_LINK_PREFIX),
            server: server.to_string(),
            code: answer.code.unwrap_or_default(),
            expires: answer.expires.unwrap_or_default(),
            starts_at: answer.starts_at.unwrap_or_default(),
            ends_at: answer.ends_at.unwrap_or_default(),
            pending: false,
        }),
        _ => Err(format!(
            "The meeting server sent a link that is not the AzMeet room it made: {link}"
        )),
    }
}

/// The meeting `server` registered for the room `room_id`, read from its answer. The answer must
/// be that room: a server that makes rooms of its own and ignores the one sent (one from before
/// links made in the app) is refused.
pub fn registered_meeting(server: &str, room_id: &str, body: &str) -> Result<Meeting, String> {
    let meeting = minted_meeting(server, body)?;
    if room_id_of(&meeting).as_deref() == Some(room_id) {
        Ok(meeting)
    } else {
        Err(format!(
            "The meeting server at {server} made a room of its own instead of registering the \
             link made here; it needs an update to register links made in AzCalendar."
        ))
    }
}

/// Whether a registration that failed with `status` (`None`: the server was not reached) is sent
/// again later: yes when unreachable, rate limited or failing itself, no when it read the
/// request and refused it.
pub fn sync_again(status: Option<u16>) -> bool {
    match status {
        None => true,
        Some(status) => status == 429 || status >= 500,
    }
}

/// What the user reads when `POST /rooms` failed: `status` and `body` of the server's answer, or
/// (`status` is `None`) why the server could not be reached.
pub fn mint_failure(server: &str, status: Option<u16>, body: &str) -> String {
    let Some(status) = status else {
        return format!("The meeting server at {server} is unreachable: {body}");
    };
    if status == 429 {
        return String::from(
            "Too many new meetings from this network; try again in a few minutes.",
        );
    }
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|json| Some(json.get("message")?.as_str()?.trim().to_string()))
        .filter(|message| !message.is_empty());
    match message {
        Some(message) => format!("The meeting server answered {status}: {message}"),
        None => format!("The meeting server answered {status}."),
    }
}

/// The AzMeet program: `setting` (`AZMEET_BIN`), else `AzMeet` next to `this_program`.
pub fn azmeet_program(setting: Option<&str>, this_program: Option<&Path>) -> Option<PathBuf> {
    sibling_program(setting, this_program, AZMEET_PROGRAM)
}

/// Another azul app's program: `setting` (its `..._BIN` variable), else the program `name`
/// next to `this_program` (the apps are built into one folder).
pub fn sibling_program(
    setting: Option<&str>,
    this_program: Option<&Path>,
    name: &str,
) -> Option<PathBuf> {
    if let Some(path) = setting.map(str::trim).filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(path));
    }
    let dir = this_program?.parent()?;
    Some(dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
}

/// The environment AzMeet is started with to join `meeting`: the link, and the server that
/// minted it (a link is only known to its own server).
pub fn join_env(meeting: &Meeting) -> Vec<(&'static str, String)> {
    vec![
        (JOIN_VAR, meeting.link.clone()),
        (WORKER_VAR, meeting.server.clone()),
    ]
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;

    use super::*;
    use crate::test_dir::TempDir;

    const ROOM: &str = "a2h859hyqkfaa11nhzxfh3gd7f";
    const SERVER: &str = "http://127.0.0.1:8787";

    fn answer(room: &str, link: Option<&str>) -> String {
        let mut json = serde_json::json!({
            "room": room,
            "code": "xq4-8kd-2nm",
            "url": format!("{SERVER}/rooms/{room}"),
            "expires": "2026-10-06T10:00:00.000Z",
            "starts_at": "2026-10-06T07:00:00.000Z",
            "ends_at": "2026-10-06T08:00:00.000Z",
        });
        if let Some(link) = link {
            json["link"] = serde_json::Value::from(link);
        }
        json.to_string()
    }

    /// There is always a meeting server: the one saved in the settings, else AzMeet's
    /// `AZMEET_WORKER`, else the built-in one, else the local development server - the same
    /// order as AzMeet's (`meet_rooms::server_prefill`). Links are made here and registered
    /// with it once it answers, so no setting is needed to make one.
    #[test]
    fn the_meeting_server_is_the_saved_one_else_azmeets_setting_else_a_default() {
        let saved = meet_rooms::encode_settings("https://saved.example.com");
        assert_eq!(
            server_setting(
                Some(&saved),
                Some("http://127.0.0.1:9000"),
                "https://built.in"
            ),
            "https://saved.example.com"
        );
        assert_eq!(
            server_setting(None, Some("http://127.0.0.1:9000/"), "https://built.in"),
            "http://127.0.0.1:9000"
        );
        assert_eq!(
            server_setting(Some("garbage"), Some("  "), "https://built.in/"),
            "https://built.in"
        );
        assert_eq!(server_setting(None, None, ""), meet_rooms::LOCAL_WORKER);
    }

    #[test]
    fn the_meeting_server_is_saved_and_read_back_and_a_broken_file_is_ignored() {
        let dir = TempDir::create();
        let path = settings::path(&dir.0);
        assert_eq!(read_saved_server(&path), None, "no file yet");
        save_server(&path, "https://meet.example.com").unwrap();
        assert_eq!(
            read_saved_server(&path).as_deref(),
            Some("https://meet.example.com")
        );
        save_server(&path, "http://127.0.0.1:8787").unwrap();
        assert_eq!(
            read_saved_server(&path).as_deref(),
            Some("http://127.0.0.1:8787")
        );
        std::fs::write(&path, "not a setting").unwrap();
        assert_eq!(read_saved_server(&path), None);
        // No temporary file is left next to it.
        let names: Vec<_> = std::fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
    }

    /// The link of a meeting made here: 26 characters of the room-id alphabet from 130 of the
    /// random bits, which AzMeet reads as a room id - the same shape as the ids the meeting
    /// server mints, so it can register it as it is.
    #[test]
    fn a_room_id_made_here_is_a_room_id_azmeet_reads_and_uses_130_bits() {
        let ids = [
            new_room_id([0, 0, 0]),
            new_room_id([u64::MAX, u64::MAX, u64::MAX]),
            new_room_id([0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210, 7]),
        ];
        assert_eq!(ids[0], "0".repeat(26));
        assert_eq!(ids[1], "z".repeat(26));
        for id in &ids {
            assert_eq!(id.len(), 26);
            assert_eq!(
                meet_rooms::parse_room_key(id),
                Some(RoomKey::Id(id.clone())),
                "{id}"
            );
        }
        // The lowest bit of the first word and the second-lowest of the third both count.
        assert_ne!(new_room_id([1, 0, 0]), new_room_id([0, 0, 0]));
        assert_ne!(new_room_id([0, 0, 2]), new_room_id([0, 0, 0]));
        assert_ne!(new_room_id([0, 1 << 63, 0]), new_room_id([0, 0, 0]));
        // Bits past the 130th do not.
        assert_eq!(new_room_id([0, 0, 4]), new_room_id([0, 0, 0]));
    }

    #[test]
    fn a_link_made_here_is_pending_on_the_meeting_server_it_will_be_registered_with() {
        assert_eq!(
            pending_meeting(SERVER, ROOM),
            Meeting {
                link: format!("azlin://meet/{ROOM}"),
                server: String::from(SERVER),
                code: String::new(),
                expires: String::new(),
                starts_at: String::new(),
                ends_at: String::new(),
                pending: true,
            }
        );
    }

    #[test]
    fn registering_sends_the_room_id_and_the_meeting_times_in_utc() {
        let body = register_body(ROOM, utc(2026, 10, 6, 7, 0), utc(2026, 10, 6, 8, 0));
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "room": ROOM,
                "starts_at": "2026-10-06T07:00:00Z",
                "ends_at": "2026-10-06T08:00:00Z",
            })
        );
    }

    #[test]
    fn the_answer_to_a_registration_must_be_the_room_that_was_sent() {
        let link = format!("azlin://meet/{ROOM}");
        let meeting = registered_meeting(SERVER, ROOM, &answer(ROOM, Some(&link))).unwrap();
        assert!(!meeting.pending);
        assert_eq!(meeting.link, link);
        assert_eq!(meeting.code, "xq4-8kd-2nm");
        assert_eq!(meeting.starts_at, "2026-10-06T07:00:00.000Z");
        let other = "b2h859hyqkfaa11nhzxfh3gd7f";
        assert!(registered_meeting(SERVER, ROOM, &answer(other, None)).is_err());
    }

    /// Unreachable, rate limited or a server error: the link stays pending and is sent again
    /// later. A refusal (the server read the request and said no) is not repeated.
    #[test]
    fn a_registration_that_failed_is_sent_again_unless_the_server_refused_it() {
        for status in [None, Some(429), Some(500), Some(502), Some(503)] {
            assert!(sync_again(status), "{status:?}");
        }
        for status in [Some(400), Some(404), Some(409), Some(413), Some(415)] {
            assert!(!sync_again(status), "{status:?}");
        }
    }

    #[test]
    fn a_minted_room_becomes_the_events_meeting() {
        let link = format!("azlin://meet/{ROOM}");
        let meeting = minted_meeting(SERVER, &answer(ROOM, Some(&link))).unwrap();
        assert_eq!(
            meeting,
            Meeting {
                link,
                server: String::from(SERVER),
                code: String::from("xq4-8kd-2nm"),
                expires: String::from("2026-10-06T10:00:00.000Z"),
                starts_at: String::from("2026-10-06T07:00:00.000Z"),
                ends_at: String::from("2026-10-06T08:00:00.000Z"),
                pending: false,
            }
        );
    }

    #[test]
    fn without_a_link_in_the_answer_the_link_is_made_from_the_room_id() {
        let meeting = minted_meeting(SERVER, &answer(ROOM, None)).unwrap();
        assert_eq!(meeting.link, format!("azlin://meet/{ROOM}"));
        let bare = minted_meeting(SERVER, &format!("{{\"room\": \"{ROOM}\"}}")).unwrap();
        assert_eq!(bare.link, format!("azlin://meet/{ROOM}"));
        assert_eq!(bare.code, "");
        assert_eq!(bare.expires, "");
        assert_eq!(bare.starts_at, "");
        assert_eq!(bare.ends_at, "");
    }

    #[test]
    fn a_room_the_server_keeps_without_times_is_a_meeting_without_them() {
        let body = format!("{{\"room\": \"{ROOM}\", \"starts_at\": null, \"ends_at\": null}}");
        let meeting = minted_meeting(SERVER, &body).unwrap();
        assert_eq!(
            (meeting.starts_at.as_str(), meeting.ends_at.as_str()),
            ("", "")
        );
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn the_meeting_times_are_the_events_day_and_times_read_in_the_local_zone_in_utc() {
        let berlin_summer = FixedOffset::east_opt(2 * 3600).unwrap();
        assert_eq!(
            utc_window(&berlin_summer, day(2026, 10, 6), at(9, 0), at(10, 0)),
            (utc(2026, 10, 6, 7, 0), utc(2026, 10, 6, 8, 0))
        );
        // West of Greenwich an evening meeting is on the next day in UTC.
        let new_york_winter = FixedOffset::west_opt(5 * 3600).unwrap();
        assert_eq!(
            utc_window(&new_york_winter, day(2026, 12, 31), at(22, 0), at(23, 30)),
            (utc(2027, 1, 1, 3, 0), utc(2027, 1, 1, 4, 30))
        );
        assert_eq!(
            utc_window(&Utc, day(2026, 10, 6), at(9, 15), at(9, 45)),
            (utc(2026, 10, 6, 9, 15), utc(2026, 10, 6, 9, 45))
        );
    }

    #[test]
    fn a_link_that_does_not_name_the_minted_room_is_refused() {
        let other = "b2h859hyqkfaa11nhzxfh3gd7f";
        for body in [
            answer(ROOM, Some(&format!("azlin://meet/{other}"))),
            answer(ROOM, Some("https://evil.example.com/")),
            answer(ROOM, Some("azlin://meet/xq4-8kd-2nm")),
            answer("not a room", None),
            String::from("{}"),
            String::from("<html>"),
            String::from(""),
        ] {
            assert!(minted_meeting(SERVER, &body).is_err(), "{body}");
        }
    }

    #[test]
    fn a_failed_mint_says_what_the_server_said_or_why_it_was_not_reached() {
        assert_eq!(
            mint_failure(SERVER, None, "connection refused"),
            "The meeting server at http://127.0.0.1:8787 is unreachable: connection refused"
        );
        assert_eq!(
            mint_failure(SERVER, Some(429), "{\"error\":\"rate_limited\"}"),
            "Too many new meetings from this network; try again in a few minutes."
        );
        assert_eq!(
            mint_failure(
                SERVER,
                Some(500),
                "{\"error\":\"internal\",\"message\":\"The database is down.\"}"
            ),
            "The meeting server answered 500: The database is down."
        );
        assert_eq!(
            mint_failure(SERVER, Some(502), "<html>Bad gateway</html>"),
            "The meeting server answered 502."
        );
    }

    #[test]
    fn join_meeting_opens_the_azmeet_next_to_azcalendar_unless_told_otherwise() {
        let exe = Path::new("/opt/azul/bin/AzCalendar");
        let next_to = PathBuf::from(format!(
            "/opt/azul/bin/AzMeet{}",
            std::env::consts::EXE_SUFFIX
        ));
        assert_eq!(azmeet_program(None, Some(exe)), Some(next_to.clone()));
        assert_eq!(azmeet_program(Some(" "), Some(exe)), Some(next_to));
        assert_eq!(
            azmeet_program(Some("/usr/local/bin/AzMeet"), Some(exe)),
            Some(PathBuf::from("/usr/local/bin/AzMeet"))
        );
        assert_eq!(azmeet_program(None, None), None);
        // The module switcher's Mail is AzMail, found the same way.
        assert_eq!(
            sibling_program(None, Some(exe), "AzMail"),
            Some(PathBuf::from(format!(
                "/opt/azul/bin/AzMail{}",
                std::env::consts::EXE_SUFFIX
            )))
        );
    }

    #[test]
    fn azmeet_joins_with_the_link_on_the_server_that_minted_it() {
        let meeting = Meeting {
            link: format!("azlin://meet/{ROOM}"),
            server: String::from(SERVER),
            code: String::new(),
            expires: String::new(),
            starts_at: String::new(),
            ends_at: String::new(),
            pending: false,
        };
        assert_eq!(
            join_env(&meeting),
            vec![
                ("AZMEET_JOIN", format!("azlin://meet/{ROOM}")),
                ("AZMEET_WORKER", String::from(SERVER)),
            ]
        );
    }
}
