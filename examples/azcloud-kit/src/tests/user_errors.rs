//! The errors users see (D33): one table from the token server's and the storage's error codes
//! to a class, a behaviour and a text in English and German, with the request ID as the error ID.

use azlin_contract::AzlinError;
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
        azlin_error: code.map(AzlinError::from_wire),
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
    assert_eq!(code_of(&azlin(429, "TooManyRequests", None)), Some(Code::Unavailable));
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
    // A prepaid period that ran out had no payment to go wrong (R1).
    assert!(en.starts_with("Your paid period has ended."), "{en}");
    assert!(en.ends_with("Error ID: n2-81723"), "{en}");
    let de = unpaid.message(Lang::De);
    assert!(de.starts_with("Dein bezahlter Zeitraum ist abgelaufen."), "{de}");
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
fn a_drive_this_device_holds_no_key_for_is_the_tables_locked_drive_in_every_language() {
    // An encrypted drive this device cannot open yet (no key here; after another device's
    // rotation the wrap is gone): the user unlocks it with the recovery code or joins it from
    // a device that has the key. The status says so in the window's language, not in the
    // library's English with the drive's id.
    let locked = DriveError::Locked {
        drive: String::from("d_h6xbwdj6w6mw3cxxzib75s57mi"),
    };
    let user = UserError::from_drive_error(&locked).expect("a user's error");
    assert_eq!(user.code.as_str(), "locked");
    assert_eq!(user.class(), Class::ReAuth);
    for (lang, word) in [(Lang::En, "recovery code"), (Lang::De, "Wiederherstellungscode")] {
        let said = user.message(lang);
        assert!(said.contains(word), "{lang:?}: {said}");
        assert!(!said.contains("d_h6"), "{lang:?}: the drive's id is no word: {said}");
    }
}

#[test]
fn an_expired_session_a_node_names_is_trouble_that_passes_not_something_that_went_wrong() {
    // A node refuses a session that ran out, or whose signing epoch ended, with x-azlin-error
    // session_expired: the drive asks for fresh credentials and goes on. Should the user see it
    // at all, it passes by itself - and no node is asked again with the same credentials.
    let expired = azlin(403, "ExpiredToken", Some("session_expired"));
    let user = UserError::from_drive_error(&expired).expect("a user's error");
    assert_eq!(user.code.as_str(), "session_expired");
    assert_eq!(user.class(), Class::Retry);
    assert!(
        !matches!(
            user.behaviour(),
            Behaviour::RetryWithBackoff | Behaviour::RetryAfterPause | Behaviour::Report
        ),
        "{:?}",
        user.behaviour()
    );
}

#[test]
fn the_table_is_a_fluent_resource_in_english_and_german() {
    let en = fluent_source(Lang::En);
    let de = fluent_source(Lang::De);
    for row in ROWS {
        let id = format!("azlin-error-{} = ", row.code.as_str().replace('_', "-"));
        assert!(en.contains(&id) && de.contains(&id), "{id}");
    }
    assert!(en.contains("azlin-error-read-only-unpaid = Your paid period has ended."));
    assert!(de.contains("azlin-error-read-only-unpaid = Dein bezahlter Zeitraum ist abgelaufen."));
    assert!(en.contains("{ $detail }"), "the placeholders as Fluent variables");
    assert_eq!(en.lines().filter(|l| l.starts_with("azlin-error-")).count(), ROWS.len());
}

#[test]
fn every_code_has_an_english_and_a_german_message_and_both_resources_parse() {
    use fluent_syntax::ast::Entry;

    use crate::user_errors::ID_LABEL_MESSAGE;

    assert_eq!(
        Code::ReadOnlyUnpaid.message_id(),
        "azlin-error-read-only-unpaid"
    );
    for lang in [Lang::En, Lang::De] {
        let source = fluent_source(lang);
        let resource = fluent_syntax::parser::parse(source.as_str())
            .unwrap_or_else(|(_, errors)| panic!("{lang:?}: {errors:?}"));
        let ids: Vec<String> = resource
            .body
            .iter()
            .filter_map(|entry| match entry {
                Entry::Message(message) => Some(message.id.name.to_string()),
                _ => None,
            })
            .collect();
        for code in Code::ALL {
            assert!(
                ids.contains(&code.message_id()),
                "{lang:?} lacks {}",
                code.message_id()
            );
        }
        assert!(
            ids.iter().any(|id| id == ID_LABEL_MESSAGE),
            "{lang:?}: {ids:?}"
        );
        assert_eq!(
            ids.len(),
            Code::ALL.len() + 1,
            "{lang:?}: one message per code and the label"
        );
    }
}

#[test]
fn a_user_error_names_its_message_its_arguments_and_its_error_id() {
    let error = UserError {
        code: Code::Other,
        retry_after: None,
        request_id: Some(String::from(" 075C54CD9331456E ")),
        detail: String::from(" the node said no "),
    };
    assert_eq!(error.message_id(), "azlin-error-other");
    assert_eq!(
        error.fluent_args(),
        vec![("detail", String::from("the node said no"))]
    );
    assert_eq!(error.error_id(), Some("075C54CD9331456E"));
    let without = UserError {
        request_id: Some(String::from("  ")),
        ..error
    };
    assert_eq!(without.error_id(), None);
}

#[test]
fn the_recovery_refusals_have_their_rows_and_texts() {
    // SRV17's recovery shapes (D42, D51, F12): a recovery-pending device waits for the 48 h,
    // a change of the recovery keys or a cancel needs the code, the last key stays, a lookup's
    // challenge ran out.
    let refused = |status: u16, code: &str| {
        UserError::from_token_error(&TokenError::Refused {
            status,
            code: code.to_string(),
            message: String::new(),
        })
        .unwrap()
    };
    let rows = [
        (403, "lockdown_pending", Code::LockdownPending, Class::Retry),
        (403, "recovery_key_required", Code::RecoveryKeyRequired, Class::Fatal),
        (409, "last_recovery_key", Code::LastRecoveryKey, Class::Fatal),
        (401, "bad_challenge", Code::BadChallenge, Class::Retry),
    ];
    for (status, text, code, class) in rows {
        let user = refused(status, text);
        assert_eq!((user.code, user.class()), (code, class), "{text}");
        assert_eq!(Code::parse(text), Some(code));
        assert!(!user.notifies(), "{text}: said in the app, no notification");
        assert!(!user.message(Lang::De).is_empty());
    }
    assert!(refused(403, "lockdown_pending").message(Lang::En).contains("48-hour"));
    assert!(refused(403, "recovery_key_required")
        .message(Lang::En)
        .contains("recovery code"));
    assert!(refused(409, "last_recovery_key").message(Lang::En).contains("last"));
    assert!(refused(401, "bad_challenge").message(Lang::En).contains("Try again"));
    assert_eq!(Code::Other.row().code, Code::Other, "Other keeps its row");
}

#[test]
fn every_code_of_the_contract_has_a_row_of_its_own() {
    for code in AzlinError::ALL {
        let ours = Code::from(code);
        assert_eq!(ours.row().code, ours, "{code}: {ours:?} has its row");
        // `internal` is the one code that is "something went wrong" by design.
        if code != AzlinError::Internal {
            assert_ne!(ours, Code::Other, "{code} has a row of its own");
        }
        let (status, s3) = code.s3();
        let read = code_of(&azlin(status, s3, Some(code.as_str())));
        assert_eq!(read, Some(ours), "{code} as a node sends it");
    }
}

#[test]
fn a_client_too_old_for_the_service_is_asked_to_update() {
    let refused = TokenError::Refused {
        status: 426,
        code: String::from("client_too_old"),
        message: String::from("this app speaks contract 2, the minimum is 3"),
    };
    let user = UserError::from_token_error(&refused).unwrap();
    assert_eq!((user.code, user.class()), (Code::UpdateApp, Class::Fatal));
    assert!(user.message(Lang::En).contains("Update the app"));
    assert!(user.message(Lang::De).contains("Aktualisiere die App"));
    assert_eq!(
        code_of(&azlin(426, "AzlinClientTooOld", Some("client_too_old"))),
        Some(Code::UpdateApp)
    );
}

/// 426 is "upgrade required" whatever carries it: a proxy's answer without the service's body
/// asks for an update too. A code this build does not know is not taken for one: it stays
/// something that went wrong, with the error ID, since no newer app may exist for it.
#[test]
fn an_answer_426_asks_for_an_update_even_without_its_code() {
    let bare = TokenError::Refused {
        status: 426,
        code: String::new(),
        message: String::new(),
    };
    assert_eq!(UserError::from_token_error(&bare).map(|u| u.code), Some(Code::UpdateApp));
    assert_eq!(code_of(&azlin(426, "UpgradeRequired", None)), Some(Code::UpdateApp));
    let unknown = TokenError::Refused {
        status: 400,
        code: String::from("a_new_word"),
        message: String::new(),
    };
    assert_eq!(UserError::from_token_error(&unknown).map(|u| u.code), Some(Code::Other));
}

#[test]
fn a_refused_order_is_said_in_the_buyers_language_never_as_the_servers_sentence() {
    let refused = |status: u16, code: &str| {
        UserError::from_token_error(&TokenError::Refused {
            status,
            code: code.to_string(),
            message: String::from("Azlin Storage is not sold to XX yet"),
        })
        .unwrap()
    };
    let blocked = refused(403, "blocked_country");
    assert_eq!(blocked.code, Code::BlockedCountry);
    assert_eq!(blocked.class(), Class::Fatal, "asking again does not help");
    for lang in Lang::ALL {
        let text = blocked.message(lang);
        assert!(!text.contains("XX") && !text.contains("HTTP"), "{lang:?}: {text}");
    }
    assert!(blocked.message(Lang::De).contains("Land"));
    for (status, code, expected) in [
        (400, "bad_months", Code::BadMonths),
        (400, "bad_method", Code::BadMethod),
        (409, "tier_unavailable", Code::TierUnavailable),
        (409, "tier_ended", Code::TierUnavailable),
        (400, "payer_required", Code::PayerRequired),
        (409, "checkout_paid", Code::CheckoutPaid),
        (502, "provider_unavailable", Code::Unavailable),
    ] {
        assert_eq!(refused(status, code).code, expected, "{code}");
    }
    // Every refusal of the contract has its row (the match in user_errors does not compile
    // without one); the ones no buyer can act on say what went wrong.
    for refusal in azlin_contract::CheckoutRefusal::ALL {
        let code = Code::from(&refusal);
        assert_eq!(code.row().code, code, "{refusal:?}");
    }
}

/// A grant asked again whose first answer was used (`already_answered`): said in the user's
/// language as what happened - the request went through before - never as the server's sentence.
#[test]
fn a_grant_answered_before_is_said_as_done_already() {
    let refused = TokenError::Refused {
        status: 409,
        code: String::from(azlin_contract::ALREADY_ANSWERED),
        message: String::from("this request was answered before and its grant is in use"),
    };
    let user = UserError::from_token_error(&refused).unwrap();
    assert_eq!((user.code, user.class()), (Code::AlreadyAnswered, Class::Fatal));
    assert!(user.message(Lang::En).contains("already"), "{}", user.message(Lang::En));
    assert!(user.message(Lang::De).contains("bereits"), "{}", user.message(Lang::De));
#[test]
fn a_read_only_drive_says_why_and_only_an_unpaid_one_speaks_of_a_payment() {
    let unpaid = UserError::from_drive_error(&azlin(403, "AccessDenied", Some("read_only_unpaid")))
        .unwrap();
    let frozen = UserError::from_drive_error(&azlin(403, "AccessDenied", Some("read_only_lockdown")))
        .unwrap();
    let operator = UserError::from_drive_error(&azlin(403, "AccessDenied", Some("read_only_operator")))
        .unwrap();
    assert_eq!(frozen.code, Code::ReadOnlyLockdown);
    assert_eq!(operator.code, Code::ReadOnlyOperator);
    for user in [&unpaid, &frozen, &operator] {
        assert_eq!(
            (user.class(), user.behaviour()),
            (Class::ReadOnly, Behaviour::ReadsOnly),
            "{:?}",
            user.code
        );
    }
    assert!(unpaid.message(Lang::En).contains("payment"));
    for user in [&frozen, &operator] {
        for lang in Lang::ALL {
            let text = user.message(lang).to_lowercase();
            assert!(!text.contains("payment") && !text.contains("zahlung"), "{text}");
        }
    }
    assert!(frozen.message(Lang::En).contains("recovery code"), "the owner's warning");
    // the token server's lockdown_pending is the recovering computer's: it waits
    let waiting = UserError::from_token_error(&TokenError::Refused {
        status: 403,
        code: String::from("lockdown_pending"),
        message: String::new(),
    })
    .unwrap();
    assert_eq!(waiting.code, Code::LockdownPending);
}

#[test]
fn a_lockdowns_delete_pause_waits_as_long_as_the_node_says_and_does_not_promise_a_moment() {
    let mut paused = azlin(503, "SlowDown", Some("delete_paused"));
    if let DriveError::Service(service) = &mut paused {
        service.retry_after = Some(86_000);
    }
    let user = UserError::from_drive_error(&paused).unwrap();
    assert_eq!(user.code, Code::DeletePaused);
    assert_eq!(user.retry_after, Some(86_000));
    assert_eq!(user.behaviour(), Behaviour::RetryAfterPause);
    assert!(!user.message(Lang::En).contains("moment"), "{}", user.message(Lang::En));
}
