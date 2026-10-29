//! What each type refuses, and what it reads that is not in a fixture.

use azlin_contract::{
    fixtures_digest, AppName, AzlinError, BucketName, Cents, ClientHello, Currency, Deadline,
    DeadlineKind, DriveId, IdempotencyKey, Months, RefreshRefusal, Rfc3339, SemVer, TierId,
    CONTRACT_VERSION,
};
use azlin_contract::{client_too_old, ClientMinimum, ClientTooOld, MIN_CONTRACT};

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
fn a_client_below_the_minimum_is_told_what_to_reach_and_one_without_a_hello_is_no_app() {
    assert!(MIN_CONTRACT <= CONTRACT_VERSION, "the services speak the contract they demand");
    let floor = SemVer::parse("0.3.1").expect("version");
    let minimum = ClientMinimum {
        contract: 3,
        versions: vec![(AppName::parse("azdrive").expect("app"), floor)],
    };
    let refused = |min_version| {
        Some(ClientTooOld {
            min_contract: 3,
            min_version,
        })
    };
    // From before the hello's format, or an older contract.
    assert_eq!(minimum.refuses(Some("azdrive/0.1")), refused(None));
    assert_eq!(minimum.refuses(Some("AzDrive/0.2")), refused(None));
    assert_eq!(minimum.refuses(Some("azmail/0.9.0; contract=2")), refused(None));
    // Below its app's version, with the version to reach.
    assert_eq!(minimum.refuses(Some("azdrive/0.3.0; contract=3")), refused(Some(floor)));
    assert_eq!(minimum.refuses(Some("azdrive/0.9.0; contract=2")), refused(Some(floor)));
    // At or above the minimum, a newer contract, another app's version, no hello at all.
    for header in ["azdrive/0.3.1; contract=3", "azdrive/1.0.0; contract=4", "azmail/0.1.0; contract=3"] {
        assert_eq!(minimum.refuses(Some(header)), None, "{header}");
    }
    assert_eq!(minimum.refuses(None), None);
    // The services' minimum answers this build's own hello.
    let ours = ClientHello::new(
        AppName::parse("azdrive").expect("app"),
        SemVer::parse("0.3.0").expect("version"),
    );
    assert_eq!(client_too_old(Some(&ours.header_value())), None);
    assert_eq!(client_too_old(Some("azdrive/0.1")), refused(None).map(|r| ClientTooOld { min_contract: MIN_CONTRACT, ..r }));
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

mod checkout {
    use azlin_contract::{
        AbandonAnswer, AbandonWire, CheckoutAnswer, CheckoutRefusal, CheckoutStatusWire, Months,
        Prepay, TierOffer,
    };

    #[test]
    fn a_yearly_only_tier_is_sold_for_12_or_24_months_and_has_no_monthly_price() {
        let tier: TierOffer = serde_json::from_str(
            r#"{"id": "12TB", "quota_bytes": 12000000000000, "price_cents_month": null,
                "price_cents_year": 34990, "currency": "EUR", "yearly_only": true,
                "methods": ["sepa", "card"], "first_month_free": true,
                "prepay_months": [24, 12]}"#,
        )
        .expect("a yearly-only tier");
        let months: Vec<u32> = tier.prepay.months().iter().map(|m| m.get()).collect();
        assert_eq!(months, [12, 24], "sorted");
        assert!(tier.yearly_only);
        assert_eq!(tier.price_cents_month, None);
        assert!(!tier.prepay.contains(Months::try_from(1).expect("1")));
        assert!(tier.takes("card") && !tier.takes("cash"));
    }

    #[test]
    fn a_tier_sold_for_no_period_or_an_odd_one_is_refused() {
        assert!(Prepay::new(Vec::new()).is_err());
        let refused = |prepay: &str| {
            serde_json::from_str::<TierOffer>(&format!(
                r#"{{"id": "1TB", "quota_bytes": 1, "price_cents_month": 1,
                    "price_cents_year": 10, "currency": "EUR", "prepay_months": {prepay}}}"#
            ))
            .is_err()
        };
        assert!(refused("[]"));
        assert!(refused("[2]"));
        assert!(!refused("[1]"));
    }

    #[test]
    fn a_status_a_refusal_or_an_abandon_answer_this_build_does_not_know_reads_as_unknown() {
        let answer: CheckoutAnswer = serde_json::from_str(
            r#"{"checkout_id": "ck_1", "status": "on_hold", "tier": "1TB", "months": 1,
                "amount_cents": 499}"#,
        )
        .expect("an answer of a newer server");
        assert_eq!(answer.status, CheckoutStatusWire::Unknown);
        let refusal: CheckoutRefusal =
            serde_json::from_str(r#"{"error": "needs_a_selfie", "message": "smile"}"#)
                .expect("a refusal of a newer server");
        assert_eq!(refusal, CheckoutRefusal::Unknown);
        let abandon: AbandonWire =
            serde_json::from_str(r#"{"checkout_id": "ck_1", "status": "frozen"}"#)
                .expect("an abandon answer of a newer server");
        assert_eq!(abandon.answer, AbandonAnswer::Unknown);
        assert!(!abandon.answer.is_over(), "an unknown answer never lets the claim key go");
    }

    #[test]
    fn only_an_abandon_that_ended_the_checkout_lets_its_claim_key_go() {
        for over in [
            AbandonAnswer::Expired,
            AbandonAnswer::Declined,
            AbandonAnswer::Reversed,
            AbandonAnswer::Rejected,
        ] {
            assert!(over.is_over(), "{over:?}");
        }
        for open in [AbandonAnswer::StillPending, AbandonAnswer::Unknown] {
            assert!(!open.is_over(), "{open:?}");
        }
        let paid: CheckoutRefusal = serde_json::from_str(
            r#"{"error": "checkout_paid", "message": "the drive comes with the payment"}"#,
        )
        .expect("the refusal of a paid checkout's abandon");
        assert_eq!(paid, CheckoutRefusal::CheckoutPaid);
        assert_eq!(paid.status(), 409);
    }

    #[test]
    fn every_refusal_word_reads_back_as_its_refusal() {
        for refusal in CheckoutRefusal::ALL {
            if refusal == CheckoutRefusal::Unknown {
                assert_eq!(CheckoutRefusal::parse("unknown"), None);
                continue;
            }
            assert_eq!(CheckoutRefusal::parse(refusal.as_str()), Some(refusal.clone()));
            let wire = serde_json::to_value(&refusal).expect("serializes");
            assert_eq!(wire["error"], refusal.as_str());
        }
        assert_eq!(CheckoutRefusal::parse("blocked_country").map(|r| r.status()), Some(403));
    }

    #[test]
    fn an_approved_checkout_without_its_sealed_sign_up_says_so() {
        let answer: CheckoutAnswer = serde_json::from_str(
            r#"{"checkout_id": "ck_1", "status": "approved", "tier": "1TB", "months": 12,
                "amount_cents": 4990, "months_paid": 12}"#,
        )
        .expect("an approved checkout whose hand-over is gone");
        assert_eq!(answer.status, CheckoutStatusWire::Approved { sealed_signup: None });
        assert_eq!(answer.months_paid, Some(12));
    }
}

mod period {
    use azlin_contract::{IssueRefusal, IssueSchedule, Rfc3339, INSTALMENT_SECS};

    const APPROVED: i64 = 1_791_255_600;

    #[test]
    fn a_prepay_is_issued_one_month_at_a_time_from_its_approval() {
        assert_eq!(INSTALMENT_SECS, 30 * 86_400);
        assert_eq!(IssueSchedule::due_by(12, APPROVED, APPROVED), 1, "the first at once");
        let day = 86_400;
        assert_eq!(IssueSchedule::due_by(12, APPROVED, APPROVED + 29 * day), 1);
        assert_eq!(IssueSchedule::due_by(12, APPROVED, APPROVED + 30 * day), 2);
        assert_eq!(IssueSchedule::due_by(12, APPROVED, APPROVED + 95 * day), 4);
        assert_eq!(IssueSchedule::due_by(3, APPROVED, APPROVED + 400 * day), 3, "never more than paid");
        assert_eq!(IssueSchedule::due_by(3, APPROVED, APPROVED - day), 1, "a clock behind");
        let mid = IssueSchedule::at(12, 2, APPROVED);
        assert_eq!(mid.next_issue_at, Some(Rfc3339::from_unix(APPROVED + 2 * INSTALMENT_SECS)));
        assert!(!mid.is_complete());
        assert!(mid.may_ask(APPROVED + 2 * INSTALMENT_SECS));
        assert!(!mid.may_ask(APPROVED + 2 * INSTALMENT_SECS - 1));
        let done = IssueSchedule::at(3, 3, APPROVED);
        assert_eq!(done.next_issue_at, None);
        assert!(done.is_complete());
        assert!(!done.may_ask(APPROVED + 400 * day));
    }

    #[test]
    fn an_issue_refusal_says_whether_asking_again_can_ever_help() {
        let read = |body: &str| -> IssueRefusal { serde_json::from_str(body).expect("reads") };
        // a reversed payment, a wrong key, a checkout gone, all months out: never
        for word in [
            "not_paid",
            "issue_key_wrong",
            "no_such_checkout",
            "already_issued",
            "mandate_stopped",
            "issue_key_required",
            "key_id_required",
            "bad_request",
        ] {
            let refusal = read(&format!(r#"{{"error":"{word}","message":"why"}}"#));
            assert!(refusal.is_final(), "{word} ends the grant");
            assert_eq!(refusal.as_str(), word);
        }
        // later, blinded anew, or a server that is busy or newer: asked again
        let not_due = read(
            r#"{"error":"not_due","message":"m","schedule":{"months":3,"issued":1,"next_issue_at":"2026-11-21T08:00:00Z"}}"#,
        );
        assert!(!not_due.is_final());
        assert_eq!(not_due.status(), 409);
        let changed = read(r#"{"error":"key_changed","key_id":"100GB/2027","public_key_pem":null}"#);
        assert!(!changed.is_final());
        for word in ["issuer_key_missing", "try_again", "unavailable", "a_new_word"] {
            assert!(!read(&format!(r#"{{"error":"{word}"}}"#)).is_final(), "{word}");
        }
        assert_eq!(read(r#"{"error":"a_new_word"}"#), IssueRefusal::Unknown);
        assert_eq!(IssueRefusal::NotPaid.status(), 409);
        assert_eq!(IssueRefusal::IssueKeyWrong.status(), 403);
        assert_eq!(IssueRefusal::NoSuchCheckout.status(), 404);
        assert_eq!(IssueRefusal::TryAgain.status(), 503);
    }
}

mod read_only_and_map {
    use azlin_contract::{
        is_service_metadata, map_epoch, AzlinError, BucketOp, DriveAccess, ReadOnlyWhy,
        MAP_HEADER, MOVED_HEADER,
    };

    /// What `access` takes: (reads, writes, deletes, links).
    fn takes(access: DriveAccess) -> (bool, bool, bool, bool) {
        (
            access.permits(BucketOp::Read),
            access.permits(BucketOp::Write),
            access.permits(BucketOp::Delete),
            access.permits(BucketOp::Link),
        )
    }

    /// One table says what each state of a drive takes, for the nodes and the apps alike: a
    /// drive frozen by the operator or for a move takes its owner's deletes as an unpaid one
    /// does, and no other write; a lockdown takes no change; a locked drive nothing.
    #[test]
    fn one_table_says_what_each_state_of_a_drive_takes() {
        use DriveAccess::{Banned, Closed, ReadOnly, Writable};
        assert_eq!(takes(Writable), (true, true, true, true));
        assert_eq!(takes(ReadOnly(ReadOnlyWhy::Unpaid)), (true, false, true, true));
        assert_eq!(takes(ReadOnly(ReadOnlyWhy::Operator)), (true, false, true, true));
        assert_eq!(takes(ReadOnly(ReadOnlyWhy::Moving)), (true, false, true, true));
        assert_eq!(takes(ReadOnly(ReadOnlyWhy::LockdownPending)), (true, false, false, true));
        assert_eq!(takes(ReadOnly(ReadOnlyWhy::Archived)), (false, false, false, false));
        // a reason a newer server wrote: reads only, until this build knows it
        assert_eq!(takes(ReadOnly(ReadOnlyWhy::Unknown)), (true, false, false, true));
        assert_eq!(takes(Banned), (true, false, false, false));
        assert_eq!(takes(Closed), (false, false, false, false));
        for why in ReadOnlyWhy::ALL {
            assert_eq!(!ReadOnly(why).permits(BucketOp::Read), why.blocks_reads(), "{why:?}");
        }
    }

    #[test]
    fn a_drive_frozen_for_a_move_has_its_own_reason_and_code_and_older_builds_still_read_it() {
        assert_eq!(ReadOnlyWhy::parse("moving"), Some(ReadOnlyWhy::Moving));
        assert_eq!(ReadOnlyWhy::Moving.as_str(), "moving");
        assert_eq!(ReadOnlyWhy::Moving.code(), AzlinError::ReadOnlyMoving);
        assert_eq!(AzlinError::ReadOnlyMoving.as_str(), "read_only_moving");
        assert_eq!(AzlinError::parse("read_only_moving"), Some(AzlinError::ReadOnlyMoving));
        assert_eq!(AzlinError::ReadOnlyMoving.s3(), (403, "AccessDenied"));
        assert!(!AzlinError::ReadOnlyMoving.is_transient());
        assert_ne!(ReadOnlyWhy::Moving.code(), ReadOnlyWhy::Operator.code());
        // the next reason a newer server writes still reads, as a freeze this build keeps
        let read: ReadOnlyWhy = serde_json::from_str("\"rebalancing\"").expect("reads");
        assert_eq!(read, ReadOnlyWhy::Unknown);
    }

    #[test]
    fn a_moved_objects_marker_is_the_services_own_metadata_and_never_the_users() {
        assert_eq!(MOVED_HEADER, "x-amz-meta-azlin-moved");
        assert!(is_service_metadata(MOVED_HEADER));
        assert!(is_service_metadata("X-Amz-Meta-Azlin-Moved"));
        assert!(is_service_metadata("x-amz-meta-azlin-anything-later"));
        assert!(!is_service_metadata("x-amz-meta-author"));
        assert!(!is_service_metadata("x-amz-meta-azlinfan"));
        assert!(!is_service_metadata("content-type"));
    }

    #[test]
    fn each_reason_a_bucket_is_read_only_has_its_own_node_code() {
        assert_eq!(ReadOnlyWhy::Unpaid.code(), AzlinError::ReadOnlyUnpaid);
        // not the token server's lockdown_pending: that one tells the recovering computer to
        // wait, this one warns the owner's devices
        assert_eq!(ReadOnlyWhy::LockdownPending.code(), AzlinError::ReadOnlyLockdown);
        assert_eq!(ReadOnlyWhy::Operator.code(), AzlinError::ReadOnlyOperator);
        for why in ReadOnlyWhy::ALL {
            assert_eq!(ReadOnlyWhy::parse(why.as_str()), Some(why));
        }
        // a row written before the reason existed is a payment that did not come
        assert_eq!(ReadOnlyWhy::default(), ReadOnlyWhy::Unpaid);
        assert_eq!(ReadOnlyWhy::parse("frozen_by_a_newer_server"), None);
        let read: ReadOnlyWhy = serde_json::from_str("\"frozen_by_a_newer_server\"").expect("reads");
        assert_eq!(read, ReadOnlyWhy::Unknown);
        assert_eq!(ReadOnlyWhy::Unknown.code(), AzlinError::ReadOnlyOperator);
    }

    #[test]
    fn only_a_locked_drive_stops_reads_and_says_so_with_its_own_code() {
        assert_eq!(ReadOnlyWhy::Archived.code(), AzlinError::DriveArchived);
        assert_eq!(ReadOnlyWhy::parse("archived"), Some(ReadOnlyWhy::Archived));
        assert_eq!(AzlinError::DriveArchived.as_str(), "drive_archived");
        assert_eq!(AzlinError::DriveArchived.s3(), (403, "AccessDenied"));
        assert!(!AzlinError::DriveArchived.is_transient(), "asking again changes nothing");
        for why in ReadOnlyWhy::ALL {
            assert_eq!(why.blocks_reads(), why == ReadOnlyWhy::Archived, "{why:?}");
        }
        // a reason a newer server wrote stops writes only, never reads
        assert!(!ReadOnlyWhy::Unknown.blocks_reads());
    }

    #[test]
    fn the_new_read_only_codes_come_after_the_old_ones() {
        let n = AzlinError::ALL.len();
        let newest = [
            AzlinError::ReadOnlyLockdown,
            AzlinError::ReadOnlyOperator,
            AzlinError::DriveArchived,
            AzlinError::ReadOnlyMoving,
        ];
        assert_eq!(&AzlinError::ALL[n - 4..], &newest[..]);
        for code in [AzlinError::ReadOnlyLockdown, AzlinError::ReadOnlyOperator] {
            assert_eq!(code.s3(), (403, "AccessDenied"), "{code}");
            assert_eq!(code.retry_after(), None, "{code}");
            assert_eq!(AzlinError::parse(code.as_str()), Some(code));
        }
        assert_eq!(AzlinError::ReadOnlyLockdown.as_str(), "read_only_lockdown");
        assert_eq!(AzlinError::ReadOnlyOperator.as_str(), "read_only_operator");
    }

    #[test]
    fn the_map_header_names_an_epoch_and_a_nodes_answer_its_epoch_with_its_hash() {
        assert_eq!(MAP_HEADER, "x-azlin-map");
        assert_eq!(map_epoch("7"), Some(7));
        assert_eq!(map_epoch(" 8 9f86d081884c7d65 "), Some(8));
        assert_eq!(map_epoch(""), None);
        assert_eq!(map_epoch("seven"), None);
        assert_eq!(map_epoch("-1 00"), None);
    }
}

mod renewal {
    use azlin_contract::{
        expiry, DeadlineKind, IssueSchedule, RedeemRefusal, RenewalSignup, Rfc3339,
        INSTALMENT_SECS, READ_ONLY_SECS, WRITABLE_GRACE_SECS,
    };

    const APPROVED: i64 = 1_791_255_600;
    const DAY: i64 = 86_400;

    #[test]
    fn a_monthly_plan_paid_up_waits_for_its_next_payment_instead_of_ending() {
        let paid_up = IssueSchedule::recurring_at(2, 2, APPROVED, APPROVED + 40 * DAY);
        assert!(!paid_up.is_complete(), "a running monthly plan is never complete");
        assert_eq!(
            paid_up.next_issue_at,
            Some(Rfc3339::from_unix(APPROVED + 2 * INSTALMENT_SECS))
        );
        assert!(paid_up.may_ask(APPROVED + 2 * INSTALMENT_SECS));
        // a payment that is late: asked again a day later, not at once in a loop
        let late = IssueSchedule::recurring_at(2, 2, APPROVED, APPROVED + 90 * DAY);
        assert_eq!(late.next_issue_at, Some(Rfc3339::from_unix(APPROVED + 91 * DAY)));
        // a month paid and not issued yet is due as with a prepay
        let owed = IssueSchedule::recurring_at(3, 2, APPROVED, APPROVED + 70 * DAY);
        assert_eq!(owed, IssueSchedule::at(3, 2, APPROVED));
    }

    #[test]
    fn an_unpaid_drive_turns_read_only_then_locks() {
        let until = APPROVED;
        let [end, read_only, locked] = expiry(until);
        assert_eq!(end.kind, DeadlineKind::PeriodEnd);
        assert_eq!(end.at.unix(), until);
        assert_eq!(read_only.kind, DeadlineKind::ReadOnlyStart);
        assert_eq!(read_only.at.unix(), until + WRITABLE_GRACE_SECS);
        assert_eq!(WRITABLE_GRACE_SECS, 14 * DAY);
        assert_eq!(locked.kind, DeadlineKind::ReadOnlyEnd);
        assert_eq!(locked.at.unix(), until + WRITABLE_GRACE_SECS + READ_ONLY_SECS);
    }

    #[test]
    fn a_signup_with_a_drive_is_no_renewal() {
        let drive = r#"{"drive":{"id":"d1"},"tier":"100GB","period_tokens":{"checkout_id":"ck","months":1,"issue_key":"k"}}"#;
        assert!(serde_json::from_str::<RenewalSignup>(drive).is_err());
        let renewal = r#"{"renewal_of":null,"grant":"period_tokens","tier":"100GB","period_tokens":{"checkout_id":"ck","months":1,"issue_key":"k","recurring":true}}"#;
        let read: RenewalSignup = serde_json::from_str(renewal).expect("a renewal");
        assert!(read.period_tokens.recurring);
        assert_eq!(read.renewal_of, None);
    }

    #[test]
    fn a_refused_tier_change_keeps_the_token() {
        let not_due: RedeemRefusal = serde_json::from_str(
            r#"{"error":"change_not_due","message":"m","from":"2026-11-14T00:00:00Z"}"#,
        )
        .expect("reads");
        assert_eq!(not_due.status(), 409);
        assert!(!not_due.is_useless());
        let full: RedeemRefusal = serde_json::from_str(
            r#"{"error":"does_not_fit","stored":5,"quota":4}"#,
        )
        .expect("reads");
        assert!(!full.is_useless());
        for word in ["token_used", "bad_token", "token_expired", "unknown_issuer"] {
            let refusal: RedeemRefusal =
                serde_json::from_str(&format!(r#"{{"error":"{word}"}}"#)).expect("reads");
            assert!(refusal.is_useless(), "{word}");
        }
        let newer: RedeemRefusal =
            serde_json::from_str(r#"{"error":"a_new_word"}"#).expect("reads");
        assert_eq!(newer, RedeemRefusal::Unknown);
    }
}

mod archive {
    use azlin_contract::{
        archive_statement, device_key_statement, months_owed, paid_through, pay_through,
        ARCHIVE_STATEMENT_FORMAT, DEVICE_KEY_STATEMENT_FORMAT,
        period_after_restore, ArchiveRefusal, ArchiveState, ArchiveWire, RedeemRefusal,
        unpaid_promise, RefreshRefusal, UnpaidDays, INSTALMENT_SECS, LOCK_AFTER_SECS,
        READ_ONLY_SECS, RESTORE_OWED_SECS, UNPAID_DAYS, WRITABLE_GRACE_SECS,
    };

    const DAY: i64 = 86_400;
    const UNTIL: i64 = 1_791_255_600;

    #[test]
    fn a_drive_locks_104_days_after_its_period_and_its_owner_pays_three_months_to_get_it_back() {
        assert_eq!(LOCK_AFTER_SECS, WRITABLE_GRACE_SECS + READ_ONLY_SECS);
        assert_eq!(LOCK_AFTER_SECS, 104 * DAY);
        assert_eq!(RESTORE_OWED_SECS, 3 * INSTALMENT_SECS);
        let locked = UNTIL + LOCK_AFTER_SECS;
        let through = pay_through(locked);
        assert_eq!(through, UNTIL + 90 * DAY, "three months after its old period end");
        assert_eq!(months_owed(UNTIL, through), 3);
        assert_eq!(months_owed(UNTIL + 2 * INSTALMENT_SECS, through), 1);
        assert_eq!(months_owed(through, through), 0);
        assert!(!paid_through(UNTIL + 2 * INSTALMENT_SECS, through));
        assert!(paid_through(UNTIL + 3 * INSTALMENT_SECS, through), "a three-month checkout");
        // back two years later with six months paid: the three beyond start now, the two years
        // in offline storage cost nothing
        let now = locked + 730 * DAY;
        let paid = UNTIL + 6 * INSTALMENT_SECS;
        assert_eq!(period_after_restore(paid, through, now), now + 90 * DAY);
        assert_eq!(period_after_restore(through, through, now), now);
    }

    /// The promise every text that states it uses (the backend's documents, AzDrive): its days
    /// come from the constants, and it names no time of keeping the offline copy.
    #[test]
    fn the_unpaid_promise_says_the_contracts_days_and_no_time_of_keeping() {
        assert_eq!(
            UNPAID_DAYS,
            UnpaidDays {
                grace: WRITABLE_GRACE_SECS / DAY,
                read_only: READ_ONLY_SECS / DAY,
                lock: LOCK_AFTER_SECS / DAY,
            }
        );
        assert_eq!(UNPAID_DAYS.grace + UNPAID_DAYS.read_only, UNPAID_DAYS.lock);
        let UnpaidDays { grace, read_only, lock } = UNPAID_DAYS;
        assert_eq!(
            unpaid_promise(),
            format!(
                "{grace} days after the paid period ends, the drive becomes read-only. {read_only} \
                 days after that (day {lock}), it is locked and its files are moved to one offline \
                 copy. Paying restores them."
            )
        );
        assert_eq!(
            unpaid_promise(),
            "14 days after the paid period ends, the drive becomes read-only. 90 days after that \
             (day 104), it is locked and its files are moved to one offline copy. Paying restores \
             them."
        );
        for never in ["year", "month", "kept", "deleted"] {
            assert!(!unpaid_promise().contains(never), "{never}");
        }
    }

    #[test]
    fn a_proof_signs_its_purpose_the_drive_the_nonce_the_expiry_and_the_payment_line_by_line() {
        let statement = archive_statement("d_x", "n0nce", 1_792_569_900, Some("ck_1"));
        assert_eq!(
            statement,
            "azlin-archive-restore-v1\ndrive=d_x\nnonce=n0nce\nexpires=1792569900\ncheckout=ck_1\n"
        );
        assert!(statement.starts_with(ARCHIVE_STATEMENT_FORMAT));
        // every line counts: another drive, nonce, expiry or payment is another statement
        for other in [
            archive_statement("d_y", "n0nce", 1_792_569_900, Some("ck_1")),
            archive_statement("d_x", "other", 1_792_569_900, Some("ck_1")),
            archive_statement("d_x", "n0nce", 1_792_569_901, Some("ck_1")),
            archive_statement("d_x", "n0nce", 1_792_569_900, Some("ck_2")),
            archive_statement("d_x", "n0nce", 1_792_569_900, None),
        ] {
            assert_ne!(other, statement);
        }
        assert!(archive_statement("d_x", "n", 1, None).ends_with("checkout=\n"));
    }

    #[test]
    fn a_device_key_s_registration_signs_its_own_purpose_drive_and_key() {
        let statement = device_key_statement("d_x", "pk==");
        assert_eq!(statement, "azlin-device-key-v1\ndrive=d_x\nkey=pk==\n");
        assert!(statement.starts_with(DEVICE_KEY_STATEMENT_FORMAT));
        assert_ne!(statement, device_key_statement("d_y", "pk=="));
        // never a restore's statement, never a lockdown's
        assert_ne!(DEVICE_KEY_STATEMENT_FORMAT, ARCHIVE_STATEMENT_FORMAT);
        assert!(!statement.starts_with("lockdown:"));
    }

    #[test]
    fn a_refused_refresh_of_a_locked_drive_says_where_it_stands_and_keeps_the_token() {
        let body = r#"{"error":"drive_archived","message":"locked","state":"restore_pending","locked_at":"2026-10-24T00:00:00Z","pay_through":"2026-10-24T00:00:00Z","proven":true}"#;
        let read: RefreshRefusal = serde_json::from_str(body).expect("reads");
        let RefreshRefusal::DriveArchived(ref wire) = read else {
            panic!("{read:?}");
        };
        assert_eq!(wire.state, ArchiveState::RestorePending);
        assert!(wire.proven);
        assert_eq!(read.status(), 403);
        assert!(!read.spent_nothing(), "nothing to send again: the drive is locked");
        let newer: ArchiveWire = serde_json::from_str(
            r#"{"state":"in_the_vault","locked_at":"2026-10-24T00:00:00Z","pay_through":"2026-10-24T00:00:00Z"}"#,
        )
        .expect("reads");
        assert_eq!(newer.state, ArchiveState::Unknown);
        assert!(!newer.proven, "no proof said is none");
    }

    #[test]
    fn a_wrong_key_a_replay_and_a_payment_without_proof_are_refused_in_their_own_words() {
        assert_eq!(ArchiveRefusal::BadSignature.status(), 401);
        assert_eq!(ArchiveRefusal::ChallengeUsed.status(), 409);
        assert_eq!(ArchiveRefusal::NoKey.status(), 403);
        let read: ArchiveRefusal =
            serde_json::from_str(r#"{"error":"challenge_used","message":"x"}"#).expect("reads");
        assert_eq!(read, ArchiveRefusal::ChallengeUsed);
        assert_eq!(read.as_str(), "challenge_used");
        let read: ArchiveRefusal = serde_json::from_str(r#"{"error":"teapot"}"#).expect("reads");
        assert_eq!(read, ArchiveRefusal::Unknown);
        let unpaid = RedeemRefusal::ArchiveProofRequired;
        assert_eq!(unpaid.status(), 403);
        assert!(!unpaid.is_useless(), "the token is kept for after the proof");
    }
}
