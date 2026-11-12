//! The receipt of a paid checkout (ENT27-B06, X-DELTA-MONEY2-3): the sale is booked by the token
//! server a moment after the approval, and the answer that brings the drive may name no receipt
//! yet. The record is made from that answer, and the checkout used to leave the list at its
//! claim - nothing asked again, so the paid record said "no sale was booked" for nearly every
//! card purchase. A checkout whose record is not complete now stays on the list, past its
//! grant, until an answer names the receipt (three days at most).

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use azul_storage::{
    keyring::{KeyringNamespace, MemoryKeyring},
    testing::TempDir,
    Transport,
};

use super::{
    bundle, json,
    period::{issuing_server, keys, D1, ISSUE_KEY, N1, NOW},
    tokens_in_drive, Fake, Shared, TOKEN,
};
use crate::{
    bundle::PeriodTokens,
    claim::seal,
    pending::{
        self, Final, Finished, PendingCheckout, Polled, ReceiptDue, Receipted, Recorded,
        RECEIPT_WAIT_SECS,
    },
    period::PeriodTokenStore,
    ClaimKey, SharedKeyring, TokenServer,
};

/// The booked sale of `ck_1`: EUR 9.90 at 19 % VAT, a direct debit.
const RECEIPT: &str = r#"{"issue_day": "2026-11-05", "service": "Azlin Storage 100GB, 12 months",
    "gross_cents": 990, "currency": "EUR", "rate_permille": 190, "country": "DE",
    "scheme": "sepa", "checkout_id": "ck_1", "kind": "payment_receipt"}"#;

const DAY: i64 = 86_400;

fn shared(dir: &TempDir) -> SharedKeyring {
    SharedKeyring::with_lock_root(
        Arc::new(MemoryKeyring::new()),
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    )
}

/// The sealed sign-up of `ck_1`'s new drive `d_1`.
fn drive_signup(claim: &ClaimKey) -> String {
    seal(
        &claim.public_base64(),
        "ck_1",
        bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa").as_bytes(),
    )
}

/// The sealed sign-up of `ck_1` as a renewal of `d_1`'s months.
fn renewal_signup(claim: &ClaimKey) -> String {
    let signup = serde_json::json!({
        "renewal_of": "ck_0", "grant": "period_tokens", "tier": "100GB",
        "period_tokens": {"checkout_id": "ck_1", "months": 3, "issue_key": ISSUE_KEY},
    });
    seal(
        &claim.public_base64(),
        "ck_1",
        signup.to_string().as_bytes(),
    )
}

/// The token server of `ck_1`: its n-th answer about the checkout names a receipt when
/// `receipts[n]` is true (the last entry goes on for every later one); it issues `ck_1`'s months
/// as the period tests' server does. The server's journal is empty until an answer names the
/// sale. What it was asked about the checkout is counted.
fn server_of(sealed: String, receipts: &'static [bool]) -> (Arc<Fake>, Arc<AtomicUsize>) {
    let issuing = issuing_server(keys(), N1, D1, "100GB/2026");
    let asked = Arc::new(AtomicUsize::new(0));
    let counted = asked.clone();
    let fake = Fake::new(move |call, _| {
        if call.url.contains("/v1/checkout/ck_1") {
            let n = counted.fetch_add(1, Ordering::SeqCst);
            let receipt = if receipts[n.min(receipts.len() - 1)] {
                format!(r#", "receipt": {RECEIPT}"#)
            } else {
                String::new()
            };
            return Ok(json(
                200,
                &format!(
                    r#"{{"checkout_id": "ck_1", "status": "approved", "sealed_signup": "{sealed}",
                        "tier": "100GB", "months": 12, "months_paid": 12{receipt}}}"#
                ),
            ));
        }
        issuing.send(call)
    });
    (fake, asked)
}

fn grant() -> PeriodTokens {
    PeriodTokens {
        checkout_id: String::from("ck_1"),
        months: 3,
        issue_key: String::from(ISSUE_KEY),
        recurring: false,
    }
}

/// `ck_1` on the list, as the app has it after its drive is saved: claimed at `NOW`.
fn claimed_entry(shared: &SharedKeyring) -> PendingCheckout {
    pending::claimed(shared, "ck_1", "d_1", Some(&grant())).unwrap();
    let mut ck = pending::list(shared).unwrap().remove(0);
    let period = ck.period.as_mut().expect("claimed");
    period.claimed_at = i64::try_from(NOW).unwrap();
    period.counted_at = 0;
    pending::add(shared, &ck).unwrap();
    pending::list(shared).unwrap().remove(0)
}

/// The paid record of `ck_1`, if there is one.
fn record_of(shared: &SharedKeyring) -> Option<pending::PaidRecord> {
    shared.paid().unwrap().of_checkout("ck_1").unwrap()
}

fn poll_claimed(
    server: &TokenServer<'_>,
    shared: &SharedKeyring,
    ck: &PendingCheckout,
) -> Recorded {
    match pending::poll(server, shared, ck) {
        Polled::Final(Final::Claimed(claimed)) => claimed.record,
        other => panic!("not claimed: {other:?}"),
    }
}

/// ENT27-B06 / X-DELTA-MONEY2-3, the client half. The token server's journal is empty when the app
/// claims the drive (the answer names no receipt) and has the sale a moment later (the next answer
/// does): the record made at the claim has no receipt, and the finish - which used to take the
/// checkout off the list - asks once more and ends with the receipt in the paid record. On the old
/// code the record ends as it began: `receipt: None`, "no sale was booked".
#[test]
fn x_delta_money2_3_a_claim_then_a_finish_against_an_empty_journal_ends_with_the_receipt_in_the_record(
) {
    let dir = TempDir::new("azcloud-receipt");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&shared, &ck).unwrap();
    let (fake, asked) = server_of(drive_signup(&claim), &[false, true]);
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();

    // The claim: the drive is delivered, the record is made, and it names no sale.
    assert_eq!(poll_claimed(&server, &shared, &ck), Recorded::Kept);
    let record = record_of(&shared).expect("the record of the payment");
    assert_eq!((record.drive.as_str(), record.months), ("d_1", 12));
    assert!(record.receipt.is_none(), "the first answer named none");
    let listed = pending::list(&shared).unwrap();
    assert_eq!(
        listed[0].receipt_due,
        Some(ReceiptDue::Awaited),
        "an incomplete record marks the checkout"
    );

    // The finish asks the token server again, and the record has the receipt.
    let ck = claimed_entry(&shared);
    assert_eq!(
        ck.receipt_due,
        Some(ReceiptDue::Awaited),
        "the claim keeps the mark"
    );
    let owed = ck.period.clone().unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let (tokens, _) = tokens_in_drive(&store);
    match pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW) {
        Finished::Issued { drive_id, count } => assert_eq!((drive_id.as_str(), count), ("d_1", 1)),
        other => panic!("not issued: {other:?}"),
    }
    assert_eq!(
        asked.load(Ordering::SeqCst),
        2,
        "the poll, then the finish's look"
    );
    let record = record_of(&shared).expect("the record");
    let charge = record.charge().expect("the booked amount");
    assert_eq!((charge.amount, charge.currency.as_str()), (990, "EUR"));
    assert_eq!(record.receipt.as_ref().map(|r| r.rate_permille), Some(190));
    assert_eq!(
        shared.paid().unwrap().records().unwrap().len(),
        1,
        "one record"
    );
    assert!(
        pending::list(&shared).unwrap().is_empty(),
        "the receipt is in: the checkout is off the list"
    );
}

/// The journal is still empty when the finish asks: the grant is done (issued, kept with the
/// drive), the checkout STAYS on the list for its receipt, and the claims' round for it - which
/// does not touch the grant again - takes it off once an answer names the sale.
#[test]
fn x_delta_money2_3_a_checkout_without_a_receipt_stays_on_the_list_and_the_next_look_takes_it_off()
{
    let dir = TempDir::new("azcloud-receipt");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&shared, &ck).unwrap();
    // The poll and the finish's look name none; the third answer does.
    let (fake, asked) = server_of(drive_signup(&claim), &[false, false, true]);
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert_eq!(poll_claimed(&server, &shared, &ck), Recorded::Kept);
    let ck = claimed_entry(&shared);
    let owed = ck.period.clone().unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let (tokens, in_drive) = tokens_in_drive(&store);
    match pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW) {
        Finished::AwaitsReceipt(inner) => {
            assert!(
                matches!(*inner, Finished::Issued { count: 1, .. }),
                "{inner:?}"
            );
        }
        other => panic!("not waiting for its receipt: {other:?}"),
    }
    // The grant is in the drive's vault; the entry stays - claim secret and all - for its receipt.
    use crate::vault::TokenVault;
    assert!(in_drive.read().unwrap().grant("ck_1").is_some());
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed.len(), 1, "still on the list");
    assert_eq!(listed[0].receipt_due, Some(ReceiptDue::GrantKept));
    assert!(
        record_of(&shared).is_some_and(|r| r.receipt.is_none()),
        "a record, no sale yet"
    );

    // The next look (ten minutes on) asks about the checkout and nothing else.
    let calls = fake.calls().len();
    let before = asked.load(Ordering::SeqCst);
    let now = NOW + 600;
    match pending::await_receipt(&server, &shared, &listed[0], now) {
        Finished::Receipt(Receipted::Kept) => {}
        other => panic!("not kept: {other:?}"),
    }
    assert_eq!(asked.load(Ordering::SeqCst), before + 1);
    assert_eq!(
        fake.calls().len(),
        calls + 1,
        "no grant work, no issue: one question"
    );
    assert!(
        record_of(&shared).is_some_and(|r| r.receipt.is_some()),
        "the sale is recorded"
    );
    assert!(
        pending::list(&shared).unwrap().is_empty(),
        "off the list now"
    );
    // Another window's look at the same checkout finds it settled.
    assert!(matches!(
        pending::await_receipt(&server, &shared, &listed[0], now),
        Finished::Settled
    ));
}

/// A receipt that never comes is waited for [`RECEIPT_WAIT_SECS`] - counted in the time this
/// computer saw pass, from the claim - and then the checkout leaves the list with its record as it
/// is: its claim secret and issue key do not sit in the keyring for a sale nobody booked.
#[test]
fn x_delta_money2_3_the_wait_for_a_receipt_ends_after_three_days_with_one_last_look() {
    assert_eq!(RECEIPT_WAIT_SECS, 3 * DAY);
    let dir = TempDir::new("azcloud-receipt");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&shared, &ck).unwrap();
    let (fake, asked) = server_of(drive_signup(&claim), &[false]);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert_eq!(poll_claimed(&server, &shared, &ck), Recorded::Kept);
    let ck = claimed_entry(&shared);
    let owed = ck.period.clone().unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let (tokens, _) = tokens_in_drive(&store);
    assert!(matches!(
        pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW),
        Finished::AwaitsReceipt(_)
    ));
    let waiting = |at: i64| {
        let listed = pending::list(&shared).unwrap();
        pending::await_receipt(
            &server,
            &shared,
            &listed[0],
            NOW + u64::try_from(at).unwrap(),
        )
    };
    // A day on: still awaited, asked.
    let before = asked.load(Ordering::SeqCst);
    match waiting(DAY) {
        Finished::Receipt(Receipted::Waits(why)) => assert!(why.contains("no sale"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(asked.load(Ordering::SeqCst), before + 1);
    assert_eq!(pending::list(&shared).unwrap().len(), 1);
    // Four days after the claim: the wait is over; it asks one last time, and lets go.
    let before = asked.load(Ordering::SeqCst);
    match waiting(4 * DAY) {
        Finished::Receipt(Receipted::GaveUp) => {}
        other => panic!("{other:?}"),
    }
    assert_eq!(asked.load(Ordering::SeqCst), before + 1, "one last look");
    assert!(pending::list(&shared).unwrap().is_empty());
    assert!(
        record_of(&shared).is_some_and(|r| r.receipt.is_none()),
        "the record stays: the drive and what was paid for it"
    );
}

/// The wait for the receipt counts what this computer saw pass (AZ-P5, X-DELTA-AZDRIVE-MONEY-8):
/// a step BACK of the clock counts for nothing and does not lengthen the wait; a step forward
/// counts at most a week - which is more than the three days, so a clock set a month forward does
/// end this wait (the receipt is a courtesy, and the wait is short).
#[test]
fn x_delta_money2_3_a_step_of_the_clock_counts_in_the_receipt_wait_as_it_does_in_the_vault_wait() {
    let dir = TempDir::new("azcloud-receipt");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&shared, &ck).unwrap();
    let (fake, _) = server_of(drive_signup(&claim), &[false]);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert_eq!(poll_claimed(&server, &shared, &ck), Recorded::Kept);
    let ck = claimed_entry(&shared);
    let owed = ck.period.clone().unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let (tokens, _) = tokens_in_drive(&store);
    assert!(matches!(
        pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW),
        Finished::AwaitsReceipt(_)
    ));
    let look = |now: u64| {
        let listed = pending::list(&shared).unwrap();
        pending::await_receipt(&server, &shared, &listed[0], now)
    };
    // Five days BACK: the claim moment moves with the clock, nothing has passed - awaited.
    assert!(matches!(
        look(NOW - 5 * 86_400),
        Finished::Receipt(Receipted::Waits(_))
    ));
    // Then a month forward: a week of the step counts, and that is more than three days - over.
    assert!(matches!(
        look(NOW + 30 * 86_400),
        Finished::Receipt(Receipted::GaveUp)
    ));
    assert!(pending::list(&shared).unwrap().is_empty());
}

/// A claim whose answer already names the receipt asks nothing more: no mark, no second question
/// at the finish, the checkout leaves the list as it always did.
#[test]
fn x_delta_money2_3_an_answer_with_the_receipt_marks_nothing_and_the_finish_asks_nothing_more() {
    let dir = TempDir::new("azcloud-receipt");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&shared, &ck).unwrap();
    let (fake, asked) = server_of(drive_signup(&claim), &[true]);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert_eq!(poll_claimed(&server, &shared, &ck), Recorded::Kept);
    assert_eq!(pending::list(&shared).unwrap()[0].receipt_due, None);
    let ck = claimed_entry(&shared);
    let owed = ck.period.clone().unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let (tokens, _) = tokens_in_drive(&store);
    assert!(matches!(
        pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW),
        Finished::Issued { count: 1, .. }
    ));
    assert_eq!(asked.load(Ordering::SeqCst), 1, "the poll only");
    assert!(record_of(&shared).is_some_and(|r| r.receipt.is_some()));
    assert!(pending::list(&shared).unwrap().is_empty());
}

/// X-DELTA-AZDRIVE-MONEY-2's remainder (the review's helper B): a paid folder that cannot be
/// written at the claim moment means no record at all - `keep_paid` ran only inside the poll, and
/// the claims never poll a claimed checkout again, so the record was never made after the way was
/// free (the old test hid it by polling twice). The next look - the finish - writes it, with the
/// receipt the answer names, and the checkout leaves the list.
#[test]
fn x_delta_money2_3_a_paid_folder_that_could_not_be_written_at_the_claim_is_written_at_the_next_look(
) {
    let dir = TempDir::new("azcloud-receipt");
    let shared = shared(&dir);
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
    let (fake, _) = server_of(drive_signup(&claim), &[true]);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let record = poll_claimed(&server, &shared, &ck);
    assert!(record.not_kept().is_some(), "told: {record:?}");
    assert_eq!(
        pending::list(&shared).unwrap()[0].receipt_due,
        Some(ReceiptDue::Awaited),
        "an unwritten record marks the checkout, receipt or not"
    );
    // The way is free; nothing polls this checkout again - the app finishes it.
    std::fs::remove_file(&folder).unwrap();
    let ck = claimed_entry(&shared);
    let owed = ck.period.clone().unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let (tokens, _) = tokens_in_drive(&store);
    assert!(matches!(
        pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW),
        Finished::Issued { count: 1, .. }
    ));
    let record = record_of(&shared).expect("written at the next look");
    assert!(
        record.receipt.is_some(),
        "with the receipt the answer names"
    );
    assert_eq!((record.drive.as_str(), record.months), ("d_1", 12));
    assert!(pending::list(&shared).unwrap().is_empty());
}

/// The same with an answer that names no receipt either: the record is written without one when
/// the way is free (a record of what was paid for which drive), and the checkout waits for the
/// receipt - the retry of a failed write does not wait for the sale.
#[test]
fn x_delta_money2_3_a_record_that_could_not_be_written_is_made_even_while_the_receipt_is_awaited() {
    let dir = TempDir::new("azcloud-receipt");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&shared, &ck).unwrap();
    let folder = shared
        .paid()
        .unwrap()
        .path()
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::create_dir_all(folder.parent().unwrap()).unwrap();
    std::fs::write(&folder, b"in the way").unwrap();
    let (fake, _) = server_of(drive_signup(&claim), &[false]);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(poll_claimed(&server, &shared, &ck).not_kept().is_some());
    assert!(
        record_of_free(&shared, &folder).is_none(),
        "nothing recorded yet"
    );
    std::fs::remove_file(&folder).unwrap();
    let ck = claimed_entry(&shared);
    let owed = ck.period.clone().unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let (tokens, _) = tokens_in_drive(&store);
    assert!(matches!(
        pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW),
        Finished::AwaitsReceipt(_)
    ));
    let record = record_of(&shared).expect("the record is made");
    assert!(record.receipt.is_none());
    assert_eq!(
        pending::list(&shared).unwrap()[0].receipt_due,
        Some(ReceiptDue::GrantKept)
    );
}

/// What the paid list holds when its folder is a file in the way: nothing can be read.
fn record_of_free(shared: &SharedKeyring, folder: &std::path::Path) -> Option<pending::PaidRecord> {
    assert!(folder.is_file(), "the way is blocked");
    shared
        .paid()
        .ok()
        .and_then(|list| list.of_checkout("ck_1").ok().flatten())
}

/// A renewal's months are recorded and awaited the same way: the poll claims them, the record is
/// made from its answer, and a finish whose second answer names the sale ends with the receipt.
#[test]
fn x_delta_money2_3_a_renewals_receipt_is_awaited_too() {
    let dir = TempDir::new("azcloud-receipt");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let mut ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "");
    ck.for_drive = String::from("d_1");
    pending::add(&shared, &ck).unwrap();
    let (fake, asked) = server_of(renewal_signup(&claim), &[false, true]);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &shared, &ck) {
        Polled::Final(Final::Renewed {
            drive_id,
            months,
            record,
        }) => {
            assert_eq!((drive_id.as_str(), months), ("d_1", 3));
            assert_eq!(record, Recorded::Kept);
        }
        other => panic!("not claimed: {other:?}"),
    }
    assert!(record_of(&shared).is_some_and(|r| r.receipt.is_none()));
    let ck = pending::list(&shared).unwrap().remove(0);
    assert_eq!(ck.receipt_due, Some(ReceiptDue::Awaited));
    let mut ck = ck;
    let period = ck.period.as_mut().expect("its months wait for the drive");
    period.claimed_at = i64::try_from(NOW).unwrap();
    period.counted_at = 0;
    pending::add(&shared, &ck).unwrap();
    let ck = pending::list(&shared).unwrap().remove(0);
    let owed = ck.period.clone().unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let (tokens, _) = tokens_in_drive(&store);
    assert!(matches!(
        pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW),
        Finished::Issued { count: 1, .. }
    ));
    assert_eq!(asked.load(Ordering::SeqCst), 2);
    assert!(record_of(&shared).is_some_and(|r| r.receipt.is_some()));
    assert!(pending::list(&shared).unwrap().is_empty());
}

/// A drive picked up with a claim code records no sale (it is the buyer's), so no receipt is
/// awaited for it: no mark, no wait.
#[test]
fn x_delta_money2_3_a_drive_picked_up_with_a_claim_code_awaits_no_receipt() {
    let dir = TempDir::new("azcloud-receipt");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let mut ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    ck.picked_up = true;
    pending::add(&shared, &ck).unwrap();
    let (fake, _) = server_of(drive_signup(&claim), &[false]);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert_eq!(poll_claimed(&server, &shared, &ck), Recorded::NotBoughtHere);
    assert_eq!(pending::list(&shared).unwrap()[0].receipt_due, None);
}

/// A checkout whose grant is not known to be in two places is `finish`'s: the receipt's look does
/// not take it off the list, whatever it finds.
#[test]
fn x_delta_money2_3_the_receipt_look_never_ends_a_checkout_whose_grant_is_not_kept() {
    let dir = TempDir::new("azcloud-receipt");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&shared, &ck).unwrap();
    let (fake, asked) = server_of(drive_signup(&claim), &[true]);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert_eq!(poll_claimed(&server, &shared, &ck), Recorded::Kept);
    let ck = claimed_entry(&shared);
    let before = asked.load(Ordering::SeqCst);
    match pending::await_receipt(&server, &shared, &ck, NOW) {
        Finished::Kept(why) => assert!(why.contains("finish it first"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(asked.load(Ordering::SeqCst), before, "nothing was asked");
    assert_eq!(
        pending::list(&shared).unwrap().len(),
        1,
        "and nothing left the list"
    );
}

/// The room the list keeps for a claim holds the worst case: the longest drive id, 24 months of a
/// monthly plan, an issue key, both stamps of the wait - and the mark of a receipt that is still
/// awaited, which stays on the entry after the claim. Without it a claim could be refused for room
/// and the checkout would be stuck unclaimed.
#[test]
fn x_delta_money2_3_the_room_for_a_claim_holds_its_worst_case_with_the_receipt_mark() {
    let claim = ClaimKey::generate().unwrap();
    let bare = PendingCheckout::new("ck_00000000000000000000000000", &claim, "100GB", TOKEN, "A");
    let size = |ck: &PendingCheckout| serde_json::to_string(ck).unwrap().len();
    let mut worst = bare.clone();
    worst.receipt_due = Some(ReceiptDue::GrantKept);
    worst.period = Some(pending::PendingTokens {
        drive_id: "d".repeat(33),
        months: 24,
        issue_key: "k".repeat(43),
        recurring: true,
        claimed_at: 1_791_450_000,
        counted_at: 1_791_450_000,
    });
    let grown = size(&worst) - size(&bare);
    assert!(
        grown <= pending::PERIOD_ROOM,
        "a claim grows its entry by {grown} bytes, the list keeps {}",
        pending::PERIOD_ROOM
    );
}

/// `checkout_paid`: the payment as the token server tells it now, without a claim key and without
/// opening the sign-up - what the receipt's look asks. Approved is `Some` however the sign-up
/// stands (a purged one included), every other status is `None`, a refusal is an error and not an
/// end.
#[test]
fn x_delta_money2_3_checkout_paid_reads_the_payment_without_the_sign_up() {
    let body = |status: &str, extra: &str| {
        let status = status.to_string();
        let extra = extra.to_string();
        Fake::new(move |_, _| {
            Ok(json(
                200,
                &format!(
                    r#"{{"checkout_id": "ck_1", "status": "{status}", "tier": "100GB",
                        "months": 12, "months_paid": 12{extra}}}"#
                ),
            ))
        })
    };
    let ask = |fake: Arc<Fake>| {
        let transport = Shared(fake);
        TokenServer::new(TOKEN, &transport)
            .unwrap()
            .checkout_paid("ck_1")
    };
    // approved, its sign-up purged: still the payment, with its receipt
    let paid = ask(body("approved", &format!(r#", "receipt": {RECEIPT}"#)))
        .unwrap()
        .expect("approved");
    assert_eq!(
        (paid.months, paid.months_paid, paid.tier.as_str()),
        (12, Some(12), "100GB")
    );
    assert_eq!(paid.receipt.as_ref().map(|r| r.gross_cents), Some(990));
    assert!(ask(body("approved", ""))
        .unwrap()
        .is_some_and(|p| p.receipt.is_none()));
    for status in ["pending", "declined", "reversed", "expired"] {
        assert!(ask(body(status, "")).unwrap().is_none(), "{status}");
    }
    // another checkout's answer, and a refusal, are errors
    let other = Fake::new(|_, _| {
        Ok(json(
            200,
            r#"{"checkout_id": "ck_2", "status": "approved"}"#,
        ))
    });
    assert!(ask(other).is_err());
    let gone = Fake::new(|_, _| {
        Ok(json(
            404,
            r#"{"error": "no_such_checkout", "message": "no such checkout"}"#,
        ))
    });
    assert!(
        ask(gone).is_err(),
        "a refusal is not an answer about the payment"
    );
}

/// A list full of checkouts that wait for their receipts only does not refuse a new order: the
/// oldest of them gives its place (its grant is kept elsewhere; only a courtesy waits). A checkout
/// that still holds the only copy of its grant - the receipt AND the grant awaited - never does:
/// the order is refused as it was.
#[test]
fn x_delta_money2_3_a_checkout_that_waits_for_its_receipt_only_gives_its_place_to_a_new_order() {
    // `count` claimed checkouts of long names that wait for their receipts, `due` as given
    let entry = |n: usize, due: ReceiptDue| {
        let claim = ClaimKey::generate().unwrap();
        let mut ck = PendingCheckout::new(
            &format!("ck_{n:026}"),
            &claim,
            "100GB",
            TOKEN,
            &"A drive with a long name ".repeat(10),
        );
        ck.started_at = 1_791_450_000 + i64::try_from(n).unwrap();
        ck.receipt_due = Some(due);
        ck.period = Some(pending::PendingTokens {
            drive_id: format!("d_{n:024}"),
            months: 12,
            issue_key: String::from(ISSUE_KEY),
            recurring: false,
            claimed_at: 1_791_450_000,
            counted_at: 1_791_450_000,
        });
        ck
    };
    let order = |shared: &SharedKeyring| {
        let claim = ClaimKey::generate().unwrap();
        pending::begin(
            shared,
            &PendingCheckout::posting(&claim, "100GB", TOKEN, "New"),
        )
    };

    // grants not kept yet: their secrets stay, and an order that would not fit is refused
    let dir = TempDir::new("azcloud-receipt");
    let shared_open = shared(&dir);
    let mut fitting = 0;
    while pending::add(&shared_open, &entry(fitting, ReceiptDue::Awaited)).is_ok() {
        fitting += 1;
        assert!(fitting < 100, "the list never fills");
    }
    assert!(fitting >= 3, "{fitting}");
    assert!(matches!(
        order(&shared_open),
        Err(pending::AddError::NoRoom { .. })
    ));
    assert_eq!(
        pending::list(&shared_open).unwrap().len(),
        fitting,
        "none was given up"
    );

    // grants kept elsewhere: the same list is not "full" for an order - the oldest waiter goes
    let dir = TempDir::new("azcloud-receipt");
    let shared_kept = shared(&dir);
    for n in 0..fitting {
        pending::add(&shared_kept, &entry(n, ReceiptDue::GrantKept)).unwrap();
    }
    assert_eq!(pending::list(&shared_kept).unwrap().len(), fitting);
    assert!(
        order(&shared_kept).is_ok(),
        "a new order is not refused for a courtesy"
    );
    let listed = pending::list(&shared_kept).unwrap();
    assert!(
        listed.iter().any(|c| c.is_posting()),
        "the order is on the list"
    );
    assert!(
        !listed
            .iter()
            .any(|c| c.checkout_id == format!("ck_{:026}", 0)),
        "the oldest waiter gave its place"
    );
    assert!(
        listed
            .iter()
            .any(|c| c.checkout_id == format!("ck_{:026}", fitting - 1)),
        "the newest stays"
    );
}

// ==== X-DELTA-MONEY5-5: a look that could not be made is not an answer ====

/// What the token server does with one question about `ck_1`.
#[derive(Clone, Copy)]
enum Step {
    /// Approved, and the sale is booked: the receipt.
    Receipt,
    /// Approved, no sale booked yet.
    NoReceipt,
    /// The connection drops: no answer.
    Down,
    /// A 503: the token server is busy.
    Busy,
}

/// The token server of `ck_1`: its n-th answer about the checkout is `steps[n]` (the last goes
/// on); it issues `ck_1`'s months as the period tests' server does. What it was asked about the
/// checkout is counted.
fn scripted(sealed: String, steps: &'static [Step]) -> (Arc<Fake>, Arc<AtomicUsize>) {
    let issuing = issuing_server(keys(), N1, D1, "100GB/2026");
    let asked = Arc::new(AtomicUsize::new(0));
    let counted = asked.clone();
    let fake = Fake::new(move |call, _| {
        if call.url.contains("/v1/checkout/ck_1") {
            let n = counted.fetch_add(1, Ordering::SeqCst);
            let approved = |receipt: &str| {
                json(
                    200,
                    &format!(
                        r#"{{"checkout_id": "ck_1", "status": "approved", "sealed_signup": "{sealed}",
                            "tier": "100GB", "months": 12, "months_paid": 12{receipt}}}"#
                    ),
                )
            };
            return match steps[n.min(steps.len() - 1)] {
                Step::Receipt => Ok(approved(&format!(r#", "receipt": {RECEIPT}"#))),
                Step::NoReceipt => Ok(approved("")),
                Step::Down => Err(String::from("connection reset by peer")),
                Step::Busy => Ok(json(
                    503,
                    r#"{"error": "busy", "message": "try again in a moment"}"#,
                )),
            };
        }
        issuing.send(call)
    });
    (fake, asked)
}

/// A claimed entry whose grant is kept and that waits for its receipt only; `steps` are what the
/// token server answers - the claim's poll and the finish's look are the first two.
fn waiting_for_a_receipt(
    steps: &'static [Step],
) -> (TempDir, SharedKeyring, Arc<Fake>, Arc<AtomicUsize>) {
    let dir = TempDir::new("azcloud-receipt");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos");
    pending::add(&shared, &ck).unwrap();
    let (fake, asked) = scripted(drive_signup(&claim), steps);
    {
        let transport = Shared(fake.clone());
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        assert_eq!(poll_claimed(&server, &shared, &ck), Recorded::Kept);
        let ck = claimed_entry(&shared);
        let owed = ck.period.clone().unwrap();
        let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
        let (tokens, _) = tokens_in_drive(&store);
        assert!(matches!(
            pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW),
            Finished::AwaitsReceipt(_)
        ));
    }
    assert_eq!(
        asked.load(Ordering::SeqCst),
        2,
        "the poll and the finish's look"
    );
    (dir, shared, fake, asked)
}

/// X-DELTA-MONEY5-5. A laptop that was shut for three days is "over" at its first look after the
/// start, and that look was the one the wait gave up on: a connection that dropped after the wake,
/// a 503 - and the entry left the list with a record that never named its receipt; nothing asked
/// again, and the sale the token server books a day later never reached it. A look that could not
/// be made is not an answer: the wait goes on, a day apart, and the receipt that is booked after
/// all reaches the record. On the old code the first look after the three days ends it (`GaveUp`,
/// the entry gone).
#[test]
fn x_delta_money5_5_a_failed_look_after_the_three_days_is_not_the_end_of_the_wait() {
    let (_dir, shared, fake, asked) = waiting_for_a_receipt(&[
        Step::NoReceipt,
        Step::NoReceipt,
        Step::Down,
        Step::Busy,
        Step::Receipt,
    ]);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let look = |days: i64| {
        let listed = pending::list(&shared).unwrap();
        pending::await_receipt(
            &server,
            &shared,
            &listed[0],
            NOW + u64::try_from(days * DAY).unwrap(),
        )
    };
    // day 4: the wait is over - and the connection drops: no answer, no end
    match look(4) {
        Finished::Receipt(Receipted::LateWaits(why)) => {
            assert!(why.contains("connection reset"), "{why}");
        }
        other => panic!("a look that got no answer ended the wait: {other:?}"),
    }
    assert_eq!(pending::list(&shared).unwrap().len(), 1, "the entry stays");
    // day 5: a busy token server is no answer either
    assert!(matches!(
        look(5),
        Finished::Receipt(Receipted::LateWaits(_))
    ));
    assert_eq!(pending::list(&shared).unwrap().len(), 1, "and stays");
    assert!(
        record_of(&shared).is_some_and(|r| r.receipt.is_none()),
        "a record, no sale yet"
    );
    // day 6: the sale is booked after all, and reaches the record
    assert!(matches!(look(6), Finished::Receipt(Receipted::Kept)));
    assert_eq!(asked.load(Ordering::SeqCst), 5);
    assert!(
        record_of(&shared)
            .is_some_and(|r| r.receipt.as_ref().map(|r| r.rate_permille) == Some(190)),
        "the sale is recorded"
    );
    assert!(pending::list(&shared).unwrap().is_empty());
}

/// The failed look is not "over" before the three days either, and the retry has an end: a token
/// server that never answers is asked once a day for a week more, and then the entry goes with its
/// record as it is. An ANSWER of "no sale booked" after the three days ends the wait at once, as it
/// always did (the negative control).
#[test]
fn x_delta_money5_5_the_retry_of_a_look_that_cannot_be_made_ends_after_a_week_and_an_answer_ends_the_wait_at_once(
) {
    // never an answer: LateWaits from day 4, the end at day 10 (three days and a week)
    let (_dir, shared, fake, _) =
        waiting_for_a_receipt(&[Step::NoReceipt, Step::NoReceipt, Step::Down]);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let look = |days: i64| {
        let listed = pending::list(&shared).unwrap();
        pending::await_receipt(
            &server,
            &shared,
            &listed[0],
            NOW + u64::try_from(days * DAY).unwrap(),
        )
    };
    // within the three days: a look that cannot be made is the plain wait
    assert!(matches!(look(2), Finished::Receipt(Receipted::Waits(_))));
    for days in 4..10 {
        assert!(
            matches!(look(days), Finished::Receipt(Receipted::LateWaits(_))),
            "day {days}"
        );
    }
    assert_eq!(
        pending::list(&shared).unwrap().len(),
        1,
        "day 9: still waiting"
    );
    assert!(
        matches!(look(10), Finished::Receipt(Receipted::GaveUp)),
        "three days and a week: the end"
    );
    assert!(pending::list(&shared).unwrap().is_empty());
    assert!(
        record_of(&shared).is_some_and(|r| r.receipt.is_none()),
        "the record stays as it is"
    );

    // an answer, "no sale booked": the end at the first look after the three days
    let (_dir, shared, fake, _) = waiting_for_a_receipt(&[Step::NoReceipt]);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let listed = pending::list(&shared).unwrap();
    assert!(matches!(
        pending::await_receipt(
            &server,
            &shared,
            &listed[0],
            NOW + 4 * u64::try_from(DAY).unwrap()
        ),
        Finished::Receipt(Receipted::GaveUp)
    ));
    assert!(pending::list(&shared).unwrap().is_empty());
}

// ==== X-DELTA-MONEY5-6: a cut after any write of a renewal's claim loses no receipt ====

/// A keyring of a process that dies: the first `writes` writes of the checkouts' list land; the
/// next one, and every call after it - reads included - fails, as the keyring does for a process
/// that is gone. What the dead process left in `inner` is what the next start finds.
struct Dying {
    inner: Arc<MemoryKeyring>,
    writes_left: AtomicUsize,
    dead: std::sync::atomic::AtomicBool,
}

impl Dying {
    fn new(inner: &Arc<MemoryKeyring>, writes: usize) -> Dying {
        Dying {
            inner: inner.clone(),
            writes_left: AtomicUsize::new(writes),
            dead: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn alive(&self) -> Result<(), azul_storage::keyring::KeyringError> {
        if self.dead.load(Ordering::SeqCst) {
            return Err(azul_storage::keyring::KeyringError::Failed(String::from(
                "the process is gone",
            )));
        }
        Ok(())
    }
}

impl azul_storage::keyring::KeyringStore for Dying {
    fn get(&self, key: &str) -> Result<Option<String>, azul_storage::keyring::KeyringError> {
        self.alive()?;
        self.inner.get(key)
    }

    fn set(&self, key: &str, secret: &str) -> Result<(), azul_storage::keyring::KeyringError> {
        self.alive()?;
        if key == pending::PENDING_KEY {
            if self.writes_left.load(Ordering::SeqCst) == 0 {
                self.dead.store(true, Ordering::SeqCst);
                return self.alive();
            }
            self.writes_left.fetch_sub(1, Ordering::SeqCst);
        }
        self.inner.set(key, secret)
    }

    fn delete(&self, key: &str) -> Result<(), azul_storage::keyring::KeyringError> {
        self.alive()?;
        self.inner.delete(key)
    }
}

/// X-DELTA-MONEY5-6. A renewal's claim wrote its period FIRST, then the paid record, then the
/// receipt mark: a cut between the first and the last left a period with no mark, the claims never
/// poll an entry with a period again, and `finish` looks for a receipt only when the mark is set -
/// the proof of payment of that renewal was lost for good. The claim now records and marks first
/// (as a new drive's claim does) and writes the period last. The crash table: for every number of
/// writes of the list that land before the cut (0: the mark fails; 1: the period fails; 2 and more:
/// the claim completes), the entry the next start finds never has months to issue without its record
/// and its mark - and the next start ends with the receipt in the record and the entry off the list.
/// On the old code the cut after one write (the period) fails the first assertion.
#[test]
fn x_delta_money5_6_a_cut_after_any_write_of_a_renewal_s_claim_loses_no_receipt() {
    for writes in 0..=3 {
        let dir = TempDir::new("azcloud-receipt");
        let inner = Arc::new(MemoryKeyring::new());
        let locks = dir.path().join("locks");
        let healthy =
            SharedKeyring::with_lock_root(inner.clone(), KeyringNamespace::azlin_shared(), &locks);
        let claim = ClaimKey::generate().unwrap();
        let mut ck = PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "");
        ck.for_drive = String::from("d_1");
        pending::add(&healthy, &ck).unwrap();
        // no sale is booked at the claim and at the first look after the restart; the third
        // question names it
        let (fake, _) = server_of(renewal_signup(&claim), &[false, false, true]);
        let transport = Shared(fake);
        let server = TokenServer::new(TOKEN, &transport).unwrap();

        // the claim, cut after `writes` writes of the list
        let dying = SharedKeyring::with_lock_root(
            Arc::new(Dying::new(&inner, writes)),
            KeyringNamespace::azlin_shared(),
            &locks,
        );
        let _ = pending::poll(&server, &dying, &ck);
        let entry = pending::list(&healthy).unwrap().remove(0);
        if entry.period.is_some() {
            let record = record_of(&healthy);
            assert!(
                record.is_some(),
                "cut after {writes} writes: months to issue and no record of the payment"
            );
            assert!(
                entry.receipt_due.is_some()
                    || record.as_ref().is_some_and(|r| r.receipt.is_some()),
                "cut after {writes} writes: months to issue, a record without a receipt and no mark - \
                 nothing will ask for the receipt"
            );
        }

        // the next start: what the claims do - poll an unclaimed entry, finish a claimed one, ask
        // for the receipt of one whose grant is kept
        let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
        let (tokens, _) = tokens_in_drive(&store);
        let now = u64::try_from(crate::now()).unwrap();
        for round in 1..=8 {
            let Some(entry) = pending::list(&healthy).unwrap().into_iter().next() else {
                break;
            };
            let at = now + round * 600;
            match (&entry.period, entry.receipt_due) {
                (None, _) => match pending::poll(&server, &healthy, &entry) {
                    Polled::Final(Final::Renewed { .. }) => {}
                    other => panic!("cut after {writes} writes: {other:?}"),
                },
                (Some(_), Some(ReceiptDue::GrantKept)) => {
                    let _ = pending::await_receipt(&server, &healthy, &entry, at);
                }
                (Some(owed), _) => {
                    let _ = pending::finish(&server, &healthy, &store, &tokens, &entry, owed, at);
                }
            }
        }
        assert!(
            pending::list(&healthy).unwrap().is_empty(),
            "cut after {writes} writes: the entry is off the list in the end"
        );
        let record = record_of(&healthy).expect("a record");
        assert!(
            record.receipt.is_some(),
            "cut after {writes} writes: the receipt reached the record"
        );
    }
}
