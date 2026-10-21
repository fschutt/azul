//! What each type refuses, and what it reads that is not in a fixture.

use azlin_contract::{
    fixtures_digest, AppName, AzlinError, BucketName, Cents, ClientHello, Currency, Deadline,
    DeadlineKind, DriveId, IdempotencyKey, Months, RefreshRefusal, Rfc3339, SemVer, TierId,
    CONTRACT_VERSION,
};

#[test]
fn a_path_id_takes_only_what_a_url_path_can_hold() {
    assert_eq!(DriveId::parse(" d_k3f9-x ").expect("ok").as_str(), "d_k3f9-x");
    let long = "a".repeat(129);
    for bad in ["", "d/1", "../x", "d 1", "d?x", "d%2f", long.as_str()] {
        assert!(DriveId::parse(bad).is_err(), "{bad:?}");
    }
    let refused: Result<DriveId, _> = serde_json::from_str("\"../etc\"");
    assert!(refused.is_err());
}

#[test]
fn a_bucket_name_follows_the_s3_rules() {
    assert!(BucketName::parse("azlin-d-1").is_ok());
    let long = "a".repeat(64);
    for bad in ["ab", "Azlin", "-azlin", "azlin-", "a..b", "a_b", long.as_str()] {
        assert!(BucketName::parse(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn a_tier_is_its_size_in_capitals_without_blanks() {
    assert_eq!(TierId::parse(" 1 tb ").expect("ok").as_str(), "1TB");
    assert_eq!(TierId::parse("100GB").expect("ok").as_str(), "100GB");
    for bad in ["", "  ", "1-TB", "1.5TB"] {
        assert!(TierId::parse(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn months_are_only_the_periods_a_checkout_sells() {
    for ok in [1, 3, 6, 12, 24] {
        assert_eq!(Months::try_from(ok).expect("ok").get(), ok);
    }
    for bad in [0, 2, 4, 5, 7, 11, 13, 25, 36] {
        assert!(Months::try_from(bad).is_err(), "{bad}");
    }
    assert!(serde_json::from_str::<Months>("2").is_err());
}

#[test]
fn amounts_in_two_currencies_do_not_add_up() {
    let eur = |amount| Cents {
        amount,
        currency: Currency::parse("EUR").expect("eur"),
    };
    assert_eq!(eur(1).checked_add(&eur(2)), Some(eur(3)));
    let pln = Cents {
        amount: 1,
        currency: Currency::parse("pln").expect("pln"),
    };
    assert_eq!(eur(1).checked_add(&pln), None);
    assert!(Currency::parse("EURO").is_err());
    assert!(Currency::parse("E1R").is_err());
}

#[test]
fn a_time_reads_what_rfc_3339_writes_and_keeps_its_text() {
    let t = Rfc3339::from_unix(0);
    assert_eq!(t.as_str(), "1970-01-01T00:00:00Z");
    assert_eq!(Rfc3339::from_unix(1_792_540_800).as_str(), "2026-10-21T00:00:00Z");
    let offset = Rfc3339::parse("2026-10-21T10:00:00.250+02:00").expect("ok");
    assert_eq!(offset.unix(), Rfc3339::parse("2026-10-21T08:00:00Z").expect("ok").unix());
    assert_eq!(offset.as_str(), "2026-10-21T10:00:00.250+02:00");
    assert_eq!(Rfc3339::parse("2024-02-29T00:00:00Z").expect("leap").unix(), 1_709_164_800);
    for bad in [
        "",
        "2026-10-21",
        "2026-10-21T08:00:00",
        "2026-10-21 08:00:00Z",
        "2026-13-01T00:00:00Z",
        "2025-02-29T00:00:00Z",
        "2026-10-21T24:00:00Z",
        "2026-10-21T08:00:00.Z",
        "2026-10-21T08:00:00+2:00",
    ] {
        assert!(Rfc3339::parse(bad).is_err(), "{bad:?}");
    }
    for unix in [-86_400, 951_782_400, 4_102_444_800] {
        assert_eq!(Rfc3339::parse(Rfc3339::from_unix(unix).as_str()).expect("ok").unix(), unix);
    }
}

#[test]
fn a_deadline_of_a_kind_a_newer_service_added_still_reads() {
    let read: Deadline =
        serde_json::from_str(r#"{"at":"2026-10-21T08:00:00Z","kind":"tax_year_end"}"#)
            .expect("reads");
    assert_eq!(read.kind, DeadlineKind::Unknown);
    assert!(read.is_past(read.at.unix()));
    assert!(!read.is_past(read.at.unix() - 1));
}

#[test]
fn an_unknown_error_code_is_unknown_and_a_known_one_is_itself() {
    for code in AzlinError::ALL {
        assert_eq!(AzlinError::parse(code.as_str()), Some(code));
        assert_eq!(AzlinError::from_wire(code.as_str()), code);
        let json = serde_json::to_string(&code).expect("writes");
        assert_eq!(json, format!("\"{}\"", code.as_str()));
    }
    assert_eq!(AzlinError::parse("teapot"), None);
    assert_eq!(AzlinError::from_wire("teapot"), AzlinError::Unknown);
    let read: AzlinError = serde_json::from_str("\"teapot\"").expect("reads");
    assert_eq!(read, AzlinError::Unknown);
    assert!(!AzlinError::ALL.contains(&AzlinError::Unknown));
}

#[test]
fn the_node_codes_keep_the_s3_answers_they_had() {
    // The codes the nodes answered with before this crate, as they answered them.
    let before = [
        (AzlinError::Unavailable, 503, "SlowDown", Some(0)),
        (AzlinError::Maintenance, 503, "ServiceUnavailable", Some(30)),
        (AzlinError::QuotaExceeded, 403, "QuotaExceeded", None),
        (AzlinError::ReadOnlyUnpaid, 403, "AccessDenied", None),
        (AzlinError::CredentialsRevoked, 403, "InvalidAccessKeyId", None),
        (AzlinError::ObjectTooLarge, 400, "EntityTooLarge", None),
        (AzlinError::NameConflict, 412, "PreconditionFailed", None),
        (AzlinError::LinkBlocked, 403, "AccessDenied", None),
        (AzlinError::Integrity, 500, "InternalError", None),
        (AzlinError::DeletePaused, 503, "SlowDown", Some(5)),
        (AzlinError::WrongBlock, 301, "PermanentRedirect", None),
        (AzlinError::Internal, 500, "InternalError", None),
        (AzlinError::DriveBanned, 403, "AccessDenied", None),
        (AzlinError::SessionExpired, 403, "ExpiredToken", None),
        (AzlinError::ClockSkew, 403, "RequestTimeTooSkewed", None),
        (AzlinError::MapStale, 409, "AzlinMapStale", None),
    ];
    for (i, (code, status, s3, retry)) in before.into_iter().enumerate() {
        assert_eq!(code.s3(), (status, s3), "{code}");
        assert_eq!(code.retry_after(), retry, "{code}");
        // The binary encodings number the variants: the old ones keep their place.
        assert_eq!(AzlinError::ALL[i], code);
    }
}

#[test]
fn the_hello_reads_its_header_and_skips_parameters_it_does_not_know() {
    let hello = ClientHello::new(
        AppName::parse("AzDrive").expect("app"),
        SemVer::parse("0.3.0-beta.1").expect("version"),
    );
    assert_eq!(hello.header_value(), format!("azdrive/0.3.0; contract={CONTRACT_VERSION}"));
    assert_eq!(ClientHello::parse(&hello.header_value()), Ok(hello.clone()));
    let later = ClientHello::parse("azdrive/0.3.0; contract=3; arch=arm64").expect("reads");
    assert_eq!(later.contract, 3);
    for bad in ["", "AzDrive/0.2", "azdrive/0.3.0", "azdrive; contract=3", "a b/1.0.0; contract=3"] {
        assert!(ClientHello::parse(bad).is_err(), "{bad:?}");
    }
    assert!(SemVer::parse("0.3").is_err());
    assert!(SemVer::parse("0.3.0.1").is_err());
    assert!(SemVer::parse("0.10.0").expect("ok") > SemVer::parse("0.9.9").expect("ok"));
}

#[test]
fn an_idempotency_key_reads_back_as_itself_and_refuses_other_text() {
    let key = IdempotencyKey::fresh([0xff; 16]);
    assert_eq!(key.header_value().len(), 22);
    assert_eq!(IdempotencyKey::parse(&key.header_value()), Ok(key));
    let zero = IdempotencyKey::fresh([0; 16]);
    assert_eq!(zero.header_value(), "AAAAAAAAAAAAAAAAAAAAAA");
    // The last character's low bits are padding: a key that sets them is not one we wrote.
    for bad in ["", "AAAAAAAAAAAAAAAAAAAAAB", "AAAAAAAAAAAAAAAAAAAAA", "AAAAAAAAAAAAAAAAAAAA+A"] {
        assert!(IdempotencyKey::parse(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn the_digest_names_the_fixtures() {
    let digest = fixtures_digest();
    assert!(digest.starts_with("fnv1a64:"));
    assert_eq!(digest.len(), "fnv1a64:".len() + 16);
    assert_eq!(digest, fixtures_digest());
}

#[test]
fn a_refresh_refusal_reads_the_token_server_s_body_and_says_whether_to_send_again() {
    let read = |body: &str| -> RefreshRefusal { serde_json::from_str(body).expect("reads") };
    let reuse = read(r#"{"error":"token_reuse","message":"an old token was reused"}"#);
    assert_eq!(reuse, RefreshRefusal::TokenReuse);
    assert_eq!(reuse.status(), 401);
    let before = read(r#"{"error":"unavailable","message":"database","spent":false}"#);
    assert_eq!(before, RefreshRefusal::Unavailable { spent: Some(false) });
    assert!(before.spent_nothing());
    let maybe = read(r#"{"error":"unavailable","message":"database"}"#);
    assert_eq!(maybe, RefreshRefusal::Unavailable { spent: None });
    assert!(!maybe.spent_nothing(), "it may have been spent");
    let spent = read(r#"{"error":"unavailable","spent":true}"#);
    assert!(!spent.spent_nothing());
    assert!(read(r#"{"error":"try_again","spent":false}"#).spent_nothing());
    assert!(read(r#"{"error":"not_verified"}"#).spent_nothing());
    let pending = read(r#"{"error":"lockdown_pending","pending_until":null}"#);
    assert_eq!(pending, RefreshRefusal::LockdownPending { pending_until: None });
    let banned = read(
        r#"{"error":"drive_banned","status":"banned","ban_reason":"spam","ban_until":"2026-10-22T08:00:00Z"}"#,
    );
    assert!(matches!(banned, RefreshRefusal::DriveBanned(ref b) if b.ban_reason.as_deref() == Some("spam")));
    assert_eq!(read(r#"{"error":"teapot","message":"new"}"#), RefreshRefusal::Unknown);
}
