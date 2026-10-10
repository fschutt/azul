//! The errors users see (D33): one table from the token server's and the storage's error codes
//! to a class, a behaviour and a text in English and German, with the request ID as the error ID.

use azul_storage::{DriveError, ServiceError};

use crate::{
    error::CloudError,
    token::TokenError,
    user_errors::{fluent_source, Behaviour, Class, Code, Lang, UserError, ROWS},
};

/// An S3 error answer of an Azlin node: its HTTP status, its S3 code, its `x-azlin-error`.
fn azlin(status: u16, s3: &str, code: Option<&str>) -> DriveError {
    DriveError::Service(ServiceError {
        status,
        code: s3.to_string(),
        message: String::from("the node's sentence"),
        request_id: Some(String::from("n2-81723")),
        azlin_error: code.map(str::to_string),
        ..ServiceError::default()
    })
}

fn code_of(e: &DriveError) -> Option<Code> {
    UserError::from_drive_error(e).map(|u| u.code)
}

#[test]
fn every_row_has_a_class_a_behaviour_and_an_english_and_a_german_text() {
    let mut seen = Vec::new();
    for row in ROWS {
        assert!(!seen.contains(&row.code), "{:?} twice", row.code);
        seen.push(row.code);
        assert_eq!(Code::parse(row.code.as_str()), Some(row.code));
        assert_eq!(row.code.row().code, row.code);
        assert!(!row.en.trim().is_empty() && !row.de.trim().is_empty());
        assert_ne!(row.en, row.de, "{:?} has a German text of its own", row.code);
    }
    assert_eq!(seen.len(), Code::ALL.len(), "one row per code");
}

#[test]
fn each_storage_error_code_has_its_class_and_behaviour() {
    let rows: [(DriveError, Code, Class, Behaviour); 14] = [
        (
            azlin(503, "SlowDown", Some("unavailable")),
            Code::Unavailable,
            Class::Retry,
            Behaviour::RetryWithBackoff,
        ),
        (
            azlin(503, "ServiceUnavailable", Some("maintenance")),
            Code::Maintenance,
            Class::Retry,
            Behaviour::RetryAfterPause,
        ),
        (
            DriveError::Transport(String::from("connection refused")),
            Code::Network,
            Class::Retry,
            Behaviour::WaitUntilOnline,
        ),
        (
            azlin(403, "QuotaExceeded", Some("quota_exceeded")),
            Code::QuotaExceeded,
            Class::ReadOnly,
            Behaviour::StopUploads,
        ),
        (
            azlin(403, "AccessDenied", Some("read_only_unpaid")),
            Code::ReadOnlyUnpaid,
            Class::ReadOnly,
            Behaviour::ReadsOnly,
        ),
        (
            azlin(403, "InvalidAccessKeyId", Some("credentials_revoked")),
            Code::CredentialsRevoked,
            Class::ReAuth,
            Behaviour::SignInAgain,
        ),
        (
            DriveError::Denied {
                message: String::from("the drive token was refused"),
            },
            Code::SignIn,
            Class::ReAuth,
            Behaviour::SignInAgain,
        ),
        (
            azlin(400, "EntityTooLarge", Some("object_too_large")),
            Code::ObjectTooLarge,
            Class::Fatal,
            Behaviour::SkipFile,
        ),
        (
            DriveError::Conflict {
                key: String::from("docs/a.txt"),
            },
            Code::NameConflict,
            Class::Fatal,
            Behaviour::KeepBoth,
        ),
        (
            azlin(403, "AccessDenied", Some("link_blocked")),
            Code::LinkBlocked,
            Class::Fatal,
            Behaviour::Nothing,
        ),
        (
            DriveError::Corrupt {
                key: String::from("docs/a.txt"),
                reason: String::from("the tag does not match"),
            },
            Code::Integrity,
            Class::Fatal,
            Behaviour::RetryOnceThenMark,
        ),
        (
            azlin(503, "SlowDown", Some("delete_paused")),
            Code::DeletePaused,
            Class::Retry,
            Behaviour::RetryAfterPause,
        ),
        (
            azlin(301, "PermanentRedirect", Some("wrong_block")),
            Code::WrongBlock,
            Class::Retry,
            Behaviour::FollowRedirect,
        ),
        (
            azlin(500, "InternalError", Some("internal")),
            Code::Other,
            Class::Fatal,
            Behaviour::Report,
        ),
    ];
    for (error, code, class, behaviour) in rows {
        let user = UserError::from_drive_error(&error).expect("a user's error");
        assert_eq!(
            (user.code, user.class(), user.behaviour()),
            (code, class, behaviour),
            "{error}"
        );
    }
    // The other codes of the same rows.
    assert_eq!(
        code_of(&azlin(412, "PreconditionFailed", Some("name_conflict"))),
        Some(Code::NameConflict)
    );
    assert_eq!(
        code_of(&azlin(500, "InternalError", Some("integrity"))),
        Some(Code::Integrity)
    );
    // A code this AzDrive does not know yet, an answer that makes no sense: "something went
    // wrong", with the error ID.
    assert_eq!(code_of(&azlin(500, "InternalError", Some("frobnicated"))), Some(Code::Other));
    assert_eq!(
        code_of(&DriveError::Protocol(String::from("not a listing"))),
        Some(Code::Other)
    );
}

#[test]
fn a_service_without_azlin_codes_is_read_by_its_status_only() {
    // Another S3 service: busy is busy, but an AccessDenied is no unpaid Azlin drive.
    assert_eq!(code_of(&azlin(503, "SlowDown", None)), Some(Code::Unavailable));
    assert_eq!(code_of(&azlin(500, "InternalError", None)), Some(Code::Other));
    assert_eq!(code_of(&azlin(403, "AccessDenied", None)), Some(Code::Other));
}

#[test]
fn what_the_user_did_is_no_service_error() {
    for error in [
        DriveError::NotFound {
            key: String::from("a.txt"),
        },
        DriveError::InvalidKey {
            key: String::from("../a"),
            reason: "it leaves the drive",
        },
        DriveError::Io(String::from("disk full")),
        DriveError::InvalidConfig(String::from("no endpoint")),
    ] {
        assert_eq!(UserError::from_drive_error(&error), None, "{error}");
    }
}

#[test]
fn the_token_servers_refusals_have_their_rows_too() {
    let of = |e: TokenError| UserError::from_token_error(&e).map(|u| u.code);
    assert_eq!(of(TokenError::Connect(String::from("dns"))), Some(Code::Network));
    assert_eq!(
        of(TokenError::SignIn(String::from("token_reuse"))),
        Some(Code::CredentialsRevoked)
    );
    for code in ["not_verified", "try_again", "busy"] {
        let refused = TokenError::Refused {
            status: 503,
            code: code.to_string(),
            message: String::new(),
        };
        assert_eq!(of(refused), Some(Code::Unavailable), "{code}");
    }
    let refused = TokenError::Refused {
        status: 409,
        code: String::from("already_issued"),
        message: String::from("3 of 3"),
    };
    assert_eq!(of(refused), Some(Code::Other));
    assert_eq!(of(TokenError::Protocol(String::from("?"))), Some(Code::Other));
    assert_eq!(of(TokenError::Config(String::from("no server"))), None);
    // Through CloudError, whatever context it carries.
    let cloud = CloudError::from(TokenError::Connect(String::from("dns"))).context("refresh");
    assert_eq!(
        UserError::from_cloud_error(&cloud).map(|u| u.code),
        Some(Code::Network)
    );
    let cloud = CloudError::Drive(azlin(403, "QuotaExceeded", Some("quota_exceeded")));
    assert_eq!(
        UserError::from_cloud_error(&cloud).map(|u| u.code),
        Some(Code::QuotaExceeded)
    );
    assert_eq!(UserError::from_cloud_error(&CloudError::failed("local")), None);
}

#[test]
fn the_message_is_in_the_users_language_and_names_the_error_id() {
    let unpaid = UserError::from_drive_error(&azlin(403, "AccessDenied", Some("read_only_unpaid")))
        .unwrap();
    assert_eq!(unpaid.request_id.as_deref(), Some("n2-81723"));
    let en = unpaid.message(Lang::En);
    assert!(en.starts_with("Your last payment didn't go through."), "{en}");
    assert!(en.ends_with("Error ID: n2-81723"), "{en}");
    let de = unpaid.message(Lang::De);
    assert!(de.starts_with("Deine letzte Zahlung"), "{de}");
    assert!(de.ends_with("Fehler-ID: n2-81723"), "{de}");
    // No request ID (no answer at all): no error ID.
    let offline = UserError::from_drive_error(&DriveError::Transport(String::from("refused")))
        .unwrap();
    assert!(!offline.message(Lang::En).contains("Error ID"));
    assert!(offline.message(Lang::En).starts_with("Offline"));
    // Something unknown keeps what went wrong, so support can read it.
    let other = UserError::from_drive_error(&azlin(500, "InternalError", Some("frobnicated")))
        .unwrap();
    let text = other.message(Lang::En);
    assert!(text.starts_with("Something went wrong"), "{text}");
    assert!(text.contains("the node's sentence") && text.contains("n2-81723"), "{text}");
    // Maintenance names the pause the node asked for.
    let mut maintenance =
        UserError::from_drive_error(&azlin(503, "ServiceUnavailable", Some("maintenance")))
            .unwrap();
    maintenance.retry_after = Some(30);
    assert_eq!(maintenance.retry_after, Some(30));
    assert!(maintenance.message(Lang::En).contains("being updated"));
}

#[test]
fn only_an_error_the_user_must_act_on_raises_a_notification() {
    let notifies = |e: DriveError| UserError::from_drive_error(&e).unwrap().notifies();
    assert!(notifies(azlin(403, "AccessDenied", Some("read_only_unpaid"))));
    assert!(notifies(azlin(403, "QuotaExceeded", Some("quota_exceeded"))));
    assert!(notifies(azlin(403, "InvalidAccessKeyId", Some("credentials_revoked"))));
    assert!(!notifies(azlin(503, "SlowDown", Some("unavailable"))), "transient");
    assert!(!notifies(DriveError::Transport(String::from("offline"))), "transient");
    assert!(!notifies(azlin(400, "EntityTooLarge", Some("object_too_large"))), "per file");
}

#[test]
fn the_language_comes_from_the_locale() {
    for de in ["de", "de_DE.UTF-8", "de-AT", "DE_ch"] {
        assert_eq!(Lang::from_locale(de), Lang::De, "{de}");
    }
    for en in ["", "C", "en_US.UTF-8", "fr_FR", "dk"] {
        assert_eq!(Lang::from_locale(en), Lang::En, "{en}");
    }
}

#[test]
fn the_table_is_a_fluent_resource_in_english_and_german() {
    let en = fluent_source(Lang::En);
    let de = fluent_source(Lang::De);
    for row in ROWS {
        let id = format!("azlin-error-{} = ", row.code.as_str().replace('_', "-"));
        assert!(en.contains(&id) && de.contains(&id), "{id}");
    }
    assert!(en.contains("azlin-error-read-only-unpaid = Your last payment didn't go through."));
    assert!(de.contains("azlin-error-read-only-unpaid = Deine letzte Zahlung"));
    assert!(en.contains("{ $detail }"), "the placeholders as Fluent variables");
    assert_eq!(en.lines().filter(|l| l.starts_with("azlin-error-")).count(), ROWS.len());
}
