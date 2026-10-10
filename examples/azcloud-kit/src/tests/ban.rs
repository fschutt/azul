//! A drive banned with a grace period (ban contract v1): `GET /v1/drives/<id>` and the
//! credentials say `status: "banned"`, `ban_reason`, `ban_until`; reads go on until then, writes
//! are refused (`drive_banned`); past the end the token server refuses the drive (403
//! `drive_banned`) and it is closed whatever this computer's clock says.

use azul_storage::{time::parse_iso8601, DriveError, ServiceError};

use super::{json, Fake, Shared, TOKEN};
use crate::{
    token::{ban_fluent_source, Ban, TokenServer, DRIVE_BANNED},
    user_errors::{fluent_source, Behaviour, Class, Code, Lang, UserError},
    DriveBundle, TokenError,
};

const UNTIL: &str = "2026-10-12T10:00:00Z";

fn server_answering(status: u16, body: &'static str) -> Shared {
    Shared(Fake::new(move |_, _| Ok(json(status, body))))
}

#[test]
fn a_banned_drive_says_why_and_until_when_it_may_still_be_read() {
    let transport = server_answering(
        200,
        r#"{"id": "d_1", "tier": "100GB", "read_only": true, "status": "banned",
            "ban_reason": "spam distribution", "ban_until": "2026-10-12T10:00:00Z",
            "period_until": "2026-11-07T00:00:00Z", "members": [], "you": "owner"}"#,
    );
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let status = server.drive_status("d_1", "dt_f.0.a").unwrap();
    let until = parse_iso8601(UNTIL).unwrap();
    assert_eq!(
        status.ban,
        Some(Ban {
            reason: String::from("spam distribution"),
            until: Some(until),
            closed: false,
        })
    );
    assert!(status.read_only, "a banned drive takes no writes");
    let ban = status.ban.unwrap();
    assert!(!ban.is_closed(until - 1));
    assert_eq!(ban.hours_left(until - 36 * 3600), 36);
    assert_eq!(ban.hours_left(until - 36 * 3600 + 1), 36, "rounded up");
    assert_eq!(ban.hours_left(until - 1), 1, "the last hour still counts");
    assert!(ban.is_closed(until), "closed at its end");
    assert_eq!(ban.hours_left(until + 5), 0);
}

#[test]
fn past_its_end_a_banned_drive_is_closed_whatever_this_clock_says() {
    let transport = server_answering(
        403,
        r#"{"error": "drive_banned", "message": "this drive was closed",
            "ban_reason": "spam distribution", "ban_until": "2026-10-12T10:00:00Z"}"#,
    );
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let status = server.drive_status("d_1", "dt_f.0.a").unwrap();
    let ban = status.ban.expect("the refusal is the ban's end, not an error");
    assert!(ban.closed);
    assert!(ban.is_closed(0), "even on a computer whose clock is behind");
    assert_eq!(ban.reason, "spam distribution");
    assert_eq!(ban.until, parse_iso8601(UNTIL));
    assert_eq!(DRIVE_BANNED, "drive_banned");
    // Another 403 stays a refusal.
    let transport = server_answering(403, r#"{"error": "forbidden", "message": "no"}"#);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        server.drive_status("d_1", "dt_f.0.a"),
        Err(TokenError::Refused { status: 403, .. })
    ));
}

#[test]
fn a_drive_in_good_standing_has_no_ban() {
    let transport = server_answering(
        200,
        r#"{"id": "d_1", "tier": "100GB", "read_only": false, "status": "active"}"#,
    );
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert_eq!(server.drive_status("d_1", "dt_f.0.a").unwrap().ban, None);
}

#[test]
fn the_credentials_of_a_banned_drive_carry_the_ban() {
    let text = super::bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa");
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(DriveBundle::from_value(&value).unwrap().ban, None);
    value["status"] = serde_json::json!("banned");
    value["ban_reason"] = serde_json::json!("spam distribution");
    value["ban_until"] = serde_json::json!(UNTIL);
    let bundle = DriveBundle::from_value(&value).unwrap();
    let ban = bundle.ban.expect("the bundle says it");
    assert_eq!(ban.reason, "spam distribution");
    assert_eq!(ban.until, parse_iso8601(UNTIL));
    assert!(!ban.closed, "credentials are handed out until the end");
}

#[test]
fn a_ban_without_a_reason_still_says_one() {
    let transport = server_answering(200, r#"{"id": "d_1", "status": "banned"}"#);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let ban = server.drive_status("d_1", "dt_f.0.a").unwrap().ban.unwrap();
    assert!(!ban.reason.trim().is_empty());
    assert_eq!(ban.until, None);
    assert!(!ban.is_closed(u64::MAX - 1), "no end said: not closed until the server says so");
}

#[test]
fn a_refused_write_of_a_banned_drive_is_read_only_in_english_and_german() {
    let refused = DriveError::Service(ServiceError {
        status: 403,
        code: String::from("AccessDenied"),
        message: String::from("the node's sentence"),
        azlin_error: Some(String::from("drive_banned")),
        ..ServiceError::default()
    });
    let user = UserError::from_drive_error(&refused).unwrap();
    assert_eq!(user.code, Code::DriveBanned);
    assert_eq!(user.class(), Class::ReadOnly);
    assert_eq!(user.behaviour(), Behaviour::ReadsOnly);
    assert!(user.notifies(), "the owner must act: copy the files");
    assert!(user.message(Lang::En).contains("banned"), "{}", user.message(Lang::En));
    assert!(user.message(Lang::De).contains("gesperrt"), "{}", user.message(Lang::De));
    let token = TokenError::Refused {
        status: 403,
        code: String::from("drive_banned"),
        message: String::from("this drive was closed"),
    };
    assert_eq!(
        UserError::from_token_error(&token).map(|u| u.code),
        Some(Code::DriveBanned)
    );
    assert_eq!(Code::parse("drive_banned"), Some(Code::DriveBanned));
    assert!(Code::ALL.contains(&Code::DriveBanned));
    for lang in [Lang::En, Lang::De] {
        assert!(
            fluent_source(lang)
                .lines()
                .any(|l| l.starts_with("azlin-error-drive-banned = ")),
            "{lang:?}"
        );
    }
}

#[test]
fn every_app_words_a_ban_the_same_counting_its_hours_down_then_closed() {
    let until = parse_iso8601(UNTIL).unwrap();
    let ban = Ban {
        reason: String::from("spam distribution"),
        until: Some(until),
        closed: false,
    };
    assert_eq!(
        ban.banner(until - 36 * 3_600),
        "Due to spam distribution, your account has been banned, but you have 36 hours to \
         migrate your files."
    );
    assert_eq!(
        ban.banner(until - 60),
        "Due to spam distribution, your account has been banned, but you have 1 hour to \
         migrate your files."
    );
    assert_eq!(
        ban.banner(until),
        "This drive was closed on 2026-10-12 because spam distribution."
    );
    assert_eq!(ban.closed_text(), ban.banner(until + 1));
}

/// The ban's words are messages of the kit's resources too (AzDrive and AzMail say them in the
/// window's language from one table): the banner with its hours, without an end, a closed
/// drive with and without its day, and the default reason as a word of its own.
#[test]
fn a_bans_words_are_phrases_of_the_kits_messages_in_english_and_german() {
    use azul_appkit::phrase::Arg;
    let until = parse_iso8601(UNTIL).unwrap();
    let ban = Ban {
        reason: String::from("spam distribution"),
        until: Some(until),
        closed: false,
    };
    let banner = ban.banner_phrase(until - 36 * 3_600);
    assert_eq!(banner.key, "azlin-ban-banner");
    assert_eq!(banner.get("hours"), Some(&Arg::Int(36)));
    assert_eq!(banner.get("reason"), Some(&Arg::from("spam distribution")));
    assert_eq!(ban.banner_phrase(until).key, "azlin-ban-closed");
    assert_eq!(ban.closed_phrase().get("day"), Some(&Arg::from("2026-10-12")));
    let open_ended = Ban {
        until: None,
        ..ban.clone()
    };
    assert_eq!(open_ended.banner_phrase(until).key, "azlin-ban-banner-no-end");
    assert_eq!(open_ended.closed_phrase().key, "azlin-ban-closed-no-day");
    // The token server's default reason is a word of the kit's (said in the language).
    let default = Ban::of(&serde_json::json!({"status": "banned"})).unwrap();
    assert!(matches!(
        default.banner_phrase(0).get("reason"),
        Some(Arg::Word { key, .. }) if key == "azlin-ban-reason-terms"
    ));
    for lang in [Lang::En, Lang::De] {
        let source = ban_fluent_source(lang);
        for id in [
            "azlin-ban-banner",
            "azlin-ban-banner-no-end",
            "azlin-ban-closed",
            "azlin-ban-closed-no-day",
            "azlin-ban-reason-terms",
        ] {
            assert!(source.contains(&format!("{id} = ")), "{lang:?} lacks {id}");
        }
    }
    assert!(ban_fluent_source(Lang::De).contains("gesperrt"));
}
