//! What each type refuses, and what it reads that is not in a fixture.

use azlin_contract::{
    fixtures_digest, AppName, AzlinError, BucketName, Cents, ClientHello, Currency, Deadline,
    DeadlineKind, DriveId, IdempotencyKey, Months, Rfc3339, SemVer, TierId, CONTRACT_VERSION,
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
