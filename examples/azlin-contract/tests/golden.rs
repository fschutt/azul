//! The golden fixtures: every type reads its fixture and writes exactly it again.
//! `AZ_BLESS=1 cargo test -p azlin-contract` writes the fixtures from the values here instead.

use std::{fmt::Debug, path::PathBuf};

use azlin_contract::{
    AppName, AzlinError, BanWire, BucketName, Cents, CheckoutId, ClientHello, Currency, Deadline,
    DeadlineKind, DriveId, FamilyId, IdempotencyKey, IdempotentScope, LogicalBytes, MemberId,
    Months, ReadOnlyWhy, RefreshRefusal, Rfc3339, SemVer, StoredBytes, TierId, FIXTURES,
};
use azlin_contract::{
    AbandonAnswer, AbandonWire, CheckoutAnswer, CheckoutRefusal, CheckoutStatusWire, DeclineWire,
    LadderRef, Legal, MailTo, Prepay, ReceiptWire, RetentionAddon, ScheduledChange, Settles,
    TierClosed, TierOffer, TierPriceWire, TiersAnswer,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn blessing() -> bool {
    std::env::var("AZ_BLESS").is_ok_and(|v| v == "1")
}

/// `value` against the fixture `name`: written when blessing, else read back and compared both
/// ways (the JSON it writes, the value the fixture reads as).
fn golden<T: Serialize + DeserializeOwned + PartialEq + Debug>(name: &str, value: &T) {
    let path = fixtures_dir().join(name);
    if blessing() {
        let text = serde_json::to_string_pretty(value).expect("serializes") + "\n";
        std::fs::write(&path, text).expect("writes the fixture");
        return;
    }
    let text = std::fs::read_to_string(&path).expect("the fixture exists");
    let read: T = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{name} does not read as its type: {e}"));
    assert_eq!(&read, value, "{name} reads as another value");
    let written = serde_json::to_value(value).expect("serializes");
    let expected: Value = serde_json::from_str(&text).expect("the fixture is JSON");
    assert_eq!(written, expected, "{name}: the JSON written is not the fixture");
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct ErrorRow {
    code: AzlinError,
    status: u16,
    s3: String,
    retry_after: Option<u32>,
}

#[test]
fn every_error_code_reads_and_writes_its_fixture() {
    let rows: Vec<ErrorRow> = AzlinError::ALL
        .into_iter()
        .map(|code| {
            let (status, s3) = code.s3();
            ErrorRow {
                code,
                status,
                s3: s3.to_string(),
                retry_after: code.retry_after(),
            }
        })
        .collect();
    golden("azlin_errors.json", &rows);
}

#[test]
fn the_client_hello_reads_and_writes_its_fixture() {
    let hello = |app: &str, version: &str| {
        ClientHello::new(
            AppName::parse(app).expect("app"),
            SemVer::parse(version).expect("version"),
        )
    };
    golden(
        "client_hello.json",
        &vec![hello("azdrive", "0.3.0"), hello("azmail", "0.1.0"), hello("azcloud", "1.2.3")],
    );
}

#[test]
fn every_deadline_kind_reads_and_writes_its_fixture() {
    let kinds = [
        DeadlineKind::Credentials,
        DeadlineKind::FamilyIdle,
        DeadlineKind::PeriodEnd,
        DeadlineKind::ReadOnlyEnd,
        DeadlineKind::ClaimWindow,
        DeadlineKind::HandoffPurge,
        DeadlineKind::LockdownEnd,
        DeadlineKind::ReadOnlyStart,
    ];
    let deadlines: Vec<Deadline> = kinds
        .into_iter()
        .enumerate()
        .map(|(i, kind)| Deadline {
            at: Rfc3339::from_unix(1_792_540_800 + 86_400 * i as i64),
            kind,
        })
        .collect();
    golden("deadlines.json", &deadlines);
}

#[test]
fn the_idempotency_key_reads_and_writes_its_fixture() {
    let mut bytes = [0u8; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = i as u8;
    }
    golden("idempotency_key.json", &IdempotencyKey::fresh(bytes));
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Scopes {
    scopes: Vec<IdempotentScope>,
    already_answered: String,
}

#[test]
fn the_idempotent_scopes_read_and_write_their_fixture() {
    for scope in IdempotentScope::ALL {
        let wire = serde_json::to_value(scope).expect("serializes");
        assert_eq!(wire, Value::from(scope.as_str()), "{scope:?}");
    }
    let scopes = Scopes {
        scopes: IdempotentScope::ALL.to_vec(),
        already_answered: azlin_contract::ALREADY_ANSWERED.to_string(),
    };
    golden("idempotent_scopes.json", &scopes);
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Ids {
    drive: DriveId,
    checkout: CheckoutId,
    family: FamilyId,
    member: MemberId,
    bucket: BucketName,
    tier: TierId,
    months: Vec<Months>,
    stored: StoredBytes,
    logical: LogicalBytes,
    price: Cents,
    at: Rfc3339,
}

#[test]
fn the_ids_and_units_read_and_write_their_fixture() {
    let ids = Ids {
        drive: DriveId::parse("d_k3f9x2mq7a").expect("drive"),
        checkout: CheckoutId::parse("ck_4n2v8r6tqe").expect("checkout"),
        family: FamilyId::parse("f7m3k9q2").expect("family"),
        member: MemberId::parse("recovery-pending").expect("member"),
        bucket: BucketName::parse("azlin-d-k3f9x2mq7a").expect("bucket"),
        tier: TierId::parse(" 1 tb ").expect("tier"),
        months: Months::ALL.to_vec(),
        stored: StoredBytes(107_374_182_400),
        logical: LogicalBytes(131_072),
        price: Cents {
            amount: 499,
            currency: Currency::parse("eur").expect("currency"),
        },
        at: Rfc3339::parse("2026-10-21T08:00:00Z").expect("time"),
    };
    golden("ids.json", &ids);
}

#[test]
fn every_refresh_refusal_reads_and_writes_its_fixture() {
    let refusals = vec![
        RefreshRefusal::Unauthorized,
        RefreshRefusal::TokenReuse,
        RefreshRefusal::CredentialsRevoked,
        RefreshRefusal::LockdownPending {
            pending_until: Some(Rfc3339::parse("2026-10-23T08:00:00Z").expect("time")),
        },
        RefreshRefusal::DriveBanned(BanWire {
            ban_reason: Some("spam distribution".into()),
            ban_until: Some(Rfc3339::parse("2026-10-22T08:00:00Z").expect("time")),
        }),
        RefreshRefusal::NotVerified,
        RefreshRefusal::TryAgain,
        RefreshRefusal::Unavailable { spent: Some(false) },
        RefreshRefusal::Unavailable { spent: None },
    ];
    golden("refresh_refusals.json", &refusals);
}

#[test]
fn the_fixture_folder_holds_exactly_the_listed_fixtures_as_built() {
    if blessing() {
        return;
    }
    let mut on_disk: Vec<String> = std::fs::read_dir(fixtures_dir())
        .expect("the fixtures folder")
        .map(|entry| entry.expect("an entry").file_name().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    let mut listed: Vec<String> = FIXTURES.iter().map(|(name, _)| name.to_string()).collect();
    listed.sort();
    assert_eq!(on_disk, listed, "every fixture is in FIXTURES, and only those");
    for (name, built) in FIXTURES {
        let disk = std::fs::read_to_string(fixtures_dir().join(name)).expect("reads");
        assert_eq!(&disk, built, "{name} changed since the crate was built");
    }
}

fn months(list: &[u32]) -> Vec<Months> {
    list.iter().map(|m| Months::try_from(*m).expect("months")).collect()
}

fn eur() -> Currency {
    Currency::parse("EUR").expect("eur")
}

/// A tier as `GET /v1/tiers` sends it, priced in euros.
fn tier(id: &str, quota: u64, month: Option<u64>, year: u64) -> TierOffer {
    let mut prices = std::collections::BTreeMap::new();
    prices.insert(
        eur(),
        TierPriceWire {
            month_cents: month.unwrap_or(0),
            year_cents: year,
        },
    );
    TierOffer {
        id: TierId::parse(id).expect("tier"),
        quota: StoredBytes(quota),
        price_cents_month: month,
        price_cents_year: Some(year),
        currency: eur(),
        prices,
        yearly_only: month.is_none(),
        methods: Vec::new(),
        first_month_free: true,
        prepay: Prepay::new(if month.is_none() {
            months(&[12, 24])
        } else {
            months(&[1, 3, 6, 12, 24])
        })
        .expect("prepay"),
        closed: None,
        waitlist: false,
        reason: None,
    }
}

#[test]
fn the_tier_list_reads_and_writes_its_fixture() {
    let monthly = tier("100GB", 100_000_000_000, Some(99), 990);
    let yearly = TierOffer {
        methods: vec![String::from("sepa"), String::from("card")],
        ..tier("12TB", 12_000_000_000_000, None, 34_990)
    };
    let closed = TierOffer {
        closed: Some(TierClosed::CapacityFull),
        waitlist: true,
        reason: Some(String::from("temporarily_unavailable")),
        ..tier("1TB", 1_000_000_000_000, Some(499), 4990)
    };
    let mut moves = std::collections::BTreeMap::new();
    moves.insert(TierId::parse("100GB").expect("tier"), None);
    let answer = TiersAnswer {
        tiers: vec![monthly, yearly],
        unavailable: vec![closed],
        unavailable_reason: None,
        methods: vec![String::from("sepa"), String::from("card"), String::from("cash")],
        retention_addons: vec![RetentionAddon {
            id: String::from("90d"),
            days: 90,
            cents_per_tb_month: 99,
        }],
        legal: Legal {
            withdrawal_consent: Some(String::from("I agree that the service starts at once.")),
            sepa_prenotification: None,
            small_amount_invoice: true,
        },
        scheduled_changes: vec![ScheduledChange {
            id: 2,
            from_version: 3,
            to_version: 4,
            announced_at: Rfc3339::parse("2026-10-01T00:00:00Z").expect("time"),
            effective_at: Rfc3339::parse("2026-12-01T00:00:00Z").expect("time"),
            prices: moves,
            note: String::from("the 100GB tier ends"),
        }],
        ladder: Some(LadderRef {
            version: 3,
            currency: eur(),
            active_from: Some(Rfc3339::parse("2026-09-01T00:00:00Z").expect("time")),
        }),
    };
    golden("tiers_answer.json", &answer);
}

#[test]
fn every_checkout_status_reads_and_writes_its_fixture() {
    let id = || CheckoutId::parse("ck_4n2v8r6tqe").expect("checkout");
    let answer = |status: CheckoutStatusWire| CheckoutAnswer {
        checkout_id: id(),
        status,
        tier: String::from("100GB"),
        months: 12,
        amount_cents: 990,
        expires_at: None,
        provider: None,
        settles: None,
        months_paid: None,
        recurring: None,
        receipt: None,
    };
    let decline = DeclineWire {
        reason: Some(String::from("insufficient_funds")),
        code: Some(String::from("card_declined")),
        text: Some(String::from("Your card has insufficient funds.")),
    };
    let answers = vec![
        CheckoutAnswer {
            provider: Some(String::from("stripe")),
            settles: Some(Settles::Instant),
            months_paid: Some(0),
            recurring: Some(false),
            ..answer(CheckoutStatusWire::Pending {
                decline: Some(decline.clone()),
            })
        },
        CheckoutAnswer {
            expires_at: Some(Rfc3339::parse("2026-12-20T08:00:00Z").expect("time")),
            ..answer(CheckoutStatusWire::AwaitingCash {
                activation_code: Some(String::from("AZC1-TEST-CODE")),
                mail_to: Some(MailTo {
                    name: String::from("Azlin"),
                    lines: vec![String::from("Postfach 1"), String::from("10115 Berlin")],
                }),
            })
        },
        CheckoutAnswer {
            provider: Some(String::from("gocardless")),
            settles: Some(Settles::Days),
            months_paid: Some(12),
            recurring: Some(false),
            receipt: Some(ReceiptWire {
                issue_day: String::from("2026-10-21"),
                service: String::from("Azlin Storage 100GB, 12 months"),
                gross_cents: 990,
                currency: eur(),
                rate_permille: 190,
                country: String::from("DE"),
                scheme: String::from("oss"),
                checkout_id: id(),
                kind: String::from("small_amount_invoice"),
            }),
            ..answer(CheckoutStatusWire::Approved {
                sealed_signup: Some(String::from("c2VhbGVk")),
            })
        },
        answer(CheckoutStatusWire::Approved {
            sealed_signup: None,
        }),
        answer(CheckoutStatusWire::Declined {
            decline: Some(decline),
            reason: None,
        }),
        answer(CheckoutStatusWire::Reversed { reason: None }),
        answer(CheckoutStatusWire::Rejected {
            reason: Some(String::from("the envelope held EUR 5.00, not EUR 9.90")),
        }),
        answer(CheckoutStatusWire::Expired { reason: None }),
    ];
    golden("checkout_answers.json", &answers);
}

#[test]
fn every_abandon_answer_reads_and_writes_its_fixture() {
    let answers: Vec<AbandonWire> = [
        AbandonAnswer::Expired,
        AbandonAnswer::StillPending,
        AbandonAnswer::Declined,
        AbandonAnswer::Reversed,
        AbandonAnswer::Rejected,
    ]
    .into_iter()
    .map(|answer| AbandonWire {
        checkout_id: CheckoutId::parse("ck_4n2v8r6tqe").expect("checkout"),
        answer,
    })
    .collect();
    golden("abandon_answers.json", &answers);
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct RefusalRow {
    #[serde(flatten)]
    refusal: CheckoutRefusal,
    status: u16,
}

#[test]
fn every_checkout_refusal_reads_and_writes_its_fixture() {
    let rows: Vec<RefusalRow> = CheckoutRefusal::ALL
        .into_iter()
        .map(|refusal| match refusal {
            CheckoutRefusal::BadMonths { .. } => CheckoutRefusal::BadMonths {
                allowed: months(&[12, 24]),
            },
            CheckoutRefusal::TierUnavailable { .. } => {
                CheckoutRefusal::TierUnavailable { waitlist: true }
            }
            other => other,
        })
        .map(|refusal| RefusalRow {
            status: refusal.status(),
            refusal,
        })
        .collect();
    golden("checkout_refusals.json", &rows);
}

mod period {
    use azlin_contract::{IssueAnswer, IssueRefusal, IssueSchedule, Rfc3339};

    use super::golden;

    fn at(text: &str) -> Rfc3339 {
        Rfc3339::parse(text).expect("time")
    }

    #[test]
    fn the_issue_answers_read_and_write_their_fixture() {
        let answers = vec![
            IssueAnswer {
                tier: "100GB".into(),
                key_id: "100GB/2026".into(),
                public_key_pem: Some("-----BEGIN PUBLIC KEY-----\n...\n".into()),
                blind_signatures: vec!["c2ln".into()],
                schedule: IssueSchedule {
                    months: 12,
                    issued: 1,
                    next_issue_at: Some(at("2026-11-21T08:00:00Z")),
                },
            },
            IssueAnswer {
                tier: "1TB".into(),
                key_id: "1TB/2026".into(),
                public_key_pem: None,
                blind_signatures: vec!["c2ln".into()],
                schedule: IssueSchedule {
                    months: 1,
                    issued: 1,
                    next_issue_at: None,
                },
            },
        ];
        golden("issue_answers.json", &answers);
    }

    #[test]
    fn every_issue_refusal_reads_and_writes_its_fixture() {
        let refusals = vec![
            IssueRefusal::BadRequest,
            IssueRefusal::IssueKeyRequired,
            IssueRefusal::KeyIdRequired,
            IssueRefusal::IssueKeyWrong,
            IssueRefusal::NoSuchCheckout,
            IssueRefusal::NotPaid,
            IssueRefusal::MandateStopped,
            IssueRefusal::AlreadyIssued,
            IssueRefusal::NotDue {
                schedule: IssueSchedule {
                    months: 3,
                    issued: 1,
                    next_issue_at: Some(at("2026-11-21T08:00:00Z")),
                },
            },
            IssueRefusal::KeyChanged {
                key_id: "100GB/2027".into(),
                public_key_pem: Some("-----BEGIN PUBLIC KEY-----\n...\n".into()),
            },
            IssueRefusal::KeyChanged {
                key_id: "100GB/2027".into(),
                public_key_pem: None,
            },
            IssueRefusal::IssuerKeyMissing,
            IssueRefusal::TryAgain,
            IssueRefusal::Unavailable,
        ];
        golden("issue_refusals.json", &refusals);
    }
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct ReadOnlyRow {
    why: ReadOnlyWhy,
    code: AzlinError,
}

#[test]
fn every_read_only_reason_reads_and_writes_its_fixture() {
    let rows: Vec<ReadOnlyRow> = ReadOnlyWhy::ALL
        .into_iter()
        .map(|why| ReadOnlyRow { why, code: why.code() })
        .collect();
    golden("read_only_why.json", &rows);
}

mod renewal {
    use azlin_contract::{
        GrantKind, GrantWire, RedeemAnswer, RedeemRefusal, RenewalSignup, Rfc3339, StoredBytes,
    };

    use super::golden;

    fn grant(recurring: bool) -> GrantWire {
        GrantWire {
            checkout_id: "ck_renew".into(),
            months: 1,
            issue_key: "a2V5".into(),
            recurring,
        }
    }

    #[test]
    fn the_renewal_signups_read_and_write_their_fixture() {
        let signups = vec![
            RenewalSignup {
                renewal_of: Some("ck_first".into()),
                grant: GrantKind::PeriodTokens,
                tier: "100GB".into(),
                period_tokens: grant(false),
            },
            RenewalSignup {
                renewal_of: None,
                grant: GrantKind::PeriodTokens,
                tier: "1TB".into(),
                period_tokens: grant(true),
            },
        ];
        golden("renewal_signups.json", &signups);
    }

    #[test]
    fn the_redeem_answers_read_and_write_their_fixture() {
        let answers = vec![
            RedeemAnswer {
                period_until: Rfc3339::parse("2026-11-21T00:00:00Z").expect("time"),
                tier: None,
                credited_days: None,
            },
            RedeemAnswer {
                period_until: Rfc3339::parse("2026-12-05T00:00:00Z").expect("time"),
                tier: Some("1TB".into()),
                credited_days: Some(14),
            },
        ];
        golden("redeem_answers.json", &answers);
    }

    #[test]
    fn every_redeem_refusal_reads_and_writes_its_fixture() {
        let refusals = vec![
            RedeemRefusal::BadRequest,
            RedeemRefusal::WrongTier,
            RedeemRefusal::TokenExpired,
            RedeemRefusal::UnknownIssuer,
            RedeemRefusal::BadToken,
            RedeemRefusal::TokenUsed,
            RedeemRefusal::ChangeNotDue {
                from: Rfc3339::parse("2026-11-14T00:00:00Z").expect("time"),
            },
            RedeemRefusal::DoesNotFit {
                stored: StoredBytes(150_000_000_000),
                quota: StoredBytes(100_000_000_000),
            },
            RedeemRefusal::StoredBytesRequired,
            RedeemRefusal::TryAgain,
        ];
        golden("redeem_refusals.json", &refusals);
    }
}
