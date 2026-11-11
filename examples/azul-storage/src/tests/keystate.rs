//! Key permits (ARCH29 C7): the truth table of `judge` as the specification, the observation of the
//! state, the cells that depend on what was read, the sentences beside the flows that say them today,
//! and the registry of what a drive key touches.
//!
//! The table is stated here as an exhaustive `match` over `(KeyState, Op)` without a catch-all: a new
//! state or a new operation does not compile until its cells are decided. The cells that carry a
//! finding of the key review (X-DELTA-AZDRIVE-KEYS-1, -2, -3, -6, -7, -8, -18, -32) are named where
//! they are.

use std::{collections::BTreeSet, sync::Arc};

use super::mem_bucket::MemBucket;
use crate::{
    crypto::{
        device::{self, add_recovery_code, setup_new_drive, MAX_EXTRA_RECOVERY_CODES},
        keys::{load_key_chain, RecoveryKdf},
        keystate::{
            compare, disagreements_at, judge, observe, owner, shadow, Change, Compared,
            Disagreement, Done, Erase, Evidence, Forget, Guard, Journal, Judged, Keeps, KeyState,
            KeyringFacts, Op, Owner, Permit, Read, Refusal, Reseal, Start, Stays, TokenFacts,
            VaultFacts, Verdict, Way, Why, CONSUMERS, MEMBERS,
        },
        DriveKey, KeyId,
    },
    encrypted::{IndexProvider, MemoryIndex, NameIndex},
    footprint::KeyEntry,
    keyring::{KeyringStore, MemoryKeyring},
    rotation, Drive, DriveError,
};

const DRIVE: &str = "d_ks";
const NOW: u64 = 1_000_000;

fn cheap() -> RecoveryKdf {
    RecoveryKdf::with_cost(64, 1, 1).unwrap()
}

// ==== The table ====

const NO_STAYS: &[Stays] = &[];

/// What a cell says.
#[derive(Debug, PartialEq)]
enum Cell {
    Permit(Way, &'static [Stays]),
    Refuse(Refusal),
    /// Remove drive: permitted only when the recovery code, the token server and the vault say so
    /// (`removal_*` below). With nothing asked it is permitted.
    Removal,
}

/// The refusal a torn or unknown state makes of every operation it refuses.
fn why_refuses(why: &Why) -> Refusal {
    match why {
        Why::OpensNothing => Refusal::OpensNothing,
        Why::OldKeyGone => Refusal::OldKeyGone,
        Why::Unreadable { what, why } => Refusal::Unreadable {
            what,
            why: why.clone(),
        },
        Why::NotAsked(what) => Refusal::NotAsked(what),
    }
}

/// THE SPECIFICATION: all 7 x 10 cells.
fn spec(state: &KeyState, op: Op) -> Cell {
    use KeyState as S;
    match (state, op) {
        // ---- Fresh: no key here, and the drive is not known to be encrypted
        (S::Fresh, Op::Setup) => Cell::Permit(Way::New, NO_STAYS),
        (
            S::Fresh,
            Op::Enroll | Op::Rotate | Op::AddCode | Op::RemoveCode | Op::Reencrypt | Op::Lockdown,
        ) => Cell::Refuse(Refusal::NoKey),
        (S::Fresh, Op::Remove) => Cell::Removal,
        (S::Fresh, Op::Recover) => Cell::Permit(Way::Plain, NO_STAYS),
        (S::Fresh, Op::RestoreAsOf) => Cell::Refuse(Refusal::NotEncrypted),

        // ---- Planned: a planned key may be the only copy (KEYS-3)
        (S::Planned, Op::Setup) => Cell::Permit(Way::Finish, NO_STAYS),
        (
            S::Planned,
            Op::Enroll
            | Op::Rotate
            | Op::Remove
            | Op::AddCode
            | Op::RemoveCode
            | Op::Recover
            | Op::Reencrypt
            | Op::RestoreAsOf
            | Op::Lockdown,
        ) => Cell::Refuse(Refusal::SetupUnfinished),

        // ---- Active: the drive key is here and no key change is under way
        (S::Active, Op::Setup) => Cell::Refuse(Refusal::EncryptedAlready),
        (
            S::Active,
            Op::Enroll | Op::AddCode | Op::RemoveCode | Op::Recover | Op::Reencrypt | Op::Lockdown,
        ) => Cell::Permit(Way::Plain, NO_STAYS),
        (S::Active, Op::Rotate) => Cell::Permit(Way::New, NO_STAYS),
        (S::Active, Op::Remove) => Cell::Removal,
        // A restore keeps the recorded key id (KEYS-18)
        (S::Active, Op::RestoreAsOf) => Cell::Permit(Way::Plain, &[Stays::RecordedKeyId]),

        // ---- Rotating: this device keeps the new key of a key change
        (S::Rotating(_), Op::Setup) => Cell::Refuse(Refusal::EncryptedAlready),
        (S::Rotating(_), Op::Enroll) => Cell::Permit(Way::Plain, NO_STAYS),
        // never a second new key (KEYS-1)
        (S::Rotating(Start::Resume), Op::Rotate) => Cell::Permit(Way::Resume, NO_STAYS),
        (S::Rotating(Start::Adopt), Op::Rotate) => Cell::Permit(Way::Adopt, NO_STAYS),
        // the recovery wrap may change (KEYS-7)
        (S::Rotating(_), Op::AddCode | Op::RemoveCode) => Cell::Refuse(Refusal::WrapMayChange),
        // the keyring's new key may be the only copy (KEYS-2)
        (S::Rotating(_), Op::Remove | Op::Recover | Op::Reencrypt | Op::RestoreAsOf) => {
            Cell::Refuse(Refusal::KeyChangeUnfinished)
        }
        // a rotation of this device has had its lockdown; a journal that is not its own has not
        (S::Rotating(Start::Resume), Op::Lockdown) => Cell::Refuse(Refusal::AlreadyLockedDown),
        (S::Rotating(Start::Adopt), Op::Lockdown) => Cell::Permit(Way::Plain, NO_STAYS),

        // ---- Torn: the keys here cannot start or finish anything
        (S::Torn(_), Op::Setup) => Cell::Refuse(Refusal::EncryptedAlready),
        // the way out: a code from the device that changed the keys, or the recovery code
        (S::Torn(_), Op::Recover) => Cell::Permit(Way::Plain, NO_STAYS),
        (
            S::Torn(why),
            Op::Enroll
            | Op::Rotate
            | Op::Remove
            | Op::AddCode
            | Op::RemoveCode
            | Op::Reencrypt
            | Op::RestoreAsOf
            | Op::Lockdown,
        ) => Cell::Refuse(why_refuses(why)),

        // ---- Locked: encrypted, and this device has no key
        (S::Locked, Op::Recover) => Cell::Permit(Way::Plain, NO_STAYS),
        (
            S::Locked,
            Op::Setup
            | Op::Enroll
            | Op::Rotate
            | Op::Remove
            | Op::AddCode
            | Op::RemoveCode
            | Op::Reencrypt
            | Op::RestoreAsOf
            | Op::Lockdown,
        ) => Cell::Refuse(Refusal::Locked),

        // ---- Unknown: a read failed, and a failed read is never "nothing there" (KEYS-6)
        (S::Unknown(_), Op::Lockdown) => Cell::Permit(Way::Plain, NO_STAYS),
        (
            S::Unknown(why),
            Op::Setup
            | Op::Enroll
            | Op::Rotate
            | Op::Remove
            | Op::AddCode
            | Op::RemoveCode
            | Op::Recover
            | Op::Reencrypt
            | Op::RestoreAsOf,
        ) => Cell::Refuse(why_refuses(why)),
    }
}

/// A state's variant (not its payload): the seven rows.
fn row(state: &KeyState) -> &'static str {
    match state {
        KeyState::Fresh => "fresh",
        KeyState::Planned => "planned",
        KeyState::Active => "active",
        KeyState::Rotating(_) => "rotating",
        KeyState::Torn(_) => "torn",
        KeyState::Locked => "locked",
        KeyState::Unknown(_) => "unknown",
    }
}

/// One state of every row, and each payload that a cell depends on.
fn states() -> Vec<KeyState> {
    vec![
        KeyState::Fresh,
        KeyState::Planned,
        KeyState::Active,
        KeyState::Rotating(Start::Resume),
        KeyState::Rotating(Start::Adopt),
        KeyState::Torn(Why::OpensNothing),
        KeyState::Torn(Why::OldKeyGone),
        KeyState::Locked,
        KeyState::Unknown(Why::Unreadable {
            what: "the drive key",
            why: String::from("the keyring refused"),
        }),
        KeyState::Unknown(Why::NotAsked("the drive key")),
    ]
}

#[test]
fn the_table_is_the_specification_and_judge_answers_all_seventy_cells_by_it() {
    let mut cells = BTreeSet::new();
    for state in states() {
        for op in Op::ALL {
            let e = Evidence::new(DRIVE, NOW);
            let got = judge(&state, op, &e);
            match (spec(&state, op), got) {
                (Cell::Permit(way, stays), Verdict::Permit(permit)) => {
                    assert_eq!(permit.op(), op, "{state:?} x {op:?}");
                    assert_eq!(permit.drive(), DRIVE, "{state:?} x {op:?}");
                    assert_eq!(permit.way(), way, "{state:?} x {op:?}");
                    assert_eq!(permit.stays(), stays, "{state:?} x {op:?}");
                }
                (Cell::Refuse(want), Verdict::Refuse(reason)) => {
                    assert_eq!(reason.kind, want, "{state:?} x {op:?}");
                    assert!(
                        !reason.sentence.is_empty(),
                        "every refusal carries its sentence: {state:?} x {op:?}"
                    );
                    assert_eq!(reason.sentence, want.sentence(DRIVE));
                }
                (Cell::Removal, Verdict::Permit(permit)) => {
                    // nothing asked, nothing refused: the conditions are `removal_*` below
                    assert_eq!(permit.op(), Op::Remove);
                    assert_eq!(
                        permit.stays(),
                        &[Stays::TheBucket, Stays::SyncedFiles, Stays::ContactKeys],
                        "Remove says what it leaves"
                    );
                }
                (want, got) => panic!("{state:?} x {op:?}: the table says {want:?}, judge {got:?}"),
            }
            cells.insert((row(&state), op.name()));
        }
    }
    assert_eq!(cells.len(), 70, "seven states, ten operations");
}

#[test]
fn the_ten_operations_are_all_listed_once() {
    let names: BTreeSet<&str> = Op::ALL.iter().map(|op| op.name()).collect();
    assert_eq!(names.len(), 10);
    assert_eq!(Op::ALL.len(), 10);
}

/// KEYS-1: the cell that finds the new key of an earlier try permits the way that goes on with it.
#[test]
fn x_delta_azdrive_keys_1_a_rotation_that_is_under_way_goes_on_with_its_key_and_makes_no_second_one(
) {
    let e = Evidence::new(DRIVE, NOW);
    for (start, way) in [(Start::Resume, Way::Resume), (Start::Adopt, Way::Adopt)] {
        match judge(&KeyState::Rotating(start), Op::Rotate, &e) {
            Verdict::Permit(permit) => assert_eq!(permit.way(), way),
            Verdict::Refuse(reason) => panic!("{start:?}: {reason}"),
        }
    }
    // and the way that makes a new key is the Active one only
    match judge(&KeyState::Active, Op::Rotate, &e) {
        Verdict::Permit(permit) => assert_eq!(permit.way(), Way::New),
        Verdict::Refuse(reason) => panic!("{reason}"),
    }
}

// ==== The cells that depend on what was read ====

/// Evidence of a drive whose recovery code was checked, registered, and whose months are kept
/// elsewhere too.
fn all_good() -> Evidence<'static> {
    let mut e = Evidence::new(DRIVE, NOW);
    e.token = TokenFacts {
        code_checked: Read::Found(NOW - 10),
        code_registered: Read::Found(true),
    };
    e.vault = VaultFacts {
        months_elsewhere: Read::Found(true),
    };
    e
}

fn removal_verdict(state: &KeyState, e: &Evidence<'_>) -> Result<Vec<Stays>, Refusal> {
    match judge(state, Op::Remove, e) {
        Verdict::Permit(permit) => Ok(permit.stays().to_vec()),
        Verdict::Refuse(reason) => Err(reason.kind),
    }
}

/// KEYS-8 and KEYS-32: Remove drive lets the keys go only with a checked code, a registered code and
/// the months kept elsewhere - in the two states that can hold them - and says what stays.
#[test]
fn x_delta_azdrive_keys_8_remove_is_permitted_only_with_a_checked_registered_code_and_months_kept_elsewhere(
) {
    for state in [KeyState::Fresh, KeyState::Active] {
        let stays = removal_verdict(&state, &all_good()).unwrap();
        assert_eq!(
            stays,
            vec![Stays::TheBucket, Stays::SyncedFiles, Stays::ContactKeys],
            "{state:?}"
        );

        let mut never = all_good();
        never.token.code_checked = Read::Absent;
        assert_eq!(
            removal_verdict(&state, &never),
            Err(Refusal::CodeNotChecked)
        );

        // a stamp from a clock that ran ahead is no check
        let mut ahead = all_good();
        ahead.token.code_checked = Read::Found(NOW + 1);
        assert_eq!(
            removal_verdict(&state, &ahead),
            Err(Refusal::CodeNotChecked)
        );

        // checked at this very second is checked
        let mut now = all_good();
        now.token.code_checked = Read::Found(NOW);
        assert!(removal_verdict(&state, &now).is_ok());

        for registered in [Read::Found(false), Read::Absent] {
            let mut unregistered = all_good();
            unregistered.token.code_registered = registered;
            assert_eq!(
                removal_verdict(&state, &unregistered),
                Err(Refusal::CodeNotRegistered),
                "a registration that is pending, missing or never recorded is not 'registered'"
            );
        }

        let mut only_here = all_good();
        only_here.vault.months_elsewhere = Read::Found(false);
        assert_eq!(
            removal_verdict(&state, &only_here),
            Err(Refusal::MonthsOnlyHere)
        );

        // a drive with no vault of its own has no months to lose
        let mut no_vault = all_good();
        no_vault.vault.months_elsewhere = Read::Absent;
        assert!(removal_verdict(&state, &no_vault).is_ok());

        // a failed read refuses, whichever it is
        for (name, mut failing) in [
            ("code_checked", all_good()),
            ("code_registered", all_good()),
            ("months", all_good()),
        ] {
            match name {
                "code_checked" => failing.token.code_checked = Read::Failed(String::from("no")),
                "code_registered" => {
                    failing.token.code_registered = Read::Failed(String::from("no"));
                }
                _ => failing.vault.months_elsewhere = Read::Failed(String::from("no")),
            }
            assert!(
                matches!(
                    removal_verdict(&state, &failing),
                    Err(Refusal::Unreadable { .. })
                ),
                "{state:?} {name}"
            );
        }
    }
}

/// KEYS-2 and KEYS-3: not even a perfect proof lets the keys go while a key change is unfinished or
/// a key is only planned.
#[test]
fn x_delta_azdrive_keys_2_and_3_nothing_is_erased_while_a_key_change_is_unfinished_or_a_key_only_planned(
) {
    let e = all_good();
    for start in [Start::Resume, Start::Adopt] {
        assert_eq!(
            removal_verdict(&KeyState::Rotating(start), &e),
            Err(Refusal::KeyChangeUnfinished)
        );
    }
    assert_eq!(
        removal_verdict(&KeyState::Planned, &e),
        Err(Refusal::SetupUnfinished)
    );
}

/// KEYS-7: no recovery code is added or removed while a key change may replace the wrap.
#[test]
fn x_delta_azdrive_keys_7_a_recovery_code_waits_for_an_unfinished_key_change() {
    let e = all_good();
    for start in [Start::Resume, Start::Adopt] {
        for op in [Op::AddCode, Op::RemoveCode] {
            match judge(&KeyState::Rotating(start), op, &e) {
                Verdict::Refuse(reason) => assert_eq!(reason.kind, Refusal::WrapMayChange),
                Verdict::Permit(_) => panic!("{start:?} x {op:?} was permitted"),
            }
        }
    }
}

/// KEYS-6: whatever could not be read, "I was hacked" is not stopped by it; everything else is.
#[test]
fn x_delta_azdrive_keys_6_a_failed_read_stops_no_lockdown_and_every_other_operation() {
    let unknown = KeyState::Unknown(Why::Unreadable {
        what: "the key change's journal",
        why: String::from("the bucket did not answer"),
    });
    let e = Evidence::new(DRIVE, NOW);
    for op in Op::ALL {
        match (op, judge(&unknown, op, &e)) {
            (Op::Lockdown, Verdict::Permit(_)) => {}
            (Op::Lockdown, Verdict::Refuse(reason)) => panic!("the lockdown was refused: {reason}"),
            (_, Verdict::Refuse(reason)) => {
                assert!(
                    matches!(reason.kind, Refusal::Unreadable { .. }),
                    "{op:?}: {reason:?}"
                );
                assert!(
                    reason.sentence.contains("could not be read"),
                    "{}",
                    reason.sentence
                );
            }
            (_, Verdict::Permit(_)) => panic!("{op:?} was permitted on an unreadable state"),
        }
    }
}

/// KEYS-25: an operation that writes a key into the keyring is refused first when the keyring keeps
/// none, before anything is done at a server.
#[test]
fn x_delta_azdrive_keys_25_a_keyring_that_keeps_no_key_refuses_every_operation_that_writes_one() {
    let writes: [(KeyState, Op); 9] = [
        (KeyState::Fresh, Op::Setup),
        (KeyState::Planned, Op::Setup),
        (KeyState::Active, Op::Enroll),
        (KeyState::Active, Op::Rotate),
        (KeyState::Rotating(Start::Resume), Op::Rotate),
        (KeyState::Rotating(Start::Adopt), Op::Rotate),
        (KeyState::Locked, Op::Recover),
        (KeyState::Fresh, Op::Recover),
        (KeyState::Torn(Why::OpensNothing), Op::Recover),
    ];
    for (state, op) in writes {
        let mut e = Evidence::new(DRIVE, NOW);
        e.keyring.keeps_keys = Read::Found(Keeps::No("a plain file"));
        match judge(&state, op, &e) {
            Verdict::Refuse(reason) => {
                assert_eq!(reason.kind, Refusal::KeyringKeepsNoKeys("a plain file"));
                assert!(
                    reason.sentence.contains("a plain file"),
                    "{}",
                    reason.sentence
                );
            }
            Verdict::Permit(_) => panic!("{state:?} x {op:?} was permitted"),
        }
        e.keyring.keeps_keys = Read::Found(Keeps::Yes);
        assert!(
            matches!(judge(&state, op, &e), Verdict::Permit(_)),
            "{state:?} x {op:?} with a keyring that keeps keys"
        );
        e.keyring.keeps_keys = Read::Failed(String::from("the keyring went"));
        assert!(
            matches!(
                judge(&state, op, &e),
                Verdict::Refuse(reason) if matches!(reason.kind, Refusal::Unreadable { .. })
            ),
            "{state:?} x {op:?}: a keyring whose protection could not be read"
        );
    }
}

#[test]
fn a_drive_holds_at_most_the_most_further_recovery_codes() {
    let mut e = Evidence::new(DRIVE, NOW);
    let most = u32::try_from(MAX_EXTRA_RECOVERY_CODES).unwrap();
    e.bucket.extra_codes = Read::Found(most - 1);
    assert!(matches!(
        judge(&KeyState::Active, Op::AddCode, &e),
        Verdict::Permit(_)
    ));
    e.bucket.extra_codes = Read::Found(most);
    match judge(&KeyState::Active, Op::AddCode, &e) {
        Verdict::Refuse(reason) => assert_eq!(reason.kind, Refusal::TooManyCodes),
        Verdict::Permit(_) => panic!("a ninth code was permitted"),
    }
    e.bucket.extra_codes = Read::Failed(String::from("the bucket did not answer"));
    assert!(matches!(
        judge(&KeyState::Active, Op::AddCode, &e),
        Verdict::Refuse(reason) if matches!(reason.kind, Refusal::Unreadable { .. })
    ));
}

// ==== observe ====

fn id(n: u8) -> KeyId {
    KeyId([n; 16])
}

/// Evidence in which the keyring was asked about all three entries.
fn keyring(drive: Option<u8>, next: Option<u8>, planned: Option<u8>) -> Evidence<'static> {
    let mut e = Evidence::new(DRIVE, NOW);
    e.keyring = KeyringFacts {
        drive_key: drive.map_or(Read::Absent, |n| Read::Found(id(n))),
        next_key: next.map_or(Read::Absent, |n| Read::Found(id(n))),
        planned_key: planned.map_or(Read::Absent, |n| Read::Found(id(n))),
        keeps_keys: Read::Found(Keeps::Yes),
    };
    e
}

fn unreadable(what: &'static str, why: &str) -> KeyState {
    KeyState::Unknown(Why::Unreadable {
        what,
        why: why.to_string(),
    })
}

#[test]
fn a_drive_key_and_nothing_else_is_active() {
    assert_eq!(observe(&keyring(Some(1), None, None)), KeyState::Active);
    // a planned key left behind by a crash between the setup's last two steps changes nothing
    assert_eq!(observe(&keyring(Some(1), None, Some(1))), KeyState::Active);
    // a finished rotation left its entries: the new key IS the drive key
    assert_eq!(observe(&keyring(Some(2), Some(2), None)), KeyState::Active);
}

#[test]
fn a_new_key_that_is_not_the_drive_key_is_a_rotation_and_a_journal_nobody_looked_at_is_not_the_devices(
) {
    // The journal asked and the device's own: resume (and skip the lockdown it has had).
    let mut own = keyring(Some(1), Some(2), None);
    own.bucket.journal = Read::Found(Journal::Own);
    assert_eq!(observe(&own), KeyState::Rotating(Start::Resume));

    // Anything else adopts the key and runs the lockdown again: never resume on a journal that
    // nobody checked.
    for journal in [Read::Found(Journal::NotOwn), Read::Absent, Read::Unasked] {
        let mut e = keyring(Some(1), Some(2), None);
        e.bucket.journal = journal.clone();
        assert_eq!(observe(&e), KeyState::Rotating(Start::Adopt), "{journal:?}");
    }

    // A bucket that cannot say is unknown, not "no journal": the lockdown is skipped on a journal's
    // word only.
    let mut failed = keyring(Some(1), Some(2), None);
    failed.bucket.journal = Read::Failed(String::from("no answer"));
    assert_eq!(
        observe(&failed),
        unreadable("the key change's journal", "no answer")
    );
}

#[test]
fn an_own_journal_resumes_whatever_the_drive_key_and_the_index_say() {
    // `rotation::resumable` asks nothing of the drive key; `start` returns before it reads the index.
    for drive in [None, Some(1)] {
        let mut e = keyring(drive, Some(2), None);
        e.bucket.journal = Read::Found(Journal::Own);
        e.bucket.index_opens = Read::Found(false);
        assert_eq!(observe(&e), KeyState::Rotating(Start::Resume), "{drive:?}");
    }
}

#[test]
fn a_new_key_without_a_drive_key_and_without_a_journal_of_its_own_is_torn() {
    let mut e = keyring(None, Some(2), None);
    e.bucket.journal = Read::Absent;
    assert_eq!(observe(&e), KeyState::Torn(Why::OldKeyGone));
}

#[test]
fn an_index_that_opens_with_none_of_the_keys_is_torn_and_one_that_cannot_be_asked_is_unknown() {
    for (drive, next, journal) in [
        (Some(1), None, Read::Unasked),
        (Some(1), Some(2), Read::Absent),
        (Some(2), Some(2), Read::Absent),
    ] {
        let mut e = keyring(drive, next, None);
        e.bucket.journal = journal;
        e.bucket.index_opens = Read::Found(false);
        assert_eq!(
            observe(&e),
            KeyState::Torn(Why::OpensNothing),
            "{drive:?} {next:?}"
        );
        e.bucket.index_opens = Read::Failed(String::from("no answer"));
        assert!(
            matches!(observe(&e), KeyState::Unknown(Why::Unreadable { .. })),
            "{drive:?} {next:?}"
        );
        e.bucket.index_opens = Read::Found(true);
        assert!(
            matches!(
                observe(&e),
                KeyState::Active | KeyState::Rotating(Start::Adopt)
            ),
            "{drive:?} {next:?}"
        );
    }
}

#[test]
fn a_planned_key_and_no_drive_key_is_planned_and_neither_is_locked_or_fresh_by_the_bucket() {
    assert_eq!(observe(&keyring(None, None, Some(9))), KeyState::Planned);

    let mut locked = keyring(None, None, None);
    locked.bucket.encrypted = Read::Found(true);
    assert_eq!(observe(&locked), KeyState::Locked);

    for fresh in [Read::Found(false), Read::Absent] {
        let mut e = keyring(None, None, None);
        e.bucket.encrypted = fresh;
        assert_eq!(observe(&e), KeyState::Fresh);
    }

    let mut failed = keyring(None, None, None);
    failed.bucket.encrypted = Read::Failed(String::from("no answer"));
    assert_eq!(
        observe(&failed),
        unreadable("whether the drive is encrypted", "no answer")
    );
}

/// An entry nobody asked is not an entry that is not there: a drive whose setup is unfinished must
/// not be taken for a new one, nor a rotation for none.
#[test]
fn a_fact_the_state_rests_on_that_nobody_asked_is_not_asked_and_never_a_state() {
    assert_eq!(
        observe(&Evidence::new(DRIVE, NOW)),
        KeyState::Unknown(Why::NotAsked("the drive key"))
    );
    let mut no_next = keyring(Some(1), None, None);
    no_next.keyring.next_key = Read::Unasked;
    assert_eq!(
        observe(&no_next),
        KeyState::Unknown(Why::NotAsked("the new key of a key change"))
    );
    let mut no_planned = keyring(None, None, None);
    no_planned.keyring.planned_key = Read::Unasked;
    no_planned.bucket.encrypted = Read::Found(false);
    assert_eq!(
        observe(&no_planned),
        KeyState::Unknown(Why::NotAsked("the key kept for the drive's setup"))
    );
    let no_bucket = keyring(None, None, None);
    assert_eq!(
        observe(&no_bucket),
        KeyState::Unknown(Why::NotAsked("whether the drive is encrypted"))
    );
}

/// A failed read lands in `Unknown`, whichever fact it is and whatever else is known: never `Active`.
#[test]
fn every_failed_read_lands_in_unknown_and_never_in_active() {
    fn failed<T>() -> Read<T> {
        Read::Failed(String::from("the keyring refused"))
    }
    let cases: [(&str, Box<dyn Fn(&mut Evidence<'_>)>); 4] = [
        ("drive key", Box::new(|e| e.keyring.drive_key = failed())),
        ("new key", Box::new(|e| e.keyring.next_key = failed())),
        (
            "planned key",
            Box::new(|e| {
                e.keyring.drive_key = Read::Absent;
                e.keyring.planned_key = failed();
            }),
        ),
        (
            "encrypted",
            Box::new(|e| {
                e.keyring.drive_key = Read::Absent;
                e.bucket.encrypted = failed();
            }),
        ),
    ];
    for (name, make) in cases {
        let mut e = keyring(Some(1), None, None);
        e.bucket.encrypted = Read::Found(false);
        make(&mut e);
        assert!(
            matches!(observe(&e), KeyState::Unknown(Why::Unreadable { .. })),
            "{name}: {:?}",
            observe(&e)
        );
    }
}

#[test]
fn the_evidence_says_which_facts_were_asked() {
    assert!(Evidence::new(DRIVE, NOW).asked().is_empty());
    let mut e = keyring(Some(1), None, None);
    e.bucket.encrypted = Read::Found(true);
    assert_eq!(
        e.asked(),
        vec![
            "drive key",
            "new key",
            "planned key",
            "keeps keys",
            "encrypted"
        ]
    );
}

// ==== The sentences ====

/// The sentence of a drive whose keys open nothing is the one `rotation::start` refuses with.
#[test]
fn the_sentence_of_keys_that_open_nothing_is_the_one_a_rotation_refuses_with() {
    assert_eq!(
        rotation::opens_nothing(),
        DriveError::Denied {
            message: Refusal::OpensNothing.sentence(DRIVE)
        }
    );
    assert!(
        !Refusal::OpensNothing.sentence(DRIVE).contains("  "),
        "one sentence with single spaces (d3607fed5)"
    );
}

/// The sentence of a locked drive is the error's own.
#[test]
fn the_sentence_of_a_locked_drive_is_the_one_open_encrypted_answers() {
    let error = DriveError::Locked {
        drive: DRIVE.to_string(),
    };
    assert_eq!(Refusal::Locked.sentence(DRIVE), error.to_string());
}

/// The sentence of an encrypted drive is the one `setup_new_drive` refuses with: the pair test enters
/// through the flow, on a bucket another device set up.
#[test]
fn the_sentence_of_an_encrypted_drive_is_the_one_setup_refuses_with() {
    let bucket = MemBucket::new();
    setup_new_drive(&bucket, &MemoryKeyring::new(), DRIVE, cheap()).unwrap();
    // another device: no key, no planned key, and the bucket holds key files
    let second_device = MemoryKeyring::new();
    match setup_new_drive(&bucket, &second_device, DRIVE, cheap()) {
        Err(DriveError::InvalidConfig(said)) => {
            assert_eq!(said, Refusal::EncryptedAlready.sentence(DRIVE));
        }
        Err(other) => panic!("{other:?}"),
        Ok(_) => panic!("a second device set an encrypted drive up again"),
    }
}

/// The sentence of a drive with all its further codes is the one `add_recovery_code` refuses with.
#[test]
fn the_sentence_of_too_many_codes_is_the_one_a_ninth_code_is_refused_with() {
    let bucket = MemBucket::new();
    let owner_device = MemoryKeyring::new();
    setup_new_drive(&bucket, &owner_device, DRIVE, cheap()).unwrap();
    for _ in 0..MAX_EXTRA_RECOVERY_CODES {
        add_recovery_code(&bucket, &owner_device, DRIVE, cheap()).unwrap();
    }
    match add_recovery_code(&bucket, &owner_device, DRIVE, cheap()) {
        Err(DriveError::Unsupported(said)) => {
            assert_eq!(said, Refusal::TooManyCodes.sentence(DRIVE));
        }
        Err(other) => panic!("{other:?}"),
        Ok(_) => panic!("a ninth further code was made"),
    }
}

#[test]
fn every_refusal_says_the_drive_or_what_to_do() {
    let all = [
        Refusal::Unreadable {
            what: "the drive key",
            why: String::from("the keyring refused"),
        },
        Refusal::NotAsked("the drive key"),
        Refusal::Locked,
        Refusal::NoKey,
        Refusal::EncryptedAlready,
        Refusal::SetupUnfinished,
        Refusal::KeyChangeUnfinished,
        Refusal::WrapMayChange,
        Refusal::OpensNothing,
        Refusal::OldKeyGone,
        Refusal::KeyringKeepsNoKeys("a plain file"),
        Refusal::CodeNotChecked,
        Refusal::CodeNotRegistered,
        Refusal::MonthsOnlyHere,
        Refusal::TooManyCodes,
        Refusal::NotEncrypted,
        Refusal::AlreadyLockedDown,
    ];
    for refusal in all {
        let said = refusal.sentence(DRIVE);
        assert!(said.len() > 20, "{refusal:?}: {said}");
        assert!(!said.contains("  "), "{refusal:?}: {said}");
        assert!(
            !said.ends_with('.'),
            "a sentence the window ends itself: {said}"
        );
    }
}

// ==== The shadow ====

/// A site name no other test uses: the record is the process's, and the tests run side by side.
fn site(name: &'static str) -> &'static str {
    name
}

#[test]
fn a_guard_that_decides_as_the_judge_does_agrees_and_one_that_does_not_is_a_disagreement() {
    let mut e = keyring(Some(1), None, None);
    e.bucket.index_opens = Read::Found(true);
    assert_eq!(
        compare(
            site("tests::agree"),
            Op::Rotate,
            &e,
            Guard::PermitsAs(Way::New)
        ),
        Compared::Agree
    );
    assert_eq!(
        compare(site("tests::agree"), Op::Rotate, &e, Guard::Permits),
        Compared::Agree
    );
    match compare(site("tests::differs"), Op::Rotate, &e, Guard::Refuses) {
        Compared::Disagree(d) => {
            assert_eq!(d.site, "tests::differs");
            assert_eq!(d.state, KeyState::Active);
            assert_eq!(d.judged, Judged::Permits(Way::New));
            assert!(d.known.is_none());
        }
        other => panic!("{other:?}"),
    }
    // the same permit, the wrong way: a resume where the judge says a new start
    assert!(matches!(
        compare(
            site("tests::way"),
            Op::Rotate,
            &e,
            Guard::PermitsAs(Way::Resume)
        ),
        Compared::Disagree(_)
    ));
    // a refusal where the judge permits is a disagreement too, and so is the other way round
    let locked = {
        let mut l = keyring(None, None, None);
        l.bucket.encrypted = Read::Found(true);
        l
    };
    assert!(matches!(
        compare(site("tests::locked"), Op::Rotate, &locked, Guard::Permits),
        Compared::Disagree(_)
    ));
    assert_eq!(
        compare(site("tests::locked"), Op::Rotate, &locked, Guard::Refuses),
        Compared::Agree
    );
}

#[test]
fn a_site_that_did_not_ask_the_keyring_is_not_judged_and_never_a_disagreement() {
    let e = Evidence::new(DRIVE, NOW);
    assert_eq!(
        compare(site("tests::unasked"), Op::Remove, &e, Guard::Permits),
        Compared::NotJudgeable("the drive key")
    );
    assert!(shadow(site("tests::unasked"), Op::Remove, &e, Guard::Refuses).is_none());
    assert!(disagreements_at("tests::unasked").is_empty());
}

#[test]
fn a_disagreement_is_noted_once_and_handed_back_each_time() {
    let e = keyring(Some(1), None, None);
    let first = shadow(site("tests::noted"), Op::Rotate, &e, Guard::Refuses);
    let second = shadow(site("tests::noted"), Op::Rotate, &e, Guard::Refuses);
    assert!(first.is_some() && second.is_some());
    let noted: Vec<Disagreement> = disagreements_at("tests::noted");
    assert_eq!(noted.len(), 1, "once");
    assert!(noted[0].to_string().contains("tests::noted"));
    assert!(noted[0].to_string().contains("rotate"));
}

#[test]
#[should_panic(expected = "tests::macro_asserts")]
fn the_macro_asserts_a_disagreement_in_a_test_build() {
    let e = keyring(Some(1), None, None);
    crate::key_shadow!("tests::macro_asserts", Op::Rotate, e, Guard::Refuses);
}

#[test]
fn a_disagreement_that_is_a_finding_is_logged_and_never_asserted() {
    let e = keyring(Some(1), None, None);
    crate::key_shadow!(
        "tests::macro_known",
        Op::Rotate,
        e,
        Guard::Refuses,
        known = "X-TEST-1: the guard is stricter than the table"
    );
    let noted = disagreements_at("tests::macro_known");
    assert_eq!(noted.len(), 1);
    assert_eq!(
        noted[0].known,
        Some("X-TEST-1: the guard is stricter than the table")
    );
    assert!(noted[0].to_string().contains("[known: X-TEST-1"));
}

#[test]
fn the_macro_is_quiet_when_the_guard_agrees() {
    let e = keyring(Some(1), None, None);
    crate::key_shadow!(
        "tests::macro_quiet",
        Op::Rotate,
        e,
        Guard::PermitsAs(Way::New)
    );
    assert!(disagreements_at("tests::macro_quiet").is_empty());
}

// ==== What a drive key touches ====

#[test]
fn a_drive_key_touches_ten_things_in_the_order_of_the_design() {
    let names: Vec<&str> = CONSUMERS.iter().map(|c| c.name).collect();
    assert_eq!(
        names,
        [
            "INDEX",
            "MEMBERS",
            "RECOVERY_WRAP",
            "KITS",
            "SHARES",
            "PERIOD_VAULT",
            "DROPS",
            "REENCRYPT_STATE",
            "CHAIN",
            "DEVICE_KEY"
        ]
    );
    let unique: BTreeSet<&str> = names.iter().copied().collect();
    assert_eq!(unique.len(), 10);
}

/// One list of what a drive leaves in the keyring: every kind of entry has an owner, the consumers'
/// `entries` are exactly the ones the owner table gives them, and a consumer's forget exists
/// for an entry it owns.
#[test]
fn every_keyring_entry_of_a_drive_has_one_owner_and_the_consumers_hold_exactly_theirs() {
    let mut owned = 0;
    for entry in KeyEntry::ALL {
        match owner(entry) {
            Owner::TheKey | Owner::Credential => {}
            Owner::Consumer(name) => {
                let consumer = CONSUMERS
                    .iter()
                    .find(|c| c.name == name)
                    .unwrap_or_else(|| {
                        panic!("{entry:?} is owned by {name}, which is no consumer")
                    });
                assert!(consumer.entries.contains(&entry), "{name} lacks {entry:?}");
                assert!(
                    matches!(consumer.forget, Forget::Fn(_)),
                    "{name} owns {entry:?} and forgets nothing"
                );
                owned += 1;
            }
        }
    }
    let held: usize = CONSUMERS.iter().map(|c| c.entries.len()).sum();
    assert_eq!(
        held, owned,
        "a consumer holds an entry its owner table does not give it"
    );
    for consumer in CONSUMERS {
        for entry in consumer.entries {
            assert_eq!(owner(*entry), Owner::Consumer(consumer.name));
        }
    }
    assert_eq!(
        KeyEntry::ALL.len(),
        10,
        "a new kind of entry is placed in the owner table"
    );
}

#[test]
fn a_consumers_functions_are_files_and_names_listed_once_and_a_reseal_within_names_a_consumer() {
    let mut seen = BTreeSet::new();
    for consumer in CONSUMERS {
        for function in consumer.functions {
            let (file, name) = function
                .split_once('#')
                .unwrap_or_else(|| panic!("{function} names no function"));
            assert!(
                file.starts_with("examples/") && file.contains("/src/") && file.ends_with(".rs"),
                "{function}"
            );
            assert!(
                !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "{function}"
            );
            assert!(seen.insert(*function), "{function} is listed twice");
        }
        if let Reseal::Within(other) = consumer.reseal {
            assert!(
                CONSUMERS
                    .iter()
                    .any(|c| c.name == other && c.name != consumer.name),
                "{} is resealed within {other}, which is no other consumer",
                consumer.name
            );
        }
    }
}

/// What a rotation reseals: six consumers do it here, one in another consumer's pass, three are not
/// sealed under the drive key or are resealed by what the app does.
#[test]
fn six_consumers_reseal_here_and_four_forget_a_keyring_entry() {
    let resealed: Vec<&str> = CONSUMERS
        .iter()
        .filter(|c| matches!(c.reseal, Reseal::Fn(_)))
        .map(|c| c.name)
        .collect();
    assert_eq!(
        resealed,
        [
            "INDEX",
            "MEMBERS",
            "RECOVERY_WRAP",
            "SHARES",
            "DROPS",
            "CHAIN"
        ]
    );
    let forgotten: Vec<&str> = CONSUMERS
        .iter()
        .filter(|c| matches!(c.forget, Forget::Fn(_)))
        .map(|c| c.name)
        .collect();
    assert_eq!(
        forgotten,
        ["MEMBERS", "RECOVERY_WRAP", "PERIOD_VAULT", "DEVICE_KEY"]
    );
}

/// A minimal provider for the adapters: the index in memory, nothing sealed.
struct Provider {
    index: Arc<MemoryIndex>,
}

impl IndexProvider for Provider {
    fn open_index(
        &self,
        _drive: &str,
        _bucket: Arc<dyn Drive>,
        _drive_key: &DriveKey,
    ) -> Result<Arc<dyn NameIndex>, DriveError> {
        Ok(self.index.clone())
    }

    fn rekey(
        &self,
        _drive: &str,
        _bucket: Arc<dyn Drive>,
        _old: &DriveKey,
        _new: &DriveKey,
    ) -> Result<(), DriveError> {
        Ok(())
    }
}

fn permit_of(state: &KeyState, op: Op) -> Permit {
    match judge(state, op, &all_good()) {
        Verdict::Permit(permit) => permit,
        Verdict::Refuse(reason) => panic!("{state:?} x {op:?}: {reason}"),
    }
}

/// A forget runs for the permit of a removal of this drive and for no other: the permit of an enrol
/// leaves the member key where it is.
#[test]
fn a_forget_takes_the_keyring_entry_only_for_a_permit_of_a_removal_of_this_drive() {
    let bucket = MemBucket::new();
    let keyring = MemoryKeyring::new();
    setup_new_drive(&bucket, &keyring, DRIVE, cheap()).unwrap();
    let entry = device::member_key_entry(DRIVE);
    assert!(
        keyring.get(&entry).unwrap().is_some(),
        "setup enrolled this device"
    );

    let erase = Erase {
        drive: DRIVE,
        keyring: &keyring,
    };
    let Forget::Fn(forget) = MEMBERS.forget else {
        panic!("MEMBERS forgets its keyring entry");
    };
    let enrol = permit_of(&KeyState::Active, Op::Enroll);
    assert!(matches!(
        forget(&enrol, &erase),
        Err(DriveError::Denied { .. })
    ));
    assert!(
        keyring.get(&entry).unwrap().is_some(),
        "a permit of another operation"
    );

    let other_drive = Erase {
        drive: "d_other",
        keyring: &keyring,
    };
    let removal = permit_of(&KeyState::Active, Op::Remove);
    assert!(matches!(
        forget(&removal, &other_drive),
        Err(DriveError::Denied { .. })
    ));
    assert!(
        keyring.get(&entry).unwrap().is_some(),
        "a permit of another drive"
    );

    assert!(forget(&removal, &erase).is_ok());
    assert!(
        keyring.get(&entry).unwrap().is_none(),
        "the permit of this removal"
    );
}

/// The reseals run for a permit of a rotation of this drive; each is the existing step of the
/// rotation, called through the registry.
#[test]
fn the_reseals_run_for_a_rotation_permit_only_and_do_the_existing_steps() {
    let bucket: Arc<dyn Drive> = Arc::new(MemBucket::new());
    let keyring = MemoryKeyring::new();
    let (old, _) = setup_new_drive(bucket.as_ref(), &keyring, DRIVE, cheap()).unwrap();
    let new = DriveKey::generate().unwrap();
    let provider = Provider {
        index: Arc::new(MemoryIndex::new()),
    };
    let change = Change {
        drive: DRIVE,
        bucket: &bucket,
        keyring: &keyring,
        provider: &provider,
        old: &old,
        new: &new,
        older: &[],
        kdf: cheap(),
    };
    let rotate = permit_of(&KeyState::Active, Op::Rotate);
    let enrol = permit_of(&KeyState::Active, Op::Enroll);

    // the chain: a link of the old key under the new one, read back
    let chain = CONSUMERS.iter().find(|c| c.name == "CHAIN").unwrap();
    let Reseal::Fn(reseal_chain) = chain.reseal else {
        panic!("CHAIN is resealed here");
    };
    assert!(matches!(
        reseal_chain(&enrol, &change),
        Err(DriveError::Denied { .. })
    ));
    assert!(
        load_key_chain(bucket.as_ref()).unwrap().is_none(),
        "the wrong permit wrote nothing"
    );
    assert!(matches!(reseal_chain(&rotate, &change), Ok(Done::Nothing)));
    let chain_now = load_key_chain(bucket.as_ref())
        .unwrap()
        .expect("the chain is there");
    assert!(
        chain_now.open(DRIVE, &new).contains(&old),
        "the new key opens the old one"
    );

    // the shares and the drop key of a drive that has none: nothing to do, and said
    for name in ["SHARES", "DROPS"] {
        let consumer = CONSUMERS.iter().find(|c| c.name == name).unwrap();
        let Reseal::Fn(reseal) = consumer.reseal else {
            panic!("{name} is resealed here");
        };
        assert!(matches!(
            reseal(&enrol, &change),
            Err(DriveError::Denied { .. })
        ));
        match reseal(&rotate, &change) {
            Ok(Done::Count(0) | Done::DropKey(None)) => {}
            other => panic!("{name}: {other:?}"),
        }
    }

    // the index: the provider seals it under the new key, the empty index has nothing to re-wrap
    let index = CONSUMERS.iter().find(|c| c.name == "INDEX").unwrap();
    let Reseal::Fn(reseal_index) = index.reseal else {
        panic!("INDEX is resealed here");
    };
    assert!(matches!(reseal_index(&rotate, &change), Ok(Done::Count(0))));

    // the recovery wrap is sealed to the key the keyring holds: still the old one here
    let wrap = CONSUMERS
        .iter()
        .find(|c| c.name == "RECOVERY_WRAP")
        .unwrap();
    let Reseal::Fn(reseal_wrap) = wrap.reseal else {
        panic!("RECOVERY_WRAP is resealed here");
    };
    assert!(
        matches!(reseal_wrap(&rotate, &change), Err(DriveError::Io(_))),
        "a recovery wrap is not sealed to a key the keyring does not hold"
    );
}
