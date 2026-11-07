//! What this computer paid for (ENT27-B06): the sale the token server booked is read from the
//! approved checkout's answer and kept - a record of its own, once the drive is delivered. Before
//! this the `receipt` field was read nowhere. The record never holds a paid drive back
//! (X-DELTA-AZDRIVE-MONEY-2).

use std::sync::Arc;

use azul_storage::{
    keyring::{KeyringNamespace, KeyringStore, MemoryKeyring},
    testing::TempDir,
};

use super::{bundle, json, Fake, Shared, TOKEN};
use crate::{
    claim::seal,
    pending::{self, Final, PaidList, PaidRecord, PendingCheckout, Polled, Recorded},
    ClaimKey, SharedKeyring, TokenServer,
};

fn shared(dir: &TempDir) -> SharedKeyring {
    shared_with_keyring(dir).0
}

/// The keyring and the locks of one user, and the keyring itself to look into.
fn shared_with_keyring(dir: &TempDir) -> (SharedKeyring, Arc<MemoryKeyring>) {
    let keyring = Arc::new(MemoryKeyring::new());
    let shared = SharedKeyring::with_lock_root(
        keyring.clone(),
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    );
    (shared, keyring)
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
        .unwrap()
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
    let records = shared.paid().unwrap().records().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].at, first);
    // The file is for its user alone.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(shared.paid().unwrap().path())
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
        .unwrap()
        .of_checkout("ck_1")
        .unwrap()
        .expect("a record");
    assert_eq!((record.drive.as_str(), record.months), ("d_1", 12));
    assert!(record.receipt.is_none());
    assert_eq!(record.charge(), None, "no receipt, no amount");
}

/// X-DELTA-AZDRIVE-MONEY-2 (ENT27-B06): a record that cannot be kept does not hold the claim
/// back. The drive is delivered - its session is in the keyring, the poll answers the claim - and
/// the app is told that the receipt was not kept; the checkout stays on the list for the app to
/// finish, and the next poll records the sale once the way is free. (Before, the poll answered
/// `NotKept` at every try and the checkout sat on the list until the token server purged the
/// sealed sign-up after 30 days: the customer paid and had no drive.)
#[test]
fn x_delta_azdrive_money_2_a_record_that_cannot_be_kept_does_not_hold_the_claim_back() {
    let dir = TempDir::new("azcloud-paid");
    let (shared, keyring) = shared_with_keyring(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&shared, &ck).unwrap();
    // A file where the folder of the list should be.
    let folder = shared
        .paid()
        .unwrap()
        .path()
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::create_dir_all(folder.parent().unwrap()).unwrap();
    std::fs::write(&folder, b"in the way").unwrap();
    let transport = Shared(approved(&claim, &format!(r#", "receipt": {RECEIPT}"#)));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &shared, &ck) {
        Polled::Final(Final::Claimed(claimed)) => {
            assert_eq!(claimed.bundle.drive_id(), "d_1", "the drive is delivered");
            let why = claimed.record.not_kept().expect("the app is told");
            assert!(!why.is_empty(), "and told why");
            assert!(
                keyring
                    .get(&azul_storage::config::keyring_key("d_1"))
                    .unwrap()
                    .is_some(),
                "its session is in the keyring"
            );
        }
        other => panic!("held back: {other:?}"),
    }
    assert_eq!(
        pending::list(&shared).unwrap().len(),
        1,
        "the app finishes it"
    );
    // The way is free again: the next poll (the claim is idempotent) records the sale.
    std::fs::remove_file(&folder).unwrap();
    match pending::poll(&server, &shared, &ck) {
        Polled::Final(Final::Claimed(claimed)) => assert_eq!(claimed.record, Recorded::Kept),
        other => panic!("not claimed: {other:?}"),
    }
    assert!(shared
        .paid()
        .unwrap()
        .of_checkout("ck_1")
        .unwrap()
        .is_some());
}

/// X-DELTA-AZDRIVE-MONEY-2: a paid list that does not parse (a hand edit, a cut file, a backup
/// tool's partial restore) is set aside - its bytes stay as `paid.json.bad` - and a new one is
/// begun with the sale: one damaged file never keeps the next receipt from being kept, and an
/// approved checkout polls to `Final::Claimed` with the record kept. A second damaged list is
/// set aside beside the first, never over it.
#[test]
fn x_delta_azdrive_money_2_a_corrupt_paid_list_is_set_aside_and_the_claim_goes_through() {
    let dir = TempDir::new("azcloud-paid");
    let shared = shared(&dir);
    let list = shared.paid().unwrap();
    let cut: &[u8] = br#"{"format": "azcloud.paid", "records": [{"checkout": "ck_0", "dr"#;
    std::fs::create_dir_all(list.path().parent().unwrap()).unwrap();
    std::fs::write(list.path(), cut).unwrap();
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&shared, &ck).unwrap();
    let transport = Shared(approved(&claim, &format!(r#", "receipt": {RECEIPT}"#)));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &shared, &ck) {
        Polled::Final(Final::Claimed(claimed)) => {
            assert_eq!(claimed.record, Recorded::Kept, "{:?}", claimed.record);
        }
        other => panic!("not claimed: {other:?}"),
    }
    let aside = list.path().with_extension("json.bad");
    assert_eq!(std::fs::read(&aside).unwrap(), cut, "its bytes stay");
    let records = list.records().unwrap();
    assert_eq!(records.len(), 1, "a new list, begun with the sale");
    assert_eq!(records[0].checkout, "ck_1");
    // Damaged again: the first one stays, the second goes beside it.
    std::fs::write(list.path(), b"not json at all").unwrap();
    let mut later = records[0].clone();
    later.checkout = String::from("ck_2");
    list.keep(&later).unwrap();
    assert_eq!(
        std::fs::read(&aside).unwrap(),
        cut,
        "the first one is not replaced"
    );
    assert_eq!(
        std::fs::read(list.path().with_extension("json.bad.2")).unwrap(),
        b"not json at all"
    );
    assert_eq!(list.records().unwrap().len(), 1);
}

/// X-DELTA-AZDRIVE-MONEY-2: a list of another format is a newer build's: it is not set aside and
/// not written over (the older build must not hide a newer one's receipts), and the claim goes
/// through all the same, told as not kept.
#[test]
fn x_delta_azdrive_money_2_a_newer_builds_paid_list_is_left_as_it_is_and_the_claim_goes_through() {
    let dir = TempDir::new("azcloud-paid");
    let shared = shared(&dir);
    let list = shared.paid().unwrap();
    let newer: &[u8] = br#"{"format": "azcloud.paid.v2", "records": []}"#;
    std::fs::create_dir_all(list.path().parent().unwrap()).unwrap();
    std::fs::write(list.path(), newer).unwrap();
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&shared, &ck).unwrap();
    let transport = Shared(approved(&claim, &format!(r#", "receipt": {RECEIPT}"#)));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &shared, &ck) {
        Polled::Final(Final::Claimed(claimed)) => {
            let why = claimed.record.not_kept().expect("told");
            assert!(why.contains("azcloud.paid.v2"), "{why}");
        }
        other => panic!("not claimed: {other:?}"),
    }
    assert_eq!(std::fs::read(list.path()).unwrap(), newer, "left as it is");
    assert!(
        !list.path().with_extension("json.bad").exists(),
        "not set aside"
    );
}

/// X-DELTA-AZDRIVE-MONEY-2: a system that names no configuration folder keeps its locks in the
/// temp folder, and a receipt must not be kept there (a clean-up takes it): the paid list has no
/// home then. The claim goes through, told as not kept.
#[test]
fn x_delta_azdrive_money_2_the_paid_list_is_not_kept_in_the_temp_folder() {
    let dir = TempDir::new("azcloud-paid");
    let homeless = SharedKeyring::with_temp_lock_root(
        Arc::new(MemoryKeyring::new()),
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    );
    assert!(homeless.paid().is_err());
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&homeless, &ck).unwrap();
    let transport = Shared(approved(&claim, &format!(r#", "receipt": {RECEIPT}"#)));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &homeless, &ck) {
        Polled::Final(Final::Claimed(claimed)) => {
            let why = claimed.record.not_kept().expect("told");
            assert!(why.contains("configuration folder"), "{why}");
        }
        other => panic!("not claimed: {other:?}"),
    }
    assert!(
        !homeless.locks().path().join("paid").exists(),
        "nothing was written for the list"
    );
}

/// X-DELTA-AZDRIVE-MONEY-17: the receipt of a purchase is the buyer's. A computer that picks the
/// drive up with a claim code gets the drive and keeps no record of the sale - the amount, the
/// VAT country and the payment scheme of someone else's payment.
#[test]
fn x_delta_azdrive_money_17_a_drive_picked_up_with_a_claim_code_records_no_sale() {
    let dir = TempDir::new("azcloud-paid");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let mut ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    ck.picked_up = true;
    pending::add(&shared, &ck).unwrap();
    let transport = Shared(approved(&claim, &format!(r#", "receipt": {RECEIPT}"#)));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &shared, &ck) {
        Polled::Final(Final::Claimed(claimed)) => {
            assert_eq!(claimed.bundle.drive_id(), "d_1");
            assert_eq!(claimed.record, Recorded::NotBoughtHere);
        }
        other => panic!("not claimed: {other:?}"),
    }
    assert!(
        shared.paid().unwrap().records().unwrap().is_empty(),
        "the buyer's receipt is the buyer's"
    );
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
    // Reading never turns a file it cannot read into an empty list. Keeping a record sets that
    // file aside (its bytes stay) and begins a new list (X-DELTA-AZDRIVE-MONEY-2).
    std::fs::write(list.path(), b"{ not json").unwrap();
    assert!(list.records().is_err());
    assert!(list.of_checkout("ck_1").is_err());
    list.keep(&plain).unwrap();
    assert_eq!(
        std::fs::read(list.path().with_extension("json.bad")).unwrap(),
        b"{ not json"
    );
    assert_eq!(list.records().unwrap().len(), 1);
}
