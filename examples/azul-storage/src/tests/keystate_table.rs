//! The table of the key permit's judge (ARCH29 C7) as the specification, the observation of the state, and
//! the cells that depend on what was read. A CHILD module of `crypto::keystate` (declared there with a
//! `#[path]`): the state and the judge are private to that module, and this is the one place that reaches
//! them. A test that makes a Permit does it through `ask` on a fixture - `tests/keystate.rs` - never from a
//! state of its own making.
//!
//! The table is stated here as an exhaustive `match` over `(KeyState, Op)` without a catch-all: a new
//! state or a new operation does not compile until its cells are decided. The cells that carry a
//! finding of the key review (X-DELTA-AZDRIVE-KEYS-1, -2, -3, -6, -7, -8, -18, -32) are named where
//! they are.

use std::collections::BTreeSet;

use super::*;

const DRIVE: &str = "d_ks";
const NOW: u64 = 1_000_000;

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
        Why::CannotResume => Refusal::CannotResume,
        Why::Unreadable { what, why } => Refusal::Unreadable {
            what,
            why: why.clone(),
        },
        Why::NotAsked(what) => Refusal::NotAsked(what),
    }
}

/// THE SPECIFICATION: all 7 x 14 cells.
fn spec(state: &KeyState, op: Op) -> Cell {
    use KeyState as S;
    match (state, op) {
        // ---- Fresh: no key here, and the drive is not known to be encrypted
        (S::Fresh, Op::Setup) => Cell::Permit(Way::New, NO_STAYS),
        (
            S::Fresh,
            Op::Enroll
            | Op::Rotate
            | Op::AddCode
            | Op::RemoveCode
            | Op::NewCode
            | Op::Invite
            | Op::RemoveMember
            | Op::Reencrypt
            | Op::Lockdown
            | Op::DropKey,
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
            | Op::NewCode
            | Op::Invite
            | Op::RemoveMember
            | Op::Recover
            | Op::Reencrypt
            | Op::RestoreAsOf
            | Op::Lockdown
            | Op::DropKey,
        ) => Cell::Refuse(Refusal::SetupUnfinished),

        // ---- Active: the drive key is here and no key change is under way
        (S::Active, Op::Setup) => Cell::Refuse(Refusal::EncryptedAlready),
        (
            S::Active,
            Op::Enroll
            | Op::AddCode
            | Op::RemoveCode
            | Op::NewCode
            | Op::Invite
            | Op::RemoveMember
            | Op::Recover
            | Op::Reencrypt
            | Op::Lockdown,
        ) => Cell::Permit(Way::Plain, NO_STAYS),
        (S::Active, Op::Rotate) => Cell::Permit(Way::New, NO_STAYS),
        (S::Active, Op::Remove) => Cell::Removal,
        // A restore keeps the recorded key id (KEYS-18)
        (S::Active, Op::RestoreAsOf) => Cell::Permit(Way::Plain, &[Stays::RecordedKeyId]),
        // incoming mail is turned on or off through a drop key file this device's key opens, or none
        // (X-DELTA-KEYS3-2: `drop_key_cell_*` below carry the cells that depend on the file)
        (S::Active, Op::DropKey) => Cell::Permit(Way::Plain, NO_STAYS),

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
        (S::Rotating(_), Op::AddCode | Op::RemoveCode | Op::NewCode) => {
            Cell::Refuse(Refusal::WrapMayChange)
        }
        // the keyring's new key may be the only copy (KEYS-2); an invite made now hands out the key being
        // replaced and the key change deletes it
        (
            S::Rotating(_),
            Op::Remove
            | Op::Invite
            | Op::RemoveMember
            | Op::Recover
            | Op::Reencrypt
            | Op::RestoreAsOf
            // the key change makes the drop key anew, in its own step, and keeps the one before it
            | Op::DropKey,
        ) => Cell::Refuse(Refusal::KeyChangeUnfinished),
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
            | Op::NewCode
            | Op::Invite
            | Op::RemoveMember
            | Op::Reencrypt
            | Op::RestoreAsOf
            | Op::Lockdown
            | Op::DropKey,
        ) => Cell::Refuse(why_refuses(why)),

        // ---- Locked: encrypted, and this device has no key
        (S::Locked, Op::Recover) => Cell::Permit(Way::Plain, NO_STAYS),
        // the setup's own refusal of an encrypted drive (`the_sentence_of_an_encrypted_drive_...`)
        (S::Locked, Op::Setup) => Cell::Refuse(Refusal::EncryptedAlready),
        // what this computer keeps of a drive it holds no drive key of goes on the removal's conditions
        (S::Locked, Op::Remove) => Cell::Removal,
        (
            S::Locked,
            Op::Enroll
            | Op::Rotate
            | Op::AddCode
            | Op::RemoveCode
            | Op::NewCode
            | Op::Invite
            | Op::RemoveMember
            | Op::Reencrypt
            | Op::RestoreAsOf
            | Op::Lockdown
            | Op::DropKey,
        ) => Cell::Refuse(Refusal::Locked),

        // ---- Unknown: a read failed, and a failed read is never "nothing there" (KEYS-6)
        (S::Unknown(_), Op::Lockdown) => Cell::Permit(Way::Plain, NO_STAYS),
        // a removal does not rest on whether the bucket lists key files (it reads no bucket)
        (S::Unknown(Why::NotAsked("whether the drive is encrypted")), Op::Remove) => Cell::Removal,
        // A recovery is judged on the entry it writes, the drive key: a device with no key may take one (the
        // keyring's protection is asked). Exactly FOUR (why, fact) pairs permit it, each decided with its reason
        // (X-DELTA-INTEGRITY3-2, -4; the other eight pairs of `states()` fall to the refusal below):
        // - NotAsked(whether the drive is encrypted): a recovery reads no bucket before it runs, and the unasked
        //   listing cannot tell a Fresh device from a Locked one - both permit Recover (Plain), and this cell is
        //   their union. It was Refuse(NotAsked) before 079cc11fe. The property still guarded: a keyring that
        //   cannot be read (Unreadable drive key), one that keeps no key (`keeps_keys`, asked) and a key change's
        //   journal nobody looked at (KeyChangeUnfinished) take no recovered key;
        // - Unreadable(the new key | the key it replaces | the setup's key): a key change's or a setup's entry
        //   that cannot be read does not lock out the user who holds the recovery code, the way back in. The
        //   control: Unreadable(the drive key) - the entry a recovery writes - refuses.
        // A listing that was asked and FAILED, and a keyring entry nobody asked, are no yes: refused.
        (
            S::Unknown(
                Why::NotAsked("whether the drive is encrypted")
                | Why::Unreadable {
                    what:
                        "the new key of a key change"
                        | "the key a key change replaces"
                        | "the key kept for the drive's setup",
                    ..
                },
            ),
            Op::Recover,
        ) => Cell::Permit(Way::Plain, NO_STAYS),
        // a key change that is not over whose journal nobody looked at: refused in the key change's own words
        (
            S::Unknown(Why::NotAsked("the key change's journal")),
            Op::AddCode | Op::RemoveCode | Op::NewCode,
        ) => Cell::Refuse(Refusal::WrapMayChange),
        (
            S::Unknown(Why::NotAsked("the key change's journal")),
            Op::Remove
            | Op::Invite
            | Op::RemoveMember
            | Op::Recover
            | Op::Reencrypt
            | Op::RestoreAsOf
            | Op::DropKey,
        ) => Cell::Refuse(Refusal::KeyChangeUnfinished),
        (
            S::Unknown(why),
            Op::Setup
            | Op::Enroll
            | Op::Rotate
            | Op::Remove
            | Op::AddCode
            | Op::RemoveCode
            | Op::NewCode
            | Op::Invite
            | Op::RemoveMember
            | Op::Recover
            | Op::Reencrypt
            | Op::RestoreAsOf
            | Op::DropKey,
        ) => Cell::Refuse(why_refuses(why)),
    }
}

/// The facts the state of a drive's key rests on (`observe`), in the table's words: each of them can be
/// unreadable or not asked, and `states()` holds every one of those twelve pairs, so that the exhaustive test
/// decides them and no arm of the judge can be wider than the table without a red test (X-DELTA-INTEGRITY3-4).
const FACTS: [&str; 6] = [
    "the drive key",
    "whether the drive is encrypted",
    "the new key of a key change",
    "the key a key change replaces",
    "the key kept for the drive's setup",
    "the key change's journal",
];

/// One state of every row, and each payload that a cell depends on.
fn states() -> Vec<KeyState> {
    let mut states = vec![
        KeyState::Fresh,
        KeyState::Planned,
        KeyState::Active,
        KeyState::Rotating(Start::Resume),
        KeyState::Rotating(Start::Adopt),
        KeyState::Rotating(Start::Again),
        KeyState::Torn(Why::OpensNothing),
        KeyState::Torn(Why::OldKeyGone),
        KeyState::Torn(Why::CannotResume),
        KeyState::Locked,
    ];
    for what in FACTS {
        states.push(KeyState::Unknown(Why::Unreadable {
            what,
            why: String::from("the keyring refused"),
        }));
        states.push(KeyState::Unknown(Why::NotAsked(what)));
    }
    states
}

/// Evidence in which everything a cell may rest on was asked and says yes: the keyring keeps keys, the
/// index opens, the drive holds none of its further codes, the recovery code was checked, registered,
/// and the months are kept elsewhere too.
fn everything_asked() -> Evidence<'static> {
    let mut e = all_good();
    e.keyring.keeps_keys = Read::Found(Keeps::Yes);
    e.bucket.index_opens = Read::Found(true);
    e.bucket.extra_codes = Read::Found(0);
    e.bucket.drop_file = Read::Found(DropFile::Opens);
    e
}

#[test]
fn the_table_is_the_specification_and_judge_answers_every_cell_by_it() {
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
    assert_eq!(cells.len(), 98, "seven states, fourteen operations");
}

/// X-DELTA-INTEGRITY3-4: a recovery is permitted on exactly four (why, fact) pairs of an unknown state - the
/// four the table states: the bucket's listing not asked, and the new key, the key it replaces and the
/// setup's key unreadable. The production arm took eight (a listing that FAILED and each of those three keyring
/// entries NOT ASKED too) while `states()` held none of the extra rows, so the exhaustive test could not tell.
/// (The facts are the production's own: a renamed or added fact shows here.)
#[test]
fn x_delta_integrity3_4_a_recovery_is_permitted_on_exactly_four_why_fact_pairs_of_an_unknown_state()
{
    assert_eq!(
        FACTS,
        [
            DRIVE_KEY,
            ENCRYPTED,
            NEW_KEY,
            PREVIOUS_KEY,
            PLANNED_KEY,
            JOURNAL
        ],
        "the facts the state rests on"
    );
    let mut permitted = Vec::new();
    for what in FACTS {
        for (kind, why) in [
            (
                "unreadable",
                Why::Unreadable {
                    what,
                    why: String::from("the keyring refused"),
                },
            ),
            ("not asked", Why::NotAsked(what)),
        ] {
            if matches!(
                judge(&KeyState::Unknown(why), Op::Recover, &everything_asked()),
                Verdict::Permit(_)
            ) {
                permitted.push((kind, what));
            }
        }
    }
    assert_eq!(
        permitted,
        [
            ("not asked", "whether the drive is encrypted"),
            ("unreadable", "the new key of a key change"),
            ("unreadable", "the key a key change replaces"),
            ("unreadable", "the key kept for the drive's setup"),
        ]
    );
}

/// X-DELTA-STRUCT-KEYSTATE-2: the table's permits rest on facts, and a fact nobody asked is no "yes".
/// With nothing asked, an operation that changes or erases key material is refused wherever the table
/// permits it, and says which fact was not asked. TWO cells rest on nothing and stay permitted; the list
/// is the exemptions' and MAY ONLY SHRINK (it grew from one to two at 079cc11fe with no reason written
/// down - X-DELTA-INTEGRITY3-2; here is each entry's):
/// - Active x RemoveCode takes a further code's wrap out of the bucket;
/// - Active x NewCode replaces the first code's wrap under the drive key the keyring holds.
/// Neither writes a keyring entry or reads a bucket fact: the state Active - observed from the keyring's
/// entries, which ARE asked - is all they rest on, and `device::wraps_permit` asks the keyring alone.
/// `Invite` and `RemoveMember` are `permit_plain` in Active too and are not in `KEY_CHANGING`: they replace
/// no recovery wrap and erase none of the user's ways back in when every device is lost, which is what the
/// list is about - an invite adds a one-time member wrap for the device the user is adding, a member's
/// removal takes ANOTHER device's wrap out - and both are refused in every state in which a key change is
/// under way (the exhaustive table). Listing them would be two more exemptions in a list that does not
/// grow. The refusals of the table stay as they are.
#[test]
fn x_delta_struct_keystate_2_with_nothing_asked_no_key_changing_operation_is_permitted_unless_its_cell_rests_on_nothing(
) {
    const KEY_CHANGING: [Op; 9] = [
        Op::Setup,
        Op::Rotate,
        Op::Remove,
        Op::AddCode,
        Op::RemoveCode,
        Op::NewCode,
        Op::Reencrypt,
        Op::Recover,
        // writes a key sealed under the drive key: its cell rests on the drop key file (X-DELTA-KEYS3-2)
        Op::DropKey,
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
    assert_eq!(
        rested_on_nothing,
        [("active", Op::RemoveCode), ("active", Op::NewCode)]
    );
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

    let unasked: [(&str, Record, Read<bool>); 3] = [
        (
            "whether the recovery code was checked",
            Record {
                checked: Read::Unasked,
                ..a_good_record()
            },
            Read::Found(true),
        ),
        (
            "whether the token server knows the recovery code",
            Record {
                registered: Read::Unasked,
                ..a_good_record()
            },
            Read::Found(true),
        ),
        (
            "whether the drive's own vault holds the paid months",
            a_good_record(),
            Read::Unasked,
        ),
    ];
    for state in removal_states() {
        for (what, record, months) in unasked.clone() {
            let e = with_a_record(Evidence::new(DRIVE, NOW), record, months);
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

/// X-DELTA-KEYS3-3: a planned key left beside the drive key - the drive key's own copy, which a setup leaves
/// when its keyring refused the planned key's deletion AFTER the wrap and the key existed and the setup
/// returned its code (X-DELTA-KEYS2-3) - finishes nothing: the drive is encrypted already. This cell used to
/// let the setup "finish" (`Way::Finish`), and a setup run again then replaced the recovery wrap of a sheet
/// that was shown and typed back with a new code's. A drive key with no planned key beside it says the same.
#[test]
fn x_delta_keys3_3_a_planned_copy_of_the_drive_key_beside_it_finishes_no_setup() {
    for planned in [Some(1), None] {
        let e = keyring(Some(1), None, planned);
        assert_eq!(observe(&e), KeyState::Active, "{planned:?}");
        match judge(&KeyState::Active, Op::Setup, &e) {
            Verdict::Refuse(reason) => {
                assert_eq!(reason.kind, Refusal::EncryptedAlready, "{planned:?}")
            }
            Verdict::Permit(permit) => {
                panic!(
                    "{planned:?}: the setup was let through ({:?})",
                    permit.way()
                )
            }
        }
    }
    // The way that finishes a setup is the Planned row's alone: a planned key and no drive key.
    match judge(&KeyState::Planned, Op::Setup, &everything_asked()) {
        Verdict::Permit(permit) => assert_eq!(permit.way(), Way::Finish),
        Verdict::Refuse(reason) => panic!("{reason}"),
    }
}

#[test]
fn the_operations_are_all_listed_once() {
    let names: BTreeSet<&str> = Op::ALL.iter().map(|op| op.name()).collect();
    assert_eq!(names.len(), 14);
    assert_eq!(Op::ALL.len(), 14);
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

/// A recovery record in a test: what it says is what the test sets.
#[derive(Clone)]
struct Record {
    checked: Read<u64>,
    registered: Read<bool>,
}

impl RecoveryRecord for Record {
    fn code_checked(&self) -> Read<u64> {
        self.checked.clone()
    }
    fn code_registered(&self) -> Read<bool> {
        self.registered.clone()
    }
}

/// The record of a code typed back right ten seconds before `NOW` and registered.
fn a_good_record() -> Record {
    Record {
        checked: Read::Found(NOW - 10),
        registered: Read::Found(true),
    }
}

/// A vault in a test: what it says is what the test sets.
struct Months(Read<bool>);

impl PaidMonths for Months {
    fn held_elsewhere(&self, _drive: &str) -> Read<bool> {
        self.0.clone()
    }
}

/// Evidence of a drive whose recovery code was checked, registered, and whose months are kept
/// elsewhere too.
fn all_good() -> Evidence<'static> {
    with_a_record(
        Evidence::new(DRIVE, NOW),
        a_good_record(),
        Read::Found(true),
    )
}

/// `e` with the recovery record `record` and the vault's answer `months`.
fn with_a_record(
    mut e: Evidence<'static>,
    record: Record,
    months: Read<bool>,
) -> Evidence<'static> {
    e.token = TokenFacts::of(&record);
    e.vault = VaultFacts::of(&Months(months), DRIVE);
    e
}

fn removal_verdict(state: &KeyState, e: &Evidence<'_>) -> Result<Vec<Stays>, Refusal> {
    match judge(state, Op::Remove, e) {
        Verdict::Permit(permit) => Ok(permit.stays().to_vec()),
        Verdict::Refuse(reason) => Err(reason.kind),
    }
}

/// The states a removal goes on its own conditions in: a drive whose keys are here or not yet made (Fresh,
/// Active) and a drive this computer holds no key of - Locked (what it keeps of such a drive goes with a code
/// typed back right) and the unknown state whose only unknown is that the bucket was not asked (a removal
/// reads no bucket). The last two are the cells 4305cacaa turned from Refuse to the removal's own
/// conditions; the loops below were Fresh and Active only, so a `locked()` that checked the code and skipped
/// the months stayed green (X-DELTA-INTEGRITY3-5).
fn removal_states() -> [KeyState; 4] {
    [
        KeyState::Fresh,
        KeyState::Active,
        KeyState::Locked,
        KeyState::Unknown(Why::NotAsked("whether the drive is encrypted")),
    ]
}

/// KEYS-8 and KEYS-32: Remove drive lets the keys go only with a checked code, a registered code and
/// the months kept elsewhere - in every state it goes on its own conditions in - and says what stays.
#[test]
fn x_delta_azdrive_keys_8_remove_is_permitted_only_with_a_checked_registered_code_and_months_kept_elsewhere(
) {
    for state in removal_states() {
        let stays = removal_verdict(&state, &all_good()).unwrap();
        assert_eq!(
            stays,
            vec![Stays::TheBucket, Stays::SyncedFiles, Stays::ContactKeys],
            "{state:?}"
        );

        let with = |record: Record, months: Read<bool>| {
            with_a_record(Evidence::new(DRIVE, NOW), record, months)
        };
        let never = with(
            Record {
                checked: Read::Absent,
                ..a_good_record()
            },
            Read::Found(true),
        );
        assert_eq!(
            removal_verdict(&state, &never),
            Err(Refusal::CodeNotChecked)
        );

        // a stamp from a clock that ran ahead is no check
        let ahead = with(
            Record {
                checked: Read::Found(NOW + 1),
                ..a_good_record()
            },
            Read::Found(true),
        );
        assert_eq!(
            removal_verdict(&state, &ahead),
            Err(Refusal::CodeNotChecked)
        );

        // checked at this very second is checked
        let now = with(
            Record {
                checked: Read::Found(NOW),
                ..a_good_record()
            },
            Read::Found(true),
        );
        assert!(removal_verdict(&state, &now).is_ok());

        for registered in [Read::Found(false), Read::Absent] {
            let unregistered = with(
                Record {
                    registered,
                    ..a_good_record()
                },
                Read::Found(true),
            );
            assert_eq!(
                removal_verdict(&state, &unregistered),
                Err(Refusal::CodeNotRegistered),
                "a registration that is pending, missing or never recorded is not 'registered'"
            );
        }

        let only_here = with(a_good_record(), Read::Found(false));
        assert_eq!(
            removal_verdict(&state, &only_here),
            Err(Refusal::MonthsOnlyHere)
        );

        // a drive with no vault of its own has no months to lose
        let no_vault = with(a_good_record(), Read::Absent);
        assert!(removal_verdict(&state, &no_vault).is_ok());

        // a failed read refuses, whichever it is
        fn failed<T>() -> Read<T> {
            Read::Failed(String::from("no"))
        }
        for (name, failing) in [
            (
                "code_checked",
                with(
                    Record {
                        checked: failed(),
                        ..a_good_record()
                    },
                    Read::Found(true),
                ),
            ),
            (
                "code_registered",
                with(
                    Record {
                        registered: failed(),
                        ..a_good_record()
                    },
                    Read::Found(true),
                ),
            ),
            ("months", with(a_good_record(), failed())),
        ] {
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
        for op in [Op::AddCode, Op::RemoveCode, Op::NewCode] {
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

/// X-DELTA-KEYS3-2: the cell of turning incoming mail on or off rests on what the bucket's drop key file is
/// to this device's key. One this key opens, or none, is let through; one sealed under ANOTHER drive key is
/// refused (it is the key the newer key sealed: this device's was replaced) with the sentence that tells the
/// user to ask for the new key, and as the class the apps tell that in (`KeyRefused`); a file that could not
/// be read is `Unreadable` and one nobody looked at is `NotAsked` - never "none".
#[test]
fn x_delta_keys3_2_the_drop_key_cell_rests_on_the_file_the_bucket_holds() {
    let ask = |file: Read<DropFile>| {
        let mut e = everything_asked();
        e.bucket.drop_file = file;
        judge(&KeyState::Active, Op::DropKey, &e)
    };
    for file in [Read::Found(DropFile::Opens), Read::Absent] {
        assert!(matches!(ask(file.clone()), Verdict::Permit(_)), "{file:?}");
    }
    match ask(Read::Found(DropFile::OtherKey)) {
        Verdict::Refuse(reason) => {
            assert_eq!(reason.kind, Refusal::DropKeyOfAnotherKey);
            assert!(
                reason.sentence.contains("ask for the new key"),
                "{}",
                reason.sentence
            );
            assert!(
                matches!(reason.error(DRIVE), DriveError::KeyRefused { drive, .. } if drive == DRIVE),
                "the class of a key that was replaced"
            );
        }
        Verdict::Permit(_) => panic!("a drop key file of another key was let through"),
    }
    assert!(matches!(
        ask(Read::Failed(String::from("no answer"))),
        Verdict::Refuse(reason) if matches!(reason.kind, Refusal::Unreadable { .. })
    ));
    assert!(matches!(
        ask(Read::Unasked),
        Verdict::Refuse(reason) if matches!(reason.kind, Refusal::NotAsked(_))
    ));
    // Not in any other state, whatever the file is: the file decides only an Active drive's cell.
    for state in states() {
        if state.row() == "active" {
            continue;
        }
        let mut e = everything_asked();
        e.bucket.drop_file = Read::Found(DropFile::Opens);
        assert!(
            matches!(judge(&state, Op::DropKey, &e), Verdict::Refuse(_)),
            "{state:?}"
        );
    }
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
    let mut own = keyring_of_a_key_change(1, 2, Some(1));
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
    // `rotation::resumable` asks nothing of the drive key; `start` returns before it reads the index. It
    // needs the key it replaces, which `rotate` reads from `drive-key-previous`.
    for drive in [None, Some(1)] {
        let mut e = keyring(drive, Some(2), None);
        e.keyring.previous_key = Read::Found(id(1));
        e.bucket.journal = Read::Found(Journal::Own);
        e.bucket.index_opens = Read::Found(false);
        assert_eq!(observe(&e), KeyState::Rotating(Start::Resume), "{drive:?}");
    }
}

/// X-DELTA-STRUCT-KEYSTATE-4: a journal of this device's own whose rotation cannot go on - `rotate` reads the
/// key it replaces from `drive-key-previous` and says "missing old" when it is gone - is no rotation to
/// resume: the device is torn, the recovery code is the way back in, and every other operation says why. A
/// journal left beside a new key that is already the drive key (the cleanup took the previous key first) is a
/// finished rotation.
#[test]
fn x_delta_struct_keystate_4_an_own_journal_without_the_key_it_replaces_is_a_rotation_that_cannot_go_on(
) {
    for drive in [None, Some(1)] {
        let mut e = keyring(drive, Some(2), None);
        e.bucket.journal = Read::Found(Journal::Own);
        assert_eq!(observe(&e), KeyState::Torn(Why::CannotResume), "{drive:?}");
    }
    let mut finished = keyring(Some(2), Some(2), None);
    finished.bucket.journal = Read::Found(Journal::Own);
    assert_eq!(
        observe(&finished),
        KeyState::Active,
        "the journal of a rotation that ended is left over"
    );
    // the sentence says what the user can do
    match judge(
        &KeyState::Torn(Why::CannotResume),
        Op::Rotate,
        &everything_asked(),
    ) {
        Verdict::Refuse(reason) => {
            assert_eq!(reason.kind, Refusal::CannotResume);
            assert!(
                reason.sentence.contains("recovery code"),
                "{}",
                reason.sentence
            );
        }
        Verdict::Permit(_) => panic!("a rotation that cannot go on was permitted"),
    }
    // ... and the way back in is open
    assert!(matches!(
        judge(
            &KeyState::Torn(Why::CannotResume),
            Op::Recover,
            &everything_asked()
        ),
        Verdict::Permit(_)
    ));
}

/// X-DELTA-STRUCT-KEYSTATE-4: a recovery is judged on the entry it writes. A failed read of the key change's,
/// the setup's or the previous key's entry does not refuse it - the user who holds the code must not be
/// locked out by an unrelated damaged entry - and a failed read of the drive key does.
#[test]
fn x_delta_struct_keystate_4_a_recovery_is_judged_on_the_entry_it_writes() {
    let mut e = keyring(None, None, None);
    e.keyring.planned_key = Read::Failed(String::from("damaged"));
    assert!(matches!(
        judge(&observe(&e), Op::Recover, &e),
        Verdict::Permit(_)
    ));
    let mut e = keyring(Some(1), None, None);
    e.keyring.next_key = Read::Failed(String::from("damaged"));
    assert!(matches!(
        judge(&observe(&e), Op::Recover, &e),
        Verdict::Permit(_)
    ));
    // the drive key's own entry cannot be read: refused, and so is every other operation
    let mut e = keyring(None, None, None);
    e.keyring.drive_key = Read::Failed(String::from("damaged"));
    match judge(&observe(&e), Op::Recover, &e) {
        Verdict::Refuse(reason) => assert!(matches!(reason.kind, Refusal::Unreadable { .. })),
        Verdict::Permit(_) => panic!("a recovery over a drive key that cannot be read"),
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
    /// How a case makes its fact fail.
    type Break = Box<dyn Fn(&mut Evidence<'_>)>;
    let cases: [(&str, Break); 4] = [
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

/// A planned key beside the drive key finishes no setup: a planned key of another id (a checkout's, for a
/// drive whose key this device holds) is no copy of the drive key - "finishing" would put it in the drive
/// key's place, and everything sealed under the drive key would open with nothing - and the drive key's own
/// copy is no setup that is not over (X-DELTA-KEYS3-3: it was let finish, X-DELTA-STRUCT-KEYSTATE-5 only
/// narrowed that to the same id).
#[test]
fn x_delta_struct_keystate_5_a_planned_key_of_another_id_beside_the_drive_key_does_not_finish_a_setup(
) {
    let other = keyring(Some(1), None, Some(2));
    assert_eq!(observe(&other), KeyState::Active);
    match judge(&KeyState::Active, Op::Setup, &other) {
        Verdict::Refuse(reason) => assert_eq!(reason.kind, Refusal::EncryptedAlready),
        Verdict::Permit(permit) => panic!("the setup was let finish ({:?})", permit.way()),
    }
    // the same id: the copy a setup left behind is no setup to finish either (X-DELTA-KEYS3-3)
    let same = keyring(Some(1), None, Some(1));
    assert!(matches!(
        judge(&KeyState::Active, Op::Setup, &same),
        Verdict::Refuse(reason) if reason.kind == Refusal::EncryptedAlready
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
