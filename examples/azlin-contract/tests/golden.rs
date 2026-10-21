//! The golden fixtures: every type reads its fixture and writes exactly it again.
//! `AZ_BLESS=1 cargo test -p azlin-contract` writes the fixtures from the values here instead.

use std::{fmt::Debug, path::PathBuf};

use azlin_contract::{
    AppName, AzlinError, BucketName, Cents, CheckoutId, ClientHello, Currency, Deadline,
    DeadlineKind, DriveId, FamilyId, IdempotencyKey, LogicalBytes, MemberId, Months, Rfc3339,
    SemVer, StoredBytes, TierId, FIXTURES,
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
