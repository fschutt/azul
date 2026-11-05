//! What this computer paid for (ENT27-B06): the sale the token server booked is read from the
//! approved checkout's answer and kept - a record of its own, before the checkout leaves the
//! keyring's list. Before this the `receipt` field was read nowhere.

use std::sync::Arc;

use azul_storage::{
    keyring::{KeyringNamespace, MemoryKeyring},
    testing::TempDir,
};

use super::{bundle, json, Fake, Shared, TOKEN};
use crate::{
    claim::seal,
    pending::{self, Final, PaidList, PaidRecord, PendingCheckout, Polled, Transient},
    ClaimKey, SharedKeyring, TokenServer,
};

fn shared(dir: &TempDir) -> SharedKeyring {
    SharedKeyring::with_lock_root(
        Arc::new(MemoryKeyring::new()),
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    )
}

/// The booked sale of `ck_1`: EUR 9.90 at 19 % VAT, a direct debit.
const RECEIPT: &str = r#"{"issue_day": "2026-11-05", "service": "Azlin Storage 100GB, 12 months",
    "gross_cents": 990, "currency": "EUR", "rate_permille": 190, "country": "DE",
    "scheme": "sepa", "checkout_id": "ck_1", "kind": "payment_receipt"}"#;

/// The approved answer of `ck_1` for a new drive, sealed to `claim`, with `extra` JSON fields.
fn approved(claim: &ClaimKey, extra: &str) -> Arc<Fake> {
    let sealed = seal(
        &claim.public_base64(),
        "ck_1",
        bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa").as_bytes(),
    );
    let body = format!(
        r#"{{"checkout_id": "ck_1", "status": "approved", "sealed_signup": "{sealed}",
            "tier": "100GB", "months": 12, "months_paid": 12{extra}}}"#
    );
    Fake::new(move |_, _| Ok(json(200, &body)))
}

/// ENT27-B06: the poll that opens the sign-up keeps the sale the answer names, for the drive it
/// bought, while the checkout is still on the list - and from the record the charge is read.
#[test]
fn ent27_b06_a_paid_checkout_is_recorded_with_its_receipt_before_it_leaves_the_list() {
    let dir = TempDir::new("azcloud-paid");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&shared, &ck).unwrap();
    let transport = Shared(approved(&claim, &format!(r#", "receipt": {RECEIPT}"#)));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &shared, &ck) {
        Polled::Final(Final::Claimed(claimed)) => assert_eq!(claimed.bundle.drive_id(), "d_1"),
        other => panic!("not claimed: {other:?}"),
    }
    assert_eq!(
        pending::list(&shared).unwrap().len(),
        1,
        "still on the list"
    );
    let record = shared
        .paid()
        .of_checkout("ck_1")
        .unwrap()
        .expect("the record of the payment");
    assert_eq!(
        (
            record.checkout.as_str(),
            record.drive.as_str(),
            record.tier.as_str(),
            record.months
        ),
        ("ck_1", "d_1", "100GB", 12)
    );
    let charge = record.charge().expect("the booked amount");
    assert_eq!((charge.amount, charge.currency.as_str()), (990, "EUR"));
    let receipt = record.receipt.as_ref().expect("the receipt");
    assert_eq!(
        (
            receipt.rate_permille,
            receipt.country.as_str(),
            receipt.kind.as_str()
        ),
        (190, "DE", "payment_receipt")
    );
    // Asked again (the claim is idempotent): one record, the first moment kept.
    let first = record.at;
    assert!(matches!(
        pending::poll(&server, &shared, &ck),
        Polled::Final(Final::Claimed(_))
    ));
    let records = shared.paid().records().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].at, first);
    // The file is for its user alone.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(shared.paid().path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
    }
}

/// A token server that books no sale answers no receipt: the record is made all the same (what
/// was paid for which drive), without a charge - nothing is made up.
#[test]
fn ent27_b06_a_token_server_without_a_receipt_leaves_a_record_without_one() {
    let dir = TempDir::new("azcloud-paid");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&shared, &ck).unwrap();
    let transport = Shared(approved(&claim, ""));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        pending::poll(&server, &shared, &ck),
        Polled::Final(Final::Claimed(_))
    ));
    let record = shared
        .paid()
        .of_checkout("ck_1")
        .unwrap()
        .expect("a record");
    assert_eq!((record.drive.as_str(), record.months), ("d_1", 12));
    assert!(record.receipt.is_none());
    assert_eq!(record.charge(), None, "no receipt, no amount");
}

/// A record that cannot be kept holds the claim back: the drive is not claimed this poll, the
/// checkout stays on the list, and the next poll tries again.
#[test]
fn ent27_b06_a_record_that_cannot_be_kept_holds_the_claim_back() {
    let dir = TempDir::new("azcloud-paid");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&shared, &ck).unwrap();
    // A file where the folder of the list should be.
    let folder = shared.paid().path().parent().unwrap().to_path_buf();
    std::fs::create_dir_all(folder.parent().unwrap()).unwrap();
    std::fs::write(&folder, b"in the way").unwrap();
    let transport = Shared(approved(&claim, &format!(r#", "receipt": {RECEIPT}"#)));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &shared, &ck) {
        Polled::Transient(Transient::NotKept(why)) => assert!(why.contains("record"), "{why}"),
        other => panic!("not held back: {other:?}"),
    }
    assert_eq!(pending::list(&shared).unwrap().len(), 1);
    // The way is free again: the next poll claims it.
    std::fs::remove_file(&folder).unwrap();
    assert!(matches!(
        pending::poll(&server, &shared, &ck),
        Polled::Final(Final::Claimed(_))
    ));
    assert!(shared.paid().of_checkout("ck_1").unwrap().is_some());
}

/// The list: a record of the same checkout is updated (the first moment stays; one that names a
/// receipt replaces one that does not), others are appended, a damaged file is an error and
/// never an empty list.
#[test]
fn ent27_b06_the_list_updates_a_checkout_and_never_reads_a_damaged_file_as_empty() {
    let dir = TempDir::new("azcloud-paid");
    let list = PaidList::in_dir(&dir.path().join("paid"));
    assert!(list.records().unwrap().is_empty());
    let plain = PaidRecord {
        checkout: String::from("ck_1"),
        drive: String::from("d_1"),
        tier: String::from("100GB"),
        months: 1,
        at: 100,
        receipt: None,
    };
    list.keep(&plain).unwrap();
    let mut later = plain.clone();
    later.at = 200;
    later.receipt = Some(serde_json::from_str(RECEIPT).unwrap());
    list.keep(&later).unwrap();
    let mut other = plain.clone();
    other.checkout = String::from("ck_2");
    list.keep(&other).unwrap();
    let records = list.records().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].at, 100, "the first moment stays");
    assert!(
        records[0].receipt.is_some(),
        "the receipt replaced the bare record"
    );
    // A bare record does not wipe a receipt.
    list.keep(&plain).unwrap();
    assert!(list.of_checkout("ck_1").unwrap().unwrap().receipt.is_some());
    std::fs::write(list.path(), b"{ not json").unwrap();
    assert!(list.records().is_err());
    assert!(
        list.keep(&plain).is_err(),
        "nothing is written over a file it cannot read"
    );
}
