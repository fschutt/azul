//! Key permits (ARCH29 C7): the truth table of `judge` as the specification, the observation of the
//! state, the cells that depend on what was read, the sentences beside the flows that say them today,
//! and the registry of what a drive key touches.
//!
//! The table is stated here as an exhaustive `match` over `(KeyState, Op)` without a catch-all: a new
//! state or a new operation does not compile until its cells are decided. The cells that carry a
//! finding of the key review (X-DELTA-AZDRIVE-KEYS-1, -2, -3, -6, -7, -8, -18, -32) are named where
//! they are.

use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use super::{
    asked::{lockdown_needed, rotate},
    mem_bucket::MemBucket,
};
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
    encrypted::{judge as enc_judge, IndexProvider, MemoryIndex, NameIndex, Recorded},
    footprint::KeyEntry,
    keyring::{KeyringError, KeyringStore, MemoryKeyring},
    rotation::{self, JOURNAL_FILE},
    trust::{EncState, Via, Why as Unencrypted},
    Drive, DriveError,
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
    /// (`removal_*` below). With nothing asked it is refused (X-DELTA-STRUCT-KEYSTATE-2).
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
        // the new key is the drive key and the previous one is still kept (X-DELTA-STRUCT-KEYSTATE-1):
        // `rotation::start` answers a new rotation
        (S::Rotating(Start::Again), Op::Rotate) => Cell::Permit(Way::New, NO_STAYS),
        // the recovery wrap may change (KEYS-7)
        (S::Rotating(_), Op::AddCode | Op::RemoveCode) => Cell::Refuse(Refusal::WrapMayChange),
        // the keyring's new key may be the only copy (KEYS-2)
        (S::Rotating(_), Op::Remove | Op::Recover | Op::Reencrypt | Op::RestoreAsOf) => {
            Cell::Refuse(Refusal::KeyChangeUnfinished)
        }
        // a rotation of this device has had its lockdown; a journal that is not its own has not
        (S::Rotating(Start::Resume), Op::Lockdown) => Cell::Refuse(Refusal::AlreadyLockedDown),
        (S::Rotating(Start::Adopt | Start::Again), Op::Lockdown) => {
            Cell::Permit(Way::Plain, NO_STAYS)
        }

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
        // the setup's own refusal of an encrypted drive (`the_sentence_of_an_encrypted_drive_...`)
        (S::Locked, Op::Setup) => Cell::Refuse(Refusal::EncryptedAlready),
        (
            S::Locked,
            Op::Enroll
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

/// One state of every row, and each payload that a cell depends on.
fn states() -> Vec<KeyState> {
    vec![
        KeyState::Fresh,
        KeyState::Planned,
        KeyState::Active,
        KeyState::Rotating(Start::Resume),
        KeyState::Rotating(Start::Adopt),
        KeyState::Rotating(Start::Again),
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

/// Evidence in which everything a cell may rest on was asked and says yes: the keyring keeps keys, the
/// index opens, the drive holds none of its further codes, the recovery code was checked, registered,
/// and the months are kept elsewhere too.
fn everything_asked() -> Evidence<'static> {
    let mut e = all_good();
    e.keyring.keeps_keys = Read::Found(Keeps::Yes);
    e.bucket.index_opens = Read::Found(true);
    e.bucket.extra_codes = Read::Found(0);
    e
}

#[test]
fn the_table_is_the_specification_and_judge_answers_all_seventy_cells_by_it() {
    let mut cells = BTreeSet::new();
    for state in states() {
        for op in Op::ALL {
            let e = everything_asked();
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
                    // everything asked and good: the conditions are `removal_*` below
                    assert_eq!(permit.op(), Op::Remove);
                    assert_eq!(
                        permit.stays(),
                        &[Stays::TheBucket, Stays::SyncedFiles, Stays::ContactKeys],
                        "Remove says what it leaves"
                    );
                }
                (want, got) => panic!("{state:?} x {op:?}: the table says {want:?}, judge {got:?}"),
            }
            cells.insert((state.row(), op.name()));
        }
    }
    assert_eq!(cells.len(), 70, "seven states, ten operations");
}

/// X-DELTA-STRUCT-KEYSTATE-2: the table's permits rest on facts, and a fact nobody asked is no "yes".
/// With nothing asked, an operation that changes or erases key material is refused wherever the table
/// permits it, and says which fact was not asked; the one cell that rests on nothing is the removal of
/// a further code's wrap in an Active drive (it writes no key and reads no fact). The refusals of the
/// table stay as they are.
#[test]
fn x_delta_struct_keystate_2_with_nothing_asked_no_key_changing_operation_is_permitted_unless_its_cell_rests_on_nothing(
) {
    const KEY_CHANGING: [Op; 7] = [
        Op::Setup,
        Op::Rotate,
        Op::Remove,
        Op::AddCode,
        Op::RemoveCode,
        Op::Reencrypt,
        Op::Recover,
    ];
    let nothing = Evidence::new(DRIVE, NOW);
    let mut rested_on_nothing = Vec::new();
    for state in states() {
        for op in KEY_CHANGING {
            let got = judge(&state, op, &nothing);
            match (spec(&state, op), got) {
                (Cell::Refuse(want), Verdict::Refuse(reason)) => {
                    assert_eq!(reason.kind, want, "{state:?} x {op:?}");
                }
                (Cell::Permit(..) | Cell::Removal, Verdict::Refuse(reason)) => assert!(
                    matches!(reason.kind, Refusal::NotAsked(_)),
                    "{state:?} x {op:?}: {reason:?}"
                ),
                (Cell::Permit(..), Verdict::Permit(_)) => rested_on_nothing.push((state.row(), op)),
                (want, got) => panic!("{state:?} x {op:?}: the table says {want:?}, judge {got:?}"),
            }
        }
    }
    assert_eq!(rested_on_nothing, [("active", Op::RemoveCode)]);
}

/// X-DELTA-STRUCT-KEYSTATE-2, the finding's own words: `judge(&observe(&e), Remove, &e)` on evidence
/// with only the keyring gathered (five lines, `Active`) returned a Permit that says what the removal
/// leaves. At step 3 that Permit replaces the removal's proof, whose weakness was that anyone could make
/// one: this one was made by gathering less. Each of the three facts the removal rests on is refused on
/// its own, too.
#[test]
fn x_delta_struct_keystate_2_a_removal_judged_on_the_keyring_alone_is_refused() {
    let e = keyring(Some(1), None, None);
    assert_eq!(observe(&e), KeyState::Active);
    match judge(&observe(&e), Op::Remove, &e) {
        Verdict::Refuse(reason) => {
            assert!(matches!(reason.kind, Refusal::NotAsked(_)), "{reason:?}");
            assert!(
                reason.sentence.contains("not looked at"),
                "{}",
                reason.sentence
            );
        }
        Verdict::Permit(permit) => panic!("the removal was permitted ({:?})", permit.stays()),
    }

    let unasked: [(&str, fn(&mut Evidence<'_>)); 3] = [
        ("whether the recovery code was checked", |e| {
            e.token.code_checked = Read::Unasked;
        }),
        ("whether the token server knows the recovery code", |e| {
            e.token.code_registered = Read::Unasked;
        }),
        ("whether the drive's own vault holds the paid months", |e| {
            e.vault.months_elsewhere = Read::Unasked;
        }),
    ];
    for state in [KeyState::Fresh, KeyState::Active] {
        for (what, hide) in unasked {
            let mut e = all_good();
            hide(&mut e);
            assert_eq!(
                removal_verdict(&state, &e),
                Err(Refusal::NotAsked(what)),
                "{state:?}"
            );
        }
    }
}

/// X-DELTA-STRUCT-KEYSTATE-2: the other facts a key-changing cell rests on, one by one: whether the
/// keyring keeps keys (Setup, Rotate, Recover), whether the index opens (Rotate, Re-encrypt: a rotation
/// of this device's own journal excepted), how many further codes the drive holds (AddCode).
#[test]
fn x_delta_struct_keystate_2_the_keyring_the_index_and_the_count_of_codes_are_facts_a_cell_rests_on(
) {
    let ask = |state: &KeyState, op: Op, hide: fn(&mut Evidence<'_>)| {
        let mut e = everything_asked();
        assert!(
            matches!(judge(state, op, &e), Verdict::Permit(_)),
            "{state:?} x {op:?} with everything asked"
        );
        hide(&mut e);
        match judge(state, op, &e) {
            Verdict::Refuse(reason) => Some(reason.kind),
            Verdict::Permit(_) => None,
        }
    };
    let keyring: fn(&mut Evidence<'_>) = |e| e.keyring.keeps_keys = Read::Unasked;
    let index: fn(&mut Evidence<'_>) = |e| e.bucket.index_opens = Read::Unasked;
    let codes: fn(&mut Evidence<'_>) = |e| e.bucket.extra_codes = Read::Unasked;
    for (state, op) in [
        (KeyState::Fresh, Op::Setup),
        (KeyState::Planned, Op::Setup),
        (KeyState::Active, Op::Rotate),
        (KeyState::Active, Op::Recover),
        (KeyState::Locked, Op::Recover),
        (KeyState::Rotating(Start::Resume), Op::Rotate),
    ] {
        assert!(
            matches!(ask(&state, op, keyring), Some(Refusal::NotAsked(_))),
            "{state:?} x {op:?}: the keyring's protection was not asked"
        );
    }
    for (state, op) in [
        (KeyState::Active, Op::Rotate),
        (KeyState::Active, Op::Reencrypt),
        (KeyState::Rotating(Start::Adopt), Op::Rotate),
        (KeyState::Rotating(Start::Again), Op::Rotate),
    ] {
        assert!(
            matches!(ask(&state, op, index), Some(Refusal::NotAsked(_))),
            "{state:?} x {op:?}: whether the index opens was not asked"
        );
    }
    assert_eq!(
        ask(&KeyState::Rotating(Start::Resume), Op::Rotate, index),
        None,
        "a journal of this device's own goes on whatever the index says"
    );
    assert!(matches!(
        ask(&KeyState::Active, Op::AddCode, codes),
        Some(Refusal::NotAsked(_))
    ));
}

/// A setup that stopped between the drive key and the planned key's deletion is finished by the same
/// call: the planned key left beside the drive key is the one cell of the Active row that depends on
/// what the keyring says.
#[test]
fn a_planned_key_left_beside_the_drive_key_lets_setup_finish_and_nothing_else_changes() {
    let e = keyring(Some(1), None, Some(1));
    assert_eq!(observe(&e), KeyState::Active);
    match judge(&KeyState::Active, Op::Setup, &e) {
        Verdict::Permit(permit) => assert_eq!(permit.way(), Way::Finish),
        Verdict::Refuse(reason) => panic!("{reason}"),
    }
    let without = keyring(Some(1), None, None);
    assert!(matches!(
        judge(&KeyState::Active, Op::Setup, &without),
        Verdict::Refuse(reason) if reason.kind == Refusal::EncryptedAlready
    ));
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
    let e = everything_asked();
    for (start, way) in [
        (Start::Resume, Way::Resume),
        (Start::Adopt, Way::Adopt),
        (Start::Again, Way::New),
    ] {
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
    for start in [Start::Resume, Start::Adopt, Start::Again] {
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
    for start in [Start::Resume, Start::Adopt, Start::Again] {
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
    let writes: [(KeyState, Op); 10] = [
        (KeyState::Fresh, Op::Setup),
        (KeyState::Planned, Op::Setup),
        (KeyState::Active, Op::Enroll),
        (KeyState::Active, Op::Rotate),
        (KeyState::Rotating(Start::Resume), Op::Rotate),
        (KeyState::Rotating(Start::Adopt), Op::Rotate),
        (KeyState::Rotating(Start::Again), Op::Rotate),
        (KeyState::Locked, Op::Recover),
        (KeyState::Fresh, Op::Recover),
        (KeyState::Torn(Why::OpensNothing), Op::Recover),
    ];
    for (state, op) in writes {
        let mut e = Evidence::new(DRIVE, NOW);
        e.bucket.index_opens = Read::Found(true);
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
        previous_key: Read::Absent,
        planned_key: planned.map_or(Read::Absent, |n| Read::Found(id(n))),
        keeps_keys: Read::Found(Keeps::Yes),
    };
    e.bucket.index_opens = Read::Found(true);
    e
}

/// [`keyring`] with the key under `drive-key-previous` named too.
fn keyring_of_a_key_change(drive: u8, next: u8, previous: Option<u8>) -> Evidence<'static> {
    let mut e = keyring(Some(drive), Some(next), None);
    e.keyring.previous_key = previous.map_or(Read::Absent, |n| Read::Found(id(n)));
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
            "previous key",
            "planned key",
            "keeps keys",
            "encrypted",
            "index opens"
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
        known = [
            ("locked", "X-TEST-2: another row"),
            ("active", "X-TEST-1: the guard is stricter than the table"),
        ]
    );
    let noted = disagreements_at("tests::macro_known");
    assert_eq!(noted.len(), 1);
    assert_eq!(
        noted[0].known,
        Some("X-TEST-1: the guard is stricter than the table")
    );
    assert!(noted[0].to_string().contains("[known: X-TEST-1"));
}

/// A finding is about one row: the same disagreement in another row is a disagreement.
#[test]
#[should_panic(expected = "tests::macro_known_elsewhere")]
fn a_known_finding_excuses_only_the_row_it_names() {
    let e = keyring(Some(1), None, None);
    crate::key_shadow!(
        "tests::macro_known_elsewhere",
        Op::Rotate,
        e,
        Guard::Refuses,
        known = [("locked", "X-TEST-2: another row")]
    );
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

/// footprint.rs is the second list of what a drive leaves in the keyring: the entries that go only
/// with a proof. They are the registry's own list less the credentials (a session and a drive token
/// stay or go without one): two lists that cannot drift apart unseen.
#[test]
fn the_entries_that_go_with_a_proof_are_the_ones_the_registry_does_not_call_credentials() {
    for entry in KeyEntry::ALL {
        assert_eq!(
            entry.goes_with_proof(),
            owner(entry) != Owner::Credential,
            "{entry:?}"
        );
    }
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

/// A minimal provider for the adapters and the rotations: the index in memory, nothing sealed, and
/// a rekey that fails as often as a test asks.
struct Provider {
    index: Arc<MemoryIndex>,
    rekey_failures: AtomicUsize,
}

impl Provider {
    fn new() -> Provider {
        Provider {
            index: Arc::new(MemoryIndex::new()),
            rekey_failures: AtomicUsize::new(0),
        }
    }
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
        if self.rekey_failures.load(Ordering::SeqCst) > 0 {
            self.rekey_failures.fetch_sub(1, Ordering::SeqCst);
            return Err(DriveError::Io(String::from("the network went")));
        }
        Ok(())
    }
}

fn permit_of(state: &KeyState, op: Op) -> Permit {
    match judge(state, op, &everything_asked()) {
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
    let provider = Provider::new();
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

// ==== The local observers over real configurations ====

/// A keyring that cannot be read.
struct Unreadable;

impl KeyringStore for Unreadable {
    fn get(&self, _key: &str) -> Result<Option<String>, KeyringError> {
        Err(KeyringError::Failed(String::from("the keyring went")))
    }
    fn set(&self, _key: &str, _secret: &str) -> Result<(), KeyringError> {
        Err(KeyringError::Failed(String::from("the keyring went")))
    }
    fn delete(&self, _key: &str) -> Result<(), KeyringError> {
        Err(KeyringError::Failed(String::from("the keyring went")))
    }
}

/// Everything about a drive that a caller can gather without a rotation's help: the keyring's
/// entries, whether the bucket lists key files, and the journal as the two observers of the rotation
/// see it.
fn gathered(bucket: &MemBucket, keyring: &dyn KeyringStore) -> Evidence<'static> {
    let mut e = Evidence::new(DRIVE, NOW);
    e.keyring = device::key_facts(keyring, DRIVE);
    e.bucket.encrypted = Read::value(device::is_encrypted(bucket));
    e.bucket.journal = match rotation::resumable(bucket, keyring, DRIVE) {
        Ok(Some(_)) => Read::Found(Journal::Own),
        Ok(None) => match rotation::pending(bucket) {
            Ok(Some(_)) | Err(DriveError::Corrupt { .. }) => Read::Found(Journal::NotOwn),
            Ok(None) => Read::Absent,
            Err(error) => Read::Failed(error.to_string()),
        },
        Err(error) => Read::Failed(error.to_string()),
    };
    e
}

/// `observe` is the union of the local observers: over seven real configurations of a drive it says
/// what each of them says (`rotation::unfinished`, `resumable`, `start`, `lockdown_needed`,
/// `device::setup_pending`, `encrypted::judge`). The calls of `start` and `lockdown_needed` run
/// their own shadows too, and a disagreement there fails this test (the macro asserts).
#[test]
fn observe_says_what_each_local_observer_says_over_real_configurations() {
    let drive_key_of = |keyring: &dyn KeyringStore| device::load_drive_key(keyring, DRIVE).unwrap();

    // Fresh: nothing anywhere.
    let bucket = MemBucket::new();
    let keyring = MemoryKeyring::new();
    let e = gathered(&bucket, &keyring);
    assert_eq!(observe(&e), KeyState::Fresh);
    assert!(!rotation::unfinished(&keyring, DRIVE).unwrap());
    assert!(!device::setup_pending(&keyring, DRIVE).unwrap());
    assert_eq!(
        enc_judge(&bucket, &keyring, DRIVE, Recorded::default()).unwrap(),
        EncState::Unencrypted(Unencrypted::New)
    );

    // Planned: a key kept for the setup, and nothing in the bucket yet.
    let key = DriveKey::generate().unwrap();
    device::plan_drive_key(&keyring, DRIVE, &key).unwrap();
    let e = gathered(&bucket, &keyring);
    assert_eq!(observe(&e), KeyState::Planned);
    assert!(device::setup_pending(&keyring, DRIVE).unwrap());
    assert_eq!(
        enc_judge(&bucket, &keyring, DRIVE, Recorded::default()).unwrap(),
        EncState::SetupPending
    );

    // Active: the setup finished.
    let bucket: Arc<MemBucket> = Arc::new(MemBucket::new());
    let keyring = MemoryKeyring::new();
    setup_new_drive(bucket.as_ref(), &keyring, DRIVE, cheap()).unwrap();
    let e = gathered(&bucket, &keyring);
    assert_eq!(observe(&e), KeyState::Active);
    assert!(!rotation::unfinished(&keyring, DRIVE).unwrap());
    assert!(!device::setup_pending(&keyring, DRIVE).unwrap());
    assert!(rotation::resumable(bucket.as_ref(), &keyring, DRIVE)
        .unwrap()
        .is_none());
    assert!(matches!(
        rotation::start(bucket.as_ref(), &keyring, DRIVE),
        Ok(rotation::Start::Fresh { .. })
    ));
    assert!(lockdown_needed(bucket.as_ref(), &keyring, DRIVE).unwrap());
    assert!(matches!(
        enc_judge(bucket.as_ref(), &keyring, DRIVE, Recorded::default()).unwrap(),
        EncState::Encrypted {
            via: Via::KeyInKeyring,
            ..
        }
    ));

    // Locked: the same drive on a device without a key.
    let stranger = MemoryKeyring::new();
    let e = gathered(&bucket, &stranger);
    assert_eq!(observe(&e), KeyState::Locked);
    assert!(matches!(
        enc_judge(bucket.as_ref(), &stranger, DRIVE, Recorded::default()).unwrap(),
        EncState::Encrypted {
            via: Via::KeyFilesListed,
            ..
        }
    ));
    assert!(rotation::start(bucket.as_ref(), &stranger, DRIVE).is_err());

    // Rotating (Resume): a rotation of this device that stopped at its first step.
    let provider = Provider::new();
    provider.rekey_failures.store(1, Ordering::SeqCst);
    let arc_bucket: Arc<dyn Drive> = bucket.clone();
    assert!(rotate(arc_bucket.clone(), &keyring, DRIVE, &provider, cheap()).is_err());
    let e = gathered(&bucket, &keyring);
    assert_eq!(observe(&e), KeyState::Rotating(Start::Resume));
    assert!(rotation::unfinished(&keyring, DRIVE).unwrap());
    assert!(rotation::resumable(bucket.as_ref(), &keyring, DRIVE)
        .unwrap()
        .is_some());
    assert!(matches!(
        rotation::start(bucket.as_ref(), &keyring, DRIVE),
        Ok(rotation::Start::Resume(_))
    ));
    assert!(!lockdown_needed(bucket.as_ref(), &keyring, DRIVE).unwrap());
    // ... and on the other device, which has no new key, nothing of its own is unfinished
    assert!(!rotation::unfinished(&stranger, DRIVE).unwrap());

    // Rotating (Adopt): the journal went (deleted, or replaced by a planted one).
    let journal = bucket.object(JOURNAL_FILE).unwrap();
    bucket.delete(JOURNAL_FILE).unwrap();
    let e = gathered(&bucket, &keyring);
    assert_eq!(observe(&e), KeyState::Rotating(Start::Adopt));
    assert!(rotation::unfinished(&keyring, DRIVE).unwrap());
    assert!(rotation::resumable(bucket.as_ref(), &keyring, DRIVE)
        .unwrap()
        .is_none());
    assert!(matches!(
        rotation::start(bucket.as_ref(), &keyring, DRIVE),
        Ok(rotation::Start::Adopt { .. })
    ));
    assert!(lockdown_needed(bucket.as_ref(), &keyring, DRIVE).unwrap());
    bucket.set(JOURNAL_FILE, b"{\"planted\": true}".to_vec());
    let e = gathered(&bucket, &keyring);
    assert_eq!(
        observe(&e),
        KeyState::Rotating(Start::Adopt),
        "a planted journal is not this device's"
    );
    assert!(lockdown_needed(bucket.as_ref(), &keyring, DRIVE).unwrap());
    bucket.set(JOURNAL_FILE, journal);

    // Torn: the new key is kept and the drive key it replaces is gone, and the journal is not ours.
    let drive_key_entry = device::drive_key_entry(DRIVE);
    let held = drive_key_of(&keyring).unwrap();
    bucket.delete(JOURNAL_FILE).unwrap();
    keyring.delete(&drive_key_entry).unwrap();
    let e = gathered(&bucket, &keyring);
    assert_eq!(observe(&e), KeyState::Torn(Why::OldKeyGone));
    assert!(
        rotation::unfinished(&keyring, DRIVE).unwrap(),
        "the drive key is not the new key"
    );
    assert!(rotation::start(bucket.as_ref(), &keyring, DRIVE).is_err());
    assert!(lockdown_needed(bucket.as_ref(), &keyring, DRIVE).is_err());
    device::store_drive_key(&keyring, DRIVE, &held).unwrap();

    // Unknown: a keyring that cannot be read says nothing, and the observers fail with it.
    let e = gathered(&bucket, &Unreadable);
    assert!(
        matches!(observe(&e), KeyState::Unknown(Why::Unreadable { .. })),
        "{:?}",
        observe(&e)
    );
    assert!(rotation::unfinished(&Unreadable, DRIVE).is_err());
    assert!(device::setup_pending(&Unreadable, DRIVE).is_err());
    assert!(rotation::start(bucket.as_ref(), &Unreadable, DRIVE).is_err());
}

/// X-DELTA-AZDRIVE-KEYS-6, X-KEYSTATE-4: whatever the keyring could not say, the key change is refused
/// BEFORE the account's lockdown. The table permits the lockdown by itself where a read failed (a hostile
/// bucket must not stop "I was hacked"), but the combined flow asks the rotation with it: a lockdown that
/// revokes every other device for a drive whose key this device cannot replace would lock its owner out.
/// `rotation::ask` gives the permit of neither.
#[test]
fn x_delta_azdrive_keys_6_a_key_change_whose_keyring_cannot_be_read_is_refused_before_the_lockdown()
{
    let bucket = MemBucket::new();
    assert!(rotation::ask(&bucket, &Unreadable, DRIVE).is_err());
    assert!(lockdown_needed(&bucket, &Unreadable, DRIVE).is_err());
}

// ==== X-DELTA-STRUCT-KEYSTATE-1: a rotation that stopped after its Members step ====

/// Evidence of a drive whose recovery code was checked and registered, built on `e`'s keyring and
/// bucket facts: what a Remove of it is judged on.
fn with_a_good_proof(mut e: Evidence<'static>) -> Evidence<'static> {
    let good = all_good();
    e.token = good.token;
    e.vault = good.vault;
    e
}

/// A rotation whose Members step ran has the new key as the drive key (K2 in `drive-key` and in
/// `drive-key-next`) and the key it replaces still in `drive-key-previous`: the recovery wrap in the
/// bucket still seals K1, and K2 is in no other place than this keyring. That is a key change that is
/// not over, whether or not its journal is in the bucket.
#[test]
fn x_delta_struct_keystate_1_a_new_key_that_is_the_drive_key_with_the_previous_key_kept_is_a_key_change_not_over(
) {
    let mut e = keyring_of_a_key_change(2, 2, Some(1));
    e.bucket.journal = Read::Found(Journal::Own);
    assert_eq!(observe(&e), KeyState::Rotating(Start::Resume));

    // Deleted, planted, nobody's: the next key change starts over.
    for journal in [Read::Absent, Read::Found(Journal::NotOwn)] {
        e.bucket.journal = journal.clone();
        assert_eq!(observe(&e), KeyState::Rotating(Start::Again), "{journal:?}");
    }

    // Not looked at: the journal is what tells a rotation of this device from a finished one that
    // left its entry, and nothing is erased on a guess.
    e.bucket.journal = Read::Unasked;
    assert_eq!(
        observe(&e),
        KeyState::Unknown(Why::NotAsked("the key change's journal"))
    );
    e.bucket.journal = Read::Failed(String::from("no answer"));
    assert_eq!(
        observe(&e),
        unreadable("the key change's journal", "no answer")
    );

    // The index opens with none of the keys this device holds: torn, as for any other key change.
    e.bucket.journal = Read::Absent;
    e.bucket.index_opens = Read::Found(false);
    assert_eq!(observe(&e), KeyState::Torn(Why::OpensNothing));
    e.bucket.index_opens = Read::Found(true);
    assert_eq!(observe(&e), KeyState::Rotating(Start::Again));

    // The previous key is a fact the state rests on once the new key is the drive key.
    e.keyring.previous_key = Read::Failed(String::from("the keyring refused"));
    assert_eq!(
        observe(&e),
        unreadable("the key a key change replaces", "the keyring refused")
    );
    e.keyring.previous_key = Read::Unasked;
    assert_eq!(
        observe(&e),
        KeyState::Unknown(Why::NotAsked("the key a key change replaces"))
    );
}

/// The other two configurations of `drive-key-next == drive-key`: a finished rotation whose cleanup took
/// the previous key and not the new key's entry (nothing under way: it is Active, as before), and the
/// previous key alone, which says nothing.
#[test]
fn x_delta_struct_keystate_1_a_finished_rotations_leftover_and_a_previous_key_alone_are_no_key_change_under_way(
) {
    let finished = keyring_of_a_key_change(2, 2, None);
    assert_eq!(observe(&finished), KeyState::Active);

    let mut alone = keyring(Some(1), None, None);
    alone.keyring.previous_key = Read::Found(id(1));
    assert_eq!(observe(&alone), KeyState::Active);
    alone.keyring.drive_key = Read::Absent;
    alone.bucket.encrypted = Read::Found(true);
    assert_eq!(observe(&alone), KeyState::Locked);
}

/// The Remove cell of that state: not even a perfect proof lets the keys go (the judge's side of the
/// finding; `forget_job` and `KeyringFootprint::forget` are the production entries below).
#[test]
fn x_delta_struct_keystate_1_remove_is_refused_for_a_key_change_past_its_members_step_whatever_the_proof_says(
) {
    for journal in [
        Read::Found(Journal::Own),
        Read::Found(Journal::NotOwn),
        Read::Absent,
    ] {
        let mut e = with_a_good_proof(keyring_of_a_key_change(2, 2, Some(1)));
        e.bucket.journal = journal.clone();
        match judge(&observe(&e), Op::Remove, &e) {
            Verdict::Refuse(reason) => {
                assert_eq!(reason.kind, Refusal::KeyChangeUnfinished, "{journal:?}");
            }
            Verdict::Permit(_) => panic!("{journal:?}: the keys were let go"),
        }
    }
    // the same keyring without the previous key is a finished rotation: the proof decides
    let finished = with_a_good_proof(keyring_of_a_key_change(2, 2, None));
    assert!(matches!(
        judge(&observe(&finished), Op::Remove, &finished),
        Verdict::Permit(_)
    ));
}

/// A bucket whose n-th write (a put, a conditional put or a delete, counted from 1) is refused, as a
/// network that went at that moment: every write before it lands, every read answers. `0`: none fails.
struct NthWrite {
    inner: Arc<MemBucket>,
    nth: usize,
    seen: AtomicUsize,
}

impl NthWrite {
    fn new(inner: Arc<MemBucket>, nth: usize) -> NthWrite {
        NthWrite {
            inner,
            nth,
            seen: AtomicUsize::new(0),
        }
    }

    fn writes(&self) -> usize {
        self.seen.load(Ordering::SeqCst)
    }

    fn write(&self) -> Result<(), DriveError> {
        let n = self.seen.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.nth {
            Err(DriveError::Transport(String::from("the network went")))
        } else {
            Ok(())
        }
    }
}

impl Drive for NthWrite {
    fn list(&self, request: &crate::ListRequest) -> Result<crate::ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: crate::ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.write()?;
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.write()?;
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<crate::ObjectInfo, DriveError> {
        self.inner.head(key)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &crate::Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.write()?;
        self.inner.put_if(key, bytes, condition)
    }
}

/// A drive set up on a device: its bucket and its keyring.
fn a_drive_set_up() -> (Arc<MemBucket>, MemoryKeyring) {
    let bucket = Arc::new(MemBucket::new());
    let keyring = MemoryKeyring::new();
    setup_new_drive(bucket.as_ref(), &keyring, DRIVE, cheap()).unwrap();
    (bucket, keyring)
}

/// What a Remove of the drive with a perfect proof is judged to be, on the evidence of the keyring and
/// the bucket as they are now.
fn removal_now(bucket: &MemBucket, keyring: &dyn KeyringStore) -> Verdict {
    let e = with_a_good_proof(gathered(bucket, keyring));
    judge(&observe(&e), Op::Remove, &e)
}

/// The window after EVERY write of a rotation: the network goes at the n-th write, for each n. The
/// rotation that stopped is unfinished for the guard and refused by the judge, in the keyring it
/// leaves and in the one an ordinary open makes of it (the member wrap of the new key brings the new
/// key in as the drive key: the state after a stop at the Members step), with its journal in the
/// bucket and without; the same call finishes it, and then the keys may go. This is the production
/// function `rotate`, stopped in each of its windows (X-DELTA-STRUCT-KEYSTATE-1, X-DELTA-AZDRIVE-KEYS-2).
#[test]
fn x_delta_struct_keystate_1_a_rotation_stopped_at_any_write_keeps_every_key_until_it_is_over() {
    let (bucket, keyring) = a_drive_set_up();
    let counting = Arc::new(NthWrite::new(bucket.clone(), 0));
    let counted: Arc<dyn Drive> = counting.clone();
    rotate(counted, &keyring, DRIVE, &Provider::new(), cheap()).unwrap();
    let writes = counting.writes();
    assert!(
        writes >= 6,
        "a rotation writes its journal, the chain, the member wrap and the recovery wrap: {writes}"
    );

    let mut windows = 0;
    for nth in 1..=writes {
        for (opened, journal_gone) in [(false, false), (true, false), (true, true)] {
            let (bucket, keyring) = a_drive_set_up();
            let failing: Arc<dyn Drive> = Arc::new(NthWrite::new(bucket.clone(), nth));
            let stopped = rotate(failing, &keyring, DRIVE, &Provider::new(), cheap());
            let finished_anyway = match stopped {
                // The last steps are said, not raised: the keys are changed.
                Ok(done) => {
                    assert!(
                        !done.left_over.is_empty(),
                        "write {nth} failed and the rotation says nothing was left"
                    );
                    true
                }
                Err(_) => false,
            };
            if finished_anyway {
                assert!(
                    !rotation::unfinished(&keyring, DRIVE).unwrap(),
                    "write {nth}"
                );
                assert!(
                    matches!(removal_now(&bucket, &keyring), Verdict::Permit(_)),
                    "write {nth}: a finished rotation leaves the removal to its proof"
                );
                continue;
            }
            windows += 1;
            if opened {
                // An ordinary open: the member wrap of the new key (when the rotation got that far)
                // makes the new key the keyring's drive key.
                let _ = device::catch_up(bucket.as_ref(), &keyring, DRIVE);
            }
            if journal_gone {
                let _ = bucket.delete(JOURNAL_FILE);
            }
            let what = format!("write {nth} failed (opened {opened}, journal gone {journal_gone})");
            assert!(rotation::unfinished(&keyring, DRIVE).unwrap(), "{what}");
            match removal_now(&bucket, &keyring) {
                Verdict::Refuse(_) => {}
                Verdict::Permit(_) => panic!("{what}: the judge let the keys go"),
            }

            // The same call finishes it.
            let again: Arc<dyn Drive> = bucket.clone();
            rotate(again, &keyring, DRIVE, &Provider::new(), cheap())
                .unwrap_or_else(|e| panic!("{what}: the rotation does not go on: {e}"));
            assert!(!rotation::unfinished(&keyring, DRIVE).unwrap(), "{what}");
            assert!(
                matches!(removal_now(&bucket, &keyring), Verdict::Permit(_)),
                "{what}: nothing is under way any more"
            );
        }
    }
    assert!(
        windows >= 6,
        "the windows of a rotation were reached: {windows}"
    );
}

/// A keyring that refuses to write the drive key once it is told to (the entry's own write, after
/// the rotation's other entries were written): the keychain's prompt answered "No" at the step that
/// makes the new key the drive key.
struct RefusesTheDriveKey<'a> {
    inner: &'a MemoryKeyring,
    entry: String,
    armed: std::sync::atomic::AtomicBool,
}

impl KeyringStore for RefusesTheDriveKey<'_> {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        self.inner.get(key)
    }
    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        if key == self.entry && self.armed.load(Ordering::SeqCst) {
            return Err(KeyringError::Denied);
        }
        self.inner.set(key, secret)
    }
    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        self.inner.delete(key)
    }
}

/// The stop after the member wrap of the new key went into the bucket and before the keyring's drive
/// key became the new one (R9e), and what an ordinary open makes of it: `catch_up` takes the new key
/// from the member wrap and writes it as the drive key, and `drive-key-previous` is still there. That
/// keyring is the one the first version of the guard called finished (X-DELTA-STRUCT-KEYSTATE-1: the
/// review's second round noted that an open reaches it as well as a stopped `rotate`).
#[test]
fn x_delta_struct_keystate_1_an_ordinary_open_after_the_member_wrap_leaves_the_keyring_of_a_key_change_not_over(
) {
    let (bucket, keyring) = a_drive_set_up();
    let old = device::load_drive_key(&keyring, DRIVE).unwrap().unwrap();
    let refusing = RefusesTheDriveKey {
        inner: &keyring,
        entry: device::drive_key_entry(DRIVE),
        armed: std::sync::atomic::AtomicBool::new(true),
    };
    let arc_bucket: Arc<dyn Drive> = bucket.clone();
    assert!(rotate(arc_bucket, &refusing, DRIVE, &Provider::new(), cheap()).is_err());

    // Stopped: the drive key is still K1, the new key and the previous are kept, the member wrap of
    // the new key is in the bucket.
    assert_eq!(
        device::load_drive_key(&keyring, DRIVE).unwrap(),
        Some(old.clone())
    );
    assert!(rotation::unfinished(&keyring, DRIVE).unwrap());

    // An ordinary open: the keyring's drive key becomes the new key.
    let (new, before) = device::catch_up(bucket.as_ref(), &keyring, DRIVE)
        .unwrap()
        .expect("the wrap holds the new key, which the keyring did not");
    assert_eq!(before, old);
    let (_, next_entry) = device::rotation_key_entries(DRIVE);
    assert_eq!(
        device::load_key_at(&keyring, &next_entry).unwrap(),
        Some(new.clone()),
        "drive-key == drive-key-next now"
    );
    assert!(
        rotation::unfinished(&keyring, DRIVE).unwrap(),
        "the old guard called this finished: the drive key IS the next key"
    );
    for journal_gone in [false, true] {
        if journal_gone {
            bucket.delete(JOURNAL_FILE).unwrap();
        }
        match removal_now(&bucket, &keyring) {
            Verdict::Refuse(reason) => assert_eq!(reason.kind, Refusal::KeyChangeUnfinished),
            Verdict::Permit(_) => {
                panic!("journal gone {journal_gone}: the only copy of the new key went")
            }
        }
    }
}

// ==== X-DELTA-STRUCT-KEYSTATE-5: a planned key finishes a setup only when it is the drive key ====

/// A planned key beside the drive key finishes a setup only when it IS the drive key: the setup that
/// stopped between its key and the planned key's deletion. A planned key of another id (a checkout's,
/// for a drive whose key this device holds) is no copy of the drive key: "finishing" would put it in the
/// drive key's place, and everything sealed under the drive key would open with nothing.
#[test]
fn x_delta_struct_keystate_5_a_planned_key_of_another_id_beside_the_drive_key_does_not_finish_a_setup(
) {
    let other = keyring(Some(1), None, Some(2));
    assert_eq!(observe(&other), KeyState::Active);
    match judge(&KeyState::Active, Op::Setup, &other) {
        Verdict::Refuse(reason) => assert_eq!(reason.kind, Refusal::EncryptedAlready),
        Verdict::Permit(permit) => panic!("the setup was let finish ({:?})", permit.way()),
    }
    // the same id: the setup that stopped before the planned key's deletion
    let same = keyring(Some(1), None, Some(1));
    assert!(matches!(
        judge(&KeyState::Active, Op::Setup, &same),
        Verdict::Permit(permit) if permit.way() == Way::Finish
    ));
    // a planned key whose read failed is no key to finish with
    let mut failed = keyring(Some(1), None, Some(1));
    failed.keyring.planned_key = Read::Failed(String::from("the keyring refused"));
    assert!(matches!(
        judge(&KeyState::Active, Op::Setup, &failed),
        Verdict::Refuse(reason) if reason.kind == Refusal::EncryptedAlready
    ));
}

// ==== C7 step 3: `rotate` takes the permit of its own operation ====

/// A keyring that keeps its entries in a plain file the user did not allow to hold keys.
struct PlainFileKeyring<'a>(&'a MemoryKeyring);

impl KeyringStore for PlainFileKeyring<'_> {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        self.0.get(key)
    }
    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        self.0.set(key, secret)
    }
    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        self.0.delete(key)
    }
    fn protection(&self) -> crate::keyring::Protection {
        crate::keyring::Protection::PlainFile {
            allow: crate::keyring::Allow::Nothing,
        }
    }
}

/// `rotate` changes the drive's key only for the permit of a rotation of THIS drive: the lockdown's
/// permit of the same `ask` is the permit of another operation, and the permit of another drive is for
/// another drive. Nothing in the bucket or the keyring changes for either.
#[test]
fn a_rotation_takes_a_permit_of_its_own_operation_and_drive_and_changes_nothing_for_another() {
    let (bucket, keyring) = a_drive_set_up();
    let drive_key = device::load_drive_key(&keyring, DRIVE).unwrap().unwrap();
    let objects = bucket.objects();

    let rotation = rotation::ask(bucket.as_ref(), &keyring, DRIVE).unwrap();
    let lockdown = rotation
        .lockdown()
        .expect("a rotation that starts has its lockdown still to run");
    assert_eq!(lockdown.op(), Op::Lockdown);
    let again: Arc<dyn Drive> = bucket.clone();
    let refused = rotation::rotate(lockdown, again, &keyring, DRIVE, &Provider::new(), cheap());
    assert!(
        matches!(&refused, Err(DriveError::Denied { message }) if message.contains("lockdown")),
        "{refused:?}"
    );

    // the permit of a rotation of another drive
    let (other_bucket, other_keyring) = {
        let bucket = Arc::new(MemBucket::new());
        let keyring = MemoryKeyring::new();
        setup_new_drive(bucket.as_ref(), &keyring, "d_other", cheap()).unwrap();
        (bucket, keyring)
    };
    let other = rotation::ask(other_bucket.as_ref(), &other_keyring, "d_other").unwrap();
    let again: Arc<dyn Drive> = bucket.clone();
    let refused = rotation::rotate(
        other.permit(),
        again,
        &keyring,
        DRIVE,
        &Provider::new(),
        cheap(),
    );
    assert!(
        matches!(&refused, Err(DriveError::Denied { message }) if message.contains("d_other")),
        "{refused:?}"
    );

    assert_eq!(bucket.objects(), objects, "nothing was written");
    assert_eq!(
        device::load_drive_key(&keyring, DRIVE).unwrap(),
        Some(drive_key),
        "the keyring is as it was"
    );
    let (previous, next) = device::rotation_key_entries(DRIVE);
    assert!(keyring.get(&previous).unwrap().is_none() && keyring.get(&next).unwrap().is_none());
}

/// A permit is the answer of one moment: asked before a rotation of this device started, it does not
/// start another over the one that stopped. The new `ask` says so too: the rotation resumes, and its
/// lockdown is not run again.
#[test]
fn a_permit_asked_before_a_key_change_started_does_not_start_another_over_the_one_that_stopped() {
    let (bucket, keyring) = a_drive_set_up();
    let stale = rotation::ask(bucket.as_ref(), &keyring, DRIVE).unwrap();
    assert_eq!(stale.permit().way(), Way::New);

    // A rotation of this device stops after its journal went in (the third write is refused).
    let failing: Arc<dyn Drive> = Arc::new(NthWrite::new(bucket.clone(), 3));
    assert!(rotate(failing, &keyring, DRIVE, &Provider::new(), cheap()).is_err());
    assert!(rotation::unfinished(&keyring, DRIVE).unwrap());
    let (_, next) = device::rotation_key_entries(DRIVE);
    let kept = device::load_key_at(&keyring, &next).unwrap();
    let objects = bucket.objects();

    let again: Arc<dyn Drive> = bucket.clone();
    let refused = rotation::rotate(
        stale.permit(),
        again,
        &keyring,
        DRIVE,
        &Provider::new(),
        cheap(),
    );
    assert!(
        matches!(&refused, Err(DriveError::Denied { message }) if message.contains("changed since")),
        "{refused:?}"
    );
    assert_eq!(bucket.objects(), objects, "nothing was written");
    assert_eq!(
        device::load_key_at(&keyring, &next).unwrap(),
        kept,
        "no second new key was made over the first"
    );

    let fresh = rotation::ask(bucket.as_ref(), &keyring, DRIVE).unwrap();
    assert_eq!(fresh.permit().way(), Way::Resume);
    assert!(
        fresh.lockdown().is_none(),
        "the rotation that stopped has had its lockdown"
    );
}

/// A keyring that keeps no key material is refused BEFORE the lockdown (KEYS-25), by the judge now: `ask`
/// gives a permit of neither, in the sentence of the keyring's protection, and nothing is read from
/// the bucket for it.
#[test]
fn a_key_change_on_a_keyring_that_keeps_no_key_is_refused_before_the_lockdown() {
    let (bucket, keyring) = a_drive_set_up();
    let plain = PlainFileKeyring(&keyring);
    let refused = rotation::ask(bucket.as_ref(), &plain, DRIVE);
    assert!(
        matches!(&refused, Err(DriveError::InvalidConfig(why)) if why.contains("plain file")),
        "{:?}",
        refused.as_ref().err()
    );
}
